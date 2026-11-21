> Recovery note: this workspace was restored from an earlier source checkpoint. Historical observations below are not fresh rerun results unless their raw JSON exists. New `recovery-*` reports explicitly record actual post-reset runs. The fixed Grace sample is reproduced by `select_grace_samples.py`; 30 selected candidates contain 23 source sites (46 definition/reference operations), six external sites and one unmatched syntactic candidate. The canonical current goldens are `results/recovery-grace-goldens46-corrected.json`, with a documented direct-FCS override for the Sha256Hash alias reference group.

# Evaluation

The standard-library Python controller measures the Rust daemon and all of its
children. It has no third-party runtime dependencies. This is measurement tooling;
the primary language engine remains Rust. It never enables the .NET oracle unless
`--modes semantic` and `--helper` explicitly request it.

```sh
python3 -m unittest discover -s evaluation -p 'test_*.py'
python3 evaluation/run.py --engine 'target/release/fsharp-navigator daemon' --modes lexical,indexed --repetitions 5 --warm-repetitions 5 --output evaluation/results/fixtures.json
python3 evaluation/run.py --engine 'target/release/fsharp-navigator daemon' --corpus ../research/corpus/fsharp --repetitions 5 --query-count 50 --build-index --output evaluation/results/fsharp.json
```

Use the same command with `../research/corpus/Grace` and `../research/corpus/B2R2`.
The corpus must be a Git checkout. The report records its actual HEAD, tracked F#
file count and bytes. Compiler fixtures/generated code are part of these raw
counts; the research corpus note explains production-code distinctions. No claim
that dotnet/fsharp is globally largest follows from this benchmark.

The fixture runner strips named source markers into a temporary workspace, obtains
zero-based UTF-16 positions, and checks exact definition/reference sets. Extra,
missing, duplicate and approximate results all fail. Hover tests check library
signature visibility and workspace-symbol suppression. The fixture includes three
project contexts, linked files with different defines, lexical shadowing, records,
overloads, aliases, real signatures and explicit nonalphabetical compile order.
One signature query explicitly accepts either the `.fsi` or implementation anchor;
lock that choice to the chosen Ionide oracle before claiming equivalence.

Phases cover process-cold/cache-empty opening, first query, all correctness cases,
varied-file warm queries, dirty body/API/rename edits, and process/cache restoration.
The API fixture edit deliberately makes consumers ill-typed; parity on this case
requires observing the oracle's behavior. Corpus queries sample declarations and
are **unvalidated** until a semantic baseline is available. Their response times
measure work performed, not successful exact code navigation.

Each JSON report retains raw answers, validation failures, per-query cumulative
counter differences, latency, immediate post-response RSS and sampled process-tree
peak RSS. Sampling can miss brief peaks. Summed RSS counts shared pages more than
once; it is not PSS, and excludes the measurement controller. OS caches are not
flushed, so “cold” means process-cold and application-cache-empty. Whole-editor
memory is a distinct metric that this daemon harness does not measure.

`--baseline path.json` compares identical workload fingerprints only. Both sides
must return complete correct answers in a phase; failed/unvalidated answers never
qualify. Reports show p95 latency and peak-memory ratios against separately
configurable provisional goals:100x less memory and2x faster latency. These are
targets, not measured achievements. A missing baseline gives null ratios. The engine command can target an
adapter implementing `docs/protocol.md`, allowing a real FSAC/Ionide baseline on
the same trace. No baseline numbers are inferred from the user's observations.

Restore/build costs must be reported separately and also added to an end-to-end
cold run when they are needed to obtain the first correct answer. The fixture
runner does not silently restore projects or run compiler workers. A later IDE
baseline needs both the actual user configuration and a comparable minimal
navigation configuration; neither should replace the other in reported results.

`--fail-on-error` makes checked correctness failures fail automation. It is omitted
by default because the initial research prototype deliberately records unsupported
cases rather than claiming full coverage.

Every new Rust run copies the executable into an immutable run directory and
records SHA-256, source hashes before/after, available Git state, helper DLL hashes
and dependency-lock hashes. `results/initial-unpinned/` deliberately preserves
superseded exploratory runs that predate this protection. Do not assign those
older observations an invented source revision.

`prepare.py` explicitly measures optional project restore/build, MSBuild evaluation,
binary-library extraction and (only with `--semantic-snapshot`) full source semantic
index export. Pass its `model.json`, `catalog.json` and `setup.json` with
`--evaluated-model`, `--library-catalog` and `--setup-report`. Full semantic cache
experiments additionally pass `--semantic-snapshot`; this is distinct from the
default selective Rust engine. Bootstrap costs remain embedded in the report.
The original setup is not repeatedly charged as if it had been rerun on every
warm query, and prepared startup is not mislabeled cold end-to-end startup.

`oracle_adapter.py` runs the actual helper for correctness checks; it invalidates
compiler state before every query. Reports identify this configuration and the
comparison code refuses to treat it as an Ionide or warmed FCS performance baseline.
Use `--correctness-only` to avoid unnecessary timing phases for this oracle.

The expanded fixture manifest contains100 clean queries: the original29 (also
preserved in `fixtures/expectations.original29.json`) and71 independent
record/property cases in `fixtures/expectations.common-fields.json`. The latter
contain58 definitions and13 complete reference sets. They cover receiver
annotations, record construction/update, alias and return flows, duplicated field
names, nested and generic records, patterns, automatic/static/inherited
properties, and cross-project consumers. All71 were checked directly with the
actual FCS helper; `results/fields-oracle-initial.json` preserves the answers and
frozen helper hash. This is a correctness result, not a performance baseline.

Four additional valid-code edit traces change a receiver annotation, change a
local alias target, rename a mutable record field while adding a new reference,
and rename a field across both project overlays. The harness recalculates marked
UTF-16 positions after edits. Reference checks require the full changed occurrence
set; merely returning the old declaration at the same position is insufficient.
All12 edit checks passed the direct FCS oracle; raw results are retained in
`results/fields-oracle-edits.json`.
For explicitly marked reference occurrences, either a member-token range or the
FCS full-receiver range is allowed, with exactly one result per occurrence.

```sh
python3 evaluation/run.py --engine 'target/release/fsharp-navigator daemon' --manifest fixtures/expectations.common-fields.json --correctness-only --repetitions 1 --output evaluation/results/fields-candidate.json
python3 evaluation/run.py --engine 'target/release/fsharp-navigator daemon' --manifest fixtures/expectations.common-fields.json --edits-only --repetitions 1 --output evaluation/results/fields-edits.json
```

`--first-query field_explicit_customer` selects the operation measured before any
other navigation request. `--edit-names receiver_type_edit,receiver_alias_edit`
selects dirty traces. `--only` selects clean query specifications and limits edit
checks to those specifications. Use separate clean and edit results when running
an expensive direct FCS oracle; the primary Rust engine never launches it.

`profile_files.py` isolates the largest real source files into separate processes,
records opening and a subsequent parse-triggering query, and stores10ms RSS traces.
This caught a3.6MB compiler regression source expanding into hundreds of MB of
index data in the initial implementation. Its query may be unsupported: this is
an allocation/work profile, not evidence of semantic navigation completeness.

## Real Ionide baseline

`ionide_adapter.py` launches an actual isolated VS Code extension host with an
installed Ionide extension and bridges the same protocol to VS Code's definition,
reference and hover provider commands. Install an official Ionide VSIX into the
chosen isolated extension directory first; record its version and SHA-256. Example:

```sh
python3 evaluation/run.py --engine 'python3 evaluation/ionide_adapter.py --vscode .tools/vscode/1.145.3/VSCode-linux-x64/code --extensions evaluation/.runtime/extensions --dotnet-root .tools/dotnet' --modes semantic --memory-scope whole-ide --timeout 120 --repetitions 1 --warm-repetitions 1 --output evaluation/results/ionide.json
```

Run under the same Xvfb/display environment as the extension tests. Fixture project
restore must finish first. The baseline activates the real extension and waits for
a real definition response; it does not treat extension activation as readiness.
Because VS Code's reference-provider command has no includeDeclaration argument,
the adapter subtracts definition locations. Workspace hover remains Ionide's raw
behavior, so it may intentionally differ from this extension's restricted hover
policy. Its memory scope is **whole editor plus language service**; the comparison
logic refuses to turn that number into a daemon-only memory improvement ratio.

The runner aborts a measured tree exceeding4GiB sampled RSS by default, reports
the abort as an error and keeps the result. Override with `--memory-limit-mib` only
when the available machine budget supports it. This is a sampled guard, not a hard
kernel-enforced memory limit.

## Reconstructed semantic contracts

`fixtures/signature-boundaries` contains 13 paired `.fsi`/`.fs` cases. Actual shipped FSAC routes ordinary uses and implementation declarations to the signature, but a cursor on the signature declaration toggles to the implementation. Reference sets include both declarations. The fresh rerun is `results/recovery-signature-fsac-confirmed.json`.

`--fixture-source` preserves separate source universes. Reference queries at a use may provide a `declaration` marker for declaration-inclusive comparison. Dirty scenarios reset buffers through monotonically versioned text changes; restarted sessions reopen requested buffers before querying. Both engines receive the same lifecycle.

Process attribution walks `/proc` parent identities, guards PIDs with kernel start times, retains observed descendants after reparenting, and removes dead/reused identities. The real child/grandchild and reparenting regressions run in `test_harness.py`. Setup commands use the same attribution plus timeout/RSS guards and owned-process cleanup. Summed RSS excludes controller and filesystem/page-cache memory, including temporary-index file storage.
