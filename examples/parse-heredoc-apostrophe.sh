#!/bin/sh
# Single quote (apostrophe) inside a heredoc body causes an over-greedy
# SingleQuotedString that spans past the heredoc delimiter. The lexer
# must correctly handle this by removing the spanning token and
# re-tokenizing the content after the heredoc.
cat <<EOF
It isn't broken
EOF
echo "done"
