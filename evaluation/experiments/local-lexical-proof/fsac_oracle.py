#!/usr/bin/env python3
"""Pinned Ionide/FSAC parameter scope oracle; no native path or timing claims."""
import argparse,hashlib,json,os,subprocess,sys,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'evaluation'))
from run import Session,canonical_location
sys.path.insert(0,str(ROOT/'semantic/tests'))
import smoke
CASES=['paired_Local.fs','paired_direct_lambda','paired_returned_lambda','paired_passed_lambda','paired_identity_lambda_computed','paired_nested_function','paired_nested_function_return_unnamed','paired_tail_lambda_behind_let_unnamed','paired_name_mismatch']

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output-dir',required=True,type=Path);a=p.parse_args()
    out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=False)
    # Preserve the exact original inputs. The later full-project audit separately
    # corrected the invalid computed signature; its revision below is explicit.
    source_oracle=HERE/'oracle-implementation-diagnostics-only.json'
    fixture=out/'fixture';fixture.mkdir();old=json.loads(source_oracle.read_text())
    bycase={v['case']:v for v in old['cases']};results={v['case']:v for v in old['results']}
    specs=[];files=[]
    for index,name in enumerate(CASES):
        original=bycase[name];control=results[name];assert control['valid'],name
        module=f'Case{index:02d}';paths={}
        revision=None
        for suffix in ['.fsi','.fs']:
            previous=next(Path(v) for v in original['projectFiles'] if Path(v).suffix==suffix)
            source=previous.read_text();assert source.startswith('module Local\n')
            if name=='paired_identity_lambda_computed' and suffix=='.fsi':
                assert 'val f : (x:int -> int)' in source
                source=source.replace('val f : (x:int -> int)','val f : (int -> int)')
                revision={'originalCase':name,'newCase':'paired_identity_lambda_computed_unnamed_signature','reason':'Actual SDK rejects the original named parenthesized function signature; old implementation-only diagnostics omitted signature errors. This separately labeled control uses a valid unnamed computed-function signature.'}
            current=fixture/(module+suffix);current.write_text(source.replace('module Local\n',f'module {module}\n',1));files.append(current.name);paths[suffix]=current
        specs.append({'case':revision['newCase'] if revision else name,'sourceOracle':control,'controlRevision':revision,'pathMap':{suffix:str(path) for suffix,path in paths.items()},'queries':original['queries']})
    # A signature-origin cursor checks declaration preference and grouped references.
    specs.append({'case':'paired_Local.fsi','sourceOracle':results['paired_Local.fsi'],'pathMap':specs[0]['pathMap'],'queries':bycase['paired_Local.fsi']['queries']})
    project=fixture/'LexicalOracle.fsproj';smoke.project(project,''.join(f'<Compile Include="{name}"/>' for name in files))
    (fixture/'global.json').write_text(json.dumps({'sdk':{'version':'10.0.400','rollForward':'disable','allowPrerelease':False}})+'\n')
    dotnet=smoke.DOTNET;ionide=ROOT/'evaluation/.runtime/extensions/Ionide.Ionide-fsharp-8.1.0'
    report={'kind':'pinned-fsac-local-parameter-boundary-oracle','nativeImplementationTested':False,'performanceClaim':False,'sourceOracle':str(source_oracle),'sourceOracleSha256':sha(source_oracle),'fixture':str(fixture),'cases':specs,'rows':[]}
    def save(): (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    build=subprocess.run([str(dotnet),'build',str(project),'--nologo','-v:q','-p:RestoreLockedMode=true'],env=smoke.ENV,cwd=fixture,text=True,capture_output=True,timeout=120)
    report['actualSdkBuild']={'exitCode':build.returncode,'stdout':build.stdout,'stderr':build.stderr};save();assert build.returncode==0,build.stdout+build.stderr
    command=[sys.executable,str(ROOT/'evaluation/fsac_adapter.py'),'--ionide',str(ionide),'--dotnet',str(dotnet),'--timeout','60','--trace',str(out/'fsac-trace.jsonl')]
    session=Session(command,timeout=90,memory_limit_mib=2048)
    def call(method,params):
        response,_=session.call(method,params);assert 'error' not in response,(method,response);return response['result']
    def mapped(range_,paths):
        return [Path(paths[Path(range_['path']).suffix]).name,range_['startLine'],range_['startCharacter'],range_['endLine'],range_['endCharacter']]
    opened=set()
    try:
        report['workspaceOpen']=call('workspace/open',{'root':str(fixture),'cacheDir':str(out/'fsac-cache'),'projects':[str(project)]});save()
        for spec in specs:
            suffix='.fsi' if spec['case']=='paired_Local.fsi' else '.fs';path=Path(spec['pathMap'][suffix])
            if path not in opened:call('document/open',{'path':str(path),'text':path.read_text(),'version':1});opened.add(path)
            for query,oracle in zip(spec['queries'],spec['sourceOracle']['results']):
                assert query['label']==oracle['label']
                position={'path':str(path),'line':query['line'],'character':query['character'],'version':1}
                definition=call('navigation/definition',position)
                refs_false=call('navigation/references',{**position,'includeDeclaration':False})
                refs_true=call('navigation/references',{**position,'includeDeclaration':True})
                actual=lambda result:sorted([list(canonical_location(row,fixture)) for row in result['locations']])
                expected_definition=[mapped(oracle['symbol']['declaration'],spec['pathMap'])]
                expected_all=sorted(mapped(row['range'],spec['pathMap']) for row in oracle['projectReferences'])
                expected_uses=sorted(mapped(row['range'],spec['pathMap']) for row in oracle['projectReferences'] if not row['isDefinition'])
                row={'case':spec['case'],'query':position,'label':query['label'],'definition':definition,'referencesIncludeDeclarationFalse':refs_false,'referencesIncludeDeclarationTrue':refs_true,
                     'canonical':{'definition':actual(definition),'refsFalse':actual(refs_false),'refsTrue':actual(refs_true)},'fcsExpected':{'definition':expected_definition,'uses':expected_uses,'all':expected_all}}
                row['comparison']={'definitionMatchesFcs':row['canonical']['definition']==expected_definition,'refsFalseMatchesFcsUses':row['canonical']['refsFalse']==expected_uses,'refsTrueMatchesFcsAll':row['canonical']['refsTrue']==expected_all,'flagChangesResult':row['canonical']['refsFalse']!=row['canonical']['refsTrue']}
                report['rows'].append(row);save()
            print('FSAC queried '+spec['case'],flush=True)
    finally:
        session.close();report['ownedProcessesClosed']=True;save()
    report['queryLocations']=len(report['rows']);report['providerResponses']=len(report['rows'])*3
    report['summary']={key:sum(row['comparison'][key] for row in report['rows']) for key in report['rows'][0]['comparison']}
    report['sourceFiles']={name:sha(fixture/name) for name in files}
    report['limits']='Actual SDK build and pinned Ionide FSAC stdio provider requests. No actual VS Code host or native path. One combined project uses separate renamed modules with original parameter/body lines unchanged; all valid paired controls only. Raw responses and includeDeclaration requests retained, no range widening or reference filtering. Definitions/reference differences are observations, not silently converted into agreement.'
    save();print(json.dumps({'report':str(out/'report.json'),'queries':report['queryLocations'],'summary':report['summary']}))
if __name__=='__main__':main()
