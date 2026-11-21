#!/usr/bin/env python3
"""Real SDK regression: project shape, binary signatures and semantic snapshots differ."""
import hashlib
import json
import contextlib
import os
import pathlib
import subprocess
import tempfile

from smoke import ROOT, DOTNET, ENV, run, request, project


def valid(stamps):
    return all((hashlib.sha256(pathlib.Path(s['path']).read_bytes()).hexdigest()
                if pathlib.Path(s['path']).is_file() else 'directory' if pathlib.Path(s['path']).is_dir() else 'missing') == s['sha256'] for s in stamps)


def native(root, model, cache, method='workspace/tree', params=None, catalog=None):
    options = {'root': str(root), 'mode': 'indexed', 'evaluatedModelPath': str(model), 'cacheDir': str(cache)}
    if catalog:
        options['libraryCatalogPath'] = str(catalog)
    messages = [
        {'id': 1, 'method': 'workspace/open', 'params': options},
        {'id': 2, 'method': method, 'params': params or {}},
        {'id': 3, 'method': 'shutdown', 'params': {}},
    ]
    output = subprocess.run([str(ROOT / 'target/release/fsharp-navigator'), 'daemon'], input=''.join(json.dumps(m)+'\n' for m in messages), text=True, capture_output=True, check=True, timeout=30)
    return json.loads(output.stdout.splitlines()[1])


def main():
    retained = os.environ.get('FSHARP_CERTIFICATE_FIXTURE_DIR')
    if retained:
        pathlib.Path(retained).mkdir(parents=True, exist_ok=False)
    with (contextlib.nullcontext(retained) if retained else tempfile.TemporaryDirectory(prefix='fsharp-certificates-')) as tmp:
        base = pathlib.Path(tmp)
        workspace = base / 'workspace'
        lib = workspace / 'library'
        lib.mkdir(parents=True)
        source = lib / 'Library.fs'
        source.write_text('namespace Workspace\nmodule Library =\n    let value = 1\n')
        library = lib / 'Library.fsproj'
        project(library, '<Compile Include="Library.fs"/>')
        consumer = workspace / 'consumer'
        consumer.mkdir()
        consumer_source = consumer / 'Consumer.fs'
        consumer_source.write_text('module Consumer\nlet value = Workspace.Library.value\nlet mapped = Microsoft.FSharp.Collections.List.map id [1]\n')
        consumer_project = consumer / 'Consumer.fsproj'
        project(consumer_project, '<Compile Include="Consumer.fs"/>', '<ProjectReference Include="../library/Library.fsproj"/>')
        run('build', consumer_project, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')
        model = request('workspace/evaluate', {'root': str(workspace)})
        model_path = base / 'model.json'
        model_path.write_text(json.dumps(model))
        assert model['inputHashes'] == model['projectInputHashes'] and valid(model['inputHashes'])
        references = sorted({p for item in model['projects'] for p in item['assemblyReferences'] if pathlib.Path(p).is_file()})
        core = next(p for p in references if p.endswith('/FSharp.Core.dll'))
        catalog = request('metadata/export', {'assemblyPaths': [core], 'referencePaths': references})
        catalog_path = base / 'catalog.json'
        catalog_path.write_text(json.dumps(catalog))
        assert valid(catalog['inputHashes'])
        assert not any('/workspace/' in stamp['path'][len(str(base)):] for stamp in catalog['inputHashes']), catalog['inputHashes']
        assert catalog['dependencyBindings'], 'Dependency resolution must be recorded'
        before = native(workspace, model_path, base / 'index-before')
        assert 'error' not in before and before['result']['completeness'] == 'exact', before
        hover_params = {'path': str(consumer_source), 'line': 2, 'character': 48}
        hover = native(workspace, model_path, base / 'hover-before', 'navigation/hover', hover_params, catalog_path)
        assert hover.get('result', {}).get('hover') and hover['result']['completeness'] == 'exact', hover

        source.write_text('namespace Workspace\nmodule Library =\n    let value = 2\n    let newPublicValue = 3\n')
        run('build', consumer_project, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')
        assert valid(model['inputHashes']), 'Ordinary DLL rebuild invalidated project shape'
        assert not valid(model['semanticInputHashes']), 'DLL rebuild must invalidate semantic facts'
        assert valid(catalog['inputHashes']), 'Unrelated workspace DLL invalidated FSharp.Core catalog'
        after = native(workspace, model_path, base / 'index-after')
        assert 'error' not in after and after['result']['completeness'] == 'exact', after
        reused_hover = native(workspace, model_path, base / 'hover-before', 'navigation/hover', hover_params, catalog_path)
        assert reused_hover.get('result', {}).get('hover') and reused_hover['result']['completeness'] == 'exact', reused_hover
        print('PASS real workspace rebuild preserves evaluated tree and native external hover, invalidates semantic bytes', flush=True)

        # A root assembly that actually references the workspace library must
        # retain that binary in its own recursive certificate.
        exported = consumer / 'bin/Debug/net10.0/Consumer.dll'
        dependent = request('metadata/export', {'assemblyPaths': [str(exported)], 'referencePaths': references})
        (base / 'dependent-catalog.json').write_text(json.dumps(dependent))
        assert any(stamp['path'].endswith('/Library.dll') for stamp in dependent['inputHashes']), dependent
        relevant = pathlib.Path(next(stamp['path'] for stamp in dependent['inputHashes'] if stamp['path'].endswith('/Library.dll')))
        original = relevant.read_bytes()
        relevant.write_bytes(original + b'changed')
        assert not valid(dependent['inputHashes']) and valid(catalog['inputHashes'])
        relevant.write_bytes(original)
        print('PASS transitive selected binary dependency changes reject only affected certificate', flush=True)

        # An imported target/config input is a project-shape dependency even
        # when it did not exist at evaluation time.
        props = workspace / 'Directory.Build.props'
        props.write_text('<Project><PropertyGroup><DefineConstants>ADDED</DefineConstants></PropertyGroup></Project>')
        assert not valid(model['inputHashes'])
        stale = native(workspace, model_path, base / 'index-stale')
        assert 'error' in stale or stale.get('result', {}).get('completeness') != 'exact', stale
        props.unlink()
        project_before = library.read_text()
        library.write_text(project_before.replace('</Project>', '<PropertyGroup><DefineConstants>CHANGED</DefineConstants></PropertyGroup></Project>'))
        assert not valid(model['inputHashes'])
        library.write_text(project_before)
        print('PASS project/import creation or edits reject shape certificate', flush=True)

        # Units of measure disappear from IL parameter types but remain in F#
        # signatures. This guards against assuming PE closure captures them.
        measures = base / 'measures'
        measures.mkdir()
        (measures / 'Measures.fs').write_text('namespace Measures\n[<Measure>] type kg\n')
        measures_project = measures / 'Measures.fsproj'
        project(measures_project, '<Compile Include="Measures.fs"/>')
        api = base / 'measure-api'
        api.mkdir()
        (api / 'Api.fs').write_text('namespace MeasureApi\nmodule Api =\n    let mass (value: float<Measures.kg>) = value\n')
        api_project = api / 'Api.fsproj'
        project(api_project, '<Compile Include="Api.fs"/>', '<ProjectReference Include="../measures/Measures.fsproj"/>')
        run('build', api_project, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')
        measure_model = request('workspace/evaluate', {'root': str(api), 'projects': [str(api_project)]})
        measure_refs = sorted({p for item in measure_model['projects'] for p in item['assemblyReferences'] if pathlib.Path(p).is_file()})
        api_dll = api / 'bin/Debug/net10.0/Api.dll'
        measure_catalog = request('metadata/export', {'assemblyPaths': [str(api_dll)], 'referencePaths': measure_refs})
        (base / 'measure-catalog.json').write_text(json.dumps(measure_catalog))
        assert any(stamp['path'].endswith('/Measures.dll') for stamp in measure_catalog['inputHashes']), 'Erased F# signature dependency omitted: ' + json.dumps({'inputs': [pathlib.Path(s['path']).name for s in measure_catalog['inputHashes']], 'signatures': [s['signature'] for s in measure_catalog['symbols']]})
        (measures / 'Measures.fs').write_text('namespace Measures\n[<Measure>] type kg\n[<Measure>] type lb\n')
        run('build', measures_project, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')
        assert not valid(measure_catalog['inputHashes']), 'Rebuilt erased-measure dependency must invalidate catalog'
        print('PASS erased F# unit-of-measure signature dependency retained and rebuild rejected', flush=True)
        alias_b = base / 'alias-b'
        alias_b.mkdir()
        (alias_b / 'Types.fs').write_text('namespace AliasB\ntype Amount = float<Measures.kg>\n')
        alias_b_project = alias_b / 'AliasB.fsproj'
        project(alias_b_project, '<Compile Include="Types.fs"/>', '<ProjectReference Include="../measures/Measures.fsproj"/>')
        alias_a = base / 'alias-a'
        alias_a.mkdir()
        (alias_a / 'Types.fs').write_text('namespace AliasA\ntype Amount = AliasB.Amount\n')
        alias_a_project = alias_a / 'AliasA.fsproj'
        project(alias_a_project, '<Compile Include="Types.fs"/>', '<ProjectReference Include="../alias-b/AliasB.fsproj"/>')
        (api / 'Api.fs').write_text('namespace MeasureApi\nmodule Api =\n    let mass (value: AliasA.Amount) = value\n')
        project(api_project, '<Compile Include="Api.fs"/>', '<ProjectReference Include="../alias-a/AliasA.fsproj"/>')
        run('build', api_project, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')
        aliases_model = request('workspace/evaluate', {'root': str(api), 'projects': [str(api_project)]})
        aliases_refs = sorted({p for item in aliases_model['projects'] for p in item['assemblyReferences'] if pathlib.Path(p).is_file()})
        alias_catalog = request('metadata/export', {'assemblyPaths': [str(api_dll)], 'referencePaths': aliases_refs})
        names = {pathlib.Path(s['path']).name for s in alias_catalog['inputHashes']}
        assert {'AliasA.dll', 'AliasB.dll', 'Measures.dll'} <= names, names
        (base / 'alias-catalog.json').write_text(json.dumps(alias_catalog))
        (alias_b / 'Types.fs').write_text('namespace AliasB\ntype Amount = float<Measures.kg>\ntype Other = float<Measures.lb>\n')
        run('build', alias_b_project, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false', '-m:1', '-p:BuildInParallel=false')
        assert not valid(alias_catalog['inputHashes']), 'Intermediate signature-only alias assembly rebuild must invalidate catalog'
        print('PASS every nominal alias-chain assembly retained and intermediate rebuild rejected', flush=True)
        if retained:
            (base / 'review-state.json').write_text(json.dumps({'modelValid': valid(model['inputHashes']), 'coreCatalogValid': valid(catalog['inputHashes']), 'dependentCatalogValid': valid(dependent['inputHashes']), 'measureCatalogValid': valid(measure_catalog['inputHashes']), 'expectedMeasureCatalogStaleAfterRebuild': True}, indent=2))
            print('Retained review fixture:', base, flush=True)
    print('All dependency certificate checks passed', flush=True)


if __name__ == '__main__':
    main()
