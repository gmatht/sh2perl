//! const-status-cond — a `true` / `false` / `:` command in CONDITION
//! position is a CONSTANT: the loop/branch decision is known at translate
//! time. It is NOT dead code though — bash runs the command, and its
//! status is observable inside the body
//! (`until false; do echo "$?"; done` prints the condition's 1). So the
//! command does not disappear: it MOVES to the front of the body/branch
//! that runs, and the condition becomes the literal it always was.
//!
//! ## Need
//!
//! A command condition has no dedicated arm in the renderers, so each one
//! answers "does it succeed?" by RUNNING it:
//!
//! - **Perl** — `while true; do …; done` rendered
//!   `while ((system('bash', '-c', '"true"') == 0))`: a **fork per loop
//!   iteration** (8 corpus sites: parse-break-or, parse-done-unexpected,
//!   parse-heredoc-in-if, not-negation, 096_head_procsub, …). It is also
//!   the ONE path that runs a command without recording its status:
//!   `$CHILD_ERROR` is a plain variable (no `use English`), so the
//!   `system()` in a condition leaves it stale — `false; until false; do
//!   echo "u:$?"; break; done` printed `u:0` where bash prints `u:1`.
//! - **Go** — `for (runCmd(exec.Command("true")) == 0)`: same per-iteration
//!   fork.
//! - **C and Rust already do what this transform emits**:
//!   `while ((_sh_rc = 0, 1))` and `while { __SH_RC.store(0, …); true }` —
//!   a constant condition plus the status write. This pass lifts the
//!   other backends to that shape instead of each renderer inventing it.
//!
//! ## The bash rule the rewrite encodes
//!
//! For `if` / `while` / `until`, `$?` after the compound command is the
//! status of the last command run in the BODY, or **0 if the body never
//! ran** — never the condition's own status (`while false; do :; done;
//! echo $?` prints 0, not 1). Hence:
//!
//! | source | restructured IR |
//! |---|---|
//! | `while C; do B; done`, C succeeds (`true`/`:`) | `while 1 { C; B }` |
//! | `until C; do B; done`, C fails (`false`) | `while 1 { C; B }` |
//! | `while C; do B; done`, C fails | `true` (status 0; B provably never runs) |
//! | `until C; do B; done`, C succeeds | `true` (status 0; B provably never runs) |
//! | `if C; then T; fi`, C succeeds | `if 1 { C; T }` |
//! | `if C; then T; else E; fi`, C fails | `if 0 { C; T } else { C; E }` |
//! | `if C; then T; fi`, C fails (no else) | `true` (status 0) |
//!
//! `if 0 { C; T } else { C; E }` needs no special case downstream:
//! `const-condition-elim` prunes it to `{ C; E }`, exactly the status
//! sequence bash produces.
//!
//! ## Soundness (REFUSE > GUESS)
//!
//! - The command must be a bare `true` / `:` / `false` with NO words: a
//!   word can carry a side effect bash would still run (`true $(rm f)`),
//!   which a constant condition would drop.
//! - An `if` with ELSIF arms is refused: those conditions run between the
//!   constant one and the branch, so "which command ran last" is no longer
//!   the constant one's answer.
//! - `do … while C` (post-test) is refused: the body runs before the
//!   condition is consulted at all.
//! - A `Not` condition is the `until` / `! cmd` spelling the parser emits
//!   (both arms carry the same node); the renderers already read only the
//!   inner command, and so does this pass.
//!
//! ## Placement
//!
//! Registered in `transforms.rs` (shared — every backend), right before
//! `const-condition-elim`, which then prunes the constant `if`s this pass
//! leaves behind. The rewrite mutates each statement in place, so a loop
//! keeps its address (`sync-ok-loops`'s verdict keys) and
//! `const_true_like` still reports a `Bool(true)` loop as const-true.

use crate::ir::{BinOpKind, IrExpr, IrStmt};

/// A bare `true` / `:` / `false` command, and the status truth it always
/// produces. Returns the COMMAND NODE itself (owned — it is re-emitted as
/// a statement), its truth, and whether the condition inverts it
/// (`until` / `! cmd`).
fn status_const(e: &IrExpr) -> Option<(IrExpr, bool, bool)> {
    let (inner, negated) = match e {
        // `until C` / `while ! C` / `if ! C` — the parser writes the same
        // node into both Not arms; the renderers read `lhs` only, and so
        // do we.
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs,
            ..
        } => (lhs.as_ref(), true),
        other => (other, false),
    };
    let IrExpr::Call { func, args } = inner else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    // A WORD is refused: `true $(rm f)` still runs the substitution.
    if !words.is_empty() {
        return None;
    }
    let truth = match cmd.as_str() {
        "true" | ":" => true,
        "false" => false,
        _ => return None,
    };
    Some((inner.clone(), truth, negated))
}

/// The canonical no-op status exec (renders natively on every backend:
/// `$main_exit_code = $CHILD_ERROR = 0;` in Perl, `_sh_rc = 0;` in C,
/// `sh2.lastExit = 0;` in JS) — what a compound command whose body never
/// ran leaves behind in bash.
fn status_zero() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![
            IrExpr::Str("true".to_string(), crate::ir::StrStyle::Raw),
            IrExpr::Array(vec![]),
        ],
    })
}

/// The command as a statement, to run where bash ran it.
fn status_stmt(cmd: IrExpr) -> IrStmt {
    IrStmt::Expr(cmd)
}

/// Apply the transform to one statement list. Returns whether anything
/// changed. Nested lists are reached because every container (`if` arms,
/// loop bodies, blocks, …) is walked by the caller's own recursion.
fn rewrite(list: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in list.iter_mut() {
        match st {
            IrStmt::While { cond, body } => {
                let Some((cmd, truth, negated)) = status_const(cond) else {
                    continue;
                };
                // `while C` continues while C succeeds; `until C` while it fails.
                let enters = if negated { !truth } else { truth };
                if enters {
                    // The command runs before the body on EVERY iteration —
                    // which is where bash ran it.
                    *cond = IrExpr::Bool(true);
                    body.insert(0, status_stmt(cmd));
                } else {
                    // The body provably never runs; bash reports 0 for a
                    // compound command that ran no body command.
                    *st = status_zero();
                }
                changed = true;
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let Some((cmd, truth, negated)) = status_const(cond) else {
                    continue;
                };
                // With elsif arms the truth depends on conditions that run
                // after this one — refuse > guess.
                if !elsifs.is_empty() {
                    continue;
                }
                let runs_then = if negated { !truth } else { truth };
                if runs_then {
                    *cond = IrExpr::Bool(true);
                    then.insert(0, status_stmt(cmd));
                } else if else_.is_empty() {
                    // No branch runs: bash's compound status is 0.
                    *st = status_zero();
                } else {
                    // The else runs next, and reads the condition's status
                    // in its first command — so the command goes there.
                    *cond = IrExpr::Bool(false);
                    else_.insert(0, status_stmt(cmd));
                }
                changed = true;
            }
            _ => {}
        }
    }
    changed
}

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = rewrite(stmts);
    for st in stmts.iter_mut() {
        changed |= walk_stmt(st);
    }
    changed
}

/// Recurse into the nested statement lists (each gets its own
/// [`rewrite`]) AND into the statement's expressions: a process
/// substitution carries a whole loop in an OPERAND
/// (`head <(while true; do echo .; sleep 1; done)`), so a statement-only
/// walk never reaches it. The expression list mirrors the sibling
/// `shir-pipeline-native` walk.
fn walk_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = walk_expr(cond);
            c |= transform(then);
            for (ec, arm) in elsifs.iter_mut() {
                c |= walk_expr(ec);
                c |= transform(arm);
            }
            c | transform(else_)
        }
        IrStmt::For { iter, body, .. } => walk_expr(iter) | transform(body),
        IrStmt::While { cond, body } => walk_expr(cond) | transform(body),
        IrStmt::DoWhile { body, cond, .. } => transform(body) | walk_expr(cond),
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = walk_expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => transform(b),
        IrStmt::Redirect { inner, redirects } => {
            let mut c = transform(inner);
            for r in redirects.iter_mut() {
                c |= walk_expr(&mut r.target);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stage in stages.iter_mut() {
                c |= transform(stage);
            }
            c
        }
        IrStmt::Expr(e) => walk_expr(e),
        IrStmt::Output { value, .. } => walk_expr(value),
        IrStmt::Assign { expr, .. } => walk_expr(expr),
        IrStmt::Declare { init: Some(e), .. } => walk_expr(e),
        IrStmt::WriteFile { path, content, .. } => walk_expr(path) | walk_expr(content),
        IrStmt::Exec { cmd, args, env, .. } => {
            let mut c = walk_expr(cmd);
            for a in args.iter_mut() {
                c |= walk_expr(a);
            }
            for (_, v) in env.iter_mut() {
                c |= walk_expr(v);
            }
            c
        }
        _ => false,
    }
}

/// Recurse into an expression's statement-carrying parts (`Arrow` bodies,
/// call arguments, …).
fn walk_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arrow(stmts) => transform(stmts),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= walk_expr(a);
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for a in items.iter_mut() {
                c |= walk_expr(a);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= walk_expr(v);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => walk_expr(lhs) | walk_expr(rhs),
        IrExpr::Index { key, .. } => walk_expr(key),
        IrExpr::Capture { expr, .. } => walk_expr(expr),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;

    fn lower(src: &str) -> Vec<IrStmt> {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        let _ = transform(&mut prog.stmts);
        prog.stmts
    }

    /// The loop condition is now the literal it always was, and the
    /// condition command runs as the body's first statement (the status
    /// `until false; do echo "$?"; done` reads).
    #[test]
    fn until_false_becomes_an_infinite_loop_with_the_status_write_inside() {
        let stmts = lower("until false; do echo hi; done");
        assert_eq!(stmts.len(), 1, "one statement: {stmts:?}");
        let IrStmt::While { cond, body } = &stmts[0] else {
            panic!("expected a While, got {:?}", stmts[0]);
        };
        assert!(matches!(cond, IrExpr::Bool(true)), "cond: {cond:?}");
        let [IrStmt::Expr(IrExpr::Call { func, args }), IrStmt::Expr(_)] = body.as_slice() else {
            panic!("expected the status write then the body: {body:?}");
        };
        assert_eq!(func, "exec");
        assert!(matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "false"));
    }

    /// `while true` — the fork-per-iteration condition is gone.
    #[test]
    fn while_true_keeps_the_write_and_drops_the_call() {
        let stmts = lower("while true; do echo hi; done");
        let IrStmt::While { cond, body } = &stmts[0] else {
            panic!("expected a While: {stmts:?}");
        };
        assert!(matches!(cond, IrExpr::Bool(true)), "cond: {cond:?}");
        assert!(
            matches!(body.first(), Some(IrStmt::Expr(IrExpr::Call { func, args }))
                if func == "exec" && matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "true")),
            "body: {body:?}"
        );
    }

    /// A loop whose condition never succeeds runs no body: bash reports 0,
    /// and the body is provably dead.
    #[test]
    fn while_false_collapses_to_a_status_zero_statement() {
        let stmts = lower("while false; do echo hi; done");
        assert_eq!(stmts.len(), 1, "body must be dropped: {stmts:?}");
        assert!(is_status_zero(&stmts[0]), "got {:?}", stmts[0]);
    }

    /// `if ! true` — no branch runs, so the compound status is 0.
    #[test]
    fn if_negated_true_collapses_to_status_zero() {
        let stmts = lower("if ! true; then echo no; fi");
        assert_eq!(stmts.len(), 1, "got {stmts:?}");
        assert!(is_status_zero(&stmts[0]), "got {:?}", stmts[0]);
    }

    /// `if true; then A; else B; fi` — the then runs; the else stays.
    #[test]
    fn if_true_prefixes_the_then_branch() {
        let stmts = lower("if true; then echo a; else echo b; fi");
        let IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } = &stmts[0]
        else {
            panic!("expected an If: {stmts:?}");
        };
        assert!(matches!(cond, IrExpr::Bool(true)), "cond: {cond:?}");
        assert!(elsifs.is_empty());
        assert_eq!(then.len(), 2, "status write + echo: {then:?}");
        assert_eq!(else_.len(), 1);
    }

    /// `if false; then A; else B; fi` — the else runs NEXT, so it is the
    /// else that must observe the condition's status.
    #[test]
    fn if_false_prefixes_the_else_branch() {
        let stmts = lower("if false; then echo a; else echo b; fi");
        let IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } = &stmts[0]
        else {
            panic!("expected an If: {stmts:?}");
        };
        assert!(matches!(cond, IrExpr::Bool(false)), "cond: {cond:?}");
        assert_eq!(then.len(), 1, "then untouched: {then:?}");
        assert!(elsifs.is_empty());
        assert_eq!(else_.len(), 2, "status write + echo: {else_:?}");
    }

    /// `elif` arrives DESUGARED (a nested `if` in the else arm), so the
    /// top level has no elsif arms: `if true` still always takes the then,
    /// and prefixing it is exactly what bash does.
    #[test]
    fn elif_desugars_to_a_nested_if_and_the_then_still_runs() {
        let stmts = lower("if true; then echo a; elif [ 1 = 1 ]; then echo b; fi");
        let IrStmt::If { cond, then, .. } = &stmts[0] else {
            panic!("expected an If: {stmts:?}");
        };
        assert!(matches!(cond, IrExpr::Bool(true)), "cond: {cond:?}");
        assert_eq!(then.len(), 2, "status write + echo: {then:?}");
    }

    /// A genuine ELSIF LIST (a frontend shape) means the truth is decided
    /// by conditions that run after the constant one — refuse, keep the
    /// command condition.
    #[test]
    fn if_with_elsif_arms_is_refused() {
        let mut stmts = vec![IrStmt::If {
            cond: true_cmd(),
            then: vec![echo_stmt("a")],
            elsifs: vec![(test_cond(), vec![echo_stmt("b")])],
            else_: vec![],
        }];
        assert!(!transform(&mut stmts), "must not rewrite: {stmts:?}");
        let IrStmt::If { cond, .. } = &stmts[0] else {
            panic!("expected an If: {stmts:?}");
        };
        assert!(status_const(cond).is_some(), "cond kept: {cond:?}");
    }

    /// A bare `builtin true` command node (the shape `status_const` keys).
    fn true_cmd() -> IrExpr {
        IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("true".to_string(), StrStyle::Raw),
                IrExpr::Array(vec![]),
            ],
        }
    }

    fn echo_stmt(text: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::Raw),
                IrExpr::Array(vec![IrExpr::Str(text.to_string(), StrStyle::Raw)]),
            ],
        })
    }

    fn test_cond() -> IrExpr {
        IrExpr::Call {
            func: "test".to_string(),
            args: vec![IrExpr::Str("1 -eq 1".to_string(), StrStyle::Raw)],
        }
    }

    /// An ARGUMENT can carry a side effect (`true $(rm f)` still runs the
    /// substitution), so a command with words is refused.
    #[test]
    fn a_condition_command_with_words_is_refused() {
        let mut stmts = lower("while true x; do echo hi; done");
        assert!(!transform(&mut stmts), "must not rewrite: {stmts:?}");
    }

    /// A non-constant condition is untouched (the shell-out it already
    /// renders stays).
    #[test]
    fn a_real_command_condition_is_left_alone() {
        let mut stmts = lower("while grep -q x f; do echo hi; done");
        assert!(!transform(&mut stmts), "must not rewrite: {stmts:?}");
    }

    /// A process substitution carries the loop in an OPERAND
    /// (`head <(while true; do echo .; done)` — 096_head_procsub): the
    /// expression walk has to reach it, or the fork-per-iteration
    /// condition survives inside the fork block.
    #[test]
    fn a_loop_inside_a_process_substitution_operand_is_rewritten() {
        use crate::shir_json::shir_to_shir_json;
        let commands =
            parse_commands_from_text("head <(while true; do echo .; done)").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        // The pipeline order: `process-subst` materializes the producer
        // (the loop) out of the redirect's TEXT target first.
        let _ = crate::transforms::process_subst::transform(&mut prog.stmts);
        assert!(transform(&mut prog.stmts), "must rewrite: {:?}", prog.stmts);
        let json = shir_to_shir_json(&prog);
        let v: serde_json::Value = serde_json::from_str(&json).expect("json");
        assert!(
            no_command_cond(&v),
            "a command condition survived: {json}"
        );
    }

    /// No `cond` anywhere in the tree is still a `true`/`false`/`:` call.
    fn no_command_cond(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("cond").map(is_status_call) == Some(true) {
                    return false;
                }
                m.values().all(no_command_cond)
            }
            serde_json::Value::Array(a) => a.iter().all(no_command_cond),
            _ => true,
        }
    }

    fn is_status_call(v: &serde_json::Value) -> bool {
        v.get("type").and_then(|t| t.as_str()) == Some("Call")
            && matches!(v.get("func").and_then(|f| f.as_str()), Some("builtin" | "exec"))
            && v.get("args")
                .and_then(|a| a.as_array())
                .map(|args| {
                    matches!(
                        args.first().and_then(|a| a.get("value")).and_then(|s| s.as_str()),
                        Some("true" | "false" | ":")
                    ) && args
                        .get(1)
                        .and_then(|w| w.get("elements"))
                        .and_then(|e| e.as_array())
                        .is_none_or(|e| e.is_empty())
                })
                == Some(true)
    }

    /// Is `stmt` the canonical status-zero statement this pass emits?
    fn is_status_zero(stmt: &IrStmt) -> bool {
        let IrStmt::Expr(IrExpr::Call { func, args }) = stmt else {
            return false;
        };
        func == "exec" && matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "true")
    }
}
