import assert from "node:assert/strict";

export async function verifyTouchControls({ call, evaluate, until, delay, screenshot, url }) {
  await call("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  await call("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  await call("Page.navigate", { url: `${url}/?paused` });
  await until("window.__cellhubFrame && document.querySelector('.world canvas[data-renderer]') && document.querySelector('.map-toolbar')");
  assert.equal(await evaluate("document.querySelector('.app').dataset.controls"), "touch");
  assert(await evaluate("document.documentElement.scrollWidth <= window.innerWidth"));
  assert.equal(await evaluate("document.querySelectorAll('.map-toolbar button').length"), 3);
  const center = await evaluate("(() => {const r=document.querySelector('.world canvas').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,z:Math.min(r.width,r.height)/3072*0.93};})()");
  const finger = (id, x, y) => ({ id, x, y, radiusX: 2, radiusY: 2 });
  const touch = (type, touchPoints) => call("Input.dispatchTouchEvent", { type, touchPoints });
  const tap = async (x, y) => {
    await touch("touchStart", [finger(1, x, y)]);
    await touch("touchEnd", []);
    await delay(100);
  };
  const tool = async (name) => {
    await evaluate(`document.querySelector('.map-toolbar button[title="${name}"]').click()`);
    await delay(80);
  };
  await tap(center.x, center.y);
  await until("window.__cellhubFrame.cells.length===10");
  assert(Math.abs((await evaluate("window.__cellhubFrame.cells[1]")) - 1536) < 1);
  await tool("Выбрать");
  await tap(center.x, center.y);
  await until("!!document.querySelector('.apply')");
  await tool("Поместить");
  // Enlarge around a moving midpoint: the original cell must follow that midpoint.
  await touch("touchStart", [finger(1, center.x - 35, center.y), finger(2, center.x + 35, center.y)]);
  await touch("touchMove", [finger(1, center.x - 45, center.y), finger(2, center.x + 85, center.y)]);
  await touch("touchEnd", [finger(1, center.x - 45, center.y)]);
  await touch("touchEnd", []);
  await delay(150);
  assert.equal(await evaluate("window.__cellhubFrame.cells.length"), 10, "pinch must not place a cell");
  await tap(center.x, center.y);
  await until("window.__cellhubFrame.cells.length===20");
  const afterPinch = await evaluate("Array.from(window.__cellhubFrame.cells)");
  assert(Math.abs(afterPinch[11] - (1536 - 20 / (center.z * 130 / 70))) < 2, "pinch zoom and moving anchor");
  // Pinch inward and keep moving the last finger: still no accidental tap.
  await touch("touchStart", [finger(1, center.x - 65, center.y), finger(2, center.x + 65, center.y)]);
  await touch("touchMove", [finger(1, center.x - 35, center.y), finger(2, center.x + 35, center.y)]);
  await touch("touchEnd", [finger(1, center.x - 35, center.y)]);
  await touch("touchMove", [finger(1, center.x - 15, center.y + 10)]);
  await touch("touchEnd", []);
  await touch("touchStart", [finger(1, center.x, center.y)]);
  await touch("touchCancel", []);
  await delay(100);
  assert.equal(await evaluate("window.__cellhubFrame.cells.length"), 20);
  await tool("Переместить");
  await touch("touchStart", [finger(1, center.x, center.y)]);
  await touch("touchMove", [finger(1, center.x + 40, center.y + 25)]);
  await touch("touchEnd", []);
  await delay(100);
  assert.equal(await evaluate("window.__cellhubFrame.cells.length"), 20);
  await tool("Поместить");
  await tap(center.x, center.y);
  await until("window.__cellhubFrame.cells.length===30");
  await screenshot("work/cellhub-mobile-controls.png");
  await evaluate("document.querySelector('.editor-title').scrollIntoView()");
  assert((await evaluate("window.scrollY")) > 0, "the editor must remain scrollable outside the canvas");
  await screenshot("work/cellhub-mobile-editor.png");
  await call("Emulation.setTouchEmulationEnabled", { enabled: false });
  await call("Emulation.setDeviceMetricsOverride", { width: 1440, height: 1080, deviceScaleFactor: 1, mobile: false });
  console.log("PASS: mobile tools, tap/select, two-finger zoom and pan, remaining finger, cancellation, editor scroll");
}
