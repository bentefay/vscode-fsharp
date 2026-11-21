#!/usr/bin/env python3
import argparse,hashlib,importlib.util,json,os,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--manifest',type=Path,default=ROOT/'docs/review/ordered-nominal-prepared.json');p.add_argument('--output',type=Path,required=True);a=p.parse_args();manifest=json.loads(a.manifest.read_text());os.environ.update(manifest['environmentOverrides'])
spec=importlib.util.spec_from_file_location('native',ROOT/'docs/review/native-smoke-replay.py');n=importlib.util.module_from_spec(spec);spec.loader.exec_module(n)
root=Path(manifest['root']);base=Path(tempfile.mkdtemp(prefix='ordered-nominal-replay-',dir=ROOT/'.tmp'));oracle={r['name']:r for r in manifest['oracle']};rows=[];sessions=[]
def session(envpath):
 d=n.Daemon(a.binary.resolve(),root);params=dict(root=str(root),mode='indexed',cacheDir=str(base/('cache-'+str(len(sessions)))),evaluatedModelPath=manifest['modelPath'])
 if envpath:params['builtinEnvironmentPath']=str(envpath)
 d.call('workspace/open',params);return d
for phase,envpath in [('full',manifest['environmentPath']),('missing',None)]:
 d=session(envpath)
 for q in manifest['queries']:
  if phase=='missing' and q['name'] not in ('Absent','Caller','Caller-generic'):continue
  response=d.call('navigation/definition',{k:v for k,v in q.items() if k in ('path','line','character')});symbol=oracle[q['name']]['symbol'];r=response.get('result',{});exact=r.get('completeness')=='exact';expected=[]
  if not symbol['assembly']:
   loc=symbol['declaration'];declpath=Path(loc['path']);declpath=declpath if declpath.is_absolute() else root/declpath;expected=[n.location(declpath,loc['line'],loc['character'],loc['endCharacter']-loc['character'])];same=n.normalized(r.get('locations',[]))==n.normalized(expected);status='exact-correct' if exact and same else 'wrong-exact' if exact else 'unavailable'
  else:
   source_result=any(str(x.get('uri','')).startswith(root.as_uri()+'/') for x in r.get('locations',[]));status='wrong-exact-source-owner' if exact and source_result else 'external-target-review' if exact else 'unavailable'
  rows.append(dict(phase=phase,name=q['name'],oracle=oracle[q['name']],expectedLocations=expected,response=response,status=status));print(phase,q['name'],status)
 d.close();sessions.append(d.transcript)
report=dict(binary=str(a.binary.resolve()),binarySha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),manifestPath=str(a.manifest),manifestSha256=hashlib.sha256(a.manifest.read_bytes()).hexdigest(),cases=rows,sessions=sessions,provenance='Genuine built project, observed-v4 model and complete schema2 environment. Actual direct FCS declarations establish source versus external owner. Missing inventory is a safety control, never positive coverage; no source snapshot supplied to native.')
a.output.write_text(json.dumps(report,indent=2)+'\n')
