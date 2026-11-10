# CellHub: локальная разработка
set shell := ["bash", "-euo", "pipefail", "-c"]

bindgen_dir := justfile_directory() / ".tools" / "wasm-bindgen"
export PATH := bindgen_dir / "bin" + ":" + env_var("PATH")

# Запустить dev-сервер (действие по умолчанию)
default: dev

# Показать список рецептов
list:
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
    npm run build:rust-frontend
    npm run dev

# Production-сборка: WASM + TypeScript + Vite
build: _deps _wasm-bindgen
    npm run build

# Проверка TypeScript (после сборки WASM)
check: build-wasm
    npm run build:rust-frontend
    npm run check

# Отформатировать Rust-код
fmt:
    bash scripts/rust.sh fmt --all

# Проверить порядок модулей в Rust-крейтах
check-order:
    python3 scripts/module_order.py crates/*/src/lib.rs

# Установить git-хуки (rustfmt и порядок модулей перед коммитом)
install-hooks:
    git config core.hooksPath scripts/hooks

# Все тесты: native Rust + настоящий WASM
test: _deps _wasm-bindgen
    npm test

# Сетевой смоук: публичная комната, чанки и server-authoritative команды
test-server: _deps
    npm run test:server

# Собрать деплой-артефакт: статический aarch64-сервер
server-build:
    bash scripts/build-server.sh

# Запустить авторитарный сервер локально (WebSocket + статика dist)
server: _deps
    bash scripts/rust.sh run -p cellhub-server -- --bind 127.0.0.1:8082 --static dist

# Только тесты модели на Rust
test-rust:
    npm run test:rust

# Только тесты WASM
test-wasm: _wasm-bindgen
    npm run test:wasm

# Посмотреть production-сборку после `just build`
preview:
    npm run preview

# Один раз установить system-сервис cellhub.service на VM (нужен root SSH):
# just bootstrap <host> [ssh-port] [user] [identity]
bootstrap host port="2223" user="root" identity="" serve_port="8081" max_rooms="16" max_players="16":
    #!/usr/bin/env bash
    set -euo pipefail

    ssh_opts=(-o ConnectTimeout=10 -p "{{port}}")
    if [ -n "{{identity}}" ]; then
      identity="{{identity}}"
      identity="${identity/#\~/$HOME}"
      ssh_opts+=(-i "$identity" -o IdentitiesOnly=yes)
    fi

    ssh "${ssh_opts[@]}" "{{user}}@{{host}}" \
      "bash -s -- '/var/lib/cellhub' '{{serve_port}}' '{{max_rooms}}' '{{max_players}}'" \
      < scripts/bootstrap-system-service.sh

# Задеплоить production-сборку и статический aarch64-сервер на VM:
# just deploy <host> [ssh-port] [user] [identity]
# или: CELLHUB_SSH_KEY=~/.ssh/... just deploy <host>
deploy host port="2223" user="cellhub" identity=env_var_or_default("CELLHUB_SSH_KEY", "") serve_port="8081":
    #!/usr/bin/env bash
    set -euo pipefail

    just build
    bash scripts/build-server.sh

    ssh_opts=(-o ConnectTimeout=10 -p "{{port}}")
    if [ -n "{{identity}}" ]; then
      identity="{{identity}}"
      identity="${identity/#\~/$HOME}"
      ssh_opts+=(-i "$identity" -o IdentitiesOnly=yes)
    fi
    target="{{user}}@{{host}}"

    remote_home=$(ssh "${ssh_opts[@]}" "$target" 'printf %s "$HOME"')
    if [ -z "$remote_home" ]; then
      echo "✗ не удалось определить HOME на {{host}}" >&2
      exit 1
    fi
    remote_bin="$remote_home/.local/bin"
    remote_incoming="$remote_bin/cellhub-server.new"

    echo "▸ rsync dist → ${target}:${remote_home}/"
    ssh "${ssh_opts[@]}" "$target" "mkdir -p '$remote_bin'"
    rsync -a --delete --exclude '.*' -e "ssh ${ssh_opts[*]}" dist/ "${target}:${remote_home}/"

    echo "▸ rsync cellhub-server → ${target}:${remote_incoming}"
    rsync -a --chmod=F755 -e "ssh ${ssh_opts[*]}" \
      target/deploy/cellhub-server "${target}:${remote_incoming}"

    echo "▸ установка system-сервиса cellhub.service на 0.0.0.0:{{serve_port}}"
    ssh "${ssh_opts[@]}" "$target" \
      "bash -s -- '$remote_home' '{{serve_port}}' 'cellhub.service'" \
      < scripts/deploy-remote.sh

    echo "✓ deploy завершён: https://{{host}} (сервер :{{serve_port}})"
