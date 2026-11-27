#!/usr/bin/env bash
# Render every step of every environment mock-up (book/src/mockups/*.html)
# to a PNG in book/src/mockups/png/, for pages and printouts where the
# interactive version cannot run. Each page opens at #step=N on step N.
#
# Needs Chrome or Chromium; run from the repository root:
#   scripts/render-mockups.sh [PAGE.html ...]
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

# Every page, or only the pages named on the command line.
if [ $# -gt 0 ]; then
  pages=("$@")
else
  pages=("$dir"/[0-9][0-9]-*.html)
fi
for page in "${pages[@]}"; do
  name=$(basename "$page" .html)
  # A page's steps are its buttons in the .steps row; a page without one
  # (the window models) is a single picture.
  steps=$(grep -c '^<button>' "$page" || true)
  [ "$steps" -ge 1 ] || steps=1
  # A tutorial's figures say their size in <meta name="figure-size"
  # content="WxH ...">, one size per step (the last one serves the rest),
  # and are rendered without the page around the screen.
  sizes=($(sed -n 's/.*<meta name="figure-size" content="\([0-9x ]*\)".*/\1/p' "$page"))
  for step in $(seq 1 "$steps"); do
    if [ ${#sizes[@]} -gt 0 ]; then
      size=${sizes[$((step - 1))]:-${sizes[$((${#sizes[@]} - 1))]}}
      window=${size/x/,}
      mode="&figure"
    else
      window=760,860
      mode=""
    fi
    png=$out/$name-$step.png
    [ "$steps" -eq 1 ] && png=$out/$name.png
    "$chrome" --headless=new --disable-gpu --hide-scrollbars \
      --force-color-profile=srgb --window-size="$window" \
      --virtual-time-budget=4000 \
      --screenshot="$PWD/$png" "file://$PWD/$page#step=$step$mode" >/dev/null 2>&1
    echo "$png"
  done
done
