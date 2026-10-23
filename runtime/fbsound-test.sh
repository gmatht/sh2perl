#!/bin/sh
# fbsound-test.sh — run the sound device's tests, under BOTH toolchains.
#
# The device is compiled by tcc on the bash-O4 path, so a suite that
# only passes under cc is not evidence of anything. Both must be green.
#
# IT IS ALSO RUN TWICE PER TOOLCHAIN, and the second run is the one that
# matters. A box with no sound card takes the headless path, where every
# fb_note and fb_samples returns 0 — and a test that only ever sees that
# cannot tell "the device parsed the payload" from "the device ignored
# it". With ALSA_CONFIG_PATH pointing at a `type null` device the audio
# library OPENS, so the open path, the counting and the parse refusals
# are all live. That is not a sound card and it produces no audible
# output — a null device rejects every write with EPIPE, measured — but
# it exercises everything above snd_pcm_writei, which is where the
# parsing bugs were.
set -e
cd "$(dirname "$0")"

rc=0
for CC in cc tcc; do
    # shellcheck disable=SC2086
    if ! $CC -Wall -I. -o /tmp/fbsound-test-$CC fbsound-test.c fbsound.c -lm -ldl 2>/dev/null; then
        echo "== $CC: FAILED TO BUILD =="
        rc=1
        continue
    fi
    for MODE in headless card; do
        echo "== $CC / $MODE =="
        conf=""
        if [ "$MODE" = card ]; then
            conf=/tmp/fbsound-null.conf
            cat > "$conf" <<'EOF'
pcm.!default { type null }
ctl.!default { type null }
EOF
        fi
        if [ -n "$conf" ]; then
            ALSA_CONFIG_PATH="$conf" /tmp/fbsound-test-$CC 2>/dev/null || rc=1
        else
            /tmp/fbsound-test-$CC 2>/dev/null || rc=1
        fi
    done
done
if [ "$rc" -eq 0 ]; then
    echo "== fbsound: all green =="
else
    echo "== fbsound: FAILURES =="
fi
exit $rc
