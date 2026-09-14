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
    tmp: &mut u32,
    used: &mut BTreeSet<String>,
    out: &mut Vec<String>,
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
        CuArith::Add(a, b) => {
            let l = arith(a, ext_idx, tmp, used, out);
            let r = arith(b, ext_idx, tmp, used, out);
            let d = format!("%qa{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  add.s64 {d}, {l}, {r};"));
            d
        }
        CuArith::Sub(a, b) => {
            let l = arith(a, ext_idx, tmp, used, out);
            let r = arith(b, ext_idx, tmp, used, out);
            let d = format!("%qs{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  sub.s64 {d}, {l}, {r};"));
            d
        }
        CuArith::Mul(a, b) => {
            let l = arith(a, ext_idx, tmp, used, out);
            let r = arith(b, ext_idx, tmp, used, out);
            let d = format!("%qm{tmp}");
            used.insert(d.clone());
            *tmp += 1;
            out.push(format!("  mul.lo.s64 {d}, {l}, {r};"));
            d
        }
        CuArith::Div(a, b) => divmod(a, b, ext_idx, tmp, used, out, true),
        CuArith::Mod(a, b) => divmod(a, b, ext_idx, tmp, used, out, false),
    }
}

/// Division/modulo with a zero guard (yields 0, mirroring the CPU
/// backend). Literal-nonzero divisors skip the branch (bare op).
fn divmod(
    a: &CuArith,
    b: &CuArith,
    ext_idx: &BTreeMap<String, usize>,
    tmp: &mut u32,
    used: &mut BTreeSet<String>,
    out: &mut Vec<String>,
    is_div: bool,
) -> String {
    let l = arith(a, ext_idx, tmp, used, out);
    // Literal divisor: bare when provably nonzero, guarded zero when 0.
    if let CuArith::Num(d) = b {
        let r = arith(b, ext_idx, tmp, used, out);
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
    let r = arith(b, ext_idx, tmp, used, out);
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
            let v = arith(&st.value, &ext_idx, &mut tmp, &mut used, &mut vlines);
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
            let v = arith(&st.value, &ext_idx, &mut tmp, &mut used, &mut vlines);
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

    #[test]
    fn emitter_is_deterministic() {
        assert_eq!(
            shir_to_cu_compute(&squares_dyn()),
            shir_to_cu_compute(&squares_dyn())
        );
    }
}
