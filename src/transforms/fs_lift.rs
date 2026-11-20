//! fs-lift — the shell's file PREDICATES as shIR nodes.
//!
//! `[ A -ef B ]`, `[[ "$a" -nt "$b" ]]`, `[ -N f ]` … are the last file-shaped
//! shapes the Rust backend still renders as HELPER calls (`__sh_samefile`,
//! `__sh_mtime`, `__sh_fnewer`) rather than as the standard-library expression
//! they are. This module lifts them into the `FsSameFile` / `FsNewerThan` /
//! `FsModifiedSinceRead` nodes (`src/shir_nodes/fs_*.node`), which a renderer
//! fills with its own `std` calls; the node carries the SEMANTICS (the
//! missing-operand rule, full-precision timestamps, stat-not-lstat) so every
//! renderer spells the same predicate.
//!
//! ## Where it runs
//!
//! `text_ops`' expression walker calls [`lift_test`] for every
//! `Call { func: "test", args: [Str(text), style?] }` it meets — the same
//! walker that lowers `tr`/`cut`/`head`, so the lift reaches nested
//! conditions, `&&` chains and `if` conditions without a second visitor. The
//! lift fires only when the target DECLARES the node
//! (`node_manifest` — Rust today).
//!
//! ## The sound rule
//!
//! The test text must be EXACTLY the operator shape — no `!`, no `-a`/`-o`,
//! no parentheses, no glob:
//!
//! * three tokens `A -ef B` / `A -nt B` / `A -ot B`
//! * one token + operand `-N A`
//!
//! and each operand must be one of:
//!
//! * a plain variable read — `$name`, `${name}`, `"$name"` (a bare identifier;
//!   `$1`, `${a[0]}`, `$?` are refused)
//! * a literal path — unquoted (no `$`, `*`, `?`, `[`, `]`, `~`, `\`, quotes,
//!   braces or parens), `'…'` (verbatim), or `"…"` with no `$`/backtick
//!
//! Anything else keeps the command, whose renderer already models the predicate
//! (that path stays exact and is what `test-operator-missing.sh` exercises).
//!
//! ## Refused on purpose (and why the NODE list stops here)
//!
//! `-O` (owned by the EFFECTIVE uid) and `-G` (effective gid): `std` exposes no
//! `geteuid`/`getegid`, and the real-vs-effective distinction is observable
//! under a setuid/setgid binary, so the helper's `/proc/self/status` real-uid
//! read is NOT the predicate — those stay helper calls rather than becoming a
//! node with a lie in its name.
//!
//! `[ -N f ]` is included because it is exactly `mtime > atime` over one stat.

use crate::ir::{IrExpr, IrStmt, InterpPart, StrStyle};

/// The node tags this lift can produce (the manifest gate has to carry each).
pub const TAGS: &[&str] = &["FsSameFile", "FsNewerThan", "FsModifiedSinceRead"];

fn offered(tag: &str) -> bool {
    let enabled = std::env::var("SH2_TRANSFORMS").unwrap_or_default();
    if enabled.split(',').any(|s| s.trim() == "fs-lift") {
        return true;
    }
    // Unlike `text_ops`, an UNSET target means NO here. These nodes are only
    // ever declared by the Rust manifest (every other backend's handler
    // returns None), so a lift taken for an unset/other target would hand a
    // renderer a node it cannot render — the estree test family showed
    // exactly that when `offered` answered true for an unset target.
    match crate::transforms::target() {
        Some(t) => crate::transforms::planner::node_manifest(&t).contains(&tag),
        None => false,
    }
}

/// A variable read as the IR expresses it: `param("", name)` — the shape the
/// renderers already turn into the variable's value (text_ops' `${v:1:3}`
/// operands use it).
fn var_read(name: &str) -> IrExpr {
    IrExpr::Call {
        func: "param".to_string(),
        args: vec![
            IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
            IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted),
        ],
    }
}

fn plain_name(s: &str) -> Option<&str> {
    if s.is_empty() {
        return None;
    }
    let mut cs = s.chars();
    let first = cs.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !cs.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(s)
}

/// One test operand: a variable read or a literal path.
fn operand(tok: &str) -> Option<IrExpr> {
    let t = tok.trim();
    if t.is_empty() {
        return None;
    }
    // A bare variable reference: `$name` / `${name}`.
    let bare_var = t
        .strip_prefix("${")
        .and_then(|r| r.strip_suffix('}'))
        .or_else(|| t.strip_prefix('$'));
    if let Some(name) = bare_var {
        // `${#x}`, `${x:-y}` and `${a[0]}` are not plain reads.
        if let Some(n) = plain_name(name) {
            return Some(var_read(n));
        }
        return None;
    }
    // A quoted token: single quotes are verbatim, double quotes expand.
    if t.len() >= 2 {
        let bytes = t.as_bytes();
        let (q, inner) = (bytes[0], &t[1..t.len() - 1]);
        if (q == b'\'' || q == b'"') && bytes[t.len() - 1] == q {
            if q == b'\'' {
                if inner.contains('\'') {
                    return None;
                }
                return Some(IrExpr::Str(inner.to_string(), StrStyle::SingleQuoted));
            }
            // Double quotes: a WHOLE-token `$name` is the only expansion we
            // model; anything else that expands is refused.
            if let Some(name) = inner
                .strip_prefix("${")
                .and_then(|r| r.strip_suffix('}'))
                .or_else(|| inner.strip_prefix('$'))
                .and_then(plain_name)
            {
                return Some(var_read(name));
            }
            if inner.contains('$')
                || inner.contains('`')
                || inner.contains('\\')
                || inner.contains('"')
            {
                return None;
            }
            return Some(IrExpr::Str(inner.to_string(), StrStyle::DoubleQuoted));
        }
    }
    // An unquoted literal path: no expansion, no glob, no separators that the
    // test grammar gives a meaning to.
    if t.contains(['$', '`', '*', '?', '[', ']', '~', '\\', '{', '}', '(', ')', '\'', '"']) {
        return None;
    }
    Some(IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted))
}

/// Split a test text into its whitespace-separated tokens. The test grammar's
/// only quoting is around operands (no `\ `), so whitespace is the separator.
fn tokens(text: &str) -> Option<Vec<&str>> {
    let v: Vec<&str> = text.split_whitespace().collect();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// The lift itself. `args` is the `test` Call's argument list; the first
/// element is the test TEXT (a `Str`), the optional second is the style.
pub fn lift_test(args: &[IrExpr]) -> Option<IrExpr> {
    if args.len() > 2 {
        return None;
    }
    let text = match args.first()? {
        IrExpr::Str(s, _) => s.as_str(),
        _ => return None,
    };
    let toks = tokens(text)?;
    match toks.as_slice() {
        [op, p] if *op == "-N" => {
            if !offered("FsModifiedSinceRead") {
                return None;
            }
            Some(IrExpr::Ext(Box::new(
                crate::shir_nodes::FsModifiedSinceRead { path: operand(p)? },
            )))
        }
        [l, op, r] if matches!(*op, "-ef" | "-nt" | "-ot") => {
            let (left, right) = (operand(l)?, operand(r)?);
            match *op {
                "-ef" => {
                    if !offered("FsSameFile") {
                        return None;
                    }
                    Some(IrExpr::Ext(Box::new(crate::shir_nodes::FsSameFile {
                        left,
                        right,
                    })))
                }
                _ => {
                    if !offered("FsNewerThan") {
                        return None;
                    }
                    Some(IrExpr::Ext(Box::new(crate::shir_nodes::FsNewerThan {
                        left,
                        right,
                        newer: *op == "-nt",
                    })))
                }
            }
        }
        _ => None,
    }
}

/// Walk the program and lift every test predicate that [`lift_test`] accepts.
/// The statement walker visits CONDITIONS too — `text_ops` lowers statement
/// bodies only, so an `if [ a -nt b ]` would otherwise never reach the lift.
///
/// Only the `test`-function shape lifts. The COMMAND spelling
/// (`exec("test", ["a", "-ef", "b"])`) is refused: its condition renderer
/// runs the command for its exit status, and an Ext node in that position is
/// evaluated as a string with the status write dropped — measured, it turned
/// `parse-ef-test-operator` equivalence-red (`if test "$A" -ef "$B"` printed
/// "same file" where bash printed nothing). A refused lift is better than a
/// silently wrong program.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= walk_stmt(st);
    }
    changed
}

fn lift_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            if func == "test" {
                if let Some(r) = lift_test(args) {
                    *e = r;
                    return true;
                }
            }
            let mut c = false;
            for a in args.iter_mut() {
                c |= lift_expr(a);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            let a = lift_expr(lhs);
            let b = lift_expr(rhs);
            a || b
        }
        IrExpr::Ternary { cond, then, else_ } => {
            let a = lift_expr(cond);
            let b = lift_expr(then);
            let c = lift_expr(else_);
            a || b || c
        }
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= lift_expr(x);
                }
            }
            c
        }
        IrExpr::Capture { expr, .. } => lift_expr(expr),
        IrExpr::Array(items) => {
            let mut c = false;
            for i in items.iter_mut() {
                c |= lift_expr(i);
            }
            c
        }
        IrExpr::Index { key, .. } => lift_expr(key),
        IrExpr::Arrow(body) => transform(body),
        _ => false,
    }
}

fn walk_stmts(stmts: &mut [IrStmt]) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= walk_stmt(s);
    }
    c
}

fn walk_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Expr(e) => lift_expr(e),
        IrStmt::Assign { expr, .. } => lift_expr(expr),
        IrStmt::Declare { init, .. } => init.as_mut().map(lift_expr).unwrap_or(false),
        IrStmt::Output { value, .. } => lift_expr(value),
        IrStmt::WriteFile { path, content, .. } => {
            let a = lift_expr(path);
            let b = lift_expr(content);
            a || b
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => lift_expr(e),
        IrStmt::Return(None) | IrStmt::Exit(None) => false,
        IrStmt::If { cond, then, elsifs, else_ } => {
            let mut c = lift_expr(cond);
            c |= walk_stmts(then);
            for (cd, b) in elsifs.iter_mut() {
                c |= lift_expr(cd);
                c |= walk_stmts(b);
            }
            c |= walk_stmts(else_);
            c
        }
        IrStmt::While { cond, body } => {
            let a = lift_expr(cond);
            let b = walk_stmts(body);
            a || b
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let a = walk_stmts(body);
            let b = lift_expr(cond);
            a || b
        }
        IrStmt::For { iter, body, .. } => {
            let a = lift_expr(iter);
            let b = walk_stmts(body);
            a || b
        }
        IrStmt::ForInit { init, cond, step, body } => {
            let mut c = walk_stmts(init);
            c |= lift_expr(cond);
            c |= walk_stmts(step);
            c |= walk_stmts(body);
            c
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => walk_stmts(b),
        IrStmt::Redirect { inner, .. } => walk_stmts(inner),
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = walk_stmts(body);
            for (_, b) in named_blocks.iter_mut() {
                c |= walk_stmts(b);
            }
            c
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut c = lift_expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= walk_stmts(&mut cl.body);
            }
            c
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut c = walk_stmts(body);
            for ex in excepts.iter_mut() {
                c |= walk_stmts(&mut ex.body);
            }
            c |= walk_stmts(else_body);
            c |= walk_stmts(finally_body);
            c
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }
    fn test(text: &str) -> Option<IrExpr> {
        // The lift answers the MANIFEST, and the manifest is only ever the
        // Rust one for these nodes — so the test states the target rather than
        // relying on an unset target meaning "everything".
        crate::transforms::set_target(Some("rs"));
        let r = lift_test(&[st(text)]);
        crate::transforms::set_target(None);
        r
    }

    fn tag_of(e: &Option<IrExpr>) -> Option<&'static str> {
        match e {
            Some(IrExpr::Ext(n)) => Some(n.tag()),
            _ => None,
        }
    }

    #[test]
    fn quoted_vars_lift_to_same_file() {
        assert_eq!(tag_of(&test("\"$A\" -ef \"$B\"")), Some("FsSameFile"));
        assert_eq!(tag_of(&test("$a -ef ${b}")), Some("FsSameFile"));
    }

    #[test]
    fn literal_paths_lift() {
        assert_eq!(tag_of(&test("\"/path\" -ef \"/other\"")), Some("FsSameFile"));
        assert_eq!(tag_of(&test("a -nt b")), Some("FsNewerThan"));
        assert_eq!(tag_of(&test("a -ot b")), Some("FsNewerThan"));
        let Some(IrExpr::Ext(n)) = test("a -ot b") else { panic!() };
        let x = n.as_any().downcast_ref::<crate::shir_nodes::FsNewerThan>().unwrap();
        assert!(!x.newer, "-ot is not newer");
    }

    #[test]
    fn minus_n_lifts_one_operand() {
        assert_eq!(tag_of(&test("-N /dev/null")), Some("FsModifiedSinceRead"));
    }

    #[test]
    fn refusals_keep_the_test_call() {
        // Negation, `-a`/`-o`, parentheses: the grammar this lift does not own.
        assert!(test("! -e x -ef y").is_none());
        assert!(test("\"$A\" -ef \"$B\" -a -e c").is_none());
        // A glob or expansion in an operand.
        assert!(test("\"$dir\"/*.txt -ef \"/tmp/*.txt\"").is_none());
        assert!(test("\"${A[0]}\" -ef B").is_none());
        assert!(test("\"$1\" -ef B").is_none());
        assert!(test("\"pre$A\" -ef B").is_none());
        assert!(test("\"$(pwd)/x\" -ef B").is_none());
        // Operators with no node (the helper path is exact there).
        assert!(test("-O /dev/null").is_none());
        assert!(test("-G /dev/null").is_none());
        // Two operands with -N is not the one-operand shape.
        assert!(test("-N a b").is_none());
    }
}
