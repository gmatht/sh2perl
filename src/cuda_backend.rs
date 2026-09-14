//! CUDA compute backend — PTX emitter for counted map loops.
//!
//! Mirrors `vulkan_backend` (GLSL) for the NVIDIA path (docs/BASH_VULKAN.md
//! §9): one counted loop shape (affine array stores of i64 arithmetic
//! over the loop counter) renders to PTX for driver-API JIT
//! (`cuModuleLoadData`, no nvcc/nvrtc) and dispatch via `cudaffi`.
//!
//! Correctness rules (same lessons as the GLSL emitter, plus PTX ones):
//! - every integer is 64-bit (bash wraps mod 2^64); `.s64` for values,
//!   `.u64` for addresses/counts; literals render full-width;
//! - outer variables become host-provided `.param .s64` slots (never
//!   baked guesses); the slot map prints as comments (host contract);
//! - the dynamic bound is a host `.param` too (trips computed on host);
//! - out-of-bounds: guards `t < trips` per lane (exact for any count);
//! - division/modulo by a possibly-zero divisor renders guarded (yields
//!   0, mirroring the CPU backend — never traps); literal-nonzero
//!   divisors are bare;
//! - WSL-driver parser quirks (bisected, do not "clean up"): entry
//!   header single-line; no user reg shadows a special (`%tid` vs
//!   `%tid.x` — all locals `q`-prefixed); shared state avoided entirely
//!   (map-only v1 needs none); immediates fit their type.
//! - the function is TOTAL over `CuLoopSpec` (invalid shapes are
//!   unrepresentable — no `TODO(unsupported)` path).

use std::collections::{BTreeMap, BTreeSet};

/// i64 arithmetic over {loop counter, host-provided externs, literals}.
/// Division/modulo are C-truncating (`div.s64`/`rem.s64` — same wrap
/// behavior as the CPU backend's signed ops, hence bash-compatible).
#[derive(Debug, Clone, PartialEq)]
pub enum CuArith {
    Num(i64),
    LoopVar,
    ExtVar(String),
    Add(Box<CuArith>, Box<CuArith>),
    Sub(Box<CuArith>, Box<CuArith>),
    Mul(Box<CuArith>, Box<CuArith>),
    Div(Box<CuArith>, Box<CuArith>),
    Mod(Box<CuArith>, Box<CuArith>),
    Min(Box<CuArith>, Box<CuArith>),
    Max(Box<CuArith>, Box<CuArith>),
    And(Box<CuArith>, Box<CuArith>),
    /// Array element read `array[index]` (reduce-over-array values).
    /// Negative indices yield 0 (bash unset-in-arith); upper bounds are
    /// trusted (dense-fill coverage — documented v1 assumption).
    ArrRead { array: String, index: Box<CuArith> },
    /// Lane-local variable (sequential-chain prelude of a seq reduce).
    /// Renders as the `%ql_<name>` register the emitter declares from
    /// the prelude's assign targets (candidacy guarantees domination).
    Lane(String),
    /// Arithmetic right shift by a literal (even-dividend `/2` peephole:
    /// exact for all even s64, positive and negative — truncation and
    /// floor agree on evens).
    Shr(Box<CuArith>, u32),
}

/// One affine array store: `array[a*i + b] = value`.
#[derive(Debug, Clone, PartialEq)]
pub struct CuStore {
    pub array: String,
    pub index_a: i64,
    pub index_b: i64,
    pub value: CuArith,
}

/// One emittable counted loop with a dynamic bound.
#[derive(Debug, Clone, PartialEq)]
pub struct CuLoopSpec {
    /// Stable id (`cu_loop_<fn>_<n>`, mirroring the GLSL convention).
    pub id: String,
    /// Loop counter name (for comments; PTX uses `qi_`).
    pub var: String,
    /// Inclusive lower bound literal (usually 0).
    pub lo: i64,
    /// Upper-bound extern var (host-provided trip bound).
    pub bound_var: String,
    /// Whether the bound is exclusive (`<`, max index n-1) or inclusive
    /// (`<=`, max index n).
    pub bound_lt: bool,
    /// Positive literal step.
    pub step: i64,
    /// Threads per block (256 portable default, mirrors local_size_x).
    pub threads: u32,
    /// Host-provided s64 slots (param order = declaration order).
    /// The bound var is always externs[0] by construction.
    pub externs: Vec<String>,
    /// Output stores (one buffer param per distinct array).
    pub stores: Vec<CuStore>,
    /// Min threshold over maskable pow2-mod sites (None = none).
    /// Candidacy guarantees every pow2-mod site contributed (or refuses
    /// wholesale), so `mask_fast` may mask them all.
    pub mask_thresh: Option<i128>,
    /// Render pow2 `%`/`/` as `&`/shr (set by the vehicle on a clone
    /// when bound <= mask_thresh; never set by candidacy directly).
    pub mask_fast: bool,
}

fn ident(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
        out = format!("v_{out}");
    }
    out
}

/// Render one arithmetic node to PTX, appending instructions to `out`.
/// Returns the result register. Single-use temps (`%qn`/`%qa`/...) with
/// a shared counter (unique across the whole kernel, including labels
/// for guarded div/mod).
fn arith(
    e: &CuArith,
    ext_idx: &BTreeMap<String, usize>,
    arr_idx: &BTreeMap<String, usize>,
    tmp: &mut u32,
    used: &mut BTreeSet<String>,
    out: &mut Vec<String>,
    mask_fast: bool,
) -> String {
    match e {
        CuArith::Num(n) => {
            let r = format!("%qn{tmp}");
            used.insert(r.clone());
            *tmp += 1;
            out.push(format!("  mov.s64 {r}, {n};"));
            r
        }
        CuArith::LoopVar => "%qi_".to_string(),
        CuArith::ExtVar(name) => {
            let i = ext_idx.get(name).copied().unwrap_or(0);
            format!("%qe{i}")
        }
        CuArith::Lane(name) => format!("%ql_{}", ident(name)),
        CuArith::Shr(a, k) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qh{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  shr.s64 {d}, {l}, {k};"));
            d
        }
        CuArith::Add(a, b) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qa{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  add.s64 {d}, {l}, {r};"));
            d
        }
        CuArith::Sub(a, b) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qs{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  sub.s64 {d}, {l}, {r};"));
            d
        }
        CuArith::Mul(a, b) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qm{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  mul.lo.s64 {d}, {l}, {r};"));
            d
        }
        CuArith::Div(a, b) => divmod(a, b, ext_idx, arr_idx, tmp, used, out, true, mask_fast),
        CuArith::Mod(a, b) => divmod(a, b, ext_idx, arr_idx, tmp, used, out, false, mask_fast),
        CuArith::Min(a, b) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qn{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  setp.lt.s64 %qp, {l}, {r};"));
            out.push(format!("  selp.s64 {d}, {l}, {r}, %qp;"));
            d
        }
        CuArith::Max(a, b) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qx{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  setp.gt.s64 %qp, {l}, {r};"));
            out.push(format!("  selp.s64 {d}, {l}, {r}, %qp;"));
            d
        }
        CuArith::And(a, b) => {
            let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let d = format!("%qa{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  and.b64 {d}, {l}, {r};"));
            d
        }
        CuArith::ArrRead { array, index } => {
            // Element load with negative guard (bash unset-in-arith is
            // 0); upper bounds trusted (dense-fill coverage assumption).
            let k = arith(index, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let bi = arr_idx.get(array).copied().unwrap_or(0);
            let d = format!("%qd{tmp}");
            used.insert(d.clone());
            let lab = *tmp;
            *tmp += 1;
            out.push(format!("  setp.lt.s64 %qp, {k}, 0;"));
            out.push(format!("  @%qp bra QARR{lab};"));
            out.push(format!("  cvt.u64.s64 %qt, {k};"));
            out.push("  mul.lo.u64 %qt, %qt, 8;".to_string());
            out.push(format!("  add.u64 %qt, %qb{bi}, %qt;"));
            out.push(format!("  ld.global.s64 {d}, [%qt];"));
            out.push(format!("  bra QARRDONE{lab};"));
            out.push(format!("QARR{lab}:"));
            out.push(format!("  mov.s64 {d}, 0;"));
            out.push(format!("QARRDONE{lab}:"));
            d
        }
    }
}

/// Division/modulo with a zero guard (yields 0, mirroring the CPU
/// backend). Literal-nonzero divisors skip the branch (bare op).
fn divmod(
    a: &CuArith,
    b: &CuArith,
    ext_idx: &BTreeMap<String, usize>,
    arr_idx: &BTreeMap<String, usize>,
    tmp: &mut u32,
    used: &mut BTreeSet<String>,
    out: &mut Vec<String>,
    is_div: bool,
    mask_fast: bool,
) -> String {
    let l = arith(a, ext_idx, arr_idx, tmp, used, out, mask_fast);
    // Mask fast path (loop-versioned flag): pow2 divisor sites were
    // all threshold-covered by candidacy (all-or-nothing contract), so
    // render `& mask` / `>> k` with no div instruction. Non-pow2 falls
    // through to the guarded/bare paths below (unchanged).
    if mask_fast {
        if let CuArith::Num(d) = b {
            if *d > 0 && (*d as u64).is_power_of_two() {
                let dreg = format!("%qz{tmp}");
                used.insert(dreg.clone());
                *tmp += 1;
                if is_div {
                    let k = (*d as u64).trailing_zeros();
                    out.push(format!("  shr.s64 {dreg}, {l}, {k};"));
                } else {
                    out.push(format!("  and.b64 {dreg}, {l}, {};" , *d - 1));
                }
                return dreg;
            }
        }
    }
    // Literal divisor: bare when provably nonzero, guarded zero when 0.
    if let CuArith::Num(d) = b {
        let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
        let op = if is_div { "div" } else { "rem" };
        let dreg = format!("%qz{tmp}");
        used.insert(dreg.clone());
        *tmp += 1;
        if *d != 0 {
            out.push(format!("  {op}.s64 {dreg}, {l}, {r};"));
        } else {
            // `x % 0` / `x / 0`: bash errors (value vanishes); the CPU
            // backend yields 0 — same here, no trapping instruction.
            out.push(format!("  mov.s64 {dreg}, 0;"));
        }
        return dreg;
    }
    let r = arith(b, ext_idx, arr_idx, tmp, used, out, mask_fast);
    let op = if is_div { "div" } else { "rem" };
    let dreg = format!("%qz{tmp}");
    used.insert(dreg.clone());
    let lab = *tmp;
    *tmp += 1;
    out.push(format!("  setp.eq.s64 %qp, {r}, 0;"));
    out.push(format!("  @%qp bra QZD{lab};"));
    out.push(format!("  {op}.s64 {dreg}, {l}, {r};"));
    out.push(format!("  bra QZDONE{lab};"));
    out.push(format!("QZD{lab}:"));
    out.push(format!("  mov.s64 {dreg}, 0;"));
    out.push(format!("QZDONE{lab}:"));
    dreg
}

/// Render the loop spec to PTX (total function).
/// Param contract (shared with the host runner): buffer params first
/// (one `.u64` per distinct array, declaration order), then extern
/// `.s64` slots (declaration order), then `.u64` trips. All on ONE
/// header line (parser rule).
pub fn shir_to_cu_compute(spec: &CuLoopSpec) -> String {
    let mut arrays: Vec<String> = Vec::new();
    for s in &spec.stores {
        if !arrays.contains(&s.array) {
            arrays.push(s.array.clone());
        }
    }
    let ext_idx: BTreeMap<String, usize> = spec
        .externs
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    let arr_idx: BTreeMap<String, usize> = arrays
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();

    let mut out = String::new();
    out.push_str(".version 7.0\n");
    out.push_str(".target sm_70\n");
    out.push_str(".address_size 64\n");
    out.push_str(&format!(
        "// bash-O4 cuda: {} ({}..bound({}) step {})\n",
        spec.id,
        spec.lo,
        if spec.bound_lt { "<" } else { "<=" },
        spec.step
    ));
    let mut params: Vec<String> = Vec::new();
    for arr in &arrays {
        params.push(format!(".param .u64 _{}", ident(arr)));
    }
    for name in &spec.externs {
        params.push(format!(".param .s64 _x{}", ident(name)));
    }
    params.push(".param .u64 _trips".to_string());
    out.push_str(&format!(".visible .entry kern({})\n", params.join(", ")));
    out.push_str("{\n");
    // Render bodies first (collects temp regs), then emit decls.
    let mut tmp: u32 = 0;
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut body: Vec<String> = Vec::new();
    // Counter (peepholed identity for lo=0, step=1).
    if spec.lo == 0 && spec.step == 1 {
        body.push("  cvt.s64.u64 %qi_, %gid;".to_string());
    } else {
        body.push("  cvt.s64.u64 %qi_, %gid;".to_string());
        body.push(format!("  mul.lo.s64 %qg0, %qi_, {};", spec.step));
        body.push(format!("  add.s64 %qg1, %qg0, {};", spec.lo));
        body.push("  mov.b64 %qi_, %qg1;".to_string());
    }
    // Stores (each: index, address, value sequence).
    for st in &spec.stores {
        let bi = arrays.iter().position(|a| a == &st.array).unwrap_or(0);
        // idx (u64): a==1,b==0 → gid directly; else via the counter.
        // (Counter-index forms keep the value in s64 and convert after
        // the negativity guard, so negative indices skip the store —
        // exactly bash's drop-negative behavior.)
        if st.index_a == 1 && st.index_b == 0 {
            let mut vlines: Vec<String> = Vec::new();
            let v = arith(&st.value, &ext_idx, &arr_idx, &mut tmp, &mut used, &mut vlines, spec.mask_fast);
            body.extend(vlines);
            body.push("  mul.lo.u64 %qt, %gid, 8;".to_string());
            body.push(format!("  add.u64 %qa, %qb{bi}, %qt;"));
            body.push(format!("  st.global.s64 [%qa], {v};"));
        } else {
            let mut vlines: Vec<String> = Vec::new();
            let t = format!("%qx{tmp}");
            used.insert(t.clone());
            tmp += 1;
            if st.index_a == 1 {
                vlines.push(format!("  add.s64 {t}, %qi_, {};", st.index_b));
            } else if st.index_b == 0 {
                vlines.push(format!("  mul.lo.s64 {t}, %qi_, {};", st.index_a));
            } else {
                let t2 = format!("%qy{tmp}");
                used.insert(t2.clone());
                tmp += 1;
                vlines.push(format!("  mul.lo.s64 {t2}, %qi_, {};", st.index_a));
                vlines.push(format!("  add.s64 {t}, {t2}, {};", st.index_b));
            }
            let v = arith(&st.value, &ext_idx, &arr_idx, &mut tmp, &mut used, &mut vlines, spec.mask_fast);
            body.extend(vlines);
            let lab = tmp;
            tmp += 1;
            body.push(format!("  setp.lt.s64 %qq, {t}, 0;"));
            body.push(format!("  @%qq bra QSKIP{lab};"));
            body.push(format!("  cvt.u64.s64 %qt, {t};"));
            body.push("  mul.lo.u64 %qt, %qt, 8;".to_string());
            body.push(format!("  add.u64 %qa, %qb{bi}, %qt;"));
            body.push(format!("  st.global.s64 [%qa], {v};"));
            body.push(format!("QSKIP{lab}:"));
        }
    }
    // Decls: base regs + all temps used above.
    out.push_str("  .reg .u64 %qt, %gid, %trips, %qa;\n");
    out.push_str("  .reg .s64 %qg0, %qg1;\n");
    out.push_str("  .reg .u32 %qtid, %qbid, %qntid, %qo;\n");
    out.push_str("  .reg .pred %qp, %qq, %pp;\n");
    out.push_str("  .reg .s64 %qi_;\n");
    for (i, name) in spec.externs.iter().enumerate() {
        out.push_str(&format!("  .reg .s64 %qe{i}; // {name}\n"));
    }
    for (bi, arr) in arrays.iter().enumerate() {
        out.push_str(&format!("  .reg .u64 %qb{bi}; // {}\n", ident(arr)));
    }
    if !used.is_empty() {
        let mut ts: Vec<&String> = used.iter().collect();
        ts.sort();
        out.push_str(&format!(
            "  .reg .s64 {};\n",
            ts.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    // Prologue: load params, global id, bounds guard.
    for (bi, arr) in arrays.iter().enumerate() {
        out.push_str(&format!("  ld.param.u64 %qb{bi}, [_{}];\n", ident(arr)));
    }
    for (i, name) in spec.externs.iter().enumerate() {
        out.push_str(&format!(
            "  ld.param.s64 %qe{i}, [_x{}]; // {}\n",
            ident(name),
            ident(name)
        ));
    }
    out.push_str("  ld.param.u64 %trips, [_trips];\n");
    out.push_str("  mov.u32 %qtid, %tid.x;\n");
    out.push_str("  mov.u32 %qbid, %ctaid.x;\n");
    out.push_str("  mov.u32 %qntid, %ntid.x;\n");
    out.push_str("  mad.lo.u32 %qo, %qbid, %qntid, %qtid;\n");
    out.push_str("  cvt.u64.u32 %gid, %qo;\n");
    out.push_str("  setp.ge.u64 %pp, %gid, %trips;\n");
    out.push_str("  @%pp bra QDONE;\n");
    for l in body {
        out.push_str(&l);
        out.push('\n');
    }
    out.push_str("QDONE:\n");
    out.push_str("  ret;\n");
    out.push_str("}\n");
    out
}

/// Reduction op for block kernels. Add/Mul wrap mod 2^64 (ring-exact
/// vs bash); Min/Max exact; ModAdd/MaskAdd fold per step on-lane with
/// raw-add tree + host finish (exact given fit bounds — candidacy
/// enforces modulus/mask <= 2^32).
#[derive(Debug, Clone, PartialEq)]
pub enum CuReduceOp {
    Add,
    Mul,
    Min,
    Max,
    ModAdd { modulus: i64 },
    MaskAdd { mask: i64 },
}

/// A sequential-chain test (per-lane prelude comparisons). Sides lower
/// through the shared `arith` (LoopVar/lane regs/externs/literals).
/// v1: signed comparisons (sound for the guarded shapes candidacy
/// admits); unsigned/shift specializations are V2 speed opts.
#[derive(Debug, Clone, PartialEq)]
pub enum CuTest {
    Cmp { op: String, lhs: CuArith, rhs: CuArith },
}

/// A sequential-chain statement (per-lane prelude of a seq reduce).
/// Restricted to scalar assigns + flat ifs + whiles over `CuTest`
/// (the collatz-chain shape); anything richer vetoes in candidacy.
#[derive(Debug, Clone, PartialEq)]
pub enum CuSeqStmt {
    Assign { var: String, expr: CuArith },
    If { test: CuTest, then: Vec<CuSeqStmt>, else_: Vec<CuSeqStmt> },
    While { test: CuTest, body: Vec<CuSeqStmt> },
}

/// One emittable scalar reduction over a counted loop.
#[derive(Debug, Clone, PartialEq)]
pub struct CuReduceSpec {
    /// Stable id (`cu_red_<fn>_<n>`).
    pub id: String,
    /// Loop counter name (for comments; PTX derives lane indices).
    pub var: String,
    /// Inclusive lower bound literal (usually 0).
    pub lo: i64,
    /// Upper-bound extern var (host-provided; trips computed on host).
    pub bound_var: String,
    /// Exclusive (`<`) or inclusive (`<=`) bound.
    pub bound_lt: bool,
    /// Positive literal step.
    pub step: i64,
    /// Threads per block (256) and sequential items per lane (1024).
    pub threads: u32,
    pub block_items: u64,
    /// Reduction op.
    pub op: CuReduceOp,
    /// Per-item value (counter arith, array reads, externs, literals).
    pub value: CuArith,
    /// Accumulator literal init (host seeds the finish with it).
    pub acc_init: i64,
    /// Host-provided s64 slots for value externs (declaration order).
    /// (Bound is NOT read in-kernel — trips covers it.)
    pub externs: Vec<String>,
    /// Read-only input arrays (for ArrRead values).
    pub arrays: Vec<String>,
    /// Min threshold over maskable pow2-mod sites (None = none).
    /// Same contract as CuLoopSpec (all-or-nothing coverage).
    pub mask_thresh: Option<i128>,
    /// Render pow2 `%`/`/` as `&`/shr (vehicle clone flag).
    pub mask_fast: bool,
    /// Mask the OUTER accum mod too (default true). When the outer
    /// dividend is unversionable but inner pow2 sites are covered,
    /// candidacy sets this false: inners mask (exact), the outer stays
    /// `rem` (exact) — all-or-nothing per site, never a wrong mask.
    pub mask_outer: bool,
    /// Per-lane sequential prelude (empty = plain reduce). Scalar
    /// assigns + flat ifs/whiles over lane regs; runs after the lane
    /// index is derived, before the value. The value is then typically
    /// `Lane(result)`. Candidacy's lane-privatization proof owns
    /// soundness (private state never crosses lanes).
    pub seq_prelude: Vec<CuSeqStmt>,
}

/// Render a reduction to PTX (total function).
/// Param contract: input array buffers (declaration order), then the
/// partials buffer, then extern `.s64` slots, then `.u64` trips.
/// Fit bounds (exactness): lane acc bounded (|x| <= 2^32 per step for
/// mod/mask with modulus/mask <= 2^32); tree sums 256 lanes (<= 2^40);
/// partials read back raw; host folds with the same op/mod. Add/Mul wrap
/// mod 2^64 identically to bash (ring-exact, no bounds needed); Min/Max
/// exact. Candidacy enforces modulus/mask <= 2^32 (veto otherwise).
/// Lane-reg targets of a prelude (sorted dedup — deterministic decls).
fn seq_lane_targets(prelude: &[CuSeqStmt], out: &mut BTreeSet<String>) {
    for st in prelude {
        match st {
            CuSeqStmt::Assign { var, .. } => {
                out.insert(format!("ql_{}", ident(var)));
            }
            CuSeqStmt::If { then, else_, .. } => {
                seq_lane_targets(then, out);
                seq_lane_targets(else_, out);
            }
            CuSeqStmt::While { body, .. } => seq_lane_targets(body, out),
        }
    }
}

/// Comparison PTX mnemonic (signed, v1).
fn cmp_mnemonic(op: &str) -> Option<&'static str> {
    match op {
        "<" => Some("lt"),
        ">" => Some("gt"),
        "<=" => Some("le"),
        ">=" => Some("ge"),
        "==" => Some("eq"),
        "!=" => Some("ne"),
        _ => None,
    }
}

/// Negated comparison (while-exit branches on test-FALSE).
fn cmp_negated(op: &str) -> Option<&'static str> {
    match op {
        "<" => Some("ge"),
        ">" => Some("le"),
        "<=" => Some("gt"),
        ">=" => Some("lt"),
        "==" => Some("ne"),
        "!=" => Some("eq"),
        _ => None,
    }
}

/// Render one test to `%pp` (sequential reuse: preds are consumed by
/// the immediately following branch — no liveness survives).
#[allow(clippy::too_many_arguments)]
fn seq_test(
    t: &CuTest,
    ext_idx: &BTreeMap<String, usize>,
    arr_idx: &BTreeMap<String, usize>,
    tmp: &mut u32,
    used: &mut BTreeSet<String>,
    out: &mut Vec<String>,
    mask_fast: bool,
    negate: bool,
) {
    match t {
        CuTest::Cmp { op, lhs, rhs } => {
            let l = arith(lhs, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let r = arith(rhs, ext_idx, arr_idx, tmp, used, out, mask_fast);
            let m = if negate { cmp_negated(op) } else { cmp_mnemonic(op) };
            match m {
                Some(mn) => out.push(format!("  setp.{mn}.s64 %pp, {l}, {r};")),
                None => {
                    // Unknown comparison op (candidacy restricts to the
                    // six; defense in depth — trap loudly, never guess).
                    out.push(format!("  // SEQ-REFUSE: unknown cmp {op}"));
                    out.push("  trap;".to_string());
                }
            }
        }
    }
}

/// Render one chain statement (labels from `qlab`, bumped per If/While).
#[allow(clippy::too_many_arguments)]
fn seq_stmt(
    st: &CuSeqStmt,
    ext_idx: &BTreeMap<String, usize>,
    arr_idx: &BTreeMap<String, usize>,
    tmp: &mut u32,
    used: &mut BTreeSet<String>,
    out: &mut Vec<String>,
    mask_fast: bool,
    qlab: &mut u32,
) {
    match st {
        CuSeqStmt::Assign { var, expr } => {
            let r = arith(expr, ext_idx, arr_idx, tmp, used, out, mask_fast);
            out.push(format!("  mov.b64 %ql_{}, {r};", ident(var)));
        }
        CuSeqStmt::If { test, then, else_ } => {
            let n = *qlab;
            *qlab += 1;
            seq_test(test, ext_idx, arr_idx, tmp, used, out, mask_fast, false);
            out.push(format!("  @%pp bra QL{n}T;" ));
            for e in else_ {
                seq_stmt(e, ext_idx, arr_idx, tmp, used, out, mask_fast, qlab);
            }
            out.push(format!("  bra QL{n}D;" ));
            out.push(format!("QL{n}T:" ));
            for t in then {
                seq_stmt(t, ext_idx, arr_idx, tmp, used, out, mask_fast, qlab);
            }
            out.push(format!("QL{n}D:" ));
        }
        CuSeqStmt::While { test, body } => {
            let n = *qlab;
            *qlab += 1;
            out.push(format!("QL{n}H:" ));
            seq_test(test, ext_idx, arr_idx, tmp, used, out, mask_fast, true);
            out.push(format!("  @%pp bra QL{n}D;" ));
            for b in body {
                seq_stmt(b, ext_idx, arr_idx, tmp, used, out, mask_fast, qlab);
            }
            out.push(format!("  bra QL{n}H;" ));
            out.push(format!("QL{n}D:" ));
        }
    }
}

pub fn shir_to_cu_reduce(spec: &CuReduceSpec) -> String {
    let ext_idx: BTreeMap<String, usize> = spec
        .externs
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    let arr_idx: BTreeMap<String, usize> = spec
        .arrays
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();

    // Render value subtree first (collects temps).
    let mut tmp: u32 = 0;
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut vlines: Vec<String> = Vec::new();
    let v = arith(&spec.value, &ext_idx, &arr_idx, &mut tmp, &mut used, &mut vlines, spec.mask_fast);
    // Sequential prelude (lane chain): rendered up front so its temps
    // join the declaration dump; emitted per-item inside KLOOP below.
    let mut plines: Vec<String> = Vec::new();
    let mut qlab: u32 = 0;
    for st in &spec.seq_prelude {
        seq_stmt(st, &ext_idx, &arr_idx, &mut tmp, &mut used, &mut plines, spec.mask_fast, &mut qlab);
    }

    let mut out = String::new();
    out.push_str(".version 7.0\n");
    out.push_str(".target sm_70\n");
    out.push_str(".address_size 64\n");
    out.push_str(&format!(
        "// bash-O4 cuda-reduce: {} ({}..bound({}) step {})\n",
        spec.id,
        spec.lo,
        if spec.bound_lt { "<" } else { "<=" },
        spec.step
    ));
    let mut params: Vec<String> = Vec::new();
    for arr in &spec.arrays {
        params.push(format!(".param .u64 _{}", ident(arr)));
    }
    params.push(".param .u64 _partials".to_string());
    for name in &spec.externs {
        params.push(format!(".param .s64 _x{}", ident(name)));
    }
    params.push(".param .u64 _trips".to_string());
    out.push_str(&format!(".visible .entry kern({})\n", params.join(", ")));
    out.push_str("{\n");
    out.push_str("  .reg .u64 %qt, %gid, %trips, %base, %ii, %kk, %blk;\n");
    out.push_str("  .reg .s64 %qg0, %qg1;\n");
    out.push_str("  .reg .u64 %qacc, %qpart;\n");
    out.push_str("  .reg .u32 %qtid, %qbid, %qntid, %qo, %qx;\n");
    out.push_str("  .reg .pred %qp, %qq, %pp;\n");
    out.push_str("  .reg .s64 %qi_;\n");
    for (i, name) in spec.externs.iter().enumerate() {
        out.push_str(&format!("  .reg .s64 %qe{i}; // {name}\n"));
    }
    for (bi, arr) in spec.arrays.iter().enumerate() {
        out.push_str(&format!("  .reg .u64 %qb{bi}; // {}\n", ident(arr)));
    }
    if !used.is_empty() {
        let mut ts: Vec<&String> = used.iter().collect();
        ts.sort();
        out.push_str(&format!(
            "  .reg .s64 {};\n",
            ts.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    out.push_str("  .shared .align 8 .s64 tile[256];\n");
    // Lane-local registers (prelude assign targets, `%ql_` namespaced —
    // collision-free by construction; candidacy proves domination).
    {
        let mut lanes = BTreeSet::new();
        seq_lane_targets(&spec.seq_prelude, &mut lanes);
        if !lanes.is_empty() {
            let mut ls: Vec<&String> = lanes.iter().collect();
            ls.sort();
            let decl = ls.iter().map(|t| format!("%{t}")).collect::<Vec<_>>().join(", ");
            out.push_str("  .reg .s64 ");
            out.push_str(&decl);
            out.push_str(";\n");
        }
    }
    for (bi, arr) in spec.arrays.iter().enumerate() {
        out.push_str(&format!("  ld.param.u64 %qb{bi}, [_{}];\n", ident(arr)));
    }
    out.push_str("  ld.param.u64 %qpart, [_partials];\n");
    for (i, name) in spec.externs.iter().enumerate() {
        out.push_str(&format!(
            "  ld.param.s64 %qe{i}, [_x{}]; // {}\n",
            ident(name),
            ident(name)
        ));
    }
    out.push_str("  ld.param.u64 %trips, [_trips];\n");
    out.push_str("  mov.u32 %qtid, %tid.x;\n");
    out.push_str("  mov.u32 %qbid, %ctaid.x;\n");
    out.push_str("  mov.u32 %qntid, %ntid.x;\n");
    out.push_str("  mad.lo.u32 %qo, %qbid, %qntid, %qtid;\n");
    out.push_str("  cvt.u64.u32 %gid, %qo;\n");
    out.push_str("  mov.u64 %blk, 1024;\n");
    out.push_str(&format!("  mov.u64 %blk, {};\n", spec.block_items));
    out.push_str("  mul.lo.u64 %base, %gid, %blk;\n");
    match &spec.op {
        CuReduceOp::Add | CuReduceOp::ModAdd { .. } | CuReduceOp::MaskAdd { .. } => {
            out.push_str("  mov.u64 %qacc, 0;\n");
        }
        CuReduceOp::Mul => {
            out.push_str("  mov.u64 %qacc, 1;\n");
        }
        CuReduceOp::Min => {
            out.push_str("  mov.u64 %qacc, 9223372036854775807;\n");
        }
        CuReduceOp::Max => {
            // 2^63 as u64 bits (s64 MIN).
            out.push_str("  mov.u64 %qacc, 9223372036854775808;\n");
        }
    }
    out.push_str("  mov.u64 %kk, 0;\n");
    out.push_str("KLOOP:\n");
    out.push_str("  setp.ge.u64 %pp, %kk, %blk;\n");
    out.push_str("  @%pp bra KDONE;\n");
    out.push_str("  add.u64 %ii, %base, %kk;\n");
    out.push_str("  setp.ge.u64 %qq, %ii, %trips;\n");
    out.push_str("  @%qq bra KNEXT;\n");
    if spec.lo == 0 && spec.step == 1 {
        out.push_str("  cvt.s64.u64 %qi_, %ii;\n");
    } else {
        out.push_str("  cvt.s64.u64 %qi_, %ii;\n");
        out.push_str(&format!("  mul.lo.s64 %qg0, %qi_, {};\n", spec.step));
        out.push_str(&format!("  add.s64 %qg1, %qg0, {};\n", spec.lo));
        out.push_str("  mov.b64 %qi_, %qg1;\n");
    }
    for l in &vlines {
        out.push_str(l);
        out.push('\n');
    }
    // Sequential prelude runs per item (after the lane index, before
    // the value/accumulate — the value typically names a lane reg).
    for l in &plines {
        out.push_str(l);
        out.push('\n');
    }
    match &spec.op {
        CuReduceOp::Add => {
            out.push_str(&format!("  add.u64 %qacc, %qacc, {v};\n"));
        }
        CuReduceOp::Mul => {
            out.push_str(&format!("  mul.lo.u64 %qacc, %qacc, {v};\n"));
        }
        CuReduceOp::Min => {
            out.push_str(&format!("  setp.lt.s64 %pp, {v}, %qacc;\n"));
            out.push_str(&format!("  selp.s64 %qacc, {v}, %qacc, %pp;\n"));
        }
        CuReduceOp::Max => {
            out.push_str(&format!("  setp.gt.s64 %pp, {v}, %qacc;\n"));
            out.push_str(&format!("  selp.s64 %qacc, {v}, %qacc, %pp;\n"));
        }
        CuReduceOp::ModAdd { modulus } => {
            out.push_str(&format!("  add.s64 %qacc, %qacc, {v};\n"));
            // Mask fast path: candidacy versioned the accum site (X
            // threshold + acc entry range), so a pow2 modulus renders
            // as `& mask` with no div instruction.
            if spec.mask_fast && spec.mask_outer && *modulus > 0 && (*modulus as u64).is_power_of_two() {
                out.push_str(&format!("  and.b64 %qacc, %qacc, {};\n", *modulus - 1));
            } else {
                out.push_str(&format!("  rem.s64 %qacc, %qacc, {modulus};\n"));
            }
        }
        CuReduceOp::MaskAdd { mask } => {
            out.push_str(&format!("  add.u64 %qacc, %qacc, {v};\n"));
            out.push_str(&format!("  and.b64 %qacc, %qacc, {mask};\n"));
        }
    }
    out.push_str("KNEXT:\n");
    out.push_str("  add.u64 %kk, %kk, 1;\n");
    out.push_str("  bra KLOOP;\n");
    out.push_str("KDONE:\n");
    out.push_str("  mov.b64 %qi_, %qacc;\n");
    out.push_str("  mul.lo.u32 %qo, %qtid, 8;\n");
    out.push_str("  st.shared.s64 [%qo], %qi_;\n");
    out.push_str("  bar.sync 0;\n");
    out.push_str("  mov.u32 %qntid, 128;\n");
    out.push_str("TLOOP:\n");
    out.push_str("  setp.eq.u32 %pp, %qntid, 0;\n");
    out.push_str("  @%pp bra TDONE;\n");
    out.push_str("  setp.lt.u32 %pp, %qtid, %qntid;\n");
    out.push_str("  @%pp bra TSKIP;\n");
    out.push_str("  bra TCONT;\n");
    out.push_str("TSKIP:\n");
    match &spec.op {
        CuReduceOp::Min => {
            out.push_str("  add.u32 %qo, %qtid, %qntid;\n");
            out.push_str("  mul.lo.u32 %qx, %qo, 8;\n");
            out.push_str("  ld.shared.s64 %qi_, [%qx];\n");
            out.push_str("  mul.lo.u32 %qx, %qtid, 8;\n");
            out.push_str("  ld.shared.s64 %qacc, [%qx];\n");
            out.push_str("  setp.lt.s64 %pp, %qi_, %qacc;\n");
            out.push_str("  selp.s64 %qacc, %qi_, %qacc, %pp;\n");
            out.push_str("  st.shared.s64 [%qx], %qacc;\n");
        }
        CuReduceOp::Max => {
            out.push_str("  add.u32 %qo, %qtid, %qntid;\n");
            out.push_str("  mul.lo.u32 %qx, %qo, 8;\n");
            out.push_str("  ld.shared.s64 %qi_, [%qx];\n");
            out.push_str("  mul.lo.u32 %qx, %qtid, 8;\n");
            out.push_str("  ld.shared.s64 %qacc, [%qx];\n");
            out.push_str("  setp.gt.s64 %pp, %qi_, %qacc;\n");
            out.push_str("  selp.s64 %qacc, %qi_, %qacc, %pp;\n");
            out.push_str("  st.shared.s64 [%qx], %qacc;\n");
        }
        CuReduceOp::Mul => {
            out.push_str("  add.u32 %qo, %qtid, %qntid;\n");
            out.push_str("  mul.lo.u32 %qx, %qo, 8;\n");
            out.push_str("  ld.shared.s64 %qi_, [%qx];\n");
            out.push_str("  mul.lo.u32 %qx, %qtid, 8;\n");
            out.push_str("  ld.shared.s64 %qacc, [%qx];\n");
            out.push_str("  mul.lo.u64 %qacc, %qacc, %qi_;\n");
            out.push_str("  st.shared.s64 [%qx], %qacc;\n");
        }
        _ => {
            out.push_str("  add.u32 %qo, %qtid, %qntid;\n");
            out.push_str("  mul.lo.u32 %qx, %qo, 8;\n");
            out.push_str("  ld.shared.s64 %qi_, [%qx];\n");
            out.push_str("  mul.lo.u32 %qx, %qtid, 8;\n");
            out.push_str("  ld.shared.s64 %qacc, [%qx];\n");
            out.push_str("  add.u64 %qacc, %qacc, %qi_;\n");
            out.push_str("  st.shared.s64 [%qx], %qacc;\n");
        }
    }
    out.push_str("TCONT:\n");
    out.push_str("  bar.sync 0;\n");
    out.push_str("  shr.u32 %qntid, %qntid, 1;\n");
    out.push_str("  bra TLOOP;\n");
    out.push_str("TDONE:\n");
    out.push_str("  setp.eq.u32 %pp, %qtid, 0;\n");
    out.push_str("  @%pp bra PSTALL;\n");
    out.push_str("  bra PDONE;\n");
    out.push_str("PSTALL:\n");
    out.push_str("  mov.u32 %qo, 0;\n");
    out.push_str("  ld.shared.s64 %qacc, [%qo];\n");
    out.push_str("  cvt.u64.u32 %qt, %qbid;\n");
    out.push_str("  mul.lo.u64 %qt, %qt, 8;\n");
    out.push_str("  add.u64 %qt, %qpart, %qt;\n");
    out.push_str("  st.global.s64 [%qt], %qacc;\n");
    out.push_str("PDONE:\n");
    out.push_str("  ret;\n");
    out.push_str("}\n");
    out
}

/// Registers referenced but never declared (assembler rejects with an
/// opaque 218). Structural guard: every `%q...`/special used must be
/// declared (or a known special/param-derived name).
#[cfg(test)]
fn undeclared_regs(ptx: &str) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    for line in ptx.lines() {
        let t = line.trim();
        // declaration lines: `.reg ... %a, %b;`
        if let Some(rest) = t.strip_prefix(".reg ") {
            for tok in rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '%')) {
                if let Some(name) = tok.strip_prefix('%') {
                    if !name.is_empty() {
                        declared.insert(name.to_string());
                    }
                }
            }
            continue;
        }
        // label definitions (`FOO:`) are not regs; skip pure-label lines.
        if t.ends_with(':') && !t.contains(' ') {
            continue;
        }
        // uses: %name tokens (skip specials resolved by the driver:
        // tid/ctaid/ntid/nctaid/laneid + param refs [_x]).
        for tok in t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '%' || c == '.')) {
            if let Some(name) = tok.strip_prefix('%') {
                if matches!(name, "tid" | "ctaid" | "ntid" | "nctaid" | "laneid" | "clock" | "clock64" | "gridid" | "smid" | "nsmid" | "warpid" | "nwarpid" | "laneid" | "envreg0") {
                    continue;
                }
                // dotted specials (%tid.x) split at '.' — head already handled
                let base = name.split('.').next().unwrap_or(name);
                if ["tid", "ctaid", "ntid", "nctaid", "laneid"].contains(&base) {
                    continue;
                }
                if !base.is_empty() {
                    used.insert(base.to_string());
                }
            }
        }
    }
    used.into_iter().filter(|u| !declared.contains(u)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squares_dyn() -> CuLoopSpec {
        CuLoopSpec {
            id: "cu_loop_main_0".to_string(),
            var: "i".to_string(),
            lo: 0,
            bound_var: "n".to_string(),
            bound_lt: true,
            step: 1,
            threads: 256,
            externs: vec!["n".to_string()],
            stores: vec![CuStore {
                array: "a".to_string(),
                index_a: 1,
                index_b: 0,
                value: CuArith::Mul(
                    Box::new(CuArith::LoopVar),
                    Box::new(CuArith::LoopVar),
                ),
            }],
            mask_thresh: None,
            mask_fast: false,
        }
    }

    #[test]
    fn kernel_has_compute_shape() {
        let p = shir_to_cu_compute(&squares_dyn());
        assert!(p.contains(".version 7.0"), "{p}");
        assert!(p.contains(".target sm_70"), "{p}");
        assert!(p.contains(".visible .entry kern("), "{p}");
        assert!(p.contains("%tid.x"), "{p}");
        assert!(p.contains("QDONE:"), "{p}");
        assert!(!p.contains("gl_FragColor"), "{p}");
    }

    #[test]
    fn params_follow_buffer_extern_trips_contract() {
        let p = shir_to_cu_compute(&squares_dyn());
        // single-line header: buffers, externs, trips (parser rule).
        assert!(
            p.contains(".visible .entry kern(.param .u64 _a, .param .s64 _xn, .param .u64 _trips)"),
            "{p}"
        );
        assert!(p.contains("// n"), "{p}");
    }

    #[test]
    fn literals_are_full_width() {
        let p = shir_to_cu_compute(&squares_dyn());
        // mul is s64 with no narrowing anywhere in the value tree.
        assert!(p.contains("mul.lo.s64"), "{p}");
        assert!(!p.contains(".u32 %qi_"), "{p}");
    }

    #[test]
    fn counter_peephole_identity() {
        // lo=0, step=1: counter is the gid (no dead LO/STEP arithmetic).
        let p = shir_to_cu_compute(&squares_dyn());
        assert!(p.contains("cvt.s64.u64 %qi_, %gid;"), "{p}");
        let mut s = squares_dyn();
        s.lo = 5;
        s.step = 3;
        let p = shir_to_cu_compute(&s);
        assert!(p.contains("mul.lo.s64"), "{p}");
        assert!(p.contains(", 3;"), "{p}");
        assert!(p.contains(", 5;"), "{p}");
    }

    #[test]
    fn affine_index_computes_from_counter() {
        let mut s = squares_dyn();
        s.stores[0].index_b = 2;
        let p = shir_to_cu_compute(&s);
        assert!(p.contains("add.s64"), "{p}");
        assert!(p.contains(", 2;"), "{p}");
    }

    #[test]
    fn div_mod_lower_to_signed_ops() {
        let mut s = squares_dyn();
        s.stores[0].value = CuArith::Mod(
            Box::new(CuArith::LoopVar),
            Box::new(CuArith::Num(251)),
        );
        let p = shir_to_cu_compute(&s);
        assert!(p.contains("rem.s64"), "{p}");
        let mut s = squares_dyn();
        s.stores[0].value = CuArith::Div(
            Box::new(CuArith::LoopVar),
            Box::new(CuArith::Num(2)),
        );
        let p = shir_to_cu_compute(&s);
        assert!(p.contains("div.s64"), "{p}");
    }

    #[test]
    fn dynamic_divisor_renders_guarded() {
        // `x % y` (y unknown): zero guard yielding 0, never a trap.
        let mut s = squares_dyn();
        s.stores[0].value = CuArith::Mod(
            Box::new(CuArith::LoopVar),
            Box::new(CuArith::ExtVar("y".to_string())),
        );
        // (y must be declared: add to externs for a well-formed spec)
        s.externs.push("y".to_string());
        let p = shir_to_cu_compute(&s);
        assert!(p.contains("QZD"), "{p}");
        assert!(p.contains("mov.s64"), "{p}");
    }

    #[test]
    fn guard_predicates_declared() {
        // Every predicate the emitter can reference must be declared
        // (an undeclared %pp broke JIT with no useful message — the
        // WSL parser just says 218).
        let p = shir_to_cu_compute(&squares_dyn());
        assert!(p.contains(".reg .pred %qp, %qq, %pp;"), "{p}");
    }

    // Reduction emitter tests.
    fn sumred_spec() -> CuReduceSpec {
        CuReduceSpec {
            id: "cu_red_main_0".to_string(),
            var: "i".to_string(),
            lo: 0,
            bound_var: "n".to_string(),
            bound_lt: true,
            step: 1,
            threads: 256,
            block_items: 1024,
            op: CuReduceOp::ModAdd { modulus: 4294967296 },
            value: CuArith::Mod(
                Box::new(CuArith::Mul(
                    Box::new(CuArith::LoopVar),
                    Box::new(CuArith::LoopVar),
                )),
                Box::new(CuArith::Num(4294967296)),
            ),
            acc_init: 0,
            externs: vec![],
            arrays: vec![],
            mask_thresh: None,
            mask_fast: false,
            mask_outer: true,
            seq_prelude: vec![],
        }
    }

    #[test]
    fn reduce_has_block_shape() {
        let p = shir_to_cu_reduce(&sumred_spec());
        assert!(p.contains(".version 7.0"), "{p}");
        assert!(p.contains(".shared .align 8 .s64 tile[256];"), "{p}");
        assert!(p.contains("bar.sync 0;"), "{p}");
        // partials param present (after zero arrays, before externs/trips)
        assert!(
            p.contains(".param .u64 _partials, .param .u64 _trips"),
            "{p}"
        );
        assert!(p.contains("rem.s64"), "{p}");
    }

    #[test]
    fn reduce_lane_identity_per_op() {
        let mut s = sumred_spec();
        s.op = CuReduceOp::Add;
        assert!(shir_to_cu_reduce(&s).contains("mov.u64 %qacc, 0;"));
        s.op = CuReduceOp::Mul;
        assert!(shir_to_cu_reduce(&s).contains("mov.u64 %qacc, 1;"));
        s.op = CuReduceOp::Min;
        assert!(shir_to_cu_reduce(&s).contains("9223372036854775807;"));
    }

    #[test]
    fn mask_fast_renders_and_not_rem() {
        let mut s = sumred_spec();
        s.mask_fast = true;
        let p = shir_to_cu_reduce(&s);
        assert!(p.contains("and.b64"), "{p}");
        assert!(!p.contains("rem.s64"), "signed rem survives mask_fast:\n{p}");
    }

    #[test]
    fn seq_prelude_renders_chain_and_declares_lanes() {
        // Collatz-shaped seq reduce: seed + while + Lane(s) value.
        let seq = CuReduceSpec {
            id: "cu_seq_main_0".to_string(),
            var: "k".to_string(),
            lo: 0,
            bound_var: "n".to_string(),
            bound_lt: true,
            step: 1,
            threads: 256,
            block_items: 1024,
            op: CuReduceOp::Add,
            value: CuArith::Lane("s".to_string()),
            acc_init: 0,
            externs: vec!["n".to_string()],
            arrays: vec![],
            mask_thresh: None,
            mask_fast: false,
            mask_outer: true,
            seq_prelude: vec![
                CuSeqStmt::Assign {
                    var: "v".to_string(),
                    expr: CuArith::Mod(
                        Box::new(CuArith::Add(
                            Box::new(CuArith::Mul(
                                Box::new(CuArith::LoopVar),
                                Box::new(CuArith::Num(37)),
                            )),
                            Box::new(CuArith::Num(3)),
                        )),
                        Box::new(CuArith::Num(251)),
                    ),
                },
                CuSeqStmt::Assign { var: "s".to_string(), expr: CuArith::Num(0) },
                CuSeqStmt::While {
                    test: CuTest::Cmp {
                        op: ">".to_string(),
                        lhs: CuArith::Lane("v".to_string()),
                        rhs: CuArith::Num(1),
                    },
                    body: vec![
                        CuSeqStmt::If {
                            test: CuTest::Cmp {
                                op: "==".to_string(),
                                lhs: CuArith::Mod(
                                    Box::new(CuArith::Lane("v".to_string())),
                                    Box::new(CuArith::Num(2)),
                                ),
                                rhs: CuArith::Num(0),
                            },
                            then: vec![CuSeqStmt::Assign {
                                var: "v".to_string(),
                                expr: CuArith::Div(
                                    Box::new(CuArith::Lane("v".to_string())),
                                    Box::new(CuArith::Num(2)),
                                ),
                            }],
                            else_: vec![CuSeqStmt::Assign {
                                var: "v".to_string(),
                                expr: CuArith::Add(
                                    Box::new(CuArith::Mul(
                                        Box::new(CuArith::Num(3)),
                                        Box::new(CuArith::Lane("v".to_string())),
                                    )),
                                    Box::new(CuArith::Num(1)),
                                ),
                            }],
                        },
                        CuSeqStmt::Assign {
                            var: "s".to_string(),
                            expr: CuArith::Add(
                                Box::new(CuArith::Lane("s".to_string())),
                                Box::new(CuArith::Num(1)),
                            ),
                        },
                    ],
                },
            ],
        };
        let p = shir_to_cu_reduce(&seq);
        // Chain structure: while + if labels, lane regs, signed div/mod.
        assert!(p.contains("QL0H:"), "{p}");
        assert!(p.contains("QL1T:"), "{p}");
        assert!(p.contains("%ql_v") && p.contains("%ql_s"), "{p}");
        assert!(p.contains("rem.s64"), "v1 signed seed mod: {p}");
        assert!(p.contains("div.s64"), "v1 signed halve: {p}");
        assert!(undeclared_regs(&p).is_empty(), "{:?}", undeclared_regs(&p));
    }

    #[test]
    fn mask_outer_false_keeps_outer_rem() {
        // Inners mask, outer stays `rem` (unversionable-outer contract).
        let mut s = sumred_spec();
        s.mask_fast = true;
        s.mask_outer = false;
        let p = shir_to_cu_reduce(&s);
        assert!(p.contains("and.b64"), "inners mask: {p}");
        assert!(p.contains("rem.s64"), "outer stays rem: {p}");
        assert!(undeclared_regs(&p).is_empty(), "{:?}", undeclared_regs(&p));
    }

    #[test]
    fn mask_fast_off_renders_rem() {
        let p = shir_to_cu_reduce(&sumred_spec());
        assert!(p.contains("rem.s64"), "{p}");
        assert!(!p.contains("and.b64"), "{p}");
    }

    #[test]
    fn reduce_all_regs_declared() {
        assert_eq!(
            undeclared_regs(&shir_to_cu_reduce(&sumred_spec())),
            Vec::<String>::new()
        );
    }

    #[test]
    fn reduce_array_read_guards_negative() {
        // `a[i]` reads: negative index yields 0 (bash unset-in-arith).
        let mut s = sumred_spec();
        s.arrays = vec!["a".to_string()];
        s.value = CuArith::ArrRead {
            array: "a".to_string(),
            index: Box::new(CuArith::LoopVar),
        };
        s.op = CuReduceOp::MaskAdd { mask: 4294967295 };
        let p = shir_to_cu_reduce(&s);
        assert!(p.contains("ld.global.s64"), "{p}");
        assert!(p.contains("QARR"), "{p}");
        assert!(p.contains(".param .u64 _a, .param .u64 _partials"), "{p}");
    }

    #[test]
    fn all_regs_declared_map() {
        // Structural JIT guard: undeclared regs fail on-device with an
        // opaque error (twice bitten: %pp). This catches them statically.
        let p = shir_to_cu_compute(&squares_dyn());
        assert_eq!(undeclared_regs(&p), Vec::<String>::new(), "{p}");
    }

    #[test]
    fn emitter_is_deterministic() {
        assert_eq!(
            shir_to_cu_compute(&squares_dyn()),
            shir_to_cu_compute(&squares_dyn())
        );
    }
}
