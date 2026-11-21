'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { createEditor } = require('./support/editorDouble.cjs');

async function setup(t, scenario) {
  const tempRoot = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(tempRoot, { recursive: true });
  const directory = await fs.mkdtemp(path.join(tempRoot, 'fsnav-adapter-contract-'));
  const binary = path.join(directory, 'engine');
  await fs.writeFile(path.join(directory, 'Main.fs'), 'module Main\nlet value = 1\nlet result = value\n');
  await fs.writeFile(path.join(directory, 'Dependency.fs'), 'module Dependency\nlet value = 1\n');
  await fs.writeFile(binary, `#!${process.execPath}
const readline = require('node:readline');
const scenario = ${JSON.stringify(scenario)};
let held;
let navigationCount = 0;
const send = (id,result) => process.stdout.write(JSON.stringify({id,result})+'\\n');
const location = path => ({path,range:{start:{line:1,character:4},end:{line:1,character:9}}});
readline.createInterface({input:process.stdin}).on('line', line => {
  const message = JSON.parse(line);
  const {id, method, params} = message;
  if(method === '$/cancelRequest') {process.stderr.write('NATIVE_CANCEL_RECEIVED\\n');return;}
  if(method === 'shutdown') process.exit(0);
  if(!id) return;
  if(method.startsWith('document/')) {
    process.stderr.write('OVERLAY_RECEIVED\\n');
    send(id,{version:params.version});
    if(held && params.version > 1) {send(held.id,held.result);held=undefined;}
    return;
  }
  if(method.startsWith('navigation/')) {
    navigationCount++;
    const result = {locations:[location(params.path)],completeness:scenario==='approximate'?'approximate':scenario==='unavailable'?'unavailable':'exact',source:'contract-fixture',version:scenario==='wrong-version'?params.version-1:params.version,reason:'deliberate contract fixture'};
    if(scenario==='context') {result.completeness=params.projectPath?'exact':'unavailable';result.projectContextId=params.projectPath;result.locations[0].range.start.line=params.projectPath?.endsWith('A.fsproj')?1:3;}
    if(scenario==='stale' && navigationCount===1) {held={id,result};process.stderr.write('NAVIGATION_HELD\\n');return;}
    if(method==='navigation/hover') {result.hover={markdown:'library text',isExternal:scenario==='external-hover'};result.ownership=scenario==='external-hover'?'external':'workspace';}
    send(id,result);return;
  }
  if(method==='workspace/tree') {send(id,{nodes:['A','B'].map(name=>({id:name,label:name,kind:'project',path:require('node:path').join(${JSON.stringify(directory)},name+'.fsproj'),children:[{id:name+'-shared',label:'Main.fs',kind:'file',path:require('node:path').join(${JSON.stringify(directory)},'Main.fs')}]}))});return;}
  send(id,{});
});
`, { mode: 0o755 });
  const editor = await createEditor(directory, binary);
  t.after(async () => { editor.dispose(); await fs.rm(directory, { recursive: true, force: true }); });
  return editor;
}

test('registering providers creates no navigator resources before the first F# request', async t => {
  const editor = await setup(t, 'exact');
  const doc = await editor.open('Main.fs');
  assert.deepEqual(await editor.api.stats(), []);
  assert.deepEqual(editor.api.processIds, []);
  assert.deepEqual(editor.resources, { outputChannels: 0, statusBarItems: 0, fileWatchers: 0 });
  const definition = await editor.providers.definition.provideDefinition(doc, editor.position(doc, 2, 'value'), editor.uncancelled);
  assert.equal(definition.length, 1);
  assert.equal(editor.resources.outputChannels, 1);
  assert.equal(editor.resources.statusBarItems, 1);
  assert.ok(editor.resources.fileWatchers > 0);
});

for (const completeness of ['approximate', 'unavailable']) {
  test(`adapter withholds ${completeness} definitions and references instead of presenting a complete set`, async t => {
    const editor = await setup(t, completeness);
    const document = await editor.open('Main.fs');
    const position = editor.position(document, 2, 'value');
    assert.equal(await editor.providers.definition.provideDefinition(document, position, editor.uncancelled), undefined);
    assert.equal(await editor.providers.references.provideReferences(document, position, { includeDeclaration: true }, editor.uncancelled), undefined);
    assert.equal(editor.api.lastResult.completeness, completeness);
    assert.equal(editor.status.text, 'F#: incomplete');
    assert.ok(editor.warnings.length, 'Explicit navigation must report incompleteness');
  });
}

test('adapter ignores a result from an obsolete cursor version', async t => {
  const editor = await setup(t, 'wrong-version');
  const document = await editor.open('Main.fs');
  assert.equal(await editor.providers.definition.provideDefinition(document, editor.position(document, 2, 'value'), editor.uncancelled), undefined);
  assert.equal(editor.api.lastResult, undefined);
});

test('dirty dependency edit invalidates a held response even when the cursor document is unchanged', async t => {
  const editor = await setup(t, 'stale');
  const document = await editor.open('Main.fs');
  const dependency = await editor.open('Dependency.fs');
  const position = editor.position(document, 2, 'value');
  const response = editor.providers.definition.provideDefinition(document, position, editor.uncancelled);
  await editor.waitForOutput('NAVIGATION_HELD');
  editor.edit(dependency, 'module Dependency\nlet replacement = 2\n');
  await editor.waitForOutput('NATIVE_CANCEL_RECEIVED');
  assert.equal(await response, undefined, 'Discarding a response is not native preemption');
  assert.equal(editor.api.lastResult, undefined);
  const current = await editor.providers.definition.provideDefinition(document, position, editor.uncancelled);
  assert.equal(current.length, 1);
  assert.equal(editor.api.lastResult.completeness, 'exact');
});

test('repository hover remains suppressed even if a response contains markdown', async t => {
  const editor = await setup(t, 'internal-hover');
  const document = await editor.open('Main.fs');
  assert.equal(await editor.providers.hover.provideHover(document, editor.position(document, 2, 'value'), editor.uncancelled), undefined);
});

test('positively external hover keeps markdown untrusted', async t => {
  const editor = await setup(t, 'external-hover');
  const document = await editor.open('Main.fs');
  const hover = await editor.providers.hover.provideHover(document, editor.position(document, 2, 'value'), editor.uncancelled);
  assert.equal(hover.contents.value, 'library text');
  assert.equal(hover.contents.isTrusted, false);
  assert.equal(hover.contents.supportHtml, false);
});

 test('project-tree context selects a linked source instance without resending its physical overlay', async t => {
  const editor = await setup(t, 'context');
  const document = await editor.open('Main.fs');
  const position = editor.position(document, 2, 'value');
  const definition = () => editor.providers.definition.provideDefinition(document, position, editor.uncancelled);
  assert.equal(await definition(), undefined, 'Ambiguous physical source must not guess first project');
  const overlaysBefore = editor.output.join('').match(/OVERLAY_RECEIVED/g)?.length ?? 0;
  const projects = await editor.api.tree.getChildren();
  const select = async project => { const item = editor.api.tree.getTreeItem(project.children[0]); assert.equal(item.command.command, 'fsharpNavigator.openProjectFile'); await editor.executeCommand(item.command.command, ...item.command.arguments); };
  await select(projects[0]);
  assert.equal((await definition())[0].range.start.line, 1);
  const firstContext = editor.api.lastResult.projectContextId;
  await select(projects[1]);
  assert.equal((await definition())[0].range.start.line, 3);
  assert.notEqual(editor.api.lastResult.projectContextId, firstContext);
  assert.equal(editor.output.join('').match(/OVERLAY_RECEIVED/g)?.length ?? 0, overlaysBefore, 'Context changes do not create new text snapshots');
  assert.equal(editor.openedUris.length, 2);
});
