#!/bin/sh
# fbwin-test.sh — the fb0 WINDOW device's tests.
#
# Run separately from fbdev-test.sh because this one needs a DISPLAY,
# and a device test that silently skips when there is no display is a
# device test that reports green without having tested anything. When
# there is no display the window tests are SKIPPED and said so; the
# fb0 rasterizer tests are in fbdev-test.sh and run regardless.
#
# TWO TOOLCHAINS, same as the fb0 device: the device is compiled by tcc
# on the bash-O4 path, so a suite that only passes under cc is not
# evidence of anything.
#
# THE ORACLE IS PART OF THE SUITE AND IS NOT OPTIONAL. fbwin-test.c
# checks the RAM surface with the device's own fb_dump_ppm, which proves
# the drawing worked; it cannot prove the BLIT worked, because
# fb_dump_ppm reads the same buffer the blit reads. fbwin-oracle.c is a
# separate program, linked against the real X11 headers, which finds the
# window on the server by NAME and reports the pixels that are actually
# there. Without it this suite would have passed while every frame went
# nowhere — which is exactly what happened during development, and it is
# the failure the whole device exists to prevent.
set -e
cd "$(dirname "$0")"

rc=0
CC_BIN=${CC:-cc}
ORACLE=/tmp/fbwin-oracle-$$

if [ -z "${DISPLAY:-}" ]; then
    echo "== fbwin: SKIPPED (no DISPLAY; nothing to open a window on) =="
    echo "== fbwin: the fb0 rasterizer tests are in fbdev-test.sh =="
    exit 0
fi

for CC in cc tcc; do
    command -v "$CC" > /dev/null 2>&1 || continue
    echo "== fbwin / $CC =="
    # shellcheck disable=SC2086
    $CC -Wall -O2 fbdev.c fbwin.c fbwin-test.c -lm -o "/tmp/fbwin-test-$CC-$$"
    # the oracle is built ONCE with the host compiler: it is a check,
    # not a device, and it must not be the thing under test
    [ -x "$ORACLE" ] || cc -Wall -O1 fbwin-oracle.c -o "$ORACLE" -lX11
    "/tmp/fbwin-test-$CC-$$" /tmp/fbwin-test-$$.ppm "$ORACLE" || rc=1
    rm -f "/tmp/fbwin-test-$CC-$$" /tmp/fbwin-test-$$.ppm
done

# THE HAND-DECLARED STRUCT, CHECKED AGAINST X11'S OWN.
#
# fbwin.c cannot include <X11/Xlib.h> — a framebuffer box has no X11
# headers, and a device that needs one to compile is not a fallback —
# so XSetWindowAttributes is declared by hand. A hand-declared X11
# struct is only as good as the number of fields somebody remembered,
# and a wrong one is SILENT: it still compiles, still opens a window,
# and the one flag the device's design rests on (override_redirect,
# field 14) simply lands at the wrong offset. That is not a theory —
# the first version of the declaration was missing `event_mask` and put
# override_redirect at byte 80 where X11 puts it at 88, and nothing
# complained until the attributes were compared.
#
# So this is checked on every run, and it needs the real header, which
# means it is NOT part of the tcc/cc loop above: this is a check on the
# source, not a build of the device.
if command -v cc > /dev/null 2>&1 && [ -f /usr/include/X11/Xlib.h ]; then
    echo "== fbwin / XSetWindowAttributes layout =="
    cc -Wall -O1 fbwin-attr-layout.c -o /tmp/fbwin-attr-layout-$$ -lX11 \
        && /tmp/fbwin-attr-layout-$$ || rc=1
    rm -f /tmp/fbwin-attr-layout-$$
else
    echo "== fbwin: attribute layout SKIPPED (no X11 headers to check against)"
fi

rm -f "$ORACLE"
[ "$rc" -eq 0 ] && echo "== fbwin: all green ==" || echo "== fbwin: FAILURES =="
exit $rc
