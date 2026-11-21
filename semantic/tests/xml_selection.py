#!/usr/bin/env python3
"""Actual XML resolver priority witnesses plus retained native replay fixtures."""
import hashlib,json,os,pathlib,shutil,subprocess,tempfile
from smoke import ROOT,DOTNET,ENV,HELPER,project,run

def main():
 root=pathlib.Path(tempfile.mkdtemp(prefix='xml-selection-',dir=ROOT/'.cache'))
 build=root/'build';build.mkdir();(build/'Library.fs').write_text('namespace Witness\nmodule Library =\n    let value = 1\n')
 project(build/'Library.fsproj','<Compile Include="Library.fs"/>');run('build',build/'Library.fsproj','--nologo','-v:q','-m:1','-nodeReuse:false')
 binary=build/'bin/Debug/net10.0/Library.dll';core=DOTNET.parent/'sdk/10.0.400/FSharp/FSharp.Core.dll'
 refs=sorted(map(str,(DOTNET.parent/'packs/Microsoft.NETCore.App.Ref/10.0.11/ref/net10.0').glob('*.dll')))+[str(core)]
 def invoke(assembly,out):
  p=subprocess.run([str(DOTNET),str(HELPER),'--once'],env=ENV,input=json.dumps({'id':1,'method':'metadata/export','params':{'assemblyPaths':[str(assembly)],'referencePaths':refs,'outputPath':str(out)}})+'\n',text=True,capture_output=True,timeout=45)
  assert p.returncode==0,p.stderr
  return json.loads(p.stdout)
 def doc(word):return '<doc><members><member name="P:Witness.Library.value"><summary>'+word+'</summary></member></members></doc>'
 # FCS emits the XML ID for this actual compiled declaration; discover it once.
 seed=root/'seed.json';reply=invoke(binary,seed);assert 'error'not in reply,reply
 target=next(s for s in json.loads(seed.read_text())['symbols']if s['fullName']=='Witness.Library.value');xml_id=target['xmlDocId']
 def doc(word):return '<doc><members><member name="'+xml_id+'"><summary>'+word+'</summary></member></members></doc>'
 low='bin/Library.xml';upper='bin/Library.XML';parent='Library.xml';empty='<doc><members></members></doc>'
 definitions=[
  ('all_absent',{}, {parent:doc('PARENT')},False,'','PARENT'),
  ('higher_priority',{upper:doc('UPPER')},{upper:doc('UPPER'),low:doc('FIRST')},False,'UPPER','FIRST'),
  ('lower_priority_unrelated',{low:doc('FIRST')},{low:doc('FIRST'),upper:doc('UPPER'),parent:doc('PARENT')},True,'FIRST','FIRST'),
  ('directory_to_file',{low:{'kind':'directory'},upper:doc('UPPER')},{low:doc('FIRST'),upper:doc('UPPER')},False,'UPPER','FIRST'),
  ('directory_contents_unrelated',{low:{'kind':'directory'},upper:doc('UPPER')},{low:{'kind':'directory'},low+'/child':'irrelevant',upper:doc('UPPER')},True,'UPPER','UPPER'),
  ('selected_removed',{low:doc('FIRST'),upper:doc('UPPER')},{upper:doc('UPPER')},False,'FIRST','UPPER'),
  ('selected_replaced_preserved_mtime',{low:doc('FIRST')},{low:doc('OTHER')},False,'FIRST','OTHER'),
  ('no_symbol_fallthrough',{low:empty,upper:doc('UPPER')},{low:empty,upper:doc('OTHER')},True,'',''),
  ('dangling_link_target_created',{low:{'kind':'symlink','target':'target.xml'},upper:doc('UPPER')},{low:{'kind':'symlink','target':'target.xml'},upper:doc('UPPER'),'bin/target.xml':doc('FIRST')},False,'UPPER','FIRST'),
 ]
 rows=[]
 def write_all(directory,states):
  for child in directory.iterdir():
   if child.is_dir()and not child.is_symlink():shutil.rmtree(child)
   else:child.unlink()
  (directory/'bin').mkdir();shutil.copy2(binary,directory/'bin/Library.dll')
  for name,value in sorted(states.items(),key=lambda v:len(v[0])):
   path=directory/name;path.parent.mkdir(parents=True,exist_ok=True)
   if isinstance(value,dict):
    if value['kind']=='directory':path.mkdir(exist_ok=True)
    elif value['kind']=='symlink':path.symlink_to(value['target'])
   else:path.write_text(value)
 for name,before,after,valid,old_doc,new_doc in definitions:
  directory=root/name;directory.mkdir();write_all(directory,before);assembly=directory/'bin/Library.dll'
  old=root/(name+'-before.json');reply=invoke(assembly,old);assert 'error'not in reply,(name,reply)
  previous=json.loads(old.read_text());actual=next(s['documentation']for s in previous['symbols']if s['fullName']==target['fullName']);assert actual==old_doc,(name,actual)
  write_all(directory,after);new=root/(name+'-after.json');reply=invoke(assembly,new);assert 'error'not in reply,(name,reply)
  fresh=json.loads(new.read_text());actual=next(s['documentation']for s in fresh['symbols']if s['fullName']==target['fullName']);assert actual==new_doc,(name,actual)
  write_all(directory,before)
  changes=[]
  for key in sorted(set(before)|set(after),key=lambda v:len(v),reverse=True):
   if before.get(key)==after.get(key):continue
   value=after.get(key);kind=value.get('kind')if isinstance(value,dict)else 'missing'if value is None else'file'
   changes.append({'path':str(directory/key),'kind':kind,'text':value if isinstance(value,str)else'','preserveMtime':name=='selected_replaced_preserved_mtime'})
  rows.append({'case':name,'beforeCatalog':str(old),'afterCatalog':str(new),'expectedBefore':old_doc,'expectedAfter':new_doc,'validAfter':valid,'changes':changes,'selection':previous['xmlSelections']})
  print('PASS producer XML priority:',name,flush=True)
 failures=[]
 for kind in ['malformed','fifo','permission']:
  directory=root/('reject-'+kind);directory.mkdir();write_all(directory,{upper:doc('UPPER')});path=directory/low
  if kind=='fifo':os.mkfifo(path)
  elif kind=='permission':
   if os.geteuid()==0:continue
   path.write_text(doc('FIRST'));path.chmod(0)
  else:path.write_text('<broken')
  try:reply=invoke(directory/'bin/Library.dll',root/(kind+'.json'))
  finally:
   if kind=='permission':path.chmod(0o600)
  assert 'error'in reply,(kind,reply);failures.append({'case':kind,'error':reply['error']});print('PASS no silent fallback:',kind,flush=True)
 manifest={'helperSha256':hashlib.sha256(HELPER.read_bytes()).hexdigest(),'fixtureRoot':str(root),'symbol':target['fullName'],'xmlDocId':xml_id,'cases':rows,'rejections':failures,'scope':'actual producer witnesses; native cached/reopened replay separate','passedProducerCases':len(rows)+len(failures)}
 (root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');(ROOT/'test-results/recovery/xml-selection-producer.json').write_text(json.dumps(manifest,indent=2)+'\n')
 print('Native fixture:',root)
if __name__=='__main__':main()
