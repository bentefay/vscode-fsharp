# Evaluation recovery ledger

The VM reset restored an earlier source checkpoint. Historical claims are not fresh measurements, and lost raw responses have not been recreated by hand. New evidence below came from actual reruns after recovery.

- Pinned public corpus sources survived: Grace `39960e07b73cd142cc0c1fd070c0591be91c1933`, dotnet/fsharp `f896797a0f9b238a8cb1c482be424b714037ddc5`, B2R2 `5a9ba0e27d868ffb44831cf2ba98afb4be2d3904`.
- Ionide 8.1.0 VSIX was downloaded again and verified against SHA-256 `30c6668b9790a36ed1f2e83a15c754e67102986e25863e368f55d67743a6d73a`. Its actual FSAC server runs through stdio; this is a language-service baseline, not a successful VS Code host test.
- The original 100 fixture specifications survived. Source-only recovered candidate `ec4153a27b684ae56c25370945c359aab604df1b77420377bb4b3f10d643ba7b` freshly returned 89/100 correct and 11 unavailable. This is a diagnostic configuration, not final acceptance.
- The paired signature suite was reconstructed exactly from preserved history. Actual FSAC freshly passed all 13 cases. Ordinary uses and implementation declarations navigate to `.fsi`; the signature declaration cursor toggles to `.fs`. References include both declarations.
- The fixed Grace selection algorithm was reconstructed, with its seed, order, token names and candidate-pool counts asserted. It selects 30 sites: 23 source sites (46 definition/reference queries), six external sites, and one unmatched syntactic candidate. No native outcomes determine selection. Scope is Grace.Types plus its evaluated source inputs, not all of Grace.
- Fresh direct FCS checks confirmed all 23 source definition targets. Fresh actual FSAC responses match all 46 canonical expected sets.
- The initial recovered semantic export still had an alias-equivalence defect: unrestricted erased-alias union previously produced 315 distinct Sha256Hash references; the recovered narrow guard yielded only 42, omitting 11 constructor-form occurrences. Fresh direct FCS and actual FSAC both return 53. The canonical manifest explicitly overrides that single reference group with the direct compiler result and records its hash. A later independent rerun with immutable helper `4c9d24e5c31ff44be6122cfcd2dd1aeb295b49cb6d0680aea920d340bd9484a4` verifies the repaired occurrence-specific export against **all 46** canonical sets, including exactly 53 alias references. The original canonical manifest remains unchanged.
- The actual FSAC constructor-cursor probe independently confirms alias references are cursor-sensitive without conflating aliases: Sha declaration, annotation and constructor all return the three Sha occurrences; Other returns its own three. Constructor definitions target decompiled System.String, while alias declaration/annotation definitions target the source alias. Both includeDeclaration flags return identical sets in this shipped server.
- Fresh model, full external type inventory and streamed Core/Runtime catalogs are prepared for the expanded fixture and Grace.Types using helper `4c9d24e5c31f`. The full inventories contain 4,537/13,987 entities and occupy 2,331,118/6,972,544 JSON bytes respectively. These artifacts and their setup costs are explicit; they are not a free startup prerequisite or a proof of native navigation coverage.
- Actual FSAC external hover captures show use-site instantiation: List.map and Array.choose retain generic declaration signatures but additionally bind generic parameters to actual argument/result types. Tuple<Person>.Item1 displays Person; Tuple<int>.Item1 displays int. Catalog-only generic declarations do not satisfy these new assertions.

Canonical fresh evidence:

| Evidence | Path |
|---|---|
| Fixed sample generator | `select_grace_samples.py` |
| Original positions, without invented goldens | `results/reconstructed-grace-selection.json` |
| Canonical 46-query goldens, explicit alias override | `results/recovery-grace-goldens46-corrected.json` |
| Actual FSAC raw 46 responses | `results/recovery-grace-fsac-raw46.json` |
| FSAC correctness comparison | `results/recovery-grace-fsac-validated46-corrected.json` |
| Independent direct FCS 23 bindings | `results/recovery-grace-direct-bindings23.json` |
| Direct alias reference proof | `results/recovery-grace-sha256hash-direct-references.json` |
| Alias export difference | `results/recovery-alias-group-difference.json` |
| Repaired export versus all 46 canonical sets | `results/recovery-grace-export-4c9d-validation46.json` |
| Alias constructor-cursor actual FSAC contract | `results/recovery-alias-routing-fsac.json` |
| Actual paired-signature FSAC responses | `results/recovery-signature-fsac-confirmed.json` |
| Actual contextual library hover responses | `results/fsac-library-hover-contract.json` |
| Contextual hover assertions | `results/fsac-library-hover-instantiation-validated.json` |

The corrected memory sampler uses PPid edges plus kernel process start identities, retains observed descendants after reparenting, and prunes dead or reused identities. Real child/grandchild and reparenting regression tests pass. Setup command sampling has timeout/RSS guards and identity-checked cleanup. Full measured subtrees and artificial Python bridge/backend breakdowns remain separate. Summed RSS excludes controller memory and filesystem/page-cache storage; it is not whole-system memory or PSS.

All recovery timings are diagnostic under concurrent development. Final comparisons still require a reviewed immutable native candidate, matching evaluated project context and complete metadata inventory, validated equivalent query sets, explicit preparation costs, restart and dirty-edit traces, and a coordinated quiet window. Unsupported responses never qualify as speed wins.

## Recovered integrated candidate 86548

The immutable diagnostic native binary has SHA-256 `86548fa855ca16f10c26cd3db5df0b56ee426c89c106c9e63c42a9a094528ac7`. It is **not accepted**: the main suite contains an exact external hover on a repository-owned System.String member. Independent review has additional acceptance gates. Current coverage is:

| Suite | Native correct | Actual FSAC correct | Qualification |
|---|---:|---:|---|
| Original combined fixture | 99/100 | Earlier raw evidence retained | One wrong external hover ownership result |
| Dirty edits | 15/15 | Earlier raw evidence retained | 12 record/property checks plus 3 original edits |
| Fixed Grace.Types source sample | 10/46 | 46/46 fresh | 36 unavailable; no wrong exact result in this sample |
| Paired signature navigation | 4/13 | 13/13 fresh | Six exact same-symbol targets differ from FSAC's `.fs`/`.fsi` routing; three unavailable |
| Contextual external hover | 0/6 | 6/6 fresh | Four unavailable; two generic declarations lack use-site type substitution |
| Conditional contexts, reconstructed v2 | 2/16 | 16/16 fresh | 14 unavailable; explicit new fixture version, not claimed identical to the lost suite |

The first Grace attempt could not reach binding because generated NuGet imports lacked reusable membership provenance. Those 46 protocol errors are preserved in `results/recovered-86548-prepared-grace46.json` and are not semantic coverage data. The helper's genuine subsequent evaluation resolved the imports. The corrected model preserves the same files, compiler arguments, references and target framework; its successful native replay is `results/recovered-86548-membership-fixed-grace46.json`.

The fixed 46 queries never load the configured external inventory, so their peak RSS of 21,316 KiB is not full-feature residency. A separate real-source String.IsNullOrEmpty hover loads the full inventory: startup 5,696 KiB, first lookup 54,288 KiB, and immediate warm steady RSS 54,644 KiB. That hover is unavailable; these figures are diagnostic resource evidence, not a speed win or complete navigation result. The retained trace `results/recovered-86548-grace-full-inventory-rss.json` records the request, full configuration, before/after loaded state and process-tree RSS. There are no matched performance ratios yet.

The new conditional suite at `../fixtures/conditional-contexts-v2/` was predeclared and built successfully before either engine was queried. It exercises two actual linked compile instances, common-declaration reference union, branch-distinct record fields, nested Boolean directives, CRLF/UTF-16 positions, inactive malformed text, and directive text inside active comments and triple strings. Its actual FSAC proof is `results/recovery-conditional-v2-fsac16.json`; it leaves the fixed Grace selection unchanged.

## Subsequent diagnostic candidate edafa

Candidate `edafa1f8dd98386f56726b31b57871e5c93acd0f1c03bdfd1a1688219ec73245` passes the main 100 and all 15 dirty checks, and fixes the paired-signature suite to 13/13. Contextual hovers improve to 2/6: Tuple property substitution passes, while the four map/choose hovers remain unavailable. Conditional v2 remains 2/16. The unchanged Grace sample remains **8/23 definitions and 2/23 complete reference sets**, 10/46 total. Independent review still blocks this candidate on an external same-qualified-name ownership counterexample and project-certificate concerns. These are accuracy diagnostics, not acceptance or timing comparisons.

The shared fixture runner now opens each cursor document before querying it, beyond the initial startup-buffer sample. This is applied to both engines and versioned in the workload fingerprint. `results/recovery-fsac-main100.json` is retained as an **invalid unopened-document probe**: its missing foreground-check errors are a harness lifecycle mistake. The corrected `results/recovery-fsac-main100-opened.json` scores 97/100 with no protocol errors. Two differences are intentional repository-hover suppression policy differences; the remaining generic Box.Value reference query omits three uses. A separate replay opens and checks every relevant generic use, confirms all three definitions target the same field, and repeats references from both declaration and use cursors twice; the omission persists in `results/recovery-fsac-box-reference-replay.json`. Expected native reference sets are not reduced to accommodate it. The same-lifecycle native result is `results/recovered-edafa-prepared-100-opened.json`, 100/100.

The optional documented FSAC navigation-only configuration is `config/fsac-navigation-only.json`. Its analysis-notification readiness policy waits for the queried document and pending changed dependencies, rather than unrelated initial opens whose checks may be superseded. The earlier all-open-notification timeout is retained as a harness-policy failure, not a baseline latency result. Both the corrected policy and direct provider requests score the same 97/100 (`results/recovery-fsac-navigation-only-main100-readiness-v2.json` and `results/recovery-fsac-navigation-only-main100-direct-providers.json`). No comparative timings are claimed from these concurrent development runs.

The largest corpus's exact SDK 11.0.100-rc.3.26522.148 was restored using its own installer. A bounded compiler-project restore, retaining repository source and supported DotnetProjInfo/IgnoreMibc settings, timed out after 240 seconds at about 232 MiB process-tree RSS; only the small AssemblyCheck prerequisite completed. A separate 90-second diagnostic confirms package metadata fan-out across the repository's 20 Azure feeds, including repeated 404s and 4–6 second requests for ordinary pinned packages. Further retries were stopped. This is an explicit setup limitation; no compiler-project semantic baseline or whole-project speedup is claimed. Raw evidence is `results/recovery-fsharp-sdk-setup.json`, `results/recovery-fsharp-compiler-restore.json`, and `results/recovery-fsharp-compiler-restore-diagnostic.json`.


## Full-scope diagnostic candidate ff85

The immutable candidate `ff85a38647ca126934e0695df3abf4befcf127b84f5db3ca2c73014650125a6a` uses genuine observed-environment v3 project models and matching full external type, scope and term inventories from helper `4efbac974e9e`. The Grace inventory is 15,607,364 JSON bytes; expanded fixture inventory is 9,482,718 bytes. Both are explicit binary-metadata preparation inputs, not workspace semantic snapshots. Their SQLite imports, not retained JSON graphs, are exercised by the native daemon. No final acceptance or relative performance claim is made.

| Suite | Correct | Qualification |
|---|---:|---|
| Fixed Grace source sample | 16/46 | 12/23 definitions; 4/23 complete reference sets; 30 unavailable |
| Combined fixture | 99/100 | Repository-owned System.String hover safely returns unavailable/null instead of proven source suppression |
| Dirty edits | 15/15 | Separate original and common-field edit checks |
| Paired signatures | 13/13 | Actual FSAC signature/implementation routing |
| Contextual external hover | 2/6 | Tuple substitutions pass; map/choose use-site hovers unavailable |
| Conditional source contexts v2 | 8/16 | Unchanged coverage; uppercase source binding sites remain unavailable |

These suites contain no wrong-exact responses, but independent review rejects this candidate: seven additional uppercase binding-scope cases return incorrect exact external hovers (opened module exports, local values, tuple/match/active-pattern binders). The retained proof is `../docs/review/uncertain-binding-ff85.json`. The fixed Grace selection and all goldens are unchanged. The one Grace gain over candidate2734 is the ArtifactId access definition. Fresh result and complete miss categories are `results/recovered-ff85-prepared-grace46.json` and `results/recovered-ff85-grace-misses.json`. Other suite files share the `recovered-ff85-prepared-` prefix. Preparation-only environment exports have `recovery-*-environment-scopes-4efbac-setup.json` reports and can be reproduced with `export_environment.py`.

A separate successful-site lifecycle trace uses Kty and OnFailure, selected because the preceding fixed sample proved them supported. It **does not replace the 16/46 coverage denominator**. An independent direct-FCS run verifies all 12 clean/body-edit/API-rename/undo answers. Native ff85 then returns 56/56 correct answers across first use, 40 repeated warm queries, both edit types, undo and persisted restart. Repository files are untouched; the API rename updates exactly the independently proven complete occurrence set in unsaved overlays. The runner is `grace_success_trace.py`; oracle evidence is `results/recovery-grace-lifecycle-edit-oracle.json`, and native evidence is `results/recovered-ff85-grace-success-lifecycle.json`.

These lifecycle numbers are diagnostic shared-machine observations, not controlled comparisons: process start through the first correct definition was about746ms, and through the first complete reference answer759ms. The first lookup parsed33of39 source instances. Warm median query times were1.34ms for definitions and1.20ms for references, with zero source reads or AST parses across all40 warm requests. Body edit through complete references took20.8ms; a valid all-occurrence API rename took17.6ms; each reparsed one file. Persisted restart through the first correct definition took108ms with zero AST parses, but did validate37files. Startup and two document-open notifications read zero source files and built zero ASTs.

Peak observed native process-tree RSS for this lifecycle was28.3MiB; full term/type inventory was loaded. Catalogs were configured but the trace requested only source navigation, so this is **not a maximum feature-residency measurement**. SQLite/index storage and operating-system page cache are excluded from process RSS. Initial cache-empty indexBytes peaked around48.4MB including transient database/WAL state; restart reported about23.7MB. No compiler/helper/decompiler process launched. The prepared model, inventory and catalogs, plus repository build prerequisites, are separate explicit setup costs rather than free cold-start work.


## Alias and DU payload follow-up

Diagnostic candidate `affad5a7dc8c599fdfadeefe40d40b34689d14f848a27f5353ca0b13e657abcc` improves the unchanged Grace sample to **18/46: 14/23 definitions and 4/23 complete references**. The gains are the Sha256Hash alias RHS definition and RepositoryId in a record argument to `ExactRelationship.ReferenceRoot`. Main100 remains99/100. The corrected shadow-guard candidate `9201ad3fdb0f628eb69d3213cf0021646d127a4c6eb747ade3f1a45dd305520a` retains the same Grace18/46 and restores main100/100. These are correctness diagnostics pending independent acceptance, not new performance comparisons. All prepared inputs remain genuine and unchanged.

The added reference diagnostic leaves the fixed sample untouched. Its raw probes and explanation are `results/recovered-affad-reference-diagnostic.json` and `results/recovered-affad-reference-analysis.md`; reproduction uses `reference_diagnostic.py`. The alias partial set contains41/53 expected uses, missing exactly11constructor forms and one nameof use. Ranges10/11 and ArtifactId6/7 each miss one member-body record label. The DU RepositoryId partial set already contains all4expected sites but cannot certify exclusion of other candidates. This distinguishes positive binding gaps from negative-domain proof gaps. Raw lexical candidate probes are explicitly supersets, including strings/comments; they are not represented as the exact native lexer candidate list. Alias constructor physical definition routing is kept separate from logical reference membership.

## Compiler corpus setup: real restore and generated sources

The pinned dotnet/fsharp checkout f896797a0f9b238a8cb1c482be424b714037ddc5 now restores its actual FSharp.Compiler.Service project with its declared SDK11.0.100-rc.3.26522.148 and unmodified source/configuration. An isolated bootstrap copied and verified all evaluated PackageSpec constraints before cache preparation from the already configured dotnet-public feed. The five genuine assets files subsequently matched the verified package identities and NuGet canonical content hashes. Real restore completed in2.08s after cache preparation; this is a warm-cache setup operation, not an Ionide comparison.

The repository’s fslex and fsyacc projects built without restore, then its CallFsLex/CallFsYacc targets generated all12 previously missing lexer/parser sources in the declared net11.0 context. Tool builds took4.24s and3.70s; generation took27.62s and peaked at541MiB summed process RSS. Read-only selected-context evaluation now lists424 existing Compile files totaling15,704,040bytes. Authored/configuration hashes remain unchanged. These setup samples were not taken in a quiet performance window.

This does not yet establish semantic readiness: a subsequent genuine design-time Compile stopped at the repository’s separately invoked FSharp.Build resource task, whose assets had not been part of the earlier project-reference restore graph. Its real graph and isolated cache replica are now recorded; required prerelease packages are available at the already configured dotnet-tools feed. Reusable navigator project certificates also currently recognize only SDK10.0.400, so any SDK11 model remains diagnostic until that profile is independently implemented and tested.

Evidence: evaluation/results/recovery-fsharp-after-closure-restore.json; recovery-fsharp-cache-closure-hashes.json; recovery-fsharp-real-assets-validation.json; recovery-fsharp-generation-{tools,generate,evaluate}.json; recovery-fsharp-generated-compile-items.json; recovery-fsharp-design-time-probe.json.

Compiler setup follow-up: the separately required FSharp.Build resource tool also restored/built successfully after an exact matching isolated graph restored11 verified packages from existing dotnet-tools + dotnet-public. The real resource-tool restore took1.39s; build3.66s. Genuine design-time Compile then timed out120s at GenerateFSharpTextResources → FSharpEmbedResXSource → external TaskHostFactory. A15s verbose probe identifies that launch, but does not expose an EPERM syscall. Actual Ionide-shipped FSAC0.86 launches under the repository SDK11/runtime11 using its shipped Major roll-forward policy, then workspaceLoad for the compiler project times out45s after projectLoading with no loaded/finished event. No full compiler semantic baseline is claimed, and no task/config substitution was used. Consolidated evidence: evaluation/results/recovery-fsharp-setup-summary.json, resource-tool-build.json, resource-tool-validation.json, design-time-stall-diagnostic.json and fsac-load.json/.lsp.jsonl (all filenames have the recovery-fsharp- prefix).

Read-only task-host review found no documented normal in-process option for the repository’s explicitly hardcoded TaskHostFactory resource tasks. Pinned SDK MSBuild18.12.0 source and Microsoft documentation agree that explicit hosting forces a separate process. Compiler semantic-load attempts stopped; no shim, override, task skip or repository rewrite was used. Evidence: evaluation/results/recovery-fsharp-taskhost-option-review.json.

## Aligned0a77/schema2 diagnostic

The immutable0a77bde1efcd candidate and freshly exported f215dac4599c schema2 metadata retain100/100 exact-correct synthetic queries. Fixed Grace.Types remains a distinct practical gate:19/46 exact-correct (15/23definitions,4/23complete references),27unavailable,0wrong-exact/protocol errors. One new successful definition is the StoragePoolId construction. There is no complete-reference coverage gain. The alias partialset remains the same41/53; positive constructor/nameof uses are stillmissing. Debugproof traces show6unrelated qualifier-root candidates removed from RepositoryId, but otherfield uses and binder-free alternative-pattern uncertainty stillprevent a completeness certificate. These are diagnostic observations, not accepted performance ratios.

Fresh actual CLI exports are under evaluation/.runtime/recovery-prepared-{grace,expanded}-f215; the existing genuinev3 model bytes are copied unchanged. Original artifacts are retained. Rawreports: evaluation/results/recovered-0a77-prepared-{grace46,100}.json; classifiedmisses recovered-0a77-grace-misses.json; exact proofgap contexts recovered-0a77-reference-gaps.json. The initial unwrapped Grace invocation had no matching DOTNET_ROOT and was correctlyrejected; preserved as invalid-env and excludedfromcoverage.

7047 follow-up (same genuinef215 schema2 artifacts): fixed Grace20/46exact-correct =15/23definitions+5/23complete references,26unavailable,0wrong-exact/protocol errors inthismatrix. validate references nowcomplete3/3. Alias partialreferences improved41→52of53; remaining positivegap is Reference.Types.fs143:43`nameof Sha256Hash`, reasonintrinsic-nameof-not-proven; Common.Types.fs615:16recordlabel is an unrelated negativecandidate. Core independently owns the main100regression. Rawfiles: recovered-7047-prepared-grace46.json and recovered-7047-grace-misses.json. Known independent.fsi active-pattern gate remains pending, so this is diagnosticcoverage, not acceptance.

## Additional genuine Fable compiler project

The pinned Fable fcs-fable project now restores and builds unchanged with its exact SDK10.0.100/runtime10.0.0. Its287 evaluated Compile inputs contain250,598 physical lines, including two SDK generated assembly-info files. Actual Ionide-shipped FSAC loads the explicitly selected project and returns all twelve predeclared operations. Independent direct FCS verifies all six definitions and complete reference sets; three raw span-start differences are explained by the pinned FSAC identifier-range adjustment and retained explicitly. This is a single selected project, not whole Fable. The diagnostic backend peak was1.975GiB; first late-file definition took14.71s after3.15s project initialization, with no accepted comparative ratio.

The unchanged helper's genuine same-host SDK100 evaluation remains nonreusable: SDK400-only profile,273 property-expanded Compile paths and other import/task/condition rules. No certification fields or project configuration were changed. Detailed context, setup costs, fixed contract, source-backed range normalization, blockers and reproduction are in `results/fable-corpus-feasibility.md`. Primary raw reports are `fable-fcs-fsac-feasibility.json`, `fable-fcs-direct-oracle.json`, `fable-fcs-verified-contract.json`, `fable-fcs-context-alignment.json` and `fable-fcs-certificate-audit.json`. Largest measured dotnet/fsharp remains separately blocked at the explicit resource TaskHost.

## Genuine v4 and85d1 Grace reference progress

Immutable helper09f01156157a28020e42e3531382d50e8f97e0a778f8dc30470454d4b43bd3db genuinely re-evaluated the Grace and expanded fixtures into observed-environment v4 models. Both are reusable and exactly preserve prior ordered files, compilerArgs, target/configuration, project references, selected assembly references, workspace assembly exclusions and output paths. The prior f215 binary catalog/inventory artifacts therefore remain unchanged. Refresh provenance is in `results/recovery-{grace,expanded}-model-v4-09f011.json`; no model fields were hand-edited.

Native85d1d8635f331ff9fff7fc8db8f0527c4a97f09a5c8716303e73e71b764b7486 reaches22/46 fixed Grace operations:15/23definitions and7/23complete references, with24unavailable and0wrong-exact/protocol errors in this matrix. Sha256Hash is now53/53exact and Artifact2/2exact, both with zero proof gaps. State6/6 remains incomplete only at Reminder.CreateWithId's label; the Default label gap is gone. Ranges10/11 and ArtifactId6/7 retain their prior positive gaps. Reports: `results/recovered-85d1-prepared-grace46.json`, `recovered-85d1-grace-misses.json`, `recovered-85d1-reference-gaps.json`. Core separately owns main100; these concurrent correctness runs establish no performance ratio.

The earlier focused field analysis remains useful but its proposed unconditional absent-label exclusion was independently rejected by seven actual compiler controls: semantic-error recovery can preserve target field references despite an extra unknown label. See `results/recovered-f27d-field-reference-analysis.md` and `docs/review/record-negative-oracle.json`. The accepted grammar-role exclusion is separate from that rejected semantic shortcut.

Digest-reuse-only successor858f9d9a16d39338b63406916bf0fdf0bb2138a3b7b2ae9efd9bdc0d316bd14d preserves the exact22/46 Grace result and all46 returned location/completeness/diagnostic/proof-gap structures from85d1. Same genuine09f011 model and f215 artifacts; no semantic changes or performance acceptance claimed. Evidence: `results/recovered-858f-prepared-grace46.json`, `recovered-858f-grace-misses.json`, `recovered-858f-vs-85d1-grace-equality.json`.

## Closed factory candidate 78fd

Immutable `78fd26ebfd67e709131b87ad457783a3cd97fffcd722f670fe8c8f854a09ae9b` replayed unchanged main100 and Grace46 using genuine 09f011 observed-v4 models/f215 metadata. Main100 remains 100/100 with identical responses. Grace regresses from 22/46 to 14/46 (definitions 15→10/23; complete references 7→4/23), with eight exact→unavailable losses and no gains; 32 unavailable, zero wrong-exact/protocol within this matrix. Partial reference losses and increased proof gaps are preserved. This is a soundness-gate coverage cost, not a performance or readiness success. Independent review is separate. See `results/recovered-78fd-vs-858f-coverage.md`, both `recovered-78fd-vs-858f-*-deltas.json`, raw `recovered-78fd-prepared-{100,grace46}.json`, and `recovered-78fd-grace-misses.json`. No timing acceptance.

## Nominal lookup candidate 3fb2

Unchanged main100 remains100/100 and fixed Grace46 remains14/46 (10definitions+4complete references), with0wrong-exact/protocol in these matrices. Three original UsageObservation value.Scope.RepositoryId sites, outside the fixed46, improve from0/3 to3/3 exact against fresh directFCS oracle. No document notifications/overlays were sent. Those remove three proof gaps from each of two fixed RepositoryId searches (13→10 and26→23), but their partial location counts stay4 and2 and neither becomes complete. Genuine09f011/f215 contexts and source hashes remain unchanged. No performance claim. See `results/recovered-3fb2-vs-78fd-coverage.md` and linked raw/delta reports.

## Reference-domain candidate 710f

Unchanged main100 regresses100→98/100: local_parameter_references and outer_shadow_references retain their same two locations but become unavailable under the broader possible-signature guard. Grace stays14/46 with identical answers/partialsets/gaps; the three unscored original no-overlay nominal chains remain3/3 FCS-matching exact. No wrong-exact/protocol in these matrices, no performance claim. See `results/recovered-710f-vs-3fb2-coverage.md` and exact raw/delta reports.
