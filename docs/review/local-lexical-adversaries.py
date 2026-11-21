#!/usr/bin/env python3
from pathlib import Path
import hashlib,json,os,re,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'docs/review'
CASES={
'nameof_type_root': 'module x=\n    type Alias=string\nlet selected = List.map (fun x -> nameof x.Alias) [1]\n',
'nameof_module_value_root': 'module x=\n    let value=3\nlet selected = List.map (fun x -> nameof x.value) [1]\n',
'nameof_instance': 'type R={Name:string}\nlet selected = List.map (fun (x:R) -> nameof x.Name, x.Name) []\n',
'nameof_shadow': 'let nameof a=a\nlet selected = List.map (fun x -> nameof x + x) [1]\n',
'nameof_plain': 'let selected = List.map (fun x -> nameof x, x) [1]\n',
'interpolation_callback': 'let selected = $"{List.sum (List.map (fun x -> x+x) [1])}"\n',
'quote_callback': 'let selected = <@ List.map (fun x -> x+x) [1] @>\n',
'nested_nameof_type': 'module x=\n    type Alias=string\nlet selected = List.map (fun x -> nameof x.Alias, x) [1]\n',
'explicit_new_type_root': 'module x=\n    type Alias()=class end\nlet selected = List.map (fun x -> new x.Alias(),x) [1]\n',
'nested_let_function': 'let outer value=let inner x=x+x in inner value\n',
'nested_let_value': 'let selected = let inner x=x+x in inner 2\n',
'nested_inline_function': 'let outer value=let inline inner x=x+x in inner value\n',
'nested_parameter_shadow': 'let selected=List.map(fun x -> let inner x=x+x in inner x+x) [1]\n',
'nonrec_function_same_name': 'let selected=List.map(fun x -> let x y=x+y in x 1) [1]\n',
'namedarg_method': 'type C=static member M(x:int)=x\nlet selected=List.map(fun x -> C.M(x=x)) [1]\n',
'namedarg_boolean': 'let g b=b\nlet selected=List.map(fun x -> g(x=x)) [1]\n',
'lambda_in_named_arg': 'type C=static member M(f:int->int)=f 2\nlet selected=C.M(f=(fun x -> x+x))\n',
'escaped_use': 'let selected=List.map(fun x -> ``x``+x) [1]\n',
'operator_parameter': 'let selected=List.map(fun op_Addition -> op_Addition+op_Addition) [1]\n',
'upper_parameter': 'let selected=List.map(fun X -> X+X) [1]\n',
'lower_literal_param': '[<Literal>]\nlet x=3\nlet selected=List.map(fun x -> x+x) [1]\n',
'active_pattern_param': 'let (|x|) n=n\nlet selected=List.map(fun x -> x+x) [1]\n',
'custom_dynamic': 'let (?) a b=a\nlet selected=List.map(fun x -> x?x) [1]\n',
'custom_infix': 'let (%%) a b=a+b\nlet selected=List.map(fun x -> x %% x) [1]\n',
}
def queries(source,token):
 result=[]
 for line,text in enumerate(source.splitlines()):
  for match in re.finditer(r'(?<![\w\'])'+re.escape(token)+r'(?![\w\'])',text):
   result.append(dict(label=f'{token}@{line}:{match.start()}',line=line,character=len(text[:match.start()].encode('utf-16-le'))//2,token=token))
 return result

def main():
 fixture=Path(tempfile.mkdtemp(prefix='local-lexical-independent-',dir=ROOT/'.tmp'))
 cases=[]
 for name,source in CASES.items():
  path=fixture/(name+'.fsx');path.write_text(source)
  token={'operator_parameter':'op_Addition','upper_parameter':'X'}.get(name,'x')
  cases.append(dict(case=name,path=str(path),source=source,queries=queries(source,token)))
 manifest=fixture/'manifest.json';manifest.write_text(json.dumps(cases))
 oracle=ROOT/'evaluation/experiments/local-lexical-proof/oracle.fsx'
 dotnet=ROOT/'.tools/dotnet/dotnet'
 env=dict(os.environ,DOTNET_ROOT=str(dotnet.parent),TMPDIR=str(ROOT/'.tmp'),DOTNET_NOLOGO='1')
 p=subprocess.run([str(dotnet),'fsi','--exec',str(oracle),str(manifest)],env=env,text=True,capture_output=True,timeout=150)
 assert p.returncode==0,p.stderr
 decoded=[json.loads(l) for l in p.stdout.splitlines() if l.startswith('{')]
 assert len(decoded)==len(cases),(len(decoded),p.stderr)
 binary=OUT/'local-lexical-stage-snapshot-final/local-lexical-probe'
 results=[]
 for case,observed in zip(cases,decoded):
  checks=[]
  for q,oq in zip(case['queries'],observed['results']):
   for op in ['definition','references']:
    r=subprocess.run([str(binary)],input=json.dumps(dict(source=case['source'],uri=Path(case['path']).as_uri(),line=q['line'],character=q['character'],operation=op)),text=True,capture_output=True,timeout=3)
    response=json.loads(r.stdout) if r.returncode==0 else dict(error=r.stderr)
    answer=response.get('answer')
    expected=([oq['symbol']['declaration']] if oq['symbol'] else []) if op=='definition' else [x['range'] for x in oq['projectReferences']]
    normal=lambda a:(a['startLine'],a['startCharacter'],a['endLine'],a['endCharacter'])
    got=lambda a:(a['range']['start']['line'],a['range']['start']['character'],a['range']['end']['line'],a['range']['end']['character'])
    verdict='unavailable' if not answer else ('exact-correct' if set(map(got,answer['locations']))==set(map(normal,expected)) else 'mismatch')
    checks.append(dict(query=q,operation=op,oracle=oq,expected=expected,response=response,verdict=verdict))
  results.append(dict(case=case['case'],valid=observed['valid'],diagnostics=observed['diagnostics'],checks=checks))
 report=dict(provenance='Independent staged source-local proof replay against actual pinned FCS project symbol identities. Invalid source is retained separately. No production integration/performance claim.',fixtureRoot=str(fixture),binarySha256=hashlib.sha256(binary.read_bytes()).hexdigest(),sourceSha256=hashlib.sha256((OUT/'local-lexical-stage-snapshot-final/local_lexical.rs').read_bytes()).hexdigest(),cases=cases,results=results)
 (OUT/'local-lexical-adversaries.json').write_text(json.dumps(report,indent=2)+'\n')
 for row in results:
  counts={v:sum(q['verdict']==v for q in row['checks']) for v in ['exact-correct','unavailable','mismatch']}
  print(row['case'],'valid='+str(row['valid']),counts)
  if counts['mismatch']:
   for q in row['checks']:
    if q['verdict']=='mismatch': print(' MISMATCH',q['query']['label'],q['operation'],q['expected'],q['response']['answer']['locations'])
if __name__=='__main__':main()
