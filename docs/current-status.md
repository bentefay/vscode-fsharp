# Current implementation and validation

Ordinary immutable expression-local `let` bindings now have a bounded References-only
proof; see [scope, fail-first gates and native qualification](local-let-references.md).
Definition and the private preview package are unchanged by this checkpoint.

Updated 22 November 2026. The Rust engine answers both fixed synthetic reference
queries missed by the imported prototype. On the Grace project, many record
labels and complete cross-file reference sets still require explicit compiler
preparation. The default indexed providers expose only answers for which the
engine can establish a binding and the requested reference domain.

The [singleton-array Definition route](singleton-array-definition.md) reuses the
existing complete arm-binding certificate for a proven same-document jump. It
does not broaden conditional, tuple or project-context proofs.

The [post-conditional candidate repair](post-conditional-candidates.md) reuses an
existing validated physical parse to recover the compiler's frozen tuple binder
after `#endif`. This is incomplete candidate recall, not exact binding or active
configuration proof. Conditional-branch binders remain a named unresolved test;
the original stronger oracle is retained. The headless gate now preserves seven
required source sites, with separate lifecycle checks for both compiler targets.

The [readiness candidate repair](readiness-candidate-repair.md) preserves current
same-document incomplete Definition candidates when broader contextual work hits
its foreground limit after indexing. A deterministic red/green regression and
pinned compiler-source replay establish the repair without a GUI. The new
[engine corpus gate](../evaluation/engine-corpus/README.md) makes six authored
Grace/FsCheck/compiler definitions and their lifecycle checks repeatable; named
compiler gaps remain separate. This is not a prepared project, broader exact
coverage, speed comparison or updated packaged-editor acceptance claim.

The [lambda-trivia repair](lambda-trivia-recall.md) restores an omitted tuple
binder in the held-out FsCheck workload. Both frozen eight-site runs improve
from one exact, six choices and one miss to two exact and six choices: only the
tuple changes, through an independently reviewed existing lexical proof. All
34 required editor scenarios, the scoped signature XFAIL and six Grace
clean/dirty/reverted controls were retained. Tuple picker selection was not
tested; the actual F12 result was a correct direct jump. No speed claim follows.

The [document-only availability qualification](foreign-buffer-membership.md#bounded-document-only-availability-rescue)
restores proven local parameter Definition jumps when an unrelated dirty external
buffer would otherwise trigger the membership refusal. Unsupported/cross-file
queries retain that guard. All 34 ordinary editor scenarios plus the signature
XFAIL passed again; six pinned Grace clean/dirty/reverted controls also passed.
This is an availability fix, not a measured speed improvement or broader exact
coverage claim. Normal-sandbox platform acceptance remains pending.

The [22 November pinned Navigator A/A measurement](performance-aa.md) records
actual keyboard-to-choice/jump timing, unsaved-edit freshness and whole-editor
PSS on unprepared Grace and a scoped FsToolkit holdout. It is a separate
measurement-only checkpoint, not an updated coverage score or competitor claim.
The [demand-switch follow-up](demand-switch-diagnostic.md) stopped at its causal
observation gate without running a new timing cohort or changing runtime.
The [subsequent opt-in observation](demand-switch-observation.md) passed the
ordinary editor gate and recorded one stopped switch/control pair. It did not
justify a scheduling change or a performance-improvement claim.
The [held-state PSS qualification](held-state-pss.md) stopped before measurement:
Solution-view initialization did not request background indexing. The +8 MiB
precision guard remains unverified; runtime was unchanged.
The [References currentness qualification](e2e-coverage.md#references-currentness-qualification)
retained the 24 ordinary passing contracts but added no qualified pass: a native
sidebar click opened an old range, and an isolated Possible References success
did not repeat in the full suite. Both strict tests remain separate regressions.
The [bounded row/click diagnostic](e2e-coverage.md#bounded-rendered-row-and-click-witness)
confirmed delivery of the native stale-row click and separately observed a
Possible References pointer sequence crossing moving rows. Native command
publication responsibility remains unresolved; runtime is unchanged.
The [Possible References pointer qualification](e2e-coverage.md#possible-references-pointer-qualification)
subsequently passed four fixed isolated/full-order runs with fresh stable-row
targeting and trusted same-row click delivery. Only that interaction is now
required, bringing the default gate to 25 passes plus the signature known
failure; native References currentness remains open.
A [single native References isolation pair](native-references-isolation.md)
then reproduced the stale activation with a public-API control provider and
Navigator absent. The exact workbench publication boundary remains unproven;
no product workaround or runtime fix was made.
The [three-family F12 slice](e2e-coverage.md#active-pattern-operator-and-generic-record-slice)
qualifies generic record substitution in the packaged editor, bringing the
default gate to 26 required scenarios plus the signature known failure.
Active-pattern distinction and custom-operator span remain strict failing
qualifications, with no refusal pass credit or runtime change.
The subsequent [scoped additive operator proof](e2e-coverage.md#scoped-additive-custom-operator-definition-proof)
qualifies the unchanged operator case and a later-open refusal control in a fresh
private package: 28 required scenarios plus the preserved signature failure.
General operators and active patterns remain outside this new proof. One
aggregate References assertion failed, then passed focused and full replays;
its cause remains unproven and no References behavior was changed.
The [scoped single-partial active-pattern proof](e2e-coverage.md#scoped-single-partial-active-pattern-definition-proof)
subsequently qualifies the unchanged recognizer-header/collision case and a
later-open labelled-choice control: 30 required scenarios plus the signature
XFAIL passed in one fresh packaged run. This is Definition-only, with bounded
syntax/scope evidence and conservative fallback; general active-pattern
resolution and References are not expanded.
The [raw target certainty checkpoint](e2e-coverage.md#raw-target-containing-project-certainty)
now withholds project-dependent exact answers for target-containing raw projects
while retaining their discovered files as incomplete candidates. A target-only
signature counterexample no longer returns an exact contract location; the
fresh editor gate passed 31 required cases plus the unchanged signature XFAIL.
This does not evaluate targets or fix signature visibility.

## Fixed-corpus results

These are fresh Linux native measurements in indexed mode. Both executables
used the same genuine project model, external metadata, source and queries.
Each unsupported operation stays in its original denominator.

| Fixed checks | Rebuilt import | Integrated candidate |
| --- | ---: | ---: |
| Synthetic definitions | 79/79 | 79/79 |
| Synthetic complete references | 16/18 | 18/18 |
| Synthetic external hover and workspace suppression | 3/3 | 3/3 |
| **Synthetic total** | **98/100** | **100/100** |
| Grace definitions | 10/23 | 12/23 |
| Grace complete references | 4/23 | 5/23 |
| **Grace total** | **14/46** | **17/46** |

All 17 returned Grace URI/range sets matched literally. The 29 unavailable
operations include record creation and update labels, projections and
cross-file uses whose inferred receiver or reference domain Rust cannot
establish. The three gains are the definition of `Kty`, its complete two-location
reference set, and the definition of the opened `serialize` function.
Both binaries also passed all 15 original checks across seven edits, including
the expected refusal after a rename. The candidate used zero compiler,
semantic-helper and decompiler launches throughout these suites.

A separate 831-operation SDK/FCS run added ambiguous argument roles and full
member ranges. On exactly these controls the imported binary gave 357 correct
answers, 376 wrong exact answers and 98 unavailable. The candidate gave 328
correct, zero wrong exact and 503 unavailable. It passed every mandatory
positive in the driver, including the prepared named-parameter and dirty
two-caller cases. Unprepared qualified equality remains at 2/9. These controls
include adversarial cases and do not estimate correctness rates in ordinary
projects. [Compiler control driver](../evaluation/navigation_role_controls.py)

## What the providers use

- **Local navigation.** On an acknowledged, explicitly versioned document,
  Rust can prove an application-lambda or nested-function parameter and its
  uses before project discovery. The first request parses the buffer. Repeat
  requests reuse a positive answer from a 32-entry cache with 128 KiB of
  accounted capacity. It retains no source or AST. Keys include the buffer
  version, source hash, URI, operation, cursor, declaration flag and engine
  identity. Edits, close/reopen, mode changes and cancellation invalidate
  entries. A definition can also populate the inclusive references key when
  its traversal proves a single use at the cursor. An indexed cache hit still
  passes the usual filesystem and context
  checks. [Protocol](protocol.md)
- **Signatures and arguments.** The handover patch prevented a mixed-case
  signature suffix from being overlooked. SDK and compiler controls also found
  signature inputs hidden by compiler response files and script loads. Rust
  declines an uncertified source domain. For ordinary parameters in a uniquely
  evaluated project it can use the compile order before the first signature.
  The argument resolver distinguishes a callee's named label from caller-side
  equality. In the supported single-project case, a static parameter's
  references include its declaration, method-body uses and caller-side labels.
  Rust checks every use of the method name and excludes additional callers
  using current source postings before claiming completeness. [Compiler controls](../evaluation/navigation_role_controls.py)
- **Project bindings.** Module abbreviations are resolved at the `open`
  directive that captures them. An opened constructor or an implicitly opened
  module can change which call head wins. A signed public module can have
  both a signature and an implementation. In record factories, the engine can
  resolve labels when a single static factory builds the complete record from
  primitive literals or identified method parameters. General record updates
  whose receiver type depends on match branches or other calls remain a gap.
- **Foreground work.** A failed document proof can consult the existing native
  index. Cold provider work has per-request limits of 512 discovery entries,
  128 reads, 4 MiB of source and 64 parses. Certificate validation has a
  separate budget for newly read bytes and filesystem visits. Exceeding a
  limit returns an unavailable answer. **F# Navigator: Index Workspace** is
  the explicit action for preparing broader postings. Indexed navigation
  never starts FCS. The budgets bound units of work and cache retention.
- **Hover.** Framework types and members, and selected root module values
  from referenced F# assemblies, can show signatures and XML documentation.
  A catalog hit must agree with the compiler-selected reference identity.
  Workspace declarations, aliases and uncertain owners suppress library
  hover. A consumer with workspace DLL references can still lack enough
  information to establish an external module's identity.
- **Explorer.** Classic solutions and XML solutions retain solution folders,
  project instances and solution items. Virtual folders follow compile order,
  including repeated sibling runs. A linked file opens its physical path in
  the selected project. Native XML previews identify their order as provisional.
  Evaluated MSBuild output supplies the authoritative compile list. Opening
  a project with no F# document leaves the extension inactive until a user
  opens a document or requests the view. Programmatic definition, reference and
  hover commands can register providers on first demand. A non-F# query can
  register the adapter while leaving the native service and file watchers idle.

## Native lifecycle

The local workload queried a callback parameter in the 26 KiB
`ContentBlockMetadata.Types.fs` buffer, using the real 39-source Grace.Types
project. Six baseline/candidate pairs alternated order after the build and
correctness jobs finished. Each cold process had an empty SQLite database.
The restarted process reused its database and received the edited buffer.
All rows below contain correct answers from every trial.

| Median wall time or sampled RSS | Rebuilt import | Integrated candidate | Operations per engine |
| --- | ---: | ---: | ---: |
| Process launch to first local definition | 197.90 ms | 13.95 ms | 6 |
| Warm local definition | 0.437 ms | 0.084 ms | 72 |
| First local references after definition | 1.194 ms | 0.063 ms | 6 |
| Warm local references | 0.453 ms | 0.069 ms | 72 |
| Unsaved edit to current definition | 17.56 ms | 8.37 ms | 6 |
| Process restart to current definition | 138.13 ms | 11.71 ms | 6 |
| Native process-tree RSS after first definition | 20.05 MiB | 14.81 MiB | 6 |
| Cold project-level definition request | 144.94 ms | 139.56 ms | 6 |
| Warm project-level definition request | 0.333 ms | 0.359 ms | 72 |

In the first integrated version, a reference query after a definition still
reparsed this document. A separate 12-pair crossover against that version
measured 6.193 ms before the cache change and 0.069 ms afterward for the first
inclusive references. Both binaries returned all 492 answers correctly. Each
single-use entry adds a 28-byte serialized payload and its key within the
existing cap. Independent hot/cold checks matched FCS on all 86 reference sets,
including UTF-16 cursor positions, dirty buffers and process-local cache state.

The project-level control queried `referenceTypeName` using a fresh
database in each trial. Its warm indexed answer skips a repeated attempt at a
document proof. RSS here is a point-in-time sample of the native process tree.
The editor has a separate process tree and a different startup measurement.
[Native workload and scoring](../evaluation/experiments/local-lexical-proof/local_lifecycle.py)

## Actual VS Code

The updated Linux x64 VSIX passed all 14 trusted cases and the installed
Restricted Mode case in each of VS Code 1.108.0 and 1.145.3, in disposable
profiles. First-demand controls exercised F12, Shift+F12 and Show Hover.
On actual VS Code 1.108.0 the rebuilt import scored 14/46 on Grace, the
on-demand candidate scored 17/46, and Ionide 8.1.0 scored 46/46. Each
configuration returned the same counts after an editor restart. Supplying an
offline snapshot path alone gave 17/46. Issuing **F# Navigator: Index
Workspace** before navigation gave 46/46, with 2.81 seconds for the initial
desktop index/import command and 1.07 seconds on restart.

A separate two-line fixture measured desktop launch to first correct result,
warm queries, unsaved edits and restart. Two fresh and two restarted profiles
per engine ran in alternating order. For the updated extension the median
fresh desktop launch to first correct definition was 3.50 s and the sampled
peak desktop-tree RSS was 1,267 MiB. Ionide measured 8.54 s and 1,627 MiB.
Warm definition medians on fresh profiles were 13.4 ms and 5.4 ms respectively.
The imported Navigator measured 4.43 s and 1,237 MiB on the same fresh-profile
trial. Both Navigator builds gave current answers immediately in all four
launches each. Two of four Ionide launches first returned a stale dirty answer
and recovered within 183 ms. General navigation parity, negligible end-to-end
startup and orders-of-magnitude improvements remain open goals. The [desktop report](editor-host-status.md)
gives the matching versions, full lifecycle, sample counts and limitations.

## Optional offline compiler snapshot

With an explicitly exported snapshot and native import, the same frozen daemon
returned 46/46 on clean Grace sources, with 23 literal definition matches and
23 literal complete reference sets. Every answer came from the persisted
snapshot, with zero compiler or helper launches during navigation. An unsaved
edit invalidated the snapshot and the bounded indexed lookup declined the
query. Explicit reimport refused the dirty buffer.

Producing the model, libraries, environment and 16,530,121-byte snapshot again
on this host took 167.59 seconds. The semantic export phase took 46.71 seconds
and peaked at 558.65 MiB of sampled helper-tree RSS. Preparing the environment
had the highest peak, 587.23 MiB. The explicit native index and import took
1.84 seconds and sampled 131.21 MiB. Other verification jobs were running during
preparation. The fixed default-navigation and editor lifecycle timings exclude
this optional work. The [replay instructions](../evaluation/contracts/README.md)
document both configurations.

## Reproduction and provenance

The measured native source is `1f375b27e2bb22605b4eee187667a7f15f14a0e9`,
with SHA-256 `b693cc9a1e03bd8d01c3c02aac468be113cfbc2cb2acc807039ce8d7ba46ee5e`
for the executable. The [native input manifest](../evaluation/candidates/source-80d1c4264cd6.json)
covers all 69 build inputs, including restored grammar bytes.

Baseline source was main at `e8fb6a93dd68da38b3b952777ae7f1776be437d3`.
The 65 native inputs matched the archived source manifest and a fresh release
build produced the original executable SHA-256
`710f046e290da02e8e85362add92125334f5ec7e1bb498f1201fc25eb6905fed`.
The original recovery checkpoint and its historical measurements remain
available in the [import report](https://github.com/bentefay/vscode-fsharp/blob/e8fb6a93dd68da38b3b952777ae7f1776be437d3/docs/current-status.md).

The reruns used Linux x64, glibc 2.41, Rust 1.100.0, Node 24.22.3, npm 11.19.1,
Safe Chain 1.5.29 and .NET SDK 10.0.400 / runtime 10.0.11. Use the protected
CLI entry points in the [toolchain handover](handoff-toolchain.md).
The actual desktop runs used disposable profiles. The full Nix workflow,
Windows and macOS remain untested for this revision.

The synthetic expectations and Grace queries are unchanged. Grace source
is pinned at `39960e07b73cd142cc0c1fd070c0591be91c1933`.
The native fixed-corpus runs validate the prepared project model and
selected external metadata. Offline compiler occurrence snapshots are a
separate configuration. The [Grace contract](../evaluation/contracts/grace46.json)
and [synthetic expectations](../fixtures/expectations.json) retain their
original query sites and expected locations. The fixed Grace scorer permits a
qualified reference to select its terminal identifier. The independent FCS
controls also compare the full source range, including qualification, for
method, property and record uses.

## Native checks and independent compiler controls

The fresh baseline CLI check passed 232 Rust core tests, three daemon tests,
19 parser bootstrap checks and 23 adapter/transport checks. The final CLI check
passed 281 Rust core tests, three daemon tests, 19 bootstrap and 24
adapter/transport checks. All 19 tests against the real native process and an
editor API double passed. The separate native protocol replay passed with
inaccessible model and catalog paths, CRLF and UTF-16 positions, an unsaved
rename, stale version, close/reopen and process restart. The default Rust
suite skips five tests that require separately generated artifacts. Four also
passed in dedicated runs with the real metadata producer, XML resolver and
locked decompiler. The fifth needs an original schema-1 inventory and its
matching live inputs, which the available archives do not contain. The
protected npm audit reported zero vulnerabilities.

Independent FCS controls compare definition spans and complete reference sets
with declaration inclusion both enabled and disabled. They exercise ordinary
equality, optional and named arguments, same-named modules and types, opened
constructors, dependent projects, stale versions, unsaved edits and persisted
caches. In the prepared static method example, FCS includes the formal
declaration, the method-body use and the named call label. For the caller's
parameter it includes the declaration and the right-hand operand. Ambiguous
overloads or additional unresolved callers keep the reference set unavailable.

The [machine-readable result](../evaluation/candidates/candidate-b693cc9a1e03.json)
identifies the executable, build inputs, fixed contract outputs and matched
native lifecycle measurements. The Linux x64 VSIX built from `1f375b2` has
SHA-256 `a8970dfaed1f327a5e15541aacc139c59ac82fc27c6efed14f9b148f16339c05`.
