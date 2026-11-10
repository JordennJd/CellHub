#!/usr/bin/env bash
# Build the authoritative server as a static aarch64 Linux binary.
#
# The target std comes from the pinned toolchain: the Nix dev shell declares
# aarch64-unknown-linux-musl in flake.nix / rust-toolchain.toml, and CI adds the
# same target to rustup. Linking uses rust-lld from the host toolchain, so no
# aarch64 gcc, glibc or expensive Nix cross build is involved.
#
# Output: target/deploy/cellhub-server
set -euo pipefail
cd "$(dirname "$0")/.."

target=aarch64-unknown-linux-musl
artifact=target/deploy/cellhub-server

if ! command -v cargo >/dev/null && [ -x .tools/cargo/bin/cargo ]; then
  export RUSTUP_HOME="$PWD/.tools/rustup" CARGO_HOME="$PWD/.tools/cargo"
  export PATH="$CARGO_HOME/bin:$PATH"
fi

host_sysroot=$(rustc --print sysroot)
host=$(rustc -vV | sed -n 's/^host: //p')
host_rustlib="$host_sysroot/lib/rustlib/$host"
rust_lld="$host_rustlib/bin/rust-lld"
if [ ! -x "$rust_lld" ]; then
  echo "✗ rust-lld not found at $rust_lld" >&2
  echo "  enter the project dev shell first: nix develop" >&2
  exit 1
fi

if ! find "$host_sysroot/lib/rustlib/$target/lib" -maxdepth 1 -name 'libstd-*.rlib' -print -quit 2>/dev/null | grep -q .; then
  echo "✗ rust-std for $target is not installed in $host_sysroot" >&2
  echo "  Nix: add \"$target\" to flake.nix targets and run nix develop" >&2
  echo "  rustup: rustup target add $target" >&2
  exit 1
fi

mkdir -p "$(dirname "$artifact")"
export CARGO_TARGET_DIR="$PWD/target"
export "CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER=$rust_lld"
cargo build --release --locked --target "$target" -p cellhub-server

install -m 755 "target/$target/release/cellhub-server" "$artifact"

if command -v readelf >/dev/null 2>&1; then
  if ! readelf -h "$artifact" | grep -q 'AArch64'; then
    echo "✗ $artifact is not an aarch64 ELF" >&2
    exit 1
  fi
  if readelf -l "$artifact" | grep -q 'INTERP'; then
    echo "✗ $artifact is dynamically linked; expected a static binary" >&2
    exit 1
  fi
fi

echo "✓ static aarch64 server: $artifact ($(du -h "$artifact" | cut -f1))"
