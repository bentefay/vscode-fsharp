#!/usr/bin/env python3
"""Independent complete-output and lifetime controls for shared binary imports.

Four explicitly selected equal-coverage arms separate the old implementation,
new streaming serialization, process reuse, and one imported binary context.
No performance interpretation is made without an externally coordinated quiet run.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import sys
import time

import smoke
sys.path.insert(0, str(smoke.ROOT / 'evaluation'))
from run import Session, process_identity


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def child_cpu():
    value = resource.getrusage(resource.RUSAGE_CHILDREN)
    return value.ru_utime + value.ru_stime


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def normalized(value):
    """Only documented nonsemantic ordering/provenance may differ."""
    def visit(node):
        if isinstance(node, list):
            return [visit(item) for item in node]
        if isinstance(node, dict):
            return {key: sorted(item) if key == 'declaredMemberNames' else visit(item) for key, item in node.items()}
        return node
    value = visit(value)
    # Preserve contract/profile/component versions, FCS provenance, and every
    # live input. Never remove a selected FCS input merely by its filename.
    value['producer']['implementationHashes'] = [row for row in value['producer']['implementationHashes'] if row['role'] != 'exporter']
    if 'symbols' in value:
        projected = {}
        for symbol in value['symbols']:
            for name in set(symbol.get('qualifiedNames', []) + [symbol['fullName']]):
                projected.setdefault((name, symbol['id']), symbol)
        value['_actualFirstRowProjection'] = [[name, identifier, symbol] for (name, identifier), symbol in sorted(projected.items())]
        value['symbols'] = sorted(value['symbols'], key=lambda item: json.dumps(item, sort_keys=True))
    return value


def canonical(value):
    return hashlib.sha256(json.dumps(normalized(value), sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()


def stopped(session, expect_exit=0, natural=False):
    try:
        if not natural and session.process.poll() is None:
            response, _ = session.call('shutdown')
            assert 'error' not in response, response
        session.process.wait(timeout=5)
        assert session.process.returncode == expect_exit, (session.process.returncode, session.stderr)
        alive = []
        for pid, started in session.owned_identities.items():
            try:
                if process_identity(pid)[1] == started:
                    alive.append(pid)
            except (FileNotFoundError, ProcessLookupError):
                pass
        assert not alive, f'Helper-owned process remains alive: {alive}'
        return {'naturalExit': natural, 'exitCode': session.process.returncode, 'ownedProcessCount': len(session.owned_identities), 'remainingOwnedProcesses': alive}
    finally:
        session.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--control-helper', required=True, type=Path)
    parser.add_argument('--helper', required=True, type=Path)
    parser.add_argument('--fixture', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--quiet', action='store_true')
    args = parser.parse_args()
    os.environ.update(smoke.ENV)
    directory = args.output_dir.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((args.fixture / 'manifest.json').read_text())
    program = manifest['programs']['positive']
    model = json.loads(Path(program['model']).read_text())
    references = program['references']
    owned = sorted({path for project in model['projects'] for path in project['workspaceAssemblyReferences']})
    assemblies = [next(path for path in references if Path(path).name == name) for name in ['FSharp.Core.dll', 'System.Runtime.dll']]
    report = {'kind': 'independent-shared-binary-import-experiment', 'quietWindow': args.quiet, 'controlHelperSha256': sha(args.control_helper), 'helperSha256': sha(args.helper), 'catalogAssemblyPaths': assemblies, 'referencePaths': references, 'workspaceAssemblyPaths': owned, 'arms': {}, 'comparisons': [], 'lifetime': []}

    def worker(helper):
        return Session([str(smoke.DOTNET), str(helper.resolve())], timeout=150, memory_limit_mib=2048)

    def request(session, method, parameters):
        response, cost = session.call(method, parameters)
        assert 'error' not in response, (response, session.stderr)
        return response['result'], cost

    for name, helper, reuse, streaming in [('old-separate', args.control_helper, False, False), ('new-separate', args.helper, False, True), ('new-reused-process', args.helper, True, True)]:
        path = directory / name
        path.mkdir()
        started = time.perf_counter()
        cpu_before = child_cpu()
        session = worker(helper) if reuse else None
        phases = []
        for index, assembly in enumerate(assemblies):
            current = session or worker(helper)
            try:
                result, cost = request(current, 'metadata/export', {'assemblyPaths': [assembly], 'referencePaths': references, 'outputPath': str(path / f'catalog-{index}.json')})
                phases.append({'phase': 'catalog', 'assemblyPath': assembly, 'measurement': cost, 'summary': result})
            finally:
                if not reuse:
                    report['lifetime'].append(stopped(current))
        current = session or worker(helper)
        try:
            parameters = {'referencePaths': references, 'workspaceAssemblyPaths': owned}
            if streaming:
                parameters['outputPath'] = str(path / 'environment.json')
            result, cost = request(current, 'metadata/environment', parameters)
            if not streaming:
                save(path / 'environment.json', result)
                result = {'completeness': result['completeness'], 'visitedEntities': result['visitedEntities'], 'outputMode': 'full JSON stdout; harness wrote artifact'}
            phases.append({'phase': 'environment', 'measurement': cost, 'summary': result})
        finally:
            report['lifetime'].append(stopped(current))
        report['arms'][name] = {'wallMsIncludingProcessLifecycle': (time.perf_counter() - started) * 1000, 'helperCpuMs': (child_cpu() - cpu_before) * 1000, 'sampledPeakTreeRssKiB': max(phase['measurement']['peak_tree_rss_kib'] for phase in phases), 'phases': phases, 'directory': str(path), 'processReuse': reuse, 'streamedEnvironmentOutput': streaming}

    output = directory / 'shared-generation'
    parameters = {'referencePaths': references, 'workspaceAssemblyPaths': owned, 'assemblyPaths': assemblies, 'outputDirectory': str(output), 'maxWorkingSetMb': 768, 'timeoutMilliseconds': 120000}
    started = time.perf_counter()
    cpu_before = child_cpu()
    current = worker(args.helper)
    try:
        result, cost = request(current, 'metadata/prepare-experimental', parameters)
        assert result['importCount'] == 1 and result['sourceProjectChecks'] == 0 and result['catalogShards'] == len(assemblies)
        report['arms']['shared-context'] = {'measurement': cost, 'summary': result, 'directory': str(output)}
    finally:
        report['lifetime'].append(stopped(current, natural=True))
    report['arms']['shared-context'].update(wallMsIncludingProcessLifecycle=(time.perf_counter() - started) * 1000, helperCpuMs=(child_cpu() - cpu_before) * 1000, sampledPeakTreeRssKiB=cost['peak_tree_rss_kib'])

    # Compare entire payloads/certificates, not just counts or chosen hovers.
    for item in ['catalog-0.json', 'catalog-1.json', 'environment.json']:
        baseline = json.loads((directory / 'old-separate' / item).read_text())
        expected = canonical(baseline)
        evidence = {'artifact': item, 'canonicalSha256': expected, 'rawArtifacts': []}
        for name, path in [('old-separate', directory / 'old-separate' / item), ('new-separate', directory / 'new-separate' / item), ('new-reused-process', directory / 'new-reused-process' / item), ('shared-context', output / ('builtins.json' if item == 'environment.json' else f'catalogs/{int(item[8]):04}.json'))]:
            value = json.loads(path.read_text())
            actual = canonical(value)
            evidence['rawArtifacts'].append({'arm': name, 'path': str(path), 'sha256': sha(path), 'canonicalSha256': actual, 'bytes': path.stat().st_size, 'producer': value['producer']})
            assert actual == expected, {'artifact': item, 'arm': name, 'expected': expected, 'actual': actual}
        report['comparisons'].append(evidence)
    shared_manifest = json.loads((output / 'libraries.json').read_text())
    inputs = {}
    for index in range(len(assemblies)):
        for value in json.loads((directory / 'old-separate' / f'catalog-{index}.json').read_text())['inputHashes']:
            assert value['path'] not in inputs or inputs[value['path']] == value['sha256']
            inputs[value['path']] = value['sha256']
    assert {row['path']: row['sha256'] for row in shared_manifest['inputHashes']} == inputs

    # An ordinary pre-publication error cleans its private generation and exits.
    bad_parent = directory / 'ordinary-failure'
    bad_parent.mkdir()
    current = worker(args.helper)
    bad = {**parameters, 'referencePaths': references + [str(bad_parent / 'Missing.dll')], 'outputDirectory': str(bad_parent / 'generation')}
    try:
        response, _ = current.call('metadata/prepare-experimental', bad)
        assert 'error' in response and not Path(bad['outputDirectory']).exists(), response
        assert not list(bad_parent.glob('.binary-job-*'))
        report['ordinaryFailure'] = response
    finally:
        report['lifetime'].append(stopped(current, natural=True))
    # Source project checking is outside this binary-only API's accepted scope.
    current = worker(args.helper)
    try:
        response, _ = current.call('metadata/prepare-experimental', {**parameters, 'outputDirectory': str(directory / 'forbidden-source'), 'root': program['root']})
        assert 'error' in response and not (directory / 'forbidden-source').exists(), response
        report['sourceInputsRejected'] = response
    finally:
        report['lifetime'].append(stopped(current, natural=True))
    report['passed'] = True
    report['limits'] = 'Complete equal-coverage comparison with exactly Core+Runtime catalog targets. Exporter implementation evidence is separated; all contract/profile, FCS provenance, live dependency, ownership and XML witnesses remain compared. declaredMemberNames order is nonsemantic; symbol multisets and exact first-row-wins alias projections are both compared. Resource values are diagnostic unless quietWindow is true; no editor host or whole-engine equivalence claim. Process exit proves imported caches cannot persist beyond this one job; ordinary-error staging cleanup is checked.'
    save(directory / 'report.json', report)
    print(json.dumps({'report': str(directory / 'report.json'), 'passed': True, 'arms': len(report['arms']), 'wholeArtifactComparisons': len(report['comparisons']), 'lifetimeChecks': len(report['lifetime'])}))


if __name__ == '__main__':
    main()
