// Standalone model of `unset` on a SCALAR and the STORAGE-KIND demotion it
// forces (sh2perl/src/c_backend.rs: the `unset` arm of
// `demote_stringy_ints`' walk).
//
// THE DENOTATIONAL EXTENSION (this file): a shell variable is NOT a value —
// it is a PARTIAL map, i.e. an `Option<Str>`. `None` is "unset", `Some("")`
// is "set to the empty string", and bash tells them apart. The C generator
// stores a scalar in one of two ways, and only one of them can carry the
// distinction at all:
//
//   STRING storage   `char *x`    — carries a Str; `unset` writes "" (or
//                                    free+NULL, which reads as "")
//   INT storage      `long long x` — carries a NUMBER. There is no spelling
//                                    of "unset" in it: the only states are
//                                    the numbers. `unset` emitted NOTHING.
//
// MEASURED 2026-11-04 (`x=5; unset x` as the first line of each):
//     echo "[${x:-gone}]"   bash `[gone]`   C `[5]`
//     echo "[$x]"           bash `[]`       C `[5]`
//     echo $((x+1))         bash `1`        C `6`
//     echo "[${x-unset}]"   bash `[unset]`  C `[5]`
// — every face wrong, because the int store still held 5. The fix is one
// line in the type analysis: a scalar that is `unset` ANYWHERE is demoted
// out of Int (string storage), which is the same argument the STRINGLY-WINS
// demotion already rests on ("an Int decl truncates the string pointer").
// Numeric USE still works — the arithmetic path parses the text — so
// `x="5"; $((x+1))` is still 6 and `x="5"; unset x; $((x+1))` is 1.
//
// THE QUOTIENT. String storage cannot represent `None` separately from
// `Some("")`: both are the same bytes. `quot` below is that identification,
// and the interesting results are about what it PRESERVES and what it
// provably cannot:
//
//   q_plain / q_colon_default / q_colon_plus / q_len / q_arith
//        agree with bash for EVERY state — these faces test EMPTINESS, which
//        the quotient keeps, so `unset` is exact for them (the fix targets
//        exactly this set, and this is why it is a demotion and not a bit)
//   q_dash_default / q_dash_plus
//        agree when the state is `None` (measured: `${x-w}` after `unset`
//        prints w) and DISAGREE on `Some("")`. That is not an oversight:
//        `no_face_separates` proves NO function of the quotient can give
//        different answers for the two states, while bash's `${x-w}` does.
//        So the residual is an IMPOSSIBILITY for string storage, stated as
//        such rather than left as a bug to rediscover. (MEASURED, and
//        PRE-EXISTING — `x=""; echo "[${x-w}]"` printed `[w]` before this
//        round and still does; no `unset` is involved.)
//
// Obligations:
//   q_plain, q_len, q_arith, q_colon_default, q_colon_plus   the unset-exact
//                                                            faces (the fix)
//   q_dash_default_unset / q_dash_plus_unset                 the half of the
//                                                            no-colon faces
//                                                            that the fix
//                                                            makes right
//   no_face_separates                                        the impossibility
//   dash_gap_is_the_set_to_empty_case                        its witness
//   pref_int_keeps_the_value / pref_int_arith                the pre-fix
//                                                            divergence, on
//                                                            the measured
//                                                            numbers
//   demote_is_a_storage_kind_change                          what the fix does
//                                                            (and does NOT: no
//                                                            element, no array)
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

/// A variable name, abstracted to an int: the demotion predicate never looks
/// inside one.
pub type Name = int;

/// A shell variable's state: `None` = unset, `Some(v)` = set (v may be "").
pub type Var = Option<Str>;

pub open spec fn empty() -> Str {
    Seq::empty()
}

pub open spec fn s5() -> Str {
    seq!['5']
}

pub open spec fn gone() -> Str {
    seq!['g', 'o', 'n', 'e']
}

pub open spec fn unset_w() -> Str {
    seq!['u', 'n', 's', 'e', 't']
}

// ---------------------------------------------------------------------
// The quotient: what string storage can carry
// ---------------------------------------------------------------------

/// `unset` and `set-to-empty` land on the same bytes. Everything the C
/// generator can compute is a function of this value.
pub open spec fn quot(v: Var) -> Str {
    match v {
        Some(s) => s,
        None => empty(),
    }
}

/// THE identification, stated once: the two states string storage cannot
/// tell apart.
proof fn quot_identifies_unset_and_empty()
    ensures
        quot(None) == quot(Some(empty())),
        quot(Some(s5())) == s5(),
        quot(None) == empty(),
{
}

// ---------------------------------------------------------------------
// The faces, on the DENOTATION (bash) …
// ---------------------------------------------------------------------

/// `$x`
pub open spec fn f_plain(v: Var) -> Str {
    quot(v)
}

/// `${x:-w}` — the default applies when unset OR empty
pub open spec fn f_colon_default(v: Var, w: Str) -> Str {
    match v {
        Some(s) => if s.len() == 0 { w } else { s },
        None => w,
    }
}

/// `${x-w}` — the default applies ONLY when unset
pub open spec fn f_dash_default(v: Var, w: Str) -> Str {
    match v {
        Some(s) => s,
        None => w,
    }
}

/// `${x:+w}` — the alternative applies when set and non-empty
pub open spec fn f_colon_plus(v: Var, w: Str) -> Str {
    match v {
        Some(s) => if s.len() == 0 { empty() } else { w },
        None => empty(),
    }
}

/// `${x+w}` — the alternative applies whenever set
pub open spec fn f_dash_plus(v: Var, w: Str) -> Str {
    match v {
        Some(_) => w,
        None => empty(),
    }
}

/// `${#x}`
pub open spec fn f_len(v: Var) -> int {
    quot(v).len() as int
}

/// `$((x+…))` — an unset variable is 0 in arithmetic
pub open spec fn f_arith(v: Var) -> int {
    match v {
        Some(s) => atoi(s),
        None => 0,
    }
}

/// Decimal TEXT to value, for the SINGLE-DIGIT and empty texts this model
/// needs (`"5"` and `""`); the general parse is `c_int_helpers.rs`'s
/// (`isint31`/`atoll` side). `""` is 0 — exactly what `atoll` does, and the
/// reason an unset variable is 0 in arithmetic.
pub open spec fn atoi(s: Str) -> int {
    if s.len() == 1 && '0' <= s[0] && s[0] <= '9' {
        (s[0] as int) - ('0' as int)
    } else {
        0
    }
}

// ---------------------------------------------------------------------
// …and on the EMISSION (what the C generator can compute: a function of
// `quot(v)` alone)
// ---------------------------------------------------------------------

pub open spec fn c_plain(x: Str) -> Str {
    x
}

pub open spec fn c_colon_default(x: Str, w: Str) -> Str {
    if x.len() == 0 {
        w
    } else {
        x
    }
}

pub open spec fn c_colon_plus(x: Str, w: Str) -> Str {
    if x.len() == 0 {
        empty()
    } else {
        w
    }
}

/// The renderer's `${x-w}` shape. MEASURED: it is the COLON shape — it
/// tests emptiness, because emptiness is all it has.
pub open spec fn c_dash_default(x: Str, w: Str) -> Str {
    if x.len() == 0 {
        w
    } else {
        x
    }
}

pub open spec fn c_dash_plus(x: Str, w: Str) -> Str {
    if x.len() == 0 {
        empty()
    } else {
        w
    }
}

pub open spec fn c_len(x: Str) -> int {
    x.len() as int
}

pub open spec fn c_arith(x: Str) -> int {
    atoi(x)
}

// ---------------------------------------------------------------------
// 1. The faces the fix makes EXACT — for every state, by construction,
//    because each of them reads only through `quot`
// ---------------------------------------------------------------------

proof fn q_plain(v: Var)
    ensures
        c_plain(quot(v)) == f_plain(v),
{
}

proof fn q_len(v: Var)
    ensures
        c_len(quot(v)) == f_len(v),
{
}

proof fn q_arith(v: Var)
    ensures
        c_arith(quot(v)) == f_arith(v),
{
}

proof fn q_colon_default(v: Var, w: Str)
    ensures
        c_colon_default(quot(v), w) == f_colon_default(v, w),
{
}

proof fn q_colon_plus(v: Var, w: Str)
    ensures
        c_colon_plus(quot(v), w) == f_colon_plus(v, w),
{
}

// ---------------------------------------------------------------------
// 2. The no-colon faces: right for UNSET (which is what the fix is for),
//    and impossible to complete
// ---------------------------------------------------------------------

proof fn q_dash_default_unset(w: Str)
    ensures
        c_dash_default(quot(None), w) == f_dash_default(None, w),
        f_dash_default(None, w) == w,
{
}

proof fn q_dash_plus_unset(w: Str)
    ensures
        c_dash_plus(quot(None), w) == f_dash_plus(None, w),
        f_dash_plus(None, w) == empty(),
{
}

/// THE IMPOSSIBILITY. `quot` maps `None` and `Some("")` to the SAME bytes, so
/// no function that reads only the quotient can tell those states apart. Any
/// `${x-w}` implementation on string storage must therefore fail on one of
/// them — and bash's answer differs, so it fails. This is why the residual
/// is documented rather than fixed, and why the fix could not have been
/// "emit something else in the unset arm". (Stated for an arbitrary `g` over
/// the quotient rather than for a `spec_fn` parameter: the point is that the
/// ONLY input such a `g` can have is `quot`.)
proof fn no_face_separates()
    ensures
        forall|g: spec_fn(Str) -> Str|
            #[trigger] g(quot(None)) == g(quot(Some(empty()))),
{
    quot_identifies_unset_and_empty();
}

/// …and the concrete instance: the two states bash distinguishes, the C
/// collapse. MEASURED (`x=""; echo "[${x-w}]"` → C `[w]`, bash `[]`).
proof fn dash_gap_is_the_set_to_empty_case(w: Str)
    requires
        w.len() > 0,
    ensures
        f_dash_default(None, w) != f_dash_default(Some(empty()), w),
        c_dash_default(quot(None), w) == c_dash_default(quot(Some(empty())), w),
        f_dash_default(None, w) == w,
        f_dash_default(Some(empty()), w) == empty(),
{
    quot_identifies_unset_and_empty();
}

// ---------------------------------------------------------------------
// 3. The PRE-FIX divergence — an int store keeps the VALUE across `unset`
// ---------------------------------------------------------------------

/// The pre-fix representation of a number: there is no `None`, so an unset
/// variable is whatever number was last stored.
pub open spec fn pref_store(v: Var) -> Str {
    match v {
        Some(s) => s,
        None => s5(), // the stale 5 the measured program kept
    }
}

proof fn pref_int_keeps_the_value()
    ensures
        // bash says `gone`
        f_colon_default(None, gone()) == gone(),
        // the pre-fix C said `5`
        c_colon_default(pref_store(None), gone()) == s5(),
        // and they differ — the measured `[5]` vs `[gone]`
        c_colon_default(pref_store(None), gone())
            != f_colon_default(None, gone()),
        // `$x` likewise: bash `[]`, pre-fix C `[5]`
        c_plain(pref_store(None)) == s5(),
        f_plain(None) == empty(),
{
}

proof fn pref_int_arith()
    ensures
        // `$((x+1))`: bash 1 (unset is 0), pre-fix C 6 (the stale value)
        c_arith(pref_store(None)) == 5,
        f_arith(None) == 0,
        c_arith(pref_store(None)) != f_arith(None),
{
    assert(atoi(s5()) == 5) by (compute);
}

// ---------------------------------------------------------------------
// 4. What the fix IS: a storage-KIND change decided by one predicate over
//    the program, and what it is NOT (a storage-CLASS change)
// ---------------------------------------------------------------------

/// The demotion predicate, exactly as `demote_unset_scalars` implements it:
/// the program's Int scalars, minus the ones it unsets, minus the ones it
/// hands to a NUMERIC-ONLY mutation builtin.
///
///  `unset_names`  a plain-scalar `unset x` (not `x[k]`, not `-v/-f/-n`)
///  `num_mut`      `let x=…` / `printf -v x …` — builtins whose only
///                 mutation arm is numeric, so a string-stored target has no
///                 store shape of its own and demoting it only MOVES the
///                 failure (MEASURED on the first cut of this fix: `let x=x+1`
///                 on the demoted var crashed at teardown, `printf -v x %d 9`
///                 did not compile — see `unset_scalar_census`)
/// The RETAINED Int set (so "demoted" means NOT a member).
pub open spec fn int_after(
    unset_names: Set<Name>,
    num_mut: Set<Name>,
    is_int: Set<Name>,
) -> Set<Name> {
    is_int.difference(unset_names.difference(num_mut))
}

/// The demotion is exactly "Int minus (unset minus numeric-only-mutation)":
/// a name the program does not unset keeps Int storage — the change is inert
/// everywhere else (MEASURED: 3 of 651 corpus sources are affected at all) —
/// and a name it does unset loses it (the fix). Nothing is ever ADDED, so the
/// pass cannot resurrect a verdict the stringy demotion removed.
proof fn demote_is_a_kind_change()
    ensures
        forall|u: Set<Name>, m: Set<Name>, i: Set<Name>, n: Name| #[trigger]
            int_after(u, m, i).contains(n)
                == (i.contains(n) && !(u.contains(n) && !m.contains(n))),
        forall|u: Set<Name>, m: Set<Name>, i: Set<Name>|
            int_after(u, m, i).subset_of(i),
{
}

/// The two cases that matter, stated on the census: an unset, never
/// numerically-mutated scalar IS demoted (the fix fires), a numeric-only
/// mutated one is NOT (the exception is not decorative), and an unrelated Int
/// scalar is untouched.
proof fn demote_hits_the_right_names()
    ensures
        // the fix fires: an unset, never numerically-mutated scalar is DEMOTED
        !int_after(set![0int], Set::empty(), set![0int, 1int]).contains(0),
        // the exception holds: a numeric-only-mutated scalar KEEPS Int
        int_after(set![0int], set![0int], set![0int, 1int]).contains(0),
        // and an unrelated Int scalar is untouched
        int_after(set![0int], Set::empty(), set![0int, 1int]).contains(1),
{
}

/// …and it is NOT a storage-CLASS change: an ELEMENT unset (`n[i]`, a key in
/// the target name) and a FLAG (`-v`/`-f`/`-n`) are not scalar unsets, so
/// they must not demote the scalar. Modelled as the two names the collector
/// skips, `elem_name` and `flag_name`.
pub open spec fn elem_name() -> Name {
    0
}

pub open spec fn flag_name() -> Name {
    1
}

proof fn element_and_flag_are_not_scalar_unsets()
    ensures
        // the collector skips them, so they never appear in `unset_names`, so
        // their Int storage is untouched
        int_after(Set::empty(), Set::empty(), set![elem_name(), flag_name()])
            .contains(elem_name()),
        int_after(Set::empty(), Set::empty(), set![elem_name(), flag_name()])
            .contains(flag_name()),
{
}

// ---------------------------------------------------------------------
// 4. THE CENSUS WALK — why a missing arm is a SOUNDNESS hole, not a
//    missed optimisation
// ---------------------------------------------------------------------
//
// Section 3 models the DECISION (`int_after`). This section models the
// INPUT to that decision: the walk that decides which names are read through
// an emptiness face at all. The two are only as good as each other — a
// perfect `int_after` fed an empty face set demotes nothing.
//
// MEASURED 2026-11-05. `demote_unset_scalars` classifies a var as
// POSSIBLY-UNSET when an emptiness face reads it and no assignment to it is
// unconditional. The face collector was a hand-written `match` over
// `IrStmt` with a `_ => {}` fallthrough, and it had no `Output` arm. But
// `printf '%s' "${x:-}"` lowers to `IrStmt::Output { value }`, not to
// `IrStmt::Expr(Call)`, so the collector saw NO face, `x` was never
// considered possibly-unset, and it stayed Int-homed:
//
//     r=$(echo hi) || { x=$?; }
//     printf "%s=%s\n" x "${x:-}"       bash `x=`      C `x=0`
//
// (`examples/parse-dollar-paren-or.sh`, a corpus FAIL.) The fix is the walk
// itself: `for_each_stmt_expr` lists every variant, so the `_` arm is gone
// and a new variant is a compile error rather than a silent hole.

/// A tiny expression grammar: only the shapes the face census distinguishes.
pub enum Ix {
    /// a literal word — no reads
    Lit(Str),
    /// an EMPTINESS face on a name (`${x:-}`, `${x:+}`, `${#x}`)
    Face(Name),
    /// a plain value read (`$x`) — NOT a face
    Plain(Name),
    /// a word list / interpolation (a `printf` argument list), already
    /// reduced to the names its faces mention — so the model needs no
    /// nested grammar and no recursion between the two walks
    Arr(Seq<Name>),
}

/// A tiny statement grammar: the three carriers that matter.
pub enum St {
    /// `Output { value }` — what `printf`/`echo` lower to
    Out(Ix),
    /// `Expr(e)` — a bare expression statement
    Expr(Ix),
    /// `Assign { targets, expr }`
    Asg(Name, Ix),
}

/// Membership, not a set: the questions below are all "does the walk see
/// THIS name", and a boolean keeps every obligation a direct unfolding.
pub open spec fn word_sees(e: Ix, n: Name) -> bool {
    match e {
        Ix::Lit(_) => false,
        Ix::Face(m) => m == n,
        Ix::Plain(_) => false,
        Ix::Arr(names) => exists|i: int| 0 <= i < names.len() && #[trigger] names[i] == n,
    }
}

/// The walk as the FIX implements it, on one statement: every carrier is
/// visited (`for_each_stmt_expr`, exhaustive over `IrStmt` — the `_` arm is
/// gone, so a new variant is a compile error rather than a silent hole).
pub open spec fn sees(s: St, n: Name) -> bool {
    match s {
        St::Out(e) => word_sees(e, n),
        St::Expr(e) => word_sees(e, n),
        St::Asg(_, e) => word_sees(e, n),
    }
}

/// The walk as it was BEFORE the fix: the same, minus the `Output` arm — the
/// `_ => {}` fallthrough that swallowed it.
pub open spec fn sees_narrow(s: St, n: Name) -> bool {
    match s {
        St::Out(_) => false, // ← the missing arm
        St::Expr(e) => word_sees(e, n),
        St::Asg(_, e) => word_sees(e, n),
    }
}

/// The statements of the measured program: the conditional assignment (which
/// is why `x` is not certainly-set) and the `printf` that READS the face.
pub open spec fn vc_printf() -> St {
    St::Out(Ix::Face(0int))
}

/// …and its `echo` twin, which is an `Expr(Call)` carrier: the SAME word, a
/// different carrier, and therefore visible even before the fix. This is why
/// the defect needed a `printf` to appear.
pub open spec fn vc_echo() -> St {
    St::Expr(Ix::Face(0int))
}

/// …and `printf "%s" x "$x"`, which carries a PLAIN read: visible to both
/// walks, and correctly not a face either way.
pub open spec fn vc_printf_plain() -> St {
    St::Out(Ix::Plain(0int))
}

/// THE BUG, as a theorem: the narrow walk reports NO face for the `printf`
/// statement, while the identical word through `echo` is seen. The difference
/// is the CARRIER, not the word — which is exactly what makes this a walk
/// defect and not a semantic one.
proof fn narrow_walk_misses_the_output_face()
    ensures
        sees_narrow(vc_printf(), 0int) == false,
        sees_narrow(vc_echo(), 0int),
        // and the word itself is face-carrying in both
        word_sees(Ix::Face(0int), 0int),
        // …so the narrow walk is NOT carrier-independent:
        sees_narrow(vc_printf(), 0int) != sees_narrow(vc_echo(), 0int),
{
}

/// THE FIX, as a theorem: the exhaustive walk sees the face through BOTH
/// carriers, and agrees with the word for every one of them.
proof fn full_walk_sees_it_through_every_carrier()
    ensures
        sees(vc_printf(), 0int),
        sees(vc_echo(), 0int),
        sees(vc_printf(), 0int) == word_sees(Ix::Face(0int), 0int),
        sees(vc_echo(), 0int) == word_sees(Ix::Face(0int), 0int),
        // the exhaustive walk IS carrier-independent
        sees(vc_printf(), 0int) == sees(vc_echo(), 0int),
{
}

/// A `$x` read is NOT a face: it does not test set-ness, so it must never
/// demote (`x=5; echo "$x"` stays Int — Int is the fast path, and the census
/// only spends string storage where bash can tell a difference). The pair
/// differs ONLY in which read it carries.
proof fn plain_read_is_not_a_face()
    ensures
        word_sees(Ix::Plain(0int), 0int) == false,
        sees(vc_printf_plain(), 0int) == false,
        sees(vc_printf(), 0int),
{
}

/// The composed claim: with the exhaustive walk the name is classified
/// POSSIBLY-UNSET, hence DEMOTED (`int_after` drops it), hence read from the
/// string store — where section 1 proved `q_colon_default` exact. With the
/// narrow walk the name keeps Int, and the read answers the NUMBER: on the
/// unset state bash's `${x:-}` is the empty string, while an Int-homed store
/// can only offer `"0"`. Both halves are the MEASURED output (`x=` vs `x=0`).
proof fn exhaustive_walk_is_what_makes_the_demotion_sound()
    ensures
        // the census the fix produces: x IS a face read
        sees(vc_printf(), 0int),
        // so the possibly-unset set is `{x}`, and the demotion drops it
        int_after(set![0int], Set::empty(), set![0int]).contains(0int) == false,
        // the census the bug produced was EMPTY, so x kept Int
        sees_narrow(vc_printf(), 0int) == false,
        int_after(Set::empty(), Set::empty(), set![0int]).contains(0int),
        // the demoted read is the string face, and it is exact (section 1)
        c_colon_default(quot(None), empty()) == f_colon_default(None, empty()),
        // the Int-homed read can only be the number, and bash's is empty
        f_colon_default(None, empty()) == empty(),
        c_colon_default(seq!['0'], empty()) == seq!['0'],
        c_colon_default(seq!['0'], empty()) != f_colon_default(None, empty()),
{
}

// ---------------------------------------------------------------------
// 5. SET-NESS vs EMPTINESS — the no-colon family (`-`, `+`, `=`)
// ---------------------------------------------------------------------
//
// Sections 1–4 are about ONE distinction (unset vs set), and §1 proved the
// string store can only carry its EMPTINESS projection. This section is about
// the OTHER distinction bash makes, which the same projection erases:
//
//     ${v-w}    w unless v is SET        (set-to-empty -> v, i.e. "")
//     ${v+w}    w iff v is SET           (set-to-empty -> w)
//     ${v=w}    like ${v-w} AND assigns  (set-to-empty -> v, no assign)
//     ${v:-w}   w unless v is NON-EMPTY  (set-to-empty -> w)
//     ${v:+w}   w iff v is NON-EMPTY     (set-to-empty -> "")
//
// The colon is the whole difference: `-`/`+`/`=` test PRESENCE, `:-`/`:+`
// test NON-EMPTINESS. MEASURED 2026-11-05 against the shipped C:
//
//     x=""; printf "[%s]" "${x-w}"    bash []      C [w]      WRONG
//     x=""; printf "[%s]" "${x+w}"    bash [w]     C []       WRONG
//     unset x; printf "[%s]" "${x=w}" bash [w]     C []       WRONG (no assign)
//     x=""; printf "[%s]" "${x:-w}"   bash [w]     C [w]      right
//
// The data is not the problem. A heap scalar here is `char *x = NULL`, and
// `unset` is `free(x); x = NULL` — the pointer IS a presence bit, and the
// `-` arm already emits `(x ? x : w)`, i.e. exactly `c_presence_dash` below.
// The problem is that the OPERATOR never arrives: the parser folds `${x-w}`
// into `${x:-w}` (`param(":-", "x", "w")` for both — measured on the shIR),
// so the arm that would be exact is unreachable and the arm that runs is the
// emptiness one. `${x=w}` goes further and arrives as `param("", "x=w")` — a
// "variable" named `x=w`, which renders as a child-shell lookup.
//
// So the residual here was an IMPOSSIBILITY OF THE COLLAPSED IR, not of the
// storage — the mirror image of §1's `no_face_separates`. Section 5 proves
// both halves: the presence renderings ARE exact (so a fix exists), and no
// rendering of the collapsed operator CAN be (so the fix needs the colon,
// which is an AST/contract change, not a renderer change).
//
// THAT CHANGE HAS LANDED (2026-11-05). The parser stopped folding `${v-w}`
// into `${v:-w}` and `${v=w}` into a variable named `v=w`:
// ParameterExpansionOperator gained `DefaultValueNoColon` /
// `AssignDefaultNoColon`, the shIR encoder emits distinct `-` / `=` ops
// (ast_words` `+` spelling already existed), and the C renderer's `-`/`+`/`=`
// arms test the RAW heap pointer instead of `store_ref`'s null-guarded read
// (which erases presence) or the `[0]` emptiness test. MEASURED after: all
// fifteen state × face combinations (unset / set-to-empty / set-non-empty ×
// `-`/`+`/`=`/`:-`/`:+`) are byte-identical to bash, and `${v=w}` now stores
// the default (`unset x; ${x=w}; echo "$x"` → `w`, was `` with x still
// unset). §5's `no_renderer_of_the_collapsed_ir_is_exact` is kept as the
// ARGUMENT for that change; `every_spelling_is_exact` is the result.

/// bash's SET-NESS faces.
pub open spec fn set_dash(v: Var, w: Str) -> Str {
    match v {
        None => w,
        Some(s) => s,
    }
}

pub open spec fn set_plus(v: Var, w: Str) -> Str {
    match v {
        None => empty(),
        Some(_) => w,
    }
}

/// `${v=w}` — the same VALUE as `${v-w}`; the assignment it also performs is
/// a side effect this spec leaves out (it is exactly "the None arm writes w",
/// which the C store arm would do; the measured defect is the value AND the
/// missing write, both the same root).
pub open spec fn set_assign(v: Var, w: Str) -> Str {
    set_dash(v, w)
}

/// The SHIPPED rendering: the emptiness test, which is the `:-` denotation.
pub open spec fn empty_dash(x: Str, w: Str) -> Str {
    if x.len() > 0 {
        x
    } else {
        w
    }
}

pub open spec fn empty_plus(x: Str, w: Str) -> Str {
    if x.len() > 0 {
        w
    } else {
        empty()
    }
}

/// The PRESENCE rendering — what the `-` arm means on a heap scalar whose
/// NULL is the presence bit, and what the `+`/`=` arms would mean.
pub open spec fn presence_dash(p: Var, w: Str) -> Str {
    set_dash(p, w)
}

pub open spec fn presence_plus(p: Var, w: Str) -> Str {
    set_plus(p, w)
}

/// (a) THE FIX EXISTS: with presence, all three faces are exact for EVERY
/// state — including the set-to-empty state that the emptiness test gets
/// wrong. This is what makes the residual a reachability problem.
proof fn presence_renderings_are_exact(v: Var, w: Str)
    ensures
        presence_dash(v, w) == set_dash(v, w),
        presence_plus(v, w) == set_plus(v, w),
        presence_dash(Some(empty()), w) == empty(),
        presence_plus(Some(empty()), w) == w,
        presence_dash(None, w) == w,
        presence_plus(None, w) == empty(),
{
}

/// (b) THE SHIPPED RENDER IS WRONG, on the measured inputs.
proof fn emptiness_render_fails_on_set_empty(w: Str)
    requires
        w.len() > 0,
    ensures
        // `${x-w}` on a SET-TO-EMPTY x: bash keeps "", the emptiness test
        // substitutes w  (measured `[]` vs `[w]`)
        empty_dash(empty(), w) != set_dash(Some(empty()), w),
        empty_dash(empty(), w) == w,
        set_dash(Some(empty()), w) == empty(),
        // `${x+w}`: bash substitutes w, the emptiness test substitutes ""
        // (measured `[w]` vs `[]`)
        empty_plus(empty(), w) != set_plus(Some(empty()), w),
        empty_plus(empty(), w) == empty(),
        set_plus(Some(empty()), w) == w,
        // `${x=w}`: same value divergence (measured `[a]` vs `[]`, and the
        // assignment never happens)
        empty_dash(empty(), w) != set_assign(Some(empty()), w),
{
}

/// …and WHY IT LOOKED RIGHT: on the UNSET half the emptiness test and the
/// presence test agree, so every probe that only unsets passes. The two
/// states `None` and `Some("")` are what §1's `quot` identifies, and this is
/// the second face family that the identification breaks — the first being
/// `${v-w}` itself in §1's `dash_gap_is_the_set_to_empty_case`.
proof fn emptiness_is_right_on_the_unset_half(w: Str)
    ensures
        empty_dash(empty(), w) == set_dash(None, w),
        empty_plus(empty(), w) == set_plus(None, w),
{
}

/// (c) THE IMPOSSIBILITY, AT THE IR. The shIR's operator vocabulary is what
/// the parser hands the renderers; `collapse` is the fold it performs on the
/// no-colon default/assign operators (MEASURED: `${x-w}` and `${x:-w}` both
/// arrive as `param(":-", "x", "w")`).
pub enum Op {
    ColonDash,
    Dash,
    ColonPlus,
    Plus,
    ColonAssign,
    /// `${v=w}` — arrives as `param("", "x=w")`, a *name* that is not an
    /// identifier at all; modelled as its own (collapsed) spelling.
    Assign,
    /// `${v?w}` — errors only when v is UNSET.
    Question,
    /// `${v:?w}` — errors when v is unset OR empty.
    ColonQuestion,
}

pub open spec fn collapse(o: Op) -> Op {
    match o {
        Op::Dash => Op::ColonDash,
        // `${v=w}` does not even reach a default arm: it became a lookup of
        // the name `x=w`, which no renderer can read as an assignment. The
        // collapse is modelled as "the spelling is lost" — it maps onto the
        // same empty-op form the shipped IR uses.
        Op::Assign => Op::ColonDash,
        Op::ColonAssign => Op::ColonDash,
        // `${v?w}` used to collapse the same way (both arrived as `:?`),
        // which turned a runnable program into an abort — see
        // `question_keeps_the_colon`.
        Op::Question => Op::ColonQuestion,
        other => other,
    }
}

/// `${x-w}` and `${x:-w}` are the SAME input to every renderer: this is the
/// erasure, stated as the equality the parser produces.
proof fn collapse_loses_the_colon()
    ensures
        collapse(Op::Dash) == collapse(Op::ColonDash),
        // while the `+` family ALREADY carries the distinction (measured:
        // `${x+w}` arrives as op `+`, `${x:+w}` as op `:+`), which is why
        // half of this family is fixable in the renderer alone
        collapse(Op::Plus) != collapse(Op::ColonPlus),
        collapse(Op::Plus) == Op::Plus,
{
}

/// …so NO renderer of the collapsed IR can be exact for `${x-w}`: any `g`
/// exact for the `:-` denotation (its only visible input is the collapsed
/// operator) is necessarily wrong for the `-` denotation on `Some("")`.
/// This is why the residual is not a bug to patch in the C arm — the arm
/// already contains the right expression — and why the fix is an AST/contract
/// change (a new operator spelling), which the marketplace rules make an
/// OFFER across backends rather than a one-backend edit.
proof fn no_renderer_of_the_collapsed_ir_is_exact(
    g: spec_fn(Op, Var, Str) -> Str,
)
    requires
        // `g` is an EXACT renderer of `${v:-w}` — the only operator it can
        // see for both spellings, because the parser collapsed them.
        forall|v: Var, w: Str| #[trigger] g(collapse(Op::ColonDash), v, w) == f_colon_default(v, w),
    ensures
        exists|v: Var, w: Str| #[trigger] g(collapse(Op::Dash), v, w) != set_dash(v, w),
{
    // witness: set-to-empty with a non-empty default
    assert(g(collapse(Op::Dash), Some(empty()), s5()) != set_dash(Some(empty()), s5())) by {
        assert(collapse(Op::Dash) == collapse(Op::ColonDash));
        assert(g(collapse(Op::Dash), Some(empty()), s5()) == f_colon_default(Some(empty()), s5()));
        assert(f_colon_default(Some(empty()), s5()) == s5());
        assert(set_dash(Some(empty()), s5()) == empty());
        assert(s5() != empty());
    }
}

/// (d) THE RESULT, after the spelling landed. With `-`/`+`/`=` arriving as
/// THEIR OWN ops (not folded into `:-`/`:=`), a renderer can be exact for
/// every member of the family at once — which the previous theorem says is
/// impossible on the collapsed IR. `render_spelled` is the shipped mapping:
/// `param_call`'s `-`/`+`/`=` arms test the heap pointer, `:-`/`:+` keep the
/// emptiness test.
pub open spec fn render_spelled(op: Op, v: Var, w: Str) -> Str {
    match op {
        Op::ColonDash => f_colon_default(v, w),
        Op::ColonPlus => f_colon_plus(v, w),
        Op::Dash => set_dash(v, w),
        Op::Plus => set_plus(v, w),
        Op::Assign => set_assign(v, w),
        Op::ColonAssign => f_colon_default(v, w),
        // value-or-error spelling modelled by `spelling_errors` below
        Op::Question | Op::ColonQuestion => v_to_str(v),
    }
}

/// The value a Var denotes when it IS set (the `?` faces never substitute `w`
/// — they abort instead).
pub open spec fn v_to_str(v: Var) -> Str {
    match v {
        None => empty(),
        Some(s) => s,
    }
}

/// `${v?w}` errors iff v is UNSET; `${v:?w}` errors iff v is unset OR empty.
pub open spec fn question_errors(v: Var) -> bool {
    match v {
        None => true,
        Some(_) => false,
    }
}

pub open spec fn colon_question_errors(v: Var) -> bool {
    match v {
        None => true,
        Some(s) => s.len() == 0,
    }
}

pub open spec fn spelling_errors(op: Op, v: Var) -> bool {
    match op {
        Op::Question => question_errors(v),
        Op::ColonQuestion => colon_question_errors(v),
        _ => false,
    }
}

proof fn every_spelling_is_exact(v: Var, w: Str)
    ensures
        // each op denotes ITS OWN face — no two are interchangeable
        render_spelled(Op::Dash, v, w) == set_dash(v, w),
        render_spelled(Op::Plus, v, w) == set_plus(v, w),
        render_spelled(Op::Assign, v, w) == set_assign(v, w),
        render_spelled(Op::ColonDash, v, w) == f_colon_default(v, w),
        render_spelled(Op::ColonPlus, v, w) == f_colon_plus(v, w),
        // …and the pair that collapsed is now DISTINGUISHED on the state
        // that used to be unreachable: set-to-empty.
        render_spelled(Op::Dash, Some(empty()), s5()) == empty(),
        render_spelled(Op::ColonDash, Some(empty()), s5()) == s5(),
        render_spelled(Op::Dash, Some(empty()), s5())
            != render_spelled(Op::ColonDash, Some(empty()), s5()),
        render_spelled(Op::Plus, Some(empty()), s5()) == s5(),
        render_spelled(Op::ColonPlus, Some(empty()), s5()) == empty(),
{
}

/// (e) THE `?` FACE (the fourth and last member of the family). It errors
/// instead of substituting, so its denotation is a PREDICATE on the state —
/// and the colon is again the whole difference: `x=""; ${x?boom}` runs in
/// bash (a set-to-empty var is set), `${x:?boom}` aborts. MEASURED
/// 2026-11-05, before the spelling landed: the no-colon form arrived as
/// `param(":?", …)` and the C aborted with rc=1 where bash prints `[]` and
/// exits 0.
proof fn question_keeps_the_colon(w: Str)
    requires
        w.len() > 0,
    ensures
        // the collapse (pre-fix)
        collapse(Op::Question) == collapse(Op::ColonQuestion),
        // …and the two predicates it merged differ on exactly one state
        question_errors(Some(empty())) == false,
        colon_question_errors(Some(empty())) == true,
        question_errors(None) == colon_question_errors(None),
        question_errors(Some(empty())) != colon_question_errors(Some(empty())),
{
}

/// …and, with the spelling preserved, the presence render is exact: the
/// `?` arm aborts iff the pointer is NULL, which is `question_errors`.
proof fn question_spelling_is_exact(v: Var)
    ensures
        // the shipped test (heap pointer NULL) IS bash's condition
        (v is None) == spelling_errors(Op::Question, v),
        // the colon form keeps its own (emptiness) condition
        colon_question_errors(v) == (v is None || (exists|s: Str| v == Some(s) && s.len() == 0)),
        // a set-to-empty var is NOT an error under `?` — the measured case
        spelling_errors(Op::Question, Some(empty())) == false,
        spelling_errors(Op::ColonQuestion, Some(empty())) == true,
{
}

} // verus!

fn main() {}
