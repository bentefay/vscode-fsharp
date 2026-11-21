# Generated parser source storage

The two generated tree-sitter C files are stored as deterministic gzip inputs
to keep individual repository uploads small. They remain build-time source,
not downloaded dependencies. `manifest.json` pins both compressed and expanded
sizes and SHA-256 digests. The expanded C bytes and all native source fingerprints
remain unchanged.

The larger F# gzip is split into seven numbered parts of at most 500 KiB for
repository transfer. Its `parts` manifest preserves their order and pins each
part's size and SHA-256, in addition to the existing whole gzip and expanded C
pins. Restoration streams the parts directly into gzip decoding; it does not
create a combined archive. Missing, reordered or changed parts fail validation.
The smaller signature gzip remains one file.

`cli/bin/run setup`, `build`, `check`, `test`, `verify`, `package`, `source`, and
installation that builds a package verify and restore these files automatically.
They still use the protected Nix/Safe Chain toolchain. The installed extension
and `engine` command do no parser restoration or source hashing at startup.

Before direct Cargo builds or `scripts/capture-native.py`, run:

```sh
node scripts/restore-parser-sources.mjs
```

Node uses only built-in modules. Restoration streams data with an 8 MiB compressed
and 64 MiB expanded hard cap per file, enforces the smaller pinned sizes, verifies
both hashes, and atomically installs without overwriting an existing file.
Existing expanded files must match the pins; local edits are rejected and retained.
Temporary output lives outside the fingerprinted vendor tree and is cleaned up on
failure. Source archives include the verified expanded files even though Git ignores
them. The source packager itself also rejects missing or modified expanded files.

The committed inputs were generated with Python's `gzip.compress(data,
compresslevel=9, mtime=0)` without a filename header. Repeated compression in the
same Python/zlib toolchain must produce the same bytes; compression implementation
changes can produce different compressed bytes and require explicit updated pins.
Do not update the expanded hashes unless deliberately updating the parser source.

Run the independent bootstrap checks with:

```sh
node --test scripts/restore-parser-sources.test.mjs
```
