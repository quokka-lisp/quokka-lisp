#!/usr/bin/env bash
# Render every step of every environment mock-up (book/src/mockups/*.html)
# to a PNG in book/src/mockups/png/, for pages and printouts where the
# interactive version cannot run. Each page opens at #step=N on step N.
#
# Needs Chrome or Chromium; run from the repository root:
#   scripts/render-mockups.sh
set -euo pipefail

dir=book/src/mockups
out=$dir/png
mkdir -p "$out"

chrome=""
for candidate in google-chrome google-chrome-stable chromium chromium-browser \
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  "/Applications/Chromium.app/Contents/MacOS/Chromium"; do
  if command -v "$candidate" >/dev/null 2>&1 || [ -x "$candidate" ]; then
    chrome=$candidate
    break
  fi
done
if [ -z "$chrome" ]; then
  echo "render-mockups: no Chrome or Chromium found" >&2
  exit 1
fi

for page in "$dir"/[0-9][0-9]-*.html; do
  name=$(basename "$page" .html)
  # A page's steps are its buttons in the .steps row; a page without one
  # (the window models) is a single picture.
  steps=$(grep -c '^<button>' "$page" || true)
  [ "$steps" -ge 1 ] || steps=1
  for step in $(seq 1 "$steps"); do
    png=$out/$name-$step.png
    [ "$steps" -eq 1 ] && png=$out/$name.png
    "$chrome" --headless=new --disable-gpu --hide-scrollbars \
      --force-color-profile=srgb --window-size=760,860 \
      --virtual-time-budget=4000 \
      --screenshot="$PWD/$png" "file://$PWD/$page#step=$step" >/dev/null 2>&1
    echo "$png"
  done
done
