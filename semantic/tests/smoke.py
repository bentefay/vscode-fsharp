#!/usr/bin/env python3
"""Real SDK/helper integration fixtures. Uses no third-party Python dependencies."""
import json, os, pathlib, subprocess, tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
DOTNET = pathlib.Path(os.environ.get("FSHARP_NAVIGATOR_DOTNET", ROOT / ".tools/dotnet/dotnet")).resolve()
HELPER = pathlib.Path(os.environ.get("FSHARP_ENV_HELPER", ROOT / "semantic/bin/Release/net10.0/FSharpNavigator.Semantic.dll"))
ENV = dict(os.environ, DOTNET_ROOT=str(DOTNET.parent), DOTNET_NOLOGO="1", DOTNET_CLI_TELEMETRY_OPTOUT="1", MSBUILDDISABLENODEREUSE="1", DOTNET_CLI_USE_MSBUILD_SERVER="0")


def run(*args, cwd=ROOT):
    p = subprocess.run([str(DOTNET), *map(str, args)], cwd=cwd, env=ENV, text=True, capture_output=True, timeout=120)
    assert p.returncode == 0, p.stderr + p.stdout
    return p.stdout


def request(method, parameters):
    p = subprocess.Popen([str(DOTNET), str(HELPER), "--once"], env=ENV, text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    owned = {p.pid: None}
    def process_states():
        states = {}
        for entry in pathlib.Path("/proc").iterdir():
            if entry.name.isdigit():
                try:
                    fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
                    states[int(entry.name)] = (int(fields[1]), fields[0], fields[19])
                except (FileNotFoundError, ProcessLookupError, PermissionError):
                    pass
        return states
    payload = json.dumps({"id": 42, "method": method, "params": parameters}) + "\n"
    for _ in range(1200):
        states = process_states()
        for _ in range(4):
            for pid, (parent, state, started) in states.items():
                if parent in owned or pid in owned:
                    owned[pid] = started
        try:
            stdout, stderr = p.communicate(payload, timeout=0.1)
            break
        except subprocess.TimeoutExpired:
            payload = None
    else:
        p.kill()
        raise AssertionError("Helper timed out")
    states = process_states()
    leaked = [pid for pid, start in owned.items() if pid in states and states[pid][2] == start and states[pid][1] != "Z"]
    assert not leaked, f"Owned helper descendants remained alive: {leaked}"
    assert p.returncode == 0, stderr
    lines = stdout.splitlines()
    assert len(lines) == 1, stdout
    response = json.loads(lines[0])
    assert response["id"] == 42, response
    assert "error" not in response, response.get("error")
    return response["result"]


def project(path, sources, extra=""):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('''<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DisableImplicitFSharpCoreReference>true</DisableImplicitFSharpCoreReference><GenerateDocumentationFile>true</GenerateDocumentationFile><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="FSharp.Core"><HintPath>$(MSBuildToolsPath)/FSharp/FSharp.Core.dll</HintPath></Reference>''' + sources + extra + '</ItemGroup></Project>')


def main():
    run("build", ROOT / "semantic/FSharpNavigator.Semantic.fsproj", "-c", "Release", "--nologo", "-v:q")
    with tempfile.TemporaryDirectory(prefix="fsharp-navigator-semantic-") as tmp:
        base = pathlib.Path(tmp)
        external = base / "external"
        external.mkdir()
        (external / "Library.fs").write_text('''namespace Sample\n/// Public library arithmetic.\nmodule Library =\n    /// Doubles a supplied integer.\n    /// <param name="value">The input integer.</param>\n    let twice (value: int) = value * 2\n    /// Keeps the supplied generic value.\n    let identity value = value\n''')
        library = external / "External.fsproj"
        project(library, '<Compile Include="Library.fs"/>')
        run("build", library, "--nologo", "-v:q", cwd=external)
        assembly = external / "bin/Debug/net10.0/External.dll"
        catalog = request("metadata/export", {"assemblyPaths": [str(assembly)]})
        assert catalog["scope"] == "binary-declarations-only"
        twice = [s for s in catalog["symbols"] if "Sample.Library.twice" in s["qualifiedNames"]]
        assert len(twice) == 1, catalog
        assert twice[0]["signature"] == "val twice : Microsoft.FSharp.Core.int -> Microsoft.FSharp.Core.int", twice
        assert "Doubles a supplied integer." in twice[0]["documentation"], twice
        assert "input integer" in twice[0]["documentation"], twice
        print("PASS external binary F# declaration signature, qualified identity and XML docs")
        artifact_path = base / "catalog.json"
        artifact = request("metadata/export", {"assemblyPaths": [str(assembly)], "outputPath": str(artifact_path)})
        serialized = json.loads(artifact_path.read_text())
        assert artifact["symbolCount"] == len(serialized["symbols"]) and serialized["symbols"] == catalog["symbols"], artifact
        assert serialized["inputHashes"] == artifact["inputHashes"] and not list(base.glob("*.partial")), artifact
        print("PASS streaming catalog artifact with compact protocol response")
        core_paths = [DOTNET.parent / "sdk/10.0.400/FSharp/FSharp.Core.dll"]
        package_core = pathlib.Path.home() / ".nuget/packages/fsharp.core/10.1.400/lib/netstandard2.1/FSharp.Core.dll"
        if package_core.exists():
            core_paths.append(package_core)
        for core_path in core_paths:
            core_catalog = request("metadata/export", {"assemblyPaths": [str(core_path)]})
            maps = [s for s in core_catalog["symbols"] if s["fullName"] == "Microsoft.FSharp.Collections.List.map"]
            assert len(maps) == 1 and maps[0]["documentation"] and maps[0]["assemblyPath"] == str(core_path), maps
        print("PASS exact selected SDK/NuGet FSharp.Core catalog identity and library comments")
        workspace = base / "workspace"
        workspace.mkdir()
        (base / "Linked.fs").write_text("module Linked\nlet linked = 3\n")
        (workspace / "First.fs").write_text("module First\nlet value = 41\ntype Widget() = class end\n")
        (workspace / "Last.fs").write_text("module Last\nlet value = First.value + Linked.linked\nlet doubled = Sample.Library.twice value\nlet item : First.Widget = First.Widget()\nlet builder : System.Text.StringBuilder = System.Text.StringBuilder()\n")
        main_project = workspace / "Main.fsproj"
        project(main_project, '<Compile Include="First.fs"><Link>A/First.fs</Link></Compile><Compile Include="../Linked.fs"><Link>B/Linked.fs</Link></Compile><Compile Include="Last.fs"><Link>A/Last.fs</Link></Compile>', f'<Reference Include="External"><HintPath>{assembly}</HintPath></Reference>')
        run("build", main_project, "--nologo", "-v:q", cwd=workspace)
        params = {"root": str(workspace)}
        model = request("workspace/evaluate", params)
        p = model["projects"][0]
        user_items = [i for i in p["items"] if "obj/" not in i["path"]]
        assert [i["virtualPath"] for i in user_items] == ["A/First.fs", "B/Linked.fs", "A/Last.fs"], p
        assert [i["path"] for i in user_items] == list(map(str, [workspace / "First.fs", base / "Linked.fs", workspace / "Last.fs"])), p
        assert str(assembly) in p["assemblyReferences"], p
        assert p["compilerArgs"] and p["inputs"], p
        print("PASS final compiler source order, linked ownership and evaluated assembly inputs")
        # Exercise selection and compile-args even after normal build made outputs current.
        solution = workspace / "Test.sln"
        solution.write_text('Microsoft Visual Studio Solution File, Format Version 12.00\nProject("{F2A71F9B-5D33-465A-A702-920D77279786}") = "Main", "Main.fsproj", "{00000000-0000-0000-0000-000000000001}"\nEndProject\n')
        assert len(request("workspace/evaluate", dict(params, solution=str(solution)))["projects"]) == 1
        print("PASS explicit solution selection")
        cursor = dict(params, path=str(workspace / "Last.fs"), line=1, character=20, documents=[])
        definition = request("navigation/definition", cursor)
        assert definition["completeness"] == "exact" and definition["locations"][0]["path"] == str(workspace / "First.fs"), definition
        hover = request("navigation/hover", cursor)
        assert hover["hover"] is None, hover
        linked_hover = request("navigation/hover", dict(cursor, character=35))
        assert linked_hover["hover"] is None, linked_hover
        refs = request("navigation/references", cursor)
        assert refs["completeness"] == "exact" and len(refs["locations"]) == 2, refs
        print("PASS explicit oracle definition/references and repository/linked-source hover suppression")
        external_hover = request("navigation/hover", dict(cursor, line=2, character=32))
        assert external_hover["hover"]["isExternal"] and "Doubles" in external_hover["hover"]["markdown"], external_hover
        print("PASS explicit oracle external library hover")
        overlays = [{"path": str(workspace / "First.fs"), "version": 7, "text": "module First\nlet changed = 99\n"}, {"path": str(workspace / "Last.fs"), "version": 8, "text": "module Last\nlet value = First.changed + Linked.linked\n"}]
        dirty = request("navigation/references", dict(cursor, character=20, documents=overlays))
        assert dirty["completeness"] == "exact" and len(dirty["locations"]) == 2, dirty
        print("PASS atomic complete dirty overlay set across compiler inputs")
        snapshot = request("semantic/export", params)
        assert snapshot["completeness"] == "exact", snapshot
        exported = snapshot["projects"][0]
        assert exported["sourceHashes"] and exported["inputHashes"], exported
        value_uses = [o for o in exported["occurrences"] if o["declaration"] and o["declaration"]["path"] == str(workspace / "First.fs") and o["declaration"]["range"]["start"]["line"] == 1]
        assert len(value_uses) == 2 and len({o["symbolId"] for o in value_uses}) == 1, value_uses
        assert sum(o["isDefinition"] for o in value_uses) == 1, value_uses
        print("PASS explicit clean semantic snapshot identity, occurrences and SHA256 input stamps")
        for line, character in [(3, 20), (4, 31)]:
            matching = [o for o in exported["occurrences"] if o["path"] == str(workspace / "Last.fs") and o["range"]["start"]["line"] == line and o["range"]["start"]["character"] <= character < o["range"]["end"]["character"]]
            selected = min(matching, key=lambda o: o["range"]["end"]["character"] - o["range"]["start"]["character"])
            snapshot_uses = {json.dumps({"path": o["path"], "range": o["range"]}, sort_keys=True) for o in exported["occurrences"] if o["symbolId"] == selected["symbolId"]}
            oracle = request("navigation/references", dict(cursor, line=line, character=character))
            oracle_uses = {json.dumps(o, sort_keys=True) for o in oracle["locations"]}
            assert snapshot_uses == oracle_uses, (snapshot_uses, oracle_uses)
        print("PASS source and external type/constructor FCS equivalence groups")
        consumer = workspace / "consumer"
        consumer.mkdir()
        (consumer / "Consumer.fs").write_text("module Consumer\nlet useValue = First.value\n")
        consumer_project = consumer / "Consumer.fsproj"
        project(consumer_project, '<Compile Include="Consumer.fs"/>', '<ProjectReference Include="../Main.fsproj"/>')
        run("build", consumer_project, "--nologo", "-v:q", "-p:UseSharedCompilation=false", "-nodeReuse:false", cwd=consumer)
        cross_model = request("workspace/evaluate", params)
        consumer_model = next(p for p in cross_model["projects"] if p["path"] == str(consumer_project))
        assert any(p.endswith("/Main.dll") for p in consumer_model["workspaceAssemblyReferences"]), consumer_model
        cross = request("navigation/references", cursor)
        assert cross["completeness"] == "exact" and len(cross["locations"]) == 3, cross
        cross_overlays = overlays + [{"path": str(consumer / "Consumer.fs"), "version": 9, "text": "module Consumer\nlet useValue = First.changed\n"}]
        cross_dirty = request("navigation/references", dict(cursor, documents=cross_overlays))
        assert cross_dirty["completeness"] == "exact" and len(cross_dirty["locations"]) == 3, cross_dirty
        snapshot = request("semantic/export", params)
        assert snapshot["completeness"] == "exact", snapshot
        uses = [o for p in snapshot["projects"] for o in p["occurrences"] if o["declaration"] and o["declaration"]["path"] == str(workspace / "First.fs") and o["declaration"]["range"]["start"]["line"] == 1]
        assert len(uses) == 3 and len({o["symbolId"] for o in uses}) == 1, uses
        print("PASS cross-project oracle references and persisted declaration identity")
        (consumer / "Consumer.fs").write_text("module Consumer\nlet broken = DoesNotExist.value\n")
        broken = request("semantic/export", params)
        assert broken["completeness"] == "unavailable", broken
        broken_project = next(p for p in broken["projects"] if p["projectPath"] == str(consumer_project))
        assert broken_project["completeness"] == "unavailable" and broken_project["diagnostics"] and not broken_project["occurrences"], broken_project
        print("PASS compiler errors prevent exact semantic coverage")
        print("PASS one-shot helper and owned MSBuild process lifecycle on all requests")
    print("All helper integration checks passed")

if __name__ == "__main__":
    main()
