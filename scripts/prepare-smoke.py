#!/usr/bin/env python3
"""Exercise real CLI preparation and native import without an editor session."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
dotnet = os.environ.get('FSHARP_NAVIGATOR_DOTNET', 'dotnet')


def run(args, **options):
    result = subprocess.run(args, cwd=root, text=True, capture_output=True, timeout=120, **options)
    assert result.returncode == 0, result.stdout + result.stderr
    return result.stdout


sdks = run([dotnet, '--list-sdks']).splitlines()
version = json.loads((root / 'global.json').read_text())['sdk']['version']
sdk = next(line for line in sdks if line.startswith(version + ' '))
fsharp_core = Path(sdk.split('[', 1)[1].rsplit(']', 1)[0]) / version / 'FSharp/FSharp.Core.dll'
with tempfile.TemporaryDirectory(prefix='fsharp-navigator-prepare-') as temporary:
    base = Path(temporary)
    workspace = base / 'workspace'
    workspace.mkdir()
    project = workspace / 'Main.fsproj'
    project.write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DisableImplicitFSharpCoreReference>true</DisableImplicitFSharpCoreReference><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="FSharp.Core"><HintPath>$(MSBuildToolsPath)/FSharp/FSharp.Core.dll</HintPath></Reference><Compile Include="Main.fs"/></ItemGroup></Project>')
    source = workspace / 'Main.fs'
    source.write_text('module Main\nlet twice x = x + x\nlet result = twice 21\ntype Entry = { Value: int }\nlet entry = { Value = 42 }\nlet amount = entry.Value\n')
    run([dotnet, 'build', str(project), '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false'], env=dict(os.environ, MSBUILDDISABLENODEREUSE='1', DOTNET_CLI_USE_MSBUILD_SERVER='0'))
    output = base / 'prepared'
    run([str(root / 'cli/bin/run'), 'prepare', '--root', str(workspace), '--out', str(output), '--assembly', str(fsharp_core), '--semantic-snapshot', '--metrics', str(output / 'metrics.json')])
    settings = json.loads((output / 'settings.json').read_text())
    assert settings['fsharpNavigator.mode'] == 'indexed'
    environment = json.loads(Path(settings['fsharpNavigator.builtinEnvironmentPath']).read_text())
    assert environment['factsVersion'] == 2 and environment['typeNameInventory'] == 'all-external-type-declarations'
    catalog = json.loads((output / 'libraries.json').read_text())
    assert catalog['catalogs']
    symbols = [symbol for shard in catalog['catalogs'] for symbol in json.loads((output / shard).read_text())['symbols']]
    assert any('Microsoft.FSharp.Collections.List.map' in symbol['qualifiedNames'] for symbol in symbols)
    metrics = json.loads((output / 'metrics.json').read_text())
    assert [phase['method'] for phase in metrics['phases']] == ['workspace/evaluate', 'metadata/export', 'metadata/environment', 'semantic/export']
    assert all(phase['wallMs'] > 0 for phase in metrics['phases'])
    params = {'root': str(workspace), 'mode': 'indexed', 'cacheDir': str(base / 'index')}
    params.update({key.split('.', 1)[1]: value for key, value in settings.items() if key.endswith('Path')})
    requests = [
        {'id': 1, 'method': 'workspace/open', 'params': params},
        {'id': 2, 'method': 'navigation/definition', 'params': {'path': str(source), 'line': 2, 'character': 14}},
        {'id': 3, 'method': 'navigation/references', 'params': {'path': str(source), 'line': 2, 'character': 14, 'includeDeclaration': True}},
        {'id': 4, 'method': 'navigation/definition', 'params': {'path': str(source), 'line': 5, 'character': 21}},
        {'id': 5, 'method': 'workspace/stats', 'params': {}},
        {'id': 6, 'method': 'shutdown', 'params': {}},
    ]
    replies = [json.loads(line) for line in run([str(root / 'target/release/fsharp-navigator'), 'daemon'], input=''.join(json.dumps(request) + '\n' for request in requests)).splitlines()]
    assert all('error' not in reply for reply in replies), replies
    definition, references, field, stats = replies[1]['result'], replies[2]['result'], replies[3]['result'], replies[4]['result']
    assert definition['completeness'] == 'exact' and definition['locations'][0]['range']['start'] == {'line': 1, 'character': 4}, definition
    assert field['source'] == 'persisted-semantic' and field['completeness'] == 'exact', field
    assert field['locations'][0]['range']['start'] == {'line': 3, 'character': 15}, field
    assert references['completeness'] == 'exact' and len(references['locations']) == 2, references
    assert stats['helperStarts'] == 0 and stats['compilerLaunches'] == 0, stats
    print('PASS CLI project evaluation, FSharp.Core catalog, explicit semantic export, settings generation, metrics and native indexed import/query without compiler launches')
