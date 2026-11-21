#!/usr/bin/env python3
"""Probe the same symbol's references from its verified declaration, unscored."""
import json
from pathlib import Path
from run import Session, file_sha

root = Path(__file__).resolve().parents[1]
selection_path = root/'evaluation/results/recovery-grace-goldens46-corrected.json'
selection = json.loads(selection_path.read_text())
prior_path = root/'evaluation/results/recovered-858f-prepared-grace46.json'
prior = json.loads(prior_path.read_text())
names = {row['name'] for row in prior['rows'] if row.get('method') == 'definition'
         and row['response'].get('result', {}).get('completeness') != 'exact'}
engine = root/'evaluation/.runtime/recovered-858f9d9a16d3/fsharp-navigator'
scope = Path(selection['root'])
prepared = root/'evaluation/.runtime/recovery-prepared-grace-f215'
report = {'kind': 'declaration-origin-reference-diagnostic-not-fixed-score',
          'selection_sha256': file_sha(selection_path), 'prior_sha256': file_sha(prior_path),
          'engine_sha256': file_sha(engine), 'rows': [], 'performance_acceptance': False,
          'notes': ['Extra declaration cursors preserve the original fixed query and complete golden reference sets.']}
assert report['engine_sha256'] == '858f9d9a16d39338b63406916bf0fdf0bb2138a3b7b2ae9efd9bdc0d316bd14d'
session = Session([str(engine),'daemon'], timeout=120, memory_limit_mib=2048)
opened = set()
def key(loc):
    return (loc.get('path',loc.get('uri','').removeprefix('file://')),
            loc['range']['end']['line'],loc['range']['end']['character'])
try:
    report['open'],_ = session.call('workspace/open', {'root':str(scope),'mode':'indexed',
        'cacheDir':str(root/'evaluation/.runtime/declaration-priority-858f-cache'),
        'evaluatedModelPath':str(root/'evaluation/.runtime/recovery-prepared-grace-v4-09f011/model.json'),
        'metadataPath':str(prepared/'catalog.json'),'builtinEnvironmentPath':str(prepared/'builtin-environment.json')})
    for query in selection['selected']:
        if query['name'] not in names:continue
        decl=query['declaration'];path=Path(decl['path'])
        if path not in opened:
            session.call('document/open',{'path':str(path),'text':path.read_text(),'version':1});opened.add(path)
        params={'path':str(path),**decl['range']['start'],'projectPath':str(scope/'Grace.Types.fsproj'),
                'includeDeclaration':True,'traceReferences':True}
        response,_=session.call('navigation/references',params);result=response.get('result',{})
        expected={key(loc) for loc in query['reference_locations']};actual={key(loc) for loc in result.get('locations',[])}
        row={'name':query['name'],'query':params,'response':response,'expected_count':len(expected),
             'returned_count':len(actual),'missing_positive_locations':sorted(expected-actual),
             'unexpected_locations':sorted(actual-expected)};report['rows'].append(row)
        print(query['name'],len(actual),len(expected),result.get('completeness'),(result.get('proofGaps')or{}).get('count'),flush=True)
finally:
    session.close();(root/'evaluation/results/recovered-858f-declaration-reference-priority.json').write_text(json.dumps(report,indent=2)+'\n')
