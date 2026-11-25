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

# The baselines are one SBCL's answers (scripts/sbcl.pin, #117). Another
# version may honestly disagree with them, so say which one this is before
# any disagreement is read as Quokka's fault. CI refuses any other; here it
# is a note, because a developer's SBCL is whatever their package manager has.
pinned=$(sed -n 's/^SBCL_VERSION=//p' scripts/sbcl.pin)
if [ "$(sbcl --version)" != "SBCL $pinned" ]; then
  echo "oracle: this is $(sbcl --version); the baselines were made with SBCL $pinned." >&2
  echo "oracle: a disagreement below may be SBCL's, and a new baseline must come from $pinned." >&2
fi

# A test that binds a COMMON-LISP symbol will disagree for a reason that is the
# test's fault. Catch it first, with a message that names the symbol.
if ! sbcl --script scripts/check-cl-names.lisp; then
  echo "oracle: fix the names above before comparing" >&2
  exit 1
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
