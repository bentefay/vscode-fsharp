# Pinned development tools

`devenv.nix` uses the unmodified release manifest `dotnet-10.0.404.nix` to
select SDK **10.0.404**, .NET runtime **10.0.15** and ASP.NET runtime **10.0.15**.
Its immutable source, SHA256 and selected attribute are in
`dotnet-10.0.404.json`. All 32 archive hashes in the manifest were checked against
official .NET release metadata on 2026-11-25. This replaces the vulnerable
10.0.11 development runtime. The older manifest remains historical audit evidence
only and is not selected by devenv.

The builder and other packages still use the nixpkgs revision in `devenv.yaml`.
The new SDK/runtime is outside the existing semantic certificate contracts.
It does not change an opened project's
`global.json`, force its SDK, or force a culture. Projects requiring another SDK
still need that SDK and a reviewed preparation profile.

**Reusable project preparation on this patched toolchain remains unsupported.** The pinned
nixpkgs uses glibc2.44/ICU78.3, patched loader-cache selection and locale archive
overrides. The accepted preparation profile was tested with glibc2.41 and the
ordinary .NET10.0.11 host. Pinning managed tool versions does not make those
environments equivalent. The CLI reports this boundary, and the helper's actual
input checks remain authoritative; an unsupported model generates no editor
settings.

A bounded rootless Nix proof has now evaluated the real pinned derivation,
realized this SDK with the normal Nix sandbox, and restored/built a tiny F#
project under it. The unchanged helper correctly rejected its unreviewed glibc
profile. The 2026-11-25 security follow-up additionally exercised the full devenv shell,
native build/check and sandboxed packaged editor on Ubuntu 26.04.1 x64 with
Code 1.147.0. Nix preparation mutation controls and a new semantic certificate
profile remain unverified. See
`docs/review/nix-portability-audit/rootless/README.md` for exact commands and
executed layers. Ordinary task-local tool checks remain separate evidence.

Use `cli/bin/run doctor` to display the selected SDK and installed runtimes. A
successful version check or SDK package build is not a preparation certificate.

See `docs/review/nix-portability-audit/README.md` for the source evidence, exact
executed/unexecuted boundary and required loader/locale mutation controls. The
current profile's locale guards are unchanged.

The runtime upgrade follows Microsoft's advisories, including
[CVE-2026-109475](https://github.com/advisories/GHSA-3g4v-f5gv-c7xf), whose .NET 10
fix is 10.0.15. Historical manifests and exact semantic profiles are retained
for provenance, not as a recommendation to run the affected runtime.
