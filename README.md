# F# Navigator — prototype

F# Navigator provides definitions, references, external-library hovers and an
ordered solution explorer through a Rust daemon and a VS Code adapter. It is a
prototype. The native engine opens its disk index on demand. For supported
callback and nested-function parameters it can answer directly from an
acknowledged editor buffer, before opening the project model. Other queries use
the indexed resolver, which reports incomplete results when it cannot prove the
binding or the complete reference set.

The Solution view reads classic Visual Studio solutions and `.slnx` files.
Logical folders preserve compile order, and linked rows open the physical source.
Use **F# Navigator: Index Workspace** to prepare a broader source index explicitly.
Prepared projects can also supply external documentation or an offline semantic
snapshot. Indexed provider requests never start the compiler.

The measured coverage, native lifecycle results, editor checks and remaining
gaps are recorded in the [current status](docs/current-status.md).

## Engine modes

| Mode | Behavior |
| --- | --- |
| `indexed` (default) | Native Rust disk-backed indexing. Never automatically launches FCS. Only confirmed results are exposed through ordinary navigation providers. |
| `lexical` | Native syntax/lexical experiment and candidate discovery. Unsupported or ambiguous requests remain explicitly incomplete. |
| `semantic` | Explicit compiler-backed experiment/oracle using the separately built .NET helper. May require substantial source context. |

The default mode's resource controls bound SQLite cache, source-file inputs and
dirty-buffer text. They do not constitute a native process RSS hard limit. The
separate helper memory guard applies only to explicit semantic operations.
The native daemon processes requests sequentially and polls cooperative cancellation
during scans, indexing, dependency validation and catalog import. A single parser
or SQLite call is not preemptible, and no hard foreground-latency guarantee is made.
Do not enable Ionide in the same test profile: providers aggregate and a second
language server invalidates resource comparisons.

## Develop and test

The command layout follows
[vscode-git-tree-compare](https://github.com/bentefay/vscode-git-tree-compare).
Install Nix with flakes enabled, then:

```sh
cli/bin/run setup
cli/bin/run doctor
cli/bin/run check
cli/bin/run build
cli/bin/run test integration
cli/bin/run test ui
cli/bin/run verify
```

The generated Tree-sitter parser sources are stored as pinned compressed inputs.
These development commands verify and restore the exact C files automatically;
the installed extension does no decompression or source hashing at startup.
Before invoking Cargo directly, run `node scripts/restore-parser-sources.mjs`.
See [the parser storage notes](scripts/generated-parser-inputs/README.md) for
the bounds, checksums and regeneration procedure.

`cli/bin/run` enters pinned devenv; direnv is optional. This managed build
workspace can also use checksum-verified task-local tools through
`scripts/with-local-tools`. The task-local tool downloads are not included in
the source archive. The pinned Nix SDK has been evaluated and realized in a rootless store, and a
tiny F# restore/build passed. The full devenv extension build/test/editor workflow
remains unverified; Nix reusable preparation is still rejected by the current
glibc profile. See `nix/README.md` for the exact executed layers.

Node dependency acquisition uses the pinned Safe Chain wrapper and a 48-hour
minimum package age. Dependency lifecycle scripts are disabled; build commands
run explicitly. Cargo uses its committed lockfile. The helper references the
F# compiler assemblies shipped with the selected .NET SDK instead of fetching
a separate FCS package. The helper and ordinary indexed extension have separate
runtime lifecycles.

CLI checks and task-local tool commands default temporary files to `.cache/tmp`
inside the workspace, so indexes and compiler response files do not fill a small
system tmpfs or silently fall back into the repository root. An explicit
`FSHARP_NAVIGATOR_TMPDIR` overrides this location, including an intentional
`/tmp`; a custom `TMPDIR` is also respected. The inherited system `/tmp` default
is replaced. Choose a filesystem with enough free space for the requested workload.

The fast tests, real extension-host tests and packaged-extension tests have
different roles. Host/UI tests use isolated editor profiles and fixtures; no
normal editor profile is modified. The source archive contains the protocol in
`docs/protocol.md` and evaluation instructions in `evaluation/README.md`.
`cli/bin/run test adapter` runs the real Rust process against a mocked editor
API; it checks adapter/native integration and is not a real VS Code host pass.

The current native index uses `index-v9.sqlite` with integer path IDs. It rebuilds
lazily after an upgrade and leaves an old `index-v8.sqlite` untouched so an older
running process cannot lose its database. Remove obsolete v8 cache files only
after closing the editor and old backend. `indexBytes` reports the active v9
database and WAL, not leftover legacy cache files.

## Prepare project order and library hover

Restore and build the selected solution using its normal SDK workflow first.
Preparation is an explicit command that runs the .NET helper and exits:

```sh
cli/bin/run prepare --root /path/to/repo --out /path/to/prepared
# Optionally select --solution /path/to/repo/App.slnx and --framework net10.0.
```

This optional preparation writes an evaluated `projects.json`, external
declaration `libraries.json` manifest and adjacent catalog shards, a complete
selected-reference name inventory `builtins.json`, and a `settings.json` snippet. Basic native navigation does not require preparation.
Keep the prepared directory and its shards together. Copy the snippet's settings into the VS Code
workspace settings, or select its absolute paths manually:

```json
{
  "fsharpNavigator.mode": "indexed",
  "fsharpNavigator.evaluatedModelPath": "/path/to/prepared/projects.json",
  "fsharpNavigator.libraryCatalogPath": "/path/to/prepared/libraries.json",
  "fsharpNavigator.builtinEnvironmentPath": "/path/to/prepared/builtins.json"
}
```

Preparation preserves evaluated compile order and excludes workspace output
assemblies from library catalogs using evaluated reference provenance. It
exports all external assembly references by default; repeat `--assembly /path/to/library.dll`
to select fewer catalog targets. Each assembly is exported in a fresh one-shot
helper, streaming its catalog to disk; `--library-batch-size` trades larger
batches for fewer process startups. Referenced dependencies remain available for
type decoding. Selecting fewer catalog targets does not narrow the complete imported
name inventory: that inventory prevents a missing external alias from being
mistaken for an absent declaration. Batching bounds accumulated declaration output, not the compiler's
dependency metadata or a single very large assembly. Preparation can still be
expensive and is measured separately. Native catalog import streams into SQLite
on first use, and subsequent qualified-name lookups fetch only matching rows.
Persisted catalogs revalidate their captured hashes on the first external lookup
of a new session; warm Unix checks include change time, so restoring an old
modification timestamp does not preserve stale documentation.
The helper requires restored assets and the selected .NET SDK;
it does not restore the user's repository. Artifacts contain absolute paths and
input hashes. Recreate them after changing dependencies, projects or build
configuration, then run **F# Navigator: Restart Engine** in VS Code. A catalog
supplies declaration signatures/documentation, while
the Rust engine must still prove the use-site binding before showing a hover.
Ordinary workspace DLL rebuilds preserve project-order certificates and unrelated
library catalogs. A genuinely referenced binary remains part of its catalog's
dependency certificate, including dependencies erased from ordinary IL types
such as F# units of measure. Semantic snapshots retain the stronger binary inputs.

To move first-use SQL import into explicit preparation, add `--native-index`
and use a **new** output directory. The generated settings select sealed `.fnmeta`
descriptors and read-only databases; first use still verifies database bytes and
live binary/XML dependencies. This adds preparation work rather than eliminating
it. See [`docs/prepared-metadata.md`](docs/prepared-metadata.md) for commands,
freshness guarantees and the measured, narrowly scoped comparison.

An independent `--experimental-shared-context` option reuses one imported binary
context while producing the same selected catalogs and complete name inventory.
It remains a development experiment with separate correctness and resource gates;
it does not change default preparation. Its bounded process lifetime and pending
comparison are described in
[`docs/shared-metadata-experiment.md`](docs/shared-metadata-experiment.md).

Separate commands are available for automation:

```sh
cli/bin/run projects evaluate --root /path/to/repo --output /path/to/projects.json
cli/bin/run libraries export --root /path/to/repo --model /path/to/projects.json --output /path/to/libraries.json
cli/bin/run libraries environment --root /path/to/repo --model /path/to/projects.json --output /path/to/builtins.json
```

`prepare --semantic-snapshot` additionally runs an explicit compiler check and
exports a persisted occurrence snapshot. It can consume substantial time and
memory, is not part of default indexing, and is not a memory-saving substitute
for inference. The generated `fsharpNavigator.semanticSnapshotPath` setting
imports it into the native index. Any source/context edit invalidates imported
results; there is no automatic compiler refresh. Measure export/import costs and
unavailable intervals as well as warm lookup time. `--metrics /path/to/metrics.json`
records each preparation phase's wall time and, on Linux, sampled helper and
descendant RSS. Sampling adds overhead and can miss brief peaks; it excludes the
CLI/editor and is not PSS or cgroup memory. `--timeout-ms` controls the one-shot
helper deadline, including descendant cleanup.

After building the native executable/helper, `scripts/with-local-tools python3
scripts/prepare-smoke.py` verifies this full preparation/import workflow on a
temporary restored project. In the Nix shell, run the Python script directly.

## Build a VSIX or source archive

```sh
cli/bin/run package
cli/bin/run source
cli/bin/run install --dry-run
cli/bin/run install --profile 'F# Navigator Development'
```

The Linux x64 prototype VSIX bundles the Rust executable. It does not bundle or
start a .NET runtime. Building the experimental helper is separate:

```sh
cli/bin/run build --semantic
```

To install the downloaded VSIX on a **Linux x64** VS Code host:

```sh
code --install-extension fsharp-navigator-0.1.0-linux-x64.vsix
```

You can also use Extensions → Install from VSIX. Windows and macOS native
binaries are not included in this prototype. Test in a separate VS Code profile
with Ionide disabled, so its providers do not mask missing functionality.

Artifacts and SHA-256 checksums go to `artifacts/`. Source archives exclude
toolchains, build outputs, dependencies and corpus clones, and include completed
evaluation evidence. `release --dry-run` reports that publishing is intentionally
not configured. Nothing automatically publishes to GitHub or Marketplace.

Use settings under `fsharpNavigator` to select the engine. The F# Navigator
activity tab displays the project tree. Native project parsing reports when it
has not evaluated MSBuild conditions/targets; an explicit evaluation helper is
needed for authoritative generated/conditional input order. For a file linked
into multiple projects, open its row beneath the intended project in the F#
solution tree. Direct ambiguous file queries remain unavailable; the selection
clears when the document closes or the engine restarts. A project path alone
cannot select two distinct target-framework instances of the same project.

## Current boundaries

General type inference, overloads, cross-file scope rules and complete global
references require careful semantic algorithms. The prototype must report
unsupported/incomplete cases rather than silently return plausible text matches
as exact. Library-only hover is filtered by symbol ownership; a cached library
name is not proof that a use-site refers to it. See the generated evaluation
results for the actual implemented accuracy envelope and measured resource use.
External go-to-definition can open a read-only cached signature after proving
the binding. An optional separate ILSpy helper can instead materialize a C#
implementation document for a proven external symbol; it starts once for a cache
miss, exits, and uses no FCS. Configure `decompilerPath` and `decompilerArgs` as
described in `semantic/Decompiler/README.md`. SourceLink retrieval is not
implemented. Signature-only fallback is explicitly labelled and does not count
as equivalent when Ionide supplies source or implementation.

The Linux desktop harness exercises the packaged extension in disposable VS Code
profiles. See the [editor results](docs/editor-host-status.md) for versions,
startup measurements and provider coverage. The full Nix extension workflow and
cross-platform packaging still need validation. The [current status](docs/current-status.md)
records the active candidate and the unchanged evaluation corpora. Earlier
[0.1.0 measurements](docs/evaluation.md) belong to the archived prototype.
