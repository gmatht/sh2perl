#!/bin/bash
# Double bracket [[ ... ]] inside a pipeline (&& / || chain)
# Triggers the parser's handling of [[ ... ]] && cmd
f=$(mktemp)
[[ -n "$f" ]] && [[ -f "$f" ]] && echo "File exists"
rm -f "$f"
