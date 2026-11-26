'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn, execFileSync } = require('node:child_process');
const { createInterface } = require('node:readline');

test('special compile inputs cannot stall indexing or current navigation across restart', { skip: process.platform !== 'linux' }, async t => {
  const directory = await fs.mkdtemp(path.join(process.env.TMPDIR, 'fsnav-special-'));
  const source = path.join(directory, 'Main.fs');
  await fs.writeFile(source, 'module Main\nlet identity value = value\n');
  await fs.writeFile(path.join(directory, 'Main.fsproj'), '<Project><ItemGroup><Compile Include="Pipe.fs"/><Compile Include="Link.fs"/><Compile Include="Directory.fs"/><Compile Include="Main.fs"/></ItemGroup></Project>');
  execFileSync('mkfifo', [path.join(directory, 'Pipe.fs')]);
  await fs.symlink('Pipe.fs', path.join(directory, 'Link.fs'));
  await fs.mkdir(path.join(directory, 'Directory.fs'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  for (let generation = 0; generation < 2; generation++) {
    const child = spawn(process.env.FSHARP_BACKEND_PATH, ['daemon'], { stdio: ['pipe', 'pipe', 'inherit'] });
    const pending = new Map();
    let nextId = 0;
    const lines = createInterface({ input: child.stdout });
    lines.on('line', line => {
      const reply = JSON.parse(line), item = pending.get(reply.id);
      if (item) { clearTimeout(item.timer); pending.delete(reply.id); item.resolve(reply); }
    });
    const request = (method, params = {}) => new Promise((resolve, reject) => {
      const id = ++nextId;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} blocked on special input`)); }, 3000);
      pending.set(id, { resolve, timer });
      child.stdin.write(JSON.stringify({ id, method, params }) + '\n');
    });
    try {
      assert.ok((await request('workspace/open', { root: directory, cacheDir: path.join(directory, 'cache'), mode: 'indexed' })).result);
      const indexed = await request('workspace/index');
      assert.match(JSON.stringify(indexed.result.indexErrors), /regular file/);
      assert.equal(Object.keys(indexed.result.indexErrors).length, 3);
      assert.equal(indexed.result.indexFileCount, 1);
      assert.ok((await request('workspace/stats')).result);
      const query = async (text, version, line) => {
        await request('document/open', { path: source, text, version });
        const reply = await request('navigation/definition', { path: source, position: { line, character: 23 }, version });
        assert.equal(reply.result.completeness, 'exact', JSON.stringify(reply));
        assert.deepEqual(reply.result.locations, [{ uri: require('node:url').pathToFileURL(source).href,
          range: { start: { line, character: 13 }, end: { line, character: 18 } } }]);
      };
      await query('module Main\nlet identity value = value\n', 1, 1);
      await query('// dirty\nmodule Main\nlet identity value = value\n', 2, 2);
      await request('document/close', { path: source });
      await query('module Main\nlet identity value = value\n', 3, 1);
    } finally {
      child.kill('SIGKILL');
      await new Promise(resolve => child.exitCode !== null || child.signalCode !== null ? resolve() : child.once('exit', resolve));
      lines.close();
      for (const item of pending.values()) clearTimeout(item.timer);
    }
  }
});
