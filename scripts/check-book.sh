#!/usr/bin/env bash
# Every chapter listed in the book's table of contents must exist, and every
# chapter file must be listed. mdBook would catch the first; nothing catches the
# second, and an orphaned chapter is invisible to readers.
set -euo pipefail

cd "$(dirname "$0")/.."

SRC=book/src
SUMMARY=$SRC/SUMMARY.md
status=0

# Markdown links in SUMMARY.md, e.g. [Title](path/to/file.md)
listed=$(grep -oE '\]\([^)]+\.md\)' "$SUMMARY" | sed -E 's/^\]\(//; s/\)$//' | sort -u)

for path in $listed; do
  if [ ! -f "$SRC/$path" ]; then
    echo "missing: $SUMMARY lists $path, which does not exist" >&2
    status=1
  fi
done

present=$(cd "$SRC" && find . -name '*.md' ! -name 'SUMMARY.md' | sed 's|^\./||' | sort -u)
for path in $present; do
  if ! printf '%s\n' "$listed" | grep -qxF "$path"; then
    echo "orphan: $path exists but is not listed in $SUMMARY" >&2
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "book: $(printf '%s\n' "$listed" | wc -l | tr -d ' ') chapters, all listed and present"
fi
exit $status
