# Current source status

Updated 21 November 2026 for the initial import into `bentefay/vscode-fsharp`.

This repository contains the recovered Rust-first prototype: the native engine,
VS Code adapter, development CLI, explicit project/metadata helpers, Nix setup,
tests, and research evidence. It is work in progress with known semantic defects.
It is not an accepted Ionide replacement.

## Recovery identity

The restored source is checkpoint 11, archive SHA-256
`cab8c94e3797cf10f94a86953c1f8fd3ab09af7fc24de027af047f62502f5751`.
The native source manifest is
[`evaluation/candidates/source-0c8920507029.json`](../evaluation/candidates/source-0c8920507029.json).
That capture produced candidate `710f046e290da02e8e85362add92125334f5ec7e1bb498f1201fc25eb6905fed`.
All 65 native inputs were compared with that manifest after restoration and
matched with no differences.

The repository stores the two large generated C parsers as pinned gzip inputs
outside the native source tree. The development CLI restores their exact bytes
before build/check/package operations; direct Cargo users run
`node scripts/restore-parser-sources.mjs` first. This storage change does not
change the 65 native inputs or add installed-extension startup work.

The initial Git import contains working source, build inputs, tests, research
scripts and summaries. Historical source archives, generated project/catalog
data, runtime logs and raw benchmark captures are not working-tree build inputs
and remain in the source checkpoint. Some historical reports therefore refer to
evidence paths that are absent from this checkout. Their recorded results are
preserved as historical claims, not presented as newly reproduced results.

Later experiments discussed during development are not part of this recovered
production source. In particular, the document-local module under
`evaluation/experiments/local-lexical-proof/implementation/` is staged research;
it is not wired into the native providers in this checkout. Later isolated
timings or cache work must not be attributed to this code.

## Recorded validation and limits

| Area | Evidence retained from the captured candidate | Current interpretation |
| --- | --- | --- |
| Native unit checks | 232 core tests and 3 daemon tests passed; 5 core tests ignored | Historical execution, not a fresh import-time run |
| Synthetic fixed corpus | 98 correct answers out of 100 queries | Coverage evidence, not general semantic correctness |
| Grace fixed corpus | 14 correct answers out of 46 queries: 10 definitions and 4 complete reference sets | 32 unavailable; full navigation parity is unmet |
| Document-local fast path | Staged implementation and acceptance suites | Not integrated in this checkout |
| Nix SDK | Pinned SDK evaluated and realized; tiny F# restore/build passed | Full extension workflow unverified; reusable model profile rejected |
| Editor validation | Adapter/native tests and host harness exist | Actual VS Code host/UI execution remains unverified |
| Performance goal | Historical component measurements only | No established orders-of-magnitude improvement over Ionide or Rider |

No test result in this table is a new execution made merely by restoring or
committing the source. Historical reports may describe earlier candidates as
pending or accepted within a narrow test slice; this document takes precedence
for the current release status.

Fresh restoration checks parsed 769 JSON files strictly and syntax-checked 18
CLI/harness JavaScript files. `cli/bin/run check` could not run because this
workspace lacks Nix and the protected task-local toolchain. No toolchain or
dependency-protection bypass was used, and no fresh native build/test pass is
claimed.

The new parser-restoration helper passed 19 focused Node tests for exact output,
repeat verification, modified files, compressed/expanded hashes and bounds,
truncation, symlinks, concurrent installation, manifest validation and missing,
corrupt or reordered multipart inputs. Two
additional independent controls rejected concatenated gzip expansion and a
symlink archive without leaving a partial target. Two further multipart controls
confirmed that a valid obsolete combined archive cannot mask a missing part and
that the expanded byte limit remains enforced. These are fresh bootstrap
checks, not a native semantic or editor-host pass.

## Known correctness blockers

1. The project-signature guard checks a case-sensitive `.fsi` suffix. Valid
   `.FSI` or `.FsI` inputs can therefore be missed, allowing a parameter reference
   set to claim completeness while omitting its signature occurrence.
2. In `let caller x = C.M(x=x)`, the left `x` can name the callee's parameter,
   while the right `x` refers to the caller's local. The current binding path
   can confuse those roles. A fix must preserve genuine equality expressions
   such as an ordinary function call `g(x=x)`.
3. Paired `.fsi`/`.fs` definition target preferences and complete references
   across arbitrary F# code are not implemented. Refusing an unsupported query
   safely does not count as feature equivalence.

The first implementation work after import is to add and validate the two
correctness repairs, then integrate and measure a bounded document-local fast
path. Continue using immutable candidate identities, compiler-backed controls,
dirty-buffer/cache invalidation tests and unchanged fixed-corpus queries.
Performance results must separate setup, native requests and actual editor
startup, and must report correctness coverage alongside latency and memory.
