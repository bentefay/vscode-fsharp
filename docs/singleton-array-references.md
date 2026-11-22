# Closed singleton-array match references

This checkpoint targets complete native References for an ordinary, unguarded
match arm whose entire pattern is one lowercase identifier in a singleton array,
for example `| [| part |] -> part.Text`. The declaration and every returned use
belong to that arm in one acknowledged current document. No project evaluation,
typechecking, or cross-file absence assumption is required.
The lowercase restriction follows F#'s variable-pattern rule, including when a
same-spelling named literal exists; see the official
[named-literal documentation](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/literals#named-literals).

The new recognition is References-only. Definition retains its existing proof
and candidate paths. The existing bounded lexical body traversal must account
for the whole arm body, including supported nested shadowing. Unknown syntax,
guards, other patterns, directives and computation expressions remain outside
this proof. The pinned parser can fold inline sibling arms into an expression;
the new proof rejects those separators rather than certify a mixed scope.
Existing document hash/version, cancellation, cache and resource guards remain
in force. `includeDeclaration=false` excludes only the declaration, not uses.

## Test-first evidence

The engine regression was committed before runtime implementation. Its first
use-range literal had an off-by-one error, corrected in a separate commit after
independently checking the source text: the declaration is columns 9–13 and use
20–24, zero-based UTF-16. The immutable pre-fix engine still returned unavailable
for both declaration settings against that corrected oracle. This fixture error
did not change the independently frozen compiler-source oracle.

The public compiler case is `dotnet/fsharp` at
`66187a30e8bb81d571f0244d4bd64405182ca219`,
`src/Compiler/Facilities/RichText.fs`: declaration 15:13–17 and use 15:24–28.
Sibling loop and other same-spelling bindings are excluded. Source-only raw
discovery readiness is not an evaluated compiler project. The broader
computation-expression reference probe remains a separate unsupported case.

The strict source-only gate now requires 14 complete reference sets across both
declaration settings: first and ready requests, an eight-line dirty shift, a
nested nonrecursive-let rebind, close/restoration, and a fresh process restart.
Two obsolete-version requests must reject safely. It independently checks the
acknowledged input text, versions, full sets and document-proof provenance;
provenance fields are checked, not cryptographically reconstructed by Python.
Missing cases/phases, unscored answers, wrong sets, extra shadowed locations and
missing provenance fail the gate independently of other successes.

On the pre-fix binary all 14 complete sets failed; both stale requests rejected
and the existing compiler definition obligations passed. The candidate passed
all three corpora: 32 existing definition observations (22 required useful
results), plus 14 complete sets and two stale rejections. All eight historical
compiler probes remain visible; no signature or other unsupported obligation
was removed. The unchanged prepared Grace replay is a separate suite and was
not rerun for this raw-source gate.

Protected aggregate checks passed: 340 core tests, 4 daemon tests, 6 explicit
ignored cases, 51 tooling/manifest tests, 3 packaging tests, 132 adapter/transport
tests and 21 real-native adapter tests. The final corpus scorer/inventory suite
passed 39 tests after its additional provenance checks. Focused native tests
also cover rename, close/reopen, pre-cancelled requests, exact shadow exclusions,
CRLF and an astral UTF-16 offset. Pre-cancellation is not an in-flight UI test.

## Bounded native measurement

A fixed A/B/B/A cohort used two fresh daemon processes and empty Navigator caches
per binary on the same pinned compiler sources. A was the immutable pre-fix
binary; B was the reviewed candidate. Filesystem caches were uncontrolled. Each
process first requested References, then waited 800 ms for memory sampling,
issued the same nonlocal discovery demand, and observed readiness before warm
and edit controls. There was no compiler/helper launch or extra retry cohort.

| Candidate complete-result boundary | Two observations |
| --- | --- |
| Process launch to first complete References | 13.16, 13.41 ms |
| Acknowledged edit to first current complete set | 6.53, 5.84 ms |
| Undo acknowledgment to first current complete set | 7.21, 6.17 ms |
| Warm References median, three calls per process | 0.33, 0.52 ms |

All 16 candidate reference observations matched the complete oracle; the old
binary withheld all 16. Refusals are not equivalent successful latency samples.
Both declaration modes were checked. No possible-reference results were mixed
into these complete sets.

Definition guard targets remained useful labelled candidates. Initial guard
queries were 723–759 ms before and 728–775 ms after; post-undo guards were
64–66 ms before and 77–82 ms after. The latter difference remains an unresolved
small-sample guardrail observation, not hidden by the reference improvement.
Observed raw readiness was 2.62–2.64 s before and 3.34–3.37 s after, including the
deliberate hold, demand and polling. It is not isolated indexing duration.
The retained responses explain the start-time difference: the old cold refusal
had already started scanning, and its later fixed demand found generation 1
indexing. The candidate's cold document proof did not demand the project; its
fixed demand then began generation 0 scanning. Consequently the readiness delta
is not evidence of reduced indexing throughput.

Held post-warm daemon-tree PSS was 40.48–41.80 MiB before and 41.53–41.63 MiB
after. Complete sampled peaks were 40.48–41.80 and 41.54–41.63 MiB respectively;
sampling can miss transient peaks. No memory improvement or precise extension
overhead is established. These are native response/daemon measurements, not
editor rendering or key-to-result measurements. Two launches per build do not
support p95, broad performance equivalence or a daily-use speed claim.

Native complete sets do not establish VS Code References-panel behavior,
keyboard cancellation or normal-sandbox installation acceptance. Those retain
their separate gates. Private reports and binaries are not published with this
source checkpoint.
