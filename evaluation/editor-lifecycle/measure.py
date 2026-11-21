#!/usr/bin/env python3
"""Exercise one fixed local site through real Linux desktop providers and a same-profile restart."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from run import file_sha, process_identity, process_tree_details


def observe_editor(args, env, output):
    start = time.monotonic_ns()
    env = dict(env, FSNAV_LAUNCH_NS=str(start), FSNAV_QUERY_RESULT=str(output))
    owned = {}
    memory_samples = []
    process_names = set()
    timeout = False
    with output.with_suffix('.log').open('w') as log:
        proc = subprocess.Popen(args, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        _, stamp = process_identity(proc.pid)
        try:
            while proc.poll() is None:
                rss, pids, _, _, _ = process_tree_details(proc.pid, stamp, owned)
                memory_samples.append(((time.monotonic_ns() - start) / 1e6, rss))
                for pid in pids:
                    try:
                        process_names.add(Path(f'/proc/{pid}/comm').read_text().strip())
                    except (FileNotFoundError, ProcessLookupError):
                        pass
                if time.monotonic_ns() - start > 120_000_000_000:
                    timeout = True
                    break
                time.sleep(.05)
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
    query = json.loads(output.read_text()) if output.is_file() else None
    for sample in query.get('samples', []) if query and memory_samples else []:
        finished = sample['finishedMsFromLaunch']
        at_ms, rss = min(memory_samples, key=lambda point: abs(point[0] - finished))
        sample['nearestTreeRssKiB'] = rss
        sample['rssSampleOffsetMs'] = at_ms - finished
    return dict(exitCode=proc.returncode, timedOut=timeout, peakTreeRssKiB=max((r for _, r in memory_samples), default=None),
                sampleCount=len(memory_samples), processNames=sorted(process_names), processesObserved=len(owned), query=query)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--code', type=Path, required=True, help='Independently checksum-verified desktop binary')
    parser.add_argument('--provider', choices=['navigator', 'ionide'], required=True)
    parser.add_argument('--vsix', action='append', type=Path, required=True, help='Local reviewed packages, dependencies first')
    parser.add_argument('--workspace', type=Path, required=True, help='Restored copy of this directory’s fixture')
    parser.add_argument('--backend', type=Path, help='Frozen navigator executable')
    parser.add_argument('--model', type=Path, help='Current evaluated model for the same workspace')
    parser.add_argument('--output', type=Path, required=True, help='New directory outside the fixture')
    args = parser.parse_args()
    nav = args.provider == 'navigator'
    if nav and not (args.backend and args.model):
        parser.error('Navigator trials require --backend and --model')
    out, workspace, code = args.output.resolve(), args.workspace.resolve(), args.code.resolve()
    if out.is_relative_to(workspace):
        parser.error('Store measurement artifacts outside the fixture')
    for name in ('Lifecycle.fs', 'Lifecycle.fsproj'):
        if file_sha(workspace/name) != file_sha(HERE/'fixture'/name):
            parser.error('Source and project must match the fixed lifecycle fixture')
    out.mkdir(parents=True, exist_ok=False)
    packages = [path.resolve() for path in args.vsix]
    env = {k: v for k, v in os.environ.items() if not k.startswith(('VSCODE_', 'ELECTRON_')) and
           k not in {'NODE_OPTIONS', 'GH_TOKEN', 'GITHUB_TOKEN', 'VSCE_PAT', 'SSH_AUTH_SOCK', 'GIT_ASKPASS', 'SSH_ASKPASS'}}
    env['FSNAV_PROVIDER'] = 'bentefay.fsharp-navigator' if nav else 'Ionide.Ionide-fsharp'
    editor = json.loads((code.parent/'resources/app/package.json').read_text())['version']
    report = dict(editor=editor, provider=args.provider, codeSha256=file_sha(code),
                  vsixSha256={p.name: file_sha(p) for p in packages}, sourceSha256=file_sha(workspace/'Lifecycle.fs'),
                  projectSha256=file_sha(workspace/'Lifecycle.fsproj'), runs=[])
    if nav:
        report.update(backendSha256=file_sha(args.backend), modelSha256=file_sha(args.model))
    with tempfile.TemporaryDirectory(prefix='fsnav-editor-measure-') as temp:
        temp = Path(temp)
        user, extensions = temp/'user', temp/'extensions'
        (user/'User').mkdir(parents=True)
        extensions.mkdir()
        flags = ['--user-data-dir', str(user), '--extensions-dir', str(extensions)]
        if int(editor.split('.')[1]) >= 131:
            flags += ['--shared-data-dir', str(temp/'shared')]
        settings = {'telemetry.telemetryLevel': 'off', 'update.mode': 'none', 'extensions.autoUpdate': False,
                    'extensions.autoCheckUpdates': False, 'workbench.enableExperiments': False, 'files.hotExit': 'off',
                    'workbench.startupEditor': 'none', 'window.restoreWindows': 'none'}
        if nav:
            settings.update({'fsharpNavigator.mode': 'indexed', 'fsharpNavigator.backendPath': str(args.backend.resolve()),
                             'fsharpNavigator.evaluatedModelPath': str(args.model.resolve())})
        else:
            settings['FSharp.dotnetRoot'] = env['DOTNET_ROOT']
        (user/'User/settings.json').write_text(json.dumps(settings))
        for pkg in packages:
            subprocess.run([str(code.parent/'bin/code'), *flags, '--install-extension', str(pkg)],
                           env=env, check=True, timeout=120)
        command = [str(code), str(workspace), *flags, '--disable-workspace-trust', '--no-sandbox',
                   '--ozone-platform=x11', '--disable-gpu', '--disable-updates', '--disable-telemetry',
                   '--skip-welcome', '--skip-release-notes',
                   '--extensionDevelopmentPath=' + str(HERE.parent/'ionide-host'),
                   '--extensionTestsPath=' + str(HERE/'query.cjs')]
        for phase in ('coldProfile', 'restartedProfile'):
            result = observe_editor(command, env, out/(phase+'.json'))
            report['runs'].append(dict(phase=phase, **result))
            (out/'report.json').write_text(json.dumps(report, indent=2))
    for run in report['runs']:
        query = run['query']
        if (run['exitCode'] or not query or query.get('error') or not query.get('firstUsefulProviderMs')
                or (nav and (query['engine']['helperStarts'] != 0 or query['engine']['compilerLaunches'] != 0))
                or any(s['status'] != 'correct' for s in query['samples'] if s['stage'] != 'readinessRetry')):
            raise SystemExit('Incomplete or incorrect editor operation; inspect the retained report.')


if __name__ == '__main__':
    main()
