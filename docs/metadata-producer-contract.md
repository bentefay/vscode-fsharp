# Binary metadata producer contract

Development implementation: focused producer, catalog, inventory, XML-selection
and backward-compatibility gates pass. Full navigation integration and matched
timing replay remain separate gates.
The immutable pre-change helpers/candidates remain available for comparison.

New binary catalogs, catalog manifests and complete import environments require
`schemaVersion:2`, `factsVersion:2`, `metadataCertificateVersion:1` and a `producer` object. Native consumers enforce
the reviewed `fsharp-navigator.binary-facts` / `binary-facts-v2` algorithm,
component versions, actual FCS assembly/informational profile and .NET 10.0.11
runtime profile. The implementation-hash list must contain exactly the exporter
and FCS roles with valid SHA256 values. Unknown, missing, obsolete or revoked
contracts are rejected; a version2 fact field alone is not sufficient. Earlier
binary-facts-v0/v1 algorithms are explicitly revoked. The incompatible schema2
envelope makes older native readers reject these artifacts instead of ignoring
the new guards.

This contract is an acceptance policy for locally generated facts, not a signed
attestation of an arbitrary producer. Implementation hashes remain immutable
provenance in the artifact but no longer require those producer-only executable
files to remain installed. The consumer can revoke reviewed algorithm IDs or
specific role/hash pairs. A policy change also invalidates persisted derived
indexes; it cannot reuse rows solely because the JSON bytes stayed unchanged.

`inputHashes` retain the live dependencies: selected assemblies, relevant PE and
F# signature/alias/measure/forwarding dependencies, and XML comments. There is no
filename exemption. When FCS or the exporter itself is selected as a project
library, its dependency role remains live. Extraction still verifies producer
and input stability before publishing an exact artifact. Source-semantic
snapshots and evaluated MSBuild models are outside this change and retain their
separate certificates.

An import environment must additionally cover the current effective reference
bindings and current workspace-owned assembly exclusions. Those roles must
participate in any future prepared-index derivation key. Complete type/scope/term
markers preserve their precise existing interpretation; this version does not
claim arbitrary `open type` or static-member-import support.

Existing catalogs/environments without the new contract must be exported again
with the current helper. Do not edit their version/provenance fields by hand.
The CLI checks the producer response and requires identical producer evidence
across catalog shards. Rust validates the artifact independently before use.
Normal indexed startup still starts no compiler and performs no metadata import
until an external proof needs it.

The `xmlSelection:1` component certifies the exact ordered resolver prefix:
sibling `.xml`, sibling `.XML`, then parent-directory `.xml`, stopping at the
first selected regular file. If no file is selected, every candidate is retained.
Missing paths and directories are distinct negative witnesses; a directory's
contents are irrelevant. Higher-priority file creation, selected-file removal or
changed bytes invalidate the old facts. Lower-priority additions do not. There
is no per-symbol fallback to another XML file and malformed selected XML fails
export. The selected path is captured before extraction, the exact parsed bytes
are hashed, and the prefix is checked again before publication.

The delivered helper's file-kind adapter is explicitly Linux x64: nonblocking
open plus descriptor-based `statx` rejects FIFOs and other special files without
reading them. Permission/unknown errors fail closed. Symlinks follow the actual
resolver target; a dangling link becoming a file invalidates its missing witness.
Other platforms require a tested adapter, not an assumption that `File.Exists`
means a regular file. XML input and parse limits are both 64 MiB.

Implementation locations: `semantic/MetadataContract.fs`, `MetadataXml.fs`,
`crates/navigator-core/src/producer_contract.rs`, `metadata.rs`, `builtins.rs`,
and `cli/commands/prepare.mjs`. Current immutable helper f215dac4599c has captured
source03d7ccc0fa2c. Four actual producer role checks, five native catalog tests
(including the genuine role and XML replay), twelve producer XML priority/error
cases and nine native cached/reopened/fresh XML scenarios pass. The old9201 and
8daa readers pass their legacy positive controls and reject new schema2 catalog
and environment artifacts (six checks). New-reader inventory checks retain
selected dual-role dependencies and reject legacy/revoked contracts. These
are scoped correctness results, not complete Ionide parity or a speed claim.

Evidence: `test-results/recovery/metadata-provenance.json`,
`metadata-provenance-native.json`, `xml-selection-producer.json`,
`xml-selection-native.json`, and
`test-results/inventory-memory/producer-policy-schema2-tests.json` plus
`old-reader-schema2-rejection.json`. The earlier e873/v1 result remains historical:
it established role separation before the missing-XML candidate counterexample
required a new revoked/accepted contract boundary.
Prebuilt read-only indexes and idle warmup remain separate experiments described
in `metadata-cold-start-design.md`.
