#!/usr/bin/env python3
"""Observe unchanged-helper SDK100 certificate behavior without guard changes."""
import os,json,sys,argparse
from pathlib import Path
from run import Session,file_sha
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--same-host',action='store_true');args=parser.parse_args()
root=Path(__file__).resolve().parents[1];repo=root.parent/'research/corpus/Fable';scope=repo/'src/fcs-fable';sdk=root/'.tools/dotnet-sdk100/dotnet';helper=root/'evaluation/.runtime/helper-f215dac4599c/FSharpNavigator.Semantic.dll';tag='-same-host' if args.same_host else '';out=root/('evaluation/.runtime/fable-fcs-f215'+tag);out.mkdir(parents=True,exist_ok=True)
os.environ.update(DOTNET_ROOT=str(sdk.parent),FSHARP_NAVIGATOR_DOTNET=str(sdk),TMPDIR=str(root/'.tmp/evaluation-recovery'),MSBUILDDISABLENODEREUSE='1')
helper_host=sdk if args.same_host else root/'.tools/dotnet/dotnet'
s=Session([str(helper_host),str(helper)],timeout=90,memory_limit_mib=2048);report={'helper':str(helper),'helper_sha256':file_sha(helper),'helper_host':str(helper_host),'msbuild_host':str(sdk),'scope':str(scope),'project_selection':[str(scope/'fcs-fable.fsproj')]}
try:
 response,measure=s.call('workspace/evaluate',{'root':str(scope),'projects':[str(scope/'fcs-fable.fsproj')]});report.update(response=response,measurement=measure)
 if 'result' in response:
  (out/'model.json').write_text(json.dumps(response['result'],indent=2)+'\n');report['model_path']=str(out/'model.json');report['model_sha256']=file_sha(out/'model.json');report['summary']=[{'path':p['path'],'targetFramework':p['targetFramework'],'files':len(p['files']),'references':len(p['assemblyReferences']),'membershipReusable':p.get('membershipReusable'),'membershipDiagnostics':p.get('membershipDiagnostics'),'environmentCertificate':p.get('environmentCertificate')} for p in response['result']['projects']]
finally:
 s.close();report['stderr']=s.stderr;(root/('evaluation/results/fable-fcs-helper-certificate'+tag+'.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'report':'evaluation/results/fable-fcs-helper-certificate'+tag+'.json','model_path':report.get('model_path'),'summary':[{'membershipReusable':p['membershipReusable'],'diagnostics':len(p['membershipDiagnostics'])} for p in report.get('summary',[])]},indent=2))
