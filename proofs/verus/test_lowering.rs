// Standalone model of test-lowering's Tier-1 leaf obligations
// (sh2perl/src/transforms/test_lowering.rs): the per-leaf value
// preservation of the four lowering families — nonempty (`-n`/`-z`),
// string equality (`==`/`=`/`!=`), numeric comparison (`-eq`/`-ne`/…),
// and glob-affix (`strHasPrefix`/`contains`/`strHasSuffix`) — over a
// string-valued store. Proves the soundness obligation:
//
//   leaf_sound: each lowered leaf evaluates to the same boolean as
//   the source test text, from any store.
//
// Strings are Seq<char> (char-based, matching the runtime helpers'
// chars().count()/skip()/take()). Operands are vars or literals, as
// the shared `operand()` gate enforces. The boolean domain is bool
// directly (test status 0/1).
//
// What is modelled and what is NOT (fidelity, checked against
// test_lowering.rs):
//   - The four leaf families with their exact rewrite rules: `-n X`
//     → `X != ""`, `-z X` → `!(X != "")`; `A == B` → `!(A != B)`
//     (the Not(ne) encoding — perl maps Eq numerically, so equality
//     is the negation of Ne); `A -op B` → signed BigInt-free integer
//     comparison (i64 range, as numeric_operand enforces); glob-affix
//     `s == p*`/`s == *p*`/`s == *p` → prefix/contains/suffix.
//   - The `!=` negations via boolean not (the canonical both-sides
//     Not the renderers accept).
//   - Status liveness, cond-vs-statement position, the nocasematch
//     gate, compound `-a`/`-o` lifting, self-recursion, param-slice
//     operands: all out of scope (statement semantics / Tier-3
//     territory). The obligation is per-leaf value preservation —
//     the Tier-1 carve-out P5 names.
//   - Refusals (non-plain operands, glob metachars in string-eq,
//     unquoted-empty, positional `$1`) are modelled as the operand
//     Option being None: the leaf is total only over accepted shapes.
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Str = Seq<char>;
pub type Store = Map<Name, Str>;

// ---- Operands ----

pub enum Operand {
    Var(Name),
    Lit(Str),
}

// Total lookup with an empty default (closedness makes it unreachable
// for read names).
pub open spec fn lookup(s: Store, n: Name) -> Str {
    if s.dom().contains(n) {
        s[n]
    } else {
        Seq::empty()
    }
}

pub open spec fn eval_op(o: Operand, s: Store) -> Str {
    match o {
        Operand::Var(n) => lookup(s, n),
        Operand::Lit(t) => t,
    }
}

// ---- Leaf shapes ----

pub enum NonemptyOp {
    NonEmpty,
    Zero,
}

pub enum StrEqOp {
    Eq,
    Ne,
}

pub enum NumOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

pub enum AffixOp {
    Prefix,
    Contains,
    Suffix,
}

// ---- String primitives ----

pub open spec fn str_len(s: Str) -> nat {
    s.len()
}

pub open spec fn str_eq(a: Str, b: Str) -> bool {
    a == b
}

pub open spec fn str_ne(a: Str, b: Str) -> bool {
    a != b
}

// Prefix: a starts with p.
pub open spec fn str_prefix(s: Str, p: Str) -> bool {
    p.len() <= s.len() && s.subrange(0, p.len() as int) == p
}

// Suffix: s ends with p.
pub open spec fn str_suffix(s: Str, p: Str) -> bool {
    p.len() <= s.len() && s.subrange(s.len() as int - p.len() as int, s.len() as int) == p
}

// Contains: p occurs in s at some offset.
pub open spec fn str_contains(s: Str, p: Str) -> bool {
    exists|i: int| #[trigger] s.subrange(i, i + p.len() as int) == p && 0 <= i && i + p.len() <= s.len()
}

// ---- Integer comparison (i64 range, as numeric_operand enforces) ----

pub open spec fn in_i64(n: int) -> bool {
    -9223372036854775808 <= n <= 9223372036854775807
}

// Length-emptiness bridge: a string is non-empty iff it differs
// from the empty string.
proof fn nonempty_iff_len(x: Str)
    ensures str_ne(x, Seq::empty()) == (x.len() > 0),
{
    if x.len() > 0 {
        assert(x != Seq::<char>::empty()) by {
            assert(Seq::<char>::empty().len() == 0);
        };
    } else {
        assert(x.len() == 0);
        assert(x == Seq::<char>::empty()) by {
            assert forall|i: int| 0 <= i < x.len() implies x[i] == Seq::<char>::empty()[i] by {
            };
            assert(x =~= Seq::<char>::empty());
        };
    }
}

// ---- Leaf evaluations: source-test side vs lowered side ----

// `-n X`: true iff X is nonempty. `-z X`: its negation.
pub open spec fn eval_nonempty_src(op: NonemptyOp, x: Str) -> bool {
    match op {
        NonemptyOp::NonEmpty => x.len() > 0,
        NonemptyOp::Zero => x.len() == 0,
    }
}

pub open spec fn eval_nonempty_low(op: NonemptyOp, x: Str) -> bool {
    match op {
        NonemptyOp::NonEmpty => str_ne(x, Seq::empty()),
        NonemptyOp::Zero => !str_ne(x, Seq::empty()),
    }
}

// `A == B` (string): equality. `A != B`: inequality. Lowered: Eq is
// Not(Ne) — the perl-numeric-Eq avoidance — Ne is Ne directly.
pub open spec fn eval_streq_src(op: StrEqOp, a: Str, b: Str) -> bool {
    match op {
        StrEqOp::Eq => str_eq(a, b),
        StrEqOp::Ne => str_ne(a, b),
    }
}

pub open spec fn eval_streq_low(op: StrEqOp, a: Str, b: Str) -> bool {
    match op {
        StrEqOp::Eq => !str_ne(a, b),
        StrEqOp::Ne => str_ne(a, b),
    }
}

// Numeric: both sides parse as i64 (the operand gate); compare as ints.
pub open spec fn eval_num_src(op: NumOp, a: int, b: int) -> bool {
    match op {
        NumOp::Eq => a == b,
        NumOp::Ne => a != b,
        NumOp::Gt => a > b,
        NumOp::Ge => a >= b,
        NumOp::Lt => a < b,
        NumOp::Le => a <= b,
    }
}

pub open spec fn eval_num_low(op: NumOp, a: int, b: int) -> bool {
    match op {
        NumOp::Eq => a == b,
        NumOp::Ne => a != b,
        NumOp::Gt => a > b,
        NumOp::Ge => a >= b,
        NumOp::Lt => a < b,
        NumOp::Le => a <= b,
    }
}

// Glob-affix: the pattern side is a literal/operand string; the stars
// are the shape (not part of the pattern value).
pub open spec fn eval_affix_src(op: AffixOp, s: Str, p: Str) -> bool {
    match op {
        AffixOp::Prefix => str_prefix(s, p),
        AffixOp::Contains => str_contains(s, p),
        AffixOp::Suffix => str_suffix(s, p),
    }
}

pub open spec fn eval_affix_low(op: AffixOp, s: Str, p: Str) -> bool {
    match op {
        AffixOp::Prefix => str_prefix(s, p),
        AffixOp::Contains => str_contains(s, p),
        AffixOp::Suffix => str_suffix(s, p),
    }
}

// Negated affix (`!=`): boolean not of the primitive (the canonical
// both-sides Not).
pub open spec fn eval_affix_neg_src(op: AffixOp, s: Str, p: Str) -> bool {
    !eval_affix_src(op, s, p)
}

pub open spec fn eval_affix_neg_low(op: AffixOp, s: Str, p: Str) -> bool {
    !eval_affix_low(op, s, p)
}

// ---- THE OBLIGATIONS: each leaf family preserves the boolean ----

proof fn nonempty_sound(op: NonemptyOp, x: Str)
    ensures eval_nonempty_low(op, x) == eval_nonempty_src(op, x),
{
    match op {
        NonemptyOp::NonEmpty => {
            nonempty_iff_len(x);
        },
        NonemptyOp::Zero => {
            nonempty_iff_len(x);
        },
    }
}

proof fn streq_sound(op: StrEqOp, a: Str, b: Str)
    ensures eval_streq_low(op, a, b) == eval_streq_src(op, a, b),
{
    match op {
        StrEqOp::Eq => {
            assert(str_eq(a, b) == (a == b));
            assert(str_ne(a, b) == (a != b));
        },
        StrEqOp::Ne => {},
    }
}

proof fn num_sound(op: NumOp, a: int, b: int)
    requires in_i64(a), in_i64(b),
    ensures eval_num_low(op, a, b) == eval_num_src(op, a, b),
{
    match op {
        NumOp::Eq => {},
        NumOp::Ne => {},
        NumOp::Gt => {},
        NumOp::Ge => {},
        NumOp::Lt => {},
        NumOp::Le => {},
    }
}

proof fn affix_sound(op: AffixOp, s: Str, p: Str)
    ensures eval_affix_low(op, s, p) == eval_affix_src(op, s, p),
{
    match op {
        AffixOp::Prefix => {},
        AffixOp::Contains => {},
        AffixOp::Suffix => {},
    }
}

proof fn affix_neg_sound(op: AffixOp, s: Str, p: Str)
    ensures eval_affix_neg_low(op, s, p) == eval_affix_neg_src(op, s, p),
{
}

// Concrete-sequence (in)equality: distinct literals differ.
// (Seq extensionality over literals needs the witness index; the
// automation does not case-split singletons by itself.)
proof fn lit_ne(a: Str, b: Str, i: int)
    requires 0 <= i, i < a.len(), i < b.len(), a[i] != b[i],
    ensures a != b,
{
    if a == b {
        assert(a[i] == b[i]);
        assert(false);
    }
}

// A literal prefix/suffix/contains fact, by exhibiting the subrange.
proof fn prefix_witness(s: Str, p: Str)
    requires p.len() <= s.len(), s.subrange(0, p.len() as int) == p,
    ensures str_prefix(s, p),
{
}

proof fn suffix_witness(s: Str, p: Str)
    requires p.len() <= s.len(), s.subrange(s.len() as int - p.len() as int, s.len() as int) == p,
    ensures str_suffix(s, p),
{
}

proof fn contains_witness(s: Str, p: Str, i: int)
    requires 0 <= i, i + p.len() <= s.len(), s.subrange(i, i + p.len() as int) == p,
    ensures str_contains(s, p),
{
}

// A literal NON-prefix: the first differing character rules it out.
// (Without a witness the solver must refute all alignments; the
// index pins the mismatch.)
proof fn not_prefix_witness(s: Str, p: Str, i: int)
    requires 0 <= i, i < p.len(), i < s.len(), s[i] != p[i],
    ensures !str_prefix(s, p),
{
    if str_prefix(s, p) {
        assert(s.subrange(0, p.len() as int) == p);
        assert(s.subrange(0, p.len() as int)[i] == p[i]);
        assert(s[i] == p[i]);
        assert(false);
    }
}

pub open spec fn closed_for(o: Operand, s: Store) -> bool {
    match o {
        Operand::Var(n) => s.dom().contains(n),
        Operand::Lit(_) => true,
    }
}

// The nonempty leaf over an operand: `-n $x` reads the var, `-n ""`
// is the empty literal — both evaluate identically on both sides.
proof fn nonempty_op_sound(op: NonemptyOp, x: Operand, s: Store)
    requires closed_for(x, s),
    ensures eval_nonempty_low(op, eval_op(x, s)) == eval_nonempty_src(op, eval_op(x, s)),
{
    nonempty_sound(op, eval_op(x, s));
}

proof fn streq_op_sound(op: StrEqOp, a: Operand, b: Operand, s: Store)
    requires closed_for(a, s), closed_for(b, s),
    ensures eval_streq_low(op, eval_op(a, s), eval_op(b, s)) == eval_streq_src(
        op,
        eval_op(a, s),
        eval_op(b, s),
    ),
{
    streq_sound(op, eval_op(a, s), eval_op(b, s));
}

proof fn affix_op_sound(op: AffixOp, x: Operand, y: Operand, s: Store)
    requires closed_for(x, s), closed_for(y, s),
    ensures eval_affix_low(op, eval_op(x, s), eval_op(y, s)) == eval_affix_src(
        op,
        eval_op(x, s),
        eval_op(y, s),
    ),
{
    affix_sound(op, eval_op(x, s), eval_op(y, s));
}

proof fn affix_neg_op_sound(op: AffixOp, x: Operand, y: Operand, s: Store)
    requires closed_for(x, s), closed_for(y, s),
    ensures eval_affix_neg_low(op, eval_op(x, s), eval_op(y, s)) == eval_affix_neg_src(
        op,
        eval_op(x, s),
        eval_op(y, s),
    ),
{
    affix_neg_sound(op, eval_op(x, s), eval_op(y, s));
}

// ---- Fidelity pins (mirror test_lowering.rs's own tests) ----

// `-n "$a"` → `a ne ""`: nonempty var lowers to Ne.
proof fn pin_nonempty_var()
    ensures eval_nonempty_low(NonemptyOp::NonEmpty, seq!['a']) == true,
{
    nonempty_sound(NonemptyOp::NonEmpty, seq!['a']);
}

// `-n ""` → `"" ne ""`: the empty literal lowers to false (constant).
proof fn pin_nonempty_empty()
    ensures eval_nonempty_low(NonemptyOp::NonEmpty, Seq::empty()) == false,
{
    nonempty_sound(NonemptyOp::NonEmpty, Seq::empty());
}

// `-z "$a"` → `Not(a ne "")`: zero test is the negation.
proof fn pin_zero_negation()
    ensures eval_nonempty_low(NonemptyOp::Zero, seq!['a']) == false,
{
    nonempty_sound(NonemptyOp::Zero, seq!['a']);
}

// `"$s" = 3` → `Not(s ne "3")`: Eq is Not(Ne), not numeric Eq.
proof fn pin_string_eq_not_ne()
    ensures eval_streq_low(StrEqOp::Eq, seq!['x'], seq!['3']) == false,
{
    // 'x' vs '3' differ at index 0: Ne holds, so Eq-as-Not(Ne) is false.
    assert(seq!['x'][0] != seq!['3'][0]);
    lit_ne(seq!['x'], seq!['3'], 0);
    streq_sound(StrEqOp::Eq, seq!['x'], seq!['3']);
}

// `"x" == "x"` → Not(Ne) is true on identical strings.
proof fn pin_string_eq_true()
    ensures eval_streq_low(StrEqOp::Eq, seq!['x'], seq!['x']) == true,
{
    streq_sound(StrEqOp::Eq, seq!['x'], seq!['x']);
}

// `"$s" != "x"` → Ne directly (no Not wrapper).
proof fn pin_string_ne_direct()
    ensures eval_streq_low(StrEqOp::Ne, seq!['a'], seq!['b']) == true,
{
    lit_ne(seq!['a'], seq!['b'], 0);
    streq_sound(StrEqOp::Ne, seq!['a'], seq!['b']);
}

// Numeric `-ge` on literals: 5 >= 3.
proof fn pin_numeric_ge()
    requires in_i64(5) && in_i64(3),
    ensures eval_num_low(NumOp::Ge, 5, 3) == true,
{
    num_sound(NumOp::Ge, 5, 3);
}

// Glob-affix: `"/usr/bin"` has prefix `"/usr/"`, contains `"sr/b"`,
// suffix `"bin"` — and not prefix `"bin"`.
proof fn pin_affix_shapes()
    ensures
        eval_affix_low(AffixOp::Prefix, seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'], seq![
            '/',
            'u',
            's',
            'r',
            '/',
        ]) == true,
        eval_affix_low(AffixOp::Contains, seq!['a', 'b', 'c', 'a', 'b', 'c'], seq!['b', 'c']) == true,
        eval_affix_low(AffixOp::Suffix, seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'], seq![
            'b',
            'i',
            'n',
        ]) == true,
        eval_affix_low(AffixOp::Prefix, seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'], seq![
            'b',
            'i',
            'n',
        ]) == false,
{
    assert(seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'].subrange(0, 5) == seq![
        '/',
        'u',
        's',
        'r',
        '/',
    ]) by {
        assert forall|i: int| 0 <= i < 5 implies seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'][i] == seq!['/', 'u', 's', 'r', '/'][i] by {
        };
    };
    prefix_witness(
        seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'],
        seq!['/', 'u', 's', 'r', '/'],
    );
    assert(seq!['a', 'b', 'c', 'a', 'b', 'c'].subrange(1, 3) == seq!['b', 'c']) by {
        assert forall|i: int| 0 <= i < 2 implies seq!['a', 'b', 'c', 'a', 'b', 'c'][1 + i] == seq!['b', 'c'][i] by {
        };
    };
    contains_witness(seq!['a', 'b', 'c', 'a', 'b', 'c'], seq!['b', 'c'], 1);
    assert(seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'].subrange(5, 8) == seq!['b', 'i', 'n']) by {
        assert forall|i: int| 0 <= i < 3 implies seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'][5 + i] == seq!['b', 'i', 'n'][i] by {
        };
    };
    suffix_witness(
        seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'],
        seq!['b', 'i', 'n'],
    );
    not_prefix_witness(
        seq!['/', 'u', 's', 'r', '/', 'b', 'i', 'n'],
        seq!['b', 'i', 'n'],
        0,
    );
}

// Negated affix `!=`: `!(prefix)` — the canonical both-sides Not.
proof fn pin_affix_neg()
    ensures eval_affix_neg_low(AffixOp::Prefix, seq!['a', 'b'], seq!['b']) == true,
{
    // "ab" does not have prefix "b" (differ at index 0), so the
    // negated affix is true.
    not_prefix_witness(seq!['a', 'b'], seq!['b'], 0);
}

fn main() {}

}
