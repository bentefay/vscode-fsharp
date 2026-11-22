'use strict';
// Actual built-in References UI. Native payloads are secondary range witnesses.
const assert = require('node:assert/strict');
const path = require('node:path');
const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
const sorted = values => values.map(x=>JSON.stringify(x)).sort();
exports.exercise = async ({vscode,renderer,api,test,c,roots,wait,current,norm,noWrongNavigation,preserveNativeRows=false}) => {
  const target=t=>({uri:vscode.Uri.file(path.join(roots[test.fixture],t.file)).toString(),range:t.range});
  const dom=()=>renderer.evaluate(`(()=>{const visible=n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);return r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden';};const panes=[...document.querySelectorAll('.pane')].filter(p=>visible(p)&&/^references\\b/i.test((p.querySelector('.pane-header')?.innerText||'').trim()));const rows=panes.flatMap(p=>[...p.querySelectorAll('.monaco-list-row')].filter(visible).map(n=>{const r=n.getBoundingClientRect(),x=r.x+Math.min(90,r.width/2),y=r.y+r.height/2,h=document.elementFromPoint(x,y);return{text:n.innerText,aria:n.getAttribute('aria-label'),level:n.getAttribute('aria-level'),expanded:n.getAttribute('aria-expanded'),highlights:[...n.querySelectorAll('.highlight')].map(x=>x.textContent),point:{x,y},hit:!!h&&(h===n||n.contains(h))};}));const messages=[...document.querySelectorAll('.monaco-editor-overlaymessage,.peekview-widget')].filter(visible).map(n=>n.innerText);const notices=[...document.querySelectorAll('.notification-list-item')].filter(visible).map(n=>({text:n.innerText,buttons:[...n.querySelectorAll('button,.monaco-button')].filter(visible).map(b=>{const r=b.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2,h=document.elementFromPoint(x,y);return{text:b.innerText.trim(),point:{x,y},hit:!!h&&(h===b||b.contains(h))};})}));const possible=[...document.querySelectorAll('.pane')].find(p=>visible(p)&&/possible references/i.test(p.querySelector('.pane-header')?.innerText||''));return{panes:panes.map(p=>p.innerText),rows,messages,notices,possible:possible?{text:possible.innerText,rows:[...possible.querySelectorAll('.monaco-list-row')].filter(visible).map(n=>{const r=n.getBoundingClientRect(),x=r.x+Math.min(90,r.width/2),y=r.y+r.height/2,h=document.elementFromPoint(x,y);return{text:n.innerText,point:{x,y},hit:!!h&&(h===n||n.contains(h))};})}:null};})()`);
  try {
  if(!preserveNativeRows){await vscode.commands.executeCommand('references-view.clearHistory');
  await vscode.commands.executeCommand('references-view.clear');
  await wait(async()=>!(await dom()).rows.some(r=>r.expanded===null),'previous native reference rows cleared');}
  await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
  assert.deepEqual(current(),c.origin);
  const before=api.lastResult;c.route=test.route;c.referenceNativeSnapshots=[];
  await renderer.key('F12',test.route==='sidebar'?9:8);
  c.keyboard=await renderer.evaluate('window.__fsnavE2EKeys');
  assert.ok(c.keyboard.some(k=>k.key==='F12'&&k.shiftKey&&k.altKey===(test.route==='sidebar')&&k.trusted&&k.inEditor),'Actual reference shortcut reached editor');
  c.provenanceLimit='Fresh lastResult is observed without patching the public provider or relay method logging. Exact authored sets and actual UI/clicks are independent evidence; includeDeclaration/token route is source-derived.';
  c.routeSemantics='Pinned built-in sidebar Shift+Alt+F12 requests declaration-inclusive references. Shift+F12 compact mode may internally retry without declarations. Flags are source-derived, not observed provider-token callbacks.';
  let previous=before;
  const fresh=()=>{const r=api.lastResult;if(r&&r!==previous){c.referenceNativeSnapshots.push(r);previous=r;}return r&&r!==before?r:null;};
  if(test.expected.kind==='exact'){
    const authored=test.expected.references.slice().sort((a,b)=>a.file.localeCompare(b.file)||a.range.start.line-b.range.start.line||a.range.start.character-b.range.start.character);const expected=authored.map(target);
    c.nativeResult=await wait(()=>{const r=fresh();return r?.completeness==='exact'&&same(sorted((r.locations||[]).map(norm)),sorted(expected))&&r;},'fresh exact authored reference set');
    const document=vscode.workspace.textDocuments.find(d=>d.uri.toString()===c.origin.uri);assert.ok(document);if(c.nativeResult.documentVersion!==undefined)assert.equal(c.nativeResult.documentVersion,document.version);
    const allowed=[c.origin,...expected.map(t=>({uri:t.uri,range:{start:t.range.start,end:t.range.start}}))];
    if(test.route==='sidebar'){
      c.referenceUi=await wait(async()=>{const s=await dom(),leaves=s.rows.filter(r=>r.expanded===null);return leaves.length===expected.length&&leaves.every(r=>r.hit)&&s;},'rendered complete native reference set');
      const leaves=c.referenceUi.rows.filter(r=>r.expanded===null);
      assert.equal(leaves.length,expected.length);c.referenceClicks=[];
      for(let index=0;index<leaves.length;index++){
        const s=await dom(),rows=s.rows.filter(r=>r.expanded===null);assert.equal(rows.length,expected.length);assert.ok(rows[index].hit);assert.notDeepEqual(current(),{uri:expected[index].uri,range:{start:expected[index].range.start,end:expected[index].range.start}},'Actual click must cause an observable caret transition');await renderer.clickPoint(rows[index].point);
        const selected=await wait(()=>{const sel=current(),t=expected[index];return same(sel,{uri:t.uri,range:{start:t.range.start,end:t.range.start}})&&sel;},'reference row opens authored URI/caret');
        assert.deepEqual(rows[index].highlights,[authored[index].text],'Rendered reference highlight equals authored literal');const reconstructed={uri:selected.uri,range:{start:selected.range.start,end:{line:selected.range.start.line,character:selected.range.start.character+rows[index].highlights[0].length}}};assert.deepEqual(reconstructed,expected[index],'UI caret plus UTF-16 highlight reconstructs full single-line range');
        c.referenceClicks.push({row:rows[index],selection:selected,reconstructedRange:reconstructed});noWrongNavigation(c,allowed);
      }
      assert.deepEqual(sorted(c.referenceClicks.map(x=>x.selection)),sorted(allowed.slice(1)),'Each authored reference row is independently reachable');
    }else{
      assert.equal(expected.length,1,'This bounded compact fixture expects one selected reference');
      const jump=allowed[1];await wait(()=>same(current(),jump),'actual compact reference jump');c.referenceUi=await dom();assert.ok(![...c.referenceUi.panes,...c.referenceUi.messages].some(x=>/no references found/i.test(x)));noWrongNavigation(c,allowed);
    }
    const finalUi=await dom();assert.ok(!finalUi.notices.some(n=>/Scope was not fully searched|references incomplete/i.test(n.text)),'Exact references must not warn incomplete');c.exactFinalUi=finalUi;c.declarationIncluded=test.declarationIncluded;c.knownUsageCount=test.oracle.knownUsageCount;
    c.uiRangeBoundary='Sidebar full single-line range reconstructed from actual clicked URI/start caret plus rendered highlighted literal UTF-16 length; native full ranges secondary. Compact Shift+F12 only verifies actual URI/caret, with full range secondary because no reference row is rendered.';
  }else{
    assert.equal(test.expected.kind,'incomplete');c.nativeResult=await wait(()=>fresh(),'fresh incomplete reference result');const document=vscode.workspace.textDocuments.find(d=>d.uri.toString()===c.origin.uri);assert.ok(document);assert.equal(c.nativeResult.version,document.version);assert.notEqual(c.nativeResult.completeness,'exact');c.internalPartialLocations=(c.nativeResult.locations||[]).map(norm);c.internalResultBoundary='lastResult records the raw internal response; incomplete adapter returns no native reference locations. UI leaves are verified separately.';
    c.referenceUi=await wait(async()=>{const s=await dom();return s.notices.some(n=>n.text.includes('Scope was not fully searched.')&&n.text.includes('“No references found” is not evidence that there are no uses.'))&&[...s.panes,...s.messages].some(t=>/no results|no references found/i.test(t))&&s;},'truthful incomplete references warning');
    c.nativeEmptyWording=[...c.referenceUi.panes,...c.referenceUi.messages].filter(t=>/no results|no references found/i.test(t));c.nativeUiLimitation=c.nativeEmptyWording.length>0;assert.equal(c.referenceUi.rows.filter(r=>r.expanded===null).length,0,'Incomplete native reference view must not publish reference rows after explicit history reset');
    assert.deepEqual(current(),c.origin);noWrongNavigation(c,[c.origin]);
    if(test.showPossible){
      const previousPossible=api.lastPossibleReferences;
      const button=await wait(async()=>{const s=await dom(),n=s.notices.find(n=>n.text.includes('Scope was not fully searched.'));if(!n)return null;assert.deepEqual(n.buttons.map(b=>b.text),['Show Possible References']);return n.buttons[0].hit&&n.buttons[0];},'actual warning action hit target');
      c.possibleAction=button;await renderer.clickPoint(button.point);
      await wait(()=>api.lastPossibleReferences&&api.lastPossibleReferences!==previousPossible,'fresh deliberate possible-reference result');c.possibleResult=api.lastPossibleReferences;assert.equal(c.possibleResult.occurrenceIdentity.origin.uri,c.origin.uri);assert.equal(c.possibleResult.occurrenceIdentity.origin.sourceHash.length,64);assert.equal(c.possibleResult.candidateKind,'same-name-occurrences');assert.equal(c.possibleResult.candidateCompleteness,'incomplete');assert.notEqual(c.possibleResult.completeness,'exact');
      const locations=(c.possibleResult.candidates||[]).map(norm),required=test.expected.possibleRequired.map(target);for(const t of required)assert.ok(locations.some(x=>same(x,t)));
      if(test.expected.possibleSet==='exact')assert.deepEqual(sorted(locations),sorted(required));
      c.possibleUi=await wait(async()=>{const s=await dom(),p=s.possible;if(!p)return null;return /incomplete/i.test(p.text)&&/unrelated|same.name/i.test(p.text)&&required.every(t=>p.rows.some(r=>r.hit&&r.text.includes(vscode.Uri.parse(t.uri).fsPath)&&r.text.includes(`${path.basename(vscode.Uri.parse(t.uri).fsPath)}:${t.range.start.line+1}:${t.range.start.character+1}`)))&&p;},'labelled possible-reference rows actually rendered');
      if(test.expected.possibleSet==='exact')assert.equal(c.possibleUi.rows.length,required.length,'Rendered possible occurrence set has no extra rows');assert.match(c.possibleUi.text,/incomplete/i);assert.match(c.possibleUi.text,/unrelated|same.name/i);assert.deepEqual(current(),c.origin);noWrongNavigation(c,[c.origin]);
      if(test.expected.possibleTarget){const gold=target(test.expected.possibleTarget),label=`${path.basename(test.expected.possibleTarget.file)}:${gold.range.start.line+1}:${gold.range.start.character+1}`,row=c.possibleUi.rows.find(r=>r.hit&&r.text.includes(label)&&r.text.includes(vscode.Uri.parse(gold.uri).fsPath));assert.ok(row);await renderer.clickPoint(row.point);await wait(()=>same(current(),gold),'explicit possible-reference current full-range selection');noWrongNavigation(c,[c.origin,gold]);}
    }
  }
  } finally { c.referenceFinalUi=await dom();c.finalReferenceKeyboard=await renderer.evaluate('window.__fsnavE2EKeys'); }
};
