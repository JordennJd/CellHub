import assert from 'node:assert/strict';
import test from 'node:test';
import { bindCanvasControls } from '../frontend/input/canvas-controls.ts';

test('zoom cancels held placement and release does not create a cell', () => {
  const originals = Object.fromEntries(['window','document','setTimeout','clearTimeout','setInterval','clearInterval'].map(key => [key, globalThis[key]]));
  const timeouts = new Map(), intervals = new Map(); let timerId = 0;
  globalThis.window = new EventTarget(); globalThis.document = new EventTarget();
  globalThis.setTimeout = callback => { timeouts.set(++timerId, callback); return timerId; };
  globalThis.clearTimeout = id => timeouts.delete(id);
  globalThis.setInterval = callback => { intervals.set(++timerId, callback); return timerId; };
  globalThis.clearInterval = id => intervals.delete(id);
  const canvas = new EventTarget(), captured = new Set(), placed = [];
  Object.assign(canvas, {clientWidth:400,clientHeight:400,getBoundingClientRect:()=>({left:0,top:0}),setPointerCapture:id=>captured.add(id),hasPointerCapture:id=>captured.has(id),releasePointerCapture:id=>captured.delete(id)});
  const send = (type, data={}) => canvas.dispatchEvent(Object.assign(new Event(type,{cancelable:true}),{pointerType:'mouse',pointerId:1,button:0,buttons:1,clientX:200,clientY:200,...data}));
  const cleanup = bindCanvasControls(canvas, {camera:{x:500,y:500,zoom:1},worldSize:3072,tool:()=> 'place',onPoint:p=>placed.push(p),onView:()=>{}});
  try {
    send('pointerdown');
    for(const callback of [...timeouts.values()]) callback();
    assert.equal(placed.length,1,'long hold produces its first cell');
    assert.equal(intervals.size,1,'holding schedules continued placement');
    send('wheel',{deltaY:-120,deltaMode:0});
    assert.equal(intervals.size,0,'zoom cancels repeated placement');
    send('pointerup',{buttons:0});
    assert.equal(placed.length,1,'release after zoom does not place another cell');
    send('pointerdown'); send('pointerup',{buttons:0});
    assert.equal(placed.length,2,'a later ordinary click still works');
  } finally {
    cleanup();
    for(const [key,value] of Object.entries(originals)) {
      if(value === undefined) delete globalThis[key]; else globalThis[key] = value;
    }
  }
});
