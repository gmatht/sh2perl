#!/usr/bin/env bash
# Differential gate: bash vs emitted Perl — the whole corpus, every channel.
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
#      requiring `perl -w -c` to accept it with no warning at all;
#   2. runs bash and the Perl in identically-named working directories (so
#      `pwd`/`usage: $0`-style output is comparable), feeding both
#      tests/stdin/<example-name>.stdin when present and /dev/null otherwise;
#   3. compares STDOUT, STDERR and EXIT STATUS byte-for-byte, for every
#      fixture — no per-directory exemption;
#   4. enforces the source's own
#        # PERL_MUST_NOT_CONTAIN <text>   /   # PERL_MUST_CONTAIN <text>
#      directives, scanning emitted CODE only (comment lines carry no
#      behaviour and the renderer's preamble legitimately mentions
#      `` `cat ``);
#   5. when the source declares an expectation, requires bash's own stdout to
#      be exactly that text, so a broken bash run and an equally empty Perl
#      run cannot "match" while proving nothing. Three forms, in priority
#      order:
#        a) a sibling file `<fixture>.expected` (byte-for-byte);
#        b) a BLOCK: a line `# expected (bash):` followed by `# `-prefixed
#           lines — one per output line, so a failure names the case;
#        c) a single line `# expected (bash): <text>`.
#
# A comparison that legitimately cannot match bash is NOT skipped: it is an
# entry in tests/bash-parity-known-gaps.txt (tab-separated `<fixture>` and
# `<reason>`), reported as KNOWN with its reason and counted separately. The
# ledger is checked both ways: an entry that is not in the sweep, or one whose
# fixture now matches bash, FAILS as stale. Nothing is silently ignored.
#
# Output is one line per example plus a summary; the exit status is non-zero
# if any comparison fails or the ledger is stale.
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
    full_sweep=1
    shopt -s nullglob
    examples=(examples/*.sh)
else
    full_sweep=0
    examples=("$@")
fi

if [ "${full_sweep:-0}" = 0 ] && [ "${#examples[@]}" -eq 0 ]; then
    # Curated: deterministic, self-contained examples that currently match
    # bash. 000__05 and 000__06 are deliberately absent — 05 is a strict
    # subset of 04d and 06 is the same text-processing script as 04c with a
    # different input file, so they add runtime but no signal. The full
    # sweep (`--all`) covers them.
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

# Fixtures that exist to pin the PARSER's diagnostics: they must keep being
# rejected (exit 2) and never reach the renderer. In `--all` they used to be
# reported as "render failed", turning five green fixtures into five false
# alarms; now the sweep asserts the rejection instead.
parse_error_fixtures=(
    parse-double-semicolon
    parse-error-doublesemicolon
    parse-parameter-expansion-eof
    parse-paren-after-do
    parse-unexpected-end-of-input
)
is_parse_error_fixture() {
    local n
    for n in "${parse_error_fixtures[@]}"; do
        [ "$n" = "$1" ] && return 0
    done
    return 1
}

# ── The checked-in known-divergence ledger ──────────────────────────────
# Every line `<fixture><TAB><reason>` (comments start with `#`) is a case
# where bash and the render provably differ and the difference is accepted
# and explained. Any other divergence FAILS. The ledger is also validated in
# the other direction: a listed fixture that was not swept, or one that now
# matches bash, is reported as STALE and fails the gate — a gap cannot be
# papered over once it is fixed.
ledger="$root/tests/bash-parity-known-gaps.txt"
declare -A known_reason=()
declare -A known_seen=()
declare -a known_order=()
while IFS=$'\t' read -r key reason; do
    key=${key%"${key##*[![:space:]]}"}
    reason=${reason#"${reason%%[![:space:]]*}"}
    [ -n "$key" ] || continue
    known_reason["$key"]="$reason"
    known_order+=("$key")
done < <(sed -n '/^[^#[:space:]]/p' "$ledger" 2>/dev/null)

# A fixture's ledger key: repo-relative path, `./` and `.sh` removed.
ledger_key() {
    local p=$1
    p=${p#"$root/"}
    p=${p#./}
    printf '%s' "${p%.sh}"
}

ok_count=0
known_count=0
diff_count=0
parse_skipped=0
fail=0
declare -a diff_list=()
declare -a stale_list=()

for f in "${examples[@]}"; do
    if [ ! -f "$f" ]; then
        echo "MISSING $f"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$f (missing)")
        continue
    fi
    name=$(basename "$f" .sh)
    key=$(ledger_key "$f")
    pl="$tmp/$name.pl"
    verdict="ok"

    # `--fixed` is the shipped `--target pl` path
    # (`perl_generator_fixed::generate`): the single renderer plus the
    # IR/text fixups the CLI applies. Testing bare `shir_to_perl` here would
    # gate a pipeline no user gets.
    # The renderer installs the SOURCE path as `$0` (bash's `$0` is the path
    # the shell was invoked with), so render with the ABSOLUTE path here: bash
    # below runs `$root/$f`, and both sides are normalised to `<script>`.
    "$BIN" --fixed "$root/$f" >"$pl.rendered" 2>"$tmp/render.err"; render_rc=$?
    if is_parse_error_fixture "$name"; then
        if [ "$render_rc" = 2 ]; then
            echo "ok   $name (parse-error fixture, rejected as intended)"
            ok_count=$((ok_count + 1))
            parse_skipped=$((parse_skipped + 1))
            continue
        fi
        echo "FAIL $name: parse-error fixture exited $render_rc instead of 2"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key (parse-error fixture rendered)")
        continue
    fi
    if [ "$render_rc" -ne 0 ]; then
        echo "FAIL $name: render failed"
        sed -n '1,5p' "$tmp/render.err"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key (render failed)")
        continue
    fi
    cp "$pl.rendered" "$pl"

    # `perl -w -c` must be accepted AND silent. A compile-time warning is a
    # render regression: the `Useless use of numeric eq in void context` and
    # the uninitialised-read classes both surfaced as warnings before they
    # surfaced as wrong output.
    if ! perl -w -c "$pl" >/dev/null 2>"$tmp/perl-c.err"; then
        echo "FAIL $name: perl -c"
        sed -n '1,5p' "$tmp/perl-c.err"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key (perl -c)")
        continue
    fi
    grep -v 'sh2perl: uu-ffi ' "$tmp/perl-c.err" \
        | grep -v 'syntax OK' >"$tmp/perl-c.warn" || true
    if [ -s "$tmp/perl-c.warn" ]; then
        echo "FAIL $name: perl -w -c warnings"
        sed -n '1,5p' "$tmp/perl-c.warn"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key (perl -w -c warnings)")
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
        diff_count=$((diff_count + 1))
        diff_list+=("$key (MUST_NOT_CONTAIN$bad)")
        continue
    fi

    # The mirror image: code the fixture REQUIRES. This is how a fixture pins
    # an idiom the comparison alone cannot see — e.g. a native capture that
    # must render `local *STDOUT` rather than silently folding to a literal.
    must_contain=()
    while IFS= read -r line; do
        [ -n "$line" ] && must_contain+=("$line")
    done < <(sed -n 's/^#.*PERL_MUST_CONTAIN[[:space:]]\+//p' "$f")
    missing=""
    for pat in "${must_contain[@]}"; do
        if ! grep -qF -- "$pat" "$tmp/$name.code.pl"; then missing="$missing [$pat]"; fi
    done
    if [ -n "$missing" ]; then
        echo "FAIL $name: MUST_CONTAIN$missing"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key (MUST_CONTAIN$missing)")
        continue
    fi

    stdin_file="$root/tests/stdin/$name.stdin"
    [ -f "$stdin_file" ] || stdin_file=/dev/null
    # Both runs use the SAME absolute working directory, sequentially (wiped
    # between them). Many examples print `pwd`; two differently-named temp
    # dirs made a CORRECT render look wrong (058, 062_05, 062_hard_to_lex,
    # qx-var-builtin-cd). A script that invokes a sibling by relative path
    # gets a copy of examples/ — but only when its own source mentions it, so
    # nothing else pays the copy.
    wd="$tmp/$name/work"
    run_side() {
        local tag=$1
        shift
        rm -rf "$wd"
        mkdir -p "$wd"
        if grep -q 'examples/' "$f"; then
            cp -a "$root/examples" "$wd/examples"
        fi
        # A renderer bug can invert a loop condition and make the PERL hang
        # forever; a hang is a failure of this gate, not a waiting game.
        ( cd "$wd" && timeout 60 "$@" <"$stdin_file" \
            >"$tmp/$name.$tag.out" 2>"$tmp/$name.$tag.err"; echo $? >"$tmp/$name.$tag.rc" )
    }
    run_side b bash "$root/$f"
    run_side p perl "$pl"
    if [ "$(cat "$tmp/$name.p.rc")" = 124 ] || [ "$(cat "$tmp/$name.b.rc")" = 124 ]; then
        echo "FAIL $name: timed out (124) after 60s"
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key (timeout)")
        continue
    fi

    # `$0` is invocation metadata: bash sees the script path, the Perl sees its
    # rendered .pl. Normalize both to <script> so a usage message that prints
    # `$0` compares on its content, not on the temp path (057_case).
    #
    # The `script: line N: ` prefix a diagnostic carries is the same kind of
    # metadata — a builtin's message reaches stdout when the script redirects
    # `2>&1`, and the line number is the RENDER's, not the script's. The prefix
    # is therefore stripped on both channels, both sides; the message that
    # follows it is compared byte-for-byte.
    for tag in b p; do
        sed -e "s|$root/$f|<script>|g" -e "s|$pl|<script>|g" \
            "$tmp/$name.$tag.out" \
            | sed -E 's/^.*: line [0-9]+: //' >"$tmp/$name.$tag.nout"
        sed -e "s|$root/$f|<script>|g" -e "s|$pl|<script>|g" \
            "$tmp/$name.$tag.err" >"$tmp/$name.$tag.nerr.raw"
    done

    # A declared expectation makes the comparison prove the DOCUMENTED
    # behaviour, not merely that two runs agree with each other. Three forms:
    # a sibling `.expected` file, a multi-line `# expected (bash):` block, or
    # the original single line. The multi-line form exists so a fixture can
    # assert one case per line and a failure names the case in the diff.
    have_expected=0
    : >"$tmp/$name.expected"
    case_file="${f%.sh}.expected"
    if [ -f "$case_file" ]; then
        cp "$case_file" "$tmp/$name.expected"
        have_expected=1
    elif grep -qx '# expected (bash):' "$f"; then
        awk '
            /^# expected \(bash\):$/ { block = 1; next }
            block && /^#/ { sub(/^# ?/, ""); print; next }
            block        { exit }
        ' "$f" >"$tmp/$name.expected"
        have_expected=1
    else
        expected=$(sed -n 's/^# expected (bash): //p' "$f" | head -1)
        if [ -n "$expected" ]; then
            printf '%s\n' "$expected" >"$tmp/$name.expected"
            have_expected=1
        fi
    fi
    if [ "$have_expected" = 1 ]; then
        if ! diff -q "$tmp/$name.expected" "$tmp/$name.b.nout" >/dev/null; then
            echo "FAIL $name: bash stdout differs from the declared expectation"
            diff -u "$tmp/$name.expected" "$tmp/$name.b.nout" | sed -n '1,20p'
            fail=1
            diff_count=$((diff_count + 1))
            diff_list+=("$key (bash differs from declared expectation)")
            continue
        fi
    fi

    # stderr: bash prefixes a builtin diagnostic with "<script>: line N: ",
    # which differs by invocation path even between two bash runs, and the Perl
    # runtime may print a local uu-ffi availability notice; strip both and
    # compare the message itself, for EVERY fixture.
    sed -E 's/^.*: line [0-9]+: //' "$tmp/$name.b.nerr.raw" >"$tmp/$name.b.nerr"
    sed -E 's/^.*: line [0-9]+: //' "$tmp/$name.p.nerr.raw" \
        | grep -v '^sh2perl: uu-ffi ' >"$tmp/$name.p.nerr" || true

    # A ledger entry whose reason starts with `nondeterministic:` names a
    # fixture whose comparison cannot be byte-ordered: concurrent subshells
    # write to the same descriptor and the kernel picks the order. Its stderr
    # is compared as a multiset (sorted), which still fails on a missing, extra
    # or reworded line.
    reason=${known_reason[$key]:-}
    # Same LINES, different ORDER: concurrent writers (process substitution,
    # background jobs) both report to the same descriptor and the kernel picks
    # the order — bash itself is not reproducible here. Such a fixture may only
    # pass with an explicit `nondeterministic:` ledger entry; without one it is
    # a named failure, never a silent pass.
    err_order_only=0
    if ! diff -q "$tmp/$name.b.nerr" "$tmp/$name.p.nerr" >/dev/null \
        && diff -q <(LC_ALL=C sort "$tmp/$name.b.nerr") \
                    <(LC_ALL=C sort "$tmp/$name.p.nerr") >/dev/null; then
        err_order_only=1
    fi
    if [ "${reason#nondeterministic}" != "$reason" ]; then
        LC_ALL=C sort "$tmp/$name.b.nerr" >"$tmp/$name.b.nerr.cmp"
        LC_ALL=C sort "$tmp/$name.p.nerr" >"$tmp/$name.p.nerr.cmp"
    else
        cp "$tmp/$name.b.nerr" "$tmp/$name.b.nerr.cmp"
        cp "$tmp/$name.p.nerr" "$tmp/$name.p.nerr.cmp"
    fi

    mismatch=""
    if ! diff -q "$tmp/$name.b.nout" "$tmp/$name.p.nout" >/dev/null; then
        mismatch="$mismatch stdout"
    fi
    if ! diff -q "$tmp/$name.b.nerr.cmp" "$tmp/$name.p.nerr.cmp" >/dev/null; then
        mismatch="$mismatch stderr"
    fi
    b_rc=$(cat "$tmp/$name.b.rc")
    p_rc=$(cat "$tmp/$name.p.rc")
    if [ "$b_rc" != "$p_rc" ]; then
        mismatch="$mismatch exit($b_rc/$p_rc)"
    fi

    if [ -n "$mismatch" ]; then
        if [ -n "$reason" ]; then
            known_seen["$key"]=$mismatch
            known_count=$((known_count + 1))
            echo "KNOWN $name:${mismatch} — ${known_reason[$key]}"
            continue
        fi
        if [ "$err_order_only" = 1 ] && [ -z "$reason" ]; then
            echo "FAIL $name: stderr ORDER differs (concurrent writers) and has no"
            echo "     \`nondeterministic:\` ledger entry in tests/bash-parity-known-gaps.txt"
        else
            echo "FAIL $name:$mismatch"
        fi
        if ! diff -q "$tmp/$name.b.nout" "$tmp/$name.p.nout" >/dev/null; then
            diff -u "$tmp/$name.b.nout" "$tmp/$name.p.nout" | sed -n '1,12p'
        fi
        if ! diff -q "$tmp/$name.b.nerr" "$tmp/$name.p.nerr" >/dev/null; then
            diff -u "$tmp/$name.b.nerr" "$tmp/$name.p.nerr" | sed -n '1,12p'
        fi
        fail=1
        diff_count=$((diff_count + 1))
        diff_list+=("$key ($mismatch)")
        continue
    fi

    # A ledger entry whose fixture now MATCHES bash is a fixed gap: drop the
    # entry instead of letting the ledger drift away from reality. The
    # `nondeterministic:` exemption is the one exception — its comparison is
    # SUPPOSED to match once the order is ignored, so it stays KNOWN (the
    # byte order is still not comparable) and is never "fixed".
    if [ -n "$reason" ]; then
        if [ "${reason#nondeterministic}" != "$reason" ]; then
            known_seen["$key"]="order-exempt"
            known_count=$((known_count + 1))
            echo "KNOWN $name:stderr order exempt — ${reason}"
            continue
        fi
        stale_list+=("$key (listed as known, now matches bash)")
        echo "STALE $name: listed as a known divergence, now matches bash"
        fail=1
        continue
    fi
    echo "ok   $name"
    ok_count=$((ok_count + 1))
done

# In a full sweep, a ledger entry whose fixture was never swept is stale too.
if [ "${full_sweep:-0}" = 1 ]; then
    for key in "${known_order[@]}"; do
        if [ ! -f "$root/$key.sh" ]; then
            stale_list+=("$key (no such fixture)")
            echo "STALE $key: listed as a known divergence but no such fixture"
            fail=1
        fi
    done
fi

echo
echo "bash-parity: ${ok_count} matched bash, ${diff_count} diverged, ${known_count} known divergence(s)"
if [ "${#known_order[@]}" -gt 0 ]; then
    echo "bash-parity: known divergences (tests/bash-parity-known-gaps.txt):"
    for key in "${known_order[@]}"; do
        printf '  %-46s %s\n' "$key" "${known_reason[$key]}"
    done
fi
if [ "$diff_count" -gt 0 ]; then
    echo "bash-parity: total diff count: ${diff_count}"
    for d in "${diff_list[@]}"; do printf '  %s\n' "$d"; done
else
    echo "bash-parity: total diff count: 0"
fi
if [ "${#stale_list[@]}" -gt 0 ]; then
    echo "bash-parity: stale ledger entries:"
    for d in "${stale_list[@]}"; do printf '  %s\n' "$d"; done
fi
if [ "$fail" -ne 0 ]; then
    echo "bash-parity: FAILURES above"
    exit 1
fi
echo "bash-parity: all compared examples match bash"
