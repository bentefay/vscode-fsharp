#!/usr/bin/env node
'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { runTests, downloadAndUnzipVSCode } = require('@vscode/test-electron');
const fixture = require('./fixtures.cjs');
const { prepareWorkspace } = require('./prepareWorkspace.cjs');

const root = path.resolve(__dirname, '../..');
const version = process.env.FSHARP_VSCODE_VERSION || '1.108.0';

function run(binary, args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { env, stdio: 'inherit' });
    child.on('error', reject);
    child.on('exit', code => code === 0 ? resolve() : reject(new Error(`Editor exited ${code}`)));
  });
}

async function main() {
  const tempRoot = process.env.TMPDIR || path.join(root, '.cache', 'test-tmp');
  await fs.mkdir(tempRoot, { recursive: true });
  const temporary = await fs.mkdtemp(path.join(tempRoot, 'fsnav-test-'));
  const workspace = path.join(temporary, 'workspace');
  const userData = path.join(temporary, 'user-data');
  const extensions = path.join(temporary, 'extensions');
  await fs.mkdir(workspace, { recursive: true });
  await fs.mkdir(path.join(userData, 'User'), { recursive: true });
  await fs.mkdir(extensions);
  for (const [name, content] of Object.entries(fixture.files)) await fs.writeFile(path.join(workspace, name), content);
  const settings = {
    'telemetry.telemetryLevel': 'off', 'update.mode': 'none', 'extensions.autoCheckUpdates': false,
    'extensions.autoUpdate': false, 'workbench.enableExperiments': false,
    'workbench.startupEditor': 'none', 'window.restoreWindows': 'none',
    'security.workspace.trust.startupPrompt': 'never',
    'fsharpNavigator.mode': 'indexed',
  };
  if (!process.env.FSHARP_TEST_VSIX && process.env.FSHARP_BACKEND_PATH) settings['fsharpNavigator.backendPath'] = path.resolve(process.env.FSHARP_BACKEND_PATH);
  const env = { ...process.env, FSHARP_TEST_WORKSPACE: workspace, FSHARP_TEST_RESULTS: temporary, FSHARP_TEST_UNTRUSTED: process.env.FSHARP_TEST_UNTRUSTED || '0' };
  for (const key of Object.keys(env)) {
    if (/^(VSCODE_|ELECTRON_|NODE_OPTIONS$)/.test(key) || /^(GITHUB_TOKEN|GH_TOKEN|VSCE_PAT|SSH_AUTH_SOCK|GIT_ASKPASS|SSH_ASKPASS)$/.test(key)) {
      // test-electron merges this object over process.env before spawning Code.
      delete env[key];
      delete process.env[key];
    }
  }
  const binary = process.env.FSHARP_VSCODE_PATH || await downloadAndUnzipVSCode(version);
  const args = [workspace, '--user-data-dir', userData, '--extensions-dir', extensions,
    '--skip-welcome', '--skip-release-notes', '--disable-updates', '--disable-telemetry', '--no-sandbox', '--disable-gpu',
    ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []),
  ];
  if (Number(version.split('.')[1]) >= 131) args.push('--shared-data-dir', path.join(temporary, 'shared-data'));
  const testPath = path.join(__dirname, 'suite.cjs');
  try {
    if (env.FSHARP_TEST_UNTRUSTED !== '1') Object.assign(settings, await prepareWorkspace(temporary, workspace, root, env));
    await fs.writeFile(path.join(userData, 'User/settings.json'), JSON.stringify(settings));
    let developmentPath = root;
    if (process.env.FSHARP_TEST_VSIX) {
      const launcher = path.join(path.dirname(binary), 'bin', 'code');
      await run(launcher, ['--user-data-dir', userData, '--extensions-dir', extensions, '--install-extension', path.resolve(process.env.FSHARP_TEST_VSIX), '--force'], env);
      const installed = JSON.parse(await fs.readFile(path.join(extensions, 'extensions.json'), 'utf8'));
      if (!installed.some(entry => entry.identifier.id === 'bentefay.fsharp-navigator')) throw new Error('The disposable profile lacks the tested extension.');
      developmentPath = path.join(temporary, 'probe');
      await fs.mkdir(developmentPath);
      await fs.writeFile(path.join(developmentPath, 'package.json'), JSON.stringify({ name: 'fsharp-navigator-test-probe', version: '0.0.0', publisher: 'test', engines: { vscode: '^1.108.0' }, main: 'index.js', activationEvents: ['*'] }));
      await fs.writeFile(path.join(developmentPath, 'index.js'), 'exports.activate = () => {};');
    }
    if (env.FSHARP_TEST_UNTRUSTED === '1') {
      // test-electron adds --disable-workspace-trust. Launch the same binary directly.
      await run(binary, [...args, `--extensionDevelopmentPath=${developmentPath}`, `--extensionTestsPath=${testPath}`], env);
    } else {
      await runTests({ vscodeExecutablePath: binary, extensionDevelopmentPath: developmentPath, extensionTestsPath: testPath, extensionTestsEnv: env, launchArgs: args });
    }
  } finally {
    const evidence = path.join(root, 'test-results', `extension-${version}-${path.basename(temporary)}`);
    await fs.mkdir(path.dirname(evidence), { recursive: true });
    await fs.cp(temporary, evidence, { recursive: true, filter: async source => {
      const stat = await fs.lstat(source);
      return stat.isDirectory() || stat.isFile() || stat.isSymbolicLink();
    } });
    await fs.rm(temporary, { recursive: true, force: true });
    console.log(`Test evidence: ${evidence}`);
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
