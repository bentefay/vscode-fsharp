#r "FSharp.Compiler.Service.dll"
open System
open System.IO
open System.Text.Json
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.EditorServices
open FSharp.Compiler.Symbols
open FSharp.Compiler.Text

let checker = FSharpChecker.Create(projectCacheSize=1,keepAssemblyContents=true,keepAllBackgroundResolutions=true,keepAllBackgroundSymbolUses=true)
let attempt fallback action = try action() with _ -> fallback
let location (r:range) = box {|path=r.FileName;startLine=r.StartLine-1;startCharacter=r.StartColumn;endLine=r.EndLine-1;endCharacter=r.EndColumn|}
let symbolInfo (s:FSharpSymbol) =
    box {|kind=s.GetType().Name;displayName=s.DisplayName;fullName=attempt "" (fun () -> s.FullName);assembly=attempt "" (fun () -> s.Assembly.QualifiedName);declaration=attempt null (fun () -> s.DeclarationLocation |> Option.map location |> Option.toObj);isAbbreviation=(match s with :? FSharpEntity as e -> e.IsFSharpAbbreviation | _ -> false)|}
let manifest = JsonDocument.Parse(File.ReadAllText(fsi.CommandLineArgs[1]))
for item in manifest.RootElement.EnumerateArray() do
    let path=item.GetProperty("path").GetString()
    let source=File.ReadAllText path
    let baseOptions,optionDiagnostics=checker.GetProjectOptionsFromScript(path,SourceText.ofString source,assumeDotNetFramework=false) |> Async.RunSynchronously
    let options =
        match item.TryGetProperty("projectFiles") with
        | true, files -> { baseOptions with ProjectFileName=path+".fsproj"; SourceFiles=files.EnumerateArray() |> Seq.map(fun x -> x.GetString()) |> Seq.toArray; UseScriptResolutionRules=false }
        | _ -> baseOptions
    let project = checker.ParseAndCheckProject(options) |> Async.RunSynchronously
    let _, answer=checker.ParseAndCheckFileInProject(path,0,SourceText.ofString source,options) |> Async.RunSynchronously
    match answer with
    | FSharpCheckFileAnswer.Aborted -> printfn "%s" (JsonSerializer.Serialize {|case=item.GetProperty("case").GetString();aborted=true|})
    | FSharpCheckFileAnswer.Succeeded checkedFile ->
        let errors=Array.concat [Seq.toArray optionDiagnostics; checkedFile.Diagnostics; project.Diagnostics] |> Array.filter(fun d -> d.Severity=FSharpDiagnosticSeverity.Error) |> Array.map(fun d -> d.ToString())
        let lines=source.Split('\n')
        let results =
            [|for q in item.GetProperty("queries").EnumerateArray() do
                let line=q.GetProperty("line").GetInt32()
                let column=q.GetProperty("character").GetInt32()+q.GetProperty("token").GetString().Length
                let partial=QuickParse.GetPartialLongNameEx(lines[line],column-1)
                let names=partial.QualifyingIdents @ [partial.PartialIdent]
                let useOpt=checkedFile.GetSymbolUseAtLocation(line+1,column,lines[line],names)
                yield box {|label=q.GetProperty("label").GetString();line=line;column=column;names=names;symbol=(useOpt |> Option.map(fun u -> symbolInfo u.Symbol) |> Option.toObj);projectReferences=(useOpt |> Option.map(fun u -> project.GetUsesOfSymbol(u.Symbol) |> Array.map(fun x -> {|range=location x.Range;isDefinition=x.IsFromDefinition|})) |> Option.defaultValue [||]);references=(useOpt |> Option.map(fun u -> checkedFile.GetUsesOfSymbolInFile(u.Symbol) |> Array.map(fun x -> {|range=location x.Range;isDefinition=x.IsFromDefinition|})) |> Option.defaultValue [||]);goto=attempt "error" (fun () -> sprintf "%A" (checkedFile.GetDeclarationLocation(line+1,column,lines[line],names)))|}|]
        printfn "%s" (JsonSerializer.Serialize {|case=item.GetProperty("case").GetString();diagnostics=errors;valid=errors.Length=0;results=results|})
    checker.InvalidateAll()
