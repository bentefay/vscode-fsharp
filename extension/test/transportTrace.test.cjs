'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const Module = require('node:module');
const { execFileSync } = require('node:child_process');
// This separate test process enables opt-in tracing before loading the adapter.
process.env.FSHARP_NAVIGATOR_TRACE = '1';
const load = Module._load;
Module._load = function(name, ...rest) {
  return name === 'vscode' ? { CancellationError: class extends Error {} } : load.call(this, name, ...rest);
};
const { Backend } = require('../dist/backend.js');
Module._load = load;
const { traceEvents, parseDemandTraceConfig, demandSwitchTraceFields } = require('../dist/trace.js');

test('trace-off ignores numeric diagnostic environment and emits no events', () => {
  const script = `const assert=require('node:assert/strict');const trace=require(${JSON.stringify(path.resolve(__dirname, '../dist/trace.js'))});
    assert.equal(trace.traceEnabled,false);assert.equal(trace.demandTraceConfig,undefined);
    trace.trace('ignored',{targetSlot:0});assert.deepEqual(trace.traceEvents(),[]);`;
  execFileSync(process.execPath, ['-e', script], { env: { ...process.env,
    FSHARP_NAVIGATOR_TRACE: '0', FSHARP_NAVIGATOR_TRACE_MODEL_ORDINALS: '[1,2]', FSHARP_NAVIGATOR_TRACE_WATCH_SLOT: '1' } });
});

test('demand selectors accept only capped numeric model ordinals and a configured slot', () => {
  assert.deepEqual(parseDemandTraceConfig('[41,99]', '1'), { modelOrdinals: [41,99], watchSlot: 1 });
  assert.deepEqual(parseDemandTraceConfig('[0]'), { modelOrdinals: [0], watchSlot: 0 });
  for (const value of [undefined, '', '{}', '[]', '[1,1]', '[1,2,3,4,5]', '[100000]', '[-1]', '[0.5]', '["secret/path"]', '[1e300]', ' '.repeat(65) + '[1]'])
    assert.equal(parseDemandTraceConfig(value), undefined);
  for (const slot of ['1', '-1', '4', '0.5', '00', 'private/path']) assert.equal(parseDemandTraceConfig('[9]', slot), undefined);
});

test('demand envelope whitelist omits arbitrary strings, paths, nesting and excess indices', () => {
  const value = { targetSlot: 0, beforeTargetState: 'pending', afterPhase: 'ready',
    postingStatus: 'unknown', candidateIndexLane: 'result.candidates', candidateIndices: [0, 7, 15, 199, 8],
    candidateIndicesTruncated: true, targetSeedCompleted: true, seedCompleted: 1,
    identityVerified: false, admissionInsideLastCompletedSlice: false, lastCompletedSliceId: 3,
    lastCompletedSliceMicros: 9.25, sliceBeforeGeneration: 1, sliceAfterGeneration: 1,
    beforeCursor: -1, afterCursor: Infinity, originSlot: null, beforePhase: 'secret/path',
    path: 'secret/path', source: 'private source', nested: { text: 'private source' } };
  const result = demandSwitchTraceFields(value);
  assert.equal(result.demand_targetSeedCompleted, true);
  assert.equal(result.demand_identityVerified, false);
  assert.equal(result.demand_admissionInsideLastCompletedSlice, false);
  assert.equal(result.demand_lastCompletedSliceMicros, 9.25);
  assert.equal(result.demand_candidateIndex3, 199);
  for (const key of ['demand_candidateIndex4', 'demand_originSlot', 'demand_beforeCursor', 'demand_afterCursor', 'demand_beforePhase', 'path', 'source', 'nested']) assert.equal(result[key], undefined);
  assert.ok(!JSON.stringify(result).includes('secret'));
  assert.deepEqual(demandSwitchTraceFields(null), {});
});

test('proof diagnostics stay correlated with transport response and do not enter results', async t => {
  const root = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(root, { recursive: true });
  const dir = await fs.mkdtemp(path.join(root, 'fsnav-proof-trace-'));
  const executable = path.join(dir, 'engine');
  await fs.writeFile(executable, `#!/usr/bin/env node
require('node:readline').createInterface({input:process.stdin}).on('line', line => {
  const m=JSON.parse(line); if(m.method==='shutdown')process.exit(0);
  if(!m.id)return;
  if(m.providerRequestId!==undefined||m.params?.providerRequestId!==undefined)throw Error('client correlation leaked onto wire');
  process.stdout.write(JSON.stringify({id:m.id,result:{ok:true},nativeTiming:{queueMs:1,dispatchMs:2},
    documentUpdate:m.method==='update'?{documentKey:'opaque-document',version:9,sourceHash:'current-hash',bytes:123,changed:true,validationMicros:1,indexUpdateMicros:12,hashMicros:2,nameScanMicros:3,namesCount:4,lookupMicros:11,transactionBeginMicros:12,postingsDeleteMicros:13,namesInsertMicros:14,stateWriteMicros:15,commitMicros:16,overlayStoreMicros:5,proofCacheEvicted:6,snapshotMicros:7,totalMicros:25,path:'not-copied',text:'not-copied'}:undefined,
    demandSwitch:m.method==='proof'?{targetSlot:0,beforeTargetState:'pending',targetSeedCompleted:true,candidateIndices:[7],candidateIndexLane:'result.candidates',path:'not-copied'}:undefined,
    localProofAttempt:m.method==='proof'?{attempt:7,documentVersion:3,outcome:'parse-time-budget',parseMicros:100001,totalMicros:100002,visitedNodes:0,parsedBytes:99,sourceHash:'not-copied',path:'not-copied'}:null})+'\\n');
});
`, { mode: 0o755 });
  const backend = new Backend(executable, { append() {} });
  t.after(async () => { backend.dispose(); await fs.rm(dir, { recursive: true, force: true }); });
  assert.deepEqual(await backend.request('proof', {}, undefined, 30_000, 42), { ok: true });
  assert.deepEqual(await backend.request('no-proof'), { ok: true });
  assert.deepEqual(await backend.request('update'), { ok: true });
  assert.deepEqual(await backend.request('no-update'), { ok: true });
  const responses = traceEvents().filter(e => e.event === 'transportResponse');
  assert.equal(responses.length, 4);
  assert.equal(responses[0].details.id, 1);
  assert.equal(responses[0].details.providerRequestId, 42);
  assert.equal(traceEvents().find(e => e.event === 'transportRequest' && e.details.id === 1).details.providerRequestId, 42);
  assert.equal(responses[0].details.demand_targetSeedCompleted, true);
  assert.equal(responses[0].details.demand_candidateIndex0, 7);
  assert.equal(responses[0].details.localProofAttempt, 7);
  assert.equal(responses[0].details.localProofVersion, 3);
  assert.equal(responses[0].details.localProofOutcome, 'parse-time-budget');
  assert.equal(responses[0].details.localProofParseMicros, 100001);
  assert.equal(responses[0].details.localProofTotalMicros, 100002);
  assert.equal(responses[0].details.localProofVisitedNodes, 0);
  assert.equal(responses[0].details.localProofParsedBytes, 99);
  assert.equal(responses[1].details.id, 2);
  assert.equal(responses[1].details.localProofOutcome, undefined);
  assert.equal(responses[1].details.providerRequestId, undefined);
  assert.equal(responses[1].details.demand_targetSeedCompleted, undefined);
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
