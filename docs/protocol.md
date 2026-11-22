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

### Progressive discovery in the default daemon

For an unprepared indexed workspace, a nonlocal navigation demand requests
background preparation. A document-local proof remains available before this
work. If the foreground discovery budget is insufficient, subsequent requests
receive an explicitly unavailable, pending result while the daemon resumes a
separate discovery cursor between requests. No compiler, project-evaluation
helper or semantic snapshot import is scheduled by this work.

Partial traversal never becomes the live project model. Once traversal finishes,
the daemon publishes the raw inventory and updates source postings in separate
idle slices. Background raw-project membership remains provisional: it can
support current-source, explicitly incomplete candidates, but cannot certify a
non-document exact binding or complete references. `ready` means the bounded
discovery/index job completed, not compiler-certified project correctness.
An explicitly supplied evaluated model retains its existing validation rules.

`workspace/stats.background` reports `phase`, `discoveryComplete`,
`indexComplete`, `entriesVisited`, `projectsDiscovered`, `filesDiscovered`,
`filesIndexed`, `filesReused`, `slices`, `generation`, `reason` and
`modelProvisional`. Pending `workspace/tree` replies expose an incomplete tree
instead of implicitly completing the scan for a linked-buffer membership check.
The existing explicit `workspace/index` command remains separate from automatic
preparation and may import an explicitly configured semantic snapshot. When an
automatic provisional model is already loaded, the command reconstructs it using
the existing explicit discovery/evaluated-model installation path before indexing.
It does not promote the old project's flags in place. The provisional guard is
cleared only after successful installation and input-event validation; cancellation,
failed discovery or changed membership retains guarded behavior. Unsupported
imports remain inexact under the existing explicit discovery rules, and no
compiler is launched implicitly.

Restart does not trust a persisted project inventory. It rediscovers membership
and reuses SQLite facts through the existing current-source validation. New
project/configuration inputs, directory membership changes and lost watcher
events invalidate the discovered generation. Filesystem calls and individual
bounded project/source operations are not hard real-time deadlines; a sustained
nonempty foreground queue can postpone idle work.

The scanner yields after at most 32 admitted entries or one project read;
postings advance one source file per idle slice. Final publication also sorts
the bounded inventory, reconciles index membership and installs source watches;
that atomic step is not a 32-entry slice and may delay an arriving request.
Processing a bounded watcher journal can also refresh several changed sources;
cooperative cancellation applies between those operations.
Retained discovery limits are
250,000 entries, 16,384 directories, depth 64, 2,048 projects, 100,000 source
paths/aggregate Compile inputs, 32 MiB of accounted paths and 64 MiB of project
input bytes. A project XML read is at most 1 MiB, with a separate 8 MiB limit on
its expanded path strings. Source reads retain `maxFileBytes`; watcher journals
hold at most 1,024 events and overflow invalidates the generation. These limits
bound selected work units and retained inputs, not total process RSS. A terminal
limit is reported as incomplete rather than silently restarting the same scan
on every query.

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
| `navigation/documentDefinition` | Indexed-only; `path`, `line`, `character`, mandatory current overlay `version`/`documentVersion`; document proof or explicit miss |
| `navigation/possibleReferences` | `path`, `line`, `character`; optional current overlay `version`/`documentVersion`, `expectedSourceText` |
| `navigation/hover` | `path`, `line`, `character` |
| `shutdown` | none; reply then exit |

Mutation replies acknowledge the accepted version/snapshot. Navigation results
must correspond to the current accepted document snapshot. Dirty-buffer text
overrides disk content in all relevant files. Unknown methods produce errors.
Workspace mutations and queries execute sequentially. A separate bounded
transport reader accepts `$/cancelRequest` while a query runs; cooperative
checks stop native work, and the completion lock prevents an accepted
cancellation from subsequently publishing an exact response. The adapter also
cancels its pending promise and discards late results. Bounded/coalesced queues
prevent repeated edits from retaining every text snapshot. Cancellation is
cooperative, not a hard wall-clock deadline for an individual filesystem call.

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

`navigation/possibleReferences` is a separate, explicit opt-in spelling search.
It does not run automatically after ordinary FAR or F12. Its response always has
`locations:[]`, `completeness:"unavailable"`, `source:"occurrence-search"`,
`candidateKind:"same-name-occurrences"`, and `candidateCompleteness:"incomplete"`.
`candidates` contains current physical-source identifier token ranges with the
selected spelling; unrelated bindings, declarations and inactive conditional
branches may be included. `includeDeclarations:true` states that declaration
tokens are included; the command does not implement semantic declaration
exclusion. No list, including an empty list or singleton, is a complete reference
set. Normal `navigation/references` behavior and proof checks remain unchanged.

The command searches the origin first, current matching eligible overlays, then
available name-posting files from the workspace model. Later compilation files
and reverse-project consumers are eligible. Linked physical files are deduplicated;
unknown external files are outside the available domain. It neither completes
name coverage nor launches a compiler/helper. An unloaded raw workspace uses the ordinary bounded initial discovery policy
(up to 512 entries), preserving small-workspace proof behavior. If preparation
is pending or hits that cap, it retains only already parsed origin occurrences,
marks preparation pending/truncation, and lets background discovery continue;
it does not reset the exhausted foreground budget. Raw physical syntax is parsed without choosing compiler defines or
entering facts into exact-proof caches. Unknown literal boundaries and parser
recovery cause source omission, not speculative identifier extraction.

`occurrenceSearch` records `scannedFiles`, `skippedFiles`, fixed-category
`skipReasons`, `scannedBytes`, `rowsExamined`, `eligiblePathsSeen`, limits, and
background state. Limits are 32 selected files including the origin, 4,096
posting rows plus overflow lookahead, 200 locations, 1 MiB per source and 4 MiB
of scanned source text; the existing foreground read budget may stop sooner.
Eligible overlays are bounded by the existing overlay memory policy and checked
for cancellation during selection. `candidateTruncated` and its reason disclose
work/page bounds. Skipped sources and unfinished discovery/indexing remain
explicit even when no numeric cap was reached. `modelAvailable` describes a
loaded model; `discoveryComplete`/`indexComplete` describe background phases and
do not certify complete workspace reference coverage.

`occurrenceIdentity.origin` and `occurrenceIdentity.targets` contain URI, BLAKE3
source hash and overlay version (null for disk) from the bytes actually searched.
They are freshness witnesses, not binding proofs. `snapshotId` identifies the
workspace snapshot. A versioned request must match the acknowledged overlay.
`expectedSourceText`, when supplied, must equal the current origin bytes exactly;
otherwise the request fails. For a clean editor buffer the adapter removes its
clean overlay, omits the native version, and supplies this text so stale clean UI
cannot mask an external disk edit. Dirty overlays remain authoritative.
Selecting a possible occurrence reissues this endpoint: the origin and selected
target identities must both remain unchanged and the full URI/range must still
exist. A deletion, rebind, rename, version or hash change rejects the old row,
even if a new same-spelling match occupies that location.

An unresolved indexed definition may additionally carry `candidates`, an array
of locations, `candidateKind: "same-name-declarations"` (or
`"same-source-declarations"` while discovery is pending), and
`candidateCompleteness: "incomplete"`. These are current-source spelling
suggestions, not established bindings, even when there is only one. `locations`
remains empty and the answer is not `exact`. The adapter displays them separately
as possible definitions without selecting a target. Existing raw name postings
are streamed in SQLite index order, with at most 4,096 ranked rows plus one overflow lookahead per pass.
A cold empty-result lookup may seed eight files and repeat once; warm lookup
uses one pass. Domain
filtering and source-project ranking precede retention of at most 32 source paths
and 200 locations. The independent scan bound prevents an unbounded foreground
query. `candidateTruncated:true` and `candidateTruncationReason` disclose visited-row,
retained-path or location limits; the adapter shows the limit reason. Even without
truncation, suggestions remain incomplete and may include unrelated declarations.
Candidate lookup does not demand whole-repository name coverage. A bounded cold
seed may inspect up to eight nearby compile inputs when no indexed path exists.

The full collected `candidates` array remains available in its original order.
Optional `likelyCandidates` is a subset of at most five of those locations,
selected from the strongest syntactic-evidence bucket. `candidateRanking:
"syntactic-evidence-v1"` identifies the presentation rule; `candidateEvidence`
contains `{location, evidence: string[]}` entries for the displayed subset.
Evidence includes written type or qualifier spelling, record-label shape,
enclosing lexical scope, and source/project proximity. `alias-rhs-declaration-shape`
also ranks nominal declarations for a syntactic alias RHS; that grammar shape can
instead denote a singleton union case, so it is not a proven type role. It is heuristic: aliases,
shadowing and unknown type identity can make a lower-tier target correct.
Neither tier changes binding completeness, reference completeness or exactness.
`likelyCandidateLimit` is five and `likelyCandidateTotal` counts all tied
strongest-bucket targets **within the retained bounded candidate union**, not
within the uncapped workspace. When that count exceeds five, the remaining ties
are still in `candidates`. An empty likely subset is permitted when no positive
evidence exists. Candidate truncation remains independently visible; revealing
all collected candidates cannot recover targets omitted by a disclosed work cap.

Current-source alias anchors and declarations before the first lexically real
preprocessor directive may be offered without guessing compiler defines. The
prefix stops at any directive, including `#nowarn` or `#load`, and rejects
unsupported lexical constructs. It is candidate-only evidence, never an exact
binding or namespace certificate. If that prefix does not contain the definition
query cursor and the first lexically real directive is `#if`, the engine may
recover only the physical query identifier and qualifier from current source
(up to 1 MiB). Full lexical-boundary validation and the existing bounded syntax
parser must succeed without recovered syntax. This does not determine whether
the branch is active. Declarations, type facts and scope facts from that physical
parse never become target/proof inputs; target declarations still follow the
conservative prefix/context rules. The result may identify
`candidateOrigin:"physical-conditional-source"`; shortlist evidence labels this
as `physical-conditional-origin-spelling`. All alternatives and existing
file/row/location caps remain unchanged. The same query-spelling recovery can
keep a pending definition request retryable while background work progresses.
Normal exact navigation, FAR and Possible References do not use this fallback.
A preceding `#nowarn`, `#r`, `#load` or other directive, uncertain literal
boundaries, recovered syntax or the source cap can still leave the query
unavailable. No compiler defines, unique binding or complete references are
inferred from the spelling. Dirty overlays remain authoritative. After model publication, an origin
with ambiguous linked ownership requires a selected project context. Early
same-source suggestions make no ownership or project-membership claim and are
withheld when an explicit project selection cannot yet be validated.
Only for a single certified, evaluated project with no project references,
unique compile inputs and exclusively `.fs` implementation files, suggestions
exclude source files later than the queried file in compiler order. Unknown
ordering, signatures (including mixed-case suffixes), scripts, duplicate inputs
or additional projects retain the broader candidate set. This filter never
changes exact results or promotes a remaining singleton.
The list is not cached as an exact answer. Cancelled and obsolete-version
requests do not publish candidates. When bounded project discovery is already pending, current-source suggestions
may be returned with explicit incomplete status; they do not certify the partial
model. The first request that exhausts the foreground discovery budget may still
be unavailable: the engine does not reset that exhausted budget to parse more
source in the same request. A subsequent request can use the source-local path.

An empty indexed definition may include
`indexing: {phase: "scanning" | "indexing", generation: number, retryAfterMs: 100}`
when active bounded background work can still supply candidate inputs. This is
an advisory continuation hint, not a completion or exactness certificate.
It is omitted for exact answers, nonempty candidate results, references, known
nonidentifier positions, invalid project selections, stopped/limited work and
ready workspaces. The first discovery-limited request may provisionally carry
this hint before source-token classification; the next request classifies it.
The generation identifies the native preparation lifecycle and does not replace
source-version, workspace or cancellation checks. The adapter may continue a
single user invocation within its bounded deadline; it must stop on cancellation
or superseding document/caret/workspace state and must not wait after usable
exact results or candidates are available.
Selecting a displayed candidate repeats the native definition request against
current overlays and disk inputs. The editor opens it only if the same URI and
full range remains among the fresh candidates or an exact definition result,
and the origin version, workspace epoch, latest definition request and displayed
list generation remain current. A removed or shifted target is rejected and
the obsolete list cleared. This check adds work only after deliberate selection;
time until choices are visible and selection-to-jump latency are separate metrics.

For indexed definition and reference requests, an explicit matching overlay
version enables a document-local proof before project discovery. Eligible
callback lambda and nested function parameters return `source: "local-lexical"`,
`proofScope: "document"`, the acknowledged `sourceHash` (BLAKE3), and
`documentProofId`. The `proof` object records the compiled engine and parser
identities, algorithm version, UTF-16 declaration range and half-open body
byte offsets. Document-scoped results omit `projectContextId` and carry the
workspace snapshot current at dispatch. A stale version is a request error.

`navigation/documentDefinition` is a separate indexed-only exact-or-miss ABI.
It requires `path`, `line`, `character` (or `position`), and a matching acknowledged
`version` or `documentVersion`; every supplied version alias must match and may
not be null. It invokes only the document-local Definition proof above. A miss
returns `completeness: "unavailable"`, `documentProofMiss: true`, no locations,
and no candidates or indexing continuation. It never falls through to contextual
navigation, model preparation, or a general answer cache. Shared dispatch watcher
polling still applies; this is not a zero-I/O guarantee.

The adapter uses this ABI only after an indexed Definition's membership lookup
would reject an unknown dirty external buffer. It acknowledges the current origin
through the existing overlay queue, including close/reopen barriers, then accepts
only an exact same-document result with document provenance and matching version.
Existing cancellation, request, editor/caret, document-lifetime and workspace epoch
checks still gate publication. A miss preserves the original membership refusal;
ordinary navigation, project selection, References and hover retain their existing
synchronization. Normal F12 incurs no extra RPC or parse. The shared queue can
still wait for other queued overlays, so this availability rescue makes no latency
claim and does not remove the membership lookup.

The process-local cache retains positive serialized answers for up to 32
distinct requests within 128 KiB of accounted storage. An acknowledged edit or
close invalidates its document's entries. A mode change or cancelled request
clears the cache. Workspace stats expose `localProofAttempts`, `localProofHits`,
`localProofCacheHits`, `localProofCacheEntries`, `localProofCacheBytes`,
`localProofParsedBytes`, `localProofVisitedNodes`, `localProofParseMicros` and
`localProofMicros`. Parse and proof metrics include unsuccessful work. Cache
hits reuse the answer without parsing the document.

`localProofOutcomes` counts fixed diagnostic categories, and
`localProofLastAttempt` records the most recently recorded attempt number,
document version, source hash, position, operation, outcome, parse/total
microseconds, visited nodes and parsed bytes. This is bounded diagnostic state,
not a proof certificate. If an early error prevents recording, the last recorded
attempt may lag `localProofAttempts`; compare the numbers before attribution.
With `FSHARP_NAVIGATOR_TRACE=1`, a response whose dispatch attempted a local proof
also carries `localProofAttempt` at the envelope level, correlated with that
response's request ID. It is null if no diagnostic was recorded for that attempt;
absence means no new attempt, not a refusal. No source text or path is logged.
The ordinary non-tracing envelope remains unchanged.

Outcomes distinguish `parse-time-budget` (100 ms), `proof-time-budget` (150 ms),
`rss-budget`, `parse-rss-budget`, depth/node/reference/location budgets,
`input-guard`, `invalid-position`, `syntax-error`, `unsupported`, `parser-aborted`,
`cancelled`, `error`, `exact` and `cache-hit`. They report the first observed
refusal in existing guard order, not a unique causal explanation when several
limits overlap. Parse callback precedence is cancellation, elapsed time, then
memory; proof traversal precedence is depth, nodes, cancellation, then elapsed
time. A final typed cancellation error is reported as `cancelled`. `exact` and
`cache-hit` describe the local proof stage, which can still be cancelled before
transport publication. Cache-hit zero parse/proof counters do not imply zero
request latency. Elapsed-time guards include process descheduling; daemon
`nativeTiming.dispatchMs` also includes other work and cannot identify the
particular refusal on its own.

With the same trace flag, successful `document/open` and `document/change`
responses carry a request-ID-correlated `documentUpdate` envelope. It records
an opaque normalized-document-key hash, acknowledged `version`, current BLAKE3
`sourceHash`, source `bytes`, whether indexed contents `changed`, and the number
of positive local cache entries evicted for this document. It contains no path
or source text. Error/cancelled responses omit this update envelope; omission
must not be interpreted as zero work or proof that an update was not applied.

Update timing fields are wall-clock microseconds: `validationMicros` covers
path/version/overlay-budget checks; `indexUpdateMicros` covers the existing
hash/index update; `overlayStoreMicros` covers local-cache invalidation and
storing the acknowledged overlay (including the opt-in document-key hash);
`snapshotMicros` covers changed-content snapshot invalidation; `totalMicros`
covers the update handler through diagnostic capture. `hashMicros` and
`nameScanMicros` are **nested within** `indexUpdateMicros`; do not add them to
that enclosing duration. `namesCount` counts the unique raw posting names
computed for changed contents. Additional nonoverlapping subspans nested within
`indexUpdateMicros` are `lookupMicros` (existing stored-hash lookup),
`transactionBeginMicros` (transaction creation), `postingsDeleteMicros`
(file-ID lookup/insertion plus deletion of prior context summaries, postings,
declarations and modules), `namesInsertMicros` (posting insertion loop, including
its existing cancellation checks, excluding name scanning), `stateWriteMicros`
(file-row update, answer invalidation and generation write), and `commitMicros`
(transaction commit). On the unchanged-hash path only hash, lookup and the
existing file-row update in `stateWriteMicros` execute; other subspans are zero.
Zero may also mean an executed phase rounded below one microsecond. Statement
preparation, trace capture and other bookkeeping remain in the unattributed
index remainder. These are wall durations around operations, not pure SQLite
CPU time; errors/rollbacks do not emit a successful update envelope.
The ordinary overlay update is lazy (`eager=false`) and does not construct a
syntax tree. Parsing for a subsequent local query is recorded separately by
`localProofAttempt`. These spans exclude queueing, wire serialization and
renderer work; elapsed time includes descheduling. Disabled tracing adds no
phase clock reads or diagnostic source/path hashing. Timing does not change
cache keys, invalidation, cancellation, or source-version checks.

### Opt-in demand-switch observation

With `FSHARP_NAVIGATOR_TRACE=1`, `workspace/open` may additionally specify
`diagnosticModelOrdinals`: at most four integer model-file indices below
100,000. A definition request selects one with `diagnosticWatchSlot`. These
numeric selectors affect observations only. They never change project
selection, ordering, budgets or navigation results. Invalid selectors disable
the observation. No paths or source text are accepted as selectors or emitted
in the new `demandSwitch` response envelope.

The editor's test-only environment entry is
`FSHARP_NAVIGATOR_TRACE_MODEL_ORDINALS` (a JSON array, at most 64 characters),
with `FSHARP_NAVIGATOR_TRACE_WATCH_SLOT` (default `0`). It is ignored unless
tracing is enabled. The adapter rejects duplicate ordinals and invalid slots.
These are not workspace settings or a supported user navigation mode.

The envelope records before/after background generation, order serial, original
order-origin slot, cursor and watched order ordinal/state. Mapping is collected
inside existing order construction and invalidated with that order/model.
`pending` means the background cursor has not consumed that slot; it does not
mean postings are absent. `postingStatus` remains `unknown`: no diagnostic
database/source reads are added. Existing candidate lookup and bounded seeding
report whether the watched file was selected initially, refresh attempt and
completion counts, watched refresh completion, and seed duration. Refresh
completion does not prove that contents changed or that seeding was necessary.

At most four `candidateIndices` refer to **the final `result.candidates` array**,
with an explicit truncation flag. `identityVerified` is always false in the
native observation: an independent harness must match an index to its literal
expected URI/range, check stable generation/order and reject missing/truncated
identity before drawing conclusions. Exact results do not acquire a candidate
identity witness through this facility. Error/cancelled requests omit the
successful observation, and each dispatch clears the previous request's state.

One optional latest-completed autonomous-slice record reports duration and
whether request admission fell within its interval. This is temporal
containment, not proof that the slice caused all queue time or delayed the
watched target. Missing records are unknown. The adapter adds a client-only
provider ID to its bounded transport trace, including continuations and
separately marked selection revalidation; that ID is not sent to the daemon.
It copies only whitelisted numeric, Boolean and fixed-enum observation fields.

Default runs create no observation and add no diagnostic I/O or phase timers.
Trace-on overhead is unquantified; instrumented durations are diagnostic, not
user-facing benchmark results. See the [demand-switch observation](demand-switch-observation.md).

A definition proof for a single-use ASCII parameter can also establish the
complete declaration-inclusive reference answer at that cursor. When the cursor
is on the use, the URI is at most 512 bytes and the cache has spare capacity,
the engine stores the two proven ranges in a compact positive entry. Its key
identifies the reference operation and the declaration flag. On a matching
reference request the engine formats the locations, labels the result with its
own proof ID and attaches the current workspace snapshot. Multi-use parameters
and declaration-exclusive requests follow the regular proof path.

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
A tree request reads solution layout from selected `.sln` or `.slnx` inputs.
Directory discovery retains solution paths, and prepared graphs without a
selected solution can use root-level solution files. Solution folders contain
their documents first, followed by nested folders and projects in declaration
order. Projects outside those solutions remain visible as roots. An unreadable
layout or a solution project missing from the graph yields a diagnostic and
approximate tree completeness.

The order of a project's `files` array determines its `compileIndex` sequence.
Evaluated `virtualPath` and literal MSBuild `Link` metadata supply display
folders. Only consecutive entries share a folder at a given level, so the
virtual paths `A/One.fs`, `B/Two.fs` and `A/Three.fs` produce three directory
runs. Files retain physical URIs for navigation, and each folder run has a
distinct tree item ID. Authoritative MSBuild compile inputs require an evaluated
manifest.

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
