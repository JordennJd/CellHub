// Export the real frontend organism catalog for
// `cargo run -p cellhub-simulation --example organism_check`.
//
// Usage: node scripts/check-organisms.mjs [output.json]
import { writeFile } from "node:fs/promises";
import { build } from "esbuild";

const bundle = await build({
  entryPoints: [new URL("../frontend/organisms.ts", import.meta.url).pathname],
  bundle: true,
  write: false,
  format: "esm",
  platform: "node",
  logLevel: "silent",
});
const modelBundle = await build({
  entryPoints: [new URL("../frontend/model.ts", import.meta.url).pathname],
  bundle: true,
  write: false,
  format: "esm",
  platform: "node",
  logLevel: "silent",
});
const load = async (output) =>
  import(`data:text/javascript;base64,${Buffer.from(output.text).toString("base64")}`);

const organisms = await load(bundle.outputFiles[0]);
const model = await load(modelBundle.outputFiles[0]);
const catalog = {
  pool: Array.from(model.flatten(model.chaosGenome())),
  entries: organisms.DEMO_ORGANISMS.map((organism) => ({
    id: organism.id,
    span: organism.span,
    initial: organism.initialMode,
    genome: Array.from(model.flatten(organism.genome())),
  })),
};
const output = process.argv[2];
if (output) await writeFile(output, JSON.stringify(catalog));
else process.stdout.write(JSON.stringify(catalog));
