# Rust architecture research for lightweight F# navigation

Research date: 20 November 2026. Scope: source/documentation investigation, not an implemented or benchmarked extension.

## Recommendation

Make Rust the primary implementation language from the outset. Use a tiny TypeScript VS Code adapter, a Rust engine and automation CLI, and an isolated .NET/F# helper only for authoritative language semantics, project evaluation and F#-specific signature formatting where necessary. This changes the original proposal's Node-first coordinator, which no longer reflects the user's preference.

The credible performance thesis is **less retained compiler state, persistent compact verified results, bounded active working sets and avoided repeated work**. It is not merely "ripgrep is faster" or "parse only matching files." Both candidate filtering and AST-derived dependency reduction already exist upstream.

Full equivalence on arbitrary current F# source, very low peak memory and uniformly fast cold queries remain an unproven combination. No implementation strategy examined removes that tradeoff. The proposal should make resolving it the first experimental gate.

## Evidence that changes the plan

### Existing upstream optimizations

At FsAutoComplete commit `152fbbdd36339557576acec3f26c12c441089aa6`, `AdaptiveServerState.fs` lines around 2390–2445 already filter candidate reference files by symbol spelling and shortened attribute spelling before invoking FCS in the transparent-compiler path. `CompilerServiceInterface.fs` sets `captureIdentifiersWhenParsing = true`. This is directly inspected source, not a benchmark.

At dotnet/fsharp commit `f896797a0f9b238a8cb1c482be424b714037ddc5`, `Driver/GraphChecking/Docs.md` describes deriving conservative dependencies from parsed modules/namespaces, opens, qualified identifiers and `AutoOpen` handling. Its objective is to reduce the sequential dependency graph and parallelize type checking. This substantially overlaps the proposed syntactic candidate/dependency strategy. The document also identifies edge cases such as aliased `AutoOpenAttribute`; a homegrown binder cannot assume these are trivial.

In the inspected `Service/TransparentCompiler.fs`, `ComputeTcLastFile` parses snapshot source files, computes a graph, then processes that graph. `ComputeDependencyGraphForFile` contains a commented-out `Graph.subGraphFor lastFileIndex` optimization, and the active code removes implementation files backed by signatures instead. Therefore one must measure actual files parsed/checked for a request; the name of a single-file API does not prove it checks just one file or the minimal dependency cone. Do not patch compiler internals merely on the strength of this observation: correctness and merged signature state need study.

The same file exposes separate strong/weak cache counts for parsing, checking, framework imports, bootstrap info, intermediate type-check results, graphs, assemblies, classifications and item keys. These counts are not byte budgets. Lowering them can trade retained RAM for repeated work.

Sources:

- [FSAC candidate filtering](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete/LspServers/AdaptiveServerState.fs)
- [FSAC checker configuration](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/CompilerServiceInterface.fs)
- [Compiler graph design](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/Driver/GraphChecking/Docs.md)
- [Transparent compiler implementation](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/Service/TransparentCompiler.fs)

### Reusable Rust search machinery

Ripgrep's core command is not itself offered as an independent library, but its `grep-matcher`, `grep-regex`, `grep-searcher`, `grep-printer` and `ignore` components are reusable. `grep-searcher::Searcher` has both file and byte-slice APIs. The engine can use the same matching path for disk files and unsaved in-memory buffers, without spawning `rg` per document or writing temporary edited copies.

Use the evaluated compile-item manifest to enumerate semantic inputs. Ripgrep/ignore defaults skip ignored, hidden and some binary-like inputs, and do not follow symbolic links; blindly using them can omit legitimate linked/generated/project-included source. The `ignore` crate is useful for discovery, not authoritative scope definition.

For symbol candidate matching, literal/multi-pattern matching is often enough. Build a small inverted identifier-to-file index during syntax scanning; use grep as a fallback/recovery path. This is a design recommendation, not measured proof that a full-text index is needed.

Sources:

- [Ripgrep library decomposition](https://github.com/BurntSushi/ripgrep/blob/master/crates/core/README.md?plain=1) — web ref `turn381580386search9`
- [Searcher APIs](https://docs.rs/grep-searcher/latest/grep_searcher/struct.Searcher.html) — `turn412953200view0`
- [Ripgrep filtering behavior](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md) — `turn381580386search6`

### Tree-sitter is practical but not an F# semantic engine

The F# grammar ships Rust bindings, generated C parsers and separate source/signature grammars. Documentation inspected includes `tree-sitter-fsharp` 0.3.12, with `tree-sitter-language` 0.1 and Tree-sitter 0.26.8 in development dependencies. These are observed versions, not a proposed lockfile without compatibility testing. The upstream description still calls the grammar work in progress.

Use it for incremental parsing, declaration/identifier extraction, active-document lexical scopes and recovery while typing. A parse with ERROR/missing nodes should make candidate filtering less selective, never assert no possible reference. Grammar coverage alone cannot solve inference, overloads, SRTP, extension lookup, active patterns or generated/type-provider names.

Source: [Published Rust crate manifest](https://docs.rs/crate/tree-sitter-fsharp/0.3.12/source/Cargo.toml) — `turn315813551search2`; [upstream grammar](https://github.com/ionide/tree-sitter-fsharp).

## Recommended ownership by language

| Component | Language / approach | Reason |
| --- | --- | --- |
| VS Code commands, tree rendering and provider registration | Minimal TypeScript | Native extension API; no heavy analysis in the extension host |
| Engine and CLI | Rust | One reusable engine for editor and automated testing; explicit lifecycle/control |
| Document snapshots and UTF-8/UTF-16 positions | Rust | Single authoritative overlay model with versioned requests |
| Text scanning and syntax indexing | Rust, grep crates, Tree-sitter F# | Native processing; same path for disk and dirty buffers |
| Persistent results and manifests | Rust with SQLite via rusqlite initially | Transactional file/symbol indexes and snapshot invalidation |
| Worker scheduler, cancellation, memory accounting | Rust | Keeps .NET lifetime decisions outside .NET process |
| XML documentation reading/rendering and cache | Rust | Straightforward portable text/structured-data operation |
| SourceLink retrieval, hashing and cached source | Rust | Independent of compiler residency; preserve verified assembly/source identity |
| MSBuild evaluation / exact compiler options | Short-lived .NET helper or `dotnet msbuild` | Avoid rebuilding MSBuild semantics in Rust |
| Exact symbol resolution / inference / F# signature interpretation | Isolated FCS helper | Mature authority for F# semantics |
| Optional decompilation fallback | Existing .NET decompiler helper | Narrow specialized reuse rather than new decompiler |

Keep the `cli/bin/run` entry point, pinned Nix/devenv, command names, dry-run behavior and disposable-editor test workflow from the reference repo. Rust `clap` command modules can replace Clipanion internals while preserving the established UX; do not interpret "follow patterns" as a requirement to keep all orchestration in JavaScript. The small package/build tasks may still invoke locked Node tools.

Use a versioned stdio protocol, preferably JSON-RPC with request ID, snapshot ID and cancellation. LSP can be used where useful; transport choice does not determine compiler memory use. Rust owns a process budget across active projects rather than one unbounded compiler worker per project. Initially keep one semantic worker warm for a small working set and evict contexts according to measured reuse.

## Storage choice

| Option | Assessment |
| --- | --- |
| SQLite / rusqlite | Recommended initial choice. Exact symbol/file lookups, reverse references, dependency manifests and transactional replacement fit tables/indexes well. Explicitly size page caches and mmap, stream query results, checkpoint WAL, and bound connections. |
| redb | Credible all-Rust alternative for typed key/value and multimap storage. Requires more hand-built indexes/migrations. Explicit cache sizing is mandatory: inspected current builder source initializes a 1 GiB cache budget. This is a configured limit, not evidence of immediate 1 GiB resident use. |
| Tantivy | Defer. Its search/indexing facilities are useful for full-text ranking, but exact navigation mainly needs symbols, occurrence lists and transactionally versioned manifests. Writer/merge-thread budgets and segment lifecycle add complexity. Revisit only if measured textual search dominates. |
| Entire index in HashMaps | Reject as primary persistent design. Can be fast on small projects, but recreates an unbounded all-repository in-memory model. A bounded hot cache over disk records is appropriate. |

Do not treat any database cache setting as a total-process memory cap. SQLite query working memory, result collection, Rust allocations, syntax trees, mapped pages and workers remain separate.

Sources: [SQLite cache/mmap controls](https://www.sqlite.org/pragma.html) `turn828862268search3`; [SQLite WAL](https://www.sqlite.org/wal.html) `turn828862268search5`; [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/) `turn412953200view2`; [redb builder](https://docs.rs/redb/latest/redb/struct.Builder.html) `turn828862268search0`, [redb source](https://docs.rs/redb/latest/src/redb/db.rs.html) `turn828862268search1`; [Tantivy writer budgets](https://docs.rs/tantivy/latest/tantivy/index/struct.Index.html) `turn828862268search6`.

## Metadata strategy

Rust `dotnetdll` can parse ECMA-335 PE metadata, resources, members and signatures, and has skip/lazy reading options. It is a genuine candidate for lightweight CLR extraction. Its documented status is pre-1.0, and a CLR metadata library is not an F# signature decoder or formatter.

F# retains additional compiler-specific signature data. Inspected compiler source recognizes multiple uncompressed/compressed and companion resource names. `TypedTreePickle` is an internal module that serializes F# typed structures and resolves cross-assembly fixups. Writing a Rust decoder would couple the extension to compiler internals and still leave F# formatting and resolution work. Do not put this on the critical path.

Recommended first implementation: FCS extracts compact F# signature records in bounded batches; Rust owns the durable cache and XML documentation. A Rust-only CLR extractor is an optional subsequent optimization if extraction memory/latency is important. Never load executable library code merely to read ordinary metadata. Type providers are a separate semantic execution case, requiring the real project context and trust.

Sources: [dotnetdll README](https://github.com/nickbclifford/dotnetdll) `turn828862268search8`; [compiler signature resources](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/Driver/CompilerImports.fs); [internal typed-tree serialization](https://github.com/dotnet/fsharp/blob/f896797a0f9b238a8cb1c482be424b714037ddc5/src/Compiler/TypedTree/TypedTreePickle.fsi).

## Approaches considered and disposition

| Approach | Potential benefit | Main obstacle | Recommendation / evidence status |
| --- | --- | --- | --- |
| Text search only | Minimal state and fast scan | Cannot distinguish same spelling with different meaning; cannot resolve inferred members | Reject for exact navigation. Source/reasoning assessment; not benchmarked. |
| Tree-sitter + custom lexical binder | Fast local names/scopes | General F# member binding requires inference and .NET/F# semantics | Use narrowly proved fast paths; do not claim full parity. Not implemented. |
| Custom Rust F# type checker | Native control of all state | Reimplementing mature language semantics, type providers and version evolution | Reject initial project scope. No evidence this is a small task. |
| Tuned existing FSAC | Lowest implementation risk; baseline | Broad adaptive state/feature graph may remain; less Rust control | Essential measured comparator and possible fallback recommendation. No new benchmark run. |
| Minimal FCS helper + Rust engine | Reuse exact semantics, control work and retained state | Cold dependent-context work and large single projects remain | Recommended feasibility candidate; source-supported, performance unproven. |
| Offline persistent semantic index | Very cheap warm lookups, low retained RAM | Upfront type checking; freshness after edits; no free live-inference replacement | Strong complementary experiment. Index every symbol use from a checked file to amortize work, not one query result at a time. |
| Build-derived DLL/PDB index only | Reuses existing builds and external metadata | Missing local/reference semantics, stale artifacts and dirty buffers | Useful auxiliary data; reject as sole exact index. |
| Signature-based project shards | Reduce checking across true `.fsi`/assembly boundaries | Huge single-project inference, inferred API changes, compiler API limitations | Exploit real boundaries; do not invent cheap synthetic signatures. |
| Fork FCS for serializable state / minimal dependency slicing | Could reduce fundamental cold costs | Internal compiler correctness and maintenance risk | Research reserve if public APIs fail; independent review and upstream preference required. |

An index format such as SCIP could be useful for interchange, but the existing `sourcegraph/scip-dotnet` implementation explicitly targets C# and VB, not F#. It is not a ready-made F# indexer. [Source](https://github.com/sourcegraph/scip-dotnet) — `turn992554847search1`.

## Accuracy requirements that must survive optimization

- Dirty buffers must override disk content in **every** involved file, not only the file holding the cursor. FCS custom document sources/file-system integration must be exercised with multiple simultaneous edits.
- Freshness keys need project configuration, source order, compiler/SDK version, defines, target framework, assembly identities and source versions. Source hashes alone are insufficient.
- A compact index should store all verified occurrences obtained in a checked file so expensive analysis is amortized. Include stable external IDs and snapshot-scoped local IDs; do not persist FCS objects or assume symbol equality survives restarts.
- Candidate generation must have measured recall before semantic filtering. Attribute short names, symbolic operators, module/type aliases and language-generated/implicit uses require language-aware treatment. Define which implicit uses the pinned Ionide/FCS oracle actually reports, then test that behavior; do not promise an unspecified expanded reference model.
- If a source edit changes an inferred exported type, downstream cached results can be wrong even when those files are unchanged. Conservatively invalidate; only optimize with proven dependency/signature evidence.
- Hover suppression is by ownership of the selected symbol. A local variable of a library type receives no hover under the requested rule; a library method used in a repository file does. Type inference may still be required to establish ownership.
- "Complete references" requires verification of every eligible candidate for the selected workspace context. A timeout or memory cap is an explicit failed/incomplete query, never an empty or complete-looking answer.

FCS documents that partial checking can cause duplicate checks, and that its identifier fast-check mode is experimental. Instrumentation is needed to show whether each configuration saves work for this workload. [FSharpChecker API](https://fsharp.github.io/fsharp-compiler-docs/reference/fsharp-compiler-codeanalysis-fsharpchecker.html) — `turn992554847search0`; [file-system integration](https://fsharp.github.io/fsharp-compiler-docs/fcs/filesystem.html) — `turn992554847search2`.

## Experiments needed before architecture acceptance

1. **Instrumented semantic baseline:** pinned installed/current FSAC versus minimal FCS. Capture actual parse/check counts, graph/import work, allocation/GC activity, peak RSS/PSS, CPU and complete-query latency for cold/warm/edited states. FCS exposes parse/check counters; use tracing for phase detail.
2. **Candidate recall experiment:** compare Rust identifier/syntax candidate sets against all oracle reference files over a broad sample. Any false negative blocks using that filter for exact navigation. Include operators/aliases/attributes/implicit constructs and parser errors.
3. **Retained-state experiment:** vary the specific FCS strong/weak cache categories, worker idle lifetime and project working set. Report p95 plus worst-case cold-after-eviction, not just repeated-query medians.
4. **Disk-index experiment:** persist all uses from each checked file, restart the engine, replay queries, then mutate an early inferred API and several dirty files. Measure invalidation fan-out and time until complete correct answers return.
5. **Metadata experiment:** compare bounded FCS extraction plus Rust XML with Rust CLR extraction on FSharp.Core and representative NuGet dependencies; assert exact F# signature presentation and version selection.
6. **Architecture discrimination:** keep native search/index costs distinct from semantic costs. If FCS still consumes most time/RAM, rewriting more coordinator code in Rust is not presented as the solution.

No numeric performance result from these experiments exists yet. Original targets such as 300 MiB idle, 1 GiB active and sub-100 ms definitions are reasonable hypotheses to discuss but unsupported acceptance promises today. Sum-of-RSS includes shared-page duplication; record RSS for comparability plus PSS/private working set where available and identify the measurement method. A single-process GC limit is not a reliable process-tree memory bound.

## Critique of original plan

The original plan correctly recognizes semantic uncertainty, stale binaries, cached-state tradeoffs and the importance of a real monolith. Changes needed:

1. Replace Node-first with Rust-first immediately to honor the new requirement.
2. Add graph-based checking to the existing-upstream-optimizations discussion; otherwise the proposal overstates novelty.
3. State that metadata extraction and low-retention FCS are hypotheses until measured; a short-lived process may still have a large peak.
4. Define oracle parity for implicit references instead of broadly claiming all implicit uses.
5. Add an explicit permanent-index strategy extracting all uses per checked file, plus sound snapshot invalidation and complete-search coverage records.
6. Distinguish source inspection, researched options, implemented probes and benchmarks in the evidence ledger. Do not label investigated ideas as "tried".
7. Include a persistent-index warmup option in the proposal without assuming user acceptance; it may be the only feasible way to combine low retained RAM and fast steady-state navigation.
8. Avoid selecting SQLite versus redb on Rust purity alone. Resource controls, correctness, migration simplicity and measured workload behavior should decide.

This recommendation is a researched design direction. Its central feasibility question remains open until the semantic benchmark runs on the largest relevant project and a representative application monolith.
