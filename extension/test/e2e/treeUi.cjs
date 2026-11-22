'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
exports.exercise = async ({vscode,renderer,api,test,c,roots,wait,current}) => {
  const expected=test.expected.tree;
  const root=roots[test.fixture],project=path.join(root,expected.project);
  // Read-only model supplies identity, never counts as rendered order or a click.
  const walk=items=>items.flatMap(n=>[n,...walk(n.children||[])]);
  const node=await wait(async()=>walk(await api.tree.getChildren()).find(n=>n.kind==='project'&&n.path===project),'authored project identity discovered');
  assert.ok(node,'Authored project exists in tree');c.projectIdentity={path:node.path,label:node.label,id:node.id};
  const snapshot=()=>renderer.evaluate(`(()=>{const pane=[...document.querySelectorAll('.pane')].find(p=>/^solution$/i.test((p.querySelector('.pane-header')?.innerText||'').trim()));if(!pane)return null;return{title:pane.querySelector('.pane-header').innerText,rows:[...pane.querySelectorAll('.monaco-list-row')].filter(n=>n.getBoundingClientRect().height>0).map(n=>{const r=n.getBoundingClientRect(),x=r.x+Math.min(90,r.width/2),y=r.y+r.height/2,h=document.elementFromPoint(x,y),tw=n.querySelector('.monaco-tl-twistie'),tr=tw?.getBoundingClientRect();return{text:n.innerText,label:n.querySelector('.label-name')?.textContent||n.innerText,level:Number(n.getAttribute('aria-level')),expanded:n.getAttribute('aria-expanded'),point:{x,y},hit:!!h&&(h===n||n.contains(h)),twistieHit:tr&&tr.width?!!document.elementFromPoint(tr.x+tr.width/2,tr.y+tr.height/2)&&tw.contains(document.elementFromPoint(tr.x+tr.width/2,tr.y+tr.height/2)):false,twistie:tr&&tr.width?{x:tr.x+tr.width/2,y:tr.y+tr.height/2}:null};})};})()`);
  const subtree=state=>{const i=state.rows.findIndex(r=>r.label===node.label);assert.ok(i>=0,'Unique authored project visible');assert.equal(state.rows.filter(r=>r.label===node.label).length,1,'Project label unambiguous in rendered tree');const base=state.rows[i].level;let end=i+1;while(end<state.rows.length&&state.rows[end].level>base)end++;return{index:i,base,rows:state.rows.slice(i+1,end)};};
  await vscode.commands.executeCommand('fsharpNavigator.solution.focus');
  const otherPane=await renderer.evaluate(`(()=>{const p=[...document.querySelectorAll('.pane')].find(p=>/possible references/i.test(p.querySelector('.pane-header')?.innerText||''));const h=p?.querySelector('.pane-header'),b=p?.querySelector('.pane-body');if(!h||!b||b.getBoundingClientRect().height===0)return null;const r=h.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2,hit=document.elementFromPoint(x,y);return hit&&(h===hit||h.contains(hit))?{x,y}:null;})()`);
  if(otherPane){await renderer.clickPoint(otherPane);c.collapsedOtherPane=true;await wait(async()=>renderer.evaluate(`(()=>{const p=[...document.querySelectorAll('.pane')].find(p=>/possible references/i.test(p.querySelector('.pane-header')?.innerText||''));return !p?.querySelector('.pane-body')?.getBoundingClientRect().height;})()`),'other pane collapsed by pointer');}
  c.viewTrigger='Supported contributed view focus, followed only by actual pointer expansion and file clicks';
  let state=await wait(async()=>{const s=await snapshot();return s?.rows.some(r=>r.label===node.label)&&s;},'authored project visible');
  let selected=state.rows[subtree(state).index];
  if(selected.expanded==='false'){assert.ok(selected.hit&&selected.twistie&&selected.twistieHit);await renderer.clickPoint(selected.twistie);}
  state=await wait(async()=>{const s=await snapshot();return s&&s.rows[subtree(s).index].expanded==='true'&&subtree(s).rows.length>0&&s;},'expanded project descendants rendered');
  const maxExpansions=expected.leaves.reduce((n,l)=>n+l.displayPath.length,1);
  for(let i=0;i<maxExpansions;i++){
    const branch=subtree(state),closed=branch.rows.find(r=>r.expanded==='false');if(!closed)break;
    assert.ok(closed.hit&&closed.twistie&&closed.twistieHit);const branchIndex=branch.rows.indexOf(closed);await renderer.clickPoint(closed.twistie);
    state=await wait(async()=>{const s=await snapshot();if(!s)return null;const rows=subtree(s).rows,opened=rows[branchIndex];return opened?.label===closed.label&&opened.expanded==='true'&&rows[branchIndex+1]?.level>opened.level&&s;},'nested folder expanded after pointer');
  }
  c.renderedTree=state;const branch=subtree(state);assert.ok(!branch.rows.some(r=>r.expanded==='false'),'All declared project folders expanded');
  const hierarchy=[],stack=[];
  for(const row of branch.rows){const depth=row.level-branch.base-1;stack.length=depth;if(row.expanded!==null)stack[depth]=row.label;else hierarchy.push({row,displayPath:[...stack,row.label]});}
  assert.deepEqual(hierarchy.map(x=>x.displayPath),expected.leaves.map(x=>x.displayPath),'Rendered compile order and nesting match authored nonalphabetic tree');
  c.treeClicks=[];
  for(let index=0;index<hierarchy.length;index++){
    const latest=await snapshot(),live=subtree(latest).rows.filter(r=>r.expanded===null),row=live[index];assert.ok(row?.hit);const before=current()?.uri,eventIndex=c.editors.length;
    const uri=vscode.Uri.file(path.join(root,expected.leaves[index].file)).toString();assert.notEqual(before,uri,'Tree click must change active physical file');await renderer.clickPoint(row.point);await wait(()=>current()?.uri===uri,'actual project-tree file click opens physical URI');for(const event of c.editors.slice(eventIndex))if(event.uri)assert.ok(event.uri===before||event.uri===uri,'No transient unrelated file while opening tree row');
    c.treeClicks.push({displayPath:hierarchy[index].displayPath,expectedUri:uri,actual:current(),row});
  }
  c.contextBoundary='Rendered project order/nesting and physical file URI clicks only; no evaluated defines or subsequent active-context binding claim.';
};
