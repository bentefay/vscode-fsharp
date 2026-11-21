using Microsoft.Build.Framework;
using System.Collections;
using System.Collections.Concurrent;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Xml.Linq;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

// Runs only inside explicit MSBuild preparation. No environment values or
// messages are serialized: the caller hashes selected inherited values.
public sealed class ConsumptionLogger : ILogger {
    public LoggerVerbosity Verbosity { get; set; } = LoggerVerbosity.Diagnostic;
    public string? Parameters { get; set; }
    readonly ConcurrentDictionary<string, bool> reads = new(StringComparer.OrdinalIgnoreCase);
    readonly ConcurrentDictionary<string, bool> imports = new(StringComparer.Ordinal);
    readonly ConcurrentDictionary<string, string[]> globals = new();
    readonly ConcurrentDictionary<int, Dictionary<string,string>> properties = new();
    readonly ConcurrentDictionary<string, bool> patterns = new(StringComparer.Ordinal);
    readonly ConcurrentDictionary<string, bool> probes = new(StringComparer.Ordinal);
    readonly ConcurrentDictionary<string, bool> problems = new(StringComparer.Ordinal);
    readonly ConcurrentDictionary<string,XDocument> xml = new(StringComparer.Ordinal);
    readonly ConcurrentDictionary<string,bool> globalization = new(StringComparer.Ordinal);
    readonly ConcurrentDictionary<string,string> loadedFileHashes = new(StringComparer.Ordinal);
    int evaluations, environmentReads, missingReads, globalizationObservationCount;
    [DllImport("libc", EntryPoint="setlocale")]
    static extern IntPtr QueryLocale(int category, IntPtr locale);
    static bool IsGlobalizationModule(string path) {
        var name=Path.GetFileName(path);
        return name.StartsWith("libicu",StringComparison.Ordinal) || name.StartsWith("libSystem.Globalization.Native",StringComparison.Ordinal)
            || name.StartsWith("libcoreclr",StringComparison.Ordinal) || name.StartsWith("libhostfxr",StringComparison.Ordinal)
            || name.StartsWith("libhostpolicy",StringComparison.Ordinal) || name.StartsWith("libc.so",StringComparison.Ordinal)
            || name.StartsWith("ld-linux",StringComparison.Ordinal);
    }
    void ObserveGlobalization() {
        try {
            // Observe the actual MSBuild process. Do not force invariant culture
            // or record inherited environment values in the private trace.
            var culture=CultureInfo.CurrentCulture;
            var libcMessagesLocale=OperatingSystem.IsLinux() ? Marshal.PtrToStringAnsi(QueryLocale(5,IntPtr.Zero)) ?? throw new IOException("Cannot observe libc LC_MESSAGES") : "unsupported";
            var version=culture.CompareInfo.Version;
            var files=new HashSet<string>(StringComparer.Ordinal) { typeof(CultureInfo).Assembly.Location };
            if(Environment.ProcessPath is string host) files.Add(host); else throw new IOException("Actual MSBuild host path is unavailable");
            using var process=Process.GetCurrentProcess();
            foreach(ProcessModule module in process.Modules) {
                var file=module.FileName;
                if(IsGlobalizationModule(file)) files.Add(Path.GetFullPath(file));
            }
            if(OperatingSystem.IsLinux()) foreach(var line in File.ReadLines("/proc/self/maps")) {
                var parts=line.Split((char[]?)null,6,StringSplitOptions.RemoveEmptyEntries);
                if(parts.Length!=6 || !parts[5].StartsWith('/')) continue;
                var file=Regex.Replace(parts[5],@"\\([0-7]{3})",match=>((char)Convert.ToInt32(match.Groups[1].Value,8)).ToString());
                if(!IsGlobalizationModule(file))continue;
                if(file.EndsWith(" (deleted)",StringComparison.Ordinal))throw new IOException("Loaded globalization mapping was deleted");
                files.Add(Path.GetFullPath(file));
            }
            var runtimeDirectory=RuntimeEnvironment.GetRuntimeDirectory();
            var sdkDirectory=Path.GetDirectoryName(typeof(IBuildEngine).Assembly.Location)!;
            foreach(var file in new[]{Path.Combine(runtimeDirectory,".version"),Path.Combine(runtimeDirectory,"Microsoft.NETCore.App.deps.json"),Path.Combine(sdkDirectory,"MSBuild.runtimeconfig.json"),Path.Combine(sdkDirectory,"MSBuild.deps.json")}) {
                if(!File.Exists(file)) throw new IOException("Required globalization/runtime selection input is missing");
                files.Add(file);
            }
            foreach(var file in files) loadedFileHashes.GetOrAdd(file,path=>{using var stream=File.OpenRead(path);return Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();});
            var value=new {cultureName=culture.Name,uiCultureName=CultureInfo.CurrentUICulture.Name,libcMessagesLocale,
                sortVersion=version.FullVersion,sortId=version.SortId.ToString(),
                runtimeVersion=Environment.Version.ToString(),runtimeDirectory=RuntimeEnvironment.GetRuntimeDirectory(),
                processPath=Environment.ProcessPath,loadedFiles=files.Order(StringComparer.Ordinal).ToArray()};
            globalization[JsonSerializer.Serialize(value)]=true;
            Interlocked.Increment(ref globalizationObservationCount);
        } catch(Exception error) { problems["Cannot certify actual MSBuild globalization context: "+error.GetType().Name]=true; }
    }
    Dictionary<string,string> Values(BuildEventArgs ev) => properties.GetOrAdd(ev.BuildEventContext?.EvaluationId ?? -1, _ => {
        var result = new Dictionary<string,string>(StringComparer.OrdinalIgnoreCase);
        foreach(DictionaryEntry pair in Environment.GetEnvironmentVariables()) result[pair.Key.ToString()!] = pair.Value?.ToString() ?? "";
        return result;
    });
    string? Expand(string expression, string definingFile, Dictionary<string,string> values) {
        var result = Regex.Replace(expression, @"\$\(([A-Za-z_][A-Za-z0-9_-]*)\)", match => {
            var name=match.Groups[1].Value;
            if(name.Equals("MSBuildThisFile",StringComparison.OrdinalIgnoreCase)) return Path.GetFileName(definingFile);
            if(name.Equals("MSBuildThisFileName",StringComparison.OrdinalIgnoreCase)) return Path.GetFileNameWithoutExtension(definingFile);
            if(name.Equals("MSBuildThisFileExtension",StringComparison.OrdinalIgnoreCase)) return Path.GetExtension(definingFile);
            if(name.Equals("MSBuildThisFileDirectoryNoRoot",StringComparison.OrdinalIgnoreCase)) return (Path.GetDirectoryName(definingFile)+Path.DirectorySeparatorChar).Substring(Path.GetPathRoot(definingFile)!.Length);
            if(name.Equals("MSBuildThisFileDirectory",StringComparison.OrdinalIgnoreCase)) return Path.GetDirectoryName(definingFile)+Path.DirectorySeparatorChar;
            if(name.Equals("MSBuildThisFileFullPath",StringComparison.OrdinalIgnoreCase)) return definingFile;
            return values.TryGetValue(name,out var value) ? value : "";
        });
        if(result.Contains("$(") || result.Contains("@(") || result.Contains("%(")) return null;
        return result;
    }
    void TrackImport(ProjectImportedEventArgs imported) {
        if(string.IsNullOrEmpty(imported.ProjectFile) || string.IsNullOrEmpty(imported.UnexpandedProject)) return;
        var definingFile=Path.GetFullPath(imported.ProjectFile);
        var values=Values(imported);
        var expression=Expand(imported.UnexpandedProject,definingFile,values);
        if(expression==null) { problems["Unresolved import-domain expression in "+definingFile]=true; return; }
        foreach(var raw in expression.Split(';',StringSplitOptions.RemoveEmptyEntries)) {
            var path=Uri.UnescapeDataString(raw).Replace('\\',Path.DirectorySeparatorChar);
            if(path.Length==0) continue;
            path=Path.GetFullPath(path,Path.GetDirectoryName(definingFile)!);
            if(path.IndexOfAny(['*','?'])>=0) patterns[path]=true;
            else probes[path]=true;
        }
        // Negative Exists guards matter even for trusted SDK imports. Parse only
        // path-valued expressions; unsupported dynamic expressions fail closed.
        try {
            var document=xml.GetOrAdd(definingFile,XDocument.Load);
            foreach(var element in document.Descendants().Where(e=>e.Name.LocalName=="Import" && (string?)e.Attribute("Project")==imported.UnexpandedProject)) {
                var condition=(string?)element.Attribute("Condition") ?? "";
                foreach(Match match in Regex.Matches(condition, """(?i)Exists\s*\(\s*['"]([^'"]+)['"]""")) {
                    var expanded=Expand(match.Groups[1].Value,definingFile,values);
                    if(expanded==null || expanded.IndexOfAny(['*','?'])>=0) { problems["Unresolved import existence probe in "+definingFile]=true; continue; }
                    var path=Uri.UnescapeDataString(expanded).Replace('\\',Path.DirectorySeparatorChar);
                    if(path.Length>0) probes[Path.GetFullPath(path,Path.GetDirectoryName(definingFile)!)]=true;
                }
            }
        } catch(Exception e) { problems["Cannot certify import conditions in "+definingFile+": "+e.GetType().Name]=true; }
    }
    public void Initialize(IEventSource source) {
        if (string.IsNullOrWhiteSpace(Parameters)) throw new LoggerException("Missing private trace output path");
        if (source is not IEventSource4 detailed) throw new LoggerException("MSBuild evaluation event API unavailable");
        detailed.IncludeEvaluationPropertiesAndItems();
        ObserveGlobalization();
        source.AnyEventRaised += (_, ev) => {
            if(ev is PropertyInitialValueSetEventArgs initial) Values(ev)[initial.PropertyName]=initial.PropertyValue;
            if(ev is PropertyReassignmentEventArgs reassigned) Values(ev)[reassigned.PropertyName]=reassigned.NewValue;
            if(ev is ProjectEvaluationStartedEventArgs started) {
                ObserveGlobalization();
                Values(ev)["MSBuildProjectDirectory"]=Path.GetDirectoryName(started.ProjectFile)!;
                Values(ev)["MSBuildProjectFile"]=Path.GetFileName(started.ProjectFile) ?? "";
                Values(ev)["MSBuildProjectFullPath"]=started.ProjectFile ?? "";
            }
            if(ev is ProjectImportedEventArgs importEvent) TrackImport(importEvent);
            if (ev is EnvironmentVariableReadEventArgs env) {
                reads[env.EnvironmentVariableName] = true;
                Interlocked.Increment(ref environmentReads);
            }
            if (ev is UninitializedPropertyReadEventArgs missing) {
                reads[missing.PropertyName] = true;
                Interlocked.Increment(ref missingReads);
            }
            if (ev is ProjectImportedEventArgs imported && Path.IsPathRooted(imported.ImportedProjectFile))
                imports[Path.GetFullPath(imported.ImportedProjectFile)] = true;
            if (ev is ProjectEvaluationFinishedEventArgs evaluation) {
                ObserveGlobalization();
                var keys = new List<string>();
                if (evaluation.GlobalProperties != null) foreach (var entry in evaluation.GlobalProperties) {
                    if (entry is DictionaryEntry de) keys.Add(de.Key.ToString()!);
                    else {
                        var key = entry.GetType().GetProperty("Key") ?? entry.GetType().GetProperty("Name");
                        if (key != null && key.GetValue(entry)?.ToString() is string name) keys.Add(name);
                    }
                }
                globals[(evaluation.ProjectFile ?? "") + ":" + evaluation.BuildEventContext?.EvaluationId] = keys.ToArray();
                Interlocked.Increment(ref evaluations);
            }
        };
    }
    public void Shutdown() {
        ObserveGlobalization();
        foreach(var entry in loadedFileHashes) {
            try { using var stream=File.OpenRead(entry.Key);if(Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant()!=entry.Value)problems["Runtime/globalization input changed during evaluation: "+entry.Key]=true; }
            catch(Exception error){problems["Cannot revalidate runtime/globalization input: "+error.GetType().Name]=true;}
        }
        var intersection = new HashSet<string>(globals.Values.FirstOrDefault() ?? [], StringComparer.OrdinalIgnoreCase);
        foreach (var names in globals.Values) intersection.IntersectWith(names);
        var output = new { schemaVersion = 1, evaluations, environmentReads, missingReads,
            names = reads.Keys.Order(StringComparer.Ordinal).ToArray(),
            imports = imports.Keys.Order(StringComparer.Ordinal).ToArray(),
            importPatterns = patterns.Keys.Order(StringComparer.Ordinal).ToArray(),
            existenceProbes = probes.Keys.Order(StringComparer.Ordinal).ToArray(),
            diagnostics = problems.Keys.Order(StringComparer.Ordinal).ToArray(),
            globalizationObservations = globalization.Keys.Order(StringComparer.Ordinal).Select(value=>JsonSerializer.Deserialize<JsonElement>(value)).ToArray(),
            globalizationObservationCount,
            loadedFileHashes = loadedFileHashes.OrderBy(entry=>entry.Key,StringComparer.Ordinal).Select(entry=>new{path=entry.Key,sha256=entry.Value}).ToArray(),
            fixedGlobals = intersection.Order(StringComparer.Ordinal).ToArray() };
        var temporary = Parameters + ".partial";
        File.WriteAllText(temporary, JsonSerializer.Serialize(output));
        File.Move(temporary, Parameters!, true);
    }
}
