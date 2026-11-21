#r "../../../.tools/dotnet/sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll"
open System
open System.IO
open System.Text.Json
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics

let modelPath = Path.GetFullPath fsi.CommandLineArgs[1]
let model = JsonDocument.Parse(File.ReadAllText modelPath)
let project = model.RootElement.GetProperty("projects")[0]
let projectPath = project.GetProperty("path").GetString()
Directory.SetCurrentDirectory(Path.GetDirectoryName projectPath)
let checker = FSharpChecker.Create(projectCacheSize=1,keepAssemblyContents=true,keepAllBackgroundSymbolUses=true)
let args = project.GetProperty("compilerArgs").EnumerateArray() |> Seq.map(fun x -> x.GetString()) |> Seq.toArray
let options = checker.GetProjectOptionsFromCommandLineArgs(projectPath,args)
let results = checker.ParseAndCheckProject(options) |> Async.RunSynchronously
let uses = results.GetAllUsesOfAllSymbols()
let lines = Set.ofList [36;79;122]
let rows = [|for u in uses do
                if Path.GetFileName(u.FileName) = "UsageObservation.Types.fs" && lines.Contains u.Range.StartLine && u.Symbol.DisplayName = "RepositoryId" then
                    yield {|line=u.Range.StartLine;fullName=u.Symbol.FullName;kind=u.Symbol.GetType().Name;
                            declaration=(u.Symbol.DeclarationLocation |> Option.map(fun r -> {|path=r.FileName;line=r.StartLine-1;character=r.StartColumn;endLine=r.EndLine-1;endCharacter=r.EndColumn|}))|}|]
printfn "%s" (JsonSerializer.Serialize {|model=modelPath;project=projectPath;errors=(results.Diagnostics |> Array.filter(fun d -> d.Severity=FSharpDiagnosticSeverity.Error) |> Array.map string);rows=rows|})
