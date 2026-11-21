'use strict';
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs/promises');
const path = require('node:path');

// Protocol scaffolding for synthetic project-context tests, not a real .NET
// globalization certificate. Actual exported witnesses are tested separately.
module.exports = async function syntheticProjectCertificate(directory) {
  const source = await fs.readFile(path.resolve(__dirname, '../../..', 'crates/navigator-core/src/environment.rs'), 'utf8');
  const selectors = ['GLOBALIZATION_SELECTORS', 'HOST_SELECTORS'].flatMap(name => {
    const declaration = source.match(new RegExp(`pub const ${name}: &\\[&str\\] = &\\[([\\s\\S]*?)\\];`));
    assert.ok(declaration, `Missing native certificate selectors: ${name}`);
    return [...declaration[1].matchAll(/"([^"\n]+)"/g)].map(match => match[1]);
  });
  assert.equal(new Set(selectors).size, selectors.length);
  const hash = value => crypto.createHash('sha256').update(value).digest('hex');
  const marker = path.join(directory, 'synthetic-runtime-marker');
  const selection = path.join(directory, 'synthetic-empty-selection');
  const markerBytes = Buffer.from('synthetic adapter test runtime');
  await fs.writeFile(marker, markerBytes);
  await fs.mkdir(selection);
  return {
    version: 4, contract: 'dotnet-msbuild-observed-v4', toolchain: '0'.repeat(64),
    inputs: selectors.map(name => ({ name, sha256: Object.hasOwn(process.env, name) ? hash(process.env[name]) : 'missing' })),
    traceInvocations: 2, positiveReads: 0, negativeReads: 0,
    globalization: {
      profile: 'linux-x64-dotnet10-icu-v1', runtimeVersion: '10.0.11', loaderVersion: '2.41',
      cultureName: '', uiCultureName: '', libcMessagesLocale: 'C', sortVersion: 0, sortId: '00000000-0000-0000-0000-000000000000',
      files: [{ path: marker, sha256: hash(markerBytes) }],
      selectionDirectories: [{ path: selection, exists: true, sha256: hash(Buffer.alloc(0)), entries: 0 }],
      pathLinks: [{ path: directory, target: null }]
    }
  };
};
