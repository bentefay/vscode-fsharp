# Pinned SDK 11 environment/profile audit

**Decision: keep the SDK 11 reusable project profile disabled.** The existing v3 producer works with the new event API, and its 15 existing controls pass, but an SDK 11 early environment switch escapes the certificate and produces a reproducible false exact reference answer. No shared implementation was edited or enabled during this audit.

## Pins and experiment boundary

| Component | Actual pin |
| --- | --- |
| SDK 11 | `11.0.100-rc.3.26522.148`, Linux x64 |
| MSBuild 11 installation | `18.12.0-1.26522.148` |
| SDK 11 source VMR | `5254d8bcf59917edc1e1abc33bdf2442b77b1c70` |
| Previous SDK | `10.0.400`, MSBuild `18.9.6` |
| Previous source VMR | `14fbf8d5271c98133561eb55185fdb05b286f578` |
| Producer contract | `dotnet-msbuild-observed-v3` |

`sdk-binary-pins.json` contains hashes of actual SDK files. `summary.json` pins the diagnostic helper and native consumer. `source-manifest.json` links each fetched primary source file to its immutable VMR commit and records its hash. The source copies retain their MIT headers.

The diagnostic producer is a private copy of the current implementation with **only its SDK version check changed** to the exact SDK 11 pin. The archive `diagnostic-producer-source.tar.gz` preserves this experiment. Its successful outputs are not an enabled production profile. It was built with SDK 10 and run with the exact SDK 11 host using explicit `DOTNET_ROLL_FORWARD=Major`; this setting is included in the generated environment certificates. Ordinary fixtures target net11.0 and pin the SDK in their own global.json.

## What actually passed

All 12 existing environment controls passed with real SDK 11 evaluation and native restart/replay: positive and negative ambient properties, conditional defaults, read-before-overwrite, unconditional overwrites, explicit global Configuration, irrelevant shell variables, an absent execution-only OtherFlags value, an early MSBuildSDKsPath selector, two approved pure property functions, and rejection of arbitrary direct environment APIs. The first positive fixture recorded **84 environment reads, 2,466 missing-property reads, and two trace invocations**. These are actual emitted events, not inferred API compatibility.

All three import controls passed: a new obj wildcard target invalidated its old model while an unrelated uncompiled source did not; a previously empty extensionless ImportAfter hook invalidated its old model; and an extensionless imported custom task remained unsupported. Raw case records and logs are included.

Pinned source comparison explains the narrow compatibility result:

- `PropertyTrackingEvaluatorDataWrapper` has the same environment/uninitialized-read handling and the same tracking bits. Its one change removes an obsolete change-wave condition from default reassignment logging. The producer explicitly uses tracking setting 15, so that default-only difference does not affect these tests.
- `ProjectPropertyInstance.cs` and `NETCoreSdkResolver.cs` are byte-identical between these source pins.
- SDK 11 retains the evaluator wrapper but adds parser configuration, evaluation stages/instrumentation and an optional evaluated glob-reporting mechanism. These additions require their own dependency review; successful old tests do not cover them.

## Confirmed false exact result

`cultural-membership-witness-v2.json` is the complete witness. Both normal F# builds succeed. Project, global.json and source bytes remain unchanged. The culture is fixed to tr-TR throughout; only `MSBUILDUSELEGACYCULTURESENSITIVEFILEGLOBS` changes from absent to `1`.

The project includes `A.fs` and `Sources/**/I/*.fs`. With SDK 11's default invariant glob behavior, the second file is `Sources/i/source.fs`, which does not reference A.value. With the compatibility switch, it is `Sources/ı/source.fs`, which does.

| Query state | Actual result |
| --- | --- |
| Initial model, initial environment | Exact, zero references |
| Old model after compatibility switch | **Exact, zero references: incorrect** |
| Fresh model after compatibility switch | Exact, one reference in the dotless-i source |

The old model declares `membershipReusable=true`, but its certificate does not contain the changed setting. The native consumer therefore cannot reject the changed evaluation context. This is independent of custom targets, generators, scripts, SDK restoration or missing assemblies.

The exact pinned `FileMatcherCulture_Tests.cs` describes the same complex-glob behavior. `FileMatcher.cs` lines 2983–2996 selects current-culture behavior when the compatibility switch is set **or feature wave 18.11 is disabled**; lines 3416–3418 selects invariant versus culture-sensitive regular-expression options. `Traits.cs` reads the switch directly from the environment, outside the observed property wrapper. `MSBUILDDISABLEFEATURESFROMVERSION` therefore needs consideration too.

A simpler filename glob was an intentionally negative control: it did not change membership. The failing witness uses the complex directory pattern supported by the pinned primary tests.

## Parser configuration: separate closure gap

SDK 11 adds `MSBUILD_PARSE_CONFIG`, `MSBUILD_DISABLE_PARSE_CONFIG`, and `Directory.Parse.config`. None of these controlling environment names appears in the sampled v3 certificate.

The actual pinned loader is more precise than an outdated Traits comment: `ParserIgnoreConfiguration.LoadGlobalConfig` reads a platform-path-separator list from `MSBUILD_PARSE_CONFIG`; `ProjectCollection` additionally loads the nearest ancestor `Directory.Parse.config` starting from the selected project directory. `XMake` passes that directory explicitly. Do not implement an exe/user-profile search based on the stale comment.

Configuration permits otherwise unrecognized XML attributes/children. `ProjectParser` consults it on unknown-element/attribute branches; **this audit does not claim that it can skip an ordinary recognized Compile item or prove a valid-to-valid source-membership mutation through it**. It remains an unaccounted input to project acceptance and parsing. A narrow initial SDK 11 profile can reject active parser configuration while certifying relevant environment states and negative ancestor candidates. Supporting it requires hashing the selected config files and witnessing the nearest-file search, including missing nearer candidates.

## Required work before enabling a profile

1. Account for early MSBuild compatibility/feature switches separately from property-read events. For the demonstrated case, record the legacy-glob and feature-wave settings. If culture-sensitive behavior is supported, certify the effective culture and the host inputs that select it. An initial profile can instead reject those opt-ins. It must not silently force invariant behavior if the user's actual build uses another mode.
2. Add a regression using the genuine before/after successful builds and the old-model/fresh-model query cycle preserved here. Merely returning unavailable for every model is not a successful repair.
3. Close or explicitly reject the parser-config path described above. Retest applicable resolver selection, global.json and negative import controls against the exact SDK/MSBuild/RID. This audit did not prove every resolver/workload/direct filesystem read is intercepted.
4. Repair the independently discovered membership hashing mismatch below, then repeat current-model positive controls.
5. Require independent scorer replay before changing the shared SDK guard. A broad SDK-major allowlist is not justified.

## Separate existing membership-hash mismatch

`membership-sort-counterexample.json` records a producer/consumer mismatch unrelated to SDK 11's new switch. F# generic sorting of byte arrays orders by array length first, while the native membership certificate orders filenames lexicographically by bytes. A fresh recursive glob containing short source paths and longer generated obj paths was rejected as stale. The producer's observed hash matches the length-first ordering exactly.

The successful environment witness avoids this independent issue by placing the glob under `Sources`, where the two directory entries have the same order under both comparisons. No production or diagnostic sorting code was changed. The producer needs an explicit byte-lexicographic comparer and a matching cross-language fixture.

## Compiler repository and remaining custom-target boundary

The unchanged dotnet/fsharp checkout is `f896797a0f9b238a8cb1c482be424b714037ddc5`. Its restored/generated state and SDK pin were supplied by the evaluator. That agent separately observed a genuine resource-generation TaskHostFactory stall and an FSAC workspace-load timeout. This audit did not rerun or alter those builds and makes no compiler-repository semantic-readiness or performance claim.

Even a repaired ordinary SDK 11 environment profile would not authorize arbitrary Arcade imports, custom tasks, resource generation or unobserved filesystem reads. Those need their own evaluated dependency contract. Keep that limitation separate from the event-compatibility tests and the confirmed cache-validity bug.

## Evidence layout

- `summary.json`: compact decision, pins and wrong-result triplet.
- `cultural-membership-witness-v2.json`: successful builds, source hashes and full native transcripts.
- `witness-fixture/`: exact source/project/global.json bytes and both evaluated models.
- `environment-certificate.json`, `import-certificates.json`: the 15 previous controls on actual SDK 11.
- `pinned-source/`, `source-manifest.json`: immutable primary-source comparisons.
- `diagnostic-producer-source.tar.gz`: private SDK-guard experiment, never a production profile.
- `CHECKSUMS.sha256`: integrity manifest for this evidence bundle.

The preserved scripts record the actual workspace/host paths from this run. For another workspace, adjust their pinned tool paths and fixture root; do not use their diagnostic helper to enable general SDK 11 reuse.
