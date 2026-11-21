#!/usr/bin/env python3
"""Compiler controls for excluding value-member selectors from alias references."""
from pathlib import Path
import hashlib,json,os,re,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2];DOTNET=ROOT/'.tools/dotnet/dotnet'
ENV=dict(os.environ,DOTNET_ROOT=str(DOTNET.parent),TMPDIR=str(ROOT/'.tmp'),DOTNET_NOLOGO='1')
PREFIX='module N =\n    type Alias = System.Text.StringBuilder // @alias:Alias\ntype Proxy() =\n    member _.Alias() = "value" // @member:Alias\n'
CASES={
'parameter_method':PREFIX+'let make (N:Proxy) = N.Alias() // @use:Alias\nprintfn "%s" (make (Proxy()))\n',
'parameter_missing_member':PREFIX+'let make (N:int) = N.Alias() // @use:Alias\nprintfn "%A" (make 1)\n',
'local_let_method':PREFIX+'let make () =\n    let N = Proxy()\n    N.Alias() // @use:Alias\nprintfn "%s" (make())\n',
'self_method':PREFIX+'type Other() =\n    member _.Alias() = "self" // @selfmember:Alias\n    member N.Make() = N.Alias() // @use:Alias\nprintfn "%s" (Other().Make())\n',
'parameter_nameof':PREFIX+'let name (N:Proxy) = nameof N.Alias // @use:Alias\nprintfn "%s" (name (Proxy()))\n',
'parameter_type_annotation':PREFIX+'let make (N:Proxy) : N.Alias = new N.Alias() // @constructor:Alias\nprintfn "%s" ((make (Proxy())).GetType().Name)\n',
'parameter_typeof':PREFIX+'let show (N:Proxy) = typeof<N.Alias>.Name // @typeuse:Alias\nprintfn "%s" (show (Proxy()))\n',
'parenthesized_local':PREFIX+'let make (N:Proxy) = (N).Alias() // @use:Alias\nprintfn "%s" (make (Proxy()))\n',
'computed_call':PREFIX+'let proxy () = Proxy()\nlet value = (proxy()).Alias() // @use:Alias\nprintfn "%s" value\n',
'parenthesized_module':PREFIX+'let value = (N).Alias() // @use:Alias\nprintfn "%A" value\n',
'parenthesized_alias_constructor':PREFIX+'let value = (N.Alias)() // @constructor:Alias\nprintfn "%s" (value.GetType().Name)\n',
'parenthesized_nameof_alias':PREFIX+'let name = nameof (N.Alias) // @typeuse:Alias\nprintfn "%s" name\n',
'companion_module_alias':'''type Alias = string // @alias:Alias
module Alias = // @module:Alias
    let value = 42
let x : Alias = Alias "a" // @constructor:Alias
let name = nameof Alias // @nameof_argument:Alias
let y = Alias.value // @module_use:Alias
printfn "%s:%s:%d" x name y
''',
'module_abbreviation_alias':'''module Other =
    let value = 42
type Alias = string // @alias:Alias
module Alias = Other // @module:Alias
let x : Alias = Alias "a" // @constructor:Alias
let name = nameof Alias // @nameof_argument:Alias
let y = Alias.value // @module_use:Alias
printfn "%s:%s:%d" x name y
''',
'named_union_field_alias':'''type Alias = string // @alias:Alias
type Holder = Case of Alias:int // @field_label:Alias
let text : Alias = "type" // @typeuse:Alias
let value = Case(Alias=42) // @construction_label:Alias
let (Case(Alias=result)) = value // @pattern_label:Alias
printfn "%s:%d" text result
''',
}
def main():
 base=Path(tempfile.mkdtemp(prefix='alias-receiver-oracle-',dir=ROOT/'.tmp'));rows=[]
 previous_path=ROOT/'docs/review/alias-receiver-oracle.json'
 previous={c['case']:c for c in json.loads(previous_path.read_text())['cases']} if previous_path.exists() else {}
 for name,source in CASES.items():
  path=base/(name+'.fsx');path.write_text(source);queries=[]
  for line,text in enumerate(source.splitlines()):
   code=text.split('// @')[0]
   for label,token in re.findall(r'@(\w+):(\w+)',text):
    found=list(re.finditer(r'\b'+re.escape(token)+r'\b',code));queries.append(dict(label=label,token=token,line=line,character=found[-1].start()))
  if name in previous and previous[name]['source']==source:
   execution=previous[name]['execution'];execution_provenance={'reusedMatchingSourceExecutionFrom':previous[name]['path']}
  else:
   p=subprocess.run([str(DOTNET),'fsi','--exec',str(path)],env=ENV,text=True,capture_output=True,timeout=30)
   execution=dict(exitCode=p.returncode,stdout=p.stdout,stderr=p.stderr);execution_provenance={'executedPath':str(path)}
  rows.append(dict(case=name,path=str(path),source=source,queries=queries,execution=execution,executionProvenance=execution_provenance));print(name,execution['exitCode'],execution['stdout'].strip(),execution['stderr'][-170:],flush=True)
 manifest=base/'cases.json';manifest.write_text(json.dumps(rows))
 p=subprocess.run([str(DOTNET),'fsi','--exec',str(ROOT/'docs/review/alias-expression-oracle.fsx'),str(manifest)],env=ENV,text=True,capture_output=True,timeout=120)
 decoded=[json.loads(s) for s in p.stdout.splitlines() if s.startswith('{')]
 output=dict(provenance='Actual pinned FSI execution plus raw FCS queries; invalid sources are boundary evidence only. No native coverage claims.',fixtureRoot=str(base),dotnetSha256=hashlib.sha256(DOTNET.read_bytes()).hexdigest(),cases=rows,fcs=decoded,fcsProcess=dict(exitCode=p.returncode,stderr=p.stderr))
 (ROOT/'docs/review/alias-receiver-oracle.json').write_text(json.dumps(output,indent=2)+'\n');assert p.returncode==0 and len(decoded)==len(rows),p.stderr
 print('saved',len(rows),'cases')
if __name__=='__main__':main()
