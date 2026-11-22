# Overlay close/reopen lifecycle

The 22 November 2026 checkpoint follows the reproduced queue failure at
`d8722f224fee33ff0fc3bbbce428568f2e1c8f2b`. With A acknowledged at version 10
and another document's send held pending, closing and reopening A at version 1
replaced its queued close. The production queue sent `document/change` and the
real daemon rejected both the first flush and an explicit retry as stale.
A matched control that acknowledged the close before reopening succeeded.

The adapter now retains one close barrier independently of each path's latest
pending document. Text can still coalesce, but a new lifetime cannot dispatch
until the close is acknowledged. Failed closes retain the barrier; successful
closes reset the acknowledged version. Late text acknowledgements cannot remove
pending lifecycle work, and disposal prevents late acknowledgements from
reopening or restoring pending work. The native monotonic-version guard is
unchanged. No compiler, discovery, startup or navigation-proof policy changes.

Deterministic transport tests cover reset and reused version numbers, 5,000
coalesced close/reopen events with only the final text read, a final close,
failed-close retry, late successful/failed text acknowledgements, and disposal
during a close followed by a replacement queue. These are queue tests, not
actual editor close/reopen gestures. The separate real-daemon witness uses a
test-side send barrier: its repaired sequence is close then open at version 1,
and both flushes succeed. Neither witness establishes a UI cancellation result.

Aggregate validation passed: 3 Python tests, 51 Node tests, 326 Rust core tests
(with 5 ignored), 4 daemon tests, and 115 adapter/transport tests. TypeScript and
Rust formatting checks passed. Both independent reviewers approved ordering,
failure retention, bounded coalescing and the deterministic assertions.

A fresh private Linux x64 package passed the unchanged packaged-editor gate on
pinned VS Code 1.108.0: 31 required scenarios, plus the scoped signature XFAIL
(33 passing phases because two dirty-edit cases include baselines). All 18
sessions reported zero compiler/helper/decompiler starts. Fixture and harness
inputs remained unchanged. This controlled disposable-profile route is not
normal-sandbox Ubuntu acceptance, nor a new actual-editor close/reopen case.

Private evaluation identities (artifacts are not published):

- VSIX SHA-256: `ebd93dd2fec215f7cbf88933cdf96a50a06d5a78d5aea283b097117b74bfabc4`
- Unchanged native SHA-256: `2497670da324f7250b3027271d6291a1d0dcea64cfb7b27692793058be513483`
- Editor executable SHA-256: `6d3e558f9d17bf273e8fa0bd11c9988052696dcc18b3e8f4c020feff8e6f0e8b`

The raw-project Exclude/ProjectExtensions findings, retained definition-choice
activation, native References currentness and performance remain separate work.
