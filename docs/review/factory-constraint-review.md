# Factory inference: independent correctness blocker

**Update:** immutable78fd passes an interim literal-only repair gate; all13 valid previously wrong cases are unavailable, with literal positives preserved. The ordinary-call positive and eight real Grace answers are lost. The broader repair contract below is still incomplete; see `factory-literal-review-78fd.md`.

Both immutable 85d1 and 858f return a wrong field as exact on valid F# static-factory code. This is independent of the successful locale certificate and bounded digest-cache changes. No fixed candidate has passed this gate yet.

The engine currently treats a factory body as unconstrained when its type group has no other suspicious occurrence of the factory member name. An inline helper can impose that member's result constraint through a statically resolved type parameter. The member name then occurs in the helper's earlier declaration, outside the factory group. The helper can receive the current factory type through explicit type arguments, an instance receiver, record-label inference or field projection. Therefore absence of same-spelled uses, or of explicit owner type tokens, is insufficient.

`factory-trait-native-source-858f.json` retains a normal successful project build (zero warnings/errors), direct compiler binding and the wrong exact answers from both frozen binaries. `factory-trait-inferred-oracle.json` adds two valid inferred-record forms. The reproducible `factory-constraint-envelope-oracle.py` now records 19 compiler/native controls in `factory-constraint-envelope-oracle.json`:

| Result on immutable 858f | Cases |
| --- | ---: |
| Valid source, wrong exact field | 13 |
| Valid source, exact-correct field | 2 |
| Valid source, unavailable | 1 |
| Compiler-invalid inference boundary, recovered field agrees | 3 |

These controls are correlated adversarial examples, not a statistical coverage estimate. The valid wrong answers include earlier instance `this`, an alias of `this`, static-let cached records, uninvoked lambdas, quotations, unreachable branches, explicit generic partial application and local wrappers. Field projection alone can infer the factory owner; passing `Unchecked.defaultof<_>` or `List.head []` then carries it into the inline constraint. Runtime execution is unnecessary: the constraint applies during typechecking. Recursive getters are never executed by this harness.

The two valid positives preserve a plain constant factory and a factory containing an ordinary unconstraining framework call. Pure unconstrained `defaultof<_>` is a compiler error and is not reported as a valid counterexample. Later outside/sibling constraints also produce errors without retroactively changing the previously selected record field. Their recovered anchors are retained separately.

## Repair review contract

The next implementation needs a bounded, explicitly admitted constraint context. A list of known bad helper names or owner spellings is not such a context. A conservative initial subset can reject unknown calls and expressions, but that rejection must be recorded as missing coverage and limited to facts that depend on this factory inference.

The review will require:

- The admitted syntax and semantic facts are stated positively. Unknown/recovered syntax, omitted type-group members, implicit instance parameters, generic instantiation, record construction/patterns/projections, operators, quotations, closures and computation-expression operations cannot silently count as absent constraints.
- Calls or static properties admitted by the subset have an actual resolved declaration and a justified constraint contribution. A distinct member identity proves only that it is not the current member; it does not prove the surrounding expression cannot constrain the current member through another route. Generic receiver/type arguments must retain their provenance.
- The entire relevant checking group and inference order are accounted for. Local closures are typechecked even when never invoked; completed outside groups and later error-producing calls are not interchangeable with earlier constraints. Explicit implementation annotations and public signature facts retain their separate contexts.
- All 13 valid wrong-result cases cease returning wrong exact results, with the two valid exact positives preserved. Newly unsupported examples remain in the coverage denominator. Additional unseen shapes and mutation tests exercise the admitted boundary before acceptance.
- Reference completeness observes any newly unknown factory-dependent candidate. No empty or partial exact set may result from dropping unresolved receiver evidence.

`distinct-static-owner-oracle.json` separately verifies 14 owner/inference controls, including alias, inherited member, recursive-group and missing-member recovery cases. Those checks help validate a disjoint-identity subroutine; they do not override this factory constraint failure. No formal proof or language-wide correctness claim follows from either finite test set.
