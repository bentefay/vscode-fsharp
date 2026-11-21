#!/usr/bin/env python3
"""Replay retained genuine paired-signature Option proofs without rebuilding."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    old = json.loads(args.prepared.read_text())
    base = Path(old['fixtureRoot'])
    spec = importlib.util.spec_from_file_location('prep', ROOT / 'docs/review/prepared-replay.py')
    prep = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prep)
    native = prep.load('native', ROOT / 'docs/review/native-smoke-replay.py')
    os.environ.update(prep.ENV)
    daemon = native.Daemon(args.binary.resolve(), base)
    daemon.call('workspace/open', {
        'root': str(base), 'mode': 'indexed', 'cacheDir': str(base / 'cache'),
        'evaluatedModelPath': old['modelPath'],
        'builtinEnvironmentPath': old['environmentPath'],
    })
    cases = []
    for case in old['cases']:
        if case['build']['exitCode'] != 0:
            cases.append({**case, 'status': 'invalid-oracle-not-scored'})
            continue
        response = daemon.call('navigation/definition', {
            **case['query'], 'projectPath': str(Path(case['root']) / 'Test.fsproj'),
        })
        value = response.get('result', {})
        exact = value.get('completeness') == 'exact'
        same = native.normalized(value.get('locations', [])) == native.normalized(case['expectedLocations'])
        cases.append({**case, 'response': response, 'status': 'pass' if exact and same else ('wrong-exact' if exact else 'unavailable')})
    daemon.close()
    result = {
        **old, 'binarySha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        'preparedReport': str(args.prepared),
        'preparedReportSha256': hashlib.sha256(args.prepared.read_bytes()).hexdigest(),
        'cases': cases, 'transcript': daemon.transcript,
        'summary': {status: sum(c['status'] == status for c in cases) for status in ['pass', 'wrong-exact', 'unavailable']},
    }
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(result['summary'])
    print([(c['case'], c['status']) for c in cases])


if __name__ == '__main__':
    main()
