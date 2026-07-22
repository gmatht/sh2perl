#!/bin/bash
# check_side_effects_overlay.sh — compare file-system side effects using overlayfs
# Usage: sudo check_side_effects_overlay.sh <test.sh>
#
# Sets up an overlay mount over the sh2perl directory, runs the test (bash then Perl),
# and compares what files each version creates/modifies/deletes in the working directory.

set -e

if [ "$EUID" -ne 0 ]; then
    echo "This script needs root for overlayfs mounts. Run with: sudo $0 <test.sh>"
    exit 1
fi

TEST="$1"
if [ -z "$TEST" ]; then
    echo "Usage: $0 <test.sh>"
    exit 1
fi

SRC="/nvme/ai/sh2loop/sh2perl"
DEBASHC="$SRC/target/debug/debashc"
BASENAME=$(basename "$TEST" .sh)
VIOLATIONS=0

for MODE in bash perl; do
    echo "=== $MODE ==="

    UPPER=$(mktemp -d /tmp/ov_upper_XXXX)
    WORK=$(mktemp -d /tmp/ov_work_XXXX)
    VIEW=$(mktemp -d /tmp/ov_view_XXXX)

    mount -t overlay overlay -o lowerdir="$SRC",upperdir="$UPPER",workdir="$WORK" "$VIEW"

    if [ "$MODE" = "bash" ]; then
        cd "$VIEW"
        bash "$VIEW/$TEST" 2>&1 || true
    else
        # Generate Perl code
        cd "$SRC"
        PERL_CODE=$($DEBASHC parse --perl "$SRC/$TEST" 2>/dev/null | sed -n '/^#!/,/^===/p' | head -n -1)
        cd "$VIEW"
        echo "$PERL_CODE" > /tmp/test_$$.pl
        perl /tmp/test_$$.pl 2>&1 || true
        rm -f /tmp/test_$$.pl
    fi

    # List changes in upper layer
    CHANGES=$(find "$UPPER" -type f -not -path "*/\.*" | sed "s|$UPPER||")
    CHANGE_COUNT=$(echo "$CHANGES" | grep -c . || true)
    echo "  Changes: $CHANGE_COUNT"
    if [ $CHANGE_COUNT -gt 0 ]; then
        echo "$CHANGES" | head -10 | sed 's/^/    /'
    fi

    eval "${MODE}_changes=\"$CHANGES\""
    eval "${MODE}_count=$CHANGE_COUNT"

    cd /
    umount "$VIEW"
    rm -rf "$UPPER" "$WORK" "$VIEW"
done

echo "=== Comparison ==="
# Compare using comm
BASH_FILE=$(mktemp)
PERL_FILE=$(mktemp)
echo "$bash_changes" | sort > "$BASH_FILE"
echo "$perl_changes" | sort > "$PERL_FILE"

BASH_ONLY=$(comm -23 "$BASH_FILE" "$PERL_FILE" | grep -c . || true)
PERL_ONLY=$(comm -13 "$BASH_FILE" "$PERL_FILE" | grep -c . || true)

if [ "$BASH_ONLY" -gt 0 ]; then
    echo "  Bash-only changes:"
    comm -23 "$BASH_FILE" "$PERL_FILE" | head -10 | sed 's/^/    /'
fi
if [ "$PERL_ONLY" -gt 0 ]; then
    echo "  Perl-only changes:"
    comm -13 "$BASH_FILE" "$PERL_FILE" | head -10 | sed 's/^/    /'
fi

if [ "$BASH_ONLY" -eq 0 ] && [ "$PERL_ONLY" -eq 0 ]; then
    echo "  OK: side effects match"
else
    echo "  FAIL: side effects differ ($BASH_ONLY bash-only, $PERL_ONLY perl-only)"
    VIOLATIONS=1
fi

rm -f "$BASH_FILE" "$PERL_FILE"
exit $VIOLATIONS
