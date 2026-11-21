#!/usr/bin/env python3
"""Real binary/XML documentation fidelity and bounded untrusted formatting."""
import hashlib,json,pathlib,subprocess,tempfile
from smoke import ROOT,DOTNET,ENV,HELPER,project,run,request

def raw(parameters):
 p=subprocess.run([str(DOTNET),str(HELPER),'--once'],env=ENV,input=json.dumps({'id':1,'method':'metadata/export','params':parameters})+'\n',text=True,capture_output=True,timeout=120);assert p.returncode==0,p.stderr[-1500:];return json.loads(p.stdout)

def main():
 root=pathlib.Path(tempfile.mkdtemp(prefix='xml-doc-proof-',dir=ROOT/'.cache'))
 source='''namespace Example
module Docs =
    /// <summary>Treat <see langword="null"/>, <see langword="true"/>, and <see langword="false"/> as literal tokens.</summary>
    /// <remarks><para>First paragraph: <paramref name="value"/> has type <typeparamref name="T"/>; see <see cref="T:System.String"/>.</para><para>Second paragraph keeps <c>a  +  b</c>.</para><para>[run](command:evil) &lt;img src=x&gt; <see href="command:evil">plain label</see>.</para></remarks>
    /// <param name="value">Input <paramref name="value"/>.</param>
    /// <typeparam name="T">The generic value type.</typeparam>
    /// <returns>The original <paramref name="value"/>.</returns>
    /// <example><code>let answer =  1
    ///   answer</code></example>
    let keep<'T> (value:'T) = value
'''
 (root/'Library.fs').write_text(source);p=root/'Library.fsproj';project(p,'<Compile Include="Library.fs"/>');run('build',p,'--nologo','-v:q','-m:1','-nodeReuse:false')
 dll=root/'bin/Debug/net10.0/Library.dll';xml=dll.with_suffix('.xml');original=xml.read_text()
 catalog=request('metadata/export',{'assemblyPaths':[str(dll)]});symbol=next(s for s in catalog['symbols'] if s['fullName']=='Example.Docs.keep');doc=symbol['documentation']
 for text in ['`null`, `true`, and `false`','`value` has type `T`','`System.String`','Second paragraph keeps `a  +  b`','**Returns:** The original `value`.','\\[run\\](command:evil)','&lt;img src=x&gt;','plain label','let answer =  1']:
  assert text in doc,(text,doc)
 assert '\n\nSecond paragraph' in doc and '[run](command:evil)'not in doc and '<img'not in doc
 print('PASS actual F# binary/XML langword, references, paragraphs, code spacing, returns and safe Markdown',flush=True)
 failures=[]
 for label,bad in [('DTD','<!DOCTYPE doc [<!ENTITY x "entity">]><doc><members><member name="x"><summary>&x;</summary></member></members></doc>'),('member character bound','<doc><members><member name="x"><summary>'+('x'*70000)+'</summary></member></members></doc>'),('nesting bound','<doc><members><member name="x"><summary>'+('<para>'*40)+'x'+('</para>'*40)+'</summary></member></members></doc>')]:
  xml.write_text(bad);reply=raw({'assemblyPaths':[str(dll)]});assert 'error'in reply,(label,reply);failures.append({'case':label,'error':reply['error']['message']});print('PASS rejection:',label,flush=True)
 xml.write_text(original)
 refdir=DOTNET.parent/'packs/Microsoft.NETCore.App.Ref/10.0.11/ref/net10.0';runtime=refdir/'System.Runtime.dll';refs=sorted(map(str,refdir.glob('*.dll')))+[str(DOTNET.parent/'sdk/10.0.400/FSharp/FSharp.Core.dll')]
 output=root/'runtime-catalog.json';result=request('metadata/export',{'assemblyPaths':[str(runtime)],'referencePaths':refs,'outputPath':str(output)})
 framework=json.loads(output.read_text());method=next(s for s in framework['symbols'] if s['xmlDocId']=='M:System.String.IsNullOrEmpty(System.String)')
 assert all('`'+word+'`'in method['documentation'] for word in ['null','true','false']),method['documentation']
 print('PASS real System.String.IsNullOrEmpty retains null/true/false meaning',flush=True)
 evidence={'helperSha256':hashlib.sha256(HELPER.read_bytes()).hexdigest(),'fixtureRoot':str(root),'symbolXmlDocId':symbol['xmlDocId'],'renderedDocumentation':doc,'frameworkXmlDocId':method['xmlDocId'],'frameworkDocumentation':method['documentation'],'boundedRejections':failures,'passed':5,'scope':'binary declaration/XML renderer only; no editor or full documentation parity claim'}
 (ROOT/'test-results/recovery/xml-documentation.json').write_text(json.dumps(evidence,indent=2)+'\n')
if __name__=='__main__':main()
