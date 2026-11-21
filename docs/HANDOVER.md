# F# Navigator handover

Updated 21 November 2026. Implementation was paused at the user's request to
prepare this handover. The next agent should resume the work, using independent
implementation and review agents, and keep committing validated changes to
`main`.

## Read this first

The repository is **<https://github.com/bentefay/vscode-fsharp>**. Its complete
prototype import is commit
[`efda6f58b7e0d1ccf29c2e85297d073e6d2774d4`](https://github.com/bentefay/vscode-fsharp/commit/efda6f58b7e0d1ccf29c2e85297d073e6d2774d4):
531 files, with Git tree `e18271af403b8a32e67f6e3bdc1f5f91b837284f`.
The remote tree was independently checked against the complete local tree.
This handover and its companion artifacts follow that import; fetch the current
`main` before continuing rather than resetting to the import commit.

**The product goal is not met.** The imported native source has known incorrect
answers. The document-local fast path and its cache are not integrated. The
imported candidate's recorded coverage is 98/100 synthetic queries and 14/46
real Grace queries. Those are historical results, not a fresh test run and not
a claim of correctness on arbitrary F# code.

Read [AGENTS.md](../AGENTS.md), this document, then the specific linked notes.
[Current source status](current-status.md) distinguishes the recovered baseline
from later work. Older research and review documents are historical: their
words “current”, “accepted” or “pending” apply to their named candidate, not
automatically to today's checkout.

## What the user wants

Replace a narrowly selected subset of Ionide:

| Feature | Required behavior |
| --- | --- |
| Definition and references | Correct navigation across repository code, projects and external APIs; complete reference sets; dirty buffers and signatures handled correctly. |
| Library-only hover | Show F# signatures/types and available documentation for external-library symbols. Suppress symbols owned by the open repository/workspace, including aliases and source projects reached through their output DLLs. |
| Solution/project explorer | Match the selected Ionide tree behavior and preserve evaluated compile order, linked files and solution/project structure. |

The purpose is **orders-of-magnitude lower memory and much faster operation than
Ionide/Rider**, achieved by doing less semantic work. Startup should be negligible;
lookups should be on demand. Background indexing is acceptable if it takes seconds,
but it must not become a mandatory whole-solution wait before navigation works.
These are goals requiring matched measurements, not achievements already shown.

Rust should own the engine, parsing, indexing, persistence and substantive work.
Keep the VS Code TypeScript adapter thin. Follow the CLI, Nix/devenv, Safe Chain,
isolated editor test and packaging conventions of
[vscode-git-tree-compare](https://github.com/bentefay/vscode-git-tree-compare).
Completion, diagnostics UI, formatting, rename/refactoring and debugger/FSI UI
are outside the requested feature set. Basic language registration/editing assets
remain necessary when Ionide is disabled.

The user explicitly authorized **incremental commits directly to `main` in
`bentefay/vscode-fsharp`, using the connected `bentefay-agent` account**, rather
than the main `bentefay` account. Do not ask again for routine commits within this
scope. Preserve remote changes and never force-push. This is not authorization
to publish to Marketplace or modify the user's normal editor profile.

## State at the handover

| Item | Actual state |
| --- | --- |
| Production native source | Matches recovered source capture `0c8920507029`, which produced candidate `710f046e290da…`; the original 65 native input hashes were verified. Known semantic defects remain. |
| Signature-absence repair | Saved as [an unapplied patch](handoff/pending-signature-guard.patch), with [notes](handoff/pending-signature-guard.md). Only `lib.rs`, +48/-8. `git apply --check` passed. Formatting and semantic/native tests **not run**. Production file restored to HEAD. |
| Named-argument repair | Design only. No `syntax.rs`/`treesitter.rs` changes landed and no new controls were run. See the required behavior below. |
| Local lookup/cache integration | Design only; the isolated `work/local-fastpath` worktree remained clean at `efda6f58`. [Detailed proposed integration and cache plan](handoff/local-fastpath-plan.md). No new implementation/build/benchmark was run. |
| Staged local proof | Existing module at `evaluation/experiments/local-lexical-proof/implementation/local_lexical.rs`; not a shipping crate module. Staged native and adapter acceptance files do not demonstrate integration. |
| Toolchain | Fresh protected setup and doctor passed; Rust 1.100.0, Safe Chain 1.5.29, SDK 10.0.400/runtime 10.0.11 and locked dependencies are ready in the current workspace. [Reproduction and exact evidence](handoff-toolchain.md). |
| Portable real-corpus contract | New [Grace46 contract](../evaluation/contracts/grace46.json), checked by extraction round-trip against the saved capture. No engine run was performed for this check. [Replay instructions](../evaluation/contracts/README.md). |
| Generated parser storage | Complete, tested repository import support: bounded compressed parts restore byte-identical C files. 19 Node tests and independent failure controls passed during the import work. This is build preparation, not native semantic validation. |

The imported source came from a durable source checkpoint after the earlier
workspace disappeared. Later experiments mentioned in the conversation were
not in that checkpoint: native candidates `9861`, `dd1a` and `b5fa87` were not
recovered. Do not claim their fixes or timings as properties of the current
source, and do not recreate them merely by changing a version field.

One historical, unrecovered local-path comparison reported first native answers
of 6.8–8.2 ms versus 160–555 ms, but warm queries regressed from about 0.77 ms to
6.16 ms through repeated parsing. This motivates testing a bounded answer cache;
it is **not reproducible performance evidence for this checkout**, an editor
startup result, or an Ionide/Rider ratio.

## Architecture and source map

The default indexed path is Rust plus a persistent SQLite index. Opening a
workspace defers source parsing and project/catalog work. Explicit preparation
can evaluate the project and extract compact library facts, then exit. Indexed
navigation must never silently start FCS or a compiler process when proof fails.
An explicit semantic mode/oracle exists for experiments; it is not the default
fallback. Earlier planning documents considered bounded query-time FCS more
broadly; the current separation in `AGENTS.md` governs implementation.

| Location | Responsibility |
| --- | --- |
| `crates/navigator-core/src/lib.rs` | Workspace lifecycle, navigation dispatch, binding, references, overlays and counters. |
| `syntax.rs`, `treesitter.rs`, `literal_projection.rs` | Tokens, scopes and structural facts; unknown/error syntax cannot prove absence. |
| `typeflow.rs`, `builtins.rs`, `library_types.rs` | Supported type/receiver/alias flows and guarded external/source selection. |
| `index.rs`, `metadata.rs`, `inventory.rs` | Disk-backed source/library facts and streamed complete name inventories. |
| `project.rs`, `membership.rs`, `context.rs`, `prepared.rs`, `validation.rs`, `globalization.rs` | Ordered project contexts, source membership, dependency/environment validation. |
| `crates/navigator-daemon` | JSON-line stdio transport, cancellation and dispatch. |
| `extension/src` | VS Code providers, solution tree, overlays, transport and virtual documents. |
| `semantic/` | Explicit MSBuild/FCS project and metadata helper; optional external declaration/decompiler helper. |
| `cli/`, `scripts/`, `nix/` | Protected build/check/test/prepare/package/install automation. |
| `evaluation/`, `semantic/tests/`, `extension/test/` | Native measurements, compiler controls, editor-API-double tests and real-host harness. |

The protocol is documented in [protocol.md](protocol.md); code and current tests
take precedence over older schema paragraphs. Model producer `09f011…` generated
observed-v4 certificates; schema2 binary facts use the `binary-facts-v2` contract
associated with producer `f215…`. These historical binaries/models/catalogs are
not currently in the runtime directory. Regenerate actual artifacts; never
hand-edit acceptance/version fields or reuse obsolete host/path witnesses.

The accepted reusable-model profile remains deliberately narrow: Linux x64,
SDK 10.0.400/runtime 10.0.11, glibc 2.41 with the reviewed globalization/loader
inputs. Nix SDK realization and a tiny F# build previously succeeded, but its
different loader/locale profile is not accepted for reusable preparation. See
[Nix notes](../nix/README.md) and the
[separate profile proposal](review/nix-profile-design/proposal.md). Do not widen
the certificate allowlist just to make a new machine's tests pass.

## Reasoning that must survive the handover

The full [researched proposal](research-history/FSharp_Extension_Plan.md),
[independent proposal review](research-history/FSharp_Plan_Review.md) and
[Rust findings](research-history/rust-architecture-findings.md) preserve the
alternatives. Their original “not implemented” statements describe planning day.

| Approach | Present conclusion |
| --- | --- |
| Ripgrep/text search | Useful candidate discovery; spelling and unique-name counts alone do not establish symbol identity or complete references. |
| Tree-sitter, lexical scopes and small type-flow rules | Preferred fast route when a closed, tested structural argument proves the result. Unknown syntax must refuse or broaden work. |
| New Rust F# type checker | Not a small initial replacement strategy; full inference, overload, SRTP, extension and generated-symbol semantics are substantial work. |
| Reduced/tuned FSAC | Useful comparator and lower-maintenance alternative, but not demonstrated to meet the footprint goal. |
| Explicit minimal FCS helpers | Appropriate authoritative oracle/preparation/export tools. A “single-file” compiler API can still load broad dependencies; instrument actual work. |
| Persisted semantic results/content hashes | Potentially cheap warm lookup; validity still depends on real project/configuration/source/assembly context. Unsaved inputs and inference dependencies prevent naive hash reuse. |
| DLL/PDB/XML/SourceLink metadata | Good external declaration/documentation source; does not by itself resolve a use-site receiver or represent dirty local semantics. XML documentation usually lives beside DLLs. |
| Existing `.fsi` boundaries | Can limit dependency work; invented/generated signatures are not automatically equivalent and must not conceal inline/public-inference dependencies. |

FCS already has incremental state and dependency-graph mechanisms; the diagnosis
is not that it always throws everything away. A new persistent immutable cache
must show that its dependency boundary is sound. Reducing the feature set creates
opportunities for smaller proofs and retained results, but does not turn global
references into a purely syntactic operation.

Important demonstrated counterexamples:

- A unique record-field name does not prove an arbitrary `x.Property` use:
  class members, extensions, anonymous records, scope and expected types matter.
  Keep ambiguous and unproved cases in the coverage denominator.
- A static factory's apparent record literal can be constrained by other code
  through SRTP, unused lambdas, quotations, aliases or generic inference. The
  retained repair permits a much narrower closed literal form and deliberately
  loses coverage. See [factory review](review/factory-literal-review-78fd.md).
- Ordered nominal lookup must use the declaration's environment, maintain
  source order/accessibility, and refuse unmodeled AutoOpen collisions. See
  [nominal review](review/ordered-nominal-review-3fb2.md).
- Repeated hashing of shared dependencies can be reduced within one validated
  generation, with file identity/stamp checks and invalidation. The
  [digest review](review/digest-generation-review.md) records component savings;
  fewer logical hashed bytes are not an equal end-to-end speedup.
- `.fsi` pairing is not simply `implementationPath + "i"`: valid same-directory
  files can have different basenames. Same-named public parameters can gain a
  signature occurrence; renamed parameters can pair for navigation while their
  reference identities remain separate. Method/constructor labels can add
  caller-side references even where the pinned FSAC provider omits them. See
  [parameter boundary findings](../evaluation/experiments/local-lexical-proof/parameter-boundary-findings.md).

## Immediate correctness work

### 1. Close signature-absence proof

The current case-sensitive `.fsi` check misses valid `.FSI`/`.FsI` inputs.
The saved patch changes the shared absence proof to accept only positively
recognized `.fs`/`.fsx` source extensions with ASCII-insensitive comparison.
Signature, unknown, extensionless, legacy and non-ASCII suffixes block that proof.
The shared factory guard also depends on it. Apply the patch only after checking
the current base, then format, run its two named tests and independently verify
real compiler admission. A clean patch application is not validation.

### 2. Separate named-call labels from caller locals

In `let caller x = C.M(x=x)`, the left `x` can name a callee parameter while the
right `x` is the caller's local. The current normal binding path can return the
caller's declaration for both and include the label in an “exact” caller reference
set. In an ordinary function call `g(x=x)`, the equality is real and both operands
can belong to the caller. Blanket removal of the left operand is also wrong.

Proposed parser API, not implemented:

```rust
CallArgumentNameCandidate { token: usize, callee: Option<usize> }
ParsedFile.call_argument_name_candidates // serde default
ParsedFile.ordinary_function_heads       // serde default
```

The candidate fact means **possible label**, not established semantic identity.
Pinned compiler `TryGetNamedArg` recognizes equality with a single
`LongOrSingleIdent` on the left, including optional names; `GetMethodArgs` handles
the outer call parentheses and tuple slots. Inspect actual parser trees before
coding, particularly `?x=x`. Adjacent and spaced calls represent parentheses
differently. Escaped single identifiers such as ``x.y`` and ``_`` must survive;
the existing `expression_name` deliberately rejects them and is unsuitable here.
Parenthesized, typed and genuinely qualified left expressions remain equality
operands. Account for inline-rule fragment reparsing and interpolation projection.

Guard shared binding resolution before lexical lookup and audit
`reference_binding()`'s earlier type/local returns. A conservative exemption can
prove the actual same-file unqualified ordinary-function head, with scope and
nominal/case/potential-name/AutoOpen/attribute collision checks. Lowercase spelling
is not proof. Exclude value aliases/methods/constructors/active patterns/operators
from structural ordinary-head facts unless separately proved. Refused labels
must create a coverage gap, not a complete empty reference result.

Independent controls must cover constructors, instance/static methods, aliases,
callbacks/nested functions, optional/escaped labels, grouping and tuple arguments,
plus ordinary/qualified/aliased/function-valued equality positives. Check both
LHS definitions and RHS reference sets; retain valid and invalid compiler cases
separately, including project/signature diagnostics. Test edits, restart and old
cache reuse, not just one cold source.

## Local-first performance work

After the normal correctness repair, integrate the staged local proof using the
[detailed plan](handoff/local-fastpath-plan.md). Begin with acknowledged overlays,
explicit matching document versions, simple application-argument lambdas and
genuinely nested function parameters. Both definitions and references exclude
exported formals, direct RHS lambdas and directly returned lambdas because `.fsi`
can change their navigation/reference domain. Unknown roles, directives, parse
errors, quotations/interpolation and other unsupported shapes fall through.

Attempt the proof before model/catalog/name-inventory loading. Preserve UTF-16
ranges and return a document proof with source/version/parser/engine identity;
do not invent a project-context ID. Its bounds include 256 KiB source, 50,000
nodes, depth 64, 4,096 references and 32 formals, with parse/total guards. They
are not hard RSS or foreground-latency guarantees. Count transient local parsing
separately; resource-monitor reads are not “zero I/O”.

Proposed cache: positive serialized answers only, at most 32 entries and 128 KiB
accounted storage, no AST/source/project/negative cache. Keys include URI, source
hash, version, cursor, operation, declaration flag and engine identity. Accepted
edits/close invalidate the document; mode changes and cancellation clear relevant
state. Oversized replies must not be cached. Add the current response snapshot
after a cache hit so unrelated edits cannot leave an obsolete envelope.

Prove useful local queries with deliberately unavailable project/catalog paths
and zero model, inventory, general source-discovery and helper work. Check cold,
warm, edit-to-answer, eviction, cancellation, close/reopen, UTF-16/CRLF and stale
versions. The existing native adapter suite uses an **editor API double**; it is
not an actual VS Code extension-host pass.

## Evaluation and acceptance

| Fixed workload | Preserved denominator and use |
| --- | --- |
| Synthetic | `fixtures/expectations.json` plus `fixtures/sources`: 100 clean queries. Historical recovered result 98/100. Do not change queries to recover a score. |
| Grace | [Portable contract](../evaluation/contracts/grace46.json): original 30 selected positions, including 23 source sites / 46 scored operations; six external and one unmatched candidate remain explicit. 189 source reference locations, including the 53-occurrence direct-FCS Sha256Hash override. Historical result 14/46: 10 definitions and four complete reference sets. |
| Supplemental local queries | Separate denominator. The later frozen supplemental matrix was not recovered; do not claim it was reproduced or substitute easier sites into the fixed suites. |

Follow [contract replay instructions](../evaluation/contracts/README.md). The
synthetic runner's stable `--work WORK` creates `WORK/fixtures`; prepare a genuine
model for that exact root. Use immutable binaries and unchanged query/source
hashes. Current runtime helper/models/catalogs are absent and need genuine
regeneration under the supported SDK/environment.

Use `dotnet/fsharp` at `f896797a0f9b238a8cb1c482be424b714037ddc5` as the largest
**among ten measured repositories**: 6,811 F# files and 1,090,766 physical lines,
not code-only SLOC or one valid giant project. Its test fixtures include invalid
code. Grace at `39960e07b73cd142cc0c1fd070c0591be91c1933` is the application gate;
B2R2 adds large projects without signature boundaries. See the
[corpus inventory](research-history/corpus-findings.md) for exact counts and
selection caveats.

Fable at `235bb0ffcfc580873aecb28eb19c192f430ee5fc` has a historically evaluated
`fcs-fable` graph with 287 ordered inputs and about 250,598 physical lines under
SDK 10.0.100. That is not the same project as `Platform.fs`'s small graph, and the
current SDK400 model profile does not authorize SDK100 preparation. The compiler
repository's former resource/task-host block and all large-project readiness
must be checked afresh. Do not replace genuine graphs with synthetic file lists.

Acceptance rules:

1. Any newly demonstrated wrong exact answer blocks the candidate, regardless
   of performance. Partial/unknown/aborted work is not an exact empty set.
2. A safe refusal closes a correctness hole but reduces coverage; report both.
   Full reference equality matters, not merely finding one plausible definition.
3. Keep compiler validity, semantic completeness, actual provider parity and
   paired signature target preferences distinct. Preserve `includeDeclaration`
   results even when the pinned baseline ignores that flag.
4. Freeze source/binary/helper identities before review. Run an independent
   critic against adversarial controls, then unchanged fixed suites and edits.
5. Measure only matching correct operations on identical source/configuration
   and lifecycle. Keep misses in denominators and setup/prepare/build costs in
   the appropriate end-to-end accounting.
6. Coordinate quiet measurement windows; use matched repeated/order-balanced
   runs. Report native process trees separately from the whole editor and all
   children, cold first useful answer separately from empty activation, and
   cache/restart/edit conditions explicitly. No cross-scope memory ratios.

Pinned historical oracle: Ionide extension **8.1.0**, bundled **FsAutoComplete
0.86.0**. Do not call that server “FSAC 8.1”. Real VS Code host validation was
blocked by AF_UNIX restrictions in the earlier workspace and remains unverified.
Native/adapter success does not remove that gap. No current full Ionide/Rider
orders-of-magnitude comparison exists.

## Practical resume sequence

1. Fetch current `main`, read `AGENTS.md`, inspect local changes, and verify the
   connected account is `bentefay-agent`. Preserve existing remote work.
2. Follow [toolchain handover](handoff-toolchain.md). In this workspace,
   `scripts/with-local-tools cli/bin/run doctor` works. A fresh clone does not
   contain ignored tools/dependencies. Run setup through the protected route;
   never bypass Safe Chain, its 48-hour age rule or disabled lifecycle scripts.
3. Establish a fresh immutable baseline with `cli/bin/run check` and
   `cli/bin/run build` through the wrapper. The latest restoration/setup is not
   itself a native test pass. Coordinate builds before any timings.
4. Apply and validate the pending signature patch. Reproduce the named-argument
   defect with actual compiler-valid controls, implement the shared parser/binder
   repair, and independently review both. Commit a coherent validated slice.
5. Regenerate genuine preparation artifacts and replay the unchanged 100/46
   gates, preserving exact misses/deltas. Do not promote safety to full parity.
6. Integrate the narrow document-local proof and bounded cache with independent
   lifecycle/invalidations tests. Merge narrow hunks into the repaired `lib.rs`,
   never overwrite it with an older isolated copy. Commit the validated slice.
7. Measure native cold/warm/edit behavior and memory, then real host and pinned
   FSAC/Ionide comparisons on ready genuine graphs. Iterate on common proven
   cases while keeping remaining coverage and whole-product gaps visible.

Commit before long experiments so work does not exist only in scratch. Keep
generated runtime caches, corpora, binaries and private execution receipts out of
Git. Source/evaluation contracts, meaningful controls and concise review reports
belong in the repository; Git-backed project work does not need duplicate storage.

## Git delivery notes

The connected GitHub API authenticated as `bentefay-agent` and successfully
published the source. Use the plugin for supported operations. In this workspace
plain Git had no authenticated push credentials; public fetch worked. Do not
extract credentials or change accounts to work around that.

Large connector payloads stalled. Bounded text tree batches around 620 KiB and
binary blobs at or below 512,000 bytes worked. Generated parser source is therefore
committed as verified compressed parts; the installed extension does no restoration
work. See [storage details](../scripts/generated-parser-inputs/README.md).

For connector commits, read current `main`, create blobs/tree for the exact
allowlisted change, compare the resulting Git tree with the local intended tree,
create a child commit and update `main` with `force: false` and an expected-head
lease. Inspect a changed head before retrying. Verify the resulting remote commit
and tree. The original local preparation commits were retained on
`prepared-source/2026-11-21`; they are historical local branches, not required
runtime inputs and not assumed to exist in another clone.
