# Experimental Rust frontend

Branch: `feat/rust-webgpu-frontend`.

The Rust/egui/wgpu application is at `/` and `/rust.html`. The React/Pixi baseline remains at `/legacy.html` for comparison. Both use the same simulation crate. Rust controls the editor, camera, input, map and cells; JavaScript on the Rust page only loads WASM and reports startup failures. WebGPU requires localhost or HTTPS and a compatible browser.

Windows build:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-rust-frontend.ps1
npm run check
node node_modules/vite/bin/vite.js
```

Then open `http://localhost:5173/rust.html`. Existing WASM bindings must also be available to build the comparison React entry. After both WASM builds, `node node_modules/vite/bin/vite.js build` includes both pages.

The Rust implementation uses egui geometry and viewport culling, not GPU compute simulation or an instanced cell shader. Simulation currently executes on the browser main thread. Its visual layout differs from the React interface. These are material differences to review before replacing the default application.

Benchmark setup (installed Google Chrome):

```powershell
npm install --no-save --package-lock=false playwright
node scripts/benchmark-frontends.mjs
```

The script starts Vite on port 5188, uses 1440×1000 at device scale 1, creates deterministic populations of 1000/5000 cells, waits five seconds and measures requestAnimationFrame intervals for ten seconds. It compares WebGL, WebGPU and Rust with simulation paused/running. Output contains FPS, p95 interval, GPU identification and errors. This measures frame scheduling, not GPU-only time. Repeat runs on an idle computer; canvas viewport sizes and visual complexity differ, so results describe the applications, not an isolated API speedup. Running populations can change through division/death.

Do not infer a speedup from successful compilation. Review browser errors, screenshot and timings before merging.

## Measured results, 2026-11-09

Headless installed Chrome, Intel gen-12lp adapter, 1440×1000, DPR 1. One final run; these are application-level results, not isolated WebGPU/Rust speedups.

| Initial cells | Simulation | React/WebGL FPS | React/WebGPU FPS | Rust/WebGPU FPS |
| --- | --- | ---: | ---: | ---: |
| 1000 | paused | 24.36 | 22.19 | 60.00 |
| 1000 | running | 24.22 | 23.16 | 60.00 |
| 5000 | paused | 5.65 | 6.06 | 34.89 |
| 5000 | running | 6.28 | 5.54 | 28.65 |

At 5000 initial cells running, p95 frame interval: WebGL 200 ms, Pixi WebGPU 250 ms, Rust 50 ms. No page errors were reported. Rust startup, a placement click and saving 64 genome values to localStorage passed; screenshot confirmed the rendered world/editor. Full production build, TypeScript checking, eight native simulation tests and two actual WASM tests passed. Running populations may change, and Rust's main-thread simulation and different viewport/layout limit attribution of the improvement.
