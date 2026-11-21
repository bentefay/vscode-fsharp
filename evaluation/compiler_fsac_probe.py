#!/usr/bin/env python3
"""Bounded actual Ionide-shipped FSAC project-load feasibility, no readiness claim."""
import json,os,sys,time
from pathlib import Path
from run import Session,file_sha
root=Path(__file__).resolve().parents[1];repo=root.parent/'research/corpus/fsharp';trace=root/'evaluation/results/recovery-fsharp-fsac-load.lsp.jsonl';cache=root/'evaluation/.runtime/compiler-fsac-load';cache.mkdir(parents=True,exist_ok=True)
os.environ.update(DotnetProjInfo='true',IgnoreMibc='true',MSBUILDDISABLENODEREUSE='1',TMPDIR=str(root/'.tmp/evaluation-recovery'))
command=[sys.executable,str(root/'evaluation/fsac_adapter.py'),'--dotnet',str(repo/'.dotnet/dotnet'),'--ionide',str(root/'evaluation/.runtime/extensions/Ionide.Ionide-fsharp-8.1.0'),'--timeout','45','--trace',str(trace)]
session=Session(command,timeout=50,memory_limit_mib=3072);report={'kind':'actual-Ionide-FSAC-compiler-project-load-feasibility','runtime_note':'Shipped FSAC Major roll-forward chooses installed SDK11 runtime11; distinct from earlier Grace runtime10 baseline. No runtimeconfig/project/SDK substitutions.','command':command,'adapter_sha256':file_sha(root/'evaluation/fsac_adapter.py'),'phases':[],'performance_acceptance':False}
try:
 response,measure=session.call('workspace/open',{'root':str(repo/'src/Compiler'),'cacheDir':str(cache)});report['phases'].append({'phase':'workspace_open','response':response,'measurement':measure})
finally:
 session.close();report['stderr']=session.stderr;report['tree_peak_rss_kib']=max((x[1] for x in session.samples),default=0);report['samples']=session.detailed_samples;report['trace_sha256']=file_sha(trace)
 if trace.is_file():
  events=[json.loads(x) for x in trace.read_text().splitlines()];report['server_notifications']=[x for x in events if x['direction']=='server' and 'method' in x['message'] and x['message']['method'] not in ['window/logMessage','$/progress']];report['server_error_log_tail']=[x for x in events if x['direction']=='server' and x['message'].get('method')=='window/logMessage'][-20:]
 out=root/'evaluation/results/recovery-fsharp-fsac-load.json';out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'report':str(out),'response':report['phases'],'peak_tree_rss_kib':report['tree_peak_rss_kib'],'stderr':report['stderr'][-10:]}),flush=True)
