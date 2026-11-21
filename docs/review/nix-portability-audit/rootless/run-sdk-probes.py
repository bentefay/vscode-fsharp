"""Run only inside the already bootstrapped, task-local canonical Nix store.

No installer, profile-policy edits, host configuration changes or sandbox
relaxation. The preceding JSON reports retain bootstrap and derivation commands.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import signal
import subprocess
import time

audit = Path(__file__).resolve().parent
project = audit.parents[3]
base = project / '.tools/nix-probe'
task = base / 'rootless'
value = json.loads(json.loads((audit / 'pinned-derivation-probe.json').read_text())['stdout'])
sdk = value['sdkOutput']
prefix = [str(base / 'nix-user-chroot-1.2.2'), str(task / 'nix')]
fixture = task / 'sdk-fixture'
shutil.copytree(audit / 'fixture', fixture, dirs_exist_ok=True)
helper = project / 'evaluation/.runtime/helper-09f01156157a/FSharpNavigator.Semantic.dll'
assert hashlib.sha256(helper.read_bytes()).hexdigest() == '09f01156157a28020e42e3531382d50e8f97e0a778f8dc30470454d4b43bd3db'
env = dict(os.environ)
changes = {
    'TMPDIR': str(task / 'tmp'),
    'XDG_CACHE_HOME': str(task / 'cache'),
    'XDG_CONFIG_HOME': str(task / 'config'),
    'XDG_DATA_HOME': str(task / 'data'),
    'NIX_USER_CONF_FILES': '',
    'NIX_REMOTE': 'local',
    'NIX_CONFIG': 'experimental-features = nix-command flakes\nbuild-users-group =\n',
    'DOTNET_ROOT': sdk + '/share/dotnet',
    'DOTNET_CLI_HOME': str(task / 'dotnet-home'),
    'DOTNET_CLI_TELEMETRY_OPTOUT': '1',
    'DOTNET_SKIP_FIRST_TIME_EXPERIENCE': '1',
    'MSBUILDDISABLENODEREUSE': '1',
    'DOTNET_CLI_USE_MSBUILD_SERVER': '0',
    'FSHARP_NAVIGATOR_DOTNET': sdk + '/bin/dotnet',
}
env.update(changes)
results = []
operations = [
    ('SDK info', ['--info'], None, 20),
    ('restore tiny project', ['restore', str(fixture / 'P.fsproj'), '--nologo', '--ignore-failed-sources', '-p:NuGetAudit=false', '-p:UseSharedCompilation=false', '-nr:false'], None, 45),
    ('build tiny project', ['build', str(fixture / 'P.fsproj'), '--no-restore', '--nologo', '-p:UseSharedCompilation=false', '-nr:false'], None, 45),
    ('evaluate preparation profile', [str(helper), '--once'], json.dumps({'id': 1, 'method': 'workspace/evaluate', 'params': {'root': str(fixture)}}) + '\n', 60),
]
for label, args, data, timeout in operations:
    command = prefix + [sdk + '/bin/dotnet'] + args
    started = time.monotonic()
    child = subprocess.Popen(command, env=env, cwd=fixture, text=True,
                             stdin=subprocess.PIPE if data else subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=True)
    try:
        out, err = child.communicate(data, timeout=timeout)
        result = {'exitCode': child.returncode, 'stdout': out, 'stderr': err}
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGTERM)
        try:
            out, err = child.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            out, err = child.communicate()
        result = {'timeoutSeconds': timeout, 'stdout': out, 'stderr': err}
    result.update({'label': label, 'command': command, 'seconds': time.monotonic() - started})
    results.append(result)
    (audit / 'sdk-helper-probe.json').write_text(json.dumps({
        'operations': results, 'environmentChanges': changes,
        'ambientCultureAndProxyVariables': 'Inherited unchanged; no values exposed.',
        'hostStoreCreated': Path('/nix/store').exists(),
        'helperSha256': hashlib.sha256(helper.read_bytes()).hexdigest(),
        'boundary': 'Actual Nix SDK and tiny project; not full devenv, editor, or accepted Nix preparation profile.'
    }, indent=2) + '\n')
    print(label, result.get('exitCode', 'timeout'), flush=True)
    if result.get('exitCode') != 0:
        break
