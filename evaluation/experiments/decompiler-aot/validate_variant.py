#!/usr/bin/env python3
"""Reuse the production 14-case suite for an isolated same-patch experiment."""
import argparse, hashlib, json, os
from pathlib import Path
root=Path(__file__).resolve().parents[3]
a=argparse.ArgumentParser();a.add_argument('variant',choices=['il','r2r','aot']);args=a.parse_args()
original=root/'semantic/Decompiler/test.py'
src=original.read_text().replace("runtime=DOTNET.parent/", "runtime=ROOT/'.tools/dotnet'/").replace("reference=DOTNET.parent/", "reference=ROOT/'.tools/dotnet'/")
src=src.replace("[str(DOTNET), str(DLL), '--once']", "COMMAND")
ns={'__file__':str(original),'__name__':'decompiler_test_variant'}
exec(compile(src,str(original),'exec'),ns)
runtime=root/'.cache/decompiler-runtime-10.0.14';variant=root/'.cache/decompiler-patched'/args.variant
publish=variant/'bin/Release/net10.0/linux-x64/publish';binary=publish/('FSharpNavigator.Decompiler' if args.variant=='aot' else 'FSharpNavigator.Decompiler.dll')
ns.update(DOTNET=runtime/'dotnet',DLL=binary,REPORT=Path(__file__).parent/(args.variant+'-validation.json'),ENV=os.environ|{'DOTNET_ROOT':str(runtime),'DOTNET_GCHeapHardLimit':'0x8000000','DOTNET_CLI_TELEMETRY_OPTOUT':'1'},COMMAND=([str(binary),'--once'] if args.variant=='aot' else [str(runtime/'dotnet'),str(binary),'--once']))
ns['main']()
p=ns['REPORT'];report=json.loads(p.read_text());report.update(kind='isolated-10.0.14-'+args.variant+'-validation',source_sha256=hashlib.sha256((variant/'Program.cs').read_bytes()).hexdigest(),test_source_sha256=hashlib.sha256(original.read_bytes()).hexdigest(),target_corpus_runtime='10.0.11 (same fixed input assemblies for all variants)',helper_runtime='10.0.14',source_changes='Explicit JsonNode serialization, explicit metadata runtime search, executable hash in certificate; production unchanged')
p.write_text(json.dumps(report,indent=2)+'\n')
