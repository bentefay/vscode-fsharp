# Singleton-array match Definition

The document-local route can return the declaration of an ordinary unguarded
singleton-array match binder. It uses the same whole-arm lexical certificate as
complete References: the selected token must be the binder or a use belonging to
that binder after checking the entire arm body and nested shadowing. A matching
spelling alone is insufficient. Member suffixes and record labels are not uses.

This adds no project evaluation, compiler invocation, parser mode or background
preparation. Unsupported shapes retain the ordinary conservative fallback.
Guards, multiple/tuple/constructor patterns, computation-expression match forms,
conditional directives, syntax errors and bodies outside the lexical proof remain
excluded. The compiler's conditional tuple `idx` case is unchanged.

The proof version advances to v4. Origin hash/version, cancellation and resource
checks remain in force. Match-arm Definition answers do not seed the opportunistic
single-use References cache; complete References still proves its own use set.

## Fail-first contract

At baseline `8877b9e`, an authored local Definition regression failed. The existing
frozen compiler `RichText.fs` target is zero-based line 15, UTF-16 columns 13–17;
the queried use is columns 24–28. Its strict corpus requirement advances from
useful incomplete candidates to an exact target, without changing the oracle or
removing other cases. The baseline failed that stronger requirement at ready,
dirty, closed/restored and restart-ready boundaries. Complete References remains
a separate exact-set obligation with and without the declaration.

Engine validation does not establish keyboard F12, picker, rendered References,
normal-sandbox package acceptance or a comparison with another language server.

## Validation and native diagnostic

The final candidate passes 344 core tests (six explicit ignores), four daemon
tests, 39 corpus-scorer tests, 51 tooling/manifest tests, three packaging tests,
132 adapter/transport tests and 21 real-native adapter tests. The three-corpus
gate passes all 32 Definition observations (22 required useful/exact outcomes)
and all 16 reference lifecycle obligations (14 complete sets, two stale-version
rejections). The eight historical compiler probes remain separately reported.
The earlier fallback regression now uses a guarded arm so it still exercises
work-limit candidate preservation; its candidate, limit and freshness assertions
were not relaxed. Unguarded arms have separate exact tests.

A fixed baseline/candidate/candidate/baseline replay used two fresh processes and
empty caches per build on pinned compiler sources and Grace. The baseline was
`8877b9e`; both arms used the same request sequence and existing opt-in trace.
Filesystem caches were uncontrolled/warm. This is a small native diagnostic,
not an editor measurement, percentile estimate or comparison with another tool.

| Compiler `part` boundary | Baseline incomplete candidates | New proven exact target |
| --- | --- | --- |
| Process launch to first useful Definition | 147–158 ms | 15–19 ms |
| First Definition after raw readiness | 757–793 ms | 6–12 ms |
| Warm Definition RPC | 61–62 ms | 0.26–0.53 ms |
| Edit acknowledgment to current Definition | 73–78 ms | 6–14 ms |
| Undo Definition RPC | 84–122 ms | 6–15 ms |
| Held daemon-tree PSS after the workload | 41.29–41.30 MiB | 23.15–24.00 MiB |

The outcome changes: one exact document-owned target replaces five incomplete
choices. Baseline cold requests needed one 100 ms continuation; the new proof
answers on the first request without project discovery. A fixed nonlocal demand
then starts indexing in both arms. Raw readiness was 2.46–2.73 s versus 2.55–2.56 s,
including intervening queries/polling; this is not isolated indexing throughput.
The lower held memory follows avoiding contextual candidate materialization for
this query; it is not a whole-editor or general memory reduction claim. Each PSS
hold used three complete process-tree samples; peaks were not qualified.

All 40 scored Definition boundaries were current and useful. All eight compiler
References checks matched the complete oracle; Grace References remained eight
explicit refusals, not useful timings. Compiler initial References RPCs were
4.84–5.43 ms before and 5.55–9.71 ms after; post-edit RPCs were 6.26–6.45 ms before
and 6.69–11.95 ms after. This small sample does not establish latency equivalence
or attribute the observed increase. References proof logic is unchanged.
Grace full candidate/ranking outputs were identical in all five checked states;
cold useful time was 364–384 ms before versus 363–370 ms after, warm 20–22 versus
18–21 ms, and held PSS 27.52–27.66 versus 26.65–27.57 MiB. These are guardrail
observations, not a general speed claim. All eight stale-version probes rejected
old requests; source pins stayed unchanged and no compiler/helper started.

No new VSIX or normal-sandbox GUI acceptance is implied. The previously built
private package predates this source change. Real keyboard F12, installation and
rendered References remain separate preview qualification steps.
