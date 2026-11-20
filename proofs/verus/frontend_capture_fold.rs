// Standalone model of `frontend_capture_fold`
// (sh2perl/src/shir_passes/frontend_capture_fold.rs).
//
// A FRONTEND's A1 spells `v = $(echo hello)` as
// `Call { func: "capture", args: [Arrow([Output/exec echo hello])] }` (or a
// `Capture { native: false }`), which the shell pipeline would already have
// folded (`const_capture_fold`). The pass replaces the capture with the
// literal string `fold_const_echo` computes, so no backend has to round-trip
// a compile-time constant through the runtime.
//
// ## What is proved here (the bash-side law, not the code)
//
//   fold_preserves_value: for EVERY expression and EVERY environment, the
//   folded program evaluates to the same bytes. That is the rewrite law — the
//   pass may only swap a subexpression for one with the same value.
//
//   The two things that make it true, and that a naive fold gets wrong, are
//   both bash facts and both pinned:
//
//   - newline_strip_is_load_bearing: command substitution STRIPS trailing
//     newlines, so `$(echo hello)` is `hello`, not the `hello\n` echo printed.
//     A fold that emits the PRINTED bytes (`Str(word ++ "\n")`) has a different
//     value, and the model shows it.
//   - flag_refusal_is_load_bearing: `echo -n x` prints `x` with NO newline, so
//     the plain-echo fold would be wrong for it; the pass refuses `-n` bodies
//     and the model shows the two bodies have different values.
//
//   runtime_body_refusal_is_load_bearing: `$(echo $x)` is a RUNTIME value. A
//   fold that bakes in the current environment would be wrong for the next
//   one; the pass refuses it, and the model exhibits two environments that
//   disagree.
//
//   fold_leaves_no_foldable_capture: COMPLETENESS — every value position is
//   reached (nested containers, both children), so no foldable capture is
//   left behind. `fold_preserves_value` alone would be satisfied by an
//   identity pass, which is why the completeness and idempotence obligations
//   exist beside it.
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - REACHABILITY / REGISTRATION: that the pass runs (`shir_passes::mod`
//   declares it, the pipeline invokes it for the frontend channel) and that
//   frontends emit the capture spelling at all is the registry's business,
//   not the model's.
// - THE VERDICT ITSELF: `const_capture_fold::fold_const_echo` decides which
//   bodies are constant (one literal `echo`/`printf` body, no flags, no
//   redirects, an all-literal words). The model models the three verdicts it
//   produces (constant word, `-n`/flagged body, runtime word) rather than
//   re-deriving the parser-level test.
// - THE STDOUT MODEL: `echo WORD` prints WORD then a newline (unless `-n`);
//   `printf`-style formats, multi-statement Arrow bodies, redirects and
//   `&&`/`||` chains are refused by the source and are outside the model.
// - CORPUS DOMAIN: whether any frontend program reaches this pass is a
//   coverage question; the law is universally quantified over the modelled
//   expressions.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 frontend_capture_fold.rs
//   -> verification results:: 18 verified, 0 errors
//
//   Mutants (checked, measured on copies under the same flags):
//     - identity fold (nothing is folded): 15 verified, 3 errors — the
//       completeness obligation and the constant-echo pin.
//     - fold to the PRINTED bytes (no newline strip): 14 verified, 4 errors —
//       `fold_preserves_value` and `newline_strip_is_load_bearing`.
//     - fold `-n` bodies with the plain-echo verdict: 17 verified, 1 error.
//     - fold a runtime word (bake the environment in): 15 verified, 3 errors.
//   Restored and re-checked at 18 verified, 0 errors.
use vstd::prelude::*;

verus! {

// The word an `echo` is given: a literal (foldable) or a runtime value.
pub enum W {
    Lit(Seq<int>),
    Var(nat),
}

// An expression. `Echo`/`EchoN` are the Arrow body's statement (`echo WORD`
// and `echo -n WORD`); `Cap` is the capture around it; `Str` is the folded
// literal; `Var` is a runtime read in expression position; `Cat` is any
// two-position container (interpolation, binop, argument list).
pub enum X {
    Echo(W),
    EchoN(W),
    Cap(Box<X>),
    Str(Seq<int>),
    Var(nat),
    Cat(Box<X>, Box<X>),
}

pub open spec fn is_nl(c: int) -> bool {
    c == 10
}

// Command substitution strips ALL trailing newlines.
pub open spec fn strip_nl(t: Seq<int>) -> Seq<int>
    decreases t.len(),
{
    if t.len() > 0 && is_nl(t[t.len() - 1]) {
        strip_nl(t.subrange(0, t.len() - 1))
    } else {
        t
    }
}

// The bytes the word contributes.
pub open spec fn word_bytes(w: W, env: Map<nat, Seq<int>>) -> Seq<int> {
    match w {
        W::Lit(s) => s,
        W::Var(n) => env[n],
    }
}

// `echo WORD` prints WORD and a newline; `echo -n WORD` prints just WORD.
pub open spec fn echo_out(w: W, env: Map<nat, Seq<int>>) -> Seq<int> {
    word_bytes(w, env).push(10)
}

pub open spec fn echo_n_out(w: W, env: Map<nat, Seq<int>>) -> Seq<int> {
    word_bytes(w, env)
}

// The value of an expression.
pub open spec fn value(x: X, env: Map<nat, Seq<int>>) -> Seq<int>
    decreases x,
{
    match x {
        X::Echo(w) => echo_out(w, env),
        X::EchoN(w) => echo_n_out(w, env),
        // a command substitution strips the trailing newline(s)
        X::Cap(i) => strip_nl(value(*i, env)),
        X::Str(s) => s,
        X::Var(n) => env[n],
        X::Cat(a, b) => value(*a, env) + value(*b, env),
    }
}

// ---- the pass ----

// The fold's verdict on a capture body: only a plain `echo` of a literal word
// is constant.
pub open spec fn foldable(body: X) -> bool {
    match body {
        X::Echo(W::Lit(_)) => true,
        _ => false,
    }
}

// `fold_const_echo` emits the stripped output of the constant echo.
pub open spec fn folded_literal(w: W) -> Seq<int> {
    match w {
        W::Lit(s) => strip_nl(s),
        W::Var(_) => Seq::empty(),
    }
}

pub open spec fn fold(x: X) -> X
    decreases x,
{
    match x {
        X::Echo(_) => x,
        X::EchoN(_) => x,
        X::Str(_) => x,
        X::Var(_) => x,
        X::Cap(i) => {
            let inner = fold(*i);
            if foldable(inner) {
                match inner {
                    X::Echo(w) => X::Str(folded_literal(w)),
                    _ => X::Cap(Box::new(inner)),
                }
            } else {
                X::Cap(Box::new(inner))
            }
        }
        X::Cat(a, b) => X::Cat(Box::new(fold(*a)), Box::new(fold(*b))),
    }
}

// ---- the law ----

// Stripping the newline the echo printed is the same as stripping the word's
// own trailing newlines: `strip(w ++ "\n") == strip(w)`.
proof fn strip_after_newline(w: Seq<int>)
    ensures strip_nl(w.push(10)) == strip_nl(w),
{
    reveal_with_fuel(strip_nl, 4);
    assert(w.push(10).len() > 0);
    assert(w.push(10)[w.push(10).len() - 1] == 10);
    assert(w.push(10).subrange(0, w.push(10).len() - 1) == w);
}

// THE REWRITE LAW: the fold never changes what the expression evaluates to,
// for any environment.
proof fn fold_preserves_value(x: X, env: Map<nat, Seq<int>>)
    ensures value(fold(x), env) == value(x, env),
    decreases x,
{
    match x {
        X::Echo(_) => {}
        X::EchoN(_) => {}
        X::Str(_) => {}
        X::Var(_) => {}
        X::Cap(i) => {
            fold_preserves_value(*i, env);
            let inner = fold(*i);
            assert(value(*i, env) == value(inner, env));
            if foldable(inner) {
                match inner {
                    X::Echo(w) => {
                        assert(value(X::Echo(w), env) == echo_out(w, env));
                        reveal_with_fuel(strip_nl, 4);
                        // fold emits Str(strip_nl(word)); bash strips the
                        // echo's printed newline
                        match w {
                            W::Lit(s) => {
                                strip_after_newline(s);
                                assert(value(X::Str(strip_nl(s)), env) == strip_nl(s));
                                assert(strip_nl(echo_out(w, env)) == strip_nl(s));
                            }
                            W::Var(_) => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        X::Cat(a, b) => {
            fold_preserves_value(*a, env);
            fold_preserves_value(*b, env);
        }
    }
}

// ---- the naive alternatives go red ----

// The PRINTED bytes: what a fold that forgets the substitution's stripping
// would emit.
pub open spec fn naive_fold(body: X) -> X {
    match body {
        X::Echo(W::Lit(s)) => X::Str(s.push(10)),
        _ => X::Str(Seq::empty()),
    }
}

pub open spec fn hello() -> Seq<int> {
    seq![104, 101, 108, 108, 111]
}

// `$(echo hello)` is `hello`; the printed bytes are `hello\n`, and the naive
// fold has a different value. This is the obligation that pins the newline
// strip: without it the fold would be wrong for EVERY constant capture.
proof fn newline_strip_is_load_bearing()
    ensures ({
        let env = Map::<nat, Seq<int>>::empty();
        let prog = X::Cap(Box::new(X::Echo(W::Lit(hello()))));
        // bash: the substitution drops the newline
        value(prog, env) == hello()
            // the pass emits exactly that
            && value(fold(prog), env) == hello()
            // the printed bytes are longer, and the naive fold is wrong
            && value(X::Echo(W::Lit(hello())), env) == hello().push(10)
            && value(naive_fold(X::Echo(W::Lit(hello()))), env) == hello().push(10)
            && value(naive_fold(X::Echo(W::Lit(hello()))), env) != value(prog, env)
    }),
{
    let env = Map::<nat, Seq<int>>::empty();
    let prog = X::Cap(Box::new(X::Echo(W::Lit(hello()))));
    reveal_with_fuel(value, 4);
    reveal_with_fuel(strip_nl, 4);
    reveal_with_fuel(fold, 4);
    assert(value(prog, env) == hello());
    assert(value(fold(prog), env) == hello());
    assert(value(X::Echo(W::Lit(hello())), env) == hello().push(10));
    assert(value(naive_fold(X::Echo(W::Lit(hello()))), env) == hello().push(10));
}

// `echo -n x` prints no newline, so the plain-echo fold must not be applied to
// it: the pass refuses flagged bodies, and the two bodies really do differ.
proof fn flag_refusal_is_load_bearing()
    ensures ({
        let env = Map::<nat, Seq<int>>::empty();
        let w = W::Lit(seq![120]); // "x"
        // $-less plain echo: "x\n" -> substitution "x"
        value(X::Cap(Box::new(X::Echo(w))), env) == seq![120]
            // `-n`: exactly "x"
            && value(X::Cap(Box::new(X::EchoN(w))), env) == seq![120]
            // the printed bytes differ, so one verdict cannot serve both
            && value(X::Echo(w), env) != value(X::EchoN(w), env)
            // and the pass keeps the `-n` capture rather than guessing
            && fold(X::Cap(Box::new(X::EchoN(w)))) == X::Cap(Box::new(X::EchoN(w)))
    }),
{
    let env = Map::<nat, Seq<int>>::empty();
    reveal_with_fuel(value, 4);
    reveal_with_fuel(strip_nl, 4);
    reveal_with_fuel(fold, 4);
}

// `$(echo $x)` is a runtime value: folding it would bake in one environment's
// bytes, which the next environment contradicts. The pass keeps the capture.
proof fn runtime_body_refusal_is_load_bearing()
    ensures ({
        let w = W::Var(0);
        let prog = X::Cap(Box::new(X::Echo(w)));
        let env1 = Map::<nat, Seq<int>>::empty().insert(0, seq![97]);
        let env2 = Map::<nat, Seq<int>>::empty().insert(0, seq![98]);
        // two environments, two values — so no literal can replace the capture
        value(prog, env1) == seq![97]
            && value(prog, env2) == seq![98]
            && value(prog, env1) != value(prog, env2)
            // and the pass does not fold it
            && fold(prog) == prog
    }),
{
    let w = W::Var(0);
    let prog = X::Cap(Box::new(X::Echo(w)));
    let env1 = Map::<nat, Seq<int>>::empty().insert(0, seq![97]);
    let env2 = Map::<nat, Seq<int>>::empty().insert(0, seq![98]);
    reveal_with_fuel(value, 4);
    reveal_with_fuel(strip_nl, 4);
    reveal_with_fuel(fold, 4);
}

// ---- completeness and idempotence ----

// No foldable capture is left anywhere in the expression.
pub open spec fn no_foldable_capture(x: X) -> bool
    decreases x,
{
    match x {
        X::Echo(_) => true,
        X::EchoN(_) => true,
        X::Str(_) => true,
        X::Var(_) => true,
        X::Cap(i) => !foldable(*i) && no_foldable_capture(*i),
        X::Cat(a, b) => no_foldable_capture(*a) && no_foldable_capture(*b),
    }
}

proof fn fold_leaves_no_foldable_capture(x: X)
    ensures no_foldable_capture(fold(x)),
    decreases x,
{
    match x {
        X::Echo(_) => {}
        X::EchoN(_) => {}
        X::Str(_) => {}
        X::Var(_) => {}
        X::Cap(i) => {
            fold_leaves_no_foldable_capture(*i);
        }
        X::Cat(a, b) => {
            fold_leaves_no_foldable_capture(*a);
            fold_leaves_no_foldable_capture(*b);
        }
    }
}

// A second run changes nothing: after the fold there is no foldable capture
// left.
pub open spec fn idempotent_at(x: X) -> bool
    decreases x,
{
    match x {
        X::Echo(_) => true,
        X::EchoN(_) => true,
        X::Str(_) => true,
        X::Var(_) => true,
        X::Cap(i) => idempotent_at(*i),
        X::Cat(a, b) => idempotent_at(*a) && idempotent_at(*b),
    }
}

proof fn fold_is_idempotent(x: X)
    ensures fold(fold(x)) == fold(x),
    decreases x,
{
    reveal_with_fuel(fold, 2);
    match x {
        X::Echo(_) => {}
        X::EchoN(_) => {}
        X::Str(_) => {}
        X::Var(_) => {}
        X::Cap(i) => {
            fold_is_idempotent(*i);
            reveal_with_fuel(fold, 3);
        }
        X::Cat(a, b) => {
            fold_is_idempotent(*a);
            fold_is_idempotent(*b);
        }
    }
}

// ---- pins: the source's tests ----

// a_constant_echo_capture_folds_to_its_output.
proof fn pin_constant_echo_folds()
    ensures fold(X::Cap(Box::new(X::Echo(W::Lit(hello())))))
        == X::Str(hello()),
{
    reveal_with_fuel(fold, 4);
    reveal_with_fuel(strip_nl, 4);
    assert(strip_nl(hello()) == hello()) by (compute_only);
}

// a_variable_word_keeps_the_capture.
proof fn pin_variable_word_keeps_capture()
    ensures fold(X::Cap(Box::new(X::Echo(W::Var(0)))))
        == X::Cap(Box::new(X::Echo(W::Var(0)))),
{
    reveal_with_fuel(fold, 4);
}

// a_real_command_keeps_the_capture: modelled as a runtime value in expression
// position, i.e. a capture of something that is not an echo at all.
proof fn pin_real_command_keeps_capture()
    ensures fold(X::Cap(Box::new(X::Var(3)))) == X::Cap(Box::new(X::Var(3))),
{
    reveal_with_fuel(fold, 4);
}

// an_echo_with_a_flag_keeps_the_capture.
proof fn pin_flagged_echo_keeps_capture()
    ensures fold(X::Cap(Box::new(X::EchoN(W::Lit(hello())))))
        == X::Cap(Box::new(X::EchoN(W::Lit(hello())))),
{
    reveal_with_fuel(fold, 4);
}

// The fold reaches a capture nested in a two-position container (the
// frontend's `v = "x" + $(echo hi)` shape), which is what makes the walk
// complete rather than top-level only.
proof fn pin_folds_nested_capture()
    ensures fold(X::Cat(
        Box::new(X::Str(seq![118, 61])),
        Box::new(X::Cap(Box::new(X::Echo(W::Lit(hello()))))),
    )) == X::Cat(
        Box::new(X::Str(seq![118, 61])),
        Box::new(X::Str(hello())),
    ),
{
    reveal_with_fuel(fold, 6);
    reveal_with_fuel(strip_nl, 4);
    assert(strip_nl(hello()) == hello()) by (compute_only);
}

fn main() {}
}
