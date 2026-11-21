#!/usr/bin/env python3
"""Replay genuine ownership/shadowing cases through prepared read-only facts.

Only complete metadata artifacts are derived. The deliberately missing or
incomplete negative fixtures remain unchanged. Models and source files are the
same actual compiler-tested inputs used by the original resolution-state suite.
"""
import argparse
import hashlib
import json
from pathlib import Path

from prepared_indexes import prepare, validate_maps
from resolution_states import replay


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--fixture', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    original = args.fixture.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((original / 'manifest.json').read_text())
    builds = []
    generations = {}

    def derive(path, kind):
        path = Path(path).resolve()
        key = (str(path), kind)
        if key in generations:
            return generations[key]
        destination = output / f'{kind}-{len(generations):02d}.fnmeta'
        row = prepare(binary, kind, path, destination)
        row['sourceSha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
        if kind == 'environment':
            row['declarationInventories'] = validate_maps(path, destination)
        builds.append(row)
        generations[key] = str(destination)
        return str(destination)

    for program in manifest['programs'].values():
        program['environment'] = derive(program['environment'], 'environment')
    for key in ['catalog', 'collisionCatalog']:
        manifest[key] = derive(manifest[key], 'catalog')
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    # Both paths are tested on exactly this binary. No model, declaration,
    # ownership exclusion or producer certificate is manually upgraded.
    direct = replay(original, binary)
    prepared = replay(output, binary)
    assert direct['passed'] == direct['total'] == prepared['passed'] == prepared['total'] == 16
    for left, right in zip(direct['rows'], prepared['rows']):
        assert left['name'] == right['name']
        a, b = left['response'].get('result', {}), right['response'].get('result', {})
        for key in ['completeness', 'hover', 'symbol', 'ownership']:
            assert a.get(key) == b.get(key), (left['name'], key, a, b)
    report = {
        'kind': 'prepared-metadata-genuine-resolution-state-regression',
        'sourceFixture': str(original),
        'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'preparation': builds,
        'jsonReplay': direct,
        'preparedReplay': prepared,
        'passed': True,
        'limits': 'Same actual compiler-tested models, source and exported facts. Safety passes are not feature coverage. Runtime values are diagnostics; no coordinated quiet window or editor-host performance claim.',
    }
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(output / 'report.json'), 'passed': True, 'cases': 16, 'preparedGenerations': len(generations)}))


if __name__ == '__main__':
    main()
