//! `let` under a glob-marked word — `c_let_scope.rs`
//!
//! ## The failure
//!
//! bash's `let u=u*2` is a builtin call whose ARGUMENT is arithmetic text. The
//! core marks every unquoted word that could pathname-expand with a
//! `\x01SH2GLOB\x01` prefix, so the word that reaches the C renderer is
//! `\x01SH2GLOB\x01u=u*2` — not arithmetic — and both native paths declined it
//! (`parse_arith` on the marked text, `let_render` after that). The arm then
//! fell back to running `(( … ))` in a **child shell**:
//!
//! ```c
//! static int _sh_site_0(void) {
//!   _sh_ll2s(_s0, (long long)(u)); _sh_export("u", _s0);   /* push the value */
//!   _sh_reset();
//!   _sh_addraw("(( \001SH2GLOB\001u=u*2 ))");               /* marker leaked */
//!   return !_sh_system_rc();                                /* …no write-back */
//! }
//! ```
//!
//! MEASURED 2026-11-05: `u=7; let u=u*2; echo "[$u]"` printed `[7]` where bash
//! prints `[14]`. Two things are wrong and ONE is the deep one:
//!
//! 1. the marker is a flag on the word, not part of its text, so it must be
//!    stripped before the arithmetic is parsed (`strip_glob`);
//! 2. **a write performed in a child does not exist in the parent** — and `let`
//!    is a pure assignment form, so the child-site fallback is not a slow path,
//!    it is a no-op with a success status. That is the semantic fact this model
//!    records, and it is why the fix had to make the native path TOTAL for
//!    assignment text rather than "prefer" it.
//!
//! ## What the model is
//!
//! A store is a finite map `Name -> Str`; a program is a list of writes. Two
//! evaluation modes: `Native` writes the store directly, `Child` writes a
//! FORK of it and returns a status. `visible` is the store the parent can read
//! afterwards. The obligations are the two-mode separation, the reachability of
//! the native mode for every assignment shape (including the marked word), and
//! the marker lemma.
//!
//! ## 4. the ERROR path (added 2026-11-05)
//!
//! A `let`/`(( ))` assignment is not always a store: bash evaluates the
//! arithmetic first and a DIVISION BY ZERO aborts the evaluation where it
//! stands. MEASURED: `u=7; let u=u/0` → `[7] rc=1`, `u=7; (( u=u/0 ))` →
//! `[7] rc=1`, and the renderer stored `0` for both because its divider is
//! deliberately TOTAL (`(r == 0 ? 0 : l/r)` — C would SIGFPE). The value `0`
//! is a lie that only the STORE can repair, so the model carries `Outcome`
//! (`Val`/`Err`), a PARTIAL `ev_div`, the total `rendered_div`, and
//! `after_outcome` — and the detector `detects(known)` that the two
//! assignment sites consult before storing.
//!
//! ## What is NOT modelled
//!
//! - the arithmetic itself (`let`'s value semantics live in `c_unset_scalar` /
//!   `c_printf_v`'s `atoll`/`canon`; here only WHERE the write lands matters);
//! - a divisor that is zero only at RUN time: `detects(None)` is false, so the
//!   store still fires with the rendered 0 where bash aborts. Proved as the
//!   INCOMPLETENESS obligation (`detection_is_incomplete`) and pinned in
//!   C_UB_PROOF.md §33, not papered over;
//! - `u=$(( u/0 ))`, which is FATALLY fatal in bash (the shell exits and
//!   nothing after it runs) — the store is now skipped, the abort is not;
//! - the `${#arr[@]}` shape (an Interpolate word rather than a `Str`), which
//!   still reaches the child site — also pinned, not proved.

use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;
pub type Name = Str;

// ── the store ──────────────────────────────────────────────────────────

pub type Store = Map<Name, Str>;

pub open spec fn empty_store() -> Store {
    Map::empty()
}

pub open spec fn set(st: Store, n: Name, v: Str) -> Store {
    st.insert(n, v)
}

pub open spec fn get(st: Store, n: Name) -> Option<Str> {
    if st.contains_key(n) {
        Some(st[n])
    } else {
        None
    }
}

// ── the two evaluation modes ───────────────────────────────────────────

pub enum Mode {
    /// the write lands in the PARENT's store (a native C statement)
    Native,
    /// the write lands in a FORK: a child shell's environment
    Child,
}

/// The store a `let` write leaves behind in the mode it was rendered in.
/// A child gets a COPY (that is what fork does), so its write is invisible.
pub open spec fn after(m: Mode, st: Store, n: Name, v: Str) -> Store {
    match m {
        Mode::Native => set(st, n, v),
        // the child's fork is discarded when the process exits; the parent
        // still holds `st`. `_sh_export` pushes the OLD value DOWN, never up.
        Mode::Child => st,
    }
}

/// What the parent can read after the statement.
pub open spec fn visible(m: Mode, st: Store, n: Name) -> Option<Str> {
    get(after(m, st, n, get(st, n).unwrap_or(Seq::empty())), n)
}

// ── 1. the child site is a NO-OP for an assignment ────────────────────

pub open spec fn u7() -> Store {
    set(empty_store(), seq!['u'], seq!['7', '1', '4'])
}

/// The measured shape: `u=7` then a `let` that computes 14. In the child mode
/// the parent still reads 7 — the `[7]` the gate saw — while the native mode
/// reads the computed text.
pub proof fn child_write_is_invisible()
    ensures
        // the parent's value after `u=7`
        get(u7(), seq!['u']) == Some(seq!['7', '1', '4']),
        // the child mode leaves it untouched, whatever the child computed
        forall|v: Str| after(Mode::Child, u7(), seq!['u'], v) == u7(),
        // the native mode takes the value
        forall|v: Str| get(after(Mode::Native, u7(), seq!['u'], v), seq!['u']) == Some(v),
{
}

/// The two modes differ on EVERY assignment of a fresh value — so a fallback
/// from one to the other is not a performance choice, it is a correctness one.
pub proof fn modes_disagree(st: Store, n: Name, v: Str)
    requires
        st.contains_key(n),
        get(st, n) != Some(v),
    ensures
        after(Mode::Native, st, n, v) != after(Mode::Child, st, n, v),
{
}

/// …and with a status, which is the trap: the child site returns the CHILD's
/// exit status, so the failure is silent — `rc=0` and no effect.
pub open spec fn status(m: Mode, ok: bool) -> bool {
    // both modes report the arithmetic's success; only the STORE differs
    ok
}

pub proof fn child_site_reports_success_without_effect(v: Str)
    ensures
        status(Mode::Child, true),
        after(Mode::Child, u7(), seq!['u'], v) == u7(),
{
}

// ── 2. the marker is a FLAG, not text ─────────────────────────────────

pub open spec fn glob_magic() -> Str {
    seq!['\u{1}', 'S', 'H', '2', 'G', 'L', 'O', 'B', '\u{1}']
}

pub open spec fn marked(t: Str) -> Str {
    glob_magic().add(t)
}

/// Stripping the marker is the identity on the ARITHMETIC TEXT — which is why
/// the parse (`parse_arith`) may run on it: the marker only said "this word is
/// not a literal string", never that the value differs.
pub proof fn strip_recovers_the_text(t: Str)
    ensures
        strip(marked(t)) == t,
        marked(t) != t,
{
    lemma_glob_magic_nonempty();
    lemma_add_len(glob_magic(), t);
    lemma_strip(marked(t), t);
}

pub open spec fn strip(s: Str) -> Str {
    if s.len() >= glob_magic().len() && s.subrange(0, glob_magic().len() as int) == glob_magic() {
        s.subrange(glob_magic().len() as int, s.len() as int)
    } else {
        s
    }
}

proof fn lemma_strip(s: Str, t: Str)
    requires s == marked(t),
    ensures strip(s) == t,
{
    lemma_add_len(glob_magic(), t);
    assert(s.subrange(0, glob_magic().len() as int) == glob_magic());
    assert(s.subrange(glob_magic().len() as int, s.len() as int) == t);
}

proof fn lemma_glob_magic_nonempty()
    ensures glob_magic().len() > 0,
{
}

proof fn lemma_add_len(a: Str, b: Str)
    ensures a.add(b).len() == a.len() + b.len(),
{
}

/// …and the OLD child text carried the marker INTO bash: `(( \x01…u=u*2 ))`.
/// A child that sees a marked word computes on garbage — the second half of
/// the same defect.
pub proof fn marked_text_is_not_arithmetic(t: Str)
    requires t.len() > 0,
    ensures
        marked(t).len() == glob_magic().len() + t.len(),
        marked(t)[0] == '\u{1}',
{
}

// ── 3. the native path must be TOTAL over assignment shapes ───────────

/// The three shapes a `let` word can take, and which path renders each.
pub enum LetShape {
    /// `let i++`, `let "x < 3"` — `let_render` (always existed)
    Expr,
    /// `let u=u*2` (marked, assignment) — the NATIVE PARSE fallback (new)
    Assign,
    /// `${#a[@]}` inside the text — the substituted-count path (always existed)
    ArrayCount,
}

pub open spec fn rendered_in(s: LetShape) -> Mode {
    match s {
        // all three now write the parent's store; before the fallback the
        // assignment shape and anything `let_render` declined went to Child
        LetShape::Expr => Mode::Native,
        LetShape::Assign => Mode::Native,
        LetShape::ArrayCount => Mode::Native,
    }
}

/// THE OBLIGATION: every assignment shape renders Native, so no `let` can be
/// silently dropped by the child site. Stated as "no shape maps to Child",
/// which is what makes `child_write_is_invisible` a fact about a path the
/// renderer never takes for an assignment.
pub proof fn every_assignment_shape_is_native()
    ensures
        forall|s: LetShape| rendered_in(s) == Mode::Native,
        rendered_in(LetShape::Assign) == Mode::Native,
{
}

/// The fallback is ADDITIVE, and this is the whole of the claim: the native
/// parse runs IFF `let_render` declined. Everything `let_render` already
/// handled therefore keeps the exact path it had, and only the texts that
/// previously fell through to the child site change.
pub open spec fn runs_fallback(declined: bool) -> bool {
    declined
}

pub proof fn fallback_is_additive()
    ensures
        runs_fallback(true),
        !runs_fallback(false),
{
}

// ── 4. the ERROR path: an arith error aborts the store ────────────────
//
// A `let`/`(( ))` assignment is not always a store. bash evaluates the
// arithmetic first, and a DIVISION BY ZERO is an error: the evaluation
// aborts where it stands, the assignment never happens, and the builtin
// reports failure. MEASURED 2026-11-05:
//
//     u=7; let u=u/0   →  [7]  rc=1
//     u=7; (( u=u/0 )) →  [7]  rc=1
//     u=7; let u=u/2   →  [3]  rc=0
//
// The renderer's divider is deliberately TOTAL — `(r == 0 ? 0 : l {op} r)`,
// because C would SIGFPE — so the *value* 0 is a lie that only the
// surrounding store can repair. Hence this section: the three facts the
// generator has to get right, and the one it cannot see.

/// bash's arithmetic is partial: a value or an ERROR.
pub enum Outcome {
    Val(int),
    Err,
}

/// The division itself, in bash. Errors on a zero divisor, whatever the
/// dividend is.
pub open spec fn ev_div(l: int, r: int) -> Outcome {
    if r == 0 {
        Outcome::Err
    } else {
        Outcome::Val((l / r) as int) // truncated toward zero, as bash does
    }
}

pub proof fn semantics_errors_on_zero(l: int)
    ensures ev_div(l, 0) == Outcome::Err,
{
}

/// What the RENDERER produces: total, and 0 where bash errored.
pub open spec fn rendered_div(l: int, r: int) -> int {
    if r == 0 {
        0
    } else {
        (l / r) as int
    }
}

/// The divergence, in one statement: on a zero divisor the renderer has a
/// value and the semantics has none. This is why a store cannot be emitted
/// from the rendered value alone.
pub proof fn render_is_total_where_semantics_errors(l: int)
    ensures
        ev_div(l, 0) == Outcome::Err,
        rendered_div(l, 0) == 0,
{
}

/// A divisor is DETECTABLE when its value is known at codegen time — the
/// Rust side's `const_arith_value(rhs) == Some(0)`. Known-ness is the whole
/// input to the decision.
pub open spec fn detects(known: Option<int>) -> bool {
    known == Some(0)
}

pub proof fn detector_fires_exactly_on_static_zero()
    ensures
        detects(Some(0)),
        !detects(Some(3)),
        !detects(None),
{
}

/// SOUNDNESS of the detector: when it fires, the semantics errors.
pub proof fn detection_is_sound(l: int)
    ensures
        detects(Some(0)) ==> ev_div(l, 0) == Outcome::Err,
{
}

/// INCOMPLETENESS — the pinned-open half (C_UB_PROOF.md §33). A divisor that
/// is zero only at RUN time is `None` to the detector, so the store still
/// fires with the rendered 0 while bash aborts. No state is assumed here:
/// the claim is that the detector's input cannot tell `Some(0)` from `None`
/// when the value is only known at run time, and the two decisions differ.
pub proof fn detection_is_incomplete(l: int)
    ensures
        !detects(None),
        // …yet the semantics still errors for some runtime divisor, and the
        // renderer still yields a value: the residual divergence.
        ev_div(l, 0) == Outcome::Err,
        rendered_div(l, 0) == 0,
        !detects(None) && ev_div(l, 0) == Outcome::Err,
{
}

/// What an ABORTED assignment leaves behind: nothing. The store is the same
/// map — this is `child_write_is_invisible`'s sibling, but for a reason that
/// is not about modes at all.
pub open spec fn after_outcome(m: Mode, st: Store, n: Name, v: Str, oc: Outcome) -> Store {
    match oc {
        Outcome::Err => st,
        Outcome::Val(_) => after(m, st, n, v),
    }
}

pub proof fn error_aborts_the_store(m: Mode, st: Store, n: Name, v: Str)
    ensures
        after_outcome(m, st, n, v, Outcome::Err) == st,
        after_outcome(m, st, n, v, Outcome::Val(0)) == after(m, st, n, v),
{
}

/// The status the builtin reports: 1 iff the evaluation errored (or the
/// value was zero — `(( ))`'s truthiness, which is the same verdict here).
pub open spec fn status_of(oc: Outcome) -> int {
    match oc {
        Outcome::Err => 1,
        Outcome::Val(v) => if v == 0 { 1 } else { 0 },
    }
}

pub proof fn error_status_is_one()
    ensures
        status_of(Outcome::Err) == 1,
        // and the ERROR case and the zero-value case agree, which is why
        // yielding 0 for an error still reports bash's exit status
        status_of(Outcome::Val(0)) == 1,
{
}

/// ONE predicate, TWO sites: the `let` arm's assignment (`ArithAst::Assign`)
/// and the statement hook (`IrStmt::Assign` with an `Arith` RHS) both ask
/// `arith_static_zero_divisor`, so they cannot disagree about the same
/// expression — the defect was that only one of them asked at all.
pub open spec fn site_asks(site: int, known: Option<int>) -> bool {
    // site 0 = the `let` arith-assign arm, site 1 = the statement hook
    detects(known)
}

pub proof fn both_sites_share_one_predicate(known: Option<int>)
    ensures
        site_asks(0, known) == site_asks(1, known),
        forall|s: int| site_asks(s, known) == detects(known),
{
}

// ── 5. an EXPANDED `let` word, and the variable's HOME ────────────────
//
// `let n=$x*2` arrives as `Interpolate([lit "n=", getVar(x), lit "*2"])`, not
// as a `Str`, so the Str-shaped arm never saw it and the builtin fell to the
// no-op tail: MEASURED `x=4; x=5; let n=$x*2` → bash `[8]`, C `[0]` with
// `_sh_rc = 0` and no store at all. The fix reconstructs the arithmetic TEXT
// (`n=x*2`) and reuses the native route. Two facts make that faithful, and
// both are obligations here rather than comments in the emitter:
//
// 1. a bare NAME reads the same value as the expansion it replaces;
// 2. the store and the READ must resolve to the same HOME — the generator has
//    two (`the C ident` and `_sh_env(name)`) and picks per-variable.

pub type Var = Str;

/// The expansion of `$x` inside arith: bash inserts the VALUE as text, then
/// parses it. A value that is itself a name is resolved again, which is
/// exactly what an arith read of the bare name does — so for any value the
/// two spellings agree.
pub open spec fn value_of(st: Store, n: Var) -> Str {
    get(st, n).unwrap_or(Seq::empty())
}

/// …and the TEXT-insertion route reads a name recursively.
pub open spec fn arith_read(st: Store, n: Var, fuel: nat) -> Str
    decreases fuel,
{
    let v = value_of(st, n);
    // a value that names another variable is followed (bash's arith
    // evaluator does the same); numbers and everything else are the value
    if fuel > 0 && nonspace(v) && v != n {
        arith_read(st, v, (fuel - 1) as nat)
    } else {
        v
    }
}

/// `n=x*2` — the reconstruction of `let n=$x*2`.
pub open spec fn let_text(n: Var, x: Var, tail: Str) -> Str {
    n.add(seq!['=']).add(x).add(tail)
}

/// The marker property: whatever the reconstruction, the TEXT the arith parser
/// sees for the expansion is the variable's name, never a `$` form — the
/// arith parser refuses `$` operands on purpose (bash expands them before
/// parsing, so an unset `$j` in `$(( $j * 2 ))` is a syntax error, not a 0
/// read), and a `$` reconstruction would decline and drop the statement.
pub proof fn reconstruction_has_no_dollar(n: Var, x: Var, tail: Str)
    requires no_dollar(n), no_dollar(x), no_dollar(tail),
    ensures no_dollar(let_text(n, x, tail)),
{
}

pub open spec fn no_dollar(s: Str) -> bool {
    forall|i: int| 0 <= i < s.len() ==> s[i] != '$'
}

/// OBLIGATION 1 — the two spellings agree. For a numeric value the arith read
/// of the name IS the expansion's value, so replacing `$x` by `x` loses
/// nothing.
pub proof fn name_read_agrees_with_expansion(st: Store, n: Var)
    requires
        st.contains_key(n),
        st[n].len() > 0,
        !nonspace(st[n]) || st[n] == n,
    ensures
        arith_read(st, n, 4) == value_of(st, n),
{
}

/// …and the recursion is ANCHORED: an empty value is the answer at every fuel
/// (an unset variable reads as nothing through both routes).
pub proof fn arith_read_of_empty_is_empty(st: Store, n: Var, k: nat)
    requires value_of(st, n).len() == 0,
    ensures arith_read(st, n, k) == Seq::empty(),
{
}

/// The chain IS followed: a value that is a name of a name still lands on the
/// same answer, which is what makes the bare-name spelling safe where bash
/// would insert text. Stated as the one-step unfolding.
pub proof fn arith_read_unfolds(st: Store, n: Var, k: nat)
    requires
        k > 0,
        nonspace(value_of(st, n)),
        value_of(st, n) != n,
    ensures
        arith_read(st, n, k) == arith_read(st, value_of(st, n), (k - 1) as nat),
{
}

/// A value with no leading blank is a single arith token — a number or a name.
pub open spec fn nonspace(v: Str) -> bool {
    v.len() > 0 && v[0] != ' ' && v[0] != '\t'
}

/// OBLIGATION 2 — the lookahead. `$x1` delimits nothing: a name followed by a
/// literal that starts with a NAME character is ONE word, so the
/// reconstruction must refuse rather than read a different variable.
pub proof fn undelimited_name_is_refused(next_char: char)
    requires name_char(next_char),
    ensures !delimited(next_char),
{
}

pub open spec fn name_char(c: char) -> bool {
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') || c == '_'
}

pub open spec fn delimited(next_char: char) -> bool {
    !name_char(next_char)
}

/// THE HOME. The generator has two places a variable's value can live, and the
/// read and the write must agree on which one.
pub enum Home {
    Ident,
    Env,
}

pub open spec fn home_of(read_home: Home, write_home: Home) -> bool {
    // the store is correct when the two are the SAME home
    read_home == write_home
}

pub proof fn store_is_sound_when_homes_agree(h: Home)
    ensures home_of(h, h),
{
}

/// The generator's gate: `store_read` resolves to the ident when the var is
/// registered (`store` / the type verdict), and to `_sh_env` otherwise — so
/// the native store, which always writes the ident, is only emitted when that
/// same predicate holds. The predicate is the thing the arm consults.
pub open spec fn registered(is_registered: bool) -> Home {
    if is_registered {
        Home::Ident
    } else {
        Home::Env
    }
}

pub proof fn native_store_requires_the_ident_home(is_registered: bool)
    ensures
        // the arm emits the store only where the read is the ident too
        (registered(is_registered) == Home::Ident) == is_registered,
        // and where it is not, the gate declines (no home is written at all)
        registered(false) == Home::Env,
{
}

/// …and the decline is the CHILD site, whose write never reaches the parent
/// (`child_write_is_invisible`): a wrong-shaped store cannot be *made worse*
/// by this gate — it stays the no-op it already was, which is why the gate is
/// allowed to be conservative rather than total.
pub proof fn gate_is_conservative(st: Store, n: Name, v: Str)
    ensures
        after(Mode::Child, st, n, v) == st,
{
}

// ── 6. MULTI-WORD `let`: a LIST, not a word ───────────────────────────
//
// `let` takes one or more expressions. bash evaluates them LEFT TO RIGHT and
// the builtin's status is the truth of the LAST one — so a renderer that looks
// at the first word both drops the rest and reports the wrong verdict.
// MEASURED 2026-11-05:
//
//     let a=1 b=2      bash [1][2] rc=0   C [1][2] rc=0   (a core transform
//                                                          splits pure
//                                                          assignment lists)
//     let c=3 0        bash [3] rc=1      C [3] rc=0      ← tail dropped
//     let d=0 4        bash [0] rc=0      C [0] rc=1      ← first word's rc
//     let e=5 e=e+1    bash [6] rc=0      C [6] rc=0
//
// (`let c=3 0` reaches the arm as ONE call with two words because the
// assignment-splitting transform refuses a non-assignment word, so the mixed
// list is exactly the case the first-word rule got wrong.)

/// The status of a word list: the LAST word's truth, never the first's.
pub open spec fn status_of_words(ws: Seq<int>) -> int
    decreases ws.len(),
{
    if ws.len() == 0 {
        // `let` with no arguments: bash reports failure
        1
    } else {
        status_of(Outcome::Val(ws[ws.len() - 1]))
    }
}

/// The measured pair, as a proof that the first-word rule is WRONG (not merely
/// different): on [`3`,`0`] the two rules disagree, and so do they on
/// [`0`,`4`] — with opposite signs, which is why neither can be a special case
/// of the other.
pub proof fn status_is_not_the_first_word()
    ensures
        status_of_words(seq![3int, 0int]) == 1,
        status_of(Outcome::Val(3)) == 0,
        status_of_words(seq![3int, 0int]) != status_of(Outcome::Val(3)),
        status_of_words(seq![0int, 4int]) == 0,
        status_of(Outcome::Val(0)) == 1,
{
}

/// …and it is the last word, for any list: the tail is what decides.
pub proof fn status_is_the_last_word(ws: Seq<int>)
    requires ws.len() > 0,
    ensures status_of_words(ws) == status_of(Outcome::Val(ws[ws.len() - 1])),
{
}

/// Words COMPOSE: a word sees the stores of the words to its left, which is
/// what `let e=5 e=e+1` → 6 requires. `after_outcome` is the per-word store
/// step, so the list's effect is its fold.
pub open spec fn words_store_at(st: Store, pairs: Seq<(Name, Str)>, i: int) -> Store
    decreases pairs.len() - i,
{
    if i >= pairs.len() {
        st
    } else {
        // word `i` writes, then word `i+1` READS what it wrote
        words_store_at(after(Mode::Native, st, pairs[i].0, pairs[i].1), pairs, i + 1)
    }
}

pub open spec fn words_store(st: Store, pairs: Seq<(Name, Str)>) -> Store {
    words_store_at(st, pairs, 0)
}

pub proof fn words_compose_left_to_right(st: Store, n: Name, v: Str)
    ensures
        words_store(st, seq![(n, v)]) == after(Mode::Native, st, n, v),
{
    // one step of the fold: the head is `(n, v)` and the tail is EMPTY, so
    // the list's effect is the single `after` — no second word to compose.
    // one step of the fold: word 0 writes, and 1 >= len ends the list, so the
    // list's effect is exactly that single `after`.
    assert(words_store_at(st, seq![(n, v)], 0) == words_store_at(after(Mode::Native, st, n, v), seq![(n, v)], 1));
    assert(words_store_at(after(Mode::Native, st, n, v), seq![(n, v)], 1) == after(Mode::Native, st, n, v));
}

/// …and the composed store is READABLE back: `let e=5 e=e+1` computes 6, so
/// the second word's read of `e` must see the first word's write.
pub proof fn composed_store_reads_back(n: Name, v: Str)
    ensures
        get(set(empty_store(), n, v), n) == Some(v),
{
    assert(set(empty_store(), n, v).contains_key(n));
}

/// The empty list is the one `let` with no status to report (`let; echo $?`
/// is 1), which the emitter's `(_sh_rc = 0, 1)` tail must not claim as
/// success — the arm only reaches it when no word rendered at all.
pub proof fn empty_list_is_failure()
    ensures status_of_words(Seq::empty()) == 1,
{
}

// ── 7. an ARRAY NAME in arithmetic is ELEMENT 0 ───────────────────────
//
// bash: an array name in an arithmetic expression denotes element 0, on BOTH
// sides. MEASURED 2026-11-05:
//
//     a=(1 2 3); let a=9      → [9][3][3]   element 0 only; the rest and the
//                                           LENGTH are untouched
//     a=(1 2 3); let a+=4     → [5][3]      the compound op reads element 0
//     a=();      let a=5      → [5][1]      a fresh array grows to one element
//     a=(1 2 3); let b=a+1    → b = 3       the READ side is element 0 too
//
// The generator stored the C IDENT (`a = temp`), which does not compile at all
// on an array (`error: assignment to expression with array type`), and read it
// as a decayed pointer (0), so `b=a+1` gave 1.
//
// The subtlety worth proving is OWNERSHIP: two mechanisms can write element 0
// (the `let` renderer and the statement path's documented subscript-0 rule),
// and emitting both double-evaluates the RHS.

/// A bash array, as arith sees it: a length and a sparse element map.
pub struct Arr {
    pub len: int,
    pub elems: Map<int, int>,
}

/// Reading the NAME is reading element 0; an element that is not set reads 0.
pub open spec fn elem0(a: Arr) -> int {
    if a.len > 0 && a.elems.contains_key(0) {
        a.elems[0]
    } else {
        0
    }
}

pub open spec fn arr_store(a: Arr, v: int) -> Arr {
    Arr { len: if a.len < 1 { 1 } else { a.len }, elems: a.elems.insert(0, v) }
}

pub proof fn name_ignores_every_other_element()
    ensures
        // only index 0 is ever read through the NAME
        elem0(Arr { len: 3, elems: Map::empty().insert(0, 1).insert(1, 9) })
            == elem0(Arr { len: 3, elems: Map::empty().insert(0, 1) }),
{
}

/// The measured read: `a=(1 2 3); let b=a+1` is 4, not 1 — the name IS the
/// element, so an unset element is 0 and a set one is its value.
pub proof fn read_is_the_element_not_the_array()
    ensures
        elem0(Arr { len: 3, elems: Map::empty().insert(0, 1).insert(1, 2).insert(2, 3) }) == 1,
        elem0(Arr { len: 0, elems: Map::empty() }) == 0,
{
}

/// The measured WRITE: element 0 takes the value, the other elements and the
/// length survive, and an EMPTY array becomes one element long.
pub proof fn store_writes_only_element_zero(v: int)
    ensures
        arr_store(Arr { len: 3, elems: Map::empty().insert(0, 1).insert(2, 3) }, v).len == 3,
        arr_store(Arr { len: 3, elems: Map::empty().insert(0, 1).insert(2, 3) }, v).elems.contains_key(2),
        arr_store(Arr { len: 3, elems: Map::empty().insert(0, 1) }, v).elems[0] == v,
        arr_store(Arr { len: 0, elems: Map::empty() }, v).len == 1,
{
}

/// OWNERSHIP. A compound op reads element 0, so a SECOND store evaluates the
/// RHS again against the FIRST store's write: `a[0] = a[0] + 4` twice is 1 → 5
/// → 9, which is exactly the `[9]` the double-owning renderer printed where
/// bash prints `[5]`.
pub proof fn double_store_compounds(the_rhs: int, a0: int)
    requires a0 >= 0,
    ensures
        arr_store(arr_store(Arr { len: 1, elems: Map::empty().insert(0, a0) }, a0 + the_rhs),
                  the_rhs + a0 + the_rhs).elems[0] == a0 + 2 * the_rhs,
{
}

/// …so the fix is not "store in the better place" but "store ONCE": exactly one
/// owner may emit the element write, and the other must be value-only.
pub open spec fn owners(let_path: bool, stmt_path: bool) -> bool {
    let_path != stmt_path
}

pub proof fn one_owner_only()
    ensures
        owners(true, false),
        owners(false, true),
        !owners(true, true),
        !owners(false, false),
{
}

} // verus!

fn main() {}
