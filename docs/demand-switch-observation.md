# Bounded demand-switch observation

This follow-up starts from `510c36a501321afecfcedc5d5dc23df9bc1ff054`.
It implements the observation prerequisite from the
[earlier preflight](demand-switch-diagnostic.md), not a scheduling optimization.
Ordering, budgets, semantic proofs, cancellation and normal navigation defaults
remain unchanged. Numeric opt-in configuration and the bounded, path/source-free
envelope are documented in the [protocol](protocol.md#opt-in-demand-switch-observation).

## What changed and was checked

- At most four numeric model-file selectors are matched inside existing index
  order construction. Mapping publishes only after successful construction and
  is invalidated with the model/order. No diagnostic SQL, source reads or hashes
  were added.
- Definition observations join native request IDs to generation, order identity,
  cursor, watched ordinal and existing candidate lookup/nearby-file refresh
  work. Final candidate indices allow an independent literal target check.
  Posting availability remains unknown; pending cursor position alone is not
  evidence that postings are absent.
- Provider-to-native correlation includes continuations and separately marked
  selection revalidation. The provider ID stays client-side. The adapter copies
  a fixed whitelist of primitive fields; arbitrary strings, paths and nested
  objects are not forwarded into the new trace fields.
- One latest-completed background slice can report temporal overlap with
  admission. It does not establish unique queue causality. Disabled tracing
  adds only fixed checks, with no diagnostic timers or I/O; enabled observation
  overhead is unquantified.

Independent index, observer-effect and editor/causal reviews found no blocking
behavior change. Focused tests cover caps, order origin, invalidation,
cancelled construction, error/cancellation cleanup, final candidate index
mapping and a pending target reached through the existing seed path. The real
two-file test compares tracing on/off and preserves candidate/likely-candidate
results and file-read/parse counts. This is correctness/work-count evidence,
not an instrumentation timing measurement.

Final protected aggregate checks passed: 3 Python tests, 41 Node
tooling/manifest tests, 323 Rust tests (5 ignored), 108 adapter/transport tests,
TypeScript compilation and formatting. A separately named private instrumented
VSIX then passed all **24 required ordinary packaged-editor scenarios**, plus
the existing scoped signature expected failure. The report has 26 passing
phase records because two scenarios include dirty-edit baselines; this is not
26 distinct scenarios. The qualified-union and prepared List.map gaps remain
unchanged and were not rerun in this slice.

## Fixed diagnostic and stop

The frozen Grace revision remained
`39960e07b73cd142cc0c1fd070c0591be91c1933`. A used the existing Grace.Types
`serialize` site. B was independently source-reviewed in a different project:
`Grace.Cache.Storage/CacheArtifactStore.fs` calls
`CacheStore.withBusyRetry`, whose declaration is in the earlier compile input
`CacheStore.fs`. B's oracle was source-authored, not compiler-certified: the
expected target is zero-based line 249, UTF-16 characters 17–30. Project and
source bytes matched the pin. No build/restore or semantic preparation ran.

The fixed maximum was three fresh switch/control pairs, stopping when the
pending/origin witness failed or foreground work already answered promptly.
**Only one launch ran**, with actual A F12 followed by B F12 after global
indexing progress was observed, and one same-launch ready-state B control.
Both B gestures produced one explicitly incomplete choice, retained the correct
literal target and navigated only on deliberate selection. The trace had 107
events and did not saturate. No wrong navigation was observed; helper, compiler
and decompiler start counts were zero.

| Observation | Switched B | Ready B control |
| --- | ---: | ---: |
| Native generation / order serial | 1 / 1 | 1 / 1 |
| Background cursor / target ordinal | 113 / 205 | 541 / 205 |
| Watched target state | pending | consumed |
| Initial candidate lookup selected target | false | true |
| Nearby refresh attempts / completed | 1 / 1 | 0 / 0 |
| Watched target refresh completed | true | false |
| Seed span | 2.711 ms | 0 ms |
| Native queue / dispatch | 0.171 / 105.038 ms | 0.029 / 20.991 ms |
| Renderer F12 → actionable known choice | 291 ms | 78.5 ms |
| Explicit target click → full-range selection | 92.267 ms | 61.484 ms |

These are **single instrumented observations**, not percentiles or a benchmark
gain. The switched response preserved generation/order/cursor through dispatch,
and its candidate index matched the independent target. The watched target's
refresh completed before the correct candidate returned, but this does not
prove the refresh changed contents or was necessary. The latest slice overlapped
admission and lasted 2.278 ms; overlap does not explain all queue or UI time.

The required original A-owned order slot was **not observed**. Consequently
the full causal eligibility check failed, and no subsequent fresh trial was
launched. The raw response reports `targetSeedCompleted=true`; a false value in
the harness's eligibility-filtered summary must not be read as absence of seed
work. A completed its candidate offer before B: this run did not test
cancellation of an in-flight A request. The ordering-origin gap is retained,
not filled by a reconstructed ordinal or an inference from elapsed time.

No at-least-150 ms avoidable indexing delay was established. In particular,
the difference between 291 ms and 78.5 ms at the UI is not a queue-attribution
result. Scheduling remains unchanged; no pending-suffix experiment is proposed
from these observations. The existing FsToolkit slice has only one project
and did not provide an independently meaningful second-project pending window.
No padded fixture or alternate target was substituted after seeing the result.

## Preserved artifacts and limits

The private diagnostic package is distinct from the unchanged `128140…` preview:

- Diagnostic VSIX SHA-256:
  `8c3b8d8bb5e2c3ff929a1818f8376be1cecb2516d4f4219e33c6bc155ae17f66`.
- Its native binary SHA-256:
  `2a6f87bee913867f6d4219a20d73e917e527341e7d3c80d6441d89a6c74241f6`.
- Private editor-evidence inventory SHA-256:
  `84d7a5134fb74af16a14c22a89f8c8c109ad2d557aa3b9f7841b47e4411bd562`.

The public runner was restored byte-identically after the private diagnostic
route. Raw traces, paths, packages and profiles remain private. Only reviewed
trace source, focused tests and sanitized documentation are published.

There is no new PSS measurement: the +8 MiB gate remains unresolved against the
earlier baseline variation. There is no matched Ionide/Rider comparison, normal
sandbox Ubuntu acceptance or Nix qualification. Do not promote the diagnostic
package as a faster preview or treat trace-on durations as default-user timings.
