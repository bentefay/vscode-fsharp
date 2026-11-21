#!/usr/bin/env python3
"""Occurrence-specific aliases: do not merge erased constructor identities."""
import json,pathlib,tempfile
from smoke import ROOT,project,run,request

def key(location):
 return json.dumps({'path':location['path'],'range':location['range']},sort_keys=True)
def main():
 evidence=[]
 with tempfile.TemporaryDirectory(prefix='fsharp-alias-snapshot-') as tmp:
  root=pathlib.Path(tmp);file=root/'Aliases.fs'
  text='module Aliases\ntype Sha = string\ntype Other = string\nlet sha : Sha = Sha "a"\nlet other : Other = Other "b"\ntype Box<\'T>(value: \'T) = member _.Value = value\nlet first = Box(1).Value\nlet second = Box("x").Value\n'
  file.write_text(text);p=root/'Aliases.fsproj';project(p,'<Compile Include="Aliases.fs"/>');run('build',p,'--nologo','-v:q')
  exported=request('semantic/export',{'root':str(root)});assert exported['completeness']=='exact',exported
  rows=[o for p in exported['projects'] for o in p['occurrences']]
  for line,char in [(1,6),(2,6),(3,11),(3,17),(4,21),(6,20),(7,22)]:
   hits=[o for o in rows if o['path']==str(file) and o['range']['start']['line']==line and o['range']['start']['character']<=char<o['range']['end']['character']]
   ids={o['symbolId'] for o in hits};assert len(ids)==1,(line,char,hits)
   actual={key(o) for o in rows if o['symbolId'] in ids}
   oracle=request('navigation/references',{'root':str(root),'path':str(file),'line':line,'character':char});expected={key(o) for o in oracle['locations']}
   assert actual==expected,(line,char,actual,expected)
   if (line,char) in [(3,17),(4,21)]:assert all(o['declaration'] is None for o in hits),'Alias constructor must not fabricate alias declaration goto'
   evidence.append({'cursor':[line,char],'referenceCount':len(actual),'matchedDirectFcs':True,'constructorDefinitionPreserved':(line,char) in [(3,17),(4,21)]})
  print('PASS distinct erased aliases retain constructor/annotation reference groups and external constructor targets',flush=True)
  print('PASS generic member instantiations still share their actual compiler reference group',flush=True)
  signature=root/'Aliases.fsi';signature.write_text('module Aliases\ntype Sha = string\ntype Other = string\nval sha : Sha\nval other : Other\ntype Box<\'T> =\n    new : value:\'T -> Box<\'T>\n    member Value : \'T\nval first : int\nval second : string\n')
  project(p,'<Compile Include="Aliases.fsi"/><Compile Include="Aliases.fs"/>');run('build',p,'--nologo','-v:q')
  exported=request('semantic/export',{'root':str(root)});assert exported['completeness']=='exact',exported
  rows=[o for p in exported['projects'] for o in p['occurrences']]
  for f,line,char in [(signature,1,6),(file,1,6),(file,3,17)]:
   ids={o['symbolId'] for o in rows if o['path']==str(f) and o['range']['start']['line']==line and o['range']['start']['character']<=char<o['range']['end']['character']};assert len(ids)==1,(f,line,ids)
   actual={key(o) for o in rows if o['symbolId'] in ids};oracle=request('navigation/references',{'root':str(root),'path':str(f),'line':line,'character':char});expected={key(o) for o in oracle['locations']};assert actual==expected,(f,line,actual,expected)
  print('PASS signature/implementation alias reference groups match direct FCS',flush=True)
 (ROOT/'test-results/recovery/alias-snapshot.json').write_text(json.dumps({'checks':evidence,'signatureImplementationPassed':True},indent=2)+'\n')
if __name__=='__main__':main()
