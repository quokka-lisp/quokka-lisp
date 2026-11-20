#!/usr/bin/env bash
# Everything CI runs, in one command. CLAUDE.md rule 2: native and wasm32, every commit.
set -euo pipefail

cd "$(dirname "$0")/.."

say() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

# Use rustup's cargo, not whatever happens to be first on PATH. Only the rustup
# shim honours rust-toolchain.toml, and a distro/Homebrew cargo typically has no
# wasm32 std, which breaks the wasm legs below with a misleading error.
if command -v rustup >/dev/null 2>&1; then
  # Put the whole toolchain first, not just cargo: cargo finds rustc on PATH, so
  # a Homebrew rustc would still be used and would still lack wasm32 std.
  PATH="$(dirname "$(rustup which cargo)"):$PATH"
  export PATH
else
  echo "warning: rustup not found; using $(command -v cargo). The wasm targets may be missing." >&2
fi

say "rustfmt"
cargo fmt --all -- --check

say "clippy"
cargo clippy --workspace --all-targets -- -D warnings

say "native build and test"
cargo build --workspace
cargo test --workspace

say "book structure"
./scripts/check-book.sh

say "rustdoc"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

say "GC torture (collect at every mutation, then verify)"
cargo test -p quokka-vm --features gc-torture

say "wasm32-unknown-unknown (browser build)"
cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown

say "wasm32-wasip1 (tests)"
if command -v wasmtime >/dev/null 2>&1; then
  CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=." \
    cargo test -p quokka-vm --target wasm32-wasip1
else
  echo "wasmtime not installed — building only. Install: brew install wasmtime"
  cargo build -p quokka-vm --target wasm32-wasip1
fi

say "all green"
