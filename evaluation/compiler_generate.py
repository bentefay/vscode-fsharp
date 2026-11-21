#!/usr/bin/env python3
"""Run pinned compiler repository's actual lexer/parser generation targets."""
import argparse,json,os,subprocess,time
from pathlib import Path
from prepare import measured_command
from run import file_sha
root=Path(__file__).resolve().parents[1]; repo=root.parent/'research/corpus/fsharp'; sdk=repo/'.dotnet/dotnet'
parser=argparse.ArgumentParser();parser.add_argument('phase',choices=['tools','generate','evaluate']);args=parser.parse_args()
env=dict(os.environ,DOTNET_ROOT=str(sdk.parent),PATH=str(sdk.parent)+os.pathsep+os.environ['PATH'],DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_SKIP_FIRST_TIME_EXPERIENCE='1',MSBUILDDISABLENODEREUSE='1',TMPDIR=str(root/'.tmp/evaluation-recovery'))
flags=['-nologo','-m:1','-p:DotnetProjInfo=true','-p:IgnoreMibc=true']
tracked=['NuGet.config','global.json','Directory.Packages.props','eng/Packages.props','eng/Versions.props','eng/Version.Details.props','src/Compiler/FSharp.Compiler.Service.fsproj','buildtools/buildtools.targets','buildtools/fslex/fslex.fsproj','buildtools/fsyacc/fsyacc.fsproj']
report={'phase':args.phase,'repository':str(repo),'commit':subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip(),'sdk':str(sdk),'sdk_sha256':file_sha(sdk),'selected_declared_framework':'net11.0','before_inputs':{x:file_sha(repo/x) for x in tracked},'measurements':[],'notes':['Diagnostic setup costs, not quiet native/baseline performance acceptance.','No repository source/configuration edits, target framework substitution, alternate package sources or compiler build.','SUM RSS of owned process tree sampled at10ms; controller and filesystem/page cache excluded.']}
commands=[]
if args.phase=='tools':
 for tool in ['fslex','fsyacc']:
  commands.append([str(sdk),'build',f'buildtools/{tool}/{tool}.fsproj','--no-restore',*flags,'-v:normal'])
elif args.phase=='generate':
 commands.append([str(sdk),'msbuild','src/Compiler/FSharp.Compiler.Service.fsproj',*flags,'-p:TargetFramework=net11.0','-t:CallFsLex;CallFsYacc','-v:normal'])
else:
 commands.append([str(sdk),'msbuild','src/Compiler/FSharp.Compiler.Service.fsproj',*flags,'-p:TargetFramework=net11.0','-getProperty:Configuration,TargetFramework,FsLexPath,FsYaccPath,FsLexOutputFolder,FsYaccOutputFolder,ProjectAssetsFile','-getItem:Compile','-getResultOutputFile:'+str(root/'evaluation/results/recovery-fsharp-generated-compile-items.json')])
for command in commands:
 measurement=measured_command(command,repo,timeout=120,env=env,memory_limit_mib=2048);report['measurements'].append(measurement)
 print(json.dumps({'command':command,'exit':measurement['exit_code'],'abort':measurement['abort_reason'],'elapsed_ms':measurement['elapsed_ms'],'peak_tree_rss_kib':measurement['peak_tree_rss_kib'],'output_tail':measurement['output'][-2000:]}),flush=True)
 if measurement['exit_code'] or measurement['abort_reason']:break
report['after_inputs']={x:file_sha(repo/x) for x in tracked};report['tracked_inputs_unchanged']=report['before_inputs']==report['after_inputs']
report['generated_files']=[{'path':str(x),'bytes':x.stat().st_size,'sha256':file_sha(x)} for x in sorted((repo/'artifacts/obj/FSharp.Compiler.Service').rglob('*')) if x.is_file() and x.suffix in {'.fs','.fsi'}]
report['tool_outputs']=[{'path':str(x),'bytes':x.stat().st_size,'sha256':file_sha(x)} for tool in ['fslex','fsyacc'] for x in sorted((repo/'artifacts/bin'/tool).rglob('*.dll'))]
out=root/f'evaluation/results/recovery-fsharp-generation-{args.phase}.json';out.write_text(json.dumps(report,indent=2)+'\n');print(str(out),flush=True)
