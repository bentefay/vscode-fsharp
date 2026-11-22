import { test } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { Cli } from 'clipanion';
import { PackageCommand } from '../cli/commands/distribution.mjs';
import { packageOutputPath, vsixPath } from '../cli/lib/package.mjs';
import { root } from '../cli/lib/process.mjs';

const cli = new Cli({ binaryName: 'run' });
cli.register(PackageCommand);
test('package without output preserves the existing default destination', () => {
  const command = cli.process(['package']);
  assert.equal(command.output, undefined);
  assert.equal(packageOutputPath(command.output), vsixPath);
});
test('explicit package output is parsed literally and resolves from invocation cwd', () => {
  const previous = process.env.FN_INVOCATION_CWD;
  process.env.FN_INVOCATION_CWD = '/tmp/package invocation';
  try {
    const command = cli.process(['package', '--output', 'references evaluation/build.vsix']);
    assert.equal(command.output, 'references evaluation/build.vsix');
    assert.equal(packageOutputPath(command.output), '/tmp/package invocation/references evaluation/build.vsix');
    assert.notEqual(packageOutputPath(command.output), vsixPath);
    assert.equal(packageOutputPath('/tmp/absolute.vsix'), '/tmp/absolute.vsix');
  } finally {
    if (previous === undefined) delete process.env.FN_INVOCATION_CWD;
    else process.env.FN_INVOCATION_CWD = previous;
  }
});
test('programmatic relative output uses repository root when no invocation cwd exists', () => {
  const previous = process.env.FN_INVOCATION_CWD;
  delete process.env.FN_INVOCATION_CWD;
  try { assert.equal(packageOutputPath('evaluation.vsix'), path.join(root, 'evaluation.vsix')); }
  finally { if (previous !== undefined) process.env.FN_INVOCATION_CWD = previous; }
});
