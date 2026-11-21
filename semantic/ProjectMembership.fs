namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text
open System.Text.RegularExpressions
open System.Xml.Linq
open System.Security.Cryptography
open System.Collections.Generic

module ProjectMembership =
    // Official workload manifest targets are part of the pinned SDK install,
    // outside sdk/<version>; their actual imported bytes remain hashed.
    let trustedSdkInput (sdkRoot:string) (file:string) =
        let installation=Path.GetDirectoryName(Path.GetDirectoryName sdkRoot)
        sdkRoot<>"" && (Paths.within sdkRoot file || Paths.within (Path.Combine(installation,"sdk-manifests")) file)

    let private capture root recursive extensions =
        let names=ResizeArray<byte array>()
        let rec walk directory =
            if Directory.Exists directory then
                if (File.GetAttributes(directory) &&& FileAttributes.ReparsePoint)<>enum<FileAttributes> 0 then failwith $"Symlink/reparse membership directory is unsupported: {directory}"
                for path in Directory.EnumerateFiles directory do
                    if Array.contains "*" extensions || (extensions |> Array.contains(Path.GetExtension(path).TrimStart('.').ToLowerInvariant())) then
                        if (File.GetAttributes(path) &&& FileAttributes.ReparsePoint)<>enum<FileAttributes> 0 then failwith $"Symlink/reparse source membership is unsupported: {path}"
                        names.Add(Encoding.UTF8.GetBytes(Path.GetRelativePath(root,path).Replace('\\','/')))
                        if names.Count>1000000 then failwith "Membership inventory limit exceeded"
                if recursive then for child in Directory.EnumerateDirectories directory do walk child
        walk root
        let sorted=names.ToArray()
        // F# generic array comparison considers length before contents. The
        // cross-language certificate contract is lexicographic UTF-8 bytes.
        Array.sortInPlaceWith (fun (left:byte array) (right:byte array) ->
            let mutable offset=0
            let mutable result=0
            while result=0 && offset<min left.Length right.Length do
                result<-compare left[offset] right[offset]
                offset<-offset+1
            if result=0 then compare left.Length right.Length else result) sorted
        use hash=IncrementalHash.CreateHash(HashAlgorithmName.SHA256)
        for name in sorted do hash.AppendData name;hash.AppendData [|0uy|]
        let digest=hash.GetHashAndReset() |> Convert.ToHexString |> fun s -> s.ToLowerInvariant()
        {root=Paths.canonical root;recursive=recursive;extensions=extensions;excludedDirectories=[||];sha256=digest;entries=sorted.Length}
    // This narrow whitelist is deterministic and has no filesystem or ambient
    // reads beyond the ordinary property arguments observed by the logger.
    let private unsafePropertyFunction (text:string) =
        let stripped=Regex.Replace(text,"""\$\(\[System\.String\]::Copy\('(?:[^'$]|\$\([A-Za-z_][A-Za-z0-9_-]*\))*'\)\.(?:Length|Substring\(\s*\d+\s*,\s*\d+\s*\))\)""","")
        let atom="""(?:-?\d+|\$\([A-Za-z_][A-Za-z0-9_-]*\))"""
        let stripped=Regex.Replace(stripped,"""\$\(\[MSBuild\]::Modulo\(\s*"""+atom+"""\s*,\s*"""+atom+"""\s*\)\)""","")
        stripped.Contains("$([")
    // Only these analyzer-only assignments are source-shape neutral under the
    // pinned F# SDK. This is structural validation, not trust by package name.
    let private analyzerNeutral (element:XElement) =
        let attribute (e:XElement) name=let value=e.Attribute(XName.Get name) in if isNull value then "" else value.Value
        let safeExpression (text:string) =
            let rest=Regex.Replace(text,"\\$\\(\\[System\\.String\\]::Copy\\('[^']*'\\)\\.IndexOf\\('[^']*'\\)\\)","")
            not(rest.Contains("$([")) && not(Regex.IsMatch(rest,"\\)\\s*\\.\\s*[A-Za-z_]\\w*\\s*\\(")) &&
            (Regex.Matches(rest,"\\$\\([A-Za-z_]\\w*\\.([A-Za-z_]\\w*)\\(") |> Seq.cast<Match> |> Seq.forall(fun m -> ["Replace";"ToLowerInvariant"] |> List.contains m.Groups[1].Value))
        let item name=List.contains name ["Analyzer";"EditorConfigFiles";"AdditionalFiles";"CompilerVisibleProperty";"EmbeddedResourceWithResxExtension";"_ObsoleteAnalyzerJson"] || Regex.IsMatch(name,"^_Microsoft_Extensions_[A-Za-z_]+Analyzer$")
        let property (name:string)=name.StartsWith("_GlobalAnalyzerConfig",StringComparison.Ordinal) || List.contains name ["EffectiveCodeAnalysisTreatWarningsAsErrors";"_SupportedPlatformList"]
        let rec node (e:XElement) =
            let safeValues=(e.Attributes() |> Seq.forall(fun a -> safeExpression a.Value)) && safeExpression e.Value
            safeValues &&
            match e.Name.LocalName with
            | "Target" ->
                let attributes=Set.ofList ["Name";"Condition";"BeforeTargets";"AfterTargets";"DependsOnTargets"]
                (e.Attributes() |> Seq.forall(fun a -> attributes.Contains a.Name.LocalName)) &&
                not((attribute e "DependsOnTargets").Contains("$(")) &&
                (e.Elements() |> Seq.forall node)
            | "ItemGroup" -> e.Elements() |> Seq.forall(fun child -> item child.Name.LocalName && not(child.HasElements) && (child.Attributes() |> Seq.forall(fun a -> safeExpression a.Value)))
            | "PropertyGroup" -> e.Elements() |> Seq.forall(fun child -> property child.Name.LocalName && not(child.HasElements) && safeExpression child.Value && (child.Attributes() |> Seq.forall(fun a -> safeExpression a.Value)))
            | "Warning" | "Message" | "Error" -> not e.HasElements
            | _ -> false
        node element

    /// Exact evaluated order is not proof of future item membership. Capture
    /// literal wildcard domains; unknown custom rules refuse persistent reuse.
    let analyze (projectPath:string) (sdkRoot:string) (nugetRoot:string) defaultCompile environmentCertified (importPatterns:string array) (actualImports:string array) (initialInputs:string array) =
        let projectDir=Path.GetDirectoryName projectPath
        let inputs=HashSet<string>(initialInputs,Paths.comparer)
        let seen=HashSet<string>(Paths.comparer)
        let importedXml=HashSet<string>(actualImports,Paths.comparer)
        let pending=Queue<string>(initialInputs)
        let domains=HashSet<string*bool*string>()
        let diagnostics=ResizeArray<string>()
        let addInput path=if inputs.Add path then pending.Enqueue path
        let expand (definingFile:string) (value:string)=
            let value=value.Replace("$(MSBuildProjectDirectory)",projectDir,StringComparison.OrdinalIgnoreCase).Replace("$(MSBuildThisFileDirectory)",Path.GetDirectoryName(definingFile)+string Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase)
            if nugetRoot="" then value else value.Replace("$(NuGetPackageRoot)",nugetRoot,StringComparison.OrdinalIgnoreCase)
        let unresolved (v:string)=v.Contains("$(") || v.Contains("@(") || v.Contains("%(")
        let addGlob extensions (pattern:string)=
            let normalized=pattern.Replace('\\',Path.DirectorySeparatorChar)
            let wildcard=normalized.IndexOfAny([|'*';'?'|])
            if wildcard>=0 then
                let extension=Path.GetExtension(normalized).TrimStart('.').ToLowerInvariant()
                if not(Array.contains "*" extensions) && not(extensions |> Array.contains extension) then
                    diagnostics.Add($"Wildcard extension domain cannot be certified: {pattern}")
                else
                    let prefix=normalized.Substring(0,wildcard)
                    let slash=prefix.LastIndexOf(Path.DirectorySeparatorChar)
                    let directory=if slash<0 then "." elif slash=0 then string Path.DirectorySeparatorChar else prefix.Substring(0,slash)
                    let recursive=normalized.Substring(wildcard).Contains(string Path.DirectorySeparatorChar)
                    domains.Add(Paths.absolute projectDir directory,recursive,String.concat ";" extensions) |> ignore
        for pattern in importPatterns do
            let extension=Path.GetExtension(pattern).TrimStart('.').ToLowerInvariant()
            let extensions=if ["props";"targets";"fsproj"] |> List.contains extension then [|extension|] else [|"*"|]
            addGlob extensions pattern
        if defaultCompile then domains.Add(projectDir,true,"fs;fsi;fsx") |> ignore
        while pending.Count>0 do
            let file=pending.Dequeue()
            if seen.Add file && File.Exists file && (importedXml.Contains file || List.contains (Path.GetExtension(file).ToLowerInvariant()) [".fsproj";".props";".targets"]) then
                let trustedSdk=trustedSdkInput sdkRoot file && not(Paths.comparer.Equals(file,projectPath))
                if not trustedSdk then
                    try
                        let document=XDocument.Load file
                        let targets=document.Descendants() |> Seq.filter(fun e -> e.Name.LocalName="Target") |> Seq.toArray
                        let neutral=HashSet<XElement>()
                        if nugetRoot<>"" && Paths.within nugetRoot file then
                            let names=targets |> Array.map(fun e -> let a=e.Attribute(XName.Get "Name") in if isNull a then "" else a.Value) |> Set.ofArray
                            let dependenciesKnown (e:XElement)=
                                let a=e.Attribute(XName.Get "DependsOnTargets")
                                isNull a || a.Value.Split(';',StringSplitOptions.RemoveEmptyEntries) |> Array.forall names.Contains
                            if targets |> Array.forall(fun e -> analyzerNeutral e && dependenciesKnown e) then
                                for e in targets do for nested in e.DescendantsAndSelf() do neutral.Add nested |> ignore
                                for e in document.Root.Elements() do
                                    if e.Name.LocalName<>"Target" && analyzerNeutral e then for nested in e.DescendantsAndSelf() do neutral.Add nested |> ignore
                        for element in document.Descendants() do
                            let attribute name=let value=element.Attribute(XName.Get name) in if isNull value then "" else value.Value
                            match element.Name.LocalName with
                            | _ when neutral.Contains element -> ()
                            | "Target" | "UsingTask" -> diagnostics.Add($"Custom target/task membership requires reevaluation: {file}")
                            | "Import" ->
                                for raw in (attribute "Project").Split(';',StringSplitOptions.RemoveEmptyEntries) do
                                    let value=expand file raw
                                    if unresolved value then diagnostics.Add($"Unresolved imported membership: {file}: {raw}")
                                    elif value.Contains('*') || value.Contains('?') then addGlob [|"props";"targets";"fsproj"|] (Paths.absolute (Path.GetDirectoryName file) value)
                                    else
                                        let imported=Paths.absolute (Path.GetDirectoryName file) value
                                        importedXml.Add imported |> ignore
                                        addInput imported
                            | "Compile" ->
                                for name in ["Include";"Remove"] do
                                    for raw in (attribute name).Split(';',StringSplitOptions.RemoveEmptyEntries) do
                                        let value=expand file raw
                                        if unresolved value then diagnostics.Add($"Dynamic Compile membership: {file}: {raw}")
                                        else addGlob [|"fs";"fsi";"fsx"|] value
                            | _ -> ()
                            if not(neutral.Contains element) then
                                let texts=Seq.append (element.Attributes() |> Seq.map(fun a -> a.Value)) (if element.HasElements then Seq.empty else Seq.singleton element.Value)
                                if texts |> Seq.exists(fun text -> unsafePropertyFunction text) then diagnostics.Add($"Custom property-function dependency requires reevaluation: {file}")
                                let condition=attribute "Condition"
                                let selectedProperties=Set.ofList ["Configuration";"TargetFramework";"MSBuildProjectDirectory";"MSBuildThisFileDirectory"]
                                let unknown=Regex.Matches(condition,"\\$\\(([A-Za-z_]\\w*)(?=[.)])") |> Seq.cast<Match> |> Seq.exists(fun m -> not(selectedProperties.Contains m.Groups[1].Value))
                                if (unknown && not environmentCertified) || unsafePropertyFunction condition then diagnostics.Add($"Uncertified ambient/property condition requires reevaluation: {file}: {condition}")
                                for matched in Regex.Matches(attribute "Condition","(?i)Exists\\s*\\(\\s*['\"]([^'\"]+)['\"]") |> Seq.cast<Match> do
                                    let value=expand file matched.Groups[1].Value
                                    if unresolved value then diagnostics.Add($"Dynamic existence condition: {file}")
                                    else
                                        addInput(Paths.absolute projectDir value)
                                        addInput(Paths.absolute (Path.GetDirectoryName file) value)
                    with error -> diagnostics.Add($"Cannot certify membership input {file}: {error.Message}")
        let stamps=ResizeArray<SourceMembership>()
        for root,recursive,extensions in domains do
            try stamps.Add(capture root recursive (extensions.Split(';')))
            with error -> diagnostics.Add error.Message
        stamps.ToArray(),diagnostics.Count=0,diagnostics |> Seq.distinct |> Seq.toArray,inputs |> Seq.toArray
