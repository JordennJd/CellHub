#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null && [ -x .tools/cargo/bin/cargo ]; then
  export RUSTUP_HOME="$PWD/.tools/rustup" CARGO_HOME="$PWD/.tools/cargo"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
cargo build --release --target wasm32-unknown-unknown -p cellhub-simulation
if [ -x .tools/wasm-bindgen/bin/wasm-bindgen ]; then
  wasm_bindgen=.tools/wasm-bindgen/bin/wasm-bindgen
elif command -v wasm-bindgen >/dev/null; then
  wasm_bindgen=wasm-bindgen
else
  echo 'Install matching bindings CLI: cargo install wasm-bindgen-cli --version 0.2.130 --locked --root .tools/wasm-bindgen' >&2
  exit 1
fi
"$wasm_bindgen" target/wasm32-unknown-unknown/release/cellhub_simulation.wasm --target web --out-dir frontend/wasm --out-name simulation
