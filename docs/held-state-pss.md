# Held-state PSS qualification: stopped at readiness

This bounded checkpoint used main `b5785691629041f6fee22d3a981f09386cc9e424`.
It attempted to qualify same-package enabled A/A repeatability for a future
+8 MiB whole-editor regression guard. It did not measure extension overhead,
a memory improvement, or Ionide/Rider. Runtime and package bytes were unchanged.

## Frozen reproduction protocol

Use pinned Code 1.108.0 and one immutable evaluation VSIX, SHA-256
`8c3b8d8bb5e2c3ff929a1818f8376be1cecb2516d4f4219e33c6bc155ae17f66`
(native SHA-256 `2a6f87bee913867f6d4219a20d73e917e527341e7d3c80d6441d89a6c74241f6`).
Diagnostics are off. This is distinct from the unchanged original preview
`1281401838752845c3c4cc9855752aef032256dff4935a39c17c23b519a5a19a`.

The predeclared schedule was four fresh enabled launches, A1–A2–A2–A1;
labels denote identical bits, not builds. Each uses a disposable profile/cache
and a fresh tracked archive of Grace
`39960e07b73cd142cc0c1fd070c0591be91c1933`, without restore, preparation or scripts.
Open only `src/Grace.Types/Events.Types.fs`, caret zero-based 49:41, version 1,
clean, with zero edits, undo or F12 actions. The source SHA-256 is
`60889a5e3d1cfc176ddffb7d5868d1eb0098036d7a8fdf52138714ff1938f089`.

Invoke the supported Solution view focus command to initialize the session/tree.
Poll for complete raw discovery/indexing with no index errors every 200 ms,
for at most 60 seconds. Verify rendered tree state, return focus to the same
source/caret, move the pointer to an observed neutral titlebar location, then
settle for five seconds and sample two five-second windows separated by
endpoint observations.
No stats requests or UI actions occur inside settling or measurement windows;
endpoint observations are outside them. There is no manual indexing, forced GC,
cache dropping, cleanup or system tuning.

Required state equality includes the same document/version/hash/caret and UI,
no visible hover/picker, stable raw-ready generation and file/project counts,
and stable native PID set. Query, parse and local-proof counters must remain
unchanged and equal between runs; helper/compiler/decompiler starts must be zero.
Any state failure stops immediately, without a replacement run.

Sample whole owned Code descendants at a target 200 ms cadence, retaining
observed descendants by PID plus kernel start time after reparenting. Use only
`smaps_rollup` PSS, never RSS or zero substitution. Each sequential read has
start/end monotonic timestamps. Require at least 15 complete samples entirely
inside each window; any overlapping partial sample or observed membership
change across both windows invalidates the run. External display, driver and
sampler processes are excluded. Processes entirely between observations can
be missed; reads are not atomic.

Report total and separate native, Extension Host, main, renderer and other
owned-process PSS. Native PID comes from extension exports, Extension Host PID
from the probe, and renderer role from process arguments. Components partition
each sample; component medians need not sum to the median total.

The frozen engineering screen requires, separately for each window, both
reversed-pair median differences and the range of all four run medians to be
at most 4 MiB. Each run's W1/W2 median difference must be at most 2 MiB.
A numeric noise failure completes the four valid launches then stops iteration;
state/completeness failure stops immediately. Even passing would establish
observed repeatability, not statistical equivalence or an 8 MiB transient bound.

## Actual result and cause

**One launch, zero qualified windows; the cohort stopped.** The 60-second
readiness limit expired. Of 298 observations, one had no session statistics;
all 297 populated observations reported background `idle`, generation 0,
zero indexed files, and incomplete discovery/index flags. The loaded model
reported 541 source files. Query, parse and local-proof counts stayed zero;
helper/compiler/decompiler starts were zero. Inputs and source bytes were
unchanged. The protected runner failed the state gate and did not launch A2.

| Required measurement | Result |
| --- | --- |
| Held whole-editor total PSS | Unavailable: readiness not reached |
| Held native / Extension Host / main / renderer / other PSS | Unavailable: no held windows |
| Reversed-pair variation and drift | Unmeasured |
| Ability to resolve future +8 MiB | Unverified |

The lifetime sampler retained 246 complete observations, but those are not
post-index held-state samples. Substituting their idle native or whole-editor
values would silently change the workload. No numerical noise conclusion is
possible, and no extra demand was inserted after observing the failure.

Independent review confirmed the setup distinction in source:
[`workspace/tree`](../crates/navigator-core/src/lib.rs) loads/drains a project
model without setting background demand. The nonlocal indexed navigation path
sets `requested` and `demand_origin`;
[`background_pending`](../crates/navigator-core/src/background.rs) requires that
request. A tree model is not a completed postings index. This is a failed
measurement prerequisite, not evidence that indexing is slow or broken.

## Decision and preserved evidence

Stop this memory iteration. Any later qualification needs a newly predeclared,
identical nonlocal priming action with a verified useful result before the held
state, or an explicitly different workload. It must not be presented as this
cohort or as a first-F12 benchmark. Until then, the +8 MiB guard remains
unresolved; the earlier broad disabled-control envelopes cannot repair it.
The highest-value independent gate remains isolated normal-sandbox Ubuntu
installation acceptance when a supported desktop becomes available.

The private harness, frozen protocol, failure, sampler records and restored
source receipt are retained. Protocol SHA-256:
`f31686fb13234a177599aad07c7c44a3e1e991391c2d1849a0ac6c6525d4d6d6`;
pre-run state addendum SHA-256:
`b76f267515e82c02e59f95cdc883e6611bbcd5f9f9fa5011808009c5f8c48e5f`;
private evidence inventory SHA-256:
`a2d761af66c88ecfc39324f259cf5cef5216d244e5be00521045b5e8c4a1b471`.
Raw traces, profiles, generated assets and binaries are not published.

This used only the existing controlled disposable route; it does not qualify
normal-sandbox Ubuntu or Nix. The historical 24 required ordinary editor cases,
scoped signature and qualified-union failures, and prepared List.map gap remain
unchanged and were not rerun. There is no new cancellation, coverage, timing,
compiler preparation or CI claim.
