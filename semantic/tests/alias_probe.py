import json,pathlib,tempfile
from smoke import project,run,request
results=[]
with tempfile.TemporaryDirectory(prefix='fsharp-alias-probe-') as tmp:
 p=pathlib.Path(tmp);file=p/'Aliases.fs'
 file.write_text('module Aliases\ntype Sha = string\ntype Other = string\nlet sha : Sha = Sha "a"\nlet other : Other = Other "b"\n')
 project(p/'Aliases.fsproj','<Compile Include="Aliases.fs"/>')
 run('build',p/'Aliases.fsproj','--nologo','-v:q')
 for line,char in [(1,6),(1,12),(2,6),(3,11),(3,17),(4,21)]:
  params={'root':str(p),'path':str(file),'line':line,'character':char}
  result={'cursor':[line,char],'definition':request('navigation/definition',params),'references':request('navigation/references',params)}
  results.append(result)
  print(json.dumps(result),flush=True)
 pathlib.Path('test-results/recovery/alias-probe.json').write_text(json.dumps({'source':file.read_text(),'results':results},indent=2)+'\n')
