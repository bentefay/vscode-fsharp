#!/usr/bin/env python3
"""Export a genuine complete metadata environment for an already evaluated model."""
import argparse,json,os,shlex,time
from pathlib import Path
from run import Session,file_sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--model',type=Path,required=True);p.add_argument('--helper',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--report',type=Path,required=True);a=p.parse_args()
model=json.loads(a.model.read_text());refs=sorted({r for p in model['projects'] for r in p['assemblyReferences']});owned={p['outputPath'] for p in model['projects']};owned.update(r for p in model['projects'] for r in p.get('workspaceAssemblyReferences',[]))
missing=[r for r in refs if not Path(r).is_file() and r not in owned]
if missing:raise ValueError('Missing selected external references: '+repr(missing))
params={'referencePaths':[r for r in refs if Path(r).is_file()],'workspaceAssemblyPaths':sorted(owned)}
command=shlex.split(a.helper);report={'model':str(a.model.resolve()),'model_sha256':file_sha(a.model),'helper_command':command,'helper_files_sha256':{str(Path(x).resolve()):file_sha(x) for x in command if Path(x).is_file()},'params':params,'temporary_storage':os.environ.get('TMPDIR','/tmp'),'script_sha256':file_sha(__file__),'notes':['Explicit binary-only inventory preparation; does not typecheck workspace source.','Sampled process-tree RSS includes observed descendants and excludes controller/filesystem cache.']}
s=Session(command,timeout=120,memory_limit_mib=4096)
try:
 response,measure=s.call('metadata/environment',params);report.update(response_error=response.get('error'),measurement=measure)
 result=response.get('result',{})
 if 'error' in response or result.get('completeness')!='exact' or result.get('factsVersion')!=2:raise ValueError('Export did not produce exact version2 facts: '+repr(response.get('error')))
 a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(result,indent=2)+'\n')
 report.update(output=str(a.output.resolve()),output_sha256=file_sha(a.output),bytes=a.output.stat().st_size,counts={name:{'keys':len(result.get(name,{})),'rows':sum(len(rows) for rows in result.get(name,{}).values())} for name in ['typeNames','scopeNames','termNames']})
finally:
 s.close();report.update(sampler_cpu_seconds=s.sampler_cpu_seconds,stderr=s.stderr,model_sha256_after=file_sha(a.model));a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({key:report[key] for key in ['output','output_sha256','bytes','counts']}))
