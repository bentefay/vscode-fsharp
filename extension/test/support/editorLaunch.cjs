'use strict';

// Launch directly: @vscode/test-electron's runTests injects --no-sandbox and
// --disable-gpu-sandbox even when the caller omits them.
exports.editorArguments = function (workspace, profileArgs, developmentPath, testPath, untrusted) {
  if (!profileArgs.includes('--user-data-dir') || !profileArgs.includes('--extensions-dir')) throw new Error('Disposable editor directories are required.');
  if (profileArgs.some(arg => /sandbox|disable-web-security/.test(arg))) throw new Error('Sandbox overrides are forbidden.');
  return [workspace, ...profileArgs, '--force-disable-user-env',
    '--skip-welcome', '--skip-release-notes', '--disable-updates', '--disable-telemetry', '--disable-gpu',
    ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []),
    ...(untrusted ? [] : ['--disable-workspace-trust']),
    `--extensionDevelopmentPath=${developmentPath}`, `--extensionTestsPath=${testPath}`];
};

exports.editorEnvironment = function (source) {
  return Object.fromEntries(Object.entries(source).filter(([key]) =>
    !/^(VSCODE_|ELECTRON_|NODE_OPTIONS$)/.test(key) &&
    !/(^|_)(TOKEN|PASSWORD|PASSWD|SECRET|CREDENTIAL|AUTHORIZATION|API_KEY|COOKIE)($|_)/.test(key) &&
    !['SSH_AUTH_SOCK', 'GIT_ASKPASS', 'SSH_ASKPASS', 'BASH_ENV', 'ENV'].includes(key)));
};
