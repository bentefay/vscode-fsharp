'use strict';
// This suite executes
// the shipped adapter and a real native process through an editor API double.
// It neither starts VS Code nor measures editor activation latency.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { createEditor } = require('./support/editorDouble.cjs');

const binary = process.env.FSHARP_BACKEND_PATH;
const initial = 'module M\nlet values = List.map (fun x -> x + x) [1; 2]\n';

async function setup(t, source = initial) {
  assert.ok(binary && path.isAbsolute(binary), 'Select an immutable candidate explicitly with FSHARP_BACKEND_PATH');
  const parent = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(parent, { recursive: true });
  const directory = await fs.mkdtemp(path.join(parent, 'fsnav-local-proof-'));
  await fs.writeFile(path.join(directory, 'Main.fs'), source);
  await fs.writeFile(path.join(directory, 'Dependency.fs'), 'module Dependency\nlet untouched = 1\n');
  // If the local route accidentally asks for any prepared/project input it
  // fails visibly. These are not synthetic accepted certificates.
  const editor = await createEditor(directory, binary, {
    evaluatedModelPath: path.join(directory, 'intentionally-absent-model.json'),
    libraryCatalogPath: path.join(directory, 'intentionally-absent-catalog.json'),
    builtinEnvironmentPath: path.join(directory, 'intentionally-absent-environment.json'),
    helperPath: path.join(directory, 'intentionally-absent-helper'),
  });
  editor.directory = directory;
  t.after(async () => { editor.dispose(); await fs.rm(directory, { recursive: true, force: true }); });
  return editor;
}

function position(document, needle, last = true) {
  const source = document.getText();
  const offset = last ? source.lastIndexOf(needle) : source.indexOf(needle);
  assert.ok(offset >= 0, `Missing ${needle}`);
  const before = source.slice(0, offset).split('\n');
  return { line: before.length - 1, character: before.at(-1).length };
}

function ranges(locations) {
  return locations?.map(row => [row.range.start.line, row.range.start.character, row.range.end.line, row.range.end.character]);
}

function proof(editor, document) {
  const result = editor.api.lastResult;
  assert.equal(result?.completeness, 'exact');
  assert.equal(result.source, 'local-lexical');
  assert.equal(result.proofScope, 'document');
  assert.equal(result.version, document.version);
  assert.equal(result.documentVersion, document.version);
  assert.match(result.sourceHash, /^[0-9a-f]{64}$/);
  assert.match(result.documentProofId, /^[0-9a-f]{64}$/);
  assert.equal(Object.hasOwn(result, 'projectContextId'), false, 'A document proof cannot fabricate a project identity');
  return { sourceHash: result.sourceHash, documentProofId: result.documentProofId };
}

async function assertNoProjectWork(editor) {
  const [stats] = await editor.api.stats();
  for (const key of ['modelLoadMicros', 'catalogLoadMicros', 'filesRead', 'bytesRead', 'filesParsed', 'sourceFileCount', 'compilerLaunches', 'helperStarts', 'decompilerStarts']) {
    assert.equal(stats[key], 0, `${key}: ${JSON.stringify(stats)}`);
  }
  assert.equal(stats.builtinEnvironmentLoaded, false);
  assert.equal(stats.watcherActive, false);
  assert.ok(Object.values(stats.modelValidation).every(value => value === 0));
  assert.ok(stats.localProofHits > 0);
  assert.ok(stats.localProofParsedBytes > 0, 'Transient local parsing must be counted, not hidden in zero general AST counters');
  return stats;
}

test('activation and clean tabs stay lazy, then local provider answers without project/tree demand', async t => {
  const editor = await setup(t);
  assert.deepEqual(editor.api.processIds, []);
  const document = await editor.open('Main.fs');
  await editor.open('Dependency.fs');
  assert.deepEqual(editor.api.processIds, [], 'No tree.getChildren call is made by activation in this editor double');
  const cursor = position(document, 'x)');
  const definition = await editor.providers.definition.provideDefinition(document, cursor, editor.uncancelled);
  assert.deepEqual(ranges(definition), [[1, 27, 1, 28]]);
  const first = proof(editor, document);
  const inclusive = await editor.providers.references.provideReferences(document, cursor, { includeDeclaration: true }, editor.uncancelled);
  assert.deepEqual(ranges(inclusive), [[1, 27, 1, 28], [1, 32, 1, 33], [1, 36, 1, 37]]);
  assert.equal(proof(editor, document).sourceHash, first.sourceHash);
  const uses = await editor.providers.references.provideReferences(document, cursor, { includeDeclaration: false }, editor.uncancelled);
  assert.deepEqual(ranges(uses), [[1, 32, 1, 33], [1, 36, 1, 37]]);
  proof(editor, document);
  await assertNoProjectWork(editor);
});

test('current overlay and shadowing replace the local proof while dirty dependencies stay unparsed', async t => {
  const editor = await setup(t);
  const document = await editor.open('Main.fs');
  const dependency = await editor.open('Dependency.fs');
  const refs = () => editor.providers.references.provideReferences(document, position(document, 'x in'), { includeDeclaration: true }, editor.uncancelled);
  await editor.providers.definition.provideDefinition(document, position(document, 'x)'), editor.uncancelled);
  const before = proof(editor, document);
  editor.edit(dependency, 'This dirty dependency is deliberately invalid and irrelevant to the local binding.');
  editor.edit(document, 'module M\r\nlet values = List.map (fun x -> let original=x in let x=original+1 in x) [1]\r\n');
  const found = await refs();
  assert.deepEqual(ranges(found), [[1, 27, 1, 28], [1, 45, 1, 46]]);
  const after = proof(editor, document);
  assert.notEqual(after.sourceHash, before.sourceHash);
  assert.notEqual(after.documentProofId, before.documentProofId);
  await assertNoProjectWork(editor);
});

test('UTF16 source positions, warm requests and restart use acknowledged source identities', async t => {
  const source = 'module M\r\nlet values = List.map (fun x -> ("😀",x)) [1]\r\n';
  const editor = await setup(t, source);
  const document = await editor.open('Main.fs');
  const query = () => editor.providers.references.provideReferences(document, position(document, 'x))'), { includeDeclaration: true }, editor.uncancelled);
  const expected = [[1, 27, 1, 28], [1, 38, 1, 39]];
  assert.deepEqual(ranges(await query()), expected);
  const first = proof(editor, document);
  assert.deepEqual(ranges(await query()), expected);
  assert.equal(proof(editor, document).sourceHash, first.sourceHash);
  const warm = await assertNoProjectWork(editor);
  assert.equal(warm.localProofCacheHits, 1);
  assert.equal(warm.localProofParsedBytes, Buffer.byteLength(source));
  editor.api.restart();
  assert.deepEqual(editor.api.processIds, []);
  assert.deepEqual(ranges(await query()), expected);
  assert.equal(proof(editor, document).sourceHash, first.sourceHash);
  const restarted = await assertNoProjectWork(editor);
  assert.equal(restarted.localProofCacheHits, 0);
  assert.equal(restarted.localProofParsedBytes, Buffer.byteLength(source));
});

test('edit while initialization is in flight withholds the old provider response', async t => {
  const editor = await setup(t);
  const document = await editor.open('Main.fs');
  const old = editor.providers.definition.provideDefinition(document, position(document, 'x)'), editor.uncancelled);
  editor.edit(document, 'module M\nlet values = List.map (fun renamed -> renamed) [1]\n');
  assert.equal(await old, undefined);
  assert.equal(editor.api.lastResult, undefined);
  const current = await editor.providers.definition.provideDefinition(document, position(document, 'renamed)'), editor.uncancelled);
  assert.deepEqual(ranges(current), [[1, 27, 1, 34]]);
  proof(editor, document);
  await assertNoProjectWork(editor);
});

for (const [name, source, token] of [
  ['exported parameter', 'module M\nlet exported x=x+x\n', 'x+x'],
  ['direct RHS lambda', 'module M\nlet exported=fun x -> x+x\n', 'x+x'],
  ['returned lambda', 'module M\nlet exported ()=fun x -> x+x\n', 'x+x'],
  ['ambiguous named argument', 'module M\nlet values=List.map(fun x -> C.M(x=x)) [1]\n', 'x))'],
  ['compound named argument', 'module M\ntype C = static member M(x:bool) = x\nlet values=List.map(fun x -> C.M(x=not x)) [true]\n', 'x=not'],
  ['directive', '#if DEBUG\nlet values=List.map(fun x -> x+x) [1]\n#endif\n', 'x+x'],
  ['parse error', 'module M\nlet broken = [1\nlet values=List.map(fun x -> x+x) [1]\n', 'x+x'],
  ['method parameter', 'module M\ntype C=static member M(x:int)=x+x\n', 'x+x'],
]) {
  test(`${name} does not become a complete document-local answer`, async t => {
    const editor = await setup(t, source);
    const document = await editor.open('Main.fs');
    const cursor = position(document, token);
    const result = await editor.providers.references.provideReferences(document, cursor, { includeDeclaration: true }, editor.uncancelled);
    assert.equal(result, undefined, 'The normal path has an intentionally inaccessible model, so unsupported local shapes must not return exact');
    assert.notEqual(editor.api.lastResult?.source, 'local-lexical');
    const [stats] = await editor.api.stats();
    assert.equal(stats.localProofHits, 0);
    assert.equal(stats.helperStarts, 0);
  });
}
