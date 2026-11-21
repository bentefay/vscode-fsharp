# SDK100/Fable audit and the accepted SDK400 culture regression

This audit keeps SDK10.0.100 and SDK11 disabled. It found a genuine dependency gap in the already accepted SDK10.0.400 profile; that repair now takes priority over enabling Fable's project profile. Original helper f215 and all captured certificates remain unchanged. The new v4 implementation is a separate candidate and must pass independent replay before acceptance.

## Reproduced culture dependency

The fixture includes `A.fs` and `Sources/**/I/*.fs`. The Latin-i directory contains a file with no reference to `A.value`; the dotless-i directory contains one reference. Changing only `LANG` and `LC_ALL` from en_US.UTF-8 to tr_TR.UTF-8 changes which file MSBuild includes. Project, global.json and source hashes remain identical, and both normal builds succeed.

| Profile and evidence | Initial model | Old model after culture change | Fresh model |
|---|---|---|---|
| SDK100 / runtime10.0.0, `culture-probe.json` | Correctly nonreusable | No native acceptance claimed | Different evaluated source selection; still nonreusable |
| SDK400 / runtime10.0.11, `culture-probe-sdk400.json` | Reusable, exact zero references | **Reusable, exact zero references: wrong** | Reusable, exact one reference |

The SDK400 witness used native `8a414b97e727a3a9355b989df564e88aaae3376dc9fd0c85d52b7ba1b3a340ad`. The independent scorer reproduced the same wrong-result triplet using frozen f27d and fresh per-arm caches in `../culture-sdk400-f27d.json`. No old certificate contained a locale selector. This is an accepted-profile correctness failure, not an unsupported-project or performance result.

Pinned primary source explains it: SDK100 VMR `b0f34d51fccc69fd334253924abd8d6853fad7aa` and SDK400 VMR `14fbf8d5271c98133561eb55185fdb05b286f578` both construct complex-glob regexes with `RegexOptions.IgnoreCase`, without `CultureInvariant`. SDK11's separately audited compatibility switch is not involved here. SDK400's actual runtime comes from VMR `e2f47b0110ed922f21a1522da67279133ce28f32`, as recorded in its installed runtime `.version` file.

## Closure required for the repair

The pinned .NET runtime obtains its default culture through ICU. ICU76.1's `putil.cpp` checks existing libc locale state, then LC_ALL, the relevant LC_MESSAGES/LC_CTYPE category, and LANG. The distinction between absent and empty environment values matters. The CLI's DOTNET_CLI_UI_LANGUAGE and VSLANG handling affects UI culture. .NET also reads invariant/predefined-culture/app-local-ICU settings; its Linux ICU loader supports DOTNET_ICU_VERSION_OVERRIDE and searches installed versioned sonames. ICU_DATA and loader hooks introduce additional file-selection paths.

The profile also observes libc's LC_MESSAGES state without modifying it and requires C/POSIX, the exact ICU branch that falls back to the witnessed environment names. Other libc bootstrap states and nonempty LOCPATH/LOCALE_ARCHIVE remain unsupported pending a locale-data dependency profile. The pinned host/PAL sources inspected here contain no locale-setting call; observing the actual branch avoids turning that limited source search into an unqualified claim about every Linux launcher. The runtime's `pinvokeoverride.cpp` statically dispatches globalization imports before ordinary dlopen; actual coreclr bytes, rather than an invented separately mapped globalization shim, carry that implementation.

Adding names to newly exported v3 certificates is insufficient: consumers must reject old v3 models. The v4 candidate records culture/sort/runtime observations inside the actual MSBuild processes, witnesses all mandatory startup selector states, and validates evaluated runtime-file hashes. POSIX getenv is case-sensitive even though MSBuild properties are case-insensitive; a canonical LANG variable must not be replaced by a lowercase `lang` variable without invalidation.

`semantic/GlobalizationProfile.fs` is the new bounded producer component. Its initial profile is Linux x64, SDK10.0.400, runtime10.0.11, glibc2.41, with a single installed SDK/runtime/fxr choice. It preserves the user's actual culture. It inventories dynamic-loader default/cache/ELF search locations and immediate entries, including directory and raw symlink states; hashes actual and candidate ICU bytes; captures ancestor symlink states; and rejects unreviewed loader/globalization overrides, distinct alternative ICU choices, and hardware-capability ICU alternatives. These are explicit initial-profile limits, not a claim of general .NET host interception. The component compiled successfully; end-to-end v4 regression results belong to the integration/scorer reports and are not asserted by this audit.

## Requested Nix workflow: still unverified

The repository uses `devenv.nix` and pinned Nixpkgs `f4a03c008967841f2a0b3cb7c436fc6fc31671f6`. These definitions have not been executed here. The current narrow certificate profile is not evidence of reusable project models under that workflow.

Pinned Nixpkgs source makes an important distinction: its `wrapper.nix` links the unwrapped dotnet bin/share entries; the name “wrapper” does not imply an environment-mutating shell program. A selected executable that is a symlink to the same actual host can satisfy host byte-identity and link-state checks. The setup hook establishes documented shell settings before launch. An arbitrary executable wrapper that changes its child's environment remains outside the contract.

Nix's binary dotnet derivation patches ICU DT_NEEDED/RPATH dependencies. Absolute ELF search paths into `/nix/store`, and literal `$ORIGIN` paths, are representable by the new search-domain certificate; no path is rejected merely for being in the store. However, nonempty LD_LIBRARY_PATH/preload/audit/app-local-ICU/version-override settings currently fail closed, as do untested loader/runtime versions, missing or unsupported host loader cache formats, multiple installed SDK/runtime choices, and non-Linux-x64 platforms.

There is also a concrete version mismatch to resolve: the pinned `10.0/releases.nix` selects SDK10.0.403 as `sdk_10_0` and runtime10.0.14; its source SDK manifest is10.0.114/runtime10.0.14, with source/default.nix combining the selected SDK and matching base runtime. The current producer profile covers SDK10.0.400/runtime10.0.11. This source inspection is not an executed Nix derivation or authorization to substitute the user's SDK. A Nix profile needs exact built-host/runtime observations, certified store/search paths, and the same stale-to-fresh regression controls. Exact-glibc/single-install restrictions are remaining product limitations, not completion of the requested Nix support.

## Fable remains a separate support task

The unchanged pinned `src/fcs-fable/fcs-fable.fsproj` has 287 evaluated source inputs and a genuine successful build. See `evaluation/results/fable-corpus-feasibility.md` for its independently verified twelve-operation FSAC workload. The current producer rejects SDK100 and several membership rules; no native performance ratio is established.

The 273 property-expanded Compile paths use two unconditional, literal-plus-directory-anchor definitions: FSharpSourcesRoot and FsYaccOutputFolder. A narrow safe extension can prove a unique top-level unconditional definition across the complete import graph, exclude globals and conditioned/Choose/Target assignments, and permit only literal path text plus reserved project/defining-file anchors. The pinned Evaluator's property/import pass precedes static item evaluation. Expansion must still capture wildcard domains and preserve negative import/environment witnesses; arbitrary evaluated final strings are not proof of their derivation. Four additional imports need deterministic MSBuildThisFileName expansion. The independent scorer requested tests for textual item/property order, imported anchors, duplicate casing, globals/TreatAsLocalProperty, and newly appearing expanded-glob items.

`fable-target-activity.json` records one successful genuine design-time Compile, with only task/target names and assembly bindings retained. Raw diagnostic output was discarded because it can include unrelated ambient values. Git LocateRepository, SourceLinkHasSingleProvider, and TranslateRepositoryUrls actually execute. The analyzer Exec does not execute in this observed path; its run-settings target does. Therefore a package-name allowlist or blanket “unreachable SourceLink tasks” exemption is unjustified. Git configuration, repository state, generated assembly-info and target output dependencies need their own source-bound contract. These task boundaries are independent of the SDK culture/profile repair and property expansion.

## Evidence

`source-manifest.json` records immutable primary URLs, local source hashes, and failed fetches explicitly. `culture-probe*.py/json/log` preserve commands, successful-build outcomes, source/model/helper hashes and native transcripts. The fixture source/model files remain alongside them for replay. `fable-target-activity.json` is a sanitized activity observation, not a timing benchmark. This audit does not enable SDK100 or SDK11 and does not modify the Fable repository's tracked sources or configuration.
