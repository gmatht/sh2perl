//! text_ops_planner: the "solve" idea, observable end to end.
//!
//! A minimal, opt-in transform that lowers `echo TEXT | wc -l` to a
//! DIFFERENT primitive composition depending on the target backend's
//! manifest (docs/shir-primitives.md §"Backend node manifests + a core
//! planner"; PLAN §11.12). A regex-capable backend (js/perl) gets
//! `RegCount(text, /\n/)`; a no-regex backend (c) gets
//! `ArrayLen(Split(text, "\n"))`. The planner (transforms/planner.rs)
//! picks the composition; this transform just recognizes the simplest
//! `wc -l` pipeline and hands the input text to it.
//!
//! Opt-in via `SH2_TRANSFORMS=text-ops-planner`, and target-restricted via
//! `transform_applies` (only the backends that want the rich nodes). The
//! shell-like backends never see it — they keep the original command.
//!
//! This is a DEMONSTRATION of the mechanism, not the final semantics: it
//! handles only the literal `echo TEXT | wc -l` shape, and the
//! `ArrayLen(Split)` candidate is the doc's off-by-one composition (only
//! offered to backends that accept it). The full `text_ops` recognition
//! (streaming, grep-wc, find-wc, captures) is the follow-on.

use crate::ir::*;
use crate::transforms::planner;

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let enabled = std::env::var("SH2_TRANSFORMS").unwrap_or_default();
    if !enabled.split(',').any(|s| s.trim() == "text-ops-planner") {
        return false;
    }
    let cap = match crate::transforms::target().as_deref() {
        Some(t) => planner::manifest_for(t),
        None => return false, // no target → leave it (historical default)
    };
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        if let IrStmt::Expr(IrExpr::Call { func, args }) = stmt {
            if func == "pipeline" {
                if let Some(text) = recognize_pipeline_wc_l(args) {
                    if let Some(ir) = planner::build_wc_l(text, &cap) {
                        // `wc -l` PRINTS the count — wrap the value in an
                        // Output statement (newline-terminated), not a bare
                        // Expr (which would discard it).
                        *stmt = IrStmt::Output {
                            value: ir,
                            newline: true,
                            target: None,
                        };
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

/// Recognize the simplest `echo TEXT | wc -l` pipeline and return the input
/// text expression. The pipeline is an `Expr(Call { func: "pipeline",
/// args: [Array([Arrow(stage1), Arrow(stage2)])] })`. Returns None for any
/// other shape (fall back).
fn recognize_pipeline_wc_l(args: &[IrExpr]) -> Option<IrExpr> {
    let stages = match args.first() {
        Some(IrExpr::Array(items)) => items,
        _ => return None,
    };
    if stages.len() != 2 {
        return None;
    }
    let stage1 = match &stages[0] {
        IrExpr::Arrow(b) => b.as_slice(),
        _ => return None,
    };
    let stage2 = match &stages[1] {
        IrExpr::Arrow(b) => b.as_slice(),
        _ => return None,
    };
    // stage 1: `echo TEXT` — a builtin/exec call whose first arg is "echo".
    let text = match stage1 {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "builtin" || func == "exec" => {
            let name = str_arg(args, 0)?;
            if name != "echo" {
                return None;
            }
            // The text is the remaining args joined; for the simplest
            // case it is a single literal/interpolated string.
            let rest = args.get(1)?;
            match rest {
                IrExpr::Array(items) => {
                    if items.len() == 1 {
                        text_of(items[0].clone())
                    } else {
                        None
                    }
                }
                other => text_of(other.clone()),
            }
        }
        _ => None,
    }?;
    // stage 2: `wc -l` — a builtin/exec call whose first arg is "wc" and
    // whose flags include exactly `-l`.
    match stage2 {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "builtin" || func == "exec" => {
            let name = str_arg(args, 0)?;
            if name != "wc" {
                return None;
            }
            let flags: Vec<String> = args
                .get(1)
                .and_then(|a| match a {
                    IrExpr::Array(items) => Some(
                        items
                            .iter()
                            .filter_map(|i| match i {
                                IrExpr::Str(s, _) => Some(s.clone()),
                                _ => None,
                            })
                            .collect(),
                    ),
                    _ => None,
                })
                .unwrap_or_default();
            // exactly one mode, and it is -l
            let has_l = flags.iter().any(|f| f.contains('l'));
            let has_c = flags.iter().any(|f| f.contains('c'));
            let has_w = flags.iter().any(|f| f.contains('w'));
            if has_l && !has_c && !has_w {
                Some(text)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The first string argument of a call (the command name).
fn str_arg(args: &[IrExpr], i: usize) -> Option<&str> {
    match args.get(i) {
        Some(IrExpr::Str(s, _)) => Some(s.as_str()),
        _ => None,
    }
}

/// Extract a plain text expression from an array element / interpolate.
fn text_of(e: IrExpr) -> Option<IrExpr> {
    match e {
        IrExpr::Str(_, _) => Some(e),
        IrExpr::Interpolate(parts) if parts.len() == 1 => match &parts[0] {
            InterpPart::Lit(s) => Some(IrExpr::Str(s.clone(), StrStyle::DoubleQuoted)),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_wc_l_pipeline(text: &str) -> IrStmt {
        // `echo TEXT | wc -l` as an Expr(Call { func: "pipeline", … }).
        let echo = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Lit(
                    text.to_string(),
                )])]),
            ],
        });
        let wc = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("wc".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("-l".to_string(), StrStyle::DoubleQuoted)]),
            ],
        });
        IrStmt::Expr(IrExpr::Call {
            func: "pipeline".to_string(),
            args: vec![IrExpr::Array(vec![
                IrExpr::Arrow(vec![echo]),
                IrExpr::Arrow(vec![wc]),
            ])],
        })
    }

    #[test]
    fn recognizer_matches_echo_wc_l() {
        let stmt = echo_wc_l_pipeline("a\nb");
        if let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmt {
            let text = recognize_pipeline_wc_l(args).expect("recognizes echo TEXT | wc -l");
            assert_eq!(format!("{text:?}"), "Str(\"a\\nb\", DoubleQuoted)");
        } else {
            panic!("not a call");
        }
    }

    #[test]
    fn recognizer_rejects_non_wc_l() {
        // `echo X | wc -c` — not -l → None.
        let mut stmt = echo_wc_l_pipeline("x");
        if let IrStmt::Expr(IrExpr::Call { args, .. }) = &mut stmt {
            if let Some(IrExpr::Array(items)) = args.first_mut() {
                if let Some(IrExpr::Arrow(body)) = items.get_mut(1) {
                    if let Some(IrStmt::Expr(IrExpr::Call { args, .. })) = body.first_mut() {
                        if let Some(IrExpr::Array(flags)) = args.get_mut(1) {
                            flags[0] = IrExpr::Str("-c".to_string(), StrStyle::DoubleQuoted);
                        }
                    }
                }
            }
        }
        if let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmt {
            assert!(recognize_pipeline_wc_l(args).is_none());
        }
    }
}
