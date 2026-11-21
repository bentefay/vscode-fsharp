# Document-local navigation handover

Frozen on 21 November 2026 at the user's request for a handover.

## Exact state

- Isolated worktree: the session-local sibling `../fsharp-navigator-local-fastpath` (not included in a clone).
- Local branch: `work/local-fastpath`.
- Base and current commit: `efda6f58b7e0d1ccf29c2e85297d073e6d2774d4`.
- Tracked changes: none. Untracked files: none. No semantic implementation was started.
- There is no pending implementation patch to apply.
- No native build, parser test, integration test, or benchmark was executed by this agent.
- No GitHub writes were performed. Main was not modified by this agent.

The work completed in this subtask was source review and integration/cache design.
Do not describe the following proposed changes as implemented or tested.

## Existing source to reuse

`evaluation/experiments/local-lexical-proof/implementation/local_lexical.rs`
contains the staged source-local proof and focused tests. It is not part of the
shipping crate. `extension/test/localLexicalNative.test.cjs` is a staged native
adapter acceptance suite that expects integration; its presence is not evidence
that the feature exists. It uses an editor API double, not a real VS Code host.

The staged proof supports simple application-argument lambda parameters and
parameters of genuinely nested let-functions. Both definition and reference
queries require this local category. Exported function parameters, direct RHS
lambdas and directly returned lambdas are excluded because signature files can
affect their navigation/reference domains. The closed body traversal refuses
unknown roles, possible named-argument equality, directives, parse errors,
quotations, interpolation and other unsupported forms. Do not broaden this
envelope incidentally during integration.

Its explicit limits are 256 KiB source, 50,000 visited nodes, depth 64,
4,096 references, 32 formal parameters, 100 ms parsing and 150 ms total proof
work. These are guards, not hard native RSS or foreground latency guarantees.
The module also samples `/proc/self/status` on Linux; these are resource-monitor
reads, not workspace/source discovery. `query_with_metrics` counts rejected
work as well as successful proofs.

## Proposed integration

1. Copy the staged proof into a new shipping module such as
   `crates/navigator-core/src/local_lexical.rs`. Keep its semantics unchanged
   initially. The build script fingerprints all shipping Rust modules, so its
   addition obtains a new engine identity automatically.
2. Add a document-local attempt at the beginning of `Workspace::navigation`,
   before `ensure_model`, event draining, name inventory, catalog or source
   domain work. Initially enable it only in indexed mode for definitions and
   references. Leave explicit semantic mode and hover behavior unchanged.
3. Require an existing acknowledged overlay and an explicitly requested
   `documentVersion`/`version` equal to that overlay's version. Queries without
   a version fall through. Reject stale versions before project work. Validate
   line/character conversion to `u32` instead of allowing integer truncation.
4. Compute and retain the source BLAKE3 once when an overlay is acknowledged.
   A private/serde-skipped field avoids changing helper overlay serialization.
   `project::normalize`, `from_uri` and `uri` perform lexical normalization;
   they do not canonicalize or read source files.
5. Return exact locations with `source: "local-lexical"`,
   `proofScope: "document"`, source hash, acknowledged version and a
   `documentProofId`. The proof ID must include URI, source hash/version,
   cursor, operation/includeDeclaration, compiled engine identity, parser
   fingerprint and proof algorithm version. Include the proven declaration
   and body range in proof metadata. Do not fabricate `projectContextId`.
6. Add current workspace snapshot/epoch to the response after either proof
   computation or cache lookup. A document-local result is independent of
   other documents, but its response envelope must not contain an obsolete
   global generation copied from an earlier request.
7. Unsupported proof shapes fall through to the existing navigation path.
   Count the query once, and expose local attempts/hits/parsed bytes/nodes/time
   separately from the general index's parsing counters.

`Workspace::change`, the document-close branch, mode changes, cancellation
cleanup and `Workspace::stats` are the other integration points. Existing
document-open acknowledgement does not parse source in indexed mode, though
it updates index metadata. Do not introduce whole-workspace work there.

## Proposed bounded positive-answer cache

The cache is justified by repeated parsing in earlier, unrecovered experiments;
no cache implementation or new timing result exists in this worktree.

- Per-workspace cache, at most 32 entries and 128 KiB accounted retained storage.
- Retain serialized positive answers only: no source text, AST, project data,
  negative result or compiler state.
- Exact key: canonical document URI, source hash, acknowledged document version,
  UTF-16 cursor, definition/reference operation, includeDeclaration flag and
  compiled engine identity. Parser/proof identities are also carried in the
  result and therefore covered by engine identity changes.
- Proposed representation: a fixed-capacity `VecDeque` of entries, with owned
  key strings and serialized answer bytes boxed to exact lengths. Account the
  deque's allocated capacity, entry structures, key storage, serialized values
  and cache structure. This is an accounting bound, not allocator/RSS accounting.
- Lookup updates recency and deserializes a transient result. Add the current
  snapshot envelope afterward. No document text is cloned for a cache hit.
- Accepted changes and close invalidate entries for the URI. Mode changes
  clear the cache. A stale/rejected edit must not mutate valid cache state.
- Check cancellation before lookup/insert and before returning. The dispatch
  cancellation cleanup must clear local cache entries too, covering a cancel
  that races insertion. Never preserve an answer produced by cancelled work.
- Oversized exact responses may still be returned but must not be cached.
  Avoid allocating a huge serialized cache candidate merely to discover it is
  over budget; a bounded serialization writer is preferable.

## Required validation before merging

Use the protected CLI toolchain required by AGENTS.md. After this work was
frozen, the toolchain agent reported that main's task-local Rust 1.100.0,
Safe Chain 1.5.29, .NET 10.0.400 and locked Node dependencies were ready, with
CLI setup/doctor passing. This agent did not rerun those checks. See that
agent's `docs/handoff-toolchain.md` for reproduction. This worktree does not
contain restored generated parser C files until the documented restoration
hook runs. Do not call a missing toolchain or a staged test file a passing test.

- Prove local positives with deliberately unavailable model/catalog/inventory
  paths and zero model validation, general source reads/parses, helper starts,
  catalog loads and watcher activation. Local parse metrics must be nonzero on
  the first successful proof and unchanged on an identical warm cache hit.
- Test cache key separation for operation, declaration flag, cursor, version,
  source hash and URI. Test entry/byte eviction and large uncached answers.
- Test accepted edits, stale edits, close/reopen, same-text version changes,
  cancellation on cache hits/misses and cancellation after proof work. Preserve
  UTF-16/CRLF positions and shadowing behavior.
- Negative controls include exported/direct RHS/returned formals, named
  arguments, directives, parse errors, method parameters and unsupported
  ancestor contexts. No negative caching should conceal later valid edits.
- Run the actual-native adapter suite. Label its editor API double accurately.
- Run existing fixed semantic corpora unchanged after the normal correctness
  repairs are integrated; retain misses/refusals and incorrect results.
- Freeze an executable/source identity before measuring cold first useful
  query, warm queries, edit-to-answer and process-tree memory. Do not infer
  Ionide/Rider ratios or real editor activation performance from native tests.

## Merge coordination

The `navigation_correctness` agent owns the main `lib.rs` repair; `syntax_facts`
owns parser facts. Apply narrow integration hunks to their finished version.
Never replace main `lib.rs` wholesale with this worktree's older copy. Preserve
the independent review gate before claiming acceptance. The user's standing
instruction is to commit validated work directly to main through
`bentefay-agent`; this local scratch branch is only an isolation mechanism.
