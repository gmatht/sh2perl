// Standalone model of optimize's unproven delta
// (sh2perl/src/shir_passes/optimize.rs): the DCE read-census
// shapes, the decl-guard, the const_prop gate, and the drop
// theorem. Proves the verdict-soundness obligation:
//
//   unmarked_unread: a store whose target is unmarked in every
//   later statement is unread by every later statement (so the
//   drop is unobserved — a missed read would be a dropped live
//   store, and each pin is a recorded regression for exactly that).
//
// DELTA ONLY (not remodelled — proved elsewhere):
//   - fold_arith_literals' Bin/Un-over-Num mechanics: arith_folds.rs
//     (11). Tests fold_arith_literals_shrinks_constant_exprs,
//     _leaves_vars_alone, _partial_subtree carry NO pin here; the
//     fold mechanics are that model's obligation (recorded, not
//     silently skipped).
//   - the DCE core (purge/drop mechanics): dead_store_elim.rs (45).
//   - the substitution core: copy_propagation.rs (27).
// What this model owns: the census SHAPES (baked indices, param
// all-ops, redirect targets, [*]-reads-IFS with the [@] control,
// bare-arith idents), the export decl-guard in both A1 shapes, the
// const_prop gate conjunction, and the unmarked-unread induction.
// The If/While-body traversal (walk_children recursion) is a
// PARAMETER, as in every model (censuses as oracles).
//
// Leaf read-calculus (no nesting — the induction stays fuel-free):
// Plain, DirectReads, BakedIndex (arr[$i] target reads i),
// ParamRead (param op reads name), RedirectTarget (getVar target
// reads name), StarJoin (name[*] reads array + IFS), AtJoin
// (name[@] reads array only — the precision control), LetArith
// (bare-ident string reads idents), ExportGuard (export/declare
// guard names in both A1 shapes; the guard_word string filter is
// an oracle premise).
//
// What is modelled and what is NOT (fidelity, checked against
// optimize.rs's 12 unit tests):
//   - Each census arm's output locked by a pin (a dropped arm goes
//     red — the MUTANT below); the [@]-control locks precision
//     (IFS marked by [*] only, never by [@]).
//   - The drop theorem generally (unmarked_unread induction over
//     arbitrary later-lists) + concrete witness pins.
//   - The const_prop gate as a 4-premise conjunction with a
//     fires/refuses pin pair.
//   - Guards mirror 9 of the 12 source unit tests (in-module pins):
//       dce_drops_never_read_store
//         -> pin_unread_drops + pin_drop_agrees
//       index_name_refs_reads_baked_index_vars -> pin_baked_index_refs
//       dce_keeps_store_read_only_by_a_baked_index_name
//         -> pin_baked_index_keeps
//       dce_keeps_param_read_store -> pin_param_reads
//       dce_keeps_redirect_target_read_store -> pin_redirect_target_reads
//       dce_keeps_ifs_store_read_by_a_star_join -> pin_star_join_reads_ifs
//       dce_keeps_store_read_only_by_a_let_arith_string
//         -> pin_let_arith_reads
//       const_prop_folds_straight_line_arith_read
//         -> pin_const_gate_fires + pin_const_gate_refuses
//       decl_guard_sees_both_shapes -> pin_export_guards
//     The three fold_arith_literals_* tests carry no pin (mechanics
//     proved in arith_folds.rs — recorded, not silently skipped).
//   - Guards (verus-sync-coverage.sh `optimize` arms):
//       census entries (expr_reads/stmts_read/stmt_reads),
//       baked-index arm (index_name_refs), star-join IFS rule,
//       bare-arith-ident scan, decl-guard entries, drop gate,
//       const_prop + dead_store_elim entries.
//
// MUTANT (checked red): dropping IFS from the star-join marked set
// (the [@]-rule applied to [*]) makes the same command report 16
// verified, 1 error — pin_star_join_reads_ifs goes red. The file
// was restored and re-checked at 17 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 optimize.rs
//   -> verification results:: 17 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Var = nat;

// IFS is var 0 by convention.
pub open spec fn ifs_var() -> Var {
    0nat
}

// Leaf read shapes (flat — no nesting, so the census induction
// below is structural and fuel-free).
pub enum Leaf {
    Plain,
    DirectReads(Seq<Var>),
    BakedIndex(Var),
    ParamRead(Var),
    RedirectTarget(Var),
    StarJoin(Var),
    AtJoin(Var),
    LetArith(Seq<Var>),
    ExportGuard(Seq<Var>),
}

pub open spec fn seq_has(vs: Seq<Var>, v: Var) -> bool
    decreases vs.len(),
{
    if vs.len() == 0 {
        false
    } else {
        vs[0] == v || seq_has(vs.drop_first(), v)
    }
}

// The census verdict per shape, as a membership test (boolean —
// no set theory, so the induction below stays solver-friendly).
pub open spec fn leaf_has(l: Leaf, v: Var) -> bool {
    match l {
        Leaf::Plain => false,
        Leaf::DirectReads(vs) => seq_has(vs, v),
        Leaf::BakedIndex(i) => i == v,
        Leaf::ParamRead(n) => n == v,
        Leaf::RedirectTarget(n) => n == v,
        Leaf::StarJoin(a) => a == v || ifs_var() == v,
        Leaf::AtJoin(a) => a == v,
        Leaf::LetArith(vs) => seq_has(vs, v),
        Leaf::ExportGuard(vs) => seq_has(vs, v),
    }
}

// The census union over a statement list, as a membership test.
pub open spec fn marks_mem(ss: Seq<Leaf>, v: Var) -> bool
    decreases ss.len(),
{
    if ss.len() == 0 {
        false
    } else {
        leaf_has(ss[0], v) || marks_mem(ss.drop_first(), v)
    }
}

// A store drops when its target is unmarked in every later statement.
pub open spec fn drop_ok(target: Var, later: Seq<Leaf>) -> bool {
    !marks_mem(later, target)
}

// The dual: every position is unread (structural twin for the
// agreement equation below).
pub open spec fn all_unread(ss: Seq<Leaf>, t: Var) -> bool
    decreases ss.len(),
{
    if ss.len() == 0 {
        true
    } else {
        !leaf_has(ss[0], t) && all_unread(ss.drop_first(), t)
    }
}

// THE OBLIGATION (two-sided agreement, the seq_agrees shape):
// marked-somewhere == NOT unread-everywhere.
proof fn mem_agrees(ss: Seq<Leaf>, t: Var)
    ensures marks_mem(ss, t) == !all_unread(ss, t),
    decreases ss.len(),
{
    if ss.len() == 0 {
    } else {
        mem_agrees(ss.drop_first(), t);
    }
}

// Unmarked in the list ==> unread at every position.
proof fn unmarked_unread(target: Var, later: Seq<Leaf>)
    requires drop_ok(target, later),
    ensures all_unread(later, target),
    decreases later.len(),
{
    if later.len() == 0 {
    } else {
        mem_agrees(later, target);
    }
}

// The const_prop gate: straight-line + ordered + Const + numeric
// context (all four must hold; dropping any blocks the fold).
pub open spec fn const_gate(straight: bool, ordered: bool, is_const: bool, numeric_ctx: bool) -> bool {
    straight && ordered && is_const && numeric_ctx
}

// ---- Fidelity pins (mirror 9 of optimize.rs's 12 unit tests) ----

// x=5; echo hi -> the x store drops (never marked later).
proof fn pin_unread_drops()
    ensures drop_ok(3nat, seq![Leaf::Plain]),
{
    reveal_with_fuel(marks_mem, 3);
}

// Baked index names decode to reads (tpx[$sp_i] reads sp_i).
proof fn pin_baked_index_refs()
    ensures leaf_has(Leaf::BakedIndex(4nat), 4nat),
{
}

// i=1 survives: the arr[$i] target marks i.
proof fn pin_baked_index_keeps()
    ensures !drop_ok(4nat, seq![Leaf::BakedIndex(4nat)]),
{
    reveal_with_fuel(marks_mem, 4);
}

// param("slice","s",..) marks s: the store stays.
proof fn pin_param_reads()
    ensures !drop_ok(5nat, seq![Leaf::ParamRead(5nat)]),
{
    reveal_with_fuel(marks_mem, 4);
}

// Redirect-target getVar(f) marks f: the store stays.
proof fn pin_redirect_target_reads()
    ensures !drop_ok(6nat, seq![Leaf::RedirectTarget(6nat)]),
{
    reveal_with_fuel(marks_mem, 4);
}

// [*] marks IFS; [@] marks exactly its array (the precision arm).
proof fn pin_star_join_reads_ifs()
    ensures !drop_ok(ifs_var(), seq![Leaf::StarJoin(7nat)])
        && leaf_has(Leaf::AtJoin(7nat), 7nat) && !leaf_has(Leaf::AtJoin(7nat), ifs_var()),
{
    reveal_with_fuel(marks_mem, 4);
    assert(7nat != ifs_var());
}

// `let "x > 3"` marks the bare ident x: the store stays.
proof fn pin_let_arith_reads()
    ensures !drop_ok(8nat, seq![Leaf::LetArith(seq![8nat])]),
{
    reveal_with_fuel(marks_mem, 5);
    reveal_with_fuel(seq_has, 4);
}

// The const gate fires when all four premises hold ...
proof fn pin_const_gate_fires()
    ensures const_gate(true, true, true, true),
{
}

// ... and refuses when any one drops (unordered here).
proof fn pin_const_gate_refuses()
    ensures !const_gate(true, false, true, true),
{
}

// export/declare guard names in both A1 shapes; a non-name is unmarked.
proof fn pin_export_guards()
    ensures leaf_has(Leaf::ExportGuard(seq![9nat]), 9nat)
        && !leaf_has(Leaf::ExportGuard(seq![9nat]), 10nat),
{
    reveal_with_fuel(seq_has, 4);
}

// The drop theorem on a witness: the dropped store is unread later.
proof fn pin_drop_agrees()
    ensures all_unread(seq![Leaf::Plain], 3nat),
{
    reveal_with_fuel(marks_mem, 3);
    unmarked_unread(3nat, seq![Leaf::Plain]);
}

fn main() {}

}
