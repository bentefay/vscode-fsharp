# Definition choice withdrawal after an unsaved dependency edit

This test-only checkpoint uses the unchanged runtime and private package from
`2cfdd7740592304f02b804413e8065119fde3ca8`. It adds one actual-editor workflow
contract, separate from native References and custom Possible References.

The fixture has two physical `Name` fields in `Types.fs`. A consumer using
`(List.head items).Name`, with `items: Person list`, has a known semantic target
but the default uncatalogued receiver retains both fields as uncertain choices.
The independently authored initial UTF-16 ranges are Person `1:16–20` and
Company `2:17–21` (zero-based). A blank line inserted after the module header
moves them to `2:16–20` and `3:17–21`.

The harness preopens the dependency without focusing it, sends trusted F12,
observes the nonmodal offer, deliberately opens the rendered incomplete picker,
and verifies both hit-testable labelled rows. With the picker still visible,
a supported WorkspaceEdit changes only the dependency. Its version increases,
its buffer becomes dirty, disk stays unchanged, and the consumer's text, version,
URI and caret remain unchanged. This is not a physical-typing test.

No candidate clearing, stale-command replay or fixed settling sleep occurs
between these actions. The fresh DOM attempt to target the old row dispatched
**no click**: both picker and offer had already disappeared. A second trusted
F12 produced exactly the shifted pair; trusted mouse down/up/click on the current
Person row reached its full new URI/range. Continuous editor/selection capture
through that selection showed no excursion to an obsolete target. Cleanup and
revert follow the whole interaction.

## Observed evidence and limits

One isolated run and one full-order run passed on pinned VS Code 1.108.0 with
the same package, editor and helper bytes. Inputs and fixture files remained
unchanged. The full gate passed **34 required scenarios plus the scoped signature
XFAIL** (36 passing phases because two existing dirty cases have baselines).
All previous 33 required cases and their fixtures remain unchanged. All 21
full-run sessions, and the isolated session, recorded zero compiler/helper/
decompiler starts. Both independent reviews approved the oracle and evidence.

Protected aggregate checks also passed: 3 Python, 51 Node tooling/manifest,
328 Rust core (5 ignored), 4 daemon and 115 adapter/transport tests, plus
TypeScript and formatting checks.

The qualified behavior is **automatic withdrawal plus refreshed deliberate
selection**. It is not a retained stale-row activation pass: no old row received
a click. Retained-row activation, rebinding/deletion, genuine in-flight
cancellation and continuous DOM-resurrection monitoring remain unverified.
No production failure or fix is claimed, and no performance inference follows.

Unchanged private VSIX SHA-256:
`d14a20e4c54a71b6592bfa8d0aacff6eea775338467f6f32eb58dd4f4bdca409`.
Native SHA-256:
`0e914d29479b838edf6cdcc1b2bc9da430a9dab2c66aae328f6c561ba979a1b4`.
Raw logs and packages are not published.

## Normal-sandbox platform boundary

This turn only inspected the existing Debian 13 x64 environment and helper
metadata. The official editor's sandbox helper remained agent-owned, mode 0700.
The preserved earlier isolated launch log reports: “The SUID sandbox helper
binary was found, but is not configured correctly,” followed by Chromium's
abort and its root-owned/mode-4755 requirement. That earlier error is historical
launch evidence, not a new Debian launch result. No denied route, namespace
probe or sandbox launch was retried; permissions and security were not changed.

A supported normal-sandbox Debian desktop would qualify that tested platform;
Ubuntu is not inherently required. Neither is qualified by this controlled
existing harness. A provisioned official editor whose normal sandbox works
unchanged remains the platform prerequisite.
