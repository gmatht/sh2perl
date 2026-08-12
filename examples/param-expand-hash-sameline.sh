#!/bin/bash
# Regression test: ${var#pattern} with ; then on same line
MAXWAIT="20 10"
if [ ${MAXWAIT% *} -gt ${MAXWAIT#* } ]; then
    echo "same line"
fi
