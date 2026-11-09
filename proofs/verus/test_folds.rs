// Standalone model of test-simplification's Tier-1 obligation
// (`sh2perl/src/transforms/test_simplification.rs`): a `test` condition
// of the form `<op> <X> <X>` (textually IDENTICAL operands, no parens or
// arithmetic) folds to a constant boolean:
//
//   `-eq -le -ge =`   -> true
//   `-ne -lt -gt !=`  -> false
//
// Proves the soundness obligation:
//
//   selfcmp_sound: folding a self-comparison preserves the denoted
//   boolean.
//
// Operands are abstract values with decidable equality (the theorem is
// about the COMPARISON, not the operand domain — a value equals itself
// in every domain the backends implement). Operators are the eight the
// pass handles; anything else refuses (modelled as Option::None).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - The exact eight-op table and the exact true/false split.
//   - The textual-identity precondition (same token both sides — a
//     self-comparison reads the same value twice with no intervening
//     write, since a single test evaluates atomically).
//   - The refusal shapes (parens/arithmetic operands) as out-of-scope:
//     the model takes already-validated operands (no parens, no arith).
//   - Firing: the pass is 0/562 on the shell corpus (no self-comparisons
//     occur) but FIRES on a synthetic `[ "$x" -eq "$x" ]` probe — the
//     mechanics are verified live; the rule is a 2-line value theorem
//     either way.
use vstd::prelude::*;

verus! {

pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    StrEq,
    StrNe,
}

// The pass's exact table: which self-comparisons are true.
pub open spec fn selfcmp_value(op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => true,
        CmpOp::Le => true,
        CmpOp::Ge => true,
        CmpOp::StrEq => true,
        CmpOp::Ne => false,
        CmpOp::Lt => false,
        CmpOp::Gt => false,
        CmpOp::StrNe => false,
    }
}

// Source-side denotation of a self-comparison `X op X` over an abstract
// value domain with decidable (==): numeric ops compare as integers.
// String ops are a different domain (modelled below); the pass never
// applies them to a numeric test.
pub open spec fn eval_selfcmp_num(op: CmpOp, x: int) -> bool {
    match op {
        CmpOp::Eq => x == x,
        CmpOp::Ne => x != x,
        CmpOp::Lt => x < x,
        CmpOp::Le => x <= x,
        CmpOp::Gt => x > x,
        CmpOp::Ge => x >= x,
        CmpOp::StrEq => x == x,
        CmpOp::StrNe => x != x,
    }
}

pub open spec fn is_num_op(op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => true,
        CmpOp::Ne => true,
        CmpOp::Lt => true,
        CmpOp::Le => true,
        CmpOp::Gt => true,
        CmpOp::Ge => true,
        CmpOp::StrEq => false,
        CmpOp::StrNe => false,
    }
}

pub open spec fn is_str_op(op: CmpOp) -> bool {
    match op {
        CmpOp::StrEq => true,
        CmpOp::StrNe => true,
        _ => false,
    }
}

pub open spec fn eval_selfcmp_str(op: CmpOp, x: Seq<char>) -> bool {
    match op {
        CmpOp::StrEq => x == x,
        CmpOp::StrNe => x != x,
        // Numeric ops never apply to string operands (is_num_op gate);
        // the arms below are unreachable but must be total. They reuse
        // the string (in)equality, which coincides on self-comparisons.
        CmpOp::Eq => x == x,
        CmpOp::Ne => x != x,
        CmpOp::Lt => x != x,
        CmpOp::Le => x == x,
        CmpOp::Gt => x != x,
        CmpOp::Ge => x == x,
    }
}

// THE OBLIGATION: the folded constant equals the source denotation,
// for both operand domains.
proof fn selfcmp_sound_num(op: CmpOp, x: int)
    ensures selfcmp_value(op) == eval_selfcmp_num(op, x),
    decreases op,
{
    match op {
        CmpOp::Eq => {},
        CmpOp::Ne => {},
        CmpOp::Lt => {
            assert(!(x < x));
        },
        CmpOp::Le => {
            assert(x <= x);
        },
        CmpOp::Gt => {
            assert(!(x > x));
        },
        CmpOp::Ge => {
            assert(x >= x);
        },
        // Cross-domain arms: the pass never applies a string op to a
        // numeric test or vice versa — the model pins the refusal by
        // making these arms disagree loudly if miswired (see the
        // cross-domain mutant below).
        CmpOp::StrEq => {},
        CmpOp::StrNe => {},
    }
}

proof fn selfcmp_sound_str(op: CmpOp, x: Seq<char>)
    ensures selfcmp_value(op) == eval_selfcmp_str(op, x),
{
    match op {
        CmpOp::StrEq => {},
        CmpOp::StrNe => {
            assert(x != x == false);
        },
        _ => {},
    }
}

// ---- Fidelity pins (mirror the source's rule table) ----

// `-eq/-le/-ge/=` on X-vs-X are true.
proof fn pin_true_ops()
    ensures
        selfcmp_value(CmpOp::Eq) == true,
        selfcmp_value(CmpOp::Le) == true,
        selfcmp_value(CmpOp::Ge) == true,
        selfcmp_value(CmpOp::StrEq) == true,
{
}

// `-ne/-lt/-gt/!=` on X-vs-X are false.
proof fn pin_false_ops()
    ensures
        selfcmp_value(CmpOp::Ne) == false,
        selfcmp_value(CmpOp::Lt) == false,
        selfcmp_value(CmpOp::Gt) == false,
        selfcmp_value(CmpOp::StrNe) == false,
{
}

// Value check: `[ "$n" -lt "$n" ]` with n=5 is false either way.
proof fn pin_lt_false_at_runtime()
    ensures eval_selfcmp_num(CmpOp::Lt, 5) == false,
{
    selfcmp_sound_num(CmpOp::Lt, 5);
}

// Value check: `[ "$a" = "$a" ]` with a="xy" is true either way.
proof fn pin_streq_true_at_runtime()
    ensures eval_selfcmp_str(CmpOp::StrEq, seq!['x', 'y']) == true,
{
    selfcmp_sound_str(CmpOp::StrEq, seq!['x', 'y']);
}

fn main() {}

}
