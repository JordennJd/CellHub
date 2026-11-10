// Optional suite: dedicated Chrome profile + Vite. Creates uniquely named test saves.
import assert from 'node:assert/strict';
import {writeFile,mkdir} from 'node:fs/promises';
import {verifyTouchControls} from './browser-touch.mjs';
import {gpuCaptureSource,verifyRenderFixtures} from './browser-render-fixture.mjs';
await mkdir('work',{recursive:true});
const cdp=process.env.CELLHUB_CDP_URL??'http://127.0.0.1:9224';
const url=process.env.CELLHUB_URL??'http://127.0.0.1:5175';
const tabs=await(await fetch(`${cdp}/json/list`)).json();
const page=tabs.find(t=>t.type==='page');assert(page,'A dedicated Chrome page is required');
const ws=new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
let sequence=0;const pending=new Map(),errors=[];
ws.onmessage=({data})=>{const m=JSON.parse(data);if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails);if(m.method==='Page.javascriptDialogOpening')void call('Page.handleJavaScriptDialog',{accept:true});if(m.id){const r=pending.get(m.id);pending.delete(m.id);if(r)m.error?r.reject(m.error):r.resolve(m.result);}};
const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const until=async expression=>{for(let i=0;i<200;i++){if(await evaluate(`Boolean(${expression})`))return;const error=await evaluate("document.querySelector('.toast.error,.error-message[role=alert]')?.textContent");if(error)throw Error(error);await delay(150);}throw Error(`Timeout: ${expression}`);};
const click=async(text,scope='document')=>{assert(await evaluate(`(()=>{const b=[...${scope}.querySelectorAll('button')].find(b=>b.textContent.trim()===${JSON.stringify(text)});if(!b||b.disabled)return false;b.click();return true;})()`),`Missing enabled button: ${text}`);await delay(80);};
const input=async(selector,value)=>{await evaluate(`(()=>{const el=document.querySelector(${JSON.stringify(selector)});if(!el)throw Error('Missing input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,${JSON.stringify(String(value))});el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));})()`);await delay(100);};
const screenshot=async path=>{const r=await call('Page.captureScreenshot',{format:'png'});await writeFile(path,Buffer.from(r.data,'base64'));};
const openSandbox=async()=>{await until("document.querySelector('.home-shell')");await evaluate("document.querySelector('#mode-sandbox-title').closest('button').click()");await until("window.__cellhubFrame&&document.querySelector('.world canvas[data-renderer]')&&![...document.querySelectorAll('.transport button')].find(b=>b.textContent==='Продолжить')?.disabled");};
const clickWorld=async(x,y,button='left')=>{const point=await evaluate(`(()=>{const r=document.querySelector('.world canvas').getBoundingClientRect();const z=Math.min(r.width,r.height)/3072*.93;return{x:r.x+r.width/2+(${x}-1536)*z,y:r.y+r.height/2+(${y}-1536)*z};})()`);await call('Input.dispatchMouseEvent',{type:'mousePressed',button,clickCount:1,...point});await call('Input.dispatchMouseEvent',{type:'mouseReleased',button,clickCount:1,...point});};
const confirm=async()=>{if(await evaluate("!!document.querySelector('[role=dialog]')"))await click('Подтвердить',"document.querySelector('[role=dialog]')");};
try{
 await call('Runtime.enable');await call('Page.enable');
 await call('Emulation.setDeviceMetricsOverride',{width:1440,height:1080,deviceScaleFactor:1,mobile:false});
 await call('Page.addScriptToEvaluateOnNewDocument',{source:gpuCaptureSource+`
 window.__cellhubFrames=0;const NativeWorker=window.Worker;
 window.Worker=class extends NativeWorker{
 postMessage(message,...args){if(message.type==='reset'||message.type==='restore'){window.__mainWorker=this;window.__cellhubFrame=null;}return super.postMessage(message,...args);}
 constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(this===window.__mainWorker&&data.type==='frame'){window.__cellhubFrame=data;window.__cellhubFrames++;}});}
 };`});
 await call('Page.navigate',{url});await until("document.querySelector('.home-shell')");
 assert.deepEqual(await evaluate("[...document.querySelectorAll('.home-mode-card')].map(button=>({title:button.querySelector('.home-mode-title').textContent,disabled:button.disabled}))"),[
  {title:'Испытания',disabled:false},{title:'Песочница',disabled:false},{title:'Хаос',disabled:true},{title:'Арена',disabled:false}
 ]);
 assert.equal(await evaluate("document.querySelectorAll('.future-section').length"),0);
 await screenshot('work/cellhub-home.png');await openSandbox();
 const hasWebGPU=await evaluate("document.querySelector('.world canvas').dataset.renderer==='webgpu'");
 assert(await evaluate("!/(WASM|RUST|WebGPU|WebGL|SEED|experimental|Большие возможности|Маленькая жизнь)/.test(document.body.innerText)"));
 assert.equal(await evaluate('window.__cellhubFrame.stats[0]'),0);
 await clickWorld(600,600);await until('window.__cellhubFrame.cells.length===12');
 // Inspection must leave an independently edited draft intact.
 await input('input[aria-label="Масса для деления"]',3.7);
 await click('Диагностика');await clickWorld(600,600);
 await until("document.querySelector('.cell-details')?.textContent.includes('Масса')");
 await click('Геном',"document.querySelector('.tabs')");
 assert.equal(await evaluate("document.querySelector('input[aria-label=\"Масса для деления\"]').value"),'3.7');
 const frozen=await evaluate('({cells:Array.from(window.__cellhubFrame.cells),stats:Array.from(window.__cellhubFrame.stats)})');
 await evaluate("document.querySelector('.genome-preview-disclosure').open=true");
 await input('input[aria-label="Возраст предпросмотра"]',40);
 await until("document.querySelector('.genome-preview').getAttribute('aria-busy')==='false'");
 assert.deepEqual(await evaluate('({cells:Array.from(window.__cellhubFrame.cells),stats:Array.from(window.__cellhubFrame.stats)})'),frozen);
 await click('Один шаг');await until('window.__cellhubFrame.stats[0]===1');
 console.log('PASS: Home entry, paused placement, draft-safe inspection, isolated preview and single step');
 const suffix=Date.now(),genomeName=`Browser genome ${suffix}`,plateName=`Browser plate ${suffix}`;
 await click('Сохранить геном');await input('[role=dialog] input',genomeName);await click('Сохранить',"document.querySelector('[role=dialog]')");await until("!document.querySelector('[role=dialog]')");
 await click('Среда',"document.querySelector('.tabs')");await input('.panel-section input[type="range"]',2);
 await until('Math.abs(window.__cellhubFrame.terrain[0]-1.9)<.001');
 const saved=await evaluate('({cells:Array.from(window.__cellhubFrame.cells),stats:Array.from(window.__cellhubFrame.stats),terrain:Array.from(window.__cellhubFrame.terrain)})');
 await click('Сохранить эксперимент');await input('[role=dialog] input',plateName);await click('Сохранить',"document.querySelector('[role=dialog]')");await until("!document.querySelector('[role=dialog]')");
 await click('Начать заново');await confirm();await until('window.__cellhubFrame?.stats[0]===0&&window.__cellhubFrame.cells.length===0');
 assert(Math.abs(await evaluate('window.__cellhubFrame.terrain[0]')-1.9)<.001,'reset preserves chosen environment');
 await click('← Меню');await confirm();await until("document.querySelector('.home-shell')");
 await click('Открыть эксперимент');await until("document.querySelector('.library-list li')");
 await evaluate(`(()=>{const item=[...document.querySelectorAll('.library-list li')].find(el=>el.querySelector('h3')?.textContent===${JSON.stringify(plateName)});if(!item)throw Error('Missing saved plate');item.querySelector('button').click();})()`);
 await until("document.querySelector('.laboratory')&&window.__cellhubFrame?.cells.length===12");
 assert.deepEqual(await evaluate('({cells:Array.from(window.__cellhubFrame.cells),stats:Array.from(window.__cellhubFrame.stats),terrain:Array.from(window.__cellhubFrame.terrain)})'),saved);
 await screenshot('work/cellhub-reopened-experiment.png');
 console.log('PASS: named saves, reset semantics, Home library reopen and exact world restoration');
 if(hasWebGPU)await verifyRenderFixtures({evaluate,delay});else console.log('SKIP: GPU pixel fixtures; browser uses WebGL fallback');
 // Engine stress is isolated from the UI; no undocumented ?count entry route.
 await evaluate(`(async()=>{const m=await import('/frontend/wasm/simulation.js');await m.default();const s=new m.Simulation(42);try{const g=new Float32Array([0,8,0,0,.5,0,0,1000,1,0,0,0]);for(let i=0;i<20000;i++)s.spawn(20+(i%142)*21,20+Math.floor(i/142)*21,0,g);s.advance(1);if(s.snapshot().length!==240000||!Array.from(s.snapshot()).every(Number.isFinite))throw Error('Invalid 20000-cell fixture');}finally{s.free();}})()`);
 await click('← Меню');await confirm();
 await verifyTouchControls({call,evaluate,until,delay,screenshot,url,openSandbox,click});
 if(await evaluate("!!document.createElement('canvas').getContext('webgl2')")){
 await call('Page.navigate',{url:`${url}/?renderer=webgl`});await openSandbox();await until("document.querySelector('.world canvas')?.dataset.renderer==='webgl'");await clickWorld(600,600);await until('window.__cellhubFrame.cells.length===12');await screenshot('work/cellhub-webgl-laboratory.png');console.log('PASS: WebGL fallback through Home and laboratory');}
 assert.deepEqual(errors,[]);
}catch(error){console.error(JSON.stringify({errors,body:await evaluate('document.body.innerText')}));await screenshot('work/cellhub-integration-failure.png');throw error;}finally{ws.close();}
