#!/usr/bin/env python3
"""Actual environment-conditioned Compile item mutation with unchanged XML."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--initial-missing',action='store_true');a=p.parse_args();initial=None if a.initial_missing else 'false'
 spec=importlib.util.spec_from_file_location('prepared',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep)
 n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py');base=Path(tempfile.mkdtemp(prefix='environment-membership-',dir=ROOT/'.tmp'));root=n.fixture(base,'project',{'A.fs':'module A\nlet value = 1\n','B.fs':'module B\nlet answer = A.value\n'})
 project=root/'Test.fsproj';text=project.read_text().replace('<Compile Include="B.fs"/>','<Compile Include="B.fs" Condition="\'$(IncludeB)\' == \'true\'"/>');project.write_text(text)
 env=dict(prep.ENV);report={'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'helperSha256':hashlib.sha256(prep.HELPER.read_bytes()).hexdigest(),'fixtureRoot':str(root),'projectXml':text,'initialIncludeB':initial,'builds':[],'sessions':[],'provenance':'Genuine environment-conditioned Compile before/after builds. Project/source bytes unchanged; IncludeB changes from false or missing to true. The old evaluated-model import is the subject under review.'}
 def select(value):
  if value is None:env.pop('IncludeB',None)
  else:env['IncludeB']=value
 def build(value):
  select(value)
  r=subprocess.run([str(prep.DOTNET),'build',str(project),'--nologo','-v:q','-m:1','-nodeReuse:false','-p:NuGetAudit=false'],env=env,text=True,capture_output=True,timeout=90);assert r.returncode==0,r.stdout+r.stderr;report['builds'].append({'IncludeB':value,'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
 def evaluate(value):
  select(value);r=subprocess.run([str(prep.DOTNET),str(prep.HELPER),'--once'],input=json.dumps({'id':1,'method':'workspace/evaluate','params':{'root':str(root)}})+'\n',env=env,text=True,capture_output=True,timeout=90);response=json.loads(r.stdout);assert 'error' not in response,response;return response['result']
 build(initial);model=evaluate(initial);modelpath=base/'model-before.json';modelpath.write_text(json.dumps(model));report['initialModel']={'path':str(modelpath),'sha256':hashlib.sha256(modelpath.read_bytes()).hexdigest(),'projects':[{'files':p['files'],'membershipReusable':p.get('membershipReusable'),'membershipDiagnostics':p.get('membershipDiagnostics')} for p in model['projects']]}
 def query(value,modelpath):
  old=os.environ.get('IncludeB')
  if value is None:os.environ.pop('IncludeB',None)
  else:os.environ['IncludeB']=value
  try:d=n.Daemon(a.binary.resolve(),root)
  finally:
   if old is None:os.environ.pop('IncludeB',None)
   else:os.environ['IncludeB']=old
  d.call('workspace/open',{'root':str(root),'mode':'indexed','cacheDir':str(base/'cache'),'evaluatedModelPath':str(modelpath)})
  response=d.call('navigation/references',{'path':str(root/'A.fs'),'line':1,'character':4,'includeDeclaration':False});d.close();report['sessions'].append({'IncludeB':value,'model':str(modelpath),'transcript':d.transcript,'stderr':d.stderr});return response
 report['before']=query(initial,modelpath);build('true');report['afterOldModel']=query('true',modelpath)
 fresh=evaluate('true');freshpath=base/'model-true.json';freshpath.write_text(json.dumps(fresh));report['freshFiles']=[p['files'] for p in fresh['projects']];report['afterFreshModel']=query('true',freshpath)
 result=report['afterOldModel'].get('result',{});expected=[n.location(root/'B.fs',1,15,5)]
 report['status']='wrong-exact' if result.get('completeness')=='exact' and n.normalized(result.get('locations',[]))!=n.normalized(expected) else ('pass' if result.get('completeness')=='exact' else 'safe-rejection')
 a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'status':report['status'],'initialModel':report['initialModel'],'before':report['before'],'afterOldModel':report['afterOldModel'],'afterFreshModel':report['afterFreshModel']},indent=2))
if __name__=='__main__':main()
