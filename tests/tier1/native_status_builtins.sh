#!/bin/bash
# Tier 1, class 2: natively lowered builtins must leave the SHELL's status in
# `$?`, and a failure must not abort the script (bash continues after a failed
# `cd`; the Perl lowering used to `chdir(...) or die` and kill the run).
#
# Every natively lowered builtin writes its own shadow status, so the `$?`
# read below sees the builtin's result and never a previous child's.
#
# A failed `cd` must NOT abort the script: the emitted Perl guards the
# chdir with an if/else that warns and sets status 1 (never `chdir(…) or
# die`, which killed the whole program where bash continues). These are
# literal-substring assertions on the known old idiom for THIS script's
# directory; the rendering itself is proven by the stdout/stderr/exit
# comparison below.
# PERL_MUST_NOT_CONTAIN chdir("/no/such/dir") or die
# PERL_MUST_NOT_CONTAIN or die "cd
# PERL_MUST_NOT_CONTAIN system("cd"
# PERL_MUST_NOT_CONTAIN system("rm"
# PERL_MUST_NOT_CONTAIN system("mkdir"
# PERL_MUST_NOT_CONTAIN system("touch"
# PERL_MUST_NOT_CONTAIN system("echo"
cd .; echo "cd-ok=$?"

cd /no/such/dir; echo "cd-bad=$?"

# The script CONTINUES past the failed cd (bash does; `or die` would not).
echo still-running

mkdir made-dir; echo "mkdir-ok=$?"

mkdir /no/such/dir/sub; echo "mkdir-bad=$?"

touch made-file; echo "touch-ok=$?"

touch /no/such/dir/f; echo "touch-bad=$?"

rm -f made-file; echo "rm-ok=$?"

rm /no/such/dir/f; echo "rm-bad=$?"

echo written > out.txt; echo "echo-ok=$?"

# The status is still the builtin's AFTER an intervening $? read.
cd /no/such/dir 2>/dev/null
rc=$?
echo "cd-rc-captured=$rc"
[ "$rc" -ne 0 ] && echo "cd-failure-observed"
