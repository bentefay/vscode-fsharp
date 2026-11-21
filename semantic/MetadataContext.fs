namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Diagnostics
open System.Collections.Generic
open System.Text.Json.Nodes
open FSharp.Compiler.CodeAnalysis
open FSharp.Compiler.Diagnostics
open FSharp.Compiler.Symbols
open FSharp.Compiler.Text

/// One binary-only imported reference snapshot. No source project, workspace
/// overlays or persistent checker is accepted by this explicit preparation API.
type MetadataImportContext = {
    references:string array
    before:Map<string,FileStamp>
    loaded:FSharpAssembly array
    selected:(string*string) array
    canonical:string->string
    checker:FSharpChecker
    producer:JsonNode
    producerInputs:FileStamp array
    importMilliseconds:float
    xmlSelections:MetadataXmlSelection array option
}

module MetadataContext =
    let create (references:string array) =
        let started=Stopwatch.StartNew()
        let references=references |> Array.map Paths.canonical |> Array.distinct
        if references.Length=0 || references.Length>4096 then failwith "Binary reference context requires1..4096 paths"
        for file in references do if not(File.Exists file) then failwith $"Missing metadata reference: {file}"
        let producer,producerInputs=MetadataContract.capture()
        let checker=FSharpChecker.Create(projectCacheSize=1,keepAssemblyContents=false,keepAllBackgroundResolutions=false,keepAllBackgroundSymbolUses=false)
        let source=SourceText.ofString "()\n"
        let script=Path.Combine(Path.GetTempPath(),"FSharpNavigator.BinaryContext.fsx")
        let options,optionErrors=checker.GetProjectOptionsFromScript(script,source,assumeDotNetFramework=false) |> Async.RunSynchronously
        let optionReference (arg:string)=if arg.StartsWith("-r:") then Some(arg.Substring(3).Trim('"')) elif arg.StartsWith("--reference:") then Some(arg.Substring(12).Trim('"')) else None
        let explicitNames=HashSet<string>(references |> Array.map Path.GetFileName,StringComparer.OrdinalIgnoreCase)
        let defaults=options.OtherOptions |> Array.filter(fun arg -> optionReference arg |> Option.forall(fun f -> not(explicitNames.Contains(Path.GetFileName f))))
        let options={options with OtherOptions=Array.append defaults (references |> Array.map(fun f -> "-r:"+f))}
        let before=Array.append references (options.OtherOptions |> Array.choose optionReference) |> Array.filter File.Exists |> Array.distinct |> Array.map Fingerprints.file |> Array.map(fun x->x.path,x) |> Map.ofArray
        let _,answer=checker.ParseAndCheckFileInProject(script,0,source,options) |> Async.RunSynchronously
        let checkedFile=match answer with FSharpCheckFileAnswer.Succeeded x->x | _->failwith "Metadata import did not complete"
        let errors=Array.append (optionErrors |> List.toArray) checkedFile.Diagnostics |> Array.filter(fun e->e.Severity=FSharpDiagnosticSeverity.Error)
        if errors.Length>0 then failwith(errors |> Array.map(fun e->e.Message) |> String.concat "\n")
        let loaded=checkedFile.ProjectContext.GetReferencedAssemblies() |> Seq.toArray
        let canonical file =
            let file=Paths.canonical file
            if before.ContainsKey file then file else
            let hash=(Fingerprints.file file).sha256
            references |> Array.filter(fun p->String.Equals(Path.GetFileName p,Path.GetFileName file,StringComparison.OrdinalIgnoreCase)&&before[p].sha256=hash) |> Array.sort |> Array.tryHead |> Option.defaultValue file
        let selected=loaded |> Array.map(fun a->a.SimpleName,a.FileName |> Option.map canonical |> Option.defaultWith(fun()->failwith "Assembly lacks binary provenance"))
        {references=references;before=before;loaded=loaded;selected=selected;canonical=canonical;checker=checker;producer=producer;producerInputs=producerInputs;importMilliseconds=started.Elapsed.TotalMilliseconds;xmlSelections=None}
    let require (context:MetadataImportContext) (references:string array) =
        for reference in references do
            if not(context.references |> Array.exists(fun file->Paths.comparer.Equals(file,Paths.canonical reference))) then
                failwith $"Shared metadata context does not include selected reference: {reference}"
    let verify (context:MetadataImportContext) =
        for input in context.before.Values do
            if (Fingerprints.state input.path).sha256<>input.sha256 then failwith $"Shared metadata input changed: {input.path}"
        MetadataContract.verify context.producerInputs
