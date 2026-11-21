namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text.Json
open System.Text.RegularExpressions
open System.Xml.Linq
open System.Collections.Generic

module Evaluation =
    let private excluded = set [".git";"node_modules";"bin";"obj";".cache";".tools";"target";"artifacts"]
    let rec private discover directory = seq {
        for file in Directory.EnumerateFiles(directory,"*.fsproj") do yield Paths.canonical file
        for child in Directory.EnumerateDirectories directory do
            if not (excluded.Contains(Path.GetFileName child)) then yield! discover child
    }
    let private selectedProjects root (parameters: JsonElement) =
        let explicit = Json.array "projects" parameters |> Array.map (fun x -> Paths.absolute root (x.GetString()))
        if explicit.Length > 0 then explicit else
        let solution = Json.str "solution" "" parameters
        if solution = "" then discover root |> Seq.toArray else
        let file = Paths.absolute root solution
        let dir = Path.GetDirectoryName file
        if file.EndsWith(".fsproj",StringComparison.OrdinalIgnoreCase) then [|file|]
        elif file.EndsWith(".slnx",StringComparison.OrdinalIgnoreCase) then
            XDocument.Load(file).Descendants()
            |> Seq.filter (fun x -> x.Name.LocalName = "Project")
            |> Seq.choose (fun x -> let a=x.Attribute(XName.Get "Path") in if isNull a then None else Some a.Value)
            |> Seq.filter (fun p -> p.EndsWith(".fsproj",StringComparison.OrdinalIgnoreCase))
            |> Seq.map (Paths.absolute dir) |> Seq.toArray
        elif file.EndsWith(".sln",StringComparison.OrdinalIgnoreCase) then
            Regex.Matches(File.ReadAllText file,"(?m)^Project\\([^\\r\\n]*?=\\s*\"[^\"]*\",\\s*\"([^\"]+\\.fsproj)\"")
            |> Seq.cast<Match> |> Seq.map (fun m -> Paths.absolute dir m.Groups[1].Value) |> Seq.toArray
        else failwith "Expected .sln, .slnx or .fsproj solution selection"

    let private arguments (path:string) tfm configuration = [
            yield! ["msbuild"; path; "-nologo"; "-verbosity:quiet"; "-nodeReuse:false"; "-maxCpuCount:1"; "-noAutoResponse"; "-p:BuildInParallel=false"
                    "-p:DesignTimeBuild=true"; "-p:SkipCompilerExecution=true"; "-p:ProvideCommandLineArgs=true"
                    "-p:BuildProjectReferences=false"; "-p:GeneratePackageOnBuild=false"; "-p:UseSharedCompilation=false"
                    "-p:NonExistentFile=__FSharpNavigatorNonExistent__/__CompilerInputSentinel__"]
            yield "-p:Configuration="+configuration
            if tfm <> "" then yield "-p:TargetFramework="+tfm
        ]

    let private invoke (path:string) tfm configuration targets =
        let arguments = [
            yield! arguments path tfm configuration
            if targets then yield "-target:Compile"
            yield "-getProperty:TargetFramework,TargetFrameworks,TargetPath,TargetRefPath,MSBuildAllProjects,ProjectAssetsFile,MSBuildToolsPath,NuGetPackageRoot,EnableDefaultCompileItems"
            yield "-getItem:Compile,ProjectReference,ReferencePath,ReferencePathWithRefAssemblies,FscCommandLineArgs"
        ]
        let code, output, errors = Processes.runWithEnvironment EvaluationEnvironment.overrides (Processes.dotnet()) arguments (Path.GetDirectoryName path) 120000
        if code <> 0 then failwith $"MSBuild evaluation failed for {path}: {errors}\n{output}"
        let beginJson = output.IndexOf('{')
        if beginJson < 0 then failwith $"MSBuild returned no JSON for {path}: {output}"
        JsonDocument.Parse(output.Substring(beginJson))

    let project path tfm configuration =
        let environmentStart=EvaluationEnvironment.snapshot()
        let preliminaryTrace=EvaluationEnvironment.trace path ((arguments path tfm configuration) @ ["-target:_CheckForInvalidConfigurationAndPlatform"])
        use preliminary = invoke path tfm configuration false
        let property name (doc: JsonDocument) =
            match Json.tryProp "Properties" doc.RootElement with Some x -> Json.str name "" x | _ -> ""
        let selectedTfm =
            if tfm <> "" then tfm else
            let single = property "TargetFramework" preliminary
            if single <> "" then single else
            property "TargetFrameworks" preliminary |> fun s -> s.Split(';',StringSplitOptions.RemoveEmptyEntries) |> Array.tryHead |> Option.defaultValue ""
        let evaluatedTrace=EvaluationEnvironment.trace path ((arguments path selectedTfm configuration) @ ["-target:Compile"])
        let traces=[|preliminaryTrace;evaluatedTrace|]
        use evaluated = invoke path selectedTfm configuration true
        let root = evaluated.RootElement
        let itemRoot = Json.tryProp "Items" root |> Option.defaultWith (fun () -> failwith "MSBuild JSON missing Items")
        let items name = Json.array name itemRoot
        let dir = Path.GetDirectoryName path
        let getPath item = Paths.absolute dir (Json.str "FullPath" (Json.str "Identity" "" item) item)
        let compile = items "Compile"
        let args = items "FscCommandLineArgs" |> Array.map (Json.str "Identity" "")
        let trimQuoted (s: string) = s.Trim().Trim('"')
        let isSource (s: string) =
            let x = trimQuoted s
            not (x.StartsWith("-")) && (x.EndsWith(".fs",StringComparison.OrdinalIgnoreCase) || x.EndsWith(".fsi",StringComparison.OrdinalIgnoreCase) || x.EndsWith(".fsx",StringComparison.OrdinalIgnoreCase))
        let fromArgs = args |> Array.filter isSource |> Array.map (trimQuoted >> Paths.absolute dir)
        if fromArgs.Length = 0 then failwith $"MSBuild did not provide ordered FscCommandLineArgs for {path}; no compiler input order can be certified. Restore the project and verify the selected SDK."
        let metadata = Dictionary<string,JsonElement>(Paths.comparer)
        for item in compile do metadata[getPath item] <- item
        let projectItems = fromArgs |> Array.mapi (fun index file ->
            let virtualPath =
                match metadata.TryGetValue file with
                | true,item ->
                    let link = Json.str "Link" "" item
                    if link <> "" then link.Replace('\\','/')
                    elif Paths.within dir file then Path.GetRelativePath(dir,file).Replace('\\','/')
                    else Path.GetFileName file
                | _ -> if Paths.within dir file then Path.GetRelativePath(dir,file).Replace('\\','/') else Path.GetFileName file
            {path=file;virtualPath=virtualPath;compileIndex=index})
        let refs = items "ProjectReference" |> Array.map getPath
        let referenceItems = Array.append (items "ReferencePath") (items "ReferencePathWithRefAssemblies")
        let assemblies = referenceItems |> Array.map getPath |> Array.distinct
        let projectAssemblies = referenceItems |> Array.filter(fun item ->
            Json.str "ReferenceSourceTarget" "" item = "ProjectReference" ||
            Json.str "MSBuildSourceProjectFile" "" item <> "" ||
            Json.str "ProjectReferenceOriginalItemSpec" "" item <> "") |> Array.map getPath
        let ownAssemblies = [|property "TargetPath" evaluated;property "TargetRefPath" evaluated|] |> Array.filter((<>) "") |> Array.map(Paths.absolute dir)
        let workspaceAssemblies = Array.append projectAssemblies ownAssemblies |> Array.distinct
        let imports = (property "MSBuildAllProjects" evaluated).Split(';',StringSplitOptions.RemoveEmptyEntries)
        let assets = property "ProjectAssetsFile" evaluated
        // Source order and reference selection depend on project shape, not on
        // the bytes of an otherwise unchanged referenced output DLL. Missing
        // ambient files are retained so their later creation is detectable.
        let rec ambient directory = seq {
            for name in ["global.json";"Directory.Build.props";"Directory.Build.targets";"Directory.Packages.props";"NuGet.Config";"nuget.config"] do
                yield Path.Combine(directory,name)
            let parent=Directory.GetParent directory
            if not(isNull parent) then yield! ambient parent.FullName
        }
        let inputs = Array.concat [ [|path|]; EvaluationEnvironment.existenceProbes traces; EvaluationEnvironment.imports traces; imports |> Array.map(Paths.absolute dir); if assets="" then [||] else [|Paths.absolute dir assets|]; ambient dir |> Seq.toArray ] |> Array.distinct
        let environmentCertificate,toolInputs,environmentNotes=EvaluationEnvironment.certify environmentStart (property "MSBuildToolsPath" evaluated) inputs traces
        let membership,reusable,membershipNotes,inputs=ProjectMembership.analyze path (property "MSBuildToolsPath" evaluated) (property "NuGetPackageRoot" evaluated) ((property "EnableDefaultCompileItems" evaluated).Equals("true",StringComparison.OrdinalIgnoreCase)) (environmentNotes.Length=0) (EvaluationEnvironment.importPatterns traces) (EvaluationEnvironment.imports traces) inputs
        let inputs=Array.append inputs toolInputs |> Array.distinct
        let membershipNotes=Array.append membershipNotes environmentNotes
        let reusable=reusable && environmentNotes.Length=0
        let bindings=assemblies |> Array.map(fun assembly ->
            let exists=File.Exists assembly
            let identity=if exists then System.Reflection.AssemblyName.GetAssemblyName(assembly).FullName else ""
            {path=assembly;identity=identity;exists=exists;isWorkspace=Array.contains assembly workspaceAssemblies})
        {path=path;name=Path.GetFileNameWithoutExtension path;targetFramework=selectedTfm;configuration=configuration;files=fromArgs;references=refs;assemblyReferences=assemblies;workspaceAssemblyReferences=workspaceAssemblies;items=projectItems;compilerArgs=args;outputPath=property "TargetPath" evaluated;inputs=inputs;referenceBindings=bindings;evaluation="design-time-fsc-arguments";sourceMembership=membership;membershipReusable=reusable;membershipDiagnostics=membershipNotes;environmentCertificate=environmentCertificate}

    let workspace (parameters: JsonElement) =
        let root = Json.str "root" "" parameters |> Paths.canonical
        if not (Directory.Exists root) then failwith $"Workspace directory not found: {root}"
        let tfm = Json.str "targetFramework" "" parameters
        let configuration = Json.str "configuration" "Debug" parameters
        let initial = selectedProjects root parameters
        if initial.Length=0 then failwith "No F# projects found; script evaluation is not yet supported"
        let seen = HashSet<string>(Paths.comparer)
        let results = ResizeArray<ProjectModel>()
        let pending = Queue<string>(initial)
        while pending.Count>0 do
            let path = pending.Dequeue()
            if seen.Add path then
                let model = project path tfm configuration
                results.Add model
                for reference in model.references do
                    if reference.EndsWith(".fsproj",StringComparison.OrdinalIgnoreCase) then pending.Enqueue reference
        let selection=Json.str "solution" "" parameters
        let inputs=Seq.append (results |> Seq.collect (fun p -> p.inputs)) (if selection="" then Seq.empty else Seq.singleton(Paths.absolute root selection)) |> Seq.distinct |> Seq.toArray
        let shapeHashes=inputs |> Array.map Fingerprints.state
        let semanticInputs=Seq.append inputs (results |> Seq.collect(fun p -> p.assemblyReferences)) |> Seq.distinct |> Seq.toArray
        {root=root;projects=results.ToArray();inputs=inputs;inputHashes=shapeHashes;projectInputHashes=shapeHashes;semanticInputHashes=semanticInputs |> Array.map Fingerprints.state;completeness="exact";diagnostics=[||]}
