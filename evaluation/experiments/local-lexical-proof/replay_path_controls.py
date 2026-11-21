#!/usr/bin/env python3
"""Replay already compiler-built path and function controls on a new native."""
import argparse, hashlib, json, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'semantic/tests'))
from resolution_states import Native
sys.path.insert(0, str(ROOT / 'evaluation'))
from run import canonical_location


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--baseline', action='append', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--require-sound-references', action='store_true')
    args = parser.parse_args()
    assert not args.output.exists(), args.output
    args.output.parent.mkdir(parents=True, exist_ok=True)
    report = {'kind': 'immutable-native-parameter-path-controls', 'binary': str(args.binary.resolve()), 'binarySha256': sha(args.binary), 'baselines': [{'path': str(path.resolve()), 'sha256': sha(path)} for path in args.baseline], 'rows': [], 'invalidBuildsNotReplayed': []}
    for path in args.baseline:
        baseline = json.loads(path.read_text())
        for old in baseline['rows']:
            if not old['validBuild']:
                report['invalidBuildsNotReplayed'].append(old['case'])
                continue
            fixture = Path(old['model']['path']).parent
            assert sha(old['model']['path']) == old['model']['sha256']
            assert all(sha(fixture / name) == expected for name, expected in old['sourceHashes'].items())
            cache = args.output.parent / ('cache-' + old['case'])
            assert not cache.exists(), cache
            native = Native(args.binary.resolve())
            try:
                opened = native.call('workspace/open', {'root': str(fixture), 'mode': 'indexed', 'evaluatedModelPath': old['model']['path'], 'cacheDir': str(cache)})
                assert 'error' not in opened, opened
                for query_old, fcs in zip(old['queries'], old['fcs']['results']):
                    assert query_old['case'] == fcs['case'] and fcs['valid']
                    query = query_old['query']
                    source = Path(query['path']).read_text()
                    native.call('document/open', {'path': query['path'], 'text': source, 'version': query['version']})
                    row = {'case': query_old['case'], 'query': query, 'operations': {}}
                    for key, previous in query_old['operations'].items():
                        method, flag_string = key.split(':')
                        flag = None if flag_string == 'None' else flag_string == 'True'
                        params = {**query, **({'includeDeclaration': flag} if flag is not None else {})}
                        response = native.call(method, params)
                        result = response.get('result', {})
                        actual = sorted(list(canonical_location(loc, fixture)) for loc in result.get('locations', []))
                        comparison = {'response': response, 'actual': actual, 'expectedFsac': previous['fsacCanonical']}
                        if method == 'navigation/references':
                            refs = fcs['results'][0]['projectReferences']
                            comparison['expectedFcsProject'] = sorted([str(Path(ref['range']['path']).relative_to(fixture)), ref['range']['startLine'], ref['range']['startCharacter'], ref['range']['endLine'], ref['range']['endCharacter']] for ref in refs if flag or not ref['isDefinition'])
                            expected = comparison['expectedFcsProject']
                            comparison['classification'] = 'unsupported' if result.get('completeness') != 'exact' else 'correct' if actual == expected else 'incomplete-exact' if set(map(tuple, actual)) < set(map(tuple, expected)) else 'exact-set-mismatch'
                        else:
                            comparison['classification'] = 'unsupported' if result.get('completeness') != 'exact' else 'correct' if actual == previous['fsacCanonical'] else 'paired-target-preference-mismatch'
                        row['operations'][key] = comparison
                    report['rows'].append(row)
            finally:
                native.close()
    report['counts'] = {kind: sum(op['classification'] == kind for row in report['rows'] for op in row['operations'].values()) for kind in ['correct', 'incomplete-exact', 'exact-set-mismatch', 'paired-target-preference-mismatch', 'unsupported']}
    report['limits'] = 'Unchanged successful SDK400 build, original full-project FCS and actual FSAC retained in the pinned baseline. No source/model regeneration. References compare complete semantic sets independently of FSAC omissions/flag behavior. Definition target preference remains separate. Explicit unavailable results are safe refusal, not recovered feature coverage. No local optimization, editor host or performance claim.'
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(args.output), 'counts': report['counts']}))
    if args.require_sound_references:
        assert not any(op['classification'] in ['incomplete-exact', 'exact-set-mismatch'] for row in report['rows'] for key, op in row['operations'].items() if key.startswith('navigation/references')), 'Incomplete or different exact reference set; see report.'


if __name__ == '__main__':
    main()
