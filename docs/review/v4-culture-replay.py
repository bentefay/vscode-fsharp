#!/usr/bin/env python3
"""Independent genuine producer/consumer locale and contract replay; no timings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / 'docs/review/sdk100-profile-audit/culture-fixture-sdk400'
DOTNET = ROOT / '.tools/dotnet/dotnet'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--helper', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--old-binary', type=Path, default=ROOT / 'evaluation/.runtime/recovered-f27d9fc7d54b/fsharp-navigator')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    base = Path(tempfile.mkdtemp(prefix='v4-culture-review-', dir=ROOT / '.tmp'))
    env = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), FSHARP_NAVIGATOR_DOTNET=str(DOTNET), MSBUILDDISABLENODEREUSE='1', DOTNET_CLI_USE_MSBUILD_SERVER='0', DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1')
    for key in ['DOTNET_CLI_UI_LANGUAGE', 'VSLANG', 'LANGUAGE', 'LC_CTYPE', 'LC_MESSAGES', 'DOTNET_SYSTEM_GLOBALIZATION_INVARIANT']:
        env.pop(key, None)
    source_paths = [FIXTURE / p for p in ['global.json', 'P.fsproj', 'A.fs', 'Sources/i/source.fs', 'Sources/ı/source.fs']]
    sources = {str(path): digest(path) for path in source_paths}
    report = dict(provenance='Independent replay. Existing fixture already has two successful ordinary builds with unchanged bytes, retained in culture-probe-sdk400.json. Models here are genuine fresh helper outputs, never schema-rewritten. Distinct cache per arm; no timing claim.', fixture=str(FIXTURE), workRoot=str(base), binarySha256=digest(args.binary), oldBinarySha256=digest(args.old_binary), helperSha256=digest(args.helper), sourceHashes=sources, preparations=[], cases=[])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    def save():
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    models = {}
    for locale in ['en_US.UTF-8', 'tr_TR.UTF-8']:
        request = dict(id=1, method='workspace/evaluate', params=dict(root=str(FIXTURE), projects=[str(FIXTURE / 'P.fsproj')]))
        current = dict(env, LANG=locale, LC_ALL=locale)
        process = subprocess.run([str(DOTNET), str(args.helper.resolve()), '--once'], input=json.dumps(request) + '\n', capture_output=True, text=True, env=current, cwd=FIXTURE, timeout=120)
        rows = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{')]
        response = next((row for row in rows if row.get('id') == 1), {})
        model = response.get('result')
        record = dict(locale=locale, request=request, exitCode=process.returncode, response=response, stderr=process.stderr)
        report['preparations'].append(record)
        save()
        if not model:
            raise RuntimeError('Actual evaluation failed; retained raw response')
        model_path = base / (locale.split('.')[0] + '-v4.json')
        model_path.write_text(json.dumps(model, indent=2) + '\n')
        record.update(modelPath=str(model_path), modelSha256=digest(model_path))
        models[locale] = model_path
    rows = [
        ('initial', args.binary, 'en_US.UTF-8', models['en_US.UTF-8'], 0),
        ('stale-locale', args.binary, 'tr_TR.UTF-8', models['en_US.UTF-8'], None),
        ('fresh-locale', args.binary, 'tr_TR.UTF-8', models['tr_TR.UTF-8'], 1),
        ('old-v3-en-rejected', args.binary, 'en_US.UTF-8', FIXTURE / 'en_US.json', None),
        ('old-v3-tr-rejected', args.binary, 'tr_TR.UTF-8', FIXTURE / 'tr_TR.json', None),
        ('old-reader-new-en-rejected', args.old_binary, 'en_US.UTF-8', models['en_US.UTF-8'], None),
        ('old-reader-new-tr-rejected', args.old_binary, 'tr_TR.UTF-8', models['tr_TR.UTF-8'], None),
    ]
    for name, binary, locale, model, count in rows:
        requests = [dict(id=1, method='workspace/open', params=dict(root=str(FIXTURE), mode='indexed', evaluatedModelPath=str(model), cacheDir=str(base / name))), dict(id=2, method='navigation/references', params=dict(path=str(FIXTURE / 'A.fs'), line=1, character=5, includeDeclaration=False)), dict(id=3, method='shutdown', params={})]
        process = subprocess.run([str(binary.resolve()), 'daemon'], input=''.join(json.dumps(row) + '\n' for row in requests), capture_output=True, text=True, env=dict(env, LANG=locale, LC_ALL=locale), cwd=FIXTURE, timeout=45)
        responses = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{')]
        answer = next((row for row in responses if row.get('id') == 2), {})
        result = answer.get('result', {})
        exact = result.get('completeness') == 'exact'
        if count is None:
            passed = not exact and bool(answer.get('error'))
        else:
            expected = [] if count == 0 else [dict(uri=(FIXTURE / 'Sources/ı/source.fs').as_uri(), range=dict(start=dict(line=1, character=12), end=dict(line=1, character=17)))]
            passed = exact and result.get('locations') == expected
        startup = next((row.get('result', {}) for row in responses if row.get('id') == 1), {})
        startup_zero = all(startup.get(key) == 0 for key in ['filesRead', 'filesParsed', 'bytesRead', 'helperStarts', 'compilerLaunches'])
        report['cases'].append(dict(case=name, binarySha256=digest(binary), locale=locale, model=str(model), expectedReferenceCount=count, passed=passed, startupZeroSourceAndHelpers=startup_zero, exitCode=process.returncode, requests=requests, responses=responses, stderr=process.stderr))
        save()
        print(name, 'PASS' if passed else 'FAIL', answer)
    report['sourceBytesUnchanged'] = sources == {str(path): digest(path) for path in source_paths}
    report['summary'] = dict(passed=sum(row['passed'] for row in report['cases']), total=len(report['cases']), startupZero=all(row['startupZeroSourceAndHelpers'] for row in report['cases']))
    save()
    print(report['summary'])


if __name__ == '__main__':
    main()
