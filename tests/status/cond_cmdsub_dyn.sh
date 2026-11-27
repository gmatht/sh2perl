# A command-substitution COMMAND WORD with a non-literal operand:
# `$(echo test) -n "$v"` runs `test -n VALUE`, so the operand decides the
# branch. Before the fix the whole condition fell back to shell-text
# rebuilding, which dropped the substitution and ran `-n "$v"` as the
# command name (always false).
v=abc
if $(echo test) -n "$v"; then echo yes; else echo no; fi
echo $?
v=''
if $(echo test) -n "$v"; then echo yes; else echo no; fi
echo $?
if ! $(echo test) -z "$v"; then echo yes; else echo no; fi
echo $?
# A multi-word capture plus a dynamic operand.
cmd='test -n'
w=x
if $(echo "$cmd") "$w"; then echo yes; else echo no; fi
echo $?
# A value carrying shell syntax is ONE argv, never re-parsed.
if $(echo false) 'x; echo INJECTED'; then echo yes; else echo no; fi
echo $?
# Dynamic operand in a while condition.
i=0
while $(echo true) "$i"; do echo iter; i=1; break; done
echo $?
