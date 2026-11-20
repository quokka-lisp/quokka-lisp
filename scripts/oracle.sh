#!/usr/bin/env bash
# The SBCL leg of the differential tests.
#
# Runs every tests/lisp/*.lisp through SBCL and compares against the committed
# baseline. A disagreement means Quokka and a real Common Lisp differ — which
# is the entire point, and is a failing build rather than a shrug.
#
# SBCL is an oracle, not a build dependency: without it this skips and says so.
set -uo pipefail

cd "$(dirname "$0")/.."

if ! command -v sbcl >/dev/null 2>&1; then
  echo "sbcl not installed — the oracle leg is skipped. Install: brew install sbcl"
  exit 0
fi

status=0
checked=0
for file in tests/lisp/*.lisp; do
  baseline="${file%.lisp}.expected"
  if [ ! -f "$baseline" ]; then
    echo "no baseline for $file" >&2
    status=1
    continue
  fi
  actual=$(sbcl --script scripts/oracle.lisp "$file" 2>/dev/null)
  if [ "$actual" = "$(cat "$baseline")" ]; then
    checked=$((checked + 1))
  else
    status=1
    echo "=== $file: SBCL disagrees with the baseline ===" >&2
    diff <(cat "$baseline") <(printf '%s\n' "$actual") \
      | sed 's/^</  quokka /; s/^>/  sbcl   /' >&2
  fi
done

if [ "$status" -eq 0 ]; then
  echo "oracle: $checked files agree with SBCL"
fi
exit $status
