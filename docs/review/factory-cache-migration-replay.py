#!/usr/bin/env python3
import argparse, hashlib, importlib.util, json, tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
old=ROOT/'evaluation/.runtime/recovered-858f9d9a16d3/fsharp-navigator'
source=json.loads((ROOT/'docs/review/factory-trait-native-source-858f.json').read_text())['source'];bad='module Main\n'+source;good=bad.replace('        let ignored = Helpers.force<Factory>()\n','')
spec=importlib.util.spec_from_file_location('smoke',ROOT/'docs/review/native-smoke-replay.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
base=Path(tempfile.mkdtemp(prefix='factory-cache-migration-',dir=ROOT/'.tmp'));root=m.fixture(base,'project',{'Main.fs':bad});path=root/'Main.fs';rows=[]
def query(d,text,version=None):
 lines=text.splitlines();line=next(i for i,s in enumerate(lines) if '{ Name = "x" }' in s);args=dict(path=str(path),line=line,character=lines[line].index('Name'))
 if version is not None:args['version']=version
 return d.call('navigation/definition',args)
def refs(d,text,version=None):
 args=dict(path=str(path),line=1,character=15,includeDeclaration=True)
 if version is not None:args['version']=version
 return d.call('navigation/references',args)
def save(d):
 d.close();return d.transcript
old_d=m.Daemon(old,root);rows.append(dict(case='old-wrong-exact-seed',response=query(old_d,bad)));rows.append(dict(case='old-warm-wrong-seed',response=query(old_d,bad)));old_transcript=save(old_d)
d=m.Daemon(a.binary.resolve(),root);rows.append(dict(case='new-reopened-same-cache',response=query(d,bad)));rows.append(dict(case='new-reopened-reference-completeness',response=refs(d,bad)))
d.call('document/open',dict(path=str(path),text=good,version=1));rows.append(dict(case='new-dirty-positive',response=query(d,good,1)))
d.call('document/change',dict(path=str(path),text=bad,version=2));rows.append(dict(case='new-dirty-constrained',response=query(d,bad,2)));rows.append(dict(case='new-dirty-reference-completeness',response=refs(d,bad,2)))
new_transcript=save(d)
expected_bad=m.location(path,1,15,4);expected_good=m.location(path,6,6,4)
for row in rows:
 expected=expected_good if row['case']=='new-dirty-positive' else expected_bad;expected_list=[expected]
 if 'reference-completeness' in row['case']: expected_list.append(m.location(path,9,10,4))
 row['expectedLocations']=expected_list;r=row['response'].get('result',{});exact=r.get('completeness')=='exact';same=m.normalized(r.get('locations',[]))==m.normalized(expected_list);row['status']='exact-correct' if exact and same else 'wrong-exact' if exact else 'unavailable'
 print(row['case'],row['status'])
report=dict(binary=str(a.binary.resolve()),binarySha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),baselineSha256=hashlib.sha256(old.read_bytes()).hexdigest(),fixtureRoot=str(root),source=bad,positiveOverlay=good,cases=rows,oldTranscript=old_transcript,newTranscript=new_transcript,provenance='Independent cache migration and versioned edit replay. Old wrong-exact seed is required; refusal is safety only. Plain literal overlay must remain exact with version1, constrained edit must not return wrong exact with version2.')
a.output.write_text(json.dumps(report,indent=2)+'\n')
