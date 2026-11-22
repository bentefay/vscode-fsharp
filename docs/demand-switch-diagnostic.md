# Demand-switch diagnostic: observation prerequisite

Audited 22 November 2026 from
`66b1daff2eec57fa93f8d2342d423f8bada8ec68`, using the unchanged evaluation VSIX
SHA-256 `1281401838752845c3c4cc9855752aef032256dff4935a39c17c23b519a5a19a`.

**Stopped at preflight: zero new editor or native workload runs.** The package
cannot expose whether the second request's target occupies the pending suffix
of the constructed index order, or which foreground work supplied its answer.
Three independent review tracks (index implementation, editor harness and
causal/UI audit) agreed that another aggregate-progress timing cohort would
not meet the requested attribution gate. No scheduling, runtime, instrumentation
or harness changes were made.

This is an observation gap, not an environment denial or a measured performance
defect. The [earlier A/A results](performance-aa.md) remain the only timing and
PSS evidence from this sequence. A globally unfinished index and a slow F12 do
not by themselves demonstrate at least 150 ms of avoidable queue delay.

## What the unchanged package can establish

| Observation | Existing evidence | Missing distinction |
| --- | --- | --- |
| Native background generation and progress | `stats()` returns phase, generation, consumed-file count, discovered-file count and slices | No actual constructed order, demand origin or per-target ordinal |
| Actual user request and current answer | Keyboard/renderer harness, provider request IDs, literal URI/range assertions and candidate snapshot | No indication that a particular answer came from foreground seeding |
| Native request duration | PID/request-correlated queue and dispatch spans | No identity of the background operation responsible for waiting |
| Candidate-list generation | `candidateOffer.generation` | This is a UI generation, not the native background generation |
| File work | Cumulative read/index counters; a read-only database snapshot can show posting presence | Neither identifies the writer or proves a target's pending queue position |

The [background implementation](../crates/navigator-core/src/background.rs)
keeps `index_order` and `demand_origin` private. Its `status()` exposes aggregate
progress. `prepare_background_index_order()` constructs the order once;
`background_step()` advances the cursor through it. The cursor is not an
inventory of all currently available postings: foreground work can populate
files ahead of that cursor.

In [core navigation](../crates/navigator-core/src/lib.rs),
`possible_definitions()` searches existing eligible postings first. If none
exist, it refreshes at most eight nearby compile inputs and searches again.
That branch returns no request-correlated seed marker. `stats()` reports
cumulative counters and the aggregate background status. Reconstructing an
order from source code or subtracting those counters is an inference about an
interleaving, not a witness of the actual interleaving.

The [adapter exports](../extension/src/extension.ts) expose `stats`, `lastResult`,
`traceEvents` and process IDs. Its candidate controller owns a separate UI
generation. The [backend trace](../extension/src/backend.ts) carries the
[daemon's](../crates/navigator-daemon/src/main.rs) queue/dispatch durations,
local-proof attempts and document-update diagnostics. These do not expose the
missing index-order/seed relationship. The compiled package exports and source
hashes were checked, not merely assumed from current source names.

## Boundary and smallest next prerequisite

Grace was available, but no eligible second-demand trial was launched without
the target-pending witness. The distinct eight-file FsToolkit slice was also
available; a meaningful pending window for this diagnostic remains unverified.
Its earlier first post-selection statistics were already ready. No padding,
artificial sleeps or manufactured backlog were added to force a result.

Before another timing cohort, qualify a bounded, opt-in observation that can
join the following **to the same provider request and its native request IDs**,
including any continuation:

1. Native model generation, constructed-order identity and cursor at dispatch.
2. The second origin and independently authored target's order membership,
   ordinal and current posting/source state, without dumping the whole order.
3. Whether that request reused existing postings, refreshed nearby inputs or
   returned background-pending for adapter continuation, with bounded stage
   spans. Queue attribution also needs the identity/timing of any autonomous
   slice overlapping request admission through dispatch.
4. The unchanged current-version result, actual F12 and deliberate UI selection.

This is a prerequisite specification, not an implemented trace or authorization
to treat inferred positions as measured. It must not introduce waits, change
scheduling, alter proof/cancellation behavior or leak paths into public logs.
Stats requests themselves use the native queue and are not simultaneous
snapshots of a different request's admission.

Stop if the target is not witnessed pending, foreground work already supplies
the answer promptly, or the observations cannot isolate at least 150 ms of
avoidable delay beyond observer noise. Only a demonstrated case could justify
the previously specified pending-suffix reprioritization experiment, retaining
the 20%/150 ms benefit, freshness/coverage/cancellation, warm/edit and readiness
gates. No such experiment is justified by this preflight.

## Memory guard remains unresolved

The earlier disabled-control envelopes cannot resolve an additional 8 MiB.
Passing their permissive drift screen was not passing the proposed memory gate.
A tighter counterfactual would compare identical enabled builds first at a
common observed raw-ready milestone, with matching open buffers, edits,
selection and UI state in each settled-memory window. Use complete
whole-owned-process-tree PSS samples
and alternating pairs; do not subtract unrelated peaks or a disabled editor
at a different lifecycle point. If identical-build paired variation is not
comfortably below 8 MiB, stop memory attribution. Only after establishing that
resolution could the same matched states compare baseline and a qualified
pending-suffix variant. That would resolve settled memory only: transient cost
during reordering needs its own event-correlated samples and remains unverified
if their cadence misses the event. Record native allocation accounting
separately if available; it cannot replace the whole-process-tree guard.

## Verification and unchanged gates

Verification was source inspection, package SHA/export inspection and independent
review. No new latency, memory, correctness or regression pass is claimed.
The ordinary historical 24-pass gate, scoped signature and qualified-union
failures, and prepared List.map gap remain unchanged. There is no new Ubuntu
normal-sandbox, Nix, Ionide or Rider qualification.

The original package, A/A evidence and source worktree were preserved. The
private preflight receipt records source/package hashes; its SHA-256 is
`28f346812b08288b86fe0b14197617f0599ad97acb4da91f78e9e230325b2515`.
Only sanitized documentation is published; raw paths and private artifacts are
not part of the repository change.
