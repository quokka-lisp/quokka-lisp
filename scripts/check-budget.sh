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
#
# Raised to 15,762 by step AD (#77, 25 Nov 2026): an array of element type
# nil is an object of its own, as SBCL's is (`ObjectType::NilVector`, a
# fourth `Kind` in `array.rs`, the `%make-nil-vector` primitive). 49 lines.
#
# Raised to 15,767 by step AF (#79, 25 Nov 2026): the primitives that make a
# symbol from a string read one that is not simple (a fill pointer,
# displaced) by its active characters. 8 lines, 3 of them under the old
# limit.
#
# Raised to 15,838 by step AG (#80, 25 Nov 2026): `throw` looks for its
# catch before unwinding, so a missing one is a catchable control-error
# (`vm.rs`); a lambda list's rewrite puts a `special` declaration of a
# required parameter in front of its `let*`, and a macro's `&optional`/`&key`
# take nested patterns (`lambda_list.rs`). 71 lines.
#
# Raised to 15,964 by step AI (#59, 25 Nov 2026): ratios, decided tier 1.
# `ObjectType::Ratio` and `crate::ratio` (making one, and which double an
# `equalp` table must hash it as), four primitives, the fallback taking a
# ratio, hashing and comparing ratio keys by value, the Rust printer. 126
# lines; the algorithms are Lisp (`lisp/library/ratios.lisp`).
#
# Raised to 16,017 by step AJ (#82, 25 Nov 2026): `%string-mismatch`, the
# walk all twelve string comparisons share, as a primitive — in Lisp it
# cost some 500 steps for two eleven-letter names, and sorting symbols by
# name ran out of steps. 53 lines.
#
# Raised to 16,095 by the review's first fixes (#90, #91, #92, 25 Nov 2026):
# no Lisp input may stop the process. The Rust printer's depth and size
# limits (`printer.rs`); `Mutation::fits` and `primitives::size`, asked before
# any allocation whose size a program chose; `Mutation::list_elements`, a walk
# with Brent's cycle detection, for every Rust loop that follows cdrs. 78
# lines, all of them bounds — Rust by nature, since they guard Rust loops.
#
# Raised to 16,180 by the review's heap and reader fixes (#85, #101, 25 Nov
# 2026): the collection trigger that follows what is alive, conses counted
# toward the hard limit, both semispaces shrunk after a spike, `Vm::abandon`
# and the recovery in `eval_source` (`heap.rs`, `vm.rs`); the Rust reader
# counting every kind of nesting and expanding a backquote in a loop
# (`reader.rs`). 85 lines, most of them the comments that say why.
LIMIT=16180

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
