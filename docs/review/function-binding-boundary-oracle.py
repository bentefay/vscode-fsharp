#!/usr/bin/env python3
"""Validate lexical body/export boundaries independently with FSI and FCS."""
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
    'unrelated_task_function': ('marker', '''let work () = task {
    let! value = System.Threading.Tasks.Task.FromResult 1
    return value }
'''),
    'task_local_nameof': ('marker', '''let work () = task {
    let! nameof = System.Threading.Tasks.Task.FromResult (fun (x:int) -> "inner:" + string x)
    return nameof 42 }
'''),
    'task_local_escaped_nameof': ('marker', '''let work () = task {
    let! ``nameof`` = System.Threading.Tasks.Task.FromResult (fun (x:int) -> "inner:" + string x)
    return ``nameof`` 42 }
'''),
    'nested_recursive_nameof': ('marker', '''let work () =
    let rec loop x = if x=0 then nameof x else loop (x-1)
    and nameof (x:int) : string = "inner:" + string x
    loop 1
'''),
    'body_pattern_nameof': ('marker', '''let work () =
    let (nameof, other) = ((fun (x:int) -> "inner:" + string x),0)
    nameof other
'''),
    'opened_function_module': ('marker', '''module Body =
    let work () =
        let nameof (x:int) = "inner:" + string x
        nameof 42
open Body
'''),
    'auto_open_function_module': ('marker', '''[<AutoOpen>]
module Body =
    let work () =
        let nameof (x:int) = "inner:" + string x
        nameof 42
'''),
    'auto_open_exported_header': ('exported:42', '''[<AutoOpen>]
module Body =
    let work () = 1
    let nameof (x:int) = "exported:" + string x
'''),
    'module_pattern_exports_nameof': ('pattern:42', '''let (nameof, other) = ((fun (x:int) -> "pattern:" + string x),0)
'''),
    'recursive_group_exports_nameof': ('group:42', '''let rec work x : string = nameof x
and nameof (x:int) : string = "group:" + string x
'''),
}


def main():
    fixture = Path(tempfile.mkdtemp(prefix='function-binding-boundary-', dir=ROOT / '.tmp'))
    rows = []
    for name, (expected, prefix) in CASES.items():
        source = prefix + 'let marker=42\nlet observed = nameof marker\nprintfn "%s" observed\n'
        path = fixture / (name + '.fsx')
        path.write_text(source)
        line = source.splitlines().index('let observed = nameof marker')
        queries = [dict(label='nameof_function', token='nameof', line=line, character=15), dict(label='nameof_argument', token='marker', line=line, character=22)]
        process = subprocess.run([str(DOTNET), 'fsi', '--exec', str(path)], env=ENV, text=True, capture_output=True, timeout=30)
        rows.append(dict(case=name, path=str(path), source=source, expectedStdout=expected, queries=queries, execution=dict(exitCode=process.returncode, stdout=process.stdout, stderr=process.stderr), runtimeMatches=process.returncode == 0 and process.stdout.strip() == expected))
        print(name, process.returncode, process.stdout.strip(), flush=True)
    manifest = fixture / 'cases.json'
    manifest.write_text(json.dumps(rows))
    process = subprocess.run([str(DOTNET), 'fsi', '--exec', str(ROOT / 'docs/review/alias-expression-oracle.fsx'), str(manifest)], env=ENV, text=True, capture_output=True, timeout=120)
    decoded = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{')]
    report = dict(provenance='Actual valid compiler controls for lexical function-body versus exported-header binding contributions. No native or general inference acceptance.', dotnetSha256=hashlib.sha256(DOTNET.read_bytes()).hexdigest(), fixtureRoot=str(fixture), cases=rows, fcsProcess=dict(exitCode=process.returncode, stderr=process.stderr), fcs=decoded)
    (ROOT / 'docs/review/function-binding-boundary-oracle.json').write_text(json.dumps(report, indent=2) + '\n')
    assert process.returncode == 0 and len(decoded) == len(rows), process.stderr
    assert all(row['runtimeMatches'] for row in rows), [(row['case'], row['execution']) for row in rows if not row['runtimeMatches']]
    assert all(row.get('valid') for row in decoded), [(row.get('case'), row.get('diagnostics')) for row in decoded if not row.get('valid')]
    print('all compiler controls passed', len(rows))


if __name__ == '__main__':
    main()
