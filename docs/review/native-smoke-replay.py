#!/usr/bin/env python3
"""Independent, correctness-only daemon replay; no compiler/helper or benchmarks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
SPEC = {c['id']: c for c in json.loads((ROOT / 'docs/review/recovery-adversaries.json').read_text())['cases']}

class Daemon:
    def __init__(self, binary, root):
        self.proc = subprocess.Popen([str(binary), 'daemon'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        self.serial = 0
        self.transcript = []
        self.root = root
        self.call('workspace/open', {'root': str(root), 'mode': 'indexed', 'cacheDir': str(root / 'cache')})
    def call(self, method, params=None):
        self.serial += 1
        request = {'id': self.serial, 'method': method, 'params': params or {}}
        self.proc.stdin.write(json.dumps(request) + '\n')
        self.proc.stdin.flush()
        if not select.select([self.proc.stdout], [], [], 20)[0]:
            self.proc.kill()
            raise RuntimeError(f'timeout: {method}')
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError(f'daemon EOF: {self.proc.stderr.read()}')
        response = json.loads(line)
        assert response.get('id') == self.serial, response
        self.transcript.append({'request': request, 'response': response})
        return response
    def close(self):
        try:
            self.call('shutdown')
        finally:
            self.proc.stdin.close()
            self.proc.wait(timeout=5)
            self.stderr = self.proc.stderr.read()

def fixture(base, name, files):
    path = base / name
    path.mkdir()
    for filename, source in files.items():
        (path / filename).write_text(source)
    items = ''.join(f'<Compile Include="{name}"/>' for name in files)
    (path / 'Test.fsproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup>' + items + '</ItemGroup></Project>')
    return path

def location(path, line, character, length):
    return {'uri': path.as_uri(), 'range': {'start': {'line': line, 'character': character}, 'end': {'line': line, 'character': character + length}}}

def normalized(locations):
    return sorted(json.dumps({'uri': x.get('uri', Path(x['path']).as_uri() if 'path' in x else None), 'range': x['range']}, sort_keys=True) for x in locations)

def main():
    args = argparse.ArgumentParser()
    args.add_argument('binary', type=Path)
    args.add_argument('--output', type=Path, required=True)
    options = args.parse_args()
    binary = options.binary.resolve()
    (ROOT / '.tmp').mkdir(exist_ok=True)
    base = Path(tempfile.mkdtemp(prefix='review-native-', dir=ROOT / '.tmp'))
    report = {'schemaVersion': 1, 'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'fixtureRoot': str(base), 'kind': 'Independent correctness replay, not performance measurement', 'oracleProvenance': 'Fresh FSI alias/constraint checks in docs/review/constraint-oracles.json; retained direct FCS dispatch oracle with historical FSAC declaration routing; freshness invariants and exact physical ranges.', 'cases': [], 'sessions': []}
    def check(name, response, expected):
        result = response.get('result', {})
        exact = result.get('completeness') == 'exact'
        same = normalized(result.get('locations', [])) == normalized(expected)
        status = 'pass' if exact and same else ('wrong-exact' if exact else 'unavailable-or-error')
        report['cases'].append({'id': name, 'status': status, 'expectedLocations': expected, 'response': response})
    def save_session(daemon):
        daemon.close()
        report['sessions'].append({'root': str(daemon.root), 'transcript': daemon.transcript, 'stderr': daemon.stderr})
    def query(daemon, file, line, token, method='definition', **kw):
        source = (daemon.root / file).read_text()
        character = source.splitlines()[line].rindex(token)
        return daemon.call('navigation/' + method, {'path': str(daemon.root / file), 'line': line, 'character': character, **kw})

    aliases = SPEC['module_alias_declaration_site_and_additive_merge']['variants']
    for variant, source in aliases.items():
        root = fixture(base, 'alias-' + variant, {'Main.fs': source})
        daemon = Daemon(binary, root)
        checks = [(8, 'value', 5, 8), (9, 'onlyFirst', 3, 8)] if variant == 'additive' else [(8, 'value', 3, 12)]
        for line, token, target, col in checks:
            check(f'alias-{variant}-{line}', query(daemon, 'Main.fs', line, token), [location(root / 'Main.fs', target, col, len(token))])
        save_session(daemon)

    dispatch = SPEC['dispatch_binding_identity_vs_goto_target']
    root = fixture(base, 'dispatch', dispatch['files'])
    daemon = Daemon(binary, root)
    for category in ('callDefinitions', 'declarationDefinitions'):
        for item in dispatch[category]:
            check(f'dispatch-{category}-{item["queryLine"]}', query(daemon, 'Main.fs', item['queryLine'], 'Name'), [location(root / 'Main.fs', item['targetLine'], item['targetCharacter'], 4)])
    save_session(daemon)

    for variant in ('removal', 'addition'):
        spec = SPEC['offline_same_size_preserved_mtime_reference_' + variant]
        root = fixture(base, 'offline-' + variant, spec['files'])
        path = root / 'B.fs'
        before = [location(path, 1, 15, 5)] if variant == 'removal' else []
        after = [] if variant == 'removal' else [location(path, 1, 15, 5)]
        daemon = Daemon(binary, root)
        check(f'offline-{variant}-before', query(daemon, 'A.fs', 1, 'value', 'references', includeDeclaration=False), before)
        save_session(daemon)
        stat = path.stat()
        old = path.read_text()
        new = old.replace('A.value', 'A.other') if variant == 'removal' else old.replace('A.other', 'A.value')
        assert len(new.encode()) == len(old.encode())
        path.write_text(new)
        os.utime(path, ns=(stat.st_atime_ns, stat.st_mtime_ns))
        daemon = Daemon(binary, root)
        check(f'offline-{variant}-after', query(daemon, 'A.fs', 1, 'value', 'references', includeDeclaration=False), after)
        save_session(daemon)

    overlay = SPEC['unsaved_overlay_does_not_survive_restart_as_disk']
    root = fixture(base, 'overlay', {'Main.fs': overlay['saved']})
    daemon = Daemon(binary, root)
    startup = daemon.transcript[0]['response'].get('result', {})
    report['cases'].append({'id': 'workspace-open-no-source-parsing', 'status': 'pass' if startup.get('filesRead') == 0 and startup.get('bytesRead') == 0 and startup.get('filesParsed') == 0 and startup.get('compilerLaunches') == 0 else 'fail', 'response': startup})
    daemon.call('document/open', {'path': str(root / 'Main.fs'), 'text': overlay['overlay'], 'version': 1})
    opened = daemon.call('workspace/stats')['result']
    report['cases'].append({'id': 'document-open-no-ast', 'status': 'pass' if opened.get('filesParsed') == 0 else 'fail', 'response': opened})
    response = daemon.call('navigation/definition', {'path': str(root / 'Main.fs'), 'line': 3, 'character': 13, 'version': 1})
    check('overlay-dirty-definition', response, [location(root / 'Main.fs', 2, 4, 5)])
    save_session(daemon)
    daemon = Daemon(binary, root)
    check('overlay-restart-saved-definition', query(daemon, 'Main.fs', 2, 'value'), [location(root / 'Main.fs', 1, 4, 5)])
    save_session(daemon)

    # All compilable constraint cases were verified independently by FSI. A safety
    # rejection is recorded as missing coverage, never credited as semantic parity.
    prefix = 'module Main\n' + SPEC['static_record_factory_expected_constraints']['prefix']
    variants = SPEC['static_record_factory_expected_constraints']['variants']
    for variant in ('plain', 'earlier_sibling', 'mutual_other_type'):
        source = prefix + variants[variant]
        root = fixture(base, 'factory-' + variant, {'Main.fs': source})
        daemon = Daemon(binary, root)
        line = next(i for i, text in enumerate(source.splitlines()) if '{ Name = "x" }' in text)
        target = 2 if variant == 'plain' else 1
        col = source.splitlines()[target].index('Name')
        check('factory-' + variant, query(daemon, 'Main.fs', line, 'Name'), [location(root / 'Main.fs', target, col, 4)])
        save_session(daemon)

    report['summary'] = {key: sum(c['status'] == key for c in report['cases']) for key in ('pass', 'wrong-exact', 'unavailable-or-error', 'fail')}
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'sha256': report['sha256'], 'summary': report['summary'], 'output': str(options.output), 'nonpassing': [{'id': c['id'], 'status': c['status'], 'response': c['response']} for c in report['cases'] if c['status'] != 'pass']}, indent=2))

if __name__ == '__main__':
    main()
