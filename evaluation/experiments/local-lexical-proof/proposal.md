# Document-local parameter navigation before project loading

Status: isolated implementation staged under `implementation/`; no production integration or performance improvement is claimed. The initial raw FCS harness omitted signature diagnostics: its exact bytes are preserved as `oracle-implementation-diagnostics-only.json`. The corrected oracle includes project diagnostics and retains invalid cases. Actual pinned FSAC additionally changes the definition boundary below.

The useful initial boundary is a strong ordinary parameter binding plus a complete account of identifier roles in its body. This can establish the identity of a parameter without knowing the type of its receiver or any imported library. `range.OrdinalStart` needs semantic analysis to identify `OrdinalStart`; it does not need that analysis to identify the ordinary lambda parameter `range`.

## Why definitions and complete references need different eligibility

Raw FCS `GetDeclarationLocation` returns the implementation parameter for `let f x=x+x`, but actual pinned FSAC routes this to the paired signature parameter, including differently named signature parameters. For same-name formals, project references also include the signature declaration; renamed signature/implementation formals retain separate reference groups even though FSAC pairs their definition destinations. The same definition correlation occurs for direct exported lambda RHS and direct curried returned lambda chains. **Both** document-only operations must fall through for these categories; raw FCS declaration preference is not Ionide parity.

For both operations in the first envelope, admit simple lambda parameters used as arguments to ordinary applications and parameters of ordinary let-functions syntactically nested in a function/lambda body. Exclude module-level let parameters and immediate RHS lambda chains that can contribute public curried argument metadata. Runtime escape is not the test: actual valid controls show a lambda passed to `id`, and a nested function returned as a computed function value, retain their own local parameter identity. Invalid unparenthesized signature variants are retained as arity-boundary evidence, not positive coverage.

Primary compiler source at checkout `f896797a0f9b238a8cb1c482be424b714037ddc5` supports these distinctions:

- `src/Compiler/pars.fsy`, atomicPatternLongIdent near 3870: a single non-uppercase identifier creates `SynPat.Named`; qualified/uppercase identifiers take a separate ambiguity path.
- `src/Compiler/Checking/CheckPatterns.fs`, `TcSimplePat` and the `SynPat.Named` branch: ordinary parameter binding is lexical.
- `src/Compiler/SyntaxTree/SyntaxTreeOps.fs`, `InferLambdaArgs` near 700 and `InferSynValData`: public syntactic argument metadata incorporates an immediate RHS lambda chain, rather than arbitrary nested/computed lambdas.

These sources explain the tested boundary; they are not a substitute for the integration and adversarial gates below.

## Initial admitted syntax

Use only `.fs` or `.fsx` text from an acknowledged editor overlay with the exact requested version. Parse that source transiently with the pinned grammar. Reject all preprocessor/directive-bearing files initially, any root parse error/missing node, cancellation, resource-budget exhaustion, or transformed/projected source. No disk or project options may silently substitute for the acknowledged text.

A declaration is a simple ASCII lower-case ordinary `let` function formal or `fun` formal, optionally carrying a positively recognized plain type annotation. No methods, constructor/self parameters, optional arguments, active/union/record/alternative patterns, escaped/operator formals, attributes, recursive binding groups, or unknown header forms. Reject duplicate formal names. Broader identifiers can follow after their own normalization and scope tests.

The body is visited through an explicit admitted expression grammar: ordinary value identifiers, receiver chains, parenthesized/tuple values, ordinary positional applications, constants, and simple nested lexical lets/lambdas with exact visibility. Initially refuse CE/custom operations, matches/complex patterns, object expressions, quotations, interpolations, dynamic lookup, casts/type applications, record puns/initializers, and every unrecognized form. This is a closed syntax walk, not an absence-of-known-dangerous-nodes heuristic.

For each matching identifier, preserve its role. A receiver `x` in `x.Member` is a value use; `Member` is not. The left `x` in `C.M(x=x)` can be a named method argument, while both xs in `g(x=x)` can be ordinary values. The compiler controls demonstrate that distinction, so any potentially named argument equality involving the queried name must refuse initial complete-reference certification unless independently disambiguated. Do not reuse `local_binding` alone as a completeness certificate. Type positions, declaration names and record labels are different roles, not automatic same-name references.

For definitions, first enforce the same non-correlated local category guard, then resolve the actual queried value-use role to the nearest admitted declaration. For references, traverse the entire declaration body and all admitted nested scopes, accounting for shadowing and every matching token, then optionally add the parameter declaration. No reference can originate outside the admitted local identity domain. The exported/signature-correlated declaration categories are excluded before this claim.

## Proposed integration contract

```rust
pub struct Snapshot<'a> {
    pub uri: &'a str,
    pub source: &'a str,
    pub version: i64,
    pub source_hash: &'a str,
}
pub enum LocalOperation { Definition, References { include_declaration: bool } }
pub fn query(
    snapshot: Snapshot<'_>, position: Position, operation: LocalOperation,
    cancellation: &CancellationToken,
) -> anyhow::Result<Option<LocalAnswer>>;
```

`None` means normal navigation must continue; typed cancellation propagates. `LocalAnswer` contains original-source UTF-16 locations, declaration/body byte identity, source hash/version, parser fingerprint and a document-local proof tag. It must not manufacture a validated `projectContextId`. Use a separate `proofScope: document` / `source: local-lexical` response identity. The main dispatcher validates the acknowledged version before calling this module, and rechecks cancellation/version before returning.

The entry belongs before `ensure_model()` and `drain_events()` in `Workspace::navigation`. Current navigation runs those before even checking the overlay version or local binding, which forces project work for otherwise local queries. `document/change` already keeps an acknowledged overlay, hashes source and defers its AST in indexed mode. Preserve the existing current-buffer acknowledgement and stale-response protection in the extension. The first implementation can retain its current dirty-dependency flush; report that cost rather than pretending it vanished.

Use a separate `local_lexical.rs` with a transient CST and explicit role walk, not the general member/type inference pipeline. Cache only compact per-source proofs/ranges in a bounded session cache keyed by URI, acknowledged version, source hash, parser/proof version and declaration offset; never retain all workspace trees. On edits/close/session restart those identities cannot be reused accidentally. Any fallback continues the existing model-aware path unchanged.

## Real source applicability

Both full original files below have no directive lines and their pinned-grammar CST has no ERROR or missing nodes. The preserved `real-source-facts.jsonl` and `real-source-ast.txt` are read-only syntax observations, not an implementation of the proposed proof.

| Real site (zero-based) | Scope | Expected local domain |
| --- | --- | --- |
| Grace `ContentBlockMetadata.Types.fs:356`, `Array.sortBy (fun range -> ...)` | Application-argument lambda | One declaration at column 37 and three receiver uses at 46, 66 and 88 |
| Fable `Platform.fs:90`, `Array.filter (fun x -> x.EndsWith(extension))` | Application-argument lambda | One declaration and one receiver use |
| Fable `Platform.fs:91`, `Array.map (fun x -> x.Replace(...))` | Application-argument lambda | One declaration and one receiver use, distinct from the prior lambda |
| Fable `Platform.fs:73`, `normalizePath (path:string)` | Exported let-function parameter | Both operations fall through without signature context |

`real-sites.json` retains exact file hashes, source text and UTF-16 positions. The Grace file is 26,380 bytes and Fable file 4,500 bytes. Actual FCS controls use the exact lambda/function bodies with minimal declared surrounding types; this does not claim a full-source compiler replay for either project.

## Acceptance before any performance claim

1. Reproduce exact definition and complete reference sets from the valid compiler controls, including nested shadowing and UTF-16. Preserve invalid signature examples separately.
2. Preserve actual FSAC's paired-signature grouping and signature-first definition preference. Independent tester evidence is `evaluation/experiments/local-lexical-proof/fsac-parameter-controls-valid/report.json`; integration must use those targets rather than raw FCS implementation anchors.
3. Test true cold `workspace/open` + `document/open` + query with project discovery/catalog/inventory paths deliberately unavailable, then assert exact local answers and zero model validation, inventory loads, compiler/decompiler starts and workspace file scans.
4. Mutate the selected parameter, its use, nested shadowing, direct/exported lambda form, named argument shape, parse validity and directives. Verify immediate correct fallback or updated locations; cancellation and stale versions must never publish cached exact results.
5. Query both linked-source contexts with the same directive-free overlay and exercise edit/reopen/session reset. The proof identity is source-local, not a stale project certificate.
6. Measure cold first useful query, warm query, edit-and-query, all-process peak/steady RSS, parse bytes and cache footprint on the original Grace/Fable files. Compare the same useful operations to the normal path and real FSAC; do not time an unavailable answer as a win.

The raw oracle includes deliberately excluded but compiler-valid forms. Those cases motivate guard boundaries; they are not unsupported-language claims or reasons to poison unrelated global navigation.
