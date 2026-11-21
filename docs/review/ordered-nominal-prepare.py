#!/usr/bin/env python3
"""Genuine reference universe and declaration-origin controls for ordered type lookup."""
import hashlib,importlib.util,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('prep',ROOT/'docs/review/prepared-replay.py');prep=importlib.util.module_from_spec(spec);spec.loader.exec_module(prep)
prep.HELPER=ROOT/'evaluation/.runtime/helper-09f01156157a/FSharpNavigator.Semantic.dll';os.environ.update(prep.ENV)
n=prep.load('native',ROOT/'docs/review/native-smoke-replay.py');base=Path(tempfile.mkdtemp(prefix='ordered-nominal-',dir=ROOT/'.tmp'))
libsource='''namespace Review.Empty
type Marker = class end
namespace Review.Shadow
type Scope = { Name: string }
namespace Review.AutoRoot
[<AutoOpen>]
module Implicit =
    type Scope = { Name: string }
namespace Review.DeepRoot
[<AutoOpen>]
module Outer =
    [<AutoOpen>]
    module Inner =
        type Scope = { Name: string }
'''
lib=n.fixture(base,'library',{'Library.fs':libsource});lp=lib/'Test.fsproj';lp.rename(lib/'ExternalScopes.fsproj');lp=lib/'ExternalScopes.fsproj';builds=[dict(phase='library',**prep.build(lp))];dll=lib/'bin/Debug/net10.0/ExternalScopes.dll'
def scope_case(namespace,opened):
 return f'''namespace ReviewCases.{namespace}
type Scope = {{ Name: string }}
open {opened}
type Holder = {{ Scope: Scope }}
module Use =
    let get (value: Holder) = value.Scope.Name
'''
files={
 'Absent.fs':scope_case('Absent','Review.Empty'),
 'Collision.fs':scope_case('Collision','Review.Shadow'),
 'AutoOpen.fs':scope_case('AutoOpen','Review.AutoRoot'),
 'DeepAutoOpen.fs':scope_case('DeepAutoOpen','Review.DeepRoot'),
 'SourceAutoOpen.fs':'''namespace ReviewCases.SourceAutoOpen
module Container =
    type Scope = { Name: string }
    [<AutoOpen>]
    module Inner =
        type Scope = { Name: string }
    open Review.Empty
    type Holder = { Scope: Scope }
    let get (value: Holder) = value.Scope.Name
''',
 'Origin.fs':'''namespace ReviewCases.Origin
type Scope = { Name: string }
type Alias = Scope
open Review.Empty
type Holder = { Scope: Alias }
type Box<'T> = { Value: 'T }
type GenericHolder = { Scope: Box<Scope> }
''',
 'Caller.fs':'''namespace ReviewCases.Caller
open ReviewCases.Origin
open Review.Shadow
module Use =
    let get (value: ReviewCases.Origin.Holder) = value.Scope.Name
    let generic (value: ReviewCases.Origin.GenericHolder) = value.Scope.Value.Name
''',
 'Signature.fsi':'''module ReviewCases.Signature
open ReviewCases.Origin
val make: unit -> Scope
''',
 'Signature.fs':'''module ReviewCases.Signature
open Review.Shadow
let make() : ReviewCases.Origin.Scope = { Name = "signature" }
''',
 'SignatureUse.fs':'''module ReviewCases.SignatureUse
open Review.Shadow
let get() = (ReviewCases.Signature.make()).Name
''',
}
root=n.fixture(base,'consumer',files);project=root/'Test.fsproj';project.write_text(project.read_text().replace('</ItemGroup>',f'<Reference Include="ExternalScopes"><HintPath>{dll}</HintPath></Reference></ItemGroup>'));builds.append(dict(phase='consumer',**prep.build(project)))
model=prep.helper('workspace/evaluate',dict(root=str(root)));mp=base/'model.json';mp.write_text(json.dumps(model));assert all(p['membershipReusable'] for p in model['projects']),model
refs=sorted({r['path'] for p in model['projects'] for r in p['referenceBindings'] if not r['isWorkspace']});owned=sorted({r for p in model['projects'] for r in p['workspaceAssemblyReferences']})
environment=prep.helper('metadata/environment',dict(referencePaths=refs,workspaceAssemblyPaths=owned));ep=base/'environment.json';ep.write_text(json.dumps(environment))
queries=[]
for filename in ['Absent.fs','Collision.fs','AutoOpen.fs','DeepAutoOpen.fs','SourceAutoOpen.fs','Caller.fs','SignatureUse.fs']:
 for line,text in enumerate(files[filename].splitlines()):
  if 'let get ' in text or 'let generic ' in text or 'let get()' in text:
   queries.append(dict(name=filename.replace('.fs','')+('-generic' if 'let generic ' in text else ''),path=str(root/filename),line=line,character=text.rindex('Name'),token='Name'))
lines=files['Absent.fs'].splitlines()
queries.extend([dict(name='Absent-intermediate',path=str(root/'Absent.fs'),line=5,character=lines[5].index('.Scope')+1,token='Scope'),dict(name='Absent-annotation',path=str(root/'Absent.fs'),line=3,character=lines[3].rindex('Scope'),token='Scope')])
manifest=dict(root=str(root),modelPath=str(mp),environmentPath=str(ep),libraryProject=str(lp),libraryPath=str(dll),librarySource=libsource,sources=files,queries=queries,builds=builds,helperSha256=hashlib.sha256(prep.HELPER.read_bytes()).hexdigest(),environmentOverrides={k:prep.ENV[k] for k in ['DOTNET_ROOT','DOTNET_NOLOGO','DOTNET_CLI_TELEMETRY_OPTOUT','MSBUILDDISABLENODEREUSE','DOTNET_CLI_USE_MSBUILD_SERVER','TMPDIR']})
manifestpath=base/'manifest.json';manifestpath.write_text(json.dumps(manifest,indent=2)+'\n')
p=subprocess.run([str(prep.DOTNET),'fsi','--exec',str(ROOT/'docs/review/nominal-project-oracle.fsx'),str(manifestpath)],env=prep.ENV,cwd=root,text=True,capture_output=True,timeout=120)
rows=[json.loads(x) for x in p.stdout.splitlines() if x.startswith('{')];assert len(rows)==len(queries),(p.stdout,p.stderr);assert all(r['valid'] and r['symbol'] for r in rows),rows
manifest['oracle']=rows;manifest['oracleProcess']=dict(exitCode=p.returncode,stderr=p.stderr);manifest['oracleScriptSha256']=hashlib.sha256((ROOT/'docs/review/nominal-project-oracle.fsx').read_bytes()).hexdigest();manifest['modelSha256']=hashlib.sha256(mp.read_bytes()).hexdigest();manifest['environmentSha256']=hashlib.sha256(ep.read_bytes()).hexdigest();manifestpath.write_text(json.dumps(manifest,indent=2)+'\n');(ROOT/'docs/review/ordered-nominal-prepared.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(manifest=str(manifestpath),queries=len(rows),bindings=[dict(name=r['name'],symbol=r['symbol']['fullName'],assembly=r['symbol']['assembly']) for r in rows]),indent=2))
