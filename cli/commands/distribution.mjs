import { Command, Option } from 'clipanion';
import path from 'node:path';
import { exec, root } from '../lib/process.mjs';
import { buildPackage, inspectPackage, vsixPath } from '../lib/package.mjs';

export class PackageCommand extends Command { static paths=[['package']];async execute(){await buildPackage();} }
export class InstallCommand extends Command {
  static paths=[['install']];
  code=Option.String('--code','code');profile=Option.String('--profile');vsix=Option.String('--vsix');force=Option.Boolean('--force',false);dryRun=Option.Boolean('--dry-run',false);
  async execute(){
    const file=this.vsix?path.resolve(process.env.FN_INVOCATION_CWD||root,this.vsix):vsixPath;
    const args=['--install-extension',file,...(this.profile?['--profile',this.profile]:[]),...(this.force?['--force']:[])];
    if(this.dryRun){console.log(JSON.stringify({build:!this.vsix,command:this.code,args},null,2));return;}
    if(!this.vsix)await buildPackage();await inspectPackage(file);await exec(this.code,args);
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
