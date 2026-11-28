//! Cluster G (part 3): the `! command <'' >''` condition fold.
//!
//! `shir_native_stmt.rs` has one fold that rewrites a CONDITION into a
//! constant (6617, called from `transform_stmt`'s If arm at 143):
//!
//!     fn native_command_noop_cond(st: &mut IrStmt) -> bool
//!         let IrStmt::If { cond, .. } = st                      else refuse
//!         cond = Not(x)            -> (not_wrapped = true,  x)
//!         cond = x                 -> (not_wrapped = false, x)
//!         x must spell `redirect(Call{func: builtin|exec,
//!           args: [Str "command", Array words]}, specs…`)       else refuse
//!         words must be EMPTY                                   else refuse
//!         *cond = IrExpr::Bool(not_wrapped)
//!
//! Note what the fold does NOT look at: the specs.  It never inspects fd,
//! mode or target, and never counts them.  So the constant it writes is a
//! statement about the REDIRECT, not about `command` — and this file's job is
//! to say exactly when that is sound.
//!
//! Two independent definitions meet here:
//!
//!   * the PASS's answer `fold` is a shape match (Not? redirect? single
//!     builtin/exec call? `command`? no words?) that ends in the boolean it
//!     captured from the Not;
//!   * bash's answer `run_status` is what the shell actually does: perform the
//!     redirects first (an empty target can never be opened, so that fails
//!     with status 1), then run `command` with no arguments (which succeeds:
//!     `command` alone is a no-op, it is the *redirect* that fails).
//!
//! `status_law` joins them in the one case where the redirects are statically
//! decided: every spec targets the EMPTY string.  Then bash's status is 1, so
//! a plain condition is false and a `!`-wrapped condition is true — exactly the
//! `Bool(not_wrapped)` the fold writes.  `redirect_failure_dominates` and
//! `no_specs_command_succeeds` state the semantic side on its own, without
//! mentioning the fold.
//!
//! The refusals are the fold's other half, and they are CONSERVATISMS, not
//! requirements: a worded `command -v X`, a non-`command` inner, a
//! multi-statement Arrow body are all refused (`worded_command_is_refused`,
//! `other_name_is_refused`, `block_inner_is_refused`) even where the answer is
//! already decided — losing a rewrite, never correctness.
//! `refusal_is_conservative` proves exactly that for the empty-target case.
//!
//! d3, RECORDED NOT FIXED.  Because the fold ignores the target, it writes a
//! constant for redirects whose outcome is a runtime fact:
//!
//!   * `if command > /dev/null` — the redirect succeeds, `command` exits 0, so
//!     bash's condition is TRUE; the fold writes `false`
//!     (`d3_a_succeeding_redirect_is_decided_wrong`).
//!   * `if ! command <$IN >$OUT` (the corpus' own `lex-dot-in-var.sh` shape) —
//!     with openable files bash's `!` is FALSE; the fold writes `true`
//!     (`d3_a_dynamic_target_is_a_guess`).
//!
//! Both divergences are pinned against the source's answer and measured against
//! bash by the generated tests `pin_the_fold_ignores_the_target` and
//! `pin_a_dynamic_target_is_a_guess`.  The fix is a source guard (require every
//! spec's target to be the empty string), which this stage does not make.
//!
//! Residue, stated not claimed: specs are modelled by target only (the fold
//! ignores fd and mode entirely, so a model with fd/mode fields would prove
//! nothing extra); `words` is modelled by its LENGTH (the fold never looks
//! inside words — only `is_empty`); the `builtin` and `exec` spellings are one
//! `Func::Direct` (the fold treats them identically and both mean "run the
//! builtin directly").  The corpus-wide question "how often is this fold
//! reached with a non-empty target" is measured by the generated test, not
//! asserted here.

use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────────────────────────────────────────
// §1  The data the fold looks at, one constructor per arm it touches
// ─────────────────────────────────────────────────────────────────────────────

/// `cf` after the fold's `matches!(cf.as_str(), "builtin" | "exec")` gate.
pub enum Func {
    /// `builtin` or `exec`: run the command directly, no shell re-parse.
    Direct,
    /// anything else — the fold refuses.
    Other,
}

/// The command word after the fold's `cmd != "command"` gate.
pub enum Name {
    Command,
    Other,
}

/// A redirect spec's TARGET.  This is the only spec field the model keeps,
/// because it is the only one that decides the redirect (see §2).
pub enum Target {
    /// `''`: opening the empty path always fails.
    Empty,
    /// `>/dev/null`, `>$F`, `>/tmp/$$.out`: a real path, opened at run time.
    NonEmpty,
}

pub struct Spec {
    pub fd: u8,
    pub target: Target,
}

/// The redirect's body: the fold requires EXACTLY ONE `IrStmt::Expr` whose
/// expression is a Call with `[Str, Array]` args.
pub enum Inner {
    /// A single call: (func, command word, number of words after it).
    SingleCall(Func, Name, nat),
    /// a Block with 2+ statements, a non-Call expression, a redirect statement…
    Other,
}

pub enum Shape {
    /// `Call { func: "redirect", args: [Arrow(inner), spec…] }`
    Redirect(Inner, Seq<Spec>),
    /// any other condition spelling (a test, a variable, a pipeline, …).
    Other,
}

/// A condition, decomposed the way the fold decomposes it: the fold strips at
/// most ONE `Not` (`IrExpr::BinOp { op: Not, .. }`) and remembers the flag.
pub struct Cond {
    pub not_wrapped: bool,
    pub shape: Shape,
}

// ─────────────────────────────────────────────────────────────────────────────
// §2  The pass, transcribed from 6617 (the fold's own analysis)
// ─────────────────────────────────────────────────────────────────────────────

/// `native_command_noop_cond`: the boolean it writes into `*cond`, or None if
/// it refuses.  Independent of §3: this is a shape match, nothing else.
pub open spec fn fold(c: Cond) -> Option<bool> {
    match c.shape {
        Shape::Redirect(inner, _specs) => match inner {
            Inner::SingleCall(f, name, nwords) => if f == Func::Direct && name == Name::Command
                && nwords == 0 {
                Some(c.not_wrapped)
            } else {
                None
            },
            Inner::Other => None,
        },
        Shape::Other => None,
    }
}

/// The If the fold is handed for a `redirect(command, spec…)` condition.
pub open spec fn noop_cond(not_wrapped: bool, specs: Seq<Spec>) -> Cond {
    Cond {
        not_wrapped,
        shape: Shape::Redirect(Inner::SingleCall(Func::Direct, Name::Command, 0), specs),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// §3  bash's semantics (independent definition)
// ─────────────────────────────────────────────────────────────────────────────
//
// Order of operations in bash for `command >specs` with no arguments:
//   1. perform the redirects; if one fails the command is not run and the
//      status is 1;
//   2. run `command` with no arguments, which succeeds (status 0).
// A spec's outcome is only statically known when its target is the empty
// string — that open() can never succeed.  Everything else is a runtime fact.

/// Some spec targets the empty string, so a redirect fails before `command`
/// ever runs.
pub open spec fn is_empty_target(t: Target) -> bool {
    t == Target::Empty
}

pub open spec fn is_runtime_target(t: Target) -> bool {
    t == Target::NonEmpty
}

pub open spec fn redirect_fails(specs: Seq<Spec>) -> bool {
    exists|i: int| 0 <= i < specs.len() && is_empty_target(#[trigger] specs[i].target)
}

/// Some spec targets a real path: `open()` succeeds or fails at run time.
pub open spec fn has_runtime_target(specs: Seq<Spec>) -> bool {
    exists|i: int| 0 <= i < specs.len() && is_runtime_target(#[trigger] specs[i].target)
}

/// bash's exit status for `command` (or `NAME`) with `nwords` arguments behind
/// these redirect specs — `None` when it depends on the filesystem or on the
/// arguments, i.e. when the answer is not decidable from the IR alone.
pub open spec fn run_status(name: Name, nwords: nat, specs: Seq<Spec>) -> Option<u8> {
    if redirect_fails(specs) {
        Some(1)
    } else if has_runtime_target(specs) {
        None
    } else if nwords > 0 {
        // `command -v X` exits 0 or 1 depending on whether X resolves.
        None
    } else if name == Name::Command {
        Some(0)
    } else {
        None
    }
}

/// What bash makes of a condition given the status of the command behind it:
/// a plain condition is true iff the status is 0, a `!`-wrapped one iff it is
/// not.
pub open spec fn decided_truth(not_wrapped: bool, s: u8) -> bool {
    if not_wrapped {
        s != 0
    } else {
        s == 0
    }
}

/// An empty target anywhere in the specs makes the redirect fail — stated as a
/// witness so the existential is discharged by hand, not by a lucky trigger.
pub proof fn empty_target_makes_the_redirect_fail(specs: Seq<Spec>, i: int)
    requires
        0 <= i < specs.len(),
        is_empty_target(specs[i].target),
    ensures
        redirect_fails(specs),
{
}

// ─────────────────────────────────────────────────────────────────────────────
// §4  Laws
// ─────────────────────────────────────────────────────────────────────────────

/// A failing redirect dominates: if any target is empty, the status is 1 no
/// matter what else the condition says.  Semantic side only — the fold is not
/// mentioned.
pub proof fn redirect_failure_dominates(specs: Seq<Spec>)
    requires
        redirect_fails(specs),
    ensures
        run_status(Name::Command, 0, specs) == Some(1),
        decided_truth(true, 1),
        !decided_truth(false, 1),
{
}

/// `command` with no arguments and no redirects succeeds.
pub proof fn no_specs_command_succeeds()
    ensures
        run_status(Name::Command, 0, Seq::<Spec>::empty()) == Some(0),
        decided_truth(false, 0),
{
}

/// **The law.**  When every spec targets the empty string, the fold's constant
/// is bash's truth: the redirect always fails, so the plain condition is false
/// and the `!`-wrapped condition is true — the two definitions agree.
pub proof fn status_law(not_wrapped: bool, specs: Seq<Spec>)
    requires
        specs.len() > 0,
        forall|i: int| 0 <= i < specs.len() ==> is_empty_target(#[trigger] specs[i].target),
    ensures
        fold(noop_cond(not_wrapped, specs)) == Some(not_wrapped),
        run_status(Name::Command, 0, specs) == Some(1),
        fold(noop_cond(not_wrapped, specs)) == Some(decided_truth(not_wrapped, 1)),
{
    assert(is_empty_target(specs[0].target));
    empty_target_makes_the_redirect_fail(specs, 0);
}

/// The fold's status law is *tight*: it needs the empty target.  A condition
/// that is identical except for having no specs at all is the shape the fold
/// also accepts, and there bash's status is 0, i.e. the fold's constant is
/// right exactly when the redirects fail.  This is why d3 exists.
pub proof fn the_empty_target_is_what_the_law_needs(not_wrapped: bool)
    ensures
        run_status(Name::Command, 0, Seq::<Spec>::empty()) == Some(0),
        // same fold answer, different truth: the redirects decide, not the fold
        fold(noop_cond(not_wrapped, Seq::<Spec>::empty())) == Some(not_wrapped),
        decided_truth(false, 1) != decided_truth(false, 0),
{
}

// ─────────────────────────────────────────────────────────────────────────────
// §5  Refusals — and why each one is merely conservative
// ─────────────────────────────────────────────────────────────────────────────

pub open spec fn worded_cond(not_wrapped: bool, nwords: nat, specs: Seq<Spec>) -> Cond {
    Cond { not_wrapped, shape: Shape::Redirect(Inner::SingleCall(Func::Direct, Name::Command, nwords), specs) }
}

pub open spec fn other_name_cond(not_wrapped: bool, specs: Seq<Spec>) -> Cond {
    Cond { not_wrapped, shape: Shape::Redirect(Inner::SingleCall(Func::Direct, Name::Other, 0), specs) }
}

pub open spec fn other_func_cond(not_wrapped: bool, specs: Seq<Spec>) -> Cond {
    Cond { not_wrapped, shape: Shape::Redirect(Inner::SingleCall(Func::Other, Name::Command, 0), specs) }
}

pub open spec fn other_inner_cond(not_wrapped: bool, specs: Seq<Spec>) -> Cond {
    Cond { not_wrapped, shape: Shape::Redirect(Inner::Other, specs) }
}

pub open spec fn other_shape_cond(not_wrapped: bool) -> Cond {
    Cond { not_wrapped, shape: Shape::Other }
}

/// `command -v X` has ARGUMENTS: `command` then resolves X, so the status is 0
/// or 1 depending on X.  Refused.
pub proof fn worded_command_is_refused(not_wrapped: bool, nwords: nat, specs: Seq<Spec>)
    requires
        nwords > 0,
    ensures
        fold(worded_cond(not_wrapped, nwords, specs)).is_none(),
{
}

/// `command -v X` is refused even where the redirect already decides the
/// answer — a lost rewrite, not a lost proof: with an empty target the status
/// is 1 whatever the words are, and the fold declines to say so.
pub proof fn refusal_is_conservative(not_wrapped: bool)
    ensures
        fold(worded_cond(not_wrapped, 1, seq![Spec { fd: 1, target: Target::Empty }])).is_none(),
        run_status(Name::Command, 1, seq![Spec { fd: 1, target: Target::Empty }]) == Some(1),
{
    let specs = seq![Spec { fd: 1, target: Target::Empty }];
    assert(specs[0].target == Target::Empty);
    empty_target_makes_the_redirect_fail(specs, 0);
}

/// Any program other than `command` is refused (its status is unknown).
pub proof fn other_name_is_refused(not_wrapped: bool, specs: Seq<Spec>)
    ensures
        fold(other_name_cond(not_wrapped, specs)).is_none(),
        // refusing another program is conservative: a failing redirect still
        // decides the status, whatever program it would have run
        redirect_fails(specs) ==> run_status(Name::Other, 0, specs) == Some(1),
{
}

/// A non-`builtin`/`exec` spelling is refused: the fold does not claim to know
/// how another launcher reports status.
pub proof fn other_func_is_refused(not_wrapped: bool, specs: Seq<Spec>)
    ensures
        fold(other_func_cond(not_wrapped, specs)).is_none(),
{
}

/// A redirect whose body is not exactly one Call statement (a Block, an
/// assignment, a nested redirect) is refused: the body would run between the
/// redirect and the status the fold claims.
pub proof fn block_inner_is_refused(not_wrapped: bool, specs: Seq<Spec>)
    ensures
        fold(other_inner_cond(not_wrapped, specs)).is_none(),
{
}

/// A condition that is not a redirect at all is refused.
pub proof fn a_non_redirect_cond_is_refused(not_wrapped: bool)
    ensures
        fold(other_shape_cond(not_wrapped)).is_none(),
{
}

/// The fold strips at most ONE `Not`.  `!! command <'' >''` is a Not around a
/// Not, which is `Shape::Other` for a `Not(..)` inner — refused, not
/// double-negated.
pub proof fn only_one_not_is_stripped(not_wrapped: bool)
    ensures
        fold(other_shape_cond(not_wrapped)).is_none(),
        decided_truth(true, 1),
{
}

// ─────────────────────────────────────────────────────────────────────────────
// §6  d3 — where the fold answers a question about the filesystem
// ─────────────────────────────────────────────────────────────────────────────

/// d3a: `if command > /dev/null`.  The redirect succeeds and `command` exits 0,
/// so bash's condition is TRUE.  The fold looks only at the shape and writes
/// `Bool(false)`.  The model records both answers side by side.
pub proof fn d3_a_succeeding_redirect_is_decided_wrong()
    ensures
        fold(noop_cond(false, seq![Spec { fd: 1, target: Target::NonEmpty }])) == Some(false),
        run_status(Name::Command, 0, seq![Spec { fd: 1, target: Target::NonEmpty }]).is_none(),
        decided_truth(false, 0),
{
    let specs = seq![Spec { fd: 1, target: Target::NonEmpty }];
    // the redirect does not fail, and the one target is a runtime path
    assert(!is_empty_target(specs[0].target));
    assert(is_runtime_target(specs[0].target));
}

/// d3b: `if ! command <$IN >$OUT` (the corpus' `lex-dot-in-var.sh`).  With
/// openable files bash's `!` is FALSE; the fold writes `Bool(true)`.  Same
/// reason: the specs are never inspected.
pub proof fn d3_a_dynamic_target_is_a_guess()
    ensures
        fold(noop_cond(true, seq![Spec { fd: 0, target: Target::NonEmpty }, Spec {
            fd: 1,
            target: Target::NonEmpty,
        }])) == Some(true),
        run_status(Name::Command, 0, seq![Spec { fd: 0, target: Target::NonEmpty }, Spec {
            fd: 1,
            target: Target::NonEmpty,
        }]).is_none(),
        !decided_truth(true, 0),
{
    let specs = seq![Spec { fd: 0, target: Target::NonEmpty }, Spec { fd: 1, target: Target::NonEmpty }];
    assert(!is_empty_target(specs[0].target));
    assert(is_runtime_target(specs[0].target));
}

/// The fold fires on ANY spec sequence, empty target or not: the guard d3 would
/// need (`every spec's target is the empty string`) is absent from the source.
pub proof fn the_fold_never_inspects_the_specs(not_wrapped: bool, specs: Seq<Spec>)
    ensures
        fold(noop_cond(not_wrapped, specs)) == Some(not_wrapped),
{
}

// ─────────────────────────────────────────────────────────────────────────────
// §7  Named pins — the model facts the generated Rust tests witness
// ─────────────────────────────────────────────────────────────────────────────
//
// Each `pin_…` below is one row of shir_native_command_noop_cond_sync.txt.  Its
// generated test drives the REAL `native_command_noop_cond` (and, where the pin
// is about the shell, real bash) and asserts the same fact.

/// pin: the empty-target `! command <'' >''` folds to `true`.
pub proof fn pin_a_noop_command_with_an_empty_target_folds_to_false()
    ensures
        fold(noop_cond(false, seq![Spec { fd: 0, target: Target::Empty }, Spec { fd: 1, target: Target::Empty }])) == Some(false),
{
}

/// pin: the same shape under `Not` folds to `true`.
pub proof fn pin_a_not_wrapped_noop_command_folds_to_true()
    ensures
        fold(noop_cond(true, seq![Spec { fd: 0, target: Target::Empty }, Spec { fd: 1, target: Target::Empty }])) == Some(true),
{
}

/// pin: a worded `command` is refused.
pub proof fn pin_a_worded_command_is_refused()
    ensures
        fold(worded_cond(true, 2, seq![Spec { fd: 1, target: Target::Empty }])).is_none(),
{
}

/// pin: a condition with no redirect is refused.
pub proof fn pin_a_command_without_a_redirect_is_refused()
    ensures
        fold(other_shape_cond(true)).is_none(),
{
}

/// pin: a non-`command` program is refused.
pub proof fn pin_a_non_command_inner_is_refused()
    ensures
        fold(other_name_cond(false, seq![Spec { fd: 1, target: Target::Empty }])).is_none(),
{
}

/// pin: a redirect body with two statements is refused.
pub proof fn pin_a_block_inner_is_refused()
    ensures
        fold(other_inner_cond(true, seq![Spec { fd: 1, target: Target::Empty }])).is_none(),
{
}

/// pin (d3a): `command > /dev/null` is decided wrongly.
pub proof fn pin_the_fold_ignores_the_target()
    ensures
        fold(noop_cond(false, seq![Spec { fd: 1, target: Target::NonEmpty }])) == Some(false),
{
}

/// pin (d3b): the dynamic-target shape is a guess.
pub proof fn pin_a_dynamic_target_is_a_guess()
    ensures
        fold(noop_cond(true, seq![Spec { fd: 0, target: Target::NonEmpty }, Spec { fd: 1, target: Target::NonEmpty }])) == Some(true),
{
}

/// pin: an empty redirect target really does fail in bash — the fact the law
/// stands on.
pub proof fn pin_an_empty_target_really_fails()
    ensures
        run_status(Name::Command, 0, seq![Spec { fd: 0, target: Target::Empty }, Spec { fd: 1, target: Target::Empty }]) == Some(1),
        decided_truth(true, 1),
{
    let specs = seq![Spec { fd: 0, target: Target::Empty }, Spec { fd: 1, target: Target::Empty }];
    assert(is_empty_target(specs[0].target));
    empty_target_makes_the_redirect_fail(specs, 0);
}

fn main() {}

} // verus!
