#!/bin/bash
# `cmd 2>&1` on a natively lowered builtin is a descriptor DUPLICATION, not a
# file named `&1`. It used to render `open(STDERR, '>', "&1")`: a junk file was
# created and the diagnostic vanished, where bash forwards it to stdout. The
# status and the branch that reads it have to stay bash's too.
# PERL_MUST_CONTAIN open(STDERR, '>&', \*STDOUT)
# PERL_MUST_NOT_CONTAIN "&1"
#
# (The `script: line N: ` prefix is normalised away by the gate, so the
# compared bytes are the message itself.)
cd /no/such/dir 2>&1
echo "rc=$?"
if cd /no/such/dir 2>&1; then echo taken; else echo else-branch; fi
echo done
