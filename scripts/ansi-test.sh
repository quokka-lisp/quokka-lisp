#!/usr/bin/env bash
# Measure Quokka against ansi-test, the Common Lisp conformance suite (#60).
#
#   scripts/ansi-test.sh                      run, and fail if a test that
#                                             passed no longer does (the ratchet)
#   ANSI_TEST_UPDATE=1 scripts/ansi-test.sh   run, and rewrite the baseline
#                                             (tests/ansi/baseline.txt) and the
#                                             summary (docs/ansi-test.md)
#
# The suite is fetched into target/ansi-test at a pinned commit (decision 1),
# so every run measures the same tests; without a network connection, and no
# copy yet, it skips and says so. Not part of check.sh (decision 5): it takes
# minutes and needs the network once. The driver is
# crates/quokka-bootstrap/tests/ansi.rs; the runner, tests/ansi/runner.lisp.
set -euo pipefail

cd "$(dirname "$0")/.."

# ansi-test at gitlab.common-lisp.net, MIT licence. Change deliberately, and
# update the baseline in the same commit.
REPO=https://gitlab.common-lisp.net/ansi-test/ansi-test.git
PIN=941290a6151b1c0e5ede819739fe5f46d4b3d6b6

# The rustup toolchain first, as check.sh does.
export PATH="$HOME/.cargo/bin:$PATH"

if [ ! -d target/ansi-test/.git ]; then
  mkdir -p target
  if ! git clone --quiet "$REPO" target/ansi-test; then
    # In CI a skip would hide a broken ratchet, so there it is a failure.
    if [ -n "${ANSI_TEST_REQUIRED:-}" ]; then
      echo "ansi-test: cannot fetch $REPO" >&2
      exit 1
    fi
    echo "ansi-test: cannot fetch $REPO; skipped" >&2
    exit 0
  fi
fi
if [ "$(git -C target/ansi-test rev-parse HEAD)" != "$PIN" ]; then
  git -C target/ansi-test fetch --quiet origin || true
  git -C target/ansi-test checkout --quiet "$PIN"
fi

cargo test --release -p quokka-bootstrap --test ansi -- --ignored --nocapture
