#!/usr/bin/env python3
"""Actual FSI attribute-identity checks plus immutable native binding replay."""
import argparse, hashlib, importlib.util, json, subprocess, tempfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
DOTNET = ROOT / '.tools/dotnet/dotnet'
CASES = {
 'autoopen_core_alias': ('type AO = Microsoft.FSharp.Core.AutoOpenAttribute\nlet value = 1\n[<AO>]\nmodule Inner =\n    let value = 2\nlet result = value\nprintfn "%d" result\n', 2, 4, 'value'),
 'autoopen_custom_spelling': ('type AutoOpenAttribute() = inherit System.Attribute()\nlet value = 1\n[<AutoOpen>]\nmodule Inner =\n    let value = 2\nlet result = value\nprintfn "%d" result\n', 1, 1, 'value'),
 'rqa_core_alias': ('type RQA = Microsoft.FSharp.Core.RequireQualifiedAccessAttribute\ntype Visible = {Name:string}\n[<RQA>]\ntype Hidden = {Name:int}\nlet get x = x.Name\nprintfn "%s" (get {Visible.Name="visible"})\n', 'visible', 1, 'Name'),
 'rqa_custom_spelling': ('type RequireQualifiedAccessAttribute() = inherit System.Attribute()\ntype First = {Name:string}\n[<RequireQualifiedAccess>]\ntype Second = {Name:int}\nlet get x = x.Name\nprintfn "%d" (get {Second.Name=2})\n', 2, 3, 'Name'),
 'autoopen_suffix_beats_alias': ('type AO = Microsoft.FSharp.Core.AutoOpenAttribute\ntype AOAttribute() = inherit System.Attribute()\nlet value = 1\n[<AO>]\nmodule Inner =\n    let value = 2\nlet result = value\nprintfn "%d" result\n', 1, 2, 'value'),
 'rqa_suffix_beats_alias': ('type RQA = Microsoft.FSharp.Core.RequireQualifiedAccessAttribute\ntype RQAAttribute() = inherit System.Attribute()\ntype First = {Name:string}\n[<RQA>]\ntype Second = {Name:int}\nlet get x = x.Name\nprintfn "%d" (get {Second.Name=2})\n', 2, 4, 'Name'),
 'rqa_explicit_receiver_control': ('type RQA = Microsoft.FSharp.Core.RequireQualifiedAccessAttribute\ntype First = {Name:string}\n[<RQA>]\ntype Hidden = {Name:int}\nlet get (x:Hidden) = x.Name\nprintfn "%d" (get {Hidden.Name=2})\n', 2, 3, 'Name'),
}
def main():
    parser=argparse.ArgumentParser();parser.add_argument('binary', type=Path);parser.add_argument('--output', type=Path, required=True);args=parser.parse_args()
    loader=importlib.util.spec_from_file_location('native_replay', ROOT/'docs/review/native-smoke-replay.py');mod=importlib.util.module_from_spec(loader);loader.loader.exec_module(mod)
    base=Path(tempfile.mkdtemp(prefix='attribute-oracle-', dir=ROOT/'.tmp'));rows=[]
    for name,(source,expected_output,target,token) in CASES.items():
        path=base/(name+'.fsx');path.write_text(source)
        result=subprocess.run([str(DOTNET),'fsi','--exec',str(path)], capture_output=True,text=True,timeout=30)
        row={'id':name,'source':source,'fsi':{'exitCode':result.returncode,'stdout':result.stdout,'stderr':result.stderr},'expectedOutput':str(expected_output)}
        row['oracleMatched']=result.returncode==0 and result.stdout.strip()==str(expected_output)
        if row['oracleMatched']:
            native_source='module Main\n'+source
            root=mod.fixture(base,'native-'+name,{'Main.fs':native_source});daemon=mod.Daemon(args.binary.resolve(),root)
            queryline=next(i for i,line in enumerate(native_source.splitlines()) if line.startswith('let result =' if name.startswith('autoopen') else 'let get '))
            character=native_source.splitlines()[queryline].rindex(token)
            response=daemon.call('navigation/definition',{'path':str(root/'Main.fs'),'line':queryline,'character':character})
            expected=[mod.location(root/'Main.fs',target+1,native_source.splitlines()[target+1].index(token),len(token))]
            value=response.get('result',{});exact=value.get('completeness')=='exact';same=mod.normalized(value.get('locations',[]))==mod.normalized(expected)
            row['status']='pass' if exact and same else ('wrong-exact' if exact else 'unavailable-or-error')
            row['response']=response;row['expectedLocations']=expected
            daemon.close();row['transcript']=daemon.transcript
        else: row['status']='oracle-mismatch'
        rows.append(row)
        print(name,row['status'],result.stdout.strip(),flush=True)
    report={'provenance':'Fresh FSI executions and immutable native replay. Compiler validity is a required precondition; no native result is accepted for an invalid oracle input.','binarySha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'fixtureRoot':str(base),'cases':rows}
    args.output.write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':main()
