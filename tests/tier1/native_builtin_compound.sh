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
