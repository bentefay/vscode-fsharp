#!/usr/bin/env python3
"""Cross-file controls for exported-name absence, with real FCS and preparation."""
import argparse, hashlib, importlib.util, json, os, subprocess, tempfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
CASES = {
 'operator_other_file': ('let (>>=) x f = f x\nlet marker=1\nlet label=nameof marker\n', False),
 'task_local_other_file': ('let work () = task {\n    let! nameof = System.Threading.Tasks.Task.FromResult (fun (x:int) -> "local:" + string x)\n    return nameof 42 }\nlet marker=1\nlet label=nameof marker\n', False),
 'operator_and_task_other_file': ('let (>>=) x f = f x\nlet work () = task {\n    let! value = System.Threading.Tasks.Task.FromResult 42\n    return value }\nlet marker=1\nlet label=nameof marker\n', False),
 'exported_name_other_file': ('let work () = 1\nlet nameof (x:int) = "exported:" + string x\n', True),
 'recursive_name_other_file': ('let rec work x : string = nameof x\nand nameof (x:int) : string = "recursive:" + string x\n', True),
 'pattern_name_other_file': ('let (nameof, other) = ((fun (x:int) -> "pattern:" + string x),0)\n', True),
}

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--binary',type=Path);ap.add_argument('--helper',type=Path);ap.add_argument('--prepared',type=Path);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);native=prep.load('native',ROOT/'docs/review/native-smoke-replay.py')
 if a.prepared:
  report=json.loads(a.prepared.read_text());base=Path(report['fixtureRoot']);root=base/'project'
 else:
  assert a.helper;prep.HELPER=a.helper.resolve();base=Path(tempfile.mkdtemp(prefix='name-boundary-split-',dir=ROOT/'.tmp'));cases=[]
  for name,(body,negative) in CASES.items():
   folder=base/name;folder.mkdir();other='module Other\n'+body;(folder/'Other.fs').write_text(other)
   suffix=('open Other\n' if negative else '')+'type Sha = string\n'+('let Sha=42\n' if negative else '')+'let observed = nameof Sha\n'
   script='#load "Other.fs"\n'+suffix;path=folder/'Main.fsx';path.write_text(script);lines=script.splitlines();alias=lines.index('type Sha = string');argument=lines.index('let observed = nameof Sha')
   cases.append(dict(case=name,path=str(path),sources={'Other.fs':other,'Main.fs':'module Main\n'+suffix},negative=negative,queries=[dict(label='alias',token='Sha',line=alias,character=5),dict(label='argument',token='Sha',line=argument,character=22)]))
  manifest=base/'oracle-input.json';manifest.write_text(json.dumps(cases));p=subprocess.run([str(prep.DOTNET),'fsi','--exec',str(ROOT/'docs/review/alias-expression-oracle.fsx'),str(manifest)],env=prep.ENV,text=True,capture_output=True,timeout=120);assert p.returncode==0,p.stderr;oracles=[json.loads(line) for line in p.stdout.splitlines() if line.startswith('{')];assert len(oracles)==len(cases)
  for case,oracle in zip(cases,oracles):
   assert oracle['valid'],oracle;case['oracle']=oracle;assert len(oracle['results'][0]['references'])==(1 if case['negative'] else 2),(case,oracle)
  root=native.fixture(base,'project',cases[0]['sources']);build=prep.build(root/'Test.fsproj');model=prep.helper('workspace/evaluate',{'root':str(root)});mp=base/'model.json';mp.write_text(json.dumps(model));refs=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({p for project in model['projects'] for p in project['workspaceAssemblyReferences']});env=prep.helper('metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});ep=base/'environment.json';ep.write_text(json.dumps(env));report=dict(provenance='All six split-file sources validated by actual FCS with #load; native equivalents substitute module Main for #load at same line offset. First variant builds, genuine f215 model/environment, subsequent variants versioned overlays. No performance claim.',fixtureRoot=str(base),modelPath=str(mp),environmentPath=str(ep),helperSha256=hashlib.sha256(a.helper.read_bytes()).hexdigest(),build=build,cases=cases)
 if a.binary:
  os.environ.update(prep.ENV);d=native.Daemon(a.binary.resolve(),root);d.call('workspace/open',dict(root=str(root),mode='indexed',cacheDir=str(base/('cache-'+hashlib.sha256(a.binary.read_bytes()).hexdigest()[:12])),evaluatedModelPath=report['modelPath'],builtinEnvironmentPath=report['environmentPath']));rows=[]
  for version,case in enumerate(report['cases'],1):
   for file,source in case['sources'].items():d.call('document/open' if version==1 else 'document/change',dict(path=str(root/file),text=source,version=version))
   for q in case['queries']:
    oracle=next(r for r in case['oracle']['results'] if r['label']==q['label'])
    for op in ['references','definition']:
     response=d.call('navigation/'+op,dict(path=str(root/'Main.fs'),line=q['line'],character=q['character'],version=version,includeDeclaration=True));value=response.get('result',{});ranges=[r['range'] for r in oracle['references']] if op=='references' else [oracle['symbol']['declaration']];expected=[native.location(root/'Main.fs',r['startLine'],r['startCharacter'],r['endCharacter']-r['startCharacter']) for r in ranges];exact=value.get('completeness')=='exact';same=native.normalized(value.get('locations',[]))==native.normalized(expected) and value.get('version')==version and value.get('documentVersion')==version;rows.append(dict(case=case['case'],label=q['label'],operation=op,requestedVersion=version,status='pass' if exact and same else ('wrong-exact' if exact else 'unavailable'),expectedLocations=expected,response=response))
  stats=d.call('workspace/stats')['result'];assert stats['compilerLaunches']==stats['helperStarts']==0;d.close();report.update(binarySha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),results=rows,transcript=d.transcript,stats=stats,summary={s:sum(r['status']==s for r in rows) for s in ['pass','wrong-exact','unavailable']});print(report['summary'])
  for row in rows:
   if row['status']=='wrong-exact':print(row)
 a.output.write_text(json.dumps(report,indent=2)+'\n');print(a.output)
if __name__=='__main__':main()
