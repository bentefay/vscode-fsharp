#!/usr/bin/env python3
"""Inspect positive reference sites and a lexical candidate superset; no coverage score changes."""
import argparse,collections,json,re,shlex
from pathlib import Path
from types import SimpleNamespace
from run import Session,canonical_location,file_sha,snapshot_engine
NAMES=['type_alias_rhs_04_Sha256Hash','record_or_property_access_07_Ranges','record_or_property_access_11_ArtifactId','record_creation_label_14_RepositoryId']
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--selection',type=Path,required=True);p.add_argument('--reference-report',type=Path,required=True);p.add_argument('--engine',required=True);p.add_argument('--engine-sha',required=True);p.add_argument('--model',type=Path,required=True);p.add_argument('--catalog',type=Path,required=True);p.add_argument('--builtin-environment',type=Path,required=True);p.add_argument('--project-path',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
selection=json.loads(a.selection.read_text());root=Path(selection['root']);model=json.loads(a.model.read_text());project=next(p for p in model['projects'] if Path(p['path']).resolve()==a.project_path.resolve());files=project['files'];texts={path:Path(path).read_text() for path in files};lines={path:text.splitlines() for path,text in texts.items()};baseline=json.loads(a.reference_report.read_text());groups=[]
work=a.output.resolve().parent/(a.output.stem+'-work');work.mkdir(parents=True,exist_ok=True)
args=SimpleNamespace(engine=a.engine,engine_sha=a.engine_sha,helper=None,evaluated_model=a.model,library_catalog=a.catalog,builtin_environment=a.builtin_environment,semantic_snapshot=None,decompiler=None,manifest=a.selection);provenance=snapshot_engine(args,work)
report={'provenance':provenance,'selection_sha256':file_sha(a.selection),'reference_report_sha256':file_sha(a.reference_report),'groups':groups,'notes':['Diagnostic only; does not alter fixed46queries or goldens.','All expected sites derive from independently validated compiler reference sets.','Other raw same-name candidates are a regex superset including comments and strings, not the native lexer candidate list; unavailable raw candidates do not alone prove why native references are incomplete.','No public proof/debug endpoint exists in this candidate. Definition probes can differ from logical reference membership, especially alias constructors that navigate to String implementation.','A partial unavailable reference response is not a complete/correct result even when it currently contains every golden location.']}
def save():a.output.write_text(json.dumps(report,indent=2)+'\n')
def key(loc):
 path=str(Path(loc.get('path') or loc['uri'].removeprefix('file://')).resolve());r=loc['range'];return(path,r['end']['line'],r['end']['character'])
def context(path,line):return [{'line':n+1,'text':lines[path][n]} for n in range(max(0,line-2),min(len(lines[path]),line+3))]
def family(q,path,line,start,end):
 text=lines[path][line];before=text.encode('utf-16-le')[:start*2].decode('utf-16-le');after=text.encode('utf-16-le')[end*2:].decode('utf-16-le')
 if text.lstrip().startswith('//'):return 'comment-text-candidate'
 if q['name'].startswith('type_alias'):
  if 'nameof' in before:return 'nameof-type-expression'
  if re.search(r'\btype\s+[\w\']+\s*=\s*$',before):return 'alias-rhs'
  if re.match(r'\s+(?:String\.|this\.)',after):return 'alias-constructor-expression'
  if ':' in before:return 'type-annotation-or-union-payload'
  if re.search(r'\btype\s*$',before):return 'type-declaration'
  return 'same-spelling-non-type-candidate'
 if re.search(r'\.\s*$',before):return 'receiver-property-access'
 if re.match(r'\s*:',after):return 'field-or-type-annotation'
 if re.match(r'\s*=',after):return 'record-construction-or-update-label'
 return 'other-same-name-token'
s=Session(shlex.split(args.engine),timeout=120,memory_limit_mib=4096);opened=set()
try:
 response,measure=s.call('workspace/open',{'root':str(root),'mode':'indexed','cacheDir':str(work/'cache'),'evaluatedModelPath':str(a.model.resolve()),'metadataPath':str(a.catalog.resolve()),'builtinEnvironmentPath':str(a.builtin_environment.resolve())});report['open_response']=response
 for name in NAMES:
  q=next(q for q in selection['selected'] if q['name']==name);token=q['token'];expected={key(loc):loc for loc in q['reference_locations']};prior=next(row for row in baseline['rows'] if row.get('name')==name and row.get('method')=='references');partial={key(loc) for loc in prior['response'].get('result',{}).get('locations',[])};origin=q['declaration']['path'];visible=files[files.index(origin):]
  candidates={}
  for path in visible:
   for line,text in enumerate(lines[path]):
    for m in re.finditer(r'(?<![\w\'])'+re.escape(token)+r'(?![\w\'])',text):
     start=len(text[:m.start()].encode('utf-16-le'))//2;end=start+len(token.encode('utf-16-le'))//2;candidates[(path,line,end)]=(start,end)
  # Compiler-known locations always remain, even if qualified ranges differ.
  for k,loc in expected.items():candidates.setdefault(k,(k[2]-len(token.encode('utf-16-le'))//2,k[2]))
  group={'name':name,'token':token,'expected_count':len(expected),'partial_native_count':len(partial),'partial_response':prior['response'],'lexical_candidate_count':len(candidates),'sites':[]};groups.append(group);save()
  for k,(start,end) in sorted(candidates.items()):
   path,line,_=k
   if path not in opened:s.call('document/open',{'path':path,'text':texts[path],'version':1});opened.add(path)
   params={'path':path,'line':line,'character':start,'projectPath':str(a.project_path.resolve()),'includeDeclaration':True};response,measure=s.call('navigation/definition',params);result=response.get('result',{});exact=result.get('completeness')=='exact' and 'error' not in response;actual=sorted(set(canonical_location(loc,root) for loc in result.get('locations',[])));decl=canonical_location(q['declaration'],root);kind=family(q,path,line,start,end)
   if not exact:outcome='unavailable'
   elif actual==[decl]:outcome='same-source-definition'
   else:outcome='other-definition-routing' if q['name'].startswith('type_alias') else 'other-source-or-external-definition'
   group['sites'].append({'path':path,'line':line,'character':start,'end_character':end,'expected_reference':k in expected,'included_in_partial_references':k in partial,'syntax_family_heuristic':kind,'context':context(path,line),'definition_probe':{'params':params,'response':response,'outcome':outcome},'diagnostic_latency_ms':measure['latency_ms']});save()
  group['summary']={'positive_definition_outcomes':dict(collections.Counter(site['definition_probe']['outcome'] for site in group['sites'] if site['expected_reference'])),'missing_positive_families':dict(collections.Counter(site['syntax_family_heuristic'] for site in group['sites'] if site['expected_reference'] and not site['included_in_partial_references'])),'other_candidate_outcomes':dict(collections.Counter(site['definition_probe']['outcome'] for site in group['sites'] if not site['expected_reference']))};save();print(name,json.dumps(group['summary']),flush=True)
 report['final_stats']=s.stats()
finally:
 s.close();report['source_hashes_after']={path:file_sha(path) for path in texts};save()
