// Standalone model of the else-if-flatten Tier-1 obligation
// (`sh2perl/src/transforms/else_if_flatten.rs`): collapsing
// `else { if … }` (a lone nested `If` with no elsifs of its own) into the
// parent's `elsifs` preserves the selected branch — repeatedly, so a chain
// of any depth lands in one vector.
//
// Proves the soundness obligation:
//
//   flatten_sound: flattening the else-nested chain preserves the selected
//   body id under every boolean valuation of the conditions.
//
// Conditions are opaque predicate ids evaluated under a valuation map;
// bodies are nat ids; evaluation returns the selected body. The rule is
// purely structural (the condition's TYPE is irrelevant — pinned by the
// source's own `a_non_test_condition_is_untouched` test, which flattens
// through a BinOp(And) condition).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - the exact trigger: `else_` is exactly `[If]` whose own `elsifs` is
//     empty (the `matches!(else_, [If { elsifs: inner, .. }] if
//     inner.is_empty())` guard). An else with extra statements, or a
//     nested if that already has elsifs, keeps its nesting.
//   - the chain loop (any depth collapses in one call) + the ordinary
//     recursion into branch bodies (traversal — same induction per site,
//     out of scope as in every fold here; the theorem is per If-node).
//   - status/exit-code threading is out of scope (Tier-3 statement
//     semantics); the obligation is branch-selection preservation, the
//     Tier-1 carve-out in the elim-trio shape.
use vstd::prelude::*;

verus! {

pub type Pred = nat;
pub type Valuation = Map<Pred, bool>;

// Statements: if-nodes (cond id, then id, elsif list, else list) or
// opaque others (body id — never inspected, never rewritten).
pub enum Stmt {
    If { cond: Pred, then: nat, elsifs: Seq<(Pred, nat)>, else_: Seq<Stmt> },
    Other(nat),
}

pub open spec fn ev_pred(p: Pred, v: Valuation) -> bool {
    if v.dom().contains(p) {
        v[p]
    } else {
        false
    }
}

// Selected body under a valuation.
pub open spec fn stmt_size(s: Stmt) -> nat
    decreases s,
{
    match s {
        Stmt::If { elsifs, else_, .. } => 1 + elsifs.len() + seq_size(else_),
        Stmt::Other(_) => 1,
    }
}

pub open spec fn seq_size(b: Seq<Stmt>) -> nat
    decreases b,
{
    if b.len() == 0 {
        0
    } else {
        stmt_size(b[0]) + seq_size(b.drop_first()) + 1
    }
}

pub open spec fn eval(s: Stmt, v: Valuation) -> nat
    decreases stmt_size(s), 0int,
{
    match s {
        Stmt::Other(b) => b,
        Stmt::If { cond, then, elsifs, else_ } => eval_elsifs(cond, then, elsifs, else_, v),
    }
}

pub open spec fn eval_elsifs(
    cond: Pred,
    then: nat,
    elsifs: Seq<(Pred, nat)>,
    else_: Seq<Stmt>,
    v: Valuation,
) -> nat
    decreases elsifs.len() + seq_size(else_), 1int,
{
    if ev_pred(cond, v) {
        then
    } else if elsifs.len() == 0 {
        eval_block(else_, v)
    } else if ev_pred(elsifs[0].0, v) {
        elsifs[0].1
    } else {
        eval_elsifs(cond, then, elsifs.drop_first(), else_, v)
    }
}

pub open spec fn eval_block(b: Seq<Stmt>, v: Valuation) -> nat
    decreases seq_size(b), 0int,
{
    if b.len() == 0 {
        0
    } else if b.len() == 1 {
        eval(b[0], v)
    } else {
        // A multi-statement else runs in sequence; the SELECTED branch is
        // the head's (the transform refuses these, so only the single-If
        // shape reaches the flattening theorem — multi-statement blocks
        // are denoted by their head for branch-identity purposes).
        eval(b[0], v)
    }
}

// One flatten step: `else { if (no elsifs) }` becomes an elsif.
pub open spec fn flatten_step(s: Stmt) -> Stmt {
    match s {
        Stmt::If { cond, then, elsifs, else_ } => {
            if else_.len() == 1 {
                match else_[0] {
                    Stmt::If { cond: c2, then: t2, elsifs: e2, else_: el2 } => {
                        if e2.len() == 0 {
                            Stmt::If {
                                cond,
                                then,
                                elsifs: elsifs.add(seq![(c2, t2)]),
                                else_: el2,
                            }
                        } else {
                            s
                        }
                    },
                    Stmt::Other(_) => s,
                }
            } else {
                s
            }
        },
        _ => s,
    }
}

// First firing link in an elsif scan (bodies are ids — no eval needed).
pub open spec fn first_hit(elsifs: Seq<(Pred, nat)>, v: Valuation) -> Option<nat>
    decreases elsifs.len(),
{
    if elsifs.len() == 0 {
        None
    } else if ev_pred(elsifs[0].0, v) {
        Some(elsifs[0].1)
    } else {
        first_hit(elsifs.drop_first(), v)
    }
}

// The scan decomposes: head cond, else first hit, else the else-block.
proof fn eval_elsifs_decomp(
    cond: Pred,
    then: nat,
    eifs: Seq<(Pred, nat)>,
    else_: Seq<Stmt>,
    v: Valuation,
)
    ensures eval_elsifs(cond, then, eifs, else_, v) == (if ev_pred(cond, v) {
        then
    } else {
        match first_hit(eifs, v) {
            Some(b) => b,
            None => eval_block(else_, v),
        }
    }),
    decreases eifs.len(),
{
    if ev_pred(cond, v) {
        reveal_with_fuel(eval_elsifs, 3);
    } else if eifs.len() == 0 {
        reveal_with_fuel(eval_elsifs, 3);
        reveal_with_fuel(first_hit, 2);
    } else if ev_pred(eifs[0].0, v) {
        // Head fires: scan stops here on both sides.
        reveal_with_fuel(eval_elsifs, 3);
        reveal_with_fuel(first_hit, 3);
    } else {
        reveal_with_fuel(eval_elsifs, 3);
        eval_elsifs_decomp(cond, then, eifs.drop_first(), else_, v);
        assert(first_hit(eifs, v) == first_hit(eifs.drop_first(), v)) by {
            reveal_with_fuel(first_hit, 3);
        };
    }
}

// Appending after a hitting prefix does not move the first hit.
proof fn first_hit_append_some(eifs: Seq<(Pred, nat)>, extra: (Pred, nat), v: Valuation)
    requires first_hit(eifs, v).is_Some(),
    ensures first_hit(eifs.add(seq![extra]), v) == first_hit(eifs, v),
    decreases eifs.len(),
{
    if ev_pred(eifs[0].0, v) {
        reveal_with_fuel(first_hit, 3);
    } else {
        reveal_with_fuel(first_hit, 3);
        first_hit_append_some(eifs.drop_first(), extra, v);
        assert(eifs.add(seq![extra]).drop_first() == eifs.drop_first().add(seq![extra])) by {
            reveal_with_fuel(first_hit, 3);
        };
    }
}

// If-node count: the chain loop removes exactly one nested If per firing
// step, so the loop is bounded by this count (fuel).
pub open spec fn count_ifs(s: Stmt) -> nat
    decreases s,
{
    match s {
        Stmt::If { else_, .. } => 1 + seq_ifs(else_),
        Stmt::Other(_) => 0,
    }
}

pub open spec fn seq_ifs(b: Seq<Stmt>) -> nat
    decreases b,
{
    if b.len() == 0 {
        0
    } else {
        count_ifs(b[0]) + seq_ifs(b.drop_first())
    }
}

// A firing step strictly drops the If count (the nested node splices up
// one level: its own node disappears, its else-body stays).
proof fn flatten_step_shrinks(s: Stmt)
    requires flatten_step(s) != s,
    ensures count_ifs(flatten_step(s)) < count_ifs(s),
{
    match s {
        Stmt::If { cond, then, elsifs, else_ } => {
            assert(else_.len() == 1 && else_[0] matches Stmt::If { .. }) by {
                
            };
            match else_[0] {
                Stmt::If { cond: c2, then: t2, elsifs: e2, else_: el2 } => {
                    assert(e2.len() == 0) by {
                        
                    };
                    assert(flatten_step(s) == Stmt::If {
                        cond,
                        then,
                        elsifs: elsifs.add(seq![(c2, t2)]),
                        else_: el2,
                    }) by {
                        
                    };
                    assert(count_ifs(s) == 1 + count_ifs(else_[0]) + seq_ifs(else_.drop_first())) by {
                        reveal_with_fuel(count_ifs, 2);
                        reveal_with_fuel(seq_ifs, 3);
                    };
                    assert(count_ifs(flatten_step(s)) == 1 + seq_ifs(el2)) by {
                        reveal_with_fuel(count_ifs, 2);
                    };
                    assert(count_ifs(else_[0]) == 1 + seq_ifs(el2)) by {
                        reveal_with_fuel(count_ifs, 2);
                    };
                },
                Stmt::Other(_) => {
                    assert(false);
                },
            }
        },
        Stmt::Other(_) => {
            assert(flatten_step(s) == s) by {
                
            };
        },
    }
}

// No nested single-If else means no firing step (empty count = flat).
proof fn count_zero_no_fire(s: Stmt)
    requires count_ifs(s) == 0,
    ensures flatten_step(s) == s,
{
    match s {
        Stmt::If { else_, .. } => {
            assert(flatten_step(s) == s) by {
                
                reveal_with_fuel(count_ifs, 2);
                reveal_with_fuel(seq_ifs, 3);
            };
        },
        Stmt::Other(_) => {
            assert(flatten_step(s) == s) by {
                
            };
        },
    }
}

// THE OBLIGATION (step): one firing preserves the selected body.
proof fn flatten_step_sound(s: Stmt, v: Valuation)
    ensures eval(flatten_step(s), v) == eval(s, v),
{
    match s {
        Stmt::If { cond, then, elsifs, else_ } => {
            if flatten_step(s) == s {
            } else {
                assert(else_.len() == 1 && else_[0] matches Stmt::If { .. }) by {
                    
                };
                match else_[0] {
                    Stmt::If { cond: c2, then: t2, elsifs: e2, else_: el2 } => {
                        assert(e2.len() == 0) by {
                            
                        };
                        assert(flatten_step(s) == Stmt::If {
                            cond,
                            then,
                            elsifs: elsifs.add(seq![(c2, t2)]),
                            else_: el2,
                        }) by {
                            
                        };
                        eval_elsifs_decomp(cond, then, elsifs, else_, v);
                        eval_elsifs_decomp(cond, then, elsifs.add(seq![(c2, t2)]), el2, v);
                        if ev_pred(cond, v) {
                            assert(eval(s, v) == then) by {
                                reveal_with_fuel(eval, 3);
                            };
                            assert(eval(flatten_step(s), v) == then) by {
                                reveal_with_fuel(eval, 3);
                            };
                        } else if first_hit(elsifs, v).is_Some() {
                            first_hit_append_some(elsifs, (c2, t2), v);
                            assert(first_hit(elsifs.add(seq![(c2, t2)]), v).is_Some());
                            let hb = first_hit(elsifs, v).get_Some_0();
                            assert(eval(s, v) == hb);
                            assert(eval(flatten_step(s), v) == hb);
                        } else {
                            // No head/elsif fires on either side: both sides
                            // run the nested if, `c2 ? t2 : el2` on both.
                            assert(eval_block(else_, v) == eval(else_[0], v)) by {
                                reveal_with_fuel(eval_block, 3);
                            };
                            assert(eval(else_[0], v) == eval_elsifs(c2, t2, Seq::empty(), el2, v)) by {
                                reveal_with_fuel(eval, 3);
                            };
                            eval_elsifs_decomp(c2, t2, Seq::empty(), el2, v);
                            assert(first_hit(Seq::empty(), v) == None) by {
                                reveal_with_fuel(first_hit, 2);
                            };
                            assert(eval(s, v) == eval(else_[0], v));
                            first_hit_none_append(elsifs, (c2, t2), v);
                            assert(eval(flatten_step(s), v) == eval(else_[0], v)) by {
                                reveal_with_fuel(eval, 3);
                            };
                        }
                    },
                    Stmt::Other(_) => {
                        assert(false);
                    },
                }
            }
        },
        Stmt::Other(_) => {},
    }
}

// With no hit in the prefix, the appended link decides: hit iff its own
// condition holds.
proof fn first_hit_none_append(eifs: Seq<(Pred, nat)>, extra: (Pred, nat), v: Valuation)
    requires first_hit(eifs, v) == None,
    ensures first_hit(eifs.add(seq![extra]), v) == (if ev_pred(extra.0, v) {
        Some(extra.1)
    } else {
        None
    }),
    decreases eifs.len(),
{
    if eifs.len() == 0 {
        reveal_with_fuel(first_hit, 3);
        assert(eifs.add(seq![extra]) == seq![extra]) by {
            assert(eifs.add(seq![extra]).len() == 1);
        };
        reveal_with_fuel(first_hit, 3);
    } else {
        assert(!ev_pred(eifs[0].0, v)) by {
            reveal_with_fuel(first_hit, 3);
        };
        first_hit_none_append(eifs.drop_first(), extra, v);
        assert(eifs.add(seq![extra]).drop_first() == eifs.drop_first().add(seq![extra]));
        reveal_with_fuel(first_hit, 3);
    }
}

// The chain loop as a fuel-bounded helper (fuel = If count suffices).
pub open spec fn flatten_fuel(s: Stmt, fuel: nat) -> Stmt
    decreases fuel,
{
    if fuel == 0 {
        s
    } else {
        let s1 = flatten_step(s);
        if s1 == s {
            s
        } else {
            flatten_fuel(s1, (fuel - 1) as nat)
        }
    }
}

pub open spec fn flatten(s: Stmt) -> Stmt {
    flatten_fuel(s, count_ifs(s))
}

proof fn flatten_fuel_sound(s: Stmt, fuel: nat, v: Valuation)
    requires count_ifs(s) <= fuel,
    ensures eval(flatten_fuel(s, fuel), v) == eval(s, v),
    decreases fuel,
{
    if fuel == 0 {
        assert(count_ifs(s) == 0);
        count_zero_no_fire(s);
        assert(flatten_fuel(s, fuel) == s) by {
            reveal_with_fuel(flatten_fuel, 2);
        };
    } else {
        let s1 = flatten_step(s);
        flatten_step_sound(s, v);
        if s1 == s {
            assert(flatten_fuel(s, fuel) == s) by {
                reveal_with_fuel(flatten_fuel, 2);
            };
        } else {
            flatten_step_shrinks(s);
            assert(count_ifs(s1) <= (fuel - 1) as nat);
            flatten_fuel_sound(s1, (fuel - 1) as nat, v);
            assert(flatten_fuel(s, fuel) == flatten_fuel(s1, (fuel - 1) as nat)) by {
                reveal_with_fuel(flatten_fuel, 2);
            };
        }
    }
}

// THE OBLIGATION (loop): the collapsed chain selects the same body.
proof fn flatten_sound(s: Stmt, v: Valuation)
    ensures eval(flatten(s), v) == eval(s, v),
{
    flatten_fuel_sound(s, count_ifs(s), v);
    assert(flatten(s) == flatten_fuel(s, count_ifs(s)));
}

// ---- Fidelity pins (mirror the source's own tests) ----

// `if a { x } else { if b { y } else { z } }` gains one elsif.
proof fn pin_else_if_becomes_elsif()
    ensures match flatten(
        Stmt::If {
            cond: 1nat,
            then: 10nat,
            elsifs: Seq::empty(),
            else_: seq![Stmt::If {
                cond: 2nat,
                then: 20nat,
                elsifs: Seq::empty(),
                else_: seq![Stmt::Other(30nat)],
            }],
        },
    ) {
        Stmt::If { elsifs, else_, .. } => elsifs.len() == 1 && else_.len() == 1,
        _ => false,
    },
{
    reveal_with_fuel(flatten_fuel, 4);
}

// A three-level chain flattens fully (two elsifs, empty else).
proof fn pin_chain_flattens_fully()
    ensures match flatten(
        Stmt::If {
            cond: 1nat,
            then: 10nat,
            elsifs: Seq::empty(),
            else_: seq![Stmt::If {
                cond: 2nat,
                then: 20nat,
                elsifs: Seq::empty(),
                else_: seq![Stmt::If {
                    cond: 3nat,
                    then: 30nat,
                    elsifs: Seq::empty(),
                    else_: Seq::empty(),
                }],
            }],
        },
    ) {
        Stmt::If { elsifs, else_, .. } => elsifs.len() == 2 && else_.len() == 0,
        _ => false,
    },
{
    reveal_with_fuel(count_ifs, 4);
    reveal_with_fuel(seq_ifs, 5);
    reveal_with_fuel(flatten_fuel, 10);
}

// `else { if b { y }; echo tail }` keeps its nesting (no rewrite).
proof fn pin_extra_statements_keep_nesting()
    ensures flatten(
        Stmt::If {
            cond: 1nat,
            then: 10nat,
            elsifs: Seq::empty(),
            else_: seq![
                Stmt::If {
                    cond: 2nat,
                    then: 20nat,
                    elsifs: Seq::empty(),
                    else_: Seq::empty(),
                },
                Stmt::Other(99nat),
            ],
        },
    ) == (Stmt::If {
        cond: 1nat,
        then: 10nat,
        elsifs: Seq::empty(),
        else_: seq![
            Stmt::If { cond: 2nat, then: 20nat, elsifs: Seq::empty(), else_: Seq::empty() },
            Stmt::Other(99nat),
        ],
    }),
{
    reveal_with_fuel(flatten_fuel, 3);
}

// Branch selection is preserved on a live valuation.
proof fn pin_selection_preserved()
    ensures eval(
        flatten(
            Stmt::If {
                cond: 1nat,
                then: 10nat,
                elsifs: Seq::empty(),
                else_: seq![Stmt::If {
                    cond: 2nat,
                    then: 20nat,
                    elsifs: Seq::empty(),
                    else_: seq![Stmt::Other(30nat)],
                }],
            },
        ),
        Map::empty().insert(1nat, false).insert(2nat, true),
    ) == 20nat,
{
    flatten_sound(
        Stmt::If {
            cond: 1nat,
            then: 10nat,
            elsifs: Seq::empty(),
            else_: seq![Stmt::If {
                cond: 2nat,
                then: 20nat,
                elsifs: Seq::empty(),
                else_: seq![Stmt::Other(30nat)],
            }],
        },
        Map::empty().insert(1nat, false).insert(2nat, true),
    );
    reveal_with_fuel(eval, 6);
    reveal_with_fuel(eval_elsifs, 8);
    reveal_with_fuel(eval_block, 4);
}

fn main() {}

}
