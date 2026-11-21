#!/usr/bin/env python3
"""Staged document-local native protocol acceptance; requires integrated candidate."""
import argparse, hashlib, json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'semantic/tests'))
from resolution_states import Native

SOURCE = 'module M\r\nlet values = List.map (fun x -> ("😀",x)) [1]\r\n'


def position(source, token):
    before = source[:source.rindex(token)].split('\n')
    return {'line': len(before) - 1, 'character': len(before[-1].encode('utf-16-le')) // 2}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    fixture = out / 'workspace'
    fixture.mkdir()
    source_path = fixture / 'Main.fs'
    source_path.write_text(SOURCE, newline='')
    report = {'kind': 'document-local-native-protocol-acceptance', 'binary': str(args.binary.resolve()), 'binarySha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'rows': [], 'performanceClaim': False}
    save = lambda: (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    def call(native, label, method, params):
        response = native.call(method, params)
        report['rows'].append({'label': label, 'method': method, 'params': params, 'response': response})
        save()
        return response

    def answer(response, version):
        assert 'error' not in response, response
        result = response['result']
        assert result['completeness'] == 'exact' and result['source'] == 'local-lexical', result
        assert result['proofScope'] == 'document' and 'projectContextId' not in result, result
        assert result['version'] == result['documentVersion'] == version, result
        assert re.fullmatch('[0-9a-f]{64}', result['sourceHash']) and re.fullmatch('[0-9a-f]{64}', result['documentProofId']), result
        return result

    def no_project_work(stats):
        for key in ['modelLoadMicros', 'catalogLoadMicros', 'filesRead', 'bytesRead', 'filesParsed', 'sourceFileCount', 'compilerLaunches', 'helperStarts', 'decompilerStarts']:
            assert stats[key] == 0, (key, stats)
        assert not stats['builtinEnvironmentLoaded'] and not stats['watcherActive'], stats
        assert all(value == 0 for value in stats['modelValidation'].values()), stats
        assert stats['localProofHits'] > 0 and stats['localProofParsedBytes'] > 0, stats

    options = {'root': str(fixture), 'mode': 'indexed', 'cacheDir': str(out / 'cache'), 'evaluatedModelPath': str(out / 'missing-model.json'), 'libraryCatalogPath': str(out / 'missing-catalog.json'), 'builtinEnvironmentPath': str(out / 'missing-environment.json'), 'helperPath': str(out / 'missing-helper')}
    native = Native(args.binary.resolve())
    try:
        opened = call(native, 'cold-open', 'workspace/open', options)
        assert 'error' not in opened, opened
        call(native, 'overlay-v7', 'document/open', {'path': str(source_path), 'text': SOURCE, 'version': 7})
        query = {'path': str(source_path), **position(SOURCE, 'x))'), 'version': 7}
        first = answer(call(native, 'cold-definition', 'navigation/definition', query), 7)
        assert len(first['locations']) == 1
        for flag, count in [(True, 2), (False, 1)]:
            result = answer(call(native, f'warm-references-{flag}', 'navigation/references', {**query, 'includeDeclaration': flag}), 7)
            assert len(result['locations']) == count
            assert result['sourceHash'] == first['sourceHash']
        stale = call(native, 'stale-version', 'navigation/definition', {**query, 'version': 6})
        assert 'error' in stale or stale.get('result', {}).get('completeness') != 'exact', stale
        changed = SOURCE.replace('fun x -> ("😀",x)', 'fun renamed -> ("😀",renamed)')
        call(native, 'overlay-v8', 'document/change', {'path': str(source_path), 'text': changed, 'version': 8})
        current = answer(call(native, 'dirty-definition', 'navigation/definition', {'path': str(source_path), **position(changed, 'renamed))'), 'version': 8}), 8)
        assert current['sourceHash'] != first['sourceHash'] and current['documentProofId'] != first['documentProofId']
        no_project_work(call(native, 'after-positive-stats', 'workspace/stats', {})['result'])
        call(native, 'close-buffer', 'document/close', {'path': str(source_path)})
        closed = call(native, 'closed-version', 'navigation/definition', {**query, 'version': 8})
        assert 'error' in closed or closed.get('result', {}).get('completeness') != 'exact', closed
        call(native, 'reopen-saved-v1', 'document/open', {'path': str(source_path), 'text': SOURCE, 'version': 1})
        reopened = answer(call(native, 'reopen-definition', 'navigation/definition', {**query, 'version': 1}), 1)
        assert reopened['sourceHash'] == first['sourceHash'] and reopened['documentProofId'] != current['documentProofId']
        no_project_work(call(native, 'before-restart-stats', 'workspace/stats', {})['result'])
    finally:
        native.close()
    restarted = Native(args.binary.resolve())
    try:
        call(restarted, 'restart-workspace', 'workspace/open', options)
        call(restarted, 'restart-overlay', 'document/open', {'path': str(source_path), 'text': SOURCE, 'version': 1})
        after_restart = answer(call(restarted, 'restart-definition', 'navigation/definition', {**query, 'version': 1}), 1)
        assert after_restart['sourceHash'] == first['sourceHash']
        no_project_work(call(restarted, 'restart-stats', 'workspace/stats', {})['result'])
    finally:
        restarted.close()
    report['passed'] = True
    report['limits'] = 'Actual immutable native process, protocol only. Deliberately inaccessible project/catalog/environment prove those paths are not required by positive queries. UTF16, includeDeclaration, stale version, dirty rename, close/reopen and process restart are correctness checks; no timing or VSCode-host claim. Rejected shapes are covered separately by the adapter test and parser oracle.'
    save()
    print(json.dumps({'report': str(out / 'report.json'), 'passed': True}))


if __name__ == '__main__':
    main()
