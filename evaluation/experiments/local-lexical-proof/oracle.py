#!/usr/bin/env python3
"""Read-only FCS definition/reference controls; never production navigation."""
from pathlib import Path
import hashlib,json,os,re,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[3]
OUT=Path(__file__).parent
CASES={
'qualified_constructor_root': 'module x=\n    type C()=class end\nlet f x=x.C()\n',
'qualified_module_root': 'module x=\n    let value=1\nlet f x=x.value\n',
'qualified_static_root': 'module x=\n    type C=static member M()=1\nlet f x=x.C.M()\n',
'local_function_type_name': 'type x()=class end\nlet f x=x()\n',
'global_value_and_open': 'module Values=\n    let x=99\nopen Values\nlet f x=x+x\n',
'global_literal': '[<Literal>]\nlet x=99\nlet f x=x+x\n',
'nested_lambda_shadow': 'let f x=(fun x -> x+1) x + x\n',
'nested_let_shadow': 'let f x=let y=x in let x=y+1 in x\n',
'function_vs_method_named_args': 'type C=\n    static member M(x:int)=x\nlet f x=C.M(x=x)\n',
'ordinary_boolean_argument': 'let g (b:bool)=b\nlet f x=g(x=x)\n',
'record_label_vs_value': 'type R={x:int}\nlet f x={x=x}\n',
'pattern_payload_shadow': 'type U=U of int\nlet f x=match x with U x -> x\n',
'nameof_value': 'let f x=nameof x\n',
'shadowed_nameof_value': 'let nameof (x:int)=string x\nlet f x=nameof x\n',
'type_position': 'type x=int\nlet f x=typeof<x>,x\n',
'interpolated_value': 'let f x = $"value={x}"\n',
'escaped_parameter': 'let f ``x``=``x``\n',
'computation_scope': 'let f x=seq { yield x; let x=1 in yield x }\n',
'property_named_like_parameter': 'type R={x:int}\nlet f x=x.x\n',
'lambda_context': 'type R={Name:string}\nlet f (rs:R array)=rs |> Array.map (fun x -> x.Name)\n',
'grace_sort_lambda': 'type Range={OrdinalStart:int;PhysicalOffset:int64;PhysicalLength:int64}\nlet selected (ranges:Range array)=ranges |> Array.sortBy (fun range -> range.OrdinalStart, range.PhysicalOffset, range.PhysicalLength)\n',
'unicode_utf16': 'let f x=("😀",x)\n',
'fable_filter_map_lambdas': "let selected (files:string array) (extension:string)=files |> Array.filter (fun x -> x.EndsWith(extension)) |> Array.map (fun x -> x.Replace('\\\\', '/'))\n",
'fable_normalizePath': "let normalizePath (path: string) = path.Replace('\\\\', '/')\n",
}
TOKENS={'grace_sort_lambda':'range','fable_normalizePath':'path'}

def queries(source,token):
 result=[]
 for line,text in enumerate(source.splitlines()):
  for match in re.finditer(r'(?<![\w\'])'+re.escape(token)+r'(?![\w\'])',text):
   result.append(dict(label=f'{token}@{line}:{match.start()}',line=line,character=len(text[:match.start()].encode('utf-16-le'))//2,token=token))
 return result

def main():
 fixture=Path(tempfile.mkdtemp(prefix='local-lexical-',dir=ROOT/'.cache/tmp'))
 rows=[]
 for case,source in CASES.items():
  path=fixture/(case+'.fsx');path.write_text(source)
  q=queries(source,TOKENS.get(case,'x')); assert q, case
  rows.append(dict(case=case,path=str(path),source=source,queries=q))
 signature='module Local\nval f : x:int -> int\n'
 implementation='module Local\nlet f x=x+x\n'
 use='module Consumer\nlet observed=Local.f 2\n'
 paths=[]
 for file,source in [('Local.fsi',signature),('Local.fs',implementation),('Use.fs',use)]:
  path=fixture/file;path.write_text(source);paths.append(str(path))
 for file,source in [('Local.fs',implementation),('Local.fsi',signature)]:
  rows.append(dict(case='paired_'+file,path=str(fixture/file),source=source,projectFiles=paths,queries=queries(source,'x')))
 paired = {
  'direct_lambda': ('val f : x:int -> int', 'let f = fun x -> x+x'),
  'returned_lambda': ('val f : unit -> x:int -> int', 'let f () = fun x -> x+x'),
  'passed_lambda': ('val f : xs:int list -> int list', 'let f xs=List.map (fun x -> x+x) xs'),
  'identity_lambda': ('val f : x:int -> int', 'let f = id (fun x -> x+x)'),
  'nested_function': ('val f : value:int -> int', 'let f value=let inner x=x+x in inner value'),
  'nested_function_return': ('val f : unit -> x:int -> int', 'let f ()=let inner x=x+x in inner'),
  'identity_lambda_unnamed_computed': ('val f : (int -> int)', 'let f = id (fun x -> x+x)'),
  'identity_lambda_computed': ('val f : (x:int -> int)', 'let f = id (fun x -> x+x)'),
  'nested_function_return_computed': ('val f : unit -> (x:int -> int)', 'let f ()=let inner x=x+x in inner'),
  'tail_lambda_behind_let': ('val f : unit -> x:int -> int', 'let f ()=let constant=1 in fun x -> x+constant'),
  'nested_function_return_unnamed': ('val f : unit -> (int -> int)', 'let f ()=let inner x=x+x in inner'),
  'tail_lambda_behind_let_unnamed': ('val f : unit -> (int -> int)', 'let f ()=let constant=1 in fun x -> x+constant'),
  'name_mismatch': ('val f : publicName:int -> int', 'let f x=x+x'),
 }
 for case,(sig,impl) in paired.items():
  directory=fixture/case;directory.mkdir()
  source='module Local\n'+impl+'\n'
  files=[directory/'Local.fsi',directory/'Local.fs']
  files[0].write_text('module Local\n'+sig+'\n');files[1].write_text(source)
  rows.append(dict(case='paired_'+case,path=str(files[1]),source=source,projectFiles=list(map(str,files)),queries=queries(source,'x')))
  if case=='name_mismatch':
   sigsource=files[0].read_text()
   rows.append(dict(case='paired_name_mismatch_signature',path=str(files[0]),source=sigsource,projectFiles=list(map(str,files)),queries=queries(sigsource,'publicName')))
 manifest=fixture/'manifest.json';manifest.write_text(json.dumps(rows))
 dotnet=ROOT/'.tools/dotnet/dotnet'
 env=dict(os.environ,DOTNET_ROOT=str(dotnet.parent),TMPDIR=str(ROOT/'.cache/tmp'),DOTNET_NOLOGO='1',DOTNET_CLI_TELEMETRY_OPTOUT='1')
 proc=subprocess.run([str(dotnet),'fsi','--exec',str(OUT/'oracle.fsx'),str(manifest)],env=env,text=True,capture_output=True,timeout=120)
 decoded=[json.loads(line) for line in proc.stdout.splitlines() if line.startswith('{')]
 report=dict(provenance='Actual pinned FCS source symbol and file/project references. Read-only fast-path research, no native implementation or performance claim.',fixtureRoot=str(fixture),dotnetSha256=hashlib.sha256(dotnet.read_bytes()).hexdigest(),cases=rows,results=decoded,process=dict(exitCode=proc.returncode,stderr=proc.stderr,stdout=proc.stdout if proc.returncode else None))
 (OUT/'oracle.json').write_text(json.dumps(report,indent=2)+'\n')
 print('process',proc.returncode,'decoded',len(decoded),'cases',len(rows))
 for row in decoded:print(row['case'],row['valid'],len(row['results']),row.get('diagnostics'))
 if proc.returncode:print(proc.stderr)
 assert proc.returncode==0 and len(rows)==len(decoded)
if __name__=='__main__':main()
