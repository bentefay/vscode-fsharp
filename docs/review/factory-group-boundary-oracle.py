#!/usr/bin/env python3
"""Completed type-group return inference versus later outside use constraints."""
from pathlib import Path
import hashlib,json,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2];DOTNET=ROOT/'.tools/dotnet/dotnet'
RECORDS='type First = { Name: string }\ntype Second = { Name: string }\n'
FACTORY=RECORDS+'type Factory =\n    static member Create() = { Name="value" }\n'
CASES={
 'later_outside_value':FACTORY+'let result:First=Factory.Create()\nprintfn "%s" (result.GetType().Name)\n',
 'later_separate_type':FACTORY+'type Consumer =\n    static member Force():First=Factory.Create()\nlet result=Consumer.Force()\nprintfn "%s" (result.GetType().Name)\n',
 'later_outside_letrec':FACTORY+'let rec force flag : First =\n    if flag then force false else Factory.Create()\nlet result=force false\nprintfn "%s" (result.GetType().Name)\n',
 'later_outside_unconstrained_control':FACTORY+'let result=Factory.Create()\nprintfn "%s" (result.GetType().Name)\n',
 'inside_nested_recursive_call_constraint':RECORDS+'type Factory =\n    static member Create() =\n        let rec force flag : First =\n            if flag then force false else Factory.Create()\n        {Name="value"}\nlet result=Factory.Create()\nprintfn "%s" (result.GetType().Name)\n'
}
base=Path(tempfile.mkdtemp(prefix='factory-group-boundary-',dir=ROOT/'.tmp'));rows=[]
for name,source in CASES.items():
 path=base/(name+'.fsx');path.write_text(source);r=subprocess.run([str(DOTNET),'fsi','--exec',str(path)],text=True,capture_output=True,timeout=30);rows.append({'case':name,'source':source,'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr});print(name,r.returncode,r.stdout.strip(),r.stderr[-450:],flush=True)
version=subprocess.run([str(DOTNET),'fsi','--version'],text=True,capture_output=True,timeout=15)
(ROOT/'docs/review/factory-group-boundary-oracle.json').write_text(json.dumps({'provenance':'Actual pinned compiler supplemental tests. Earlier/later sibling, mutual type and recursive-module controls remain in constraint-oracles.json; paired signature non-injection in signature-scope-oracle.json.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'compilerVersion':version.stdout,'fixture':str(base),'cases':rows},indent=2)+'\n')
