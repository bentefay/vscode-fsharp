#!/usr/bin/env python3
"""Summarize the unchanged sampled query matrix without hiding incomplete rows."""
import argparse
import collections
import json
from pathlib import Path

from run import file_sha

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--report', type=Path, required=True)
parser.add_argument('--selection', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
report = json.loads(args.report.read_text())
selection = json.loads(args.selection.read_text())
sites = {site['name']: site for site in selection['selected']}
rows = [row for row in report['rows'] if row['phase'] == 'query']

def counts(items):
    return {
        'queries': len(items),
        'correct': sum(row.get('correct') is True for row in items),
        'unavailable': sum(not row['complete'] and 'error' not in row['response'] for row in items),
        'protocol_errors': sum('error' in row['response'] for row in items),
        'wrong_exact': sum(row['complete'] and row.get('correct') is False for row in items),
    }

strata = collections.defaultdict(list)
methods = collections.defaultdict(list)
misses = []
for row in rows:
    strata[row['stratum']].append(row)
    methods[row['method']].append(row)
    if row.get('correct') is not False:
        continue
    site = sites[row['name']]
    lines = Path(site['path']).read_text().splitlines()
    misses.append({
        'name': row['name'], 'method': row['method'],
        'path': site['path'], 'line': site['line'], 'character': site['character'],
        'source_context': [{'line': line, 'text': lines[line]} for line in
                           range(max(0, site['line'] - 2), min(len(lines), site['line'] + 3))],
        'expected': row['expected'], 'actual': row['actual'],
        'reason': row['response'].get('error') or row['response'].get('result', {}).get('reason')
                  or row['response'].get('result', {}).get('diagnostics'),
        'counter_delta': row.get('counter_delta'),
        'proof_gaps': row['response'].get('result', {}).get('proofGaps'),
    })

output = {
    'source_report': str(args.report), 'source_report_sha256': file_sha(args.report),
    'selection': str(args.selection), 'selection_sha256': file_sha(args.selection),
    'candidate_provenance': report.get('provenance'),
    'prepared_inputs': report.get('prepared_inputs'),
    'selected_project_path': report.get('selected_project_path'),
    'total': counts(rows),
    'methods': {method: counts(items) for method, items in methods.items()},
    'strata': {stratum: counts(items) for stratum, items in strata.items()},
    'misses': misses,
    'notes': ['Unavailable and failed operations remain in every denominator.',
              'A complete reference set must equal the canonical golden set; partial sets do not count.',
              'No comparative performance claim is made by this accuracy summary.'],
}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps({'total': output['total'], 'methods': output['methods']}))
