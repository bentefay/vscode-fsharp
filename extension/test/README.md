# Extension tests

`suite.cjs` runs inside the real VS Code extension host and calls the registered
definition/reference/hover providers. It exercises the native Rust backend,
ordered project tree, scope shadowing, dirty overlays, explicit unsupported
bindings, cached external hover and zero compiler launches in indexed mode.
It also invokes native Go to Definition and can capture the private display.

The trusted host fixture builds an external F# DLL and XML documentation outside the
workspace, restores the consumer, and runs the explicit `prepare` CLI command
before launching Code. The prepared project, catalog and environment have to
match the helper and native process environment. The local lifecycle test records
verified first, warm, unsaved edit and backend restart answers from the registered
providers. The edit and restart samples include their triggering action.

Run after building the Rust executable and compiling the TypeScript adapter:

```sh
FSHARP_VSCODE_PATH=/absolute/path/VSCode-linux-x64/code cli/bin/run test integration
```

On Linux this uses private Xvfb, Openbox and D-Bus. With the pinned Nix
environment, those tools come from the development shell. A task-local tool
installation under `.tools/display/root` is also recognized. No user editor
profile is used. The wrapper never reuses the user's Wayland/display session.

Optional environment variables:

| Variable | Effect |
| --- | --- |
| `FSHARP_VSCODE_PATH` | Required explicit VS Code executable; use the checksum-pinned runtime from `cli/bin/run runtime fetch`. |
| `FSHARP_VSCODE_VERSION` | Optional expected version; must match the supplied editor's manifest. |
| `FSHARP_BACKEND_PATH` | Native development binary override. |
| `FSHARP_TEST_VSIX` | Install this VSIX into the disposable profile; the development host contains only a probe. |
| `FSHARP_TEST_TREE_FIRST=1` | Activate from opening the solution explorer before any F# document. The default opens the document first. |
| `FSHARP_TEST_UNTRUSTED=1` | Test Workspace Trust without trusting the generated fixture. |
| `FSHARP_CAPTURE_SCREENSHOTS=1` | Capture tree/editor/hover images with ImageMagick `import`. |

Profiles, logs, fixture sources and `results.json` are copied to `test-results`
after each run. Tests use observable conditions with deadlines. The test
launcher runs actual editor binaries; it does not substitute a JavaScript-only
VS Code mock when the editor cannot start.

`transport.test.cjs` is a separate Node unit suite for the JSON-lines transport.
It uses real child processes and a small VS Code cancellation-type double. It
checks framing, request IDs, cancellation, process failures, timeout errors,
bounded pending requests and pipe backpressure. A delayed child acknowledges a
5,000-edit burst: only the initial and latest document snapshots are serialized.
The overlay queue also checks close/reopen ordering and explicit retry after a
failed synchronization. These are bounded-buffer regression tests, not an RSS
benchmark;
it makes no claims about F# correctness or native editor integration.

```sh
node node_modules/typescript/bin/tsc -p extension/tsconfig.json
node --test extension/test/transport.test.cjs
```

`adapterContract.test.cjs` runs the compiled adapter with a small editor API
boundary double and a controllable real child process. It checks incomplete
results, obsolete versions, dirty dependency invalidation and hover ownership.
`nativeAdapter.test.cjs` uses that same boundary double with the **real Rust
executable** for dirty-buffer and receiver-proof integration. Neither suite is
a substitute for the real editor host. They do not measure VS Code activation
or editor latency. The host suite writes labelled activation/provider samples
when it can actually run; its memory samples cover the shared extension-host
process, not every editor child process.

```sh
node --test extension/test/adapterContract.test.cjs
FSHARP_BACKEND_PATH="$PWD/target/release/fsharp-navigator" node --test extension/test/nativeAdapter.test.cjs
```

Temporary workspaces use `TMPDIR`, falling back to `.cache/test-tmp` inside
this repository. The native adapter suite requires the current built executable;
the receiver-proof cases must fail against engines that do not yet implement
the claimed feature.

`nativeCancellation.test.cjs` observes real SQLite WAL progress before
cancelling a running native operation. It verifies a partial index can resume
and that a dirty dependency proactively cancels a reference scan, allows a
current definition query, then produces a complete reference set on retry.
The adapter portion uses the labelled editor boundary double. Its captured
timings are functional-test observations, not controlled editor benchmarks.

```sh
FSHARP_BACKEND_PATH="$PWD/target/release/fsharp-navigator" node --test extension/test/nativeCancellation.test.cjs
```

## Editor runtime

During the 20 November 2026 implementation, both pinned VS Code versions were
downloaded and checksum-verified, and Xvfb/Openbox dependencies were prepared.
That execution host denied AF_UNIX sockets (`EPERM`). The standard
private D-Bus launch therefore failed in that environment. A TCP X11 probe reached Electron main
initialization, but VS Code still requires a Unix main IPC server, preventing
that host from starting. The historical execution evidence is described in `docs/editor-host-status.md`.
A later Linux host with private Unix sockets ran both pinned desktop versions.

The original example repository's runtime-lock.json was copied here to pin
VS Code 1.108.0 and 1.145.3. The runner now launches Code directly because
test-electron injects sandbox-disabling flags. It requires an explicit verified
`FSHARP_VSCODE_PATH`, isolates XDG/temp/profile directories, and removes inherited
Electron/Node overrides and credential environment variables. An unavailable
sandbox is a failed gate, never a reason to change system security settings.
Historical no-sandbox results do not qualify this new route.


The first provider request in the trusted document-first test runs immediately
after opening the F# document. Both pinned editors route the public definition,
references and hover commands through the internal `_execute*Provider` commands.
Registering their activation events lets VS Code wait for registration before
dispatch. The language selector restricts provider callbacks to F# files. The
adapter creates its navigator, watchers, output channel and status item on first
F# demand. Set `FSHARP_TEST_PLAINTEXT_FIRST=1` to check a non-F# command starts
zero native sessions before the F# operations.

The direct F12, Shift+F12 and Show Hover actions have their own workbench command
IDs and activation events. In a fresh profile set `FSHARP_TEST_FIRST_EDITOR_ACTION`
to `revealDefinition`, `goToReferences` or `showHover` to exercise one of these
actions as the first operation after document open.
