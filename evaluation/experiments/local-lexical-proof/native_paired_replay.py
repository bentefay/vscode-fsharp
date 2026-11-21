#!/usr/bin/env python3
"""Normal native path vs actual pinned-FSAC parameter definition targets."""
import argparse,hashlib,json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'semantic/tests'));import smoke
from resolution_states import Native
sys.path.insert(0,str(ROOT/'evaluation'));from run import canonical_location

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,required=True);p.add_argument('--helper',type=Path,required=True);p.add_argument('--fsac-report',type=Path,required=True);p.add_argument('--model',type=Path);p.add_argument('--project',default='LexicalOracle.fsproj');p.add_argument('--require-sound-references',action='store_true');p.add_argument('--output-dir',type=Path,required=True);a=p.parse_args()
 out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=False);baseline=json.loads(a.fsac_report.read_text());fixture=Path(baseline['fixture']);smoke.HELPER=a.helper.resolve()
 model=json.loads(a.model.read_text()) if a.model else smoke.request('workspace/evaluate',{'root':str(fixture),'projects':[str(fixture/a.project)]});assert model['completeness']=='exact' and all(x['membershipReusable'] and x['environmentCertificate']['version']==4 for x in model['projects']),model
 model_path=out/'model.json';model_path.write_text(json.dumps(model,indent=2)+'\n')
 report={'kind':'normal-native-paired-parameter-fsac-comparison','binary':str(a.binary.resolve()),'binarySha256':sha(a.binary),'helperSha256':sha(a.helper),'fsacReport':str(a.fsac_report.resolve()),'fsacReportSha256':sha(a.fsac_report),'model':str(model_path),'modelSha256':sha(model_path),'rows':[]}
 native=Native(a.binary.resolve());opened=set()
 try:
  report['open']=native.call('workspace/open',{'root':str(fixture),'mode':'indexed','evaluatedModelPath':str(model_path),'cacheDir':str(out/'cache')})
  assert 'error' not in report['open'];report['startup']=native.call('workspace/stats',{})
  for old in baseline['rows']:
   q=old['query'];path=Path(q['path'])
   if path not in opened:
    ack=native.call('document/open',{'path':str(path),'text':path.read_text(),'version':q['version']});assert 'error' not in ack;opened.add(path)
   response=native.call('navigation/definition',q);result=response.get('result',{});actual=sorted([list(canonical_location(x,fixture)) for x in result.get('locations',[])])
   expected=old['canonical']['definition'];exact=result.get('completeness')=='exact';passed=exact and actual==expected
   row={'case':old['case'],'query':q,'label':old['label'],'native':response,'actual':actual,'expectedFsac':expected,'classification':'correct' if passed else 'exact-target-preference-mismatch' if exact else 'unsupported','references':{}}
   for flag in [False,True]:
    ref_response=native.call('navigation/references',{**q,'includeDeclaration':flag});ref_result=ref_response.get('result',{});ref_actual=sorted([list(canonical_location(x,fixture)) for x in ref_result.get('locations',[])])
    ref_expected=old['canonical']['refsTrue'] if flag else old['fcsExpected']['uses'];ref_exact=ref_result.get('completeness')=='exact'
    ref_class='unsupported' if not ref_exact else 'correct' if ref_actual==ref_expected else 'incomplete-exact' if set(map(tuple,ref_actual))<set(map(tuple,ref_expected)) else 'exact-set-mismatch'
    semantic_expected=old['fcsExpected']['all' if flag else 'uses']
    semantic_class='unsupported' if not ref_exact else 'correct' if ref_actual==semantic_expected else 'incomplete-exact' if set(map(tuple,ref_actual))<set(map(tuple,semantic_expected)) else 'exact-set-mismatch'
    row['references'][str(flag).lower()]={'native':ref_response,'actual':ref_actual,'expected':ref_expected,'expectedSource':'actualFSAC declaration-inclusive result' if flag else 'FCS uses excluding definition occurrences; explicit API flag contract','actualFsacSameFlag':old['canonical']['refsTrue' if flag else 'refsFalse'],'classification':ref_class,'expectedFcsProject':semantic_expected,'semanticClassification':semantic_class}
   report['rows'].append(row)
   print(row['case'],row['label'],row['classification'],flush=True)
  report['finalStats']=native.call('workspace/stats',{})
 finally:native.close()
 report['counts']={kind:sum(r['classification']==kind for r in report['rows']) for kind in ['correct','exact-target-preference-mismatch','unsupported']};report['referenceCounts']={flag:{kind:sum(r['references'][flag]['classification']==kind for r in report['rows']) for kind in ['correct','incomplete-exact','exact-set-mismatch','unsupported']} for flag in ['false','true']};report['limits']='Genuine09f project evaluation; same compiler-built sources and actualFSAC targets. Standard indexed path, no local lexical module integrated. No catalog/semantic snapshot/helper fallback. Paired definition preference is distinct from unrelated-symbol selection. Reference true compares actualFSAC inclusive sets; false preserves explicit API exclusion against FCS nondefinition uses, retaining FSAC ignored-flag differences. Missing references returned exact are classified separately.'
 report['semanticReferenceCounts']={flag:{kind:sum(r['references'][flag]['semanticClassification']==kind for r in report['rows']) for kind in ['correct','incomplete-exact','exact-set-mismatch','unsupported']} for flag in ['false','true']}
 (out/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'report':str(out/'report.json'),'counts':report['counts'],'semanticReferenceCounts':report['semanticReferenceCounts']}))
 if a.require_sound_references:
  assert all(r['references'][flag]['semanticClassification'] in ['correct','unsupported'] for r in report['rows'] for flag in ['false','true']), 'Native marked an incomplete or different FCS project reference set exact; see report.'
if __name__=='__main__':main()
