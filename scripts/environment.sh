#!/usr/bin/env bash
# Build and serve the environment, phase 4's listener in the browser (#142).
#
#   scripts/environment.sh            build everything, serve on port 8080, open it
#   scripts/environment.sh 9000       the same on another port
#   scripts/environment.sh --no-build serve what was built last time
#
# It runs the three commands of docs/manual.md, "In the browser": build a
# world from the Lisp sources on disk, bundle the machine for the browser
# with wasm-pack, and serve crates/quokka-wasm/www. The server tells the
# browser not to cache anything, so a reload after a rebuild gets the new
# world and scripts (python's plain http.server would let it keep old ones).
# Ctrl-C stops the server.
set -euo pipefail

cd "$(dirname "$0")/.."

build=1
port=8080
for arg in "$@"; do
  case "$arg" in
    --no-build) build=0 ;;
    -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *[!0-9]*|'') echo "usage: $0 [--no-build] [PORT]" >&2; exit 2 ;;
    *) port=$arg ;;
  esac
done

www=crates/quokka-wasm/www

if [ "$build" = 1 ]; then
  # The rustup toolchain first, as check.sh does: a Homebrew cargo earlier on
  # PATH has no wasm32 std and fails with "can't find crate for `core`".
  if command -v rustup >/dev/null 2>&1; then
    PATH="$(dirname "$(rustup which cargo)"):$PATH"
    export PATH
  fi
  if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "wasm-pack is missing: cargo install wasm-pack" >&2
    exit 1
  fi
  echo "== building the world (the Lisp sources are compiled into the binary)"
  cargo run --quiet --release -p quokka -- build "$www/world.qi"
  echo "== bundling the machine for the browser"
  wasm-pack --quiet build crates/quokka-wasm --target web --out-dir www/pkg
elif [ ! -f "$www/world.qi" ] || [ ! -d "$www/pkg" ]; then
  echo "nothing built yet: run $0 without --no-build first" >&2
  exit 1
fi

url="http://localhost:$port"
echo "== serving $www at $url (Ctrl-C to stop)"

# Open the page once the server answers, in the background.
(
  for _ in $(seq 50); do
    if curl -fs -o /dev/null "$url/"; then
      if command -v open >/dev/null 2>&1; then open "$url"
      elif command -v xdg-open >/dev/null 2>&1; then xdg-open "$url"
      fi
      exit 0
    fi
    sleep 0.2
  done
) &

exec python3 - "$port" "$www" <<'PY'
import functools, http.server, sys

class NoCache(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass

port, root = int(sys.argv[1]), sys.argv[2]
server = http.server.ThreadingHTTPServer(("", port), functools.partial(NoCache, directory=root))
try:
    server.serve_forever()
except KeyboardInterrupt:
    pass
PY
