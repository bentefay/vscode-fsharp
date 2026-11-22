# Immutable packaged-editor navigation gate

The current manifest contains **12 definition scenarios and 5 References route
scenarios**. The final References cohort and its identical frozen-input repeat
each passed **16 required scenarios** and reproduced the one existing signature-candidate known failure separately. See
[References details](#native-references-slice) for UI/range confidence boundaries.

The preceding definition-only slice contained 12 scenarios. The first
extended pinned Code 1.108.0 run passed **11/12**, with a signature-hidden
candidate defect. Its 14 phases include two positive before-edit baselines;
15 trusted editor F12 inputs include the idle-picker repeat. These are different
denominators, not extra language coverage. The original six passed again.
The defect remains in a narrowly scoped expected-failure backlog; an expected
failure is never a passed feature. The final scoped-backlog run passed all
11 required scenarios and reproduced that one expected failure, with no
unexpected failures or passes. See the [coverage matrix and defect](../../../docs/e2e-coverage.md).
Runtime code is unchanged. The installed evaluation VSIX SHA-256 was
`1281401838752845c3c4cc9855752aef032256dff4935a39c17c23b519a5a19a`; its native
SHA-256 was `ee2704a4aafd65dd32c671bc64b6602986826e55a925cb2adaf9d14178744dce`.
No binary or raw run evidence is included in this source checkpoint.

## Run with explicitly provided local assets

Use this slice only with the already provisioned official Code 1.108.0
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
classified as expected product failures. The signature-hidden candidate defect is the only expected failure. Its
classification also requires empty exact results, no wrong navigation and the
observed incomplete one-candidate offer; setup or unrelated failures still fail. Raw reports belong outside tracked source.

## Case contract and actual UI checks

`manifest.json` contains source text, compile order, UTF-16 query coordinates,
full expected target ranges, negative targets and independent oracle rationale.
Fixtures isolate conditional and signature contexts from ordinary exact cases.
Dirty-linked fixtures mount only the App directory, with Shared as a linked
sibling. Authored literal destination text is checked against UTF-16 spans;
fully specified candidate fixtures require exact sets, including a singleton.
`manifest.test.cjs` checks data integrity; its unit tests are not E2E cases.

| Rule ID | Required outcome | Negative/UI obligation |
| --- | --- | --- |
| `lexical.nested-shadow` | Exact inner lambda parameter | Outer same-name parameter forbidden; actual URI/caret at inner declaration |
| `module.ordered-open` | Exact declaration from later open | Earlier module's same-name declaration forbidden; actual cross-file jump |
| `record.typed-receiver` | Exact Person.Name | Company.Name forbidden for explicitly typed receiver |
| `record.library-result-receiver` | Incomplete choices for uncatalogued List.head result | Both Name declarations visibly reachable; no forced jump; idle-picker Escape; explicit refocus, fresh F12, then deliberate Person selection |
| `member.unresolved` | Explicit unavailable outcome | Intentionally ill-typed member, no candidate or navigation; not a complete-reference oracle |
| `conditional.unknown-active-origin` | Incomplete physical-origin choices | Both prefix declarations visibly reachable; no claim branch is active or binding exact; deliberate target selection |
| `signature.public-consumer-interface-target` | Exact public `.fsi` declaration | Implementation destination forbidden; signature-first is explicit product policy |
| `signature.omitted-implementation-not-exported` | No inaccessible implementation suggestion | Known failure: hidden declaration offered, no forced jump observed |
| `project.linked-conditional-context-required` | Explicit unavailable | Both physical branches forbidden while active project context is unknown |
| `dirty.linked-outside-root-current-range` | Exact moved declaration after unsaved linked edit | Positive F12 baseline; unchanged consumer/disk; old range forbidden; clean revert |
| `dirty.linked-rebound-current-declaration` | Exact current wrapper after unsaved linked rebind | Positive F12 baseline; old binding range forbidden; clean revert |
| `candidate.singleton-is-not-exact` | One incomplete choice | No jump at offer or picker; deliberate selection reaches authored full range |

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
restoration, pre-registration Escape, or every late-response schedule. The original and extended
runs reported zero compiler/helper/decompiler starts. No timing or
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
The original six established no runtime defect. The extended slice established
the signature-hidden candidate defect described below; no runtime fix was made.

## Extended-run defect and review

The first extended run failed on the forbidden hidden implementation candidate.
Native exact locations were empty; the final screen offered one possible definition
and the consumer caret did not move. Opening/selecting that hidden candidate was
not exercised. The failed report is preserved unchanged. The subsequent scoped
classification must not erase that failure or count it as a feature pass.

An independent reviewer checked the authored oracles, real key receipts, full
candidate sets, unsaved linked-provider hashes/versions, unchanged consumer and
disk, and clean reverts. A reviewer initially accepted an incorrect singleton
query column; literal span integrity corrected it to 48 before any editor run.
Dirty edits use supported WorkspaceEdit, not simulated physical typing.

## Remaining scope

The required manifest has 17 scenarios: 16 pass and one is a known defect. At least
7 more scenarios are needed even to reach the proposed 24-case floor, which
is not a completeness claim. The [traceable matrix](../../../docs/e2e-coverage.md)
records distinct language rules, explicit unsupported outcomes, References,
hover and tree work, and the next decision. Provider-only support is not a
packaged UI pass. In-flight cancellation needs a proven pending request;
the existing idle Escape case cannot substitute for it.

A normal-sandbox Ubuntu clean install/activation/reinstall remains unobserved.

## Native References slice

Five additional route scenarios exercise two lambda-argument bindings and an
uncertified cross-file scope. They are five public UI behaviors, not five new
language-binding rules. Original definition scenarios remain frozen.

The pinned editor's Shift+Alt+F12 invokes sidebar **Find All References**;
Shift+F12 invokes the compact editor action. Both require a trusted shortcut
receipt from the focused editor. The sidebar must display the exact authored
set and every row must be clicked. Its built-in open command intentionally
collapses the selection: for these single-line spans the driver reconstructs
the full range from actual clicked URI/start caret and rendered highlighted
literal length in UTF-16. Native full ranges are secondary evidence. Carets
begin inside the token, so a declaration-row click cannot pass merely because
it was already at the destination.

For the used binder, sidebar results include declaration and use; compact
Shift+F12 moves from the declaration to its sole use. An unused binder has a
declaration-only result, not a fabricated empty References UI. Both fixtures
exclude same-spelled binders and uses in another lambda scope. Compact mode
checks the actual URI/caret; it renders no row from which to independently
establish an end column, so its full-range confidence remains native-secondary.
The include-declaration/compact-retry explanation comes from pinned editor
source; the harness does not claim to observe provider token identity or each
internal retry.

The cross-file fixture uses only an inert `Directory.Build.props` containing
`<Project/>` to leave the project context uncertified. The runner never executes
it or builds the project. This tests an incomplete scope, not a timing-dependent
pending-discovery window. Standard FAR must expose no partial result rows and
display the explicit warning that the scope was not fully searched and native “No references found” is not evidence of no uses. Only a
click on **Show Possible References** starts the separate occurrence search.
The internal `lastResult` payload may contain partial locations; those are not
the provider export and are not scored as native UI results. The separate
Possible References view contains four physical occurrences including unrelated
homonyms, visibly labelled incomplete; they are not a complete semantic-reference set. All rows must be
reachable and a deliberate known-use click must select the current full range.

Readiness uses bounded polling of matching fresh results and rendered state,
not a fixed sleep. The earlier idle-definition Escape observation interval is
unchanged. This slice does not test References cancellation, dirty edits,
restart, virtualized large lists, hover or project-tree actions.

The first four direct-application lambda reference cases remain in
[reference-proof-backlog.json](reference-proof-backlog.json), with their valid
exact oracles and observed unsupported outcome. They are not executed by the
required gate or counted as passes. The first incomplete-state attempt exposed
a raw-payload/provider distinction in the harness; a subsequent observer checked
the explanatory message before it arrived. Both failed reports are retained.
See the coverage matrix for the precise failure classifications.
