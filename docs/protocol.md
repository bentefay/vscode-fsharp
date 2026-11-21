# F# Navigator prototype protocol v1

Transport: UTF-8 JSON lines on stdin/stdout of `fsharp-navigator daemon`.
Diagnostics/logging go to stderr. One JSON object per line. Requests are
`{"id":1,"method":"workspace/open","params":{...}}`; replies echo `id`
with either `result` or `error:{code,message}`. Paths are absolute filesystem
paths on the workspace host. Positions/ranges are zero-based UTF-16, matching
VS Code. Replies contain file URIs for navigation locations.

## Modes and performance contract

Modes: `lexical`, `semantic`, `indexed`; `syntax` may be accepted as an alias
for `lexical`. Default is **indexed**. Routine indexed/lexical operations never
silently start FCS or load a monolith into a compiler. Semantic mode is an
explicit experimental/oracle mode. It requires a configured helper and may be
expensive. A missing exact answer is reported, not fabricated or silently
upgraded to expensive analysis. Project evaluation/library extraction are
separate explicit, bounded helper operations.

An indexed provider request can visit 512 entries while discovering files, reading project declarations and selecting a prepared graph,
read 128 source/project/manifest/snapshot files totaling 4 MiB and materialize
64 cold syntax summaries. Validated postings and existing summaries avoid
cold reads and parses. Exhaustion returns `unavailable` and discards the
incomplete proof. The explicit index and explorer operations can establish
larger source domains. A cold model has a separate certificate budget of 128 MiB of uncached
digest reads and 32,768 input/selection-directory visits. Content digest reuse
within one validation generation avoids charging repeated hashes. The bounds cover
source and project-model work. Explicit library artifacts have separate ingestion
and validation paths. SQLite row visits and allocator/RSS usage are distinct.

## Requests

| Method | Parameters |
| --- | --- |
| `workspace/open` | `root`, optional `mode` (indexed), `helperPath`, `helperArgs`, `cacheDir`, `evaluatedModelPath`, `libraryCatalogPath`, `semanticSnapshotPath`, `indexCacheMb` (8), `overlayBudgetMb` (32), `helperMemoryLimitMb` (256) |
| `workspace/tree` | none |
| `workspace/index` | Builds or refreshes Rust source postings and imports an explicitly configured semantic snapshot. Available in the editor as **F# Navigator: Index Workspace**. |
| `workspace/evaluate` | explicit request for a bounded project-evaluation helper |
| `workspace/mode` | `mode` |
| `workspace/stats` | none |
| `metadata/import` | `path` to an explicitly exported library declaration catalog |
| `semantic/import` | `path` to an explicitly exported clean semantic snapshot; never launches FCS |
| `document/open`, `document/change` | `path`, complete `text`, monotonically increasing integer `version` |
| `document/close` | `path` |
| `navigation/definition`, `navigation/references` | `path`, `line`, `character`; references optionally `includeDeclaration` |
| `navigation/hover` | `path`, `line`, `character` |
| `shutdown` | none; reply then exit |

Mutation replies acknowledge the accepted version/snapshot. Navigation results
must correspond to the current accepted document snapshot. Dirty-buffer text
overrides disk content in all relevant files. Unknown methods produce errors.
The native daemon is sequential: `$/cancelRequest` cannot preempt a running
request. The adapter cancels its pending promise and discards late results;
bounded/coalesced queues prevent repeated edits from retaining every text
snapshot. This does not implement foreground-priority scheduling.

## Navigation result shapes

Definition/reference result:

```json
{"locations":[{"uri":"file:///repo/A.fs","range":{"start":{"line":0,"character":4},"end":{"line":0,"character":8}}}],"completeness":"exact","source":"indexed","diagnostics":[]}
```

`completeness` is `exact`, `approximate`, or `unavailable`. The lexical engine
must not label a mere same-spelling search exact. `source` identifies the engine
or fast path actually used. Diagnostics are optional strings. Standard editor
providers may expose only exact results; a separate candidate command can show
approximate results explicitly. Cancellation/limits must not resemble a complete
empty reference list.

For indexed definition and reference requests, an explicit matching overlay
version enables a document-local proof before project discovery. Eligible
callback lambda and nested function parameters return `source: "local-lexical"`,
`proofScope: "document"`, the acknowledged `sourceHash` (BLAKE3), and
`documentProofId`. The `proof` object records the compiled engine and parser
identities, algorithm version, UTF-16 declaration range and half-open body
byte offsets. Document-scoped results omit `projectContextId` and carry the
workspace snapshot current at dispatch. A stale version is a request error.

The process-local cache retains positive serialized answers for up to 32
distinct requests within 128 KiB of accounted storage. An acknowledged edit or
close invalidates its document's entries. A mode change or cancelled request
clears the cache. Workspace stats expose `localProofAttempts`, `localProofHits`,
`localProofCacheHits`, `localProofCacheEntries`, `localProofCacheBytes`,
`localProofParsedBytes`, `localProofVisitedNodes`, `localProofParseMicros` and
`localProofMicros`. Parse and proof metrics include unsuccessful work. Cache
hits reuse the answer without parsing the document.

Hover result:

```json
{"hover":{"markdown":"```fsharp\nval map : ...\n```","range":{"start":{"line":0,"character":4},"end":{"line":0,"character":8}}},"completeness":"exact","source":"semantic"}
```

`hover` is null for repository/workspace-owned symbols, unavailable metadata or
unsupported binding. Ownership is the original symbol's ownership, before
normalizing an alias. Cached library metadata does not imply a use-site binding
has been semantically established.

## Tree and helper integration

Tree result: `{nodes:[{id,label,kind,path?,uri?,children?}],projects:[...],completeness,source}`.
Stats are engine-owned JSON describing memory/index/query/helper counters.

Evaluated project artifacts separate certificates: `inputs`/`inputHashes`
(`projectInputHashes` alias) cover project order and configuration; DLL byte
fingerprints live in `semanticInputHashes`. Per-project `referenceBindings`
record selected path/identity/existence/ownership. Missing configuration files
are negative dependencies with `sha256: "missing"`. Explicit semantic snapshots
retain both project and binary inputs. Library catalog `inputHashes` cover the
recursive compiler-selected assembly dependency closure, selected XML bytes and
ordered XML candidate-prefix witnesses (`missing`/`directory` are explicit states).
Catalogs and import environments use schema2, factsVersion2 and the enforced
binary-facts-v2 producer contract. Producer-only implementation hashes are
provenance; selected dual-role files remain live. `dependencyBindings` record the
selected identities. Older readers reject schema2 rather than ignoring its guards.
`indexCacheMb` is only SQLite's page-cache budget; `overlayBudgetMb` bounds
accepted dirty text. Neither is a hard native RSS limit. Native file input is
also guarded by `maxFileBytes` (8 MiB default). `helperMemoryLimitMb` is a sampled
RSS guard for the explicitly requested semantic helper; `memoryLimitMb` is a
compatibility alias. Observed memory and configured budgets must be distinct.
The implementation is authoritative and tests pin it. Preserve ordered runs
of folders rather than globally merging paths. Native project parsing must
report its evaluation limitations; exact MSBuild order is available only from
an evaluated manifest.

The separate .NET helper also uses request/reply JSON lines. It implements
`workspace/evaluate`, `metadata/export`, `semantic/export` and
`navigation/definition`, `navigation/references`, `navigation/hover`.
The CLI `prepare` command evaluates project inputs and exports metadata through
one-shot helper invocations. `--semantic-snapshot` explicitly opts into source
checking and persisted occurrence export. Imported source/compiler/build
fingerprints are validated before reuse. Imported semantic results become
unavailable after a source/context change; no compiler repair is implicit.
Semantic requests include `root`, cursor parameters and a
`documents` array of `{path,text,version}` overlays. The Rust owner and helper
owner must keep their adapter contract synchronized. No helper starts merely
because the extension activates in indexed mode.

Library catalogs are schema version 1 JSON objects with `completeness:"exact"`,
`inputHashes:[{path,sha256}]` and either `symbols:[...]` or `catalogs:[...]`.
`catalogs` names catalog shards relative to the manifest directory (absolute
paths are also accepted). Each symbol has `id`, `fullName`, `qualifiedNames`,
`assemblyPath`, `signature` and `documentation`. Import reads one symbol at a
time into SQLite, verifies supplied input hashes, and records content and file
identity stamps. Persisted rows are reused only after streaming SHA256 validation
of their captured inputs on the first external lookup in each workspace session;
this cost is deferred past startup and local navigation. Warm Unix validation
includes change time, modification time, size, device and inode, detecting
same-length in-place edits even when modification time is restored. Non-Unix
validation conservatively hashes inputs because the same change-time certificate
is unavailable. Normal qualified-name lookups retrieve at most two distinct
matches so ambiguous declarations remain unavailable. Cancellation during import
rolls back the replacement transaction and cannot certify a partial catalog.
Activation defers catalog import until needed. The explicit CLI preparation
helper can stream each shard directly to `outputPath`, returning only its count
and input hashes over JSON lines.


Evaluated project and optional semantic-snapshot inputs now require the development observed-environment contract described in [environment-certificates.md](environment-certificates.md): per-project `environmentCertificate` version3, `membershipReusable:true`, and `sourceMembership` witnesses. Input hashes can distinguish `missing`, `directory`, or file SHA256. Imported models from earlier development contracts must be prepared again; clients must not rewrite their version fields. These checks run on first demand and do not authorize any automatic compiler fallback.

The additive `callableInfo` on a version2 catalog symbol carries
`callableFactsVersion:1`, `identity`, `declaringModule`, logical `sourceFullName`,
the exact `xmlDocId`, `genericArity`, `curriedArguments:[[TypeExpr]]`, `returnType`
and `supported`. Its physical identity is the selected assembly path/name plus
compiled module/member name, matching the complete term inventory. Selecting a
function through a partial catalog is insufficient: the current type/scope/term
environment must prove that declaration first, then the catalog row must match
both identity and XML ID. Older catalogs with no callable facts do not prove a
scheme; unknown callable fact versions are rejected.

Portable type expressions now include `array {element,rank}` and
`function {domain,range}`, alongside named types and indexed generic slots.
Function-level slots are `methodParameter`; enclosing type slots remain
`typeParameter`. Current callable facts cover ordinary module functions with
unconstrained generic parameters; constraints, SRTP, measures, byrefs/native
pointers, optional/paramarray arguments and unrepresentable tuples are explicitly
unsupported. Ordinary inline functions such as the selected Core pipeline
operator remain representable. Source identities may be supplied by a current
native proof during substitution; they are never accepted from a catalog.

These schemes describe declarations, not inference order. The initial native
consumer may use a proven pipeline input to constrain an immediately applied
literal lambda. It must not use a later direct-call argument to retroactively
type an earlier callback. Source/module/operator shadowing remains a separate
binding proof. `semantic/tests/callable_facts.py` records independent real FCS
instantiation and pipeline/direct-call evidence; it does not certify the native
consumer by itself.

The helper-only `metadata/prepare-experimental` method is an explicit one-shot
binary preparation job, not a daemon navigation request. It accepts the complete
`referencePaths` and `workspaceAssemblyPaths`, selected external `assemblyPaths`,
a new `outputDirectory`, and optional sampled `maxWorkingSetMb` and deadline
`timeoutMilliseconds`. It shares one imported binary context across the selected
catalogs and complete inventory, validates their combined live inputs, publishes
a new generation, and exits. The response includes schema2 producer provenance,
`catalogPath`, `environmentPath`, `importCount:1` and `sourceProjectChecks:0`.
The latter excludes repository projects; the importer still checks a dummy `()`
expression. Experimental CLI flags and publication/resource limitations are in
[shared-metadata-experiment.md](shared-metadata-experiment.md). No default
navigation or preparation path switches to this experiment implicitly.
