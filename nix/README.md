# Pinned development tools

`devenv.nix` uses the unmodified historical release manifest
`dotnet-10.0.400.nix` to select SDK **10.0.400**, .NET runtime **10.0.11** and
ASP.NET runtime **10.0.11** together. Its immutable source, SHA256 and selected
attribute are in `dotnet-10.0.400.json`. All 24 SDK/runtime archive hashes in that
manifest were checked against official .NET release metadata.

The builder and other packages still use the nixpkgs revision in `devenv.yaml`.
This avoids the default alias selecting SDK10.0.403/runtime10.0.14, which is
outside the current helper contracts. It does not change an opened project's
`global.json`, force its SDK, or force a culture. Projects requiring another SDK
still need that SDK and a reviewed preparation profile.

**Reusable project preparation under Nix remains unsupported.** The pinned
nixpkgs uses glibc2.44/ICU78.3, patched loader-cache selection and locale archive
overrides. The accepted preparation profile was tested with glibc2.41 and the
ordinary .NET10.0.11 host. Pinning managed tool versions does not make those
environments equivalent. The CLI reports this boundary, and the helper's actual
input checks remain authoritative; an unsupported model generates no editor
settings.

A bounded rootless Nix proof has now evaluated the real pinned derivation,
realized this SDK with the normal Nix sandbox, and restored/built a tiny F#
project under it. The unchanged helper correctly rejected its unreviewed glibc
profile. The full devenv shell, this extension's complete Nix build/test/editor
workflow, and Nix preparation mutation controls remain unverified. See
`docs/review/nix-portability-audit/rootless/README.md` for exact commands and
executed layers. Ordinary task-local tool checks remain separate evidence.

Use `cli/bin/run doctor` to display the selected SDK and installed runtimes. A
successful version check or SDK package build is not a preparation certificate.

See `docs/review/nix-portability-audit/README.md` for the source evidence, exact
executed/unexecuted boundary and required loader/locale mutation controls. The
current profile's locale guards are unchanged.
