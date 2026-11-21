namespace FSharpNavigator.Semantic

open System
open System.Text.Json
open System.Text.Json.Nodes

module Program =
    [<EntryPoint>]
    let main argv =
        let once=argv |> Array.contains "--once"
        let mutable running=true
        while running do
            let line=Console.ReadLine()
            if isNull line then running <- false
            elif not (String.IsNullOrWhiteSpace line) then
                let response=JsonObject()
                try
                    use request=JsonDocument.Parse line
                    let root=request.RootElement
                    response["id"] <- Json.tryProp "id" root |> Option.map (fun x -> JsonNode.Parse(x.GetRawText())) |> Option.defaultValue null
                    let methodName=Json.str "method" "" root
                    let parameters=Json.tryProp "params" root |> Option.defaultValue root
                    let result=
                        match methodName with
                        | "workspace/evaluate" -> Evaluation.workspace parameters |> Json.node
                        | "metadata/export" -> Semantic.exportMetadata parameters
                        | "metadata/environment" -> MetadataEnvironment.export parameters
                        | "metadata/prepare-experimental" -> running<-false; MetadataPreparation.run parameters
                        | "semantic/export" -> Semantic.exportSnapshot parameters
                        | "navigation/definition" | "navigation/references" | "navigation/hover" -> Semantic.oracle methodName parameters
                        | "shutdown" -> running <- false; Json.node {|ok=true|}
                        | "capabilities" -> Json.node {|protocolVersion=1;oneShot=true;sourceOverlay=true;oracleOnly=true;projectEvaluation="design-time-fsc-arguments";scriptSupport=false|}
                        | _ -> failwith $"Unknown method: {methodName}"
                    response["result"] <- result
                with ex ->
                    Console.Error.WriteLine(ex.ToString())
                    response["error"] <- Json.node {|code="helper-error";message=ex.Message|}
                Console.WriteLine(response.ToJsonString(Json.options))
                Console.Out.Flush()
                if once then running <- false
        0
