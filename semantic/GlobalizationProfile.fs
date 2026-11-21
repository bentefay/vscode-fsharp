namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text
open System.Text.Json
open System.Text.Json.Nodes
open System.Collections.Generic
open System.Runtime.InteropServices
open System.Security.Cryptography

/// Startup inputs outside MSBuild's property-read events. This deliberately
/// supports the tested Linux/glibc/runtime profile, not arbitrary loader hooks.
module GlobalizationProfile =
    let selectors = [|
        "LANG";"LC_ALL";"LC_MESSAGES";"LC_CTYPE";"LANGUAGE";"DOTNET_CLI_UI_LANGUAGE";"VSLANG"
        "LOCPATH";"LOCALE_ARCHIVE"
        "DOTNET_SYSTEM_GLOBALIZATION_INVARIANT";"DOTNET_SYSTEM_GLOBALIZATION_PREDEFINED_CULTURES_ONLY"
        "DOTNET_SYSTEM_GLOBALIZATION_APPLOCALICU";"DOTNET_SYSTEM_GLOBALIZATION_USENLS";"DOTNET_ICU_VERSION_OVERRIDE"
        "ICU_DATA";"ICU_TIMEZONE_FILES_DIR"
        "LD_LIBRARY_PATH";"LD_PRELOAD";"LD_AUDIT";"LD_HWCAP_MASK";"LD_ORIGIN_PATH";"LD_ASSUME_KERNEL";"GLIBC_TUNABLES"
        "DOTNET_ADDITIONAL_DEPS";"DOTNET_SHARED_STORE";"DOTNET_STARTUP_HOOKS"
        "MSBUILDDISABLEFEATURESFROMVERSION";"MsBuildSkipEagerWildCardEvaluationRegexes"
        "MSBUILDUSECASESENSITIVEITEMNAMES";"MSBUILDIGNORETREATASLOCALPROPERTY";"MSBUILDIGNOREEMPTYIMPORTS"
        "MsBuildForceEvaluateAsFullFramework";"MSBUILD_SDKREFERENCE_PROPERTY_EXPANSION_MODE"
    |]

    let private unsupported = selectors |> Array.skip 7
    let private utf8 = UTF8Encoding(false,true)
    let private sha (bytes:byte array) = SHA256.HashData bytes |> Convert.ToHexString |> fun x -> x.ToLowerInvariant()
    let private byteCompare (a:byte array) (b:byte array) =
        let mutable i=0
        let mutable result=0
        while result=0 && i<min a.Length b.Length do
            result <- compare a[i] b[i]
            i <- i+1
        if result=0 then compare a.Length b.Length else result
    let private rawLink path =
        try (FileInfo(path)).LinkTarget
        with :? FileNotFoundException | :? DirectoryNotFoundException -> null
    let private isIcu (path:string) =
        let name=Path.GetFileName path
        name.StartsWith("libicu",StringComparison.Ordinal) && name.Contains(".so",StringComparison.Ordinal)

    // Read the actual modern glibc cache's value paths, including an entry
    // whose target is currently missing. Such a target can become selectable.
    let private cachedIcuPaths path =
        let bytes=File.ReadAllBytes path
        if bytes.Length<48 || bytes.Length>64*1024*1024 || Encoding.ASCII.GetString(bytes,0,20)<>"glibc-ld.so.cache1.1" || bytes[28]<>2uy then
            failwith "Unsupported dynamic-loader cache format"
        let count=int(BitConverter.ToUInt32(bytes,20))
        if count<0 || count>1000000 || 48L+int64 count*24L>int64 bytes.Length then failwith "Invalid dynamic-loader cache entries"
        let text offset =
            if offset<48+count*24 || offset>=bytes.Length then failwith "Invalid dynamic-loader cache string"
            let ending=Array.IndexOf(bytes,0uy,offset)
            if ending<offset || ending-offset>4096 then failwith "Invalid dynamic-loader cache path"
            utf8.GetString(bytes,offset,ending-offset)
        [|for i in 0..count-1 do
            let key=text(int(BitConverter.ToUInt32(bytes,48+i*24+4)))
            let value=text(int(BitConverter.ToUInt32(bytes,48+i*24+8)))
            if isIcu key then
                if not(Path.IsPathRooted value) then failwith "Nonabsolute dynamic-loader cache entry"
                yield Paths.canonical value|] |> Array.distinct

    // ELF64 DT_RPATH/DT_RUNPATH can add search directories even when all LD_*
    // overrides are absent. Read only bounded headers/string tables, not a
    // complete native library (ICU data alone is tens of megabytes).
    let private elfSearchPaths path =
        use stream=File.OpenRead path
        use reader=new BinaryReader(stream,utf8,true)
        let read offset length =
            if offset<0L || offset+int64 length>stream.Length then failwith "Invalid ELF bounds"
            stream.Position<-offset
            let bytes=reader.ReadBytes length
            if bytes.Length<>length then failwith "Truncated ELF input"
            bytes
        if stream.Length<4L then [||] else
        let magic=read 0L 4
        if magic<>[|0x7fuy;byte 'E';byte 'L';byte 'F'|] then [||] else
        let header=read 0L 64
        if header[4]<>2uy || header[5]<>1uy || BitConverter.ToUInt16(header,18)<>62us then failwith "Unsupported native ELF profile"
        let offset=int64(BitConverter.ToUInt64(header,32))
        let size=int(BitConverter.ToUInt16(header,54))
        let count=int(BitConverter.ToUInt16(header,56))
        if size<56 || count>1024 then failwith "Unsupported ELF program headers"
        let headers=[|for i in 0..count-1 do
                        let h=read (offset+int64(i*size)) 56
                        yield BitConverter.ToUInt32(h,0),int64(BitConverter.ToUInt64(h,8)),BitConverter.ToUInt64(h,16),BitConverter.ToUInt64(h,32)|]
        match headers |> Array.tryFind(fun (kind,_,_,_)->kind=2u) with
        | None -> [||]
        | Some(_,position,_,length) ->
            if length>1048576UL || length%16UL<>0UL then failwith "Unsupported ELF dynamic table"
            let table=read position (int length)
            let entries=[|for i in 0..16..table.Length-16 do yield BitConverter.ToInt64(table,i),BitConverter.ToUInt64(table,i+8)|]
            let wanted=entries |> Array.filter(fun (tag,_)->tag=15L || tag=29L)
            if wanted.Length=0 then [||] else
            let address=entries |> Array.find(fun (tag,_)->tag=5L) |> snd
            let length=entries |> Array.find(fun (tag,_)->tag=10L) |> snd
            if length>1048576UL then failwith "Unsupported ELF string table"
            let _,position,start,_=headers |> Array.find(fun (kind,_,start,length)->kind=1u && address>=start && address-start<length)
            let strings=read (position+int64(address-start)) (int length)
            wanted |> Array.collect(fun (_,index)->
                if index>=uint64 strings.Length then failwith "Invalid ELF search-path string"
                let ending=Array.IndexOf(strings,0uy,int index)
                if ending<0 then failwith "Unterminated ELF search path"
                let value=utf8.GetString(strings,int index,ending-int index)
                value.Split(':') |> Array.map(fun part ->
                    if part="" then failwith "Current-directory ELF search path is unsupported"
                    let expanded=part.Replace("${ORIGIN}",Path.GetDirectoryName path).Replace("$ORIGIN",Path.GetDirectoryName path)
                    if expanded.Contains('$') || not(Path.IsPathRooted expanded) then failwith "Dynamic ELF search path is unsupported"
                    Paths.canonical expanded))

    let certify (ambient:Dictionary<string,string>) (sdkRoot:string) (traces:JsonElement array) : JsonNode * string array * string array =
        let diagnostics=ResizeArray<string>()
        let inputs=HashSet<string>(Paths.comparer)
        let fail message=diagnostics.Add message
        try
            if not(OperatingSystem.IsLinux()) || RuntimeInformation.ProcessArchitecture<>Architecture.X64 || not BitConverter.IsLittleEndian then
                failwith "Globalization profile requires tested Linux x64"
            // The MSBuild property map is case-insensitive; POSIX getenv is
            // not. Do not let a lower-case alias stand in for a startup knob.
            for entry in Environment.GetEnvironmentVariables() |> Seq.cast<System.Collections.DictionaryEntry> do
                let actual=string entry.Key
                match selectors |> Array.tryFind(fun name -> name.Equals(actual,StringComparison.OrdinalIgnoreCase)) with
                | Some name when actual<>name -> fail $"Noncanonical startup selector casing is unsupported: {name}"
                | _ -> ()
            for name in unsupported do
                match ambient.TryGetValue name with
                | true,value when value<>"" -> fail $"Unsupported startup selector: {name}"
                | _ -> ()
            if diagnostics.Count>0 then failwith "Startup override profile is unsupported"
            let observations=traces |> Array.collect(Json.array "globalizationObservations")
            if observations.Length=0 || traces |> Array.exists(fun t -> Json.integer "globalizationObservationCount" 0 t<3 || (Json.array "globalizationObservations" t).Length=0) then
                failwith "MSBuild globalization observations are missing"
            let first=observations[0]
            let stableFields=[|"cultureName";"uiCultureName";"sortId";"runtimeVersion";"runtimeDirectory";"processPath";"libcMessagesLocale"|]
            for field in stableFields do
                if Json.tryProp field first |> Option.isNone then failwith $"Missing globalization observation: {field}"
                if observations |> Array.exists(fun observation -> Json.str field "" observation<>Json.str field "" first) then
                    failwith $"MSBuild globalization state changed between observations: {field}"
            let sortVersion=Json.integer "sortVersion" -1 first
            if sortVersion<0 || observations |> Array.exists(fun o -> Json.integer "sortVersion" -1 o<>sortVersion) then failwith "Invalid or changed globalization sort version"
            // ICU's default-locale discovery first queries libc LC_MESSAGES.
            // C/POSIX takes the directly witnessed environment fallback; other
            // bootstrap states need an explicit locale-data dependency profile.
            let libcMessages=Json.str "libcMessagesLocale" "" first
            if libcMessages<>"C" && libcMessages<>"POSIX" then failwith "Non-default libc locale bootstrap requires a supported locale-data profile"
            if Json.str "runtimeVersion" "" first<>"10.0.11" || Path.GetFileName(sdkRoot.TrimEnd(Path.DirectorySeparatorChar))<>"10.0.400" then
                failwith "Globalization profile requires tested SDK10.0.400/runtime10.0.11"
            let sdkRoot=(Paths.canonical sdkRoot).TrimEnd(Path.DirectorySeparatorChar)
            let installation=Path.GetDirectoryName(Path.GetDirectoryName sdkRoot)
            let runtime=(Paths.canonical(Json.str "runtimeDirectory" "" first)).TrimEnd(Path.DirectorySeparatorChar)
            if runtime<>Paths.canonical(Path.Combine(installation,"shared/Microsoft.NETCore.App/10.0.11")) then failwith "Unexpected runtime selection root"
            let loaded=observations |> Array.collect(fun o -> Json.array "loadedFiles" o |> Array.map(fun p -> Paths.canonical(p.GetString()))) |> Array.distinct
            // The pinned CoreCLR intercepts libSystem.Globalization.Native
            // P/Invokes via its statically linked GlobalizationResolveDllImport
            // registry before ordinary library loading. The implementation
            // bytes are therefore the actually mapped libcoreclr.so, even
            // though the distribution also ships a standalone shim.
            let required=[|"System.Private.CoreLib.dll";"libcoreclr.so";"libhostfxr.so";"libhostpolicy.so";"libc.so.6";"ld-linux-x86-64.so.2"|]
            for name in required do
                if not(loaded |> Array.exists(fun p -> Path.GetFileName p=name)) then failwith $"Missing loaded runtime dependency: {name}"
            for prefix in [|"libicuuc.so";"libicui18n.so";"libicudata.so"|] do
                if loaded |> Array.filter(fun p -> (Path.GetFileName p).StartsWith(prefix,StringComparison.Ordinal)) |> Array.length <> 1 then
                    failwith $"Unproven single ICU dependency: {prefix}"
            let loader=loaded |> Array.find(fun p -> Path.GetFileName p="ld-linux-x86-64.so.2")
            let code,version,_=Processes.runWithEnvironment ["LANG","C";"LC_ALL","C"] loader ["--version"] "/" 10000
            let versionLine=(version.Split('\n'))[0]
            if code<>0 || not(versionLine.EndsWith("version 2.41.",StringComparison.Ordinal)) then failwith "Unsupported glibc loader version"
            let code,help,_=Processes.runWithEnvironment ["LANG","C";"LC_ALL","C"] loader ["--help"] "/" 10000
            if code<>0 then failwith "Cannot inspect dynamic-loader search paths"
            let defaults=help.Split('\n') |> Array.choose(fun line ->
                let value=line.Trim()
                let suffix=" (system search path)"
                if value.EndsWith(suffix,StringComparison.Ordinal) then
                    let path=value.Substring(0,value.Length-suffix.Length)
                    if not(Path.IsPathRooted path) then failwith "Unsupported default loader path"
                    Some(Paths.canonical path)
                else None)
            if defaults.Length=0 || defaults.Length>16 then failwith "Unsupported dynamic-loader default paths"
            let cache="/etc/ld.so.cache"
            let cacheBefore=Fingerprints.file cache
            let cached=cachedIcuPaths cache
            inputs.Add cache |> ignore
            let search=HashSet<string>(defaults,Paths.comparer)
            for path in cached do search.Add(Path.GetDirectoryName path) |> ignore
            for path in loaded do
                inputs.Add path |> ignore
                for directory in elfSearchPaths path do search.Add directory |> ignore
            if search.Count>64 then failwith "Dynamic-loader search-domain limit exceeded"
            let directories=HashSet<string>(search,Paths.comparer)
            for directory in [|Path.Combine(installation,"sdk");Path.Combine(installation,"host/fxr");Path.Combine(installation,"shared/Microsoft.NETCore.App");sdkRoot;runtime;Path.GetDirectoryName loader|] do directories.Add directory |> ignore
            for path in loaded |> Array.filter(fun p -> Paths.within installation p) do directories.Add(Path.GetDirectoryName path) |> ignore
            // A new version is a membership mutation. Existing alternative
            // versions are outside this first tested host-selection profile.
            for parent,expected in [|Path.Combine(installation,"sdk"),"10.0.400";Path.Combine(installation,"host/fxr"),"10.0.11";Path.Combine(installation,"shared/Microsoft.NETCore.App"),"10.0.11"|] do
                for directory in Directory.EnumerateDirectories parent do
                    if Path.GetFileName directory<>expected then failwith "Multiple installed host/runtime/SDK choices require a supported selection profile"
            for path in [|Path.Combine(sdkRoot,"MSBuild.runtimeconfig.json");Path.Combine(sdkRoot,"MSBuild.deps.json");Path.Combine(runtime,"Microsoft.NETCore.App.deps.json");Path.Combine(runtime,".version");Path.Combine(runtime,"libSystem.Globalization.Native.so")|] do inputs.Add path |> ignore
            for directory in search do
                let hwcaps=Path.Combine(directory,"glibc-hwcaps")
                directories.Add hwcaps |> ignore
                if Directory.Exists hwcaps then
                    for child in Directory.EnumerateDirectories hwcaps do
                        directories.Add child |> ignore
                        if Directory.EnumerateFiles(child) |> Seq.exists isIcu then failwith "ICU hardware-capability alternatives require a supported selection profile"
                for name in [|"x86-64-v2";"x86-64-v3";"x86-64-v4"|] do directories.Add(Path.Combine(hwcaps,name)) |> ignore
                if Directory.Exists directory then
                    for path in Directory.EnumerateFiles directory |> Seq.filter isIcu do inputs.Add path |> ignore
            for path in cached do
                if path.Contains("/glibc-hwcaps/",StringComparison.Ordinal) then failwith "Cached ICU hardware-capability choices are unsupported"
                if File.Exists path then inputs.Add path |> ignore
            // Keep the first profile single-choice even when another soname
            // already exists but currently fails to load. Rewriting that file
            // into a valid higher version is a real selection change.
            for prefix in [|"libicuuc.so";"libicui18n.so";"libicudata.so"|] do
                let selected=loaded |> Array.find(fun p -> (Path.GetFileName p).StartsWith(prefix,StringComparison.Ordinal))
                let selectedHash=(Fingerprints.file selected).sha256
                for candidate in inputs |> Seq.filter(fun p -> (Path.GetFileName p).StartsWith(prefix,StringComparison.Ordinal)) do
                    if (Fingerprints.file candidate).sha256<>selectedHash then failwith "Multiple distinct ICU choices require a supported selection profile"
            if directories.Count>256 then failwith "Globalization directory-domain limit exceeded"
            let linkStates=Dictionary<string,string>(Paths.comparer)
            let addLinks path =
                let mutable current=Paths.canonical path
                while current<>null do
                    let target=rawLink current
                    match linkStates.TryGetValue current with
                    | true,old when old<>target -> failwith "Selection path symlink changed during capture"
                    | _ -> linkStates[current]<-target
                    current<-Path.GetDirectoryName current
            let capture directory =
                addLinks directory
                let exists=Directory.Exists directory
                if not exists && File.Exists directory then failwith "Selection directory became a file"
                let rows=if not exists then [||] else
                         Directory.EnumerateFileSystemEntries directory |> Seq.map(fun path ->
                             let attributes=File.GetAttributes path
                             let link=rawLink path
                             let kind=if link<>null then "symlink" elif (attributes &&& FileAttributes.Directory)<>enum<FileAttributes> 0 then "directory" elif File.Exists path then "file" else "other"
                             utf8.GetBytes(Path.GetFileName(path)+"\000"+kind+"\000"+(if isNull link then "" else link)+"\000")) |> Seq.toArray
                if rows.Length>100000 then failwith "Selection directory inventory limit exceeded"
                Array.sortInPlaceWith byteCompare rows
                use digest=IncrementalHash.CreateHash(HashAlgorithmName.SHA256)
                for row in rows do digest.AppendData row
                {|path=directory;exists=exists;entries=rows.Length;sha256=digest.GetHashAndReset() |> Convert.ToHexString |> fun x -> x.ToLowerInvariant()|}
            let selections=directories |> Seq.sortWith (fun a b -> StringComparer.Ordinal.Compare(a,b)) |> Seq.map capture |> Seq.toArray
            let files=inputs |> Seq.sortWith (fun a b -> StringComparer.Ordinal.Compare(a,b)) |> Seq.map(fun path -> addLinks path; Fingerprints.file path) |> Seq.toArray
            let hashes=files |> Array.map(fun stamp -> stamp.path,stamp.sha256) |> Map.ofArray
            for trace in traces do
                let observed=Json.array "loadedFileHashes" trace
                if observed.Length=0 then failwith "Missing evaluation-time runtime file hashes"
                for input in observed do
                    let path=Paths.canonical(Json.str "path" "" input)
                    let expected=Json.str "sha256" "" input
                    if expected.Length<>64 || not(File.Exists path) || (Fingerprints.file path).sha256<>expected then
                        failwith "Observed runtime input changed during preparation"
                for path in loaded do
                    match observed |> Array.tryFind(fun input -> Paths.canonical(Json.str "path" "" input)=path) with
                    | Some input when Json.str "sha256" "" input=hashes[path] -> ()
                    | _ -> failwith "Runtime input lacks matching evaluation-time provenance"
            if (Fingerprints.file cache).sha256<>cacheBefore.sha256 then failwith "Dynamic-loader cache changed during capture"
            for selection in selections do
                let current=capture selection.path
                if current<>selection then failwith "Globalization selection directory changed during capture"
            for path in linkStates.Keys |> Seq.toArray do
                if rawLink path<>linkStates[path] then failwith "Globalization path link changed during capture"
            let links=linkStates |> Seq.sortBy(fun pair -> pair.Key) |> Seq.map(fun pair -> {|path=pair.Key;target=pair.Value|}) |> Seq.toArray
            let result=Json.node {|profile="linux-x64-dotnet10-icu-v1";cultureName=Json.str "cultureName" "" first;uiCultureName=Json.str "uiCultureName" "" first;libcMessagesLocale=libcMessages;sortVersion=sortVersion;sortId=Json.str "sortId" "" first;runtimeVersion="10.0.11";loaderVersion="2.41";files=files;selectionDirectories=selections;pathLinks=links|}
            result,inputs |> Seq.toArray,diagnostics.ToArray()
        with error ->
            diagnostics.Add($"Globalization startup profile is unsupported: {error.Message}")
            null,[||],diagnostics.ToArray()
