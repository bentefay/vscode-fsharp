#!/usr/bin/env python3
"""Real F# binary/module/term inventory regression, including implicit opens."""
import argparse, hashlib, json, shutil, tempfile
from pathlib import Path
import smoke


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--helper',type=Path,default=smoke.HELPER);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    root=Path(tempfile.mkdtemp(prefix='term-inventory-',dir=smoke.ROOT/'.tmp'))
    runtime=root/'helper';shutil.copytree(args.helper.resolve().parent,runtime);smoke.HELPER=runtime/args.helper.name
    library=root/'Library';library.mkdir()
    source='''namespace Inventory
[<AutoOpen>]
module Imported =
    let System = 7
    [<Literal>]
    let Answer = 42
    let (|Even|Odd|) n = if n % 2 = 0 then Even else Odd
    let (.+.) a b = a + b
[<RequireQualifiedAccess; CompilationRepresentation(CompilationRepresentationFlags.ModuleSuffix)>]
module Mapped =
    module Nested =
        let choose x = x
    type Choice = Alpha of int | Beta
    [<RequireQualifiedAccess>]
    type Strict = Only of int
[<assembly: AutoOpen("Inventory.Imported")>]
do ()
'''
    (library/'Library.fs').write_text(source);project=library/'Library.fsproj';smoke.project(project,'<Compile Include="Library.fs"/>')
    build=smoke.run('build',project,'--nologo','-v:q','-nodeReuse:false','-m:1','-p:BuildInParallel=false')
    dll=library/'bin/Debug/net10.0/Library.dll'
    consumer=root/'Consumer';consumer.mkdir();consumer_source='module Consumer\n[<EntryPoint>]\nlet main _ = printfn "%d" System; 0\n';(consumer/'Main.fs').write_text(consumer_source)
    consumer_project=consumer/'Consumer.fsproj';smoke.project(consumer_project,'<Compile Include="Main.fs"/>',f'<Reference Include="Library"><HintPath>{dll}</HintPath></Reference>');consumer_project.write_text(consumer_project.read_text().replace('</PropertyGroup>','<OutputType>Exe</OutputType></PropertyGroup>'))
    consumer_build=smoke.run('build',consumer_project,'--nologo','-v:q','-nodeReuse:false','-m:1','-p:BuildInParallel=false')
    oracle=smoke.run(consumer/'bin/Debug/net10.0/Consumer.dll').strip();assert oracle=='7',oracle
    model=smoke.request('workspace/evaluate',{'root':str(consumer),'projects':[str(consumer_project)]})
    refs=sorted({b['path'] for p in model['projects'] for b in p['referenceBindings'] if not b['isWorkspace']})
    env=smoke.request('metadata/environment',{'referencePaths':refs});(root/'environment.json').write_text(json.dumps(env,indent=2))
    def rows(domain,name):return [r for r in env[domain].get(name,[]) if r['identity']['assemblyName']=='Library']
    checks={}
    checks['namespace']=any(r['kind']=='namespace' and r['sourceFullName']=='Inventory' for r in rows('scopeNames','Inventory'))
    checks['autoOpenModule']=any(r['autoOpen'] and r['sourceFullName']=='Inventory.Imported' for r in rows('scopeNames','Imported'))
    checks['assemblyAutoOpen']=any(r['assemblyIdentity']['assemblyName']=='Library' and r['modulePath']=='Inventory.Imported' for r in env['assemblyAutoOpens'])
    checks['renamedModule']=any(r['sourceFullName']=='Inventory.Mapped' and r['identity']['fullName']=='Inventory.MappedModule' and r['qualifiedOnly'] for r in rows('scopeNames','Mapped'))
    checks['nestedLogicalModule']=any(r['sourceFullName']=='Inventory.Mapped.Nested' for r in rows('scopeNames','Nested'))
    checks['value']=any(r['kind']=='value' and r['declaringScope']=='Inventory.Imported' for r in rows('termNames','System'))
    checks['literal']=any(r['kind']=='literal' for r in rows('termNames','Answer'))
    checks['activePatterns']=all(any(r['kind']=='activePattern' for r in rows('termNames',n)) for n in ['Even','Odd'])
    checks['unionCase']=any(r['kind']=='unionCase' and r['declaringScope']=='Inventory.Mapped.Choice' and r['enclosingScope']=='Inventory.Mapped' for r in rows('termNames','Alpha'))
    checks['qualifiedUnion']=any(r['kind']=='unionCase' and r['qualifiedOnly'] for r in rows('termNames','Only'))
    checks['operatorIdentity']=any(r['kind']=='value' and r['compiledName'].startswith('op_') for r in rows('termNames','.+.'))
    checks['frameworkOperator']=any(r['compiledName']=='op_PipeRight' for r in env['termNames'].get('|>',[]))
    checks['allDomains']=env['scopeNameInventory']=='all-external-namespace-and-module-declarations' and env['termNameInventory']=='all-external-expression-bindings'
    report={'fixtureRoot':str(root),'helperSha256':hashlib.sha256(smoke.HELPER.read_bytes()).hexdigest(),'librarySource':source,'consumerSource':consumer_source,'build':build,'consumerBuild':consumer_build,'runtimeOracle':oracle,'checks':checks,'counts':{d:{'names':len(env[d]),'entries':sum(map(len,env[d].values()))} for d in ['typeNames','scopeNames','termNames']},'environment':str(root/'environment.json')}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'output':str(args.output),'checks':checks,'fixtureRoot':str(root)}),flush=True)
    assert all(checks.values()),checks

if __name__=='__main__':main()
