# Real VS Code lifecycle measurement

This supplemental Linux workload uses the editor's registered definition and reference
providers for one F# application-lambda parameter. It checks every URI, range and
occurrence before and after an unsaved buffer edit. It launches a fresh, disposable
desktop profile and then restarts Code with that same profile and index. The output
retains the first request, readiness retries, eight identical warm definitions, both
complete reference sets and edit-to-answer. An incorrect or unavailable edit response
remains in the report. Additional bounded probes record time to each first current
answer following that edit. A short setup guide follows.

```sh
mkdir -p .cache/editor-lifecycle
cp -R evaluation/editor-lifecycle/fixture .cache/editor-lifecycle/workspace
scripts/with-local-tools dotnet build .cache/editor-lifecycle/workspace/Lifecycle.fsproj -p:NuGetAudit=false
scripts/with-local-tools cli/bin/run projects evaluate --root .cache/editor-lifecycle/workspace --output .cache/editor-lifecycle/model.json
scripts/with-local-tools extension/test/with-display.sh python3 evaluation/editor-lifecycle/measure.py \
  --code "$CODE_BINARY" --provider navigator --vsix "$NAVIGATOR_VSIX" --backend "$FROZEN_BACKEND" \
  --workspace .cache/editor-lifecycle/workspace --model .cache/editor-lifecycle/model.json \
  --output .cache/editor-lifecycle/navigator-trial
scripts/with-local-tools extension/test/with-display.sh python3 evaluation/editor-lifecycle/measure.py \
  --code "$CODE_BINARY" --provider ionide --vsix "$DOTNET_RUNTIME_VSIX" --vsix "$CSHARP_VSIX" --vsix "$IONIDE_VSIX" \
  --workspace .cache/editor-lifecycle/workspace --output .cache/editor-lifecycle/ionide-trial
```

Use a pinned desktop binary from `extension/test/runtime-lock.json`. The Ionide 8.1.0
package is pinned in `evaluation/ionide-provenance.json`. Its activation requires one
of its supported C# extensions. The November 2026 Linux x64 trials used the OpenVSX
packages below, checked against their registry SHA-256 files.

| Package | Version | SHA-256 |
| --- | --- | --- |
| muhammad-sammy.csharp, linux-x64 | 2.160.9-g01198ecc1f | `3be755b4ccdb2a31d435910b6ff6c13ca34993ba085202f939be97a674fc0189` |
| ms-dotnettools.vscode-dotnet-runtime | 3.2.0 | `a4cff5409f66e3827ee2c6a727ae59382290a182a653ca0f9ac97c63536ee541` |

Run trials sequentially, reverse their order for a second block and leave the
workspace and preparation environment fixed. The VSIX package determines the
actual adapter. The Navigator executable override identifies the native candidate.
Each run measures the full Linux desktop process tree, including the extension host
and its native service. Summed RSS counts shared pages once per process. Sampling
at 50 ms intervals can miss short peaks. `launchToFirstUsefulMs` includes editor
startup and activation. `firstUsefulProviderMs` starts at the first provider call
after the editor opens the document. `alreadyActive` distinguishes an earlier
activation from one initiated by the probe. Restore, preparation, VSIX installation,
display setup and OS cache eviction are outside these timings. All measurements use
OS caches in their current state.

## Grace46 in the editor

Supply the frozen Grace checkout and the preparation settings for its actual
`src/Grace.Types` workspace. The same launcher opens the 23 original source
sites and requests each definition and reference set through the registered
providers. It records extension activation and whether the adapter received a
fresh native result. A retry of the opening query is appended after all 46 scored
operations. The scorer verifies the corpus revision and source digests and uses
the native evaluation's location comparison. The report distinguishes wrong
exact answers, unsupported native results and requests with no observed native
answer.

```sh
scripts/with-local-tools extension/test/with-display.sh python3 evaluation/editor-lifecycle/measure.py \
  --code "$CODE_BINARY" --provider navigator --vsix "$NAVIGATOR_VSIX" --backend "$FROZEN_BACKEND" \
  --workspace "$GRACE_ROOT/src/Grace.Types" --model "$GRACE_MODEL" --grace-settings "$PREPARED_SETTINGS_JSON" \
  --output .cache/editor-lifecycle/grace-trial
```

For on-demand navigation, supply the catalog and builtin environment settings.
An explicitly supplied `fsharpNavigator.semanticSnapshotPath` is recorded as a
separate preparation mode. Keep its compilation and export costs with that mode.
