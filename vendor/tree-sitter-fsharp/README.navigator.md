# Local F# grammar correction

This directory vendors tree-sitter-fsharp0.3.13 (MIT), upstream commit12ed86cfef3ed8c7106a6e599dafe5de911d01b3. The published crate SHA and each retained input/output hash are recorded in provenance.json. Cargo's root patch selects this directory. No registry cache was edited.

The local patch adds the F# prefix-caret IndexFromEnd syntax missing from the published grammar. It preserves three distinctions: a tight prefix operand rather than surrounding infix arithmetic; an ordinary infix caret when a left operand exists; and SRTP type-argument/static-constraint syntax. Runtime conflict precedence selects infix over a prefix-argument interpretation. The scanner is unchanged. The AST records syntax only; invalid standalone/double-caret source is not claimed to be valid F# or a supported semantic navigation case.

Regenerate with tree-sitter-cli0.27.2 installed using `cargo install tree-sitter-cli --version 0.27.2 --locked`, then run `tree-sitter generate --abi 15 --js-runtime native` from each of fsharp and fsharp_signature. The authored tree-sitter.json supplies the upstream0.3.13 grammar metadata omitted from the published crate. QuickJS is bundled in that exact generator; no npm install is needed. The generated signature parser remains identical to upstream.

Both generated parser.c files and headers are required source inputs and must remain in checkpoints. The implementation parser exceeds the normal source-pack20MiB limit; the packager permits only its exact reviewed path/size/SHA. Any regeneration requires refreshing provenance and that explicit allowance. The native build fingerprint includes vendor inputs, so changed grammar or scanner bytes invalidate cached summaries.

Evidence and actual compiler/CST controls: evaluation/experiments/reverse-index. Primary source: pinned F# Compiler pars.fsy minusExpr (IndexFromEnd), plus FSharp.Core UnitTests ListModule reverse-slicing and Array2Module multidimensional-slicing tests. Generator documentation: https://tree-sitter.github.io/tree-sitter/cli/generate.html .
