#!/usr/bin/env python3
"""Read-only narrow reference-gap replay; never changes the fixed Grace score."""
import argparse
import json
import shlex
from pathlib import Path
from run import Session, file_sha

NAMES = ['record_or_property_access_07_Ranges', 'record_or_property_access_11_ArtifactId',
         'record_creation_label_16_Artifact', 'record_update_label_18_State',
         'record_update_label_21_DeletedAt']

def key(location):
    path = location.get('path', location.get('uri', '').removeprefix('file://'))
    r = location['range']
    return (str(Path(path).resolve()), r['end']['line'], r['end']['character'])

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', type=Path, required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--model', type=Path, help='Genuinely refreshed model; binary metadata stays in --prepared')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    selection_path = root / 'evaluation/results/recovery-grace-goldens46-corrected.json'
    selection = json.loads(selection_path.read_text())
    workspace = Path(selection['root'])
    project = workspace / 'Grace.Types.fsproj'
    engine = args.engine.resolve()
    assert file_sha(engine) == args.sha256
    prepared = args.prepared.resolve()
    inputs = {key: prepared / name for key, name in [('evaluatedModelPath', 'model.json'),
              ('metadataPath', 'catalog.json'), ('builtinEnvironmentPath', 'builtin-environment.json')]}
    if args.model:
        inputs['evaluatedModelPath'] = args.model.resolve()
    work = args.output.resolve().parent / (args.output.stem + '-work')
    work.mkdir(parents=True, exist_ok=True)
    report = {'kind': 'narrow-reference-completeness-diagnostic-not-fixed-score',
              'engine': str(engine), 'engine_sha256': args.sha256,
              'selection': str(selection_path), 'selection_sha256': file_sha(selection_path),
              'prepared_inputs': {key: {'path': str(path), 'sha256': file_sha(path)} for key, path in inputs.items()},
              'groups': [], 'performance_acceptance': False, 'notes': [
                  'Five unchanged fixed-golden symbols; extra gap-cursor queries do not modify the46-operation score.',
                  'Positive/negative membership is determined by the independently validated complete golden sets.',
                  'Null trace reasons mean this binary does not emit a finer predicate for that record/module case.']}
    session = Session([str(engine), 'daemon'], timeout=120, memory_limit_mib=2048)
    texts = {}
    opened = set()
    def open_document(path):
        if path not in texts:
            texts[path] = Path(path).read_text()
        if path not in opened:
            response, _ = session.call('document/open', {'path': path, 'text': texts[path], 'version': 1})
            assert 'error' not in response, response
            opened.add(path)
    def save():
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    try:
        response, _ = session.call('workspace/open', {'root': str(workspace), 'mode': 'indexed',
            'cacheDir': str(work / 'cache'), **{key: str(path) for key, path in inputs.items()}})
        report['workspace_open'] = response
        for name in NAMES:
            query = next(q for q in selection['selected'] if q['name'] == name)
            params = {'path': query['path'], 'line': query['line'], 'character': query['character'],
                      'projectPath': str(project), 'includeDeclaration': True, 'traceReferences': True}
            open_document(params['path'])
            response, _ = session.call('navigation/references', params)
            expected = {key(loc): loc for loc in query['reference_locations']}
            result = response.get('result', {})
            actual = {key(loc): loc for loc in result.get('locations', [])}
            group = {'name': name, 'query': params, 'declaration': query['declaration'],
                     'expected_count': len(expected), 'partial_count': len(actual), 'response': response,
                     'missing_positive_locations': [expected[k] for k in sorted(expected.keys() - actual.keys())],
                     'unexpected_locations': [actual[k] for k in sorted(actual.keys() - expected.keys())], 'gaps': []}
            report['groups'].append(group)
            for gap in (result.get('proofGaps') or {}).get('examples', []):
                if gap['kind'] != 'unresolved-occurrence':
                    group['gaps'].append({'native_gap': gap})
                    continue
                path = gap['path']; line = gap['range']['start']['line']
                open_document(path)
                cursor = {'path': path, 'line': line, 'character': gap['range']['start']['character'],
                          'projectPath': str(project)}
                definition, _ = session.call('navigation/definition', cursor)
                lines = texts[path].splitlines()
                group['gaps'].append({'native_gap': gap, 'is_expected_reference': key(gap) in expected,
                    'definition_response': definition, 'context': [{'line': n + 1, 'text': lines[n]}
                        for n in range(max(0, line - 8), min(len(lines), line + 9))]})
            save()
            print(json.dumps({'name': name, 'partial': len(actual), 'expected': len(expected),
                              'gaps': len(group['gaps']), 'missing': len(group['missing_positive_locations'])}), flush=True)
        report['final_stats'] = session.stats()
    finally:
        session.close()
        report['engine_sha256_after'] = file_sha(engine)
        report['source_sha256_after'] = {path: file_sha(Path(path)) for path in texts}
        report['source_stable'] = all(Path(path).read_text() == text for path, text in texts.items())
        save()

if __name__ == '__main__':
    main()
