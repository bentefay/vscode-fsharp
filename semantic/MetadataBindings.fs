namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Reflection
open System.Reflection.Metadata
open System.Reflection.PortableExecutable
open System.Collections.Generic

type private AssemblyView = {
    identity:AssemblyName; facade:bool; references:AssemblyName array
    definitions:string array; typeReferences:(AssemblyName*string) array
    forwards:(string*AssemblyName) array
}

/// Compiler-selected bindings, including demand-directed pure-facade forwarding.
/// Ordinary assemblies retain their conservative complete reference closure.
type MetadataBindings(selected:(string*string) array, ?declarationInventoryOnly:bool) =
    let inventoryOnly=defaultArg declarationInventoryOnly false
    let byName=Dictionary<string,string>(StringComparer.OrdinalIgnoreCase)
    let views=Dictionary<string,AssemblyView>(Paths.comparer)
    let files=HashSet<string>(Paths.comparer)
    let checkedAssemblies=HashSet<string>(Paths.comparer)
    let bindings=ResizeArray<obj>()
    let bindingKeys=HashSet<string*string*string>()
    do
        for name,path in selected do
            match byName.TryGetValue name with
            | true,previous when not(Paths.comparer.Equals(previous,path)) ->
                if (Fingerprints.file previous).sha256<>(Fingerprints.file path).sha256 then failwith $"Ambiguous compiler-selected assembly: {name}"
            | _ -> byName[name] <- path
    let key (name:string)=name.Replace('+','.')
    let view file =
        match views.TryGetValue file with
        | true,value -> value
        | _ ->
            use stream=File.OpenRead file
            use pe=new PEReader(stream)
            if not pe.HasMetadata then failwith $"Assembly has no managed metadata: {file}"
            let reader=pe.GetMetadataReader()
            if reader.AssemblyFiles.Count<>0 then failwith $"Multi-module assembly catalogs are unsupported: {file}"
            let assemblyReference handle =
                let reference=reader.GetAssemblyReference handle
                let name=AssemblyName()
                name.Name <- reader.GetString reference.Name
                name.Version <- reference.Version
                name.CultureName <- if reference.Culture.IsNil then "" else reader.GetString reference.Culture
                let token=reader.GetBlobBytes reference.PublicKeyOrToken
                if (reference.Flags &&& AssemblyFlags.PublicKey)<>enum<AssemblyFlags> 0 then name.SetPublicKey token else name.SetPublicKeyToken token
                name
            let qualified (ns:StringHandle) (name:StringHandle) =
                let ns=reader.GetString ns
                if ns="" then reader.GetString name else ns+"."+reader.GetString name
            let rec typeReference (handle:TypeReferenceHandle) =
                let value=reader.GetTypeReference handle
                match value.ResolutionScope.Kind with
                | HandleKind.AssemblyReference -> Some(assemblyReference(AssemblyReferenceHandle.op_Explicit value.ResolutionScope),qualified value.Namespace value.Name)
                | HandleKind.TypeReference -> typeReference(TypeReferenceHandle.op_Explicit value.ResolutionScope) |> Option.map(fun (assembly,parent)->assembly,parent+"+"+reader.GetString value.Name)
                | _ -> None
            let rec forwarded (handle:ExportedTypeHandle) =
                let value=reader.GetExportedType handle
                match value.Implementation.Kind with
                | HandleKind.AssemblyReference when value.IsForwarder -> Some(qualified value.Namespace value.Name,assemblyReference(AssemblyReferenceHandle.op_Explicit value.Implementation))
                | HandleKind.ExportedType -> forwarded(ExportedTypeHandle.op_Explicit value.Implementation) |> Option.map(fun (parent,assembly)->parent+"+"+reader.GetString value.Name,assembly)
                | _ -> None
            let rec defined (handle:TypeDefinitionHandle) =
                let value=reader.GetTypeDefinition handle
                let parent=value.GetDeclaringType()
                if parent.IsNil then qualified value.Namespace value.Name else defined parent+"+"+reader.GetString value.Name
            let definitions=reader.TypeDefinitions |> Seq.map defined |> Seq.filter((<>) "<Module>") |> Seq.toArray
            let hasSignature=reader.ManifestResources |> Seq.exists(fun h -> (reader.GetString(reader.GetManifestResource(h).Name)).StartsWith("FSharpSignature",StringComparison.Ordinal))
            let forwards=reader.ExportedTypes |> Seq.choose forwarded |> Seq.toArray
            let value={identity=AssemblyName.GetAssemblyName file;facade=definitions.Length=0 && not hasSignature && forwards.Length=reader.ExportedTypes.Count;references=reader.AssemblyReferences |> Seq.map assemblyReference |> Seq.toArray;definitions=definitions;typeReferences=reader.TypeReferences |> Seq.choose typeReference |> Seq.toArray;forwards=forwards}
            views[file] <- value
            value
    let selectedPath name=match byName.TryGetValue name with true,path->path | _->failwith $"Unresolved metadata assembly: {name}"
    let resolveReference fromPath (requested:AssemblyName) =
        let path=selectedPath requested.Name
        let actual=(view path).identity
        let culture (name:AssemblyName)=name.CultureName |> Option.ofObj |> Option.defaultValue ""
        if culture requested<>culture actual || requested.GetPublicKeyToken()<>actual.GetPublicKeyToken() then failwith $"Metadata dependency identity mismatch: {requested.FullName} -> {actual.FullName}"
        if bindingKeys.Add(fromPath,requested.FullName,path) then bindings.Add(box {|fromPath=fromPath;requestedIdentity=requested.FullName;resolvedPath=path;resolvedIdentity=actual.FullName;resolution="compiler-selected-ccu"|})
        path
    let rec visitAssembly depth file =
        if depth>128 then failwith "Metadata dependency depth exceeded"
        files.Add file |> ignore
        if checkedAssemblies.Add file then
            let info=view file
            if not inventoryOnly && not info.facade then
                for reference in info.references do
                    let target=resolveReference file reference
                    visitAssembly (depth+1) target
                    for requested,typeName in info.typeReferences do
                        if requested.Name=reference.Name then resolveType (depth+1) target typeName |> ignore
    and resolveType depth file name =
        if depth>128 then failwith "Type-forwarding dependency depth exceeded"
        visitAssembly (depth+1) file
        resolveForwarded depth (HashSet<string*string>()) file name
    and resolveForwarded depth (active:HashSet<string*string>) file name =
        if depth>128 || not(active.Add(file,key name)) then failwith $"Cyclic type forwarder: {name}"
        files.Add file |> ignore
        let info=view file
        let matches=info.forwards |> Array.filter(fun (candidate,_)->key candidate=key name)
        let definitions=info.definitions |> Array.filter(fun candidate -> key candidate=key name)
        if matches.Length+definitions.Length>1 then failwith $"Ambiguous nested or namespace metadata type: {name} in {file}"
        elif matches.Length=1 then
            let target=resolveReference file (snd matches[0])
            visitAssembly (depth+1) target
            resolveForwarded (depth+1) active target (fst matches[0])
        elif definitions.Length<>1 then failwith $"Demanded type has no definition or forwarder: {name} in {file}"
        else file,info.identity.Name
    member _.ResolveType(assemblyName,typeName)=resolveType 0 (selectedPath assemblyName) typeName
    member _.ResolveAssembly(assemblyName)=
        let file=selectedPath assemblyName
        visitAssembly 0 file
        file,(view file).identity.Name
    member _.IncludeAssembly(file)=visitAssembly 0 file
    member _.Files=files |> Seq.toArray
    member _.Bindings=bindings.ToArray()
