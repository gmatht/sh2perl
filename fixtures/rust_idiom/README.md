# Rust backend idiom fixtures

Small shell programs whose *rendered Rust* is checked in, so the shape of
the backend's output can be reviewed and diffed like ordinary code.

    src/*.sh          the shell input
    expected/*.rs     what `rust_backend::shir_to_rust` emits for it

Re-render after a backend change, then look at the diff:

    cargo run --bin rust_fixtures            # rewrite expected/*.rs
    git diff fixtures/rust_idiom/expected

Verify the checked-in output is current *and* that it compiles warning-free:

    cargo run --bin rust_fixtures -- --check

The render path is the same one `shir_render --target rust` uses:

    parse_commands_from_text -> shir::ast_to_ir -> transforms::apply
      -> restructure_goto_only -> strip_cfor -> rust_backend::shir_to_rust

Every fixture is compiled with `rustc --edition 2021 -D warnings`, except
those whose output legitimately needs a third-party crate in the *generated*
program's own manifest. Those are reported as `NEEDS-CRATE <name>`:

* `08_bigint.sh` — `$((9223372036854775807 + 1))` cannot be an `i64`, so the
  renderer emits `num_bigint::BigInt` (already a dependency of this crate).
  It compiles clean in a crate that has `num-bigint` (verified), but its
  output is *not* bash-identical: bash wraps to `-9223372036854775808`,
  the BigInt lowering prints the exact `9223372036854775808`. This fixture
  pins the render shape, not behavioural equality.

Adding a fixture: drop `NN_something.sh` in `src/`, run the harness without
`--check`, commit both files.

## Known-best-effort fixtures

`09_regex_group.sh` and `10_regex_alt.sh` use `[[ "$x" =~ ^v([0-9]+)... ]]` —
a parenthesized group in a `[[ ]]` test, which the front end does not lower
yet, so they render a `// TODO(unsupported)` marker. They are kept because
they pin two things: the native ERE matcher is *not* reached for them (they
must therefore still compile), and the dead `let` left behind by the
unsupported construct is removed rather than warned about.
