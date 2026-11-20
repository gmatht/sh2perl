// Standalone model of the `ScopedHoist` shIR pass
// (sh2perl/src/shir_passes/scoped_hoist.rs), the frontend-ingress rewrite
// that turns a main-scope `s = <value>` into a DECLARATION so every
// renderer can home `s` in a host local (`let s = …`) instead of a global
// cell (`thread_local!` / module static / environment read).
//
// # What the rewrite changes
//
// Not the value, and not the position: the declaration sits exactly where
// the assignment sat, and the initialiser is evaluated there. What it
// changes is the **storage home**: the name moves from the shell's global
// cell to a main-scope `let`. That home is observable, which is the whole
// content of the law:
//
//   * a read in a NESTED scope (a function body, a subshell) sees the
//     global cell, never a main-scope `let`;
//   * a child reading the name from the ENVIRONMENT sees the exported
//     cell, never a `let`;
//   * a DYNAMIC store (`eval`, `setVar`) or any SECOND store writes the
//     cell, so it is invisible to a later main-scope read of the `let`;
//   * a read BEFORE the declaration (including a self-referential RHS,
//     `s = "${s:-d}"`) reads an uninitialised `let`, which is an error
//     (`E0381`/`E0425`), where the unset cell reads as the empty string;
//   * a read that exists only as SHELL-WORD TEXT (`test -n "$s"`) is not
//     seen by any renderer's binding census, so a `Declare` whose only
//     read is word text emits NO `let` while the read site still renders
//     the bare identifier (measured: `error[E0425]: cannot find value s`).
//     That is why the pass now demands an IR-VISIBLE condition read.
//
// So the pass is a home CHANGE, and the law is that the change is
// unobservable exactly when the census says it is:
//
//   run(hoist(evs), has_ir(evs)) == run(Global, has_ir(evs))   for all evs
//
// with `hoist`/`eligible` computed FROM THE EVENT LIST (no antecedent, no
// assumed side condition) and `run` a byte-trace semantics in which the
// two homes genuinely differ on every refused shape — each refusal has a
// witness below whose traces differ, and each witness is a mutant trap.
//
// # Stated contracts (not carved-out cases)
//
// * THE RENDERER'S BINDING CENSUS: a main-scope binding is emitted iff the
//   program has at least one IR-VISIBLE use of the name. That is the
//   measured behaviour of `compute_local_vars` (it counts `Var`/`getVar`/
//   word-text mentions through `use_scopes`, but the *binding* emission
//   path follows IR uses); the word-only case is pinned by the E0425
//   measurement above. The model encodes it as `has_ir`.
// * VALUE IDENTITY: every store in a witness writes the one value `v1`
//   (a second/dynamic store writes `v2`); the trace records which value a
//   read observed, so the home difference is visible as different bytes.
// * PURITY / PLAIN NAME / NO LOOP BINDING: conservative refusals that the
//   law does not need (an impure RHS is evaluated at the same position in
//   either home; a non-plain name never reaches a `let`). They are stated
//   here as the pass's own scope, not as hypotheses of the law.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 scoped_hoist.rs
//   -> verification results:: 21 verified, 0 errors
use vstd::prelude::*;

verus! {

// ---------- values ----------

/// The value the (single) top-level store writes — `'a'`.
pub open spec fn v1() -> int {
    97
}

/// The value a second / dynamic store writes — `'b'`.
pub open spec fn v2() -> int {
    98
}

/// The byte a main-scope read of an UNINITIALISED `let` stands for: the
/// binding does not exist yet, which is the renderer's E0381/E0425, not
/// bash's empty string.
pub open spec fn unbound() -> int {
    45
}

/// An empty global cell reads as the empty string: it contributes nothing.
pub open spec fn empty() -> int {
    0
}

// ---------- the homes ----------

pub enum Home {
    /// The shell's global cell (pre-rewrite home).
    Global,
    /// A main-scope `let` (post-rewrite home).
    Local,
}

// ---------- the event language ----------

/// One observable event of a program that mentions ONE name. Everything the
/// pass's census looks at is a property of this list.
pub enum Ev {
    /// The one main-scope, plain, top-level store (`s = <value>`).
    TopWrite,
    /// The same store whose RHS reads the name (`s = "${s:-d}"`).
    SelfWrite,
    /// Any further store: nested body, indexed/multi target, `setVar`,
    /// builtin write (`read`, `printf -v`), array store.
    SecondWrite,
    /// A read the IR carries: `Var`/`Ident`, a `getVar` argument.
    ReadMain,
    /// An IR-visible read in a main-scope conditional's TEST.
    CondReadMain,
    /// A shell-word read (`$s` inside `test -n "$s"`) in a real condition.
    WordCondMain,
    /// A read inside a function body / sub / subshell.
    ReadOther,
    /// A shell-word read inside a function body / sub / subshell.
    WordReadOther,
    /// A child process reads the name from the ENVIRONMENT.
    EnvRead,
    /// `eval` / `source` / `.` appears: it can store to any name.
    Dynamic,
}

// ---------- the census, derived from the event list ----------

pub open spec fn is_ir_read(ev: Ev) -> bool {
    match ev {
        Ev::ReadMain => true,
        Ev::CondReadMain => true,
        _ => false,
    }
}

pub open spec fn is_cond_ir(ev: Ev) -> bool {
    match ev {
        Ev::CondReadMain => true,
        _ => false,
    }
}

pub open spec fn is_main_read(ev: Ev) -> bool {
    match ev {
        Ev::ReadMain => true,
        Ev::CondReadMain => true,
        Ev::WordCondMain => true,
        _ => false,
    }
}

pub open spec fn any_ir(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        is_ir_read(evs[0]) || any_ir(evs.subrange(1, evs.len() as int))
    }
}

pub open spec fn any_cond_ir(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        is_cond_ir(evs[0]) || any_cond_ir(evs.subrange(1, evs.len() as int))
    }
}

/// Every event after the first is a main-scope read. This is the census:
/// no second store, no dynamic store, no other-scope read, no env read.
pub open spec fn only_main_reads(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        true
    } else {
        is_main_read(evs[0]) && only_main_reads(evs.subrange(1, evs.len() as int))
    }
}

pub open spec fn tail(evs: Seq<Ev>) -> Seq<Ev> {
    evs.subrange(1, evs.len() as int)
}

pub open spec fn first_is_top(evs: Seq<Ev>) -> bool {
    if evs.len() == 0 {
        false
    } else {
        match evs[0] {
            Ev::TopWrite => true,
            _ => false,
        }
    }
}

/// The pass's eligibility, exactly as `run()` computes it.
pub open spec fn eligible(evs: Seq<Ev>) -> bool {
    first_is_top(evs) && only_main_reads(tail(evs)) && any_cond_ir(evs)
}

/// The pass's decision: rewrite (Local) or leave alone (Global).
pub open spec fn hoist(evs: Seq<Ev>) -> Home {
    if eligible(evs) {
        Home::Local
    } else {
        Home::Global
    }
}

/// Does the program have the IR-visible use every renderer's binding census
/// needs before it will emit the `let`?
pub open spec fn bindable(evs: Seq<Ev>) -> bool {
    any_ir(evs)
}

// ---------- the semantics ----------

/// A main-scope read under `home`. `cval`/`lval` are the current values,
/// with `empty()` meaning "unbound".
pub open spec fn main_read(home: Home, cval: int, lval: int) -> Seq<int> {
    match home {
        Home::Global => if cval == empty() { Seq::empty() } else { seq![cval] },
        Home::Local => if lval == empty() { seq![unbound()] } else { seq![lval] },
    }
}

/// A read from any OTHER scope (function body, sub, subshell, child
/// environment): it can only ever see the global cell.
pub open spec fn other_read(cval: int) -> Seq<int> {
    if cval == empty() { Seq::empty() } else { seq![cval] }
}

pub open spec fn run(evs: Seq<Ev>, home: Home, bind: bool) -> Seq<int> {
    go(evs, home, bind, empty(), empty())
}

/// The value a main `let` holds after a top-level store: `v1` when the
/// binding is emitted, unchanged otherwise (the renderer emitted no `let`).
pub open spec fn local_after_top(home: Home, bind: bool, lval: int) -> int {
    match home {
        Home::Global => lval,
        Home::Local => if bind { v1() } else { lval },
    }
}

/// The value the GLOBAL CELL holds after the top-level store. Under the
/// Global home the store writes the cell; under the Local home it writes the
/// `let` and the cell is never touched (that is the whole difference).
pub open spec fn cell_after_top(home: Home, cval: int) -> int {
    match home {
        Home::Global => v1(),
        Home::Local => cval,
    }
}

pub open spec fn go(evs: Seq<Ev>, home: Home, bind: bool, cval: int, lval: int) -> Seq<int>
    decreases evs.len(),
{
    if evs.len() == 0 {
        Seq::empty()
    } else {
        let rest = evs.subrange(1, evs.len() as int);
        match evs[0] {
            Ev::TopWrite => go(rest, home, bind, cell_after_top(home, cval), local_after_top(home, bind, lval)),
            Ev::SelfWrite => main_read(home, cval, lval)
                + go(rest, home, bind, v1(), local_after_top(home, bind, lval)),
            Ev::SecondWrite => go(rest, home, bind, v2(), lval),
            Ev::Dynamic => go(rest, home, bind, v2(), lval),
            Ev::ReadMain => main_read(home, cval, lval) + go(rest, home, bind, cval, lval),
            Ev::CondReadMain => main_read(home, cval, lval) + go(rest, home, bind, cval, lval),
            Ev::WordCondMain => main_read(home, cval, lval) + go(rest, home, bind, cval, lval),
            Ev::ReadOther => other_read(cval) + go(rest, home, bind, cval, lval),
            Ev::WordReadOther => other_read(cval) + go(rest, home, bind, cval, lval),
            Ev::EnvRead => other_read(cval) + go(rest, home, bind, cval, lval),
        }
    }
}

// ---------- lemmas ----------

/// A condition read the IR carries is an IR read.
proof fn cond_ir_is_ir(evs: Seq<Ev>)
    ensures any_cond_ir(evs) ==> any_ir(evs),
    decreases evs.len(),
{
    reveal_with_fuel(any_cond_ir, 2);
    reveal_with_fuel(any_ir, 2);
    if evs.len() > 0 {
        cond_ir_is_ir(evs.subrange(1, evs.len() as int));
    }
}

/// After the top-level store the cell holds `v1` and the emitted `let`
/// holds `v1`. Every main-scope read therefore observes the same byte in
/// both homes. `lval_g` is the local slot under the GLOBAL home, which no
/// main read consults (the global home has no `let`).
proof fn main_reads_agree(evs: Seq<Ev>, cval_l: int, lval: int, cval_g: int, lval_g: int, bind: bool)
    requires
        only_main_reads(evs),
        lval == v1(),
        cval_g == v1(),
    ensures
        go(evs, Home::Local, bind, cval_l, lval) == go(evs, Home::Global, bind, cval_g, lval_g),
    decreases evs.len(),
{
    reveal_with_fuel(go, 2);
    reveal_with_fuel(only_main_reads, 2);
    if evs.len() > 0 {
        assert(is_main_read(evs[0]));
        main_reads_agree(evs.subrange(1, evs.len() as int), cval_l, lval, cval_g, lval_g, bind);
    }
}

// ---------- THE LAW ----------

/// The rewrite preserves the observable bytes, for EVERY event list. There
/// is no antecedent: an ineligible program is the identity (`hoist` is
/// Global, both sides agree), and an eligible one is proved to observe the
/// same value in both homes.
pub proof fn hoist_preserves_output(evs: Seq<Ev>)
    ensures
        go(evs, hoist(evs), bindable(evs), empty(), empty())
            == go(evs, Home::Global, bindable(evs), empty(), empty()),
{
    if eligible(evs) {
        cond_ir_is_ir(evs);
        assert(bindable(evs));
        assert(first_is_top(evs));
        assert(only_main_reads(tail(evs)));
        reveal_with_fuel(go, 2);
        reveal_with_fuel(any_ir, 3);
        reveal_with_fuel(any_cond_ir, 3);
        reveal_with_fuel(only_main_reads, 3);
        assert(tail(evs) == evs.subrange(1, evs.len() as int));
        assert(hoist(evs) == Home::Local);
        assert(cell_after_top(Home::Local, empty()) == empty());
        assert(cell_after_top(Home::Global, empty()) == v1());
        assert(local_after_top(Home::Local, bindable(evs), empty()) == v1());
        assert(go(evs, Home::Local, bindable(evs), empty(), empty())
            == go(tail(evs), Home::Local, bindable(evs), empty(), v1()));
        assert(go(evs, Home::Global, bindable(evs), empty(), empty())
            == go(tail(evs), Home::Global, bindable(evs), v1(), empty()));
        main_reads_agree(tail(evs), empty(), v1(), v1(), empty(), bindable(evs));
    }
}

// ---------- completeness: the pass is not the identity ----------

/// The shape this pass exists for — one top-level store, read from a
/// conditional's TEST — IS rewritten, and prints `'a'` on both sides.
pub open spec fn t60() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::CondReadMain]
}

proof fn a_conditional_reader_is_hoisted()
    ensures
        eligible(t60()),
        hoist(t60()) == Home::Local,
        hoist(t60()) != Home::Global,
        go(t60(), hoist(t60()), bindable(t60()), empty(), empty()) == seq![97],
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(any_cond_ir, 4);
    reveal_with_fuel(any_ir, 4);
    reveal_with_fuel(only_main_reads, 4);
    assert(any_cond_ir(t60()));
    assert(any_ir(t60()));
    assert(only_main_reads(tail(t60())));
}

/// Every eligible program is rewritten — eligibility is not a dead
/// predicate (the analogue of the fold law's `fold_leaves_nothing`).
proof fn eligible_implies_hoisted(evs: Seq<Ev>)
    ensures eligible(evs) ==> hoist(evs) == Home::Local,
{
}

/// A program with no condition read is left alone: the pass is narrow.
proof fn a_plain_reader_is_not_hoisted()
    ensures
        hoist(seq![Ev::TopWrite, Ev::ReadMain]) == Home::Global,
        go(seq![Ev::TopWrite, Ev::ReadMain], Home::Global, true, empty(), empty()) == seq![97],
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(any_cond_ir, 4);
    reveal_with_fuel(only_main_reads, 4);
}

// ---------- every refusal is load-bearing ----------

/// A read from a nested scope cannot see a main-scope `let`: the rewritten
/// program would print NOTHING where bash prints the value.
proof fn nested_read_is_load_bearing()
    ensures ({
        let w = seq![Ev::TopWrite, Ev::ReadOther];
        hoist(w) == Home::Global
            && go(w, Home::Global, true, empty(), empty()) == seq![97]
            && go(w, Home::Local, true, empty(), empty()) == Seq::<int>::empty()
            && go(w, Home::Local, true, empty(), empty())
                != go(w, Home::Global, true, empty(), empty())
    }),
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(only_main_reads, 4);
    reveal_with_fuel(any_cond_ir, 4);
}

/// A child that reads the name from the ENVIRONMENT sees the exported cell.
proof fn env_read_is_load_bearing()
    ensures ({
        let w = seq![Ev::TopWrite, Ev::EnvRead];
        hoist(w) == Home::Global
            && go(w, Home::Global, true, empty(), empty()) == seq![97]
            && go(w, Home::Local, true, empty(), empty()) == Seq::<int>::empty()
            && go(w, Home::Local, true, empty(), empty())
                != go(w, Home::Global, true, empty(), empty())
    }),
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(only_main_reads, 4);
    reveal_with_fuel(any_cond_ir, 4);
}

/// A second store writes the CELL; a later main-scope read of the `let`
/// would still see the first value.
proof fn second_write_is_load_bearing()
    ensures hoist(seq![Ev::TopWrite, Ev::SecondWrite, Ev::ReadMain]) == Home::Global,
        go(seq![Ev::TopWrite, Ev::SecondWrite, Ev::ReadMain], Home::Global, true, empty(), empty()) == seq![98],
        go(seq![Ev::TopWrite, Ev::SecondWrite, Ev::ReadMain], Home::Local, true, empty(), empty()) == seq![97],
{
    reveal_with_fuel(go, 5);
    reveal_with_fuel(only_main_reads, 5);
    reveal_with_fuel(any_cond_ir, 5);
}

proof fn dynamic_store_is_load_bearing()
    ensures ({
        let w = seq![Ev::TopWrite, Ev::Dynamic, Ev::ReadMain];
        hoist(w) == Home::Global
            && go(w, Home::Global, true, empty(), empty()) == seq![98]
            && go(w, Home::Local, true, empty(), empty()) == seq![97]
    }),
{
    reveal_with_fuel(go, 5);
    reveal_with_fuel(only_main_reads, 5);
    reveal_with_fuel(any_cond_ir, 5);
}

/// A read BEFORE the declaration reads an uninitialised `let` (the renderer
/// errors) where bash reads the unset cell as "".
proof fn read_before_write_is_load_bearing()
    ensures ({
        let w = seq![Ev::CondReadMain, Ev::TopWrite];
        hoist(w) == Home::Global
            && go(w, Home::Global, true, empty(), empty()) == Seq::<int>::empty()
            && go(w, Home::Local, true, empty(), empty()) == seq![45]
            && go(w, Home::Local, true, empty(), empty())
                != go(w, Home::Global, true, empty(), empty())
    }),
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(only_main_reads, 4);
    reveal_with_fuel(any_cond_ir, 4);
    assert(!first_is_top(w_seq()));
}

pub open spec fn w_seq() -> Seq<Ev> {
    seq![Ev::CondReadMain, Ev::TopWrite]
}

/// A self-referential RHS reads the binding it defines: uninitialised in the
/// `let`, empty in the cell.
proof fn self_reference_is_load_bearing()
    ensures ({
        let w = seq![Ev::SelfWrite, Ev::CondReadMain];
        hoist(w) == Home::Global
            && go(w, Home::Global, true, empty(), empty()) == seq![97]
            && go(w, Home::Local, true, empty(), empty()) == seq![45, 97]
            && go(w, Home::Local, true, empty(), empty())
                != go(w, Home::Global, true, empty(), empty())
    }),
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(only_main_reads, 4);
    reveal_with_fuel(any_cond_ir, 4);
}

/// A condition read that exists only as WORD TEXT gives the renderers no
/// binding at all (measured E0425), so the hoist must not fire on it: with
/// the binding suppressed, the rewritten program prints the unbound marker
/// where bash prints the value.
proof fn word_only_condition_read_is_load_bearing()
    ensures !bindable(seq![Ev::TopWrite, Ev::WordCondMain]),
        hoist(seq![Ev::TopWrite, Ev::WordCondMain]) == Home::Global,
        go(seq![Ev::TopWrite, Ev::WordCondMain], Home::Global, true, empty(), empty()) == seq![97],
        go(seq![Ev::TopWrite, Ev::WordCondMain], Home::Local, false, empty(), empty()) == seq![45],
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(only_main_reads, 4);
    reveal_with_fuel(any_cond_ir, 4);
    reveal_with_fuel(any_ir, 4);
}

/// The repaired shape: a word operand BESIDE an IR-visible condition read is
/// rewritten, and both homes print the same bytes (the shape the real
/// pipeline renders and runs).
proof fn word_read_beside_ir_read_is_hoisted()
    ensures ({
        let w = seq![Ev::TopWrite, Ev::CondReadMain, Ev::WordCondMain];
        eligible(w)
            && hoist(w) == Home::Local
            && go(w, hoist(w), bindable(w), empty(), empty()) == seq![97, 97]
            && go(w, Home::Local, bindable(w), empty(), empty())
                == go(w, Home::Global, bindable(w), empty(), empty())
    }),
{
    reveal_with_fuel(go, 5);
    reveal_with_fuel(only_main_reads, 5);
    reveal_with_fuel(any_cond_ir, 5);
    reveal_with_fuel(any_ir, 5);
}

/// Mutant trap for the whole-shape law: hoisting a program whose conditional
/// read is not IR-visible (the pre-repair pass) is refuted by the shapes
/// themselves, not by a parse error.
proof fn permissive_hoist_is_not_a_law()
    ensures hoist(seq![Ev::TopWrite, Ev::WordCondMain]) == Home::Global,
        go(seq![Ev::TopWrite, Ev::WordCondMain], Home::Local, false, empty(), empty()) == seq![45],
        go(seq![Ev::TopWrite, Ev::WordCondMain], Home::Global, false, empty(), empty()) == seq![97],
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(only_main_reads, 4);
    reveal_with_fuel(any_cond_ir, 4);
    reveal_with_fuel(any_ir, 4);
    assert(go(seq![Ev::TopWrite, Ev::WordCondMain], Home::Local, false, empty(), empty())
        == seq![45]);
    assert(go(seq![Ev::TopWrite, Ev::WordCondMain], Home::Global, false, empty(), empty())
        == seq![97]);
}

// ---------- the law, instantiated at every witness ----------

pub open spec fn w_sub() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::ReadOther]
}

pub open spec fn w_env() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::EnvRead]
}

pub open spec fn w_second() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::SecondWrite, Ev::ReadMain]
}

pub open spec fn w_dynamic() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::Dynamic, Ev::ReadMain]
}

pub open spec fn w_sub_law() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::CondReadMain, Ev::ReadOther]
}

pub open spec fn w_env_law() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::CondReadMain, Ev::EnvRead]
}

pub open spec fn w_second_law() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::SecondWrite, Ev::CondReadMain]
}

pub open spec fn w_dynamic_law() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::Dynamic, Ev::CondReadMain]
}

pub open spec fn w_self() -> Seq<Ev> {
    seq![Ev::SelfWrite, Ev::CondReadMain]
}

pub open spec fn w_word() -> Seq<Ev> {
    seq![Ev::TopWrite, Ev::WordCondMain]
}

/// The law evaluated at every refusal witness, with NO lemma in between: a
/// census that lets any of these through makes one conjunct false here, so
/// the failure is pinned on the law itself and not only on the agreement
/// lemma it consumes.
proof fn law_holds_at_every_witness()
    ensures
        go(w_sub(), hoist(w_sub()), bindable(w_sub()), empty(), empty())
            == go(w_sub(), Home::Global, bindable(w_sub()), empty(), empty()),
        go(w_env(), hoist(w_env()), bindable(w_env()), empty(), empty())
            == go(w_env(), Home::Global, bindable(w_env()), empty(), empty()),
        go(w_second(), hoist(w_second()), bindable(w_second()), empty(), empty())
            == go(w_second(), Home::Global, bindable(w_second()), empty(), empty()),
        go(w_dynamic(), hoist(w_dynamic()), bindable(w_dynamic()), empty(), empty())
            == go(w_dynamic(), Home::Global, bindable(w_dynamic()), empty(), empty()),
        go(w_seq(), hoist(w_seq()), bindable(w_seq()), empty(), empty())
            == go(w_seq(), Home::Global, bindable(w_seq()), empty(), empty()),
        go(w_self(), hoist(w_self()), bindable(w_self()), empty(), empty())
            == go(w_self(), Home::Global, bindable(w_self()), empty(), empty()),
        go(w_word(), hoist(w_word()), bindable(w_word()), empty(), empty())
            == go(w_word(), Home::Global, bindable(w_word()), empty(), empty()),
        go(w_sub_law(), hoist(w_sub_law()), bindable(w_sub_law()), empty(), empty())
            == go(w_sub_law(), Home::Global, bindable(w_sub_law()), empty(), empty()),
        go(w_env_law(), hoist(w_env_law()), bindable(w_env_law()), empty(), empty())
            == go(w_env_law(), Home::Global, bindable(w_env_law()), empty(), empty()),
        go(w_second_law(), hoist(w_second_law()), bindable(w_second_law()), empty(), empty())
            == go(w_second_law(), Home::Global, bindable(w_second_law()), empty(), empty()),
        go(w_dynamic_law(), hoist(w_dynamic_law()), bindable(w_dynamic_law()), empty(), empty())
            == go(w_dynamic_law(), Home::Global, bindable(w_dynamic_law()), empty(), empty()),
{
    reveal_with_fuel(go, 5);
    reveal_with_fuel(only_main_reads, 5);
    reveal_with_fuel(any_cond_ir, 5);
    reveal_with_fuel(any_ir, 5);
    assert(hoist(w_sub()) == Home::Global);
    assert(hoist(w_env()) == Home::Global);
    assert(hoist(w_second()) == Home::Global);
    assert(hoist(w_dynamic()) == Home::Global);
    assert(hoist(w_seq()) == Home::Global);
    assert(hoist(w_self()) == Home::Global);
    assert(hoist(w_word()) == Home::Global);
    assert(hoist(w_sub_law()) == Home::Global);
    assert(hoist(w_env_law()) == Home::Global);
    assert(hoist(w_second_law()) == Home::Global);
    assert(hoist(w_dynamic_law()) == Home::Global);
}

fn main() {}

} // verus!
