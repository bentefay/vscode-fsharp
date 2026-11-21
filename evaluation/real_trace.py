#!/usr/bin/env python3
"""Actual tracked Grace source trace; reconstructed after VM reset.

No source files are copied/simplified. Dirty edits are document overlays.
All timing remains diagnostic until a reviewed immutable quiet run.
"""
import argparse,json,re,shlex,subprocess,time
from pathlib import Path
from run import Session,canonical_location,file_sha

def equivalent_locations(actual,expected,root):
    normalized=[]
    for value in actual:
        replacement=value
        for target in expected:
            if value==target:break
            if value[0]!=target[0] or value[1]!=value[3] or target[1]!=target[3] or value[1]!=target[1] or value[4]!=target[4] or value[2]<=target[2]:continue
            line=(root/value[0]).read_text().splitlines()[value[1]].encode('utf-16-le');leaf=line[value[2]*2:value[4]*2].decode('utf-16-le');whole=line[target[2]*2:target[4]*2].decode('utf-16-le')
            if re.fullmatch(r"[\w']+",leaf) and whole.endswith('.'+leaf):replacement=target;break
        normalized.append(replacement)
    return sorted(normalized)==sorted(expected)

def locate(root,file,anchor,token,occurrence=0):
    path=root/file;lines=path.read_text().splitlines();found=[(i,line) for i,line in enumerate(lines) if (line==anchor if anchor.strip()=='s' else anchor in line)]
    if len(found)!=1:raise ValueError((file,anchor,len(found)))
    line,text=found[0];starts=[m.start() for m in re.finditer(r"(?<![\w'])"+re.escape(token)+r"(?![\w'])",text)];char=starts[occurrence]
    return {'path':str(path),'line':line,'character':len(text[:char].encode('utf-16-le'))//2,'includeDeclaration':True}

def queries(root,profile):
    if profile=='types':
        return [(name,method,locate(root,file,anchor,token,occurrence)) for name,method,file,anchor,token,occurrence in [
          ('source_type_alias','definition','Common.Types.fs','type ParentBranchId = BranchId','BranchId',0),
          ('local_parameter_definition','definition','Common.Types.fs','        s','s',0),
          ('local_parameter_references','references','Common.Types.fs','let cleanJson (s: string)','s',0),
          ('record_field_definition','definition','ArtifactGrant.Types.fs','member this.Route =','RepositoryId',0),
          ('record_field_references','references','ArtifactGrant.Types.fs','member this.Route =','RepositoryId',0),
          ('member_parameter_definition','definition','ArtifactGrant.Types.fs','if String.IsNullOrEmpty(blake3Hash)','blake3Hash',0)]]
    return [(name,method,locate(root,'Log.CLI.fs',anchor,token,occurrence)) for name,method,anchor,token,occurrence in [('cross_binding_definition','definition','let LogInformational','Log',0),('local_parameter_definition','definition','printfn $','message',0),('local_parameter_references','references','let Log (level: LogLevel)','message',0),('source_union_definition','definition','let LogError','Error',0)]]

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--profile',choices=['types','cli'],required=True);p.add_argument('--engine',required=True);p.add_argument('--model',type=Path);p.add_argument('--golden',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--export-goldens',action='store_true');p.add_argument('--timeout',type=float,default=120);p.add_argument('--warm',type=int,default=2);p.add_argument('--clean-only',action='store_true');p.add_argument('--project-path',type=Path);a=p.parse_args()
    root=a.root.resolve();qs=queries(root,a.profile);out=a.output.resolve();out.parent.mkdir(parents=True,exist_ok=True)
    if a.project_path:
        for _,_,params in qs:params['projectPath']=str(a.project_path.resolve())
    expected=json.loads(a.golden.read_text())['goldens'] if a.golden else {};rows=[];goldens={};report={'root':str(root),'profile':a.profile,'command':shlex.split(a.engine),'model_sha256':file_sha(a.model) if a.model else None,'selected_project_path':str(a.project_path.resolve()) if a.project_path else None,'corpus_sha':subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip(),'query_source_hashes':{q[2]['path']:file_sha(q[2]['path']) for q in qs},'rows':rows,'goldens':goldens,'recovery_status':'Actual post-reset rerun, reconstructed trace','notes':['Rawtrackedsource; dirtyoverlaysonly. Sharedmachine diagnostic until explicitlyfrozenquietrun.','BackendRSS subtracts artificialPythonbridge; wholeprocesssubtree also retained.']}
    def save():out.write_text(json.dumps(report,indent=2)+'\n')
    s=Session(shlex.split(a.engine),timeout=a.timeout,memory_limit_mib=4096)
    def call(method,params,phase,name=None):
        response,measure=s.call(method,params);row={'phase':phase,'query':name,'method':method,'params':{k:v for k,v in params.items() if k!='text'},'response':response,**measure,'since_start_ms':(time.perf_counter()-s.started)*1000}
        if method.startswith('navigation/'):
            actual=sorted(set(canonical_location(x,root) for x in response.get('result',{}).get('locations',[])));exact='error' not in response and response.get('result',{}).get('completeness')=='exact';row.update(actual=actual,raw_location_count=len(response.get('result',{}).get('locations',[])),complete=exact,correct=None)
            if name in expected:
                wanted=[tuple(x) for x in expected[name]];row['correct']=exact and (equivalent_locations(actual,wanted,root) if method.endswith('/references') else actual==wanted);row['expected']=wanted
            if a.export_goldens and exact and phase=='clean':goldens[name]=actual
        rows.append(row);save();print(phase,name,response.get('error') or response.get('result',{}).get('completeness','ok'),row.get('correct'),flush=True)
    try:
        params={'root':str(root),'mode':'indexed','cacheDir':str(out.parent/(out.stem+'-cache'))}
        if a.model:params['evaluatedModelPath']=str(a.model.resolve())
        call('workspace/open',params,'startup')
        for path in dict.fromkeys(q[2]['path'] for q in qs):call('document/open',{'path':path,'text':Path(path).read_text(),'version':1},'document_open')
        for name,method,params in qs:call('navigation/'+method,params,'clean',name)
        for repeat in range(a.warm):
            for name,method,params in reversed(qs):call('navigation/'+method,params,'warm',name)
        if a.profile=='types' and not a.clean_only:
            path=root/'Common.Types.fs';source=path.read_text();parameter=next(q for q in qs if q[0]=='local_parameter_references')
            for version,kind,text in [(2,'body_edit',source.replace('.Replace(@"\\r\\n", Environment.NewLine)','.Replace(@"\\r\\n", "\\n")')),(3,'inferred_api_edit',source.replace('.Replace(@"\\r\\n", Environment.NewLine)','.Replace(@"\\r\\n", Environment.NewLine).Length'))]:
                if text==source:raise ValueError('Edit anchor absent')
                start=time.perf_counter();call('document/change',{'path':str(path),'text':text,'version':version},kind);call('navigation/'+parameter[1],parameter[2],kind,parameter[0]);rows[-1]['edit_to_answer_ms']=(time.perf_counter()-start)*1000
    finally:s.close();report['sampler_cpu_seconds']=s.sampler_cpu_seconds;report['stderr']=s.stderr;save()
if __name__=='__main__':main()
