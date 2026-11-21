#!/usr/bin/env python3
"""Compiler-valid signature pairing across physical directories and Link aliases."""
import argparse, hashlib, json, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'semantic/tests'))
import smoke
from resolution_states import Native
sys.path.insert(0, str(ROOT / 'evaluation'))
from run import Session, canonical_location

CASES = [
    ('different_directories_same_basename', 'Signatures/Local.fsi', 'Sources/Local.fs', None),
    ('same_basename_different_link_aliases', 'Signatures/Local.fsi', 'Sources/Local.fs', ('View/Signature.fsi', 'Other/Implementation.fs')),
    ('different_basenames_same_module', 'Signature.fsi', 'Implementation.fs', None),
    ('different_basenames_same_link_basename', 'Signature.fsi', 'Implementation.fs', ('Logical/Local.fsi', 'Logical/Local.fs')),
    ('direct_function_pattern', 'Local.fsi', 'Local.fs', None),
]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--helper', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--case', choices=[row[0] for row in CASES])
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    smoke.HELPER = args.helper.resolve()
    report = {'kind': 'physical-signature-pairing-validity', 'binary': str(args.binary.resolve()), 'binarySha256': sha(args.binary), 'helperSha256': sha(args.helper), 'rows': []}
    save = lambda: (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    for name, sig_name, impl_name, links in CASES:
        if args.case and name != args.case:
            continue
        directory = out / name
        directory.mkdir()
        sig, impl = directory / sig_name, directory / impl_name
        impl_source = 'module Local\nlet f = function x -> x+x\n' if name == 'direct_function_pattern' else 'module Local\nlet f x=x+x\n'
        for path, source in [(sig, 'module Local\nval f : x:int -> int\n'), (impl, impl_source)]:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(source)
        include = ''.join(f'<Compile Include="{p}">' + (f'<Link>{links[i]}</Link>' if links else '') + '</Compile>' for i, p in enumerate([sig_name, impl_name]))
        project = directory / 'Pairing.fsproj'
        smoke.project(project, include)
        (directory / 'global.json').write_text(json.dumps({'sdk': {'version': '10.0.400', 'rollForward': 'disable', 'allowPrerelease': False}}) + '\n')
        build = subprocess.run([str(smoke.DOTNET), 'build', str(project), '--nologo', '-v:q'], env=smoke.ENV, cwd=directory, text=True, capture_output=True, timeout=120)
        row = {'case': name, 'signature': str(sig), 'implementation': str(impl), 'links': links, 'sourceHashes': {sig_name: sha(sig), impl_name: sha(impl)}, 'actualSdkBuild': {'exitCode': build.returncode, 'stdout': build.stdout, 'stderr': build.stderr}, 'validBuild': build.returncode == 0}
        report['rows'].append(row)
        save()
        if build.returncode:
            print(name, 'invalid build', flush=True)
            continue
        cases = [{'case': name + suffix, 'path': str(path), 'source': path.read_text(), 'projectFiles': [str(sig), str(impl)], 'queries': [{'line': 1, 'character': path.read_text().splitlines()[1].rindex('x'), 'token': 'x', 'label': suffix + ':x'}]} for suffix, path in [('-signature', sig), ('-implementation', impl)]]
        manifest = directory / 'oracle-manifest.json'
        manifest.write_text(json.dumps(cases))
        fcs = subprocess.run([str(smoke.DOTNET), 'fsi', '--exec', str(HERE / 'oracle.fsx'), str(manifest)], env=smoke.ENV, cwd=ROOT, text=True, capture_output=True, timeout=120)
        row['fcs'] = {'exitCode': fcs.returncode, 'stderr': fcs.stderr, 'driverSha256': sha(HERE / 'oracle.fsx'), 'results': [json.loads(line) for line in fcs.stdout.splitlines() if line.startswith('{')]}
        save()
        assert fcs.returncode == 0 and len(row['fcs']['results']) == 2 and all(x['valid'] for x in row['fcs']['results']), row
        model = smoke.request('workspace/evaluate', {'root': str(directory), 'projects': [str(project)]})
        assert model['completeness'] == 'exact' and all(p['membershipReusable'] for p in model['projects']), model
        model_path = directory / 'model.json'
        model_path.write_text(json.dumps(model, indent=2) + '\n')
        row['model'] = {'path': str(model_path), 'sha256': sha(model_path)}
        session = Session([sys.executable, str(ROOT / 'evaluation/fsac_adapter.py'), '--ionide', str(ROOT / 'evaluation/.runtime/extensions/Ionide.Ionide-fsharp-8.1.0'), '--dotnet', str(smoke.DOTNET), '--timeout', '60', '--trace', str(directory / 'fsac-trace.jsonl')], timeout=90, memory_limit_mib=2048)
        native = Native(args.binary.resolve())

        def call(method, params):
            response, _ = session.call(method, params)
            assert 'error' not in response, response
            return response['result']

        try:
            row['fsacOpen'] = call('workspace/open', {'root': str(directory), 'cacheDir': str(directory / 'fsac-cache'), 'projects': [str(project)]})
            row['nativeOpen'] = native.call('workspace/open', {'root': str(directory), 'cacheDir': str(directory / 'native-cache'), 'mode': 'indexed', 'evaluatedModelPath': str(model_path)})
            row['queries'] = []
            for case in cases:
                path = Path(case['path'])
                opened = {'path': str(path), 'text': path.read_text(), 'version': 1}
                call('document/open', opened)
                native.call('document/open', opened)
                query = {'path': str(path), 'line': 1, 'character': case['queries'][0]['character'], 'version': 1}
                item = {'query': query, 'case': case['case'], 'operations': {}}
                for method, flag in [('navigation/definition', None), ('navigation/references', False), ('navigation/references', True)]:
                    params = {**query, **({'includeDeclaration': flag} if flag is not None else {})}
                    fsac = call(method, params)
                    answer = native.call(method, params)
                    actual = answer.get('result', {})
                    canon = lambda result: sorted(list(canonical_location(x, directory)) for x in result.get('locations', []))
                    item['operations'][method + ':' + str(flag)] = {'fsac': fsac, 'native': answer, 'fsacCanonical': canon(fsac), 'nativeCanonical': canon(actual)}
                row['queries'].append(item)
                save()
        finally:
            session.close()
            native.close()
        print(name, 'valid, queried', flush=True)
        save()
    report['limits'] = 'Actual SDK400 build gates validity; full-project FCS diagnostics retained. Only successful builds evaluated with genuine09f and queried via actual bundled FSAC0.86 plus immutable native. Link aliases never substituted for physical paths. No performance claim or actual editor host.'
    save()
    print(json.dumps({'report': str(out / 'report.json'), 'valid': sum(row['validBuild'] for row in report['rows']), 'total': len(report['rows'])}))


if __name__ == '__main__':
    main()
