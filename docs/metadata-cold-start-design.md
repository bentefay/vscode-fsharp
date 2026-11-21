# Metadata validity and first-use cost: design review

Status: reviewed source audit and experiment design, 2026-11-21. The subsequently
authorized new producer-contract implementation is tracked separately in
`metadata-producer-contract.md`; its focused actual role, XML-selection and compatibility tests pass.
No prebuilt-index implementation is claimed, and no proposed speedup below is a
measurement. This concerns exported **binary declaration
inventories/catalogs**, not evaluated project models, optional source-semantic
snapshots, or live decompiler processes.

The recommended next experiment separates producer provenance from live fact
dependencies, then builds the same bounded Rust index during explicit
preparation. A short, cancellable validation/import job after activation is a
complement for users without a prepared native index. Activation and local
source navigation must remain usable before that work finishes. None of these
options removes current-reference binding or source-shadow proofs.

## What the current work costs

The frozen pre-split `MetadataEnvironment.fs` appended helper and FCS DLLs
to `inputHashes`; the new enforced schema2 contract separates producer-only
provenance. The baseline below retains the original dependency set.
`builtins.rs::Environment::load` hashes the entire JSON artifact, streams three
name maps into SQLite on a cache miss, hashes every listed input, then commits.
A reused SQLite inventory skips row import but still hashes JSON and every
dependency on first demand in each session. Warm checks compare captured file
identities/change times; non-Unix targets conservatively hash again. Rendered
catalogs in `metadata.rs` use a similar first-use validation rule.

The existing complete-inventory positive diagnostic returned exact framework
hover at 939.61 ms cold and 2.07/1.15 ms warm, including its harness stats call.
It measured 23.42 MiB native steady RSS. This is one controlled diagnostic with
an uncontrolled OS cache, not a distribution or an accepted speed ratio. Grace's
28.88 MiB trigger remained unavailable and is not a successful navigation timing.
See `inventory-storage.md` and its raw reports. A newer instrumented diagnostic
reported 457.738 ms inside inventory loading (177.525 ms dependency
validation, 176.458 ms import, 76.403 ms commit):
`test-results/resolution-states/native-9201ad3f-current-ownership.json`,
the `positive_complete` row's `stats.builtinEnvironmentLoad`. That run had
concurrent work and is a diagnostic phase breakdown, not quiet timing; it must
not be substituted for the earlier binary's measurements or the upcoming quiet
pre/post comparison.

The subsequently frozen **pre-split quiet baseline** is now available in
`test-results/inventory-memory/cold-phases-8daa-quiet.json`: exact first hover
896.43 ms (model 25.66 ms, rendered catalog 538.91 ms, inventory 323.52 ms).
Inventory import took 185.20 ms, validation 58.05 ms and commit 67.25 ms.
Reopening the prepared SQLite cache took 209.02 ms for the first exact hover
(catalog 110.04 ms, inventory 48.41 ms); warm calls were 0.54–1.19 ms. This is the
controlled comparison baseline; it is still a small fixture and not a monolith
performance claim. Its explicit helper479 preparation took 15.15 seconds for
the model, two selected catalog shards and the full import inventory, with a
425 MiB helper-process-tree peak. See
`test-results/inventory-memory/explicit-preparation-479-quiet/setup.json`.
That transient setup memory must be reported alongside the roughly 24 MiB native
steady process, not hidden as a free prerequisite.

Read-only inventory of the actual certificate files found:

| Certificate | Selected assembly bytes | Producer-only FCS/helper bytes | Producer share of input bytes |
| --- | ---: | ---: | ---: |
| Framework positive | 10,307,720 | 40,319,784 | 79.6% |
| Grace | 29,924,240 | 40,319,784 | 57.4% |

The JSON files themselves are 9,530,270 and 15,607,364 bytes. These counts are
reproducible in `test-results/inventory-memory/provenance-byte-audit.json`; the
two producer paths were checked not to occur in the selected reference set.
Byte fractions are **not** time-saving estimates: page cache, storage, hashing,
system calls, project validation and SQLite work remain distinct costs.

## A sound provenance/dependency split

An exporter derives facts from assemblies using a particular implementation.
Removing that implementation after export does not alter the serialized facts.
Requiring the old helper installation to remain on disk is therefore stronger
than the fact's freshness requirement. Conversely, accepting a known-buggy
producer just because its files still have their original hashes is insufficient.

The proposed artifact has three distinct components:

1. **Producer evidence:** helper/FCS implementation hashes, compiler/importer
   profile, semantic contract identifiers, formatting contract, extraction
   options and source/build provenance. These stay inside the artifact and its
   content digest; the original executable files need not remain present.
2. **Live fact inputs:** selected PE files, F# signature resources embedded in
   them, actual demanded forwarding/type/alias/measure dependencies, and XML
   documentation for rendered comments. Hash these on first use as today.
   Inventories must retain the complete effective selected-reference domain,
   including assembly AutoOpen and competing declaration data; a detailed
   catalog subset is not an absence certificate.
3. **Current applicability:** current evaluated reference bindings and workspace
   ownership/exclusion roles must match the certificate's covered set, with current source
   lookup/order/shadow proofs. New selected packages cannot be ignored merely
   because every old input file still hashes correctly.

Files have **roles**, not a filename-based exemption. If the project actually
references FSharp.Compiler.Service.dll or the helper DLL as a library, that
path remains a live dependency. FSharp.Core is normally a live selected binary
even when the producer also uses a runtime copy. Deduplicate identical physical
inputs without dropping any live role. Moving dependencies still requires a
new binding certificate under the present absolute-path contract; automatic
relocation is outside this proposal.

Consumer acceptance must enforce recognized semantic contracts, not trust an
informational version string. Initially use an explicit accepted producer/import
profile and fact-domain versions, plus revoked implementation IDs where needed.
For example, declarations, full name inventories, XML rendering and callable
schemes can have separate interpretation versions. A corrected alias, forwarding,
ownership or documentation bug must revoke the affected old facts even if the
old tool's bytes remain available. Unknown or revoked formats fail closed with
a prepare-again reason. A compiler profile change must be checked for compatible
signature interpretation; it cannot silently assert equivalence.

Independent review identified a concrete migration prerequisite: the current
catalog importer accepts schema1/exact data without enforcing `factsVersion` or
producer compatibility, and can accept a catalog without input hashes. The
inventory consumer checks factsVersion2 but has no accepted-producer/revocation
policy; version2 spans both corrected and previously unsound producer builds.
Thus **do not simply remove executable paths from today's inputHashes**. Add a
new explicitly required certificate contract and consumer policy first. Legacy
artifacts either retain their current strict validation under an explicitly
limited legacy path, or require re-preparation; they cannot acquire the new
absence/binding guarantees by dropping fields. Completeness markers cover the
precise type/scope/term domains; ordinary static-member/open-type lookup is not
implicitly added to the current term map's guarantee.

The native database builder has its own contract and provenance, independent
of the helper's fact contract. A corrected importer must invalidate an old
database even when its source JSON is unchanged. Initially retaining the current
full native engine fingerprint is conservative; narrowing that to a reviewed
component identity is a later compatibility experiment, not a prerequisite for
the first prebuilt-index trial.

This is not artifact authentication against a malicious user who can rewrite
both an artifact and its manifest. Current local artifacts already rely on an
authorized producer workflow. Digests bind content and detect accidental or
unexpected changes; they do not prove an algorithm correct. Distribution to
untrusted machines would need a separately designed signed provenance policy.

The split does not weaken extraction-time checks: snapshot input identities
before extraction, validate bytes/bindings after it, reject unresolved dependencies
and publish only complete results. Keep the current PE plus F# signature graph
closure for rendered/type facts. The name inventory's narrower declaration-only
closure must not be reused as a complete callable/signature certificate.

## Three scheduling/storage options

| Option | Explicit preparation | First exact external lookup | Restart | Main risk |
| --- | --- | --- | --- | --- |
| Current JSON → workspace SQLite | FCS export and JSON write | JSON hash + import + live-input hashes + commit + binding work | JSON/input hashes; reuse indexed rows | Large latency spike; repeated workspace copies |
| Rust prebuilt content-addressed database | Same export, then bounded Rust import/validation/finalization/digest | Database verification + live-input hashes + indexed lookup + binding work | Same validation, no SQL import/commit | Moving cost can hide setup; bigger database may cost more to hash |
| Cooperative idle import/validation | Optional; works with existing prepared JSON | Immediate only if job finished; otherwise promote work or report pending honestly | Revalidate lazily/idle; reuse completed rows | Background CPU/IO competes with foreground and can waste work |

A prebuilt database addresses the measured import/commit phase. The provenance
split addresses unnecessary input validation. Neither removes source/model
binding work, and neither justifies calling first use instantaneous. In particular,
a database can be larger than JSON: measure its streaming integrity check before
assuming it improves the remaining cold path.

A separate small experiment is a bounded **same-session verified-digest cache**
shared by inventory and catalog validation. Both currently validate overlapping
binary inputs. A cached result can be reused only for the same path/file identity,
unchanged change-time/mtime/length and validation generation; compare the actual
digest against each certificate's expected value. Detect conflicting expected
hashes and mutation between the two validations. Do not carry these observations
across process restarts or weaken platforms lacking equivalent change identity.
This may eliminate duplicate work while retaining all live inputs, but its byte
overlap and time savings must be measured separately from the provenance split.

### Proposed prebuilt-index publication protocol

Use a separate Rust preparation command/library with the exact same parsing,
row limits, complete-map checks and schema interpretation as the daemon. Produce
one external inventory database per complete reference-universe certificate and
a separate lazy documentation database; do not duplicate source indexes or
bundle checked workspace state. The initial artifact key covers canonical
reference bindings, live input digests, producer contract IDs, schema/index
format version, explicit workspace assembly ownership/exclusion roles and
canonical data digests. Matching external references alone is insufficient if
workspace-owned assemblies differ. Because absolute paths currently
participate in identities, the first version is machine-local and reusable
across matching workspaces, not blindly portable across installations.

Build into a fresh private staging path on the destination filesystem. Retain
the current bounded streaming limits and explicitly budget SQLite page cache,
temporary files, row buffers and cancellation. Use an ordinary rollback-journal
database for the finalized artifact, or fully checkpoint and leave WAL mode
before sealing. Close the connection, verify schema/constraints and canonical
map counts/digests against the exporter, hash the closed database, then durably
flush the files and publish a descriptor atomically. A descriptor is published
last and names one complete generation; multiple independent renames are not
a transaction. Failed/cancelled/disk-full preparation cannot make staging rows
visible as an exact inventory. Distinguish the logical derivation key from the
actual database-byte digest: equivalent rows can have different SQLite page
layouts. Concurrent builders of the same derivation verify the winner's accepted
contracts and canonical semantic content, then retain the descriptor's actual
file digest and discard their private staging generation. Do not demand that
different valid physical layouts have identical database hashes.

At runtime open lazily in **read-only mode**, with loadable extensions disabled,
trusted-schema behavior disabled, schema allowlisting and existing bounded query
results. Use a bounded page cache, no default unbounded mmap, and keep writable
source/overlay state separate. Verify the descriptor, supported contracts and
closed database content digest before accepting data, then validate current live
inputs and bindings. Keep file identity before/after validation and query epochs;
replacement or mutation invalidates the connection and dependent answers.
Unix ctime/inode plus watcher generations remain an optimization within a
session, not a replacement for first-session content verification. Other
platforms need their own proven equivalent or conservative hashing.

Do not set SQLite `immutable=1` just because the filename is content-addressed
or chmod removes write bits. SQLite deliberately skips locking/change detection
under that flag; mutation can return incorrect results. `mode=ro` is the safer
initial choice. File hashes detect a semantically changed but structurally valid
database, whereas `integrity_check`/`quick_check` do not authenticate row contents.
An integrity error, digest mismatch, unsupported schema or stale dependency
rejects the cached artifact and permits bounded rebuild from the explicit JSON
source. It must never become an exact empty reference result.

### Cooperative idle warmup

Activation should only register/queue optional work and return. A worker builds
a private generation, not a long transaction holding the live source-index
connection. Foreground navigation preempts it at existing cancellation/poll
boundaries. Local proofs never wait for external metadata. A first external
query can promote the same job rather than duplicate imports; until ready it
must return an explicit pending/unavailable status, or await it within the normal
query budget. Measure **time to first exact answer**, not merely how quickly an
incomplete reply was returned.

Only relevant binary/config/reference/ownership-input changes cancel or discard
an inventory publication. An ordinary source edit invalidates the affected query
proof epoch but does not restart preparation of unchanged assembly facts. Keep
those two generations distinct. All three inventory maps and the certificate become available as
one generation. Do not expose a partly loaded negative-name lookup. Initially
warm only configured binary artifacts, never invoke FCS or scan all workspace
source as a side effect. A background job taking seconds is acceptable only if
measured foreground latency, CPU, IO and peak combined memory remain acceptable;
idle scheduling is not evidence of zero cost.

## Falsification and measurement gates

Before changing production guards, implement the following paired controls:

| Probe | Required result |
| --- | --- |
| Delete/move/change only producer executable after export | Facts still work under an accepted contract; recorded producer provenance unchanged |
| Select that same executable as an actual project assembly | Its mutation still rejects the affected facts |
| Change selected PE, XML, forwarded type, erased measure or alias-chain dependency | Relevant facts reject, same session and restart, including preserved mtime/length |
| Add a new selected package exporting a competing type/value | Old complete inventory rejects despite unchanged old files |
| Revoke producer fact contract while bytes remain unchanged | Old facts reject; rebuilding with accepted producer restores results |
| Mutate one valid SQL row or remove one name entry | Database digest/canonical completeness check rejects; no false absence |
| Kill/cancel preparation, fill disk, truncate DB, leave WAL, interrupt descriptor publication | Old complete generation remains usable or new generation is unavailable; never partial exact data |
| Race two builders or replace DB/input during first validation | One coherent generation or explicit retry, no mixed certificates |
| Start local query during idle import, then edit dependency/cancel/retry | Local foreground work remains responsive; stale generation discarded; retry completes correctly |
| Old/new artifacts coexist, consumer/schema upgrade, cache eviction | Explicit compatibility/migration; bounded disk cleanup cannot delete an in-use generation |

Measure the current path, provenance-only split, prebuilt-index path and idle
warmup separately on the same frozen native/helper/source facts. Include the
small exact fixture and full Grace reference inventory, with actual successful
queries distinguished from coverage misses. Record explicit preparation wall/CPU,
full process-tree peak RSS/PSS, output bytes, first workspace-open work, time to
first exact query, warm p50/p95, reopened-cache queries, foreground latency while
warming, and full dependency/JSON/DB bytes hashed. Report application-cold versus
OS-cache state and filesystem separately; use workspace disk, not an accidental
tmpfs advantage. Preserve identical row counts and canonical type/scope/term
digests, result identities and stale-input adversaries across variants.

A useful first go/no-go experiment is the provenance-only change with byte roles
and revocation tests: it is smaller and directly attacks a measured input set.
Then compare prebuilt inventory and catalog databases against identical already
prepared JSON. Only after that test cooperative idle validation, so scheduling
does not hide missing preparation or a slower total algorithm. None of these
experiments by itself establishes the user's monolith memory/performance goal.

## Evidence and primary references

- Local implementation inspected: `semantic/MetadataEnvironment.fs`,
  `semantic/Semantic.fs`, `crates/navigator-core/src/{builtins,inventory,metadata,index}.rs`,
  and `cli/commands/prepare.mjs`.
- Actual byte audit: `test-results/inventory-memory/provenance-byte-audit.json`.
- Existing measured inventory diagnostics: `docs/inventory-storage.md` and
  `test-results/inventory-memory/full-ff85-*-quiet.json`.
- [SQLite URI parameters](https://www.sqlite.org/uri.html): read-only versus
  immutable semantics and mutation risk.
- [SQLite WAL](https://www.sqlite.org/wal.html): WAL is persistent database state;
  finalize/checkpoint it correctly before distributing a standalone file.
- [SQLite integrity pragmas](https://www.sqlite.org/pragma.html#pragma_integrity_check):
  structural consistency checks and the narrower coverage of quick_check.
- [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html): database
  transaction guarantees; filesystem publication of a separate descriptor still
  needs its own durable ordering.

Primary documentation was read on 2026-11-21. No SQLite/runtime upgrade or cache
trust-policy change is part of this design note.

Independent scorer review approved this as an experiment design, with the
ownership-key, logical-key/physical-digest, and source-edit separation corrections
incorporated. That review is not implementation or performance acceptance.
