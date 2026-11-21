#!/usr/bin/env python3
"""Matched native requests on a supplemental local parameter in the pinned Grace application."""
import argparse
import hashlib
import json
import statistics
import sys
import tempfile
from pathlib import Path
from time import perf_counter_ns

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'semantic/tests'))
sys.path.insert(0, str(ROOT / 'evaluation'))
from resolution_states import Native
from run import canonical_location, process_tree_rss

FILENAME = 'ContentBlockMetadata.Types.fs'
SOURCE_SHA = '8b96de26c296267e760ad39c3466a6d6bf30fd6dcfccd9b83e189b026d75f0d3'
LINE = 420


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def positions(source, name):
    line = source.splitlines()[LINE]
    marker = f'(fun {name} -> {name}.ActiveManifestCount)'
    assert line.count(marker) == 1
    formal = line.index(marker) + len('(fun ')
    use = formal + len(name + ' -> ')
    # The pinned line contains ASCII only, including the dirty rename.
    assert line.isascii()
    return [(FILENAME, LINE, col, LINE, col + len(name)) for col in [formal, use]]


def measure(binary, directory, model, cache, source, warm_count):
    filename = directory / FILENAME
    edited = source.splitlines(keepends=True)
    edited[LINE] = edited[LINE].replace('range', 'navRange')
    changed = ''.join(edited)
    rows = []
    phases = []
    for restart in (False, True):
        started = perf_counter_ns()
        daemon = Native(binary)
        try:
            def ack(method, params):
                response = daemon.call(method, params)
                if 'error' in response:
                    raise RuntimeError(response['error'])
                return response['result']

            def query(phase, version, expected, references=False, start_time=None):
                method = 'references' if references else 'definition'
                cursor = expected[1]
                params = dict(path=str(filename), line=LINE, character=cursor[2],
                              version=version, includeDeclaration=True)
                start = perf_counter_ns()
                response = daemon.call('navigation/' + method, params)
                end = perf_counter_ns()
                result = response.get('result', {})
                actual = sorted(canonical_location(loc, directory) for loc in result.get('locations', []))
                target = sorted(expected if references else expected[:1])
                status = 'unsupported'
                if result.get('completeness') == 'exact':
                    status = 'correct' if actual == target else 'wrong-exact'
                row = dict(phase=phase, method=method, status=status, source=result.get('source'),
                           requestMs=(end - start) * 1e-6)
                if start_time is not None:
                    row['lifecycleMs'] = (end - start_time) * 1e-6
                if status != 'correct':
                    row.update(expected=target, actual=actual, reason=result.get('reason', response.get('error')))
                rows.append(row)

            ack('workspace/open', dict(root=str(directory), mode='indexed', evaluatedModelPath=str(model),
                                       cacheDir=str(cache)))
            text, version, name = (changed, 2, 'navRange') if restart else (source, 1, 'range')
            ack('document/open', dict(path=str(filename), text=text, version=version))
            opened = perf_counter_ns()
            expected = positions(text, name)
            query('restart' if restart else 'cold', version, expected, start_time=started)
            rss_first, pids_first = process_tree_rss(daemon.process.pid)
            rss_warm = None
            if not restart:
                for _ in range(warm_count):
                    query('warm', version, expected)
                query('firstReferences', version, expected, references=True)
                for _ in range(warm_count):
                    query('warmReferences', version, expected, references=True)
                rss_warm, _ = process_tree_rss(daemon.process.pid)
                edit_start = perf_counter_ns()
                ack('document/change', dict(path=str(filename), text=changed, version=2))
                query('edit', 2, positions(changed, 'navRange'), start_time=edit_start)
            stats = ack('workspace/stats', {})
            rss_end, pids_end = process_tree_rss(daemon.process.pid)
            phases.append(dict(restart=restart, openMs=(opened-started)*1e-6,
                               firstTreeRssKiB=rss_first, warmTreeRssKiB=rss_warm, endTreeRssKiB=rss_end,
                               firstProcessCount=len(pids_first), endProcessCount=len(pids_end),
                               stats=stats))
        finally:
            daemon.close()
    return dict(rows=rows, processes=phases)


def measure_global(binary, directory, model, cache, warm_count):
    # Reuse a fixed, compiler-checked module-value site with an acknowledged
    # overlay. This is a lifecycle measurement outside the fixed-suite score.
    contract = json.loads((ROOT / 'evaluation/contracts/grace46.json').read_text())
    site = next(q for q in contract['selected'] if q['name'] == 'module_value_use_24_referenceTypeName')
    filename = directory / Path(site['path']).name
    assert sha(filename) == contract['corpus']['source_sha256'][site['path']]
    expected = site['declaration']
    target = (Path(expected['path']).name, expected['range']['start']['line'],
              expected['range']['start']['character'], expected['range']['end']['line'],
              expected['range']['end']['character'])
    daemon = Native(binary)
    rows = []
    try:
        opened = daemon.call('workspace/open', dict(root=str(directory), mode='indexed',
                             evaluatedModelPath=str(model), cacheDir=str(cache)))
        assert 'error' not in opened, opened
        opened = daemon.call('document/open', dict(path=str(filename), version=1, text=filename.read_text()))
        assert 'error' not in opened, opened
        cursor = dict(path=str(filename), line=site['line'], character=site['character'], version=1)
        for attempt in range(warm_count + 1):
            start = perf_counter_ns()
            response = daemon.call('navigation/definition', cursor)
            elapsed = (perf_counter_ns() - start) * 1e-6
            result = response.get('result', {})
            locations = [canonical_location(loc, directory) for loc in result.get('locations', [])]
            status = 'unsupported'
            if result.get('completeness') == 'exact':
                status = 'correct' if locations == [target] else 'wrong-exact'
            rows.append(dict(phase='globalWarm' if attempt else 'globalCold', method='definition',
                             status=status, source=result.get('source'), requestMs=elapsed))
            if status != 'correct':
                rows[-1].update(actual=locations, expected=[target],
                                reason=result.get('reason', response.get('error')))
        stats = daemon.call('workspace/stats', {})['result']
    finally:
        daemon.close()
    return rows, stats


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True, help='pinned Grace src/Grace.Types')
    parser.add_argument('--model', type=Path, required=True, help='genuine prepared model for --root')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=6)
    parser.add_argument('--warm', type=int, default=12)
    args = parser.parse_args()
    if args.pairs < 2 or args.pairs % 2 or args.warm < 1:
        parser.error('use an even number of pairs and at least one warm query')
    directory = args.root.resolve()
    filename = directory / FILENAME
    assert sha(filename) == SOURCE_SHA
    source = filename.read_bytes().decode()
    model = args.model.resolve()
    binaries = dict(baseline=args.baseline.resolve(), candidate=args.candidate.resolve())
    report = dict(sourceSha256=SOURCE_SHA, modelSha256=sha(model),
                  binaries={label: sha(binary) for label, binary in binaries.items()}, runs=[])
    for pair in range(args.pairs):
        labels = list(binaries) if pair % 2 == 0 else list(reversed(binaries))
        for label in labels:
            with tempfile.TemporaryDirectory(prefix='local-lifecycle-') as cache:
                result = measure(binaries[label], directory, model, Path(cache) / 'parameter', source, args.warm)
                rows, stats = measure_global(binaries[label], directory, model, Path(cache) / 'module', args.warm)
                result['rows'].extend(rows)
                result['globalStats'] = stats
            report['runs'].append(dict(pair=pair, engine=label, **result))
    summary = {}
    for label in binaries:
        runs = [run for run in report['runs'] if run['engine'] == label]
        summary[label] = {}
        for phase in dict.fromkeys(row['phase'] for row in runs[0]['rows']):
            rows = [row for run in runs for row in run['rows'] if row['phase'] == phase]
            correct = [row for row in rows if row['status'] == 'correct']
            summary[label][phase] = dict(correct=len(correct), total=len(rows),
                                         wrongExact=sum(row['status'] == 'wrong-exact' for row in rows))
            for metric in ('requestMs', 'lifecycleMs'):
                values = [row[metric] for row in correct if metric in row]
                if values:
                    summary[label][phase][metric + 'Median'] = statistics.median(values)
        summary[label]['firstTreeRssKiBMedian'] = statistics.median(run['processes'][0]['firstTreeRssKiB'] for run in runs)
    assert report['binaries'] == {label: sha(binary) for label, binary in binaries.items()}
    assert sha(model) == report['modelSha256'] and sha(filename) == SOURCE_SHA
    report['summary'] = summary
    with args.output.open('x') as output:
        json.dump(report, output, indent=2)
    print(json.dumps(summary, indent=2))
    if any(row['status'] == 'wrong-exact' for run in report['runs'] for row in run['rows']):
        raise SystemExit('wrong exact answer')


if __name__ == '__main__':
    main()
