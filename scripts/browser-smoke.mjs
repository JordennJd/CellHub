// Run against a dedicated headless Chrome and the Vite development server.
import assert from "node:assert/strict";
import { verifyTouchControls } from "./browser-touch.mjs";
import { writeFile, mkdir } from "node:fs/promises";
await mkdir("work", { recursive: true });
const cdp = process.env.CELLHUB_CDP_URL ?? "http://127.0.0.1:9224";
const url = process.env.CELLHUB_URL ?? "http://127.0.0.1:5175";
const tabs = await (await fetch(`${cdp}/json/list`)).json();
const ws = new WebSocket(tabs.find(t => t.type === "page").webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let sequence = 0;
const pending = new Map(), errors = [];
ws.onmessage = ({data}) => {
  const message = JSON.parse(data);
  if (message.method === "Runtime.exceptionThrown") errors.push(message.params.exceptionDetails);
  if (message.id) {
    const request = pending.get(message.id); pending.delete(message.id);
    message.error ? request.reject(message.error) : request.resolve(message.result);
  }
};
const call = (method, params = {}) => new Promise((resolve, reject) => {
  const id = ++sequence; pending.set(id, {resolve, reject});
  ws.send(JSON.stringify({id, method, params}));
});
const evaluate = async expression => {
  const result = await call("Runtime.evaluate", {expression, returnByValue:true, awaitPromise:true});
  if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
};
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const until = async expression => {
  for (let i=0; i<200; i++) {
    if (await evaluate(`Boolean(${expression})`)) return;
    const alert = await evaluate("document.querySelector('[role=alert]')?.textContent");
    if (alert) throw Error(alert);
    await delay(150);
  }
  throw Error(`Timeout: ${expression}`);
};
const click = async text => {
  assert(await evaluate(`(() => {const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`), `Missing button: ${text}`);
  await delay(80);
};
const flags = () => evaluate("[...document.querySelectorAll('.division-settings input[type=checkbox], .editor-content > .toggle input')].map(x=>x.checked)");
const screenshot = async (path, clip) => {
  const image = await call("Page.captureScreenshot", {format:"png", ...(clip ? {clip} : {})});
  await writeFile(path, Buffer.from(image.data,"base64"));
};
const clickWorld = async (x,y,button="left") => {
  const point = await evaluate(`(() => {const r=document.querySelector('.world canvas').getBoundingClientRect();const z=Math.min(r.width,r.height)/3072*0.93;return {x:r.x+r.width/2+(${x}-1536)*z,y:r.y+r.height/2+(${y}-1536)*z};})()`);
  await call("Input.dispatchMouseEvent", {type:"mousePressed",button,clickCount:1,...point});
  await call("Input.dispatchMouseEvent", {type:"mouseReleased",button,clickCount:1,...point});
};
try {
  await call("Runtime.enable"); await call("Page.enable");
  await call("Emulation.setDeviceMetricsOverride", {width:1440,height:1080,deviceScaleFactor:1,mobile:false});
  await call("Page.addScriptToEvaluateOnNewDocument", {source:`
    // Read the rendered GPU texture as well as checking the DOM. Headless
    // compositors on Linux can omit WebGPU canvases from page screenshots.
    if (window.GPUCanvasContext) {
      const contexts=new Map();
      const configure=GPUCanvasContext.prototype.configure;
      const texture=GPUCanvasContext.prototype.getCurrentTexture;
      const submit=GPUQueue.prototype.submit;
      GPUCanvasContext.prototype.configure=function(config){
        contexts.set(this,{device:config.device,format:config.format,canvas:this.canvas});
        return configure.call(this,{...config,usage:(config.usage??16)|1});
      };
      GPUCanvasContext.prototype.getCurrentTexture=function(){
        const value=texture.call(this);const state=contexts.get(this);if(state)state.texture=value;return value;
      };
      GPUQueue.prototype.submit=function(commands){
        submit.call(this,commands);
        for(const state of contexts.values()) {
          if(state.device.queue!==this || !state.texture || state.canvas.id!==window.__captureGPU)continue;
          window.__captureGPU=null;
          const {device,canvas}=state, width=canvas.width,height=canvas.height;
          const stride=Math.ceil(width*4/256)*256;
          const buffer=device.createBuffer({size:stride*height,usage:9});
          const encoder=device.createCommandEncoder();
          encoder.copyTextureToBuffer({texture:state.texture},{buffer,bytesPerRow:stride,rowsPerImage:height},{width,height});
          submit.call(this,[encoder.finish()]);
          window.__gpuReadback=buffer.mapAsync(1).then(()=>{
            const mapped=new Uint8Array(buffer.getMappedRange()),pixels=new Uint8ClampedArray(width*height*4);
            for(let y=0;y<height;y++)pixels.set(mapped.subarray(y*stride,y*stride+width*4),y*width*4);
            if(state.format.startsWith('bgra'))for(let i=0;i<pixels.length;i+=4){const r=pixels[i];pixels[i]=pixels[i+2];pixels[i+2]=r;}
            buffer.unmap();buffer.destroy();
            const image=document.createElement('canvas');image.width=width;image.height=height;
            image.getContext('2d').putImageData(new ImageData(pixels,width,height),0,0);
            const offset=(Math.floor(height/2)*width+Math.floor(width/2))*4;
            return {pixel:Array.from(pixels.slice(offset,offset+4)),png:image.toDataURL('image/png').split(',')[1]};
          });
        }
      };
    }
    window.__cellhubFrames=0;
    window.__spawnCommands=[];
    const NativeWorker=window.Worker;
    window.Worker=class extends NativeWorker {
      postMessage(message, ...args) {
        if(message.type==='spawn') window.__spawnCommands.push({time:performance.now(),x:message.x,y:message.y});
        return super.postMessage(message,...args);
      }
      constructor(...args){super(...args);this.addEventListener('message',({data})=>{
        if(data.type==='frame'){window.__cellhubFrame=data;window.__cellhubFrames++;}
      });}
    };
  `});
  await call("Page.navigate", {url:`${url}/?paused`});
  await until("!!document.querySelector('.world canvas[data-renderer]') && window.__cellhubFrame");
  const hasWebGPU = await evaluate("document.querySelector('.world canvas').dataset.renderer==='webgpu'");
  assert.equal(await evaluate("document.querySelectorAll('.renderer-toggle button').length"),2);
  assert.equal(await evaluate("document.querySelector('.renderer-toggle [aria-pressed=true]').dataset.backend"),hasWebGPU?"webgpu":"webgl");
  assert.equal(await evaluate("document.querySelector('[data-backend=webgpu]').disabled"),!hasWebGPU);
  console.log("PASS: real Rust renderer initialized (WebGPU or forced WebGL fallback)");
  assert.equal(await evaluate("document.querySelector('.app').dataset.controls"),"mouse");
  assert.equal(await evaluate("document.querySelector('.map-toolbar')"),null);
  assert(await evaluate("!/(WASM|RUST|LAB|SEED|experimental|Большие возможности|КОНСТРУКТОР ЖИЗНИ)/.test(document.body.innerText)"));
  assert(await evaluate("!document.querySelector('.world canvas').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true}))"));
  const center = await evaluate("(() => {const r=document.querySelector('.world canvas').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,z:Math.min(r.width,r.height)/3072*0.93};})()");
  await call("Input.dispatchMouseEvent",{type:"mousePressed",button:"left",clickCount:1,x:center.x,y:center.y});
  await call("Input.dispatchMouseEvent",{type:"mouseMoved",buttons:1,x:center.x+80,y:center.y});
  await call("Input.dispatchMouseEvent",{type:"mouseReleased",button:"left",clickCount:1,x:center.x+80,y:center.y});
  await until("window.__cellhubFrame.cells.length===10");
  assert(Math.abs((await evaluate("window.__cellhubFrame.cells[1]"))-(1536+80/center.z))<1);
  await click("↺ Очистить"); await until("window.__cellhubFrame.cells.length===0");
  // Hold spawning starts at one second and follows the cursor at 200 ms intervals.
  await evaluate("window.__spawnCommands=[];window.__holdStart=performance.now()");
  await call("Input.dispatchMouseEvent",{type:"mousePressed",button:"left",clickCount:1,x:center.x-60,y:center.y});
  await delay(700);
  await call("Input.dispatchMouseEvent",{type:"mouseMoved",buttons:1,x:center.x-30,y:center.y});
  assert.equal(await evaluate("window.__spawnCommands.length"),0);
  await delay(380);
  await call("Input.dispatchMouseEvent",{type:"mouseMoved",buttons:1,x:center.x+60,y:center.y});
  await delay(450);
  await call("Input.dispatchMouseEvent",{type:"mouseReleased",button:"left",clickCount:1,x:center.x+60,y:center.y});
  const spawned=await evaluate("window.__spawnCommands");
  assert(spawned.length>=3 && spawned.length<=4);
  assert(spawned[0].time-(await evaluate("window.__holdStart"))>=950);
  assert(Math.abs(spawned[0].x-(1536-30/center.z))<1);
  assert(Math.abs(spawned.at(-1).x-(1536+60/center.z))<1);
  for(let i=1;i<spawned.length;i++) assert(spawned[i].time-spawned[i-1].time>=150 && spawned[i].time-spawned[i-1].time<500);
  await delay(350);
  assert.equal(await evaluate("window.__spawnCommands.length"),spawned.length,"release must stop repetition without an extra cell");
  // Losing window focus stops an active hold; a fresh click must still work.
  await call("Input.dispatchMouseEvent",{type:"mousePressed",button:"left",clickCount:1,x:center.x,y:center.y});
  await delay(1100);
  await evaluate("window.dispatchEvent(new Event('blur'))");
  const afterBlur=await evaluate("window.__spawnCommands.length");
  await delay(350);
  assert.equal(await evaluate("window.__spawnCommands.length"),afterBlur);
  await call("Input.dispatchMouseEvent",{type:"mouseReleased",button:"left",clickCount:1,x:center.x,y:center.y});
  await clickWorld(1536,1536);
  await until(`window.__cellhubFrame.cells.length===${(afterBlur+1)*10}`);
  await click("↺ Очистить"); await until("window.__cellhubFrame.cells.length===0");
  console.log("PASS: moving clicks, 1-second hold, 200ms cursor-following spawn, release and focus-loss cancellation");

  await call("Input.dispatchMouseEvent",{type:"mousePressed",button:"middle",clickCount:1,x:center.x,y:center.y});
  await call("Input.dispatchMouseEvent",{type:"mouseMoved",buttons:4,x:center.x+80,y:center.y});
  await call("Input.dispatchMouseEvent",{type:"mouseReleased",button:"middle",clickCount:1,x:center.x+80,y:center.y});
  await clickWorld(1536,1536);
  await until("window.__cellhubFrame.cells.length===10");
  assert(Math.abs((await evaluate("window.__cellhubFrame.cells[1]"))-(1536-80/center.z))<1);
  await clickWorld(1536,1536,"right");
  await until("!!document.querySelector('.apply')");
  assert.equal(await evaluate("window.__cellhubFrame.cells.length"),10);
  await click("↺ Очистить"); await until("window.__cellhubFrame.cells.length===0");
  await evaluate("document.querySelector('[title=\"Показать всю карту\"]').click()");
  const zoomBefore=await evaluate("document.querySelector('.map-scale').textContent");
  await call("Input.dispatchMouseEvent",{type:"mouseWheel",x:center.x,y:center.y,deltaX:0,deltaY:-300});
  await delay(150);
  assert.notEqual(await evaluate("document.querySelector('.map-scale').textContent"),zoomBefore);
  assert.equal(await evaluate("window.scrollY"),0);
  await evaluate("document.querySelector('[title=\"Показать всю карту\"]').click()");
  console.log("PASS: desktop buttons hidden, left-place/right-select/middle-pan, wheel zoom, canvas defaults blocked");

  assert.equal(await evaluate("document.querySelectorAll('.modes button').length"),1);
  assert.equal(await evaluate("document.querySelector('.slot-name input').value"),"M1");
  assert.equal(await evaluate("document.querySelector('.cell-type-control select').value"),"0");
  assert.equal(await evaluate("document.querySelector('#genome-examples')"),null);
  assert.equal(await evaluate("document.querySelectorAll('.slider-control').length"),5);
  assert.equal(await evaluate("document.querySelectorAll('.split').length"),0);
  await evaluate("document.querySelector('.mass-slider').scrollIntoView({block:'center'})");
  const massPoint=await evaluate("(() => {const r=document.querySelector('.mass-slider').getBoundingClientRect();return {x:r.x+r.width*0.7,y:r.y+r.height/2};})()");
  await call("Input.dispatchMouseEvent", {type:"mousePressed",button:"left",clickCount:1,...massPoint});
  await call("Input.dispatchMouseEvent", {type:"mouseReleased",button:"left",clickCount:1,...massPoint});
  const massValue=Number(await evaluate("document.querySelector('.mass-slider input').value"));
  assert(Math.abs(massValue-0.7)<0.02);
  await call("Input.dispatchKeyEvent", {type:"keyDown",key:"ArrowLeft",code:"ArrowLeft",windowsVirtualKeyCode:37});
  await call("Input.dispatchKeyEvent", {type:"keyUp",key:"ArrowLeft",code:"ArrowLeft",windowsVirtualKeyCode:37});
  await delay(80);
  assert(Math.abs(Number(await evaluate("document.querySelector('.mass-slider input').value"))-(massValue-0.01))<0.001);
  await screenshot("work/cellhub-sliders.png");
  await click("↺");
  await evaluate("document.querySelector('.editor-content').scrollTop=0");

  await evaluate(`(() => {const input=document.querySelector('.slot-name input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'Лист');input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await delay(80);
  await evaluate("document.querySelector('.slot-name input').dispatchEvent(new FocusEvent('focusout',{bubbles:true}))");
  await delay(80);
  assert.equal(await evaluate("document.querySelector('.modes button').textContent.trim()"),"Лист");
  await click("↓ Сохранить"); await click("↺"); await click("↑ Загрузить");
  assert.equal(await evaluate("document.querySelector('.slot-name input').value"),"Лист");
  await clickWorld(600,900);
  await until("window.__cellhubFrame.cells.length===10");
  const beforeExample=await evaluate("Array.from(window.__cellhubFrame.cells)");
  await evaluate("document.querySelector('[role=switch]').click()"); await delay(80);
  await click("Пример: все типы клеток");
  assert.equal(await evaluate("document.querySelectorAll('.modes button').length"),6);
  assert.deepEqual(await evaluate("Array.from(window.__cellhubFrame.cells)"),beforeExample);
  for(let i=6;i<42;i++) await click("＋ Слот");
  assert.equal(await evaluate("document.querySelectorAll('.modes button').length"),42);
  await clickWorld(1200,900);
  await until("window.__cellhubFrame.cells.length===20");
  await click("↺ Очистить");
  await until("window.__cellhubFrame.cells.length===0");
  console.log("PASS: single photocyte, rename and save/load, 42 slots, examples preserve canvas, unified sliders");
  await click("Пример: многоклеточный организм");
  assert.equal(await evaluate("document.querySelectorAll('.modes button').length"),3);
  assert.deepEqual(await flags(),[true,false,false,true]);
  await click("M2"); assert.deepEqual(await flags(),[false,false,false,false]);
  assert(await evaluate("document.querySelector('.division-settings').disabled"));
  await click("M3"); assert.deepEqual(await flags(),[true,true,false,false]);
  // Lower only the division threshold to observe the complete cycle quickly.
  await evaluate(`(() => {const input=document.querySelector('.division-settings input[type=range]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'1');input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await click("Копия"); assert.equal(await evaluate("document.querySelectorAll('.modes button').length"),4);
  await click("Удалить");
  await click("↓ Сохранить"); await click("↺"); await click("↑ Загрузить");
  await click("M3"); assert.deepEqual(await flags(),[true,true,false,false]);
  assert.equal(await evaluate("document.querySelector('.division-settings input[type=range]').value"),"1");
  await click("M1");
  // (600,600) is under the map toolbar; place M1 lower on the same light side.
  await clickWorld(600,900);
  await click("4×");
  await until("window.__cellhubFrame.cells.length===30 && window.__cellhubFrame.links.length===2");
  await click("Ⅱ"); await delay(150);
  const cycle = await evaluate(`(() => {const f=window.__cellhubFrame;const rows=Array.from({length:f.cells.length/10},(_,i)=>Array.from(f.cells.slice(i*10,(i+1)*10)));return {rows,links:Array.from(f.links)};})()`);
  const photo=cycle.rows.find(r=>r[4]===0), flag=cycle.rows.find(r=>r[4]===2), phago=cycle.rows.find(r=>r[4]===1);
  assert(photo && flag && phago);
  assert.deepEqual(cycle.links.slice().sort((a,b)=>a-b),[flag[0],phago[0]].sort((a,b)=>a-b));
  assert(!cycle.links.includes(photo[0]));
  const pausedFrames=await evaluate("window.__cellhubFrames");
  await delay(500); assert.equal(await evaluate("window.__cellhubFrames"),pausedFrames);
  const tick=await evaluate("window.__cellhubFrame.stats[0]");
  await evaluate("document.querySelector('button[title=\"Один шаг симуляции\"]').click()");
  await until(`window.__cellhubFrame.stats[0]===${tick+1}`);
  await clickWorld(flag[1],flag[2],"right");
  await until("document.querySelector('.mode-badge')?.textContent==='M2'");
  assert.equal((await flags())[0],false);
  await click("Наблюдение");
  assert(await evaluate("document.querySelector('.metric-grid').textContent.includes('Связей1')"));
  await click("Геном");
  console.log("PASS: M1/M2/M3 cycle, detached offspring, selection, save/load, pause and step");
  await screenshot("work/cellhub-integration-ui.png");
  if (hasWebGPU) {
  await evaluate("document.querySelector('.world canvas').id='main-scene';window.__captureGPU='main-scene';window.__gpuReadback=null");
  const zoomPoint=await evaluate("(() => {const r=document.querySelector('.world canvas').getBoundingClientRect();const z=Math.min(r.width,r.height)/3072*0.93;return {x:r.x+r.width/2+(600-1536)*z,y:r.y+r.height/2+(600-1536)*z};})()");
  await call("Input.dispatchMouseEvent",{type:"mouseWheel",...zoomPoint,deltaX:0,deltaY:-2400});
  await until("window.__gpuReadback!==null");
  const worldImage=await evaluate("window.__gpuReadback");
  await writeFile("work/cellhub-integration-world.png",Buffer.from(worldImage.png,"base64"));
  // Exercise real GPU bond geometry independently of cell-body occlusion.
  await evaluate(`(async()=>{
    const mod=await import('/frontend/rust-wasm/cellhub_frontend.js');await mod.default();
    const canvas=document.createElement('canvas');canvas.id='gpu-fixture';
    canvas.style.cssText='position:fixed;left:0;top:0;width:200px;height:200px;z-index:10000';document.body.appendChild(canvas);
    const renderer=await mod.WorldRenderer.create(canvas,"auto");
    const cells=new Float32Array([2,60,100,10,0,1,0,0,0,0,9,140,100,10,2,1,0,0,0,0]);
    const terrain=new Float32Array(48*48*4);
    renderer.set_view(100,100,1,0,-1);renderer.set_frame(cells,terrain,new Uint32Array([2,9]));window.__captureGPU='gpu-fixture';renderer.render();
    window.__gpuFixture={renderer,cells,terrain,canvas};
  })()`);
  await delay(250);
  const gpuImage = async path => {
    const result=await evaluate("window.__gpuReadback");
    assert(result?.png, "GPU readback was not captured");
    await writeFile(path,Buffer.from(result.png,"base64"));return result.pixel;
  };
  const normal=await gpuImage("work/cellhub-bond-normal.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_view(100,100,1,0,9);__gpuFixture.renderer.render()");
  await delay(100);
  const selected=await gpuImage("work/cellhub-bond-selected.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array());__gpuFixture.renderer.render()");
  await delay(100);
  const removed=await gpuImage("work/cellhub-bond-removed.png");
  assert.deepEqual(removed,[37,62,43,255]);
  assert(normal.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-removed[i]),0)>100);
  assert(selected.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-normal[i]),0)>30);
  console.log(JSON.stringify({gpuPixels:{normal,selected,removed}}));
  assert(await evaluate(`(() => {try {__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array([2]));return false;}catch(e){return String(e).includes('bond snapshot');}})()`));
  await evaluate("__gpuFixture.renderer.destroy();__gpuFixture.renderer.free();__gpuFixture.canvas.remove();delete window.__gpuFixture");
  console.log("PASS: GPU bond fixtures and invalid-frame rejection");
  } else { console.log("SKIP: GPU readback fixtures; this Chrome uses WebGL fallback"); }
  await call("Page.navigate",{url:`${url}/?count=20000&paused`});
  await until("window.__cellhubFrame?.cells.length===200000 && !!document.querySelector('.world canvas[data-renderer]')");
  assert(await evaluate("Array.from(window.__cellhubFrame.cells).every(Number.isFinite)"));
  assert.equal(await evaluate("window.__cellhubFrame.stats[1]"),20000);
  await evaluate("document.querySelector('button[title=\"Один шаг симуляции\"]').click()");
  await until("window.__cellhubFrame.stats[0]===1");
  assert(await evaluate("Array.from(window.__cellhubFrame.cells).every(Number.isFinite)"));
  await click("↺ Очистить"); await until("window.__cellhubFrame.cells.length===0 && window.__cellhubFrame.links.length===0");
  await call("Emulation.setDeviceMetricsOverride",{width:390,height:844,deviceScaleFactor:1,mobile:false});
  await delay(250);
  assert(await evaluate("document.documentElement.scrollWidth <= window.innerWidth"));
  assert.deepEqual(errors,[]);
  console.log("PASS: 20000-cell snapshot and simulation step, reset, narrow layout; no runtime errors");
  await call("Emulation.setDeviceMetricsOverride",{width:1440,height:1080,deviceScaleFactor:1,mobile:false});
  await verifyTouchControls({ call, evaluate, until, delay, screenshot, url });
  // Some headless GPU setups expose WebGPU but disable WebGL. In that case
  // the app correctly reports an error, so skip instead of failing the whole
  // run; validate WebGL2 in a Chrome started with `--enable-unsafe-swiftshader
  // --use-angle=swiftshader`.
  if (await evaluate("!!document.createElement('canvas').getContext('webgl2')")) {
    await call("Page.navigate",{url:`${url}/?paused`});
    await until("!!document.querySelector('.world canvas[data-renderer]') && window.__cellhubFrame");
    if (hasWebGPU) {
      assert.equal(await evaluate("document.querySelector('.renderer-toggle [aria-pressed=true]').dataset.backend"),"webgpu");
      assert(!(await evaluate("document.querySelector('[data-backend=webgpu]').disabled")));
      await evaluate("document.querySelector('[data-backend=webgl]').click()");
      await until("document.querySelector('.world canvas')?.dataset.renderer==='webgl' && document.querySelector('.renderer-toggle [aria-pressed=true]')?.dataset.backend==='webgl' && window.__cellhubFrame");
      console.log("PASS: manual RendererToggle switches a live scene to WebGL");
    } else {
      assert.equal(await evaluate("document.querySelector('.renderer-toggle [aria-pressed=true]').dataset.backend"),"webgl");
      assert(await evaluate("document.querySelector('[data-backend=webgpu]').disabled"));
      console.log("PASS: failed WebGPU is blocked in the UI and WebGL is forced");
    }
    await evaluate("document.querySelector('[role=switch]').click()"); await delay(80);
    await click("Пример: многоклеточный организм");
    await clickWorld(600,900);await click("4×");
    await until("window.__cellhubFrame?.links.length>0");
    await click("Ⅱ");
    await screenshot("work/cellhub-integration-webgl.png");
    console.log("PASS: Rust WebGL backend retains multicellular organisms");
  } else {
    console.log("SKIP: WebGL2 unavailable in this Chrome; tested separately");
  }
  assert.deepEqual(errors,[]);
} catch (e) {
  console.error(JSON.stringify({errors,body:await evaluate("document.body.innerText")}));
  await screenshot("work/cellhub-integration-failure.png");
  throw e;
} finally { ws.close(); }
