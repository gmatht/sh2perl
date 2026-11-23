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
//! ## The one variable operand this lift accepts: our own scratch paths
//!
//! Process substitution appends a cleanup `rm -f $__ps_tmp0 …` to the block,
//! AFTER this transform has run in the registry, so the statement never
//! reached here and the Rust side shelled out `bash -c "rm -f …"` to delete
//! files the transform itself minted. `process_subst::rm_exec` therefore
//! RE-APPLIES this lift to that single cleanup statement (the same pattern
//! `producer_redirect` uses for late-appended `shir-pipeline-native` work),
//! and the lift accepts a bare `IrStmt::Exec` whose ONLY dynamic operands are
//! `__ps_tmp*` reads. That is sound by construction: those variables are
//! assigned by the native `mktemp` block (`native_capture_value`), so each
//! holds exactly one literal unique path — never a glob, never several words,
//! never empty. A user-written `rm -f "$f"` is still REFUSED (`liftable` and
//! `lift_cleanup` both only know plain literals or `__ps_tmp*`): `$f` may hold
//! a glob, a word list or the empty string, and the renderer would treat it
//! as one path.
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

/// A read of one of the process-substitution transform's own scratch paths.
/// The name prefix is the transform's contract (`process_subst::tmp_var`);
/// such a variable is only ever assigned the native `mktemp` result, i.e.
/// exactly one literal path.
fn ps_tmp_word(w: &IrExpr) -> bool {
    matches!(w, IrExpr::Var(n, _) | IrExpr::Ident(n) if n.starts_with("__ps_tmp"))
}

/// The late-appended process-substitution cleanup, as a BARE `Exec` (the
/// shape `process_subst::rm_exec` builds). `lift_stmt` only knows the
/// statement-position exec CALL, so that shape fell through to the shell-text
/// renderer. Every word must be a modelled `rm` flag or one of our own
/// `__ps_tmp*` reads, and at least one such read must be present — an
/// operand-less `rm -f` is already a no-op and is not worth a node.
fn lift_cleanup(st: &mut IrStmt) -> bool {
    let IrStmt::Exec { cmd, args, capture: None, redirects, env } = st else {
        return false;
    };
    if !redirects.is_empty() || !env.is_empty() {
        return false;
    }
    let IrExpr::Str(name, _) = cmd else {
        return false;
    };
    if name != "rm" {
        return false;
    }
    let mut saw_tmp = false;
    for w in args.iter() {
        match w {
            IrExpr::Str(s, _) => {
                if s == "--" {
                    continue;
                }
                let Some(rest) = s.strip_prefix('-') else {
                    return false; // a literal operand is the plain lift's business
                };
                if rest.is_empty() || !rest.chars().all(|c| "frRd".contains(c)) {
                    return false;
                }
            }
            other if ps_tmp_word(other) => saw_tmp = true,
            _ => return false,
        }
    }
    if !saw_tmp {
        return false;
    }
    *st = IrStmt::Expr(IrExpr::Ext(Box::new(crate::shir_nodes::FsCommand {
        cmd: name.clone(),
        argv: IrExpr::Array(args.clone()),
    })));
    true
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
    if lift_stmt(st) || lift_cleanup(st) {
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

    /// The process-subst cleanup is the only VARIABLE operand this lift
    /// accepts, and only because the transform minted the variable itself.
    #[test]
    fn lifts_the_own_cleanup_but_not_a_user_variable() {
        crate::transforms::set_target(Some("rs"));
        let cleanup = IrStmt::Exec {
            cmd: IrExpr::Str("rm".to_string(), StrStyle::DoubleQuoted),
            args: vec![
                IrExpr::Str("-f".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Var("__ps_tmp0".to_string(), None),
                IrExpr::Var("__ps_tmp1".to_string(), None),
            ],
            capture: None,
            redirects: vec![],
            env: vec![],
        };
        let mut stmts = vec![cleanup];
        assert!(transform(&mut stmts), "the scratch cleanup did not lift");
        assert!(has_node(&stmts));

        // A user variable may hold a glob, several words or "" — the node's
        // renderer treats an operand as ONE path, so the shell keeps it.
        let user = IrStmt::Exec {
            cmd: IrExpr::Str("rm".to_string(), StrStyle::DoubleQuoted),
            args: vec![
                IrExpr::Str("-f".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Var("f".to_string(), None),
            ],
            capture: None,
            redirects: vec![],
            env: vec![],
        };
        let mut stmts = vec![user];
        assert!(!transform(&mut stmts), "a user variable lifted");
        assert!(!has_node(&stmts));

        // An unmodelled flag on our own temp path is refused too.
        let interactive = IrStmt::Exec {
            cmd: IrExpr::Str("rm".to_string(), StrStyle::DoubleQuoted),
            args: vec![
                IrExpr::Str("-i".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Var("__ps_tmp0".to_string(), None),
            ],
            capture: None,
            redirects: vec![],
            env: vec![],
        };
        let mut stmts = vec![interactive];
        assert!(!transform(&mut stmts), "rm -i lifted");
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
