# Operator declaration name-absence review

The proposed optimization is acceptable as a **narrow negative-name fact**, subject to the producer and consumer gates below. It is not a proof that the operator's inferred types, effects or references are understood. No new native behavior is accepted by this note alone.

The actual blocker is a recognized custom operator in Grace.Shared/Utilities.Shared.fs. Its existing unnamed uncertainty reaches the module's visibility end and prevents the engine from proving that a later `nameof` refers to the captured Core intrinsic. The useful additional fact is: this particular declaration cannot introduce the ordinary binding name being queried.

## Actual compiler controls

`operator-name-absence-oracle.json` records eleven successful FSI programs and independent FCS symbol/goto/reference queries. The script is `operator-name-absence-oracle.py`.

| Control | Actual result |
| --- | --- |
| Ordinary `let (>>=) x f = f x` | Later `nameof marker` uses Core's intrinsic. |
| Operator parameter named `nameof` | The parameter does not escape; the later call is intrinsic. |
| Escaped local ``nameof`` inside the operator body | The local binding does not escape; the later call is intrinsic. |
| Recursive `let rec (>>=) ... and nameof ...` | The later call resolves to the exported custom function. |
| Same recursive group with escaped ``nameof`` | It also resolves to the custom function. |
| Operator with `[<CompiledName("nameof")>]` | The compiled name does not create an ordinary source `nameof` in this test. Rejecting attributed declarations initially remains conservative. |
| Earlier parent `nameof`, then opening `CustomOps` | The parent custom function remains selected. |
| Opened module exports its own `nameof` | The imported custom function is selected. |
| Recursive module exports `nameof` | The imported custom function is selected. |
| Unopened module exports `nameof` | The later call remains intrinsic. |
| Earlier parent `nameof`, then opening a source module named `Operators` | Core's merged Operators contribution restores the intrinsic. A name set alone cannot choose the winner. |

The first diagnostic used unannotated recursive return types and failed FS0072 at an early member lookup; those invalid programs are retained in `operator-name-absence-initial-diagnostic.json` and are not positive expectations. Explicit `: string` return annotations make both recursive controls valid.

## Required proof boundary

- Retain the existing unnamed uncertainty and its visibility extent. Attach the optional name set to the declaration that caused it; do not replace that uncertainty with general supported status or shorten its scope to the declaration's end.
- Collect a conservative superset of all identifier/operator spellings in an error-free recognized declaration. Normalize escaped identifiers consistently with queried names. A body-local name may unnecessarily block this first implementation; that is safe and distinct from declaring it exported.
- Refuse recursive groups initially, or certify the entire group, including later `and` binders. Certifying only the first operator left-hand side would miss the demonstrated ordinary-name export.
- Refuse attached attributes, unknown/recovered syntax, implicit builder contributions, module/open/type contributions and budget overflow unless each has its own complete proof. Inspect the actual attribute attachment, not only the function body.
- The first consumer may discharge only the queried **ordinary** name's absence for this specific barrier. Generated `op_...` aliases and operator-resolution rules remain outside this certificate.
- Keep all other source declarations, scope/order checks, inventory identity and remaining unnamed barriers active. Neither absence from one operator nor a globally unique spelling establishes the intrinsic winner.
- Keep the certificate in the parser summary/fingerprint and invalidate it with source/context changes. It must not independently enable return inference, label resolution, or complete references.

Required native controls are the unchanged ordinary positive, both recursive exported-name negatives, escaped spelling, unrelated and relevant local scopes, unknown/budget paths, attributes, and the exact Grace occurrence that motivated the change. A missing result remains a coverage miss; a fast wrong exact result blocks acceptance.

## Structural function-body boundary

The following actual Grace barrier is a task `let!`/`return!` inside an ordinary completed function. A structural distinction is preferable to an accumulating list of expression-specific exemptions: local lexical binders in that function body do not become exports of its enclosing module. This remains true when their types or builder semantics are not understood.

`function-binding-boundary-oracle.json` supplies ten successful FSI/FCS controls. An unrelated task function, task-local `nameof` (plain and escaped), local recursive functions, a body-local tuple pattern, and an opened or AutoOpen module containing such a function all leave the outside `nameof` intrinsic intact. Conversely, an AutoOpen module's top-level `nameof` header, a module-level tuple pattern binding `nameof`, and a recursive group's later `and nameof` header select the custom function. Function-body isolation must not erase these exported declarations.

A possible implementation is a certified ordinary-function body range. Only an uncertainty fully contained in that range may be disregarded as an **exported ordinary-name contribution**, and only when the lookup is outside the body or in another file. Preserve the original uncertainty for local queries, type inference, labels, general navigation and references. A header-spanning operator uncertainty is not body-contained and still requires its separate narrow certificate. The parser must establish a trustworthy body/group boundary without recovery swallowing another declaration; all group headers, module attributes/imports and accessibility remain separate proof inputs.

The current operator producer and intrinsic consumer were source-reviewed after the eleven compiler controls: the producer retains the original span, bounds its normalized-token superset and refuses the stated unknown forms; the consumer uses it only for ordinary `nameof` absence. No new defect was found in that envelope. Frozen native replay and the proposed structural function-body implementation remain separate gates.

## Frozen native replay

The independent84-query edit fixture combines all21 compiler controls with actual alias/nameof definitions and complete reference sets, a genuine f215 model/environment and versioned source overlays. Frozen f27d and b6bc both return33 exact-correct and51 unavailable results, with0 wrong (`name-boundary-{f27d,b6bc}.json`). Same-file broad inference uncertainty remains deliberately active, so this is a safety regression, not a coverage gain.

The complementary split-file fixture places the operator/function body in an earlier file and the alias/nameof consumer in a later file, matching the actual Grace exported-name proof. All six variants are independently FCS-validated. With separate caches and sequential immutable replays, f27d returns6/24 correct and18 unavailable; b6bc returns15/24 correct and9 unavailable, with0 wrong in either run (`name-boundary-split-{f27d,b6bc}.json`). All three positive operator/task/local-binding contexts now have exact definitions and complete references. Exported-name, recursive-group and top-level-pattern negative contexts retain unavailable operations; they are not counted as full coverage. Exact results match requested overlay versions.

An initial pair accidentally shared one writable cache concurrently; those runs are explicitly retained as excluded diagnostics. The reported comparison uses separate per-binary caches and sequential reruns. A source review also caught synthetic inline-fragment body scopes being copied without coordinate remapping; the core owner corrected that before b6bc and supplied a mutation-sensitive parser regression. This is scoped acceptance of the name-proof change, not overall acceptance: b6bc retains the independent v3 project-culture certificate blocker.
