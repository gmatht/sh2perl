//! Cluster G (part 2): the `command -v` decision and its /dev/null gate.
//!
//! `command -v NAME` asks bash "does NAME resolve to a command?" — it prints
//! the resolved path and exits 0, or prints nothing and exits 1.  This file
//! models the two places `shir_native_stmt.rs` turns that question into a
//! constant:
//!
//!     fn native_command_v(st: &mut IrStmt) -> bool                      // 2116
//!         unwraps `redirect(command -v NAME, [fd1 w /dev/null])` if present
//!         requires the Call to be `exec`/`builtin`, args [Str "command",
//!           Array [Str "-v", Str NAME]]                        (else refuse)
//!         NAME in the hard-coded known-command list              -> status 0
//!         NAME == "nonexistent"                                  -> status 1
//!         otherwise                                              -> refuse
//!         *st = status_exec(known)
//!
//!     fn command_v_redirect_truth(args: &[IrExpr]) -> Option<bool>      // 2153
//!         the same decision, in CONDITION position, and only inside
//!         `redirect(…, [fd1 w /dev/null])` — it returns None for any other
//!         shape, so a condition never loses the printed path.
//!
//!     fn redirect_to_devnull(specs: &[IrExpr]) -> bool                  // 2175
//!         the sink test both callers share: exactly one Object whose fd is 1,
//!         whose mode is "w", whose target is "/dev/null".
//!
//! Two independent definitions meet here, and this is what the laws compare:
//!
//!   * the PASS's answer is a hard-coded membership test over a name list
//!     (`known`) plus one literal missing name, gated on the shape, the flag
//!     and the sink;
//!   * bash's answer is PATH RESOLUTION — `resolves(avail, name)`, "some
//!     executable of that name is on PATH", which has nothing to do with any
//!     list in this file.
//!
//! `status_law` proves the pass's status equals bash's resolution *given* that
//! the pass's list names programs that are really on the reference PATH and
//! that its missing name really is absent.  That hypothesis is not assumed: the
//! generated test `pin_the_known_list_is_on_path` runs `command -v` for every
//! name the source lists and fails if any is unresolvable, and
//! `pin_the_missing_name_is_absent` does the same for "nonexistent".  So the
//! model's hypothesis is measured on the machine the corpus runs on.
//!
//! Two more laws carry the rewrite's justification:
//!
//!   * `sink_law` — the fold replaces the command with `status_exec` and prints
//!     NOTHING; bash prints the resolved path.  Those agree only when the sink
//!     is `/dev/null` (or nothing resolves).  This is why `redirect_to_devnull`
//!     is load-bearing, and why `command_v_redirect_truth` refuses every other
//!     shape (`cond_refuses_every_non_devnull_sink`).
//!   * `bare_form_is_lossy` — d1, a recorded divergence: `native_command_v`
//!     also fires on the BARE `command -v NAME`, where the path goes to real
//!     stdout.  bash prints `/usr/bin/ls`; the fold prints nothing.  The model
//!     keeps both sinks and pins the disagreement rather than hiding it.
//!
//! Residue, stated not claimed: the two source copies (`native_command_v` and
//! `command_v_redirect_truth`) each repeat the ~54-name list.  That they agree
//! is a DATA equality (two long disjunctions), so it is checked exhaustively by
//! the generated test `pin_the_two_copies_agree` over every name in the list
//! plus unknowns, not "proved" by unfolding 54 string equalities — a proof of
//! that shape would be a restatement of the two lists, not a refinement.  The
//! laws here are stated over ONE transcribed decision, and the copied lists are
//! the experiment.
//!
//! Out of model: PATH order and shadowing, aliases and functions (the pass's
//! list is a proof-time approximation, and the hypothesis above is exactly its
//! scope), `command -v`'s output format for builtins and keywords, and
//! `2>&1`-style extra redirects with other fds (the source accepts only the
//! single fd-1 spec).
//!
//! Verus: 0.2026.10.18.7d82994, `--triggers-mode silent --rlimit 40`.

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// §1 The model.
// ---------------------------------------------------------------------------

/// Where the command's stdout goes.
pub enum Sink {
    /// `> /dev/null` — the path text is discarded.
    DevNull,
    /// No redirect at all: the path goes to the shell's stdout (d1).
    Bare,
    /// A redirect to a real file: the path would land in the file.
    File,
}

/// Membership over a name list — a closed form (an existential), not a
/// recursion, so nothing here depends on fuel.
pub open spec fn member(names: Seq<Seq<u8>>, n: Seq<u8>) -> bool {
    exists|i: int| 0 <= i < names.len() && #[trigger] names[i] == n
}

/// bash: the name resolves through PATH.  The model's `avail` is the reference
/// PATH's program names, measured by the generated tests, not a list in the
/// source.
pub open spec fn resolves(avail: Seq<Seq<u8>>, n: Seq<u8>) -> bool {
    member(avail, n)
}

pub open spec fn is_devnull(s: Sink) -> bool {
    match s {
        Sink::DevNull => true,
        _ => false,
    }
}

pub open spec fn is_file_sink(s: Sink) -> bool {
    match s {
        Sink::File => true,
        _ => false,
    }
}

/// `native_command_v`, transcribed in the source's own order: unwrap the
/// redirect if there is one (a non-`/dev/null` redirect is a File sink and is
/// refused), then require the exec/builtin Call and the `-v` flag, then the
/// known-name list, then the one literal missing name.
pub open spec fn stmt_decides(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
) -> Option<bool> {
    if !shape_ok || !flag_v || is_file_sink(sink) {
        None
    } else if member(known, name) {
        Some(true)
    } else if name == missing {
        Some(false)
    } else {
        None
    }
}

/// `command_v_redirect_truth`, transcribed: the same decision, but the sink
/// must be exactly `/dev/null`.
pub open spec fn cond_decides(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
) -> Option<bool> {
    if !shape_ok || !flag_v || !is_devnull(sink) {
        None
    } else if member(known, name) {
        Some(true)
    } else if name == missing {
        Some(false)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// §2 bash's side: status and the path it prints.
// ---------------------------------------------------------------------------

pub open spec fn bash_status(avail: Seq<Seq<u8>>, name: Seq<u8>) -> bool {
    resolves(avail, name)
}

/// bash prints the resolved path exactly when the name resolves.
pub open spec fn bash_prints_path(avail: Seq<Seq<u8>>, name: Seq<u8>) -> bool {
    resolves(avail, name)
}

/// The fold's replacement — `status_exec(b)` — prints nothing.
pub open spec fn fold_prints_path() -> bool {
    false
}

/// What actually reaches the caller's stdout: `/dev/null` swallows whatever the
/// command prints; a bare command or a file redirect keeps it.
pub open spec fn reaches_stdout(sink: Sink, prints: bool) -> bool {
    prints && !is_devnull(sink)
}

// ---------------------------------------------------------------------------
// §3 The laws.
// ---------------------------------------------------------------------------

/// **The status law** (the statement fold).  Given that the pass's known list
/// names programs the reference PATH really has, and that its missing name is
/// really absent, the status the fold writes is bash's resolution status.
/// The pass consults a LIST; bash consults PATH.  Nothing but the hypothesis
/// connects them.
proof fn status_law(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
    avail: Seq<Seq<u8>>,
    b: bool,
)
    requires
        stmt_decides(shape_ok, flag_v, sink, known, missing, name) == Some(b),
        forall|i: int| 0 <= i < known.len() ==> resolves(
            avail,
            #[trigger] known[i],
        ),
        !resolves(avail, missing),
    ensures
        bash_status(avail, name) == b,
{
    if member(known, name) {
        let i = choose|i: int| 0 <= i < known.len() && #[trigger] known[i] == name;
        // the list's entry is on PATH, and it is this very name
        assert(resolves(avail, known[i]));
        assert(known[i] == name);
        assert(b == true);
    } else {
        // not in the list: the fold only fires for the missing name, and that
        // one is absent from PATH
        assert(name == missing);
        assert(!resolves(avail, name));
        assert(b == false);
    }
}

/// **The status law** (the condition fold) — the same content for
/// `command_v_redirect_truth`, whose extra gate is the sink.
proof fn cond_status_law(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
    avail: Seq<Seq<u8>>,
    b: bool,
)
    requires
        cond_decides(shape_ok, flag_v, sink, known, missing, name) == Some(b),
        forall|i: int| 0 <= i < known.len() ==> resolves(
            avail,
            #[trigger] known[i],
        ),
        !resolves(avail, missing),
    ensures
        bash_status(avail, name) == b,
{
    if member(known, name) {
        let i = choose|i: int| 0 <= i < known.len() && #[trigger] known[i] == name;
        assert(resolves(avail, known[i]));
        assert(known[i] == name);
        assert(b == true);
    } else {
        assert(name == missing);
        assert(!resolves(avail, name));
        assert(b == false);
    }
}

/// **The sink law.**  The rewrite prints nothing where bash prints the path, so
/// the two agree on stdout only when the sink is `/dev/null` or when nothing
/// resolves.  This is the obligation `redirect_to_devnull` discharges, and it is
/// why the fold must refuse a redirect to a real file.
proof fn sink_law(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
    avail: Seq<Seq<u8>>,
    b: bool,
)
    requires
        stmt_decides(shape_ok, flag_v, sink, known, missing, name) == Some(b),
        forall|i: int| 0 <= i < known.len() ==> resolves(
            avail,
            #[trigger] known[i],
        ),
        !resolves(avail, missing),
    ensures
        is_devnull(sink) || !resolves(avail, name) ==> {
            reaches_stdout(sink, fold_prints_path()) == reaches_stdout(
                sink,
                bash_prints_path(avail, name),
            )
        },
        // and a bare/filed sink on a resolvable name really does differ
        !is_devnull(sink) && resolves(avail, name) ==> reaches_stdout(
            sink,
            fold_prints_path(),
        ) != reaches_stdout(sink, bash_prints_path(avail, name)),
{
    status_law(shape_ok, flag_v, sink, known, missing, name, avail, b);
}

/// `command_v_redirect_truth` accepts `/dev/null` only: every other sink is
/// refused, so a condition can never be decided where the path would be lost.
proof fn cond_refuses_every_non_devnull_sink(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
)
    requires
        shape_ok,
        flag_v,
        !is_devnull(sink),
    ensures
        !cond_decides(shape_ok, flag_v, sink, known, missing, name).is_some(),
{
}

/// The statement fold refuses a redirect to a real file (a File sink): the path
/// would have to appear in that file.
proof fn stmt_refuses_a_file_sink(
    shape_ok: bool,
    flag_v: bool,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
)
    ensures
        !stmt_decides(shape_ok, flag_v, Sink::File, known, missing, name).is_some(),
{
}

/// A name that is neither on the list nor the missing marker keeps the
/// shell-out: the pass knows nothing about it, so it may or may not resolve.
proof fn unknown_name_is_refused(
    shape_ok: bool,
    flag_v: bool,
    sink: Sink,
    known: Seq<Seq<u8>>,
    missing: Seq<u8>,
    name: Seq<u8>,
)
    requires
        shape_ok,
        flag_v,
        !member(known, name),
        name != missing,
    ensures
        !stmt_decides(shape_ok, flag_v, sink, known, missing, name).is_some(),
{
}

/// Named refusal: a flag other than `-v` keeps the shell-out.
proof fn a_flag_other_than_v_is_refused(known: Seq<Seq<u8>>, missing: Seq<u8>, name: Seq<u8>)
    ensures
        !stmt_decides(true, false, Sink::DevNull, known, missing, name).is_some(),
        !cond_decides(true, false, Sink::DevNull, known, missing, name).is_some(),
{
}

// ---------------------------------------------------------------------------
// §4 Pins: the source's answers, each a proof fn the sync checker can find.
// ---------------------------------------------------------------------------

pub open spec fn n_true() -> Seq<u8> {
    seq![b't', b'r', b'u', b'e']
}

pub open spec fn n_ls() -> Seq<u8> {
    seq![b'l', b's']
}

pub open spec fn n_nonexistent() -> Seq<u8> {
    seq![b'n', b'o', b'n', b'e', b'x', b'i', b's', b't', b'e', b'n', b't']
}

/// A two-entry stand-in for the source's list: `true` and `ls`.
pub open spec fn known2() -> Seq<Seq<u8>> {
    seq![n_true(), n_ls()]
}

/// A PATH that really has both of them.
pub open spec fn avail2() -> Seq<Seq<u8>> {
    seq![n_ls(), n_true()]
}

/// `command -v true > /dev/null` exits 0: the name is on the list.
proof fn pin_a_known_command_is_status_zero()
{
    assert(member(known2(), n_true()));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_true()) == Some(true));
    assert(cond_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_true()) == Some(true));
    assert(bash_status(avail2(), n_true()));
    status_law(true, true, Sink::DevNull, known2(), n_nonexistent(), n_true(), avail2(), true);
}

/// `command -v nonexistent` exits 1: the literal missing name.
proof fn pin_the_missing_name_is_status_one()
{
    assert(!member(known2(), n_nonexistent()));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_nonexistent()) == Some(false));
    assert(!bash_status(avail2(), n_nonexistent()));
}

/// The condition twin decides the same thing for the `/dev/null` shape.
proof fn pin_the_two_copies_agree_on_the_modelled_shape()
{
    assert(cond_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_ls()) == Some(true));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_ls()) == Some(true));
    assert(cond_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_nonexistent())
        == stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_nonexistent()));
}

/// A name the pass does not know keeps the shell-out, in both positions.
proof fn pin_an_unknown_name_keeps_the_shell_out()
{
    let other = seq![b'r', b'g'];
    assert(!member(known2(), other));
    assert(other != n_nonexistent());
    unknown_name_is_refused(true, true, Sink::DevNull, known2(), n_nonexistent(), other);
    assert(!stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), other).is_some());
    assert(!cond_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), other).is_some());
}

/// `command -v NAME > FILE` keeps the shell-out: the path belongs in the file.
proof fn pin_a_file_redirect_keeps_the_shell_out()
{
    stmt_refuses_a_file_sink(true, true, known2(), n_nonexistent(), n_ls());
    assert(!stmt_decides(true, true, Sink::File, known2(), n_nonexistent(), n_ls()).is_some());
}

/// A condition form the sink gate rejects keeps the shell-out: the condition
/// fold only ever fires behind `> /dev/null`.
proof fn pin_a_condition_without_devnull_keeps_the_shell_out()
{
    cond_refuses_every_non_devnull_sink(true, true, Sink::Bare, known2(), n_nonexistent(), n_ls());
    cond_refuses_every_non_devnull_sink(true, true, Sink::File, known2(), n_nonexistent(), n_ls());
}

/// A flag other than `-v` keeps the shell-out in both positions.
proof fn pin_a_non_v_flag_keeps_the_shell_out()
{
    a_flag_other_than_v_is_refused(known2(), n_nonexistent(), n_ls());
}

/// **d1, a recorded divergence.**  The statement fold also fires on the BARE
/// form, where bash prints the resolved path to stdout and the fold prints
/// nothing.  Pinned as the source's answer: the model keeps the `Bare` sink so
/// the disagreement is a fact.
proof fn pin_bare_form_is_lossy()
{
    assert(stmt_decides(true, true, Sink::Bare, known2(), n_nonexistent(), n_ls()) == Some(true));
    // bash prints "/usr/bin/ls" here; the fold prints nothing
    assert(bash_prints_path(avail2(), n_ls()));
    assert(!fold_prints_path());
    assert(reaches_stdout(Sink::Bare, bash_prints_path(avail2(), n_ls())));
    assert(!reaches_stdout(Sink::Bare, fold_prints_path()));
    assert(reaches_stdout(Sink::Bare, fold_prints_path())
        != reaches_stdout(Sink::Bare, bash_prints_path(avail2(), n_ls())));
}

/// The fold claims status 0 for every name its list carries: membership alone
/// decides, so the pass asserts availability for all of them.
proof fn pin_the_fold_claims_every_listed_name()
{
    assert(member(known2(), n_true()));
    assert(member(known2(), n_ls()));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_true()) == Some(true));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_ls()) == Some(true));
}

/// **d2, a divergence the measured hypothesis found.**  The list names programs
/// the reference machine may NOT have (`python`, here: only `python3` exists).
/// For such a name the law's hypothesis `forall i: resolves(avail, known[i])`
/// is false, so `status_law` does not apply — and the fold writes status 0 where
/// bash exits 1.  Pinned as the source's answer rather than modelled away.
proof fn pin_an_absent_listed_name_diverges()
{
    // a PATH that lacks `ls`, although the pass lists it
    let avail = seq![n_true()];
    assert(member(known2(), n_ls()));
    assert(!resolves(avail, n_ls()));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_ls()) == Some(true));
    assert(bash_status(avail, n_ls()) == false);
}

/// The fold's missing name really is absent: `nonexistent` is a literal that no
/// PATH in the model contains, so `!resolves(avail, missing)` is bash's answer
/// and the false branch of the list is sound.
proof fn pin_the_missing_name_is_absent()
{
    assert(!member(known2(), n_nonexistent()));
    assert(stmt_decides(true, true, Sink::DevNull, known2(), n_nonexistent(), n_nonexistent()) == Some(false));
    assert(!bash_status(avail2(), n_nonexistent()));
}

fn main() {}

} // verus!
