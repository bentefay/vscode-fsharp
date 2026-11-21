#!/usr/bin/env python3
"""Cache-only replicas of genuine PackageSpecs, verified before optional restore."""
import argparse,base64,hashlib,json,os,xml.etree.ElementTree as ET
from pathlib import Path
from prepare import measured_command
from run import file_sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--restore',action='store_true');p.add_argument('--graph',type=Path);p.add_argument('--work-dir',type=Path);p.add_argument('--output',type=Path);p.add_argument('--source-name',action='append',help='Already configured, eligible source key; default dotnet-public');a=p.parse_args()
root=Path(__file__).resolve().parents[1];repo=root.parent/'research/corpus/fsharp';work=(a.work_dir or root/'evaluation/.runtime/compiler-cache-closure-v2').resolve();work.mkdir(parents=True,exist_ok=True);graph_path=(a.graph or root/'evaluation/results/recovery-fsharp-restore-graph.json').resolve();original=json.loads(graph_path.read_text());sdk=repo/'.dotnet/dotnet'
paths={name:work/Path(name).stem/(Path(name).stem+'.csproj') for name in original['projects']}
config=ET.parse(repo/'NuGet.config');sources=config.getroot().find('packageSources');selected_sources=set(a.source_name or ['dotnet-public']);feed={}
for node in list(sources):
 if node.tag=='add':
  if node.attrib['key'] in selected_sources:feed[node.attrib['key']]=node.attrib['value']
  else:sources.remove(node)
if set(feed)!=selected_sources:raise ValueError('Configured source absent')
for path in [repo/'NuGet.config',Path('/home/agent/.nuget/NuGet/NuGet.Config')]:
 if ET.parse(path).getroot().find('packageSourceMapping') is not None:raise ValueError('Mapping requires explicit policy analysis')
config.write(work/'NuGet.config',encoding='utf-8',xml_declaration=True);(work/'global.json').write_text(json.dumps({'sdk':{'version':'11.0.100-rc.3.26522.148','rollForward':'disable'}})+'\n')
solution=ET.Element('Solution')
for source,spec in original['projects'].items():
 path=paths[source];path.parent.mkdir(exist_ok=True);ET.SubElement(solution,'Project',Path=str(path.relative_to(work)))
 project=ET.Element('Project',Sdk='Microsoft.NET.Sdk');props=ET.SubElement(project,'PropertyGroup');restore=spec['restore']
 values={'TargetFrameworks':';'.join(spec['frameworks']),'EnableDefaultCompileItems':'false','DisableImplicitAssetTargetFallback':'true','ManagePackageVersionsCentrally':'true','CentralPackageTransitivePinningEnabled':'true','RestorePackagesWithLockFile':'true','NuGetAudit':'true','NuGetAuditMode':restore.get('restoreAuditProperties',{}).get('auditMode','all')}
 if spec.get('runtimes'):values['RuntimeIdentifiers']=';'.join(spec['runtimes'])
 for name,value in values.items():ET.SubElement(props,name).text=value
 central=ET.Element('Project')
 for tfm,f in spec['frameworks'].items():
  condition=f"'$(TargetFramework)' == '{tfm}'";properties=ET.SubElement(project,'PropertyGroup',Condition=condition);ET.SubElement(properties,'AssetTargetFallback').text=';'.join(f.get('imports',[]));ET.SubElement(properties,'RestoreEnablePackagePruning').text='true' if f.get('packagesToPrune') else 'false';group=ET.SubElement(project,'ItemGroup',Condition=condition)
  for name,dep in f['dependencies'].items():
   if dep.get('autoReferenced'):continue
   attrs={'Include':name,'GeneratePathProperty':str(dep.get('generatePathProperty',False)).lower()}
   if not dep.get('versionCentrallyManaged'):attrs['Version']=dep['version']
   ET.SubElement(group,'PackageReference',attrs)
  for d in f.get('downloadDependencies',[]):ET.SubElement(group,'PackageDownload',Include=d['name'],Version=d['version'])
  for referenced in restore['frameworks'][tfm].get('projectReferences',{}):ET.SubElement(group,'ProjectReference',Include=str(paths[referenced]))
  versions=ET.SubElement(central,'ItemGroup',Condition=condition)
  for name,version in f.get('centralPackageVersions',{}).items():ET.SubElement(versions,'PackageVersion',Include=name,Version=version)
  # Cache-only explicit clone of evaluated pruning, avoiding SDK guesswork.
  target=ET.SubElement(project,'Target',Name='ExactPruning_'+tfm.replace('.','_'),BeforeTargets='CollectPrunePackageReferences',DependsOnTargets='AddPrunePackageReferences',Condition=condition);items=ET.SubElement(target,'ItemGroup');ET.SubElement(items,'PrunePackageReference',Remove='@(PrunePackageReference)')
  for name,version in f.get('packagesToPrune',{}).items():
   if not (version.startswith('(,') and version.endswith(']')):raise ValueError('Unexpected prune range representation')
   ET.SubElement(items,'PrunePackageReference',Include=name,Version=version[2:-1])
 ET.indent(project);ET.ElementTree(project).write(path,encoding='utf-8',xml_declaration=True);ET.indent(central);ET.ElementTree(central).write(path.parent/'Directory.Packages.props',encoding='utf-8',xml_declaration=True)
ET.indent(solution);solution_path=work/'CacheClosure.slnx';ET.ElementTree(solution).write(solution_path,encoding='utf-8',xml_declaration=True)
env=dict(os.environ,DOTNET_ROOT=str(sdk.parent),DOTNET_CLI_TELEMETRY_OPTOUT='1',MSBUILDDISABLENODEREUSE='1',TMPDIR=str(root/'.tmp/evaluation-recovery'))
bootstrap_graph=work/'bootstrap-graph.json';cmd=[str(sdk),'msbuild',str(solution_path),'-nologo','-t:GenerateRestoreGraphFile',f'-p:RestoreGraphOutputPath={bootstrap_graph}','-m:1','-v:minimal'];evaluation=measured_command(cmd,work,timeout=25,env=env,memory_limit_mib=1024)
report={'original_graph':str(graph_path),'original_graph_sha256':file_sha(graph_path),'source':feed,'graph_generation':evaluation,'bootstrap_paths_sha256':{str(p.relative_to(work)):file_sha(p) for p in work.rglob('*') if p.is_file() and 'obj' not in p.parts},'restore_requested':a.restore,'notes':['Isolated PackageSpec replicas are cache preparation only. Original source, project configuration and SDK remain unchanged.','Original central pins, dependency ranges, framework fallbacks, pruning maps, download versions and project-reference edges are compared before restore.','No repository lockfile existed; bootstrap lock/content hashes are new evidence, not pre-existing pins.']}
if evaluation['exit_code']==0:
 candidates=json.loads(bootstrap_graph.read_text())['projects'];checks={};details={}
 for source,spec in original['projects'].items():
  candidate=candidates[str(paths[source])];label=Path(source).stem;checks[label+':frameworks']=set(candidate['frameworks'])==set(spec['frameworks']);checks[label+':runtimes']=candidate.get('runtimes')==spec.get('runtimes');checks[label+':centralTransitivePinning']=candidate['restore'].get('CentralPackageTransitivePinningEnabled')==spec['restore'].get('CentralPackageTransitivePinningEnabled')
  for tfm,f in spec['frameworks'].items():
   actual=candidate['frameworks'][tfm]
   for key in ['centralPackageVersions','imports','assetTargetFallback','packagesToPrune','frameworkReferences']:
    checks[label+':'+tfm+':'+key]=actual.get(key,{})==f.get(key,{})
   deps=lambda f:{k:{x:v for x,v in dep.items() if x in ['target','version','versionCentrallyManaged','autoReferenced','suppressParent']} for k,dep in f['dependencies'].items()}
   checks[label+':'+tfm+':dependencies']=deps(actual)==deps(f)
   if deps(actual)!=deps(f):details[label+':'+tfm+':dependencies']={'actual':deps(actual),'expected':deps(f)}
   downloads=lambda f:sorted((d['name'],d['version']) for d in f.get('downloadDependencies',[]))
   checks[label+':'+tfm+':downloadDependencies']=downloads(actual)==downloads(f)
   refs=lambda p:sorted(p['restore']['frameworks'][tfm].get('projectReferences',{}))
   checks[label+':'+tfm+':projectReferences']=refs(candidate)==sorted(str(paths[n]) for n in refs(spec))
 report.update(constraint_checks=checks,constraint_details=details,constraints_match=all(checks.values()))
 if a.restore and report['constraints_match']:
  command=[str(sdk),'restore',str(solution_path),'--disable-parallel','--nologo','-m:1','-v:normal'];report['restore']=measured_command(command,work,timeout=90,env=env,memory_limit_mib=1024)
  if report['restore']['exit_code']==0:
   packages={};assets_hashes={};lock_hashes={}
   for path in paths.values():
    assets=path.parent/'obj/project.assets.json';data=json.loads(assets.read_text());assets_hashes[str(assets)]=file_sha(assets);lock_hashes[str(path.parent/'packages.lock.json')]=file_sha(path.parent/'packages.lock.json');cache=Path(next(iter(data['packageFolders'])))
    for identity,lib in data['libraries'].items():
     if lib['type']!='package':continue
     name,version=identity.rsplit('/',1);folder=cache/name.lower()/version.lower();archive=folder/(name.lower()+'.'+version.lower()+'.nupkg');digest=base64.b64encode(hashlib.sha512(archive.read_bytes()).digest()).decode() if archive.is_file() else None;meta=folder/'.nupkg.metadata';packages[identity]={'name':name,'version':version,'lock_sha512':lib.get('sha512'),'archive_sha512':digest,'hash_matches':digest==lib.get('sha512'),'metadata':json.loads(meta.read_text()) if meta.is_file() else None}
   downloads=[]
   for original_project in original['projects'].values():
    for framework in original_project['frameworks'].values():
     for item in framework.get('downloadDependencies',[]):
      name=item['name'];version=item['version'].split(',')[0].lstrip('[');folder=cache/name.lower()/version.lower();archive=folder/(name.lower()+'.'+version.lower()+'.nupkg');sha=folder/(name.lower()+'.'+version.lower()+'.nupkg.sha512');meta=folder/'.nupkg.metadata';digest=base64.b64encode(hashlib.sha512(archive.read_bytes()).digest()).decode() if archive.is_file() else None
      row={'name':name,'version':version,'archive_sha512':digest,'nuget_sha512':sha.read_text().strip() if sha.is_file() else None,'hash_matches':digest is not None and sha.is_file() and digest==sha.read_text().strip(),'metadata':json.loads(meta.read_text()) if meta.is_file() else None}
      if not any(x['name']==name and x['version']==version for x in downloads):downloads.append(row)
   report.update(cache_packages=list(packages.values()),required_downloads=downloads,cache_package_hashes_valid=all(v['hash_matches'] for v in packages.values()) and all(v['hash_matches'] for v in downloads),assets_sha256=assets_hashes,lock_sha256=lock_hashes)
output=(a.output or root/'evaluation/results/recovery-fsharp-cache-closure.json').resolve();output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'constraints_match':report.get('constraints_match'),'failed_checks':[k for k,v in report.get('constraint_checks',{}).items() if not v],'restore_requested':a.restore,'restore_exit':report.get('restore',{}).get('exit_code'),'cache_package_hashes_valid':report.get('cache_package_hashes_valid')}));print('report',output)
