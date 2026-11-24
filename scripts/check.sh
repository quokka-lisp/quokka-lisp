set -euo pipefail

cd "$(dirname "$0")/.."

say() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

usage() {
  cat <<'EOF'
usage: check.sh [all|code|book]

  all    everything CI runs (the default)
  code   the Rust and Lisp checks: fmt, clippy, tests, budget, wasm
  book   the book and prose checks only; needs no toolchain, takes a moment

Prose is written and reviewed far more often than the VM changes under it,
and the code half takes about two and a half minutes, so the two halves can
be run apart. CI still runs both.
EOF
}

# The book half. No cargo, so this works in a checkout with no Rust installed.
book_checks() {
  say "book structure"
  ./scripts/check-book.sh
}

# The code half, which is everything else.
code_checks() {
  if command -v rustup >/dev/null 2>&1; then
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

  say "line budget"
  ./scripts/check-budget.sh

  say "differential test coverage"
  ./scripts/check-differential.sh

  say "SBCL oracle"
  ./scripts/oracle.sh

  say "rustdoc"
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

  say "GC torture (collect at every mutation, then verify)"
  cargo test -p quokka-vm -p quokka-bootstrap --features quokka-vm/gc-torture

  say "wasm32-unknown-unknown (browser build)"
  cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown

  say "wasm32-wasip1 (tests)"
  if command -v wasmtime >/dev/null 2>&1; then
    CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=. --dir=$PWD" \
      cargo test -p quokka-vm -p quokka-bootstrap --target wasm32-wasip1
  else
    echo "wasmtime not installed — building only. Install: brew install wasmtime"
    cargo build -p quokka-vm -p quokka-bootstrap --target wasm32-wasip1
  fi
}

case "${1:-all}" in
  all)
    # The book first: it takes a moment and catches a missing chapter before
    # two and a half minutes of compiling.
    book_checks
    code_checks
    ;;
  code|--code) code_checks ;;
  book|--book) book_checks ;;
  -h|--help|help) usage; exit 0 ;;
  *)
    echo "check.sh: unknown argument '$1'" >&2
    usage >&2
    exit 2
    ;;
esac

say "all green"
