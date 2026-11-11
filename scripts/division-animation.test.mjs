import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import ts from "typescript";

const model = new URL("../frontend/model.ts", import.meta.url);
const { STRIDE } = await import(model.href);
const source = await readFile(new URL("../frontend/division-animation.ts", import.meta.url), "utf8");
const javascript = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
}).outputText.replace('"./model"', JSON.stringify(model.href));
const { DivisionAnimator } = await import(`data:text/javascript;base64,${Buffer.from(javascript).toString("base64")}`);

const cell = (id, x, radius, angle = 0) => [id, x, 50, radius, 0, 1, 0, 0, 0, angle, -1, 0];
const frame = (tick, cells, divisions = []) => ({
  cells: new Float32Array(cells.flat()), stats: new Float32Array([tick]),
  divisions: new Float32Array(divisions.flat()),
});
const pose = (cells, row) => [cells[row * STRIDE + 1], cells[row * STRIDE + 3]];

test("observed daughters unfold from the same footprint without mutating simulation geometry", () => {
  const animator = new DivisionAnimator();
  const parentAngle = Math.PI * 2 - 0.2;
  animator.update(frame(10, [cell(1, 100, 10, parentAngle)]), 0);
  const next = frame(11, [cell(2, 80, 6, 0.2), cell(3, 120, 8)], [
    [11, 1, 2, 3, 100, 50, 10, parentAngle],
  ]);
  animator.update(next, 100);
  assert.deepEqual(pose(animator.sample(100), 0), [100, 10]);
  assert.deepEqual(pose(animator.sample(100), 1), [100, 10]);
  assert.deepEqual(pose(animator.sample(190), 0), [90, 8]);
  assert.deepEqual(pose(animator.sample(190), 1), [110, 9]);
  assert.ok(Math.abs(animator.sample(190)[9] - Math.PI * 2) < 0.00001, "Uses the shortest turn across zero");
  assert.deepEqual(pose(next.cells, 0), [80, 6]);
  assert.equal(animator.sample(280), next.cells);
  assert.equal(animator.active, false);
});

test("an active daughter starts a new division from her current drawn pose", () => {
  const animator = new DivisionAnimator();
  animator.update(frame(10, [cell(1, 100, 10)]), 0);
  animator.update(frame(11, [cell(2, 80, 6), cell(3, 120, 8)], [[11, 1, 2, 3, 100, 50, 10, 0]]), 100);
  const next = frame(12, [cell(3, 124, 8), cell(4, 65, 5), cell(5, 90, 5)], [[12, 2, 4, 5, 80, 50, 6, 0]]);
  animator.update(next, 190);
  assert.deepEqual(pose(animator.sample(190), 1), [90, 8]);
  assert.deepEqual(pose(animator.sample(190), 2), [90, 8]);
  assert.equal(animator.sample(370), next.cells);
});

test("unseen parents, old events, rewinds and skipped previews render immediately", () => {
  const animator = new DivisionAnimator();
  const first = frame(10, [cell(1, 100, 10)]);
  const hiddenDivision = frame(11, [cell(2, 80, 6)], [[11, 9, 2, 3, 100, 50, 10, 0]]);
  animator.update(first, 0);
  animator.update(hiddenDivision, 10);
  assert.equal(animator.sample(10), hiddenDivision.cells);
  animator.update(first, 20);
  const next = frame(11, [cell(2, 80, 6)], [[10, 1, 2, 3, 100, 50, 10, 0]]);
  animator.update(next, 30);
  assert.equal(animator.sample(30), next.cells);
  animator.update(first, 40);
  const fresh = frame(11, [cell(2, 80, 6)], [[11, 1, 2, 3, 100, 50, 10, 0]]);
  animator.update(fresh, 50, false);
  assert.equal(animator.sample(50), fresh.cells);
  animator.update(null, 60);
  assert.equal(animator.sample(60), undefined);
});
