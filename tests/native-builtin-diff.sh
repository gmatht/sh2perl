#!/usr/bin/env bash
# Native-builtin differential: bash vs the emitted Perl.
#
# Every shell builtin this backend lowers NATIVELY (cd, exit, echo, printf,
# true, false, test, [, rm, mkdir, touch) is exercised as a COMMAND WORD in
# every position that can hold one:
#
#   stmt    plain statement
#   if      if-condition (and the elif chain)
#   while   while-condition
#   until   until-condition
#   neg     negated `!`
#   and/or  either side of `&&` / `||`
#
# For each case the two programs run in SEPARATE temp dirs (so the file
# effects cannot leak between them) and the harness compares stdout,
# normalised stderr, exit status, and checks the rendered Perl for
#
#   * `system(…)` / `bash -c` — a builtin must never fork a child,
#   * a bare `die`/`or die` — a failing builtin must never kill the script,
#   * `perl -w -c` warnings (uninitialised reads, "Useless use of == in
#     void context", …).
#
# Success AND failure forms are covered where the builtin has one; `echo`
# and `true` cannot fail in a shell without an I/O error, and `false` /
# `test -n ''` / `[ -z … ]` are the failing forms of their own truth, so the
# matrix uses the meaningful pair for each.
#
# Run from the repo root:
#
#   cargo build --quiet --bin perl_backend && bash tests/native-builtin-diff.sh
#
# Exit status: 0 when every cell matches, 1 otherwise (with the mismatches
# printed).

set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BACKEND="$ROOT/target/debug/perl_backend"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/sh2-builtin-diff.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$BACKEND" ]; then
    echo "native-builtin-diff: $BACKEND missing (cargo build --quiet --bin perl_backend)"
    exit 2
fi
command -v perl >/dev/null || { echo "native-builtin-diff: perl not installed"; exit 2; }

# name | success form | failure form ("" = builtin has no failing form)
BUILTINS=$(cat <<'EOF'
cd|cd /tmp|cd /no/such
exit|exit 0|exit 3
echo|echo hi|
printf|printf '%s\n' ok|printf '%d\n' zz
true|true|
false|false|false
test|test -n abc|test -n ''
bracket|[ -n abc ]|[ -z abc ]
rm|rm -f sh2_diff_missing|rm /no/such/dir/f
mkdir|mkdir sh2_diff_dir|mkdir /no/such/dir
touch|touch sh2_diff_touch|touch /no/such/dir/t
EOF
)

run_case() {
    local name=$1 src=$2 allow_fork=${3:-0}
    local dir="$WORK/$name"
    rm -rf "$dir"; mkdir -p "$dir/b" "$dir/p"

    printf '%s\n' "$src" > "$dir/b/prog.sh"
    cp "$dir/b/prog.sh" "$dir/p/prog.sh"

    if ! "$BACKEND" "$dir/b/prog.sh" > "$dir/p/prog.pl" 2> "$dir/p/render.err"; then
        echo "FAIL $name: render failed"; sed -n '1,3p' "$dir/p/render.err"; return 1
    fi
    local bad=""
    if grep -q 'sh2.unsupported' "$dir/p/prog.pl"; then bad="$bad [sh2.unsupported]"; fi
    # A `$(…)` command WORD is a different construct: bash expands it and
    # runs the RESULT as a command, so the emitted Perl legitimately hands
    # the captured text to `bash -c` (the two cmdsub_word_* cases below ask
    # for that with allow_fork=1). Builtins themselves never fork.
    if [ "$allow_fork" = 0 ]; then
        if grep -qF 'system(' "$dir/p/prog.pl"; then bad="$bad [system()]"; fi
        if grep -qF "bash', '-c'" "$dir/p/prog.pl"; then bad="$bad [bash -c]"; fi
    fi
    if grep -Eq '(^|[^_a-zA-Z])die[ "()]' "$dir/p/prog.pl"; then bad="$bad [die]"; fi
    if [ -n "$bad" ]; then
        echo "FAIL $name: render$bad"
        return 1
    fi

    if ! perl -w -c "$dir/p/prog.pl" 2> "$dir/p/perlc.err" >/dev/null; then
        echo "FAIL $name: perl -c"; sed -n '1,3p' "$dir/p/perlc.err"; return 1
    fi
    if grep -Eq 'Useless use|uninitialized|isn.t numeric' "$dir/p/perlc.err"; then
        echo "FAIL $name: perl -w warnings"; grep -E 'Useless use|uninitialized|isn.t numeric' "$dir/p/perlc.err" | sed -n '1,3p'
        return 1
    fi

    ( cd "$dir/b" && bash prog.sh >stdout 2>stderr; echo $? >rc )
    ( cd "$dir/p" && perl prog.pl >stdout 2>stderr; echo $? >rc )
    # bash prefixes diagnostics with "<script>: line N: "; the Perl runtime
    # may announce a missing optional uu-ffi library.
    sed -E 's/^.*: line [0-9]+: //' "$dir/b/stderr" > "$dir/b/stderr.n"
    sed -E 's/^.*: line [0-9]+: //' "$dir/p/stderr" | grep -v '^sh2perl: uu-ffi ' > "$dir/p/stderr.n"

    local fail=0
    if ! cmp -s "$dir/b/stdout" "$dir/p/stdout"; then
        echo "FAIL $name: stdout"; diff "$dir/b/stdout" "$dir/p/stdout" | sed -n '1,6p'; fail=1
    fi
    if ! cmp -s "$dir/b/stderr.n" "$dir/p/stderr.n"; then
        echo "FAIL $name: stderr"; diff "$dir/b/stderr.n" "$dir/p/stderr.n" | sed -n '1,6p'; fail=1
    fi
    if [ "$(cat "$dir/b/rc")" != "$(cat "$dir/p/rc")" ]; then
        echo "FAIL $name: exit $(cat "$dir/b/rc") vs $(cat "$dir/p/rc")"; fail=1
    fi
    return $fail
}

pass=0; fail=0
while IFS='|' read -r bname okform badform; do
    [ -n "$bname" ] || continue
    for form in ok fail; do
        form_src=$okform
        [ "$form" = fail ] && form_src=$badform
        # `false` fails by nature; its "ok" cell is the negated success form.
        [ -z "$form_src" ] && continue

        run_case "${bname}_${form}_stmt" "$form_src" && pass=$((pass+1)) || fail=$((fail+1))
        if [ "$form" = fail ] && [ "$bname" = "exit" ]; then
            # `exit 3` ends the shell, so only the statement cell applies.
            continue
        fi
        run_case "${bname}_${form}_if" \
            "if $form_src; then echo T; else echo F; fi" && pass=$((pass+1)) || fail=$((fail+1))
        run_case "${bname}_${form}_elsif" \
            "if false; then echo A; elif $form_src; then echo T; else echo F; fi" \
            && pass=$((pass+1)) || fail=$((fail+1))
        run_case "${bname}_${form}_while" \
            "n=0; while $form_src; do n=\$((n + 1)); break; done; echo n=\$n" \
            && pass=$((pass+1)) || fail=$((fail+1))
        run_case "${bname}_${form}_until" \
            "n=0; until $form_src; do n=\$((n + 1)); break; done; echo n=\$n" \
            && pass=$((pass+1)) || fail=$((fail+1))
        run_case "${bname}_${form}_neg" \
            "if ! $form_src; then echo T; else echo F; fi" && pass=$((pass+1)) || fail=$((fail+1))
        run_case "${bname}_${form}_and" \
            "$form_src && echo AND || echo OR" && pass=$((pass+1)) || fail=$((fail+1))
        run_case "${bname}_${form}_or" \
            "false || $form_src && echo AND || echo OR" && pass=$((pass+1)) || fail=$((fail+1))
    done
done <<< "$BUILTINS"

# The `$?` shadow: after a FAILED builtin the script continues with the
# builtin's status (never the previous child's), and a `&&` chain stops.
run_case "shadow_cd_then_test" \
    $'cd /no/such\nif [ $? -eq 0 ]; then echo ok; else echo bad; fi\necho st=$?' \
    && pass=$((pass+1)) || fail=$((fail+1))
run_case "shadow_rm_f_success" \
    $'rm -f sh2_diff_missing\necho $?' && pass=$((pass+1)) || fail=$((fail+1))
run_case "shadow_cd_and_echo" \
    $'cd /no/such && echo x\necho st=$?' && pass=$((pass+1)) || fail=$((fail+1))
run_case "shadow_mkdir_then_test" \
    $'mkdir /no/such/dir\nif [ $? -eq 0 ]; then echo ok; else echo bad; fi' \
    && pass=$((pass+1)) || fail=$((fail+1))
run_case "shadow_touch_then_test" \
    $'touch /no/such/dir/t\nif [ $? -ne 0 ]; then echo bad-ok; else echo bad-bad; fi' \
    && pass=$((pass+1)) || fail=$((fail+1))
run_case "shadow_printf_then_test" \
    $'printf \'%d\\n\' zz\nif [ $? -ne 0 ]; then echo bad-ok; else echo bad-bad; fi' \
    && pass=$((pass+1)) || fail=$((fail+1))

# The `$(…)`-command-word condition lowering must keep matching bash.
run_case "cmdsub_word_dyn_ok" \
    $'v=abc\nif $(echo test) -n "$v"; then echo yes; else echo no; fi' 1 \
    && pass=$((pass+1)) || fail=$((fail+1))
run_case "cmdsub_word_dyn_fail" \
    $'v=\'\'\nif $(echo test) -n "$v"; then echo yes; else echo no; fi' 1 \
    && pass=$((pass+1)) || fail=$((fail+1))

echo
echo "native-builtin-diff: $pass matched, $fail failed"
[ "$fail" -eq 0 ]
