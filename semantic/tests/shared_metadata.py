#!/usr/bin/env python3
"""Experimental shared binary import: complete output parity and abort controls.

Correctness only; concurrent work and Python JSON comparisons are not a memory
or latency benchmark. Every mutable dependency below is a disposable copy.
"""
import argparse,hashlib,json,os,pathlib,shutil,subprocess,tempfile,time
from smoke import ROOT,DOTNET,ENV

def sha(path):return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--helper',required=True);parser.add_argument('--model',required=True);parser.add_argument('--control-helper');args=parser.parse_args()
 helper=pathlib.Path(args.helper).resolve();control=pathlib.Path(args.control_helper).resolve()if args.control_helper else helper
 model=json.loads(pathlib.Path(args.model).read_text());refs=sorted({p for project in model['projects']for p in project['assemblyReferences']if pathlib.Path(p).is_file()});owned=sorted({p for project in model['projects']for p in project['workspaceAssemblyReferences']})
 assemblies=[next(p for p in refs if pathlib.Path(p).name==name)for name in ['FSharp.Core.dll','System.Runtime.dll']]
 root=pathlib.Path(tempfile.mkdtemp(prefix='shared-metadata-',dir=ROOT/'.cache'));records=[]
 def call(binary,method,params):
  process=subprocess.run([str(DOTNET),str(binary),'--once'],env=ENV,input=json.dumps({'id':1,'method':method,'params':params})+'\n',text=True,capture_output=True,timeout=150)
  assert process.returncode==0,process.stderr
  reply=json.loads(process.stdout);assert 'error'not in reply,reply
  return reply['result']
 for i,assembly in enumerate(assemblies):call(control,'metadata/export',{'assemblyPaths':[assembly],'referencePaths':refs,'outputPath':str(root/f'control-{i}.json')})
 old_env=call(control,'metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});(root/'control-environment.json').write_text(json.dumps(old_env))
 output=root/'shared';result=call(helper,'metadata/prepare-experimental',{'referencePaths':refs,'workspaceAssemblyPaths':owned,'assemblyPaths':assemblies,'outputDirectory':str(output),'maxWorkingSetMb':768})
 assert result['importCount']==1 and result['sourceProjectChecks']==0 and result['catalogShards']==2
 def semantic(value):
  value=json.loads(json.dumps(value));value['producer']['implementationHashes']=[v for v in value['producer']['implementationHashes']if v['role']!='exporter']
  def normalize(node):
   if isinstance(node,list):return [normalize(item)for item in node]
   if isinstance(node,dict):return {key:sorted(item)if key=='declaredMemberNames'else normalize(item)for key,item in node.items()}
   return node
  value=normalize(value)
  if 'symbols'in value:
   # FCS enumerates some members through hash-backed collections. Preserve
   # multiplicity, and also compare the actual first-row-wins SQLite projection;
   # sorting a multiset alone would conceal an order-dependent duplicate.
   projected={}
   for symbol in value['symbols']:
    for name in set(symbol.get('qualifiedNames',[])+[symbol['fullName']]):projected.setdefault((name,symbol['id']),symbol)
   value['_importedRows']=[[name,identifier,symbol]for(name,identifier),symbol in sorted(projected.items())]
   value['symbols']=sorted(value['symbols'],key=lambda item:json.dumps(item,sort_keys=True))
  return value
 for i in range(2):
  old=json.loads((root/f'control-{i}.json').read_text());new=json.loads((output/f'catalogs/{i:04}.json').read_text())
  assert semantic(old)==semantic(new),f'Catalog{i} differs beyond exporter provenance'
  records.append({'case':f'complete_catalog_{i}','symbols':len(new['symbols']),'passed':True})
 new_env=json.loads((output/'builtins.json').read_text());assert semantic(old_env)==semantic(new_env),'Complete environment differs beyond exporter provenance'
 records.append({'case':'complete_type_scope_term_inventory','typeNames':len(new_env['typeNames']),'scopeNames':len(new_env['scopeNames']),'termNames':len(new_env['termNames']),'passed':True})
 # Copy the real tiny binary/XML role fixture; never change shared SDK files.
 provenance=json.loads((ROOT/'test-results/recovery/metadata-provenance.json').read_text());source=pathlib.Path(provenance['fixtureRoot'])/'bin/Debug/net10.0/Library.dll'
 for mode in ['xml-bytes','higher-xml-created','assembly-bytes']:
  directory=root/mode;directory.mkdir();binary=directory/'Library.dll';shutil.copy2(source,binary);xml=binary.with_suffix('.xml');shutil.copy2(source.with_suffix('.xml'),xml)
  if mode=='higher-xml-created':xml.rename(binary.with_suffix('.XML'))
  job_output=directory/'generation';job_refs=[p for p in refs if pathlib.Path(p).name!='Library.dll']+[str(binary)]
  params={'referencePaths':job_refs,'workspaceAssemblyPaths':owned,'assemblyPaths':[str(binary),assemblies[1]],'outputDirectory':str(job_output),'maxWorkingSetMb':768}
  worker=subprocess.Popen([str(DOTNET),str(helper)],env=ENV,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  worker.stdin.write(json.dumps({'id':1,'method':'metadata/prepare-experimental','params':params})+'\n');worker.stdin.flush()
  changed=False;deadline=time.monotonic()+90
  while worker.poll()is None and time.monotonic()<deadline:
   if list(directory.glob('.binary-job-*/catalogs/0000.json')):
    if mode=='assembly-bytes':binary.write_bytes(binary.read_bytes()+b'changed')
    else:xml.write_text('<doc><members><member name="P:Provenance.Library.keep"><summary>Changed</summary></member></members></doc>')
    changed=True;break
   time.sleep(.01)
  assert changed,mode
  stdout,stderr=worker.communicate(timeout=90);assert worker.returncode==0,stderr
  reply=json.loads(stdout);assert 'error'in reply,(mode,reply);assert not job_output.exists(),mode
  # Restoring the input and rebuilding must recover a complete generation.
  shutil.copy2(source,binary)
  if mode=='higher-xml-created' and xml.exists():xml.unlink()
  elif mode=='xml-bytes':shutil.copy2(source.with_suffix('.xml'),xml)
  recovered=call(helper,'metadata/prepare-experimental',params);assert recovered['catalogShards']==2
  records.append({'case':mode,'rejected':reply['error'],'recovered':True,'passed':True})
 # A sampled memory rejection exits the worker; it must not publish a result.
 limited=root/'limited';limited.mkdir();target=limited/'generation'
 process=subprocess.run([str(DOTNET),str(helper)],env=ENV,input=json.dumps({'id':1,'method':'metadata/prepare-experimental','params':{'referencePaths':refs,'workspaceAssemblyPaths':owned,'assemblyPaths':assemblies,'outputDirectory':str(target),'maxWorkingSetMb':32}})+'\n',capture_output=True,text=True,timeout=30)
 assert process.returncode==75 and not target.exists(),(process.returncode,process.stdout,process.stderr)
 records.append({'case':'sampled_memory_guard_no_publication','exitCode':process.returncode,'passed':True})
 evidence={'scope':'correctness only; not quiet performance or whole-engine acceptance','canonicalization':'exporter provenance excluded; declaredMemberNames are sets; catalog multiset plus exact first-row-wins alias projection compared; every other value/order retained','helperSha256':sha(helper),'controlHelperSha256':sha(control),'fixtureRoot':str(root),'sharedSummary':result,'cases':records,'passed':len(records)}
 (ROOT/'test-results/recovery/shared-metadata.json').write_text(json.dumps(evidence,indent=2)+'\n');print(json.dumps(evidence,indent=2))
if __name__=='__main__':main()
