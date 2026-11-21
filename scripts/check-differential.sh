#!/usr/bin/env bash
# Every differential test file must be listed in the Rust test, which embeds
# them with include_str! and so cannot discover them.
#
# A test file nobody runs is worse than no test file: it looks like coverage.
set -euo pipefail

cd "$(dirname "$0")/.."

TEST=crates/quokka-vm/tests/differential.rs
status=0

for file in tests/lisp/*.lisp; do
  stem=$(basename "$file" .lisp)
  # Twice: once to be run, and once in the compile leg.
  if [ "$(grep -c "\"$stem\"" "$TEST")" -lt 2 ]; then
    echo "missing: $file is not listed in both the run and the compile cases of $TEST" >&2
    status=1
  fi
  if [ ! -f "${file%.lisp}.expected" ]; then
    echo "missing: ${file%.lisp}.expected — run 'target/debug/quokka --run $file'" >&2
    status=1
  fi
done

for baseline in tests/lisp/*.expected; do
  if [ ! -f "${baseline%.expected}.lisp" ]; then
    echo "orphan: $baseline has no .lisp beside it" >&2
    status=1
  fi
  # `#<...>` is Common Lisp's mark for "this cannot be read back", and every
  # implementation renders it differently — SBCL puts an address in it. A
  # baseline containing one can never match the oracle.
  #
  # The README said not to print a function. functions.lisp did it anyway, on
  # the first run, and only SBCL noticed. A rule in prose is a hope; this is
  # the same rule where it can fail the build.
  if grep -n '#<' "$baseline" >/dev/null 2>&1; then
    echo "unprintable: $baseline contains #<...>, which no two implementations render alike:" >&2
    grep -n '#<' "$baseline" | sed 's/^/    /' >&2
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "differential: $(ls tests/lisp/*.lisp | wc -l | tr -d ' ') files, all listed with baselines"
fi
exit $status
