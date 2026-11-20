// Standalone model of `fbdev_gpu_elim`
// (sh2perl/src/transforms/fbdev_gpu_elim.rs).
//
// After `fbdev-gpu` lifts writes to `/dev/fb0` into `GpuOp` nodes, the pass
// drops a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its predecessor:
// same device, verb, slot and payload with nothing in between. The claim is
// that no observer can see the intermediate device state, so dropping the
// second write leaves every execution identical — and that a duplicate with
// an intervening statement must be KEPT, because the statement between the
// two writes could read the device back.
//
// ## What is proved here (device-state law, not the code)
//
//   elim_preserves_visible: for every statement list, the states the program
//   OBSERVES (every `Read` plus the final state) are identical before and
//   after the elimination. That is the rewrite law: the pass is allowed to
//   drop writes only because nothing observes them.
//
//   non_consecutive_duplicate_is_load_bearing: THE NAIVE ALTERNATIVE GOES RED.
//   A "drop every repeated write" rule is NOT sound: `Op(1); Op(2); Op(1)`
//   ends with state 1 in bash, but dropping the last write leaves state 2 —
//   any later read sees the wrong value. The pass's consecutiveness guard is
//   exactly what makes `elim_preserves_visible` true, and the model proves it
//   is load-bearing rather than defensive.
//
//   The payload/verb/device distinction is the `status` of the comparison:
//   `Op(id)` uses one id standing for the whole (device, verb, slot, payload)
//   tuple, so "different payload", "different verb", "different slot" and
//   "different device" all collapse to "different id" — which is what the
//   source's `is_duplicate` compares field by field.
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - OBSERVER DOMAIN: the model's observers are `Read` statements plus the
//   program's final state. A program that observes the device through a node
//   the model does not model (a nested process, a signal handler reading
//   `/dev/fb0` directly) would be an observer between two writes — which is
//   exactly why the pass only removes CONSECUTIVE duplicates: any statement
//   between them is treated as an observer. Stated, not proved.
// - NESTING: the pass applies `elim_vec` to every nested body (Block,
//   Function, While, For, ForInit, DoWhile, Subshell, Background, If
//   then/elsifs/else, Pipeline stages). The model is ONE flat list; the
//   nested case is the same theorem instantiated on the smaller list, so the
//   recursion is stated, not re-derived. (This is the "one-level recursor
//   misses three-deep" bug class the source's comment records.)
// - REACHABILITY / REGISTRATION: that `fbdev-gpu-elim` runs after
//   `fbdev-gpu` and `fbdev-gpu-batch` and that the pipeline can produce
//   `Ext(GpuOp)` nodes at all (the C-only target gate on
//   `fbdev-gpu-calls`) is the registration table's business, not the model's.
// - CORPUS DOMAIN: the pass fires 0 times on the 552-example corpus; that is
//   a coverage fact, not a correctness one, and the law below is universally
//   quantified over statement lists.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 fbdev_gpu_elim.rs
//   -> verification results:: 21 verified, 0 errors
//
//   Mutants (checked, measured on copies under the same flags):
//     - the naive rule (drop every repeated write, not only consecutive):
//       13 verified, 8 errors.
//     - dedupe ignoring the op identity (drop any consecutive Op):
//       19 verified, 2 errors.
//     - identity (the pass removes nothing): 17 verified, 4 errors.
//   Restored and re-checked at 21 verified, 0 errors.
use vstd::prelude::*;

verus! {

// A statement in one flat list. `Op(id)` writes the device (id stands for
// device+verb+slot+payload); `Read` observes the current device state;
// `Other` is any statement that does not touch the device.
pub enum St {
    Op(int),
    Read,
    Other,
}

pub enum L {
    Nil,
    Cons(Box<St>, Box<L>),
}

// ---- observations ----

// The states observed by the program's `Read`s, in order.
pub open spec fn vis(l: L, st: int, acc: Seq<int>) -> Seq<int>
    decreases l,
{
    match l {
        L::Nil => acc,
        L::Cons(s, rest) => match *s {
            St::Read => vis(*rest, st, acc.push(st)),
            St::Op(v) => vis(*rest, v, acc),
            St::Other => vis(*rest, st, acc),
        },
    }
}

// The device state at the end.
pub open spec fn end_state(l: L, st: int) -> int
    decreases l,
{
    match l {
        L::Nil => st,
        L::Cons(s, rest) => match *s {
            St::Op(v) => end_state(*rest, v),
            _ => end_state(*rest, st),
        },
    }
}

// What the program observes: every read, then the final state.
pub open spec fn visible(l: L) -> Seq<int> {
    vis(l, 0, Seq::empty()).push(end_state(l, 0))
}

// ---- the pass ----

// Keep `Op(v)` and drop the NEXT statement when it is `Op(v)` again (the
// consecutive exact duplicate). Nothing else is dropped.
pub open spec fn push_op(v: int, rest: L) -> L {
    match rest {
        L::Cons(s, r) => match *s {
            St::Op(w) => if w == v { rest } else { L::Cons(Box::new(St::Op(v)), Box::new(rest)) },
            _ => L::Cons(Box::new(St::Op(v)), Box::new(rest)),
        },
        L::Nil => L::Cons(Box::new(St::Op(v)), Box::new(L::Nil)),
    }
}

pub open spec fn elim(l: L) -> L
    decreases l,
{
    match l {
        L::Nil => L::Nil,
        L::Cons(s, rest) => match *s {
            St::Op(v) => push_op(v, elim(*rest)),
            St::Read => L::Cons(Box::new(St::Read), Box::new(elim(*rest))),
            St::Other => L::Cons(Box::new(St::Other), Box::new(elim(*rest))),
        },
    }
}

// ---- the law ----

// Two consecutive writes of the same value are one write, observationally:
// nothing runs between them, so no `Read` and no final state can see the
// intermediate value.
proof fn op_twice_is_invisible(v: int, r: L, st: int, acc: Seq<int>)
    ensures vis(L::Cons(Box::new(St::Op(v)), Box::new(L::Cons(Box::new(St::Op(v)), Box::new(r)))), st, acc)
            == vis(L::Cons(Box::new(St::Op(v)), Box::new(r)), st, acc),
        end_state(L::Cons(Box::new(St::Op(v)), Box::new(L::Cons(Box::new(St::Op(v)), Box::new(r)))), st)
            == end_state(L::Cons(Box::new(St::Op(v)), Box::new(r)), st),
{
    reveal_with_fuel(vis, 4);
    reveal_with_fuel(end_state, 4);
    assert(vis(L::Cons(Box::new(St::Op(v)), Box::new(L::Cons(Box::new(St::Op(v)), Box::new(r)))), st, acc)
        == vis(L::Cons(Box::new(St::Op(v)), Box::new(r)), v, acc));
    assert(vis(L::Cons(Box::new(St::Op(v)), Box::new(r)), v, acc) == vis(r, v, acc));
    assert(vis(L::Cons(Box::new(St::Op(v)), Box::new(r)), st, acc) == vis(r, v, acc));
    assert(end_state(L::Cons(Box::new(St::Op(v)), Box::new(L::Cons(Box::new(St::Op(v)), Box::new(r)))), st)
        == end_state(r, v));
    assert(end_state(L::Cons(Box::new(St::Op(v)), Box::new(r)), st) == end_state(r, v));
}

// `push_op` is the pass keeping a write and dropping its exact successor: it
// observes exactly what keeping both observes.
proof fn push_op_vis(v: int, r: L, st: int, acc: Seq<int>)
    ensures vis(push_op(v, r), st, acc) == vis(L::Cons(Box::new(St::Op(v)), Box::new(r)), st, acc),
        end_state(push_op(v, r), st) == end_state(L::Cons(Box::new(St::Op(v)), Box::new(r)), st),
{
    reveal_with_fuel(vis, 2);
    reveal_with_fuel(end_state, 2);
    match r {
        L::Nil => {}
        L::Cons(s, rr) => match *s {
            St::Op(w) => {
                if w == v {
                    // the successor is the same write: the pair is invisible,
                    // so the survivor alone observes the same states
                    op_twice_is_invisible(v, *rr, st, acc);
                    assert(push_op(v, r) == r);
                } else {
                    assert(push_op(v, r) == L::Cons(Box::new(St::Op(v)), Box::new(r)));
                }
            }
            _ => {
                assert(push_op(v, r) == L::Cons(Box::new(St::Op(v)), Box::new(r)));
            }
        },
    }
}

// The law at an arbitrary starting state and accumulator — the general form
// the induction needs.
proof fn elim_preserves_vis(l: L, st: int, acc: Seq<int>)
    ensures vis(elim(l), st, acc) == vis(l, st, acc),
        end_state(elim(l), st) == end_state(l, st),
    decreases l,
{
    match l {
        L::Nil => {}
        L::Cons(s, rest) => {
            elim_preserves_vis(*rest, st, acc);
            match *s {
                St::Op(v) => {
                    elim_preserves_vis(*rest, v, acc);
                    push_op_vis(v, elim(*rest), st, acc);
                    reveal_with_fuel(vis, 2);
                    assert(vis(L::Cons(Box::new(St::Op(v)), Box::new(elim(*rest))), st, acc)
                        == vis(elim(*rest), v, acc));
                    assert(vis(L::Cons(Box::new(St::Op(v)), Box::new(*rest)), st, acc)
                        == vis(*rest, v, acc));
                }
                St::Read => {
                    elim_preserves_vis(*rest, st, acc.push(st));
                    reveal_with_fuel(vis, 2);
                    assert(vis(L::Cons(Box::new(St::Read), Box::new(elim(*rest))), st, acc)
                        == vis(elim(*rest), st, acc.push(st)));
                    assert(vis(L::Cons(Box::new(St::Read), Box::new(*rest)), st, acc)
                        == vis(*rest, st, acc.push(st)));
                }
                St::Other => {
                    reveal_with_fuel(vis, 2);
                    assert(vis(L::Cons(Box::new(St::Other), Box::new(elim(*rest))), st, acc)
                        == vis(elim(*rest), st, acc));
                    assert(vis(L::Cons(Box::new(St::Other), Box::new(*rest)), st, acc)
                        == vis(*rest, st, acc));
                }
            }
        }
    }
}

// THE REWRITE LAW: the elimination does not change anything the program can
// observe.
proof fn elim_preserves_visible(l: L)
    ensures visible(elim(l)) == visible(l),
{
    elim_preserves_vis(l, 0, Seq::empty());
}

// ---- the naive alternative goes red ----

// "Drop every write whose value was already written" — the rule a reader of
// the module docs might expect. It is UNSOUND: the re-assert after another
// write is observable.
// Membership over the modelled `seen` list, defined here so it evaluates
// (vstd's `Seq::contains` is not reducible by the SMT solver).
pub open spec fn mem(v: int, seen: Seq<int>) -> bool
    decreases seen.len(),
{
    if seen.len() == 0 {
        false
    } else if seen[0] == v {
        true
    } else {
        mem(v, seen.subrange(1, seen.len() as int))
    }
}

pub open spec fn seen_before(v: int, seen: Seq<int>) -> bool {
    mem(v, seen)
}

// The naive pass: keep a write only if its value has not been written yet.
pub open spec fn naive(l: L, seen: Seq<int>) -> L
    decreases l,
{
    match l {
        L::Nil => L::Nil,
        L::Cons(s, rest) => match *s {
            St::Op(v) => {
                if seen_before(v, seen) {
                    naive(*rest, seen)
                } else {
                    L::Cons(Box::new(St::Op(v)), Box::new(naive(*rest, seen.push(v))))
                }
            }
            St::Read => L::Cons(Box::new(St::Read), Box::new(naive(*rest, seen))),
            St::Other => L::Cons(Box::new(St::Other), Box::new(naive(*rest, seen))),
        },
    }
}

// `Op(1); Op(2); Op(1); Read` — bash ends in state 1 (the last write is the
// deliberate re-assert); the naive rule leaves state 2, and the trailing read
// sees the difference.
pub open spec fn three_writes() -> L {
    L::Cons(
        Box::new(St::Op(1)),
        Box::new(L::Cons(
            Box::new(St::Op(2)),
            Box::new(L::Cons(
                Box::new(St::Op(1)),
                Box::new(L::Cons(Box::new(St::Read), Box::new(L::Nil))),
            )),
        )),
    )
}

pub open spec fn non_consecutive_witness() -> bool {
    // the program's read sees 1 — the pass keeps all three writes
    vis(three_writes(), 0, Seq::empty()) == seq![1]
        // the naive "drop every repeated write" rule drops the re-assert, so
        // the same read sees 2
        && vis(naive(three_writes(), Seq::empty()), 0, Seq::empty()) == seq![2]
}

proof fn non_consecutive_duplicate_is_load_bearing()
    ensures non_consecutive_witness(),
{
    let p = three_writes();
    reveal_with_fuel(vis, 8);
    reveal_with_fuel(naive, 12);
    reveal_with_fuel(mem, 8);
    assert(vis(p, 0, Seq::empty()) == seq![1]);
    assert(vis(naive(p, Seq::empty()), 0, Seq::empty()) == seq![2]);
}

// The same writes with the duplicate made CONSECUTIVE is invisible: this is
// the pair the pass does remove, and dropping it changes nothing.
pub open spec fn dup_pair() -> L {
    L::Cons(
        Box::new(St::Op(1)),
        Box::new(L::Cons(
            Box::new(St::Op(1)),
            Box::new(L::Cons(Box::new(St::Read), Box::new(L::Nil))),
        )),
    )
}

pub open spec fn consecutive_witness() -> bool {
    visible(elim(dup_pair())) == visible(dup_pair())
        && vis(dup_pair(), 0, Seq::empty()) == seq![1]
        && elim(dup_pair()) == L::Cons(
            Box::new(St::Op(1)),
            Box::new(L::Cons(Box::new(St::Read), Box::new(L::Nil))),
        )
}

proof fn consecutive_duplicate_is_invisible()
    ensures consecutive_witness(),
{
    let p = dup_pair();
    reveal_with_fuel(vis, 8);
    reveal_with_fuel(end_state, 8);
    reveal_with_fuel(elim, 8);
    assert(vis(p, 0, Seq::empty()) == seq![1]);
    reveal_with_fuel(elim, 12);
    assert(elim(p) == L::Cons(
        Box::new(St::Op(1)),
        Box::new(L::Cons(Box::new(St::Read), Box::new(L::Nil))),
    ));
    assert(visible(elim(p)) == visible(p));
}

// ---- pins: the source's tests ----

// removes_exact_duplicates: the pair collapses to one statement.
proof fn pin_removes_exact_duplicates()
    ensures elim(L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(Box::new(St::Op(7)), Box::new(L::Nil))),
    )) == L::Cons(Box::new(St::Op(7)), Box::new(L::Nil)),
{
    reveal_with_fuel(elim, 4);
}

// keeps_different_payloads / keeps_different_verbs / keeps_different_devices
// / duplicate_with_different_slot_is_kept: different ids are not duplicates.
proof fn pin_keeps_different_ids()
    ensures elim(L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(Box::new(St::Op(8)), Box::new(L::Nil))),
    )) == L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(Box::new(St::Op(8)), Box::new(L::Nil))),
    ),
{
    reveal_with_fuel(elim, 4);
}

// removes_a_run_of_three: a run collapses to one statement.
proof fn pin_removes_a_run_of_three()
    ensures elim(L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(
            Box::new(St::Op(7)),
            Box::new(L::Cons(Box::new(St::Op(7)), Box::new(L::Nil))),
        )),
    )) == L::Cons(Box::new(St::Op(7)), Box::new(L::Nil)),
{
    reveal_with_fuel(elim, 6);
}

// keeps_non_consecutive_duplicates / intervening_statement_breaks_the_pair:
// the separated pair is kept (and the statement between survives).
proof fn pin_keeps_non_consecutive_duplicates()
    ensures elim(L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(
            Box::new(St::Read),
            Box::new(L::Cons(Box::new(St::Op(7)), Box::new(L::Nil))),
        )),
    )) == L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(
            Box::new(St::Read),
            Box::new(L::Cons(Box::new(St::Op(7)), Box::new(L::Nil))),
        )),
    ),
{
    reveal_with_fuel(elim, 6);
}

// non_gpu_ops_are_untouched: `Other` statements are neither dropped nor
// compared against the writes.
proof fn pin_non_gpu_ops_untouched()
    ensures elim(L::Cons(
        Box::new(St::Other),
        Box::new(L::Cons(Box::new(St::Other), Box::new(L::Nil))),
    )) == L::Cons(
        Box::new(St::Other),
        Box::new(L::Cons(Box::new(St::Other), Box::new(L::Nil))),
    ),
{
    reveal_with_fuel(elim, 4);
}

// handles_empty_list.
proof fn pin_empty_list()
    ensures elim(L::Nil) == L::Nil,
{
}

// is_idempotent: after the pass there are no consecutive duplicates left, so
// a second run changes nothing.
pub open spec fn dedup_free(l: L) -> bool
    decreases l,
{
    match l {
        L::Nil => true,
        L::Cons(s, rest) => {
            let rest_ok = dedup_free(*rest);
            match *s {
                St::Op(v) => rest_ok && (match *rest {
                    L::Cons(s2, _) => match *s2 {
                        St::Op(w) => w != v,
                        _ => true,
                    },
                    L::Nil => true,
                }),
                _ => rest_ok,
            }
        }
    }
}

proof fn elim_leaves_no_duplicates(l: L)
    ensures dedup_free(elim(l)),
    decreases l,
{
    match l {
        L::Nil => {}
        L::Cons(s, rest) => {
            elim_leaves_no_duplicates(*rest);
            match *s {
                St::Op(v) => {
                    match elim(*rest) {
                        L::Cons(s2, r2) => match *s2 {
                            St::Op(w) => {
                                if w == v {
                                    // the run collapses: the survivor is the
                                    // same write, and the dropped one cannot
                                    // start a new pair with what follows it
                                }
                            }
                            _ => {}
                        },
                        L::Nil => {}
                    }
                }
                _ => {}
            }
        }
    }
}

proof fn pin_is_idempotent()
    ensures ({
        let p = L::Cons(
            Box::new(St::Op(7)),
            Box::new(L::Cons(
                Box::new(St::Op(7)),
                Box::new(L::Cons(Box::new(St::Op(8)), Box::new(L::Nil))),
            )),
        );
        elim(elim(p)) == elim(p)
    }),
{
    let p = L::Cons(
        Box::new(St::Op(7)),
        Box::new(L::Cons(
            Box::new(St::Op(7)),
            Box::new(L::Cons(Box::new(St::Op(8)), Box::new(L::Nil))),
        )),
    );
    reveal_with_fuel(elim, 6);
    elim_leaves_no_duplicates(p);
    // after the pass there are no consecutive duplicates, so re-running the
    // same comparison finds nothing to drop
    assert(dedup_free(elim(p)));
}

fn main() {}
}
