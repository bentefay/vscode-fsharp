#!/usr/bin/env python3
"""Check locked Cargo/NuGet release ages, including Cargo's locked-version gap.

Registry metadata only; downloads/executes no package code. This is an age and
yank/listing check, not Safe Chain malware scanning or an advisory database.
"""
import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
from pathlib import Path
import time
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
MIN_AGE = dt.timedelta(hours=48)


def packages(root):
    result = set()
    for lock in [root / 'Cargo.lock', *sorted((root / 'evaluation').rglob('Cargo.lock'))]:
        for row in tomllib.loads(lock.read_text())['package']:
            if 'source' not in row:
                continue  # Workspace and reviewed vendored sources have no registry release.
            if row['source'] != 'registry+https://github.com/rust-lang/crates.io-index':
                raise ValueError(f'Unreviewed Cargo source in {lock}: {row["source"]}')
            result.add(('cargo', row['name'], row['version']))
    for lock in sorted((root / 'semantic').rglob('packages*.lock.json')):
        if 'obj' in lock.parts or 'bin' in lock.parts:
            continue
        for framework in json.loads(lock.read_text())['dependencies'].values():
            for name, row in framework.items():
                if row['type'].lower() != 'project':
                    result.add(('nuget', name.lower(), row['resolved']))
    for manifest in sorted((root / 'semantic').rglob('runtime-packs.*.json')):
        for row in json.loads(manifest.read_text())['packages']:
            result.add(('nuget', row['id'].lower(), row['version']))
    return sorted(result)


def fetch(url, data=None):
    for attempt in range(3):
        try:
            request = urllib.request.Request(url, data=None if data is None else json.dumps(data).encode(),
                                             headers={'User-Agent': 'fsharp-navigator-dependency-review/1', 'Content-Type': 'application/json'})
            with urllib.request.urlopen(request, timeout=30) as response:
                raw = response.read(8 * 1024 * 1024 + 1)
                if len(raw) > 8 * 1024 * 1024:
                    raise ValueError('Registry response exceeded limit')
                return json.loads(raw)
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 502, 503, 504) or attempt == 2:
                raise
            time.sleep(1 + attempt)


def release(package):
    ecosystem, name, version = package
    if ecosystem == 'cargo':
        row = fetch(f'https://crates.io/api/v1/crates/{name}/{version}')['version']
        return row['created_at'], not row['yanked']
    leaf = fetch(f'https://api.nuget.org/v3/registration5-semver1/{name}/{version.lower()}.json')
    return leaf['published'], leaf['listed']


def validate(package, published, available, now):
    if not available:
        raise ValueError(f'{package}: release is yanked or unlisted')
    age = now - dt.datetime.fromisoformat(published.replace('Z', '+00:00'))
    if age < MIN_AGE:
        raise ValueError(f'{package}: release is younger than 48 hours ({published})')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cached', action='store_true')
    parser.add_argument('--audit-nuget', action='store_true')
    args = parser.parse_args()
    selected = packages(ROOT)
    if args.audit_nuget:
        nuget = [row for row in selected if row[0] == 'nuget']
        if nuget:
            result = fetch('https://api.osv.dev/v1/querybatch', {'queries': [
                {'package': {'ecosystem': 'NuGet', 'name': name}, 'version': version} for _, name, version in nuget
            ]})['results']
            if len(result) != len(nuget) or any(row.get('next_page_token') or row.get('vulns') for row in result):
                raise ValueError(f'NuGet advisory audit failed: {list(zip(nuget, result))}')
        print(f'No OSV advisory matches for {len(nuget)} locked NuGet releases; SDK assemblies are outside this scan.')
    key = hashlib.sha256(Path(__file__).read_bytes() + json.dumps(selected).encode()).hexdigest()
    cache = ROOT / '.cache/dependency-age.json'
    now = dt.datetime.now(dt.timezone.utc)
    if args.cached and cache.is_file():
        evidence = json.loads(cache.read_text())
        elapsed = now.timestamp() - evidence.get('checkedAt', 0)
        if evidence.get('key') == key and 0 <= elapsed < 86400:
            return
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        rows = list(pool.map(release, selected))
    for package, (published, available) in zip(selected, rows):
        validate(package, published, available, now)
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps({'key': key, 'checkedAt': now.timestamp(), 'releases': [
        {'package': package, 'published': row[0]} for package, row in zip(selected, rows)
    ]}, indent=2) + '\n')
    print(f'Validated 48-hour age and listing status for {len(selected)} locked Cargo/NuGet releases.')


if __name__ == '__main__':
    main()
