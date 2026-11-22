# Foreign dirty-buffer membership: stopped speed qualification

At source revision `3e1248e94a98c5ee3612388a9f991ffeb2c8e9dd`, the proposed
unrelated-foreign-buffer speed comparison failed its useful-answer prerequisite.
No product change or optimization is justified by this checkpoint.

## Fixed inputs and stop rule

The private evaluation used unchanged VSIX SHA-256
`d14a20e4c54a71b6592bfa8d0aacff6eea775338467f6f32eb58dd4f4bdca409`, pinned
VS Code 1.108.0 and a clean archive of public
[Grace](https://github.com/ScottArbeit/Grace) commit
`39960e07b73cd142cc0c1fd070c0591be91c1933`. No restored assets, evaluated model,
compiler, build or workspace scripts were used. The existing controlled isolated
editor route was reused; this is not normal-sandbox Debian qualification.

Independent review found that raw discovery remains provisional even at full
readiness. `extension/src/extension.ts` (`synchronize`, lines 475–502 at the
revision above) cannot exclude an unknown foreign dirty buffer from an
approximate tree and deliberately refuses the query. The predeclared feasibility
schedule was one session: clean first-use F12, wait for normal raw readiness,
foreign-dirty F12, revert, recovery F12. A confirmed refusal stops the speed
cohort; no retries, alternative corpus or successful-latency substitution.

To reproduce the interaction with an isolated public copy:

1. Open a clean sibling `Unrelated.fs` outside the workspace, containing only
   `module Unrelated` and `let sentinel = 1`. Preopen it without changing the
   active editor during the subsequent edit.
2. In `src/Grace.Types/Events.Types.fs`, issue actual F12 at zero-based `49:41`
   (`serialize`). Require the known `Utilities.Shared.fs` target `213:8–17`
   through the labelled picker and deliberate selection.
3. Wait for ordinary background readiness; this run witnessed 541 indexed files
   across 22 projects. Do not invoke Rebuild Index or preparation.
4. Insert a comment into the external buffer using supported WorkspaceEdit,
   leaving it unsaved. Keep the consumer source/version and F12 position fixed.
   Issue actual F12 again. Record explicit status and correlated existing traces.
5. Revert the external buffer and repeat the same F12/selection oracle.

## Fresh observations

All three F12 inputs were trusted and focused. Clean and reverted states selected
the authored target. Dirty-ready F12 displayed `F#: unavailable` with the exact
reason: “Cannot exclude a dirty external F# buffer from the current project.
Navigation is unavailable until project membership is known.” Its trace contains
one `workspace/tree` exchange, then uncancelled provider completion, **no native
definition request**, no candidate offer and no selection. Reverting restored
useful navigation. Source copies/inputs remained unchanged; tracing was not
saturated; compiler/helper/decompiler counters were zero.

| Observation | Single measured value | Boundary |
| --- | ---: | --- |
| Clean first-use key → hit-testable known choice | 714.60 ms | Renderer clock; includes deliberate picker launch; extension explicitly activated beforehand |
| Dirty-ready host key dispatch → observed refusal | 34.08 ms | Host clock and polling; not time to a useful answer |
| Reverted warm key → hit-testable known choice | 82.40 ms | Renderer clock; same known target |
| Deliberate target click → selection, clean / recovery | 303.10 / 71.86 ms | Host dispatch/selection observation; not human decision time |
| Dirty tree request → parsed response | 13.102679 ms | Same host monotonic clock, matched PID/request ID |
| Native queue / dispatch inside that response | 0.042789 / 7.016346 ms | Native durations; not adapter flattening |
| Tree response → provider completion | 3.207380 ms | Upper bound including flatten, validation, error reporting and scheduling |

There is **one observation per condition**, no medians, tail estimates or noise
qualification. First use and recovery have different readiness/cache states;
their timings must not be interpreted as a matched speed comparison. Dirty-ready
has no useful answer and cannot enter a useful-answer latency distribution.

The immutable trace does not expose response wire bytes or a dedicated flatten
span. Bytes are unmeasured; reserializing another tree would only be a proxy.
The single 13.10 ms fetch is not a repeatedly observed ≥10 ms cost. The 3.21 ms
post-response upper bound does not support ≥10 ms flattening in this sample.
Neither clock subtraction nor native timing isolates transfer/JSON parsing.

## Decision

Stop the speed cohort. A smaller cancellable membership query would not by
itself prove exclusion or repair this availability boundary. Any future narrower
API must preserve unknown membership rather than silently excluding a dirty
buffer. A genuinely included foreign linked dependency would be a different,
valid candidate for a speed experiment, with independent target truth.

The proposed performance gates remain **unverified**: repeated ≥10 ms stage,
≥50% payload reduction, ≥10 ms or 10% useful E2E improvement and ≤5 ms ordinary
median regression. No optimization, RAM, competitor or release claim follows.
Both independent reviewers confirmed the clocks and outcome distinction.
The ordinary packaged suite was not rerun in this docs-only checkpoint; its
34 required passes and signature XFAIL remain prior results. Raw traces, private
harness/profile files and binaries are not published.
