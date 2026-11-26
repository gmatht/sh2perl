// ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/status_reads_traversal.rs — the OWED hypothesis of
//  `status_reads.rs`: its three per-statement facts really are the may/must
//  approximations the liveness machine consumes.
//
//  `proofs/verus/status_reads.rs` proves the `$?` store-liveness law UNDER
//  three facts it takes as given (`reads`, `stores`, `nested`). That file's
//  header says so explicitly: "that `stmt_reads_status_in` is a
//  may-over-approximation of the `$?` occurrences in a statement is a
//  syntax-traversal property over ~30 `IrStmt`/`IrExpr` variants … an
//  argument, not a proof."  This file is that proof, over a model of the
//  grammar with one constructor per arm the source's matches touch:
//
//    READS   `stmt_reads_status_in(s, fns) == false` ⇒ `s` has no `$?`
//            occurrence (`reads_complete_s`), and in fact the two agree
//            exactly (`reads_exact_s`), so an OVER-claim is caught too.
//            `fns` models the source's "descend into function bodies" flag:
//            the `program_reads_status` direction uses true, the per-body
//            liveness direction uses false and analyses each body separately.
//
//    STORES  `stmt_definitely_stores_status(s)` ⇒ the statement's own
//            emitted fragment writes the register (`stores_is_must`). The
//            right-hand predicate is written from the RENDERER's placement
//            (`emit_s`), not from the analysis, which is what makes it a
//            refinement and not a restatement: an arm where the analysis
//            trusts a shape the renderer does not write goes red (the
//            command-substitution assign is exactly that shape).
//
//    NESTED  a nested list that reads `$?` before its own first definite
//            store makes `stmt_nested_reads_before_store(s)` true
//            (`nested_complete`). The direction is one-way on purpose: the
//            source also counts a CONDITION read there, so it may over-claim
//            (keep more stores), never miss.
//
//  ── what is modelled, and what is not ───────────────────────────────────
//
//  * One constructor per source arm, with the arm's own conditions:
//    `Interp` carries literal parts (a text atom with its `$?` bit) and
//    expression parts; `Call` classifies the function (status-named,
//    `block`, `getVar("?")`, a capture spelling, the pure-print arms, plain);
//    `Asg` carries the target count and the `asm` flag; `ExitS` carries
//    "has an argument" (a BARE `exit` reads the current status).
//  * TEXT is atomic: `StrQ(q)` / `LitQ(q)` mean "this text contains `$?` or
//    not".  The model does not re-derive `str::contains`; it states the
//    property the analysis keys on, which is what the Debug-text fallback in
//    `program_reads_status` provides in practice.
//  * `has_cap` models `expr_has_capture`, which is TEXTUAL on the Debug form
//    (`"Capture"`, `"\"capture\""`, `"\"captureWords\""`, `"__sh_bash"`), so
//    the model treats a capture anywhere inside the expression, including
//    inside an `Arrow` body's statements, as a capture.
//  * `emit_s` models the instructions a statement contributes AT ITS OWN
//    LEVEL (children are separate statements): the renderer writes the
//    register for prints, writes, plain assigns, `exit`, and for the
//    control-flow constructs (bash's `if`/`while`/`for`/`case` set `$?` on
//    EVERY path — 0 when no branch runs), and does not for a capture-assign,
//    an `asm` assign, a multi-target assign, `try`, a subshell/block/
//    background/redirect/pipeline wrapper, or the pure-print call arms.
//  * Out of model, stated: the renderer actually emitting those instructions
//    (that is the backend contract, anchored end-to-end, not proved here);
//    the call-site machine for bodies (a per-body vector is the same scan,
//    see status_reads.rs); the Debug-text fallback's totality; and the
//    behaviour of `IrStmt`/`IrExpr` variants the source's `_ => false` arm
//    already declines to analyse (the model has `OtherX` / `OtherS` for
//    them, analysed as "nothing" exactly as the source does).
//  * KNOWN DIVERGENCE, reported rather than smoothed over: the `Function`
//    arm. The model scans the body AND its named blocks (`an_ss(b) ||
//    an_sss(named)`, and `flat_s` likewise), while the source's
//    `stmts_read_status_in` matches `IrStmt::Function { body, .. }` — the
//    named blocks are dropped by the `..`. That is the safe direction for
//    the liveness consumer (the model never calls a body with a reading
//    named block "status-free"), but it is NOT arm-for-arm: the shipped
//    analysis can miss a `$?` read inside a named block, and only
//    `program_reads_status`'s `$?`-in-Debug fallback catches that in
//    practice.
//
//  Run: verus --triggers-mode silent --rlimit 40 proofs/verus/status_reads_traversal.rs
// ─────────────────────────────────────────────────────────────────────────

//  ── measured mutants (2026-11-27 revision: 70 verified, 0 errors baseline) ──
//
//    run: verus --triggers-mode silent --rlimit 40 proofs/verus/status_reads_traversal.rs
//    (a) read walk drops the Cap/backtick arm ..... 69v/1e, at `reads_exact_x`
//    (b) "a literal text is never a read" ......... rejected at
//        simplification (`pin_bodies_are_localised` computes false: the
//        test-operand shape stops being a read at all)
//    (c) nested walk drops Redirect ................ rejected at
//        simplification (`pin_nested_redirect_reads_before_store`)
//    (d) the `If` arm called a must-store while `emit_s` stops writing the
//        register for an `If` fragment ............ rejected at
//        simplification (`pin_control_flow_stores` computes false) — the
//        arm is only sound because the renderer writes `$?` on every path
//
//  The simplification rejections are the repo's usual class (typeset_attrs
//  records the same for its leading-zero mutant): the concrete pins are
//  strong enough to refute the mutant before the verifier runs, so those
//  rows carry no verified/error count. Restored byte-identically after every
//  run (sha256 in `status_reads_traversal_sync.txt`).
//
use vstd::prelude::*;

verus! {

// ───────────────────────────────────────────────────────────── the grammar ──

pub enum Name {
    Q,
    Other,
}

pub enum Fn {
    /// `lastStatus` / `lastexit` / `lastExit` / `status` / `exitCode`
    Status,
    /// the `block` seam (its helper returns `__sh_rc == 0`)
    Block,
    /// `getVar("?")`
    GetVarQ,
    /// a capture spelling (`capture` / `captureWords`)
    Capture,
    /// the pure-print arms: no status store
    GrepMatches,
    PipeFn,
    RedirectFn,
    FnValue,
    /// anything else
    Plain,
}

pub enum Part {
    /// literal interpolation text, with its `$?` bit
    LitQ(bool),
    Ex(X),
}

pub enum X {
    Var(Name),
    /// a string literal, with its `$?` bit
    StrQ(bool),
    /// a raw expression text, with its `$?` bit
    RawQ(bool),
    /// the key operand
    Idx(Seq<X>),
    /// the two operands
    Bin(Seq<X>),
    Call(Fn, Seq<X>),
    /// the object, then the args (one child list, in evaluation order)
    Meth(Seq<X>),
    /// the three operands
    Tern(Seq<X>),
    Dor(Seq<X>),
    Interp(Seq<Part>),
    /// the substituted expression
    Cap(Seq<X>),
    /// a `$( … )` body in word position
    Arrow(Seq<S>),
    Arr(Seq<X>),
    OtherX,
}

pub enum S {
    Out(X),
    WFile(X, X),
    /// target index-lists, rhs, `asm`
    Asg(Seq<Seq<X>>, X, bool),
    /// has an initialiser, the initialiser
    Decl(bool, X),
    DeclArr(Seq<X>),
    ExprS(X),
    /// condition, then, elsifs (`(cond, body)`), else
    IfS(X, Seq<S>, Seq<(X, Seq<S>)>, Seq<S>),
    Wh(X, Seq<S>),
    ForS(X, Seq<S>),
    DoWh(Seq<S>, X),
    /// init list, condition, step list, body
    ForInit(Seq<S>, X, Seq<S>, Seq<S>),
    /// body, except bodies, else body, finally body
    TryS(Seq<S>, Seq<Seq<S>>, Seq<S>, Seq<S>),
    /// has an argument, the argument (a BARE `exit` reads the status)
    ExitS(bool, X),
    /// body, named blocks
    FnS(Seq<S>, Seq<Seq<S>>),
    Blk(Seq<S>),
    Subsh(Seq<S>),
    Bg(Seq<S>),
    Redir(Seq<S>),
    Pipe(Seq<Seq<S>>),
    /// discriminant, clause bodies
    CaseS(X, Seq<Seq<S>>),
    OtherS,
}

// ─────────────────────────────────────────────── the semantic side: atoms ──
//
// A statement/expression is flattened into the atoms its ANALYSIS must visit.
// This is deliberately a different recursion from the analysis: it collects,
// it does not short-circuit, so a walker that forgets an arm leaves an atom
// unvisited and the equality below fails.

pub enum Atom {
    AVar(Name),
    AText(bool),
    ARaw(bool),
    ACall(Fn),
    /// a direct status read (a bare `exit`)
    AStatus,
}

pub open spec fn is_status_fn(f: Fn) -> bool {
    match f {
        Fn::Status => true,
        Fn::Block => true,
        Fn::GetVarQ => true,
        _ => false,
    }
}

pub open spec fn is_capture_fn(f: Fn) -> bool {
    match f {
        Fn::Capture => true,
        _ => false,
    }
}

/// The call arms that do NOT write the status register (the source's
/// `expr_definitely_stores_status` non-storing list).
pub open spec fn nonstoring_fn(f: Fn) -> bool {
    match f {
        Fn::GrepMatches => true,
        Fn::PipeFn => true,
        Fn::RedirectFn => true,
        Fn::FnValue => true,
        _ => false,
    }
}

pub open spec fn is_read_atom(a: Atom) -> bool {
    match a {
        Atom::AVar(n) => n == Name::Q,
        Atom::AText(q) => q,
        Atom::ARaw(q) => q,
        Atom::ACall(f) => is_status_fn(f),
        Atom::AStatus => true,
    }
}

pub open spec fn any_read(a: Seq<Atom>) -> bool
    decreases a,
{
    if a.len() == 0 {
        false
    } else {
        is_read_atom(a[0]) || any_read(a.drop_first())
    }
}

pub proof fn any_read_cat(a: Seq<Atom>, b: Seq<Atom>)
    ensures any_read(a.add(b)) == (any_read(a) || any_read(b)),
    decreases a,
{
    reveal_with_fuel(any_read, 2);
    if a.len() == 0 {
        assert(a.add(b) == b);
    } else {
        assert(a.add(b).len() > 0);
        assert(a.add(b)[0] == a[0]);
        assert(a.add(b).drop_first() == a.drop_first().add(b));
        any_read_cat(a.drop_first(), b);
    }
}

// ───────────────────────────────────────────────────── the flattening ──────

pub open spec fn flat_p(ps: Seq<Part>, fns: bool) -> Seq<Atom>
    decreases ps,
{
    if ps.len() == 0 {
        Seq::empty()
    } else {
        (match ps[0] {
            Part::LitQ(q) => seq![Atom::AText(q)],
            Part::Ex(x) => flat_x(x, fns),
        }).add(flat_p(ps.drop_first(), fns))
    }
}

pub open spec fn flat_x(x: X, fns: bool) -> Seq<Atom>
    decreases x,
{
    match x {
        X::Var(n) => seq![Atom::AVar(n)],
        X::StrQ(q) => seq![Atom::AText(q)],
        X::RawQ(q) => seq![Atom::ARaw(q)],
        X::Idx(k) => flat_xs(k, fns),
        X::Bin(k) => flat_xs(k, fns),
        X::Call(f, args) => seq![Atom::ACall(f)].add(flat_xs(args, fns)),
        X::Meth(k) => flat_xs(k, fns),
        X::Tern(k) => flat_xs(k, fns),
        X::Dor(k) => flat_xs(k, fns),
        X::Interp(ps) => flat_p(ps, fns),
        X::Cap(k) => flat_xs(k, fns),
        X::Arrow(b) => flat_ss(b, fns),
        X::Arr(items) => flat_xs(items, fns),
        X::OtherX => Seq::empty(),
    }
}

pub open spec fn flat_xs(l: Seq<X>, fns: bool) -> Seq<Atom>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        flat_x(l[0], fns).add(flat_xs(l.drop_first(), fns))
    }
}

pub open spec fn flat_ts(l: Seq<Seq<X>>, fns: bool) -> Seq<Atom>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        flat_xs(l[0], fns).add(flat_ts(l.drop_first(), fns))
    }
}

pub open spec fn flat_ss(l: Seq<S>, fns: bool) -> Seq<Atom>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        flat_s(l[0], fns).add(flat_ss(l.drop_first(), fns))
    }
}

pub open spec fn flat_sss(l: Seq<Seq<S>>, fns: bool) -> Seq<Atom>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        flat_ss(l[0], fns).add(flat_sss(l.drop_first(), fns))
    }
}

pub open spec fn flat_els(l: Seq<(X, Seq<S>)>, fns: bool) -> Seq<Atom>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        flat_x(l[0].0, fns).add(flat_ss(l[0].1, fns)).add(flat_els(l.drop_first(), fns))
    }
}

pub open spec fn flat_s(s: S, fns: bool) -> Seq<Atom>
    decreases s,
{
    match s {
        S::Out(v) => flat_x(v, fns),
        S::WFile(p, c) => flat_x(p, fns).add(flat_x(c, fns)),
        S::Asg(ts, rhs, _asm) => flat_ts(ts, fns).add(flat_x(rhs, fns)),
        S::Decl(with_init, e) => if with_init { flat_x(e, fns) } else { Seq::empty() },
        S::DeclArr(es) => flat_xs(es, fns),
        S::ExprS(e) => flat_x(e, fns),
        S::IfS(c, t, ei, el) => flat_x(c, fns).add(flat_ss(t, fns)).add(flat_els(ei, fns)).add(flat_ss(el, fns)),
        S::Wh(c, b) => flat_x(c, fns).add(flat_ss(b, fns)),
        S::ForS(c, b) => flat_x(c, fns).add(flat_ss(b, fns)),
        S::DoWh(b, c) => flat_ss(b, fns).add(flat_x(c, fns)),
        S::ForInit(i, c, st, b) => flat_ss(i, fns).add(flat_x(c, fns)).add(flat_ss(st, fns)).add(flat_ss(b, fns)),
        S::TryS(b, ex, eb, fb) => flat_ss(b, fns).add(flat_sss(ex, fns)).add(flat_ss(eb, fns)).add(flat_ss(fb, fns)),
        S::ExitS(with_init, e) => if with_init { flat_x(e, fns) } else { seq![Atom::AStatus] },
        S::FnS(b, named) => if fns { flat_ss(b, fns).add(flat_sss(named, fns)) } else { Seq::empty() },
        S::Blk(l) => flat_ss(l, fns),
        S::Subsh(l) => flat_ss(l, fns),
        S::Bg(l) => flat_ss(l, fns),
        S::Redir(l) => flat_ss(l, fns),
        S::Pipe(st) => flat_sss(st, fns),
        S::CaseS(d, cs) => flat_x(d, fns).add(flat_sss(cs, fns)),
        S::OtherS => Seq::empty(),
    }
}

// ────────────────────────────────────── the analysis, transcribed from source ──
//
// `an_*` mirrors `expr_reads_status` / `stmt_reads_status_in` arm for arm.

pub open spec fn an_p(ps: Seq<Part>, fns: bool) -> bool
    decreases ps,
{
    if ps.len() == 0 {
        false
    } else {
        (match ps[0] {
            Part::LitQ(q) => q,
            Part::Ex(x) => an_x(x, fns),
        }) || an_p(ps.drop_first(), fns)
    }
}

pub open spec fn an_x(x: X, fns: bool) -> bool
    decreases x,
{
    match x {
        X::Var(n) => n == Name::Q,
        X::StrQ(q) => q,
        X::RawQ(q) => q,
        X::Idx(k) => an_xs(k, fns),
        X::Bin(k) => an_xs(k, fns),
        X::Call(f, args) => is_status_fn(f) || an_xs(args, fns),
        X::Meth(k) => an_xs(k, fns),
        X::Tern(k) => an_xs(k, fns),
        X::Dor(k) => an_xs(k, fns),
        X::Interp(ps) => an_p(ps, fns),
        X::Cap(k) => an_xs(k, fns),
        X::Arrow(b) => an_ss(b, fns),
        X::Arr(items) => an_xs(items, fns),
        X::OtherX => false,
    }
}

pub open spec fn an_xs(l: Seq<X>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        an_x(l[0], fns) || an_xs(l.drop_first(), fns)
    }
}

pub open spec fn an_ts(l: Seq<Seq<X>>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        an_xs(l[0], fns) || an_ts(l.drop_first(), fns)
    }
}

pub open spec fn an_ss(l: Seq<S>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        an_s(l[0], fns) || an_ss(l.drop_first(), fns)
    }
}

pub open spec fn an_sss(l: Seq<Seq<S>>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        an_ss(l[0], fns) || an_sss(l.drop_first(), fns)
    }
}

pub open spec fn an_els(l: Seq<(X, Seq<S>)>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        an_x(l[0].0, fns) || an_ss(l[0].1, fns) || an_els(l.drop_first(), fns)
    }
}

pub open spec fn an_s(s: S, fns: bool) -> bool
    decreases s,
{
    match s {
        S::Out(v) => an_x(v, fns),
        S::WFile(p, c) => an_x(p, fns) || an_x(c, fns),
        S::Asg(ts, rhs, _asm) => an_ts(ts, fns) || an_x(rhs, fns),
        S::Decl(with_init, e) => with_init && an_x(e, fns),
        S::DeclArr(es) => an_xs(es, fns),
        S::ExprS(e) => an_x(e, fns),
        S::IfS(c, t, ei, el) => an_x(c, fns) || an_ss(t, fns) || an_els(ei, fns) || an_ss(el, fns),
        S::Wh(c, b) => an_x(c, fns) || an_ss(b, fns),
        S::ForS(c, b) => an_x(c, fns) || an_ss(b, fns),
        S::DoWh(b, c) => an_ss(b, fns) || an_x(c, fns),
        S::ForInit(i, c, st, b) => an_ss(i, fns) || an_x(c, fns) || an_ss(st, fns) || an_ss(b, fns),
        S::TryS(b, ex, eb, fb) => an_ss(b, fns) || an_sss(ex, fns) || an_ss(eb, fns) || an_ss(fb, fns),
        S::ExitS(with_init, e) => if with_init { an_x(e, fns) } else { true },
        S::FnS(b, named) => fns && (an_ss(b, fns) || an_sss(named, fns)),
        S::Blk(l) => an_ss(l, fns),
        S::Subsh(l) => an_ss(l, fns),
        S::Bg(l) => an_ss(l, fns),
        S::Redir(l) => an_ss(l, fns),
        S::Pipe(st) => an_sss(st, fns),
        S::CaseS(d, cs) => an_x(d, fns) || an_sss(cs, fns),
        S::OtherS => false,
    }
}

// The may-predicates: "the flattened atoms contain a status read".
pub open spec fn may_x(x: X, fns: bool) -> bool {
    any_read(flat_x(x, fns))
}

pub open spec fn may_s(s: S, fns: bool) -> bool {
    any_read(flat_s(s, fns))
}

// ──────────────────────────────── LAW 1: the read walk visits every atom ────

pub proof fn reads_exact_ps(ps: Seq<Part>, fns: bool)
    ensures an_p(ps, fns) == any_read(flat_p(ps, fns)),
    decreases ps,
{
    reveal_with_fuel(an_p, 2);
    reveal_with_fuel(flat_p, 2);
    reveal_with_fuel(any_read, 2);
    if ps.len() > 0 {
        reads_exact_ps(ps.drop_first(), fns);
        match ps[0] {
            Part::LitQ(q) => {
                any_read_cat(seq![Atom::AText(q)], flat_p(ps.drop_first(), fns));
                assert(an_p(ps, fns) == any_read(flat_p(ps, fns)));
            }
            Part::Ex(x) => {
                reads_exact_x(x, fns);
                any_read_cat(flat_x(x, fns), flat_p(ps.drop_first(), fns));
                assert(an_p(ps, fns) == any_read(flat_p(ps, fns)));
            }
        }
    }
}

pub proof fn reads_exact_x(x: X, fns: bool)
    ensures an_x(x, fns) == may_x(x, fns),
    decreases x,
{
    reveal_with_fuel(an_x, 2);
    reveal_with_fuel(flat_x, 2);
    reveal_with_fuel(any_read, 2);
    match x {
        X::Var(n) => { assert(an_x(x, fns) == may_x(x, fns)); }
        X::StrQ(q) => { assert(an_x(x, fns) == may_x(x, fns)); }
        X::RawQ(q) => { assert(an_x(x, fns) == may_x(x, fns)); }
        X::Idx(k) => { reads_exact_xs(k, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Bin(k) => { reads_exact_xs(k, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Call(f, args) => {
            reads_exact_xs(args, fns);
            any_read_cat(seq![Atom::ACall(f)], flat_xs(args, fns));
            assert(an_x(x, fns) == may_x(x, fns));
        }
        X::Meth(k) => { reads_exact_xs(k, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Tern(k) => { reads_exact_xs(k, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Dor(k) => { reads_exact_xs(k, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Interp(ps) => { reads_exact_ps(ps, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Cap(k) => { reads_exact_xs(k, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Arrow(b) => { reads_exact_ss(b, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::Arr(items) => { reads_exact_xs(items, fns); assert(an_x(x, fns) == may_x(x, fns)); }
        X::OtherX => { assert(an_x(x, fns) == may_x(x, fns)); }
    }
}

pub proof fn reads_exact_xs(l: Seq<X>, fns: bool)
    ensures an_xs(l, fns) == any_read(flat_xs(l, fns)),
    decreases l,
{
    reveal_with_fuel(an_xs, 2);
    reveal_with_fuel(flat_xs, 2);
    reveal_with_fuel(any_read, 2);
    if l.len() > 0 {
        reads_exact_x(l[0], fns);
        reads_exact_xs(l.drop_first(), fns);
        any_read_cat(flat_x(l[0], fns), flat_xs(l.drop_first(), fns));
    }
}

pub proof fn reads_exact_ts(l: Seq<Seq<X>>, fns: bool)
    ensures an_ts(l, fns) == any_read(flat_ts(l, fns)),
    decreases l,
{
    reveal_with_fuel(an_ts, 2);
    reveal_with_fuel(flat_ts, 2);
    reveal_with_fuel(any_read, 2);
    if l.len() > 0 {
        reads_exact_xs(l[0], fns);
        reads_exact_ts(l.drop_first(), fns);
        any_read_cat(flat_xs(l[0], fns), flat_ts(l.drop_first(), fns));
    }
}

pub proof fn reads_exact_els(l: Seq<(X, Seq<S>)>, fns: bool)
    ensures an_els(l, fns) == any_read(flat_els(l, fns)),
    decreases l,
{
    reveal_with_fuel(an_els, 2);
    reveal_with_fuel(flat_els, 2);
    reveal_with_fuel(any_read, 2);
    if l.len() > 0 {
        reads_exact_x(l[0].0, fns);
        reads_exact_ss(l[0].1, fns);
        reads_exact_els(l.drop_first(), fns);
        any_read_cat(flat_x(l[0].0, fns), flat_ss(l[0].1, fns));
        any_read_cat(flat_x(l[0].0, fns).add(flat_ss(l[0].1, fns)), flat_els(l.drop_first(), fns));
    }
}

pub proof fn reads_exact_sss(l: Seq<Seq<S>>, fns: bool)
    ensures an_sss(l, fns) == any_read(flat_sss(l, fns)),
    decreases l,
{
    reveal_with_fuel(an_sss, 2);
    reveal_with_fuel(flat_sss, 2);
    reveal_with_fuel(any_read, 2);
    if l.len() > 0 {
        reads_exact_ss(l[0], fns);
        reads_exact_sss(l.drop_first(), fns);
        any_read_cat(flat_ss(l[0], fns), flat_sss(l.drop_first(), fns));
    }
}

pub proof fn reads_exact_ss(l: Seq<S>, fns: bool)
    ensures an_ss(l, fns) == any_read(flat_ss(l, fns)),
    decreases l,
{
    reveal_with_fuel(an_ss, 2);
    reveal_with_fuel(flat_ss, 2);
    reveal_with_fuel(any_read, 2);
    if l.len() > 0 {
        reads_exact_s(l[0], fns);
        reads_exact_ss(l.drop_first(), fns);
        any_read_cat(flat_s(l[0], fns), flat_ss(l.drop_first(), fns));
    }
}

pub proof fn reads_exact_s(s: S, fns: bool)
    ensures an_s(s, fns) == may_s(s, fns),
    decreases s,
{
    reveal_with_fuel(an_s, 2);
    reveal_with_fuel(flat_s, 2);
    reveal_with_fuel(any_read, 2);
    match s {
        S::Out(v) => reads_exact_x(v, fns),
        S::WFile(p, c) => {
            reads_exact_x(p, fns);
            reads_exact_x(c, fns);
            any_read_cat(flat_x(p, fns), flat_x(c, fns));
        }
        S::Asg(ts, rhs, _asm) => {
            reads_exact_ts(ts, fns);
            reads_exact_x(rhs, fns);
            any_read_cat(flat_ts(ts, fns), flat_x(rhs, fns));
        }
        S::Decl(with_init, e) => {
            if with_init {
                reads_exact_x(e, fns);
            }
        }
        S::DeclArr(es) => reads_exact_xs(es, fns),
        S::ExprS(e) => reads_exact_x(e, fns),
        S::IfS(c, t, ei, el) => {
            reads_exact_x(c, fns);
            reads_exact_ss(t, fns);
            reads_exact_els(ei, fns);
            reads_exact_ss(el, fns);
            any_read_cat(flat_x(c, fns), flat_ss(t, fns));
            any_read_cat(flat_x(c, fns).add(flat_ss(t, fns)), flat_els(ei, fns));
            any_read_cat(flat_x(c, fns).add(flat_ss(t, fns)).add(flat_els(ei, fns)), flat_ss(el, fns));
        }
        S::Wh(c, b) => {
            reads_exact_x(c, fns);
            reads_exact_ss(b, fns);
            any_read_cat(flat_x(c, fns), flat_ss(b, fns));
        }
        S::ForS(c, b) => {
            reads_exact_x(c, fns);
            reads_exact_ss(b, fns);
            any_read_cat(flat_x(c, fns), flat_ss(b, fns));
        }
        S::DoWh(b, c) => {
            reads_exact_ss(b, fns);
            reads_exact_x(c, fns);
            any_read_cat(flat_ss(b, fns), flat_x(c, fns));
        }
        S::ForInit(i, c, st, b) => {
            reads_exact_ss(i, fns);
            reads_exact_x(c, fns);
            reads_exact_ss(st, fns);
            reads_exact_ss(b, fns);
            any_read_cat(flat_ss(i, fns), flat_x(c, fns));
            any_read_cat(flat_ss(i, fns).add(flat_x(c, fns)), flat_ss(st, fns));
            any_read_cat(flat_ss(i, fns).add(flat_x(c, fns)).add(flat_ss(st, fns)), flat_ss(b, fns));
        }
        S::TryS(b, ex, eb, fb) => {
            reads_exact_ss(b, fns);
            reads_exact_sss(ex, fns);
            reads_exact_ss(eb, fns);
            reads_exact_ss(fb, fns);
            any_read_cat(flat_ss(b, fns), flat_sss(ex, fns));
            any_read_cat(flat_ss(b, fns).add(flat_sss(ex, fns)), flat_ss(eb, fns));
            any_read_cat(flat_ss(b, fns).add(flat_sss(ex, fns)).add(flat_ss(eb, fns)), flat_ss(fb, fns));
        }
        S::ExitS(with_init, e) => {
            if with_init {
                reads_exact_x(e, fns);
            }
        }
        S::FnS(b, named) => {
            if fns {
                reads_exact_ss(b, fns);
                reads_exact_sss(named, fns);
                any_read_cat(flat_ss(b, fns), flat_sss(named, fns));
            }
        }
        S::Blk(l) => reads_exact_ss(l, fns),
        S::Subsh(l) => reads_exact_ss(l, fns),
        S::Bg(l) => reads_exact_ss(l, fns),
        S::Redir(l) => reads_exact_ss(l, fns),
        S::Pipe(st) => reads_exact_sss(st, fns),
        S::CaseS(d, cs) => {
            reads_exact_x(d, fns);
            reads_exact_sss(cs, fns);
            any_read_cat(flat_x(d, fns), flat_sss(cs, fns));
        }
        S::OtherS => {}
    }
}

/// THE OBLIGATION (reads), as a false-negative law: what the analysis calls
/// "no reader" really has no read.
pub proof fn reads_complete_s(s: S, fns: bool)
    ensures !an_s(s, fns) ==> !may_s(s, fns),
{
    reads_exact_s(s, fns);
}

pub proof fn reads_complete_x(x: X, fns: bool)
    ensures !an_x(x, fns) ==> !may_x(x, fns),
{
    reads_exact_x(x, fns);
}

pub proof fn reads_complete_ss(l: Seq<S>, fns: bool)
    ensures !an_ss(l, fns) ==> !any_read(flat_ss(l, fns)),
{
    reads_exact_ss(l, fns);
}

/// The traversal plan's name for the read law: the analysis's "no reader" is
/// a may-over-approximation of the semantic read set, so `!analysis ⇒ !may`.
/// (It is the same obligation as `reads_complete_s`; this is the entry point
/// the consumer and the plan cite.)
pub proof fn reads_is_may_approx(s: S, fns: bool)
    ensures !an_s(s, fns) ==> !may_s(s, fns),
{
    reads_complete_s(s, fns);
}

// ──────────────────────────────── has_cap: `expr_has_capture` ───────────────

pub open spec fn has_cap_x(x: X) -> bool
    decreases x,
{
    match x {
        X::Var(_) => false,
        X::StrQ(_) => false,
        X::RawQ(_) => false,
        X::Idx(k) => has_cap_xs(k),
        X::Bin(k) => has_cap_xs(k),
        X::Call(f, args) => is_capture_fn(f) || has_cap_xs(args),
        X::Meth(k) => has_cap_xs(k),
        X::Tern(k) => has_cap_xs(k),
        X::Dor(k) => has_cap_xs(k),
        X::Interp(ps) => has_cap_ps(ps),
        X::Cap(_) => true,
        X::Arrow(b) => has_cap_ss(b),
        X::Arr(items) => has_cap_xs(items),
        X::OtherX => false,
    }
}

pub open spec fn has_cap_xs(l: Seq<X>) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        has_cap_x(l[0]) || has_cap_xs(l.drop_first())
    }
}

pub open spec fn has_cap_ps(l: Seq<Part>) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        (match l[0] {
            Part::LitQ(_) => false,
            Part::Ex(x) => has_cap_x(x),
        }) || has_cap_ps(l.drop_first())
    }
}

pub open spec fn has_cap_ss(l: Seq<S>) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        has_cap_s(l[0]) || has_cap_ss(l.drop_first())
    }
}

pub open spec fn has_cap_s(s: S) -> bool
    decreases s,
{
    match s {
        S::Out(v) => has_cap_x(v),
        S::WFile(p, c) => has_cap_x(p) || has_cap_x(c),
        S::Asg(ts, rhs, _) => has_cap_ts(ts) || has_cap_x(rhs),
        S::Decl(with_init, e) => with_init && has_cap_x(e),
        S::DeclArr(es) => has_cap_xs(es),
        S::ExprS(e) => has_cap_x(e),
        S::IfS(c, t, ei, el) => has_cap_x(c) || has_cap_ss(t) || has_cap_els(ei) || has_cap_ss(el),
        S::Wh(c, b) => has_cap_x(c) || has_cap_ss(b),
        S::ForS(c, b) => has_cap_x(c) || has_cap_ss(b),
        S::DoWh(b, c) => has_cap_ss(b) || has_cap_x(c),
        S::ForInit(i, c, st, b) => has_cap_ss(i) || has_cap_x(c) || has_cap_ss(st) || has_cap_ss(b),
        S::TryS(b, ex, eb, fb) => has_cap_ss(b) || has_cap_sss(ex) || has_cap_ss(eb) || has_cap_ss(fb),
        S::ExitS(with_init, e) => with_init && has_cap_x(e),
        S::FnS(b, named) => has_cap_ss(b) || has_cap_sss(named),
        S::Blk(l) => has_cap_ss(l),
        S::Subsh(l) => has_cap_ss(l),
        S::Bg(l) => has_cap_ss(l),
        S::Redir(l) => has_cap_ss(l),
        S::Pipe(st) => has_cap_sss(st),
        S::CaseS(d, cs) => has_cap_x(d) || has_cap_sss(cs),
        S::OtherS => false,
    }
}

pub open spec fn has_cap_ts(l: Seq<Seq<X>>) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        has_cap_xs(l[0]) || has_cap_ts(l.drop_first())
    }
}

pub open spec fn has_cap_sss(l: Seq<Seq<S>>) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        has_cap_ss(l[0]) || has_cap_sss(l.drop_first())
    }
}

pub open spec fn has_cap_els(l: Seq<(X, Seq<S>)>) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        has_cap_x(l[0].0) || has_cap_ss(l[0].1) || has_cap_els(l.drop_first())
    }
}

// ───────────────────── LAW 2: the "definitely stores" arms really write ─────
//
// `emit_s` is the statement's own emitted fragment, written from the
// RENDERER's register placement (see the header). `own_rc` asks whether that
// fragment writes the register: the predicate a later `$?` read depends on.

pub enum Ins {
    /// a status-register write
    IRc,
    /// a variable store (an assignment), no register write of its own
    IVar,
    /// a command substitution — renders through `__sh_bash`, leaves the
    /// register as it found it
    ICap,
    /// nothing observable
    INone,
    /// a branch (the construct's status is written by its own IRc, not here)
    IBranch,
}

pub open spec fn expr_ins(e: X) -> Seq<Ins> {
    match e {
        X::Call(f, _) => if nonstoring_fn(f) { seq![Ins::INone] } else { seq![Ins::IRc] },
        _ => seq![Ins::IRc],
    }
}

pub open spec fn emit_s(s: S) -> Seq<Ins> {
    match s {
        S::Out(_) => seq![Ins::IRc],
        S::WFile(_, _) => seq![Ins::IRc],
        S::Asg(ts, rhs, is_asm) => if is_asm {
            seq![Ins::IBranch]
        } else if ts.len() != 1 {
            seq![Ins::IBranch]
        } else if has_cap_x(rhs) {
            seq![Ins::ICap]
        } else {
            seq![Ins::IVar, Ins::IRc]
        },
        S::Decl(with_init, e) => if with_init && has_cap_x(e) { seq![Ins::ICap] } else { seq![Ins::IVar, Ins::IRc] },
        S::DeclArr(_) => seq![Ins::IVar, Ins::IRc],
        S::ExprS(e) => expr_ins(e),
        S::IfS(_, _, _, _) => seq![Ins::IRc, Ins::IBranch],
        S::Wh(_, _) => seq![Ins::IRc, Ins::IBranch],
        S::ForS(_, _) => seq![Ins::IRc, Ins::IBranch],
        S::DoWh(_, _) => seq![Ins::IRc, Ins::IBranch],
        S::ForInit(_, _, _, _) => seq![Ins::IRc, Ins::IBranch],
        S::CaseS(_, _) => seq![Ins::IRc, Ins::IBranch],
        S::ExitS(_, _) => seq![Ins::IRc],
        S::TryS(_, _, _, _) => seq![Ins::IBranch],
        S::Blk(_) => seq![Ins::IBranch],
        S::Subsh(_) => seq![Ins::IBranch],
        S::Bg(_) => seq![Ins::IBranch],
        S::Redir(_) => seq![Ins::IBranch],
        S::Pipe(_) => seq![Ins::IBranch],
        S::FnS(_, _) => seq![Ins::INone],
        S::OtherS => seq![Ins::INone],
    }
}

/// Does the fragment write the register? (A recursive scan, not an `exists`:
/// `compute` pins can evaluate this one.)
pub open spec fn has_rc(a: Seq<Ins>) -> bool
    decreases a,
{
    if a.len() == 0 {
        false
    } else {
        a[0] == Ins::IRc || has_rc(a.drop_first())
    }
}

pub open spec fn own_rc(s: S) -> bool {
    has_rc(emit_s(s))
}

/// The analysis's "definitely stores" predicate, transcribed from
/// `stmt_definitely_stores_status` / `expr_definitely_stores_status`.
pub open spec fn an_stores(s: S) -> bool {
    match s {
        S::Asg(ts, rhs, is_asm) => !is_asm && ts.len() == 1 && !has_cap_x(rhs),
        S::Decl(with_init, e) => !with_init || !has_cap_x(e),
        S::DeclArr(_) => true,
        S::Out(_) => true,
        S::WFile(_, _) => true,
        S::ExitS(_, _) => true,
        S::ExprS(e) => match e {
            X::Call(f, _) => !nonstoring_fn(f),
            _ => true,
        },
        S::IfS(_, _, _, _) => true,
        S::Wh(_, _) => true,
        S::ForS(_, _) => true,
        S::CaseS(_, _) => true,
        _ => false,
    }
}

pub proof fn stores_is_must(s: S)
    ensures an_stores(s) ==> own_rc(s),
{
    reveal_with_fuel(has_cap_x, 2);
    reveal_with_fuel(has_rc, 2);
    match s {
        S::Asg(ts, rhs, is_asm) => {
            if an_stores(s) {
                assert(emit_s(s) == seq![Ins::IVar, Ins::IRc]);
                assert(own_rc(s)) by {
                    assert(emit_s(s)[1] == Ins::IRc);
                }
            }
        }
        S::Decl(with_init, e) => {
            if an_stores(s) {
                if with_init && has_cap_x(e) {
                    assert(!an_stores(s));
                }
                assert(emit_s(s) == seq![Ins::IVar, Ins::IRc]);
                assert(own_rc(s)) by {
                    assert(emit_s(s)[1] == Ins::IRc);
                }
            }
        }
        S::DeclArr(_) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[1] == Ins::IRc);
            }
        }
        S::Out(_) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        S::WFile(_, _) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        S::ExitS(_, _) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        S::ExprS(e) => {
            if an_stores(s) {
                match e {
                    X::Call(f, _) => {
                        assert(!nonstoring_fn(f));
                        assert(expr_ins(e) == seq![Ins::IRc]);
                        assert(own_rc(s)) by {
                            assert(emit_s(s)[0] == Ins::IRc);
                        }
                    }
                    _ => {
                        assert(expr_ins(e) == seq![Ins::IRc]);
                        assert(own_rc(s)) by {
                            assert(emit_s(s)[0] == Ins::IRc);
                        }
                    }
                }
            }
        }
        S::IfS(_, _, _, _) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        S::Wh(_, _) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        S::ForS(_, _) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        S::CaseS(_, _) => {
            assert(own_rc(s)) by {
                assert(emit_s(s)[0] == Ins::IRc);
            }
        }
        _ => {
            assert(!an_stores(s));
        }
    }
}

// ───────────────── LAW 3: the nested read-before-store walk misses nothing ────

pub enum Ev {
    Read,
    Store,
}

/// The events a statement contributes TO ITS ENCLOSING LIST, in the source's
/// granularity: one Read if it reads `$?` anywhere, then one Store if it
/// definitely stores (that is `list_reads_before_store`'s view).
pub open spec fn stmt_evs(s: S, fns: bool) -> Seq<Ev> {
    (if may_s(s, fns) { seq![Ev::Read] } else { Seq::<Ev>::empty() }).add(
        if own_rc(s) { seq![Ev::Store] } else { Seq::<Ev>::empty() },
    )
}

pub open spec fn evs_l(l: Seq<S>, fns: bool) -> Seq<Ev>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        stmt_evs(l[0], fns).add(evs_l(l.drop_first(), fns))
    }
}

pub open spec fn evs_sss(l: Seq<Seq<S>>, fns: bool) -> Seq<Ev>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        evs_l(l[0], fns).add(evs_sss(l.drop_first(), fns))
    }
}

/// The nested lists' events of a statement (its own expressions excluded: a
/// read in the RHS of an assignment runs BEFORE that statement's store, which
/// is the separate `reads` fact, not this one).
pub open spec fn evs_s(s: S, fns: bool) -> Seq<Ev>
    decreases s,
{
    match s {
        S::IfS(_c, t, ei, el) => evs_l(t, fns).add(evs_els(ei, fns)).add(evs_l(el, fns)),
        S::Wh(_c, b) => evs_l(b, fns),
        S::ForS(_c, b) => evs_l(b, fns),
        S::DoWh(b, _c) => evs_l(b, fns),
        S::ForInit(i, _c, st, b) => evs_l(i, fns).add(evs_l(st, fns)).add(evs_l(b, fns)),
        S::TryS(b, ex, eb, fb) => evs_l(b, fns).add(evs_sss(ex, fns)).add(evs_l(eb, fns)).add(evs_l(fb, fns)),
        S::CaseS(_d, cs) => evs_sss(cs, fns),
        S::Blk(l) => evs_l(l, fns),
        S::Subsh(l) => evs_l(l, fns),
        S::Bg(l) => evs_l(l, fns),
        S::Redir(l) => evs_l(l, fns),
        S::Pipe(st) => evs_sss(st, fns),
        _ => Seq::empty(),
    }
}

pub open spec fn evs_els(l: Seq<(X, Seq<S>)>, fns: bool) -> Seq<Ev>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        evs_l(l[0].1, fns).add(evs_els(l.drop_first(), fns))
    }
}

/// "A read happens before the first definite store."
pub open spec fn starts_read(e: Seq<Ev>) -> bool {
    e.len() > 0 && e[0] == Ev::Read
}

/// THE SEMANTIC nested fact.
pub open spec fn nested_sem(s: S, fns: bool) -> bool {
    starts_read(evs_s(s, fns))
}

pub proof fn starts_read_cat(a: Seq<Ev>, b: Seq<Ev>)
    ensures starts_read(a.add(b)) ==> (starts_read(a) || starts_read(b)),
{
    if a.len() == 0 {
        assert(a.add(b) == b);
    } else {
        assert(a.add(b).len() > 0);
        assert(a.add(b)[0] == a[0]);
    }
}

/// The source's `list_reads_before_store`, over the model.
pub open spec fn rbs_l(l: Seq<S>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else if may_s(l[0], fns) {
        true
    } else if own_rc(l[0]) {
        false
    } else {
        rbs_l(l.drop_first(), fns)
    }
}

pub open spec fn rbs_sss(l: Seq<Seq<S>>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        rbs_l(l[0], fns) || rbs_sss(l.drop_first(), fns)
    }
}

pub open spec fn rbs_els(l: Seq<(X, Seq<S>)>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        rbs_l(l[0].1, fns) || rbs_els(l.drop_first(), fns)
    }
}

/// The source's `stmt_nested_reads_before_store`, over the model. It also
/// counts CONDITION reads, which is why the law below is one-way.
pub open spec fn an_nested(s: S, fns: bool) -> bool
    decreases s,
{
    match s {
        S::IfS(c, t, ei, el) => an_x(c, fns) || rbs_l(t, fns) || an_els_nested(ei, fns) || rbs_l(el, fns),
        S::Wh(c, b) => an_x(c, fns) || rbs_l(b, fns),
        S::ForS(c, b) => an_x(c, fns) || rbs_l(b, fns),
        S::DoWh(b, c) => rbs_l(b, fns) || an_x(c, fns),
        S::ForInit(i, c, st, b) => rbs_l(i, fns) || an_x(c, fns) || rbs_l(st, fns) || rbs_l(b, fns),
        S::CaseS(d, cs) => an_x(d, fns) || rbs_sss(cs, fns),
        S::Blk(l) => rbs_l(l, fns),
        S::Subsh(l) => rbs_l(l, fns),
        S::Bg(l) => rbs_l(l, fns),
        S::Redir(l) => rbs_l(l, fns),
        S::Pipe(st) => rbs_sss(st, fns),
        S::TryS(b, ex, eb, fb) => rbs_l(b, fns) || rbs_sss(ex, fns) || rbs_l(eb, fns) || rbs_l(fb, fns),
        _ => false,
    }
}

pub open spec fn an_els_nested(l: Seq<(X, Seq<S>)>, fns: bool) -> bool
    decreases l,
{
    if l.len() == 0 {
        false
    } else {
        an_x(l[0].0, fns) || rbs_l(l[0].1, fns) || an_els_nested(l.drop_first(), fns)
    }
}

// The bridge: the event stream of a list starts with a read exactly when the
// source's list predicate says the list reads before its first store.
pub proof fn rbs_bridge(l: Seq<S>, fns: bool)
    ensures starts_read(evs_l(l, fns)) == rbs_l(l, fns),
    decreases l,
{
    reveal_with_fuel(rbs_l, 2);
    reveal_with_fuel(evs_l, 2);
    if l.len() == 0 {
        assert(evs_l(l, fns) == Seq::<Ev>::empty());
    } else {
        let head = l[0];
        let tail = l.drop_first();
        // stmt_evs(head) is [] / [Read] / [Store] / [Read, Store]
        if may_s(head, fns) {
            assert(stmt_evs(head, fns) == seq![Ev::Read].add(
                if own_rc(head) { seq![Ev::Store] } else { Seq::<Ev>::empty() },
            )) by {
                assert(may_s(head, fns));
            }
            assert(starts_read(stmt_evs(head, fns))) by {
                assert(stmt_evs(head, fns)[0] == Ev::Read);
            }
            assert(starts_read(stmt_evs(head, fns).add(evs_l(tail, fns))));
            assert(rbs_l(l, fns));
        } else if own_rc(head) {
            assert(stmt_evs(head, fns) == Seq::<Ev>::empty().add(seq![Ev::Store]));
            assert(!starts_read(stmt_evs(head, fns))) by {
                assert(stmt_evs(head, fns)[0] == Ev::Store);
            }
            assert(!starts_read(stmt_evs(head, fns).add(evs_l(tail, fns)))) by {
                assert((stmt_evs(head, fns).add(evs_l(tail, fns)))[0] == Ev::Store);
            }
            assert(!rbs_l(l, fns));
        } else {
            assert(stmt_evs(head, fns) == Seq::<Ev>::empty()) by {
                assert(!may_s(head, fns));
                assert(!own_rc(head));
            }
            assert(evs_l(l, fns) == evs_l(tail, fns));
            rbs_bridge(tail, fns);
            // an empty head means the list's verdict IS the tail's verdict,
            // in both directions
            assert(rbs_l(l, fns) == rbs_l(tail, fns));
            assert(starts_read(evs_l(l, fns)) == rbs_l(l, fns));
        }
    }
}

pub proof fn rbs_bridge_sss(l: Seq<Seq<S>>, fns: bool)
    ensures starts_read(evs_sss(l, fns)) ==> rbs_sss(l, fns),
    decreases l,
{
    reveal_with_fuel(evs_sss, 2);
    reveal_with_fuel(rbs_sss, 2);
    if l.len() > 0 {
        let head = l[0];
        rbs_bridge(head, fns);
        rbs_bridge_sss(l.drop_first(), fns);
        if starts_read(evs_l(head, fns).add(evs_sss(l.drop_first(), fns))) {
            starts_read_cat(evs_l(head, fns), evs_sss(l.drop_first(), fns));
            assert(starts_read(evs_l(head, fns)) || starts_read(evs_sss(l.drop_first(), fns)));
        }
    }
}

pub proof fn rbs_bridge_els(l: Seq<(X, Seq<S>)>, fns: bool)
    ensures starts_read(evs_els(l, fns)) ==> rbs_els(l, fns),
    decreases l,
{
    reveal_with_fuel(evs_els, 2);
    reveal_with_fuel(rbs_els, 2);
    if l.len() > 0 {
        let head = l[0];
        rbs_bridge(head.1, fns);
        rbs_bridge_els(l.drop_first(), fns);
        if starts_read(evs_l(head.1, fns).add(evs_els(l.drop_first(), fns))) {
            starts_read_cat(evs_l(head.1, fns), evs_els(l.drop_first(), fns));
            assert(starts_read(evs_l(head.1, fns)) || starts_read(evs_els(l.drop_first(), fns)));
        }
    }
}

/// THE OBLIGATION (nested): the analysis never misses a nested
/// read-before-store that the event semantics can produce.
pub proof fn nested_complete(s: S, fns: bool)
    ensures nested_sem(s, fns) ==> an_nested(s, fns),
    decreases s,
{
    reveal_with_fuel(evs_s, 1);
    reveal_with_fuel(an_nested, 1);
    reveal_with_fuel(rbs_l, 2);
    reveal_with_fuel(evs_l, 2);
    reveal_with_fuel(rbs_sss, 2);
    reveal_with_fuel(rbs_els, 2);
    reveal_with_fuel(an_els_nested, 2);
    match s {
        S::IfS(c, t, ei, el) => {
            rbs_bridge(t, fns);
            rbs_bridge(el, fns);
            rbs_bridge_els(ei, fns);
            els_bridge(ei, fns);
            if nested_sem(s, fns) {
                starts_read_cat(evs_l(t, fns).add(evs_els(ei, fns)), evs_l(el, fns));
                starts_read_cat(evs_l(t, fns), evs_els(ei, fns));
                assert(starts_read(evs_l(t, fns)) || starts_read(evs_els(ei, fns)) || starts_read(evs_l(el, fns)));
            }
        }
        S::Wh(c, b) => {
            rbs_bridge(b, fns);
        }
        S::ForS(c, b) => {
            rbs_bridge(b, fns);
        }
        S::DoWh(b, c) => {
            rbs_bridge(b, fns);
        }
        S::ForInit(i, c, st, b) => {
            rbs_bridge(i, fns);
            rbs_bridge(st, fns);
            rbs_bridge(b, fns);
            if nested_sem(s, fns) {
                starts_read_cat(evs_l(i, fns).add(evs_l(st, fns)), evs_l(b, fns));
                starts_read_cat(evs_l(i, fns), evs_l(st, fns));
                assert(starts_read(evs_l(i, fns)) || starts_read(evs_l(st, fns)) || starts_read(evs_l(b, fns)));
            }
        }
        S::TryS(b, ex, eb, fb) => {
            rbs_bridge(b, fns);
            rbs_bridge(eb, fns);
            rbs_bridge(fb, fns);
            rbs_bridge_sss(ex, fns);
            if nested_sem(s, fns) {
                starts_read_cat(evs_l(b, fns), evs_sss(ex, fns));
                starts_read_cat(evs_l(b, fns).add(evs_sss(ex, fns)), evs_l(eb, fns));
                starts_read_cat(evs_l(b, fns).add(evs_sss(ex, fns)).add(evs_l(eb, fns)), evs_l(fb, fns));
                assert(starts_read(evs_l(b, fns)) || starts_read(evs_sss(ex, fns)) || starts_read(evs_l(eb, fns)) || starts_read(evs_l(fb, fns)));
            }
        }
        S::CaseS(d, cs) => {
            rbs_bridge_sss(cs, fns);
        }
        S::Blk(l) => {
            rbs_bridge(l, fns);
        }
        S::Subsh(l) => {
            rbs_bridge(l, fns);
        }
        S::Bg(l) => {
            rbs_bridge(l, fns);
        }
        S::Redir(l) => {
            rbs_bridge(l, fns);
        }
        S::Pipe(st) => {
            rbs_bridge_sss(st, fns);
        }
        _ => {}
    }
}

/// `rbs_els` implies the analysis's elsif clause predicate: a clause whose
/// body reads before its first store is exactly `an_els_nested`'s body arm.
pub proof fn els_bridge(l: Seq<(X, Seq<S>)>, fns: bool)
    ensures rbs_els(l, fns) ==> an_els_nested(l, fns),
    decreases l,
{
    reveal_with_fuel(rbs_els, 2);
    reveal_with_fuel(an_els_nested, 2);
    if l.len() > 0 {
        els_bridge(l.drop_first(), fns);
    }
}

/// The traversal plan's name for the nested law: a semantic nested
/// read-before-store implies the analysis's predicate — one-way on purpose,
/// because the source also counts CONDITION reads there (it may over-claim,
/// never miss). Same obligation as `nested_complete`, cited by this name.
pub proof fn nested_is_may_approx(s: S, fns: bool)
    ensures nested_sem(s, fns) ==> an_nested(s, fns),
{
    nested_complete(s, fns);
}

// ────────────────────────────────── the three facts, in one statement ───────

/// The consumer of `status_reads.rs` gets all three facts from this model.
pub proof fn facts_sound(s: S, fns: bool)
    ensures
        (!an_s(s, fns) ==> !may_s(s, fns)),
        (an_stores(s) ==> own_rc(s)),
        (nested_sem(s, fns) ==> an_nested(s, fns)),
{
    reads_is_may_approx(s, fns);
    stores_is_must(s);
    nested_is_may_approx(s, fns);
}

// ───────────────────────────────────────────────────────── the pins ───────
// Shapes taken from the source's own tests.

/// `[ -z "$?" ]` — a test operand carrying `$?`.
pub open spec fn test_operand() -> S {
    S::ExprS(X::Call(Fn::Plain, seq![X::StrQ(true)]))
}

/// `getVar("?")`
pub open spec fn getvar_q() -> S {
    S::ExprS(X::Call(Fn::GetVarQ, seq![X::StrQ(false)]))
}

/// `echo hi`
pub open spec fn echo_hi() -> S {
    S::Out(X::StrQ(false))
}

/// `a=1`
pub open spec fn assign_one() -> S {
    S::Asg(seq![Seq::<X>::empty()], X::RawQ(false), false)
}

/// `x=$(echo hi)` — the capture-assign shape
pub open spec fn assign_capture() -> S {
    S::Asg(seq![Seq::<X>::empty()], X::Cap(seq![X::StrQ(false)]), false)
}

/// Mirrors `detects_test_operand_status`.
pub proof fn pin_test_operand_is_a_read()
    ensures may_s(test_operand(), true),
{
    assert(may_s(test_operand(), true)) by (compute);
}

/// Mirrors `detects_getvar_status`.
pub proof fn pin_getvar_status_is_a_read()
    ensures may_s(getvar_q(), true),
{
    assert(may_s(getvar_q(), true)) by (compute);
}

/// Mirrors `status_free_program_is_false`.
pub proof fn pin_echo_is_status_free()
    ensures !may_s(echo_hi(), true),
{
    assert(!may_s(echo_hi(), true)) by (compute);
}

/// Mirrors `bare_exit_reads_the_status`: a bare `exit` reads, `exit 0` does not.
pub proof fn pin_bare_exit_reads_the_status()
    ensures
        may_s(S::ExitS(false, X::OtherX), true),
        !may_s(S::ExitS(true, X::OtherX), true),
{
    assert(may_s(S::ExitS(false, X::OtherX), true)) by (compute);
    assert(!may_s(S::ExitS(true, X::OtherX), true)) by (compute);
}

/// Mirrors `a_function_body_localises_its_own_reads`' premise: with
/// `fns = false` a body's reads are NOT counted (the body is its own scan),
/// while `program_reads_status` (fns = true) does count them.
pub proof fn pin_bodies_are_localised()
    ensures
        !an_s(S::FnS(seq![test_operand()], Seq::<Seq<S>>::empty()), false),
        an_s(S::FnS(seq![test_operand()], Seq::<Seq<S>>::empty()), true),
{
    assert(!an_s(S::FnS(seq![test_operand()], Seq::<Seq<S>>::empty()), false)) by (compute);
    assert(an_s(S::FnS(seq![test_operand()], Seq::<Seq<S>>::empty()), true)) by (compute);
}

/// A `$( … )` body runs at the point of use: a read inside it is a read.
pub proof fn pin_capture_body_read_is_a_read()
    ensures may_x(X::Call(Fn::Capture, seq![X::Arrow(seq![test_operand()])]), true),
{
    assert(may_x(X::Call(Fn::Capture, seq![X::Arrow(seq![test_operand()])]), true)) by (compute);
}

/// The control-flow constructs are must-stores (bash sets `$?` on every path).
pub proof fn pin_control_flow_stores()
    ensures
        an_stores(S::IfS(X::OtherX, Seq::<S>::empty(), Seq::<(X, Seq<S>)>::empty(), Seq::<S>::empty())),
        an_stores(S::Wh(X::OtherX, Seq::<S>::empty())),
        an_stores(S::ForS(X::OtherX, Seq::<S>::empty())),
        an_stores(S::CaseS(X::OtherX, Seq::<Seq<S>>::empty())),
        own_rc(S::IfS(X::OtherX, Seq::<S>::empty(), Seq::<(X, Seq<S>)>::empty(), Seq::<S>::empty())),
{
    assert(an_stores(S::IfS(X::OtherX, Seq::<S>::empty(), Seq::<(X, Seq<S>)>::empty(), Seq::<S>::empty()))) by (compute);
    assert(an_stores(S::Wh(X::OtherX, Seq::<S>::empty()))) by (compute);
    assert(an_stores(S::ForS(X::OtherX, Seq::<S>::empty()))) by (compute);
    assert(an_stores(S::CaseS(X::OtherX, Seq::<Seq<S>>::empty()))) by (compute);
    assert(own_rc(S::IfS(X::OtherX, Seq::<S>::empty(), Seq::<(X, Seq<S>)>::empty(), Seq::<S>::empty()))) by (compute);
}

/// A capture-assign does NOT store (it renders through `__sh_bash`), and the
/// analysis agrees — this is the shape mutant (c) attacks.
pub proof fn pin_capture_assign_does_not_store()
    ensures
        !an_stores(assign_capture()),
        !own_rc(assign_capture()),
        an_stores(assign_one()),
{
    assert(!an_stores(assign_capture())) by (compute);
    assert(!own_rc(assign_capture())) by (compute);
    assert(an_stores(assign_one())) by (compute);
}

/// `asm` and multi-target assigns do not definitely store.
pub proof fn pin_asm_and_multi_target_do_not_store()
    ensures
        !an_stores(S::Asg(seq![Seq::<X>::empty()], X::RawQ(false), true)),
        !an_stores(S::Asg(seq![Seq::<X>::empty(), Seq::<X>::empty()], X::RawQ(false), false)),
{
    assert(!an_stores(S::Asg(seq![Seq::<X>::empty()], X::RawQ(false), true))) by (compute);
    assert(!an_stores(S::Asg(seq![Seq::<X>::empty(), Seq::<X>::empty()], X::RawQ(false), false))) by (compute);
}

/// The pure-print call arms do not store; a plain call does.
pub proof fn pin_pure_print_calls_do_not_store()
    ensures
        !an_stores(S::ExprS(X::Call(Fn::GrepMatches, seq![X::StrQ(false)]))),
        !own_rc(S::ExprS(X::Call(Fn::GrepMatches, seq![X::StrQ(false)]))),
        an_stores(S::ExprS(X::Call(Fn::Plain, seq![X::StrQ(false)]))),
{
    assert(!an_stores(S::ExprS(X::Call(Fn::GrepMatches, seq![X::StrQ(false)])))) by (compute);
    assert(!own_rc(S::ExprS(X::Call(Fn::GrepMatches, seq![X::StrQ(false)])))) by (compute);
    assert(an_stores(S::ExprS(X::Call(Fn::Plain, seq![X::StrQ(false)])))) by (compute);
}

/// A redirect whose inner list reads before its own first store: the
/// enclosing statement's store must stay (mutant (d) drops this arm).
pub proof fn pin_nested_redirect_reads_before_store()
    ensures
        nested_sem(S::Redir(seq![test_operand()]), true),
        an_nested(S::Redir(seq![test_operand()]), true),
{
    assert(nested_sem(S::Redir(seq![test_operand()]), true)) by (compute);
    assert(an_nested(S::Redir(seq![test_operand()]), true)) by (compute);
}

/// A read AFTER a definite store does not count (the store already overwrote
/// the value the read would see).
pub proof fn pin_read_after_a_store_is_not_nested()
    ensures
        !nested_sem(S::Redir(seq![assign_one(), test_operand()]), true),
        !an_nested(S::Redir(seq![assign_one(), test_operand()]), true),
{
    assert(!nested_sem(S::Redir(seq![assign_one(), test_operand()]), true)) by (compute);
    assert(!an_nested(S::Redir(seq![assign_one(), test_operand()]), true)) by (compute);
}

/// A `case` clause body is scanned (bash's `case` captures nothing), a
/// function body is NOT (it is analysed separately).
pub proof fn pin_case_clause_is_scanned_and_body_is_not()
    ensures
        an_nested(S::CaseS(X::OtherX, seq![seq![test_operand()]]), true),
        !an_nested(S::FnS(seq![test_operand()], Seq::<Seq<S>>::empty()), true),
{
    assert(an_nested(S::CaseS(X::OtherX, seq![seq![test_operand()]]), true)) by (compute);
    assert(!an_nested(S::FnS(seq![test_operand()], Seq::<Seq<S>>::empty()), true)) by (compute);
}

/// The three facts on the smallest interesting statement, together.
pub proof fn pin_facts_on_a_capture_assign()
    ensures
        !an_s(assign_capture(), true),
        !an_stores(assign_capture()),
        !nested_sem(assign_capture(), true),
{
    facts_sound(assign_capture(), true);
    assert(!an_s(assign_capture(), true)) by (compute);
    assert(!nested_sem(assign_capture(), true)) by (compute);
}

fn main() {}

} // verus!
