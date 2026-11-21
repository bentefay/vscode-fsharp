#!/usr/bin/env python3
"""Actual SDK400 culture-sensitive glob: versioned reuse and compatibility.

All sources/build outputs are disposable copies. No shared runtime/ICU binary
is mutated. Native unit tests cover file/directory/link mutation certificates.
"""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile
from smoke import ROOT, DOTNET, ENV

def sha(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--helper',required=True)
    parser.add_argument('--native',required=True)
    parser.add_argument('--old-helper',default=str(ROOT/'evaluation/.runtime/helper-f215dac4599c/FSharpNavigator.Semantic.dll'))
    parser.add_argument('--old-native',default=str(ROOT/'evaluation/.runtime/recovered-f27d9fc7d54b/fsharp-navigator'))
    args=parser.parse_args();helper=pathlib.Path(args.helper).resolve();native=pathlib.Path(args.native).resolve()
    base=pathlib.Path(tempfile.mkdtemp(prefix='culture-certificate-',dir=ROOT/'.cache'))
    source=ROOT/'docs/review/sdk100-profile-audit/culture-fixture-sdk400'
    for name in ['P.fsproj','A.fs','global.json']: shutil.copy2(source/name,base/name)
    shutil.copytree(source/'Sources',base/'Sources')
    sources={str(path.relative_to(base)):sha(path)for path in base.rglob('*')if path.is_file()}
    rows=[]
    def culture(locale):return dict(ENV,LANG=locale,LC_ALL=locale)
    def build(env,label):
        process=subprocess.run([str(DOTNET),'build',str(base/'P.fsproj'),'--nologo','-v:q','-m:1','-nodeReuse:false'],env=env,text=True,capture_output=True,timeout=120)
        assert process.returncode==0,process.stdout+process.stderr
        rows.append({'case':label,'buildPassed':True})
    def evaluate(binary,env,label,reusable=True):
        process=subprocess.run([str(DOTNET),str(binary),'--once'],env=env,input=json.dumps({'id':1,'method':'workspace/evaluate','params':{'root':str(base)}})+'\n',text=True,capture_output=True,timeout=180)
        assert process.returncode==0,process.stderr
        response=json.loads(process.stdout);assert 'error'not in response,response
        model=response['result'];path=base/(label+'.json');path.write_text(json.dumps(model,indent=2)+'\n')
        assert all(p['membershipReusable']==reusable for p in model['projects']),[p['membershipDiagnostics']for p in model['projects']]
        return path,model
    def query(binary,model,env,label):
        requests=[{'id':1,'method':'workspace/open','params':{'root':str(base),'mode':'indexed','cacheDir':str(base/('cache-'+label)),'evaluatedModelPath':str(model)}},
                  {'id':2,'method':'navigation/references','params':{'path':str(base/'A.fs'),'line':1,'character':5,'includeDeclaration':False}},
                  {'id':3,'method':'shutdown','params':{}}]
        process=subprocess.run([str(binary),'daemon'],env=env,input=''.join(json.dumps(q)+'\n'for q in requests),text=True,capture_output=True,timeout=45)
        assert process.returncode==0,process.stderr
        replies=[json.loads(line)for line in process.stdout.splitlines()if line.strip()];opened=next(row for row in replies if row['id']==1)['result'];assert opened['filesRead']==0 and opened['filesParsed']==0 and opened['helperStarts']==0,opened
        reply=next(row for row in replies if row['id']==2);rows.append({'case':label,'open':opened,'query':reply});return reply
    en=culture('en_US.UTF-8');tr=culture('tr_TR.UTF-8')
    build(en,'en-US build');old,old_model=evaluate(args.old_helper,en,'legacy-v3');current,model=evaluate(helper,en,'current-en')
    assert model['projects'][0]['environmentCertificate']['version']==4
    initial=query(native,current,en,'initial-current');assert initial.get('result',{}).get('completeness')=='exact'and initial['result']['locations']==[],initial
    legacy=query(native,old,en,'new-native-old-v3');assert 'error'in legacy and 'provenance'in legacy['error']['message'],legacy
    backward=query(args.old_native,current,en,'old-native-new-v4');assert 'error'in backward and 'provenance'in backward['error']['message'],backward
    build(tr,'tr-TR build');stale=query(native,current,tr,'stale-culture');assert 'error'in stale and 'environment changed'in stale['error']['message'],stale
    fresh,model_tr=evaluate(helper,tr,'current-tr');actual=query(native,fresh,tr,'fresh-current');assert actual.get('result',{}).get('completeness')=='exact'and len(actual['result']['locations'])==1,actual
    assert '%C4%B1' in actual['result']['locations'][0]['uri'] or 'ı' in actual['result']['locations'][0]['uri']
    alias=dict(tr);alias.pop('LANG');alias['lang']='tr_TR.UTF-8';case=query(native,fresh,alias,'noncanonical-selector-case');assert 'error'in case and 'Noncanonical case'in case['error']['message'],case
    _,alias_model=evaluate(helper,alias,'producer-noncanonical',False)
    assert any('Noncanonical startup selector casing'in note for p in alias_model['projects']for note in p['membershipDiagnostics'])
    assert sources=={name:sha(base/name)for name in sources}
    report={'scope':'actual builds and lazy native source references; no editor or performance acceptance','fixtureRoot':str(base),'helperSha256':sha(helper),'nativeSha256':sha(native),'oldHelperSha256':sha(args.old_helper),'oldNativeSha256':sha(args.old_native),'sourcesUnchanged':sources,'cases':rows,'producerCaseAliasRejected':True,'modelPaths':{'v3':str(old),'en':str(current),'tr':str(fresh)},'globalization':model['projects'][0]['environmentCertificate']['globalization'],'passed':True}
    evidence=ROOT/'test-results/recovery'/('globalization-models-'+sha(helper)[:12]+'-'+sha(native)[:12]);evidence.mkdir(exist_ok=True)
    retained={}
    for name,path in [('v3',old),('en',current),('tr',fresh)]:
        target=evidence/(name+'.json');shutil.copy2(path,target);retained[name]={'path':str(target.relative_to(ROOT)),'sha256':sha(target)}
    for name in sources:
        target=evidence/'fixture'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(base/name,target)
    report['retainedModels']=retained
    output=ROOT/'test-results/recovery/globalization-certificate.json';output.write_text(json.dumps(report,indent=2)+'\n');print('PASS culture initial/stale/fresh, both version directions, selector case, zero-source startup; '+str(output))
if __name__=='__main__':main()
