#!/usr/bin/env python3
"""Per-query correctness/coverage deltas; incomplete results are never wins."""
import argparse
import json
from pathlib import Path
from run import file_sha

def rows(report):
    grace = any(r.get('method') in ('definition', 'references') for r in report['rows'])
    result = {}
    for row in report['rows']:
        if grace:
            if row.get('phase') != 'query':
                continue
            key = (row['name'], row['method'])
        else:
            if row.get('phase') != 'correctness':
                continue
            key = (row['query'], row.get('request', {}).get('method', ''))
        value = row.get('response', {}).get('result', {})
        locations = value.get('locations', [])
        result[key] = {'correct': row.get('correct'), 'completeness': value.get('completeness'),
            'locations': sorted(locations, key=lambda x: json.dumps(x, sort_keys=True)),
            'raw_location_count': len(locations), 'hover': value.get('hover'),
            'proof_gaps': value.get('proofGaps'), 'diagnostics': value.get('diagnostics'),
            'error': row.get('response', {}).get('error'), 'validation_reason': row.get('reason')}
    return result

def summary(values):
    return {'queries': len(values), 'correct': sum(v['correct'] is True for v in values),
        'unavailable': sum(v['completeness'] == 'unavailable' for v in values),
        'protocol_errors': sum(v['error'] is not None for v in values),
        'wrong_exact': sum(v['correct'] is False and v['completeness'] == 'exact' for v in values)}

def same_present(a, b):
    return None if a is None and b is None else a == b

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    before, after = json.loads(args.before.read_text()), json.loads(args.after.read_text())
    old, new = rows(before), rows(after)
    changes = []
    for key in sorted(old.keys() | new.keys()):
        a, b = old.get(key), new.get(key)
        if a == b:
            continue
        changes.append({'name': key[0], 'method': key[1], 'before': a, 'after': b,
            'coverage_change': 'lost_correct_answer' if a and b and a['correct'] is True and b['correct'] is not True
            else 'gained_correct_answer' if a and b and a['correct'] is not True and b['correct'] is True
            else 'response_change_without_correctness_gain'})
    report = {'kind': 'per-query-correctness-and-coverage-delta',
        'before': str(args.before), 'before_report_sha256': file_sha(args.before),
        'after': str(args.after), 'after_report_sha256': file_sha(args.after),
        'before_candidate': before['provenance']['engine_binary_sha256'],
        'after_candidate': after['provenance']['engine_binary_sha256'],
        'same_query_keys': old.keys() == new.keys(),
        'same_fixture_fingerprint': same_present(before.get('workload', {}).get('fingerprint'), after.get('workload', {}).get('fingerprint')),
        'same_selection_hash': same_present(before.get('selection_sha256'), after.get('selection_sha256')),
        'same_prepared_input_hashes': before['provenance'].get('prepared_input_sha256') == after['provenance'].get('prepared_input_sha256'),
        'before_summary': summary(old.values()), 'after_summary': summary(new.values()),
        'changed_queries': changes,
        'lost_correct_queries': [v['name'] for v in changes if v['coverage_change'] == 'lost_correct_answer'],
        'gained_correct_queries': [v['name'] for v in changes if v['coverage_change'] == 'gained_correct_answer'],
        'lost_correct_operations': [{'name': v['name'], 'method': v['method']} for v in changes if v['coverage_change'] == 'lost_correct_answer'],
        'gained_correct_operations': [{'name': v['name'], 'method': v['method']} for v in changes if v['coverage_change'] == 'gained_correct_answer'],
        'performance_acceptance': False,
        'notes': ['Returning unavailable can repair false exactness but still loses feature coverage.',
                  'Partial reference locations and proof gaps are retained; aggregate correctness does not conceal removed uses.']}
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['same_query_keys', 'same_fixture_fingerprint', 'same_selection_hash',
        'same_prepared_input_hashes', 'before_summary', 'after_summary', 'lost_correct_queries', 'gained_correct_queries']}))

if __name__ == '__main__':
    main()
