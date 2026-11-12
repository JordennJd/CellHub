import assert from 'node:assert/strict';
import test from 'node:test';
import { bindCanvasControls, fitDishCamera, isPointInDish, resizeDishCamera, zoomDishCamera } from '../frontend/input/canvas-controls.ts';

function controlsFixture(camera = {x:500,y:500,zoom:1}, worldSize = 3072, selectedTool) {
  const originals = Object.fromEntries(['window','document','setTimeout','clearTimeout','setInterval','clearInterval'].map(key => [key, globalThis[key]]));
  const timeouts = new Map(), intervals = new Map(); let timerId = 0;
  globalThis.window = new EventTarget(); globalThis.document = new EventTarget();
  globalThis.setTimeout = callback => { timeouts.set(++timerId, callback); return timerId; };
  globalThis.clearTimeout = id => timeouts.delete(id);
  globalThis.setInterval = callback => { intervals.set(++timerId, callback); return timerId; };
  globalThis.clearInterval = id => intervals.delete(id);
  const canvas = new EventTarget(), captured = new Set(), placed = [], actions = [];
  Object.assign(canvas, {clientWidth:400,clientHeight:400,getBoundingClientRect:()=>({left:0,top:0}),setPointerCapture:id=>captured.add(id),hasPointerCapture:id=>captured.has(id),releasePointerCapture:id=>captured.delete(id)});
  const send = (type, data={}) => canvas.dispatchEvent(Object.assign(new Event(type,{cancelable:true}),{pointerType:'mouse',pointerId:1,button:0,buttons:1,clientX:200,clientY:200,...data}));
  const removeControls = bindCanvasControls(canvas, {camera,worldSize,tool:()=> selectedTool ?? 'place',mouseTool:()=> selectedTool ?? 'place',onPoint:(p, action)=>{placed.push(p);actions.push(action);},onView:()=>{}});
  return { send, placed, actions, timeouts, intervals, cleanup: () => {
    removeControls();
    for(const [key,value] of Object.entries(originals)) {
      if(value === undefined) delete globalThis[key]; else globalThis[key] = value;
    }
  }};
}

test('zoom cancels held placement and release does not create a cell', () => {
  const { send, placed, timeouts, intervals, cleanup } = controlsFixture();
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
  }
});

test('queued capture release from a previous click does not cancel the next held stroke', () => {
  const {send,placed,timeouts,cleanup}=controlsFixture();
  try {
    send('pointerdown');send('pointerup',{buttons:0});
    send('pointerdown');send('lostpointercapture');
    send('pointermove',{clientX:240});
    for(const callback of [...timeouts.values()]) callback();
    assert.deepEqual(placed,[{x:500,y:500},{x:540,y:500}]);
    send('pointerup',{buttons:0});
  } finally {cleanup();}
});

test('rectangular field includes corners and rejects points outside its bounds', () => {
  assert(isPointInDish({x:500,y:500},1000));
  for(const point of [{x:0,y:500},{x:1000,y:500},{x:500,y:0},{x:500,y:1000}]) assert(isPointInDish(point,1000));
  for(const point of [{x:0,y:0},{x:1000,y:1000},{x:50,y:50}]) assert(isPointInDish(point,1000));
  for(const point of [{x:-1,y:50},{x:1001,y:500},{x:500,y:1001}]) assert(!isPointInDish(point,1000));
  assert(isPointInDish({x:1800,y:900},1000,2000));
  assert(!isPointInDish({x:2001,y:900},1000,2000));
});

test('clicks and moving holds can place in rectangular corners', () => {
  const {send,placed,timeouts,intervals,cleanup}=controlsFixture({x:500,y:500,zoom:0.4},1000);
  try {
    send('pointerdown',{clientX:20,clientY:20}); send('pointerup',{clientX:20,clientY:20,buttons:0});
    assert.deepEqual(placed,[{x:50,y:50}]);
    send('pointerdown',{clientX:20,clientY:20}); for(const cb of [...timeouts.values()]) cb();
    send('pointermove',{clientX:100,clientY:200}); for(const cb of [...intervals.values()]) cb();
    assert.deepEqual(placed,[{x:50,y:50},{x:50,y:50},{x:250,y:500}]);
    send('pointermove',{clientX:420,clientY:200}); for(const cb of [...intervals.values()]) cb();
    send('pointerup',{clientX:420,buttons:0}); assert.equal(placed.length,3);
  } finally {cleanup();}
});

test('captured release outside canvas does not place in an offscreen part of the dish', () => {
  const { send, placed, cleanup } = controlsFixture({x:500,y:500,zoom:1},1000);
  try {
    send('pointerdown');
    send('pointermove',{clientX:420});
    send('pointerup',{clientX:420,buttons:0});
    assert.equal(placed.length,0);
  } finally { cleanup(); }
});

test('full dish fit follows viewport resizing, while a manually changed view is preserved', () => {
  const initial = {width:800,height:600}, portrait = {width:390,height:440};
  const fitted = fitDishCamera(initial,3000);
  assert.equal(fitted.x,1500);
  assert.equal(fitted.y,1500);
  assert(Math.abs(fitted.zoom * 3000 - 600)<1e-9);
  const resized = resizeDishCamera(fitted,initial,portrait,3000);
  assert.equal(resized.x,1500);
  assert.equal(resized.y,1500);
  assert(Math.abs(resized.zoom * 3000 - 390)<1e-9,'full rectangular field fits without an artificial circular margin');
  for(const manual of [{...fitted,x:1510},{...fitted,zoom:0.8}]) {
    assert.deepEqual(resizeDishCamera(manual,initial,portrait,3000),manual);
  }
});

test('button zoom keeps a selected world point anchored and clamps the supported scale', () => {
  const camera = {x:1500,y:1500,zoom:0.25}, viewport = {width:800,height:600}, selected = {x:1700,y:1300};
  const zoomed = zoomDishCamera(camera,1.5,selected,viewport,3000);
  assert.equal(zoomed.zoom,0.375);
  assert(Math.abs((selected.x-zoomed.x)*zoomed.zoom-(selected.x-camera.x)*camera.zoom)<1e-9);
  assert(Math.abs((selected.y-zoomed.y)*zoomed.zoom-(selected.y-camera.y)*camera.zoom)<1e-9);
  const centered = zoomDishCamera(camera,1.5,camera,viewport,3000);
  assert.equal(centered.x,camera.x);
  assert.equal(centered.y,camera.y);
  assert.equal(zoomDishCamera(camera,100,camera,viewport,3000).zoom,4);
  assert.equal(zoomDishCamera(camera,0.001,camera,viewport,3000).zoom,0.12);
});


test('explicit mouse tools inspect without placement and pan without interacting', () => {
  for (const tool of ['select', 'boost', 'remove', 'move', 'pan']) {
    const {send, actions, timeouts, cleanup} = controlsFixture(undefined, undefined, tool);
    try {
      send('pointerdown');
      assert.equal(timeouts.size, 0, `${tool} must not start held placement`);
      send('pointerup', {buttons:0});
      assert.deepEqual(actions, tool === 'pan' ? [] : ['select']);
    } finally { cleanup(); }
  }
});
