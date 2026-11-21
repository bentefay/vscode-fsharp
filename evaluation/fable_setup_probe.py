#!/usr/bin/env python3
"""Bounded unchanged Fable fcs-fable SDK100 setup/evaluation probes."""
import argparse,json,os,subprocess
from pathlib import Path
from prepare import measured_command
from run import file_sha
p=argparse.ArgumentParser();p.add_argument('phase',choices=['graph','restore','design-time','build']);a=p.parse_args();root=Path(__file__).resolve().parents[1];repo=root.parent/'research/corpus/Fable';sdk=root/'.tools/dotnet-sdk100/dotnet';project=repo/'src/fcs-fable/fcs-fable.fsproj';out=root/'evaluation/results';items=out/f'fable-fcs-{a.phase}-items.json';graph=out/'fable-fcs-restore-graph.json'
tracked=['global.json','src/fcs-fable/fcs-fable.fsproj','src/Directory.Build.props','src/Directory.Build.targets'];before={x:file_sha(repo/x) for x in tracked};env=dict(os.environ,DOTNET_ROOT=str(sdk.parent),PATH=str(sdk.parent)+os.pathsep+os.environ['PATH'],DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_SKIP_FIRST_TIME_EXPERIENCE='1',MSBUILDDISABLENODEREUSE='1',TMPDIR=str(root/'.tmp/evaluation-recovery'))
if a.phase=='restore':command=[str(sdk),'restore',str(project),'--disable-parallel','--nologo','-m:1','-v:minimal']
elif a.phase=='build':command=[str(sdk),'build',str(project),'--no-restore','--nologo','-m:1','-v:minimal']
else:
 command=[str(sdk),'msbuild',str(project),'-nologo','-m:1','-nodeReuse:false','-getProperty:Configuration,TargetFramework,ProjectAssetsFile,DefineConstants,TargetPath','-getItem:Compile,ProjectReference,ReferencePath,FscCommandLineArgs','-getResultOutputFile:'+str(items)]
 if a.phase=='graph':command+=['-t:GenerateRestoreGraphFile','-p:RestoreGraphOutputPath='+str(graph)]
 else:command+=['-t:Compile','-p:DesignTimeBuild=true','-p:SkipCompilerExecution=true','-p:ProvideCommandLineArgs=true','-p:BuildProjectReferences=false','-p:GeneratePackageOnBuild=false','-p:UseSharedCompilation=false']
limit=90 if a.phase in ['restore','build'] else 60;memory=4096 if a.phase=='build' else 1536
measurement=measured_command(command,repo,timeout=limit,env=env,memory_limit_mib=memory);report={'kind':'additional-corpus-genuine-selected-project-setup','phase':a.phase,'repository':str(repo),'project':str(project),'commit':subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip(),'sdk':'10.0.100','sdk_dotnet_sha256':file_sha(sdk),'before_config_hashes':before,'after_config_hashes':{x:file_sha(repo/x) for x in tracked},'measurement':measurement,'notes':['The selected project is fcs-fable.fsproj plus genuine references, not whole Fable or dotnet/fsharp readiness.','No TFM,SDK,source,configuration,analyzer or package-source substitutions.','SUM owned-process-tree RSS excludes controller and filesystem cache. Shared-machine setup is not a comparative performance result.']}
if items.exists():
 data=json.loads(items.read_text());sources=[Path(x['FullPath']) for x in data.get('Items',{}).get('Compile',[])];report['evaluated']={'properties':data.get('Properties'),'compile_entries':len(sources),'existing_sources':sum(x.is_file() for x in sources),'missing_sources':[str(x) for x in sources if not x.is_file()],'source_bytes':sum(x.stat().st_size for x in sources if x.is_file()),'source_physical_lines':sum(len(x.read_text().splitlines()) for x in sources if x.is_file()),'project_references':data.get('Items',{}).get('ProjectReference',[]),'compiler_argument_count':len(data.get('Items',{}).get('FscCommandLineArgs',[])),'reference_count':len(data.get('Items',{}).get('ReferencePath',[])),'items_sha256':file_sha(items)}
if graph.exists():report['restore_graph_sha256']=file_sha(graph)
path=out/f'fable-fcs-{a.phase}-setup.json';path.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'report':str(path),'evaluated':report.get('evaluated'),'measurement':{k:measurement[k] for k in ['exit_code','abort_reason','elapsed_ms','peak_tree_rss_kib','output']}},indent=2))
