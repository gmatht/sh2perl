//! string-accumulator — identify the `v="$v$seg"` self-append pattern in
//! a loop and mark it the renderers can lower to a chunk-array + join
//! instead of re-concatenating a growing string, the shIR equivalent of
//! the stalled proposal estree-20260813-184909 (string-accumulator).
//!
//! ## Need
//! The game's render path builds the block list this way (4 A1
//! self-append sites; the two in `try_draw`/`draw_block` run 768×/
//! frame):
//!
//! ```text
//! blk_p=""
//! while …; do
//!   blk_p="$blk_p$td_a $td_b $td_c 1 1 1 …\n"
//!   …
//! done
//! echo "$blk_p" > /dev/webgl/…
//! ```
//!
//! The emitted code re-evaluates a growing template literal per append
//! (`blk_p = \`${blk_p}${td_a} …\n\``) — ~1500 appends/frame growing to
//! ~30KB. Analysis-only (like `sync-ok-loops`): the transform marks the
//! accumulator and the renderer lowers it — `__acc_v.push(seg)` per
//! append and ONE `__acc_v.join("")` per post-loop read.
//!
//! ## The verdict
//! A scalar var `v` is an ACCUMULATOR when ALL of:
//!   - it is initialized to a literal (usually `""`) before a loop,
//!   - every write inside the loop is a SELF-APPEND: `v` assigned an
//!     `Interpolate` that begins with `Var(v)` (the `"$v$seg"` shape),
//!   - `v` is not written outside the loop after the init, and
//!   - every read of `v` is AFTER the loop (its final value), except the
//!     self-append's own leading read.
//! The transform stores the verdict (var, the init statement index, the
//! loop statement index, the post-loop read indexes) in module statics;
//! the renderers emit the push/join lowering. `newline`/interpolation of
//! each `seg` is preserved exactly.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Analysis
//! only, no structural mutation. Renderer hooks: estree.rs (the shrinking
//! `blk_p` cons-chain), and any backend with a mutable string builder.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ir::{AssignTarget, InterpPart, IrExpr, IrStmt, StrStyle};

#[derive(Debug, Clone, PartialEq)]
pub struct Accumulator {
    pub var: String,
    /// index into the top-level statement list of the loop stmt
    pub loop_idx: usize,
    /// index of the init statement (before the loop)
    pub init_idx: usize,
}

static VERDICTS: Mutex<Option<Vec<Accumulator>>> = Mutex::new(None);

/// Drop the cached verdict — the out-of-band caller's reset (see
/// `transforms::reset_verdicts`). These transforms are analysis-only: they
/// return `false` from `run()` and a RENDERER reads the verdict, so without
/// a reset a second run observes the PREVIOUS input's verdict.
pub fn clear_verdicts() {
    *VERDICTS.lock().unwrap() = None;
}

pub fn accumulators() -> Vec<Accumulator> {
    VERDICTS.lock().unwrap().clone().unwrap_or_default()
}

/// Apply the transform: compute + cache the verdicts, then (for the JS /
/// ESTree renderer only, which has the `concatItems` arm) structurally
/// lower the verifier's shape to a chunk array:
///
/// ```text
/// v="";  while …; do v="$v$seg"; …; done;  echo "$v"
///   ↓
/// v="";  setArray("__acc_v", []);  while …; do setArrayAppend("__acc_v", [seg]); …; done;
/// v = concatItems("__acc_v");  echo "$v"
/// ```
///
/// Materializing ONCE right after the loop (instead of rewriting every
/// read) is what keeps the lowering sound with no read-set walk: every
/// post-loop read already sees `v`, and a pre-loop read still sees the
/// init value because nothing before the loop changes. The two mid-loop
/// read positions (the loop CONDITION and the body) are guarded below.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let verdicts = analyze(stmts);
    *VERDICTS.lock().unwrap() = Some(verdicts.clone());
    if !matches!(crate::transforms::target().as_deref(), Some("js") | Some("estree")) {
        return false; // analysis-only for every other renderer
    }
    lower(stmts, &verdicts)
}

fn analyze(stmts: &[IrStmt]) -> Vec<Accumulator> {
    let mut out = Vec::new();
    for (li, st) in stmts.iter().enumerate() {
        let body = match st {
            IrStmt::While { body, .. } | IrStmt::For { body, .. } => body,
            _ => continue,
        };
        for var in self_append_vars(body) {
            // The init is the LAST statement before the loop that writes
            // `var` — NOT necessarily `stmts[li - 1]`. The canonical shape
            // interleaves other initialisers (`blk=""; i=0; while …`), so
            // requiring adjacency (as this analysis did) missed the very
            // case the request is about. Scanning back for the last write
            // keeps the "no intervening write" guarantee: a later
            // non-literal rebind is found FIRST and refused by the literal
            // check below.
            let Some(ii) = (0..li).rev().find(|&k| writes_var(&stmts[k], &var)) else {
                continue;
            };
            if !init_is_literal(&stmts[ii], &var) {
                continue;
            }
            // A mid-loop read observes the growing string — the deferred
            // push/join lowering would change its value. Require the body
            // to read the accumulator only as the self-append's leader.
            if body_reads_var_nonleading(body, &var) {
                continue;
            }
            // The loop CONDITION is a mid-loop read too (`while [ -n "$v" ]`
            // observes the growing string) — the body scan above cannot see
            // it.
            if loop_reads_var(st, &var) {
                continue;
            }
            if reads_after_loop(&stmts[li + 1..], &var) {
                out.push(Accumulator { var, loop_idx: li, init_idx: ii });
            }
        }
    }
    out
}

/// Does the loop's own header (the `while` condition / the `for` iterable)
/// read `var`? That read runs once per iteration against the growing value.
fn loop_reads_var(st: &IrStmt, var: &str) -> bool {
    let mut found = false;
    match st {
        IrStmt::While { cond, .. } => expr_read_walk(cond, var, &mut found),
        IrStmt::For { iter, .. } => expr_read_walk(iter, var, &mut found),
        _ => {}
    }
    found
}

/// The vars whose ONLY writes in this body are self-appends.
fn self_append_vars(body: &[IrStmt]) -> Vec<String> {
    let mut writes: Vec<String> = Vec::new();
    let mut bad: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in body {
        if let IrStmt::Assign { targets, expr, .. } = s {
            for t in targets {
                if t.indices.is_empty() && !writes.contains(&t.var) {
                    writes.push(t.var.clone());
                }
                if !is_self_append(t.var.as_str(), expr) {
                    bad.insert(t.var.clone());
                }
            }
        } else {
            // a non-assign statement reads/writes unknown things — mark
            // everything crystal-clear only (conservative: bail on any
            // other statement in the body)
            return Vec::new();
        }
    }
    writes.into_iter().filter(|v| !bad.contains(v)).collect()
}

/// Is this expression a READ of `var`? The real A1 carries reads three ways:
/// a bare `Var`/`Ident`, a `getVar("v")` call, or `param("", "v")` — the
/// last two hold the name as a STRING argument, so a walk that only matches
/// `IrExpr::Var` sees nothing at all (`echo "$blk"` lowers to
/// `getVar("blk")`).
fn is_var_read(e: &IrExpr, var: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == var,
        IrExpr::Call { func, args } => {
            let idx = if func == "param" { 1 } else { 0 };
            matches!(func.as_str(), "getVar" | "param")
                && matches!(args.get(idx), Some(IrExpr::Str(n, _)) if n == var)
        }
        _ => false,
    }
}

/// `v` assigned an Interpolate starting with a read of `v` (the `"$v$seg"`
/// shape).
fn is_self_append(var: &str, expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Interpolate(parts) => match parts.first() {
            Some(InterpPart::Expr(e)) => is_var_read(e, var),
            _ => false,
        },
        _ => false,
    }
}

/// Does this statement WRITE `var`? (assign target / declare / setVar)
fn writes_var(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => {
            targets.iter().any(|t| t.var == var && t.indices.is_empty())
        }
        IrStmt::Declare { vars, .. } => vars.iter().any(|d| d.name == var),
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "setVar" => {
            matches!(args.first(), Some(IrExpr::Str(n, _)) if n == var)
        }
        _ => false,
    }
}

/// Assigned to a CONSTANT string (the `blk_p=""` init). The parser lowers
/// `v=""` to `Interpolate([Lit("")])`, not `Str`, so both forms qualify —
/// an interpolation qualifies only when every part is a literal (no read at
/// all, so seeding a chunk array with it cannot change evaluation order).
fn init_is_literal(st: &IrStmt, var: &str) -> bool {
    let expr = match st {
        IrStmt::Assign { targets, expr, .. }
            if targets.iter().any(|t| t.var == var && t.indices.is_empty()) =>
        {
            expr
        }
        IrStmt::Declare { vars, init: Some(init), .. }
            if vars.iter().any(|d| d.name == var) =>
        {
            init
        }
        _ => return false,
    };
    match expr {
        IrExpr::Str(_, _) => true,
        IrExpr::Interpolate(parts) => parts.iter().all(|p| matches!(p, InterpPart::Lit(_))),
        _ => false,
    }
}

/// `var` is read after the loop (its final value is observed) — this is
/// what makes the join-at-read lowering valid (there are no mid-loop
/// reads besides the self-append's own leading `Var(v)`).
fn reads_after_loop(after: &[IrStmt], var: &str) -> bool {
    let mut found = false;
    for st in after {
        read_walk(st, var, &mut found);
    }
    found
}

fn read_walk(st: &IrStmt, var: &str, found: &mut bool) {
    if *found {
        return;
    }
    match st {
        IrStmt::Output { value, .. } => expr_read_walk(value, var, found),
        IrStmt::Assign { expr, .. } => expr_read_walk(expr, var, found),
        IrStmt::Exec { cmd, args, .. } => {
            expr_read_walk(cmd, var, found);
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        IrStmt::WriteFile { path, content, .. } => {
            expr_read_walk(path, var, found);
            expr_read_walk(content, var, found);
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_read_walk(cond, var, found);
            for s in then {
                read_walk(s, var, found);
            }
            for (c, b) in elsifs {
                expr_read_walk(c, var, found);
                for s in b {
                    read_walk(s, var, found);
                }
            }
            for s in else_ {
                read_walk(s, var, found);
            }
        }
        IrStmt::While { cond, body } => {
            expr_read_walk(cond, var, found);
            for s in body {
                read_walk(s, var, found);
            }
        }
        IrStmt::For { iter, body, .. } => {
            expr_read_walk(iter, var, found);
            for s in body {
                read_walk(s, var, found);
            }
        }
        IrStmt::Expr(e) => expr_read_walk(e, var, found),
        _ => {}
    }
}

/// A `$var` / `${var…}` reference (or a bare word) inside a command/test/
/// arith TEXT. The A1 keeps those as strings — `test("-n $blk")`, `arith` —
/// so an operand walk alone cannot see the read.
fn text_mentions_var(s: &str, var: &str) -> bool {
    let (bytes, vb) = (s.as_bytes(), var.as_bytes());
    if vb.is_empty() || vb.len() > bytes.len() {
        return false;
    }
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    for i in 0..=bytes.len() - vb.len() {
        if &bytes[i..i + vb.len()] == vb {
            let before_ok = i == 0 || !ident(bytes[i - 1]);
            let after = i + vb.len();
            let after_ok = after == bytes.len() || !ident(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

fn expr_read_walk(e: &IrExpr, var: &str, found: &mut bool) {
    if *found {
        return;
    }
    match e {
        IrExpr::Str(s, _) => {
            if text_mentions_var(s, var) {
                *found = true;
            }
        }
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            if v == var {
                *found = true;
            }
        }
        IrExpr::Index { var: v, key } => {
            if v == var {
                *found = true;
            }
            expr_read_walk(key, var, found);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_read_walk(lhs, var, found);
            expr_read_walk(rhs, var, found);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    expr_read_walk(x, var, found);
                }
            }
        }
        // A command's argv / an array literal / an object value carries the
        // operand in a CHILD list — walking only scalar operands missed
        // `echo "$blk"` (its read sits inside the `builtin` argv Array), so
        // the verdict never fired on the canonical shape.
        IrExpr::Array(items) => {
            for i in items {
                expr_read_walk(i, var, found);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                expr_read_walk(v, var, found);
            }
        }
        IrExpr::Splice(inner) => expr_read_walk(inner, var, found),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_read_walk(cond, var, found);
            expr_read_walk(then, var, found);
            expr_read_walk(else_, var, found);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_read_walk(expr, var, found);
            expr_read_walk(default, var, found);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_read_walk(obj, var, found);
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        IrExpr::Call { func, args } => {
            // `getVar("v")` / `param("", "v")` — the name is a STRING arg,
            // so the operand walk below cannot see the reference.
            let idx = if func == "param" { 1 } else { 0 };
            if matches!(func.as_str(), "getVar" | "param")
                && matches!(args.get(idx), Some(IrExpr::Str(n, _)) if n == var)
            {
                *found = true;
            }
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        _ => {}
    }
}

/// Is `var` read ANYWHERE in the loop body outside a self-append's leading
/// self-read? A mid-loop read observes the growing string, so deferring the
/// concatenation to a `join` at the post-loop read sites would change its
/// value — the lowering must be refused.
fn body_reads_var_nonleading(body: &[IrStmt], var: &str) -> bool {
    let mut found = false;
    for s in body {
        match s {
            IrStmt::Assign { targets, expr, .. }
                if targets.iter().any(|t| t.var == var) && is_self_append(var, expr) =>
            {
                if let IrExpr::Interpolate(parts) = expr {
                    for p in parts.iter().skip(1) {
                        if let InterpPart::Expr(e) = p {
                            expr_read_walk(e, var, &mut found);
                        }
                    }
                }
            }
            other => read_walk(other, var, &mut found),
        }
        if found {
            return true;
        }
    }
    false
}


/// The literal text of the accumulator's init (`v=""`). The analysis
/// already refused a non-literal init; the lowering re-derives the text
/// because a non-empty init must SEED the chunk array.
fn literal_init_text(st: &IrStmt, var: &str) -> Option<String> {
    let expr = match st {
        IrStmt::Assign { targets, expr, .. }
            if targets.iter().any(|t| t.var == var && t.indices.is_empty()) =>
        {
            expr
        }
        IrStmt::Declare { vars, init: Some(init), .. }
            if vars.iter().any(|d| d.name == var) =>
        {
            init
        }
        _ => return None,
    };
    match expr {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// The appended segment: the interpolation's parts AFTER the leading
/// self-read (`"$v$seg"` → `seg`).
fn appended_segment(parts: &[InterpPart]) -> IrExpr {
    let rest = parts.get(1..).unwrap_or(&[]);
    match rest {
        [] => IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
        [InterpPart::Lit(s)] => IrExpr::Str(s.clone(), StrStyle::DoubleQuoted),
        [InterpPart::Expr(e)] => (**e).clone(),
        _ => IrExpr::Interpolate(rest.to_vec()),
    }
}

fn chunk_assign(target: &str, expr: IrExpr) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: target.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr,
        asm: None,
    }
}

fn array_call(func: &str, name: &str, items: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        func: func.to_string(),
        args: vec![
            IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(items),
        ],
    }
}

fn loop_body_mut(st: &mut IrStmt) -> Option<&mut Vec<IrStmt>> {
    match st {
        IrStmt::While { body, .. } | IrStmt::For { body, .. } => Some(body),
        _ => None,
    }
}

/// The structural lowering (JS/ESTree only): chunk init after the `v` init,
/// `setArrayAppend` for each self-append, one `concatItems` materialization
/// immediately after the loop.
fn lower(stmts: &mut Vec<IrStmt>, accs: &[Accumulator]) -> bool {
    // init_idx -> [(chunk name, seed text)]; loop_idx -> [(var, chunk name)]
    let mut seeds: HashMap<usize, Vec<(String, String)>> = HashMap::new();
    let mut at_loop: HashMap<usize, Vec<(String, String)>> = HashMap::new();
    for a in accs {
        // A reserved/special var (IFS, PATH, …) is read implicitly by the
        // runtime between iterations; its growing value must stay visible.
        if crate::shir::is_reserved_var(&a.var) {
            continue;
        }
        let Some(seed) = literal_init_text(&stmts[a.init_idx], &a.var) else {
            continue;
        };
        let acc = format!("__acc_{}", a.var);
        seeds.entry(a.init_idx).or_default().push((acc.clone(), seed));
        at_loop
            .entry(a.loop_idx)
            .or_default()
            .push((a.var.clone(), acc));
    }
    if at_loop.is_empty() {
        return false;
    }
    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len() + at_loop.len() * 2);
    let mut changed = false;
    for (i, mut st) in stmts.drain(..).enumerate() {
        if let Some(names) = at_loop.get(&i) {
            if let Some(body) = loop_body_mut(&mut st) {
                for (var, acc) in names {
                    for b in body.iter_mut() {
                        let seg = match b {
                            IrStmt::Assign { targets, expr, .. }
                                if targets.len() == 1
                                    && targets[0].var == *var
                                    && targets[0].indices.is_empty()
                                    && is_self_append(var, expr) =>
                            {
                                match expr {
                                    IrExpr::Interpolate(parts) => Some(appended_segment(parts)),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        if let Some(seg) = seg {
                            *b = chunk_assign(acc, array_call("setArrayAppend", acc, vec![seg]));
                            changed = true;
                        }
                    }
                }
            }
        }
        out.push(st);
        if let Some(list) = seeds.get(&i) {
            for (acc, seed) in list {
                let items = if seed.is_empty() {
                    vec![]
                } else {
                    vec![IrExpr::Str(seed.clone(), StrStyle::DoubleQuoted)]
                };
                out.push(chunk_assign(acc, array_call("setArray", acc, items)));
            }
        }
        if let Some(names) = at_loop.get(&i) {
            for (var, acc) in names {
                out.push(chunk_assign(
                    var,
                    IrExpr::Call {
                        func: "concatItems".to_string(),
                        args: vec![IrExpr::Str(acc.clone(), StrStyle::DoubleQuoted)],
                    },
                ));
                changed = true;
            }
        }
    }
    *stmts = out;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrExpr, IrStmt, StrStyle};

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn get(var: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![IrExpr::Str(var.to_string(), StrStyle::DoubleQuoted)],
        }
    }

    fn echo(var: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![get(var)]),
            ],
        })
    }

    /// The request's own repro — `blk=""; i=0; while …; done; echo "$blk"` —
    /// must be recognized. Every shape here is the one the REAL A1 carries:
    /// the init is an `Interpolate` of literals (not a bare `Str`), the
    /// self-read is `getVar("blk")` (not `Var`), the init is not adjacent to
    /// the loop, and the post-loop read is another `getVar`.
    #[test]
    fn recognizes_canonical_accumulator() {
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let i_init = assign("i", IrExpr::Str("0".into(), StrStyle::DoubleQuoted));
        let body = vec![
            assign(
                "blk",
                IrExpr::Interpolate(vec![
                    InterpPart::Expr(Box::new(get("blk"))),
                    InterpPart::Lit(" x ".into()),
                    InterpPart::Expr(Box::new(get("i"))),
                ]),
            ),
            assign("i", IrExpr::Str("1".into(), StrStyle::DoubleQuoted)),
        ];
        let mut stmts = vec![
            init,
            i_init,
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        let acc = accumulators();
        assert_eq!(
            acc,
            vec![Accumulator {
                var: "blk".to_string(),
                loop_idx: 2,
                init_idx: 0,
            }],
            "canonical self-append accumulator must be recognized"
        );
    }

    /// A mid-loop read observes the growing string, so the verdict must be
    /// refused (a deferred join would return a different value).
    #[test]
    fn refuses_mid_loop_read() {
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let body = vec![
            echo("blk"),
            assign(
                "blk",
                IrExpr::Interpolate(vec![
                    InterpPart::Expr(Box::new(get("blk"))),
                    InterpPart::Lit("x".into()),
                ]),
            ),
        ];
        let mut stmts = vec![
            init,
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        assert!(
            accumulators().is_empty(),
            "a mid-loop read must refuse the verdict"
        );
    }

    /// A read in the loop HEADER observes the growing value just like a
    /// body read, so it must refuse the verdict.
    #[test]
    fn refuses_loop_condition_read() {
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let body = vec![assign(
            "blk",
            IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(get("blk"))),
                InterpPart::Lit("x".into()),
            ]),
        )];
        let mut stmts = vec![
            init,
            IrStmt::While {
                cond: IrExpr::Call {
                    func: "test".to_string(),
                    args: vec![IrExpr::Str("-n $blk".into(), StrStyle::DoubleQuoted)],
                },
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        assert!(
            accumulators().is_empty(),
            "a loop-condition read must refuse the verdict"
        );
    }

    /// Find a statement by shape: `Assign` whose single target is `target`
    /// and whose expr is a call to `func` (searching nested loop bodies).
    fn has_call(stmts: &[IrStmt], target: &str, func: &str) -> bool {
        stmts.iter().any(|s| match s {
            IrStmt::Assign { targets, expr, .. }
                if targets.len() == 1 && targets[0].var == target =>
            {
                matches!(expr, IrExpr::Call { func: f, .. } if f == func)
            }
            IrStmt::While { body, .. } | IrStmt::For { body, .. } => {
                has_call(body, target, func)
            }
            IrStmt::If { then, else_, .. } => {
                has_call(then, target, func) || has_call(else_, target, func)
            }
            _ => false,
        })
    }

    /// The canonical shape lowers to `setArray` (chunk init) + a body
    /// `setArrayAppend` + ONE post-loop `concatItems` materialization, and
    /// leaves no self-append assignment behind.
    #[test]
    fn lowers_self_append_to_chunks() {
        crate::transforms::set_target(Some("js"));
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let i_init = assign("i", IrExpr::Str("0".into(), StrStyle::DoubleQuoted));
        let loop_body = vec![
            assign(
                "blk",
                IrExpr::Interpolate(vec![
                    InterpPart::Expr(Box::new(get("blk"))),
                    InterpPart::Lit(" x ".into()),
                    InterpPart::Expr(Box::new(get("i"))),
                ]),
            ),
            assign("i", IrExpr::Str("1".into(), StrStyle::DoubleQuoted)),
        ];
        let mut stmts = vec![
            init,
            i_init,
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body: loop_body,
            },
            echo("blk"),
        ];
        assert!(transform(&mut stmts), "the lowering must report a change");
        assert!(
            has_call(&stmts, "__acc_blk", "setArray"),
            "chunk init after the v init: {stmts:?}"
        );
        assert!(
            has_call(&stmts, "__acc_blk", "setArrayAppend"),
            "the self-append becomes a chunk push: {stmts:?}"
        );
        assert!(
            has_call(&stmts, "blk", "concatItems"),
            "one post-loop materialization: {stmts:?}"
        );
        // The materialization must sit AFTER the loop, not before it.
        let loop_at = stmts
            .iter()
            .position(|s| matches!(s, IrStmt::While { .. }))
            .expect("loop kept");
        let mat_at = stmts
            .iter()
            .position(|s| match s {
                IrStmt::Assign { targets, expr, .. } => {
                    targets.len() == 1
                        && targets[0].var == "blk"
                        && matches!(expr, IrExpr::Call { func, .. } if func == "concatItems")
                }
                _ => false,
            })
            .expect("materialization emitted");
        assert!(mat_at > loop_at, "materialization must follow the loop");
        // The growing-template assignment must be gone.
        assert!(
            !stmts.iter().any(|s| match s {
                IrStmt::Assign { targets, expr, .. } =>
                    targets.iter().any(|t| t.var == "blk") && is_self_append("blk", expr),
                _ => false,
            }),
            "no self-append assignment survives: {stmts:?}"
        );
        crate::transforms::set_target(None);
    }

    /// A NON-JS renderer keeps the analysis verdict but never sees the
    /// chunk lowering (its `concatItems` op has no arm).
    #[test]
    fn does_not_lower_for_perl() {
        crate::transforms::set_target(Some("perl"));
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let body = vec![assign(
            "blk",
            IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(get("blk"))),
                InterpPart::Lit("x".into()),
            ]),
        )];
        let mut stmts = vec![
            init,
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        let before = format!("{stmts:?}");
        assert!(!transform(&mut stmts), "perl target must not lower");
        assert_eq!(format!("{stmts:?}"), before, "perl shape is unchanged");
        assert_eq!(accumulators().len(), 1, "the verdict still fires");
        crate::transforms::set_target(None);
    }

    /// A non-literal re-init before the loop poisons the verdict.
    #[test]
    fn refuses_nonliteral_init() {
        let body = vec![assign(
            "blk",
            IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(get("blk"))),
                InterpPart::Lit("x".into()),
            ]),
        )];
        let mut stmts = vec![
            assign("blk", get("seed")),
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        assert!(accumulators().is_empty(), "non-literal init must refuse");
    }
}
