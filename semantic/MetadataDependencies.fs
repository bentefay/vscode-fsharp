namespace FSharpNavigator.Semantic

open System.Collections.Generic
open FSharp.Compiler.Symbols

/// PE metadata omits erased F# dependencies (for example units of measure).
/// Track assemblies occurring in the exported FCS type and constraint graph as
/// additional dependency roots. A failed traversal rejects certification.
type MetadataDependencies() =
    let assemblies=HashSet<string>()
    let namedTypes=HashSet<string*string>()
    let abbreviations=HashSet<FSharpType>()
    let entities=HashSet<FSharpEntity>()
    let parameters=HashSet<FSharpGenericParameter>()
    let addAssembly (symbol:FSharpSymbol) = assemblies.Add symbol.Assembly.SimpleName |> ignore
    let rec visitType depth (ty:FSharpType) =
        if depth>128 then failwith "Metadata type graph exceeded dependency traversal depth"
        // FSharpType.Equals is modulo abbreviations: using it to deduplicate
        // before visiting original arguments can erase measure dependencies.
        if ty.IsUnresolved then failwith "Unresolved metadata type dependency"
        if ty.HasTypeDefinition then
            let entity=ty.TypeDefinition
            addAssembly entity
            if not entity.IsFSharp then entity.TryFullName |> Option.iter(fun name -> namedTypes.Add(entity.Assembly.SimpleName,name) |> ignore)
            // FSharpType.AbbreviatedType may collapse multiple aliases. The
            // nominal entity accessor preserves the next link and its CCU.
            if entity.IsFSharpAbbreviation && entities.Add entity then
                visitType (depth+1) entity.AbbreviatedType
        if ty.IsGenericParameter then visitParameter (depth+1) ty.GenericParameter
        else for argument in ty.GenericArguments do visitType (depth+1) argument
        if ty.IsAbbreviation && abbreviations.Add ty then visitType (depth+1) ty.AbbreviatedType
    and visitParameter depth (parameter:FSharpGenericParameter) =
        if parameters.Add parameter then
            for constraintInfo in parameter.Constraints do
                if constraintInfo.IsCoercesToConstraint then visitType depth constraintInfo.CoercesToTarget
                if constraintInfo.IsDefaultsToConstraint then visitType depth constraintInfo.DefaultsToConstraintData.DefaultsToTarget
                if constraintInfo.IsSimpleChoiceConstraint then for ty in constraintInfo.SimpleChoices do visitType depth ty
                if constraintInfo.IsEnumConstraint then visitType depth constraintInfo.EnumConstraintTarget
                if constraintInfo.IsDelegateConstraint then
                    visitType depth constraintInfo.DelegateConstraintData.DelegateTupledArgumentType
                    visitType depth constraintInfo.DelegateConstraintData.DelegateReturnType
                if constraintInfo.IsMemberConstraint then
                    let memberInfo=constraintInfo.MemberConstraintData
                    for ty in memberInfo.MemberSources do visitType depth ty
                    for ty in memberInfo.MemberArgumentTypes do visitType depth ty
                    visitType depth memberInfo.MemberReturnType
    member _.Add(symbol:FSharpSymbol) =
        addAssembly symbol
        match symbol with
        | :? FSharpEntity as entity ->
            if entity.IsProvided then failwith "Dynamic type-provider metadata dependencies are not certified"
            for parameter in entity.GenericParameters do visitParameter 0 parameter
            for ty in entity.DeclaredInterfaces do visitType 0 ty
            entity.BaseType |> Option.iter(visitType 0)
            if entity.IsFSharpAbbreviation then visitType 0 entity.AbbreviatedType
            if entity.IsDelegate && entity.IsFSharp then
                let signature=entity.FSharpDelegateSignature
                for _,ty in signature.DelegateArguments do visitType 0 ty
                visitType 0 signature.DelegateReturnType
        | :? FSharpMemberOrFunctionOrValue as value ->
            visitType 0 value.FullType
            for parameter in value.GenericParameters do visitParameter 0 parameter
        | :? FSharpField as field -> visitType 0 field.FieldType
        | :? FSharpUnionCase as unionCase ->
            visitType 0 unionCase.ReturnType
            for field in unionCase.Fields do visitType 0 field.FieldType
        | _ -> ()
    member _.AssemblyNames = assemblies |> Seq.toArray
    member _.TypeReferences = namedTypes |> Seq.toArray
