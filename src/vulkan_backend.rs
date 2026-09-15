//! Vulkan compute backend — GLSL 450 emitter for autoshaderised loops.
//!
//! Stage ⑥ of docs/BASH-O4.md §4.2: one counted loop shape (affine
//! array stores of i64 arithmetic over the loop counter) renders to a
//! GLSL 450 **compute** shader for SPIR-V compilation (`glslangValidator
//! -V`) and Vulkan dispatch. This is NOT the `glsl_backend` fragment
//! sketch (whole-program WebGL output encoding) — per-loop compute with
//! SSBOs, no `gl_FragColor`, no string table.
//!
//! Correctness rules (the v40/v37 lessons — never silently change width):
//! - every integer is `int64_t` (bash wraps mod 2^64); ALL literals go
//!   through an explicit `int64_t(...)` constructor (GLSL would default
//!   them to i32);
//! - outer variables become host-provided `in_data[]` slots (never baked
//!   guesses); the slot map is printed as comments (the host-split
//!   contract);
//! - out-of-bounds: the shader trusts the host-sized buffers (the future
//!   host split sizes them from this same spec); documented, not hidden.
//! - the function is TOTAL over `VkLoopSpec` (invalid shapes are
//!   unrepresentable — the type has no error case, so there is no
//!   `TODO(unsupported)` marker path).

use std::collections::BTreeMap;

/// i64 arithmetic over {loop counter, host-provided externs, literals}.
#[derive(Debug, Clone, PartialEq)]
pub enum VkArith {
    Num(i64),
    LoopVar,
    ExtVar(String),
    Add(Box<VkArith>, Box<VkArith>),
    Sub(Box<VkArith>, Box<VkArith>),
    Mul(Box<VkArith>, Box<VkArith>),
}

/// One affine array store: `array[a*i + b] = value`.
#[derive(Debug, Clone, PartialEq)]
pub struct VkStore {
    pub array: String,
    pub index_a: i64,
    pub index_b: i64,
    pub value: VkArith,
}

/// One emittable counted loop.
#[derive(Debug, Clone, PartialEq)]
pub struct VkLoopSpec {
    /// Stable id (`sh_loop_<fn>_<n>`, docs/BASH-O4.md §4.3).
    pub id: String,
    /// Loop counter name (for comments; the shader uses `i_`).
    pub var: String,
    /// Inclusive lower bound, exclusive/adjustable upper bound.
    pub lo: i64,
    /// Upper bound value as written (with `hi_inclusive`).
    pub hi: i64,
    /// Whether the upper bound is inclusive (`<=`) or exclusive (`<`).
    pub hi_inclusive: bool,
    /// Positive step.
    pub step: i64,
    /// Exact trip count (checked by the producer).
    pub trips: u64,
    /// Workgroup size (`local_size_x`). 256 is the portable default;
    /// tune per device (see docs/BASH_VULKAN.md measurements).
    pub local_size_x: u32,
    /// Unroll factor: lanes per invocation (1 or 4). 4 quarters the
    /// invocation count with per-element bounds guards (exact when
    /// trips % 4 == 0); measured ~1.3x on lavapipe (docs/BASH_VULKAN.md).
    pub unroll: u32,
    /// Host-provided i64 slots (`in_data[]` order = declaration order).
    pub externs: Vec<String>,
    /// Output stores (one SSBO per distinct array).
    pub stores: Vec<VkStore>,
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

fn num(n: i64) -> String {
    format!("int64_t({n})")
}

fn arith(e: &VkArith, ext_idx: &BTreeMap<String, usize>) -> String {
    match e {
        VkArith::Num(n) => num(*n),
        VkArith::LoopVar => "i_".to_string(),
        VkArith::ExtVar(name) => {
            let i = ext_idx.get(name).copied().unwrap_or(0);
            format!("in_data[{i}]")
        }
        VkArith::Add(a, b) => format!("({} + {})", arith(a, ext_idx), arith(b, ext_idx)),
        VkArith::Sub(a, b) => format!("({} - {})", arith(a, ext_idx), arith(b, ext_idx)),
        VkArith::Mul(a, b) => format!("({} * {})", arith(a, ext_idx), arith(b, ext_idx)),
    }
}

/// Render the loop spec to a GLSL 450 compute shader (total function).
pub fn shir_to_vk_compute(spec: &VkLoopSpec) -> String {
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
    out.push_str("#version 450\n");
    out.push_str("#extension GL_EXT_shader_explicit_arithmetic_types : enable\n");
    out.push_str(&format!("// bash-O4 autoshader: {} ({}..{}{} step {})\n",
        spec.id, spec.lo, if spec.hi_inclusive { "=" } else { "" },
        spec.hi, spec.step));
    out.push_str(&format!("layout(local_size_x = {}) in;\n", spec.local_size_x));
    // Descriptor contract (shared with the host runner): binding 0 is
    // `in_data[]` IFF externs are present; output arrays follow from
    // `in_offset`. No externs ⇒ no InBuf at all (no dummy buffer traffic).
    // Every block is `restrict`: distinct host allocations never alias,
    // and InBuf is read-only.
    let in_offset: usize = if spec.externs.is_empty() { 0 } else { 1 };
    if in_offset == 1 {
        out.push_str("// counter: i_ = lo + t*step; externs via in_data[] (host-provided):\n");
        for (name, i) in &ext_idx {
            out.push_str(&format!("//   in_data[{i}] = {name}\n"));
        }
        out.push_str(
            "layout(set = 0, binding = 0) restrict buffer InBuf { int64_t in_data[]; };\n",
        );
    } else {
        out.push_str("// counter: i_ = lo + t*step; no externs.\n");
    }
    for (bi, arr) in arrays.iter().enumerate() {
        out.push_str(&format!(
            "layout(set = 0, binding = {}) restrict buffer Out_{} {{ int64_t data_{}[]; }};\n",
            in_offset + bi,
            ident(arr),
            ident(arr)
        ));
    }
    out.push_str(&format!("const uint64_t TRIPS = {}u;\n", spec.trips));
    out.push_str(&format!("const int64_t LO = {};\n", num(spec.lo)));
    out.push_str(&format!("const int64_t STEP = {};\n", num(spec.step)));
    out.push_str("void main() {\n");
    out.push_str("  uint64_t t = gl_GlobalInvocationID.x;\n");
    if spec.unroll == 4 {
        // Four lanes per invocation, loop form (measured faster than
        // straightline on lavapipe: the driver unrolls/optimizes the
        // induction itself; hand-unrolled blocks with a shared mutable
        // counter optimized worse). Per-element bounds guards make tails
        // exact for ANY trip count (indices ascend, so a fired guard
        // means all later lanes are out too — plain `return` is correct,
        // no divergence beyond the tail workgroup). Candidacy requests
        // unroll only when trips % 4 == 0, where the guards provably
        // never fire.
        out.push_str("  uint64_t base = t * 4u;\n");
        out.push_str("  if (base >= TRIPS) return;\n");
        out.push_str("  for (uint k = 0u; k < 4u; k++) {\n");
        out.push_str("    uint64_t idx = base + k;\n");
        out.push_str("    if (idx >= TRIPS) return;\n");
        out.push_str(&format!(
            "    int64_t i_ = {};\n",
            counter_expr(spec.lo, spec.step, "idx")
        ));
        for st in &spec.stores {
            let arr = ident(&st.array);
            out.push_str(&format!(
                "    data_{arr}[{}] = {};\n",
                index_expr(st, "idx"),
                arith(&st.value, &ext_idx)
            ));
        }
        out.push_str("  }\n");
    } else {
        out.push_str("  if (t >= TRIPS) return;\n");
        out.push_str(&format!("  int64_t i_ = {};\n", counter_expr(spec.lo, spec.step, "t")));
        for s in &spec.stores {
            let arr = ident(&s.array);
            out.push_str(&format!(
                "  data_{arr}[{}] = {};\n",
                index_expr(s, "t"),
                arith(&s.value, &ext_idx)
            ));
        }
    }
    out.push_str("}\n");
    out
}

/// Counter value for t-expression `t` (peepholed: identity bounds emit
/// no arithmetic — `*1`/`+0` cost two int64 ALU ops per lane).
fn counter_expr(lo: i64, step: i64, t: &str) -> String {
    match (lo == 0, step == 1) {
        (true, true) => format!("int64_t({t})"),
        (false, true) => format!("LO + int64_t({t})"),
        (true, false) => format!("int64_t({t}) * STEP"),
        (false, false) => format!("LO + int64_t({t}) * STEP"),
    }
}

/// Index expression for store `s` (always a 32-bit `uint(...)`: GLSL
/// rejects 64-bit SSBO indices — 64-bit ids must convert explicitly).
///
/// idx = a*i + b. The common case (a=1,b=0) indexes the invocation id
/// directly (coalesced, exact); anything else computes from the `i_`
/// counter assigned just above the store.
fn index_expr(s: &VkStore, t: &str) -> String {
    if s.index_a == 1 && s.index_b == 0 {
        return format!("uint({t})");
    }
    let a = num(s.index_a);
    let b = num(s.index_b);
    format!("uint({a} * i_ + {b})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squares() -> VkLoopSpec {
        VkLoopSpec {
            id: "sh_loop_main_0".to_string(),
            var: "i".to_string(),
            lo: 0,
            hi: 10,
            hi_inclusive: false,
            step: 1,
            trips: 10,
            local_size_x: 256,
            unroll: 1,
            externs: vec![],
            stores: vec![VkStore {
                array: "a".to_string(),
                index_a: 1,
                index_b: 0,
                value: VkArith::Mul(
                    Box::new(VkArith::LoopVar),
                    Box::new(VkArith::LoopVar),
                ),
            }],
        }
    }

    #[test]
    fn shader_has_compute_shape() {
        let g = shir_to_vk_compute(&squares());
        assert!(g.contains("#version 450"), "{g}");
        assert!(g.contains("GL_EXT_shader_explicit_arithmetic_types"), "{g}");
        assert!(g.contains("layout(local_size_x = 256) in;"), "{g}");
        assert!(g.contains("gl_GlobalInvocationID"), "{g}");
        assert!(g.contains("if (t >= TRIPS) return;"), "{g}");
        assert!(!g.contains("gl_FragColor"), "no fragment output: {g}");
    }

    #[test]
    fn literals_are_i64_constructed() {
        let g = shir_to_vk_compute(&squares());
        // No bare i32 literal in arithmetic position: bounds go through
        // int64_t(...). (SSBO indices excepted — `binding = 1` etc.)
        assert!(g.contains("const int64_t LO = int64_t(0);"), "{g}");
        assert!(g.contains("data_a[uint(t)] = (i_ * i_);"), "{g}");
    }

    #[test]
    fn externs_map_to_in_data_slots() {
        let mut s = squares();
        s.externs = vec!["k".to_string()];
        s.stores[0].value = VkArith::Mul(
            Box::new(VkArith::LoopVar),
            Box::new(VkArith::ExtVar("k".to_string())),
        );
        let g = shir_to_vk_compute(&s);
        assert!(g.contains("//   in_data[0] = k"), "{g}");
        assert!(g.contains("data_a[uint(t)] = (i_ * in_data[0]);"), "{g}");
    }

    #[test]
    fn affine_index_computes_from_counter() {
        let mut s = squares();
        s.stores[0].index_b = 2;
        let g = shir_to_vk_compute(&s);
        assert!(g.contains("uint(int64_t(1) * i_ + int64_t(2))"), "{g}");
    }

    #[test]
    fn two_arrays_get_two_bindings() {
        let mut s = squares();
        s.stores.push(VkStore {
            array: "b".to_string(),
            index_a: 1,
            index_b: 0,
            value: VkArith::LoopVar,
        });
        let g = shir_to_vk_compute(&s);
        // No externs: InBuf omitted, outputs start at binding 0.
        assert!(!g.contains("InBuf"), "{g}");
        assert!(g.contains("binding = 0) restrict buffer Out_a"), "{g}");
        assert!(g.contains("binding = 1) restrict buffer Out_b"), "{g}");
    }

    #[test]
    fn peephole_drops_identity_arithmetic() {
        // lo=0, step=1: no `+0`/`*1` dead ALU.
        let g = shir_to_vk_compute(&squares());
        assert!(g.contains("int64_t i_ = int64_t(t);"), "{g}");
        let mut s = squares();
        s.lo = 5;
        let g = shir_to_vk_compute(&s);
        assert!(g.contains("int64_t i_ = LO + int64_t(t);"), "{g}");
        let mut s = squares();
        s.step = 3;
        let g = shir_to_vk_compute(&s);
        assert!(g.contains("int64_t i_ = int64_t(t) * STEP;"), "{g}");
    }

    #[test]
    fn emitter_is_deterministic() {
        assert_eq!(shir_to_vk_compute(&squares()), shir_to_vk_compute(&squares()));
    }
}
