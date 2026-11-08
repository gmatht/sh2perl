// Conditional model of `background-decide`
// (`sh2perl/src/transforms/background_decide.rs`).
//
// NOT a Tier-1 artifact: it mentions the denotation of a STATEMENT (`run`),
// so it is the conditional "middle rung" -- an equivalence under a stated
// environment model, not a verdict theorem over IR-shaped data.
//
// ## The obligation
//
// The transform is analysis-only. For every `cmd &` body it stores a
// verdict: THREAD (self-contained pure compute -- a renderer may run the
// body on a worker with a FRESH runtime) or FORK (run detached on the
// current chain, i.e. bash's subshell). The routing is sound iff, for every
// THREAD verdict, the two routings print the same bytes:
//
//   thread_equals_fork: intended_ok(s, {}) => as_fork(s,m) == as_thread(s,m)
//
// ## Environment model (the hypothesis, stated not proved)
//
// bash's `cmd &` runs the body in a subshell:
//   - variable READS see the parent's bindings (the subshell inherits them);
//   - variable WRITES are discarded when the subshell ends;
//   - stdout is shared (a worker prints to the same stream).
//
// `M` carries `vars` and the printed `out`; `run` implements exactly those
// three rules. An unset variable reads as 0: we only need "a fresh runtime
// does not have the parent's value", not what `echo "$x"` of an unset x
// prints.
//
// ## What is proved
//
//   - `thread_equals_fork` -- the Thread routing is observationally equal to
//     Fork for every body the ORDERED rule accepts (every read sees a binding
//     the body itself assigned earlier on that path).
//   - `intended_implies_shipped` -- the ordered rule is at least as strict as
//     the set-membership check the pass implements, so fixing the check would
//     refuse nothing the pass accepts today.
//   - `shipped_check_is_unsound` -- the two are NOT equivalent. The pass's
//     `is_self_contained_stmt` collects `assigned` and `reads` into two
//     HashSets (`collect`) and compares them as SETS, never recording an
//     order, while the module's own doc states the rule as "a variable the
//     body itself assigns first ... assigned BEFORE it is read". It therefore
//     accepts `if [ -n "$x" ]; then x=1; echo 7; fi`: the subshell inherits
//     x, prints 7; a worker with a fresh runtime sees x unset, prints
//     nothing. The ordered rule rejects that body.
//
// ## What is NOT proved
//
//   - concurrency: threads, races, the runtime's scheduling. The model
//     compares two sequential runs, which is what "only args + stdout cross
//     the boundary" reduces to.
//   - signals, exit codes, fd state, PATH resolution, `Exec` subprocess
//     effects (the pass counts a subprocess exec as self-contained compute;
//     not modelled), file writes (`WriteFile` is modelled as a refused side
//     effect, which is what the pass does).
//   - the per-statement loop `body.iter().all(is_self_contained_stmt)`: the
//     model checks the whole body with the ordered rule. The witness below is
//     a SINGLE statement, so it passes the per-statement loop unchanged --
//     the finding does not depend on the loop shape.
//   - the pass's traversal/verdict cache (pointer-keyed HashSet); this is
//     about the predicate, not the plumbing.
use vstd::prelude::*;

verus! {

// ---- the statement language (the shape the pass classifies) ----

pub enum S {
    Skip,
    /// `x=k` -- a local assignment (discarded with the subshell).
    Assign(int, int),
    /// `echo "$x"` -- reads the parent's binding when the subshell inherits it.
    OutRead(int),
    /// `echo k` -- stdout only, no read.
    OutConst(int),
    /// a file write: observable outside the body -> the pass refuses it.
    WriteFile,
    /// `return`/`exit`/`die`: control transfer -> the pass refuses it.
    Exit,
    /// `if [ -n "$x" ]; then A; else B; fi` -- reads x in the condition.
    IfRead(int, Box<S>, Box<S>),
    Seq(Box<S>, Box<S>),
}

pub struct M {
    pub vars: Map<int, int>,
    pub out: Seq<int>,
}

// ---- the pass's syntactic check (mirrors `reads`/`assigns`/`collect`) ----

pub open spec fn reads(s: S) -> Set<int>
    decreases s,
{
    match s {
        S::Skip => Set::empty(),
        S::Assign(_, _) => Set::empty(),
        S::OutRead(x) => Set::empty().insert(x),
        S::OutConst(_) => Set::empty(),
        S::WriteFile => Set::empty(),
        S::Exit => Set::empty(),
        S::IfRead(x, a, b) => reads(*a).union(reads(*b)).insert(x),
        S::Seq(a, b) => reads(*a).union(reads(*b)),
    }
}

pub open spec fn assigns(s: S) -> Set<int>
    decreases s,
{
    match s {
        S::Skip => Set::empty(),
        S::Assign(x, _) => Set::empty().insert(x),
        S::OutRead(_) => Set::empty(),
        S::OutConst(_) => Set::empty(),
        S::WriteFile => Set::empty(),
        S::Exit => Set::empty(),
        S::IfRead(_, a, b) => assigns(*a).union(assigns(*b)),
        S::Seq(a, b) => assigns(*a).union(assigns(*b)),
    }
}

pub open spec fn has_se(s: S) -> bool
    decreases s,
{
    match s {
        S::Skip => false,
        S::Assign(_, _) => false,
        S::OutRead(_) => false,
        S::OutConst(_) => false,
        S::WriteFile => true,
        S::Exit => true,
        S::IfRead(_, a, b) => has_se(*a) || has_se(*b),
        S::Seq(a, b) => has_se(*a) || has_se(*b),
    }
}

/// `is_self_contained_stmt`: every read is of a name the statement assigns
/// SOMEWHERE, and there is no side effect. Order is not consulted.
pub open spec fn shipped_ok(s: S) -> bool {
    reads(s).subset_of(assigns(s)) && !has_se(s)
}

// ---- the rule the module DOC states (ordered) ----

/// The names bound on every path through `s` when it starts with `d` bound.
/// An `if` keeps only what BOTH arms bind (bash cannot know which ran).
pub open spec fn defined_after(s: S, d: Set<int>) -> Set<int>
    decreases s,
{
    match s {
        S::Skip => d,
        S::Assign(x, _) => d.insert(x),
        S::OutRead(_) => d,
        S::OutConst(_) => d,
        S::WriteFile => d,
        S::Exit => d,
        S::IfRead(_, a, b) => defined_after(*a, d).intersect(defined_after(*b, d)),
        S::Seq(a, b) => defined_after(*b, defined_after(*a, d)),
    }
}

/// "Every read sees a binding this body assigned earlier on that path."
pub open spec fn intended_ok(s: S, d: Set<int>) -> bool
    decreases s,
{
    match s {
        S::Skip => true,
        S::Assign(_, _) => true,
        S::OutRead(x) => d.contains(x),
        S::OutConst(_) => true,
        S::WriteFile => false,
        S::Exit => false,
        S::IfRead(x, a, b) => d.contains(x) && intended_ok(*a, d) && intended_ok(*b, d),
        S::Seq(a, b) => intended_ok(*a, d) && intended_ok(*b, defined_after(*a, d)),
    }
}

// ---- the two routings ----

pub open spec fn read_val(vars: Map<int, int>, x: int) -> int {
    if vars.dom().contains(x) {
        vars[x]
    } else {
        0
    }
}

/// Fork: the body runs as bash's subshell -- reads inherited, writes local
/// (they are discarded with the subshell, so only `.out` is observed).
pub open spec fn run(s: S, m: M) -> M
    decreases s,
{
    match s {
        S::Skip => m,
        S::Assign(x, k) => M { vars: m.vars.insert(x, k), out: m.out },
        S::OutRead(x) => M { vars: m.vars, out: m.out.push(read_val(m.vars, x)) },
        S::OutConst(k) => M { vars: m.vars, out: m.out.push(k) },
        S::WriteFile => m,
        S::Exit => m,
        S::IfRead(x, a, b) => if read_val(m.vars, x) != 0 {
            run(*a, m)
        } else {
            run(*b, m)
        },
        S::Seq(a, b) => run(*b, run(*a, m)),
    }
}

/// The observable of the Fork routing (subshell: inherits the parent vars).
pub open spec fn as_fork(s: S, m: M) -> Seq<int> {
    run(s, m).out
}

/// The observable of the Thread routing (fresh runtime: no inherited vars,
/// the same stdout stream).
pub open spec fn as_thread(s: S, m: M) -> Seq<int> {
    run(s, M { vars: Map::empty(), out: m.out }).out
}

// ---- the agreement (framing) lemma ----

/// `m` and `n` bind the same values for every name in `d`.
pub open spec fn agree(d: Set<int>, m: M, n: M) -> bool {
    forall|x: int|
        #[trigger] d.contains(x) ==> (m.vars.dom().contains(x) && n.vars.dom().contains(x)
            && m.vars[x] == n.vars[x])
}

proof fn agree_lemma(s: S, d: Set<int>, m: M, n: M)
    requires
        intended_ok(s, d),
        agree(d, m, n),
        m.out == n.out,
    ensures
        run(s, m).out == run(s, n).out,
        agree(defined_after(s, d), run(s, m), run(s, n)),
    decreases s,
{
    match s {
        S::Skip => {}
        S::Assign(x, k) => {
            assert forall|y: int|
                defined_after(s, d).contains(y) implies (run(s, m).vars.dom().contains(y)
                && run(s, n).vars.dom().contains(y) && run(s, m).vars[y] == run(s, n).vars[y]) by {
                if y == x {
                } else {
                    assert(d.contains(y));
                }
            }
        }
        S::OutRead(x) => {
            assert(d.contains(x));
            assert(read_val(m.vars, x) == read_val(n.vars, x));
        }
        S::OutConst(k) => {}
        S::WriteFile => {}
        S::Exit => {}
        S::IfRead(x, a, b) => {
            assert(d.contains(x));
            assert(read_val(m.vars, x) == read_val(n.vars, x));
            if read_val(m.vars, x) != 0 {
                agree_lemma(*a, d, m, n);
            } else {
                agree_lemma(*b, d, m, n);
            }
            assert forall|y: int|
                defined_after(s, d).contains(y) implies (run(s, m).vars.dom().contains(y)
                && run(s, n).vars.dom().contains(y) && run(s, m).vars[y] == run(s, n).vars[y]) by {
                if read_val(m.vars, x) != 0 {
                    assert(defined_after(*a, d).contains(y));
                } else {
                    assert(defined_after(*b, d).contains(y));
                }
            }
        }
        S::Seq(a, b) => {
            agree_lemma(*a, d, m, n);
            agree_lemma(*b, defined_after(*a, d), run(*a, m), run(*a, n));
        }
    }
}

// ---- THE OBLIGATION ----

/// If the ordered rule accepts the body, then running it on a worker with a
/// fresh runtime prints exactly what bash's subshell prints.
proof fn thread_equals_fork(s: S, m: M)
    requires
        intended_ok(s, Set::empty()),
    ensures
        as_fork(s, m) == as_thread(s, m),
{
    let fresh = M { vars: Map::empty(), out: m.out };
    assert(agree(Set::empty(), m, fresh)) by {
        assert forall|x: int| #[trigger] Set::empty().contains(x) implies (m.vars.dom().contains(x)
            && fresh.vars.dom().contains(x) && m.vars[x] == fresh.vars[x]) by {
        }
    }
    agree_lemma(s, Set::empty(), m, fresh);
}

// ---- the pass's check is implied by the ordered rule ----

/// Every name `defined_after` reports is either assigned by the statement or
/// was already bound -- the fact the implication lemma needs in the `Seq` arm.
proof fn defined_after_subset(s: S, d: Set<int>)
    ensures
        defined_after(s, d).subset_of(assigns(s).union(d)),
    decreases s,
{
    match s {
        S::Skip => {}
        S::Assign(x, _) => {
            assert(defined_after(s, d) == d.insert(x)) by {
                reveal(defined_after);
            }
            assert forall|y: int|
                defined_after(s, d).contains(y) implies assigns(s).union(d).contains(y) by {
                if y == x {
                } else {
                    assert(d.contains(y));
                }
            }
        }
        S::OutRead(_) => {}
        S::OutConst(_) => {}
        S::WriteFile => {}
        S::Exit => {}
        S::IfRead(_, a, b) => {
            defined_after_subset(*a, d);
            defined_after_subset(*b, d);
            assert(defined_after(s, d) == defined_after(*a, d).intersect(defined_after(*b, d))) by {
                reveal(defined_after);
            }
            assert forall|y: int|
                defined_after(s, d).contains(y) implies assigns(s).union(d).contains(y) by {
                assert(defined_after(*a, d).contains(y));
                assert(assigns(*a).union(d).contains(y));
            }
        }
        S::Seq(a, b) => {
            defined_after_subset(*a, d);
            defined_after_subset(*b, defined_after(*a, d));
            assert(defined_after(s, d) == defined_after(*b, defined_after(*a, d))) by {
                reveal(defined_after);
            }
            assert forall|y: int|
                defined_after(s, d).contains(y) implies assigns(s).union(d).contains(y) by {
                assert(assigns(*b).union(defined_after(*a, d)).contains(y));
                if assigns(*b).contains(y) {
                } else {
                    assert(defined_after(*a, d).contains(y));
                    assert(assigns(*a).union(d).contains(y));
                }
            }
        }
    }
}

/// The ordered rule never accepts something the pass would refuse: every read
/// is of a name the statement assigns (or that was already bound), and no side
/// effect survives. Replacing the check with the ordered rule therefore keeps
/// every Thread verdict the pass issues today.
proof fn intended_implies_shipped(s: S, d: Set<int>)
    requires
        intended_ok(s, d),
    ensures
        reads(s).subset_of(assigns(s).union(d)),
        !has_se(s),
    decreases s,
{
    match s {
        S::Skip => {}
        S::Assign(_, _) => {}
        S::OutRead(x) => {
            assert(reads(s) == Set::empty().insert(x)) by {
                reveal(reads);
            }
            assert(d.contains(x));
            assert forall|y: int| reads(s).contains(y) implies assigns(s).union(d).contains(y) by {
                if y == x {
                    assert(d.contains(y));
                }
            }
        }
        S::OutConst(_) => {}
        S::WriteFile => {}
        S::Exit => {}
        S::IfRead(x, a, b) => {
            intended_implies_shipped(*a, d);
            intended_implies_shipped(*b, d);
            assert(reads(s) == reads(*a).union(reads(*b)).insert(x)) by {
                reveal(reads);
            }
            assert forall|y: int| reads(s).contains(y) implies assigns(s).union(d).contains(y) by {
                if y == x {
                    assert(d.contains(y));
                } else if reads(*a).contains(y) {
                    assert(assigns(*a).union(d).contains(y));
                } else {
                    assert(reads(*b).contains(y));
                    assert(assigns(*b).union(d).contains(y));
                }
            }
        }
        S::Seq(a, b) => {
            intended_implies_shipped(*a, d);
            intended_implies_shipped(*b, defined_after(*a, d));
            defined_after_subset(*a, d);
            assert(reads(s) == reads(*a).union(reads(*b))) by {
                reveal(reads);
            }
            assert forall|y: int| reads(s).contains(y) implies assigns(s).union(d).contains(y) by {
                if reads(*a).contains(y) {
                    assert(assigns(*a).union(d).contains(y));
                } else {
                    assert(reads(*b).contains(y));
                    assert(assigns(*b).union(defined_after(*a, d)).contains(y));
                    if assigns(*b).contains(y) {
                    } else {
                        assert(defined_after(*a, d).contains(y));
                        assert(assigns(*a).union(d).contains(y));
                    }
                }
            }
        }
    }
}

// ---- NEGATIVE CONTROL: the shipped check accepts an unsafe body ----

/// `if [ -n "$x" ]; then x=1; echo 7; fi` -- the condition reads `x`, the
/// branch assigns it. The subscript checks order, so it is NOT thread-safe.
pub open spec fn witness() -> S {
    S::IfRead(
        0,
        Box::new(S::Seq(Box::new(S::Assign(0, 1)), Box::new(S::OutConst(7)))),
        Box::new(S::Skip),
    )
}

/// The parent binds x=1, so the subshell takes the branch and prints 7; a
/// fresh runtime sees x unset, takes the else arm, prints nothing. The pass
/// calls the body self-contained; the ordered rule does not.
proof fn shipped_check_is_unsound()
    ensures
        shipped_ok(witness()),
        !intended_ok(witness(), Set::empty()),
        as_fork(witness(), M { vars: map![0 => 1], out: Seq::empty() }).len() == 1,
        as_thread(witness(), M { vars: map![0 => 1], out: Seq::empty() }).len() == 0,
{
    let m = M { vars: map![0 => 1], out: Seq::empty() };
    assert(read_val(m.vars, 0) != 0);
    assert(read_val(Map::empty(), 0) == 0);
    assert(shipped_ok(witness())) by {
        reveal(witness);
        reveal_with_fuel(reads, 4);
        reveal_with_fuel(assigns, 4);
        reveal_with_fuel(has_se, 4);
        assert forall|y: int| reads(witness()).contains(y) implies assigns(witness()).contains(y) by {
        }
    }
    assert(!intended_ok(witness(), Set::empty())) by {
        reveal(witness);
        reveal_with_fuel(intended_ok, 3);
    }
    assert(as_fork(witness(), m).len() == 1) by {
        reveal(witness);
        reveal_with_fuel(run, 5);
    }
    assert(as_thread(witness(), m).len() == 0) by {
        reveal(witness);
        reveal_with_fuel(run, 5);
    }
}

fn main() {}

} // verus!
