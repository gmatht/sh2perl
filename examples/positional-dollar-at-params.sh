# $@ / $# / $* inside a function — listVar must exist and splat args
# (previously any $@ usage crashed).
show() { echo "count=$#"; echo "all=$*"; for a in "$@"; do echo "arg=$a"; done; }
show one "two three" four
