#!/usr/bin/env bash
# tests/parity_vs_bash.sh — UU-FFI.md §7.5 correctness gate.
#
# For a set of genuinely-external commands, run the SAME argv through
# (a) the uu-ffi in-process runtime (via a small C harness) and (b) the
# real GNU coreutils fork/exec, and assert byte-identical stdout + same
# exit code. A divergence is a bug in the uu-ffi integration, not something
# to bless.
#
# Usage: [CC=tcc] tests/parity_vs_bash.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIBDIR="$ROOT/lib"
CC="${CC:-tcc}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

# Build the harness: sh2_uu_capture(argv[1..]) — argv[0] is the program name,
# argv[1] must be the util name. It prints payload to stdout, "RC <code>" to
# stderr.
cat > "$TMP/harness.c" <<'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "uu_run.h"
int main(int argc, char **argv) {
    size_t cap = 64 << 20; char *buf = malloc(cap);
    size_t n = 0;
    int rc = sh2_uu_capture(argc - 1, argv + 1, buf, cap, &n);
    fwrite(buf, 1, n, stdout);
    fprintf(stderr, "RC %d\n", rc);
    return 0;
}
EOF
$CC -I"$LIBDIR" -I"$ROOT" -o "$TMP/harness" "$TMP/harness.c" "$ROOT/uu_run.c" \
  "$LIBDIR/libcoreutils_ffi.so" -lpthread -Wl,-rpath,"$LIBDIR" 2>&1 | head -5 \
  || { echo "harness build failed"; exit 2; }

WORK="$(mktemp -d)"; trap 'rm -rf "$TMP" "$WORK"' EXIT
printf 'banana\nAPPLE\ncherry\napple\n' > "$WORK/f.txt"

fails=0; total=0

# Each case is a bash array: ( util arg... ) — run identically via harness
# and via real coreutils (fork/exec), compare stdout bytes + exit code.
run_case() {
  local args=("$@")
  local util="${args[0]}"
  local -a harness_args=("${args[@]}")
  # harness argv: program, util, args...
  local out_u rc_u out_r rc_r
  out_u="$("$TMP/harness" "${harness_args[@]}" 2>"$TMP/u.err")"; rc_u=$?
  # harness rc = RC line on stderr
  rc_u="$(sed -n 's/^RC //p' "$TMP/u.err" | tail -1)"
  # real coreutils: run the util via the real binaries (fork/exec)
  out_r="$("${args[@]}")"; rc_r=$?
  total=$((total+1))
  if [ "$out_u" != "$out_r" ] || [ "$rc_u" != "$rc_r" ]; then
    fails=$((fails+1))
    echo "  PARITY FAIL: ${args[*]}"
    echo "    uu:   rc=$rc_u out=${#out_u}B"
    echo "    real: rc=$rc_r out=${#out_r}B"
    printf '    uu out:    %q\n' "$out_u"
    printf '    real out:  %q\n' "$out_r"
  fi
}

run_case sort   "$WORK/f.txt"
run_case grep   apple "$WORK/f.txt"          # no match line numbers (GNU default)
run_case grep   -i apple "$WORK/f.txt"
run_case wc     -l "$WORK/f.txt"
run_case sed    s/apple/APPLE/ "$WORK/f.txt"
run_case cat    "$WORK/f.txt"
run_case awk    '{print NR ":" $1}' "$WORK/f.txt"

# KNOWN-LIMITATION check: the uu-ffi grep frontend does not implement `-x`
# (whole-line) or option clustering (`-ix`). It must FAIL LOUDLY (rc 2,
# "unsupported flag") rather than emit silently-wrong output. A backend that
# lowers a grep to uu-ffi MUST first confirm the exact argv form is supported;
# anything else must keep fork/exec. See docs/UU-FFI.md §5 and the grep gap.
# This assertion proves the refusal is loud, which is the correct runtime
# behavior — never silence it.
total=$((total+1))
ou="$( "$TMP/harness" grep -ix APPLE "$WORK/f.txt" 2>"$TMP/u.err" )"
rcu="$(sed -n 's/^RC //p' "$TMP/u.err" | tail -1)"
if [ "$rcu" = "2" ] && ! echo "$ou" | grep -q .; then
  : # loud refusal, no output — correct
else
  fails=$((fails+1))
  echo "  LIMITATION NOT LOUD: grep -ix APPLE → rc=$rcu out=${#ou}B (must refuse rc 2, no output)"
fi

echo "UU-FFI parity: $total cases, $fails divergences from real coreutils"
[ "$fails" -eq 0 ]
