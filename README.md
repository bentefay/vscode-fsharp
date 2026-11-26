# F# Navigator (prototype)

A Linux x64 VS Code extension for F# navigation and ordered project browsing.
The default Rust indexer does not launch a compiler, MSBuild, restore or project
scripts. Compiler diagnostics, refactoring and general language-server features
are outside its scope.

## Navigation

Open a trusted F# workspace and press F12. Proven definitions jump directly.
Uncertain definitions offer labelled possible targets; explicitly open the chooser
and select a target. The chooser places ranked matches first and includes every
collected alternative in the same list. A likely match is not promoted to an exact
result. Background discovery
is bounded and progressive; document-local navigation does not wait for the whole
repository. A pending request can continue within its bounded indexing budget.

Edits, superseding requests, cancellation and workspace changes invalidate pending
results. Candidate selection revalidates current source before navigation. Escape
uses the editor's operation/picker cancellation, not a global keybinding; a press
before an operation registers is outside that cancellation surface.

Native Find All References returns only proven complete results. An incomplete
scope produces an explanatory message: VS Code's “No references found” is not
proof of no uses. **Show Possible References** is a deliberate opt-in action that
shows labelled incomplete spelling matches. Those matches never become native
complete reference results. Proven empty scopes do not trigger the warning.

The Solution view preserves F# compile order. Library-only hover requires an
explicitly supplied verified documentation catalog; no catalog is built implicitly.
Restricted Mode does not start the backend. Keep Ionide out of an evaluation
profile to avoid competing language providers; coexistence is not qualified.

## Development and installation

Use the committed protected toolchain entry point:

```sh
cli/bin/run setup
cli/bin/run check
cli/bin/run test adapter
cli/bin/run test installer
cli/bin/run package --output /absolute/path/navigator.vsix
```

The Nix/devenv route requires an already working Nix installation. Its full shell
and editor activation remain unverified; see [Nix limitations](nix/README.md).
Safe Chain protections, locked dependencies and disabled package lifecycle scripts
are required. The source CLI is a developer tool; an installed VSIX does not need
Node, Rust or .NET for ordinary indexed navigation.

Safe Chain wraps npm, not Cargo or NuGet. Rust uses pinned Cargo 1.100 with a
two-day minimum publish age and `cargo-audit`; the CLI also checks already locked
Cargo/NuGet releases for age and yanked/unlisted status before building. Age
evidence is cached for at most 24 hours. `cli/bin/run dependencies verify` runs
the Safe Chain blocking probe, npm audit, RustSec audits (including maintenance
warnings), and the locked NuGet OSV audit. Registry failures fail the check.
These advisory/age checks are not equivalent to Safe Chain malware scanning.
The decompiler runtime packs are included. SDK-provided assemblies and Nix
inputs remain separately pinned toolchain inputs, outside the package advisory
scan. Development uses patched .NET SDK 10.0.404/runtime 10.0.15; existing
semantic certificates for 10.0.400/10.0.11 are not qualified on it. Reusable
semantic preparation remains unavailable pending profile validation.

Use `cli/bin/run dependencies update` to resolve and audit Rust dependency
updates. Review the resulting lockfile before committing. Use `cli/bin/run format`
for Rust formatting. Do not use `cargo install` as an age-protected install path.

Editor tests launch with Chromium's sandbox enabled and require an explicit
official editor path; there is no sandbox-disabled fallback. On Linux x64,
`cli/bin/run runtime fetch` verifies and extracts the pinned Code 1.145.3 archive
into `.cache`, recording its path and hashes in `.cache/editor-runtime.json`.
Supply that executable as `FSHARP_VSCODE_PATH` (or `FSHARP_E2E_CODE` for E2E).

For an existing Linux x64 VSIX, the opt-in isolated installer requires a trusted
nonportable official Code CLI and a new or owned private directory:

```sh
cli/bin/run install --vsix /absolute/path/navigator.vsix \
  --code /absolute/path/VSCode-linux-x64/bin/code \
  --isolated-dir /absolute/path/navigator-preview --dry-run
```

Remove `--dry-run` to install. The isolated command never builds or launches a
window. See [isolated installation](docs/isolated-installation.md) for uninstall,
reinstall, launch instructions and the outstanding Ubuntu desktop gate.

Semantic mode, project preparation, snapshot import and external declaration
helpers are explicit experiments. They are not default-navigation fallbacks.
The [protocol](docs/protocol.md) describes completeness, budgets and freshness.
See [preview status](docs/preview-status.md) for current validation limits.

For the immutable packaged-keyboard test slice and its explicit limits, see the
[manifest-driven editor gate](extension/test/e2e/README.md). It is separate from
the build-and-prepare integration suite.
