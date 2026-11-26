#!/usr/bin/env python3
"""Packaged actual-key F12 tests in a disposable controlled VS Code profile.
No downloads, builds, project preparation, normal-profile installs or provider-query substitutes.
Run through `cli/bin/run test e2e` with explicit paths, or inside the existing display wrapper.
"""
import argparse, hashlib, json, os, re, shutil, signal, socket, subprocess, tempfile, zipfile
from pathlib import Path
HERE = Path(__file__).resolve().parent

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def relative_file(value):
    p = Path(value)
    if p.is_absolute() or '..' in p.parts or not p.parts:
        raise ValueError('Fixture files must be nonempty safe relative paths')
    return p

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('code', 'vsix', 'manifest', 'output'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    code, vsix, manifest_path, output = (getattr(args, n).resolve() for n in ('code', 'vsix', 'manifest', 'output'))
    assert not (code.parent/'data').exists(), 'Portable Code data directory would override isolation'
    editor_version = json.loads((code.parent/'resources/app/package.json').read_text())['version']
    manifest = json.loads(manifest_path.read_text())
    assert manifest['schemaVersion'] == 1 and manifest['cases'] and manifest['fixtures']
    fixture_ids = [f['id'] for f in manifest['fixtures']]
    assert len(fixture_ids) == len(set(fixture_ids)) and all(re.fullmatch(r'[A-Za-z0-9_-]+', x) for x in fixture_ids)
    source_root = Path(os.environ['FSHARP_E2E_SOURCE_ROOT']).resolve() if os.environ.get('FSHARP_E2E_SOURCE_ROOT') else None
    if source_root:
        assert source_root.is_dir() and len(manifest['fixtures']) == 1
        assert not any(c.get('edit') or c.get('operation') in ('referenceFreshness', 'definitionFreshness') for c in manifest['cases']), 'Original workspace sources are read-only'
        assert not any(f.get('externalAsset') for f in manifest['fixtures'])
    external = [f for f in manifest['fixtures'] if f.get('externalAsset')]
    descriptor_path = Path(os.environ['FSHARP_E2E_PREPARED_DESCRIPTOR']).resolve() if external and os.environ.get('FSHARP_E2E_PREPARED_DESCRIPTOR') else None
    assert not external or descriptor_path, 'Prepared asset manifest requires explicit FSHARP_E2E_PREPARED_DESCRIPTOR'
    prepared = json.loads(descriptor_path.read_text()) if descriptor_path else None
    if prepared:
        assert len(external) == len(manifest['fixtures']) == 1, 'Prepared assets must use a separate one-root suite'
        assert prepared['schemaVersion'] == 1 and external[0]['externalAsset'] == prepared['asset']
        assert set(prepared['settings']) == {'libraryCatalogPath', 'builtinEnvironmentPath', 'evaluatedModelPath'}
        assert all(value in prepared['hashes'] for value in prepared['settings'].values()), 'All configured assets must have pinned hashes'
        assert {key: prepared['hashes'][value] for key, value in prepared['settings'].items()} == external[0]['assetPins'], 'Descriptor assets differ from public immutable catalog/context pins'
        assert not any(c.get('edit') for c in manifest['cases']), 'Prepared original sources are read-only'
    for fixture in manifest['fixtures']:
        assert fixture.get('externalAsset') or fixture['project'] in fixture['files']
        if fixture.get('workspaceFolder'): relative_file(fixture['workspaceFolder'])
        for name, text in fixture['files'].items(): relative_file(name); assert isinstance(text, str)
    for case in manifest['cases']:
        fixture = next(f for f in manifest['fixtures'] if f['id'] == case['fixture'])
        assert case['expected']['kind'] in ('exact', 'candidates', 'unavailable', 'incomplete', 'external', 'suppressed', 'ordered-tree')
        if case.get('operation') != 'projectTree': assert case['query']['file'] in fixture['files']
        if case.get('operation') == 'references': assert case['route'] in ('sidebar', 'peek')
        if case.get('edit'):
            assert case['edit']['file'] in fixture['files'] and isinstance(case['edit']['text'], str)
            if case['edit'].get('baseline'):
                assert case['edit']['baseline']['query']['file'] in fixture['files']
                assert case['edit']['baseline']['expected']['kind'] in ('exact', 'candidates', 'unavailable')
        if case.get('expectedFailure'):
            assert case['expectedFailure'].get('reason') and case['expectedFailure'].get('issue') and case['expectedFailure'].get('match'), 'Expected failures require a scoped reason and error match'
    output.mkdir(parents=True, exist_ok=False)
    inputs = [code, vsix, manifest_path, HERE/'run.py', HERE/'host.cjs', HERE/'renderer.cjs', HERE/'referenceUi.cjs', HERE/'referenceFreshness.cjs', HERE/'hoverUi.cjs', HERE/'treeUi.cjs', HERE/'definitionFreshness.cjs']
    if descriptor_path: inputs.append(descriptor_path)
    hashes = {str(p): digest(p) for p in inputs}
    for p in [manifest_path, HERE/'run.py', HERE/'host.cjs', HERE/'renderer.cjs', HERE/'referenceUi.cjs', HERE/'referenceFreshness.cjs', HERE/'hoverUi.cjs', HERE/'treeUi.cjs', HERE/'definitionFreshness.cjs']:
        shutil.copy2(p, output/p.name)
    if descriptor_path: shutil.copy2(descriptor_path, output/'prepared-descriptor.json')
    with zipfile.ZipFile(vsix) as archive:
        package = json.loads(archive.read('extension/package.json'))
        assert package['publisher'] + '.' + package['name'] == 'bentefay.fsharp-navigator'
        bundled = {name: hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist() if name.startswith('extension/bin/') and name.endswith('/fsharp-navigator')}
    report = {'schemaVersion': 1, 'manifestSuite': manifest.get('suite'), 'inputsBefore': hashes,
              'packageVersion': package['version'], 'editorVersion': editor_version, 'bundledNativeHashes': bundled, 'cases': [],
              'route': 'Disposable packaged editor with Chromium sandbox enabled; fixture trust is explicit. No sandbox fallback. Not general platform qualification.',
              'noPreparation': 'No model, catalog, snapshot, index command, build, restore or definition-provider API calls are made by this runner.'}
    env = {k: v for k, v in os.environ.items() if not k.startswith(('VSCODE_', 'ELECTRON_')) and k != 'NODE_OPTIONS'
           and not re.search(r'(^|_)(TOKEN|PASSWORD|PASSWD|SECRET|CREDENTIAL|AUTHORIZATION|API_KEY|COOKIE)($|_)', k)
           and k not in {'SSH_AUTH_SOCK', 'GIT_ASKPASS', 'SSH_ASKPASS', 'BASH_ENV', 'ENV', 'FSHARP_BACKEND_PATH', 'FSHARP_TEST_VSIX'}}
    if prepared and prepared.get('environment'):
        assert set(prepared['environment']) == {'DOTNET_ROOT'}, 'Only certified SDK spelling may be restored'
        sdk = prepared['environment']['DOTNET_ROOT']
        assert Path(sdk).resolve() == Path(env['DOTNET_ROOT']).resolve(), 'Prepared SDK must be the already approved physical SDK'
        model = json.loads(Path(prepared['settings']['evaluatedModelPath']).read_text())
        sdk_hash = hashlib.sha256(sdk.encode()).hexdigest()
        for project in model['projects']:
            witnesses = [entry for entry in project['environmentCertificate']['inputs'] if entry.get('name') == 'DOTNET_ROOT']
            assert len(witnesses) == 1 and witnesses[0]['sha256'] == sdk_hash, 'SDK spelling must match every original environment certificate'
        report['preparedEnvironmentRestoration'] = {'name': 'DOTNET_ROOT', 'original': env['DOTNET_ROOT'], 'restored': sdk, 'sha256': sdk_hash, 'samePhysicalSDK': True}
        env['DOTNET_ROOT'] = sdk
    def verify_prepared():
        if not prepared: return None
        observed = {name: digest(name) for name in prepared['hashes']}
        assert observed == prepared['hashes'], 'Prepared provenance or input hash changed'
        assert all(not Path(name).exists() for name in prepared['missingPaths']), 'Prepared missing-input witness changed'
        assert all(Path(name).is_dir() for name in prepared['directories']), 'Prepared directory witness changed'
        return {'hashes': observed, 'missingPaths': prepared['missingPaths'], 'directories': prepared['directories'], 'verified': True}
    def source_inventory():
        if not source_root: return None
        names = subprocess.check_output(['git', '-C', str(source_root), 'ls-files', '-z', '--cached', '--others', '--exclude-standard']).split(b'\0')
        return {name.decode(): digest(source_root/name.decode()) for name in sorted(set(names)) if name and Path(name.decode()).suffix.lower() in ('.fs', '.fsi', '.fsx', '.fsproj', '.props', '.targets', '.sln', '.slnx') and (source_root/name.decode()).is_file()}
    try:
        if source_root: report['sourceInputsBefore'] = source_inventory()
        if prepared:
            report['preparedInputsBefore'] = verify_prepared()
            report['preparationBoundary'] = 'Explicit opt-in original path-bound prepared root and genuine catalog/environment/model; no certificate relocation, edits, build, restore or helper invocation. New native index/cache lives in disposable extension storage.'
            report['noPreparation'] = 'This run consumes existing explicitly prepared assets. It does not establish default unprepared hover behavior.'
        with tempfile.TemporaryDirectory(prefix='fsnav-packaged-e2e-') as td:
            temp = Path(td); roots = {}; workspace_roots = {}; fixtures = temp/'fixtures'; fixtures.mkdir()
            for key in ('XDG_CACHE_HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'XDG_RUNTIME_DIR', 'TMPDIR'):
                # Unix-domain sockets have a short pathname limit. Keep the
                # private runtime directory short without reusing user storage.
                private = temp/('r' if key == 'XDG_RUNTIME_DIR' else key.lower()); private.mkdir(mode=0o700); env[key] = str(private)
            env['TMP'] = env['TMPDIR']; env['TEMP'] = env['TMPDIR']
            report['privateEnvironmentDirectories'] = {key: env[key] for key in ('XDG_CACHE_HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'XDG_RUNTIME_DIR', 'TMPDIR')}
            report['fixtureAncestors'] = [str(p) for p in fixtures.parents]
            for fixture in manifest['fixtures']:
                if source_root:
                    for name, text in fixture['files'].items():
                        assert (source_root/relative_file(name)).read_text() == text, 'Workspace source differs from authored snapshot'
                    roots[fixture['id']] = str(source_root); workspace_roots[fixture['id']] = str(source_root)
                    continue
                if fixture.get('externalAsset'):
                    root = Path(prepared['root']).resolve(); assert root.is_dir() and (root/relative_file(fixture['project'])).is_file()
                    for name, text in fixture['files'].items():
                        assert (root/relative_file(name)).read_text() == text, 'Prepared source differs from authored snapshot'
                        assert str(root/relative_file(name)) in prepared['hashes'], 'Prepared query source must be hash-pinned'
                    roots[fixture['id']] = str(root); workspace_roots[fixture['id']] = str(root)
                    continue
                root = fixtures/fixture['id']; root.mkdir(); roots[fixture['id']] = str(root)
                workspace_roots[fixture['id']] = str(root/fixture.get('workspaceFolder', '.'))
                for name, text in fixture['files'].items():
                    dest = root/relative_file(name); dest.parent.mkdir(parents=True, exist_ok=True); dest.write_text(text)
            source_hashes = {str(p.relative_to(fixtures)): digest(p) for p in fixtures.rglob('*') if p.is_file()}
            shutil.copytree(fixtures, output/'fixtures')
            assert all(Path(folder).is_dir() for folder in workspace_roots.values()), 'Declared workspace folders must exist'
            report['fixtureRoots'] = roots; report['workspaceRoots'] = workspace_roots
            workspace = temp/'Suite.code-workspace'; workspace.write_text(json.dumps({'folders': [{'name': k, 'path': v} for k, v in workspace_roots.items()]}))
            user, extensions, probe = temp/'user', temp/'extensions', temp/'probe'
            (user/'User').mkdir(parents=True); extensions.mkdir(); probe.mkdir()
            settings = {'telemetry.telemetryLevel': 'off', 'update.mode': 'none', 'extensions.autoUpdate': False,
                        'extensions.autoCheckUpdates': False, 'workbench.enableExperiments': False,
                        'workbench.startupEditor': 'none', 'window.restoreWindows': 'none',
                        'security.workspace.trust.startupPrompt': 'never', 'task.allowAutomaticTasks': 'off', 'fsharpNavigator.mode': 'indexed'}
            if prepared: settings.update({'fsharpNavigator.'+key:value for key,value in prepared['settings'].items()})
            (user/'User/settings.json').write_text(json.dumps(settings)); report['settings'] = settings
            (probe/'package.json').write_text(json.dumps({'name': 'packaged-e2e-probe', 'publisher': 'test', 'version': '0.0.0', 'engines': {'vscode': '^1.108.0'}, 'main': 'index.js', 'activationEvents': ['*']}))
            (probe/'index.js').write_text('exports.activate = () => {};')
            flags = ['--force-disable-user-env', '--user-data-dir', str(user), '--extensions-dir', str(extensions)]
            if int(editor_version.split('.')[1]) >= 131:
                flags += ['--shared-data-dir', str(temp/'shared-data')]
            launcher = code.parent/'bin/code'
            with (output/'install.log').open('w') as log:
                subprocess.run([str(launcher), *flags, '--install-extension', str(vsix), '--do-not-include-pack-dependencies'], env=env, check=True, timeout=120, stdout=log, stderr=subprocess.STDOUT)
            installed = json.loads((extensions/'extensions.json').read_text())
            assert {item['identifier']['id'].lower() for item in installed} == {'bentefay.fsharp-navigator'}
            report['installedExtensionIds'] = [item['identifier']['id'] for item in installed]
            installed_root = next(p.parent for p in extensions.glob('*/package.json') if json.loads(p.read_text()).get('name') == 'fsharp-navigator')
            report['installedExtensionPath'] = str(installed_root)
            report['installedNativeHashes'] = {name: digest(installed_root/Path(name).relative_to('extension')) for name in bundled}
            assert report['installedNativeHashes'] == bundled, 'Installed native differs from immutable VSIX'
            with socket.socket() as sock: sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
            run_env = {**env, 'FSNAV_E2E_MANIFEST': str(manifest_path), 'FSNAV_E2E_ROOTS': json.dumps(roots),
                       'FSNAV_E2E_RESULT': str(output/'host.json'), 'FSNAV_E2E_PORT': str(port), 'FSHARP_NAVIGATOR_TRACE': '0'}
            command = [str(code), str(workspace), *flags, '--disable-workspace-trust', '--disable-gpu',
                       '--disable-updates', '--disable-telemetry', '--skip-welcome', '--skip-release-notes',
                       '--ozone-platform=x11', f'--remote-debugging-port={port}', '--remote-debugging-address=127.0.0.1',
                       '--extensionDevelopmentPath='+str(probe), '--extensionTestsPath='+str(HERE/'host.cjs')]
            report['command'] = command
            with (output/'editor.log').open('w') as log:
                child = subprocess.Popen(command, env=run_env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try: report['editorExitCode'] = child.wait(timeout=180)
                except subprocess.TimeoutExpired: report['timedOut'] = True
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try: child.wait(timeout=5)
                        except subprocess.TimeoutExpired: os.killpg(child.pid, signal.SIGKILL); child.wait()
            if source_root:
                report['sourceInputsAfter'] = source_inventory()
                assert report['sourceInputsBefore'] == report['sourceInputsAfter'], 'Original workspace sources changed during the run'
            report['fixtureHashesBefore'] = source_hashes
            report['fixtureHashesAfter'] = {str(p.relative_to(fixtures)): digest(p) for p in fixtures.rglob('*') if p.is_file()}
            report['fixturesUnchanged'] = report['fixtureHashesAfter'] == source_hashes
            if (output/'host.json').exists(): report['host'] = json.loads((output/'host.json').read_text()); report['cases'] = report['host']['cases']
    except Exception as error:
        report['runnerError'] = str(error)
    finally:
        if prepared:
            try: report['preparedInputsAfter'] = verify_prepared()
            except Exception as error: report['preparedInputError'] = str(error)
        report['inputsAfter'] = {str(p): digest(p) for p in inputs}; report['inputsUnchanged'] = hashes == report['inputsAfter']
        report['passed'] = bool(report.get('editorExitCode') == 0 and report.get('fixturesUnchanged') and report['inputsUnchanged'] and report.get('host', {}).get('passed') and (not prepared or report.get('preparedInputsAfter', {}).get('verified')))
        (output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'passed': report['passed'], 'cases': [{'id': c['id'], 'status': c['status']} for c in report['cases']], 'output': str(output)}))
    raise SystemExit(0 if report['passed'] else 1)

if __name__ == '__main__': main()
