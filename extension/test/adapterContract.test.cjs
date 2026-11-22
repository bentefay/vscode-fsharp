'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { createEditor } = require('./support/editorDouble.cjs');

async function setup(t, scenario, configuration = {}) {
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
let occurrenceCount = 0; const snapshots = new Map(); let overlayHeldOnce=false;
const send = (id,result) => process.stdout.write(JSON.stringify({id,result})+'\\n');
const location = path => ({path,range:{start:{line:1,character:4},end:{line:1,character:9}}});
readline.createInterface({input:process.stdin}).on('line', line => {
  const message = JSON.parse(line);
  const {id, method, params} = message;
  if(method === '$/cancelRequest') {process.stderr.write('NATIVE_CANCEL_RECEIVED\\n');return;}
  if(method === 'shutdown') process.exit(0);
  if(!id) return;
  if(scenario==='pending-init' && method==='workspace/open') {process.stderr.write('INIT_HELD\\n');return;}
  if(method.startsWith('document/')) {
    if(method==='document/close') snapshots.delete(params.path); else snapshots.set(params.path,{text:params.text,version:params.version});
    process.stderr.write('OVERLAY_METHOD_'+method+'\\n');
    if(scenario==='occurrences-sync-held'&&method==='document/open'&&!overlayHeldOnce){
      overlayHeldOnce=true;process.stderr.write('OVERLAY_HELD\\n');
      const timer=setInterval(()=>{if(require('node:fs').existsSync(require('node:path').join(require('node:path').dirname(params.path),'release-overlay'))){clearInterval(timer);send(id,{version:params.version});}},5);return;
    }
    process.stderr.write('OVERLAY_RECEIVED\\n');
    send(id,{version:params.version});
    if(held && params.version > 1) {send(held.id,held.result);held=undefined;}
    return;
  }
  if(method==='navigation/possibleReferences') {
    occurrenceCount++; process.stderr.write('POSSIBLE_'+occurrenceCount+'\\n');
    const fs=require('node:fs'),url=require('node:url'),p=require('node:path');
    const paths=[params.path,p.join(p.dirname(params.path),'Dependency.fs')].filter(file=>snapshots.has(file)||fs.existsSync(file));
    const identity=file=>({uri:url.pathToFileURL(file).toString(),sourceHash:require('node:crypto').createHash('sha256').update(snapshots.get(file)?.text??fs.readFileSync(file,'utf8')).digest('hex'),version:snapshots.get(file)?.version??null});
    const result={locations:[],completeness:'unavailable',candidateKind:'same-name-occurrences',candidateCompleteness:'incomplete',queryName:'value',candidates:scenario==='occurrences-empty'?[]:paths.map(location),occurrenceIdentity:{origin:identity(params.path),targets:paths.map(identity)},occurrenceSearch:{scannedFiles:paths.length,maxFiles:32,maxRows:4096,maxLocations:200,discoveryPhase:'ready',discoveryComplete:true},version:params.version??null};
    if(scenario==='occurrences-overlap'&&occurrenceCount===3&&held){send(held.id,held.result);held=undefined;}
    if((scenario==='occurrences-overlap'&&occurrenceCount===2)||scenario==='occurrences-held'||(scenario==='occurrences-validation-held'&&occurrenceCount>1)){held={id,result};process.stderr.write('OCCURRENCE_HELD\\n');return;}
    send(id,result);return;
  }
  if(method.startsWith('navigation/')) {
    navigationCount++;
    process.stderr.write('NAVIGATION_'+navigationCount+'\\n');
    const result = {locations:[location(params.path)],completeness:scenario==='approximate'?'approximate':scenario==='unavailable'?'unavailable':'exact',source:'contract-fixture',version:(scenario==='wrong-version'||scenario==='pending-wrong-version')?params.version-1:params.version,reason:'deliberate contract fixture'};
    if(scenario.startsWith('ref-')) {result.completeness='unavailable';result.locations=[];}
    if(scenario==='ref-exact-after' && navigationCount>1) {result.completeness='exact';result.locations=[];}
    if(scenario==='ref-delay-first-exact' && navigationCount===1) {result.completeness='exact';result.locations=[location(params.path)];}
    if(scenario==='ref-exact-empty') {result.completeness='exact';result.locations=[];}
    if((scenario==='ref-delay-second'&&navigationCount===2)||((scenario==='ref-delay-first'||scenario==='ref-delay-first-exact')&&navigationCount===1)) {process.stderr.write('REF_HELD\\n');setTimeout(()=>send(id,result),120);return;}
    if(scenario.startsWith('candidates')) {result.completeness='unavailable';result.locations=[];result.candidates=[location(params.path),location(require('node:path').join(require('node:path').dirname(params.path),'Dependency.fs'))];}
    if(scenario==='candidates-truncated') {result.candidateTruncated=true;result.candidateTruncationReason='32 source files inspected; further targets omitted.';}
    if(scenario==='candidates-tiered') {result.likelyCandidates=[result.candidates[1]];result.candidateRanking='syntactic-evidence-v1';result.likelyCandidateTotal=2;result.candidateEvidence=[{location:result.candidates[1],evidence:['qualifier-spelling']}];}
    if(scenario.startsWith('pending') && (navigationCount===1 || scenario==='pending-forever')) {result.completeness='unavailable';result.locations=[];result.indexing={phase:'indexing',retryAfterMs:100,generation:1};process.stderr.write('INDEXING_PENDING\\n');}
    if(scenario==='pending-candidates' && navigationCount>1) {result.completeness='unavailable';result.locations=[];result.candidates=[location(params.path)];}
    if(scenario==='candidates-removed' && navigationCount>1) result.candidates=[];
    if(scenario==='candidates-shifted' && navigationCount>1) for(const item of result.candidates) {item.range.start.line++;item.range.end.line++;}
    if(scenario==='candidates-now-exact' && navigationCount>1) {result.completeness='exact';result.locations=[result.candidates[1]];delete result.candidates;}
    if(scenario==='candidates-validation-delayed' && navigationCount===2) {process.stderr.write('VALIDATION_HELD\\n');setTimeout(()=>send(id,result),100);return;}
    if(scenario==='candidates-validation-held' && navigationCount===2) {held={id,result};process.stderr.write('VALIDATION_HELD\\n');return;}
    if(scenario==='candidates-validation-held' && navigationCount>2 && held) {send(held.id,held.result);held=undefined;}
    if(scenario==='context') {result.completeness=params.projectPath?'exact':'unavailable';result.projectContextId=params.projectPath;result.locations[0].range.start.line=params.projectPath?.endsWith('A.fsproj')?1:3;}
    if((scenario==='stale' || scenario==='candidates-held' || scenario==='pending-first-held') && navigationCount===1) {held={id,result};process.stderr.write('NAVIGATION_HELD\\n');return;}
    if(method==='navigation/hover') {result.hover={markdown:'library text',isExternal:scenario==='external-hover'};result.ownership=scenario==='external-hover'?'external':'workspace';}
    send(id,result);return;
  }
  if(method==='workspace/tree') {send(id,{nodes:['A','B'].map(name=>({id:name,label:name,kind:'project',path:require('node:path').join(${JSON.stringify(directory)},name+'.fsproj'),children:[{id:name+'-shared',label:'Main.fs',kind:'file',path:require('node:path').join(${JSON.stringify(directory)},'Main.fs')}]}))});return;}
  send(id,{});
});
`, { mode: 0o755 });
  const editor = await createEditor(directory, binary, configuration);
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
    assert.equal(editor.status.text, 'F#: references incomplete');
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


const offer = editor => editor.statusItems.find(item => item.visible && item.command?.command === 'fsharpNavigator.chooseCandidates');
async function choose(editor) { const item = offer(editor); assert.ok(item); await editor.executeCommand(item.command.command, ...item.command.arguments); return editor.quickPicks.at(-1); }

test('ambiguous F12 publishes only a count; explicit chooser includes all unverified alternatives', async t => {
  const editor = await setup(t, 'candidates'); const doc = await editor.open('Main.fs');
  const pos = editor.position(doc, 2, 'value');
  assert.equal(await editor.providers.definition.provideDefinition(doc, pos, editor.uncancelled), undefined);
  assert.equal(editor.openedUris.length, 0); assert.equal(editor.quickPicks.length, 0); assert.equal(editor.candidateViews.length, 0);
  assert.match(offer(editor).text, /2 possible definitions \(choose\)/);
  const picker = await choose(editor); assert.equal(picker.items.length, 2);
  assert.match(picker.title, /incomplete/); assert.match(picker.items[0].detail, /unverified/);
  picker.selectedItems = [picker.items[1]]; await settle(10);
  assert.equal(editor.openedUris.length, 0, 'selection alone never navigates');
  assert.equal(await editor.providers.references.provideReferences(doc, pos, {includeDeclaration:true}, editor.uncancelled), undefined);
  editor.edit(doc, doc.text+'\n'); picker.accept(picker.items[1]); await settle(20);
  assert.equal(editor.openedUris.length, 0); assert.equal(picker.disposed, true); assert.equal(offer(editor), undefined);
});

test('candidate cap remains visible in offer and explicit chooser', async t => {
  const editor = await setup(t, 'candidates-truncated'); const doc = await editor.open('Main.fs');
  await editor.providers.definition.provideDefinition(doc, editor.position(doc,2,'value'), editor.uncancelled);
  assert.match(offer(editor).tooltip, /Candidate limit reached: 32 source files inspected/);
  assert.match((await choose(editor)).placeholder, /Candidate limit reached/);
});

for (const change of ['cancel', 'dirty dependency']) {
  test(`${change} suppresses held ambiguous offers`, async t => {
    const editor=await setup(t,'candidates-held'),doc=await editor.open('Main.fs'),dependency=await editor.open('Dependency.fs'),cancel=editor.cancellationSource();
    const pending=editor.providers.definition.provideDefinition(doc,editor.position(doc,2,'value'),cancel.token);
    await editor.waitForOutput('NAVIGATION_HELD');
    if(change==='cancel') cancel.cancel(); else editor.edit(dependency,'module Dependency\nlet replacement=2\n');
    assert.equal(await pending,undefined); assert.equal(offer(editor),undefined); assert.equal(editor.quickPicks.length,0); assert.equal(editor.api.lastResult,undefined); cancel.dispose();
  });
}

test('old generation and malformed commands cannot open or revive a chooser', async t => {
  const editor=await setup(t,'candidates'),doc=await editor.open('Main.fs'),pos=editor.position(doc,2,'value');
  await editor.providers.definition.provideDefinition(doc,pos,editor.uncancelled); const old=offer(editor).command;
  await editor.providers.definition.provideDefinition(doc,pos,editor.uncancelled);
  for(const value of [old.arguments[0], 'bad', {}, null, NaN]) await editor.executeCommand('fsharpNavigator.chooseCandidates',value);
  assert.equal(editor.quickPicks.length,0); const picker=await choose(editor);
  picker.accept({ ...picker.items[0] }); await settle(10); assert.equal(editor.openedUris.length,0,'foreign row identity rejected');
  picker.hide(); await editor.executeCommand(old.command,...old.arguments); assert.equal(editor.quickPicks.length,1);
});

for(const scenario of ['candidates-removed','candidates-shifted','candidates-now-exact']) {
  test(`explicit acceptance revalidates full location: ${scenario}`,async t=>{
    const editor=await setup(t,scenario),doc=await editor.open('Main.fs');
    await editor.providers.definition.provideDefinition(doc,editor.position(doc,2,'value'),editor.uncancelled);
    const picker=await choose(editor);picker.accept(picker.items[1]); await until(()=>picker.disposed);
    assert.equal(editor.openedUris.length,scenario==='candidates-now-exact'?1:0);
  });
}

for(const change of ['hide','edit','new-request','caret','editor']) {
  test(`pending acceptance is cancelled by ${change} and cannot publish a late jump`,async t=>{
    const editor=await setup(t,'candidates-validation-held'),doc=await editor.open('Main.fs'),pos=editor.position(doc,2,'value');editor.focus(doc,pos);
    await editor.providers.definition.provideDefinition(doc,pos,editor.uncancelled);const picker=await choose(editor);
    picker.accept(picker.items[0]);await editor.waitForOutput('VALIDATION_HELD');
    picker.accept(picker.items[1]);assert.equal((editor.output.join('').match(/NAVIGATION_\d+/g)||[]).length,2,'duplicate acceptance does not dispatch');
    if(change==='hide')picker.hide();
    if(change==='edit')editor.edit(doc,doc.text+'\n');
    if(change==='new-request')await editor.providers.definition.provideDefinition(doc,pos,editor.uncancelled);
    if(change==='caret'){editor.move(editor.position(doc,1,'value'));editor.move(pos);}
    if(change==='editor')editor.focus(doc,pos);
    await editor.waitForOutput('NATIVE_CANCEL_RECEIVED');await settle(20);
    assert.equal(editor.openedUris.length,0);assert.equal(picker.disposed,true);
    if(change==='new-request')assert.ok(offer(editor),'late acceptance cannot clear new offer');else assert.equal(offer(editor),undefined);
  });
}

test('duplicate acceptance opens exactly once after one fresh native validation',async t=>{
  const editor=await setup(t,'candidates'),doc=await editor.open('Main.fs');
  await editor.providers.definition.provideDefinition(doc,editor.position(doc,2,'value'),editor.uncancelled);
  const picker=await choose(editor);picker.accept();picker.accept();await until(()=>picker.disposed);
  assert.equal(editor.openedUris.length,1);assert.equal((editor.output.join('').match(/NAVIGATION_\d+/g)||[]).length,2);
});

test('changing selection during validation cannot redirect or complete the old acceptance',async t=>{
  const editor=await setup(t,'candidates-validation-delayed'),doc=await editor.open('Main.fs');
  await editor.providers.definition.provideDefinition(doc,editor.position(doc,2,'value'),editor.uncancelled);
  const picker=await choose(editor);picker.accept(picker.items[0]);await editor.waitForOutput('VALIDATION_HELD');
  picker.selectedItems=[picker.items[1]];await until(()=>picker.disposed);assert.equal(editor.openedUris.length,0);
});

test('settled offers are revoked by caret and same-document pane changes',async t=>{
  const editor=await setup(t,'candidates'),doc=await editor.open('Main.fs'),pos=editor.position(doc,2,'value');editor.focus(doc,pos);
  await editor.providers.definition.provideDefinition(doc,pos,editor.uncancelled);editor.move(editor.position(doc,1,'value'));assert.equal(offer(editor),undefined);
  editor.move(pos);await editor.providers.definition.provideDefinition(doc,pos,editor.uncancelled);editor.focus(doc,pos);assert.equal(offer(editor),undefined);
});

// Independent membership/transport controls: the consumer stays unchanged while
// a shared physical dependency lives outside both workspace roots.
async function linkedMembershipSetup(t, initialMode = 'include') {
  const tempRoot = process.env.TMPDIR || path.resolve(__dirname, '../../.cache/test-tmp');
  await fs.mkdir(tempRoot, { recursive: true });
  const root = await fs.mkdtemp(path.join(tempRoot, 'fsnav-linked-membership-'));
  const roots = ['A', 'B'].map(name => path.join(root, name));
  for (const directory of roots) {
    await fs.mkdir(directory);
    await fs.writeFile(path.join(directory, 'Main.fs'), 'module Main\nlet result = value\n');
  }
  const linked = path.join(root, 'Shared.fs');
  await fs.writeFile(linked, 'let value = 1\n');
  const modeFile = path.join(root, 'mode');
  const logFile = path.join(root, 'requests.jsonl');
  await fs.writeFile(modeFile, initialMode);
  const binary = path.join(root, 'engine');
  await fs.writeFile(binary, `#!${process.execPath}
const fs = require('node:fs');
const readline = require('node:readline');
const linked = ${JSON.stringify(linked)};
let root;
const overlays = new Map();
readline.createInterface({input:process.stdin}).on('line', line => {
  const {id,method,params} = JSON.parse(line);
  if(method === 'shutdown') process.exit(0);
  if(!id) return;
  if(method === 'workspace/open') root=params.root;
  fs.appendFileSync(${JSON.stringify(logFile)},JSON.stringify({root,method,params})+'\\n');
  const send = result => process.stdout.write(JSON.stringify({id,result})+'\\n');
  if(method === 'workspace/tree') {
    const mode=fs.readFileSync(${JSON.stringify(modeFile)},'utf8');
    if(mode === 'fail') {process.stdout.write(JSON.stringify({id,error:{code:1,message:'membership unavailable'}})+'\\n');return;}
    send({nodes:[],completeness:mode.includes('approximate')?'approximate':'exact',projects:[{files:mode.startsWith('include')?[linked]:[]}]});return;
  }
  if(method === 'document/open' || method === 'document/change') overlays.set(params.path,params.text);
  if(method === 'document/close') overlays.delete(params.path);
  if(method === 'navigation/definition') {
    const text=overlays.get(linked)||fs.readFileSync(linked,'utf8');
    const column=text.indexOf('value');
    send({completeness:'exact',source:'membership-contract-fixture',version:params.version,locations:[{path:linked,range:{start:{line:0,character:column},end:{line:0,character:column+5}}}]});return;
  }
  send({version:params?.version});
});
`, { mode: 0o755 });
  const editor = await createEditor(root, binary, { workspaceDirectories: roots });
  t.after(async () => { editor.dispose(); await fs.rm(root, { recursive: true, force: true }); });
  const consumer = await editor.open('A/Main.fs');
  const dependency = await editor.open('Shared.fs');
  const query = (document = consumer) => editor.providers.definition.provideDefinition(document, editor.position(document, 1, 'value'), editor.uncancelled);
  const requests = async () => (await fs.readFile(logFile, 'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse);
  return { editor, consumer, dependency, linked, roots, query, requests, mode: value => fs.writeFile(modeFile, value) };
}

test('local navigation stays lazy and uncached foreign exclusions are refreshed', async t => {
  const fixture = await linkedMembershipSetup(t, 'exclude');
  assert.equal((await fixture.query())[0].range.start.character, 4);
  assert.equal((await fixture.requests()).filter(row => row.method === 'workspace/tree').length, 0);
  fixture.editor.edit(fixture.dependency, 'let  value = 2\n');
  assert.equal((await fixture.query())[0].range.start.character, 4, 'Certified unrelated source is not broadcast');
  assert.equal((await fixture.requests()).filter(row => row.params?.path === fixture.linked).length, 0);
  await fixture.mode('include');
  assert.equal((await fixture.query())[0].range.start.character, 5, 'Same dirty version is sent after newly established membership');
  assert.equal(fixture.consumer.version, 1);
  assert.equal((await fixture.requests()).filter(row => row.method === 'workspace/tree').length, 2);
});

for (const membership of ['exclude-approximate', 'fail']) {
  test(`${membership} membership suppresses navigation instead of ignoring a foreign dirty buffer`, async t => {
    const fixture = await linkedMembershipSetup(t, membership);
    fixture.editor.edit(fixture.dependency, 'let  value = 2\n');
    assert.equal(await fixture.query(), undefined);
    assert.equal(fixture.editor.api.lastResult, undefined);
    assert.equal((await fixture.requests()).filter(row => row.method.startsWith('navigation/')).length, 0);
  });
}

test('positive linked membership synchronizes dirty text and save/close revokes every owning session', async t => {
  const fixture = await linkedMembershipSetup(t, 'include-approximate');
  const second = await fixture.editor.open('B/Main.fs');
  fixture.editor.edit(fixture.dependency, 'let  value = 2\n');
  for (const document of [fixture.consumer, second]) assert.equal((await fixture.query(document))[0].range.start.character, 5);
  fixture.editor.save(fixture.dependency);
  for (const document of [fixture.consumer, second]) assert.equal((await fixture.query(document))[0].range.start.character, 4);
  fixture.editor.edit(fixture.dependency, 'let   value = 3\n');
  for (const document of [fixture.consumer, second]) assert.equal((await fixture.query(document))[0].range.start.character, 6);
  fixture.editor.close(fixture.dependency);
  for (const document of [fixture.consumer, second]) assert.equal((await fixture.query(document))[0].range.start.character, 4);
  const requests = await fixture.requests();
  for (const root of fixture.roots) {
    assert.equal(requests.filter(row => row.root === root && row.method === 'document/close' && row.params.path === fixture.linked).length, 2);
  }
});

test('a newly certified membership exclusion revokes a previously forwarded linked overlay', async t => {
  const fixture = await linkedMembershipSetup(t);
  fixture.editor.edit(fixture.dependency, 'let  value = 2\n');
  assert.equal((await fixture.query())[0].range.start.character, 5);
  await fixture.mode('exclude-approximate');
  assert.equal(await fixture.query(), undefined, 'Unproven absence cannot authorize an answer');
  await fixture.mode('exclude');
  assert.equal((await fixture.query())[0].range.start.character, 4);
  assert.equal((await fixture.requests()).filter(row => row.method === 'document/close' && row.params.path === fixture.linked).length, 1);
});

const settle = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
async function until(predicate) {
  for (let i = 0; i < 200; i++) { if (predicate()) return; await settle(5); }
  assert.fail('Expected adapter state did not become observable');
}

test('one definition invocation continues pending indexing to exact without implicit follow-up invocation', async t => {
  const editor = await setup(t, 'pending-once');
  const doc = await editor.open('Main.fs');
  const result = await editor.providers.definition.provideDefinition(doc, editor.position(doc, 2, 'value'), editor.uncancelled);
  assert.equal(result.length, 1);
  assert.match(editor.output.join(''), /NAVIGATION_2/);
  assert.equal(editor.status.command, 'fsharpNavigator.selectMode');
});

test('one definition invocation continues pending indexing to unforced incomplete candidates', async t => {
  const editor = await setup(t, 'pending-candidates');
  const doc = await editor.open('Main.fs');
  assert.equal(await editor.providers.definition.provideDefinition(doc, editor.position(doc, 2, 'value'), editor.uncancelled), undefined);
  assert.equal(editor.api.candidates.getChildren().length, 1);
  assert.equal(editor.openedUris.length, 0);
});

for (const change of ['token', 'status-command', 'edit', 'caret-away-back', 'active-editor', 'other-pane-same-document']) {
  test(`pending continuation stops without a retry after ${change}`, async t => {
    const editor = await setup(t, 'pending-forever');
    const doc = await editor.open('Main.fs'), other = await editor.open('Dependency.fs');
    const position = editor.position(doc, 2, 'value'); editor.focus(doc, position);
    const token = editor.cancellationSource();
    const pending = editor.providers.definition.provideDefinition(doc, position, token.token);
    await until(() => editor.status.command === 'fsharpNavigator.cancelDefinition');
    const before = editor.output.join('').match(/NAVIGATION_\d+/g)?.length || 0;
    if (change === 'token') token.cancel();
    if (change === 'status-command') await editor.executeCommand('fsharpNavigator.cancelDefinition');
    if (change === 'edit') editor.edit(doc, doc.text + '\n');
    if (change === 'caret-away-back') { editor.move(editor.position(doc, 1, 'value')); editor.move(position); }
    if (change === 'active-editor') editor.focus(other, editor.position(other, 1, 'value'));
    if (change === 'other-pane-same-document') editor.focus(doc, editor.position(doc, 1, 'value'));
    assert.equal(await pending, undefined);
    await settle(150);
    assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g)?.length || 0, before);
    assert.equal(editor.api.candidates.getChildren().length, 0);
  });
}

test('new definition cancels previous continuation and restores the status command after fast exact success', async t => {
  const editor = await setup(t, 'pending-once');
  const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value');
  const previous = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  await until(() => editor.status.command === 'fsharpNavigator.cancelDefinition');
  const current = await editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  assert.equal(await previous, undefined); assert.equal(current.length, 1);
  assert.equal(editor.status.command, 'fsharpNavigator.selectMode');
  await settle(150);
  assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g).length, 2);
});

for (const scenario of ['pending-forever', 'pending-init']) {
  test(`whole indexed definition has a bounded deadline including ${scenario}`, async t => {
    const editor = await setup(t, scenario);
    const doc = await editor.open('Main.fs'); const started = performance.now();
    assert.equal(await editor.providers.definition.provideDefinition(doc, editor.position(doc, 2, 'value'), editor.uncancelled), undefined);
    assert.ok(performance.now() - started < 3700, 'Provider must not inherit the transport 30-second bound');
    assert.equal(editor.status.text, 'F#: indexing budget reached');
    const count = editor.output.join('').match(/NAVIGATION_\d+/g)?.length || 0;
    await settle(150);
    assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g)?.length || 0, count);
    assert.equal(editor.api.candidates.getChildren().length, 0);
  });
}

test('likely tier is ordered first without hiding any collected alternatives', async t => {
  const editor=await setup(t,'candidates-tiered'),doc=await editor.open('Main.fs');
  await editor.providers.definition.provideDefinition(doc,editor.position(doc,2,'value'),editor.uncancelled);
  assert.equal(editor.api.candidates.getChildren().length,1);
  assert.match(offer(editor).text,/2 possible definitions/);
  const picker=await choose(editor);assert.equal(picker.items.length,2);
  assert.match(picker.items[0].detail,/Matching qualifier/);
  assert.equal(picker.items[0].target,editor.api.candidates.getChildren()[0]);
  editor.edit(doc,doc.text+'\n');assert.equal(picker.disposed,true);assert.equal(editor.api.candidates.getAllCandidates().length,0);
});

test('caret move away and back during the first dispatch cancels before any continuation', async t => {
  const editor = await setup(t, 'pending-first-held');
  const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value');
  editor.focus(doc, position);
  const pending = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  await editor.waitForOutput('NAVIGATION_HELD');
  editor.move(editor.position(doc, 1, 'value')); editor.move(position);
  assert.equal(await pending, undefined);
  editor.edit(doc, doc.text + '\n');
  await settle(150);
  assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g).length, 1);
  assert.equal(editor.api.candidates.getChildren().length, 0);
});

test('candidate provider completion never waits on chooser interaction', async t => {
  const editor=await setup(t,'candidates'),doc=await editor.open('Main.fs');
  const started=performance.now();await editor.providers.definition.provideDefinition(doc,editor.position(doc,2,'value'),editor.uncancelled);
  assert.ok(performance.now()-started<3000);assert.equal(editor.quickPicks.length,0);assert.ok(offer(editor));
  const picker=await choose(editor);picker.hide();assert.equal(offer(editor),undefined);
});

test('incomplete references explain that withholding a set does not mean no references exist', async t => {
  const editor = await setup(t, 'unavailable');
  const doc = await editor.open('Main.fs');
  assert.equal(await editor.providers.references.provideReferences(doc, editor.position(doc, 2, 'value'), { includeDeclaration: true }, editor.uncancelled), undefined);
  assert.equal(editor.status.text, 'F#: references incomplete');
  assert.match(editor.warnings.join(''), /Scope was not fully searched.*not evidence that there are no uses/);
});

async function referenceNoticeSetup(t, scenario='unavailable') {
  const editor=await setup(t,scenario,{warning:call=>new Promise(resolve=>{call.resolve=resolve;})});
  const doc=await editor.open('Main.fs'),position=editor.position(doc,2,'value');editor.focus(doc,position);
  const read=(includeDeclaration=true,token=editor.uncancelled)=>editor.providers.references.provideReferences(doc,position,{includeDeclaration},token);
  return {editor,doc,position,read};
}
const possibleCount=editor=>(editor.output.join('').match(/POSSIBLE_\d+/g)||[]).length;

test('incomplete reference warning offers one explicit search without changing standard FAR',async t=>{
  const {editor,doc,read}=await referenceNoticeSetup(t);
  assert.equal(await read(),undefined);assert.equal(possibleCount(editor),0);
  assert.deepEqual(editor.warningCalls[0].actions,['Show Possible References']);
  assert.equal(editor.status.tooltip,editor.warningCalls[0].text);
  editor.warningCalls[0].resolve('Show Possible References');await editor.waitForOutput('POSSIBLE_1');
  await until(()=>editor.api.lastPossibleReferences!==undefined);assert.equal(possibleCount(editor),1);
  assert.equal(editor.api.lastPossibleReferences.occurrenceIdentity.origin.uri,doc.uri.toString());
  assert.equal(editor.api.lastResult.completeness,'unavailable');assert.equal(editor.openedUris.length,0);
});

test('identical include-declaration retries share a valid newest-token action',async t=>{
  const {editor,read}=await referenceNoticeSetup(t);const old=editor.cancellationSource(),latest=editor.cancellationSource();
  await read(true,old.token);await read(false,latest.token);assert.equal(editor.warningCalls.length,1);
  old.cancel();editor.warningCalls[0].resolve('Show Possible References');await editor.waitForOutput('POSSIBLE_1');
  assert.equal(possibleCount(editor),1);old.dispose();latest.dispose();
});

test('the newest repeated reference token still revokes the shared action after provider return',async t=>{
  const {editor,read}=await referenceNoticeSetup(t);const old=editor.cancellationSource(),latest=editor.cancellationSource();
  await read(true,old.token);await read(false,latest.token);latest.cancel();
  editor.warningCalls[0].resolve('Show Possible References');await settle(20);assert.equal(possibleCount(editor),0);
  old.dispose();latest.dispose();
});

for(const change of ['edit','caret-away-back','other-editor','workspace','definition','newest-token','different-origin']) {
  test(`deferred reference action is inert after ${change}`,async t=>{
    const {editor,doc,position,read}=await referenceNoticeSetup(t);const token=editor.cancellationSource();
    await read(true,token.token);const warning=editor.warningCalls[0];
    if(change==='edit')editor.edit(doc,doc.text+'\n');
    if(change==='caret-away-back'){editor.move(editor.position(doc,1,'value'));editor.move(position);}
    if(change==='other-editor')editor.focus(doc,position);
    if(change==='workspace')editor.changeFolders();
    if(change==='definition')await editor.providers.definition.provideDefinition(doc,position,editor.uncancelled);
    if(change==='newest-token')token.cancel();
    if(change==='different-origin')await editor.providers.references.provideReferences(doc,editor.position(doc,1,'value'),{includeDeclaration:true},editor.uncancelled);
    warning.resolve('Show Possible References');await settle(30);assert.equal(possibleCount(editor),0);token.dispose();
  });
}

test('one pending notification bounds different-context retries and stale buttons remain inert',async t=>{
  const {editor,doc,read}=await referenceNoticeSetup(t);await read();
  for(let i=0;i<4;i++)await editor.providers.references.provideReferences(doc,editor.position(doc,i%2+1,'value'),{includeDeclaration:true},editor.uncancelled);
  assert.equal(editor.warningCalls.length,1);editor.warningCalls[0].resolve('Show Possible References');await settle(20);assert.equal(possibleCount(editor),0);
});

test('dismissal permits a fresh explicit retry without a cooldown',async t=>{
  const {editor,read}=await referenceNoticeSetup(t);await read();editor.warningCalls[0].resolve(undefined);await settle(5);
  await read();assert.equal(editor.warningCalls.length,2);editor.warningCalls[1].resolve('Show Possible References');await editor.waitForOutput('POSSIBLE_1');
});

test('click during replacement request cannot use the previous incomplete answer',async t=>{
  const {editor,read}=await referenceNoticeSetup(t,'ref-delay-second');await read();const newer=read(false);
  await editor.waitForOutput('REF_HELD');editor.warningCalls[0].resolve('Show Possible References');await newer;await settle(20);
  assert.equal(possibleCount(editor),0);assert.equal(editor.warningCalls.length,2);editor.warningCalls[1].resolve(undefined);
});

test('late superseded reference response cannot overwrite latest status or notification',async t=>{
  const {editor,doc,read}=await referenceNoticeSetup(t,'ref-delay-first');const first=read();await editor.waitForOutput('REF_HELD');
  await editor.providers.references.provideReferences(doc,editor.position(doc,1,'value'),{includeDeclaration:false},editor.uncancelled);
  const result=editor.api.lastResult;await first;assert.equal(editor.api.lastResult,result);assert.equal(editor.warningCalls.length,1);
  editor.warningCalls[0].resolve(undefined);
});

test('notification supersession does not suppress an independently valid exact reference answer',async t=>{
  const {editor,doc,read}=await referenceNoticeSetup(t,'ref-delay-first-exact');const pending=read();await editor.waitForOutput('REF_HELD');
  await editor.providers.references.provideReferences(doc,editor.position(doc,1,'value'),{includeDeclaration:false},editor.uncancelled);
  const latest=editor.api.lastResult,status=editor.status.text;const exact=await pending;
  assert.equal(exact.length,1);assert.equal(editor.api.lastResult,latest);assert.equal(editor.status.text,status);assert.equal(editor.warningCalls.length,1);
  editor.warningCalls[0].resolve(undefined);
});

test('F12 supersedes an in-flight reference before any warning exists',async t=>{
  const {editor,doc,position,read}=await referenceNoticeSetup(t,'ref-delay-first');const pending=read();await editor.waitForOutput('REF_HELD');
  await editor.providers.definition.provideDefinition(doc,position,editor.uncancelled);const result=editor.api.lastResult,status=editor.status.text,warnings=editor.warningCalls.length;
  await pending;assert.equal(editor.api.lastResult,result);assert.equal(editor.status.text,status);assert.equal(editor.warningCalls.length,warnings);
  for(const warning of editor.warningCalls)warning.resolve(undefined);
});

test('redundant same-position selection events preserve a current shared reference action',async t=>{
  const {editor,position,read}=await referenceNoticeSetup(t);await read();editor.move(position);await read(false);
  assert.equal(editor.warningCalls.length,1);editor.warningCalls[0].resolve('Show Possible References');await editor.waitForOutput('POSSIBLE_1');
});

test('exact empty stays silent and revokes a pending incomplete-reference action',async t=>{
  const {editor,read}=await referenceNoticeSetup(t,'ref-exact-after');await read();assert.deepEqual(await read(false),[]);
  assert.equal(editor.warningCalls.length,1);editor.warningCalls[0].resolve('Show Possible References');await settle(20);assert.equal(possibleCount(editor),0);
  const fresh=await referenceNoticeSetup(t,'ref-exact-empty');assert.deepEqual(await fresh.read(false),[]);assert.equal(fresh.editor.warningCalls.length,0);
});

test('a pending response for the wrong version cannot authorize continuation', async t => {
  const editor = await setup(t, 'pending-wrong-version');
  const doc = await editor.open('Main.fs');
  assert.equal(await editor.providers.definition.provideDefinition(doc, editor.position(doc, 2, 'value'), editor.uncancelled), undefined);
  await settle(150);
  assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g).length, 1);
});


test('identical UI definitions share one in-flight continuation without an early empty result', async t => {
  const editor = await setup(t, 'pending-once');
  const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value'); editor.focus(doc, position);
  let finished = false;
  const first = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled).then(value => { finished = true; return value; });
  await until(() => editor.status.command === 'fsharpNavigator.cancelDefinition');
  const second = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  assert.equal(finished, false);
  const results = await Promise.all([first, second]);
  assert.equal(results[0].length, 1); assert.deepEqual(results[0], results[1]);
  assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g).length, 2, 'One initial request and one continuation, no duplicate');
});
for (const caller of ['original', 'joined']) {
  test(`cancelling the ${caller} UI caller cancels the shared native flight`, async t => {
    const editor = await setup(t, 'pending-first-held');
    const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value'); editor.focus(doc, position);
    const original = editor.cancellationSource(), joined = editor.cancellationSource();
    const first = editor.providers.definition.provideDefinition(doc, position, original.token);
    await editor.waitForOutput('NAVIGATION_HELD');
    const second = editor.providers.definition.provideDefinition(doc, position, joined.token);
    (caller === 'original' ? original : joined).cancel();
    assert.deepEqual(await Promise.all([first, second]), [undefined, undefined]);
    await editor.waitForOutput('NATIVE_CANCEL_RECEIVED');
    assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g).length, 1);
    assert.equal(editor.api.candidates.getChildren().length, 0);
  });
}
test('joining an old UI flight retains its original deadline', async t => {
  const editor = await setup(t, 'pending-init');
  const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value'); editor.focus(doc, position);
  const started = performance.now();
  const first = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  await editor.waitForOutput('INIT_HELD'); await settle(2400);
  const second = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  assert.deepEqual(await Promise.all([first, second]), [undefined, undefined]);
  assert.ok(performance.now() - started < 3700);
});

for (const change of ['edit', 'caret', 'pane']) {
  test(`a shared UI flight is invalidated by ${change} before a new definition`, async t => {
    const editor = await setup(t, 'pending-first-held');
    const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value'); editor.focus(doc, position);
    const first = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
    await editor.waitForOutput('NAVIGATION_HELD');
    const joined = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
    if (change === 'edit') editor.edit(doc, doc.text + '\n');
    if (change === 'caret') editor.move(editor.position(doc, 1, 'value'));
    if (change === 'pane') editor.focus(doc, position);
    const next = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
    assert.deepEqual(await Promise.all([first, joined]), [undefined, undefined]);
    assert.equal((await next).length, 1);
    assert.equal(editor.output.join('').match(/NAVIGATION_\d+/g).length, 2);
  });
}

test('an offscreen request supersedes the previous UI flight before an immediate matching UI request', async t => {
  const editor = await setup(t, 'pending-first-held');
  const doc = await editor.open('Main.fs'), position = editor.position(doc, 2, 'value'); editor.focus(doc, position);
  const first = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  await editor.waitForOutput('NAVIGATION_HELD');
  const offscreen = editor.providers.definition.provideDefinition(doc, editor.position(doc, 1, 'value'), editor.uncancelled);
  const latest = editor.providers.definition.provideDefinition(doc, position, editor.uncancelled);
  assert.equal(await first, undefined); assert.equal(await offscreen, undefined);
  assert.equal((await latest).length, 1, 'Must not join the superseded UI promise');
});


test('possible references are explicit, labelled occurrences and do not expand standard FAR', async t => {
  const editor = await setup(t, 'occurrences');const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
  assert.equal((await editor.providers.references.provideReferences(doc,editor.position(doc,2,'value'),{includeDeclaration:true},editor.uncancelled)).length,1);
  assert.ok(!editor.output.join('').includes('POSSIBLE_'));
  await editor.executeCommand('fsharpNavigator.showPossibleReferences');
  const rows=editor.api.possibleReferences.getChildren();assert.equal(rows.length,2);assert.equal(editor.openedUris.length,0);
  const view=editor.candidateViews.find(v=>v.id==='fsharpNavigator.possibleReferences');assert.match(view.message,/inactive branches and unrelated/);assert.match(view.message,/Not a complete reference set, even when empty/);
  assert.equal(editor.api.candidates.getChildren().length,0);
  await editor.executeCommand('fsharpNavigator.openPossibleReference',rows[1].id);assert.equal(editor.openedUris.length,1);
  await editor.open('Dependency.fs');assert.equal(editor.api.possibleReferences.getChildren().length,2,'Clean target open must retain settled occurrence list');
  await editor.executeCommand('fsharpNavigator.openPossibleReference',rows[0].id);assert.equal(editor.openedUris.length,2);
});
test('an empty occurrence view is explicitly incomplete and visible', async t => {
  const editor=await setup(t,'occurrences-empty');const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
  await editor.executeCommand('fsharpNavigator.showPossibleReferences');assert.equal(editor.api.possibleReferences.getChildren().length,0);
  assert.match(editor.candidateViews[0].message,/Showing 0 matches/);assert.match(editor.candidateViews[0].message,/even when empty/);
});
for(const change of ['delete','rebind','dirty-dependency'])test(`possible-reference click rejects ${change} at the same old row`,async t=>{
 const editor=await setup(t,'occurrences');const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
 await editor.executeCommand('fsharpNavigator.showPossibleReferences');const row=editor.api.possibleReferences.getChildren()[1];
 if(change==='delete')await fs.rm(row.location.uri.fsPath);
 if(change==='rebind')await fs.writeFile(row.location.uri.fsPath,'module Dependency\nlet value = "different meaning"\n');
 if(change==='dirty-dependency'){const dependency=await editor.open('Dependency.fs');editor.edit(dependency,'let value = "dirty meaning"\n');}
 await editor.executeCommand('fsharpNavigator.openPossibleReference',row.id);assert.equal(editor.openedUris.length,0);assert.equal(editor.api.possibleReferences.getChildren().length,0);
});
for(const change of ['token','caret','edit','restart'])test(`possible-reference pending query cancels on ${change}`,async t=>{
 const settings={};const editor=await setup(t,'occurrences-held',settings);const cancel=editor.cancellationSource();settings.progressToken=cancel.token;
 const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
 const pending=editor.executeCommand('fsharpNavigator.showPossibleReferences');await editor.waitForOutput('OCCURRENCE_HELD');
 if(change==='token')cancel.cancel();if(change==='caret')editor.move(editor.position(doc,1,'value'));if(change==='edit')editor.edit(doc,doc.text+'\n');if(change==='restart')editor.api.restart();
 await pending;assert.equal(editor.api.possibleReferences.getChildren().length,0);assert.equal(editor.openedUris.length,0);
});
test('possible-reference validation cannot open after a dirty edit overtakes it',async t=>{
 const editor=await setup(t,'occurrences-validation-held');const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
 await editor.executeCommand('fsharpNavigator.showPossibleReferences');const row=editor.api.possibleReferences.getChildren()[1];
 const pending=editor.executeCommand('fsharpNavigator.openPossibleReference',row.id);await editor.waitForOutput('OCCURRENCE_HELD');editor.edit(doc,doc.text+'\n');await pending;
 assert.equal(editor.openedUris.length,0);assert.equal(editor.api.possibleReferences.getChildren().length,0);
});
test('possible references reject nonindexed mode before starting the backend',async t=>{
 const editor=await setup(t,'occurrences',{mode:'semantic'});const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
 await editor.executeCommand('fsharpNavigator.showPossibleReferences');assert.equal(editor.api.processIds.length,0);assert.match(editor.warnings.join(''),/only in Indexed mode/);
});

test('F12 cancels an explicit occurrence search without waiting for its native response',async t=>{
 const editor=await setup(t,'occurrences-held');const doc=await editor.open('Main.fs'),position=editor.position(doc,2,'value');editor.focus(doc,position);
 const pending=editor.executeCommand('fsharpNavigator.showPossibleReferences');await editor.waitForOutput('OCCURRENCE_HELD');
 const definition=await editor.providers.definition.provideDefinition(doc,position,editor.uncancelled);await pending;
 assert.equal(definition.length,1);await editor.waitForOutput('NATIVE_CANCEL_RECEIVED');assert.equal(editor.api.possibleReferences.getChildren().length,0);
});
test('caret away and back before progress callback prevents obsolete occurrence work',async t=>{
 let release;const settings={progress:(task,token)=>new Promise(resolve=>{release=()=>resolve(task({report(){}},token));})};
 const editor=await setup(t,'occurrences',settings);const doc=await editor.open('Main.fs'),position=editor.position(doc,2,'value');editor.focus(doc,position);
 const pending=editor.executeCommand('fsharpNavigator.showPossibleReferences');await until(()=>!!release);editor.move(editor.position(doc,1,'value'));editor.move(position);
 await pending;release();await settle(20);assert.ok(!editor.output.join('').includes('POSSIBLE_'));assert.equal(editor.api.processIds.length,0);
});
test('cancel during occurrence reveal clears rows and rejects delayed stale tree items',async t=>{
 let release;const settings={reveal:()=>new Promise(resolve=>{release=resolve;})};const editor=await setup(t,'occurrences',settings),cancel=editor.cancellationSource();settings.progressToken=cancel.token;
 const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));const pending=editor.executeCommand('fsharpNavigator.showPossibleReferences');await until(()=>!!release);
 const row=editor.api.possibleReferences.getChildren()[0];cancel.cancel();await pending;assert.equal(editor.api.possibleReferences.getChildren().length,0);assert.throws(()=>editor.api.possibleReferences.getTreeItem(row));release();await settle(20);assert.equal(editor.api.possibleReferences.getChildren().length,0);
});

test('only the latest overlapping occurrence click opens a target',async t=>{
 const editor=await setup(t,'occurrences-overlap');const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
 await editor.executeCommand('fsharpNavigator.showPossibleReferences');const rows=editor.api.possibleReferences.getChildren();
 const first=editor.executeCommand('fsharpNavigator.openPossibleReference',rows[1].id);await editor.waitForOutput('OCCURRENCE_HELD');
 await editor.executeCommand('fsharpNavigator.openPossibleReference',rows[0].id);await first;assert.deepEqual(editor.openedUris.map(u=>u.toString()),[doc.uri.toString()]);
});
test('an explicit occurrence query has its own bounded five-second deadline',async t=>{
 const editor=await setup(t,'occurrences-held');const doc=await editor.open('Main.fs');editor.focus(doc,editor.position(doc,2,'value'));
 const started=performance.now();await editor.executeCommand('fsharpNavigator.showPossibleReferences');assert.ok(performance.now()-started<5700);await editor.waitForOutput('NATIVE_CANCEL_RECEIVED');assert.equal(editor.api.possibleReferences.getChildren().length,0);
});

test('F12 preemption prevents a delayed occurrence sync from closing its clean origin later',async t=>{
 const editor=await setup(t,'occurrences-sync-held');const doc=await editor.open('Main.fs'),position=editor.position(doc,2,'value');editor.focus(doc,position);
 const possible=editor.executeCommand('fsharpNavigator.showPossibleReferences');await editor.waitForOutput('OVERLAY_HELD');
 const definition=editor.providers.definition.provideDefinition(doc,position,editor.uncancelled);await possible;
 await fs.writeFile(path.join(path.dirname(doc.uri.fsPath),'release-overlay'),'go');assert.equal((await definition).length,1);await settle(30);
 assert.ok(!editor.output.join('').includes('OVERLAY_METHOD_document/close'),'Cancelled search must not close the origin after sync resumes');assert.ok(!editor.output.join('').includes('POSSIBLE_'));
});
