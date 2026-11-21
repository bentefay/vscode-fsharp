#!/usr/bin/env python3
"""Independent FCS controls; no engine or fixed workload modifications."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'evaluation/experiments/unique-field'
HEAD = 'type R = { OnlyLabel:int }\n'
CASES = {
    'plain_unique_record': HEAD + 'let value = { OnlyLabel=1 }\n',
    'unique_record_annotated': HEAD + 'let value:R = { OnlyLabel=1 }\n',
    'unannotated_member_receiver': HEAD + 'let read value = value.OnlyLabel\n',
    'incompatible_expected_record': HEAD + 'type S = { Other:int }\nlet value:S = { OnlyLabel=1; Other=2 }\n',
    'incompatible_expected_record_single_label': HEAD + 'type S = { Other:int }\nlet value:S = { OnlyLabel=1 }\n',
    'incompatible_expected_class': HEAD + 'type C() = member _.OnlyLabel = 2\nlet value:C = { OnlyLabel=1 }\n',
    'missing_record_field': 'type R = { OnlyLabel:int; Other:int }\nlet value = { OnlyLabel=1 }\n',
    'wrong_value_type': HEAD + 'let value = { OnlyLabel="bad" }\n',
    'unopened_module_label': 'module Hidden =\n    type R = { OnlyLabel:int }\nlet value = { OnlyLabel=1 }\n',
    'private_representation': 'module Hidden =\n    type R = private { OnlyLabel:int }\nopen Hidden\nlet value = { OnlyLabel=1 }\n',
    'qualified_access_required': '[<RequireQualifiedAccess>]\ntype R = { OnlyLabel:int }\nlet value = { OnlyLabel=1 }\n',
    'class_member_collision': HEAD + 'type C() = member _.OnlyLabel = 2\nlet read (value:C) = value.OnlyLabel\n',
    'extension_member_collision': HEAD + 'type C() = class end\ntype C with\n    member _.OnlyLabel = 2\nlet read (value:C) = value.OnlyLabel\n',
    'anonymous_record_collision': HEAD + 'let value = {| OnlyLabel=2 |}\nlet read = value.OnlyLabel\n',
    'record_alias_same_identity': HEAD + 'type Alias = R\nlet value:Alias = { OnlyLabel=1 }\nlet read = value.OnlyLabel\n',
    'copy_update_wrong_receiver': HEAD + 'type S = { Other:int }\nlet other = { Other=2 }\nlet value = { other with OnlyLabel=1 }\n',
    'srtp_member_constraint': HEAD + 'let inline read (value:^T) = ((^T) : (member OnlyLabel:int) value)\n',
    'unknown_label_value': HEAD + 'let value = { OnlyLabel=missing }\n',
    'error_expected_anonymous': HEAD + 'let value:{| OnlyLabel:int |} = { OnlyLabel=1 }\n',
    'hidden_srtp_expected_other_record': HEAD + 'type S = { Other:int }\nlet inline constrain< ^T when ^T : (static member Create: unit -> S)> () = ()\ntype Factory =\n    static member Create() = { OnlyLabel=1 }\n    static member Force() = constrain<Factory>()\n',
}

def main():
    BASE.mkdir(parents=True, exist_ok=True)
    rows = []
    for name, source in CASES.items():
        path = BASE / (name + '.fsx')
        path.write_text(source)
        queries = [dict(label=f'OnlyLabel_{line}_{n}', token='OnlyLabel', line=line, character=m.start())
                   for line, text in enumerate(source.splitlines())
                   for n, m in enumerate(re.finditer(r'\bOnlyLabel\b', text))]
        rows.append(dict(case=name, path=str(path), source=source, queries=queries))
    manifest = BASE / 'cases.json'
    manifest.write_text(json.dumps(rows, indent=2) + '\n')
    dotnet = ROOT / '.tools/dotnet/dotnet'
    oracle = ROOT / 'docs/review/alias-expression-oracle.fsx'
    env = dict(os.environ, DOTNET_ROOT=str(dotnet.parent), TMPDIR=str(ROOT / '.tmp'))
    result = subprocess.run([str(dotnet), 'fsi', '--exec', str(oracle), str(manifest)],
                            env=env, text=True, capture_output=True, timeout=180)
    answers = [json.loads(x) for x in result.stdout.splitlines() if x.startswith('{')]
    report = dict(kind='independent-FCS-unique-record-field-counterexamples', cases=rows,
                  answers=answers, exitCode=result.returncode, stderr=result.stderr,
                  dotnetSha256=hashlib.sha256(dotnet.read_bytes()).hexdigest(),
                  oracleSha256=hashlib.sha256(oracle.read_bytes()).hexdigest(),
                  fixedWorkloadModified=False, performanceAcceptance=False)
    output = ROOT / 'evaluation/results/unique-field-fcs-controls.json'
    output.write_text(json.dumps(report, indent=2) + '\n')
    assert result.returncode == 0 and len(answers) == len(rows), result.stderr
    for row in answers:
        print(row['case'], 'valid=' + str(row['valid']),
              [(x['line'], (x['symbol'] or {}).get('kind'), (x['symbol'] or {}).get('fullName')) for x in row['results']])

if __name__ == '__main__':
    main()
