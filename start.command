#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [ ! -d node_modules ]; then npm ci; fi
npm run build:wasm
npm run build:rust-frontend
exec npm run dev
