// Run against a dedicated headless Chrome and the Vite development server.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
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
    if (await evaluate(expression)) return;
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
const flags = () => evaluate("[...document.querySelectorAll('input[type=checkbox]')].map(x=>x.checked)");
const screenshot = async (path, clip) => {
  const image = await call("Page.captureScreenshot", {format:"png", ...(clip ? {clip} : {})});
  await writeFile(path, Buffer.from(image.data,"base64"));
};
const clickWorld = async (x,y) => {
  const point = await evaluate(`(() => {const r=document.querySelector('.world canvas').getBoundingClientRect();const z=Math.min(r.width,r.height)/3072*0.93;return {x:r.x+r.width/2+(${x}-1536)*z,y:r.y+r.height/2+(${y}-1536)*z};})()`);
  await call("Input.dispatchMouseEvent", {type:"mousePressed",button:"left",clickCount:1,...point});
  await call("Input.dispatchMouseEvent", {type:"mouseReleased",button:"left",clickCount:1,...point});
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
    const NativeWorker=window.Worker;
    window.Worker=class extends NativeWorker {
      constructor(...args){super(...args);this.addEventListener('message',({data})=>{
        if(data.type==='frame'){window.__cellhubFrame=data;window.__cellhubFrames++;}
      });}
    };
  `});
  await call("Page.navigate", {url:`${url}/?paused`});
  await until("document.querySelector('.engine')?.textContent.includes('RUST WEBGPU') && window.__cellhubFrame");
  console.log("PASS: real Rust/WebGPU renderer initialized");
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
  await clickWorld(600,600);
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
  await click("◎Выбрать").catch(async()=>{await evaluate("document.querySelector('button[title=\"Выбрать\"]').click()");});
  await clickWorld(flag[1],flag[2]);
  await until("document.querySelector('.mode-badge')?.textContent==='M2'");
  assert.equal((await flags())[0],false);
  await click("Наблюдение");
  assert(await evaluate("document.querySelector('.metric-grid').textContent.includes('Связей1')"));
  await click("Геном");
  console.log("PASS: M1/M2/M3 cycle, detached offspring, selection, save/load, pause and step");
  await screenshot("/tmp/cellhub-integration-ui.png");
  await evaluate("document.querySelector('.world canvas').id='main-scene';window.__captureGPU='main-scene';window.__gpuReadback=null");
  const zoomPoint=await evaluate("(() => {const r=document.querySelector('.world canvas').getBoundingClientRect();const z=Math.min(r.width,r.height)/3072*0.93;return {x:r.x+r.width/2+(600-1536)*z,y:r.y+r.height/2+(600-1536)*z};})()");
  await call("Input.dispatchMouseEvent",{type:"mouseWheel",...zoomPoint,deltaX:0,deltaY:-2400});
  await until("window.__gpuReadback!==null");
  const worldImage=await evaluate("window.__gpuReadback");
  await writeFile("/tmp/cellhub-integration-world.png",Buffer.from(worldImage.png,"base64"));
  // Exercise real GPU bond geometry independently of cell-body occlusion.
  await evaluate(`(async()=>{
    const mod=await import('/frontend/rust-wasm/cellhub_frontend.js');await mod.default();
    const canvas=document.createElement('canvas');canvas.id='gpu-fixture';
    canvas.style.cssText='position:fixed;left:0;top:0;width:200px;height:200px;z-index:10000';document.body.appendChild(canvas);
    const renderer=await mod.WorldRenderer.create(canvas);
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
  const normal=await gpuImage("/tmp/cellhub-bond-normal.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_view(100,100,1,0,9);__gpuFixture.renderer.render()");
  await delay(100);
  const selected=await gpuImage("/tmp/cellhub-bond-selected.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array());__gpuFixture.renderer.render()");
  await delay(100);
  const removed=await gpuImage("/tmp/cellhub-bond-removed.png");
  assert.deepEqual(removed,[37,62,43,255]);
  assert(normal.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-removed[i]),0)>100);
  assert(selected.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-normal[i]),0)>30);
  console.log(JSON.stringify({gpuPixels:{normal,selected,removed}}));
  assert(await evaluate(`(() => {try {__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array([2]));return false;}catch(e){return String(e).includes('bond snapshot');}})()`));
  await evaluate("__gpuFixture.renderer.destroy();__gpuFixture.renderer.free();__gpuFixture.canvas.remove();delete window.__gpuFixture");
  console.log("PASS: GPU bond fixtures and invalid-frame rejection");
  await call("Page.navigate",{url:`${url}/?count=20000&paused`});
  await until("window.__cellhubFrame?.cells.length===200000 && document.querySelector('.engine')?.textContent.includes('RUST WEBGPU')");
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
  await call("Page.navigate",{url:`${url}/?renderer=webgl&paused`});
  await until("document.querySelector('.engine')?.textContent.includes('FALLBACK') && document.querySelector('.world canvas')");
  await click("Пример: многоклеточный организм");
  await clickWorld(600,600);await click("4×");
  await until("window.__cellhubFrame?.links.length>0");
  await click("Ⅱ");
  await screenshot("/tmp/cellhub-integration-webgl.png");
  assert.deepEqual(errors,[]);
  console.log("PASS: WebGL fallback retains multicellular organisms");
} catch (e) {
  console.error(JSON.stringify({errors,body:await evaluate("document.body.innerText")}));
  await screenshot("/tmp/cellhub-integration-failure.png");
  throw e;
} finally { ws.close(); }
