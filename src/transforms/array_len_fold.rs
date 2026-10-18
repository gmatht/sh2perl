//! array-len-fold — `${#arr[@]}` IS `arrayLen(arr)`: one node, not a
//! slice the length of a rope.
//!
//! name: array-len-fold
//! prereqs: [none — param("slice", …, "@", …) comes straight from the
//!   parser's subscript lowering, which tags the length read by keeping
//!   the `#` in the NAME slot (words.rs: "Keep the # prefix in the
//!   variable name so the generator can detect it")]
//! invariant: param("slice", "#NAME", "@", "") folds to
//!   arrayLen(NAME) ONLY when NAME is a plain identifier
//!   ([A-Za-z_][A-Za-z0-9_]*). The parser emits this exact shape for
//!   `${#NAME[@]}` / `${#NAME[*]}` — bash's ELEMENT COUNT of the list —
//!   and arrayLen is the contract's element-count node (universal
//!   contract §primitives), so the rewrite is a renaming, not a
//!   semantic change. Every other spelling is refuse-on-doubt: a name
//!   without `#` (a whole-LIST read, not a length), `#` with a
//!   non-`@` offset (`${#arr:2}` is a substring of the count!), a
//!   non-empty or non-literal length, `*` in the offset slot, and any
//!   name that is not `#` + plain identifier.
//! scope: offered to all backends (estree, c, go, java, perl, python,
//!   rust, sh, fish, zig, zsh, glsl)
//! updates: none (first offer)
//!
//! ## Need
//! `${#arr[@]}` lowers as a SLICE — `param("slice", "#arr", "@", "")`
//! — so every backend that has not written its own `#`-peephole runs
//! the full substring machinery (offset parse, negative-length clamp
//! algebra, index casts) just to count a list. Measured on the corpus:
//! 9 sites reach the shared pipeline untouched, and the idiomatic
//! renderings differ per backend precisely because the SHAPE never
//! said "length" — estree re-derives it with an `array_len_join`
//! special case, the rust word-sink re-derives it with a `strip_prefix('#')`
//! special case, and a backend that has neither emits the dance.
//! Whether a shape is a length is a PROGRAM property, so the rewrite
//! belongs in the shared shIR→shIR channel: after the fold, a backend
//! needs only its ordinary `arrayLen` lowering — which every backend
//! already has because the contract node is in the universal
//! primitive list — and estree's native `name.length` rewrite now
//! matches the node that says what it is.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS-gated, default ON —
//! empty = all). A pure local substitution of an already-lowered node:
//! the var READ is preserved (arrayLen(NAME) is the same array touch
//! every analysis already keys; estree's var tracker records
//! `arrayLen` as a read exactly as it recorded the slice).

use crate::ir::{InterpPart, IrExpr, IrStmt, StrStyle};

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

/// A name that is just an identifier (after the `#`) can only be a
/// plain array reference — anything else (`arr[@]`, `@`, `*`, a
/// positional, a dotted name) keeps its slice spelling.
fn is_plain_ident(n: &str) -> bool {
    let mut cs = n.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Fold one node: `param("slice", "#NAME", "@", "")` → `arrayLen(NAME)`
/// when NAME is a plain identifier and the length slot is literal
/// ""/absent.
fn fold(e: &mut IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if func != "param" {
        return false;
    }
    // `slice` arrives as 3 args (op, name, off — length implicit) or 4
    // (op, name, off, len). A 5th arg is the `(k)`-annotation variant —
    // refuse-on-doubt rather than reason about it here.
    if !(3..=4).contains(&args.len()) {
        return false;
    }
    let (IrExpr::Str(op, _), IrExpr::Str(name, name_style)) = (&args[0], &args[1]) else {
        return false;
    };
    if op != "slice" {
        return false;
    }
    // The parser tags the `${#arr[@]}` length read by keeping the `#`
    // in the name slot; no `#` means a whole-LIST read (`${arr[@]}`),
    // which is NOT this fold.
    let Some(base) = name.strip_prefix('#') else {
        return false;
    };
    if !is_plain_ident(base) {
        return false;
    }
    // The offset slot must be the "@" list marker: `${#arr[@]}` /
    // `${#arr[*]}` — the parser normalises both to "@". A numeric
    // offset here would be a substring OF THE COUNT (`${#arr:2}` is a
    // 1-char string slice) — never a length.
    match &args[2] {
        IrExpr::Str(off, _) if off == "@" => {}
        _ => return false,
    }
    // Length slot literal "" (or absent) — a NON-empty length is a real
    // substring op over the count.
    if let Some(len) = args.get(3) {
        match len {
            IrExpr::Str(l, _) if l.is_empty() => {}
            _ => return false,
        }
    }
    // Rebuild as the contract's element-count node; the name style is
    // preserved so the export round-trip is byte-stable.
    *e = IrExpr::Call {
        func: "arrayLen".to_string(),
        args: vec![IrExpr::Str(base.to_string(), name_style.clone())],
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn is_len(e: &IrExpr, name: &str) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                func == "arrayLen"
                    && args.len() == 1
                    && matches!(&args[0], IrExpr::Str(v, _) if v == name)
            }
            _ => false,
        }
    }

    #[test]
    fn hash_name_list_count_folds_to_array_len() {
        // `${#arr[@]}`
        let mut e = slice(vec![s("#arr"), s("@"), s("")]);
        assert!(folded(&mut e));
        assert!(is_len(&e, "arr"));
    }

    #[test]
    fn hash_name_three_arg_form_folds() {
        // length slot absent
        let mut e = slice(vec![s("#arr"), s("@")]);
        assert!(folded(&mut e));
        assert!(is_len(&e, "arr"));
    }

    #[test]
    fn underscore_digit_names_fold() {
        let mut e = slice(vec![s("#_v1"), s("@"), s("")]);
        assert!(folded(&mut e));
        assert!(is_len(&e, "_v1"));
    }

    #[test]
    fn plain_name_is_a_list_read_not_a_length() {
        // `${arr[@]}` — a whole-LIST read, stays a slice
        let mut e = slice(vec![s("arr"), s("@"), s("")]);
        let before = format!("{e:?}");
        assert!(!folded(&mut e));
        assert_eq!(format!("{e:?}"), before);
    }

    #[test]
    fn numeric_offset_is_a_substring_of_the_count() {
        // `${#arr:2}` — count rendered then sliced; not a length
        let mut e = slice(vec![s("#arr"), s("2"), s("")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn star_offset_is_kept() {
        // the parser normalises `${#arr[*]}` to "@" — a "*" offset is
        // some other spelling; refuse on doubt
        let mut e = slice(vec![s("#arr"), s("*"), s("")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn non_empty_length_is_kept() {
        let mut e = slice(vec![s("#arr"), s("@"), s("1")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn non_literal_length_is_kept() {
        let mut e = slice(vec![s("#arr"), s("@"), IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![s("n")],
        }]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn non_literal_offset_is_kept() {
        let mut e = slice(vec![s("#arr"), IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![s("i")],
        }, s("")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn bare_hash_and_non_plain_bases_are_kept() {
        for name in ["#", "#arr[@]", "#a-b", "#1", "#@"] {
            let mut e = slice(vec![s(name), s("@"), s("")]);
            assert!(!folded(&mut e), "must keep slice for name {name:?}");
        }
    }

    #[test]
    fn annotation_arity_is_kept() {
        let mut e = slice(vec![s("#arr"), s("@"), s(""), s("k")]);
        assert!(!folded(&mut e));
    }

    #[test]
    fn non_slice_param_ops_are_kept() {
        let mut e = IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(""), s("#arr"), s("@"), s("")],
        };
        assert!(!folded(&mut e));
    }

    #[test]
    fn traversal_reaches_nested_positions() {
        let inner = slice(vec![s("#arr"), s("@"), s("")]);
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
        assert!(is_len(&items[0], "arr"), "fold reached through nesting");
    }

    #[test]
    fn transform_reports_false_when_nothing_matches() {
        let mut stmts = vec![IrStmt::Expr(slice(vec![s("#arr"), s("2"), s("")]))];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn transform_reports_true_and_rewrites() {
        let mut stmts = vec![IrStmt::Expr(slice(vec![s("#arr"), s("@"), s("")]))];
        assert!(transform(&mut stmts));
        let IrStmt::Expr(e) = &stmts[0] else {
            panic!()
        };
        assert!(is_len(e, "arr"));
    }
}
