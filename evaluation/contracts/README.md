# Frozen evaluation contracts

`grace46.json` preserves the original 30 hash-selected Grace sites: 23 source
sites (46 definition/reference operations), six external sites and one unmatched
candidate. Its 189 source reference locations, 23 definition targets and original
UTF-16 positions are copied from the canonical checkpoint capture. The
Sha256Hash group retains the direct-FCS override of exactly 53 occurrences.
Excluded candidates remain explicit; they must not be replaced with easier sites.

The corpus is `ScottArbeit/Grace` at
`39960e07b73cd142cc0c1fd070c0591be91c1933`; paths in the contract are relative to
that repository. The selected workspace is `src/Grace.Types`. The contract pins
all 20 source files used by the selected positions and scored answers. It also
records the original capture, selection-script and oracle evidence hashes.
Those observations are historical, not fresh semantic runs.

The extraction round-trip was checked against the restored canonical JSON:
all scored targets/ranges, query positions, ordering, classifications and
selection hashes are unchanged after rebasing. No engine was run for that check.

## Use with the existing replay driver

The replay driver currently takes absolute paths. Materialize a temporary
selection from the contract before calling it; do not edit the committed file.
From the repository root, with the pinned Grace checkout at the example path:

```sh
python3 - <<'PY'
import copy, hashlib, json, pathlib, subprocess
corpus = pathlib.Path('../research/corpus/Grace').resolve()
contract = json.loads(pathlib.Path('evaluation/contracts/grace46.json').read_text())
assert subprocess.check_output(['git', '-C', str(corpus), 'rev-parse', 'HEAD'], text=True).strip() == contract['corpus']['commit']
for name, expected in contract['corpus']['source_sha256'].items():
    assert hashlib.sha256((corpus / name).read_bytes()).hexdigest() == expected, name

def absolute(value):
    if isinstance(value, dict):
        return {key: str(corpus / item) if key == 'path' and isinstance(item, str) else absolute(item)
                for key, item in value.items()}
    if isinstance(value, list):
        return [absolute(item) for item in value]
    return value

selection = copy.deepcopy(contract['selection'])
selection['root'] = str(corpus / contract['corpus']['workspace'])
selection['selected'] = absolute(contract['selected'])
selection['contract_sha256'] = hashlib.sha256(pathlib.Path('evaluation/contracts/grace46.json').read_bytes()).hexdigest()
output = pathlib.Path('evaluation/.runtime/grace46-selection.json')
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(selection, indent=2) + '\n')
PY
```

After genuinely regenerating the selected project model and metadata, replay:

```sh
scripts/with-local-tools python3 evaluation/sample_trace.py \
  --selection evaluation/.runtime/grace46-selection.json \
  --engine 'PATH_TO_FROZEN_NATIVE daemon' --engine-sha NATIVE_SHA256 \
  --model PATH_TO_GENUINE_MODEL_JSON \
  --catalog PATH_TO_EXPORTED_CATALOG_JSON \
  --builtin-environment PATH_TO_EXPORTED_ENVIRONMENT_JSON \
  --project-path ../research/corpus/Grace/src/Grace.Types/Grace.Types.fsproj \
  --output evaluation/results/NEW_CANDIDATE-grace46.json
```

The placeholders must refer to actual producer outputs and an immutable binary.
Never hand-edit certificate acceptance fields or reuse stale host/path witnesses.
Run under the matching SDK/runtime/globalization environment. Record preparation
separately; no comparative timing is accepted during concurrent compilation.

The independent 100-query fixture remains in `fixtures/expectations.json` and
`fixtures/sources`. Use `evaluation/run.py` with `--correctness-only`,
`--repetitions 1`, `--include-declaration` and
`--reference-normalization vscode`. Materialize fixture source files into `WORK/fixtures` before preparing their
model, then pass `--work WORK`, because the runner appends `fixtures`. Do not change the fixture queries
or combine this denominator with Grace or supplemental local-fast-path sites.
