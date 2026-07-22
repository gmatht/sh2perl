# Failing Test Notes

## All tests passing (169/169) 🎉

## Previously fixed tests
- 063_11_complex_while_loop.sh, 063_hard_to_parse.sh: Fixed infinite loop in while conditions containing `let` (arithmetic `(( ... ))`) commands inside And/Or trees and as direct conditions. The `let` command generator now sets `$CHILD_ERROR` so that the while loop's exit-code-based condition check works correctly. Also fixed direct `while ((...))` handling by negating the exit-code condition to match Perl's truthiness convention, and fixed if-statement combined conditions that use `!()` wrapping.
