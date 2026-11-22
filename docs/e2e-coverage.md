# Packaged editor coverage and remaining gates

The current default gate has **26 required passing scenarios and one scoped
signature failure**. The [three-family F12 slice](#active-pattern-operator-and-generic-record-slice)
adds one generic record proof and preserves two strict qualification failures. The [Possible References pointer qualification](#possible-references-pointer-qualification)
adds one post-edit interaction; it does not close native References currentness
or in-flight cancellation.

The preceding language-rule slice extends verified main
`75080a75dec2533fab4baaf3ddf9ba25e7830322`: **24 required passes and one scoped
signature failure across 25 default scenarios**. A second known failure,
qualified union-case identity, is preserved as a separate strict regression.
The original prepared-hover generation remains **0/3 verified**. A separate
fresh, explicitly prepared generation now has **2/3 passing checks in one run**;
its overall gate remains failed. These counts do not include unimplemented
matrix rows or turn known failures into passing features.

The preceding References slice extends source checkpoint
`70b00733090db4ddc239109c60622897b0bfa09e`. Its final pinned Code 1.108.0
run and its identical frozen-input repeat each passed **16 required scenarios**,
with the existing signature-candidate known failure reproduced separately. These are **12 definition scenarios and
5 References route scenarios**, or 19 execution phases including two dirty
baselines. References adds two lexical binding rules through two UI routes and
one incomplete-scope action; route counts are not language completeness counts.

The preceding definition inventory extended `d203de6763b8353c2d647b61dd0f3ed3528b4460`.
Runtime and the selected evaluation package are unchanged. The first extended
Code 1.108.0 run passed **11/12 scenarios**, or 13/14 phases, with 15 trusted editor
F12 inputs. Two phases positively prime linked-provider targets before edits;
one candidate scenario repeats F12 after idle-picker Escape. These do not add
independent language rules. Original six contracts remain frozen by a digest.
The final `packaged-1.108.0-scoped-backlog` run passed all 11 required scenarios
and reproduced one narrowly classified known failure: 13 passing phases and
one expected-failure phase, with no unexpected failure or pass.

Inputs and runnable method: [manifest](../extension/test/e2e/manifest.json),
[host](../extension/test/e2e/host.cjs), [renderer](../extension/test/e2e/renderer.cjs),
[runner](../extension/test/e2e/run.py), [instructions](../extension/test/e2e/README.md).
The manifest supplies source-linked language rationale and authored literal
UTF-16 targets. Golden answers are not generated from Navigator output.
Signature-first navigation is a product policy, not a language requirement.
The preserved failed run is named `packaged-1.108.0-first`; each local run directory
contains `report.json`, `receipt.json`, hashed input snapshots and fixture copies.
Raw reports are deliberately not committed. The sanitized result below is not a
substitute for those local logs when independently reproducing an execution.

## Evidence boundary

F12 is trusted renderer keyboard input received by the focused editor. Exact
cases assert the active URI/caret and secondary native full range; candidates
assert the labelled visible offer, hit-testable rows, no forced jump, and the
full selected range after a deliberate click. Dirty edits use WorkspaceEdit,
not physical typing. Provider calls and integrity tests are not UI passes.
The package is preactivated in a disposable controlled QA profile; this is not
cold startup, normal-sandbox Ubuntu installation, or a performance benchmark.
No compiler/helper/decompiler starts were reported in this cohort.

VSIX SHA-256: `1281401838752845c3c4cc9855752aef032256dff4935a39c17c23b519a5a19a`.
Native SHA-256: `ee2704a4aafd65dd32c671bc64b6602986826e55a925cb2adaf9d14178744dce`.

## Signature-hidden candidate defect

Repro: `signature-hidden-consumer` in the manifest. `Wire.fsi` exports `render`
but omits implementation binding `concealed`. A later consumer opens Wire and
uses `concealed`. Required behavior is explicit unavailable without suggesting
that inaccessible implementation declaration. Existing signature-boundary policy
is also recorded in [the native fixture](../fixtures/signature-boundaries/expectations.json).

Observed: exact locations were empty, but the full and likely candidate lists
contained `Wire.fs` line 2, UTF-16 columns 4–13 (zero-based). The visible offer
showed one possible definition with an incomplete status. The active consumer
caret stayed at its origin. This is a candidate visibility false positive;
**no wrong exact jump was observed, and selecting the hidden candidate was not
tested**. The first required run remains failed. A separate narrowly matched
expected failure records this backlog item; it cannot absorb setup, keyboard,
wrong-navigation or unrelated assertion failures. An unexpected pass fails the
backlog gate so the expectation must be reviewed rather than silently retired.

Proposed follow-up, not implemented: inspect candidate collection at a verified
signature boundary and suppress an implementation-only export only where the
current signature, paired implementation and applicable consumer context prove
inaccessibility. Preserve uncertain alternatives when that proof is unavailable.
Test public consumers, same-implementation private uses, conditional signatures,
linked contexts and dirty signature changes. Do not turn spelling uniqueness or
candidate ranking into proof, and do not weaken stale-result rejection.

## Definition rules

Every implemented fixture and exact range is in the manifest. `Planned:` names
below are proposed fixture IDs, not files already present. Each missing family
needs an ambiguity or safe-refusal negative where proof is unavailable.

| Distinct rule | Fixture / user action | Authored oracle | Status |
| --- | --- | --- | --- |
| Nested lexical shadow | `nested-shadow`, F12 | Inner binder, forbid outer | Pass |
| Ordered opens | `ordered-open`, F12 | Later open, forbid earlier | Pass |
| Typed record label | `typed-record-label`, F12 | Person.Name, forbid Company.Name | Pass |
| Uncertified library result receiver | `library-result-receiver-choices`, F12 then offer/click | Exact two-choice set, incomplete, no forced jump | Pass; idle Escape/repeat also passes |
| Unknown member | `unknown-member`, F12 | Unavailable, no target/candidates | Pass |
| Unknown conditional origin | `conditional-origin`, F12 then choice | Exact two physical declarations, no active-branch claim | Pass |
| Public signature boundary | `signature-public-consumer`, F12 | `.fsi` export, forbid implementation target | Pass |
| Hidden signature implementation | `signature-hidden-consumer`, F12 | No inaccessible implementation suggestion | Known failure above |
| Unknown linked project/conditional context | `linked-conditional-context-unknown`, F12 | Refuse forced branch; forbid both branch targets | Pass; not selected-context coverage |
| Dirty linked range movement | `dirty-linked-shift`, baseline F12, unsaved edit, F12 | Current shifted declaration; forbid old range | Both phases pass |
| Dirty linked rebinding | `dirty-linked-rebind`, baseline F12, unsaved edit, F12 | Current wrapper; forbid old binding range | Both phases pass |
| One unproven candidate | `singleton-unproven-choice`, F12, offer, click | Exact singleton set remains incomplete; only selection jumps | Pass |
| Tuple/destructuring slots | `tuple-second-slot-binding`, F12 | Second tuple callback binder; forbid adjacent slot and other callback | Pass; broader patterns pending |
| Match/guard lifetime | `match-guard-arm-binding`, F12 in guard | Own arm binder; forbid outer and sibling-arm binders | Pass; post-match leakage pending |
| Union case/field | `qualified-union-case-identity`, F12 at `Parcel.Item` | Exact Parcel case; forbid Other.Item | Known failure in separate strict regression; payload binders pending |
| Active pattern | Strict `active-pattern-case-not-union-homonym`, F12 at match case | Recognizer header `Hit`; reject qualified-only union homonym and payload binder | Failed qualification: incomplete internal candidates include the forbidden union; no direct jump |
| Symbolic/custom operator | Strict `custom-symbolic-operator-full-span`, F12 inside `+++` | Full three-character declaration span in own module; reject unopened sibling operator | Failed qualification: unavailable before exact/span assertion; not refusal pass credit |
| Type parameter binding | Planned: `type-parameter`, F12 | Binder, forbid nominal homonym | Pending |
| Generic receiver substitution | `generic-record-substitution-nominal-field`, F12 at final projection | `Envelope<Human>.Payload.Label` → `Human.Label`; reject `Robot.Label` and intermediate `Payload` | Pass; generic type-parameter binder navigation remains separate |
| Overload resolution | Planned: `overload`, F12 | Signature-correct target or all plausible choices | Pending |
| Extension visibility | Planned: `extension-scope`, F12 | Applicable intrinsic/optional extension or safe refusal | Pending |
| Interface dispatch | Planned: `interface-slot`, F12 | Explicit slot/implementation target policy | Pending |
| Inheritance and shadow | Planned: `inherited-member`, F12 | Correct base/shadow declaration or choices | Pending |
| Computation expressions | Planned: `ce-binding`, F12 | Builder-specific binder/member or unsupported | Pending |
| Script boundaries | Planned: `script-load`, F12 | Ordered load/directive context or explicit unsupported | Pending |
| Signature declaration toggle | Planned: `signature-toggle`, F12 on `.fsi` declaration | Paired implementation target under policy | Pending |
| Selected linked context | Planned: `selected-linked`, actual tree context then F12 | Selected branch; forbid opposite | Pending |
| Module alias capture | `module-alias-captures-original-owner`, F12 | Captured original owner; forbid later nested homonym | Pass |
| Recursive group scope | `recursive-self-binding`, F12 | Local recursive self, forbid outer value | Pass for self-recursion; mutual groups pending |
| Quoted/Unicode identifiers | `quoted-unicode-callback-binding`, F12 | Full quoted literal after astral prefix; forbid callback homonym | Pass; encoding contract, not another binding rule |
| In-flight cancellation | Planned: `pending-cancel`, F12 then Escape while proven pending | No later jump/offer/picker; next request works | Pending; idle Escape is not this test |

## Public feature coverage

Statuses below distinguish actual packaged actions from supporting native and
Extension Host checks. Provider checks alone are not E2E passes. The new
[References observer](../extension/test/e2e/referenceUi.cjs) is included in the
runner's immutable input snapshot/hash set.

| Behavior | Planned fixture / action | Oracle and negative obligation | Status |
| --- | --- | --- | --- |
| Sidebar Find All References | `references-used-once-sidebar`, actual Shift+Alt+F12 | Exact declaration + use set; click every row, exclude homonyms | Pass |
| Shift+F12 | `references-used-once-peek`, actual shortcut inside declaration | Observed jump to sole use; full end-range secondary, retry flags source-derived | Pass within that boundary |
| Unused binder / zero semantic uses | `references-unused-sidebar` and `references-unused-peek` | Declaration-only result, actual movement from interior caret; no incomplete warning | Pass; not an empty native UI claim |
| Incomplete cross-file references | `references-incomplete-explicit-possible`, actual sidebar shortcut | Empty native result view plus scope-not-searched warning | Pass; stable uncertified context, not pending-discovery timing |
| Possible References action | Same fixture, click warning action | Four reachable physical occurrences including unrelated homonyms; explicit full-range selection | Pass; not semantic completeness |
| Repeats and currentness | Required `references-possible-post-edit-currentness`; separate native repeat regression | Current rendered/clicked ranges, removal and captured-command refusal | Possible References shift passes in four fixed contexts with same-row trusted clicks; native stale-coordinate failure remains. Linked/delete/rebind remain pending |
| Reference cancellation/restart | Same fixture, actual pending cancel/restart | No late stale publication/navigation | Pending; sidebar non-cancellable token differs from Shift+F12 |
| External library hover | Separate `hover-core-list-map` and `hover-framework-string-test`, Show Hover | Pinned name and discriminating signature/docs in actual widget | Fresh prepared run: framework documentation passes; List.map owner proof fails. Original generation remains blocked |
| Hover suppression | `hover-local-parameter-suppressed`; `hover-unprepared-homonym-unavailable`; separate prepared framework homonym | Fresh exact workspace suppression versus explicit unknown/unavailable; no speculative widget | Ordinary two pass; fresh prepared collision passes; stale catalog pending |
| Hover restart | Same fixture, restart then hover | Current identity/content; no cached wrong owner | Pending |
| Project compile order | `project-tree-compile-order-linked-path`, pointer expansion | Exact rendered nonalphabetic order, repeated folders and linked display path | Pass |
| Tree file actions | Same fixture, click each of five rows | Each click changes to authored physical URI; no transient unrelated file | Pass for file opens; subsequent context-sensitive F12 pending |
| Tree linked context | `linked-tree`, select duplicate instance | Correct selected project and navigation context | Pending |
| Tree changes/discovery | Same fixture, rename/project edit/restart | Current order/paths; incomplete discovery not represented as complete | Pending |

Relevant supporting suites include `extension/test/suite.cjs`,
`extension/test/nativeAdapter.test.cjs`, `extension/test/adapterContract.test.cjs`,
and Rust `typeflow.rs`, `treesitter.rs`, `possible_references.rs`, and
`solution_tree.rs`. None establishes the pending rendered behaviors by itself.

## References result interpretation

Sidebar exact sets are checked through every rendered row's highlighted literal
and actual clicked URI/start caret. The full single-line UTF-16 range is
reconstructed from that caret and highlight length. Compact Shift+F12 displays
no row in these one-result cases: its actual caret movement is observed, while
full end ranges are secondary native evidence. Homonyms in other lambda scopes
are absent from exact sets. An unused binder contributes its declaration, so
this is zero uses, not an empty native result panel.

The incomplete case visibly retains the native “No results” wording alongside
the explicit warning. The warning explains that the scope was not fully searched
and that “No references found” is not evidence of no uses; it does not change
VS Code's native wording. The separate deliberate action shows the four labelled
physical occurrences, including two unrelated homonym occurrences, and selects
the known consumer use. Internal diagnostic `lastResult` partial locations are
not provider exports. Readiness is bounded observed-state polling, not a fixed
sleep. No timing, memory, pending-discovery, dirty-References or cancellation
claim follows from this slice.

## References fixture failure retained separately

The first References cohort used directly applied lambdas, such as
`(fun value -> value + 1) 20`. Its four exact route contracts are valid language
oracles, but the unchanged engine declined their complete reference domains.
The diagnostic says callable parameter references may include named arguments
outside the body. Source inspection identifies a narrower implementation limit:
`local_lexical::argument_lambda` certifies a lambda passed as an application
argument, not a lambda that is itself the applied function. This is a conservative
coverage gap, not a claim that the source is invalid or that multiline syntax
or Unicode caused the failure.

The authored source and four route contracts are retained in
[reference-proof-backlog.json](../extension/test/e2e/reference-proof-backlog.json).
They are observed unsupported cases, not passing required cases or invisible
provider substitutes. The required UI slice instead uses `List.map` argument
lambdas, with independent declaration/use/homonym oracles. That fixture change
does not fix the original direct-application gap.

The first incomplete cross-file case also failed a harness assertion:
`api.lastResult` is an internal diagnostic payload and can retain partial native
locations even though the provider withholds them. The corrected assertion must
observe the empty native result view and explicit incomplete warning. No exported
partial-result leak was established by the failed internal-payload assertion.
Raw first failures remain preserved; no runtime change was made.

A second run passed all four exact References routes and reached the explicit
Possible References action, but the observer asserted the explanatory message
as soon as rows appeared. The captured view had four reachable rows and an
incomplete title, without the message yet observed. Selection was not exercised
in that failed case. The final observer waits jointly for labelled explanatory
text and all expected rows within the existing bounded wait; it does not add a
sleep or extend the timeout. This observation alone does not establish a runtime
publication defect or prove that the text would never arrive.

## Next checkpoint and release boundary

Keep the signature-candidate repro: its proposed exclusion remains blocked by
unproven signature applicability in raw project context. Do not drop plausible
candidates or add implicit evaluation to make that gate pass. The rendered hover/tree slice is described below; selected-context tree
semantics and stale selection remain unqualified. Reference dirty
edits, real in-flight cancellation and restart remain additional separate gates;
the direct-application proof gap remains in the explicit backlog.
Do not fabricate hover catalog ownership or claim an unwitnessed pending race.

Twenty-four scenarios is a minimum planning floor, not a language completeness
percentage. Keep this parameterized manifest, per-rule oracle review, immutable
package receipts and separate required/known-failure counts. Suggested CI budgets
are targets, not observed timings: native/contracts ≤5 minutes, small packaged
gate ≤10 minutes, broader two-editor nightly ≤30 minutes. Normal-sandbox Ubuntu
clean install/reinstall is a separate unobserved release gate. This cohort does
not establish general speed, memory, daily-use readiness or a qualified new VSIX.

## Observed automation boundary

This checkpoint was validated locally through the protected repository CLI and
the existing disposable packaged-editor route. At the base commit, GitHub
reported zero Actions runs and zero check-runs; the repository has no tracked
`.github` workflow. An empty combined-status response reported `pending`, which
is not evidence of a running CI job. Remote runs/checks are queried again after
publication; absent checks must not be described as CI passing. Normal-sandbox
Ubuntu GUI acceptance remains unobserved.

## Rendered hover/tree slice

This test-only slice extends `db56caf48d8597cb016c1d4577b74b7bc4f0778b`.
The original 17 cases and nine fixtures remain frozen. The ordinary manifest
now has 20 scenarios. The corrected run and its identical frozen-input repeat
each passed 19 required scenarios and reproduced the same scoped signature failure.
Its 22 phase records include the two earlier dirty baselines, not additional
language rules. The package and native executable above remain unchanged.

[Hover observer](../extension/test/e2e/hoverUi.cjs) invokes the supported editor
Show Hover command, proves a fresh response, and inspects the actual visible
widget. A local parameter proves workspace ownership and intentionally returns
no hover. The unprepared workspace module homonym instead returns unknown
ownership/unavailable: absence of speculative library content passes only the
explicit unsupported contract. Its valid exact-ownership expectation remains
in [hover-proof-backlog.json](../extension/test/e2e/hover-proof-backlog.json),
with the original source and first executed manifest digest. This is a
conservative ownership coverage gap, not a false library result or an exact
ownership success. Negative widget assertions are fresh-response snapshots;
late publication, dirty hover and restart remain untested.

[Tree observer](../extension/test/e2e/treeUi.cjs) uses model data only to identify
the authored project. Actual pointer expansion must reveal the exact five-leaf
compile order and nesting, including repeated folder runs and a logical linked
path. Every row is hit-testable and clicked; the active URI must change to its
physical target, including the file outside the App workspace folder. This is
not proof of an active semantic project context, stale-selection refusal or
large-list virtualization. The first attempt failed because the observer saw
the expanded indicator before any children; the corrected predicate waits for
rendered descendants without a fixed sleep or longer timeout.

The separate [prepared manifest](../extension/test/e2e/prepared-hover-manifest.json)
uses genuine pinned DLL/XML catalog, builtin environment and evaluated-model
assets. The two positive contracts require the actual `map` and `IsNullOrEmpty`
names plus discriminating declaration signature and XML documentation text.
A third contract rejects framework content on a proven workspace-owned homonym.
These are explicit prepared-asset cases, not default first-use hover. The
runner checks path-bound inputs before/after and uses disposable index storage;
it does not generate or rewrite certificates. Investigating the first prepared
timeouts reproduced an environment fingerprint rejection in a read-only native
replay: protected CLI and original certificate used different path spellings
for the same SDK. The descriptor can restore only the
certified spelling of that same physical SDK in the isolated process. This
setup failure is preserved and is not evidence of a hover semantic defect.
See [run prerequisites](../extension/test/e2e/README.md#rendered-hover-and-ordered-project-tree);
a clean checkout alone does not include these external assets.

Both prepared editor attempts timed out in all three fresh-response waits. The
SDK correction resolved the explicit environment mismatch in a read-only native
probe but was insufficient in the editor; model/catalog load remained zero.
No positive documentation widget or classified prepared collision result was
observed. The remaining child-context/response blocker is unresolved. Strict
contracts remain opt-in and fail on these setup outcomes; they are neither
expected failures nor passing E2E coverage. No additional setup or runtime
changes were made to obtain a green result.

The subsequent diagnostic identified query-time TEMP certificate rejection,
and the fresh-preparation qualification below separates that setup failure
from a remaining ownership-proof gap. References edits/cancellation/restart and selected tree
context remain separate gaps. The later language-rule slice reaches the 24-passing-case planning floor;
scenario counts do not measure completeness.

## Language-rule slice and union regression

This test-only slice starts at `75080a75dec2533fab4baaf3ddf9ba25e7830322`.
Six independently reviewed exact scenarios were first run together with all
prior cases: **24 passed, one scoped signature failure reproduced, and one
new required union case failed**. That first 26-scenario run remains failed.
It has 28 phase records because of the two existing dirty baselines. No runtime
or runner change was made. The five successful additions cover four binding
rules (tuple slot, match guard, alias capture, self-recursion) plus a quoted
Unicode/UTF-16 contract. An encoding variation is not a fifth binding rule.

All five use actual trusted editor F12 and assert the active URI/start caret,
with full native URI/range as secondary evidence and forbidden homonyms. The
quoted callback uses an astral character before the identifier, so UTF-16
coordinates differ from scalar offsets; its full backtick-delimited target
is checked. This is not symbolic operator coverage.

The valid union use `Parcel.Item 7` must target the Parcel case and exclude the
Other type's identically named case. Instead, the unchanged package returned
no exact locations and included both cases in full and likely candidate lists.
The caret remained at the use. The assertion stopped before opening or checking
the chooser: no candidate reachability or explicit-selection behavior is
established for this failed case, and no wrong direct jump was observed.

Its complete fixture and original strict exact expectation are preserved in
[union-definition-regression.json](../extension/test/e2e/union-definition-regression.json).
This separate runnable regression has no expected-failure or unavailable
substitute and is not part of the passing required count. Its fixture remains
identical to the six-file fixture in the default suite, so isolating the known
failure does not alter the other cases' source context. Run it with the same
pinned inputs and `FSHARP_E2E_MANIFEST` pointing to that manifest. A failed
regression command is expected from the recorded product defect, not a green
feature gate. The earlier signature failure and prepared-hover 0/3 attempts
remain unchanged and separate.

The strongest newly witnessed language gap is qualified union-case identity
in the presence of a same-named case on another type. A separate runtime
correctness checkpoint should diagnose that exact repro before generalizing
to union payloads or broader inference. Other pending families in the matrix
remain necessary; passing a 24-case floor is not language completeness or
daily-use readiness.

The final default run and its identical frozen-input repeat each passed all
**24 required scenarios** and reproduced only
the existing signature failure. Its 27 phase records include the two dirty
baselines, not additional scenarios. The first failed union attempt remains
failed; no runtime, runner or golden answer was changed to achieve this passing
subset. Independent reviewers verified semantics, literal UTF-16 spans and
actual keyboard/caret evidence.

Inventory accounting at this checkpoint: 24 passing default scenarios, two
known failing scenarios (one scoped in the default run, one separate), and
a separate fresh prepared-hover run with two passing checks and one failed
ownership proof. The original three prepared-generation scenarios remain unverified. The
matrix also lists 18 wholly pending rows: 13 definition behaviors and five
public-feature behaviors. Partially covered rows retain their narrower gaps,
such as union payloads, mutual recursion and dirty hover. These row counts are
an inventory, not a fixed denominator for F# support or a coverage percentage.

## Fresh explicit prepared-hover qualification

At source `eb4c69d6a86ef6064f6a3a5c95f796e348fdc9e8`, one new instrumented
Code 1.108.0 run used the unchanged `128140…` evaluation VSIX and fresh genuine
preparation outputs. **Two of three strict checks passed; the suite remained
failed.** The original queries and expected outcomes were unchanged.

| Check | Observed result | Boundary |
| --- | --- | --- |
| Framework `System.String.IsNullOrEmpty` | Exact external ownership; visible expected name, string-to-bool signature and XML documentation | Rendered positive library hover |
| Workspace framework homonym | Exact workspace ownership; null hover and no widget | Proven suppression in this prepared context |
| FSharp.Core `List.map` | Fresh unavailable/unknown ownership; no widget | Strict exact requirement failed; catalog contains documentation, but use ownership was not proven |

The earlier diagnostic had observed successful workspace open and overlays,
then query-time `TEMP` environment-certificate rejection. It did not establish
a rendering defect. This qualification generated new certificates rather than
editing those rejected certificates or weakening validation.

The existing fixture materializer produced the reviewed three-project fixture.
An explicit offline restore used only the pinned SDK-local FSharp.Core package;
`cli/bin/run prepare` used the reviewed existing helper, selected FSharp.Core
and framework assemblies, and exported the complete builtin inventory. No
semantic snapshot or source build was requested. All three project certificates
were reusable. Preparation was separate from the editor's navigation requests.

Private temporary directories were allocated before preparation and retained
for the editor session. A private, temporary test overlay checked all 1,908
distinct environment inputs, including missing values, against both the actual
Extension Host and native process before hover. All matched; all hover requests
used that same native PID. The contributed Solution view established the
session first. Model, catalog and builtin inventory loads were observed, and
editor compiler/helper/decompiler counters stayed zero.

Independent review checked the producer and genuine DLL/XML provenance, source
snapshots, rendered results and all before/after witnesses: 365 file hashes,
90 missing paths and 14 directories. Fresh library provenance differs from the
original generation; no claim is made that the original path-bound assets now
work. Both temporary harness files were restored exactly. The public runner
and its original prepared asset pins are unchanged; this was not a new generic
fresh-preparation runner interface. Raw logs, generated assets and the private
overlay remain local.

There was no repeat, cold-start measurement or uninstrumented performance
comparison. Ordinary 24-pass coverage is retained from its earlier runs, not
rerun here. Normal-sandbox Ubuntu qualification, dirty/restart hover, union
precision and the signature failure remain open. The next narrow hover decision
is to diagnose why the validated inventory/catalog still cannot prove this
`List.map` use's owner, preserving workspace exclusion and all strict oracles.

## References currentness qualification

This test-only checkpoint starts at
`8a5e1c2fd452ff83eb4ab2713ad0814d57b7853b`. Existing deterministic held-response
markers belong to mocked adapter/transport tests. No existing production
request hold/release hook was established for the packaged keyboard route.
Consequently this checkpoint does not qualify in-flight cancellation,
supersession, or publication after a pending native request. Idle picker
dismissal and a timeout are not substitutes.

Two bounded post-edit interaction cases were authored from existing independent
reference oracles. The first full run preserved all 24 required scenarios and
reproduced the scoped signature failure, but both new interactions failed.
The 26 passing phase records include the two existing dirty baselines, not two
additional passing scenarios. No runtime files or package bytes changed.

### Native sidebar stale-coordinate activation: strict regression

`references-native-post-edit-repeat` first invokes actual Shift+Alt+F12 and
clicks both declaration and use rows. A supported unsaved `WorkspaceEdit`
inserts one comment line; the disk remains unchanged. It then issues a second
actual shortcut without clearing native rows. The fresh native response is
exact at document version 2 with ranges `3:34–39` and `3:43–48` (zero-based
UTF-16). Nevertheless, the actual click on the first hit-testable sidebar row
opened **old caret `2:34`**, then the test timed out waiting for `3:34`.
The wrong coordinate is directly recorded; the timeout alone is not the proof.

The observer checked row count, label, highlights and hit testing. Those values
can be identical across a line shift, so they do not establish rendered result
generation. The native `lastResult` export precedes UI completion. This run
cannot distinguish a transient view-replacement race from a persistent native
References-view defect. It must not be relabelled a passed currentness test,
a native-computation error, or a harmless observer-only failure.

The complete original case and oracle are preserved in
[reference-currentness-regression.json](../extension/test/e2e/reference-currentness-regression.json),
a separate strict manifest that is not a required-pass or expected-failure
credit. Its planned second post-edit repeat was never reached. Run it through
the existing protected `test e2e` route with `FSHARP_E2E_MANIFEST` pointing to
that file and fresh private output. No runtime fix, added wait, cleared view
between repeats, or weakened target is used to make it green.

### Independent Possible References interaction

The first attempt at this second interaction was blocked at setup: the initial
clear observer searched only for the new token, so old rows for a different
token survived its check. The preserved failed report shows those rows under
“No results.” This is not evidence of a Possible References computation defect.
The sole correction requires **all native leaf rows absent** after the already
supported initial clear commands. It does not clear rows between the native
regression's post-edit repeats.

The Possible References case requires actual sidebar input, the explicit
incomplete-scope warning action, four labelled physical occurrences, and a
pointer-selected consumer range. After the consumer's unsaved one-line shift,
old custom rows must visibly disappear. The saved command from the previously
shown row is replayed and must not navigate. That replay is a **supported
command-level check**, not an actual stale-row pointer click. A new actual
References shortcut and warning action must render the exact authored physical
set, including unrelated homonyms, and an explicit click must select the
consumer's new full range `2:20–31`, never its former `1:20–31`.

These sets remain incomplete semantic references. Edits use supported API
insertion, not physical typing. This layout-shift case does not qualify
delete/rebind, linked edits, restart, native stale-row refusal or genuine
in-flight cancellation. The qualified-union, signature and prepared List.map
gaps remain separate. Normal-sandbox Ubuntu, Nix, performance and competitor
claims are outside this checkpoint. Raw attempts and packages remain private.

### Final bounded outcome

After the one initial-clear observer correction, the Possible References
interaction passed in isolation: both actual shortcuts and warning actions,
row removal, captured-command refusal and current full-range selection were
observed. In the subsequent full-suite run it failed at the **baseline** row
selection, before any edit: the expected labelled hit-tested row was observed,
but the pointer action produced no selection event and the original editor
caret remained unchanged. This is neither a stale jump nor cancellation proof.
Whether the observed coordinate ceased to be actionable before dispatch or
another selection path failed is unresolved. No further observer correction
or editor run was attempted.

**Zero new interactions are qualified as required passes.** Both original
interaction cases remain in the separate strict regression manifest without
expected-failure metadata or downgraded goldens. The default manifest is
byte-identical to the starting revision: 24 required scenarios plus the scoped
signature failure. Those 24 passed in both full attempts; the two dirty
baselines yield 26 passing phase records per attempt. The new Possible
References interaction has one isolated pass and two failed full-suite attempts
(one initial-clear setup failure, one pre-edit selection failure). The native
interaction has one failed attempt and was not rerun.

Independent review verified literal UTF-16 oracles, frozen prior cases, the
actual stale native caret, the isolated success, and the final no-movement
failure. The highest-value next correctness step is to establish a rendered
reference-row generation and click-delivery witness, then distinguish the
native stale-view lifecycle from the nonrepeatable Possible References
selection. Do not implement a semantic/runtime fix from these incomplete
attributions or claim genuine pending cancellation.

Validation used pinned Code 1.108.0 and unchanged evaluation VSIX SHA-256
`8c3b8d8bb5e2c3ff929a1818f8376be1cecb2516d4f4219e33c6bc155ae17f66`,
with bundled native SHA-256
`2a6f87bee913867f6d4219a20d73e917e527341e7d3c80d6441d89a6c74241f6`.
The original `128140…` preview is unchanged. Final protected aggregate checks
passed: 3 Python, 43 Node tooling/manifest, 323 Rust (5 ignored), and 108
adapter/transport tests, plus TypeScript and formatting. These lower-level
tests do not add editor interaction passes. The restored default manifest was
not launched a third time; its unchanged cases passed in both full attempts
on the same test implementation, with the initial-clear correction in the
second attempt. Private evidence inventory SHA-256:
`6c381f7431868a37fa7424cf45d652098b031ad7c68cb2e3457414ae644af80e`.

## Bounded rendered-row and click witness

A separate diagnostic starts from
`f9deba4edff9392ba5cdd9cfd2611bda5c343a44`. Before execution it fixed one
Code 1.108.0 launch: the unchanged default 25 scenarios followed by the two
unchanged strict currentness regressions, using the same `8c3b8d8b…` VSIX.
There was no second launch, extra sleep, mid-query clear, target change, runtime
change or product-dispatch interception. The no-clear native-repeat oracle
remains strict. **All 24 required scenarios passed; the signature expected
failure and both strict currentness failures reproduced.** The 29 phase records
contain 26 passes, one scoped expected failure and two failures; dirty baselines
are not additional scenarios.

Set `FSHARP_E2E_ROW_WITNESS=1` only for diagnostic runs through the existing
protected `test e2e` route. To reproduce the observed order, create a private
manifest containing the default cases followed by the strict regression cases,
with their unchanged shared fixtures; pass it via `FSHARP_E2E_MANIFEST` and use
a fresh private output directory. This flag does not change the default test
thresholds or click oracle. Reports contain fixture paths and must stay private.

The test-side observer assigns WeakMap identities to DOM nodes, snapshots row
attributes/rectangles, observes relevant subtree mutations, and records trusted
`mousedown`, `mouseup` and `click` targets even outside reference panes. It reads
existing Possible References row IDs/commands without invoking or replacing
dispatch. Limits are 256 events per phase, 100 rows per pane, 20 mutations per
batch, 10 added/removed nodes per mutation side and 2,048 characters per captured
text/attribute value. Event, batch, row and node-list drop/truncation flags
accompany records; text/attribute values have no separate truncation flags.
All timestamps used for ordering are from the renderer clock; there is no cross-clock
subtraction or user-facing latency claim. Observations and layout reads can
influence timing. DOM identity is not provider command generation.

### Native References: stale activation confirmed, persistence unresolved

The first post-edit response again contained the correct version-2 ranges on
line 3. The intended first row received trusted down, up and click events on
the same DOM node and hit target. The editor then opened **old caret `2:34`**.
Thus this occurrence was not a missed or displaced pointer action. Later row
attribute/child updates were observed within the existing assertion window.
The row nodes were reused and their labels were unchanged, so those updates
do not expose the command's captured range or prove when it became current.
No second click was made after that update. A transient refresh window versus
a persistently stale command remains **unresolved**.

Independent review of pinned VS Code's builtin References implementation found
that each row command captures a returned location's range, while preview text
is read separately from a document. The builtin does not assign an explicit
generation ID or perform click-time native revalidation. The bundled source
SHA-256 is `ce9546e5d5559e3670d7fc39866f3a3b1866cc883fcb479b2c6baf70e7cae749`.
This explains why row labels cannot certify freshness, but it does not establish
which publication/cache layer owns the observed stale command. No runtime fix
is justified from this evidence alone.

### Possible References: pointer sequence crossed moving rows

At the pre-edit consumer selection, trusted mouse-down landed on the intended
Consumer row. At the same screen coordinates, mouse-up landed on an Other row
after the layout moved; the click event targeted their common list container,
not a row. No editor selection event or navigation followed. This **positively
localizes this attempt to an unstable pointer target during layout**, rather
than proving a rejected current-row command or a freshness-validation defect.
It does not retroactively prove the cause of the earlier unwitnessed failure.

Navigator's Possible References rows have generation-qualified IDs and perform
currentness validation before opening. The DOM snapshot did not expose the
provider's generation ID, so it does not add command-dispatch proof. The
smallest separately testable harness proposal is to require a fresh intended
row/rectangle at dispatch and verify trusted down/up/click delivery to that row.
A displaced gesture must remain a failed interaction, never silently retried
or counted as successful selection. This proposal was not implemented or run
after the fixed cohort; no product behavior was changed.

### Stop and retained gates

The failed native phase recorded 17 events and the failed Possible References
phase 32, with zero dropped events and no row/node-list truncation. Each had
one truncated mutation batch. Positive pointer/selection evidence remains
usable; an exhaustive lifecycle or absence claim does not. The bounded
diagnostic stops here, with **zero new required passes** and both original
strict oracles intact. Genuine in-flight cancellation and the signature,
qualified-union and prepared List.map gaps remain open.

Protected aggregate checks passed: 3 Python, 43 Node tooling/manifest, 323 Rust
(5 ignored), 108 adapter/transport, TypeScript and formatting. Independent
review checked the oracle and event interpretation. The normal-sandbox Ubuntu
acceptance gate remains separate; no performance or CI qualification is claimed.
Protocol SHA-256: `6da052c987487a2104dd1c6f9d06747e8a0469654bd68d4305724a0dd7bbb2bb`.
Private 73-file evidence inventory SHA-256:
`a10a1f05dc57a9e3dd5e45ccabe9a0971b1ddcce5685b33999fbecb6d8902fd1`.

## Possible References pointer qualification

From `aea0756832567e8febcefc4e1b47ca6369e473c2`, one test-only variant addressed
the previously observed pointer sequence crossing moving rows. The original
semantic/physical target oracle was unchanged. The old failures remain in
private evidence and the preceding sections; they are not retroactively passes.
The native stale-coordinate case was not run or modified in this checkpoint.

Only the `possible-post-edit-command` interaction uses the new boundary. Within
the existing five-second possible-row readiness deadline, it requires the full
authored row set and incomplete disclaimer, a connected exact path/label target,
a live hit on that row, and identical node/text/attributes/rectangle across
three consecutive animation-frame observations. Running or pending animations
in the pane subtree or its ancestors prevent qualification. The final live
snapshot must still match. This is an observed recent layout boundary, not a
guarantee against later layout work or proof of provider generation.

The test uses the fresh target point once. Trusted mouse-down, mouse-up and
click must all reach the same intended row and hit target, followed by the
original URI/full-range assertion. A displaced click fails; there is no silent
retry, alternate command, extra fixed sleep, extended deadline, or relaxed
target. The existing ordinary Possible References route is unchanged. Runtime
and VSIX bytes are unchanged.

Before results, the schedule was frozen as **isolated, full-order, full-order,
isolated**, each in a fresh disposable Code 1.108.0 profile. Full-order means
the prior default 25 scenarios followed by this exact interaction; the native
regression is separate. All four used identical harness, fixture and package
bytes, with the unchanged `8c3b8d8b…` evaluation VSIX. The stop rule was first
failure with no replacement run or second variant. All four completed green.

| Observed contract | Result |
| --- | --- |
| New interaction across fixed contexts | 4/4 passed |
| Baseline and edited target gestures | 8/8 trusted down/up/click on the intended row, then correct full range |
| Baseline consumer range | Zero-based UTF-16 `1:20–31` |
| Unsaved one-line-shift consumer range | `2:20–31`; previous target absent from refreshed set |
| Edit identity and cleanup | New document version, unchanged disk, rendered custom rows removed, clean revert in each run |
| Prior required scenarios in each full-order run | 24/24 passed; signature expected failure reproduced |

Each full-order run has **25 required passes plus the scoped signature failure**,
or 27 passing phase records plus one expected-failure phase because two existing
cases have dirty baselines. This is one newly qualified interaction, not a new
semantic binding rule. Both native and Possible References still expose their
previously documented completeness boundaries.

The default manifest now appends only the qualified case. Its fixture, query,
expected set and edit oracle equal the original strict case. The historical
strict manifest remains unchanged, including the native failing regression;
its copy of this case is not a second coverage credit. Independent review
checked all eight stability receipts and trusted pointer sequences, original
UTF-16 targets, current dirty versions, row removal and frozen inputs.

Captured old-row command replay remains a separate adapter-level check: after
visible invalidation it did not navigate. It is **not** an actual stale-row
pointer click, an edit during acceptance, or cancellation while native work is
pending. This qualification also does not cover linked/delete/rebind/restart,
every layout, other editor versions, or a general click-success rate. Layout
observation may influence timing; no speed result is claimed.

The native next step remains separate: determine the captured row-command range
after an observed builtin refresh and a separately identified deliberate
activation, retaining the first stale activation as a failure. Neither DOM-node
reuse nor current native output establishes that command's freshness. No native
fix or additional instrumentation is implemented here. Normal-sandbox Ubuntu
acceptance remains pending a suitable supported desktop.

The promoted tree passed protected aggregate checks: 3 Python, 43 Node
tooling/manifest, 323 Rust (5 ignored), 108 adapter/transport tests, TypeScript
and formatting. No extra editor run followed promotion: the promoted case and
harness are the same ones used in the fixed cohort. Package SHA-256:
`8c3b8d8bb5e2c3ff929a1818f8376be1cecb2516d4f4219e33c6bc155ae17f66`.
Protocol SHA-256:
`bc8e09f8a458faff1821d7185d7a1b02ae0b62be03bfe25296b82134ed7f15be`.
Private 263-file evidence inventory SHA-256:
`9e28646c15d18183e9f8c89625a0617e3ba076e1fc52a6ebb421c50315199739`.
No binaries, profiles or raw reports are published.

## Native References isolation follow-up

The [single paired isolation checkpoint](native-references-isolation.md) from
`4cabef8c452d71b40c3a43ba82c27546e69ebc04` reproduced the strict stale activation
with Navigator and an independent public reference provider without Navigator
installed. This adds no passing coverage. The correct provider return does not
expose the selected tree item's captured command generation; the exact editor
publication boundary and persistence remain unproven. Both failures and the
unchanged no-clear oracle are retained. No further passive diagnostic loop or
product change is justified without a specific separately testable remedy.

## Active-pattern, operator and generic-record slice

This test-only slice starts from
`33a3262bf3468cdd20a50e0c577ae0263deb7855`. An independent author supplied exactly
three valid F# fixtures and literal UTF-16 target oracles; an independent
reviewer checked their language rules, negative targets and real-editor
assertions. Primary Microsoft language documentation is linked in each manifest
oracle. No compiler output or engine result generated the golden answers.

The initial pinned Code 1.108.0 run used the prior 26 default scenarios plus
these three strict exact cases. All prior 25 required scenarios passed and the
scoped signature failure reproduced. Of the new cases, **one passed and two
failed**. Its 31 phase records contain 28 passes, one expected failure and two
failures; the two dirty baselines are not additional scenarios.

| Rule and independent oracle | Observed first-run outcome |
| --- | --- |
| Partial active-pattern case: match `Hit` → header `Hit` at `4:6–9`; exclude qualified-only `Other.Hit` and payload binder | Incomplete result; internal candidates include both correct header and forbidden union homonym. No exact locations or direct jump. |
| Scoped custom operator: infix `+++` → own-module declaration at `4:9–12`; exclude unopened sibling operator | Unavailable, with explicit custom-operator/active-pattern semantic-identity reason. No locations, candidates or jump. |
| Generic record substitution: `Envelope<Human>.Payload.Label` → `Human.Label` at `1:15–20`; exclude `Robot.Label` and `Payload` | Exact result and actual URI/caret jump to `1:15`; no chooser. |

All positions above are zero-based UTF-16. The operator query includes an astral
comment before the token, so its authored query offset differs from a Unicode
code-point offset. The operator destination policy requires all three symbol
characters and excludes parentheses. The active-pattern policy targets the
case identifier within the recognizer header, excluding its delimiters. These
policies were fixed before execution, not adjusted to match a result.

Both failures are preserved unchanged in
[language-rule-regression.json](../extension/test/e2e/language-rule-regression.json).
They remain separately runnable strict tests without expected-failure metadata.
Neither was converted into an unavailable pass, rerun with a simpler fixture,
or repaired in runtime. The active-pattern check failed on the internal
forbidden-candidate assertion before exercising a picker; its captured final
UI had no offer/picker and displayed “No definition found.” This does **not**
qualify rendered candidate reachability. The operator's symbolic destination
span remains unqualified because resolution was unavailable. No wrong direct
jump was observed in either failure. These are missing qualifications, not
claims that the engine previously promised exact support for these fixtures.

The generic-record case is the sole new required scenario. Its explicit
receiver annotation requires substitution of `Human` for the record parameter
and then resolution of the final field's nominal owner. This is distinct from
the existing directly typed record-label case. The full returned range is
secondary native evidence; actual keyboard F12 and the active URI/caret provide
the editor proof. It does not cover navigation to the `'T` binder itself,
constraints, higher-order generics or arbitrary inferred receivers.

The original fixtures/cases, scoped signature failure, qualified-union and
native References strict regressions, and prepared List.map gap are preserved.
The native References investigation was not reopened. Runtime and the existing
evaluation package remain unchanged; no preparation, build or compiler is
implicitly invoked by the editor fixtures. This is not normal-sandbox Ubuntu,
performance or daily-use qualification. Raw failed attempts remain private.

The final required-subset run passed **26 required scenarios** and reproduced
the one scoped signature expected failure: 28 passing phase records plus one
expected failure, including two dirty baselines. The generic case passed in
both runs with trusted editor F12; the two new failures were not retried. All
15 final sessions reported zero helper/compiler/decompiler launches. The
reviewer independently checked both reports and unchanged fixture/oracle bytes.

Protected aggregate checks passed: 45 Node tooling/manifest tests, 319 Rust
unit tests plus 4 daemon tests (5 ignored), 108 adapter/transport tests,
TypeScript and formatting. The final manifest SHA-256 is
`151fb08a6a853bc5029cdd6f390cccaa969d9ad4ad4108cdbd7bf9929bcbcd53`.
The unchanged private package SHA-256 is
`8c3b8d8bb5e2c3ff929a1818f8376be1cecb2516d4f4219e33c6bc155ae17f66`.
The private 147-file evidence inventory SHA-256 is
`967091aee41dea40c82a4889370e91d9f0407ae59c50521dd7260d2b7225839d`.
Raw reports, first failures and verification receipts remain local; only
fixtures, integrity checks and sanitized documentation are published.
