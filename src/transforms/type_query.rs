//! `type NAME…` → the ANSWER for the names this PROGRAM defines.
//!
//! bash's `type` reports on the shell's own state; there is no `/usr/bin/
//! type`, so the pre-existing lowering printed NOTHING even when the
//! function existed — parse-eval-multiline's `type doselect 2>/dev/null |
//! head -1` printed an empty line where bash printed "doselect is a
//! function", and that example exists precisely to prove the eval'd
//! function DEFINES (once `select …; do` parses — see the parser's
//! select desugar).
//!
//! Shared shIR→shIR: the rewrite runs on the IR every backend consumes.
//! The parser path stores function definitions as `IrStmt::Function` in
//! the statement list, so this pass can see them; a program that defines
//! its own `type` shadows the builtin and the pass leaves every call
//! alone. A queried name that is neither a program function nor a true
//! bash builtin (an external, a dynamic word, a `type -t` flag) keeps
//! its original lowering untouched — PATH lookups and `-t` answers need
//! runtime state this pass does not model (refuse > guess).
//!
//! TIMING: the transform phase alone is not enough for eval'd code — the
//! perl generator's Fix 7 splices the eval'd program's functions into
//! the statement list later, so `perl_generator_fixed::fixups` calls
//! this transform again after that inlining. Any backend that inlines
//! eval text can call it at its own equivalent point.

use crate::ir::{IrExpr, IrStmt, StrStyle};

/// True bash BUILTINS — for `type`'s answer only. Deliberately NOT the
/// transpiler's wider builtins namespace (`transforms::builtin::BUILTINS`),
/// which also lists external coreutils like `head`/`ls`; `type ls` must
/// never claim "ls is a shell builtin".
const BASH_BUILTINS: &[&str] = &[
    "[", "alias", "bg", "bind", "builtin", "caller", "cd", "command", "compgen", "complete",
    "compopt", "continue", "declare", "dirs", "disown", "echo", "enable", "eval", "exec", "exit",
    "export", "false", "fc", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let",
    "local", "logout", "mapfile", "popd", "printf", "pushd", "pwd", "read", "readarray",
    "readonly", "return", "set", "shift", "shopt", "source", "suspend", "test", "times", "trap",
    "true", "type", "typeset", "ulimit", "umask", "unalias", "unset", "wait",
];

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut fns: Vec<String> = Vec::new();
    collect_functions(stmts, &mut fns);
    collect_eval_payload_functions(stmts, &mut fns);
    // A program-defined `type` SHADOWS the builtin — leave it all alone.
    if fns.iter().any(|n| n == "type") {
        return false;
    }
    rewrite_stmts(stmts, &fns)
}

/// Functions defined inside a literal `eval '…'` payload.
///
/// The payload is SHELL TEXT until the perl emitter re-parses it, and the
/// inliner (perl_generator_fixed Fix 7) deliberately refuses function
/// payloads — so at transform time the only way to know `doselect` will
/// exist is to parse the text ourselves, exactly as the rust backend does
/// for its `type` support. Unparseable or dynamic payloads contribute
/// nothing (the `type` call then keeps its original lowering).
fn collect_eval_payload_functions(stmts: &[IrStmt], out: &mut Vec<String>) {
    for s in stmts {
        let payload = match s {
            IrStmt::Expr(e) => eval_payload(e),
            IrStmt::Redirect { inner, .. } if inner.len() == 1 => match &inner[0] {
                IrStmt::Expr(e) => eval_payload(e),
                _ => None,
            },
            _ => None,
        };
        let Some(text) = payload else { continue };
        let Ok(cmds) = crate::parser::commands::parse(&text) else {
            continue;
        };
        if cmds.is_empty() {
            continue;
        }
        // raw — the payload is parsed for NAMES only here, but the same
        // no-optimization rule applies: nothing inside is dead.
        let inner = crate::shir::ast_to_ir_raw(&cmds);
        collect_functions(&inner.stmts, out);
    }
}

fn eval_payload(e: &IrExpr) -> Option<String> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "eval" {
        return None;
    }
    match words.as_slice() {
        [IrExpr::Str(text, _)] => Some(text.clone()),
        _ => None,
    }
}

fn collect_functions(stmts: &[IrStmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body, .. } => {
                out.push(name.clone());
                collect_functions(body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => collect_functions(b, out),
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_functions(then, out);
                for (_, b) in elsifs {
                    collect_functions(b, out);
                }
                collect_functions(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => collect_functions(body, out),
            IrStmt::ForInit { init, step, body, .. } => {
                collect_functions(init, out);
                collect_functions(step, out);
                collect_functions(body, out);
            }
            IrStmt::Redirect { inner, .. } => collect_functions(inner, out),
            _ => {}
        }
    }
}

fn rewrite_stmts(stmts: &mut Vec<IrStmt>, fns: &[String]) -> bool {
    let mut changed = false;
    for s in stmts.iter_mut() {
        match s {
            IrStmt::Expr(e) => {
                if rewrite_expr(e, fns) {
                    changed = true;
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                changed |= rewrite_stmts(b, fns)
            }
            IrStmt::If {
                then, elsifs, else_, ..
            } => {
                changed |= rewrite_stmts(then, fns);
                for (_, b) in elsifs.iter_mut() {
                    changed |= rewrite_stmts(b, fns);
                }
                changed |= rewrite_stmts(else_, fns);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                changed |= rewrite_stmts(body, fns)
            }
            IrStmt::ForInit { init, step, body, .. } => {
                changed |= rewrite_stmts(init, fns);
                changed |= rewrite_stmts(step, fns);
                changed |= rewrite_stmts(body, fns);
            }
            IrStmt::Function { body, .. } => changed |= rewrite_stmts(body, fns),
            IrStmt::Redirect { inner, .. } => changed |= rewrite_stmts(inner, fns),
            // `type f | head -1` — the call sits DEEP inside the
            // statement: Expr(Call{pipeline}) → Array(stage) →
            // Arrow(stage stmts) → Expr(Call{redirect}) → Arrow → the
            // type call itself (parse-eval-multiline). recurse_expr below
            // walks those expression children (pipeline stages, redirect
            // bodies) down to it; the echo replaces the call IN PLACE, so
            // the stage's own redirect (`2>/dev/null`) still wraps it.
            IrStmt::Pipeline { stages, .. } => {
                for st in stages.iter_mut() {
                    changed |= rewrite_stmts(st, fns);
                }
            }
            _ => {}
        }
    }
    changed
}

/// Rewrite one expression: the answer here, or descend into its children
/// (the depth is exactly where pipelines/redirects hide the call).
fn rewrite_expr(e: &mut IrExpr, fns: &[String]) -> bool {
    if let Some(answer) = type_call_answer(e, fns) {
        // the answer is always Call{builtin, ["echo", lines]} — splice the
        // expression form in, whichever nesting level fired
        if let IrStmt::Expr(inner) = answer {
            *e = inner;
            return true;
        }
        return false;
    }
    let mut changed = false;
    match e {
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            for a in args.iter_mut() {
                changed |= rewrite_expr(a, fns);
            }
        }
        IrExpr::Arrow(stmts) => changed |= rewrite_stmts(stmts, fns),
        _ => {}
    }
    changed
}

/// `exec/builtin("type", [names…])` → one `echo` of the answer lines, or
/// None to keep the call unchanged.
fn type_call_answer(e: &IrExpr, fns: &[String]) -> Option<IrStmt> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "type" || words.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for w in words {
        let IrExpr::Str(name, _) = w else {
            return None; // dynamic name — runtime state, refuse
        };
        if name.starts_with('-') {
            return None; // `type -t`/`-a` flag forms — not modelled
        }
        if fns.iter().any(|f| f == name) {
            lines.push(format!("{name} is a function"));
        } else if BASH_BUILTINS.contains(&name.as_str()) {
            lines.push(format!("{name} is a shell builtin"));
        } else {
            return None; // external/unknown — PATH is runtime state
        }
    }
    Some(IrStmt::Expr(IrExpr::Call {
        func: "builtin".to_string(),
        args: vec![
            IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(vec![IrExpr::Str(
                lines.join("\n"),
                StrStyle::DoubleQuoted,
            )]),
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fn_def(name: &str) -> IrStmt {
        IrStmt::Function {
            name: name.to_string(),
            params: vec![],
            body: vec![],
            named_blocks: vec![],
        }
    }

    fn type_call(names: &[&str]) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("type".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    names
                        .iter()
                        .map(|n| IrExpr::Str(n.to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        })
    }

    fn rendered_echo(stmts: &[IrStmt]) -> Option<String> {
        let IrStmt::Expr(IrExpr::Call { func, args }) = &stmts[0] else {
            return None;
        };
        if func != "builtin" {
            return None;
        }
        match &args[1] {
            IrExpr::Array(e) => match &e[0] {
                IrExpr::Str(s, _) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn type_of_defined_function_answers() {
        let mut stmts = vec![fn_def("doselect"), type_call(&["doselect"])];
        assert!(transform(&mut stmts));
        // the type call sits at index 1 (after the fn def) and is rewritten
        // IN PLACE — index 0 is still the definition
        assert_eq!(
            rendered_echo(&stmts[1..]).as_deref(),
            Some("doselect is a function")
        );
    }

    #[test]
    fn type_of_true_builtin_answers() {
        let mut stmts = vec![type_call(&["printf"])];
        assert!(transform(&mut stmts));
        assert_eq!(
            rendered_echo(&stmts).as_deref(),
            Some("printf is a shell builtin")
        );
    }

    #[test]
    fn type_of_external_stays_unchanged() {
        let mut stmts = vec![type_call(&["definitely_not_a_builtin_xyz"])];
        assert!(!transform(&mut stmts));
        assert!(matches!(&stmts[0], IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec"));
    }

    #[test]
    fn type_flag_forms_stay_unchanged() {
        let mut stmts = vec![type_call(&["-t", "printf"])];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn type_inside_pipeline_and_redirect_rewrites_in_place() {
        // real IR shape of `type f 2>/dev/null | head -1`:
        // Expr(pipeline [Arrow(stage0 = [Expr(redirect(Arrow([type call])))])])
        let type_call_stmt = type_call(&["doselect"]);
        let stage = IrStmt::Expr(IrExpr::Call {
            func: "redirect".to_string(),
            args: vec![
                IrExpr::Arrow(vec![type_call_stmt]),
                IrExpr::Array(vec![]),
            ],
        });
        let mut stmts = vec![
            fn_def("doselect"),
            IrStmt::Expr(IrExpr::Call {
                func: "pipeline".to_string(),
                args: vec![IrExpr::Array(vec![IrExpr::Arrow(vec![stage])])],
            }),
        ];
        assert!(transform(&mut stmts));
        // descend: pipeline Call → Array → Arrow → redirect Call → Arrow
        // → Expr(call)  — and the type call is now the echo
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmts[1] else {
            panic!("still a pipeline stmt")
        };
        let IrExpr::Array(stages) = &args[0] else {
            panic!("stage array")
        };
        let IrExpr::Arrow(st0) = &stages[0] else {
            panic!("stage arrow")
        };
        let IrStmt::Expr(IrExpr::Call { func: rq, args: ra }) = &st0[0] else {
            panic!("redirect wraps the stage")
        };
        assert_eq!(rq, "redirect", "the 2>/dev/null redirect stays");
        let IrExpr::Arrow(inner) = &ra[0] else {
            panic!("redirect body arrow")
        };
        let echo = rendered_echo(&inner).expect("type call became the echo");
        assert_eq!(echo, "doselect is a function");
    }

    #[test]
    fn user_defined_type_shadows_everything() {
        let mut stmts = vec![fn_def("type"), type_call(&["printf"])];
        assert!(!transform(&mut stmts));
        assert!(matches!(&stmts[1], IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec"));
    }

    #[test]
    fn mixed_known_and_unknown_keeps_the_call() {
        // bash would print the known line THEN the not-found error; a
        // partial rewrite would silently lose the error — refuse.
        let mut stmts = vec![fn_def("f"), type_call(&["f", "nope_xyz"])];
        assert!(!transform(&mut stmts));
    }
}
