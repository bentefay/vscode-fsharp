#!/usr/bin/env python3
"""Genuinely reevaluate existing compiler-tested fixtures under the current policy.

The old model files and binary metadata are preserved. Reusing the old binary
metadata is permitted here only when ordered compiler inputs, references and
workspace ownership remain unchanged after actual helper evaluation.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path

import smoke


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    helper = args.helper.resolve()
    smoke.HELPER = helper
    original = json.loads((args.fixture / 'manifest.json').read_text())
    manifest = copy.deepcopy(original)
    records = []
    comparable = ['path', 'files', 'references', 'defines', 'targetFramework', 'configuration', 'compilerArgs', 'assemblyReferences', 'referenceBindings', 'workspaceAssemblyReferences']
    for name, program in manifest['programs'].items():
        previous = json.loads(Path(program['model']).read_text())
        projects = [project['path'] for project in previous['projects']]
        current = smoke.request('workspace/evaluate', {'root': program['root'], 'projects': projects})
        assert current['completeness'] == 'exact', current
        assert len(current['projects']) == len(previous['projects'])
        prior = {project['path']: project for project in previous['projects']}
        for project in current['projects']:
            assert project['membershipReusable'] is True
            certificate = project['environmentCertificate']
            assert certificate['version'] == 4 and certificate['contract'] == 'dotnet-msbuild-observed-v4'
            assert certificate['globalization']['files'] and certificate['globalization']['selectionDirectories'] and certificate['globalization']['pathLinks']
            for field in comparable:
                assert project.get(field) == prior[project['path']].get(field), (name, field, project.get(field), prior[project['path']].get(field))
        path = output / f'{name}-model.json'
        path.write_text(json.dumps(current, indent=2) + '\n')
        records.append({'name': name, 'previousModel': program['model'], 'previousModelSha256': sha(program['model']), 'model': str(path), 'modelSha256': sha(path), 'orderedCompilerInputsAndOwnershipUnchanged': True, 'metadataEnvironmentSha256': sha(program['environment']), 'certificateVersion': 4})
        program['model'] = str(path)
        print('EVALUATED genuine v4 model: ' + name, flush=True)
    manifest['modelHelper'] = str(helper)
    manifest['modelHelperSha256'] = sha(helper)
    manifest['modelCertificateVersion'] = 4
    manifest['sourceFixture'] = str(args.fixture.resolve())
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    report = {'kind': 'genuine-project-model-policy-refresh', 'helper': str(helper), 'helperSha256': sha(helper), 'sourceFixture': str(args.fixture.resolve()), 'records': records, 'metadataReexported': False, 'limits': 'Actual helper evaluation, not a hand-edited certificate upgrade. Existing binary facts reused only after exact ordered compiler-input/reference/ownership equality; original compiler oracles and metadata provenance remain attached.'}
    (output / 'model-refresh.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'manifest': str(output / 'manifest.json'), 'models': len(records), 'helperSha256': sha(helper)}))


if __name__ == '__main__':
    main()
