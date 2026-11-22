'use strict';
const assert = require('node:assert/strict');
exports.exercise = async ({vscode,renderer,api,test,c,wait,current,noWrongNavigation}) => {
  const visibleHover=()=>renderer.evaluate(`(()=>{const visible=n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);return r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden';};return [...document.querySelectorAll('.monaco-hover')].filter(visible).map(n=>({text:n.innerText,role:n.getAttribute('role')}));})()`);
  await renderer.key('Escape');
  await wait(async()=>!(await visibleHover()).length,'previous hover absent');
  await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
  assert.deepEqual(current(),c.origin);
  const before=api.lastResult;
  c.hoverTrigger='Supported editor.action.showHover UI command; never executeHoverProvider';
  try {
    await vscode.commands.executeCommand('editor.action.showHover');
    c.nativeResult=await wait(()=>api.lastResult&&api.lastResult!==before&&api.lastResult,'fresh editor-hover result');
    const doc=vscode.window.activeTextEditor.document;
    if(c.nativeResult.version!==undefined)assert.equal(c.nativeResult.version,doc.version);
    if(test.expected.kind==='external'){
      assert.equal(c.nativeResult.completeness,'exact');assert.equal(c.nativeResult.ownership,'external');assert.equal(c.nativeResult.hover?.isExternal,true);
      assert.match(test.expected.name,/^[A-Za-z_][A-Za-z0-9_]*$/);const namePattern=new RegExp('\\b'+test.expected.name+'\\b');
      c.hoverUi=await wait(async()=>{const rows=await visibleHover();return rows.some(r=>namePattern.test(r.text)&&test.expected.contains.every(text=>r.text.includes(text)))&&rows;},'rendered authored external signature and documentation');
      c.classification='External documentation rendered from explicitly supplied validated prepared assets';
    }else{
      assert.ok(['suppressed','unavailable'].includes(test.expected.kind));
      if(test.expected.kind==='suppressed') {assert.equal(c.nativeResult.completeness,'exact');assert.equal(c.nativeResult.ownership,'workspace');c.classification='Fresh proven workspace ownership; hover intentionally suppressed';}
      else {assert.equal(test.expected.completeness,'unavailable');assert.equal(test.expected.ownership,'unknown');assert.equal(c.nativeResult.completeness,test.expected.completeness);assert.equal(c.nativeResult.ownership,test.expected.ownership);c.classification='Fresh unsupported ownership result; no claim of proven absence';}
      assert.ok(!c.nativeResult.hover);c.hoverUi=await visibleHover();assert.equal(c.hoverUi.length,0,'No speculative or retained documentation widget');
    }
    assert.deepEqual(current(),c.origin);noWrongNavigation(c,[c.origin]);
  } finally { c.finalHoverUi=await visibleHover();await renderer.key('Escape'); }
};
