#!/usr/bin/env python3
"""Diagnostic real-Grace lifecycle trace on two preselected successful source sites.

This deliberately does not replace the fixed 46-query coverage denominator. All
edits are unsaved document overlays; a separate compiler-oracle run can validate
both edit shapes using the same expected sets before native timing is assessed.
"""
import argparse
import copy
import hashlib
import json
import shlex
import time
from pathlib import Path
from types import SimpleNamespace

from run import Session, canonical_location, file_sha, numeric_delta, snapshot_engine, source_sha

NAMES = ['record_creation_label_13_Kty', 'record_creation_label_12_OnFailure']


def rename_case(case, texts, new_name):
    """Rename exactly the compiler-proven occurrence set, preserving UTF-16 spans."""
    result = copy.deepcopy(case)
    changed = dict(texts)
    edits = {}
    for loc in case['reference_locations']:
        r = loc['range']; start = r['start']; end = r['end']
        if start['line'] != end['line']:
            raise ValueError('Expected single-line identifier occurrence')
        edits.setdefault(loc['path'], set()).add((start['line'], start['character'], end['character']))
    for path, spans in edits.items():
        lines = changed[path].splitlines(keepends=True)
        for line, start, end in sorted(spans, reverse=True):
            raw = lines[line].encode('utf-16-le')
            if raw[start * 2:end * 2].decode('utf-16-le') != case['token']:
                raise ValueError('Rename location does not match proven token')
            lines[line] = (raw[:start * 2] + new_name.encode('utf-16-le') + raw[end * 2:]).decode('utf-16-le')
        changed[path] = ''.join(lines)
    delta = len(new_name.encode('utf-16-le')) // 2 - len(case['token'].encode('utf-16-le')) // 2
    for loc in [result['declaration']] + result['reference_locations']:
        for boundary in ['start', 'end']:
            pos = loc['range'][boundary]
            pos['character'] += delta * sum(1 for line, _, end in edits.get(loc['path'], [])
                                           if line == pos['line'] and end <= pos['character'])
    result['character'] += delta * sum(1 for line, _, end in edits.get(result['path'], [])
                                       if line == result['line'] and end <= result['character'])
    result['token'] = new_name
    return result, changed


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--selection', type=Path, required=True)
    p.add_argument('--engine', required=True)
    p.add_argument('--engine-sha')
    p.add_argument('--model', type=Path)
    p.add_argument('--catalog', type=Path)
    p.add_argument('--builtin-environment', type=Path)
    p.add_argument('--project-path', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--warm', type=int, default=10)
    p.add_argument('--oracle', action='store_true', help='Correctness-only FCS oracle; no comparative timing interpretation')
    p.add_argument('--timeout', type=float, default=120)
    a = p.parse_args()
    selection = json.loads(a.selection.read_text())
    root = Path(selection['root'])
    cases = [next(x for x in selection['selected'] if x['name'] == name) for name in NAMES]
    texts = {q['path']: Path(q['path']).read_text() for q in cases}
    renamed, renamed_texts = rename_case(cases[0], texts, 'NavigatorKtyProbe')
    body_texts = dict(texts)
    lines = body_texts[cases[0]['path']].splitlines(keepends=True)
    old_line = lines[cases[0]['line']]
    if old_line.count('Kty = "EC"') != 1:
        raise ValueError('Body edit anchor changed')
    lines[cases[0]['line']] = old_line.replace('Kty = "EC"', 'Kty = "ED"')
    body_texts[cases[0]['path']] = ''.join(lines)
    work = a.output.resolve().parent / (a.output.stem + '-work')
    work.mkdir(parents=True, exist_ok=True)
    cache = work / 'cache'
    if cache.exists():
        raise ValueError('Fresh lifecycle trace requires a new output/cache path')
    args = SimpleNamespace(engine=a.engine, engine_sha=a.engine_sha, helper=None,
                           evaluated_model=a.model, library_catalog=a.catalog,
                           builtin_environment=a.builtin_environment, semantic_snapshot=None,
                           decompiler=None, manifest=a.selection)
    provenance = snapshot_engine(args, work)
    report = {
        'kind': 'compiler-edit-oracle' if a.oracle else 'native-selected-success-lifecycle-diagnostic',
        'lifecycle_script_sha256': file_sha(__file__),
        'provenance': provenance, 'selection_sha256': file_sha(a.selection),
        'selected_names': NAMES, 'fixed_coverage_denominator': 46,
        'project_path': str(a.project_path.resolve()), 'root': str(root),
        'prepared_inputs': {name: {'path': str(path.resolve()), 'sha256': file_sha(path)}
                            for name, path in [('model', a.model), ('catalog', a.catalog),
                                               ('builtin_environment', a.builtin_environment)] if path},
        'source_hashes_before': {path: file_sha(path) for path in texts},
        'edit_payload_sha256': {kind: {path: hashlib.sha256(text.encode()).hexdigest() for path, text in values.items()}
                               for kind, values in [('body', body_texts), ('api_rename', renamed_texts)]},
        'rows': [], 'sessions': [],
        'notes': [
            'Two sites selected after prior native success; this is not an unbiased coverage or whole-Grace performance sample.',
            'Fixed 23 source sites / 46 operation goldens remain the coverage gate; unavailable responses are failures.',
            'All requested features configured through explicit model/catalog/inventory paths; actual loaded state is reported by counters.',
            'Preparation and repository restore/build are prerequisites, not included in daemon timing. No comparative speedup is claimed.',
            'Fresh native cache then same-cache restart. Edits use unsaved overlays; repository source bytes never change.',
            'Summed process-tree RSS includes owned descendants; excludes controller and filesystem/page-cache memory.',
            'Diagnostic shared-machine measurements until explicitly designated a coordinated quiet acceptance run.'
        ]}
    def save():
        a.output.parent.mkdir(parents=True, exist_ok=True)
        a.output.write_text(json.dumps(report, indent=2) + '\n')
    def query(s, case, method, phase, prior_stats, edit_started=None):
        params = {key: case[key] for key in ['path', 'line', 'character']}
        params.update(projectPath=str(a.project_path.resolve()), includeDeclaration=True)
        response, measure = s.call('navigation/' + method, params)
        answered = time.perf_counter()
        after = s.stats()
        actual = sorted(set(canonical_location(loc, root) for loc in response.get('result', {}).get('locations', [])))
        locations = [case['declaration']] if method == 'definition' else case['reference_locations']
        expected = sorted(set(canonical_location(loc, root) for loc in locations))
        exact = 'error' not in response and response.get('result', {}).get('completeness') == 'exact'
        row = dict(phase=phase, name=case['name'], method=method, params=params,
                   response=response, actual=actual, expected=expected,
                   correct=exact and actual == expected, complete=exact,
                   since_process_start_ms=(answered - s.started) * 1000,
                   stats_before=prior_stats, stats_after=after,
                   counter_delta=numeric_delta(prior_stats, after), **measure)
        if edit_started is not None:
            row['edit_notification_to_answer_ms'] = (answered - edit_started) * 1000
        report['rows'].append(row); save()
        print(phase, case['name'], method, row['correct'], response.get('error') or response.get('result', {}).get('completeness'), flush=True)
        return after
    try:
        for restart in [False, True] if not a.oracle else [False]:
            s = Session(shlex.split(args.engine), timeout=a.timeout, memory_limit_mib=4096)
            try:
                params = {'root': str(root), 'mode': 'indexed', 'cacheDir': str(cache)}
                for key, path in [('evaluatedModelPath', a.model), ('metadataPath', a.catalog), ('builtinEnvironmentPath', a.builtin_environment)]:
                    if path: params[key] = str(path.resolve())
                response, measure = s.call('workspace/open', params)
                report['rows'].append(dict(phase='restart_open' if restart else 'cold_open', response=response, **measure))
                before_documents = s.stats()
                for path, text in texts.items():
                    response, measure = s.call('document/open', {'path': path, 'text': text, 'version': 1})
                    report['rows'].append(dict(phase='restart_document_open' if restart else 'cold_document_open', path=path, response=response, **measure))
                current_stats = s.stats()
                report['rows'].append(dict(phase='restart_open_stats' if restart else 'cold_open_stats',
                                           before_documents=before_documents, after_documents=current_stats,
                                           document_counter_delta=numeric_delta(before_documents, current_stats)))
                for case in cases:
                    for method in ['definition', 'references']:
                        current_stats = query(s, case, method, 'restart' if restart else 'first_use', current_stats)
                if restart: continue
                for repetition in range(0 if a.oracle else a.warm):
                    for case in reversed(cases) if repetition % 2 else cases:
                        for method in ['definition', 'references']:
                            current_stats = query(s, case, method, 'warm', current_stats)
                for version, phase, overlay, target in [
                    (2, 'body_edit', body_texts, cases[0]),
                    (3, 'body_undo', texts, cases[0]),
                    (4, 'api_rename', renamed_texts, renamed),
                    (5, 'api_undo', texts, cases[0])]:
                    # Build/validate payloads before timing; send only changed document.
                    params = {'path': cases[0]['path'], 'text': overlay[cases[0]['path']], 'version': version}
                    started = time.perf_counter()
                    response, measure = s.call('document/change', params)
                    report['rows'].append(dict(phase=phase + '_notification', response=response, **measure))
                    current_stats = query(s, target, 'references', phase, current_stats, started)
                    current_stats = query(s, target, 'definition', phase, current_stats)
            finally:
                s.close()
                report['sessions'].append({'restart': restart, 'sampler_cpu_seconds': s.sampler_cpu_seconds,
                                           'sampler_wall_seconds': s.sampler_wall_seconds, 'stderr': s.stderr,
                                           'peak_tree_rss_kib': max((sample[1] for sample in s.samples), default=0)})
                save()
    finally:
        report['source_hashes_after'] = {path: file_sha(path) for path in texts}
        report['prepared_hashes_after'] = {name: file_sha(value['path']) for name, value in report['prepared_inputs'].items()}
        report['source_sha256_after'] = source_sha()
        queries = [row for row in report['rows'] if 'correct' in row]
        report['summary'] = {'query_count': len(queries), 'correct': sum(row['correct'] for row in queries),
                             'unavailable': sum(not row['complete'] for row in queries),
                             'wrong_exact': sum(row['complete'] and not row['correct'] for row in queries),
                             'source_bytes_unchanged': report['source_hashes_before'] == report['source_hashes_after']}
        save()

if __name__ == '__main__':
    main()
