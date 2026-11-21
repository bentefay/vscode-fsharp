#!/usr/bin/env python3
"""Genuine prepared Option active-pattern scope controls, followed by native replay."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--helper',type=Path,required=True);ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);prep.HELPER=a.helper.resolve();native=prep.load('native',ROOT/'docs/review/native-smoke-replay.py')
 base=Path(tempfile.mkdtemp(prefix='closed-option-pattern-',dir=ROOT/'.tmp'))
 types='type Customer = { Name:string }\ntype Supplier = { Name:string }\n'
 pattern='''module Patterns =
    let (|Some|_|) (x:Customer option) : Supplier option =
        match x with
        | Microsoft.FSharp.Core.Some _ -> Microsoft.FSharp.Core.Some ({Name="supplier"}:Supplier)
        | Microsoft.FSharp.Core.None -> Microsoft.FSharp.Core.None
'''
 reader='''let read (x:Customer option) =
    match x with
    | Some customer -> customer.Name
    | None -> "none"
let result = read (Microsoft.FSharp.Core.Some ({Name="customer"}:Customer))
'''
 files={'Plain.fs':'module Plain\n'+types+reader,'Opened.fs':'module Opened\n'+types+pattern+'open Patterns\n'+reader,'Unopened.fs':'module Unopened\n'+types+pattern+reader,'Main.fs':'module Main\n[<EntryPoint>]\nlet main _ =\n    printfn "%s:%s:%s" Plain.result Opened.result Unopened.result\n    0\n'}
 root=native.fixture(base,'fixtures',files);project=root/'Test.fsproj';project.write_text(project.read_text().replace('</PropertyGroup>','<OutputType>Exe</OutputType></PropertyGroup>'))
 build=prep.build(project);runtime=subprocess.run([str(prep.DOTNET),str(root/'bin/Debug/net10.0/Test.dll')],env=prep.ENV,text=True,capture_output=True,timeout=30)
 assert runtime.returncode==0 and runtime.stdout.strip()=='customer:supplier:customer',(runtime.stdout,runtime.stderr)
 model=prep.helper('workspace/evaluate',{'root':str(root)});mp=base/'model.json';mp.write_text(json.dumps(model))
 refs=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({r for p in model['projects'] for r in p['workspaceAssemblyReferences']})
 env=prep.helper('metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});ep=base/'environment.json';ep.write_text(json.dumps(env))
 old={k:os.environ.get(k) for k in prep.ENV};os.environ.update(prep.ENV)
 try:d=native.Daemon(a.binary.resolve(),root)
 finally:
  for k,v in old.items():
   if v is None:os.environ.pop(k,None)
   else:os.environ[k]=v
 d.call('workspace/open',{'root':str(root),'mode':'indexed','cacheDir':str(base/'cache'),'evaluatedModelPath':str(mp),'builtinEnvironmentPath':str(ep)})
 cases=[]
 for file in ['Plain.fs','Opened.fs','Unopened.fs']:
  source=files[file];line=next(i for i,s in enumerate(source.splitlines()) if 'customer.Name' in s);col=source.splitlines()[line].index('Name');target=2 if file=='Opened.fs' else 1;tcol=source.splitlines()[target].index('Name')
  response=d.call('navigation/definition',{'path':str(root/file),'line':line,'character':col});value=response.get('result',{});exact=value.get('completeness')=='exact';expected=[native.location(root/file,target,tcol,4)];same=native.normalized(value.get('locations',[]))==native.normalized(expected)
  cases.append({'file':file,'query':{'line':line,'character':col},'expectedLocations':expected,'status':'pass' if exact and same else ('wrong-exact' if exact else 'unavailable'),'response':response})
 d.close()
 # Actual evaluated FCS definitions independently identify the receiver field.
 oracles=[]
 for c in cases:oracles.append({'file':c['file'],'response':prep.helper('navigation/definition',{'root':str(root),'path':str(root/c['file']),**c['query']})})
 out={'provenance':'Fresh successful SDK build/runtime and evaluated FCS definitions, genuine schema2 selected-reference environment; native queries invoke no helper.','fixtureRoot':str(base),'sources':files,'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'helperSha256':hashlib.sha256(a.helper.read_bytes()).hexdigest(),'modelPath':str(mp),'environmentPath':str(ep),'build':build,'runtime':{'exitCode':runtime.returncode,'stdout':runtime.stdout,'stderr':runtime.stderr},'oracleDefinitions':oracles,'cases':cases,'transcript':d.transcript,'summary':{s:sum(c['status']==s for c in cases) for s in ['pass','wrong-exact','unavailable']}}
 a.output.write_text(json.dumps(out,indent=2)+'\n');print(out['summary']);print(json.dumps(cases,indent=2))
if __name__=='__main__':main()
