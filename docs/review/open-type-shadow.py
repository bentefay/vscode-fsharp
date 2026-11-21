#!/usr/bin/env python3
"""Real open-type property shadow, including equivalent keyword whitespace."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('--helper',type=Path,required=True);p.add_argument('--binary',type=Path);p.add_argument('--prepared',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);prep.HELPER=a.helper.resolve();n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py')
 if a.prepared:report=json.loads(a.prepared.read_text());base=Path(report['fixtureRoot']);consumer=base/'consumer';sourcefiles=report['consumerSources']
 else:
  base=Path(tempfile.mkdtemp(prefix='open-type-shadow-',dir=ROOT/'.tmp'));libsource='''namespace External
type TextLike() =
    member _.IsNullOrEmpty (_value:string) = false
type SystemLike() =
    member _.String = TextLike()
type Holder =
    static member System = SystemLike()
type PlainHolder =
    static member Unrelated = 42
''';lib=n.fixture(base,'library',{'Library.fs':libsource});(lib/'Test.fsproj').rename(lib/'OpenTypeLib.fsproj');report={'helperSha256':hashlib.sha256(prep.HELPER.read_bytes()).hexdigest(),'fixtureRoot':str(base),'librarySource':libsource,'builds':[prep.build(lib/'OpenTypeLib.fsproj')],'provenance':'Actual compiler disproves the initial namespace-shadow hypothesis: imported static System is usable as a standalone value, but System.String.IsNullOrEmpty retains framework namespace/type precedence. All qualified-call runtime results are true. Equivalent whitespace/comment forms test syntax classification; no performance claim.'};dll=lib/'bin/Debug/net10.0/OpenTypeLib.dll'
  sourcefiles={'Plain.fs':'module Plain\nlet result = System.String.IsNullOrEmpty("")\n','Unrelated.fs':'module Unrelated\nopen type External.PlainHolder\nlet result = System.String.IsNullOrEmpty("")\n','Single.fs':'module Single\nopen type External.Holder\nlet result = System.String.IsNullOrEmpty("")\n','Spaces.fs':'module Spaces\nopen  type External.Holder\nlet result = System.String.IsNullOrEmpty("")\n','Comment.fs':'module Comment\nopen (* gap *) type External.Holder\nlet result = System.String.IsNullOrEmpty("")\n','Main.fs':'module Main\n[<EntryPoint>]\nlet main _ =\n    printfn "%b:%b:%b:%b:%b" Plain.result Unrelated.result Single.result Spaces.result Comment.result\n    0\n'};report['consumerSources']=sourcefiles;consumer=n.fixture(base,'consumer',sourcefiles);project=consumer/'Test.fsproj';project.write_text(project.read_text().replace('</PropertyGroup>','<OutputType>Exe</OutputType></PropertyGroup>').replace('</ItemGroup>','<Reference Include="OpenTypeLib"><HintPath>'+str(dll)+'</HintPath></Reference></ItemGroup>'));report['builds'].append(prep.build(project));r=subprocess.run([str(prep.DOTNET),str(consumer/'bin/Debug/net10.0/Test.dll')],env=prep.ENV,text=True,capture_output=True,timeout=30);report['runtimeOracle']={'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr};assert r.returncode==0 and r.stdout.strip()=='true:true:true:true:true',report['runtimeOracle']
  model=prep.helper('workspace/evaluate',{'root':str(consumer)});mp=base/'model.json';mp.write_text(json.dumps(model));refs=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({p for x in model['projects'] for p in x['workspaceAssemblyReferences']});environment=prep.helper('metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});ep=base/'environment.json';ep.write_text(json.dumps(environment));report['modelPath']=str(mp);report['environmentPath']=str(ep);report['inventoryMarkers']={k:environment.get(k) for k in ['scopeNameInventory','termNameInventory']};report['systemTermRows']=environment.get('termNames',{}).get('System',[])
 if a.binary:
  report['binarySha256']=hashlib.sha256(a.binary.read_bytes()).hexdigest();report['cases']=[];old={k:os.environ.get(k) for k in prep.ENV};os.environ.update(prep.ENV)
  try:d=n.Daemon(a.binary.resolve(),consumer)
  finally:
   for k,v in old.items():
    if v is None:os.environ.pop(k,None)
    else:os.environ[k]=v
  d.call('workspace/open',{'root':str(consumer),'mode':'indexed','cacheDir':str(base/'cache'),'evaluatedModelPath':report['modelPath'],'builtinEnvironmentPath':report['environmentPath'],'metadataPath':str(prep.PREP/'catalog.json')})
  for file in ['Plain.fs','Unrelated.fs','Single.fs','Spaces.fs','Comment.fs']:
   line=1 if file=='Plain.fs' else 2;q={'path':str(consumer/file),'line':line,'character':sourcefiles[file].splitlines()[line].index('IsNullOrEmpty')};r=d.call('navigation/hover',q);v=r.get('result',{});exact=v.get('completeness')=='exact';expected_framework=True;owner=v.get('symbol',{}).get('assemblyName');correct=owner=='System.Runtime' if expected_framework else owner=='OpenTypeLib';status='pass' if exact and correct else ('wrong-exact' if exact else 'unavailable');report['cases'].append({'file':file,'expectedFramework':expected_framework,'status':status,'response':r})
  d.close();report['transcript']=d.transcript;report['summary']={s:sum(x['status']==s for x in report['cases']) for s in ['pass','wrong-exact','unavailable']}
 a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'output':str(a.output),'runtimeOracle':report['runtimeOracle'],'summary':report.get('summary'),'cases':report.get('cases',[])},indent=2))
if __name__=='__main__':main()
