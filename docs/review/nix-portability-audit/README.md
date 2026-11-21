# Nix portability audit: coherent toolchain first, separate loader profile

Status: coherent development pin implemented and its real Nix SDK package executed, 2026-11-21. **The supported rootless canonical-store route now passes actual Nix evaluation, SDK realization, and a tiny F# project restore/build. Reusable Nix preparation remains unsupported:** unchanged helper09f correctly rejects the unreviewed glibc profile. See `rootless/README.md` for commands and layer-by-layer evidence. The earlier direct outside-namespace ELF failure is preserved and remains accurate. The accepted ordinary SDK10.0.400/runtime10.0.11/glibc2.41 profile is unchanged; SDK10.0.100 and SDK11 remain disabled.

Implemented files: `devenv.nix` now selects the unmodified `nix/dotnet-10.0.400.nix` historical manifest through the pinned current binary builder; `nix/dotnet-10.0.400.json` records provenance and upstream MIT license. Both the package list and helper-launch environment use that same derivation. `nix/README.md` records the unsupported Nix preparation boundary. CLI doctor prints selected SDK/runtimes and the Nix limitation; prepare emits an early diagnostic while retaining the helper's actual certificate gate. There is no new acceptance profile.

`pin-validation.json` records passing JavaScript syntax checks and the actual task-local doctor: Node24.22.3, npm11.19.1, Cargo/rustc1.100.0, Safe Chain1.5.29, SDK10.0.400, .NET/ASP.NET10.0.11. A separate run **simulated only FN_DEVENV=1 using the same task-local tools** to exercise the message branch; it is not Nix execution. The early prepare diagnostic was checked with a deliberately absent helper, so no evaluation or editor settings occurred.

## Decision

Use an explicitly pinned, coherent SDK10.0.400/runtime10.0.11 development package as the smaller first slice. Keep it separate from support for the user's project toolchain. Follow that with an independently reviewed **Nix-specific** loader/locale profile. Merely selecting SDK400 does not make current Nix preparation reusable. Do not advertise that it does.

The alternative, accepting SDK10.0.403/runtime10.0.14, requires the same Nix loader/locale work **and** reviewing new MSBuild startup/import behavior and runtime-dependent metadata interpretation. The existing binary-facts consumer pins .NET10.0.11 and a specific FCS informational identity. Actual SDK403 FCS bytes have not been inspected here; their equality or difference must not be assumed. Runtime14 is already a definite policy mismatch. A version allowlist change alone is insufficient.

## Original configuration mismatch (before this pin)

The unchanged `devenv.yaml` pins nixpkgs `f4a03c008967841f2a0b3cb7c436fc6fc31671f6`. Its `dotnetCorePackages.sdk_10_0` selects SDK10.0.403 and runtime10.0.14. The same nixpkgs source selects glibc2.44 and default ICU78.3. `devenv.nix` includes `glibcLocales`; its setup hook exports `LOCALE_ARCHIVE=<store path>/lib/locale/locale-archive`.

Before the new explicit derivation selection, the project `global.json` asks for SDK10.0.400 with `latestFeature`, so installing only403 is allowed by SDK selection. It is not allowed by the current evaluated-model and metadata producer profiles. Native Rust-only operations do not start .NET, but the development build/explicit preparation workflow is not yet an end-to-end verified Nix workflow.

Evidence is retained in `pinned-source/` and the adjacent SDK100 audit's `pinned-source/nixpkgs-*.nix`, with immutable URLs and hashes in their manifests. Relevant primary locations:

- [Current SDK/runtime manifest](https://github.com/NixOS/nixpkgs/blob/f4a03c008967841f2a0b3cb7c436fc6fc31671f6/pkgs/development/compilers/dotnet/10.0/releases.nix): SDK alias and coherent runtime references.
- [glibc source/profile](https://github.com/NixOS/nixpkgs/blob/f4a03c008967841f2a0b3cb7c436fc6fc31671f6/pkgs/development/libraries/glibc/common.nix): version2.44 and applied patches.
- [Locale setup hook](https://github.com/NixOS/nixpkgs/blob/f4a03c008967841f2a0b3cb7c436fc6fc31671f6/pkgs/development/libraries/glibc/locales.nix): exports `LOCALE_ARCHIVE`.
- [ICU default](https://github.com/NixOS/nixpkgs/blob/f4a03c008967841f2a0b3cb7c436fc6fc31671f6/pkgs/top-level/all-packages.nix): `icu = icu78`; `icu/default.nix` specifies78.3.

## Exact pin proposal

Historical nixpkgs commit `4862a919017b040f59d1adb4c67840efee8092cf` contains a coherent SDK10.0.400/runtime10.0.11/ASP.NET10.0.11 binary manifest. Its unmodified contents are retained as `pinned-source/sdk400-releases.nix`.

`pkgs.dotnetCorePackages.buildDotnetSdk` is a public attribute at the currently committed nixpkgs pin. It accepts a release-manifest path and supplies the binary package constructors. The implemented configuration vendors that exact historical manifest with provenance and chooses:

```nix
supportedDotnet =
  (pkgs.dotnetCorePackages.buildDotnetSdk ./nix/dotnet-10.0.400.nix).sdk_10_0_4xx;
```

Use that one value in both `packages` and `FSHARP_NAVIGATOR_DOTNET`. Leave other nixpkgs packages, project global.json files, locale values, and user project SDK selection intact. Do not override the entire nixpkgs pin or substitute SDK400 when an opened project requires100/11. The launcher must report an unsupported actual selected SDK where appropriate.

`sdk400-manifest-verification.json` verifies all24 SDK400/runtime11/ASP11 archive URL+SHA512 pairs in this manifest against the retained official .NET10 release metadata. Every pair matches. That JSON is archive-manifest verification; subsequent actual derivation/build evidence is separate in `rootless/`. Current Nix packaging patches the native binaries for glibc2.44/ICU78.3; it does not reproduce this machine's accepted glibc2.41 environment.

## Why the current loader/locale profile rejects Nix

| Input | Pinned Nix behavior | Required treatment |
| --- | --- | --- |
| Actual host identity | The SDK wrapper is a symlink composition of `bin/*` and `share/dotnet`; the binary SDK has a symlink `bin/dotnet` to its actual host. | Keep launch path provenance separate from mapped host/SDK/runtime identity; witness raw ancestor and leaf links. Do not reject every symlink as an unsupported shell wrapper. |
| Native library lookup | `autoPatchelfHook` supplies store RPATHs; the SDK builder adds DT_NEEDED for ICU to coreclr and other native components. | Observe actual ELF paths and mappings, including transitive ICU data and libc. Existing absolute-path/$ORIGIN parsing is useful but needs an actual Nix replay. |
| Loader cache | The Nix patch changes LD_SO_CACHE to the glibc prefix plus `/etc/ld.so.cache`. Package installation removes a generated cache. It does **not** universally disable caches. | Bind the actual patched loader's cache path and missing/present state. Do not substitute host `/etc/ld.so.cache` as if Nix consumed it. |
| Locale archive | The patch tries `LOCALE_ARCHIVE_2_27`, then `LOCALE_ARCHIVE`, then `/run/current-system/sw/lib/locale/locale-archive`, `/usr/lib/locale/locale-archive`, and the compiled archive path. | Add the versioned selector and certify the selected archive plus preceding absence/failure domains, or prove the observed stable C/POSIX bootstrap branch does not load archives. Do not simply delete the existing override rejection. |
| Culture | MSBuild glob regex behavior is culture-sensitive; Nix packaging does not remove that dependency. | Preserve actual culture and all locale selector states; repeat genuine en-US/tr-TR stale-model rejection and fresh recovery. |
| SDK imports | The binary SDK builder injects an `extra.targets` import into Microsoft.NET.Sdk.targets, including Nix native-AOT linker arguments. | Include its bytes and effective import/membership provenance. SDK version equality does not imply identical imports. |
| Runtime producer | Native binary-facts policy accepts the reviewed .NET10.0.11/FCS profile. | SDK400/runtime11 avoids changing this interpretation contract; verify the Nix-packaged managed FCS identity/bytes rather than assuming them. |

The exact patches are retained as `glibc-dont-use-system-ld-so-cache.patch` and `glibc-nix-locale-archive.patch`. They are the evidence for the extra selector and cache-path distinction. The current host profile's hard-coded `/etc/ld.so.cache`, loader2.41, runtime11, and rejection of nonempty `LOCALE_ARCHIVE` correctly prevent a successful Nix-profile claim today.

## Smallest separately reviewable implementation sequence

1. Vendor and select the coherent400/11 manifest. Add a CLI doctor report for actual SDK, runtime, launch path and supported preparation profile; distinguish Rust build readiness from reusable preparation readiness. Do not mark the Nix configuration verified after source-only checks.
2. In an actual pinned Nix shell, capture host/SDK/runtime paths, file hashes, ELF dependencies/RPATHs, loader `--list-diagnostics`, actual mappings and initial/final libc/managed culture. Retain the derivation identities and closure references, not only human-readable versions. Check that injected SDK imports and locale archive selection match source predictions.
3. Add a new, narrow Nix profile identifier whose producer and consumer agree on the actual selector set, glibc/ICU/runtime versions and cache/locale witnesses. Preserve existing v4 host acceptance rules. Unknown wrapper mutations, arbitrary LD hooks, another glibc revision or an unreviewed project SDK remain unsupported. Do not allow a producer to claim support simply by naming the profile.
4. Run actual build positive controls, existing import/environment/membership regressions, both culture directions, absent-to-present `LOCALE_ARCHIVE_2_27` and cache tests, archive-byte and symlink-retarget mutations, SDK selection/global.json mutations, fresh-process cached-model rejection and fresh positive recovery. Copies used for mutations must preserve behavior; do not mutate shared store paths. Compare actual Compile order with the compiler project context and check linked sources.
5. Measure explicit preparation and first native query again, including all newly hashed closure bytes and helper descendants. Nix closure validation is real startup work, not a free portability flag.

This sequence does not certify arbitrary NixOS configurations, alternate libc implementations, ARM/macOS, arbitrary app-local ICU, or projects selecting different SDKs. A future broader profile needs its own evidence.

## Executed and unexecuted boundary

Executed here: immutable primary-source retrieval; exact manifest history inspection;24 official archive-hash comparisons; implementation review; CLI syntax/doctor/diagnostic checks; capability probe; checksum-verified official Nix archive retrieval and failed direct executable launch. The environment is Linuxx64, uid1000, has `bwrap` and working unprivileged user namespaces, but no `nix`, `nix-instantiate`, or `/nix/store`.

The initial probe did not execute Nix. The subsequent supported rootless route did execute Nix expression evaluation, real SDK package realization, a tiny project build and unchanged helper evaluation. Still unexecuted: full Nix devenv entry, this extension's complete Nix build/test/editor workflow, locale/archive/loader/import mutation controls and reusable Nix profile acceptance. The bounded probe downloaded the official Nix2.31.2 x86_64 archive (25,865,344 bytes, SHA256 `d1f67c86eed016214864ba08bfb9529c307aea7e8fafb74853f96fcc3bfd8a60`) and verified its published checksum. The supplied regular `bin/nix` executable was copied unchanged into scratch and launched with `--version`. The kernel returned ENOENT because its ELF interpreter is `/nix/store/g8zyryr9cr6540xsyg4avqkwgxpnwj2a-glibc-2.40-66/lib/ld-linux-x86-64.so.2`, which is absent. The supplied installer hardcodes `/nix` and would alter the global store/profile, so it was not invoked. That initial direct probe stopped there, with no host changes or namespace use. A subsequent source-reviewed, documented `nix-user-chroot` arrangement legitimately exposed the task-local store at canonical `/nix` inside user/mount namespaces. It required no host store, installer, security exception, binary bootstrap rewrite or sandbox relaxation. See the separately retained rootless reports; do not conflate the two attempts.

`nix-distribution-probe.json` and `nix-binary-probe.json` retain exact download/ELF/installer/error evidence; `environment-capability-probe.json` records the environment. There is no new reusable profile or controlled performance result. Nix success is claimed only for the executed layers in `rootless/README.md`, not the full development/editor workflow or semantic-profile acceptance.
