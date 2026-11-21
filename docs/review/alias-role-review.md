# Independent alias-expression role review

Status: actual compiler oracles, 2026-11-21; new native role shortcuts are not yet independently accepted. Raw sources, execution outputs and FCS symbols/reference groups are in `alias-receiver-oracle.json` (fifteen scenarios: twelve valid, three invalid boundaries). The earlier nineteen expression/precedence scenarios and rejected9201 native results remain in `alias-expression-oracle.json` and `alias-expression-native-9201.json`.

For a pure expression selector whose actual receiver is a parameter, self identifier or local let binding, the tested F# compiler binds the member on that value rather than a same-spelled type alias in a module. This also holds inside intrinsic nameof. A missing member is a compiler error, not a successful fallback to the module's alias. The error-recovery FCS cursor may nevertheless return an alias symbol in that invalid program; it must not be promoted to a valid-code proof.

The exclusion must be subordinate to explicit type roles: in a function with a local parameter named N, both `typeof<N.Alias>` and `new N.Alias()` still refer to the type alias in module N. An actual local value declaration existing somewhere in scope does not prove that every occurrence of its spelling is in the expression namespace. The candidate implementation's TypeUse/alias-RHS precedence and restriction to a proved non-top-level local receiver are therefore essential.

Parenthesized local receivers and actual call-result receivers resolve value members in the valid controls. A parenthesized module receiver is invalid, while `nameof (N.Alias)` remains a valid type alias reference. The parser must distinguish these roles before a generalized computed-source negative shortcut is accepted. A generic 'has parentheses/source expression' flag is not enough by itself.

Three additional positive controls establish narrower role exclusions:

| Syntactic role | Actual compiler reference group |
| --- | --- |
| Companion `module Alias = ...` declaration and its module qualifier | Module entity, distinct from same-spelled type alias |
| `module Alias = Other` abbreviation declaration | Referenced module group, distinct from same-spelled type alias |
| Named DU field `Case of Alias:int`, construction label and pattern label | DU FSharpField group, distinct from type annotation Alias |
| Constructor application of winning type alias | Logical alias reference group, but external physical constructor goto |
| Intrinsic nameof of winning type alias | Logical alias group and source alias goto |

These support rejecting module declaration-name tokens and named DU label tokens as candidates for a type-alias reference search. They do not support identifying arbitrary qualified expression roles from spelling alone. Keep payload type tokens, record labels/puns, ordinary member selectors and nameof binding decisions separate.

Next gate: replay the frozen native candidate against these oracle roles and the known earlier-value/later-alias false-exact definitions. Complete reference sets must contain every eligible constructor/name-of occurrence while excluding member/module/field occurrences. An unavailable result is safe rejection but remains missing navigation coverage.
