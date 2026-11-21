import os,pathlib
ROOT=pathlib.Path('/workspace/scratch/dd9aeba5da3c/fsharp-navigator')
DOTNET=pathlib.Path('/workspace/scratch/dd9aeba5da3c/research/corpus/fsharp/.dotnet/dotnet')
ENV=dict(os.environ,DOTNET_ROOT=str(DOTNET.parent),FSHARP_NAVIGATOR_DOTNET=str(DOTNET),DOTNET_ROLL_FORWARD="Major",DOTNET_NOLOGO="1",DOTNET_CLI_TELEMETRY_OPTOUT="1",MSBUILDDISABLENODEREUSE="1",DOTNET_CLI_USE_MSBUILD_SERVER="0")
def project(path,sources,extra=""):
 path.parent.mkdir(parents=True,exist_ok=True)
 (path.parent/"global.json").write_text('{"sdk":{"version":"11.0.100-rc.3.26522.148","allowPrerelease":true,"rollForward":"disable"}}')
 path.write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net11.0</TargetFramework><DisableImplicitFSharpCoreReference>true</DisableImplicitFSharpCoreReference><GenerateDocumentationFile>true</GenerateDocumentationFile><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="FSharp.Core"><HintPath>$(MSBuildToolsPath)/FSharp/FSharp.Core.dll</HintPath></Reference>'+sources+extra+'</ItemGroup></Project>')
