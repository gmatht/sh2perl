//! SH2_PROFILE — Phase-0 profile emission for the C target
//! (docs/PROFILING.md §9.1/§9.2), plus the minimal Phase-2 consumption
//! (`SH2_PROFILE_IN`, §5.1 exact cap pre-sizing).
//!
//! Emission contract (default OFF — the default render is byte-identical
//! to the pre-profiler output; the A/B firewall of §9.2):
//!
//! * `SH2_PROFILE=1` plants probes at four site classes — loop trips,
//!   branch arms, value-magnitude buckets, array lengths — and a
//!   destructor dumps one JSON report (`schema` 1) at exit.
//! * `SH2_PROFILE_OUT=<path>` overrides the report path (default:
//!   `sh2-profile.<pid>.json` beside the binary's CWD).
//! * `SH2_PROFILE_IN=<path>` consumes a report: advisory only, an
//!   observed array `len_max` pre-sizes the (still growable) vec, so a
//!   wrong-or-stale profile costs a `realloc`, never correctness
//!   (§5.1/§5.3 — the growth check stays).
//!
//! Site ids are assigned in RENDER order (post-transform, so a
//! duplicated body gets distinct ids — §9.1), which is deterministic:
//! same source + same flags + same transpiler ⇒ same ids and same
//! byte-identical C.
//!
//! Consumption is a pure function of (program, profile file): no global
//! mutable profiler state, no I/O in the decision path (§9.2).

use std::collections::HashMap;

/// Per-render profile state. `None` outside `SH2_PROFILE=1` renders, so
/// every probe is a no-op and the emitted C never mentions `_prof_`.
#[derive(Debug, Clone, Default)]
pub struct ProfState {
    /// `{scope}:{kind}@{id}` site names, one vector per site class (the
    /// id IS the index — §9.2 "id = array index").
    pub loops: Vec<String>,
    pub branches: Vec<String>,
    pub mags: Vec<String>,
    pub lens: Vec<String>,
    /// staleness key: hash of the normalized program (§9.2)
    pub hash: u64,
    /// report path baked into the binary
    pub out: String,
    /// options actually in force (stamped into the report header)
    pub options: String,
    /// a GMP slot is instrumented (emit the wide `_sh_prof_val_mpz`
    /// probe, which measures past the 64-bit wrap)
    pub gmp: bool,
}

/// The subset of a report the consumption path reads (advisory only).
#[derive(Debug, Clone, Default)]
pub struct ProfileIn {
    /// array -> observed max length (max over every `lens` site naming it)
    pub max_lens: HashMap<String, usize>,
    /// the report's staleness key (None when the report predates hashing)
    pub hash: Option<u64>,
    /// var/array -> max magnitude bucket seen (bytes; 0..16, 16 = >120 bits).
    /// Read from the `mags` sites; drives `SH2_ASSUME_OBSERVED_WIDTHS`.
    pub mag_buckets: HashMap<String, u8>,
    /// `SH2_ASSUME_OBSERVED_WIDTHS` in force (the assertive tier flag,
    /// DUAL_LOOPS §4). Default OFF: the buckets are still parsed but no
    /// width is specialised from them.
    pub assume_widths: bool,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x1000_0000_01b3;

/// FNV-1a 64 — the schema's `fnv1a64` (§4).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The staleness key: a hash of the source it was taken against
/// (normalized so formatting churn in the frontend does not invalidate).
/// The IR's debug form is used because the backend sees an `IrProgram`,
/// not the file text; it is stable for a fixed source+transpiler.
pub fn source_hash(stmts: &str, subs: &str) -> u64 {
    fnv1a64(format!("sh2-profile-v1\n{stmts}\n{subs}").as_bytes())
}

/// `SH2_PROFILE` truthy? (`1`, `true`, `yes`, or any non-empty value.)
pub fn profile_enabled() -> bool {
    match std::env::var("SH2_PROFILE") {
        Ok(v) => !matches!(v.as_str(), "" | "0" | "false" | "no"),
        Err(_) => false,
    }
}

/// `SH2_ASSUME_OBSERVED_WIDTHS` truthy? See `profile_enabled` for the
/// value convention. DUAL_LOOPS §4: compile as if every integer fits
/// the max bucket its profile records, with a guard on the store.
pub fn assume_observed_widths() -> bool {
    match std::env::var("SH2_ASSUME_OBSERVED_WIDTHS") {
        Ok(v) => !matches!(v.as_str(), "" | "0" | "false" | "no"),
        Err(_) => false,
    }
}

/// The inclusive upper bound implied by a magnitude bucket (bytes of
/// unsigned magnitude). Bucket 0 = zero/never observed; 16 is the clamp
/// for anything above 120 bits.
pub fn bucket_hi(b: u8) -> i128 {
    let b = b as u32;
    if b == 0 {
        0
    } else if b >= 16 {
        i128::MAX
    } else {
        (1i128 << (8 * b)) - 1
    }
}

fn env_opt(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "-".to_string())
}

/// The report's `options` header (target + the lowering flags that move
/// site ids/lowering shapes — §4/§5.4).
pub fn options_json() -> String {
    format!(
        "{{\"target\": \"c\", \"SH2_DUAL_LOOPS\": \"{}\", \"SH2_LIST_VEC\": \"{}\", \"SH2_PROFILE_WIDE\": \"{}\"}}",
        env_opt("SH2_DUAL_LOOPS"),
        env_opt("SH2_LIST_VEC"),
        env_opt("SH2_PROFILE_WIDE")
    )
}

/// Load + validate `SH2_PROFILE_IN` (None when unset/unreadable).
/// Schema mismatch is refused outright; a source-hash mismatch is a
/// warning (advisory use stays sound — §5.4).
pub fn load_profile_in() -> Option<ProfileIn> {
    let path = std::env::var("SH2_PROFILE_IN").ok()?;
    if path.is_empty() {
        return None;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("sh2-profile: cannot read {path}: {e} (ignoring profile)");
            return None;
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("sh2-profile: {path} is not valid JSON: {e} (ignoring profile)");
            return None;
        }
    };
    let schema = v.get("schema").and_then(|s| s.as_i64()).unwrap_or(-1);
    if schema != 1 {
        eprintln!("sh2-profile: {path} has schema {schema} (want 1) — refusing");
        return None;
    }
    let mut p = ProfileIn::default();
    p.hash = v
        .get("source_hash")
        .and_then(|h| h.as_str())
        .and_then(|h| u64::from_str_radix(h, 16).ok());
    if let Some(lens) = v.get("lens").and_then(|l| l.as_array()) {
        for site in lens {
            let name = site.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let len = site.get("len_max").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
            if let Some(arr) = lens_site_array(name) {
                let e = p.max_lens.entry(arr).or_insert(0);
                if len > *e {
                    *e = len;
                }
            }
        }
    }
    if let Some(mags) = v.get("mags").and_then(|m| m.as_array()) {
        for site in mags {
            let name = site.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let b = site.get("max_bucket").and_then(|n| n.as_i64()).unwrap_or(0);
            if let Some(var) = mag_site_name(name) {
                let b = b.clamp(0, 17) as u8;
                let e = p.mag_buckets.entry(var).or_insert(0);
                if b > *e {
                    *e = b;
                }
            }
        }
    }
    p.assume_widths = assume_observed_widths();
    Some(p)
}

/// `{scope}:mag:{var}@{id}` -> `{var}` (None for a foreign shape).
fn mag_site_name(name: &str) -> Option<String> {
    let rest = name.split(":mag:").nth(1)?;
    let var = rest.split('@').next()?;
    if var.is_empty() {
        None
    } else {
        Some(var.to_string())
    }
}

/// `{scope}:len:{array}@{id}` -> `{array}` (None for a foreign shape).
fn lens_site_array(name: &str) -> Option<String> {
    let rest = name.split(":len:").nth(1)?;
    let arr = rest.split('@').next()?;
    if arr.is_empty() {
        None
    } else {
        Some(arr.to_string())
    }
}

impl ProfileIn {
    /// The advisory cap for a vec array: the exact observed max (§5.1
    /// "exact `cap` pre-sizing of the `Vec`" — the grow check stays, so
    /// exceeding the profile only costs a `realloc`).
    pub fn cap_for(&self, arr: &str) -> Option<usize> {
        match self.max_lens.get(arr) {
            Some(0) | None => None,
            Some(n) => Some(*n),
        }
    }

    /// The observed magnitude bucket for a var/array (None when the
    /// profile never named it).
    pub fn bucket_for(&self, name: &str) -> Option<u8> {
        self.mag_buckets.get(name).copied()
    }
}

// ── C emission ───────────────────────────────────────────────────────

/// A C string literal (site names are identifiers:/: so this is belt
/// and braces, but a name must never break the emitted C).
pub fn cstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\{:03o}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn carr(list: &[String]) -> String {
    let mut out = String::new();
    for (i, n) in list.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&cstr(n));
    }
    if out.is_empty() {
        out.push_str("\"\"");
    }
    out
}

/// The instrumented C preamble (probe state + helpers + exit dump).
/// Inserted after the render post-passes, right before the include trim,
/// so it is never trimmed as (unused) runtime and its `fprintf`/`FILE`
/// uses keep `<stdio.h>`.
pub fn preamble(st: &ProfState) -> Vec<String> {
    let nl = st.loops.len().max(1);
    let nb = st.branches.len().max(1);
    let nm = st.mags.len().max(1);
    let nn = st.lens.len().max(1);
    let mut v: Vec<String> = Vec::new();
    let mut p = |s: String| v.push(s);
    p("/* ── SH2_PROFILE=1 (docs/PROFILING.md Phase 0): site probes + exit dump ── */".into());
    p(format!("static const uint64_t _prof_hash = 0x{:016x}ULL;", st.hash));
    p(format!("static const char *_prof_out = {};", cstr(&st.out)));
    p(format!("static const char *_prof_options = {};", cstr(&st.options)));
    p(format!(
        "#define _PROF_NL {nl}\n#define _PROF_NB {nb}\n#define _PROF_NM {nm}\n#define _PROF_NN {nn}"
    ));
    p(format!(
        "static const char *_prof_loop_names[] = {{{}}};",
        carr(&st.loops)
    ));
    p(format!(
        "static const char *_prof_branch_names[] = {{{}}};",
        carr(&st.branches)
    ));
    p(format!(
        "static const char *_prof_mag_names[] = {{{}}};",
        carr(&st.mags)
    ));
    p(format!(
        "static const char *_prof_len_names[] = {{{}}};",
        carr(&st.lens)
    ));
    p("static uint64_t _prof_loop_sum[_PROF_NL], _prof_loop_max[_PROF_NL], _prof_loop_exec[_PROF_NL], _prof_loop_cur[_PROF_NL];".into());
    p("static uint32_t _prof_loop_ntrips[_PROF_NL], _prof_loop_trips[_PROF_NL][64];".into());
    p("static uint32_t _prof_branch_exec[_PROF_NB];".into());
    p("static unsigned char _prof_mag[_PROF_NM];".into());
    p("static uint64_t _prof_len_max[_PROF_NN], _prof_len_apps[_PROF_NN];".into());
    // loop probe: cond-position expression (evaluates the cond exactly
    // once, so the measurement cannot perturb what it measures — §9.1).
    p("static void _sh_prof_loop_seal(unsigned i);".into());
    p("static int _sh_prof_loop(unsigned i, int c) {".into());
    p("  if (c) { _prof_loop_cur[i]++; return 1; }".into());
    p("  return (_sh_prof_loop_seal(i), 0);".into());
    p("}".into());
    p("static void _sh_prof_loop_seal(unsigned i) {".into());
    p("  if (_prof_loop_cur[i] > _prof_loop_max[i]) _prof_loop_max[i] = _prof_loop_cur[i];".into());
    p("  _prof_loop_sum[i] += _prof_loop_cur[i];".into());
    p("  if (_prof_loop_ntrips[i] < 64) _prof_loop_trips[i][_prof_loop_ntrips[i]++] = (uint32_t)_prof_loop_cur[i];".into());
    p("  _prof_loop_exec[i]++;".into());
    p("  _prof_loop_cur[i] = 0;".into());
    p("}".into());
    // trailing seal: reached on `break` too (the cond never fails then).
    p("static void _sh_prof_loop_done(unsigned i) {".into());
    p("  if (_prof_loop_cur[i]) _sh_prof_loop_seal(i);".into());
    p("}".into());
    p("static int _sh_prof_branch(unsigned i, int c) { if (c) { _prof_branch_exec[i]++; return 1; } return 0; }".into());
    p("static void _sh_prof_arm(unsigned i) { _prof_branch_exec[i]++; }".into());
    p("static void _sh_prof_val(unsigned i, long long v) {".into());
    p("  uint64_t m = (v < 0) ? (~(uint64_t)v) + 1 : (uint64_t)v;".into());
    p("  unsigned b = 0;".into());
    p("  while (m) { b++; m >>= 8; }".into());
    // 17 buckets: byte 17 is the "past 128 bits" saturation marker
    // (buckets 1..8 = u8..u64, 9..16 = needs __int128, 17 = needs GMP
    // or wider). A narrower clamp would hide exactly the middle tier.
    p("  if (b > 17) b = 17;".into());
    p("  if ((unsigned char)b > _prof_mag[i]) _prof_mag[i] = (unsigned char)b;".into());
    p("}".into());
    if st.gmp {
        // Wide probe: measure a GMP slot's true magnitude even after the
        // release representation would have wrapped — this is what lets
        // the profile distinguish "needs 128 bits" from "needs more".
        p("static void _sh_prof_val_mpz(unsigned i, const mpz_t v) {".into());
        p("  size_t bits = (mpz_sgn(v) == 0) ? 0 : mpz_sizeinbase(v, 2);".into());
        p("  unsigned b = (unsigned)((bits + 7) / 8);".into());
        p("  if (b > 17) b = 17;".into());
        p("  if ((unsigned char)b > _prof_mag[i]) _prof_mag[i] = (unsigned char)b;".into());
        p("}".into());
    }
    p("static void _sh_prof_len(unsigned i, size_t n) {".into());
    p("  if ((uint64_t)n > _prof_len_max[i]) _prof_len_max[i] = (uint64_t)n;".into());
    p("  _prof_len_apps[i]++;".into());
    p("}".into());
    p("static void _sh_prof_dump(void) {".into());
    p("  FILE *f = fopen(_prof_out, \"w\");".into());
    p("  unsigned i, k;".into());
    p("  if (!f) return;".into());
    p("  fprintf(f, \"{\\n \\\"schema\\\": 1,\\n \\\"hash_algo\\\": \\\"fnv1a64\\\",\\n \\\"source_hash\\\": \\\"%016llx\\\",\\n\", (unsigned long long)_prof_hash);".into());
    p("  fprintf(f, \" \\\"options\\\": %s,\\n\", _prof_options);".into());
    p("  fputs(\" \\\"loops\\\": [\", f);".into());
    p("  for (i = 0; i < _PROF_NL; i++) {".into());
    p("    if (i) fputs(\", \", f);".into());
    p("    fprintf(f, \"{\\\"name\\\": \\\"%s\\\", \\\"trips\\\": [\", _prof_loop_names[i]);".into());
    p("    for (k = 0; k < _prof_loop_ntrips[i]; k++) fprintf(f, \"%s%u\", k ? \", \" : \"\", (unsigned)_prof_loop_trips[i][k]);".into());
    p("    fprintf(f, \"], \\\"execs\\\": %llu, \\\"trip_sum\\\": %llu, \\\"trip_max\\\": %llu}\", (unsigned long long)_prof_loop_exec[i], (unsigned long long)_prof_loop_sum[i], (unsigned long long)_prof_loop_max[i]);".into());
    p("  }".into());
    p("  fputs(\"],\\n \\\"branches\\\": [\", f);".into());
    p("  for (i = 0; i < _PROF_NB; i++) fprintf(f, \"%s{\\\"name\\\": \\\"%s\\\", \\\"execs\\\": %llu}\", i ? \", \" : \"\", _prof_branch_names[i], (unsigned long long)_prof_branch_exec[i]);".into());
    p("  fputs(\"],\\n \\\"mags\\\": [\", f);".into());
    p("  for (i = 0; i < _PROF_NM; i++) fprintf(f, \"%s{\\\"name\\\": \\\"%s\\\", \\\"max_bucket\\\": %u}\", i ? \", \" : \"\", _prof_mag_names[i], (unsigned)_prof_mag[i]);".into());
    p("  fputs(\"],\\n \\\"lens\\\": [\", f);".into());
    p("  for (i = 0; i < _PROF_NN; i++) fprintf(f, \"%s{\\\"name\\\": \\\"%s\\\", \\\"len_max\\\": %llu, \\\"appends\\\": %llu}\", i ? \", \" : \"\", _prof_len_names[i], (unsigned long long)_prof_len_max[i], (unsigned long long)_prof_len_apps[i]);".into());
    p("  fputs(\"]\\n}\\n\", f);".into());
    p("  fclose(f);".into());
    p("}".into());
    p("__attribute__((destructor)) static void _sh_prof_dump_at_exit(void) { _sh_prof_dump(); }".into());
    p(String::new());
    v
}

/// Phase-2 block: the advisory pre-sizing of heap vecs observed by a
/// profile. File-scope vecs are allocated by a constructor; function
/// locals are inlined by the renderer at their declaration.
pub fn prealloc_block(items: &[(String, String, usize)]) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut v: Vec<String> = Vec::new();
    v.push("/* ── SH2_PROFILE_IN (docs/PROFILING.md §5.1): observed-max pre-sizing ──".into());
    v.push("   advisory only — the growth check stays, so exceeding the profile".into());
    v.push("   costs a realloc, never correctness. */".into());
    for (id, ty, cap) in items {
        v.push(format!(
            "static void _sh_prealloc_{id}(void) {{ {id}_cap = (size_t){cap}; {id} = ({ty}*)realloc({id}, {id}_cap * sizeof *{id}); if (!{id}) {{ fprintf(stderr, \"sh2c: out of memory\\n\"); exit(127); }} }}"
        ));
    }
    let calls: Vec<String> = items.iter().map(|(id, _, _)| format!("  _sh_prealloc_{id}();")).collect();
    v.push("__attribute__((constructor)) static void _sh_prealloc_init(void) {".into());
    v.extend(calls);
    v.push("}".into());
    v.push(String::new());
    v
}
