# Packaged editor coverage and remaining gates

This inventory extends source checkpoint `d203de6763b8353c2d647b61dd0f3ed3528b4460`.
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

## Public feature behavior still to add

These rows are pending in this shared packaged manifest. Existing native and
Extension Host checks are supporting evidence only, not invisible E2E passes.

| Behavior | Planned fixture / action | Oracle and negative obligation | Status |
| --- | --- | --- | --- |
| Sidebar Find All References | `local-references`, actual menu/palette action | Rendered declaration + use URI/range set, click each | Pending |
| Shift+F12 | Same fixture, keyboard from declaration | Correct compact use/retry behavior; not already-at-target | Pending |
| Proven empty use scope | `unused-formal`, both entry points | No incomplete warning; distinguish declaration row from zero uses | Pending; exclude-declaration API check alone is not empty UI proof |
| Incomplete cross-file references | `pending-references`, action before witnessed readiness | No partial native set as complete; explicit scope-not-searched warning | Pending |
| Possible References action | Same fixture, click warning action | Labelled incomplete set, one deliberate action, no automatic search | Pending |
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

## Next checkpoint and release boundary

First decide on the narrowly scoped signature-candidate fix using the retained
failing repro. The next test implementation should cover native References in
both entry points with one local use, an unused binder and an incomplete
cross-file scope, followed by rendered hover and ordered tree/file actions.
Do not fabricate hover catalog ownership or claim an unwitnessed pending race.

Twenty-four scenarios is a minimum planning floor, not a language completeness
percentage. Keep this parameterized manifest, per-rule oracle review, immutable
package receipts and separate required/known-failure counts. Suggested CI budgets
are targets, not observed timings: native/contracts ≤5 minutes, small packaged
gate ≤10 minutes, broader two-editor nightly ≤30 minutes. Normal-sandbox Ubuntu
clean install/reinstall is a separate unobserved release gate. This cohort does
not establish general speed, memory, daily-use readiness or a qualified new VSIX.
