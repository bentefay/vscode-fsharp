'use strict';
// Real packaged adapter/daemon boundary; graphical checks use the same authored
// fixture matrix separately. These tests never count as actual editor gestures.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { createEditor } = require('./support/editorDouble.cjs');
const matrix = require('./e2e/document-types.json');
const binary = process.env.FSHARP_BACKEND_PATH;
const plain = rows => rows?.map(x => ({ uri: x.uri.toString(), range: JSON.parse(JSON.stringify(x.range)) }));

for (const scenario of matrix.cases) {
  test(`${scenario.id}: authored cold/warm, dirty, close and restart navigation`, async t => {
    assert.ok(binary && path.isAbsolute(binary));
    const parent = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
    await fs.mkdir(parent, { recursive: true });
    const root = await fs.mkdtemp(path.join(parent, 'fsnav-document-'));
    const fixture = matrix.fixtures.find(x => x.id === scenario.fixture);
    for (const [name, source] of Object.entries(fixture.files)) await fs.writeFile(path.join(root, name), source);
    const editor = await createEditor(root, binary, {
      evaluatedModelPath: path.join(root, 'absent-model.json'),
      libraryCatalogPath: path.join(root, 'absent-catalog.json'),
      builtinEnvironmentPath: path.join(root, 'absent-builtins.json'),
      helperPath: path.join(root, 'absent-helper'),
    });
    t.after(async () => { editor.dispose(); await fs.rm(root, { recursive: true, force: true }); });
    let document = await editor.open(scenario.query.file);
    const authored = scenario.expected.references || [scenario.expected.target];
    const expected = offset => authored.map(x => ({ uri: pathToFileURL(path.join(root, x.file)).toString(), range: {
      start: { line: x.range.start.line + offset, character: x.range.start.character },
      end: { line: x.range.end.line + offset, character: x.range.end.character },
    } }));
    async function query(offset = 0) {
      const cursor = { line: scenario.query.line + offset, character: scenario.query.character };
      const found = scenario.operation === 'references'
        ? await editor.providers.references.provideReferences(document, cursor, { includeDeclaration: true }, editor.uncancelled)
        : await editor.providers.definition.provideDefinition(document, cursor, editor.uncancelled);
      assert.deepEqual(plain(found), expected(offset));
      assert.equal(editor.api.lastResult.completeness, 'exact');
      assert.equal(editor.api.lastResult.proofScope, 'document');
      assert.equal(editor.api.lastResult.documentVersion, document.version);
      return editor.api.lastResult.sourceHash;
    }
    const original = await query();
    assert.equal(await query(), original);
    editor.edit(document, '\n' + fixture.files[scenario.query.file]);
    assert.notEqual(await query(1), original);
    editor.api.restart();
    assert.notEqual(await query(1), original, 'Restart must use dirty buffer');
    editor.close(document);
    document = await editor.open(scenario.query.file);
    assert.equal(await query(), original, 'Close discards unsaved overlay');
    const [stats] = await editor.api.stats();
    for (const key of ['modelLoadMicros', 'filesRead', 'filesParsed', 'compilerLaunches', 'helperStarts', 'decompilerStarts']) assert.equal(stats[key], 0, key);
    assert.ok(stats.localProofParsedBytes > 0);
  });
}
