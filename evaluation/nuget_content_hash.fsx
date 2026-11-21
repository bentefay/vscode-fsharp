// Run with the pinned SDK's NuGet.Packaging/Common/Versioning/Frameworks references.
// NuGet's content hash for signed packages differs from the complete ZIP hash.
open NuGet.Packaging
open System.Threading

for path in fsi.CommandLineArgs.[1..] do
    use reader = new PackageArchiveReader(path)
    let signed = reader.IsSignedAsync(CancellationToken.None).GetAwaiter().GetResult()
    if signed then
        let signature = reader.GetPrimarySignatureAsync(CancellationToken.None).GetAwaiter().GetResult()
        reader.ValidateIntegrityAsync(signature.SignatureContent, CancellationToken.None).GetAwaiter().GetResult()
    printfn "%s\t%s\t%b" path (reader.GetContentHash(CancellationToken.None, null)) signed
