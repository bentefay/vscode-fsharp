#!/usr/bin/env python3
"""Same-source/patch, fresh-process comparison. Does not clear filesystem caches."""
import hashlib,json,os,statistics,subprocess,sys,threading,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'evaluation'))
from run import process_tree_details,process_identity
BASE=ROOT/'.cache/decompiler-patched';RUNTIME=ROOT/'.cache/decompiler-runtime-10.0.14'
ENV=os.environ|{'DOTNET_ROOT':str(RUNTIME),'DOTNET_GCHeapHardLimit':'0x8000000','DOTNET_CLI_TELEMETRY_OPTOUT':'1'}
CASES=[('string',ROOT/'.tools/dotnet/shared/Microsoft.NETCore.App/10.0.11/System.Private.CoreLib.dll','P:System.String.Length'),('year',ROOT/'.tools/dotnet/packs/Microsoft.NETCore.App.Ref/10.0.11/ref/net10.0/System.Runtime.dll','P:System.DateTime.Year'),('package',Path.home()/'.nuget/packages/icsharpcode.decompiler/11.1.0.9782/lib/netstandard2.0/ICSharpCode.Decompiler.dll','P:ICSharpCode.Decompiler.DecompilerSettings.UsingDeclarations')]
rows=[];canonical={}
for repetition in range(3):
 for case,path,symbol in CASES+[('startup',Path(),None)]:
  order=['il','r2r','aot'];order=order[repetition:]+order[:repetition]
  for mode in order:
   folder=BASE/mode/'bin/Release/net10.0/linux-x64/publish';binary=folder/('FSharpNavigator.Decompiler'if mode=='aot'else'FSharpNavigator.Decompiler.dll')
   command=([str(binary)]if mode=='aot'else[str(RUNTIME/'dotnet'),str(binary)])+(['--once']if symbol else['--invalid'])
   payload=json.dumps({'assemblyPath':str(path),'xmlDocId':symbol})+'\n'if symbol else''
   begin=time.monotonic();p=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=ENV)
   peak=[0];retained={};stop=threading.Event();pid_start=process_identity(p.pid)[1]
   def sample():
    while not stop.is_set():
     try:peak[0]=max(peak[0],process_tree_details(p.pid,pid_start,retained)[0])
     except(OSError,ValueError):pass
     stop.wait(.01)
   worker=threading.Thread(target=sample,daemon=True);worker.start()
   try:out,err=p.communicate(payload,timeout=6)
   except BaseException:p.kill();p.wait();raise
   finally:stop.set();worker.join()
   elapsed=(time.monotonic()-begin)*1000;result=json.loads(out)
   assert p.returncode==0,(mode,p.returncode,err)
   if symbol:
    assert result['completeness']=='exact',result
    doc=result['documents'][0];loc=result['locations'][0];identity=(doc['text'],loc['range'],result['symbol']['xmlDocId'],result['symbol']['metadataToken'])
    assert canonical.setdefault(case,identity)==identity,(case,mode,'output differs')
    for dependency in result['dependencies']:
     file=Path(dependency['path']);actual=hashlib.sha256(file.read_bytes()).hexdigest()if file.is_file()else'missing'
     assert actual==dependency['sha256'],dependency
   else:assert result['completeness']=='unavailable'
   rows.append({'variant':mode,'case':case,'repetition':repetition,'elapsed_ms':elapsed,'peak_tree_rss_kib':peak[0],'result':result})
summary=[]
for mode in ('il','r2r','aot'):
 for case,_,_ in CASES+[('startup',None,None)]:
  selected=[r for r in rows if r['variant']==mode and r['case']==case]
  summary.append({'variant':mode,'case':case,'median_ms':statistics.median(r['elapsed_ms']for r in selected),'min_ms':min(r['elapsed_ms']for r in selected),'max_ms':max(r['elapsed_ms']for r in selected),'max_tree_rss_mib':max(r['peak_tree_rss_kib']for r in selected)/1024})
report={'kind':'controlled-isolated-helper-comparison','runtime':'10.0.14','sdk':'10.0.400','source_sha256':hashlib.sha256((BASE/'aot/Program.cs').read_bytes()).hexdigest(),'sampling':'10ms PPid/PID-starttime process-tree RSS; excludes Python controller','fresh_process_each_request':True,'filesystem_caches':'not cleared; warmed by prior correctness tests','order':'three rotations per case','target_input_runtime':'10.0.11 fixed assembly inputs across variants','document_range_and_target_identity_equal':True,'all_dependency_hashes_verified':True,'startup_case':'invalid-command JSON response; no decompilation, lower-bound process/protocol startup','summary':summary,'rows':rows}
path=Path(__file__).parent/'comparison-10.0.14.json';path.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'report':str(path),'summary':summary},indent=2))
