#!/bin/bash
# check_side_effects_overlay.sh — compare file-system side effects using fuse-overlayfs
# Usage: bash check_side_effects_overlay.sh <test.sh>

TEST="$1"
[ -n "$TEST" ] || { echo "Usage: $0 <test.sh>"; exit 1; }

SRC="$(cd "$(dirname "$0")" && pwd)"
OTRANS="$(cd "$SRC/.." && pwd)/otranspilerl/target/debug/otranspilerl-cli"
VIOLATIONS=0

for MODE in bash perl; do
    echo "=== $MODE ==="
    UPPER=$(mktemp -d /tmp/ov_up_XXXX)
    WORK=$(mktemp -d /tmp/ov_wk_XXXX)
    VIEW=$(mktemp -d /tmp/ov_vi_XXXX)

    fuse-overlayfs -o lowerdir="$SRC",upperdir="$UPPER",workdir="$WORK" "$VIEW" 2>/dev/null

    cd "$VIEW"

    if [ "$MODE" = "bash" ]; then
        bash "$TEST" 2>&1 || true
    else
        PERL_CODE=$("$OTRANS" --target pl "$VIEW/$TEST" 2>/dev/null)
        if [ -n "${PERL_CODE:-}" ]; then
            echo "$PERL_CODE" | timeout 30 perl - 2>&1 || true
        else
            echo "  (Perl generation returned no code)"
        fi
    fi

    CHANGES=$(find "$UPPER" \( -type f -o -type d -o -type p -o -type l \) -not -name ".*" 2>/dev/null | grep -v "^$UPPER$" | sed "s|$UPPER||" | sort -u || true)
    CHANGE_COUNT=$(echo "$CHANGES" | grep -c . 2>/dev/null || true)
    : "${CHANGE_COUNT:=0}"
    echo "  Files changed: $CHANGE_COUNT"
    [ "$CHANGE_COUNT" -gt 0 ] 2>/dev/null && echo "$CHANGES" | head -10 | sed 's/^/    /'

    eval "${MODE}_changes=\"\$CHANGES\""

    cd /tmp
    fusermount -u "$VIEW" 2>/dev/null || true
    sleep 0.2
    rm -rf "$UPPER" "$WORK" "$VIEW" 2>/dev/null || true
done

echo "=== Comparison ==="
BASH_FILE=$(mktemp /tmp/bc_XXXX)
PERL_FILE=$(mktemp /tmp/pc_XXXX)
echo "$bash_changes" | sort -u > "$BASH_FILE"
echo "$perl_changes" | sort -u > "$PERL_FILE"

BASH_ONLY=$(comm -23 "$BASH_FILE" "$PERL_FILE" | grep -c . || true)
PERL_ONLY=$(comm -13 "$BASH_FILE" "$PERL_FILE" | grep -c . || true)
: "${BASH_ONLY:=0}"
: "${PERL_ONLY:=0}"

[ "$BASH_ONLY" -gt 0 ] 2>/dev/null && echo "  Bash-only:" && comm -23 "$BASH_FILE" "$PERL_FILE" | head -10 | sed 's/^/    /'
[ "$PERL_ONLY" -gt 0 ] 2>/dev/null && echo "  Perl-only:" && comm -13 "$BASH_FILE" "$PERL_FILE" | head -10 | sed 's/^/    /'

if [ "$BASH_ONLY" = 0 ] && [ "$PERL_ONLY" = 0 ]; then
    echo "  OK: side effects match"
else
    echo "  FAIL: side effects differ"
    VIOLATIONS=1
fi

rm -f "$BASH_FILE" "$PERL_FILE" 2>/dev/null
exit $VIOLATIONS
