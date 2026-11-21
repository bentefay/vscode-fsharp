# Isolated NativeAOT decompiler experiment

This is an optional Linux x64 experiment, not the production helper or a parity acceptance result. Production remains pinned to .NET10.0.11. All three comparison variants use the same adjusted C# source, SDK10.0.400, runtime/compiler10.0.14 and ICSharpCode.Decompiler11.1.0.9782. The requested framework assemblies are the same fixed10.0.11 input corpus in every variant. No resident helper process or FCS is introduced.

## Evidence

`comparison-10.0.14.json` contains a coordinated quiet-window comparison: three fresh processes per variant and case, rotated order, filesystem caches warmed by correctness tests. All target text, exact UTF16 identifier ranges and metadata identities match. Every returned dependency hash/missing marker is checked. Python controller, fixture builds and native daemon are excluded from the 10ms PPid/PID-starttime process-tree sampler. These are helper costs only; native invocation and cache validation cost are additional.

| Requested member | IL median ms / peak MiB | R2R median ms / peak MiB | AOT median ms / peak MiB |
|---|---:|---:|---:|
| String.Length |599 /87.0|456 /80.6|115 /45.4|
| DateTime.Year through reference assembly |637 /93.9|395 /86.1|121 /50.7|
| ILSpy package UsingDeclarations |753 /87.5|476 /87.6|182 /50.9|

On these three members, AOT is 4.1–5.2x faster than same-source IL and 2.6–4.0x faster than R2R, with roughly half the helper RSS. This is a small three-member benchmark, not orders-of-magnitude extension performance or full decompiler parity. First-process filesystem coldness was not measured.

The invalid-command startup lower bound is about40ms IL,37ms R2R and11ms AOT. This is not a decomposition of actual member time or a JIT profiler. The10ms sampler undersamples the very short AOT startup case; its reported RSS must not be interpreted as a realistic runtime footprint.

Each variant separately passes the existing14 real assembly tests (`*-validation.json`), with cross-variant equality recorded in `validation-parity.json`: exact body/span, overload identity, Unicode, fields, reference-only refusal, package ref-to-implementation identity, mismatched-signature refusal and output guard. All42 test invocations passed. Content URIs and code-dependency certificates necessarily differ by binary; actual document text/ranges and target member identity match. Dependency certificates are individually validated, not falsely declared identical.

## Pins, footprint, and warnings

`patched/inventory.json` records source, binaries, NuGet package archive SHA512 and sizes. Deployment payload excluding symbols: IL3.08MiB, R2R7.25MiB, AOT18.45MiB. AOT emits another~43MiB optional debug file. Metadata resolution still uses an explicitly selected .NET runtime metadata folder (or requested assembly dependencies); AOT is not a promise that all metadata files disappear from installation. Build/package download costs are separate: the runtime archive is36,673,986bytes, verified and unpacked in8.16seconds in this run; the initial full AOT compile wall time/build peak were not instrumented and are not claimed. Package archive/unpacked sizes are preserved.

The first10.0.11 probe failed to start an MSBuild worker and emitted NU1903 for CVE-2026-111497. It remains in `first-publish.log`; that warning was not suppressed. Official advisory lists10.0.14 as fixed. The patched experiment uses isolated10.0.14 runtime and compiler/linker/crossgen packages, preserving production10.0.11 historical evidence. See `runtime-10.0.14.json` for verified official archive URL/SHA512.

The patched build succeeds using the documented in-process .NET MSBuild task context (`UsingTask Runtime="NET" Override="true"`, default AssemblyTaskFactory); no package edits, socket permission changes or IPC shims. SDK/pack files are not modified. Single-file warnings remain visible in `patched/aot/publish.log`: own guarded Assembly.Location calls return empty; embedded code identity is instead covered by the executable hash. ILSpy UniversalAssemblyResolver and XmlDocLoader retain upstream file-path assumptions. The tested requests use an explicit metadata runtime search path and NoDocumentation provider, and pass. Unsupported Mono/other framework resolution, broader arbitrary assemblies, other platforms, SourceLink and embedded-source mapping are not established by this experiment. Do not suppress warnings or promote AOT to default on these14 cases alone.

## Reproduction

1. Run the normal pinned project setup for SDK10.0.400/runtime10.0.11 and NuGet dependencies. Fetch the official10.0.14 runtime archive from `runtime-10.0.14.json`, verify its SHA512, extract to `.cache/decompiler-runtime-10.0.14`, and symlink its `sdk`, `packs`, `sdk-manifests` to the existing `.tools/dotnet` folders. This leaves production runtime files unchanged.
2. Run `python3 evaluation/experiments/decompiler-aot/prepare.py`. It copies the preserved adjusted source, three projects, locks and license into separate experimental folders.
3. For each `il`, `r2r`, `aot`, invoke `.cache/decompiler-runtime-10.0.14/dotnet restore .cache/decompiler-patched/<variant>/FSharpNavigator.Decompiler.csproj --locked-mode`, then `publish -c Release --no-restore -p:BuildInParallel=false -m:1`, with `DOTNET_ROOT` set to that isolated runtime. GCC is the explicitly selected AOT linker/compiler supported by the pinned pack; other toolchains were not tested. Compare downloaded package SHA512 to `patched/inventory.json` before accepting reproduction.
4. Run `python3 evaluation/experiments/decompiler-aot/validate_variant.py <variant>` for each mode. Then coordinate a quiet window and run `python3 evaluation/experiments/decompiler-aot/compare.py`.

## Primary sources

- [NativeAOT deployment and limitations](https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/)
- [Microsoft .NET security advisory](https://github.com/advisories/GHSA-p3jp-9v83-76w9)
- [SDK10 task execution context](https://learn.microsoft.com/en-us/dotnet/core/whats-new/dotnet-10/sdk)
- [UsingTask override and runtime attributes](https://learn.microsoft.com/en-us/visualstudio/msbuild/usingtask-element-msbuild)
- [Official runtime release metadata](https://builds.dotnet.microsoft.com/dotnet/release-metadata/10.0/releases.json)
