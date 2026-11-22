'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const hash = text => crypto.createHash('sha256').update(text).digest('hex');

// Real F12 and rendered selection around one supported unsaved dependency edit.
// No candidate clearing, command replay or fixed settling sleep inside the action.
exports.exercise = async ({vscode,renderer,api,test,c,roots,wait,current,norm,noWrongNavigation,state,target}) => {
  const edit = test.afterChoiceEdit;
  const dependency = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(roots[test.fixture],edit.file)));
  const original = dependency.getText(), disk = await fs.readFile(dependency.uri.fsPath,'utf8');
  assert.equal(original,disk); assert.equal(dependency.isDirty,false);
  const consumer = vscode.window.activeTextEditor.document;
  const consumerBefore = {uri:consumer.uri.toString(),version:consumer.version,hash:hash(consumer.getText()),selection:current()};
  const gold = x => target(test.fixture,x);
  const label = x => `${path.basename(x.file)}:${x.range.start.line+1}:${x.range.start.character+1}`;
  const locationKey = x => JSON.stringify(x);
  const pointers = async () => renderer.evaluate('window.__fsnavDefinitionPointers');
  await renderer.evaluate(`(()=>{window.__fsnavDefinitionPointers=[];window.__fsnavDefinitionPointer=e=>{const row=e.target.closest?.('.quick-input-widget .monaco-list-row'),status=e.target.closest?.('.statusbar-item');if((row||status)&&window.__fsnavDefinitionPointers.length<32)window.__fsnavDefinitionPointers.push({type:e.type,trusted:e.isTrusted,row:row?.textContent||null,status:status?.textContent||null});};for(const type of ['mousedown','mouseup','click'])document.addEventListener(type,window.__fsnavDefinitionPointer,true);})()`);
  async function f12(expected) {
    const previous=api.lastResult;
    await renderer.key('F12');
    await wait(()=>api.lastResult!==previous&&api.lastResult,'fresh keyboard definition result');
    const result=api.lastResult, collected=api.candidates.getAllCandidates().map(norm);
    assert.notEqual(result.completeness,'exact'); assert.deepEqual(result.locations,[]); assert.equal(result.candidateCompleteness,'incomplete');
    assert.deepEqual(collected.map(locationKey).sort(),expected.required.map(x=>locationKey(gold(x))).sort(),'Exact authored current physical candidate set');
    const offer=await wait(async()=>{const s=await state();return s.offer&&s;},'rendered current offer');
    assert.equal(offer.picker,null); assert.ok(offer.offer.includes(`${expected.required.length} possible definitions`));
    assert.deepEqual(current(),c.origin); noWrongNavigation(c,[c.origin]);
    assert.ok(await renderer.clickStatus(offer.offer));
    const picker=await wait(async()=>{const s=await state();return s.picker?.rows.length&&s.picker;},'rendered current picker');
    assert.match(picker.text,/incomplete/i);
    for(const x of expected.required)assert.ok(picker.rows.some(row=>row.hit&&/binding unverified/i.test(row.text)&&row.text.includes(label(x))&&row.text.includes(vscode.Uri.parse(gold(x).uri).fsPath)),'Every authored current target is visible and hit-testable');
    assert.deepEqual(current(),c.origin);noWrongNavigation(c,[c.origin]);
    return {result,collected,offer,picker};
  }
  try {
    c.initial=await f12(test.expected);c.nativeResult=c.initial.result;c.collected=c.initial.collected;
    c.beforeEditUi=await state();assert.ok(c.beforeEditUi.picker?.rows.length,'Picker still rendered at edit boundary');
    const version=dependency.version;
    const change=new vscode.WorkspaceEdit();change.replace(dependency.uri,new vscode.Range(new vscode.Position(0,0),dependency.positionAt(original.length)),edit.text);
    assert.ok(await vscode.workspace.applyEdit(change));assert.equal(dependency.getText(),edit.text);assert.equal(dependency.isDirty,true);assert.ok(dependency.version>version);
    assert.equal(await fs.readFile(dependency.uri.fsPath,'utf8'),disk);
    c.editWitness={method:'supported WorkspaceEdit on preopened dependency, not physical typing',beforeVersion:version,afterVersion:dependency.version,beforeHash:hash(original),afterHash:hash(edit.text),diskHash:hash(disk),dirty:dependency.isDirty};
    assert.deepEqual({uri:consumer.uri.toString(),version:consumer.version,hash:hash(consumer.getText()),selection:current()},consumerBefore,'Consumer and caret unchanged');
    c.immediateAfterEditUi=await state();
    const old=test.expected.target;
    const pointerStart=(await pointers()).length;
    c.oldAttempt={method:'fresh DOM lookup and hit-test for old label/path',sent:await renderer.clickChoice(label(old),vscode.Uri.parse(gold(old).uri).fsPath)};
    c.oldAttempt.pointers=(await pointers()).slice(pointerStart);
    assert.equal(c.oldAttempt.sent,false,'This scenario qualifies withdrawal, not retained-row activation');
    assert.deepEqual(c.oldAttempt.pointers,[]);
    c.dismissed=await wait(async()=>{const s=await state();return !s.offer&&!s.picker&&s;},'automatic withdrawal after dependency edit');
    assert.deepEqual(current(),c.origin);noWrongNavigation(c,[c.origin]);
    c.activationScope='Old picker/offer withdrawn automatically; no retained stale row received a click. Strict retained-row activation and in-flight cancellation remain unverified.';
    await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
    assert.deepEqual(current(),c.origin);
    c.fresh=await f12(edit.expected);
    const selected=edit.expected.target, expected=gold(selected),start=(await pointers()).length;
    assert.ok(await renderer.clickChoice(label(selected),vscode.Uri.parse(expected.uri).fsPath));
    await wait(()=>JSON.stringify(current())===JSON.stringify(expected),'fresh explicit selection URI/full range');
    c.selectionPointers=(await pointers()).slice(start);
    for(const type of ['mousedown','mouseup','click'])assert.ok(c.selectionPointers.some(e=>e.type===type&&e.trusted&&e.row?.includes(label(selected))&&e.row.includes(vscode.Uri.parse(expected.uri).fsPath)),'Trusted current-row pointer delivery: '+type);
    noWrongNavigation(c,[c.origin,expected]);
    c.keyboard=await renderer.evaluate('window.__fsnavE2EKeys');assert.equal(c.keyboard.filter(k=>k.key==='F12'&&!k.shiftKey&&k.inEditor&&k.trusted).length,2);
    c.selected=current();c.interactionSelections=[...c.selections];c.interactionEditors=[...c.editors];
    c.freshnessPassed=true;
  } finally {
    await renderer.evaluate(`(()=>{for(const type of ['mousedown','mouseup','click'])document.removeEventListener(type,window.__fsnavDefinitionPointer,true);})()`);
    // Cleanup follows the entire interaction and is not part of the safety oracle.
    try {
      await vscode.window.showTextDocument(dependency,{preview:false});await vscode.commands.executeCommand('workbench.action.files.revert');
      assert.equal(dependency.isDirty,false);assert.equal(dependency.getText(),original);assert.equal(await fs.readFile(dependency.uri.fsPath,'utf8'),disk);
      c.definitionCleanup={passed:true};
    } catch(error) {c.definitionCleanup={passed:false,error:error.stack};throw error;}
  }
};
