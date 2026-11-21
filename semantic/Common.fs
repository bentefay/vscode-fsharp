namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Diagnostics
open System.Text.Json
open System.Text.Json.Nodes
open System.Security.Cryptography

module Json =
    let options = JsonSerializerOptions(PropertyNamingPolicy = JsonNamingPolicy.CamelCase)
    let node (value: 'T) = JsonSerializer.SerializeToNode(value, options)
    let tryProp name (value: JsonElement) =
        if value.ValueKind = JsonValueKind.Object then
            match value.TryGetProperty(name: string) with
            | true, result -> Some result
            | _ -> None
        else None
    let str name fallback value =
        match tryProp name value with
        | Some x when x.ValueKind = JsonValueKind.String -> x.GetString()
        | _ -> fallback
    let integer name fallback value =
        match tryProp name value with
        | Some x when x.ValueKind = JsonValueKind.Number -> x.GetInt32()
        | _ -> fallback
    let array name value =
        match tryProp name value with
        | Some x when x.ValueKind = JsonValueKind.Array -> x.EnumerateArray() |> Seq.toArray
        | _ -> [||]

module Paths =
    let canonical (path: string) = Path.GetFullPath(path)
    let comparer = if OperatingSystem.IsWindows() then StringComparer.OrdinalIgnoreCase else StringComparer.Ordinal
    let within (root: string) (path: string) =
        let relative = Path.GetRelativePath(canonical root, canonical path)
        relative <> ".." && not (relative.StartsWith(".." + string Path.DirectorySeparatorChar, StringComparison.Ordinal)) && not (Path.IsPathRooted relative)
    let absolute root (path: string) =
        if Path.IsPathRooted path then canonical path else canonical (Path.Combine(root,path.Replace('\\',Path.DirectorySeparatorChar)))

module Processes =
    let dotnet () =
        match Environment.GetEnvironmentVariable("FSHARP_NAVIGATOR_DOTNET") with
        | null | "" ->
            let exe = Environment.ProcessPath
            if not (String.IsNullOrWhiteSpace exe) && Path.GetFileNameWithoutExtension(exe) = "dotnet" then exe else "dotnet"
        | path -> path
    let runWithEnvironment (overrides:(string*string) list) executable (arguments: string list) directory (timeoutMilliseconds: int) =
        let info = ProcessStartInfo(executable)
        info.WorkingDirectory <- directory
        info.RedirectStandardOutput <- true
        info.RedirectStandardError <- true
        info.UseShellExecute <- false
        for arg in arguments do info.ArgumentList.Add arg
        info.Environment["DOTNET_NOLOGO"] <- "1"
        info.Environment["DOTNET_CLI_TELEMETRY_OPTOUT"] <- "1"
        info.Environment["MSBUILDDISABLENODEREUSE"] <- "1"
        info.Environment["DOTNET_CLI_USE_MSBUILD_SERVER"] <- "0"
        for name,value in overrides do info.Environment[name] <- value
        use child = new Process(StartInfo = info)
        if not (child.Start()) then failwith $"Could not start {executable}"
        let stdout = child.StandardOutput.ReadToEndAsync()
        let stderr = child.StandardError.ReadToEndAsync()
        if not (child.WaitForExit(timeoutMilliseconds)) then
            child.Kill(true)
            child.WaitForExit()
            failwith $"Process exceeded {timeoutMilliseconds} ms: {executable}"
        child.ExitCode, stdout.GetAwaiter().GetResult(), stderr.GetAwaiter().GetResult()
    let run executable arguments directory timeoutMilliseconds = runWithEnvironment [] executable arguments directory timeoutMilliseconds

type Position = { line: int; character: int }
type SourceRange = { start: Position; ``end``: Position }
type Location = { path: string; range: SourceRange }
type FileStamp = { path: string; sha256: string }
module Fingerprints =
    let file path =
        use stream=File.OpenRead path
        {path=Paths.canonical path;sha256=SHA256.HashData(stream) |> Convert.ToHexString |> fun s -> s.ToLowerInvariant()}
    let state path = if File.Exists path then file path elif Directory.Exists path then {path=Paths.canonical path;sha256="directory"} else {path=Paths.canonical path;sha256="missing"}
type ProjectItem = { path: string; virtualPath: string; compileIndex: int }
type ReferenceBinding = { path: string; identity: string; exists: bool; isWorkspace: bool }
type SourceMembership = { root:string; recursive:bool; extensions:string array; excludedDirectories:string array; sha256:string; entries:int }
type EnvironmentInput = { name:string; sha256:string }
type EnvironmentCertificate = { version:int; contract:string; toolchain:string; inputs:EnvironmentInput array; traceInvocations:int; positiveReads:int; negativeReads:int; globalization:JsonNode }
type ProjectModel = {
    path: string; name: string; targetFramework: string; configuration: string
    files: string array; references: string array; assemblyReferences: string array; workspaceAssemblyReferences: string array
    items: ProjectItem array; compilerArgs: string array; outputPath: string
    inputs: string array; referenceBindings: ReferenceBinding array; evaluation: string
    sourceMembership:SourceMembership array; membershipReusable:bool; membershipDiagnostics:string array
    environmentCertificate:EnvironmentCertificate
}
type WorkspaceModel = {
    root: string; projects: ProjectModel array; inputs: string array
    inputHashes: FileStamp array; projectInputHashes: FileStamp array; semanticInputHashes: FileStamp array
    completeness: string; diagnostics: string array
}
