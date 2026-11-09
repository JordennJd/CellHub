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

# Установить git-хуки (проверка rustfmt перед коммитом)
install-hooks:
    git config core.hooksPath scripts/hooks

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

# Задеплоить production-сборку на тестовый сервер: just deploy <host> [ssh-port] [user]
deploy host port="2223" user="root" remote_dir="/var/lib/cellhub" serve_port="8081":
    #!/usr/bin/env bash
    set -euo pipefail

    just build

    ssh_opts=(-o ConnectTimeout=10 -p "{{port}}")
    target="{{user}}@{{host}}"

    echo "▸ rsync → ${target}:{{remote_dir}}/"
    ssh "${ssh_opts[@]}" "$target" "mkdir -p '{{remote_dir}}'"
    rsync -a --delete -e "ssh ${ssh_opts[*]}" dist/ "${target}:{{remote_dir}}/"

    echo "▸ запускаю HTTP-сервер на :{{serve_port}}"
    ssh "${ssh_opts[@]}" "$target" 'bash -s' <<'REMOTE'
    set -euo pipefail
    remote_dir="{{remote_dir}}"
    serve_port="{{serve_port}}"

    test -f "$remote_dir/index.html"

    # Разрешить reverse-proxy основного сервера ходить на этот порт.
    if iptables -S nixos-fw >/dev/null 2>&1; then
      if ! iptables -C nixos-fw -p tcp --dport "$serve_port" -j nixos-fw-accept 2>/dev/null; then
        iptables -I nixos-fw 1 -p tcp --dport "$serve_port" -j nixos-fw-accept
      fi
    fi

    # На microVM нет caddy в PATH, но бинарник есть в общем /nix/store.
    caddy_bin=$(ls -d /nix/store/*-caddy-*/bin/caddy 2>/dev/null | sort -V | tail -1 || true)
    if [ -z "$caddy_bin" ]; then
      echo "✗ caddy не найден в /nix/store" >&2
      exit 1
    fi

    systemctl stop cellhub.service 2>/dev/null || true
    systemctl reset-failed cellhub.service 2>/dev/null || true
    systemd-run --unit=cellhub --collect \
      --property=Restart=on-failure \
      --property=WorkingDirectory="$remote_dir" \
      --setenv=HOME=/tmp \
      "$caddy_bin" file-server --root "$remote_dir" --listen ":$serve_port" >/dev/null

    ready=0
    for _ in {1..20}; do
      if curl -fsS "http://127.0.0.1:$serve_port/" >/dev/null 2>&1; then
        ready=1
        break
      fi
      sleep 0.25
    done
    if [ "$ready" != 1 ]; then
      echo "✗ HTTP-сервер не поднялся на :$serve_port" >&2
      exit 1
    fi

    echo "✓ CellHub слушает 0.0.0.0:$serve_port"
    REMOTE

    echo "✓ deploy завершён (HTTP :{{serve_port}} внутри microVM)"
