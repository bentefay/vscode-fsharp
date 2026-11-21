# F# Navigator prototype evaluation — historical 0.1.0

This report is frozen evidence for native `7ca9b1dd…`; its figures and lack of a
matched baseline describe that version. Later shipped-FSAC experiments and the
recovered development tree are recorded separately in `iteration-status.md`,
`recovery-status.md`, and `evaluation/results/`. Do not apply these numbers to
a newer executable.

**The full requested replacement and relative performance goals are not achieved.** The default Rust engine supports a limited exact subset. Explicitly prepared semantic snapshots cover more clean-source cases, but ordinary edits invalidate their answers. No matched Ionide/Rider performance baseline was obtained.

Frozen native executable SHA-256: `7ca9b1dd88412cb025ec6f999ff5dfffb874e3d5e1aba63860b996ec6f4da78f`. Each run executed an immutable copy, with executable, source, lockfile and prepared-input hashes retained in JSON.

## Correctness before performance

| Configuration | Clean fixture queries correct | Repeated warm replies correct | Wrong exact replies observed |
|---|---:|---:|---:|
| Default Rust, no compiler/catalog bootstrap | 17/29 | 255/435 | 0 |
| Rust + evaluated project model and library catalog | 20/29 | 300/435 | 0 |
| Rust + explicit FCS occurrence snapshot | 29/29 | 435/435 | 0 |

The fixture uses real compiled F# projects and exact occurrence-set checks. It covers parameters, shadowing, aliases, records, overloads, project order, linked files with different defines, `.fsi` boundaries, Unicode, escaped identifiers, library-only hover, `open`/`AutoOpen`, private accessibility and signature hiding. Qualified reference ranges have explicit alternatives matching observed FCS conventions; extra or missing occurrences still fail.

The FCS oracle was actually run for the original 27 cases and the two added accessibility cases. It invalidates compiler state before each query and is a correctness oracle, **not a warmed Ionide/FCS performance baseline**. Unsupported responses count as failures. Successful stale-answer guards do not count as fast navigation answers.

- Default Rust, no compiler/catalog bootstrap unsupported/failing cases: `cross_project_definition`, `all_increment_references`, `module_alias_definition`, `record_person_field`, `record_widget_field`, `record_person_references`, `overload_integer`, `overload_string`, `signature_definition`, `linked_first_context`, `linked_second_context`, `external_library_hover`.
- Rust + evaluated project model and library catalog unsupported/failing cases: `all_increment_references`, `module_alias_definition`, `record_person_field`, `record_widget_field`, `record_person_references`, `overload_integer`, `overload_string`, `linked_first_context`, `linked_second_context`.

## Startup, first lookup and retained memory

Five process-cold/cache-empty fixture repetitions; OS file caches were not flushed. RSS is the sum of the daemon and every observed descendant, sampled every 10 ms. This excludes the test controller and whole editor, counts shared pages more than once, and can miss short peaks.

| Configuration | Open p50 / p95 ms | Open + first local answer p50 / p95 ms | Peak measured tree MiB |
|---|---:|---:|---:|
| Default Rust, no compiler/catalog bootstrap | 3.45 / 4.13 | 6.06 / 6.97 | 11.76 |
| Rust + evaluated project model and library catalog | 7.80 / 9.42 | 20.57 / 36.07 | 23.31 |
| Rust + explicit FCS occurrence snapshot | 5.84 / 9.04 | 18.98 / 28.40 | 21.50 |

- Default Rust, no compiler/catalog bootstrap: zero instrumented reads/parses/compiler launches at workspace/open in 5/5 runs; zero reported AST work in 55/55 buffer-open notifications.
- Rust + evaluated project model and library catalog: zero instrumented reads/parses/compiler launches at workspace/open in 5/5 runs; zero reported AST work in 55/55 buffer-open notifications.
- Rust + explicit FCS occurrence snapshot: zero instrumented reads/parses/compiler launches at workspace/open in 5/5 runs; zero reported AST work in 55/55 buffer-open notifications.

With the real all-reference library catalog enabled, first external hover including process/open/import cost was p50 1311.14 ms, p95 1585.82 ms; subsequent exact hover p95 was 1.03 ms. Peak observed process-tree RSS was 20.66 MiB. The separate first-local-query run above does not need to import library declarations.

Reader counters require care: semantic snapshot validation hashes source **and assemblies/project artifacts**. A large `bytesRead` value in this mode is not all F# source text. Evaluated-model validation also hashes build/assembly inputs on the first query; these inputs are not all charged to the source-read counters. Default corpus runs have no imported semantic snapshot or catalog; their source-read counters are more directly interpretable. Wall-clock latency and subtree RSS include the actual validation/import work.

## Edits and cache restoration

| Configuration | Body edit: exact answers | API edit: exact answers | Process/cache restart: correct queries |
|---|---:|---:|---:|
| Default Rust, no compiler/catalog bootstrap | 0/5 | 0/5 | 85/145 |
| Rust + evaluated project model and library catalog | 0/5 | 0/5 | 100/145 |
| Rust + explicit FCS occurrence snapshot | 0/5 | 0/5 | 145/145 |

Body/API edit phases request complete cross-project references after an unsaved edit. The API edit intentionally makes consumers ill-typed. A rapid unavailable reply does not satisfy either goal. The current semantic snapshot invalidation is conservative; preserving exact answers after ordinary edits remains a major unfinished requirement.

## Explicit preparation cost

Observed explicit preparation took 26.62 seconds, including the recorded fixture builds, MSBuild evaluation, all-reference library extraction and FCS occurrence snapshot export. Package/SDK caches were already warm. This cost is outside prepared Rust startup and is reported separately rather than hidden in the startup headline.

| Preparation phase | Time ms | Peak subtree MiB |
|---|---:|---:|
| build | 5747.09 | 183.43 |
| build | 1309.76 | 157.48 |
| build | 3249.07 | 164.59 |
| evaluate_model | 4290.12 | 72.78 |
| library_catalog | 6593.51 | 325.71 |
| semantic_snapshot | 5188.15 | 384.80 |

## Pinned open-source corpus workload

These corpus traces are allocation/work profiles, **not validated semantic-navigation benchmarks**. They mix definition/reference requests in varied files. Timing an unavailable or unverified result establishes no navigation speedup. dotnet/fsharp is the largest by tracked physical F# lines among the ten repositories measured during research; no global-largest claim is made.

| Corpus | SHA | Tracked F# files | Engine source universe / indexed files | Open p95 ms | Raw-index attempt p50 s / status | Index MiB | Peak tree MiB |
|---|---|---:|---:|---:|---|---:|---:|
| fsharp | `f896797a0f9b238a8cb1c482be424b714037ddc5` | 6811 | 1774 / 1783 | 20.81 | 16.685; partial; 4 file errors | 292.33 | 30.41 |
| grace | `39960e07b73cd142cc0c1fd070c0591be91c1933` | 545 | 541 / 541 | 8.86 | 5.077; finished; 0 file errors | 101.25 | 84.00 |
| b2r2 | `5a9ba0e27d868ffb44831cf2ba98afb4be2d3904` | 1426 | 1425 / 1425 | 6.56 | 17.142; partial; 1 file errors | 280.87 | 51.16 |

- fsharp: 90/180 warm responses self-reported exact, 90 unavailable, 0 protocol errors. These classifications were not verified against a semantic oracle.
  - First indexing attempt: 4 file errors of type `No such file or directory (os error 2)`.
  - Maximum reported warm-query reads: 2 files / 51458 bytes; maximum parses: 2 files.
- grace: 92/216 warm responses self-reported exact, 117 unavailable, 7 protocol errors. These classifications were not verified against a semantic oracle.
  - 7 warm errors: Edited document could not be parsed within resource budget; retry or raise parser budget.
  - Maximum reported warm-query reads: 3 files / 824737 bytes; maximum parses: 4 files.
- b2r2: 90/216 warm responses self-reported exact, 126 unavailable, 0 protocol errors. These classifications were not verified against a semantic oracle.
  - First indexing attempt: 1 file errors of type `No such file or directory (os error 2)`.
  - Maximum reported warm-query reads: 73 files / 1718070 bytes; maximum parses: 74 files.

Final corpus runs use workspace-backed overlay storage for their temporary indexes, with capacity/path recorded in JSON. An earlier compiler-corpus attempt exhausted the `/tmp` tmpfs and reported SQLite disk-full errors; it indexed only 436/1774 known files. That environment-contaminated attempt is preserved in `evaluation/results/rejected-tmpfs-full/` and is not used as final performance or coverage evidence. Fixture default/profile temporary files were on tmpfs; prepared fixture caches were workspace-backed. No cross-storage speedup ratio is claimed. Process RSS excludes filesystem/page-cache memory, including tmpfs index storage.

Tracked repository size and evaluated/discovered compile inputs are different universes. In particular, many compiler tests are source fixtures outside the native project model. The index can also contain explicitly opened sampled files outside that model, so its count can exceed the model universe. Index-attempt file errors and warm-query protocol errors are separate counts: an indexing request can finish with missing-file errors while later queries return ordinary responses. Any attempt with file errors is partial. Raw indexing builds identifier postings, not completed semantic analysis. The first lookup is measured before this explicit indexing step. Index bytes include the measured SQLite database and WAL.

Universal full indexing within a few seconds is not demonstrated: the compiler corpus and B2R2 require substantially longer in these runs. Low service startup time is a separate result and does not imply a complete index or a correct first arbitrary query.

## Failed experiment and bounded fallback

The initial eager/token-JSON implementation was rejected: a single 3.59 MB `ComparersRegression.fs` source produced about 861 MiB indexed-mode RSS peak and a 288 MiB database+WAL footprint; lexical mode peaked above 1 GiB. The initial whole compiler-corpus run also retained hundreds of MiB. Those raw observations remain in the evidence rather than being overwritten.

On the final executable, the same indexed singleton opened at 4.96 MiB. Its demanded parse/query peaked at 68.63 MiB and took 287.14 ms. Result completeness was `unavailable`. A parse-budget rejection protects resources but remains an unsupported query, not successful equivalence.

## Validation limits and remaining goal

- TypeScript compiled; 15 Node transport/virtual-document tests and the Python evaluator guard tests passed. The actual VS Code host launch failed before tests because this environment denies AF_UNIX sockets required by D-Bus/VS Code IPC. Neither candidate native-host success nor an Ionide host baseline is claimed.
- The real Ionide 8.1.0 VSIX was downloaded and an isolated provider-command adapter was implemented. It was not successfully run in this environment. Rider was not measured. Consequently all relative improvement ratios remain unavailable; the provisional 100x memory and 2x latency targets are unproven.
- Nix configuration exists, but Nix execution was unavailable here. Local pinned toolchains were used. GUI-host behavior, installed-extension behavior and Nix builds still require a supported CI/development environment.
- Default exact coverage, updates after edits, unsupported language constructs and complete references remain release blockers. Passing selected examples and bounding allocation do not establish full Ionide equivalence.

## Reproduction and raw evidence

Use `evaluation/run.py --engine-sha <the SHA above>` with the commands documented in `evaluation/README.md`. The runner refuses a different executable and reports incomplete answers separately. `evaluation/prepare.py` records explicit setup; `evaluation/profile_files.py` measures both lazy opening and the first demanded parse.

Raw files:

- `evaluation/results/final-default.json` and `.md`.
- `evaluation/results/final-prepared.json` and `.md`.
- `evaluation/results/final-semantic-cache.json` and `.md`.
- `evaluation/results/final-library-hover.json` and `.md`.
- `evaluation/results/final-fsharp.json` and `.md`.
- `evaluation/results/final-grace.json` and `.md`.
- `evaluation/results/final-b2r2.json` and `.md`.
- `evaluation/results/final-file-profile.json`.
- `evaluation/results/initial-unpinned/`, `rejected-2ff71e5e/`, `rejected-e0a92fe0/` preserve failed/superseded candidates.
- `evaluation/results/fcs-oracle-correctness-reviewed.json` and `fcs-accessibility-oracle.json` retain actual oracle checks.
- `docs/review-evidence.json` retains independent adversarial review.
- `test-results/host-environment/environment-status.json` and `vscode-launch.log` retain the actual host failure.
- `evaluation/ionide-provenance.json` pins the downloaded Ionide artifact.
