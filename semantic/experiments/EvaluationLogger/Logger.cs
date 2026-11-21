using Microsoft.Build.Framework;
using System.Collections.Concurrent;
using System.Text.Json;
public sealed class ConsumptionLogger : ILogger {
    public LoggerVerbosity Verbosity { get; set; } = LoggerVerbosity.Diagnostic;
    public string? Parameters { get; set; }
    readonly ConcurrentDictionary<string,string> names = new(StringComparer.OrdinalIgnoreCase);
    readonly ConcurrentDictionary<string,int> kinds = new();
    readonly ConcurrentDictionary<string,bool> imports = new();
    readonly ConcurrentBag<object> importEvents = new();
    readonly ConcurrentDictionary<string,string> tracked = new();
    readonly ConcurrentDictionary<string,string[]> globals = new();
    public void Initialize(IEventSource source) {
        File.WriteAllText(Parameters+".started","initialized");
        if(source is IEventSource4 detailed) detailed.IncludeEvaluationPropertiesAndItems();
        source.AnyEventRaised += (_, ev) => {
            var kind=ev.GetType().Name;
            kinds.AddOrUpdate(kind,1,(_,n)=>n+1);
            if(ev is PropertyInitialValueSetEventArgs set && new[]{"MSBuildProjectExtensionsPath","MSBuildProjectFile","MSBuildToolsVersion","MSBuildExtensionsPath","MSBuildUserExtensionsPath"}.Contains(set.PropertyName)) tracked[set.PropertyName]=set.PropertyValue;
            if (ev is EnvironmentVariableReadEventArgs env) names[env.EnvironmentVariableName]="environment";
            if (ev is UninitializedPropertyReadEventArgs missing) names.TryAdd(missing.PropertyName,"missing");
            if(ev is ProjectImportedEventArgs import) { importEvents.Add(new {import.ImportedProjectFile,import.UnexpandedProject,import.File,import.ProjectFile,import.ImportIgnored,import.Message}); if(!string.IsNullOrEmpty(import.ImportedProjectFile)) imports[import.ImportedProjectFile]=import.ImportIgnored; }
            if(ev is ProjectEvaluationFinishedEventArgs evaluation) {
                var keys=new List<string>();
                if(evaluation.GlobalProperties != null) foreach(var entry in evaluation.GlobalProperties) {
                    if(entry is System.Collections.DictionaryEntry de) keys.Add(de.Key.ToString()!);
                    else { var key=entry.GetType().GetProperty("Key") ?? entry.GetType().GetProperty("Name"); if(key!=null) keys.Add(key.GetValue(entry)?.ToString() ?? ""); }
                }
                globals[(evaluation.ProjectFile??"")+":"+evaluation.BuildEventContext?.EvaluationId]=keys.ToArray();
            }
            if (ev is ProjectEvaluationFinishedEventArgs || ev is BuildFinishedEventArgs) Save();
        };
    }
    void Save() => File.WriteAllText(Parameters!,JsonSerializer.Serialize(new {names,kinds,imports,globals,importEvents,tracked}));
    public void Shutdown() => Save();
}
