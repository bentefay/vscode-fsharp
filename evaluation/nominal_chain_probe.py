#!/usr/bin/env python3
"""Unscored original-source cursor probes; never sends document overlays."""
import argparse
import json
from pathlib import Path
import tempfile
from urllib.parse import unquote, urlparse
from run import Session, file_sha

ROOT = Path(__file__).resolve().parents[1]

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--engine', type=Path, required=True)
    p.add_argument('--engine-sha', required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    engine = args.engine.resolve()
    assert file_sha(engine) == args.engine_sha
    scope = ROOT.parent / 'research/corpus/Grace/src/Grace.Types'
    path = scope / 'UsageObservation.Types.fs'
    model = ROOT / 'evaluation/.runtime/recovery-prepared-grace-v4-09f011/model.json'
    prepared = ROOT / 'evaluation/.runtime/recovery-prepared-grace-f215'
    inputs = [model, prepared / 'catalog.json', prepared / 'builtin-environment.json']
    oracle_path = ROOT / 'evaluation/results/nominal-chain-direct-fcs-oracle.json'
    oracle = json.loads(oracle_path.read_text())
    assert not oracle['errors'] and len(oracle['rows']) == 3
    report = dict(kind='unscored-original-source-nominal-chain-probe',
                  engine_sha256=file_sha(engine), source_sha256=file_sha(path),
                  prepared_inputs={str(x):file_sha(x) for x in inputs}, rows=[],
                  document_notifications=0, performance_acceptance=False,
                  oracle_path=str(oracle_path), oracle_sha256=file_sha(oracle_path),
                  notes=['Three additional diagnostic cursors; fixed Grace46 denominator and goldens unchanged.',
                         'No document/open, change, or overlay was sent. Source remained on disk.'])
    cache = tempfile.mkdtemp(prefix='nominal-chain-', dir=ROOT / '.tmp/evaluation-recovery')
    session = Session([str(engine), 'daemon'], timeout=120, memory_limit_mib=2048)
    lines = path.read_text().splitlines()
    try:
        report['workspace_open'], _ = session.call('workspace/open', dict(root=str(scope), mode='indexed',
            cacheDir=cache, evaluatedModelPath=str(model), metadataPath=str(inputs[1]),
            builtinEnvironmentPath=str(inputs[2])))
        for one_based in [36, 79, 122]:
            line = lines[one_based-1]
            start = line.index('RepositoryId', line.index('value.Scope.'))
            request = dict(path=str(path), line=one_based-1,
                           character=len(line[:start].encode('utf-16-le'))//2,
                           projectPath=str(scope / 'Grace.Types.fsproj'))
            response, _ = session.call('navigation/definition', request)
            expected = next(x['declaration'] for x in oracle['rows'] if x['line'] == one_based)
            expected_path = (scope / expected['path']).resolve()
            expected_range = dict(start=dict(line=expected['line'], character=expected['character']),
                                  end=dict(line=expected['endLine'], character=expected['endCharacter']))
            result = response.get('result', {})
            locations = result.get('locations', [])
            correct = result.get('completeness') == 'exact' and len(locations) == 1 and \
                Path(unquote(urlparse(locations[0]['uri']).path)).resolve() == expected_path and \
                locations[0]['range'] == expected_range
            report['rows'].append(dict(name=f'value_scope_repository_{one_based}',
                                       request=request, source_line=line, response=response,
                                       expected_path=str(expected_path), expected_range=expected_range, correct=correct))
            print(one_based, response.get('result', {}).get('completeness'), response.get('result', {}).get('locations'), flush=True)
        report['stats'], _ = session.call('workspace/stats', {})
    finally:
        session.close()
    report['source_unchanged'] = file_sha(path) == report['source_sha256']
    report['engine_unchanged'] = file_sha(engine) == report['engine_sha256']
    args.output.write_text(json.dumps(report, indent=2) + '\n')

if __name__ == '__main__':
    main()
