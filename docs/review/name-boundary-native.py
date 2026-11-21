#!/usr/bin/env python3
"""Prepared edit replay of compiler-verified function/operator name boundaries."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--binary', type=Path)
    ap.add_argument('--helper', type=Path)
    ap.add_argument('--prepared', type=Path)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    spec = importlib.util.spec_from_file_location('prep', ROOT / 'docs/review/prepared-replay.py')
    prep = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prep)
    native = prep.load('native', ROOT / 'docs/review/native-smoke-replay.py')
    if args.prepared:
        report = json.loads(args.prepared.read_text())
        base = Path(report['fixtureRoot'])
        root = base / 'project'
    else:
        assert args.helper
        prep.HELPER = args.helper.resolve()
        base = Path(tempfile.mkdtemp(prefix='name-boundary-native-', dir=ROOT / '.tmp'))
        cases = []
        for family in ['operator-name-absence', 'function-binding-boundary']:
            previous = json.loads((ROOT / ('docs/review/' + family + '-oracle.json')).read_text())
            for case in previous['cases']:
                prefix = case['source'].split('let marker=42\n')[0]
                positive = case['expectedStdout'] == 'marker'
                source = 'module Case\ntype Sha = string\n' + prefix + ('' if positive else 'let Sha = 42\n') + 'let observed = nameof Sha\n'
                path = base / (family + '-' + case['case'] + '.fsx')
                path.write_text(source)
                line = source.splitlines().index('let observed = nameof Sha')
                cases.append(dict(case=family + '-' + case['case'], source=source, path=str(path), expectedIntrinsic=positive, queries=[dict(label='alias', token='Sha', line=1, character=5), dict(label='argument', token='Sha', line=line, character=22), dict(label='callee', token='nameof', line=line, character=15)]))
        manifest = base / 'oracle-input.json'
        manifest.write_text(json.dumps(cases))
        process = subprocess.run([str(prep.DOTNET), 'fsi', '--exec', str(ROOT / 'docs/review/alias-expression-oracle.fsx'), str(manifest)], env=prep.ENV, text=True, capture_output=True, timeout=120)
        oracles = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{')]
        assert process.returncode == 0 and len(oracles) == len(cases), process.stderr
        assert all(row.get('valid') for row in oracles), [(row['case'], row.get('diagnostics')) for row in oracles if not row.get('valid')]
        for case, oracle in zip(cases, oracles):
            assert case['case'] == oracle['case']
            alias = next(row for row in oracle['results'] if row['label'] == 'alias')
            assert len(alias['references']) == (2 if case['expectedIntrinsic'] else 1), (case, alias)
            case['oracle'] = oracle
        root = native.fixture(base, 'project', {'Main.fs': cases[0]['source']})
        build = prep.build(root / 'Test.fsproj')
        model = prep.helper('workspace/evaluate', {'root': str(root)})
        model_path = base / 'model.json'
        model_path.write_text(json.dumps(model))
        refs = sorted({row['path'] for project in model['projects'] for row in project['referenceBindings'] if not row['isWorkspace']})
        owned = sorted({path for project in model['projects'] for path in project['workspaceAssemblyReferences']})
        environment = prep.helper('metadata/environment', {'referencePaths': refs, 'workspaceAssemblyPaths': owned})
        environment_path = base / 'environment.json'
        environment_path.write_text(json.dumps(environment))
        report = dict(provenance='Actual FCS checks all21 valid alias/nameof variants; first variant real SDK build; genuine evaluated model and full f215 schema2 environment. Cases change only the versioned Main.fs overlay in one source context. No performance claim.', fixtureRoot=str(base), modelPath=str(model_path), environmentPath=str(environment_path), helperSha256=hashlib.sha256(args.helper.read_bytes()).hexdigest(), build=build, cases=cases, fcsProcess=dict(exitCode=process.returncode, stderr=process.stderr))
    if args.binary:
        os.environ.update(prep.ENV)
        daemon = native.Daemon(args.binary.resolve(), root)
        daemon.call('workspace/open', dict(root=str(root), mode='indexed', cacheDir=str(base / 'cache'), evaluatedModelPath=report['modelPath'], builtinEnvironmentPath=report['environmentPath']))
        results = []
        for version, case in enumerate(report['cases'], 1):
            daemon.call('document/open' if version == 1 else 'document/change', dict(path=str(root / 'Main.fs'), text=case['source'], version=version))
            for query in case['queries']:
                if query['label'] == 'callee':
                    continue
                oracle = next(row for row in case['oracle']['results'] if row['label'] == query['label'])
                for operation in ['references', 'definition']:
                    response = daemon.call('navigation/' + operation, dict(path=str(root / 'Main.fs'), line=query['line'], character=query['character'], version=version, includeDeclaration=True))
                    if operation == 'references':
                        ranges = [row['range'] for row in oracle['references']]
                    else:
                        ranges = [oracle['symbol']['declaration']]
                    expected = [native.location(root / 'Main.fs', row['startLine'], row['startCharacter'], row['endCharacter'] - row['startCharacter']) for row in ranges]
                    value = response.get('result', {})
                    exact = value.get('completeness') == 'exact'
                    same = native.normalized(value.get('locations', [])) == native.normalized(expected) and value.get('version') == version and value.get('documentVersion') == version
                    results.append(dict(case=case['case'], requestedVersion=version, label=query['label'], operation=operation, status='pass' if exact and same else ('wrong-exact' if exact else 'unavailable'), expectedLocations=expected, response=response))
        stats = daemon.call('workspace/stats')['result']
        assert stats['compilerLaunches'] == stats['helperStarts'] == 0, stats
        daemon.close()
        report.update(binarySha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(), results=results, transcript=daemon.transcript, stats=stats, summary={status: sum(row['status'] == status for row in results) for status in ['pass', 'wrong-exact', 'unavailable']})
        print(report['summary'])
        for row in results:
            if row['status'] == 'wrong-exact':
                print(row['case'], row['label'], row['operation'], row['response'])
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(args.output)


if __name__ == '__main__':
    main()
