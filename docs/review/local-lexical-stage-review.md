# Independent review of the staged document-local parameter path

**The reviewed isolated module passes the scoped semantic replay; production integration is not accepted or measured yet.** Reviewed module SHA256 is `bd81ba845c8c6faf08d9894c8c29d1924cbb3a4ee50a46f06105c4b97cc8646f`; development probe SHA256 is `d45bd183b8a3f3805f60dcc47ebb944d1a475f793f904c29645e6bfdfbe658b2`. Copies are retained in `local-lexical-stage-snapshot-final/`. The source is staged outside the shipping crates. Its parser fingerprint matches the actual vendored parser C SHA256 `00943a441bd59446ed9d321f3e0e09402ec3069c623890fc0b1893717d001302`.

## Actual compiler and independent probe results

`local-lexical-adversaries.py` runs24 independent cases through the pinned FCS project checker, retaining project diagnostics and actual symbol-specific complete occurrence sets. It then invokes the copied isolated probe at every selected token for both definitions and references. `local-lexical-adversaries.json` records:

| Compiler status | Cases | Exact-correct cursor/operation checks | Unavailable | Mismatches |
| --- | ---: | ---: | ---: | ---: |
| Valid | 19 | 52 | 78 | 0 |
| Invalid inference/role boundaries | 5 | 20 | 14 | 0 |

These are correlated token/operation checks, not independent success probabilities. Invalid-source outcomes are retained separately and do not enlarge the valid denominator. A correct binding recovered by FCS in an otherwise invalid expression is not evidence that the entire expression is semantically valid.

Positive controls cover ordinary `nameof` uses, an instance-member operand of `nameof`, a shadowed ordinary `nameof` function, nested let-function parameters, nested parameter shadowing, a nonrecursive same-name function binding, and a lowercase source literal name shadowed by a lambda parameter. The body traversal preserves the old outer value inside a nonrecursive binding's RHS and the new binding in its continuation.

Valid unavailable controls include explicit `new x.Alias()` despite a same-spelled local value, quotations, possible method named arguments, ordinary boolean equality passed as an argument, lambdas embedded in named arguments, escaped parameter uses, uppercase parameters, dynamic lookup and unsupported custom operator syntax. These refusals are missing coverage. Intrinsic `nameof x.Alias` with a local `x` did not demonstrate a type-namespace escape: FCS still binds the receiver `x` and diagnoses the invalid member access. That failed hypothesis is preserved.

The initial module admitted a simple callback lambda nested inside an interpolated string because its ancestor filter covered quotations but not interpolation nodes. The six returned checks were actually correct, so this was an advertised-envelope escape rather than a wrong-symbol result. The final module refuses this context; the original source/probe and `local-lexical-adversaries-initial.json` remain retained. No other expected result or source fixture changed.

## Source review

Both operations require a positively recognized local category: an application-argument lambda or a let-function nested in another binding body. Public direct parameters and immediate exported/returned lambda forms are refused. The body walk uses an explicit syntax whitelist, not unrestricted recursion. It rejects unknown roles, named-argument equality ambiguity, escaped names, dynamic member lookup and type/constructor syntax. A selector in `x.Member` is not treated as a use of the formal named `Member`; the root is matched to the nearest admitted lexical binding.

The complete body is traversed even for definition queries. Nested parameters and nonrecursive let continuations update the environment explicitly. The selected cursor must be the parameter declaration or one of the proven value-use ranges. UTF-16 output is derived from the original snapshot; its BLAKE3 digest and version are checked. The module does not invent a project-context certificate or load workspace/project/library data. Its `/proc/self/status` reads are resource monitoring, not source discovery.

Root parse errors, directive-bearing sources, cancellation and resource exhaustion fail closed. The source cap is256KiB; parse/total budgets are100/150ms with50,000 visited nodes, depth64 and4,096 references. A stress check with1,000/4,000/8,000 parameters aborted during parsing around100ms (`local-lexical-parameter-bound.json`). It did not demonstrate a budget overrun. The parameter duplicate check nevertheless has a quadratic vector scan outside the walk's inner cancellation checks; a small parameter-count cap would make that bound explicit rather than relying on the parser to reject such shapes first. This is a hardening observation, not a demonstrated semantic blocker.

## Normal-path defects remain separate blockers

The existing normal3fb2 path still fails complete semantic parameter references. `../../evaluation/experiments/local-lexical-proof/native-paired-3fb2-complete/report.json` has nine same-name signature-linked cursor checks reporting exact3 when actual FCS and FSAC return4. Twelve definition checks instead differ only in paired callable-slot target preference. Renamed signature `publicName` and implementation `x` demonstrate why these contracts must be separated: navigation pairs slots, while references remain one signature occurrence versus three implementation occurrences.

`native-callable-3fb2/fcs-completeness.json` in the same experiment separately has eight incomplete-exact method/constructor checks per declaration flag. The missing occurrences are caller named-argument labels. Shipped FSAC also omits those labels; matching that omission does not fulfill the requested complete semantic-reference contract. The valid same-directory `Signature.fsi`/`Implementation.fs` case also prevents using filename-stem equality as a complete pairing rule.

## Required integration gate

Before adopting the staged path, replay these controls through an immutable native executable with acknowledged overlay versions, stale/cancelled responses, edits, linked instances and session restart. Demonstrate exact local positives with deliberately unavailable model/catalog paths and zero model validation, inventory loads, helper starts and source-domain scans. Retain public/signature-correlated and named-argument cases as fallbacks, rather than allowing the fast path to conceal normal-path incompleteness. Measure first useful query, edits and all-process memory only after those checks. No latency, memory, full-language or whole-extension acceptance is assigned by this staged review.
