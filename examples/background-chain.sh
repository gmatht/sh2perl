#!/bin/sh
# Background operator followed by another command on the same line
# This was a parse error: "Unexpected token: Background"
echo start & echo done

echo "exit: $?"
