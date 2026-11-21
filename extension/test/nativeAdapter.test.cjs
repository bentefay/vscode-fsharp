'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { createEditor } = require('./support/editorDouble.cjs');
const fixture = require('./fixtures.cjs');
const syntheticProjectCertificate = require('./support/syntheticProjectCertificate.cjs');

// Real shipped adapter + native process, mocked editor API boundary. This is
// deliberately separate from suite.cjs: passing is not a VS Code host pass.
async function setup(t) {
  const tempRoot = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(tempRoot, { recursive: true });
  const temporary = await fs.mkdtemp(path.join(tempRoot, 'fsnav-native-adapter-'));
  const directory = path.join(temporary, 'workspace');
  await fs.mkdir(directory);
  for (const [name, source] of Object.entries(fixture.files)) await fs.writeFile(path.join(directory, name), source);
  const catalog = path.join(temporary, 'catalog.json');
  await fs.writeFile(catalog, JSON.stringify(fixture.catalog(path.join(temporary, 'ExternalLibrary.dll'))));
  const binary = process.env.FSHARP_BACKEND_PATH || path.resolve(__dirname, '../../target/release/fsharp-navigator');
  await fs.access(binary);
  const editor = await createEditor(directory, binary, { libraryCatalogPath: catalog });
  t.after(async () => { editor.dispose(); await fs.rm(temporary, { recursive: true, force: true }); });
  editor.directory = directory;
  editor.definition = (document, line, name) => editor.providers.definition.provideDefinition(document, editor.position(document, line, name), editor.uncancelled);
  editor.references = (document, line, name) => editor.providers.references.provideReferences(document, editor.position(document, line, name), { includeDeclaration: true }, editor.uncancelled);
  return editor;
}

test('native adapter activation is lazy; indexed definitions respect dirty dependency buffers without a compiler', async t => {
  const editor = await setup(t);
  assert.deepEqual(editor.api.processIds, [], 'Activation alone must not spawn the native process');
  const document = await editor.open('Main.fs');
  const dependency = await editor.open('Alpha.fs');
  assert.deepEqual(editor.api.processIds, [], 'Opening clean tabs must not initialize their source ASTs');
  let found = await editor.definition(document, 3, 'value');
  assert.equal(found?.length, 1, JSON.stringify(editor.api.lastResult));
  assert.equal(found[0].uri.fsPath, dependency.uri.fsPath);
  assert.equal(editor.api.lastResult.completeness, 'exact');
  editor.edit(dependency, dependency.getText().replace('let value', 'let replacement'));
  editor.edit(document, document.getText().replace('let result = value', 'let result = replacement'));
  found = await editor.definition(document, 3, 'replacement');
  assert.equal(found?.length, 1, JSON.stringify(editor.api.lastResult));
  assert.equal(found[0].uri.fsPath, dependency.uri.fsPath);
  assert.equal(editor.api.lastResult.version, document.version);
  const stats = await editor.api.stats();
  assert.equal(stats.length, 1);
  assert.equal(stats[0].mode, 'indexed');
  assert.equal(stats[0].helperStarts ?? stats[0].helper_starts, 0);
});

test('native receiver proofs keep record fields separate and invalidate on dirty type edits', async t => {
  const editor = await setup(t);
  const document = await editor.open('Members.fs');
  const query = async (line, target) => {
    const found = await editor.definition(document, line, 'Name');
    assert.equal(found?.length, 1, JSON.stringify(editor.api.lastResult));
    assert.equal(found[0].range.start.line, target);
    assert.equal(editor.api.lastResult.completeness, 'exact');
    assert.equal(editor.api.lastResult.version, document.version);
  };
  await query(3, 1);
  await query(4, 2);
  let references = await editor.references(document, 1, 'Name');
  assert.deepEqual(references?.map(value => value.range.start.line).sort((a, b) => a - b), [1, 3], JSON.stringify(editor.api.lastResult));
  editor.edit(document, document.getText().replace('read (x: Person)', 'read (x: Company)'));
  await query(3, 2);
  references = await editor.references(document, 1, 'Name');
  assert.deepEqual(references?.map(value => value.range.start.line), [1], JSON.stringify(editor.api.lastResult));
  references = await editor.references(document, 2, 'Name');
  assert.deepEqual(references?.map(value => value.range.start.line).sort((a, b) => a - b), [2, 3, 4], JSON.stringify(editor.api.lastResult));
  assert.equal(editor.api.lastResult.completeness, 'exact');
  editor.edit(document, document.getText().replace('read (x: Company) = x.Name', 'read (x: string) = x.Length'));
  assert.equal(await editor.definition(document, 3, 'Length'), undefined);
  assert.ok(['unavailable', 'approximate'].includes(editor.api.lastResult.completeness));
  assert.equal(editor.status.text, 'F#: incomplete');
});

test('native catalog-only hover cannot bypass ownership proof and unsupported members stay explicit', async t => {
  const editor = await setup(t);
  const document = await editor.open('Main.fs');
  const hover = (line, name) => editor.providers.hover.provideHover(document, editor.position(document, line, name), editor.uncancelled);
  assert.equal(await hover(3, 'value'), undefined);
  assert.equal(editor.api.lastResult.completeness, 'exact');
  const external = await hover(11, 'answer');
  assert.equal(external, undefined, 'A catalog entry alone is not a binding proof');
  assert.equal(editor.api.lastResult.completeness, 'unavailable');
  assert.equal(editor.status.text, 'F#: incomplete');
  assert.equal(await editor.definition(document, 10, 'NotKnown'), undefined);
  assert.ok(['unavailable', 'approximate'].includes(editor.api.lastResult.completeness));
});

// Synthetic evaluated-model boundary fixture. Real MSBuild/FCS certificates are
// tested independently by semantic/tests and the evaluation harness.
test('native linked contexts use tree selection, preserve branch identity and share one dirty buffer', async t => {
  const tempRoot = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(tempRoot, { recursive: true });
  const directory = await fs.mkdtemp(path.join(tempRoot, 'fsnav-linked-adapter-'));
  const source = 'module Shared\n#if A\nlet selected = 1\n#else\nlet selected = 2\n#endif\nlet useSelected = selected\n';
  await fs.writeFile(path.join(directory, 'Shared.fs'), source);
  const projects = [];
  const inputHashes = [];
  const environmentCertificate = await syntheticProjectCertificate(directory);
  for (const name of ['A', 'B']) {
    const projectPath = path.join(directory, `${name}.fsproj`);
    const xml = `<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><DefineConstants>${name}</DefineConstants></PropertyGroup><ItemGroup><Compile Include="Shared.fs"/><Compile Include="Consumer${name}.fs"/></ItemGroup></Project>`;
    await fs.writeFile(projectPath, xml);
    await fs.writeFile(path.join(directory, `Consumer${name}.fs`), `module Consumer${name}\nlet value = Shared.selected\n`);
    projects.push({ path: projectPath, name, files: [path.join(directory, 'Shared.fs'), path.join(directory, `Consumer${name}.fs`)], references: [], defines: [name], targetFramework: 'net10.0', configuration: 'Debug', compilerArgs: [`--define:${name}`, '--langversion:preview'], evaluation: 'design-time-fsc-arguments', membershipReusable: true, sourceMembership: [], environmentCertificate, exact: true });
    inputHashes.push({ path: projectPath, sha256: require('node:crypto').createHash('sha256').update(xml).digest('hex') });
  }
  const modelPath = path.join(directory, 'evaluated.json');
  await fs.writeFile(modelPath, JSON.stringify({ projects, completeness: 'exact', inputs: inputHashes.map(value => value.path), inputHashes }));
  const binary = process.env.FSHARP_BACKEND_PATH || path.resolve(__dirname, '../../target/release/fsharp-navigator');
  const editor = await createEditor(directory, binary, { evaluatedModelPath: modelPath });
  t.after(async () => { editor.dispose(); await fs.rm(directory, { recursive: true, force: true }); });
  const shared = await editor.open('Shared.fs');
  const definition = (document, line, name = 'selected') => editor.providers.definition.provideDefinition(document, editor.position(document, line, name), editor.uncancelled);
  for (const [name, target] of [['A', 2], ['B', 4]]) {
    const consumer = await editor.open(`Consumer${name}.fs`);
    const result = await definition(consumer, 1);
    assert.equal(result?.[0].range.start.line, target, JSON.stringify(editor.api.lastResult));
  }
  assert.equal(await definition(shared, 6), undefined, 'A physical shared-file cursor needs an owning project');
  assert.equal(editor.api.lastResult.completeness, 'unavailable');
  const tree = await editor.api.tree.getChildren();
  const select = async name => {
    const project = tree.find(node => node.label === name);
    const file = project.children.find(node => node.path === shared.uri.fsPath);
    const item = editor.api.tree.getTreeItem(file);
    await editor.executeCommand(item.command.command, ...item.command.arguments);
  };
  await select('A');
  assert.equal((await definition(shared, 6))?.[0].range.start.line, 2, JSON.stringify(editor.api.lastResult));
  const contextA = editor.api.lastResult.projectContextId;
  assert.equal(await definition(shared, 4), undefined, 'The inactive branch is not a selected-instance symbol');
  await select('B');
  assert.equal((await definition(shared, 6))?.[0].range.start.line, 4, JSON.stringify(editor.api.lastResult));
  assert.notEqual(editor.api.lastResult.projectContextId, contextA);
  const refs = await editor.providers.references.provideReferences(shared, editor.position(shared, 4, 'selected'), { includeDeclaration: true }, editor.uncancelled);
  assert.deepEqual(refs?.map(value => `${path.basename(value.uri.fsPath)}:${value.range.start.line}`).sort(), ['ConsumerB.fs:1', 'Shared.fs:4', 'Shared.fs:6']);
  editor.edit(shared, source.replace('module Shared\n', 'module Shared\n\n'));
  assert.equal((await definition(shared, 7))?.[0].range.start.line, 5, JSON.stringify(editor.api.lastResult));
  const bytesBeforeSwitch = (await editor.api.stats())[0].overlayBytesReceived;
  await select('A');
  assert.equal((await definition(shared, 7))?.[0].range.start.line, 3, JSON.stringify(editor.api.lastResult));
  const stats = (await editor.api.stats())[0];
  assert.equal(stats.overlayBytesReceived, bytesBeforeSwitch, 'Project selection preserves physical text synchronization');
  assert.equal(stats.helperStarts, 0);
});
