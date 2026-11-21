#!/usr/bin/env python3
"""Refresh reconstructed language rules with the real compiler after recovery."""
from pathlib import Path
import hashlib,json,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2];DOTNET=ROOT/'.tools/dotnet/dotnet'
spec=json.loads((ROOT/'docs/review/recovery-adversaries.json').read_text());byid={x['id']:x for x in spec['cases']};cases=[]
static=byid['static_record_factory_expected_constraints']
for name,body in static['variants'].items():cases.append((name,static['prefix']+body+static['suffix'],1 if name=='later_sibling' else 0))
r=byid['recursive_module_static_factory_constraint'];cases.append(('recursive_module',r['source'],0))
option=byid['option_pattern_identity']
for name,body in option['variants'].items():
 argument='Some {Supplier.Name="supplier"}' if name=='custom_option' else 'Microsoft.FSharp.Core.Some {Customer.Name="customer"}'
 cases.append(('option_'+name,option['prefix']+body+'printfn "%s" (read ('+argument+'))\n',0))
alias=byid['module_alias_declaration_site_and_additive_merge']
for name,source in alias['variants'].items():
 source=source.removeprefix('module Main\n');source+=('printfn "%d %d" chosen preserved\n' if name=='additive' else 'printfn "%d" Outer.Nested.chosen\n');cases.append(('alias_'+name,source,0))
cases.append(('namespace_alias_rejected','module Alias = System\n',1))
base=Path(tempfile.mkdtemp(prefix='constraint-oracles-',dir=ROOT/'.tmp'));rows=[]
for name,source,exitcode in cases:
 path=base/(name+'.fsx');path.write_text(source);r=subprocess.run([str(DOTNET),'fsi','--exec',str(path)],text=True,capture_output=True,timeout=30)
 row={'case':name,'source':source,'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'expectedExitCode':exitcode,'exitMatched':r.returncode==exitcode};rows.append(row);print(name,r.returncode,r.stdout.strip(),r.stderr[-250:],flush=True)
report={'provenance':'Fresh real FSI executions after recovery. This validates language semantics, not native correctness or performance.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'fixture':str(base),'cases':rows};(ROOT/'docs/review/constraint-oracles.json').write_text(json.dumps(report,indent=2)+'\n')
