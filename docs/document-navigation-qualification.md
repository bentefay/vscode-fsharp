# Document navigation qualification — 2026-11-26

This change fixes same-file nominal type definitions and ordinary lexical
definitions that previously depended on project/index readiness. It also fixes
complete callback references when a multi-part property receiver appears in the
body. It does not establish complete F# navigation.

The motivating `SagaId` annotation in the original private workspace is a type
declared in a preceding local module, made visible by a local `open`. The resolver
now follows source-order type declarations, nested modules, module aliases,
ordered opens and generic arity. It uses the current acknowledged document and
the native tree-sitter parser; it does not ask FCS or a metadata helper to fill in
missing indexed results.

Unknown external opens invalidate visible evidence. Unsupported declaration
heads, escaped type shadows, attributed nested modules and recursive modules
withhold the nominal proof. Subsequent known local declarations can establish
fresh visibility after an unknown open. This is a scope/import resolver for
supported same-document type positions, not spelling uniqueness promoted to
certainty. Existing source and cross-file candidate search/ranking remain
separate; this change does not extend cross-file nominal proof.

Ordinary local values and public/private function or lambda parameters gain
definition support, including return annotations and nominal/generic/tuple/array
parameter annotations. When a whole expression cannot be traversed, definitions
can follow a selected lexical path through supported expressions and match
subjects or simple/wildcard arms. Unknown intervening binders, field labels,
named arguments and parser-recovered inline rule separators cannot establish an
outer binding. Public parameters and partial traversals never seed a complete
reference set. References still require a complete supported local domain.

## Authored coverage

The [32-case matrix](../extension/test/e2e/document-types.json) supplies literal
targets and complete reference sets independently of engine output. The same
matrix runs through the real native adapter and actual packaged-editor gestures.

| Family | Cases | Boundary exercised |
| --- | ---: | --- |
| Nominal type definitions | 14 | Alias, record, union, class, generic arity, nested module, qualification, module alias, ordered local opens, source-order shadowing, type alias and field annotation |
| Lexical definitions | 13 | Public/direct/returned lambda formal, local let, private and return-annotated formal, generic/tuple/array annotation, match subject/wildcard body, shadowing initializer, selected callback path, multiline callback |
| Complete callback references | 5 | Conditional branches, conditional subject use, sequential expressions, multiline call, multi-part property receiver |

Each native matrix case checks cold and warm requests, a dirty line shift,
restart retaining the dirty buffer, and close/reopen restoring disk text, with
model/catalog/builtin/helper paths deliberately absent. Separate real-native
controls test escaped type shadowing (direct and opened), inline match shadowing,
and Definition-then-References for a public single-use parameter, exported lambda
and partial match traversal. Cache controls run both includeDeclaration modes
after cold/warm definitions, dirty edits and dirty restarts. Unsupported
references must return no normal provider result, not a fabricated exact set.

Rust tests additionally preserve unknown patterns, recursion, named arguments,
field-label selection, escaped shadows, arity mismatch, external opens, private
type exports, directives, syntax errors and the existing bounded-work guards.
These refusal checks are safety coverage, not successful navigation coverage.

## Final validation

All commands use `cli/bin/run` under pinned Nix/devenv, locked dependencies,
minimum package age and Safe Chain. No normal-profile installation or publication
was performed. UI runs were sequential, used a disposable profile and retained
the Chromium sandbox. No precomputed semantic model or catalog was prepared.

| Gate | Result | Private local evidence |
| --- | --- | --- |
| `check` | Pass: 364 core tests, 4 daemon tests; 6 core tests ignored; Python/manifest/TypeScript/format/adapter-contract checks passed, including 132 final JS tests | `.cache/navigation-final-check.log` |
| `test adapter` | 60/60 pass with real native processes; includes the 32 matrix cases | `.cache/navigation-final-adapter.log` |
| New packaged-editor matrix | 32/32 pass: 27 F12 definitions and 5 References sidebar sets | `.cache/navigation-final-document-ui/report.json` |
| Existing packaged-editor suite | 36 passing phases and 1 scoped signature-hidden known failure; 35 scenarios plus 2 edit baselines | `.cache/navigation-final-existing-ui/report.json` |
| Original private-workspace audit | 21/21 pass: 9 type definitions, 4 callback definitions and their 4 complete reference sets, 4 formal definitions; 1,520 source/project input hashes unchanged | `.cache/navigation-final-cosmos-ui/report.json` |
| Pinned Grace/FsCheck/compiler corpus | All required gates pass, all three inputs present and unchanged; 32 definition protocol observations and 32 reference obligations (28 exact sets, 4 stale rejections) | `.cache/navigation-final-corpus.json` |
| Separate strict qualified-union regression | **Fail**, exit 1: 0/1; two labelled candidates include the forbidden homonym, no exact location; original oracle preserved | `.cache/navigation-final-union-ui/report.json` |

The corpus retains eight historical compiler probes separately: two contain the
known target as labelled candidates; six still fail target recall. None is added
to the required-pass count. The tuple probe overlaps a required site and is not
an extra independent success. No corpus input is blocked in this run.

The private-workspace audit uses the original read-only source root, not copied
generic fixtures. All six prior failures now pass: the multiline callback's
definition and reference set, two private formals, the match subject formal and
the wildcard-arm outer formal. The original `SagaId` annotation jumps to its
authored declaration at zero-based line 22, columns 9–15. This is a measured
21-site audit, not a claim that every navigation action in that workspace works.

The six ignored core tests remain unexecuted: one conditional-origin tuple
candidate gap and five tests requiring genuine metadata/XML/builtin or optional
decompiler artifacts. They are not counted as passes. The prepared semantic
`test ui` route remains unsupported on this toolchain; permitted unprepared
`test e2e` was used. Windows, macOS, normal-profile behavior and general rendered
hover correctness were not qualified by this change. These are correctness
checks, not startup/query benchmarks or full-process-tree memory measurements;
bounded caches and input guards are not a hard native RSS limit.

Versions actually recorded for this final run: Code 1.147.0, Rust/Cargo 1.100.0,
Node 24.15.0, npm 11.12.1, Safe Chain 1.5.10, SDK 10.0.404 and .NET runtime
10.0.15. See `.cache/navigation-final-versions.log`. Indexed navigation did not
launch a compiler, helper or decompiler.

Final package: `.cache/navigation-final.vsix`, SHA-256
`ea025e05ebd99c7be1e50345089c003982a93708ab9ad3161c47c3c45e4dd029`.
Bundled native SHA-256:
`2a65b293b9142196f9cdab53675e38bbebc7a24d58452ad7a9571cedbc679855`.
UI reports verify installed/package/native hashes and unchanged inputs; corpus
reports independently hash the binary and source inventories.

## Preserved failures and remaining work

Before-fix evidence stays private: `.cache/cosmos-saga-before` has the original
correct `SagaId` cursor; `.cache/cosmos-before` used an incorrect cursor and must
not be used as that baseline. `.cache/cosmos-audit-before` records 15/21 passes
on the earlier package. `.cache/document-bindings-before.log`,
`.cache/document-annotated-before.log`, `.cache/document-multiline-before.log`,
`.cache/navigation-cache-regression-before.log`,
`.cache/navigation-review-before.log`, `.cache/navigation-tuple-before.log` and
`.cache/navigation-array-before.log` preserve the relevant failing regressions.
The review regressions demonstrated wrong outer exact targets for escaped type
shadowing and inline match arms; both now withhold that proof.

The nominal route is limited to syntactic type positions and ordinary ASCII
names. Constructor expressions, cross-file nominal identity, members, overloads,
generic inference, computation expressions, recursive scope groups and arbitrary
pattern binders need further work. Complete public-type/public-function reference
domains are not established by a useful local definition. Existing search may
still produce labelled incomplete candidates in unsupported cases; the editor
currently exposes that offer through its status command while F12 itself returns
no location. This is a discoverability limitation, not evidence that no spelling
search exists.

The signature-hidden candidate issue and strict
[qualified-union regression](../extension/test/e2e/union-definition-regression.json)
remain separate obligations. Their original full-range oracles are unchanged.
Further cross-file type resolution should reuse established project membership,
compile order and import facts; uncertain external shadowing must remain labelled.

The source-root editor harness accepts an explicit `FSHARP_E2E_SOURCE_ROOT`,
rejects edit/freshness cases, verifies authored source snapshots, disables
automatic tasks and hashes tracked/untracked F# and project inputs before and
after. Private business source and snapshots are never committed here.
