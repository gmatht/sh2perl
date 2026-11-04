// Standalone model of the C backend's CHECKED i64 ARITHMETIC TIER —
// the overflow-flag protocol behind both fast arms that render an
// ArithAst over `long long` with `__builtin_{add,sub,mul}_overflow`
// (sh2perl/src/c_backend.rs):
//
//   PROTOCOL A — tiered_i64_expr (≈10319) + tiered_store_general (≈10365):
//     int ov = any-participant-promoted;
//     if (!ov) { t = <post-order walk, `ov |= __builtin_*(l, r, &t)` at
//                 each node, unary minus as __builtin_sub_overflow(0, x)>;
//                if (!ov) fast = t; }        // commit gated on ov
//     if (ov) { spill every participant to its mpz slot; exact recompute }
//     A refused shape (tiered_i64_expr -> None) sets `ov = 1` — the
//     source's own comment: "shape drifted from the selector: stay exact
//     via the mpz arm".
//
//   PROTOCOL B — the speculative loop arm: entry gates (≈10577-10640)
//     (`if (g_big) ovf = 1`, `mpz_fits_slong_p else ovf = 1`) then
//     `if (!ovf) { while (cond) { <stmts> } }` + exact replay
//     `if (ovf)`, with spec_checked_expr (≈10774) emitting per node
//     `if (__builtin_*(l, r, &t)) { ovf = 1; break; }` and
//     spec_checked_store (≈10844) emitting `{id} = {t};` AFTER the
//     checks — a flag `break`s out of the enclosing while BEFORE the
//     store line, so the overflowing store never commits and the replay
//     discards the partial fast state from the loop-entry snapshots.
//
// WHY THIS EXISTS. C_UB_PROOF.md §2a names this as the first
// render-discipline obligation to mechanize, verbatim: "the theorem is
// 'the ov flag is set exactly when the exact result leaves long long',
// i.e. the tier's escape hatch to GMP is only taken when needed *and*
// always taken when needed". The backend relies on it in informal
// comments and in no proof. An underestimated width is the one failure
// C_UB_PROOF.md §1 marks as SILENT (no guard, no exit code); this tier
// is the guarded counterpart, and the guard is the `ov` flag.
//
// WHAT IS PROVED (all unconditional — the model's leaves are the
// emission's own: a Num is an i64 by the source's type, a Var's shadow
// is a long long or passed the entry mpz_fits_slong_p gate):
//   * walk_value_sound       — whenever no flag is set, the walk's value
//                              equals the shIR denotation `eval`;
//   * flag_iff_exact_exits   — THE OBLIGATION: `ov == false` iff every
//                              flagged node's EXACT value lies in i64
//                              (both directions: the escape hatch is
//                              taken only when needed AND whenever
//                              needed);
//   * air_value_sound        — the flag-free walk is the denotation and
//                              re-establishes the range invariant;
//   * break_matches_flag     — the short-circuit (break) protocol flags
//                              iff the continue (|=) protocol does,
//                              which is why one `walk` models both;
//   * commit_a_sound         — protocol A commits `fast` only under
//                              `!ov`, so a committed value IS the
//                              denotation;
//   * commit_a_spill_exactly_when_needed — the GMP spill runs iff a
//                              participant was already promoted or some
//                              exact value left i64;
//   * commit_b_sound / commit_b_entry_refusal — protocol B's store is
//                              reachable only when the entry gates held
//                              and no flag fired;
//   * refusal_forces_spill   — a shape the selector refuses sets the
//                              flag (the source's None -> ov = 1 arm);
//   * raw_divisor_gate       — the unflagged `/`/`%` arm is reachable
//                              only under spec_rhs_ok's positive-LITERAL
//                              divisor, which excludes both UBs of raw
//                              C division (zero divisor, LLONG_MIN / -1);
//   * unguarded_commit_ships_wrap — NEGATIVE CONTROL: delete the
//                              `if (!ov)` gate and MAX+1 "commits"
//                              -2^63 (the wrap64 bits) instead of the
//                              denotation — the concrete counterexample
//                              the guard exists to prevent;
//   * protocol_b_skips_the_overflowing_store — NEGATIVE CONTROL: under
//                              the same witness protocol B breaks before
//                              the store, so nothing wrapped is stored.
//
// WHAT IS NOT MODELLED: the GMP replay's own exactness (the mpz arm is
// assumed exact — that is GMP's contract, not the renderer's); the
// operand renderers (a leaf's C read is its shadow VALUE, supplied by
// the store); width NARROWING upstream (var_types/arith_range_local
// choosing int32_t mirrors — C_UB_PROOF.md's separate "proving
// passes" layer); ArithAst::Cast, which both spec_rhs_ok and
// tiered_i64_expr traverse transparently (the model's trees are
// post-Cast); `/` `%` by DYNAMIC divisors, which the tier refuses
// (spec_rhs_ok) and the plain path guards instead (c_backend.rs ≈15269).
//
// CORRESPONDENCE: `eval` is the shIR denotation of this fragment — the
// same reading ArithIdentity.thy gives (apply_op over unbounded int),
// here with truncating division as bash and C agree on it; the flagged
// fragment {+,-,*} coincides exactly. The shIR-side machine-width
// reading (wrap64 over the denotation) is stated there in
// ArithIdentity/ArithWidth.thy §4, so the two levels state one
// contract.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_checked_arith.rs
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

// The ArithAst shapes, mirroring ArithAst + spec_rhs_ok (c_backend.rs
// ≈36844). `Refuse` is every shape the selector rejects: the tier never
// renders it, and protocol A's fallback sets `ov = 1` for it.
pub enum Tree {
    // The source's Num holds i64 — the literal is in range BY TYPE.
    Num(int),
    Var(Name),
    Neg(Box<Tree>),
    Add(Box<Tree>, Box<Tree>),
    Sub(Box<Tree>, Box<Tree>),
    Mul(Box<Tree>, Box<Tree>),
    // Rendered RAW (no builtin, no flag): spec_rhs_ok admits them only
    // with a positive-literal divisor (`ok` mirrors that gate).
    Div(Box<Tree>, Box<Tree>),
    Mod(Box<Tree>, Box<Tree>),
    Refuse,
}

// A variable read: an unset name reads as 0 (both the walk and eval
// read it the same way, so the value theorems do not depend on
// definedness — the shadow's entry gate owns that).
pub open spec fn rd(s: Store, x: Name) -> int {
    if s.dom().contains(x) {
        s[x]
    } else {
        0int
    }
}

// ---- the shIR denotation (exact, unbounded) ----

// Truncating division, as bash and C both do it (intmax_t / long long
// truncate toward zero). Totalized: a zero divisor reads 0 — under
// `ok` it cannot occur (positive-literal gate), and outside `ok` the
// arm is never taken.
pub open spec fn tdiv(a: int, b: int) -> int {
    if b == 0 {
        0int
    } else if a < 0int {
        if b < 0int {
            (0 - a) / (0 - b)
        } else {
            0 - ((0 - a) / b)
        }
    } else if b < 0int {
        0 - (a / (0 - b))
    } else {
        a / b
    }
}

pub open spec fn tmod(a: int, b: int) -> int {
    if b == 0int {
        0int
    } else {
        a - tdiv(a, b) * b
    }
}

pub open spec fn eval(t: Tree, s: Store) -> int
    decreases t,
{
    match t {
        Tree::Num(n) => n,
        Tree::Var(x) => rd(s, x),
        Tree::Neg(a) => 0 - eval(*a, s),
        Tree::Add(l, r) => eval(*l, s) + eval(*r, s),
        Tree::Sub(l, r) => eval(*l, s) - eval(*r, s),
        Tree::Mul(l, r) => eval(*l, s) * eval(*r, s),
        Tree::Div(l, r) => tdiv(eval(*l, s), eval(*r, s)),
        Tree::Mod(l, r) => tmod(eval(*l, s), eval(*r, s)),
        // Unreachable under `ok`; 0 keeps the model total.
        Tree::Refuse => 0int,
    }
}

// ---- the source's shape gate (spec_rhs_ok, verbatim in structure) ----

pub open spec fn is_pos_lit(t: Tree) -> bool {
    match t {
        Tree::Num(n) => n > 0int,
        _ => false,
    }
}

pub open spec fn ok(t: Tree) -> bool
    decreases t,
{
    match t {
        Tree::Num(_) => true,
        Tree::Var(_) => true,
        Tree::Neg(a) => ok(*a),
        Tree::Add(l, r) => ok(*l) && ok(*r),
        Tree::Sub(l, r) => ok(*l) && ok(*r),
        Tree::Mul(l, r) => ok(*l) && ok(*r),
        // Positive literal divisor only: raw `l / r` — a zero divisor
        // would SIGFPE and LLONG_MIN / -1 is UB; a negative literal
        // keeps the exact GMP path instead.
        Tree::Div(l, r) => ok(*l) && is_pos_lit(*r),
        Tree::Mod(l, r) => ok(*l) && is_pos_lit(*r),
        Tree::Refuse => false,
    }
}

// ---- machine width: i64 and the builtins ----

pub open spec fn i64_max() -> int {
    9223372036854775807int
}

pub open spec fn i64_min() -> int {
    0 - 9223372036854775808int
}

pub open spec fn pow2_64() -> int {
    18446744073709551616int
}

pub open spec fn in64(x: int) -> bool {
    i64_min() <= x && x <= i64_max()
}

// The bits C stores through `*out` when the exact result leaves i64:
// two's-complement wrap of the exact value.
pub open spec fn wrap64(x: int) -> int {
    let m = x % pow2_64();
    let m = if m < 0int { m + pow2_64() } else { m };
    if m >= pow2_64() / 2 {
        m - pow2_64()
    } else {
        m
    }
}

// `__builtin_add_overflow(a, b, &out)` semantics over int inputs: the
// flag says the EXACT result left i64; `out` is the exact result when
// it did not, the wrapped bits when it did.
pub open spec fn add_ov(a: int, b: int) -> (bool, int) {
    let x = a + b;
    if in64(x) {
        (false, x)
    } else {
        (true, wrap64(x))
    }
}

pub open spec fn sub_ov(a: int, b: int) -> (bool, int) {
    let x = a - b;
    if in64(x) {
        (false, x)
    } else {
        (true, wrap64(x))
    }
}

pub open spec fn mul_ov(a: int, b: int) -> (bool, int) {
    let x = a * b;
    if in64(x) {
        (false, x)
    } else {
        (true, wrap64(x))
    }
}

// ---- the emission walk (protocol A's continue-on-flag channel) ----
//
// Post-order exactly as tiered_i64_expr emits it: children first, the
// builtin over the CHILDREN'S TEMPS (wrapped if a child flagged), the
// flag OR-ed into the running `ov`. `Refuse` is the caller's
// `None => ov = 1` fallback.
pub open spec fn walk(t: Tree, s: Store) -> (bool, int)
    decreases t,
{
    match t {
        Tree::Num(n) => (false, n),
        Tree::Var(x) => (false, rd(s, x)),
        Tree::Neg(a) => {
            let wa = walk(*a, s);
            let f = sub_ov(0int, wa.1);
            (wa.0 || f.0, f.1)
        },
        Tree::Add(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let f = add_ov(wl.1, wr.1);
            (wl.0 || wr.0 || f.0, f.1)
        },
        Tree::Sub(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let f = sub_ov(wl.1, wr.1);
            (wl.0 || wr.0 || f.0, f.1)
        },
        Tree::Mul(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let f = mul_ov(wl.1, wr.1);
            (wl.0 || wr.0 || f.0, f.1)
        },
        // Raw arm: no builtin, no flag — the divisor gate makes it
        // total, and it contributes no escape-hatch bit.
        Tree::Div(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            (wl.0 || wr.0, tdiv(wl.1, wr.1))
        },
        Tree::Mod(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            (wl.0 || wr.0, tmod(wl.1, wr.1))
        },
        Tree::Refuse => (true, 0int),
    }
}

// ---- the range invariant, shaped exactly like the flag ----
//
// `air` = every FLAGGED node's exact value lies in i64. Leaves and the
// raw `/` `%` arm carry no check because they emit no builtin — the
// flag can only originate at Neg/Add/Sub/Mul, so this is precisely the
// set the equivalence below relates to `ov`.
pub open spec fn air(t: Tree, s: Store) -> bool
    decreases t,
{
    match t {
        Tree::Num(_) => true,
        Tree::Var(_) => true,
        Tree::Neg(a) => air(*a, s) && in64(0 - eval(*a, s)),
        Tree::Add(l, r) => air(*l, s) && air(*r, s) && in64(eval(*l, s) + eval(*r, s)),
        Tree::Sub(l, r) => air(*l, s) && air(*r, s) && in64(eval(*l, s) - eval(*r, s)),
        Tree::Mul(l, r) => air(*l, s) && air(*r, s) && in64(eval(*l, s) * eval(*r, s)),
        Tree::Div(l, r) => air(*l, s) && air(*r, s),
        Tree::Mod(l, r) => air(*l, s) && air(*r, s),
        Tree::Refuse => false,
    }
}

// ---- protocol B: the short-circuit (break) channel ----
//
// spec_checked_expr stops the whole statement at the first flag: the
// siblings after it are never evaluated (the `break` leaves the while
// body before its store line). Value is only meaningful when no flag
// fired — which is exactly the case in which the store commits.
pub open spec fn walk_b(t: Tree, s: Store) -> (bool, int)
    decreases t,
{
    match t {
        Tree::Num(n) => (false, n),
        Tree::Var(x) => (false, rd(s, x)),
        Tree::Neg(a) => {
            let wa = walk_b(*a, s);
            if wa.0 {
                (true, 0int)
            } else {
                let f = sub_ov(0int, wa.1);
                (f.0, f.1)
            }
        },
        Tree::Add(l, r) => {
            let wl = walk_b(*l, s);
            if wl.0 {
                (true, 0int)
            } else {
                let wr = walk_b(*r, s);
                if wr.0 {
                    (true, 0int)
                } else {
                    let f = add_ov(wl.1, wr.1);
                    (f.0, f.1)
                }
            }
        },
        Tree::Sub(l, r) => {
            let wl = walk_b(*l, s);
            if wl.0 {
                (true, 0int)
            } else {
                let wr = walk_b(*r, s);
                if wr.0 {
                    (true, 0int)
                } else {
                    let f = sub_ov(wl.1, wr.1);
                    (f.0, f.1)
                }
            }
        },
        Tree::Mul(l, r) => {
            let wl = walk_b(*l, s);
            if wl.0 {
                (true, 0int)
            } else {
                let wr = walk_b(*r, s);
                if wr.0 {
                    (true, 0int)
                } else {
                    let f = mul_ov(wl.1, wr.1);
                    (f.0, f.1)
                }
            }
        },
        Tree::Div(l, r) => {
            let wl = walk_b(*l, s);
            if wl.0 {
                (true, 0int)
            } else {
                let wr = walk_b(*r, s);
                if wr.0 {
                    (true, 0int)
                } else {
                    (false, tdiv(wl.1, wr.1))
                }
            }
        },
        Tree::Mod(l, r) => {
            let wl = walk_b(*l, s);
            if wl.0 {
                (true, 0int)
            } else {
                let wr = walk_b(*r, s);
                if wr.0 {
                    (true, 0int)
                } else {
                    (false, tmod(wl.1, wr.1))
                }
            }
        },
        Tree::Refuse => (true, 0int),
    }
}

// ---- the two commit protocols ----
//
// A: `fast` is assigned only under `!ov` and only when every
// participant is still authoritative in its i64 mirror (none `_big`).
// `None` = the spill arm ran (participants promoted to mpz, or
// overflow).
pub open spec fn commit_a(participant_promoted: bool, t: Tree, s: Store) -> Option<int> {
    let w = walk(t, s);
    if participant_promoted || w.0 {
        Option::None
    } else {
        Option::Some(w.1)
    }
}

// B: the store line runs only when the entry gates held (`entry_ok`:
// every growth var's `_big` clear and its shadow fitting `long long`,
// which is what the emitted `if (g_big) ovf = 1` /
// `mpz_fits_slong_p else ovf = 1` pair establishes) and the RHS
// evaluation broke no flag. `None` = the break (or an entry refusal)
// skipped the store; the replay discards the fast state.
pub open spec fn commit_b(entry_ok: bool, t: Tree, s: Store) -> Option<int> {
    if !entry_ok {
        Option::None
    } else {
        let w = walk_b(t, s);
        if w.0 {
            Option::None
        } else {
            Option::Some(w.1)
        }
    }
}

// ── The obligations ────────────────────────────────────────────────────

// Flag-free walk = the denotation, unconditionally: no flag means every
// flagged node took the exact branch, so every temp is its node's exact
// value all the way up. The raw `/` `%` arm passes its children's exact
// values through tdiv/tmod unchanged.
proof fn walk_value_sound(t: Tree, s: Store)
    ensures !walk(t, s).0 ==> walk(t, s).1 == eval(t, s),
    decreases t,
{
    match t {
        Tree::Num(n) => {},
        Tree::Var(x) => {},
        Tree::Neg(a) => {
            let wa = walk(*a, s);
            walk_value_sound(*a, s);
            if !wa.0 {
                let f = sub_ov(0int, wa.1);
                if !f.0 {
                    assert(wa.1 == eval(*a, s));
                    assert(f.1 == 0int - wa.1);
                }
            }
        },
        Tree::Add(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            walk_value_sound(*l, s);
            walk_value_sound(*r, s);
            if !wl.0 && !wr.0 {
                let f = add_ov(wl.1, wr.1);
                if !f.0 {
                    assert(f.1 == wl.1 + wr.1);
                }
            }
        },
        Tree::Sub(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            walk_value_sound(*l, s);
            walk_value_sound(*r, s);
            if !wl.0 && !wr.0 {
                let f = sub_ov(wl.1, wr.1);
                if !f.0 {
                    assert(f.1 == wl.1 - wr.1);
                }
            }
        },
        Tree::Mul(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            walk_value_sound(*l, s);
            walk_value_sound(*r, s);
            if !wl.0 && !wr.0 {
                let f = mul_ov(wl.1, wr.1);
                if !f.0 {
                    assert(f.1 == wl.1 * wr.1);
                }
            }
        },
        Tree::Div(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            walk_value_sound(*l, s);
            walk_value_sound(*r, s);
        },
        Tree::Mod(l, r) => {
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            walk_value_sound(*l, s);
            walk_value_sound(*r, s);
        },
        Tree::Refuse => {},
    }
}

// THE OBLIGATION (both directions): the flag is clear exactly when
// every flagged node's exact value fits i64. No flag => every child
// flag-free => (IH + walk_value_sound) every temp exact => the builtin
// reports on the EXACT operation result. Conversely an exact value
// outside i64 sets the flag — at its own node if its children were in
// range, or at the child that flagged first.
proof fn flag_iff_exact_exits(t: Tree, s: Store)
    ensures !walk(t, s).0 == air(t, s),
    decreases t,
{
    match t {
        Tree::Num(n) => {},
        Tree::Var(x) => {},
        Tree::Neg(a) => {
            flag_iff_exact_exits(*a, s);
            let wa = walk(*a, s);
            let f = sub_ov(0int, wa.1);
            if wa.0 {
                // A child flag forces the parent flag (the |= chain),
                // and kills air by the IH.
                assert(!air(*a, s));
            } else {
                walk_value_sound(*a, s);
                assert(wa.1 == eval(*a, s));
            }
        },
        Tree::Add(l, r) => {
            flag_iff_exact_exits(*l, s);
            flag_iff_exact_exits(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            if wl.0 {
                assert(!air(*l, s));
            } else if wr.0 {
                assert(!air(*r, s));
            } else {
                walk_value_sound(*l, s);
                walk_value_sound(*r, s);
                let f = add_ov(wl.1, wr.1);
                assert(wl.1 == eval(*l, s));
                assert(wr.1 == eval(*r, s));
            }
        },
        Tree::Sub(l, r) => {
            flag_iff_exact_exits(*l, s);
            flag_iff_exact_exits(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            if wl.0 {
                assert(!air(*l, s));
            } else if wr.0 {
                assert(!air(*r, s));
            } else {
                walk_value_sound(*l, s);
                walk_value_sound(*r, s);
                let f = sub_ov(wl.1, wr.1);
                assert(wl.1 == eval(*l, s));
                assert(wr.1 == eval(*r, s));
            }
        },
        Tree::Mul(l, r) => {
            flag_iff_exact_exits(*l, s);
            flag_iff_exact_exits(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            if wl.0 {
                assert(!air(*l, s));
            } else if wr.0 {
                assert(!air(*r, s));
            } else {
                walk_value_sound(*l, s);
                walk_value_sound(*r, s);
                let f = mul_ov(wl.1, wr.1);
                assert(wl.1 == eval(*l, s));
                assert(wr.1 == eval(*r, s));
            }
        },
        Tree::Div(l, r) => {
            flag_iff_exact_exits(*l, s);
            flag_iff_exact_exits(*r, s);
        },
        Tree::Mod(l, r) => {
            flag_iff_exact_exits(*l, s);
            flag_iff_exact_exits(*r, s);
        },
        Tree::Refuse => {},
    }
}

// The value half of the "whenever needed" direction: a range-good tree
// walks flag-free and to its denotation (compose the equivalence with
// walk_value_sound — kept separate so each statement is proved from
// the one it needs).
proof fn air_value_sound(t: Tree, s: Store)
    ensures air(t, s) ==> !walk(t, s).0 && walk(t, s).1 == eval(t, s),
    decreases t,
{
    if air(t, s) {
        flag_iff_exact_exits(t, s);
        walk_value_sound(t, s);
    }
}

// Protocol B's `break` and protocol A's `|=` see the SAME flag: if any
// node's builtin flags, both protocols report it (protocol A because
// the flag OR-s into `ov`, protocol B because evaluation reaches the
// flagging node before breaking), and when nothing flags both compute
// identical temps through the same builtins. This is what makes one
// `walk` the model of both emissions.
proof fn break_matches_flag(t: Tree, s: Store)
    ensures walk_b(t, s).0 == walk(t, s).0,
        (!walk_b(t, s).0 ==> walk_b(t, s).1 == walk(t, s).1),
    decreases t,
{
    match t {
        Tree::Num(n) => {},
        Tree::Var(x) => {},
        Tree::Neg(a) => {
            break_matches_flag(*a, s);
            let wa = walk(*a, s);
            let wba = walk_b(*a, s);
            if wba.0 {
                assert(wa.0);
            } else {
                let f = sub_ov(0int, wa.1);
                let fb = sub_ov(0int, wba.1);
            }
        },
        Tree::Add(l, r) => {
            break_matches_flag(*l, s);
            break_matches_flag(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let wbl = walk_b(*l, s);
            if wbl.0 {
                assert(wl.0);
            } else {
                let wbr = walk_b(*r, s);
                if wbr.0 {
                    assert(wr.0);
                } else {
                    let f = add_ov(wl.1, wr.1);
                    let fb = add_ov(wbl.1, wbr.1);
                    assert(wbl.1 == wl.1);
                    assert(wbr.1 == wr.1);
                }
            }
        },
        Tree::Sub(l, r) => {
            break_matches_flag(*l, s);
            break_matches_flag(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let wbl = walk_b(*l, s);
            if wbl.0 {
                assert(wl.0);
            } else {
                let wbr = walk_b(*r, s);
                if wbr.0 {
                    assert(wr.0);
                } else {
                    let f = sub_ov(wl.1, wr.1);
                    let fb = sub_ov(wbl.1, wbr.1);
                    assert(wbl.1 == wl.1);
                    assert(wbr.1 == wr.1);
                }
            }
        },
        Tree::Mul(l, r) => {
            break_matches_flag(*l, s);
            break_matches_flag(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let wbl = walk_b(*l, s);
            if wbl.0 {
                assert(wl.0);
            } else {
                let wbr = walk_b(*r, s);
                if wbr.0 {
                    assert(wr.0);
                } else {
                    let f = mul_ov(wl.1, wr.1);
                    let fb = mul_ov(wbl.1, wbr.1);
                    assert(wbl.1 == wl.1);
                    assert(wbr.1 == wr.1);
                }
            }
        },
        Tree::Div(l, r) => {
            break_matches_flag(*l, s);
            break_matches_flag(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let wbl = walk_b(*l, s);
            if wbl.0 {
                assert(wl.0);
            } else {
                let wbr = walk_b(*r, s);
                if wbr.0 {
                    assert(wr.0);
                } else {
                    assert(wbl.1 == wl.1);
                    assert(wbr.1 == wr.1);
                }
            }
        },
        Tree::Mod(l, r) => {
            break_matches_flag(*l, s);
            break_matches_flag(*r, s);
            let wl = walk(*l, s);
            let wr = walk(*r, s);
            let wbl = walk_b(*l, s);
            if wbl.0 {
                assert(wl.0);
            } else {
                let wbr = walk_b(*r, s);
                if wbr.0 {
                    assert(wr.0);
                } else {
                    assert(wbl.1 == wl.1);
                    assert(wbr.1 == wr.1);
                }
            }
        },
        Tree::Refuse => {},
    }
}

// Protocol A: a committed `fast` value IS the denotation — the commit
// line sits under `if (!ov)`.
proof fn commit_a_sound(p: bool, t: Tree, s: Store) {
    if let Option::Some(v) = commit_a(p, t, s) {
        walk_value_sound(t, s);
        assert(v == eval(t, s));
    }
}

// The escape hatch, both halves: the GMP spill runs iff a participant
// was already promoted OR some exact value left i64 — "only when
// needed *and* whenever needed".
proof fn commit_a_spill_exactly_when_needed(p: bool, t: Tree, s: Store) {
    flag_iff_exact_exits(t, s);
    if commit_a(p, t, s) == Option::None {
        assert(p || !air(t, s));
    } else {
        assert(!p && air(t, s));
    }
}

// Protocol B: a committed store IS the denotation.
proof fn commit_b_sound(entry_ok: bool, t: Tree, s: Store) {
    if let Option::Some(v) = commit_b(entry_ok, t, s) {
        break_matches_flag(t, s);
        walk_value_sound(t, s);
        assert(v == eval(t, s));
    }
}

// The entry gates: no entry_ok, no store — the replay runs instead.
proof fn commit_b_entry_refusal(entry_ok: bool, t: Tree, s: Store) {
    if !entry_ok {
        assert(commit_b(entry_ok, t, s) == Option::None);
    }
}

// A refused shape forces the spill: the source's `None => ov = 1`
// fallback, pinned.
proof fn refusal_forces_spill(p: bool, s: Store) {
    assert(walk(Tree::Refuse, s).0);
    assert(commit_a(p, Tree::Refuse, s) == Option::None);
    assert(commit_b(true, Tree::Refuse, s) == Option::None);
}

// The raw `/` `%` arm's gate, pinned: reachable only with a POSITIVE
// LITERAL divisor — excluding both UBs of raw C division (zero divisor
// => SIGFPE; LLONG_MIN / -1) at the gate, not at a runtime check.
proof fn raw_divisor_gate(l: Tree, r: Tree) {
    if ok(Tree::Div(Box::new(l), Box::new(r))) {
        assert(is_pos_lit(r));
        if let Tree::Num(n) = r {
            assert(n > 0int);
        }
    }
    if ok(Tree::Mod(Box::new(l), Box::new(r))) {
        assert(is_pos_lit(r));
        if let Tree::Num(n) = r {
            assert(n > 0int);
        }
    }
}

// ── The negative controls ──────────────────────────────────────────────

// The concrete witness: MAX + 1. The exact result is 2^63, outside
// i64; the builtin flags and stores the wrapped -2^63. (Staged in the
// proofs below: leaves first, then the flagged node — Verus's
// auto-unfold needs the intermediate facts.)

// NEGATIVE CONTROL (guard removed): an UNGUARDED commit — `fast = t`
// with no `if (!ov)` — ships the wrapped value. The guard is what
// makes commit_a_sound true; this is the counterexample without it.
proof fn unguarded_commit_ships_wrap() {
    let s: Store = Map::empty();
    let a = Box::new(Tree::Num(9223372036854775807int));
    let b = Box::new(Tree::Num(1int));
    let t = Tree::Add(a, b);
    assert(eval(*a, s) == 9223372036854775807int);
    assert(eval(*b, s) == 1int);
    assert(walk(*a, s) == (false, 9223372036854775807int));
    assert(walk(*b, s) == (false, 1int));
    assert(walk_b(*a, s) == (false, 9223372036854775807int));
    assert(walk_b(*b, s) == (false, 1int));
    assert(eval(t, s) == 9223372036854775808int);
    assert(walk(t, s).0);                                 // the flag fires
    assert(walk(t, s).1 == 0int - 9223372036854775808int); // the wrapped bits
    assert(walk(t, s).1 != eval(t, s));                   // an unguarded value
    assert(commit_a(false, t, s) == Option::None);        // the guard holds it back
    assert(commit_b(true, t, s) == Option::None);         // and protocol B breaks
}

// NEGATIVE CONTROL (store before checks): protocol B's store line sits
// AFTER the builtin checks, so the overflowing statement stores
// nothing — modelled by commit_b's None. The same witness: had the
// store preceded the checks (or the break been a plain flag-and-
// continue), the shadow would read MIN for a variable whose shell
// value is 2^63.
proof fn protocol_b_skips_the_overflowing_store() {
    let s: Store = Map::empty();
    let a = Box::new(Tree::Num(9223372036854775807int));
    let b = Box::new(Tree::Num(1int));
    let t = Tree::Add(a, b);
    assert(walk(*a, s) == (false, 9223372036854775807int));
    assert(walk(*b, s) == (false, 1int));
    assert(walk_b(*a, s) == (false, 9223372036854775807int));
    assert(walk_b(*b, s) == (false, 1int));
    assert(walk_b(t, s).0);
    assert(commit_b(true, t, s) == Option::None);
    // And the continue form agrees the flag fired, so protocol A
    // discards too — one flag, two protocols, no wrapped store.
    assert(walk(t, s).0);
    assert(commit_a(false, t, s) == Option::None);
}

fn main() {}

} // verus!
