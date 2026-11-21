#!/usr/bin/env python3
"""Ensure cached workspace targets retain external/builtin proof dependencies."""
import argparse,hashlib,importlib.util,json,os,shutil,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--prepared-report',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 spec=importlib.util.spec_from_file_location('native',ROOT/'docs/review/native-smoke-replay.py');n=importlib.util.module_from_spec(spec);spec.loader.exec_module(n)
 previous=json.loads(a.prepared_report.read_text());base=Path(previous['fixtureRoot']);root=base/'proofs';model=base/'model.json';source=(root/'Mixed.fs').read_text();out={'binarySha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'preparedReport':str(a.prepared_report),'cases':[],'sessions':[],'provenance':'Fresh immutable daemon replay using copied artifacts only. Positive exact source target is required before mutation; same-size artifact mutation restores mtime. Safe rejection does not count as refreshed navigation coverage.'}
 def check(name,response,expected=None,safety=False):
  result=response.get('result',{});exact=result.get('completeness')=='exact'
  same=expected is not None and n.normalized(result.get('locations',[]))==n.normalized(expected)
  status=('wrong-exact' if exact else 'safe-rejection') if safety else ('pass' if exact and same else ('wrong-exact' if exact else 'unavailable-or-error'))
  out['cases'].append({'id':name,'status':status,'response':response,'expectedLocations':expected})
  return exact and same
 def finish(d):
  d.close();out['sessions'].append({'transcript':d.transcript,'stderr':d.stderr})
 for mode,line in [('metadata',2),('builtin',3)]:
  folder=Path(tempfile.mkdtemp(prefix='mixed-'+mode+'-',dir=ROOT/'.tmp'))
  catalog=ROOT/'evaluation/.runtime/recovery-prepared-expanded-4c9d/catalog.json';copied=folder/'catalog.json';shutil.copy2(catalog,copied);shutil.copytree(catalog.parent/'catalog-shards',folder/'catalog-shards')
  environment=folder/'environment.json';shutil.copy2(base/'builtin-environment.json',environment)
  def opened():
   d=n.Daemon(a.binary.resolve(),root);d.call('workspace/open',{'root':str(root),'mode':'indexed','cacheDir':str(folder/'cache'),'evaluatedModelPath':str(model),'metadataPath':str(copied),'builtinEnvironmentPath':str(environment)});return d
  q={'path':str(root/'Mixed.fs'),'line':line,'character':source.splitlines()[line].rindex('Name')};expected=[n.location(root/'Mixed.fs',1,16,4)]
  d=opened();positive=check(mode+'-positive',d.call('navigation/definition',q),expected)
  check(mode+'-warm-positive',d.call('navigation/definition',q),expected)
  if not positive:
   finish(d);continue
  target=copied if mode=='metadata' else environment;text=target.read_text();data=json.loads(text);old=data['inputHashes'][0]['sha256'];assert old!='missing' and len(old)==64;changed=text.replace(old,'0'*64,1);assert len(changed.encode())==len(text.encode());stat=target.stat();target.write_text(changed);os.utime(target,ns=(stat.st_atime_ns,stat.st_mtime_ns))
  check(mode+'-same-session-stale-proof',d.call('navigation/definition',q),safety=True);finish(d)
  d=opened();check(mode+'-restart-stale-proof',d.call('navigation/definition',q),safety=True);finish(d)
 out['summary']={s:sum(c['status']==s for c in out['cases']) for s in ('pass','wrong-exact','unavailable-or-error','safe-rejection')};a.output.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({'summary':out['summary'],'nonpassing':[c for c in out['cases'] if c['status'] not in ['pass','safe-rejection']]},indent=2))
if __name__=='__main__':main()
