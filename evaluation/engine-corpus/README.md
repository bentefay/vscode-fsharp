# Portable source-only native corpus gate

`cli/bin/run test corpus` runs the native engine against supplied, pinned public
source trees. It never downloads source, restores packages, evaluates projects,
launches helpers, builds, or uses VS Code. Source acquisition is deliberately an
explicit prerequisite, separate from this gate.

Set `FSHARP_CORPUS_GRACE`, `FSHARP_CORPUS_FSCHECK`, and
`FSHARP_CORPUS_COMPILER` to repository roots at the revisions in `manifest.json`;
set `FSHARP_CORPUS_OUTPUT` to a private output JSON path. The CLI uses the existing
release binary unless `FSHARP_BACKEND_PATH` is supplied. Missing inputs and changed
source inventories are **blocked (exit 2)**, never skipped or passed. Failed
required navigation/lifecycle controls return **1**; only all selected required
checks passing returns **0**.

The runner also accepts `--binary`, `--grace-root`, `--fscheck-root`,
`--compiler-root`, `--output`, optional repeated `--suite grace|fscheck|compiler`,
and `--timeout` (default 30 seconds per readiness wait/request). Selecting a suite
explicitly reduces scope; the report lists the selected suites. Default is all.
Do not treat a subset result as the all-corpus gate.

## Frozen obligations

- Grace: source-local closed-lambda `range` must be exact; cross-file `serialize`
  must reach its known full-range target as exact or labelled incomplete choices.
- FsCheck: tuple-pattern `res` and recursive `loop` must remain exact; qualified
  `Common.takeWhilePlusLast` must reach its known target.
- Compiler: the closed array-pattern `part` must reach its known target. Current
  historical refusal is a failing required obligation, not a weakened oracle.

These six definitions reuse source-inspected/frozen historical oracles. They are
not a general compiler-correctness suite. Exact means the **full singleton URI and
range**, with no candidate mixing. Useful means exact, or empty exact locations
plus explicitly incomplete candidates containing the known full-range target.
Candidate recall does not establish precise binding, complete references, or UI
visibility. A refusal cannot satisfy a required useful result.

Every suite starts with a fresh cache, records the first response and a frozen nonlocal discovery-trigger query, waits for both
raw discovery and indexing completion, then checks its required cases. An eight
comment-line unsaved prefix shifts a selected target; its old location must be
absent from exact/full/likely outputs. Closing restores the original disk target
and excludes the shifted target. A second process reopens the same cache and
checks all required cases after readiness. This is layout-edit freshness, not a
semantic rename/rebind test. No manual index or evaluated model is supplied.
Source, project/import inputs and native binary are hashed before/after; source
files are not edited on disk. Cached results never substitute for current checks.

`known-failures.json` preserves eight additional compiler definition obligations
as separately reported probes. Their failure is **not a required pass**. Signature
contracts and corresponding implementation declarations remain distinct; an
implementation exact hit is reported separately from signature-policy success.
Wrong exact targets, unlabelled candidates and malformed tiers remain failures.
References and external-hover coverage are explicitly not gated by this subset.

All corpus content stays outside the repository. The tracked manifests contain
relative paths, hashes and literal target spans only; supply licensed upstream
source separately. The inventory pins all `.fs`, `.fsi`, project/solution and
MSBuild input files under the selected workspace plus enclosing props/targets and
configuration files. It is a byte-identity check, not an evaluated membership
certificate. Git metadata/build output directories are excluded as documented in
the manifest. Additions to the selected source inventory also block the gate.

Scorer/inventory unit tests in `evaluation/test_engine_corpus_gate.py` use no
engine, downloads or compilers. Runtime reports include raw requests/results and
are correctness evidence, **not performance benchmarks**.

The runner validates the fixed suite IDs and each required case's strength before
launch. It then independently audits every required raw answer: each site's
fresh-ready and restart-ready observation, plus the selected dirty and restored
observation, must occur exactly once and have a current acknowledged origin.
Readiness must precede these observations. Missing cases/phases, empty selections,
unscored answers, duplicate observations, and wrong targets fail even if other
cases or known-failure probes succeed. The final audit re-scores raw responses;
stored success flags and aggregate totals cannot hide a lost obligation. This
small raw-source gate is distinct from the historical prepared Grace 46-operation
suite and does not replace that suite's coverage claims.

Initial responses and explicit discovery-demand responses must also occur exactly
once per process, be scored, and satisfy currentness/safety checks; they may
remain unavailable. All eight known-failure probes must remain observed and
reported separately, even though their useful-target failures are not required
passes. Omitting their evidence cannot make a protocol-complete report.
