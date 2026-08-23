#!/usr/bin/env bash
# Regression test: a constant read ONLY inside a test bracket
# (`[ "$i" -lt "$LIMIT" ]`). dead-store-elim must not drop the LIMIT
# assignment — its read lives inside a Str operand ("$LIMIT"), which a
# census that only walks Var/Index/name-arg nodes treats as opaque.
# When the store is dropped, the comparison runs against 0 and the loop
# body never executes (MIMEcroft.sh's RD_VR=16000 culling, the settings
# menu's sm_tex_total texture preload, RANGE shooting, … all broke the
# same way).
#
# expected (bash): 0123456789
#   estree bug:    (empty — LIMIT dropped, loop never runs)
LIMIT=10
i=0
while [ "$i" -lt "$LIMIT" ]; do
  printf "%s" "$i"
  i=$((i + 1))
done
echo ""
