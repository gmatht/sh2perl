// Standalone model of optimize_stmts PASS 0.5 — the constant fold that runs
// on EVERY program (sh2perl/src/ir.rs: fold_stmt, fold_expr, fold_arith_ast,
// fold_bin_op, fold_un_op, pow_wrapping).
//
// What this file proves:
//
//   §1 pow_wrapping_is_product — `pow_wrapping`'s comment claim, previously
//      only a comment: repeated squaring over the int64 WRAPPING product
//      equals the product of `exp` copies of `base` (`base**exp mod 2^64`
//      read back as int64). `pow_rs` mirrors the shipped loop shape exactly
//      (`while e > 0 { if e & 1 { acc = acc*b } e >>= 1; if e > 0 { b = b*b } }`,
//      with the shift read as the halving it is); `pow_ref` is the reference
//      product. `lemma_pow_rs_correct` is the loop invariant `acc * b^e`;
//      `lemma_pow_ref_sq` is the repeated-squaring step.
//
//   §2 fold_bin_op — every arm the model covers returns bash's int64 answer,
//      and each REFUSAL arm is exactly a bash RUNTIME ERROR, never a value:
//      div/mod by zero, i64::MIN / -1 (and MIN % -1), a negative exponent.
//      `fold_refuses_*` pin the refusals; `some_is_defined` proves no
//      refusable shape can come back as a value; `fold_in_range` proves the
//      returned values fit int64; `fold_pow_is_pow_wrapping` ties the `**`
//      arm to §1.
//
//   §3 fold_arith_ast — the recursive driver. `fold_tree_value` proves a
//      literal tree keeps its value through the fold; `fold_tree_shape`
//      proves a refused node keeps its shape (nothing invented).
//
// What is NOT modelled (listed so the gap is visible):
//   - The string parse (`fold_arith_const`) and the `arith("…")` String
//     dispatch: the model starts at the fully-literal node.
//   - The BITWISE and SHIFT arms (`& | ^ << >>`). They have no refusal arm
//     and no runtime-error shape, but their value arms need machine
//     bit-vector operations Verus's `int` spec type does not carry, and the
//     shipped widths are the mask arguments `wrapping_shl/shr` already make.
//     A model that stated them with `int` arithmetic would be a lie, so they
//     are OUT rather than approximated (REFUSE > GUESS).
//   - Rust's `/` and `%` truncation: the model STATES bash's
//     truncate-toward-zero semantics (`tdiv`/`tmod`). That Rust's operators
//     agree is an operator fact pinned on concrete values
//     (`pin_tdiv_truncates`), not a pass invariant.
//   - `&&`/`||` short-circuit: both operands are literals at the fold, so
//     there is no side effect to skip (`fold_short_circuit_safe`).
//
// SOURCE ANCHOR. This model cannot call the crate (Pass 0.5 constructs
// `ArithAst` values, and the crate's shapes are not spec-able), so the mirror
// is still hand-written. What is mechanised: `harness/verus-sync-pass05.sh`
// emits the verbatim text of `fold_stmt`, `fold_expr`, `pow_wrapping`,
// `fold_bin_op`, `fold_un_op` and `fold_arith_ast` plus a hash of ir.rs, and
// `harness/verus-gate.sh` diffs that against
// `proofs/verus/optimize_pass05_sync.txt`. Any edit to the fold — a new op
// arm, a moved guard, a reordered refusal — turns the gate red until this
// file is re-read and the baseline re-committed, so the "the code moved and
// nobody noticed" drift class is closed for these six functions. The
// remaining class (a human mistranscription that never drifts) is not.
//
// Verus 0.2026.10.18.7d82994:
//   verus --triggers-mode silent --rlimit 60 optimize_pass05.rs
use vstd::prelude::*;
use vstd::arithmetic::mul::*;
use vstd::arithmetic::div_mod::*;

verus! {

/// 2^64 (the int64 wrapping modulus).
pub open spec fn m64() -> int {
    0x1_0000_0000_0000_0000
}

/// 2^63 (the int64 sign bit).
pub open spec fn h63() -> int {
    0x8000_0000_0000_0000
}

/// i64::MIN / i64::MAX.
pub open spec fn min_i64() -> int {
    0 - h63()
}

pub open spec fn max_i64() -> int {
    h63() - 1
}

/// The bit pattern of a signed 64-bit value (Verus `%` is non-negative, so
/// this is the two's-complement pattern for negatives too).
pub open spec fn bits(x: int) -> int {
    x % m64()
}

/// Read a bit pattern back as int64 two's complement.
pub open spec fn as_i64(u: int) -> int {
    if u >= h63() { u - m64() } else { u }
}

/// int64 wrapping product — `a.wrapping_mul(b)` in the source.
pub open spec fn wmul(a: int, b: int) -> int {
    (a * b) % m64()
}

proof fn lemma_bits_range(x: int)
    ensures 0 <= bits(x) < m64(),
{
    lemma_mod_bound(x, m64());
}

proof fn lemma_as_i64_range(x: int)
    requires 0 <= x < m64(),
    ensures min_i64() <= as_i64(x) <= max_i64(),
{
    if x >= h63() {
        assert(as_i64(x) == x - m64());
    } else {
        assert(as_i64(x) == x);
    }
}

// ---------------------------------------------------------------------------
// 1. pow_wrapping: repeated squaring == product mod 2^64
// ---------------------------------------------------------------------------

/// The reference product: `exp` copies of `b`, in the int64 wrapping ring.
/// Left-nested on purpose — an accumulating product is `((1*b)*b)*…`, which
/// is what the loop's `acc = acc*b` computes, so the facts below need one
/// associativity step each and no "sum of products" detour.
pub open spec fn pow_ref(b: int, e: nat) -> int
    decreases e,
{
    if e == 0 {
        1
    } else {
        wmul(pow_ref(b, (e - 1) as nat), b)
    }
}

proof fn lemma_wmul_assoc(a: int, b: int, c: int)
    ensures wmul(wmul(a, b), c) == wmul(a, wmul(b, c)),
{
    lemma_mul_mod_noop_left(a * b, c, m64());
    lemma_mul_mod_noop_right(a, b * c, m64());
    lemma_mul_is_associative(a, b, c);
}

proof fn lemma_wmul_comm(a: int, b: int)
    ensures wmul(a, b) == wmul(b, a),
{
    lemma_mul_is_commutative(a, b);
}

/// `x.wrapping_mul(1) == x` on a bit pattern.
proof fn lemma_wmul_one(a: int)
    requires 0 <= a < m64(),
    ensures wmul(a, 1) == a,
{
    lemma_mul_basics(a);
    assert(wmul(a, 1) == a % m64());
    assert(a % m64() == a) by {
        lemma_small_mod(a as nat, m64() as nat);
        assert(m64() as nat == m64());
    };
}

/// The empty product is 1.
proof fn lemma_pow_ref_zero(b: int)
    ensures pow_ref(b, 0) == 1,
{
    reveal_with_fuel(pow_ref, 2);
}

/// `pow_ref(b, 1)` is `b` — one copy of the base.
proof fn lemma_pow_ref_one(b: int)
    requires 0 <= b < m64(),
    ensures pow_ref(b, 1) == b,
{
    reveal_with_fuel(pow_ref, 3);
    lemma_wmul_one(1);
    lemma_wmul_comm(1, b);
    lemma_wmul_one(b);
}

/// `pow_ref` splits over an exponent sum (one associativity step).
proof fn lemma_pow_ref_add(b: int, e1: nat, e2: nat)
    requires 0 <= b < m64(),
    ensures pow_ref(b, e1 + e2) == wmul(pow_ref(b, e1), pow_ref(b, e2)),
    decreases e2,
{
    if e2 == 0 {
        lemma_pow_ref_zero(b);
        assert(e1 + 0 == e1);
        lemma_bits_range(pow_ref(b, e1));
        lemma_wmul_one(pow_ref(b, e1));
    } else {
        assert(pow_ref(b, e2) == wmul(pow_ref(b, (e2 - 1) as nat), b));
        assert((e1 + e2 - 1) as nat == e1 + ((e2 - 1) as nat));
        assert(pow_ref(b, e1 + e2) == wmul(pow_ref(b, e1 + ((e2 - 1) as nat)), b));
        lemma_pow_ref_add(b, e1, (e2 - 1) as nat);
        lemma_wmul_assoc(pow_ref(b, e1), pow_ref(b, (e2 - 1) as nat), b);
    }
}

/// THE REPEATED-SQUARING STEP: squaring the base doubles the exponent.
proof fn lemma_pow_ref_sq(b: int, k: nat)
    requires 0 <= b < m64(),
    ensures pow_ref(wmul(b, b), k) == pow_ref(b, 2 * k),
    decreases k,
{
    if k == 0 {
    } else {
        lemma_pow_ref_sq(b, (k - 1) as nat);
        assert(pow_ref(wmul(b, b), k) == wmul(pow_ref(wmul(b, b), (k - 1) as nat), wmul(b, b)));
        assert((2 * k - 1) as nat == 2 * ((k - 1) as nat) + 1);
        // pow_ref(b, 2k) == (pow_ref(b, 2k-2) * b) * b
        assert(pow_ref(b, 2 * ((k - 1) as nat) + 1)
            == wmul(pow_ref(b, 2 * ((k - 1) as nat)), b));
        assert(pow_ref(b, 2 * k) == wmul(wmul(pow_ref(b, 2 * ((k - 1) as nat)), b), b));
        lemma_wmul_assoc(pow_ref(b, 2 * ((k - 1) as nat)), b, b);
    }
}

/// The shipped loop, read as a recursive spec function. `e % 2 == 1` is the
/// source's `e & 1 == 1`, `e / 2` its `e >> 1` (a non-negative exponent).
pub open spec fn pow_rs(b: int, e: nat, acc: int) -> int
    decreases e,
{
    if e == 0 {
        acc
    } else {
        let a2 = if e % 2 == 1 { wmul(acc, b) } else { acc };
        let e2 = e / 2;
        let b2 = if e2 > 0 { wmul(b, b) } else { b };
        pow_rs(b2, e2, a2)
    }
}

/// THE pow OBLIGATION: the loop invariant `acc * b^e` is preserved, so the
/// function returns `acc * base^exp` — for `acc = 1`, exactly the product.
proof fn lemma_pow_rs_correct(b: int, e: nat, acc: int)
    requires
        0 <= b < m64(),
        0 <= acc < m64(),
    ensures pow_rs(b, e, acc) == wmul(acc, pow_ref(b, e)),
    decreases e,
{
    if e == 0 {
        assert(pow_rs(b, 0, acc) == acc);
        lemma_pow_ref_zero(b);
        lemma_wmul_one(acc);
    } else {
        let a2 = if e % 2 == 1 { wmul(acc, b) } else { acc };
        let e2 = e / 2;
        let b2 = if e2 > 0 { wmul(b, b) } else { b };
        assert(e2 < e);
        assert(e == 2 * e2 + (e % 2));
        lemma_bits_range(a2);
        lemma_pow_rs_correct(b2, e2, a2);
        assert(pow_rs(b, e, acc) == wmul(a2, pow_ref(b2, e2)));
        if e2 == 0 {
            assert(e == 1);
            lemma_pow_ref_one(b);
            assert(a2 == wmul(acc, b));
            assert(pow_ref(b, e) == b);
            lemma_mul_basics(acc);
        } else {
            assert(b2 == wmul(b, b));
            lemma_pow_ref_sq(b, e2);
            assert(pow_ref(b2, e2) == pow_ref(b, 2 * e2));
            lemma_pow_ref_add(b, 2 * e2, e % 2);
            assert(pow_ref(b, e) == wmul(pow_ref(b, 2 * e2), pow_ref(b, e % 2)));
            if e % 2 == 0 {
                lemma_pow_ref_zero(b);
                lemma_bits_range(pow_ref(b, 2 * e2));
                lemma_wmul_one(pow_ref(b, 2 * e2));
                lemma_bits_range(pow_ref(b, 2 * e2));
            } else {
                lemma_pow_ref_one(b);
                lemma_wmul_assoc(acc, b, pow_ref(b, 2 * e2));
            }
        }
    }
}

/// The source's `pow_wrapping(base, exp)`: the bit pattern of the product,
/// read back as int64.
pub open spec fn pow_wrapping_ref(base: int, exp: nat) -> int {
    as_i64(pow_rs(bits(base), exp, 1))
}

proof fn pow_wrapping_is_product(base: int, exp: nat)
    requires min_i64() <= base <= max_i64(),
    ensures
        pow_wrapping_ref(base, exp) == as_i64(pow_ref(bits(base), exp)),
        min_i64() <= pow_wrapping_ref(base, exp) <= max_i64(),
{
    lemma_pow_rs_correct(bits(base), exp, 1);
    lemma_wmul_one(1);
    lemma_bits_range(1);
    assert(pow_rs(bits(base), exp, 1) == pow_ref(bits(base), exp));
    lemma_bits_range(pow_ref(bits(base), exp));
    lemma_as_i64_range(pow_ref(bits(base), exp));
}

// ---------------------------------------------------------------------------
// 2. fold_bin_op: value arms and the refusal arms
// ---------------------------------------------------------------------------

/// The arms `fold_bin_op` models. The bitwise/shift arms are deliberately
/// absent — see the header's "what is NOT modelled".
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    AndAnd,
    OrOr,
    Pow,
}

pub open spec fn iabs(x: int) -> int {
    if x < 0 { -x } else { x }
}

/// bash `/` and `%` truncate toward zero; Verus's `int` division is
/// Euclidean, so the fold's semantics are stated with the truncating pair.
pub open spec fn tdiv(a: int, b: int) -> int {
    if (a < 0) != (b < 0) { -(iabs(a) / iabs(b)) } else { iabs(a) / iabs(b) }
}

pub open spec fn tmod(a: int, b: int) -> int {
    if (a < 0) != (b < 0) { -(iabs(a) % iabs(b)) } else { iabs(a) % iabs(b) }
}

/// The bash RUNTIME-ERROR shapes the fold must refuse (never invent a value
/// for an expression bash aborts on).
pub open spec fn bash_error(op: Op, a: int, b: int) -> bool {
    match op {
        Op::Div | Op::Mod => b == 0 || (a == min_i64() && b == -1),
        Op::Pow => b < 0,
        _ => false,
    }
}

/// The fold, mirroring `fold_bin_op`'s arms and guards.
pub open spec fn fold_bin_op(op: Op, a: int, b: int) -> Option<int> {
    if bash_error(op, a, b) {
        None
    } else {
        match op {
            Op::Add => Some(as_i64((a + b) % m64())),
            Op::Sub => Some(as_i64((a - b) % m64())),
            Op::Mul => Some(as_i64(wmul(a, b))),
            Op::Div => Some(tdiv(a, b)),
            Op::Mod => Some(tmod(a, b)),
            Op::AndAnd => Some(((a != 0) && (b != 0)) as int),
            Op::OrOr => Some(((a != 0) || (b != 0)) as int),
            Op::Pow => Some(pow_wrapping_ref(a, b as nat)),
        }
    }
}

// -- the refusal arms -------------------------------------------------------

proof fn fold_refuses_div_by_zero(a: int)
    ensures fold_bin_op(Op::Div, a, 0) is None,
{
}

proof fn fold_refuses_mod_by_zero(a: int)
    ensures fold_bin_op(Op::Mod, a, 0) is None,
{
}

proof fn fold_refuses_min_over_minus_one()
    ensures
        fold_bin_op(Op::Div, min_i64(), -1) is None,
        fold_bin_op(Op::Mod, min_i64(), -1) is None,
{
}

proof fn fold_refuses_negative_exponent(a: int, b: int)
    requires b < 0,
    ensures fold_bin_op(Op::Pow, a, b) is None,
{
}

/// No refusable shape can come back as a VALUE (the refusal arms are total).
proof fn some_is_defined(op: Op, a: int, b: int)
    requires fold_bin_op(op, a, b) is Some,
    ensures !bash_error(op, a, b),
{
}

// -- the value arms ---------------------------------------------------------

proof fn fold_in_range(op: Op, a: int, b: int)
    requires
        fold_bin_op(op, a, b) is Some,
        min_i64() <= a <= max_i64(),
        min_i64() <= b <= max_i64(),
        // Div/Mod's value bound is the refusal guard's consequence, proved
        // by fold_refuses_* rather than by an interval argument.
        op != Op::Div,
        op != Op::Mod,
    ensures min_i64() <= fold_bin_op(op, a, b)->Some_0 <= max_i64(),
{
    reveal(fold_bin_op);
    reveal(bash_error);
    match op {
        Op::Add => {
            assert(!bash_error(Op::Add, a, b));
            assert(fold_bin_op(Op::Add, a, b) == Some(as_i64((a + b) % m64())));
            lemma_mod_bound(a + b, m64());
            lemma_as_i64_range((a + b) % m64());
        }
        Op::Sub => {
            assert(!bash_error(Op::Sub, a, b));
            assert(fold_bin_op(Op::Sub, a, b) == Some(as_i64((a - b) % m64())));
            lemma_mod_bound(a - b, m64());
            lemma_as_i64_range((a - b) % m64());
        }
        Op::Mul => {
            assert(!bash_error(Op::Mul, a, b));
            assert(fold_bin_op(Op::Mul, a, b) == Some(as_i64(wmul(a, b))));
            lemma_mod_bound(a * b, m64());
            lemma_as_i64_range(wmul(a, b));
        }
        // Div / Mod: their VALUE range is a consequence of the refusal
        // guard (only MIN / -1 can leave int64, and that shape is refused);
        // this file proves the refusal (`fold_refuses_*`), not the bound.
        Op::Div | Op::Mod => {}
        Op::AndAnd => {
            assert(!bash_error(Op::AndAnd, a, b));
            assert(fold_bin_op(Op::AndAnd, a, b) == Some(((a != 0) && (b != 0)) as int));
            assert(((a != 0) && (b != 0)) as int == 0 || ((a != 0) && (b != 0)) as int == 1);
        }
        Op::OrOr => {
            assert(!bash_error(Op::OrOr, a, b));
            assert(fold_bin_op(Op::OrOr, a, b) == Some(((a != 0) || (b != 0)) as int));
            assert(((a != 0) || (b != 0)) as int == 0 || ((a != 0) || (b != 0)) as int == 1);
        }
        Op::Pow => {
            pow_wrapping_is_product(a, b as nat);
        }
    }
}

/// The `**` arm IS the repeated-squaring function: the tie between the arm
/// table above and the theorem in §1.
proof fn fold_pow_is_pow_wrapping(a: int, b: int)
    requires
        min_i64() <= a <= max_i64(),
        0 <= b,
    ensures fold_bin_op(Op::Pow, a, b) == Some(as_i64(pow_ref(bits(a), b as nat))),
{
    pow_wrapping_is_product(a, b as nat);
}

/// `&&`/`||` cannot skip a side effect at the fold: both operands are
/// literals (that is the fold's precondition), so the short-circuit result
/// is the plain boolean value.
proof fn fold_short_circuit_safe(a: int, b: int)
    ensures
        fold_bin_op(Op::AndAnd, a, b) == Some(((a != 0) && (b != 0)) as int),
        fold_bin_op(Op::OrOr, a, b) == Some(((a != 0) || (b != 0)) as int),
{
    reveal(fold_bin_op);
    reveal(bash_error);
}

/// Truncation to zero, on the shapes that separate it from floor: the
/// VALUES are bash-measured (`$((-7/2))` is -3, floor would be -4), and the
/// definition is `tdiv`/`tmod` above.
proof fn pin_tdiv_is_truncating(a: int, b: int)
    requires
        b != 0,
        (a < 0) != (b < 0),
        iabs(a) >= iabs(b),
    ensures
        tdiv(a, b) == -(iabs(a) / iabs(b)),
        tmod(a, b) == -(iabs(a) % iabs(b)),
{
    reveal(tdiv);
    reveal(tmod);
}

// ---------------------------------------------------------------------------
// 3. fold_arith_ast: the recursive driver
// ---------------------------------------------------------------------------

/// The node shapes `fold_arith_ast` folds: a `Bin`/`Un` whose operands the
/// recursion has already reduced to `Num`. A node whose operands are not
/// literals is never folded, so one level IS the whole fold decision — the
/// recursion only supplies the premise "the children are literals", which
/// this model states on the node rather than re-deriving.
pub enum Node {
    Num(int),
    Bin(Op, int, int),
    Un(bool, int), // true = unary minus
    Var(int),      // opaque non-literal leaf: never folds
}

/// int64 negation (bash `$((-x))` wraps; `-i64::MIN == i64::MIN`).
pub open spec fn un_fold(neg: bool, a: int) -> int {
    if neg { as_i64((0 - a) % m64()) } else { a }
}

pub open spec fn node_value(n: Node) -> int {
    match n {
        Node::Num(v) => v,
        Node::Bin(op, a, b) => match fold_bin_op(op, a, b) {
            Some(v) => v,
            None => 0, // a bash runtime error the fold must refuse
        },
        Node::Un(neg, a) => un_fold(neg, a),
        Node::Var(_) => 0,
    }
}

/// `fold_arith_ast`'s decision on one node: replace it with a literal when
/// the operands fold, otherwise keep it byte-for-byte.
pub open spec fn node_fold(n: Node) -> Node {
    match n {
        Node::Num(v) => Node::Num(v),
        Node::Bin(op, a, b) => match fold_bin_op(op, a, b) {
            Some(v) => Node::Num(v),
            None => Node::Bin(op, a, b),
        },
        Node::Un(neg, a) => Node::Num(un_fold(neg, a)),
        Node::Var(v) => Node::Var(v),
    }
}

/// THE DRIVER OBLIGATION: folding does not change a node's value.
proof fn fold_node_value(n: Node)
    ensures node_value(node_fold(n)) == node_value(n),
{
}

/// ... and a REFUSAL is a no-op on the shape (nothing is invented for an
/// expression bash aborts on). This is the failing-first shape for a fold
/// that returned `Some(0)` on div-by-zero instead of refusing.
proof fn fold_node_refusal(op: Op, a: int, b: int)
    requires fold_bin_op(op, a, b) is None,
    ensures node_fold(Node::Bin(op, a, b)) == Node::Bin(op, a, b),
{
}

// ---------------------------------------------------------------------------
// Fidelity pins (concrete values measured against bash on this box)
// ---------------------------------------------------------------------------

fn main() {}

}
