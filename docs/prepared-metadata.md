# Explicit prepared metadata indexes

Status: implemented development experiment. Focused tests, actual paired
framework navigation, full-map comparisons, corruption controls and all 16
JSON/prepared ownership cases pass on diagnostic native 7047. A quiet same-binary
comparison measured fresh lookup at 740–991 ms via JSON import versus 87–125 ms
via prepared indexes; reopening was similar. Explicit native preparation cost
another approximately 790 ms, beyond helper extraction. See
`test-results/prepared-indexes/quiet-7047`. This candidate retains an unrelated
signature-file active-pattern safety blocker and is not whole-engine acceptance.
There is no automatic background job or query-time index builder.

The later immutable f27 candidate passed freshly rebuilt complete-map,
corruption and sixteen paired ownership gates, plus independent replay. Its
Some signature/scope safety regressions now pass by returning unsupported
payloads unavailable. The timing values above remain 7047 measurements; see
`prepared-index-evaluation.md` for the separate provenance and required checks.

The Rust engine can build the existing streamed catalog/inventory tables before
an external navigation request. After building the current engine, prepare a
**new output generation**:

```sh
cli/bin/run prepare --root /path/to/workspace --out /path/to/new-generation --native-index
```

Use the existing `--assembly`, `--helper`, `--metrics` and new `--backend` options
to pin the same inputs/binary as an experiment. Without `--assembly`, the CLI's
existing behavior exports all selected external assemblies. To reuse already
exported schema2 facts without repeating helper extraction:

```sh
cli/bin/run metadata prepare catalog --input /path/to/libraries.json --output /path/to/new/libraries.fnmeta
cli/bin/run metadata prepare environment --input /path/to/builtins.json --output /path/to/new/builtins.fnmeta
```

Point the existing `libraryCatalogPath` and `builtinEnvironmentPath` settings at
those descriptors. The project model setting stays separate. `.fnmeta` is an
explicit format choice: a malformed descriptor fails instead of falling back to
JSON. Ordinary source startup does not open or hash these indexes; the existing
lazy proof edge does. The source JSON can be removed after successful sealing;
actual assembly/XML inputs remain live.

Each descriptor binds the source artifact digest, complete dependency and
applicability certificate, native engine/builder fingerprint, producer policy,
canonical semantic-row digest and actual closed SQLite file digest. Inventory
certificates include all reference identities and workspace exclusion roles.
Positive-only catalogs carry no absence or workspace-ownership guarantee: current
model, complete inventory, ordered binding and source-ownership checks remain
mandatory at query time. Merely sharing a catalog never makes a workspace member
eligible for an external hover.

Publication uses a private database, bounded streaming imports, an 8 MiB SQLite
page cache, DELETE journaling and synchronous finalization. Schema/columns and
complete headers are checked, then the closed file is hashed. Actual live inputs
are rehashed after sealing and checked again as one generation immediately before
publication. A digest-named database is installed before an exclusive atomic
`.fnmeta` descriptor link. Failures cannot expose a partial descriptor; unused
digest files can remain after interrupted publication. Existing output generations
are refused. `prepare --native-index` reserves its new output directory before
writing even a model, and publishes settings only after both artifacts succeed.
This is a fixed-session experiment, **not hot atomic replacement of a live pair**.

Runtime hashes the complete descriptor and database on first use, opens SQLite
read-only with `trusted_schema=OFF` and `query_only=ON`, checks its exact allowed
tables/columns, rejects WAL/SHM/journal sidecars, and validates the real binary/XML
certificate. It does not use SQLite `immutable=1`. Current file identities/change
times protect warm reuse; conservative content checks remain on platforms without
Unix identity support. Corruption, missing rows after file mutation, changed
inputs or revoked builder/producer policy cause rejection, never exact absence.
Per-artifact publication and runtime checks are distinct from cryptographic
attestation of an untrusted producer.

Current guards are a 2 MiB descriptor, 1 MiB certificate header and 1 GiB database
per artifact. Page-cache budgets are separate from total process RSS and from the
mutable source index. Report native preparation, temporary/final disk bytes,
complete first-use bytes hashed, live-input bytes, cold/warm/reopened latency and
whole helper/native process trees. Moving import work to explicit preparation is
scheduling, not eliminating its cost. See `metadata-cold-start-design.md` for the
frozen JSON-import baseline and `review/preparation-cost-audit.md` for further
helper-cost hypotheses.
