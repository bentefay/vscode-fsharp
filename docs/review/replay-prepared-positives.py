#!/usr/bin/env python3
"""Replay unchanged genuinely prepared positive requests without another build."""
import argparse,hashlib,importlib.util,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--prepared-report',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep);n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py');before=json.loads(a.prepared_report.read_text());report={'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'preparedReport':str(a.prepared_report),'preparedReportSha256':hashlib.sha256(a.prepared_report.read_bytes()).hexdigest(),'helperSha256':before['helperSha256'],'provenance':'Reuse unchanged genuinely built/evaluated positive proof project and matching full-term inventory. Only its positive sessions are replayed; earlier glob mutation chronology is not rerun against later filesystem state. Native engine fingerprint invalidates old algorithm cache entries.','cases':[],'sessions':[]}
 for session in before['sessions']:
  if Path(session['root']).name!='proofs':continue
  requests=[row['request'] for row in session['transcript']];r=subprocess.run([str(a.binary.resolve()),'daemon'],input=''.join(json.dumps(q)+'\n' for q in requests),env=prep.ENV,text=True,capture_output=True,timeout=30);responses=[json.loads(x) for x in r.stdout.splitlines()];byid={x.get('id'):x for x in responses};report['sessions'].append({'requests':requests,'responses':responses,'stderr':r.stderr});assert r.returncode==0,r.stderr
  for row in session['transcript']:
   q=row['request']
   if not q['method'].startswith('navigation/'):continue
   expected=row['response']['result'];actual=byid[q['id']].get('result',{});assert expected.get('completeness')=='exact',row
   same=n.normalized(actual.get('locations',[]))==n.normalized(expected.get('locations',[]))
   if q['method']=='navigation/hover':same=same and actual.get('ownership')==expected.get('ownership') and actual.get('symbol',{}).get('xmlDocId')==expected.get('symbol',{}).get('xmlDocId') and actual.get('symbol',{}).get('assemblyPath')==expected.get('symbol',{}).get('assemblyPath') and actual.get('contents')==expected.get('contents')
   exact=actual.get('completeness')=='exact';report['cases'].append({'request':q,'status':'pass' if exact and same else 'wrong-exact' if exact else 'unavailable','expected':row['response'],'actual':byid[q['id']]})
 report['summary']={s:sum(x['status']==s for x in report['cases']) for s in ['pass','wrong-exact','unavailable']};a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report['summary']))
if __name__=='__main__':main()
