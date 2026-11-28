#!/bin/bash
# A COMMAND CONDITION is true when the command SUCCEEDS (status 0), in every
# condition position — if / elsif / while / until, `!`, either side of
# `&&`/`||`, a function body, a nested `if` — for external commands and for
# natively lowered builtins (cd, echo, printf, true, false, test, [, rm,
# mkdir, touch). A natively lowered builtin writes the shadow `$?`/`$CHILD_ERROR`
# its statement form writes, so `cmd; if [ $? -eq 0 ]` agrees with bash.
#
# The emitted Perl must not fork a shell for any of those builtins: a
# `system('bash', '-c', …)` fallback would both hide the status and re-enter a
# shell inside a render that claims to be native.
#
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'cd'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'echo'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'printf'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'true'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'false'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'test'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'['
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'rm'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'mkdir'
# PERL_MUST_NOT_CONTAIN: system('bash', '-c', q{'touch'

printf 'alpha\n' > cf.txt
mkdir -p cdir

if grep -q alpha cf.txt; then echo ext-true; else echo ext-false; fi
if grep -q zzz cf.txt; then echo ext2-true; else echo ext2-false; fi
if [ -f cf.txt ]; then echo br-true; else echo br-false; fi
if test -d cdir; then echo test-true; else echo test-false; fi
if [ -d cf.txt ]; then echo br2-true; else echo br2-false; fi
if true; then echo true-taken; fi
if false; then echo false-taken; else echo false-not; fi
if echo n-ok > /dev/null; then echo echo-true; else echo echo-false; fi
if printf '%s' p > /dev/null; then echo printf-true; else echo printf-false; fi
if mkdir cdir; then echo mkdir-true; else echo mkdir-false; fi
if mkdir newdir; then echo mkdir2-true; else echo mkdir2-false; fi
if touch tf; then echo touch-true; else echo touch-false; fi
if rm -f cf.txt; then echo rm-true; else echo rm-false; fi
if rm cf.txt; then echo rm2-true; else echo rm2-false; fi
if cd /no/such/dir; then echo s1; elif true; then echo s2; else echo s3; fi
if cd cdir; then echo cd-true; else echo cd-false; fi
cd ..
n=0; while false; do n=$((n+1)); done; echo "never=$n"
n=0; while [ $n -lt 2 ]; do n=$((n+1)); done; echo "k=$n"
n=0; while cd /no/such/dir; do n=$((n+1)); done; echo "j=$n"
n=0; until true; do n=$((n+1)); done; echo "until=$n"
cd /no/such/dir && echo and-ran
cd /no/such/dir || echo or-ran
cd /no/such/dir; echo "st=$?"
rm -f nosuchfile; echo "rmst=$?"
mkdir cdir; echo "mkdirst=$?"
g() { if cd /no/such/dir; then echo fn-t; else echo fn-f; fi; }
g
if ! cd /no/such/dir; then echo bang-taken; else echo bang-not; fi
if if true; then false; fi; then echo nested-taken; else echo nested-not; fi
echo done

# expected (bash):
# ext-true
# ext2-false
# br-true
# test-true
# br2-false
# true-taken
# false-not
# echo-true
# printf-true
# mkdir-false
# mkdir2-true
# touch-true
# rm-true
# rm2-false
# s2
# cd-true
# never=0
# k=2
# j=0
# until=0
# or-ran
# st=1
# rmst=0
# mkdirst=1
# fn-f
# bang-taken
# nested-not
# done
