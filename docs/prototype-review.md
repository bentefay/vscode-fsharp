# Independent prototype review

**Historical review of the original 0.1.0 binary only.** This document is not
approval of the current development tree. Fresh recovered candidates have
additional known wrong-exact blockers and remain rejected. See
[the current independent review ledger](recovery-review-ledger.md) and
`docs/review/` for the latest immutable-binary evidence. Historical limitations
below, including the old cancellation implementation, must not be mistaken for
measurements of later code.

**Verdict: accept as a scoped experimental prototype; do not accept it as an
Ionide replacement or a verified orders-of-magnitude improvement.** No
false-exact result remained in the 18 independent scenarios replayed against the
final frozen binary:

`7ca9b1dd88412cb025ec6f999ff5dfffb874e3d5e1aba63860b996ec6f4da78f`

The final replay passed all 12 focused scenarios and six additional visibility,
freshness and lazy-catalog checks. The catalog-cache and stale evaluated-model
paths now reject invalidated evidence. Earlier rejected builds and their defects
remain in `docs/review-evidence.json`, explicitly separated from final evidence.
A finite regression set does not prove correctness for arbitrary F# code.

## Acceptance gates

These are separate gates, not a weighted average that can hide a failure.

| Gate | Assessment |
|---|---|
| Correctness of the tested native subset | Pass for the scoped replay: 18/18 expected outcomes, including honest budget failure; zero false-exact findings. This is not full-language certification. |
| Useful native coverage | Partial. The engine resolves real lexical/module bindings; it does not achieve the requested navigation equivalence for all F# syntax and type-dependent members. Unavailable answers are missing coverage. |
| On-demand native startup | Verified in the focused replay: zero source-body reads and zero AST parses at `workspace/open`; three buffer opens still perform zero AST parses; the first local lookup parses one target. This measures daemon startup, not VS Code activation. |
| Bounded runtime design | Improved and inspectable, but not a hard native RSS guarantee. SQLite page cache, dirty-text limits and parser guards cover different allocations. No implicit compiler fallback exists in normal indexed operations. |
| Complete feature parity | Not met. Typed native navigation, contextual library binding, external implementation views, and complete ordered solution-explorer behavior still have gaps or unexecuted parity tests. |
| Matched performance improvement | Unproven. There is no successful matched Ionide/Rider baseline for complete operations. A proposed 100× target is a gate, not an achieved result. |
| Actual editor behavior | Unproven in this environment: isolated source/VSIX host tests are blocked by AF_UNIX restrictions. Fifteen frontend transport/queue/virtual-document tests and compilation are useful but do not substitute for a real editor run. |
| Reproducibility | Source, CLI, locks, Nix configuration, test harness and VSIX packaging exist. Final binary/package/evaluation hashes must agree. Nix and platform execution cannot be inferred merely from configuration files. |

## What was independently exercised

The reviewer ran an immutable copy of the real daemon against disposable source
projects and inspected returned targets and complete reference sets. Tests used
valid F# examples; F# Interactive independently confirmed disputed binding
semantics. Raw requests, responses, counters and binary hashes are retained in
`docs/review-evidence.json`.

The focused set covers explicit `open` shadowing, `AutoOpen`, isolated inline
function branches, local parameter binding, conservative candidate recall across
backticks in comments, combining-mark identifiers, deferred parsing of opened
buffers, a changed reference file while the daemon was stopped, parse-budget
failure in a candidate, successful retry with a higher budget, source-owned hover
suppression, and a genuine external-library hover.

The additional boundary cases cover a public value competing with a private
value from a later-opened module, a value hidden by an `.fsi`, and an inaccessible
qualified private value. The first must resolve to the public value; the latter
two must not be advertised as proven external bindings. The replay also exercises
old-cache invalidation when changing the engine version.

Earlier spot checks found correct behavior for lambda capture, loop binders,
exception binders, mutually recursive functions, quotations, nested modules,
constructor parameters, class-local values, multiline match branches, object
expression capture, `let ... in`, tuple patterns and while-block scopes. Protocol
checks rejected stale versions and conflicting equal-version updates, accepted
identical retries, used changed declaration ranges, and restored disk text when
an overlay closed. These earlier checks are supporting evidence, not an
exhaustive language proof or additional final-build test counts.

Opening buffers is not literally zero work: received text is hashed and raw name
postings are written. The verified guarantee is no eager AST construction or
repository-wide source scan during activation. Project discovery is deferred to
the first tree/navigation request. First-query latency must therefore be reported
alongside daemon-open latency; neither can stand in for editor activation.

## Corrections made during review

The first implementation incorrectly labelled later-open shadowing, auto-opened
bindings, inline pattern-branch leakage, and source/library type shadowing as
exact. Targeted reproductions drove fixes, followed by the private/signature
boundary fixes. The final catalog freshness check closes a separate path where
a cached hover could outlive the catalog that justified it. The reviewer also
replayed a real helper-exported model after its project file changed: the first
tree request explicitly rejected it as stale. A local definition with a catalog
configured succeeded without creating metadata tables, preserving lazy loading.

Architecture review also removed a whole-index hash from every edit, avoided
materializing repository-wide occurrences for local parameter references,
required unresolved/aborted candidates to make global references incomplete,
added analysis-version invalidation, preserved raw-search recall, and moved
library metadata from a resident JSON object to a disk-backed indexed catalog.
The editor transport now coalesces pending edits and bounds its outbound queue.
These changes are meaningful implementation improvements; their presence alone
does not establish an application-level performance ratio.

## Material remaining limits

**Native semantic coverage is incomplete.** The prepared semantic-snapshot mode
can produce a complete clean-fixture result set, but that is compiler-produced
snapshot data. It must be reported separately from native binding coverage and
from preparation time/memory. Even a clean 29/29 fixture result would not imply 29/29 after edits: changed
semantic inputs invalidate snapshot answers and some queries become unavailable.
The final evaluation matrix must state clean and edited coverage separately.
This does not satisfy live Ionide equivalence.

**Resource guards can refuse valid code.** Dense-file stress previously exposed
very large allocations. The bounded parser now reports unavailability rather
than silently dropping candidates. A refused large file is a coverage failure,
not a fast successful navigation result. Raising the limit can trade latency and
memory for coverage; it is not the final algorithmic solution.

**Cancellation discards results without preempting native work.** The daemon
reads requests sequentially. In the recorded stress probe, a cancellation queued
about 0.052 ms after a reference request was processed only after that request
completed around 255 ms, and correctly returned `cancelled: false`. Four generated
20,000-binding candidates and a zero-ms parse budget were used. Per-file parser
budgets do not impose a whole-query deadline across a large candidate set. The
adapter's fast cancellation response must not be described as stopping native
CPU work.

**External definition views currently show metadata signatures.** Opening a
signature document is useful but does not establish parity with Ionide's source
or decompiled implementation view. Library declaration metadata also does not
supply all use-site type inference or overload selection.

**Project and filesystem assumptions need explicit boundaries.** Evaluated
manifests are snapshots of a selected configuration. Input-hash validation can
reject changed recorded inputs, but cannot by itself discover new glob members,
changed build-environment variables or every missed filesystem event. Such
changes require re-evaluation/reconciliation; an exact project-context claim
must be restricted accordingly.

## Performance interpretation

For a proposed **100×** target, require correct complete operations in the same
source state with navigation, library metadata and the tree enabled. Compare
language-service process trees with language-service process trees, and report
whole-editor memory separately. Record setup, first query, warm query, restart,
and edit-to-correct-answer as different states.

Cold end-to-end cost includes project evaluation, metadata extraction and every
helper peak. A small warm Rust resident set cannot hide a large preparation
helper. Record disk/cache scope as well as RSS and sampling limitations. The
user's reported 10 GB experience is not a measured comparison denominator.

An FCS oracle that deliberately invalidates on each request is a correctness
reference, not an Ionide performance baseline. Corpus timings without validated
answers, and quick unavailable responses, do not qualify as speedups. No such
ratio is endorsed by this review.

The next acceptance work is substantive: complete bounded type-dependent
resolution and invalidation, validate representative corpus query traces and
edits, and execute the isolated source/packaged editor suites in an environment
that supports VS Code's IPC. Packaging and a passing toy fixture cannot replace
those gates.
