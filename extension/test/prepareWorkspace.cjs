'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const exec = promisify(execFile);

function project(items) {
  return `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <DisableImplicitFSharpCoreReference>true</DisableImplicitFSharpCoreReference>
    <GenerateAssemblyInfo>false</GenerateAssemblyInfo>
    <GenerateTargetFrameworkAttribute>false</GenerateTargetFrameworkAttribute>
    <GenerateDocumentationFile>true</GenerateDocumentationFile>
    <NuGetAudit>false</NuGetAudit>
  </PropertyGroup>
  <ItemGroup>
    <Reference Include="FSharp.Core"><HintPath>$(MSBuildToolsPath)/FSharp/FSharp.Core.dll</HintPath></Reference>
    ${items}
  </ItemGroup>
</Project>\n`;
}

// Explicit CLI fixture preparation finishes before the editor and the indexed daemon start.
exports.prepareWorkspace = async function (temporary, workspace, root, env) {
  await fs.appendFile(path.join(workspace, 'Main.fs'), 'let callbacks = List.map (fun x -> x + x) [1; 2]\n');
  const external = path.join(temporary, 'external');
  const prepared = path.join(temporary, 'prepared');
  await fs.mkdir(external);
  await fs.writeFile(path.join(external, 'ExternalLibrary.fsproj'), project('<Compile Include="Library.fs"/>'));
  await fs.writeFile(path.join(external, 'Library.fs'), 'module ExternalLibrary\n/// The answer from a library.\nlet answer = 42\n');
  const dotnet = env.FSHARP_NAVIGATOR_DOTNET || 'dotnet';
  const options = { env, timeout: 300_000 };
  const links = ['A/Zulu.fs', 'B/Alpha.fs', 'A/Nested/Members.fs', 'A/Main.fs'];
  const compile = links.map(link => `<Compile Include="${path.basename(link)}"><Link>${link}</Link></Compile>`);
  const assembly = path.join(external, 'bin', 'Release', 'net10.0', 'ExternalLibrary.dll');
  // The reference lives outside the workspace. It is a real binary, never a source project.
  compile.push('<Reference Include="ExternalLibrary"><HintPath>../external/bin/Release/net10.0/ExternalLibrary.dll</HintPath></Reference>');
  await fs.writeFile(path.join(workspace, 'Ordered.fsproj'), project(compile.join('\n    ')));
  const solution = path.join(workspace, 'Ordered.slnx');
  await fs.writeFile(solution, '<Solution><Folder Name="/Src/"><Project Path="Ordered.fsproj"/></Folder></Solution>\n');
  await exec(dotnet, ['build', 'ExternalLibrary.fsproj', '-c', 'Release', '--nologo', '-p:RestoreSources=' + external], { ...options, cwd: external });
  // Main.fs deliberately includes an unresolved use for the unsupported-binding control.
  await exec(dotnet, ['restore', 'Ordered.fsproj', '--source', external, '--nologo'], { ...options, cwd: workspace });
  await exec(path.join(root, 'cli/bin/run'), ['prepare', '--root', workspace, '--solution', solution, '--out', prepared, '--assembly', assembly], { ...options, cwd: root });
  return JSON.parse(await fs.readFile(path.join(prepared, 'settings.json'), 'utf8'));
};
