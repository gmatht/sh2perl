#!/usr/bin/env bash
v="v1.2"
if [[ "$v" =~ ^v([0-9]+)[.]([0-9]+)$ ]]; then
  echo "semver"
fi
