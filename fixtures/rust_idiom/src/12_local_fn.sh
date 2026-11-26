greet() {
  local who="$1"
  local msg="hello $who"
  echo "$msg"
}
greet world
