// Standalone model of the slice-identity Tier-1 obligation
// (`sh2perl/src/transforms/slice_identity.rs`): `param("slice", NAME, OFF, LEN)`
// folds to the plain read `param("", NAME)` — preserving the denoted string —
// ONLY when NAME is a plain identifier, OFF is literal "0"/"", and LEN is
// literal ""/absent.
//
// Proves the soundness obligation:
//
//   slice_sound: folding a gate-satisfying slice call preserves the denoted
//   string from any store.
//
// Strings are Seq<char>; the store maps names to strings. Identifier-ness is
// an oracle parameter (`is_plain_ident` over name spellings is a char-class
// scan — the model takes its verdict as a boolean input, exactly as the
// transform checks it before folding). The op is modelled as given-slice:
// the theorem is per accepted `param("slice", …)` node; non-slice calls and
// wrong arities refuse structurally (same per-node shape as array_len_fold).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - the exact gate conjunction: 3..=4 args, Str("slice") op, plain-ident
//     name, Str OFF ∈ {"", "0"}, optional Str LEN ∈ {""}. `"0"` length is a
//     REFUSAL (`${v:0:0}` is the empty string — a real substring op).
//   - the rebuilt shape: `param("", NAME)` with styles preserved (style is
//     not modelled — byte-stability of the export round-trip, not value).
//   - traversal (`stmt`/`expr` walkers) is out of scope: every site is an
//     independent local renaming — the theorem is per-node, as in
//     array_len_fold.
//   - the denotation identity itself: `${v:0}`/`${v:}` read the SAME
//     parameter value as `${v}` (bash manual: offset 0 with no length is a
//     no-op) — both sides denote the plain store read.
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Str = Seq<char>;
pub type Store = Map<Name, Str>;

// Operand spellings the model distinguishes.
pub enum Off {
    Empty,
    Zero,
    Other,
}

pub enum Len {
    Absent,
    Empty,
    Other,
}

pub enum Expr {
    // param("slice", NAME, OFF, LEN) — NAME by interned id, gates as data.
    Slice { name: Name, plain_ident: bool, off: Off, len: Len },
    // param("", NAME) — the plain read.
    Read(Name),
}

pub open spec fn lookup(s: Store, n: Name) -> Str {
    if s.dom().contains(n) {
        s[n]
    } else {
        Seq::empty()
    }
}

// Both spellings denote the plain parameter read (unset reads empty — the
// same convention string_folds uses; the identity holds regardless since
// both sides read identically).
pub open spec fn ev(e: Expr, s: Store) -> Str {
    match e {
        Expr::Slice { name, .. } => lookup(s, name),
        Expr::Read(n) => lookup(s, n),
    }
}

// The gate conjunction, mirroring `fold()`'s check order: slice op (given),
// plain-identifier name, zero/empty offset, absent/empty length.
pub open spec fn foldable(e: Expr) -> bool {
    match e {
        Expr::Slice { plain_ident, off, len, .. } => {
            plain_ident && match off {
                Off::Empty => true,
                Off::Zero => true,
                Off::Other => false,
            } && match len {
                Len::Absent => true,
                Len::Empty => true,
                Len::Other => false,
            }
        },
        Expr::Read(_) => false,
    }
}

// The fold: gate-satisfying slice -> plain read; everything else stays.
pub open spec fn fold(e: Expr) -> Expr {
    match e {
        Expr::Slice { name, .. } => {
            if foldable(e) {
                Expr::Read(name)
            } else {
                e
            }
        },
        _ => e,
    }
}

// THE OBLIGATION: folding preserves the denoted string from any store.
proof fn slice_sound(e: Expr, s: Store)
    ensures ev(fold(e), s) == ev(e, s),
{
    match e {
        Expr::Slice { name, .. } => {
            if foldable(e) {
                assert(fold(e) == Expr::Read(name));
            } else {
                assert(fold(e) == e);
            }
        },
        Expr::Read(_) => {},
    }
}

// ---- Fidelity pins (mirror the source's own tests) ----

// `param("slice","s","0","")` folds to the plain read.
proof fn pin_zero_offset_empty_length_folds()
    ensures fold(Expr::Slice { name: 3nat, plain_ident: true, off: Off::Zero, len: Len::Empty }) == Expr::Read(
        3nat,
    ),
{
}

// Empty offset means zero and folds.
proof fn pin_empty_offset_folds()
    ensures fold(Expr::Slice { name: 3nat, plain_ident: true, off: Off::Empty, len: Len::Empty }) == Expr::Read(
        3nat,
    ),
{
}

// 3-arg form (length absent, to end) folds at zero.
proof fn pin_absent_length_folds()
    ensures fold(Expr::Slice { name: 3nat, plain_ident: true, off: Off::Zero, len: Len::Absent }) == Expr::Read(
        3nat,
    ),
{
}

// A real offset is kept (a real substring).
proof fn pin_real_offset_kept()
    ensures fold(Expr::Slice { name: 3nat, plain_ident: true, off: Off::Other, len: Len::Empty }) == (Expr::Slice {
        name: 3nat,
        plain_ident: true,
        off: Off::Other,
        len: Len::Empty,
    }),
{
}

// A real length is kept — including "0" (`${v:0:0}` is empty).
proof fn pin_real_length_kept()
    ensures fold(Expr::Slice { name: 3nat, plain_ident: true, off: Off::Zero, len: Len::Other }) == (Expr::Slice {
        name: 3nat,
        plain_ident: true,
        off: Off::Zero,
        len: Len::Other,
    }),
{
}

// Non-identifier names keep the slice spelling (different meaning).
proof fn pin_non_ident_kept()
    ensures fold(Expr::Slice { name: 3nat, plain_ident: false, off: Off::Zero, len: Len::Empty }) == (Expr::Slice {
        name: 3nat,
        plain_ident: false,
        off: Off::Zero,
        len: Len::Empty,
    }),
{
}

// A plain read is untouched (not a slice — nothing to fold).
proof fn pin_plain_read_untouched()
    ensures fold(Expr::Read(3nat)) == Expr::Read(3nat),
{
}

// Value check on a live store: the fold is invisible.
proof fn pin_fold_invisible_at_runtime()
    ensures ev(
        fold(Expr::Slice { name: 3nat, plain_ident: true, off: Off::Zero, len: Len::Empty }),
        Map::empty().insert(3nat, seq!['v', 'a', 'l']),
    ) == seq!['v', 'a', 'l'],
{
    slice_sound(
        Expr::Slice { name: 3nat, plain_ident: true, off: Off::Zero, len: Len::Empty },
        Map::empty().insert(3nat, seq!['v', 'a', 'l']),
    );
}

fn main() {}

}
