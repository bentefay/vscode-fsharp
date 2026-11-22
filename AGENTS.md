# Working on F# Navigator

Use `cli/bin/run` for setup, build, check, integration/UI tests, verification,
packaging and installation. Nix/devenv and Safe Chain follow the reference
Git Tree Compare repository. Keep dependency/runtime lockfiles committed.
Do not bypass Safe Chain or minimum package age; disable dependency lifecycle
scripts and run explicit reviewed build commands.

Rust owns native indexing, parsing, lookup, persistence and daemon transport.
TypeScript is an editor adapter. .NET/FCS is an explicit semantic experiment or
one-shot project/metadata helper, never an implicit indexed-mode fallback.
Normal providers expose confirmed results. Approximate or incomplete results
must be labelled and must not appear to be a complete empty reference set.

The engine uses bounded caches and input guards; do not describe them as a hard
native RSS limit. Measure full process trees and distinguish baseline startup,
query timing, source/index preparation and incomplete operations.

Run real editor tests only in disposable profiles. Package and source commands
are local. Installation/publication requires the user's corresponding request;
test installation into the disposable harness is part of verification. Never
publish this prototype to Marketplace or an upstream repository implicitly.

Use argument arrays for process execution. Preserve existing changes. Keep
compiler/runtime/editor version evidence with measured results and state any
untested platform or unexecuted validation step explicitly.

## Repository workflow

The user requests incremental commits directly on `main` in
`bentefay/vscode-fsharp`, using the connected `bentefay-agent` account rather than
the main `bentefay` account. Preserve remote changes and never force-push. Commit
completed implementation work to the repository rather than leaving it only in
temporary workspaces or source checkpoints. This authorization does not publish
Marketplace releases or install into the user's normal editor profile.

## Navigation regression gate

For a navigation behavior change, first preserve a failing test with an authored
target/range or complete reference oracle, then run the protected aggregate and
real-native adapter checks. Run `cli/bin/run test corpus` against the pinned
Grace, FsCheck and compiler source inputs before publishing the change; see
`evaluation/engine-corpus/README.md`. Record required passes, failures, known
failures and blocked inputs separately. Missing corpus inputs do not pass;
report any tested subset and its blockers explicitly before publication.

Keep useful exact or labelled candidate results distinct from safe refusal.
Check discovery, ready, dirty and restart states rather than assuming index
readiness proves project context or navigation availability. A fixed small
regression belongs in the normal checks even when the full corpus is unavailable.
Engine checks do not establish keyboard, picker, references-panel or rendered
hover behavior; those need their separate permitted packaged-editor gate.
