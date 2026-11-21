#!/usr/bin/env python3
"""Small read-only cursor decomposition for residual fixed Grace misses."""
import json
from pathlib import Path
from run import Session, file_sha

root = Path(__file__).resolve().parents[1]
scope = root.parent / 'research/corpus/Grace/src/Grace.Types'
engine = root / 'evaluation/.runtime/recovered-858f9d9a16d3/fsharp-navigator'
assert file_sha(engine) == '858f9d9a16d39338b63406916bf0fdf0bb2138a3b7b2ae9efd9bdc0d316bd14d'
prepared = root / 'evaluation/.runtime/recovery-prepared-grace-f215'
output = root / 'evaluation/results/recovered-858f-priority-cursors.json'
specs = [
    ('typed_value', scope/'UsageObservation.Types.fs', 36, 'value'),
    ('typed_scope', scope/'UsageObservation.Types.fs', 36, 'Scope'),
    ('typed_repository', scope/'UsageObservation.Types.fs', 36, 'RepositoryId'),
    ('scope_type_annotation', scope/'UsageObservation.Types.fs', 16, 'UsageFactScope'),
    ('type_test_binder', scope/'ManifestContributionWorkflow.Types.fs', 50, 'otherRange'),
    ('type_test_field', scope/'ManifestContributionWorkflow.Types.fs', 50, 'StoragePoolId', 1),
    ('serialize_declaration', scope.parent/'Grace.Shared/Utilities.Shared.fs', 214, 'serialize'),
    ('union_name_declaration', scope.parent/'Grace.Shared/Utilities.Shared.fs', 156, 'getDiscriminatedUnionFullName'),
    ('state_creator_declaration', scope/'Reminder.Types.fs', 72, 'CreateWithId'),
    ('state_creator_label', scope/'Reminder.Types.fs', 85, 'State'),
]
report = {'kind': 'non-scored-read-only-query-decomposition', 'engine_sha256': file_sha(engine), 'rows': [],
          'performance_acceptance': False, 'notes': ['Fixed46 goldens/selection unchanged; these extra cursors are diagnostic only.']}
session = Session([str(engine), 'daemon'], timeout=120, memory_limit_mib=2048)
opened = set()
try:
    report['open'], _ = session.call('workspace/open', {'root': str(scope), 'mode': 'indexed',
        'cacheDir': str(root/'evaluation/.runtime/priority-858f-cache'),
        'evaluatedModelPath': str(root/'evaluation/.runtime/recovery-prepared-grace-v4-09f011/model.json'),
        'metadataPath': str(prepared/'catalog.json'), 'builtinEnvironmentPath': str(prepared/'builtin-environment.json')})
    for spec in specs:
        name, path, line, token, *rest = spec
        text = path.read_text(); source_line = text.splitlines()[line-1]
        start = -1
        for _ in range((rest[0] if rest else 0)+1):
            start = source_line.index(token, start+1)
        if path not in opened:
            session.call('document/open', {'path': str(path), 'text': text, 'version': 1}); opened.add(path)
        params = {'path': str(path), 'line': line-1, 'character': len(source_line[:start].encode('utf-16-le'))//2,
                  'projectPath': str(scope/'Grace.Types.fsproj')}
        response, _ = session.call('navigation/definition', params)
        report['rows'].append({'name': name, 'params': params, 'source_line': source_line,
                              'source_sha256': file_sha(path), 'response': response})
        print(name, response.get('result', {}).get('completeness'), response.get('result', {}).get('locations'), flush=True)
finally:
    session.close()
    output.write_text(json.dumps(report, indent=2)+'\n')
