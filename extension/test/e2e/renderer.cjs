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
