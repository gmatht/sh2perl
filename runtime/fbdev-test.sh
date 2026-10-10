#!/bin/sh
# fbdev-test.sh — run the fb0 device's tests, under BOTH toolchains.
#
# The device is compiled by tcc on the bash-O4 path, so a suite that
# only passes under cc is not evidence of anything. Both must be green.
#
# The silhouette check is NOT in the C suite: the C suite writes its
# 112 cases to /tmp/fbsil and tools/fbdev-silhouette.py (an oracle
# written independently) diffs them. Three C oracles were tried first
# and every one of them was the broken half.
set -e
cd "$(dirname "$0")"

rc=0

# FACE_N (the back-face-cull normals) is a second statement about the
# cube geometry CUBE_P/CUBE_I already carry, and a wrong one removes
# VISIBLE faces rather than drawing bad ones — the silhouette oracle
# catches it, but only after a full build and 112 renders. This is the
# check that says so directly, and it runs before either.
if ! python3 tools/fbdev-faces.py fbdev.c; then rc=1; fi

for CC in cc tcc; do
    echo "== $CC =="
    rm -f /tmp/fbdev-test-$CC
    # shellcheck disable=SC2086
    # fbwin.c is on the line because fbdev.c now CALLS it: fb_present
    # blits to the window and fb_window_* asks about it, so the fb0
    # rasterizer suite links the window half too. The suite still runs
    # with FBWIN=0, so no window is opened here — the window's own
    # tests are in fbwin-test.sh, which needs a DISPLAY.
    $CC -Wall -O2 fbdev.c fbwin.c fbdev-test.c -lm -o /tmp/fbdev-test-$CC
    rm -rf /tmp/fbsil && mkdir -p /tmp/fbsil
    /tmp/fbdev-test-$CC || rc=1
    if [ -f /tmp/fbsil/cases.txt ]; then
        python3 tools/fbdev-silhouette.py /tmp/fbsil/cases.txt /tmp/fbsil || rc=1
    fi
done
[ "$rc" -eq 0 ] && echo "== fbdev: all green ==" || echo "== fbdev: FAILURES =="
exit $rc
