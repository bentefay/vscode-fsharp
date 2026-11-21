# Proposed narrow Nix preparation profile — not implemented or accepted

The coherent SDK pin now has real Nix execution evidence. A new reusable preparation profile still needs independent review. This proposal changes neither the ordinary v4 acceptance rules nor SDK100/SDK11 support. The completed pin proof remains in `../nix-portability-audit/rootless/`.

## What is established, and what is not

The exact nixpkgs derivation realizes SDK10.0.400/runtime10.0.11 against glibc2.44 and ICU78.3. Its managed FCS and FSharp.Core bytes equal the previously reviewed SDK400 files. Native loader/coreclr/ICU dependencies and symlinks differ. The unchanged producer completes evaluation but returns `membershipReusable=false`, explicitly rejecting the glibc loader version. A successful Nix build therefore has not accidentally enabled reuse.

The new control fixture was genuinely restored and built under that Nix SDK. With all source/project hashes unchanged:

| Controlled inputs | Evaluated Compile sequence | Actual managed culture / libc LC_MESSAGES |
| --- | --- | --- |
| LANG and LC_ALL = en_US.UTF-8 | A.fs; Sources/i/source.fs | en-US / C |
| LANG and LC_ALL = tr_TR.UTF-8 | A.fs; Sources/ı/source.fs | tr-TR / C |
| Turkish inputs plus both archive selectors pointing at absent task-local files | A.fs; Sources/ı/source.fs | tr-TR / C |

All three builds passed, and the existing logger reported stable observations with no diagnostic errors. `culture-selector-controls.json`, the three logger JSON files and `observations-summary.json` retain the evidence. These are behavior controls, not an accepted Nix certificate or proof that every archive path was unread.

A normal strace attempt against the owned child was denied by the environment's ptrace policy. It was stopped, with the error retained in `baseline-probe.json`. No alternative tracing mechanism, security exception or permission broadening was attempted. File-access tracing is consequently unavailable here.

Actual loader metadata agrees with the Nix patch: the glibc prefix is `/nix/store/r3hvhzl5fxhrgn0rfy5g7rda17702pj4-glibc-2.44-57`; its `etc` directory exists and its `etc/ld.so.cache` is absent. The compiled locale archive is absent too. See `loader-selection-probe.json`. Treating host `/etc/ld.so.cache` as Nix's cache would be wrong.

## Narrow support boundary

Start with one separately named profile for the exact Linux x64 SDK400/runtime11/native packaging combination already realized. Do not expand the old generic loader-version condition to include2.44, and do not identify the profile merely by a `/nix/store` pathname or SDK version string. Pin the reviewed loader/libc and critical runtime implementation identities, source/derivation provenance and observed binary bytes. Nix content-address conventions do not remove the need to check actual bytes: a user-owned store or symlink can be modified.

Retain the current restrictions on arbitrary LD hooks, app-local ICU, ICU overrides and custom SDK resolvers. Require one selected SDK/runtime context and stable actual observations. Preserve the user's effective culture; the profile must explain its inputs rather than force invariant or another language. SDK100/SDK11 and other Nix glibc/compiler revisions remain unsupported.

The actual mapped host, observed SDK/runtime directories and launch path are different roles. The default Nix wrapper is a symlink composition, not an environment-mutating executable wrapper. Certify each raw ancestor/leaf link and the mapped native host bytes; do not require path-string equality between aliases. Existing raw-link witnesses and selected/actual host byte checks can cover most of this without changing the ownership model. A retarget to another package must invalidate even if the old package is still present.

## Minimal proposed producer and consumer changes

| Component | Proposed change | Required rejection |
| --- | --- | --- |
| GlobalizationProfile.fs | Dispatch to a separate pinned Nix profile; retain actual observed culture/runtime/loaded-file hashes and before/after validation. | Unknown native identity, loader policy, runtime choice or unstable observations. |
| Nix cache/search selection | Derive the reviewed loader prefix/cache location from the observed loader and its pinned implementation; use actual absolute ELF search paths and default directories. Initially support only the observed absent prefix cache. | A cache appears, an unreviewed search path/token/ICU alternative appears, or directory/link state changes. |
| Locale selector policy | Add LOCALE_ARCHIVE_2_27 to mandatory inputs for the Nix profile. Initially reject its nonempty override. Support ordinary Nix LOCALE_ARCHIVE only after the branch proof below is reviewed. | Missing selector witness, case alias, unsupported libc bootstrap state, unproven archive consumption. |
| Native globalization/environment validation | Recognize the new profile explicitly, enforce its required selectors/identities and new absence witness, validate through the existing shared cancellation-aware digest generation. | Old/unknown/incomplete Nix profile data, stale files/directories/links, or outside-namespace paths that do not exist. |
| Project/import evaluation | Keep existing evaluated import/property/negative-domain tracking; inspect Nix-injected targets structurally and include their bytes. | New competing import, property/global context or source membership not proven by existing logic. |

A small new `absentFiles` witness is preferable to inventing an empty SHA for a nonexistent cache. It should include the exact prefix cache path, require absence on first use, participate in warm identities and capture parent membership plus raw ancestor links. The parent `etc` directory is present here, so a cache creation changes its membership. The first Nix profile can reject all nonempty loader caches instead of implementing another cache parser immediately. The producer must take and verify this selection context around evaluation; the consumer must not treat an after-the-fact label as proof.

The ordinary profile can remain v4 with its existing identifier. A new Nix profile identifier and profile-specific required fields make old consumers reject it and new consumers reject incomplete records. If implementation introduces a shared semantic change affecting ordinary v4 interpretation, increment the outer contract and revoke old acceptance instead. A blanket global allowlist change is not an acceptable substitute.

The actual external import is `/nix/store/y7pdmpixpwqrj2yivgqwxlppslxmgmvr-extra.targets`. It is outside the SDK's directory because the binary builder injects it. Analyze the actual XML and reachable item/property effects, not the filename, package name or store provenance. Do not mark all store targets trusted. Existing tracking should capture its positive import and any conditional negative domains; any remaining membership rejection must be resolved by a specific supported construct.

## Locale archive proof is the main open design decision

The Nix libc patch tries LOCALE_ARCHIVE_2_27, then LOCALE_ARCHIVE, then its configured fallback archives when loading a locale. The default devenv glibcLocales hook supplies LOCALE_ARCHIVE, so permanently rejecting every nonempty archive setting would leave the requested shell unsupported.

The smallest plausible profile uses the already observed stable C/POSIX libc bootstrap branch. The pinned ICU default-locale routine queries LC_MESSAGES; on C/POSIX it follows environment language selection without setting a libc locale. The observed controls are consistent with that path: actual MSBuild culture changes and Compile membership changes, while LC_MESSAGES remains C even with both archive selectors set. **This evidence does not yet prove complete non-consumption.** Before allowing LOCALE_ARCHIVE without hashing its payload, review the exact Nix libc/bootstrap, CoreCLR static globalization shim and ICU78.3 startup path and independently check that every supported pre-evaluation path stays on that branch. Do not transfer the ordinary ICU76 source audit to78.3 by version similarity.

If this proof holds, certify the selector's presence/value and the branch provenance, and deliberately exclude an archive payload that is not a semantic input. This does not allow arbitrary MSBuild file reads: project conditions/imports that consume the path remain subject to the existing source-membership/import proof. Non-C/POSIX bootstrap remains unsupported until it has a real archive-selection certificate.

If the proof cannot be closed, either hash the selected archive and all prior absence/selection domains and measure the cost, or keep the profile unsupported. Do not omit a consumed archive to improve a benchmark. The current three controls do not replace that proof; present-to-different-valid-archive and versioned-override controls are still required.

## First-query cost boundary

The existing logger observed15 runtime/host/ICU/SDK configuration files totaling **68,489,807 bytes** in this Nix context. ICU data accounts for33,113,032 bytes and CoreLib15,576,872 bytes. This is a lower bound for a full future certificate: remaining SDK/import/reference/source files, selection directories, links and any consumed locale archive are excluded. It is not an RSS figure or a measured native query time.

Preserve zero compiler/helper startup for native queries. Use the same per-validation-generation digest sharing and bounded identity caches as the ordinary profile; do not hash an entire Nix closure or enumerate `/nix/store`. Witness only actual selected implementation files and the specific search/selection directories. Avoid watching the whole store, since installing an unrelated package must not invalidate every project.

Before acceptance, run a quiet comparison on the same query/artifacts with fresh and reopened native processes. Report unique bytes actually hashed, requested bytes, cache hits, first/warm query time, RSS and selection-directory entry counts. The previous ordinary-profile comparison reported about75.5MB actually hashed; that is a useful accounting reference, not a Nix performance promise. Include archive bytes if the final proof requires them. A large archive-dependent first query is a real product gap, even when Nix setup and offline preparation are reported separately.

## Acceptance gates, still outstanding

1. Independently review exact native/ICU78.3 bootstrap sources and approved identities; prove the supported locale branch or account for every consumed archive.
2. Obtain an ordinary positive Nix certificate without forcing culture or removing the normal glibcLocales environment. Match its complete ordered Compile set and flags to the actual compiler context, including linked sources.
3. Run both culture directions through actual builds: old cached model must reject after the change; fresh model must produce the correct membership. Repeat with a fresh native process, not only an existing watcher.
4. Add absent-to-present and changed-value LOCALE_ARCHIVE_2_27 cases, LOCALE_ARCHIVE selection/data changes as appropriate, prefix cache creation, ICU candidate/hwcaps additions and symlink retargeting. Use owned copies; never mutate shared store paths.
5. Preserve existing environment/import/negative-wildcard/global.json/membership mutation controls. New SDK/runtime installation or a closer SDK selection file must invalidate. Verify injected targets cannot change source membership unnoticed.
6. Validate malformed/incomplete/old profile rejection independently, then measure actual first-query cost under quiet conditions. Do not enable the profile before those gates pass.

No shared producer, consumer or profile acceptance files were edited by this design task. The supported ordinary profile and the new Nix rejection remain intact.
