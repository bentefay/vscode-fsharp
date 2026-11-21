# External hover and ordered project model research

20 November 2026. Source investigation only; no extension or metadata-reader benchmark was implemented or executed.

## Recommendation

Put ownership rules, disk caching, XML-document indexing/rendering, tree construction, scheduling and process supervision in Rust. Keep a narrow, disposable .NET helper for actual MSBuild project evaluation and FCS symbol/signature interpretation. A full Rust port of F# signature decoding is a possible later research branch, not a prerequisite for a small dependable replacement.

External hover has two different costs: identifying what a source expression denotes, then displaying metadata/documentation for that identity. Caching solves much of the second cost. It cannot in general avoid the first: `x.Method()` may require inference through repository code even though `Method` is defined in a library.

## What current FsAutoComplete actually does

Inspected local clone at `152fbbdd36339557576acec3f26c12c441089aa6`.

- `AdaptiveFSharpLspServer.fs:1299–1388` obtains the open document and its type-check results before asking for enhanced tooltip information. It formats the signature as an F# code block, XML documentation as Markdown, optional documentation links, and footer lines.
- `ParseAndCheckResults.fs:417–540` extracts identifier context, calls both `GetToolTip` and `GetSymbolUseAtLocation`, and then calls `SignatureFormatter.getTooltipDetailsFromSymbolUse`. The formatting uses a symbol use's display context. The documentation identity follows `GetAbbreviatedParent` for aliases, while the symbol information records an assembly name.
- `SignatureFormatter.fs:895–965` dispatches over entity, constructor, operator, property, function/value, field, union case, active pattern and generic parameter symbols. It passes `symbol.DisplayContext` into the signature formatter. This is more than printing a CLR method signature.
- `TipFormatter.fs:1307–1471` locates XML near the DLL, supports localized XML, a netstandard documentation path workaround, uppercase extensions on Linux and a parent-directory fallback for reference assemblies. It stores parsed XML members in a process-wide dictionary. An explicit `inheritdoc cref` is resolved one level within the same assembly XML file in this path.
- `Sourcelink.fs:257–315` uses document-hash-derived temporary paths and reads SourceLink information from PDB metadata; the actual downloads are distinct from ordinary hover text. SourceLink success is not necessary to show documentation from XML.

These observations define the baseline to pin and test. They do not imply copying every accidental limitation or all optional keyword/help-link UI. The user's requirement is types and comments for external symbols, not keyword documentation or the whole documentation panel.

### Consequences for the cache

Separate immutable declaration data from use-site data:

| Record | Cache key/input | Lifetime |
| --- | --- | --- |
| Assembly catalog | Content hash/MVID plus assembly identity, reference selection and extractor version | Shared across workspaces using identical artifacts |
| XML member documentation | Actual XML content hash, culture, member ID and renderer version | Shared immutable document chunks |
| F# declaration signature | Assembly identity, exact symbol identity, FCS/formatter version and required dependency identity | Reusable declaration record |
| Hover binding | Source snapshot, project instance/TFM/defines, position and semantic dependency stamp | Invalidated by relevant source/project changes |
| Display-context rendering | Verified symbol binding plus formatting context | Cannot blindly reuse a global Markdown string |
| Source navigation target | Assembly/PDB identity, document checksum and member/declaration information | Persistent read-only source cache |

A declaration signature and a contextual hover are not necessarily textually identical. Overload selection, type-name qualification, aliases and display context must match the actual baseline. Generic specialization should be tested against the pinned FSAC behavior rather than assuming every generic use gets specialized in its displayed signature.

Do not promise a standalone FCS API that reads any DLL without a reference context. A practical helper can import assemblies through a tiny synthetic FCS project with the required exact reference closure, then export compact records. That avoids loading repository source, but the library closure can still be large and must be measured. An extractor must tolerate missing/trimmed F# metadata and unsupported compiler formats explicitly.

### Ownership must be checked before alias normalization

| Source target | Required outcome |
| --- | --- |
| External `List.map`, `HttpClient.SendAsync`, NuGet member | Show external hover |
| Local variable with an external inferred type | Suppress; selected symbol is local |
| Repository-defined type abbreviation pointing to a library type | Suppress on the alias symbol; do not normalize to external ownership first |
| Repository extension member on a framework type | Suppress; owner is the extension declaration |
| Workspace project encountered through a DLL reference | Suppress if it maps to workspace-owned source |
| Vendored library source within the open repository | Suppress under the user's rule |
| Cached SourceLink file outside repository roots | Still external; path location alone is insufficient |

Use project/source membership plus assembly-to-project ownership mappings. Do not classify with namespace prefixes, NuGet-like filenames, or `DeclarationLocation` path alone. The semantic response should carry the original symbol's ownership and normalized documentation identity separately. If ownership cannot be established, suppress or report an unresolved hover internally rather than incorrectly exposing repository hovers.

Concrete predicate: a declaration is workspace-owned if its source identity belongs to any loaded project/script's evaluated source set (including linked source outside the directory roots), or its owning project/assembly maps to a workspace project instance, or it is repository-owned/vendored source under a selected repository root. SourceLink downloads remain external unless independently included as workspace compilation source. Compare normalized host-specific canonical source identities, not string prefixes; retain logical link paths separately from physical identity. Only a semantically verified symbol failing every workspace-ownership test and positively mapped to an external reference gets an external hover. Classification is versioned with the project/source ownership manifest; switching solutions or linking a formerly external file invalidates affected hover records.

Cache reuse requires the relevant *whole semantic context* to be unchanged, not only the hovered file and DLL: compiler/language version, target framework, defines, ordered source inputs and their semantic dependency stamps, reference closure, project imports/configuration, and all pertinent dirty buffers. Until a narrower sound dependency fingerprint exists, invalidate broadly. XML parsing/rendering caches can remain valid independently of a source binding becoming stale.

## Rust metadata readers: useful, but not a replacement for F# semantics

The F# compiler overview explicitly separates ECMA-335 assembly metadata from F#'s additional binary resource data. It identifies `TypedTreePickle.fs` as the F# metadata reader/writer, and describes importing F# binaries into type/module/function/member typed-tree data. Embedded resources include signature and optimization data; compressed and other variants exist.

Rust can read ordinary .NET assemblies. Two examined primary-source examples:

- `dotnetdll`: ECMA-335 reader/writer, with high-level types/methods/resources and lazy/deferred reading. Its repository advertises Rust 1.85.1+ and testing against .NET 5+ assemblies. Its license is GPL-3.0-or-later, making it an inappropriate default dependency if this extension follows the reference repository's permissive distribution expectations without a deliberate license decision. Search results showed differing `0.3` docs and `0.4` repository examples, so versions need explicit pinning.
- `dotscope`: pure Rust CIL/metadata analysis with Apache-2.0 licensing and a broad ECMA-335-oriented API. Its stated feature set does not establish F# signature-data decoding, F#-idiomatic formatting, or equivalence to FCS.

No primary source found in this investigation established a mature Rust `FSharpSignatureData` decoder with the required F# fidelity. This is an evidence gap, not proof none exists. A CLR reader can locate opaque F# resources and decode CLR signatures; it does not automatically recover all F# source presentation. Building that capability would duplicate a compiler-specific format and still would not bind source use sites.

Recommended first implementation: Rust XML docs and indexing, with FCS returning neutral signature data or formatted fragments for F#-specific constructs. Explore Rust CLR extraction only if metadata extraction is measured as a meaningful ongoing cost; it is often a one-time per-package operation. Decompilation can remain an on-demand helper. Do not pay a large correctness/maintenance cost just to remove a short-lived extractor process.

## Ordered explorer: a key behavior absent from the original proposal

Inspected Ionide editor clone at `12da8c46c3c6854ba46e93dd7b99dace14188a00`.

`SolutionExplorer.fs:55–89` adds virtual paths in order. It only reuses a directory when it is the most recently added child at that level. Otherwise it creates another directory node. The root children are reversed on construction (`:143–154`), and folder children are reversed when exposed (`:424`). Therefore the sequence:

1. `A/one.fs`
2. `B/two.fs`
3. `A/three.fs`

can appear as two separate `A` runs, with `B` between them. A normal path trie that globally merges both `A` groups would change visible compile order. The Rust tree should preserve ordered directory runs and use stable occurrence-sensitive IDs; physical/virtual path alone is insufficient to identify repeated folder nodes. Test flattening the tree's leaf sequence against the source item order.

Other inspected behavior:

- Project items come from `proj.Items` as `(VirtualPath, FilePath)` (`:192–195`).
- Package references and project references precede file nodes (`:415–423`). Packages are deduplicated by name and project references by project path in this implementation.
- Solution folders/items preserve the server-provided order (`:289–314`).
- This particular head also discovers special directories such as `.deps`, `bin`, `obj`, `packages`, `docs`, `scripts`, `tools`; solution-level scans recurse to a bounded depth. Those are current-source behaviors, not necessarily the stable release behavior or requirements to reproduce. Benchmark cost and establish whether they belong in the accepted subset.
- Failed, unloaded, unrestored and unsupported project states remain visible.

The initial proposal's claim to show all noncompile project items deserves a compatibility fixture. Ionide.ProjInfo's current `ProjectOptions.Items` is assembled from compiler source files, whereas `AllItems` holds the broader MSBuild item data. The extension must distinguish “compiler ordered source tree” from optional noncompile files/solution items; adding content should not change compile leaf ordering or accidentally claim exact parity based only on an item enum.

## MSBuild: why `-getItem:Compile` is a useful preview but insufficient authority

Inspected Ionide.ProjInfo at `cff02ece1f67f931e502726ba387832ed4fdd0d3`.

`src/Ionide.ProjInfo/Library.fs:557–573` configures a design-time build with `ProvideCommandLineArgs`, `DesignTimeBuild`, `SkipCompilerExecution`, no package generation and no recursive project-reference builds. A target framework is selected as a project-instance property.

Its SDK target list (`:594–613`) includes design-time assembly/project/package resolution, reference-assembly selection, `_GenerateCompileDependencyCache`, `BeforeBuild`, `BeforeCompile` and `CoreCompile`. `CoreCompile` is used to obtain compiler arguments while compiler execution is skipped. Consequently a project can execute custom build logic during loading; a plain XML parser is neither equivalent nor a trusted-workspace substitute.

The authoritative source sequence is parsed from the generated F# compiler arguments (`:950–984`). That source list is then combined with evaluated `Compile` metadata to determine virtual paths. `getCompileItems` captures `Link`, full paths and other metadata (`:764–789`). `VisualTree.fs` normalizes virtual separators and uses the filename for a linked-outside-project path with no explicit `Link` metadata.

The current build helper explicitly sets `ShutdownInProcNodeOnBuildFinish` and disables node reuse (`:625–641`), with an explanatory comment that otherwise MSBuild 18 can leave a node thread retaining evaluated projects. This is a concrete existing memory fix, so the proposal should not assume every version of project loading behaves the same.

Recommendation: Rust can parse solution structure and cached manifests, but the final project-instance manifest should come from a pinned, short-lived ProjInfo/MSBuild helper using the workspace-selected SDK. Pure `-getItem` evaluation can produce a fast preliminary tree or a fallback when no target-generated inputs are present; it should not be claimed equivalent before comparing the final compiler arguments. Evaluate multi-targeted projects separately for the selected TFM and record configuration/defines/imports in the fingerprint.

## Suggested minimal helper boundary

Use a versioned request/response protocol shared with the Rust coordinator. All results identify the input snapshot and capability/version; cancellation is cooperative with a supervised process fallback.

| Operation | Input | Output |
| --- | --- | --- |
| `evaluate-project` | Project path, SDK identity, explicit global properties/TFM, trusted workspace context | Ordered compiler sources/arguments; project/assembly/package refs; visual paths and item metadata; output/ref-output identity; imports/configuration dependencies; load status/errors |
| `resolve-hover-target` | Project manifest version, complete dirty-buffer snapshot, file/UTF-16 position | Original symbol kind/ownership and identity; normalized documentation identity; exact selected overload; F# signature/display fragments; resolution status |
| `describe-assembly` | Exact DLL/reference-closure identities and extraction version | Compact declaration records with XML IDs, F# signature representation, source-location hints and metadata provenance |
| `resolve-external-definition` | Verified external symbol identity, DLL/PDB identities | Verified source mapping or on-demand signature/decompilation target; explicit missing-source status |

Rust owns the immutable on-disk catalogs, XML parsing/localization selection, bounded document cache, rendering and source downloads. Return enough neutral data to avoid keeping FCS objects alive. However, “structured signature representation” itself is a design choice to prototype; for the first parity experiment, returning exact formatter strings plus display-context identity is simpler and less risky than designing a full portable F# type algebra.

## Verification fixtures that should be added

- Hover ownership: local aliases to framework types, local variables of external types, repository extension methods, source project encountered through reference DLL, and vendored source.
- Signature fidelity: curried vs tupled arguments, generic constraints/SRTP, units of measure, records/unions, active patterns, F# abbreviations, operators/compiled names, overload selection and identical symbol under different `open` contexts.
- Docs: no XML, mismatched package-version XML, localized XML, uppercase extension on Linux, reference-assembly parent fallback, same-assembly explicit inherited docs and malformed XML. Pin desired doc style and normalize formatting only where behavior is semantically unchanged.
- Explorer: interleaved repeated directories; nested repeated directory runs; linked files outside root with/without `Link`; two files with identical basenames; conditional includes; multi-TFM compile lists; generated-before-compile sources; `.fsi` ordering; noncompile items; solution folders and missing project states.
- Resources: metadata import with a large reference closure; many packages with large XML docs; repeated project reevaluation to ensure no retained MSBuild processes/threads; cached hover without network; source-download cancellation and checksum mismatch.

## Primary sources and retrieval references

- [FSAC hover endpoint](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete/LspServers/AdaptiveFSharpLspServer.fs), [checked tooltip extraction](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/ParseAndCheckResults.fs), [signature formatting](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/SignatureFormatter.fs), [XML formatting](https://github.com/ionide/FsAutoComplete/blob/152fbbdd36339557576acec3f26c12c441089aa6/src/FsAutoComplete.Core/TipFormatter.fs). Inspected locally.
- [Ionide explorer](https://github.com/ionide/ionide-vscode-fsharp/blob/12da8c46c3c6854ba46e93dd7b99dace14188a00/src/Components/SolutionExplorer.fs). Inspected locally.
- [ProjInfo evaluation](https://github.com/ionide/proj-info/blob/cff02ece1f67f931e502726ba387832ed4fdd0d3/src/Ionide.ProjInfo/Library.fs), [visual paths](https://github.com/ionide/proj-info/blob/cff02ece1f67f931e502726ba387832ed4fdd0d3/src/Ionide.ProjInfo/VisualTree.fs). Inspected locally.
- [F# compiler architecture/metadata overview](https://github.com/dotnet/fsharp/blob/main/docs/overview.md), retrieval `turn166086549view1` / `turn121031680search1`.
- [F# resource variants discussion](https://github.com/dotnet/fsharp/issues/18591), retrieval `turn121031680search0`.
- [dotnetdll crate documentation](https://docs.rs/dotnetdll/latest/dotnetdll/), retrieval `turn328897560search0`; primary repository result `turn328897560search2` describes licensing/version constraints. Verify repository link/version before dependency selection.
- [dotscope repository](https://github.com/ATRAPSLLC/dotscope), retrieval `turn166086549view0`; [metadata module docs](https://docs.rs/dotscope/latest/dotscope/metadata/index.html), retrieval `turn328897560search4`.
- [ProjInfo package release notes](https://www.nuget.org/packages/Ionide.ProjInfo.ProjectSystem/), retrieval `turn328897560search7`, links the in-process MSBuild shutdown fix.

These are source-level findings, not measured memory or latency improvements. The original proposal's core feasibility warning remains correct: even external-only hover can need a substantial source dependency context.
