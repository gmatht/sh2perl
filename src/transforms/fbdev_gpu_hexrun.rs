//! fbdev-gpu-hexrun — the rasterizer's hex-run build becomes ONE repeat op.
//!
//! ## Need
//!
//! MIMEcroft's bash rasterizer builds a framebuffer row run by REPEATING an
//! 8-hex-char pixel. `rb_mkrun N "$PX"` (`bash-o4/src/sh/rb-raster.sh:271`)
//! sets the global `rb_run` to N copies of PX by doubling a 256-copy block
//! and appending the remainder one pixel at a time. For a 320-pixel run that
//! is ~65 grow-and-append stores over a buffer climbing to 2560 bytes — the
//! comment at `rb-raster.sh:311` measures the garbage it makes (~2KB per
//! pixel of the 1-pixel path, ~1GB/frame when it was hit per pixel directly).
//! The row SPLICE that consumes the run is `str-splice-fold`'s node
//! (`StrSplice`, whose `_sh_splice_arr` already patches a row in place when
//! the replacement is as wide as the span); this pass is the *generator*
//! half of the same row: the run itself.
//!
//! The row is `unit` repeated `count` times, which is exactly what the
//! shared `RepeatStr` node means — and `TextOps#repeat_str_sound` already
//! proves that meaning (`concat (replicate count text)`, its length
//! `count * length text`, and the zero/one/empty identities the arms rely
//! on). So this lowering introduces NO new semantic node: it reuses a node
//! whose law is proved, which is the "tie the model to code" half of the
//! mission.
//!
//! ## The rewrite
//!
//!     <builder> N UNIT          # a call of a verified run-builder
//!  →  rb_run = RepeatStr(UNIT, N)
//!
//! The C backend renders that node as one `_sh_repeat_v` call (one grow, one
//! doubling fill, no per-pixel append); the other backends already render
//! `RepeatStr` (`C x N`, `.repeat()`, `strings.Repeat`, …). The transform is
//! gated to the `c` target (`transform_applies`), so every other target
//! keeps the original call statement byte for byte — the fallback is "the
//! pass does not fire", which is the `fbdev-gpu-calls` precedent.
//!
//! ## REFUSE > GUESS (each refusal keeps the original call)
//!
//! - the callee must be defined EXACTLY ONCE in this program;
//! - its body must be the closed run-builder template, matched statement by
//!   statement (two positional params, `out=""`, `[ "$n" -le 0 ]` → `return`,
//!   an 8x self-doubling buffer, `c = n/256`, a block append loop, `rem =
//!   n - c*256`, a pixel append loop). Any deviation — a different block
//!   size, an extra statement, a different increment — refuses, because the
//!   equivalence "this body == repeat" is a property of THIS template;
//! - the builder's temporaries (everything it assigns except the output)
//!   must not be mentioned ANYWHERE outside its own body (a conservative
//!   whole-program census: a leftover read of a temp would observe a value
//!   the rewrite no longer computes);
//! - the call must be a statement `exec(F, [n, unit])` with exactly two
//!   words (a `$(…)` capture goes through `direct-calls` instead).
//!
//! ## Why not a `GpuOp` node
//!
//! `GpuOp` (shir_nodes/gpu_op.node) lifts a device WRITE and its C fallback
//! IS that write; a producer has no write to fall back to, and a leftover
//! `IrStmt::Ext` poisons the C backend's program-wide lift map (see the
//! `has_ext` bail in `c_backend.rs` — that is why `fbdev-gpu-calls` rewrites
//! every GpuOp back into a plain Call before render). A value-level rewrite
//! to a node that C already renders is therefore the only shape that keeps
//! the firewall intact.
//!
//! ## Kill switch
//!
//! `SH2_NO_GPU_ROWS=1` disables the lift without a rebuild (the A/B switch,
//! the `SH2_NO_SPLICE` precedent); off, the call site is untouched.

use crate::ir::{ArithAst, AssignTarget, InterpPart, IrExpr, IrStmt, StrStyle};
use crate::shir_nodes::RepeatStr;

/// The one A/B kill switch (transform + cache key read the same predicate).
fn kill_switch() -> bool {
    std::env::var("SH2_NO_GPU_ROWS").map(|v| v == "1").unwrap_or(false)
}

/// A verified run-builder: the variable its accumulation lands in, plus every
/// other variable it assigns (its temporaries, which must be dead outside).
#[derive(Debug, Clone)]
pub struct RunBuilder {
    pub out: String,
    pub temps: Vec<String>,
}

/// Apply the rewrite. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if kill_switch() {
        return false;
    }
    let builders = verified_builders(stmts);
    if builders.is_empty() {
        return false;
    }
    let mut changed = false;
    walk(stmts, &builders, &mut changed);
    changed
}

/// Every program-defined function, refusing a name defined more than once
/// (a redefinition makes "the callee's body" ambiguous).
fn defined_fns(stmts: &[IrStmt]) -> Vec<(String, Vec<IrStmt>)> {
    let mut out: Vec<(String, Vec<IrStmt>)> = Vec::new();
    fn scan(stmts: &[IrStmt], out: &mut Vec<(String, Vec<IrStmt>)>) {
        for st in stmts {
            match st {
                IrStmt::Function { name, body, .. } => {
                    out.push((name.clone(), body.clone()));
                    scan(body, out);
                }
                IrStmt::If { then, elsifs, else_, .. } => {
                    scan(then, out);
                    for (_, b) in elsifs {
                        scan(b, out);
                    }
                    scan(else_, out);
                }
                IrStmt::While { body, .. }
                | IrStmt::For { body, .. }
                | IrStmt::ForInit { body, .. }
                | IrStmt::DoWhile { body, .. }
                | IrStmt::Subshell(body)
                | IrStmt::Block(body)
                | IrStmt::Background(body)
                | IrStmt::Redirect { inner: body, .. } => scan(body, out),
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        scan(&c.body, out);
                    }
                }
                _ => {}
            }
        }
    }
    scan(stmts, &mut out);
    // drop every name that appears twice
    let mut dup: Vec<String> = Vec::new();
    for (i, (n, _)) in out.iter().enumerate() {
        if out.iter().skip(i + 1).any(|(m, _)| m == n) && !dup.contains(n) {
            dup.push(n.clone());
        }
    }
    out.retain(|(n, _)| !dup.contains(n));
    out
}

/// The builders this program defines, with the dead-outside census applied.
fn verified_builders(prog: &[IrStmt]) -> Vec<(String, RunBuilder)> {
    let fns = defined_fns(prog);
    let mut out = Vec::new();
    for (name, body) in &fns {
        let Some(b) = verify_builder(body) else { continue };
        // every temporary must be unmentioned outside the builder's own body
        if b.temps.iter().any(|t| mentioned_outside(prog, name, t)) {
            continue;
        }
        out.push((name.clone(), b));
    }
    out
}

/// ── the closed template ──────────────────────────────────────────────

/// `Assign { targets: [one plain var], expr }` → (var, expr).
fn assign1(st: &IrStmt) -> Option<(&str, &IrExpr)> {
    let IrStmt::Assign { targets, expr, asm: None } = st else {
        return None;
    };
    let [t] = targets.as_slice() else { return None };
    if !t.indices.is_empty() || t.sigil.is_some() {
        return None;
    }
    Some((t.var.as_str(), expr))
}

/// `getVar("NAME")` — a shell positional / variable read in a word.
fn getvar(e: &IrExpr) -> Option<&str> {
    let IrExpr::Call { func, args } = e else { return None };
    if func != "getVar" {
        return None;
    }
    match args.as_slice() {
        [IrExpr::Str(n, _)] => Some(n.as_str()),
        _ => None,
    }
}

/// `param("", NAME)` — the plain scalar read of NAME.
fn param_read(e: &IrExpr) -> Option<&str> {
    let IrExpr::Call { func, args } = e else { return None };
    if func != "param" {
        return None;
    }
    match args.as_slice() {
        [IrExpr::Str(op, _), IrExpr::Str(n, _)] if op.is_empty() => Some(n.as_str()),
        _ => None,
    }
}

/// The words of a `[ … ]` test, quotes stripped.
fn test_tokens(cond: &IrExpr) -> Option<Vec<String>> {
    let IrExpr::Call { func, args } = cond else {
        return None;
    };
    if func != "test" {
        return None;
    }
    match args.as_slice() {
        [IrExpr::Str(s, _)] => Some(
            s.split_whitespace()
                .map(|t| t.trim_matches(|c| c == '"' || c == '\'').to_string())
                .collect(),
        ),
        _ => None,
    }
}

/// `"${a}${b}"` with both parts plain reads.
fn interp2(e: &IrExpr, a: &str, b: &str) -> bool {
    let IrExpr::Interpolate(parts) = e else { return false };
    let [InterpPart::Expr(p0), InterpPart::Expr(p1)] = parts.as_slice() else {
        return false;
    };
    param_read(p0) == Some(a) && param_read(p1) == Some(b)
}

/// `i=$((i + 1))`
fn incr_of(e: &IrExpr, i: &str) -> bool {
    let IrExpr::Arith(a) = e else { return false };
    matches!(
        &**a,
        ArithAst::Bin { op, lhs, rhs }
            if op == "+"
                && matches!(&**lhs, ArithAst::Var(v) | ArithAst::Ident(v) if v == i)
                && matches!(&**rhs, ArithAst::Num(1))
    )
}

/// `VAR = n / K`
fn is_div(e: &IrExpr, var: &str, k: i64) -> bool {
    let IrExpr::Arith(a) = e else { return false };
    matches!(
        &**a,
        ArithAst::Bin { op, lhs, rhs }
            if op == "/"
                && matches!(&**lhs, ArithAst::Var(v) | ArithAst::Ident(v) if v == var)
                && matches!(&**rhs, ArithAst::Num(n) if *n == k)
    )
}

/// `n - c * K`
fn is_rem(e: &IrExpr, n: &str, c: &str, k: i64) -> bool {
    let IrExpr::Arith(a) = e else { return false };
    let ArithAst::Bin { op, lhs, rhs } = &**a else { return false };
    if op != "-" || !matches!(&**lhs, ArithAst::Var(v) | ArithAst::Ident(v) if v == n) {
        return false;
    }
    matches!(
        &**rhs,
        ArithAst::Bin { op, lhs, rhs }
            if op == "*"
                && matches!(&**lhs, ArithAst::Var(v) | ArithAst::Ident(v) if v == c)
                && matches!(&**rhs, ArithAst::Num(m) if *m == k)
    )
}

/// A loop bound: a literal (`-lt 8`) or a variable (`-lt "$c"`).
#[derive(Debug, Clone, PartialEq)]
enum Bound {
    Lit(i64),
    Var(String),
}

/// A counter loop's condition → (counter var, bound). Both spellings the
/// frontend produces are accepted: the raw `[ "$i" -lt N ]` test and the
/// structured `BinOp { Lt }` that `test-lowering`/`for-recovery` mint.
fn cond_bound(e: &IrExpr) -> Option<(String, Bound)> {
    match e {
        IrExpr::BinOp { lhs, op: crate::ir::BinOpKind::Lt, rhs } => {
            let IrExpr::Arith(a) = &**lhs else { return None };
            let i = match &**a {
                ArithAst::Var(v) | ArithAst::Ident(v) => v.clone(),
                _ => return None,
            };
            let IrExpr::Arith(b) = &**rhs else { return None };
            let bound = match &**b {
                ArithAst::Num(k) => Bound::Lit(*k),
                ArithAst::Var(v) | ArithAst::Ident(v) => Bound::Var(v.clone()),
                _ => return None,
            };
            Some((i, bound))
        }
        _ => {
            let t = test_tokens(e)?;
            if t.len() != 3 || t[1] != "-lt" {
                return None;
            }
            let i = t[0].strip_prefix('$')?.to_string();
            let bound = match t[2].strip_prefix('$') {
                Some(v) => Bound::Var(v.to_string()),
                None => Bound::Lit(t[2].parse::<i64>().ok()?),
            };
            Some((i, bound))
        }
    }
}

fn is_zero_lit(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(s, _) => s.trim() == "0",
        IrExpr::Int(n) => *n == 0,
        _ => false,
    }
}

/// A counter loop over `bound` whose only work is `acc="${acc}${part}"`:
/// either the raw `while` form (body = [acc, i=i+1]) or the `ForInit` form
/// `for-recovery` builds (init = [i=0], step = [i=i+1], body = [acc]).
/// Returns the counter's name, so the three loops can be tied together.
fn loop_matches(st: &IrStmt, bound: &Bound, acc: &str, part: &str) -> Option<String> {
    match st {
        IrStmt::While { cond, body } => {
            let (i, b) = cond_bound(cond)?;
            if &b != bound {
                return None;
            }
            let [a0, a1] = body.as_slice() else { return None };
            let (v0, e0) = assign1(a0)?;
            let (v1, e1) = assign1(a1)?;
            if v0 != acc || !interp2(e0, acc, part) || v1 != i || !incr_of(e1, &i) {
                return None;
            }
            Some(i)
        }
        IrStmt::ForInit { init, cond, step, body, .. } => {
            let (i, b) = cond_bound(cond)?;
            if &b != bound {
                return None;
            }
            let [s0] = init.as_slice() else { return None };
            let (vi, ei) = assign1(s0)?;
            if vi != i || !is_zero_lit(ei) {
                return None;
            }
            let [t0] = step.as_slice() else { return None };
            let (vs, es) = assign1(t0)?;
            if vs != i || !incr_of(es, &i) {
                return None;
            }
            let [a0] = body.as_slice() else { return None };
            let (v0, e0) = assign1(a0)?;
            if v0 != acc || !interp2(e0, acc, part) {
                return None;
            }
            Some(i)
        }
        _ => None,
    }
}

/// Verify the closed run-builder template. Returns the accumulation var and
/// the temps, or None.
pub fn verify_builder(body: &[IrStmt]) -> Option<RunBuilder> {
    if body.len() != 10 {
        return None;
    }
    let (n, e0) = assign1(&body[0])?;
    if getvar(e0)? != "1" {
        return None;
    }
    let (p, e1) = assign1(&body[1])?;
    if getvar(e1)? != "2" {
        return None;
    }
    let (out, e2) = assign1(&body[2])?;
    if !matches!(e2, IrExpr::Str(s, _) if s.is_empty()) || out == n || out == p {
        return None;
    }
    // `if [ "$n" -le 0 ]; then return; fi`
    let IrStmt::If { cond, then, elsifs, else_ } = &body[3] else {
        return None;
    };
    if !elsifs.is_empty() || !else_.is_empty() {
        return None;
    }
    if !matches!(then.as_slice(), [IrStmt::Return(None)]) {
        return None;
    }
    if test_tokens(cond)?.as_slice()
        != [format!("${n}"), "-le".to_string(), "0".to_string()]
    {
        return None;
    }
    // the 8x self-doubling buffer
    let (blk, e4) = assign1(&body[4])?;
    if getvar(e4)? != p || blk == out {
        return None;
    }
    // the 8x self-doubling counter is the same variable all three loops use
    let Some(i) = loop_matches(&body[5], &Bound::Lit(8), blk, blk) else {
        return None;
    };
    // c = n / 256
    let (c, e6) = assign1(&body[6])?;
    if !is_div(e6, n, 256) {
        return None;
    }
    // while [ "$i" -lt "$c" ]; do out="${out}${blk}"; i=$((i+1)); done
    if loop_matches(&body[7], &Bound::Var(c.to_string()), out, blk).as_deref() != Some(i.as_str()) {
        return None;
    }
    // rem = n - c*256
    let (rem, e8) = assign1(&body[8])?;
    if !is_rem(e8, n, c, 256) {
        return None;
    }
    // while [ "$i" -lt "$rem" ]; do out="${out}${p}"; i=$((i+1)); done
    if loop_matches(&body[9], &Bound::Var(rem.to_string()), out, p).as_deref() != Some(i.as_str()) {
        return None;
    }
    // every name distinct where it must be (a self-aliasing template is not
    // the repeat it looks like)
    let mut names = vec![n, p, out, blk, c, rem, i.as_str()];
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    if names.len() != before {
        return None;
    }
    Some(RunBuilder {
        out: out.to_string(),
        temps: vec![
            n.to_string(),
            p.to_string(),
            blk.to_string(),
            c.to_string(),
            rem.to_string(),
            i,
        ],
    })
}

/// ── the census ────────────────────────────────────────────────────────

/// Is `name` mentioned anywhere in `stmts` outside the body of the function
/// called `skip`? Conservative: an opaque node counts as a mention.
fn mentioned_outside(stmts: &[IrStmt], skip: &str, name: &str) -> bool {
    stmts
        .iter()
        .any(|st| stmt_mentions_outside(st, skip, name))
}

fn stmt_mentions_outside(st: &IrStmt, skip: &str, name: &str) -> bool {
    match st {
        // the builder itself is the definition site of its temps: its own
        // body is exactly what the census must NOT count.
        IrStmt::Function { name: fname, .. } if fname == skip => false,
        // any OTHER function body is ordinary code: it observes globals at
        // call time, so a temp it reads keeps the temp live.
        IrStmt::Function { body, .. } => {
            body.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::Ext(_) => true, // opaque
        IrStmt::RawText(t) => str_mentions(t, name),
        IrStmt::Asm { template, outputs, inputs, clobbers, .. } => {
            str_mentions(template, name)
                || outputs.iter().any(|(o, _)| str_mentions(o, name))
                || inputs.iter().any(|(i, _)| str_mentions(i, name))
                || clobbers.iter().any(|c| str_mentions(c, name))
        }
        IrStmt::Assign { targets, expr, asm } => {
            targets.iter().any(|t| {
                t.var == name || t.var.starts_with(&format!("{name}["))
            }) || expr_mentions(expr, name)
                || asm.as_ref().is_some_and(|a| str_mentions(&format!("{a:?}"), name))
        }
        IrStmt::Output { value, target, .. } => {
            expr_mentions(value, name) || target.as_deref() == Some(name)
        }
        IrStmt::WriteFile { path, content, .. } => {
            expr_mentions(path, name) || expr_mentions(content, name)
        }
        IrStmt::Declare { vars, init, .. } => {
            vars.iter().any(|d| d.name == name) || init.as_ref().is_some_and(|e| expr_mentions(e, name))
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            var == name || elements.iter().any(|e| expr_mentions(e, name))
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_mentions(cond, name)
                || then.iter().any(|s| stmt_mentions_outside(s, "", name))
                || elsifs.iter().any(|(c, b)| {
                    expr_mentions(c, name) || b.iter().any(|s| stmt_mentions_outside(s, "", name))
                })
                || else_.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::While { cond, body } => {
            expr_mentions(cond, name) || body.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            expr_mentions(cond, name) || body.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::For { var, iter, body } => {
            var == name
                || expr_mentions(iter, name)
                || body.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::ForInit { init, cond, step, body } => {
            init.iter().any(|s| stmt_mentions_outside(s, "", name))
                || expr_mentions(cond, name)
                || step.iter().any(|s| stmt_mentions_outside(s, "", name))
                || body.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::Case { discriminant, clauses } => {
            expr_mentions(discriminant, name)
                || clauses.iter().any(|c| c.body.iter().any(|s| stmt_mentions_outside(s, "", name)))
        }
        IrStmt::Subshell(b) | IrStmt::Block(b) | IrStmt::Background(b) => {
            b.iter().any(|s| stmt_mentions_outside(s, "", name))
        }
        IrStmt::Redirect { inner, redirects } => {
            inner.iter().any(|s| stmt_mentions_outside(s, "", name))
                || redirects.iter().any(|r| expr_mentions(&r.target, name))
        }
        IrStmt::Expr(e) => expr_mentions(e, name),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::Die { expr: e, .. } => {
            expr_mentions(e, name)
        }
        IrStmt::Warn { expr, .. } => expr_mentions(expr, name),
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().any(|s| s.iter().any(|x| stmt_mentions_outside(x, "", name)))
        }
        // Statements with no expression surface at all.
        IrStmt::Break
        | IrStmt::Continue
        | IrStmt::Return(None)
        | IrStmt::Exit(None)
        | IrStmt::Goto(_)
        | IrStmt::Label(_) => false,
        // REFUSE > GUESS: any statement shape not classified above may hide
        // a read of the temp, so it counts as a mention.
        _ => true,
    }
}

fn expr_mentions(e: &IrExpr, name: &str) -> bool {
    // EXHAUSTIVE by construction: a future IrExpr variant must be given a
    // verdict here rather than silently inheriting one. (The first cut had a
    // `_ => true` fallback and every MIMEcroft statement tripped it, which
    // refused the lift it was meant to allow.)
    use IrExpr::*;
    match e {
        Int(_) | Bool(_) | Range { .. } => false,
        Str(s, _) | RawExpr(s) => str_mentions(s, name),
        Var(v, _) | Ident(v) => v == name,
        Index { var, key } => var == name || expr_mentions(key, name),
        BinOp { lhs, rhs, .. } => expr_mentions(lhs, name) || expr_mentions(rhs, name),
        Call { func, args } => func == name || args.iter().any(|a| expr_mentions(a, name)),
        MethodCall { obj, method, args } => {
            method == name
                || expr_mentions(obj, name)
                || args.iter().any(|a| expr_mentions(a, name))
        }
        Ternary { cond, then, else_ } => {
            expr_mentions(cond, name) || expr_mentions(then, name) || expr_mentions(else_, name)
        }
        DefinedOr { expr, default } => expr_mentions(expr, name) || expr_mentions(default, name),
        Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(x) => expr_mentions(x, name),
        }),
        Capture { expr, .. } => expr_mentions(expr, name),
        Regex { pattern, flags } => str_mentions(pattern, name) || str_mentions(flags, name),
        Arrow(body) | Lambda { body, .. } => {
            body.iter().any(|st| stmt_mentions_outside(st, "", name))
        }
        Array(items) => items.iter().any(|a| expr_mentions(a, name)),
        ArrayComp { var, iter, elem, cond } => {
            var == name
                || expr_mentions(iter, name)
                || expr_mentions(elem, name)
                || cond.as_ref().is_some_and(|c| expr_mentions(c, name))
        }
        Json(v) => str_mentions(&v.to_string(), name),
        Object(pairs) => pairs
            .iter()
            .any(|(k, v)| str_mentions(k, name) || expr_mentions(v, name)),
        Splice(x) => expr_mentions(x, name),
        Arith(a) => arith_mentions(a, name),
        Ext(n) => n.children().iter().any(|e| expr_mentions(e, name)),
    }
}

fn arith_mentions(a: &ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Num(_) => false,
        ArithAst::Var(v) | ArithAst::Ident(v) => v == name,
        ArithAst::Index { var, key } => var == name || arith_mentions(key, name),
        ArithAst::Bin { lhs, rhs, .. } => arith_mentions(lhs, name) || arith_mentions(rhs, name),
        ArithAst::Un { arg, .. } => arith_mentions(arg, name),
        ArithAst::Cond { test, then, else_ } => {
            arith_mentions(test, name) || arith_mentions(then, name) || arith_mentions(else_, name)
        }
        _ => true,
    }
}

/// A `$name` / `${name}` / bare-word occurrence with identifier boundaries.
fn str_mentions(s: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let b = s.as_bytes();
    let n = name.as_bytes();
    let mut i = 0;
    while let Some(pos) = s[i..].find(name) {
        let at = i + pos;
        let before_ok = at == 0 || {
            let c = b[at - 1] as char;
            !(c.is_ascii_alphanumeric() || c == '_')
        };
        let after = at + n.len();
        let after_ok = after >= b.len() || {
            let c = b[after] as char;
            !(c.is_ascii_alphanumeric() || c == '_')
        };
        if before_ok && after_ok {
            return true;
        }
        i = at + 1;
    }
    false
}

/// ── the rewrite ───────────────────────────────────────────────────────

fn walk(stmts: &mut Vec<IrStmt>, builders: &[(String, RunBuilder)], changed: &mut bool) {
    for st in stmts.iter_mut() {
        if let Some(b) = call_target(st, builders) {
            // the call statement becomes the store the builder performed
            *st = IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: b.1.out.clone(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Ext(Box::new(RepeatStr {
                    // `UNIT` first, `N` second: RepeatStr(text, count)
                    text: b.0.1.clone(),
                    count: b.0.0.clone(),
                })),
                asm: None,
            };
            *changed = true;
            continue;
        }
        for body in bodies_mut(st) {
            walk(body, builders, changed);
        }
        if let IrStmt::Pipeline { stages, .. } = st {
            for s in stages.iter_mut() {
                walk(s, builders, changed);
            }
        }
    }
}

/// The `(count, unit)` words of a call of a verified builder, plus the
/// builder itself.
fn call_target<'a>(
    st: &IrStmt,
    builders: &'a [(String, RunBuilder)],
) -> Option<((IrExpr, IrExpr), &'a RunBuilder)> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    let [count, unit] = words.as_slice() else {
        return None;
    };
    let b = builders.iter().find(|(n, _)| n == name)?;
    Some(((count.clone(), unit.clone()), &b.1))
}

fn bodies_mut(st: &mut IrStmt) -> Vec<&mut Vec<IrStmt>> {
    let mut v = Vec::new();
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            v.push(then);
            for (_, b) in elsifs {
                v.push(b);
            }
            v.push(else_);
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            v.push(body);
            for e in excepts {
                v.push(&mut e.body);
            }
            v.push(else_body);
            v.push(finally_body);
        }
        IrStmt::Function { body, .. } => v.push(body),
        IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. } => v.push(body),
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                v.push(&mut c.body);
            }
        }
        IrStmt::Select { clauses } => {
            for c in clauses {
                v.push(&mut c.body);
            }
        }
        IrStmt::Subshell(b)
        | IrStmt::Block(b)
        | IrStmt::Background(b)
        | IrStmt::Redirect { inner: b, .. } => v.push(b),
        _ => {}
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrStmt;

    fn s(t: &str) -> IrExpr {
        IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted)
    }
    fn gv(n: &str) -> IrExpr {
        IrExpr::Call { func: "getVar".into(), args: vec![s(n)] }
    }
    fn prm(n: &str) -> IrExpr {
        IrExpr::Call { func: "param".into(), args: vec![s(""), s(n)] }
    }
    fn asg(var: &str, e: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget { var: var.into(), sigil: None, indices: vec![] }],
            expr: e,
            asm: None,
        }
    }
    fn test(t: &str) -> IrExpr {
        IrExpr::Call { func: "test".into(), args: vec![s(t)] }
    }
    fn arith(a: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(a))
    }
    fn var(n: &str) -> ArithAst {
        ArithAst::Var(n.into())
    }
    fn bin(op: &str, l: ArithAst, r: ArithAst) -> ArithAst {
        ArithAst::Bin { op: op.into(), lhs: Box::new(l), rhs: Box::new(r) }
    }
    fn interp(parts: Vec<IrExpr>) -> IrExpr {
        IrExpr::Interpolate(
            parts
                .into_iter()
                .map(|p| InterpPart::Expr(Box::new(p)))
                .collect(),
        )
    }
    fn count_loop(cond: &str, acc_a: &str, acc_b: &str, i: &str) -> IrStmt {
        IrStmt::While {
            cond: test(cond),
            body: vec![
                asg(acc_a, interp(vec![prm(acc_a), prm(acc_b)])),
                asg(i, arith(bin("+", var(i), ArithAst::Num(1)))),
            ],
        }
    }

    /// The closed template as MIMEcroft's `rb_mkrun` dumps (`--dump-shir`).
    pub(crate) fn mkrun_body() -> Vec<IrStmt> {
        vec![
            asg("rbm_n", gv("1")),
            asg("rbm_p", gv("2")),
            asg("rb_run", s("")),
            IrStmt::If {
                cond: test("\"$rbm_n\" -le 0"),
                then: vec![IrStmt::Return(None)],
                elsifs: vec![],
                else_: vec![],
            },
            asg("rbm_blk", gv("rbm_p")),
            IrStmt::While {
                cond: test("\"$rbm_i\" -lt 8"),
                body: vec![
                    asg("rbm_blk", interp(vec![prm("rbm_blk"), prm("rbm_blk")])),
                    asg("rbm_i", arith(bin("+", var("rbm_i"), ArithAst::Num(1)))),
                ],
            },
            asg("rbm_c", arith(bin("/", var("rbm_n"), ArithAst::Num(256)))),
            count_loop("\"$rbm_i\" -lt \"$rbm_c\"", "rb_run", "rbm_blk", "rbm_i"),
            asg(
                "rbm_rem",
                arith(bin(
                    "-",
                    var("rbm_n"),
                    bin("*", var("rbm_c"), ArithAst::Num(256)),
                )),
            ),
            count_loop("\"$rbm_i\" -lt \"$rbm_rem\"", "rb_run", "rbm_p", "rbm_i"),
        ]
    }

    fn mkrun_fn() -> IrStmt {
        IrStmt::Function {
            name: "rb_mkrun".into(),
            params: vec![],
            body: mkrun_body(),
            named_blocks: vec![],
        }
    }

    fn call(f: &str, n: &str, unit: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                s(f),
                IrExpr::Array(vec![
                    IrExpr::Call { func: "split".into(), args: vec![gv(n)] },
                    gv(unit),
                ]),
            ],
        })
    }

    fn repeat_of(st: &IrStmt) -> Option<(String, IrExpr, IrExpr)> {
        let IrStmt::Assign { targets, expr, .. } = st else { return None };
        let IrExpr::Ext(n) = expr else { return None };
        let r = n.as_any().downcast_ref::<RepeatStr>()?;
        Some((targets[0].var.clone(), r.text.clone(), r.count.clone()))
    }

    #[test]
    fn lifts_the_run_build_to_a_repeat_node() {
        let mut stmts = vec![mkrun_fn(), IrStmt::Function {
            name: "rb_putrun".into(),
            params: vec![],
            body: vec![call("rb_mkrun", "rbp_n", "rbp_p")],
            named_blocks: vec![],
        }];
        assert!(transform(&mut stmts), "the call statement should lift");
        let IrStmt::Function { body, .. } = &stmts[1] else { panic!() };
        let (out, text, count) = repeat_of(&body[0]).expect("a RepeatStr store");
        assert_eq!(out, "rb_run", "the builder's output var");
        assert_eq!(text, gv("rbp_p"), "unit carried verbatim");
        assert_eq!(count, IrExpr::Call { func: "split".into(), args: vec![gv("rbp_n")] });
    }

    #[test]
    fn refuses_a_different_block_size() {
        // /255 instead of /256 changes the remainder — not a repeat
        let mut body = mkrun_body();
        body[6] = asg("rbm_c", arith(bin("/", var("rbm_n"), ArithAst::Num(255))));
        assert!(verify_builder(&body).is_none());
    }

    #[test]
    fn refuses_a_seven_step_doubling() {
        let mut body = mkrun_body();
        if let IrStmt::While { cond, .. } = &mut body[5] {
            *cond = test("\"$rbm_i\" -lt 7");
        }
        assert!(verify_builder(&body).is_none());
    }

    #[test]
    fn refuses_an_extra_statement() {
        let mut body = mkrun_body();
        body.push(asg("rbm_extra", s("x")));
        assert!(verify_builder(&body).is_none());
    }

    #[test]
    fn refuses_a_temp_read_outside_the_builder() {
        // rbm_p is a temp of rb_mkrun; reading it elsewhere means the body's
        // other assignments are observable, so the rewrite is not safe.
        let mut stmts = vec![
            mkrun_fn(),
            IrStmt::Function {
                name: "other".into(),
                params: vec![],
                body: vec![IrStmt::Expr(IrExpr::Call {
                    func: "builtin".into(),
                    args: vec![s("echo"), IrExpr::Array(vec![gv("rbm_p")])],
                })],
                named_blocks: vec![],
            },
            call("rb_mkrun", "rbp_n", "rbp_p"),
        ];
        assert!(!transform(&mut stmts), "a live temp must refuse the lift");
        assert!(repeat_of(&stmts[2]).is_none());
    }

    #[test]
    fn refuses_a_redefined_builder() {
        let mut stmts = vec![
            mkrun_fn(),
            mkrun_fn(),
            call("rb_mkrun", "rbp_n", "rbp_p"),
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn refuses_a_capture_site() {
        // `x=$(rb_mkrun 3 px)` is direct-calls' shape, not this pass's.
        let mut stmts = vec![mkrun_fn(), asg("x", IrExpr::Capture {
            expr: Box::new(IrExpr::Call {
                func: "rb_mkrun".into(),
                args: vec![IrExpr::Array(vec![s("3"), s("AABBCC00")])],
            }),
            native: false,
        })];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn is_idempotent() {
        let mut stmts = vec![mkrun_fn(), call("rb_mkrun", "rbp_n", "rbp_p")];
        assert!(transform(&mut stmts));
        assert!(!transform(&mut stmts), "an Assign is not a call statement");
    }

    /// The structured loop `for-recovery` builds from
    /// `i=0; while [ "$i" -lt BOUND ]; do …; i=$((i+1)); done` — the shape
    /// the real pipeline hands this pass (the raw `while` never survives to
    /// it). `--dump-shir` on the rasterizer shows ForInit at body[5/7/9].
    fn forinit_loop(bound: ArithAst, acc: &str, part: &str, i: &str) -> IrStmt {
        IrStmt::ForInit {
            init: vec![asg(i, s("0"))],
            cond: IrExpr::BinOp {
                lhs: Box::new(arith(var(i))),
                op: crate::ir::BinOpKind::Lt,
                rhs: Box::new(arith(bound)),
            },
            step: vec![asg(i, arith(bin("+", var(i), ArithAst::Num(1))))],
            body: vec![asg(acc, interp(vec![prm(acc), prm(part)]))],
        }
    }

    #[test]
    fn accepts_the_forinit_loop_form() {
        let mut body = mkrun_body();
        body[5] = IrStmt::ForInit {
            init: vec![asg("rbm_i", s("0"))],
            cond: match &body[5] {
                IrStmt::While { cond, .. } => {
                    // the shell test → the structured BinOp the pipeline mints
                    let _ = cond;
                    IrExpr::BinOp {
                        lhs: Box::new(arith(var("rbm_i"))),
                        op: crate::ir::BinOpKind::Lt,
                        rhs: Box::new(arith(ArithAst::Num(8))),
                    }
                }
                _ => panic!(),
            },
            step: vec![asg("rbm_i", arith(bin("+", var("rbm_i"), ArithAst::Num(1))))],
            body: vec![asg("rbm_blk", interp(vec![prm("rbm_blk"), prm("rbm_blk")]))],
        };
        body[7] = forinit_loop(var("rbm_c"), "rb_run", "rbm_blk", "rbm_i");
        body[9] = forinit_loop(var("rbm_rem"), "rb_run", "rbm_p", "rbm_i");
        assert!(verify_builder(&body).is_some(), "the ForInit form is the real shape");
        let mut stmts = vec![
            IrStmt::Function {
                name: "rb_mkrun".into(),
                params: vec![],
                body,
                named_blocks: vec![],
            },
            call("rb_mkrun", "rbp_n", "rbp_p"),
        ];
        assert!(transform(&mut stmts));
        assert!(repeat_of(&stmts[1]).is_some());
    }

    #[test]
    fn gated_to_the_c_target() {
        use crate::transforms::{set_target, transform_applies};
        set_target(Some("c"));
        assert!(transform_applies("fbdev-gpu-hexrun"));
        set_target(Some("perl"));
        assert!(!transform_applies("fbdev-gpu-hexrun"));
        set_target(None);
        assert!(!transform_applies("fbdev-gpu-hexrun"));
    }
}
