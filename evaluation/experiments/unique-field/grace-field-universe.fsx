#r "../../../.tools/dotnet/sdk/10.0.400/FSharp/FSharp.Compiler.Service.dll"
open System
open System.IO
open System.Text.Json
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.Symbols

let modelPath = Path.GetFullPath fsi.CommandLineArgs[1]
let model = JsonDocument.Parse(File.ReadAllText modelPath)
let project = model.RootElement.GetProperty("projects")[0]
let projectPath = project.GetProperty("path").GetString()
Directory.SetCurrentDirectory(Path.GetDirectoryName projectPath)
let checker = FSharpChecker.Create(projectCacheSize=1,keepAssemblyContents=true)
let args = project.GetProperty("compilerArgs").EnumerateArray() |> Seq.map(fun x -> x.GetString()) |> Seq.toArray
let options = checker.GetProjectOptionsFromCommandLineArgs(projectPath,args)
let results = checker.ParseAndCheckProject(options) |> Async.RunSynchronously
let names = Set.ofList ["OnFailure";"Kty";"Artifact";"StoragePoolId";"ArtifactId"]
let records = ResizeArray<obj>()
let members = ResizeArray<obj>()
let failures = ResizeArray<string>()
let providers = ResizeArray<string>()
let mutable entities = 0
let mutable fields = 0
let attempt fallback f = try f() with e -> failures.Add(e.Message); fallback
let location (s:FSharpSymbol) = s.DeclarationLocation |> Option.map(fun r -> box {|path=r.FileName;line=r.StartLine-1;character=r.StartColumn|}) |> Option.toObj
let rec visit origin (entity:FSharpEntity) =
    entities <- entities+1
    let owner = entity.TryFullName |> Option.defaultValue entity.DisplayName
    if entity.IsProvided then providers.Add(owner)
    for field in attempt [||] (fun () -> Seq.toArray entity.FSharpFields) do
        fields <- fields+1
        if names.Contains field.Name then
            records.Add(box {|origin=origin;owner=owner;name=field.Name;isRecord=entity.IsFSharpRecord;isAbbreviation=entity.IsFSharpAbbreviation;assembly=entity.Assembly.QualifiedName;declaration=location field|})
    for memberValue in attempt [||] (fun () -> Seq.toArray entity.MembersFunctionsAndValues) do
        if names.Contains memberValue.DisplayName then
            members.Add(box {|origin=origin;owner=owner;name=memberValue.DisplayName;isProperty=memberValue.IsProperty;assembly=entity.Assembly.QualifiedName;declaration=location memberValue|})
    for nested in attempt [||] (fun () -> Seq.toArray entity.NestedEntities) do visit origin nested
for entity in results.AssemblySignature.Entities do visit "source-project" entity
let refs = results.ProjectContext.GetReferencedAssemblies()
for assembly in refs do
    for entity in attempt [||] (fun () -> Seq.toArray assembly.Contents.Entities) do visit "compiled-reference" entity
let diagnostics = results.Diagnostics |> Array.map(fun d -> {|severity=string d.Severity;message=d.ToString()|})
let bindings = refs |> List.map(fun a -> {|path=a.FileName |> Option.toObj;identity=a.QualifiedName|}) |> List.toArray
let report = {|project=projectPath;model=modelPath;criticalErrors=results.HasCriticalErrors;errors=(results.Diagnostics |> Array.filter(fun d -> d.Severity=FSharpDiagnosticSeverity.Error) |> Array.length);diagnostics=diagnostics;referencedAssemblies=refs.Length;referenceBindings=bindings;visitedEntities=entities;visitedFields=fields;matchingFields=records.ToArray();matchingMembers=members.ToArray();providedEntities=providers.ToArray();enumerationFailures=failures.ToArray()|}
printfn "%s" (JsonSerializer.Serialize report)
