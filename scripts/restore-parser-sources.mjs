#!/usr/bin/env node
// Development checkout bootstrap only. The installed extension never loads this.
import { createReadStream, createWriteStream } from 'node:fs';
import { lstat, mkdir, mkdtemp, readFile, link, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { Readable, Transform, Writable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { createGunzip } from 'node:zlib';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const targets = new Map([
  ['vendor/tree-sitter-fsharp/fsharp/src/parser.c', 'scripts/generated-parser-inputs/fsharp-parser.c.gz'],
  ['vendor/tree-sitter-fsharp/fsharp_signature/src/parser.c', 'scripts/generated-parser-inputs/signature-parser.c.gz'],
]);
const maxSourceBytes = 64 * 1024 * 1024;
const maxArchiveBytes = 8 * 1024 * 1024;
const maxPartBytes = 500 * 1024;

function validatePin(pin) {
  if (targets.get(pin.source) !== pin.archive) throw new Error('Unsupported generated parser path.');
  for (const [size, cap, hash] of [[pin.sourceBytes, maxSourceBytes, pin.sourceSha256], [pin.archiveBytes, maxArchiveBytes, pin.archiveSha256]]) {
    if (!Number.isSafeInteger(size) || size <= 0 || size > cap || !/^[0-9a-f]{64}$/.test(hash)) {
      throw new Error('Invalid generated parser size or SHA-256 pin.');
    }
  }
  if (pin.parts !== undefined) {
    if (!Array.isArray(pin.parts) || pin.parts.length < 1 || pin.parts.length > 32) throw new Error('Invalid generated parser parts.');
    let bytes = 0;
    for (const [index, part] of pin.parts.entries()) {
      if (part.path !== `${pin.archive}.part-${String(index + 1).padStart(3, '0')}` ||
          !Number.isSafeInteger(part.bytes) || part.bytes <= 0 || part.bytes > maxPartBytes ||
          !/^[0-9a-f]{64}$/.test(part.sha256)) throw new Error('Invalid generated parser part path, size or SHA-256 pin.');
      bytes += part.bytes;
    }
    if (bytes !== pin.archiveBytes) throw new Error('Generated parser part sizes do not match the archive size.');
  }
}

function meter(label, expectedBytes, expectedHash) {
  let bytes = 0;
  const hash = createHash('sha256');
  const stream = new Transform({
    transform(chunk, encoding, callback) {
      bytes += chunk.length;
      if (bytes > expectedBytes) return callback(new Error(`${label}: byte limit exceeded.`));
      hash.update(chunk);
      callback(null, chunk);
    },
  });
  return {
    stream,
    verify() {
      if (bytes !== expectedBytes || hash.digest('hex') !== expectedHash) throw new Error(`${label}: size or SHA-256 mismatch.`);
    },
  };
}

async function regularFile(filename, expectedBytes, optional = false) {
  let info;
  try { info = await lstat(filename); }
  catch (error) { if (optional && error.code === 'ENOENT') return false; throw error; }
  if (!info.isFile() || info.size !== expectedBytes) throw new Error(`${filename}: expected a regular file of ${expectedBytes} bytes.`);
  return true;
}

async function verifyFile(filename, expectedBytes, expectedHash, optional = false) {
  if (!await regularFile(filename, expectedBytes, optional)) return false;
  const check = meter(filename, expectedBytes, expectedHash);
  // Drain a bounded stream; never retain the full generated C source in memory.
  await pipeline(createReadStream(filename), check.stream, new Writable({ write(chunk, encoding, callback) { callback(); } }));
  check.verify();
  return true;
}

async function* archiveChunks(root, pin) {
  const parts = pin.parts ?? [{ path: pin.archive, bytes: pin.archiveBytes, sha256: pin.archiveSha256 }];
  for (const part of parts) {
    const filename = path.join(root, part.path);
    await regularFile(filename, part.bytes);
    let bytes = 0;
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(filename)) {
      bytes += chunk.length;
      if (bytes > part.bytes) throw new Error(`${part.path}: byte limit exceeded.`);
      hash.update(chunk);
      yield chunk;
    }
    if (bytes !== part.bytes || hash.digest('hex') !== part.sha256) throw new Error(`${part.path}: size or SHA-256 mismatch.`);
  }
}

function archiveStream(root, pin) {
  return Readable.from(archiveChunks(root, pin), { objectMode: false });
}

export async function restoreParser(root, pin) {
  validatePin(pin);
  const source = path.join(root, pin.source);
  if (await verifyFile(source, pin.sourceBytes, pin.sourceSha256, true)) {
    const compressed = meter(pin.archive, pin.archiveBytes, pin.archiveSha256);
    await pipeline(archiveStream(root, pin), compressed.stream, new Writable({ write(chunk, encoding, callback) { callback(); } }));
    compressed.verify();
    return { source: pin.source, restored: false };
  }
  const cache = path.join(root, '.cache');
  await mkdir(cache, { recursive: true });
  // Keep temporary data outside vendor: every vendor file is fingerprinted.
  const temporaryDirectory = await mkdtemp(path.join(cache, 'parser-restore-'));
  const temporary = path.join(temporaryDirectory, 'parser.c');
  try {
    const compressed = meter(pin.archive, pin.archiveBytes, pin.archiveSha256);
    const expanded = meter(pin.source, pin.sourceBytes, pin.sourceSha256);
    await pipeline(archiveStream(root, pin), compressed.stream, createGunzip(), expanded.stream,
      createWriteStream(temporary, { flags: 'wx', mode: 0o644 }));
    compressed.verify();
    expanded.verify();
    await mkdir(path.dirname(source), { recursive: true });
    try {
      // Atomic no-clobber installation. A concurrent restore cannot overwrite
      // a developer's changed source; an existing target must pass the same pins.
      await link(temporary, source);
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      await verifyFile(source, pin.sourceBytes, pin.sourceSha256);
      return { source: pin.source, restored: false };
    }
    return { source: pin.source, restored: true };
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

export async function restoreParserSources(root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')) {
  const filename = path.join(root, 'scripts/generated-parser-inputs/manifest.json');
  const info = await lstat(filename);
  if (!info.isFile() || info.size > 16 * 1024) throw new Error('Invalid generated parser manifest.');
  const manifest = JSON.parse(await readFile(filename, 'utf8'));
  if (manifest.schema !== 1 || !Array.isArray(manifest.parsers) || manifest.parsers.length !== targets.size ||
      new Set(manifest.parsers.map(pin => pin.source)).size !== targets.size) throw new Error('Invalid generated parser manifest.');
  for (const pin of manifest.parsers) validatePin(pin);
  const results = [];
  for (const pin of manifest.parsers) results.push(await restoreParser(root, pin));
  return results;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const results = await restoreParserSources();
    console.log(`Verified ${results.length} generated parsers; restored ${results.filter(row => row.restored).length}.`);
  } catch (error) {
    console.error(`Parser source preparation failed: ${error.message}`);
    process.exitCode = 1;
  }
}
