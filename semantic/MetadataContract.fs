namespace FSharpNavigator.Semantic

open System.Reflection
open System.Runtime.InteropServices
open System.Text.Json.Nodes
open FSharp.Compiler.CodeAnalysis

/// Versioned local derivation contract, not a cryptographic attestation.
/// Change the algorithm/component version when fact interpretation changes;
/// consumers independently accept/revoke profiles. Implementation hashes are
/// retained evidence, not a requirement that the exporter stays installed.
module MetadataContract =
    let private compilerIdentity="FSharp.Compiler.Service, Version=43.12.400.0, Culture=neutral, PublicKeyToken=b03f5f7f11d50a3a"
    let private compilerVersion="43.12.400-servicing.26379.115+14fbf8d5271c98133561eb55185fdb05b286f578"
    let capture ():JsonNode*FileStamp array =
        let compiler=typeof<FSharpChecker>.Assembly
        let informational=compiler.GetCustomAttribute<AssemblyInformationalVersionAttribute>().InformationalVersion
        if compiler.FullName<>compilerIdentity || informational<>compilerVersion then
            failwith "Metadata producer compiler profile is not supported by this semantic contract"
        let exporter=typeof<ProjectModel>.Assembly
        let inputs=[|Fingerprints.file exporter.Location;Fingerprints.file compiler.Location|]
        let producer=Json.node {|
            contract="fsharp-navigator.binary-facts"
            algorithmId="binary-facts-v2"
            componentVersions={|declarations=2;typeNames=1;scopeNames=1;termNames=1;xmlDocumentation=2;xmlSelection=1;callables=1|}
            compilerProfile={|assemblyIdentity=compiler.FullName;informationalVersion=informational|}
            implementationHashes=[|{|role="exporter";sha256=inputs[0].sha256|};{|role="fcs";sha256=inputs[1].sha256|}|]
            runtime=RuntimeInformation.FrameworkDescription
        |}
        producer,inputs
    let verify (inputs:FileStamp array) =
        for input in inputs do
            if (Fingerprints.state input.path).sha256<>input.sha256 then
                failwith "Metadata producer bytes changed during extraction"
