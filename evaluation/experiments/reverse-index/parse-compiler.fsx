#r @"/workspace/scratch/dd9aeba5da3c/fsharp-navigator/.tools/dotnet/sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll"
open System.IO
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Text
let file=Path.GetFullPath(fsi.CommandLineArgs[1])
let checker=FSharpChecker.Create()
let options={FSharpParsingOptions.Default with SourceFiles=[|file|]; LangVersionText="preview"}
let result=checker.ParseFile(file,SourceText.ofString(File.ReadAllText file),options)|>Async.RunSynchronously
printfn "%A" result.ParseTree
printfn "Diagnostics: %A" result.Diagnostics
