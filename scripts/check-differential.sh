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
  if ! grep -q "\"$stem\"" "$TEST"; then
    echo "missing: $file is not listed in $TEST" >&2
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
done

if [ "$status" -eq 0 ]; then
  echo "differential: $(ls tests/lisp/*.lisp | wc -l | tr -d ' ') files, all listed with baselines"
fi
exit $status
