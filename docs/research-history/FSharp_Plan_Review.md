# Independent review of the Rust F# extension proposal

20 November 2026 · Independent scorer agent

## Verdict

**93/100 for proposal quality. Approve the proposal as the basis for the feasibility stage.** The revised document meets the clarified planning requirements: Rust is the primary implementation language, the reference extension's real build/test conventions are incorporated, a defensible large public corpus has been measured, alternatives are compared, and semantic/performance uncertainty remains explicit.

**Implementation correctness, Ionide parity, memory reduction and latency targets are NOT ESTABLISHED.** No extension or FCS semantic benchmark has run. The score is a subjective assessment of the proposal's completeness, reasoning and evidence, not a 93% probability of success or a measured performance result.

No hard planning blocker remains in the reviewed revision. The first implementation stage is an experiment that can reject the architecture; it is not permission to assume the complete replacement is feasible within the proposed budgets.

Reviewed proposal SHA-256: `fa4f0e7b1367329783e469249f06d433df99460d1bab24d48926a0d8832dfbb9`.

## Same rubric, rescored

| Criterion | Weight | Initial | Revised | Assessment |
| --- | ---: | ---: | ---: | --- |
| Exact semantic navigation and freshness contract | 20 | 18 | 19 | Defines snapshots, all dirty overlays, stable identities, conservative validity, complete-reference coverage and explicit failures. The remaining work is to prove these mechanisms in real compiler calls. |
| Semantic architecture and alternatives | 20 | 15 | 17 | Compares nine approaches, explains existing upstream filtering/graph work, and proposes falsifiable cold/dirty experiments. The FCS-to-persistent-index bridge and practical dependency costs remain the central unvalidated hypotheses. |
| Footprint and latency accounting | 10 | 8 | 9 | Includes helper overlap, full process trees, RSS/PSS/cgroup distinctions, cache state, cold readiness, complete-result time and failed requests. Exact workload-specific thresholds and hardware remain to be fixed. |
| Rust allocation and justified interoperability | 10 | 2 | 10 | Rust owns engine, CLI, search, syntax, storage, snapshots, scheduling, supervision and documentation handling. TypeScript is the editor adapter; .NET is narrowly justified for semantics, MSBuild and baseline-matching decompilation. The thin inherited development dispatcher preserves the user's conventions. |
| Reference-repository build, CLI and tests | 12 | 10 | 11 | Maps concrete directories, command meanings, pinned runtimes, trust tests, disposable profiles, actual packaged VSIX, private Linux display and retained evidence. Clean-runtime/platform packaging still needs an implementation manifest and tests. |
| Corpus and evaluation design | 12 | 5 | 11 | Ten measured repositories with pinned commits and reproducible physical-line counts; separates tests/generated data and static versus evaluated project sizes. Three complementary primary corpora avoid overfitting to compiler `.fsi` boundaries. Actual MSBuild evaluation and fixed query traces are pending. |
| External hover and ordered-tree fidelity | 8 | 7 | 8 | Handles ownership before alias normalization, linked/vendored/project-output symbols, use-site inference, versioned docs, ordered folder runs and baseline-matching external implementation navigation. |
| Evidence discipline and actionable research | 8 | 6 | 8 | Clear inspected/measured/not-run ledger, independent review, agent responsibilities, six experiments and explicit stop/reconsider conditions. Does not relabel repository inventory as semantic benchmarking. |
| **Total** | **100** | **71** | **93** | **Proposal quality only.** |

## Hard gates and present status

| Gate | Status now | Evidence still required |
| --- | --- | --- |
| Rust-first boundary | Planning pass | Implement the stated division without moving substantive runtime work into the adapter. |
| Defensible public-corpus selection | Inventory pass | Evaluate actual project instances/configurations and pin compatible oracle/toolchain versions. “Largest” remains limited to the ten measured candidates. |
| Exactness, freshness and completeness contract | Planning pass | Correct complete outputs on golden fixtures and differential query traces, including dirty dependencies and worker restarts. |
| Reference-repository testing conventions | Planning pass | Real host/provider and packaged UI runs, including trust, persistence and clean-runtime execution. |
| Semantic feasibility within the budget | **Unproven** | Cold giant-project and unsaved early-API experiments must succeed within agreed thresholds. |
| Memory reduction without shifting waits | **Unproven** | Separately run baseline/candidate process measurements and repeated post-edit/post-eviction traces. |
| Full requested feature parity | **Unproven** | Navigation sets/targets, external hover semantics, tree behavior and external implementation views, with known exclusions made explicit. |

A worker killed by the memory cap is not a successful low-memory navigation request. An accurate cached answer does not establish that creating or refreshing its index is affordable. A fast tree does not establish that navigation is ready.

## Independent verification performed

I reviewed the revised proposal and all four specialist notes, then independently reran the supplied read-only inventory for the three recommended repositories. Exact commits, file totals, physical lines, project counts, path classifications and static per-project statistics all reproduced without differences:

| Repository | F# files | Physical lines | Result |
| --- | ---: | ---: | --- |
| dotnet/fsharp | 6,811 | 1,090,766 | Reproduced |
| B2R2 | 1,426 | 780,888 | Reproduced |
| Grace | 545 | 301,823 | Reproduced |

These counts include blanks/comments. They do not establish code-only SLOC, evaluated compilation size or navigation performance. I did not independently rerun all seven remaining repositories; I inspected their recorded metrics and the common measurement script.

I also directly checked the actual Ionide ordered-tree implementation, the reference repository's integration/UI launchers, FSAC's checker settings/reference filter, and its existing decompiler use. The important repeated-directory-run behavior and isolated oracle/provider strategy are grounded in source, not simply copied from the research summaries.

One concrete defect found during rescoring was corrected: “signatures or decompilation” could have silently weakened external definition behavior. The final revision now requires the pinned Ionide implementation-navigation fallback, permits a small existing .NET decompiler helper, and labels signature-only behavior as degraded unless that is also the baseline's limit.

## Remaining research risks

1. **Exact cold semantics may dominate regardless of Rust.** A candidate near the end of an inference-heavy project can need substantial preceding context. Search speed and low Rust overhead cannot remove that dependency.
2. **The safe initial cache predicate may invalidate broadly.** Using whole project/transitive source snapshots is conservative, but early edits may discard most warm semantic results. Narrowing this safely is its own research task.
3. **Portable semantic identities are not demonstrated.** Overloads, local scopes, signatures/implementations and cross-project symbols must survive worker restarts without stale or conflated mappings.
4. **Type providers and other external inputs weaken persistence.** Unknown input dependencies cannot be certified merely by hashing source files. Their unsupported/nonreusable cases need observable behavior and tests.
5. **Largest does not mean most representative.** dotnet/fsharp's extensive signatures and invalid fixtures differ from an ordinary monolith. Grace and B2R2 are necessary complementary gates; the user's actual code remains the final relevance test.
6. **Supported snapshots may differ from inventoried HEADs.** SDK/compiler/oracle compatibility has not been established through builds. Selecting an older compatible revision requires new provenance and regenerated counts.

These are normal open questions for the proposed feasibility stage, not reasons to pretend the proposal itself is incomplete or to claim the goals have already been met.

## Next required evidence

Run these in order before committing to full extension implementation:

1. **Evaluate and pin project contexts.** Obtain final compiler arguments, source order, references and configurations for dotnet/fsharp, Grace and B2R2; record the compatible baseline and toolchain.
2. **Cold definition and dirty-reference experiments.** Resolve inferred symbols near the end of the large project, then change an upstream inferred API in an unsaved buffer and request complete cross-project references. Record actual parse/check/import work and all owned process memory.
3. **Persistent-record and filter validation.** Compare complete occurrence sets, prove conservative cache invalidation, restart workers and test candidate recall. Include operators, aliases, implicit uses, signature pairs and malformed/incomplete source.
4. **Replay measured edit/eviction traces.** Use fixed hardware and separate baseline/candidate runs, explicit cache states, raw samples/failures and the proposed independent cold trials plus warm distributions.
5. **Only after the semantic gate, implement and validate the packaged editor.** Reuse the reference host/UI machinery; test the actual VSIX, external/internal hover distinction, repeated-folder order, SourceLink/decompilation fallback and clean-runtime execution.

The correct next conclusion can be either “the architecture meets the combined target” or “the combined target requires a different budget, indexing strategy or scope.” The plan now provides a credible way to tell those outcomes apart.
