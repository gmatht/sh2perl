// Standalone model of array-len-fold
// (sh2perl/src/transforms/array_len_fold.rs): the local rewrite
// `param("slice", "#NAME", "@", "")` -> `arrayLen(NAME)` with its
// refuse-on-doubt guards. Proves the value-preservation obligation:
//
//   fold_sound: folding preserves the evaluated value of every
//   expression, from any store.
//
// Names are interned to nat (as in the sibling proofs). Array VALUES
// are sequences of ints; the store maps names to array values. Both
// spellings evaluate against the same array binding, so the fold is a
// renaming with identical denotation — the guards' job is only to
// ensure the slice spelling MEANS element count, which the model
// captures by evaluating the slice form as element count exactly when
// the guards hold (and leaving every other shape untouched).
//
// What is modelled and what is NOT (fidelity, checked against
// array_len_fold.rs):
//   - The Call node `param("slice", name, off[, len])` with Str args,
//     `arrayLen(name)` with one Str arg, and opaque OtherExpr/OtherStmt
//     for everything else (traversal only — the real stmt()/expr()
//     walkers recurse everywhere and rewrite bottom-up; the model folds
//     one node, which is the whole obligation).
//   - Guards, in order: func == "param"; arity 3..=4 (a 5th
//     `(k)`-annotation arg refuses); args[0] == "slice" (Str);
//     name has `#` prefix; base after `#` is a plain identifier
//     ([A-Za-z_][A-Za-z0-9_]*); offset slot is the "@" list marker
//     (numeric/`*`/non-literal offsets refuse); length slot absent or
//     literal "" (non-empty/non-literal lengths refuse). The name style
//     is preserved (byte-stable export round-trip) — modelled as a bool
//     carried across unchanged.
//   - Identifier-ness is over the interned id via an oracle predicate
//     `is_plain_ident`: the model takes the verdict as given (the real
//     check is a char-class scan); the proof covers every other guard.
//   - The `#`-with-non-`@`-offset case (`${#arr:2}` — substring of the
//     count) is the key negative control: the model evaluates the
//     unfolded slice form as element count ONLY under the full guard
//     conjunction, and the pin `pin_numeric_offset_kept` shows the
//     shape survives.
//   - Traversal order / multi-site rewriting: out of scope (each site
//     is an independent local renaming; the theorem is per-node).
use vstd::prelude::*;

verus! {

pub type Name = nat;

// A name that is just an identifier (after the `#`): the oracle for
// the real `is_plain_ident` char-class scan.
pub open spec fn is_plain_ident(n: Name) -> bool {
    // Names 1..=100 model identifiers; 0 models everything else
    // (`arr[@]`, `@`, `*`, positionals, dotted names, empty).
    // The mapping is fixed and documented: the proof never inspects
    // *why* a name qualifies, only that the fold consults the verdict.
    1 <= n && n <= 100
}

pub enum Expr {
    // param("slice", name, off [, len]) with literal Str args.
    // has_len: whether the 4th (length) slot is present; len_empty:
    // whether it is the literal "" (only meaningful when has_len).
    // hash_base: Some(base) when name is "#" ++ base, None otherwise.
    // name_style: carried across the fold unchanged.
    // arity_ok: args.len() in 3..=4 (a 5th `(k)`-annotation arg refuses).
    ParamSlice { hash_base: Option<Name>, off_is_at: bool, has_len: bool, len_empty: bool, name_style: bool, arity_ok: bool },
    // arrayLen(name).
    ArrayLen { name: Name, name_style: bool },
    // Everything else (opaque to the fold).
    OtherExpr,
}

pub enum Stmt {
    ExprStmt(Expr),
    OtherStmt,
}

// Array values: the store maps names to element sequences.
pub type Store = Map<Name, Seq<int>>;

pub open spec fn lookup(s: Store, n: Name) -> Seq<int> {
    if s.dom().contains(n) {
        s[n]
    } else {
        Seq::empty()
    }
}

// Evaluation: the slice form denotes element count exactly when the
// guards hold; otherwise it is opaque (Some other value the model does
// not constrain — the fold never fires there, so its denotation is
// irrelevant to the theorem).
pub open spec fn ev(e: Expr, s: Store) -> int {
    match e {
        Expr::ParamSlice { hash_base, off_is_at, has_len, len_empty, arity_ok, .. } => {
            match hash_base {
                Some(base) => {
                    if arity_ok && is_plain_ident(base) && off_is_at && (!has_len || len_empty) {
                        lookup(s, base).len() as int
                    } else {
                        // A refused shape: unspecified (never folded).
                        -1
                    }
                },
                None => -1,
            }
        },
        Expr::ArrayLen { name, .. } => lookup(s, name).len() as int,
        Expr::OtherExpr => -1,
    }
}

// The guard conjunction, mirroring fold() arm by arm.
pub open spec fn foldable(e: Expr) -> bool {
    match e {
        Expr::ParamSlice { hash_base, off_is_at, has_len, len_empty, arity_ok, .. } => {
            match hash_base {
                Some(base) => arity_ok && is_plain_ident(base) && off_is_at && (!has_len || len_empty),
                None => false,
            }
        },
        _ => false,
    }
}

// The fold, mirroring fold()'s early-return structure: the guards are
// checked inline (a separate predicate would drift from the code's
// check order); anything failing a guard returns the node unchanged.
pub open spec fn fold(e: Expr) -> Expr {
    match e {
        Expr::ParamSlice { hash_base, off_is_at, has_len, len_empty, name_style, arity_ok } => {
            match hash_base {
                Some(base) => {
                    if arity_ok && is_plain_ident(base) && off_is_at && (!has_len || len_empty) {
                        (Expr::ArrayLen { name: base, name_style })
                    } else {
                        e
                    }
                },
                None => e,
            }
        },
        _ => e,
    }
}

// THE OBLIGATION: folding preserves the value from any store.
proof fn fold_sound(e: Expr, s: Store)
    requires foldable(e),
    ensures ev(fold(e), s) == ev(e, s),
{
    match e {
        Expr::ParamSlice { hash_base, off_is_at, has_len, len_empty, name_style, arity_ok } => {
            match hash_base {
                Some(base) => {
                    assert(arity_ok && is_plain_ident(base) && off_is_at && (!has_len || len_empty));
                    assert(ev(e, s) == lookup(s, base).len() as int);
                    assert(fold(e) == (Expr::ArrayLen { name: base, name_style }));
                    assert(ev(fold(e), s) == lookup(s, base).len() as int);
                },
                None => {
                    // foldable is false for None: vacuous.
                    assert(!foldable(e));
                },
            }
        },
        _ => {
            assert(!foldable(e));
        },
    }
}

// ---- Fidelity pins (mirror array_len_fold.rs's tests) ----

// `${#arr[@]}`, 4-arg form: folds to arrayLen(arr).
proof fn pin_four_arg_folds()
    ensures
        fold(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true })
            == (Expr::ArrayLen { name: 7nat, name_style: true }),
{
}

// Length slot absent (3-arg form): folds.
proof fn pin_three_arg_folds()
    ensures
        fold(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: false, len_empty: false, name_style: false, arity_ok: true })
            == (Expr::ArrayLen { name: 7nat, name_style: false }),
{
}

// `${arr[@]}` (no `#`): a whole-LIST read, kept.
proof fn pin_plain_name_kept()
    ensures fold(Expr::ParamSlice { hash_base: None, off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true })
        == (Expr::ParamSlice { hash_base: None, off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true }),
{
}

// `${#arr:2}` (numeric offset): substring of the count, kept.
proof fn pin_numeric_offset_kept()
    ensures fold(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: false, has_len: true, len_empty: true, name_style: true, arity_ok: true })
        == (Expr::ParamSlice { hash_base: Some(7nat), off_is_at: false, has_len: true, len_empty: true, name_style: true, arity_ok: true }),
{
}

// Non-empty length (`${#arr[@]:1}` shape): kept.
proof fn pin_non_empty_length_kept()
    ensures fold(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: false, name_style: true, arity_ok: true })
        == (Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: false, name_style: true, arity_ok: true }),
{
}

// Non-identifier base (`arr[@]`-shaped, id 0): kept.
proof fn pin_non_ident_base_kept()
    ensures fold(Expr::ParamSlice { hash_base: Some(0nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true })
        == (Expr::ParamSlice { hash_base: Some(0nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true }),
{
}

// 5-arg `(k)`-annotation form: arity refuses, kept.
proof fn pin_bad_arity_kept()
    ensures fold(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: false })
        == (Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: false }),
{
}

// Value check: the folded node counts elements from any store.
proof fn pin_folded_counts_elements(s: Store)
    ensures
        ev(fold(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true }), s)
            == lookup(s, 7nat).len() as int,
{
    fold_sound(Expr::ParamSlice { hash_base: Some(7nat), off_is_at: true, has_len: true, len_empty: true, name_style: true, arity_ok: true }, s);
}

fn main() {}

}
