set -euo pipefail

cd "$(dirname "$0")/.."

say() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

usage() {
  cat <<'EOF'
usage: check.sh [all|code|book]

  all    everything CI runs and the two heavy runs it leaves to this script (the default)
  code   the Rust and Lisp checks: fmt, clippy, tests, budget, wasm
  book   the book and prose checks only; needs no toolchain, takes a moment

Prose is written and reviewed far more often than the VM changes under it,
and the code half takes about five minutes, so the two halves can be run
apart. CI runs the book half and the quick part of the code half; the two
heavy runs, GC torture and the tests under wasmtime, are this script's.
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

  say "test placement"
  ./scripts/check-test-placement.sh

  say "differential test coverage"
  ./scripts/check-differential.sh

  say "compiler and library file lists"
  ./scripts/check-compiler.sh

  say "SBCL oracle"
  ./scripts/oracle.sh

  say "rustdoc"
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

  say "wasm32-unknown-unknown (browser build)"
  cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown

  # The page: the bundle it imports, a world for it, and the two smoke
  # tests CI runs on them (#197). Until this was here a change to
  # renderer.js, sexp.js, index.html or the bindings was first tested on
  # GitHub, though this script is said to run everything CI does. Each
  # needs a tool that is optional on a developer's machine, and says so
  # when it is missing.
  if command -v wasm-pack >/dev/null 2>&1 && command -v node >/dev/null 2>&1; then
    say "the page: bundle, world, and a form answered in a JavaScript engine"
    wasm-pack build crates/quokka-wasm --target web --out-dir www/pkg
    cargo run --release -p quokka -- build crates/quokka-wasm/www/world.qi
    node scripts/smoke-wasm.mjs
    say "the page in a headless browser"
    ./scripts/smoke-browser.sh
  else
    say "the page: SKIPPED (needs wasm-pack and node); CI runs it"
  fi

  # The two heavy runs, side by side. Each runs the whole suite again —
  # GC torture collecting and verifying the heap after every slice, the
  # other on WebAssembly — and one after the other they were five and a
  # half of this script's seven minutes, with cores idle throughout: the
  # rebuild tests in each are a chain that no number of cores shortens.
  #
  # Both are *built* first, one after the other, because two cargos building
  # into one target directory only take turns at its lock. Then each runs
  # with its output in a file of its own, and both are shown under their
  # headings when both are done — two runs writing to one terminal at once
  # would be unreadable, and a failure in either must still be found.
  wasi=()
  if command -v wasmtime >/dev/null 2>&1; then
    # An image built here, natively, for the wasm32 tests to load: a browser
    # runs a world that some 64-bit machine built (#84).
    cargo run --quiet --release -p quokka -- build target/native-world.qi
    wasi=(cargo test -p quokka-vm -p quokka-bootstrap --target wasm32-wasip1)
    "${wasi[@]}" --no-run
  fi
  torture=(cargo test -p quokka-vm -p quokka-bootstrap --features quokka-vm/gc-torture)
  "${torture[@]}" --no-run

  logs=target/check
  mkdir -p "$logs"
  "${torture[@]}" > "$logs/torture.log" 2>&1 &
  torture_pid=$!
  wasi_pid=""
  if [ "${#wasi[@]}" -gt 0 ]; then
    # Through a runner that deals the tests out to a wasmtime per core: WASI
    # has no threads, and in one process this run alone was six minutes.
    QUOKKA_ROOT="$PWD" QUOKKA_FOREIGN_IMAGE="$PWD/target/native-world.qi" QUOKKA_FOREIGN_IMAGE_REQUIRED=1 \
    CARGO_TARGET_WASM32_WASIP1_RUNNER="$PWD/scripts/wasi-test-runner.sh" \
      "${wasi[@]}" > "$logs/wasi.log" 2>&1 &
    wasi_pid=$!
  fi

  # `wait` gives each run's own exit status, and under `set -e` the first
  # failure would end the script before the second was looked at — so both
  # are collected, both logs shown, and then the script fails if either did.
  failed=0
  wait "$torture_pid" || failed=1
  say "GC torture (collect at every mutation, then verify)"
  cat "$logs/torture.log"

  say "wasm32-wasip1 (tests)"
  if [ -n "$wasi_pid" ]; then
    wait "$wasi_pid" || failed=1
    cat "$logs/wasi.log"
  else
    echo "wasmtime not installed — building only. Install: brew install wasmtime"
    cargo build -p quokka-vm -p quokka-bootstrap --target wasm32-wasip1
  fi
  if [ "$failed" -ne 0 ]; then
    echo "check.sh: GC torture or the wasm32-wasip1 tests failed; see above" >&2
    exit 1
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
