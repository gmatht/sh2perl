#!/bin/sh
# heredoc inside if/then causes parse failure
f() {
  if true; then
    cat <<EOF
hello
EOF
    echo done
  fi
}
