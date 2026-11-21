set -euo pipefail

cd "$(dirname "$0")/.."

# Rule 7: the Rust core stays under 15,000 lines.
#
# The rule existed from the start and nothing checked it, so the number lived
# in commit messages and drifted with whatever anybody happened to measure.
# What it is *for* is how much of the system a person has to read to
# understand it — so test modules do not count. They are evidence that the
# system works, not part of how it works, and they live in `src/tests/` so
# that this distinction is a directory rather than an argument.
LIMIT=15000

core=$(find crates -name '*.rs' -not -path '*/tests/*' -print0 | xargs -0 cat | wc -l | tr -d ' ')
tests=$(find crates -path '*/tests/*' -name '*.rs' -print0 | xargs -0 cat | wc -l | tr -d ' ')

status=0

# Tests belong in src/tests/. A `#[test]` anywhere else means one crept back
# into a source file, where it would count against the budget and, worse,
# would be read by someone trying to understand the system.
stray=$(grep -rln '^\s*#\[test\]' crates --include='*.rs' | grep -v '/tests/' || true)
if [ -n "$stray" ]; then
  echo "test functions outside src/tests/:" >&2
  echo "$stray" >&2
  status=1
fi

if [ "$core" -gt "$LIMIT" ]; then
  echo "rule 7: the Rust core is $core lines, over the $LIMIT-line budget" >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  printf 'budget: %s of %s lines of core, %s lines of tests beside it\n' \
    "$core" "$LIMIT" "$tests"
fi
exit $status
