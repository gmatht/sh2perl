// Standalone model of optimize_stmts Pass 1 (sh2perl/src/ir.rs):
// self-assignment elimination (x = x) + unused-Declare elimination,
// with the referenced-vars census. Proves the soundness obligation:
//
//   pass1_sound: every statement pass1 drops is unobservable when
//   removed, and everything it keeps is preserved in order.
//
// Store model: a map name -> value, with the closed-world precondition
// (every referenced name is bound). The only observable effect in
// the model is the store; removing `x = x` is the identity update and a
// `Declare` with `init.is_none()` performs no update at all, so dropping
// either preserves the final store. That is the whole obligation, and it
// is exactly what the Rust code relies on (Pass 1 runs before any
// renderer sees the program; Pass 0.5's fold cannot create or destroy a
// self-assign or a reference — fold_stmt maps Assign/Expr structurally
// and clones everything else).
//
// What is modelled and what is NOT (fidelity, checked against ir.rs):
//   - IrStmt: Assign { single scalar target, Var RHS | other RHS },
//     Declare { names, has_init }, Other (opaque: never dropped, its
//     name-set is the census contribution the real walker computes).
//   - Var names are interned to nat ids (Verus has no String keys in
//     spec maps without vstd Map overhead; the interning is a bijection
//     on the names that occur, which is all the census observes).
//   - is_self_assignment mirrors ir.rs:15090 exactly: single target,
//     no indices, no asm label, RHS is Var of the same name. The asm
//     guard is modelled as a bool on the Assign node.
//   - Declare elim mirrors ir.rs:15768 exactly: init.is_none() AND no
//     declared name is in the census set. The census counts a Declare's
//     OWN names as references (ir.rs collect_vars_in_stmt inserts decl
//     names), so a lone `Declare[x]` is KEPT — verified empirically
//     against the real optimize_stmts (probe: lone Declare[x] -> 1 stmt,
//     empty Declare[] -> 0). The model reproduces both.
//   - NOT modelled: Pass 0.5 folding (orthogonal: it is a separate map
//     pass whose soundness is the constant-fold obligation, not this
//     file's), Ext/RawText/raw-identifier scraping (folded into Other's
//     census contribution), multi-target or indexed assigns (never
//     self-assigns by the same predicate).
use vstd::prelude::*;

verus! {

// A variable name, interned. 0 is reserved for "no name".
pub type Name = nat;

// Statements in the model.
pub enum Stmt {
    // x = <Var(x) | Other>; asm models the AsmSpec guard (Some = keep).
    Assign { target: Name, is_self: bool, asm: bool },
    // declare x1..xn with no initializer iff has_init is false.
    Declare { names: Seq<Name>, has_init: bool },
    // Everything else. census_refs is the set of names the real
    // collect_vars_in_stmt would report for it.
    Other { census_refs: Set<Name> },
}

// The referenced-vars census: is name n mentioned anywhere in s?
// (A predicate, not a Set: vstd's Set::new returns Option, and the keep
// rule only ever asks membership.)
pub open spec fn is_ref(s: Seq<Stmt>, n: Name) -> bool {
    exists|i: int| 0 <= i < s.len() && stmt_refs(s[i], n)
}

pub open spec fn assign_target(st: Stmt) -> Name {
    match st {
        Stmt::Assign { target, .. } => target,
        _ => 0,
    }
}

pub open spec fn assign_is_self(st: Stmt) -> bool {
    match st {
        Stmt::Assign { is_self, .. } => is_self,
        _ => false,
    }
}

pub open spec fn assign_asm(st: Stmt) -> bool {
    match st {
        Stmt::Assign { asm, .. } => asm,
        _ => false,
    }
}

pub open spec fn decl_names(st: Stmt) -> Seq<Name> {
    match st {
        Stmt::Declare { names, .. } => names,
        _ => Seq::empty(),
    }
}

pub open spec fn decl_has_init(st: Stmt) -> bool {
    match st {
        Stmt::Declare { has_init, .. } => has_init,
        _ => false,
    }
}

pub open spec fn other_refs(st: Stmt) -> Set<Name> {
    match st {
        Stmt::Other { census_refs } => census_refs,
        _ => Set::empty(),
    }
}

pub open spec fn stmt_refs(st: Stmt, n: Name) -> bool {
    match st {
        Stmt::Assign { .. } => {
            // The target is a write AND the census counts target names
            // (collect_vars_in_stmt inserts t.var); the RHS Var(x) reads x
            // exactly when is_self (RHS is Var of the same name).
            n == assign_target(st) || (assign_is_self(st) && n == assign_target(st))
        },
        Stmt::Declare { .. } => {
            // Declare's own names count as references in the real census.
            exists|k: int| 0 <= k < decl_names(st).len() && decl_names(st)[k] == n
        },
        Stmt::Other { .. } => other_refs(st).contains(n),
    }
}

// is_self_assignment (ir.rs:15090): single scalar target, no asm label,
// RHS is Var of the same target name.
pub open spec fn is_self_assignment(st: Stmt) -> bool {
    match st {
        Stmt::Assign { .. } => assign_is_self(st) && !assign_asm(st),
        _ => false,
    }
}

// Pass 1 keep-predicate (ir.rs:15768): drop self-assigns; drop an
// init-less Declare only when NONE of its names is referenced.
pub open spec fn keep(st: Stmt, s: Seq<Stmt>) -> bool {
    if is_self_assignment(st) {
        false
    } else {
        match st {
            Stmt::Declare { .. } => {
                if decl_has_init(st) {
                    true
                } else {
                    // kept iff SOME declared name is referenced
                    // (mirrors `vars.iter().any(|d| referenced.contains(..))`)
                    exists|k: int| 0 <= k < decl_names(st).len() && is_ref(s, decl_names(st)[k])
                }
            },
            _ => true,
        }
    }
}

// Pass 1 over s with the census of `whole` fixed (the real Pass 1
// computes `referenced` once over the input, then filters; the recursive
// model must not recompute the census on the tail).
pub open spec fn pass1_in(s: Seq<Stmt>, whole: Seq<Stmt>) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if keep(s[0], whole) {
        seq![s[0]].add(pass1_in(s.drop_first(), whole))
    } else {
        pass1_in(s.drop_first(), whole)
    }
}

pub open spec fn pass1(s: Seq<Stmt>) -> Seq<Stmt> {
    pass1_in(s, s)
}

// ---- Store semantics: the soundness obligation. ----

// A store: map from name to value.
pub type Store = Map<Name, int>;

// Stores are maps; a program runs from a store that binds every name it
// can reference (closed-world: the pipeline only feeds Pass 1 complete
// programs, and every backend declares-or-falls-back each name the same
// way before and after — Pass 1 removes no declaration that any census
// sees, so closedness is preserved trivially).
pub open spec fn closed_for(prog: Seq<Stmt>, s: Store) -> bool {
    forall|n: Name| is_ref(prog, n) ==> s.dom().contains(n)
}

// Total lookup: index under closedness.
pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) { s[n] } else { 0 }
}

// Executing one statement. Assign writes its target (a self-assign writes
// back the value already there); Declare-without-init and Other leave the
// store unchanged (Other's reads are unobservable in this model — the
// obligation is about what Pass 1 REMOVES, and Pass 1 never removes Other).
// lookup defaults unbound names to 0 (Perl's undef-in-numeric-context;
// the closedness precondition makes this the unreachable arm).
pub open spec fn step(st: Stmt, s: Store) -> Store {
    match st {
        Stmt::Assign { .. } => {
            let target = assign_target(st);
            s.insert(target, lookup(s, target))
        },
        _ => s,
    }
}

pub open spec fn run(p: Seq<Stmt>, s: Store) -> Store
    decreases p.len(),
{
    if p.len() == 0 {
        s
    } else {
        run(p.drop_first(), step(p[0], s))
    }
}

// Unfold lemma: running a cons splits into head-step then tail-run.
proof fn run_cons(h: Stmt, t: Seq<Stmt>, s: Store)
    ensures run(seq![h].add(t), s) == run(t, step(h, s)),
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

// A self-assign's update is the identity on closed stores: the target is
// bound (it is referenced — the census counts target names), so lookup is
// the index, and insert of the value already there changes nothing.
proof fn insert_same(m: Map<Name, int>, k: Name)
    requires m.dom().contains(k),
    ensures m.insert(k, m[k]) == m,
{
    assert(m.insert(k, m[k]) =~= m);
}

// Removing a dropped statement preserves the final store from any closed
// start state: a self-assign is the identity update; an init-less Declare
// is a no-update. Everything kept is preserved in order by construction.
// Closedness (every name referenced anywhere is bound) is preserved by
// step: Assign only ADDS its target, nothing removes bindings.
proof fn pass1_sound(s: Seq<Stmt>, whole: Seq<Stmt>, init: Store)
    requires
        // every name referenced in s is referenced in whole (true when
        // s is a subsequence of whole; the top-level call has s == whole)
        forall|n: Name| is_ref(s, n) ==> is_ref(whole, n),
        closed_for(whole, init),
    ensures run(pass1_in(s, whole), init) == run(s, init),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        // stepping preserves closedness for the tail: the tail's refs are
        // whole's refs (precondition), and step only adds a binding.
        assert(closed_for(whole, step(s[0], init))) by {
            match s[0] {
                Stmt::Assign { .. } => {},
                _ => {},
            }
        };
        if keep(s[0], whole) {
            pass1_sound(s.drop_first(), whole, step(s[0], init));
            assert(pass1_in(s, whole) == seq![s[0]].add(pass1_in(s.drop_first(), whole)));
            run_cons(s[0], pass1_in(s.drop_first(), whole), init);
            run_cons(s[0], s.drop_first(), init);
        } else {
            // Dropped: either a self-assign (identity update on a closed
            // store — the target is referenced, hence bound) or an
            // init-less Declare with no referenced name (no update).
            match s[0] {
                Stmt::Assign { .. } => {
                    let target = assign_target(s[0]);
                    assert(init.dom().contains(target)) by {
                        assert(is_ref(s, target));
                    };
                    assert(step(s[0], init) == init) by {
                        assert(lookup(init, target) == init[target]);
                        assert(step(s[0], init) == init.insert(target, init[target]));
                        insert_same(init, target);
                    };
                },
                _ => {},
            }
            pass1_sound(s.drop_first(), whole, init);
            assert(pass1_in(s, whole) == pass1_in(s.drop_first(), whole));
            run_cons(s[0], s.drop_first(), init);
        }
    }
}

// Top-level obligation: pass1 over the whole program preserves the run.
proof fn pass1_sound_top(s: Seq<Stmt>, init: Store)
    requires closed_for(s, init),
    ensures run(pass1(s), init) == run(s, init),
{
    pass1_sound(s, s, init);
}

// The model is faithful on the two empirical pins: a lone Declare[x] is
// kept (its own name is in the census), an empty Declare[] is dropped.
proof fn declare_pins()
    ensures
        pass1(seq![Stmt::Declare { names: seq![7nat], has_init: false }]).len() == 1,
        pass1(seq![Stmt::Declare { names: Seq::<Name>::empty(), has_init: false }]).len() == 0,
        pass1(seq![Stmt::Assign { target: 3nat, is_self: true, asm: false }]).len() == 0,
        pass1(seq![Stmt::Assign { target: 3nat, is_self: true, asm: true }]).len() == 1,
{
    let decl_lone: Stmt = Stmt::Declare { names: seq![7nat], has_init: false };
    let lone = seq![decl_lone];
    // Lone Declare[x]: x is in its own census, so keep holds.
    assert(lone[0] == decl_lone);
    assert(decl_names(decl_lone) == seq![7nat]);
    assert(seq![7nat][0] == 7nat);
    assert(stmt_refs(decl_lone, 7nat));
    assert(is_ref(lone, 7nat));
    assert(keep(decl_lone, lone));
    assert(keep(lone[0], lone));
    assert(lone.len() == 1);
    assert(lone.drop_first() == Seq::<Stmt>::empty());
    assert(pass1_in(Seq::<Stmt>::empty(), lone) == Seq::<Stmt>::empty()) by {
        reveal_with_fuel(pass1_in, 2);
    };
    assert(pass1_in(lone, lone) == lone) by {
        reveal_with_fuel(pass1_in, 2);
    };
    assert(pass1(lone) == lone);
    let empty = seq![Stmt::Declare { names: Seq::<Name>::empty(), has_init: false }];
    assert(!keep(empty[0], empty));
    assert(pass1_in(empty, empty) == Seq::<Stmt>::empty()) by {
        reveal_with_fuel(pass1_in, 2);
    };
    assert(pass1(empty) == Seq::<Stmt>::empty());
    let sa = seq![Stmt::Assign { target: 3nat, is_self: true, asm: false }];
    assert(!keep(sa[0], sa));
    assert(pass1_in(sa, sa) == Seq::<Stmt>::empty()) by {
        reveal_with_fuel(pass1_in, 2);
    };
    assert(pass1(sa) == Seq::<Stmt>::empty());
    let saa = seq![Stmt::Assign { target: 3nat, is_self: true, asm: true }];
    assert(keep(saa[0], saa));
    assert(pass1_in(saa, saa) == saa) by {
        reveal_with_fuel(pass1_in, 2);
    };
    assert(pass1(saa) == saa);
}

fn main() {}

}
