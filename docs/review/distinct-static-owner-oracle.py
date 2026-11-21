#!/usr/bin/env python3
"""Compiler controls for negative same-name occurrence proofs in record factories."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT / '.tools/dotnet/dotnet'
BASE = Path(tempfile.mkdtemp(prefix='distinct-static-owner-', dir=ROOT / '.tmp'))
FIRST = 'type First = { Name: string }\n'
OTHER = 'type Other =\n    static member Empty = "other"\n'


def record_factory(body, before='', after=''):
    return 'type Factory =\n    { Name: string }\n' + before + '    static member Empty =\n' + ''.join('        ' + line + '\n' for line in body.splitlines()) + '        { Name = "x" }\n' + after


cases = [
    ('distinct_source_property', FIRST + OTHER + record_factory('let ignored = Other.Empty'), True, 'Factory'),
    ('external_string_field', FIRST + record_factory('let ignored = System.String.Empty'), True, 'Factory'),
    ('missing_external_member', FIRST + record_factory('let ignored = System.Object.Empty'), False, 'Factory'),
    ('missing_source_member', FIRST + 'type Other = class end\n' + record_factory('let ignored = Other.Empty'), False, 'Factory'),
    ('distinct_overloaded_method', FIRST + 'type Other =\n    static member Empty(x:int) = x\n    static member Empty(x:string) = x\n' + record_factory('let ignored = Other.Empty(1)'), True, 'Factory'),
    ('distinct_source_alias', FIRST + 'type Target =\n    static member Empty = "target"\ntype Other = Target\n' + record_factory('let ignored = Other.Empty'), True, 'Factory'),
    ('earlier_current_member_constraint', FIRST + OTHER + record_factory('let ignored = Other.Empty', '    static member Force() : First = Factory.Empty\n'), True, 'First'),
    ('later_current_member_constraint', FIRST + OTHER + record_factory('let ignored = Other.Empty', after='    static member Force() : First = Factory.Empty\n'), False, 'Factory'),
    ('current_member_passed_to_distinct_method', FIRST + 'type Other =\n    static member Empty(x:First) = x\n' + record_factory('let ignored = "x"', '    static member Force() = Other.Empty(Factory.Empty)\n'), True, 'First'),
    ('companion_module', FIRST + OTHER + 'module Other =\n    let Empty = "module"\n' + record_factory('let ignored = Other.Empty'), True, 'Factory'),
    ('local_value_shadow', FIRST + OTHER + 'type Proxy() =\n    member _.Empty = "value"\n' + record_factory('let Other = Proxy()\nlet ignored = Other.Empty'), True, 'Factory'),
    ('mutual_alias_to_current_owner', FIRST + OTHER + record_factory('let ignored = Other.Empty', '    static member Force() : First = Alias.Empty\n') + 'and Alias = Factory\n', True, 'First'),
    ('recursive_module_current_constraint', 'module rec Scope =\n' + ''.join('    ' + line + '\n' for line in (FIRST + OTHER + 'let force () : First = Factory.Empty\n' + record_factory('let ignored = Other.Empty')).splitlines()), True, 'First'),
    ('inherited_current_static_member', FIRST + 'type Second = { Name: string }\ntype Factory() =\n    static member Force() : First = Child.Empty\n    static member Empty = { Name = "x" }\nand Child() =\n    inherit Factory()\n', True, 'First'),
]
manifest = []
for name, source, valid, owner in cases:
    path = BASE / (name + '.fsx')
    path.write_text(source)
    lines = source.splitlines()
    tail = next(i for i, line in enumerate(lines) if '{ Name = "x" }' in line)
    queries = [dict(label='tail-field', token='Name', line=tail, character=lines[tail].index('Name'))]
    for i, line in enumerate(lines):
        for qualifier in ['Other', 'System.String', 'System.Object', 'Factory', 'Alias', 'Child']:
            spelling = qualifier + '.Empty'
            if spelling in line:
                queries.append(dict(label=f'{qualifier}-Empty-line{i}', token='Empty', line=i, character=line.index(spelling) + len(qualifier) + 1))
    manifest.append(dict(case=name, source=source, path=str(path), expectedValid=valid, expectedTailOwner=owner, queries=queries))
input_path = BASE / 'input.json'
input_path.write_text(json.dumps(manifest))
env = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1', TMPDIR=str(ROOT / '.tmp'))
process = subprocess.run([str(DOTNET), 'fsi', '--exec', str(ROOT / 'docs/review/alias-expression-oracle.fsx'), str(input_path)], env=env, text=True, capture_output=True, timeout=120)
results = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{')]
assert len(results) == len(manifest), process.stderr
for case, result in zip(manifest, results):
    assert case['case'] == result['case']
    case['oracle'] = result
    field = next(row for row in result['results'] if row['label'] == 'tail-field')['symbol']
    name = field['fullName'] if field else None
    case['expectationMatched'] = result['valid'] == case['expectedValid'] and name is not None and name.endswith('.' + case['expectedTailOwner'] + '.Name')
    print(case['case'], 'valid=' + str(result['valid']), name, 'matched=' + str(case['expectationMatched']))
report = dict(provenance='Actual pinned FCS checks, including semantic-error recovery cases. No native support or performance claim. Distinct owner proves only exclusion from the current member identity; it does not prove that the other member exists or infer its value.', fixtureRoot=str(BASE), dotnetSha256=hashlib.sha256(DOTNET.read_bytes()).hexdigest(), cases=manifest, compiler=dict(exitCode=process.returncode, stderr=process.stderr), summary=dict(total=len(manifest), expectationMatched=sum(case['expectationMatched'] for case in manifest)))
(ROOT / 'docs/review/distinct-static-owner-oracle.json').write_text(json.dumps(report, indent=2) + '\n')
