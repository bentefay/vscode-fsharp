#!/usr/bin/env python3
"""Source-only diagnostic replay of independently valid alias-expression cases."""
from pathlib import Path
import argparse, hashlib, importlib.util, json, os, subprocess, tempfile

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('native_smoke',ROOT/'docs/review/native-smoke-replay.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

def key(line,column,endline,endcolumn):return (line,column,endline,endcolumn)
def main():
    ap=argparse.ArgumentParser();ap.add_argument('binary',type=Path);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    evidence=json.loads((ROOT/'docs/review/alias-expression-oracle.json').read_text())
    byname={x['case']:x for x in evidence['fcs']}
    root=Path(tempfile.mkdtemp(prefix='alias-expression-native-',dir=ROOT/'.tmp'))
    cases=[x for x in evidence['cases'] if byname[x['case']]['valid']]
    for c in cases:(root/(c['case']+'.fs')).write_text('module '+c['case']+'\n'+c['source'])
    compile_items=''.join('<Compile Include="'+c['case']+'.fs"/>' for c in cases)
    (root/'Cases.fsproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DisableImplicitFSharpCoreReference>true</DisableImplicitFSharpCoreReference><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="FSharp.Core"><HintPath>$(MSBuildToolsPath)/FSharp/FSharp.Core.dll</HintPath></Reference>'+compile_items+'</ItemGroup></Project>')
    env=dict(os.environ,DOTNET_ROOT=str(ROOT/'.tools/dotnet'),TMPDIR=str(ROOT/'.tmp'),MSBUILDDISABLENODEREUSE='1',DOTNET_CLI_USE_MSBUILD_SERVER='0')
    build=subprocess.run([str(ROOT/'.tools/dotnet/dotnet'),'build',str(root/'Cases.fsproj'),'--nologo','-v:q','-m:1'],env=env,text=True,capture_output=True,timeout=90)
    assert build.returncode==0,build.stderr+build.stdout
    daemon=module.Daemon(args.binary.resolve(),root);rows=[]
    for case in cases:
        queries={q['label']:q for q in case['queries']}
        for oracle in byname[case['case']]['results']:
            if oracle['label'] in ['nameof_function','nameof_declaration']:continue
            q=queries[oracle['label']];path=root/(case['case']+'.fs')
            for operation in ['definition','references']:
                response=daemon.call('navigation/'+operation,{'path':str(path),'line':q['line']+1,'character':q['character'],'includeDeclaration':True})
                result=response.get('result',{});exact=result.get('completeness')=='exact';actual=[]
                for loc in result.get('locations',[]):
                    r=loc['range'];actual.append((loc.get('uri',loc.get('path','')),key(r['start']['line'],r['start']['character'],r['end']['line'],r['end']['character'])))
                expected=[];external=False
                if operation=='references':
                    expected=sorted(set(key(x['range']['startLine']+1,x['range']['startCharacter'],x['range']['endLine']+1,x['range']['endCharacter']) for x in oracle['references']))
                else:
                    decl=(oracle['symbol'] or {}).get('declaration')
                    if decl and decl['path']==case['path']:expected=[key(decl['startLine']+1,decl['startCharacter'],decl['endLine']+1,decl['endCharacter'])]
                    else:external=True
                source_actual=all(uri in [str(path),path.as_uri()] for uri,_ in actual)
                equal=source_actual and sorted(set(r for _,r in actual))==expected
                if not exact:status='unavailable-or-error'
                elif external:status='wrong-exact' if any(uri in [str(path),path.as_uri()] for uri,_ in actual) else 'external-target-needs-inspection'
                else:status='pass' if equal else 'wrong-exact'
                rows.append(dict(case=case['case'],label=q['label'],operation=operation,status=status,expectedSourceRanges=expected,expectedExternal=external,response=response,oracle=oracle))
    daemon.close()
    output=dict(binarySha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),oracleReport='docs/review/alias-expression-oracle.json',fixtureRoot=str(root),provenance='Source-only indexed mode, no prepared environment. Fresh real SDK build of explicit-module equivalents of the valid FSI sources. Unsupported results are coverage misses; external exact targets require separate inspection.',build=dict(exitCode=build.returncode,stdout=build.stdout,stderr=build.stderr),cases=rows,transcript=daemon.transcript,summary={s:sum(r['status']==s for r in rows) for s in ['pass','wrong-exact','unavailable-or-error','external-target-needs-inspection']})
    args.output.write_text(json.dumps(output,indent=2)+'\n');print(output['summary'])
    for r in rows:
        if r['status']=='wrong-exact':print(r['case'],r['label'],r['operation'],r['response'])

if __name__=='__main__':main()
