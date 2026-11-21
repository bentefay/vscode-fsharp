#!/usr/bin/env python3
"""Replay the fixed sample; semantic goldens and capture-only evidence stay separate."""
import argparse,json,shlex
from types import SimpleNamespace
from pathlib import Path
from run import Session,canonical_location,file_sha,snapshot_engine,source_sha,numeric_delta
from real_trace import equivalent_locations
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--selection',type=Path,required=True);p.add_argument('--engine',required=True);p.add_argument('--engine-sha');p.add_argument('--model',type=Path);p.add_argument('--catalog',type=Path);p.add_argument('--builtin-environment',type=Path);p.add_argument('--project-path',type=Path);p.add_argument('--decompiler');p.add_argument('--output',type=Path,required=True);p.add_argument('--definitions-only',action='store_true');p.add_argument('--capture-only',action='store_true',help='Capture responses for historical source sites; no semantic correctness claims');p.add_argument('--timeout',type=float,default=120);p.add_argument('--trace-references',action='store_true',help='Capture bounded native proof gaps without changing reference expectations');a=p.parse_args()
selection=json.loads(a.selection.read_text());root=Path(selection['root']);rows=[];historical_source={4,6,7,8,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,26,27,28,29}
chosen=[q for i,q in enumerate(selection['selected']) if (i in historical_source if a.capture_only else q.get('classification')=='source')]
if len(chosen)!=23:raise ValueError('Fixed source denominator changed; expected23sites/46operations')
work=a.output.resolve().parent/(a.output.stem+'-work');work.mkdir(parents=True,exist_ok=True)
snapshot_args=SimpleNamespace(engine=a.engine,engine_sha=a.engine_sha,helper=None,evaluated_model=a.model,library_catalog=a.catalog,builtin_environment=a.builtin_environment,semantic_snapshot=None,decompiler=a.decompiler,manifest=a.selection)
provenance=snapshot_engine(snapshot_args,work);a.engine=snapshot_args.engine
report={'selection_sha256':file_sha(a.selection),'selection':str(a.selection),'command':shlex.split(a.engine),'provenance':provenance,'source_files_sha256':{q['path']:file_sha(q['path']) for q in chosen},'rows':rows,'capture_only':a.capture_only,'excluded_candidates':[q for q in selection['selected'] if q not in chosen],'recovery_status':'Actual reconstructed post-reset rerun','notes':['Fixed23historicalsource sites;6external+1unmatched retained separately. Capture-only mode does not assert semantic correctness.','Complete source-reference goldens require directFCS or validated snapshot; alias erasure overmerging must be corrected from direct uses.','Source-verified receiver/token range alternatives, same exact duplicate normalization as pinnedVSCodeUI.','Shared-machine diagnostic until explicitly reviewed frozenquietrun.']}
report['trace_references']=a.trace_references
report['prepared_inputs']={name:{'path':str(path.resolve()),'sha256':file_sha(path)} for name,path in [('model',a.model),('catalog',a.catalog),('builtin_environment',a.builtin_environment)] if path};report['selected_project_path']=str(a.project_path.resolve()) if a.project_path else None
s=Session(shlex.split(a.engine),timeout=a.timeout,memory_limit_mib=4096)
def save():a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(report,indent=2)+'\n')
try:
 params={'root':str(root),'mode':'indexed','cacheDir':str(a.output.resolve().parent/(a.output.stem+'-cache'))}
 if a.model:params['evaluatedModelPath']=str(a.model.resolve())
 if a.catalog:params['metadataPath']=str(a.catalog.resolve())
 if a.builtin_environment:params['builtinEnvironmentPath']=str(a.builtin_environment.resolve())
 if a.decompiler:
  command=shlex.split(a.decompiler);params.update(decompilerPath=command[0],decompilerArgs=command[1:])
 r,m=s.call('workspace/open',params);rows.append({'phase':'startup','response':r,**m});save();opened=set()
 for q in chosen:
  if q['path'] not in opened:s.call('document/open',{'path':q['path'],'text':Path(q['path']).read_text(),'version':1});opened.add(q['path'])
  for method in ['definition'] if a.definitions_only else ['definition','references']:
   params={k:q[k] for k in ['path','line','character']};params['includeDeclaration']=True
   if a.project_path:params['projectPath']=str(a.project_path.resolve())
   if a.trace_references and method=='references':params['traceReferences']=True
   before=s.stats();r,m=s.call('navigation/'+method,params);after=s.stats();actual=sorted(set(canonical_location(x,root) for x in r.get('result',{}).get('locations',[])));exact='error' not in r and r.get('result',{}).get('completeness')=='exact';expected=None;correct=None
   if not a.capture_only:
    expected=sorted(set(canonical_location(x,root) for x in ([q['declaration']] if method=='definition' else q['reference_locations'])));correct=exact and (equivalent_locations(actual,expected,root) if method=='references' else actual==expected)
   rows.append({'phase':'query','name':q['name'],'stratum':q['stratum'],'method':method,'params':params,'response':r,'stats_before':before,'stats_after':after,'counter_delta':numeric_delta(before,after),'raw_location_count':len(r.get('result',{}).get('locations',[])),'normalized_location_count':len(actual),'actual':actual,'expected':expected,'correct':correct,'complete':exact,**m});save();print(q['name'],method,correct,str(r.get('error') or r.get('result',{}).get('completeness'))[:200],flush=True)
 r,m=s.call('workspace/stats',{});report['final_stats']={'response':r,**m}
finally:
 s.close();report['source_files_sha256_after']={path:file_sha(path) for path in report['source_files_sha256']};report['provenance']['source_sha256_after']=source_sha();report['prepared_inputs_sha256_after']={name:file_sha(item['path']) for name,item in report['prepared_inputs'].items()};save()
