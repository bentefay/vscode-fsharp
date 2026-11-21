# Review contract for observed MSBuild environment dependencies

The reproduced failure is concrete: `environment-membership-86548.json` changes `IncludeB=false` to `true`, leaves project/source bytes unchanged, and successfully builds both variants. The old model claims reusable membership and returns an exact empty reference set; fresh evaluation includes B.fs and its reference. This must be fixed before candidate acceptance.

## What the primary source establishes

[PropertyTrackingEvaluatorDataWrapper](https://source.dot.net/Microsoft.Build/Evaluation/PropertyTrackingEvaluatorDataWrapper.cs.html) instruments property reads when enabled, distinguishes environment-backed reads from missing properties, and tracks overriding assignments. Its flags are 4 for environment reads and 8 for uninitialized reads. The wrapper's environment event message is not a raw ambient-value certificate. Capture values from the evaluation's starting environment snapshot. A later assignment must not erase an earlier dependency.

[Evaluator](https://source.dot.net/Microsoft.Build/Evaluation/Evaluator.cs.html) installs the wrapper using `Traits.Instance.LogPropertyTracking` and supplies it to expansion. The pinned local SDK10.0.400 Framework binary contains the setting name `MsBuildLogPropertyTracking`; initialization order and actual event delivery still need a runtime probe.

[ProjectPropertyInstance](https://source.dot.net/Microsoft.Build/Instance/ProjectPropertyInstance.cs.html) also emits environment-derived value-access events under logging conditions. That is another path to verify, not proof that arbitrary process environment access is intercepted. These online source pages describe current code; pin runtime assembly hashes and test the actual shipped SDK before relying on matching behavior.

## Required probes

| Probe | Mutation / expected observation | Acceptance condition |
|---|---|---|
| Direct positive read | `Compile B.fs` conditioned on ambient IncludeB; false→true. | Certificate contains the actual ambient dependency and old model is rejected or refreshed. |
| Negative read | IncludeB absent→present true. | Missing state is captured; an old empty result cannot remain exact. |
| Conditional default | `ChooseB Condition="'$(ChooseB)'==''">false`; then Compile condition. | Certify initial ambient absence, not merely the final default value. |
| Read before overwrite | Copy ChooseB into Selected; then set ChooseB=false unconditionally. | Earlier read remains a dependency of Selected and the item domain. |
| Unconditional overwrite control | Set ChooseB=false before any use regardless of ambient value. | Changing ambient ChooseB need not invalidate when evaluation never consumes it. |
| Explicit global property control | Pass a fixed global ChooseB=false while ambient changes. | Global properties are separate context inputs; ambient changes must not be mistaken for changes to the effective property. |
| Transitive/imported use | Import props that assign Selected=$(ChooseB), then Include=$(SelectedFile). | Events cover the actual transitive read; imported bytes and resolved import paths remain certified. |
| Short-circuit condition | A false guard suppresses reading a second property; later change the guard. | Guard dependency invalidates the old trace before a newly relevant second property can matter. |
| SDK-only input | Change an SDK-supported default-item/exclusion switch not textually referenced by the project. | Observed dependency must cover SDK evaluation, not just user XML tokens. Verify actual source-item change in a valid project. |
| Resolver/setup input | Change an effective SDK resolver/environment input before project evaluation starts. | Either record the applicable resolver/toolchain context separately or reject reuse. Property-read events alone must not be advertised as intercepting this. |
| Direct environment property function | Use `[System.Environment]::GetEnvironmentVariable(...)` in a source-affecting expression. | Prove an observed dependency or mark unsupported; absence of property events is not proof of independence. |
| Execution-phase input | A permitted target reads a property or produces a Compile item after evaluation. | Capture relevant target execution dependencies too, or keep custom source-producing targets non-reusable. |
| Case/missing/value transitions | Mixed-case keys, conflicting duplicate case-insensitive names, empty and missing values. | Match MSBuild's lookup semantics; ambiguous inputs reject. Do not merge missing and empty without proof. |
| Log configuration | Diagnostic/normal quiet logger, tracking12/15, LogAllEnvironmentVariables settings, multi-project evaluation. | Certifier requires demonstrated event coverage and project context; a missing/filtered event stream cannot silently produce a reusable certificate. |

Each mutation test needs a positive current-model query, a successful relevant build/evaluation, a restart with the old model, and a fresh-model control. Keep build environment differences explicit. Do not publish unrelated ambient variable values; only dependency names, missing markers and hashes are needed for the certificate.

## Bounded claim

Observed positive and negative property dependencies can support precise cache reuse for the supported evaluation path. They do not by themselves capture arbitrary filesystem reads, SDK resolver internals, custom tasks, direct environment API calls, or generated source membership. Those dependencies need their own certificates or an explicit unsupported result. A global environment fingerprint is safe only with an intentionally broader invalidation contract, and may make CLI-to-editor reuse impractical; it is not the preferred final UX.

Until the event path is verified, rejecting unknown source-affecting conditions is the safe interim behavior. Rejection is a correctness guard, not completion of the user's project/navigation requirement.
