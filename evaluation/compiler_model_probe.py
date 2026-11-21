#!/usr/bin/env python3
"""Bounded genuine SDK11 compiler design-time command-line evaluation."""
import json,os
from pathlib import Path
from prepare import measured_command
from run import file_sha
root=Path(__file__).resolve().parents[1];repo=root.parent/'research/corpus/fsharp';sdk=repo/'.dotnet/dotnet'
out=root/'evaluation/results/recovery-fsharp-design-time-items.json'
env=dict(os.environ,DOTNET_ROOT=str(sdk.parent),PATH=str(sdk.parent)+os.pathsep+os.environ['PATH'],DOTNET_CLI_TELEMETRY_OPTOUT='1',MSBUILDDISABLENODEREUSE='1',TMPDIR=str(root/'.tmp/evaluation-recovery'))
command=[str(sdk),'msbuild','src/Compiler/FSharp.Compiler.Service.fsproj','-nologo','-m:1','-p:DotnetProjInfo=true','-p:IgnoreMibc=true','-p:TargetFramework=net11.0','-p:DesignTimeBuild=true','-p:SkipCompilerExecution=true','-p:ProvideCommandLineArgs=true','-p:BuildProjectReferences=false','-p:GeneratePackageOnBuild=false','-p:UseSharedCompilation=false','-t:Compile','-getProperty:Configuration,TargetFramework,ProjectAssetsFile,TargetPath','-getItem:Compile,ProjectReference,ReferencePath,ReferencePathWithRefAssemblies,FscCommandLineArgs','-getResultOutputFile:'+str(out)]
measurement=measured_command(command,repo,timeout=120,env=env,memory_limit_mib=2048)
report={'kind':'genuine-SDK11-design-time-compile-no-source-compilation','measurement':measurement,'output_path':str(out),'output_sha256':file_sha(out)}
if out.is_file():
 data=json.loads(out.read_text());items=data.get('Items',{});files=[Path(x['FullPath']) for x in items.get('Compile',[])];report['summary']={'compile_count':len(files),'missing_sources':[str(x) for x in files if not x.is_file()],'compiler_argument_count':len(items.get('FscCommandLineArgs',[])),'reference_count':len(items.get('ReferencePath',[]))}
path=root/'evaluation/results/recovery-fsharp-design-time-probe.json';path.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='measurement'}));print(json.dumps({k:measurement[k] for k in ['exit_code','abort_reason','elapsed_ms','peak_tree_rss_kib','output']}))
