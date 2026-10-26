//! slice-identity — `${v:0}` is `v`: fold a zero-offset, empty-length
//! `param("slice", …)` into the plain parameter read.
//!
//! name: slice-identity
//! prereqs: [none — param("slice") comes straight from the parser's
//!   subscript lowering]
//! invariant: param("slice", NAME, OFF, LEN) folds to the plain read
//!   param("", NAME) ONLY when NAME is a plain identifier
//!   ([A-Za-z_][A-Za-z0-9_]*), OFF is a literal "0" or "" and LEN is a
//!   literal "" or absent. `${v:0:}` and `${v}` expand identically in
//!   every context string, list or assignment — the bash manual defines
//!   the offset form as reading the SAME parameter value. Everything
//!   else is refuse-on-doubt: non-empty lengths (a real substring),
//!   non-literal offsets, longer arg lists, and every indexed /
//!   positional / indirect / array-list spelling (`arr[@]`, `@`, `*`,
//!   `!p`, `#n` — names with anything but alphanumerics/underscore).
//! scope: offered to all backends (estree, c, go, java, perl, python,
//!   rust, sh, fish, zig, zsh, glsl)
//! updates: none (first offer)
//!
//! ## Need
//! The parser lowers every `${v:off:len}` subscript to a param("slice")
//! CALL, and each backend then re-derives — at every render — that
//! offset 0 with no length is a no-op:
//!   * Rust spins up the whole `__sh_substr` runtime helper
//!     (`let ch: Vec<char> … ch[o as usize..(o+l) as usize]`, with its
//!     own `as usize`/`as i64` casts) just to print `${s:0}` where
//!     `${s}` prints `s`;
//!   * Java builds `shSubstr(...)` into every program that touches it,
//!     perl emits `substr($v, 0)`, the emulating shells cut a byte
//!     range with `cut`.
//! Whether a shape is a no-op is a PROGRAM property, not a backend
//! detail — so the rewrite belongs in the shared shIR→shIR channel:
//! one fold retires the callout (and its helper definition) for every
//! backend at once, which is exactly the marketplace §11 shape — and it
//! also feeds later transforms (copy-propagation sees a plain read).
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS-gated, default ON —
//! empty = all). A pure local substitution of an already-lowered node:
//! no analysis counts on seeing slice-with-zero afterwards (the var
//! READ it recorded is preserved — param("", NAME) is the SUNDRY plain
//! read every analysis already keys).

use crate::ir::{InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
/// Signature matches `transforms::TransformFn` (`fn(&mut Vec<IrStmt>)`).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

fn stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr: rhs, .. } => expr(rhs),
        IrStmt::Declare { init: Some(e), .. } => expr(e),
        IrStmt::Expr(e) => expr(e),
        IrStmt::Output { value, .. } => expr(value),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => expr(e),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = expr(cond);
            c |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                c |= transform(eb);
            }
            c |= transform(else_);
            c
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            expr(cond) | transform(body)
        }
        IrStmt::For { iter, body, .. } => expr(iter) | transform(body),
        IrStmt::ForInit { init, cond, step, body } => {
            let mut c = transform(init);
            c |= expr(cond);
            c |= transform(step);
            c |= transform(body);
            c
        }
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = transform(body);
            for (_, b) in named_blocks.iter_mut() {
                c |= transform(b);
            }
            c
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => transform(b),
        IrStmt::Redirect { inner, .. } => transform(inner),
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for s in stages.iter_mut() {
                c |= transform(s);
            }
            c
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut c = transform(body);
            for e in excepts.iter_mut() {
                c |= transform(&mut e.body);
            }
            c |= transform(else_body);
            c |= transform(finally_body);
            c
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut c = expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::WriteFile { path, content, .. } => expr(path) | expr(content),
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= expr(e);
            }
            c
        }
        _ => false,
    }
}

fn expr(e: &mut IrExpr) -> bool {
    let mut c = match e {
        IrExpr::Index { key, .. } => expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => expr(lhs) | expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => expr(cond) | expr(then) | expr(else_),
        IrExpr::DefinedOr { expr: x, default } => expr(x) | expr(default),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= expr(x);
                }
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for i in items.iter_mut() {
                c |= expr(i);
            }
            c
        }
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = expr(obj);
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::Capture { expr: x, .. } => expr(x),
        IrExpr::Arrow(body) => transform(body),
        IrExpr::Lambda { body, .. } => transform(body),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut c = expr(iter) | expr(elem);
            if let Some(cd) = cond.as_mut() {
                c |= expr(cd);
            }
            c
        }
        IrExpr::Object(vals) => {
            let mut c = false;
            for (_, v) in vals.iter_mut() {
                c |= expr(v);
            }
            c
        }
        _ => false,
    };
    c |= fold(e);
    c
}

/// A name that is just an identifier can only be a plain parameter
/// reference — anything else (`arr[@]`, `!p`, `#n`, `@`, `*`, `1`, a
/// dotted/bracketed name, an interpolated string) keeps its slice
/// spelling, because the slice form then means something different from
/// the plain read).
fn is_plain_ident(n: &str) -> bool {
    let mut cs = n.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Fold one node: `param("slice", NAME, OFF, LEN)` → `param("", NAME)`
/// when NAME is a plain identifier, OFF is literal 0/"" and LEN is
/// literal ""/absent.
fn fold(e: &mut IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if func != "param" {
        return false;
    }
    // `slice` arrives as 3 args (op, name, off — length implicit, to the
    // end) or 4 (op, name, off, len). A 5th arg is the `(k)`-annotation
    // variant — refuse-on-doubt rather than reason about it here.
    if !(3..=4).contains(&args.len()) {
        return false;
    }
    let (IrExpr::Str(op, op_style), IrExpr::Str(name, name_style)) = (&args[0], &args[1]) else {
        return false;
    };
    if op != "slice" || !is_plain_ident(name) {
        return false;
    }
    // offset literal 0 (or empty = omitted 0)
    match &args[2] {
        IrExpr::Str(off, _) if off.is_empty() || off == "0" => {}
        _ => return false,
    }
    // length literal "" (or absent) — a NON-empty length (even "0":
    // `${v:0:0}` is the empty string) is a real substring op.
    if let Some(len) = args.get(3) {
        match len {
            IrExpr::Str(l, _) if l.is_empty() => {}
            _ => return false,
        }
    }
    // Rebuild as the parser's plain-read shape (op style preserved so the
    // export round-trip is byte-stable).
    *e = IrExpr::Call {
        func: "param".to_string(),
        args: vec![
            IrExpr::Str(String::new(), op_style.clone()),
            IrExpr::Str(name.clone(), name_style.clone()),
        ],
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn s(v: &str) -> IrExpr {
        IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)
    }

    fn slice(args: Vec<IrExpr>) -> IrExpr {
        let mut a = vec![s("slice")];
        a.extend(args);
        IrExpr::Call { func: "param".to_string(), args: a }
    }

    fn folded(e: &mut IrExpr) -> bool {
        expr(e)
    }

    fn is_plain(e: &IrExpr, name: &str) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                func == "param"
                    && args.len() == 2
                    && matches!(&args[0], IrExpr::Str(v, _) if v.is_empty())
                    && matches!(&args[1], IrExpr::Str(v, _) if v == name)
            }
            _ => false,
        }
    }

    #[test]
    fn zero_offset_empty_length_folds_to_plain_read() {
        let mut e = slice(vec![s("s"), s("0"), s("")]);
        assert!(folded(&mut e));
        assert!(is_plain(&e, "s"));
    }

    #[test]
    fn empty_offset_means_zero_and_folds() {
        // `${s::}` — offset omitted
        let mut e = slice(vec![s("s"), s(""), s("")]);
        assert!(folded(&mut e));
        assert!(is_plain(&e, "s"));
    }

    #[test]
    fn length_absent_to_end_folds_at_zero() {
        // 3-arg form: `${s:0}`
        let mut e = slice(vec![s("s"), s("0")]);
        assert!(folded(&mut e));
        assert!(is_plain(&e, "s"));
    }

    #[test]
    fn real_offset_is_kept() {
        // `${s:2}` — a genuine substring
        let mut e = slice(vec![s("s"), s("2"), s("")]);
        let before = format!("{e:?}");
        assert!(!folded(&mut e));
        assert_eq!(format!("{e:?}"), before);
    }

    #[test]
    fn real_length_is_kept() {
        // `${s:0:3}` — even at offset 0, a length is a real substring
        let mut e = slice(vec![s("s"), s("0"), s("3")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn zero_length_is_kept() {
        // `${s:0:0}` is the EMPTY string — not the plain read
        let mut e = slice(vec![s("s"), s("0"), s("0")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn negative_offset_is_kept() {
        // `${s: -2}` — last two chars
        let mut e = slice(vec![s("s"), s("-2"), s("")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn array_and_positional_names_are_kept() {
        for name in ["arr[@]", "arr[*]", "@", "*", "#arr", "!pre", "1", "x.y", ""] {
            let mut e = slice(vec![s(name), s("0"), s("")]);
            assert!(!folded(&mut e), "must keep slice for name {name:?}");
        }
    }

    #[test]
    fn non_literal_offset_is_kept() {
        let mut e = slice(vec![
            s("s"),
            IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![s("i")],
            },
            s(""),
        ]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn list_forms_with_at_offset_are_kept() {
        // `${arr[@]}` etc: the `@` offset is a LIST read, never identity
        let mut e = slice(vec![s("arr"), s("@"), s("")]);
        assert!(!folded(&mut e));
        // `${arr[@]:0}` — offset slot carries @… list semantics
        let mut e = slice(vec![s("arr[@]"), s("0"), s("")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn annotation_arity_is_kept() {
        // 5 args = the `(k)` annotation variant — refuse on doubt
        let mut e = slice(vec![s("s"), s("0"), s(""), s("k")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn traversal_reaches_nested_positions() {
        // inside a builtin call's word list
        let inner = slice(vec![s("s"), s("0"), s("")]);
        let mut st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![s("echo"), IrExpr::Array(vec![inner])],
        });
        assert!(stmt(&mut st));
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &st else {
            panic!("stmt shape changed")
        };
        let IrExpr::Array(items) = &args[1] else {
            panic!("expect array")
        };
        assert!(is_plain(&items[0], "s"), "slice folded through nesting");
    }

    #[test]
    fn transform_reports_false_when_nothing_matches() {
        let mut stmts = vec![IrStmt::Expr(slice(vec![s("s"), s("2"), s("")]))];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn transform_reports_true_and_rewrites() {
        let mut stmts = vec![IrStmt::Expr(slice(vec![s("s"), s("0"), s("")]))];
        assert!(transform(&mut stmts));
        let IrStmt::Expr(e) = &stmts[0] else {
            panic!()
        };
        assert!(is_plain(e, "s"));
    }

    /// End-to-end through the REAL pipeline: the parser lowers `${v:0}` to
    /// `param("slice", "v", "0", "")`, and `ast_to_ir` (which runs
    /// `transforms::apply` → the registered `slice-identity` entry) folds it
    /// back to the plain read. Proves the registry wiring, not just `fold()`.
    #[test]
    fn parser_to_registry_folds_zero_offset_slice() {
        // A dynamic def keeps `v` a real read after the slice-identity
        // fold (param("", "v")): with a literal def, copy-propagation's
        // param-fold would collapse the plain read to a constant too and
        // this test would stop exercising slice-identity's own fold.
        let cmds = crate::Parser::new("v=\"$1\"\necho \"${v:0}\"\n")
            .parse()
            .expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let mut saw_plain_v = false;
        let mut leftover: Vec<Vec<String>> = Vec::new();
        fn visit(e: &IrExpr, leftover: &mut Vec<Vec<String>>, plain: &mut bool) {
            match e {
                IrExpr::Call { func, args } => {
                    if func == "param" {
                        let vals: Vec<String> = args
                            .iter()
                            .filter_map(|a| match a {
                                IrExpr::Str(v, _) => Some(v.clone()),
                                _ => None,
                            })
                            .collect();
                        if vals.first().map(String::as_str) == Some("slice") {
                            leftover.push(vals);
                        } else if vals.len() == 2 && vals[0].is_empty() && vals[1] == "v" {
                            *plain = true;
                        }
                    }
                    for a in args {
                        visit(a, leftover, plain);
                    }
                }
                IrExpr::Array(items) => {
                    for i in items {
                        visit(i, leftover, plain);
                    }
                }
                IrExpr::Interpolate(parts) => {
                    for p in parts {
                        if let InterpPart::Expr(x) = p {
                            visit(x, leftover, plain);
                        }
                    }
                }
                IrExpr::MethodCall { obj, args, .. } => {
                    visit(obj, leftover, plain);
                    for a in args {
                        visit(a, leftover, plain);
                    }
                }
                IrExpr::BinOp { lhs, rhs, .. } => {
                    visit(lhs, leftover, plain);
                    visit(rhs, leftover, plain);
                }
                IrExpr::Index { key, .. } => visit(key, leftover, plain),
                IrExpr::Ternary { cond, then, else_ } => {
                    visit(cond, leftover, plain);
                    visit(then, leftover, plain);
                    visit(else_, leftover, plain);
                }
                IrExpr::Capture { expr, .. } => visit(expr, leftover, plain),
                _ => {}
            }
        }
        fn walk_stmts(
            stmts: &[IrStmt],
            leftover: &mut Vec<Vec<String>>,
            plain: &mut bool,
        ) {
            for st in stmts {
                match st {
                    IrStmt::Output { value, .. } | IrStmt::Expr(value) | IrStmt::Assign { expr: value, .. } => {
                        visit(value, leftover, plain)
                    }
                    IrStmt::If { cond, then, elsifs, else_, .. } => {
                        visit(cond, leftover, plain);
                        walk_stmts(then, leftover, plain);
                        for (c, b) in elsifs {
                            visit(c, leftover, plain);
                            walk_stmts(b, leftover, plain);
                        }
                        walk_stmts(else_, leftover, plain);
                    }
                    IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
                        visit(cond, leftover, plain);
                        walk_stmts(body, leftover, plain);
                    }
                    IrStmt::Declare { init: Some(e), .. } => visit(e, leftover, plain),
                    _ => {}
                }
            }
        }
        walk_stmts(&prog.stmts, &mut leftover, &mut saw_plain_v);
        assert!(
            leftover.is_empty(),
            "zero-offset slice must not survive the registry: {leftover:?}"
        );
        assert!(saw_plain_v, "expected the folded plain param(\"\", \"v\") read");

        // Idiom check on the final Perl: without the fold, the slice arm of
        // `param` (ir.rs) emits `substr($v, 0)` for `${v:0}`; the folded plain
        // read emits just `$v`. One less runtime call per site.
        let perl = crate::perl_generator_fixed::generate(&prog);
        assert!(
            !perl.contains("substr($v"),
            "substr($v, 0) callout must be retired by the fold:\n{perl}"
        );
        assert!(
            perl.contains(r"join(' ', $v)") || perl.contains(r"print($v"),
            "expected a direct $v read in the rendered Perl:\n{perl}"
        );
    }
}