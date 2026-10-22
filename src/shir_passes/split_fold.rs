//! Trivial IFS word-split elimination: `split[e]` → `e` when `e` is
//! PROVABLY free of IFS whitespace.
//!
//! An unquoted expansion in bash yields the value's words; when the
//! value provably contains no IFS whitespace (space/tab/newline — the
//! `\s+` word-split set) the split produces exactly one field equal to
//! the value, so the node is a no-op. That is a fact about SHELL
//! semantics, not about any backend, which is why the rewrite lives in
//! the shared shIR→shIR layer: every renderer (C, Perl, Go, Rust, the
//! ESTree/JS path, …) inherits it without its own fold.
//!
//! The ESTree renderer already folds this at render time from the same
//! evidence (`expr_known_nospace` + the custom-IFS gate in
//! `shir.rs`), and the Python backend folds `split[getVar(x)]`
//! unconditionally. This pass is the shared, evidence-gated
//! canonicalization of that fold — the estree render-time fold becomes
//! redundant once the IR it sees has no such nodes left (it stays
//! harmlessly idempotent for the A1-ingress paths that skip the pass).
//!
//! Soundness:
//! - The verdict used is conservative: `shir::analyze_var_nospace`'s
//!   per-var fixpoint (every assignment RHS provably whitespace-free,
//!   no runtime-store writes) plus the A2 `Int` type verdict — a var
//!   homed as an integer has canonical decimal text, and decimal text
//!   never contains IFS whitespace. Literals, arithmetic, template
//!   literals and `${#x}` lengths are whitespace-free by shape.
//! - A program that may set a CUSTOM IFS is never rewritten
//!   (`shir::program_may_custom_ifs`): with a non-whitespace IFS a
//!   whitespace-free value can still split into several fields.
//!   Whitespace-only IFS assignments are fine (they are the default
//!   split set).
//! - **Element-position carve-out.** A split whose value is STORED
//!   INTO AN ARRAY (`setArray`/`setArrayAppend` element values,
//!   array-literal declares, indexed-assign RHS) is folded only when
//!   the target array already carries an A2 `Int` verdict. The
//!   rewrite is value-preserving either way, but it removes the
//!   string-store evidence the C ingest uses to home an array, and
//!   that flips a `char**` array to `long long*` — while the C
//!   renderer's array-SLICE path (`${arr[@]:0:10}`) is implemented
//!   only for string arrays (real crash: `051_primes` died at its
//!   final slice echoes with the fold on). Folding only where the
//!   array is already int-homed adds no new evidence and cannot flip
//!   anything: the store just matches the existing convention.
//! - Only the single-argument `split[e]` shape is folded, and the
//!   inner expression is rewritten first, so nested splits fold from
//!   the inside out.
//!
//! Marketplace (PLAN §11): shared transform, offered to all backends;
//! consumed by wiring one call at any post-verdict pipeline point
//! (the C path wires it in `bash-o4/src/pipeline.rs`'s O3/O4 pre-render
//! stage next to `loop_opt`/`int_cycles`).

use crate::ir::{IrExpr, IrProgram, IrStmt, IrType, InterpPart};
use std::collections::HashSet;

/// Fold provable-no-op word splits program-wide. Returns true when it
/// changed anything. Idempotent: a second call returns false.
pub fn fold_trivial_splits(prog: &mut IrProgram) -> bool {
    // A custom IFS makes the split consult runtime separator chars:
    // nothing is foldable (same gate the ESTree fold uses).
    if crate::shir::program_may_custom_ifs(prog) {
        return false;
    }
    // The shared "No spaces" verdicts (populated at A1 export; compute
    // here when a caller hands over an IR program without them).
    if prog.var_nospace.is_empty() {
        prog.var_nospace = crate::shir::analyze_var_nospace(prog);
    }
    let tagged: HashSet<String> = prog
        .var_nospace
        .iter()
        .filter(|(_, b)| *b)
        .map(|(n, _)| n.clone())
        // A conservatively-int-typed var reads back as canonical
        // decimal text — whitespace-free by shape (Float excluded:
        // its formatting is backend-defined, so no proof).
        .chain(prog.var_types.iter().filter_map(|(n, t)| match t {
            IrType::Int | IrType::Int32 | IrType::Int64 | IrType::UInt32 | IrType::UInt64 => {
                Some(n.clone())
            }
            _ => None,
        }))
        .collect();
    // Int-homed vars (the same family the `tagged` chain folds in):
    // an array with this verdict already stores native integers, so
    // folding an element-position split cannot flip its storage
    // class (the carve-out in the module doc).
    let int_vars: HashSet<String> = prog
        .var_types
        .iter()
        .filter_map(|(n, t)| match t {
            IrType::Int | IrType::Int32 | IrType::Int64 | IrType::UInt32 | IrType::UInt64 => {
                Some(n.clone())
            }
            _ => None,
        })
        .collect();
    let mut f = Fold {
        tagged: &tagged,
        int_vars: &int_vars,
        changed: false,
    };
    f.stmts(&mut prog.stmts);
    for sub in &mut prog.subs {
        f.stmts(&mut sub.body);
    }
    f.changed
}

/// Shared fold state: the verdict sets plus the change flag.
struct Fold<'a> {
    tagged: &'a HashSet<String>,
    int_vars: &'a HashSet<String>,
    changed: bool,
}

impl Fold<'_> {
    fn stmts(&mut self, stmts: &mut [IrStmt]) {
        walk_stmts(stmts, self)
    }
}

fn whitespace_free(s: &str) -> bool {
    !s.chars()
        .any(|c| c == ' ' || c == '\t' || c == '\n' || c == '\r')
}

/// Is `e`'s value provably free of IFS whitespace? (Local mirror of
/// `shir.rs::expr_nospace`, keyed on this pass's tag set.)
fn expr_nospace(e: &IrExpr, tagged: &HashSet<String>) -> bool {
    match e {
        IrExpr::Int(_) => true,
        // A numeric result is decimal text.
        IrExpr::Arith(_) | IrExpr::BinOp { .. } => true,
        IrExpr::Str(s, _) => whitespace_free(s),
        IrExpr::Var(name, _) | IrExpr::Ident(name) => tagged.contains(name),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(s) => whitespace_free(s),
            InterpPart::Expr(x) => expr_nospace(x, tagged),
        }),
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => matches!(args.first(), Some(IrExpr::Str(n, _)) if tagged.contains(n)),
            "arith" => true,
            // `${#x}` — a length is decimal digits.
            "param" => matches!(args.first(), Some(IrExpr::Str(op, _)) if op == "len"),
            _ => false,
        },
        _ => false,
    }
}

fn fold_expr(e: &mut IrExpr, f: &mut Fold, elem_of: Option<&str>) {
    match e {
        IrExpr::Call { func, args } => {
            // Rewrite the inner expression(s) first (nested splits fold
            // from the inside out).
            if func == "setArray" || func == "setArrayAppend" {
                // args[0] is the array NAME (a scalar position); args[1..]
                // are ELEMENT VALUES (the ir.rs census shape) — those get
                // the element context so the carve-out gate applies.
                if let Some((name, rest)) = args.split_first_mut() {
                    fold_expr(name, f, None);
                    let key = match &*name {
                        IrExpr::Str(n, _) => Some(n.clone()),
                        // Unresolvable name: a sentinel absent from
                        // int_vars gates every element fold.
                        _ => Some(String::new()),
                    };
                    for a in rest.iter_mut() {
                        fold_expr(a, f, key.as_deref());
                    }
                }
            } else {
                for a in args.iter_mut() {
                    fold_expr(a, f, elem_of);
                }
            }
            // Element-position gate (module-doc carve-out): an already
            // int-homed array folds (its convention cannot flip); any
            // other array keeps the split so its string-store evidence
            // — and its string-array slice rendering — survive.
            let elem_ok = elem_of.map(|n| f.int_vars.contains(n)).unwrap_or(true);
            if func == "split" && elem_ok && args.len() == 1 && expr_nospace(&args[0], f.tagged) {
                let inner = args.remove(0);
                *e = inner;
                f.changed = true;
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            fold_expr(obj, f, elem_of);
            for a in args.iter_mut() {
                fold_expr(a, f, elem_of);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            fold_expr(cond, f, elem_of);
            fold_expr(then, f, elem_of);
            fold_expr(else_, f, elem_of);
        }
        IrExpr::DefinedOr { expr, default } => {
            fold_expr(expr, f, elem_of);
            fold_expr(default, f, elem_of);
        }
        IrExpr::Index { key, .. } => fold_expr(key, f, elem_of),
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    fold_expr(x, f, elem_of);
                }
            }
        }
        IrExpr::Capture { expr, .. } => fold_expr(expr, f, elem_of),
        IrExpr::Arrow(stmts) => walk_stmts(stmts, f),
        IrExpr::Array(items) => {
            for it in items.iter_mut() {
                fold_expr(it, f, elem_of);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            fold_expr(iter, f, elem_of);
            fold_expr(elem, f, elem_of);
            if let Some(c) = cond {
                fold_expr(c, f, elem_of);
            }
        }
        IrExpr::Lambda { body, .. } => walk_stmts(body, f),
        _ => {}
    }
}

/// Full statement walk (the `optimize.rs` pattern): every statement
/// shape that carries expressions or nested bodies. A shape omitted
/// here merely misses a fold — never miscompiles — but the list is kept
/// exhaustive against `IrStmt`'s current variants.
fn walk_stmts(stmts: &mut [IrStmt], f: &mut Fold) {
    for s in stmts.iter_mut() {
        match s {
            IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => fold_expr(e, f, None),
            IrStmt::WriteFile { path, content, .. } => {
                fold_expr(path, f, None);
                fold_expr(content, f, None);
            }
            IrStmt::Assign { targets, expr, .. } => {
                // The indices are scalar reads — fold freely. An
                // indexed-assign RHS is an ELEMENT STORE, so it carries
                // the target array's name into the carve-out gate (the
                // same rule as setArray; ir.rs's `+=`/indexed shapes).
                for t in targets.iter_mut() {
                    for ix in t.indices.iter_mut() {
                        fold_expr(ix, f, None);
                    }
                }
                let elem = targets
                    .iter()
                    .find(|t| !t.indices.is_empty())
                    .map(|t| t.var.as_str());
                fold_expr(expr, f, elem);
            }
            IrStmt::Declare { init: Some(i), .. } => fold_expr(i, f, None),
            IrStmt::DeclareArray { var, elements, .. } => {
                for e in elements.iter_mut() {
                    fold_expr(e, f, Some(var));
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                fold_expr(cond, f, None);
                walk_stmts(then, f);
                for (c, b) in elsifs.iter_mut() {
                    fold_expr(c, f, None);
                    walk_stmts(b, f);
                }
                walk_stmts(else_, f);
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                walk_stmts(body, f);
                for e in excepts.iter_mut() {
                    walk_stmts(&mut e.body, f);
                }
                walk_stmts(else_body, f);
                walk_stmts(finally_body, f);
            }
            IrStmt::For { iter, body, .. } => {
                fold_expr(iter, f, None);
                walk_stmts(body, f);
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                walk_stmts(init, f);
                fold_expr(cond, f, None);
                walk_stmts(step, f);
                walk_stmts(body, f);
            }
            IrStmt::While { cond, body } => {
                fold_expr(cond, f, None);
                walk_stmts(body, f);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                walk_stmts(body, f);
                fold_expr(cond, f, None);
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                fold_expr(expr, f, None);
            }
            IrStmt::Case {
                discriminant,
                clauses,
                ..
            } => {
                fold_expr(discriminant, f, None);
                for c in clauses.iter_mut() {
                    walk_stmts(&mut c.body, f);
                }
            }
            IrStmt::Redirect { inner, .. } => walk_stmts(inner, f),
            IrStmt::Function {
                body,
                named_blocks,
                ..
            } => {
                walk_stmts(body, f);
                for (_, nb) in named_blocks.iter_mut() {
                    walk_stmts(nb, f);
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                walk_stmts(body, f)
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages.iter_mut() {
                    walk_stmts(st, f);
                }
            }
            IrStmt::Select { clauses } => {
                for c in clauses.iter_mut() {
                    if let Some(ch) = &mut c.ch {
                        fold_expr(ch, f, None);
                    }
                    if let Some(v) = &mut c.value {
                        fold_expr(v, f, None);
                    }
                    walk_stmts(&mut c.body, f);
                }
            }
            IrStmt::Exec {
                cmd,
                args,
                redirects,
                env,
                ..
            } => {
                fold_expr(cmd, f, None);
                for a in args.iter_mut() {
                    fold_expr(a, f, None);
                }
                for r in redirects.iter_mut() {
                    fold_expr(r, f, None);
                }
                for (_, v) in env.iter_mut() {
                    fold_expr(v, f, None);
                }
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
                fold_expr(e, f, None)
            }
            IrStmt::Block(b) => walk_stmts(b, f),
            // Remaining shapes (Ext/Asm/RawText/Goto/Label/…/Require)
            // carry no foldable value expressions: a missed shape can
            // only leave a fold on the table, never miscompile one.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir_json_in::shir_json_to_ir;

    fn prog(json: &str) -> IrProgram {
        shir_json_to_ir(json).expect("ir json")
    }

    const HEAD: &str = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":["#;

    /// An Int-typed var: the split of its read folds.
    #[test]
    fn folds_int_typed_var_read() {
        let src = format!(
            r#"{HEAD}{{"type":"Assign","targets":[{{"var":"i","sigil":null,"indices":[]}}],"expr":{{"type":"Int","value":5}}}}]}}"#
        );
        let mut p = prog(&src);
        p.var_types.push(("i".to_string(), IrType::Int));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("i".to_string(), crate::ir::StrStyle::DoubleQuoted)],
            }],
        }));
        assert!(fold_trivial_splits(&mut p));
        match &p.stmts.last() {
            Some(IrStmt::Expr(IrExpr::Call { func, .. })) => assert_ne!(func, "split"),
            _ => panic!("stmt vanished or not a call"),
        }
        // idempotent
        assert!(!fold_trivial_splits(&mut p));
    }

    /// An UNKNOWN var keeps the split (no proof, no rewrite).
    #[test]
    fn keeps_unknown_var() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("x".to_string(), crate::ir::StrStyle::DoubleQuoted)],
            }],
        }));
        assert!(!fold_trivial_splits(&mut p));
        assert!(matches!(
            &p.stmts[0],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "split"
        ));
    }

    /// A whitespace-containing literal keeps the split.
    #[test]
    fn keeps_whitespace_literal() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Str("a b".to_string(), crate::ir::StrStyle::DoubleQuoted)],
        }));
        assert!(!fold_trivial_splits(&mut p));
    }

    /// A spaceless literal folds (shape proof, no var involved).
    #[test]
    fn folds_spaceless_literal() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Str("42".to_string(), crate::ir::StrStyle::DoubleQuoted)],
        }));
        assert!(fold_trivial_splits(&mut p));
        assert!(matches!(
            &p.stmts[0],
            IrStmt::Expr(IrExpr::Str(s, _)) if s == "42"
        ));
    }

    /// A pre-populated `var_nospace` verdict (the shared analysis's own
    /// output) tags the var: non-Int numeric-text vars fold too.
    #[test]
    fn uses_var_nospace_verdict() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.var_nospace.push(("seed_txt".to_string(), true));
        p.var_nospace.push(("plain_txt".to_string(), false));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("seed_txt".to_string(), crate::ir::StrStyle::DoubleQuoted)],
            }],
        }));
        assert!(fold_trivial_splits(&mut p));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("plain_txt".to_string(), crate::ir::StrStyle::DoubleQuoted)],
            }],
        }));
        assert!(!fold_trivial_splits(&mut p));
    }

    /// A program that may set a custom IFS never folds.
    #[test]
    fn custom_ifs_blocks_the_fold() {
        let src = format!(
            r#"{HEAD}{{"type":"Assign","targets":[{{"var":"IFS","sigil":null,"indices":[]}}],"expr":{{"type":"Str","value":":","style":"DoubleQuoted"}}}}]}}"#
        );
        let mut p = prog(&src);
        p.var_types.push(("i".to_string(), IrType::Int));
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "split".to_string(),
            args: vec![IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("i".to_string(), crate::ir::StrStyle::DoubleQuoted)],
            }],
        }));
        assert!(!fold_trivial_splits(&mut p));
    }

    /// Nested splits fold inside-out; functions (subs) are walked too.
    #[test]
    fn nested_split_and_subs() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.var_types.push(("i".to_string(), IrType::Int));
        p.subs.push(crate::ir::IrSub {
            name: "f".to_string(),
            params: vec![],
            body: vec![IrStmt::Expr(IrExpr::Call {
                func: "split".to_string(),
                args: vec![IrExpr::Call {
                    func: "split".to_string(),
                    args: vec![IrExpr::Call {
                        func: "getVar".to_string(),
                        args: vec![IrExpr::Str("i".to_string(), crate::ir::StrStyle::DoubleQuoted)],
                    }],
                }],
            })],
        });
        assert!(fold_trivial_splits(&mut p));
        assert!(matches!(
            &p.subs[0].body[0],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "getVar"
        ));
        assert!(!fold_trivial_splits(&mut p));
    }

    /// Element-position carve-out: `arr+=(split[i])` into a NON-int
    /// array keeps the split (051_primes regression: folding it flipped
    /// the C ingest's array homing to `long long*`, and the renderer's
    /// string-array-only slice path then crashed at `${arr[@]:0:10}`).
    #[test]
    fn keeps_element_split_of_non_int_array() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.var_types.push(("candidate".to_string(), IrType::Int));
        p.stmts.push(IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: "primes".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "setArrayAppend".to_string(),
                args: vec![
                    IrExpr::Str("primes".to_string(), crate::ir::StrStyle::DoubleQuoted),
                    IrExpr::Call {
                        func: "split".to_string(),
                        args: vec![IrExpr::Call {
                            func: "getVar".to_string(),
                            args: vec![IrExpr::Str(
                                "candidate".to_string(),
                                crate::ir::StrStyle::DoubleQuoted,
                            )],
                        }],
                    },
                ],
            },
            asm: None,
        });
        assert!(!fold_trivial_splits(&mut p));
        assert!(matches!(
            &p.stmts[0],
            IrStmt::Assign { expr: IrExpr::Call { func, .. }, .. } if func == "setArrayAppend"
        ));
    }

    /// The same element split FOLDS when the array is already int-homed
    /// (the carve-out's allowed side — no evidence flip is possible).
    #[test]
    fn folds_element_split_of_int_array() {
        let mut p = prog(&format!(r#"{HEAD}]}}"#));
        p.var_types.push(("candidate".to_string(), IrType::Int));
        p.var_types.push(("primes".to_string(), IrType::Int));
        p.stmts.push(IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: "primes".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "setArrayAppend".to_string(),
                args: vec![
                    IrExpr::Str("primes".to_string(), crate::ir::StrStyle::DoubleQuoted),
                    IrExpr::Call {
                        func: "split".to_string(),
                        args: vec![IrExpr::Call {
                            func: "getVar".to_string(),
                            args: vec![IrExpr::Str(
                                "candidate".to_string(),
                                crate::ir::StrStyle::DoubleQuoted,
                            )],
                        }],
                    },
                ],
            },
            asm: None,
        });
        assert!(fold_trivial_splits(&mut p));
        assert!(matches!(
            &p.stmts[0],
            IrStmt::Assign { expr: IrExpr::Call { func, args, .. }, .. }
                if func == "setArrayAppend" && matches!(&args[1], IrExpr::Call { func, .. } if func == "getVar")
        ));
    }

    /// End-to-end funnel wiring (the generalisation): until now the pass
    /// had NO caller in this tree, so every backend received provably
    /// no-op `split[e]` nodes. `ast_to_ir` — the single funnel all
    /// backends share — now folds them, while raw lowering (the pinned
    /// `F(S)_raw` boundary) still carries the marker untouched, and a
    /// var with no whitespace-free verdict keeps its split.
    #[test]
    fn ast_to_ir_folds_trivial_splits_at_the_shared_funnel() {
        let src = "x=abc\necho $x\n";
        let raw_cmds = crate::Parser::new(src).parse().expect("parse");
        let raw = crate::shir::ast_to_ir_raw(&raw_cmds);
        let raw_text = raw
            .stmts
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            raw_text.contains("func: \"split\""),
            "sanity: the split marker must exist before the fold: {raw_text}"
        );

        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let text = prog
            .stmts
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !text.contains("func: \"split\""),
            "no-op split survived the ast_to_ir funnel: {text}"
        );

        // No verdict for a runtime var → the split must survive (the
        // conservative direction the module doc promises).
        let cmds = crate::Parser::new("echo $x\n").parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let text = prog
            .stmts
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("func: \"split\""),
            "an unprovable split was folded: {text}"
        );
    }
}
