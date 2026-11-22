#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Every Lisp file in lisp/compiler/ must be listed in boot.rs's COMPILER, in
# the order it is to be read. There is no `load` yet, so a file nobody lists
# is a file nobody reads — and it would sit there looking like part of the
# compiler while contributing nothing.
#
# The same reason check-differential.sh exists, and the same failure it stops.
status=0
count=0

# The compiler and the tier-1 library are both listed by hand in boot.rs,
# because order matters and there is nothing to discover them with. A file in
# either directory that is not listed would never be loaded.
for path in lisp/compiler/*.lisp lisp/library/*.lisp; do
  [ -e "$path" ] || continue
  count=$((count + 1))
  name=$(basename "$path")
  if ! grep -q "\"$name\"" crates/quokka-vm/src/boot.rs; then
    echo "not listed in boot.rs (COMPILER or LIBRARY): $path" >&2
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "compiler and library: $count file(s), all listed"
fi
exit $status
