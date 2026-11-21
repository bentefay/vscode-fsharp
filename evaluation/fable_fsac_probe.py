#!/usr/bin/env python3
"""Bounded, explicitly selected Fable fcs-fable FSAC feasibility workload.

Sites are declared before responses and retained even when unavailable. This is
setup/correctness evidence, not a controlled performance comparison.
"""
import json
import os
import sys
import time
from pathlib import Path
from run import Session, file_sha

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent / 'research/corpus/Fable'
SCOPE = REPO / 'src/fcs-fable'
PROJECT = SCOPE / 'fcs-fable.fsproj'
SDK = ROOT / '.tools/dotnet-sdk100/dotnet'
OUT = ROOT / 'evaluation/results/fable-fcs-fsac-feasibility.json'
TRACE = OUT.with_suffix('.lsp.jsonl')

SITE_SPECS = [
    ('late_local_parameter', 'service_slim.fs', 106, 'readAllBytes'),
    ('late_record_field_access', 'service_slim.fs', 170, 'checkCache'),
    ('late_type_alias', 'service_slim.fs', 58, 'TcResult'),
    ('late_record_property', 'service_slim.fs', 73, 'OtherOptions'),
    ('signature_paired_function', 'src/Compiler/Facilities/DiagnosticResolutionHints.fs', 98, 'JaroWinklerDistance'),
    ('middle_local_argument', 'src/Compiler/Facilities/DiagnosticResolutionHints.fs', 98, 'uppercaseText'),
]

def main():
    os.environ.update(DOTNET_ROOT=str(SDK.parent), MSBUILDDISABLENODEREUSE='1',
                      TMPDIR=str(ROOT / '.tmp/evaluation-recovery'))
    sites = []
    for name, relative, lineno, token in SITE_SPECS:
        path = SCOPE / relative
        line = path.read_text().splitlines()[lineno - 1]
        start = line.index(token)
        sites.append({'name': name, 'path': str(path), 'line': lineno - 1,
                      'character': len(line[:start].encode('utf-16-le')) // 2,
                      'token': token, 'lineText': line, 'sourceSha256': file_sha(path)})
    command = [sys.executable, str(ROOT / 'evaluation/fsac_adapter.py'), '--dotnet', str(SDK),
               '--ionide', str(ROOT / 'evaluation/.runtime/extensions/Ionide.Ionide-fsharp-8.1.0'),
               '--timeout', '90', '--trace', str(TRACE)]
    report = {'kind': 'actual-Ionide-FSAC-selected-Fable-project-feasibility',
              'selection_version': 'fable-fcs-feasibility-v1-predeclared-six-sites',
              'scope': str(SCOPE), 'projects': [str(PROJECT)], 'sites': sites,
              'command': command, 'adapter_sha256': file_sha(ROOT / 'evaluation/fsac_adapter.py'),
              'performance_acceptance': False, 'independent_oracle_verified': False,
              'phases': [], 'queries': []}
    OUT.write_text(json.dumps(report, indent=2) + '\n')
    session = Session(command, timeout=95, memory_limit_mib=4096)
    deadline = time.monotonic() + 240
    try:
        response, measurement = session.call('workspace/open', {
            'root': str(SCOPE), 'projects': [str(PROJECT)],
            'cacheDir': str(ROOT / 'evaluation/.runtime/fable-fcs-fsac-cache')})
        report['phases'].append({'phase': 'workspace_open', 'response': response, 'measurement': measurement})
        print(json.dumps({'phase': 'workspace_open', 'success': 'error' not in response, 'measurement': measurement}), flush=True)
        if 'error' not in response:
            opened = set()
            for site in sites:
                if time.monotonic() > deadline or session.failed:
                    report['stopped_reason'] = 'bounded feasibility deadline or failed session'
                    break
                if site['path'] not in opened:
                    opened.add(site['path'])
                    opened_response, opened_measurement = session.call('document/open', {
                        'path': site['path'], 'text': Path(site['path']).read_text(), 'version': 1})
                    report['phases'].append({'phase': 'document_open', 'path': site['path'],
                                            'response': opened_response, 'measurement': opened_measurement})
                for feature in ('definition', 'references'):
                    if time.monotonic() > deadline or session.failed:
                        break
                    query = {k: site[k] for k in ('path', 'line', 'character')}
                    query.update(projectPath=str(PROJECT), includeDeclaration=True)
                    answer, measure = session.call('navigation/' + feature, query)
                    report['queries'].append({'name': site['name'] + '_' + feature,
                                              'query': query, 'response': answer, 'measurement': measure})
                    OUT.write_text(json.dumps(report, indent=2) + '\n')
                    print(json.dumps({'query': report['queries'][-1]['name'], 'error': answer.get('error'),
                                      'locations': len(answer.get('result', {}).get('locations', [])),
                                      'measurement': measure}), flush=True)
    finally:
        session.close()
        report['stderr'] = session.stderr
        report['samples'] = session.detailed_samples
        report['peak_raw_tree_rss_kib'] = max((x[1] for x in session.samples), default=0)
        report['trace_sha256'] = file_sha(TRACE) if TRACE.exists() else None
        if TRACE.exists():
            events = [json.loads(x) for x in TRACE.read_text().splitlines()]
            report['server_project_notifications'] = [x for x in events if x['direction'] == 'server'
                and x['message'].get('method') == 'fsharp/notifyWorkspace']
            report['error_logs'] = [x for x in events if x['direction'] == 'server'
                and x['message'].get('method') == 'window/logMessage'
                and x['message'].get('params', {}).get('type', 4) <= 2]
        OUT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(OUT), 'queries': len(report['queries']),
                      'peak_raw_tree_rss_kib': report['peak_raw_tree_rss_kib']}), flush=True)

if __name__ == '__main__':
    main()
