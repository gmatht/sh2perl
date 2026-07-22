# Failing Test Notes

## All tests passing (169/169) 🎉

## Previously fixed tests
- 063_11_complex_while_loop.sh, 063_hard_to_parse.sh: Fixed infinite loop in while conditions containing `let` (arithmetic `(( ... ))`) commands inside And/Or trees. The `let` command generator did not set `$CHILD_ERROR`, so the while loop's exit-code-based condition check always saw success and never terminated the loop.
