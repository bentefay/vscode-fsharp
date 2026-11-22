# Immutable packaged-editor navigation gate

The current ordinary manifest contains **17 definition scenarios, 5 References
route scenarios, 2 hover safety scenarios and 1 project-tree scenario**. The
final default language-rule run and its identical frozen-input repeat each
passed **24 required scenarios** and reproduced
the existing scoped signature failure separately. A second known defect—the
qualified union case—remains a strict separately runnable regression, not a
passing unavailable case. See [language-rule details](#language-rule-slice).
The original prepared hover generation remains **0/3 verified**. A separate
fresh explicit-preparation qualification passed **2/3 once**, including rendered
framework documentation, while List.map ownership remained unproven. That
overall gate still failed; see the qualification note below. See [References details](#native-references-slice) for UI/range
confidence boundaries.

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

The ordinary manifest has 25 scenarios: 24 pass and one is a scoped known
defect. A separate qualified-union regression is also a known failure. The
24-passing-case planning floor is reached; it is not a completeness claim. The [traceable matrix](../../../docs/e2e-coverage.md)
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
unchanged. The References slice does not test References cancellation, dirty edits,
restart or virtualized large lists. Hover/tree coverage is separated below.

The first four direct-application lambda reference cases remain in
[reference-proof-backlog.json](reference-proof-backlog.json), with their valid
exact oracles and observed unsupported outcome. They are not executed by the
required gate or counted as passes. The first incomplete-state attempt exposed
a raw-payload/provider distinction in the harness; a subsequent observer checked
the explanatory message before it arrived. Both failed reports are retained.
See the coverage matrix for the precise failure classifications.

## Rendered hover and ordered project tree

The ordinary manifest adds a fresh local-parameter hover suppression check, an
explicitly unsupported unprepared module-homonym check, and one ordered tree
scenario. Hover uses the supported `editor.action.showHover` command and waits
for a fresh response and rendered widget state. Exact workspace ownership and
unknown ownership are different contracts. The latter does not prove absence;
it checks that unavailable ownership does not produce speculative library docs.
Negative widget checks are snapshots after the fresh response, not a proof
against every possible late publication schedule.

The tree scenario focuses the contributed Solution view, expands actual rows
with pointer input, and checks the complete rendered hierarchy. Compile order
is deliberately nonalphabetic and interleaves repeated folder runs. Every leaf
is clicked and must change the active physical URI, including a logical
`Links/Remote.fs` row whose file is outside the mounted App directory. Model
inspection supplies only project identity/readiness. It is not counted as
rendered order or a file-open assertion. This does not establish selected
project semantic context or stale-row rejection.

The separate [prepared hover manifest](prepared-hover-manifest.json) checks
rendered `List.map` and `System.String.IsNullOrEmpty` declaration documentation,
plus suppression for a workspace member colliding with the framework name.
Names and discriminating signature/documentation text are authored from pinned
DLL/XML assets, not copied from a provider response. It does not test use-site
generic specialization. This suite requires genuine existing path-bound
catalog, builtin environment and evaluated-model assets; a clean checkout alone
cannot reproduce it. Never relocate certificates by rewriting paths or hashes.

Run it separately using the same pinned editor/package arguments above, plus:

```sh
FSHARP_E2E_MANIFEST=extension/test/e2e/prepared-hover-manifest.json \
FSHARP_E2E_PREPARED_DESCRIPTOR=/absolute/path/reviewed-local-descriptor.json \
FSHARP_E2E_OUTPUT=/absolute/path/new-prepared-results \
  cli/bin/run test e2e
```

The private descriptor has schemaVersion 1, asset `prepared-library-hover-v1`,
the original root, settings for `libraryCatalogPath`, `builtinEnvironmentPath`
and `evaluatedModelPath`, a `hashes` map of absolute input paths to SHA-256,
`missingPaths` and `directories` witnesses. Its configured asset hashes must
match the public manifest pins. Sources must match the authored snapshots.
Every recorded input is checked before and after execution. An optional
`environment` object may restore only `DOTNET_ROOT`: its literal value must
match every project certificate and resolve to the same already provisioned
SDK as the inherited protected CLI environment. It cannot select another SDK.
No preparation, compiler, restore or manual indexing is performed by this
runner. Test storage and editor configuration remain disposable. Descriptors,
raw reports and path-bound assets are private prerequisites, not public files.

First failures are retained. The unprepared homonym's original exact-ownership
expectation is preserved in [hover-proof-backlog.json](hover-proof-backlog.json),
not scored as a pass. An initial tree observer saw an expanded indicator before
its children arrived; readiness now requires rendered descendants too. Investigating the
initial prepared timeouts reproduced an SDK environment fingerprint rejection
in a read-only native replay, caused by different path spellings for the same SDK. Restoring the original certified
spelling in the isolated process is a setup correction, not a runtime or
certificate change, and that failed run is not erased.

### Prepared hover execution remains blocked

Both prepared attempts failed all three fresh-result waits; these are not
expected failures and do not pass the optional suite. Correcting the certified
SDK spelling resolved the environment error in a read-only native probe, but
did not resolve the editor run. No fresh classified hover result or positive
widget was observed there. The editor reported no model/catalog load and no
compiler/helper/decompiler starts. The remaining editor-child setup/response
blocker is unresolved. Do not infer a hover semantic defect, successful
prepared ownership suppression, or successful documentation rendering from
these timeouts. The strict authored contracts are retained for a separately
diagnosed prepared setup; there is no synthetic replacement catalog or relaxed
expected outcome.

## Language-rule slice

Six strict F12 scenarios were authored against the twenty-case manifest from
`75080a75dec2533fab4baaf3ddf9ba25e7830322`: five remain in the default suite and
the failed union contract is retained separately. Together the authored cases
cover five binding rules plus one identifier-encoding contract:

- Tuple callback: the second destructured slot, excluding the first slot and
  a same-named binder in another callback.
- Match guard: its own arm binder, excluding outer and sibling-arm homonyms.
- Separate failed qualified union regression: `Parcel.Item`, excluding `Other.Item`.
- Module abbreviation: the captured original module, excluding a later nested
  same-named module.
- Recursive self-call: the local `let rec` binding, excluding an outer value.
- Quoted Unicode callback: full backtick-delimited identifier and UTF-16
  coordinates after an astral character, excluding another callback's binder.

Independent semantic review authored/checked the targets without engine output.
The runner is unchanged: trusted F12 must reach the focused editor, the fresh
native result must match the entire authored URI/range, and the actual editor
must jump to its URI/start caret without a candidate offer or wrong transition.
Full end ranges remain native-secondary evidence. The quoted case is a real
keyboard scenario, not merely the existing literal-integrity unit check.
It does not add another lexical binding rule or establish symbolic operator
support. Mutual recursion, post-match leakage, union payload binders and broader
destructuring remain separate missing cases. No unsupported outcome substitutes
for these exact contracts.

The first six-addition run had 24 passes, the existing scoped signature failure,
and one new required failure: `qualified-union-case-identity`. Both same-named
union cases were offered as incomplete candidates instead of the required
exact Parcel case. No wrong direct jump was observed; the assertion stopped
before chooser reachability or selection. The strict contract and unchanged
fixture are retained in [union-definition-regression.json](union-definition-regression.json).
Use the optional manifest flag above to run that still-failing regression; it
has no expected-failure exemption and is not scored as a passing unavailable
case. The default suite retains the other five additions and all original
source context. The first failed report remains preserved.

## Fresh-preparation qualification note

One subsequent controlled run generated fresh genuine assets for the same
reviewed fixture and kept the original three queries and expected outcomes.
Framework `IsNullOrEmpty` documentation and exact workspace-homonym suppression
passed. `List.map` returned fresh unavailable/unknown ownership and failed its
strict exact assertion. This is **2/3 once, not a passing prepared suite**.

Preparation and editor launch shared preallocated private environment paths;
all certificate environment inputs were checked in the actual Extension Host
and native process before hover. No certificate was edited. The temporary
prepared-only runner overlay was restored after this instrumented run; the
committed generic runner does not orchestrate this fresh-preparation workflow.
The original manifest's path-bound asset pins remain unchanged. Do not merely
substitute freshly generated paths into an old descriptor or claim an old
certificate is current because its files still hash correctly.

See the [qualification evidence boundary](../../../docs/e2e-coverage.md#fresh-explicit-prepared-hover-qualification)
and [preparation requirements](../../../docs/prepared-metadata.md#environment-bound-preparation-and-editor-qualification).
The ordinary 24-pass matrix and both known failures are unchanged.

## Opt-in References currentness regressions

The default manifest remains unchanged at 24 required passes plus its scoped
signature failure. `reference-currentness-regression.json` contains two strict
interaction contracts, **not additional passing cases**. Select it with
`FSHARP_E2E_MANIFEST` through the same protected command and a fresh private
output directory. No runtime test barrier or new runtime behavior is introduced.

The native sidebar case observed an old-line click after the native response
already contained current post-edit ranges. Its row-count/label observer cannot
distinguish a transient replacement race from a persistent view defect. The
Possible References case passed in isolation, including visible row removal,
old-command refusal and a refreshed target click, but did not repeat in the
full suite: selection failed before the edit. A preceding setup attempt also
failed because old native rows survived the token-specific initial-clear
check; that check now waits for all native leaf rows to disappear.

Neither interaction earns required-pass or cancellation credit. The command
replay is not a stale-row pointer click, and a timeout is not cancellation.
Both failed attempts and the isolated success remain private. See the
[qualification and exact boundaries](../../../docs/e2e-coverage.md#references-currentness-qualification).
