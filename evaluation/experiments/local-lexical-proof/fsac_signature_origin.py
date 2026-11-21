#!/usr/bin/env python3
"""Add the renamed signature-origin cursor without rebuilding/changing its source."""
import argparse, hashlib, json, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'evaluation'))
from run import Session, canonical_location
sys.path.insert(0, str(ROOT / 'semantic/tests'))
import smoke


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-report', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = json.loads(args.base_report.read_text())
    fixture = Path(report['fixture'])
    assert report['actualSdkBuild']['exitCode'] == 0
    assert all(sha(fixture / name) == expected for name, expected in report['sourceFiles'].items())
    report['priorReport'] = str(args.base_report.resolve())
    report['priorReportSha256'] = sha(args.base_report)
    report['kind'] = 'pinned-fsac-parameter-boundary-with-renamed-signature-origin'
    report['sourceOraclePreservedPath'] = str(HERE / 'oracle-implementation-diagnostics-only.json')
    assert sha(report['sourceOraclePreservedPath']) == report['sourceOracleSha256']
    oracle_path = HERE / 'oracle.json'
    oracle = json.loads(oracle_path.read_text())
    oracle_case = next(row for row in oracle['cases'] if row['case'] == 'paired_name_mismatch_signature')
    oracle_result = next(row for row in oracle['results'] if row['case'] == oracle_case['case'])
    assert oracle_result['valid'] and not oracle_result['diagnostics']
    report['correctedProjectOracle'] = {'path': str(oracle_path), 'sha256': sha(oracle_path), 'case': oracle_case, 'result': oracle_result}
    report['versions'] = {'ionideExtension': '8.1.0', 'bundledFsAutoComplete': '0.86.0', 'actualEditorHost': False}
    command = [sys.executable, str(ROOT / 'evaluation/fsac_adapter.py'), '--ionide', str(ROOT / 'evaluation/.runtime/extensions/Ionide.Ionide-fsharp-8.1.0'), '--dotnet', str(smoke.DOTNET), '--timeout', '60', '--trace', str(out / 'fsac-trace.jsonl')]
    session = Session(command, timeout=90, memory_limit_mib=2048)

    def call(method, params):
        response, _ = session.call(method, params)
        assert 'error' not in response, (method, response)
        return response['result']

    try:
        report['signatureOriginWorkspaceOpen'] = call('workspace/open', {'root': str(fixture), 'cacheDir': str(out / 'fsac-cache'), 'projects': [str(fixture / 'LexicalOracle.fsproj')]})
        path = fixture / 'Case08.fsi'
        assert 'publicName' in path.read_text()
        call('document/open', {'path': str(path), 'text': path.read_text(), 'version': 1})
        original = oracle_case['queries'][0]
        query = {'path': str(path), 'line': original['line'], 'character': original['character'], 'version': 1}
        definition = call('navigation/definition', query)
        refs_false = call('navigation/references', {**query, 'includeDeclaration': False})
        refs_true = call('navigation/references', {**query, 'includeDeclaration': True})
        actual = lambda result: sorted(list(canonical_location(row, fixture)) for row in result['locations'])
        raw = oracle_result['results'][0]
        mapped = lambda r: ['Case08' + Path(r['path']).suffix, r['startLine'], r['startCharacter'], r['endLine'], r['endCharacter']]
        expected_all = sorted(mapped(row['range']) for row in raw['projectReferences'])
        expected_uses = sorted(mapped(row['range']) for row in raw['projectReferences'] if not row['isDefinition'])
        row = {'case': oracle_case['case'], 'query': query, 'label': original['label'], 'definition': definition, 'referencesIncludeDeclarationFalse': refs_false, 'referencesIncludeDeclarationTrue': refs_true, 'canonical': {'definition': actual(definition), 'refsFalse': actual(refs_false), 'refsTrue': actual(refs_true)}, 'fcsExpected': {'definition': [mapped(raw['symbol']['declaration'])], 'uses': expected_uses, 'all': expected_all}}
        row['comparison'] = {'definitionMatchesFcs': row['canonical']['definition'] == row['fcsExpected']['definition'], 'refsFalseMatchesFcsUses': row['canonical']['refsFalse'] == expected_uses, 'refsTrueMatchesFcsAll': row['canonical']['refsTrue'] == expected_all, 'flagChangesResult': row['canonical']['refsFalse'] != row['canonical']['refsTrue']}
        report['rows'].append(row)
    finally:
        session.close()
        report['ownedProcessesClosed'] = True
    report['queryLocations'] = len(report['rows'])
    report['providerResponses'] = len(report['rows']) * 3
    report['summary'] = {key: sum(row['comparison'][key] for row in report['rows']) for key in report['rows'][0]['comparison']}
    report['limits'] += ' The additional renamed signature-origin cursor uses the unchanged built project and the corrected full-project FCS oracle. Prior 27 responses are inherited with their original provenance; only this extra cursor ran in the supplemental process.'
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(out / 'report.json'), 'added': row}, indent=2))


if __name__ == '__main__':
    main()
