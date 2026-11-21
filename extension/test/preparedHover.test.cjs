'use strict';
// Actual adapter + actual native process + genuine offline compiler artifacts.
// The editor boundary is a double; this is explicitly not a VS Code host test.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { createEditor } = require('./support/editorDouble.cjs');
const prepared = process.env.FSHARP_PREPARED_FIXTURE;
const binary = process.env.FSHARP_BACKEND_PATH;

test('prepared native hover reaches the adapter only after exact current ownership proof', { skip: !prepared || !binary }, async t => {
  const manifest = JSON.parse(await fs.readFile(path.join(prepared, 'manifest.json'), 'utf8'));
  for (const [name, expected] of [['positive', 'external'], ['unopened_module', 'external'], ['unopened_value', 'external'], ['module', 'blocked'], ['value', 'blocked'], ['assembly_auto_open', 'blocked'], ['source_module', 'source'], ['source_value', 'blocked']]) {
    await t.test(name, async () => {
      const program = manifest.programs[name];
      assert.ok(program, `Missing genuine fixture ${name}`);
      const editor = await createEditor(program.root, binary, { libraryCatalogPath: manifest.catalog, evaluatedModelPath: program.model, builtinEnvironmentPath: program.environment });
      try {
        const document = await editor.open('Main.fs');
        const result = await editor.providers.hover.provideHover(document, editor.position(document, program.params.line, 'IsNullOrEmpty'), editor.uncancelled);
        if (expected === 'external') {
          assert.ok(result, JSON.stringify(editor.api.lastResult));
          assert.equal(editor.api.lastResult.completeness, 'exact');
          assert.equal(editor.api.lastResult.symbol.assemblyName, 'System.Runtime');
          assert.equal(result.contents.isTrusted, false);
        } else {
          assert.equal(result, undefined, JSON.stringify(editor.api.lastResult));
          assert.equal(editor.api.lastResult.completeness, expected === 'source' ? 'exact' : 'unavailable');
          if (expected === 'blocked') assert.equal(editor.status.text, 'F#: incomplete');
        }
        const [stats] = await editor.api.stats();
        assert.equal(stats.helperStarts, 0);
        assert.equal(stats.compilerLaunches, 0);
      } finally { editor.dispose(); }
    });
  }
});
