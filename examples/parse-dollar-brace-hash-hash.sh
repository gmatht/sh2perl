#!/bin/sh
# Parameter expansion with ## (remove longest prefix pattern)
# inside a [[ ... ]] test expression.
# The ${p##*/} pattern extracts the basename of the path.
p=/usr/local/bin/example
if [[ "${p##*/}" == "example" ]]; then
    echo "it is example"
fi
