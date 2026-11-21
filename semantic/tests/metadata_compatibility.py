#!/usr/bin/env python3
"""Old native readers must reject genuine schema2 artifacts for their envelope.
The positive legacy control and each component are tested separately. A missing
XML/binary dependency is not accepted as the reason for compatibility rejection.
"""
import argparse,hashlib,json,tempfile,shutil
from pathlib import Path
from resolution_states import Native,save

def inspect_catalog(path,seen=None):
    seen=set() if seen is None else seen
    path=path.resolve();assert path not in seen;seen.add(path)
    value=json.loads(path.read_text());assert value['schemaVersion']==2
    for entry in value.get('inputHashes',[]):
        expected=entry['sha256'];assert len(expected)==64 and all(c in '0123456789abcdefABCDEF' for c in expected),'Use a selected-first XML fixture without missing/directory sentinels'
        assert Path(entry['path']).is_file(),entry
        assert hashlib.sha256(Path(entry['path']).read_bytes()).hexdigest()==expected,entry
    for shard in value.get('catalogs',[]):inspect_catalog(path.parent/shard,seen)

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--prepared',required=True,type=Path);ap.add_argument('--new-environment',required=True,type=Path);ap.add_argument('--new-catalog',required=True,type=Path);ap.add_argument('--old-binary',required=True,type=Path,action='append');ap.add_argument('--output',required=True,type=Path);args=ap.parse_args()
    manifest=json.loads((args.prepared/'manifest.json').read_text());program=manifest['programs']['positive'];environment=json.loads(args.new_environment.read_text());assert environment['schemaVersion']==2
    inspect_catalog(args.new_catalog)
    for item in environment['inputHashes']:assert len(item['sha256'])==64 and Path(item['path']).is_file()
    root=Path(tempfile.mkdtemp(prefix='metadata-compat-',dir=args.prepared));reports=[]
    try:
        for n,binary in enumerate(args.old_binary):
            daemon=Native(binary.resolve());rows=[]
            try:
                for label,env,catalog in [('legacy_positive',program['environment'],manifest['catalog']),('new_environment',str(args.new_environment.resolve()),manifest['catalog']),('new_catalog',program['environment'],str(args.new_catalog.resolve()))]:
                    daemon.call('workspace/open',{'root':program['root'],'mode':'indexed','cacheDir':str(root/str(n)),'evaluatedModelPath':program['model'],'builtinEnvironmentPath':env,'metadataPath':catalog})
                    response=daemon.call('navigation/hover',program['params']);stats=daemon.call('workspace/stats',{})['result'];assert stats['helperStarts']==stats['compilerLaunches']==0
                    if label=='legacy_positive':passed=response.get('result',{}).get('completeness')=='exact' and bool(response['result'].get('hover'))
                    else:
                        message=json.dumps(response).lower();passed='error' in response and ('supported certificate' in message if label=='new_environment' else 'schemaversion 1' in message)
                    rows.append({'case':label,'passed':bool(passed),'response':response,'stats':stats});print(('PASS' if passed else 'FAIL')+' '+binary.parent.name+' '+label,flush=True)
            finally:daemon.close()
            reports.append({'binary':str(binary.resolve()),'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rows':rows})
        result={'kind':'genuine-new-envelope-old-reader-rejection','newEnvironment':str(args.new_environment.resolve()),'newCatalog':str(args.new_catalog.resolve()),'selectedFirstXmlInputsAllPresent':True,'readers':reports,'passed':all(row['passed'] for report in reports for row in report['rows'])};args.output.parent.mkdir(parents=True,exist_ok=True);save(args.output,result);assert result['passed'],result
    finally:shutil.rmtree(root)

if __name__=='__main__':main()
