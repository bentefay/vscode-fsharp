#!/usr/bin/env python3
"""JSONL bridge to real Ionide VS Code providers in an isolated test host.

Important: the process tree includes the entire editor, not just FSAC. Do not
compare its aggregate RSS directly with daemon-only memory as a service speedup.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--vscode', required=True)
    parser.add_argument('--extensions', required=True)
    parser.add_argument('--dotnet-root', required=True)
    args = parser.parse_args()
    connection = reader = process = None
    with tempfile.TemporaryDirectory(prefix='ionide-baseline-profile-') as directory:
        profile = Path(directory)
        settings = profile / 'User' / 'settings.json'
        settings.parent.mkdir()
        settings.write_text(json.dumps({'telemetry.telemetryLevel': 'off', 'update.mode': 'none',
            'extensions.autoUpdate': False, 'extensions.autoCheckUpdates': False,
            'security.workspace.trust.enabled': False, 'FSharp.dotnetRoot': str(Path(args.dotnet_root).resolve()),
            'FSharp.workspaceModePeekDeepLevel': 4, 'workbench.startupEditor': 'none'}))
        try:
            for line in sys.stdin:
                request = json.loads(line)
                if process is None:
                    if request.get('method') != 'workspace/open':
                        print(json.dumps({'id': request.get('id'), 'error': {'message': 'workspace/open must be first'}}), flush=True)
                        continue
                    server = socket.socket()
                    server.bind(('127.0.0.1', 0)); server.listen(1); server.settimeout(90)
                    env = os.environ.copy()
                    env['FSHARP_BASELINE_PORT'] = str(server.getsockname()[1])
                    env['DOTNET_ROOT'] = str(Path(args.dotnet_root).resolve())
                    env['PATH'] = env['DOTNET_ROOT'] + os.pathsep + env['PATH']
                    host = Path(__file__).resolve().parent / 'ionide-host'
                    command = [str(Path(args.vscode).resolve()), '--no-sandbox', '--disable-gpu', '--disable-workspace-trust',
                               '--skip-welcome', '--skip-release-notes', '--user-data-dir=' + str(profile),
                               '--extensions-dir=' + str(Path(args.extensions).resolve()),
                               '--extensionDevelopmentPath=' + str(host), '--extensionTestsPath=' + str(host / 'test.cjs'),
                               request['params']['root']]
                    process = subprocess.Popen(command, env=env, stdout=sys.stderr, stderr=sys.stderr)
                    connection, _ = server.accept(); server.close()
                    reader = connection.makefile('r')
                    handshake = json.loads(reader.readline())
                    if not handshake.get('ready'):
                        raise RuntimeError('VS Code baseline handshake failed')
                connection.sendall(line.encode())
                response = reader.readline()
                if not response:
                    raise RuntimeError('VS Code baseline disconnected')
                print(response, end='', flush=True)
                if request.get('method') == 'shutdown':
                    break
        finally:
            if connection:
                connection.close()
            if process:
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=5)


if __name__ == '__main__':
    main()
