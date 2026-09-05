#!/bin/bash
# grep -P forms — every PCRE usage shape the grep-pcre transform must
# either translate to portable `grep -E` (POSIX ERE) or keep as `grep -P`
# (refuse > guess). Forms sourced from Armbian's sh.new files:
#   functions.sh / softy:   ip -4 route ls | grep default | tail -1 |
#                             grep -Po '(?<=dev )(\S+)'
#   jobs.sh:                ... | grep -Po '(?<=dev )(\S+)' |
#                             grep -v vpn_se | head -1
#   snap-debug-info.sh:     snap changes | tail -n +2 |
#                             grep -Po '(?:[0-9]+\s+Doing|Error)' | awk '{print $1}'
#   softy:                  [[ "$(echo $HOSTNAMEFQDN | grep -P
#                             '(?=^.{1,254}$)(^(?>(?!\d+\.)[a-zA-Z0-9_\-]{1,63}\.?)+(?:[a-zA-Z]{2,})$)')" == "" ]]
#   memtester.sh / terminateProcess.sh: pgrep -P — parent-PID matching,
#                             NOT a regex form; grep-pcre must not mistake
#                             the -P for a PCRE flag (the kill -9 from the
#                             original is deliberately omitted — no corpus
#                             example may kill processes).
# Deterministic: fixed /tmp inputs, no real ip/snap state (the pgrep test
# asserts only that the *shape* survives translation — the name pattern
# never matches, so the output is stable).
#
# Expected translations pin the transform:
#   • lookbehind + -o inside $(…) → `grep -Eo 'dev …' | sed 's/^dev //'`
#   • (?:…) + \s + -o → `grep -Eo '([0-9]+[[:space:]]+Doing)'`
#   • lookahead/atomic/negative-lookahead FQDN pattern → KEPT as grep -P
#   • pgrep -P → untouched
# stdout + exit code are byte-identical to bash (verified); the check_qx
# verdict on this example is the PERL BACKEND's pipeline wrapping gap
# (grep/sed/cat execs shell out through bash -c — the same pre-existing
# verdict as the 015..019 grep examples), not the grep-pcre translation.

cd /tmp

# ── input fixtures (what `ip -4 route ls | grep default` would produce) ──
printf 'default via 10.0.0.1 dev eth0 proto dhcp src 10.0.0.1 metric 100\n' > grep_p_routes.txt
printf 'default via 10.7.7.7 dev vpn_se proto static\n' > grep_p_routes2.txt

echo "-- functions.sh / softy: lookbehind -o extraction --"
DEFAULT_ADAPTER=$(grep default grep_p_routes.txt | tail -1 | grep -Po '(?<=dev )(\S+)')
echo "adapter=[$DEFAULT_ADAPTER]"

echo "-- jobs.sh: lookbehind -o + grep -v + head -1 --"
ADAPTER=$(grep default grep_p_routes2.txt | grep -Po '(?<=dev )(\S+)' | grep -v vpn_se | head -1)
echo "adapter2=[${ADAPTER:-EMPTY}]"
ADAPTER3=$(grep default grep_p_routes.txt grep_p_routes2.txt | grep -Po '(?<=dev )(\S+)' | grep -v vpn_se | head -1)
echo "adapter3=[${ADAPTER3:-EMPTY}]"

echo "-- snap-debug-info.sh: non-capturing group + \s, -o --"
printf 'ID Status When\n1 Doing now\n2 Error now\n3 Doing now\n4 Error later\n' > grep_p_snap.txt
DOINGS=$(cat grep_p_snap.txt | tail -n +2 | grep -Po '(?:[0-9]+\s+Doing)' | awk '{print $1}')
echo "doings=[$DOINGS]"
ERRORS=$(cat grep_p_snap.txt | tail -n +2 | grep -Po '(?:[0-9]+\s+Error)' | awk '{print $1}')
echo "errors=[$ERRORS]"

echo "-- softy: FQDN validation pattern (kept as grep -P) --"
# lookahead + atomic group + negative lookahead: not ERE-expressible —
# the grep-pcre transform REFUSES, the exec stays `grep -P`. (The softy
# original tested the capture with `[[ "$(...)" == "" ]]`; a count is
# used here so the example stays deterministic through the perl backend —
# the -z/\$var single-bracket test shape is a pre-existing parser gap
# unrelated to grep -P.)
N=$(echo sub.example.com | grep -Pc '(?=^.{1,254}$)(^(?>(?!\d+\.)[a-zA-Z0-9_\-]{1,63}\.?)+(?:[a-zA-Z]{2,})$)')
echo "fqdn-count=$N"
N=$(echo 192.168.1.1 | grep -Pc '(?=^.{1,254}$)(^(?>(?!\d+\.)[a-zA-Z0-9_\-]{1,63}\.?)+(?:[a-zA-Z]{2,})$)')
echo "ip-count=$N"

echo "-- memtester.sh / terminateProcess.sh: pgrep -P (untouched) --"
sleep 0.01 &
BGPID=$!
CPID=$(pgrep -P "$BGPID" no-such-name-xyz 2>/dev/null || true)
echo "pgrep-no-match=[${CPID:-EMPTY}]"
wait "$BGPID" 2>/dev/null

exit 0
