// Standalone model of the C backend's Eq/Ne EMISSION decision
// (sh2perl/src/c_backend.rs: `expr()`'s BinOp Eq|Ne arm plus its
// `str_cmp_valued` predicate). The first Verus model aimed at the C
// GENERATOR's own render decision rather than a shared shIR->shIR pass.
//
// THE DECISION, verbatim in structure (as FIXED in the source
// 2026-10-28 — this model previously pinned the PRE-FIX bug):
//   * a literal-string operand (Str/Interpolate/Capture) OR two
//     certainly-text operands (`str_cmp_valued`)  ->  a STRING
//     compare: `(strcmp(l, r) ==/!= 0)`. A non-text side is rendered
//     through the backend's canonical stringify (`value_c`: itoa /
//     pyfloat / mpz decimal) so both sides are `char*`; `_sh_sc`
//     restores the NULL safety the pointer compare it replaces used to
//     have;
//   * otherwise  ->  a RAW `==`/`!=` on the operand emissions (both
//     C integers — the numeric lift wraps its operands in `Arith`, so
//     a `Str` operand can never come from numeric intent).
// The raw arm is meaningful only when BOTH operand emissions are C
// integers. When one side is a `char*` it compares an integer to a
// pointer: UNDEFINED BEHAVIOUR in C — and gcc says so
// ("warning: comparison between pointer and integer").
//
// WHY THIS EXISTS (measured 2026-10-27, on the 563-example corpus):
// `test-lowering`'s string-eq lift turns `[ "$i" = "2" ]` into
// `Not(BinOp{Ne}, Var(i), Str("2"))` — a STRING compare by the lift's own
// contract ("every other backend maps Ne to its string-capable
// inequality"). When `i` is numeric-homed (loop counters, the common
// case), PRE-FIX `str_cmp_valued(Var i)` was FALSE (is_num) while the
// literal was text, so the decision took the RAW arm and emitted
// `i != "2"` — a pointer/integer comparison. Exactly 2 of 564 renders
// carried the gcc warning (or-continue, eq-string-num-var — both from
// this lift), and the C gate (harness/c_gate_main.sh) failed 5 files
// whose transforms-ON output diverged from bash while transforms-OFF
// matched: or-continue (silent — no line printed), func-after-test-or
// (rc=134, abort on UB — a sibling defect, the chain arm's unowned
// literal store), eq-string-num-var, 083_process_sub_missing_files and
// 064_12_brace_expansion_nested_sequences (the last two, plus
// func-after-test-or's lift half, bisected to OTHER render decisions;
// 083 <- `process-subst`, 064_12 <- `brace-fold` are separate bugs).
// The fix landed in the decision itself (either-side string trigger +
// value_c stringify); or-continue and eq-string-num-var now PASS the
// gate. This file models the FIXED decision and keeps the pre-fix one
// as the mutant the negative control drops into.
//
// WHAT IS PROVED:
//   * emit_string_cmp_sound  — the strcmp arm is TOTAL (value_c
//     stringifies a numeric side, `_sh_sc` covers NULL) and equals the
//     shIR STRING compare whenever the decision's trigger fires;
//   * emit_raw_cmp_sound     — the raw arm equals the shIR NUMERIC compare
//     whenever both operand emissions are C integers (NULL counts as 0,
//     exactly as C's null-pointer constant compares);
//   * raw_arm_ub_on_text     — an operand that is a non-null `char*`
//     makes the raw arm's C evaluation UB (modelled as `None`) — the
//     hazard the mutant below re-enters;
//   * shipped_mixed_pair_fixed_sound — on the measured mixed pair the
//     FIXED decision takes the string arm and denotes the shIR compare,
//     while `decide_old` (the pre-fix both-text-only trigger) takes the
//     raw arm and is UB: the fix and its counterexample in one place;
//   * intent_sound_guard    — the CORRECT guard, per intended semantics:
//     a string-intent compare (the string-eq lift) is sound under the
//     decision's trigger, a numeric-intent compare (the `-eq`/`-lt`
//     lift) under both_int; under it the decision's output denotes the
//     shIR meaning.
//
// WHAT IS NOT MODELLED: the operand renderers themselves (`value_c`/
// `expr`) beyond their RESULT's C type and value — the oracle `cval`
// supplies both, as the real decision derives them from the var-verdict
// tables (is_num / is_float / bigint_vars). The string-eq lift's own
// refusals are out of scope (test_lowering's model covers its guards);
// this file is about the BACKEND's two-armed decision.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_eqne_emit.rs
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Str = Seq<char>;

// A C value as an operand emission can produce it. `Null`: a global
// `char*` read before first assignment — the NULL `_sh_sc` restores
// NULL-safety for.
pub enum CVal {
    Int(int),
    Str(Str),
    Null,
}

pub open spec fn is_intv(cv: CVal) -> bool {
    match cv {
        CVal::Int(_) => true,
        _ => false,
    }
}

pub open spec fn is_strv(cv: CVal) -> bool {
    match cv {
        CVal::Str(_) => true,
        _ => false,
    }
}

// A variable's homing verdict, reduced to what `str_cmp_valued` consults
// (is_num / is_float / bigint_vars all mean "not text").
pub enum Homing {
    Num,
    Str,
}

pub enum Operand {
    // IrExpr::Str -> a C string literal (never NULL).
    TextLit(Str),
    // IrExpr::Var/Ident -> `char*` or `long long` per the verdict tables.
    VarRead(Name),
}

// The C value of an operand's emission under the verdict map and the
// value store. A name with no assignment reads as NULL (the C backend's
// global-read / `_sh_env` semantics for an un-stored name).
pub open spec fn cval(h: Map<Name, Homing>, v: Map<Name, CVal>, o: Operand) -> CVal {
    match o {
        Operand::TextLit(s) => CVal::Str(s),
        Operand::VarRead(n) => if v.dom().contains(n) { v[n] } else { CVal::Null },
    }
}

// WELL-FORMEDNESS of the verdict/store pair: a variable's value is a C
// integer exactly when its verdict homes it numeric. The C backend
// maintains this by construction — the homing tables choose the
// DECLARATION (`long long` vs `char*`) and every store to that name then
// renders through the matching bridge — and it is what makes
// `str_cmp_valued` and the emission agree.
pub open spec fn well_formed(h: Map<Name, Homing>, v: Map<Name, CVal>) -> bool {
    forall|n: Name|
        v.dom().contains(n) ==>
            (is_intv(v[n]) == (h.dom().contains(n) && h[n] == Homing::Num))
}

// `str_cmp_valued`, mirroring the source exactly: literals are certainly
// text; a variable is text exactly when its verdict is NOT numeric. An
// UNVERDICTED name is text (the source's `!self.is_num(name)` is true for
// a name the tables do not mention).
pub open spec fn str_cmp_valued(h: Map<Name, Homing>, o: Operand) -> bool {
    match o {
        Operand::TextLit(_) => true,
        Operand::VarRead(n) => !h.dom().contains(n) || h[n] == Homing::Str,
    }
}

// The decision (the source's Eq|Ne arm, FIXED 2026-10-28: a
// literal-string operand pins the string arm — the string-eq lift and
// the case lowering both build `Ne(subject, Str)` while the numeric
// lift wraps BOTH operands in `Arith` and never emits a `Str` — or two
// certainly-text operands, the original rule).
pub enum Emit {
    StrCmp,
    Raw,
}

pub open spec fn strish(o: Operand) -> bool {
    match o {
        Operand::TextLit(_) => true,
        Operand::VarRead(_) => false,
    }
}

pub open spec fn decide(h: Map<Name, Homing>, l: Operand, r: Operand) -> Emit {
    if strish(l) || strish(r) || (str_cmp_valued(h, l) && str_cmp_valued(h, r)) {
        Emit::StrCmp
    } else {
        Emit::Raw
    }
}

// The PRE-FIX decision (both-certainly-text only): kept as the MUTANT
// the negative control drops the either-side trigger back into.
pub open spec fn decide_old(h: Map<Name, Homing>, l: Operand, r: Operand) -> Emit {
    if str_cmp_valued(h, l) && str_cmp_valued(h, r) { Emit::StrCmp } else { Emit::Raw }
}

// What the strcmp arm compares: the text a C value contributes through
// `_sh_sc` (NULL -> "", string -> itself). `Int` is unreachable under
// the arm's own guard (both_str_no_int below); the total definition keeps
// the model total without asserting anything about that case.
pub open spec fn sc_text(cv: CVal) -> Str {
    match cv {
        CVal::Str(s) => s,
        CVal::Null => Seq::empty(),
        CVal::Int(_) => Seq::empty(),
    }
}

// The DECIMAL string `value_c` renders for a C integer (num_temp →
// `_sh_ll2s`). Uninterpreted here ON PURPOSE: the spelling itself is
// proved elsewhere — c_dec_str.rs `ll2s_exact` (byte-for-byte `%lld`)
// and ArithDec.thy's `render`/`parse_render_id` (the shell round trip).
// What THIS file needs is only that the emitted strcmp reads both sides
// through one spelling, which `sform` gives it.
pub closed spec fn itoa(i: int) -> Str;

// What each operand contributes to the fixed decision's strcmp: a text
// side as itself (through `_sh_sc`: Null -> ""), a numeric side through
// value_c's itoa. TOTAL — the fixed arm never UBs on an operand type.
pub open spec fn sform(cv: CVal) -> Str {
    match cv {
        CVal::Int(i) => itoa(i),
        other => sc_text(other),
    }
}

// What a raw C `==`/`!=` over the operand sees: an integer. A non-null
// `char*` compared against an integer is UB in C — `None`.
pub open spec fn num_val(cv: CVal) -> Option<int> {
    match cv {
        CVal::Int(i) => Option::Some(i),
        // C: the null pointer constant compares equal to 0 — defined.
        CVal::Null => Option::Some(0int),
        CVal::Str(_) => Option::None,
    }
}

// The intended shIR semantics of the comparison, carried as data: the
// string-eq lift intends STRING equality (bash `[ x = y ]`), the numeric
// lift (`-eq -ne -lt …`) intends the numeric comparison.
pub enum Intent {
    StringEq,
    NumericEq,
}

// shIR denotation of `l == r`. Ne is the negation in BOTH arms (the
// source renders it `strcmp(…) != 0` / raw `!=`), so the Eq model plus
// c_eval_ne/shir_denote_ne below covers it.
pub open spec fn shir_denote(intent: Intent, l: CVal, r: CVal) -> bool {
    match intent {
        Intent::StringEq => sform(l) == sform(r),
        Intent::NumericEq => num_val(l) == num_val(r),
    }
}

// The C program the decision EMITS, evaluated. `None` = UB (an operand
// of the wrong C type under a raw operator, or a non-text operand under
// a strcmp the guard should have excluded).
pub open spec fn c_eval(e: Emit, l: CVal, r: CVal) -> Option<bool> {
    match e {
        // The FIXED arm stringifies a non-text side through value_c
        // before the strcmp, so every operand type is well defined here
        // (`_sh_sc` covers NULL, itoa covers C integers).
        Emit::StrCmp => Option::Some(sform(l) == sform(r)),
        Emit::Raw => match (num_val(l), num_val(r)) {
            (Option::Some(a), Option::Some(b)) => Option::Some(a == b),
            _ => Option::None,
        },
    }
}

// Ne flips the result in both arms: one definition ties the Eq model to
// the Ne emission the measured pair actually went through.
pub open spec fn c_eval_ne(e: Emit, l: CVal, r: CVal) -> Option<bool> {
    match c_eval(e, l, r) {
        Option::Some(b) => Option::Some(!b),
        Option::None => Option::None,
    }
}

pub open spec fn shir_denote_ne(intent: Intent, l: CVal, r: CVal) -> bool {
    !shir_denote(intent, l, r)
}

// ── The obligations ───────────────────────────────────────────────────

// Certainly-text operands are never C integers: the strcmp arm's
// operand-shape guard is exactly what makes it well typed.
proof fn both_str_no_int(h: Map<Name, Homing>, v: Map<Name, CVal>, l: Operand, r: Operand)
    requires well_formed(h, v),
        str_cmp_valued(h, l) && str_cmp_valued(h, r)
    ensures !is_intv(cval(h, v, l)), !is_intv(cval(h, v, r))
{
    match l {
        Operand::TextLit(_) => {},
        Operand::VarRead(n) => {
            if v.dom().contains(n) {
                assert(is_intv(v[n]) == (h.dom().contains(n) && h[n] == Homing::Num));
                if h.dom().contains(n) {
                    assert(h[n] == Homing::Str);
                }
            }
        },
    }
    match r {
        Operand::TextLit(_) => {},
        Operand::VarRead(n) => {
            if v.dom().contains(n) {
                if h.dom().contains(n) {
                    assert(h[n] == Homing::Str);
                }
            }
        },
    }
}

// THE OBLIGATION (string arm): the strcmp emission equals the shIR
// string compare, from any well-formed state — total (no UB): value_c
// stringifies a numeric side, `_sh_sc` covers NULL. The precondition is
// the FIXED decision's trigger (a Str operand pins the arm, or both are
// certainly-text).
proof fn emit_string_cmp_sound(h: Map<Name, Homing>, v: Map<Name, CVal>, l: Operand, r: Operand)
    requires well_formed(h, v),
        strish(l) || strish(r)
            || (str_cmp_valued(h, l) && str_cmp_valued(h, r))
    ensures c_eval(decide(h, l, r), cval(h, v, l), cval(h, v, r))
            == Option::Some(shir_denote(Intent::StringEq, cval(h, v, l), cval(h, v, r)))
{
    assert(decide(h, l, r) == Emit::StrCmp);
}

// THE OBLIGATION (numeric arm): the raw emission equals the shIR numeric
// compare whenever both operand emissions are C integers.
proof fn emit_raw_cmp_sound(l: CVal, r: CVal)
    requires num_val(l).is_Some() && num_val(r).is_Some()
    ensures c_eval(Emit::Raw, l, r) == Option::Some(shir_denote(Intent::NumericEq, l, r))
{
}

// The UB direction: a text operand under the RAW arm — the measured
// shape — makes the emitted C undefined.
proof fn raw_arm_ub_on_text(l: CVal, r: CVal)
    requires is_strv(r)
    ensures c_eval(Emit::Raw, l, r) == Option::None
{
}

// The concrete case the corpus reds WERE. Numeric-homed `i` holding
// the C integer 2, compared (string intent) against the literal "2":
// the FIXED decision takes the string arm (the Str operand pins it)
// and denotes the shIR compare; the pre-fix `decide_old` (mutant: the
// either-side trigger deleted) took the raw arm and its evaluation is
// UB — the exact shape that emitted `i != "2"`.
pub open spec fn shipped_h() -> Map<Name, Homing> {
    Map::empty().insert(0nat, Homing::Num)
}

pub open spec fn shipped_v() -> Map<Name, CVal> {
    Map::empty().insert(0nat, CVal::Int(2int))
}

pub open spec fn shipped_l() -> Operand {
    Operand::VarRead(0nat)
}

pub open spec fn shipped_r() -> Operand {
    Operand::TextLit(Seq::empty().push('2'))
}

proof fn shipped_mixed_pair_fixed_sound()
    ensures well_formed(shipped_h(), shipped_v()),
        // the fix: string arm, denoting the shIR compare (total, no UB)
        decide(shipped_h(), shipped_l(), shipped_r()) == Emit::StrCmp,
        c_eval(decide(shipped_h(), shipped_l(), shipped_r()),
               cval(shipped_h(), shipped_v(), shipped_l()),
               cval(shipped_h(), shipped_v(), shipped_r())) == Option::Some(
                   shir_denote(Intent::StringEq,
                               cval(shipped_h(), shipped_v(), shipped_l()),
                               cval(shipped_h(), shipped_v(), shipped_r()))),
        c_eval_ne(decide(shipped_h(), shipped_l(), shipped_r()),
                  cval(shipped_h(), shipped_v(), shipped_l()),
                  cval(shipped_h(), shipped_v(), shipped_r())) == Option::Some(
                      shir_denote_ne(Intent::StringEq,
                                     cval(shipped_h(), shipped_v(), shipped_l()),
                                     cval(shipped_h(), shipped_v(), shipped_r()))),
        // the mutant: pre-fix decision → raw arm → UB (the Ne form the
        // lift actually emitted has the same fate)
        decide_old(shipped_h(), shipped_l(), shipped_r()) == Emit::Raw,
        c_eval(decide_old(shipped_h(), shipped_l(), shipped_r()),
               cval(shipped_h(), shipped_v(), shipped_l()),
               cval(shipped_h(), shipped_v(), shipped_r())) == Option::None,
        c_eval_ne(decide_old(shipped_h(), shipped_l(), shipped_r()),
                  cval(shipped_h(), shipped_v(), shipped_l()),
                  cval(shipped_h(), shipped_v(), shipped_r())) == Option::None
{
    assert(well_formed(shipped_h(), shipped_v()));
    assert(strish(shipped_r()));
    assert(decide(shipped_h(), shipped_l(), shipped_r()) == Emit::StrCmp);
    emit_string_cmp_sound(shipped_h(), shipped_v(), shipped_l(), shipped_r());
    assert(str_cmp_valued(shipped_h(), shipped_l()) == false);
    assert(decide_old(shipped_h(), shipped_l(), shipped_r()) == Emit::Raw);
    raw_arm_ub_on_text(cval(shipped_h(), shipped_v(), shipped_l()),
                       cval(shipped_h(), shipped_v(), shipped_r()));
}

// THE CORRECT GUARD, per intended semantics. A string-intent compare is
// sound whenever the fixed decision's trigger fires (a Str operand or
// both text); a numeric-intent compare (`-eq`/`-lt` lift: both operands
// C integers) when both operand emissions are C integers. Under it the
// decision's output denotes the shIR meaning — no UB, no silent
// reinterpretation.
pub open spec fn intent_sound(h: Map<Name, Homing>, v: Map<Name, CVal>, intent: Intent,
                              l: Operand, r: Operand) -> bool
{
    match intent {
        // string intent: the FIXED decision's trigger (a Str operand
        // pins the arm — the mixed pair included — or both text)
        Intent::StringEq => strish(l) || strish(r)
            || (str_cmp_valued(h, l) && str_cmp_valued(h, r)),
        Intent::NumericEq => num_val(cval(h, v, l)).is_Some() && num_val(cval(h, v, r)).is_Some(),
    }
}

proof fn intent_sound_guard(h: Map<Name, Homing>, v: Map<Name, CVal>, intent: Intent,
                            l: Operand, r: Operand)
    requires well_formed(h, v), intent_sound(h, v, intent, l, r)
    ensures c_eval(decide(h, l, r), cval(h, v, l), cval(h, v, r))
            == Option::Some(shir_denote(intent, cval(h, v, l), cval(h, v, r)))
{
    match intent {
        Intent::StringEq => emit_string_cmp_sound(h, v, l, r),
        Intent::NumericEq => emit_raw_cmp_sound(cval(h, v, l), cval(h, v, r)),
    }
}

// The Ne form of the same guard result (the lift emitted `Not(Ne)`).
proof fn intent_sound_guard_ne(h: Map<Name, Homing>, v: Map<Name, CVal>, intent: Intent,
                               l: Operand, r: Operand)
    requires well_formed(h, v), intent_sound(h, v, intent, l, r)
    ensures c_eval_ne(decide(h, l, r), cval(h, v, l), cval(h, v, r))
            == Option::Some(shir_denote_ne(intent, cval(h, v, l), cval(h, v, r)))
{
    intent_sound_guard(h, v, intent, l, r);
}

// ── Fidelity pins (the shapes the corpus actually produced) ───────────

// The string-eq lift's shape on a TEXT-homed variable: the shipped
// decision already picks the strcmp arm and is sound — which is why the
// corpus reds are exactly the numeric-homed instances.
proof fn pin_text_homed_pair_sound()
    ensures true
{
    let h = Map::empty().insert(0nat, Homing::Str);
    let v = Map::empty().insert(0nat, CVal::Str(Seq::empty().push('x')));
    let l = Operand::VarRead(0nat);
    let r = Operand::TextLit(Seq::empty().push('x'));
    emit_string_cmp_sound(h, v, l, r);
    assert(shir_denote(Intent::StringEq, cval(h, v, l), cval(h, v, r)));
}

// An unset variable (NULL) under the strcmp arm: `_sh_sc` restores the
// NULL safety — the emission still denotes the string compare (equal to
// the empty literal here).
proof fn pin_null_sc_restores()
    ensures true
{
    let h = Map::empty();
    let v = Map::empty();
    let l = Operand::VarRead(0nat);
    let r = Operand::TextLit(Seq::empty());
    assert(cval(h, v, l) == CVal::Null);
    assert(sc_text(CVal::Null) == Seq::empty());
    emit_string_cmp_sound(h, v, l, r);
}

// The numeric lift's shape (`-lt` on a numeric-homed var vs an integer
// literal): the numeric LIFT wraps its operands in an Arith node whose C
// emission is an integer (the atoll bridge) even for a text-homed var —
// that wrapper is what keeps the numeric lift out of the mixed-pair
// hole, and why the string-eq lift (which emits BARE operands) is the
// one that falls in. Here: the numeric reading of the shipped pair's
// lhs, which the raw arm WOULD have been sound on.
proof fn pin_numeric_pair_raw_sound()
    ensures c_eval(Emit::Raw, cval(shipped_h(), shipped_v(), shipped_l()), CVal::Int(2int))
            == Option::Some(true)
{
    emit_raw_cmp_sound(cval(shipped_h(), shipped_v(), shipped_l()), CVal::Int(2int));
}

fn main() {}

} // verus!
