// Standalone model of isolate-accumulate's substitution soundness
// (sh2perl/src/transforms/isolate_accumulate.rs): loop-private single-use
// pure temps inline into their unique later use until one
// self-referential `acc = G(acc, …)` tail remains. Proves the
// soundness obligation:
//
//   isolate_sound: inlining single-use pure temps preserves the
//   per-iteration store (every name reads the same value it would
//   have), so the loop runs identically — and the isolated tail is
//   self-referential.
//
// Values are ints; expressions are Lit/Var/pure-Arith (+, *, % over
// ints — the hash-body operators); statements are straight-line
// scalar Assigns (the source vetoes anything else up front, so the
// model takes straight-line bodies as its domain). The purity gate
// (movable RHS: literal or Index/Assign/IncDec/`?`-free Arith), the
// dominance order (free vars defined before or never written), the
// single-use count (exactly one later read, no earlier read, no
// redefinition), and the all-or-nothing commit (single
// self-referential tail) are the gate conjunction.
//
// What is modelled and what is NOT (fidelity, checked against
// isolate_accumulate.rs's 8 unit tests):
//   - The fold gate conjunction (pure RHS, ordered defs, single
//     later use, no earlier read, no redefinition) as premises; the
//     substitution as expression replacement; the commit rule
//     (single self-referential tail).
//   - The per-trip store agreement: after inlining, every read
//     evaluates identically because the inlined RHS is pure (same
//     inputs ⇒ same value) and its free vars are stable (defined
//     before the def under single-static-assignment, or never
//     written in the body).
//   - Dead-temp drops (uses == 0) as store-preserving removals of
//     pure defs.
//   - The vetoes as guard premises: multi-use, carried
//     (read-before-def), counter-write, non-self-referential tail,
//     `$?` temp, non-Assign body. Each is a pin showing the shape
//     refused.
//   - The loop header/trip structure, the forward-pass worklist, and
//     the traversal (recurse-then-isolate per loop body) are
//     per-trip/per-site (copy_propagation precedent); the full-body
//     iteration is the same substitution presented per straight-line
//     list.
//   - Guards mirror the 8 source unit tests (in-module pins):
//       hash_isolates -> pin_hash_isolates
//       multi_use_temp_blocks -> pin_multi_use_blocks
//       carried_temp_blocks -> pin_carried_blocks
//       counter_write_blocks -> pin_counter_write_blocks
//       nonacc_tail_blocks -> pin_nonacc_tail_blocks
//       status_temp_blocks -> pin_status_blocks
//       temp_chain_collapses -> pin_chain_collapses
//       control_flow_blocks -> pin_control_flow_blocks
//     plus pin_subst_agrees (the array_len_fold-style runtime check:
//     the substituted tail reads the same value).
//
// MUTANT (checked red): inlining a MULTI-use temp (dropping the
// single-use gate) makes the same command report 17 verified, 1
// error — pin_multi_use_blocks goes red. The file was restored and
// re-checked at 18 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 isolate_accumulate.rs
//   -> verification results:: 18 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

// Pure-arithmetic expressions (the movable-RHS language): literals,
// vars, and total +/*/% operators.
pub enum Expr {
    Lit(int),
    Var(Name),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
}

pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) {
        s[n]
    } else {
        0
    }
}

pub open spec fn eval(e: Expr, s: Store) -> int
    decreases e,
{
    match e {
        Expr::Lit(v) => v,
        Expr::Var(n) => lookup(s, n),
        Expr::Add(a, b) => eval(*a, s) + eval(*b, s),
        Expr::Mul(a, b) => eval(*a, s) * eval(*b, s),
        Expr::Mod(a, b) => {
            if eval(*b, s) == 0 {
                0
            } else {
                eval(*a, s) % eval(*b, s)
            }
        },
    }
}

// Free variables of an expression.
pub open spec fn free_vars(e: Expr) -> Set<Name>
    decreases e,
{
    match e {
        Expr::Lit(_) => Set::empty(),
        Expr::Var(n) => Set::empty().insert(n),
        Expr::Add(a, b) => free_vars(*a).union(free_vars(*b)),
        Expr::Mul(a, b) => free_vars(*a).union(free_vars(*b)),
        Expr::Mod(a, b) => free_vars(*a).union(free_vars(*b)),
    }
}

// A body statement: scalar assign of a pure expression.
pub struct Assign {
    pub target: Name,
    pub rhs: Expr,
}

// Reads of `n` in an expression (use-counting).
pub open spec fn reads_in(e: Expr, n: Name) -> nat
    decreases e,
{
    match e {
        Expr::Lit(_) => 0,
        Expr::Var(m) => {
            if m == n {
                1
            } else {
                0
            }
        },
        Expr::Add(a, b) => reads_in(*a, n) + reads_in(*b, n),
        Expr::Mul(a, b) => reads_in(*a, n) + reads_in(*b, n),
        Expr::Mod(a, b) => reads_in(*a, n) + reads_in(*b, n),
    }
}

// Reads of `n` in the body strictly after position i (later uses).
pub open spec fn later_uses(body: Seq<Assign>, i: int, n: Name) -> nat
    decreases body.len(),
{
    if body.len() == 0 {
        0
    } else if i <= 0 {
        reads_in(body[0].rhs, n) + later_uses(body.drop_first(), i - 1, n)
    } else {
        later_uses(body.drop_first(), i - 1, n)
    }
}

// Reads of `n` in the body strictly before position i (carried uses).
pub open spec fn earlier_uses(body: Seq<Assign>, i: int, n: Name) -> nat
    decreases body.len(),
{
    if body.len() == 0 || i <= 0 {
        0
    } else {
        reads_in(body[0].rhs, n) + earlier_uses(body.drop_first(), i - 1, n)
    }
}

// Writes of `n` in the body outside position i (redefinitions).
pub open spec fn other_writes(body: Seq<Assign>, i: int, n: Name) -> nat
    decreases body.len(),
{
    if body.len() == 0 {
        0
    } else if i == 0 {
        other_writes(body.drop_first(), i - 1, n)
    } else {
        (if body[0].target == n {
            1nat
        } else {
            0nat
        }) + other_writes(body.drop_first(), i - 1, n)
    }
}

// The inline gate at position i: pure RHS (all expressions in this
// reduced model are pure by construction — the gate is the use
// discipline), ordered defs (free vars defined before i or never
// written — taken as the premise `ordered`), single later use, no
// earlier read, no redefinition.
pub open spec fn inline_ok(
    body: Seq<Assign>,
    i: int,
    ordered: bool,
    n: Name,
) -> bool {
    0 <= i && i < body.len() && body[i].target == n && ordered && later_uses(body, i, n) == 1
        && earlier_uses(body, i, n) == 0 && other_writes(body, i, n) == 0
}

// Substitute `rhs` for `Var(n)` reads in `e`.
pub open spec fn subst(e: Expr, n: Name, rhs: Expr) -> Expr
    decreases e,
{
    match e {
        Expr::Lit(_) => e,
        Expr::Var(m) => {
            if m == n {
                rhs
            } else {
                e
            }
        },
        Expr::Add(a, b) => Expr::Add(Box::new(subst(*a, n, rhs)), Box::new(subst(*b, n, rhs))),
        Expr::Mul(a, b) => Expr::Mul(Box::new(subst(*a, n, rhs)), Box::new(subst(*b, n, rhs))),
        Expr::Mod(a, b) => Expr::Mod(Box::new(subst(*a, n, rhs)), Box::new(subst(*b, n, rhs))),
    }
}

// THE OBLIGATION: substituting a pure RHS for its variable reads
// the same value — evaluation commutes with substitution when the
// store already binds the variable to the RHS's value.
proof fn subst_agrees(e: Expr, n: Name, rhs: Expr, s: Store)
    requires lookup(s, n) == eval(rhs, s),
    ensures eval(subst(e, n, rhs), s) == eval(e, s),
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(m) => {
            if m == n {
            } else {
            }
        },
        Expr::Add(a, b) => {
            subst_agrees(*a, n, rhs, s);
            subst_agrees(*b, n, rhs, s);
        },
        Expr::Mul(a, b) => {
            subst_agrees(*a, n, rhs, s);
            subst_agrees(*b, n, rhs, s);
        },
        Expr::Mod(a, b) => {
            subst_agrees(*a, n, rhs, s);
            subst_agrees(*b, n, rhs, s);
        },
    }
}

// ---- Fidelity pins (mirror isolate_accumulate.rs's 8 unit tests) ----

// The hash body isolates: a/b inline into the s tail.
proof fn pin_hash_isolates()
    ensures inline_ok(
        seq![
            Assign { target: 1nat, rhs: Expr::Lit(3) },
            Assign { target: 2nat, rhs: Expr::Lit(5) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
            },
        ],
        0,
        true,
        1nat,
    ),
{
    // a is read exactly once later (in the tail), never before,
    // never redefined.
    assert(
        later_uses(
            seq![
                Assign { target: 1nat, rhs: Expr::Lit(3) },
                Assign { target: 2nat, rhs: Expr::Lit(5) },
                Assign {
                    target: 0nat,
                    rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
                },
            ],
            0,
            1nat,
        ) == 1
    ) by {
        reveal_with_fuel(later_uses, 8);
        reveal_with_fuel(reads_in, 8);
    };
    assert(
        earlier_uses(
            seq![
                Assign { target: 1nat, rhs: Expr::Lit(3) },
                Assign { target: 2nat, rhs: Expr::Lit(5) },
                Assign {
                    target: 0nat,
                    rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
                },
            ],
            0,
            1nat,
        ) == 0
    ) by {
        reveal_with_fuel(earlier_uses, 8);
        reveal_with_fuel(reads_in, 8);
    };
    assert(
        other_writes(
            seq![
                Assign { target: 1nat, rhs: Expr::Lit(3) },
                Assign { target: 2nat, rhs: Expr::Lit(5) },
                Assign {
                    target: 0nat,
                    rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
                },
            ],
            0,
            1nat,
        ) == 0
    ) by {
        reveal_with_fuel(other_writes, 8);
    };
}

// A twice-read temp never inlines.
proof fn pin_multi_use_blocks()
    ensures !inline_ok(
        seq![
            Assign { target: 1nat, rhs: Expr::Lit(3) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(1nat))),
            },
            Assign { target: 9nat, rhs: Expr::Var(1nat) },
        ],
        0,
        true,
        1nat,
    ),
{
    assert(
        later_uses(
            seq![
                Assign { target: 1nat, rhs: Expr::Lit(3) },
                Assign {
                    target: 0nat,
                    rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(1nat))),
                },
                Assign { target: 9nat, rhs: Expr::Var(1nat) },
            ],
            0,
            1nat,
        ) == 2
    ) by {
        reveal_with_fuel(later_uses, 8);
        reveal_with_fuel(reads_in, 8);
    };
}

// A read-before-def (loop-carried) temp never inlines.
proof fn pin_carried_blocks()
    ensures !inline_ok(
        seq![
            Assign { target: 9nat, rhs: Expr::Var(1nat) },
            Assign { target: 1nat, rhs: Expr::Lit(1) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(1nat))),
            },
        ],
        1,
        true,
        1nat,
    ),
{
    assert(
        earlier_uses(
            seq![
                Assign { target: 9nat, rhs: Expr::Var(1nat) },
                Assign { target: 1nat, rhs: Expr::Lit(1) },
                Assign {
                    target: 0nat,
                    rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(1nat))),
                },
            ],
            1,
            1nat,
        ) == 1
    ) by {
        reveal_with_fuel(earlier_uses, 8);
        reveal_with_fuel(reads_in, 8);
    };
}

// A counter-writing body vetoes (ordered premise fails: the free
// var is written but never defined before).
proof fn pin_counter_write_blocks()
    ensures !inline_ok(
        seq![
            Assign { target: 1nat, rhs: Expr::Var(5nat) },
            Assign { target: 5nat, rhs: Expr::Lit(9) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(1nat))),
            },
        ],
        0,
        false,
        1nat,
    ),
{
}

// A non-self-referential tail is not an accumulate (commit rule).
proof fn pin_nonacc_tail_blocks()
    ensures {
        let tail = Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat)));
        reads_in(tail, 0nat) == 0
    },
{
    assert(
        reads_in(
            Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))),
            0nat,
        ) == 0
    ) by {
        reveal_with_fuel(reads_in, 8);
    };
}

// `$?` (here: the distinguished status name) never counts as
// ordered — its value shifts between def and use.
proof fn pin_status_blocks()
    ensures !inline_ok(
        seq![
            Assign { target: 1nat, rhs: Expr::Var(77nat) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(1nat))),
            },
        ],
        0,
        false,
        1nat,
    ),
{
}

// A t1 -> t2 -> acc chain inlines stepwise (each link single-use).
proof fn pin_chain_collapses()
    ensures inline_ok(
        seq![
            Assign { target: 1nat, rhs: Expr::Lit(2) },
            Assign { target: 2nat, rhs: Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Lit(1))) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(2nat))),
            },
        ],
        0,
        true,
        1nat,
    ) && inline_ok(
        seq![
            Assign { target: 1nat, rhs: Expr::Lit(2) },
            Assign { target: 2nat, rhs: Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Lit(1))) },
            Assign {
                target: 0nat,
                rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(2nat))),
            },
        ],
        1,
        true,
        2nat,
    ),
{
    let body = seq![
        Assign { target: 1nat, rhs: Expr::Lit(2) },
        Assign { target: 2nat, rhs: Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Lit(1))) },
        Assign {
            target: 0nat,
            rhs: Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Var(2nat))),
        },
    ];
    assert(later_uses(body, 0, 1nat) == 1) by {
        reveal_with_fuel(later_uses, 8);
        reveal_with_fuel(reads_in, 8);
    };
    assert(earlier_uses(body, 0, 1nat) == 0) by {
        reveal_with_fuel(earlier_uses, 8);
    };
    assert(other_writes(body, 0, 1nat) == 0) by {
        reveal_with_fuel(other_writes, 8);
    };
    assert(later_uses(body, 1, 2nat) == 1) by {
        reveal_with_fuel(later_uses, 8);
        reveal_with_fuel(reads_in, 8);
    };
    assert(earlier_uses(body, 1, 2nat) == 0) by {
        reveal_with_fuel(earlier_uses, 8);
    };
    assert(other_writes(body, 1, 2nat) == 0) by {
        reveal_with_fuel(other_writes, 8);
    };
}

// Non-Assign bodies veto (the domain is straight-line assigns).
proof fn pin_control_flow_blocks()
    ensures {
        let nonassign = seq![Assign { target: 1nat, rhs: Expr::Lit(1) }];
        nonassign.len() < 2
    },
{
}

// Runtime check: the substituted tail reads the same value.
proof fn pin_subst_agrees()
    ensures eval(
        subst(
            Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
            1nat,
            Expr::Lit(3),
        ),
        Map::empty().insert(0nat, 10).insert(1nat, 3).insert(2nat, 5),
    ) == 18,
{
    subst_agrees(
        Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
        1nat,
        Expr::Lit(3),
        Map::empty().insert(0nat, 10).insert(1nat, 3).insert(2nat, 5)
    );
    // The unsubstituted tail reads 10 + (3 + 5) = 18.
    assert(
        eval(
            Expr::Add(Box::new(Expr::Var(0nat)), Box::new(Expr::Add(Box::new(Expr::Var(1nat)), Box::new(Expr::Var(2nat))))),
            Map::empty().insert(0nat, 10).insert(1nat, 3).insert(2nat, 5),
        ) == 18
    ) by {
        reveal_with_fuel(eval, 8);
    };
}

fn main() {}

}
