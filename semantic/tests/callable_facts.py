#!/usr/bin/env python3
"""Binary schemes checked against real FCS use-site generic instantiation.

This is preparation/oracle evidence, not native inference or performance proof.
"""
import hashlib,json,pathlib,subprocess,tempfile
from smoke import ROOT,DOTNET,ENV,HELPER,project,run,request

def main():
 root=pathlib.Path(tempfile.mkdtemp(prefix='callable-facts-',dir=ROOT/'.cache'))
 source='''namespace Example
module Transform =
    /// Selects outputs using an ordinary generic callback.
    let selectMaybe (chooser:'T -> 'U option) (values:'T array) : 'U array = Array.choose chooser values
    let matrixIdentity (values:'T[,]) = values
    let constrained<'T when 'T:equality> (x:'T) = x
    let inline staticallyConstrained x = x + x
    let tupleInput (x,y) = x
'''
 (root/'Library.fs').write_text(source);p=root/'Library.fsproj';project(p,'<Compile Include="Library.fs"/>');run('build',p,'--nologo','-v:q','-m:1','-nodeReuse:false')
 dll=root/'bin/Debug/net10.0/Library.dll';core=DOTNET.parent/'sdk/10.0.400/FSharp/FSharp.Core.dll'
 catalog=root/'catalog.json';request('metadata/export',{'assemblyPaths':[str(dll),str(core)],'outputPath':str(catalog)})
 symbols=json.loads(catalog.read_text())['symbols']
 def row(name):return next(s for s in symbols if s['fullName']==name)
 custom=row('Example.Transform.selectMaybe');choose=row('Microsoft.FSharp.Collections.Array.choose');pipe=row('Microsoft.FSharp.Core.Operators.(|>)')
 for s in [custom,choose,pipe]:
  info=s['callableInfo'];assert info['supported'] and info['callableFactsVersion']==1,info
  assert info['identity']['fullName'] in s['qualifiedNames'] and info['xmlDocId']==s['xmlDocId']
 assert row('Example.Transform.constrained')['callableInfo']['supported'] is False
 assert row('Example.Transform.staticallyConstrained')['callableInfo']['supported'] is False
 assert row('Example.Transform.matrixIdentity')['callableInfo']['curriedArguments'][0][0]['rank']==2
 # The FCS API below is independent of the exporter. It obtains the selected
 # symbol use's actual generic arguments then instantiates its declaration type.
 probe=root/'Probe.fsx'
 probe.write_text('#r @"'+str(dll)+'"\n'+'''type First = { Name: string }
type Second = { Name: string }
let xs: First array = [| {First.Name="first"} |]
let custom = xs |> Example.Transform.selectMaybe (fun item -> Some item.Name)
let standard = xs |> Array.choose (fun item -> Some item.Name)
''')
 checker=root/'Check.fsx'
 checker.write_text('#r @"'+str(core.parent/'FSharp.Compiler.Service.dll')+'"\n'+r'''
open System
open System.IO
open System.Text.Json
open System.Text.Json.Nodes
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.Symbols
open FSharp.Compiler.Text
let node x = JsonSerializer.SerializeToNode x
let rec shape (ty:FSharpType):JsonNode =
    if ty.IsAbbreviation then shape ty.AbbreviatedType
    elif ty.IsGenericParameter then node {|kind="parameter";name=ty.GenericParameter.Name|}
    elif ty.IsArrayType then node {|kind="array";rank=ty.TypeDefinition.ArrayRank;element=shape ty.GenericArguments[0]|}
    elif ty.IsFunctionType then node {|kind="function";domain=shape ty.GenericArguments[0];range=shape ty.GenericArguments[1]|}
    elif ty.HasTypeDefinition then node {|kind="named";name=ty.TypeDefinition.FullName;arguments=ty.GenericArguments |> Seq.map shape |> Seq.toArray|}
    else node {|kind="unsupported"|}
let path=System.Environment.GetCommandLineArgs() |> Array.last
let checker=FSharpChecker.Create(keepAssemblyContents=true)
let source=SourceText.ofString(File.ReadAllText path)
let options,diagnostics=checker.GetProjectOptionsFromScript(path,source,assumeDotNetFramework=false) |> Async.RunSynchronously
let _,answer=checker.ParseAndCheckFileInProject(path,0,source,options) |> Async.RunSynchronously
let result=match answer with FSharpCheckFileAnswer.Succeeded value -> value | _ -> failwith "check failed"
let errors=Array.append (diagnostics |> Seq.toArray) result.Diagnostics |> Array.filter(fun d -> d.Severity=FSharpDiagnosticSeverity.Error)
if errors.Length>0 then failwithf "%A" errors
let uses=result.GetAllUsesOfAllSymbolsInFile() |> Seq.choose(fun usage ->
    match usage.Symbol with
    | :? FSharpMemberOrFunctionOrValue as value when not usage.IsFromDefinition && (value.DisplayName="choose" || value.DisplayName="selectMaybe" || value.CompiledName="op_PipeRight") ->
        Some {|fullName=value.FullName;xmlDocId=value.XmlDocSig;genericArguments=usage.GenericArguments |> List.map(fun (parameter,ty) -> {|name=parameter.Name;shape=shape ty|}) |> List.toArray;instantiated=shape(value.FullType.Instantiate usage.GenericArguments)|}
    | _ -> None) |> Seq.toArray
printfn "%s" (JsonSerializer.Serialize uses)
''')
 output=run('fsi','--exec',checker,probe);uses=json.loads(output.strip().splitlines()[-1]);assert len(uses)==4,uses
 def instantiate(t,args):
  kind=t['kind']
  if kind=='methodParameter':return args[t['index']]
  if kind=='named':return {'kind':'named','name':t['identity']['fullName'],'arguments':[instantiate(a,args)for a in t['arguments']]}
  if kind=='array':return {'kind':'array','rank':t['rank'],'element':instantiate(t['element'],args)}
  if kind=='function':return {'kind':'function','domain':instantiate(t['domain'],args),'range':instantiate(t['range'],args)}
  raise AssertionError(t)
 for symbol in [custom,choose]:
  usage=next(u for u in uses if u['xmlDocId']==symbol['xmlDocId']);info=symbol['callableInfo'];args=[a['shape'] for a in usage['genericArguments']]
  assert len(args)==info['genericArity'] and args[0]['name'].endswith('.First') and args[1]['name']=='System.String',usage
  body=instantiate(info['returnType'],args)
  for group in reversed(info['curriedArguments']):
   assert len(group)==1
   body={'kind':'function','domain':instantiate(group[0],args),'range':body}
  assert body==usage['instantiated'],(body,usage)
 # Recheck the inference-order distinction with the same custom declaration.
 direct=root/'Direct.fsx';direct.write_text(probe.read_text().split('let custom')[0]+'let result = Example.Transform.selectMaybe (fun item -> Some item.Name) xs\n')
 bad=subprocess.run([str(DOTNET),'fsi','--exec',str(direct)],env=ENV,text=True,capture_output=True,timeout=30)
 assert bad.returncode!=0 and 'First' in bad.stderr and 'Second' in bad.stderr,bad.stderr
 good=subprocess.run([str(DOTNET),'fsi','--exec',str(probe)],env=ENV,text=True,capture_output=True,timeout=30);assert good.returncode==0,good.stderr
 report={'scope':'one-shot binary schemes and independent actual FCS instantiation/order; no native callback correctness claim',
  'helperSha256':hashlib.sha256(HELPER.read_bytes()).hexdigest(),'fcsSha256':hashlib.sha256((core.parent/'FSharp.Compiler.Service.dll').read_bytes()).hexdigest(),
  'fixture':str(root),'catalog':str(catalog),'catalogSha256':hashlib.sha256(catalog.read_bytes()).hexdigest(),
  'selectedSymbols':[{k:s[k]for k in ['fullName','xmlDocId','callableInfo']}for s in [custom,choose,pipe]],'actualFcsUses':uses,
  'directCall':{'source':direct.read_text(),'exitCode':bad.returncode,'stderr':bad.stderr},'pipeline':{'source':probe.read_text(),'exitCode':good.returncode},
  'passed':6,'checks':['actual custom generic callback scheme','actual Core choose scheme','actual ordinary inline pipeline scheme','constraint/SRTP rejection','rank-two array preservation','actual FCS instantiation and pipeline/direct inference order']}
 (ROOT/'test-results/recovery/callable-facts.json').write_text(json.dumps(report,indent=2)+'\n')
 print('PASS 6 binary callable/array/function checks; actual FCS agrees with both generic instantiations and inference order')
if __name__=='__main__':main()
