//! fs-cmd-lift — the file-mutating externals as ONE shIR node.
//!
//! `rm`, `mkdir` and `touch` were native in the Rust backend as a statement
//! arm (`native_fs_stmt`), but they were not NODES: a second renderer would
//! have had to re-derive the directory rule, the `-f` missing-path rule, the
//! diagnostic wording and the status aggregation. This module lifts a
//! STATEMENT-position `exec CMD …` into `FsCommand` (`shir_nodes/fs_command.node`),
//! whose Rust arm reuses that same statement renderer — one implementation of
//! the semantics, carried by the node.
//!
//! ## The sound rule
//!
//! The lift fires only when
//!  * the command word is `rm`, `mkdir` or `touch`;
//!  * EVERY word is a plain literal (`plain_word`: no glob, expansion, quote
//!    or backslash character) — the shell would resolve those at runtime, so
//!    they are refused rather than rendered;
//!  * every flag is one the renderer models (`rm`: f,r,R,d; `mkdir`: p,v;
//!    `touch`: none), and no `--long` form is accepted;
//!  * there is at least one word and no lone `-` operand;
//!  * the target backend DECLARES `FsCommand` in its manifest.
//!
//! Anything else keeps the exec Call, whose renderer is unchanged (an
//! unmodelled flag or a dynamic operand keeps its shell-out where it had one).
//!
//! `mktemp` and `sleep` are NOT in this lift: their observable value is the
//! result (the generated path, the elapsed time), not a statement side
//! effect, and both already render natively in the capture/bool positions
//! (`native_capture_value`, `call_bool`) with no spawn; a statement-position
//! `mktemp`/`sleep` is the value-less spelling of the same call.

use crate::ir::{IrExpr, IrStmt};

/// The node this lift produces (the manifest gate has to carry it).
pub const TAG: &str = "FsCommand";

fn offered() -> bool {
    let enabled = std::env::var("SH2_TRANSFORMS").unwrap_or_default();
    if enabled.split(',').any(|s| s.trim() == "fs-cmd-lift") {
        return true;
    }
    match crate::transforms::target() {
        Some(t) => crate::transforms::planner::node_manifest(&t).contains(&TAG),
        None => false,
    }
}

/// The characters a word may contain. A glob or an expansion in the word is
/// resolved by the shell at runtime (measured: `rm file_*.txt` is one literal
/// `Str` at IR time and expands to N paths in bash) and the node's renderer
/// refuses such a shape — so the lift must refuse it too, or the statement
/// would render as nothing.
fn plain_word(s: &str) -> bool {
    !s.is_empty()
        && s != "-"
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:,@=~".contains(c))
}

fn flags_ok(cmd: &str, words: &[&IrExpr]) -> bool {
    let allowed: &[char] = match cmd {
        "rm" => &['f', 'r', 'R', 'd'],
        "mkdir" => &['p', 'v'],
        _ => &[],
    };
    for w in words {
        let IrExpr::Str(s, _) = w else {
            return false; // a dynamic word is not this node's contract
        };
        if s == "--" {
            continue;
        }
        if let Some(rest) = s.strip_prefix('-') {
            if rest.is_empty() {
                continue; // a lone "-" is an operand, refused by `liftable`
            }
            if rest.starts_with('-') || !rest.chars().all(|c| allowed.contains(&c)) {
                return false;
            }
        }
    }
    true
}

fn liftable(cmd: &str, words: &[&IrExpr]) -> bool {
    matches!(cmd, "rm" | "mkdir" | "touch")
        && !words.is_empty()
        && words.iter().all(|w| matches!(w, IrExpr::Str(s, _) if plain_word(s)))
        && flags_ok(cmd, words)
}

fn lift_stmt(st: &mut IrStmt) -> bool {
    let stmt = st.clone();
    let IrStmt::Expr(IrExpr::Call { func, args }) = stmt else {
        return false;
    };
    if func != "exec" && func != "builtin" {
        return false;
    }
    let Some(IrExpr::Str(cmd, _)) = args.first() else {
        return false;
    };
    let cmd = cmd.clone();
    let words: Vec<IrExpr> = match args.get(1) {
        Some(IrExpr::Array(a)) => a.clone(),
        None => Vec::new(),
        _ => return false,
    };
    let refs: Vec<&IrExpr> = words.iter().collect();
    if !liftable(&cmd, &refs) {
        return false;
    }
    *st = IrStmt::Expr(IrExpr::Ext(Box::new(crate::shir_nodes::FsCommand {
        cmd,
        argv: IrExpr::Array(words),
    })));
    true
}

fn walk_stmts(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= walk_stmt(s);
    }
    c
}

fn walk_stmt(st: &mut IrStmt) -> bool {
    if lift_stmt(st) {
        return true;
    }
    match st {
        IrStmt::If { cond: _, then, elsifs, else_ } => {
            let mut c = walk_stmts(then);
            for (_, b) in elsifs.iter_mut() {
                c |= walk_stmts(b);
            }
            c |= walk_stmts(else_);
            c
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => walk_stmts(body),
        IrStmt::For { body, .. } => walk_stmts(body),
        IrStmt::ForInit { init, body, .. } => {
            let a = walk_stmts(init);
            let b = walk_stmts(body);
            a || b
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
        IrStmt::Case { clauses, .. } => {
            let mut c = false;
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

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if !offered() {
        return false;
    }
    walk_stmts(stmts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn exec(cmd: &str, words: &[&str]) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str(cmd.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    words
                        .iter()
                        .map(|w| IrExpr::Str((*w).to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        })
    }

    fn has_node(stmts: &[IrStmt]) -> bool {
        stmts
            .iter()
            .any(|s| matches!(s, IrStmt::Expr(IrExpr::Ext(n)) if n.tag() == TAG))
    }

    #[test]
    fn lifts_static_rm_mkdir_touch() {
        crate::transforms::set_target(Some("rs"));
        for (cmd, words) in [
            ("rm", ["-f", "a", "b"].as_slice()),
            ("rm", ["-rf", "dir"].as_slice()),
            ("rm", ["-d", "empty"].as_slice()),
            ("rm", ["gone.txt"].as_slice()),
            ("mkdir", ["-p", "a/b"].as_slice()),
            ("mkdir", ["-pv", "d"].as_slice()),
            ("mkdir", ["d"].as_slice()),
            ("touch", ["f"].as_slice()),
            ("touch", ["a", "b", "c"].as_slice()),
        ] {
            let mut stmts = vec![exec(cmd, words)];
            assert!(transform(&mut stmts), "{cmd} {words:?} did not lift");
            assert!(has_node(&stmts), "{cmd} {words:?} produced no node");
        }
        crate::transforms::set_target(None);
    }

    #[test]
    fn refuses_dynamic_and_unmodelled_shapes() {
        crate::transforms::set_target(Some("rs"));
        let dynamic = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("rm".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Var("f".to_string(), None)]),
            ],
        });
        for (name, stmt) in [
            ("variable operand", dynamic),
            ("glob operand", exec("rm", &["file_*.txt"])),
            ("unmodelled flag", exec("rm", &["-i", "f"])),
            ("long flag", exec("mkdir", &["--parents", "d"])),
            ("no operand", exec("rm", &[])),
            ("lone dash", exec("rm", &["-"])),
            ("sleep", exec("sleep", &["1"])),
            ("mktemp", exec("mktemp", &["-d"])),
            ("cat", exec("cat", &["f"])),
        ] {
            let mut stmts = vec![stmt];
            assert!(!transform(&mut stmts), "{name}: lifted a refused shape");
            assert!(!has_node(&stmts), "{name}: built the node anyway");
        }
        crate::transforms::set_target(None);
    }
}
