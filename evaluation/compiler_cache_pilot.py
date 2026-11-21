#!/usr/bin/env python3
"""Bounded exact-version cache priming from an existing compiler repository feed."""
import base64,hashlib,json,os,subprocess,xml.etree.ElementTree as ET
from pathlib import Path
from prepare import measured_command
from run import file_sha
root=Path(__file__).resolve().parents[1];repo=root.parent/'research/corpus/fsharp';sdk=repo/'.dotnet/dotnet';work=root/'evaluation/.runtime/compiler-cache-pilot';work.mkdir(parents=True,exist_ok=True)
config=repo/'NuGet.config';tree=ET.parse(config);sources=tree.getroot().find('packageSources');selected=next(x for x in sources.findall('add') if x.attrib['key']=='dotnet-public');feed=selected.attrib['value']
# Narrow only this isolated bootstrap project to one already eligible source.
# Repository/user source mapping must be absent; never silently bypass mapping.
for path in [config,Path('/home/agent/.nuget/NuGet/NuGet.Config')]:
 parsed=ET.parse(path).getroot()
 if parsed.find('packageSourceMapping') is not None:raise RuntimeError('Source mapping requires explicit policy analysis; pilot refused')
for node in list(sources):
 if node.tag=='add' and node is not selected:sources.remove(node)
tree.write(work/'NuGet.config',encoding='utf-8',xml_declaration=True)
versions={'System.Buffers':'4.6.1','System.Memory':'4.6.3','System.Runtime.CompilerServices.Unsafe':'6.1.2','NETStandard.Library':'2.0.3'}
(work/'global.json').write_text(json.dumps({'sdk':{'version':'11.0.100-rc.3.26522.148','rollForward':'disable'}})+'\n')
project='<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net11.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems><RestorePackagesWithLockFile>true</RestorePackagesWithLockFile></PropertyGroup><ItemGroup>'+''.join(f'<PackageDownload Include="{name}" Version="[{version}]" />' for name,version in versions.items())+'</ItemGroup></Project>'
(work/'CachePilot.csproj').write_text(project+'\n')
tracked=['NuGet.config','global.json','Directory.Packages.props','eng/Packages.props','eng/Versions.props','eng/Version.Details.props','src/Compiler/FSharp.Compiler.Service.fsproj'];before={p:file_sha(repo/p) for p in tracked}
cache=Path('/home/agent/.nuget/packages');prior={name:(cache/name.lower()/version/'.nupkg.metadata').is_file() for name,version in versions.items()}
env=dict(os.environ,DOTNET_ROOT=str(sdk.parent),DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_SKIP_FIRST_TIME_EXPERIENCE='1',MSBUILDDISABLENODEREUSE='1',TMPDIR=str(root/'.tmp/evaluation-recovery'))
command=[str(sdk),'restore',str(work/'CachePilot.csproj'),'--disable-parallel','--nologo','-m:1','-v:normal']
measurement=measured_command(command,work,timeout=60,env=env,memory_limit_mib=1024)
packages=[]
for name,version in versions.items():
 folder=cache/name.lower()/version;nupkg=folder/(name.lower()+'.'+version+'.nupkg');sha=folder/(name.lower()+'.'+version+'.nupkg.sha512');meta=folder/'.nupkg.metadata';m=json.loads(meta.read_text()) if meta.is_file() else None;actual=base64.b64encode(hashlib.sha512(nupkg.read_bytes()).digest()).decode() if nupkg.is_file() else None
 packages.append({'name':name,'version':version,'already_present':prior[name],'downloaded':meta.is_file(),'metadata':m,'package_sha256':file_sha(nupkg),'computed_archive_sha512_base64':actual,'nuget_sha512':sha.read_text().strip() if sha.is_file() else None,'hash_file_matches_archive':actual is not None and sha.is_file() and actual==sha.read_text().strip()})
report={'kind':'exact-configured-feed-cache-pilot','repository':str(repo),'repository_commit':subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip(),'feed':feed,'packages':packages,'measurement':measurement,'tracked_inputs_before':before,'tracked_inputs_after':{p:file_sha(repo/p) for p in tracked},'pilot_config_sha256':file_sha(work/'NuGet.config'),'pilot_project_sha256':file_sha(work/'CachePilot.csproj'),'pilot_lock_sha256':file_sha(work/'packages.lock.json'),'notes':['Repository configuration/source/SDK unchanged. Bootstrap narrows its own feed list to already configured dotnet-public; no package source mapping exists in active configs.','nuget.org is audit-only in repository and was not used as a package source.','No pre-existing repository packages.lock.json content hashes; downloaded archive and NuGet hash provenance recorded, not invented as pre-pinned hashes.','PackageDownload requests exact selected versions and does not silently substitute or retarget the compiler project.']}
output=root/'evaluation/results/recovery-fsharp-cache-pilot.json';output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'report':str(output),'exit':measurement['exit_code'],'abort':measurement['abort_reason'],'packages':[{k:x[k] for k in ['name','downloaded','hash_file_matches_archive']} for x in packages]}))
