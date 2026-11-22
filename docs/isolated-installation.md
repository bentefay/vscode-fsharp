# Isolated Linux x64 installation

Use a trusted nonportable official VS Code CLI, an existing reviewed VSIX, and
an absent or empty absolute preview directory. From the protected source toolchain:

```sh
cli/bin/run install --vsix /absolute/path/navigator.vsix \
  --code /absolute/path/VSCode-linux-x64/bin/code \
  --isolated-dir /absolute/path/navigator-preview --dry-run
```

Dry-run describes arguments; it does not validate the executable or package.
Remove `--dry-run` to install. Explicit `--vsix` is mandatory; no build, project
preparation or editor launch occurs. Do not combine this option with `--profile`:
a named profile alone does not isolate editor data and extensions.

The child receives separate user-data/extensions directories and private XDG
cache/config/data/state/runtime and temporary directories. Inherited editor routing
variables are removed; HOME and the parent environment are unchanged. Default
editor directories, symlinks, portable layouts and nonempty unowned roots are
rejected. This is trusted-tool isolation, not protection against a malicious CLI
or concurrent hostile filesystem changes. The source toolchain may also use its
repository-local temporary directory.

Uninstall and reinstall using the same owned root:

```sh
cli/bin/run uninstall --code /absolute/path/VSCode-linux-x64/bin/code \
  --isolated-dir /absolute/path/navigator-preview
cli/bin/run install --vsix /absolute/path/navigator.vsix \
  --code /absolute/path/VSCode-linux-x64/bin/code \
  --isolated-dir /absolute/path/navigator-preview
```

Uninstall removes the extension and preserves profile data/cache. It requires the
matching ownership marker and cannot fall back to the normal profile.

## Explicit manual GUI acceptance

The normal-sandbox Ubuntu 24.04 x64 graphical gate remains unverified. Use an
approved desktop where official Code's sandbox already works. Do not disable it,
change helper permissions or alter Workspace Trust to make a test pass. From a
fresh terminal outside Code, explicitly launch only when ready:

```sh
preview=/absolute/path/navigator-preview
env -u VSCODE_PORTABLE -u VSCODE_APPDATA -u VSCODE_IPC_HOOK_CLI \
  -u NODE_OPTIONS -u ELECTRON_RUN_AS_NODE \
  XDG_CACHE_HOME="$preview/cache" XDG_CONFIG_HOME="$preview/config" \
  XDG_DATA_HOME="$preview/data" XDG_STATE_HOME="$preview/state" \
  XDG_RUNTIME_DIR="$preview/runtime" TMPDIR="$preview/tmp" \
  TMP="$preview/tmp" TEMP="$preview/tmp" \
  /absolute/path/VSCode-linux-x64/bin/code --force-disable-user-env \
  --user-data-dir "$preview/user-data" --extensions-dir "$preview/extensions" \
  --new-window /absolute/path/trusted-unprepared-repository
```

Record the package hash, editor version and OS. Test exact local F12, a labelled
cross-file offer with every plausible option reachable, deliberate selection,
edit invalidation, cancellation, references messaging and project browsing. Keep
Ionide outside this profile. Close the window, uninstall/reinstall, then repeat
one F12. No .NET SDK, compiler, restore or semantic snapshot is a default-navigation
prerequisite. Library hover requires existing optional catalog inputs.
