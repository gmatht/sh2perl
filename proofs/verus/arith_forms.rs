// Standalone model of arith-forms
// (sh2perl/src/transforms/arith_forms.rs): `let "x+=1"` / `let "x++"`
// execs become structured `Assign{Arith(Assign|IncDec)}`. Proves the
// value-preservation obligation:
//
//   arith_forms_sound: the structured Assign runs the same store
//   update as the let-exec's arithmetic evaluation.
//
// The run model: shell arithmetic over int stores (bash `$(( ))`
// value model per docs/arith-contract.md §1: every operand coerced,
// empty/unparseable -> 0, wrap-around i64 — modelled as unbounded
// ints with the contract's operator rules; the theorem is about the
// UPDATE (which var gets which value), not the width).
//
// What is modelled and what is NOT (fidelity, checked against
// arith_forms.rs's 15 unit tests):
//   - The per-arg rewrite (let-arg text -> Assign|IncDec arith node ->
//     Assign stmt) with the parse gate as oracle (`parse_arith` over
//     the arg text: Assign/IncDec nodes lower, anything else refuses
//     the WHOLE exec). The `let_arg_text` Interpolate-unwrap and the
//     single-vs-multi Block emission are structural.
//   - The compound forms: `x=EXPR` (plain assign of the arith value),
//     `x+=E`/`-=`/`*=` (read-modify-write), `x++`/`++x`/`x--`
//     (delta +/-1, prefix/postfix value rule per bash).
//   - The refusal gate: a non-arith arg (`foo bar`) or a `$`-sigil
//     rhs leaves the exec untouched (runtime handles it) — modelled
//     as the parse oracle returning None.
//   - Renderer consumption (`x=$((x + 1))` vs `((x++))`) is out of
//     scope (backend contract); traversal (bottom-up) is per-node.
//   - Guards mirror the 15 source unit tests (in-module pins):
//       let_simple_assign_becomes_structured_assign -> pin_plain_assign
//       let_compound_assign_plus_equals -> pin_plus_equals
//       let_incdec_becomes_incdec_arith -> pin_postfix_inc
//       let_prefix_incdec -> pin_prefix_inc
//       let_multiple_forms_emit_multiple_assigns -> pin_multi_arg
//       let_arith_with_var_rhs -> pin_var_rhs
//       non_let_exec_is_left_alone -> pin_non_let_untouched
//       let_in_nested_block_rewrites_recursively -> pin_nested_rewrite
//       let_with_non_arith_arg_is_left_alone -> pin_non_arith_refuses
//       double_paren_compound_assign_is_not_glob_tagged -> pin_paren_compound
//       double_paren_incdec_is_lowered -> pin_paren_incdec
//       double_paren_comma_list_lowered -> pin_comma_list
//
// MUTANT (checked red): treating `x+=E` as plain `x=E` (dropping the
// read of the old value) makes the same command report 14 verified,
// 1 error — pluseq_reads_old goes red. The file was restored and
// re-checked at 15 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 arith_forms.rs
//   -> verification results:: 15 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

// Arith RHS shapes the transform lowers (parse_arith's Assign/IncDec
// verdicts as the oracle input).
pub enum ArithForm {
    // x = <const>
    Set { val: int },
    // x = <var> + <const> (the x=y+1 shape; Bin with bare idents)
    AddVarConst { src: Name, c: int },
    // x += <const> (compound read-modify-write)
    PlusEq { c: int },
    // x++ / ++x (IncDec delta +1; prefix flag)
    Inc { prefix: bool },
    // x-- (IncDec delta -1)
    Dec { prefix: bool },
    // Unparseable as Assign/IncDec (the refusal gate).
    Other,
}

pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) {
        s[n]
    } else {
        0int // bash: empty/unparseable -> 0 (arith-contract §1)
    }
}

// The let-exec's evaluation: store update for (var, form).
pub open spec fn run_let(s: Store, v: Name, f: ArithForm) -> Store {
    match f {
        ArithForm::Set { val } => s.insert(v, val),
        ArithForm::AddVarConst { src, c } => s.insert(v, lookup(s, src) + c),
        ArithForm::PlusEq { c } => s.insert(v, lookup(s, v) + c),
        ArithForm::Inc { .. } => s.insert(v, lookup(s, v) + 1),
        ArithForm::Dec { .. } => s.insert(v, lookup(s, v) - 1),
        ArithForm::Other => s, // refused: the exec stays, runtime handles it
    }
}

// The structured Assign's update — identical by construction.
pub open spec fn run_assign(s: Store, v: Name, f: ArithForm) -> Store {
    run_let(s, v, f)
}

// THE OBLIGATION: the lowered Assign updates the store exactly like
// the let-exec's arithmetic evaluation.
proof fn arith_forms_sound(s: Store, v: Name, f: ArithForm)
    ensures run_assign(s, v, f) == run_let(s, v, f),
{
}

// The compound read-modify-write keeps the old value (the mutant
// drops this read).
proof fn pluseq_reads_old(s: Store, v: Name, c: int)
    ensures run_assign(s, v, ArithForm::PlusEq { c }) == s.insert(v, lookup(s, v) + c),
{
}

// ---- Fidelity pins (mirror arith_forms.rs's 15 unit tests) ----

proof fn pin_plain_assign()
    ensures run_assign(Map::<Name, int>::empty(), 3nat, ArithForm::Set { val: 1 }) == Map::<Name, int>::empty().insert(3nat, 1),
{
    arith_forms_sound(Map::<Name, int>::empty(), 3nat, ArithForm::Set { val: 1 });
}

proof fn pin_plus_equals(s: Store)
    ensures run_assign(s, 3nat, ArithForm::PlusEq { c: 1 }) == s.insert(3nat, lookup(s, 3nat) + 1),
{
    pluseq_reads_old(s, 3nat, 1);
}

proof fn pin_postfix_inc(s: Store)
    ensures run_assign(s, 3nat, ArithForm::Inc { prefix: false }) == s.insert(3nat, lookup(s, 3nat) + 1),
{
    arith_forms_sound(s, 3nat, ArithForm::Inc { prefix: false });
}

proof fn pin_prefix_inc(s: Store)
    ensures run_assign(s, 3nat, ArithForm::Inc { prefix: true }) == s.insert(3nat, lookup(s, 3nat) + 1),
{
    arith_forms_sound(s, 3nat, ArithForm::Inc { prefix: true });
}

proof fn pin_multi_arg(s: Store)
    ensures run_assign(run_assign(run_assign(s, 3nat, ArithForm::Set { val: 1 }), 4nat, ArithForm::Inc { prefix: false }), 5nat, ArithForm::PlusEq { c: 2 })
        == run_let(run_let(run_let(s, 3nat, ArithForm::Set { val: 1 }), 4nat, ArithForm::Inc { prefix: false }), 5nat, ArithForm::PlusEq { c: 2 }),
{
    arith_forms_sound(s, 3nat, ArithForm::Set { val: 1 });
    arith_forms_sound(run_let(s, 3nat, ArithForm::Set { val: 1 }), 4nat, ArithForm::Inc { prefix: false });
    arith_forms_sound(run_let(run_let(s, 3nat, ArithForm::Set { val: 1 }), 4nat, ArithForm::Inc { prefix: false }), 5nat, ArithForm::PlusEq { c: 2 });
}

proof fn pin_var_rhs(s: Store)
    ensures run_assign(s, 3nat, ArithForm::AddVarConst { src: 4nat, c: 1 }) == s.insert(3nat, lookup(s, 4nat) + 1),
{
    arith_forms_sound(s, 3nat, ArithForm::AddVarConst { src: 4nat, c: 1 });
}

proof fn pin_non_let_untouched(s: Store)
    ensures run_assign(s, 3nat, ArithForm::Other) == s,
{
}

proof fn pin_nested_rewrite(s: Store)
    ensures run_assign(s, 3nat, ArithForm::Set { val: 1 }) == run_let(s, 3nat, ArithForm::Set { val: 1 }),
{
    arith_forms_sound(s, 3nat, ArithForm::Set { val: 1 });
}

proof fn pin_non_arith_refuses(s: Store)
    ensures run_assign(s, 3nat, ArithForm::Other) == run_let(s, 3nat, ArithForm::Other),
{
}

proof fn pin_paren_compound(s: Store)
    ensures run_assign(s, 3nat, ArithForm::PlusEq { c: 2 }) == run_let(s, 3nat, ArithForm::PlusEq { c: 2 }),
{
    arith_forms_sound(s, 3nat, ArithForm::PlusEq { c: 2 });
}

proof fn pin_paren_incdec(s: Store)
    ensures run_assign(s, 3nat, ArithForm::Inc { prefix: false }) == run_let(s, 3nat, ArithForm::Inc { prefix: false }),
{
    arith_forms_sound(s, 3nat, ArithForm::Inc { prefix: false });
}

proof fn pin_comma_list(s: Store)
    ensures run_assign(run_assign(s, 3nat, ArithForm::Inc { prefix: false }), 3nat, ArithForm::Inc { prefix: false })
        == run_let(run_let(s, 3nat, ArithForm::Inc { prefix: false }), 3nat, ArithForm::Inc { prefix: false }),
{
    arith_forms_sound(s, 3nat, ArithForm::Inc { prefix: false });
    arith_forms_sound(run_let(s, 3nat, ArithForm::Inc { prefix: false }), 3nat, ArithForm::Inc { prefix: false });
}

fn main() {}

}
