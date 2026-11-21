#!/usr/bin/env python3
"""Reuse genuine prepared Option scope fixtures against one immutable binary."""
import argparse,hashlib,importlib.util,json,os
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--prepared',type=Path,required=True);ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
 old=json.loads(a.prepared.read_text());base=Path(old['fixtureRoot']);root=base/'fixtures'
 spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);native=prep.load('native',ROOT/'docs/review/native-smoke-replay.py')
 os.environ.update(prep.ENV);d=native.Daemon(a.binary.resolve(),root)
 d.call('workspace/open',{'root':str(root),'mode':'indexed','cacheDir':str(base/'cache'),'evaluatedModelPath':old['modelPath'],'builtinEnvironmentPath':old['environmentPath']})
 rows=[]
 for c in old['cases']:
  response=d.call('navigation/definition',{'path':str(root/c['file']),**c['query']});v=response.get('result',{});exact=v.get('completeness')=='exact';same=native.normalized(v.get('locations',[]))==native.normalized(c['expectedLocations'])
  rows.append({**c,'status':'pass' if exact and same else ('wrong-exact' if exact else 'unavailable'),'response':response})
 d.close();out={**old,'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'preparedReport':str(a.prepared),'preparedReportSha256':hashlib.sha256(a.prepared.read_bytes()).hexdigest(),'cases':rows,'transcript':d.transcript,'summary':{s:sum(c['status']==s for c in rows) for s in ['pass','wrong-exact','unavailable']}}
 a.output.write_text(json.dumps(out,indent=2)+'\n');print(out['summary']);print([(c['file'],c['status']) for c in rows])
if __name__=='__main__':main()
