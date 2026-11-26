#!/usr/bin/env python3
"""Real assembly decompilation and provenance tests; no mocked ILSpy outputs."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'evaluation'))
from run import process_tree_details, process_identity

DOTNET = Path(os.environ.get('FSHARP_NAVIGATOR_DOTNET') or shutil.which('dotnet') or '').resolve()
DLL = Path(os.environ.get('FSHARP_NAVIGATOR_DECOMPILER_TEST_DLL', ROOT / 'bin/decompiler/il/FSharpNavigator.Decompiler.dll'))
REPORT = Path(os.environ.get('FSHARP_NAVIGATOR_DECOMPILER_TEST_REPORT', ROOT / '.cache/decompiler-helper.json'))
ENV = os.environ | {'DOTNET_ROOT': str(DOTNET.parent), 'DOTNET_GCHeapHardLimit': '0x8000000', 'DOTNET_CLI_TELEMETRY_OPTOUT': '1'}
rows = []

def invoke(path, symbol, expected=None, body=None, request=None):
    payload = json.dumps(request or {'assemblyPath': str(path), 'xmlDocId': symbol}) + '\n'
    begin = time.monotonic()
    process = subprocess.Popen([str(DOTNET), str(DLL), '--once'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=ENV)
    peak = 0
    starttime = process_identity(process.pid)[1]
    retained = {}
    stop = threading.Event()
    def sample():
        nonlocal peak
        while not stop.is_set():
            try: peak = max(peak, process_tree_details(process.pid, starttime, retained)[0])
            except (OSError, ValueError): pass
            stop.wait(.01)
    worker = threading.Thread(target=sample, daemon=True); worker.start()
    try: stdout, stderr = process.communicate(payload, timeout=8)
    except BaseException:
        process.kill(); process.wait(); raise
    finally: stop.set(); worker.join()
    result = json.loads(stdout)
    row = {'symbol': symbol, 'assemblyPath': str(path), 'elapsed_ms': (time.monotonic()-begin)*1000, 'peak_tree_rss_kib': peak, 'result': result}
    if expected:
        assert result['completeness'] == 'exact', result
        assert result['targetKind'] == 'decompiled-implementation', result
        doc, loc = result['documents'][0], result['locations'][0]
        assert doc['uri'] == loc['uri'] and doc['language'] == 'csharp'
        start, end = loc['range']['start'], loc['range']['end']
        assert start['line'] == end['line']
        line = doc['text'].splitlines()[start['line']].encode('utf-16-le')
        assert line[start['character']*2:end['character']*2].decode('utf-16-le') == expected
        if body: assert body in doc['text'], doc['text']
        assert result['symbol']['xmlDocId'] == symbol
        for dependency in result['dependencies']:
            p=Path(dependency['path']); actual=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else 'missing'
            assert dependency['sha256'] == actual, dependency
    else:
        assert result['completeness'] == 'unavailable' and not result['locations'] and not result['documents'], result
    rows.append(row)
    return result

def build_fixture(directory, name, code):
    directory.mkdir(parents=True)
    (directory/'Fixture.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><AssemblyName>'+name+'</AssemblyName><ProduceReferenceAssembly>true</ProduceReferenceAssembly></PropertyGroup></Project>')
    (directory/'Fixture.cs').write_text(code)
    subprocess.run([str(DOTNET),'build',str(directory/'Fixture.csproj'),'-c','Release','--nologo','-v:q'],env=ENV,check=True,capture_output=True,text=True,timeout=60)
    return directory/'bin/Release/net10.0'/f'{name}.dll', directory/'obj/Release/net10.0/ref'/f'{name}.dll'

def main():
    runtime=DOTNET.parent/'shared/Microsoft.NETCore.App/10.0.15'
    reference=DOTNET.parent/'packs/Microsoft.NETCore.App.Ref/10.0.15/ref/net10.0'
    package=Path.home()/'.nuget/packages/icsharpcode.decompiler/11.1.0.9782/lib/netstandard2.0/ICSharpCode.Decompiler.dll'
    invoke(runtime/'System.Private.CoreLib.dll','P:System.String.Length','Length','return _stringLength')
    result=invoke(reference/'System.Runtime.dll','P:System.String.Length','Length','return _stringLength');assert result['referenceAssemblyRedirected']
    invoke(reference/'System.Runtime.dll','P:System.DateTime.Year','Year','GetYear(_dateData)')
    invoke(runtime/'System.Private.CoreLib.dll','P:System.Tuple`1.Item1','Item1','m_Item1')
    invoke(package,'P:ICSharpCode.Decompiler.DecompilerSettings.UsingDeclarations','UsingDeclarations','usingDeclarations = value')
    temp_root=ROOT/'.cache/tmp';temp_root.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='decompiler-tests-',dir=temp_root) as temporary:
        work=Path(temporary)
        assembly,ref=build_fixture(work/'first','Fixture','namespace Test; public class Widget { public int Change=>42; public int Café=>7; public int Field=3; public static int Pick(int x)=>x+1; public static string Pick(string x)=>x+"!"; }')
        invoke(assembly,'M:Test.Widget.Pick(System.Int32)','Pick','x + 1')
        invoke(assembly,'M:Test.Widget.Pick(System.String)','Pick','x + "!"')
        invoke(assembly,'P:Test.Widget.Café','Café','7')
        invoke(assembly,'F:Test.Widget.Field','Field')
        invoke(assembly,'P:Test.Widget.Missing')
        invoke(ref,'P:Test.Widget.Change') # No installed package implementation.
        package_ref=work/'package/ref/net10.0/Fixture.dll';package_lib=work/'package/lib/net10.0/Fixture.dll'
        package_ref.parent.mkdir(parents=True);package_lib.parent.mkdir(parents=True)
        shutil.copy2(ref,package_ref);shutil.copy2(assembly,package_lib)
        result=invoke(package_ref,'P:Test.Widget.Change','Change','42');assert result['referenceAssemblyRedirected']
        mismatch,_=build_fixture(work/'mismatch','Fixture','namespace Test; public class Widget { public string Change=>"different"; }')
        shutil.copy2(mismatch,package_lib)
        result=invoke(package_ref,'P:Test.Widget.Change');assert 'signatures differ' in result['reason']
        large,_=build_fixture(work/'large','Large','namespace Test; public class Huge { public string Value=>"'+('x'*130000)+'"; }')
        result=invoke(large,'P:Test.Huge.Value');assert 'output budget' in result['reason']
    REPORT.parent.mkdir(parents=True,exist_ok=True)
    REPORT.write_text(json.dumps({'kind':'reconstructed-helper-validation','helper_sha256':hashlib.sha256(DLL.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256((ROOT/'semantic/Decompiler/Program.cs').read_bytes()).hexdigest(),'sampling':'10ms process-tree RSS from /proc PPid; managed GC guard128MiB is not a hard native RSS cap','test_count':len(rows),'passed':len(rows),'rows':rows},indent=2)+'\n')
    print(json.dumps({'passed':len(rows),'report':str(REPORT),'peak_tree_rss_kib':max(r['peak_tree_rss_kib'] for r in rows)}))

if __name__=='__main__': main()
