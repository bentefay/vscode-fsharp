# Read-only metadata index experiment

The explicit native preparation path removes repeated metadata SQL import and
commit work from a fresh workspace. It retains complete declaration maps and
revalidates the actual database, producer policy, current binary/XML inputs and
workspace ownership. It does not improve binding coverage by itself.

These results use immutable native
`7047e793d62f52030c72cfcec4abdf1e8ecb2cda46313b6ce11138de54ec8f2c`
(source `6a239c1ae52b`) and genuine schema2 artifacts from helper
`f215dac4599cb436d748ead64c01cf5f8795d8ddea79add66bb29cfdb6b763c2`.
The native is **diagnostic**: an exported active-pattern binding in a paired
`.fsi` signature has a separately confirmed wrong-exact result. The measurements
below use an unaffected exact framework hover and are not whole-engine acceptance.

## Correctness gates

`semantic/tests/prepared_indexes.py` tested the same exported JSON through normal
import and prepared indexes. Fresh and reopened answers matched exactly; every
type, scope and term row matched the original canonical digest. No binary fact
rows were copied into the mutable workspace index. A valid SQLite deletion with
the original mtime restored caused both the already-loaded and fresh processes
to refuse an exact answer.

`semantic/tests/prepared_resolution_states.py` genuinely prepared eleven index
generations for the nine compiler-tested workspaces. All sixteen ownership and
shadowing controls passed on both JSON and prepared paths with identical semantic
responses. Unsupported cases remain safety checks, not feature-coverage successes.
Normal Rust tests also cover read-only writes, source-JSON removal, live binary
mutation/recovery, cancellation and retry, and preserving certified empty maps.

Reports are under `test-results/prepared-indexes/diagnostic-7047-positive` and
`diagnostic-7047-states`. They include commands, source/derived hashes, full map
digests and response transcripts. The same helper artifacts and source model were
used without manually changing their schema, provenance or ownership fields.

## Controlled resource comparison

All other team CPU/IO workloads stopped. The order was JSON then prepared,
followed by prepared then JSON. Each profile used separate processes for a fresh
workspace index, a reopened index and a fresh index with its model loaded first.
The OS page cache was not flushed. These are two samples per condition, not a
distribution or an actual VS Code host measurement.

| Exact framework hover | JSON import | Prepared read-only index |
| --- | ---: | ---: |
| Fresh workspace first answer | 739.69–990.56 ms | 86.76–125.50 ms |
| Fresh first-answer process CPU | 700–910 ms | 80–100 ms |
| Fresh native peak RSS | 23.71–23.84 MiB | 16.47–16.71 MiB |
| Reopened workspace first answer | 77.86–85.30 ms | 83.62–87.82 ms |
| Reopened native peak RSS | 10.54–10.80 MiB | 10.79–10.82 MiB |
| Model loaded first, then first hover | 756.60–871.23 ms | 73.11–74.01 ms |
| Subsequent warm answers | 0.59–1.62 ms | 0.54–0.71 ms |

Both paths read/parsed zero source files and launched zero helpers at workspace
startup. Prepared loading records zero source JSON bytes and zero inventory
import/commit time, but it still hashes the full database and live dependencies.
The reopened results show that preparation mainly removes the first SQL import;
it does not eliminate the remaining freshness and binding cost.

| Prepared artifact used for runtime | Descriptor bytes | Database bytes hashed | Live dependency bytes hashed |
| --- | ---: | ---: | ---: |
| Complete inventory | 204,874 | 10,383,360 | 10,307,720 |
| Library catalog | 12,306 | 37,744,640 | 15,418,248 |

The complete inventory still has 3,950 type keys/4,295 rows, 200 scope keys/529
rows and 5,672 term keys/7,993 rows. All three digests match the JSON baseline.
The original catalog JSON is 25,678,699 bytes, so the database's integrity scan
is not free and does not read fewer bytes merely because JSON parsing is avoided.
Byte counters explicitly separate descriptors, databases and live input hashing.
They do not claim to count every filesystem/SQLite page read.

## Explicit preparation cost

After the runtime profiles, the same native binary prepared new generations in
the same quiet window:

| Native preparation phase | Wall | CPU | Sampled peak native RSS |
| --- | ---: | ---: | ---: |
| Complete inventory | 309.14 ms | 290.76 ms | 14.89 MiB |
| Library catalog | 481.32 ms | 449.52 ms | 13.87 MiB |

This adds about 790 ms to preparation; it is moved work, not eliminated work.
The prior explicit helper export/evaluation cost remains separate: 14.97 seconds
and 470.96 MiB peak in the single-session f215 evaluation harness. That harness
exports the two relevant catalog assemblies. The installation CLI uses separate
helper processes and may export a different assembly set, so those helper values
are not measurements of the shipping CLI workflow.

The second catalog database occupied 37,810,176 bytes rather than 37,744,640.
Its source hash, certificate, canonical semantic digest and derivation key were
identical; SQLite page layouts and physical byte hashes can differ. Each
descriptor binds its actual closed database. The raw comparison is
`quiet-7047/publication-layout-comparison.json`.

All raw profiles, CPU/RSS/byte values and commands are in
`test-results/prepared-indexes/quiet-7047`.

## Subsequent required checks

The later signature-parser candidate
`f27d9fc7d54ba690209054c2a08aedf5c72f45069979fdafdf9c7c1a7695e0b6`
(captured source `30eabe984981`) passed newly generated prepared-index positive,
full-map, corruption and all sixteen JSON/prepared ownership gates. Its builder
key differs, so its descriptors were actually rebuilt; no 7047 descriptor was
reused. The above timing results remain measurements of 7047, not f27.

On a held source tree, the normal `cli/bin/run check` passed TypeScript checks
and compilation, Rust formatting, 195 core tests, three daemon tests and 23
frontend transport/virtual-document/contract tests. Five Rust tests requiring
separate real helper fixtures were explicitly ignored by this normal command;
their earlier independent fixture reports are separate evidence. An additional
actual-native adapter run passed four base cases and eight prepared ownership
cases (13 Node tests including the parent). Its editor boundary is a double.

The exact commands, logs and hashes are recorded in
`test-results/verification/f27-required-checks.json`. Actual VS Code host tests
remain blocked by the execution host's previously confirmed AF_UNIX restriction
and were not retried. Nix is unavailable here and was not executed. These checks
do not replace the independent signature-scope compiler oracle or establish
whole-feature parity.

## Current observed-v4 regression checks

The final locale-certificate candidate
`85d1d8635f331ff9fff7fc8db8f0527c4a97f09a5c8716303e73e71b764b7486`
(source `7ce4ec30eb37`) passed the positive/full-map/corruption suite and all
sixteen JSON/prepared ownership controls. Eleven prepared generations were
rebuilt with its own builder key. The nine genuine project models were freshly
evaluated by helper `09f01156157a` under the observed-v4 contract, including the
runtime, ICU, loader and locale witnesses. Ordered source files, compiler
arguments, references and workspace ownership exactly matched the original
compiler-tested contexts, so the existing schema2 f215 binary facts were retained
with their original provenance. No model certificate was manually upgraded.

The held-source normal CLI check passed 216 core tests, three daemon tests and
23 frontend tests, plus TypeScript and formatting checks; five fixture-dependent
Rust tests remained explicitly ignored. The real-native adapter suite passed
four base cases and eight genuine prepared ownership cases (13 Node tests
including the parent). Its editor API remains a double. The base linked-context
case uses an explicitly synthetic protocol certificate, separate from the nine
genuinely evaluated prepared models.

`test-results/verification/85d1-required-checks.json` records exact binary/source/
helper hashes, logs and report digests. Current prepared results are under
`test-results/prepared-indexes/85d1-positive` and `85d1-states`; model refresh
evidence is under `test-results/resolution-states/v4-models-09f011`. These are
correctness runs with concurrent development workloads, separate from the quiet
current-v4 profile below. All resource tables above remain historical 7047
measurements.

## Observed-v4 full-context baseline (85d1)

After the aligned correctness gates, other team CPU/IO workloads stopped for two
sequential runs on immutable 85d1, the genuinely evaluated 09f011 positive model
and its rebuilt 85d1 prepared descriptors. No helper export or unrelated test ran
in this window. All 18 hover responses were exact. These are two samples per
condition across six native processes; OS page cache was not controlled.

| Prepared framework hover, current observed-v4 model | Measured range |
| --- | ---: |
| Fresh workspace first answer, including model validation | 382.76–533.42 ms |
| Fresh first-answer process CPU | 370–480 ms |
| Reopened workspace first answer, including model validation | 369.54–384.00 ms |
| Reopened first-answer process CPU | 360–380 ms |
| Subsequent warm answers | 0.56–0.92 ms |
| Native RSS after lazy workspace open | 5.32–5.88 MiB |
| Fresh workspace sampled native peak RSS | 17.78–17.85 MiB |
| Reopened workspace sampled native peak RSS | 11.95–12.07 MiB |

The complete model phase took 295–345 ms across these processes. Loading the model
explicitly through the project tree cost 297.97–327.80 ms; the subsequent first
hover then took 72.38–72.67 ms. That is paid work moved ahead of the hover, not an
omitted cost or a current full-startup result. Lazy workspace/open itself took
1.21–7.49 ms and read/parsed zero source files or launched helpers; its timing
excludes subprocess creation overhead. The staged-model processes peaked around
18.1 MiB. The first fresh process also paid 105.55 ms for catalog loading and 74.72 ms
for inventory loading, whereas the second paid 43.25 ms and 21.78 ms. This variation
is retained rather than attributed to an unmeasured OS-cache effect.

The 610,383-byte project model contains 1,908 environment-value hashes, 37
globalization file witnesses, 27 selection-directory witnesses covering 8,203
immediate entries, and 74 path-link witnesses. The file payload accounting is:

| Validation list | Real-file rows | Listed file bytes |
| --- | ---: | ---: |
| Project `inputHashes` | 178 | 190,256,036 |
| Globalization `files` | 37 | 178,961,862 |
| Both validators combined | 215 | 369,217,898 |

The current validators independently hash these lists on first use. They contain
160 distinct device/inode files totaling 75,541,388 bytes: path aliases and overlap
between the validators account for 293,676,510 repeated payload bytes. For
example, one 31.86 MB ICU data file appears eight times across the lists. This
post-run certificate/file-size audit is not disk-IO or syscall instrumentation;
directory enumeration and link validation add work. Any future digest reuse must
retain every path-selection witness and validate the same file generation.

The current first-use cost is therefore substantially above the historical
7047 prepared 87–125 ms samples; those older values must not represent current
observed-v4 activation. Native-only warm answers remain under 1 ms in these samples.
This is not a VS Code/extension-host or Ionide comparison. Existing helper-export
and native-index-preparation costs remain separate and were not rerun here.

Raw responses, model/catalog/inventory phase counters, CPU/RSS observations,
artifact hashes and the certificate-byte reproducer are retained under
`test-results/prepared-indexes/quiet-85d1-v4`, with `summary.json` as the entrypoint.

## Bounded validation digest reuse (858f)

Immutable `858f9d9a16d39338b63406916bf0fdf0bb2138a3b7b2ae9efd9bdc0d316bd14d`
(source `37381dc2743e`) adds a digest cache scoped to one model-validation
generation, bounded to 128 retained regular-file descriptors. Every path and
expected digest remains checked. Changed file identities, mismatched expected
hashes, cancellation and special-file inputs invalidate the generation; there
is no persisted trust. The source/artifact inputs and semantic lookup data remain
unchanged. Its builder-specific prepared descriptors were genuinely rebuilt.

Before timing, all sixteen JSON/prepared ownership controls, positive/full-map/
reopen/corruption checks and twelve actual-native adapter scenarios passed.
The held-source normal CLI check passed 221 core tests, three daemon tests and
23 frontend tests; five separate-fixture tests remained explicitly ignored.
Independent reviewer mutation/culture/bootstrap gates are separate evidence.

A new quiet window used the same model and exact hover in alternating order:
85d1, 858f, 858f, 85d1. All 36 answers matched the positive correctness result.
Source JSON hashes, complete certificates, catalog lookup rows and all three
inventory maps matched. Builder-derived state/digests and SQLite page layouts
may differ; neither is claimed byte-identical. Two samples per condition are
not a latency distribution, and OS page cache remained uncontrolled.

| Full-context native measurement | 85d1 baseline | 858f digest reuse |
| --- | ---: | ---: |
| Fresh prepared first hover | 345.31–387.16 ms | 164.47–250.17 ms |
| Fresh first-hover CPU | 340–390 ms | 140–230 ms |
| Reopened prepared first hover | 342.33–503.80 ms | 189.08–286.19 ms |
| Reopened first-hover CPU | 330–490 ms | 140–260 ms |
| Model phase across all processes | 277.79–374.93 ms | 100.27–207.83 ms |
| Subsequent warm hover | 0.51–0.89 ms | 0.50–1.64 ms |
| Fresh sampled native peak RSS | 16.85–16.92 MiB | 17.63–17.84 MiB |

Each 858f first model validation reported 287 file-state checks, 160 regular-file
reads and 55 digest-cache hits. Requested file payload remained 369,217,898 bytes;
the actual read loops fed 75,541,388 bytes into SHA256. Warm queries added zero
validation hashing. These are measured implementation counters, not physical
disk IO. The approximately 4.9-fold reduction in hash input is not an end-to-end
latency ratio or an Ionide comparison. All path/link/directory witnesses remain.

Explicitly staging the model took 124.17–124.82 ms in 858f; its following first
hover still cost 64.31–68.08 ms. This cost remains included when describing full
first use. Source startup remains lazy with zero source reads/parses/helpers;
native startup RSS was 5.63–6.07 MiB across the optimized processes. The modest
memory increase is retained in the comparison rather than omitted.

Raw paired runs, exact artifact and table digests, counters, phases and the
summary reproducer are in `test-results/prepared-indexes/quiet-digest-858f`.
`test-results/verification/858f-required-checks.json` links the required checks
and correctness reports. No new helper export/preparation workload ran during
timing, and real VS Code host/Nix limitations remain unchanged.
