#!/usr/bin/env python3
"""Locate parser/index memory outliers with real, isolated corpus source files."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile

from run import Session,utf16


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine',required=True,type=Path)
    parser.add_argument('--engine-sha')
    parser.add_argument('--corpus',required=True,type=Path)
    parser.add_argument('--count',type=int,default=20)
    parser.add_argument('--modes',default='indexed,lexical')
    parser.add_argument('--output',type=Path,default=Path('evaluation/results/file-profile.json'))
    args=parser.parse_args()
    if (Path(__file__).resolve().parent/'.runtime'/'pause-measurements').exists():
        parser.error('Measurement coordinator paused new runs until review completes')
    corpus=args.corpus.resolve()
    names=subprocess.check_output(['git','-C',str(corpus),'ls-files','-z']).split(b'\0')
    files=sorted([( (corpus/os.fsdecode(name)).stat().st_size,os.fsdecode(name)) for name in names
                  if name.endswith((b'.fs',b'.fsi',b'.fsx'))],reverse=True)[:args.count]
    temporary_root=Path(__file__).resolve().parent/'.runtime'/'runs';temporary_root.mkdir(parents=True,exist_ok=True)
    work=Path(tempfile.mkdtemp(prefix='fsharp-file-profile-',dir=temporary_root))
    binary=work/'fsharp-navigator';shutil.copy2(args.engine,binary)
    digest=hashlib.sha256(binary.read_bytes()).hexdigest()
    if args.engine_sha and digest!=args.engine_sha:
        raise RuntimeError('Engine differs from the requested frozen SHA')
    result={'engine_sha256':digest,'corpus':str(corpus),
            'sha':subprocess.check_output(['git','-C',str(corpus),'rev-parse','HEAD'],text=True).strip(),
            'notes':['Each process indexes one actual file hardlinked into an isolated workspace, with a fresh SQLite cache.',
                     'This profiles parser/index construction, not semantic query correctness or whole-project navigation.',
                     'RSS sampled10ms; includes all owned descendants; process-cold with warm OS caches.'], 'rows':[]}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    for number,(size,name) in enumerate(files):
        workspace=work/f'file-{number:03d}';workspace.mkdir()
        try:
            os.link(corpus/name,workspace/Path(name).name)
        except OSError:
            shutil.copy2(corpus/name,workspace/Path(name).name)
        for mode in args.modes.split(','):
            session=Session([str(binary),'daemon'],timeout=120,memory_limit_mib=4096)
            try:
                response,measure=session.call('workspace/open',{'root':str(workspace),'mode':mode,'cacheDir':str(work/f'cache-{number}-{mode}')})
                row={'file':name,'bytes':size,'mode':mode,**measure,'response':response,
                     'rss_timeline_ms_kib':[[round((stamp-session.started)*1000,3),rss] for stamp,rss,_ in session.samples]}
                text=(workspace/Path(name).name).read_text(errors='replace')
                match=re.search(r'\b(?:let|val)\s+(?:private\s+|internal\s+|mutable\s+|rec\s+|inline\s+)*([A-Za-z_][A-Za-z0-9_]*)',text)
                if match:
                    prefix=text[:match.start(1)]
                    query_response,query_measure=session.call('navigation/definition',{
                        'path':str(workspace/Path(name).name),'line':prefix.count('\n'),
                        'character':utf16(prefix.rsplit('\n',1)[-1])+1})
                    row.update(query_measurement=query_measure,query_response=query_response,
                               combined_peak_tree_rss_kib=max(measure['peak_tree_rss_kib'],query_measure['peak_tree_rss_kib']))
                    row['rss_timeline_ms_kib']=[[round((stamp-session.started)*1000,3),rss] for stamp,rss,_ in session.samples]
                result['rows'].append(row)
                args.output.write_text(json.dumps(result,indent=2)+'\n')
                print(json.dumps({key:value for key,value in row.items() if key not in {'response','rss_timeline_ms_kib','query_response'}}),flush=True)
            finally:
                session.close()
    shutil.rmtree(work)


if __name__=='__main__':
    main()
