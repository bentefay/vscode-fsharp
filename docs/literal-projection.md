# Literal-only array projection

A bounded lexical pass recognizes semicolon-separated, unsuffixed decimal integer constants in arrays. It keeps the first existing literal and replaces the remaining ASCII interior with spaces while preserving every byte offset and newline. Tight negative constants are eligible; spaced unary-minus expressions, identifiers, units, custom numeric suffixes, interpolation and directives are excluded. The original source remains authoritative for text and UTF-16 positions.

The public `dotnet/fsharp` ComparersRegression.fs file contains 2,704 eligible arrays with 1,331,124 constants. The isolated projection pass took 30 ms. The completed parser experiment dropped from 6,227 ms and 732,900 KiB peak RSS to 711 ms and 72,408 KiB. This is an approximately 8.8× parse-time and 10.1× memory improvement for this selected pathological file, not a comparison with Ionide or Rider and not a whole-workspace measurement.

The complete pre-integration serialized `ParsedFile` was identical before and after projection: tokens, declarations and scopes, modules, imports, aliases, uncertainties, ownership/type facts and member facts. The original/projected SHA256 hashes, exact source commit/hash, compared field names and all queried occurrence identities are recorded in `evaluation/results/literal-projection-differential.json`. One gzip-compressed copy of the identical summary is stored alongside it. `createUnionWrap` had 2/2 occurrences resolve to the same declaration; `createUnionTypes` had 38/38. The 4,176 nondeclaration `validate` occurrences remained unsupported in both runs; they are not counted as successful navigation.

Production integration adds explicit literal-island provenance and refuses expression-shape facts intersecting projected regions. Consequently production summaries need not be byte-identical to the historical control. The certified path permits 1,500 ms and 64 MiB additional resident memory, retaining the 96 MiB absolute guard. Ordinary parsing retains 250 ms/48 MiB. Explicit environment overrides apply to both. Small files under 64 KiB skip the projection pass.

The historical standalone probe was built with `cargo build --release --manifest-path ../tsprobe/Cargo.toml --bin projection_compare`, then run as:

```sh
FSHARP_NAVIGATOR_PARSE_MS=10000 FSHARP_NAVIGATOR_PARSE_GROWTH_MB=1536 FSHARP_NAVIGATOR_PARSE_RSS_MB=2048 ../tsprobe/target/release/projection_compare original ../research/corpus/fsharp/tests/FSharp.Core.UnitTests/FSharp.Core/ComparersRegression.fs original-summary.json
FSHARP_NAVIGATOR_PARSE_MS=2000 FSHARP_NAVIGATOR_PARSE_GROWTH_MB=96 FSHARP_NAVIGATOR_PARSE_RSS_MB=128 ../tsprobe/target/release/projection_compare projected ../research/corpus/fsharp/tests/FSharp.Core.UnitTests/FSharp.Core/ComparersRegression.fs projected-summary.json
```

Those commands describe the preserved probe run; the probe was an experiment outside the shipped package. Rebuilding against current production code requires the explicit original-parser control `parse_impl_with_projection(source, false, false)`, otherwise automatic projection would affect both branches. The current built-in differential regression, including exact token/declaration identities and a useful definition after an island, runs with:

```sh
scripts/with-local-tools cargo test -p navigator-core --lib treesitter::tests::literal_projection_preserves_navigation_identity_and_records_provenance
```

Normal opened-buffer daemon definition/reference latency and peak measurements remain pending the final reviewed executable.
