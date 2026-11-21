#!/usr/bin/env python3
"""Actual compiler controls for narrow operator declaration name-absence facts."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT / '.tools/dotnet/dotnet'
ENV = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), TMPDIR=str(ROOT / '.tmp'), DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1')
CASES = {
    'ordinary_operator': ('marker', 'let (>>=) x f = f x\n'),
    'local_nameof_parameter': ('marker', 'let (>>=) nameof f = f nameof\n'),
    'escaped_local_nameof': ('marker', 'let (>>=) x f =\n    let ``nameof`` (_:int) = "inner"\n    f (``nameof`` x)\n'),
    'recursive_group_nameof': ('custom:42', 'let rec (>>=) (x:int) (f:int->int) = if x=0 then f x else (nameof x).Length\nand nameof (x:int) : string = "custom:" + string x\n'),
    'recursive_group_escaped_nameof': ('escaped:42', 'let rec (>>=) (x:int) (f:int->int) = if x=0 then f x else (``nameof`` x).Length\nand ``nameof`` (x:int) : string = "escaped:" + string x\n'),
    'compiled_name_attribute': ('marker', '[<CompiledName("nameof")>]\nlet (>>=) x f = f x\n'),
    'parent_value_then_core_operators_open': ('marker', 'let nameof (x:int) = "outer:" + string x\nmodule Operators =\n    let (>>=) x f = f x\nopen Operators\n'),
    'parent_value_shadow': ('outer:42', 'let nameof (x:int) = "outer:" + string x\nmodule CustomOps =\n    let (>>=) x f = f x\nopen CustomOps\n'),
    'opened_module_value_shadow': ('opened:42', 'module Operators =\n    let (>>=) x f = f x\n    let nameof (x:int) = "opened:" + string x\nopen Operators\n'),
    'recursive_module_value_shadow': ('recursive:42', 'module rec Operators =\n    let (>>=) x f = f x\n    let nameof (x:int) = "recursive:" + string x\nopen Operators\n'),
    'unopened_module_value_control': ('marker', 'module Operators =\n    let (>>=) x f = f x\n    let nameof (x:int) = "closed:" + string x\n'),
}


def main():
    fixture = Path(tempfile.mkdtemp(prefix='operator-name-absence-', dir=ROOT / '.tmp'))
    rows = []
    for name, (expected, prefix) in CASES.items():
        source = prefix + 'let marker=42\nlet observed = nameof marker\nprintfn "%s" observed\n'
        path = fixture / (name + '.fsx')
        path.write_text(source)
        line = source.splitlines().index('let observed = nameof marker')
        queries = [dict(label='nameof_function', token='nameof', line=line, character=15), dict(label='nameof_argument', token='marker', line=line, character=22)]
        result = subprocess.run([str(DOTNET), 'fsi', '--exec', str(path)], env=ENV, text=True, capture_output=True, timeout=30)
        row = dict(case=name, path=str(path), source=source, expectedStdout=expected, queries=queries, execution=dict(exitCode=result.returncode, stdout=result.stdout, stderr=result.stderr))
        row['runtimeMatches'] = result.returncode == 0 and result.stdout.strip() == expected
        rows.append(row)
        print(name, result.returncode, result.stdout.strip(), flush=True)
    manifest = fixture / 'cases.json'
    manifest.write_text(json.dumps(rows))
    driver = ROOT / 'docs/review/alias-expression-oracle.fsx'
    result = subprocess.run([str(DOTNET), 'fsi', '--exec', str(driver), str(manifest)], env=ENV, text=True, capture_output=True, timeout=120)
    decoded = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
    report = dict(provenance='Independent actual FSI execution and FCS symbol/goto/ref oracle. Name-absence certificate review only; no native acceptance or type-inference relaxation.', dotnetSha256=hashlib.sha256(DOTNET.read_bytes()).hexdigest(), fixtureRoot=str(fixture), cases=rows, fcsProcess=dict(exitCode=result.returncode, stderr=result.stderr), fcs=decoded)
    (ROOT / 'docs/review/operator-name-absence-oracle.json').write_text(json.dumps(report, indent=2) + '\n')
    assert result.returncode == 0 and len(decoded) == len(rows), result.stderr
    assert all(row['runtimeMatches'] for row in rows), [(row['case'], row['execution']) for row in rows if not row['runtimeMatches']]
    assert all(row.get('valid') for row in decoded), [(row.get('case'), row.get('diagnostics')) for row in decoded if not row.get('valid')]
    print('all compiler controls passed', len(rows))


if __name__ == '__main__':
    main()
