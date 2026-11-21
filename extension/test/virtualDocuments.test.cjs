'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const { pathToFileURL } = require('node:url');

// A narrow content-provider boundary double: these tests do not simulate the
// editor or compiler. URI conversion and cache accounting run the shipped code.
class Uri {
  constructor(value) { this.value = new URL(value); }
  static parse(value) { return new Uri(value); }
  static file(value) { return new Uri(pathToFileURL(value).toString()); }
  get scheme() { return this.value.protocol.slice(0, -1); }
  get authority() { return this.value.host; }
  get path() { return this.value.pathname; }
  get query() { return this.value.search.slice(1); }
  get fragment() { return this.value.hash.slice(1); }
  toString() { return this.value.toString(); }
}
class EventEmitter {
  listeners = [];
  event = listener => { this.listeners.push(listener); return { dispose() {} }; };
  fire(value) { for (const listener of this.listeners) listener(value); }
  dispose() { this.listeners = []; }
}
const load = Module._load;
Module._load = function (name, ...rest) { return name === 'vscode' ? { Uri, EventEmitter } : load.call(this, name, ...rest); };
const { MetadataDocuments, locationUri } = require('../dist/virtualDocuments.js');
Module._load = load;

const document = (id, text = 'val answer: int\n') => ({ uri: `fsharp-navigator://metadata/${id}.fsi`, language: 'fsharp', text });

test('helper metadata URI carried in path opens the cached F# signature', () => {
  const cache = new MetadataDocuments();
  const response = { locations: [{ path: document('helper').uri }], documents: [document('helper')] };
  cache.accept(response.documents);
  const target = locationUri(response.locations[0]);
  assert.equal(target.scheme, 'fsharp-navigator');
  assert.equal(cache.provideTextDocumentContent(target), 'val answer: int\n');
  assert.equal(locationUri({ path: '/workspace/Module.fs' }).scheme, 'file');
  cache.dispose();
});

test('cache evicts least recently used signatures and refreshes changed documents', () => {
  const cache = new MetadataDocuments(4096, 2, 2048);
  const changes = [];
  cache.onDidChange(uri => changes.push(uri.toString()));
  cache.accept([document('a'), document('b')]);
  cache.provideTextDocumentContent(Uri.parse(document('a').uri));
  cache.accept([document('c')]);
  assert.throws(() => cache.provideTextDocumentContent(Uri.parse(document('b').uri)), /expired/);
  cache.accept([document('a', 'val answer: string\n')]);
  assert.deepEqual(changes, [document('a').uri]);
  assert.equal(cache.provideTextDocumentContent(Uri.parse(document('a').uri)), 'val answer: string\n');
  cache.dispose();
});

test('oversized or unsupported protocol documents fail before replacing cached contents', () => {
  const cache = new MetadataDocuments(512, 2, 256);
  cache.accept([document('a')]);
  assert.throws(() => cache.accept([document('a', 'replacement'), document('big', '😀'.repeat(100))]), /cache limit/);
  assert.equal(cache.provideTextDocumentContent(Uri.parse(document('a').uri)), 'val answer: int\n');
  assert.throws(() => cache.accept([{ ...document('bad'), uri: 'file:///workspace/Source.fs' }]), /unsupported/);
  assert.throws(() => cache.accept([{ ...document('bad'), language: 'html' }]), /invalid/);
  cache.dispose();
});

test('aggregate UTF-16 byte budget evicts older entries even below the count limit', () => {
  const sample = document('a', 'x'.repeat(40));
  const bytes = 2 * (sample.uri.length + sample.text.length);
  const cache = new MetadataDocuments(bytes + 1, 64, 1024);
  cache.accept([sample]);
  cache.accept([document('b', sample.text)]);
  assert.equal(cache.has(Uri.parse(sample.uri)), false);
  assert.equal(cache.has(Uri.parse(document('b').uri)), true);
  assert.throws(() => cache.accept([document('a', sample.text), document('c', sample.text)]), /cache limit/);
  assert.equal(cache.has(Uri.parse(document('b').uri)), true);
  cache.dispose();
});

 test('decompiled C# documents share the bounded read-only cache with F# signatures', () => {
  const cache = new MetadataDocuments();
  const decompiled = { uri: 'fsharp-navigator://metadata/csharp.cs', language: 'csharp', text: 'public sealed class Library {}' };
  cache.accept([document('signature'), decompiled]);
  assert.equal(cache.provideTextDocumentContent(Uri.parse(decompiled.uri)), decompiled.text);
  assert.equal(cache.provideTextDocumentContent(Uri.parse(document('signature').uri)), 'val answer: int\n');
  assert.throws(() => cache.accept([{ ...decompiled, language: 'fsharp' }]), /language/);
  assert.throws(() => cache.accept([{ ...document('bad'), language: 'csharp' }]), /language/);
  assert.equal(cache.provideTextDocumentContent(Uri.parse(decompiled.uri)), decompiled.text);
  cache.dispose();
});
