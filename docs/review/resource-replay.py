#!/usr/bin/env python3
"""Independent incomplete-candidate and active cancellation correctness checks."""
import argparse,hashlib,importlib.util,json,os,select,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();s=importlib.util.spec_from_file_location('native',ROOT/'docs/review/native-smoke-replay.py');n=importlib.util.module_from_spec(s);s.loader.exec_module(n)
 base=Path(tempfile.mkdtemp(prefix='resource-review-',dir=ROOT/'.tmp'));report={'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'fixtureRoot':str(base),'cases':[],'transcripts':[],'provenance':'Native correctness stress only. Unavailable budget answers are missing coverage, not speed wins. Cancellation timings are diagnostic elapsed values, not a performance comparison.'}
 body='module B\n'+''.join('let item'+str(i)+' = '+str(i)+'\n' for i in range(10000))+'let result = A.value\n';root=n.fixture(base,'budget',{'A.fs':'module A\nlet value = 1\n','B.fs':body});q={'path':str(root/'A.fs'),'line':1,'character':4,'includeDeclaration':False}
 for label,budget in [('zero',0),('retry',1500)]:
  old=os.environ.get('FSHARP_NAVIGATOR_PARSE_MS');os.environ['FSHARP_NAVIGATOR_PARSE_MS']=str(budget)
  try:d=n.Daemon(a.binary.resolve(),root)
  finally:
   if old is None:os.environ.pop('FSHARP_NAVIGATOR_PARSE_MS',None)
   else:os.environ['FSHARP_NAVIGATOR_PARSE_MS']=old
  response=d.call('navigation/references',q);stats=d.call('workspace/stats');d.close();v=response.get('result',{});exact=v.get('completeness')=='exact';expected=[n.location(root/'B.fs',10001,15,5)]
  state=('wrong-exact' if exact else 'safe-rejection') if label=='zero' else ('pass' if exact and n.normalized(v.get('locations',[]))==n.normalized(expected) else ('wrong-exact' if exact else 'unavailable-or-error'))
  report['cases'].append({'id':'parse-budget-'+label,'status':state,'response':response,'stats':stats});report['transcripts'].append(d.transcript)
 files={'A.fs':'module A\nlet value = 1\n'}
 for i in range(500):files['B'+str(i)+'.fs']='module B'+str(i)+'\nlet result = A.value\n'
 root=n.fixture(base,'cancel',files);d=n.Daemon(a.binary.resolve(),root);query={'id':2,'method':'navigation/references','params':{'path':str(root/'A.fs'),'line':1,'character':4,'includeDeclaration':False}};cancel={'id':3,'method':'$/cancelRequest','params':{'id':2}};start=time.perf_counter();d.proc.stdin.write(json.dumps(query)+'\n');d.proc.stdin.flush();time.sleep(.005);d.proc.stdin.write(json.dumps(cancel)+'\n');d.proc.stdin.flush();responses=[]
 for _ in range(2):
  assert select.select([d.proc.stdout],[],[],20)[0],'cancel timeout';responses.append(json.loads(d.proc.stdout.readline()))
 elapsed=(time.perf_counter()-start)*1000;byid={r['id']:r for r in responses};okay=byid.get(2,{}).get('error',{}).get('code')=='cancelled' and byid.get(3,{}).get('result',{}).get('cancelled') is True;report['cases'].append({'id':'active-cancellation','status':'pass' if okay else 'fail','responses':responses,'elapsedMs':elapsed});d.serial=3;stats=d.call('workspace/stats');retry=d.call('navigation/references',query['params']);expected=[n.location(root/('B'+str(i)+'.fs'),1,15,5) for i in range(500)];v=retry.get('result',{});same=v.get('completeness')=='exact' and n.normalized(v.get('locations',[]))==n.normalized(expected);report['cases'].append({'id':'cancel-complete-retry','status':'pass' if same else ('wrong-exact' if v.get('completeness')=='exact' else 'unavailable-or-error'),'statsAfterCancel':stats,'response':retry});d.close();report['transcripts'].append(d.transcript)
 report['summary']={s:sum(c['status']==s for c in report['cases']) for s in ['pass','safe-rejection','wrong-exact','unavailable-or-error','fail']};a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'summary':report['summary'],'cases':[{'id':c['id'],'status':c['status'],'elapsedMs':c.get('elapsedMs')} for c in report['cases']]},indent=2))
if __name__=='__main__':main()
