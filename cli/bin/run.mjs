#!/usr/bin/env node
import { Builtins, Cli } from 'clipanion';
import { SetupCommand, DoctorCommand, CheckCommand, BuildCommand, TestCommand, VerifyCommand, DependenciesCommand, EngineCommand } from '../commands/development.mjs';
import { PackageCommand, InstallCommand, UninstallCommand, ReleaseCommand, SourceCommand } from '../commands/distribution.mjs';
import { PrepareCommand, LibrariesExportCommand, EnvironmentExportCommand, EvaluateCommand, MetadataPrepareCommand } from '../commands/prepare.mjs';
import { restoreParserSources } from '../../scripts/restore-parser-sources.mjs';

if(process.env.FN_DEVENV!=='1'&&process.env.FN_LOCAL_TOOLS!=='1')throw new Error('Use cli/bin/run to enter the protected toolchain.');
// Checkout preparation belongs to development commands, never engine startup.
const sourceCommands = new Set(['setup', 'build', 'check', 'test', 'verify', 'package', 'source']);
if (sourceCommands.has(process.argv[2]) || (process.argv[2] === 'install' && !process.argv.slice(3).some(arg => ['--vsix','--isolated-dir'].some(option => arg === option || arg.startsWith(option+'='))))) await restoreParserSources();
const cli=new Cli({binaryLabel:'F# Navigator',binaryName:'run',binaryVersion:'0.1.0'});
for(const command of [Builtins.HelpCommand,Builtins.VersionCommand,SetupCommand,DoctorCommand,CheckCommand,BuildCommand,TestCommand,VerifyCommand,DependenciesCommand,EngineCommand,PackageCommand,InstallCommand,UninstallCommand,ReleaseCommand,SourceCommand,PrepareCommand,LibrariesExportCommand,EnvironmentExportCommand,EvaluateCommand,MetadataPrepareCommand])cli.register(command);
await cli.runExit(process.argv.slice(2));
