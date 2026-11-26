const { test } = require('node:test');
const assert = require('node:assert/strict');
const { editorArguments, editorEnvironment } = require('./support/editorLaunch.cjs');

test('editor launch preserves sandbox and requires private profiles for both trust states', () => {
  for (const untrusted of [false, true]) {
    const args = editorArguments('/fixture', ['--user-data-dir', '/private/user', '--extensions-dir', '/private/extensions'], '/probe', '/tests', untrusted);
    assert.ok(!args.some(arg => /sandbox/.test(arg)));
    assert.equal(args.includes('--disable-workspace-trust'), !untrusted);
    assert.ok(args.includes('--force-disable-user-env'));
  }
  assert.throws(() => editorArguments('/fixture', [], '/probe', '/tests', false));
  assert.throws(() => editorArguments('/fixture', ['--user-data-dir', '/u', '--extensions-dir', '/e', '--no-sandbox'], '/probe', '/tests', false));
});

test('editor child cannot inherit sandbox overrides, Node injection or credentials', () => {
  assert.deepEqual(editorEnvironment({ PATH: '/bin', HOME: '/home/test', ELECTRON_DISABLE_SANDBOX: '1', NODE_OPTIONS: '--require injected', AWS_SECRET_ACCESS_KEY: 'secret', GH_TOKEN: 'secret', BASH_ENV: '/injected', VSCODE_IPC_HOOK_CLI: 'normal-profile' }), { PATH: '/bin', HOME: '/home/test' });
});
