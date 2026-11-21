'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const { watch } = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { createInterface } = require('node:readline');
const { performance } = require('node:perf_hooks');
const { createEditor } = require('./support/editorDouble.cjs');
const root = path.resolve(__dirname, '../..');
const binary = process.env.FSHARP_BACKEND_PATH || path.join(root, 'target/release/fsharp-navigator');
const dependencyCount = 600;

async function fixture(t, files = dependencyCount) {
  const temporaryRoot = process.env.TMPDIR || path.join(root, '.cache/test-tmp');
  await fs.mkdir(temporaryRoot, { recursive: true });
  const temporary = await fs.mkdtemp(path.join(temporaryRoot, 'fsnav-cancellation-'));
  const workspace = path.join(temporary, 'workspace');
  await fs.mkdir(workspace);
  await fs.writeFile(path.join(workspace, 'Shared.fs'), 'module Shared\nlet target = 42\n');
  const names = ['Shared.fs'];
  const words = Array.from({ length: 200 }, (_, i) => `rawName${i}`).join(' ');
  for (let i = 0; i < files; i++) {
    const name = `F${String(i).padStart(4, '0')}.fs`;
    names.push(name);
    await fs.writeFile(path.join(workspace, name), `module F${i}\nlet result = Shared.target\n// ${words}\n`);
  }
  await fs.writeFile(path.join(workspace, 'Test.fsproj'), '<Project Sdk="Microsoft.NET.Sdk"><ItemGroup>' + names.map(name => `<Compile Include="${name}" />`).join('') + '</ItemGroup></Project>');
  t.after(() => fs.rm(temporary, { recursive: true, force: true }));
  return { temporary, workspace, cache: path.join(temporary, 'cache') };
}

async function walProgress(cache) {
  const wal = path.join(cache, 'index-v9.sqlite-wal');
  const initial = (await fs.stat(wal)).size;
  let watcher;
  let timer;
  const promise = new Promise((resolve, reject) => {
    let checking = false;
    const inspect = async () => {
      if (checking) return;
      checking = true;
      try {
        if ((await fs.stat(wal)).size > initial + 4096) { clearTimeout(timer); watcher.close(); resolve(); }
      } catch (error) { clearTimeout(timer); watcher.close(); reject(error); }
      finally { checking = false; }
    };
    watcher = watch(wal, inspect);
    timer = setTimeout(() => { watcher.close(); reject(new Error('No observable native index progress before deadline')); }, 10_000);
  });
  return { promise, dispose() { clearTimeout(timer); watcher.close(); } };
}

function client(t) {
  const process = spawn(binary, ['daemon'], { stdio: ['pipe', 'pipe', 'pipe'] });
  const pending = new Map();
  let nextId = 0;
  const stderr = [];
  process.stderr.on('data', chunk => stderr.push(String(chunk)));
  createInterface({ input: process.stdout }).on('line', line => {
    const response = JSON.parse(line);
    const waiter = pending.get(response.id);
    if (!waiter) return;
    pending.delete(response.id); clearTimeout(waiter.timer); waiter.resolve(response);
  });
  process.on('exit', code => {
    for (const waiter of pending.values()) { clearTimeout(waiter.timer); waiter.reject(new Error(`Native child exited ${code}: ${stderr.join('')}`)); }
    pending.clear();
  });
  t.after(() => process.kill());
  return {
    request(method, params = {}) {
      const id = ++nextId;
      const promise = new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} exceeded the test deadline`)); }, 15_000);
        pending.set(id, { resolve, reject, timer });
      });
      process.stdin.write(JSON.stringify({ id, method, params }) + '\n');
      return { id, promise };
    },
  };
}

async function record(name, value) {
  const directory = path.join(root, 'test-results/cancellation');
  await fs.mkdir(directory, { recursive: true });
  await fs.writeFile(path.join(directory, `${name}.json`), JSON.stringify(value, null, 2));
}

test('native cancellation interrupts a running index and retry completes without restart', async t => {
  const input = await fixture(t);
  const engine = client(t);
  const opened = await engine.request('workspace/open', { root: input.workspace, mode: 'indexed', cacheDir: input.cache }).promise;
  assert.ok(opened.result, JSON.stringify(opened));
  const progress = await walProgress(input.cache);
  t.after(() => progress.dispose());
  const indexing = engine.request('workspace/index');
  await progress.promise;
  const started = performance.now();
  const acknowledgement = await engine.request('$/cancelRequest', { id: indexing.id }).promise;
  assert.equal(acknowledgement.result.cancelled, true);
  const cancelled = await indexing.promise;
  assert.equal(cancelled.error?.code, 'cancelled', JSON.stringify(cancelled));
  const stats = (await engine.request('workspace/stats').promise).result;
  const foregroundMs = performance.now() - started;
  assert.ok(stats.indexFileCount < dependencyCount + 1, JSON.stringify(stats));
  assert.equal(stats.filesParsed, 0);
  assert.equal(stats.helperStarts, 0);
  const retried = await engine.request('workspace/index').promise;
  assert.equal(retried.result?.indexFileCount, dependencyCount + 1, JSON.stringify(retried));
  assert.deepEqual(retried.result.indexErrors, {});
  assert.equal(retried.result.filesParsed, 0);
  await record('daemon-index', { kind: 'actual-native-daemon', foregroundMs, cancelledResponse: cancelled, partialFileCount: stats.indexFileCount, completedFileCount: retried.result.indexFileCount, limitations: 'Functional interruption/recovery test; timing is not a controlled benchmark.' });
});

test('explicit indexing enables navigation after cold workspace discovery is declined', async t => {
  const input = await fixture(t);
  const editor = await createEditor(input.workspace, binary);
  t.after(() => editor.dispose());
  const document = await editor.open('Shared.fs');
  const position = editor.position(document, 1, 'target');
  const definition = () => editor.providers.definition.provideDefinition(document, position, editor.uncancelled);
  assert.equal(await definition(), undefined);
  assert.match(editor.api.lastResult.reason, /Index Workspace/);
  assert.equal((await editor.api.stats())[0].filesRead, 0);
  await editor.executeCommand('fsharpNavigator.indexWorkspace');
  assert.equal((await definition())?.length, 1, JSON.stringify(editor.api.lastResult));
  const stats = (await editor.api.stats())[0];
  assert.equal(stats.indexFileCount, dependencyCount + 1);
  assert.equal(stats.helperStarts, 0);
});

test('dirty dependency cancels native reference coverage and a fresh definition plus complete retry succeed', async t => {
  // Keep the reference retry below the cold foreground read/parse limits.
  const files = 48;
  const input = await fixture(t, files);
  const editor = await createEditor(input.workspace, binary);
  t.after(() => editor.dispose());
  const document = await editor.open('Shared.fs');
  const dependency = await editor.open('F0000.fs');
  const cursor = editor.position(document, 1, 'target');
  const definition = () => editor.providers.definition.provideDefinition(document, cursor, editor.uncancelled);
  const references = () => editor.providers.references.provideReferences(document, cursor, { includeDeclaration: true }, editor.uncancelled);
  assert.equal((await definition())?.length, 1, JSON.stringify(editor.api.lastResult));
  const progress = await walProgress(path.join(input.workspace, '.storage/workspace-0'));
  t.after(() => progress.dispose());
  const old = references();
  await progress.promise;
  const started = performance.now();
  editor.edit(dependency, dependency.getText().replace('Shared.target', 'Shared.target + 1'));
  assert.equal(await old, undefined);
  assert.equal((await definition())?.length, 1, JSON.stringify(editor.api.lastResult));
  const foregroundMs = performance.now() - started;
  const partial = (await editor.api.stats())[0];
  assert.ok(partial.indexFileCount < files + 1, 'The obsolete scan must stop, not merely discard its reply: ' + JSON.stringify(partial));
  assert.equal(partial.helperStarts, 0);
  const complete = await references();
  assert.equal(complete?.length, files + 1, JSON.stringify(editor.api.lastResult));
  assert.equal(editor.api.lastResult.completeness, 'exact');
  assert.equal(editor.api.lastResult.version, document.version);
  await record('adapter-dirty-references', { kind: 'compiled-adapter-real-native-mocked-editor-api', foregroundMs, partialFileCount: partial.indexFileCount, completedReferences: complete.length, limitations: 'Not an actual VS Code host run; functional cancellation/recovery test, not a controlled latency benchmark.' });
});
