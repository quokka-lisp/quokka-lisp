#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Rule 7: the Rust core stays under 15,000 lines.
#
# The rule existed from the start and nothing checked it, so the number lived
# in commit messages and drifted with whatever anybody happened to measure.
# What it is *for* is how much of the system a person has to read to
# understand it — so tests do not count. They are evidence that the system
# works, not part of how it works, and they live in directories of their own
# so that this distinction is a path rather than an argument.
#
# There are two such directories and they are different things:
#
#   src/unit/   unit tests, included into their module with #[path], so they
#               see private items
#   tests/      cargo's integration tests: a separate crate that can only
#               reach the public API, which is the point of it
# Raised to 15,500 by step T (#67, Carsten, 23 Nov 2026): arrays of every
# rank, fill pointers, adjustable and displaced arrays and bit vectors were
# pulled from tier 2 into phase 2, and their representation is Rust's side of
# rule 1 (`crates/quokka-vm/src/array.rs`, ~345 lines). The budget is a guide,
# not a ceiling; this is the deliberate raise it allows.
#
# Raised to 15,537 by step Z (#73, Carsten, 24 Nov 2026): a structure became
# a heap object of its own (`ObjectType::Instance`, four primitives in
# `primitives.rs`), `block`/`catch` markers moved to `ObjectType::Marker`,
# and `structures.lisp` joined `boot::LIBRARY`. 69 lines, of which 32 fitted
# under the old limit.
#
# Raised to 15,713 by step AC (#76, 24 Nov 2026): a macro's expander became
# a function of the form and the environment, as in CL (Carsten's choice of
# the faithful option), so a macro lambda list is taken apart in generated
# code, which needed `&whole`, `&environment`, `&key` and `&aux` there
# (`lambda_list.rs`); default forms bound under fresh names; `values-list`'s
# checks (`vm.rs`); image format 6. 176 lines, against an estimate of 80-120.
LIMIT=15713

not_core=( -path '*/unit/*' -o -path '*/tests/*' )

core=$(find crates -name '*.rs' -not \( "${not_core[@]}" \) -print0 | xargs -0 cat | wc -l | tr -d ' ')
tests=$(find crates -name '*.rs' \( "${not_core[@]}" \) -print0 | xargs -0 cat | wc -l | tr -d ' ')

status=0

# A `#[test]` outside those two directories means one crept back into a source
# file, where it would count against the budget and, worse, would be read by
# someone trying to understand the system.
stray=$(grep -rln '^\s*#\[test\]' crates --include='*.rs' | grep -v '/unit/' | grep -v '/tests/' || true)
if [ -n "$stray" ]; then
  echo "test functions outside src/unit/ and tests/:" >&2
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
