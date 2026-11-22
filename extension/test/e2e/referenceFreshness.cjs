'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {exercise}=require('./referenceUi.cjs');
const hash=text=>crypto.createHash('sha256').update(text).digest('hex'),same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
exports.exercise=async context=>{
 const {vscode,renderer,api,test,c,roots,wait,current,norm,noWrongNavigation}=context,plan=test.interaction,root=roots[test.fixture];c.interactionPhases=[];c.setupWitnesses=[];
 const slice=(selectionStart,editorStart)=>({selections:c.selections.slice(selectionStart),editors:c.editors.slice(editorStart)});
 const nativeSnapshot=()=>renderer.evaluate(`(()=>{const p=[...document.querySelectorAll('.pane')].find(p=>/^references\\b/i.test((p.querySelector('.pane-header')?.innerText||'').trim()));return p?{text:p.innerText,rows:[...p.querySelectorAll('.monaco-list-row')].filter(n=>n.getBoundingClientRect().height>0).map(n=>({text:n.innerText,expanded:n.getAttribute('aria-expanded'),highlights:[...n.querySelectorAll('.highlight')].map(x=>x.textContent)}))}:null;})()`);
 const possibleSnapshot=()=>renderer.evaluate(`(()=>{const p=[...document.querySelectorAll('.pane')].find(p=>/possible references/i.test(p.querySelector('.pane-header')?.innerText||''));return p?{text:p.innerText,rows:[...p.querySelectorAll('.monaco-list-row')].filter(n=>n.getBoundingClientRect().height>0).map(n=>n.innerText)}:null;})()`);
 const focus=async query=>{
  const before=current(),ss=c.selections.length,es=c.editors.length,doc=await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(root,query.file))),editor=await vscode.window.showTextDocument(doc,{preview:false}),q=new vscode.Position(query.line,query.character);
  editor.selection=new vscode.Selection(q,q);await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');const expected={uri:doc.uri.toString(),range:{start:{line:q.line,character:q.character},end:{line:q.line,character:q.character}}};await wait(()=>same(current(),expected),'authored interaction query caret');const events=slice(ss,es);noWrongNavigation(events,[before,expected]);c.setupWitnesses.push({action:'supported focus and authored query selection',before,after:current(),...events});return doc;
 };
 const phase=async(scenario,name,preserveNativeRows)=>{
  await focus(scenario.query);await renderer.evaluate('window.__fsnavE2EKeys=[]');const ss=c.selections.length,es=c.editors.length;let se,ee;const record={name,origin:current(),selections:[],editors:[]};Object.defineProperties(record,{selections:{enumerable:true,get:()=>c.selections.slice(ss,se)},editors:{enumerable:true,get:()=>c.editors.slice(es,ee)}});c.interactionPhases.push(record);
  try{await exercise({...context,test:scenario,c:record,preserveNativeRows});assert.equal(record.keyboard.filter(k=>k.key==='F12'&&k.shiftKey&&k.altKey&&k.trusted&&k.inEditor).length,1,'Each phase requires one fresh actual sidebar shortcut');record.passed=true;c.nativeResult=record.nativeResult;return record;}finally{se=c.selections.length;ee=c.editors.length;}
 };
 let edited,original;
 try{
  const baseline=await phase(test,'baseline',false);let captured;
  if(plan.kind==='possible-post-edit-command'){
   const target=test.expected.possibleTarget,gold={uri:vscode.Uri.file(path.join(root,target.file)).toString(),range:target.range};const row=api.possibleReferences.getChildren().find(row=>same(norm(row.location),gold));assert.ok(row,'Capture command from current shown authored row');const item=api.possibleReferences.getTreeItem(row);assert.equal(item.command.command,'fsharpNavigator.openPossibleReference');captured={command:item.command.command,arguments:[...item.command.arguments]};c.capturedCommand={...captured,location:gold,boundary:'Real shown row command captured through provider; later replay is contract evidence, not a stale pointer click'};
  }
  const uri=vscode.Uri.file(path.join(root,plan.edit.file));edited=await vscode.workspace.openTextDocument(uri);original=edited.getText();assert.equal(edited.isDirty,false);assert.equal(await fs.readFile(uri.fsPath,'utf8'),original);const before=current(),version=edited.version,ss=c.selections.length,es=c.editors.length;
  assert.equal(before.uri,uri.toString(),'Edit occurs in currently selected reference target');assert.equal(plan.edit.prefix.split('\n').length-1,1);const change=new vscode.WorkspaceEdit();change.insert(uri,new vscode.Position(0,0),plan.edit.prefix);assert.ok(await vscode.workspace.applyEdit(change));const shifted={uri:before.uri,range:{start:{line:before.range.start.line+1,character:before.range.start.character},end:{line:before.range.end.line+1,character:before.range.end.character}}};await wait(()=>same(current(),shifted),'exact automatic caret shift from prefix insertion');assert.equal(edited.getText(),plan.edit.prefix+original);assert.ok(edited.version>version&&edited.isDirty);assert.equal(await fs.readFile(uri.fsPath,'utf8'),original);noWrongNavigation(slice(ss,es),[before,shifted]);c.editWitness={method:'supported unsaved WorkspaceEdit insertion, not physical typing',file:plan.edit.file,beforeVersion:version,afterVersion:edited.version,beforeHash:hash(original),afterHash:hash(edited.getText()),diskHash:hash(original),beforeSelection:before,afterSelection:current(),...slice(ss,es)};
  if(plan.kind==='native-post-edit-repeat'){
   c.nativeUiAfterEditBeforeRequery=await nativeSnapshot();c.nativeReplacementBoundary='Native References rows are not cleared by this harness between edit and either repeat; only post-requery currentness is asserted, not automatic stale-row removal.';
  }else{
   assert.equal(plan.kind,'possible-post-edit-command');c.customRowsRemoved=await wait(async()=>{const s=await possibleSnapshot();return s&&s.rows.length===0&&s;},'edited custom occurrence rows removed');assert.equal(api.possibleReferences.getChildren().length,0);
   const origin=current(),ss=c.selections.length,es=c.editors.length;await vscode.commands.executeCommand(captured.command,...captured.arguments);assert.deepEqual(current(),origin);noWrongNavigation(slice(ss,es),[origin]);c.staleCommand={boundary:'Supported captured command replay after rendered rows removed; no stale-row pointer activation claimed',before:origin,after:current(),...slice(ss,es)};assert.equal((await possibleSnapshot()).rows.length,0);
  }
  const after={...test,...plan.after,operation:'references'};
  for(let i=0;i<plan.repeats;i++){
   if(plan.kind==='possible-post-edit-command')await vscode.commands.executeCommand('notifications.clearAll');
   const result=await phase(after,`after-edit-${i+1}`,true);
   const fresh=plan.kind==='native-post-edit-repeat'?result.referenceClicks.map(x=>x.reconstructedRange):(result.possibleResult.candidates||[]).map(norm);
   const old=plan.kind==='native-post-edit-repeat'?test.expected.references:[test.expected.possibleTarget];for(const t of old){const forbidden={uri:vscode.Uri.file(path.join(root,t.file)).toString(),range:t.range};assert.ok(!fresh.some(x=>same(x,forbidden)),'Old authored occurrence coordinates absent after requery');}
   assert.equal(edited.getText(),plan.edit.prefix+original);assert.ok(edited.isDirty);
  }
  c.freshnessPassed=true;
 }finally{
  if(edited){try{await vscode.window.showTextDocument(edited,{preview:false});await vscode.commands.executeCommand('workbench.action.files.revert');assert.equal(edited.isDirty,false);assert.equal(edited.getText(),original);assert.equal(await fs.readFile(edited.uri.fsPath,'utf8'),original);c.referenceCleanup={passed:true};}catch(error){c.referenceCleanup={passed:false,error:error.stack};throw error;}}
 }
};
