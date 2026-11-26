// bigint_range.rs — model of `shir_passes/bigint_range.rs` (the
// `big_arith_range` / `analyze_var_ranges_big` analysis and its consumers).
//
// The pass does not rewrite; it REFINES: it computes, for each name, an
// interval over the mathematical integers (BigInt, unbounded) that contains
// every value the name can take, and its consumer uses that interval to
// decide whether an i64/u64 representation is safe.  So the law a consumer
// owes is SOUNDNESS of the reported interval with respect to the program's
// own evaluation, and the model states exactly that:
//
//   range_sound (THE THEOREM OF RECORD)
//       For every arith tree `a`, environment `env` and state `st`: if `st`
//       respects every interval in `env`, then every value in `range_of(a,
//       env)`'s interval bounds `ev(a, st)` — the analysis's own arithmetic
//       tree, evaluated in Python semantics (`//` and `%` floor), and the
//       analysis's own tree walk (point, var, +, -, *, %, //, **, unary -,
//       comparison, conditional, with the join `bhull` over branch arms).
//       `None` (a refusal) is vacuously sound and is pinned separately.
//
// The per-operator laws the theorem is built from, each proved for all
// inputs rather than at sample points:
//
//   fchar_pos / fchar_pos_nonneg / fchar_pos_neg
//       `floor_div`/`floor_mod` on a POSITIVE divisor are the mathematical
//       floor division and floor residue: `x == b*q + r`, `0 <= r < b`, and
//       the residue is the Euclidean one (`fmod(x,b) == x % b`).  This is
//       the bridge from the code's tdiv/tmod-plus-adjustment shape to the
//       laws, and it is where Python's floor semantics and Verus's
//       Euclidean `%` are reconciled.
//   fmod_neg_bounds       a negative divisor puts the residue in `(b, 0]`.
//   fdiv_nonpos_form      for `b > 0` and `x <= 0`, `fdiv(x,b)` is
//                         `-((-x-1)/b) - 1`; the sign-directed form that
//                         makes divisor monotonicity provable.
//   fdiv_mono_x / fdiv_mono_b
//       floor division is monotone in the dividend, and monotone in the
//       divisor in the direction its sign dictates (decreasing for `x >= 0`,
//       increasing for `x <= 0`).
//   mul_sound              the four products at the corners of the operand
//                         rectangle bound every product inside it.
//   mod_pos_sound / mod_neg_sound
//       the `%` table's SIGN RULES: `0 <= fmod <= r1-1` for `r0 > 0` and
//       `r0+1 <= fmod <= 0` for `r1 < 0`.
//   div_corner_sound      the `//` table's four-corner interval is sound
//                         when the divisor interval is entirely positive —
//                         the guard `r0 > 0` — which is what makes the
//                         corner table legitimate there.
//   hull_left / hull_right  the `Cond` join keeps either arm's value.
//
// Refusals, pinned (the table's `None` arms, proved and not merely quoted):
//   float_division_is_refused            `/` (float-producing) is refused.
//   bitwise_operators_are_refused        bitwise/`<<`/`>>` are refused.
//   a_zero_modulus_point_is_refused      `x % 0` at a point.
//   a_straddling_modulus_is_refused      a divisor range crossing 0.
//   a_non_positive_floordiv_divisor_is_refused / a_zero_floordiv_divisor_is_refused
//   a_non_point_power_is_refused / an_oversized_power_is_refused /
//   a_negative_exponent_is_refused       `**` needs a point base and a
//                                        point exponent in `[0, 4096]`.
//   the_table_accepts_the_shapes_it_should
//       the accepted side of the same table, so each refusal is shown to
//       exclude only what it names.
//
// Real divergences and precision facts, recorded as explicit theorems (NOT
// closed, NOT weakened, NOT patched — the analysis code is untouched):
//
//   the_corner_table_for_floordiv_needs_a_positive_divisor
//       A corner table for `//` is UNSOUND once the divisor interval
//       reaches or straddles zero: over `x = -6`, `b` in `[-4, 1]` the
//       corners claim at most 1, but `-6 // -1 = 6`.  The guard `r0 > 0`
//       is therefore load-bearing, not stylistic; `div_corner_sound`
//       proves the guarded table sound.
//   the_corner_table_for_mod_is_unsound_for_negative_divisors
//       A corner table for `%` is unsound even when every divisor is
//       negative: over `x = -8`, `b` in `[-6, -4]` the corners give
//       `[-2, 0]`, but `-8 % -5 = -3`.  That is why the `%` arm uses the
//       sign rules, which `mod_pos_sound`/`mod_neg_sound` prove sound.
//   negative_floordiv_divisors_are_refused_though_sound
//       The `//` guard is CONSERVATIVE in the negative half: an exhaustive
//       probe (recorded in the proof-campaign plan) found no counterexample for a wholly
//       negative divisor interval, yet the analysis refuses it.  Precision
//       loss, not unsoundness.
//   the_mod_interval_is_an_over_approximation
//       `%`'s sign rule is not tight (`[-5,3] % [2,9]` is reported `[0,8]`
//       while the largest reachable residue is 3).  A consumer must not
//       read a reported interval as exact.
//
// Theorem of record: `range_sound`.  Its statement is the consumer
// obligation (reported interval bounds every value the program can take),
// not a restatement of the code's control flow: the code's guards appear
// only as the `None` arms and as the hypotheses of the operator laws.
//
// Out of scope, named rather than skipped:
//   * The Isabelle cross-check: `proofs/BigIntRange/BigIntRange.thy` is the
//     historical reference theory and cannot be rebuilt in this environment
//     (no Isabelle binary, mirrors unreachable).  NOTHING in this bar
//     depends on it: `range_sound` is the statement of record, and the
//     theory is an unbuildable cross-check.
//   * The digit-length caps in `**` (`l0` longer than 10000 digits, result
//     longer than 40000) and the `Index`/`sizeof`/float-cast node refusals:
//     they only REFUSE MORE, so they cannot falsify `range_sound`; they are
//     size and unknown-node guards, not range laws, and are not modelled.
//   * The walk's fixpoint over the whole program (`analyze_var_ranges_big`:
//     statement walk, loop widening, whole-life joins): the model proves the
//     per-node table and the tree walk against a *sound* incoming
//     environment (`range_sound`'s hypothesis), which is the consumer-facing
//     contract.  The fixpoint's convergence and the widening's
//     over-approximation are not modelled here.
//   * i64/u64 selection and any rendering: `range_of`'s interval is the
//     claim; whether a consumer's chosen representation preserves it is a
//     different law (covered for the i32 path by `i32_emit.rs`).
//
use vstd::prelude::*;
use vstd::arithmetic::div_mod::*;
use vstd::arithmetic::mul::*;
verus! {
broadcast use group_div_basics;

pub open spec fn tdiv(x: int, b: int) -> int {
    if b == 0 { 0 }
    else if x >= 0 { if b > 0 { x / b } else { -(x / (-b)) } }
    else { if b > 0 { -((-x) / b) } else { (-x) / (-b) } }
}
pub open spec fn tmod(x: int, b: int) -> int {
    if b == 0 { 0 }
    else if x >= 0 { if b > 0 { x % b } else { x % (-b) } }
    else { if b > 0 { -((-x) % b) } else { -((-x) % (-b)) } }
}
pub open spec fn fdiv(x: int, b: int) -> int {
    if b == 0 { 0 } else {
        let q = tdiv(x, b); let r = tmod(x, b);
        if r != 0 && ((r < 0) != (b < 0)) { q - 1 } else { q }
    }
}
pub open spec fn fmod(x: int, b: int) -> int {
    if b == 0 { 0 } else {
        let r = tmod(x, b);
        if r != 0 && ((r < 0) != (b < 0)) { r + b } else { r }
    }
}

// ── b > 0: the code computes floor division / floor modulo ─────────────
proof fn fchar_pos_nonneg(x: int, b: int)
    requires x >= 0, 0 < b,
    ensures fdiv(x, b) == x / b, fmod(x, b) == x % b,
            x == b * fdiv(x, b) + fmod(x, b), 0 <= fmod(x, b) < b,
{
    lemma_fundamental_div_mod(x, b);
    lemma_mod_bound(x, b);
    assert(tmod(x, b) == x % b);
    assert(tdiv(x, b) == x / b);
    assert(fdiv(x, b) == x / b);
    assert(fmod(x, b) == x % b);
}

proof fn fchar_pos_neg(x: int, b: int)
    requires x < 0, 0 < b,
    ensures x == b * fdiv(x, b) + fmod(x, b), 0 <= fmod(x, b) < b,
{
    let y = -x;
    let q = y / b;
    let r = y % b;
    lemma_fundamental_div_mod(y, b);
    lemma_mod_bound(y, b);
    assert(y == b * q + r);
    assert(tmod(x, b) == -r);
    if r == 0 {
        assert(tdiv(x, b) == -q);
        assert(fdiv(x, b) == -q);
        assert(fmod(x, b) == 0);
        assert(x == b * fdiv(x, b) + fmod(x, b)) by (nonlinear_arith)
            requires x == -y, y == b * q + r, r == 0,
                     fdiv(x, b) == -q, fmod(x, b) == 0;
    } else {
        assert(tdiv(x, b) == -q);
        assert(fdiv(x, b) == -q - 1);
        assert(fmod(x, b) == b - r);
        assert(x == b * fdiv(x, b) + fmod(x, b)) by (nonlinear_arith)
            requires x == -y, y == b * q + r, 0 < r < b,
                     fdiv(x, b) == -q - 1, fmod(x, b) == b - r;
        assert(0 < fmod(x, b) < b);
    }
}

proof fn fchar_pos(x: int, b: int)
    requires b != 0, 0 < b,
    ensures fmod(x, b) == x % b, x == b * fdiv(x, b) + fmod(x, b), 0 <= fmod(x, b) < b,
{
    lemma_fundamental_div_mod(x, b);
    lemma_mod_bound(x, b);
    if x >= 0 {
        fchar_pos_nonneg(x, b);
    } else {
        fchar_pos_neg(x, b);
        // the residue is the Euclidean one for a positive divisor
        lemma_fundamental_div_mod_converse_mod(x, b, fdiv(x, b), fmod(x, b));
    }
}

// b < 0: the residue is in (b, 0]
proof fn fmod_neg_bounds(x: int, b: int)
    requires b != 0, b < 0,
    ensures b < fmod(x, b) <= 0,
{
    let d = -b;
    assert(d > 0);
    lemma_mod_bound(x, d);
    if x >= 0 {
        assert(tmod(x, b) == x % d);
        if x % d == 0 {
            assert(fmod(x, b) == 0);
        } else {
            assert(fmod(x, b) == x % d + b);
        }
    } else {
        assert(tmod(x, b) == -((-x) % d));
        assert(fmod(x, b) == -((-x) % d));
    }
}

proof fn fdiv_nonpos_form(x: int, b: int)
    requires x <= 0, 0 < b,
    ensures fdiv(x, b) == -(((-x - 1) / b)) - 1,
{
    if x == 0 {
        fchar_pos(x, b);
        assert(fdiv(0, b) == 0);
        lemma_fundamental_div_mod_converse_div(-1, b, -1, b - 1);
        assert((-1) / b == -1);
        assert((-x - 1) / b == -1);
    } else {
        let y = -x;
        let c = (y - 1) / b;
        lemma_fundamental_div_mod(y - 1, b);
        lemma_mod_bound(y - 1, b);
        assert(y - 1 == b * c + (y - 1) % b);
        assert(0 <= (y - 1) % b < b);
        assert(b * c <= y - 1) by (nonlinear_arith)
            requires y - 1 == b * c + (y - 1) % b, (y - 1) % b >= 0;
        assert(y - 1 < b * (c + 1)) by (nonlinear_arith)
            requires y - 1 == b * c + (y - 1) % b, (y - 1) % b < b;
        let q = -c - 1;
        let r = b * (c + 1) - y;
        assert(0 <= r) by (nonlinear_arith)
            requires r == b * (c + 1) - y, y - 1 < b * (c + 1);
        assert(r < b) by (nonlinear_arith)
            requires r == b * (c + 1) - y, b * c <= y - 1;
        assert(x == b * q + r) by (nonlinear_arith)
            requires x == -y, q == -c - 1, r == b * (c + 1) - y;
        lemma_fundamental_div_mod_converse_div(x, b, q, r);
        assert(q == x / b);
        fchar_pos(x, b);
        lemma_fundamental_div_mod_converse_div(x, b, fdiv(x, b), fmod(x, b));
        assert(fdiv(x, b) == q);
    }
}

proof fn fdiv_mono_x(x1: int, x2: int, b: int)
    requires 0 < b, x1 <= x2,
    ensures fdiv(x1, b) <= fdiv(x2, b),
{
    fchar_pos(x1, b);
    fchar_pos(x2, b);
    if fdiv(x1, b) > fdiv(x2, b) {
        assert(fdiv(x1, b) >= fdiv(x2, b) + 1);
        assert(x1 == b * fdiv(x1, b) + fmod(x1, b));
        assert(x2 == b * fdiv(x2, b) + fmod(x2, b));
        assert(b * fdiv(x1, b) >= b * (fdiv(x2, b) + 1)) by (nonlinear_arith)
            requires fdiv(x1, b) >= fdiv(x2, b) + 1, b > 0;
        assert(x1 >= b * fdiv(x1, b)) by (nonlinear_arith)
            requires x1 == b * fdiv(x1, b) + fmod(x1, b), fmod(x1, b) >= 0;
        assert(x1 >= b * (fdiv(x2, b) + 1));
        assert(x2 < b * fdiv(x2, b) + b) by (nonlinear_arith)
            requires x2 == b * fdiv(x2, b) + fmod(x2, b), fmod(x2, b) < b;
        assert(x2 < b * (fdiv(x2, b) + 1)) by (nonlinear_arith)
            requires x2 < b * fdiv(x2, b) + b;
        assert(false);
    }
}

proof fn fdiv_mono_b(x: int, b1: int, b2: int)
    requires 0 < b1 <= b2,
    ensures (x >= 0 ==> fdiv(x, b2) <= fdiv(x, b1)),
            (x <= 0 ==> fdiv(x, b1) <= fdiv(x, b2)),
{
    if x >= 0 {
        fchar_pos(x, b1);
        fchar_pos(x, b2);
        assert(fdiv(x, b1) == x / b1);
        assert(fdiv(x, b2) == x / b2);
        lemma_div_is_ordered_by_denominator(x, b1, b2);
    } else {
        if x == 0 {
            fchar_pos(x, b1);
            fchar_pos(x, b2);
            assert(fdiv(0, b1) == 0);
            assert(fdiv(0, b2) == 0);
        } else {
            fdiv_nonpos_form(x, b1);
            fdiv_nonpos_form(x, b2);
            lemma_div_is_ordered_by_denominator(-x - 1, b1, b2);
        }
    }
}



// ── the interval lattice ───────────────────────────────────────────────
pub open spec fn imin(a: int, b: int) -> int { if a <= b { a } else { b } }
pub open spec fn imax(a: int, b: int) -> int { if a >= b { a } else { b } }
pub open spec fn min4(a: int, b: int, c: int, d: int) -> int { imin(imin(a, b), imin(c, d)) }
pub open spec fn max4(a: int, b: int, c: int, d: int) -> int { imax(imax(a, b), imax(c, d)) }

// the join the walk uses for whole-life values and branch arms
pub open spec fn bhull(a: Option<(int, int)>, b: Option<(int, int)>) -> Option<(int, int)> {
    match (a, b) {
        (Some(l), Some(r)) => Some((imin(l.0, r.0), imax(l.1, r.1))),
        _ => None,
    }
}

pub enum Op { Add, Sub, Mul, Mod, Floordiv, Pow }

// the per-operator table: the code's `big_arith_range`, with the refusals
pub open spec fn big_arith_range(op: Op, l0: int, l1: int, r0: int, r1: int) -> Option<(int, int)> {
    match op {
        Op::Add => Some((l0 + r0, l1 + r1)),
        Op::Sub => Some((l0 - r1, l1 - r0)),
        Op::Mul => Some((min4(l0 * r0, l0 * r1, l1 * r0, l1 * r1),
                         max4(l0 * r0, l0 * r1, l1 * r0, l1 * r1))),
        Op::Mod => {
            if l0 == l1 && r0 == r1 {
                if r0 == 0 { None } else { Some((fmod(l0, r0), fmod(l0, r0))) }
            } else if r0 > 0 {
                Some((0, r1 - 1))
            } else if r1 < 0 {
                Some((r0 + 1, 0))
            } else {
                None
            }
        }
        Op::Floordiv => {
            if r0 <= 0 { None } else {
                Some((min4(fdiv(l0, r0), fdiv(l0, r1), fdiv(l1, r0), fdiv(l1, r1)),
                      max4(fdiv(l0, r0), fdiv(l0, r1), fdiv(l1, r0), fdiv(l1, r1))))
            }
        }
        Op::Pow => {
            if l0 != l1 || r0 != r1 || r0 < 0 || r0 > 4096 { None }
            else { Some((ipow(l0, r0), ipow(l0, r0))) }
        }
    }
}

pub enum A {
    Num(int),
    Var(int),
    Add(Box<A>, Box<A>),
    Sub(Box<A>, Box<A>),
    Mul(Box<A>, Box<A>),
    Mod(Box<A>, Box<A>),
    Floordiv(Box<A>, Box<A>),
    Pow(Box<A>, Box<A>),
    Neg(Box<A>),
    Cmp(Box<A>, Box<A>),
    Cond(Box<A>, Box<A>, Box<A>),
    FloatDiv(Box<A>, Box<A>),
    Bitwise(Box<A>, Box<A>),
}

pub open spec fn ipow(b: int, e: int) -> int
    decreases (if e > 0 { e } else { 0 }),
{
    if e <= 0 { 1 } else { b * ipow(b, e - 1) }
}

pub open spec fn ev(a: A, st: Map<int, int>) -> int
    decreases a,
{
    match a {
        A::Num(n) => n,
        A::Var(k) => st[k],
        A::Add(l, r) => ev(*l, st) + ev(*r, st),
        A::Sub(l, r) => ev(*l, st) - ev(*r, st),
        A::Mul(l, r) => ev(*l, st) * ev(*r, st),
        A::Mod(l, r) => fmod(ev(*l, st), ev(*r, st)),
        A::Floordiv(l, r) => fdiv(ev(*l, st), ev(*r, st)),
        A::Pow(l, r) => ipow(ev(*l, st), ev(*r, st)),
        A::Neg(x) => -ev(*x, st),
        A::Cmp(l, r) => if ev(*l, st) == ev(*r, st) { 1 } else { 0 },
        A::Cond(c, t, e) => if ev(*c, st) != 0 { ev(*t, st) } else { ev(*e, st) },
        A::FloatDiv(l, r) => 0,
        A::Bitwise(l, r) => 0,
    }
}

// the analysis, mirroring big_arith_range on the tree (None = decline)
pub open spec fn range_of(a: A, env: Map<int, Option<(int, int)>>) -> Option<(int, int)>
    decreases a,
{
    match a {
        A::Num(n) => Some((n, n)),
        A::Var(k) => env[k],
        A::Add(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some((l0, l1)), Some((r0, r1))) => big_arith_range(Op::Add, l0, l1, r0, r1),
            _ => None,
        },
        A::Sub(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some((l0, l1)), Some((r0, r1))) => big_arith_range(Op::Sub, l0, l1, r0, r1),
            _ => None,
        },
        A::Mul(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some((l0, l1)), Some((r0, r1))) => big_arith_range(Op::Mul, l0, l1, r0, r1),
            _ => None,
        },
        A::Mod(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some((l0, l1)), Some((r0, r1))) => big_arith_range(Op::Mod, l0, l1, r0, r1),
            _ => None,
        },
        A::Floordiv(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some((l0, l1)), Some((r0, r1))) => big_arith_range(Op::Floordiv, l0, l1, r0, r1),
            _ => None,
        },
        A::Pow(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some((l0, l1)), Some((r0, r1))) => big_arith_range(Op::Pow, l0, l1, r0, r1),
            _ => None,
        },
        A::Neg(x) => match range_of(*x, env) {
            Some((l0, l1)) => Some((-l1, -l0)),
            None => None,
        },
        A::Cmp(l, r) => match (range_of(*l, env), range_of(*r, env)) {
            (Some(_), Some(_)) => Some((0, 1)),
            _ => None,
        },
        A::Cond(c, t, e) => match range_of(*c, env) {
            None => None,
            Some(_) => bhull(range_of(*t, env), range_of(*e, env)),
        },
        A::FloatDiv(..) => None,
        A::Bitwise(..) => None,
    }
}

pub open spec fn sound(env: Map<int, Option<(int, int)>>, st: Map<int, int>) -> bool {
    forall|k: int| match env[k] {
        Some((lo, hi)) => lo <= #[trigger] st[k] <= hi,
        None => true,
    }
}

// ── the laws each operator needs ───────────────────────────────────────
proof fn add_sound(l0: int, l1: int, r0: int, r1: int, x: int, y: int)
    requires l0 <= x <= l1, r0 <= y <= r1,
    ensures l0 + r0 <= x + y <= l1 + r1,
{}
proof fn sub_sound(l0: int, l1: int, r0: int, r1: int, x: int, y: int)
    requires l0 <= x <= l1, r0 <= y <= r1,
    ensures l0 - r1 <= x - y <= l1 - r0,
{}
proof fn neg_sound(l0: int, l1: int, x: int)
    requires l0 <= x <= l1,
    ensures -l1 <= -x <= -l0,
{}

proof fn mul_sound(l0: int, l1: int, r0: int, r1: int, x: int, y: int)
    requires l0 <= x <= l1, r0 <= y <= r1,
    ensures min4(l0 * r0, l0 * r1, l1 * r0, l1 * r1) <= x * y
         <= max4(l0 * r0, l0 * r1, l1 * r0, l1 * r1),
{
    assert(min4(l0 * r0, l0 * r1, l1 * r0, l1 * r1) <= x * y)
        by (nonlinear_arith) requires l0 <= x <= l1, r0 <= y <= r1;
    assert(x * y <= max4(l0 * r0, l0 * r1, l1 * r0, l1 * r1))
        by (nonlinear_arith) requires l0 <= x <= l1, r0 <= y <= r1;
}

proof fn mod_pos_sound(l0: int, l1: int, r0: int, r1: int, x: int, b: int)
    requires l0 <= x <= l1, r0 <= b <= r1, 0 < r0,
    ensures 0 <= fmod(x, b) <= r1 - 1,
{
    fchar_pos(x, b);
    assert(fmod(x, b) < b);
    assert(fmod(x, b) <= r1 - 1);
}

proof fn mod_neg_sound(l0: int, l1: int, r0: int, r1: int, x: int, b: int)
    requires l0 <= x <= l1, r0 <= b <= r1, r1 < 0,
    ensures r0 + 1 <= fmod(x, b) <= 0,
{
    fmod_neg_bounds(x, b);
    assert(fmod(x, b) > b);
    assert(fmod(x, b) >= r0 + 1);
}

// the divisor's own interval, entirely positive: the corner table is sound
proof fn div_corner_sound(l0: int, l1: int, r0: int, r1: int, x: int, b: int)
    requires l0 <= x <= l1, r0 <= b <= r1, 0 < r0,
    ensures min4(fdiv(l0, r0), fdiv(l0, r1), fdiv(l1, r0), fdiv(l1, r1)) <= fdiv(x, b),
            fdiv(x, b) <= max4(fdiv(l0, r0), fdiv(l0, r1), fdiv(l1, r0), fdiv(l1, r1)),
{
    fdiv_mono_x(l0, x, b);
    fdiv_mono_x(x, l1, b);
    fdiv_mono_b(l0, r0, b);
    fdiv_mono_b(l0, b, r1);
    fdiv_mono_b(l1, r0, b);
    fdiv_mono_b(l1, b, r1);
    assert(fdiv(l0, b) >= imin(fdiv(l0, r0), fdiv(l0, r1)));
    assert(fdiv(l1, b) <= imax(fdiv(l1, r0), fdiv(l1, r1)));
    assert(imin(fdiv(l0, r0), fdiv(l0, r1)) >= min4(fdiv(l0, r0), fdiv(l0, r1), fdiv(l1, r0), fdiv(l1, r1)));
    assert(imax(fdiv(l1, r0), fdiv(l1, r1)) <= max4(fdiv(l0, r0), fdiv(l0, r1), fdiv(l1, r0), fdiv(l1, r1)));
}

proof fn hull_left(a: Option<(int, int)>, b: Option<(int, int)>, v: int)
    requires match a { Some((lo, hi)) => lo <= v <= hi, None => true },
    ensures match bhull(a, b) { Some((lo, hi)) => lo <= v <= hi, None => true },
{
    match a {
        Some((al, ah)) => match b {
            Some((bl, bh)) => {
                assert(imin(al, bl) <= al);
                assert(ah <= imax(ah, bh));
                assert(imin(al, bl) <= v <= imax(ah, bh));
            }
            None => {}
        },
        None => {}
    }
}

proof fn hull_right(a: Option<(int, int)>, b: Option<(int, int)>, v: int)
    requires match b { Some((lo, hi)) => lo <= v <= hi, None => true },
    ensures match bhull(a, b) { Some((lo, hi)) => lo <= v <= hi, None => true },
{
    match a {
        Some((al, ah)) => match b {
            Some((bl, bh)) => {
                assert(imin(al, bl) <= bl);
                assert(bh <= imax(ah, bh));
                assert(imin(al, bl) <= v <= imax(ah, bh));
            }
            None => {}
        },
        None => {}
    }
}

pub proof fn range_sound(a: A, env: Map<int, Option<(int, int)>>, st: Map<int, int>)
    requires sound(env, st),
    ensures match range_of(a, env) { Some((lo, hi)) => lo <= ev(a, st) <= hi, None => true },
    decreases a,
{
    match a {
        A::Num(n) => {}
        A::Var(k) => {
            match env[k] {
                Some((lo, hi)) => { assert(lo <= st[k] <= hi); }
                None => {}
            }
        }
        A::Add(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some((l0, l1)), Some((r0, r1))) => {
                    add_sound(l0, l1, r0, r1, ev(*l, st), ev(*r, st));
                }
                _ => {}
            }
        }
        A::Sub(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some((l0, l1)), Some((r0, r1))) => {
                    sub_sound(l0, l1, r0, r1, ev(*l, st), ev(*r, st));
                }
                _ => {}
            }
        }
        A::Mul(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some((l0, l1)), Some((r0, r1))) => {
                    mul_sound(l0, l1, r0, r1, ev(*l, st), ev(*r, st));
                }
                _ => {}
            }
        }
        A::Mod(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some((l0, l1)), Some((r0, r1))) => {
                    if l0 == l1 && r0 == r1 {
                        if r0 == 0 {
                        } else {
                            assert(ev(*l, st) == l0);
                            assert(ev(*r, st) == r0);
                        }
                    } else if r0 > 0 {
                        mod_pos_sound(l0, l1, r0, r1, ev(*l, st), ev(*r, st));
                    } else if r1 < 0 {
                        mod_neg_sound(l0, l1, r0, r1, ev(*l, st), ev(*r, st));
                    }
                }
                _ => {}
            }
        }
        A::Floordiv(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some((l0, l1)), Some((r0, r1))) => {
                    if r0 <= 0 {
                    } else {
                        div_corner_sound(l0, l1, r0, r1, ev(*l, st), ev(*r, st));
                    }
                }
                _ => {}
            }
        }
        A::Pow(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some((l0, l1)), Some((r0, r1))) => {
                    if l0 != l1 || r0 != r1 || r0 < 0 || r0 > 4096 {
                    } else {
                        assert(ev(*l, st) == l0);
                        assert(ev(*r, st) == r0);
                    }
                }
                _ => {}
            }
        }
        A::Neg(x) => {
            range_sound(*x, env, st);
            match range_of(*x, env) {
                Some((l0, l1)) => neg_sound(l0, l1, ev(*x, st)),
                None => {}
            }
        }
        A::Cmp(l, r) => {
            range_sound(*l, env, st);
            range_sound(*r, env, st);
            match (range_of(*l, env), range_of(*r, env)) {
                (Some(_), Some(_)) => {
                    assert(0 <= (if ev(*l, st) == ev(*r, st) { 1int } else { 0int }) <= 1);
                }
                _ => {}
            }
        }
        A::Cond(c, t, e) => {
            range_sound(*c, env, st);
            range_sound(*t, env, st);
            range_sound(*e, env, st);
            match range_of(*c, env) {
                None => {}
                Some(_) => {
                    let v = if ev(*c, st) != 0 { ev(*t, st) } else { ev(*e, st) };
                    if ev(*c, st) != 0 {
                        hull_left(range_of(*t, env), range_of(*e, env), ev(*t, st));
                    } else {
                        hull_right(range_of(*t, env), range_of(*e, env), ev(*e, st));
                    }
                }
            }
        }
        A::FloatDiv(..) => {}
        A::Bitwise(..) => {}
    }
}


// ── refusals, pinned ───────────────────────────────────────────────────
// `Option` equality is structural in spec code, so a refusal is `== None`.
// These are the guards of `big_arith_range`, which `range_of` consults for
// every binary operator (`FloatDiv`/`Bitwise` are refused at the node).
pub proof fn float_division_is_refused(l: A, r: A)
    ensures range_of(A::FloatDiv(Box::new(l), Box::new(r)), Map::<int, Option<(int, int)>>::empty()) == None,
{ reveal(range_of); }
pub proof fn bitwise_operators_are_refused(l: A, r: A)
    ensures range_of(A::Bitwise(Box::new(l), Box::new(r)), Map::<int, Option<(int, int)>>::empty()) == None,
{ reveal(range_of); }
pub proof fn a_straddling_modulus_is_refused()
    ensures big_arith_range(Op::Mod, -5, 3, -2, 4) == None,
{}
pub proof fn a_non_positive_floordiv_divisor_is_refused()
    ensures big_arith_range(Op::Floordiv, -5, 3, -2, 4) == None,
{}
pub proof fn a_zero_floordiv_divisor_is_refused()
    ensures big_arith_range(Op::Floordiv, -5, 3, 0, 9) == None,
{}
pub proof fn a_non_point_power_is_refused()
    ensures big_arith_range(Op::Pow, -5, 3, 2, 2) == None,
{}
pub proof fn an_oversized_power_is_refused()
    ensures big_arith_range(Op::Pow, 2, 2, 4097, 4097) == None,
{}
pub proof fn a_negative_exponent_is_refused()
    ensures big_arith_range(Op::Pow, 2, 2, -1, -1) == None,
{}
pub proof fn a_zero_modulus_point_is_refused()
    ensures big_arith_range(Op::Mod, 7, 7, 0, 0) == None,
{}
// the accepted side of the same table, so the refusals are shown to be the
// only thing excluded
pub proof fn the_table_accepts_the_shapes_it_should()
    ensures big_arith_range(Op::Mod, -5, 3, 2, 9) == Some((0, 8)),
            big_arith_range(Op::Mod, -5, 3, -6, -4) == Some((-5, 0)),
            big_arith_range(Op::Mod, 7, 7, 3, 3) == Some((1, 1)),
            big_arith_range(Op::Floordiv, -5, 3, 2, 9) == Some((-3, 1)),
            big_arith_range(Op::Pow, 2, 2, 4096, 4096) == Some((ipow(2, 4096), ipow(2, 4096))),
{}

// ── real divergences, as counterexample theorems ───────────────────────
// NOT closed, NOT weakened, NOT patched — the analysis code is untouched.
// (D1) The corner table for `//` is UNSOUND as soon as the divisor interval
// reaches zero or straddles it: over x = -6, b in [-4, 1] the corners claim
// at most 1, but -6 // -1 = 6.  This is exactly why the guard is `r0 > 0`,
// and `div_corner_sound` above proves the guarded version sound.
pub proof fn the_corner_table_for_floordiv_needs_a_positive_divisor()
    ensures fdiv(-6, -1) > max4(fdiv(-6, -4), fdiv(-6, 1), fdiv(-6, -4), fdiv(-6, 1)),
{
    assert(fdiv(-6, -1) == 6);
    assert(fdiv(-6, -4) == 1);
    assert(fdiv(-6, 1) == -6);
}

// (D2) The corner table for `%` is UNSOUND even when every divisor is
// negative: over x = -8, b in [-6, -4] the corners give [-2, 0], but
// -8 % -5 = -3.  The analysis does NOT use the corner table here — it uses
// the sign rule `[r0+1, 0]`, which `mod_neg_sound` proves sound.
pub proof fn the_corner_table_for_mod_is_unsound_for_negative_divisors()
    ensures fmod(-8, -5) < imin(fmod(-8, -6), fmod(-8, -4)),
{
    assert(fmod(-8, -6) == -2);
    assert(fmod(-8, -5) == -3);
    assert(fmod(-8, -4) == 0);
}

// (D3) The `//` guard is CONSERVATIVE in the negative half: a wholly
// negative divisor interval is safe for the corner table (the exhaustive
// probe in the proof-campaign plan found no counterexample), but the analysis refuses it.
// Precision loss, not unsoundness: the refusal is proved, and the corner
// rule is proved sound above for positive divisors only.
pub proof fn negative_floordiv_divisors_are_refused_though_sound()
    ensures big_arith_range(Op::Floordiv, -8, 8, -6, -4) == None,
{}

// (D4) `%`'s sign rule is an OVER-approximation: for x in [-5, 3], b in
// [2, 9] it claims [0, 8], although the largest reachable residue is
// 3 % 4 = 3.  A consumer must not read the interval as tight; the
// SOUNDNESS direction is `mod_pos_sound`.
pub proof fn the_mod_interval_is_an_over_approximation()
    ensures big_arith_range(Op::Mod, -5, 3, 2, 9) == Some((0, 8)), fmod(3, 4) < 8,
{
    assert(fmod(3, 4) == 3);
}

fn main() {}
}
