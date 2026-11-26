# Navigation preview status

This checkpoint contains the Rust navigation engine, thin VS Code adapter,
progressive discovery, labelled definition chooser, opt-in incomplete references,
and isolated Linux x64 installer. It is a source checkpoint, not a Marketplace
release or a bundled binary distribution.

Exactness and completeness are intentionally conservative. Raw project membership,
unique spelling and ranking are not proof of a binding. Unprepared projects may
require an extra chooser action. References remain incomplete for many cross-file
cases; Possible References is an explicit, potentially noisy search, not a complete
semantic reference set. Unknown dirty linked membership can withhold navigation.

Focused automated tests cover complete/incomplete/empty references, repeat-safe
warning actions, cancellation and late response rejection, dirty/linked edits,
explicit chooser selection, transport behavior and installer write boundaries.
They do not replace real graphical acceptance or a matched performance benchmark.
Historical controlled editor testing is not a normal-sandbox Ubuntu qualification;
the narrower new sandboxed run is recorded below.
No general speed advantage or daily-use readiness is claimed.

## Daily-use acceptance checklist

| Gate | Current evidence / remaining requirement |
| --- | --- |
| Real-project Definition | Pinned Grace, FsCheck and compiler engine gate checks authored targets, raw readiness, dirty/close and restart. This small subset is not whole-project coverage; signature-policy and unsupported cases remain explicit. |
| Currentness | Engine version/range and candidate safeguards are tested. Natural in-flight UI cancellation and all linked/rebind interleavings are not implied by those tests. |
| References / Hover | The raw-source gate now requires complete document-local references for one closed singleton-array match binder, including dirty/rebind/close/restart. See [scope and validation](singleton-array-references.md). Cross-file completeness, native References stale-row behavior and prepared List.map ownership remain open; unavailable hover is not positive documentation coverage. |
| Speed / memory | Prioritize cold first useful Definition/References and first current result after edit. Warm behavior is a guardrail. Native-only timings/RSS/PSS are not editor overhead; no orders-of-magnitude gain is established. |
| Packaged acceptance | An exact package hash must pass clean install, activation, real gestures, edit/cancel, uninstall/reinstall in official Code with its normal sandbox. The sandboxed default gesture suite and untrusted activation now pass on Ubuntu 26.04.1/Code 1.147.0. Uninstall/reinstall, the original Ubuntu 24.04 target, prepared hover and broader timing qualification remain unverified. |
| Delivery | Public source is available; no downloadable, newly qualified daily-use binary is published. Build/package from the pinned source using the protected toolchain, record its hash, then follow the isolated acceptance procedure. |

Use [the engine gate](../evaluation/engine-corpus/README.md) before publishing
navigation changes. For package installation and rollback, follow
[isolated installation](isolated-installation.md); it never launches Code
implicitly. Do not substitute a normal profile or disable the sandbox to complete
acceptance. Packaging a new build alone does not qualify it for daily use.

Before a private Ubuntu desktop preview, verify a clean install, actual native
activation, exact F12, ambiguous offer and explicit selection, cancellation/edit
freshness, references messaging, project tree and reinstall in an isolated official
Code profile with its normal sandbox. Library hover needs existing catalog inputs.
The first intended target is Ubuntu 24.04 x64; older distributions are unqualified.
The minimum declared Code version is 1.108.0. Nix shell and isolated editor activation were exercised on Ubuntu 26.04.1;
coexistence with Ionide remains unverified. Explicit semantic
preparation under Nix retains its existing loader/locale restrictions.

An Escape press before provider registration and a previously observed concurrent
availability miss remain qualification limits. Cancellation tests demonstrate
rejection of controlled late delivery, not every natural timing interleaving.

## Security maintenance evidence — 2026-11-25

The native dependencies now use quick-xml 0.42.0, rusqlite 0.43.0 with bundled
SQLite, url 2.5.9, notify 8.2.2 and postcard 1.2.0. The obsolete bincode and
instant dependencies were removed. Persisted syntax-summary fingerprints changed
with the serialization format. Special source inputs are refused before reading;
Unix opens also use nonblocking mode to close the FIFO replacement race. Explicit
semantic helper transport now bounds messages and queues and applies deadlines
and cancellation while writing stdin, with owned process-group cleanup.

The preserved pre-fix regressions showed a blocked FIFO index request and helper
stdin/output failures. The real-native adapter gate subsequently passed all 22
checks, including a FIFO, a symlink to a FIFO, a directory, authored definition
ranges after dirty edits, and restart. All pinned Grace, FsCheck and compiler
corpus suites passed: 32 definition obligations and 32 reference obligations,
including 28 complete-set checks and four stale-result rejections. All inputs
were present and unchanged. The eight separately reported known-failure probes
remain distinct: two useful labelled candidate outcomes and six unavailable
outcomes; these are not extra required passes.

The immutable local VSIX SHA256 was
`a82fc3dbdf2ee2df683a813e7136c839b395b5087975150277b52507dd066881`,
with native SHA256
`488b048e02cb07de3c46709eb61723295b6b669b715bed7db385485fb9dfb7d4`.
Its default packaged UI gate passed 36 checks (including two baseline checks)
with one existing signature-policy XFAIL kept separate. A separate packaged
untrusted-workspace test confirmed that no native engine starts. Both used
private disposable profiles and Chromium's sandbox, under official system
Code 1.147.0 (executable SHA256
`c9b5e01f7a13f7fcf11b378a8918856ba3e9aa27fc75d0cb85be695fdfa1c925`)
on Ubuntu 26.04.1 x64. No normal profile installation or publication occurred.

The checksum-pinned downloaded Code 1.145.3 archive could not start its sandbox
on this host; it was not given a sandbox exemption. The existing system Code
installation already had a configured sandbox. An initial system-Code attempt
also failed because of an overlong private IPC path; the harness now shortens
that path. These failed attempts are distinct from the final passing run.
Neither this suite nor the corpus proves prepared library hover, every natural
cancellation timing, native References stale-row clicks, other platforms, or
daily-use readiness. No performance comparison was performed.

Tool evidence: Rust/Cargo 1.100.0, Node 24.22.2, npm 11.19.1, Safe Chain 1.5.29.
The editor/native checks initially used SDK10.0.400/runtime10.0.11 only as a
shell tool; indexed mode launched no managed helper. The final development
pin is SDK10.0.404/runtime10.0.15 after the extended NuGet audit found advisories
in the old optional runtime packs. The semantic preparation profile remains
unqualified and its old exact version checks are intentionally retained.
Local raw logs and reports are under `.cache/security-*`; generated packages
and disposable test profiles are not published artifacts.

Final checks on the patched Nix toolchain passed: the aggregate (359 Rust tests,
six explicitly ignored; 53 tooling/editor-launch checks; 132 adapter/transport
unit checks; Python gates), the 14-case real-assembly decompiler suite, and its
linux-x64 ReadyToRun build with runtime-pack SHA512 verification. Dependency
verification blocked Safe Chain's harmless malware fixture, performed a fresh
script-disabled npm install, and reported no npm vulnerabilities. The final
RustSec audits reported no vulnerabilities or denied maintenance warnings in
both Cargo lockfiles. The OSV audit matched no advisories in the three locked
NuGet releases, and 106 Cargo/NuGet releases passed the 48-hour age/listing
check. An earlier redundant Cargo yank lookup timed out; the final audit retry
and independent registry age/listing check succeeded.

The trusted `test ui` route built successfully but then failed at explicit
semantic preparation: the new SDK and Nix locale profile cannot produce an
accepted reusable certificate. No editor settings were generated. This is a
blocked prepared-editor gate, not a passing UI result or a reason to relax the
certificate. The separately passing unprepared packaged E2E and untrusted tests
remain the editor evidence for ordinary indexed mode.
