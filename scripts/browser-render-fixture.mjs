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
            const offset=(Math.floor(height/2)*width+Math.floor(width/2))*4;
            return {pixel:Array.from(pixels.slice(offset,offset+4)),png:image.toDataURL('image/png').split(',')[1]};
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
    await writeFile(path,Buffer.from(result.png,"base64"));return result.pixel;
  };
  const normal=await gpuImage("work/cellhub-bond-normal.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_view(1536,1536,1,0,9);__gpuFixture.renderer.render()");
  await delay(100);
  const selected=await gpuImage("work/cellhub-bond-selected.png");
  await evaluate("window.__captureGPU='gpu-fixture';__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array());__gpuFixture.renderer.render()");
  await delay(100);
  const removed=await gpuImage("work/cellhub-bond-removed.png");
  assert.deepEqual(removed,[240,245,235,255],"removing the bond reveals the light dish background");
  assert.equal(normal[3],255,"the bond remains opaque over the dish");
  assert.equal(selected[3],255,"selection remains opaque over the dish");
  assert(normal.slice(0,3).every((v,i)=>v<removed[i]),"the green bond is darker than the dish");
  assert(normal.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-removed[i]),0)>100,"the bond is visibly distinct from the dish");
  assert(selected.slice(0,3).every((v,i)=>v<normal[i]),"selection darkens the bond instead of turning it white");
  assert(selected.slice(0,3).reduce((sum,v,i)=>sum+Math.abs(v-normal[i]),0)>30,"selection visibly changes the bond");
  console.log(JSON.stringify({gpuPixels:{normal,selected,removed}}));
  assert(await evaluate(`(() => {try {__gpuFixture.renderer.set_frame(__gpuFixture.cells,__gpuFixture.terrain,new Uint32Array([2]));return false;}catch(e){return String(e).includes('bond snapshot');}})()`));
  await evaluate("__gpuFixture.renderer.destroy();__gpuFixture.renderer.free();__gpuFixture.canvas.remove();delete window.__gpuFixture");
  console.log("PASS: GPU bond fixtures and invalid-frame rejection");

}
