# Navigator A/A measurement boundary

This checkpoint starts from `a1fd6101e1cd25496ce39942386b5f6ece185c08`.
It compares Navigator with itself, using the same immutable evaluation VSIX
SHA-256 `1281401838752845c3c4cc9855752aef032256dff4935a39c17c23b519a5a19a`
and pinned Code 1.108.0. A1/A2 are ordering labels, not different builds.
There is no matched Ionide/Rider comparison or normal-sandbox Ubuntu claim.

## Observed outcome

All 12 corrected launches completed: eight enabled and four disabled. Each
performed 203 keyboard inputs (one cold, two excluded primes and 200 measured
warm/edit inputs). The 1,600 enabled measured inputs comprised 400 correct exact
jumps and 1,200 labelled known-target picker selections, with zero observed
wrong/stale targets or misses. All eight cold cross-file requests reached the
known target through the picker on the same request. All enabled final counters
reported zero helper, compiler and decompiler starts. These are four selected
sites, not a coverage estimate for either repository.

All 16 paired site/state median comparisons passed the predeclared noise screen.
The largest choice-median difference was 21.7 ms; the largest exact-median
difference was 1.34 ms. This qualifies an exploratory median baseline, not
stable population tails, an optimization result or a speed comparison with
another extension. Runtime and package bytes were unchanged.

One initial diagnostic pair is excluded from these counts and retained
privately. Its enabled run completed the cold and prime selections, then the
harness scrolled a previously observed target out of view before clicking it.
No wrong navigation occurred. Independent review confirmed an observer/action
race. One correction required a *currently* hit-testable matching row before
clicking or scrolling; the original first-visible timestamp remained intact.
The complete corrected schedule then ran with identical frozen inputs.

### First use and readiness

Ranges below span four enabled launches per workload; no cold percentile is
claimed. “Focused” is the test file after supported focus commands, not the
earliest possible window-ready event. The harness sent F12 another 82–241 ms
later on Grace and 42–94 ms later on the slice. Choices are observed layout and
hit-testing, not pixel-paint timestamps.

| Boundary | Grace root | FsToolkit eight-file slice |
| --- | ---: | ---: |
| Launch → focused source | 5.22–6.94 s | 5.09–5.35 s |
| Activation callback start → end | 1.67–2.67 ms | 2.32–4.60 ms |
| Native spawn → workspace-ready reply | 9.23–20.73 ms | 12.43–25.86 ms |
| Actual renderer F12 → known target actionable | 897–1,132 ms | 518–675 ms |
| Launch → host-observed actionable target | 6.25–8.28 s | 5.75–6.04 s |
| Deliberate target click → current full-range selection | 214–298 ms | 116–186 ms |
| Host F12 dispatch → completed selection | 1.16–1.52 s | 0.68–0.88 s |
| Launch → observed raw readiness | 9.50–13.16 s | 6.00–6.35 s |

Grace's first post-selection statistics still showed indexing in all four
launches (125–144 of 541 files), followed by completion in the same generation:
the useful result preceded full raw readiness. All 22 projects and 541 files
were present at completion. On the slice, the first statistics already showed
ready, so its during-indexing frontier is unverified. Neither workload has a
query-time index-state witness on the UI critical path. Readiness polling gives
an observation bound, not the exact completion instant.

Disabled focused-source observations were 6.15/5.54 s on Grace and 5.98/4.94 s
on the slice. These few fresh launches do not isolate a precise extension-added
startup cost. Activation callbacks exclude module loading, and native
workspace readiness is not full indexing. Do not call the whole launch time
extension overhead or infer negligible overhead from the callback alone.

### Warm and unsaved edits

Each cell is the range of **four separate per-launch statistics**, each from
50 samples; samples were not pooled. Exact timing is host dispatch → observed
correct caret, whereas choices use renderer key → known actionable row. These
are different endpoints and clocks. All choice samples also completed explicit
current-target selection.

| Site / actual outcome | State | p50 range (ms) | Empirical p95 range (ms) |
| --- | --- | ---: | ---: |
| Grace DeleteReason / choices | warm | 80.3–93.9 | 137.7–193.8 |
| Grace DeleteReason / choices | unsaved edit | 90.9–103.6 | 136.6–272.5 |
| Grace serialize / choices | warm | 87.7–111.2 | 143.2–208.2 |
| Grace serialize / choices | unsaved edit | 83.7–115.8 | 155.8–188.6 |
| FsToolkit pattern x / exact | warm | 12.8–14.2 | 19.9–23.0 |
| FsToolkit pattern x / exact | unsaved edit | 17.2–19.8 | 29.3–37.7 |
| FsToolkit apply / choices | warm | 61.9–67.4 | 77.4–121.3 |
| FsToolkit apply / choices | unsaved edit | 59.5–67.9 | 92.2–134.3 |

The known target was rank 4 of 21 collected choices for DeleteReason, rank 1 of
1 for serialize, and rank 1 of 2 for apply, throughout the measured cohorts.
One collected target remained explicitly incomplete; it did not auto-jump.
Across warm/edit per-launch medians, target click → selection added 25.9–32.5 ms,
56.9–62.7 ms and 42.5–47.6 ms respectively. Complete host key → selection
medians were 133.6–161.2 ms, 174.2–199.4 ms and 123.4–135.2 ms. These automated
clicks omit human reading/decision time. Opening the picker is an extra action
before selecting its target, not a direct jump.

The harness did not issue F12 immediately at edit completion. The following
separates that head start from the response; edit → useful uses a host-observed
endpoint and includes focus/setup. It must not be substituted for the renderer
key timing above.

| Edited site | Edit → key p50 range (ms) | Edit → useful p50 range (ms) | Edit → useful p95 range (ms) |
| --- | ---: | ---: | ---: |
| Grace DeleteReason | 33.0–37.6 | 150.9–166.2 | 223.5–373.6 |
| Grace serialize dependency | 127.1–145.5 | 246.5–288.6 | 380.1–586.2 |
| FsToolkit pattern x | 25.5–30.2 | 42.4–46.3 | 57.5–89.4 |
| FsToolkit apply dependency | 86.1–97.8 | 167.9–193.7 | 262.2–307.5 |

Edits alternate an eight-line unsaved prefix and its removal, with new document
versions, independently authored shifted ranges, forbidden former targets and
unchanged disk bytes. These test layout-changing freshness, not every semantic
rebind/delete case. Saturated 2,048-event native traces prevent later native
phase attribution; UI and registered-provider observations remain available.
Edit/ACK overlap and parser/queue causality are unverified.

### Whole-editor PSS

All values are MiB. Each quiet window uses 6–8 complete samples whose entire
read interval lies inside its two-second window. Of 2,661 process-tree samples,
2,660 were complete. One process-exit/read race outside the quiet windows was
kept partial, never substituted with zero. Both control brackets passed the
predeclared drift screen, but that screen is permissive and does not establish
causal memory attribution.

| Workload / window | Opening / closing disabled medians | Enabled median range | Enabled-minus-both-controls envelope |
| --- | ---: | ---: | ---: |
| Grace pre-work | 791.6 / 810.9 | 790.3–859.0 | −20.6 to +67.4 |
| Grace after-work | 904.5 / 911.9 | 953.4–992.7 | +41.5 to +88.2 |
| FsToolkit pre-work | 800.9 / 805.3 | 832.6–849.2 | +27.4 to +48.3 |
| FsToolkit after-work | 852.8 / 914.6 | 896.7–957.0 | −17.8 to +104.2 |

The envelope subtracts each disabled median from each enabled median in the
same named window; it is not a confidence interval. In particular, the slice's
61.8 MiB after-work control drift leaves a broad incremental estimate. Negative
bounds are retained, not interpreted as an extension memory saving. Enabled
sampled peaks were 1,058–1,113 MiB on Grace and 1,031–1,066 MiB on the slice;
they are lower bounds and are not subtracted from unrelated control peaks.
The scope includes all observed owned Code descendants, including the native
daemon, and excludes the external display, driver and sampler.

Disabled runs opened the same authored target files and applied the same edits,
but their scripted collapsed selection at DeleteReason differed from the
enabled picker's full range. Elapsed times and UI allocation paths also differ.
These measurements cannot be reduced to a precise native-index memory cost.

### Decision and preserved gates

There is a usable exploratory median baseline for these sites, but no measured
runtime improvement or identified scheduling bottleneck. The next bounded
diagnostic should switch nonlocal demand to a second project while the initial
index order is still pending. Stop without a runtime patch if foreground
seeding already answers promptly or the pending order cannot be shown to cause
at least 150 ms of avoidable delay beyond observer noise. Only then consider
reprioritizing the unprocessed suffix. A later paired trial should require
both 20% and 150 ms improvement, no wrong/stale/missing targets or cancellation
regression, no warm/edit median or adequately sampled p95 regression above
`max(5 ms, 10%)`, no more than 8 MiB
additional measured process-tree PSS, and no more than 10% slower raw readiness.
If memory variation cannot resolve the 8 MiB guard, that gate is unverified,
not passed. This checkpoint implements none of that experiment.

The ordinary 24-pass gate and scoped signature/qualified-union failures remain
unchanged; they were not rerun as part of this measurement-only checkpoint.
Prepared List.map remains a separate documented gap. No new cancellation,
normal-sandbox Ubuntu, Nix or matched Ionide/Rider qualification is claimed.
The protected isolated route and all reviewed source/package inputs were
preserved. Raw paths, renderer traces, profiles and binaries remain private.
Private evidence inventory SHA-256:
`dd7b19cc529ff8963715b1f034454aafc187e7041178c6a730b3af3b8cf10090`.

## Frozen workload and protocol

- Full Grace repository root, revision `39960e07b73cd142cc0c1fd070c0591be91c1933`:
  frozen `DeleteReason06` and cross-file `serialize26` sites.
- Existing eight-source-file FsToolkit slice, upstream revision
  `8f5fbc83874a102cdfecbb71e19c5b4467a20f96`: `pattern_x` and cross-file
  `result_apply`. This is a scoped holdout, not full FsToolkit coverage.

The executed order per workload was disabled control, A1, A2, A2, A1, disabled
control. Each launch uses a fresh isolated profile and native cache. Filesystem
cache state is uncontrolled; “fresh” must not be described as OS-cold. No
project preparation, restore, workspace script or implicit compiler is part
of navigation. The prepared List.map, signature and qualified-union gaps stay
outside the timing samples, not outside the project's coverage accounting.

Every accepted timing needs the frozen correct target. Exact navigation and
incomplete choices are different outcome classes. A status count alone is not
a useful choice: the known target must be visible and hit-testable. The explicit launcher click, target visibility and deliberate selection are
separate steps. This pinned package presents the full collected list in its
explicit picker; no Show All action is assumed. Test clicks do not measure human decision time. Misses, timeouts and
wrong/stale results are recorded outside successful-latency denominators; any
wrong or stale result stops the cohort. A later retry cannot reset the first
F12 clock or turn its miss into first-use success.

Unsaved-edit samples must actually change the document version. A target-source
prefix shift has an independently authored current range and forbids the old
range. Cross-file consumer text stays unchanged. Edit-to-key delay and
edit-to-useful response are distinct from key-to-useful response: focusing the
consumer or other harness work can give overlay processing a head start.

The fixed warm/edit budget was 50 samples per site/state/launch. Quantiles
must remain separate by workload, site, state and actual outcome. An empirical
p95 at this size has only a few tail observations; it is descriptive, not a
validated population tail or regression guarantee. Four fresh launches per
workload support only descriptive startup ranges, not startup confidence
intervals. Failed or missing samples cannot be silently removed to satisfy a
noise screen.

## Memory and instrumentation

The memory measure is the sum of PSS for the measured editor process
tree, including renderer, Extension Host and native descendants. PID plus start
time prevents PID-reuse attribution. Unreadable or missing live-process data
makes a sample partial; it must not be replaced with zero or RSS. Shared
display and measurement processes are outside that editor-tree scope.

Sampling targets a 200 ms interval; reading overhead lengthens the actual
cadence. It reports a sampled peak lower bound, not the true peak.
Incremental PSS uses comparable pre-work/after-work observation windows
and the bracketing disabled controls, with both baseline values and the
subtracted range reported. Never subtract unrelated peaks. A disabled editor
has no native index-ready milestone. Different elapsed times, file/undo state,
shared-page proportional accounting and process lifetime limit causal claims.
A fixed observation window after an established state is memory sampling, not
a sleep used to assert readiness.

Trace activation callbacks, launch-to-editor readiness and actual useful UI
are separate spans. Callback traces alone do not establish module-load cost.
No forced activation or manual indexing should precede a claimed first-user
F12. “Full readiness” here means bounded raw discovery/postings completion;
it does not mean full semantic coverage, exact references or compiler-certified
project membership. Limited/error states are not readiness successes.

## Predeclared exploratory noise screen

These thresholds were chosen before results, not fitted afterward:

- Both reversed A/A pairs must have the same correct sample counts and outcome
  classes at each site/state. For median timings, require absolute pair
  difference at most `max(10 ms, 20% of the pair mean)`; useful-choice medians
  use `max(25 ms, 20%)`.
- Disabled-control PSS bracket drift above `max(16 MiB, 10% of the bracket mean)`
  prevents incremental attribution for that window. Smaller drift still does
  not prove unique extension causality.
- Wrong/stale results stop execution. A screen failure or inadequate sample
  count blocks an optimization conclusion; it is not grounds to tune the
  thresholds or discard the failing run.

## Existing scheduling behavior

Read-only review found that these mechanisms already exist:

- Versioned document-local proofs can answer before background discovery.
- Initial indexing order prioritizes the demand file, its unique project,
  referenced projects, then other files.
- Background discovery admits at most 32 entries or one project XML read per
  quantum; indexing admits one source file per quantum. These are bounded work
  units, not hard wall-clock or RSS limits.
- Dirty overlays coalesce by path, retain the latest pending document, serialize
  snapshots and flush acknowledgements before queries. Version/epoch/generation
  checks reject stale results. Foreground requests can preempt background work.

Relevant code: [background order/slices](../crates/navigator-core/src/background.rs),
[foreground navigation](../crates/navigator-core/src/lib.rs),
[overlay queue](../extension/src/overlays.ts), and
[daemon scheduling](../crates/navigator-daemon/src/main.rs).

There is one narrower missing behavior: changing nonlocal demand does not
rebuild an already constructed indexing order. Discovery itself is depth-first
and publishes the raw model after traversal completes. Neither observation
proves a user-visible bottleneck: foreground candidate seeding may already
answer promptly. No scheduling or parsing rewrite is justified by code review
alone. A possible later experiment must first reproduce a delay attributable
to the pending order, preserve every remaining file and all freshness/proof
checks, and outperform the measured A/A noise. No runtime experiment is part
of this checkpoint.
