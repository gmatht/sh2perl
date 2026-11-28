#!/bin/bash
# Tier 1, class 4: a natively lowered builtin in COMPOUND / CONDITION position.
#
# `cd /no/such && echo x` must test the cd's status (not fork a shell), and
# neither `cd` nor `echo`/`printf`/`true`/`false`/`test` may be run through
# `system(...)` — that forks, and `((system(...)) == 0)` in void context is
# exactly the "Useless use of numeric eq in void context" warning.
# PERL_MUST_NOT_CONTAIN system("cd"
# PERL_MUST_NOT_CONTAIN system("echo"
# PERL_MUST_NOT_CONTAIN system("printf"
# PERL_MUST_NOT_CONTAIN system("true"
# PERL_MUST_NOT_CONTAIN system("false"
# PERL_MUST_NOT_CONTAIN system("test"
# PERL_MUST_NOT_CONTAIN system("rm"
# PERL_MUST_NOT_CONTAIN system("mkdir"
# PERL_MUST_NOT_CONTAIN system("touch"
# PERL_MUST_NOT_CONTAIN q{'rmdir'
# PERL_MUST_NOT_CONTAIN q{'mkdir'
# PERL_MUST_NOT_CONTAIN q{'touch'
# PERL_MUST_NOT_CONTAIN q{'rm'
# PERL_MUST_NOT_CONTAIN __sh_pipe_run(q{'cd'
# (a capture `$(cd …; …)` legitimately runs in a subshell -> __sh_pipe_capture)
# PERL_MUST_NOT_CONTAIN q{'['
# PERL_MUST_NOT_CONTAIN q{'true'
# PERL_MUST_NOT_CONTAIN q{'false' 
# expected (bash):
# after-cd-and=1
# cd-and-ran
# after-cd-ok=0
# true-and-ran
# false-or-ran
# if-cd-hit
# if-cd-miss
# if-false-miss
# if-true-hit
# if-test-hit
# if-test-miss
# grep-and-ran
# if-compound-hit
# mixed-chain-ran
# plain
# printf-rc=0
# echo-redirect-rc=0
# compound-rc=1
# redir-pwd=redir
# redir-cd-rc=1
# redir-and=redir
# redir-if=redir
# redir-chain-rc=0
# redir-chain-second-failed
# redir-chain-rc2=0
# redir-touch-failed
# redir-touch-rc=0
# captured-cd-rc=1
# captured-cd-ok=0
# cb-cd-and=1
# cb-cd-and-ok
# cb-cd-and-ok=0
# cb-cd-or-ok
# cb-cd-or=0
# cb-bang-cd
# cb-bang-cd=0
# cb-bang-cd-ran=1
# cb-rm-and-ok
# cb-rm-and=0
# cb-rm-dir-fail=1
# cb-rm-or-ok
# cb-rm-or=0
# cb-rmdir-and-ok
# cb-rmdir-and=0
# cb-rmdir-fail=1
# cb-rmdir-or-ok
# cb-rmdir-or=0
# cb-mkdir-and-ok
# cb-mkdir-and=0
# cb-mkdir-fail=1
# cb-mkdir-or-ok
# cb-mkdir-or=0
# cb-touch-and-ok
# cb-touch-and=0
# cb-touch-fail=1
# cb-touch-or-ok
# cb-touch-or=0
# cb-a
# cb-echo-and
# cb-echo-and=0
# cb-b
# cb-echo-or=0
# cb-p
# cb-printf-and
# cb-printf-and=0
# 0
# cb-printf-fail=1
# 0
# cb-printf-or-ok
# cb-printf-or=0
# cb-true-and
# cb-true-and=0
# cb-false-and=1
# cb-false-or
# cb-false-or=0
# cb-true-or=0
# cb-bang-true-or
# cb-bang-true-or=0
# cb-bang-false-and
# cb-bang-false-and=0
# cb-bracket-and
# cb-bracket-and=0
# cb-bracket-fail=1
# cb-bracket-or
# cb-bracket-or=0
# cb-bracket-chain
# cb-bracket-chain=0
# cb-dbl-and
# cb-dbl-and=0
# cb-dbl-fail=1
# cb-dbl-or
# cb-dbl-or=0
# cb-dbl-both
# cb-dbl-both=0
# cb-bang-dbl
# cb-bang-dbl=0
# cb-later=1
# cb-still-running
# cb-cd-after-false
# cb-cd-after-false=0
# cb-true-after-false
# cb-true-after-false=0
# cb-test-after-false
# cb-test-after-false=0

printf 'alpha\n' > hay.txt

# A failing natively lowered builtin on the left of `&&` runs nothing.
cd /no/such/dir && echo cd-and-ran
echo "after-cd-and=$?"

# A succeeding one runs its right-hand side.
cd . && echo cd-and-ran
echo "after-cd-ok=$?"

# `&&` chain whose left side is native and whose right side is native.
true && echo true-and-ran
false && echo false-and-never
false || echo false-or-ran
true || echo true-or-never

# Native builtins as CONDITION words (external grep keeps its fork).
if cd .; then echo if-cd-hit; else echo if-cd-miss; fi
if cd /no/such/dir; then echo if-cd-hit; else echo if-cd-miss; fi
if false; then echo if-false-hit; else echo if-false-miss; fi
if true; then echo if-true-hit; else echo if-true-miss; fi
if [ -n alpha ]; then echo if-test-hit; else echo if-test-miss; fi
if [ -z alpha ]; then echo if-test-hit; else echo if-test-miss; fi
if grep -q alpha hay.txt && echo grep-and-ran; then echo if-compound-hit; else echo if-compound-miss; fi

# Mixed chain: native builtin, external command, native builtin.
mkdir made && grep -q alpha hay.txt && echo mixed-chain-ran
printf 'plain\n'; echo "printf-rc=$?"
echo plain > /dev/null; echo "echo-redirect-rc=$?"

# The status of a native compound is observable afterwards.
cd /no/such/dir && echo never
echo "compound-rc=$?"

# A STDERR-only redirect on a native builtin redirects ONLY its diagnostics:
# the builtin still runs in this shell, so the chdir MUST take effect here
# (the shell-fallback path chdir'd a child and left the parent where it was).
mkdir -p redir/sub
cd redir 2>/dev/null
echo "redir-pwd=$(basename $(pwd))"
cd /no/such/dir 2>/dev/null
echo "redir-cd-rc=$?"
cd ..
rm -rf redir made

# Same via `&&` and `if`, still native (no fork in the emitted Perl).
mkdir -p redir/sub
cd redir 2>/dev/null && echo "redir-and=$(basename $(pwd))"
cd ..
if cd redir 2>/dev/null; then echo "redir-if=$(basename $(pwd))"; fi
cd ..
rm -rf redir

# A redirect-wrapped native builtin on either side of a chain.
mkdir redir-chain 2>/dev/null || echo "redir-chain-failed"
echo "redir-chain-rc=$?"
mkdir redir-chain 2>/dev/null || echo "redir-chain-second-failed"
echo "redir-chain-rc2=$?"
rm -rf redir-chain
touch /no/such/f 2>/dev/null || echo "redir-touch-failed"
echo "redir-touch-rc=$?"

# The status of a native builtin survives a CAPTURE, value form included.
x=$(cd /no/such/dir; echo $?)
echo "captured-cd-rc=$x"
y=$(cd . ; echo $?)
echo "captured-cd-ok=$y"

# ── per-builtin compound position: SUCCESS and FAILURE for every builtin ──
# Each shape prints a labelled line; the `$?` line right after it shows the
# chain status bash leaves. Diagnostics that bash words differently are
# silenced (the STATUS is what these shapes pin).

# cd
cd /no/such/dir 2>/dev/null && echo "cb-cd-and-ran"
echo "cb-cd-and=$?"
cd . && echo "cb-cd-and-ok"
echo "cb-cd-and-ok=$?"
cd /no/such/dir 2>/dev/null || echo "cb-cd-or-ok"
echo "cb-cd-or=$?"
! cd /no/such/dir 2>/dev/null && echo "cb-bang-cd"
echo "cb-bang-cd=$?"
! cd . && echo "cb-bang-cd-ran"
echo "cb-bang-cd-ran=$?"

# rm
touch cb-file
rm -f cb-file && echo "cb-rm-and-ok"
echo "cb-rm-and=$?"
mkdir cb-dir
rm cb-dir 2>/dev/null && echo "cb-rm-dir-ran"
echo "cb-rm-dir-fail=$?"
rm cb-dir 2>/dev/null || echo "cb-rm-or-ok"
echo "cb-rm-or=$?"
rmdir cb-dir

# rmdir
mkdir cb-rd
rmdir cb-rd && echo "cb-rmdir-and-ok"
echo "cb-rmdir-and=$?"
rmdir cb-rd 2>/dev/null && echo "cb-rmdir-ran"
echo "cb-rmdir-fail=$?"
rmdir cb-rd 2>/dev/null || echo "cb-rmdir-or-ok"
echo "cb-rmdir-or=$?"

# mkdir
mkdir cb-md && echo "cb-mkdir-and-ok"
echo "cb-mkdir-and=$?"
mkdir cb-md 2>/dev/null && echo "cb-mkdir-ran"
echo "cb-mkdir-fail=$?"
mkdir cb-md 2>/dev/null || echo "cb-mkdir-or-ok"
echo "cb-mkdir-or=$?"
rmdir cb-md

# touch
touch cb-t && echo "cb-touch-and-ok"
echo "cb-touch-and=$?"
touch /no/such/t 2>/dev/null && echo "cb-touch-ran"
echo "cb-touch-fail=$?"
touch /no/such/t 2>/dev/null || echo "cb-touch-or-ok"
echo "cb-touch-or=$?"
rm -f cb-t

# echo / printf
echo cb-a && echo "cb-echo-and"
echo "cb-echo-and=$?"
false; echo cb-b || echo "cb-echo-or-BAD"
echo "cb-echo-or=$?"
printf 'cb-p\n' && echo "cb-printf-and"
echo "cb-printf-and=$?"
printf '%d\n' zz 2>/dev/null && echo "cb-printf-ran"
echo "cb-printf-fail=$?"
printf '%d\n' zz 2>/dev/null || echo "cb-printf-or-ok"
echo "cb-printf-or=$?"

# true / false
true && echo "cb-true-and"
echo "cb-true-and=$?"
false && echo "cb-false-and-BAD"
echo "cb-false-and=$?"
false || echo "cb-false-or"
echo "cb-false-or=$?"
true || echo "cb-true-or-BAD"
echo "cb-true-or=$?"
! true || echo "cb-bang-true-or"
echo "cb-bang-true-or=$?"
! false && echo "cb-bang-false-and"
echo "cb-bang-false-and=$?"

# test / [ ] / [[ ]]
[ -f /etc/passwd ] && echo "cb-bracket-and"
echo "cb-bracket-and=$?"
[ -f /no/passwd ] && echo "cb-bracket-ran"
echo "cb-bracket-fail=$?"
[ -f /no/passwd ] || echo "cb-bracket-or"
echo "cb-bracket-or=$?"
[ -d /tmp ] && [ -f /etc/passwd ] && echo "cb-bracket-chain"
echo "cb-bracket-chain=$?"
[[ -d /tmp ]] && echo "cb-dbl-and"
echo "cb-dbl-and=$?"
[[ -f /no/x ]] && echo "cb-dbl-ran"
echo "cb-dbl-fail=$?"
[[ -f /no/x ]] || echo "cb-dbl-or"
echo "cb-dbl-or=$?"
[[ -d /tmp && -f /etc/passwd ]] && echo "cb-dbl-both"
echo "cb-dbl-both=$?"
! [[ -f /no/x ]] && echo "cb-bang-dbl"
echo "cb-bang-dbl=$?"

# a failing left operand does not stop the script; a successful builtin after
# an earlier failure reads 0.
cd /no/such/dir 2>/dev/null && echo "cb-never"
echo "cb-later=$?"
echo "cb-still-running"
false; cd . && echo "cb-cd-after-false"
echo "cb-cd-after-false=$?"
false; true && echo "cb-true-after-false"
echo "cb-true-after-false=$?"
false; [ -f /etc/passwd ] && echo "cb-test-after-false"
echo "cb-test-after-false=$?"
