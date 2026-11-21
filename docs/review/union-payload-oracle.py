#!/usr/bin/env python3
"""Real compiler controls for expected record types from immediate DU payloads."""
from pathlib import Path
import hashlib,json,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2];DOTNET=ROOT/'.tools/dotnet/dotnet'
PREFIX='type First = { Name: string }\ntype Second = { Name: string }\n'
CASES={
 'direct_case':PREFIX+'type Event = Wrap of First\nlet e = Wrap { Name="value" }\nlet (Wrap p) = e\nprintfn "%s" (p.GetType().Name)\n',
 'shadowed_value':PREFIX+'type Event = Wrap of First\nlet Wrap (x:Second) = x\nlet p = Wrap { Name="value" }\nprintfn "%s" (p.GetType().Name)\n',
 'qualified_companion_shadow':PREFIX+'type Event = Wrap of First\nmodule Event =\n    let Wrap (x:Second) = x\nlet p = Event.Wrap { Name="value" }\nprintfn "%s" (p.GetType().Name)\n',
 'tuple_payload':PREFIX+'type Event = Pair of left:First * right:Second\nlet e = Pair ({Name="left"}, {Name="right"})\nlet (Pair(a,b)) = e\nprintfn "%s:%s" (a.GetType().Name) (b.GetType().Name)\n',
 'generic_expected':PREFIX+"type Envelope<'a> = Wrap of 'a\nlet e:Envelope<First> = Wrap {Name=\"value\"}\nlet (Wrap p) = e\nprintfn \"%s\" (p.GetType().Name)\n",
 'generic_unconstrained':PREFIX+"type Envelope<'a> = Wrap of 'a\nlet e = Wrap {Name=\"value\"}\nlet (Wrap p) = e\nprintfn \"%s\" (p.GetType().Name)\n",
 'qualified_alias':PREFIX+'type Event = Wrap of First\ntype Alias = Event\nlet e = Alias.Wrap {Name="value"}\nlet (Wrap p) = e\nprintfn "%s" (p.GetType().Name)\n'
}
base=Path(tempfile.mkdtemp(prefix='union-payload-oracle-',dir=ROOT/'.tmp'));rows=[]
for name,source in CASES.items():
 path=base/(name+'.fsx');path.write_text(source);r=subprocess.run([str(DOTNET),'fsi','--exec',str(path)],text=True,capture_output=True,timeout=30);rows.append({'case':name,'source':source,'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr});print(name,r.returncode,r.stdout.strip(),r.stderr[-400:],flush=True)
version=subprocess.run([str(DOTNET),'fsi','--version'],text=True,capture_output=True,timeout=15)
(ROOT/'docs/review/union-payload-oracle.json').write_text(json.dumps({'provenance':'Actual pinned compiler executions; no native coverage or performance claim.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'compilerVersion':version.stdout,'fixture':str(base),'cases':rows},indent=2)+'\n')
