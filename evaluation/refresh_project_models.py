#!/usr/bin/env python3
"""Re-evaluate genuine project certificates; never edit old/new model fields.

Existing binary metadata can be reused only after the exact source/assembly
context is compared. Original project selections and goldens remain unchanged.
"""
import argparse
import json
from pathlib import Path
from run import Session, file_sha

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', type=Path, required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('workloads', nargs='*', default=['grace', 'expanded'])
    args = parser.parse_args()
    helper = args.helper.resolve()
    assert file_sha(helper) == args.sha256
    for workload in args.workloads:
        old_dir = ROOT / f'evaluation/.runtime/recovery-prepared-{workload}-f215'
        old_path = old_dir / 'model.json'
        old = json.loads(old_path.read_text())
        out_dir = ROOT / f'evaluation/.runtime/recovery-prepared-{workload}-{args.tag}'
        out_dir.mkdir(parents=True, exist_ok=True)
        report_path = ROOT / f'evaluation/results/recovery-{workload}-model-{args.tag}.json'
        params = {'root': old['root']}
        report = {'kind': 'genuine-project-model-refresh-with-context-comparison',
                  'workload': workload, 'helper': str(helper), 'helper_sha256': args.sha256,
                  'old_model': str(old_path), 'old_model_sha256': file_sha(old_path),
                  'request': {'method': 'workspace/evaluate', 'params': params},
                  'notes': ['New producer response is saved as returned; no certificate fields are edited.',
                            'No restore/build/source/configuration change is requested.',
                            'Setup measurements are diagnostic, not quiet accepted performance samples.']}
        session = Session([str(ROOT / '.tools/dotnet/dotnet'), str(helper)], timeout=120, memory_limit_mib=2048)
        try:
            response, measurement = session.call('workspace/evaluate', params)
            report.update(response=response, measurement=measurement)
            if 'result' in response:
                new = response['result']
                model_path = out_dir / 'model.json'
                model_path.write_text(json.dumps(new, indent=2) + '\n')
                report.update(model_path=str(model_path), model_sha256=file_sha(model_path))
                before = {p['path']: p for p in old['projects']}
                after = {p['path']: p for p in new['projects']}
                fields = ['targetFramework', 'configuration', 'files', 'references',
                          'assemblyReferences', 'workspaceAssemblyReferences', 'compilerArgs', 'outputPath']
                comparisons = []
                for path in sorted(set(before) | set(after)):
                    a, b = before.get(path), after.get(path)
                    comparisons.append({'project': path, 'present_before_and_after': a is not None and b is not None,
                        'equal_fields': {field: a.get(field) == b.get(field) for field in fields} if a and b else {},
                        'membershipReusable': b.get('membershipReusable') if b else None,
                        'membershipDiagnostics': b.get('membershipDiagnostics') if b else None,
                        'environmentContract': b.get('environmentCertificate', {}).get('contract') if b else None})
                report['context_comparison'] = comparisons
                report['unchanged_context'] = set(before) == set(after) and all(all(c['equal_fields'].values()) for c in comparisons)
                report['all_membership_reusable'] = all(c['membershipReusable'] for c in comparisons)
                report['existing_binary_metadata'] = {name: {'path': str(old_dir / name), 'sha256': file_sha(old_dir / name)}
                                                       for name in ['catalog.json', 'builtin-environment.json']}
        finally:
            session.close()
            report['helper_sha256_after'] = file_sha(helper)
            report['stderr'] = session.stderr
            report_path.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'workload': workload, 'report': str(report_path), 'model': report.get('model_path'),
                          'unchanged_context': report.get('unchanged_context'),
                          'all_membership_reusable': report.get('all_membership_reusable'),
                          'error': report.get('response', {}).get('error')}), flush=True)

if __name__ == '__main__':
    main()
