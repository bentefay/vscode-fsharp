# Quiet navigation feedback — 26 November 2026

Routine incomplete/indexing navigation now uses status-bar feedback. Clicking an
incomplete status opens **Show Details** and, for a current incomplete reference
request, **Show Possible References**. That search remains an explicit, labelled
incomplete same-name search. Possible definitions retain their separate labelled
status launcher and explicit command. Native reference providers still withhold
incomplete sets; the status tooltip explains why the editor's empty-result wording
does not establish that there are no uses.

Status actions retain request, editor, caret, document-version, project, epoch and
cancellation checks. A new explicit query clears the preceding status action;
automatic hover cannot replace an existing uncertainty explanation. Identical
reference retries use the newest cancellation token, including declaration-mode
retries. Menus are bounded to one open instance. Genuine errors keep diagnostic
output and a **Show Details** error notification, deduplicated until successful
explicit navigation. Explicit possible-reference operations retain their
cancellable progress indicator; those are not routine navigation warnings.

The final package is
`.cache/quiet-navigation-20261126/final-v2/navigator.vsix` (4,384,264 bytes).
Its SHA-256 is
`5ba1bf02c818977a14eddd20cdd54e784c7b0ca62835ab39bf299d42b8645720`.
Its native SHA-256 is
`2a65b293b9142196f9cdab53675e38bbebc7a24d58452ad7a9571cedbc679855`.
The packaged adapter JavaScript matches the final compiled source.

All build, check, packaging and editor gates ran through `cli/bin/run`. Editor
runs were sequential, retained the Chromium sandbox and installed only into
disposable profiles. No normal-profile installation or push was performed by
this worker; the coordinator owns those requested final actions.

| Final gate | Result | Local evidence |
| --- | --- | --- |
| Protected `check` | Pass: 138 JavaScript tests, 364 core tests, 4 daemon tests, plus Python/manifest/TypeScript/format checks; 6 core tests ignored | `.cache/quiet-ux-check-final-v2.log` |
| Real-native `test adapter` | 60/60 pass | `.cache/quiet-ux-native-final.log` |
| Existing packaged editor suite | 36 passing phases, 1 scoped signature-hidden expected failure; 35 scenarios plus 2 edit baselines | `.cache/quiet-navigation-20261126/final-existing-ui/report.json` |
| Packaged document-navigation matrix | 32/32 pass | `.cache/quiet-navigation-20261126/final-document-ui/report.json` |
| Original Cosmos audit | 21/21 pass; all 1,520 source/project input hashes unchanged | `.cache/quiet-navigation-20261126/final-cosmos-ui/report.json` |

The adapter assertions cover repeated incomplete/indexing requests with zero
warning/info/error calls, click-accessible details, explicit action labels,
stale/new-query clearing, cancellation, includeDeclaration retries, hover
interleaving and actionable scripted transport errors. Existing reference
safety tests were adapted to the real status-command/menu interaction rather
than deleted. The editor gate observes the rendered uncertainty tooltip, actual
status/menu clicks, the visible output panel, labelled possible-reference rows,
and post-edit currentness with the original full-range and complete-set oracles.
It separately excludes unrelated extensions' notifications and progress from an
explicit occurrence search when checking routine F# Navigator toasts.

The first authored quiet-status regression failed against the original adapter
(`.cache/quiet-ux-before.log`). Intermediate failures were fixed: the first UI
assertion also counted Git and explicit occurrence progress, and a native hover
test caught missing idle-hover feedback. Final required failures: **zero**.
The signature-hidden expected failure remains separate from passes. No unrelated
union/signature gap is claimed fixed, and the separate strict union regression
was not rerun for this UI-only change.

Rust sources and native bytes are unchanged from `74a5450`, which has the same
tree as the previously qualified `8db5ef8`. Under the task's explicit UI-only
qualification boundary, the full pinned Grace/FsCheck/compiler corpus was not
rerun: its prior required passes remain the native evidence described in
[document navigation qualification](document-navigation-qualification.md).
This is not a new corpus pass or a claim of general navigation completeness.
No inputs were blocked for the executed gates. The six ignored core tests,
prepared semantic/editor integration, other operating systems and other editor
versions were not newly validated.

Measured environment: Linux x64, VS Code **1.147.0** at `/usr/share/code/code`,
extension **0.1.0**, Node **24.15.0**, npm **11.12.1**, Rust/cargo **1.100.0**,
Safe Chain **1.5.10**, Nix SDK **10.0.404** and .NET runtime **10.0.15**.
The indexed editor suites started no compiler/helper/decompiler processes.
Version evidence is in `.cache/quiet-ux-versions.log`; no latency or memory
benchmark claim is made.
