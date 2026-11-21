#!/usr/bin/env python3
"""A live daemon must invalidate a newly created nested SDK import root."""
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--helper',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py');base=Path(tempfile.mkdtemp(prefix='live-import-watch-',dir=ROOT/'.tmp'));root=n.fixture(base,'consumer',{'A.fs':'module A\nlet value = 1\n','B.fs':'module B\nlet answer = A.value\n'});project=root/'Test.fsproj';project.write_text(project.read_text().replace('<Compile Include="B.fs"/>',''));userroot=base/'absent-user-extensions';env=dict(prep.ENV,MSBuildUserExtensionsPath=str(userroot));report={'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'helperSha256':hashlib.sha256(a.helper.read_bytes()).hexdigest(),'fixtureRoot':str(base),'sourceFiles':{x.name:x.read_text() for x in root.glob('*.fs')},'projectXml':project.read_text(),'builds':[],'sessions':[],'provenance':'One live daemon seeds a complete empty reference answer; previously absent nested user ImportAfter directory and extensionless XML file are created, then an actual ordinary build verifies newly compiled B.fs before requery. This is a normal synchronization test, not an atomic filesystem snapshot or timing claim.'}
 def build(restore):
  command=[str(prep.DOTNET),'build',str(project),'--nologo','-v:q','-m:1','-nodeReuse:false','-p:NuGetAudit=false']+([] if restore else ['--no-restore']);r=subprocess.run(command,env=env,text=True,capture_output=True,timeout=90);assert r.returncode==0,r.stdout+r.stderr;report['builds'].append({'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
 def evaluate(label):
  r=subprocess.run([str(prep.DOTNET),str(a.helper.resolve()),'--once'],input=json.dumps({'id':1,'method':'workspace/evaluate','params':{'root':str(root)}})+'\n',env=env,text=True,capture_output=True,timeout=120);v=json.loads(r.stdout);assert 'error' not in v,v;path=base/(label+'.json');path.write_text(json.dumps(v['result']));return v['result'],path
 def opened(model):
  old={k:os.environ.get(k) for k in env};os.environ.update(env)
  try:d=n.Daemon(a.binary.resolve(),root)
  finally:
   for k,v in old.items():
    if v is None:os.environ.pop(k,None)
    else:os.environ[k]=v
  d.call('workspace/open',{'root':str(root),'mode':'indexed','cacheDir':str(base/'cache'),'evaluatedModelPath':str(model)});return d
 def finish(d):d.close();report['sessions'].append({'transcript':d.transcript,'stderr':d.stderr})
 build(True);model,modelpath=evaluate('before-model');assert not userroot.exists();report['initialReusable']=model['projects'][0]['membershipReusable'];report['initialDomains']=model['projects'][0]['sourceMembership'];q={'path':str(root/'A.fs'),'line':1,'character':4,'includeDeclaration':False};d=opened(modelpath);report['before']=d.call('navigation/references',q);report['warmBefore']=d.call('navigation/references',q)
 added=userroot/'Current/Microsoft.Common.targets/ImportAfter/reviewer-extension';added.parent.mkdir(parents=True);added.write_text('<Project><ItemGroup><Compile Include="B.fs"/></ItemGroup></Project>');report['addedImport']={'path':str(added),'text':added.read_text()};build(False);report['sameSessionAfter']=d.call('navigation/references',q);finish(d)
 d=opened(modelpath);report['restartAfter']=d.call('navigation/references',q);finish(d);fresh,freshpath=evaluate('after-model');report['freshFiles']=[p['files'] for p in fresh['projects']];d=opened(freshpath);report['afterFreshModel']=d.call('navigation/references',q);finish(d)
 expected=[n.location(root/'B.fs',1,15,5)];report['expectedLocations']=expected;report['checks']={}
 for key in ['before','warmBefore','afterFreshModel']:
  v=report[key].get('result',{});wanted=expected if key=='afterFreshModel' else [];report['checks'][key]=v.get('completeness')=='exact' and v.get('complete') is True and n.normalized(v.get('locations',[]))==n.normalized(wanted)
 for key in ['sameSessionAfter','restartAfter']:report['checks'][key]=report[key].get('result',{}).get('completeness')!='exact'
 report['passed']=all(report['checks'].values());a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'passed':report['passed'],'checks':report['checks'],'sameSessionAfter':report['sameSessionAfter'],'afterFreshModel':report['afterFreshModel']},indent=2))
if __name__=='__main__':main()
