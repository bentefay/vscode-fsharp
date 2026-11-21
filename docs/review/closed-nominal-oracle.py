#!/usr/bin/env python3
import argparse,hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();base=Path(tempfile.mkdtemp(prefix='closed-nominal-',dir=ROOT/'.tmp'))
outer='type Scope={Name:string}\n'
inner='module Closed=\n    type Scope={Name:string}\n    type Alias=Scope\n'
access='let get (value:Holder)=value.Scope.Name\n'
cases=[
 ('mutual_forward_type',outer+'module M=\n    type Holder={Scope:Scope}\n    and Scope={Name:string}\n    let get (value:Holder)=value.Scope.Name\n'),
 ('mutual_forward_alias',outer+'type Alias=Scope\nmodule M=\n    type Holder={Scope:Alias}\n    and Alias=Scope\n    and Scope={Name:string}\n    let get (value:Holder)=value.Scope.Name\n'),
 ('later_separate_type',outer+'module M=\n    type Holder={Scope:Scope}\n    type Scope={Name:string}\n    let get (value:Holder)=value.Scope.Name\n'),
 ('public_open',outer+inner+'open Closed\ntype Holder={Scope:Alias}\n'+access),
 ('public_qualified',outer+inner+'type Holder={Scope:Closed.Alias}\n'+access),
 ('private_type_open',outer+'module Closed=\n    type private Scope={Name:string}\nopen Closed\ntype Holder={Scope:Scope}\n'+access),
 ('private_alias_open',outer+'type Alias=Scope\nmodule Closed=\n    type Inner={Name:string}\n    type private Alias=Inner\nopen Closed\ntype Holder={Scope:Alias}\n'+access),
 ('internal_same_project',outer+'module Closed=\n    type internal Scope={Name:string}\n    type internal Alias=Scope\nopen Closed\ntype internal Holder={Scope:Alias}\n'+access.replace('let get','let internal get')),
 ('declaration_origin_alias',outer+inner+'open Closed\ntype Holder={Scope:Alias}\nmodule Later=\n    type Scope={Name:string}\nopen Later\n'+access),
 ('later_closed_type',outer+'module Closed=\n    type Scope={Name:string}\n    type Holder={Scope:Scope}\n    let get (value:Holder)=value.Scope.Name\n    module Later=\n        type Scope={Name:string}\n'),
 ('module_abbreviation_not_exported','module Original=\n    type Scope={Name:string}\nmodule Closed=\n    module Alias=Original\ntype Holder={Scope:Closed.Alias.Scope}\n'+access),
]
manifest=[]
for name,source in cases:
 path=base/(name+'.fsx');path.write_text(source);lines=source.splitlines();line=next(i for i,s in enumerate(lines) if 'get (value:' in s);manifest.append(dict(case=name,source=source,path=str(path),queries=[dict(label='field',line=line,character=lines[line].rindex('Name'),token='Name')]))
mp=base/'input.json';mp.write_text(json.dumps(manifest));env=dict(os.environ,DOTNET_ROOT=str(ROOT/'.tools/dotnet'),TMPDIR=str(ROOT/'.tmp'));proc=subprocess.run([str(ROOT/'.tools/dotnet/dotnet'),'fsi','--exec',str(ROOT/'docs/review/alias-expression-oracle.fsx'),str(mp)],env=env,text=True,capture_output=True,timeout=120);rows=[json.loads(x) for x in proc.stdout.splitlines() if x.startswith('{')];assert len(rows)==len(manifest),(proc.stdout,proc.stderr)
spec=importlib.util.spec_from_file_location('native',ROOT/'docs/review/native-smoke-replay.py');n=importlib.util.module_from_spec(spec);spec.loader.exec_module(n)
for c,o in zip(manifest,rows):
 c['oracle']=o;root=n.fixture(base,c['case'],{'Main.fs':'module Main\n'+c['source']});d=n.Daemon(a.binary.resolve(),root);q=c['queries'][0];answer=d.call('navigation/definition',dict(path=str(root/'Main.fs'),line=q['line']+1,character=q['character']));d.close();c['response']=answer;c['transcript']=d.transcript;symbol=o['results'][0]['symbol'];expected=[]
 if symbol and symbol['declaration']:
  z=symbol['declaration'];expected=[n.location(root/'Main.fs',z['startLine']+1,z['startCharacter'],z['endCharacter']-z['startCharacter'])]
 exact=answer.get('result',{}).get('completeness')=='exact';same=n.normalized(answer.get('result',{}).get('locations',[]))==n.normalized(expected);c['status']='exact-correct' if exact and same else 'wrong-exact' if exact else 'unavailable';c['expectedLocations']=expected;print(c['case'],o['valid'],symbol['fullName'] if symbol else None,c['status'])
a.output.write_text(json.dumps(dict(binary=str(a.binary.resolve()),binarySha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),cases=manifest,fixtureRoot=str(base),provenance='Independent FCS and native source-only controls. Invalid module abbreviation export stays a boundary, not valid-source coverage.'),indent=2)+'\n')
