# Development toolchain handover

Updated 21 November 2026. This describes fresh environment setup performed after
commit `efda6f58`; it is not a native build, semantic test, or editor-host pass.

## Completed in the current workspace

Repository: `/workspace/scratch/dd9aeba5da3c/fsharp-navigator`.

- Installed official Rust 1.100.0 components into `.tools/rust` after verifying
  their SHA-256 values against the official versioned Rust release manifest.
- Installed Safe Chain 1.5.29 into `.tools/safe-chain`, matching the SHA-256 pin
  in `nix/toolchain.nix`.
- Installed .NET SDK 10.0.400 / runtime 10.0.11 into `.tools/dotnet`, matching the
  SHA-512 archive pin in `nix/dotnet-10.0.400.nix`.
- Created `.tools/bin` wrappers equivalent to the project's Nix protection:
  package acquisition passes through Safe Chain, minimum package age is 48
  hours, dependency lifecycle scripts remain disabled, and alternate package
  managers / `npm exec` are blocked.
- Ran `scripts/with-local-tools cli/bin/run setup` successfully. Both locked
  Node acquisition (156 packages) and `cargo fetch --locked` completed; the CLI
  doctor printed the expected tool versions. No dependency lockfile changed.
- No full `check`, native build, release build, benchmark or real editor test was
  run by this setup agent. Other implementation/review agents own their checks.

Fresh observed versions:

| Tool | Observed version |
| --- | --- |
| Node | 24.22.3, provided by the workspace runtime |
| npm | 11.19.1 |
| Safe Chain | 1.5.29 |
| rustc | 1.100.0 (`0ef1bc386`, 2026-11-09) |
| cargo | 1.100.0 (`3acc79c0c`, 2026-11-05) |
| rustfmt | 1.11.0-stable, from Rust 1.100.0 |
| .NET SDK / MSBuild | 10.0.400 / 18.9.6 |
| .NET runtime / ASP.NET runtime | 10.0.11 / 10.0.11 |
| Host | Linux x86-64, GCC 14, glibc 2.41 |

The task-local directories, downloaded archives, `node_modules`, setup log and
Cargo cache are ignored and **will not exist in a fresh clone**. They must not be
added to Git. The setup log in this workspace is
`.cache/tmp/toolchain-setup.log`; it includes an npm warning about inherited
`http-proxy` configuration, but setup exited successfully.

## Commands in this workspace

Use the existing project entry point:

```sh
scripts/with-local-tools cli/bin/run doctor
scripts/with-local-tools cli/bin/run check
scripts/with-local-tools cli/bin/run build
scripts/with-local-tools cli/bin/run test adapter
```

For targeted Rust checks while iterating:

```sh
scripts/with-local-tools cargo fmt --all -- --check
scripts/with-local-tools cargo test --workspace --locked <test-filter>
```

The compiler oracle is available at `.tools/dotnet/dotnet`; prefer the wrapper so
`DOTNET_ROOT`, telemetry settings and the workspace temporary directory agree.
All generated parser C bytes are restored by normal development CLI commands.
Before bypassing that entry point for direct Cargo/source-capture commands, run
`node scripts/restore-parser-sources.mjs`.

Coordinate full builds and especially timed measurements with the other agents.
Do not interpret concurrent-build timings as controlled performance evidence.

## Reproducing in a new environment

Prefer the committed Nix workflow when Nix is available:

```sh
cli/bin/run setup
cli/bin/run doctor
```

The full Nix extension/editor workflow remains unverified. Its SDK was previously
built, but the current native project-environment certificate still rejects the
Nix loader/locale profile; installing Nix does not resolve that semantic gate.

For a managed Linux x86-64 workspace without Nix, the allowed task-local route
is reproducible without global installation:

1. Use an existing Node 24 runtime, GCC, `ar`, `make`, Python 3, xz/gzip and curl.
2. Download `https://static.rust-lang.org/dist/channel-rust-1.100.0.toml` and its
   `.sha256` file over HTTPS. Check the manifest digest before parsing it.
3. Download the following official component archives, check SHA-256, extract
   safely, then run each official `install.sh` with
   `--prefix="$PWD/.tools/rust" --disable-ldconfig`.
4. Download the Linux static Safe Chain asset specified in `nix/toolchain.nix`;
   verify its pin before making it executable at `.tools/safe-chain`.
5. Download the `sdk_10_0_4xx.srcs.linux-x64` URL in
   `nix/dotnet-10.0.400.nix`, verify that entry's SHA-512, and extract safely into
   `.tools/dotnet`. Use that SDK alone, not a silently substituted newer SDK.
6. Recreate the protected wrappers in `.tools/bin` from the policy in
   `nix/toolchain.nix`. Do not merely add raw npm to PATH: acquisition must invoke
   `.tools/safe-chain npm`, with `SAFE_CHAIN_MINIMUM_PACKAGE_AGE_HOURS=48` and
   lifecycle scripts disabled. `npm run` may delegate to the actual npm while
   preserving the protected PATH. Block `npm exec`/`npm x`, alternate package
   managers, and Safe Chain setup/teardown commands. Point the `node` wrapper at
   the actual Node 24 executable. `scripts/with-local-tools` supplies the
   project-local PATH, DOTNET_ROOT and temporary-directory settings.
7. Run `scripts/with-local-tools cli/bin/run setup` and retain its exit status
   and version evidence. Do not mark builds/tests passed merely because setup
   succeeded.

All four Rust archive URLs use the prefix
`https://static.rust-lang.org/dist/2026-11-12/`:

| Archive basename | SHA-256 |
| --- | --- |
| `rustc-1.100.0-x86_64-unknown-linux-gnu.tar.xz` | `f1d0b1dec81dab6f523cd098507763487b0d4bd7fd2c87582f154128719be03b` |
| `cargo-1.100.0-x86_64-unknown-linux-gnu.tar.xz` | `7e29ce07d7c6e92d1826870575ba27d57fff6cb3e754aa57d99099cb88fab35d` |
| `rust-std-1.100.0-x86_64-unknown-linux-gnu.tar.xz` | `4591b6486594ded52ecd6135db95bf9355bbd3ca7a380888b141528bbcf9a5d4` |
| `rustfmt-1.100.0-x86_64-unknown-linux-gnu.tar.xz` | `4c9389e4722b16499e5d47d89837ec84a4ef755f105ed2a84c038156064e01ab` |

Safe Chain Linux x64 SHA-256:
`0a7287bbf5562610e1c4881b928037378c97b4ba7578694287b2ad3b7140b033`.
.NET archive source and digest are already committed in the Nix pin, so use that
single source of truth rather than duplicating or loosening its version here.

The temporary restoration script and Rust component manifest in this workspace
are `.tools/downloads/restore-toolchain.py` and
`.tools/downloads/rust-components.json`; they are convenience evidence, not
required committed build inputs. No Safe Chain, certificate, compiler or runtime
validation rule was weakened to recover this environment.
