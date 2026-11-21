import pathlib,subprocess,json,os,hashlib,time
from smoke import DOTNET,ENV,project,ROOT
stage=pathlib.Path.cwd();root=stage/'cultural-membership-witness-v2';root.mkdir(exist_ok=True)
helper=stage/'producer-source/bin/Release/net10.0/FSharpNavigator.Semantic.dll';native=ROOT/'target/debug/fsharp-navigator'
(root/'A.fs').write_text('module A\nlet value = 1\n')
for name,text in [('Sources/i/source.fs','module Latin\nlet number = 0\n'),('Sources/ı/source.fs','module Dotless\nlet number = A.value\n')]:
 p=root/name;p.parent.mkdir(exist_ok=True,parents=True);p.write_text(text)
project(root/'P.fsproj','<Compile Include="A.fs"/><Compile Include="Sources/**/I/*.fs"/>')
env=dict(ENV,LANG='tr_TR.UTF-8',LC_ALL='tr_TR.UTF-8');env.pop('MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS',None)
changed=dict(env,MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS='1')
def command(args,e,**kw):
 p=subprocess.run(list(map(str,args)),env=e,cwd=root,text=True,capture_output=True,timeout=120,**kw)
 assert p.returncode==0,(p.stdout+p.stderr)[-3000:]
 return p.stdout
def evaluate(e,label):
 output=command([DOTNET,helper,'--once'],e,input=json.dumps({'id':1,'method':'workspace/evaluate','params':{'root':str(root)}})+'\n');response=json.loads(output);assert 'result'in response,response;model=response['result'];path=root/(label+'.json');path.write_text(json.dumps(model));return model,path
def query(e,path):
 q=[{'id':1,'method':'workspace/open','params':{'root':str(root),'evaluatedModelPath':str(path),'mode':'indexed','cacheDir':str(root/'cache')}},{'id':2,'method':'navigation/references','params':{'path':str(root/'A.fs'),'line':1,'character':5,'includeDeclaration':False}},{'id':3,'method':'shutdown','params':{}}]
 return [json.loads(x)for x in command([native,'daemon'],e,input=''.join(json.dumps(x)+'\n'for x in q)).splitlines()]
def fingerprints():return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in [root/'P.fsproj',root/'A.fs',root/'Sources/i/source.fs',root/'Sources/ı/source.fs',root/'global.json']}
command([DOTNET,'restore',root/'P.fsproj','--nologo','-v:q'],env)
build1=command([DOTNET,'build',root/'P.fsproj','--no-restore','--nologo','-v:q','-nodeReuse:false'],env)
old,path=evaluate(env,'model-before');before=query(env,path);stamps=fingerprints()
build2=command([DOTNET,'build',root/'P.fsproj','--no-restore','--nologo','-v:q','-nodeReuse:false'],changed)
stale=query(changed,path);fresh,freshpath=evaluate(changed,'model-after');after=query(changed,freshpath)
report={'scope':'SDK11 actual builds, frozen-source source-membership mutation; diagnostic profile only, shared SDK11 profile remains disabled','fixtureRoot':str(root),'sdk':str(DOTNET),'helper':str(helper),'helperSha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'nativeSha256':hashlib.sha256(native.read_bytes()).hexdigest(),'mutatedEnvironmentName':'MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS','culture':'tr-TR','fixedSourceHashes':stamps,'sourceBytesUnchanged':stamps==fingerprints(),'buildBefore':build1,'buildAfter':build2,'oldMembershipReusable':old['projects'][0]['membershipReusable'],'oldDiagnostics':old['projects'][0]['membershipDiagnostics'],'oldCertificateHasMutationName':any(x['name'].upper()=='MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS'for x in old['projects'][0]['environmentCertificate']['inputs']),'oldFiles':old['projects'][0]['files'],'freshFiles':fresh['projects'][0]['files'],'before':before,'afterOldModel':stale,'afterFreshModel':after}
(stage/'cultural-membership-witness-v2.json').write_text(json.dumps(report,indent=2))
print(json.dumps({k:report[k]for k in ['sourceBytesUnchanged','oldMembershipReusable','oldCertificateHasMutationName','oldFiles','freshFiles','before','afterOldModel','afterFreshModel']},indent=2))
