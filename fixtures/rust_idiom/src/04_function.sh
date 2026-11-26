#!/usr/bin/env bash
greet() {
  local who="$1"
  echo "hi $who"
}
greet "there"
