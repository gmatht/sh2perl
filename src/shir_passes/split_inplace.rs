//! split-in-place — destructive-buffer-reuse for legacy `split` calls in
//! `for w in $var` iteration position.
//!
//! When a split's SOURCE is a plain mutable store var that is never read
//! again after the loop statement, the C backend can tokenize the var's
//! own buffer in place (delimiters stamped with NUL) instead of copying
//! it into a 64 KB scratch first. The transform proves safety and
//! rewrites the legacy `Call{"split", [Var X]}` iterator into
//! `Ext(Split{in_place: true})`; each backend decides what the flag
//! means (GC'd string languages ignore it — immutable strings make it
//! meaningless; C treats it as exclusive-ownership permission).
//!
//! Soundness rules (each conservative):
//! - the text operand must be a PLAIN variable read — literals,
//!   interpolations, env fallbacks and composite exprs are ineligible;
//! - the var must have ZERO reads after the loop statement anywhere in
//!   the program, including inside function bodies (call time unknown);
//! - exactly ONE read at the loop statement itself — a body that also
//!   reads the source would observe the mutated content, diverging from
//!   bash where `$y` keeps its value through `for w in $y`;
//! - export-visible names are ineligible.
//!
//! Registered in transforms.rs ("split-in-place"). Additive contract:
//! Split gains an optional_bool field defaulting to false.

use crate::ir::{InterpPart, IrExpr, IrStmt};
use std::collections::{BTreeSet, HashMap};

/// Names READ by an expression (conservative: any Var/Ident and any
/// getVar/param/split/arrayItems first-name mention counts).
fn expr_reads(e: &IrExpr, out: &mut Vec<String>) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => out.push(n.clone()),
        IrExpr::Call { func, args } => {
            if matches!(func.as_str(), "getVar" | "param" | "split" | "arrayItems") {
                if let Some(IrExpr::Str(nm, _)) = args.first() {
                    if nm.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                        out.push(nm.clone());
                    }
                }
            }
            for a in args {
                expr_reads(a, out);
            }
        }
        IrExpr::Array(items) => items.iter().for_each(|x| expr_reads(x, out)),
        IrExpr::Interpolate(parts) => parts.iter().for_each(|p| {
            if let InterpPart::Expr(x) = p {
                expr_reads(x, out);
            }
        }),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_reads(lhs, out);
            expr_reads(rhs, out);
        }
        IrExpr::Ext(n) => {
            for c in crate::shir_nodes::ExtExpr::children(&**n) {
                expr_reads(c, out);
            }
        }
        _ => {}
    }
}

fn stmt_reads(s: &IrStmt, out: &mut Vec<String>) {
    match s {
        IrStmt::Expr(e) => expr_reads(e, out),
        IrStmt::Output { value, .. } => expr_reads(value, out),
        IrStmt::Assign { targets, expr, .. } => {
            expr_reads(expr, out);
            for t in targets {
                for i in &t.indices {
                    expr_reads(i, out);
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                expr_reads(i, out);
            }
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads(cond, out);
            for b in then.iter() { stmt_reads(b, out); }
            for (_, b) in elsifs.iter() { for s in b.iter() { stmt_reads(s, out); } }
            for s in else_.iter() { stmt_reads(s, out); }
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
            expr_reads(cond, out);
            for s in body.iter() { stmt_reads(s, out); }
        }
        IrStmt::For { iter, body, .. } => {
            expr_reads(iter, out);
            for s in body.iter() { stmt_reads(s, out); }
        }
        IrStmt::Pipeline { stages, .. } => {
            for st in stages { for s in st.iter() { stmt_reads(s, out); } }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner.iter() { stmt_reads(s, out); }
        }
        IrStmt::Block(body) | IrStmt::Background(body) => {
            for s in body.iter() { stmt_reads(s, out); }
        }
        _ => {}
    }
}

/// Rewrite eligible splits in place. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // per top-level statement index: read census for var names
    let mut counts: Vec<HashMap<String, usize>> = Vec::new();
    for s in stmts.iter() {
        let mut reads: Vec<String> = Vec::new();
        stmt_reads(s, &mut reads);
        let mut m: HashMap<String, usize> = HashMap::new();
        for r in reads {
            *m.entry(r).or_insert(0) += 1;
        }
        counts.push(m);
    }

    // reads inside FUNCTION definitions: call time unknown — any var
    // mentioned there is ineligible everywhere
    let mut fn_read_names: BTreeSet<String> = BTreeSet::new();
    fn collect_fn_reads(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
        for s in stmts {
            if let IrStmt::Function { name, body, .. } = s {
                out.insert(name.clone());
                for b in body.iter() {
                    let mut reads: Vec<String> = Vec::new();
                    stmt_reads(b, &mut reads);
                    out.extend(reads);
                    collect_fn_reads(body, out);
                }
            }
        }
    }
    collect_fn_reads(stmts, &mut fn_read_names);

    let exported = exported_names(stmts);

    // candidate shape: For{iter: Array[Call{"split", [Var X]}]}
    let mut changed = false;
    for (i, s) in stmts.iter_mut().enumerate() {
        let IrStmt::For { iter, .. } = s else { continue };
        let IrExpr::Array(items) = &*iter else { continue };
        if items.len() != 1 {
            continue;
        }
        let IrExpr::Call { func, args } = &items[0] else { continue };
        if func != "split" {
            continue;
        }
        // the source arrives as getVar("y") OR Var("y")
        let xv: String = match args.first() {
            Some(IrExpr::Var(v, _)) | Some(IrExpr::Ident(v)) => v.clone(),
            Some(IrExpr::Call { func, args: ca })
                if func == "getVar" && matches!(ca.first(), Some(IrExpr::Str(_, _))) =>
            {
                match ca.first() {
                    Some(IrExpr::Str(nm, _)) => nm.clone(),
                    _ => continue,
                }
            }
            _ => continue,
        };
        if !is_plain_mutable_name(&xv) {
            continue;
        }
        let x = xv.clone();
        let cnt_i = counts.get(i).and_then(|m| m.get(&x)).copied().unwrap_or(0);
        let later = counts.iter().skip(i + 1).any(|m| m.contains_key(&x));
        let ok =
            cnt_i == 1 && !later && !fn_read_names.contains(&x) && !exported.contains(&x);
        if !ok {
            continue;
        }
        // eligible: swap the legacy split-call for the marked Ext(Split);
        // the For iter stays a word-list ARRAY (shape preserved)
        *iter = IrExpr::Array(vec![IrExpr::Ext(Box::new(crate::shir_nodes::Split {
            text: IrExpr::Var(x.clone(), None),
            delim: delim_from(args),
            is_regex: false,
            in_place: true,
        }))]);
        changed = true;
    }
    changed
}

fn delim_from(args: &[IrExpr]) -> String {
    match args.get(1) {
        Some(IrExpr::Str(d, _)) => d.clone(),
        _ => " ".to_string(),
    }
}


fn is_plain_mutable_name(x: &str) -> bool {
    !x.is_empty()
        && x.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && x.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
}

/// Names given the EXPORT attribute or otherwise env-visible — never
/// eligible for in-place destruction.
fn exported_names(stmts: &[IrStmt]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for s in stmts.iter() {
        if let IrStmt::Expr(IrExpr::Call { func, args }) = s {
            if (func == "exec" || func == "builtin")
                && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "export")
            {
                let mut words: Vec<&IrExpr> = Vec::new();
                for a in args.iter().skip(1) {
                    match a {
                        IrExpr::Array(items) => words.extend(items.iter()),
                        other => words.push(other),
                    }
                }
                for w in words {
                    if let IrExpr::Str(ws, _) = w {
                        if let Some((n, _)) = ws.split_once('=') {
                            out.insert(n.to_string());
                        } else {
                            out.insert(ws.clone());
                        }
                    }
                }
            }
        }
    }
    out
}
