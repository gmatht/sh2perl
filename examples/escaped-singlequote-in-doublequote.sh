#!/bin/sh
# Escaped single-quote inside double-quoted string
# Produces SingleQuotedString that spans across shell code
x="$(dpkg-query -W -f='${Conffiles}' "$pkg" | \
    sed -n -e "\' $conffile ' { s/ obsolete$//; s/.* //; p }")"
echo ok
