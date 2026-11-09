// Standalone model of two arithmetic-identity Tier-1 obligations:
//   (a) arith-normalize (`sh2perl/src/transforms/arith_normalize.rs`):
//       `0 - X` -> `-X`, `0 + X`/`X + 0` -> `X`, `X * 1`/`1 * X` -> `X`;
//   (b) bc-float-clean (`sh2perl/src/transforms/bc_float_clean.rs`):
//       stripping a trailing `+ 0.0` from a float bc-capture argument
//       preserves the value (`X + 0.0 == X` on finite floats).
//
// Proves the soundness obligations:
//
//   arith_norm_sound: each canonical rewrite preserves the denoted
//   integer value.
//   bc_clean_sound: stripping the trailing +0.0 preserves the denoted
//   float value for finite operands.
//
// Integers are int (mathematical); floats are modelled as int
// mantissa pairs would be overkill — instead the float claim is
// stated over a `Finite` newtype whose addition with zero is the
// identity BY CONSTRUCTION of the model (the real justification is
// IEEE-754: x+0.0 == x for finite x; infinities/NaN are refused by
// the pass's decimal-point gate and never reach the strip).
//
// What is modelled and what is NOT (fidelity, checked against the sources):
//   - arith-normalize's exact three rules plus the exact refusals
//     (`0 / X`, `0 % X`, `0 * X` never fold; nothing under `/`/`%`/`**`
//     is touched). Division/modulo/power are opaque constructors in the
//     model — uninterpreted, hence untouchable, exactly as the pass treats
//     them.
//   - bc-float-clean's gate (last literal ends `+ 0.0`, remainder has a
//     decimal point) as a parameter: the model proves the strip sound
//     GIVEN the gate; scanning echo pipelines for the pattern is traversal,
//     out of scope.
//   - NaN/infinity: not modelled (the pass only strips when a decimal
//     point is already present, i.e. the expression is float-typed by
//     construction; non-finite bc outputs are out of corpus scope).
use vstd::prelude::*;

verus! {

// ---- Integer arithmetic (arith-normalize) ----

pub enum Arith {
    Num(int),
    Var(nat),
    Neg(Box<Arith>),
    Add(Box<Arith>, Box<Arith>),
    Sub(Box<Arith>, Box<Arith>),
    Mul(Box<Arith>, Box<Arith>),
    Div(Box<Arith>, Box<Arith>),
    Mod(Box<Arith>, Box<Arith>),
    Pow(Box<Arith>, Box<Arith>),
}

pub type IStore = Map<nat, int>;

pub open spec fn ieval(a: Arith, s: IStore) -> int
    decreases a,
{
    match a {
        Arith::Num(n) => n,
        Arith::Var(v) => {
            if s.dom().contains(v) {
                s[v]
            } else {
                0
            }
        },
        Arith::Neg(x) => 0 - ieval(*x, s),
        Arith::Add(l, r) => ieval(*l, s) + ieval(*r, s),
        Arith::Sub(l, r) => ieval(*l, s) - ieval(*r, s),
        Arith::Mul(l, r) => ieval(*l, s) * ieval(*r, s),
        // Opaque: uninterpreted, never rewritten (refuse-on-doubt).
        Arith::Div(_, _) => 0,
        Arith::Mod(_, _) => 0,
        Arith::Pow(_, _) => 0,
    }
}

// One canonical rewrite step (exactly the pass's three rules).
pub open spec fn arith_norm_step(a: Arith) -> Arith {
    match a {
        Arith::Sub(l, r) => {
            match *l {
                Arith::Num(n) => {
                    if n == 0 {
                        Arith::Neg(r)
                    } else {
                        a
                    }
                },
                _ => a,
            }
        },
        Arith::Add(l, r) => {
            match (*l, *r) {
                (Arith::Num(n), _) => {
                    if n == 0 {
                        *r
                    } else {
                        Arith::Add(Box::new(Arith::Num(n)), r)
                    }
                },
                (_, Arith::Num(n)) => {
                    if n == 0 {
                        *l
                    } else {
                        Arith::Add(l, Box::new(Arith::Num(n)))
                    }
                },
                _ => a,
            }
        },
        Arith::Mul(l, r) => {
            match (*l, *r) {
                (Arith::Num(n), _) => {
                    if n == 1 {
                        *r
                    } else {
                        Arith::Mul(Box::new(Arith::Num(n)), r)
                    }
                },
                (_, Arith::Num(n)) => {
                    if n == 1 {
                        *l
                    } else {
                        Arith::Mul(l, Box::new(Arith::Num(n)))
                    }
                },
                _ => a,
            }
        },
        _ => a,
    }
}

// THE OBLIGATION (a): the canonical step preserves the integer value.
proof fn arith_norm_sound(a: Arith, s: IStore)
    ensures ieval(arith_norm_step(a), s) == ieval(a, s),
{
    match a {
        Arith::Num(_) => {},
        Arith::Var(_) => {},
        Arith::Neg(_) => {},
        Arith::Add(l, r) => {
            if l == Box::new(Arith::Num(0)) {
                assert(ieval(*l, s) == 0);
            } else if r == Box::new(Arith::Num(0)) {
                assert(ieval(*r, s) == 0);
            }
        },
        Arith::Sub(l, r) => {
            if l == Box::new(Arith::Num(0)) {
                assert(ieval(*l, s) == 0);
            }
        },
        Arith::Mul(l, r) => {
            if l == Box::new(Arith::Num(1)) {
                assert(ieval(*l, s) == 1);
            } else if r == Box::new(Arith::Num(1)) {
                assert(ieval(*r, s) == 1);
            }
        },
        Arith::Div(_, _) => {},
        Arith::Mod(_, _) => {},
        Arith::Pow(_, _) => {},
    }
}

// ---- Float identity strip (bc-float-clean) ----

// Finite floats: the model tracks finiteness explicitly; addition with
// the zero literal is the identity on finite values.
pub struct Finite {
    pub v: int,
}

pub open spec fn fadd(x: Finite, y: Finite) -> Finite {
    Finite { v: x.v + y.v }
}

pub open spec fn fzero() -> Finite {
    Finite { v: 0 }
}

// THE OBLIGATION (b): X + 0.0 == X on finite values.
proof fn bc_clean_sound(x: Finite)
    ensures fadd(x, fzero()) == x,
{
}

// ---- Fidelity pins (mirror the sources' own tests) ----

// `0 - X` -> `-X` (frontend unary-minus shape).
proof fn pin_zero_minus_negates()
    ensures arith_norm_step(
        Arith::Sub(Box::new(Arith::Num(0)), Box::new(Arith::Var(3nat))),
    ) == Arith::Neg(Box::new(Arith::Var(3nat))),
{
}

// `0 + X` / `X + 0` -> `X`.
proof fn pin_zero_add_identity()
    ensures
        arith_norm_step(Arith::Add(Box::new(Arith::Num(0)), Box::new(Arith::Var(3nat)))) == Arith::Var(
            3nat,
        ),
        arith_norm_step(Arith::Add(Box::new(Arith::Var(3nat)), Box::new(Arith::Num(0)))) == Arith::Var(
            3nat,
        ),
{
}

// `X * 1` / `1 * X` -> `X`.
proof fn pin_one_mul_identity()
    ensures
        arith_norm_step(Arith::Mul(Box::new(Arith::Var(3nat)), Box::new(Arith::Num(1)))) == Arith::Var(
            3nat,
        ),
        arith_norm_step(Arith::Mul(Box::new(Arith::Num(1)), Box::new(Arith::Var(3nat)))) == Arith::Var(
            3nat,
        ),
{
}

// Refusals: `0 * X` stays (forces X's error evaluation); `0 / X` stays.
proof fn pin_zero_mul_div_refuse()
    ensures
        arith_norm_step(Arith::Mul(Box::new(Arith::Num(0)), Box::new(Arith::Var(3nat)))) == Arith::Mul(
            Box::new(Arith::Num(0)),
            Box::new(Arith::Var(3nat)),
        ),
        arith_norm_step(Arith::Div(Box::new(Arith::Num(0)), Box::new(Arith::Var(3nat)))) == Arith::Div(
            Box::new(Arith::Num(0)),
            Box::new(Arith::Var(3nat)),
        ),
{
}

// Non-zero literals do not trigger: `5 - X` stays.
proof fn pin_nonzero_no_fold()
    ensures arith_norm_step(
        Arith::Sub(Box::new(Arith::Num(5)), Box::new(Arith::Var(3nat))),
    ) == Arith::Sub(Box::new(Arith::Num(5)), Box::new(Arith::Var(3nat))),
{
}

// Value check on a live store: `0 - x` with x=7 denotes -7 either way.
proof fn pin_norm_invisible_at_runtime()
    ensures ieval(
        arith_norm_step(Arith::Sub(Box::new(Arith::Num(0)), Box::new(Arith::Var(3nat)))),
        Map::empty().insert(3nat, 7),
    ) == -7,
{
    let orig = Arith::Sub(Box::new(Arith::Num(0)), Box::new(Arith::Var(3nat)));
    let st: IStore = Map::empty().insert(3nat, 7);
    assert(arith_norm_step(orig) == Arith::Neg(Box::new(Arith::Var(3nat))));
    assert(st.dom().contains(3nat));
    assert(st[3nat] == 7);
    assert(ieval(Arith::Var(3nat), st) == 7);
    assert(ieval(Arith::Neg(Box::new(Arith::Var(3nat))), st) == -7);
    arith_norm_sound(orig, st);
    assert(ieval(arith_norm_step(orig), st) == -7);
}

// bc strip is the identity on a finite shader value.
proof fn pin_bc_strip_identity()
    ensures fadd(Finite { v: 1065353216 }, fzero()) == (Finite { v: 1065353216 }),
{
    bc_clean_sound(Finite { v: 1065353216 });
}

fn main() {}

}
