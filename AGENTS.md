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
