# Current source candidates after index readiness

Base: `1fe6a2059e962199fff17b457aa0af68be6dbed7`.

The pinned compiler-source study found a real availability regression: an array
pattern binder was offered during discovery, but disappeared once the raw index
reported ready and broader contextual work exceeded its foreground budget.
Readiness remained provisional project evidence, not a loaded compiler project.

## Red and green

`ready_context_limit_preserves_current_source_candidates` reproduces the failure
without a compiler or fabricated result. More than 512 entries force discovery;
17 same-name source files make later contextual work exceed the read budget.
The authored local target is present before readiness and was absent afterwards.
The pre-fix test failed at the ready-target assertion. Earlier malformed fixture
attempts are retained privately and are not counted as the regression.

The repair retains origin candidates from the already-current parse in the
existing nonexact Definition fallback, before broader candidate collection.
The existing work-limit handler can expose those candidates as incomplete.
There is no extra parse/read, budget reset, exact proof, compiler invocation or
change to the successful exact fast path. The ordinary full candidate union is
unchanged; the source-only fallback remains capped at 200 with truncation labels.

The green regression also checks both source alternatives, shifted and renamed
overlays, stale-version rejection, an already-cancelled request, reference/hover
isolation, and close-to-disk restoration with all 19 normal candidates. This is
not an in-flight editor cancellation qualification.

## Real-source qualification

The unchanged `dotnet/fsharp` source pin is
`66187a30e8bb81d571f0244d4bd64405182ca219`. No SDK substitution, restore,
evaluation, semantic import or helper was used. The earlier 12-operation oracle
was reused: nine definitions, two references and one external hover.

The old binary again returned the local target during discovery and then lost
it after readiness. The repaired binary retains it at rank 1 of five candidates
on the first ready query and ten repeats, after an eight-line unsaved shift,
and after restoration. The former range is absent after the edit; an obsolete
version is rejected. Both binaries initially return an indexing response before
the cold continuation finds the target. There is no new cold-start claim.

The repair also exposes the frozen record-field target as an incomplete
singleton. Thus two of nine strict definition targets are available after
readiness, versus zero in the old ready-state run. No exact results are newly
claimed. Signature-preferred targets remain distinct from implementation
declarations; an implementation candidate is not a wrong semantic binding or a
pass of the signature policy. The remaining target-policy gaps, both withheld
reference sets and withheld hover remain visible, not passing coverage.

Native timings are descriptive, one process per binary and ten warm repeats:

| Equivalent successful control | Old median | Repaired median |
| --- | ---: | ---: |
| Exact local binding | 0.151 ms | 0.194 ms |
| Same two incomplete candidates | 0.425 ms | 0.391 ms |

Each control retained its authored outcome on all eleven queries. The repaired
compiler local-candidate query had a 57.64 ms warm median (52.95–65.12 ms).
The old 74.06 ms median measured refusal, so these are not equivalent speed
observations. Fixed run order, uncontrolled OS cache, sampler overhead and small
samples preclude p95, general speed or whole-editor memory claims.

## Durable gate and limits

The [portable engine gate](../evaluation/engine-corpus/README.md) now checks six
required definitions on pinned Grace, FsCheck and compiler source, including
fresh/ready, dirty/close and persistent-cache restart states. All 18 required
useful/exact observations passed on the repaired binary. The same compiler gate
fails on the old binary at ready, dirty, close and restart. Eight additional compiler probes
are reported separately and never counted as required passes. Missing or changed
inputs block the gate rather than silently skipping it. The final audit requires
every selected corpus, required case and lifecycle phase, including the first
response and discovery demand. It re-scores raw answers individually: an empty
run, missing score, wrong target or extra success cannot hide a lost obligation.
Known probes must also be recorded separately. This gate is distinct from the
prepared native Grace46 replay and the older exploratory corpus runner.

Protected checks passed: 3 packaging Python tests, 26 corpus-gate unit tests,
51 tooling/manifest tests, 334 core tests and 4 daemon tests (5 core tests
ignored), TypeScript/format checks, 132 adapter/transport tests and 21 real-native
adapter tests.

The historical 34 packaged-editor cases and scoped signature XFAIL were not
rerun: no permitted normal-sandbox editor route was available. No GUI runner,
profile or security settings were changed. This engine repair does not close
signature visibility, native References currentness, prepared List.map hover,
or normal-sandbox preview acceptance.
