#!/usr/bin/env python3
"""Actual MSBuild consumed-input certificates plus native lazy replay.
No ambient values are retained in evidence; fixture values are synthetic.
"""
import hashlib,json,os,pathlib,subprocess,tempfile,time
from smoke import ROOT,DOTNET,ENV,project
HELPER=pathlib.Path(os.environ.get('FSHARP_ENV_HELPER',ROOT/'semantic/bin/Release/net10.0/FSharpNavigator.Semantic.dll'))
NATIVE=pathlib.Path(os.environ.get('FSHARP_BACKEND_PATH',ROOT/'target/debug/fsharp-navigator'))

def run(command,environment,**kwargs):
 p=subprocess.run(list(map(str,command)),env=environment,text=True,capture_output=True,timeout=120,**kwargs)
 assert p.returncode==0,(p.stderr+p.stdout)[-2000:]
 return p.stdout

def main():
 base=pathlib.Path(tempfile.mkdtemp(prefix='environment-proof-',dir=ROOT/'.cache'))
 environment=dict(ENV)
 for key in list(environment):
  if key.lower() in {'includeb','chooseb','otherflags','unused_env_probe','configuration','includenumber','textprobe'}:environment.pop(key)
 records=[]
 def evaluate(directory,env,label):
  payload={'id':1,'method':'workspace/evaluate','params':{'root':str(directory),'projects':[str(directory/'P.fsproj')]}}
  before=time.monotonic();reply=json.loads(run([DOTNET,HELPER,'--once'],env,input=json.dumps(payload)+'\n'));assert 'error'not in reply,reply
  model=reply['result'];file=directory/(label+'.json');file.write_text(json.dumps(model))
  cert=model['projects'][0]['environmentCertificate']
  return model,file,cert,time.monotonic()-before
 def native(directory,file,env):
  requests=[{'id':1,'method':'workspace/open','params':{'root':str(directory),'mode':'indexed','evaluatedModelPath':str(file),'cacheDir':str(directory/'cache')}},{'id':2,'method':'workspace/tree','params':{}},{'id':3,'method':'shutdown','params':{}}]
  output=run([NATIVE,'daemon'],env,input=''.join(json.dumps(r)+'\n'for r in requests));return json.loads(output.splitlines()[1])
 def create(name,props='',condition="'$(IncludeB)' == 'true'",extra=''):
  directory=base/name;directory.mkdir();(directory/'A.fs').write_text('module A\nlet value=1\n');(directory/'B.fs').write_text('module B\nlet value=A.value\n')
  project(directory/'P.fsproj','<Compile Include="A.fs"/><Compile Include="B.fs" Condition="'+condition.replace('"','&quot;')+'"/>')
  p=directory/'P.fsproj';p.write_text(p.read_text().replace('<TargetFramework>',props+'<TargetFramework>').replace('</Project>',extra+'</Project>'))
  run([DOTNET,'restore',p,'--nologo','-v:q'],environment)
  return directory
 def acceptance(label,directory,env,changed,expected_stale=True,inspect=None):
  model,file,cert,seconds=evaluate(directory,env,'before');assert model['projects'][0]['membershipReusable'],model['projects'][0]['membershipDiagnostics'][:5]
  if inspect:inspect(cert)
  first=native(directory,file,env);assert first.get('result',{}).get('completeness')=='exact',first.get('error',first.get('result',{}).get('diagnostics'))
  replay=native(directory,file,changed)
  if expected_stale:assert 'error'in replay and 'environment changed'in replay['error']['message'],replay
  else:assert replay.get('result',{}).get('completeness')=='exact',replay.get('error',replay.get('result',{}).get('diagnostics'))
  fresh,_,_,_=evaluate(directory,changed,'after');assert fresh['projects'][0]['membershipReusable']
  records.append({'case':label,'passed':True,'oldModel':str(file),'currentModel':str(directory/'after.json'),'oldReplay':replay,'preparationSeconds':seconds,'inputCount':len(cert['inputs'])})
  print('PASS',label,flush=True)
 positive=create('positive');acceptance('positive ambient change rejects old source model',positive,dict(environment,IncludeB='false'),dict(environment,IncludeB='true'))
 negative=create('missing');acceptance('missing ambient becomes present',negative,environment,dict(environment,IncludeB='true'))
 default=create('default','<IncludeB Condition="\'$(IncludeB)\' == \'\'">false</IncludeB>');acceptance('conditional default captures original missing state',default,environment,dict(environment,IncludeB='true'))
 earlier=create('before-overwrite','<Selected>$(IncludeB)</Selected><IncludeB>false</IncludeB>',"'$(Selected)' == 'true'");acceptance('read before overwrite remains a dependency',earlier,dict(environment,IncludeB='false'),dict(environment,IncludeB='true'))
 overwritten=create('overwritten','<IncludeB>false</IncludeB>');acceptance('unconditional overwrite avoids unused present ambient dependency',overwritten,dict(environment,IncludeB='false'),dict(environment,IncludeB='true'),False)
 fixed=create('global',condition="'$(Configuration)' == 'Release'");acceptance('explicit Configuration global overrides ambient',fixed,dict(environment,Configuration='Debug'),dict(environment,Configuration='Release'),False)
 unused=create('irrelevant');acceptance('irrelevant editor shell values do not invalidate',unused,environment,dict(environment,UNUSED_ENV_PROBE='changed',PWD='/somewhere/else',SHLVL='99',VSCODE_ENV_PROBE='added'),False)
 flags=create('execution');acceptance('absent execution-only OtherFlags gets negative certificate',flags,environment,dict(environment,OtherFlags='--define:ENV_PROBE'))
 resolver=create('resolver');acceptance('early SDK resolver override is certified',resolver,environment,dict(environment,MSBuildSDKsPath=str(DOTNET.parent/'sdk/10.0.400/Sdks')))
 numeric=create('pure-modulo','<IncludeNumber Condition="&apos;$(IncludeNumber)&apos; == &apos;&apos;">0</IncludeNumber><Remainder>$([MSBuild]::Modulo($(IncludeNumber), 2))</Remainder>',"'$(Remainder)' == '1'");acceptance('pure Modulo keeps property argument dependency',numeric,dict(environment,IncludeNumber='1'),dict(environment,IncludeNumber='2'))
 text=create('pure-string',"<Prefix Condition=\"$([System.String]::Copy('$(TextProbe)').Length) >= 2\">$([System.String]::Copy('$(TextProbe)').Substring(0, 2))</Prefix>","'$(Prefix)' == 'ab'");acceptance('pure String Copy Length Substring keeps argument dependency',text,dict(environment,TextProbe='abcd'),dict(environment,TextProbe='cdef'))
 direct=create('direct' ,'<IncludeB>$([System.Environment]::GetEnvironmentVariable(\'ENV_DIRECT_PROBE\'))</IncludeB>');model,_,_,_=evaluate(direct,environment,'unsupported');assert not model['projects'][0]['membershipReusable'];records.append({'case':'direct arbitrary environment API refuses reuse','passed':True,'diagnostics':model['projects'][0]['membershipDiagnostics']});print('PASS direct arbitrary environment API refuses reuse',flush=True)
 evidence={'helperSha256':hashlib.sha256(HELPER.read_bytes()).hexdigest(),'nativeSha256':hashlib.sha256(NATIVE.read_bytes()).hexdigest(),'fixtureRoot':str(base),'scope':'real MSBuild models and native lazy tree replay; no editor or timing acceptance','cases':records}
 destination=ROOT/'test-results/recovery/environment-certificate.json';destination.write_text(json.dumps(evidence,indent=2)+'\n');print('Evidence',destination,flush=True)
if __name__=='__main__':main()
