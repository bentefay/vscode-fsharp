# Shared binary preparation experiment

This opt-in development path imports the selected binary reference context once
and reuses it for explicitly selected library catalogs and the complete external
type/scope/term inventory. It performs no workspace source-project check and
does not change normal navigation or the default preparation workflow.

The captured helper is `e450b15c7f767fd3863b8b9c023757bc8fd4782289377bdbfe2dfe49863328d2`,
with source manifest/archive `evaluation/candidates/helper-e450b15c7f76.json` and
`helper-source-19e2e2d7705a.tar.gz`. Frozen f215 remains the unchanged semantic
control. The first controlled comparison found a time/memory tradeoff, not an
incremental latency improvement from the single-import path. It stays opt-in.

## Explicit invocation

Use a new output directory and the same explicit assembly selection as the
control. For example, substituting actual selected reference paths:

```sh
cli/bin/run prepare --root /workspace/project --out /workspace/new-generation \
  --helper /path/to/experimental/FSharpNavigator.Semantic.dll \
  --assembly /selected/FSharp.Core.dll --assembly /selected/System.Runtime.dll \
  --experimental-shared-context --shared-context-max-mb 768 --timeout-ms 120000
```

`--native-index --backend /path/to/pinned/fsharp-navigator` additionally prepares
read-only native indexes. That cost is separate from helper extraction. The CLI
reserves the fresh output directory before any write and publishes settings only
after every requested phase succeeds. Without `--assembly`, the existing CLI
behavior selects all available external assemblies; the two-assembly experiment
does not establish the cost or success of that larger workload.

The direct JSON-lines method is `metadata/prepare-experimental`. Its parameters
are `referencePaths`, `workspaceAssemblyPaths`, `assemblyPaths`, a new absolute
`outputDirectory`, and optional `maxWorkingSetMb`/`timeoutMilliseconds`. It rejects
source-project/document inputs. The process exits after this request even if
`--once` was omitted or another request is queued. It is not a resident cache.

The result identifies `catalogPath`, `environmentPath`, `importCount:1`,
`sourceProjectChecks:0`, complete counts, per-phase observations and the exact
producer contract. The published generation contains per-assembly catalog
shards, a schema2 manifest, the complete schema2 inventory and `job.json`.
Every artifact preserves the existing producer compatibility and live input
certificates; the experiment does not rewrite old artifacts' version fields.

## Lifetime and publication

One checker imports the same explicit selected references used by the control.
The synthetic import expression is `()` and is parsed/checked; zero source-project
checks means no repository source project is loaded. Catalog declaration traversal
and per-artifact dependency closures remain separate. Context reuse is private to
this same full-reference job, not arbitrary per-artifact subset requests.
XML candidate prefixes are captured before import, and documentation maps are
scoped to one catalog assembly instead of the process-wide documentation cache.
Catalog symbols are streamed to private shard files. Inventory serialization
writes directly to a file, but the complete name dictionaries still exist in
memory: this is not a streaming inventory walker or a hard total-memory bound.

After the final artifact, the helper revalidates the selected binaries, producer
implementation, every captured XML selection and the union of artifact live
inputs. Only then is the private directory atomically renamed to the new output
generation. A failed mutation check publishes no generation; restored inputs
can produce a valid generation on retry. This is explicit preparation of a new
generation, not hot replacement of files used by a running editor.

A 25 ms sampler enforces a configurable current-helper RSS/deadline guard. It
is not an OS-enforced allocation cap or a process-tree memory limit. Exit75
means the sampled guard stopped the job; exit76 means sampling failed. A forced
exit can leave an unpublished `.binary-job-*` staging directory. If it occurs
after publication but before the response, a complete unused generation can
remain; the caller still receives no accepted response. External measurement
must count the owned process tree and temporary disk bytes.

## Correctness evidence

The captured e450 build passed seven correctness controls against f215:
complete catalogs for the two selected assemblies, complete type/scope/term
maps, XML content mutation, higher-priority XML appearance, assembly mutation,
successful recovery, and sampled-budget rejection without publication in that
test. The catalog comparison preserves all symbol values and duplicate
multiplicity, compares the exact first-row-wins native lookup projection, and
treats only `declaredMemberNames` as a set. Exporter implementation hash is
provenance and is excluded from semantic equality; FCS identity, selected
assembly ownership, input/XML witnesses and all other facts remain compared.
Raw evidence is `test-results/recovery/shared-metadata.json`.

An actual CLI run using e450 and native f27 produced both native indexes and an
exact external hover with intact XML langwords. Refusing an existing generation
preserved all selected file hashes. See `shared-cli-e450.json` and
`shared-cli-no-overwrite.json` in `test-results/recovery`. Independent comparison of all four arms also passed complete catalog payloads,
lookup projections, full environment/header/certificate equality, ten natural
exit/lifecycle controls, ordinary-error staging cleanup and source-input rejection.
The report is `test-results/shared-import/independent-e450-v2/report.json`.

Timing must separate four effects: unchanged f215, e450 separate processes with
streamed inventory output, e450 one process with a fresh import for each request,
and e450 one shared import. All variants must use identical selected catalogs,
full reference/ownership context and complete inventory. Record preparation
wall/CPU, owned process-tree peak RSS, temporary/final bytes, cache/OS conditions
and natural process exit before considering a default change. The earlier
14.97 second f215 harness already reused one process; process reuse alone was
never evidence that compiler imports were shared.

## First controlled comparison

All other team workloads paused for one sample of each arm, with identical
Core/Runtime catalog selection, complete reference/ownership context and full
inventory. Canonical facts, first-row alias projections, producer/dependency
certificates and lifecycle assertions matched. These are binary preparation
measurements; project evaluation, restore/build and native index creation are
additional work. The OS cache was not flushed.

| Preparation arm | Wall | Owned-process CPU | Sampled process-tree peak RSS |
| --- | ---: | ---: | ---: |
| Frozen f215, separate processes, full inventory stdout | 16.00 s | 17.99 s | 344.22 MiB |
| e450, separate processes, streamed inventory file | 15.18 s | 17.33 s | 287.23 MiB |
| e450, one process, new checker/import per request | 11.35 s | 14.04 s | 388.78 MiB |
| e450, one imported context, per-assembly XML cache lifetime | 11.93 s | 14.96 s | 319.73 MiB |

The shared-import arm did not beat same-process/new-checker latency in this
sample. Its lower peak comes with changed import and XML-cache lifetimes, so it
does not isolate either mechanism's individual contribution. Separate processes
with streamed inventory used the lowest peak but took longer. The experiment
does not justify silently changing default preparation or claiming a universal
speedup; repeated, rotated trials and a broader assembly set would be needed to
make that choice. Raw evidence is
`test-results/shared-import/quiet-e450/report.json`.
