#!/usr/bin/env python3
"""Actual private and signature-hidden potential-binding visibility controls."""
import argparse,hashlib,importlib.util,json,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('--helper',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);prep.HELPER=a.helper.resolve();n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py');base=Path(tempfile.mkdtemp(prefix='binding-accessibility-',dir=ROOT/'.tmp'));proxy='{ String = { IsNullOrEmpty = fun _ -> false } }'
 sources={
  'Common.fs':'namespace Proof\ntype StringProxy = { IsNullOrEmpty: string -> bool }\ntype SystemProxy = { String: StringProxy }\n',
  'Private.fs':'namespace Proof\nmodule PublicClosed =\n    let private System = '+proxy+'\n    let inside = System.String.IsNullOrEmpty("")\n',
  'Hidden.fsi':'module Hidden\nval inside: bool\n',
  'Hidden.fs':'module Hidden\nopen Proof\nlet System = '+proxy+'\nlet inside = System.String.IsNullOrEmpty("")\n',
  'UsePrivate.fs':'module UsePrivate\nopen Proof.PublicClosed\nlet value = System.String.IsNullOrEmpty("")\n',
  'UseHidden.fs':'module UseHidden\nopen Hidden\nlet value = System.String.IsNullOrEmpty("")\n',
  'Main.fs':'module Main\n[<EntryPoint>]\nlet main _ =\n    printfn "%b:%b:%b:%b" Proof.PublicClosed.inside UsePrivate.value Hidden.inside UseHidden.value\n    0\n'
 };consumer=n.fixture(base,'consumer',sources);project=consumer/'Test.fsproj';project.write_text(project.read_text().replace('</PropertyGroup>','<OutputType>Exe</OutputType></PropertyGroup>'));report={'helperSha256':hashlib.sha256(prep.HELPER.read_bytes()).hexdigest(),'fixtureRoot':str(base),'sources':sources,'queryCases':[('Private.fs',3,False),('UsePrivate.fs',2,True),('Hidden.fs',3,False),('UseHidden.fs',2,True)],'build':prep.build(project),'provenance':'Separate versioned scope controls: private uppercase value in a public module and a signature-hidden implementation value. Each still shadows inside its implementation scope, while a different source file opening the module uses the framework. Actual compiler/runtime oracle precedes native assertions.'};r=subprocess.run([str(prep.DOTNET),str(consumer/'bin/Debug/net10.0/Test.dll')],env=prep.ENV,text=True,capture_output=True,timeout=30);report['runtimeOracle']={'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr};assert r.returncode==0 and r.stdout.strip()=='false:true:false:true',report['runtimeOracle']
 model=prep.helper('workspace/evaluate',{'root':str(consumer)});mp=base/'model.json';mp.write_text(json.dumps(model));refs=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({p for x in model['projects'] for p in x['workspaceAssemblyReferences']});environment=prep.helper('metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});ep=base/'environment.json';ep.write_text(json.dumps(environment));report['modelPath']=str(mp);report['environmentPath']=str(ep);a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'output':str(a.output),'runtimeOracle':report['runtimeOracle'],'queryCases':report['queryCases']},indent=2))
if __name__=='__main__':main()
