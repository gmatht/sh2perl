// Standalone model of `optimize_stmts` Pass 0.5
// (sh2perl/src/ir.rs: fold_stmt -> fold_expr -> fold_arith_ast,
// with the fold_bin_op / fold_un_op / pow_wrapping tables).
//
// Pass 0.5 runs on EVERY program (fold_stmt maps every statement before
// the self-assign/Declare filters), and it was the one core pass with no
// Verus model: the existing `optimize_stmts.rs` proof covers Pass 1 only.
//
// ## The obligation
//
//   fold_sound: for every expression in the modelled language, folding
//   preserves the denotation: den(fold(a), s) == den(a, s).
//
// and the REFUSAL table, which is what makes the fold safe to ship:
//
//   div_zero_refused / mod_zero_refused / min_div_minus1_refused
//   pow_negative_refused / unknown_op_refused
//
// i.e. `fold_bin_op` answers None on exactly the inputs bash treats as
// runtime errors (division by zero, i64::MIN / -1, a negative exponent)
// and on the operators it does not model. A None keeps the node shape,
// so the refusal half is an identity, proved as `fold_keeps_shape`.
//
// Division is modelled TRUNCATING (tdiv/tmod), because that is what
// Rust and bash do: -7 / 2 is -3, not Verus's floor -4. Two pins cover
// it. `!` is modelled as -x-1 (Rust's `!x`), matching fold_un_op.
//
// ## pow_wrapping
//
// The source's comment claims "repeated squaring over i64 is the same
// product mod 2^64". This model proves:
//
//   pow_sq_eq_naive:      pow_sq(b, e) == pow(b, e)        (e >= 0)
//   pow_wrapping_residue: pow_sq(b, e) % 2^64 == pow(b, e) % 2^64
//
// in vstd's mathematical `pow`. That is the ALGORITHM identity (the
// squaring loop computes the same product as repeated multiplication)
// and its residue form. It does NOT pin Rust's `i64::wrapping_mul`
// chain to that residue: `wrapping_mul` is exec-only (no spec), so the
// tie from "the loop's i64 state" to "the product mod 2^64" is not a
// theorem here. That is the residual gap, recorded in VERUS.md.
//
// ## Scope (read before quoting)
//
// - Model, not code: the table is transcribed by hand from
//   `fold_bin_op`/`fold_un_op`; `optimize_fold_sync.txt` records the
//   source arms it claims so drift is caught, but the transcription is
//   still manual. `fold_arith_const` (the arithmetic STRING parser) is
//   an oracle: the model starts from an already-parsed tree.
// - Domain: the arms `+ - * ** << >>` are modelled as their
//   mathematical values, so the Some-value half is an IN-DOMAIN
//   statement (no i64 overflow, shift count in 0..64). Outside it bash
//   wraps and masks; that generalisation is not proved. The refusal
//   half is not domain-limited: those answers are None regardless.
// - `<< >>`: in-domain only (0 <= b < 64); the source masks the count
//   mod 64 and wraps, which this model does not encode.
// - `& | ^` are opaque bit functions, called identically on both sides
//   (an oracle, like `glob` in case_lowering).
// - `Index`/`Assign`/`Cast` nodes are NOT modelled: the fold only
//   recurses into them and the induction is the same at the child; their
//   denotations are state-dependent and out of this Tier-1 fragment.
// - Fidelity: the source has no unit suite for these tables, so every
//   pin is UNMIRRORED by design (recorded in VERUS.md §(b).4).
//
// ## Mutant (checked)
//
// Replacing the `/` arm with unconditional `Some(tdiv(a, b))` reports
// 19 verified, 4 errors: div_zero_refused, min_div_minus1_refused,
// pin_div_zero_kept and pin_min_div_minus1_kept go red. Restored and
// re-checked at 23 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 optimize_fold.rs
//   -> verification results:: 23 verified, 0 errors
use vstd::prelude::*;
use vstd::arithmetic::power::{pow, lemma_pow_adds, lemma_pow_multiplies, lemma_pow_mod, lemma_pow1};

verus! {

// i64 bounds as mathematical ints (the literal form keeps rustc happy).
pub open spec fn i64_min() -> int { -9223372036854775807 - 1 }
pub open spec fn i64_max() -> int { 9223372036854775807 }

pub open spec fn in_i64(x: int) -> bool {
    i64_min() <= x <= i64_max()
}

// ---- the operator vocabulary (mirrors the match arms) ----
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    And,
    Or,
    Unknown,
}

pub enum UnOp {
    Neg,
    Pos,
    Not,
    BitNot,
    Other,
}

// Opaque bit functions: the model calls them identically on both sides,
// exactly as the renderer does (an oracle, like `glob` in case_lowering).
pub uninterp spec fn band(a: int, b: int) -> int;
pub uninterp spec fn bor(a: int, b: int) -> int;
pub uninterp spec fn bxor(a: int, b: int) -> int;

// ---- the arithmetic model (mathematical ints; no wrapping) ----
// the mathematical power (vstd); pow_wrapping's comment is about this.
pub open spec fn pow_naive(b: int, e: nat) -> int { pow(b, e) }

// fold_bin_op's `**` arm: repeated squaring, exactly the source loop.
pub open spec fn pow_sq(b: int, e: nat) -> int
    decreases e,
{
    if e == 0 {
        1
    } else if e % 2 == 0 {
        pow_sq(b * b, (e / 2) as nat)
    } else {
        b * pow_sq(b * b, (e / 2) as nat)
    }
}

// Rust's `/` and `%` truncate toward zero; Verus `int` division floors,
// so the model defines truncation explicitly (this matters IN domain:
// -7 / 2 is -3 in bash/Rust, -4 with floor division).
pub open spec fn tdiv(a: int, b: int) -> int
    recommends b != 0,
{
    if b > 0 {
        if a >= 0 { a / b } else { -((-a) / b) }
    } else {
        if a >= 0 { -(a / (-b)) } else { (-a) / (-b) }
    }
}

pub open spec fn tmod(a: int, b: int) -> int
    recommends b != 0,
{
    a - b * tdiv(a, b)
}

// `fold_bin_op(op, a, b)`: Some(v) means the source folds the node to v.
pub open spec fn fold_bin(op: Op, a: int, b: int) -> Option<int> {
    match op {
        Op::Add => Some(a + b),
        Op::Sub => Some(a - b),
        Op::Mul => Some(a * b),
        // zero divisor and i64::MIN / -1 are bash runtime errors: refuse.
        Op::Div => if b != 0 && !(a == i64_min() && b == -1) { Some(tdiv(a, b)) } else { Option::None },
        Op::Mod => if b != 0 && !(a == i64_min() && b == -1) { Some(tmod(a, b)) } else { Option::None },
        // `**`: negative exponent is a runtime error; otherwise the
        // squaring loop.
        Op::Pow => if b >= 0 { Some(pow_sq(a, b as nat)) } else { Option::None },
        // bitwise: always exact on int64 (no wrap concerns in-domain).
        Op::BitAnd => Some(band(a, b)),
        Op::BitOr => Some(bor(a, b)),
        Op::BitXor => Some(bxor(a, b)),
        // shifts: in the no-overflow domain the shift is exactly a*2^b
        // for 0 <= b < 64; outside it the source masks, which is the
        // wrap half of the scope note.
        Op::Shl => if 0 <= b < 64 { Some(a * pow2_int(b)) } else { Option::None },
        Op::Shr => if 0 <= b < 64 { Some(a / pow2_int(b)) } else { Option::None },
        // arithmetic `&&`/`||` yield 1/0.
        Op::And => Some(if a != 0 && b != 0 { 1 } else { 0 }),
        Op::Or => Some(if a != 0 || b != 0 { 1 } else { 0 }),
        Op::Unknown => Option::None,
    }
}

pub open spec fn pow2_int(e: int) -> int
    decreases e,
{
    if e <= 0 {
        1
    } else {
        2 * pow2_int(e - 1)
    }
}

pub open spec fn fold_un(op: UnOp, a: int) -> Option<int> {
    match op {
        UnOp::Neg => Some(-a),
        UnOp::Pos => Some(a),
        UnOp::Not => Some(if a == 0 { 1 } else { 0 }),
        UnOp::BitNot => Some(-a - 1), // ~x == -x - 1 on int64
        UnOp::Other => Option::None,
    }
}

// ---- the expression language ----
pub enum A {
    Num(int),
    Var(nat),
    Bin(Op, Box<A>, Box<A>),
    Un(UnOp, Box<A>),
    Cond(Box<A>, Box<A>, Box<A>),
}

pub type Store = Map<nat, int>;

pub open spec fn den(a: A, s: Store) -> int
    decreases a,
{
    match a {
        A::Num(n) => n,
        A::Var(v) => if s.dom().contains(v) { s[v] } else { 0 },
        A::Bin(op, l, r) => {
            let x = den(*l, s);
            let y = den(*r, s);
            match fold_bin(op, x, y) {
                Some(v) => v,
                // a refused op is a bash runtime error; the value the
                // runtime leaves is outside this model, so it stays 0
                // (the same convention as `lookup` in the sibling models).
                Option::None => 0,
            }
        }
        A::Un(op, x) => {
            match fold_un(op, den(*x, s)) {
                Some(v) => v,
                Option::None => 0,
            }
        }
        A::Cond(c, t, e) => if den(*c, s) != 0 { den(*t, s) } else { den(*e, s) },
    }
}

// `fold_arith_ast`, transcribed: recurse everywhere; a fully literal
// binary/unary node becomes Num when the table answers Some.
pub open spec fn fold(a: A) -> A
    decreases a,
{
    match a {
        A::Num(n) => A::Num(n),
        A::Var(v) => A::Var(v),
        A::Bin(op, l, r) => {
            let fl = fold(*l);
            let fr = fold(*r);
            match (fl, fr) {
                (A::Num(x), A::Num(y)) => match fold_bin(op, x, y) {
                    Some(v) => A::Num(v),
                    Option::None => A::Bin(op, Box::new(fl), Box::new(fr)),
                },
                _ => A::Bin(op, Box::new(fl), Box::new(fr)),
            }
        }
        A::Un(op, x) => {
            let fx = fold(*x);
            match fx {
                A::Num(n) => match fold_un(op, n) {
                    Some(v) => A::Num(v),
                    Option::None => A::Un(op, Box::new(fx)),
                },
                _ => A::Un(op, Box::new(fx)),
            }
        }
        A::Cond(c, t, e) => A::Cond(Box::new(fold(*c)), Box::new(fold(*t)), Box::new(fold(*e))),
    }
}

// ---- the refusal table ----
proof fn div_zero_refused(a: int)
    ensures fold_bin(Op::Div, a, 0) == Option::None,
{
}

proof fn mod_zero_refused(a: int)
    ensures fold_bin(Op::Mod, a, 0) == Option::None,
{
}

proof fn min_div_minus1_refused()
    ensures fold_bin(Op::Div, i64_min(), -1) == Option::None,
{
}

proof fn pow_negative_refused(a: int, b: int)
    requires b < 0,
    ensures fold_bin(Op::Pow, a, b) == Option::None,
{
}

proof fn unknown_op_refused(a: int, b: int)
    ensures fold_bin(Op::Unknown, a, b) == Option::None,
{
}

// A refused fold keeps the node's shape, so nothing is guessed.
proof fn fold_keeps_shape(op: Op, l: A, r: A)
    requires
        forall|x: int, y: int| #[trigger] fold_bin(op, x, y) == Option::None,
    ensures
        fold(A::Bin(op, Box::new(l), Box::new(r)))
            == A::Bin(op, Box::new(fold(l)), Box::new(fold(r))),
{
}

// ---- the pow algorithm ----
// repeated squaring == repeated multiplication.
proof fn pow_sq_eq_naive(b: int, e: nat)
    ensures pow_sq(b, e) == pow(b, e),
    decreases e,
{
    reveal_with_fuel(pow_sq, 2);
    reveal_with_fuel(pow, 1);
    if e == 0 {
        assert(pow_sq(b, 0) == 1);
        assert(pow(b, 0) == 1);
    } else if e % 2 == 0 {
        assert((2 * ((e / 2) as nat)) as nat == e);
        pow_sq_eq_naive(b * b, (e / 2) as nat);
        assert(pow_sq(b, e) == pow_sq(b * b, (e / 2) as nat));
        lemma_pow1(b);
        lemma_pow_adds(b, 1, 1);
        assert(pow(b, 2) == b * b);
        lemma_pow_multiplies(b, 2, (e / 2) as nat);
        assert(pow(pow(b, 2), (e / 2) as nat) == pow(b * b, (e / 2) as nat));
        assert(pow(b, e) == pow(b, (2 * ((e / 2) as nat)) as nat));
        assert(pow(b, e) == pow(pow(b, 2), (e / 2) as nat));
        assert(pow_sq(b, e) == pow(b, e));
    } else {
        assert((1 + 2 * ((e / 2) as nat)) as nat == e);
        pow_sq_eq_naive(b * b, (e / 2) as nat);
        assert(pow_sq(b, e) == b * pow_sq(b * b, (e / 2) as nat));
        lemma_pow1(b);
        lemma_pow_adds(b, 1, 1);
        assert(pow(b, 2) == b * b);
        lemma_pow_multiplies(b, 2, (e / 2) as nat);
        assert(pow(pow(b, 2), (e / 2) as nat) == pow(b * b, (e / 2) as nat));
        lemma_pow_adds(b, 1, (2 * ((e / 2) as nat)) as nat);
        assert(pow(b, e) == pow(b, 1) * pow(b, (2 * ((e / 2) as nat)) as nat));
        assert(pow(b, (2 * ((e / 2) as nat)) as nat) == pow(pow(b, 2), (e / 2) as nat));
        assert(pow(b, e) == pow(b, (1 + 2 * ((e / 2) as nat)) as nat));
        assert(pow_sq(b, e) == pow(b, e));
    }
}

// The claim in pow_wrapping's comment, in residue form: the squared
// product and the plain product agree modulo 2^64.
proof fn pow_wrapping_residue(b: int, e: nat)
    ensures pow_sq(b, e) % (pow(2, 64)) == pow(b, e) % (pow(2, 64)),
{
    pow_sq_eq_naive(b, e);
}

// ---- THE OBLIGATION ----
proof fn fold_sound(a: A, s: Store)
    ensures den(fold(a), s) == den(a, s),
    decreases a,
{
    reveal_with_fuel(fold, 2);
    reveal_with_fuel(den, 2);
    match a {
        A::Num(_) => {}
        A::Var(_) => {}
        A::Bin(op, l, r) => {
            fold_sound(*l, s);
            fold_sound(*r, s);
        }
        A::Un(op, x) => {
            fold_sound(*x, s);
        }
        A::Cond(c, t, e) => {
            fold_sound(*c, s);
            fold_sound(*t, s);
            fold_sound(*e, s);
        }
    }
}

// ---- fidelity pins (UNMIRRORED: the source tables have no unit suite) ----
proof fn pin_literal_add_folds()
    ensures fold(A::Bin(Op::Add, Box::new(A::Num(2)), Box::new(A::Num(3)))) == A::Num(5),
{
    reveal_with_fuel(fold, 4);
}
proof fn pin_pow_folds()
    ensures fold(A::Bin(Op::Pow, Box::new(A::Num(3)), Box::new(A::Num(4)))) == A::Num(81),
{
    reveal_with_fuel(fold, 4);
    reveal_with_fuel(pow_sq, 8);
}
proof fn pin_div_zero_kept()
    ensures fold(A::Bin(Op::Div, Box::new(A::Num(7)), Box::new(A::Num(0))))
        == A::Bin(Op::Div, Box::new(A::Num(7)), Box::new(A::Num(0))),
{
    reveal_with_fuel(fold, 4);
}
proof fn pin_negative_pow_kept()
    ensures fold(A::Bin(Op::Pow, Box::new(A::Num(3)), Box::new(A::Num(-1))))
        == A::Bin(Op::Pow, Box::new(A::Num(3)), Box::new(A::Num(-1))),
{
    reveal_with_fuel(fold, 4);
}
proof fn pin_variable_operand_kept()
    ensures fold(A::Bin(Op::Add, Box::new(A::Var(0)), Box::new(A::Num(3))))
        == A::Bin(Op::Add, Box::new(A::Var(0)), Box::new(A::Num(3))),
{
    reveal_with_fuel(fold, 4);
}
proof fn pin_unary_not_folds()
    ensures fold(A::Un(UnOp::Not, Box::new(A::Num(0)))) == A::Num(1),
{
    reveal_with_fuel(fold, 4);
}
proof fn pin_truncating_div_folds()
    ensures fold(A::Bin(Op::Div, Box::new(A::Num(-7)), Box::new(A::Num(2)))) == A::Num(-3),
{
    reveal_with_fuel(fold, 4);
}

proof fn pin_truncating_mod_folds()
    ensures fold(A::Bin(Op::Mod, Box::new(A::Num(-7)), Box::new(A::Num(2)))) == A::Num(-1),
{
    reveal_with_fuel(fold, 4);
}

proof fn pin_min_div_minus1_kept()
    ensures fold(A::Bin(Op::Div, Box::new(A::Num(i64_min())), Box::new(A::Num(-1))))
        == A::Bin(Op::Div, Box::new(A::Num(i64_min())), Box::new(A::Num(-1))),
{
    reveal_with_fuel(fold, 4);
}

fn main() {}
}
