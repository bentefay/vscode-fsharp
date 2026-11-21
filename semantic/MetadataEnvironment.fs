namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text.Json
open System.Text.Json.Nodes
open System.Collections.Generic
open System.Reflection.Metadata
open System.Reflection.PortableExecutable
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.Symbols
open FSharp.Compiler.Text

/// Complete type-name inventory and conservative requested-pattern inventory.
/// Includes unopened/internal declarations; callers still prove source scope.
module MetadataEnvironment =
    // FCS imported AssemblySignature.Attributes is not an assembly-wide
    // attribute certificate. Read the real metadata to include implicit opens.
    let private assemblyOpens (bindings:MetadataBindings) (file:string) =
        use stream=File.OpenRead file
        use pe=new PEReader(stream)
        let reader=pe.GetMetadataReader()
        let assembly=reader.GetAssemblyDefinition()
        let here=reader.GetString assembly.Name
        let qualify (ns:StringHandle) (name:StringHandle) = let ns=reader.GetString ns in if ns="" then reader.GetString name else ns+"."+reader.GetString name
        let attributeOwner (attribute:CustomAttribute) =
            match attribute.Constructor.Kind with
            | HandleKind.MemberReference ->
                let ctor=reader.GetMemberReference(MemberReferenceHandle.op_Explicit attribute.Constructor)
                match ctor.Parent.Kind with
                | HandleKind.TypeReference ->
                    let typ=reader.GetTypeReference(TypeReferenceHandle.op_Explicit ctor.Parent)
                    let name=qualify typ.Namespace typ.Name
                    let owner=if typ.ResolutionScope.Kind=HandleKind.AssemblyReference then reader.GetString(reader.GetAssemblyReference(AssemblyReferenceHandle.op_Explicit typ.ResolutionScope).Name) else ""
                    Some(owner,name,ctor.Signature)
                | _ -> None
            | HandleKind.MethodDefinition ->
                let ctor=reader.GetMethodDefinition(MethodDefinitionHandle.op_Explicit attribute.Constructor)
                let typ=reader.GetTypeDefinition(ctor.GetDeclaringType())
                Some(here,qualify typ.Namespace typ.Name,ctor.Signature)
            | _ -> None
        [|for handle in assembly.GetCustomAttributes() do
            let attribute=reader.GetCustomAttribute handle
            match attributeOwner attribute with
            | Some(owner,"Microsoft.FSharp.Core.AutoOpenAttribute",signature) ->
                let actual,_=bindings.ResolveType(owner,"Microsoft.FSharp.Core.AutoOpenAttribute")
                let core,_=bindings.ResolveAssembly "FSharp.Core"
                if not(Paths.comparer.Equals(actual,core)) then failwith "AutoOpen attribute identity is not selected FSharp.Core"
                let mutable ctor=reader.GetBlobReader signature
                let header=ctor.ReadSignatureHeader()
                if header.IsGeneric then failwith "Generic AutoOpen attribute constructor unsupported"
                let parameters=ctor.ReadCompressedInteger()
                if ctor.ReadSignatureTypeCode()<>SignatureTypeCode.Void || parameters<0 || parameters>1 then failwith "Unsupported AutoOpen attribute constructor"
                if parameters=1 && ctor.ReadSignatureTypeCode()<>SignatureTypeCode.String then failwith "Unsupported AutoOpen attribute path argument"
                if ctor.RemainingBytes<>0 then failwith "Unsupported AutoOpen attribute signature suffix"
                let mutable blob=reader.GetBlobReader attribute.Value
                if blob.ReadUInt16()<>1us then failwith "Invalid AutoOpen attribute blob"
                let path=if parameters=0 then "" else blob.ReadSerializedString()
                if isNull path || blob.ReadUInt16()<>0us || blob.RemainingBytes<>0 then failwith "Unsupported AutoOpen attribute arguments"
                // Empty paths are a real compiler input; retain them rather
                // than inventing a target or certifying absence.
                yield {|assemblyIdentity={|assemblyPath=file;assemblyName=here;fullName=System.Reflection.AssemblyName.GetAssemblyName(file).FullName|};modulePath=path|}
            | _ -> ()|]
    let exportWithContext (shared:MetadataImportContext option) (parameters:JsonElement) =
        let references=Json.array "referencePaths" parameters |> Array.map(fun x -> Paths.canonical(x.GetString())) |> Array.distinct
        if references.Length=0 then failwith "metadata/environment requires effective referencePaths"
        for file in references do if not(File.Exists file) then failwith $"Missing environment reference: {file}"
        let owned=Json.array "workspaceAssemblyPaths" parameters |> Array.map(fun x -> Paths.canonical(x.GetString())) |> fun xs -> HashSet<string>(xs,Paths.comparer)
        let requested=Json.array "names" parameters |> Array.map(fun x -> x.GetString())
        let names=HashSet<string>((if requested.Length=0 then [|"Some";"None";"ValueSome";"ValueNone";"option";"voption";"int";"string"|] else requested),StringComparer.Ordinal)
        if names.Count>64 || names |> Seq.exists(fun n -> String.IsNullOrEmpty n || n.Length>128) then failwith "Requested-name limits exceeded"
        let context=shared |> Option.defaultWith(fun()->MetadataContext.create references)
        MetadataContext.require context references
        let producer,producerInputs=context.producer.DeepClone(),context.producerInputs
        let before,loaded,canonical,selected=context.before,context.loaded,context.canonical,context.selected
        // This certificate inventories declarations, not method bodies or a
        // rendered signature catalog. Every selected assembly is hashed below;
        // nominal owners still require a real TypeDef/forwarder proof. Private
        // implementation AssemblyRefs absent from the compiler reference set
        // cannot affect the names exported here and are not loaded speculatively.
        let bindings=MetadataBindings(selected,declarationInventoryOnly=true)
        let types=Dictionary<string,ResizeArray<obj>>(StringComparer.Ordinal)
        let patterns=Dictionary<string,ResizeArray<obj>>(StringComparer.Ordinal)
        let typeNames=Dictionary<string,ResizeArray<obj>>(StringComparer.Ordinal)
        let typeNameKeys=HashSet<string*string*string*int>()
        let scopeNames=Dictionary<string,ResizeArray<obj>>(StringComparer.Ordinal)
        let termNames=Dictionary<string,ResizeArray<obj>>(StringComparer.Ordinal)
        let scopeKeys=HashSet<string*string*string*string>()
        let termKeys=HashSet<string*string*string*string*string>()
        let assemblyAutoOpens=ResizeArray<obj>()
        let add (map:Dictionary<string,ResizeArray<obj>>) name value =
            if String.IsNullOrEmpty name then failwith "Empty declaration name in import environment"
            if not(map.ContainsKey name) then map[name] <- ResizeArray()
            map[name].Add value
        let rec logicalName (entity:FSharpEntity) =
            match entity.DeclaringEntity with
            | Some parent -> logicalName parent+"."+entity.DisplayName
            | None ->
                let access=entity.Namespace |> Option.defaultValue entity.AccessPath
                if access="" || access="global" then entity.DisplayName else access+"."+entity.DisplayName
        let attributeFlag (entity:FSharpEntity) suffix =
            entity.Attributes |> Seq.exists(fun attribute ->
                if attribute.IsUnresolved then failwith "Unresolved declaration attribute in import environment"
                let typ=attribute.AttributeType
                if typ.FullName<>"Microsoft.FSharp.Core."+suffix then false else
                let actual,_=bindings.ResolveType(typ.Assembly.SimpleName,typ.FullName)
                let core,_=bindings.ResolveAssembly "FSharp.Core"
                if not(Paths.comparer.Equals(actual,core)) then failwith "Scope attribute identity is not selected FSharp.Core"
                true)
        let spellings (display:string) (logical:string) (compiled:string) =
            let trimmed=display.Trim()
            let operator=if trimmed.StartsWith("(") && trimmed.EndsWith(")") then trimmed.Substring(1,trimmed.Length-2).Trim() else trimmed
            [|display;logical;compiled;operator|] |> Array.filter(String.IsNullOrEmpty >> not) |> Array.distinct
        for name in names do types[name] <- ResizeArray();patterns[name] <- ResizeArray()
        let identity path assembly name={|assemblyPath=path;assemblyName=assembly;fullName=name|}
        let fullName (s:FSharpSymbol)=s.FullName
        let mutable visited=0
        let mutable corePath=""
        for assembly in loaded do
            let file=assembly.FileName |> Option.map canonical |> Option.defaultWith(fun()->failwith "Unproven assembly location")
            if assembly.SimpleName="FSharp.Core" then corePath <- file
            if assembly.IsProviderGenerated then failwith "Generated-provider environment unsupported"
            if not(owned.Contains file) then
                for row in assemblyOpens bindings file do assemblyAutoOpens.Add(box row)
                let rec walk (entity:FSharpEntity) =
                    visited <- visited+1
                    if visited>1000000 then failwith "Declaration inventory limit exceeded"
                    if entity.IsProvided || entity.IsUnresolved then failwith "Provided or unresolved environment declaration"
                    let name=entity.DisplayName
                    // Imported FCS signatures usually flatten namespace nodes.
                    // Every namespace prefix is nevertheless an exported scope.
                    let namespacePath=entity.Namespace |> Option.defaultValue ""
                    if namespacePath<>"" then
                        let parts=namespacePath.Split('.')
                        for n in 0..parts.Length-1 do
                            let path=String.Join(".",parts,0,n+1)
                            let parent=if n=0 then "" else String.Join(".",parts,0,n)
                            if scopeKeys.Add(parts[n],file,path,"namespace") then
                                add scopeNames parts[n] (box {|kind="namespace";identity=identity file assembly.SimpleName path;sourceFullName=path;enclosingScope=parent;autoOpen=false;qualifiedOnly=false|})
                    let sourceScope=logicalName entity
                    let enclosingScope=entity.DeclaringEntity |> Option.map logicalName |> Option.defaultValue (if entity.AccessPath="global" then "" else entity.AccessPath)
                    let qualifiedOnly=attributeFlag entity "RequireQualifiedAccessAttribute"
                    let autoOpen=attributeFlag entity "AutoOpenAttribute"
                    if entity.IsNamespace || entity.IsFSharpModule then
                        let physical=entity.TryFullName |> Option.defaultValue entity.FullName
                        let path,assemblyName=if entity.IsNamespace then file,assembly.SimpleName else bindings.ResolveType(entity.Assembly.SimpleName,physical)
                        if scopeKeys.Add(name,path,physical,(if entity.IsNamespace then "namespace" else "module")) then
                            add scopeNames name (box {|kind=(if entity.IsNamespace then "namespace" else "module");identity=identity path assemblyName physical;sourceFullName=sourceScope;enclosingScope=enclosingScope;autoOpen=autoOpen;qualifiedOnly=qualifiedOnly|})
                    let addTerm kind display logical compiled scope enclosing staticValue xmlDoc =
                        let physical=entity.TryFullName |> Option.defaultValue entity.FullName
                        let path,assemblyName=if entity.IsNamespace || entity.IsFSharpAbbreviation then bindings.ResolveAssembly entity.Assembly.SimpleName else bindings.ResolveType(entity.Assembly.SimpleName,physical)
                        for spelling in spellings display logical compiled do
                            if termKeys.Add(spelling,path,physical,compiled,xmlDoc) then
                                add termNames spelling (box {|kind=kind;identity=identity path assemblyName (physical+"."+compiled);sourceFullName=scope+"."+display;declaringScope=scope;enclosingScope=enclosing;displayName=display;logicalName=logical;compiledName=compiled;isStatic=staticValue;qualifiedOnly=qualifiedOnly;xmlDocId=xmlDoc|})
                    if not(entity.IsNamespace || entity.IsFSharpModule) && entity.CompiledName<>"<Module>" then
                        let sourceName=fullName entity
                        let physicalName=entity.TryFullName |> Option.defaultValue sourceName
                        let erased=entity.IsFSharpAbbreviation || entity.IsMeasure || (entity.GenericParameters |> Seq.exists(fun p -> p.IsMeasure)) || entity.TryFullName.IsNone
                        let ownerPath,ownerName=if erased then bindings.ResolveAssembly entity.Assembly.SimpleName else bindings.ResolveType(entity.Assembly.SimpleName,physicalName)
                        if typeNameKeys.Add(name,ownerPath,physicalName,entity.GenericParameters.Count) then
                            if not(typeNames.ContainsKey name) then typeNames[name] <- ResizeArray()
                            typeNames[name].Add(box {|identity=identity ownerPath ownerName physicalName;sourceFullName=sourceName;genericArity=entity.GenericParameters.Count;isAbbreviation=entity.IsFSharpAbbreviation;isNominal=not erased|})
                        if names.Contains name then
                            let info,_=MetadataTypes.facts bindings.ResolveType bindings.ResolveAssembly sourceName entity
                            let canonicalType=if isNull info then Json.node {|kind="unsupported";reason="unsupported-declaration"|} else info["type"].DeepClone()
                            let alias=entity.IsFSharpAbbreviation && assembly.SimpleName="FSharp.Core" && sourceName="Microsoft.FSharp.Core."+name
                            types[name].Add(box {|identity=identity file assembly.SimpleName sourceName;canonicalType=canonicalType;genericArity=entity.GenericParameters.Count;isBuiltin=alias;xmlDocId=entity.XmlDocSig|})
                    if not entity.IsFSharpAbbreviation then
                        for case in entity.UnionCases do
                            if case.IsUnresolved then failwith "Unresolved union case inventory"
                            addTerm "unionCase" case.Name case.Name case.CompiledName sourceScope enclosingScope true case.XmlDocSig
                            if names.Contains case.Name then
                                let owner=case.DeclaringEntity
                                let parameters=owner.GenericParameters |> Seq.toArray
                                let ownerType=MetadataTypes.typeExpression bindings.ResolveType parameters (owner.AsType())
                                let fields=case.Fields |> Seq.map(fun f -> MetadataTypes.typeExpression bindings.ResolveType parameters f.FieldType) |> Seq.toArray
                                let builtin=assembly.SimpleName="FSharp.Core" && ((owner.FullName="Microsoft.FSharp.Core.FSharpOption`1" && (case.Name="Some" || case.Name="None")) || (owner.FullName="Microsoft.FSharp.Core.FSharpValueOption`1" && (case.Name="ValueSome" || case.Name="ValueNone")))
                                patterns[case.Name].Add(box {|identity=identity file assembly.SimpleName (fullName case);kind="unionCase";declaringType=ownerType;payloadType=(if fields.Length=1 then fields[0] else null);payloadCount=fields.Length;isBuiltin=builtin;xmlDocId=case.XmlDocSig|})
                        for case in entity.ActivePatternCases do
                            addTerm "activePattern" case.Name case.Name case.Name sourceScope enclosingScope true case.XmlDocSig
                            if names.Contains case.Name then patterns[case.Name].Add(box {|identity=identity file assembly.SimpleName ((fullName entity)+"."+case.Name);kind="activePattern";declaringType=null;payloadType=null;payloadCount= -1;isBuiltin=false;xmlDocId=case.XmlDocSig|})
                        for field in entity.FSharpFields do
                            if field.IsLiteral then addTerm (if entity.IsEnum then "enumCase" else "literal") field.Name field.Name field.Name sourceScope enclosingScope true field.XmlDocSig
                            if field.IsLiteral && names.Contains field.Name then patterns[field.Name].Add(box {|identity=identity file assembly.SimpleName (fullName field);kind="literal";declaringType=null;payloadType=null;payloadCount=0;isBuiltin=false;xmlDocId=field.XmlDocSig|})
                        for value in entity.MembersFunctionsAndValues do
                            if entity.IsFSharpModule && not value.IsMember then
                                addTerm (if value.LiteralValue.IsSome then "literal" else "value") value.DisplayName value.LogicalName value.CompiledName sourceScope enclosingScope (not value.IsInstanceMember) value.XmlDocSig
                            if value.LiteralValue.IsSome && names.Contains value.DisplayName then patterns[value.DisplayName].Add(box {|identity=identity file assembly.SimpleName (fullName value);kind="literal";declaringType=null;payloadType=null;payloadCount=0;isBuiltin=false;xmlDocId=value.XmlDocSig|})
                        for nested in entity.NestedEntities do walk nested
                for entity in assembly.Contents.Entities do walk entity
        if corePath="" || owned.Contains corePath then failwith "Selected external FSharp.Core missing"
        let external=selected |> Array.map snd |> Array.filter(owned.Contains >> not)
        // Selected/fact-dependency roles win even when a selected assembly is
        // also the exporter or FCS. Do not subtract files by basename.
        let inputs=Array.concat [external;references |> Array.filter(owned.Contains >> not);bindings.Files] |> Array.distinct |> Array.map Fingerprints.file
        for input in inputs do
            match before.TryFind input.path with
            | Some old when old.sha256=input.sha256 -> ()
            | _ -> failwith $"Uncaptured or changed environment input: {input.path}"
        let referenceBindings=selected |> Array.map(fun (name,file)->{|assemblyName=name;path=file;identity=System.Reflection.AssemblyName.GetAssemblyName(file).FullName;isWorkspace=owned.Contains file|})
        MetadataContract.verify producerInputs
        let artifact={|schemaVersion=2;factsVersion=2;metadataCertificateVersion=1;producer=producer;kind="builtin-environment";certificateScope="selected-reference-declaration-inventory";completeness="exact";inventory="all-visible-and-internal-external-declarations";typeNameInventory="all-external-type-declarations";scopeNameInventory="all-external-namespace-and-module-declarations";termNameInventory="all-external-expression-bindings";scopeNames=scopeNames;termNames=termNames;assemblyAutoOpens=assemblyAutoOpens;inputHashes=inputs;referenceBindings=referenceBindings;workspaceAssemblies=owned |> Seq.toArray;types=types;typeNames=typeNames;patterns=patterns;coreAssemblyPath=corePath;visitedEntities=visited|}
        let outputPath=Json.str "outputPath" "" parameters
        if outputPath="" then Json.node artifact else
            let outputPath=Paths.canonical outputPath
            if File.Exists outputPath || Directory.Exists outputPath then failwith "Environment output already exists"
            Directory.CreateDirectory(Path.GetDirectoryName outputPath) |> ignore
            let temporary=outputPath+"."+Guid.NewGuid().ToString("N")+".tmp"
            try
                use stream=new FileStream(temporary,FileMode.CreateNew,FileAccess.Write,FileShare.None)
                JsonSerializer.Serialize(stream,artifact,Json.options)
                stream.Flush(true)
                stream.Dispose()
                MetadataContext.verify context
                File.Move(temporary,outputPath,false)
                Json.node {|outputPath=outputPath;schemaVersion=2;factsVersion=2;metadataCertificateVersion=1;producer=producer.DeepClone();kind="builtin-environment";completeness="exact";typeNameInventory="all-external-type-declarations";scopeNameInventory="all-external-namespace-and-module-declarations";termNameInventory="all-external-expression-bindings";inputHashes=inputs;referenceBindings=referenceBindings;workspaceAssemblies=owned |> Seq.toArray;visitedEntities=visited|}
            finally
                if File.Exists temporary then File.Delete temporary
    let export parameters = exportWithContext None parameters
