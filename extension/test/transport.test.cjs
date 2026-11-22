'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const Module = require('node:module');

// VS Code is available only inside its host. This narrow unit double supplies
// CancellationError; source-analysis behavior is exercised by suite.cjs instead.
class CancellationError extends Error { constructor() { super('Cancelled'); } }
const load = Module._load;
Module._load = function (name, ...rest) { return name === 'vscode' ? { CancellationError } : load.call(this, name, ...rest); };
const { Backend } = require('../dist/backend.js');
Module._load = load;
const { OverlayQueue } = require('../dist/overlays.js');

async function engine(t, limits) {
  const tempRoot = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(tempRoot, { recursive: true });
  const directory = await fs.mkdtemp(path.join(tempRoot, 'fsnav-transport-'));
  const executable = path.join(directory, 'engine');
  await fs.writeFile(executable, `#!/usr/bin/env node
const readline = require('node:readline');
const records = [];
let failClose = true;
const reader = readline.createInterface({ input: process.stdin });
reader.on('line', line => {
  const message = JSON.parse(line);
  if (message.method === 'shutdown') process.exit(0);
  if (message.method === 'exit') process.exit(7);
  if (message.method === 'invalid') return process.stdout.write('not JSON\\n');
  if (!message.id) return;
  if (message.method === 'records') return process.stdout.write(JSON.stringify({id:message.id,result:records})+'\\n');
  if (message.method.startsWith('document/')) {
    records.push({method:message.method,path:message.params.path,version:message.params.version});
    if (message.method === 'document/close' && message.params.path === '/fixture/Retry.fs' && failClose) {
      failClose = false;
      return process.stdout.write(JSON.stringify({id:message.id,error:{message:'retry close'}})+'\\n');
    }
    return setTimeout(() => process.stdout.write(JSON.stringify({id:message.id,result:{version:message.params.version}})+'\\n'), 100);
  }
  const reply = JSON.stringify({ id: message.id, result: message.params });
  if (message.method === 'pause') {
    reader.pause();
    setTimeout(() => reader.resume(), 200);
  }
  if (message.method === 'fragment') {
    process.stdout.write(reply.slice(0, 8));
    setTimeout(() => process.stdout.write(reply.slice(8) + '\\n'), 10);
  } else if (message.method === 'delayed') {
    setTimeout(() => process.stdout.write(reply + '\\n'), 100);
  } else process.stdout.write(reply + '\\n');
});
`, { mode: 0o755 });
  const backend = new Backend(executable, { append() {} }, limits);
  t.after(async () => { backend.dispose(); await fs.rm(directory, { recursive: true, force: true }); });
  return backend;
}

test('fragmented JSON lines are assembled and concurrent IDs stay correlated', async t => {
  const backend = await engine(t);
  assert.deepEqual(await backend.request('fragment', { text: 'F# 😀' }), { text: 'F# 😀' });
  const values = await Promise.all([backend.request('delayed', { order: 1 }), backend.request('echo', { order: 2 })]);
  assert.deepEqual(values, [{ order: 1 }, { order: 2 }]);
});

test('cancelled request rejects promptly and a late reply cannot satisfy the next request', async t => {
  const backend = await engine(t);
  let cancel;
  const token = { isCancellationRequested: false, onCancellationRequested(listener) { cancel = listener; return { dispose() {} }; } };
  const cancelled = backend.request('delayed', { stale: true }, token);
  cancel();
  await assert.rejects(cancelled, CancellationError);
  assert.deepEqual(await backend.request('delayed', { fresh: true }), { fresh: true });
});

test('engine failure rejects pending requests instead of returning an empty answer', async t => {
  const backend = await engine(t);
  await assert.rejects(backend.request('exit'), /exited.*7/);
  await assert.rejects(backend.request('echo'), /not running/);
});

test('malformed engine output is an explicit protocol failure', async t => {
  const backend = await engine(t);
  await assert.rejects(backend.request('invalid'), /invalid protocol/);
});

test('timeout is an incomplete-operation error, never a complete empty result', async t => {
  const backend = await engine(t);
  await assert.rejects(backend.request('delayed', {}, undefined, 10), /no complete result/);
});

test('pending request count is bounded while a child is slow', async t => {
  const backend = await engine(t, { maxPendingRequests: 2 });
  const first = backend.request('delayed', { order: 1 });
  const second = backend.request('delayed', { order: 2 });
  await assert.rejects(backend.request('echo'), /request queue is full/);
  assert.deepEqual(await Promise.all([first, second]), [{ order: 1 }, { order: 2 }]);
});

test('pipe backpressure cannot grow the transport beyond its byte budget', async t => {
  const backend = await engine(t, { maxQueuedBytes: 3 * 1024 * 1024, maxMessageBytes: 3 * 1024 * 1024 });
  await backend.request('pause');
  const content = 'x'.repeat(2 * 1024 * 1024);
  const accepted = backend.request('echo', { content });
  await assert.rejects(backend.request('echo', { content }), /transport byte budget exceeded/);
  assert.equal((await accepted).content.length, content.length);
});

test('five thousand edits during an acknowledgement retain only first and latest snapshots', async t => {
  const backend = await engine(t);
  let textReads = 0;
  const errors = [];
  const queue = new OverlayQueue(Promise.resolve(), (method, params) => backend.request(method, params), error => errors.push(error));
  t.after(() => queue.dispose());
  const document = { version: 1, isClosed: false, getText() { textReads++; return `let value = ${this.version}`; } };
  queue.schedule('/fixture/Main.fs', document);
  await backend.request('echo');
  for (let version = 2; version <= 5000; version++) {
    document.version = version;
    queue.schedule('/fixture/Main.fs', document);
  }
  await queue.flush();
  assert.deepEqual(await backend.request('records'), [
    { method: 'document/open', path: '/fixture/Main.fs', version: 1 },
    { method: 'document/change', path: '/fixture/Main.fs', version: 5000 },
  ]);
  assert.equal(textReads, 2, 'Full text must be read at dispatch, not for every keystroke');
  assert.deepEqual(errors, []);
});

test('closing a document supersedes queued edits and is acknowledged after in-flight text', async t => {
  const backend = await engine(t);
  const queue = new OverlayQueue(Promise.resolve(), (method, params) => backend.request(method, params), () => {});
  t.after(() => queue.dispose());
  const document = { version: 1, isClosed: false, getText() { return `let value = ${this.version}`; } };
  queue.schedule('/fixture/Main.fs', document);
  await backend.request('echo');
  document.version = 2;
  queue.schedule('/fixture/Main.fs', document);
  queue.close('/fixture/Main.fs');
  await queue.flush();
  assert.deepEqual(await backend.request('records'), [
    { method: 'document/open', path: '/fixture/Main.fs', version: 1 },
    { method: 'document/close', path: '/fixture/Main.fs' },
  ]);
});

test('reopening the same document version while close is in flight restores its overlay', async t => {
  const backend = await engine(t);
  const queue = new OverlayQueue(Promise.resolve(), (method, params) => backend.request(method, params), () => {});
  t.after(() => queue.dispose());
  const document = { version: 1, isClosed: false, getText() { return 'let value = 1'; } };
  queue.schedule('/fixture/Main.fs', document);
  await queue.flush();
  queue.close('/fixture/Main.fs');
  await backend.request('echo');
  queue.schedule('/fixture/Main.fs', document);
  await queue.flush();
  assert.deepEqual((await backend.request('records')).map(record => record.method), ['document/open', 'document/close', 'document/open']);
});

test('failed synchronization rejects the barrier and retains failed close plus other documents for retry', async t => {
  const backend = await engine(t);
  const queue = new OverlayQueue(Promise.resolve(), (method, params) => backend.request(method, params), () => {});
  t.after(() => queue.dispose());
  const document = { version: 1, isClosed: false, getText() { return 'let value = 1'; } };
  queue.schedule('/fixture/Retry.fs', document);
  await queue.flush();
  queue.close('/fixture/Retry.fs');
  queue.schedule('/fixture/Other.fs', document);
  await assert.rejects(queue.flush(), /retry close/);
  await queue.flush();
  assert.deepEqual(await backend.request('records'), [
    { method: 'document/open', path: '/fixture/Retry.fs', version: 1 },
    { method: 'document/close', path: '/fixture/Retry.fs' },
    { method: 'document/open', path: '/fixture/Other.fs', version: 1 },
    { method: 'document/close', path: '/fixture/Retry.fs' },
  ]);
});

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function controlledOverlays(t) {
  const calls = [], waiting = [];
  const queue = new OverlayQueue(Promise.resolve(), (method, params) => {
    const ack = deferred();
    const call = { method, ...params, ack };
    const waiter = waiting.shift();
    if (waiter) waiter.resolve(call); else calls.push(call);
    return ack.promise;
  }, () => {});
  t.after(() => queue.dispose());
  return { queue, calls, next() {
    if (calls.length) return Promise.resolve(calls.shift());
    const waiter = deferred(); waiting.push(waiter); return waiter.promise;
  } };
}
const overlayDocument = (version, text = `let value = ${version}`) => ({ version, isClosed: false, getText: () => text });

for (const reopenedVersion of [1, 10]) test(`queued close survives reopening at version ${reopenedVersion} behind another document`, async t => {
  const { queue, next } = controlledOverlays(t);
  queue.schedule('A.fs', overlayDocument(10));
  (await next()).ack.resolve(); await queue.flush();
  queue.schedule('B.fs', overlayDocument(1)); const blocked = await next();
  queue.close('A.fs');
  let reads = 0;
  for (let i = 0; i < 5000; i++) {
    queue.close('A.fs');
    queue.schedule('A.fs', { version: reopenedVersion, isClosed: false, getText() { reads++; return `let latest = ${i}`; } });
  }
  assert.equal(reads, 0);
  blocked.ack.resolve();
  const close = await next(); assert.equal(close.method, 'document/close'); assert.equal(close.path, 'A.fs');
  assert.equal(reads, 0, 'New lifetime text waits for close acknowledgement');
  close.ack.resolve();
  const open = await next();
  assert.equal(open.method, 'document/open'); assert.equal(open.version, reopenedVersion); assert.equal(open.text, 'let latest = 4999');
  open.ack.resolve(); await queue.flush(); assert.equal(reads, 1);
});

test('close reopen close while another send is pending ends closed', async t => {
  const { queue, next, calls } = controlledOverlays(t);
  queue.schedule('A.fs', overlayDocument(10)); (await next()).ack.resolve(); await queue.flush();
  queue.schedule('B.fs', overlayDocument(1)); const blocked = await next();
  queue.close('A.fs'); queue.schedule('A.fs', overlayDocument(1)); queue.close('A.fs');
  blocked.ack.resolve(); const close = await next(); assert.equal(close.method, 'document/close');
  close.ack.resolve(); await queue.flush(); assert.equal(calls.length, 0);
});

test('failed close retains its barrier when a newer reopened document is pending', async t => {
  const { queue, next } = controlledOverlays(t);
  queue.schedule('A.fs', overlayDocument(10)); (await next()).ack.resolve(); await queue.flush();
  queue.close('A.fs'); const close = await next();
  queue.schedule('A.fs', overlayDocument(1));
  const failure = assert.rejects(queue.flush(), /close rejected/); close.ack.reject(new Error('close rejected')); await failure;
  const retry = queue.flush(); const closeAgain = await next(); assert.equal(closeAgain.method, 'document/close'); closeAgain.ack.resolve();
  const open = await next(); assert.equal(open.method, 'document/open'); assert.equal(open.version, 1); open.ack.resolve(); await retry;
});

for (const rejectOld of [false, true]) test(`late old text acknowledgement (${rejectOld ? 'failure' : 'success'}) preserves close and newest lifetime`, async t => {
  const { queue, next } = controlledOverlays(t);
  const old = overlayDocument(10);
  queue.schedule('A.fs', old); const active = await next();
  queue.close('A.fs'); queue.schedule('A.fs', old);
  if (rejectOld) {
    const failure = assert.rejects(queue.flush(), /old rejected/); active.ack.reject(new Error('old rejected')); await failure;
  } else active.ack.resolve();
  const completed = queue.flush(); const close = await next(); assert.equal(close.method, 'document/close');
  queue.schedule('A.fs', overlayDocument(1, 'let newest = 1')); close.ack.resolve();
  const open = await next(); assert.equal(open.method, 'document/open'); assert.equal(open.text, 'let newest = 1'); open.ack.resolve(); await completed;
});

test('dispose during close ignores late acknowledgement and never reopens; replacement queue starts fresh', async t => {
  const { queue, next, calls } = controlledOverlays(t);
  queue.schedule('A.fs', overlayDocument(10)); (await next()).ack.resolve(); await queue.flush();
  queue.close('A.fs'); const close = await next();
  let reads = 0; queue.schedule('A.fs', { version: 1, isClosed: false, getText() { reads++; return 'let fresh = 1'; } });
  queue.dispose(); close.ack.resolve(); await queue.flush();
  queue.schedule('A.fs', overlayDocument(1)); await queue.flush();
  assert.equal(reads, 0); assert.equal(calls.length, 0);
  const replacement = controlledOverlays(t); replacement.queue.schedule('A.fs', overlayDocument(1));
  const open = await replacement.next(); assert.equal(open.method, 'document/open'); open.ack.resolve(); await replacement.queue.flush();
});
