# Pending signature guard repair — unvalidated

This patch preserves an unfinished change to `crates/navigator-core/src/lib.rs` and its tests.
The production file was restored to Git HEAD after capturing this patch.

- Patch: `pending-signature-guard.patch`
- SHA-256: `9693d1604341b300c4b98153255d9806697415984c59a70933f122868f368e45`
- Diff: **48 additions, 8 deletions**, one file.
- Formatting: **NOT RUN**.
- Semantic/native tests: **NOT RUN**.
- Mechanical verification: `git apply --check docs/handoff/pending-signature-guard.patch` passed against the restored production file.

## Intended behavior

`possible_signature_context` positively recognizes only `.fs` and `.fsx` input
extensions, with ASCII case-insensitive matching. Any `.fsi`, mixed-case
signature suffix, legacy/unknown suffix, missing extension or non-ASCII variant
blocks proof that the selected project has no signature context. This shared
guard also applies to factory proofs.

The existing parameter reference-domain test is extended with `.fsi`, `.FSI`
and `.FsI` project inputs. A new test covers `.FS`/`.FsX` positives and unknown,
legacy, non-ASCII, extensionless and inexact-context negatives. These test
expectations have not been executed and must not be reported as passing.

## Resume

Apply the patch, format the code and run the two focused tests:

- `parameter_reference_domains_keep_local_sets_but_refuse_possible_signature_exports`
- `signature_absence_requires_only_recognized_implementation_inputs`

Then independently verify actual compiler admission and paired parameter
references for mixed-case signature filenames, followed by the unchanged
100-query synthetic and 46-query Grace gates. Use the protected CLI/toolchain
workflow documented in `AGENTS.md`. Do not accept this patch merely because it
applies cleanly.

The named-argument role repair remains design only; this patch does not fix it.
No `syntax.rs` or `treesitter.rs` changes were made by this agent.
