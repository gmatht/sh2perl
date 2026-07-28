#!/bin/sh
# return inside if/then eats 'fi' as return value
f() {
  if true; then
    return
  fi
}
