#r "../../.tools/dotnet/sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll"
open System
open System.IO
open System.Text.Json
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.EditorServices
open FSharp.Compiler.Symbols
open FSharp.Compiler.Text
let input=JsonDocument.Parse(File.ReadAllText fsi.CommandLineArgs[1])
let model=JsonDocument.Parse(File.ReadAllText(input.RootElement.GetProperty("modelPath").GetString()))
let project=model.RootElement.GetProperty("projects").EnumerateArray() |> Seq.exactlyOne
let checker=FSharpChecker.Create(projectCacheSize=1,keepAssemblyContents=true,keepAllBackgroundResolutions=true,keepAllBackgroundSymbolUses=true)
let options=checker.GetProjectOptionsFromCommandLineArgs(project.GetProperty("path").GetString(),project.GetProperty("compilerArgs").EnumerateArray() |> Seq.map(fun a->a.GetString()) |> Seq.toArray)
let attempt fallback f=try f() with _->fallback
let loc(r:range)=box {|path=r.FileName;line=r.StartLine-1;character=r.StartColumn;endLine=r.EndLine-1;endCharacter=r.EndColumn|}
for query in input.RootElement.GetProperty("queries").EnumerateArray() do
 let path=query.GetProperty("path").GetString()
 let text=File.ReadAllText path
 let line=query.GetProperty("line").GetInt32()
 let col=query.GetProperty("character").GetInt32()+query.GetProperty("token").GetString().Length
 let lines=text.Split('\n')
 let _,answer=checker.ParseAndCheckFileInProject(path,0,SourceText.ofString text,options) |> Async.RunSynchronously
 match answer with
 | FSharpCheckFileAnswer.Aborted -> failwith "FCS aborted"
 | FSharpCheckFileAnswer.Succeeded checkedFile ->
  let errors=checkedFile.Diagnostics |> Array.filter(fun d->d.Severity=FSharpDiagnosticSeverity.Error) |> Array.map string
  let partial=QuickParse.GetPartialLongNameEx(lines[line],col-1)
  let names=partial.QualifyingIdents@[partial.PartialIdent]
  let symbol=checkedFile.GetSymbolUseAtLocation(line+1,col,lines[line],names)
  let info=symbol |> Option.map(fun u->box {|kind=u.Symbol.GetType().Name;fullName=attempt "" (fun()->u.Symbol.FullName);assembly=attempt "" (fun()->u.Symbol.Assembly.QualifiedName);assemblyPath=attempt "" (fun()->u.Symbol.Assembly.FileName |> Option.defaultValue "");declaration=attempt null(fun()->u.Symbol.DeclarationLocation |> Option.map loc |> Option.toObj)|}) |> Option.toObj
  printfn "%s" (JsonSerializer.Serialize {|name=query.GetProperty("name").GetString();valid=errors.Length=0;errors=errors;symbol=info|})
