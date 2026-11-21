namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text
open System.Text.Json
open System.Text.RegularExpressions
open System.Security.Cryptography
open System.Collections.Generic
open System.Xml.Linq

/// A bounded certificate for the pinned ordinary .NET SDK evaluation path.
/// This is not an interceptor for arbitrary custom tasks or SDK resolvers.
module EvaluationEnvironment =
    let contract = "dotnet-msbuild-observed-v4"
    let overrides = [
        "MsBuildLogPropertyTracking","15"; "MSBUILDLOGIMPORTS","1"
        "MSBUILDLOGALLENVIRONMENTVARIABLES","0"
        "DOTNET_NOLOGO","1"; "DOTNET_CLI_TELEMETRY_OPTOUT","1"
        "MSBUILDDISABLENODEREUSE","1"; "DOTNET_CLI_USE_MSBUILD_SERVER","0" ]
    let private fixedNames=HashSet<string>(overrides |> Seq.map fst,StringComparer.OrdinalIgnoreCase)
    // Resolver/host reads happen before evaluator events. These supported knobs
    // are certified separately; arbitrary additional resolver folders reject.
    let private resolverNames = [|
        "FSHARP_NAVIGATOR_DOTNET";"MSBuildSDKsPath";"MSBuildExtensionsPath";"MSBuildExtensionsPath32";"MSBuildExtensionsPath64"
        "MSBUILD_EXE_PATH";"MSBUILDADDITIONALSDKRESOLVERSFOLDER";"MSBUILD_NUGET_PATH"
        "MSBuildEnableWorkloadResolver";"MSBuildSDKResolverCacheFile";"MSBuildSDKsPath"
        "DOTNET_MSBUILD_SDK_RESOLVER_SDKS_DIR";"DOTNET_MSBUILD_SDK_RESOLVER_SDKS_VER";"DOTNET_MSBUILD_SDK_RESOLVER_CLI_DIR"
        "DOTNET_ROOT";"DOTNET_ROOT_X64";"DOTNET_ROOT_X86";"DOTNET_ROOT_ARM64";"DOTNET_ROOT(x86)"
        "DOTNET_HOST_PATH";"DOTNET_MULTILEVEL_LOOKUP";"DOTNET_ROLL_FORWARD";"DOTNET_ROLL_FORWARD_TO_PRERELEASE"
        "DOTNETSDK_WORKLOAD_MANIFEST_ROOTS";"DOTNETSDK_WORKLOAD_PACK_ROOTS";"DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE"
        "DOTNET_CLI_RUN_MSBUILD_OUTOFPROC";"DOTNET_CLI_USE_MSBUILDNOINPROCNODE";"MSBUILDNOINPROCNODE"
        "MSBUILDENABLEALLPROPERTYFUNCTIONS";"MSBUILDNOAUTORESPONSE" |]
    let snapshot () =
        let values=Dictionary<string,string>(StringComparer.OrdinalIgnoreCase)
        for entry in Environment.GetEnvironmentVariables() |> Seq.cast<System.Collections.DictionaryEntry> do
            let name=string entry.Key
            let value=string entry.Value
            match values.TryGetValue name with
            | true,previous when previous<>value -> failwith $"Conflicting case-insensitive ambient variable names: {name}"
            | _ -> values[name] <- value
        values
    let private valueHash (ambient:Dictionary<string,string>) name =
        match ambient.TryGetValue name with
        | true,value -> SHA256.HashData(Encoding.UTF8.GetBytes value) |> Convert.ToHexString |> fun s -> s.ToLowerInvariant()
        | _ -> "missing"
    let loggerPath () = Path.Combine(AppContext.BaseDirectory,"FSharpNavigator.EvaluationLogger.dll")
    let trace (path:string) (arguments:string list) =
        let logger=loggerPath()
        if not(File.Exists logger) then failwith "Evaluation event logger is missing; rebuild the semantic helper"
        let output=Path.Combine(Path.GetTempPath(),"fsharp-evaluation-"+Guid.NewGuid().ToString("N")+".json")
        try
            let argument="-logger:ConsumptionLogger,"+logger+";"+output
            let code,_,errors=Processes.runWithEnvironment overrides (Processes.dotnet()) (arguments @ [argument]) (Path.GetDirectoryName path) 120000
            if code<>0 then failwith $"MSBuild dependency trace failed for {path}: {errors}"
            if not(File.Exists output) then failwith "MSBuild did not produce an evaluation dependency trace"
            use document=JsonDocument.Parse(File.ReadAllText output)
            let result=document.RootElement.Clone()
            if Json.integer "schemaVersion" 0 result<>1 || Json.integer "evaluations" 0 result<1 || Json.integer "missingReads" 0 result<1 then
                failwith "MSBuild dependency trace lacks required evaluation/read events"
            result
        finally
            for file in [output;output+".partial"] do if File.Exists file then File.Delete file
    let imports (traces:JsonElement array) =
        traces |> Array.collect(fun trace -> Json.array "imports" trace |> Array.map(fun x -> Paths.canonical(x.GetString()))) |> Array.distinct
    let importPatterns (traces:JsonElement array) = traces |> Array.collect(fun trace -> Json.array "importPatterns" trace |> Array.map(fun x -> x.GetString())) |> Array.distinct
    let existenceProbes (traces:JsonElement array) = traces |> Array.collect(fun trace -> Json.array "existenceProbes" trace |> Array.map(fun x -> x.GetString())) |> Array.distinct
    let certify (ambient:Dictionary<string,string>) sdkRoot (inputs:string array) (traces:JsonElement array) =
        let diagnostics=ResizeArray<string>()
        let names=HashSet<string>(StringComparer.OrdinalIgnoreCase)
        let globals=HashSet<string>(StringComparer.OrdinalIgnoreCase)
        if traces.Length>0 then
            globals.UnionWith(Json.array "fixedGlobals" traces[0] |> Array.map(fun x -> x.GetString()))
            for trace in traces do globals.IntersectWith(Json.array "fixedGlobals" trace |> Array.map(fun x -> x.GetString()))
        for trace in traces do
            for diagnostic in Json.array "diagnostics" trace do diagnostics.Add(diagnostic.GetString())
            for name in Json.array "names" trace |> Array.map(fun x -> x.GetString()) do names.Add name |> ignore
        // Missing target-execution properties have no property instance and
        // emit no negative event. Inspect actual imported XML for only absent
        // ambient names. Present unused PATH/PWD/editor variables stay absent.
        let actualXml=HashSet<string>(imports traces,Paths.comparer)
        for file in inputs do
            if File.Exists file && (actualXml.Contains file || ([".props";".targets";".fsproj"] |> List.contains(Path.GetExtension(file).ToLowerInvariant()))) then
                try
                    let document=XDocument.Load file
                    for element in document.Descendants() do
                        let texts=Seq.append (element.Attributes() |> Seq.map(fun a -> a.Value)) (if element.HasElements then Seq.empty else Seq.singleton element.Value)
                        for text in texts do
                            for matched in Regex.Matches(text,"\\$\\(([A-Za-z_][A-Za-z0-9_-]*)(?=[.)])") |> Seq.cast<Match> do
                                let name=matched.Groups[1].Value
                                if not(ambient.ContainsKey name) then names.Add name |> ignore
                        if not(ProjectMembership.trustedSdkInput sdkRoot file) then
                            let sdk=element.Attribute(XName.Get "Sdk")
                            if not(isNull sdk) && sdk.Value<>"Microsoft.NET.Sdk" then diagnostics.Add($"Custom SDK environment dependency is unsupported: {file}")
                with error -> diagnostics.Add($"Cannot inspect evaluation dependency XML {file}: {error.Message}")
        for key in Environment.GetEnvironmentVariables().Keys |> Seq.cast<string> do
            for name in resolverNames do
                if String.Equals(key,name,StringComparison.OrdinalIgnoreCase) && key<>name then diagnostics.Add($"Noncanonical case of startup environment selector: {key}")
        for name in resolverNames do names.Add name |> ignore
        for name in GlobalizationProfile.selectors do names.Add name |> ignore
        let globalization,globalizationFiles,globalizationNotes=GlobalizationProfile.certify ambient sdkRoot traces
        diagnostics.AddRange globalizationNotes
        if not(Path.IsPathRooted(Processes.dotnet())) then names.Add "PATH" |> ignore
        if Path.GetFileName(sdkRoot.TrimEnd(Path.DirectorySeparatorChar))<>"10.0.400" then diagnostics.Add "Observed environment contract currently requires the tested .NET SDK10.0.400"
        for name in ["MSBUILDADDITIONALSDKRESOLVERSFOLDER";"MSBUILD_NUGET_PATH";"MSBUILDENABLEALLPROPERTYFUNCTIONS"] do
            match ambient.TryGetValue name with
            | true,value when value<>"" -> diagnostics.Add($"Custom resolver/property-function environment is unsupported: {name}")
            | _ -> ()
        // Hash the tested implementation and actual import bytes as ordinary
        // model inputs. Values are never emitted, even in error diagnostics.
        let toolFiles=[|"MSBuild.dll";"Microsoft.Build.dll";"Microsoft.Build.Framework.dll";"Microsoft.NET.Sdk.WorkloadMSBuildSdkResolver.dll";"Microsoft.DotNet.MSBuildSdkResolver.dll"|]
                      |> Array.map(fun name -> Path.Combine(sdkRoot,name)) |> Array.filter File.Exists
        let selectedHost=Processes.dotnet()
        let actualHost=Environment.ProcessPath
        let hostInputs=
            if not(Path.IsPathRooted selectedHost) || String.IsNullOrEmpty actualHost || Path.GetFileNameWithoutExtension(actualHost)<>"dotnet" then
                diagnostics.Add "Observed evaluation requires the explicit pinned dotnet host; PATH-selected hosts and apphost/wrapper overrides are unsupported"
                [||]
            elif not(File.Exists selectedHost) || (Fingerprints.file selectedHost).sha256<>(Fingerprints.file actualHost).sha256 then
                diagnostics.Add "Custom dotnet wrapper/executable override cannot certify inherited environment; use the pinned dotnet host"
                [|selectedHost|]
            else [|selectedHost;actualHost|]
        let toolFiles=Array.concat [toolFiles;hostInputs;globalizationFiles;[|loggerPath();typeof<ProjectModel>.Assembly.Location|]] |> Array.distinct
        let toolchain=toolFiles |> Array.map(fun file -> let hash=Fingerprints.state file in hash.path+"="+hash.sha256) |> String.concat "\n" |> Encoding.UTF8.GetBytes |> SHA256.HashData |> Convert.ToHexString |> fun s -> s.ToLowerInvariant()
        let required=HashSet<string>(Array.append resolverNames GlobalizationProfile.selectors,StringComparer.OrdinalIgnoreCase)
        let states=names |> Seq.filter(fun name -> required.Contains name || (not(fixedNames.Contains name) && not(globals.Contains name))) |> Seq.sortWith (fun a b -> StringComparer.OrdinalIgnoreCase.Compare(a,b)) |> Seq.map(fun name -> {name=name;sha256=valueHash ambient name}) |> Seq.toArray
        let certificate={version=4;contract=contract;toolchain=toolchain;inputs=states;traceInvocations=traces.Length;positiveReads=traces |> Array.sumBy(Json.integer "environmentReads" 0);negativeReads=traces |> Array.sumBy(Json.integer "missingReads" 0);globalization=globalization}
        certificate,toolFiles,diagnostics.ToArray()
