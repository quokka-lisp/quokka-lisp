#!/usr/bin/env bash
# The dev console, in a real browser with no window (#116).
#
# scripts/smoke-wasm.mjs runs the bundle in Node: the engine and the
# bindings. This runs the *page*: index.html fetches the world, the pump
# gives the browser back between slices, and what a form prints lands in the
# document. The form goes in after `#` in the address, which the page
# evaluates once the world is up; the browser is asked for the document
# afterwards and the answer is looked for in it.
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
python3 -m http.server -d crates/quokka-wasm/www "$port" >/dev/null 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT
sleep 1

# (list (+ 1 2) (expt 2 100) (mapcar #'1+ '(1 2 3))): arithmetic, a bignum,
# and a library function taking a function.
form="(list%20(%2B%201%202)%20(expt%202%20100)%20(mapcar%20%23'1%2B%20'(1%202%203)))"
wanted="(3 1267650600228229401496703205376 (2 3 4))"

# Virtual time: the browser runs the page's timers as fast as it can and
# stops when that much page time has passed, so the pump's slices all run.
page=$("$browser" --headless=new --no-sandbox --disable-gpu \
  --virtual-time-budget=30000 --dump-dom \
  "http://localhost:$port/index.html#$form" 2>/dev/null)

if printf '%s' "$page" | grep -qF "$wanted"; then
  echo "smoke-browser: the page evaluated a form and printed $wanted"
else
  echo "smoke-browser: the page did not print $wanted. Its transcript:" >&2
  printf '%s' "$page" | sed -n '/id="transcript"/,/<\/pre>/p' | head -n 20 >&2
  exit 1
fi
