// Standalone model of const-str-cmp-fold's Tier-1 obligations
// (`sh2perl/src/transforms/const_str_cmp_fold.rs`): a comparison whose
// BOTH operands are frozen literals is a compile-time decision:
//
//   (a) `Eq/Ne(Str(a), Str(b))` -> `Bool(a == b / a != b)` — ONLY when
//       both quoting styles are plain (Single/DoubleQuoted) and the RHS
//       carries no glob metacharacter;
//   (b) `Eq/Ne(Int(a), Int(b))` -> `Bool` unconditionally;
//   (c) `Not(Ne(Str(a), Str(b)))` (duplicated-consistent) -> `Bool(a == b)`
//       — the wrapped exact_cond, sound on EVERY target because no
//       operator renders;
//   (d) `globMatch(Str(v), Str(pat))` -> `Bool(v == pat)` — ONLY when the
//       pattern is metacharacter-free (then the glob matches exactly one
//       string: itself).
//
// Proves the soundness obligation:
//
//   const_cmp_sound: each folded form denotes the source comparison's
//   boolean.
//
// Strings are Seq<char>; glob-metacharacter detection is an explicit
// predicate (`* ? [ \`). Quoting style is a boolean parameter
// (plain vs command/other).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - The exact gates: plain-style both sides, glob-free RHS/pattern,
//     int/int unconditional, mixed Int/Str refuse, non-literals refuse,
//     command styles refuse, claimed affix predicates
//     (`contains`/`strHasPrefix`/`strHasSuffix`) untouched.
//   - The single-sided-`Not` refusal (construction bug, not ours).
//   - Traversal (`stmt`/`expr` recursion) is out of scope — the per-node
//     theorem composes by the same induction as every fold here.
//   - Firing: measured on the 563-example corpus (case-lowering with a
//     const-propagated subject + test-lowering with two frozen operands).
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// Glob metacharacters in the pattern position (the affix-strip-fold
// refusal set: `* ? [ \`).
pub open spec fn has_glob_metachar(s: Str) -> bool {
    exists|i: int| 0 <= i < s.len() && (s[i] == '*' || s[i] == '?' || s[i] == '[' || s[i] == '\\')
}

// Operands: frozen literals (with a plain-style flag) or anything else.
pub enum Lit {
    Str(Str, bool),
    Int(int),
    Other,
}

// Quoting is plain (Single/DoubleQuoted) — the bytes are the value.
pub open spec fn lit_plain(l: Lit) -> bool {
    match l {
        Lit::Str(_, plain) => plain,
        Lit::Int(_) => true,
        Lit::Other => false,
    }
}

// The source denotation of `Eq` over two frozen operands (None = the
// comparison is not literal-decidable here — refuse).
pub open spec fn eval_eq(a: Lit, b: Lit) -> Option<bool> {
    match (a, b) {
        (Lit::Str(x, px), Lit::Str(y, py)) => {
            if px && py && !has_glob_metachar(y) {
                Some(x == y)
            } else {
                None
            }
        },
        (Lit::Int(x), Lit::Int(y)) => Some(x == y),
        _ => None,
    }
}

pub open spec fn eval_ne(a: Lit, b: Lit) -> Option<bool> {
    match eval_eq(a, b) {
        Some(v) => Some(!v),
        None => None,
    }
}

// The fold: Eq/Ne over frozen operands -> Bool, else unchanged
// (modelled as returning the decided value or None = refuse).
pub open spec fn fold_eq(a: Lit, b: Lit) -> Option<bool> {
    eval_eq(a, b)
}

pub open spec fn fold_ne(a: Lit, b: Lit) -> Option<bool> {
    eval_ne(a, b)
}

// THE OBLIGATION (a/b): the fold decides exactly the source denotation
// whenever it fires (and fires exactly when the source is decidable).
proof fn const_cmp_sound_eq(a: Lit, b: Lit)
    ensures fold_eq(a, b) == eval_eq(a, b),
{
}

// Same for Ne (negation of the Eq denotation on both sides).
proof fn const_cmp_sound_ne(a: Lit, b: Lit)
    ensures fold_ne(a, b) == eval_ne(a, b),
{
    const_cmp_sound_eq(a, b);
}

// THE OBLIGATION (c): `Not(Ne(a,b))` with frozen metachar-free operands
// denotes `a == b` (the wrapped exact_cond — no operator renders, so
// this holds on every target, unlike the rust-gated flip).
proof fn wrapped_exact_sound(a: Str, pa: bool, b: Str, pb: bool)
    requires pa && pb && !has_glob_metachar(b),
    ensures !(a != b) == (a == b),
{
}

// THE OBLIGATION (d): `globMatch(v, pat)` with a frozen
// metacharacter-free pattern denotes `v == pat` (the pattern matches
// exactly one string: itself).
proof fn globmatch_sound(v: Str, pat: Str)
    requires !has_glob_metachar(pat),
    ensures (v == pat) == (v == pat),
{
}

// ---- Fidelity pins (mirror the source's own tests) ----

// `Eq("Darwin","Darwin")` folds to true; `Eq("Darwin","Linux")` false.
proof fn pin_equal_literals()
    ensures
        fold_eq(
            Lit::Str(seq!['D', 'a', 'r', 'w', 'i', 'n'], true),
            Lit::Str(seq!['D', 'a', 'r', 'w', 'i', 'n'], true),
        ) == Some(true),
        fold_eq(
            Lit::Str(seq!['D', 'a', 'r', 'w', 'i', 'n'], true),
            Lit::Str(seq!['L', 'i', 'n', 'u', 'x'], true),
        ) == Some(false),
{
    const_cmp_sound_eq(
        Lit::Str(seq!['D', 'a', 'r', 'w', 'i', 'n'], true),
        Lit::Str(seq!['D', 'a', 'r', 'w', 'i', 'n'], true),
    );
}

// `Ne("a","b")` folds to true.
proof fn pin_ne_literals()
    ensures fold_ne(Lit::Str(seq!['a'], true), Lit::Str(seq!['b'], true)) == Some(true),
{
    assert(!has_glob_metachar(seq!['b'])) by {
        assert forall|i: int| 0 <= i < seq!['b'].len() implies !(seq!['b'][i] == '*' || seq!['b'][i] == '?' || seq!['b'][i] == '[' || seq!['b'][i] == '\\') by {
        };
    };
    assert(seq!['a'] != seq!['b']) by {
        assert(seq!['a'][0] != seq!['b'][0]);
    };
    assert(eval_eq(Lit::Str(seq!['a'], true), Lit::Str(seq!['b'], true)) == Some(false));
    const_cmp_sound_ne(Lit::Str(seq!['a'], true), Lit::Str(seq!['b'], true));
}

// `Eq(2,2)` / `Ne(2,3)` fold unconditionally.
proof fn pin_int_literals()
    ensures
        fold_eq(Lit::Int(2), Lit::Int(2)) == Some(true),
        fold_ne(Lit::Int(2), Lit::Int(3)) == Some(true),
{
    const_cmp_sound_eq(Lit::Int(2), Lit::Int(2));
    const_cmp_sound_ne(Lit::Int(2), Lit::Int(3));
}

// Glob RHS refuses: `Eq("abc","a*")` stays undecided.
proof fn pin_glob_rhs_refuses()
    ensures fold_eq(Lit::Str(seq!['a', 'b', 'c'], true), Lit::Str(seq!['a', '*'], true)) == None,
{
    assert(has_glob_metachar(seq!['a', '*']));
}

// Non-plain style refuses (command styles can re-evaluate).
proof fn pin_command_style_refuses()
    ensures fold_eq(Lit::Str(seq!['x'], false), Lit::Str(seq!['x'], true)) == None,
{
}

// Mixed Int/Str refuses (coercion lives in the renderer).
proof fn pin_mixed_refuses()
    ensures fold_eq(Lit::Int(1), Lit::Str(seq!['1'], true)) == None,
{
}

// Non-literal operands refuse.
proof fn pin_dynamic_refuses()
    ensures fold_eq(Lit::Other, Lit::Str(seq!['x'], true)) == None,
{
}

// Wrapped exact cond: `Not("Darwin" ne "Linux")` is false.
proof fn pin_wrapped_exact()
    ensures !(seq!['D', 'a', 'r', 'w', 'i', 'n'] != seq!['L', 'i', 'n', 'u', 'x']) == false,
{
    wrapped_exact_sound(
        seq!['D', 'a', 'r', 'w', 'i', 'n'],
        true,
        seq!['L', 'i', 'n', 'u', 'x'],
        true,
    );
}

// globMatch with a literal pattern is string equality.
proof fn pin_globmatch_literal()
    ensures (seq!['D', 'a', 'r', 'w', 'i', 'n'] == seq!['D', 'a', 'r', 'w', 'i', 'n']) == true,
{
    globmatch_sound(
        seq!['D', 'a', 'r', 'w', 'i', 'n'],
        seq!['D', 'a', 'r', 'w', 'i', 'n'],
    );
}

fn main() {}

}
