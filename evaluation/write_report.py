#!/usr/bin/env python3
"""Create the concise delivery report from a single reviewed, pinned run matrix."""
import argparse
import collections
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
RESULTS=ROOT/'evaluation'/'results'


def summary(report,phase,mode='indexed'):
    return next(row for row in report['summary'] if row['phase']==phase and row['mode']==mode)


def phase_rows(report,phase,mode='indexed'):
    return [row for row in report['rows'] if row.get('phase')==phase and row.get('mode')==mode]


def coverage(report):
    rows=[r for r in phase_rows(report,'correctness') if r['repetition']==0]
    return sum(r.get('correct') is True for r in rows),len(rows)


def peak(report):
    return max((r.get('peak_tree_rss_kib',0) for r in report['rows']),default=0)/1024


def number(value,digits=2):
    return 'unavailable' if value is None else f'{value:.{digits}f}'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine-sha',required=True)
    args=parser.parse_args()
    names=['default','prepared','semantic-cache','library-hover','fsharp','grace','b2r2']
    reports={name:json.loads((RESULTS/f'final-{name}.json').read_text()) for name in names}
    for name,data in reports.items():
        if data['provenance']['engine_binary_sha256']!=args.engine_sha:
            raise ValueError(f'{name} is not from the final frozen executable')
    profile=json.loads((RESULTS/'final-file-profile.json').read_text())
    if profile['engine_sha256']!=args.engine_sha:
        raise ValueError('File profile is not from the final frozen executable')
    lines=['# F# Navigator prototype evaluation','',
      '**The full requested replacement and relative performance goals are not achieved.** The default Rust engine supports a limited exact subset. Explicitly prepared semantic snapshots cover more clean-source cases, but ordinary edits invalidate their answers. No matched Ionide/Rider performance baseline was obtained.','',
      f'Frozen native executable SHA-256: `{args.engine_sha}`. Each run executed an immutable copy, with executable, source, lockfile and prepared-input hashes retained in JSON.','',
      '## Correctness before performance','',
      '| Configuration | Clean fixture queries correct | Repeated warm replies correct | Wrong exact replies observed |',
      '|---|---:|---:|---:|']
    labels={'default':'Default Rust, no compiler/catalog bootstrap','prepared':'Rust + evaluated project model and library catalog','semantic-cache':'Rust + explicit FCS occurrence snapshot'}
    for name in ['default','prepared','semantic-cache']:
        report=reports[name];correct,count=coverage(report);warm=summary(report,'warm_query')
        wrong=sum(r.get('correct') is False and r.get('completeness')=='exact' for r in report['rows'] if r.get('mode')=='indexed')
        lines.append(f"| {labels[name]} | {correct}/{count} | {warm['correct']}/{warm['checked']} | {wrong} |")
    lines += ['', 'The fixture uses real compiled F# projects and exact occurrence-set checks. It covers parameters, shadowing, aliases, records, overloads, project order, linked files with different defines, `.fsi` boundaries, Unicode, escaped identifiers, library-only hover, `open`/`AutoOpen`, private accessibility and signature hiding. Qualified reference ranges have explicit alternatives matching observed FCS conventions; extra or missing occurrences still fail.', '',
      'The FCS oracle was actually run for the original 27 cases and the two added accessibility cases. It invalidates compiler state before each query and is a correctness oracle, **not a warmed Ionide/FCS performance baseline**. Unsupported responses count as failures. Successful stale-answer guards do not count as fast navigation answers.','']
    for name in ['default','prepared','semantic-cache']:
        failures=[r['query'] for r in phase_rows(reports[name],'correctness') if r['repetition']==0 and r.get('correct') is False]
        if failures:lines.append(f"- {labels[name]} unsupported/failing cases: "+', '.join(f'`{x}`' for x in failures)+'.')
    lines += ['', '## Startup, first lookup and retained memory','',
      'Five process-cold/cache-empty fixture repetitions; OS file caches were not flushed. RSS is the sum of the daemon and every observed descendant, sampled every 10 ms. This excludes the test controller and whole editor, counts shared pages more than once, and can miss short peaks.','',
      '| Configuration | Open p50 / p95 ms | Open + first local answer p50 / p95 ms | Peak measured tree MiB |',
      '|---|---:|---:|---:|']
    for name in ['default','prepared','semantic-cache']:
        report=reports[name];opened=summary(report,'cold_start');first=summary(report,'cold_start_to_first_answer')
        lines.append(f"| {labels[name]} | {number(opened['p50_ms_all_responses'])} / {number(opened['p95_ms_all_responses'])} | {number(first['p50_ms_all_responses'])} / {number(first['p95_ms_all_responses'])} | {peak(report):.2f} |")
    lines.append('')
    for name in ['default','prepared','semantic-cache']:
        report=reports[name];starts=phase_rows(report,'cold_start');opens=phase_rows(report,'document_open')
        passed=sum(all(r.get('startup_zero_source_work_gates',{}).values()) and bool(r.get('startup_zero_source_work_gates')) for r in starts)
        lines.append(f"- {labels[name]}: zero instrumented reads/parses/compiler launches at workspace/open in {passed}/{len(starts)} runs; zero reported AST work in {sum(r.get('zero_ast_work') is True for r in opens)}/{len(opens)} buffer-open notifications.")
    hover=reports['library-hover'];first=summary(hover,'cold_start_to_first_answer');warm=summary(hover,'warm_query')
    lines += ['',f"With the real all-reference library catalog enabled, first external hover including process/open/import cost was p50 {number(first['p50_ms_all_responses'])} ms, p95 {number(first['p95_ms_all_responses'])} ms; subsequent exact hover p95 was {number(warm['p95_ms_exact_correct'])} ms. Peak observed process-tree RSS was {peak(hover):.2f} MiB. The separate first-local-query run above does not need to import library declarations.", '',
      'Reader counters require care: semantic snapshot validation hashes source **and assemblies/project artifacts**. A large `bytesRead` value in this mode is not all F# source text. Evaluated-model validation also hashes build/assembly inputs on the first query; these inputs are not all charged to the source-read counters. Default corpus runs have no imported semantic snapshot or catalog; their source-read counters are more directly interpretable. Wall-clock latency and subtree RSS include the actual validation/import work.','',
      '## Edits and cache restoration','',
      '| Configuration | Body edit: exact answers | API edit: exact answers | Process/cache restart: correct queries |',
      '|---|---:|---:|---:|']
    for name in ['default','prepared','semantic-cache']:
        report=reports[name];body=summary(report,'body_edit');api=summary(report,'api_edit');restore=summary(report,'cache_restore_query')
        lines.append(f"| {labels[name]} | {body['correct']}/{body['checked']} | {api['correct']}/{api['checked']} | {restore['correct']}/{restore['checked']} |")
    lines += ['', 'Body/API edit phases request complete cross-project references after an unsaved edit. The API edit intentionally makes consumers ill-typed. A rapid unavailable reply does not satisfy either goal. The current semantic snapshot invalidation is conservative; preserving exact answers after ordinary edits remains a major unfinished requirement.','',
      '## Explicit preparation cost','']
    setup=reports['semantic-cache']['prepared_inputs']['setup']
    lines += [f"Observed explicit preparation took {setup['elapsed_ms']/1000:.2f} seconds, including the recorded fixture builds, MSBuild evaluation, all-reference library extraction and FCS occurrence snapshot export. Package/SDK caches were already warm. This cost is outside prepared Rust startup and is reported separately rather than hidden in the startup headline.", '',
      '| Preparation phase | Time ms | Peak subtree MiB |','|---|---:|---:|']
    for row in setup['phases']:
        lines.append(f"| {row['phase']} | {number(row.get('latency_ms',row.get('elapsed_ms')))} | {row.get('peak_tree_rss_kib',0)/1024:.2f} |")
    lines += ['', '## Pinned open-source corpus workload','',
      'These corpus traces are allocation/work profiles, **not validated semantic-navigation benchmarks**. They mix definition/reference requests in varied files. Timing an unavailable or unverified result establishes no navigation speedup. dotnet/fsharp is the largest by tracked physical F# lines among the ten repositories measured during research; no global-largest claim is made.','',
      '| Corpus | SHA | Tracked F# files | Engine source universe / indexed files | Open p95 ms | Raw-index attempt p50 s / status | Index MiB | Peak tree MiB |',
      '|---|---|---:|---:|---:|---|---:|---:|']
    for name in ['fsharp','grace','b2r2']:
        report=reports[name];stats=phase_rows(report,'final_stats')[0]['stats'];opened=summary(report,'cold_start');index=summary(report,'explicit_index_completion')
        index_rows=phase_rows(report,'explicit_index_completion')
        error_count=max(len(r.get('response',{}).get('result',{}).get('indexErrors',{})) for r in index_rows)
        status=f'partial; {error_count} file errors' if error_count else 'finished; 0 file errors'
        if any('error' in r.get('response',{}) for r in index_rows):status='request failed'
        lines.append(f"| {name} | `{report['workload']['sha']}` | {report['workload']['tracked_fsharp_files']} | {stats.get('sourceFileCount','?')} / {stats.get('indexFileCount','?')} | {number(opened['p95_ms_all_responses'])} | {index['p50_ms_all_responses']/1000:.3f}; {status} | {stats.get('indexBytes',0)/1048576:.2f} | {peak(report):.2f} |")
    lines.append('')
    for name in ['fsharp','grace','b2r2']:
        rows=phase_rows(reports[name],'warm_query')
        exact=sum(row.get('completeness')=='exact' for row in rows)
        unavailable=sum(row.get('completeness')=='unavailable' for row in rows)
        errors=sum('error' in row.get('response',{}) for row in rows)
        lines.append(f'- {name}: {exact}/{len(rows)} warm responses self-reported exact, {unavailable} unavailable, {errors} protocol errors. These classifications were not verified against a semantic oracle.')
        reasons=collections.Counter(row['response']['error'].get('message','unknown') for row in rows if 'error' in row.get('response',{}))
        for reason,count in reasons.most_common(3):
            lines.append(f'  - {count} warm errors: {reason[:220]}.')
        attempt=phase_rows(reports[name],'explicit_index_completion')[0]
        file_errors=collections.Counter(attempt.get('response',{}).get('result',{}).get('indexErrors',{}).values())
        for reason,count in file_errors.most_common(3):
            lines.append(f'  - First indexing attempt: {count} file errors of type `{reason[:160]}`.')
        warm=summary(reports[name],'warm_query')
        lines.append(f"  - Maximum reported warm-query reads: {warm['max_query_files_read']} files / {warm['max_query_bytes_read']} bytes; maximum parses: {warm['max_query_files_parsed']} files.")
    lines += ['', 'Final corpus runs use workspace-backed overlay storage for their temporary indexes, with capacity/path recorded in JSON. An earlier compiler-corpus attempt exhausted the `/tmp` tmpfs and reported SQLite disk-full errors; it indexed only 436/1774 known files. That environment-contaminated attempt is preserved in `evaluation/results/rejected-tmpfs-full/` and is not used as final performance or coverage evidence. Fixture default/profile temporary files were on tmpfs; prepared fixture caches were workspace-backed. No cross-storage speedup ratio is claimed. Process RSS excludes filesystem/page-cache memory, including tmpfs index storage.', '',
      'Tracked repository size and evaluated/discovered compile inputs are different universes. In particular, many compiler tests are source fixtures outside the native project model. The index can also contain explicitly opened sampled files outside that model, so its count can exceed the model universe. Index-attempt file errors and warm-query protocol errors are separate counts: an indexing request can finish with missing-file errors while later queries return ordinary responses. Any attempt with file errors is partial. Raw indexing builds identifier postings, not completed semantic analysis. The first lookup is measured before this explicit indexing step. Index bytes include the measured SQLite database and WAL.','',
      'Universal full indexing within a few seconds is not demonstrated: the compiler corpus and B2R2 require substantially longer in these runs. Low service startup time is a separate result and does not imply a complete index or a correct first arbitrary query.','',
      '## Failed experiment and bounded fallback','',
      'The initial eager/token-JSON implementation was rejected: a single 3.59 MB `ComparersRegression.fs` source produced about 861 MiB indexed-mode RSS peak and a 288 MiB database+WAL footprint; lexical mode peaked above 1 GiB. The initial whole compiler-corpus run also retained hundreds of MiB. Those raw observations remain in the evidence rather than being overwritten.','']
    largest=profile['rows'][0];query=largest.get('query_measurement',{})
    lines += [f"On the final executable, the same indexed singleton opened at {largest['peak_tree_rss_kib']/1024:.2f} MiB. Its demanded parse/query peaked at {query.get('peak_tree_rss_kib',0)/1024:.2f} MiB and took {number(query.get('latency_ms'))} ms. Result completeness was `{largest.get('query_response',{}).get('result',{}).get('completeness','error')}`. A parse-budget rejection protects resources but remains an unsupported query, not successful equivalence.", '',
      '## Validation limits and remaining goal','',
      '- TypeScript compiled; 15 Node transport/virtual-document tests and the Python evaluator guard tests passed. The actual VS Code host launch failed before tests because this environment denies AF_UNIX sockets required by D-Bus/VS Code IPC. Neither candidate native-host success nor an Ionide host baseline is claimed.',
      '- The real Ionide 8.1.0 VSIX was downloaded and an isolated provider-command adapter was implemented. It was not successfully run in this environment. Rider was not measured. Consequently all relative improvement ratios remain unavailable; the provisional 100x memory and 2x latency targets are unproven.',
      '- Nix configuration exists, but Nix execution was unavailable here. Local pinned toolchains were used. GUI-host behavior, installed-extension behavior and Nix builds still require a supported CI/development environment.',
      '- Default exact coverage, updates after edits, unsupported language constructs and complete references remain release blockers. Passing selected examples and bounding allocation do not establish full Ionide equivalence.','',
      '## Reproduction and raw evidence','',
      'Use `evaluation/run.py --engine-sha <the SHA above>` with the commands documented in `evaluation/README.md`. The runner refuses a different executable and reports incomplete answers separately. `evaluation/prepare.py` records explicit setup; `evaluation/profile_files.py` measures both lazy opening and the first demanded parse.','',
      'Raw files:','']
    lines += [f'- `evaluation/results/final-{name}.json` and `.md`.' for name in names]
    lines += ['- `evaluation/results/final-file-profile.json`.',
              '- `evaluation/results/initial-unpinned/`, `rejected-2ff71e5e/`, `rejected-e0a92fe0/` preserve failed/superseded candidates.',
              '- `evaluation/results/fcs-oracle-correctness-reviewed.json` and `fcs-accessibility-oracle.json` retain actual oracle checks.',
              '- `docs/review-evidence.json` retains independent adversarial review.',
              '- `test-results/host-environment/environment-status.json` and `vscode-launch.log` retain the actual host failure.',
              '- `evaluation/ionide-provenance.json` pins the downloaded Ionide artifact.','']
    text='\n'.join(lines)
    for path in [ROOT/'docs'/'evaluation.md',ROOT/'artifacts'/'Prototype_Evaluation.md']:
        path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
    print(json.dumps({'report':str(ROOT/'docs'/'evaluation.md'),'bytes':len(text.encode())}))


if __name__=='__main__':
    main()
