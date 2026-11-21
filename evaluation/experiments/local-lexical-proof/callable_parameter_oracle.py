#!/usr/bin/env python3
"""Actual SDK/FCS/FSAC method and constructor parameter reference boundary."""
import argparse, hashlib, json, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'evaluation'))
from run import Session, canonical_location
sys.path.insert(0, str(ROOT / 'semantic/tests'))
import smoke

SOURCES = {
    'Callable.fs': 'module Callable\ntype C(seed:int) =\n    member _.Value = seed\n    member _.M(amount:int) = amount + amount\n    static member S(delta:int) = delta + delta\n',
    'Use.fs': 'module Use\nopen Callable\nlet instance = C(seed=1)\nlet methodResult = instance.M(amount=2)\nlet staticResult = C.S(delta=3)\n',
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', required=True, type=Path)
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    fixture = out / 'fixture'
    fixture.mkdir()
    for name, source in SOURCES.items():
        (fixture / name).write_text(source)
    project = fixture / 'CallableOracle.fsproj'
    smoke.project(project, ''.join(f'<Compile Include="{name}"/>' for name in SOURCES))
    (fixture / 'global.json').write_text(json.dumps({'sdk': {'version': '10.0.400', 'rollForward': 'disable', 'allowPrerelease': False}}) + '\n')
    files = [str(fixture / name) for name in SOURCES]
    cases = []
    for name, source in SOURCES.items():
        queries = []
        for line, text in enumerate(source.splitlines()):
            for match in re.finditer(r'\b(seed|amount|delta)\b', text):
                queries.append({'line': line, 'character': match.start(), 'token': match.group(), 'label': f'{name}:{line}:{match.start()}:{match.group()}'})
        cases.append({'case': name, 'path': str(fixture / name), 'source': source, 'projectFiles': files, 'queries': queries})
    manifest = out / 'manifest.json'
    manifest.write_text(json.dumps(cases, indent=2) + '\n')
    report = {'kind': 'method-constructor-named-argument-parameter-oracle', 'fixture': str(fixture), 'cases': cases, 'rows': [], 'versions': {'ionideExtension': '8.1.0', 'bundledFsAutoComplete': '0.86.0'}, 'sourceFiles': {name: sha(fixture / name) for name in SOURCES}}
    save = lambda: (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    build = subprocess.run([str(smoke.DOTNET), 'build', str(project), '--nologo', '-v:q'], env=smoke.ENV, cwd=fixture, text=True, capture_output=True, timeout=120)
    report['actualSdkBuild'] = {'exitCode': build.returncode, 'stdout': build.stdout, 'stderr': build.stderr}
    save()
    assert build.returncode == 0, build.stdout + build.stderr
    fcs = subprocess.run([str(smoke.DOTNET), 'fsi', '--exec', str(HERE / 'oracle.fsx'), str(manifest)], env=smoke.ENV, cwd=ROOT, text=True, capture_output=True, timeout=120)
    report['fcs'] = {'exitCode': fcs.returncode, 'stderr': fcs.stderr, 'driverSha256': sha(HERE / 'oracle.fsx'), 'results': [json.loads(line) for line in fcs.stdout.splitlines() if line.startswith('{')]}
    save()
    assert fcs.returncode == 0 and len(report['fcs']['results']) == len(cases), report['fcs']
    assert all(row['valid'] for row in report['fcs']['results']), report['fcs']
    session = Session([sys.executable, str(ROOT / 'evaluation/fsac_adapter.py'), '--ionide', str(ROOT / 'evaluation/.runtime/extensions/Ionide.Ionide-fsharp-8.1.0'), '--dotnet', str(smoke.DOTNET), '--timeout', '60', '--trace', str(out / 'fsac-trace.jsonl')], timeout=90, memory_limit_mib=2048)

    def call(method, params):
        response, _ = session.call(method, params)
        assert 'error' not in response, (method, response)
        return response['result']

    def mapped(value):
        return [Path(value['path']).name, value['startLine'], value['startCharacter'], value['endLine'], value['endCharacter']]

    try:
        report['workspaceOpen'] = call('workspace/open', {'root': str(fixture), 'cacheDir': str(out / 'fsac-cache'), 'projects': [str(project)]})
        for case, oracle in zip(cases, report['fcs']['results']):
            assert case['case'] == oracle['case']
            path = Path(case['path'])
            call('document/open', {'path': str(path), 'text': case['source'], 'version': 1})
            for query, fcs_row in zip(case['queries'], oracle['results']):
                position = {'path': str(path), 'line': query['line'], 'character': query['character'], 'version': 1}
                definition = call('navigation/definition', position)
                refs_false = call('navigation/references', {**position, 'includeDeclaration': False})
                refs_true = call('navigation/references', {**position, 'includeDeclaration': True})
                actual = lambda result: sorted(list(canonical_location(row, fixture)) for row in result['locations'])
                row = {'case': case['case'], 'label': query['label'], 'query': position, 'definition': definition, 'referencesIncludeDeclarationFalse': refs_false, 'referencesIncludeDeclarationTrue': refs_true, 'canonical': {'definition': actual(definition), 'refsFalse': actual(refs_false), 'refsTrue': actual(refs_true)}, 'fcsExpected': {'definition': [mapped(fcs_row['symbol']['declaration'])], 'uses': sorted(mapped(x['range']) for x in fcs_row['projectReferences'] if not x['isDefinition']), 'all': sorted(mapped(x['range']) for x in fcs_row['projectReferences'])}, 'fcsSymbol': fcs_row['symbol']}
                report['rows'].append(row)
                save()
    finally:
        session.close()
        report['ownedProcessesClosed'] = True
        save()
    report['queryLocations'] = len(report['rows'])
    report['limits'] = 'Genuine successful SDK400 build, full-project FCS diagnostics and actual Ionide8.1-bundled FSAC0.86 stdio requests. Cross-file caller labels retained. No native or editor host result in this report; no performance claim. includeDeclaration false and true raw arms retained.'
    save()
    print(json.dumps({'report': str(out / 'report.json'), 'queryLocations': report['queryLocations']}))


if __name__ == '__main__':
    main()
