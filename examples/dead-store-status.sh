#!/usr/bin/env bash
# A dead assignment's RHS can have side effects. DCE must not remove the
# command substitution merely because the assigned name is never read.
rm -f /tmp/dead-store-elim-probe
dead=$(printf 'side-effect\n' > /tmp/dead-store-elim-probe)
cat /tmp/dead-store-elim-probe
rm -f /tmp/dead-store-elim-probe
