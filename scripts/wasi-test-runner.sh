#!/usr/bin/env bash
# Run a test binary built for wasm32-wasip1, on every core.
#
# Cargo hands a test binary to this script (CARGO_TARGET_WASM32_WASIP1_RUNNER
# in scripts/check.sh). A WASI program has no threads, so Rust's test
# harness, which natively runs tests on every core, runs them one after
# another inside one wasmtime process: the suite took six minutes on a
# twelve-core machine, on one of the twelve. So this asks the binary for its
# list of tests, deals them out into shares, and runs a wasmtime for each
# share at the same time. It is *sharding*: the tests are independent, so
# which process runs which does not matter.
#
# One group is kept together. The rebuild and fixpoint tests share two
# rebuilt worlds, made once and remembered for the process; dealt out to
# different processes each would make its own, and the sharing is the
# larger saving.
set -uo pipefail

binary="$1"
shift

run() {
  wasmtime --dir=. --dir="$QUOKKA_ROOT" \
    ${QUOKKA_FOREIGN_IMAGE:+--env "QUOKKA_FOREIGN_IMAGE=$QUOKKA_FOREIGN_IMAGE"} \
    ${QUOKKA_FOREIGN_IMAGE_REQUIRED:+--env "QUOKKA_FOREIGN_IMAGE_REQUIRED=$QUOKKA_FOREIGN_IMAGE_REQUIRED"} \
    "$binary" "$@"
}

# Asked for something in particular (a filter, --list, --ignored): do as
# asked, in one process. `--quiet`, which `cargo test -q` passes along, asks
# for nothing in particular.
for argument in "$@"; do
  case "$argument" in
    -q|--quiet) ;;
    *)
      run "$@"
      exit $?
      ;;
  esac
done

tests=$(run --list --format terse 2>/dev/null | sed -n 's/: test$//p')
count=$(printf '%s\n' "$tests" | grep -c . || true)
shares=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
[ "$shares" -gt 8 ] && shares=8

# Too few to be worth dealing out.
if [ "$count" -lt $((shares * 2)) ]; then
  run
  exit $?
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

together='rebuil|fixpoint|reproduces'
printf '%s\n' "$tests" | grep -E "$together" > "$work/names.0" || true
printf '%s\n' "$tests" | grep -vE "$together" | awk -v n="$shares" -v dir="$work" \
  '{ print > (dir "/names." (NR % n + 1)) }' || true

pids=()
for names in "$work"/names.*; do
  [ -s "$names" ] || continue
  share="${names##*.}"
  # --exact, so that a test named `a` does not also run `a_b`.
  # shellcheck disable=SC2046
  ( run --exact $(cat "$names") > "$work/out.$share" 2>&1; echo $? > "$work/status.$share" ) &
  pids+=($!)
done
wait "${pids[@]}"

status=0
passed=0
ignored=0
for result in "$work"/status.*; do
  share="${result##*.}"
  if [ "$(cat "$result")" != 0 ]; then
    status=1
    cat "$work/out.$share"
  fi
  line=$(grep '^test result:' "$work/out.$share" | tail -n 1)
  passed=$((passed + $(printf '%s' "$line" | sed -n 's/.* \([0-9]*\) passed.*/\1/p' | grep . || echo 0)))
  ignored=$((ignored + $(printf '%s' "$line" | sed -n 's/.* \([0-9]*\) ignored.*/\1/p' | grep . || echo 0)))
done

if [ "$status" -eq 0 ]; then
  echo "test result: ok. $passed passed; 0 failed; $ignored ignored; in $(ls "$work"/status.* | wc -l | tr -d ' ') wasmtime processes"
  if [ "$((passed + ignored))" -ne "$count" ]; then
    echo "wasi-test-runner: $count tests were listed and $((passed + ignored)) ran" >&2
    status=1
  fi
else
  echo "test result: FAILED, in one or more of the processes above" >&2
fi
exit $status
