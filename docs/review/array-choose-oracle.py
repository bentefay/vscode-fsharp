#!/usr/bin/env python3
"""Small real-compiler proof probes; not performance measurements."""
from pathlib import Path
import hashlib, json, subprocess, tempfile
ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT/'.tools/dotnet/dotnet'
RECORDS='type First = { Name: string }\ntype Second = { Name: string }\nlet xs: First array = [| { First.Name = "first" } |]\n'
CLASSES='type Base() =\n    member _.Name = "base"\ntype Derived() =\n    inherit Base()\n    member _.Name = "derived"\nlet xs: Derived array = [|Derived()|]\n'
CASES={
 'records_pipeline':RECORDS+'let result = xs |> Array.choose (fun x -> Some x.Name)\nprintfn "%A" result\n',
 'records_direct':RECORDS+'let result = Array.choose (fun x -> Some x.Name) xs\nprintfn "%A" result\n',
 'records_direct_annotated':RECORDS+'let result = Array.choose (fun (x:First) -> Some x.Name) xs\nprintfn "%A" result\n',
 'classes_pipeline':CLASSES+'let result = xs |> Array.choose (fun x -> Some x.Name)\nprintfn "%A" result\n',
 'classes_direct':CLASSES+'let result = Array.choose (fun x -> Some x.Name) xs\nprintfn "%A" result\n',
 'classes_direct_base_annotation':CLASSES+'let result = Array.choose (fun (x:Base) -> Some x.Name) xs\nprintfn "%A" result\n',
 'classes_pipeline_base_annotation':CLASSES+'let result = xs |> Array.choose (fun (x:Base) -> Some x.Name)\nprintfn "%A" result\n',
 'records_direct_obj_annotation':RECORDS+'let result = Array.choose (fun (x:obj) -> Some (x.GetType().Name)) xs\nprintfn "%A" result\n',
 'shadowed_array':RECORDS+'module Array =\n    let choose f (_:First array) = [|f {Second.Name="second"}|]\nlet result = xs |> Array.choose (fun x -> Some x.Name)\nprintfn "%A" result\n',
 'shadowed_pipeline':RECORDS+'let (|>) (_:First array) f = f [|{Second.Name="second"}|]\nlet result = xs |> Array.choose (fun x -> Some x.Name)\nprintfn "%A" result\n'
}
base=Path(tempfile.mkdtemp(prefix='array-choose-oracle-',dir=ROOT/'.tmp'))
rows=[]
for name,source in CASES.items():
 path=base/(name+'.fsx');path.write_text(source)
 p=subprocess.run([str(DOTNET),'fsi','--exec',str(path)],text=True,capture_output=True,timeout=30)
 row={'case':name,'source':source,'exitCode':p.returncode,'stdout':p.stdout,'stderr':p.stderr};rows.append(row)
 print(name,p.returncode,p.stdout.strip(),p.stderr[-450:],flush=True)
version=subprocess.run([str(DOTNET),'fsi','--version'],text=True,capture_output=True,timeout=15)
report={'provenance':'Fresh execution after workspace recovery; compiler probes only, no native/performance claims.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'compilerVersion':version.stdout,'fixture':str(base),'cases':rows}
(ROOT/'docs/review/array-choose-oracle.json').write_text(json.dumps(report,indent=2)+'\n')
