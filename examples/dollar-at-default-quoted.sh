#!/bin/sh
# $@ with default value containing quotes: ${@:-""}
# sh2perl generation (parse error)
ARGS=${@:-"default"}
echo "$ARGS"
