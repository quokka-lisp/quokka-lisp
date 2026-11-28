#!/usr/bin/env bash
# The browser pages, in a real browser with no window (#116, #142).
#
# scripts/smoke-wasm.mjs runs the bundle in Node: the engine, the bindings
# and the renderer's model. This runs the *pages*: they fetch the world, the
# pump gives the browser back between slices, and what a form prints lands
# in the document. The form goes in after `#` in the address. The dev
# console (console.html) evaluates it once the world is up; the environment
# (index.html, phase 4's gate) types it into the listener's editor and
# presses Enter, as a person would, and the renderer draws the answer. The
# browser is asked for each document afterwards and the answer is looked
# for in it.
#
#   wasm-pack build crates/quokka-wasm --target web --out-dir www/pkg
#   cargo run --release -p quokka -- build crates/quokka-wasm/www/world.qi
#   scripts/smoke-browser.sh
#
# Without a Chrome or Chromium it skips and says so, unless
# SMOKE_BROWSER_REQUIRED is set, as it is in CI.
set -euo pipefail

cd "$(dirname "$0")/.."

browser=""
for candidate in google-chrome google-chrome-stable chromium chromium-browser \
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  "/Applications/Chromium.app/Contents/MacOS/Chromium"; do
  if command -v "$candidate" >/dev/null 2>&1 || [ -x "$candidate" ]; then
    browser="$candidate"
    break
  fi
done
if [ -z "$browser" ]; then
  echo "smoke-browser: no Chrome or Chromium found; skipped"
  [ -z "${SMOKE_BROWSER_REQUIRED:-}" ] || exit 1
  exit 0
fi
for needed in crates/quokka-wasm/www/pkg/quokka_wasm.js crates/quokka-wasm/www/world.qi; do
  if [ ! -f "$needed" ]; then
    echo "smoke-browser: $needed is missing; the header of this script says how to make it" >&2
    exit 1
  fi
done

port=8791
python3 -m http.server --bind 127.0.0.1 -d crates/quokka-wasm/www "$port" >/dev/null 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT
sleep 1
# A port already taken means the server above did not start, and the test
# would be talking to whatever holds the port.
if ! kill -0 "$server" 2>/dev/null; then
  echo "smoke-browser: could not serve on port $port; is something else using it?" >&2
  exit 1
fi

# (list (+ 1 2) (expt 2 100) (mapcar #'1+ '(1 2 3))): arithmetic, a bignum,
# and a library function taking a function.
form="(list%20(%2B%201%202)%20(expt%202%20100)%20(mapcar%20%23'1%2B%20'(1%202%203)))"
wanted="(3 1267650600228229401496703205376 (2 3 4))"

# Virtual time: the browser runs the page's timers as fast as it can and
# stops when that much page time has passed, so the pump's slices all run.
for page_name in console.html index.html; do
  page=$("$browser" --headless=new --no-sandbox --disable-gpu \
    --virtual-time-budget=30000 --dump-dom \
    "http://localhost:$port/$page_name#$form" 2>/dev/null)
  if printf '%s' "$page" | grep -qF "$wanted"; then
    echo "smoke-browser: $page_name evaluated a form and printed $wanted"
  else
    echo "smoke-browser: $page_name did not print $wanted. Its body:" >&2
    printf '%s' "$page" | sed -n '/<body>/,$p' | head -n 30 >&2
    exit 1
  fi
done
