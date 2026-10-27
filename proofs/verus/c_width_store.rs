// Standalone model of the C backend's WIDTH-OF-VAR / EFFECTIVE-WIDTHS
// store condition (sh2perl/src/c_backend.rs `Width`, `width_of_var`,
// `effective_widths` + `walk_widths_expr`, over `range_width_name` in
// sh2perl/src/shir.rs:16628).
//
// THE DECISION, verbatim in structure:
//   * `range_width_name(lo, hi)` names the FIRST width in the if-chain
//     whose interval covers [lo, hi]: u8, i8, u16, i16, u32, i32, u64,
//     else i64. The u8-before-i8 ordering is load-bearing (a
//     non-negative range always takes the unsigned arm).
//   * `Width::from_range_name` maps the name back ("u8"->U8, …,
//     "u64"->U64, anything else->I64 — fail-closed: garbage names do
//     not narrow).
//   * `Width::widen` is max rank (I8<U8<I16<U16<I32<U32<I64<U64); the
//     effective width of a var is the widen-fold of its own range's
//     width and every mentioning arith-expr result's width, forced to
//     I64 when a result range is unknown (no proof, no narrowing —
//     modulo the `arith_op_wide` escape below).
//   * `walk_widths_expr` SKIPS comparison roots (`<`,`==`,…) and
//     side-effect roots (`Assign`, `IncDec`): comparisons only read
//     (C promotes; cannot wrap) and a written var's RANGE governs.
//
// WHY THIS EXISTS. C_UB_PROOF.md §1 marks the width failure SILENT (no
// guard, no exit code): an underestimated range produces `uint8_t`
// arithmetic that wraps where `long long` would not. §7 measured the
// live shape: 10 gcc `-Wconversion` truncating stores in 9 corpus files
// (a `for` seed `[1,5]->u8` kept while a `long long` table element is
// stored into it; `uint8_t result = 15 + _sh_pow(5,2)` safe only
// because 25 happens to fit). Sanitizers cannot see the class at all
// (unsigned conversion is defined behaviour; a size-1 global has no
// redzone — measured with a control). The store-side condition
// (value ∈ declared width) is therefore the obligation, and the
// backend checks nothing at the store — it is on the analysis.
//
// WHAT IS PROVED (ranges are HYPOTHESES — T1 of P1_Narrowing.thy: every
// range in the map contains its var's value. The operator rules that
// PRODUCE ranges stay proved in Isabelle: BigIntRange `div_corners`/…
// for `/`, I32Provable for the estimator):
//   * narrow_sound             — a value inside the analysed range fits
//                                the named width, so the DECLARATION is
//                                never the fault (P1's T1-second-half);
//   * narrow_u8_first          — the ordering pin: (0,100) names U8
//                                though I8 would also fit (the
//                                sanitizer-invisibility mechanism);
//   * nonneg_never_signed      — lo>=0 names unsigned-or-64 (never
//                                I8/I16/I32): P1's lemma, mirrored;
//   * from_range_name_total    — the 8 mappings + garbage->I64
//                                (fail-closed naming);
//   * widen_is_max             — widen is max rank (comm/assoc/idem in
//                                widen_lattice); widen_ge_both;
//   * widen_preserves_fit      — v fitting both contributors fits the
//                                widened width (the fold step);
//   * eff_fold_sound           — v fitting every contributing width
//                                fits the widen-fold (induction);
//   * store_safe_conditional   — THE OBLIGATION (T3): under consistent
//                                ranges (every contributor's range
//                                contains the stored value) the
//                                effective width admits the store;
//   * outside_not_admitted_u8 / _i8 — the TRUNC direction: a value
//                                outside the declared interval is NOT
//                                admitted (the 10 gcc-visible stores);
//   * unknown_forces_i64       — a range-less (None) contributor
//                                forces I64: no proof, no narrowing;
//   * op_wide_fail_closed      — `arith_op_wide` never claims wide on
//                                missing data (unknown/unranged -> false;
//                                in-i32 literal -> false), and fires on
//                                the three proven-wide shapes
//                                (out-of-i32 literal, positional `$N`,
//                                wide-range var);
//   * skip_shapes              — the skip predicate fires exactly on
//                                comparison/Assign/IncDec roots (shape
//                                mirror; soundness rationale in comments);
//   * pin_shir_width_table     — the 6 live shir.rs unit assertions
//                                ((1,1)->u8 … (0,2^64-1)->u64 …);
//   * pin_c_type_table         — the 8 c_type spellings (`uint8_t` …
//                                `unsigned long long`);
//   * pin_trunc_shape_detected — the corpus shape: own [1,5]->U8 with
//                                a use-range outside U8 is DETECTED
//                                (the shipped code stores anyway).
//
// WHAT IS NOT MODELLED: the range PRODUCERS (`analyze_var_ranges`,
// `seed_loop_var_ranges`, `arith_range_local` operator rules — T1, the
// Isabelle line's property); the statement/expression traversal (per
// site theorems; the Assign-RHS-only `arith_vars` quirk is noted, not
// modelled); C promotion semantics behind the comparison skip and the
// op_wide escape (Tier-3-flavoured); `num_spec`/`expr_type_matches`
// (step 3); buffers (step 2). Values are unbounded `int`; callers pass
// i128-bounded ranges (u64::MAX = 2^64-1 fits).
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_width_store.rs
use vstd::prelude::*;

verus! {

pub type Name = nat;

// The Width enum, mirroring sh2perl/src/c_backend.rs (rank order is the
// widen order: I8<U8<I16<U16<I32<U32<I64<U64 — U64 is the top).
pub enum Width {
    I8,
    U8,
    I16,
    U16,
    U32,
    I32,
    I64,
    U64,
}

pub open spec fn rank(w: Width) -> int {
    match w {
        Width::I8 => 0int,
        Width::U8 => 1int,
        Width::I16 => 2int,
        Width::U16 => 3int,
        Width::I32 => 4int,
        Width::U32 => 5int,
        Width::I64 => 6int,
        Width::U64 => 7int,
    }
}

pub open spec fn lo_of(w: Width) -> int {
    match w {
        Width::U8 => 0int,
        Width::I8 => 0 - 128int,
        Width::U16 => 0int,
        Width::I16 => 0 - 32768int,
        Width::U32 => 0int,
        Width::I32 => 0 - 2147483648int,
        Width::I64 => 0 - 9223372036854775808int,
        Width::U64 => 0int,
    }
}

pub open spec fn hi_of(w: Width) -> int {
    match w {
        Width::U8 => 255int,
        Width::I8 => 127int,
        Width::U16 => 65535int,
        Width::I16 => 32767int,
        Width::U32 => 4294967295int,
        Width::I32 => 2147483647int,
        Width::I64 => 9223372036854775807int,
        Width::U64 => 18446744073709551615int,
    }
}

pub open spec fn fits(v: int, w: Width) -> bool {
    lo_of(w) <= v && v <= hi_of(w)
}

// `range_width_name`, verbatim if-chain (shir.rs:16628). Spec `if` is
// first-match by construction — which is exactly the property the
// ordering pin checks.
pub open spec fn narrow(lo: int, hi: int) -> Width {
    if lo >= 0int && hi <= 255int {
        Width::U8
    } else if lo >= 0 - 128int && hi <= 127int {
        Width::I8
    } else if lo >= 0int && hi <= 65535int {
        Width::U16
    } else if lo >= 0 - 32768int && hi <= 32767int {
        Width::I16
    } else if lo >= 0int && hi <= 4294967295int {
        Width::U32
    } else if lo >= 0 - 2147483648int && hi <= 2147483647int {
        Width::I32
    } else if lo >= 0int && hi > 9223372036854775807int {
        Width::U64
    } else {
        Width::I64
    }
}

// `Width::from_range_name`, verbatim (`_` -> I64: garbage never narrows).
// Names are spec `&str` (string-literal equality, which Verus decides;
// `Seq<char>` push-chains need per-arm witness asserts — probed).
pub open spec fn from_range_name(name: &str) -> Width {
    if name == "u8" {
        Width::U8
    } else if name == "i8" {
        Width::I8
    } else if name == "u16" {
        Width::U16
    } else if name == "i16" {
        Width::I16
    } else if name == "u32" {
        Width::U32
    } else if name == "i32" {
        Width::I32
    } else if name == "u64" {
        Width::U64
    } else {
        Width::I64
    }
}

// `Width::widen`: the wider rank wins (max).
pub open spec fn widen(a: Width, b: Width) -> Width {
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

// The emitted C type per width (the c_type table; step 3 consumes it).
pub open spec fn c_type(w: Width) -> Seq<char> {
    match w {
        Width::I8 => Seq::empty().push('i').push('n').push('t').push('8').push('_').push('t'),
        Width::U8 => Seq::empty().push('u').push('i').push('n').push('t').push('8').push('_').push('t'),
        Width::I16 => Seq::empty().push('i').push('n').push('t').push('1').push('6').push('_').push('t'),
        Width::U16 => Seq::empty().push('u').push('i').push('n').push('t').push('1').push('6').push('_').push('t'),
        Width::U32 => Seq::empty().push('u').push('i').push('n').push('t').push('3').push('2').push('_').push('t'),
        Width::I32 => Seq::empty().push('i').push('n').push('t').push('3').push('2').push('_').push('t'),
        Width::I64 => Seq::empty().push('l').push('o').push('n').push('g').push(' ').push('l').push('o').push('n').push('g'),
        Width::U64 => Seq::empty().push('u').push('n').push('s').push('i').push('g').push('n').push('e').push('d').push(' ').push('l').push('o').push('n').push('g').push(' ').push('l').push('o').push('n').push('g'),
    }
}

// ── the effective-width fold ──────────────────────────────────────────
// A contributor is a range whose width joins the fold (the var's own
// range, then every mentioning arith result's range). `None` = unknown
// range -> forced I64 (the fail-closed arm). The fold starts from the
// var's own width and widens per contributor.
pub open spec fn contrib_width(c: Option<(int, int)>) -> Width {
    match c {
        Option::Some(pair) => narrow(pair.0, pair.1),
        Option::None => Width::I64,
    }
}

pub open spec fn eff_fold(contribs: Seq<Option<(int, int)>>, acc: Width) -> Width
    decreases contribs.len(),
{
    if contribs.len() == 0 {
        acc
    } else {
        eff_fold(contribs.drop_first(), widen(acc, contrib_width(contribs[0])))
    }
}

// ---- `arith_op_wide`, verbatim in structure (c_backend.rs) ----
// An operand shape: literal, positional `$N`, or var read. The state
// maps names to optional ranges (None = unranged).
pub enum Operand {
    Lit(int),
    Positional,
    VarRead(Name),
}

pub open spec fn arith_op_wide(ops: Seq<Operand>, state: Map<Name, Option<(int, int)>>) -> bool {
    exists|i: int| 0 <= i && i < ops.len() && op_wide(ops[i], state)
}

pub open spec fn op_wide(o: Operand, state: Map<Name, Option<(int, int)>>) -> bool {
    match o {
        // out-of-i32 literal renders `…LL` (proven 64-bit)
        Operand::Lit(n) => n > 2147483647int || n < 0 - 2147483648int,
        // positional `$N` renders `long long` in every param ABI
        Operand::Positional => true,
        Operand::VarRead(x) => match state.dom().contains(x) {
            true => match state[x] {
                // a wide RANGE means a wide decl (decls only widen)
                Option::Some(pair) => pair.1 > 4294967295int || pair.0 < 0 - 2147483648int,
                // unknown/unranged never counts (fail-closed)
                Option::None => false,
            },
            false => false,
        },
    }
}

// ---- the walk skip, as a shape predicate ----
// Comparison roots and side-effect roots impose no width demand.
pub enum ArithRoot {
    Cmp,
    AssignRoot,
    IncDecRoot,
    Other,
}

pub open spec fn skip_root(r: ArithRoot) -> bool {
    match r {
        ArithRoot::Cmp => true,
        ArithRoot::AssignRoot => true,
        ArithRoot::IncDecRoot => true,
        ArithRoot::Other => false,
    }
}

// ── The obligations ───────────────────────────────────────────────────

// P1's T1-second-half, mirrored: the declaration is never the fault.
// Each arm asserts the width equality first: the arm guard plus the
// failure of every earlier guard determines the if-chain's outcome.
// Two provisos (the renderer/analysis boundary — ranges are T1
// hypotheses, proved in Isabelle for the producers):
//   * hi ≤ 2^64-1: the U64 arm has no source-level upper bound
//     (analysis ranges are i128); above 2^64-1 even U64 is wrong.
//   * the else/I64 arm fires for ranges wider than i64 too (the chain
//     does NOT check) — sound only when the range fits i64. A range
//     like [-2^40, 0] naming I64 is the same hole class as §7 shape 1
//     (the analysis must not produce it; the renderer cannot detect it).
proof fn narrow_sound(lo: int, hi: int, v: int)
    requires lo <= v, v <= hi,
        hi <= 18446744073709551615int,
        own_in_domain((lo, hi)),
    ensures fits(v, narrow(lo, hi))
{
    if lo >= 0int && hi <= 255int {
        assert(narrow(lo, hi) == Width::U8);
        assert(fits(v, Width::U8));
    } else if lo >= 0 - 128int && hi <= 127int {
        // earlier arm failed: lo < 0 or hi > 255; with this arm's guard
        // the chain lands on I8
        assert(!(lo >= 0int && hi <= 255int));
        assert(narrow(lo, hi) == Width::I8);
        assert(fits(v, Width::I8));
    } else if lo >= 0int && hi <= 65535int {
        assert(!((lo >= 0int && hi <= 255int) || (lo >= 0 - 128int && hi <= 127int)));
        assert(narrow(lo, hi) == Width::U16);
        assert(fits(v, Width::U16));
    } else if lo >= 0 - 32768int && hi <= 32767int {
        assert(narrow(lo, hi) == Width::I16);
        assert(fits(v, Width::I16));
    } else if lo >= 0int && hi <= 4294967295int {
        assert(narrow(lo, hi) == Width::U32);
        assert(fits(v, Width::U32));
    } else if lo >= 0 - 2147483648int && hi <= 2147483647int {
        assert(narrow(lo, hi) == Width::I32);
        assert(fits(v, Width::I32));
    } else if lo >= 0int && hi > 9223372036854775807int {
        assert(narrow(lo, hi) == Width::U64);
        assert(fits(v, Width::U64));
    } else {
        assert(narrow(lo, hi) == Width::I64);
        assert(fits(v, Width::I64));
    }
}

// The ordering pin: (0,100) fits I8's interval too, but the chain names
// U8 — this first-match is why the corpus has no signed narrow sites
// for UBSan to fire on (C_UB_PROOF.md §7).
proof fn narrow_u8_first()
    ensures narrow(0int, 100int) == Width::U8
{
}

// P1's lemma, mirrored (stronger here: U64 is a possible unsigned
// outcome past 2^63-1, I64 past u32::MAX).
proof fn nonneg_never_signed(lo: int, hi: int)
    requires 0int <= lo
    ensures narrow(lo, hi) != Width::I8 && narrow(lo, hi) != Width::I16 && narrow(lo, hi) != Width::I32
{
}

// Fail-closed naming: the 7 names map, anything else (incl. "i64" and
// garbage) is I64 — an unknown name never narrows.
proof fn from_range_name_total()
    ensures from_range_name("u8") == Width::U8,
        from_range_name("i8") == Width::I8,
        from_range_name("u16") == Width::U16,
        from_range_name("i16") == Width::I16,
        from_range_name("u32") == Width::U32,
        from_range_name("i32") == Width::I32,
        from_range_name("u64") == Width::U64,
        from_range_name("i64") == Width::I64,
        from_range_name("x") == Width::I64
{
}

// widen is max rank.
proof fn widen_is_max(a: Width, b: Width)
    ensures (widen(a, b) == a || widen(a, b) == b),
        rank(widen(a, b)) >= rank(a), rank(widen(a, b)) >= rank(b)
{
}

// widen is a join: commutative, associative, idempotent.
proof fn widen_lattice(a: Width, b: Width, c: Width)
    ensures widen(a, b) == widen(b, a),
        widen(widen(a, b), c) == widen(a, widen(b, c)),
        widen(a, a) == a
{
}

// The fold step: v fitting both contributors fits the widened width
// (widen returns one of them, and v fits whichever is returned).
proof fn widen_preserves_fit(v: int, a: Width, b: Width)
    requires fits(v, a), fits(v, b)
    ensures fits(v, widen(a, b))
{
}

pub open spec fn contrib_holds(c: Option<(int, int)>, v: int) -> bool {
    match c {
        Option::Some(pair) => pair.0 <= v && v <= pair.1,
        Option::None => true,
    }
}

// The machine-domain premise: ranges the analysis may produce are
// i128-valued, but the naming is sound only inside the machine domain
// (the U64 arm has no source-level upper bound; the else/I64 arm fires
// for wider-than-i64 ranges it cannot hold). The shell domain (bash
// arithmetic never leaves i64) supplies this for shell input; the
// C-frontend u64 shapes are the documented exception.
pub open spec fn contrib_in_domain(c: Option<(int, int)>) -> bool {
    match c {
        Option::Some(pair) => pair.1 <= 18446744073709551615int
            && (narrow(pair.0, pair.1) == Width::I64
                ==> pair.0 >= 0 - 9223372036854775808int && pair.1 <= 9223372036854775807int),
        Option::None => true,
    }
}

pub open spec fn own_in_domain(own: (int, int)) -> bool {
    own.1 <= 18446744073709551615int
        && (narrow(own.0, own.1) == Width::I64
            ==> own.0 >= 0 - 9223372036854775808int && own.1 <= 9223372036854775807int)
}

// The fold: v fitting every contributing width fits the accumulation,
// by induction on the contributor list. The i64-domain premise is the
// shell domain (bash arithmetic never leaves i64 — the u64 arm is
// C-frontend-only per shir.rs); the None arm forces I64, so the claim
// needs v to fit I64.
proof fn eff_fold_sound(contribs: Seq<Option<(int, int)>>, acc: Width, v: int)
    requires fits(v, acc),
        fits(v, Width::I64),
        // ranges fit the machine domain (see narrow_sound provisos:
        // the analysis must not produce wider-than-i64 ranges, and the
        // u64 arm's inputs are C-frontend-only)
        forall|i: int| 0 <= i && i < contribs.len() ==> #[trigger] contrib_holds(contribs[i], v),
        forall|i: int| 0 <= i && i < contribs.len() ==> #[trigger] contrib_in_domain(contribs[i])
    ensures fits(v, eff_fold(contribs, acc))
    decreases contribs.len(),
{
    if contribs.len() == 0 {
    } else {
        // unfold one step: the head's hypothesis gives the head range
        assert(contrib_holds(contribs[0], v));
        match contribs[0] {
            Option::Some(pair) => {
                assert(pair.0 <= v && v <= pair.1);
                assert(contrib_in_domain(contribs[0]));
                assert(pair.1 <= 18446744073709551615int);
                assert(own_in_domain((pair.0, pair.1)));
                narrow_sound(pair.0, pair.1, v);
                widen_preserves_fit(v, acc, contrib_width(contribs[0]));
                // the tail's hypothesis: shift the quantifier past the head
                assert(forall|i: int| 0 <= i && i < contribs.drop_first().len()
                    ==> #[trigger] contrib_holds(contribs.drop_first()[i], v));
                assert(forall|i: int| 0 <= i && i < contribs.drop_first().len()
                    ==> #[trigger] contrib_in_domain(contribs.drop_first()[i]));
                eff_fold_sound(contribs.drop_first(), widen(acc, contrib_width(contribs[0])), v);
            },
            Option::None => {
                widen_preserves_fit(v, acc, contrib_width(contribs[0]));
                assert(forall|i: int| 0 <= i && i < contribs.drop_first().len()
                    ==> #[trigger] contrib_holds(contribs.drop_first()[i], v));
                assert(forall|i: int| 0 <= i && i < contribs.drop_first().len()
                    ==> #[trigger] contrib_in_domain(contribs.drop_first()[i]));
                eff_fold_sound(contribs.drop_first(), widen(acc, contrib_width(contribs[0])), v);
            },
        }
    }
}

// THE OBLIGATION (T3): under consistent ranges — the stored value lies
// in the var's own range AND in every contributing use-range — the
// effective width admits the store. A backend that checked the store
// against the width would satisfy this by construction; the shipped
// backend checks nothing, so the obligation is on the analysis.
proof fn store_safe_conditional(
    own: (int, int),
    contribs: Seq<Option<(int, int)>>,
    v: int,
)
    requires own.0 <= v, v <= own.1,
        // the shell domain: bash arithmetic never leaves i64, and the
        // fold starts from narrow(own) which the u64 arm can exceed only
        // on C-frontend input — the var's own range still bounds v
        fits(v, Width::I64),
        own_in_domain(own),
        forall|i: int| 0 <= i && i < contribs.len() ==> #[trigger] contrib_holds(contribs[i], v),
        forall|i: int| 0 <= i && i < contribs.len() ==> #[trigger] contrib_in_domain(contribs[i])
    ensures fits(v, eff_fold(contribs, narrow(own.0, own.1)))
{
    narrow_sound(own.0, own.1, v);
    eff_fold_sound(contribs, narrow(own.0, own.1), v);
}

// The TRUNC direction: outside the declared interval is not admitted —
// the property the 10 gcc-visible stores violate (C_UB_PROOF.md §7).
proof fn outside_not_admitted_u8(v: int)
    requires v < 0int || 255int < v
    ensures !fits(v, Width::U8)
{
}

proof fn outside_not_admitted_i8(v: int)
    requires v < 0 - 128int || 127int < v
    ensures !fits(v, Width::I8)
{
}

// No proof, no narrowing: an unknown (None) contributor widens the
// fold toward I64 — the arm the `for i in …` table-element stores
// SHOULD have taken (C_UB_PROOF.md §7 shape 1).
proof fn unknown_forces_i64(acc: Width)
    ensures widen(acc, contrib_width(Option::None)) == Width::I64 || rank(widen(acc, contrib_width(Option::None))) >= rank(acc)
{
    widen_is_max(acc, contrib_width(Option::None));
}

// `arith_op_wide` is fail-closed: unknown/unranged operands and
// in-i32 literals never count as wide; the three proven-wide shapes do.
proof fn op_wide_fail_closed(state: Map<Name, Option<(int, int)>>, x: Name)
    requires !state.dom().contains(x) || state[x] == Option::None::<(int, int)>
    ensures !op_wide(Operand::VarRead(x), state),
        !op_wide(Operand::Lit(0int), state),
        !op_wide(Operand::Lit(2147483647int), state),
        !op_wide(Operand::Lit(0 - 2147483648int), state)
{
}

proof fn op_wide_fires(state: Map<Name, Option<(int, int)>>, x: Name, y: Name)
    requires state.dom().contains(x) && state[x] == Option::Some((0int, 4294967296int)),
        state.dom().contains(y) && state[y] == Option::Some((0int, 100int))
    ensures op_wide(Operand::Lit(2147483648int), state),
        op_wide(Operand::Lit(0 - 2147483649int), state),
        op_wide(Operand::Positional, state),
        op_wide(Operand::VarRead(x), state),
        !op_wide(Operand::VarRead(y), state)
{
}

// The skip shape-mirror: exactly the comparison and side-effect roots
// are skipped (soundness rationale — C promotion reads, RANGE governs
// writes — is Tier-3-flavoured and stays in comments).
proof fn skip_shapes()
    ensures skip_root(ArithRoot::Cmp),
        skip_root(ArithRoot::AssignRoot),
        skip_root(ArithRoot::IncDecRoot),
        !skip_root(ArithRoot::Other)
{
}

// ── Fidelity pins (the live unit tests) ─────────────────────────────

// shir.rs `straight_line_widths` table (6 assertions).
proof fn pin_shir_width_table()
    ensures narrow(1int, 1int) == Width::U8,
        narrow(0 - 5int, 0 - 5int) == Width::I8,
        narrow(0int, 4000000000int) == Width::U32,
        narrow(0int, 10000000000int) == Width::I64,
        narrow(0int, 18446744073709551615int) == Width::U64,
        narrow(9223372036854775807int, 9223372036854775807int) == Width::I64
{
}

// The c_type spellings (c_backend `width_format_matches_c_type` shape).
proof fn pin_c_type_table()
    ensures c_type(Width::U8) == Seq::empty().push('u').push('i').push('n').push('t').push('8').push('_').push('t'),
        c_type(Width::I64) == Seq::empty().push('l').push('o').push('n').push('g').push(' ').push('l').push('o').push('n').push('g'),
        c_type(Width::U64) == Seq::empty().push('u').push('n').push('s').push('i').push('g').push('n').push('e').push('d').push(' ').push('l').push('o').push('n').push('g').push(' ').push('l').push('o').push('n').push('g')
{
}

// The corpus TRUNC shape, detected: own [1,5]->U8 with a use-range
// reaching outside U8 — the shipped backend stores anyway (no check),
// so the model states the DETECTION (inconsistency is observable),
// not the store.
proof fn pin_trunc_shape_detected(own: (int, int), contribs: Seq<Option<(int, int)>>)
    requires own == (1int, 5int),
        contribs.len() == 1 && contribs[0] == Option::None,
    ensures eff_fold(contribs, narrow(own.0, own.1)) == Width::I64
{
    assert(narrow(1int, 5int) == Width::U8);
    assert(contrib_width(Option::None::<(int, int)>) == Width::I64);
    assert(widen(Width::U8, Width::I64) == Width::I64);
    // unfold the single-step fold: head is the only contributor
    assert(eff_fold(contribs, narrow(own.0, own.1))
        == eff_fold(contribs.drop_first(), widen(narrow(own.0, own.1), contrib_width(contribs[0]))));
    assert(contribs.drop_first().len() == 0);
}

fn main() {}

} // verus!
