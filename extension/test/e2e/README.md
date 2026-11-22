# Immutable packaged-editor definition gate

This first manifest-driven slice contains **six distinct rule scenarios**, not a
claim of comprehensive F# support. Its final pinned Code 1.108.0 run passed six:
three exact jumps, two candidate flows and one explicit unavailable outcome.
Runtime code is unchanged. The installed evaluation VSIX SHA-256 was
`1281401838752845c3c4cc9855752aef032256dff4935a39c17c23b519a5a19a`; its native
SHA-256 was `ee2704a4aafd65dd32c671bc64b6602986826e55a925cb2adaf9d14178744dce`.
No binary or raw run evidence is included in this source checkpoint.

## Run with explicitly provided local assets

Use this first slice only with the already provisioned official Code 1.108.0
and a reviewed VSIX. Other editor versions are not qualified by this runner.
Use the protected repository CLI. `FSHARP_E2E_CODE` is the editor executable at the distribution
root, **not** its `bin/code` launcher. Choose a new output directory:

```sh
FSHARP_E2E_CODE=/absolute/path/VSCode-linux-x64/code \
FSHARP_E2E_VSIX=/absolute/path/navigator.vsix \
FSHARP_E2E_OUTPUT=/absolute/path/new-test-results \
  cli/bin/run test e2e
```

The runner does not download, compile or prepare projects, import a snapshot,
run manual indexing, install into a normal profile, or substitute provider API
calls for F12. It uses a disposable installed extension directory, user data,
private XDG/temp directories and a separate test-probe extension. It records
editor/package/native/manifest/harness and fixture hashes. The extension is
explicitly activated before the cases: this does not test cold activation.

This command deliberately reuses the repository's existing controlled Linux
QA route, including its inherited no-sandbox/disabled-trust test flags. It is
**not normal-sandbox Ubuntu acceptance** and is not a recipe for changing system
security or testing a user's normal desktop. Use only an approved disposable
QA environment. Do not repair an unavailable route by changing permissions or
security settings. The ordinary Ubuntu qualification remains a separate gate.

The runner fails on setup errors, changed inputs, nonzero editor exit, failed
required cases or unexpected passes. Expected-failure metadata requires an issue,
reason and scoped product-outcome match; generic input/setup failures cannot be
classified as expected product failures. There are **no expected failures in the
current required manifest**. Raw reports belong outside tracked source.

## Case contract and actual UI checks

`manifest.json` contains source text, compile order, UTF-16 query coordinates,
full expected target ranges, negative targets and independent oracle rationale.
Its two fixtures isolate conditional context from the ordinary exact cases.
`manifest.test.cjs` checks data integrity; those two unit tests are not E2E cases.

| Rule ID | Required outcome | Negative/UI obligation |
| --- | --- | --- |
| `lexical.nested-shadow` | Exact inner lambda parameter | Outer same-name parameter forbidden; actual URI/caret at inner declaration |
| `module.ordered-open` | Exact declaration from later open | Earlier module's same-name declaration forbidden; actual cross-file jump |
| `record.typed-receiver` | Exact Person.Name | Company.Name forbidden for explicitly typed receiver |
| `record.library-result-receiver` | Incomplete choices for uncatalogued List.head result | Both Name declarations visibly reachable; no forced jump; idle-picker Escape; explicit refocus, fresh F12, then deliberate Person selection |
| `member.unresolved` | Explicit unavailable outcome | Intentionally ill-typed member, no candidate or navigation; not a complete-reference oracle |
| `conditional.unknown-active-origin` | Incomplete physical-origin choices | Both prefix declarations visibly reachable; no claim branch is active or binding exact; deliberate target selection |

The driver dispatches F12 through loopback renderer keyboard input and verifies
a trusted key event reached the focused editor. This exercises real key routing;
it is not a physical human keyboard measurement. Exact results assert actual
active URI and collapsed caret, with full native URI/range as secondary evidence.
Candidate cases assert visible incomplete labels, every collected row's full
path and hit-testable location, no forced jump, and full editor selection after
a deliberate pointer click. Provider-only results never count as an E2E pass.
These small fixtures do not establish virtualized large-list reachability.

Cancellation covers a registered **idle picker**, absence of offer/picker and
navigation for 350 ms, then explicit editor refocus and a second recorded F12.
It does not prove in-flight native/acceptance cancellation, automatic focus
restoration, pre-registration Escape, or every late-response schedule. Both
native sessions reported zero compiler/helper/decompiler starts. No timing or
memory performance claim follows from this functional run.

## Failure history and independent review

Two earlier attempts were preserved as **5/6 failures**, not relabelled passes:

1. The first candidate example was `item.Name` on a fresh unconstrained formal.
   F# ordered record-label inference legitimately selected Company.Name. The
   authored expectation of candidates—and the initial oracle review—missed that
   rule. The test aborted before UI delivery; it proves neither wrong navigation
   nor a completed correct jump. The replacement fixture uses `(List.head people).Name`
   with `people: Person list`: Person is the language target, while the unprepared
   engine lacks the library certificate needed to prove the result. Goldens were
   independently reviewed, not generated from engine output.
2. With that corrected fixture, visible choices and Escape dismissal passed, but
   the second F12 timed out before a second-key receipt was recorded. Lost focus
   was an inference, not an observed cause. The final harness explicitly refocuses
   and records the second editor key before waiting. It does not claim Escape
   automatically restores editor focus.

An independent reviewer who did not implement the driver checked authored
coordinates, language assumptions, key receipts, rendered candidate reachability,
full-target selection, cancellation scope and immutable-package provenance in
the final raw reports. All executed inputs and fixture hashes remained unchanged.
No runtime defect was established and no runtime fix was made.

## Remaining scope toward the proposed 24 cases

This is **6 of 24 planned scenarios**, leaving 18 unimplemented; it is not 25% of
all F# language coverage. Next add independently authored positive/negative pairs
for `.fsi` visibility/target policy, conditional active context, and linked dirty
rename/rebind with forbidden old locations. Add deterministic in-flight acceptance
cancellation separately from this idle-picker case. Pattern binders, active
patterns, operators, signatures, generic and dispatch families still need the
shared rule manifest and real-key assertions described by the coverage audit.

References next: sidebar and Shift+F12 separately; one use plus declaration,
declaration exclusion, proven-empty scope, pending cross-file incomplete warning,
repeat/action/currentness and stale-row rejection after linked edits/restart.
Possible References remains a labelled incomplete search, never native FAR.
Hover next: exact rendered external signature/docs with reviewed catalog identity,
workspace collision suppression, missing/stale catalog and restart.
Tree next: visible compile order, duplicate linked instances, actual context
selection followed by F12, rename/project edit and incomplete discovery.

These are pending tests, not expected failures or passed provider substitutes.
A normal-sandbox Ubuntu clean install/activation/reinstall remains unobserved.
