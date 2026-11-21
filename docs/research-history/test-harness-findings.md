# Test, CLI and packaging research for the F# extension

Research-only note, 20 November 2026. No builds, extension-host tests or UI tests were executed for this note. All repository observations are from direct source inspection at `bentefay/vscode-git-tree-compare` commit `e18fd03ab412216c556e66451ac6bf7c5c0705e1`. Commands below for the new extension are proposals, not existing commands.

## Recommendation

Reuse the reference repository's complete three-layer test loop and disposable runtime machinery. Put the production engine, index, automation interface and benchmark data collection in Rust. Keep the TypeScript VS Code adapter small. Retaining a thin Node/Clipanion development launcher preserves the existing command structure and reuses the official VS Code testing ecosystem; it need not own substantive runtime logic.

Use the large open-source corpus for throughput, semantic coverage and invalidation evaluation. Keep deterministic hand-authored F# fixtures as the correctness gate. A repository with millions of test-data lines does not automatically give a runnable million-line compilation unit or a reliable navigation oracle.

## Exact conventions found in the example repository

| Existing pattern | Source evidence | Application to the new extension |
| --- | --- | --- |
| `cli/bin/run` enters the pinned devenv, preserves invocation working directory, then dispatches to Clipanion command classes | `cli/bin/run`; `cli/bin/run.mjs`; `AGENTS.md` | Preserve command names and behavior. Let dev commands call Rust binaries and the narrow .NET helper using argument arrays. |
| `check` is fast; `verify` is the full ordered pipeline | `cli/commands/development.mjs:15` | Add Rust fmt/clippy/tests and helper checks to `check`; preserve build → host tests → package → packaged UI → dependency checks for `verify`. |
| Integration tests run both pinned VS Code versions and trusted/untrusted workspaces | `cli/lib/testing.mjs:13`; `.vscode-test.cjs` | Run identical F# fixtures under minimum and pinned current VS Code, using installed runtimes rather than an arbitrary locally installed editor. |
| Untrusted tests launch the host directly because test-electron adds `--disable-workspace-trust` | `cli/lib/testing.mjs:26` | Reuse this explicit trust test path; prove untrusted workspaces do not invoke MSBuild, FCS, type providers or a restore. |
| UI tests install the actual VSIX, with no extension-development path | `cli/lib/testing.mjs:44–75` | Test the distributed Rust binary, helper files and grammar assets, not only the development bundle. |
| Editor, ChromeDriver and cached downloads are SHA-256 verified | `cli/lib/runtime.mjs:18`; `test/runtime-lock.json` | Extend locks to oracle VSIX/FSAC versions, grammar artifacts and helper/runtime dependencies. Preserve verification on cache hits. |
| Linux binaries are patched for pinned Nix loader and libraries | `cli/lib/runtime.mjs:30`; `devenv.nix:25` | Keep this for test editor tooling. Separately prove the released VSIX is portable outside the devenv and on a Nix machine. Do not ship accidental developer-store paths as a generic Linux release. |
| Every run gets short temporary paths, independent user-data/extensions/shared-data, isolated Git config and disabled hooks | `cli/lib/fixtures.mjs:6–61` | Separate all engine/index/oracle caches by test case as well. Short paths matter for editor IPC. |
| Common credentials/editor variables are removed, updates/telemetry/experiments disabled | `cli/lib/fixtures.mjs:20–59` | Reuse the deterministic settings and scrub inherited environment before loading public corpus builds. Do not imply this makes arbitrary MSBuild code sandboxed. |
| Private Xvfb + private D-Bus + Openbox; no inherited Wayland/desktop activation tokens | `scripts/with-display:6–22`; `test/ui/smoke.test.cjs:59` | Reuse verbatim in structure. Keep screen-dimension assertions so automated windows cannot accidentally attach to a real desktop. |
| Timeouts terminate the process group | `cli/lib/process.mjs:6–29` | Extend cleanup to Rust worker, FCS helper, MSBuild children and orphan detection. Windows needs an appropriate job/process-tree strategy, not an unsupported claim that killing one PID suffices. |
| Screenshots, workbench HTML, Mocha JSON and logs survive failures | `test/ui/smoke.test.cjs:65`; `cli/lib/fixtures.mjs:64`; CI artifact list | Add request traces, project manifests, cache statistics and metric samples. Save failing corpus/query IDs and exact snapshots. |
| Condition-based waits, rather than a fixed delay replacing assertions | `test/integration/extension.test.cjs:15`; `test/ui/smoke.test.cjs:8`; `AGENTS.md` | Await activation/readiness and document-version acknowledgements; keep deadlines. A readiness wait must not be excluded from cold-start reporting. |
| Npm installs through Safe Chain, minimum package age 48 hours, lifecycle scripts disabled | `nix/toolchain.nix:24–49`; `devenv.nix:11`; `cli/commands/development.mjs:63` | Keep the wrappers for the unavoidable Node tooling. Add Cargo/NuGet policy separately; do not describe the npm wrapper as protecting those ecosystems. |
| Packaging validates identity and contents, writes VSIX and checksum | `cli/lib/package.mjs:9–27` | Add exact manifest checks for platform Rust executables, execute permissions, helper/runtime and grammar. Exclude corpus, test caches, source maps with private paths and oracle binaries. |
| CI uses SHA-pinned actions, read-only repository permission, no persisted checkout credentials; invokes the same CLI | `.github/workflows/ci.yml:8–37` | Preserve this structure; upload correctness and benchmark evidence. Keep expensive corpus benchmarks separate from small per-change checks. |

The reference pins VS Code 1.108.0 and 1.145.3 in its current test matrix. These are inspected values, not a recommendation to carry the same current pin indefinitely. The source explicitly supports Linux x64 UI automation, macOS GUI sessions and Linux ARM integration, with no configured Windows development environment. Copying the harness does not establish cross-platform support for the new extension.

## Suggested layout and command boundary

Keep `cli/bin/run`, `cli/commands`, `cli/lib`, `test/unit`, `test/integration`, `test/ui`, `test/runtime-lock.json`, `scripts/with-display`, `devenv.nix`, `devenv.yaml`, `devenv.lock` and `test-results` recognizable.

Add a Rust Cargo workspace for the engine/library and CLI, plus a narrow .NET semantic/project helper. The Rust CLI and editor must call the same engine services so CLI parity does not test a separate implementation. Prefer one production Rust executable with `serve`, `query`, `index`, `project`, `benchmark` and `parity` subcommands. The dev launcher only handles toolchain/setup, builds, official editor harnesses and distribution.

Proposed commands:

| Command | Purpose |
| --- | --- |
| `cli/bin/run setup` | Protected npm install; locked Rust/NuGet dependency preparation; environment checks. No application build/restore hidden inside a language query. |
| `cli/bin/run check` | TypeScript typecheck, Rust formatting/clippy/unit tests and fast helper/CLI regression tests. |
| `cli/bin/run test integration` | Real VS Code providers and tree model on both editor pins; trusted/untrusted fixtures. |
| `cli/bin/run test ui` | Install the actual VSIX in a disposable profile and exercise visible behavior, then restart with preserved profile. |
| `cli/bin/run parity --corpus NAME --queries PATH --json` | Two isolated oracle/candidate runs, normalized comparison and complete discrepancy report. |
| `cli/bin/run benchmark --corpus NAME --scenario NAME --json` | Rust supervisor measures reproducible cold/warm/edit/eviction scenarios and process lifetimes. |
| `cli/bin/run verify` | Full deterministic tests, packaging/UI, and package policy checks; small benchmark smoke gate. |
| `cli/bin/run package`, `install`, `release` | Preserve example semantics, including `--dry-run`, profile selection and publishing only as an explicit command. |

If “Rust as much as possible” should include dev orchestration too, Rust can implement these same commands behind `cli/bin/run`. The tradeoff is rewriting mature ExTester/test-electron orchestration that still launches Node. Recommendation: keep the thin reusable wrapper, build the production automation and measurements in Rust, and do not spend semantic-engine effort eliminating a small dev-only JavaScript dispatcher.

Commit Cargo.lock, pin rustc/cargo through Nix and/or an aligned rust-toolchain file, and use locked builds. After explicit dependency fetching, frozen/offline builds are possible; Cargo documents `--frozen` as combining `--locked` and `--offline`. NuGet locked-mode restores and an exact helper SDK/runtime manifest need their own setup and verification. Workspace SDK discovery must still respect the user's `global.json` independently of the extension's helper SDK. [Cargo documentation](https://doc.rust-lang.org/cargo/commands/cargo-build.html)

## Functional parity harness

Use separate VS Code profiles and sequential sessions: oracle Ionide alone, then candidate alone. VS Code's execute-provider commands execute *all* registered providers, so running both extensions together can conceal missing results and double memory. Pin the actual Ionide VSIX and its included/downloaded FSAC identity, settings, SDK and corpus SHA. A source HEAD is not the shipped oracle.

From a real extension-host test, call `vscode.executeDefinitionProvider`, `vscode.executeReferenceProvider` and `vscode.executeHoverProvider` at fixture/query positions. Normalize URIs, line/character positions, and definition `Location` versus `LocationLink` representation before comparison. Compare full reference sets, not counts. The built-in references command documents only URI and position; test the `includeDeclaration` variants at the provider/protocol boundary separately rather than inventing another argument. [VS Code command reference](https://code.visualstudio.com/api/references/commands)

Golden fixture expectations complement the differential oracle. FCS/FSAC can have bugs and stale-state behavior, and the requested hover semantics intentionally differ from Ionide for workspace-owned symbols. For external hovers, compare selected overload, type/signature and documentation semantics, allowing documented harmless whitespace/Markdown differences; for internal symbols, assert no hover at all.

Each query record should specify repository SHA, project/TFM/configuration, source file and stable source anchor or validated position, all dirty document versions, request kind, expected ownership, exact target/reference identities, readiness state and maximum deadline. Record not-found, unresolved, cancelled, partial and complete distinctly. Do not score cancelled or truncated searches as successful fast queries.

Exercise actual unsaved buffers through `WorkspaceEdit`/TextEditor editing in host tests, including two dirty files affecting one another. Test a source edit while an older request is in flight, then require correct results for the current version. Run save, revert, reopen, branch-change, project-reorder and package-update sequences. Tree tests should compare evaluated compiler source order separately from the grouped display tree and verify linked/noncompile/generated items, active configuration and expansion/reveal preservation.

The GUI layer remains small and concrete: F12 reaches the right file/range; Find References opens the expected results; library hover appears while an internal-symbol hover does not; solution files display in the required order; linked files open; refresh preserves user expansion; a full editor restart retains usable cached project/index state; the Rust/helper binaries execute from the actual VSIX. Inspect both screenshots and exit status. Use real visible conditions and event completion, with deadlines and retained evidence.

## Performance measurement design

Separate “application-cache cold” from “OS page-cache cold.” A fresh index/profile with warm filesystem cache is still useful, but must be labelled. Do not drop machine-wide caches on the user's workstation. Install/restore the pinned corpus prerequisites before navigation measurements, then report that preparation cost separately and include a fresh-checkout setup scenario.

Run at least these scenario classes: empty application cache; warm editor restart; warm exact query; worker-evicted query; unsaved local-body edit; early-file inferred API change; broad/common-name references; project/restore configuration change; long idle and churn/cancellation sessions. Report launch-to-tree and launch-to-first-complete-navigation, not only request time after a lengthy readiness wait. Emit raw observations and p50/p95 with sample counts; warm repetitions do not substitute for independent cold trials.

Measure Rust and all .NET/MSBuild children, plus a separately reported full-editor session. Enumerate descendants dynamically and detect leaked children. Per-process RSS sums double-count shared pages; label RSS/PSS and do not call a JavaScript heap metric total extension memory. A dedicated Linux cgroup can report whole-group current/peak use and CPU/I/O when delegation is available. Cgroup memory includes page cache and some kernel state, so report it distinctly from process RSS. Preserve raw metrics and use the same method for the oracle and candidate; when unavailable, explicitly state the sampling/peak limitation. [Linux cgroup v2 documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html)

The Rust supervisor should attach query IDs and monotonic timestamps to project evaluation, cache hits, candidate discovery, semantic checking and result publication. This establishes whether a faster scan actually improves end-to-end latency. Track complete-correct query rate, false positives/negatives, memory-budget failures, cache misses, rechecked source volume and worker restarts next to timing. A low-memory run that frequently restarts and takes tens of seconds to recover is a failure for this use case.

Keep PR CI functional gates and small deterministic performance-regression fixtures quick. Run the pinned large-corpus matrix manually or on dedicated/scheduled benchmark hardware, using identical resource limits and no simultaneous agent builds to distort results. Do not enforce absolute sub-second p95 on noisy shared GitHub runners as if that were workstation evidence. Retain before/after distributions and raw failures.

## Risks to add to the plan

- The current plan must change “prototype the coordinator in Node, Rust only if necessary” to a Rust production coordinator, per the latest user preference. The VS Code adapter and existing test tools remain practical JavaScript boundaries.
- “A memory cap” is not a performance result; record rejected/unfinished requests and cold recovery. Keep correctness as a separate hard gate.
- Do not run oracle and candidate providers in one editor, because built-in command aggregation can manufacture apparent parity.
- The solution tree's visible folder order and the flattened compiler compile order are different invariants; test both explicitly.
- SourceLink/decompilation fallback requires packaged/offline and virtual-document navigation tests, not only a metadata-signature unit test.
- Workspace Trust needs a direct test path independent of the standard harness's trust-disabling option.
- A packed VSIX that works only inside the Nix developer shell is not a distributable success. Include a clean runtime smoke test and state every untested target.

## Evidence links

All repository anchors below refer to the inspected commit:

- [AGENTS.md](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/AGENTS.md)
- [Development guide](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/docs/DEVELOPMENT.md)
- [Integration and UI launcher](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/cli/lib/testing.mjs)
- [Runtime pinning and validation](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/cli/lib/runtime.mjs)
- [Disposable fixtures](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/cli/lib/fixtures.mjs)
- [Display isolation](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/scripts/with-display)
- [Packaged UI tests](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/test/ui/smoke.test.cjs)
- [Safe Chain wrappers](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/nix/toolchain.nix)
- [CI pipeline](https://github.com/bentefay/vscode-git-tree-compare/blob/e18fd03ab412216c556e66451ac6bf7c5c0705e1/.github/workflows/ci.yml)
- [Official VS Code testing guide](https://code.visualstudio.com/api/working-with-extensions/testing-extension)

Web retrieval references for the parent: VS Code commands `turn796908333view0` / `turn530582121view0`; VS Code testing `turn796908333view1`; Cargo locked/frozen `turn796908333view2` / `turn530582121view5`; cgroup metrics `turn796908333view3` / `turn530582121view3` / `turn530582121view4`. Parent should open any web source it cites in its final response.
