#!/bin/bash
# Tier 1, class 3 follow-up: a nested `$()` inside a CD-LED multi-command
# capture.
#
# `z=$(cd /tmp ; echo "b$(pwd)")` was appended to the outer capture as the inner
# capture's RENDERED PERL, inside a double-quoted shell word:
#
#   __sh_pipe_capture(q['cd' '.'; 'echo' "bdo { my $__sh_cap; … }"], …)
#
# bash then expanded the (unset) `$__sh_cap` away and printed the remainder as
# text, so the shell saw `bdo { my ;  = (require Cwd, …) }` instead of the
# real answer. The word must be rebuilt as SHELL syntax (`$(…)`) — the nested
# command runs in the same shell bash runs it in — or computed in Perl.
#
# The assertions below pin the shapes:
#   - the nested substitution is `$(` inside the outer capture's text;
#   - the flattened literal form (`do { my ;` — the dollar-stripped trace)
#     never appears;
#   - a native capture in this file still uses the `local *STDOUT` lowering,
#     so the two capture paths are both exercised.
#
# (The rebuilt substitution keeps the command name quoted — `$('basename'
# '/a/b')` — which bash parses exactly like `$(basename /a/b)`.)
# PERL_MUST_CONTAIN "b$('pwd')"
# PERL_MUST_CONTAIN $('pwd')
# PERL_MUST_CONTAIN $('basename' '/a/b')
# PERL_MUST_CONTAIN "u=$('basename' '/p/q/r')"
# PERL_MUST_CONTAIN local *STDOUT
# PERL_MUST_NOT_CONTAIN do { my ;
# PERL_MUST_NOT_CONTAIN "bdo {
# PERL_MUST_NOT_CONTAIN "zdo {
# PERL_MUST_NOT_CONTAIN "udo {

# 1. `cd`-led body, nested `$()` in a LATER command's quoted argument.
z=$(cd /tmp ; echo "b$(pwd)")
echo "z=$z"

# 2. Nested `$()` in the FIRST command of the body, after the cd.
y=$(cd /tmp ; echo "$(pwd)" ; echo tail)
echo "y=$y"

# 3. Same shape inside a BARE (unquoted) assignment word.
w=$(cd /tmp ; echo b$(pwd))
echo "w=$w"

# 4. Nested `$()` at the very start of the body (first command, first word),
#    before any cd executes.
v=$(echo "$(basename /x/y/z)"; cd /tmp ; echo done)
echo "v=$v"

# 5. The same shapes through a shell function, and with a `cd` that fails:
#    bash still runs the rest of the body and keeps the captured text.
show() {
    local u
    u=$(cd /no/such/dir ; echo "u=$(basename /p/q/r)")
    echo "u=$u"
}
show

# 6. A NATIVE single-command capture with a nested substitution, so the
#    non-fork path and the shell-text path are covered side by side.
n=$(echo "$(basename /a/b)")
echo "n=$n"

# 7. Two nested substitutions in one quoted word.
t=$(cd /tmp ; echo "$(basename /a/b)-$(basename /c/d)")
echo "t=$t"
