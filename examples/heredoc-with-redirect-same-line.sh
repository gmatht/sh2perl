# Demonstrate heredoc with output redirect on same line
# Parser had issues with heredoc body location
cat << _EOF_ > /dev/null
hello world
_EOF_
