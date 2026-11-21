'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { performance } = require('node:perf_hooks');
const vscode = require('vscode');
const { exerciseLocalNavigation } = require('./localLifecycle.cjs');

exports.run = async function () {
  const results = [];
  const providerSamples = [];
  const localLifecycle = [];
  const root = process.env.FSHARP_TEST_WORKSPACE;
  const extension = vscode.extensions.getExtension('bentefay.fsharp-navigator');
  const untrusted = process.env.FSHARP_TEST_UNTRUSTED === '1';
  const packaged = Boolean(process.env.FSHARP_TEST_VSIX);
  if (!untrusted || !packaged) assert.ok(extension, 'Expected the installed or development extension in the host');
  const activation = { alreadyActive: extension?.isActive ?? false, beforeMemory: process.memoryUsage(), startMs: performance.now() };
  const api = untrusted ? undefined : await extension.activate();
  activation.elapsedMs = performance.now() - activation.startMs;
  activation.afterMemory = process.memoryUsage();
  const executeProvider = async (command, ...args) => {
    const start = performance.now();
    try { return await vscode.commands.executeCommand(command, ...args); }
    finally { providerSamples.push({ command, elapsedMs: performance.now() - start }); }
  };
  const uri = name => vscode.Uri.file(path.join(root, name));
  const document = await vscode.workspace.openTextDocument(uri('Main.fs'));
  await vscode.window.showTextDocument(document);
  const position = (doc, line, text, offset = 1) => new vscode.Position(line, doc.lineAt(line).text.lastIndexOf(text) + offset);
  const definitions = async (doc, line, text) => {
    const values = await executeProvider('vscode.executeDefinitionProvider', doc.uri, position(doc, line, text));
    return (values || []).map(value => ({ uri: value.uri || value.targetUri, range: value.range || value.targetSelectionRange }));
  };
  const references = async (doc, line, text) => executeProvider('vscode.executeReferenceProvider', doc.uri, position(doc, line, text));
  const test = async (name, action) => {
    const start = Date.now();
    try { await action(); results.push({ name, status: 'passed', durationMs: Date.now() - start }); }
    catch (error) { results.push({ name, status: 'failed', error: error.stack, durationMs: Date.now() - start }); }
  };
  const eventually = async predicate => {
    const end = Date.now() + 10_000;
    while (Date.now() < end) {
      if (await predicate()) return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error('Timed out waiting for observable editor state');
  };
  const screenshot = async name => {
    if (process.env.FSHARP_CAPTURE_SCREENSHOTS !== '1') return;
    const directory = path.join(process.env.FSHARP_TEST_RESULTS, 'screenshots');
    await fs.mkdir(directory, { recursive: true });
    await promisify(execFile)('import', ['-window', 'root', path.join(directory, name + '.png')]);
  };
  if (untrusted) {
    await test('untrusted workspace starts no native engine', async () => {
      assert.equal(vscode.workspace.isTrusted, false);
      if (extension) assert.equal(extension.packageJSON.capabilities.untrustedWorkspaces.supported, false);
      assert.deepEqual(await definitions(document, 3, 'value'), []);
      if (packaged) assert.ok(!extension?.isActive, 'Installed extension is disabled in Restricted Mode');
      // Code can activate development extensions in a restricted workspace.
      if (extension?.isActive) {
        assert.deepEqual(extension.exports.processIds, []);
        assert.deepEqual(await extension.exports.stats(), []);
      }
    });
  } else {
    await test('local providers preserve exact targets across warm, edit and backend restart', () =>
      exerciseLocalNavigation(document, api, localLifecycle));
    await test('solution explorer preserves declared compile order', async () => {
      await vscode.commands.executeCommand('workbench.view.extension.fsharpNavigator');
      const roots = await api.tree.getChildren();
      const walk = nodes => nodes.flatMap(node => [node, ...walk(node.children || [])]);
      const project = walk(roots).find(node => node.kind === 'project');
      assert.ok(project, JSON.stringify(roots));
      const files = walk(project.children || []).filter(node => /\.(fs|fsi)$/.test(node.path || node.label)).map(node => path.basename(node.path || node.label));
      assert.deepEqual(files, ['Zulu.fs', 'Alpha.fs', 'Members.fs', 'Main.fs']);
      const firstFile = walk(project.children || []).find(node => node.kind === 'file');
      const item = api.tree.getTreeItem(firstFile);
      assert.equal(item.command.command, 'fsharpNavigator.openProjectFile');
      await vscode.commands.executeCommand(item.command.command, ...item.command.arguments);
      await eventually(() => vscode.window.activeTextEditor?.document.uri.fsPath === firstFile.path);
      await vscode.window.showTextDocument(document);
      await screenshot('solution-tree');
    });
    await test('go to definition respects ordered open declarations', async () => {
      const found = await definitions(document, 3, 'value');
      assert.equal(found.length, 1, JSON.stringify(api.lastResult));
      assert.equal(found[0].uri.fsPath, path.join(root, 'Alpha.fs'));
      assert.equal(found[0].range.start.line, 1);
      assert.equal(api.lastResult.completeness, 'exact');
    });
    await test('native Go to Definition opens the target editor', async () => {
      const editor = await vscode.window.showTextDocument(document);
      editor.selection = new vscode.Selection(position(document, 3, 'value'), position(document, 3, 'value'));
      await vscode.commands.executeCommand('editor.action.revealDefinition');
      await eventually(() => vscode.window.activeTextEditor?.document.uri.fsPath === path.join(root, 'Alpha.fs'));
      assert.equal(vscode.window.activeTextEditor.selection.start.line, 1);
      await screenshot('definition-editor');
      await vscode.window.showTextDocument(document);
    });
    await test('go to definition distinguishes local shadowing', async () => {
      const found = await definitions(document, 9, 'shared');
      assert.equal(found.length, 1, JSON.stringify(api.lastResult));
      assert.equal(found[0].range.start.line, 8);
    });
    await test('references return the first local symbol without the second scope', async () => {
      const found = await references(document, 6, 'shared');
      assert.ok(found?.length, JSON.stringify(api.lastResult));
      assert.ok(found.some(value => value.range.start.line === 6));
      assert.ok(found.every(value => [5, 6].includes(value.range.start.line)), JSON.stringify(found));
      assert.equal(api.lastResult.completeness, 'exact');
    });
    await test('internal hover is suppressed', async () => {
      const found = await executeProvider('vscode.executeHoverProvider', document.uri, position(document, 3, 'value'));
      assert.deepEqual(found, []);
      assert.equal(api.lastResult.completeness, 'exact');
    });
    await test('qualified external hover uses cached documentation', async () => {
      const found = await executeProvider('vscode.executeHoverProvider', document.uri, position(document, 11, 'answer'));
      assert.equal(found.length, 1, JSON.stringify(api.lastResult));
      const content = found[0].contents.map(item => typeof item === 'string' ? item : item.value).join('\n');
      assert.match(content, /val answer\s*:\s*(?:Microsoft\.FSharp\.Core\.)?int\b/);
      assert.match(content, /The answer from a library/);
      const editor = await vscode.window.showTextDocument(document);
      const cursor = position(document, 11, 'answer');
      editor.selection = new vscode.Selection(cursor, cursor);
      await vscode.commands.executeCommand('editor.action.showHover');
      await screenshot('external-hover');
    });
    await test('unsupported member binding is explicit and returns no speculative navigation', async () => {
      const found = await definitions(document, 10, 'NotKnown');
      assert.deepEqual(found, []);
      assert.ok(['unavailable', 'approximate'].includes(api.lastResult.completeness));
    });
    await test('receiver annotations disambiguate identically named record fields', async () => {
      const members = await vscode.workspace.openTextDocument(uri('Members.fs'));
      for (const [useLine, declarationLine] of [[3, 1], [4, 2]]) {
        const found = await definitions(members, useLine, 'Name');
        assert.equal(found.length, 1, JSON.stringify(api.lastResult));
        assert.equal(found[0].uri.fsPath, members.uri.fsPath);
        assert.equal(found[0].range.start.line, declarationLine);
        assert.equal(api.lastResult.completeness, 'exact');
      }
      const found = await references(members, 1, 'Name');
      assert.ok(found.some(value => value.range.start.line === 3), JSON.stringify(api.lastResult));
      assert.ok(found.every(value => [1, 3].includes(value.range.start.line)), JSON.stringify(found));
      assert.equal(api.lastResult.completeness, 'exact');
    });
    await test('dirty receiver type changes rebind definitions and complete reference sets', async () => {
      const members = await vscode.workspace.openTextDocument(uri('Members.fs'));
      const start = members.lineAt(3).text.indexOf('Person');
      const edit = new vscode.WorkspaceEdit();
      edit.replace(members.uri, new vscode.Range(3, start, 3, start + 'Person'.length), 'Company');
      assert.equal(await vscode.workspace.applyEdit(edit), true);
      const found = await definitions(members, 3, 'Name');
      assert.equal(found.length, 1, JSON.stringify(api.lastResult));
      assert.equal(found[0].range.start.line, 2);
      assert.equal(api.lastResult.completeness, 'exact');
      assert.equal(api.lastResult.version, members.version);
      const oldReferences = await references(members, 1, 'Name');
      assert.ok(oldReferences.every(value => value.range.start.line === 1), JSON.stringify(oldReferences));
      assert.equal(api.lastResult.completeness, 'exact');
      const newReferences = await references(members, 2, 'Name');
      assert.ok(newReferences.some(value => value.range.start.line === 3), JSON.stringify(newReferences));
      assert.ok(newReferences.some(value => value.range.start.line === 4), JSON.stringify(newReferences));
      assert.ok(newReferences.every(value => [2, 3, 4].includes(value.range.start.line)), JSON.stringify(newReferences));
      assert.equal(api.lastResult.completeness, 'exact');
      assert.match(await fs.readFile(members.uri.fsPath, 'utf8'), /read \(x: Person\)/);
    });
    await test('dirty buffers in multiple files override disk', async () => {
      const library = await vscode.workspace.openTextDocument(uri('Alpha.fs'));
      const edit = new vscode.WorkspaceEdit();
      edit.replace(library.uri, new vscode.Range(1, 4, 1, 9), 'changed');
      edit.replace(document.uri, new vscode.Range(3, 13, 3, 18), 'changed');
      assert.equal(await vscode.workspace.applyEdit(edit), true);
      assert.equal(library.isDirty, true);
      assert.equal(document.isDirty, true);
      const found = await definitions(document, 3, 'changed');
      assert.equal(found.length, 1, JSON.stringify(api.lastResult));
      assert.equal(found[0].uri.fsPath, library.uri.fsPath);
      assert.equal(found[0].range.start.line, 1);
      assert.match(await fs.readFile(library.uri.fsPath, 'utf8'), /let value/);
    });
    await test('routine indexed queries never invoke the compiler helper', async () => {
      const values = await api.stats();
      assert.ok(values.length);
      for (const stats of values) {
        assert.equal(stats.mode, 'indexed', JSON.stringify(stats));
        const helperStarts = stats.helperStarts ?? stats.helper_starts;
        assert.equal(helperStarts, 0, 'Engine must report zero helper starts: ' + JSON.stringify(stats));
        assert.equal(stats.compilerLaunches, 0);
      }
    });
  }
  await fs.writeFile(path.join(process.env.FSHARP_TEST_RESULTS, 'results.json'), JSON.stringify({
    environment: { kind: 'actual-vscode-extension-host', vscode: vscode.version, node: process.version, extensionHostPid: process.pid, backendPids: api?.processIds ?? [] },
    measurements: { activation, providerSamples, localLifecycle, finalExtensionHostMemory: process.memoryUsage(), limitations: 'Test-run timings, not repeated controlled benchmarks. Memory describes the shared extension-host process, not the entire editor process tree. alreadyActive activation cannot measure a cold activation.' },
    results,
  }, null, 2));
  for (const result of results) console.log(`${result.status}: ${result.name}${result.error ? '\n' + result.error : ''}`);
  const failures = results.filter(result => result.status === 'failed');
  if (failures.length) throw new Error(`${failures.length} extension-host tests failed`);
};
