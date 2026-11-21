# Independent CLI profile diagnostics

The protected task-local CLI passed the independent diagnostic suite. This is
not a Nix expression, shell, package-build or editor-host execution result from this review.

The review confirmed that `devenv.nix` selects the same historical SDK 10.0.400 /
runtime 10.0.11 derivation for both the package set and the helper launch path.
The vendored manifest and license match their recorded hashes; the retained
official archive-verification report contains 24 matching entries. Native/helper
acceptance profiles, editor defaults, permissions and Safe Chain controls were
not changed by this review.

Actual task-local `doctor` reports SDK 10.0.400 and runtime 10.0.11. Setting only
`FN_DEVENV=1` with those same task-local tools exercises the Nix warning branch;
the report explicitly labels this simulation. It does not establish a Nix
runtime or supported preparation profile.

A real immutable helper 09f011 preparation attempt inherited a deliberately
nonempty `LOCPATH`. It rejected the context, saved the full project diagnostic
report, and produced no editor settings or binary metadata. Preparation metrics
contain only `workspace/evaluate`, proving that no catalog/environment export
was reached. The first independent run found that the CLI truncated its output
to generic NuGet-property diagnostics and hid the actual locale failure. The CLI
owner changed diagnostic prioritization only. The exact rerun then passed,
including the assertion that `LOCPATH` precedes generic follow-on diagnostics.

JavaScript syntax checks, TypeScript typecheck/compilation and all 23 frontend
transport/virtual-document/adapter-contract tests also passed. The subsequent
immutable 78fd held-source normal CLI check passed 226 core tests, three daemon
tests and 23 frontend tests, with five fixture-dependent Rust tests explicitly
ignored. Exact provenance is in
`test-results/verification/78fd-required-checks.json`. This is separate from the
independent SRTP/corpus checks and does not accept the globally rejected 858f
semantic candidate.

Reproduction:

```bash
scripts/with-local-tools python3 semantic/tests/cli_profile_diagnostics.py \
  --helper evaluation/.runtime/helper-09f01156157a/FSharpNavigator.Semantic.dll \
  --fixture test-results/resolution-states/v4-models-09f011 \
  --output-dir test-results/verification/new-cli-profile-run
```

Evidence is `test-results/verification/nix-cli-independent-prioritized/report.json`
and its sibling log. The original failed text assertion and report remain under
`test-results/verification/nix-cli-independent`; that failure was a diagnostic
visibility issue, not an unsafe acceptance. The reports include source hashes,
exact commands, process outputs and what this suite did not execute.

Subsequent work by the Nix audit owner successfully ran Nix 2.31.2 and evaluated
the pinned SDK 400 derivation through the official rootless canonical-store route.
That independent result is recorded in
`docs/review/nix-portability-audit/rootless/pinned-derivation-probe.json`; follow
the latest Nix audit for realization progress. It does not change this suite
into a Nix execution test or establish a supported Nix preparation profile.
