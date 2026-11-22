# Lambda documentation and candidate recall

This checkpoint starts from `9a5c7cb13cd3f25792d66d3a613f960809b68528`.
The frozen FsCheck core workload is upstream revision
`1ef52cfc65d5c318f5ee4da467df9087ad03e34c`, with eight unchanged definition
oracles. It is an unprepared, trusted workspace; no restore or compiler
preparation is part of this qualification.

## Established cause and bounded repair

At `Internals.Common.fs`, the use in `| true, res -> res` has its expected
declaration at zero-based UTF-16 range `12:28–31`. Previous packaged runs
offered six incomplete candidates but omitted that declaration. This was a
candidate miss, not a query timeout or evidence that tuple patterns were
unsupported.

Independent inspection of the current parser established an intact tuple
pattern. The enclosing lambda's named children included trailing XML
documentation for the next declaration. The visitor selected that comment as
the body, skipping the real application/match subtree. Both the tuple binder
and the sibling arm's local `res` were absent from extracted declarations.
Candidate collection already included ordinary declarations and searched the
origin first; the loss preceded contextual summary and collection.

The lambda visitor now uses the existing child filter that excludes line,
block and XML comments. Binding, scope, ranking and certainty rules are
unchanged. The indexed summary revision advances so persisted omissions are
not reused; the independent lexical parser's revision is unchanged.

This repairs shared parser facts, not just a candidate-specific lane. A small
native control separately verifies an existing exact binding with the full
singleton URI/range. The raw-background control remains incomplete with empty
exact locations. No new exact proof is inferred from candidate uniqueness.

## Regression scope

The regression tests cover the original omitted subtree, comment/no-comment
controls, nested lambda and sibling match-arm shadowing, and record pattern
labels that must not become binders. Native integration also moves the binder
with an unsaved edit, requires the current candidate range, excludes the old
range, and rejects an obsolete document version. The original failing parser
run and diagnostic output are retained privately.

## Qualification outcome and publication boundary

The fresh private package SHA-256 is
`54866f5137055693651d787d58b07f3fef7c7df8cb6f49ad2f8dd4cbbe322d31`.
Its native SHA-256 is
`181af90f995e28b712670366e6cc2328a738c8ce1b6a4e1c260167dd1822b915`.
The existing package and all frozen source/oracle inputs remain unchanged.

On pinned Code 1.108.0 in the existing controlled disposable runner:

- All 34 required packaged scenarios passed; the scoped signature XFAIL
  remained (36 passing phases and one expected-failing phase).
- Two fresh FsCheck sessions each performed one cold Common query and the
  unchanged eight ready-state queries. Each ready-state set produced two exact
  jumps and six completed choices, with no misses. Across 18 F12 gestures there
  were four exact jumps and 14 deliberate full-range candidate selections.
- Both tuple queries returned a fresh exact singleton at `12:28–31`; trusted
  F12 moved the actual editor caret to `12:28`. The other seven targets remained
  correct. Independent reviewers verified these outcomes against the frozen
  oracles.
- All six Grace local/cross-file clean, foreign-dirty and reverted controls
  passed, including the unchanged guarded cross-file refusal.

The tuple result was **not** the planned incomplete picker. Its existing local
proof is now available: the pattern declaration is non-top-level, its lexical
interval covers only the arm body, and the existing local-symbol route does
not require signature pairing or candidate uniqueness. The actual workspace
was no longer provisional at that query. The native regression's deliberately
provisional model exercises a different state and does not predict this UI.

Independent review identified no proof-safety blocker. Release acceptance now
explicitly accepts this demonstrated existing lexical proof in place of the
anticipated tuple picker. Tuple offer and explicit picker selection were **not
tested**; no choice is artificially forced and no oracle is weakened.

### Site-by-site baseline reconciliation

Both independently audited runs agree on every classification below. A choice
means the known full URI/range was present and deliberately selected, not merely
a same-name row. Exact results require both the native full target range and
the actual editor's destination URI/caret.

| Frozen ready-state site | Baseline | Repaired package |
| --- | --- | --- |
| `Common.takeWhilePlusLast` | Choice | Choice |
| `Gen.map` | Choice | Choice |
| `Result.Stamp` | Choice | Choice |
| Tuple-pattern `res` | Candidate miss | Exact |
| Recursive `loop` | Exact | Exact |
| `MapContains` active pattern | Choice | Choice |
| `Random.RangeInt` | Choice | Choice |
| `Arbitrary.Generator` | Choice | Choice |

The tuple is the only classification change. Per eight-site ready-state cohort,
baseline was **1 exact, 6 choices, 1 miss**; repaired is **2 exact, 6 choices,
0 misses**. Across both runs, exact outcomes increase from two to four; the
previously exact recursive `loop` never regresses. The extra cold Common query
remains a completed choice in both runs. All seven non-tuple targets retain
their independently authored full URI/range, including the strict active-pattern
header span.

The record-label negative and nested/sibling shadow tests remain conservative.
The packaged ambiguous library-receiver, conditional, singleton-unproven and
later-open controls still return empty exact locations and require deliberate
choice. The known signature XFAIL remains separate and is not credited as a
passing ambiguity control.

The retained reviewed source patch matches the outgoing Rust diff byte-for-byte.
The tested VSIX's native and adapter bytes match the retained build outputs;
all three editor cohorts used its fixed package hash. Only this sanitized
qualification record and status documentation changed after those tests.

Protected aggregate checks passed: 3 Python tests, 51 tooling/manifest tests,
333 core tests and 4 daemon tests (5 core tests ignored), TypeScript and format
checks, and 132 adapter/transport tests. The strengthened native integration
test was rerun successfully after final review; real-native adapter tests passed
21/21. The FsCheck driver initially stopped before launching Code because a
protocol file was missing from its private copy. That failure was preserved;
the unchanged protocol was copied before the separately recorded executed run.

Raw evidence and packages remain private. These are correctness observations,
not a speed comparison, memory measurement, broader references qualification,
or normal-sandbox platform acceptance.
