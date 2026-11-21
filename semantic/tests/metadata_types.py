#!/usr/bin/env python3
"""Real binary-only structured facts and complete imported-name inventory."""
import json, pathlib, tempfile, hashlib
from smoke import ROOT, DOTNET, request, project, run


def main():
    with tempfile.TemporaryDirectory(prefix='fsharp-metadata-types-') as tmp:
        root = pathlib.Path(tmp)
        (root / 'Types.fs').write_text('''namespace External
// A source alias must remain a declaration distinct from its expanded target.
type DateOnly = System.DateTime
type internal HiddenType = { Value: int }
type Holder<'T>(value: 'T) =
    member _.Value = value
module Patterns =
    let (|Some|_|) (value: obj) : int option = None
module Cases =
    type internal Choice = Some of int
''')
        p = root / 'External.fsproj'
        project(p, '<Compile Include="Types.fs"/>')
        run('build', p, '--nologo', '-v:q', '-nodeReuse:false', '-p:UseSharedCompilation=false')
        model = request('workspace/evaluate', {'root': str(root)})
        refs = sorted({x for p in model['projects'] for x in p['assemblyReferences'] if pathlib.Path(x).is_file()})
        dll = root / 'bin/Debug/net10.0/External.dll'
        refs.append(str(dll))
        environment = request('metadata/environment', {'referencePaths': refs})
        assert environment['factsVersion'] == 2
        assert environment['typeNameInventory'] == 'all-external-type-declarations'
        alias = [x for x in environment['typeNames']['DateOnly'] if x['sourceFullName'] == 'External.DateOnly']
        assert len(alias) == 1 and alias[0]['identity']['assemblyPath'] == str(dll) and alias[0]['isAbbreviation'], alias
        assert any(x['sourceFullName'] == 'External.HiddenType' for x in environment['typeNames']['HiddenType'])
        assert {x['kind'] for x in environment['patterns']['Some']} >= {'unionCase', 'activePattern'}
        assert len(environment['patterns']['Some']) >= 3
        assert any(x['isBuiltin'] for x in environment['patterns']['Some'])
        print('PASS complete external aliases/internal types/active-pattern cases/union cases inventory', flush=True)
        core = next(x for x in refs if x.endswith('/FSharp.Core.dll'))
        runtime = next(x for x in refs if x.endswith('/System.Runtime.dll'))
        def catalog(assembly):
            result = request('metadata/export', {'assemblyPaths': [str(assembly)], 'referencePaths': refs})
            assert result['factsVersion'] == 2
            assert all(x['factsVersion'] == 2 for x in result['symbols'])
            return result
        c = catalog(dll)
        alias = next(x for x in c['symbols'] if x['fullName'] == 'External.DateOnly')
        assert alias['typeInfo']['identity']['assemblyPath'] == str(dll)
        assert alias['typeInfo']['type']['identity']['fullName'] == 'System.DateTime'
        prop = next(x for x in c['symbols'] if x['xmlDocId'].startswith('P:External.Holder') and x['name'] == 'Value' and x.get('memberInfo'))
        assert prop['memberInfo']['returnType'] == {'kind': 'typeParameter', 'index': 0}, prop
        assert prop['memberInfo']['parameterCount'] == 0 and not prop['memberInfo']['isStatic']
        key = prop['memberInfo']['declaringType']['fullName'] + '.Value'
        matching = [x for x in c['symbols'] if key in x['qualifiedNames']]
        assert len(matching) == 1 and matching[0]['memberInfo'], 'Accessors must not occupy the property lookup row'
        assert any(x['fullName'].endswith('.get_Value') for x in c['symbols']), 'Accessor retains its compiled spelling'
        print('PASS declaration identity retained before alias expansion; generic property slots preserved', flush=True)
        c = catalog(runtime)
        for doc, owner in [('P:System.String.Length', 'System.String'), ('P:System.DateTime.Year', 'System.DateTime'), ('P:System.Tuple`1.Item1', 'System.Tuple`1')]:
            prop = next(x for x in c['symbols'] if x['xmlDocId'] == doc)
            info = prop['memberInfo']
            assert info['supported'] and info['isProperty'] and info['parameterCount'] == 0 and not info['isStatic'], prop
            assert info['declaringType']['fullName'] == owner and info['declaringType']['assemblyPath'] == runtime, prop
        print('PASS framework forwarded identities and nominal instance property facts', flush=True)
        c = catalog(core)
        string = next(x for x in c['symbols'] if x['fullName'] == 'Microsoft.FSharp.Core.string')
        assert string['typeInfo']['identity']['assemblyPath'] == core
        assert string['typeInfo']['type']['identity']['fullName'] == 'System.String'
        assert string['typeInfo']['type']['identity']['assemblyPath'] == runtime
        assert not any(x['fullName'] == 'System.String.Length' for x in c['symbols']), 'Alias must not reexport target members with Core ownership'
        print('PASS primitive alias captures Core declaration and selected Runtime target without member ownership duplication', flush=True)
        print(json.dumps({'helperSha256': hashlib.sha256((ROOT / 'semantic/bin/Release/net10.0/FSharpNavigator.Semantic.dll').read_bytes()).hexdigest(), 'inventoryBytes': len(json.dumps(environment).encode()), 'visitedEntities': environment['visitedEntities']}), flush=True)

if __name__ == '__main__':
    main()
