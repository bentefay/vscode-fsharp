#!/usr/bin/env python3
"""Compiler-backed literal-factory boundary replay on an immutable native."""
import argparse, hashlib, importlib.util, json, os, subprocess, tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
base=Path(tempfile.mkdtemp(prefix='factory-literal-boundary-',dir=ROOT/'.tmp'))
head='type First={Name:string}\ntype Second={Name:string}\n'
def f(body,header='static member Empty', extra='', owner='type Factory =\n    {Tag:int}\n'):
 return head+owner+extra+'    '+header+' = '+body+'\n'
cases=[
 ('plain_string',f('{Name="plain"}'),'Name','positive'),
 ('literal_dollar_braces',f('{Name="${Helpers.force<Factory>()}"}'),'Name','positive'),
 ('verbatim_dollar',f('{Name = @"${not a call}"}'),'Name','positive'),
 ('triple_dollar',f('{Name="""${not a call}"""}'),'Name','positive'),
 ('unit_method',f('{Name="plain"}','static member Empty()'),'Name','positive'),
 ('typed_parameter_unused',f('{Name="plain"}','static member Empty(value:int)'),'Name','positive'),
 ('explicit_result',f('{Name="plain"}','static member Empty : First'),'Name','positive'),
 ('qualified_label',f('{First.Name="plain"}'),'Name','positive'),
 ('interpolated',f('{Name = $"{1}"}'),'Name','outside-literal-gate'),
 ('verbatim_interpolated',f('{Name = $@"{1}"}'),'Name','outside-literal-gate'),
 ('triple_interpolated',f('{Name = $"""{1}"""}'),'Name','outside-literal-gate'),
 ('multidollar_interpolated',f('{Name = $$"""{{1}}"""}'),'Name','outside-literal-gate'),
 ('sibling_constant',f('{Name="plain"}',extra='    static member Other=1\n'),'Name','outside-literal-gate'),
 ('staticlet',f('{Name="plain"}',extra='    static let state=1\n'),'Name','outside-literal-gate'),
 ('constructor',f('{Name="plain"}',owner='type Factory() =\n'),'Name','outside-literal-gate'),
 ('generic_method',f('{Name="plain"}',"static member Empty<'T>()"),'Name','outside-literal-gate'),
 ('optional_parameter',f('{Name="plain"}','static member Empty(?value:int)'),'Name','outside-literal-gate'),
 ('member_attribute',f('{Name="plain"}',extra='    [<System.Obsolete>]\n'),'Name','outside-literal-gate'),
 ('tuple_container','type Result={Data:int*string}\ntype Factory=\n    {Tag:int}\n    static member Empty={Data=(1,"two")}\n','Data','positive'),
 ('list_container','type Result={Data:int list}\ntype Factory=\n    {Tag:int}\n    static member Empty={Data=[1;2]}\n','Data','positive'),
 ('array_container','type Result={Data:int array}\ntype Factory=\n    {Tag:int}\n    static member Empty={Data=[|1;2|]}\n','Data','positive'),
 ('nested_record','type Child={Value:string}\ntype Result={Data:Child}\ntype Factory=\n    {Tag:int}\n    static member Empty={Data={Value="one"}}\n','Data','positive'),
 ('range_container','type Result={Data:int list}\ntype Factory=\n    {Tag:int}\n    static member Empty={Data=[1..2]}\n','Data','outside-literal-gate'),
 ('numeric_suffix','type Result={Data:bigint}\ntype Factory=\n    {Tag:int}\n    static member Empty={Data=1I}\n','Data','outside-literal-gate'),
]
manifest=[]
for name,source,token,scope in cases:
 path=base/(name+'.fsx');path.write_text(source);lines=source.splitlines();line=len(lines)-1;column=lines[line].index(token,lines[line].index('{'))
 manifest.append(dict(case=name,source=source,path=str(path),declaredEnvelope=scope,queries=[dict(label='field',token=token,line=line,character=column)]))
mp=base/'oracle-input.json';mp.write_text(json.dumps(manifest));env=dict(os.environ,DOTNET_ROOT=str(ROOT/'.tools/dotnet'),TMPDIR=str(ROOT/'.tmp'),DOTNET_NOLOGO='1')
proc=subprocess.run([str(ROOT/'.tools/dotnet/dotnet'),'fsi','--exec',str(ROOT/'docs/review/alias-expression-oracle.fsx'),str(mp)],env=env,text=True,capture_output=True,timeout=120)
rows=[json.loads(x) for x in proc.stdout.splitlines() if x.startswith('{')];assert len(rows)==len(manifest),(proc.stderr,proc.stdout)
spec=importlib.util.spec_from_file_location('smoke',ROOT/'docs/review/native-smoke-replay.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
for item,oracle in zip(manifest,rows):
 assert item['case']==oracle['case'];item['oracle']=oracle
 root=m.fixture(base,item['case'],{'Main.fs':'module Main\n'+item['source']});d=m.Daemon(a.binary.resolve(),root);q=item['queries'][0]
 answer=d.call('navigation/definition',dict(path=str(root/'Main.fs'),line=q['line']+1,character=q['character']));d.close();item['native']=answer;item['transcript']=d.transcript
 symbol=oracle['results'][0]['symbol'];target=symbol['declaration'] if symbol else None;expected=[]
 if target:expected=[m.location(root/'Main.fs',target['startLine']+1,target['startCharacter'],target['endCharacter']-target['startCharacter'])]
 item['expectedLocations']=expected;exact=answer.get('result',{}).get('completeness')=='exact';same=m.normalized(answer.get('result',{}).get('locations',[]))==m.normalized(expected)
 item['status']='exact-correct' if exact and same else 'wrong-exact' if exact else 'unavailable'
 print(item['case'],oracle['valid'],item['status'])
report=dict(oracleScriptSha256=hashlib.sha256((ROOT/'docs/review/alias-expression-oracle.fsx').read_bytes()).hexdigest(),fcsSha256=hashlib.sha256((ROOT/'.tools/dotnet/sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll').read_bytes()).hexdigest(),binary=str(a.binary.resolve()),binarySha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),fixtureRoot=str(base),provenance='Independent actual FCS and native definitions. Outside-literal-gate refusal is safe but remains missing coverage; only actual exact-correct responses count as navigation.',cases=manifest,compilerStderr=proc.stderr)
a.output.write_text(json.dumps(report,indent=2)+'\n')
