# Ordinary local-let References

The document-local References route can certify a plain immutable nonrecursive
`let` binding inside an expression. Its complete name scope is the explicit
continuation following the initializer. The initializer does not refer to the
new binding; an inner nonrecursive binding's initializer can still refer to the
outer binding. The existing bounded lexical walker checks the entire continuation,
including nested shadows and token roles, before returning a complete set.

This route does not add a Definition proof. It does not infer project identity,
signature applicability or nominal types, invoke a compiler, or seed the compact
single-use References cache. Proof version v5 separates cached certificates.
Source hash/version, cancellation, parsing and resource limits remain in force.

Admission is deliberately narrow: plain ordinary identifiers, immutable `let`,
nonrecursive and genuinely expression-local. Module values, class fields,
recursive/mutable/use/let! forms, typed or destructured binder patterns, directives,
recovered syntax and unsupported continuations retain conservative fallback.
Inline sibling-rule ambiguity is rejected. A refusal is not a complete empty set.

## Real-source regression

The pinned compiler's `RichText.fs` has `namespaceParts` declared at zero-based
UTF-16 131:16–30 and used at 136:21–35. Baseline `7b502e3` withholds both
include-declaration modes before and after raw readiness. The existing FsCheck
`memoize` local `t` is already complete and serves only as a guardrail.

An authored engine test failed before the change. The additive strict corpus
contract independently freezes the two-location set and a nested rebind: the
outer declaration plus the inner initializer's use, excluding the inner
binding/body. Baseline fails all 14 new complete-set obligations; its two stale
version rejections and the existing `part` obligations still pass. No existing
Definition or References denominator is removed.

Engine evidence does not establish real keyboard input, rendered reference
panels or package acceptance. The previously qualified private VSIX predates this
change. Measurements, when recorded, distinguish complete current results from
refusal and do not establish a comparison with another language server.

## Qualification

The candidate passes 348 core tests (six explicit ignores), four daemon tests,
51 corpus-scorer tests, 51 tooling/manifest tests, three package-normalization
tests, 132 mocked adapter/transport tests and 21 real-native adapter tests.
The strict Grace/FsCheck/compiler gate passes 32 Definition observations and
32 References obligations: 28 complete sets and four stale-version rejections.
The eight historical compiler Definition probes remain separately reported.
The new engine lifecycle regression covers changed names/ranges, rebinding,
pre-cancelled requests, stale versions, close/reopen and restart. A separate
untimed check on a disposable physical copy of the pinned source passes six
complete sets across original, saved edit and saved undo, closing and reopening
the document between states. These do not establish real editor in-flight
cancellation.

A small native diagnostic used two fresh processes with empty caches for the
compiler `namespaceParts` site. References was the first navigation request;
all results were checked against the literal current sets. Process launch to
first complete result was 14.27 and 20.55 ms; first References RPC was 5.53 and
8.64 ms. Edit acknowledgment to a current complete result was 6.45 and 8.62 ms.
Per-launch medians of three warm requests were 0.221 and 0.295 ms. Three held
PSS samples per process after undo were 14,628 and 14,504 KiB respectively, with
background indexing idle. These are daemon measurements, not whole-editor
memory, percentiles or a comparison with another language server. Filesystem
caches were uncontrolled/warm. Baseline refusals are not useful latency samples.

The existing exact FsCheck `memoize` local `t` control used baseline/candidate/
candidate/baseline fresh sessions. All 36 complete sets and four stale rejections
passed. Cold RPCs were 4.73–5.72 ms before versus 1.54–2.11 ms after, and post-edit
3.43–3.83 versus 1.23–1.57 ms. The new local route avoids starting background
indexing for this control; baseline memory sampling spanned indexing progress,
so these runs do not establish matched steady-state memory equivalence or a
general speed claim. The compiler runs add 18 complete sets and two stale
rejections. No helper, compiler or decompiler started.
