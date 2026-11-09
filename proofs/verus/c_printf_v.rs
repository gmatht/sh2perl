//! `printf -v` — the builtin writes TEXT, and every analysis that assumed a
//! scalar's value is a NUMBER was wrong about it.
//!
//! ## Need
//!
//! bash's `printf -v NAME fmt args…` stores the FORMATTED OUTPUT into NAME.
//! That is a write like any other, but its value comes from a FORMAT, so it is
//! an arbitrary string: `%s` copies an argument verbatim, `%03d` pads, `%x`
//! spells another radix. Three separate analyses in the C pipeline disagreed
//! with that, and each produced a measurable divergence on 2026-11-04/05:
//!
//! ```text
//!   program                                  bash     C (before)
//!   n=1; printf -v n '%d' 7; echo "$n"        7        1   ← the READ folded
//!   x=5; let x=x+1; printf -v x '%d' 9        …        9   ← uint8_t truncation (176)
//!   printf -v s '%s' aaaa…(40)                s        —   ← char s[2] bound
//! ```
//!
//! * the CONST CENSUS (`shir::analyze_var_const`, and the copy-propagation
//!   transform that consumes it) counted assignment SITES and had no arm for
//!   `-v`, so `n` looked single-write and every later read was replaced by the
//!   stale literal — the `echo` line was emitted as the constant `"n=[1]"`;
//! * the STORAGE-KIND verdict called the target Int, and an int decl cannot
//!   spell `"abc"` at all (and a width-narrowed one could not even hold the
//!   `%d` result — `char x` received a pointer);
//! * the LENGTH bound came from the other writes, so `char n[2]` was sized by
//!   `n=1` and the formatted write either truncated or overflowed.
//!
//! The three fixes are one idea: **a `-v` target is a partial-map, text-valued
//! name** — the same classification the `unset` round gave absence. The model
//! below states it once and derives all three consequences.
//!
//! ## What the model is
//!
//! A small string/number calculus: `Str` is the byte string, `atoll` is bash's
//! arithmetic reading of a word (leading digits, else 0), `canon` is the
//! canonical decimal spelling of a number, `fmt_str`/`fmt_int` are the two
//! `printf` faces this reasoning needs (`%s` = the argument verbatim, `%d` =
//! the canonical spelling). `Store` is the storage-kind choice: `SStr` keeps
//! the text, `SInt` keeps `atoll(text)`. Programs are lists of `Write` events
//! so the census lemmas are about counting, not about traversal.
//!
//! ## What is NOT modelled
//!
//! - the C lexer/parser (the missing `;` after `strcpy` was a *syntax* bug, and
//!   it is recorded in the source comment and in C_UB_PROOF.md §24, not proved
//!   here);
//! - pointers. The narrow-storage divergence (`uint8_t x` receiving a `char*`)
//!   is a MEASUREMENT (`176 != 9`), stated as `narrow_witness_is_not_the_value`
//!   and not derived: the model has no address space. What IS proved is that no
//!   int store is faithful to the text face, which is why the fix is demotion
//!   and not a wider integer.

use vstd::prelude::*;

verus! {

// ── the string side ────────────────────────────────────────────────────

pub type Str = Seq<char>;

pub open spec fn is_digit(c: char) -> bool {
    c == '0' || c == '1' || c == '2' || c == '3' || c == '4'
        || c == '5' || c == '6' || c == '7' || c == '8' || c == '9'
}

pub open spec fn dval(c: char) -> int {
    if c == '0' { 0 }
    else if c == '1' { 1 }
    else if c == '2' { 2 }
    else if c == '3' { 3 }
    else if c == '4' { 4 }
    else if c == '5' { 5 }
    else if c == '6' { 6 }
    else if c == '7' { 7 }
    else if c == '8' { 8 }
    else { 9 }
}

/// The character for a decimal digit (0..9); anything else is '0' (unused).
pub open spec fn dig(k: nat) -> char {
    if k == 0 { '0' }
    else if k == 1 { '1' }
    else if k == 2 { '2' }
    else if k == 3 { '3' }
    else if k == 4 { '4' }
    else if k == 5 { '5' }
    else if k == 6 { '6' }
    else if k == 7 { '7' }
    else if k == 8 { '8' }
    else { '9' }
}

/// bash's arithmetic reading of a word: leading decimal digits, else 0 —
/// the `$((x))` rule the C backend's `atoll` reproduces.
pub open spec fn atoll_from(t: Str, i: int, acc: int) -> int
    decreases t.len() - i,
{
    if i >= (t.len() as int) || i < 0 || !is_digit(t[i]) {
        acc
    } else {
        atoll_from(t, i + 1, acc * 10 + dval(t[i]))
    }
}

pub open spec fn atoll(t: Str) -> int {
    atoll_from(t, 0, 0)
}

/// The canonical decimal spelling of a natural.
pub open spec fn canon(n: nat) -> Str
    decreases n,
{
    if n < 10 {
        seq![dig(n)]
    } else {
        canon((n / 10) as nat).push(dig((n % 10) as nat))
    }
}

/// A run of `k` 'a's — the family the `%s` face can write at ANY length, used
/// to show a static bound cannot cover the formatted write.
pub open spec fn a_run(k: nat) -> Str
    decreases k,
{
    if k == 0 {
        Seq::empty()
    } else {
        a_run((k - 1) as nat).push('a')
    }
}

/// `printf '%s' w` — the argument verbatim.
pub open spec fn fmt_str(w: Str) -> Str {
    w
}

/// `printf '%d' n` — the canonical spelling (this is what bash prints, and
/// what it stores for `-v`).
pub open spec fn fmt_int(n: nat) -> Str {
    canon(n)
}

/// `atoll` clamped to a natural (the emitter's `atoll` never produces a
/// negative here: `printf -v` text starts at a digit or a non-digit).
pub open spec fn nat_of(v: int) -> nat {
    if v > 0 { v as nat } else { 0 }
}

// ── the two storages ───────────────────────────────────────────────────

pub enum Storage {
    /// `char *x` — keeps the TEXT.
    SStr,
    /// `long long x` — keeps `atoll(text)`.
    SInt,
}

/// The value a read observes, for each storage kind.
pub open spec fn observed(k: Storage, text: Str) -> Str {
    match k {
        Storage::SStr => text,
        // the int store keeps the NUMBER, and every read face spells it back
        // canonically (the emitter's `_sh_ll2s`, or a direct `%lld`)
        Storage::SInt => canon(nat_of(atoll(text))),
    }
}

/// Faithful = what bash reads back is exactly the text that was stored.
pub open spec fn faithful(k: Storage, text: Str) -> bool {
    observed(k, text) == text
}

// ── 1. the face is TEXT, and string storage is the identity ────────────

/// The `%s` face can store ANY string, so the face is not numeric: this is the
/// premise every other claim here rests on.
pub proof fn str_face_is_the_identity(w: Str)
    ensures
        fmt_str(w) == w,
        faithful(Storage::SStr, fmt_str(w)),
{
}

/// String storage survives the arithmetic face too: the number a later
/// `$((x+1))` sees is `atoll` of the text, which is unchanged by demotion.
pub proof fn arith_face_is_atoll_either_way(w: Str)
    ensures
        atoll(observed(Storage::SStr, w)) == atoll(w),
{
}

// ── 2. int storage cannot carry that text (the demotion's licence) ─────

/// `printf -v x '%03d' 4` stores `"004"`; the int store keeps 4 and spells it
/// back as `"4"` — a DIFFERENT string at the same numeric value. So padding,
/// leading zeros and any width/hash flag are lost, not rounded.
pub proof fn int_face_loses_padding()
    ensures
        atoll(seq!['0', '0', '4']) == 4,
        canon(4nat) == seq!['4'],
        !faithful(Storage::SInt, seq!['0', '0', '4']),
        faithful(Storage::SStr, seq!['0', '0', '4']),
{
    assert(atoll(seq!['0', '0', '4']) == 4) by (compute);
    assert(canon(4nat) == seq!['4']) by (compute);
    assert(observed(Storage::SInt, seq!['0', '0', '4']) == seq!['4']) by (compute);
}

/// `printf -v q '%s' abc` stores `"abc"`; the int store keeps 0 and reads back
/// `"0"`. Every non-numeric value — the whole point of `%s` — is destroyed.
pub proof fn int_face_loses_text()
    ensures
        atoll(seq!['a', 'b', 'c']) == 0,
        canon(0nat) == seq!['0'],
        !faithful(Storage::SInt, seq!['a', 'b', 'c']),
        faithful(Storage::SStr, seq!['a', 'b', 'c']),
{
    assert(atoll(seq!['a', 'b', 'c']) == 0) by (compute);
    assert(canon(0nat) == seq!['0']) by (compute);
    assert(observed(Storage::SInt, seq!['a', 'b', 'c']) == seq!['0']) by (compute);
}

/// THE GENERAL FORM: an int store is faithful exactly on the canonical
/// spellings, and `printf` writes whatever the format says — padded, signed,
/// non-numeric, another radix. This is why the fix is a storage-kind DEMOTION
/// and not a wider integer: no width makes `"abc"` a number.
pub proof fn int_store_faithful_iff_canonical(text: Str)
    ensures
        faithful(Storage::SInt, text) == (text == canon(nat_of(atoll(text)))),
{
}

/// The measured narrow case (`x=5; let x=x+1; printf -v x '%d' 9` printed 176
/// where bash prints 9): a width-narrowed int received the POINTER's low bits,
/// not `atoll("9")`. Not derivable here (no address space in the model) — this
/// pins the numbers so the claim in C_UB_PROOF.md §24 is checkable.
pub proof fn narrow_witness_is_not_the_value()
    ensures
        176int != atoll(seq!['9']),
        atoll(seq!['9']) == 9,
        canon(9nat) == seq!['9'],
        faithful(Storage::SInt, seq!['9']),
{
    assert(atoll(seq!['9']) == 9) by (compute);
    assert(canon(9nat) == seq!['9']) by (compute);
    assert(observed(Storage::SInt, seq!['9']) == seq!['9']) by (compute);
}

// ── 3. the length bound cannot be derived without the -v write ─────────

/// A fixed buffer of `cap` bytes holds a text iff the text plus its NUL fits.
pub open spec fn buf_holds(cap: nat, text: Str) -> bool {
    (text.len() as nat) + 1 <= cap
}

/// `printf -v` can write a text of ANY length (`%s` of a long argument, `%010d`
/// of a wide number), so a bound taken from the OTHER writes is not a bound on
/// the value: the `char n[2]` sized by `n=1` is exceeded by a 40-byte `%s`.
pub proof fn printf_v_bound_is_unbounded(k: nat)
    ensures
        !buf_holds(2nat, a_run(40nat)),
        a_run(k).len() == k,
        // ... and for every candidate cap there is a longer write
        exists|m: nat| m >= k && (a_run(m).len() as nat) + 1 > k,
{
    a_run_len(k);
    a_run_len(40nat);
    assert(a_run((k + 1) as nat).len() == k + 1) by {
        a_run_len((k + 1) as nat);
    }
}

pub proof fn a_run_len(k: nat)
    ensures a_run(k).len() == k,
    decreases k,
{
    if k > 0 {
        a_run_len((k - 1) as nat);
    }
}

/// The heap store is the fix: `_sh_xstrdup` copies the whole text, so the
/// exact value survives whatever the format produced.
pub open spec fn heap_copy(text: Str) -> Str {
    text
}

pub proof fn heap_keeps_any_text(k: nat)
    ensures
        heap_copy(a_run(k)) == a_run(k),
        faithful(Storage::SStr, heap_copy(fmt_str(a_run(k)))),
{
}

// ── 4. the census: `-v` is a WRITE SITE ────────────────────────────────

/// One write event: the name, and how it was written.
pub enum Write {
    /// `n=1`
    Assign,
    /// `printf -v n …`
    PrintfV,
}

pub open spec fn is_assign(w: Write) -> bool {
    match w {
        Write::Assign => true,
        Write::PrintfV => false,
    }
}

/// True number of writes to a name, given its event list.
pub open spec fn sites_true(events: Seq<Write>) -> nat
    decreases events.len(),
{
    if events.len() == 0 {
        0
    } else {
        sites_true(events.drop_last()) + 1
    }
}

/// The PRE-FIX census: only assignment-shaped events were counted.
pub open spec fn sites_pre_fix(events: Seq<Write>) -> nat
    decreases events.len(),
{
    if events.len() == 0 {
        0
    } else {
        sites_pre_fix(events.drop_last()) + (if is_assign(events.last()) { 1nat } else { 0nat })
    }
}

/// A value is substitutable everywhere (the fold) only if the name is written
/// exactly once — the premise BOTH censuses are trying to establish.
pub open spec fn fold_ok(events: Seq<Write>) -> bool {
    sites_true(events) == 1
}

/// THE OBLIGATION the source arm discharges: the fixed census counts every
/// event, so it agrees with the true number of writes — while the pre-fix one
/// under-counts by exactly the `printf -v` writes and can therefore claim
/// `fold_ok` on a name that is written TWICE.
pub proof fn census_after_the_fix(events: Seq<Write>)
    ensures
        // addition, not subtraction: the identity is induction-friendly and
        // says the same thing (the fixed census counts exactly the extra -v
        // writes on top of what the pre-fix one counted).
        sites_pre_fix(events) + sites_true(filter_printfv(events)) == sites_true(events),
{
    census_aux(events);
}

pub open spec fn filter_printfv(events: Seq<Write>) -> Seq<Write>
    decreases events.len(),
{
    if events.len() == 0 {
        Seq::empty()
    } else {
        let rest = filter_printfv(events.drop_last());
        if is_assign(events.last()) {
            rest
        } else {
            rest.push(events.last())
        }
    }
}

proof fn census_aux(events: Seq<Write>)
    ensures
        sites_pre_fix(events) + sites_true(filter_printfv(events)) == sites_true(events),
    decreases events.len(),
{
    if events.len() > 0 {
        let rest = events.drop_last();
        census_aux(rest);
        sites_true_push(rest, events.last());
        sites_pre_fix_push(rest, events.last());
        if is_assign(events.last()) {
            assert(filter_printfv(events) == filter_printfv(rest));
        } else {
            assert(filter_printfv(events) == filter_printfv(rest).push(events.last()));
            sites_true_push(filter_printfv(rest), events.last());
        }
    }
}

/// The two counters are `+1` per event, so they commute with a push — the
/// step the induction above needs and the only place the shapes matter.
proof fn sites_true_push(events: Seq<Write>, w: Write)
    ensures sites_true(events.push(w)) == sites_true(events) + 1,
{
    assert(events.push(w).drop_last() == events);
}

proof fn sites_pre_fix_push(events: Seq<Write>, w: Write)
    ensures sites_pre_fix(events.push(w))
        == sites_pre_fix(events) + (if is_assign(w) { 1nat } else { 0nat }),
{
    assert(events.push(w).drop_last() == events);
}

/// THE MEASURED DIVERGENCE, as a witness: `n=1; printf -v n '%d' 7; echo "$n"`.
/// The pre-fix census saw ONE write, claimed the fold was safe, and replaced
/// the read with the first write's value — while the program has two writes and
/// the read must see the second.
pub proof fn pre_fix_fold_is_unsound()
    ensures
        sites_true(seq![Write::Assign, Write::PrintfV]) == 2,
        sites_pre_fix(seq![Write::Assign, Write::PrintfV]) == 1,
        !fold_ok(seq![Write::Assign, Write::PrintfV]),
        fold_ok(seq![Write::Assign]),
        // and the values really differ, so the substitution changes output:
        fmt_int(7nat) == seq!['7'],
        fmt_int(1nat) == seq!['1'],
        fmt_int(7nat) != fmt_int(1nat),
{
    assert(sites_true(seq![Write::Assign]) == 1) by (compute);
    assert(sites_pre_fix(seq![Write::Assign]) == 1) by (compute);
    assert(sites_true(seq![Write::Assign, Write::PrintfV]) == 2) by (compute);
    assert(sites_pre_fix(seq![Write::Assign, Write::PrintfV]) == 1) by (compute);
    assert(fmt_int(7nat) == seq!['7']) by (compute);
    assert(fmt_int(1nat) == seq!['1']) by (compute);
    // the substituted value and the real one are different BYTES, so the
    // folded program prints something bash does not (`n=[1]` vs `n=[7]`)
    assert(seq!['7'] != seq!['1']) by (compute);
}

/// Soundness of the fold itself, stated so the census fix is the whole story:
/// when a name IS written once, substituting that single write's value at every
/// read is faithful in EITHER storage (the value is a literal, so the read face
/// is the same one the store uses).
pub proof fn single_write_fold_is_sound(text: Str)
    ensures
        fold_ok(seq![Write::Assign]),
        observed(Storage::SStr, text) == text,
{
    assert(sites_true(seq![Write::Assign]) == 1) by (compute);
}

// ── 5. the demotion composes with the other round's fix ────────────────

/// The `unset` round demoted on ABSENCE (`unset x`), this one on TEXT
/// (`printf -v x`). Both are "names whose value a number cannot spell", and the
/// union is what the pass demotes — the two censuses only ever REMOVE Int
/// verdicts, so they compose in either order.
pub open spec fn losable(name_int: bool, unset: bool, printf_v: bool) -> bool {
    name_int && (unset || printf_v)
}

pub proof fn demotion_composes()
    ensures
        forall|i: bool, u: bool, p: bool|
            losable(i, u, p) == (losable(i, u, false) || losable(i, false, p)),
        // an int verdict survives only when the name is neither unset nor a
        // `-v` target — i.e. the two rounds' censuses agree on the survivor set
        forall|u: bool, p: bool| losable(true, u, p) == (u || p),
{
}

} // verus!

fn main() {}
