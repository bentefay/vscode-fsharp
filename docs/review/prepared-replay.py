#!/usr/bin/env python3
"""Fresh real project certificates, then immutable Rust-only correctness probes."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
DOTNET=ROOT/'.tools/dotnet/dotnet'
PREP=ROOT/'evaluation/.runtime/recovery-prepared-expanded-4c9d'
HELPER=Path(os.environ.get('FSHARP_REVIEW_HELPER',PREP/'helper-runtime/FSharpNavigator.Semantic.dll'))
ENV=dict(os.environ,DOTNET_ROOT=str(DOTNET.parent),DOTNET_NOLOGO='1',DOTNET_CLI_TELEMETRY_OPTOUT='1',MSBUILDDISABLENODEREUSE='1',DOTNET_CLI_USE_MSBUILD_SERVER='0',TMPDIR=str(ROOT/'.tmp'))
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
def helper(method,params):
 p=subprocess.run([str(DOTNET),str(HELPER),'--once'],input=json.dumps({'id':1,'method':method,'params':params})+'\n',text=True,capture_output=True,env=ENV,timeout=90)
 assert p.returncode==0,p.stderr
 result=json.loads(p.stdout);assert 'error' not in result,result
 return result['result']
def build(project):
 p=subprocess.run([str(DOTNET),'build',str(project),'--nologo','-v:q','-m:1','-nodeReuse:false','-p:NuGetAudit=false'],text=True,capture_output=True,env=ENV,timeout=90)
 assert p.returncode==0,p.stdout+p.stderr
 return {'exitCode':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
def main():
 parser=argparse.ArgumentParser();parser.add_argument('binary',type=Path);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
 native=load('native_replay',ROOT/'docs/review/native-smoke-replay.py');attrs=load('attr_replay',ROOT/'docs/review/attribute-identity-oracle.py')
 base=Path(tempfile.mkdtemp(prefix='review-prepared-',dir=ROOT/'.tmp'));files={};attr_queries=[]
 for n,(source,expected,target,token) in attrs.CASES.items():
  filename=n+'.fs';text='module Case_'+n+'\n'+source;files[filename]=text
  line=next(i for i,s in enumerate(text.splitlines()) if s.startswith('let result =' if n.startswith('autoopen') else 'let get '))
  attr_queries.append((n,filename,line,text.splitlines()[line].rindex(token),target+1,text.splitlines()[target+1].index(token),len(token)))
 files['Types.fs']='namespace Shared\ntype DateOnly = System.DateTime\n'
 files['Use.fs']='namespace Shared\nmodule Use =\n    open System\n    let day (x: DateOnly) = x.Day\n    let proof = day (System.DateOnly(2026,11,20))\n'
 files['Mixed.fs']='module Mixed\ntype Person = { Name: string }\nlet tupleName (x: System.Tuple<Person>) = x.Item1.Name\nlet optionName (x: Person option) = match x with Some p -> p.Name | None -> ""\n'
 root=native.fixture(base,'proofs',files);report={'binarySha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'helperSha256':hashlib.sha256(HELPER.read_bytes()).hexdigest(),'fixtureRoot':str(base),'sources':files,'preparation':[],'cases':[],'sessions':[],'provenance':'Fresh real SDK builds/evaluation followed by immutable Rust-only query replay. No synthetic model, no implicit helper on queries, no performance claims.'}
 report['preparation'].append({'phase':'proof_project_build',**build(root/'Test.fsproj')});model=helper('workspace/evaluate',{'root':str(root)});model_path=base/'model.json';model_path.write_text(json.dumps(model))
 report['preparation'].append({'phase':'proof_project_evaluate','modelPath':str(model_path),'modelSha256':hashlib.sha256(model_path.read_bytes()).hexdigest(),'membershipReusable':[p.get('membershipReusable') for p in model['projects']]})
 owned=sorted({path for p in model['projects'] for path in p['workspaceAssemblyReferences']})
 references=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']})
 environment=helper('metadata/environment',{'referencePaths':references,'workspaceAssemblyPaths':owned})
 environment_path=base/'builtin-environment.json';environment_path.write_text(json.dumps(environment))
 report['preparation'].append({'phase':'proof_environment_export','path':str(environment_path),'sha256':hashlib.sha256(environment_path.read_bytes()).hexdigest()})
 def open_daemon(folder,modelpath):
  d=native.Daemon(args.binary.resolve(),folder)
  d.call('workspace/open',{'root':str(folder),'mode':'indexed','cacheDir':str(folder/'prepared-cache'),'evaluatedModelPath':str(modelpath),'metadataPath':str(PREP/'catalog.json'),'builtinEnvironmentPath':str(environment_path)});return d
 def finish(d):
  d.close();report['sessions'].append({'root':str(d.root),'transcript':d.transcript,'stderr':d.stderr})
 def check(name,response,locations):
  v=response.get('result',{});exact=v.get('completeness')=='exact';same=native.normalized(v.get('locations',[]))==native.normalized(locations)
  report['cases'].append({'id':name,'status':'pass' if exact and same else ('wrong-exact' if exact else 'unavailable-or-error'),'expectedLocations':locations,'response':response})
 d=open_daemon(root,model_path)
 for name,filename,line,col,target,tcol,length in attr_queries:
  response=d.call('navigation/definition',{'path':str(root/filename),'line':line,'character':col})
  check('attribute-'+name,response,[native.location(root/filename,target,tcol,length)])
 hover=d.call('navigation/hover',{'path':str(root/'Use.fs'),'line':3,'character':files['Use.fs'].splitlines()[3].rindex('Day')})
 value=hover.get('result',{});serialized=json.dumps(value);exact=value.get('completeness')=='exact';correct='P:System.DateOnly.Day' in serialized and 'P:System.DateTime.Day' not in serialized
 report['cases'].append({'id':'cross_file_alias_then_external_open','status':'pass' if exact and correct else ('wrong-exact' if exact else 'unavailable-or-error'),'expectedXmlDocId':'P:System.DateOnly.Day','response':hover})
 for line,label in [(2,'tuple'),(3,'option')]:
  response=d.call('navigation/definition',{'path':str(root/'Mixed.fs'),'line':line,'character':files['Mixed.fs'].splitlines()[line].rindex('Name')})
  check('mixed-'+label+'-source-target',response,[native.location(root/'Mixed.fs',1,16,4)])
 finish(d)
 (root/'NotCompiled.fs').write_text('module NotCompiled\nlet value = 1\n')
 d=open_daemon(root,model_path);response=d.call('navigation/definition',{'path':str(root/'Mixed.fs'),'line':1,'character':16})
 check('explicit_include_unrelated_file_control',response,[native.location(root/'Mixed.fs',1,16,4)]);finish(d)
 glob=native.fixture(base,'glob',{'A.fs':'module A\nlet value = 1\n'});project=glob/'Test.fsproj';project.write_text(project.read_text().replace('Include="A.fs"','Include="*.fs"'))
 report['preparation'].append({'phase':'glob_initial_build',**build(project)})
 glob_model=helper('workspace/evaluate',{'root':str(glob)});glob_path=base/'glob-model.json';glob_path.write_text(json.dumps(glob_model));report['preparation'].append({'phase':'glob_evaluate','modelPath':str(glob_path),'modelSha256':hashlib.sha256(glob_path.read_bytes()).hexdigest(),'membershipReusable':[p.get('membershipReusable') for p in glob_model['projects']]})
 d=open_daemon(glob,glob_path);q={'path':str(glob/'A.fs'),'line':1,'character':4,'includeDeclaration':False};check('glob_initial_empty_refs',d.call('navigation/references',q),[]);finish(d)
 (glob/'B.fs').write_text('module B\nlet answer = A.value\n');report['preparation'].append({'phase':'glob_added_source_build',**build(project)})
 d=open_daemon(glob,glob_path);response=d.call('navigation/references',q);v=response.get('result',{});exact=v.get('completeness')=='exact';expected=[native.location(glob/'B.fs',1,15,5)];same=native.normalized(v.get('locations',[]))==native.normalized(expected)
 report['cases'].append({'id':'glob_added_source_old_model','status':'pass' if exact and same else ('wrong-exact' if exact else 'safe-rejection'),'expectedLocations':expected,'response':response});finish(d)
 report['summary']={s:sum(c['status']==s for c in report['cases']) for s in ['pass','wrong-exact','unavailable-or-error','safe-rejection']}
 args.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'output':str(args.output),'summary':report['summary'],'nonpassing':[c for c in report['cases'] if c['status']!='pass']},indent=2))
if __name__=='__main__':main()
