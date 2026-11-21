#!/usr/bin/env python3
"""Real MSBuild wildcard provenance; no source bodies in membership digest."""
import hashlib,pathlib,subprocess,tempfile
from smoke import DOTNET, ENV, run,request,project

def digest(cert):
 root=pathlib.Path(cert['root']); paths=root.rglob('*') if cert['recursive'] else root.glob('*')
 names=sorted(p.relative_to(root).as_posix().encode() for p in paths if p.is_file() and p.suffix.lstrip('.').lower() in cert['extensions'])
 return hashlib.sha256(b''.join(n+b'\0' for n in names)).hexdigest(),len(names)
def evaluate(root,proj):return request('workspace/evaluate',{'root':str(root),'projects':[str(proj)]})['projects'][0]
def check_indirect_signature_sources(root):
    for kind in ["response", "script"]:
        folder = root / kind
        folder.mkdir()
        if kind == "script":
            (folder / "Load.fsx").write_text('#load "Target.fsi"\nlet n = 0\n')
            sources = '<Compile Include="Load.fsx"/>'
            flags = ""
        else:
            (folder / "sources.rsp").write_text("Target.fsi\n")
            sources = ""
            flags = "<OtherFlags>@sources.rsp</OtherFlags>"
        proj = folder / "P.fsproj"
        project(proj, sources + '<Compile Include="Target.fs"/>')
        proj.write_text(proj.read_text().replace("</PropertyGroup>", flags + "</PropertyGroup>"))
        impl = folder / "Target.fs"
        sig = folder / "Target.fsi"
        impl.write_text("module Target\nlet f (x:int) = x+x\n")
        sig.write_text("module Target\nval f: x:int -> int\n")
        run("build", proj, "-m:1", "-nr:false", cwd=folder)
        model = evaluate(folder, proj)
        assert not model["membershipReusable"] and any(
            "Unmodeled compiler input" in note for note in model["membershipDiagnostics"]
        ), model["membershipDiagnostics"]
        # The conflicting signature must reach fsc even though it does not
        # appear among the evaluated files. Force a rebuild after its edit.
        assert str(sig) not in model["files"]
        sig.write_text("module Target\nval f: x:string -> string\n")
        result = subprocess.run(
            [str(DOTNET), "build", str(proj), "--no-restore", "-t:Rebuild", "-m:1", "-nr:false"],
            cwd=folder, env=ENV, text=True, capture_output=True, timeout=120,
        )
        assert result.returncode and "FS0034" in result.stdout, result.stdout + result.stderr

def main():
 with tempfile.TemporaryDirectory(prefix='membership-') as tmp:
  check_indirect_signature_sources(pathlib.Path(tmp))
  root=pathlib.Path(tmp);g=root/'glob';g.mkdir();(g/'A.fs').write_text('module A\nlet value=1\n');p=g/'Glob.fsproj';project(p,'<Compile Include="*.fs"/>');run('restore',p,'--nologo','-v:q')
  old=evaluate(g,p);assert old['membershipReusable'],old['membershipDiagnostics'];cert=next(c for c in old['sourceMembership'] if 'fs' in c['extensions']);assert not cert['recursive'];assert digest(cert)==(cert['sha256'],cert['entries'])
  (g/'B.fs').write_text('module B\nlet value=A.value\n');run('build',p,'--nologo','-v:q','-m:1','-nodeReuse:false');assert digest(cert)[0]!=cert['sha256'];assert str(g/'B.fs') in evaluate(g,p)['files'];print('PASS new glob source invalidates membership without project-byte changes',flush=True)
  varied=root/'varied';(varied/'source/sub').mkdir(parents=True)
  for i,name in enumerate(['A-long.fs','Z.fs','sub/x.fs','ı.fs','𐐀.fs']):
   (varied/'source'/name).write_text(f'module Member{i}\nlet value={i}\n')
  varied_project=varied/'Varied.fsproj';project(varied_project,'<Compile Include="source/**/*.fs"/>');run('restore',varied_project,'--nologo','-v:q')
  varied_model=evaluate(varied,varied_project);assert varied_model['membershipReusable'],varied_model['membershipDiagnostics']
  varied_cert=next(c for c in varied_model['sourceMembership']if c['root']==str(varied/'source'))
  assert digest(varied_cert)==(varied_cert['sha256'],varied_cert['entries'])
  names=[path.relative_to(varied/'source').as_posix().encode()for path in (varied/'source').rglob('*.fs')]
  wrong=hashlib.sha256(b''.join(n+b'\0'for n in sorted(names,key=lambda n:(len(n),n)))).hexdigest();assert wrong!=varied_cert['sha256']
  print('PASS varied-length and Unicode source names use lexicographic UTF-8 membership order',flush=True)
  project(p,'<Compile Include="A.fs"/>');explicit=evaluate(g,p);assert explicit['membershipReusable'] and not any('fs' in c['extensions'] for c in explicit['sourceMembership']);print('PASS explicit-only source list has no unrelated directory dependency',flush=True)
  shared=root/'shared';shared.mkdir();(shared/'S.fs').write_text('module Shared\nlet value=1\n');project(p,'<Compile Include="../shared/*.fs"/>');linked=evaluate(g,p);assert linked['membershipReusable'] and any(c['root']==str(shared) for c in linked['sourceMembership']);(shared/'T.fs').write_text('module T\nlet value=2\n');assert any(digest(c)[0]!=c['sha256'] for c in linked['sourceMembership'] if c['root']==str(shared));print('PASS external literal glob domain retained',flush=True)
  for pattern in ['*.txt', '*', '*.f?']:
   project(p,f'<Compile Include="{pattern}"/>'); unsupported=evaluate(g,p);assert not unsupported['membershipReusable'],unsupported
  print('PASS wildcard extensions outside the certified filename domain refuse persistent reuse',flush=True)
  project(p,'<Compile Include="$(ExtraFiles)"/>');p.write_text(p.read_text().replace('<TargetFramework>','<ExtraFiles>A.fs</ExtraFiles><TargetFramework>'));assert not evaluate(g,p)['membershipReusable']
  project(p,'<Compile Include="A.fs"/>');p.write_text(p.read_text().replace('</Project>','<Target Name="Custom" BeforeTargets="Compile" /></Project>'));assert not evaluate(g,p)['membershipReusable'];print('PASS unknown item expressions and custom targets refuse persistent shape proof',flush=True)
  packages=root/'packages';packages.mkdir();target=packages/'Analyzer.targets'
  target.write_text('<Project><Target Name="AnalyzerOnly" BeforeTargets="CoreCompile"><ItemGroup><Analyzer Remove="@(Analyzer)" /></ItemGroup><Message Text="analyzer-only" /></Target></Project>')
  optional=packages/'Optional.targets'
  project(p,'<Compile Include="A.fs"/>')
  p.write_text(p.read_text().replace('<TargetFramework>',f'<NuGetPackageRoot>{packages}/</NuGetPackageRoot><TargetFramework>').replace('</Project>','<Import Project="$(NuGetPackageRoot)Analyzer.targets" /><Import Project="$(NuGetPackageRoot)Optional.targets" Condition="Exists(&apos;$(NuGetPackageRoot)Optional.targets&apos;)" /></Project>'))
  package_model=evaluate(g,p);assert package_model['membershipReusable'],package_model['membershipDiagnostics'];assert str(target) in package_model['inputs'] and str(optional) in package_model['inputs']
  print('PASS evaluated NuGet root imports and missing-to-present probes with neutral analyzer-only targets',flush=True)
  target.write_text('<Project><Target Name="Unknown" BeforeTargets="NeverRun"><Exec Command="echo no" /></Target></Project>');assert not evaluate(g,p)['membershipReusable']
  target.write_text('<Project><Target Name="Unknown" BeforeTargets="NeverRun"><ItemGroup><Compile Include="*.fs"/></ItemGroup></Target></Project>');assert not evaluate(g,p)['membershipReusable']
  target.write_text('<Project><Target Name="Unknown" BeforeTargets="NeverRun"><PropertyGroup><_GlobalAnalyzerConfigX>$([System.IO.File]::Exists(&apos;x&apos;))</_GlobalAnalyzerConfigX></PropertyGroup></Target></Project>');assert not evaluate(g,p)['membershipReusable']
  print('PASS arbitrary NuGet task/source mutation/property-function targets remain unsupported',flush=True)
if __name__=='__main__':main()
