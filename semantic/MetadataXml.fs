namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Runtime.InteropServices
open System.Security.Cryptography
open Microsoft.Win32.SafeHandles

type MetadataXmlSelection={assemblyPath:string;selectedPath:string;candidates:FileStamp array}

/// The delivered helper is Linux x64. statx's stable ABI verifies descriptor
/// kind; O_NONBLOCK prevents a FIFO replacement from blocking an open. Other
/// platforms require an independently tested adapter rather than File.Exists.
module MetadataXml =
    [<Struct;StructLayout(LayoutKind.Explicit,Size=256)>]
    type private Statx =
        [<FieldOffset(0)>] val mutable Mask:uint32
        [<FieldOffset(28)>] val mutable Mode:uint16
    [<DllImport("libc",EntryPoint="open",SetLastError=true)>]
    extern int private openFile(string path,int flags)
    [<DllImport("libc",EntryPoint="statx",SetLastError=true)>]
    extern int private statx(int directory,string path,int flags,uint32 mask,Statx& value)
    let private limit=64L*1024L*1024L
    type private BoundedRead(inner:Stream) =
        inherit Stream()
        let mutable count=0L
        override _.CanRead=true
        override _.CanSeek=false
        override _.CanWrite=false
        override _.Length=raise(NotSupportedException())
        override _.Position with get()=raise(NotSupportedException()) and set _=raise(NotSupportedException())
        override _.Flush()=()
        override _.Read(buffer,offset,length)=
            let wanted=if count>=limit then min 1 length else min length (int(limit-count))
            let read=inner.Read(buffer,offset,wanted)
            count<-count+int64 read
            if count>limit then raise(InvalidDataException "XML documentation exceeds64MiB input bound")
            read
        override _.Seek(_,_) = raise(NotSupportedException())
        override _.SetLength(_) = raise(NotSupportedException())
        override _.Write(_,_,_) = raise(NotSupportedException())
        override _.Dispose(disposing)=
            if disposing then inner.Dispose()
            base.Dispose(disposing)
    let private openKind path =
        if not(OperatingSystem.IsLinux()) || RuntimeInformation.ProcessArchitecture<>Architecture.X64 then
            failwith "Verified XML file-kind adapter currently requires Linux x64"
        let descriptor=openFile(path,0x800 ||| 0x80000)
        if descriptor<0 then
            let error=Marshal.GetLastPInvokeError()
            if error=2 || error=20 then None else raise(IOException($"Cannot verify XML candidate: errno {error}: {path}"))
        else
            let handle=new SafeFileHandle(IntPtr descriptor,true)
            let mutable status=Unchecked.defaultof<Statx>
            if statx(descriptor,"",0x1000,1u,&status)<>0 || status.Mask&&&1u=0u then
                let error=Marshal.GetLastPInvokeError()
                handle.Dispose()
                raise(IOException($"Cannot verify XML descriptor kind: errno {error}: {path}"))
            Some(handle,int(status.Mode&&&0xF000us))
    let private stream (handle:SafeFileHandle) =
        let input=new FileStream(handle,FileAccess.Read)
        if input.Length>limit then
            input.Dispose()
            raise(InvalidDataException "XML documentation exceeds64MiB input bound")
        new BoundedRead(input):>Stream
    let openRegular path =
        match openKind path with
        | Some(handle,0x8000) -> stream handle
        | Some(handle,_) -> handle.Dispose();raise(InvalidDataException $"XML candidate is not a regular file: {path}")
        | None -> raise(FileNotFoundException("Selected XML documentation disappeared",path))
    let state path =
        let path=Paths.canonical path
        match openKind path with
        | None -> {path=path;sha256="missing"}
        | Some(handle,0x4000) -> handle.Dispose();{path=path;sha256="directory"}
        | Some(handle,0x8000) ->
            use input=stream handle
            {path=path;sha256=SHA256.HashData input |> Convert.ToHexString |> fun value -> value.ToLowerInvariant()}
        | Some(handle,_) -> handle.Dispose();raise(InvalidDataException $"Unsupported special XML candidate: {path}")
    let capture assemblyPath =
        let first=Path.ChangeExtension(assemblyPath,".xml")
        let paths=[|first;Path.ChangeExtension(assemblyPath,".XML");Path.Combine(Path.GetDirectoryName(Path.GetDirectoryName assemblyPath),Path.GetFileName first)|]
        let seen=System.Collections.Generic.HashSet<string>(Paths.comparer)
        let prefix=ResizeArray<FileStamp>()
        let mutable selected=""
        for path in paths do
            if selected="" && seen.Add(Paths.canonical path) then
                let input=state path
                prefix.Add input
                if input.sha256<>"missing" && input.sha256<>"directory" then selected<-input.path
        {assemblyPath=assemblyPath;selectedPath=selected;candidates=prefix.ToArray()}
    let verify selections =
        for selection in selections do
            for input in selection.candidates do
                if (state input.path).sha256<>input.sha256 then failwith $"XML selection changed during extraction: {input.path}"
