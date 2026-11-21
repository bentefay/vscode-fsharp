#!/usr/bin/env python3
"""Real compiler oracles and native regressions for Resolved/Absent/Unknown.

Prepare once, then replay immutable native candidates without rebuilding:
  scripts/with-local-tools python3 semantic/tests/resolution_states.py --prepare-only --helper PATH --output REPORT
  scripts/with-local-tools python3 semantic/tests/resolution_states.py --prepared DIR --binary PATH --output REPORT

All builds/exports run outside the native daemon. Missing/incomplete answers are
safety passes only where specified; the positive control requires exact hover.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import smoke

ROOT = smoke.ROOT


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def compiler_project(path, source, reference=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    (path.parent / 'Main.fs').write_text(source)
    extra = '' if reference is None else f'<Reference Include="{reference.stem}"><HintPath>{reference}</HintPath></Reference>'
    smoke.project(path, '<Compile Include="Main.fs"/>', extra)
    path.write_text(path.read_text().replace('</PropertyGroup>', '<OutputType>Exe</OutputType></PropertyGroup>'))


def build(path):
    return smoke.run('build', path, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')


def program(prefix='', owner=False):
    lines = ['module Main', prefix.rstrip(), 'let value = System.String.IsNullOrEmpty("")']
    if owner:
        lines += ['let owner = typeof<System.String>.Assembly.GetName().Name']
    lines += ['[<EntryPoint>]', 'let main _ =', '    printfn "%s:%b" owner value' if owner else '    printfn "%b" value', '    0']
    return '\n'.join(line for line in lines if line) + '\n'


def prepare(helper):
    (ROOT / '.tmp').mkdir(exist_ok=True)
    base = Path(tempfile.mkdtemp(prefix='resolution-states-', dir=ROOT / '.tmp'))
    # Freeze the helper, FCS and logger dependencies for all certificates/oracles.
    runtime = base / 'helper-runtime'
    shutil.copytree(helper.parent, runtime)
    smoke.HELPER = runtime / helper.name
    libraries = {
        'Collision': 'namespace System\ntype String =\n    /// Collision library returns false even for an empty string.\n    static member IsNullOrEmpty (_value:string) = false\n',
        'ModuleShadow': 'namespace ModuleShadow\nmodule System =\n    module String =\n        /// External module value, not the framework static method.\n        let IsNullOrEmpty (_value:string) = false\n',
        'ValueShadow': 'namespace ValueShadow\ntype StringProxy = { IsNullOrEmpty: string -> bool }\ntype SystemProxy = { String: StringProxy }\n[<AutoOpen>]\nmodule Values =\n    /// External value shadows the namespace in expression position.\n    let System = { String = { IsNullOrEmpty = fun _ -> false } }\n',
    }
    dlls = {}
    report = {'fixtureRoot': str(base), 'helperSha256': hashlib.sha256(smoke.HELPER.read_bytes()).hexdigest(), 'dotnetVersion': smoke.run('--version').strip(), 'libraries': {}, 'programs': {}}
    for name, source in libraries.items():
        directory = base / 'libraries' / name
        directory.mkdir(parents=True)
        (directory / 'Library.fs').write_text(source)
        project = directory / f'{name}.fsproj'
        smoke.project(project, '<Compile Include="Library.fs"/>')
        output = build(project)
        dlls[name] = directory / f'bin/Debug/net10.0/{name}.dll'
        report['libraries'][name] = {'source': source, 'build': output, 'assemblySha256': hashlib.sha256(dlls[name].read_bytes()).hexdigest()}
    specs = {
        'positive': (program(owner=True), None, 'System.Private.CoreLib:true'),
        'nominal': (program(owner=True), dlls['Collision'], 'Collision:false'),
        'module': (program('open ModuleShadow'), dlls['ModuleShadow'], 'false'),
        'value': (program('open ValueShadow'), dlls['ValueShadow'], 'false'),
        'source_module': (program('module System =\n    module String =\n        let IsNullOrEmpty (_value:string) = false'), None, 'false'),
        'source_value': (program('type StringProxy = { IsNullOrEmpty: string -> bool }\ntype SystemProxy = { String: StringProxy }\nlet System = { String = { IsNullOrEmpty = fun _ -> false } }'), None, 'false'),
    }
    manifests = {}
    for name, (source, reference, expected) in specs.items():
        project = base / name / 'Consumer.fsproj'
        compiler_project(project, source, reference)
        output = build(project)
        oracle = smoke.run(project.parent / 'bin/Debug/net10.0/Consumer.dll', cwd=project.parent).strip()
        assert oracle == expected, {'case': name, 'actual': oracle, 'expected': expected}
        print(f'ORACLE {name}: {oracle}', flush=True)
        model = smoke.request('workspace/evaluate', {'root': str(project.parent), 'projects': [str(project)]})
        assert model['completeness'] == 'exact', model
        for item in model['projects']:
            assert item.get('membershipReusable') is True and item.get('environmentCertificate'), item
        model_path = base / f'{name}-model.json'
        save(model_path, model)
        references = sorted({entry['path'] for item in model['projects'] for entry in item['referenceBindings'] if not entry['isWorkspace']})
        owned = sorted({path for item in model['projects'] for path in item.get('workspaceAssemblyReferences', [])})
        # Workspace exclusions are part of the certificate. Equal external
        # references do not permit reuse across different project outputs.
        environment = smoke.request('metadata/environment', {'referencePaths': references, 'workspaceAssemblyPaths': owned})
        assert environment['completeness'] == 'exact' and environment['typeNameInventory'] == 'all-external-type-declarations'
        environment_path = base / f'{name}-environment.json'
        save(environment_path, environment)
        line = next(i for i, value in enumerate(source.splitlines()) if value.startswith('let value ='))
        params = {'path': str(project.parent / 'Main.fs'), 'line': line, 'character': source.splitlines()[line].index('IsNullOrEmpty') + 1, 'projectPath': str(project)}
        manifests[name] = {'root': str(project.parent), 'model': str(model_path), 'environment': str(environment_path), 'references': references, 'params': params}
        report['programs'][name] = {'source': source, 'build': output, 'oracleStdout': oracle, 'expectedOracle': expected}
    refs = manifests['positive']['references']
    runtime_assembly = next(path for path in refs if Path(path).name == 'System.Runtime.dll')
    core_assembly = next(path for path in refs if Path(path).name == 'FSharp.Core.dll')
    catalog = smoke.request('metadata/export', {'assemblyPaths': [runtime_assembly, core_assembly], 'referencePaths': refs})
    catalog_path = base / 'framework-catalog.json'
    save(catalog_path, catalog)
    full_catalog = smoke.request('metadata/export', {'assemblyPaths': [runtime_assembly, core_assembly, str(dlls['Collision'])], 'referencePaths': manifests['nominal']['references']})
    full_catalog_path = base / 'collision-full-catalog.json'
    save(full_catalog_path, full_catalog)
    for name in ['positive', 'nominal']:
        value = json.loads(Path(manifests[name]['environment']).read_text())
        value['completeness'] = 'approximate'
        incomplete = base / f'{name}-incomplete-environment.json'
        save(incomplete, value)
        manifests[name]['incompleteEnvironment'] = str(incomplete)
    manifest = {'programs': manifests, 'catalog': str(catalog_path), 'collisionCatalog': str(full_catalog_path), 'helper': str(smoke.HELPER), 'helperSha256': report['helperSha256'], 'oracleReport': str(base / 'oracles.json')}
    save(base / 'manifest.json', manifest)
    save(base / 'oracles.json', report)
    return base


def extend_scope_controls(base):
    """Real positive unopened-reference controls and assembly AutoOpen shadow."""
    manifest=json.loads((base/'manifest.json').read_text())
    smoke.HELPER=Path(manifest['helper'])
    report=json.loads((base/'oracles.json').read_text())
    library=base/'libraries'/'AssemblyValueShadow';library.mkdir(exist_ok=True)
    source='namespace AssemblyValueShadow\ntype StringProxy = { IsNullOrEmpty: string -> bool }\ntype SystemProxy = { String: StringProxy }\nmodule Values =\n    let System = { String = { IsNullOrEmpty = fun _ -> false } }\n[<assembly: AutoOpen("AssemblyValueShadow.Values")>]\ndo ()\n'
    (library/'Library.fs').write_text(source);project=library/'AssemblyValueShadow.fsproj';smoke.project(project,'<Compile Include="Library.fs"/>');build_output=build(project)
    assembly=library/'bin/Debug/net10.0/AssemblyValueShadow.dll'
    report['libraries']['AssemblyValueShadow']={'source':source,'build':build_output,'assemblySha256':hashlib.sha256(assembly.read_bytes()).hexdigest()}
    specs=[]
    for key,owner in [('module','ModuleShadow'),('value','ValueShadow')]:
        reference=next(Path(p) for p in manifest['programs'][key]['references'] if Path(p).name==owner+'.dll')
        specs.append(('unopened_'+key,reference,'true','exact_runtime',key))
    specs.append(('assembly_auto_open',assembly,'false','blocked',None))
    extras=[]
    for name,reference,expected,expectation,reuse in specs:
        source=program();project=base/name/'Consumer.fsproj';compiler_project(project,source,reference);output=build(project)
        actual=smoke.run(project.parent/'bin/Debug/net10.0/Consumer.dll').strip();assert actual==expected,(name,actual,expected)
        model=smoke.request('workspace/evaluate',{'root':str(project.parent),'projects':[str(project)]});assert all(p['membershipReusable'] and p['environmentCertificate']['version']==4 for p in model['projects'])
        model_path=base/(name+'-model.json');save(model_path,model)
        refs=sorted({b['path'] for p in model['projects'] for b in p['referenceBindings'] if not b['isWorkspace']})
        owned=sorted({path for p in model['projects'] for path in p.get('workspaceAssemblyReferences',[])})
        environment=smoke.request('metadata/environment',{'referencePaths':refs,'workspaceAssemblyPaths':owned});environment_path=base/(name+'-environment.json');save(environment_path,environment)
        line=next(i for i,v in enumerate(source.splitlines()) if v.startswith('let value ='))
        manifest['programs'][name]={'root':str(project.parent),'model':str(model_path),'environment':str(environment_path),'references':refs,'params':{'path':str(project.parent/'Main.fs'),'line':line,'character':source.splitlines()[line].index('IsNullOrEmpty')+1,'projectPath':str(project)}}
        report['programs'][name]={'source':source,'build':output,'oracleStdout':actual,'expectedOracle':expected}
        extras.append([name,name,'complete',expectation]);print('ORACLE '+name+': '+actual,flush=True)
    manifest['extraCases']=extras;save(base/'manifest.json',manifest);save(base/'oracles.json',report)


def refresh_scope_environments(base):
    manifest=json.loads((base/'manifest.json').read_text());smoke.HELPER=Path(manifest['helper'])
    for name,program in manifest['programs'].items():
        if not(name.startswith('source_') or name.startswith('unopened_') or name=='assembly_auto_open'):continue
        model=json.loads(Path(program['model']).read_text())
        owned=sorted({path for p in model['projects'] for path in p.get('workspaceAssemblyReferences',[])})
        environment=smoke.request('metadata/environment',{'referencePaths':program['references'],'workspaceAssemblyPaths':owned})
        path=base/(name+'-owned-environment.json');save(path,environment);program['environment']=str(path)
        print('EXPORTED current ownership '+name,flush=True)
    save(base/'manifest.json',manifest)


class Native:
    def __init__(self, binary):
        self.process = subprocess.Popen([str(binary), 'daemon'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=smoke.ENV)
        self.serial = 0
        self.transcript = []

    def call(self, method, params):
        self.serial += 1
        message = {'id': self.serial, 'method': method, 'params': params}
        self.process.stdin.write(json.dumps(message) + '\n')
        self.process.stdin.flush()
        raw = self.process.stdout.readline()
        if not raw:
            raise AssertionError(f'Native process exited: {self.process.stderr.read()}')
        response = json.loads(raw)
        assert response.get('id') == self.serial, response
        self.transcript.append({'request': message, 'response': response})
        return response

    def close(self):
        if self.process.poll() is None:
            self.call('shutdown', {})
            self.process.wait(timeout=10)
        assert self.process.returncode == 0, self.process.stderr.read()


def replay(base, binary):
    manifest = json.loads((base / 'manifest.json').read_text())
    cases = [
        ('positive_complete', 'positive', 'complete', 'exact_runtime'),
        ('positive_missing_inventory', 'positive', 'missing', 'blocked'),
        ('positive_incomplete_inventory', 'positive', 'incomplete', 'blocked'),
        ('nominal_complete_subset_catalog', 'nominal', 'complete', 'blocked'),
        ('nominal_complete_full_catalog', 'nominal', 'full_catalog', 'blocked'),
        ('nominal_missing_inventory', 'nominal', 'missing', 'blocked'),
        ('nominal_incomplete_inventory', 'nominal', 'incomplete', 'blocked'),
        ('changed_references_old_inventory', 'nominal', 'old_references', 'blocked'),
        ('external_module_owner', 'module', 'complete', 'blocked'),
        ('external_value_owner', 'value', 'complete', 'blocked'),
        ('source_module_owner', 'source_module', 'complete', 'source_suppressed_or_blocked'),
        ('source_value_owner', 'source_value', 'complete', 'source_suppressed_or_blocked'),
        ('positive_recovery_after_reference_changes', 'positive', 'complete', 'exact_runtime'),
    ]
    cases.extend(manifest.get('extraCases', []))
    root = Path(tempfile.mkdtemp(prefix='native-replay-', dir=base))
    daemon = Native(binary)
    rows = []
    try:
        for name, program_name, variant, expectation in cases:
            program = manifest['programs'][program_name]
            # All transitions reuse the cache. A prior exact answer must not
            # survive a missing/incomplete/current-but-ambiguous owner proof.
            params = {'root': program['root'], 'mode': 'indexed', 'cacheDir': str(root / 'cache'), 'evaluatedModelPath': program['model'], 'metadataPath': manifest['collisionCatalog'] if variant == 'full_catalog' else manifest['catalog']}
            if variant != 'missing':
                params['builtinEnvironmentPath'] = (manifest['programs']['positive']['environment'] if variant == 'old_references' else program['incompleteEnvironment'] if variant == 'incomplete' else program['environment'])
            opened = daemon.call('workspace/open', params)
            assert 'error' not in opened, opened
            response = daemon.call('navigation/hover', program['params'])
            stats = daemon.call('workspace/stats', {})
            assert stats.get('result', {}).get('helperStarts') == 0 and stats['result'].get('compilerLaunches') == 0, stats
            result = response.get('result', {})
            exact = result.get('completeness') == 'exact'
            hover = result.get('hover')
            if expectation == 'exact_runtime':
                passed = exact and bool(hover) and result.get('symbol', {}).get('assemblyName') == 'System.Runtime'
            elif expectation == 'source_suppressed_or_blocked':
                passed = not hover and (not exact or result.get('ownership') in ['workspace', None])
            else:
                passed = not exact and not hover
            # Complete external-shadow controls must actually load a current
            # certificate; an unrelated ownership/setup rejection is not a
            # successful scoped-resolution regression.
            requires_loaded = name in ['external_module_owner', 'external_value_owner', 'assembly_auto_open']
            setup_valid = not requires_loaded or stats['result'].get('builtinEnvironmentLoaded') is True
            passed = passed and setup_valid
            row = {'name': name, 'expectation': expectation, 'passed': passed, 'setupValid': setup_valid, 'response': response, 'stats': stats['result']}
            rows.append(row)
            print(f'{"PASS" if passed else "FAIL"} {name}: {result.get("completeness", "error")}', flush=True)
    finally:
        daemon.close()
    report = {'kind': 'real-compiler-tristate-native-regression', 'binary': str(binary), 'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'helperSha256': manifest['helperSha256'], 'fixtureRoot': str(base), 'oracleReport': manifest['oracleReport'], 'rows': rows, 'transcript': daemon.transcript, 'passed': sum(row['passed'] for row in rows), 'total': len(rows), 'limits': 'Blocked cases count as safety regression passes, not feature coverage. Positive control requires exact System.Runtime hover. No timing or VS Code host claim.'}
    for key in ['modelHelper', 'modelHelperSha256', 'modelCertificateVersion', 'sourceFixture']:
        if key in manifest:
            report[key] = manifest[key]
    shutil.rmtree(root)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', type=Path, default=smoke.HELPER)
    parser.add_argument('--prepared', type=Path)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--extend-scope-controls', action='store_true')
    parser.add_argument('--refresh-scope-environments', action='store_true')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/fsharp-navigator')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    base = args.prepared.resolve() if args.prepared else prepare(args.helper.resolve())
    if args.extend_scope_controls:
        extend_scope_controls(base)
    if args.refresh_scope_environments:
        refresh_scope_environments(base)
    if args.prepare_only:
        report = {'prepared': str(base), 'manifest': str(base / 'manifest.json'), 'oracles': json.loads((base / 'oracles.json').read_text())}
    else:
        report = replay(base, args.binary.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    save(args.output, report)
    print(json.dumps({'output': str(args.output), 'fixtureRoot': str(base), 'passed': report.get('passed'), 'total': report.get('total')}), flush=True)
    if not args.prepare_only and report['passed'] != report['total']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
