import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const exe = process.platform === "win32" ? ".exe" : "";
const simulation = process.argv.includes("--simulation");
const crate = simulation ? "cellhub-simulation" : "cellhub-frontend";
const stem = simulation ? "cellhub_simulation" : "cellhub_frontend";
const output = simulation ? "frontend/wasm" : "frontend/rust-wasm";
const name = simulation ? "simulation" : "cellhub_frontend";
const env = { ...process.env };
const projectCargo = join(root, ".tools", "cargo");
const candidates = [env.CARGO_HOME, projectCargo, join(homedir(), ".cargo")]
  .filter(Boolean).map((dir) => join(dir, "bin", `cargo${exe}`));
const cargo = candidates.find(existsSync) ?? `cargo${exe}`;
if (cargo === join(projectCargo, "bin", `cargo${exe}`)) {
  env.CARGO_HOME = projectCargo;
  env.RUSTUP_HOME ??= join(root, ".tools", "rustup");
}
const localBindgen = join(root, ".tools", "wasm-bindgen", "bin", `wasm-bindgen${exe}`);
const bindgen = existsSync(localBindgen) ? localBindgen : `wasm-bindgen${exe}`;
const target = resolve(root, env.CARGO_TARGET_DIR ?? "target");
function run(program, args, buildEnv) {
  const result = spawnSync(program, args, { stdio: "inherit", cwd: root, env: buildEnv });
  if (result.error) throw new Error(`Cannot run ${program}: ${result.error.message}`);
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function build(directory, destination, cargoArgs = [], buildEnv = env, moduleName = name) {
  run(cargo, ["build", "--locked", "-p", crate, "--release", "--target",
    "wasm32-unknown-unknown", "--target-dir", directory, ...cargoArgs], buildEnv);
  run(bindgen, [join(directory, "wasm32-unknown-unknown", "release", `${stem}.wasm`),
    "--target", "web", "--out-dir", destination, "--out-name", moduleName], env);
}

build(target, output);
if (simulation) {
  // Rebuild std with atomics only for the parallel variant. The ordinary build
  // stays usable in browsers without cross-origin isolation.
  const flags = [
    "-Ctarget-feature=+atomics,+bulk-memory",
    "-Clink-arg=--shared-memory",
    "-Clink-arg=--max-memory=1073741824",
    "-Clink-arg=--import-memory",
    "-Clink-arg=--export=__wasm_init_tls",
    "-Clink-arg=--export=__tls_size",
    "-Clink-arg=--export=__tls_align",
    "-Clink-arg=--export=__tls_base",
  ];
  build(join(target, "wasm-threads"), `${output}/threaded`,
    ["--features", "wasm-threads", "-Z", "build-std=panic_abort,std"],
    { ...env, RUSTFLAGS: [env.RUSTFLAGS, ...flags].filter(Boolean).join(" ") }, "index");
}
