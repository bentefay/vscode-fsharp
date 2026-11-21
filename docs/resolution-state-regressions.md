# Resolution state regressions

`semantic/tests/resolution_states.py` builds real F# libraries and executable
consumers before creating genuine evaluated models, declaration inventories and
library catalogs. Runtime output distinguishes the framework implementation
(`true` for an empty string) from deliberately shadowing implementations (`false`).
The native replay never launches the compiler or helper. The positive control
requires exact external hover; rejected ambiguous cases are safety checks, not
feature-equivalence successes.

The 16 controls cover a same-FQN external nominal collision, a catalog containing
only one of the competing assemblies, complete versus missing/incomplete name
inventories, changed references with an old certificate, external modules and
AutoOpen values, repository-owned modules/values, unaffected unopened external
scopes, assembly-level AutoOpen, and recovery after reference changes. Complete
external-shadow cases additionally require that a current builtin certificate
actually loaded; an unrelated setup rejection cannot satisfy that test.

Reproduce with a matching immutable helper and native binary:

```sh
scripts/with-local-tools python3 semantic/tests/resolution_states.py \
  --prepare-only --extend-scope-controls --helper HELPER_DLL --output ORACLES_JSON
scripts/with-local-tools python3 semantic/tests/resolution_states.py \
  --prepared FIXTURE_ROOT --binary NATIVE_BINARY --output NATIVE_REPORT_JSON
```

`term_inventory.py` separately checks 13 producer properties against a real
compiled library: logical versus compiled module names, nested source paths,
namespace prefixes, module/assembly AutoOpen, RequireQualifiedAccess, union and
active-pattern cases, literals and operator spellings/identities.

The initial ff85 diagnostic incorrectly reused inventories between source-only
or unopened-control workspaces based only on equal reference paths. Current
workspace exclusion paths are also certificate inputs. The engine correctly
refused the two unopened positive controls. Those inventories were genuinely
re-exported with their current ownership; no certificate fields were edited.
The original report remains `native-ff85a386.json` as a rejected setup diagnostic.

The corrected ff85 report, `native-ff85a386-current-ownership.json`, passed 15/16
checks. Both unopened positives are exact and the assembly AutoOpen shadow is
blocked. It exposed a real wrong-exact framework hover for the repository's
uppercase value `let System = ...`, whose runtime oracle returns false. That
result remains a correctness blocker until the next immutable candidate passes;
there is no accepted 16/16 claim for ff85.

`extension/test/preparedHover.test.cjs` tests the same genuine artifacts through
the compiled adapter and actual native process, using an editor API double.
Set `FSHARP_PREPARED_FIXTURE` and `FSHARP_BACKEND_PATH` to run it. It explicitly
skips when artifacts are absent and is never labeled an actual VS Code host test.
The ordinary native adapter test now correctly rejects a synthetic catalog-only
hover instead of treating an unproven catalog match as an exact library binding.

The corrected immutable native candidate
`9201ad3fdb0f628eb69d3213cf0021646d127a4c6eb747ade3f1a45dd305520a`
passed all 16 current-ownership checks. Required framework positives remain exact;
repository module hover is suppressed; unsupported source-value and external
shadow cases are unavailable. Zero wrong-exact answers and zero helper/compiler
launches were observed. The full report is
`test-results/resolution-states/native-9201ad3f-current-ownership.json`.

The genuine prepared adapter test then passed its eight scenario subtests (nine
Node tests including the parent), recorded in `prepared-adapter-9201.log` beside
the native report. It confirms provider results, untrusted Markdown, explicit
incomplete status and no hidden compiler process through the actual adapter and
native binary. It remains an editor API double; no actual VS Code host run is
claimed. These safety/positive controls do not establish broad feature parity.

The schema2 metadata generation was tested independently in a fresh nine-project
fixture using immutable helper
`f215dac4599cb436d748ead64c01cf5f8795d8ddea79add66bb29cfdb6b763c2`.
All nine compiled runtime oracles still match. Native candidate
`0a77bde1efcd0860b8804f963ac602c123308931da12a4ad3dc7ea0cbd68598b`
passes the same 16 native controls and eight prepared adapter scenarios. The
framework positives require exact answers; unavailable shadow cases remain
safety checks. Reports are `native-0a77bde-f215-schema2.json`,
`prepared-adapter-0a77bde-schema2.log` and `certificates-f215-schema2.json` in
`test-results/resolution-states`. The certificate report records the exact new
model, complete-inventory and catalog hashes. Earlier fixtures and reports were
preserved; no artifact's schema or provenance was manually upgraded.
