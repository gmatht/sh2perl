//! loop-opt — shared A1 loop-invariant statement hoisting (LICM-lite).
//!
//! Core requests:
//! - estree-20260813-182436-loop-index-srr — the loop-INVARIANT
//!   HOISTING arm: within every `While`/`For`/`DoWhile` body, hoist the
//!   leading maximal run of pure, loop-invariant statements into the
//!   loop PROLOGUE, so the emitted code stops re-deriving the same
//!   products/arith every iteration (`idx = b*CELLS + …` recomputed per
//!   cell). The index-accumulator STRENGTH-REDUCTION half is left to the
//!   per-backend renderer (offered in the manifest; the hoist alone is
//!   value-preserving and renderer-agnostic).
//! - estree-20260813-201235-hoist-pure-loop-invariants — the PURE-CALL
//!   arm: a call to a provably-pure user function (`lat_hash $x 0 $SIZE
//!   1`) plus the statements derived ONLY from its outputs
//!   (`gph`/`stripe`/`coff`), when every operand is loop-invariant in
//!   the INNER loop (they reference the OUTER loop counter), hoists to
//!   the inner loop's prologue — the column hash runs once per column
//!   instead of once per pixel. `rand`-style stateful functions are
//!   Impure and never hoisted; an `echo` (observable output) is not
//!   pure and never hoisted; a loop reading its own counter is not
//!   invariant and never hoisted.
//!
//! ## Soundness (each guard is a corpus-regression trap)
//! 1. **Leading run only**: the hoisted statements sit at the TOP of the
//!    loop body, so every iteration that enters the body executes them
//!    first (a `continue` re-enters from the top — the group still runs;
//!    a `break` cannot precede the top).
//! 2. **No control escape**: a body containing `Break`/`Return`/`Exit`/
//!    `Goto`/`Die`/`Warn`/`Label` anywhere blocks the hoist (a skip
//!    would change the post-loop values).
//! 3. **Target clobber**: a hoisted statement's targets must not be
//!    written by ANY body statement outside the group — the write sets
//!    of pure-function `exec` calls count (a function's outputs are
//!    writes at the call site), so a mid-body store to a group output
//!    blocks the hoist.
//! 4. **Operand invariance**: every variable READ (including a pure
//!    function's read set) must be either not assigned anywhere in the
//!    body, or assigned only by an EARLIER group member (whose post-group
//!    value is the group's constant).
//! 5. **Status**: if the group contains an `exec`/call statement and the
//!    remaining body reads `$?`/lastExit, the hoist is blocked.
//! 6. **Strict subset**: the group is never the entire body (a loop of
//!    only-invariant statements could not terminate).
//!
//! ## Placement
//! Registered in `shir_passes/mod.rs`; run inside
//! `shir_passes::optimize::optimize()` — the shared A1 optimizer family
//! at the A1-ingress points (frontend JSON → render), behind the same
//! entry that runs const_prop/const_fold/dead_store_elim. Renderer-
//! agnostic: every backend consumes the hoisted A1 (the rewrite is
//! value-preserving by the guards above).

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, InterpPart};
use std::collections::{HashMap, HashSet};

// ────────────────────────────────────────────────────────────────────────
// function-level analysis: purity + read set + write set (one fixpoint)
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Purity {
    Impure,
    Pure,
}

struct FnInfo {
    purity: Purity,
    reads: HashSet<String>,
    writes: HashSet<String>,
}

fn function_bodies(stmts: &[IrStmt]) -> HashMap<String, Vec<IrStmt>> {
    let mut out = HashMap::new();
    for st in stmts {
        if let IrStmt::Function { name, body, .. } = st {
            out.insert(name.clone(), body.clone());
        }
    }
    out
}

fn analyze_functions(prog: &IrProgram) -> HashMap<String, FnInfo> {
    let mut bodies = function_bodies(&prog.stmts);
    for sub in &prog.subs {
        bodies.insert(sub.name.clone(), sub.body.clone());
    }
    let mut info: HashMap<String, FnInfo> = HashMap::new();
    for (name, body) in &bodies {
        let reads = stmts_read_names(body, &info);
        let writes = stmts_write_names(body, &info);
        let purity = body
            .iter()
            .map(|s| stmt_purity(s, &info))
            .min()
            .unwrap_or(Purity::Pure);
        info.insert(
            name.clone(),
            FnInfo { purity, reads, writes },
        );
    }
    // closure: recompute until stable (a called function's verdicts feed
    // the caller's)
    loop {
        let mut changed = false;
        for (name, body) in &bodies {
            let reads = stmts_read_names(body, &info);
            let writes = stmts_write_names(body, &info);
            let purity = body
                .iter()
                .map(|s| stmt_purity(s, &info))
                .min()
                .unwrap_or(Purity::Pure);
            let e = info.get_mut(name).unwrap();
            if e.purity != purity || e.reads != reads || e.writes != writes {
                e.purity = purity;
                e.reads = reads;
                e.writes = writes;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    info
}

// ── purity ─────────────────────────────────────────────────────────────

fn stmt_purity(st: &IrStmt, fns: &HashMap<String, FnInfo>) -> Purity {
    match st {
        IrStmt::Assign { expr, .. } => expr_purity(expr, fns),
        IrStmt::Declare { init, .. } => init
            .as_ref()
            .map(|i| expr_purity(i, fns))
            .unwrap_or(Purity::Pure),
        IrStmt::DeclareArray { elements, .. } => elements
            .iter()
            .map(|e| expr_purity(e, fns))
            .min()
            .unwrap_or(Purity::Pure),
        IrStmt::Expr(e) => expr_purity(e, fns),
        IrStmt::If { cond, then, elsifs, else_ } => {
            let mut p = expr_purity(cond, fns);
            for s in then {
                p = p.min(stmt_purity(s, fns));
            }
            for (c, b) in elsifs {
                p = p.min(expr_purity(c, fns));
                for s in b {
                    p = p.min(stmt_purity(s, fns));
                }
            }
            for s in else_ {
                p = p.min(stmt_purity(s, fns));
            }
            p
        }
        IrStmt::For { iter, body, .. } | IrStmt::While { cond: iter, body } => {
            let mut p = expr_purity(iter, fns);
            for s in body {
                p = p.min(stmt_purity(s, fns));
            }
            p
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut p = expr_purity(cond, fns);
            for s in body {
                p = p.min(stmt_purity(s, fns));
            }
            p
        }
        // control transfers + effects (Output/Exec/Break/Return/Exit/…)
        // are Impure — an Output would re-emit when hoisted.
        _ => Purity::Impure,
    }
}

fn expr_purity(e: &IrExpr, fns: &HashMap<String, FnInfo>) -> Purity {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Ident(_)
        | IrExpr::Bool(_) | IrExpr::Json(_) | IrExpr::RawExpr(_) | IrExpr::Regex { .. }
        | IrExpr::Range { .. } => Purity::Pure,
        IrExpr::Arith(a) => arith_purity(a),
        IrExpr::BinOp { lhs, rhs, .. } => expr_purity(lhs, fns).min(expr_purity(rhs, fns)),
        IrExpr::Ternary { cond, then, else_ } => expr_purity(cond, fns)
            .min(expr_purity(then, fns))
            .min(expr_purity(else_, fns)),
        IrExpr::DefinedOr { expr, default } => {
            expr_purity(expr, fns).min(expr_purity(default, fns))
        }
        IrExpr::Index { key, .. } => expr_purity(key, fns),
        IrExpr::Interpolate(parts) => parts
            .iter()
            .map(|p| match p {
                InterpPart::Lit(_) => Purity::Pure,
                InterpPart::Expr(x) => expr_purity(x, fns),
            })
            .min()
            .unwrap_or(Purity::Pure),
        IrExpr::Array(items) => items
            .iter()
            .map(|i| expr_purity(i, fns))
            .min()
            .unwrap_or(Purity::Pure),
        IrExpr::Splice(inner) => expr_purity(inner, fns),
        IrExpr::Call { func, args } => {
            // `builtin echo …` / `builtin printf …` have observable
            // OUTPUT — never hoist (the Output arm is Impure for the
            // same reason).
            if func == "builtin" {
                return Purity::Impure;
            }
            let args_p = args
                .iter()
                .map(|a| expr_purity(a, fns))
                .min()
                .unwrap_or(Purity::Pure);
            if func == "exec" || func == "fnCall" {
                // `exec "fnname" …` dispatches a user function: pure iff
                // the callee is Pure and the args are pure.
                let callee = args
                    .first()
                    .and_then(|a| match a {
                        IrExpr::Str(s, _) => fns.get(s),
                        _ => None,
                    });
                match callee {
                    Some(fi) if fi.purity == Purity::Pure => args_p,
                    _ => Purity::Impure,
                }
            } else {
                args_p
            }
        }
        // Capture/Arrow/Lambda/ArrayComp/MethodCall/Object — opaque
        // effects — Impure.
        _ => Purity::Impure,
    }
}

fn arith_purity(a: &ArithAst) -> Purity {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => Purity::Pure,
        ArithAst::Index { key, .. } => arith_purity(key),
        ArithAst::Bin { lhs, rhs, .. } => arith_purity(lhs).min(arith_purity(rhs)),
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => arith_purity(arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_purity(test).min(arith_purity(then)).min(arith_purity(else_))
        }
        ArithAst::Assign { rhs, .. } => arith_purity(rhs),
        ArithAst::IncDec { .. } => Purity::Pure,
    }
}

// ── read sets ──────────────────────────────────────────────────────────

fn stmts_read_names(stmts: &[IrStmt], fns: &HashMap<String, FnInfo>) -> HashSet<String> {
    let mut out = HashSet::new();
    for st in stmts {
        collect_stmt_reads(st, fns, &mut out);
    }
    out
}

fn collect_stmt_reads(st: &IrStmt, fns: &HashMap<String, FnInfo>, out: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            collect_expr_reads(expr, fns, out);
            // an indexed TARGET (`mime_lookup[$ml_i] = v`) READS the index
            // variable — the A1 renders the index INSIDE the var string
            // (`"mime_lookup[$ml_i]"`), so the loop counter is invisible
            // to the read set unless decoded. Without it the leading-write
            // invariance misses the counter and hoists `arr[$i]=…` out of
            // the loop (the maze-init loops, the mime-slot resets, then
            // everything derived from them — the turn45 culling corruption).
            // The base name is NOT added: writes to the same array are
            // governed by the writes-not-clobbered guard (which keys the
            // base too); the index operand is the read at stake here.
            for t in targets {
                if let Some(b) = t.var.find('[') {
                    bare_dollar_names(&t.var[b..], out);
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                collect_expr_reads(i, fns, out);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                collect_expr_reads(e, fns, out);
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            collect_expr_reads(cond, fns, out);
            for s in then {
                collect_stmt_reads(s, fns, out);
            }
            for (c, b) in elsifs {
                collect_expr_reads(c, fns, out);
                for s in b {
                    collect_stmt_reads(s, fns, out);
                }
            }
            for s in else_ {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrStmt::Case { discriminant, clauses } => {
            collect_expr_reads(discriminant, fns, out);
            for c in clauses {
                for s in &c.body {
                    collect_stmt_reads(s, fns, out);
                }
            }
        }
        IrStmt::Redirect { inner, .. } | IrStmt::Subshell(inner) | IrStmt::Background(inner) => {
            for s in inner {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            for s in body {
                collect_stmt_reads(s, fns, out);
            }
            for ex in excepts {
                for s in &ex.body {
                    collect_stmt_reads(s, fns, out);
                }
            }
            for s in else_body {
                collect_stmt_reads(s, fns, out);
            }
            for s in finally_body {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrStmt::For { var, iter, body, .. } => {
            collect_expr_reads(iter, fns, out);
            // the For-var is a per-iteration rebind — mark it as an
            // assigned name (reads of it are NOT loop-invariant).
            out.insert(var.clone());
            for s in body {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrStmt::While { cond, body } => {
            collect_expr_reads(cond, fns, out);
            for s in body {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for s in body {
                collect_stmt_reads(s, fns, out);
            }
            collect_expr_reads(cond, fns, out);
        }
        IrStmt::Block(b) => {
            for s in b {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrStmt::Return(Some(e)) => collect_expr_reads(e, fns, out),
        IrStmt::Output { value, .. } => collect_expr_reads(value, fns, out),
        IrStmt::Expr(e) => collect_expr_reads(e, fns, out),
        _ => {}
    }
}

fn collect_expr_reads(e: &IrExpr, fns: &HashMap<String, FnInfo>, out: &mut HashSet<String>) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => {
            out.insert(n.clone());
        }
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            collect_expr_reads(key, fns, out);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_expr_reads(lhs, fns, out);
            collect_expr_reads(rhs, fns, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_expr_reads(cond, fns, out);
            collect_expr_reads(then, fns, out);
            collect_expr_reads(else_, fns, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_expr_reads(expr, fns, out);
            collect_expr_reads(default, fns, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_expr_reads(x, fns, out);
                }
            }
        }
        IrExpr::Capture { expr, .. } => collect_expr_reads(expr, fns, out),
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => {
            for s in stmts {
                collect_stmt_reads(s, fns, out);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_expr_reads(i, fns, out);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            collect_expr_reads(iter, fns, out);
            collect_expr_reads(elem, fns, out);
            if let Some(c) = cond {
                collect_expr_reads(c, fns, out);
            }
        }
        IrExpr::Splice(inner) => collect_expr_reads(inner, fns, out),
        IrExpr::Arith(a) => collect_arith_reads(a, out),
        IrExpr::Object(pairs) => {
            for (_, v) in pairs {
                collect_expr_reads(v, fns, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_expr_reads(obj, fns, out);
            for a in args {
                collect_expr_reads(a, fns, out);
            }
        }
        IrExpr::Call { func, args } => {
            if func == "exec" || func == "fnCall" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if let Some(fi) = fns.get(n) {
                        out.extend(fi.reads.iter().cloned());
                    }
                }
            }
            // literal-name ops read the NAME
            if matches!(func.as_str(), "getVar" | "listVar" | "arrayItems" | "arrayLen" | "setVar" | "setArray" | "arrayIndex" | "idxassign") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.insert(n.clone());
                }
                // the array ops carry the INDEX as an UNEXPANDED string
                // literal (`arrayIndex("mx", "$um_i")` — a Str, not a Var),
                // so the counter it names is invisible to the read set
                // unless decoded — an invariance miss that hoisted
                // `um_a=mx[$um_i]` out of the mime loop (stale mimes every
                // step + the turn45 culling corruption). Decode every Str
                // arg with the same bare_dollar_names the test/arith arm
                // uses; the NAME arg decodes to nothing (no $).
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        bare_dollar_names(s, out);
                    }
                }
            } else if func == "param" {
                if let Some(IrExpr::Str(n, _)) = args.get(1) {
                    out.insert(n.clone());
                }
                // param's OTHER args carry unexpanded $vars too: the
                // slice start of `${s:$i:1}` arrives as Str("$i") (not a
                // Var node), so the invariance analysis saw dt_ch as
                // loop-invariant and hoisted the slice out of draw_text's
                // loop — every HUD char drew the first one. Decode every
                // Str arg like the array ops arm does (the NAME arg
                // decodes to nothing — no $).
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        bare_dollar_names(s, out);
                    }
                }
            } else if func == "test" || func == "arith" {
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        bare_dollar_names(s, out);
                    }
                }
            }
            // EVERY arg is a read (the op-KEY operands included): the
            // name-only arms above miss `arrayIndex("mx", um_i)` reading
            // um_i, so the invariance analysis marked loop-counter shapes
            // invariant and hoisted `um_a=mx[um_i]` out of the loop (the
            // update_mimes stale-mime + turn45-culling corruption). Str
            // literals add nothing; Var/Index/Call args add their reads.
            for a in args {
                collect_expr_reads(a, fns, out);
            }
        }
        _ => {}
    }
}

fn collect_arith_reads(a: &ArithAst, out: &mut HashSet<String>) {
    match a {
        ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
        ArithAst::Var(n) | ArithAst::Ident(n) => {
            out.insert(n.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_arith_reads(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_arith_reads(lhs, out);
            collect_arith_reads(rhs, out);
        }
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => collect_arith_reads(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_arith_reads(test, out);
            collect_arith_reads(then, out);
            collect_arith_reads(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            collect_arith_reads(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
    }
}

fn bare_dollar_names(s: &str, out: &mut HashSet<String>) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() {
            let mut j = i + 1;
            if b[j] == b'{' {
                j += 1;
            }
            let start = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j > start {
                out.insert(s[start..j].to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
}

// ── write sets ─────────────────────────────────────────────────────────

fn stmts_write_names(stmts: &[IrStmt], fns: &HashMap<String, FnInfo>) -> HashSet<String> {
    let mut out = HashSet::new();
    for st in stmts {
        collect_stmt_writes(st, fns, &mut out);
    }
    out
}

fn collect_stmt_writes(st: &IrStmt, fns: &HashMap<String, FnInfo>, out: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                out.insert(t.var.clone());
                // an indexed target (`rmx[$rm_i]`) writes the BASE array
                // too — readers record `rmx`, so record both or a
                // mid-body store to the same array bypasses the
                // writes-not-clobbered guard (both the base and the
                // full "[$i]" key are kept for the multi-var case).
                if let Some(b) = t.var.find('[') {
                    out.insert(t.var[..b].to_string());
                }
            }
            if let IrExpr::Arith(a) = expr {
                collect_arith_writes(a, out);
            }
        }
        IrStmt::Declare { vars, .. } => {
            for v in vars {
                out.insert(v.name.clone());
            }
        }
        IrStmt::DeclareArray { var, .. } => {
            out.insert(var.clone());
        }
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then {
                collect_stmt_writes(s, fns, out);
            }
            for (_, b) in elsifs {
                for s in b {
                    collect_stmt_writes(s, fns, out);
                }
            }
            for s in else_ {
                collect_stmt_writes(s, fns, out);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                for s in &c.body {
                    collect_stmt_writes(s, fns, out);
                }
            }
        }
        IrStmt::Redirect { inner, .. } | IrStmt::Subshell(inner) | IrStmt::Background(inner) => {
            for s in inner {
                collect_stmt_writes(s, fns, out);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            for s in body {
                collect_stmt_writes(s, fns, out);
            }
            for ex in excepts {
                for s in &ex.body {
                    collect_stmt_writes(s, fns, out);
                }
            }
            for s in else_body {
                collect_stmt_writes(s, fns, out);
            }
            for s in finally_body {
                collect_stmt_writes(s, fns, out);
            }
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            for s in body {
                collect_stmt_writes(s, fns, out);
            }
        }
        IrStmt::Block(b) => {
            for s in b {
                collect_stmt_writes(s, fns, out);
            }
        }
        IrStmt::Expr(IrExpr::Call { func, args }) => {
            if func == "exec" || func == "fnCall" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if let Some(fi) = fns.get(n) {
                        out.extend(fi.writes.iter().cloned());
                    }
                }
            }
            if matches!(func.as_str(), "setVar" | "setArray") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.insert(n.clone());
                }
            } else if func == "param" {
                if let Some(IrExpr::Str(op, _)) = args.first() {
                    if op == ":=" {
                        if let Some(IrExpr::Str(n, _)) = args.get(1) {
                            out.insert(n.clone());
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_arith_writes(a: &ArithAst, out: &mut HashSet<String>) {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_arith_writes(lhs, out);
            collect_arith_writes(rhs, out);
        }
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => collect_arith_writes(arg, out),
        ArithAst::Index { key, .. } => collect_arith_writes(key, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_arith_writes(test, out);
            collect_arith_writes(then, out);
            collect_arith_writes(else_, out);
        }
        _ => {}
    }
}

// ── guards ─────────────────────────────────────────────────────────────

fn has_control_escape(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|st| match st {
        IrStmt::Break | IrStmt::Continue | IrStmt::Return(_) | IrStmt::Exit(_)
        | IrStmt::Goto(_) | IrStmt::Die { .. } | IrStmt::Warn { .. } | IrStmt::Label(_) => true,
        IrStmt::If { then, elsifs, else_, .. } => {
            has_control_escape(then)
                || elsifs.iter().any(|(_, b)| has_control_escape(b))
                || has_control_escape(else_)
        }
        IrStmt::Case { clauses, .. } => clauses.iter().any(|c| has_control_escape(&c.body)),
        IrStmt::Redirect { inner, .. } | IrStmt::Subshell(inner) | IrStmt::Background(inner) => {
            has_control_escape(inner)
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            has_control_escape(body)
                || excepts.iter().any(|e| has_control_escape(&e.body))
                || has_control_escape(else_body)
                || has_control_escape(finally_body)
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            has_control_escape(body)
        }
        IrStmt::Block(b) => has_control_escape(b),
        _ => false,
    })
}

/// Does any statement in the remaining body OBSERVE `$?`?
fn observes_status(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|st| match st {
        IrStmt::Expr(IrExpr::Call { func, .. }) => {
            matches!(
                func.as_str(),
                "lastStatus" | "lastexit" | "status" | "getStatus" | "exitCode"
            )
        }
        IrStmt::Output { value, .. } | IrStmt::Assign { expr: value, .. } => expr_has_status(value),
        _ => false,
    })
}

fn expr_has_status(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(s, _) => s.contains("$?"),
        IrExpr::Interpolate(parts) => {
            parts.iter().any(|p| match p {
                InterpPart::Lit(t) => t.contains("$?"),
                InterpPart::Expr(x) => expr_has_status(x),
            })
        }
        IrExpr::Arith(a) => arith_has_status(a),
        IrExpr::Call { func, args } => {
            matches!(func.as_str(), "lastStatus" | "lastexit" | "status" | "getStatus")
                || args.iter().any(expr_has_status)
        }
        _ => false,
    }
}

fn arith_has_status(a: &ArithAst) -> bool {
    match a {
        ArithAst::Var(n) => n == "?",
        ArithAst::Num(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => false,
        ArithAst::Index { key, .. } => arith_has_status(key),
        ArithAst::Bin { lhs, rhs, .. } => arith_has_status(lhs) || arith_has_status(rhs),
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => arith_has_status(arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_has_status(test) || arith_has_status(then) || arith_has_status(else_)
        }
        ArithAst::Assign { rhs, .. } => arith_has_status(rhs),
        ArithAst::IncDec { .. } => false,
    }
}

// ────────────────────────────────────────────────────────────────────────
// the hoist
// ────────────────────────────────────────────────────────────────────────

/// Hoist the leading maximal run of pure + loop-invariant statements out
/// of ONE loop body. Returns the number of statements hoisted (0 = none).
/// The caller splices them before the loop statement.
fn hoist_group_len(
    body: &mut Vec<IrStmt>,
    fns: &HashMap<String, FnInfo>,
    for_var: Option<&str>,
) -> usize {
    if body.len() < 2 {
        return 0;
    }
    // pre-scan: every name assigned anywhere in the body, with its total
    // writer count (the For-var counts as assigned per-iteration).
    let mut writer_count: HashMap<String, usize> = HashMap::new();
    for st in body.iter() {
        let mut ws = HashSet::new();
        collect_stmt_writes(st, fns, &mut ws);
        for n in ws {
            *writer_count.entry(n).or_insert(0) += 1;
        }
    }
    if let Some(v) = for_var {
        *writer_count.entry(v.to_string()).or_insert(0) += 1;
    }

    let mut g_writes: HashSet<String> = HashSet::new();
    let mut g_reads: HashSet<String> = HashSet::new();
    let mut group_ends = 0usize;

    for (i, st) in body.iter().enumerate() {
        // 1) purity
        if !stmt_hoistable(st, fns) {
            break;
        }
        // 2) reads invariant: not assigned in body, or assigned only by
        //    an earlier group member
        let mut reads = HashSet::new();
        collect_stmt_reads(st, fns, &mut reads);
        let reads_ok = reads
            .iter()
            .all(|r| !writer_count.contains_key(r) || g_writes.contains(r) || r == "?");
        if !reads_ok {
            break;
        }
        // 3) writes not clobbered outside the group
        let mut writes = HashSet::new();
        collect_stmt_writes(st, fns, &mut writes);
        let writes_ok = writes.iter().all(|t| {
            let total = writer_count.get(t).copied().unwrap_or(0);
            let in_group = g_writes.contains(t);
            total == (usize::from(in_group) + 1)
        });
        if !writes_ok {
            break;
        }
        g_writes.extend(writes);
        g_reads.extend(reads);
        group_ends = i + 1;
    }

    if group_ends == 0 || group_ends >= body.len() {
        return 0;
    }
    if has_control_escape(body) {
        return 0;
    }
    let group_has_call = body[..group_ends]
        .iter()
        .any(|s| matches!(s, IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec" || func == "fnCall"));
    // a hoisted group must not include the ONLY statement that would make
    // the loop terminate — guaranteed by `group_ends < body.len()`, but a
    // full-body group is additionally blocked above.
    let _ = g_reads;
    if group_has_call && observes_status(&body[group_ends..]) {
        return 0;
    }
    group_ends
}

fn stmt_hoistable(st: &IrStmt, fns: &HashMap<String, FnInfo>) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_purity(expr, fns) == Purity::Pure,
        IrStmt::Declare { init, .. } => init
            .as_ref()
            .map(|i| expr_purity(i, fns) == Purity::Pure)
            .unwrap_or(true),
        IrStmt::DeclareArray { elements, .. } => elements
            .iter()
            .all(|e| expr_purity(e, fns) == Purity::Pure),
        IrStmt::Expr(e) => expr_purity(e, fns) == Purity::Pure,
        _ => false,
    }
}

/// One pass over a statement list: recurse into nested bodies first,
/// then hoist each loop's leading invariant group into the enclosing
/// list (the loop prologue).
fn pass(stmts: &mut Vec<IrStmt>, fns: &HashMap<String, FnInfo>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let hoisted_len: usize = {
            let st = &mut stmts[i];
            match st {
                IrStmt::Function { body, .. } => {
                    changed |= pass(body, fns);
                    0
                }
                IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                    changed |= pass(body, fns);
                    hoist_group_len(body, fns, None)
                }
                IrStmt::For { var, body, .. } => {
                    changed |= pass(body, fns);
                    hoist_group_len(body, fns, Some(var))
                }
                IrStmt::If {
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    changed |= pass(then, fns);
                    for (_, b) in elsifs.iter_mut() {
                        changed |= pass(b, fns);
                    }
                    changed |= pass(else_, fns);
                    0
                }
                IrStmt::Case { clauses, .. } => {
                    for c in clauses.iter_mut() {
                        changed |= pass(&mut c.body, fns);
                    }
                    0
                }
                IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                    changed |= pass(b, fns);
                    0
                }
                IrStmt::Redirect { inner, .. } => {
                    changed |= pass(inner, fns);
                    0
                }
                IrStmt::Try {
                    body,
                    excepts,
                    else_body,
                    finally_body,
                } => {
                    changed |= pass(body, fns);
                    for e in excepts.iter_mut() {
                        changed |= pass(&mut e.body, fns);
                    }
                    changed |= pass(else_body, fns);
                    changed |= pass(finally_body, fns);
                    0
                }
                _ => 0,
            }
        };
        if hoisted_len > 0 {
            let group: Vec<IrStmt> = stmts[i].body_drain_leading(hoisted_len);
            stmts.splice(i..i, group);
            changed = true;
        } else {
            i += 1;
        }
    }
    changed
}

impl IrStmt {
    /// Drain the first `n` statements of a loop body out of the node.
    fn body_drain_leading(&mut self, n: usize) -> Vec<IrStmt> {
        match self {
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } | IrStmt::For { body, .. } => {
                body.drain(..n).collect()
            }
            _ => Vec::new(),
        }
    }
}

/// Hoist loop-invariant leading statement groups out of every loop in
/// the program (fixpoint — inner loops hoist first, then the outer
/// loops may grab the moved group again).
pub fn hoist_loop_invariants(prog: &mut IrProgram) -> bool {
    let fns = analyze_functions(prog);
    let mut changed = false;
    loop {
        let c = pass(&mut prog.stmts, &fns);
        let mut c2 = false;
        for sub in &mut prog.subs {
            c2 |= pass(&mut sub.body, &fns);
        }
        if !(c || c2) {
            break;
        }
        changed = true;
    }
    changed
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ArithAst, AssignTarget, IrType, StrStyle, VarKind};

    fn prog_of(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
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

    fn arith(ast: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(ast))
    }

    fn int(i: i64) -> IrExpr {
        IrExpr::Int(i)
    }

    fn while_loop(body: Vec<IrStmt>) -> IrStmt {
        IrStmt::While {
            cond: IrExpr::Call {
                func: "test".to_string(),
                args: vec![],
            },
            body,
        }
    }

    fn fn_call(name: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted)],
        })
    }

    fn fn_def(name: &str, body: Vec<IrStmt>) -> IrStmt {
        IrStmt::Function {
            name: name.to_string(),
            body,
            named_blocks: vec![],
        }
    }

    fn echo() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![]),
            ],
        })
    }

    /// 201235: a pure column-hash call + its derived assigns, invariant
    /// in the INNER (row) loop, hoist to the inner loop prologue.
    #[test]
    fn pure_call_and_derived_hoist_out_of_inner_loop() {
        let prog = prog_of(vec![
            fn_def(
                "lat_hash",
                vec![assign(
                    "lhn",
                    arith(ArithAst::Bin {
                        op: "*".to_string(),
                        lhs: Box::new(ArithAst::Var("x".to_string())),
                        rhs: Box::new(ArithAst::Num(48271)),
                    }),
                )],
            ),
            assign("x", int(0)),
            while_loop(vec![
                assign("y", int(0)),
                while_loop(vec![
                    fn_call("lat_hash"),
                    assign(
                        "gph",
                        arith(ArithAst::Bin {
                            op: "%".to_string(),
                            lhs: Box::new(ArithAst::Var("lhn".to_string())),
                            rhs: Box::new(ArithAst::Num(3)),
                        }),
                    ),
                    echo(),
                    assign(
                        "y",
                        arith(ArithAst::Bin {
                            op: "+".to_string(),
                            lhs: Box::new(ArithAst::Var("y".to_string())),
                            rhs: Box::new(ArithAst::Num(1)),
                        }),
                    ),
                ]),
                assign(
                    "x",
                    arith(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(ArithAst::Var("x".to_string())),
                        rhs: Box::new(ArithAst::Num(1)),
                    }),
                ),
            ]),
        ]);
        let mut prog = prog;
        assert!(hoist_loop_invariants(&mut prog));
        // outer while body: [y=0, lat_hash, gph, inner-while, x=x+1]
        let outer = match &prog.stmts[2] {
            IrStmt::While { body, .. } => body,
            other => panic!("expected outer while, got {other:?}"),
        };
        assert_eq!(outer.len(), 5, "the call+derived group sits before the inner loop");
        assert!(matches!(&outer[1], IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec"));
        assert!(matches!(&outer[2], IrStmt::Assign { targets, .. } if targets[0].var == "gph"));
        // inner body keeps only the echo + counter
        let inner = match &outer[3] {
            IrStmt::While { body, .. } => body,
            other => panic!("expected inner while, got {other:?}"),
        };
        assert_eq!(inner.len(), 2);
        assert!(matches!(&inner[0], IrStmt::Expr(IrExpr::Call { func, .. }) if func == "builtin"));
        assert!(matches!(&inner[1], IrStmt::Assign { targets, .. } if targets[0].var == "y"));
    }

    /// A derived stmt that reads the INNER counter is NOT invariant —
    /// the group must stop before it (and the call itself stays when the
    /// read guard trips the whole leading run? no: the call is still
    /// invariant — the counter read is only in the derived stmt).
    #[test]
    fn counter_dependent_derived_blocks_only_itself() {
        // inner loop: [lat_hash, stripe=$((x + gph) % 5), x=x+1] where x
        // is the INNER counter → lat_hash is still invariant (reads only
        // x — which IS written by x=x+1 in the body!). So nothing hoists
        // (lat_hash reads the counter too).
        let prog = prog_of(vec![
            fn_def(
                "lat_hash",
                vec![assign(
                    "lhn",
                    arith(ArithAst::Bin {
                        op: "*".to_string(),
                        lhs: Box::new(ArithAst::Var("x".to_string())),
                        rhs: Box::new(ArithAst::Num(7)),
                    }),
                )],
            ),
            assign("x", int(0)),
            while_loop(vec![
                fn_call("lat_hash"),
                assign(
                    "stripe",
                    arith(ArithAst::Bin {
                        op: "%".to_string(),
                        lhs: Box::new(ArithAst::Bin {
                            op: "+".to_string(),
                            lhs: Box::new(ArithAst::Var("x".to_string())),
                            rhs: Box::new(ArithAst::Var("gph".to_string())),
                        }),
                        rhs: Box::new(ArithAst::Num(5)),
                    }),
                ),
                assign(
                    "x",
                    arith(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(ArithAst::Var("x".to_string())),
                        rhs: Box::new(ArithAst::Num(1)),
                    }),
                ),
            ]),
        ]);
        let mut prog = prog;
        assert!(!hoist_loop_invariants(&mut prog), "counter reads block the hoist");
    }

    /// 182436 arm: pure const arithmetic on loop-invariant operands
    /// hoists; an arith on the counter stays.
    #[test]
    fn invariant_arith_hoists_counter_arith_stays() {
        let prog = prog_of(vec![
            assign("CELLS", int(16)),
            assign("MAP_W", int(16)),
            assign("i", int(0)),
            while_loop(vec![
                // cells-per-row product: reads only consts → hoist
                assign(
                    "rowprod",
                    arith(ArithAst::Bin {
                        op: "*".to_string(),
                        lhs: Box::new(ArithAst::Var("CELLS".to_string())),
                        rhs: Box::new(ArithAst::Var("MAP_W".to_string())),
                    }),
                ),
                echo(),
                assign(
                    "i",
                    arith(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(ArithAst::Var("i".to_string())),
                        rhs: Box::new(ArithAst::Num(1)),
                    }),
                ),
            ]),
        ]);
        let mut prog = prog;
        assert!(hoist_loop_invariants(&mut prog));
        let stmts = &prog.stmts;
        assert_eq!(stmts.len(), 5, "rowprod hoisted before the loop");
        assert!(matches!(&stmts[3], IrStmt::Assign { targets, .. } if targets[0].var == "rowprod"));
        if let IrStmt::While { body, .. } = &stmts[4] {
            assert_eq!(body.len(), 2);
            assert!(matches!(&body[1], IrStmt::Assign { targets, .. } if targets[0].var == "i"));
        } else {
            panic!("expected the while at the end");
        }
    }

    /// A mid-body clobber of a group target blocks the hoist (the
    /// hoisted value would be overwritten in the original order).
    #[test]
    fn clobbered_target_blocks_hoist() {
        let prog = prog_of(vec![
            assign("i", int(0)),
            while_loop(vec![
                assign("acc", arith(ArithAst::Num(1))),
                echo(),
                assign(
                    "acc",
                    arith(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(ArithAst::Var("acc".to_string())),
                        rhs: Box::new(ArithAst::Num(1)),
                    }),
                ),
            ]),
        ]);
        let mut prog = prog;
        assert!(!hoist_loop_invariants(&mut prog), "acc is rewritten mid-body — hoisting would change values");
    }

    /// A body with a `break` blocks the hoist (a skipped iteration
    /// would leave stale values).
    #[test]
    fn break_in_body_blocks_hoist() {
        let prog = prog_of(vec![
            while_loop(vec![
                assign("acc", arith(ArithAst::Num(1))),
                IrStmt::Break,
                echo(),
            ]),
        ]);
        let mut prog = prog;
        assert!(!hoist_loop_invariants(&mut prog));
    }

    /// `echo` (observable output) is never part of a hoisted group.
    #[test]
    fn echo_is_not_hoisted() {
        let prog = prog_of(vec![
            while_loop(vec![echo(), assign("i", arith(ArithAst::Num(1)))]),
        ]);
        let mut prog = prog;
        assert!(!hoist_loop_invariants(&mut prog), "an Output statement is Impure — never hoisted");
    }

    /// The transform is deterministic + idempotent at the fixpoint.
    #[test]
    fn fixpoint_is_idempotent() {
        let prog = prog_of(vec![
            fn_def(
                "lat_hash",
                vec![assign(
                    "lhn",
                    arith(ArithAst::Bin {
                        op: "*".to_string(),
                        lhs: Box::new(ArithAst::Var("x".to_string())),
                        rhs: Box::new(ArithAst::Num(48271)),
                    }),
                )],
            ),
            assign("x", int(0)),
            while_loop(vec![
                assign("y", int(0)),
                while_loop(vec![
                    fn_call("lat_hash"),
                    assign(
                        "gph",
                        arith(ArithAst::Bin {
                            op: "%".to_string(),
                            lhs: Box::new(ArithAst::Var("lhn".to_string())),
                            rhs: Box::new(ArithAst::Num(3)),
                        }),
                    ),
                    echo(),
                    assign("y", arith(ArithAst::Num(1))),
                ]),
                assign("x", arith(ArithAst::Num(1))),
            ]),
        ]);
        let mut prog = prog;
        assert!(hoist_loop_invariants(&mut prog));
        let json1 = crate::shir_json::shir_to_shir_json(&prog);
        assert!(!hoist_loop_invariants(&mut prog), "second run must be a no-op");
        let json2 = crate::shir_json::shir_to_shir_json(&prog);
        assert_eq!(json1, json2, "deterministic at the fixpoint");
    }
}
