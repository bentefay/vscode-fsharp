namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text
open System.Text.Json
open System.Text.Json.Nodes
open System.Text.RegularExpressions
open System.Xml.Linq
open System.Collections.Generic
open System.Security.Cryptography
open System.Reflection
open System.Reflection.Metadata
open System.Reflection.PortableExecutable
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.EditorServices
open FSharp.Compiler.Symbols
open FSharp.Compiler.Text

type CatalogSymbol = {
    factsVersion:int
    id: string; assemblyPath: string; assemblyName: string; fullName: string; name: string
    kind: string; signature: string; xmlDocId: string; documentation: string
    qualifiedNames: string array; isExternal: bool
    typeInfo:JsonNode; memberInfo:JsonNode; callableInfo:JsonNode
}

/// The optional artifact path avoids retaining both the catalog records and a
/// duplicate JsonNode/string representation in the helper's protocol response.
[<AllowNullLiteral>]
type private CatalogSink(outputPath:string,producer:JsonNode,xmlSelections:MetadataXmlSelection array) =
    let target=Paths.canonical outputPath
    let temporary=target+"."+Guid.NewGuid().ToString("N")+".partial"
    do
        if File.Exists target then failwith $"Catalog output already exists: {target}"
        Directory.CreateDirectory(Path.GetDirectoryName target) |> ignore
    let stream=new FileStream(temporary,FileMode.CreateNew,FileAccess.Write,FileShare.None)
    let writer=new Utf8JsonWriter(stream)
    do
        writer.WriteStartObject()
        writer.WriteNumber("schemaVersion",2)
        writer.WriteNumber("factsVersion",2)
        writer.WriteNumber("metadataCertificateVersion",1)
        writer.WritePropertyName("producer")
        producer.WriteTo(writer)
        writer.WritePropertyName("xmlSelections")
        JsonSerializer.Serialize(writer,xmlSelections,Json.options)
        writer.WriteString("scope","binary-declarations-only")
        writer.WriteString("completeness","exact")
        writer.WriteStartArray("symbols")
    member _.Add(symbol:CatalogSymbol) = JsonSerializer.Serialize(writer,symbol,Json.options)
    member _.Complete(inputs:FileStamp array,bindings:obj array) =
        writer.WriteEndArray()
        writer.WritePropertyName("inputHashes")
        JsonSerializer.Serialize(writer,inputs,Json.options)
        writer.WritePropertyName("dependencyBindings")
        JsonSerializer.Serialize(writer,bindings,Json.options)
        writer.WriteEndObject()
        writer.Flush()
        writer.Dispose()
        stream.Dispose()
        File.Move(temporary,target)
        target
    interface IDisposable with
        member _.Dispose() =
            writer.Dispose()
            stream.Dispose()
            if File.Exists temporary then File.Delete temporary

type SnapshotOccurrence = { symbolId: string; path: string; range: SourceRange; isDefinition: bool; declaration: obj }
type ProjectSnapshot = {
    projectPath: string; contextId: string; targetFramework: string; configuration: string
    sourceHashes: FileStamp array; inputHashes: FileStamp array; compilerArgs: string array
    environmentCertificate: EnvironmentCertificate
    sourceMembership: SourceMembership array; membershipReusable: bool; membershipDiagnostics: string array
    dependencies: string array; completeness: string; diagnostics: string array
    occurrences: SnapshotOccurrence array
}

module Semantic =
    let private attempt fallback f = try f() with _ -> fallback
    let private hash (text: string) = SHA256.HashData(Encoding.UTF8.GetBytes text) |> Convert.ToHexString |> fun s -> s.ToLowerInvariant()
    let private fileHash path = use stream = File.OpenRead path in SHA256.HashData stream |> Convert.ToHexString
    let private range (r: range) : SourceRange =
        {start={line=max 0 (r.StartLine-1);character=r.StartColumn};``end``={line=max 0 (r.EndLine-1);character=r.EndColumn}}

    let private xmlCache = Dictionary<string,Map<string,string>>(Paths.comparer)
    let private xmlDocs (selection:MetadataXmlSelection) =
        if selection.selectedPath="" then Map.empty else
            let file=selection.selectedPath
            let expected=selection.candidates |> Array.find(fun input -> input.path=file)
            let cacheKey = file + ":" + expected.sha256
            match xmlCache.TryGetValue cacheKey with
            | true,docs -> docs
            | _ ->
                let docs=XmlDocumentation.readVerified file expected.sha256
                xmlCache[cacheKey] <- docs
                docs

    let private symbolXmlId (symbol: FSharpSymbol) =
        match symbol with
        | :? FSharpEntity as x -> attempt "" (fun () -> x.XmlDocSig)
        | :? FSharpMemberOrFunctionOrValue as x -> attempt "" (fun () -> x.XmlDocSig)
        | :? FSharpField as x -> attempt "" (fun () -> x.XmlDocSig)
        | :? FSharpUnionCase as x -> attempt "" (fun () -> x.XmlDocSig)
        | _ -> ""
    let private symbolSignature context (symbol: FSharpSymbol) =
        match symbol with
        | :? FSharpEntity as x -> "type " + x.DisplayName
        | :? FSharpMemberOrFunctionOrValue as x ->
            let prefix = if x.IsMember then "member " else "val "
            prefix + x.DisplayName + " : " + attempt "<unavailable>" (fun () -> x.FullType.Format context)
        | :? FSharpField as x -> x.DisplayName + " : " + attempt "<unavailable>" (fun () -> x.FieldType.Format context)
        | :? FSharpUnionCase as x ->
            let fields = x.Fields |> Seq.map (fun f -> f.FieldType.Format context) |> String.concat " * "
            "| " + x.DisplayName + (if fields="" then "" else " of " + fields)
        | _ -> symbol.DisplayName
    let private symbolFullName (symbol:FSharpSymbol) = attempt symbol.DisplayName (fun () -> symbol.FullName)
    let private symbolKind (symbol:FSharpSymbol) =
        match symbol with
        | :? FSharpEntity as x -> if x.IsFSharpModule then "module" elif x.IsNamespace then "namespace" else "type"
        | :? FSharpMemberOrFunctionOrValue as x -> if x.IsMember then "member" else "function"
        | :? FSharpField -> "field"
        | :? FSharpUnionCase -> "unionCase"
        | _ -> "symbol"
    let private documentation assembly (symbol:FSharpSymbol) =
        if String.IsNullOrEmpty assembly then "" else xmlDocs (MetadataXml.capture assembly) |> Map.tryFind (symbolXmlId symbol) |> Option.defaultValue ""

    // The compiler has already selected a path for each CCU/simple assembly
    // name. Follow the PE reference graph through THAT selected set, not a
    // basename search on disk. FCS intentionally unifies framework versions by
    // name (CompilerImports.FindCcuFromAssemblyRef); preserve both identities
    // in the certificate so this resolution is visible rather than guessed.
    let exportMetadataWithContext (shared:MetadataImportContext option) (parameters:JsonElement) =
        let assemblies = Json.array "assemblyPaths" parameters |> Array.map (fun x -> Paths.canonical(x.GetString())) |> Array.distinct
        if assemblies.Length=0 then failwith "metadata/export requires assemblyPaths"
        for path in assemblies do if not (File.Exists path) then failwith $"Assembly does not exist: {path}"
        let xmlSelections=
            match shared |> Option.bind(fun context->context.xmlSelections) with
            | None -> assemblies |> Array.map MetadataXml.capture
            | Some captured -> assemblies |> Array.map(fun path->captured |> Array.tryFind(fun value->Paths.comparer.Equals(value.assemblyPath,path)) |> Option.defaultWith(fun()->failwith "Shared job did not capture requested XML selection"))
        let referencePaths = Json.array "referencePaths" parameters |> Array.map (fun x -> Paths.canonical(x.GetString()))
        let rootHashes=assemblies |> Array.map Fingerprints.file
        // This imports binary metadata only. No workspace source project is supplied.
        let explicitReferences=Array.append referencePaths assemblies |> Array.distinct
        let context=shared |> Option.defaultWith(fun()->MetadataContext.create explicitReferences)
        MetadataContext.require context explicitReferences
        let producer,producerInputs=context.producer.DeepClone(),context.producerInputs
        let before=context.before
        let requested = HashSet<string>(assemblies,Paths.comparer)
        let requestedHashes=rootHashes |> Array.map(fun x -> x.path,x.sha256) |> Map.ofArray
        let requestedFile importedPath =
            let importedPath=Paths.canonical importedPath
            if requested.Contains importedPath then Some importedPath else
            // FCS uses its already-loaded FSharp.Core copy even when #r names
            // the identical SDK binary. Alias only byte-identical assemblies;
            // matching a simple name/version alone would conceal different APIs.
            assemblies |> Array.tryFind(fun requestedPath ->
                String.Equals(Path.GetFileName requestedPath,Path.GetFileName importedPath,StringComparison.OrdinalIgnoreCase) &&
                File.Exists importedPath && (Fingerprints.file importedPath).sha256=requestedHashes[requestedPath])
        let loaded,selected=context.loaded,context.selected
        let bindings=MetadataBindings(selected)
        let typeDependencies=MetadataDependencies()
        let results = ResizeArray<CatalogSymbol>()
        let outputPath=Json.str "outputPath" "" parameters
        use sink=if outputPath="" then null else new CatalogSink(outputPath,producer,xmlSelections)
        let mutable symbolCount=0
        let imported = HashSet<string>(Paths.comparer)
        for assembly in loaded do
            match assembly.FileName |> Option.bind requestedFile with
            | Some file ->
                imported.Add file |> ignore
                let contentHash = fileHash file
                let selection=xmlSelections |> Array.find(fun selection -> Paths.comparer.Equals(selection.assemblyPath,file))
                // The shared job keeps at most this assembly's rendered XML
                // map, never the process-wide cache used by the legacy path.
                let docs =
                    if shared.IsNone then xmlDocs selection
                    elif selection.selectedPath="" then Map.empty
                    else
                        let expected=selection.candidates |> Array.find(fun input->input.path=selection.selectedPath)
                        XmlDocumentation.readVerified selection.selectedPath expected.sha256
                let add qualifiedName (symbol:FSharpSymbol) =
                    let id = symbolXmlId symbol
                    // FCS can expose both an accessor and the property using
                    // the same display/full name and XML id. Keep the compiled
                    // accessor spelling separate so a first-insert SQLite row
                    // cannot replace the property's structured type facts.
                    let accessorName=
                        match symbol with
                        | :? FSharpMemberOrFunctionOrValue as memberValue when memberValue.IsPropertyAccessor ->
                            memberValue.DeclaringEntity |> Option.map(fun owner -> owner.FullName+"."+memberValue.CompiledName)
                        | _ -> None
                    let fullName = accessorName |> Option.defaultWith(fun () -> symbolFullName symbol)
                    let qualifiedName=accessorName |> Option.defaultValue qualifiedName
                    if symbol.Accessibility.IsPublic then
                        typeDependencies.Add symbol
                        let signature=symbolSignature FSharpDisplayContext.Empty symbol
                        if signature.Contains("<unavailable>") then failwith $"Cannot certify unresolved signature: {fullName}"
                        let typeInfo,memberInfo=MetadataTypes.facts bindings.ResolveType bindings.ResolveAssembly fullName symbol
                        let callableInfo=MetadataTypes.callableFacts bindings.ResolveType qualifiedName id symbol
                        let names=ResizeArray<string>([|fullName;qualifiedName|])
                        if not(isNull typeInfo) then
                            let owner=typeInfo["identity"]
                            names.Add(owner["fullName"].GetValue<string>())
                        if not(isNull memberInfo) then
                            let owner=memberInfo["declaringType"]
                            names.Add(owner["fullName"].GetValue<string>()+"."+memberInfo["name"].GetValue<string>())
                        if not(isNull callableInfo) then names.Add((callableInfo["identity"]["fullName"]).GetValue<string>())
                        let record={factsVersion=2;id=hash(contentHash+"|"+id+"|"+fullName);assemblyPath=file;assemblyName=assembly.SimpleName;fullName=fullName;name=symbol.DisplayName;kind=symbolKind symbol;signature=signature;xmlDocId=id;documentation=docs |> Map.tryFind id |> Option.defaultValue "";qualifiedNames=names |> Seq.distinct |> Seq.toArray;isExternal=true;typeInfo=typeInfo;memberInfo=memberInfo;callableInfo=callableInfo}
                        if isNull sink then results.Add record else sink.Add record
                        symbolCount <- symbolCount+1
                let rec walk parent (entity:FSharpEntity) =
                    if entity.Accessibility.IsPublic then
                        let logicalName = if parent="" then entity.DisplayName else parent+"."+entity.DisplayName
                        add logicalName entity
                        if not entity.IsFSharpAbbreviation then
                            for field in entity.FSharpFields do add (logicalName+"."+field.DisplayName) field
                            for case in entity.UnionCases do add (logicalName+"."+case.DisplayName) case
                            for memberValue in entity.MembersFunctionsAndValues do
                                if not memberValue.IsCompilerGenerated then add (logicalName+"."+memberValue.DisplayName) memberValue
                            for nested in entity.NestedEntities do walk logicalName nested
                for entity in assembly.Contents.Entities do
                    let parent = entity.Namespace |> Option.defaultValue ""
                    walk parent entity
            | _ -> ()
        let missing = assemblies |> Array.filter (imported.Contains >> not)
        if missing.Length>0 then failwith ("FCS did not import requested assemblies: "+String.concat ", " missing)
        for reference in typeDependencies.TypeReferences do bindings.ResolveType reference |> ignore
        for assembly in typeDependencies.AssemblyNames do bindings.ResolveAssembly assembly |> ignore
        for file in assemblies do bindings.IncludeAssembly file
        let closure,dependencyBindings=bindings.Files,bindings.Bindings
        // The actual dependency closure retains a producer binary if selected
        // as an exported/referenced library; provenance alone adds no live path.
        let binaryInputs=closure |> Array.distinct |> Array.map(fun file ->
            match before |> Map.tryFind file with Some stamp -> stamp | None -> Fingerprints.file file)
        let inputs=Array.append binaryInputs (xmlSelections |> Array.collect(fun selection -> selection.candidates)) |> Array.distinctBy(fun input -> input.path)
        if binaryInputs |> Array.exists(fun before -> not(File.Exists before.path) || (Fingerprints.file before.path).sha256<>before.sha256) then
            failwith "Assembly catalog inputs changed during extraction"
        MetadataXml.verify xmlSelections
        MetadataContract.verify producerInputs
        if isNull sink then Json.node {|symbols=results.ToArray();inputHashes=inputs;dependencyBindings=dependencyBindings;xmlSelections=xmlSelections;completeness="exact";schemaVersion=2;factsVersion=2;metadataCertificateVersion=1;producer=producer;scope="binary-declarations-only"|}
        else Json.node {|outputPath=sink.Complete(inputs,dependencyBindings);symbolCount=symbolCount;inputHashes=inputs;dependencyBindings=dependencyBindings;xmlSelections=xmlSelections;completeness="exact";schemaVersion=2;factsVersion=2;metadataCertificateVersion=1;producer=producer;scope="binary-declarations-only"|}

    let exportMetadata parameters = exportMetadataWithContext None parameters

    let private overlays = Dictionary<string,string>(Paths.comparer)
    let private checker = lazy (
        FSharpChecker.Create(projectCacheSize=2,keepAssemblyContents=false,keepAllBackgroundResolutions=false,keepAllBackgroundSymbolUses=false,
            documentSource=DocumentSource.Custom(fun path -> async {
                let file=Paths.canonical path
                match overlays.TryGetValue file with
                | true,text -> return Some(SourceText.ofString text)
                | _ when File.Exists file -> return Some(SourceText.ofString(File.ReadAllText file))
                | _ -> return None
            })))

    let private projectOptions (workspace:WorkspaceModel) =
        let byPath = workspace.projects |> Array.map (fun p -> p.path,p) |> Map.ofArray
        let made = Dictionary<string,FSharpProjectOptions>(Paths.comparer)
        let visiting = HashSet<string>(Paths.comparer)
        let rec build path =
            match made.TryGetValue path with
            | true,x -> x
            | _ ->
                if not (visiting.Add path) then failwith "Cyclic F# project references are unsupported"
                let project=byPath[path]
                // Command-line option construction preserves source arguments in
                // OtherOptions. Pass source paths once, explicitly and absolute.
                let arguments = project.compilerArgs |> Array.filter (fun arg ->
                    let value=arg.Trim('"')
                    value.StartsWith("-") || not (value.EndsWith(".fs",StringComparison.OrdinalIgnoreCase) || value.EndsWith(".fsi",StringComparison.OrdinalIgnoreCase) || value.EndsWith(".fsx",StringComparison.OrdinalIgnoreCase)))
                let initial = checker.Value.GetProjectOptionsFromCommandLineArgs(path,arguments)
                let references = project.references |> Array.choose (fun reference ->
                    match byPath |> Map.tryFind reference with
                    | Some target ->
                        // SDK builds may reference obj/.../ref/Library.dll even
                        // though TargetPath is bin/.../Library.dll. FCS matches
                        // source project references against the actual -r path.
                        let referencePath =
                            arguments |> Array.tryPick(fun arg ->
                                let candidate=if arg.StartsWith("-r:") then arg.Substring(3).Trim('"') elif arg.StartsWith("--reference:") then arg.Substring(12).Trim('"') else ""
                                if candidate<>"" && Path.GetFileName(candidate)=Path.GetFileName(target.outputPath) && (project.workspaceAssemblyReferences |> Array.exists(fun p -> Paths.comparer.Equals(p,Paths.absolute (Path.GetDirectoryName project.path) candidate))) then Some(Paths.absolute (Path.GetDirectoryName project.path) candidate) else None)
                            |> Option.defaultValue target.outputPath
                        Some(FSharpReferencedProject.FSharpReference(referencePath,build reference))
                    | None -> None)
                let options={initial with SourceFiles=project.files; ReferencedProjects=references}
                visiting.Remove path |> ignore
                made[path] <- options
                options
        workspace.projects |> Array.map (fun p -> p.path,build p.path) |> Map.ofArray

    let private sourceText path =
        match overlays.TryGetValue path with true,text -> text | _ -> File.ReadAllText path

    let exportSnapshot (parameters:JsonElement) =
        if (Json.array "documents" parameters).Length<>0 then
            failwith "semantic/export accepts clean disk snapshots only; save or discard dirty documents first"
        overlays.Clear()
        let workspace=Evaluation.workspace parameters
        checker.Value.InvalidateAll()
        let options=projectOptions workspace
        let stamp path = {path=Paths.canonical path;sha256=fileHash path |> fun s -> s.ToLowerInvariant()}
        let helperAssembly=typeof<FSharpChecker>.Assembly.Location
        let sourceStamps=workspace.projects |> Array.map(fun p -> p.path,p.files |> Array.map stamp) |> Map.ofArray
        let inputStamps=workspace.projects |> Array.map(fun p ->
            p.path, Array.concat [workspace.inputs;p.assemblyReferences;[|helperAssembly;typeof<ProjectModel>.Assembly.Location|]] |> Array.distinct |> Array.map Fingerprints.state) |> Map.ofArray
        let contextIds=workspace.projects |> Array.map(fun p ->
            let sources=sourceStamps[p.path] |> Array.map(fun s -> s.path+"="+s.sha256)
            let inputs=inputStamps[p.path] |> Array.map(fun s -> s.path+"="+s.sha256)
            p.path, hash(String.concat "\n" (Array.concat [[|p.path;p.targetFramework;p.configuration;JsonSerializer.Serialize(p.environmentCertificate,Json.options)|];p.compilerArgs;sources;inputs]))) |> Map.ofArray
        let owners=Dictionary<string,ResizeArray<ProjectModel>>(Paths.comparer)
        for p in workspace.projects do
            for file in p.files do
                match owners.TryGetValue file with
                | true,projects -> projects.Add p
                | _ -> owners[file] <- ResizeArray([|p|])
        let selection=Json.str "project" "" parameters
        let selected=if selection="" then workspace.projects else workspace.projects |> Array.filter(fun p -> Paths.comparer.Equals(p.path,Paths.absolute workspace.root selection))
        if selected.Length=0 then failwith "The selected export project is absent from the evaluated workspace"
        // Group identity by a source declaration anchor (including its evaluated
        // project context). Detect, rather than silently merge, anchor collisions.
        let identities=Dictionary<string,FSharpSymbol>()
        let equivalences=Dictionary<int,ResizeArray<FSharpSymbol*string>>()
        let parents=Dictionary<string,string>()
        let rec representative id =
            match parents.TryGetValue id with
            | true,parent when parent<>id ->
                let root=representative parent
                parents[id] <- root
                root
            | _ -> id
        let registerEquivalent (symbol:FSharpSymbol) id =
            // Effective equality erases abbreviations; different declared aliases
            // can have different reference sets even when both erase to string.
            let alias = match symbol with :? FSharpEntity as e -> e.IsFSharpAbbreviation | _ -> false
            if not alias then
                let bucketKey=symbol.GetEffectivelySameAsHash()
                let bucket=
                    match equivalences.TryGetValue bucketKey with
                    | true,bucket -> bucket
                    | _ -> let bucket=ResizeArray() in equivalences[bucketKey] <- bucket; bucket
                match bucket |> Seq.tryFind(fun (previous,_) -> previous.IsEffectivelySameAs symbol) with
                | Some(_,other) ->
                    let first,second=representative id,representative other
                    if first<>second then parents[max first second] <- min first second
                | None -> bucket.Add(symbol,id)
        let results=ResizeArray<ProjectSnapshot>()
        for project in selected do
            let occurrences=ResizeArray<SnapshotOccurrence>()
            let errors=ResizeArray<string>()
            let keyFor (symbol:FSharpSymbol) =
                let declaration=
                    match symbol with
                    | :? FSharpEntity as entity when entity.IsFSharpAbbreviation ->
                        attempt None (fun () -> symbol.DeclarationLocation) |> Option.orElseWith(fun () -> attempt None (fun () -> symbol.ImplementationLocation))
                    | _ -> attempt None (fun () -> symbol.ImplementationLocation) |> Option.orElseWith(fun () -> attempt None (fun () -> symbol.DeclarationLocation))
                let sourceDeclaration=declaration |> Option.filter(fun d -> owners.ContainsKey(Paths.canonical d.FileName))
                match sourceDeclaration with
                | Some d ->
                    let file=Paths.canonical d.FileName
                    let candidates=owners[file]
                    let owner=
                        if candidates |> Seq.exists(fun p -> p.path=project.path) then project
                        elif candidates.Count=1 then candidates[0]
                        else
                            let assemblyName=attempt "" (fun () -> symbol.Assembly.SimpleName)
                            candidates |> Seq.tryFind(fun p -> Path.GetFileNameWithoutExtension(p.outputPath)=assemblyName)
                            |> Option.defaultWith(fun () -> failwith $"Ambiguous linked-source ownership for {file}")
                    let location={path=file;range=range d}
                    "source|"+contextIds[owner.path]+"|"+file+"|"+string d.StartLine+":"+string d.StartColumn+"-"+string d.EndLine+":"+string d.EndColumn+"|"+symbolKind symbol+"|"+symbolXmlId symbol+"|"+symbolFullName symbol,box location
                | None ->
                    let assembly=attempt "" (fun () -> symbol.Assembly.QualifiedName)
                    let xmlId=symbolXmlId symbol
                    let name=symbolFullName symbol
                    if assembly="" && xmlId="" then failwith $"No stable external declaration identity for {name}"
                    "binary|"+assembly+"|"+(if xmlId="" then symbolKind symbol+"|"+name else xmlId),null
            for file in project.files do
                let _, answer=checker.Value.ParseAndCheckFileInProject(file,0,SourceText.ofString(File.ReadAllText file),options[project.path]) |> Async.RunSynchronously
                match answer with
                | FSharpCheckFileAnswer.Succeeded checkedFile ->
                    for diagnostic in checkedFile.Diagnostics do
                        if diagnostic.Severity=FSharpDiagnosticSeverity.Error then errors.Add diagnostic.Message
                    // FCS effective equality erases source abbreviations, but
                    // GetUsesOfSymbolInFile preserves the alias written at a
                    // constructor occurrence. Certify that relationship per
                    // occurrence instead of merging every underlying .ctor.
                    let aliases=Dictionary<string,FSharpSymbol>()
                    let rec collectAliases (entity:FSharpEntity) =
                        if entity.IsFSharpAbbreviation then
                            let key,declaration=keyFor entity
                            if not(isNull declaration) then aliases[hash key] <- entity
                        else
                            for nested in entity.NestedEntities do collectAliases nested
                    for entity in checkedFile.PartialAssemblySignature.Entities do collectAliases entity
                    let sourceAssemblies=HashSet<string>(workspace.projects |> Array.map(fun p -> Path.GetFileNameWithoutExtension p.outputPath),StringComparer.Ordinal)
                    for assembly in checkedFile.ProjectContext.GetReferencedAssemblies() do
                        if sourceAssemblies.Contains assembly.SimpleName then
                            for entity in assembly.Contents.Entities do collectAliases entity
                    let spanKey (usage:FSharpSymbolUse)=Paths.canonical usage.Range.FileName,range usage.Range
                    let claims=Dictionary<string*SourceRange,string>()
                    let supplemental=Dictionary<string*SourceRange,FSharpSymbolUse>()
                    for KeyValue(id,symbol) in aliases do
                        for usage in checkedFile.GetUsesOfSymbolInFile symbol do
                            let span=spanKey usage
                            match claims.TryGetValue span with
                            | true,other when other<>id -> errors.Add($"Multiple source aliases claim the same occurrence: {usage.Range}")
                            | _ -> claims[span] <- id; supplemental[span] <- usage
                    let allUses=checkedFile.GetAllUsesOfAllSymbolsInFile() |> Seq.toArray
                    for usage in allUses do supplemental.Remove(spanKey usage) |> ignore
                    for usage in Seq.append allUses supplemental.Values do
                        try
                            let key,declaration=keyFor usage.Symbol
                            match identities.TryGetValue key with
                            | true,previous when not(previous.IsEffectivelySameAs usage.Symbol) ->
                                errors.Add($"Non-equivalent compiler symbols share declaration identity: {usage.Symbol.DisplayName}")
                            | _ ->
                                identities[key] <- usage.Symbol
                                let id=hash key
                                let group=
                                    match claims.TryGetValue(spanKey usage) with
                                    | true,aliasId -> aliasId
                                    | _ -> registerEquivalent usage.Symbol id;id
                                // Keep the occurrence's definition target. An
                                // alias-spelled constructor can navigate to the
                                // underlying external ctor while its reference
                                // group is the source alias.
                                occurrences.Add {symbolId=group;path=Paths.canonical usage.Range.FileName;range=range usage.Range;isDefinition=usage.IsFromDefinition;declaration=declaration}
                        with ex -> errors.Add ex.Message
                | _ -> errors.Add($"FCS aborted checking {file}")
            let changed=Array.append sourceStamps[project.path] inputStamps[project.path] |> Array.exists(fun before -> (Fingerprints.state before.path).sha256<>before.sha256)
            if changed then errors.Add "Inputs changed while the semantic snapshot was being exported"
            results.Add {projectPath=project.path;contextId=contextIds[project.path];targetFramework=project.targetFramework;configuration=project.configuration;sourceHashes=sourceStamps[project.path];inputHashes=inputStamps[project.path];environmentCertificate=project.environmentCertificate;sourceMembership=project.sourceMembership;membershipReusable=project.membershipReusable;membershipDiagnostics=project.membershipDiagnostics;compilerArgs=project.compilerArgs;dependencies=project.references;completeness=(if errors.Count=0 then "exact" else "unavailable");diagnostics=errors |> Seq.distinct |> Seq.toArray;occurrences=(if errors.Count=0 then occurrences.ToArray() else [||])}
        // A dependency may have changed after its own project was processed.
        let allStamps=Seq.append (sourceStamps |> Map.values |> Seq.concat) (inputStamps |> Map.values |> Seq.concat)
        let stable=allStamps |> Seq.forall(fun before -> (Fingerprints.state before.path).sha256=before.sha256)
        let projects=results.ToArray() |> Array.map(fun p ->
            if stable then {p with occurrences=p.occurrences |> Array.map(fun o -> {o with symbolId=representative o.symbolId})}
            else {p with completeness="unavailable";diagnostics=Array.append p.diagnostics [|"Workspace inputs changed during export"|];occurrences=[||]})
        Json.node {|schemaVersion=1;factsVersion=2;kind="semantic-snapshot";root=workspace.root;workspaceProjects=workspace.projects |> Array.map(fun p -> p.path);projects=projects;completeness=(if stable && projects |> Array.forall(fun p -> p.completeness="exact") then "exact" else "unavailable");scope="evaluated-project-snapshot"|}

    let private atCursor (parameters:JsonElement) (workspace:WorkspaceModel) options =
        let path=Paths.absolute workspace.root (Json.str "path" "" parameters)
        let project = workspace.projects |> Array.tryFind (fun p -> p.files |> Array.exists (fun f -> Paths.comparer.Equals(f,path))) |> Option.defaultWith (fun () -> failwith "File is not in the evaluated project input set")
        let text=sourceText path
        let lines=text.Replace("\r\n","\n").Split('\n')
        let line=Json.integer "line" 0 parameters
        let character=Json.integer "character" 0 parameters
        if line<0 || line>=lines.Length || character<0 || character>lines[line].Length then failwith "Position is outside document"
        let lineText=lines[line]
        let token=Regex.Matches(lineText,"``[^`]+``|[\\p{L}_][\\p{L}\\p{N}_']*|[!%&*+\\-/<=>?@^|~:]+") |> Seq.cast<Match> |> Seq.tryFind(fun m -> character>=m.Index && character<m.Index+m.Length)
        let token=token |> Option.defaultWith(fun () -> failwith "No supported identifier at cursor")
        let column=token.Index+token.Length
        let partial=QuickParse.GetPartialLongNameEx(lineText,column-1)
        let names=partial.QualifyingIdents @ [partial.PartialIdent]
        let parseResult, answer = checker.Value.ParseAndCheckFileInProject(path,0,SourceText.ofString text,options |> Map.find project.path) |> Async.RunSynchronously
        let result=match answer with FSharpCheckFileAnswer.Succeeded x -> x | _ -> failwith "FCS aborted checking the document"
        let errors=result.Diagnostics |> Array.filter(fun diagnostic -> diagnostic.Severity=FSharpDiagnosticSeverity.Error)
        if errors.Length>0 then failwith ("Compiler errors prevent an exact cursor binding: "+(errors |> Array.map(fun d -> d.Message) |> String.concat "; "))
        let symbol=result.GetSymbolUseAtLocation(line+1,column,lineText,names) |> Option.defaultWith(fun () -> failwith "FCS could not resolve the cursor symbol")
        path, project, result, symbol

    let oracle methodName (parameters:JsonElement) =
        // Explicit experimental/oracle path only. Callers must never invoke this
        // as the normal native engine's hidden fallback.
        overlays.Clear()
        for document in Json.array "documents" parameters do
            let path=Json.str "path" "" document |> Paths.canonical
            overlays[path] <- Json.str "text" "" document
        let workspace=Evaluation.workspace parameters
        checker.Value.InvalidateAll()
        let options=projectOptions workspace
        let path, project, checkedFile, symbolUse=atCursor parameters workspace options
        let symbol=symbolUse.Symbol
        let sourceFiles=HashSet<string>(workspace.projects |> Array.collect (fun p -> p.files),Paths.comparer)
        let assemblyPath=attempt "" (fun () -> symbol.Assembly.FileName |> Option.defaultValue "")
        let declaration=attempt None (fun () -> symbol.DeclarationLocation)
        let internalSymbol =
            (declaration |> Option.exists(fun r -> (Path.IsPathRooted r.FileName && sourceFiles.Contains(Paths.canonical r.FileName)) || (File.Exists r.FileName && Paths.within workspace.root r.FileName))) ||
            (assemblyPath<>"" && workspace.projects |> Array.exists(fun p -> p.workspaceAssemblyReferences |> Array.exists(fun output -> Paths.comparer.Equals(output,Paths.canonical assemblyPath)))) ||
            (attempt "" (fun () -> symbol.Assembly.SimpleName) = Path.GetFileNameWithoutExtension project.outputPath)
        match methodName with
        | "navigation/hover" ->
            if internalSymbol then Json.node {|hover=(null:obj);completeness="exact"|}
            else
                let signature=symbolSignature symbolUse.DisplayContext symbol
                let doc=documentation assemblyPath symbol
                Json.node {|hover=box {|markdown="```fsharp\n"+signature+"\n```"+(if doc="" then "" else "\n\n"+doc);range=range symbolUse.Range;isExternal=true;originPath=assemblyPath|};completeness="exact"|}
        | "navigation/definition" ->
            match declaration with
            | Some r when sourceFiles.Contains(Paths.canonical r.FileName) || File.Exists r.FileName ->
                Json.node {|locations=[|{path=Paths.canonical r.FileName;range=range r}|];completeness="exact"|}
            | _ when not internalSymbol ->
                let signature=symbolSignature symbolUse.DisplayContext symbol
                let uri="fsharp-navigator://metadata/"+hash(assemblyPath+symbolFullName symbol)+".fsi"
                let text="// External declaration; source is unavailable.\n"+signature+"\n"
                let loc={path=uri;range={start={line=1;character=0};``end``={line=1;character=signature.Length}}}
                Json.node {|locations=[|loc|];documents=[|{|uri=uri;language="fsharp";text=text|}|];completeness="exact";targetKind="metadata-signature"|}
            | _ -> failwith "Internal symbol has no usable declaration location"
        | "navigation/references" ->
            let locations=ResizeArray<Location>()
            let errors=ResizeArray<string>()
            for p in workspace.projects do
                // Foreground checks expose symbol uses without retaining all
                // background project uses in the checker between requests.
                for file in p.files do
                    let _, answer = checker.Value.ParseAndCheckFileInProject(file,0,SourceText.ofString(sourceText file),options[p.path]) |> Async.RunSynchronously
                    match answer with
                    | FSharpCheckFileAnswer.Succeeded result ->
                        for error in result.Diagnostics do
                            if error.Severity=FSharpDiagnosticSeverity.Error then errors.Add(error.Message)
                        for usage in result.GetUsesOfSymbolInFile symbol do
                            locations.Add {path=Paths.canonical usage.Range.FileName;range=range usage.Range}
                    | _ -> errors.Add($"FCS aborted checking {file}")
            if errors.Count>0 then Json.node {|locations=([||]:Location array);completeness="unavailable";diagnostics=errors.ToArray()|}
            else
                let includeDeclaration = match Json.tryProp "includeDeclaration" parameters with Some x when x.ValueKind=JsonValueKind.False -> false | _ -> true
                let locs=locations |> Seq.distinct |> Seq.filter(fun l -> includeDeclaration || declaration |> Option.forall(fun d -> not(Paths.comparer.Equals(l.path,Paths.canonical d.FileName) && l.range=range d))) |> Seq.toArray
                Json.node {|locations=locs;completeness="exact"|}
        | _ -> failwith $"Unknown oracle method: {methodName}"
