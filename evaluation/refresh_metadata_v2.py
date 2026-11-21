#!/usr/bin/env python3
"""Regenerate genuine selected catalogs/inventories through the schema2 CLI."""
import argparse,json,os,shutil,sys
from pathlib import Path
from prepare import measured_command
from run import file_sha
p=argparse.ArgumentParser();p.add_argument('workloads',nargs='*',default=['grace','expanded']);a=p.parse_args();root=Path(__file__).resolve().parents[1]
helper=root/'evaluation/.runtime/helper-f215dac4599c/FSharpNavigator.Semantic.dll';assert file_sha(helper)=='f215dac4599cb436d748ead64c01cf5f8795d8ddea79add66bb29cfdb6b763c2'
for name in a.workloads:
 old=root/f'evaluation/.runtime/recovery-prepared-{name}-4c9d';model=old/'model-observed-80e5.json';data=json.loads(model.read_text());out=root/f'evaluation/.runtime/recovery-prepared-{name}-f215';out.mkdir(parents=True,exist_ok=True);copy=out/'model.json';shutil.copy2(model,copy)
 owned={p['outputPath'] for p in data['projects']};owned.update(x for p in data['projects'] for x in p.get('workspaceAssemblyReferences',[]));refs=sorted({x for p in data['projects'] for x in p['assemblyReferences']});targets=[x for x in refs if x not in owned and Path(x).name in ['FSharp.Core.dll','System.Runtime.dll']];assert len(targets)>=2
 prefix=[str(root/'scripts/with-local-tools'),'node',str(root/'cli/bin/run.mjs')];common=['--root',data['root'],'--model',str(copy),'--helper',str(helper),'--timeout-ms','120000'];commands=[prefix+['libraries','export']+common+sum((['--assembly',x] for x in targets),[])+['--output',str(out/'catalog.json')],prefix+['libraries','environment']+common+['--output',str(out/'builtin-environment.json')]]
 report={'workload':name,'source_model':str(model),'source_model_sha256':file_sha(model),'model_copy':str(copy),'helper':str(helper),'helper_sha256':file_sha(helper),'selected_assemblies':targets,'phases':[],'notes':['Fresh producer calls and CLI schema2 manifests; old facts are not relabeled.','Existing genuine v3 evaluated model is copied unchanged.','Setup costs include CLI plus all owned helper descendants, exclude this controller and filesystem cache; diagnostic, not quiet comparison.']};env=dict(os.environ,TMPDIR=str(root/'.tmp/evaluation-recovery'),MSBUILDDISABLENODEREUSE='1')
 for phase,command in zip(['catalog','environment'],commands):
  result=measured_command(command,root,timeout=150,env=env,memory_limit_mib=2048);report['phases'].append({'phase':phase,**result});(root/f'evaluation/results/recovery-{name}-metadata-v2-f215-setup.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'workload':name,'phase':phase,**{k:result[k] for k in ['exit_code','elapsed_ms','peak_tree_rss_kib','output']}}),flush=True)
  if result['exit_code']:raise SystemExit(1)
 report['output_hashes']={x:file_sha(out/x) for x in ['model.json','catalog.json','builtin-environment.json']};report['bytes']={x:(out/x).stat().st_size for x in report['output_hashes']};report['source_model_sha256_after']=file_sha(model);(root/f'evaluation/results/recovery-{name}-metadata-v2-f215-setup.json').write_text(json.dumps(report,indent=2)+'\n')
