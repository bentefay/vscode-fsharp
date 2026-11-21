#!/usr/bin/env python3
"""Genuine exported metadata -> native read-only indexes -> same exact query.

The JSON artifacts/model must already be prepared with the current producer.
This script does not launch a compiler. Preparation resource values are diagnostic
unless all other workloads were explicitly stopped; it is not a VS Code host test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import sqlite3
import subprocess
import tempfile
import time

import smoke
from resolution_states import Native


def sha(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as source:
        for chunk in iter(lambda: source.read(65536), b''):
            digest.update(chunk)
    return digest.hexdigest()


def prepare(binary, kind, source, output):
    command = [str(binary), 'metadata', 'prepare', kind, str(source), str(output)]
    started = time.perf_counter()
    cpu_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    peak = 0
    with tempfile.TemporaryFile(mode='w+t') as stdout, tempfile.TemporaryFile(mode='w+t') as stderr:
        process = subprocess.Popen(command, env=smoke.ENV, stdout=stdout, stderr=stderr)
        while process.poll() is None:
            try:
                status = Path(f'/proc/{process.pid}/status').read_text()
                peak = max(peak, int(next(line.split()[1] for line in status.splitlines() if line.startswith('VmRSS:'))) * 1024)
            except (FileNotFoundError, StopIteration, ProcessLookupError):
                pass
            if time.perf_counter() - started > 120:
                process.kill()
                process.wait()
                raise AssertionError('Native preparation exceeded its test deadline')
            time.sleep(.01)
        elapsed = (time.perf_counter() - started) * 1000
        stdout.seek(0)
        stderr.seek(0)
        assert process.returncode == 0, stderr.read() + stdout.read()
        result = json.load(stdout)
    cpu_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu_ms = 1000 * (cpu_after.ru_utime + cpu_after.ru_stime - cpu_before.ru_utime - cpu_before.ru_stime)
    return {'command': command, 'wallMs': elapsed, 'cpuMs': cpu_ms, 'sampledPeakNativeRssBytes': peak, 'sourceBytes': Path(source).stat().st_size, 'descriptorBytes': Path(output).stat().st_size, 'result': result}


def inventory_summary(rows):
    digest = hashlib.sha256()
    for name in sorted(rows):
        digest.update(json.dumps([name, rows[name]], sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode() + b'\0')
    return {'names': len(rows), 'entries': sum(map(len, rows.values())), 'sha256': digest.hexdigest()}


def validate_maps(source, descriptor):
    exported = json.loads(source.read_text())
    publication = json.loads(descriptor.read_text())
    database = descriptor.parent / publication['databaseFile']
    assert sha(database) == publication['databaseSha256']
    result = {}
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as db:
        for domain, table in [('typeNames', 'builtin_names'), ('scopeNames', 'builtin_scope_names'), ('termNames', 'builtin_term_names')]:
            actual = {}
            for name, ordinal, payload in db.execute(f'SELECT name,ordinal,payload FROM {table} ORDER BY name,ordinal'):
                actual.setdefault(name, [])
                if ordinal >= 0:
                    actual[name].append(json.loads(payload))
            expected = inventory_summary(exported[domain])
            observed = inventory_summary(actual)
            assert observed == expected, domain
            result[domain] = {'exported': expected, 'prepared': observed}
    return result


def query(binary, config, cache):
    daemon = Native(binary)
    try:
        opened = daemon.call('workspace/open', {**config['open'], 'mode': 'indexed', 'cacheDir': str(cache)})
        assert 'error' not in opened, opened
        startup = daemon.call('workspace/stats', {})['result']
        assert startup['filesRead'] == startup['filesParsed'] == 0
        answers = [daemon.call(item['method'], item['params']) for item in config['queries']]
        stats = daemon.call('workspace/stats', {})['result']
        assert stats['helperStarts'] == stats['compilerLaunches'] == 0
        return {'startup': startup, 'answers': answers, 'stats': stats}
    finally:
        daemon.close()


def semantic_answer(response):
    assert 'error' not in response, response
    answer = response['result']
    assert answer.get('completeness') == 'exact' and answer.get('hover'), answer
    return {key: answer.get(key) for key in ['completeness', 'hover', 'symbol', 'ownership', 'locations', 'documents']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--quiet', action='store_true')
    args = parser.parse_args()
    binary = args.binary.resolve()
    directory = args.output_dir.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    config = json.loads(args.config.read_text())
    report = {'kind': 'real-native-prepared-metadata-regression', 'actualVSCodeHost': False, 'quietWindow': args.quiet, 'binarySha256': sha(binary), 'sourceConfig': config, 'preparation': {}}
    prepared = json.loads(json.dumps(config))
    for kind, field in [('environment', 'builtinEnvironmentPath'), ('catalog', 'metadataPath')]:
        source = Path(config['open'][field])
        destination = directory / (kind + '.fnmeta')
        report['preparation'][kind] = prepare(binary, kind, source, destination)
        prepared['open'][field] = str(destination)
    report['declarationInventories'] = validate_maps(Path(config['open']['builtinEnvironmentPath']), directory / 'environment.fnmeta')
    report['json'] = query(binary, config, directory / 'json-cache')
    report['prepared'] = query(binary, prepared, directory / 'prepared-cache')
    report['reopened'] = query(binary, prepared, directory / 'prepared-cache')
    for original, current, reopened in zip(report['json']['answers'], report['prepared']['answers'], report['reopened']['answers']):
        assert semantic_answer(original) == semantic_answer(current) == semantic_answer(reopened)
    assert report['prepared']['stats']['builtinEnvironmentLoad']['prepared'] is True
    # A prepared generation must not copy large binary fact tables into each
    # workspace. Empty pre-created tables are acceptable; imported rows are not.
    with sqlite3.connect(directory / 'prepared-cache' / 'index-v9.sqlite') as db:
        counts = {}
        for table in ['builtin_names', 'builtin_scope_names', 'builtin_term_names', 'metadata_symbols']:
            counts[table] = db.execute(f'SELECT count(*) FROM {table}').fetchone()[0] if db.execute('SELECT 1 FROM sqlite_master WHERE name=?', [table]).fetchone() else 0
            assert counts[table] == 0, (table, counts[table])
        report['mutableWorkspaceFactRows'] = counts
    # Mutate only a private copy. A valid SQLite deletion cannot become a
    # certified negative lookup even when mtime and file length are preserved.
    corrupt = directory / 'corrupt'
    corrupt.mkdir()
    source_descriptor = directory / 'environment.fnmeta'
    descriptor = json.loads(source_descriptor.read_text())
    shutil.copy2(source_descriptor, corrupt / source_descriptor.name)
    original_database = directory / descriptor['databaseFile']
    copy_database = corrupt / descriptor['databaseFile']
    shutil.copy2(original_database, copy_database)
    bad_config = json.loads(json.dumps(prepared))
    bad_config['open']['builtinEnvironmentPath'] = str(corrupt / source_descriptor.name)
    daemon = Native(binary)
    try:
        opened = daemon.call('workspace/open', {**bad_config['open'], 'mode': 'indexed', 'cacheDir': str(directory / 'mutation-cache')})
        assert 'error' not in opened, opened
        first_query = bad_config['queries'][0]
        before = daemon.call(first_query['method'], first_query['params'])
        semantic_answer(before)
        times = copy_database.stat()
        with sqlite3.connect(copy_database) as db:
            removed = db.execute("DELETE FROM builtin_names WHERE name='String'").rowcount
            assert removed > 0
        os.utime(copy_database, ns=(times.st_atime_ns, times.st_mtime_ns))
        after = daemon.call(first_query['method'], first_query['params'])
        assert after.get('result', {}).get('completeness') != 'exact', after
        report['sameSessionMutation'] = {'before': before, 'after': after, 'deletedRows': removed}
    finally:
        daemon.close()
    report['corrupt'] = query(binary, bad_config, directory / 'corrupt-cache')
    for response in report['corrupt']['answers']:
        assert response.get('result', {}).get('completeness') != 'exact', response
        assert 'digest mismatch' in json.dumps(response), response
    report['preparedConfig'] = prepared
    report['passed'] = True
    report['limits'] = 'Real native processes and genuine pre-exported metadata. No editor host or broad parity claim. Preparation includes hashing, bounded import, sealing and publication. Timings are diagnostic unless a coordinated quiet window is recorded.'
    (directory / 'prepared-config.json').write_text(json.dumps(prepared, indent=2) + '\n')
    (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(directory / 'report.json'), 'passed': True, 'preparationMs': {key: value['wallMs'] for key, value in report['preparation'].items()}}))


if __name__ == '__main__':
    main()
