import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import os from 'node:os';
import { mkdtemp, writeFile, readFile, mkdir, readdir, rm, symlink, chmod } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { isolatedProfile } from '../cli/lib/isolated-profile.mjs';

const repo = path.resolve(import.meta.dirname, '..');
async function fixture(t) {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'navigator-installer-'));
  t.after(() => rm(temp, { recursive: true, force: true }));
  // Only the mock CLI receives this non-executable package fixture. No build is needed.
  const vsix = path.join(temp, 'mock package.vsix');
  const made = spawnSync('python3', ['-c', `import json,sys,zipfile,pathlib
root=pathlib.Path(sys.argv[1]); manifest=json.loads((root/'package.json').read_text())
license=(root/'vendor/tree-sitter-fsharp/LICENSE').read_text().strip()
with zipfile.ZipFile(sys.argv[2],'w') as z:
 z.writestr('extension/package.json',json.dumps(manifest))
 z.writestr('extension/THIRD_PARTY_NOTICES.md','## tree-sitter-fsharp 0.3.13\\n'+license)
 for name in ['extension','backend','overlays','trace','virtualDocuments']:z.writestr('extension/extension/dist/'+name+'.js','// mock only')
 info=zipfile.ZipInfo('extension/bin/linux-x64/fsharp-navigator');info.external_attr=0o100755<<16;z.writestr(info,b'mock only')
`, repo, vsix], { encoding: 'utf8' });
  assert.equal(made.status, 0, made.stderr);
  const directory = path.join(temp, 'isolated profile');
  const mock = path.join(temp, 'mock code');
  await writeFile(mock, `#!/usr/bin/env node
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const args=process.argv.slice(2),dir=process.env.MOCK_PROFILE;
assert(args.includes('--force-disable-user-env'));
for(const [flag,child]of [['--user-data-dir','user-data'],['--extensions-dir','extensions']])assert.equal(args[args.indexOf(flag)+1],path.join(dir,child));
for(const key of ['VSCODE_PORTABLE','VSCODE_APPDATA','VSCODE_IPC_HOOK_CLI','NODE_OPTIONS','GH_TOKEN'])assert.equal(process.env[key],undefined);
assert.equal(process.env.HOME,process.env.MOCK_ORIGINAL_HOME);
for(const [key,child]of [['XDG_CACHE_HOME','cache'],['XDG_CONFIG_HOME','config'],['XDG_DATA_HOME','data'],['XDG_STATE_HOME','state'],['XDG_RUNTIME_DIR','runtime'],['TMPDIR','tmp']]){assert.equal(process.env[key],path.join(dir,child));fs.writeFileSync(path.join(process.env[key],'mock-write'),'isolated');}
fs.appendFileSync(path.join(dir,'mock-cli.jsonl'),JSON.stringify(args)+'\\n');
const state=path.join(dir,'extensions','installed.json');
if(args.includes('--install-extension')){assert(args.includes('--do-not-include-pack-dependencies'));fs.writeFileSync(state,JSON.stringify({id:'bentefay.fsharp-navigator'}));}
else if(args.includes('--uninstall-extension')){assert.equal(args.at(-1),'bentefay.fsharp-navigator');fs.unlinkSync(state);}
else throw Error('Unexpected CLI operation or editor launch');
if(process.env.MOCK_FAIL)process.exit(7);
`);
  await chmod(mock, 0o755);
  const defaults = path.join(temp, 'caller-defaults'); await mkdir(defaults); await writeFile(path.join(defaults, 'sentinel'), 'untouched');
  const env = { ...process.env, FN_LOCAL_TOOLS: '1', FN_INVOCATION_CWD: temp,
    MOCK_PROFILE: directory, MOCK_ORIGINAL_HOME: process.env.HOME,
    VSCODE_PORTABLE: defaults, VSCODE_APPDATA: defaults, VSCODE_IPC_HOOK_CLI: 'do-not-forward',
    GH_TOKEN: 'mock-only-secret', NODE_OPTIONS: '--no-warnings', XDG_CACHE_HOME: defaults, XDG_CONFIG_HOME: defaults };
  const run = (...args) => spawnSync(path.join(repo, 'cli/bin/run'), args, { cwd: repo, env, encoding: 'utf8' });
  return { directory, mock, defaults, env, run, vsix };
}

test('dry-run uses explicit separate directories and creates no isolated state or editor process', async t => {
  const f = await fixture(t), r = f.run('install','--isolated-dir',f.directory,'--vsix',f.vsix,'--code',f.mock,'--dry-run');
  assert.equal(r.status,0,r.stderr+r.stdout); const plan=JSON.parse(r.stdout);
  assert.equal(plan.build,false);assert.equal(plan.launch,false);
  assert(plan.args.includes('--user-data-dir'));assert(plan.args.includes('--extensions-dir'));
  await assert.rejects(readdir(f.directory),{code:'ENOENT'});
});
test('mock install, uninstall and reinstall stay isolated; uninstall preserves profile data', async t => {
  const f=await fixture(t);
  for(const action of ['install','uninstall','install']){
    const r=f.run(action,'--isolated-dir',f.directory,'--code',f.mock,...(action==='install'?['--vsix',f.vsix]:[]));
    assert.equal(r.status,0,r.stderr+r.stdout);
    if(action==='uninstall')await assert.rejects(readFile(path.join(f.directory,'extensions/installed.json')),{code:'ENOENT'});
    assert.equal(await readFile(path.join(f.defaults,'sentinel'),'utf8'),'untouched');
    assert.deepEqual(await readdir(f.defaults),['sentinel']);
  }
  assert.equal((await readFile(path.join(f.directory,'mock-cli.jsonl'),'utf8')).trim().split('\n').length,3);
  assert.equal(JSON.parse(await readFile(path.join(f.directory,'extensions/installed.json'),'utf8')).id,'bentefay.fsharp-navigator');
});
test('isolated mode refuses missing VSIX and named profile; uninstall cannot fall back to defaults', async t => {
  const f=await fixture(t);
  for(const args of [
    ['install','--isolated-dir',f.directory],
    ['install','--isolated-dir',f.directory,'--vsix',f.vsix,'--profile','normal'],
    ['uninstall'], ['uninstall','--isolated-dir',f.directory],
  ])assert.notEqual(f.run(...args,'--code',f.mock).status,0);
  await assert.rejects(readdir(f.directory),{code:'ENOENT'});
});
test('unowned nonempty and default storage paths are rejected without spawning', async t => {
  const f=await fixture(t);await mkdir(f.directory);await writeFile(path.join(f.directory,'sentinel'),'keep');
  assert.notEqual(f.run('install','--isolated-dir',f.directory,'--vsix',f.vsix,'--code',f.mock).status,0);
  await assert.rejects(isolatedProfile(path.join(process.env.HOME,'.vscode','preview')),/default VS Code/);
  await assert.rejects(isolatedProfile(process.env.HOME),/default VS Code/);
  assert.deepEqual(await readdir(f.directory),['sentinel']);
});
test('directory and child symlinks cannot redirect writes', async t => {
  const f=await fixture(t);await symlink(f.defaults,f.directory);
  await assert.rejects(isolatedProfile(f.directory),/link or file/);await rm(f.directory);
  await isolatedProfile(f.directory,{create:true});await rm(path.join(f.directory,'extensions'),{recursive:true});await symlink(f.defaults,path.join(f.directory,'extensions'));
  assert.notEqual(f.run('uninstall','--isolated-dir',f.directory,'--code',f.mock).status,0);
  assert.deepEqual(await readdir(f.defaults),['sentinel']);
});
test('invalid VSIX fails before profile creation; CLI failures propagate', async t => {
  const f=await fixture(t);let r=f.run('install','--isolated-dir',f.directory,'--code',f.mock,'--vsix',path.join(f.defaults,'sentinel'));
  assert.notEqual(r.status,0);await assert.rejects(readdir(f.directory),{code:'ENOENT'});
  f.env.MOCK_FAIL='1';r=f.run('install','--isolated-dir',f.directory,'--code',f.mock,'--vsix',f.vsix);
  assert.notEqual(r.status,0);assert.match(r.stdout+r.stderr,/failed \(7\)/);
});
test('portable launcher data cannot override the requested isolated directories', async t => {
  const f=await fixture(t);await mkdir(path.join(path.dirname(f.mock),'data'));
  const alias=path.join(path.dirname(f.mock),'code-link');await symlink(f.mock,alias);
  const r=f.run('install','--isolated-dir',f.directory,'--vsix',f.vsix,'--code',alias);
  assert.notEqual(r.status,0);assert.match(r.stdout+r.stderr,/Portable VS Code/);
  await assert.rejects(readdir(f.directory),{code:'ENOENT'});
});
test('existing named-profile install dry-run keeps its original argument contract', async t => {
  const f=await fixture(t),r=f.run('install','--vsix',f.vsix,'--code',f.mock,'--profile','Existing','--force','--dry-run');
  assert.equal(r.status,0,r.stdout+r.stderr);
  assert.deepEqual(JSON.parse(r.stdout),{build:false,command:f.mock,args:['--install-extension',f.vsix,'--profile','Existing','--force']});
  await assert.rejects(readdir(f.directory),{code:'ENOENT'});
});
