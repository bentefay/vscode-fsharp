#!/usr/bin/env python3
"""Independent direct FCS checks of the fixed selected-project FSAC probe.

This helper invalidates FCS before each query, so these are correctness/setup
costs, never a warm language-service performance baseline.
"""
import json
import os
import time
from pathlib import Path
from urllib.parse import unquote, urlparse
from run import Session, file_sha

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / 'evaluation/results/fable-fcs-fsac-feasibility.json'
OUT = ROOT / 'evaluation/results/fable-fcs-direct-oracle.json'
HELPER = ROOT / 'evaluation/.runtime/helper-f215dac4599c/FSharpNavigator.Semantic.dll'
SDK = ROOT / '.tools/dotnet-sdk100/dotnet'

def normalize(answer):
    result = answer.get('result', {})
    if result.get('completeness') != 'exact':
        return None
    values = set()
    for location in result.get('locations', []):
        path = location.get('path', location.get('uri', ''))
        if path.startswith('file:'):
            path = unquote(urlparse(path).path)
        if not '://' in path:
            path = str(Path(path).resolve())
        r = location['range']
        values.add((path, r['start']['line'], r['start']['character'],
                    r['end']['line'], r['end']['character']))
    return sorted(values)

def main():
    baseline = json.loads(BASELINE.read_text())
    os.environ.update(DOTNET_ROOT=str(SDK.parent), FSHARP_NAVIGATOR_DOTNET=str(SDK),
                      TMPDIR=str(ROOT / '.tmp/evaluation-recovery'), MSBUILDDISABLENODEREUSE='1')
    command = [str(ROOT / '.tools/dotnet/dotnet'), str(HELPER)]
    report = {'kind': 'independent-direct-FCS-selected-Fable-correctness-oracle',
              'helper_sha256': file_sha(HELPER), 'baseline_sha256': file_sha(BASELINE),
              'selection_version': baseline['selection_version'], 'projects': baseline['projects'],
              'command': command, 'compiler_state': 'InvalidateAll before every query',
              'performance_acceptance': False, 'queries': []}
    session = Session(command, timeout=100, memory_limit_mib=4096)
    deadline = time.monotonic() + 420
    try:
        for query in baseline['queries']:
            if session.failed or time.monotonic() > deadline:
                report['stopped_reason'] = 'bounded deadline or failed process'
                break
            method = 'navigation/' + query['name'].rsplit('_', 1)[1]
            params = dict(query['query'], root=baseline['scope'], projects=baseline['projects'])
            response, measurement = session.call(method, params)
            expected, actual = normalize(response), normalize(query['response'])
            row = {'name': query['name'], 'query': params, 'response': response,
                   'measurement': measurement, 'direct_normalized': expected,
                   'fsac_normalized': actual, 'same_distinct_locations': expected is not None and expected == actual}
            report['queries'].append(row)
            OUT.write_text(json.dumps(report, indent=2) + '\n')
            print(json.dumps({'name': query['name'], 'error': response.get('error'),
                              'completeness': response.get('result', {}).get('completeness'),
                              'direct_count': len(expected) if expected is not None else None,
                              'fsac_count': len(actual) if actual is not None else None,
                              'equal': row['same_distinct_locations'], 'measurement': measurement}), flush=True)
    finally:
        session.close()
        report['stderr'] = session.stderr
        report['samples'] = session.detailed_samples
        report['all_distinct_locations_equal'] = len(report['queries']) == len(baseline['queries']) and all(x['same_distinct_locations'] for x in report['queries'])
        OUT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(OUT), 'queries': len(report['queries']),
                      'all_equal': report['all_distinct_locations_equal']}), flush=True)

if __name__ == '__main__':
    main()
