namespace FSharpNavigator.Semantic

open System
open System.IO
open System.Text.RegularExpressions
open System.Xml
open System.Xml.Linq
open System.Security.Cryptography

/// XML documentation is data, never trusted Markdown/HTML or a link command.
/// Bound each member's traversal/output and prohibit DTD/entity expansion.
module XmlDocumentation =
    let private limit=65536
    let private whitespace (text:string)=Regex.Replace(text,"\\s+"," ")
    let private escape (text:string)=
        text.Replace("&","&amp;").Replace("<","&lt;").Replace(">","&gt;")
        |> fun s -> Regex.Replace(s,"([\\\\`*_\\[\\]#!|])","\\$1")
    let private inlineCode (text:string)=
        let text=text.Replace("\r"," ").Replace("\n"," ")
        let width=Regex.Matches(text,"`+") |> Seq.cast<Match> |> Seq.fold(fun n m -> max n (m.Length+1)) 1
        let fence=String('`',width)
        let pad=if text.StartsWith('`') || text.EndsWith('`') || text.StartsWith(' ') || text.EndsWith(' ') then " " else ""
        fence+pad+text+pad+fence
    let private checkedText (text:string)=
        if text.Length>limit then raise(InvalidDataException "XML documentation member exceeds65536characters")
        text
    let render (memberElement:XElement) =
        let mutable visited=0
        let mutable rawCharacters=0
        let visit depth =
            visited<-visited+1
            if depth>32 || visited>8192 then raise(InvalidDataException "XML documentation member exceeds traversal budget")
        let accept text =
            rawCharacters<-rawCharacters+(text:string).Length
            if rawCharacters>limit then raise(InvalidDataException "XML documentation member exceeds text budget")
            text
        let attribute (element:XElement) name = let a=element.Attribute(XName.Get name) in if isNull a then "" else a.Value
        let rec literal depth (node:XNode) =
            visit depth
            match node with
            | :? XText as text -> accept text.Value
            | :? XElement as element -> element.Nodes() |> Seq.map(literal (depth+1)) |> String.concat "" |> checkedText
            | _ -> ""
        let rec content depth (node:XNode) =
            visit depth
            match node with
            | :? XText as text -> text.Value |> accept |> whitespace |> escape
            | :? XElement as element ->
                let children ()=element.Nodes() |> Seq.map(content (depth+1)) |> String.concat "" |> checkedText
                match element.Name.LocalName with
                | "see" | "seealso" ->
                    if element.Nodes() |> Seq.isEmpty |> not then children()
                    else
                        let keyword=attribute element "langword"
                        let reference=attribute element "cref"
                        let href=attribute element "href"
                        let label=if keyword<>"" then keyword elif reference<>"" then (if reference.Length>2 && reference[1]=':' then reference.Substring 2 else reference) else href
                        label |> accept |> inlineCode
                | "paramref" | "typeparamref" -> attribute element "name" |> accept |> inlineCode
                | "c" -> element.Nodes() |> Seq.map(literal (depth+1)) |> String.concat "" |> checkedText |> inlineCode
                | "code" ->
                    let text=element.Nodes() |> Seq.map(literal (depth+1)) |> String.concat "" |> checkedText
                    let width=Regex.Matches(text,"`+") |> Seq.cast<Match> |> Seq.fold(fun n m -> max n (m.Length+1)) 3
                    let fence=String('`',width)
                    "\n\n"+fence+"\n"+text.Trim('\r','\n')+"\n"+fence+"\n\n"
                | "para" -> "\n\n"+(children()).Trim()+"\n\n"
                | "br" -> "\n"
                | "list" -> "\n\n"+children()+"\n\n"
                | "item" -> "\n- "+(children()).Trim()+"\n"
                | "term" -> children()+": "
                | _ -> children()
            | _ -> ""
        let blocks=memberElement.Elements() |> Seq.map(fun element ->
            let body=element.Nodes() |> Seq.map(content 1) |> String.concat "" |> checkedText |> fun s -> s.Trim()
            let name=attribute element "name"
            match element.Name.LocalName with
            | "param" | "typeparam" when name<>"" -> "**"+escape name+":** "+body
            | "returns" -> "**Returns:** "+body
            | "value" -> "**Value:** "+body
            | "exception" -> "**Throws "+(attribute element "cref" |> escape)+":** "+body
            | "summary" | "remarks" | "example" -> body
            | _ -> "") |> Seq.filter((<>) "")
        blocks |> String.concat "\n\n" |> checkedText
    let readVerified (file:string) (expected:string) =
        let settings=XmlReaderSettings(DtdProcessing=DtdProcessing.Prohibit,XmlResolver=null,MaxCharactersInDocument=64L*1024L*1024L)
        use input=MetadataXml.openRegular file
        use algorithm=SHA256.Create()
        use hashed=new CryptoStream(input,algorithm,CryptoStreamMode.Read)
        use reader=XmlReader.Create(hashed,settings)
        let document=XDocument.Load(reader,LoadOptions.PreserveWhitespace)
        hashed.CopyTo(Stream.Null)
        let actual=Convert.ToHexString(algorithm.Hash).ToLowerInvariant()
        if actual<>expected then raise(InvalidDataException "XML bytes changed between selection and rendering")
        let raw=document.Descendants(XName.Get "member") |> Seq.choose(fun element -> let name=element.Attribute(XName.Get "name") in if isNull name then None else Some(name.Value,element)) |> Map.ofSeq
        raw |> Map.map(fun _ element ->
            let inherited=element.Element(XName.Get "inheritdoc")
            let element=
                if isNull inherited then element else
                let reference=inherited.Attribute(XName.Get "cref")
                if isNull reference then element else raw |> Map.tryFind reference.Value |> Option.defaultValue element
            render element)
