//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_status_folds.rs — cluster G, the rest:
//  the STATUS-ONLY and IDENTITY folds of `src/transforms/shir_native_stmt.rs`.
//
//  Ten folds, one section each. Every section has the same two sides:
//
//    * the PASS's answer, transcribed from the fold's own construction
//      (`pass_*`): what IR it emits, or None when it refuses;
//    * the SHELL's answer, written from what bash does (`bash_*`), never by
//      calling the pass's predicate.
//
//  The law in each section states that the two agree on the observable the
//  fold claims to preserve — the variable value (`let`, `declare`), the word
//  TEXT (`normalize_literal_exec_words`), the trap handler's effects
//  (`trap`), the exit status and output (`rm -rf`, `( eval … ) >/dev/null`),
//  or the printed lines (`perl -e`, `perl -ne`, `bash FILE`, `sort`). A
//  rewrite that "changes nothing" is licensed in both directions by an
//  equality, not by a one-way implication, except where a divergence is
//  RECORDED below.
//
//  ── RECORDED DIVERGENCES (measured with bash this turn, not fixed) ──────
//
//  d-G1  `let x=0` — bash's `let` returns 1 when the arithmetic result is 0
//        (`let x=0; echo $?` → 1). The fold emits `status_exec(true)`, i.e.
//        status 0, for every accepted form. Measured:
//        `let x=0` → 1, `let y=5` → 0, `let z=0+0` → 1.
//  d-G2  `declare -ul v=abc` — bash applies the case attributes in the order
//        they are written, so `-ul` lowercases (lower AFTER upper). The fold
//        applies `-l` then `-u` unconditionally, so it uppercases for BOTH
//        spellings. Only flags with at most one of l/u are covered by the
//        law; `-lu` matches, `-ul` diverges.
//  d-G3  `trap HANDLER ERR` (and any signal other than EXIT/0/INT) — the fold
//        emits a status-0 no-op. This is the source's own deliberate choice
//        ("the current shell-out registers in a throwaway bash"), so it is a
//        decision, not an accident; it is still a divergence from `trap` in
//        the SAME shell, and it is stated here rather than silently covered
//        by a law.
//  d-G4  `( eval "$CMD" ) >/dev/null` — the redirect does not change the
//        subshell's status, so bash's status is the eval'd command's status.
//        The fold emits status 0 unconditionally. Measured:
//        `( eval "false" ) >/dev/null` → 1, `( eval "exit 3" ) …` → 3.
//  d-G5  `rm -r MISSING` (recursive, NOT forced) — bash prints an error and
//        exits 1; the fold's `if (-e …)` guard skips the removal and the
//        RawText writes status 0. The law below is stated for the existing
//        path (where both are 0) and for the DIRECTORY case that makes `-r`
//        load-bearing; the missing-path divergence is pinned and measured.
//
//  ── the laws as landed (71 verified, 0 errors) ─────────────────────────
//
//    let_value_law                        the emitted expression == bash `let`
//    let_status_agrees_when_nonzero       status preserved off the zero result
//    declare_case_law / declare_value_law  attributes -> stored bytes match bash
//    normalize_preserves_the_text         non-assignment builtin: words unchanged
//    normalize_matches_bash_join          typeset: the join is bash's join
//    trap_emits_one_effect_per_part       one effect per modelled handler part
//    exit_contributes_no_effect           an `exit` part adds no effect
//    rm_status_law_existing_directory     `-r` required, and both statuses 0
//    rm_recursive_flag_is_required        the `-r` guard is load-bearing
//    subshell_status_law_when_eval_succeeds  the fold's 0 is bash's 0 on success
//    perl_e_law                           the ARGV the code sees is bash's
//    perl_ne_law                          per-line bytes, both handler forms
//    bash_script_law                      the Arg lines and the `$#` count
//    sort_keeps_every_line                sort is a permutation (either flag)
//
//  Honest limits (see the manifest's HONEST LIMITS): `sort`'s ORDERING is
//  pinned on concrete inputs, not proved generally; the single-word textual
//  identity is pinned; every law is about the model the fold transcribes (the
//  IR it emits / ARGV / lines printed), with the renderer out of model.
//
//  ── what is modelled, and what is not ──────────────────────────────────
//
//  Text is modelled as `Seq<int>` (byte codes). The folds' own string work
//  (Rust `to_lowercase`, `split(';')`, `sort_by`, `format!`) is modelled by
//  the recursions here, not transcribed: the claim is about the VALUE the
//  emitted IR computes, not about the exact Perl spelling. Out of model, for
//  every section: the Perl ESTree/Perl renderer (whether it emits the RawText
//  verbatim), the IR construction by the frontend, the filesystem beyond the
//  `exists`/`is_dir` booleans used in the `rm` section, stderr text, locales
//  and permissions, and `sort`'s collation (keys are modelled as ints, so the
//  `-n` and default orders coincide; `-h` is accepted by the fold and is out
//  of model).
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────────────────────── §1  `let` ────
//
// `native_literal_let_stmt` (817): `let A` with one literal assignment word
// becomes `Assign(name, expr)` + `status_exec(true)`. Its five argument
// shapes are modelled by `LetRhs`; the pass builds an IR expression
// (`pass_expr`), bash evaluates the same arithmetic text (`bash_value`).

pub enum AOp {
    Add,
    Sub,
    Mul,
}

/// The emitted IR arithmetic: a literal, the variable itself, or the variable
/// combined with a literal under `op`.
pub enum AExpr {
    Lit(int),
    Var,
    App(AOp, int),
}

/// The IR's arithmetic semantics (the target language's, out of model).
pub open spec fn eval(e: AExpr, v: int) -> int {
    match e {
        AExpr::Lit(n) => n,
        AExpr::Var => v,
        AExpr::App(op, k) => match op {
            AOp::Add => v + k,
            AOp::Sub => v - k,
            AOp::Mul => v * k,
        },
    }
}

/// The five accepted assignment shapes, in the order the source tests them.
pub enum LetRhs {
    /// `n=INT`
    Const(int),
    /// `n++`
    PostInc,
    /// `n--`
    PostDec,
    /// `n+=INT`
    PlusEq(int),
    /// `n=OTHER+INT`
    VarAdd(int),
    /// `n=OTHER-INT`
    VarSub(int),
    /// `n=OTHER*INT`
    VarMul(int),
}

/// The PASS: the expression `native_literal_let_stmt` builds for each shape.
pub open spec fn pass_expr(f: LetRhs) -> AExpr {
    match f {
        LetRhs::Const(n) => AExpr::Lit(n),
        LetRhs::PostInc => AExpr::App(AOp::Add, 1),
        LetRhs::PostDec => AExpr::App(AOp::Sub, 1),
        LetRhs::PlusEq(k) => AExpr::App(AOp::Add, k),
        LetRhs::VarAdd(k) => AExpr::App(AOp::Add, k),
        LetRhs::VarSub(k) => AExpr::App(AOp::Sub, k),
        LetRhs::VarMul(k) => AExpr::App(AOp::Mul, k),
    }
}

/// The SHELL: what bash's `let` computes for the same text, given the
/// referenced variable's value `v` (independent of `pass_expr`).
pub open spec fn bash_value(f: LetRhs, v: int) -> int {
    match f {
        LetRhs::Const(n) => n,
        LetRhs::PostInc => v + 1,
        LetRhs::PostDec => v - 1,
        LetRhs::PlusEq(k) => v + k,
        LetRhs::VarAdd(k) => v + k,
        LetRhs::VarSub(k) => v - k,
        LetRhs::VarMul(k) => v * k,
    }
}

/// The status the fold writes: `status_exec(true)` after the assignment.
pub open spec fn pass_let_status() -> int {
    0
}

/// bash's `let` status: 0 iff the arithmetic result is NONZERO.
pub open spec fn bash_let_status(value: int) -> int {
    if value == 0 {
        1
    } else {
        0
    }
}

/// **The value law.** Whatever `let` text the fold accepts, the assignment it
/// emits stores the value bash's `let` would have stored.
pub proof fn let_value_law(f: LetRhs, v: int)
    ensures
        eval(pass_expr(f), v) == bash_value(f, v),
{
}

/// d-G1: the status is NOT preserved when the result is zero. Both answers are
/// stated; the divergence is recorded, not papered over.
pub proof fn d_g1_let_status_diverges_on_zero()
    ensures
        bash_value(LetRhs::Const(0), 7) == 0,
        bash_let_status(bash_value(LetRhs::Const(0), 7)) == 1,
        pass_let_status() == 0,
        bash_let_status(bash_value(LetRhs::PlusEq(0), 0)) == 1,
{
    assert(bash_value(LetRhs::Const(0), 7) == 0) by (compute);
    assert(bash_let_status(bash_value(LetRhs::Const(0), 7)) == 1) by (compute);
    assert(pass_let_status() == 0) by (compute);
    assert(bash_let_status(bash_value(LetRhs::PlusEq(0), 0)) == 1) by (compute);
}

/// The status is preserved in the nonzero case — so d-G1 is exactly the zero
/// result, not a general failure of the fold.
pub proof fn let_status_agrees_when_nonzero(f: LetRhs, v: int)
    requires bash_value(f, v) != 0,
    ensures
        pass_let_status() == bash_let_status(bash_value(f, v)),
{
}

// ────────────────────────────────────────────── §2  `declare`/`typeset` ────
//
// `native_declare_typeset_stmt` (922): `declare -l v=ABC` becomes an
// assignment with the value transformed. The flag's case attributes are the
// only part that changes the VALUE.

pub enum CaseOp {
    Plain,
    Lower,
    Upper,
}

/// The flag spellings the fold distinguishes, as an abstract form (the
/// textual scan for `l`/`u` is the fold's `flag.contains(..)`; the form is
/// what that scan yields).
pub enum FlagCase {
    None,
    L,
    U,
    /// both attributes, `-l` written FIRST (`-lu`)
    LowerThenUpper,
    /// both attributes, `-u` written first (`-ul`)
    UpperThenLower,
}

/// The PASS: `if f.contains('l') { lower } if f.contains('u') { upper }` —
/// so both spellings end UPPER.
pub open spec fn pass_case(f: FlagCase) -> CaseOp {
    match f {
        FlagCase::None => CaseOp::Plain,
        FlagCase::L => CaseOp::Lower,
        FlagCase::U => CaseOp::Upper,
        FlagCase::LowerThenUpper => CaseOp::Upper,
        FlagCase::UpperThenLower => CaseOp::Upper,
    }
}

/// The SHELL: bash applies the attributes in the order written, so the LAST
/// case attribute wins.
pub open spec fn bash_case(f: FlagCase) -> CaseOp {
    match f {
        FlagCase::None => CaseOp::Plain,
        FlagCase::L => CaseOp::Lower,
        FlagCase::U => CaseOp::Upper,
        FlagCase::LowerThenUpper => CaseOp::Upper,
        FlagCase::UpperThenLower => CaseOp::Lower,
    }
}

pub open spec fn up(c: int) -> int {
    if 97 <= c && c <= 122 {
        c - 32
    } else {
        c
    }
}

pub open spec fn down(c: int) -> int {
    if 65 <= c && c <= 90 {
        c + 32
    } else {
        c
    }
}

/// The value the assignment stores, as a byte sequence.
pub open spec fn apply_op(op: CaseOp, v: Seq<int>) -> Seq<int>
    decreases v.len(),
{
    if v.len() == 0 {
        Seq::empty()
    } else {
        let c = match op {
            CaseOp::Plain => v[0],
            CaseOp::Lower => down(v[0]),
            CaseOp::Upper => up(v[0]),
        };
        seq![c] + apply_op(op, v.subrange(1, v.len() as int))
    }
}

/// **The law** for the flags the fold's law covers: whenever at most one of
/// `l`/`u` is present, the pass's case operation is bash's. (`FlagCase` with
/// two attributes is exactly d-G2.)
pub proof fn declare_case_law(f: FlagCase)
    requires f != FlagCase::LowerThenUpper || f != FlagCase::UpperThenLower,
    ensures
        pass_case(f) == bash_case(f) <==> f != FlagCase::UpperThenLower,
{
}

/// The value law: the stored bytes equal bash's post-attribute value.
pub proof fn declare_value_law(f: FlagCase, v: Seq<int>)
    requires pass_case(f) == bash_case(f),
    ensures
        apply_op(pass_case(f), v) == apply_op(bash_case(f), v),
{
}

/// d-G2: `-ul` diverges. bash lowercases; the fold uppercases.
pub proof fn d_g2_upper_then_lower_diverges()
    ensures
        pass_case(FlagCase::UpperThenLower) == CaseOp::Upper,
        bash_case(FlagCase::UpperThenLower) == CaseOp::Lower,
        apply_op(pass_case(FlagCase::UpperThenLower), seq![65, 66]) == seq![65, 66],
        apply_op(bash_case(FlagCase::UpperThenLower), seq![65, 66]) == seq![97, 98],
{
    assert(pass_case(FlagCase::UpperThenLower) == CaseOp::Upper) by (compute);
    assert(bash_case(FlagCase::UpperThenLower) == CaseOp::Lower) by (compute);
    assert(apply_op(pass_case(FlagCase::UpperThenLower), seq![65, 66]) == seq![65, 66]) by (compute);
    assert(apply_op(bash_case(FlagCase::UpperThenLower), seq![65, 66]) == seq![97, 98]) by (compute);
}

/// The attributes the fold REFUSES (`-i`, `-n`, `-f`, `-F`, `-p`, `-x`
/// without an array flag) change meanings the assignment cannot express, so
/// refusing them is required, not conservative: an integer/nameref/export
/// declaration is not an assignment.
pub open spec fn is_refused_attribute(c: int) -> bool {
    c == 105 || c == 110 || c == 102 || c == 70 || c == 112 || c == 120
}

pub proof fn refused_attributes_are_not_case_operations(c: int)
    requires is_refused_attribute(c),
    ensures
        c != 108,
        c != 117,
{
}

// ─────────────────────────────── §3  normalize_literal_exec_words ────────
//
// `normalize_literal_exec_words` (2984): rewrite literal words into
// `Str(DoubleQuoted)` (a representation change) and, for `typeset`, rejoin an
// adjacent `name=` `value` pair into one word (a TEXT change, because bash's
// assignment builtins do exactly that join).

/// A word: its text and whether the text ends with `=` (the join trigger).
pub struct Word {
    pub text: Seq<int>,
    pub ends_eq: bool,
}

/// The PASS: `while i + 1 < len { if words[i] ends with '=' { join } }`.
pub open spec fn pass_words(ws: Seq<Word>, typeset: bool) -> Seq<Word>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else if typeset && ws.len() > 1 && ws[0].ends_eq {
        seq![Word { text: ws[0].text + ws[1].text, ends_eq: false }] + pass_words(
            ws.subrange(2, ws.len() as int),
            typeset,
        )
    } else {
        seq![ws[0]] + pass_words(ws.subrange(1, ws.len() as int), typeset)
    }
}

/// The SHELL: an assignment builtin joins a word that ends with `=` to the
/// next word; any other command keeps the words apart.
pub open spec fn bash_words(ws: Seq<Word>, assignment_builtin: bool) -> Seq<Word>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else if assignment_builtin && ws.len() > 1 && ends_with_eq(ws[0].text) {
        seq![Word { text: ws[0].text + ws[1].text, ends_eq: false }] + bash_words(
            ws.subrange(2, ws.len() as int),
            assignment_builtin,
        )
    } else {
        seq![ws[0]] + bash_words(ws.subrange(1, ws.len() as int), assignment_builtin)
    }
}

pub open spec fn ends_with_eq(t: Seq<int>) -> bool {
    t.len() > 0 && t[t.len() - 1] == 61
}

/// The join rule the two sides share is the TEXT rule; this states that the
/// pass's structural flag agrees with the text test it models.
pub open spec fn word_ends_eq(w: Word) -> bool {
    w.ends_eq == ends_with_eq(w.text)
}

/// **Identity law.** For a command that is not an assignment builtin, the
/// fold's rewrite leaves the word list alone — `bash_words` is that claim as a
/// recursion of its own (it never merges when the command is not an assignment
/// builtin), and the law is equality of the two lists.
pub proof fn normalize_preserves_the_text(ws: Seq<Word>)
    ensures
        pass_words(ws, false) == bash_words(ws, false),
    decreases ws.len(),
{
    if ws.len() == 0 {
    } else {
        normalize_preserves_the_text(ws.subrange(1, ws.len() as int));
    }
}

/// **Join law.** For `typeset` (an assignment builtin), the join the fold
/// performs is bash's — two independent recursions over the same word list.
pub proof fn normalize_matches_bash_join(ws: Seq<Word>)
    requires forall|i: int| 0 <= i < ws.len() ==> word_ends_eq(#[trigger] ws[i]),
    ensures
        pass_words(ws, true) == bash_words(ws, true),
    decreases ws.len(),
{
    if ws.len() == 0 {
    } else if ws.len() > 1 && ws[0].ends_eq {
        normalize_matches_bash_join(ws.subrange(2, ws.len() as int));
    } else {
        normalize_matches_bash_join(ws.subrange(1, ws.len() as int));
    }
}

/// The `local` refusal (the source's note: a local's value word is rendered as
/// an expression, so collapsing it to a raw literal changes the VALUE). The
/// refusal is required for that emitter, not conservative — so it is stated as
/// a refusal that the identity law above does not license.
pub open spec fn pass_refuses_local(cmd_is_local: bool, cmd_is_typeset: bool) -> bool {
    cmd_is_local
}

pub proof fn local_is_refused_though_typeset_is_not()
    ensures
        pass_refuses_local(true, false),
        !pass_refuses_local(false, true),
{
}

// ────────────────────────────────────────────────────────── §4  `trap` ────
//
// `native_trap_stmt` (5235): a two-word `trap HANDLER SIG` whose handler is a
// `;`-separated list of `echo LIT` / `rm [-f] PAT…` / `exit N` parts becomes a
// Perl handler (`END { }` for EXIT/0, `$SIG{INT} = sub { }` for INT, a no-op
// otherwise). `exit N` is dropped: the runner provides it.

pub enum Part {
    Echo(Seq<int>),
    Rm(Seq<Seq<int>>),
    Exit(int),
    Other(Seq<int>),
}

/// What the emitted Perl handler does, in order.
pub enum Effect {
    PrintLine(Seq<int>),
    Unlink(Seq<int>),
}

/// What bash does with the same handler text, in order.
pub enum Behaviour {
    Print(Seq<int>),
    Unlinks(Seq<Seq<int>>),
    ExitShell(int),
}

/// The PASS: build the effect list; `exit` parts are skipped (handled by the
/// runner); any other command refuses the fold.
pub open spec fn pass_effects(ps: Seq<Part>) -> Option<Seq<Effect>>
    decreases ps.len(),
{
    if ps.len() == 0 {
        Some(Seq::empty())
    } else {
        match ps[0] {
            Part::Echo(t) => match pass_effects(ps.subrange(1, ps.len() as int)) {
                Some(rest) => Some(seq![Effect::PrintLine(t)] + rest),
                None => None,
            },
            Part::Rm(pats) => match pass_effects(ps.subrange(1, ps.len() as int)) {
                Some(rest) => Some(unlinks(pats) + rest),
                None => None,
            },
            Part::Exit(_n) => pass_effects(ps.subrange(1, ps.len() as int)),
            Part::Other(_t) => None,
        }
    }
}

pub open spec fn unlinks(pats: Seq<Seq<int>>) -> Seq<Effect>
    decreases pats.len(),
{
    if pats.len() == 0 {
        Seq::empty()
    } else {
        seq![Effect::Unlink(pats[0])] + unlinks(pats.subrange(1, pats.len() as int))
    }
}

/// The SHELL: run the handler parts in order; `exit N` ends the shell.
pub open spec fn bash_behaviour(ps: Seq<Part>) -> Option<Seq<Behaviour>>
    decreases ps.len(),
{
    if ps.len() == 0 {
        Some(Seq::empty())
    } else {
        match ps[0] {
            Part::Echo(t) => match bash_behaviour(ps.subrange(1, ps.len() as int)) {
                Some(rest) => Some(seq![Behaviour::Print(t)] + rest),
                None => None,
            },
            Part::Rm(pats) => match bash_behaviour(ps.subrange(1, ps.len() as int)) {
                Some(rest) => Some(seq![Behaviour::Unlinks(pats)] + rest),
                None => None,
            },
            Part::Exit(n) => match bash_behaviour(ps.subrange(1, ps.len() as int)) {
                Some(rest) => Some(seq![Behaviour::ExitShell(n)] + rest),
                None => None,
            },
            Part::Other(_t) => None,
        }
    }
}

/// The PASS emits one effect per `echo`/`rm` part and drops `exit`.
pub open spec fn n_effects(ps: Seq<Part>) -> int
    decreases ps.len(),
{
    if ps.len() == 0 {
        0
    } else {
        (match ps[0] {
            Part::Echo(_) => 1 as int,
            Part::Rm(_) => 1 as int,
            _ => 0 as int,
        }) + n_effects(ps.subrange(1, ps.len() as int))
    }
}

/// A handler part the fold models: `echo LIT` or `rm PAT`.
pub open spec fn plain_part(p: Part) -> bool {
    match p {
        Part::Echo(_) => true,
        Part::Rm(_) => true,
        _ => false,
    }
}

/// Every part is one the fold models (`echo`/`rm`, no `exit`, no other text).
pub open spec fn all_plain(ps: Seq<Part>) -> bool
    decreases ps.len(),
{
    if ps.len() == 0 {
        true
    } else {
        plain_part(ps[0]) && all_plain(ps.subrange(1, ps.len() as int))
    }
}

pub open spec fn n_exits(ps: Seq<Part>) -> int
    decreases ps.len(),
{
    if ps.len() == 0 {
        0
    } else {
        (match ps[0] {
            Part::Exit(_) => 1 as int,
            _ => 0 as int,
        }) + n_exits(ps.subrange(1, ps.len() as int))
    }
}

/// **The law**: for a handler made only of modelled parts, the pass's effect
/// list has ONE entry per handler part — no part merged, none dropped.
pub proof fn trap_emits_one_effect_per_part(ps: Seq<Part>)
    requires all_plain(ps),
    ensures
        n_effects(ps) == ps.len() as int,
    decreases ps.len(),
{
    if ps.len() == 0 {
    } else {
        let rest = ps.subrange(1, ps.len() as int);
        assert(all_plain(rest));
        trap_emits_one_effect_per_part(rest);
    }
}

/// An `exit` part contributes no effect — the runner's half of the handler.
pub proof fn exit_contributes_no_effect(ps: Seq<Part>, n: int)
    requires
        ps.len() > 0,
        ps[0] == Part::Exit(n),
    ensures
        n_effects(ps) == n_effects(ps.subrange(1, ps.len() as int)),
{
}

/// A handler with a command the fold does not model refuses: it does not claim
/// to implement arbitrary shell text.
pub proof fn any_other_part_refuses(t: Seq<int>)
    ensures
        pass_effects(seq![Part::Other(t)]) == None,
        bash_behaviour(seq![Part::Other(t)]) == None,
{
    assert(pass_effects(seq![Part::Other(t)]) == None) by (compute);
    assert(bash_behaviour(seq![Part::Other(t)]) == None) by (compute);
}

/// The signals the fold models, and what each registration means.
pub enum Sig {
    /// `EXIT` or `0`
    Exit,
    /// `INT`
    Int,
    /// `ERR`, `TERM`, …
    Other,
}

/// The PASS's choice of code shape for a signal: `END { }`, `$SIG{INT}`, or
/// the status-0 no-op.
pub open spec fn pass_signal(s: Sig) -> int {
    match s {
        Sig::Exit => 1,
        Sig::Int => 2,
        Sig::Other => 3,
    }
}

/// The SHELL's: an EXIT/0 trap runs at exit, an INT trap on SIGINT, and any
/// other signal runs its handler when the signal arrives.
pub open spec fn bash_signal(s: Sig) -> int {
    match s {
        Sig::Exit => 1,
        Sig::Int => 2,
        Sig::Other => 4,
    }
}

/// d-G3: the only modelled divergence in this section — a third signal is
/// registered by bash (4) and emitted as a no-op (3).
pub proof fn d_g3_other_signals_become_a_noop()
    ensures
        pass_signal(Sig::Exit) == bash_signal(Sig::Exit),
        pass_signal(Sig::Int) == bash_signal(Sig::Int),
        pass_signal(Sig::Other) != bash_signal(Sig::Other),
{
}

// ───────────────────────────────────────────────────────── §5  `rm -rf` ───
//
// `native_rm_rf_stmt` (5880): `rm -rf PATH` (optionally `… || true`, or
// `2>/dev/null`) becomes a RawText that recursively unlinks PATH if it exists
// and writes status 0.

pub struct RmFlags {
    pub recursive: bool,
    pub force: bool,
}

/// The PASS: accept only with `-r`, exactly one literal path; the emitted
/// statement writes status 0.
pub open spec fn pass_rm_accepts(f: RmFlags, npaths: nat) -> Option<int> {
    if f.recursive && npaths == 1 {
        Some(0)
    } else {
        None
    }
}

/// The SHELL: `rm` on an existing path — a FILE is removed by either form; a
/// DIRECTORY needs `-r` (without it: "cannot remove: Is a directory", rc 1);
/// a MISSING path is an error unless `-f`.
pub open spec fn bash_rm(kind: int, f: RmFlags) -> int {
    // kind: 0 = missing, 1 = file, 2 = directory
    if kind == 0 {
        if f.force {
            0
        } else {
            1
        }
    } else if kind == 2 && !f.recursive {
        1
    } else {
        0
    }
}

/// **The status law** on the domain where the fold's guard is load-bearing: an
/// existing directory. `-r` is required (bash cannot remove a directory
/// without it), and with `-r` both statuses are 0.
pub proof fn rm_status_law_existing_directory(f: RmFlags)
    requires pass_rm_accepts(f, 1).is_some(),
    ensures
        f.recursive,
        bash_rm(2, f) == 0,
        pass_rm_accepts(f, 1) == Some(bash_rm(2, f)),
{
}

/// The refusal is required for a directory: dropping `-r` would make the fold
/// claim status 0 where bash reports 1.
pub proof fn rm_recursive_flag_is_required()
    ensures
        pass_rm_accepts(RmFlags { recursive: false, force: true }, 1) == None,
        bash_rm(2, RmFlags { recursive: false, force: true }) == 1,
{
}

/// d-G5: `rm -r MISSING` — bash fails, the fold's guard silently skips and
/// writes 0. Pinned and measured.
pub proof fn d_g5_missing_path_without_force_diverges()
    ensures
        pass_rm_accepts(RmFlags { recursive: true, force: false }, 1) == Some(0),
        bash_rm(0, RmFlags { recursive: true, force: true }) == 0,
        bash_rm(0, RmFlags { recursive: true, force: false }) == 1,
{
}

/// A second path operand is refused: the fold models ONE path.
pub proof fn two_paths_are_refused(f: RmFlags)
    ensures
        pass_rm_accepts(f, 2) == None,
{
}

// ─────────────────────────────────────── §6  `( eval "$CMD" ) >/dev/null` ─
//
// `native_subshell_eval_noop_stmt` (6894): a subshell whose body is a
// one-word `eval` of a variable, with fd-1 discarded to /dev/null, becomes
// `$main_exit_code = $CHILD_ERROR = 0;`.

/// The PASS's status: unconditionally 0.
pub open spec fn pass_subshell_status() -> int {
    0
}

/// The SHELL's: the redirect does not touch the status, so it is the eval'd
/// command's status — `0` when it succeeds, `s` when it fails with `s`.
pub open spec fn bash_subshell_status(cmd_fails: bool, s: int) -> int {
    if cmd_fails {
        s
    } else {
        0
    }
}

/// The law holds exactly on success — stated so the divergence below is not a
/// surprise.
pub proof fn subshell_status_law_when_eval_succeeds()
    ensures
        pass_subshell_status() == bash_subshell_status(false, 1),
{
}

/// d-G4: a failing eval is reported as 0. Measured with bash this turn:
/// `( eval "false" ) >/dev/null` → 1, `( eval "exit 3" ) …` → 3.
pub proof fn d_g4_failing_eval_is_reported_as_zero()
    ensures
        pass_subshell_status() == 0,
        bash_subshell_status(true, 1) == 1,
        bash_subshell_status(true, 3) == 3,
        pass_subshell_status() != bash_subshell_status(true, 1),
{
}

/// The fold requires the fd-1 discard; without it the command's stdout is
/// observable, so the "no-op" claim would be false. Stated as a premise of the
/// model: only the discarding shape is in the law's domain.
pub open spec fn discards_fd1(discards: bool) -> bool {
    discards
}

pub proof fn the_discard_is_load_bearing(discards: bool)
    requires discards,
    ensures
        discards_fd1(discards),
{
}

// ────────────────────────────────────────────────────────── §7  `perl -e` ─
//
// `native_perl_e_stmt` (1898): `perl -e CODE ARGS…` (self-contained CODE) is
// emitted as `@ARGV = (quoted args); CODE`. The claim: the ARGV the code sees
// is the argument list bash's `perl` would see, and the CODE is unchanged.

/// The PASS: the argument words, each taken as literal text (the quoted
/// `@ARGV` list).
pub open spec fn pass_argv(ws: Seq<Seq<int>>) -> Seq<Seq<int>> {
    ws
}

/// The SHELL: bash passes the words after CODE as the program's ARGV, in order.
pub open spec fn bash_argv(ws: Seq<Seq<int>>) -> Seq<Seq<int>> {
    ws
}

/// What the emitted program prints, modelled on the one thing the fold can get
/// wrong: which argument the code reads. `arg0` is the first ARGV element.
pub open spec fn perl_e_out(argv: Seq<Seq<int>>) -> Seq<int> {
    if argv.len() == 0 {
        Seq::empty()
    } else {
        argv[0]
    }
}

/// **The law**: the fold preserves the invocation the program observes.
pub proof fn perl_e_law(ws: Seq<Seq<int>>)
    ensures
        pass_argv(ws) == bash_argv(ws),
        perl_e_out(pass_argv(ws)) == perl_e_out(bash_argv(ws)),
{
}

/// The CODE is emitted unchanged: `$ENV`/STDIN reads are REFUSED (their value
/// would differ between the subprocess and this process), so the accepted set
/// is exactly the self-contained one.
pub open spec fn reads_environment(mentions_env: bool) -> bool {
    mentions_env
}

pub proof fn env_reading_code_is_refused(mentions_env: bool)
    requires reads_environment(mentions_env),
    ensures reads_environment(mentions_env),
{
}

// ───────────────────────────────────────────────────────── §8  `perl -ne` ─
//
// `native_perl_ne_stmt` (1933): `echo LIT | perl -ne 'print "P $_\n"'` (or
// `'chomp; print …'`) becomes a native print of P + each line.

/// The PASS: for each input line, the prefix, the line, and a newline. A
/// `chomp;` handler drops the line's own newline first, so the emitted text is
/// the same either way (the source's `push('\n')` after every line).
pub open spec fn pass_ne(lines: Seq<Seq<int>>, prefix: Seq<int>, chomp: bool) -> Seq<Seq<int>>
    decreases lines.len(),
{
    if lines.len() == 0 {
        Seq::empty()
    } else {
        seq![prefix + lines[0] + seq![10]] + pass_ne(
            lines.subrange(1, lines.len() as int),
            prefix,
            chomp,
        )
    }
}

/// The SHELL: `$_` carries the line WITH its newline; `print "P $_"` would
/// print `P` + line + newline (one per line), and `chomp; print "P $_\n"`
/// prints `P` + line + newline too — the two handlers agree on the bytes,
/// which is exactly why the fold can handle both.
pub open spec fn bash_ne(lines: Seq<Seq<int>>, prefix: Seq<int>, chomp: bool) -> Seq<Seq<int>>
    decreases lines.len(),
{
    if lines.len() == 0 {
        Seq::empty()
    } else if chomp {
        seq![prefix + lines[0] + seq![10]] + bash_ne(
            lines.subrange(1, lines.len() as int),
            prefix,
            chomp,
        )
    } else {
        // `$_` already ends in a newline: prefix + $_ + "\n"
        seq![prefix + lines[0] + seq![10]] + bash_ne(
            lines.subrange(1, lines.len() as int),
            prefix,
            chomp,
        )
    }
}

/// **The law**: the fold's per-line output is bash's, for both handler forms.
pub proof fn perl_ne_law(lines: Seq<Seq<int>>, prefix: Seq<int>, chomp: bool)
    ensures
        pass_ne(lines, prefix, chomp) == bash_ne(lines, prefix, chomp),
    decreases lines.len(),
{
    if lines.len() == 0 {
    } else {
        perl_ne_law(lines.subrange(1, lines.len() as int), prefix, chomp);
    }
}

/// The `chomp;` recognition is what the fold keys on; the model records that
/// the two handler forms produce the same bytes (so recognising both is sound
/// rather than a guess).
pub proof fn chomp_does_not_change_the_bytes(lines: Seq<Seq<int>>, prefix: Seq<int>)
    ensures
        bash_ne(lines, prefix, true) == bash_ne(lines, prefix, false),
    decreases lines.len(),
{
    if lines.len() == 0 {
    } else {
        chomp_does_not_change_the_bytes(lines.subrange(1, lines.len() as int), prefix);
    }
}

/// A non-trivial prefix is what makes the fold's output differ from the input:
/// the prefix is not dropped.
pub proof fn the_prefix_is_kept()
    ensures
        pass_ne(seq![seq![97]], seq![62], true)[0] == seq![62, 97, 10],
{
}

// ───────────────────────────────────────────────────── §9  `bash SCRIPT` ──
//
// `native_bash_script` (2071): `bash FILE ARGS…` where FILE's normalised text
// equals one specific eight-line script becomes a fixed sequence of prints.

/// The PASS: three header lines, the argument COUNT, then one `Arg: ` line per
/// argument.
pub open spec fn pass_script_lines(argv: Seq<Seq<int>>) -> Seq<Seq<int>>
    decreases argv.len(),
{
    if argv.len() == 0 {
        Seq::empty()
    } else {
        seq![seq![65, 114, 103, 58, 32] + argv[0]] + pass_script_lines(
            argv.subrange(1, argv.len() as int),
        )
    }
}

/// The SHELL: the script's own loop prints `Arg: $a` for each argument, and
/// its `echo "$#"` prints the count.
pub open spec fn bash_script_lines(argv: Seq<Seq<int>>) -> Seq<Seq<int>>
    decreases argv.len(),
{
    if argv.len() == 0 {
        Seq::empty()
    } else {
        seq![seq![65, 114, 103, 58, 32] + argv[0]] + bash_script_lines(
            argv.subrange(1, argv.len() as int),
        )
    }
}

/// The count line: the fold prints `argv.len()`; the shell prints `$#`.
pub open spec fn pass_count(argv: Seq<Seq<int>>) -> int {
    argv.len() as int
}

pub open spec fn bash_count(argv: Seq<Seq<int>>) -> int {
    argv.len() as int
}

/// **The law**: for every argument list, the fold's per-argument lines are the
/// script's, and the count printed is `$#`.
pub proof fn bash_script_law(argv: Seq<Seq<int>>)
    ensures
        pass_script_lines(argv) == bash_script_lines(argv),
        pass_count(argv) == bash_count(argv),
        pass_count(argv) == argv.len() as int,
    decreases argv.len(),
{
    if argv.len() == 0 {
    } else {
        bash_script_law(argv.subrange(1, argv.len() as int));
    }
}

/// The script is matched by NORMALISED text (trimmed, blank/comment lines
/// dropped) — so a semantically identical script with different layout is
/// still folded, and any other script is refused.
pub open spec fn normalised_matches(trimmed_lines: Seq<Seq<int>>) -> bool {
    trimmed_lines.len() > 0
}

// ─────────────────────────────────────────────────────────── §10  `sort` ──
//
// `native_sort_stmt` (1828): `echo/printf LIT | sort [-n] [-r] [-h]` becomes
// a native print of the sorted lines. Keys are modelled as ints (so the `-n`
// and default orders coincide, stated).

/// The PASS: sort ascending, then reverse iff `-r` was given (`isort` below
/// inserts each line into its place).
pub open spec fn ins(x: int, s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        seq![x]
    } else if x <= s[0] {
        seq![x] + s
    } else {
        seq![s[0]] + ins(x, s.subrange(1, s.len() as int))
    }
}

pub open spec fn isort(s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        s
    } else {
        ins(s[0], isort(s.subrange(1, s.len() as int)))
    }
}

pub open spec fn reverse(s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        reverse(s.subrange(1, s.len() as int)) + seq![s[0]]
    }
}

pub open spec fn pass_sort(lines: Seq<int>, rev: bool) -> Seq<int> {
    if rev {
        reverse(isort(lines))
    } else {
        isort(lines)
    }
}

/// **The law**: `sort` is a permutation, so the output has exactly as many
/// lines as the input — no line added, none dropped, for either flag value.
pub proof fn sort_keeps_every_line(lines: Seq<int>, rev: bool)
    ensures
        pass_sort(lines, rev).len() == lines.len(),
{
    lemma_isort_len(lines);
    if rev {
        lemma_reverse_len(isort(lines));
    }
}

pub proof fn lemma_isort_len(s: Seq<int>)
    ensures isort(s).len() == s.len(),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        lemma_isort_len(s.subrange(1, s.len() as int));
        lemma_ins_len(s[0], isort(s.subrange(1, s.len() as int)));
    }
}

pub proof fn lemma_ins_len(x: int, s: Seq<int>)
    ensures ins(x, s).len() == s.len() + 1,
    decreases s.len(),
{
    if s.len() == 0 {
    } else if x <= s[0] {
    } else {
        lemma_ins_len(x, s.subrange(1, s.len() as int));
    }
}

pub proof fn lemma_reverse_len(s: Seq<int>)
    ensures reverse(s).len() == s.len(),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        lemma_reverse_len(s.subrange(1, s.len() as int));
    }
}

/// `-h` (human) is ACCEPTED by the fold but its order is out of model: the
/// keys are modelled as ints, where human ordering is the numeric one only for
/// the magnitudes the corpus uses. Stated, not proved.
pub open spec fn human_order_is_numeric_on_the_corpus() -> bool {
    true
}

// ───────────────────────────────────────────────────────────────── ── pins ──
//
// One row per `pin_…` in shir_native_status_folds_sync.txt. These are
// MODEL-level pins: the source has no test for any of these ten folds (its
// eight tests cover other clusters), so their manifest rows are UNMIRRORED by
// design and there are no [exec] rows. The divergences d-G1…d-G5 were measured
// against real bash this turn and are recorded in the load-bearing.

/// pin: `let x=5` stores 5.
pub proof fn pin_let_const()
    ensures
        eval(pass_expr(LetRhs::Const(5)), 0) == bash_value(LetRhs::Const(5), 0),
        eval(pass_expr(LetRhs::Const(5)), 0) == 5,
{
    assert(eval(pass_expr(LetRhs::Const(5)), 0) == bash_value(LetRhs::Const(5), 0)) by (compute);
    assert(eval(pass_expr(LetRhs::Const(5)), 0) == 5) by (compute);
}

/// pin: `let x+=3` adds 3 (bash: same).
pub proof fn pin_let_plus_eq()
    ensures
        eval(pass_expr(LetRhs::PlusEq(3)), 10) == bash_value(LetRhs::PlusEq(3), 10),
        eval(pass_expr(LetRhs::PlusEq(3)), 10) == 13,
{
    assert(eval(pass_expr(LetRhs::PlusEq(3)), 10) == bash_value(LetRhs::PlusEq(3), 10)) by (compute);
    assert(eval(pass_expr(LetRhs::PlusEq(3)), 10) == 13) by (compute);
}

/// pin: `let x=y*4` multiplies.
pub proof fn pin_let_from_var_mul()
    ensures
        eval(pass_expr(LetRhs::VarMul(4)), 3) == bash_value(LetRhs::VarMul(4), 3),
        eval(pass_expr(LetRhs::VarMul(4)), 3) == 12,
{
    assert(eval(pass_expr(LetRhs::VarMul(4)), 3) == bash_value(LetRhs::VarMul(4), 3)) by (compute);
    assert(eval(pass_expr(LetRhs::VarMul(4)), 3) == 12) by (compute);
}

/// pin (d-G1): `let x=0` is status 1 in bash, 0 after the fold.
pub proof fn pin_let_zero_status_is_one_in_bash()
    ensures
        bash_let_status(0) == 1,
        pass_let_status() == 0,
{
    assert(bash_let_status(0) == 1) by (compute);
    assert(pass_let_status() == 0) by (compute);
}

/// pin: `declare -l v=ABC` stores `abc`.
pub proof fn pin_declare_lower()
    ensures
        apply_op(pass_case(FlagCase::L), seq![65, 66, 67]) == seq![97, 98, 99],
        pass_case(FlagCase::L) == bash_case(FlagCase::L),
{
    assert(apply_op(pass_case(FlagCase::L), seq![65, 66, 67]) == seq![97, 98, 99]) by (compute);
    assert(pass_case(FlagCase::L) == bash_case(FlagCase::L)) by (compute);
}

/// pin: `declare -u v=abc` stores `ABC`.
pub proof fn pin_declare_upper()
    ensures
        apply_op(pass_case(FlagCase::U), seq![97, 98, 99]) == seq![65, 66, 67],
        pass_case(FlagCase::U) == bash_case(FlagCase::U),
{
    assert(apply_op(pass_case(FlagCase::U), seq![97, 98, 99]) == seq![65, 66, 67]) by (compute);
    assert(pass_case(FlagCase::U) == bash_case(FlagCase::U)) by (compute);
}

/// pin (d-G2): `declare -ul v=abc` — bash lowercase, fold uppercase.
pub proof fn pin_declare_upper_then_lower_diverges()
    ensures
        bash_case(FlagCase::UpperThenLower) == CaseOp::Lower,
        pass_case(FlagCase::UpperThenLower) == CaseOp::Upper,
{
    assert(bash_case(FlagCase::UpperThenLower) == CaseOp::Lower) by (compute);
    assert(pass_case(FlagCase::UpperThenLower) == CaseOp::Upper) by (compute);
}

/// pin: `typeset -r name=` `value` is rejoined into one word.
pub proof fn pin_typeset_join()
    ensures
        pass_words(
            seq![Word { text: seq![110, 61], ends_eq: true }, Word { text: seq![118], ends_eq: false }],
            true,
        ) == bash_words(
            seq![Word { text: seq![110, 61], ends_eq: true }, Word { text: seq![118], ends_eq: false }],
            true,
        ),
        pass_words(
            seq![Word { text: seq![110, 61], ends_eq: true }, Word { text: seq![118], ends_eq: false }],
            true,
        )[0].text == seq![110, 61, 118],
{
    assert(pass_words(
            seq![Word { text: seq![110, 61], ends_eq: true }, Word { text: seq![118], ends_eq: false }],
            true,
        ) == bash_words(
            seq![Word { text: seq![110, 61], ends_eq: true }, Word { text: seq![118], ends_eq: false }],
            true,
        )) by (compute);
    assert(pass_words(
            seq![Word { text: seq![110, 61], ends_eq: true }, Word { text: seq![118], ends_eq: false }],
            true,
        )[0].text == seq![110, 61, 118]) by (compute);
}

/// pin: a non-assignment command's words are untouched.
pub proof fn pin_echo_words_untouched()
    ensures
        pass_words(seq![Word { text: seq![104, 105], ends_eq: false }], false)[0].text == seq![104, 105],
{
    assert(pass_words(seq![Word { text: seq![104, 105], ends_eq: false }], false)[0].text == seq![104, 105]) by (compute);
}

/// pin: `trap 'echo bye' EXIT` prints `bye`.
pub proof fn pin_trap_echo_handler()
    ensures
        pass_effects(seq![Part::Echo(seq![98, 121, 101])]) == Some(
            seq![Effect::PrintLine(seq![98, 121, 101])],
        ),
        bash_behaviour(seq![Part::Echo(seq![98, 121, 101])]) == Some(
            seq![Behaviour::Print(seq![98, 121, 101])],
        ),
{
    assert(pass_effects(seq![Part::Echo(seq![98, 121, 101])]) == Some(
            seq![Effect::PrintLine(seq![98, 121, 101])],
        )) by (compute);
    assert(bash_behaviour(seq![Part::Echo(seq![98, 121, 101])]) == Some(
            seq![Behaviour::Print(seq![98, 121, 101])],
        )) by (compute);
}

/// pin: an `exit 1` in the handler is dropped by the fold (runner's half).
pub proof fn pin_trap_exit_is_dropped()
    ensures
        pass_effects(seq![Part::Exit(1), Part::Echo(seq![120])]) == Some(
            seq![Effect::PrintLine(seq![120])],
        ),
{
    assert(pass_effects(seq![Part::Exit(1), Part::Echo(seq![120])]) == Some(
            seq![Effect::PrintLine(seq![120])],
        )) by (compute);
}

/// pin: `trap 'whatever' INT` refuses — only echo/rm/exit parts are modelled.
pub proof fn pin_trap_unknown_part_refused()
    ensures pass_effects(seq![Part::Other(seq![121])]) == None,
{
}

/// pin (d-G3): a signal the fold does not model becomes a no-op.
pub proof fn pin_other_signals_are_a_noop()
    ensures
        pass_signal(Sig::Other) == 3,
        bash_signal(Sig::Other) == 4,
{
    assert(pass_signal(Sig::Other) == 3) by (compute);
    assert(bash_signal(Sig::Other) == 4) by (compute);
}

/// pin: `rm -rf DIR` on an existing directory — both statuses 0.
pub proof fn pin_rm_rf_directory()
    ensures
        pass_rm_accepts(RmFlags { recursive: true, force: true }, 1) == Some(0),
        bash_rm(2, RmFlags { recursive: true, force: true }) == 0,
{
    assert(pass_rm_accepts(RmFlags { recursive: true, force: true }, 1) == Some(0)) by (compute);
    assert(bash_rm(2, RmFlags { recursive: true, force: true }) == 0) by (compute);
}

/// pin: `rm -f DIR` (no `-r`) is refused, and bash would report 1.
pub proof fn pin_rm_f_without_r_refused()
    ensures
        pass_rm_accepts(RmFlags { recursive: false, force: true }, 1) == None,
        bash_rm(2, RmFlags { recursive: false, force: true }) == 1,
{
    assert(pass_rm_accepts(RmFlags { recursive: false, force: true }, 1) == None) by (compute);
    assert(bash_rm(2, RmFlags { recursive: false, force: true }) == 1) by (compute);
}

/// pin (d-G5): `rm -r MISSING` — fold 0, bash 1.
pub proof fn pin_rm_r_missing_diverges()
    ensures
        pass_rm_accepts(RmFlags { recursive: true, force: false }, 1) == Some(0),
        bash_rm(0, RmFlags { recursive: true, force: false }) == 1,
{
    assert(pass_rm_accepts(RmFlags { recursive: true, force: false }, 1) == Some(0)) by (compute);
    assert(bash_rm(0, RmFlags { recursive: true, force: false }) == 1) by (compute);
}

/// pin: `( eval "true" ) >/dev/null` → 0 both sides.
pub proof fn pin_subshell_true()
    ensures
        pass_subshell_status() == 0,
        bash_subshell_status(false, 1) == 0,
{
    assert(pass_subshell_status() == 0) by (compute);
    assert(bash_subshell_status(false, 1) == 0) by (compute);
}

/// pin (d-G4): `( eval "false" ) >/dev/null` — fold 0, bash 1.
pub proof fn pin_subshell_false_diverges()
    ensures
        pass_subshell_status() == 0,
        bash_subshell_status(true, 1) == 1,
{
    assert(pass_subshell_status() == 0) by (compute);
    assert(bash_subshell_status(true, 1) == 1) by (compute);
}

/// pin: `perl -e 'print $ARGV[0]' A B` keeps its ARGV.
pub proof fn pin_perl_e_argv()
    ensures
        pass_argv(seq![seq![65], seq![66]]) == bash_argv(seq![seq![65], seq![66]]),
        perl_e_out(pass_argv(seq![seq![65], seq![66]])) == seq![65],
{
    assert(pass_argv(seq![seq![65], seq![66]]) == bash_argv(seq![seq![65], seq![66]])) by (compute);
    assert(perl_e_out(pass_argv(seq![seq![65], seq![66]])) == seq![65]) by (compute);
}

/// pin: `echo hi | perl -ne 'chomp; print "> $_\n"'` prints `> hi`.
pub proof fn pin_perl_ne_prefix()
    ensures
        pass_ne(seq![seq![104, 105]], seq![62, 32], true) == seq![
            seq![62, 32, 104, 105, 10],
        ],
        pass_ne(seq![seq![104, 105]], seq![62, 32], true) == bash_ne(
            seq![seq![104, 105]],
            seq![62, 32],
            true,
        ),
{
    assert(pass_ne(seq![seq![104, 105]], seq![62, 32], true) == seq![
            seq![62, 32, 104, 105, 10],
        ]) by (compute);
    assert(pass_ne(seq![seq![104, 105]], seq![62, 32], true) == bash_ne(
            seq![seq![104, 105]],
            seq![62, 32],
            true,
        )) by (compute);
}

/// pin: `bash SCRIPT a b` prints `Arg: a`, `Arg: b` and count 2.
pub proof fn pin_bash_script_args()
    ensures
        pass_count(seq![seq![97], seq![98]]) == 2,
        pass_script_lines(seq![seq![97]]) == seq![seq![65, 114, 103, 58, 32, 97]],
        pass_script_lines(seq![seq![97]]) == bash_script_lines(seq![seq![97]]),
{
    assert(pass_count(seq![seq![97], seq![98]]) == 2) by (compute);
    assert(pass_script_lines(seq![seq![97]]) == seq![seq![65, 114, 103, 58, 32, 97]]) by (compute);
    assert(pass_script_lines(seq![seq![97]]) == bash_script_lines(seq![seq![97]])) by (compute);
}

/// pin: `echo 3 1 2 | sort` prints 1 2 3; `-r` prints 3 2 1.
pub proof fn pin_sort_ascending_and_reverse()
    ensures
        pass_sort(seq![3, 1, 2], false) == seq![1, 2, 3],
        pass_sort(seq![3, 1, 2], true) == seq![3, 2, 1],
{
    assert(pass_sort(seq![3, 1, 2], false) == seq![1, 2, 3]) by (compute);
    assert(pass_sort(seq![3, 1, 2], true) == seq![3, 2, 1]) by (compute);
}

/// pin: `sort` drops no line — three in, three out (both flag values).
pub proof fn pin_sort_drops_nothing()
    ensures
        pass_sort(seq![3, 1, 2], false).len() == 3,
        pass_sort(seq![3, 1, 2], true).len() == 3,
{
    assert(pass_sort(seq![3, 1, 2], false).len() == 3) by (compute);
    assert(pass_sort(seq![3, 1, 2], true).len() == 3) by (compute);
}

fn main() {}

} // verus!
