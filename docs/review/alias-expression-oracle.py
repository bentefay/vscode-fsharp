#!/usr/bin/env python3
"""Independent FSI execution and raw FCS binding/reference probes; no native claims."""
from pathlib import Path
import hashlib, json, os, re, subprocess, sys, tempfile

ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT / '.tools/dotnet/dotnet'
ENV = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), TMPDIR=str(ROOT / '.tmp'), DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1')
CASES = {
    'ordinary_alias': '''type Sha = string // @alias:Sha
let value : Sha = Sha "a" // @constructor:Sha
let name = nameof Sha // @nameof_argument:Sha @nameof_function:nameof
printfn "%s:%s" value name
''',
    'later_value_shadow': '''type Sha = string // @alias:Sha
let Sha (x:string) = "value:" + x // @value_declaration:Sha
let value : Sha = Sha "a" // @application:Sha
let name = nameof Sha // @nameof_argument:Sha
printfn "%s:%s" value name
''',
    'earlier_value_shadow': '''let Sha (x:string) = "value:" + x // @value_declaration:Sha
type Sha = string // @alias:Sha
let value : Sha = Sha "a" // @application:Sha
let name = nameof Sha // @nameof_argument:Sha
printfn "%s:%s" value name
''',
    'opened_value_shadow': '''type Sha = string // @alias:Sha
module Values =
    let Sha (x:string) = "opened:" + x // @value_declaration:Sha
open Values
let value : Sha = Sha "a" // @application:Sha
let name = nameof Sha // @nameof_argument:Sha
printfn "%s:%s" value name
''',
    'qualified_companion_value': '''type Sha = string // @alias:Sha
module Factory =
    type Sha = string // @nested_alias:Sha
    let Sha (x:string) = "qualified:" + x // @value_declaration:Sha
let value = Factory.Sha "a" // @application:Sha
let name = nameof Factory.Sha // @nameof_argument:Sha
printfn "%s:%s" value name
''',
    'later_union_case': '''type Sha = string // @alias:Sha
type Event = Sha of string // @case_declaration:Sha
let value = Sha "a" // @application:Sha
let name = nameof Sha // @nameof_argument:Sha
printfn "%A:%s" value name
''',
    'earlier_union_case': '''type Event = Sha of string // @case_declaration:Sha
type Sha = string // @alias:Sha
let value = Sha "a" // @application:Sha
let name = nameof Sha // @nameof_argument:Sha
printfn "%A:%s" value name
''',
    'some_alias': '''type Some = string // @alias:Some
let value = Some "a" // @application:Some
let name = nameof Some // @nameof_argument:Some
printfn "%A:%s" value name
''',
    'error_alias': '''type Error = string // @alias:Error
let value = Error "a" // @application:Error
let name = nameof Error // @nameof_argument:Error
printfn "%A:%s" value name
''',
    'generic_constructor': '''type Bag<'a> = System.Collections.Generic.List<'a> // @alias:Bag
let value = Bag<int>() // @constructor:Bag
let name = nameof Bag<int> // @nameof_argument:Bag
value.Add 42
printfn "%d:%s" value.Count name
''',
    'generic_value_shadow': '''type Bag<'a> = System.Collections.Generic.List<'a> // @alias:Bag
let Bag<'a> () = [| Unchecked.defaultof<'a> |] // @value_declaration:Bag
let value = Bag<int>() // @application:Bag
let name = nameof Bag<int> // @nameof_argument:Bag
printfn "%d:%s" value.Length name
''',
    'generic_tuple_alias': '''type Pair<'a> = System.Tuple<'a,'a> // @alias:Pair
let value = Pair<int>(1,2) // @constructor:Pair
let name = nameof Pair<int> // @nameof_argument:Pair
printfn "%d:%s" value.Item2 name
''',
    'generic_tuple_inferred': '''type Pair<'a> = System.Tuple<'a,'a> // @alias:Pair
let value = Pair(1,2) // @constructor:Pair
let name = nameof Pair<int> // @nameof_argument:Pair
printfn "%d:%s" value.Item2 name
''',
    'generic_tuple_physical_arity': '''type Pair<'a> = System.Tuple<'a,'a> // @alias:Pair
let value = Pair<int,int>(1,2) // @constructor:Pair
printfn "%d" value.Item2
''',
    'generic_reordered_alias': '''type Flip<'a,'b> = System.Collections.Generic.Dictionary<'b,'a> // @alias:Flip
let value = Flip<int,string>() // @constructor:Flip
let args = value.GetType().GenericTypeArguments
printfn "%s:%s" args[0].Name args[1].Name
''',
    'fixed_alias_explicit_arguments': '''type IntBag = System.Collections.Generic.List<int> // @alias:IntBag
let value = IntBag<string>() // @constructor:IntBag
printfn "%s" (value.GetType().GenericTypeArguments[0].Name)
''',
    'shadowed_nameof_with_value': '''type Sha = string // @alias:Sha
let Sha = 42 // @value_declaration:Sha
let nameof (x:int) = "custom:" + string x // @nameof_declaration:nameof
let name = nameof Sha // @nameof_argument:Sha @nameof_function:nameof
printfn "%s" name
''',
    'shadowed_nameof_with_type_only': '''type Sha = string // @alias:Sha
let nameof (x:string) = "custom:" + x // @nameof_declaration:nameof
let name = nameof Sha // @nameof_argument:Sha @nameof_function:nameof
printfn "%s" name
''',
    'local_parameter_shadow': '''type Sha = string // @alias:Sha
let consume Sha =
    let value = Sha "a" // @application:Sha
    let name = nameof Sha // @nameof_argument:Sha
    printfn "%s:%s" value name
consume (fun x -> "parameter:" + x)
''',
}

def main():
    fixture = Path(tempfile.mkdtemp(prefix='alias-expression-oracle-', dir=ROOT / '.tmp'))
    rows = []
    old_path = ROOT / 'docs/review/alias-expression-oracle.json'
    previous = {r['case']:r for r in json.loads(old_path.read_text())['cases']} if '--reuse-existing' in sys.argv and old_path.exists() else {}
    for name, source in CASES.items():
        path = fixture / (name + '.fsx')
        path.write_text(source)
        queries = []
        for line, text in enumerate(source.splitlines()):
            code = text.split('// @')[0]
            for label, token in re.findall(r'@(\w+):(\w+)', text):
                matches = list(re.finditer(r'\b' + re.escape(token) + r'\b', code))
                assert matches, (name, text, token)
                queries.append(dict(label=label, token=token, line=line, character=matches[-1].start()))
        if name in previous and previous[name]['source']==source:
            execution = previous[name]['execution']
            execution_provenance = {'reusedMatchingSourceExecutionFrom':previous[name]['path']}
        else:
            result = subprocess.run([str(DOTNET), 'fsi', '--exec', str(path)], env=ENV, text=True, capture_output=True, timeout=30)
            execution = dict(exitCode=result.returncode, stdout=result.stdout, stderr=result.stderr)
            execution_provenance = {'executedPath':str(path)}
        rows.append(dict(case=name, path=str(path), source=source, queries=queries, execution=execution, executionProvenance=execution_provenance))
        print(name, execution['exitCode'], execution['stdout'].strip(), execution['stderr'][-180:], flush=True)
    manifest = fixture / 'cases.json'
    manifest.write_text(json.dumps(rows))
    driver = ROOT / 'docs/review/alias-expression-oracle.fsx'
    result = subprocess.run([str(DOTNET), 'fsi', '--exec', str(driver), str(manifest)], env=ENV, text=True, capture_output=True, timeout=120)
    decoded = []
    for line in result.stdout.splitlines():
        if line.startswith('{'):
            decoded.append(json.loads(line))
    output = dict(provenance='Fresh actual FSI executions and raw FCS queries. Invalid source retained only as a boundary, never exact native coverage. No helper snapshot equivalence assumptions.', dotnetSha256=hashlib.sha256(DOTNET.read_bytes()).hexdigest(), fcsSha256=hashlib.sha256((DOTNET.parent/'sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll').read_bytes()).hexdigest(), fixture=str(fixture), cases=rows, fcsProcess=dict(exitCode=result.returncode,stderr=result.stderr), fcs=decoded)
    target = ROOT / 'docs/review/alias-expression-oracle.json'
    target.write_text(json.dumps(output,indent=2)+'\n')
    print('saved',target,'FCS',result.returncode,len(decoded),result.stderr[-1200:])
    assert result.returncode == 0 and len(decoded)==len(rows), result.stderr

if __name__=='__main__':
    main()
