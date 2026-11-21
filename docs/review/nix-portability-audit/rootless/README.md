# Actual rootless Nix proof, with preparation still unsupported

This follow-up supersedes the earlier claim that Nix had not executed. It does not change the earlier direct-launch result: the ordinary outside-namespace executable still requires an absent `/nix/store` interpreter. The documented canonical-store arrangement below supplies that path only inside an unprivileged process environment.

## Supported route and pins

The [Nix2.31.2 source manual](https://github.com/NixOS/nix/blob/fdea16241777b4a827d99436608b961960198785/src/libstore/local-store.md) documents local chroot stores: physical files may live under a user directory while logical paths remain `/nix/store`. Linux user and mount namespaces enable execution. This avoids changing logical store paths, which would invalidate ordinary substituter compatibility.

For bootstrap when no Nix executable can yet run, the [official NixOS installation guide, revision31888](https://wiki.nixos.org/w/index.php?title=Nix_Installation_Guide&oldid=31888) recommends `nix-user-chroot`. We used release1.2.2, source commit `d2de5e4eee0c0f99d00dfa7734bed5f58f7f4172`. Its retained source uses only user and mount namespaces, mirrors the existing filesystem, substitutes the supplied local directory at `/nix`, and preserves the invoking UID/GID mapping. It does not change network namespaces or install syscall-filter exceptions.

- Nix2.31.2 official archive: SHA256 `d1f67c86eed016214864ba08bfb9529c307aea7e8fafb74853f96fcc3bfd8a60`, verified against the publisher's checksum.
- nix-user-chroot1.2.2 x86_64 musl release asset: SHA256 `e11aff604bb8d3ffd1d9c0c68cd636816d7eb8da540de18ee3a41ccad7ac0972`. This digest was computed from the pinned maintainer HTTPS release; the old release supplied no separate published digest. Do not describe it as an independent checksum comparison.
- nixpkgs: committed revision `f4a03c008967841f2a0b3cb7c436fc6fc31671f6`; the exact lock NAR hash used is recorded below and in `pinned-derivation-probe.json`.

The exact lock NAR hash used was `sha256-ziZn2PvCJE5JEEpqTdxXwJpPB+L3YqoCpTAMCzyaP3U=`. All source revisions, release URLs and retained-file hashes are in `source-manifest.json`.

## Actual completed layers

| Layer | Actual result | Evidence |
| --- | --- | --- |
| Canonical rootless Nix executable | `nix (Nix) 2.31.2`, exit0 | `version-probe.json` |
| Official closure registration and expression evaluation | Passed | `evaluation-probe.json` |
| Committed devenv syntax and vendored manifest | Parsed; manifest400/11/11 | `evaluation-probe.json` |
| Actual pinned nixpkgs SDK derivation | SDK400/runtime11/ASP11, glibc2.44, ICU78.3 | `pinned-derivation-probe.json` |
| Standard Nix sandbox | Small build passed with `sandbox=true`, existing Bash closure and no downloads | `sandbox-build-probe.json` |
| Coherent SDK package realization | Passed; upstream package installCheck ran dotnet --info | `sdk-realization-probe.json` |
| Standalone selected SDK and tiny F# project | --info, restore and build all passed | `sdk-helper-probe.json` |
| Existing helper under the actual Nix SDK | Evaluation completed, but `membershipReusable=false`; explicit unsupported glibc loader diagnostic | `sdk-helper-probe.json` |
| Managed compiler identity | FCS and FSharp.Core bytes equal reviewed ordinary SDK400 | `managed-compiler-comparison.json` |

The dry run estimated128 fetched paths,459.26MiB download and942.84MiB unpacked, plus4 local derivations. Actual realization completed in46.74s with maximum1 job and2 cores. This is environment setup evidence from a shared machine, **not** a controlled extension preparation or first-query performance measurement.

The real wrapper output is `/nix/store/pz50fdpc8bxlmhixhf7dpq3hz4xychlr-dotnet-sdk-wrapped-10.0.400`; the underlying SDK is `/nix/store/n0hzhs06j0s3hbp5fz7vhiy3dwg8fpxi-dotnet-sdk-10.0.400`. Actual raw symlinks, native ELF interpreter, ICU DT_NEEDED/RUNPATH, hashes and ambient selector states are retained in `native-path-evidence.json`. These observations confirm the source audit's native-package differences. They are not a complete loaded-file/locale certificate.

## Reproduction arrangement

All paths below are relative to the project root unless absolute. The physical store is `.tools/nix-probe/rootless/nix/store`. It is mounted at logical `/nix/store` only for children of the pinned helper. The host still has no `/nix/store`.

1. Retrieve the exact Nix archive and chroot-helper assets listed in `source-manifest.json`/the parent `nix-distribution-probe.json`; verify the recorded digests. Extract the reviewed archive into a new task-local distribution directory, preserving its canonical store symlinks. No archive member may escape the distribution directory or write through a symlink ancestor. Move its `store` directory to `.tools/nix-probe/rootless/nix/store`. Retain the archive `.reginfo` beside the extracted installer. Do not run the installer.
2. Make task-local `nix/var/nix`, `nix/etc/nix`, `tmp`, `cache`, `config` and `data` directories. Use the helper invocation shown in each JSON report: `nix-user-chroot-1.2.2 <physical-nix-directory> /nix/store/i91hkfq9raq9qznwjrd59mlf1zb7nlpq-nix-2.31.2/bin/<command> ...`.
3. Set `TMPDIR`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, and `XDG_DATA_HOME` to those task-local directories; set `NIX_REMOTE=local`, `NIX_USER_CONF_FILES` empty, and `NIX_CONFIG` to `experimental-features = nix-command flakes` plus an empty `build-users-group`. This is the normal single-user store configuration. The helper supplies `NIX_CONF_DIR=/nix/etc/nix`. Keep `sandbox=true` for builds. Inherited proxy/security/culture settings are not altered.
4. Register the verified official closure with `nix-store --load-db`, passing the archive `.reginfo` through stdin. Exact pure-evaluation, parse, real nixpkgs import/NAR verification, SDK instantiation and realization arguments are in the JSON reports. The realization uses `--max-jobs 1 --cores 2 --option sandbox true`, with a 180-second process-group deadline.
5. `python3 docs/review/nix-portability-audit/rootless/run-sdk-probes.py` repeats the selected SDK/tiny project/helper checks once that store is ready. It creates its own copied fixture, uses explicit SDK/host paths, disables MSBuild node reuse/shared compilation, applies owned process-group deadlines and records all intentional environment changes. It does not change the user's project or current helper/profile policy.

No host installer, profile edits, system service, setuid helper, sysctl, capability broadening, security-policy exception, PRoot, binary-interpreter rewrite, or sandbox disable was used. Nix's own normal sandboxing was retained. Upstream Nix SDK packaging legitimately performs its documented autoPatchelf step; that is the derivation under test, not an ad hoc bootstrap workaround.

## Still unexecuted / unsupported

The complete devenv shell, its `glibcLocales` setup hook, the project's full Nix build/test/VS Code workflow and Nix culture/archive/import/symlink mutation controls have not been executed. This probe inherited ambient culture; it did not claim the full shell's LOCALE_ARCHIVE environment. The helper correctly refuses to certify this unreviewed Nix glibc/ICU profile. No native exactness, acceptance-policy widening, or portability-wide performance result follows from the successful SDK build.

Rootless store paths are visible only inside the mapped environment. A native consumer outside it cannot consume `/nix/store`-referencing facts as if those paths existed; the ordinary identity guards must reject that mismatch. This does not affect normal Nix hosts with their real canonical store, but is an important limit of this test arrangement.
