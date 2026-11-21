# Current development status

> Recovery note: this report records the checkpoint 11 state before later
> correctness findings. See [current-status.md](current-status.md) for the
> current source identity, known defects and validation limits.

This is source work in progress, not a release. **Reviewed candidate3fb2 is
blocked by incomplete exact parameter reference sets.** Nine same-name paired
signature checks omit a declaration, and eight method/constructor body checks per
declaration flag omit caller named-argument labels. Actual FCS establishes those
missing locations; FSAC's own named-label omission does not permit a complete
claim. Twelve paired definition-target preferences are tracked separately and
are not mislabeled as selection of an unrelated symbol. Renamed signature/body
parameters retain separate reference groups despite paired navigation.

Repair710f/source0c892 is captured with232 core and3 daemon tests passing; initial fixed reports are98/100 main and14/46 Grace. Independent acceptance
remains pending; no repair acceptance is claimed.
See [current-goal-score.md](current-goal-score.md), [recovery-status.md](recovery-status.md)
and `docs/current-candidate-status.json` for exact evidence and historical status.

The3fb2 nominal-lookup slice passes scoped controls and adds three real Grace
receiver-chain answers. Main100 remains100/100; the fixed Grace sample remains
14/46 (10 definitions and4 complete reference sets), with32 unavailable. This does
not restore the eight answers lost under the earlier factory-safety restriction.
An isolated document-local parameter module and review are preserved, but it is
not integrated, measured or accepted as a production fast path. Actual adapter
and native local-acceptance suites remain future execution gates.

The real Nix SDK proof evaluates and realizes the pinned SDK400/runtime11 and
restores/builds a tiny F# project. The unchanged helper rejects its unreviewed
glibc profile; separate profile research does not widen acceptance. Full devenv,
real editor execution, all-code parity and monolith performance remain unmet.
Earlier digest performance is retained as component evidence only. Delivered
0.1.0 artifacts remain separate from this newer development work.
