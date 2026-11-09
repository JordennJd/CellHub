# CellHub: локальная разработка
set shell := ["bash", "-euo", "pipefail", "-c"]

bindgen_dir := justfile_directory() / ".tools" / "wasm-bindgen"
export PATH := bindgen_dir / "bin" + ":" + env_var("PATH")

# Показать список рецептов
default:
    @just --list

# Установить JS-зависимости, если их ещё нет
_deps:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d node_modules ]; then npm ci; fi

# Установить wasm-bindgen-cli 0.2.130 в .tools/wasm-bindgen, если его нет
_wasm-bindgen:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cargo >/dev/null && [ -x .tools/cargo/bin/cargo ]; then
      export RUSTUP_HOME="$PWD/.tools/rustup" CARGO_HOME="$PWD/.tools/cargo"
      export PATH="$CARGO_HOME/bin:$PATH"
    fi
    bindgen="{{bindgen_dir}}/bin/wasm-bindgen"
    if [ -x "$bindgen" ] && "$bindgen" --version 2>/dev/null | grep -qx 'wasm-bindgen 0.2.130'; then
      exit 0
    fi
    echo "Устанавливаю wasm-bindgen-cli 0.2.130 в .tools/wasm-bindgen..."
    cargo install wasm-bindgen-cli --version 0.2.130 --locked --root "{{bindgen_dir}}"

# Собрать WASM в frontend/wasm
build-wasm: _wasm-bindgen
    npm run build:wasm

# Запустить dev-сервер Vite (http://127.0.0.1:5173)
dev: _deps build-wasm
    npm run dev

# Production-сборка: WASM + TypeScript + Vite
build: _deps _wasm-bindgen
    npm run build

# Проверка TypeScript (после сборки WASM)
check: build-wasm
    npm run check

# Все тесты: native Rust + настоящий WASM
test: _deps _wasm-bindgen
    npm test

# Только тесты модели на Rust
test-rust:
    npm run test:rust

# Только тесты WASM
test-wasm: _wasm-bindgen
    npm run test:wasm

# Посмотреть production-сборку после `just build`
preview:
    npm run preview
