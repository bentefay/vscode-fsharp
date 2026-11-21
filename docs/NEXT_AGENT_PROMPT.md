# Prompt for the next agent

Continue my Rust-first F# VS Code extension in
https://github.com/bentefay/vscode-fsharp. Start by fetching `main` and reading
`AGENTS.md`, `docs/HANDOVER.md`, `docs/current-status.md` and
`docs/handoff-toolchain.md`. The full prototype was imported in `efda6f58`;
the handover artifacts follow it. Preserve newer remote changes.

I want only these Ionide features: accurate go-to-definition and complete find
references across code; type/signature/documentation hover only for external
library symbols, suppressing workspace-owned symbols; and the ordered
solution/project explorer. The central goal is orders-of-magnitude lower memory
and much faster navigation than Ionide/Rider, with negligible startup. Prefer
on-demand work; background indexing is acceptable if it takes seconds. Use Rust
as much as possible, a thin editor adapter, and the repository's CLI/Nix/Safe
Chain/test conventions. Indexed navigation must never implicitly start FCS or
load the whole solution when a fast proof fails.

Act as an orchestrator: delegate implementation and research, keep an independent
critic/scorer, and iterate on actual failures. Do not stop after a plan or call
safe refusals feature equivalence. I explicitly authorize incremental commits
directly to `main` in this public repository using the connected `bentefay-agent`
account, not my main `bentefay` account. Preserve remote work; never force-push.
No Marketplace publication or installation into my normal editor profile is
authorized.

The current implementation is incomplete and has known correctness defects.
Its recorded results are 98/100 synthetic queries and 14/46 Grace operations;
these are historical, not freshly rerun. Full native/editor checks still need
fresh execution. The development toolchain was restored and protected setup/
doctor passed; ignored tools and dependencies are absent in a fresh clone.

First establish a fresh baseline, then validate the unapplied
`docs/handoff/pending-signature-guard.patch`. It closes the `.FSI`/`.FsI`
signature-absence hole but has not been formatted or tested. Next repair named
argument roles: in `C.M(x=x)` the left `x` can belong to the callee, while in
ordinary `g(x=x)` both can be caller locals. The parser/binder repair is design
only; preserve optional/escaped names, equality positives, dirty edits and
reference completeness.

Then integrate the staged document-local proof and bounded positive-answer cache
described in `docs/handoff/local-fastpath-plan.md`. No such integration/cache is
currently implemented. Prove that supported local queries require no project,
catalog or inventory work; measure cold, warm, edit and restart behavior.

Use unchanged fixed corpora, especially `evaluation/contracts/grace46.json`, and
keep unsupported cases in the denominator. Use genuine project/compiler inputs,
immutable candidates and independent adversarial controls. A wrong exact answer
blocks acceptance. Report preparation separately and compare matched correct
operations with consistent native/whole-editor memory scopes. Do not reuse
unrecovered historical timings as current results or claim Ionide/Rider ratios
without measuring them. Continue committing validated improvements and updating
the handover/status evidence as you work.
