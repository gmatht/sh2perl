#!/usr/bin/env bash
# A dead assignment's RHS can have side effects. DCE must not remove the
# command substitution merely because the assigned name is never read.
rm -f /tmp/dead-store-elim-probe
dead=$(python3 -c 'open("/tmp/dead-store-elim-probe", "w").write("side-effect\\n")')
if [ -e /tmp/dead-store-elim-probe ]; then
  echo side-effect
fi
rm -f /tmp/dead-store-elim-probe
