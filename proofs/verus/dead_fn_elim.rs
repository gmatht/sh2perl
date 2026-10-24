// Standalone model of dead-fn-elim (sh2perl/src/transforms/dead_fn_elim.rs):
// remove Function definitions whose name never occurs as a string
// literal anywhere in the program. Proves the soundness obligation:
//
//   elim_sound: eliminating dead functions preserves the call trace,
//   from any fuel bound.
//
// Names are interned to nat (as in proofs/verus/optimize_stmts.rs).
// A function body is modelled as its literal occurrences: `calls` (callee
// names actually invoked — the execution content) and `lits` (every other
// literal occurrence in the body — conservative keepers). The keep
// criterion is membership of the name in the program's literal census,
// exactly the real Pass 2+3 rule (`defined` vs `strs`).
//
// Store model: none. The observable effect in the model is the call
// trace (the sequence of function invocations, externals included); a
// removed definition is never dispatched because its name occurs in no
// call position, so the trace is unchanged. Fuel bounds recursion in the
// spec (Verus needs structural termination); the theorem holds for all
// fuel, hence for the unbounded runs the fuel approximates.
//
// What is modelled and what is NOT (fidelity, checked against
// dead_fn_elim.rs):
//   - Pass 1 (defined names) + Pass 2 (literal census, def names
//     excluded — a def name is a String field, not a Str literal) +
//     Pass 3 (per-NAME removal: every def with a dead name goes) +
//     Pass 4/prune (dead defs removed at every nesting level — the model
//     is a flat sequence with defs allowed anywhere, same verdict).
//   - The KEEP_VARIABLES hatch is a bool parameter: set = identity.
//   - A call to an undefined name emits the name and stops (the shell's
//     external-exec fallback — preserved trivially since elim never
//     touches Calls and never adds defs).
//   - Case-pattern identifier harvesting (collect_identifiers) is folded
//     into body/Other lits: any literal occurrence keeps.
//   - NOT modelled: InterpPart::Lit runs. The real collect_expr_strs
//     skips Lit fragments, so a function referenced ONLY through a
//     concatenated literal is dropped by the real code but kept by this
//     model — the model is conservative (keeps superset real keeps) and
//     the proof covers every drop the model makes. Lit-fragment-only
//     drops are out of scope (flagged, not proved).
//   - First-def wins in find_body; bash uses last-def. Either rule is
//     preserved by per-name uniform removal, and the theorem is about
//     elim-preservation, not bash equivalence — but the two sides must
//     use the SAME rule, which they do.
use vstd::prelude::*;

verus! {

// A name (function or literal occurrence), interned.
pub type Name = nat;

// A function body: calls = invoked callee names (execution);
// lits = all other literal occurrences (keepers only).
pub struct Body {
    pub calls: Seq<Name>,
    pub lits: Seq<Name>,
}

pub enum Stmt {
    FnDef { name: Name, body: Body },
    Call { name: Name },
    // Any other statement; lits = the literal occurrences the real
    // census would harvest from it (Str args, target vars, index vars,
    // case-pattern identifiers). Never removed, never dispatched.
    Other { lits: Seq<Name> },
}

// ---- Field projectors (match arms returning fields directly reduce;
// ---- an exists nested inside a match arm does not — see
// ---- proofs/verus/optimize_stmts.rs and the mini5 probe). ----
pub open spec fn stmt_kind(st: Stmt) -> nat {
    match st {
        Stmt::FnDef { .. } => 0,
        Stmt::Call { .. } => 1,
        Stmt::Other { .. } => 2,
    }
}

pub open spec fn fndef_name(st: Stmt) -> Name {
    match st {
        Stmt::FnDef { name, .. } => name,
        _ => 0,
    }
}

pub open spec fn fndef_body(st: Stmt) -> Body {
    match st {
        Stmt::FnDef { body, .. } => body,
        _ => Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() },
    }
}

pub open spec fn call_name(st: Stmt) -> Name {
    match st {
        Stmt::Call { name } => name,
        _ => 0,
    }
}

pub open spec fn other_lits(st: Stmt) -> Seq<Name> {
    match st {
        Stmt::Other { lits } => lits,
        _ => Seq::empty(),
    }
}

// Every literal occurrence contributed by one statement. A def's own
// NAME is excluded (it is a field, not a literal — the real Pass 2
// skips def names for exactly this reason).
pub open spec fn stmt_lits(st: Stmt) -> Seq<Name> {
    if stmt_kind(st) == 0 {
        fndef_body(st).calls.add(fndef_body(st).lits)
    } else if stmt_kind(st) == 1 {
        seq![call_name(st)]
    } else {
        other_lits(st)
    }
}

pub open spec fn seq_has(s: Seq<Name>, n: Name) -> bool {
    exists|k: int| 0 <= k < s.len() && s[k] == n
}

pub open spec fn stmt_has(st: Stmt, n: Name) -> bool {
    seq_has(stmt_lits(st), n)
}

// The literal census: does n occur anywhere in p?
pub open spec fn is_ref(p: Seq<Stmt>, n: Name) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_has(p[i], n)
}

// Every name in a call worklist occurs as a literal in the program.
pub open spec fn all_refs(p: Seq<Stmt>, calls: Seq<Name>) -> bool {
    forall|i: int| 0 <= i < calls.len() ==> is_ref(p, calls[i])
}

// Keep rule (Pass 3): a def survives iff its name is referenced;
// everything else survives unconditionally.
pub open spec fn keep_fn(st: Stmt, whole: Seq<Stmt>) -> bool {
    if stmt_kind(st) == 0 {
        is_ref(whole, fndef_name(st))
    } else {
        true
    }
}

// Elimination with the census of `whole` fixed (the real transform
// computes `strs` once, then removes — like optimize_stmts' pass1_in).
pub open spec fn elim_in(s: Seq<Stmt>, whole: Seq<Stmt>) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if keep_fn(s[0], whole) {
        seq![s[0]].add(elim_in(s.drop_first(), whole))
    } else {
        elim_in(s.drop_first(), whole)
    }
}

pub open spec fn elim_raw(p: Seq<Stmt>) -> Seq<Stmt> {
    elim_in(p, p)
}

// The transform: the KEEP_VARIABLES hatch makes it the identity.
pub open spec fn elim(p: Seq<Stmt>, keep_all: bool) -> Seq<Stmt> {
    if keep_all {
        p
    } else {
        elim_raw(p)
    }
}

// ---- Execution: call traces with fuel. ----

// First definition body for c in env (dummy when absent).
pub open spec fn find_body(env: Seq<Stmt>, c: Name) -> (bool, Body)
    decreases env.len(),
{
    if env.len() == 0 {
        (false, Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() })
    } else if stmt_kind(env[0]) == 0 && fndef_name(env[0]) == c {
        (true, fndef_body(env[0]))
    } else {
        find_body(env.drop_first(), c)
    }
}

// Dispatch one call: emit it; on a defined callee with fuel left,
// execute the body with one less fuel. An undefined callee is an
// external call — emitted, never expanded. Decreases (fuel, 0): the
// second component aligns with exec_calls' (fuel, len) for the mutual
// recursion (exec_calls calls dispatch at equal fuel but len > 0).
pub open spec fn dispatch(env: Seq<Stmt>, c: Name, fuel: int) -> Seq<Name>
    decreases fuel, 0int,
{
    if fuel <= 0 {
        seq![c]
    } else {
        let f = find_body(env, c);
        if f.0 {
            seq![c].add(exec_calls(env, f.1.calls, fuel - 1))
        } else {
            seq![c]
        }
    }
}

// Execute a worklist of calls in order.
pub open spec fn exec_calls(env: Seq<Stmt>, calls: Seq<Name>, fuel: int) -> Seq<Name>
    decreases fuel, calls.len(),
{
    if fuel <= 0 || calls.len() == 0 {
        Seq::empty()
    } else {
        dispatch(env, calls[0], fuel).add(exec_calls(env, calls.drop_first(), fuel))
    }
}

// Top-level calls of a program, in order.
pub open spec fn top_calls(p: Seq<Stmt>) -> Seq<Name>
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else if stmt_kind(p[0]) == 1 {
        seq![call_name(p[0])].add(top_calls(p.drop_first()))
    } else {
        top_calls(p.drop_first())
    }
}

// The observable run: trace of all invocations from the top level.
pub open spec fn run_prog(p: Seq<Stmt>, fuel: int) -> Seq<Name> {
    exec_calls(p, top_calls(p), fuel)
}

// ---- Lemmas. ----

// Unfold helpers (recursive spec fns need explicit unfolding at use).
proof fn exec_cons(env: Seq<Stmt>, calls: Seq<Name>, fuel: int)
    requires fuel > 0 && calls.len() > 0,
    ensures exec_calls(env, calls, fuel) == dispatch(env, calls[0], fuel).add(exec_calls(env, calls.drop_first(), fuel)),
{
}

proof fn dispatch_hit(env: Seq<Stmt>, c: Name, fuel: int)
    requires fuel > 0 && find_body(env, c).0,
    ensures dispatch(env, c, fuel) == seq![c].add(exec_calls(env, find_body(env, c).1.calls, fuel - 1)),
{
}

proof fn dispatch_miss(env: Seq<Stmt>, c: Name, fuel: int)
    requires fuel <= 0 || !find_body(env, c).0,
    ensures dispatch(env, c, fuel) == seq![c] || fuel <= 0 && dispatch(env, c, fuel) == seq![c],
{
}

// Unfold find_body at a head that defines c.
proof fn find_body_hit(env: Seq<Stmt>, c: Name)
    requires
        env.len() > 0,
        stmt_kind(env[0]) == 0,
        fndef_name(env[0]) == c,
    ensures find_body(env, c) == (true, fndef_body(env[0])),
{
    reveal_with_fuel(find_body, 2);
}

// Unfold find_body past a head that does not define c.
proof fn find_body_skip(env: Seq<Stmt>, c: Name)
    requires
        env.len() > 0,
        !(stmt_kind(env[0]) == 0 && fndef_name(env[0]) == c),
    ensures find_body(env, c) == find_body(env.drop_first(), c),
{
    reveal_with_fuel(find_body, 2);
}

// Unfold elim_in one step, by verdict (no if to evaluate).
proof fn elim_step_kept(s: Seq<Stmt>, whole: Seq<Stmt>)
    requires s.len() > 0, keep_fn(s[0], whole),
    ensures elim_in(s, whole) == seq![s[0]].add(elim_in(s.drop_first(), whole)),
{
    reveal_with_fuel(elim_in, 2);
}

proof fn elim_step_dropped(s: Seq<Stmt>, whole: Seq<Stmt>)
    requires s.len() > 0, !keep_fn(s[0], whole),
    ensures elim_in(s, whole) == elim_in(s.drop_first(), whole),
{
    reveal_with_fuel(elim_in, 2);
}

// Unfold elim_in one step.
proof fn elim_step(s: Seq<Stmt>, whole: Seq<Stmt>)
    requires s.len() > 0,
    ensures elim_in(s, whole) == (if keep_fn(s[0], whole) { seq![s[0]].add(elim_in(s.drop_first(), whole)) } else { elim_in(s.drop_first(), whole) }),
{
    reveal_with_fuel(elim_in, 2);
}

// Unfold top_calls one step, by outcome.
proof fn top_step_call(s: Seq<Stmt>)
    requires s.len() > 0, stmt_kind(s[0]) == 1,
    ensures top_calls(s) == seq![call_name(s[0])].add(top_calls(s.drop_first())),
{
    reveal_with_fuel(top_calls, 2);
}

proof fn top_step_nocall(s: Seq<Stmt>)
    requires s.len() > 0, stmt_kind(s[0]) != 1,
    ensures top_calls(s) == top_calls(s.drop_first()),
{
    reveal_with_fuel(top_calls, 2);
}

// Unfold top_calls one step.
proof fn top_step(s: Seq<Stmt>)
    requires s.len() > 0,
    ensures top_calls(s) == (if stmt_kind(s[0]) == 1 { seq![call_name(s[0])].add(top_calls(s.drop_first())) } else { top_calls(s.drop_first()) }),
{
    reveal_with_fuel(top_calls, 2);
}

// Census monotonicity: a tail's references are the whole's references.
proof fn refs_mono(s: Seq<Stmt>, n: Name)
    requires
        s.len() > 0,
        is_ref(s.drop_first(), n),
    ensures is_ref(s, n),
{
    let i = choose|i: int| 0 <= i < s.drop_first().len() && stmt_has(s.drop_first()[i], n);
    assert(stmt_has(s[i + 1], n));
}

// A body found in env mentions only literals of env (it sits inside a
// def that is itself in env).
proof fn body_calls_ref(env: Seq<Stmt>, c: Name, j: int)
    requires
        find_body(env, c).0,
        0 <= j < find_body(env, c).1.calls.len(),
    ensures is_ref(env, find_body(env, c).1.calls[j]),
    decreases env.len(),
{
    if stmt_kind(env[0]) == 0 && fndef_name(env[0]) == c {
        let b = fndef_body(env[0]);
        assert(find_body(env, c).1 == b);
        assert(seq_has(stmt_lits(env[0]), b.calls[j])) by {
            assert(b.calls.add(b.lits)[j] == b.calls[j]);
        };
        assert(stmt_has(env[0], b.calls[j]));
        assert(is_ref(env, b.calls[j]));
    } else {
        assert(env.len() > 0);
        body_calls_ref(env.drop_first(), c, j);
        assert(is_ref(env.drop_first(), find_body(env.drop_first(), c).1.calls[j]));
        assert(find_body(env, c) == find_body(env.drop_first(), c));
        refs_mono(env, find_body(env, c).1.calls[j]);
    }
}

// Lookup agrees after elimination for every referenced name: removed
// defs all have unreferenced names, so the first def of a referenced
// name survives in place.
proof fn find_body_kept(s: Seq<Stmt>, whole: Seq<Stmt>, c: Name)
    requires
        forall|n: Name| is_ref(s, n) ==> is_ref(whole, n),
        is_ref(whole, c),
    ensures find_body(elim_in(s, whole), c) == find_body(s, c),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        assert(forall|n: Name| is_ref(s.drop_first(), n) ==> is_ref(whole, n)) by {
            assert forall|n: Name| is_ref(s.drop_first(), n) implies is_ref(whole, n) by {
                refs_mono(s, n);
            }
        };
        if stmt_kind(s[0]) == 0 {
            if fndef_name(s[0]) == c {
                // c's own first def: referenced, hence kept on both sides.
                assert(is_ref(whole, fndef_name(s[0])));
                assert(keep_fn(s[0], whole));
                elim_step_kept(s, whole);
                assert(elim_in(s, whole)[0] == s[0]);
                // Bridge: the skip lemmas speak of `.drop_first()`.
                assert(elim_in(s, whole).drop_first() == elim_in(s.drop_first(), whole));
                find_body_hit(elim_in(s, whole), c);
                find_body_hit(s, c);
            } else {
                // A def of some other name: both lookups skip it (kept or
                // not, its name differs from c), then agree by IH.
                if keep_fn(s[0], whole) {
                    elim_step_kept(s, whole);
                    assert(elim_in(s, whole)[0] == s[0]);
                    assert(elim_in(s, whole).drop_first() == elim_in(s.drop_first(), whole));
                    find_body_skip(elim_in(s, whole), c);
                    find_body_skip(s, c);
                } else {
                    elim_step_dropped(s, whole);
                    find_body_skip(s, c);
                }
                find_body_kept(s.drop_first(), whole, c);
                assert(find_body(elim_in(s.drop_first(), whole), c) == find_body(s.drop_first(), c));
                if !keep_fn(s[0], whole) {
                    assert(elim_in(s, whole) == elim_in(s.drop_first(), whole));
                    assert(find_body(elim_in(s, whole), c) == find_body(elim_in(s.drop_first(), whole), c));
                    assert(find_body(s, c) == find_body(s.drop_first(), c));
                }
            }
        } else {
            // Non-defs are always kept: both sides step identically.
            assert(keep_fn(s[0], whole));
            elim_step_kept(s, whole);
            assert(elim_in(s, whole)[0] == s[0]);
            assert(elim_in(s, whole).drop_first() == elim_in(s.drop_first(), whole));
            find_body_skip(elim_in(s, whole), c);
            find_body_skip(s, c);
            find_body_kept(s.drop_first(), whole, c);
        }
    }
}

// The core: executing any all-referenced worklist gives the same trace
// before and after elimination.
proof fn exec_calls_same(env: Seq<Stmt>, calls: Seq<Name>, fuel: int)
    requires
        fuel >= 0,
        all_refs(env, calls),
    ensures exec_calls(elim_raw(env), calls, fuel) == exec_calls(env, calls, fuel),
    decreases fuel, calls.len(),
{
    if fuel <= 0 || calls.len() == 0 {
    } else {
        let c = calls[0];
        assert(is_ref(env, c));
        // The dispatched call resolves identically on both sides.
        find_body_kept(env, env, c);
        assert(find_body(elim_raw(env), c) == find_body(env, c));
        // The tail worklist stays all-referenced.
        assert(all_refs(env, calls.drop_first())) by {
            assert forall|i: int| 0 <= i < calls.drop_first().len() implies is_ref(env, calls.drop_first()[i]) by {
                assert(is_ref(env, calls[i + 1]));
            }
        };
        exec_calls_same(env, calls.drop_first(), fuel);
        assert(exec_calls(elim_raw(env), calls.drop_first(), fuel) == exec_calls(env, calls.drop_first(), fuel));
        if find_body(env, c).0 {
            // The callee body runs with one less fuel; its calls are
            // literals of env, so the IH applies.
            assert(all_refs(env, find_body(env, c).1.calls)) by {
                assert forall|i: int| 0 <= i < find_body(env, c).1.calls.len() implies is_ref(env, find_body(env, c).1.calls[i]) by {
                    body_calls_ref(env, c, i);
                }
            };
            exec_calls_same(env, find_body(env, c).1.calls, fuel - 1);
            dispatch_hit(elim_raw(env), c, fuel);
            dispatch_hit(env, c, fuel);
        } else {
            dispatch_miss(elim_raw(env), c, fuel);
            dispatch_miss(env, c, fuel);
        }
        exec_cons(elim_raw(env), calls, fuel);
        exec_cons(env, calls, fuel);
    }
}

// Elimination keeps every Call (and Other) in order, so top-level calls
// are unchanged.
proof fn top_calls_elim(s: Seq<Stmt>, whole: Seq<Stmt>)
    requires forall|n: Name| is_ref(s, n) ==> is_ref(whole, n),
    ensures top_calls(elim_in(s, whole)) == top_calls(s),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        assert(forall|n: Name| is_ref(s.drop_first(), n) ==> is_ref(whole, n)) by {
            assert forall|n: Name| is_ref(s.drop_first(), n) implies is_ref(whole, n) by {
                refs_mono(s, n);
            }
        };
        top_calls_elim(s.drop_first(), whole);
        if keep_fn(s[0], whole) {
            elim_step_kept(s, whole);
            // Head survives: both sides cons identically.
            assert(elim_in(s, whole)[0] == s[0]);
            assert(elim_in(s, whole).drop_first() == elim_in(s.drop_first(), whole));
            assert(elim_in(s, whole).len() > 0);
            if stmt_kind(s[0]) == 1 {
                assert(stmt_kind(elim_in(s, whole)[0]) == 1);
                top_step_call(s);
                top_step_call(elim_in(s, whole));
            } else {
                assert(stmt_kind(elim_in(s, whole)[0]) != 1);
                top_step_nocall(s);
                top_step_nocall(elim_in(s, whole));
            }
        } else {
            // Only a def can drop; a def is never a top-level call.
            assert(stmt_kind(s[0]) == 0);
            elim_step_dropped(s, whole);
            assert(stmt_kind(s[0]) != 1);
            top_step_nocall(s);
        }
    }
}

proof fn top_calls_ref(p: Seq<Stmt>, i: int)
    requires
        0 <= i < top_calls(p).len(),
        p.len() > 0,
    ensures is_ref(p, top_calls(p)[i]),
    decreases p.len(),
{
    if stmt_kind(p[0]) == 1 {
        top_step_call(p);
        if i == 0 {
            assert(top_calls(p)[0] == call_name(p[0]));
            assert(seq_has(stmt_lits(p[0]), call_name(p[0]))) by {
                assert(stmt_lits(p[0]) == seq![call_name(p[0])]);
                assert(seq![call_name(p[0])][0] == call_name(p[0]));
            };
            assert(stmt_has(p[0], top_calls(p)[0]));
        } else {
            assert(top_calls(p)[i] == top_calls(p.drop_first())[i - 1]);
            top_calls_ref(p.drop_first(), i - 1);
            refs_mono(p, top_calls(p)[i]);
        }
    } else {
        top_step_nocall(p);
        assert(top_calls(p) == top_calls(p.drop_first()));
        top_calls_ref(p.drop_first(), i);
        refs_mono(p, top_calls(p)[i]);
    }
}

// THE OBLIGATION: dead-function elimination preserves the call trace.
proof fn elim_sound(p: Seq<Stmt>, keep_all: bool, fuel: int)
    requires fuel >= 0,
    ensures run_prog(elim(p, keep_all), fuel) == run_prog(p, fuel),
{
    if keep_all {
    } else {
        assert(elim(p, keep_all) == elim_raw(p));
        top_calls_elim(p, p);
        assert(top_calls(elim_raw(p)) == top_calls(p));
        assert(all_refs(p, top_calls(p))) by {
            assert(p.len() > 0 || top_calls(p).len() == 0) by {
                if p.len() == 0 {
                    assert(top_calls(p) == Seq::<Name>::empty()) by {
                        reveal_with_fuel(top_calls, 2);
                    };
                }
            };
            assert forall|i: int| 0 <= i < top_calls(p).len() implies is_ref(p, top_calls(p)[i]) by {
                top_calls_ref(p, i);
            }
        };
        exec_calls_same(p, top_calls(p), fuel);
    }
}

// Fidelity pins against dead_fn_elim.rs's own tests: an uncalled fn is
// removed; a called or indirectly-referenced fn is kept.
proof fn elim_pins()
    ensures
        elim_raw(seq![Stmt::FnDef { name: 1, body: Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() } }]).len() == 0,
        elim_raw(seq![
            Stmt::FnDef { name: 1, body: Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() } },
            Stmt::Call { name: 1 },
        ]).len() == 2,
        elim_raw(seq![
            Stmt::FnDef { name: 2, body: Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() } },
            Stmt::Other { lits: seq![2] },
        ]).len() == 2,
{
    let dead = seq![Stmt::FnDef { name: 1, body: Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() } }];
    assert(!keep_fn(dead[0], dead)) by {
        reveal_with_fuel(elim_in, 3);
    };
    assert(elim_in(dead, dead) == Seq::<Stmt>::empty()) by {
        reveal_with_fuel(elim_in, 3);
    };
    let live_def: Stmt = Stmt::FnDef { name: 1, body: Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() } };
    let live_call: Stmt = Stmt::Call { name: 1 };
    let live = seq![live_def, live_call];
    assert(live[0] == live_def);
    assert(live[1] == live_call);
    assert(stmt_kind(live[0]) == 0);
    assert(fndef_name(live[0]) == 1nat);
    assert(call_name(live[1]) == 1nat);
    assert(stmt_lits(live[1]) == seq![1nat]);
    assert(seq![1nat][0] == 1nat);
    assert(stmt_has(live[1], 1nat));
    assert(is_ref(live, 1nat));
    assert(keep_fn(live[0], live));
    assert(elim_in(live, live) == live) by {
        reveal_with_fuel(elim_in, 3);
    };
    let indir_def: Stmt = Stmt::FnDef { name: 2, body: Body { calls: Seq::<Name>::empty(), lits: Seq::<Name>::empty() } };
    let indir_oth: Stmt = Stmt::Other { lits: seq![2] };
    let indir = seq![indir_def, indir_oth];
    assert(indir[0] == indir_def);
    assert(indir[1] == indir_oth);
    assert(stmt_kind(indir[0]) == 0);
    assert(fndef_name(indir[0]) == 2nat);
    assert(other_lits(indir[1]) == seq![2nat]);
    assert(seq![2nat][0] == 2nat);
    assert(stmt_has(indir[1], 2nat));
    assert(is_ref(indir, 2nat));
    assert(keep_fn(indir[0], indir));
    assert(elim_in(indir, indir) == indir) by {
        reveal_with_fuel(elim_in, 3);
    };
}

fn main() {}

}
