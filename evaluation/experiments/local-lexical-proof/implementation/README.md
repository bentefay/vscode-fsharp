# Isolated local parameter proof

This development harness does not register a module in navigator-core and does not change its engine fingerprint. The production owner will copy `local_lexical.rs` only after the review boundary is approved.

`query_with_metrics` takes an exact source snapshot, UTF-16 position, operation, cancellation token and mutable metrics. Both operations admit only ordinary callback lambda parameters or parameters of ordinary let-functions nested inside a function/lambda body. Public let parameters and direct exported/returned lambda chains fall through because actual FSAC can route their definitions to signatures. The entire chosen body must have positively recognized lexical roles; named argument ambiguity, complex binders, CE/quotation/interpolation/type roles and unknown shapes fall through. Rejection returns no partial set.

Metrics report rejected parses too. Guards bound source bytes, parse/total time, depth, visited work and result count; Linux additionally checks process RSS during parser progress. Only the OS process-status file is consulted for that guard. There are no project, index, catalog, source-file or library reads and no persistent tree/cache in this module. UTF-16 reference coordinates are mapped in one original-source scan.

Run `CARGO_TARGET_DIR=target scripts/with-local-tools cargo test --manifest-path evaluation/experiments/local-lexical-proof/implementation/Cargo.toml` from the repository root. `probe.rs` is a development-only stdin JSON source probe, not a shipping CLI. Actual source observations and corrected compiler/FSAC evidence are in the parent directory. No native integration or performance acceptance is claimed by these tests.
