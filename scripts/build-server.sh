#!/usr/bin/env bash
# Build the authoritative server as a static Linux binary.
#
# The target defaults to aarch64-unknown-linux-musl (the original ARM VM) and
# can be overridden with CELLHUB_SERVER_TARGET, e.g.:
#
#   CELLHUB_SERVER_TARGET=x86_64-unknown-linux-musl bash scripts/build-server.sh
#
# The target std comes from the pinned toolchain: the Nix dev shell declares the
# supported targets in flake.nix / rust-toolchain.toml, and CI adds the same
# targets to rustup. Linking uses rust-lld from the host toolchain, so no cross
# gcc, glibc or expensive Nix cross build is involved.
#
# Output: target/deploy/cellhub-server
set -euo pipefail
cd "$(dirname "$0")/.."

target=${CELLHUB_SERVER_TARGET:-aarch64-unknown-linux-musl}
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

# Verify the architecture we asked for and that nothing needs a system loader.
case "$target" in
  aarch64*) elf_machine='AArch64' ;;
  x86_64*) elf_machine='X86-64' ;;
  *) elf_machine='' ;;
esac
if command -v readelf >/dev/null 2>&1 && [ -n "$elf_machine" ]; then
  if ! readelf -h "$artifact" | grep -q "$elf_machine"; then
    echo "✗ $artifact is not a $target ELF" >&2
    exit 1
  fi
  if readelf -l "$artifact" | grep -q 'INTERP'; then
    echo "✗ $artifact is dynamically linked; expected a static binary" >&2
    exit 1
  fi
fi

echo "✓ static $target server: $artifact ($(du -h "$artifact" | cut -f1))"
