import { Command, Option } from 'clipanion';
import { mkdir, copyFile, writeFile, readFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { exec, npm, root } from '../lib/process.mjs';

export async function doctor() {
  for (const [program, args] of [['node',['--version']],['npm',['--version']],['cargo',['--version']],['rustc',['--version']],['safe-chain',['--version']]]) await exec(program,args);
  const dotnet = process.env.FSHARP_NAVIGATOR_DOTNET || 'dotnet';
  console.log(`.NET launch path: ${dotnet}; selected SDK and installed runtimes:`);
  await exec(dotnet, ['--version']);
  await exec(dotnet, ['--list-runtimes']);
  console.log(`Environment: ${process.env.FN_DEVENV === '1' ? 'pinned Nix/devenv' : 'task-local verified tools'}; indexed mode does not start FCS.`);
  if (process.env.FN_DEVENV === '1') console.error('Nix pins SDK 10.0.404/runtime 10.0.15 for development. Reusable semantic preparation is unsupported on this patched toolchain until its SDK/runtime and loader/locale profiles are validated. See nix/README.md.');
}
export async function build() {
  await exec('cargo',['build','--release','--locked']);
  await npm('run','compile');
}
export async function buildDecompiler({ readyToRun = false, rid } = {}) {
  if (readyToRun && rid !== 'linux-x64') throw new Error('--ready-to-run requires explicit --rid linux-x64; no other R2R target is verified.');
  if (rid && !readyToRun) throw new Error('--rid is only supported with --ready-to-run.');
  const project = path.join(root, 'semantic/Decompiler/FSharpNavigator.Decompiler.csproj');
  const options = readyToRun ? ['--runtime', rid, '-p:PublishReadyToRun=true', '-p:RuntimeFrameworkVersion=10.0.15', '-p:TargetLatestRuntimePatch=false', `-p:NuGetLockFilePath=${path.join(root, 'semantic/Decompiler/packages.linux-x64.lock.json')}`] : [];
  await exec('dotnet', ['restore', project, '--locked-mode', ...options]);
  if (readyToRun) {
    const pins = JSON.parse(await readFile(path.join(root, 'semantic/Decompiler/runtime-packs.linux-x64.json'), 'utf8'));
    const packages = process.env.NUGET_PACKAGES || path.join(os.homedir(), '.nuget/packages');
    for (const pin of pins.packages) {
      const filename = path.join(packages, pin.id, pin.version, `${pin.id}.${pin.version}.nupkg`);
      const hash = createHash('sha512');
      for await (const chunk of createReadStream(filename)) hash.update(chunk);
      if (hash.digest('base64') !== pin.sha512) throw new Error(`Runtime build package checksum mismatch: ${pin.id} ${pin.version}`);
    }
  }
  const output = path.join(root, 'bin/decompiler', readyToRun ? `${rid}-r2r` : 'il');
  await exec('dotnet', ['publish', project, '-c', 'Release', '--no-restore', '--self-contained', 'false', '-p:UseAppHost=false', '-p:PublishReadyToRunShowWarnings=true', '--output', output, ...options]);
  console.log(`Optional one-shot decompiler: ${path.join(output, 'FSharpNavigator.Decompiler.dll')}`);
  console.log('This framework-dependent helper requires installed .NET 10.0.15; it is not bundled into the VSIX.');
}
export async function check() {
  await exec('python3', ['scripts/test_dependency_age.py']);
  await exec('python3', ['scripts/test_normalize_vsix.py']);
  await exec('python3', ['evaluation/test_engine_corpus_gate.py']);
  await exec('node', ['--test', 'scripts/restore-parser-sources.test.mjs', 'scripts/package-output.test.mjs', 'scripts/isolated-install.test.mjs', 'extension/test/e2e/manifest.test.cjs', 'extension/test/editorLaunch.test.cjs']);
  await exec('node',['node_modules/typescript/bin/tsc','-p','extension/tsconfig.json','--noEmit']);
  await exec('cargo',['fmt','--all','--','--check']);
  await exec('cargo',['test','--workspace','--locked']);
  await npm('run','compile');
  await exec('node',['--test','extension/test/transport.test.cjs','extension/test/transportTrace.test.cjs','extension/test/virtualDocuments.test.cjs','extension/test/adapterContract.test.cjs']);
}
export async function testHost(packaged = false) {
  const env = { ...process.env, FSHARP_BACKEND_PATH: path.join(root,'target/release/fsharp-navigator') };
  if (packaged) {
    const { buildPackage } = await import('../lib/package.mjs');
    env.FSHARP_TEST_VSIX = await buildPackage();
  }
  const command = process.platform === 'linux' ? 'extension/test/with-display.sh' : 'node';
  const args = process.platform === 'linux' ? ['node','extension/test/runTest.cjs'] : ['extension/test/runTest.cjs'];
  await exec(command,args,{env,timeout:600000});
}
export class SetupCommand extends Command {
  static paths = [['setup']];
  async execute() { await npm('ci','--ignore-scripts'); await auditDependencies(); await exec('cargo',['fetch','--locked']); await doctor(); }
}
export class DoctorCommand extends Command { static paths=[['doctor']]; async execute(){await doctor();} }
export class CheckCommand extends Command { static paths=[['check']]; async execute(){await check();} }
export class FormatCommand extends Command { static paths=[['format']]; async execute(){await exec('cargo',['fmt','--all']);} }
export class RuntimeCommand extends Command {
  static paths = [['runtime', 'fetch']];
  version = Option.String('--version', '1.145.3');
  async execute() { await exec('python3', ['scripts/fetch-editor.py', '--version', this.version]); }
}
export class BuildCommand extends Command {
  static paths=[['build']];
  semantic=Option.Boolean('--semantic',false);
  decompiler=Option.Boolean('--decompiler',false);
  readyToRun=Option.Boolean('--ready-to-run',false);
  rid=Option.String('--rid');
  async execute(){
    if ((this.readyToRun || this.rid) && !this.decompiler) throw new Error('--ready-to-run/--rid require --decompiler.');
    if (this.readyToRun && this.rid !== 'linux-x64') throw new Error('--ready-to-run requires explicit --rid linux-x64.');
    if (this.rid && !this.readyToRun) throw new Error('--rid requires --ready-to-run.');
    await build();
    if(this.semantic) await exec('dotnet',['build','semantic/FSharpNavigator.Semantic.fsproj','-c','Release','--nologo','-p:RestoreLockedMode=true']);
    if(this.decompiler) await buildDecompiler({readyToRun:this.readyToRun,rid:this.rid});
  }
}
export class TestCommand extends Command {
  static paths=[['test']];
  layer=Option.String();
  async execute(){
    if(this.layer==='installer') { await exec('node',['--test','scripts/isolated-install.test.mjs']); return; }
    if(this.layer==='corpus') {
      const inputs = [['FSHARP_CORPUS_GRACE', '--grace-root'], ['FSHARP_CORPUS_FSCHECK', '--fscheck-root'], ['FSHARP_CORPUS_COMPILER', '--compiler-root'], ['FSHARP_CORPUS_OUTPUT', '--output']];
      const missing = inputs.filter(([key]) => !process.env[key]).map(([key]) => key);
      if (missing.length) { console.error(`BLOCKED: engine corpus gate requires ${missing.join(', ')}.`); return 2; }
      try {
        await exec('python3', ['evaluation/engine_corpus_gate.py', '--binary', process.env.FSHARP_BACKEND_PATH || path.join(root, 'target/release/fsharp-navigator'), ...inputs.flatMap(([key, flag]) => [flag, process.env[key]])], { timeout: 240000 });
      } catch (error) {
        if (error.exitCode === 2) return 2;
        throw error;
      }
      return;
    }
    if(this.layer==='e2e') {
      for (const key of ['FSHARP_E2E_CODE', 'FSHARP_E2E_VSIX', 'FSHARP_E2E_OUTPUT']) if (!process.env[key]) throw new Error(`${key} is required for the immutable packaged editor gate.`);
      await exec('extension/test/with-display.sh', ['python3', 'extension/test/e2e/run.py',
        '--code', process.env.FSHARP_E2E_CODE, '--vsix', process.env.FSHARP_E2E_VSIX,
        '--manifest', process.env.FSHARP_E2E_MANIFEST || path.join(root, 'extension/test/e2e/manifest.json'), '--output', process.env.FSHARP_E2E_OUTPUT], { timeout: 240000 });
      return;
    }
    if(this.layer==='unit') return check();
    if (this.layer === 'decompiler') { await buildDecompiler(); await exec('python3', ['semantic/Decompiler/test.py']); return; }
    if (this.layer === 'helper') { await exec('cargo', ['test', '--workspace', '--locked', 'helper::tests']); return; }
    if (this.layer === 'safety') {
      await build();
      await exec('node', ['--test', 'extension/test/nativeSafety.test.cjs'], {
        env: { ...process.env, FSHARP_BACKEND_PATH: path.join(root, 'target/release/fsharp-navigator') },
      });
      return;
    }
    if (this.layer === 'adapter') {
      await build();
      await exec('node', ['--test', 'extension/test/nativeAdapter.test.cjs',
        'extension/test/nativeCancellation.test.cjs', 'extension/test/localLexicalNative.test.cjs', 'extension/test/nativeSafety.test.cjs'], {
        env: { ...process.env, FSHARP_BACKEND_PATH: path.join(root, 'target/release/fsharp-navigator') },
      });
      return;
    }
    if(this.layer==='integration'){await build();return testHost(false);}
    if(this.layer==='ui'){await build();return testHost(true);}
    throw new Error('Expected unit, decompiler, helper, safety, installer, corpus (pinned real-source engine gate), e2e, adapter (mocked editor API with real Rust), integration, or ui.');
  }
}
export class VerifyCommand extends Command {
  static paths=[['verify']];
  async execute(){await doctor();await auditDependencies();await check();await build();await testHost(false);await testHost(true);}
}
export async function auditDependencies() {
  await exec('python3', ['scripts/dependency-age.py', '--audit-nuget']);
  await exec('cargo', ['audit', '--deny', 'warnings']);
  await exec('cargo', ['audit', '--file', 'evaluation/experiments/local-lexical-proof/implementation/Cargo.lock', '--deny', 'warnings']);
  await npm('audit', '--audit-level=low');
}
export class UpdateDependenciesCommand extends Command {
  static paths = [['dependencies', 'update']];
  async execute() {
    await exec('cargo', ['update']);
    await auditDependencies();
  }
}
export class AuditDependenciesCommand extends Command {
  static paths = [['dependencies', 'audit']];
  async execute() { await auditDependencies(); }
}
export class DependenciesCommand extends Command {
  static paths=[['dependencies','verify']];
  async execute(){
    const directory=path.join(root,'.cache','dependency-verification');await mkdir(directory,{recursive:true});
    await writeFile(path.join(directory,'package.json'),'{"private":true}');
    let blocked=false;
    try { await exec('npm',['install','safe-chain-test','--ignore-scripts','--cache',path.join(directory,'probe-cache')],{cwd:directory,capture:true}); }
    catch(error){if(!/malware|malicious|blocked/i.test(error.message))throw error;blocked=true;}
    if(!blocked)throw new Error('Safe Chain did not block its harmless malware fixture.');
    for(const file of ['package.json','package-lock.json','.npmrc'])await copyFile(path.join(root,file),path.join(directory,file));
    await exec('npm',['ci','--ignore-scripts','--cache',path.join(directory,'fresh-cache')],{cwd:directory});
    await exec('npm',['audit','--audit-level=low'],{cwd:directory});
    await auditDependencies();
  }
}
export class EngineCommand extends Command {
  static paths=[['engine']];
  args=Option.Proxy();
  async execute(){const binary=path.join(root,'target/release/fsharp-navigator');await exec(binary,this.args,{cwd:process.env.FN_INVOCATION_CWD||root});}
}
