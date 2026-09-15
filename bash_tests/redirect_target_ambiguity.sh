#!/usr/bin/env bash
# ─── redirect_target_ambiguity.sh — unquoted $(…) as a redirect target ──
#
# Found while building mimecroft (the sh2runtime project): a 130-byte junk
# file appeared in the repo root, named after a fragment of shader source
#
#   $(echo "scale=4; 0.0 - $relz + 0.0" | bc)\n  if [ "$(echo ...)" = "1" ]; \
#     then w=0.0001; fi|
#
# with the contents `   bc)| bc)` — the stdout of two `bc` runs. The name
# is a bash float capture glued to the shell-syntax form of a GLSL clamp
# that www/bin/mimecroft.sh injects at line 1307:
#
#   glsl=${glsl/g_w = …;/g_w = …; if (g_w < 0.0001) g_w = 0.0001;}
#
# The `\n` in that name is a LITERAL backslash-n, never a real newline:
# the fingerprint of the text being re-escaped at least twice on its way
# into a filename. The spaces in it make it MANY words.
#
# bash's contract for a redirect target built from a command substitution
# is exact, and word splitting is the whole point:
#
#   ONE resulting word  → the redirect happens (file created)
#   ZERO or MANY words  → `ambiguous redirect`, status 1, NO file created
#
# The JS backend currently gets this wrong for EVERY case. `--target js`
# lowers the whole statement to a TODO marker:
#
#   /* TODO(unsupported): stmt Redirect { … IrRedirect { fd: Some(1),
#      mode: "w", target: Call { func: "captureWords", … } } } */
#
# so the program prints NOTHING, creates NOTHING, and exits 0 — silently
# ignoring a statement that bash executes. The `captureWords` target is
# the tell: it is the word-SPLITTER (it returns a list), so it cannot be
# used as a path without collapsing a one-word list and rejecting an
# ambiguous one.
#
# The `sh` backend renders `echo hi >$(echo alpha)` — correct — so this is
# JS-target specific, not a parse failure.
#
# Fix belongs in the emitter: unwrap a ONE-word list to its single word,
# and make ZERO-or-MANY raise `ambiguous redirect` with status 1 WITHOUT
# touching the filesystem. A permissive path here is what let a multi-word
# expansion write a file named after shader source instead of failing.
#
# Usage: bash bash_tests/redirect_target_ambiguity.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OTR="${OTRANSPILER:-}"
for c in "$OTR" "$ROOT/target/release/otranspiler" "$ROOT/target/debug/otranspiler"; do
  [ -n "$c" ] && [ -x "$c" ] && OTR="$c" && break
done
[ -n "$OTR" ] && [ -x "$OTR" ] || { echo "SKIP: otranspiler not built (cargo build --release --bin otranspiler)"; exit 0; }

RUNNER="$ROOT/harness/js-runner.mjs"
[ -f "$RUNNER" ] || { echo "SKIP: $RUNNER missing"; exit 0; }

# the sh2 runtime namespace the runner wraps the program with
NS="${SH2_NAMESPACE:-/home/llm/sh2loop/harness/sh2-namespace.mjs}"
[ -f "$NS" ] || NS="$ROOT/harness/sh2-namespace.mjs"
[ -f "$NS" ] || { echo "SKIP: sh2-namespace.mjs not found (set SH2_NAMESPACE)"; exit 0; }

TMP=/tmp/rtat; rm -rf "$TMP"; mkdir -p "$TMP"
pass=0; fail=0; skip=0

# check <name> <source> <expect>
#   expect = file:<name>  → bash creates it (so must we)
#          = reject       → bash refuses, status≠0, no file (so must we)
check() {
  local name="$1" body="$2" expect="$3"
  local d="$TMP/$name"; mkdir -p "$d/bash"
  printf '%s\n' "$body" > "$d/in.sh"
  cp "$d/in.sh" "$d/bash/in.sh"

  # ── real bash, in its own directory ──
  ( cd "$d/bash" && timeout 20 bash in.sh ) >"$d/bash.out" 2>"$d/bash.err"
  local bst=$?
  local bfiles; bfiles="$(cd "$d/bash" && ls -A 2>/dev/null | grep -v '^in\.sh$' | sort | tr '\n' ' ')"

  # ── the JS target ──
  if ! timeout 30 "$OTR" "$d/in.sh" - --target js >"$d/prog.js" 2>"$d/otr.err"; then
    echo "  FAIL $name — otranspiler --target js failed: $(head -1 "$d/otr.err")"
    fail=$((fail+1)); return
  fi
  # the emitter prefixes a node shebang, which the ESM runner rejects
  sed '1{/^#!/d;}' "$d/prog.js" > "$d/prog.mjs"
  ( cd "$d" && timeout 20 node "$RUNNER" "$d/prog.mjs" --source "$d/in.sh" --ns "$NS" ) \
    >"$d/got.out" 2>"$d/got.err"
  local gst=$?
  local got; got="$(cat "$d/got.out" "$d/got.err" 2>/dev/null)"
  local gfiles; gfiles="$(cd "$d" && ls -A 2>/dev/null \
      | grep -v -e '^in\.sh$' -e '^bash$' -e '^prog\.' -e '^otr\.err$' -e '^got\.' -e '^bash\.' \
      | sort | tr '\n' ' ')"

  if [ "$expect" = "reject" ]; then
    [ "$bst" -ne 0 ] || { echo "  ?? $name: bash unexpectedly ACCEPTED"; skip=$((skip+1)); return; }
    if [ -z "$gfiles" ] && { [ "$gst" -ne 0 ] || echo "$got" | grep -q 'ambiguous redirect'; }; then
      pass=$((pass+1)); echo "  ok   $name"
    else
      fail=$((fail+1))
      echo "  FAIL $name — bash rejects (status $bst, no file); we did not"
      echo "        transpiled: status=$gst files=[$gfiles] out=$(printf '%s' "$got" | head -c 100)"
    fi
  else
    local want="${expect#file:}"
    [ -n "$bfiles" ] || { echo "  ?? $name: bash created nothing"; skip=$((skip+1)); return; }
    if echo " $gfiles " | grep -q " $want " ; then
      pass=$((pass+1)); echo "  ok   $name"
    else
      fail=$((fail+1))
      echo "  FAIL $name — bash creates '$want'; we created [$gfiles]"
      echo "        transpiled: status=$gst out=$(printf '%s' "$got" | head -c 100)"
    fi
  fi
}

echo "== unquoted \$(…) redirect targets: JS target vs real bash =="

# 1. ONE word — bash executes the redirect and creates the file.
check one-word   'echo hi > $(echo onepiece)'        file:onepiece

# 2. TWO words — bash: `ambiguous redirect`, status 1, NO file.
check two-words  'echo hi > $(echo alpha beta)'      reject

# 3. ZERO words — bash: `ambiguous redirect`, status 1, NO file.
check zero-words 'echo hi > $(true)'                 reject

# 4. The junk-name SHAPE — embedded spaces + a literal \n → many words,
#    so bash rejects it outright and never writes such a file.
check junk-shape "echo hi > \$(printf 'a\\nb c\\n')" reject

echo
echo "redirect-target ambiguity: pass=$pass fail=$fail skip=$skip"
[ "$fail" -eq 0 ] || exit 1
