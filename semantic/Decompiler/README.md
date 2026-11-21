# One-shot external implementation documents

This optional .NET 10.0.11 helper reads a requested assembly and exact XML
documentation member ID. It does not load a workspace, start FCS, execute target
code, or keep a managed process resident. Rust coordinates cancellation, process
ownership, limits, and the document cache.

The pinned dependency is ICSharpCode.Decompiler 11.1.0.9782, upstream commit
`a6909b2e1062d2404a52711926fa74030bd96119`, with a NuGet lockfile and its MIT
license. The implementation uses the upstream `IdStringProvider`,
`CSharpDecompiler.Decompile(EntityHandle[])`, and symbol-annotated identifier
writer APIs. `NoDocumentation` avoids ILSpy's default framework XML-doc loading;
library hover documentation remains the metadata catalog's responsibility.

```sh
scripts/with-local-tools dotnet restore semantic/Decompiler/FSharpNavigator.Decompiler.csproj --locked-mode
scripts/with-local-tools dotnet build semantic/Decompiler/FSharpNavigator.Decompiler.csproj -c Release --no-restore
scripts/with-local-tools python3 semantic/Decompiler/test.py
scripts/with-local-tools cargo test -p navigator-core --lib decompiler::tests
scripts/with-local-tools cargo test -p navigator-core --lib decompiler::tests::actual_helper -- --ignored --nocapture
```

The process accepts one JSON line with `--once`:

```json
{"assemblyPath":"/absolute/assembly.dll","xmlDocId":"P:System.String.Length"}
```

Successful responses contain one `csharp` document at a read-only
`fsharp-navigator://metadata/<content-key>.cs` URI, a nonempty UTF-16 declaration
identifier range, `targetKind: "decompiled-implementation"`, exact member
identity, and input dependency SHA-256 hashes. An unavailable response contains
no document or location and gives a reason. These are generated implementation
documents, not original source. SourceLink and embedded-source declaration
mapping are not implemented.

Ownership is restricted to the requested assembly and explicit metadata type
forwarders. Reference assemblies are redirected only through the same package's
`ref/<tfm>` to `lib/<tfm>` layout or an exactly matching installed framework pack
and runtime patch. Assembly identities and decoded requested member signatures
are checked independently on both sides; reference metadata tokens are never
reused in an implementation assembly. Signature comparison is conservative:
unmodeled cross-assembly nominal equivalence and nested forwarders can decline.
Reference-only stubs and methods/properties without IL bodies are unavailable.

The helper decompiles only the requested member (or the explicitly requested
type). Dependencies are read as metadata; the selected implementation's PE
image is loaded for its body. The document range comes from the emitted
declaration identifier's symbol, not a spelling search. Certificates cover
loaded inputs, helper/runtime binaries, and XML/PDB/dependency sidecars, including
missing-file markers; selected inputs are checked again before returning.

Rust imposes a five-second deadline, 512 KiB stdout / 64 KiB stderr limits, and
a sampled 128 MiB process-tree RSS guard. The helper has a three-second
cooperative budget, 128 MiB per-input guard, 64 KiB request limit and 120 KiB
character document limit. The managed GC limit and sampled native RSS guard are
not hard OS memory caps. Linux process groups plus observed descendants with
PID/start-time identities are terminated on cancellation, error, or completion.
Other native platforms are not yet implemented for this helper path.

The SQLite response cache is bounded to 64 documents / 8 MiB. Its key includes
the host implementation, executable and helper argument files; successful
dependency certificates are revalidated before reuse. Timeouts and unavailable
results are not cached.

Fresh recovery validation is in
`evaluation/results/decompiler-helper-recovered.json`: 14 real assembly cases,
including runtime and reference framework properties, a NuGet member,
overloaded methods, Unicode ranges, fields, mismatched implementation signatures,
missing members, reference-only assemblies, and output-size rejection. Five
native boundary tests and a separate real-helper/cache test passed. These are
functional checks, not a controlled performance comparison.

The matched diagnostic report
`evaluation/results/decompiler-helper-recovered-full-prefetch.json` preserves
the initial whole-dependency-image implementation. Metadata-only dependency
reads reduced the requested NuGet property's observed helper peak from 126.1
MiB to 87.3 MiB; current framework cases reached 95.0 MiB. RSS excludes the test
controller and fixture compilation and uses 10 ms PPid/start-time process-tree
sampling. Runs were not isolated timing benchmarks. Prior pre-recovery R2R
measurements do not certify this rebuilt helper.
