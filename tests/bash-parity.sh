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
#   2. runs bash and the Perl in identically-named working directories (so
#      `pwd`/`usage: $0`-style output is comparable), feeding both
#      tests/stdin/<example-name>.stdin when present and /dev/null otherwise;
#   3. compares stdout and exit status byte-for-byte;
#   4. enforces the source's own
#        # PERL_MUST_NOT_CONTAIN <text>
#      directives (default: no backtick-`echo`/`printf`, i.e. those captures
#      must not become a qx fork), scanning emitted CODE only — comment lines
#      carry no behaviour and the renderer's preamble legitimately mentions
#      `` `cat ``;
#   5. when the source declares `# expected (bash): <text>`, requires bash's
#      own stdout to be exactly that line. Without this a broken bash run
#      (e.g. a missing stdin) and an equally empty Perl run "match" while
#      proving nothing.
#
# Output is one line per example; the exit status is non-zero if any
# comparison fails.
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

# Build the renderer ONCE, up front, and invoke the binary directly from then
# on. `cargo run` per fixture re-takes the target-dir lock, so any concurrently
# running cargo command (another gate, a `cargo test`) made fixtures fail with
# "render failed" and compiler text in render.err — a build collision dressed
# up as a render regression. A build error now fails the gate immediately,
# before any comparison, instead of once per example.
if ! cargo build --quiet --bin perl_backend; then
    echo "FAIL: cargo build --bin perl_backend"
    exit 2
fi
BIN="${CARGO_TARGET_DIR:-$root/target}/debug/perl_backend"
if [ ! -x "$BIN" ]; then
    echo "FAIL: built binary not found at $BIN"
    exit 2
fi

if [ "${1:-}" = "--all" ]; then
    shopt -s nullglob
    examples=(examples/*.sh)
elif [ "$#" -gt 0 ]; then
    examples=("$@")
else
    # Curated: deterministic, self-contained examples that currently match
    # bash. 000__05 and 000__06 are deliberately absent — 05 is a strict
    # subset of 04d and 06 is the same text-processing script as 04c with a
    # different input file, so they add runtime but no signal. Divergent
    # examples are listed in tests/bash-parity-known-gaps.txt.
    examples=(
        examples/000__02_output_formatting_commands.sh
        examples/000__03_file_manipulation_commands.sh
        examples/000__04a_basic_command_substitution.sh
        examples/000__04b_file_directory_operations.sh
        examples/000__04c_text_processing_commands.sh
        examples/000__04d_system_utilities.sh
        examples/000__04h_complex_examples.sh
        examples/012_test_const_read.sh
        # really reads stdin (`cat -`), fed from tests/stdin/
        examples/cat-dash-stdin.sh
        # loop exit status: zero iterations, last body command, break,
        # continue, and each condition form (test, external, natively
        # lowered builtin, compound) for both while and until
        tests/loop-status/*.sh
        # function-call and case exit status, in statement, &&/||, if and
        # $?-read positions
        tests/status/*.sh
        # the four Tier-1 correctness classes: command conditions, the
        # $?-shadow of natively lowered builtins, nested `$()` inside a
        # backtick capture, and native builtins in compound/condition position
        tests/tier1/*.sh
    )
fi

fail=0
for f in "${examples[@]}"; do
    if [ ! -f "$f" ]; then
        echo "MISSING $f"
        fail=1
        continue
    fi
    name=$(basename "$f" .sh)
    pl="$tmp/$name.pl"

    # `--fixed` is the shipped `--target pl` path
    # (`perl_generator_fixed::generate`): the single renderer plus the
    # IR/text fixups the CLI applies. Testing bare `shir_to_perl` here would
    # gate a pipeline no user gets.
    if ! "$BIN" --fixed "$f" >"$pl" 2>"$tmp/render.err"; then
        echo "FAIL $name: render failed"
        sed -n '1,5p' "$tmp/render.err"
        fail=1
        continue
    fi
    if ! perl -w -c "$pl" >/dev/null 2>"$tmp/perl-c.err"; then
        echo "FAIL $name: perl -c"
        sed -n '1,5p' "$tmp/perl-c.err"
        fail=1
        continue
    fi
    # `-w` warnings are a quality signal: the conditional-`== 0` and
    # uninitialised-read bugs both surfaced as warnings before they
    # surfaced as wrong output.
    if grep -Eq 'Useless use|uninitialized' "$tmp/perl-c.err"; then
        echo "FAIL $name: perl -w warnings"
        grep -E 'Useless use|uninitialized' "$tmp/perl-c.err" | sed -n '1,5p'
        fail=1
        continue
    fi

    # The source's own prohibitions, plus the default capture directive.
    must_not=('`echo' '`printf')
    while IFS= read -r line; do
        [ -n "$line" ] && must_not+=("$line")
    done < <(sed -n 's/^#.*PERL_MUST_NOT_CONTAIN[[:space:]]\+//p' "$f")
    grep -vE '^[[:space:]]*#' "$pl" >"$tmp/$name.code.pl"
    bad=""
    for pat in "${must_not[@]}"; do
        if grep -qF -- "$pat" "$tmp/$name.code.pl"; then bad="$bad [$pat]"; fi
    done
    if [ -n "$bad" ]; then
        echo "FAIL $name: MUST_NOT_CONTAIN$bad"
        fail=1
        continue
    fi

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
            echo "FAIL $name: bash stdout differs from the declared expectation"
            diff -u "$tmp/$name.expected" "$tmp/$name.b/work/out" | sed -n '1,12p'
            fail=1
            continue
        fi
    fi
    if ! diff -q "$tmp/$name.b/work/out" "$tmp/$name.p/work/out" >/dev/null; then
        echo "FAIL $name: stdout differs"
        diff -u "$tmp/$name.b/work/out" "$tmp/$name.p/work/out" | sed -n '1,12p'
        fail=1
        continue
    fi
    if [ "$(cat "$tmp/$name.b/work/rc")" != "$(cat "$tmp/$name.p/work/rc")" ]; then
        echo "FAIL $name: exit $(cat "$tmp/$name.b/work/rc") vs $(cat "$tmp/$name.p/work/rc")"
        fail=1
        continue
    fi

    # Loop-status fixtures assert a loop's EXIT STATUS, so stderr is part of
    # the contract. bash prefixes builtin diagnostics with "<script>: line
    # N: ", which differs by invocation path even between two bash runs, and
    # the Perl runtime may print a local uu-ffi availability notice; strip
    # both and compare the message itself.
    case "$f" in
    tests/loop-status/* | tests/status/* | tests/tier1/*) strict_stderr=1 ;;
    *) strict_stderr=0 ;;
    esac
    if [ "$strict_stderr" = 1 ]; then
        sed -E 's/^.*: line [0-9]+: //' "$tmp/$name.b/work/err" >"$tmp/$name.b.nerr"
        sed -E 's/^.*: line [0-9]+: //' "$tmp/$name.p/work/err" \
            | grep -v '^sh2perl: uu-ffi ' >"$tmp/$name.p.nerr"
        if ! diff -q "$tmp/$name.b.nerr" "$tmp/$name.p.nerr" >/dev/null; then
            echo "FAIL $name: stderr differs"
            diff -u "$tmp/$name.b.nerr" "$tmp/$name.p.nerr" | sed -n '1,12p'
            fail=1
            continue
        fi
    fi
    echo "ok   $name"
done

if [ "$fail" -ne 0 ]; then
    echo "bash-parity: FAILURES above"
    exit 1
fi
echo "bash-parity: all compared examples match bash"
