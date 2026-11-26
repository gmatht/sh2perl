#!/usr/bin/env bash
# Differential gate: bash vs emitted Perl.
#
# This repo's standalone guarantee used to be "it compiles": the 552-example
# corpus lives in the external workspace harness. Every wrong-output bug in
# this backend (doubled qx escapes, `system("cd")`, inverted command
# conditions, nested $(…), $? after a native builtin) was found by hand with
# `diff`, not by a test. This script is that hand-check, automated.
#
# For each example it:
#   1. renders it the way the CLI does (`perl_backend --fixed` =
#      perl_generator_fixed::generate over the single emitter ir::shir_to_perl),
#      requiring `perl -w -c` to accept it;
#   2. runs bash and each Perl in identically-named working directories
#      (so `pwd`/`usage: $0`-style output is comparable);
#   3. compares stdout and exit status byte-for-byte;
#   4. enforces the source's own
#        # PERL_MUST_NOT_CONTAIN <text>
#      directives (default: no backtick-`echo`/`printf`, i.e. those captures
#      must not become a qx fork);
#   5. when the source declares `# expected (bash): <text>`, requires bash's
#      own stdout to be exactly that line. Without this a broken bash run
#      (e.g. a missing stdin) and an equally empty Perl run "match" while
#      proving nothing.
#
# An example that needs input gets tests/stdin/<example-name>.stdin; both
# interpreters receive it (and /dev/null otherwise, so runs are deterministic).
# Output is one line per (example, emitter); the exit status is non-zero if
# any comparison fails.
#
# Usage:
#   bash tests/bash-parity.sh                     # curated default list
#   bash tests/bash-parity.sh --all               # every examples/*.sh
#   bash tests/bash-parity.sh examples/foo.sh …   # explicit list
set -uo pipefail

root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
cd "$root"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [ "${1:-}" = "--all" ]; then
    shopt -s nullglob
    examples=(examples/*.sh)
elif [ "$#" -gt 0 ]; then
    examples=("$@")
else
    # Curated: deterministic, self-contained examples that currently match
    # bash on both emitters. Known-divergent examples are listed in
    # tests/bash-parity-known-gaps.txt instead of being silently skipped.
    examples=(
        examples/000__02_output_formatting_commands.sh
        examples/000__03_file_manipulation_commands.sh
        examples/000__04a_basic_command_substitution.sh
        examples/000__04b_file_directory_operations.sh
        examples/000__04c_text_processing_commands.sh
        examples/000__04d_system_utilities.sh
        examples/000__04h_complex_examples.sh
        examples/000__05_system_utilities.sh
        examples/000__06_text_processing_commands.sh
        examples/012_test_const_read.sh
        # really reads stdin (`cat -`), fed from tests/stdin/
        examples/cat-dash-stdin.sh
    )
fi

fail=0
for f in "${examples[@]}"; do
    [ -f "$f" ] || { echo "MISSING $f"; fail=1; continue; }
    name=$(basename "$f" .sh)

    # the source's own prohibitions (+ the default capture directive)
    must_not=('`echo' '`printf')
    while IFS= read -r line; do
        [ -n "$line" ] && must_not+=("$line")
    done < <(sed -n 's/^#.*PERL_MUST_NOT_CONTAIN[[:space:]]\+//p' "$f")

    for emitter in perl; do
        bin=perl_backend
        pl="$tmp/$name.$emitter.pl"
        # `--fixed` is the shipped `--target pl` path
        # (`perl_generator_fixed::generate`): the single renderer plus the
        # IR/text fixups the CLI applies. Testing bare `shir_to_perl` here
        # would gate a pipeline no user gets.
        if ! cargo run --quiet --bin "$bin" -- --fixed "$f" >"$pl" 2>"$tmp/render.err"; then
            echo "FAIL $name [$emitter]: render failed"; sed -n '1,5p' "$tmp/render.err"; fail=1; continue
        fi
        if ! perl -w -c "$pl" >/dev/null 2>"$tmp/perl-c.err"; then
            echo "FAIL $name [$emitter]: perl -c"; sed -n '1,5p' "$tmp/perl-c.err"; fail=1; continue
        fi
        # `-w` warnings are a quality signal: the conditional-`== 0` and
        # uninitialised-read bugs both surfaced as warnings before they
        # surfaced as wrong output.
        if grep -Eq 'Useless use|uninitialized' "$tmp/perl-c.err"; then
            echo "FAIL $name [$emitter]: perl -w warnings"
            grep -E 'Useless use|uninitialized' "$tmp/perl-c.err" | sed -n '1,5p'
            fail=1
            continue
        fi
        # The directive is about emitted CODE (`\`cat` must not become a
        # shell-out command substitution). Comment lines carry no behaviour,
        # and the renderer's own preamble comments legitimately *mention*
        # `` `cat `` — checking them would fail every program for a doc
        # sentence, so the scan runs over the code surface only.
        code="$tmp/$name.$emitter.code.pl"
        grep -vE '^[[:space:]]*#' "$pl" >"$code"
        bad=""
        for pat in "${must_not[@]}"; do
            if grep -qF -- "$pat" "$code"; then bad="$bad [$pat]"; fi
        done
        if [ -n "$bad" ]; then
            echo "FAIL $name [$emitter]: MUST_NOT_CONTAIN$bad"; fail=1; continue
        fi

        # same basename for both trees: `/…/b/work` vs `/…/p/work`
        stdin_file="$root/tests/stdin/$name.stdin"
        [ -f "$stdin_file" ] || stdin_file=/dev/null
        rm -rf "$tmp/$name.b" "$tmp/$name.p"
        mkdir -p "$tmp/$name.b/work" "$tmp/$name.p/work"
        ( cd "$tmp/$name.b/work" && bash "$root/$f" <"$stdin_file" >out 2>err; echo $? >rc )
        ( cd "$tmp/$name.p/work" && perl "$pl"        <"$stdin_file" >out 2>err; echo $? >rc )
        # A declared expectation makes the comparison prove the DOCUMENTED
        # behaviour, not merely that two runs agree with each other.
        expected=$(sed -n 's/^# expected (bash): //p' "$f" | head -1)
        if [ -n "$expected" ]; then
            printf '%s\n' "$expected" >"$tmp/$name.expected"
            if ! diff -q "$tmp/$name.expected" "$tmp/$name.b/work/out" >/dev/null; then
                echo "FAIL $name [$emitter]: bash stdout differs from the declared expectation"
                diff -u "$tmp/$name.expected" "$tmp/$name.b/work/out" | sed -n '1,12p'
                fail=1
                continue
            fi
        fi
        if ! diff -q "$tmp/$name.b/work/out" "$tmp/$name.p/work/out" >/dev/null; then
            echo "FAIL $name [$emitter]: stdout differs"
            diff -u "$tmp/$name.b/work/out" "$tmp/$name.p/work/out" | sed -n '1,12p'
            fail=1
            continue
        fi
        if [ "$(cat "$tmp/$name.b/work/rc")" != "$(cat "$tmp/$name.p/work/rc")" ]; then
            echo "FAIL $name [$emitter]: exit $(cat "$tmp/$name.b/work/rc") vs $(cat "$tmp/$name.p/work/rc")"
            fail=1
            continue
        fi
        echo "ok   $name [$emitter]"
    done
done

if [ "$fail" -ne 0 ]; then
    echo "bash-parity: FAILURES above"
    exit 1
fi
echo "bash-parity: all compared examples match bash"
