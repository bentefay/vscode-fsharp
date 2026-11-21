#!/usr/bin/env python3
"""Reevaluate a retained review workspace; reuse binary facts only on equal context."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT / '.tools/dotnet/dotnet'
CONTEXT = ['path', 'targetFramework', 'configuration', 'files', 'references', 'assemblyReferences', 'workspaceAssemblyReferences', 'compilerArgs', 'outputPath', 'referenceBindings', 'evaluation']


def context(model):
    return [{key: project.get(key) for key in CONTEXT} for project in sorted(model['projects'], key=lambda row: row['path'])]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--prepared', type=Path, required=True)
    ap.add_argument('--helper', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    report = json.loads(args.prepared.read_text())
    old_path = Path(report['modelPath'])
    old = json.loads(old_path.read_text())
    env = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1', MSBUILDDISABLENODEREUSE='1', DOTNET_CLI_USE_MSBUILD_SERVER='0', TMPDIR=str(ROOT / '.tmp'))
    request = dict(id=1, method='workspace/evaluate', params=dict(root=old['root'], projects=[project['path'] for project in old['projects']]))
    result = subprocess.run([str(DOTNET), str(args.helper.resolve()), '--once'], input=json.dumps(request) + '\n', env=env, text=True, capture_output=True, timeout=180)
    response = next((json.loads(line) for line in result.stdout.splitlines() if line.startswith('{') and json.loads(line).get('id') == 1), {})
    model = response.get('result')
    work = Path(tempfile.mkdtemp(prefix='review-model-refresh-', dir=ROOT / '.tmp'))
    evidence = dict(previousPrepared=str(args.prepared.resolve()), previousModel=str(old_path), helper=str(args.helper.resolve()), helperSha256=hashlib.sha256(args.helper.read_bytes()).hexdigest(), request=request, exitCode=result.returncode, response=response, stderr=result.stderr)
    (work / 'evaluation.json').write_text(json.dumps(evidence, indent=2) + '\n')
    assert model and result.returncode == 0, f'Actual model evaluation failed; see {work}/evaluation.json'
    evidence['contextExactlyEqual'] = context(old) == context(model)
    evidence['contextFields'] = CONTEXT
    (work / 'evaluation.json').write_text(json.dumps(evidence, indent=2) + '\n')
    assert evidence['contextExactlyEqual'], f'Context changed; metadata reuse refused: {work}/evaluation.json'
    assert all(p.get('membershipReusable') and p['environmentCertificate']['version'] == 4 for p in model['projects']), f'Fresh v4 model is not reusable: {work}/evaluation.json'
    model_path = work / 'model-v4.json'
    model_path.write_text(json.dumps(model, indent=2) + '\n')
    report['modelPath'] = str(model_path)
    report['modelRefresh'] = dict(previousPrepared=str(args.prepared.resolve()), evidence=str(work / 'evaluation.json'), modelSha256=hashlib.sha256(model_path.read_bytes()).hexdigest(), helperSha256=evidence['helperSha256'], contextExactlyEqual=True, retainedMetadataPath=report['environmentPath'])
    report['provenance'] += ' Model independently re-evaluated through a genuine v4 helper; full compile/source/reference context exactly compared before retaining the original schema2 binary inventory. Original model and reports remain unchanged.'
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(args.output, model_path)


if __name__ == '__main__':
    main()
