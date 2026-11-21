using System.Collections.Concurrent;
using System.Collections.Immutable;
using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Xml.Linq;
using ICSharpCode.Decompiler;
using ICSharpCode.Decompiler.CSharp;
using ICSharpCode.Decompiler.CSharp.OutputVisitor;
using ICSharpCode.Decompiler.CSharp.Syntax;
using ICSharpCode.Decompiler.Documentation;
using ICSharpCode.Decompiler.Metadata;
using ICSharpCode.Decompiler.TypeSystem;

// A one-shot metadata/IL reader. It neither executes the requested assembly nor
// starts FCS. The native parent owns the hard deadline and process-tree guard.
internal static class Program
{
    const int MaxRequest = 64 * 1024, MaxOutput = 120 * 1024;
    const long MaxAssembly = 128L * 1024 * 1024;
    static int Main(string[] args)
    {
        object result;
        try
        {
            if (args.Length != 1 || args[0] != "--once") throw new InvalidOperationException("Expected --once");
            var input = new StringBuilder();
            while (true)
            {
                int ch = Console.In.Read();
                if (ch < 0 || ch == '\n') break;
                if (input.Length >= MaxRequest) throw new InvalidOperationException("Request exceeds size budget");
                input.Append((char)ch);
            }
            using var json = JsonDocument.Parse(input.ToString());
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(3));
            result = Run(json.RootElement, timeout.Token);
        }
        catch (Exception error)
        {
            string reason = error is OperationCanceledException ? "Decompilation exceeded its time budget" : error.Message;
            if (reason.Length > 400) reason = reason[..400];
            result = new { completeness = "unavailable", reason, locations = Array.Empty<object>(), documents = Array.Empty<object>() };
        }
        Console.WriteLine(JsonSerializer.Serialize(result));
        return 0;
    }

    static object Run(JsonElement request, CancellationToken cancellation)
    {
        string originalPath = Path.GetFullPath(request.GetProperty("assemblyPath").GetString() ?? "");
        string id = request.GetProperty("xmlDocId").GetString() ?? "";
        if (id.Length < 3 || id[1] != ':' || !"TMPFE".Contains(id[0])) throw new InvalidOperationException("Unsupported exact XML member identity");
        using var files = new Inputs(cancellation);
        var original = files.Open(originalPath, body: true);
        var originalEntity = FindBound(original, id, files, cancellation);
        string originalSignature = Signature(originalEntity.File, originalEntity.Handle);
        bool redirected = IsReference(originalEntity.File);
        var target = originalEntity;
        if (redirected)
        {
            string implementation = ImplementationPath(originalPath, files);
            if (!SameAssembly(originalPath, implementation)) throw new InvalidOperationException("Reference and implementation assembly identities differ");
            target = FindBound(files.Open(implementation), id, files, cancellation);
            if (IsReference(target.File)) throw new InvalidOperationException("Only a reference-assembly stub is available");
            if (originalSignature != Signature(target.File, target.Handle)) throw new InvalidOperationException("Reference and implementation member signatures differ");
        }
        if (IsReference(target.File)) throw new InvalidOperationException("Reference assemblies do not contain implementation bodies");
        target = (files.Open(target.File.FileName, body: true), target.Handle);
        var metadata=target.File.Metadata;
        if (target.Handle.Kind==HandleKind.MethodDefinition && metadata.GetMethodDefinition((MethodDefinitionHandle)target.Handle).RelativeVirtualAddress==0)
            throw new InvalidOperationException("Requested method has no decompilable IL implementation body");
        if (target.Handle.Kind==HandleKind.PropertyDefinition)
        {
            var accessors=metadata.GetPropertyDefinition((PropertyDefinitionHandle)target.Handle).GetAccessors();
            if ((accessors.Getter.IsNil || metadata.GetMethodDefinition(accessors.Getter).RelativeVirtualAddress==0) &&
                (accessors.Setter.IsNil || metadata.GetMethodDefinition(accessors.Setter).RelativeVirtualAddress==0))
                throw new InvalidOperationException("Requested property has no decompilable IL implementation body");
        }
        cancellation.ThrowIfCancellationRequested();
        var settings = new DecompilerSettings { ShowXmlDocumentation = false, ThrowOnAssemblyResolveErrors = true };
        var resolver = new TrackingResolver(target.File, files);
        var decompiler = new CSharpDecompiler(target.File, resolver, settings)
        {
            CancellationToken = cancellation,
            // ILSpy otherwise builds the framework XML-doc aggregate even when
            // ShowXmlDocumentation is false. Hover owns documentation separately.
            DocumentationProvider = NoDocumentation.Instance
        };
        var entity = decompiler.TypeSystem.MainModule.ResolveEntity(target.Handle)
            ?? throw new InvalidOperationException("Requested metadata entity could not be resolved");
        if (IdStringProvider.GetIdString(entity) != id) throw new InvalidOperationException("Resolved member identity differs");
        var tree = decompiler.Decompile(target.Handle);
        if (tree.Descendants.Any(node => node is ErrorExpression)) throw new InvalidOperationException("Decompiler could not reconstruct the implementation");
        using var output = new BoundedWriter(MaxOutput);
        output.Write("// Decompiled implementation; generated from metadata and IL.\n");
        var writer = new IdentifierWriter(output, entity);
        // Prefix is one complete line; TokenWriter starts counting at one.
        tree.AcceptVisitor(new CSharpOutputVisitor(writer, settings.CSharpFormattingOptions));
        var range = writer.Range ?? throw new InvalidOperationException("Exact declaration identifier range is unavailable");
        string text = output.ToString();
        cancellation.ThrowIfCancellationRequested();
        files.Add(typeof(Program).Assembly.Location);
        files.Add(typeof(CSharpDecompiler).Assembly.Location);
        foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
            if (!assembly.IsDynamic && !string.IsNullOrEmpty(assembly.Location)) files.Add(assembly.Location);
        foreach (string path in files.Paths.ToArray())
        {
            files.Add(Path.ChangeExtension(path, ".pdb"), allowMissing: true);
            files.Add(Path.ChangeExtension(path, ".xml"), allowMissing: true);
            files.Add(Path.ChangeExtension(path, ".deps.json"), allowMissing: true);
            files.Add(Path.ChangeExtension(path, ".runtimeconfig.json"), allowMissing: true);
        }
        var dependencies = files.Certificate();
        string key = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(JsonSerializer.Serialize(new { id, dependencies, text }))));
        string uri = $"fsharp-navigator://metadata/{key}.cs";
        return new
        {
            completeness = "exact", targetKind = "decompiled-implementation", sourceMapping = "metadata-member-identifier",
            documents = new[] { new { uri, language = "csharp", text, kind = "decompiled-implementation" } },
            locations = new[] { new { uri, range } },
            symbol = new { xmlDocId = id, assemblyPath = target.File.FileName, originalAssemblyPath = originalPath, metadataToken = MetadataTokens.GetToken(target.Handle) },
            dependencies, referenceAssemblyRedirected = redirected,
            decompilerVersion = typeof(CSharpDecompiler).Assembly.GetName().Version?.ToString(),
            originalSource = "unavailable-or-not-mapped-to-an-exact-declaration"
        };
    }

    // Bind only inside the requested assembly or its explicit type-forwarding
    // chain. Searching all referenced assemblies by full name is not a proof.
    static (MetadataFile File, EntityHandle Handle) FindBound(MetadataFile origin, string id, Inputs files, CancellationToken cancellation)
    {
        var seen = new HashSet<string>(StringComparer.Ordinal);
        MetadataFile current = origin;
        for (int depth = 0; depth < 16; depth++)
        {
            cancellation.ThrowIfCancellationRequested();
            if (!seen.Add(current.FileName)) throw new InvalidOperationException("Cyclic type forwarding");
            var found = IdStringProvider.FindEntity(id, new[] { current });
            if (!found.Handle.IsNil && found.Handle.Kind != HandleKind.ExportedType)
            {
                if (IdStringProvider.GetIdString(current, found.Handle) != id) throw new InvalidOperationException("Metadata identity mismatch");
                return (current, found.Handle);
            }
            string owner = OwnerFromId(id);
            var reader = current.Metadata;
            var forwarders = reader.ExportedTypes.Select(h => reader.GetExportedType(h)).Where(t =>
                (t.Attributes & (TypeAttributes)0x00200000) != 0 &&
                t.Implementation.Kind == HandleKind.AssemblyReference &&
                Join(reader.GetString(t.Namespace), reader.GetString(t.Name)) == owner).ToArray();
            if (forwarders.Length != 1) throw new InvalidOperationException("Requested exact member is not defined or explicitly forwarded by the assembly");
            var reference = new ICSharpCode.Decompiler.Metadata.AssemblyReference(current, (AssemblyReferenceHandle)forwarders[0].Implementation);
            var next = new TrackingResolver(current, files).Resolve(reference) ?? throw new InvalidOperationException("Forwarded implementation unavailable");
            if (AssemblyName.GetAssemblyName(next.FileName).FullName != reference.FullName) throw new InvalidOperationException("Forwarded assembly identity mismatch");
            current = next;
        }
        throw new InvalidOperationException("Type forwarding exceeded depth budget");
    }
    static string OwnerFromId(string id)
    {
        if (id[0] == 'T') return id[2..];
        int end = id.IndexOf('('); if (end < 0) end = id.IndexOf('~'); if (end < 0) end = id.Length;
        int split = id.LastIndexOf('.', end - 1);
        if (split < 2) throw new InvalidOperationException("Malformed member identity");
        return id[2..split];
    }
    static string Join(string ns, string name) => ns.Length == 0 ? name : ns + "." + name;
    static bool SameAssembly(string left, string right) => AssemblyName.GetAssemblyName(left).FullName == AssemblyName.GetAssemblyName(right).FullName;
    static bool IsReference(MetadataFile file)
    {
        var reader = file.Metadata;
        foreach (var handle in reader.GetAssemblyDefinition().GetCustomAttributes())
        {
            var constructor = reader.GetCustomAttribute(handle).Constructor;
            EntityHandle type = constructor.Kind switch
            {
                HandleKind.MemberReference => reader.GetMemberReference((MemberReferenceHandle)constructor).Parent,
                HandleKind.MethodDefinition => reader.GetMethodDefinition((MethodDefinitionHandle)constructor).GetDeclaringType(),
                _ => default
            };
            string name = type.Kind switch
            {
                HandleKind.TypeReference => Join(reader.GetString(reader.GetTypeReference((TypeReferenceHandle)type).Namespace), reader.GetString(reader.GetTypeReference((TypeReferenceHandle)type).Name)),
                HandleKind.TypeDefinition => Join(reader.GetString(reader.GetTypeDefinition((TypeDefinitionHandle)type).Namespace), reader.GetString(reader.GetTypeDefinition((TypeDefinitionHandle)type).Name)),
                _ => ""
            };
            if (name == "System.Runtime.CompilerServices.ReferenceAssemblyAttribute") return true;
        }
        return false;
    }
    static string ImplementationPath(string path, Inputs files)
    {
        var tfm = Directory.GetParent(path)!;
        var refDir = tfm.Parent;
        if (refDir?.Name != "ref") throw new InvalidOperationException("Reference assembly has no verified ref/runtime layout");
        string package = refDir.Parent!.FullName;
        string samePackage = Path.Combine(package, "lib", tfm.Name, Path.GetFileName(path));
        files.Add(samePackage, allowMissing: true);
        if (File.Exists(samePackage)) return samePackage;
        var packVersion = refDir.Parent!;
        var packName = packVersion.Parent;
        var packs = packName?.Parent;
        if (packs?.Name != "packs" || packName!.Name != "Microsoft.NETCore.App.Ref") throw new InvalidOperationException("Matching implementation package is unavailable");
        string frameworkList = Path.Combine(packVersion.FullName, "data", "FrameworkList.xml");
        files.Add(frameworkList);
        var root = XDocument.Load(frameworkList).Root ?? throw new InvalidOperationException("Missing framework identity");
        string frameworkVersion = packVersion.Name;
        if (root.Attribute("FrameworkName")?.Value != "Microsoft.NETCore.App" ||
            root.Attribute("TargetFrameworkIdentifier")?.Value != ".NETCoreApp" ||
            "net" + root.Attribute("TargetFrameworkVersion")?.Value != tfm.Name)
            throw new InvalidOperationException("Framework/runtime layout identity is not exact");
        string implementation = Path.Combine(packs.Parent!.FullName, "shared", "Microsoft.NETCore.App", frameworkVersion, Path.GetFileName(path));
        files.Add(implementation, allowMissing: true);
        if (!File.Exists(implementation)) throw new InvalidOperationException("Exact framework implementation is not installed");
        return implementation;
    }

    static string Signature(MetadataFile file, EntityHandle handle)
    {
        var r = file.Metadata; var provider = new SignatureProvider();
        return handle.Kind switch
        {
            HandleKind.MethodDefinition => Method(r.GetMethodDefinition((MethodDefinitionHandle)handle).DecodeSignature(provider, (object?)null)),
            HandleKind.PropertyDefinition => Method(r.GetPropertyDefinition((PropertyDefinitionHandle)handle).DecodeSignature(provider, (object?)null)),
            HandleKind.FieldDefinition => r.GetFieldDefinition((FieldDefinitionHandle)handle).DecodeSignature(provider, (object?)null),
            HandleKind.EventDefinition => provider.Type(r, r.GetEventDefinition((EventDefinitionHandle)handle).Type),
            HandleKind.TypeDefinition => "type:" + IdStringProvider.GetIdString(file, handle),
            _ => throw new InvalidOperationException("Unsupported member signature")
        };
    }
    static string Method(MethodSignature<string> signature) => $"{signature.Header.RawValue}:{signature.GenericParameterCount}:{signature.RequiredParameterCount}:{signature.ReturnType}({string.Join(",", signature.ParameterTypes)})";
    sealed class SignatureProvider : ISignatureTypeProvider<string, object?>
    {
        public string Type(MetadataReader r, EntityHandle h) => h.Kind switch
        {
            HandleKind.TypeDefinition => GetTypeFromDefinition(r, (TypeDefinitionHandle)h, 0),
            HandleKind.TypeReference => GetTypeFromReference(r, (TypeReferenceHandle)h, 0),
            HandleKind.TypeSpecification => GetTypeFromSpecification(r, null, (TypeSpecificationHandle)h, 0),
            _ => throw new InvalidOperationException("Unsupported signature type")
        };
        public string GetArrayType(string element, ArrayShape shape) => $"{element}[{shape.Rank};{string.Join(',',shape.Sizes)};{string.Join(',',shape.LowerBounds)}]";
        public string GetByReferenceType(string element) => element + "&";
        public string GetFunctionPointerType(MethodSignature<string> signature) => "fn:" + Method(signature);
        public string GetGenericInstantiation(string type, ImmutableArray<string> arguments) => type + "<" + string.Join(',', arguments) + ">";
        public string GetGenericMethodParameter(object? context, int index) => "!!" + index;
        public string GetGenericTypeParameter(object? context, int index) => "!" + index;
        public string GetModifiedType(string modifier, string type, bool required) => $"{type}[{required}:{modifier}]";
        public string GetPinnedType(string element) => element + " pinned";
        public string GetPointerType(string element) => element + "*";
        public string GetPrimitiveType(PrimitiveTypeCode code) => "primitive:" + code;
        public string GetSZArrayType(string element) => element + "[]";
        public string GetTypeFromDefinition(MetadataReader r, TypeDefinitionHandle h, byte kind)
        {
            var t=r.GetTypeDefinition(h); var parent=t.GetDeclaringType();
            var a=r.GetAssemblyDefinition();var identity=new AssemblyName{Name=r.GetString(a.Name),Version=a.Version,CultureName=r.GetString(a.Culture)};identity.SetPublicKey(r.GetBlobBytes(a.PublicKey));
            return parent.IsNil ? $"[{identity.FullName}]{Join(r.GetString(t.Namespace),r.GetString(t.Name))}" : GetTypeFromDefinition(r,parent,kind)+"+"+r.GetString(t.Name);
        }
        public string GetTypeFromReference(MetadataReader r, TypeReferenceHandle h, byte kind)
        {
            var t=r.GetTypeReference(h);
            string owner=t.ResolutionScope.Kind==HandleKind.AssemblyReference ? AssemblyReferenceIdentity(r,(AssemblyReferenceHandle)t.ResolutionScope) : t.ResolutionScope.Kind==HandleKind.TypeReference ? GetTypeFromReference(r,(TypeReferenceHandle)t.ResolutionScope,kind) : "module";
            return $"[{owner}]{Join(r.GetString(t.Namespace),r.GetString(t.Name))}";
        }
        public string GetTypeFromSpecification(MetadataReader r, object? context, TypeSpecificationHandle h, byte kind) => r.GetTypeSpecification(h).DecodeSignature(this,context);
        static string AssemblyReferenceIdentity(MetadataReader r, AssemblyReferenceHandle h) {var a=r.GetAssemblyReference(h);var n=new AssemblyName{Name=r.GetString(a.Name),Version=a.Version,CultureName=r.GetString(a.Culture)};if((a.Flags&AssemblyFlags.PublicKey)!=0)n.SetPublicKey(r.GetBlobBytes(a.PublicKeyOrToken));else n.SetPublicKeyToken(r.GetBlobBytes(a.PublicKeyOrToken));return n.FullName;}
    }
    sealed class NoDocumentation : IDocumentationProvider
    {
        internal static readonly NoDocumentation Instance = new();
        public string? GetDocumentation(IEntity entity) => null;
    }
    sealed class IdentifierWriter(TextWriter output, IEntity wanted) : TextWriterTokenWriter(output)
    {
        public object? Range { get; private set; }
        public override void WriteIdentifier(Identifier identifier)
        {
            bool declaration = identifier.Parent is EntityDeclaration or VariableInitializer;
            var symbol = identifier.GetSymbol() as IEntity ?? identifier.Parent?.GetSymbol() as IEntity;
            if (symbol == null && identifier.Parent is VariableInitializer)
                symbol = identifier.Parent.Parent?.GetSymbol() as IEntity;
            bool match = declaration && symbol?.MetadataToken == wanted.MetadataToken && symbol.ParentModule?.MetadataFile?.FileName == wanted.ParentModule?.MetadataFile?.FileName;
            var start = Location;
            base.WriteIdentifier(identifier);
            var end = Location;
            if (match)
            {
                if (Range != null) throw new InvalidOperationException("Declaration range is ambiguous");
                Range = new { start = new { line = start.Line, character = start.Column - 1 }, end = new { line = end.Line, character = end.Column - 1 } };
            }
        }
    }
    sealed class BoundedWriter(int limit) : StringWriter(System.Globalization.CultureInfo.InvariantCulture)
    {
        public override void Write(char value) {if(GetStringBuilder().Length>=limit)throw new InvalidOperationException("Decompiled document exceeds output budget");base.Write(value);}
        public override void Write(string? value) {if(GetStringBuilder().Length+(value?.Length??0)>limit)throw new InvalidOperationException("Decompiled document exceeds output budget");base.Write(value);}
        public override void Write(char[] buffer,int index,int count) {if(GetStringBuilder().Length+count>limit)throw new InvalidOperationException("Decompiled document exceeds output budget");base.Write(buffer,index,count);}
    }
    sealed class Inputs(CancellationToken cancellation) : IDisposable
    {
        readonly ConcurrentDictionary<string,string> hashes = new(StringComparer.Ordinal);
        readonly ConcurrentDictionary<(string,bool),Lazy<PEFile>> modules = new();
        public IEnumerable<string> Paths => hashes.Keys;
        public void Add(string path,bool allowMissing=false)
        {
            cancellation.ThrowIfCancellationRequested();path=Path.GetFullPath(path);
            hashes.GetOrAdd(path,p=>{
                if(!File.Exists(p)){if(allowMissing)return "missing";throw new FileNotFoundException("Required input is missing",p);}
                if(new FileInfo(p).Length>MaxAssembly)throw new InvalidOperationException("Metadata input exceeds size budget");
                using var stream=File.OpenRead(p);return Convert.ToHexStringLower(SHA256.HashData(stream));
            });
        }
        public PEFile Open(string path,bool body=false)
        {
            path=Path.GetFullPath(path);Add(path);
            if(modules.TryGetValue((path,true),out var complete))return complete.Value;
            return modules.GetOrAdd((path,body),p=>new Lazy<PEFile>(()=>new PEFile(p.Item1,p.Item2?PEStreamOptions.PrefetchEntireImage:PEStreamOptions.PrefetchMetadata,MetadataReaderOptions.None))).Value;
        }
        public object[] Certificate() => hashes.OrderBy(x=>x.Key,StringComparer.Ordinal).Select(x=>{
            cancellation.ThrowIfCancellationRequested();string current="missing";
            if(File.Exists(x.Key)){using var s=File.OpenRead(x.Key);current=Convert.ToHexStringLower(SHA256.HashData(s));}
            if(current!=x.Value)throw new InvalidOperationException("Input changed while decompiling");
            return (object)new{path=x.Key,sha256=x.Value};
        }).ToArray();
        public void Dispose(){foreach(var file in modules.Values)if(file.IsValueCreated)file.Value.Dispose();}
    }
    sealed class TrackingResolver : IAssemblyResolver
    {
        readonly UniversalAssemblyResolver resolver; readonly Inputs files; readonly string directory;
        public TrackingResolver(MetadataFile origin, Inputs files)
        {
            this.files=files;directory=Path.GetDirectoryName(origin.FileName)!;resolver=new UniversalAssemblyResolver(origin.FileName,true,origin.DetectTargetFrameworkId(),origin.DetectRuntimePack(),PEStreamOptions.PrefetchMetadata,MetadataReaderOptions.None);
            resolver.AddSearchDirectory(directory);
            resolver.AddSearchDirectory(Path.GetDirectoryName(typeof(object).Assembly.Location)!);
        }
        public MetadataFile? Resolve(IAssemblyReference name){files.Add(Path.Combine(directory,name.Name+".dll"),allowMissing:true);files.Add(Path.Combine(directory,name.Name+".exe"),allowMissing:true);string? path=resolver.FindAssemblyFile(name);return path==null?null:files.Open(path);}
        public IDisposable? BeginSnapshot()=>resolver.BeginSnapshot();
        public MetadataFile? ResolveModule(MetadataFile mainModule,string name){if(Path.GetFileName(name)!=name)throw new InvalidOperationException("Invalid module path");return files.Open(Path.Combine(Path.GetDirectoryName(mainModule.FileName)!,name));}
        public Task<MetadataFile?> ResolveAsync(IAssemblyReference name)=>Task.FromResult(Resolve(name));
        public Task<MetadataFile?> ResolveModuleAsync(MetadataFile module,string name)=>Task.FromResult(ResolveModule(module,name));
    }
}
