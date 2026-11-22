'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const Module = require('node:module');
// This separate test process enables opt-in tracing before loading the adapter.
process.env.FSHARP_NAVIGATOR_TRACE = '1';
const load = Module._load;
Module._load = function(name, ...rest) {
  return name === 'vscode' ? { CancellationError: class extends Error {} } : load.call(this, name, ...rest);
};
const { Backend } = require('../dist/backend.js');
Module._load = load;
const { traceEvents } = require('../dist/trace.js');

test('proof diagnostics stay correlated with transport response and do not enter results', async t => {
  const root = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(root, { recursive: true });
  const dir = await fs.mkdtemp(path.join(root, 'fsnav-proof-trace-'));
  const executable = path.join(dir, 'engine');
  await fs.writeFile(executable, `#!/usr/bin/env node
require('node:readline').createInterface({input:process.stdin}).on('line', line => {
  const m=JSON.parse(line); if(m.method==='shutdown')process.exit(0);
  if(!m.id)return;
  process.stdout.write(JSON.stringify({id:m.id,result:{ok:true},nativeTiming:{queueMs:1,dispatchMs:2},
    documentUpdate:m.method==='update'?{documentKey:'opaque-document',version:9,sourceHash:'current-hash',bytes:123,changed:true,validationMicros:1,indexUpdateMicros:12,hashMicros:2,nameScanMicros:3,namesCount:4,lookupMicros:11,transactionBeginMicros:12,postingsDeleteMicros:13,namesInsertMicros:14,stateWriteMicros:15,commitMicros:16,overlayStoreMicros:5,proofCacheEvicted:6,snapshotMicros:7,totalMicros:25,path:'not-copied',text:'not-copied'}:undefined,
    localProofAttempt:m.method==='proof'?{attempt:7,documentVersion:3,outcome:'parse-time-budget',parseMicros:100001,totalMicros:100002,visitedNodes:0,parsedBytes:99,sourceHash:'not-copied',path:'not-copied'}:null})+'\\n');
});
`, { mode: 0o755 });
  const backend = new Backend(executable, { append() {} });
  t.after(async () => { backend.dispose(); await fs.rm(dir, { recursive: true, force: true }); });
  assert.deepEqual(await backend.request('proof'), { ok: true });
  assert.deepEqual(await backend.request('no-proof'), { ok: true });
  assert.deepEqual(await backend.request('update'), { ok: true });
  assert.deepEqual(await backend.request('no-update'), { ok: true });
  const responses = traceEvents().filter(e => e.event === 'transportResponse');
  assert.equal(responses.length, 4);
  assert.equal(responses[0].details.id, 1);
  assert.equal(responses[0].details.localProofAttempt, 7);
  assert.equal(responses[0].details.localProofVersion, 3);
  assert.equal(responses[0].details.localProofOutcome, 'parse-time-budget');
  assert.equal(responses[0].details.localProofParseMicros, 100001);
  assert.equal(responses[0].details.localProofTotalMicros, 100002);
  assert.equal(responses[0].details.localProofVisitedNodes, 0);
  assert.equal(responses[0].details.localProofParsedBytes, 99);
  assert.equal(responses[1].details.id, 2);
  assert.equal(responses[1].details.localProofOutcome, undefined);
  assert.equal(responses[2].details.id, 3);
  assert.equal(responses[2].details.updateDocumentKey, 'opaque-document');
  assert.equal(responses[2].details.updateVersion, 9);
  assert.equal(responses[2].details.updateSourceHash, 'current-hash');
  assert.equal(responses[2].details.updateIndexMicros, 12);
  assert.equal(responses[2].details.updateNameScanMicros, 3);
  assert.equal(responses[2].details.updateProofCacheEvicted, 6);
  assert.equal(responses[2].details.updateTotalMicros, 25);
  for (const [field, value] of Object.entries({updateLookupMicros:11,updateTransactionBeginMicros:12,updatePostingsDeleteMicros:13,updateNamesInsertMicros:14,updateStateWriteMicros:15,updateCommitMicros:16})) assert.equal(responses[2].details[field], value);
  assert.equal(responses[3].details.updateVersion, undefined);
  assert.equal(responses[3].details.localProofOutcome, undefined);
  assert.ok(!JSON.stringify(responses).includes('not-copied'));
});
