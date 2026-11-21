# Independent review: bounded model-validation digest reuse

**Whole-engine acceptance is blocked independently of this optimization.** Both baseline 85d1 and optimized 858f produce a compiler-demonstrated wrong exact static-factory field under an inline SRTP constraint (`factory-trait-native-source-858f.json`; additional inferred-owner cases in `factory-trait-inferred-oracle.json`). The digest change's mutation checks and matched performance observations remain valid scoped results; they are not release acceptance.

Status: source/module and immutable integrated correctness gates pass. Optimized native is `858f9d9a16d39338b63406916bf0fdf0bb2138a3b7b2ae9efd9bdc0d316bd14d`, compared with85d1. The reviewed production module SHA256 is `2a878791edfc7ade9624155ad8bae63195f1d5f7e5ea07c3b62e18dee3317d56`. Matched performance evidence is reported separately; no all-feature acceptance follows from this optimization.

The change reuses an actual SHA256 digest within one model-validation generation. It does not reuse a certificate-validity decision. Every regular-file path is opened nonblocking, checked against its descriptor, and checked again after either a read or cache hit. The key includes device, inode, mode, size, and nanosecond modification/change stamps. A retained descriptor prevents inode recycling while its digest is cached. The cache retains at most128 entries/descriptors; eviction requires a new read. A changed previously observed inode, an expected-digest mismatch, cancellation or another validation error ends the generation. A new generation starts without those cached digests.

Source review confirms that each input-hash/globalization row still compares its own expected value. Missing/directory states, all environment values, selection-directory contents, raw path links and the globalization before/after snapshots remain checked. The existing process-local globalization snapshot cache is unchanged. There is no persisted content trust, and unsupported physical-identity platforms do not reuse digests. This is not an atomic-filesystem-snapshot guarantee.

## Independent deterministic tests

`digest-generation-module-stable-inode.json` passes6/6 integration tests against a byte-identical copy of the production module. The implementation owner's unit tests are excluded from that compilation. A deterministic cancellation stub exercises the module's error boundary; it is not a claim about daemon transport cancellation.

- Direct path, hardlink and symlink share one actual content read while every expected digest is compared. A contradictory expected value after cache hits fails and poisons the generation.
- A same-inode, same-size edit with restored mtime between checks cannot reuse the old digest. The current generation fails; a fresh generation reads and accepts the new expected bytes.
- Symlink retargeting and same-length atomic replacement use the opened target identity. A remaining old hardlink whose ctime changed is refused in that generation and accepted only after a fresh read.
- Cancellation after a partial large-file read does not publish a digest and prevents reuse of earlier successes. A fresh generation succeeds.
- A FIFO is rejected without blocking; missing and directory sentinels retain their distinct meanings.
- 150 distinct files leave at most128 retained descriptors. An evicted entry is rehashed, a retained entry is reused, and dropping the generation releases all retained handles.

The earlier developmental report tested a version that rehashed changed identities inside the same generation. The final module instead refuses that mixed generation, a stronger rule. Its tests require refusal plus a fresh positive retry; no stale cache hit is permitted. The earlier source hash is not the accepted module hash.

## Integrated baseline and next gate

`digest-generation-baseline-85d1.json` passes8/8 positive/safety controls on a genuine built SDK project and genuine09f v4 model. The model certifies one owned imported props file through direct, hardlink and symlink paths. Three positive controls return the complete original reference set. Same-mtime mutation and alias retarget are refused in both the existing session and a new process. A synthetic contradictory hash is explicitly a negative certificate test. Same-session watcher invalidation produces diagnostic unavailable responses; restart checks produce errors. Both safely refuse exactness, and neither counts as navigation coverage.

The immutable optimized candidate passes8/8 in `digest-generation-858f.json`, with the contradictory expectation on the last alias row so earlier rows can populate the cache. Existing locale/version gates pass7/7 in `culture-v4-858f.json`; mandatory bootstrap-proof gates pass5/5 in `culture-v4-bootstrap-858f.json`. Initial/fresh exact reference positives remain intact and native open still performs zero source/AST/helper work. No wrong exact result appears in these scoped replays.

The baseline manifest contains roughly369MB of hash rows but about75.5MB of distinct physical files. An additional exact-positive optimized native replay records369,222,367 logical requested bytes,75,545,697 bytes actually fed to the hash loops,57 digest-cache hits,161 regular-file reads and290 file-state checks. This verifies actual reuse rather than only computing a theoretical unique-file sum. These are hashing bytes, not physical disk-I/O counters. First-query wall time, process memory, startup and preparation costs remain separate measurements.

## Matched performance result and verdict

The test owner's coordinated ABBA comparison is `../../test-results/prepared-indexes/quiet-digest-858f/summary.json`, with all36 responses exactly equal to the correctness answer. The reviewer read its raw phase results, provenance and full lookup-table digests. The genuine project model, producer/live-input certificates and lookup payloads match; rebuilt descriptors and SQLite physical layouts differ with the required engine identity.

| Condition | Baseline85d1 | Optimized858f |
| --- | ---: | ---: |
| Full first hover, fresh process/cache |345–387ms|164–250ms|
| Full first hover, reopened process |342–504ms|189–286ms|
| First-hover CPU, fresh process/cache |340–390ms|140–230ms|
| Fresh native peak RSS |16.85–16.92MiB|17.63–17.84MiB|

Each condition has two samples. OS page cache was uncontrolled. Candidate counters consistently show75,541,388 actual hashing bytes versus369,217,898 requested bytes,55 hits and zero additional warm-query hashing; this fixture is distinct from the owned props-alias review fixture above. Roughly4.9 times fewer bytes are hashed, but end-to-end latency improves by a much smaller amount. Native RSS increases slightly. No whole-editor, physical-disk-I/O or Ionide/Rider ratio is established.

**This bounded optimization is supported for adoption.** It preserves the reviewed correctness boundaries and produces a measured benefit on the matched exact query. Full language/corpus coverage and the overall low-footprint replacement goal remain separate, unmet acceptance gates. The full feature counts currently cited elsewhere retain their85d1 provenance until rerun on the successor.
