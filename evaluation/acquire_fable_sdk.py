#!/usr/bin/env python3
"""Install the unchanged Fable global.json SDK from official pinned release metadata."""
import hashlib,json,os,subprocess,time,urllib.request
from pathlib import Path
from run import file_sha
root=Path(__file__).resolve().parents[1];meta=root/'.tools/downloads/dotnet-releases.json';j=json.loads(meta.read_text());files=[f for r in j['releases'] for sdk in r.get('sdks',[r.get('sdk',{})]) if sdk.get('version')=='10.0.100' for f in sdk.get('files',[]) if f.get('rid')=='linux-x64' and f.get('name','').endswith('.tar.gz')];assert len(files)==1;selected=files[0];archive=root/'.tools/downloads/dotnet-sdk-10.0.100-linux-x64.tar.gz';destination=root/'.tools/dotnet-sdk100';start=time.perf_counter();downloaded=False
if not archive.exists():
 temp=archive.with_suffix(archive.suffix+'.part')
 with urllib.request.urlopen(selected['url'],timeout=30) as response,temp.open('wb') as output:
  while block:=response.read(1024*1024):output.write(block)
 temp.rename(archive);downloaded=True
with archive.open('rb') as f:actual=hashlib.file_digest(f,'sha512').hexdigest()
assert actual.lower()==selected['hash'].lower(),'Official SDK archive hash mismatch'
verified=time.perf_counter();destination.mkdir(parents=True,exist_ok=True)
if not (destination/'sdk/10.0.100/dotnet.dll').is_file():subprocess.run(['tar','-xzf',str(archive),'-C',str(destination)],check=True,timeout=60)
extracted=time.perf_counter();repo=root.parent/'research/corpus/Fable';command=[str(destination/'dotnet'),'--version'];env=dict(os.environ,DOTNET_ROOT=str(destination));probe=subprocess.run(command,cwd=repo,env=env,capture_output=True,text=True,timeout=10);assert probe.returncode==0 and probe.stdout.strip()=='10.0.100'
report={'kind':'exact-Fable-SDK-acquisition','repository_global_json':str(repo/'global.json'),'global_json_sha256':file_sha(repo/'global.json'),'metadata':str(meta),'metadata_sha256':file_sha(meta),'selected_download':selected,'archive':str(archive),'archive_bytes':archive.stat().st_size,'archive_sha512':actual,'hash_matches':True,'new_download':downloaded,'download_and_hash_ms':(verified-start)*1000,'extraction_ms':(extracted-verified)*1000,'installed_dotnet':str(destination/'dotnet'),'dotnet_sha256':file_sha(destination/'dotnet'),'sdk_probe':{'command':command,'cwd':str(repo),'exit_code':probe.returncode,'stdout':probe.stdout,'stderr':probe.stderr},'notes':['Project global.json/latestPatch and sources are unchanged. Installed exactly10.0.100, not existing10.0.400.','Tool acquisition is setup, not semantic readiness or quiet benchmark performance.']};out=root/'evaluation/results/fable-sdk100-install.json';out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report),flush=True)
