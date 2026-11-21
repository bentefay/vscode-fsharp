# Parameter navigation: compiler and shipped FSAC boundary

This is correctness evidence for immutable native **3fb27998dab1**, before any document-local navigation integration. It identifies an existing complete-reference defect; the new lexical route has not been run in the production daemon. Timing fields in the raw transport responses are not performance measurements or accepted ratios.

## Provenance and validity

- Ionide extension **8.1.0** bundles **FsAutoComplete 0.86.0**, as reported by LSP initialization and its dependency manifest. Server DLL SHA256: `782521a137ddb71e6aec1e168912dadb774b7c382db35f4ba9b1664398e0e535`. Package SHA256: `d1183f936e092e3152e1bfca52895ba2418ef68f78e9eaed329b493af0ae998c`.
- The actual SDK400 build, pinned FSAC stdio requests, and direct pinned FCS project-symbol references are separate evidence sources. No actual VS Code host ran.
- Native SHA256: `3fb27998dab1ec2f0ee362deee64cc15af139e9c351ab71461dc1a724230f03f`; genuine observed-v4 project models came from immutable helper `09f01156157a28020e42e3531382d50e8f97e0a778f8dc30470454d4b43bd3db`. No handpatched evaluated models or semantic snapshots were used.
- The initial source oracle omitted signature/project diagnostics. Its unchanged bytes remain `oracle-implementation-diagnostics-only.json`; the corrected `oracle.json`/`oracle-validity-audit.json` classify **31 valid and 5 invalid cases out of 36**. In particular, `val f : (x:int -> int)` is invalid. The FSAC project uses a separately labeled valid `val f : (int -> int)` computed-function control, and its actual SDK build succeeds. The earlier failed build and report remain under `fsac-parameter-controls/`.

## Paired signature parameters

`fsac-signature-origin/report.json` retains 28 cursor sites / 84 actual provider responses. Twenty-seven came from the original valid combined project; one supplemental process added the unchanged renamed signature-origin cursor. `native-paired-3fb2-all-origins/report.json` replays those same cursor locations through the normal indexed path with genuine project inclusion.

| Operation | Correct | Incomplete exact set | Paired-target preference difference | Unavailable |
| --- | ---: | ---: | ---: | ---: |
| Definition versus actual FSAC target | 14 | — | 12 | 2 |
| References, declaration included, versus FSAC and FCS | 17 | 9 | — | 2 |
| References, declaration excluded, versus explicit FCS uses-only contract | 26 | 0 | — | 2 |

The nine incomplete sets are three cursor positions each in an ordinary exported function formal, direct RHS lambda formal, and direct returned lambda formal. Native returns the implementation declaration and two uses as **exact**, but FCS/FSAC also include the same-named `.fsi` parameter declaration. This is a semantic completeness blocker, distinct from definition preference.

FSAC directs implementation parameter definitions to the paired signature. The reverse signature-origin cursor usually directs to the implementation. The native implementation target represents the paired parameter, so this difference is recorded as target-preference parity, not selection of an unrelated symbol.

With signature `publicName` and implementation `x`, FSAC still navigates between the two parameter declarations. References stay separate: implementation-origin `x` has three implementation occurrences; signature-origin `publicName` has one signature occurrence. Direct FCS agrees on these separate reference sets. A fix must not append a signature parameter merely by ordinal or name substitution. Both native signature-origin cursors are currently unavailable.

Application-argument lambda, computed `id` lambda, ordinary nested function and computed returned local-function controls retain local parameter identity. Both operations in the new document-only route must exclude exported/signature-correlated forms; raw FCS declaration preference alone is insufficient for Ionide parity.

## `includeDeclaration` is not normalized away

For all 28 paired cursor sites, bundled FSAC0.86 returns the same declaration-inclusive references with either request flag. The normal adapter forwards `referenceContext.includeDeclaration` unchanged; Ionide's bundled languageclient also forwards it. This report does not assume every actual VS Code UI action supplies `true`.

The `true` arm is the direct set comparison. The `false` arm preserves our explicit exclusion contract against FCS `IsFromDefinition` markers and keeps actual FSAC's same-flag raw result alongside it. Its disagreement is recorded rather than silently called parity. A real editor-host UI reference invocation remains unverified.

## Physical and logical signature filenames

`signature-path-controls/report.json` preserves four independent actual SDK builds and queries only successful builds:

| Physical files / project Link metadata | SDK validity | Observed parameter references |
| --- | --- | --- |
| `Signatures/Local.fsi`, `Sources/Local.fs` | Invalid, FS0240 | No native/FSAC acceptance claim |
| Same physical paths, different Link aliases | Invalid, FS0240 | No acceptance claim |
| Same directory `Signature.fsi`, `Implementation.fs`, both `module Local` | Valid | FCS/FSAC group signature plus implementation occurrences |
| Same different basenames, Link aliases `Logical/Local.fsi` and `.fs` | Valid | Same grouped references |

Both valid cases reproduce the native exact subset defect. Therefore a completeness guard cannot look only for the implementation path with `i` appended. Build validity here is an observation on these precise SDK400 fixtures, not a general filename pairing algorithm.

## Methods and constructors without a signature

`callable-parameter-controls/report.json` contains an actual successful project build, full-project FCS diagnostics and 33 FSAC responses at 11 cursor sites. Constructor `seed`, instance-method `amount`, and static-method `delta` each have a named-argument use in a later source file.

FCS project references include those caller labels. Native returns body-only exact sets at eight declaration/body cursor sites, missing the caller occurrence for **both flags**; three caller-label origins are unavailable. The derived independent comparison is `native-callable-3fb2/fcs-completeness.json`.

Bundled FSAC0.86 also omits these labels: it returns body-only sets from the declaration/body and empty sets from caller-label origins. Native therefore matches that FSAC limitation on the eight body origins, but still violates the required complete-reference semantics. This distinction is explicit; a baseline omission is not permission to claim semantic completeness.

## Direct function-pattern positive

`function-pattern-control/report.json` validates `val f : x:int -> int` with `let f = function x -> x+x`. The simple match binder remains local: FCS/FSAC give three implementation occurrences, while the signature parameter is its own one-item identity. Native implementation definition and both explicit reference sets are correct; the signature origin is unavailable. This control prevents a signature guard from treating every binder in an exported function body as a public formal. Complex match/payload patterns are outside this one control.

## Staged document-local acceptance

Two executable suites are written and syntax-checked, **not yet run against an integrated native candidate**:

- `extension/test/localLexicalNative.test.cjs`: real adapter/native transport with an explicitly labeled editor API double. Activation and clean tabs spawn nothing; first local provider call must work with deliberately missing model/catalog/environment/helper paths. It checks exact document version/proof identity, no invented project context, UTF16/CRLF, declaration flag, dirty dependencies, shadowing, restart, initialization/edit race and excluded syntax falling through.
- `native_acceptance.py`: direct actual-native protocol checks for cold/warm queries, stale version, rename, close/reopen, process restart and zero model/source-scan/catalog/inventory/helper counters. Transient local parse work must be counted separately rather than hidden by zero general AST counters.

No tree `getChildren` call is injected into those positive tests. The production adapter requests the tree only when that API is invoked; an actual restored visible VS Code view may invoke it independently, which remains editor-host work to measure.

The provisional response contract is `source: local-lexical`, `proofScope: document`, exact `version`/`documentVersion`, BLAKE3 `sourceHash`, `documentProofId`, and no `projectContextId`. The suites require an explicit immutable native path and are not wired into the normal CLI until the implementation exists and its schema is finalized.

Example future normal-path safety replay (fresh output directory required):

```sh
scripts/with-local-tools python3 evaluation/experiments/local-lexical-proof/native_paired_replay.py \
  --binary /absolute/immutable/fsharp-navigator \
  --helper evaluation/.runtime/helper-09f01156157a/FSharpNavigator.Semantic.dll \
  --fsac-report evaluation/experiments/local-lexical-proof/fsac-signature-origin/report.json \
  --model evaluation/experiments/local-lexical-proof/native-paired-3fb2/model.json \
  --require-sound-references --output-dir /absolute/new-output
```

The safety flag permits explicitly unavailable results but rejects exact sets that differ from direct FCS project references. Coverage and target-preference parity remain separately reported; safe refusal is not feature equivalence.
