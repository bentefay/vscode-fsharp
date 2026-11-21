"""Verify the portable contract and apply the native scorer's exact range rules."""
import json
from pathlib import Path
import subprocess
from real_trace import equivalent_locations
from run import canonical_location, file_sha

CONTRACT = Path(__file__).resolve().parents[1] / 'contracts/grace46.json'


def inputs(workspace):
    contract = json.loads(CONTRACT.read_text())
    root = workspace.parents[len(Path(contract['corpus']['workspace']).parts) - 1]
    if root / contract['corpus']['workspace'] != workspace:
        raise ValueError('Select the workspace recorded in grace46.json')
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    if revision != contract['corpus']['commit']:
        raise ValueError('Grace revision differs from its contract')
    for name, expected in contract['corpus']['source_sha256'].items():
        if file_sha(root/name) != expected:
            raise ValueError(f'Grace source hash changed: {name}')
    return root


def score(query, root, navigator=True, rows_key='rows'):
    sites = [s for s in json.loads(CONTRACT.read_text())['selected'] if s['classification'] == 'source']
    operations = [(s, method) for s in sites for method in ('definition', 'references')]
    if len(operations) != 46 or len(query.get(rows_key, [])) != len(operations):
        raise ValueError('Incomplete Grace46 capture')
    def assess(site, method, row):
        if site['name'] != row['name'] or method != row['method']:
            raise ValueError('Grace operation order changed')
        actual = sorted(set(canonical_location(loc, root) for loc in row['locations']))
        targets = [site['declaration']] if method == 'definition' else site['reference_locations']
        expected = sorted(set(canonical_location(dict(loc, path=str(root/loc['path'])), root) for loc in targets))
        correct = actual == expected if method == 'definition' else equivalent_locations(actual, expected, root)
        if not navigator:
            if not actual:
                return 'unavailable'
            return 'correct' if correct else 'incorrectLocations'
        if not row['nativeResultObserved']:
            return 'noNativeResponse'
        if row['completeness'] != 'exact':
            return 'unavailable'
        return 'correct' if correct else 'wrongExact'
    counts = dict(correct=0, wrongExact=0, unavailable=0, noNativeResponse=0)
    if not navigator:
        counts = dict(correct=0, incorrectLocations=0, unavailable=0)
    for (site, method), row in zip(operations, query[rows_key]):
        row['status'] = assess(site, method, row)
        counts[row['status']] += 1
    if query.get('firstRequestRetry'):
        query['firstRequestRetry']['status'] = assess(*operations[0], query['firstRequestRetry'])
    return counts
