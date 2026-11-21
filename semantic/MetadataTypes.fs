namespace FSharpNavigator.Semantic

open System.Text.Json.Nodes
open FSharp.Compiler.Symbols

/// Portable declaration facts, never a serialized use-site compiler state.
module MetadataTypes =
    let private unsupported reason=Json.node {|kind="unsupported";reason=reason|}
    let private identity resolve (entity:FSharpEntity)=
        let path,name=resolve(entity.Assembly.SimpleName,entity.FullName)
        Json.node {|assemblyPath=path;assemblyName=name;fullName=entity.FullName|}
    let rec private encode resolve (typeParameters:FSharpGenericParameter array) (methodParameters:FSharpGenericParameter array) depth (ty:FSharpType):JsonNode =
        if depth>32 then unsupported "type-depth-limit"
        elif ty.IsUnresolved then unsupported "unresolved-type"
        elif ty.IsMeasureType then unsupported "measure-type"
        elif ty.IsGenericParameter then
            let parameter=ty.GenericParameter
            if parameter.IsMeasure then unsupported "measure-parameter" else
            match typeParameters |> Array.tryFindIndex(fun p -> p.Equals parameter) with
            | Some index -> Json.node {|kind="typeParameter";index=index|}
            | None ->
                match methodParameters |> Array.tryFindIndex(fun p -> p.Equals parameter) with
                | Some index -> Json.node {|kind="methodParameter";index=index|}
                | None -> unsupported "unbound-generic-parameter"
        elif ty.GenericArguments |> Seq.exists(fun t -> t.IsMeasureType || (t.IsGenericParameter && t.GenericParameter.IsMeasure)) then unsupported "measured-type"
        elif ty.IsAbbreviation then encode resolve typeParameters methodParameters (depth+1) ty.AbbreviatedType
        elif ty.IsArrayType then
            let arguments=ty.GenericArguments |> Seq.toArray
            if arguments.Length<>1 || not ty.HasTypeDefinition then unsupported "invalid-array-type" else
            Json.node {|kind="array";rank=ty.TypeDefinition.ArrayRank;element=encode resolve typeParameters methodParameters (depth+1) arguments[0]|}
        elif ty.IsFunctionType then
            let arguments=ty.GenericArguments |> Seq.toArray
            if arguments.Length<>2 then unsupported "invalid-function-type" else
            Json.node {|kind="function";domain=encode resolve typeParameters methodParameters (depth+1) arguments[0];range=encode resolve typeParameters methodParameters (depth+1) arguments[1]|}
        elif ty.IsTupleType || ty.IsNativePointerType || (ty.HasTypeDefinition && ty.TypeDefinition.IsByRef) then unsupported "tuple-byref-or-native-pointer"
        elif ty.HasTypeDefinition && not ty.IsArrayType then
            let entity=ty.TypeDefinition
            if entity.IsProvided || entity.IsUnresolved || entity.TryFullName.IsNone then unsupported "provided-or-unresolved-type"
            else Json.node {|kind="named";identity=identity resolve entity;arguments=ty.GenericArguments |> Seq.map(encode resolve typeParameters methodParameters (depth+1)) |> Seq.toArray|}
        else unsupported "non-nominal-type"
    let rec private supported (node:JsonNode)=
        if isNull node then false else
        match node["kind"].GetValue<string>() with
        | "named" -> node["arguments"].AsArray() |> Seq.forall supported
        | "array" -> node["rank"].GetValue<int>()>0 && node["rank"].GetValue<int>()<=32 && supported node["element"]
        | "function" -> supported node["domain"] && supported node["range"]
        | "typeParameter" -> true
        | _ -> false
    let rec private callableSupported arity depth (node:JsonNode)=
        if depth>32 || isNull node then false else
        let next=callableSupported arity (depth+1)
        match node["kind"].GetValue<string>() with
        | "named" -> node["arguments"].AsArray() |> Seq.forall next
        | "array" -> node["rank"].GetValue<int>()>0 && node["rank"].GetValue<int>()<=32 && next node["element"]
        | "function" -> next node["domain"] && next node["range"]
        | "methodParameter" -> let index=node["index"].GetValue<int>() in index>=0 && index<arity
        | _ -> false
    /// An additive, versioned scheme for ordinary module functions. The caller
    /// must select this declaration through the complete imported term scope.
    /// This is not a checked use-site or a license to reorder F# inference.
    let callableFacts resolve sourceName xmlDocId (symbol:FSharpSymbol):JsonNode =
        match symbol with
        | :? FSharpMemberOrFunctionOrValue as value when value.IsModuleValueOrMember && not value.IsMember ->
            match value.DeclaringEntity with
            | Some declaring when declaring.IsFSharpModule && declaring.TryFullName.IsSome ->
                let parameters=value.GenericParameters |> Seq.toArray
                let groups=value.CurriedParameterGroups |> Seq.map Seq.toArray |> Seq.toArray
                let encode=encode resolve [||] parameters 0
                let arguments=groups |> Array.map(Array.map(fun parameter -> encode parameter.Type))
                let result=encode value.ReturnParameter.Type
                let ordinary=not value.IsMutable && declaring.GenericParameters.Count=0 && parameters.Length<=64 &&
                             (parameters |> Array.forall(fun p -> not p.IsMeasure && not p.IsSolveAtCompileTime && p.Constraints.Count=0)) &&
                             groups.Length>0 && groups.Length<=64 && (groups |> Array.forall(fun group -> group.Length>0 && group.Length<=64 && group |> Array.forall(fun p -> not p.IsOptionalArg && not p.IsParamArrayArg && not p.IsOutArg && not p.IsInArg)))
                let usable=ordinary && callableSupported parameters.Length 0 result && (arguments |> Array.forall(Array.forall(callableSupported parameters.Length 0)))
                let owner=identity resolve declaring
                let path=owner["assemblyPath"].GetValue<string>()
                let assembly=owner["assemblyName"].GetValue<string>()
                let fullName=owner["fullName"].GetValue<string>()+"."+value.CompiledName
                Json.node {|callableFactsVersion=1;identity=Json.node {|assemblyPath=path;assemblyName=assembly;fullName=fullName|};declaringModule=owner;sourceFullName=sourceName;xmlDocId=xmlDocId;genericArity=parameters.Length;curriedArguments=arguments;returnType=result;supported=usable|}
            | _ -> null
        | _ -> null
    let typeExpression resolve parameters ty=encode resolve parameters [||] 0 ty
    let facts resolve resolveAssembly declarationName (symbol:FSharpSymbol):JsonNode*JsonNode =
        match symbol with
        | :? FSharpEntity as entity when not(entity.IsNamespace || entity.IsFSharpModule) && (entity.TryFullName.IsSome || entity.IsFSharpAbbreviation) ->
            let parameters=entity.GenericParameters |> Seq.toArray
            let encode=encode resolve parameters [||] 0
            let canonical=encode(entity.AsType())
            let baseType=entity.BaseType |> Option.map encode |> Option.defaultValue null
            let interfaces=entity.DeclaredInterfaces |> Seq.map encode |> Seq.toArray
            let declarationIdentity=
                if entity.IsFSharpAbbreviation || not(supported canonical) then
                    let path,name=resolveAssembly entity.Assembly.SimpleName
                    Json.node {|assemblyPath=path;assemblyName=name;fullName=declarationName|}
                else identity resolve entity
            let declaredMemberNames=entity.MembersFunctionsAndValues |> Seq.map(fun m -> m.DisplayName) |> Seq.distinct |> Seq.toArray
            Json.node {|identity=declarationIdentity;``type``=canonical;genericArity=parameters.Length;baseType=baseType;interfaces=interfaces;declaredMemberNames=declaredMemberNames;isValueType=entity.IsValueType;supported=supported canonical|},null
        | :? FSharpMemberOrFunctionOrValue as value when value.IsMember && value.IsProperty ->
            match value.DeclaringEntity with
            | Some declaring when declaring.TryFullName.IsSome ->
                let parameterCount=value.CurriedParameterGroups |> Seq.sumBy(fun group -> group.Count)
                let returnType=encode resolve (declaring.GenericParameters |> Seq.toArray) (value.GenericParameters |> Seq.toArray) 0 value.ReturnParameter.Type
                let readable=value.HasGetterMethod && value.GetterMethod.Accessibility.IsPublic
                let usable=readable && parameterCount=0 && not value.IsExtensionMember && not value.IsExplicitInterfaceImplementation && supported returnType
                null,Json.node {|declaringType=identity resolve declaring;name=value.DisplayName;isStatic=not value.IsInstanceMember;isProperty=true;parameterCount=parameterCount;returnType=returnType;supported=usable|}
            | _ -> null,null
        | _ -> null,null
