//! f32-narrow — prove which float-domain variables are EXACT in
//! binary32 (single precision), so a renderer may declare them `float`
//! (Cython `cdef float`, JS `Math.fround`, C `float`) instead of the
//! conservative double.
//!
//! ## Need
//! The Python frontend marks every float-domain variable `Float(64)` in
//! `var_types` (a double is always sound). But many float values are
//! exactly representable in binary32 — `1.5`, `3.0`, a copied exact
//! value, `x + 0.0` — and a renderer that can PROVE it may home the
//! variable in 32 bits. That provability is a whole-program property —
//! a shIR verdict — not a per-frontend guess:
//!   - the Cython renderer emits `cdef float` for proven vars;
//!   - the ESTree renderer reuses its `Float(32) → Math.fround` lowering;
//!   - every other backend keeps the conservative double.
//!
//! ## The verdict
//! `is_f32_var(name)` is true when:
//!   - the variable is float-domain (EVERY definition is float-valued:
//!     a float literal, a copy of a float var, float arithmetic — the
//!     `x = 1; x = 1.5` shape is NEITHER, like autocython's float rule),
//!   - EVERY value ever assigned to it is finite and exactly
//!     representable in binary32 (`(v as f32) as f64 == v`), AND
//!   - every arithmetic op that could execute in f32 is
//!     exactness-preserving (the whole-program condition below).
//!
//! The whole-program condition (C-strictest promotion): an op executes
//! in f32 when every operand converts to float32 or narrower — i.e. no
//! operand is proven-double (a non-candidate float var, which the
//! renderer keeps double) or unknown-domain (never narrowed, so double
//! or wider). Int operands and int/f32-exact literals CONVERT, so they
//! never save an op. An at-risk op that is not exactness-preserving
//! (`+`/`-`/`*`/`/` in general, always `%`/`**`/`sqrt`) drops its
//! candidate operands; removal is monotone, so the fixpoint terminates
//! with no at-risk inexact op left.
//!
//! Exactness-preserving forms (all operands exact): copies, unary `-`,
//! `+0.0`/`-0.0`, `*±1.0`, `*±0.0`, `/±1.0`, and literal-only folds
//! evaluated in f64 (`%`/`//` fold with Python floor semantics) whose
//! result round-trips through f32. Everything else refuses — including
//! the `4097.0 * 4097.0` hazard (folds to 16785409, which f32 rounds to
//! 16785408). Observations (comparisons, print/echo, `int()`/`trunc`,
//! string contexts, calls) never drop: an exact value promotes exactly.
//!
//! ## Scope
//! Analysis-only (like `i32-provable` / `sync-ok-loops`): one
//! `Mutex<Option<HashSet<String>>>` keyed by VARIABLE NAME (the
//! `escape-classes` pattern — names are the stable cross-scope key;
//! same-name vars in different scopes merge conservatively), read by
//! renderers via `is_f32_var`. No structural mutation.
//!
//! ## Placement
//! Registered in `transforms.rs` (`f32-narrow`, SH2_TRANSFORMS gated).
//! The reader consults the same gate, so the bisect machinery disables
//! the verdict end-to-end. Runs on frontend A1 ingress too
//! (`transforms::apply` is on the ingress channel), where the Python
//! float shapes (`arith("…")` strings, structured `Arith` over float
//! vars) actually occur — shell A1 has no float-domain defs, so the
//! verdict set stays empty there and the corpus gate cannot move.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Vars proven binary32-exact (one compilation per process; reset by
/// every `transform` call so verdicts never leak across compilations).
static F32_VARS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Renderer hook: may `name` be homed in 32 bits? False unless the
/// `f32-narrow` transform ran and proved it (bisect-safe: disabling the
/// transform end-to-end also disables the verdict).
pub fn is_f32_var(name: &str) -> bool {
    if !crate::transforms::transform_enabled("f32-narrow") {
        return false;
    }
    F32_VARS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false)
}

/// Apply the transform (analysis-only — computes + caches the verdicts).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    *F32_VARS.lock().unwrap() = Some(analyze(stmts));
    false // analysis-only
}

// ── value lattice ────────────────────────────────────────────────────

/// Provenance domain of a definition or subexpression. `Int` operands
/// CONVERT in an f32 op (never save it); `Unknown` operands stay double
/// or wider (always save it); only `Float` operands participate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dom {
    Float,
    Int,
    Unknown,
}

/// One lattice value: the domain, whether a float value is proven
/// binary32-exact (meaningful only for `Dom::Float`), and the constant
/// fold (present only for literal-only subexpressions).
#[derive(Clone)]
struct FVal {
    dom: Dom,
    exact: bool,
    cval: Option<f64>,
}

impl FVal {
    fn unknown() -> Self {
        FVal {
            dom: Dom::Unknown,
            exact: false,
            cval: None,
        }
    }
    fn int_val() -> Self {
        FVal {
            dom: Dom::Int,
            exact: false,
            cval: None,
        }
    }
}

/// Binary32-exactness of one concrete double: finite and unchanged by
/// the f32 round-trip. Non-finite (`inf`, `NaN` from `1e999`-style
/// literals or overflow folds) is never exact — refuse.
fn f32_exact(v: f64) -> bool {
    v.is_finite() && (v as f32) as f64 == v
}

/// Python floor-mod for constant folding (`%` in the A1 is Python
/// semantics — sign of the divisor — not C `fmod`). `None` = unusable
/// (zero divisor or non-finite side): the fold refuses.
fn py_fmod(a: f64, b: f64) -> Option<f64> {
    if b == 0.0 || !a.is_finite() || !b.is_finite() {
        return None;
    }
    let r = a - b * (a / b).floor();
    Some(if r == 0.0 { r.copysign(b) } else { r })
}

/// Constant-fold one float binop in f64 (JS/C doubles agree on these
/// for finite inputs). `None` = refuse (unsupported op, zero divisor,
/// non-finite side, or a non-finite result).
fn fold(op: &str, a: f64, b: f64) -> Option<f64> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let r = match op {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => {
            if b == 0.0 {
                return None;
            }
            a / b
        }
        "%" => py_fmod(a, b)?,
        "//" => {
            if b == 0.0 {
                return None;
            }
            (a / b).floor()
        }
        _ => return None, // `**` and friends: refuse, even for literals
    };
    if r.is_finite() {
        Some(r)
    } else {
        None
    }
}

/// Exactness of one operand *in a float op*: a Float operand must be
/// proven exact; an Int operand converts, so it is exact exactly when
/// its (constant) value survives f32 — small literals do, `16777217`
/// does not, and unknown int expressions refuse. Unknown never is.
fn operand_exact(v: &FVal) -> bool {
    match v.dom {
        Dom::Float => v.exact,
        Dom::Int => v.cval.map(f32_exact).unwrap_or(false),
        Dom::Unknown => false,
    }
}

fn is_zero(v: &FVal) -> bool {
    matches!(v.cval, Some(x) if x == 0.0)
}

fn is_one(v: &FVal) -> bool {
    matches!(v.cval, Some(x) if x == 1.0 || x == -1.0)
}

fn is_one_or_zero(v: &FVal) -> bool {
    is_zero(v) || is_one(v)
}

/// Combine two operand lattices through one binop. Domain first (Float
/// wins; `Int`/`Int` stays `Int` — NOTE: `/` of two ints is `Int` here
/// even though Python numeric `/` is float: shell `$((7/2))` is integer
/// division with the same A1 shape, so the shared pass stays neutral
/// and the Python-aware renderer may narrow `7/2 = 3.0` on its own).
/// Exactness: literal-only folds check the f32 round-trip of the folded
/// value; otherwise only the exactness-preserving forms (`+0`/`-0`,
/// `*±1`, `*±0`, `/±1`) with exact operands.
fn combine(op: &str, l: &FVal, r: &FVal) -> FVal {
    let dom = if l.dom == Dom::Float || r.dom == Dom::Float {
        Dom::Float
    } else if l.dom == Dom::Unknown || r.dom == Dom::Unknown {
        Dom::Unknown
    } else {
        Dom::Int
    };
    if dom != Dom::Float {
        return FVal {
            dom,
            exact: false,
            cval: None,
        };
    }
    if let (Some(x), Some(y)) = (l.cval, r.cval) {
        return match fold(op, x, y) {
            Some(v) => FVal {
                dom: Dom::Float,
                exact: f32_exact(v),
                cval: Some(v),
            },
            None => FVal {
                dom: Dom::Float,
                exact: false,
                cval: None,
            },
        };
    }
    let exact = operand_exact(l)
        && operand_exact(r)
        && match op {
            "+" => is_zero(l) || is_zero(r),
            "-" => is_zero(r),
            "*" => is_one_or_zero(l) || is_one_or_zero(r),
            "/" => is_one(r),
            _ => false, // `%`, `//`, `**` with operands: refuse
        };
    FVal {
        dom: Dom::Float,
        exact,
        cval: None,
    }
}

// ── analysis environment ─────────────────────────────────────────────

/// The current fixpoint knowledge: float-domain vars, int-domain vars,
/// and the surviving exactness candidates.
struct Env<'a> {
    float_vars: &'a HashSet<String>,
    int_vars: &'a HashSet<String>,
    cand: &'a HashSet<String>,
}

fn var_fval(w: &str, env: &Env) -> FVal {
    if env.cand.contains(w) {
        FVal {
            dom: Dom::Float,
            exact: true,
            cval: None,
        }
    } else if env.float_vars.contains(w) {
        FVal {
            dom: Dom::Float,
            exact: false,
            cval: None,
        }
    } else if env.int_vars.contains(w) {
        FVal::int_val()
    } else {
        FVal::unknown()
    }
}

// ── use tracking (the at-risk rule) ──────────────────────────────────

/// Float-domain variable reads under one op node, plus whether an
/// unknown-domain read occurs (which forces f64 and saves the op).
/// Int reads and int/exact literals convert and never save it.
#[derive(Default)]
struct UseInfo {
    fvars: Vec<String>,
    unknown: bool,
}

impl UseInfo {
    fn merge(&mut self, o: UseInfo) {
        self.fvars.extend(o.fvars);
        self.unknown = self.unknown || o.unknown;
    }
    fn var(w: &str, env: &Env) -> Self {
        if env.float_vars.contains(w) {
            UseInfo {
                fvars: vec![w.to_string()],
                unknown: false,
            }
        } else if env.int_vars.contains(w) {
            UseInfo::default()
        } else {
            UseInfo {
                fvars: vec![],
                unknown: true,
            }
        }
    }
}

/// Drop-check one op node: an at-risk (all-float-operands candidates,
/// no unknown reads) inexact float op drops its candidate operands.
fn check_node(v: &FVal, u: &UseInfo, env: &Env, drops: &mut Vec<String>) {
    if v.dom == Dom::Float && !v.exact && !u.unknown && u.fvars.iter().all(|w| env.cand.contains(w))
    {
        drops.extend(u.fvars.iter().cloned());
    }
}

// ── structured Arith evaluation ──────────────────────────────────────

fn eval_tree(t: &ArithAst, env: &Env) -> FVal {
    match t {
        ArithAst::Num(n) => FVal {
            dom: Dom::Int,
            exact: false,
            cval: Some(*n as f64),
        },
        ArithAst::Var(w) | ArithAst::Ident(w) => var_fval(w, env),
        // An element read is unknown-domain (element types are the
        // list's business — see the renderer-obligation invariant).
        ArithAst::Index { .. } => FVal::unknown(),
        ArithAst::Bin { op, lhs, rhs } => {
            combine(op.as_str(), &eval_tree(lhs, env), &eval_tree(rhs, env))
        }
        ArithAst::Un { op, arg } => {
            if op.as_str() == "-" {
                let a = eval_tree(arg, env);
                FVal {
                    exact: a.exact && a.dom == Dom::Float,
                    cval: a.cval.map(|x| -x),
                    dom: a.dom,
                }
            } else {
                FVal::unknown()
            }
        }
        ArithAst::Cond { then, else_, .. } => join(&eval_tree(then, env), &eval_tree(else_, env)),
        // Casts, sizeof, inline assigns: opaque — refuse.
        _ => FVal::unknown(),
    }
}

/// Join two branch lattices (ternary/defined-or): float only when BOTH
/// sides are float (a value that is sometimes int is not float-domain).
fn join(a: &FVal, b: &FVal) -> FVal {
    match (a.dom, b.dom) {
        (Dom::Float, Dom::Float) => FVal {
            dom: Dom::Float,
            exact: a.exact && b.exact,
            cval: None,
        },
        (Dom::Int, Dom::Int) => FVal::int_val(),
        _ => FVal::unknown(),
    }
}

fn uses_tree(t: &ArithAst, env: &Env) -> UseInfo {
    match t {
        ArithAst::Num(_) => UseInfo::default(),
        ArithAst::Var(w) | ArithAst::Ident(w) => UseInfo::var(w, env),
        ArithAst::Index { key, .. } => {
            let mut u = UseInfo {
                fvars: vec![],
                unknown: true,
            };
            u.merge(uses_tree(key, env));
            u
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            let mut u = uses_tree(lhs, env);
            u.merge(uses_tree(rhs, env));
            u
        }
        ArithAst::Un { arg, .. } => uses_tree(arg, env),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            let mut u = uses_tree(test, env);
            u.merge(uses_tree(then, env));
            u.merge(uses_tree(else_, env));
            u
        }
        _ => UseInfo {
            fvars: vec![],
            unknown: true,
        },
    }
}

/// Drop-check every Bin node in a structured tree (each op executes;
/// each is independently at-risk).
fn scan_tree(t: &ArithAst, env: &Env, drops: &mut Vec<String>) {
    match t {
        ArithAst::Bin { lhs, rhs, .. } => {
            check_node(&eval_tree(t, env), &uses_tree(t, env), env, drops);
            scan_tree(lhs, env, drops);
            scan_tree(rhs, env, drops);
        }
        ArithAst::Un { arg, .. } => scan_tree(arg, env, drops),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            scan_tree(test, env, drops);
            scan_tree(then, env, drops);
            scan_tree(else_, env, drops);
        }
        ArithAst::Index { key, .. } => scan_tree(key, env, drops),
        _ => {}
    }
}

// ── arith-string mini-language ───────────────────────────────────────
// The float-path lowering (`x = arith("($x + 0.5)")`) carries the whole
// expression as text: numbers, `$vars`, `(…)` grouping, `+ - * / %`
// (`//` too), and the `int(…)` / `sqrt(…)` forms the frontend emits.
// Anything else parses to `Unknown` (refuse).

enum SExpr {
    IntNum(i64),
    FloatNum(f64),
    Var(String),
    UnknownRead, // `$1` params and anything unresolvable
    Neg(Box<SExpr>),
    Bin(String, Box<SExpr>, Box<SExpr>),
    IntCall(Box<SExpr>), // `int(…)` — int-valued, but the inner float
    // op still executes (drop-checked by the scan)
    Sqrt(Box<SExpr>), // float-valued, never exact
    Unknown,
}

struct Parser<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.pos < self.b.len() && (self.b[self.pos] as char).is_whitespace() {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }
    fn expr(&mut self) -> SExpr {
        let mut l = self.term();
        loop {
            self.ws();
            let op = match self.peek() {
                Some(b'+') => "+",
                Some(b'-') => "-",
                _ => return l,
            };
            self.pos += 1;
            let r = self.term();
            l = SExpr::Bin(op.to_string(), Box::new(l), Box::new(r));
        }
    }
    fn term(&mut self) -> SExpr {
        let mut l = self.unary();
        loop {
            self.ws();
            let op = match self.peek() {
                Some(b'*') => {
                    // `**` (pow): refuse the whole node.
                    if self.b.get(self.pos + 1) == Some(&b'*') {
                        return SExpr::Unknown;
                    }
                    "*"
                }
                Some(b'/') => {
                    if self.b.get(self.pos + 1) == Some(&b'/') {
                        self.pos += 2;
                        let r = self.unary();
                        l = SExpr::Bin("//".to_string(), Box::new(l), Box::new(r));
                        continue;
                    }
                    "/"
                }
                Some(b'%') => "%",
                _ => return l,
            };
            self.pos += 1;
            let r = self.unary();
            l = SExpr::Bin(op.to_string(), Box::new(l), Box::new(r));
        }
    }
    fn unary(&mut self) -> SExpr {
        self.ws();
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                SExpr::Neg(Box::new(self.unary()))
            }
            Some(b'+') => {
                self.pos += 1;
                self.unary()
            }
            _ => self.atom(),
        }
    }
    fn atom(&mut self) -> SExpr {
        self.ws();
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                let e = self.expr();
                self.ws();
                if self.peek() == Some(b')') {
                    self.pos += 1;
                    e
                } else {
                    SExpr::Unknown
                }
            }
            Some(b'$') => {
                self.pos += 1;
                let start = self.pos;
                while self.pos < self.b.len()
                    && ((self.b[self.pos] as char).is_ascii_alphanumeric()
                        || self.b[self.pos] == b'_')
                {
                    self.pos += 1;
                }
                let name = String::from_utf8_lossy(&self.b[start..self.pos]).to_string();
                if name.is_empty()
                    || !name
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_alphabetic() || c == '_')
                        .unwrap_or(false)
                {
                    // `$1` params and malformed refs: unresolvable.
                    SExpr::UnknownRead
                } else {
                    SExpr::Var(name)
                }
            }
            Some(c) if (c as char).is_ascii_digit() || c == b'.' => self.number(),
            Some(c) if (c as char).is_ascii_alphabetic() || c == b'_' => self.call(),
            _ => SExpr::Unknown,
        }
    }
    fn number(&mut self) -> SExpr {
        let start = self.pos;
        while self.pos < self.b.len() && (self.b[self.pos] as char).is_ascii_digit() {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.b.get(self.pos) == Some(&b'.') {
            // Require a digit after the dot (`1.` alone is not emitted
            // by the frontend; refuse it rather than guess).
            if !self
                .b
                .get(self.pos + 1)
                .map(|c| (*c as char).is_ascii_digit())
                .unwrap_or(false)
            {
                return SExpr::Unknown;
            }
            is_float = true;
            self.pos += 1;
            while self.pos < self.b.len() && (self.b[self.pos] as char).is_ascii_digit() {
                self.pos += 1;
            }
        }
        if matches!(self.b.get(self.pos), Some(b'e') | Some(b'E')) {
            let mut j = self.pos + 1;
            if matches!(self.b.get(j), Some(b'+') | Some(b'-')) {
                j += 1;
            }
            if !self
                .b
                .get(j)
                .map(|c| (*c as char).is_ascii_digit())
                .unwrap_or(false)
            {
                return SExpr::Unknown;
            }
            is_float = true;
            j += 1;
            while j < self.b.len() && (self.b[j] as char).is_ascii_digit() {
                j += 1;
            }
            self.pos = j;
        }
        if start == self.pos {
            // lone `.`: not a number.
            return SExpr::Unknown;
        }
        let text = String::from_utf8_lossy(&self.b[start..self.pos]).to_string();
        if !is_float {
            if let Ok(n) = text.parse::<i64>() {
                return SExpr::IntNum(n);
            }
            return SExpr::Unknown;
        }
        match text.parse::<f64>() {
            Ok(f) => SExpr::FloatNum(f),
            Err(_) => SExpr::Unknown,
        }
    }
    fn call(&mut self) -> SExpr {
        let start = self.pos;
        while self.pos < self.b.len()
            && ((self.b[self.pos] as char).is_ascii_alphanumeric() || self.b[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let name = String::from_utf8_lossy(&self.b[start..self.pos]).to_string();
        self.ws();
        if self.peek() != Some(b'(') {
            return SExpr::Unknown;
        }
        self.pos += 1;
        let inner = self.expr();
        self.ws();
        if self.peek() != Some(b')') {
            return SExpr::Unknown;
        }
        self.pos += 1;
        match name.as_str() {
            "int" => SExpr::IntCall(Box::new(inner)),
            "sqrt" => SExpr::Sqrt(Box::new(inner)),
            _ => SExpr::Unknown,
        }
    }
}

fn parse_arith_string(s: &str) -> SExpr {
    let mut p = Parser {
        b: s.as_bytes(),
        pos: 0,
    };
    let e = p.expr();
    p.ws();
    if p.pos != p.b.len() {
        // Trailing garbage (`$((…))` nests, shell quoting, …): refuse.
        return SExpr::Unknown;
    }
    e
}

fn eval_sexpr(s: &SExpr, env: &Env) -> FVal {
    match s {
        SExpr::IntNum(n) => FVal {
            dom: Dom::Int,
            exact: false,
            cval: Some(*n as f64),
        },
        SExpr::FloatNum(f) => FVal {
            dom: Dom::Float,
            exact: f32_exact(*f),
            cval: Some(*f),
        },
        SExpr::Var(w) => var_fval(w, env),
        SExpr::UnknownRead => FVal::unknown(),
        SExpr::Neg(a) => {
            let v = eval_sexpr(a, env);
            FVal {
                exact: v.exact && v.dom == Dom::Float,
                cval: v.cval.map(|x| -x),
                dom: v.dom,
            }
        }
        SExpr::Bin(op, l, r) => combine(op.as_str(), &eval_sexpr(l, env), &eval_sexpr(r, env)),
        // `int(…)` is int-valued; `sqrt(…)` is float but never exact.
        SExpr::IntCall(_) => FVal::int_val(),
        SExpr::Sqrt(_) => FVal {
            dom: Dom::Float,
            exact: false,
            cval: None,
        },
        SExpr::Unknown => FVal::unknown(),
    }
}

fn uses_sexpr(s: &SExpr, env: &Env) -> UseInfo {
    match s {
        SExpr::IntNum(_) | SExpr::FloatNum(_) => UseInfo::default(),
        SExpr::Var(w) => UseInfo::var(w, env),
        SExpr::UnknownRead | SExpr::Unknown => UseInfo {
            fvars: vec![],
            unknown: true,
        },
        SExpr::Neg(a) | SExpr::IntCall(a) | SExpr::Sqrt(a) => uses_sexpr(a, env),
        SExpr::Bin(_, l, r) => {
            let mut u = uses_sexpr(l, env);
            u.merge(uses_sexpr(r, env));
            u
        }
    }
}

/// Drop-check every Bin/Sqrt node in a parsed string (`int(…)` is
/// int-valued itself, but its inner float op still executes).
fn scan_sexpr(s: &SExpr, env: &Env, drops: &mut Vec<String>) {
    match s {
        SExpr::Bin(_, l, r) => {
            check_node(&eval_sexpr(s, env), &uses_sexpr(s, env), env, drops);
            scan_sexpr(l, env, drops);
            scan_sexpr(r, env, drops);
        }
        SExpr::Sqrt(a) => {
            check_node(&eval_sexpr(s, env), &uses_sexpr(s, env), env, drops);
            scan_sexpr(a, env, drops);
        }
        SExpr::Neg(a) | SExpr::IntCall(a) => scan_sexpr(a, env, drops),
        _ => {}
    }
}

// ── definition collection ────────────────────────────────────────────

/// One re-evaluable right-hand side per assigned variable. `Other` is
/// any definition this pass cannot classify (unknown calls, arrays,
/// captures, indexed stores, …) — it blocks float-domain membership
/// (REFUSE > GUESS).
enum Rhs {
    Expr(IrExpr),
    Copy(String),
    Int,
    Other,
}

fn is_int_literal_str(s: &str) -> bool {
    let t = s.trim();
    let t = t
        .strip_prefix('+')
        .or_else(|| t.strip_prefix('-'))
        .unwrap_or(t);
    !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit())
}

fn classify_rhs(e: &IrExpr) -> Rhs {
    match e {
        IrExpr::Str(s, _) if is_int_literal_str(s) => Rhs::Int,
        IrExpr::Call { func, args } if func == "getVar" => match args.first() {
            Some(IrExpr::Str(w, _)) => Rhs::Copy(w.clone()),
            _ => Rhs::Other,
        },
        IrExpr::Var(w, _) => Rhs::Copy(w.clone()),
        IrExpr::Arith(_)
        | IrExpr::Call { .. }
        | IrExpr::Ternary { .. }
        | IrExpr::DefinedOr { .. } => Rhs::Expr(e.clone()),
        _ => Rhs::Other,
    }
}

fn eval_rhs(rhs: &Rhs, env: &Env) -> FVal {
    match rhs {
        Rhs::Int => FVal::int_val(),
        Rhs::Other => FVal::unknown(),
        Rhs::Copy(w) => {
            let v = var_fval(w, env);
            FVal {
                dom: v.dom,
                exact: v.exact,
                cval: None,
            }
        }
        Rhs::Expr(e) => eval_expr(e, env),
    }
}

fn eval_expr(e: &IrExpr, env: &Env) -> FVal {
    match e {
        IrExpr::Str(s, _) if is_int_literal_str(s) => FVal::int_val(),
        IrExpr::Arith(t) => eval_tree(t, env),
        IrExpr::Call { func, args } if func == "arith" => match args.first() {
            Some(IrExpr::Str(s, _)) => eval_sexpr(&parse_arith_string(s), env),
            _ => FVal::unknown(),
        },
        IrExpr::Call { func, args } if func == "getVar" => match args.first() {
            Some(IrExpr::Str(w, _)) => {
                let v = var_fval(w, env);
                FVal {
                    dom: v.dom,
                    exact: v.exact,
                    cval: None,
                }
            }
            _ => FVal::unknown(),
        },
        IrExpr::Var(w, _) => {
            let v = var_fval(w, env);
            FVal {
                dom: v.dom,
                exact: v.exact,
                cval: None,
            }
        }
        IrExpr::Ternary { then, else_, .. } => join(&eval_expr(then, env), &eval_expr(else_, env)),
        IrExpr::DefinedOr { expr, default } => {
            join(&eval_expr(expr, env), &eval_expr(default, env))
        }
        _ => FVal::unknown(),
    }
}

// ── statement + expression walk ──────────────────────────────────────

/// Collect one scalar definition, or block the variable (indexed
/// stores, captures, loop vars, declarations — anything whose value
/// this pass cannot see is `Other`).
fn def_target(t: &crate::ir::AssignTarget, rhs: &IrExpr, defs: &mut HashMap<String, Vec<Rhs>>) {
    if t.indices.is_empty() {
        defs.entry(t.var.clone())
            .or_default()
            .push(classify_rhs(rhs));
    } else {
        // `a[i] = v`: an element write to a scalar-tracked name —
        // the value is no longer a plain scalar; block it.
        defs.entry(t.var.clone()).or_default().push(Rhs::Other);
    }
}

fn block(name: &str, defs: &mut HashMap<String, Vec<Rhs>>) {
    defs.entry(name.to_string()).or_default().push(Rhs::Other);
}

fn walk_stmts(
    stmts: &[IrStmt],
    defs: &mut HashMap<String, Vec<Rhs>>,
    env: &Env,
    drops: &mut Vec<String>,
) {
    for st in stmts {
        walk_stmt(st, defs, env, drops);
    }
}

#[allow(clippy::too_many_lines)]
fn walk_stmt(
    st: &IrStmt,
    defs: &mut HashMap<String, Vec<Rhs>>,
    env: &Env,
    drops: &mut Vec<String>,
) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                def_target(t, expr, defs);
            }
            scan_expr(expr, defs, env, drops);
        }
        IrStmt::Declare { vars, init, .. } => {
            for v in vars {
                block(&v.name, defs);
            }
            if let Some(e) = init {
                scan_expr(e, defs, env, drops);
            }
        }
        IrStmt::DeclareArray { var, .. } => block(var, defs),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            scan_expr(cond, defs, env, drops);
            walk_stmts(then, defs, env, drops);
            for (c, b) in elsifs {
                scan_expr(c, defs, env, drops);
                walk_stmts(b, defs, env, drops);
            }
            walk_stmts(else_, defs, env, drops);
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            walk_stmts(body, defs, env, drops);
            for ex in excepts {
                if let Some(m) = &ex.match_expr {
                    scan_expr(m, defs, env, drops);
                }
                if let Some(n) = &ex.as_name {
                    block(n, defs);
                }
                walk_stmts(&ex.body, defs, env, drops);
            }
            walk_stmts(else_body, defs, env, drops);
            walk_stmts(finally_body, defs, env, drops);
        }
        IrStmt::For { var, iter, body } => {
            block(var, defs);
            scan_expr(iter, defs, env, drops);
            walk_stmts(body, defs, env, drops);
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            walk_stmts(init, defs, env, drops);
            scan_expr(cond, defs, env, drops);
            walk_stmts(step, defs, env, drops);
            walk_stmts(body, defs, env, drops);
        }
        IrStmt::While { cond, body } => {
            scan_expr(cond, defs, env, drops);
            walk_stmts(body, defs, env, drops);
        }
        IrStmt::DoWhile { body, cond, .. } => {
            walk_stmts(body, defs, env, drops);
            scan_expr(cond, defs, env, drops);
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            // Same-name vars merge across scopes (conservative: no
            // shadowing here — any inexact def anywhere drops the name
            // everywhere).
            walk_stmts(body, defs, env, drops);
            for (_, b) in named_blocks {
                walk_stmts(b, defs, env, drops);
            }
        }
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
            walk_stmts(b, defs, env, drops);
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            scan_expr(discriminant, defs, env, drops);
            for c in clauses {
                walk_stmts(&c.body, defs, env, drops);
            }
        }
        IrStmt::Redirect { inner, .. } => {
            walk_stmts(inner, defs, env, drops);
        }
        IrStmt::Select { clauses, .. } => {
            for c in clauses {
                if let Some(t) = &c.target {
                    block(t, defs);
                }
                if let Some(ch) = &c.ch {
                    scan_expr(ch, defs, env, drops);
                }
                if let Some(v) = &c.value {
                    scan_expr(v, defs, env, drops);
                }
                walk_stmts(&c.body, defs, env, drops);
            }
        }
        IrStmt::Asm {
            outputs, inputs, ..
        } => {
            for (o, _) in outputs {
                block(o, defs);
            }
            for (_, i) in inputs {
                scan_expr(i, defs, env, drops);
            }
        }
        IrStmt::Exec {
            cmd,
            args,
            capture,
            env: envs,
            ..
        } => {
            if let Some(v) = capture {
                block(v, defs);
            }
            scan_expr(cmd, defs, env, drops);
            for a in args {
                scan_expr(a, defs, env, drops);
            }
            for (_, v) in envs {
                scan_expr(v, defs, env, drops);
            }
        }
        IrStmt::Pipeline {
            stages,
            last_output,
            ..
        } => {
            if let Some(v) = last_output {
                block(v, defs);
            }
            for s in stages {
                walk_stmts(s, defs, env, drops);
            }
        }
        IrStmt::Output { value, .. } => scan_expr(value, defs, env, drops),
        IrStmt::WriteFile { path, content, .. } => {
            scan_expr(path, defs, env, drops);
            scan_expr(content, defs, env, drops);
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => scan_expr(expr, defs, env, drops),
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            if let Some(e) = e {
                scan_expr(e, defs, env, drops);
            }
        }
        IrStmt::SetChildError(e) => scan_expr(e, defs, env, drops),
        IrStmt::Require(_) | IrStmt::RawText(_) | IrStmt::Continue | IrStmt::Break => {}
        IrStmt::Label(_) | IrStmt::Goto(_) => {
            // Control-flow markers: no definitions, no expressions.
        }
        IrStmt::Shaderize { var, body, .. } => {
            // Autoshader loop counter: opaque integer domain — block.
            block(var, defs);
            walk_stmts(body, defs, env, drops);
        }
        IrStmt::Expr(e) => scan_expr(e, defs, env, drops),
        IrStmt::Ext(_) => {
            // Opaque backend extension: treated as observation (see the
            // `Ext` limitation in the module invariant below).
        }
    }
}

/// Drop-scan one expression: every structured `Arith` Bin and every
/// `arith("…")` compound anywhere (defs, conds, args, prints) is an op
/// that executes and is independently at-risk. All other shapes are
/// observations (comparisons, calls, string contexts, `int()`/
fn scan_expr(e: &IrExpr, defs: &mut HashMap<String, Vec<Rhs>>, env: &Env, drops: &mut Vec<String>) {
    match e {
        IrExpr::Arith(t) => scan_tree(t, env, drops),
        IrExpr::Call { func, args } if func == "arith" => {
            if let Some(IrExpr::Str(s, _)) = args.first() {
                let parsed = parse_arith_string(s);
                scan_sexpr(&parsed, env, drops);
            }
            for a in args {
                scan_expr(a, defs, env, drops);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                scan_expr(a, defs, env, drops);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            scan_expr(obj, defs, env, drops);
            for a in args {
                scan_expr(a, defs, env, drops);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            scan_expr(lhs, defs, env, drops);
            scan_expr(rhs, defs, env, drops);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            scan_expr(cond, defs, env, drops);
            scan_expr(then, defs, env, drops);
            scan_expr(else_, defs, env, drops);
        }
        IrExpr::DefinedOr { expr, default } => {
            scan_expr(expr, defs, env, drops);
            scan_expr(default, defs, env, drops);
        }
        IrExpr::Index { key, .. } => scan_expr(key, defs, env, drops),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    scan_expr(x, defs, env, drops);
                }
            }
        }
        IrExpr::Capture { expr, .. } => scan_expr(expr, defs, env, drops),
        IrExpr::Array(elems) => {
            for x in elems {
                scan_expr(x, defs, env, drops);
            }
        }
        IrExpr::ArrayComp {
            var,
            iter,
            elem,
            cond,
        } => {
            // The comprehension binding takes unknown element values;
            // the subexpressions still execute (drop-checked).
            block(var, defs);
            scan_expr(iter, defs, env, drops);
            scan_expr(elem, defs, env, drops);
            if let Some(c) = cond {
                scan_expr(c, defs, env, drops);
            }
        }
        IrExpr::Lambda { body, .. } => walk_stmts(body, defs, env, drops),
        IrExpr::Arrow(b) => walk_stmts(b, defs, env, drops),
        IrExpr::Object(kvs) => {
            for (_, v) in kvs {
                scan_expr(v, defs, env, drops);
            }
        }
        IrExpr::Splice(x) => scan_expr(x, defs, env, drops),
        _ => {}
    }
}

// ── fixpoint driver ──────────────────────────────────────────────────

fn analyze(stmts: &[IrStmt]) -> HashSet<String> {
    // Pass 0: collect every definition site (order-free union — loop
    // bodies count as definitions). Purely syntactic (Rhs kinds only);
    // the empty envs evaluate nothing here.
    let mut defs: HashMap<String, Vec<Rhs>> = HashMap::new();
    {
        let empty = HashSet::new();
        let env = Env {
            float_vars: &empty,
            int_vars: &empty,
            cand: &empty,
        };
        let mut drops = Vec::new();
        walk_stmts(stmts, &mut defs, &env, &mut drops);
    }

    let mut float_vars: HashSet<String> = HashSet::new();
    let mut int_vars: HashSet<String> = HashSet::new();
    // Phase 1: grow the domain sets to fixpoint (a def's domain only
    // moves Unknown → known as the sets grow — monotone).
    let bound = 2 * defs.len() + 4;
    for _ in 0..bound {
        let env = Env {
            float_vars: &float_vars,
            int_vars: &int_vars,
            cand: &float_vars,
        };
        let mut next_float = float_vars.clone();
        let mut next_int = int_vars.clone();
        let mut grown = false;
        for (v, rhs_list) in defs.iter() {
            if rhs_list.is_empty() {
                continue;
            }
            let vals: Vec<FVal> = rhs_list.iter().map(|r| eval_rhs(r, &env)).collect();
            if vals.iter().all(|x| x.dom == Dom::Float) {
                grown |= next_float.insert(v.clone());
            }
            if vals.iter().all(|x| x.dom == Dom::Int) {
                grown |= next_int.insert(v.clone());
            }
        }
        float_vars = next_float;
        int_vars = next_int;
        if !grown {
            break;
        }
    }

    // Phase 2: seed candidates (float-domain vars whose every
    // definition is exact), then remove monotonically: each round
    // drop-scans every op under the surviving set and rechecks
    // def-exactness under it. Bounded by the candidate count.
    let env_full = Env {
        float_vars: &float_vars,
        int_vars: &int_vars,
        cand: &float_vars,
    };
    let mut cand: HashSet<String> = float_vars
        .iter()
        .filter(|v| {
            defs.get(*v)
                .map(|rhs_list| {
                    rhs_list
                        .iter()
                        .map(|r| eval_rhs(r, &env_full))
                        .all(|x| x.dom == Dom::Float && x.exact)
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    for _ in 0..cand.len() + 1 {
        let env = Env {
            float_vars: &float_vars,
            int_vars: &int_vars,
            cand: &cand,
        };
        let mut drops: Vec<String> = Vec::new();
        walk_stmts(stmts, &mut HashMap::new(), &env, &mut drops);
        let drop_set: HashSet<String> = drops.into_iter().collect();
        let next: HashSet<String> = cand
            .iter()
            .filter(|v| {
                !drop_set.contains(*v)
                    && defs
                        .get(*v)
                        .map(|rhs_list| {
                            rhs_list
                                .iter()
                                .map(|r| eval_rhs(r, &env))
                                .all(|x| x.dom == Dom::Float && x.exact)
                        })
                        .unwrap_or(false)
            })
            .cloned()
            .collect();
        if next == cand {
            cand = next;
            break;
        }
        cand = next;
    }
    cand
}

// name: f32-narrow
// prereqs: [] — self-contained by design (re-derives float-domain from
//   statement shapes instead of reading frontend `var_types`, so shell
//   A1 — which has no float literals and no float vars — can never
//   produce a verdict; the corpus gate cannot move on non-float input).
//   A future range-facts field (loop-counter intervals for dyadic
//   exactness of `i * 0.5`-style scalings) plugs into `Env` + `combine`
//   without touching the fixpoint or the renderer contract.
// invariant: analysis-only; no structural mutation. Every verdict is a
//   proof, never a guess:
//   - a var is `Float` only when EVERY definition is float-valued (a
//     single int/str/opaque/capture/indexed/loop-var definition blocks);
//   - `exact` requires finite f32 round-trip at every literal and fold,
//     and exactness-preserving forms elsewhere (accumulator arithmetic
//     like `h = h * 31 + i` ALWAYS refuses — the `4097.0 * 4097.0 →
//     16785408` hazard is the regression test);
//   - the drop fixpoint leaves no at-risk inexact op: under
//     C-strictest promotion an op executes in f32 only when no operand
//     is proven-double or unknown, and every such op here is exact —
//     so the verdict is sound for every backend (JS always computes in
//     f64; C/Cython narrowest).
//   - RENDERER OBLIGATIONS (soundness needs both sides): keep every
//     non-candidate float-domain value double-or-wider (the default —
//     declare `float` ONLY through `is_f32_var`); container/element
//     narrowing (float lists, memoryviews) must re-apply the same
//     whole-program rule to element reads — an `Index` read is
//     unknown-domain here and saves its op, so a renderer that narrows
//     the list behind this pass's back owns the resulting ops.
//   - `Ext`/opaque shapes are observations (Python A1 never emits them;
//     shell A1 never has float domains — both sides documented above).
//   - KNOWN LIMITS (false negatives, never false positives): no
//     loop-counter intervals yet (`x = i * 0.5` over a bounded `i`
//     refuses — NEXT step); structured all-int `/` stays `Int`
//     (shell `$((7/2))` shares the shape); `**`/`sqrt` always refuse.
// scope: offered to cython (owner — `cdef float` declarations), estree
//   (`Math.fround` reuse), c, glsl.
// updates: none (first offer)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn scalar(var: &str) -> AssignTarget {
        AssignTarget {
            var: var.to_string(),
            sigil: None,
            indices: vec![],
        }
    }
    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![scalar(var)],
            expr,
            asm: None,
        }
    }
    fn arith_str(s: &str) -> IrExpr {
        IrExpr::Call {
            func: "arith".to_string(),
            args: vec![IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)],
        }
    }
    fn num(n: i64) -> ArithAst {
        ArithAst::Num(n)
    }
    fn var(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }
    fn arith(a: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(a))
    }
    fn bin(op: &str, l: ArithAst, r: ArithAst) -> ArithAst {
        ArithAst::Bin {
            op: op.to_string(),
            lhs: Box::new(l),
            rhs: Box::new(r),
        }
    }
    /// Static-free entry: `analyze` is pure, so parallel tests never
    /// race on the verdict static (which `transform` alone writes).
    fn run(stmts: &[IrStmt]) -> HashSet<String> {
        analyze(stmts)
    }

    #[test]
    fn exact_literal_narrows() {
        let s = vec![assign("x", arith_str("1.5"))];
        let out = run(&s);
        assert!(out.contains("x"), "1.5 is binary32-exact: {out:?}");
    }

    #[test]
    fn inexact_literal_refuses() {
        // 31.4159 does not survive the f32 round-trip.
        let s = vec![assign("x", arith_str("31.4159"))];
        let out = run(&s);
        assert!(!out.contains("x"), "31.4159 is not exact: {out:?}");
    }

    #[test]
    fn dyadic_hazard_refuses() {
        // THE hazard: 4097² = 16785409 folds exactly in f64 but f32
        // rounds it to 16785408 — a whole-program demotion must refuse.
        let s = vec![assign("x", arith_str("(4097.0 * 4097.0)"))];
        let out = run(&s);
        assert!(!out.contains("x"), "4097² is not f32-exact: {out:?}");
    }

    #[test]
    fn literal_fold_exact_narrows() {
        // 1.5 * 2 folds to 3.0 in f64 and survives f32: narrowable.
        let s = vec![assign("c", arith_str("(1.5 * 2)"))];
        let out = run(&s);
        assert!(out.contains("c"), "1.5*2 folds to exact 3.0: {out:?}");
    }

    #[test]
    fn inexact_fold_refuses() {
        // 1.1 + 2.2 folds to 3.3000000000000003 — not f32-exact.
        let s = vec![assign("x", arith_str("(1.1 + 2.2)"))];
        let out = run(&s);
        assert!(!out.contains("x"), "1.1+2.2 is not exact: {out:?}");
    }

    #[test]
    fn inexact_use_drops_the_operand() {
        // `y = x + 0.5` runs in f32 under C promotion (x float, 0.5
        // converts) and rounds — so y refuses AND x drops.
        let s = vec![
            assign("x", arith_str("1.5")),
            assign("y", arith_str("($x + 0.5)")),
        ];
        let out = run(&s);
        assert!(!out.contains("y"), "x+0.5 is not exact: {out:?}");
        assert!(!out.contains("x"), "x must drop (at-risk op): {out:?}");
    }

    #[test]
    fn exact_form_keeps_both() {
        // `+ 0.0` is exactness-preserving: both narrow.
        let s = vec![
            assign("x", arith_str("1.5")),
            assign("y", arith_str("($x + 0.0)")),
        ];
        let out = run(&s);
        assert!(
            out.contains("x") && out.contains("y"),
            "x+0 keeps both: {out:?}"
        );
    }

    #[test]
    fn int_call_poison_is_int() {
        // `int(…)` is int-valued: the target is not float-domain.
        let s = vec![assign("r", arith_str("int(sqrt($n))"))];
        let out = run(&s);
        assert!(!out.contains("r"), "int() is int-valued: {out:?}");
        assert!(!out.contains("n"), "unknown $n never narrows: {out:?}");
    }

    #[test]
    fn structured_plus_zero_narrows() {
        // `y = x + 0` over a proven var: exact form in tree shape.
        let s = vec![
            assign("x", arith_str("1.5")),
            assign("y", arith(bin("+", var("x"), num(0)))),
        ];
        let out = run(&s);
        assert!(out.contains("x") && out.contains("y"), "tree +0: {out:?}");
    }

    #[test]
    fn structured_plus_one_drops() {
        // `y = x + 1`: general add, at-risk, inexact — both go.
        let s = vec![
            assign("x", arith_str("1.5")),
            assign("y", arith(bin("+", var("x"), num(1)))),
        ];
        let out = run(&s);
        assert!(
            !out.contains("y") && !out.contains("x"),
            "x+1 drops: {out:?}"
        );
    }

    #[test]
    fn observation_does_not_drop() {
        // A print/echo read observes the exact value (same text in f32
        // and f64) — it must not drop the verdict.
        let s = vec![
            assign("x", arith_str("1.5")),
            IrStmt::Expr(IrExpr::Call {
                func: "echo".to_string(),
                args: vec![IrExpr::Call {
                    func: "getVar".to_string(),
                    args: vec![IrExpr::Str("x".to_string(), StrStyle::DoubleQuoted)],
                }],
            }),
        ];
        let out = run(&s);
        assert!(out.contains("x"), "observation keeps x: {out:?}");
    }

    #[test]
    fn mixed_int_float_def_is_neither() {
        // `x = 1; x = 1.5`: not uniformly float — refuse (mirrors
        // autocython's float-domain rule).
        let s = vec![
            assign("x", IrExpr::Str("1".to_string(), StrStyle::DoubleQuoted)),
            assign("x", arith_str("1.5")),
        ];
        let out = run(&s);
        assert!(!out.contains("x"), "mixed defs refuse: {out:?}");
    }

    #[test]
    fn opaque_def_kills() {
        let s = vec![
            assign("x", arith_str("1.5")),
            assign(
                "x",
                IrExpr::Call {
                    func: "something".to_string(),
                    args: vec![],
                },
            ),
        ];
        let out = run(&s);
        assert!(!out.contains("x"), "opaque def kills: {out:?}");
    }

    #[test]
    fn loop_carried_inexact_drops() {
        // A def inside a loop body is a def: the inexact one wins.
        let s = vec![
            assign("x", arith_str("1.5")),
            IrStmt::While {
                cond: arith(bin("<", var("i"), num(10))),
                body: vec![assign("x", arith_str("($x + 1)"))],
            },
        ];
        let out = run(&s);
        assert!(!out.contains("x"), "loop inexact def drops: {out:?}");
    }

    #[test]
    fn shell_shapes_stay_empty() {
        // Integer arith-strings with unknown vars: unknown-domain, no
        // verdicts, no panics.
        let s = vec![assign("a", arith_str("($a + 1)"))];
        let out = run(&s);
        assert!(out.is_empty(), "shell shape: {out:?}");
    }

    #[test]
    fn all_int_division_stays_int() {
        // Structured all-int `/` is Int-domain (shell `$((7/2))` shares
        // the shape) — the Python-aware renderer may narrow `7/2 = 3.5`
        // on its own; the shared pass refuses.
        let s = vec![assign("q", arith(bin("/", num(7), num(2))))];
        let out = run(&s);
        assert!(!out.contains("q"), "all-int / refuses: {out:?}");
    }

    #[test]
    fn big_int_literal_operand_refuses() {
        // `16777217` (2²⁴+1) does not convert to f32 exactly, so
        // `x + 16777217` is inexact even though `x + 0` is exact —
        // int-literal exactness is value-checked, not assumed.
        let s = vec![
            assign("x", arith_str("1.5")),
            assign("y", arith(bin("+", var("x"), num(16777217)))),
        ];
        let out = run(&s);
        assert!(
            !out.contains("y") && !out.contains("x"),
            "x+16777217 drops: {out:?}"
        );
    }

    #[test]
    fn registered_in_transforms_all() {
        // The pass must be on the shared channel (frontend A1 ingress
        // runs `transforms::apply`), SH2_TRANSFORMS-gated like peers.
        assert!(
            crate::transforms::all()
                .iter()
                .any(|(n, _)| *n == "f32-narrow"),
            "f32-narrow registered"
        );
    }

    #[test]
    fn real_py_sh_go_a1_narrows() {
        // End-to-end on genuine py-sh-go `--shir` output shape
        // (`x = 1.5`, `y = x + q` with `q` undefined here): ingress
        // accepts it, `x` proves binary32-exact, `y` refuses (general
        // add), and the unknown `q` saves `x` from the drop.
        let a1 = r#"{"contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[{"expr":{"args":[{"style":"DoubleQuoted","type":"Str","value":"1.5"}],"func":"arith","purity":"PureCpu","type":"Call"},"targets":[{"indices":[],"sigil":null,"var":"x"}],"type":"Assign"},{"expr":{"ast":{"lhs":{"name":"x","type":"Var"},"op":"+","rhs":{"name":"q","type":"Var"},"type":"Bin"},"type":"Arith"},"targets":[{"indices":[],"sigil":null,"var":"y"}],"type":"Assign"}],"subs":[],"type":"Program","var_types":[{"name":"x","type":{"kind":"Float","width":64}},{"name":"y","type":{"kind":"Float","width":64}}]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(a1).expect("ingress accepts py-sh-go A1");
        let out = run(&prog.stmts);
        assert!(out.contains("x"), "x = 1.5 narrows: {out:?}");
        assert!(!out.contains("y"), "y = x+q refuses: {out:?}");
    }

    #[test]
    fn transform_is_analysis_only() {
        let mut s = vec![assign("x", arith_str("1.5"))];
        let before = format!("{s:?}");
        assert!(!transform(&mut s));
        assert_eq!(format!("{s:?}"), before, "no structural mutation");
    }

    #[test]
    fn gate_disables_the_reader() {
        // The bisect machinery disables the verdict end-to-end via
        // SH2_TRANSFORMS — whatever the static holds, a disabled
        // transform reads false. (Env-based, no static racing: no other
        // test in this module touches the process env.)
        let mut s = vec![assign("x", arith_str("1.5"))];
        transform(&mut s);
        std::env::set_var("SH2_TRANSFORMS", "definitely-not-f32-narrow");
        let gated = is_f32_var("x");
        std::env::remove_var("SH2_TRANSFORMS");
        assert!(!gated, "disabled transform must read false");
        assert!(
            crate::transforms::transform_enabled("f32-narrow"),
            "gate defaults to all-enabled"
        );
    }
}
