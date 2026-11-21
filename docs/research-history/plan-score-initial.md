# Independent review of the initial F# extension proposal

Reviewer: independent scorer agent · 20 November 2026

Reviewed `/workspace/scratch/dd9aeba5da3c/FSharp_Extension_Plan.md` as it existed before incorporation of the user's Rust-first, corpus-selection and agent-orchestration clarification. This is a proposal-quality review, not a claim of tested implementation quality.

## Verdict

**71/100 for the current proposal against the clarified brief.** The research direction is credible and unusually careful about correctness, but the current document should not yet be treated as the frozen implementation architecture. It needs a Rust-first component allocation, measured public-corpus selection and a more explicit alternatives/experiment decision matrix.

**Empirical performance and parity readiness: NOT ESTABLISHED.** No implementation or semantic benchmark has run. The proposed RAM and latency numbers are targets, not evidence. No planning score can override that status.

## Weighted rubric

| Criterion | Weight | Current | Basis |
| --- | ---: | ---: | --- |
| Exact semantic navigation and freshness contract | 20 | 18 | Explicitly rejects text-match equivalence, includes dirty buffers, overloads, implicit constructs, dependency invalidation and complete-result requirements. Needs an executable cache-validity contract and concrete cross-worker symbol-identity proof. |
| Feasible semantic architecture and alternatives | 20 | 15 | Correctly identifies FCS dependency-prefix costs, keeps an escape from retained whole-solution state, and avoids promising cheap exact queries. It has not compared all proposed approaches with falsifiable selection criteria or demonstrated the persistent verified-index bridge. |
| Footprint and latency accounting | 10 | 8 | Includes the full process tree, cold/warm/evicted contexts and the memory/latency tradeoff. Needs measurement definitions for incremental VS Code memory, concurrent helper overlap and repeat statistics. No points awarded for meeting budgets. |
| Rust allocation and minimal non-Rust boundary | 10 | 2 | Initial Node coordinator with Rust conditional on profiling conflicts with the clarified instruction. TypeScript adapter and minimal FCS/MSBuild bridge remain justified; coordinator/index/scheduler/CLI core should default to Rust. |
| Reference-repository build, CLI and extension-test reuse | 12 | 10 | Accurate pinned devenv/Safe Chain/CLI, real extension-host plus packaged VSIX/UI layers, disposable profiles and failure evidence. Needs concrete directory/test mapping and explicit persistence/remote/platform test ownership. |
| Benchmark corpus and evaluation design | 12 | 5 | Good workload matrix, baseline and giant-single-project requirement. No named quantified corpus, fixed query traces or reproducible metric results yet. |
| External hover and ordered-tree fidelity | 8 | 7 | Good source-ownership cases, XML versus binary distinction, actual MSBuild evaluation and ordered items. Exact scope of tree manipulation versus read-only display remains an assumption, and ownership policies for linked workspace sources need a fixture. |
| Evidence discipline and actionable research plan | 8 | 6 | Pins inspected revisions and says no implementation/benchmark ran. Needs a compact evidence ledger distinguishing inspected, tested and hypothetical; team responsibilities and reassessment rules should be explicit. |
| **Total** | **100** | **71** | |

These points judge whether the proposal provides a sound basis for experimentation. They do not estimate the probability that an unbuilt system can meet 1 GiB or subsecond queries.

## Hard gates

| Gate | Initial status | What constitutes a pass |
| --- | --- | --- |
| Exactness is the product contract | Pass at design level | Ordinary navigation never silently substitutes a heuristic, stale cache or incomplete reference set. Document remaining language/project exclusions. |
| Rust-first implementation boundary | Fail after clarified brief | Rust owns indexing, syntax/search, persistence, scheduling, supervision and benchmark/automation core. Any retained Node or .NET role has a specific interoperability justification. |
| Corpus claim is defensible | Not established | Reproducible counts over pinned candidate revisions; production, tests, generated and negative fixtures distinguished; “largest verified among candidates” used unless global coverage can actually be established. |
| Semantic feasibility on large live source | Unproven | Complete correct definitions/references after relevant unsaved dependency edits, including a large single compilation unit, within declared memory/latency limits. |
| Footprint is genuinely reduced | Unproven | Process-tree peak/steady memory and CPU measured against separately run pinned baseline, accounting for MSBuild/metadata workers and reclaimed compiler contexts. |
| Low-memory behavior does not merely shift waits | Unproven | Repeated query traces after eviction, edits and branch changes stay within the agreed targets; no hiding minutes of required indexing behind a “ready” tree. |
| Packaged extension behaves like tested engine | Design only | Real extension-host providers and packaged VSIX/UI tests, on both pinned editor versions where the baseline harness does so, with failure artifacts and restart persistence checks. |

## Highest-priority changes

1. **Replace the Node-first coordinator recommendation now.** Rust is a user preference, not an optimization contingent on a future profile. Keep the smallest necessary TypeScript adapter. A thin inherited Clipanion automation layer is reasonable if it preserves the reference repository's conventions, but explain that distinction rather than letting Node own the runtime engine.
2. **Add an alternatives table with decisions and test status.** Include text-only search, Tree-sitter plus structural binding, a new type checker, minimal FCS, filtered FCS, build/offline semantic indexing, and minimal/forked FSAC. For each: exactness envelope, expected memory mechanism, cold-query failure mode, dependency burden, evidence inspected, experiment that could reject it, and recommendation. “Researched” must not be relabelled “tried”.
3. **Make the first two semantic experiments explicit.** Experiment A: resolve a symbol at the end of a very large F# project from a cold worker with all required preceding context. Experiment B: change an early inferred public API in an unsaved buffer, then run complete cross-project references. These expose whether the design avoids whole-solution retention while still incurring intolerable repeated dependency checking.
4. **Specify the persisted result's validity predicate.** File-content hashes alone do not certify that a symbol use remains valid. Start with a conservative compiler-context fingerprint covering source prefix/dependency/configuration and all dirty overlays. Define how a result loses validity, how worker-local FCS identities become stable records, and how `.fsi`/`.fs`, overloads and repeated declarations are distinguished. Optimizing this predicate is a separate research problem.
5. **Use the largest measured public corpus plus a relevant second one.** Source lines in invalid compiler test inputs are valuable for parser robustness but cannot be fed into one giant semantically valid solution. Require both repository size and actual largest evaluated compilation unit. A compiler implementation also has atypical generated sources, bootstrap settings and internal dependencies; do not assume it models a healthcare application monolith.
6. **Define statistical/reporting mechanics.** Fix query/edit traces, warm-up policy, at least repeated trials and median/p95 with sample counts, hardware/OS/SDK/runtime IDs, page-cache state and full-process measurement scope. Distinguish first useful result, exact query completion, all-index completion and initial project-evaluation completion.

## Additional risks the revised plan should retain

- A capped worker that aborts an ordinary exact request is a resource-control success but a feature failure. It cannot be counted as meeting both goals.
- Structural candidate rejection requires no-false-negative reasoning. Unknown grammar nodes, aliases and implicit references should widen the candidate set; broadening may eliminate most speedup on difficult symbols.
- Lower-retention FCS flags can cause duplicate checks or remove information required by reference APIs. Benchmark a capability matrix, not one aggregate cache-size number.
- External-only hover still needs internal inference when the receiver or overload is inferred locally. Metadata extraction does not remove that cost.
- An external assembly cache needs version/dependency/target-framework identity. Last-build data cannot authoritatively describe dirty workspace symbols.
- A fresh exact-reference index can make repeated queries cheap; generating it or refreshing it after an early inferred API edit can remain expensive. Report both costs.

## Independent checks performed

I read the full initial plan and directly inspected:

- FsAutoComplete `CompilerServiceInterface.fs` checker creation, showing project cache size 200, retained background resolutions/symbol uses and semantic-classification storage.
- FsAutoComplete `AdaptiveServerState.fs` reference prefilter and its transparent/background compiler paths, confirming the plan's warning that text filtering is not new by itself.
- Reference repository `AGENTS.md`, `docs/DEVELOPMENT.md`, `cli/lib/testing.mjs`, `cli/lib/runtime.mjs`, `.vscode-test.cjs`, UI smoke test references and CI artifact configuration. The isolated/profile/runtime-lock description is grounded in actual code.
- [FSharpChecker API](https://fsharp.github.io/fsharp-compiler-docs/reference/fsharp-compiler-codeanalysis-fsharpchecker.html), including checker input context and cache APIs. Retrieval ref `turn811894949view0`.
- [VS Code programmatic language features](https://code.visualstudio.com/api/language-extensions/programmatic-language-features), confirming the direct provider interface and the fact that all registered hover providers participate. Retrieval ref `turn811894949view1`.

No extension build, FCS query, performance run or repository corpus count was executed by this scorer. Other research agents' forthcoming results must be labelled by their actual evidence level.
