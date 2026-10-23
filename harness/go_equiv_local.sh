#!/usr/bin/env bash
# go_equiv_local.sh — the gate's go arm, runnable on its own.
#
# Mirrors setup_backends.sh --backend-gate go EXACTLY:
#   * A1 emit from the CORE CLI, render through THIS worktree's shir_render;
#   * `bash -n` failure = SKIP (the gate skips bash-unparseable files);
#   * the translation runs `go run /tmp/x.go` and bash runs `bash "$f"`,
#     BOTH from the workspace root (go gets no eq_scratch in the gate);
#   * PASS = no stub marker, translation rc 0, bash rc 0, stdout identical
#     (a 124 timeout on either side is a failure).
#
# Running either side in a temp dir (as an earlier version did) measures a
# DIFFERENT file set — the corpus globs the CWD — and does not predict the
# gate. Keep both sides in $ROOT.
#
# Usage: harness/go_equiv_local.sh [-j N] [-v] [file ...]
#        (no files = sh2perl/examples/*.sh + frontends/*/testdata/*.sh)
set -uo pipefail
WT="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="${SH2LOOP_ROOT:-/mnt/wsl/projects/sh2loop}"
CLI="${OTRANSPILERL_BIN:-$ROOT/otranspilerl/target/debug/otranspilerl-cli}"
BIN="${GO_SHIR_RENDER:-$WT/target/debug/shir_render}"
CACHE="${GO_FAST_CACHE:-/mnt/wsl/tmpbtrfs/go-fast-cache}"
mkdir -p "$CACHE/gocache" "$CACHE/gomodcache" 2>/dev/null || CACHE=$(mktemp -d)
export GOCACHE="$CACHE/gocache" GOMODCACHE="$CACHE/gomodcache" GOFLAGS=-mod=mod
J="${J:-4}"
VERBOSE=0
FILES=()
while [ $# -gt 0 ]; do
    case "$1" in
    -j) J="$2"; shift 2 ;;
    -v) VERBOSE=1; shift ;;
    *) FILES+=("$1"); shift ;;
    esac
done
[ -x "$CLI" ] || { echo "missing $CLI" >&2; exit 1; }
[ -x "$BIN" ] || { echo "missing $BIN (cargo build --bin shir_render)" >&2; exit 1; }
if [ ${#FILES[@]} -eq 0 ]; then
    FILES=("$ROOT"/sh2perl/examples/*.sh "$ROOT"/sh2perl/frontends/*/testdata/*.sh)
fi

one() {
    local f="$1" d shir rc_eq rc_ref
    d=$(mktemp -d "${TMPDIR:-/tmp}/goeq.XXXXXX")
    if ! bash -n "$f" 2>/dev/null; then echo "SKIP-PARSE $f"; rm -rf "$d"; return; fi
    shir=$("$CLI" --target shir "$f" 2>/dev/null)
    if [ -z "$shir" ]; then echo "SKIP-EMIT $f"; rm -rf "$d"; return; fi
    if ! printf '%s' "$shir" | "$BIN" --target=go - > "$d/x.go" 2>/dev/null; then
        echo "FAIL-RENDER $f"; rm -rf "$d"; return
    fi
    if grep -qE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' "$d/x.go"; then
        echo "FAIL-STUB $f"; rm -rf "$d"; return
    fi
    ( cd "$ROOT" && timeout 30 go run "$d/x.go" > "$d/out" 2>/dev/null )
    rc_eq=$?
    ( cd "$ROOT" && timeout 15 bash "$f" > "$d/ref" 2>/dev/null )
    rc_ref=$?
    if [ "$rc_eq" = 124 ] || [ "$rc_ref" = 124 ]; then
        echo "FAIL-TIMEOUT $f"; rm -rf "$d"; return
    fi
    if [ "$rc_eq" != 0 ] || [ "$rc_ref" != 0 ] || ! diff -q "$d/out" "$d/ref" >/dev/null 2>&1; then
        echo "FAIL-EQ $f rc_eq=$rc_eq rc_ref=$rc_ref"
        if [ "$VERBOSE" = 1 ]; then
            diff "$d/out" "$d/ref" | head -20 | sed 's/^/    /'
        fi
        rm -rf "$d"; return
    fi
    echo "PASS $f"
    rm -rf "$d"
}
export -f one
export CLI BIN VERBOSE ROOT

out=$(mktemp)
printf '%s\n' "${FILES[@]}" | xargs -d '\n' -P "$J" -I{} bash -c 'one "$@"' _ {} > "$out" 2>&1
sort "$out" | awk '{print $1}' | uniq -c
echo "--- failures ---"
grep -v '^PASS' "$out" | sort
grep -q '^FAIL' "$out" && exit 1
rm -f "$out"
exit 0
