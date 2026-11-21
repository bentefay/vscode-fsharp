#!/usr/bin/env python3
"""Independent FCS/native controls for factory type-group constraint closure."""
import argparse, hashlib, importlib.util, json, os, subprocess, tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
a=argparse.ArgumentParser();a.add_argument('--binary',type=Path);a.add_argument('--output',type=Path,default=ROOT/'docs/review/factory-constraint-envelope-oracle.json');opts=a.parse_args()
base=Path(tempfile.mkdtemp(prefix='factory-envelope-',dir=ROOT/'.tmp'))
head='type First = { Name: string }\nmodule Helpers =\n    let inline force (x: ^T) : First =\n        ((^T) : (static member Empty : First) ())\n    let inline explicit< ^T when ^T : (static member Empty : First)> () =\n        ((^T) : (static member Empty : First) ())\n    let inline ( !@ ) (x: ^T) : First =\n        ((^T) : (static member Empty : First) ())\ntype Second = { Name: string }\n'
def factory(body='', before='',after=''):
 return head+'type Factory =\n    { Tag: int }\n'+before+'    static member Empty =\n'+''.join('        '+x+'\n' for x in body.splitlines())+'        { Name = "x" }\n'+after
cases=[
 ('plain_positive',factory()),
 ('unconstrained_defaultof',factory('let ignored = Helpers.force Unchecked.defaultof<_>')),
 ('constrained_generic_return',factory('let force seed =\n    let ignored = seed.Tag\n    Helpers.force seed\nlet ignored = force (List.head [])')),

 ('earlier_instance_self',factory(before='    member this.Force() = Helpers.force this\n')),
 ('later_instance_self',factory(after='    member this.Force() = Helpers.force this\n')),
 ('earlier_instance_self_alias',factory(before='    member this.Force() =\n        let item = this\n        Helpers.force item\n')),
 ('uninvoked_lambda',factory('let ignored = fun () -> Helpers.force { Tag = 0 }')),
 ('quoted_application',factory('let ignored = <@ Helpers.force { Tag = 0 } @>')),
 ('unreachable_branch',factory('let ignored = if false then Some(Helpers.force { Tag = 0 }) else None')),
 ('operator_application',factory('let ignored = Helpers.(!@) { Tag = 0 }')),
 ('partial_explicit_application',factory('let ignored = Helpers.explicit<Factory>')),
 ('earlier_member_partial_explicit',factory(before='    static member Force = Helpers.explicit<Factory>\n')),
 ('staticlet_in_record',factory(before='    static let cached = { Tag = 0 }\n    static let forced = Helpers.force cached\n')),
 ('nested_local_function',factory('let forceLocal x = Helpers.force x\nlet ignored = forceLocal { Tag = 0 }')),
 ('ordinary_unconstrained_call_positive',factory('let ignored = System.String.IsNullOrEmpty "x"')),
 ('later_outside_call',factory()+'let ignored = Helpers.force { Tag = 0 }\n'),
 ('lambda_value_escaped',factory('let ignored = [fun () -> Helpers.force { Tag = 0 }]')),
 ('field_projection_inferred_self',factory('let force seed =\n    let ignored = seed.Tag\n    Helpers.force seed\nlet ignored = force Unchecked.defaultof<_>')),
 ('earlier_field_projection_sibling',factory(before='    static member Force(seed) =\n        let ignored = seed.Tag\n        Helpers.force seed\n')),

]
manifest=[]
for name,source in cases:
 if name != 'operator_application':
  source=source.replace('    let inline ( !@ ) (x: ^T) : First =\n        ((^T) : (static member Empty : First) ())\n','')
 p=base/(name+'.fsx');p.write_text(source);lines=source.splitlines();line=next(i for i,s in enumerate(lines) if '{ Name = "x" }' in s)
 manifest.append(dict(case=name,source=source,path=str(p),queries=[dict(label='tail-field',token='Name',line=line,character=lines[line].index('Name'))]))
manifestpath=base/'input.json';manifestpath.write_text(json.dumps(manifest))
env=dict(os.environ,DOTNET_ROOT=str(ROOT/'.tools/dotnet'),DOTNET_NOLOGO='1',TMPDIR=str(ROOT/'.tmp'))
proc=subprocess.run([str(ROOT/'.tools/dotnet/dotnet'),'fsi','--exec',str(ROOT/'docs/review/alias-expression-oracle.fsx'),str(manifestpath)],env=env,text=True,capture_output=True,timeout=120)
rows=[json.loads(line) for line in proc.stdout.splitlines() if line.startswith('{')];assert len(rows)==len(manifest),(proc.stderr,proc.stdout)
if opts.binary:
 spec=importlib.util.spec_from_file_location('smoke',ROOT/'docs/review/native-smoke-replay.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
for item,oracle in zip(manifest,rows):
 assert item['case']==oracle['case'];item['oracle']=oracle;symbol=oracle['results'][0]['symbol'];item['tailOwner']=symbol['fullName'] if symbol else None
 if opts.binary:
  root=m.fixture(base,item['case'],{'Main.fs':'module Main\n'+item['source']});d=m.Daemon(opts.binary.resolve(),root);q=item['queries'][0];answer=d.call('navigation/definition',dict(path=str(root/'Main.fs'),line=q['line']+1,character=q['character']));d.close();item['native']=answer;item['nativeTranscript']=d.transcript
  if symbol and symbol['declaration']:
   target=symbol['declaration'];expected=m.location(root/'Main.fs',target['startLine']+1,target['startCharacter'],target['endCharacter']-target['startCharacter']);item['expectedLocation']=expected
   exact=answer.get('result',{}).get('completeness')=='exact';equal=m.normalized(answer.get('result',{}).get('locations',[]))==m.normalized([expected]);item['nativeStatus']='exact-correct' if exact and equal else 'wrong-exact' if exact else 'unavailable'
 print(item['case'],oracle['valid'],item['tailOwner'],item.get('nativeStatus',''))
report=dict(oracleScriptSha256=hashlib.sha256((ROOT/'docs/review/alias-expression-oracle.fsx').read_bytes()).hexdigest(),fcsSha256=hashlib.sha256((ROOT/'.tools/dotnet/sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll').read_bytes()).hexdigest(),provenance='Actual pinned FCS typechecking, never runtime evaluation of recursive static getters. Invalid cases remain inference-order boundary evidence, not valid-source acceptance.',fixtureRoot=str(base),cases=manifest,compiler=dict(exitCode=proc.returncode,stderr=proc.stderr))
if opts.binary:report['binary']=str(opts.binary.resolve());report['binarySha256']=hashlib.sha256(opts.binary.read_bytes()).hexdigest()
opts.output.write_text(json.dumps(report,indent=2)+'\n')
