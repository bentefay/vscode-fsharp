# Independent review: interim literal-factory repair

**The narrow repair passes its independent regression gate. Broader factory navigation remains incomplete.** Reviewed native SHA256 is `78fd26ebfd67e709131b87ad457783a3cd97fffcd722f670fe8c8f854a09ae9b`, from captured source `cdb472a6b21d`. It refuses all 13 valid factory cases that returned wrong exact fields on 858f. Refusal repairs the demonstrated soundness failure but does not navigate those cases.

The parser now supplies a separate `method_result_literal_group` fact. The consumer requires it before the earlier fresh-result flag and cannot reinterpret a rejected method as a fresh let binding. The positive envelope is one ordinary static member in an error-free type group, with a complete direct record RHS made from admitted literals and structural containers. Sibling/instance/constructor/static-let bodies, generics, attributes, opaque calls, interpolations and unmodelled header forms are excluded. Explicit record annotations and qualified labels retain their separate proof paths.

Read-only inspection found no concrete escape in this admitted syntax. Opaque string checks distinguish literal dollar/braces from interpolation nodes; numeric suffixes and prefixed expressions cannot pass as ordinary integer constants. Traversal is bounded and rejects parse errors, missing nodes and projected literal islands. This is a scoped source review plus finite compiler-backed testing, not a formal proof or all-F# certification.

## Independent results

| Gate | Exact-correct navigation | Unavailable | Wrong exact |
| --- | ---: | ---: | ---: |
| 16 valid constraint controls | 1 | 15 | 0 |
| 3 separately retained compiler-invalid inference boundaries | 1 | 2 | 0 |
| 24 valid literal/header controls | 12 | 12 | 0 |

`factory-constraint-envelope-78fd.json` retains actual FCS output, compiler/script hashes, sources and native responses. The plain literal factory remains exact. The previously exact factory containing an ordinary unconstraining framework call becomes unavailable; this is an explicit coverage loss under the interim envelope. The inline operator example was already unavailable. All 13 previously wrong valid examples are now unavailable.

`factory-literal-boundary-78fd.json` verifies ordinary, verbatim and triple strings containing inert dollar/braces; property/unit/typed-simple headers; explicit/qualified record paths; and tuple/list/array/nested-record containers. These 12 cases are exact-correct. Four interpolation forms, excluded headers, ranges and numeric suffixes are unavailable. All 24 final fixtures are compiler-valid. The first runner accidentally used invalid indentation in six container fixtures; that diagnostic is retained as `factory-literal-boundary-78fd-invalid-fixture-diagnostic.json` and is excluded from this gate.

`factory-cache-migration-78fd.json` seeds the same cache with 858f's cold and warm wrong exact answer. The new build refuses it on reopen. A version1 edit to a plain literal produces the exact correct field; a version2 edit restoring the constraint is unavailable. Both the reopened and edited constrained reference queries remain unavailable rather than publishing an incomplete exact set. Safety refusals are not counted as successful reference navigation. Native open counters remain zero for source reads, AST parsing and helper/compiler starts in these independent runs.

## Real coverage regression

The evaluator reran the identical prepared inputs and unchanged query selections. Main100 remains 100/100. The fixed Grace46 sample falls from 22/46 to **14/46**: definitions fall 15→10 of23, and complete reference sets fall 7→4 of23. There are 32 unavailable queries and no wrong exact or protocol errors within that matrix.

Eight prior correct answers are lost: OnFailure, Kty and Artifact definitions and references, plus the StoragePoolId creation definition and ArtifactId access definition. There are no gains in this fixed sample. See `../../evaluation/results/recovered-78fd-vs-858f-grace46-deltas.json` and the corresponding main100 delta report. This loss is not renamed as an improvement merely because the new rule is safer.

No new timing acceptance was run for 78fd. Earlier digest-cache performance remains a component measurement on 858f, whose whole-engine factory correctness was rejected. The next practical work should recover bounded typed receiver chains and type-test binders, then broaden factory inference only with explicit constraint evidence. Production replacement and full feature parity gates remain unmet.
