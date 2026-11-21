# External declaration inventory storage

The complete exported external declaration inventory is now streamed into the
workspace SQLite database on first use. Requested names are fetched by indexed
lookup; an absent name is returned as a verified negative only after a complete,
validated import committed. No declarations are omitted to reduce memory.

The daemon retains builtin alias/pattern facts, compact input stamps and
reference identity maps. The persisted certificate retains the complete small
header needed for dependency revalidation on another process start. Reuse checks
the JSON digest, native semantic-engine fingerprint and every dependency hash.
Current model/reference ownership checks remain in the workspace layer.

Import, certificate publication and name replacement share a transaction.
Cancellation or malformed data rolls it back; the previous complete inventory
remains available. Duplicate JSON name keys are rejected with disk-backed key
sentinels, including names whose arrays are empty. This avoids retaining a second
in-memory set of every imported name.

The expanded type/scope/term export has an explicit 64 MiB streamed input-file guard. A parser-consumed byte budget rejects oversized individual JSON values before allocating the whole input (1 MiB encoded row, 4 MiB encoded header field). Additional guards are a 1 MiB
certificate header, 128 KiB individual serialized declaration, 100,000 name
keys and 250,000 declaration rows. A requested name lookup has a 512-declaration
and 1 MiB serialized result budget; import retains names with larger collision
sets in full. Exceeding a guard reports unavailable; no partial
inventory is treated as complete. These are structural guards, not a hard RSS
limit; the existing SQLite page-cache budget is shared with the source index.

Five fresh focused tests pass: all 5,000 generated names and their identities
survive import; negative lookup is certified; malformed or cancelled replacement
rolls back and retries successfully; persisted rows are reused while dependency
contents are revalidated, including a restored-mtime change. A 600-identity
collision set remains fully stored; only its oversized lookup is unavailable,
while unrelated unique lookups continue to work.

`test-results/inventory-memory/measure.py` measures the native process with the
actual model, catalog and builtin environment configured. It records startup,
first-query and warm RSS, completeness and counters. On the new storage schema
it compares a canonical SHA256 of every stored name/identity against the original
exported JSON. This is separate from editor-host measurements and does not claim
that unavailable queries satisfy navigation parity.

The pre-change immutable candidate is `86548fa855ca16f10c26cd3db5df0b56ee426c89c106c9e63c42a9a094528ac7`.
Its expanded-fixture exact external hover loaded the 2.33 MB inventory and reached
34.83 MiB native RSS from 5.60 MiB at startup. The independent Grace full-inventory
trace loaded 6.97 MB and reached 53.36 MiB warm RSS. Its trigger hover was
unavailable, explicitly retained as a resource diagnostic. A fixed Grace query
selection that never loaded the inventory is insufficient for this comparison.
The matched quiet-window comparison against `edafa1f8dd98` completed. Expanded
steady native RSS fell from **34.09 to 26.17 MiB** (23.2%); Grace fell from
**52.96 to 27.00 MiB** (49.0%). The expanded inventory retained all 3,950 names /
4,297 entries; Grace retained all 10,528 names / 13,578 entries. Canonical full
inventory hashes match the original JSON exactly. Both measured answer sequences
match after excluding version/snapshot fields; expanded remains exact and Grace
remains unavailable. Neither launched a compiler/helper, and parsed-file counts
remain 22 and 39 respectively.

Expanded's single accepted warm sample was 0.94 ms, versus 1.75 ms before. Grace's
unavailable warm operations remained about 49–50 ms and are not successful
navigation. These are one accepted run per candidate/corpus, with fresh process
and index directories but an unspecified OS file-cache state. The candidates also
include unrelated correctness changes; this is a candidate resource comparison,
not statistical isolation of one code change. Two initial overlapping Grace
samples were discarded and both Grace candidates rerun sequentially before the
quiet window ended.

Full data and limits: `test-results/inventory-memory/comparison.json` and the
four `quiet-*.json` files. Measurements exclude the editor and helper preparation.


The next slice stores complete module/namespace and expression-binding inventories
in separate SQLite tables alongside the types. A missing scope or term certificate
returns unknown; it never becomes an empty complete name list. Module rows retain
logical and compiled owners, entity AutoOpen and RequireQualifiedAccess flags.
Term rows retain module values, active-pattern cases, union cases, literals, enum
cases and display/logical/compiled operator names. Assembly AutoOpen attributes
are read from managed binary metadata because imported FCS assembly signatures
do not expose a complete attribute inventory. The producer synthesizes namespace
prefixes from imported declarations, since FCS commonly flattens namespace nodes.

`semantic/tests/term_inventory.py` builds an actual F# library and consumer. Its
13 checks include a consumer that prints the library's value through an assembly
AutoOpen without any explicit `open`, renamed compiled module identities, nested
logical paths, qualification attributes and operators. This tests declaration
inventory, not full callable type inference. The resolver must still prove which
lexical/open scope is applicable; unrelated enum names are not namespace blockers.

All three maps and their certificate publish atomically. Structural input/name/row
bounds never silently truncate data. The larger producer's resource measurements
below are separate from the earlier type-only RSS comparison.

The complete three-map producer was measured separately on immutable native
`ff85a38647ca126934e0695df3abf4befcf127b84f5db3ca2c73014650125a6a`
and helper `4efbac974e9eebd019c8ad4e81e44a37691ba8cd844072e2c7e249352340bc20`.
All team workloads were stopped for two sequential native sessions. These are
single-run resource diagnostics with an uncontrolled OS file cache, not an A/B
performance comparison or an editor-host measurement.

| Configuration | JSON bytes | Type keys / rows | Scope keys / rows | Term keys / rows | Native startup / steady RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exact framework hover fixture | 9,530,270 (pretty JSON) | 3,950 / 4,295 | 200 / 529 | 5,672 / 7,993 | 5.86 / 23.42 MiB |
| Grace full reference set | 15,607,364 (compact JSON) | 10,528 / 13,578 | 484 / 1,160 | 9,084 / 13,704 | 5.84 / 28.88 MiB |

Both loaded all three inventories, launched zero helpers/compilers and read/parsed
zero source files at workspace startup. The framework fixture returned exact
`System.String.IsNullOrEmpty` hover on all three requests: cold 939.61 ms, warm
2.07 and 1.15 ms (including the harness stats call). Grace's trigger remained
**unavailable** on all three requests, so its 28.88 MiB is only evidence of the
resource cost of loading the configured data; its request times are not navigation
success. Neither includes VS Code or offline metadata preparation memory.

Every stored entry matched its exported array, including order and identity.
Canonical SHA256 digests were:

| Configuration | Types | Scopes | Terms |
| --- | --- | --- | --- |
| Framework fixture | `30586443c5d55686c597bde9f10e273dd5690dd245b6dd8e91b3c37e49be4a3e` | `0b147c7e8a665a8b1180c8117dda30b9c745d4cace44f0c8ed6fa7b6a6ce9622` | `561183fb1cb03e4590d39c1e2dd828b9562521118c554ce940db56e099eefb67` |
| Grace | `7dbf8b9f1f167b05be6a7f88325a4a6ce81ab32ae458ba48b0b55f958449d919` | `fd16cd9169dd23bde63663f4decc161fded0192424284e5a684423d4beafa9ec` | `936b996c58dfff10cb504c7decb5dcac532da0d926e2180def516ee15b2f62b1` |

Raw reports are `test-results/inventory-memory/full-ff85-positive-quiet.json` and
`full-ff85-grace-quiet.json`; configurations and export counts are beside them.
Eight storage tests cover all-domain rollback, cancellation in the term map and
successful retry, missing-map claims, bounded oversized JSON allocation, input
file bounds, complete collision retention and dependency freshness.

The term certificate supports module/namespace lookup, union/active-pattern
cases and literal/enum names. It does not enumerate arbitrary ordinary CLR
properties/methods as free names. `open type`/static-member import resolution
therefore remains a separate unsupported scope, not certified absence. The
current marker name is interpreted only within this explicit consumer boundary.

The later pre-provenance-split cold-load profile used immutable
`8daa950d5b93397ad29aae2fd9ec59ca50e793856b241e6755d33fce1ac1bd1f`,
which is the passing 9201 source snapshot with six coarse timer edits only. The
source archive and exact patch are preserved in `evaluation/candidates`.
All 16 shadowing controls passed before the coordinated quiet measurement.

| Exact hover phase | Fresh SQLite | Reopened populated SQLite |
| --- | ---: | ---: |
| Request wall time | 896.43 ms | 209.02 ms |
| Project model, watchers and context | 25.66 ms | 20.01 ms |
| Rendered catalog import/validation | 538.91 ms | 110.04 ms |
| Complete inventory load | 323.52 ms | 48.41 ms |
| Inventory parsing/SQLite insert portion | 185.20 ms | 0 ms |
| Inventory dependency validation portion | 58.05 ms | 39.77 ms |
| Inventory commit portion | 67.25 ms | 0 ms |
| Inventory JSON fingerprint portion | 12.68 ms | 8.10 ms |
| Sampled native peak RSS | 23.57 MiB | 10.83 MiB |

Warm exact requests across the sessions took 0.54–1.19 ms. A third fresh database
loaded the project tree first (50.24 ms request, 44.21 ms recorded model load),
then returned exact hover in 805.93 ms. The phase counters are successful-load
wall durations; request timing excludes stats collection. OS page cache was not
flushed, CPU accounting has coarse OS ticks, and there is one run per condition.
There is no actual editor-host measurement. Raw data:
`test-results/inventory-memory/cold-phases-8daa-quiet.json`.

Explicit preparation was measured sequentially in the same quiet window using
immutable helper `479aceb46341`, without restore, build or semantic snapshot:
15.15 seconds total, comprising project evaluation 3.50 s, two catalog exports
5.17 s + 2.48 s, and complete inventory export 3.40 s. The maximum sampled helper
process-tree RSS was 425 MiB; all helper processes exited at completion. This
setup cost is separate from native queries and is not hidden in startup claims.
The exact command, helper hashes, phase costs and process-tree peaks are in
`explicit-preparation-479-quiet/setup.json` beside the native profile.

These measurements motivate two separately tested changes: reviewed portable
producer provenance to avoid treating the exporter implementation as a live fact
file, and explicit prebuilt read-only indexes to move the dominant import/commit
work into preparation. Source/reference/XML freshness, current ownership and
complete negative-name evidence must remain validated. No prebuilt-index speedup
has been measured or claimed here.

The schema2 post-change profile used native
`0a77bde1efcd0860b8804f963ac602c123308931da12a4ad3dc7ea0cbd68598b`
and freshly exported helper
`f215dac4599cb436d748ead64c01cf5f8795d8ddea79add66bb29cfdb6b763c2`
artifacts. All 16 genuine ownership/shadowing controls and eight adapter
scenarios passed before measurement. All active team workloads stopped; native
profiles and explicit preparation ran sequentially. The OS page cache was not
flushed, and each condition has one run.

| Exact hover phase | Fresh SQLite | Reopened populated SQLite |
| --- | ---: | ---: |
| Request wall time | 1,025.69 ms | 66.21 ms |
| Project model, watchers and context | 66.07 ms | 15.91 ms |
| Rendered catalog import/validation | 656.36 ms | 28.84 ms |
| Complete inventory load | 294.61 ms | 15.71 ms |
| Inventory parsing/SQLite insert portion | 170.72 ms | 0 ms |
| Inventory dependency validation portion | 31.02 ms | 9.04 ms |
| Inventory commit portion | 77.97 ms | 0 ms |
| Inventory JSON fingerprint portion | 14.57 ms | 6.16 ms |
| Sampled native peak RSS | 22.12 MiB | 10.55 MiB |

Warm exact requests took 0.58–2.47 ms. Loading the model first took 17.06 ms of
recorded model work, followed by an exact hover in 821.01 ms. Startup still read
and parsed zero source files and launched zero helpers/compilers. The separate
full-data check retained every exported type, scope and term row; all three
canonical digests equal the earlier framework values above. Its native startup
RSS was 5.58 MiB and peak RSS 22.24 MiB. Raw reports are
`cold-phases-0a77-f215-quiet.json` and `full-0a77-f215-positive-quiet.json` in
`test-results/inventory-memory`.

Explicit f215 preparation took 14.97 seconds: model evaluation 3.41 s, catalog
exports 4.78 s + 2.58 s, and complete inventory export 3.58 s. Peak sampled helper
process-tree RSS was 470.96 MiB; all helper processes exited. This excludes
restore, build and semantic snapshot generation. Commands and hashes are in
`explicit-preparation-f215-quiet/setup.json` beside the native reports.
This measurement uses the evaluation harness's single helper session across
phases. The actual installation/automation CLI starts separate helper processes
per phase, so 14.97 seconds and that peak do not measure the CLI workflow.

This is a combined-change comparison, not an isolated provenance speedup. The
inventory's live input bytes fell from 50,627,504 to 10,307,720 and the catalog's
from 55,738,032 to 15,418,248, while all selected binary dependencies remain
certified. The new catalog also contains corrected documentation, callable
facts and other interpretation changes: its JSON grew from 20,962,905 to
25,678,699 bytes. The artifact difference report is
`post-change-artifact-comparison.json`. Fresh import is still expensive; faster
reopened-cache results do not establish a cold-start improvement or editor-host
performance.
