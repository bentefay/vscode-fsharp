#!/usr/bin/env python3
"""Explicit measured MSBuild/model/library preparation for an existing workspace.

No source compiler runs in subsequent indexed queries. Restore is separately
authorized by --restore and all subprocess-tree RSS/costs remain in the report.
"""
import argparse
import json
import os
import signal
from pathlib import Path
import shlex
import shutil
import subprocess
import threading
import time

from run import Session, process_tree_details, process_identity, file_sha


def measured_command(command, cwd, timeout=120, env=None, memory_limit_mib=4096):
    started=time.perf_counter()
    process=subprocess.Popen(command,cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,start_new_session=True)
    identity=process_identity(process.pid)[1];owned={process.pid:identity}
    done,samples,aborts=threading.Event(),[],[]
    def terminate_owned():
        if process.poll() is None:
            try:os.killpg(process.pid,signal.SIGKILL)
            except ProcessLookupError:pass
        for pid,starttime in list(owned.items()):
            try:
                if process_identity(pid)[1]==starttime:os.kill(pid,signal.SIGKILL)
            except (FileNotFoundError,ProcessLookupError,PermissionError):pass
    def sample():
        while not done.is_set():
            rss,pids,_,_,_=process_tree_details(process.pid,identity,owned)
            samples.append({'elapsed_ms':(time.perf_counter()-started)*1000,'rss_kib':rss,'pids':pids})
            if memory_limit_mib and rss>memory_limit_mib*1024:
                aborts.append('memory_limit');terminate_owned();break
            done.wait(.01)
    worker=threading.Thread(target=sample,daemon=True);worker.start()
    try:output,_=process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        aborts.append('timeout');terminate_owned();output,_=process.communicate(timeout=10)
    finally:
        done.set();worker.join();terminate_owned()
    return {'command':command,'elapsed_ms':(time.perf_counter()-started)*1000,
            'peak_tree_rss_kib':max((x['rss_kib'] for x in samples),default=0),'exit_code':process.returncode,
            'abort_reason':aborts[0] if aborts else None,'timeout_seconds':timeout,'memory_limit_mib':memory_limit_mib,
            'observed_process_identities':owned,'samples':samples,'output':output[-12000:]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', required=True, type=Path)
    parser.add_argument('--helper', required=True)
    parser.add_argument('--dotnet', type=Path)
    parser.add_argument('--restore', action='store_true')
    parser.add_argument('--build', action='store_true',help='Explicitly build fixture outputs needed by binary catalog/oracle experiments')
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--catalog-assemblies',default='FSharp.Core.dll,System.Runtime.dll',help='Comma-separated referenced assembly filenames, or * for all external references')
    parser.add_argument('--catalog-batch-size',type=int,default=1,help='Assemblies per independently streamed catalog shard')
    parser.add_argument('--builtin-environment',action='store_true',help='Explicitly inventory all referenced external type names and selected builtin patterns')
    parser.add_argument('--semantic-snapshot',action='store_true',help='Explicitly compile/check all source contexts into a persistent semantic occurrence snapshot')
    args = parser.parse_args()
    if not 1 <= args.catalog_batch_size <= 128:
        parser.error('--catalog-batch-size must be between 1 and 128')
    workspace = args.workspace.resolve(); out = args.output_dir.resolve(); out.mkdir(parents=True, exist_ok=True)
    started, phases = time.perf_counter(), []
    if args.restore or args.build:
        if not args.dotnet:
            parser.error('--restore requires --dotnet')
        for project in sorted(workspace.rglob('*.fsproj')):
            if any(part in {'obj','bin','node_modules'} for part in project.parts):
                continue
            operation = 'build' if args.build else 'restore'
            phase = measured_command([str(args.dotnet.resolve()), operation, str(project), '--nologo', '-m:1'], workspace)
            phases.append({'phase':operation, **phase})
            if phase['exit_code']:
                raise RuntimeError('Restore failed: ' + phase['output'])
    helper_command=shlex.split(args.helper)
    helper_hashes={}
    for index,item in enumerate(helper_command):
        if item.endswith('.dll'):
            original=Path(item).resolve();runtime=out/'helper-runtime';runtime.mkdir(exist_ok=True)
            for dependency in original.parent.iterdir():
                if dependency.is_file() and (dependency.suffix=='.dll' or dependency.name.endswith(('.deps.json','.runtimeconfig.json'))):
                    target=runtime/dependency.name;shutil.copy2(dependency,target)
                    helper_hashes[dependency.name]=file_sha(target)
            helper_command[index]=str(runtime/original.name)
    helper = Session(helper_command, timeout=120, memory_limit_mib=4096)
    try:
        response, measure = helper.call('workspace/evaluate', {'root':str(workspace)})
        phases.append({'phase':'evaluate_model', **measure, 'error':response.get('error')})
        if 'error' in response:
            raise RuntimeError(str(response['error']))
        model = response['result']; (out/'model.json').write_text(json.dumps(model,indent=2)+'\n')
        references = sorted({item for project in model['projects'] for item in project['assemblyReferences']})
        workspace_assemblies={project['outputPath'] for project in model['projects']}
        workspace_assemblies.update(item for project in model['projects'] for item in project.get('workspaceAssemblyReferences',[]))
        missing_external=[item for item in references if not Path(item).is_file() and item not in workspace_assemblies]
        if missing_external:
            raise RuntimeError('Evaluated external references are missing: '+repr(missing_external))
        available_references=[item for item in references if Path(item).is_file()]
        targets = [item for item in references if item not in workspace_assemblies and (args.catalog_assemblies=='*' or Path(item).name in set(args.catalog_assemblies.split(',')))]
        shards=[];inputs={};symbols=0
        shard_directory=out/'catalog-shards';shard_directory.mkdir(exist_ok=True)
        for first in range(0,len(targets),args.catalog_batch_size):
            shard=shard_directory/f'{len(shards):04d}.json'
            response, measure = helper.call('metadata/export', {'assemblyPaths':targets[first:first+args.catalog_batch_size],'referencePaths':available_references,'outputPath':str(shard)})
            phases.append({'phase':'library_catalog_shard', **measure, 'error':response.get('error'),
                           'export_summary':response.get('result') if response.get('result',{}).get('outputPath') else None})
            if 'error' in response:
                raise RuntimeError(str(response['error']))
            result=response['result']
            if result.get('outputPath')!=str(shard) or not shard.is_file() or result.get('completeness')!='exact':
                raise RuntimeError('Metadata helper did not confirm the exact streamed catalog shard')
            for item in result.get('inputHashes',[]):
                if item['path'] in inputs and inputs[item['path']]!=item['sha256']:
                    raise RuntimeError('Catalog input changed between shards: '+item['path'])
                inputs[item['path']]=item['sha256']
            shards.append(str(shard.relative_to(out)));symbols+=result['symbolCount']
        (out/'catalog.json').write_text(json.dumps({'schemaVersion':1,'completeness':'exact','scope':'binary-declarations-only','catalogs':shards,'inputHashes':[{'path':path,'sha256':sha} for path,sha in sorted(inputs.items())]},indent=2)+'\n')
        if args.builtin_environment:
            response,measure=helper.call('metadata/environment',{'referencePaths':available_references,'workspaceAssemblyPaths':sorted(workspace_assemblies)})
            phases.append({'phase':'builtin_environment',**measure,'error':response.get('error')})
            if 'error' in response:
                raise RuntimeError(str(response['error']))
            result=response['result']
            if result.get('completeness')!='exact' or result.get('factsVersion')!=2:
                raise RuntimeError('Environment export lacks exact version 2 facts')
            (out/'builtin-environment.json').write_text(json.dumps(result,indent=2)+'\n')
        if args.semantic_snapshot:
            response,measure=helper.call('semantic/export',{'root':str(workspace)})
            phases.append({'phase':'semantic_snapshot',**measure,'error':response.get('error')})
            if 'error' in response:
                raise RuntimeError(str(response['error']))
            (out/'semantic-snapshot.json').write_text(json.dumps(response['result'])+'\n')
    finally:
        helper.close()
        report = {'workspace':str(workspace), 'elapsed_ms':(time.perf_counter()-started)*1000, 'phases':phases,
                  'catalog_assembly_filenames':args.catalog_assemblies.split(','),
                  'builtin_environment_requested':args.builtin_environment,
                  'temporary_storage':{'path':os.environ.get('TMPDIR','/tmp'),'rss_excludes_filesystem_cache':True},
                  'helper_command':helper_command,'helper_files_sha256':helper_hashes,
                  'preparation_script_sha256':file_sha(__file__),
                  'notes':['One-time explicit setup; this cost is separate from prepared Rust queries.',
                           'Restore uses current package cache and network; no claim of empty machine package cache.',
                           'RSS includes every observed helper/MSBuild descendant; sampled at10ms.']}
        (out/'setup.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'model':str(out/'model.json'),'catalog':str(out/'catalog.json'),'setup':str(out/'setup.json'),
                      'elapsed_ms':report['elapsed_ms']}))


if __name__=='__main__':
    main()
