'use strict';
// Test-only loopback CDP input in the explicitly supplied disposable Code process.
const assert = require('node:assert/strict');
exports.connect = async function (port) {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = targets.find(t => t.type === 'page' && /workbench/.test(t.url));
  assert.ok(page?.webSocketDebuggerUrl, 'Disposable workbench CDP target required');
  const socket = new WebSocket(page.webSocketDebuggerUrl), pending = new Map(); let id = 0;
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
  socket.addEventListener('message', e => { const m = JSON.parse(e.data), p = pending.get(m.id); if (!p) return; pending.delete(m.id); clearTimeout(p.timer); m.error ? p.reject(new Error(JSON.stringify(m.error))) : p.resolve(m.result); });
  const send = (method, params) => new Promise((resolve, reject) => { const key = ++id, timer = setTimeout(() => { pending.delete(key); reject(new Error('CDP timeout')); }, 10000); pending.set(key, { resolve, reject, timer }); socket.send(JSON.stringify({ id: key, method, params })); });
  const evaluate = async expression => { const r = await send('Runtime.evaluate', { expression, returnByValue: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text); return r.result.value; };
  return {
    evaluate,
    async beginReferenceWitness() { return evaluate(`(()=>{
      window.__fsnavReferenceWitness?.stop();
      const ids=new WeakMap();let next=0,dropped=0;const events=[];
      const pane=n=>n?.closest?.('.pane'), relevant=p=>/^(?:possible )?references\\b/i.test((p?.querySelector('.pane-header')?.innerText||'').trim());
      const describe=n=>{if(!n)return null;if(!ids.has(n))ids.set(n,++next);const r=n.getBoundingClientRect();return{nodeId:ids.get(n),connected:n.isConnected,text:(n.innerText||'').slice(0,2048),attrs:Object.fromEntries([...n.attributes].map(a=>[a.name,a.value.slice(0,2048)])),rect:{x:r.x,y:r.y,width:r.width,height:r.height}};};
      const rows=()=>[...document.querySelectorAll('.pane')].filter(relevant).map(p=>({title:p.querySelector('.pane-header')?.innerText,rowsTruncated:p.querySelectorAll('.monaco-list-row').length>100,rows:[...p.querySelectorAll('.monaco-list-row')].slice(0,100).map(describe)}));
      const record=e=>{if(events.length<256)events.push({at:performance.now(),...e});else dropped++;};
      const observer=new MutationObserver(ms=>{const selected=ms.filter(m=>relevant(pane(m.target))||[...m.addedNodes,...m.removedNodes].some(n=>n.nodeType===1&&(relevant(pane(n))||[...n.querySelectorAll?.('.pane')||[]].some(relevant))));if(selected.length)record({kind:'mutation',mutationsTruncated:selected.length>20,mutations:selected.slice(0,20).map(m=>({type:m.type,attribute:m.attributeName,addedTruncated:m.addedNodes.length>10,removedTruncated:m.removedNodes.length>10,target:describe(m.target.nodeType===1?m.target:m.target.parentElement),added:[...m.addedNodes].filter(n=>n.nodeType===1).slice(0,10).map(describe),removed:[...m.removedNodes].filter(n=>n.nodeType===1).slice(0,10).map(describe)})),rows:rows()});});
      observer.observe(document.body,{subtree:true,childList:true,attributes:true,characterData:true});
      const pointer=e=>{const row=e.target.closest?.('.monaco-list-row');record({kind:'pointer',relevant:relevant(pane(e.target)),type:e.type,trusted:e.isTrusted,x:e.clientX,y:e.clientY,targetTag:e.target.tagName,targetClass:e.target.className,path:e.composedPath().slice(0,8).map(n=>({tag:n.tagName,id:n.id,className:typeof n.className==='string'?n.className:null})),row:describe(row),hitRow:describe(document.elementFromPoint(e.clientX,e.clientY)?.closest('.monaco-list-row'))});};
      for(const type of ['mousedown','mouseup','click'])document.addEventListener(type,pointer,true);
      window.__fsnavReferenceWitness={describe,record,rows,read:()=>({events,dropped,rows:rows(),boundary:'DOM node identity is not provider generation; capped read-only observer, renderer clock only'}),stop:()=>{observer.disconnect();for(const type of ['mousedown','mouseup','click'])document.removeEventListener(type,pointer,true);}};
      record({kind:'armed',rows:rows()});return true;
    })()`); },
    async beginPossibleStability(config) { return evaluate(`(()=>{
      window.__fsnavPossibleStability?.stop();const config=${JSON.stringify(config)},w=window.__fsnavReferenceWitness;let stopped=false,raf,previous=null,count=0,dropped=0;const frames=[];
      const inspect=()=>{const pane=[...document.querySelectorAll('.pane')].find(p=>/possible references/i.test(p.querySelector('.pane-header')?.innerText||''));if(!pane)return null;
        const rows=[...pane.querySelectorAll('.monaco-list-row')].map(n=>{const d=w.describe(n),r=n.getBoundingClientRect(),point={x:r.x+Math.min(90,r.width/2),y:r.y+r.height/2},h=document.elementFromPoint(point.x,point.y);return{witness:d,text:n.innerText,point,hit:r.width>0&&r.height>0&&!!h&&(h===n||n.contains(h))};});
        const matches=(r,t)=>r.text.includes(t.path)&&r.text.includes(t.label+t.path);const target=rows.find(r=>matches(r,config.target));let animations=[];for(let n=pane;n;n=n.parentElement)animations.push(...n.getAnimations());animations.push(...pane.getAnimations({subtree:true}));const active=animations.some(a=>a.pending||a.playState==='running');
        const ready=/incomplete/i.test(pane.innerText)&&/unrelated|same.name/i.test(pane.innerText)&&config.required.every(t=>rows.some(r=>r.hit&&matches(r,t)))&&(!config.exactCount||rows.length===config.required.length)&&target?.hit&&target.witness.connected&&!active;
        return{ready,activeAnimations:active,target,signature:ready?JSON.stringify(target.witness):null};};
      const tick=at=>{if(stopped)return;const s=inspect();count=s?.ready?(s.signature===previous?count+1:1):0;previous=s?.signature||null;const frame={at,count,...s};if(frames.length<360)frames.push(frame);else dropped++;raf=requestAnimationFrame(tick);};raf=requestAnimationFrame(tick);
      window.__fsnavPossibleStability={read:()=>{const live=inspect();return{frames,dropped,count,live,qualified:count>=3&&live?.ready&&live.signature===previous};},stop:()=>{stopped=true;cancelAnimationFrame(raf);}};return true;
    })()`); },
    async readPossibleStability() { return evaluate('window.__fsnavPossibleStability?.read()'); },
    async stopPossibleStability() { return evaluate('window.__fsnavPossibleStability?.stop()'); },
    async markReferenceWitness(kind, intended) { return evaluate(`(()=>{const w=window.__fsnavReferenceWitness;if(!w)return;const intended=${JSON.stringify(intended||null)},p=intended?.point;w.record({kind:${JSON.stringify(kind)},intended,hitRow:p?w.describe(document.elementFromPoint(p.x,p.y)?.closest('.monaco-list-row')):null,rows:w.rows()});})()`); },
    async endReferenceWitness() { return evaluate(`(()=>{const w=window.__fsnavReferenceWitness;if(!w)return null;w.record({kind:'end',rows:w.rows()});const r=w.read();w.stop();return r;})()`); },
    async key(key, modifiers = 0) { const code = key === 'F12' ? 123 : key === 'Escape' ? 27 : undefined; assert.ok(code); for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key, code: key, windowsVirtualKeyCode: code, modifiers }); },
    async scroll(point, deltaY) { await send('Input.dispatchMouseEvent',{type:'mouseWheel',...point,deltaX:0,deltaY}); },
    async clickPoint(point) { assert.ok(point&&Number.isFinite(point.x)&&Number.isFinite(point.y));for(const type of ['mousePressed','mouseReleased'])await send('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1}); },
    async clickStatus(text) {
      const point = await evaluate(`(()=>{const n=[...document.querySelectorAll('.statusbar-item')].find(n=>(n.textContent||'').includes(${JSON.stringify(text)}));if(!n)return null;const r=n.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2,h=document.elementFromPoint(x,y);return r.width&&r.height&&h&&(h===n||n.contains(h))?{x,y}:null;})()`);
      if (!point) return false;
      for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, ...point, button: 'left', clickCount: 1 }); return true;
    },
    async clickChoice(label, fullPath) {
      const point=await evaluate(`(()=>{for(const n of document.querySelectorAll('.quick-input-widget .monaco-list-row')){const text=n.textContent||'';if(!text.includes(${JSON.stringify(label)})||!text.includes(${JSON.stringify(fullPath)}))continue;const r=n.getBoundingClientRect(),x=r.x+Math.min(80,r.width/2),y=r.y+r.height/2,h=document.elementFromPoint(x,y);if(r.width&&r.height&&h&&(h===n||n.contains(h)))return{x,y};}return null;})()`);
      if(!point)return false;for(const type of ['mousePressed','mouseReleased'])await send('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1});return true;
    },
    dispose() { for (const p of pending.values()) { clearTimeout(p.timer); p.reject(new Error('Renderer disposed')); } pending.clear(); socket.close(); },
  };
};
