'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { performance } = require('node:perf_hooks');
const vscode = require('vscode');

// Real registered providers, all UTF-16 ranges checked. The first request can include startup
// work. Restart exercises a new daemon against the same disposable editor storage.
exports.exerciseLocalNavigation = async function (doc, extension, samples) {
  const line = doc.lineCount - 2;
  const position = text => {
    const column = doc.lineAt(line).text.indexOf(text);
    assert.ok(column >= 0, text);
    return new vscode.Position(line, column);
  };
  const query = async (stage, method) => {
    const cursor = position('x)');
    const declaration = position('x ->');
    const uses = [position('x +'), cursor];
    const locations = await vscode.commands.executeCommand(`vscode.execute${method}Provider`, doc.uri, cursor);
    const actual = (locations || []).map(location => {
      const range = location.range || location.targetSelectionRange;
      assert.equal((location.uri || location.targetUri).toString(), doc.uri.toString());
      assert.equal(range.start.line, line);
      assert.equal(range.end.line, line);
      assert.equal(range.end.character, range.start.character + 1);
      return range.start.character;
    }).sort((a, b) => a - b);
    const expected = (method === 'Definition' ? [declaration] : [declaration, ...uses]).map(p => p.character);
    assert.deepEqual(actual, expected, `${stage}: ${JSON.stringify(extension.isActive && extension.exports.lastResult)}`);
    assert.equal(extension.exports.lastResult.completeness, 'exact');
    assert.equal(extension.exports.lastResult.version, doc.version);
  };
  const sample = async (stage, method, action) => {
    const start = performance.now();
    if (action) await action();
    await query(stage, method);
    samples.push({ stage, method, elapsedMs: performance.now() - start, documentVersion: doc.version, source: extension.exports.lastResult.source });
  };
  const firstPids = extension.isActive ? extension.exports.processIds : [];
  await sample('first', 'Definition');
  samples[0].existingBackendCount = firstPids.length;
  await sample('coldReferences', 'Reference');
  for (let i = 0; i < 8; i++) await sample('warm', 'Definition');
  const insertion = position('fun x');
  const edit = new vscode.WorkspaceEdit();
  edit.insert(doc.uri, insertion, ' ');
  try {
    await sample('dirtyEditToAnswer', 'Definition', async () => assert.equal(await vscode.workspace.applyEdit(edit), true));
    await sample('dirtyReferences', 'Reference');
    const prior = extension.exports.processIds;
    await sample('backendRestartToAnswer', 'Definition', () => vscode.commands.executeCommand('fsharpNavigator.restart'));
    assert.ok(extension.exports.processIds.length && prior.every(pid => !extension.exports.processIds.includes(pid)));
  } finally {
    await vscode.window.showTextDocument(doc);
    await vscode.commands.executeCommand('workbench.action.files.revert');
  }
};


exports.exerciseSingleUseReferences = async function (root, api) {
  const file = path.join(root, 'SingleUseProbe.fs');
  const line = 'let values = List.map (fun param -> param) [1]';
  await fs.writeFile(file, 'module SingleUseProbe\n' + line + '\n', { flag: 'wx' });
  const uri = vscode.Uri.file(file);
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  const start = line.indexOf('param'), use = line.lastIndexOf('param');
  const cursor = new vscode.Position(1, use);
  const ranges = locations => locations.map(location => {
    const range = location.range || location.targetSelectionRange;
    assert.equal((location.uri || location.targetUri).toString(), uri.toString());
    return [range.start.line, range.start.character, range.end.line, range.end.character];
  }).sort((a, b) => a[1] - b[1]);
  const location = column => [1, column, 1, column + 'param'.length];
  const definition = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', uri, cursor);
  assert.deepEqual(ranges(definition), [location(start)]);
  assert.equal(api.lastResult.source, 'local-lexical');
  const [before] = await api.stats();
  assert.ok(before.localProofParsedBytes > 0);
  const references = await vscode.commands.executeCommand('vscode.executeReferenceProvider', uri, cursor);
  assert.deepEqual(ranges(references), [location(start), location(use)]);
  assert.equal(api.lastResult.completeness, 'exact');
  const [after] = await api.stats();
  assert.equal(after.localProofParsedBytes, before.localProofParsedBytes);
  assert.ok(after.localProofCacheHits > before.localProofCacheHits);
};
