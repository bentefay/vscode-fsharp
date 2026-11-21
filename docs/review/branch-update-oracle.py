#!/usr/bin/env python3
"""Compiler inference-order controls for common match and record-update flow."""
from pathlib import Path
import hashlib,json,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2];DOTNET=ROOT/'.tools/dotnet/dotnet'
PREFIX='''type First =
    { Name: string; Count: int }
    static member Default: First = {Name="first";Count=0}
type Second =
    { Name: string; Count: int }
    static member Default: Second = {Name="second";Count=0}
'''
PRINT='printfn "%s" (result.GetType().Name)\n'
CASES={
 'known_first_then_update':PREFIX+'''let update flag current =
    let next =
        match flag with
        | true -> {First.Default with Name="created"}
        | false -> {current with Name="updated"}
    {next with Count=next.Count+1}
let result=update false First.Default
'''+PRINT,
 'update_first_then_known':PREFIX+'''let update flag current =
    match flag with
    | false -> {current with Name="updated"}
    | true -> {First.Default with Name="created"}
let result=update false First.Default
'''+PRINT,
 'update_first_explicit_result':PREFIX+'''let update flag current : First =
    match flag with
    | false -> {current with Name="updated"}
    | true -> First.Default
let result=update false First.Default
'''+PRINT,
 'alias_first_then_known':PREFIX+'''let update flag current =
    let next = match flag with true -> current | false -> First.Default
    {current with Name="updated";Count=next.Count}
let result=update true First.Default
'''+PRINT,
 'earlier_member_constraint':PREFIX+'''let update flag current =
    let oldName = current.Name
    match flag with
    | true -> First.Default
    | false -> {current with Name=oldName}
let result=update false First.Default
'''+PRINT,
 'known_first_nested_local_mutable':PREFIX+'''let update flag current =
    let next =
        match flag with
        | true ->
            let mutable fresh={First.Default with Name="created"}
            for index in [1;2] do fresh <- {fresh with Count=index}
            fresh
        | false -> {current with Name="updated"}
    {next with Count=next.Count+1}
let result=update false First.Default
'''+PRINT,
 'companion_default_shadow':PREFIX+'''module First =
    let Default: Second = {Name="shadow";Count=0}
let update flag current =
    match flag with
    | true -> First.Default
    | false -> {current with Name="updated"}
let result=update false Second.Default
'''+PRINT,
 'value_default_shadow':PREFIX+'''let First = {|Default=Second.Default|}
let update flag current =
    match flag with
    | true -> First.Default
    | false -> {current with Name="updated"}
let result=update false Second.Default
'''+PRINT,
 'static_default_other_return':'''type First =
    { Name: string; Count: int }
    static member Default: Second = {Name="other";Count=0}
and Second =
    { Name: string; Count: int }
let update flag current =
    match flag with
    | true -> First.Default
    | false -> {current with Name="updated"}
let result=update false First.Default
'''+PRINT,
 'recursive_argument_same':PREFIX+'''let rec update flag current =
    match flag with
    | 0 -> First.Default
    | 1 -> update 0 current
    | _ -> {current with Name="updated"}
let result=update 2 First.Default
'''+PRINT,
 'recursive_earlier_argument_constraint':PREFIX+'''let rec update flag current =
    let forced = if flag=1 then update 0 Second.Default else First.Default
    match flag with
    | 0 -> First.Default
    | _ -> {current with Name="updated"}
let result=update 2 Second.Default
'''+PRINT,
 'later_branch_cannot_retype_early_projection':PREFIX+'''let update flag current =
    match flag with
    | false -> let name=current.Name in {current with Name=name}
    | true -> First.Default
let result=update false First.Default
'''+PRINT,
}
base=Path(tempfile.mkdtemp(prefix='branch-update-oracle-',dir=ROOT/'.tmp'));rows=[]
for name,source in CASES.items():
 path=base/(name+'.fsx');path.write_text(source);r=subprocess.run([str(DOTNET),'fsi','--exec',str(path)],text=True,capture_output=True,timeout=30);rows.append({'case':name,'source':source,'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr});print(name,r.returncode,r.stdout.strip(),r.stderr[-450:],flush=True)
version=subprocess.run([str(DOTNET),'fsi','--version'],text=True,capture_output=True,timeout=15)
(ROOT/'docs/review/branch-update-oracle.json').write_text(json.dumps({'provenance':'Actual pinned compiler executions; inference order, not native coverage or performance evidence.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'compilerVersion':version.stdout,'fixture':str(base),'cases':rows},indent=2)+'\n')
