#!/usr/bin/env bash
pet="dog"
if [[ "$pet" =~ ^(cat|dog)$ ]]; then
  echo "animal"
fi
