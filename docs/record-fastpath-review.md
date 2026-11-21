# Independent review of record navigation fast paths

This is a soundness review of the next native implementation, not a passing performance or parity result. The previously accepted experimental binary was `7ca9b1dd88412cb025ec6f999ff5dfffb874e3d5e1aba63860b996ec6f4da78f`: native navigation passed 17/29 goldens, explicit model/catalog preparation 20/29, and a clean compiler-produced snapshot 29/29. After relevant edits, the snapshot's five tested reference queries were unavailable. A fast unavailable answer does not meet the requirement.

## What can be proved cheaply

F# already uses scoped record labels to infer an otherwise unconstrained receiver. The opportunity is to implement that rule with enough context to prove it applies. Global uniqueness of a spelling is insufficient.

| Query situation | A sufficient native proof | Required rejection or extra work |
|---|---|---|
| `(p: Person).Name` | Resolve the exact `Person` declaration in the annotation's scope; find its accessible `Name` field. | A later opened class/type alias with the same type name must block fallback to an earlier record. |
| `let alias = original; alias.Name` | Follow the actual binding identity to a proven receiver type. | A missing type fact is not a fresh compiler type variable. Detect real cycles separately from a bounded long valid chain. |
| `{ Person.Name = value }` | Qualified label identifies an accessible record owner in this scope. | Respect expected type, representation visibility, `.fsi` exposure, and every other supplied label. |
| `let value = { Name = n; Home = h }` | Fresh nonrecursive direct initializer, no expected type; intersect ordered label candidates. A complete local record declared after every active import can dominate older candidates. | Track explicit and AutoOpen contributions, current/ancestor lexical scopes, accessibility and RequireQualifiedAccess. Initially defer multiple applicable local candidates and unsupported context. |
| `{ original with Name = n }` | Infer `original`'s exact record type, then resolve update labels within it. | Do not infer a new owner from a globally unique update label. |
| `let get p = p.UniqueLabel` | Fresh unannotated formal; body exactly the access; no expected function/member/recursive/SRTP constraints; prove the head of the applicable record-label environment. | Do not apply to `let get p = consumeAsString p; p.Length`, a known factory result, or an arbitrary unknown receiver. |
| `box.Value.Name` with `box: Box<Person>` | Resolve `Box.Value`, substitute `Person` for the field's type parameter, then resolve `Person.Name`. | Generic arguments need their original scope/provenance. `Box<Company>` must resolve the second access differently. |
| Complete references of a field | Resolve every conservatively possible occurrence to its receiver/constructor/update/pattern owner; compare declaration identities. | Candidate parsing/type proof failures make the reference result incomplete. Lexically same field names on another record are excluded only after proof. |

Local declaration identities and explicit snapshot/context certificates are the foundation. A type or field spelling alone is never an identity. A new declaration or import that could change a former negative lookup must invalidate the proof.

## Compiler evidence

Inspected primary source: `dotnet/fsharp`, commit `f896797a0f9b238a8cb1c482be424b714037ddc5`.

- `src/Compiler/Checking/NameResolution.fs`, `ResolveExprDotLongIdent`: resolve through the known receiver type first. Only when that resolution fails and the receiver is not a named/applied type does the compiler use the first scoped record-label candidate.
- The same file, `AddStaticPartsOfTyconRefToNameEnv` and `AddRecdField`: record fields enter an ordered label environment; RequireQualifiedAccess affects unqualified availability.
- `src/Compiler/Checking/Expressions/CheckExpressions.fs`, `BuildFieldMap`: record construction intersects candidates for all supplied labels. A singleton identifies the owner; ambiguity is narrowed by complete field-count matches and ordered candidates. This is not an unordered global unique-name rule.
- `src/Compiler/Utilities/lib.fs`, `ListSet.intersect`: candidate order is preserved from the second input. A local type that leads every supplied label list can therefore dominate older imported candidates, provided no later import or scope contribution has been missed.
- Language documentation: <https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/records> and <https://fsharp.github.io/fslang-spec/inference-procedures/>. Source inspection and actual compiler fixtures are required for edge conditions beyond the overview.

Actual FSI counterexamples confirmed that a String-producing factory's `.Length` remains a String member despite a local record's unique `Length`; later record labels select the later record for an unconstrained formal; explicit annotations override such label inference; aliases and copy/update preserve the annotated owner; generic nested access substitutes the receiver argument; a known record's extension property can take precedence over another record's label; anonymous record fields are not nominal record fields. Temporary raw evidence was `/tmp/fsharp-record-proof-volola8o/results.json`; independent native replay evidence must be copied into the durable review report before acceptance.

## Complexity and editing requirements

A supported local access can need one source parse and a short type-constraint walk, with compact on-disk declaration facts for other files. Memoize by current declaration identity, detect cycles with an active set, and bound total proof work. Reuse the current parsed summary rather than cloning it at every alias step. A depth cap is a resource limit, not evidence of a type cycle.

No algorithm can guarantee fixed millisecond latency for every F# reference query: the dependency slice, candidate domain and returned result set can all grow. Measure common paths and tail cases separately. Report unsupported coverage and every incomplete operation in the denominator. Native editing must be evaluated without silently using a stale precomputed compiler snapshot.

## Dependency certificates after ordinary builds

Project membership/order, source binding facts and external library signatures have different dependencies:

1. Project shape depends on project/import/assets/configuration inputs, evaluated compile order and selected reference bindings. An unchanged-path rebuild of a workspace DLL should not alone invalidate this shape.
2. Source binding/type facts depend on current source snapshots, relevant project context and any external signatures actually consumed. DLL-backed semantic exports retain binary dependencies; splitting shape certificates must not weaken those.
3. A library catalog depends on exported assembly bytes, selected transitive dependencies, documentation and the formatter/compiler version. An unrelated workspace DLL supplied among the compiler reference options should not automatically become a dependency.

The metadata helper must follow actual compiler-selected assembly bindings, not guess from file basenames. Framework version unification is real: retain requested and selected identities and selected bytes. PE AssemblyRef closure covers CLR dependencies and forwarder targets, but F# signature resources have their own serialized CCU-name table. Signature-only erased dependencies need an actual test and, if absent from the PE graph, a separately certified F# type/signature dependency walk. A formatting failure cannot justify silently certifying an incomplete signature.

Whole-catalog invalidation remains conservative if one genuinely dependent shard changes. Per-shard certificates could improve this later. Missing-to-present inputs, imported binding changes and custom MSBuild targets still need reconciliation; merely excluding all workspace DLLs would be unsound.

## Dense literal arrays

The proposed syntax projection recognizes only long arrays of decimal Int32 constants separated by semicolons/whitespace. It retains byte/newline offsets, the first literal and explicit elided-span provenance. Strings, comments, escaped identifiers, suffixes, units, expressions, names and unsupported lexical states are excluded.

An actual FSI probe confirmed `-1` is a signed constant while `- 1` can invoke a user-defined unary operator. Therefore whitespace between sign and digits must prevent elision. Array cardinality/value facts, pattern reachability and any dependent inference must never be learned from the projected contents. Position/UTF-16 conversion uses original source. Projection reduces parser input structure; it does not establish successful navigation, bounded whole-query memory, or the runtime latency target until measured end to end.

## Acceptance still required

Replay independent adversarial cases on a frozen new binary, including dirty receiver/type/field edits, offline edits, scope/AutoOpen/type shadows, long aliases, generic nested fields, expected-type constraints, source-owned hover, complete references, parser-budget failures and dependency mutations. Report native correctness and edited coverage separately from optional prepared snapshots. Count all setup/query processes. Actual VS Code activation/provider behavior and matched Ionide/Rider comparisons remain separate gates.

## Development review findings

Candidate `86b72c873668df26001e3e9728b849b2a41f050e636e3ab4b67fc9cfceea7eae` initially returned 11 correct exact definitions and nine unavailable results in 20 independent probes. All 20 source snippets compiled under the installed FSI when isolated in nested modules. Additional targeted probes then found two real false-exact families, so this candidate is rejected pending fixes. Raw sources, responses and compiler checks are preserved in `docs/review-record-development.json`.

1. **Cross-file opened types.** An earlier `Shared.Person` record and later `open Other` in another source file caused the native annotation resolver to use `Shared.Person` rather than `Shared.Other.Person`. Actual FSI execution confirmed the latter. The same problem affected a function's annotated return type and a module alias.
2. **Persistent freshness.** With the daemon stopped, a same-length `A.value` to `A.other` edit with the original mtime restored produced stale exact references after restart. The inverse edit from a previously negative file produced a stale exact empty result. Validating only old candidates cannot fix the addition case. Keep startup source reads at zero, but validate content before reusing persisted relevant-domain certificates; retain a session-local validation record and handle watcher gaps/errors explicitly.

A proposed nearest-module-only fix was disproved by the compiler. If `Shared.Use.Other` contains only `Widget`, while `Shared.Other` contains `Person`, `open Other` still makes the latter `Person` available. Module abbreviations preserve the same merged environment. `NameResolution.ResolveLongIdentAsModuleOrNamespace` collects all applicable module references; `AddModuleOrNamespaceRefsContentsToNameEnv` uses `foldBack` with most recently introduced modules first. `AddModuleAbbrevToNameEnv` prepends the new module-reference list to the prior list. Tests must preserve this ordered merged behavior rather than encode the rejected nearest-only hypothesis.

The metadata certificate review also established an actual missing dependency: an exported `float<Measures.kg>` signature did not place `Measures.dll` in the PE AssemblyRef graph. The helper owner added an FCS signature/type/constraint dependency walk and a real mutation regression. That fix still requires final independent integration replay; PE-only closure is no longer an acceptable certificate.
