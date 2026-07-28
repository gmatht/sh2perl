# Demonstrate heredoc with output redirect on same line
cat << _EOF_ > /dev/null
hello world
_EOF_
printf 'heredoc+same-line-redirect parsed OK\n'
