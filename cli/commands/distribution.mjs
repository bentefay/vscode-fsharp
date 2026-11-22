import { Command, Option } from 'clipanion';
import path from 'node:path';
import { exec, root } from '../lib/process.mjs';
import { buildPackage, inspectPackage, vsixPath } from '../lib/package.mjs';
import { isolatedCode, isolatedProfile } from '../lib/isolated-profile.mjs';

const extensionId = 'bentefay.fsharp-navigator';

export class PackageCommand extends Command {
  static paths=[['package']];
  output=Option.String('--output', { description: 'Write the VSIX and adjacent hash/build metadata to this path instead of the default artifact.' });
  async execute(){await buildPackage(this.output);}
}
export class InstallCommand extends Command {
  static paths=[['install']];
  code=Option.String('--code','code');profile=Option.String('--profile');vsix=Option.String('--vsix');force=Option.Boolean('--force',false);dryRun=Option.Boolean('--dry-run',false);
  isolatedDir=Option.String('--isolated-dir', { description: 'Opt in to a new/owned Linux preview directory with separate user-data, extensions and caches; requires --vsix. Never launches the editor.' });
  async execute(){
    if(this.isolatedDir!==undefined){
      if(!this.vsix || this.profile!==undefined) throw new Error('--isolated-dir requires --vsix and cannot be combined with --profile.');
      const file=path.resolve(process.env.FN_INVOCATION_CWD||root,this.vsix);
      let isolated=await isolatedProfile(this.isolatedDir,{cwd:process.env.FN_INVOCATION_CWD||root});
      const args=[...isolated.args,'--install-extension',file,'--do-not-include-pack-dependencies',...(this.force?['--force']:[])];
      if(this.dryRun){console.log(JSON.stringify({build:false,launch:false,command:this.code,args,directories:isolated.directories},null,2));return;}
      const code=await isolatedCode(this.code,{cwd:process.env.FN_INVOCATION_CWD||root});
      await inspectPackage(file);
      isolated=await isolatedProfile(this.isolatedDir,{cwd:process.env.FN_INVOCATION_CWD||root,create:true});
      await exec(code,args,{env:isolated.env});
      return;
    }
    const file=this.vsix?path.resolve(process.env.FN_INVOCATION_CWD||root,this.vsix):vsixPath;
    const args=['--install-extension',file,...(this.profile?['--profile',this.profile]:[]),...(this.force?['--force']:[])];
    if(this.dryRun){console.log(JSON.stringify({build:!this.vsix,command:this.code,args},null,2));return;}
    if(!this.vsix)await buildPackage();await inspectPackage(file);await exec(this.code,args);
  }
}
export class UninstallCommand extends Command {
  static paths=[['uninstall']];
  code=Option.String('--code','code');
  isolatedDir=Option.String('--isolated-dir',{required:true});
  dryRun=Option.Boolean('--dry-run',false);
  async execute(){
    const isolated=await isolatedProfile(this.isolatedDir,{cwd:process.env.FN_INVOCATION_CWD||root,requireOwned:true});
    const args=[...isolated.args,'--uninstall-extension',extensionId];
    if(this.dryRun){console.log(JSON.stringify({build:false,launch:false,command:this.code,args,directories:isolated.directories,preservesProfileData:true},null,2));return;}
    const code=await isolatedCode(this.code,{cwd:process.env.FN_INVOCATION_CWD||root});
    await exec(code,args,{env:isolated.env});
  }
}
export class ReleaseCommand extends Command {
  static paths=[['release']];
  dryRun=Option.Boolean('--dry-run',false);tag=Option.String('--tag');
  async execute(){
    if(this.dryRun){console.log(JSON.stringify({status:'prototype-not-configured',tag:this.tag,steps:['verify','package','review destination','explicit publish command'],publishes:false},null,2));return;}
    throw new Error('Publishing is not configured for this local prototype. Package and install are available.');
  }
}
export class SourceCommand extends Command {
  static paths=[['source']];
  output=Option.String('--output');
  async execute(){await exec('python3',['scripts/package-source.py',...(this.output?['--output',path.resolve(process.env.FN_INVOCATION_CWD||root,this.output)]:[])]);}
}
