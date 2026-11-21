# Shared binary import experiment

The experiment preserves complete exported facts but does not show an additional
latency benefit from retaining one imported FCS context over simply reusing the
helper process. It changes the memory tradeoff. Keep the option experimental;
these results do not justify changing the default preparation path.

## Fixed workload and correctness

All arms use exactly FSharp.Core and System.Runtime as catalog targets, the same
168 selected binary references and the same workspace ownership exclusions from
the genuine positive fixture. They export 2,547 and 13,081 catalog symbols plus
the complete type/scope/term environment. This is not an all-catalog or large
monolith benchmark, and no source project checking occurs.

The unchanged control helper is
`f215dac4599cb436d748ead64c01cf5f8795d8ddea79add66bb29cfdb6b763c2`.
The experimental helper is
`e450b15c7f767fd3863b8b9c023757bc8fd4782289377bdbfe2dfe49863328d2`,
captured source `19e2e2d7705afeaafb7fb5dc99da378aa2a193bf6ae0929a6b091b640ea2d6e9`.

`semantic/tests/shared_metadata_independent.py` compares complete payloads,
including signatures, documentation, structured types, scope/term inventories,
producer contracts/profiles, FCS provenance, live dependency hashes, ownership
and XML-selection witnesses. Only exporter implementation evidence is separated.
The nonsemantic order of `declaredMemberNames` is normalized; catalog symbol
multisets and the exact first-row-wins alias projection are both checked. All
other values and ordering remain compared. Both catalog shards and the complete
environment match across all four arms, including the union of manifest inputs.

Ten process-lifetime checks pass: helpers exit, the shared job exits naturally
after one request, ordinary failures remove private staging, and source-project
inputs are rejected. A separate exact-e450 seven-case suite passes concurrent
XML byte mutation, creation of a higher-priority XML candidate, binary mutation,
positive recovery and sampled-budget exit without publication. These were gates
before the timed run, not failures counted as useful navigation results.

Raw correctness evidence is
`test-results/shared-import/independent-e450-v2/report.json` and
`test-results/recovery/shared-metadata.json`. The earlier failed independent
attempt was a harness `/proc` check incorrectly treating an already-exited
helper as an error; its log remains preserved and is not a helper failure.

## One coordinated quiet run

All other team CPU/IO workloads stopped. Each arm ran once, sequentially in the
order below; OS page caches were not flushed. Wall time includes process
lifecycle and artifact production. CPU is reaped helper-child user+system time;
RSS is externally sampled over the helper process tree. No editor is included.

| Arm | Wall | Helper CPU | Peak tree RSS |
| --- | ---: | ---: | ---: |
| f215, separate processes, full environment JSON on stdout | 16.00 s | 17.99 s | 344.22 MiB |
| e450, separate processes, streamed environment file | 15.18 s | 17.33 s | 287.23 MiB |
| e450, one process, new checker/import per request, streamed file | 11.35 s | 14.04 s | 388.78 MiB |
| e450, one imported context, per-assembly XML maps, streamed file | 11.93 s | 14.96 s | 319.73 MiB |

The shared job recorded one binary import (1.52 s), two catalog traversals
(2.85 s and 3.95 s), and complete inventory generation (3.41 s). It was not faster
than the same-process/new-checker arm in this sample. Its peak was lower than
that arm, while the separate streamed processes had the lowest peak. One sample
cannot establish a small latency difference as statistically meaningful.

The independent streaming arm is an experimental control. The default shipping
CLI still requests the complete environment on stdout; it does not silently
adopt file streaming or process reuse. The shared-context flag combines a single
import, scoped XML retention, streamed serialization and atomic generation
publication. Any overall difference must not be attributed solely to import
reuse or solely to process reuse.

Catalog shard sizes are identical across arms: 5,597,935 and 16,167,553 bytes.
The full environment is 9,530,719 bytes in the control's pretty JSON and
7,583,282 bytes in streamed form; canonical complete data and certificates are
identical. No declarations are omitted to obtain smaller output or lower memory.
The helper still builds complete in-memory name dictionaries before streaming
serialization; it is not a disk-backed metadata traversal.

The shared job used a 768 MiB current-process RSS guard sampled every 25 ms and
a 120-second deadline. That is not a hard memory limit or process-tree
enforcement. The reported tree peaks come from the separate external sampler;
all owned helper processes were gone at completion. These are explicit offline
preparation costs, separate from the Rust editor backend's startup/query costs.

The complete timings, phase measurements, commands, provenance and canonical
digests are in `test-results/shared-import/quiet-e450/report.json` and
`summary.json`. This experiment did not repeat restore, project evaluation,
native read-only index preparation or actual VS Code host tests.
