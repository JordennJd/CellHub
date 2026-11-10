import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
export const gpuCaptureSource = `    // Read the rendered GPU texture as well as checking the DOM. Headless
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
            const sample=(x,y)=>{
              const col=Math.floor(width/2+x*width/canvas.clientWidth);
              const row=Math.floor(height/2+y*height/canvas.clientHeight);
              const offset=(row*width+col)*4;
              return Array.from(pixels.slice(offset,offset+4));
            };
            const surface=[[-65,-65],[-65,0],[-65,65],[0,-65],[0,65],[65,-65],[65,0],[65,65]];
            const body=[[-8,-8],[-8,8],[8,-8],[8,8],[-12,0],[0,12]];
            return {pixel:sample(0,0),surface:surface.map(([x,y])=>sample(x,y)),body:body.map(([x,y])=>sample(x,y)),png:image.toDataURL('image/png').split(',')[1]};
          });
        }
      };
    }
`;

export async function verifyRenderFixtures({evaluate, delay}) {
  // Exercise real GPU bond geometry independently of cell-body occlusion.
  // Keep the fixture at the dish center; square-world corners are now masked.
  await evaluate(`(async()=>{
    const mod=await import('/frontend/rust-wasm/cellhub_frontend.js');await mod.default();
    const canvas=document.createElement('canvas');canvas.id='gpu-fixture';
    canvas.style.cssText='position:fixed;left:0;top:0;width:200px;height:200px;z-index:10000';document.body.appendChild(canvas);
    const renderer=await mod.WorldRenderer.create(canvas,"auto");
    const cells=new Float32Array([2,1496,1536,10,0,1,0,0,0,0,-1,0,9,1576,1536,10,2,1,0,0,0,0,-1,0]);
    const terrain=new Float32Array(48*48*4);
    renderer.set_view(1536,1536,1,0,-1);renderer.set_frame(cells,terrain,new Uint32Array([2,9]));window.__captureGPU='gpu-fixture';renderer.render();
    window.__gpuFixture={renderer,cells,terrain,canvas};
  })()`);
  await delay(250);
  const gpuImage = async path => {
    const result=await evaluate("window.__gpuReadback");
    assert(result?.png, "GPU readback was not captured");
    if(path) await writeFile(path,Buffer.from(result.png,"base64"));
    return result;
  };
  const {pixel:normal}=await gpuImage("work/cellhub-bond-normal.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_view(1536,1536,1,0,9);__gpuFixture.renderer.render()");
  await delay(100);
  const {pixel:selected}=await gpuImage("work/cellhub-bond-selected.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array());__gpuFixture.renderer.render()");
  await delay(100);
  const {pixel:removed,surface}=await gpuImage("work/cellhub-bond-removed.png");
  assert(removed[1]>removed[0]&&removed[1]>removed[2]&&removed[3]===255,"removing the bond reveals the dark green dish background");
  const green=surface.map(pixel=>pixel[1]);
  assert(Math.max(...green)-Math.min(...green)>8,"the substrate has visible mottling");
  assert.equal(normal[3],255,"the bond remains opaque over the dish");
  assert.equal(selected[3],255,"selection remains opaque over the dish");
  assert(normal.slice(0,3).every((v,i)=>v>removed[i]),"the light green bond is brighter than the dish");
  assert(normal.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-removed[i]),0)>100,"the bond is visibly distinct from the dish");
  assert(selected.slice(0,3).every((v,i)=>v>normal[i]),"selection brightens the bond against the dark dish");
  assert(selected.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-normal[i]),0)>30,"selection visibly changes the bond");
  console.log(JSON.stringify({gpuPixels:{normal,selected,removed}}));
  assert(await evaluate(`(() => {try {__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array([2]));return false;}catch(e){return String(e).includes('bond snapshot');}})()`));

  // Subtract the textured dish at the same coordinates to isolate the cell's texture.
  await evaluate("__gpuFixture.renderer.set_frame(new Float32Array(),__gpuFixture.terrain,new Uint32Array());__gpuFixture.renderer.set_view(1536,1536,2,0,-1);window.__captureGPU='gpu-fixture';__gpuFixture.renderer.render()");
  const {body:emptyBody}=await gpuImage();
  await evaluate("__gpuFixture.renderer.set_frame(new Float32Array([2,1536,1536,10,0,1,0,0,0,0,0xff0000,0]),__gpuFixture.terrain,new Uint32Array());window.__captureGPU='gpu-fixture';__gpuFixture.renderer.render()");
  const {body:granules}=await gpuImage("work/cellhub-cell-texture.png");
  const alpha=granules.map(([red],i)=>(red-emptyBody[i][0])/(255-emptyBody[i][0]));
  assert(Math.max(...alpha)-Math.min(...alpha)>0.1,"cytoplasm varies across one cell independently of the substrate");

  // A black custom cell retains its dark body and gets a visible nucleus/outline.
  await evaluate("__gpuFixture.renderer.set_frame(new Float32Array([2,1536,1536,10,0,1,0,0,0,0,0,0]),__gpuFixture.terrain,new Uint32Array());__gpuFixture.renderer.set_view(1536,1536,2,0,-1);window.__captureGPU='gpu-fixture';__gpuFixture.renderer.render()");
  await delay(100);
  const {pixel:darkNucleus}=await gpuImage("work/cellhub-custom-dark-nucleus.png");
  assert(darkNucleus.slice(0,3).every(v=>v>180),"a black custom cell has a light visible nucleus");
  await evaluate("__gpuFixture.renderer.set_view(1545.75,1536,2,0,-1);window.__captureGPU='gpu-fixture';__gpuFixture.renderer.render()");
  await delay(100);
  const {pixel:darkOutline}=await gpuImage("work/cellhub-custom-dark-outline.png");
  assert(darkOutline.slice(0,3).every(v=>v>150),"a black custom cell has a light visible outline");
  await evaluate("__gpuFixture.renderer.set_view(1531,1536,2,0,-1);window.__captureGPU='gpu-fixture';__gpuFixture.renderer.render()");
  await delay(100);
  const {pixel:darkBody}=await gpuImage("work/cellhub-custom-dark-body.png");
  assert(darkBody.slice(0,3).every((v,i)=>v<removed[i]),"contrast fallback preserves the custom black body");

  await evaluate(`(()=>{
    const {renderer,terrain}=__gpuFixture;
    const nitrates=terrain.slice();
    for(let i=2;i<nitrates.length;i+=4)nitrates[i]=1;
    renderer.set_frame(new Float32Array(),nitrates,new Uint32Array());
    renderer.set_view(1536,1536,1,4,-1);window.__captureGPU='gpu-fixture';renderer.render();
  })()`);
  const {pixel:nitrate}=await gpuImage();
  assert(nitrate[2]>nitrate[1]+12&&nitrate[2]>nitrate[0]+40,"the nitrate layer uses the projected field and its own blue scale");
  await evaluate("__gpuFixture.renderer.destroy();__gpuFixture.renderer.free();__gpuFixture.canvas.remove();delete window.__gpuFixture");
  console.log("PASS: GPU textures, nitrate layer, bonds, custom colors and invalid-frame rejection");

}
