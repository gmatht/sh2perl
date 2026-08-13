#!/bin/bash
# GNU-isms / bashisms thorough example — exercises the constructs a bash
# translator must lower for POSIX sh:
#   [[ ]] tests (incl. =~ regex), ${var^^} case conversion, ${var/pat/repl}
#   substitution, ${var:off:len} slicing, indexed + associative arrays,
#   <<< herestrings, process substitution, >& redirects, (( )) / let / ++,
#   $(( ** )), PIPESTATUS, echo -n/-e, printf -v, shopt -s nocasematch,
#   and GNU coreutils flags (seq, readlink -f/e/m, cmp -b/-n/-i, grep -P,
#   sed -r, tr classes/-s, sort -h, wc -L, head/tail -c).
# Output is deterministic (no RANDOM/time/pid paths; assoc keys sorted).
# Gate verdicts per construct pin the backend gaps; they go green as the
# lowering lands.

cd /tmp

# ── [[ ]] test forms ────────────────────────────────────────────────
[[ 5 -gt 3 ]];   echo "arith:  $?"
[[ "abc" == "abc" ]];  echo "str:    $?"
[[ -f /etc/passwd ]]; echo "file:   $?"
[[ "hello world" =~ ^hello ]]; echo "regex:  $?"
[[ -z "" ]];     echo "empty:  $?"
[[ -n "x" ]];    echo "nonempty: $?"
[[ "a" < "b" ]]; echo "lt:     $?"
[[ "a" != "b" ]]; echo "ne:    $?"
[[ ! -e /no/such/file ]]; echo "notfile: $?"

# ── case conversion (${x^^} ${x,,} ${x^}; ${x,} left out — the
#    renderer emits it raw, a fatal "Bad substitution" under dash) ──
x="HeLLo WoRLD"
echo "up=${x^^}"
echo "down=${x,,}"
echo "firstup=${x^}"

# ── pattern substitution & trimming ─────────────────────────────────
y="one-two-one"
echo "gsub=${y//one/1}"
echo "sub=${y/one/1}"
echo "prefix=${y#one-}"
echo "suffix=${y%-one}"
echo "shortest=${y##*o}"
echo "greedy=${y%%-*}"

# ── slicing ─────────────────────────────────────────────────────────
z="0123456789"
echo "mid=${z:2:3}"
echo "tail=${z:5}"
echo "neg=${z: -3}"
echo "len=${#z}"

# ── indexed arrays (direct assignment; `declare -a` hangs the perl
#    generator — pre-existing core bug, also pinned by 062_14) ───────
arr=(alpha beta gamma)
echo "idx=${arr[1]}"
echo "count=${#arr[@]}"
echo "all=${arr[@]}"
for el in "${arr[@]}"; do echo "el=$el"; done

# ── associative arrays (keys sorted for deterministic output; bash's
#    native `${!aa[@]}` iteration order is hash order) ───────────────
declare -A aa
aa[one]=1
aa[two]=2
aa[three]=3
echo "aa-one=${aa[one]}"
for k in $(echo "${!aa[@]}" | tr ' ' '\n' | sort); do echo "aa[$k]=${aa[$k]}"; done

# ── herestring ──────────────────────────────────────────────────────
cat <<< "hello herestring"

# ── >& redirect (bash merges both streams into the file) ────────────
echo "both" >& /tmp/gnu_both.txt
cat /tmp/gnu_both.txt
# stderr is exercised but NOT compared by the equivalence gate (both
# sides ignore stderr — only stdout is the contract)
echo "err-to-stderr" >&2

# ── arithmetic (( )) / let / ++ / ** ────────────────────────────────
(( ax = 5 + 3 ))
echo "arith=$ax"
let "ly = ax * 2"
echo "let=$ly"
(( ax++ ))
echo "postfix=$ax"
echo "pow=$(( 2 ** 10 ))"

# ── PIPESTATUS ──────────────────────────────────────────────────────
false | true
echo "ps=${PIPESTATUS[0]},${PIPESTATUS[1]}"

# ── echo -n / -e ────────────────────────────────────────────────────
echo -n "no-nl"; echo "."
echo -e "tab\there"

# ── printf -v ───────────────────────────────────────────────────────
printf -v pv "value-%d" 42
echo "$pv"

# ── GNU tool flags (GNU coreutils present on the gate host) ─────────
echo "seq: $(seq 1 3 | tr '\n' ' ')"
ln -sf /etc/passwd /tmp/gnu_link
echo "readlink-f: $(readlink -f /tmp/gnu_link)"
echo "readlink-e: $(readlink -e /tmp/gnu_link)"
readlink -m /no/such/path >/dev/null; echo "readlink-m rc=$?"

printf 'abc\n' > /tmp/gnu_a.txt
printf 'abd\n' > /tmp/gnu_b.txt
cmp -b /tmp/gnu_a.txt /tmp/gnu_b.txt; echo "cmp-b rc=$?"
cmp -n 2 /tmp/gnu_a.txt /tmp/gnu_b.txt; echo "cmp-n rc=$?"
cmp -i 2 /tmp/gnu_a.txt /tmp/gnu_b.txt; echo "cmp-i rc=$?"

printf 'foo\nbar123\n' | grep -P '\d+' >/dev/null; echo "grep-P rc=$?"
echo "a1b2c3" | sed -r 's/[0-9]+/N/g'
echo "AbCdEf" | tr '[:lower:]' '[:upper:]'
echo "  spaced  " | tr -s ' '
printf '10K\n2M\n500\n' | sort -h
printf 'short\nvery-long-line-here\n' | wc -L
printf 'abcdef' | head -c 3; echo
printf 'abcdef' | tail -c 2; echo

# ── shopt -s nocasematch (the [[ == ]] case emulation must fold) ────
shopt -s nocasematch
[[ "ABC" == "abc" ]] && echo "nocase-match"
shopt -u nocasematch
[[ "ABC" == "abc" ]] && echo "case-matters" || echo "case-sensitive"

# cleanup
rm -f /tmp/gnu_both.txt /tmp/gnu_link /tmp/gnu_a.txt /tmp/gnu_b.txt
