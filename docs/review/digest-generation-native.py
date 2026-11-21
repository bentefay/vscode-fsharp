#!/usr/bin/env python3
"""Genuine model plus owned imported-file aliases; explicit negative mutations."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT / '.tools/dotnet/dotnet'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--helper', type=Path, required=True)
    ap.add_argument('--binary', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    base = Path(tempfile.mkdtemp(prefix='digest-generation-review-', dir=ROOT / '.tmp'))
    root = base / 'project'
    root.mkdir()
    props = root / 'Input.props'
    hard = root / 'Alias.props'
    link = root / 'Link.props'
    other = base / 'Other.props'
    before = '<Project><PropertyGroup><ReviewFlag>first</ReviewFlag></PropertyGroup></Project>'
    after = before.replace('first', 'other')
    props.write_text(before)
    os.link(props, hard)
    link.symlink_to('Input.props')
    other.write_text(after)
    (root / 'A.fs').write_text('module A\nlet value = 1\n')
    (root / 'B.fs').write_text('module B\nlet result = A.value\n')
    project = root / 'Review.fsproj'
    project.write_text('<Project Sdk="Microsoft.NET.Sdk"><Import Project="Input.props"/><Import Project="Alias.props"/><Import Project="Link.props"/><PropertyGroup><TargetFramework>net10.0</TargetFramework><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Compile Include="A.fs"/><Compile Include="B.fs"/></ItemGroup></Project>')
    env = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1', MSBUILDDISABLENODEREUSE='1', DOTNET_CLI_USE_MSBUILD_SERVER='0', TMPDIR=str(ROOT / '.tmp'))
    build = subprocess.run([str(DOTNET), 'build', str(project), '--nologo', '-v:q', '-m:1', '-nodeReuse:false'], capture_output=True, text=True, env=env, timeout=90)
    request = dict(id=1, method='workspace/evaluate', params=dict(root=str(root), projects=[str(project)]))
    process = subprocess.run([str(DOTNET), str(args.helper.resolve()), '--once'], input=json.dumps(request) + '\n', capture_output=True, text=True, env=env, timeout=120)
    response = next((json.loads(line) for line in process.stdout.splitlines() if line.startswith('{') and json.loads(line).get('id') == 1), {})
    model = response.get('result')
    report = dict(provenance='Independent end-to-end validation with a genuine built SDK project and helper09f v4 model. Imported props have one physical inode through distinct hardlink/symlink paths. Only owned fixture files are mutated; conflicting expected hashes are explicitly synthetic certificate negatives. No performance claim.', fixtureRoot=str(base), binarySha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(), helperSha256=hashlib.sha256(args.helper.read_bytes()).hexdigest(), build=dict(exitCode=build.returncode, stdout=build.stdout, stderr=build.stderr), preparation=dict(request=request, response=response, exitCode=process.returncode, stderr=process.stderr), cases=[])
    def save():
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    save()
    assert build.returncode == 0 and model and process.returncode == 0, 'Genuine preparation failed; raw evidence saved'
    assert model['projects'][0]['membershipReusable'], model['projects'][0]['membershipDiagnostics']
    certified = {row['path'] for row in model['inputHashes']}
    assert all(str(path) in certified for path in [props, hard, link]), ('Missing alias certificates', certified)
    model_path = base / 'model.json'
    model_path.write_text(json.dumps(model, indent=2) + '\n')
    spec = importlib.util.spec_from_file_location('native', ROOT / 'docs/review/native-smoke-replay.py')
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    os.environ.update(env)
    expected = [native.location(root / 'B.fs', 1, 15, 5)]
    def query(daemon):
        return daemon.call('navigation/references', dict(path=str(root / 'A.fs'), line=1, character=5, includeDeclaration=False))
    def open_daemon(name, artifact=model_path):
        daemon = native.Daemon(args.binary.resolve(), root)
        daemon.call('workspace/open', dict(root=str(root), mode='indexed', evaluatedModelPath=str(artifact), cacheDir=str(base / ('cache-' + name))))
        return daemon
    def add(name, response, want_exact):
        result = response.get('result', {})
        exact = result.get('completeness') == 'exact'
        passed = exact and native.normalized(result.get('locations', [])) == native.normalized(expected) if want_exact else not exact and bool(response.get('error') or result.get('diagnostics'))
        report['cases'].append(dict(case=name, passed=passed, expected='exact original reference' if want_exact else 'explicit refusal of exact answer after invalid proof', response=response))
        save()
    daemon = open_daemon('same-session')
    add('genuine-alias-positive', query(daemon), True)
    report['initialValidationMetrics'] = daemon.call('workspace/stats').get('result', {}).get('modelValidation')
    original = props.stat()
    hard.write_text(after)
    os.utime(hard, ns=(original.st_atime_ns, original.st_mtime_ns))
    report['sameMtimeMutation'] = dict(sameInode=props.stat().st_ino == original.st_ino, sameSize=props.stat().st_size == original.st_size, sameMtime=props.stat().st_mtime_ns == original.st_mtime_ns, ctimeChanged=props.stat().st_ctime_ns != original.st_ctime_ns)
    time.sleep(0.03)
    add('same-session-same-mtime-hardlink-change', query(daemon), False)
    daemon.close()
    daemon = open_daemon('restart-stale')
    add('restart-same-mtime-hardlink-change', query(daemon), False)
    daemon.close()
    props.write_text(before)
    os.utime(props, ns=(original.st_atime_ns, original.st_mtime_ns))
    daemon = open_daemon('restored')
    add('restored-bytes-positive', query(daemon), True)
    link.unlink()
    link.symlink_to(other)
    time.sleep(0.03)
    add('same-session-alias-retarget', query(daemon), False)
    daemon.close()
    daemon = open_daemon('retarget-restart')
    add('restart-alias-retarget', query(daemon), False)
    daemon.close()
    link.unlink()
    link.symlink_to('Input.props')
    conflicting = json.loads(json.dumps(model))
    # Choose the last of the certified alias rows so the optimized validator
    # encounters the contradictory expectation after a successful same-inode row.
    alias_rows = [row for row in conflicting['inputHashes'] if row['path'] in {str(props), str(hard), str(link)}]
    assert len(alias_rows) == 3
    alias_rows[-1]['sha256'] = hashlib.sha256(after.encode()).hexdigest()
    report['conflictingExpectedAliasPath'] = alias_rows[-1]['path']
    conflict_path = base / 'synthetic-conflicting-expected.json'
    conflict_path.write_text(json.dumps(conflicting, indent=2) + '\n')
    daemon = open_daemon('conflicting', conflict_path)
    add('same-inode-conflicting-expected-hash', query(daemon), False)
    daemon.close()
    daemon = open_daemon('final-positive')
    add('final-genuine-positive', query(daemon), True)
    daemon.close()
    report['summary'] = dict(passed=sum(row['passed'] for row in report['cases']), total=len(report['cases']))
    save()
    print(report['summary'])
    for row in report['cases']:
        if not row['passed']:
            print(row)


if __name__ == '__main__':
    main()
