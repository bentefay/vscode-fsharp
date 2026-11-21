# Actual editor-host execution status

On 21 November 2026 the Linux x64 desktop harness ran with private Xvfb, Openbox,
D-Bus and disposable VS Code profiles. Both checksum-pinned releases, 1.108.0
and 1.145.3, executed the original trusted suite against native baseline
`710f046e290da02e8e85362add92125334f5ec7e1bb498f1201fc25eb6905fed`.
Each passed 11 of 12 cases. The missing operation was the external F# module
value hover for `ExternalLibrary.answer`. Both editors passed ordered file
opening, definition, references, workspace hover suppression and dirty-buffer
checks. The indexed engine reported zero helper starts.

A subsequent 1.108.0 run installed the baseline VSIX in the disposable profile.
The expanded suite built a real external DLL and XML file and prepared the
consumer explicitly. The same module hover remained unavailable. The new local
definitions and complete references passed before and after an unsaved edit and
daemon restart (12 of 13 cases). Separate packaged Restricted Mode runs passed
in both desktop versions. Functional run data do not measure a cold extension
activation when Code has already activated the extension.

## Verified Linux x64 package

The package from revision `684ec70` contained the frozen Rust daemon and
13 approved VSIX entries. Its SHA-256 was
`7ba55d3891d20d0542691a1c6ea0e27875f249066885483e8913da35b5fb7448`.
The unpacked daemon matched
`7068357ab7a77bff7d362009a48ef08d6e6276ae4f74b4251a8b00b40f8393f8`.

Both VS Code 1.108.0 and 1.145.3 passed all 13 trusted cases and the installed
Restricted Mode check in separate disposable profiles. The trusted tests cover
the external module's signature and original XML documentation, workspace hover
suppression, complete local references, dirty buffers, and ordered linked
sources. Opening the repeated virtual folder runs A, B, A preserved compile
order, and selecting a linked file opened its physical source. The tests
observed zero helper or compiler launches in indexed mode. In the tree-first
1.145.3 run the solution view activated before an F# file opened.

## Fixed Grace46 through desktop providers

VS Code 1.108.0 executed the original 23 definition and 23 reference queries
against the pinned Grace revision. A fresh profile and a full restart against
the same profile produced the same counts.

| Installed provider and configuration | Definitions | Complete reference sets | Correct | Unavailable | Incorrect |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original Navigator, on demand | 10/23 | 4/23 | 14/46 | 32 | 0 |
| Updated Navigator, on demand | 12/23 | 5/23 | 17/46 | 29 | 0 |
| Updated Navigator, precomputed snapshot configured | 12/23 | 5/23 | 17/46 | 29 | 0 |
| Updated Navigator, precomputed snapshot explicitly imported | 23/23 | 23/23 | 46/46 | 0 | 0 |
| Ionide 8.1.0 with FSAC 0.86.0 | 23/23 | 23/23 | 46/46 | 0 | 0 |

The optional semantic snapshot was generated in a separate, explicit 167.6 s
preparation step. Invoking the contributed **Index Workspace** command imported
it in 1.111 s on a fresh profile and 0.707 s after restart. Simply configuring
the files left first-demand coverage at 17/46. Indexed editor requests reported
zero helper starts and compiler launches in all three updated configurations.

The scorer checked the original files, query order, target ranges and complete
reference sets. Each Navigator first request reached the daemon. Both Navigator
on-demand results matched their independent native replays. Ionide used its
actual C# and .NET runtime extension dependencies. It also returned 46/46 on
a second full pass after each measured first pass. The later pass is recorded
separately. [Desktop Grace evidence](../evaluation/editor-lifecycle/2026-11-21-grace.json)

## Matched desktop lifecycle

The two-line fixture uses one lambda parameter, two references, and an unsaved
one-column insertion. VS Code 1.108.0 opened identical source and project bytes
in isolated profiles. The trial order was original, updated, Ionide, Ionide,
updated, original. Each trial included a full desktop restart using its own
profile. Package installation and preparation occurred before timing.

Times below are milliseconds. Each entry is the median of two launches. Warm
definition first takes the median of eight correct commands in each launch.
Launch-to-first includes starting Electron. Demand-to-first starts immediately
before the first registered command after opening the document. Edit-to-current
includes applying the edit, and any retries needed to see the correct range.
Peak RSS sums the full observed desktop process family, sampled every 50 ms.
Shared pages count in each process.

| Provider | Profile | Launch to first | Demand to first | Warm definition | First complete references | Edit to current definition | Peak RSS (MiB) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Original Navigator | fresh | 3,901 | 223.3 | 13.1 | 279.5 | 54.6 | 1,224 |
| Original Navigator | restart | 2,935 | 368.8 | 4.3 | 58.4 | 40.8 | 1,213 |
| Updated Navigator | fresh | 4,156 | 120.1 | 14.1 | 35.4 | 49.7 | 1,229 |
| Updated Navigator | restart | 3,332 | 466.5 | 7.4 | 35.8 | 33.2 | 1,198 |
| Ionide 8.1.0 | fresh | 8,409 | 2,903.8 | 4.2 | 100.6 | 121.1 | 1,611 |
| Ionide 8.1.0 | restart | 7,514 | 2,895.7 | 3.5 | 89.3 | 75.6 | 1,578 |

All 12 launches completed. The original and updated Navigator returned correct
immediate results in all four launches each, with zero helper starts or compiler
launches. The updated adapter began inactive in every launch. The original
adapter was already active. Ionide returned correct immediate results in three
of four launches. In one fresh profile the first dirty definition and reference
set used pre-edit ranges. The next requests returned current results 153.6 ms
and 156.6 ms after applying the edit. The updated Navigator used less full-tree
memory than Ionide on this fixture. Its warm definitions and launch-to-first
were slower than the original Navigator in these small samples. These results
do not establish orders-of-magnitude improvements or coverage parity.
[Per-launch measurements and artifact hashes](../evaluation/editor-lifecycle/2026-11-21-lifecycle.json)

## First request and activation

The original package's project-file activation made the adapter active at the
start of a trusted F# workspace with no open documents. Removing that event
by itself caused a reproducible first-command miss on 1.108.0. With the baseline
engine held constant, the unchanged Grace46 desktop replay fell from 14/46 to
13/46 in both the fresh and restarted profiles. The first definition command
returned no location before the extension registered its provider. The daemon
received no query. A separate retry returned the expected definition.

Both pinned versions route the public language feature commands through the
internal definition, references and hover commands. The adapter subscribes to
those three command activation events and delays construction of its navigator
until an F# provider or the solution tree needs it. A fresh inactive profile per
operation on each editor returned the expected first definition, all 53
references for the tested Grace symbol, and the external F# module signature
with its XML documentation. These activation-specific probes held the known
baseline engine constant for Grace and used the separately verified external
module candidate for hover. The direct workbench actions for F12, Shift+F12
and Show Hover need their own activation events. On both versions each action
missed when submitted immediately after document open with only the provider
events. Adding the action events opened the correct definition, issued the
reference request and showed the external hover. A plaintext definition command registered providers
but created zero native sessions. The adapter boundary test also observed zero
status items, output channels and file watchers before the first F# demand.
[VS Code 1.108.0 command mapping](https://github.com/microsoft/vscode/blob/94e8ae2b28cb5cc932b86e1070569c4463565c37/src/vs/workbench/api/common/extHostApiCommands.ts)
and [VS Code 1.145.3 command mapping](https://github.com/microsoft/vscode/blob/dcd3ad6e368fbe19d03f37054a40dbaf40d676f4/src/vs/workbench/api/common/extHostApiCommands.ts).

## Earlier host limitation

The 20 November host denied AF_UNIX sockets. Its launch failures describe that
environment.

A supported remote route was tested using official VS Code Server **1.108.0**,
commit `94e8ae2b28cb5cc932b86e1070569c4463565c37`. Its published server-web archive
SHA256 was verified as
`73d65772dad4eade8d40bc0f3aeb306e779ccc25fdaf5e7e26e86c01b540fd26`.
The unmodified server successfully bound a random loopback TCP port and started
its extension-host agent. An isolated web-client URL carried the standard
workspace-provider `payload` entries `extensionDevelopmentPath`,
`extensionDevelopmentKind=workspace`, and `extensionTestsPath`, pointing to the
existing real provider suite. No tunnel or account authentication was used.

The standard headless Chromium client failed before connecting the workbench:
`process_singleton_posix.cc:297: socket() failed: Operation not permitted (1)`.
The server additionally logged restricted `uv_interface_addresses`. No provider
result file was produced. Both owned processes were terminated after the bounded
attempt. Exact server arguments, version, elapsed time and redacted logs are in
`test-results/host-environment/server-1.108.0`. No editor binaries were patched,
and no IPC interception or policy workaround was used.

Relevant supported architecture: a web-only extension host cannot run this
Node/native extension; an actual remote Node host is required. The pinned server
source's `ExtensionHostConnection` uses socket transfer for Linux TCP connections
and skips its separate named-pipe listener. This made the server route worth
testing but did not remove the browser's own platform requirements.

Sources: [web extension capabilities](https://code.visualstudio.com/api/extension-guides/web-extensions),
[VS Code Server](https://code.visualstudio.com/docs/remote/vscode-server),
[pinned remote host source](https://github.com/microsoft/vscode/blob/1.108.0/src/vs/server/node/extensionHostConnection.ts),
and the [official release metadata](https://update.code.visualstudio.com/api/versions/1.108.0/server-linux-x64-web/stable).

## Reference-result normalization

The official `ReferencesModel` source was inspected at both pinned commits:
[1.108.0](https://github.com/microsoft/vscode/blob/94e8ae2b28cb5cc932b86e1070569c4463565c37/src/vs/editor/contrib/gotoSymbol/browser/referencesModel.ts#L158)
and [1.145.3](https://github.com/microsoft/vscode/blob/dcd3ad6e368fbe19d03f37054a40dbaf40d676f4/src/vs/editor/contrib/gotoSymbol/browser/referencesModel.ts#L158).
The files are identical. Its constructor sorts references and omits a new child
when the URI/range comparator equals the preceding child's comparator. Therefore
an evaluation of the References UI may deduplicate **identical URI and full
range** pairs symmetrically for both engines, while retaining raw duplicate
counts in evidence. This source-confirmed normalization does not remove missing
references or merge different spans, and does not constitute a real editor test.
