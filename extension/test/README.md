# Extension tests

`suite.cjs` runs inside the real VS Code extension host and calls the registered
definition/reference/hover providers. It exercises the native Rust backend,
ordered project tree, scope shadowing, dirty overlays, explicit unsupported
bindings, cached external hover and zero compiler launches in indexed mode.
It also invokes native Go to Definition and can capture the private display.

The external hover case imports a deterministic catalog fixture. Actual .NET
assembly/XML extraction is tested independently by `semantic/tests/smoke.py`.

Run after building the Rust executable and compiling the TypeScript adapter:

```sh
extension/test/with-display.sh node extension/test/runTest.cjs
```

On Linux this uses private Xvfb, Openbox and D-Bus. With the pinned Nix
environment, those tools come from the development shell. A task-local tool
installation under `.tools/display/root` is also recognized. No user editor
profile is used. The wrapper never reuses the user's Wayland/display session.

Optional environment variables:

| Variable | Effect |
| --- | --- |
| `FSHARP_VSCODE_PATH` | Explicit VS Code executable; use the checksum-pinned runtime. |
| `FSHARP_VSCODE_VERSION` | Editor version label (default 1.108.0). |
| `FSHARP_BACKEND_PATH` | Native development binary override. |
| `FSHARP_TEST_VSIX` | Install this VSIX into the disposable profile; the development host contains only a probe. |
| `FSHARP_TEST_UNTRUSTED=1` | Direct host launch testing Workspace Trust without the standard test-electron override. |
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

## Current execution-environment limitation

During the 20 November 2026 implementation, both pinned VS Code versions were
downloaded and checksum-verified, and Xvfb/Openbox dependencies were prepared.
This cloud execution host denies AF_UNIX sockets (`EPERM`). The standard
private D-Bus launch therefore fails. A TCP X11 probe reached Electron main
initialization, but VS Code still requires a Unix main IPC server, preventing
the real test host from starting here. No native extension-host/UI test pass is
claimed for this environment. The failed launch log is retained under
`test-results/host-environment`. Run the harness on a normal Linux/Nix host or
CI before treating the extension as validated in VS Code.

The original example repository's runtime-lock.json was copied here to pin
VS Code 1.108.0 and 1.145.3. The automatic test-electron downloader is a
convenience fallback; release validation should supply the independently
checksum-verified executable through `FSHARP_VSCODE_PATH`.
