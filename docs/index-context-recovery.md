# Reconstructed index and adapter checks

These are fresh checks after the workspace restore on 2026-11-20. They do not
inherit passes or performance claims from the lost VM state.

The native index now separates physical raw-name postings from project-instance
syntax summaries. `context.rs` preserves byte offsets while masking conditional
directives and inactive lines before parsing. Effective defines require an
explicit evaluated-compiler-arguments certificate; direct linked-file cursors
need a project selection. The adapter's existing project tree records that
selection and forwards `projectPath` while keeping one physical text overlay.

Request caches have a shared 2 MiB accounting budget, compact requested-name
export facts, and mutation invalidation. They do not retain whole workspace
ASTs. Previous-session dirty buffers are validated against saved bytes on first
use; never-saved buffers are removed from persisted postings and summaries.

All native semantic Rust sources and Cargo inputs participate in a build-time
fingerprint. Only the digest enters the binary. A different fingerprint clears
persisted summaries, exact answers and semantic snapshots. Explicit semantic
snapshot inputs now retain file identities as well as hashes: same-length
restored-mtime changes invalidate on Unix, while platforms without that identity
must rehash. Missing ambient inputs remain negative certificates.

Evaluated dynamic Compile/import membership is a filename-only certificate;
ordinary source content edits do not rescan directory membership. Model loading
checks the certificate on first demand. Explicit-only projects have no unrelated
directory dependency. Unsupported dynamic provenance requires preparation again.

Fresh commands completed:

- `scripts/with-local-tools cargo test -p navigator-core index::tests:: -- --nocapture`: 11 passes, including two context-summary/export tests.
- `scripts/with-local-tools cargo test -p navigator-core context::tests:: -- --nocapture`: 6 passes.
- `scripts/with-local-tools cargo test -p navigator-core membership -- --nocapture`: 7 passes (4 filename-domain checks and 3 imported-model checks).
- Targeted semantic-input restored-mtime check: passed after identity hardening.
- `cargo test --locked -p navigator-core index::tests::changed_engine_fingerprint`: passed after moving hashing to build time.
- TypeScript compile and Node adapter/transport/content-provider tests: 23 passes. These execute shipped adapter code and real subprocess transport with an explicitly mocked editor API; they are not VS Code host results.

A real-native linked-context adapter test and the actual VS Code host tree-open
scenario are present. The linked test awaits the integrated native candidate;
the actual host remains blocked by this VM's AF_UNIX socket restriction. No new
GUI retries or host-pass claims were made. No controlled performance measurements
have been taken for this reconstructed version.
