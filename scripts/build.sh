#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -f .tools/emsdk/emsdk_env.sh ]; then
  source .tools/emsdk/emsdk_env.sh
fi
if ! command -v emcmake >/dev/null 2>&1; then
  echo 'Emscripten не найден. Установите emsdk и выполните: source /path/to/emsdk/emsdk_env.sh' >&2
  exit 1
fi
emcmake cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
