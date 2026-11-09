import { useEffect, useRef } from "react";
import init, { WorldRenderer } from "./rust-wasm/cellhub_frontend.js";
import { WORLD, STRIDE, type Frame } from "./model";
export type ViewState = { zoom: number; x: number; y: number };
type Props = {
 frame: Frame | null; tool: string; layer: string; selected: number | null;
 onPoint: (x: number, y: number, id: number | null) => void;
 onView: (v: ViewState) => void; onRenderer: (name: string) => void;
 onError: (message: string) => void; recenter: number;
};
let moduleReady: Promise<unknown> | undefined;
export function WorldView(props: Props) {
 const host=useRef<HTMLDivElement>(null), live=useRef(props);
 live.current=props;
 const reset=useRef<(() => void)|null>(null);
 useEffect(()=>{reset.current?.();},[props.recenter]);
 useEffect(()=>{
  const el=host.current!, canvas=document.createElement("canvas");
  canvas.style.width="100%";canvas.style.height="100%";
  canvas.setAttribute("aria-label","Субстрат CellHub: клетки и биомы");el.appendChild(canvas);
  let disposed=false,renderer:WorldRenderer|null=null,animation=0;
  const camera={x:WORLD/2,y:WORLD/2,zoom:0.55};
  reset.current=()=>{camera.x=WORLD/2;camera.y=WORLD/2;camera.zoom=Math.max(0.001,Math.min(el.clientWidth,el.clientHeight)/WORLD*0.93);live.current.onView({...camera});};
  reset.current();
  let down:{x:number;y:number;cx:number;cy:number}|null=null,moved=false;
  const point=(e:PointerEvent|WheelEvent)=>{const r=canvas.getBoundingClientRect();return {x:e.clientX-r.left,y:e.clientY-r.top};};
  const world=(p:{x:number;y:number})=>({x:(p.x-el.clientWidth/2)/camera.zoom+camera.x,y:(p.y-el.clientHeight/2)/camera.zoom+camera.y});
  const pd=(e:PointerEvent)=>{const p=point(e);down={...p,cx:camera.x,cy:camera.y};moved=false;canvas.setPointerCapture(e.pointerId);};
  const pm=(e:PointerEvent)=>{if(!down)return;const p=point(e);if(Math.hypot(p.x-down.x,p.y-down.y)>5)moved=true;if(moved||live.current.tool==="pan"){camera.x=down.cx-(p.x-down.x)/camera.zoom;camera.y=down.cy-(p.y-down.y)/camera.zoom;}};
  const pu=(e:PointerEvent)=>{
   if(down&&!moved&&live.current.tool!=="pan"){
    const p=world(point(e));if(p.x>=0&&p.y>=0&&p.x<=WORLD&&p.y<=WORLD){
     let id:number|null=null,best=Infinity;const data=live.current.frame?.cells;
     if(data)for(let i=0;i<data.length;i+=STRIDE){const d=Math.hypot(data[i+1]-p.x,data[i+2]-p.y);if(d<Math.max(data[i+3],12/camera.zoom)&&d<best){id=data[i];best=d;}}
     live.current.onPoint(p.x,p.y,id);
    }
   }
   down=null;if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);
  };
  const cancel=()=>{down=null;};
  const wheel=(e:WheelEvent)=>{e.preventDefault();const p=point(e),before=world(p);camera.zoom=Math.max(0.12,Math.min(4,camera.zoom*Math.exp(-e.deltaY*0.001)));const after=world(p);camera.x+=before.x-after.x;camera.y+=before.y-after.y;};
  canvas.addEventListener("pointerdown",pd);canvas.addEventListener("pointermove",pm);canvas.addEventListener("pointerup",pu);canvas.addEventListener("pointercancel",cancel);canvas.addEventListener("lostpointercapture",cancel);canvas.addEventListener("wheel",wheel,{passive:false});
  (async()=>{
   if(!("gpu" in navigator))throw new Error("WebGPU недоступен. Используйте браузер с WebGPU через localhost или HTTPS.");
   await(moduleReady??=init().catch(e=>{moduleReady=undefined;throw e;}));if(disposed)return;
   const instance=await WorldRenderer.create(canvas);if(disposed){instance.destroy();instance.free();return;}
   renderer=instance;live.current.onRenderer("Rust WebGPU");
   let oldFrame:Frame|null=null,oldView="",counter=0;
   const tick=()=>{
    if(disposed||!renderer)return;
    try{
     const p=live.current;
     if(p.frame&&p.frame!==oldFrame){renderer.set_frame(p.frame.cells,p.frame.terrain);oldFrame=p.frame;}
     const layer=({biome:0,light:1,radiation:2,food:3} as Record<string,number>)[p.layer]??0;
     const view=`${camera.x}/${camera.y}/${camera.zoom}/${layer}/${p.selected}`;
     if(view!==oldView){renderer.set_view(camera.x,camera.y,camera.zoom,layer,p.selected??-1);oldView=view;}
     if(counter++%15===0)p.onView({...camera});animation=requestAnimationFrame(tick);
    }catch(e){live.current.onError(`Renderer: ${String(e)}`);}
   };tick();
  })().catch(e=>{if(!disposed)live.current.onError(`Renderer: ${String(e)}`);});
  return()=>{
   disposed=true;reset.current=null;cancelAnimationFrame(animation);
   canvas.removeEventListener("pointerdown",pd);canvas.removeEventListener("pointermove",pm);canvas.removeEventListener("pointerup",pu);canvas.removeEventListener("pointercancel",cancel);canvas.removeEventListener("lostpointercapture",cancel);canvas.removeEventListener("wheel",wheel);
   if(renderer){renderer.destroy();renderer.free();}canvas.remove();
  };
 },[]);
 return <div className="world" ref={host}/>;
}
