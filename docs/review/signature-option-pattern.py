#!/usr/bin/env python3
"""Paired signature controls for exported, hidden and private Some patterns."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--helper',type=Path,required=True);ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);prep.HELPER=a.helper.resolve();native=prep.load('native',ROOT/'docs/review/native-smoke-replay.py')
 base=Path(tempfile.mkdtemp(prefix='signature-option-pattern-',dir=ROOT/'.tmp'));rows=[]
 for case in ['exported','hidden','private']:
  ns='Case'+case.capitalize();private='private ' if case=='private' else ''
  signature='module '+ns+'.Patterns\nopen '+ns+'\n'+('' if case=='hidden' else 'val '+private+'(|Some|_|) : Customer option -> Supplier option\n')+'val marker : int\n'
  impl='module '+ns+'.Patterns\nopen '+ns+'\nlet '+private+'(|Some|_|) (input:Customer option) : Supplier option =\n    match input with\n    | Microsoft.FSharp.Core.Some _ -> Microsoft.FSharp.Core.Some ({Name="supplier"}:Supplier)\n    | Microsoft.FSharp.Core.None -> Microsoft.FSharp.Core.None\nlet marker=0\n'
  use='module '+ns+'.Use\nopen '+ns+'\nopen '+ns+'.Patterns\nlet read (input:Customer option) =\n    match input with\n    | Some value -> value.Name\n    | None -> "none"\nlet result=read (Microsoft.FSharp.Core.Some ({Name="customer"}:Customer))\n'
  files={'Types.fs':'namespace '+ns+'\ntype Customer={Name:string}\ntype Supplier={Name:string}\n','Patterns.fsi':signature,'Patterns.fs':impl,'Use.fs':use,'Main.fs':'module '+ns+'.Main\n[<EntryPoint>]\nlet main _ =\n    printfn "%s" '+ns+'.Use.result\n    0\n'}
  root=native.fixture(base,case,files);project=root/'Test.fsproj';project.write_text(project.read_text().replace('</PropertyGroup>','<OutputType>Exe</OutputType></PropertyGroup>'))
  build=subprocess.run([str(prep.DOTNET),'build',str(project),'--nologo','-v:q','-m:1','-nodeReuse:false','-p:NuGetAudit=false'],env=prep.ENV,text=True,capture_output=True,timeout=90)
  row={'case':case,'root':str(root),'sources':files,'build':{'exitCode':build.returncode,'stdout':build.stdout,'stderr':build.stderr}};rows.append(row)
  if build.returncode:
   project.rename(project.with_suffix('.invalid'));print(case,'INVALID',build.stdout[-600:],flush=True);continue
  run=subprocess.run([str(prep.DOTNET),str(root/'bin/Debug/net10.0/Test.dll')],env=prep.ENV,text=True,capture_output=True,timeout=30);row['runtime']={'exitCode':run.returncode,'stdout':run.stdout,'stderr':run.stderr};expected='supplier' if case=='exported' else 'customer';assert run.returncode==0 and run.stdout.strip()==expected,row
  target=2 if case=='exported' else 1;row['query']={'path':str(root/'Use.fs'),'line':5,'character':use.splitlines()[5].index('Name')};row['expectedLocations']=[native.location(root/'Types.fs',target,15,4)];row['oracle']=prep.helper('navigation/definition',{'root':str(root),**row['query']});print(case,run.stdout.strip(),row['oracle'],flush=True)
 model=prep.helper('workspace/evaluate',{'root':str(base)});mp=base/'model.json';mp.write_text(json.dumps(model));refs=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({r for p in model['projects'] for r in p['workspaceAssemblyReferences']});environment=prep.helper('metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});ep=base/'environment.json';ep.write_text(json.dumps(environment))
 os.environ.update(prep.ENV);d=native.Daemon(a.binary.resolve(),base);d.call('workspace/open',{'root':str(base),'mode':'indexed','cacheDir':str(base/'cache'),'evaluatedModelPath':str(mp),'builtinEnvironmentPath':str(ep)})
 for row in rows:
  if row['build']['exitCode']:continue
  response=d.call('navigation/definition',{**row['query'],'projectPath':str(Path(row['root'])/'Test.fsproj')});value=response.get('result',{});exact=value.get('completeness')=='exact';same=native.normalized(value.get('locations',[]))==native.normalized(row['expectedLocations']);row['response']=response;row['status']='pass' if exact and same else ('wrong-exact' if exact else 'unavailable');print(row['case'],row['status'],flush=True)
 d.close();out={'provenance':'Actual SDK paired-signature build/runtime and evaluated FCS target; genuine schema2 model/full inventory; native indexed query with no hidden compiler. Invalid cases, if any, are not positive expectations.','fixtureRoot':str(base),'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'helperSha256':hashlib.sha256(a.helper.read_bytes()).hexdigest(),'modelPath':str(mp),'environmentPath':str(ep),'cases':rows,'transcript':d.transcript};a.output.write_text(json.dumps(out,indent=2)+'\n')
if __name__=='__main__':main()
