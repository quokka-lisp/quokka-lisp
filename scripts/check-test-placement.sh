#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Rule 7: stay comprehensible. This checks the one part of that a script can
# check, and reports one figure a person may want.
#
# **Tests live in directories of their own.** A source file is what someone
# reads to understand how the system works; a test is evidence that it does.
# So a `#[test]` belongs in one of two places, and one found anywhere else
# fails the build:
#
#   src/unit/   unit tests, included into their module with #[path], so they
#               see private items
#   tests/      cargo's integration tests: a separate crate that can only
#               reach the public API, which is the point of it
#
# **How large each crate is**, printed and not judged. There was a limit
# here once — 15,000 lines of Rust, a number from the first design sketch —
# and it was raised twenty-one times, each with a paragraph of
# justification, until Carsten called it what it was (25 Nov 2026, #89): an
# artificial figure that took attention from building the system well. What
# keeps a codebase small is rule 1 (Rust below the primitive table, Lisp
# above it) and a clear separation of concerns, which is what the crates
# are. A count cannot tell a hundred lines that check an image before
# trusting it from a hundred that should have been Lisp; a reviewer can.
# The figures stay because they answer a fair question — where is the Rust?
# — and git has the history of the limit for anyone who wants it.

not_source=( -path '*/unit/*' -o -path '*/tests/*' )

stray=$(grep -rln '^\s*#\[test\]' crates --include='*.rs' | grep -v '/unit/' | grep -v '/tests/' || true)
if [ -n "$stray" ]; then
  echo "test functions outside src/unit/ and tests/:" >&2
  echo "$stray" >&2
  exit 1
fi

lines() { find "$@" -print0 | xargs -0 cat | wc -l | tr -d ' '; }

echo "tests: all in src/unit/ or tests/"
echo "Rust, in lines, tests apart:"
for crate in quokka-vm quokka-bootstrap quokka quokka-wasm; do
  printf '  %6s  %-17s %6s of tests\n' \
    "$(lines "crates/$crate" -name '*.rs' -not \( "${not_source[@]}" \))" \
    "$crate" \
    "$(lines "crates/$crate" -name '*.rs' \( "${not_source[@]}" \))"
done
