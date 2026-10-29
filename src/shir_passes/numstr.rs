//! Numeric-string seeding: the vars × params fixpoint (PLAN §3, shared
//! shIR→shIR analysis).
//
// A bash var holds TEXT; the C render homes an `Int`-typed var as a C
// `long long`, which (a) drops per-use `atoll()` on string arithmetic and
// (b) for positionals makes the fn's C params `long long` too. The verdict
// is a joint fixpoint over VARS (every plain-assign RHS of the var is
// int-valued) and PARAMS (every call site passes an int-valued actual at
// the position) — both because the legs feed each other: a `v=$1` copy
// inside a callee is int-valued exactly when the caller's position is
// numeric, and a position is numeric exactly when the actuals are.
//
// Soundness bar (the same one the C ingest's demote_stringy_ints applies):
// homing a var long long changes its STRING value only when the text it
// held was not the canonical decimal of its number. Every accepted write
// is int-valued — Arith/Int/canonical-decimal literals produce canonical
// text, a `v=$N` positional copy carries a value whose call-site actuals
// are themselves int-valued (canonical by the same argument, transitively),
// and arrayIndex/min/max over an INT-homed array is int. The two "never
// seed" boundaries: a bare declare unsets the name ("" re-stringifies to
// "0"), and a call site that OMITS an arg disqualifies that param — the
// unset positional is "", which parses to 0 but re-stringifies to "0".
//
// A row's CALLER liveness gates the param leg only: a defined fn with no
// call site of its own never executes (the walk's statement-enum
// exhaustiveness is already a soundness assumption of this analysis), so
// a row recorded FROM such a caller disqualifies nothing — one dead
// helper must not poison a position that every live caller passes
// canonically (the rb_rect → rb_mkhex shape). The var leg is untouched:
// an uncalled fn's positional copies still key on its (absent) param set
// and stay non-int-valued.
//
// Two phases (mirrors the int_cycles design, jointly over vars × params):
//   1. GROUNDED (least fixpoint from the existing Int verdicts) — everything
//      provable with no assumption.
//   2. ANCHORED COINDUCTION — admits the call-boundary cycles the least
//      fixpoint deadlocks on (`v=$1` in a callee whose actual is `v` at
//      every call site, e.g. a texture sampler whose texel index is the
//      stringified slot). Sound because an admitted var must have at
//      least one ANCHOR: an assign that is int-valued under the grounded
//      sets alone, so some execution path writes a value this analysis
//      proved canonical without assuming the cycle. A PURE self-cycle
//      (`f(){ x=$1; }; f $x`) has no anchor — the value would enter the
//      cycle from "" — and is rejected (the least fixpoint already did).
//
// Consumed by the C backend at ingest (after demote re-validates the A2
// verdicts, so these proposals need the soundness argument above). Per
// PLAN §11 it is offered to the other backends as-is: the inputs
// (`blocked_extra` = the render's force-string set, `int_arrays` = the
// consumer's own int-homed array verdicts) are the only
// backend-specific parts, and a consumer that homes nothing natively can
// pass empty sets (the pass then only proves value flow).

use crate::ir::{InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Backend-supplied inputs. Both sets exist because the two verdicts
/// cannot disagree with the render: `blocked_extra` names vars the
/// render force-strings, `int_arrays` names arrays the consumer homes
/// as native integers.
pub struct Inputs<'a> {
    /// Vars the consuming backend force-strings (the C backend's
    /// `string_loop_vars`: foreach loop vars over int arrays, whose
    /// elements the render takes char*).
    pub blocked_extra: &'a BTreeSet<String>,
    /// Arrays the consumer homes as native ints — element reads and
    /// min/max/sum over those compute an int.
    pub int_arrays: &'a HashSet<String>,
}

/// The verdict. `ints` ⊇ `pinned`; `params` is per-callee numeric
/// position sets. The `assigns`/`sites`/`blocked`/`defined` fields are
/// the analysis tables, kept for diagnostics (SH2_DEBUG_NUMSTR).
pub struct Seed {
    pub ints: HashSet<String>,
    pub params: HashMap<String, BTreeSet<usize>>,
    pub dyn_call: bool,
    /// pre-existing Int verdicts (pinned — never re-proposed)
    pub pinned: HashSet<String>,
    pub assigns: HashMap<String, Vec<(String, IrExpr)>>,
    pub sites: HashMap<String, Vec<(String, Vec<IrExpr>)>>,
    pub blocked: BTreeSet<String>,
    pub defined: BTreeSet<String>,
    /// the consumer's int-homed array verdicts (kept so
    /// `revalidate_params` can re-check actuals under a shrunken int
    /// set with the same element-read rule the fixpoint used)
    pub int_arrays: HashSet<String>,
}

impl Seed {
    /// Removal-only RE-VALIDATION of the params leg against a SHRUNKEN
    /// int set — for a consumer whose own demote removed Int verdicts
    /// the original fixpoint leaned on (the C ingest's
    /// `demote_stringy_ints`: a var with a stringy assign restores Str,
    /// but the position promotions computed while it was still believed
    /// Int were never re-checked — the MIMEcroft rb_o_ndc hole: a
    /// `v=${1}` copy inside `rb_o_rect` looked int-valued because its
    /// site actuals split off vars the fixpoint admitted, so the
    /// position got promoted and the caller-side `atoll` truncated the
    /// fraction text "0.85" that bash's decoder consumed verbatim).
    ///
    /// `ints` must contain ONLY consumer-validated Int verdicts (post
    /// demote). The iteration mirrors phase 2's params half: starting
    /// from the full position sets, drop any position whose live-site
    /// actuals are not int-valued under the current sets; only removals,
    /// so it converges. The result is additionally intersected with the
    /// original verdict — with a shrunken int set monotonicity makes
    /// that intersection a no-op in practice, and it makes the
    /// contract explicit: re-validation may only ever REMOVE positions.
    /// A caller-side reader seed added after the fixpoint may make a
    /// position newly provable; that is not this pass's business.
    pub fn revalidate_params(
        &self,
        ints: &HashSet<String>,
    ) -> HashMap<String, BTreeSet<usize>> {
        let mut params: HashMap<String, BTreeSet<usize>> = HashMap::new();
        if self.dyn_call {
            return params;
        }
        let dead: HashSet<String> = self
            .defined
            .iter()
            .filter(|f| !self.sites.contains_key(*f))
            .cloned()
            .collect();
        for (f, ss) in &self.sites {
            let maxarg = ss.iter().map(|(_, a)| a.len()).max().unwrap_or(0);
            for i in 1..=maxarg {
                params.entry(f.clone()).or_default().insert(i);
            }
        }
        loop {
            let mut removed_any = false;
            let mut drop: Vec<(String, usize)> = Vec::new();
            for (f, ss) in &self.sites {
                for i in params.get(f).into_iter().flatten() {
                    let ok = ss.iter().all(|(caller, args)| {
                        dead.contains(caller)
                            || args.get(i - 1).is_some_and(|a| {
                                expr_int(a, ints, &params, caller, &self.int_arrays)
                            })
                    });
                    if !ok {
                        drop.push((f.clone(), *i));
                    }
                }
            }
            for (f, i) in drop {
                if let Some(ps) = params.get_mut(&f) {
                    ps.remove(&i);
                    removed_any = true;
                }
            }
            if !removed_any {
                break;
            }
        }
        for (f, ps) in params.iter_mut() {
            if let Some(old) = self.params.get(f) {
                *ps = ps.intersection(old).copied().collect();
            } else {
                ps.clear();
            }
        }
        params.retain(|_, ps| !ps.is_empty());
        params
    }
}

/// A one-line shape name for diagnostics.
pub fn shape(e: &IrExpr) -> String {
    match e {
        IrExpr::Int(_) => "Int".into(),
        IrExpr::Str(_, _) => "Str".into(),
        IrExpr::Var(n, _) => format!("Var({n})"),
        IrExpr::Ident(n) => format!("Ident({n})"),
        IrExpr::Arith(_) => "Arith".into(),
        IrExpr::Call { func, .. } => format!("Call({func})"),
        IrExpr::Array(_) => "Array".into(),
        IrExpr::BinOp { .. } => "BinOp".into(),
        IrExpr::Interpolate(_) => "Interpolate".into(),
        IrExpr::Arrow(_) => "Arrow".into(),
        IrExpr::Index { .. } => "Index".into(),
        IrExpr::ArrayComp { .. } => "ArrayComp".into(),
        _ => "?".into(),
    }
}

fn str_arg(args: &[IrExpr], i: usize) -> Option<String> {
    match args.get(i) {
        Some(IrExpr::Str(s, _)) => Some(s.clone()),
        _ => None,
    }
}

/// Is this RHS int-valued (canonical decimal when stringified) under the
/// given verdict sets? `cur` is the enclosing fn's name — a positional
/// read (`getVar("N")`) keys on ITS OWN fn's param set.
pub fn expr_int(
    e: &IrExpr,
    ints: &HashSet<String>,
    params: &HashMap<String, BTreeSet<usize>>,
    cur: &str,
    int_arrays: &HashSet<String>,
) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Arith(_) => true,
        // canonical decimal text (no leading zeros: `x="007"` must
        // keep its exact string)
        IrExpr::Str(s, _) => {
            let t = s.trim();
            let d = t.strip_prefix('-').unwrap_or(t);
            !d.is_empty()
                && d.bytes().all(|c| c.is_ascii_digit())
                && !(d.len() > 1 && d.starts_with('0'))
        }
        IrExpr::Var(n, _) | IrExpr::Ident(n) => {
            ints.contains(n) || matches!(n.as_str(), "?" | "#" | "!" | "$")
        }
        IrExpr::Call { func, args } if func == "getVar" => {
            match str_arg(args, 0).as_deref() {
                Some(n) if matches!(n, "?" | "#" | "!" | "$") => true,
                Some(n) if n.bytes().all(|c| c.is_ascii_digit()) => params
                    .get(cur)
                    .is_some_and(|ps| ps.contains(&n.parse::<usize>().unwrap_or(0))),
                Some(n) => ints.contains(n),
                None => false,
            }
        }
        // `${#x}` (the `len` op, 2-arg) — a length is canonical digit
        // text whatever the subject holds. The `#` OP with a pattern
        // arg (3-arg, `RemoveShortestPrefix` — `${x#pat}`) is NOT: the
        // result is an arbitrary suffix of the subject, so grouping it
        // with `len` conflates bash's two `#` glyphs and made a
        // self-cycle like `lt_s=${lt_s#?}` trivially "true" — seeded
        // Int with no grounding (texture-line text rendered as a C
        // `long long`; the take-field fusion then correctly refuses the
        // now-int cursor and `take_field_fuses_call_site` lost its
        // `strchr`). Strip results stay unproven, like `##`/`%`/`%%`.
        IrExpr::Call { func, args } if func == "param" => {
            matches!(str_arg(args, 0).as_deref(), Some("len"))
        }
        // an IFS word-split of an int-valued expr: that value is
        // canonical decimal text (no interior whitespace, or the
        // Str arm above already rejected it), so the split is a
        // provable no-op — exactly one word, unchanged, and the
        // position mapping stays aligned.
        IrExpr::Call { func, args } if func == "split" => args
            .first()
            .is_some_and(|x| expr_int(x, ints, params, cur, int_arrays)),
        // a typed reduction / element read over an INT-homed array
        // computes an int; over a string array it is decimal text.
        IrExpr::Call { func, args } if matches!(func.as_str(), "min" | "max" | "sum" | "arrayIndex") => {
            matches!(str_arg(args, 0), Some(n) if int_arrays.contains(&n))
        }
        // `str-chop-cursor`'s cursor advance (shir_nodes/field_strip.node):
        // the ABSOLUTE byte offset past the delimiter — a non-negative
        // integer by construction, written as canonical decimal by the
        // render arm that computes it, and grounded by the transform's own
        // canonical `"0"` init (the seed is never a pure self-cycle).
        // Homing the cursor native drops the per-chop `atoll` + itoa +
        // string store the decimal channel costs — the MIMEcroft texture
        // tokenizer runs 6+ chops per texel.
        IrExpr::Ext(n) if n.tag() == "FieldStrip" => true,
        // …and the guarded pair's draining advance (field_strip_end.node):
        // same absolute offset, `end of text` instead of `off` on a miss —
        // equally a non-negative integer by construction, equally grounded
        // by the same canonical `"0"` init. Homing it is REQUIRED, not an
        // optimization: a cursor var whose advance homes native must not
        // be demoted by the next pass or the two advances disagree about
        // the channel the cursor lives on.
        IrExpr::Ext(n) if n.tag() == "FieldStripEnd" => true,
        _ => false,
    }
}

/// Defined function names (nested defs included — same traversal the C
/// lift scan uses).
fn collect_defined(stmts: &[IrStmt], names: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body, .. } => {
                names.insert(name.clone());
                collect_defined(body, names);
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_defined(then, names);
                for (_, b) in elsifs {
                    collect_defined(b, names);
                }
                collect_defined(else_, names);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_defined(body, names),
            _ => {}
        }
    }
}

// The walk. Same discipline as before: exhaustive over the statement
// enum, and an unknown shape disqualifies rather than passes.
type Ctx<'a, 'b> = (
    &'a BTreeSet<String>,
    &'a mut HashMap<String, Vec<(String, IrExpr)>>,
    &'a mut HashMap<String, Vec<(String, Vec<IrExpr>)>>,
    &'a mut HashSet<String>,
    &'a mut bool,
);

fn walk_expr(e: &IrExpr, cur: &str, ctx: &mut Ctx) {
    let (defined, _assigns, sites, _blocked, saw_ext) = ctx;
    match e {
        IrExpr::Call { func, args } => {
            // A literal eval/source/. executes text this walk never
            // parses — it can write ANY variable, so nothing may seed.
            if matches!(func.as_str(), "eval" | "source" | ".") {
                if std::env::var_os("SH2_DEBUG_NUMSTR").is_some() {
                    eprintln!("NUMSTR saw_ext call-func={func:?} in {cur:?}");
                }
                **saw_ext = true;
            }
            // The call vocabulary (see census `user_call`/
            // `array_or_rest`): a direct call of a defined name after
            // `direct_calls`, and the dispatch forms
            // `fnCall("f", [a…])` (+ exec / callDirect / fnValue /
            // $fn_call) — callee name in arg 0, actuals in arg 1's
            // Array (or the flat rest). A dispatch whose name is not a
            // literal may reach any defined fn with text as any
            // position, so it retires the param leg for the whole
            // program (the "" sentinel) — the var leg stays sound:
            // callee BODY assigns are still walked, and with no param
            // verdict a positional read never seeds.
            if matches!(
                func.as_str(),
                "fnCall" | "exec" | "callDirect" | "fnValue" | "$fn_call"
            ) {
                match args.first() {
                    Some(IrExpr::Str(n, _)) if defined.contains(n) => {
                        let actuals: Vec<IrExpr> =
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                items.clone()
                            } else if args.len() > 1 {
                                args[1..].to_vec()
                            } else {
                                Vec::new()
                            };
                        sites
                            .entry(n.clone())
                            .or_default()
                            .push((cur.to_string(), actuals));
                    }
                    Some(IrExpr::Str(_, _)) => {}
                    _ => {
                        sites.entry(String::new()).or_default();
                    }
                }
            } else if defined.contains(func) {
                sites
                    .entry(func.clone())
                    .or_default()
                    .push((cur.to_string(), args.clone()));
            }
            for a in args {
                walk_expr(a, cur, ctx);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                walk_expr(i, cur, ctx);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_expr(lhs, cur, ctx);
            walk_expr(rhs, cur, ctx);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    walk_expr(x, cur, ctx);
                }
            }
        }
        // Arrow bodies are nested statements (subshells, captures) —
        // a disqualifier found inside one belongs to the same pool.
        IrExpr::Arrow(stmts) => walk(stmts, cur, ctx),
        IrExpr::Index { key, .. } => walk_expr(key, cur, ctx),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            walk_expr(iter, cur, ctx);
            walk_expr(elem, cur, ctx);
            if let Some(c) = cond {
                walk_expr(c, cur, ctx);
            }
        }
        // Ext expression nodes are pure forms over their operands
        // (FieldGrab/FieldStrip's text+off, the primitive nodes' fields):
        // walk them so reads and nested call sites inside still register.
        // Skipping them silently would let a hidden `eval` in an operand go
        // unrecorded — the STATEMENT-level Ext arm is the one that blocks
        // a whole program, and it is unchanged here.
        IrExpr::Ext(n) => {
            for c in n.children() {
                walk_expr(c, cur, ctx);
            }
        }
        _ => {}
    }
}

fn walk(stmts: &[IrStmt], cur: &str, ctx: &mut Ctx) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    if t.indices.is_empty() && !t.var.contains('[') {
                        ctx.1
                            .entry(t.var.clone())
                            .or_default()
                            .push((cur.to_string(), expr.clone()));
                    }
                    // a subscript expression can itself call a fn —
                    // a missed call site would leave the param leg
                    // believing a text-carrying position numeric.
                    for ix in &t.indices {
                        walk_expr(ix, cur, ctx);
                    }
                }
                walk_expr(expr, cur, ctx);
            }
            IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => walk_expr(e, cur, ctx),
            IrStmt::If { cond, then, elsifs, else_ } => {
                walk_expr(cond, cur, ctx);
                walk(then, cur, ctx);
                for (c, b) in elsifs {
                    walk_expr(c, cur, ctx);
                    walk(b, cur, ctx);
                }
                walk(else_, cur, ctx);
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, cur, ctx);
                walk(body, cur, ctx);
            }
            // the foreach var takes `char*` elements (or an int-homed
            // array's — still not a value this pass can prove), so it
            // is never seedable; the render's `force_str` covers the
            // same set from the same source.
            IrStmt::For { var, iter, body } => {
                if !var.is_empty() {
                    ctx.3.insert(var.clone());
                }
                walk_expr(iter, cur, ctx);
                walk(body, cur, ctx);
            }
            IrStmt::ForInit { init, cond, step, body } => {
                walk(init, cur, ctx);
                walk_expr(cond, cur, ctx);
                walk(step, cur, ctx);
                walk(body, cur, ctx);
            }
            IrStmt::Function { name, body, .. } => walk(body, name, ctx),
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                walk(b, cur, ctx)
            }
            // an init-less Declare UNSETS the names — the value is
            // "" until something writes it, and "" re-stringifies to
            // "0", so no var it names may be seeded.
            IrStmt::Declare { vars, init, .. } => match init {
                Some(e) => {
                    for v in vars {
                        ctx.1
                            .entry(v.name.clone())
                            .or_default()
                            .push((cur.to_string(), e.clone()));
                    }
                    walk_expr(e, cur, ctx);
                }
                None => {
                    for v in vars {
                        ctx.3.insert(v.name.clone());
                    }
                }
            },
            // an array is never a scalar Int.
            IrStmt::DeclareArray { var, elements, .. } => {
                ctx.3.insert(var.clone());
                for e in elements {
                    walk_expr(e, cur, ctx);
                }
            }
            // a capture's value is command output — unmodelled text.
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            } => {
                walk_expr(cmd, cur, ctx);
                // user-function calls also arrive as a statement:
                // the raw form (`f a b`, literal name = defined fn)
                // or a dispatch word lowered into an Exec
                // (`fnCall("f", [a…])`). Literal eval/source/. makes
                // every assignment unmodelled (see walk_expr).
                if let IrExpr::Str(c, _) = cmd {
                    match c.as_str() {
                        "eval" | "source" | "." => {
                            if std::env::var_os("SH2_DEBUG_NUMSTR").is_some() {
                                eprintln!("NUMSTR saw_ext exec-cmd={c:?} in {cur:?}");
                            }
                            *ctx.4 = true
                        }
                        "exec" | "fnCall" | "callDirect" | "fnValue" | "$fn_call" => {
                            match args.first() {
                                Some(IrExpr::Str(n, _)) if ctx.0.contains(n) => {
                                    let actuals: Vec<IrExpr> =
                                        if let Some(IrExpr::Array(items)) = args.get(1) {
                                            items.clone()
                                        } else if args.len() > 1 {
                                            args[1..].to_vec()
                                        } else {
                                            Vec::new()
                                        };
                                    ctx.2
                                        .entry(n.clone())
                                        .or_default()
                                        .push((cur.to_string(), actuals));
                                }
                                Some(IrExpr::Str(_, _)) => {}
                                _ => {
                                    ctx.2.entry(String::new()).or_default();
                                }
                            }
                        }
                        _ if ctx.0.contains(c) => {
                            ctx.2
                                .entry(c.clone())
                                .or_default()
                                .push((cur.to_string(), args.clone()));
                        }
                        _ => {}
                    }
                } else {
                    // dynamic command word (`$g a b`) can reach any
                    // defined fn — retire the param leg.
                    ctx.2.entry(String::new()).or_default();
                }
                for a in args {
                    walk_expr(a, cur, ctx);
                }
                if let Some(v) = capture {
                    ctx.3.insert(v.clone());
                }
                for r in redirects {
                    walk_expr(r, cur, ctx);
                }
                for (_, e) in env {
                    walk_expr(e, cur, ctx);
                }
            }
            IrStmt::Pipeline { stages, capture, .. } => {
                for st in stages.iter().flatten() {
                    walk(std::slice::from_ref(st), cur, ctx);
                }
                if let Some(v) = capture {
                    ctx.3.insert(v.clone());
                }
            }
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, cur, ctx);
                walk_expr(content, cur, ctx);
            }
            IrStmt::Return(Some(e))
            | IrStmt::Exit(Some(e))
            | IrStmt::Die { expr: e, .. }
            | IrStmt::Warn { expr: e, .. }
            | IrStmt::SetChildError(e) => walk_expr(e, cur, ctx),
            IrStmt::Case { discriminant, clauses } => {
                walk_expr(discriminant, cur, ctx);
                for c in clauses {
                    walk(&c.body, cur, ctx);
                }
            }
            IrStmt::Select { clauses } => {
                for c in clauses {
                    walk(&c.body, cur, ctx);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                walk(inner, cur, ctx);
                for r in redirects {
                    walk_expr(&r.target, cur, ctx);
                }
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                walk(body, cur, ctx);
                for e in excepts {
                    walk(&e.body, cur, ctx);
                }
                walk(else_body, cur, ctx);
                walk(finally_body, cur, ctx);
            }
            // A caller-side fn call hidden in asm operands, or a
            // shaderized loop body (unwaltzed C-frontend only): not
            // modelled here — block everything rather than seed past
            // an unscanned write/call (same verdict as Ext below).
            IrStmt::Asm { inputs, outputs, .. } => {
                if std::env::var_os("SH2_DEBUG_NUMSTR").is_some() {
                    eprintln!("NUMSTR saw_ext=Asm in {cur:?}");
                }
                *ctx.4 = true;
                for (_, e) in inputs.iter().chain(outputs.iter()) {
                    walk_expr(e, cur, ctx);
                }
            }
            IrStmt::Shaderize { body, .. } => {
                if std::env::var_os("SH2_DEBUG_NUMSTR").is_some() {
                    eprintln!("NUMSTR saw_ext=Shaderize in {cur:?}");
                }
                *ctx.4 = true;
                walk(body, cur, ctx);
            }
            // Ext nodes own unknown children, and a transform-declared
            // node that assigns a var we seeded would be a
            // miscompile. The C backend renders the Ext nodes it
            // knows (and refuses the rest loudly), so treat the
            // whole program as unmodelled once one appears: flag it
            // and block every candidate at the end of the walk.
            //
            // GpuOp (shir_nodes/gpu_op.node) is the documented
            // exception: a lifted shell WRITE to a device file —
            // `echo "$v" > /dev/fb0/blocks` — whose payload is the
            // written value and whose fields contain no variable
            // target at all, i.e. an Output to an fd in assign terms.
            // Walk its payload (the reads and nested call sites still
            // register) and keep the program modelled.
            IrStmt::Ext(n) => {
                if let Some(op) =
                    n.as_any().downcast_ref::<crate::shir_nodes::GpuOp>()
                {
                    walk_expr(&op.payload, cur, ctx);
                } else {
                    if std::env::var_os("SH2_DEBUG_NUMSTR").is_some() {
                        eprintln!(
                            "NUMSTR saw_ext=Ext(tag={}) in {cur:?}",
                            n.tag()
                        );
                    }
                    *ctx.4 = true;
                }
            }
            _ => {}
        }
    }
}

/// Run the vars × params fixpoint over `prog` and return the verdict.
pub fn seed(prog: &IrProgram, inputs: &Inputs) -> Seed {
    let mut defined: BTreeSet<String> = BTreeSet::new();
    collect_defined(&prog.stmts, &mut defined);

    // var -> every plain-assign RHS in the program, with the enclosing
    // fn's name ("" = top level). A var seeds only when ALL are
    // int-valued, so a missed site can only ever be a missed SEED — the
    // walk below is exhaustive over the statement enum, and an unknown
    // shape disqualifies rather than passes.
    let mut assigns: HashMap<String, Vec<(String, IrExpr)>> = HashMap::new();
    // callee -> every call site (caller name, args), for the param leg.
    let mut sites: HashMap<String, Vec<(String, Vec<IrExpr>)>> = HashMap::new();
    // a var assigned from a source this analysis does not model (loop
    // var, capture, bare declare) — never seedable.
    let mut blocked: HashSet<String> = HashSet::new();
    let mut saw_ext = false;

    {
        let mut ctx: Ctx = (&defined, &mut assigns, &mut sites, &mut blocked, &mut saw_ext);
        walk(&prog.stmts, "", &mut ctx);
        for sub in &prog.subs {
            walk(&sub.body, "", &mut ctx);
        }
    }
    if std::env::var_os("SH2_DEBUG_NUMSTR").is_some() {
        eprintln!(
            "NUMSTR walk: assigns={} blocked={} saw_ext={}",
            assigns.len(),
            blocked.len(),
            saw_ext
        );
    }
    if saw_ext {
        blocked.extend(assigns.keys().cloned());
        assigns.clear();
        // an unmodelled stmt may hold a call site of its own — the
        // static row is incomplete, which is exactly what the param leg
        // may not assume.
        sites.clear();
    }
    // exported vars come back from a child process as text the IR never
    // wrote — never seed those either.
    blocked.extend(prog.var_bash_env.iter().cloned());
    blocked.extend(inputs.blocked_extra.iter().cloned());

    for v in &blocked {
        assigns.remove(v);
    }

    // existing Int verdicts seed the fixpoint (validated by whoever
    // typed them — the C ingest's demote —) and are never removed.
    let pinned: HashSet<String> = prog
        .var_types
        .iter()
        .filter(|(_, t)| *t == IrType::Int)
        .map(|(n, _)| n.clone())
        .collect();
    let int_arrays = inputs.int_arrays;
    // the "" sentinel: a dynamic dispatch (`fnCall($g, …)` / `$g args`)
    // can pass text at any position of any simple-name call, so no static
    // position may be deemed numeric (the walked row of static sites is
    // provably incomplete).
    let dyn_call = sites.remove("").is_some();

    // Defined fns with no observed call site of their own: statically
    // unreached. Rows they contribute to OTHER fns' site lists are rows
    // of code that never runs — see the module doc. (Disjoint from the
    // sites keys by construction, so a row's caller being dead never
    // hides a live one.)
    let dead: HashSet<String> = defined
        .iter()
        .filter(|f| !sites.contains_key(*f))
        .cloned()
        .collect();

    // Phase 1 — GROUNDED: least fixpoint from the pinned set.
    let mut ints_g: HashSet<String> = pinned.clone();
    let mut params_g: HashMap<String, BTreeSet<usize>> = HashMap::new();
    loop {
        let mut next_params: HashMap<String, BTreeSet<usize>> = HashMap::new();
        if !dyn_call {
            for (f, ss) in &sites {
                let maxarg = ss.iter().map(|(_, a)| a.len()).max().unwrap_or(0);
                for i in 1..=maxarg {
                    if ss.iter().all(|(caller, args)| {
                        dead.contains(caller)
                            || args.get(i - 1)
                                .is_some_and(|a| expr_int(a, &ints_g, &params_g, caller, int_arrays))
                    }) {
                        next_params.entry(f.clone()).or_default().insert(i);
                    }
                }
            }
        }
        let mut next_ints: HashSet<String> = pinned.clone();
        for (v, als) in &assigns {
            if als
                .iter()
                .all(|(f, e)| expr_int(e, &ints_g, &params_g, f, int_arrays))
            {
                next_ints.insert(v.clone());
            }
        }
        if next_ints == ints_g && next_params == params_g {
            break;
        }
        ints_g = next_ints;
        params_g = next_params;
    }

    // Phase 2 — ANCHORED COINDUCTION: candidates are the non-grounded
    // vars with at least one ANCHOR (an assign int-valued under the
    // grounded sets alone). Iterate a descending removal: any candidate
    // whose assigns are not all int-valued under the current joint sets
    // leaves, and any param position whose actuals are not all
    // int-valued under the current sets leaves. Finite lattice, only
    // removals, so it converges.
    let mut params: HashMap<String, BTreeSet<usize>> = HashMap::new();
    if !dyn_call {
        for (f, ss) in &sites {
            let maxarg = ss.iter().map(|(_, a)| a.len()).max().unwrap_or(0);
            for i in 1..=maxarg {
                params.entry(f.clone()).or_default().insert(i);
            }
        }
    }
    let mut candidates: HashSet<String> = assigns
        .iter()
        .filter(|(v, _)| !ints_g.contains(*v))
        .filter(|(_, als)| {
            als.iter()
                .any(|(f, e)| expr_int(e, &ints_g, &params_g, f, int_arrays))
        })
        .map(|(v, _)| v.clone())
        .collect();
    let mut ints = ints_g.clone();
    ints.extend(candidates.iter().cloned());
    // Phase 2b — COPY CLOSURE: a pure pass-through (a swap temp such
    // as `rbt_t=$rbt_x1` in MIMEcroft's rb_tri, whose every assign is a
    // read of another var) has no anchor of its own, so the per-var
    // anchor filter above cannot admit it — and without it the anchored
    // cycle around it collapses in the removal loop below: x2's copy of
    // `t` fails once `t` leaves, then x1's copy of `x2` follows, and
    // three long-long-worthy scalars pay the atoll/snprintf channel
    // forever. Admit any var whose every assign is int-valued while
    // GRANTING the candidates int, grown least-fixpoint-style FROM the
    // anchored set: a var only enters when its reads already resolve,
    // so a purely text cycle (`w=$x; x=$w` with no producer anywhere)
    // never enters — nothing joins in round 1 without an anchored name
    // behind it. Grounding therefore still comes from the anchors; this
    // only widens candidacy. The removal loop re-validates every
    // candidate afterwards, so a member whose support collapses still
    // leaves (taking its dependents with it).
    loop {
        let mut grew = false;
        for (v, als) in &assigns {
            if ints.contains(v) {
                continue;
            }
            if als
                .iter()
                .all(|(f, e)| expr_int(e, &ints, &params_g, f, int_arrays))
            {
                ints.insert(v.clone());
                candidates.insert(v.clone());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    loop {
        let mut removed_any = false;
        // params first: a position that goes unsat can unseat its
        // caller's vars in the same round.
        if !dyn_call {
            let mut drop: Vec<(String, usize)> = Vec::new();
            for (f, ss) in &sites {
                for i in params.get(f).into_iter().flatten() {
                    let ok = ss.iter().all(|(caller, args)| {
                        dead.contains(caller)
                            || args.get(i - 1)
                                .is_some_and(|a| expr_int(a, &ints, &params, caller, int_arrays))
                    });
                    if !ok {
                        drop.push((f.clone(), *i));
                    }
                }
            }
            for (f, i) in drop {
                if let Some(ps) = params.get_mut(&f) {
                    ps.remove(&i);
                    removed_any = true;
                }
            }
        }
        let mut drop_vars: Vec<String> = Vec::new();
        for v in &candidates {
            let als = &assigns[v];
            if !als
                .iter()
                .all(|(f, e)| expr_int(e, &ints, &params, f, int_arrays))
            {
                drop_vars.push(v.clone());
            }
        }
        for v in drop_vars {
            candidates.remove(&v);
            ints.remove(&v);
            removed_any = true;
        }
        if !removed_any {
            break;
        }
    }

    Seed {
        ints,
        params,
        dyn_call,
        pinned,
        assigns,
        sites,
        blocked: blocked.into_iter().collect(),
        defined,
        int_arrays: inputs.int_arrays.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrType, StrStyle};

    fn prog_of(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            var_storage: vec![],
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }

    fn assign(name: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: name.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn var(name: &str) -> IrExpr {
        IrExpr::Var(name.to_string(), None)
    }

    fn str(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn int(i: i64) -> IrExpr {
        IrExpr::Int(i)
    }

    fn pos(n: usize) -> IrExpr {
        // a positional read lowers to getVar("<n>")
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![str(&n.to_string())],
        }
    }

    fn fn_def(name: &str, body: Vec<IrStmt>) -> IrStmt {
        IrStmt::Function {
            name: name.to_string(),
            params: vec![],
            body,
            named_blocks: vec![],
        }
    }

    /// a caller-side call of a defined fn with the given actuals.
    fn call(name: &str, args: Vec<IrExpr>) -> IrStmt {
        IrStmt::Exec {
            cmd: str(name),
            args,
            capture: None,
            redirects: vec![],
            env: vec![],
        }
    }

    fn run(prog: &IrProgram) -> Seed {
        let empty = BTreeSet::new();
        let empty_h = HashSet::new();
        seed(
            prog,
            &Inputs {
                blocked_extra: &empty,
                int_arrays: &empty_h,
            },
        )
    }

    /// The rb_tex_sample shape: the callee copies `$N` into a var that
    /// every call site passes back as the actual at the same position —
    /// a call-boundary cycle. Grounded alone it deadlocks; the anchored
    /// phase (here: an int-valued side assign) settles it.
    #[test]
    fn anchored_cycle_admitted() {
        let p = prog_of(vec![
            fn_def(
                "sampler",
                vec![
                    assign("x", pos(1)),
                    assign("y", pos(2)),
                    // an anchor: some path writes a canonical value.
                    assign("x", int(9)),
                ],
            ),
            assign("x", str("7")), // grounded entry into the cycle
            assign("y", int(0)),
            call("sampler", vec![var("x"), var("y")]),
            call("sampler", vec![var("x"), var("y")]),
        ]);
        let s = run(&p);
        assert!(s.ints.contains("x") && s.ints.contains("y"), "ints={:?}", s.ints);
        assert_eq!(s.params.get("sampler").map(|p| p.len()), Some(2));
    }

    /// A PURE self-cycle has no anchor: `f(){ x=$1; }; f $x` — the value
    /// would enter the cycle from "" (unset), which parses to 0 but
    /// re-stringifies to "0". Must NOT seed (the least fixpoint already
    /// rejected it; the anchored phase must not weaken that).
    #[test]
    fn pure_cycle_rejected() {
        let p = prog_of(vec![fn_def("f", vec![assign("x", pos(1))]), call("f", vec![var("x")])]);
        let s = run(&p);
        assert!(!s.ints.contains("x"), "ints={:?}", s.ints);
    }

    /// A row recorded from a DEAD caller (defined, never called) must not
    /// disqualify the callee's position: that call never executes. This
    /// is the rb_rect→rb_mkhex shape — a dead helper passing a text
    /// actual poisoned a position every live caller passes numerically.
    #[test]
    fn dead_caller_actual_does_not_block_param() {
        let p = prog_of(vec![
            fn_def("g", vec![assign("gx", pos(1)), assign("gx", int(0))]),
            // d is never called; its actual is a var that cannot seed.
            fn_def("d", vec![call("g", vec![var("s")])]),
            call("g", vec![int(1)]),
            assign("s", str("ff")),
        ]);
        let s = run(&p);
        assert!(
            s.params.get("g").is_some_and(|ps| ps.contains(&1)),
            "params={:?}",
            s.params
        );
        assert!(s.ints.contains("gx"), "ints={:?}", s.ints);
        // the dead fn's own positional copies still do not seed
        // (absent param set): only the PARAM leg gained liveness.
    }

    /// The same text actual from a LIVE caller still disqualifies the
    /// position (and through it the callee's positional read).
    #[test]
    fn live_caller_text_actual_still_blocks_param() {
        let p = prog_of(vec![
            fn_def("g", vec![assign("gx", pos(1)), assign("gx", int(0))]),
            call("g", vec![int(1)]),
            call("g", vec![var("s")]),
            assign("s", str("ff")),
        ]);
        let s = run(&p);
        assert!(
            !s.params.get("g").is_some_and(|ps| ps.contains(&1)),
            "params={:?}",
            s.params
        );
        assert!(!s.ints.contains("gx"), "ints={:?}", s.ints);
    }

    /// A non-canonical literal disqualifies its var everywhere, even
    /// inside a cycle (`x="007"` must keep its exact string).
    #[test]
    fn noncanonical_literal_disqualifies() {
        let p = prog_of(vec![
            fn_def("f", vec![assign("x", pos(1)), assign("x", str("007"))]),
            assign("x", int(1)),
            call("f", vec![var("x")]),
        ]);
        let s = run(&p);
        assert!(!s.ints.contains("x"), "ints={:?}", s.ints);
    }

    /// A call site that OMITS an arg disqualifies that position (the
    /// unset positional is "" — parses to 0, re-stringifies to "0").
    #[test]
    fn omitted_arg_disqualifies_param() {
        let p = prog_of(vec![
            fn_def("f", vec![assign("a", pos(1)), assign("a", pos(2)), assign("a", int(0))]),
            call("f", vec![int(1), int(2)]),
            call("f", vec![int(3)]), // position 2 omitted
        ]);
        let s = run(&p);
        let ps = s.params.get("f");
        assert!(ps.is_some_and(|p| p.contains(&1)), "params={:?}", s.params);
        assert!(!ps.is_some_and(|p| p.contains(&2)), "params={:?}", s.params);
        // position 2 not numeric ⇒ `b=$2`-style reads of it don't seed:
        assert!(!s.ints.contains("a"), "ints={:?}", s.ints);
    }

    /// A dynamic dispatch (`$g args`) retires the param leg entirely,
    /// but a grounded var still seeds.
    #[test]
    fn dyn_call_retires_params_but_not_vars() {
        let p = prog_of(vec![
            fn_def("f", vec![assign("a", pos(1)), assign("a", int(0))]),
            call("f", vec![int(1)]),
            // dynamic command word: retires every position.
            IrStmt::Exec {
                cmd: var("g"),
                args: vec![str("f"), int(1)],
                capture: None,
                redirects: vec![],
                env: vec![],
            },
            assign("v", int(5)),
        ]);
        let s = run(&p);
        assert!(s.dyn_call);
        assert!(s.params.is_empty(), "params={:?}", s.params);
        assert!(s.ints.contains("v"), "ints={:?}", s.ints);
    }

    /// eval executes unmodelled text — nothing may seed (poison sweep).
    #[test]
    fn eval_poisons() {
        let p = prog_of(vec![IrStmt::Expr(IrExpr::Call {
            func: "eval".to_string(),
            args: vec![str("x=1")],
        }), assign("v", int(5))]);
        let s = run(&p);
        assert!(!s.ints.contains("v"), "ints={:?}", s.ints);
        assert!(s.assigns.is_empty());
    }

    /// An existing Int verdict pins the set (never re-proposed), and the
    /// pass adds to it without removing.
    #[test]
    fn pinned_verdicts_survive() {
        let mut p = prog_of(vec![assign("pinned", int(1)), assign("v", int(2))]);
        p.var_types.push(("pinned".to_string(), IrType::Int));
        let s = run(&p);
        assert!(s.pinned.contains("pinned"));
        assert!(s.ints.contains("pinned") && s.ints.contains("v"));
    }

    /// POSITIONS RE-VALIDATION — the MIMEcroft rb_o_ndc hole: a
    /// positional copy inside `f` is int-valued exactly when f's own
    /// position is, and f's position keys on an actual (`t`) the
    /// fixpoint proves int — but the CONSUMER demotes `t` (its text
    /// reaches an observer this analysis cannot model; the C render
    /// text-reads it). The promotions computed while `t` was still
    /// believed Int must be withdrawable: `revalidate_params` under the
    /// post-demotion int set drops f's position (its actual is `t`) and
    /// through it g's (its actual is f's copy).
    #[test]
    fn revalidate_params_withdraws_demoted_chain() {
        let p = prog_of(vec![
            fn_def("f", vec![assign("a", pos(1))]),
            fn_def("g", vec![assign("gx", pos(1)), assign("gx", int(0))]),
            assign("t", str("5")),
            assign("t", int(0)),
            call("f", vec![var("t")]),
            call("g", vec![var("a")]),
        ]);
        let s = run(&p);
        // the fixpoint admits the whole chain: `t` is grounded-int,
        // f's position 1 and g's position 1 follow from it.
        assert!(s.ints.contains("t") && s.ints.contains("a"), "ints={:?}", s.ints);
        assert!(s.params.get("f").is_some_and(|ps| ps.contains(&1)), "params={:?}", s.params);
        assert!(s.params.get("g").is_some_and(|ps| ps.contains(&1)), "params={:?}", s.params);
        // the consumer demotes `t` and, through the copy, `a`:
        let ints_now: HashSet<String> = s
            .ints
            .iter()
            .filter(|v| *v != "t" && *v != "a")
            .cloned()
            .collect();
        let rp = s.revalidate_params(&ints_now);
        assert!(
            !rp.get("f").is_some_and(|ps| ps.contains(&1)),
            "f position survived: {rp:?}"
        );
        assert!(
            !rp.get("g").is_some_and(|ps| ps.contains(&1)),
            "g position survived: {rp:?}"
        );
    }
}
