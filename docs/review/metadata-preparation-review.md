# Independent review: prepared metadata and producer provenance

Status: design review only, 2026-11-21. No implementation, guard removal or speedup is approved by this note. The reviewed proposal is `../metadata-cold-start-design.md`. The current native correctness evidence is separately recorded in `../recovery-review-ledger.md` for candidate9201; it does not establish full feature parity.

The proposed direction is sound enough for a staged experiment: keep exporter implementation identity as enforced producer provenance, keep all actual semantic dependencies live, and prepare the bounded read-only lookup database explicitly. This can remove redundant executable hashing and first-query SQL import/commit. It does not remove integrity verification, current reference applicability, complete negative-name proofs or their cost.

## Findings from the current source

`MetadataEnvironment.fs` adds the FCS and helper assemblies both before extraction and to final `inputHashes`. `Semantic.fs` likewise adds them to rendered catalog dependencies. `builtins.rs` hashes all listed inputs at first use in each session; its warm Unix check uses change time and file identity. `metadata.rs` also verifies persisted input bytes on first load. These checks currently combine two different roles: the files whose meanings are described, and the program that previously described them.

An already sealed declaration fact does not change because its exporter executable is subsequently moved, rebuilt or deleted. Conversely, unchanged exporter bytes do not establish that its algorithm was correct. Producer compatibility therefore needs a consumer-enforced contract independent of current executable-file existence.

The migration is currently blocked by insufficient explicit compatibility checks: `metadata.rs::CatalogSeed` accepts schema1/exact/content while ignoring factsVersion and producer identity, and permits absent dependency hashes; `builtins.rs` checks factsVersion2 but no accepted producer profile. Version2 spans earlier producer defects corrected during this project. Removing executable inputs from that existing format would remove an incidental invalidation mechanism without replacing it. Require a new contract before changing the input roles; reject or retain the old strict legacy semantics for older artifacts.

## Required distinctions

| Item | Role in the proposed contract |
| --- | --- |
| Helper/FCS implementation bytes used only to export | Provenance and consumer compatibility/revocation identity; no continued file-existence requirement after sealed export |
| The same file also selected as a referenced library or reached by a signature dependency | Live dependency; never exempt by basename or by its producer role |
| Selected assemblies, demanded facade targets, F# aliases/measures/signatures, rendered XML | Preserve complete relevant live dependencies, including existing missing-file rules |
| Full type/scope/term declaration universe and workspace ownership exclusions | Applicability and absence-proof certificate; a catalog subset cannot replace this domain |
| Source bindings, opens, local constraints and current document version | Query proof dependencies; separate from whether unchanged external facts need rebuilding |
| Rust JSON-to-index implementation | Independently enforced builder compatibility; unchanged exporter JSON does not validate an index built by a defective importer |
| Actual prepared DB bytes | Content integrity; distinct from a logical derivation key and semantic row digest |

The accepted contract must state exact domains. The present term inventory covers module values and selected case/literal/pattern forms; it is not a blanket certificate for ordinary static members imported with `open type`. Unknown lookup domains must remain unknown after moving the data to SQLite.

Sharing requires the same applicable ownership/exclusion map, not merely the same framework binaries. An earlier test preparation accidentally reused another workspace's ownership certificate and masked a genuine wrong-exact source shadow; this is a demonstrated relevance boundary. Dependency roles must be unioned before deduplication.

## Publication and runtime invariants

1. Build privately into a fresh generation on the destination filesystem. Retain all streaming, row, header, page-cache and cancellation bounds. Finalize a standalone database with no required WAL/journal sidecar, close it, validate complete semantic maps and schema, and compute the actual byte digest.
2. Distinguish the logical derivation identity from the final DB byte digest. Equivalent rows do not require identical SQLite byte layout. The published descriptor binds the accepted builder contract, semantic content, input/ownership certificate and actual closed-file digest.
3. Publish the complete generation before atomically switching one descriptor. Never independently update live inventory, catalog and header paths. A concurrent winner must be verified; a crash or disk-full event may leave unused staging files but cannot expose partial exact maps. Apply durable ordering to files and descriptor as part of the platform-specific publication implementation.
4. Initially use SQLite read-only mode, not `immutable=1`. A content-addressed name or read-only permission bits are not the stronger immutability promise that flag conveys. Keep the prepared connection separate from writable source/overlay state.
5. Verify integrity and supported contracts before accepting lookups; validate live inputs and applicability before using their facts. Keep file identities around checks and query generations. Digest mismatch, missing table/domain, truncation or a read error means unavailable/rebuild, never exact absence.
6. Pin one coherent generation for an operation. A new published generation must not mix with an old query's certificate. Retire in-use generations safely. Metadata-dependent answer caches remain dependent even when their final target is workspace-owned.
7. A source edit invalidates relevant query proofs. It need not cancel an in-progress external inventory build when reference, ownership and artifact inputs are unchanged. Cancelling all metadata work for every keystroke would reintroduce the latency problem this design addresses.

SQLite's official URI documentation says immutable mode skips locking and change detection and warns that mutation may yield incorrect results. Its WAL documentation makes clear that committed content can remain in the WAL. These support the conservative standalone-file/read-only starting point; they do not implement the surrounding publication protocol for us.

## Acceptance experiments

Retain positive and negative controls together. Delete or rebuild producer-only executables and require unchanged exact answers under an accepted contract; select one as a real library and require its mutation to invalidate. Revoke an old producer or builder contract and require rejection despite unchanged artifact bytes. Mutate PE, XML, forwarded, erased-measure and alias-chain inputs with preserved size/mtime, add a competing reference, and replay workspace-target mixed proof caches.

Compare every imported/prebuilt type, scope and term map by canonical content, including empty buckets and row counts. Delete a valid SQL row, alter a semantically significant payload, truncate a DB, leave a required WAL, interrupt publication, race builders and mutate during validation. None may produce a certified empty candidate domain. After each rejected stale artifact, a correctly rebuilt artifact must restore the expected positive result.

Measure provenance-only, prebuilt-index and idle variants separately with the same accepted facts and queries. Count explicit preparation wall/CPU, all child-process memory, database and temporary bytes, bytes hashed, first exact query, warm/restart queries and foreground latency under idle work. File-cache state and filesystem must be recorded. Moving work to prepare or idle is useful scheduling, not elimination of work. First-session full integrity and dependency hashing remains proportional to the data read; no universal millisecond bound or orders-of-magnitude improvement follows from this design alone.

## Primary material

- Local source audit: `semantic/{MetadataEnvironment,Semantic}.fs`, `crates/navigator-core/src/{builtins,inventory,metadata}.rs` and `cli/commands/prepare.mjs`.
- [SQLite URI parameters](https://www.sqlite.org/uri.html), read on 2026-11-21.
- [SQLite WAL](https://www.sqlite.org/wal.html), read on 2026-11-21.
- [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html), read on 2026-11-21. SQLite transactions and multi-file application publication are separate guarantees.

No numeric approval score is assigned. The prerequisite is preserved correctness and useful positive coverage on the fixed traces, followed by measured improvement; a fast unavailable response does not satisfy navigation.


## Implementation source follow-up (not runtime acceptance)

The pending `prepared.rs` implementation was inspected after0a77. It uses standalone rollback-journal files, bounded descriptors/databases, full DB hashing on first use, read-only SQL connections, restricted schema, producer-policy and builder fingerprints, and current dependency validation. A source JSON file is retained as derivation provenance rather than a live input after the index is sealed. Catalog rows remain positive facts; current external ownership and absence proofs still come from the applicable project/inventory context.

Separate catalog/environment descriptors are sufficient for the first fixed-session A/B if they are immutable, selected only after both preparations succeed, and every query retains required current model/reference/ownership proof dependencies. This does not claim atomic hot replacement of a running pair. A globally shareable positive-only catalog need not copy a workspace ownership map, but its `ownershipScope: lookup-time-project-model` marker cannot substitute for the enforced lookup checks. The complete inventory descriptor does retain the ownership exclusions.

Three concrete review requests have now been reflected in source or owner evidence: `prepare --native-index` reserves a new output directory before any model/artifact writes (actual sentinel-preservation test at `test-results/recovery/prepared-cli-generation.json`); load metrics separately expose descriptor, database, source JSON and actual hashed dependency bytes; and live dependencies are revalidated immediately before publication. Prior code that counted only the descriptor as input bytes would have hidden most first-read DB hashing. No prepared-index performance result existed at this review point.

The generic container tests and source audit do not establish full semantic parity. Required next actual tests remain same-facts JSON/prepared equality for all three name maps and rendered catalog rows, selected exact navigation/hover, missing XML selection, input mutation, ownership-context changes, cached workspace-target proof invalidation, schema compatibility, corruption and partial-publication rejection. Count preparation and runtime verification independently in the comparison.


## Frozen runtime follow-up

Independent execution on f27d with genuine f215 artifacts passed the owner's real native harness: same-facts JSON/prepared/reopened exact hover, complete type/scope/term maps, no copied fact rows in the writable workspace database, cached same-session row deletion rejection and fresh-process DB digest rejection. Evidence is `prepared-f27d/report.json`; its timings are diagnostic because other correctness work overlapped. This is runtime evidence beyond the earlier design-only review, but does not establish broader language or editor parity.

A separate coordinated same-facts7047 A/B measures first exact hover at87–125ms prepared versus740–991ms JSON, with reopened queries roughly78–88ms for either path. Native preparation adds790ms; actual helper export cost remains separate. Full byte hashing and dependency checks remain active. The filesystem cache was not flushed, and7047's unrelated signature active-pattern bug still excludes whole-engine acceptance. Keep the improvement scoped to this exact operation and preparation mode.

## Shared binary-import experiment source audit

Immutable helper `e450b15c7f767fd3863b8b9c023757bc8fd4782289377bdbfe2dfe49863328d2` adds an opt-in single-job shared import. `MetadataContext.fs` captures one fixed explicit/default reference universe before import and retains its checker/selected assemblies only for that job. `MetadataPreparation.fs` captures each selected XML prefix before import, emits per-assembly catalogs plus a complete environment into a private generation, then revalidates context inputs, producer inputs, XML selections and the union of all emitted live certificates before publication. The process exits after the request, even without `--once`.

Source review found no new semantic dependency being dropped by sharing. The existing per-artifact positive facts, complete negative-name/ownership domain and final live checks remain required. `require` currently permits a subset of the context references; keep that API confined to the orchestrated job using the same full reference set. General reuse across independent reference contexts would require a new applicability proof.

Two accounting boundaries matter. `sourceProjectChecks=0` means no workspace source project is checked: FCS still parses/checks the synthetic `()` script to import binaries. The 25ms memory/deadline guard samples the current helper process; it is neither a hard instantaneous bound nor a child-process aggregate. A late resource exit can leave a complete unused published generation, so the guarantee is no accepted response after rejection, not that every failure leaves zero files. The caller must select outputs only from a successful response.

The implementation owner's seven complete-output/concurrent-input-change/recovery/budget controls and the test owner's independent lifetime/equality checks are separate evidence. This paragraph is a source audit, not a rerun, a performance measurement or adoption approval. Shared-import benefit must be measured on identical complete outputs with all preparation costs retained.
