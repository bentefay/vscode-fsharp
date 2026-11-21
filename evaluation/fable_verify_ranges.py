#!/usr/bin/env python3
"""Compare the fixed six-site probe using pinned FSAC's identifier-range rule.

Retains strict raw FCS comparison and every distinct occurrence. This restricted
rule supports only these predeclared simple ASCII identifier tokens; it is not a
general FCS range/identity normalizer.
"""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / 'evaluation/results/fable-fcs-direct-oracle.json'
BASE = ROOT / 'evaluation/results/fable-fcs-fsac-feasibility.json'
OUT = ROOT / 'evaluation/results/fable-fcs-verified-contract.json'

def adjust(location, token):
    path, first, start, last, end = location
    if first != last or not token.isascii() or not token.isidentifier():
        raise ValueError('Outside the restricted one-line simple-identifier rule')
    line = Path(path).read_text().splitlines()[first].encode('utf-16-le')
    content = line[start * 2:end * 2].decode('utf-16-le')
    offset = content.rfind(token)
    if offset < 0:
        raise ValueError('Expected symbol spelling absent from direct FCS span')
    adjusted_start = start + len(content[:offset].encode('utf-16-le')) // 2
    return [path, first, adjusted_start, last, adjusted_start + len(token)]

def main():
    raw, base = json.loads(RAW.read_text()), json.loads(BASE.read_text())
    tokens = {site['name']: site['token'] for site in base['sites']}
    rows = []
    for query in raw['queries']:
        name, feature = query['name'].rsplit('_', 1)
        direct = query['direct_normalized']
        try:
            adjusted = sorted(adjust(loc, tokens[name]) for loc in direct) if feature == 'references' else direct
            error = None
        except (ValueError, TypeError) as exc:
            adjusted, error = None, str(exc)
        rows.append({'name': query['name'], 'raw_full_ranges_equal': query['same_distinct_locations'],
                     'direct_raw_count': len(direct) if direct is not None else None,
                     'adjusted_count': len(adjusted) if adjusted is not None else None,
                     'adjusted_direct_locations': adjusted, 'actual_fsac_locations': query['fsac_normalized'],
                     'adjustment_error': error, 'verified': adjusted is not None and adjusted == query['fsac_normalized']})
    source_root = ROOT / 'evaluation/.runtime/fsac-0.86-source'
    report = {'selection_version': base['selection_version'], 'projects': base['projects'],
              'baseline_sha256': hashlib.sha256(BASE.read_bytes()).hexdigest(),
              'direct_oracle_sha256': hashlib.sha256(RAW.read_bytes()).hexdigest(),
              'normalization': 'FSAC v0.86.0 Commands.tryAdjustRanges -> Tokenizer.tryFixupRange: last simple symbol spelling inside FCS span, preserving every occurrence; definitions unchanged',
              'source_provenance': json.loads((source_root / 'provenance.json').read_text()),
              'source_lines': {'AdaptiveFSharpLspServer.fs': '1585-1606', 'Commands.fs': '874-890', 'FileSystem.fs': '713-798'},
              'queries': rows, 'verified_count': sum(row['verified'] for row in rows),
              'expected_query_count': len(base['queries']), 'performance_acceptance': False}
    OUT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'verified': report['verified_count'], 'expected': report['expected_query_count'], 'report': str(OUT)}))

if __name__ == '__main__':
    main()
