#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null && [ -x .tools/cargo/bin/cargo ]; then
  export RUSTUP_HOME="$PWD/.tools/rustup" CARGO_HOME="$PWD/.tools/cargo"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
cargo build --release --target wasm32-unknown-unknown -p cellhub-simulation
if ! command -v wasm-bindgen >/dev/null; then
  echo 'Install matching bindings CLI: cargo install wasm-bindgen-cli --version 0.2.130 --locked' >&2
  exit 1
fi
wasm-bindgen target/wasm32-unknown-unknown/release/cellhub_simulation.wasm --target web --out-dir frontend/wasm --out-name simulation
