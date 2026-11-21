#!/usr/bin/env python3
"""Actual metadata role certificates; copied producer runtime is disposable.

Run after building the current helper. Native tests independently consume the
retained artifacts; neither this producer test nor a digest authenticates an
untrusted producer implementation.
"""
import hashlib,json,pathlib,shutil,subprocess,tempfile
from smoke import ROOT,DOTNET,ENV,HELPER,project,run

def main():
 root=pathlib.Path(tempfile.mkdtemp(prefix='metadata-provenance-',dir=ROOT/'.cache'))
 runtime=root/'producer';shutil.copytree(HELPER.parent,runtime)
 helper=runtime/HELPER.name;fcs=runtime/'FSharp.Compiler.Service.dll'
 def invoke(method,params):
  process=subprocess.run([str(DOTNET),str(helper),'--once'],env=ENV,input=json.dumps({'id':1,'method':method,'params':params})+'\n',text=True,capture_output=True,timeout=180)
  assert process.returncode==0,process.stderr
  result=json.loads(process.stdout);assert 'error'not in result,result
  return result['result']
 (root/'Library.fs').write_text('namespace Provenance\n/// Binary declaration comment.\nmodule Library =\n    /// Preserves an ordinary value.\n    let keep (value:int) = value\n')
 project(root/'Library.fsproj','<Compile Include="Library.fs"/>');run('build',root/'Library.fsproj','--nologo','-v:q','-m:1','-nodeReuse:false')
 assembly=root/'bin/Debug/net10.0/Library.dll'
 refs=sorted(map(str,(DOTNET.parent/'packs/Microsoft.NETCore.App.Ref/10.0.11/ref/net10.0').glob('*.dll')))+[str(DOTNET.parent/'sdk/10.0.400/FSharp/FSharp.Core.dll'),str(assembly)]
 catalog=root/'catalog.json';response=invoke('metadata/export',{'assemblyPaths':[str(assembly)],'referencePaths':refs,'outputPath':str(catalog)})
 environment=invoke('metadata/environment',{'referencePaths':refs});(root/'environment.json').write_text(json.dumps(environment,separators=(',',':'))+'\n')
 dual_root=root/'dual';dual_root.mkdir()
 (dual_root/'Library.fs').write_text('namespace Provenance\nmodule DualRole =\n    let keepSymbol (value:FSharp.Compiler.Symbols.FSharpSymbol) = value\n')
 project(dual_root/'Dual.fsproj','<Compile Include="Library.fs"/>','<Reference Include="FSharp.Compiler.Service"><HintPath>'+str(fcs)+'</HintPath></Reference>')
 run('build',dual_root/'Dual.fsproj','--nologo','-v:q','-m:1','-nodeReuse:false')
 dual_assembly=dual_root/'bin/Debug/net10.0/Dual.dll';dual_catalog=root/'dual-role-catalog.json'
 dual=invoke('metadata/export',{'assemblyPaths':[str(dual_assembly)],'referencePaths':refs+[str(fcs),str(dual_assembly)],'outputPath':str(dual_catalog)})
 def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
 def inputs(value):return {item['path']:item['sha256']for item in value['inputHashes']}
 expected={'exporter':digest(helper),'fcs':digest(fcs)}
 for artifact in [response,environment,dual]:
  assert artifact['metadataCertificateVersion']==1
  producer=artifact['producer'];assert producer['contract']=='fsharp-navigator.binary-facts' and producer['algorithmId']=='binary-facts-v2'
  assert {v['role']:v['sha256']for v in producer['implementationHashes']}==expected
 for artifact in [response,environment]:
  assert str(helper)not in inputs(artifact) and str(fcs)not in inputs(artifact)
  assert str(assembly)in inputs(artifact)
 assert str(assembly.with_suffix('.xml'))in inputs(response)
 assert inputs(dual)[str(fcs)]==expected['fcs']
 assert any(v.get('resolvedPath')==str(fcs)or v.get('path')==str(fcs)for v in dual['dependencyBindings']),dual['dependencyBindings']
 result={'scope':'actual producer role separation; native consumer replay is a separate gate','fixtureRoot':str(root),'helperSha256':expected['exporter'],
  'catalogPath':str(catalog),'environmentPath':str(root/'environment.json'),'dualRoleCatalogPath':str(dual_catalog),
  'producerFiles':[str(helper),str(fcs)],'producer':response['producer'],'checks':{'catalogOmitsProducerOnlyPaths':True,'inventoryOmitsProducerOnlyPaths':True,'selectedAssemblyAndXmlRemainLive':True,'selectedFcsDualRoleRemainsLive':True},'passed':4}
 (ROOT/'test-results/recovery/metadata-provenance.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result,indent=2))
if __name__=='__main__':main()
