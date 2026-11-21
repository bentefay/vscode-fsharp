# Independent review: ordered nominal lookup

The ordered source-type lookup change passes the scoped independent gates below on immutable `3fb27998dab1ec2f0ee362deee64cc15af139e9c351ab71461dc1a724230f03f` (source `8c55c7aba942`). It preserves the distinction between a certified absent external candidate and an unknown candidate. This is not acceptance of the whole extension or all nominal/type-inference cases. A separate normal-path parameter review now blocks whole-candidate acceptance: nine declaration-inclusive reference queries omit a signature occurrence while claiming completeness. Twelve definition checks also differ from FSAC's paired-slot target preference; renamed parameters demonstrate that slot pairing must not be treated as reference equality. See `../../evaluation/experiments/local-lexical-proof/native-paired-3fb2-complete/report.json`.

## Actual compiled reference universe

`ordered-nominal-prepared.json` retains a real successful external-library and consumer build, genuine09f011 observed-v4 model, complete schema2 imported-name environment, and direct FCS checks using the evaluated compiler arguments. The consumer includes external-only opens, direct external collisions, one-level and transitive external AutoOpen modules, source AutoOpen, aliases and generic fields resolved in their declaration context, and a paired signature result.

On the unchanged10-query fixture,78fd had two exact intermediate/type-position answers and eight unavailable chains.3fb2 has **six exact-correct answers and four unavailable** (`ordered-nominal-78fd.json`, `ordered-nominal-3fb2.json`). New exact answers are the external-absence source chain, source AutoOpen chain, declaration-origin alias chain and generic chain. Direct and both external AutoOpen collisions remain unavailable; none fall back to the older source type. The signature call-result case also remains unavailable. Removing the required imported-name environment leaves all three checked dependent source chains unavailable.

These tests supply no external declaration catalog. An unavailable external member is therefore a coverage limit, not a successful external-navigation result. The external owners are independently established by the real compiler; the tests specifically exclude an incorrect fallback to a source record.

## Closed modules, order and access

`closed-nominal-3fb2-expanded.json` has11 independent source-only compiler controls:10 valid and one deliberately invalid exported-module-abbreviation boundary. Five valid controls are exact-correct; five are unavailable; the invalid boundary is unavailable. There are no wrong exact results in this set.

Qualified/opened public types and type aliases from a completed module remain usable. Internal types in the same project remain usable. A later separate type declaration does not retroactively replace the type selected in an earlier field annotation. Mutually recursive forward type/alias cases remain unavailable. Private opened types/aliases correctly fall back to outer types in the actual compiler, while native currently remains unavailable. A declaration-origin alias under a later caller open is also unavailable in this source-only fixture. Those refusals are missing coverage. F# module abbreviations are not treated as exported type aliases.

The original internal-control fixture exposed an internal type through a public function, causing a real accessibility error. That diagnostic is retained in `closed-nominal-78fd-internal-access-diagnostic.json`; the corrected internal function is compiler-valid. The main raw alias-expression suite also remains22 correct and86 unavailable with zero wrong exact (`alias-expression-3fb2.json`).

## Cached negative evidence after a real DLL change

`ordered-nominal-freshness-3fb2.json` starts with four exact source-target answers and one repeated warm answer. The isolated external DLL is then rebuilt to add `Review.Empty.Scope`, while consumer source/project bytes remain unchanged and its build still succeeds. A direct compiler replay changes three owners to that new external type; the declaration-origin alias remains the original source type.

Old imported-name evidence is refused in both the live process and a restarted process using the same cache. The model's ordered tree remains exact: the external DLL is not a model-shape content hash. This specifically exercises the imported-name dependency of a final workspace-owned answer. Fresh genuine imported-name evidence preserves the unchanged source alias answer and leaves the new external targets unavailable rather than returning old source declarations. A genuinely exported but incomplete reference universe is also refused. Original library source and output bytes are restored after the test.

## Coverage and acceptance limits

The evaluator's unchanged main100 remains100/100 and fixed Grace46 remains **14/46**. Three separately declared original Grace `Scope.RepositoryId` cursors now resolve correctly, backed by a fresh direct FCS oracle, but they do not create a new complete answer in the fixed46 sample. Reference proof gaps shrink; unresolved candidates still prevent complete sets. See `../../evaluation/results/recovered-3fb2-original-nominal-chains.json`, `nominal-chain-direct-fcs-oracle.json` and `recovered-3fb2-vs-78fd-grace46-deltas.json`.

No performance comparison was run or accepted for this slice. The literal-factory constraint guard remains separate. Paired signature/implementation parameter targets must be classified precisely: choosing another declaration of the same parameter is a target-preference parity issue, whereas an exact reference set omitting a paired declaration is a distinct completeness issue. Neither should be conflated with the earlier factory bug selecting a different record field.
