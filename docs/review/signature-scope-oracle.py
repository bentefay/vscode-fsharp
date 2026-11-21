#!/usr/bin/env python3
"""Test whether .fsi return signatures contextually type implementation records."""
from pathlib import Path
import hashlib,json,os,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2];DOTNET=ROOT/'.tools/dotnet/dotnet'
env=dict(os.environ,DOTNET_ROOT=str(DOTNET.parent),MSBUILDDISABLENODEREUSE='1',DOTNET_CLI_USE_MSBUILD_SERVER='0',DOTNET_CLI_TELEMETRY_OPTOUT='1',TMPDIR=str(ROOT/'.tmp'))
base=Path(tempfile.mkdtemp(prefix='signature-scope-',dir=ROOT/'.tmp'));rows=[]
types='namespace Lib\nmodule A =\n    type Person = { Name: string }\nmodule B =\n    type Person = { Name: string }\n'
signature='module Api\nopen Lib.A\nval make : unit -> Person\n'
variants={'unqualified':'module Api\nopen Lib.B\nlet make () = { Name = "value" }\n','qualified':'module Api\nopen Lib.B\nlet make () = { Lib.A.Person.Name = "value" }\n','annotated':'module Api\nopen Lib.B\nlet make () : Lib.A.Person = { Name = "value" }\n','opaque_public_result':'module Api\nopen Lib.B\nlet make () = Unchecked.defaultof<Lib.A.Person>\n'}
for name,implementation in variants.items():
 folder=base/name;folder.mkdir();files={'Types.fs':types,'Api.fsi':signature,'Api.fs':implementation,'Caller.fs':'module Caller\nopen Lib.B\nlet name () = (Api.make()).Name\n'}
 for filename,text in files.items():(folder/filename).write_text(text)
 project=folder/'Test.fsproj';project.write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup>'+''.join('<Compile Include="'+f+'"/>' for f in files)+'</ItemGroup></Project>')
 p=subprocess.run([str(DOTNET),'build',str(project),'--nologo','-v:q','-m:1','-nodeReuse:false'],env=env,text=True,capture_output=True,timeout=90)
 row={'id':name,'sourceFiles':files,'root':str(folder),'exitCode':p.returncode,'stdout':p.stdout,'stderr':p.stderr};rows.append(row);print(name,p.returncode,p.stdout[-1700:],flush=True)
report={'provenance':'Fresh actual SDK compile with ordered Types.fs, Api.fsi, Api.fs. Successful and failed variants are retained; an invalid program is not a positive navigation oracle.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'fixtureRoot':str(base),'cases':rows}
(ROOT/'docs/review/signature-scope-oracle.json').write_text(json.dumps(report,indent=2)+'\n')
