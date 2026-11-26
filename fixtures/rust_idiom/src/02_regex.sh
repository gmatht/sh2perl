#!/usr/bin/env bash
id="abc123"
if [[ "$id" =~ ^[a-z]+[0-9]+$ ]]; then
  echo "matched"
else
  echo "no"
fi
