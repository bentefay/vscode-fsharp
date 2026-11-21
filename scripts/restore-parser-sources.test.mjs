import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm, lstat, readdir, symlink } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import os from 'node:os';
import path from 'node:path';
import { restoreParser, restoreParserSources } from './restore-parser-sources.mjs';

const sha = data => createHash('sha256').update(data).digest('hex');
async function fixture(t, data = Buffer.from('int parser = 1;\n')) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'parser-restore-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const archive = gzipSync(data, { level: 9 });
  const pin = {
    source: 'vendor/tree-sitter-fsharp/fsharp/src/parser.c',
    archive: 'scripts/generated-parser-inputs/fsharp-parser.c.gz',
    sourceBytes: data.length, sourceSha256: sha(data), archiveBytes: archive.length, archiveSha256: sha(archive),
  };
  await mkdir(path.dirname(path.join(root, pin.archive)), { recursive: true });
  await writeFile(path.join(root, pin.archive), archive);
  return { root, data, pin, source: path.join(root, pin.source), archive: path.join(root, pin.archive) };
}
async function assertUninstalled(f) {
  await assert.rejects(lstat(f.source), { code: 'ENOENT' });
  assert.deepEqual(await readdir(path.join(f.root, '.cache')).catch(error => error.code === 'ENOENT' ? [] : Promise.reject(error)), []);
}

async function multipart(f) {
  const data = await readFile(f.archive);
  const partSize = Math.ceil(data.length / 3);
  f.pin.parts = [];
  for (let start = 0; start < data.length; start += partSize) {
    const content = data.subarray(start, start + partSize);
    const name = `${f.pin.archive}.part-${String(f.pin.parts.length + 1).padStart(3, '0')}`;
    await writeFile(path.join(f.root, name), content);
    f.pin.parts.push({ path: name, bytes: content.length, sha256: sha(content) });
  }
  await rm(f.archive);
  return f;
}

test('fresh restore is byte-identical and repeat verifies without rewriting', async t => {
  const f = await fixture(t);
  assert.equal((await restoreParser(f.root, f.pin)).restored, true);
  assert.deepEqual(await readFile(f.source), f.data);
  const before = await lstat(f.source);
  assert.equal((await restoreParser(f.root, f.pin)).restored, false);
  assert.equal((await lstat(f.source)).mtimeMs, before.mtimeMs);
  assert.deepEqual(await readdir(path.join(f.root, '.cache')), []);
});
test('existing modified source is rejected and never overwritten', async t => {
  const f = await fixture(t);
  await restoreParser(f.root, f.pin);
  const modified = Buffer.from(f.data); modified[0] ^= 1;
  await writeFile(f.source, modified);
  await assert.rejects(restoreParser(f.root, f.pin), /SHA-256 mismatch/);
  assert.deepEqual(await readFile(f.source), modified);
});
test('wrong expanded hash cannot install a source', async t => {
  const f = await fixture(t);
  await assert.rejects(restoreParser(f.root, { ...f.pin, sourceSha256: '0'.repeat(64) }), /SHA-256 mismatch/);
  await assertUninstalled(f);
});
test('wrong compressed hash cannot install a source', async t => {
  const f = await fixture(t);
  await assert.rejects(restoreParser(f.root, { ...f.pin, archiveSha256: '0'.repeat(64) }), /SHA-256 mismatch/);
  await assertUninstalled(f);
});
test('truncated gzip is rejected without partial target', async t => {
  const f = await fixture(t);
  const truncated = (await readFile(f.archive)).subarray(0, -4);
  await writeFile(f.archive, truncated);
  await assert.rejects(restoreParser(f.root, { ...f.pin, archiveBytes: truncated.length, archiveSha256: sha(truncated) }));
  await assertUninstalled(f);
});
test('expanded stream has an enforced byte limit', async t => {
  const f = await fixture(t, Buffer.alloc(1024 * 1024, 65));
  await assert.rejects(restoreParser(f.root, { ...f.pin, sourceBytes: 64 }), /byte limit exceeded/);
  await assertUninstalled(f);
});
test('hard size caps and path traversal are rejected before work', async t => {
  const f = await fixture(t);
  await assert.rejects(restoreParser(f.root, { ...f.pin, sourceBytes: 64 * 1024 * 1024 + 1 }), /Invalid generated parser size/);
  await assert.rejects(restoreParser(f.root, { ...f.pin, archiveBytes: 8 * 1024 * 1024 + 1 }), /Invalid generated parser size/);
  await assert.rejects(restoreParser(f.root, { ...f.pin, source: '../escape.c' }), /Unsupported generated parser path/);
  await assertUninstalled(f);
});
test('symlink source is rejected without changing target', async t => {
  const f = await fixture(t);
  await mkdir(path.dirname(f.source), { recursive: true });
  const outside = path.join(f.root, 'outside.c'); await writeFile(outside, f.data);
  await symlink(outside, f.source);
  await assert.rejects(restoreParser(f.root, f.pin), /regular file/);
  assert.deepEqual(await readFile(outside), f.data);
});
test('concurrent restores install one exact file and clean temporary files', async t => {
  const f = await fixture(t, Buffer.alloc(1024 * 1024, 65));
  const results = await Promise.all([restoreParser(f.root, f.pin), restoreParser(f.root, f.pin)]);
  assert.equal(results.filter(row => row.restored).length, 1);
  assert.equal(sha(await readFile(f.source)), f.pin.sourceSha256);
  assert.deepEqual(await readdir(path.join(f.root, '.cache')), []);
});
test('existing source does not bypass compressed hash verification', async t => {
  const f = await fixture(t);
  await restoreParser(f.root, f.pin);
  await assert.rejects(restoreParser(f.root, { ...f.pin, archiveSha256: '0'.repeat(64) }), /SHA-256 mismatch/);
});
test('manifest requires both distinct known parser paths', async t => {
  const f = await fixture(t);
  await writeFile(path.join(f.root, 'scripts/generated-parser-inputs/manifest.json'), JSON.stringify({schema: 1, parsers: [f.pin, f.pin]}));
  await assert.rejects(restoreParserSources(f.root), /Invalid generated parser manifest/);
});

test('multipart archive restores without a combined gzip file and verifies repeat', async t => {
  const f = await multipart(await fixture(t));
  assert.equal((await restoreParser(f.root, f.pin)).restored, true);
  assert.deepEqual(await readFile(f.source), f.data);
  assert.equal((await restoreParser(f.root, f.pin)).restored, false);
  await assert.rejects(lstat(f.archive), { code: 'ENOENT' });
});
test('missing middle part fails without a partial target or retained temporary file', async t => {
  const f = await multipart(await fixture(t));
  await rm(path.join(f.root, f.pin.parts[1].path));
  await assert.rejects(restoreParser(f.root, f.pin), { code: 'ENOENT' });
  await assertUninstalled(f);
});
test('wrong individual part hash fails even when combined archive bytes are correct', async t => {
  const f = await multipart(await fixture(t));
  f.pin.parts[1].sha256 = '0'.repeat(64);
  await assert.rejects(restoreParser(f.root, f.pin), /SHA-256 mismatch/);
  await assertUninstalled(f);
});
test('corrupted part bytes cannot install a source', async t => {
  const f = await multipart(await fixture(t));
  const filename = path.join(f.root, f.pin.parts[1].path);
  const data = await readFile(filename); data[0] ^= 1;
  await writeFile(filename, data);
  await assert.rejects(restoreParser(f.root, f.pin));
  await assertUninstalled(f);
});
test('reordered or duplicated manifest parts are rejected', async t => {
  const f = await multipart(await fixture(t));
  const reordered = [...f.pin.parts]; [reordered[0], reordered[1]] = [reordered[1], reordered[0]];
  await assert.rejects(restoreParser(f.root, { ...f.pin, parts: reordered }), /Invalid generated parser part path/);
  await assert.rejects(restoreParser(f.root, { ...f.pin, parts: [f.pin.parts[0], f.pin.parts[0], f.pin.parts[2]] }), /Invalid generated parser part path/);
  await assertUninstalled(f);
});
test('whole archive hash remains required after every part hash passes', async t => {
  const f = await multipart(await fixture(t));
  await assert.rejects(restoreParser(f.root, { ...f.pin, archiveSha256: '0'.repeat(64) }), /SHA-256 mismatch/);
  await assertUninstalled(f);
});
test('part hard limit, aggregate size and path traversal are rejected', async t => {
  const f = await multipart(await fixture(t));
  const altered = change => ({ ...f.pin, parts: f.pin.parts.map((part, i) => i === 0 ? { ...part, ...change } : part) });
  await assert.rejects(restoreParser(f.root, altered({ bytes: 500 * 1024 + 1 })), /Invalid generated parser part/);
  await assert.rejects(restoreParser(f.root, altered({ bytes: f.pin.parts[0].bytes + 1 })), /part sizes/);
  await assert.rejects(restoreParser(f.root, altered({ path: '../outside' })), /Invalid generated parser part path/);
  await assertUninstalled(f);
});
test('missing part cannot bypass verification when the original source already exists', async t => {
  const f = await multipart(await fixture(t));
  await restoreParser(f.root, f.pin);
  await rm(path.join(f.root, f.pin.parts[1].path));
  await assert.rejects(restoreParser(f.root, f.pin), { code: 'ENOENT' });
  assert.deepEqual(await readFile(f.source), f.data);
});
