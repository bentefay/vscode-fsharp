#!/usr/bin/env python3
"""Replay alias occurrence navigation and refs separately against actual FSAC."""
import argparse,hashlib,importlib.util,json,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 parser=argparse.ArgumentParser();parser.add_argument('binary',type=Path);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
 loader=importlib.util.spec_from_file_location('native_replay',ROOT/'docs/review/native-smoke-replay.py');mod=importlib.util.module_from_spec(loader);loader.loader.exec_module(mod)
 baseline_path=ROOT/'evaluation/results/recovery-alias-routing-fsac.json';baseline=json.loads(baseline_path.read_text());source=baseline['source']
 base=Path(tempfile.mkdtemp(prefix='review-alias-',dir=ROOT/'.tmp'));root=mod.fixture(base,'project',{'Aliases.fs':source});daemon=mod.Daemon(args.binary.resolve(),root);rows=[]
 for oracle in baseline['rows']:
  if oracle['params'].get('includeDeclaration',True) is False:continue
  params={**oracle['params'],'path':str(root/'Aliases.fs')};response=daemon.call('navigation/'+oracle['method'],params)
  expected=oracle['response']['result'];actual=response.get('result',{})
  def normalize(locations):
   return sorted(set(json.dumps({'uri':'SOURCE:Aliases.fs' if x['uri'].endswith('/Aliases.fs') else x['uri'],'range':x['range']},sort_keys=True) for x in locations))
  status='unavailable' if actual.get('completeness')!='exact' else ('pass' if normalize(actual.get('locations',[]))==normalize(expected.get('locations',[])) else 'wrong-exact')
  rows.append({'id':oracle['id'],'method':oracle['method'],'params':params,'response':response,'oracle':expected,'status':status})
 daemon.close();report={'provenance':'Independent immutable native replay against fresh actualFSAC. References and goto targets are distinct assertions. Baseline includeDeclaration=false behavior retained in raw oracle, not silently normalized here.','binarySha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'oraclePath':str(baseline_path),'oracleSha256':hashlib.sha256(baseline_path.read_bytes()).hexdigest(),'source':source,'fixtureRoot':str(root),'rows':rows,'summary':{s:sum(x['status']==s for x in rows) for s in ('pass','unavailable','wrong-exact')}}
 args.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report['summary']))
if __name__=='__main__':main()
