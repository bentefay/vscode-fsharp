# Lightweight F# navigation extension — researched proposal

Prepared for Ben · 20 November 2026 · Planning and repository inventory only

## Recommendation and decision boundary

Build a **Rust engine and CLI**, a small TypeScript VS Code adapter, and narrowly scoped .NET helpers for authoritative F# semantics and MSBuild evaluation. Rust should own source snapshots, search, incremental syntax, indexes, caching, scheduling, process supervision, XML documentation and automation. The proposed savings come from compact persistent results and bounded compiler residency, not simply replacing a search loop with ripgrep.

The first milestone is a **go/no-go semantic experiment**, before building the full extension. It must show that exact navigation survives cold queries and unsaved dependency edits within acceptable memory and latency. No investigated approach yet establishes that all three goals—complete Ionide-equivalent semantics, very low memory and uniformly fast cold queries—can be achieved on arbitrary F# monoliths.

Use `dotnet/fsharp`, the largest repository among ten measured candidates, together with **Grace** for application behavior and **B2R2** for large projects without signature files. The compiler repository alone could produce an optimistic result because its production code has extensive `.fsi` boundaries.

## Evidence ledger

| Work completed | Evidence level | What it establishes |
| --- | --- | --- |
| Inspected Ionide, FsAutoComplete, F# compiler and ProjInfo source | Direct source inspection at recorded commits | Existing behavior, semantic dependencies and optimization mechanisms |
| Inspected the example extension's CLI, Nix, tests, packaging and CI | Direct source inspection | Concrete conventions to reuse, including actual packaged-VSIX tests |
| Cloned and inventoried ten public F# repositories | Executed read-only inventory, reproducible script and JSON results | File/physical-line counts, static project candidates and provenance |
| Compared Rust search/storage/metadata and semantic approaches | Documentation/source research and engineering assessment | Candidate designs and rejection criteria |
| Independent review of the initial proposal | Proposal-quality assessment | Initial score 71/100 against the clarified brief; revisions below address its gaps |
| Extension prototype, FCS query experiments and parity/performance tests | **Not executed** | No empirical performance, memory or semantic-equivalence claim is made |

“Researched” is not “benchmarked.” The reported 10 GB usage and long delays are Ben's observations; the investigation has not independently reproduced them. No extension was installed or published.

## Required behavior

| Feature | Acceptance contract |
| --- | --- |
| Definition and peek definition | Correct local, cross-file, cross-project and external targets from F# source, including dirty buffers and `.fsi`/`.fs` behavior. |
| Find references | Complete semantic reference sets within the selected solution/configuration, matching the pinned baseline's treatment of implicit references. No heuristic, stale or truncated set is presented as complete. |
| External-only hover | At library-symbol use sites inside repository code, show accurate F# types/signatures and available documentation. Suppress symbols owned by the open repository or workspace projects. |
| Solution explorer | Preserve solution/project structure, ordered source items, linked paths, reference nodes, load states, expansion, reveal, opening and refresh behavior. |
| Basic editing | Register F# file types, syntax highlighting and language configuration so disabling Ionide preserves ordinary editing. Reuse licensed assets. |

Cover `.fs`, `.fsi` and `.fsx`, including script contexts. The default interpretation is navigation **from F# documents**, including F# uses of .NET APIs; it is not a new C# language service. Mixed projects remain visible and their exposed APIs must resolve where F# uses them. A read-only explorer matches the requested display feature; add/remove/reorder/build commands are outside the initial scope. Completion, diagnostics UI, rename, refactoring, formatting, analyzers and FSI/debugger UI are excluded.

Pin the supported SDK/language matrix and the shipped Ionide/FSAC oracle before implementation. Current repository heads are research inputs, not automatically compatible released binaries.

## Approaches investigated

None of these semantic approaches has been runtime-benchmarked in this investigation.

| Approach | Accuracy and resource tradeoff | Disposition and falsifying experiment |
| --- | --- | --- |
| Fast text search alone | Small retained state; cannot distinguish identical names or resolve inferred members. | Reject as exact navigation. Keep candidate discovery only. |
| Tree-sitter plus lexical binding/heuristics | Incremental local syntax and scopes are cheap; inference, overloads, extension lookup, SRTP and generated symbols exceed syntax. | Use only proved fast paths. Any missed oracle reference rejects the relevant exclusion rule. |
| New Rust F# type checker | Full control of state, but requires reproducing language, .NET and type-provider semantics. | Reject initial scope; no evidence this is a small replacement project. |
| Tuned or reduced FSAC | Reuses mature behavior with the lowest implementation risk; broad state and dependencies may remain. | Essential comparator. If tuning meets the goal, weigh its maintenance advantage against a new engine. |
| Rust engine plus minimal FCS helper | Controls residency and amortizes exact checks; a cold request can still need a large source/dependency context. | Recommended feasibility candidate. Cold tail-of-project and dirty early-API experiments can reject it. |
| Persistent semantic index from checked files | Fast warm lookups with little resident state; initial checking and invalidation can remain expensive. | Recommended complement. Persist all verified uses from each checked file, not only the requested query. Test restart and broad invalidation. |
| Build-derived DLL/PDB index | Reuses existing metadata/source locations; omits local source semantics and cannot represent dirty buffers. | Auxiliary only. Never substitute stale build outputs for current source. |
| Real `.fsi`/assembly boundaries | Can reduce dependency checking; helps less in a large inference-heavy project without signatures. | Exploit existing boundaries. Do not manufacture synthetic signatures and assume equivalence. |
| Fork FCS for finer slicing/serializable state | Potentially addresses cold costs, but couples the product to compiler internals and correctness risks. | Reserve research branch if supported APIs fail; prefer upstream improvements and separate review. |

Upstream already performs related optimizations: inspected FSAC filters some reference candidates by text and captures identifiers; the F# compiler derives AST-based dependency graphs. The inspected transparent compiler still performs broader graph/import work than a “single-file” API name might suggest. Instrument actual parsed/checked files and imported context instead of claiming candidate-only checking. [1–3]

## Rust-first component design

| Component | Implementation | Boundary rationale |
| --- | --- | --- |
| VS Code provider registration, commands and tree rendering | Minimal TypeScript | Required editor API integration; no heavy work on the extension-host thread |
| Engine, installed CLI, snapshots and UTF-8/UTF-16 mapping | Rust | One implementation shared by editor, automation and tests |
| Search and incremental syntax | Rust `grep-*` crates and Tree-sitter F# | Native file/buffer search and reusable incremental parsing |
| Index, project manifests and verified occurrences | Rust + SQLite initially | Transactional replacement, reverse references and bounded disk-backed lookup |
| Scheduling, cancellation and worker/resource supervision | Rust | Compiler lifetime remains controlled outside its process |
| XML docs, ownership rules, source cache and SourceLink retrieval | Rust | Mostly metadata/text work independent of compiler residency |
| F# inference, symbol interpretation and signature formatting | Narrow FCS helper | Mature semantic authority; avoids a new compiler implementation |
| Final compiler arguments/project evaluation | Short-lived ProjInfo/MSBuild helper | Avoids reimplementing MSBuild conditions, targets and SDK behavior |
| External implementation navigation when source is unavailable | Narrow existing .NET decompiler helper | Reproduces the pinned Ionide fallback without writing a new decompiler |
| Development/test/distribution dispatcher | Thin inherited Clipanion facade | Reuses the example's official Node editor tooling; substantive runtime/benchmark logic stays in Rust |

Use a versioned stdio protocol with request, snapshot and cancellation IDs. LSP is acceptable where useful; transport does not determine compiler memory. Detect simultaneous Ionide activation: another provider can supply unwanted hovers and another FSAC process defeats resource accounting. Never silently disable the user's extension.

Ripgrep's reusable `grep-searcher` supports byte buffers as well as files, so dirty documents need no temporary files or per-document subprocess. Enumerate semantic files from evaluated project manifests: normal ignore rules can omit linked, generated, hidden or explicitly included source. Tree-sitter's F# grammar has Rust bindings but still describes itself as work in progress. Unknown/error nodes widen candidate sets or require semantic analysis. [4–5]

SQLite is the initial store, with explicit page-cache/mmap/connection/WAL budgets and streamed results. `redb` is a credible Rust alternative but requires custom secondary indexes; its inspected default cache budget is 1 GiB, not evidence of immediate residency. Tantivy adds ranking/segment machinery that exact symbol lookup may not need. Reconsider storage only against measured access patterns. An unbounded in-memory map of the whole index is excluded. [6]

Start with one warm semantic worker and a small global process budget, not one resident compiler per project. Compare cache categories, retention flags, legacy/transparent compiler behavior and idle lifetime. GC limits are not total-process caps. Partial checking can cause duplicate work; flags must pass an API capability matrix. Export compact records, release rich objects and recycle based on measured reuse—not on every request. Short-lived helpers can still have high peaks.

## Exactness, persistence and invalidation

A query captures a consistent project/configuration snapshot and **all relevant dirty buffers**, then resolves the selected symbol from a current verified record or FCS. Narrow structural rules may bypass FCS only inside an explicitly tested correctness envelope. Candidate ranking is never a symbol-identity proof.

For references, use visibility and reverse project dependencies, current verified occurrence records, and conservative text/syntax discovery. Handle attribute short names, operators, aliases, backticks and the oracle's implicit references. An uncertain spelling or parse broadens the search, potentially to every eligible source. Semantic checking uses the candidate's real context, which can include a large preceding project prefix and referenced projects.

Persist all available verified symbol uses from a checked file to amortize that cost. Stable records must distinguish assembly/member/overload identities, project instances, source declarations, local scopes and `.fsi`/`.fs` mappings. Worker-local FCS equality is not assumed to survive process restarts; demonstrate portable identity mapping in the first experiment.

A persisted semantic record is reusable only when:

- Schema, compiler, helper and formatter versions match.
- Project identity, SDK, target framework, defines, flags and ordered sources match.
- The queried source version and every conservatively relevant source, assembly and build-input fingerprint match.
- Dirty overlays belong to the same acknowledged snapshot; line-shift matching alone never reattaches results.
- External semantic inputs are known. Unknown type-provider/environment dependencies make affected persistence nonreusable.

Initially use the whole selected project snapshot plus all sources/configuration/references in transitive source dependencies as the conservative context fingerprint. Narrowing it to compiler-proven dependencies or signature boundaries is a separate optimization. This may invalidate much work; measure the cost rather than weaken correctness. Public inferred types and inline bodies can affect unchanged downstream files.

A reference result additionally needs a **coverage manifest** over the current eligible file universe: every file is verified for this context or excluded by a validated conservative rule. Measure filter recall against the oracle; finite testing is evidence, not a proof that arbitrary future syntax is safe. Unrecognized constructs take the broad path.

Publish only after confirming snapshot freshness. Internally distinguish verified-current, stale, candidate, unresolved, cancelled and incomplete. The normal VS Code reference provider receives a complete verified set; do not assume it can display custom completeness metadata. Timeouts, memory aborts and unavailable semantics are explicit failures, never a misleading empty set. A cap that regularly aborts ordinary queries fails the feature/performance objective.

## External hover and navigation

Ownership belongs to the **original selected symbol**, before normalizing aliases for documentation lookup.

| Symbol under the cursor | Hover |
| --- | --- |
| External `List.map`, framework member or NuGet API | Show signature and available XML docs |
| Local variable whose inferred type is external | Suppress |
| Repository type abbreviation pointing to a library type | Suppress on the alias |
| Repository extension member on a library type | Suppress |
| Workspace project symbol reached through its DLL | Suppress |
| Vendored source or evaluated linked source outside workspace roots | Suppress |
| Cached external SourceLink source | Remains external unless independently included as workspace source |

Rust maintains ownership from evaluated project/script source sets, repository roots and assembly-to-project mappings. Positively establish external ownership; namespace/path guesses are insufficient.

Current FSAC obtains checked use-site information and display context before producing its tooltip. A metadata cache solves display/document retrieval, not necessarily the inference needed to identify `x.Method()`. Cache immutable declaration data separately from contextual binding/rendering. XML comments generally reside in companion XML files, not DLL comments. [7]

Use FCS initially to interpret F# signatures through an exact reference context and export compact records or formatter strings. Rust owns XML parsing, culture/version selection, documentation IDs and durable caches. CLR readers exist in Rust, but the investigation found no established equivalent of FCS's compiler-specific F# signature decoder/formatter. `dotnetdll` is GPL-3.0-or-later; permissively licensed `dotscope` advertises CLR metadata, not proven F# signature fidelity. Rewriting decoding is optional research if extraction proves costly. [8]

Key artifacts by content/assembly identity, reference closure, target framework, XML content/culture and extractor/renderer version. Test curried versus tupled arguments, constraints, aliases, units, union cases, records and overloads. Missing XML yields a correct signature without invented documentation. Match the pinned baseline's contextual formatting rather than assume every generic use has a specialized tooltip.

For external definitions, prefer verified available source/SourceLink, then reproduce the pinned Ionide fallback, including a narrowly scoped .NET decompiler helper where the baseline supplies implementation navigation. A signature-only view must not silently replace that behavior: treat it as explicitly degraded and exclude it from parity passes unless the baseline itself provides only a signature. Compare the actual target/member and available implementation view in tests, not merely successful document opening. Preserve assembly/PDB/document checksums, cancellation and offline behavior. Source download must not become a dependency of every cached hover.

## Ordered solution/project model

Rust owns solution selection, cached manifests and display construction. A pinned helper using the workspace-selected SDK supplies the final ordered compiler arguments, references, source/virtual paths, project output identity, configuration/import dependencies and load status. Respect `global.json` separately from the extension helper's runtime.

`dotnet msbuild -getItem:Compile` is a useful preliminary evaluation, but target-generated inputs and reference resolution require more. Inspected ProjInfo obtains the authoritative source sequence from design-time compiler arguments, with compiler execution skipped but build targets still run. Use this final sequence for semantic truth. Cache project evaluation independently from compiler state and reevaluate relevant imports/restore inputs; a source edit should not reload every project. Reap helper/build-server state owned by the extension. [9]

Ionide's tree preserves **ordered directory runs**. The inputs `A/one.fs`, `B/two.fs`, `A/three.fs` can produce two separate `A` groups. A normal trie that globally merges folders changes the visible order. Give repeated runs stable occurrence-sensitive IDs and assert that flattening ordered source leaves preserves the compiler sequence. Test this separately from solution folders and additional noncompile content. [10]

Cover linked files with/without `Link`, repeated basenames, multi-target/configuration conditions, generated sources, `.fsi` ordering, package/project references and failed/unrestored/unsupported nodes. Pin actual release behavior for special directories and noncompile items instead of promising every current-head enhancement. Workspace Trust gates MSBuild/type-provider execution. Preserve user expansion/reveal state on refresh.

## Public evaluation corpus

The inventory counted Git-tracked `.fs`, `.fsi` and `.fsx` physical lines, including blanks/comments. These are **not code-only SLOC**. Project counts include fixture projects, not just the main solution.

| Repository | F# files | Physical lines | Tracked `.fsproj` |
| --- | ---: | ---: | ---: |
| dotnet/fsharp | 6,811 | 1,090,766 | 78 |
| B2R2-org/B2R2 | 1,426 | 780,888 | 110 |
| fable-compiler/Fable | 1,041 | 599,865 | 85 |
| ScottArbeit/Grace | 545 | 301,823 | 22 |
| darklang/dark | 339 | 178,569 | 21 |
| aardvark-platform/aardvark.base | 190 | 131,619 | 15 |
| JetBrains/resharper-fsharp | 6,731 | 103,727 | 65 |
| fsprojects/FAKE | 447 | 99,476 | 103 |
| ionide/FsAutoComplete | 386 | 81,700 | 45 |
| fsprojects/Paket | 295 | 62,265 | 34 |

**Largest verified among these candidates: dotnet/fsharp.** This is not a global census. Its production-path inventory is 363,834 lines, while test/benchmark paths contain 681,049; deliberately invalid fixtures cannot become one valid giant project. Production includes 225 `.fsi` files. Start with its FCS solution; static literal includes identify 412 existing files/287,994 lines in the FCS project, with 12 generated/property-based inputs unresolved.

**Application gate: Grace.** Its production-path proxy is 154,799 lines, with no production `.fsi`. The CLI's 46 literal source files total 63,534 lines and reference SDK/shared/types projects. Its ASP.NET Core/Orleans/NodaTime environment is relevant to Ben's application work. The solution lists 20 F# projects plus three others; navigation evaluation need not launch cloud services.

**No-signature stress: B2R2.** Its production-path proxy is 581,985 lines, without `.fsi`, and the solution lists 107 F# projects. Intel's 30 literal sources total 142,726 lines, including large conditional generated opcode maps. Keep the normal evaluated configuration; report generated-heavy cost separately instead of deleting difficult inputs.

These project sizes are **static literal-include inventories**, not evaluated compile units or lower bounds. Conditions, imports, generated files and properties can change them. The next experiment must evaluate actual project instances. Fable contains a large copied compiler, so it is less independent as the sole second corpus.

The snapshots need careful toolchain selection: dotnet/fsharp pins an 11.0 release-candidate SDK and bootstrap context; B2R2 targets net11.0; Grace pins 10.0.400 and preview/graph compiler options. If the oracle cannot support a snapshot, choose a compatible pinned release and regenerate counts. Do not force an incompatible latest checkout to make a benchmark “largest.” The actual private monolith remains the final relevance check via a local CLI run.

## Example-repository structure and tests

Preserve the example's `cli/bin/run`, `cli/commands`, `cli/lib`, `test/unit`, `test/integration`, `test/ui`, `test/runtime-lock.json`, `scripts/with-display`, devenv files and `test-results`. Add a Cargo workspace for engine/CLI and a narrow .NET helper. Keep the small development dispatcher; the installed Rust executable exposes `serve`, `query`, `project`, `index`, `benchmark` and `parity` through the same services used by VS Code.

Keep `setup`, `doctor`, `check`, `build`, `test unit`, `test integration`, `test ui`, `verify`, `dependencies verify`, `package`, `install` and `release`, including profile/dry-run options. Add parity/benchmark commands and explicit index status/warm/clear. Commit Cargo/npm/NuGet/devenv/runtime locks, pin grammar/native assets, use argument arrays and retain licenses. Safe Chain continues protecting npm acquisition with the 48-hour minimum age and disabled lifecycle scripts; Cargo/NuGet need separate locked acquisition and verification. No ad hoc `npx` tools. [11]

| Layer | Required reuse and additions |
| --- | --- |
| Fast tests | Rust engine/invalidation/identity tests, TS adapter/CLI tests and helper capability tests. `check` includes formatting/type checks and appropriate linting. |
| Real extension host | `@vscode/test-cli`/`test-electron` on minimum and pinned current VS Code; trusted/untrusted fixtures, dirty edits and real provider calls. |
| Packaged UI | ExTester/ChromeDriver installs the actual VSIX, checks visible navigation/hover/tree behavior and restart persistence in a disposable profile. |
| Large-corpus evaluation | Dedicated or local fixed hardware; pinned traces/oracles, isolated caches and Rust process instrumentation. Keep heavy runs separate from fast per-change CI. |

Reuse independent user/extensions/shared-data directories, short temporary paths, credential scrubbing, SHA-checked downloads/cache hits, private Xvfb/D-Bus/Openbox and retained screenshots/HTML/logs/JSON. The example's special untrusted-workspace launcher avoids test-electron's automatic trust-disabling argument. Condition-based waits have deadlines and do not hide startup time. Test process-tree cleanup and leaks.

Run the oracle and candidate in **separate sequential editor profiles**: VS Code execute-provider commands aggregate registered providers. Call definition/reference/hover commands, normalize locations, and compare full sets plus golden fixtures. Test `includeDeclaration` at the provider/protocol boundary; the built-in reference command does not document that extra argument. Internal hover expectations intentionally differ from Ionide. F12/UI tests must prove the packaged binaries execute outside the development shell, including Linux/Nix; declare other platforms and Remote/WSL support only after tests. [12]

## Experiments and acceptance gates

Compare installed Ionide, pinned current/tuned FSAC, minimal FCS, and Rust filtering/persistence plus that same helper. Record versions/settings; source HEAD observations are not the shipped oracle.

| Experiment | Question and failure condition |
| --- | --- |
| A. Cold tail-of-project definition | Resolve an inferred member near the end of a giant project. Fail if required dependency work exceeds the declared budget or repeatedly produces long waits. |
| B. Unsaved early public-API edit | Edit an inferred API upstream, keep several buffers dirty, request complete cross-project references. Any stale/missed result fails correctness; excessive recovery fails performance. |
| C. Candidate recall | Compare candidate files against oracle uses, including aliases/operators/implicit uses/parser errors. A missed file blocks that filter's exact-navigation use. |
| D. Persistent index and eviction | Extract all uses, restart, replay traces, edit/revert/reorder/switch branches and evict workers. Fail if “saved memory” merely shifts waits to repeated recovery. |
| E. Hover and tree fidelity | Compare ownership, contextual F# signature/docs, external navigation and final compiler/tree order. Include repeated folders and linked sources. |
| F. Long-session resource behavior | Churn projects/queries/cancellation and metadata extraction. Detect retained MSBuild children, growing caches and overlapping helper peaks. |

Fix query/edit traces and stable anchors with project/TFM/snapshot IDs. Use golden expectations plus the differential oracle; a baseline bug is investigated, not blindly copied. Begin with ten independent application-cache-cold trials reporting median/range and at least 100 warm requests per scenario reporting median/p95/sample counts. Record warm-up, hardware/OS/toolchains and raw failures. Distinguish application-cache cold from OS-page-cache cold; do not drop the user's global caches. Report restore/setup separately, then measure launch-to-tree, launch-to-first-complete-navigation, complete query and full-index completion, including readiness waits.

Instrument candidate count, actual files parsed/checked, graph/import work, cache hits, invalidation fan-out, worker restarts, CPU/GC, disk I/O and exact completion rate. Measure every owned Rust/.NET/MSBuild process and separately report whole-editor overhead. RSS sums double-count shared pages; label RSS/PSS. A delegated Linux cgroup supplies group peak/CPU/I/O but includes page cache/kernel state, so report it separately. When only sampling is available, disclose peak limitations. Same method, no concurrent agent builds, for each baseline. [13]

Initial **provisional targets**, not achieved results: idle engine/children at most 300 MiB; active aim below 1 GiB with a 2 GiB ceiling to investigate; cached tree under 1 second; warm exact definitions p95 under 100 ms; cached external hover under 75 ms; ordinary complete warm references under 1 second; local syntax update under 50 ms off the editor thread; cold semantic requests in low single-digit seconds. Final thresholds must name hardware, corpus, scenario and memory metric. Broad reference searches get separate distributions, not hidden exclusions.

Hard gates are independent: exactness, honest completeness/freshness, bounded resource use, acceptable cold/edited recovery, and packaged-editor parity. A memory abort is not a successful fast query. If a giant no-signature project cannot meet the combined requirements, stop before full implementation and present evidence-backed choices: greater budget, optional initial semantic indexing, real source/signature boundaries, or explicitly reduced scope. None is silently substituted.

## Delivery and independent review

The orchestrator integrates specialist work; agents own research, implementation and focused tests. A separate scorer critiques the proposal and later evidence without editing the design it scores. Review changes should address real requirement gaps rather than optimize a number.

1. **Feasibility CLI and report:** experiments A–D, reproducible manifests/traces, exactness and resource evidence. This gates architecture acceptance.
2. **Project model/explorer and definitions:** final ordering, ownership, overlays, external targets and cancellation.
3. **Complete references and external hover:** verified coverage, persistent results, invalidation and fidelity fixtures.
4. **Distribution/soak:** actual VSIX, minimum/current editor matrix, restart/clean-runtime tests and explicit platform status.

Score proposal quality separately from implementation validation. The initial reviewer weighted semantic correctness 20, architecture/alternatives 20, footprint 10, Rust boundary 10, repository/tests 12, corpus/evaluation 12, hover/tree 8 and evidence/actionability 8. Its 71/100 initial assessment does not imply a 71% chance of meeting performance goals. Revised scoring and every empirical gate must retain an explicit “not established” status until measured.

Two workload details refine the experiment: whether Ben's monolith is one huge project or many, and whether a one-time bounded semantic indexing pass is acceptable. Default: no mandatory full-index wait blocks opening; bounded on-demand FCS is permitted; stale build outputs never certify current-source answers.

## Sources and provenance

All numerical corpus results derive from the read-only `research/corpus/measure_corpus.py` inventory and `metrics.json`; classification rules, generated-source limitations and full per-project results are preserved in the companion corpus research note. Counts exclude untracked generated outputs and unexpanded submodules.

| Corpus | Measured commit |
| --- | --- |
| [dotnet/fsharp](https://github.com/dotnet/fsharp) | `f896797a0f9b238a8cb1c482be424b714037ddc5` |
| [B2R2](https://github.com/B2R2-org/B2R2) | `5a9ba0e27d868ffb44831cf2ba98afb4be2d3904` |
| [Fable](https://github.com/fable-compiler/Fable) | `235bb0ffcfc580873aecb28eb19c192f430ee5fc` |
| [Grace](https://github.com/ScottArbeit/Grace) | `39960e07b73cd142cc0c1fd070c0591be91c1933` |
| [Darklang](https://github.com/darklang/dark) | `b6a3cb85d96e4d6386411a2a66e84e164f3815e3` |
| [aardvark.base](https://github.com/aardvark-platform/aardvark.base) | `7b40abfe264185dffffef5711fe9092542a80c54` |
| [resharper-fsharp](https://github.com/JetBrains/resharper-fsharp) | `dbabbb812239f653dc5ab3c2a5606cdc4dcdeb2f` |
| [FAKE](https://github.com/fsprojects/FAKE) | `1093050c8eb8fd7c8ab05cad1119c4b97d4cfa42` |
| [FsAutoComplete](https://github.com/ionide/FsAutoComplete) | `152fbbdd36339557576acec3f26c12c441089aa6` |
| [Paket](https://github.com/fsprojects/Paket) | `09fc3e7f71b6abd79943e58ed7cd94fd6016eb9b` |

1. FSAC [candidate filtering](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete/LspServers/AdaptiveServerState.fs) and [checker configuration](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/CompilerServiceInterface.fs).
2. F# [dependency-graph design](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/Driver/GraphChecking/Docs.md) and [transparent compiler](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/Service/TransparentCompiler.fs).
3. [FSharpChecker API](https://fsharp.github.io/fsharp-compiler-docs/reference/fsharp-compiler-codeanalysis-fsharpchecker.html) and [project analysis](https://fsharp.github.io/fsharp-compiler-docs/fcs/project.html).
4. [Ripgrep library components](https://github.com/BurntSushi/ripgrep/blob/master/crates/core/README.md), [Searcher buffer/file API](https://docs.rs/grep-searcher/latest/grep_searcher/struct.Searcher.html).
5. [Tree-sitter F#](https://github.com/ionide/tree-sitter-fsharp) and [Rust crate 0.3.12](https://docs.rs/crate/tree-sitter-fsharp/0.3.12/source/Cargo.toml). Versions observed, not yet selected dependency locks.
6. [SQLite controls](https://www.sqlite.org/pragma.html), [redb builder](https://docs.rs/redb/latest/redb/struct.Builder.html), [Tantivy writer budgets](https://docs.rs/tantivy/latest/tantivy/index/struct.Index.html).
7. FSAC [checked tooltip extraction](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/ParseAndCheckResults.fs), [signature formatting](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/SignatureFormatter.fs), [XML handling](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/TipFormatter.fs), [F# XML documentation](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/xml-documentation).
8. [F# typed-tree serialization](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/TypedTree/TypedTreePickle.fsi), [dotnetdll](https://github.com/nickbclifford/dotnetdll), [dotscope](https://github.com/ATRAPSLLC/dotscope).
9. [ProjInfo evaluation](https://github.com/ionide/proj-info/blob/cff02ece1f67f931e502726ba387832ed4fdd0d3/src/Ionide.ProjInfo/Library.fs), [MSBuild evaluated items](https://learn.microsoft.com/en-us/visualstudio/msbuild/evaluate-items-and-properties).
10. [Ionide ordered tree](https://github.com/ionide/ionide-vscode-fsharp/blob/12da8c46c3c6854ba46e93dd7b99dace14188a00/src/Components/SolutionExplorer.fs).
11. Example repo at `e18fd03ab412216c556e66451ac6bf7c5c0705e1`: [development](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/docs/DEVELOPMENT.md), [test launcher](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/cli/lib/testing.mjs), [fixtures](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/cli/lib/fixtures.mjs), [Safe Chain](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/nix/toolchain.nix), [UI test](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/test/ui/smoke.test.cjs).
12. [VS Code command API](https://code.visualstudio.com/api/references/commands) and [extension testing](https://code.visualstudio.com/api/working-with-extensions/testing-extension).
13. [Linux cgroup v2 metrics](https://docs.kernel.org/admin-guide/cgroup-v2.html).
