//! brace-fold — expand a literal `brace` call into its word LIST at IR
//! time, in list positions only.
//!
//! name: brace-fold
//! prereqs: [none — `Call { func: "brace" }` comes straight from the
//!   parser's brace-expansion lowering; args are
//!   [prefix-Str, Json(groups), Json(middles), suffix-Str]]
//! invariant: inside a LIST position — an `IrExpr::Array` element or a
//!   `DeclareArray` element — a brace call whose four args are all
//!   literal expands, with the CANONICAL `ir::brace_expand` (the very
//!   function the renderers call at render time), to one `Str` per
//!   expanded word, spliced in place. Every other position (bare
//!   assignment `v={a,b}`, conditions, redirect targets, Interpolate
//!   embeddings, non-literal args) is left untouched — refuse > guess.
//! scope: offered to all backends (estree, c, go, java, perl, python,
//!   rust, sh, fish, zig, zsh, glsl)
//! updates: none (first offer)
//!
//! ## Need
//! Every backend re-derives brace expansion at RENDER time from the
//! callout (ir.rs `render_brace_word`/`render_brace_iter`, per-backend
//! `brace_expand` copies in c/java/go/python/rust), and the metric counts
//! each site as a contract callout (52 across the 560-example corpus).
//! Worse, the word-position rendering is `join(' ', …)` — ONE joined
//! word — which is only bash-equivalent for join-shaped consumers:
//!
//!   * `printf 'P:%s|' {a,b}`  bash → `P:a|P:b|`   joined-perl → `P:a b|`
//!   * `set -- {1,2}; $#`      bash → 2             joined → 1
//!   * `words=({p,q}); ${#w[@]}` bash → 2           joined → 1
//!
//! (probe 2026-10-20, /tmp/braceprobe: `echo A:{a,b}` and for-iter are
//! correct today; the three shapes above diverge — latent because the
//! corpus only uses echo/for/iter shapes.) Expanding once, in list
//! positions, to N literal words is both the idiomatic IR (a word list
//! IS how the parser spells `echo a b`) and the bash-correct fanout: the
//! callout disappears for every backend (metric −52), the render-time
//! expanders keep their own callers, and the three latent divergences
//! become correct.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS env gate), BEFORE
//! `const-capture-fold` so the cascade fires in one pass:
//! `result=$(echo {1..5})` → splice → `$(echo 1 2 3 4 5)` → the capture
//! fold then folds the whole substitution to its literal (−brace −exec
//! −capture at once).

use crate::ir::{IrExpr, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s);
    }
    c
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Label(_) | IrStmt::Goto(_) | IrStmt::RawText(_) | IrStmt::Continue
        | IrStmt::Break | IrStmt::Require(_) => false,
        IrStmt::Output { value, .. } | IrStmt::Assign { expr: value, .. }
        | IrStmt::Expr(value) => transform_expr(value),
        IrStmt::WriteFile { path, content, .. } => {
            transform_expr(path) | transform_expr(content)
        }
        IrStmt::Declare { init, .. } => init.as_mut().map(transform_expr).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => splice_word_list(elements),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = transform_expr(cond);
            c |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                c |= transform_expr(ec);
                c |= transform(eb);
            }
            c | transform(else_)
        }
        IrStmt::For { iter, body, .. } => transform_expr(iter) | transform(body),
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, until: _ } => {
            transform_expr(cond) | transform(body)
        }
        IrStmt::ForInit { init, cond, step, body } => {
            let mut c = false;
            for i in init.iter_mut() {
                c |= transform_stmt(i);
            }
            c |= transform_expr(cond);
            for s in step.iter_mut() {
                c |= transform_stmt(s);
            }
            c | transform(body)
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut c = transform_expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => transform(b),
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stage in stages.iter_mut() {
                for s in stage.iter_mut() {
                    c |= transform_stmt(s);
                }
            }
            c
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut c = transform(inner);
            for r in redirects.iter_mut() {
                c |= transform_expr(&mut r.target);
            }
            c
        }
        // capture/redirects are left untouched (refuse > guess: a brace
        // in a redirect word is a question bash answers with its raw
        // text, not the fanout this fold asserts).
        IrStmt::Exec {
            cmd,
            args,
            env,
            capture: _,
            redirects: _,
        } => {
            let mut c = transform_expr(cmd);
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            for (_, v) in env.iter_mut() {
                c |= transform_expr(v);
            }
            c
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            e.as_mut().map(transform_expr).unwrap_or(false)
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. }
        | IrStmt::SetChildError(expr) => transform_expr(expr),
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = transform(body);
            for ex in excepts.iter_mut() {
                if let Some(m) = ex.match_expr.as_mut() {
                    c |= transform_expr(m);
                }
                c |= transform(&mut ex.body);
            }
            c |= transform(else_body);
            c | transform(finally_body)
        }
        IrStmt::Select { clauses } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                if let Some(ch) = cl.ch.as_mut() {
                    c |= transform_expr(ch);
                }
                if let Some(v) = cl.value.as_mut() {
                    c |= transform_expr(v);
                }
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            let mut c = false;
            for (_, e) in outputs.iter_mut().chain(inputs.iter_mut()) {
                c |= transform_expr(e);
            }
            c
        }
        IrStmt::Shaderize { body, .. } => transform(body),
        IrStmt::Ext(n) => {
            let mut c = false;
            for child in crate::shir_nodes::ExtNode::children_mut(&mut **n) {
                c |= transform_stmt(child);
            }
            c
        }
    }
}

fn transform_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Str(_, _)
        | IrExpr::Int(_)
        | IrExpr::Bool(_)
        | IrExpr::Var(_, _)
        | IrExpr::Regex { .. }
        | IrExpr::RawExpr(_)
        | IrExpr::Ident(_)
        | IrExpr::Json(_)
        | IrExpr::Range { .. }
        | IrExpr::Arith(_)
        | IrExpr::Ext(_) => false,
        // LIST position: a bare literal brace element expands in place.
        // This is the whole transform — every context that renders an
        // Array-of-words (for-iter, array literals, command arg lists)
        // then sees plain Str elements, which is the parser's own
        // spelling of literal words and therefore safe everywhere.
        IrExpr::Array(items) => splice_word_list(items),
        IrExpr::Arrow(stmts) => transform(stmts),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = transform_expr(obj);
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= transform_expr(v);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => transform_expr(lhs) | transform_expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            transform_expr(cond) | transform_expr(then) | transform_expr(else_)
        }
        IrExpr::DefinedOr { expr, default } => transform_expr(expr) | transform_expr(default),
        IrExpr::Index { key, .. } => transform_expr(key),
        // A brace call embedded in an interpolated word is REFUSED as a
        // splice candidate (the walk still descends for nesting): bash
        // expands braces on the RAW text before parameter expansion, so
        // the mixed-word semantics are an unproven different question.
        // The corpus has zero such sites.
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(e) = p {
                    c |= transform_expr(e);
                }
            }
            c
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut c = transform_expr(iter) | transform_expr(elem);
            if let Some(cond) = cond.as_mut() {
                c |= transform_expr(cond);
            }
            c
        }
        IrExpr::Lambda { body, .. } => transform(body),
        IrExpr::Splice(e) => transform_expr(e),
        IrExpr::Capture { expr, .. } => transform_expr(expr),
    }
}

/// Splice every literal-shape `brace` element of a word list into its
/// expanded words; recurse into the other elements. Returns whether
/// anything changed.
fn splice_word_list(items: &mut Vec<IrExpr>) -> bool {
    let has_brace = items.iter().any(is_literal_brace);
    if !has_brace {
        let mut c = false;
        for it in items.iter_mut() {
            c |= transform_expr(it);
        }
        return c;
    }
    let mut spliced = false;
    let mut out: Vec<IrExpr> = Vec::with_capacity(items.len());
    for it in items.drain(..) {
        match it {
            IrExpr::Call { func, args } if func == "brace" && is_literal_brace_call(&args) => {
                match expand_words(&args) {
                    Some(words) => {
                        spliced = true;
                        out.extend(words);
                    }
                    // Empty expansion (`{}` is literal text in bash):
                    // keep the call, refuse > guess.
                    None => out.push(IrExpr::Call { func, args }),
                }
            }
            other => {
                let mut o = other;
                let _ = transform_expr(&mut o);
                out.push(o);
            }
        }
    }
    *items = out;
    spliced
}

/// A brace call with the canonical all-literal four-arg shape:
/// [prefix-Str, Json(groups), Json(middles), suffix-Str].
fn is_literal_brace(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } if func == "brace" => is_literal_brace_call(args),
        _ => false,
    }
}

fn is_literal_brace_call(args: &[IrExpr]) -> bool {
    matches!(
        args,
        [IrExpr::Str(_, _), IrExpr::Json(_), IrExpr::Json(_), IrExpr::Str(_, _)]
    )
}

/// Expand to the word list via the CANONICAL `ir::brace_expand` (the
/// renderers' own function — a fold and a render-time expansion cannot
/// disagree). `None` = nothing to fold (empty expansion, refuse).
fn expand_words(args: &[IrExpr]) -> Option<Vec<IrExpr>> {
    let items = crate::ir::brace_expand(args);
    if items.is_empty() {
        return None;
    }
    Some(
        items
            .into_iter()
            .map(|s| IrExpr::Str(s, StrStyle::DoubleQuoted))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    fn fold(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "no-op for {src}");
        shir_to_shir_json(&prog)
    }

    fn assert_refused(src: &str) {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        let before = shir_to_shir_json(&prog);
        let changed = transform(&mut prog.stmts);
        let after = shir_to_shir_json(&prog);
        assert_eq!(before, after, "refused shape must be untouched: {src}");
        assert!(!changed, "unexpected change for {src}");
    }

    #[test]
    fn for_iter_range_folds_to_elements() {
        let json = fold("for i in {1..5}; do echo $i; done");
        assert!(!json.contains("\"brace\""), "brace call must be gone: {json}");
        for n in 1..=5 {
            assert!(json.contains(&format!("\"{n}\"")), "missing {n}: {json}");
        }
    }

    #[test]
    fn zero_padded_range_keeps_padding() {
        // bash: {00..04} → 00 01 02 03 04 (pad width from the first
        // operand) — the spliced items carry the padding.
        let json = fold("for i in {00..04}; do echo $i; done");
        for n in ["00", "01", "02", "03", "04"] {
            assert!(json.contains(&format!("\"{n}\"")), "missing {n}: {json}");
        }
        assert!(!json.contains("\"brace\""), "{json}");
    }

    #[test]
    fn echo_arg_fans_out() {
        // bash: echo A:{a,b} → "A:a A:b"
        let json = fold("echo A:{a,b}");
        assert!(!json.contains("\"brace\""), "{json}");
        assert!(json.contains("\"A:a\"") && json.contains("\"A:b\""), "{json}");
    }

    #[test]
    fn printf_arg_fans_out() {
        // bash: printf 'P:%s|' {a,b} → P:a|P:b| — TWO args after the
        // format. The old render-time join produced P:a b| (divergent;
        // probe /tmp/braceprobe 2026-10-20). The splice makes the arg
        // list literal: ['P:%s|', 'a', 'b'].
        let json = fold("printf 'P:%s|' {a,b}");
        assert!(!json.contains("\"brace\""), "{json}");
        assert!(
            json.contains("\"P:%s|\"") && json.contains("\"a\"") && json.contains("\"b\""),
            "{json}"
        );
    }

    #[test]
    fn set_positional_fans_out() {
        // bash: set -- {1,2} → $# = 2 (two words, not one joined).
        let json = fold("set -- {1,2}");
        assert!(!json.contains("\"brace\""), "{json}");
        assert!(json.contains("\"1\"") && json.contains("\"2\""), "{json}");
    }

    #[test]
    fn array_literal_expands_elements() {
        // bash: words=({p,q}) → ${#words[@]} = 2
        let json = fold("words=({p,q})");
        assert!(!json.contains("\"brace\""), "{json}");
        assert!(json.contains("\"p\"") && json.contains("\"q\""), "{json}");
    }

    #[test]
    fn descending_and_stride_ranges() {
        let json = fold("for i in {5..1}; do echo $i; done");
        for n in 1..=5 {
            assert!(json.contains(&format!("\"{n}\"")), "missing {n}: {json}");
        }
        let json = fold("for i in {1..9..2}; do echo $i; done");
        for n in ["1", "3", "5", "7", "9"] {
            assert!(json.contains(&format!("\"{n}\"")), "missing {n}: {json}");
        }
        assert!(!json.contains("\"brace\""), "{json}");
    }

    #[test]
    fn product_and_nested_expand() {
        // bash: {src,doc}/{x,y} → src/x src/y doc/x doc/y (middles)
        let json = fold("echo {src,doc}/{x,y}");
        for w in ["src/x", "src/y", "doc/x", "doc/y"] {
            assert!(json.contains(&format!("\"{w}\"")), "missing {w}: {json}");
        }
        // bash: x{a,{b,c}}y → xay xby xcy (nesting + prefix/suffix)
        let json = fold("echo x{a,{b,c}}y");
        for w in ["xay", "xby", "xcy"] {
            assert!(json.contains(&format!("\"{w}\"")), "missing {w}: {json}");
        }
        assert!(!json.contains("\"brace\""), "{json}");
    }

    #[test]
    fn capture_body_arg_folds() {
        // Cascades with const-capture-fold downstream (registered before
        // it): the exec arg list becomes literal words.
        let json = fold("result=$(echo {1..3})");
        assert!(!json.contains("\"brace\""), "{json}");
        for n in 1..=3 {
            assert!(json.contains(&format!("\"{n}\"")), "missing {n}: {json}");
        }
    }

    #[test]
    fn bare_assignment_is_refused() {
        // bash keeps `v={x,y}` LITERALLY (probe 2026-10-20) — the scalar
        // assignment position is not a list, so the call must survive.
        assert_refused("v={x,y}");
    }

    #[test]
    fn empty_expansion_is_refused() {
        // `{}` is literal text in bash — no groups, nothing to splice.
        assert_refused("echo {}");
    }

    #[test]
    fn non_literal_args_are_refused() {
        // A non-Str suffix (a getVar) means the args are not the
        // canonical all-literal shape — refuse rather than guess.
        let mut stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "echo".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Call {
                    func: "brace".into(),
                    args: vec![
                        IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
                        IrExpr::Json(serde_json::json!([[["a", "b"]]])),
                        IrExpr::Json(serde_json::json!([])),
                        // NOT a Str — a getVar suffix: refuse.
                        IrExpr::Call {
                            func: "getVar".into(),
                            args: vec![IrExpr::Str("v".into(), StrStyle::DoubleQuoted)],
                        },
                    ],
                }]),
            ],
        })];
        let before = format!("{stmts:?}");
        let changed = transform(&mut stmts);
        assert!(!changed, "non-literal shape must be refused");
        assert!(format!("{stmts:?}").contains("brace"), "call must survive");
        assert_eq!(before, format!("{stmts:?}"));
    }

    #[test]
    fn idempotent() {
        let commands =
            parse_commands_from_text("for i in {1..3}; do echo A:{x,y}; done").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts));
        let json = shir_to_shir_json(&prog);
        assert!(!json.contains("\"brace\""), "all brace calls folded: {json}");
        // The transform's own idempotency contract: a second pass reports
        // no change. (Full-JSON equality was dropped: `shir_to_shir_json`
        // computes the For `runs` flag through a process-global analysis
        // cache that sibling tests mutate mid-run — comparing it across
        // two serializations is racy in the parallel harness and says
        // nothing about this transform.)
        assert!(!transform(&mut prog.stmts), "second pass must be a no-op");
    }
}
