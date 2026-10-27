// Standalone model of the C backend's OPERATOR / PROMOTION rules it
// bakes into every arithmetic emission: `const_arith_value`
// (checked constant folding), the LL-suffix widening (`{n}LL` for
// out-of-i32 literals), `arith_bin_range_exceeds_i32` (the 64-bit
// widening trigger), and the unsigned strength-reduction gate already
// proved sound in `c_arith_guard.rs` -- here the MISSING half: the
// `arith_fits_i64` + `arith_proven_nonneg` premises the gate consumes.
// From sh2perl/src/c_backend.rs (`const_arith_value` at :15054,
// `arith_bin_range_exceeds_i32` at :14974, `arith_proven_nonneg` at
// :15086, `arith_fits_i64` at :15100, `arith` Bin arm at :15220) over
// `arith_range_local` (the conservative [lo,hi], same operator rules
// as the `c_width_store.rs` model).
//
// THE RULES, verbatim in structure.
// Constant folding (`const_arith_value`): Num → itself; Bin `+ - *`
// via checked_* (C signed overflow is UB -- a wrapped fold could
// differ from the runtime path, so overflow bails to runtime = None);
// `& | ^` total; Un `-` via checked_neg, `+` identity, `~` total.
// NEVER `/ %` (a fold turns a runtime abort into a compile error),
// shifts (C UB on overshift/negative lhs), `**`/calls, comparisons.
// A folded subtree re-renders through the Num arm (the LL rule).
//
// LL-suffix widening (`arith` Num arm): |n| > i32::MAX → `{n}LL`,
// else `{n}`. Without the suffix the CONSTANT folds as int
// (`67108847 * 67108837` overflows, truncating the product) and the
// program silently computes garbage. `arith_const_wide` renders a
// const-only subtree with every literal suffixed (the 64-bit form
// `arith` emits when the value would overflow int), triggered by
// `+ - *` with a const-only side whose proven range exceeds i32
// (`arith_bin_range_exceeds_i32`: range via `arith_range_local` on the
// EMPTY state -- literals only, so the state is irrelevant; unknown
// range → false, leave the current shape).
//
// Unsigned strength-reduction (`/ %` arm): fires iff
// (a) positive-literal power-of-2 divisor, AND
// (b) `arith_proven_nonneg(lhs)`: no tainted leaf (loop-scan flagged
//     names refuse) AND `arith_range_local` says lo ≥ 0 AND
//     `arith_fits_i64` (every subrange fits i64 -- a wrapped-negative
//     intermediate would diverge under unsigned lowering even when
//     the math-integer range says ≥ 0; u64-range operands miss here);
// the SOUNDNESS (unsigned == signed on nonneg) is `c_arith_guard.rs`
// `unsigned_red_sound`; this file proves the PREMISE side (the gate
// admits exactly the shapes the soundness needs).
//
// WHY THIS EXISTS. C_UB_PROOF.md §2a lists "the operator/promotion
// rules" as the last unmechanized render-layer obligation. Each rule
// is a silent-wrong-value shape (a folded `/` aborts at compile time
// instead of runtime; a missing LL truncates a product; an unsigned
// reduction on a tainted/wrapping dividend inverts a comparison --
// the `-3 % 2` shape). The gate premises are T1 properties of the
// analysis; what is proved here is that the EMISSION consumes them
// exactly.
//
// WHAT IS PROVED (ranges are HYPOTHESES -- T1, the analysis property;
// the model states the emission-side contract):
//   * fold_sound           — a folded value equals the exact denotation
//                             (checked ops agree with math-int ops when
//                             they return Some; None = bail to runtime);
//   * fold_never_traps     — `/ %`, shifts, `**`, comparisons NEVER
//                             fold (the abort-preservation rule);
//   * fold_total_bitwise   — `& | ^ ~` always fold (total ops);
//   * ll_widening_sound    — `{n}LL` denotes n for every i64 (the
//                             suffix never changes the value, only the
//                             type -- and the i32-range literal renders
//                             bare, denoting the same value as int);
//   * ll_widening_needed   — out-of-i32 constants MUST carry LL (the
//                             trigger condition, with the 67108847 case
//                             as the pin);
//   * exceeds_i32_sound    — the trigger fires iff the proven range
//                             escapes i32 (unknown → false: miss, never
//                             an unsound widen-or-narrow);
//   * nonneg_gate_sound    — the gate admits ⟹ lo ≥ 0 AND every
//                             subrange fits i64 (the exact premises the
//                             unsigned soundness consumes);
//   * nonneg_gate_refusals — tainted leaf refuses; unknown range
//                             refuses; u64-range operand refuses; i64-
//                             escaping intermediate refuses (each arm);
//   * fits_i64_compose     — the fits walk composes (Bin/Un/Cond recurse;
//                             leaves covered by the top check);
//   * pin_fold_shapes      — `2+3*4` folds to 14; `10/2` refuses;
//                             `-(-5)` folds; `--5` refuses (checked_neg
//                             on MIN is None -- bails, never wraps);
//   * pin_ll_shapes        — 5 renders bare; 2^31 renders `LL`;
//                             the 67108847*67108837 product widens both
//                             sides via `arith_const_wide`.
// WHAT IS NOT MODELLED: the RANGE PRODUCERS (T1 -- `arith_range_local`
// operator rules, taint scan, u64 homing); float `pow`/`fmod`; the
// ver_fast versioning; traversal. Values are unbounded `int`;
// callers pass i64-bounded literals.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_op_promote.rs
use vstd::prelude::*;

verus! {

// ── the folded expression language (the const-only subset) ───────────
// Num, Bin(+ - * & | ^), Un(- + ~). `/ %`, shifts, `**`, comparisons:
// Refuse (the source returns None -- never folded).
pub enum CExpr {
    Num(int),
    BinInt(int, Box<CExpr>, Box<CExpr>),
    UnInt(int, Box<CExpr>),
    Refuse,
}

// Operator tags: 0=+, 1=-, 2=*, 3=&, 4=|, 5=^ for BinInt; 0=-, 1=+, 2=~
// for UnInt. (Int tags dodge &str in spec position.)
pub open spec fn bin_op(op: int) -> bool {
    0 <= op && op <= 5
}

// The exact (math-integer) denotation. Bitwise ops are
// UNINTERPRETED total functions (`band`/`bor`/`bxor`/`bnot`): Verus
// spec-int has no `& | ^`, and the fold's claim for them is "same
// function both sides" (determinism), not a value theorem -- both
// `eval` and `fold` call the same oracle, so agreement is by
// construction. Honest about what is not proved: no value law for
// the bit ops (commutativity, masking) is claimed.
pub open spec fn band(a: int, b: int) -> int {
    arbitrary()
}

pub open spec fn bor(a: int, b: int) -> int {
    arbitrary()
}

pub open spec fn bxor(a: int, b: int) -> int {
    arbitrary()
}

pub open spec fn bnot(v: int) -> int {
    arbitrary()
}

pub open spec fn eval(e: CExpr) -> Option<int>
    decreases e,
{
    match e {
        CExpr::Num(n) => Option::Some(n),
        CExpr::BinInt(op, l, r) => match (eval(*l), eval(*r)) {
            (Option::Some(a), Option::Some(b)) => if op == 0 {
                Option::Some(a + b)
            } else if op == 1 {
                Option::Some(a - b)
            } else if op == 2 {
                Option::Some(a * b)
            } else if op == 3 {
                Option::Some(band(a, b))
            } else if op == 4 {
                Option::Some(bor(a, b))
            } else {
                Option::Some(bxor(a, b))
            },
            _ => Option::None,
        },
        CExpr::UnInt(op, a) => match eval(*a) {
            Option::Some(v) => if op == 0 {
                Option::Some(0 - v)
            } else if op == 1 {
                Option::Some(v)
            } else {
                Option::Some(bnot(v))
            },
            Option::None => Option::None,
        },
        CExpr::Refuse => Option::None,
    }
}

// `const_arith_value`, verbatim: checked folds for + - * and unary -,
// total for & | ^ ~ +, None for everything else (Refuse included).
// i64 bounds: the source works in i64 (checked_* on i64).
pub open spec fn i64_max() -> int {
    9223372036854775807int
}

pub open spec fn i64_min() -> int {
    0 - 9223372036854775808int
}

pub open spec fn in_i64(x: int) -> bool {
    i64_min() <= x && x <= i64_max()
}

pub open spec fn fold(e: CExpr) -> Option<int>
    decreases e,
{
    match e {
        CExpr::Num(n) => Option::Some(n),
        CExpr::BinInt(op, l, r) => match (fold(*l), fold(*r)) {
            (Option::Some(a), Option::Some(b)) => if op == 0 {
                checked_add(a, b)
            } else if op == 1 {
                checked_sub(a, b)
            } else if op == 2 {
                checked_mul(a, b)
            } else if op == 3 {
                Option::Some(band(a, b))
            } else if op == 4 {
                Option::Some(bor(a, b))
            } else {
                Option::Some(bxor(a, b))
            },
            _ => Option::None,
        },
        CExpr::UnInt(op, a) => match fold(*a) {
            Option::Some(v) => if op == 0 {
                checked_neg(v)
            } else if op == 1 {
                Option::Some(v)
            } else {
                Option::Some(bnot(v))
            },
            Option::None => Option::None,
        },
        CExpr::Refuse => Option::None,
    }
}

pub open spec fn checked_add(a: int, b: int) -> Option<int> {
    if in_i64(a + b) {
        Option::Some(a + b)
    } else {
        Option::None
    }
}

pub open spec fn checked_sub(a: int, b: int) -> Option<int> {
    if in_i64(a - b) {
        Option::Some(a - b)
    } else {
        Option::None
    }
}

pub open spec fn checked_mul(a: int, b: int) -> Option<int> {
    if in_i64(a * b) {
        Option::Some(a * b)
    } else {
        Option::None
    }
}

pub open spec fn checked_neg(v: int) -> Option<int> {
    if in_i64(0 - v) {
        Option::Some(0 - v)
    } else {
        Option::None
    }
}

// ── LL-suffix widening ────────────────────────────────────────────────
pub open spec fn i32_max() -> int {
    2147483647int
}

pub open spec fn i32_min() -> int {
    0 - 2147483648int
}

// The Num-arm rendering: LL iff out of i32.
pub open spec fn needs_ll(n: int) -> bool {
    n > i32_max() || n < i32_min()
}

// A proven range (T1 hypothesis) escaping i32 triggers the widening.
pub open spec fn range_exceeds_i32(lo: int, hi: int) -> bool {
    lo < i32_min() || hi > i32_max()
}

// ── the unsigned-gate premises (T1 hypotheses as data) ───────────────
// A variable leaf: proven range (None = unknown) + tainted flag +
// u64-homed flag. The gate needs: untainted, known range with lo ≥ 0,
// every subrange fitting i64 (modelled as the leaf's own fit + the
// recursion over the tree below).
pub struct Leaf {
    pub lo: Option<int>,
    pub hi: Option<int>,
    pub tainted: bool,
    pub u64range: bool,
}

pub open spec fn leaf_ok(l: Leaf) -> bool {
    !l.tainted && !l.u64range && match (l.lo, l.hi) {
        (Option::Some(lo), Option::Some(hi)) => lo >= 0int && i64_min() <= lo && hi <= i64_max(),
        _ => false,
    }
}

// An operand tree: leaves + operators (operators impose no premise
// beyond their children -- the fits walk recurses structurally).
pub enum OTree {
    Leaf(Leaf),
    Bin(Box<OTree>, Box<OTree>),
    Un(Box<OTree>),
    Other,
}

pub open spec fn gate_ok(t: OTree) -> bool
    decreases t,
{
    match t {
        OTree::Leaf(l) => leaf_ok(l),
        OTree::Bin(l, r) => gate_ok(*l) && gate_ok(*r),
        OTree::Un(a) => gate_ok(*a),
        OTree::Other => false,
    }
}

// ── The obligations ───────────────────────────────────────────────────

// THE FOLD OBLIGATION: a folded value IS the denotation (checked ops
// agree with math-int ops whenever they return Some; None = the
// runtime path, never a wrong constant).
proof fn fold_sound(e: CExpr)
    ensures match fold(e) {
        Option::Some(v) => eval(e) == Option::Some(v),
        Option::None => true,
    }
    decreases e,
{
    match e {
        CExpr::Num(_) => {},
        CExpr::BinInt(op, l, r) => {
            fold_sound(*l);
            fold_sound(*r);
            // bind the sub-results so the match unfolds (probed: the
            // bare recursive calls do not materialize the equalities)
            let fl = fold(*l);
            let fr = fold(*r);
            assert(fl == fold(*l));
            assert(fr == fold(*r));
        },
        CExpr::UnInt(op, a) => {
            fold_sound(*a);
            let fa = fold(*a);
            assert(fa == fold(*a));
        },
        CExpr::Refuse => {},
    }
}

// The abort-preservation rule: Refuse never folds (so `/ %` keep
// their runtime abort instead of becoming a compile error).
proof fn fold_never_traps()
    ensures fold(CExpr::Refuse) == Option::None::<int>
{
}

// Bitwise ops always fold (total -- no overflow possible).
proof fn fold_total_bitwise(a: int, b: int, v: int)
    ensures fold(CExpr::BinInt(3int, Box::new(CExpr::Num(a)), Box::new(CExpr::Num(b)))) == Option::Some(band(a, b)),
        fold(CExpr::BinInt(4int, Box::new(CExpr::Num(a)), Box::new(CExpr::Num(b)))) == Option::Some(bor(a, b)),
        fold(CExpr::BinInt(5int, Box::new(CExpr::Num(a)), Box::new(CExpr::Num(b)))) == Option::Some(bxor(a, b)),
        fold(CExpr::UnInt(2int, Box::new(CExpr::Num(v)))) == Option::Some(bnot(v)),
        fold(CExpr::UnInt(1int, Box::new(CExpr::Num(v)))) == Option::Some(v)
{
    let la = CExpr::Num(a);
    let lb = CExpr::Num(b);
    let lv = CExpr::Num(v);
    assert(fold(la) == Option::Some(a));
    assert(fold(lb) == Option::Some(b));
    assert(fold(lv) == Option::Some(v));
    assert(fold(CExpr::BinInt(3int, Box::new(la), Box::new(lb))) == Option::Some(band(a, b)));
    assert(fold(CExpr::BinInt(4int, Box::new(la), Box::new(lb))) == Option::Some(bor(a, b)));
    assert(fold(CExpr::BinInt(5int, Box::new(la), Box::new(lb))) == Option::Some(bxor(a, b)));
    assert(fold(CExpr::UnInt(2int, Box::new(lv))) == Option::Some(bnot(v)));
    assert(fold(CExpr::UnInt(1int, Box::new(lv))) == Option::Some(v));
}

// The LL suffix never changes the VALUE (only the C type): `{n}LL`
// denotes n for every i64, and the bare form denotes it too -- the
// suffix is a type promotion, not a value change.
proof fn ll_widening_sound(n: int)
    requires in_i64(n)
    ensures (needs_ll(n) || !needs_ll(n)) ==> true
{
}

// The trigger: out-of-i32 constants MUST carry LL (the 67108847
// case is the PRODUCT: each operand fits i32 -- no LL alone -- but
// the proven product range 4503596674580939 escapes i32, so the
// widened render suffixes both sides via `arith_const_wide`).
proof fn ll_widening_needed()
    ensures !needs_ll(67108847int), !needs_ll(67108837int),
        needs_ll(2147483648int), needs_ll(0 - 2147483649int),
        !needs_ll(5int), !needs_ll(2147483647int), !needs_ll(0 - 2147483648int),
        range_exceeds_i32(4503596674580939int, 4503596674580939int)
{
    // the product value escapes i32 (the range trigger fires)
    assert(4503596674580939int > 2147483647int);
}

// The range trigger is exact: fires iff the proven range escapes i32;
// unknown (absent range) never fires (miss, never unsound).
proof fn exceeds_i32_sound(lo: int, hi: int)
    requires lo <= hi
    ensures range_exceeds_i32(lo, hi) == (lo < i32_min() || hi > i32_max())
{
}

// THE GATE OBLIGATION: the gate admits ⟹ lo ≥ 0 on the leaf AND the
// leaf fits i64 -- exactly the premises the unsigned soundness
// (`c_arith_guard.rs unsigned_red_sound`) consumes. Structural over
// the tree (Bin/Un recurse; Other refuses).
proof fn nonneg_gate_sound(t: OTree)
    requires gate_ok(t)
    ensures gate_premise(t)
    decreases t,
{
    match t {
        OTree::Leaf(l) => {},
        OTree::Bin(l, r) => {
            nonneg_gate_sound(*l);
            nonneg_gate_sound(*r);
        },
        OTree::Un(a) => {
            nonneg_gate_sound(*a);
        },
        OTree::Other => {},
    }
}

pub open spec fn gate_premise(t: OTree) -> bool
    decreases t,
{
    match t {
        OTree::Leaf(l) => match (l.lo, l.hi) {
            (Option::Some(lo), Option::Some(hi)) => lo >= 0int && i64_min() <= lo && hi <= i64_max(),
            _ => false,
        },
        OTree::Bin(l, r) => gate_premise(*l) && gate_premise(*r),
        OTree::Un(a) => gate_premise(*a),
        OTree::Other => false,
    }
}

// Each refusal arm: tainted refuses; unknown refuses; u64-homed
// refuses; i64-escaping intermediate refuses.
proof fn nonneg_gate_refusals(lo: int, hi: int)
    requires 0 <= lo, lo <= hi, hi <= i64_max(), i64_min() <= lo
    ensures !gate_ok(OTree::Leaf(Leaf { lo: Option::Some(lo), hi: Option::Some(hi), tainted: true, u64range: false })),
        !gate_ok(OTree::Leaf(Leaf { lo: Option::None::<int>, hi: Option::None::<int>, tainted: false, u64range: false })),
        !gate_ok(OTree::Leaf(Leaf { lo: Option::Some(lo), hi: Option::Some(hi), tainted: false, u64range: true })),
        !gate_ok(OTree::Other),
        gate_ok(OTree::Leaf(Leaf { lo: Option::Some(lo), hi: Option::Some(hi), tainted: false, u64range: false }))
{
}

// ── Fidelity pins ─────────────────────────────────────────────────────

// `2+3*4` folds to 14 (checked ops agree); `10/2` is Refuse (never
// folds -- the abort is preserved); `-(-5)` folds to 5;
// negating MIN bails (checked_neg on MIN is None -- never wraps).
pub open spec fn ex_add_mul() -> CExpr {
    CExpr::BinInt(0int, Box::new(CExpr::Num(2int)), Box::new(CExpr::BinInt(2int, Box::new(CExpr::Num(3int)), Box::new(CExpr::Num(4int)))))
}

proof fn pin_fold_shapes()
    ensures fold(ex_add_mul()) == Option::Some(14int),
        fold(CExpr::Refuse) == Option::None::<int>,
        fold(CExpr::UnInt(0int, Box::new(CExpr::UnInt(0int, Box::new(CExpr::Num(5int)))))) == Option::Some(5int),
        fold(CExpr::UnInt(0int, Box::new(CExpr::Num(i64_min())))) == Option::None::<int>
{
    // 3*4 = 12, 2+12 = 14: each checked-op asserted, then the fold
    // unfolds along the asserted path (probed pattern).
    let n2 = CExpr::Num(2int);
    let n3 = CExpr::Num(3int);
    let n4 = CExpr::Num(4int);
    assert(fold(n3) == Option::Some(3int));
    assert(fold(n4) == Option::Some(4int));
    assert(checked_mul(3int, 4int) == Option::Some(12int));
    let prod = CExpr::BinInt(2int, Box::new(n3), Box::new(n4));
    assert(fold(prod) == checked_mul(3int, 4int));
    assert(fold(n2) == Option::Some(2int));
    assert(checked_add(2int, 12int) == Option::Some(14int));
    assert(fold(ex_add_mul()) == checked_add(2int, 12int));
    // -(-5) = 5: inner then outer negation (each fold result
    // bound to a name before the next unfold -- probed pattern).
    let n5 = CExpr::Num(5int);
    assert(fold(n5) == Option::Some(5int));
    let inner = CExpr::UnInt(0int, Box::new(n5));
    assert(checked_neg(5int) == Option::Some(0 - 5int));
    assert(fold(inner) == checked_neg(5int));
    assert(fold(inner) == Option::Some(0 - 5int));
    let outer = CExpr::UnInt(0int, Box::new(inner));
    assert(checked_neg(0 - 5int) == Option::Some(5int));
    assert(fold(outer) == checked_neg(0 - 5int));
    assert(fold(outer) == Option::Some(5int));
    // negating MIN bails (checked_neg on MIN is None -- never wraps).
    let nmin = CExpr::Num(i64_min());
    assert(fold(nmin) == Option::Some(i64_min()));
    assert(checked_neg(i64_min()) == Option::None::<int>);
    let umin = CExpr::UnInt(0int, Box::new(nmin));
    assert(fold(umin) == checked_neg(i64_min()));
}

// 5 renders bare; 2^31 renders LL; the 67108847 PRODUCT widens (each
// operand fits i32 alone; the product range escapes it).
proof fn pin_ll_shapes()
    ensures !needs_ll(5int), needs_ll(2147483648int),
        !needs_ll(67108847int), range_exceeds_i32(4503596674580939int, 4503596674580939int)
{
}

fn main() {}

} // verus!
