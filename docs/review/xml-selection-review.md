# Independent review: XML lookup dependency witnesses

Status: design/source audit, 2026-11-21. No fixed-helper or native replay acceptance is claimed here. The producer owner's actual exporter evidence is `test-results/recovery/metadata-xml-absence-before.json`: the old artifact's documentation is empty; restoring XML changes freshly exported documentation while all previously certified hashes remain valid.

The proposed ordered-prefix certificate is appropriate for the current resolver in `semantic/Semantic.fs`: sibling `.xml`, sibling `.XML`, then the parent directory's `.xml`. Record the selected file's content hash and all prior non-file candidates. If no file is selected, record all candidates. Do not certify lower-priority paths after a selected file; changes there cannot change the current lookup result.

The producer must capture the actual selected path and prefix before extraction, load documentation from that captured selection, and revalidate the prefix and selected bytes before publication. Independently recomputing selection during reading and during certificate construction risks certifying a different choice. Existing per-file hashing remains necessary even when the document cache is hit. The current renderer resolves inheritdoc within the selected document; this review found no external XML include-resolution path requiring an additional file closure.

The new binary-facts-v2/xmlSelection1 contract should explicitly revoke v1 for this guarantee. Updating an informational version without consumer enforcement is insufficient. Native cached and reopened artifacts must validate missing/directory witnesses, not just the files that existed during export. A missing or changed witness must reject stale facts, never become an exact empty hover result.

Required paired controls:

| Initial selection and change | Required result |
| --- | --- |
| All candidates absent, then one appears | Old artifact rejects; fresh export restores exact documentation |
| `.XML` selected, earlier `.xml` appears | Old artifact rejects; fresh export selects `.xml` |
| First candidate is a directory, then replaced by a file | Old artifact rejects; fresh selection follows the ordered resolver |
| First `.xml` selected, lower candidates change | Existing artifact remains valid |
| Selected file content changes with preserved size/mtime | Reject in-session and after restart |
| Selected file is removed and lower fallback remains | Reject old selection; fresh export uses fallback |
| Higher-priority XML exists but is malformed | Fail export, matching current behavior; do not silently fall through |
| Selected XML lacks one member while lower XML has it | No per-member fallback; selected document remains authoritative |
| Symlink target appears or is replaced | Follow actual resolver semantics and detect changed selection/content |
| Case-insensitive duplicate paths | Preserve resolver order while deduplicating equivalent dependency observations |

One independently executed file-kind probe adds a precision requirement: on this pinned .NET/Linux runtime, `File.Exists` returns true for a FIFO and reports Normal attributes. It does not prove a regular file. Raw evidence is `review/xml-kind-oracle.json`; no read of the FIFO was performed. Do not let the producer select/open such a path while the Rust consumer treats it as missing. Reject unsupported special file kinds consistently before reading, or enforce an equivalent documented regular-file precondition. Filesystem access/stat failures must not silently become a certified absence.

These tests establish selection/freshness behavior only. Full comment rendering equivalence, SourceLink, library-definition routing and comparative performance remain separate gates.
