import assert from "node:assert/strict";
import { test, after } from "node:test";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";

// Exercise the actual client modules without requiring a browser or server.
const directory = await mkdtemp(join(tmpdir(), "cellhub-prediction-tests-"));
for (const [source, target] of [
  ["frontend/model.ts", "model.mjs"],
  ["frontend/net/protocol.ts", "protocol.mjs"],
  ["frontend/net/prediction.ts", "prediction.mjs"],
]) {
  const original = await readFile(new URL(`../${source}`, import.meta.url), "utf8");
  const output = ts.transpileModule(original, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText.replaceAll('"../model"', '"./model.mjs"').replaceAll('"./protocol"', '"./protocol.mjs"');
  await writeFile(join(directory, target), output);
}
after(() => rm(directory, { recursive: true, force: true }));
const { PredictionWorld } = await import(pathToFileURL(join(directory, "prediction.mjs")));
const { CELL_STRIDE, RENDER_STRIDE } = await import(pathToFileURL(join(directory, "protocol.mjs")));

test("network bond prediction keeps either anchored endpoint fixed and skips two anchors", () => {
  for (const [anchorA, anchorB] of [[true, false], [false, true], [true, true]]) {
    const cells = new Float32Array(2 * CELL_STRIDE);
    for (const [index, anchored] of [anchorA, anchorB].entries()) {
      const start = index * CELL_STRIDE;
      cells.set([index + 1, 100 + index * 60, 100, 10, 0, index + 1], start);
      cells[start + 11] = 1; // Bonded; authoritative propulsion speed is zero.
      cells[start + 27] = Number(anchored); // details[12] after eight signals.
    }
    const world = new PredictionWorld();
    world.applyChunk({
      tick: 1, population: 2, births: 0, deaths: 0, infections: 0,
      chunkX: 0, chunkY: 0, cells, links: new Uint32Array([1, 2]),
      terrain: new Float32Array(),
    }, 1000);
    world.tick(1 / 30, 1033);
    const frame = world.frame().cells;
    const positions = new Map(Array.from({ length: 2 }, (_, index) => {
      const start = index * RENDER_STRIDE;
      return [frame[start], [frame[start + 1], frame[start + 2]]];
    }));
    if (anchorA) assert.deepEqual(positions.get(1), [100, 100], "first anchor stays fixed");
    if (anchorB) assert.deepEqual(positions.get(2), [160, 100], "second anchor stays fixed");
    if (!anchorA) assert.deepEqual(positions.get(1), [138, 100], "free first endpoint takes the full correction");
    if (!anchorB) assert.deepEqual(positions.get(2), [122, 100], "free second endpoint takes the full correction");
  }
});
