// Standalone model of `frontend_int_lift`
// (sh2perl/src/shir_passes/frontend_int_lift.rs).
//
// The imperative frontends spell a source number as the STRING "42" and record
// the domain only in the A2 verdict `prog.var_types[name] = Int`. Renderers read
// the shell convention instead — an `Int` node — so the pass rewrites the
// numeric SPELLING to the numeric NODE: a definition `Str("42")` of an int-typed
// name becomes `Int(42)`, and a literal `Arith` tree whose value is exact in i64
// becomes an `Int` literal. The value never changes; only which node spells it.
//
// ## The law
//
//   txt(x) is the text the expression denotes. fold(x) is the pass's rewrite.
//   fold_preserves_text: for every expression and every environment the text
//   is unchanged. On top of that, lift_preserves_text says the same for a
//   whole statement list, which is where the pass's SCOPE lives: it rewrites
//   only the top-level value of a single-target assignment (or a singleton
//   declare) of a name that is
//     (a) A2-typed Int, and
//     (b) written exactly ONCE in the program (count_writes == 1).
//   A statement outside those shapes is not even walked (`_ => continue`), so a
//   foldable value inside it survives — the model pins that too, because it is
//   the difference between this pass and a program-wide rewrite.
//
//   Three obligations carry the content, and each is pinned by a mutant that
//   goes red:
//
//   - decimal_spelling_is_canonical: the pass folds `Str(s)` only when `s` is
//     exactly the canonical decimal spelling of the integer (`dec(k)`). A
//     permissive parser — `str::parse::<i64>`, which accepts "007" and "+7" —
//     would fold to `Int(7)`, whose text is "7", so "007" would silently
//     change meaning. The mutant is red.
//   - checked_is_exact: `arith_eval` evaluates a literal `+ - *` tree ONLY when
//     every intermediate fits i64 (else it refuses, because the source
//     language's arithmetic is unbounded). checked_exact proves the accepted
//     value equals the mathematical one, and the overflow witness shows a
//     wrapping evaluator writes a DIFFERENT integer (i64::MAX + 1). The mutant
//     is red.
//   - a runtime-valued tree cannot be folded: `arith_eval` returns None for a
//     `Var` leaf, so a tree containing an int-typed NAME is not folded at all.
//     Treating the name as 0 (a plausible slip) changes the value under a
//     non-zero environment; the mutant is red.
//
//   An identity pass is red on the completeness obligation (a foldable
//   definition must not survive).
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - REACHABILITY / REGISTRATION: that the pass runs (it is a `Transform` in
//   the shIR pass list, invoked for the frontend channel) is the pipeline's
//   business.
// - THE A2 VERDICT ITSELF: that `frontend_types::int_names(prog)` puts a name
//   in the int set only when the frontend's A2 analysis really typed it `Int`
//   (so its value is an integer, and no string position consumes it) is the
//   frontend's contract, not the pass's. The model takes the set as given
//   (`ints`).
// - THE PARSER: `frontend_types::int_literal` refuses signs, floats, `0x`,
//   underscores and leading zeros, and range-checks i64. The model renders that
//   verdict as `parse(s) = Some(dig_val s)` for `canonical(s)` inside i64
//   range; `decimal_spelling_is_canonical` is the lemma that such a spelling
//   renders back to itself, and `parse_permissive` is the mutant that shows
//   what a looser parser breaks.
// - THE WRITE COUNT: `count_writes` walks every statement shape that can bind
//   a name (Assign targets, Declare vars, DeclareArray, and the bodies of
//   If/Try/Case/loops/Function). The model takes the count as given and pins
//   that `== 1` is required.
// - CORPUS DOMAIN: whether a program reaches a foldable definition is a
//   coverage question.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 frontend_int_lift.rs
//   -> verification results:: 33 verified, 0 errors
//
//   Mutants (spec edited on copies outside the tree, same flags):
//     - permissive parser (accepts a leading-zero spelling): 31 verified,
//       2 errors.
//     - wrapping arith evaluation (folds an overflowed value): 28 / 5.
//     - folding a runtime-valued tree (treats the name as 0): 31 / 2.
//     - identity pass (nothing is lifted): 27 / 6.
use vstd::prelude::*;

verus! {

// ---------- decimal spellings ----------

pub open spec fn in_i64(v: int) -> bool {
    -9223372036854775808 <= v <= 9223372036854775807
}

// Digits-only, as a pointwise predicate (no recursion, so no fuel).
pub open spec fn all_digits(s: Seq<int>) -> bool {
    forall|i: int| 0 <= i < s.len() ==> 48 <= s[i] && s[i] <= 57
}

// The value of a digit string, read left to right.
pub open spec fn dig_val(s: Seq<int>) -> int
    decreases s.len(),
{
    if s.len() == 0 {
        0
    } else {
        dig_val(s.subrange(0, s.len() as int - 1)) * 10 + (s[s.len() as int - 1] - 48)
    }
}

// A canonical spelling: digits only, and no leading zero (except "0" itself).
pub open spec fn canonical(s: Seq<int>) -> bool {
    s.len() > 0 && all_digits(s) && (s.len() == 1 || s[0] != 48)
}

// The canonical decimal spelling of a non-negative integer.
pub open spec fn dec(k: int) -> Seq<int>
    decreases k,
{
    if k < 10 {
        seq![48 + k]
    } else {
        dec(k / 10) + seq![48 + k % 10]
    }
}

// Signed rendering, so a negative Arith value has a text too.
pub open spec fn repr(v: int) -> Seq<int> {
    if v >= 0 {
        dec(v)
    } else {
        seq![45] + dec(-v)
    }
}

// The parser's verdict, as the pass consumes it: canonical spelling, i64 range.
pub open spec fn parse(s: Seq<int>) -> Option<int> {
    if canonical(s) && in_i64(dig_val(s)) {
        Some(dig_val(s))
    } else {
        None
    }
}

// ---------- lemmas about the spelling ----------

proof fn dig_val_nonneg(s: Seq<int>)
    requires all_digits(s),
    ensures 0 <= dig_val(s),
    decreases s.len(),
{
    reveal_with_fuel(dig_val, 2);
    if s.len() > 0 {
        let pref = s.subrange(0, s.len() as int - 1);
        assert forall|i: int| 0 <= i < pref.len() implies 48 <= pref[i] && pref[i] <= 57 by {
            assert(pref[i] == s[i]);
        }
        dig_val_nonneg(pref);
    }
}

proof fn all_digits_at(s: Seq<int>, i: int)
    requires all_digits(s), 0 <= i < s.len(),
    ensures 48 <= s[i] <= 57,
{
}

proof fn all_digits_prefix(s: Seq<int>)
    requires all_digits(s), s.len() > 0,
    ensures all_digits(s.subrange(0, s.len() as int - 1)),
        all_digits(s.subrange(1, s.len() as int)),
{
    let n = s.len() as int;
    let pref = s.subrange(0, n - 1);
    let rest = s.subrange(1, n);
    assert forall|i: int| 0 <= i < pref.len() implies 48 <= pref[i] && pref[i] <= 57 by {
        assert(pref[i] == s[i]);
    }
    assert forall|i: int| 0 <= i < rest.len() implies 48 <= rest[i] && rest[i] <= 57 by {
        assert(rest[i] == s[i + 1]);
    }
}

// A spelling whose first byte is not '0' has a positive value.
proof fn dig_val_positive(s: Seq<int>)
    requires all_digits(s), s.len() > 0, s[0] != 48,
    ensures dig_val(s) >= 1,
    decreases s.len(),
{
    reveal_with_fuel(dig_val, 2);
    let n = s.len() as int;
    if n == 1 {
        all_digits_at(s, 0);
        assert(dig_val(s) == s[0] - 48);
    } else {
        let pref = s.subrange(0, n - 1);
        all_digits_prefix(s);
        assert(pref.len() > 0);
        assert(pref[0] == s[0]);
        dig_val_positive(pref);
        dig_val_nonneg(pref);
        all_digits_at(s, n - 1);
        assert(dig_val(s) == dig_val(pref) * 10 + (s[n - 1] - 48));
        assert(s[n - 1] - 48 >= 0);
    }
}

// The step of the decimal rendering: for a quotient >= 1 and a digit, appending
// the digit is the same as rendering the joined number.
proof fn dec_step(q: int, d: int)
    requires q >= 1, 0 <= d <= 9,
    ensures dec(10 * q + d) == dec(q) + seq![48 + d],
{
    reveal_with_fuel(dec, 2);
    assert(10 * q + d >= 10);
    assert((10 * q + d) / 10 == q) by (nonlinear_arith)
        requires 10 * q + d >= 10, 0 <= d <= 9;
    assert((10 * q + d) % 10 == d) by (nonlinear_arith)
        requires 10 * q + d >= 10, 0 <= d <= 9;
}

// A canonical spelling renders back to itself: the parser's answer is the
// canonical spelling, so replacing the spelling by the node preserves the text.
proof fn decimal_spelling_is_canonical(s: Seq<int>)
    requires canonical(s),
    ensures dec(dig_val(s)) == s,
    decreases s.len(),
{
    reveal_with_fuel(dig_val, 2);
    let n = s.len() as int;
    if n == 1 {
        all_digits_at(s, 0);
        assert(dig_val(s) == s[0] - 48);
        assert(0 <= s[0] - 48 <= 9);
        assert(s =~= seq![s[0]]);
        assert(dec(dig_val(s)) == seq![48 + (s[0] - 48)]);
    } else {
        let pref = s.subrange(0, n - 1);
        let last = s[n - 1];
        assert(s =~= pref + seq![last]);
        assert(dig_val(s) == dig_val(pref) * 10 + (last - 48));
        all_digits_prefix(s);
        all_digits_at(s, n - 1);
        assert(pref.len() > 0);
        assert(pref[0] == s[0]);
        assert(pref[0] != 48);
        assert(all_digits(pref));
        assert(canonical(pref));
        decimal_spelling_is_canonical(pref);
        dig_val_positive(pref);
        assert(dig_val(pref) >= 1);
        assert(0 <= last - 48 <= 9);
        dec_step(dig_val(pref), last - 48);
        assert(dec(dig_val(pref) * 10 + (last - 48))
            == dec(dig_val(pref)) + seq![48 + (last - 48)]);
        assert(seq![48 + (last - 48)] == seq![last]);
    }
}

// ---------- the arithmetic side ----------

// A literal arith tree. `Div`/`Mod` are absent on purpose: `arith_is_int`
// admits only `+ - *` (the source language's `/` is not truncating, so an
// integer node could not carry it), and `arith_eval` refuses a `Var` leaf.
pub enum A {
    Num(int),
    Var(nat),
    Add(Box<A>, Box<A>),
    Sub(Box<A>, Box<A>),
    Mul(Box<A>, Box<A>),
}

// The mathematical value (unbounded, as the source language's arithmetic is).
pub open spec fn ev(a: A, env: Map<nat, int>) -> int
    decreases a,
{
    match a {
        A::Num(n) => n,
        A::Var(n) => env[n],
        A::Add(l, r) => ev(*l, env) + ev(*r, env),
        A::Sub(l, r) => ev(*l, env) - ev(*r, env),
        A::Mul(l, r) => ev(*l, env) * ev(*r, env),
    }
}

// Mirror of `arith_is_int`: every leaf a literal, every operator `+ - *`.
pub open spec fn is_lit_ast(a: A) -> bool
    decreases a,
{
    match a {
        A::Num(_) => true,
        A::Var(_) => false,
        A::Add(l, r) => is_lit_ast(*l) && is_lit_ast(*r),
        A::Sub(l, r) => is_lit_ast(*l) && is_lit_ast(*r),
        A::Mul(l, r) => is_lit_ast(*l) && is_lit_ast(*r),
    }
}

// Mirror of `arith_eval`: exact within i64, `None` on overflow or a Var leaf.
pub open spec fn checked(a: A) -> Option<int>
    decreases a,
{
    match a {
        A::Num(n) => if in_i64(n) {
            Some(n)
        } else {
            None
        },
        A::Var(_) => None,
        A::Add(l, r) => match (checked(*l), checked(*r)) {
            (Some(x), Some(y)) => if in_i64(x + y) {
                Some(x + y)
            } else {
                None
            },
            _ => None,
        },
        A::Sub(l, r) => match (checked(*l), checked(*r)) {
            (Some(x), Some(y)) => if in_i64(x - y) {
                Some(x - y)
            } else {
                None
            },
            _ => None,
        },
        A::Mul(l, r) => match (checked(*l), checked(*r)) {
            (Some(x), Some(y)) => if in_i64(x * y) {
                Some(x * y)
            } else {
                None
            },
            _ => None,
        },
    }
}

// THE arithmetic obligation: an accepted value IS the mathematical value. This
// is what makes the refusal-on-overflow rule sound rather than merely cautious.
proof fn checked_is_exact(a: A, env: Map<nat, int>)
    ensures checked(a) is Some ==> ev(a, env) == checked(a).unwrap(),
    decreases a,
{
    match a {
        A::Num(_) => {}
        A::Var(_) => {}
        A::Add(l, r) => {
            checked_is_exact(*l, env);
            checked_is_exact(*r, env);
            if checked(*l) is Some && checked(*r) is Some {
                assert(ev(*l, env) == checked(*l).unwrap());
                assert(ev(*r, env) == checked(*r).unwrap());
            }
        }
        A::Sub(l, r) => {
            checked_is_exact(*l, env);
            checked_is_exact(*r, env);
            if checked(*l) is Some && checked(*r) is Some {
                assert(ev(*l, env) == checked(*l).unwrap());
                assert(ev(*r, env) == checked(*r).unwrap());
            }
        }
        A::Mul(l, r) => {
            checked_is_exact(*l, env);
            checked_is_exact(*r, env);
            if checked(*l) is Some && checked(*r) is Some {
                assert(ev(*l, env) == checked(*l).unwrap());
                assert(ev(*r, env) == checked(*r).unwrap());
            }
        }
    }
}

// ---------- expressions and the fold ----------

pub enum X {
    Str(Seq<int>),
    Int(int),
    Arith(A),
}

// The text the expression denotes.
pub open spec fn txt(x: X, env: Map<nat, int>) -> Seq<int>
    decreases x,
{
    match x {
        X::Str(s) => s,
        X::Int(k) => repr(k),
        X::Arith(a) => repr(ev(a, env)),
    }
}

pub open spec fn fold(x: X) -> X
    decreases x,
{
    match x {
        X::Str(s) => match parse(s) {
            Some(k) => X::Int(k),
            None => X::Str(s),
        },
        X::Int(_) => x,
        X::Arith(a) => if is_lit_ast(a) {
            match checked(a) {
                Some(v) => X::Int(v),
                None => X::Arith(a),
            }
        } else {
            X::Arith(a)
        },
    }
}

// ---------- the law ----------

proof fn fold_preserves_text(x: X, env: Map<nat, int>)
    ensures txt(fold(x), env) == txt(x, env),
    decreases x,
{
    match x {
        X::Str(s) => {
            if canonical(s) && in_i64(dig_val(s)) {
                decimal_spelling_is_canonical(s);
                assert(parse(s) == Some(dig_val(s)));
                assert(fold(x) == X::Int(dig_val(s)));
                assert(dig_val(s) >= 0);
                assert(repr(dig_val(s)) == dec(dig_val(s)));
                assert(txt(fold(x), env) == s);
            } else {
                assert(parse(s) is None);
                assert(fold(x) == X::Str(s));
            }
        }
        X::Int(_) => {}
        X::Arith(a) => {
            if is_lit_ast(a) {
                checked_is_exact(a, env);
                if checked(a) is Some {
                    assert(ev(a, env) == checked(a).unwrap());
                }
            }
        }
    }
}

// ---------- the pass's scope: which definitions it touches ----------

// A statement, at the granularity the pass distinguishes: the two definition
// shapes it rewrites, and everything else it does not even walk.
pub enum Stmt {
    Assign(nat, X),
    Declare(nat, X),
    Other(X),
}

pub open spec fn stmt_txt(s: Stmt, env: Map<nat, int>) -> Seq<int> {
    match s {
        Stmt::Assign(_, v) => txt(v, env),
        Stmt::Declare(_, v) => txt(v, env),
        Stmt::Other(v) => txt(v, env),
    }
}

// `ints` is the A2 verdict set, `writes` the `count_writes` census.
pub open spec fn lift_stmt(s: Stmt, ints: Set<nat>, writes: Map<nat, int>) -> Stmt {
    match s {
        Stmt::Assign(n, v) => if ints.contains(n) && writes[n] == 1 {
            Stmt::Assign(n, fold(v))
        } else {
            s
        },
        Stmt::Declare(n, v) => if ints.contains(n) && writes[n] == 1 {
            Stmt::Declare(n, fold(v))
        } else {
            s
        },
        // the pass reaches no value inside any other statement shape
        Stmt::Other(_) => s,
    }
}

// The lifted list, pointwise: the rewrite depends only on the statement itself.
pub open spec fn lift_stmts(ss: Seq<Stmt>, ints: Set<nat>, writes: Map<nat, int>) -> Seq<Stmt> {
    Seq::new(ss.len(), |i: int| lift_stmt(ss[i], ints, writes))
}

proof fn lift_preserves_statement(s: Stmt, ints: Set<nat>, writes: Map<nat, int>, env: Map<nat, int>)
    ensures stmt_txt(lift_stmt(s, ints, writes), env) == stmt_txt(s, env),
{
    match s {
        Stmt::Assign(n, v) => {
            if ints.contains(n) && writes[n] == 1 {
                fold_preserves_text(v, env);
            }
        }
        Stmt::Declare(n, v) => {
            if ints.contains(n) && writes[n] == 1 {
                fold_preserves_text(v, env);
            }
        }
        Stmt::Other(_) => {}
    }
}

// The program-level law: the lifted statement list denotes the same texts.
proof fn lift_preserves_text(
    ss: Seq<Stmt>,
    ints: Set<nat>,
    writes: Map<nat, int>,
    env: Map<nat, int>,
)
    requires ss.len() > 0,
    ensures forall|i: int| 0 <= i < ss.len() ==> stmt_txt(lift_stmts(ss, ints, writes)[i], env)
        == stmt_txt(ss[i], env),
{
    assert forall|i: int| 0 <= i < ss.len() implies
        stmt_txt(lift_stmts(ss, ints, writes)[i], env) == stmt_txt(ss[i], env) by {
        assert(lift_stmts(ss, ints, writes)[i] == lift_stmt(ss[i], ints, writes));
        lift_preserves_statement(ss[i], ints, writes, env);
    }
}

// ---------- completeness and idempotence ----------

pub open spec fn foldable(x: X) -> bool {
    match x {
        X::Str(s) => parse(s) is Some,
        X::Int(_) => false,
        X::Arith(a) => is_lit_ast(a) && checked(a) is Some,
    }
}

pub open spec fn settled(x: X) -> bool {
    !foldable(x)
}

proof fn fold_leaves_nothing(x: X)
    ensures settled(fold(x)),
{
    match x {
        X::Str(s) => {
            if canonical(s) && in_i64(dig_val(s)) {
                assert(fold(x) == X::Int(dig_val(s)));
                assert(!foldable(X::Int(dig_val(s))));
            }
        }
        X::Int(_) => {}
        X::Arith(a) => {
            if is_lit_ast(a) && checked(a) is Some {
                assert(fold(x) == X::Int(checked(a).unwrap()));
                assert(!foldable(X::Int(checked(a).unwrap())));
            }
        }
    }
}

proof fn fold_is_idempotent(x: X)
    ensures fold(fold(x)) == fold(x),
{
}

// ---------- the naive alternatives go red ----------

// MUTANT A: a permissive parser — `str::parse::<i64>` accepts leading zeros.
// It reports the same integer, but the spelling is not what that integer
// renders as, so the rewrite changes the text.
pub open spec fn parse_permissive(s: Seq<int>) -> Option<int> {
    if canonical(s) && in_i64(dig_val(s)) {
        Some(dig_val(s))
    } else if s.len() > 1 && s[0] == 48 && all_digits(s.subrange(1, s.len() as int))
        && in_i64(dig_val(s))
    {
        Some(dig_val(s))
    } else {
        None
    }
}

proof fn leading_zero_is_load_bearing()
    ensures parse(seq![48, 55]) is None
        && parse_permissive(seq![48, 55]) == Some(7)
        && dig_val(seq![48, 55]) == 7
        && repr(7) == seq![55]
        && repr(7) != seq![48, 55]
        && txt(X::Int(7), Map::<nat, int>::empty()) == seq![55]
        && txt(X::Str(seq![48, 55]), Map::<nat, int>::empty()) == seq![48, 55],
{
    reveal_with_fuel(dig_val, 4);
    reveal_with_fuel(dec, 2);
}

// MUTANT B: wrapping arithmetic — folds an overflowed value as if the source
// language's arithmetic were 64-bit.
pub open spec fn wrap64(v: int) -> int {
    if in_i64(v) {
        v
    } else if v > 9223372036854775807 {
        v - 18446744073709551616
    } else {
        v + 18446744073709551616
    }
}

pub open spec fn wrap_eval(a: A) -> int
    decreases a,
{
    match a {
        A::Num(n) => wrap64(n),
        A::Var(_) => 0,
        A::Add(l, r) => wrap64(wrap_eval(*l) + wrap_eval(*r)),
        A::Sub(l, r) => wrap64(wrap_eval(*l) - wrap_eval(*r)),
        A::Mul(l, r) => wrap64(wrap_eval(*l) * wrap_eval(*r)),
    }
}

proof fn overflow_is_load_bearing()
    ensures ev(
            A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1))),
            Map::<nat, int>::empty(),
        ) == 9223372036854775808
        && checked(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1)))) is None
        && is_lit_ast(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1))))
        && wrap_eval(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1))))
            == -9223372036854775808
        && wrap_eval(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1))))
            != ev(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1))), Map::<nat, int>::empty())
        && fold(X::Arith(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1)))))
            == X::Arith(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1)))),
{
    reveal_with_fuel(checked, 4);
    reveal_with_fuel(is_lit_ast, 4);
    reveal_with_fuel(wrap_eval, 4);
    reveal_with_fuel(ev, 4);
}

// MUTANT C: folding a RUNTIME-valued tree by treating the name as 0. Two
// environments disagree, so no literal can replace the tree.
pub open spec fn is_lit_ast_wide(a: A) -> bool
    decreases a,
{
    match a {
        A::Num(_) => true,
        A::Var(_) => true,
        A::Add(l, r) => is_lit_ast_wide(*l) && is_lit_ast_wide(*r),
        A::Sub(l, r) => is_lit_ast_wide(*l) && is_lit_ast_wide(*r),
        A::Mul(l, r) => is_lit_ast_wide(*l) && is_lit_ast_wide(*r),
    }
}

proof fn runtime_leaf_is_load_bearing()
    ensures checked(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0)))) is None
        && is_lit_ast(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0)))) == false
        && is_lit_ast_wide(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0))))
        && ev(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0))), Map::<nat, int>::empty().insert(0, 4)) == 5
        && ev(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0))), Map::<nat, int>::empty().insert(0, 5)) == 6
        && fold(X::Arith(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0)))))
            == X::Arith(A::Add(Box::new(A::Num(1)), Box::new(A::Var(0)))),
{
    reveal_with_fuel(checked, 4);
    reveal_with_fuel(is_lit_ast, 4);
    reveal_with_fuel(is_lit_ast_wide, 4);
    reveal_with_fuel(ev, 4);
}

// MUTANT D: an identity pass — a foldable definition survives it.
proof fn identity_is_not_a_lift()
    ensures foldable(X::Str(seq![52, 50])),
{
    reveal_with_fuel(dig_val, 4);
}

// ---------- pins: the source's tests ----------

// an_int_typed_literal_becomes_an_int_node.
proof fn pin_int_literal_becomes_int()
    ensures fold(X::Str(seq![52, 50])) == X::Int(42),
{
    reveal_with_fuel(dig_val, 4);
}

// an_untyped_literal_is_untouched: the pass only rewrites a name in the A2 int
// set, so the definition of a name outside it is untouched even when foldable.
proof fn pin_untyped_name_untouched()
    ensures lift_stmt(
            Stmt::Assign(0, X::Str(seq![52, 50])),
            Set::<nat>::empty(),
            Map::<nat, int>::empty().insert(0, 1),
        ) == Stmt::Assign(0, X::Str(seq![52, 50]))
        && stmt_txt(lift_stmt(
                Stmt::Assign(0, X::Str(seq![52, 50])),
                Set::<nat>::empty(),
                Map::<nat, int>::empty().insert(0, 1),
            ), Map::<nat, int>::empty()) == seq![52, 50],
{
}

// a_literal_arithmetic_expression_folds_exactly.
proof fn pin_literal_arith_folds()
    ensures fold(X::Arith(A::Add(Box::new(A::Num(1)), Box::new(A::Num(2))))) == X::Int(3)
        && ev(A::Add(Box::new(A::Num(1)), Box::new(A::Num(2))), Map::<nat, int>::empty()) == 3,
{
    reveal_with_fuel(checked, 4);
    reveal_with_fuel(is_lit_ast, 4);
    reveal_with_fuel(ev, 4);
}

// overflow_refuses.
proof fn pin_overflow_refuses()
    ensures fold(X::Arith(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1)))))
        == X::Arith(A::Add(Box::new(A::Num(9223372036854775807)), Box::new(A::Num(1)))),
{
    reveal_with_fuel(checked, 4);
    reveal_with_fuel(is_lit_ast, 4);
}

// a_float_spelling_is_not_an_int.
proof fn pin_float_refused()
    ensures parse(seq![49, 46, 53]) is None
        && fold(X::Str(seq![49, 46, 53])) == X::Str(seq![49, 46, 53]),
{
    reveal_with_fuel(dig_val, 2);
}

// The scope pins: a second write keeps the renderer's own lowering, and a
// statement the pass does not walk keeps a foldable value untouched.
proof fn pin_scope()
    ensures lift_stmt(
            Stmt::Assign(0, X::Str(seq![52, 50])),
            Set::<nat>::empty().insert(0),
            Map::<nat, int>::empty().insert(0, 1),
        ) == Stmt::Assign(0, X::Int(42))
        && lift_stmt(
            Stmt::Assign(0, X::Str(seq![52, 50])),
            Set::<nat>::empty().insert(0),
            Map::<nat, int>::empty().insert(0, 2),
        ) == Stmt::Assign(0, X::Str(seq![52, 50]))
        && lift_stmt(
            Stmt::Other(X::Str(seq![52, 50])),
            Set::<nat>::empty().insert(0),
            Map::<nat, int>::empty().insert(0, 1),
        ) == Stmt::Other(X::Str(seq![52, 50]))
        && lift_stmt(
            Stmt::Other(X::Arith(A::Add(Box::new(A::Num(1)), Box::new(A::Num(2))))),
            Set::<nat>::empty().insert(0),
            Map::<nat, int>::empty().insert(0, 1),
        ) == Stmt::Other(X::Arith(A::Add(Box::new(A::Num(1)), Box::new(A::Num(2))))),
{
    reveal_with_fuel(dig_val, 4);
}

// The whole-list law, on a two-statement program: the lifted value keeps its
// text, and the statement that is not an int-typed single-write definition is
// untouched.
proof fn pin_list_preserves()
    ensures lift_stmts(
            seq![
                Stmt::Assign(0, X::Str(seq![52, 50])),
                Stmt::Assign(1, X::Str(seq![49, 46, 53])),
            ],
            Set::<nat>::empty().insert(0),
            Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
        )[0] == Stmt::Assign(0, X::Int(42))
        && lift_stmts(
            seq![
                Stmt::Assign(0, X::Str(seq![52, 50])),
                Stmt::Assign(1, X::Str(seq![49, 46, 53])),
            ],
            Set::<nat>::empty().insert(0),
            Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
        )[1] == Stmt::Assign(1, X::Str(seq![49, 46, 53]))
        && stmt_txt(
            lift_stmts(
                seq![
                    Stmt::Assign(0, X::Str(seq![52, 50])),
                    Stmt::Assign(1, X::Str(seq![49, 46, 53])),
                ],
                Set::<nat>::empty().insert(0),
                Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
            )[0],
            Map::<nat, int>::empty(),
        ) == seq![52, 50],
{
    reveal_with_fuel(dig_val, 4);
    reveal_with_fuel(dec, 4);
    assert(lift_stmts(
        seq![
            Stmt::Assign(0, X::Str(seq![52, 50])),
            Stmt::Assign(1, X::Str(seq![49, 46, 53])),
        ],
        Set::<nat>::empty().insert(0),
        Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
    )[0] == lift_stmt(
        Stmt::Assign(0, X::Str(seq![52, 50])),
        Set::<nat>::empty().insert(0),
        Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
    ));
    assert(lift_stmt(
        Stmt::Assign(0, X::Str(seq![52, 50])),
        Set::<nat>::empty().insert(0),
        Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
    ) == Stmt::Assign(0, X::Int(42)));
    assert(lift_stmts(
        seq![
            Stmt::Assign(0, X::Str(seq![52, 50])),
            Stmt::Assign(1, X::Str(seq![49, 46, 53])),
        ],
        Set::<nat>::empty().insert(0),
        Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
    )[1] == lift_stmt(
        Stmt::Assign(1, X::Str(seq![49, 46, 53])),
        Set::<nat>::empty().insert(0),
        Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
    ));
    assert(lift_stmt(
        Stmt::Assign(1, X::Str(seq![49, 46, 53])),
        Set::<nat>::empty().insert(0),
        Map::<nat, int>::empty().insert(0, 1).insert(1, 1),
    ) == Stmt::Assign(1, X::Str(seq![49, 46, 53])));
}

fn main() {}
}
