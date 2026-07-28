#!/bin/sh
# Brace group in a pipeline/while condition should be parsed as block
while ! { : >> /tmp/test; } 2>/dev/null; do
  echo "waiting"
done
