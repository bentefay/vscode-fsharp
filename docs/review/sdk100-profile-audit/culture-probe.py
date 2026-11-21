import pathlib,subprocess,json,os,hashlib,time
ROOT=pathlib.Path(__file__).resolve().parents[3]
stage=pathlib.Path(__file__).resolve().parent
fixture=stage/'culture-fixture';fixture.mkdir(exist_ok=True)
sdk=ROOT/'.tools/dotnet-sdk100/dotnet';helper=ROOT/'evaluation/.runtime/helper-f215dac4599c/FSharpNavigator.Semantic.dll'
(fixture/'global.json').write_text(json.dumps({'sdk':{'version':'10.0.100','rollForward':'latestPatch'}}))
(fixture/'P.fsproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DisableImplicitFSharpCoreReference>true</DisableImplicitFSharpCoreReference><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="FSharp.Core"><HintPath>$(MSBuildToolsPath)/FSharp/FSharp.Core.dll</HintPath></Reference><Compile Include="A.fs"/><Compile Include="Sources/**/I/*.fs"/></ItemGroup></Project>')
(fixture/'A.fs').write_text('module A\nlet value=1\n')
for name,text in [('Sources/i/source.fs','module Latin\nlet count=0\n'),('Sources/ı/source.fs','module Dotless\nlet count=A.value\n')]:
 p=fixture/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)
env=dict(os.environ,DOTNET_ROOT=str(sdk.parent),FSHARP_NAVIGATOR_DOTNET=str(sdk),MSBUILDDISABLENODEREUSE='1',DOTNET_CLI_USE_MSBUILD_SERVER='0',DOTNET_NOLOGO='1',DOTNET_CLI_TELEMETRY_OPTOUT='1')
for name in ['DOTNET_CLI_UI_LANGUAGE','VSLANG','LANGUAGE','LC_CTYPE','LC_MESSAGES','DOTNET_SYSTEM_GLOBALIZATION_INVARIANT']:
 env.pop(name,None)
results=[]
def command(args,environment,**kwargs):
 started=time.monotonic();p=subprocess.run([str(x) for x in args],cwd=fixture,env=environment,text=True,capture_output=True,timeout=60,**kwargs)
 record={'command':[str(x) for x in args],'exitCode':p.returncode,'elapsedSeconds':time.monotonic()-started,'stdout':p.stdout,'stderr':p.stderr};results.append(record)
 if p.returncode:raise RuntimeError(str(record)[-3000:])
 return p.stdout
def hashes():
 return {str(p.relative_to(fixture)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [fixture/'global.json',fixture/'P.fsproj',fixture/'A.fs',fixture/'Sources/i/source.fs',fixture/'Sources/ı/source.fs']}
fixed=hashes();summaries=[]
for culture in ['en_US.UTF-8','tr_TR.UTF-8']:
 current=dict(env,LANG=culture,LC_ALL=culture)
 if not summaries:command([sdk,'restore',fixture/'P.fsproj','--nologo','-v:q'],current)
 command([sdk,'build',fixture/'P.fsproj','--no-restore','--nologo','-v:q','-nodeReuse:false','-p:UseSharedCompilation=false'],current)
 payload={'id':1,'method':'workspace/evaluate','params':{'root':str(fixture),'projects':[str(fixture/'P.fsproj')]}}
 response=json.loads(command([sdk,helper,'--once'],current,input=json.dumps(payload)+'\n'));assert 'result'in response,response
 model=response['result'];destination=fixture/(culture.split('.')[0]+'.json');destination.write_text(json.dumps(model,indent=2)+'\n');project=model['projects'][0];names={v['name'].upper() for v in project['environmentCertificate']['inputs']}
 summaries.append({'culture':culture,'model':str(destination.relative_to(stage)),'modelSha256':hashlib.sha256(destination.read_bytes()).hexdigest(),'files':project['files'],'membershipReusable':project['membershipReusable'],'diagnostics':project['membershipDiagnostics'],'certificateCultureNames':sorted(names.intersection({'LANG','LC_ALL','LC_CTYPE','LC_MESSAGES','DOTNET_CLI_UI_LANGUAGE','VSLANG','DOTNET_SYSTEM_GLOBALIZATION_INVARIANT'})),'environmentCertificate':project['environmentCertificate']})
report={'scope':'unchanged frozen helper; SDK100 profile remains rejected; genuine current builds and cert evidence only, no native false-exact claim','sdk':'10.0.100','helper':str(helper),'helperSha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'sourceBytesUnchanged':hashes()==fixed,'sourceHashes':fixed,'summaries':summaries,'commands':results}
(stage/'culture-probe.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:report[k] for k in ['scope','sourceBytesUnchanged','summaries']},indent=2))
