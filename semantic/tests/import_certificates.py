#!/usr/bin/env python3
"""Real SDK wildcard/negative import witnesses, including extensionless hooks."""
import hashlib,json,os,pathlib,subprocess,tempfile
from smoke import ROOT,DOTNET,ENV,project
HELPER=pathlib.Path(os.environ.get('FSHARP_ENV_HELPER',ROOT/'semantic/bin/Release/net10.0/FSharpNavigator.Semantic.dll'))
NATIVE=pathlib.Path(os.environ.get('FSHARP_BACKEND_PATH',ROOT/'target/debug/fsharp-navigator'))
def command(args,env,**kw):
 p=subprocess.run(list(map(str,args)),env=env,text=True,capture_output=True,timeout=120,**kw);assert p.returncode==0,(p.stderr+p.stdout)[-2000:];return p.stdout

def main():
 root=pathlib.Path(tempfile.mkdtemp(prefix='sdk-import-proof-',dir=ROOT/'.cache'));records=[]
 def create(name):
  d=root/name;d.mkdir();(d/'A.fs').write_text('module A\nlet value=1\n');(d/'B.fs').write_text('module B\nlet value=A.value\n');project(d/'P.fsproj','<Compile Include="A.fs"/>');command([DOTNET,'restore',d/'P.fsproj','-v:q','--nologo'],ENV);return d
 def evaluate(d,env,label):
  response=json.loads(command([DOTNET,HELPER,'--once'],env,input=json.dumps({'id':1,'method':'workspace/evaluate','params':{'root':str(d),'projects':[str(d/'P.fsproj')]}})+'\n'));assert 'error'not in response,response
  model=response['result'];p=model['projects'][0];assert p['membershipReusable'],p['membershipDiagnostics'][:6];path=d/(label+'.json');path.write_text(json.dumps(model));return model,path
 def native(d,path,env):
  requests=[{'id':1,'method':'workspace/open','params':{'root':str(d),'mode':'indexed','evaluatedModelPath':str(path),'cacheDir':str(d/'cache')}},{'id':2,'method':'workspace/tree','params':{}},{'id':3,'method':'shutdown','params':{}}]
  result=command([NATIVE,'daemon'],env,input=''.join(json.dumps(x)+'\n'for x in requests));return json.loads(result.splitlines()[1])
 d=create('obj');old,path=evaluate(d,ENV,'before');baseline=native(d,path,ENV);assert baseline.get('result',{}).get('completeness')=='exact',baseline
 (d/'Uncompiled.fs').write_text('module Uncompiled\nlet value=A.value\n');control=native(d,path,ENV);assert control.get('result',{}).get('completeness')=='exact',control
 imported=d/'obj/P.fsproj.extra.targets';imported.write_text('<Project><ItemGroup><Compile Include="B.fs"/></ItemGroup></Project>');command([DOTNET,'build',d/'P.fsproj','--no-restore','--nologo','-v:q'],ENV)
 rejected=native(d,path,ENV);assert 'error'in rejected,rejected
 fresh,freshpath=evaluate(d,ENV,'after');assert str(d/'B.fs')in fresh['projects'][0]['files'];current=native(d,freshpath,ENV);assert current.get('result',{}).get('completeness')=='exact',current
 records.append({'case':'SDK obj wildcard import addition rejects old source domain; unrelated source remains valid','passed':True,'oldModel':str(path),'freshModel':str(freshpath),'oldResult':rejected,'freshResult':current});print('PASS SDK obj wildcard import addition and unrelated source control',flush=True)
 d=create('zero-hook');hook=root/'hooks';env=dict(ENV,MSBuildUserExtensionsPath=str(hook));before,path=evaluate(d,env,'before');assert native(d,path,env).get('result',{}).get('completeness')=='exact'
 directory=hook/'Current/Microsoft.Common.targets/ImportAfter';directory.mkdir(parents=True)
 # An ordinary import hook can be extensionless. The witness is all filenames,
 # and its XML is still checked for unsupported source-producing tasks.
 (directory/'extra').write_text('<Project><ItemGroup><Compile Include="B.fs"/></ItemGroup></Project>')
 command([DOTNET,'build',d/'P.fsproj','--no-restore','--nologo','-v:q'],env)
 rejected=native(d,path,env);assert 'error'in rejected,rejected
 fresh,freshpath=evaluate(d,env,'after');assert str(d/'B.fs')in fresh['projects'][0]['files'];assert any(c['extensions']==['*'] and c['root']==str(directory)for c in fresh['projects'][0]['sourceMembership']);current=native(d,freshpath,env);assert current.get('result',{}).get('completeness')=='exact',current
 records.append({'case':'zero-match extensionless SDK hook addition rejects old model','passed':True,'oldModel':str(path),'freshModel':str(freshpath),'oldResult':rejected});print('PASS zero-match extensionless SDK import hook',flush=True)
 (directory/'extra').write_text('<Project><Target Name="Unknown" BeforeTargets="NeverRun"><Exec Command="echo unavailable" /></Target></Project>')
 response=json.loads(command([DOTNET,HELPER,'--once'],env,input=json.dumps({'id':1,'method':'workspace/evaluate','params':{'root':str(d),'projects':[str(d/'P.fsproj')]}})+'\n'));assert not response['result']['projects'][0]['membershipReusable'];records.append({'case':'extensionless imported custom task remains unsupported','passed':True});print('PASS extensionless custom task is not trusted by omission',flush=True)
 evidence={'helperSha256':hashlib.sha256(HELPER.read_bytes()).hexdigest(),'nativeSha256':hashlib.sha256(NATIVE.read_bytes()).hexdigest(),'fixtureRoot':str(root),'cases':records};(ROOT/'test-results/recovery/import-certificates.json').write_text(json.dumps(evidence,indent=2)+'\n')
if __name__=='__main__':main()
