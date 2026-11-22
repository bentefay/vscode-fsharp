# Native References isolation: reproduction without Navigator

This bounded diagnostic starts from
`4cabef8c452d71b40c3a43ba82c27546e69ebc04`. One fixed pair reproduced the
post-edit stale sidebar activation both with Navigator and with an independent
public-API reference provider. **The sampled symptom does not require
Navigator.** Its exact workbench publication/cache boundary and persistence
after refresh remain unproven. No runtime change or passing coverage is claimed.

## Exact shipped source and handoff

The inspected official editor is Code **1.108.0**, shipped commit
`94e8ae2b28cb5cc932b86e1070569c4463565c37`, dated 8 January 2026. This review
used the installed files, not current upstream source:

| Shipped file | SHA-256 |
| --- | --- |
| `extensions/references-view/dist/extension.js` | `ce9546e5d5559e3670d7fc39866f3a3b1866cc883fcb479b2c6baf70e7cae749` |
| `out/vs/workbench/api/node/extensionHostProcess.js` | `afa5848ef337e9de69d9d52b3a0a1eb3918598a27211699815d31e0eb9c9b65c` |
| `out/vs/workbench/workbench.desktop.main.js` | `8a6a501cf89406f4066f3a600c0e1d427fba662eafd665e75fb66ab3e1fb68b5` |

Navigator's [registered reference provider](../extension/src/extension.ts)
awaits the query, then maps its locations into VS Code Locations. The query
checks cancellation, document version and epoch before returning exact data.
Its `lastResult` export is not an acknowledgement of renderer publication.

The shipped builtin References implementation creates row objects from the
returned locations. `getTreeItem` captures each location's range for a direct
`vscode.open` command; preview text is read separately from the document. There
is no explicit row generation ID or click-time provider revalidation. The
extension-host tree-refresh path has 200 ms batching and generation checks;
the main-thread tree maintains item handles and activates the **selected model
item's command**. A pointer-target DOM node is not that private model item.

Therefore identical labels, a fresh provider response, DOM replacement, or a
trusted click cannot expose the selected item's captured command generation.
The public API and passive DOM observations used here provide no command-range
publication acknowledgement. No dispatch monkey-patching, debugger inspection
of private objects, or modified editor was used to bridge that gap. The 200 ms
batching is source evidence, not a measured causal attribution.

## Fixed pair and reproducer

The predeclared order was one fresh Navigator profile followed by one fresh
control profile, with no retries or post-result harness changes. Both used the
same shipped editor, fixture, authored UTF-16 targets, strict keyboard/click
sequence, existing five-second assertion bounds and passive row/click observer.
There was no added delay or native-view clear between repeated requests.

The fixture and oracle are the unchanged `references-native-post-edit-repeat`
case in [the strict manifest](../extension/test/e2e/reference-currentness-regression.json).
Open its query inside the selected `value` declaration; invoke actual
Shift+Alt+F12, then click both declaration and use rows. Insert the authored
one-line comment prefix using an unsaved WorkspaceEdit, leaving disk unchanged.
Invoke the same shortcut at the shifted query, observe the fresh returned
reference set, and click the first currently hit-testable row without clearing
the view. Keep the original strict expectation: current caret `3:34`, not
former caret `2:34` (zero-based UTF-16). A timeout alone is not evidence of a
stale jump; the actual selection events are required.

The Navigator arm used immutable VSIX SHA-256
`8c3b8d8bb5e2c3ff929a1818f8376be1cecb2516d4f4219e33c6bc155ae17f66`.
The control installed no user extension or Navigator package. A throwaway test
probe contributed the F# language registration and used the supported
`vscode.languages.registerReferenceProvider` API, scoped to the fixture URI.
Builtin editor extensions remained available.

The control callback finds the unique literal fixture expression
`List.map (fun value -> value + 1) [20]` in `document.getText()`, checks that the
query is inside its declaration, and derives declaration/use offsets from that
actual text. It constructs Locations with `document.positionAt`, respecting
`includeDeclaration`, and returns them directly. It records the actual returned
Locations, document version, query position, source hash and cancellation flag.
This is an independent narrow fixture provider, not a general F# resolver,
compiler oracle, fabricated Navigator response or Navigator E2E coverage.

| Observation | Navigator | Independent control |
| --- | --- | --- |
| Baseline declaration/use clicks | Correct | Correct |
| Fresh post-edit version | 2 | 2 |
| Returned declaration/use ranges | `3:34–39`, `3:43–48` | Same |
| Actual first-row activation | Old caret `2:34` | Old caret `2:34` |
| Trusted down/up/click | Same intended row and hit node | Same intended row and hit node |
| Cleanup and input/fixture hashes | Passed / unchanged | Passed / unchanged |

Both strict arms failed at the first post-edit click. The planned second
post-edit repeat was not reached. The control logged two actual public-provider
callbacks with declaration inclusion and cancellation false; Navigator was
absent from its extension registry and installed-extension directory. This
positive control reproduction establishes that Navigator's analysis and adapter
are not necessary for this instance of the symptom. It does not identify which
editor cache, selected-item binding or refresh stage is responsible, nor rule
out other Navigator-specific defects. The arms have different asynchronous
work, so they are not a latency comparison.

Each failing phase retained 18 observer events with zero event drops and one
truncated mutation batch. The positive click and wrong-selection evidence is
usable; an exhaustive lifecycle or absence claim is not. No later deliberate
click tested whether the command became current after the observed updates.

## Decision and smallest separate remediation proposal

Stop this diagnostic loop. Preserve the strict first-click failure; do not add
clear/wait behavior to turn it green. The next warranted remediation experiment
belongs in the builtin reference-view input/row-command lifecycle: tag row
commands with their input generation and reject obsolete generations at
activation. First test that proposal against this independent control and
current-generation navigation. It is a **proposal**, not an established fix;
no generation guard, editor fork or Navigator workaround was implemented here.
Further work requires that specific, separately testable cause/fix hypothesis,
not more passive timing cohorts.

The ordinary 25 qualified scenarios and scoped signature failure are unchanged;
they were not rerun in this native-only pair. The separate native and qualified
union failures, prepared List.map gap and in-flight cancellation gate remain.
The control adds no E2E pass. Normal-sandbox Ubuntu acceptance remains pending
and separate from the existing controlled disposable route. No package,
release, performance or CI qualification is claimed.

The temporary private test route was restored byte-for-byte, and the original
strict manifests and product source are unchanged. Independent review checked
source identity, provider/control separation, authored sets, actual click and
selection events, and the attribution limit. Private evidence includes both
failed arms, source hashes, callback receipts, protocol and restoration receipt.
Protocol SHA-256:
`156e8a52d1db8cffe6e250f7ab3f656dd20a2aa4476641c9c31eb252745814f8`.
Private 50-file evidence inventory SHA-256:
`586b8a56cff6b9fc0a7c42f7fd8ab10cac0d9e014367ca885925212c565258c6`.
Raw evidence, private profiles and binaries are not published.
