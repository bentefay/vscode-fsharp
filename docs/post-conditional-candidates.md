# Candidate recall after conditional directives

Base: `9c80f4aa2f6229942bf7f6b7b5aa5d5e3854b3e1`.

A fresh engine replay on compiler source
`66187a30e8bb81d571f0244d4bd64405182ca219` reclassified the eight retained
probes before selecting a fix. One reached its strict target (`SHAStream.pos`);
four offered the corresponding implementation declaration instead of the
signature-policy target (`IsEmpty`, `While`, `Value`, `RecoverableException`).
Those four are not wrong-symbol answers. Three lacked the known target: tuple
`idx`, cross-file `mkTag`, and the custom operator. None returned a wrong exact
answer. The latter two still lack sufficient raw project/context evidence.

## One bounded cause

The tuple use and binder follow a `#if`/`#endif` section. Existing candidate-query
recovery validates and parses the current physical source to recover the use
spelling, but discarded declarations from that parse. Conservative context
parsing retained only the prefix before the conditional. This lost a useful
physical candidate without establishing any semantic reason to exclude it.

The repair passes the already-produced physical parse explicitly into current
origin candidate collection. It never enters exact-binding, reference, semantic
or persistent context caches. Existing lexical validation, parse recovery,
one-MiB source guard, foreground budget and cancellation remain in force. No new
read, parse, helper or restore is added. The parse lives until candidate collection
ends, so transient memory overlap can increase; no standing-memory saving is
claimed.

Normal candidate union and ordering are preserved before new physical-origin
rows are appended. The existing 200-location cap and truncation label apply;
new rows cannot displace earlier alternatives. A full cap can still omit a known
target, so this is not complete candidate recall. All new rows remain incomplete
possibilities, including when a single row is collected.

## Regression scope

The first committed red test also required binders inside both conditional
branches. Existing parser traversal does not collect those branch bodies. That
stronger four-target test is retained intact as an explicitly ignored known
failure, not weakened into a pass. No branch parser was added in this milestone.

A separate scoped test requires the unchanged prefix declaration and the exact
post-`#endif` tuple range. The old engine returns only the prefix and fails that
obligation; the repair retains both. Dirty movement/rebinding, old-range absence,
stale version rejection, close restoration, normal candidate caps and withheld
reference behavior remain separately checked. Candidates are physical source
possibilities, not a claim that an unknown conditional configuration was selected.

The strict corpus gate adds the original compiler tuple oracle as a seventh
required definition, preserving all six prior obligations and all eight historic
probes. Both compiler `part` and `idx` have individual dirty/close/restart checks.
This is 22 required useful-result observations across 32 protocol observations;
the repeated historical tuple probe does not increase the distinct-site count.

Normal-sandbox packaged-editor acceptance remains separate and unrun. See the
[daily-use checklist](preview-status.md#daily-use-acceptance-checklist). Engine
timing is not keyboard-to-visible-choice timing, and raw readiness is not proof
of a loaded compiler project or complete references.

## Measured outcome and remaining costs

The promoted three-corpus gate passes all 22 useful-result obligations and all
32 protocol observations on the repaired binary. The old engine fails the four
promoted tuple states while retaining the prior compiler `part` results. Across
the unchanged eight probes, strict-target hits rise from one to two, four
implementation-only hits remain, and true missing targets fall from three to
two. No new exact answer is claimed.

One fixed-order native-only baseline/candidate run used the unchanged 12-site
compiler cohort, separate cold tuple/reference sessions, and ordinary controls.
Warm repeats number three per site; cold and edit observations number one per
arm. OS caches are uncontrolled and sampling overhead is included. No p95 or
matched-editor/comparator claim follows.

| Native boundary | Baseline | Repaired |
| --- | ---: | ---: |
| Cold tuple: launch to first known candidate | No known hit in the bounded continuation | 214.28 ms; rank 3 of 7 |
| Tuple: acknowledged edit to first current candidate | Refusal | 55.44 ms |
| Ready tuple warm median | Refusal | 49.62 ms; rank 1 of 5 |
| Existing `part`: cold first useful | 143.95 ms | 320.00 ms |
| Existing `part`: first current result after edit | 85.16 ms | 73.20 ms |
| Existing `part`: first current result after undo | 72.28 ms | 210.99 ms |
| Main compiler: launch to observed raw-ready reply | 2456.89 ms | 2984.24 ms |

Cold continuations include a 100 ms harness retry interval. Raw-ready timing
includes an intentional 800 ms memory hold and 250 ms polling; it is not exact
index completion or isolated indexing cost. The existing `part`
cold and undo increases remain unresolved observations, not attributed or hidden
by the tuple improvement. They prevent speed-preservation qualification or a
new speed-first preview recommendation on this evidence. The existing packaged
preview remains unchanged. Ordinary exact/candidate controls retained their
outcomes; tiny warm medians do not establish a latency distribution. Old ranges
were absent after edits and obsolete versions were rejected. Downstream Grace
edit latency was not measured here, although its correctness gate was rerun.

Both frozen complete-reference cases and external hover remained withheld;
the cold reference probe produced no useful complete set. Opt-in Possible
References was not timed. Refusal is not a useful fast answer.

In the main compiler session, daemon-tree PSS after readiness was 25.44/25.00 MiB
and after warm operations 43.57/42.68 MiB (old/new). Complete sampled peak PSS was
44.67/44.26 MiB; this is a sampled lower bound, not a hard peak or memory cap.
The 800 ms holds had three or four contained samples. No editor baseline,
extension-host growth, long steady-state or whole-IDE incremental measurement
was made. These figures establish no orders-of-magnitude memory improvement.

Protected checks: 337 core passes, 4 daemon passes, 6 explicit ignored tests,
28 corpus-gate unit tests, 51 tooling/manifest tests, 3 packaging Python tests,
132 adapter/transport tests and 21 real-native adapter tests. An initial adapter
test received the correct stdout result before its stderr diagnostic marker;
the failure was preserved. The test now waits for that marker through its
existing bounded event observer before asserting the unchanged count. No
adapter runtime change was made.
