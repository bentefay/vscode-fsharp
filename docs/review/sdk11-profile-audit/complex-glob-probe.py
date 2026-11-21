import pathlib,subprocess,json,os
from smoke import DOTNET,ENV,project
root=pathlib.Path.cwd()/'complex-glob-probe';root.mkdir(exist_ok=True)
(root/'A.fs').write_text('module A\nlet value=1\n')
for name,module in [('i/source.fs','Latin'),('ı/source.fs','Dotless'),('İ/source.fs','CapitalDot')]:
 (root/name).parent.mkdir(exist_ok=True)
 (root/name).write_text('module '+module+'\nlet useValue=A.value\n')
project(root/'P.fsproj','<Compile Include="A.fs"/><Compile Include="**/I/*.fs"/>')
results=[]
for flag in [None,'1']:
 env=dict(ENV,LANG='tr_TR.UTF-8',LC_ALL='tr_TR.UTF-8')
 if flag is None:env.pop('MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS',None)
 else:env['MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS']=flag
 r=subprocess.run([str(DOTNET),'msbuild',str(root/'P.fsproj'),'-nologo','-getItem:Compile'],env=env,cwd=root,text=True,capture_output=True,timeout=45)
 result={'flag':flag,'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr};results.append(result)
 print(flag,r.returncode,[x['Identity']for x in json.loads(r.stdout).get('Items',{}).get('Compile',[])])
(pathlib.Path.cwd()/'complex-glob-probe.json').write_text(json.dumps(results,indent=2))
