#!/usr/bin/env python3
"""Actual valid-to-valid external type addition invalidates a negative name proof."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();manifest=json.loads((ROOT/'docs/review/ordered-nominal-prepared.json').read_text());os.environ.update(manifest['environmentOverrides'])
spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);prep.HELPER=ROOT/'evaluation/.runtime/helper-09f01156157a/FSharpNavigator.Semantic.dll';n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py')
root=Path(manifest['root']);libproject=Path(manifest['libraryProject']);libfile=libproject.parent/'Library.fs';dll=Path(manifest['libraryPath']);base=Path(tempfile.mkdtemp(prefix='nominal-freshness-',dir=ROOT/'.tmp'));before_source=libfile.read_bytes();backup={x:x.read_bytes() for x in dll.parent.rglob('*') if x.is_file()};before_dll=hashlib.sha256(dll.read_bytes()).hexdigest();old_env=Path(manifest['environmentPath']);mp=Path(manifest['modelPath']);model=json.loads(mp.read_text());sessions=[];cases=[];builds=[];active=[]
queries={q['name']:q for q in manifest['queries']};names=['Absent','SourceAutoOpen','Caller-generic','Caller']
def start(ep,cache):
 d=n.Daemon(a.binary.resolve(),root);active.append(d);d.call('workspace/open',dict(root=str(root),mode='indexed',cacheDir=str(cache),evaluatedModelPath=str(mp),builtinEnvironmentPath=str(ep)));return d
def query(d,name,phase):
 q=queries[name];response=d.call('navigation/definition',{k:v for k,v in q.items() if k in ('path','line','character')});cases.append(dict(phase=phase,name=name,response=response));print(phase,name,response.get('result',{}).get('completeness'),response.get('error'));return response

def close(d):
 d.close();sessions.append(d.transcript);active.remove(d)
report=dict(binarySha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),manifest='ordered-nominal-prepared.json',fixtureRoot=str(root),baselineLibrarySha256=before_dll,cases=cases,sessions=sessions,builds=builds,provenance='Actual external DLL rebuilt to add a previously absent opened type. Consumer source/project bytes stay unchanged and both builds succeed. Old and fresh artifact tests use genuine producer exports, not edited certificate claims. All mutations owned by this isolated reviewer fixture; original binaries/source restored afterward.')
try:
 d=start(old_env,base/'shared-cache')
 for name in names:query(d,name,'initial')
 query(d,'Absent','warm')
 libfile.write_text(manifest['librarySource'].replace('namespace Review.Empty\n','namespace Review.Empty\ntype Scope = { Name: string }\n'))
 builds.append(dict(phase='changed-library',**prep.build(libproject)));builds.append(dict(phase='unchanged-consumer',**prep.build(root/'Test.fsproj')));report['changedLibrarySha256']=hashlib.sha256(dll.read_bytes()).hexdigest();assert report['changedLibrarySha256']!=before_dll
 report['treeAfterLibraryChange']=d.call('workspace/tree')
 for name in names:query(d,name,'same-session-old-inventory')
 close(d)
 d=start(old_env,base/'shared-cache')
 for name in names:query(d,name,'restarted-old-inventory')
 close(d)
 references=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({r for p in model['projects'] for r in p['workspaceAssemblyReferences']})
 environment=prep.helper('metadata/environment',dict(referencePaths=references,workspaceAssemblyPaths=owned));fresh=base/'fresh-environment.json';fresh.write_text(json.dumps(environment));report['freshEnvironmentPath']=str(fresh);report['freshEnvironmentSha256']=hashlib.sha256(fresh.read_bytes()).hexdigest()
 oracle_manifest={**manifest,'queries':[queries[name] for name in names]};op=base/'oracle-input.json';op.write_text(json.dumps(oracle_manifest));proc=subprocess.run([str(prep.DOTNET),'fsi','--exec',str(ROOT/'docs/review/nominal-project-oracle.fsx'),str(op)],env=prep.ENV,cwd=root,text=True,capture_output=True,timeout=120);oracles=[json.loads(x) for x in proc.stdout.splitlines() if x.startswith('{')];assert len(oracles)==4 and all(x['valid'] for x in oracles),(proc.stderr,oracles);report['freshOracle']=oracles
 d=start(fresh,base/'fresh-cache')
 for name in names:query(d,name,'fresh-inventory')
 close(d)
 incomplete=prep.helper('metadata/environment',dict(referencePaths=[r for r in references if Path(r)!=dll],workspaceAssemblyPaths=owned));ip=base/'genuine-incomplete-reference-universe.json';ip.write_text(json.dumps(incomplete));report['incompleteEnvironmentPath']=str(ip)
 d=start(ip,base/'incomplete-cache');query(d,'Absent','genuine-incomplete-reference-universe');close(d)
finally:
 for d in list(active):
  try:close(d)
  except Exception:pass
 libfile.write_bytes(before_source)
 for path,data in backup.items():path.write_bytes(data)
 report['postRunSourceBytesMatchPreparedManifest']={f:(root/f).read_text()==text for f,text in manifest['sources'].items()}
 report['restoredLibrarySha256']=hashlib.sha256(dll.read_bytes()).hexdigest();report['restored']=report['postRunSourceBytesMatchPreparedManifest']={f:(root/f).read_text()==text for f,text in manifest['sources'].items()}
 report['restoredLibrarySha256']==before_dll
 a.output.write_text(json.dumps(report,indent=2)+'\n')
print('restored',report['restored'])
