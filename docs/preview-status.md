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
Historical controlled editor testing is not a normal-sandbox Ubuntu qualification.
No general speed advantage or daily-use readiness is claimed.

## Daily-use acceptance checklist

| Gate | Current evidence / remaining requirement |
| --- | --- |
| Real-project Definition | Pinned Grace, FsCheck and compiler engine gate checks authored targets, raw readiness, dirty/close and restart. This small subset is not whole-project coverage; signature-policy and unsupported cases remain explicit. |
| Currentness | Engine version/range and candidate safeguards are tested. Natural in-flight UI cancellation and all linked/rebind interleavings are not implied by those tests. |
| References / Hover | Complete native references must remain distinct from opt-in incomplete possibilities. Cross-file completeness, native References stale-row behavior and prepared List.map ownership remain open; unavailable hover is not positive documentation coverage. |
| Speed / memory | Prioritize cold first useful Definition/References and first current result after edit. Warm behavior is a guardrail. Native-only timings/RSS/PSS are not editor overhead; no orders-of-magnitude gain is established. |
| Packaged acceptance | An exact package hash must pass clean install, activation, real gestures, edit/cancel, uninstall/reinstall in official Code with its normal sandbox. This gate is still unverified; historical controlled runs do not replace it. |
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
The minimum declared Code version is 1.108.0. Nix shell/editor activation and
coexistence with Ionide remain separate unverified gates. Explicit semantic
preparation under Nix retains its existing loader/locale restrictions.

An Escape press before provider registration and a previously observed concurrent
availability miss remain qualification limits. Cancellation tests demonstrate
rejection of controlled late delivery, not every natural timing interleaving.
