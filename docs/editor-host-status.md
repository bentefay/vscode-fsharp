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
