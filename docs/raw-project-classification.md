# Raw project membership classification

This bounded checkpoint follows the native observations at
`a9febeb53f48ac75318a79be0b5d15ce2fc4e14e`: both an excluded `Compile` item and
Compile-shaped XML in `ProjectExtensions` entered the raw file inventory with
project-level `exact: true`. The overall tree response remained approximate.
Those observations were membership/certainty evidence, not wrong editor jumps.

The native preview now distinguishes ordinary `Project → ItemGroup → item`
ancestry from unsupported contexts. Literal inputs in unsupported contexts stay
tentative. `Exclude`, `Remove` and `Update` on `Compile` or `ProjectReference`
refuse certainty; no wildcard, exclusion or build evaluation is implemented.

A recognized direct `ProjectExtensions` subtree is skipped completely. Its
source-shaped metadata, references, defines and other properties cannot supply
project facts. Neighboring ordinary inputs retain their order, Link metadata
and existing proof eligibility. This differs from uncertainty about an excluded
item: recognized extension metadata is inert, not a possible compile directive.

Only unprefixed elements in an ordinary empty or standard MSBuild default
namespace are eligible for certification. Prefix use, foreign namespaces and
nested namespace switches are conservative unsupported contexts. Unused prefix
declarations alone do not demote an ordinary project. The stack retains a small
namespace enum, has a 128-level limit, and rejects unclosed XML and malformed
attributes. This is bounded classification, not a complete XML/MSBuild evaluator.

Native controls cover ordinary items, default namespaces, ignored metadata,
ordered links, target/conditional/other ancestry, excluded project references,
malformed/truncated XML and excessive nesting. Definition controls distinguish
an ordinary preceding signature, uncertain excluded/target-only signatures and
a metadata-only signature. They also require stale implementation targets to
disappear after an unsaved rename and preserve an independent exact local lambda.

The authored editor cases distinguish:

- Excluded signature: no automatic jump; both physical declarations remain
  labelled incomplete choices; deliberate selection reaches the implementation.
- Metadata-only signature: exact implementation navigation, with the signature
  range forbidden. The remaining ordered project prefix supplies this proof.

An intermediate native assertion incorrectly required the metadata case to be
incomplete; it rejected the correct implementation jump. Independent oracle
review approved correcting that expectation from documented ignored-subtree
semantics. The failed attempt remains private; it is not a runtime fix claim.

Sources: [MSBuild item attributes](https://learn.microsoft.com/en-us/visualstudio/msbuild/item-element-msbuild),
[ProjectExtensions](https://learn.microsoft.com/en-us/visualstudio/msbuild/projectextensions-element-msbuild),
and [F# signature files](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/signature-files).

The scoped signature-hidden-consumer XFAIL remains separate. No implicit
MSBuild/compiler invocation, launch-policy change, References fix, performance
claim or normal-sandbox Ubuntu acceptance follows from this checkpoint.

## Validation receipt

Protected aggregate checks passed: 3 Python, 51 Node tooling/manifest, 328 Rust
core (5 ignored), 4 daemon and 115 adapter/transport tests, plus TypeScript and
formatting. The overlay lifecycle repair and its tests remain unchanged.

A fresh private Linux x64 package passed pinned VS Code 1.108.0: **33 required
scenarios plus the scoped signature XFAIL**, or 35 passing phases and one XFAIL
including the two existing dirty-edit baselines. All prior 31 required cases
and fixture objects are unchanged. Both new cases received trusted keyboard F12.
The excluded-signature case visibly offered the exact authored pair as incomplete
choices, made no forced jump and selected `Wire.fs` at full range `1:4–10` only
on deliberate selection. The metadata-only case jumped to `Wire.fs` caret `1:4`,
with native full range `1:4–10` and no signature choice. All 20 sessions reported
zero compiler/helper/decompiler starts; fixture and harness inputs stayed fixed.

The final native probe repeated the original matched inventory controls:
ordinary A/B stayed project-exact; excluded A/B stayed inventoried but inexact;
metadata-only A disappeared from project membership while ordinary B remained
project-exact; the B-only control was unchanged. All tree envelopes remained
approximate. These checks do not certify arbitrary build evaluation.

Private evaluation identities (not downloadable/public release artifacts):

- VSIX SHA-256: `d14a20e4c54a71b6592bfa8d0aacff6eea775338467f6f32eb58dd4f4bdca409`
- Native SHA-256: `0e914d29479b838edf6cdcc1b2bc9da430a9dab2c66aae328f6c561ba979a1b4`
- Editor SHA-256: `6d3e558f9d17bf273e8fa0bd11c9988052696dcc18b3e8f4c020feff8e6f0e8b`

Independent reviews checked XML scope/namespace boundaries, bounded retained
state and the authored rendered/editor oracles. The editor route remains the
controlled disposable harness, not normal-sandbox Ubuntu acceptance.
