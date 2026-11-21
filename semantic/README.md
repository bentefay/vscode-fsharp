# Explicit .NET helper

The default Rust engine does not start this program. The helper offers separately
invoked project evaluation, external assembly catalog export, and correctness
experiments. These operations are not a lightweight replacement for semantic
analysis; their startup time, peak process-tree memory and output size belong in
cold end-to-end measurements.

Build with `scripts/with-local-tools dotnet build semantic/FSharpNavigator.Semantic.fsproj -c Release`.
Launch `dotnet semantic/bin/Release/net10.0/FSharpNavigator.Semantic.dll --once`.
Without `--once` the process reads multiple requests, intended only for explicitly
selected oracle experiments. The pinned SDK supplies FSharp.Core and FCS, with no
second compiler-service NuGet package. The custom source-overlay FCS API is
experimental and tied to that SDK.

Every stdin line is `{ "id": 1, "method": "...", "params": { ... } }`.
Every stdout line is `{ "id": 1, "result": ... }` or an `error` object.
Diagnostic logging goes to stderr. `--once` exits after one request. MSBuild node
reuse, the MSBuild server and shared compilation are disabled for owned child
processes. There is no global build-server shutdown.

## Project evaluation

`workspace/evaluate` accepts `root`, optional `solution` (`.sln`, `.slnx`, `.fsproj`),
`projects`, `configuration` (default Debug), and `targetFramework` (default first
evaluated target). Projects must already have valid restored assets; the helper
does not restore or build reference assemblies automatically. Unbuilt project-reference DLLs are represented by `sha256: "missing"`; importers must verify that those paths remain absent.

A design-time `Compile` target with compiler execution disabled yields final
`FscCommandLineArgs`. A nonexistent output sentinel prevents up-to-date checks
from silently omitting those arguments. `files` and `items` preserve compiler
source order, including generated and linked sources. `virtualPath` carries Link
metadata independently of the physical path, so `A/a.fs, B/b.fs, A/c.fs` remains
three ordered folder runs in the consumer. Referenced F# projects are evaluated
recursively. `workspaceAssemblyReferences` identifies evaluated project-reference binaries
and own target/reference output paths. `inputs`, `inputHashes` and the explicit
`projectInputHashes` alias certify **project shape**: project, reported imports,
assets, selected solution, and ancestor configuration paths. Missing ambient
configuration paths carry `sha256: "missing"` so their creation invalidates the
certificate. Referenced DLL bytes are deliberately separate in
`semanticInputHashes`: an ordinary workspace rebuild does not change compiler
source order. Each project's `referenceBindings` records selected assembly path,
identity, existence and workspace ownership; creation/removal or changed
reference resolution requires reevaluation. This is not a promise that arbitrary
environment-driven MSBuild logic or custom targets reading DLL contents can be
cached without reevaluation. `membershipReusable` and `sourceMembership` separately
certify literal wildcard domains by UTF-8-byte-sorted slash-relative filenames,
NUL separators, SHA256 and count. Explicit-only source lists gain no source-directory domain; SDK import wildcard hooks retain separate filename domains, including initially empty directories.
Unknown item expressions, symlinks and unsupported wildcard extensions refuse
persistent reuse. Evaluated NuGetPackageRoot and defining-file imports retain
missing-to-present probes. Only a narrowly validated analyzer-only package-target
structure is source-neutral; arbitrary tasks, compiler/source mutations and
unsafe property functions remain unsupported. Mixed-language project references remain
binary references. Script projects and simultaneous multiple target contexts are
not implemented.

Observed environment/import certification is currently bounded to the tested ordinary .NET SDK10.0.400 path. An additional names-only logger traces SDK/TFM selection and final Compile using property read/assignment events; it expands import paths at the event's property state. The certificate records inherited consumed names with SHA256 or a missing marker, plus initially absent property references from actual imported XML. It excludes fixed helper overrides and globals common to all actual invocation contexts. `MSBuildThisFile*` properties are derived from each defining import file. No plaintext inherited environment values are persisted.

The current project contract is version4 (`dotnet-msbuild-observed-v4`); old v3 models are rejected after a real culture-sensitive glob counterexample. Its initial globalization profile is Linux x64 / SDK10.0.400 / runtime10.0.11 / glibc2.41 with one installed SDK/runtime/fxr choice. Actual MSBuild culture is preserved and observed; mandatory locale/host selectors, runtime/ICU files, candidate directories and symlink states are live dependencies. Unsupported loader/globalization overrides and non-C/POSIX libc bootstrap states refuse reuse. SDK100/SDK11 and Nix profile reuse remain unverified. See `docs/environment-certificates.md`. SDK imports add filename membership witnesses for matching suffixes or all files (`extensions:["*"]`), with `sha256:"directory"` existence witnesses distinct from missing/file hashes. A new `obj/P.fsproj.extra.targets` or previously empty extensionless ImportBefore hook must invalidate the old model. Validation is lazy; native startup still reads no source bodies. Custom SDK/resolver folders, wrapper executables, arbitrary tasks and IO/environment property functions remain unsupported. Narrow pure String.Copy/Length/numeric Substring and MSBuild.Modulo expressions keep their ordinary property-argument dependencies. This is not a general MSBuild dependency oracle.

Primary implementation references: [MSBuild property tracking](https://source.dot.net/Microsoft.Build/Evaluation/PropertyTrackingEvaluatorDataWrapper.cs.html), [property-instance read events](https://source.dot.net/Microsoft.Build/Instance/ProjectPropertyInstance.cs.html), and [intrinsic arithmetic](https://source.dot.net/Microsoft.Build/Evaluation/IntrinsicFunctions.cs.html). Pinned runtime probes are retained under `test-results/recovery`; online current source alone is not a runtime guarantee.

Design-time MSBuild uses one worker and `BuildInParallel=false`, with node reuse
disabled. This bounds the explicit helper's process lifetime; a four-project
signature-alias fixture verifies no owned MSBuild nodes remain after exit.

## External declaration catalog

`metadata/export` accepts `assemblyPaths`, optional dependency `referencePaths`,
and optional `outputPath`. With `outputPath`, symbols stream directly to a new
catalog artifact and stdout returns only `outputPath`, `symbolCount`, input hashes
and completeness. The output must not already exist; a temporary file is renamed
only after successful extraction and input validation. This avoids duplicate
in-memory catalog/JSON representations. Request one assembly per fresh process
to limit accumulated symbol objects; FCS dependency metadata and a single large
assembly can still dominate preparation memory.
It imports those binaries through a small synthetic FCS script with explicit
reference flags, never workspace source files. Evaluated references replace script
defaults, including a selected NuGet FSharp.Core version. It emits schema version 2 and `factsVersion: 2` with
`symbols` containing stable assembly-content/XML-ID identities, logical qualified
names, F# declaration signatures and adjacent XML documentation. Private APIs are
excluded. XML summary, remarks, examples, parameter/type-parameter and return text are supported. Inline `see` langwords/references, `paramref`, `typeparamref`, paragraphs and code preserve meaning; raw text is Markdown-escaped and XML links are not emitted as active links. DTDs are prohibited, each member is bounded to65536characters/8192nodes/depth32, and a document has a64MiB character limit. Exceeding these bounds rejects the extraction instead of silently truncating exact comments. Full documentation formatting and arbitrary external inheritdoc resolution are not implemented.

Catalog `inputHashes` cover the exported binaries, their recursive PE AssemblyRef
dependency closure, additional assemblies in exported FCS types/constraints,
the selected XML bytes and ordered candidate-prefix states. Producer-only
helper/compiler hashes are versioned provenance under the enforced binary-facts-v2
contract; a producer file also selected as a library remains a live dependency.
See `docs/metadata-producer-contract.md` for schema migration, revocation and the
Linux x64 verified XML adapter boundary. The FCS graph is
essential: an erased unit of measure can reference an assembly omitted from PE
AssemblyRefs. It traverses generic arguments, abbreviations, constraints, base
types, interfaces, delegate/member/field/union signatures. Nominal entity
abbreviations are followed one link at a time: the public type-level accessor
can collapse a chain and otherwise omit an intermediate assembly. Unresolved traversal
or signature formatting rejects certification rather than caching a placeholder.
Dependency names resolve through the **actual FCS-selected assembly set** from
the same compiler reference context. `dependencyBindings` records both the
requested and resolved full identities and selected path; normal framework
version unification follows FCS's selected-CCU rule. Culture/key mismatches,
unresolved dependencies, competing selected identities and multi-module
assemblies are rejected. This also follows facade/type-forwarder AssemblyRefs.
Unrelated workspace DLLs supplied in the reference context do not enter a
catalog's validity certificate; a workspace DLL that is truly referenced remains
a dependency. Native cache invalidation is currently for the whole imported
catalog bundle, so a stale shard can still invalidate unrelated shards in that
same bundle. Provided types are rejected because dynamic type-provider inputs
are not certified by this declaration-only format; arbitrary external
`inheritdoc` resolution is also not implemented.

Structured `typeInfo` keeps declaration assembly identity separate from its
canonical alias target, generic slots, base type and member names. `memberInfo`
certifies non-indexed readable properties and return types. Compiled accessors
keep get_/set_ names rather than occupying the property lookup key. New native
nominal consumers require facts version 2; older signature-only facts cannot
prove a new type binding or absence of a competing declaration.

`metadata/environment` takes all effective `referencePaths` and explicit
`workspaceAssemblyPaths` exclusions. It emits a facts-version-2 builtin-environment
certificate scoped to `selected-reference-declaration-inventory`, including a
complete `typeNames` map and requested builtin/pattern maps. Internal types and
active-pattern CASE names are included. Selected reference bindings must cover
the current project; native source ownership and ordered lexical lookup are
still separate proofs. Every nominal identity resolves to a TypeDef/forwarder,
and all selected/reference bytes are hashed. Private method-body dependencies
are not dependencies of this name-only certificate; signature catalogs retain
the stronger traversal above. Unresolved/provided inventories are rejected.
The complete inventory can be substantial and is not a fixed small-memory promise.

Catalog signatures are declaration types. The catalog does not resolve overloads,
receiver types, lexical shadowing, inferred generic arguments or use-site types.
Native consumers must prove the binding or return unavailable. `isExternal` means
binary provenance relative to this extraction request: the caller must exclude
workspace output assemblies and suppress workspace-owned symbols using the
evaluated model, including linked sources outside the root. Do not export a
workspace's own project DLL as an external library catalog.

## Explicit correctness oracle

`navigation/definition`, `navigation/references`, and `navigation/hover` accept the
project selection parameters plus absolute `path`, zero-based UTF-16 `line` and
`character`, and `documents: [{path,text,version}]`. Each request replaces the entire
set of dirty overlays. References accepts `includeDeclaration` (default true).

The oracle reevaluates the project model and invalidates FCS for each request.
That conservative policy avoids stale overlay results and makes it a **cold
correctness experiment, not a fair performance baseline for Ionide or warm FCS**.
References check every evaluated source file and refuse an exact result if any
file has compiler errors. Foreground file checks collect symbol uses without
retaining all background symbol uses. Hover suppresses evaluated source ownership
(including linked sources), workspace paths and workspace output identities.
External definitions without local source return a generated signature document;
SourceLink retrieval is not implemented. Signature display is a compact FCS type
format, not the complete Ionide tooltip formatter. Cursor extraction currently
supports identifiers/backticks/common operators; scripts and exotic syntax need
additional fixtures before claiming parity.

## Explicit persisted semantic snapshot experiment

`semantic/export` accepts the same clean project-selection context, plus optional
`project` to export just one evaluated project. Dirty `documents` are rejected.
It emits `kind: "semantic-snapshot"`, `schemaVersion: 1`, `factsVersion: 2`, `workspaceProjects`, and
`projects` with:

- `projectPath`, `contextId`, `targetFramework`, `configuration`, `compilerArgs`, and `dependencies`;
- `sourceHashes` and `inputHashes`, each `{path,sha256}`;
- `occurrences`, each `{symbolId,path,range,isDefinition,declaration}`;
- `completeness` and `diagnostics`.

Source IDs include the evaluated owner project context and declaration anchor;
linked-file contexts remain distinct. External IDs include the assembly identity
and XML member identity. FCS equivalence groups normalize constructor/type and signature/implementation
uses. Source abbreviations instead use occurrence-specific GetUsesOfSymbolInFile
reference groups, with declaration anchors aligning signature/implementation
aliases. Constructor occurrences keep their original external definition target;
erased binary constructor identity never merges distinct source aliases.
Conflicting alias claims reject completeness. Non-equivalent symbols sharing the same detailed identity make the project
unavailable. Independently generated partial exports must not be merged to claim
whole-workspace coverage: canonical group representatives are chosen within one
export invocation. Any compiler error or input mutation during export discards
that project's occurrences; a workspace input change detected at completion
discards all output occurrences. Each project is checked file by file. An explicit
one-project invocation can still require FCS to check source dependencies; there
is no claim that one project bounds a transitive project's peak memory.

An importer must validate all SHA256 stamps and evaluated context before serving
results, and require complete coverage of `workspaceProjects` for whole-workspace
references. The initial Rust experiment invalidates all imported semantic data
when any source/context changes. Compiler/configuration/environment changes need
reevaluation and a fresh export. The helper performs no automatic offline index
build after an edit. Export time/memory and the interval of unavailable results
must be counted separately from warm disk-query latency.

Run `python3 semantic/tests/smoke.py` for real SDK fixtures covering declaration
catalogs, ordered linked sources, solution selection, dirty overlays, library-only
hovers, exact source references and stamped semantic export.
# Structured callable facts

`metadata/export` also emits versioned `callableInfo` for ordinary module
functions. The scheme preserves physical declaration identity, logical source
name, exact XML ID, generic slots, curried argument groups and return type.
Arrays retain their rank; function arrows retain domain/range. Unsupported
constraints, SRTP, measures, byrefs and optional arguments are marked unusable.
This preparation step reads binary declarations; it does not type-check an open
workspace or authorize a default compiler fallback.

Run `scripts/with-local-tools python3 semantic/tests/callable_facts.py` for the
actual binary/FCS tests. They check both Core `Array.choose` and a separately
named library callback function against `FSharpSymbolUse.GenericArguments` and
`FSharpType.Instantiate`, and retain the direct-call versus pipeline inference
order difference. Native code must independently prove the selected function
and pipeline operator through the complete current imported term environment.

The opt-in `metadata/prepare-experimental` method shares one binary import across
selected catalogs and the complete external inventory, publishes a new generation
only after revalidation, and then exits. The default methods remain independent
controls. Its contract, resource limits and pending comparison are documented in
[`docs/shared-metadata-experiment.md`](../docs/shared-metadata-experiment.md).

