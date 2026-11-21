#!/usr/bin/env python3
"""Reconstruct the fixed pre-reset Grace selection, independently of native results.

Seed, strata, six hash-ranked candidates per stratum and keyword-candidate bug
are intentionally preserved. An invalid candidate is reported, never replaced.
Without --oracle, this creates positions only: no semantic goldens are invented.
"""
import argparse,collections,hashlib,json,re,xml.etree.ElementTree as ET
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--model',type=Path);p.add_argument('--project',type=Path);p.add_argument('--oracle',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
if a.model:
 model=json.loads(a.model.read_text());root=Path(model['root']);files=sorted({Path(x) for project in model['projects'] for x in project['files'] if '/obj/' not in x and Path(x).suffix=='.fs'});context={'model_sha256':hashlib.sha256(a.model.read_bytes()).hexdigest()}
else:
 if not a.project:p.error('--model or --project is required')
 project=a.project.resolve();root=project.parent;files=sorted({(root/x.attrib['Include'].replace('\\','/')).resolve() for x in ET.parse(project).iter('Compile') if x.attrib.get('Include','').endswith('.fs')});context={'static_project_inputs_only':True,'project_sha256':hashlib.sha256(project.read_bytes()).hexdigest()}
seed='fsharp-navigator-public-grace-v1';lines={path:path.read_text().splitlines() for path in files}
patterns={'type_alias_rhs':r'\btype\s+\w+\s*=\s*(?P<token>[A-Z]\w*)\s*$', 'record_or_property_access':r'\b(?:this|[a-z]\w*)\.(?P<token>[A-Z]\w*)\b','record_creation_label':r'\{\s*(?P<token>[A-Z]\w*)\s*=','record_update_label':r'\bwith\s+(?P<token>[A-Z]\w*)\s*=','record_pattern_label':r'\blet\s+\{\s*(?P<token>[A-Z]\w*)\s*='}
def candidate(group,path,line,text,at,token):
 key=f'{seed}|{group}|{path.relative_to(root.parent)}|{line}|{at}|{token}'
 return {'stratum':group,'path':str(path),'line':line,'character':len(text[:at].encode('utf-16-le'))//2,'token':token,'selection_hash':hashlib.sha256(key.encode()).hexdigest()}
selected=[];counts={}
for group,pattern in patterns.items():
 pool=[]
 for path,content in lines.items():
  for line,text in enumerate(content):
   if text.lstrip().startswith(('//','(*')):continue
   for match in re.finditer(pattern,text):pool.append(candidate(group,path,line,text,match.start('token'),match.group('token')))
 counts[group]=len(pool);selected.extend(sorted(pool,key=lambda x:x['selection_hash'])[:6])
declarations=collections.defaultdict(set)
for path,content in lines.items():
 for line,text in enumerate(content):
  match=re.match(r'    let (?:private |internal )?(?P<token>[a-z]\w*)\b',text)
  if match:declarations[match.group('token')].add((path,line))
pool=[]
for path,content in lines.items():
 for line,text in enumerate(content):
  if text.lstrip().startswith(('//','(*')):continue
  for token,decls in declarations.items():
   if (path,line) in decls:continue
   for match in re.finditer(r"(?<![\w'])"+re.escape(token)+r"(?![\w'])",text):pool.append(candidate('module_value_use',path,line,text,match.start(),token))
counts['module_value_use']=len(pool);selected.extend(sorted(pool,key=lambda x:x['selection_hash'])[:6])
for number,q in enumerate(selected):q['name']=f'{q["stratum"]}_{number:02d}_{q["token"]}'
expected_tokens=['Guid','Guid','Guid','Guid','Sha256Hash','Guid','DeleteReason','Ranges','RepositoryId','PropertyNamingPolicy','OrdinalCount','ArtifactId','OnFailure','Kty','RepositoryId','WorkItem','Artifact','StoragePoolId','State','StoragePoolId','RecordSaves','DeletedAt','DeletedAt','CommitEnabled','referenceTypeName','mutable','serialize','error','validate','getDiscriminatedUnionFullName']
if [q['token'] for q in selected]!=expected_tokens:raise ValueError('Fixed pre-reset selection changed; do not silently benchmark a new denominator')
if counts!={'type_alias_rhs':40,'record_or_property_access':1192,'record_creation_label':54,'record_update_label':105,'record_pattern_label':0,'module_value_use':255}:raise ValueError('Fixed candidate pools changed')
if a.oracle:
 oracle=json.loads(a.oracle.read_text());oracle=oracle.get('result',oracle)
 if any(project['completeness']!='exact' for project in oracle['projects']):raise ValueError('Incomplete semantic oracle cannot supply complete reference goldens')
 occurrences=[x for project in oracle['projects'] for x in project['occurrences']];groups=collections.defaultdict(list)
 for x in occurrences:groups[x['symbolId']].append(x)
 for q in selected:
  end=q['character']+len(q['token'].encode('utf-16-le'))//2;candidates=[x for x in occurrences if x['path']==q['path'] and x['range']['start']['line']==q['line'] and x['range']['end']['line']==q['line'] and x['range']['start']['character']<=q['character'] and x['range']['end']['character']==end];identities={x['symbolId'] for x in candidates};q['oracle_symbol_count']=len(identities)
  if len(identities)==1:
   x=candidates[0];q.update(symbol_id=x['symbolId'],declaration=x['declaration'],reference_locations=[{'path':u['path'],'range':u['range']} for u in groups[x['symbolId']]],classification='source' if x['declaration'] else 'external')
  else:q.update(classification='ambiguous_or_unmatched_export',candidates=candidates)
 context['oracle_sha256']=hashlib.sha256(a.oracle.read_bytes()).hexdigest()
a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps({'seed':seed,'root':str(root),'recovery_status':'Reconstructed from preserved agent history; semantic evidence requires actual rerun.','selection':'First6SHA256-ranked syntactic candidates per fixed stratum; source-only matrix has23symbolsites/46operations after semantic confirmation;6external+1unmatched remain explicit.','strata_candidate_counts':counts,'selected':selected,**context},indent=2)+'\n');print('Reproduced fixed',len(selected),'candidate positions')
