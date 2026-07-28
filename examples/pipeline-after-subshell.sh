#!/bin/sh
cd "$(dirname "$0")"/.. || exit 1
printf 'resolved dir=[%s]\n' "$PWD"
