namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Diagnostics
open System.Threading
open System.Collections.Generic
open System.Text.Json
open System.Text.Json.Nodes

/// Opt-in experiment: one binary import, complete inventories, per-assembly
/// catalog closures, one private generation, then process exit. Not a daemon.
module MetadataPreparation =
    let private element (value:JsonNode) = JsonSerializer.SerializeToElement(value,Json.options)
    let private save path value =
        use stream=new FileStream(path,FileMode.CreateNew,FileAccess.Write,FileShare.None)
        JsonSerializer.Serialize(stream,value,Json.options)
        stream.Flush(true)
    let run (parameters:JsonElement) =
        for key in ["root";"projects";"documents";"solution"] do
            if Json.tryProp key parameters |> Option.isSome then failwith "Shared binary job does not accept source-project inputs"
        let references=Json.array "referencePaths" parameters |> Array.map(fun value->Paths.canonical(value.GetString())) |> Array.distinct
        let assemblies=Json.array "assemblyPaths" parameters |> Array.map(fun value->Paths.canonical(value.GetString())) |> Array.distinct
        if assemblies.Length=0 || assemblies.Length>4096 then failwith "Shared binary job requires1..4096 selected catalog targets"
        let referenceSet=HashSet<string>(references,Paths.comparer)
        if assemblies |> Array.exists(referenceSet.Contains >> not) then failwith "Shared catalog targets must belong to the selected reference context"
        let owned=Json.array "workspaceAssemblyPaths" parameters |> Array.map(fun value->Paths.canonical(value.GetString())) |> Array.distinct
        let ownedSet=HashSet<string>(owned,Paths.comparer)
        if assemblies |> Array.exists ownedSet.Contains then failwith "Workspace-owned binaries cannot be exported as external catalogs"
        let output=Json.str "outputDirectory" "" parameters
        if output="" then failwith "Shared binary job requires outputDirectory"
        let output=Paths.canonical output
        if File.Exists output || Directory.Exists output then failwith "Shared binary job requires a new output generation"
        let memoryMb=Json.integer "maxWorkingSetMb" 512 parameters
        let timeout=Json.integer "timeoutMilliseconds" 120000 parameters
        if memoryMb<32 || memoryMb>4096 || timeout<1000 || timeout>300000 then failwith "Shared binary job resource guard is outside supported bounds"
        Directory.CreateDirectory(Path.GetDirectoryName output) |> ignore
        let stage=Path.Combine(Path.GetDirectoryName output,".binary-job-"+Guid.NewGuid().ToString("N"))
        Directory.CreateDirectory stage |> ignore
        let started=Stopwatch.StartNew()
        let mutable peak=0L
        use workerProcess=Process.GetCurrentProcess()
        use guard=new Timer((fun _ ->
            try
                workerProcess.Refresh()
                let rss=workerProcess.WorkingSet64
                let mutable old=Interlocked.Read(&peak)
                while rss>old && Interlocked.CompareExchange(&peak,rss,old)<>old do old<-Interlocked.Read(&peak)
                if rss>int64 memoryMb*1024L*1024L || started.ElapsedMilliseconds>int64 timeout then
                    Console.Error.WriteLine("Shared binary preparation exceeded its sampled process-memory/deadline guard; no accepted response")
                    Environment.Exit(75)
            with _ ->
                Console.Error.WriteLine("Shared binary preparation resource sampling failed")
                Environment.Exit(76)),null,0,25)
        try
            let xmlSelections=assemblies |> Array.map MetadataXml.capture
            let importedContext=MetadataContext.create references
            let context={importedContext with xmlSelections=Some xmlSelections}
            let phases=ResizeArray<obj>()
            phases.Add(box {|phase="binary-import";milliseconds=context.importMilliseconds;importCount=1;selectedAssemblies=context.selected.Length|})
            let catalogDirectory=Path.Combine(stage,"catalogs")
            Directory.CreateDirectory catalogDirectory |> ignore
            let inputs=Dictionary<string,string>(Paths.comparer)
            let addInputs (response:JsonNode) =
                for input in response["inputHashes"].AsArray() do
                    let path=input["path"].GetValue<string>()
                    let hash=input["sha256"].GetValue<string>()
                    match inputs.TryGetValue path with
                    | true,previous when previous<>hash -> failwith $"Shared artifact input certificates disagree: {path}"
                    | _ -> inputs[path]<-hash
            let shards=ResizeArray<string>()
            let mutable symbols=0
            for index in 0..assemblies.Length-1 do
                let phase=Stopwatch.StartNew()
                let relative=$"catalogs/{index:D4}.json"
                let response=Semantic.exportMetadataWithContext (Some context) (Json.node {|assemblyPaths=[|assemblies[index]|];referencePaths=references;outputPath=Path.Combine(stage,relative)|} |> element)
                addInputs response
                let count=response["symbolCount"].GetValue<int>()
                symbols<-symbols+count
                shards.Add relative
                phases.Add(box {|phase="catalog";assemblyPath=assemblies[index];symbols=count;milliseconds=phase.Elapsed.TotalMilliseconds|})
            let catalogInputs=inputs |> Seq.map(fun item->{path=item.Key;sha256=item.Value}) |> Seq.toArray
            save (Path.Combine(stage,"libraries.json")) {|schemaVersion=2;factsVersion=2;metadataCertificateVersion=1;producer=context.producer.DeepClone();completeness="exact";scope="binary-declarations-only";catalogs=shards;inputHashes=catalogInputs|}
            let phase=Stopwatch.StartNew()
            let environment=MetadataEnvironment.exportWithContext (Some context) (Json.node {|referencePaths=references;workspaceAssemblyPaths=owned;outputPath=Path.Combine(stage,"builtins.json")|} |> element)
            addInputs environment
            phases.Add(box {|phase="environment";visitedEntities=environment["visitedEntities"].GetValue<int>();milliseconds=phase.Elapsed.TotalMilliseconds|})
            // Validate all artifacts together after the final traversal. Earlier
            // shard certificates must not silently survive later input changes.
            MetadataContext.verify context
            MetadataXml.verify xmlSelections
            for input in inputs do
                let current=if input.Value="missing" || input.Value="directory" then (MetadataXml.state input.Key).sha256 else (Fingerprints.file input.Key).sha256
                if current<>input.Value then failwith $"Shared preparation input changed before publication: {input.Key}"
            let result=Json.node {|kind="experimental-shared-binary-preparation";schemaVersion=2;factsVersion=2;metadataCertificateVersion=1;completeness="exact";outputDirectory=output;catalogPath=Path.Combine(output,"libraries.json");environmentPath=Path.Combine(output,"builtins.json");catalogSymbols=symbols;catalogShards=shards.Count;visitedEntities=environment["visitedEntities"].GetValue<int>();importCount=1;sourceProjectChecks=0;phases=phases;elapsedMilliseconds=started.Elapsed.TotalMilliseconds;sampledPeakWorkingSetBytes=Interlocked.Read(&peak);resourceGuard={|scope="sampled-current-binary-only-process";intervalMilliseconds=25;maxWorkingSetMb=memoryMb;timeoutMilliseconds=timeout|};xmlCacheScope="one-catalog-assembly";inventoryOutput="streamed-serialization-of-complete-in-memory-name-maps";producer=context.producer.DeepClone()|}
            save (Path.Combine(stage,"job.json")) result
            Directory.Move(stage,output)
            result
        finally
            if Directory.Exists stage then Directory.Delete(stage,true)
