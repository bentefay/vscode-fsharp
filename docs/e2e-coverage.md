# Packaged editor coverage and remaining gates

The References slice extends source checkpoint
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
| Tuple/destructuring slots | Planned: `tuple-slot`, F12 | Correct binder, forbid sibling | Pending |
| Match/guard lifetime | Planned: `clause-scope`, F12 | Own clause binder, forbid adjacent clause | Pending |
| Union case/field | Planned: `union-case`, F12 at construction/pattern use | Correct case or field declaration; reject homonym | Pending |
| Active pattern | Planned: `active-pattern`, F12 at recognizer/case use | Correct declaration or explicit unsupported; reject union homonym | Pending |
| Symbolic/custom operator | Planned: `operator-scope`, F12 | Full symbolic span, reject builtin/unrelated | Pending; literal integrity only |
| Type parameter binding | Planned: `type-parameter`, F12 | Binder, forbid nominal homonym | Pending |
| Generic receiver substitution | Planned: `generic-record`, F12 | Instantiated field owner or labelled alternatives | Pending |
| Overload resolution | Planned: `overload`, F12 | Signature-correct target or all plausible choices | Pending |
| Extension visibility | Planned: `extension-scope`, F12 | Applicable intrinsic/optional extension or safe refusal | Pending |
| Interface dispatch | Planned: `interface-slot`, F12 | Explicit slot/implementation target policy | Pending |
| Inheritance and shadow | Planned: `inherited-member`, F12 | Correct base/shadow declaration or choices | Pending |
| Computation expressions | Planned: `ce-binding`, F12 | Builder-specific binder/member or unsupported | Pending |
| Script boundaries | Planned: `script-load`, F12 | Ordered load/directive context or explicit unsupported | Pending |
| Signature declaration toggle | Planned: `signature-toggle`, F12 on `.fsi` declaration | Paired implementation target under policy | Pending |
| Selected linked context | Planned: `selected-linked`, actual tree context then F12 | Selected branch; forbid opposite | Pending |
| Module alias capture | Planned: `alias-capture`, F12 | Captured target, forbid later-open capture | Pending |
| Recursive group scope | Planned: `recursive-group`, F12 | Correct lifetime, forbid unrelated later binding | Pending |
| Quoted/Unicode identifiers | Planned: `identifier-span`, F12 | Exact UTF-16 literal span at a real use | Pending; not a new binding rule |
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
| Repeats and currentness | `reference-edits`, linked edit/delete/rebind, repeated action | Deduplication, updated rows, stale rows cannot navigate | Pending |
| Reference cancellation/restart | Same fixture, actual pending cancel/restart | No late stale publication/navigation | Pending; sidebar non-cancellable token differs from Shift+F12 |
| External library hover | `external-docs`, rendered Show Hover | Reviewed catalog identity, exact signature/docs | Pending; needs legitimate catalog assets, no implicit preparation |
| Hover suppression | `hover-collision`, actual hover | No library content on source collision/missing or stale catalog | Pending |
| Hover restart | Same fixture, restart then hover | Current identity/content; no cached wrong owner | Pending |
| Project compile order | `ordered-tree`, open/expand view | Visible nonalphabetic compile order, including repeated folders | Pending |
| Tree file actions | Same fixture, actual row click | Correct active file URI, then real F12 | Pending |
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
candidates or add implicit evaluation to make that gate pass. The next test slice
is rendered library-only hover and ordered tree/file actions. Reference dirty
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
