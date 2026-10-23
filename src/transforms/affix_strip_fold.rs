//! affix-strip-fold — `${v#lit}` is `strip_prefix(lit)`: one node, not a
//! glob engine.
//!
//! name: affix-strip-fold
//! prereqs: [none — param("#"/"##"/"%"/"%%", …) comes straight from the
//!   parser's parameter-expansion lowering (words.rs → ast_words.rs →
//!   shir.rs, which spells the operator as "#" / "##" / "%" / "%%" with
//!   the pattern in the 3rd slot)]
//! invariant: param(OP, NAME, PAT) folds to AffixStrip(text=param("",NAME),
//!   pattern=PAT, prefix) ONLY when OP ∈ {"#","##","%","%%"}, NAME is a
//!   plain identifier ([A-Za-z_][A-Za-z0-9_]*), PAT is a literal string
//!   carrying NO glob magic (`*`, `?`, `[`, `\`), and the target backend
//!   has a verified AffixStrip render arm (rust, go, java — the three
//!   whose CURRENT literal-strip render drags a glob/regex helper).
//!   For a magic-free literal, bash's SHORTEST and LONGEST match are
//!   identical (a literal matches exactly once, at the very front for
//!   `#`/`##` and at the very end for `%`/`%%`), so `#`≡`##` and `%`≡`%%`
//!   and each reduces to removing AT MOST ONE leading/trailing occurrence
//!   — which is precisely AffixStrip's semantics (`strip_prefix`/
//!   `strip_suffix` in Rust, `TrimPrefix`/`TrimSuffix` in Go,
//!   `startsWith`/`endsWith`+`substring` in Java). Everything else is
//!   refuse-on-doubt: a pattern with glob magic (the engine is REQUIRED
//!   then), a non-literal or interpolated pattern, a non-identifier NAME
//!   (`arr[i]`, `@`, `*`, `!p`, `#n` — positional/indirect/array-list
//!   spellings mean something else), the `/`/`//` substitute ops, and any
//!   target without a VERIFIED AffixStrip render (estree/perl/python
//!   already inline literals optimally; c/sh/zig have no AffixStrip arm;
//!   go's Ext path emits a `sh2.AffixStrip(...)` stub rather than routing
//!   to the shared handler, so it is refused until that backend accepts
//!   the node).
//! scope: offered to rust, java (target-gated, end-to-end verified). estree/
//!   perl/python keep their own inline literal render (already idiomatic);
//!   c/sh/zig/fish/zsh/glsl/go refuse (no verified AffixStrip render).
//! updates: none (first offer)
//!
//! ## Need
//! The parser lowers `${v#lit}` to a `param("#", v, lit)` CALL, and each
//! backend then re-derives — at every render — that a literal pattern
//! needs no glob engine:
//!   * Rust spins up `__sh_strippre`, whose helper table pulls in
//!     `__sh_fnmatch` (the full glob engine: `Vec<char>`, prefix/suffix
//!     enumeration, `fnmatch`-style matching) just to print `${v#git-}`
//!     where `v.strip_prefix("git-")` would do;
//!   * Go emits `pStrip` → `re.FindStringIndex` (the regex engine) for the
//!     same literal;
//!   * Java emits `shStripShortestPrefix` → its own glob matcher.
//! Whether a pattern is magic-free is a PROGRAM property, not a backend
//! detail — so the rewrite belongs in the shared shIR→shIR channel: one
//! fold retires the glob/regex helper (and its definition) for every
//! accepting backend at once, the marketplace §11 shape. Measured on the
//! emitted corpus: 14 literal strip sites across 9 files, of which 7 files
//! use ONLY literal strips (folding retires `__sh_fnmatch` entirely there).
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS-gated, default ON —
//! empty = all). A pure local substitution of an already-lowered node:
//! the var READ is preserved (AffixStrip.text is param("",NAME), the same
//! plain read every analysis already keys), so nothing that counted the
//! slice-with-pattern afterwards is stranded.

use crate::ir::{IrExpr, IrStmt, StrStyle};
use crate::shir_nodes::AffixStrip;

/// Apply the transform. Returns whether anything changed.
/// Signature matches `transforms::TransformFn` (`fn(&mut Vec<IrStmt>)`).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // Target gate: only the backends whose literal-strip render currently
    // drags a glob/regex helper AND that carry a verified AffixStrip arm.
    // Unset target (the shir embed path / non-CLI ingress) refuses — the
    // fold is a render-quality optimisation, never a semantic requirement.
    if !fold_target_ok() {
        return false;
    }
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

/// Targets whose AffixStrip render is VERIFIED end-to-end (the backend
/// routes `IrExpr::Ext` to a correct native arm) AND whose current
/// `param("#"/"##"/"%"/"%%")` render is heavier than it (a glob helper
/// for a magic-free literal):
///   * rust — now renders `strip_prefix`/`strip_suffix` (this change),
///     retiring `__sh_strippre`/`__sh_fnmatch` for literal patterns;
///   * java — routes Ext through render_ext_expr → startsWith/substring;
///   * estree/perl/python already inline literals natively
///     (startsWith/slice, s{}//r, removeprefix) — folding is neutral, so
///     they refuse (no shape churn for no gain).
/// go is NOT accepted yet: its `Ext` path emits a `sh2.AffixStrip(...)`
/// STUB (not a refusal) because the Go backend doesn't route Ext to the
/// shared handler — folding there would emit an unrunnable call. c/sh/zig
/// have no AffixStrip arm (loud refusal) and stay out.
fn fold_target_ok() -> bool {
    match crate::transforms::target() {
        Some(t) => matches!(t.as_str(), "rs" | "rust" | "java"),
        None => false,
    }
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
            for s in step.iter_mut() {
                c |= stmt(s);
            }
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
                if let crate::ir::InterpPart::Expr(x) = p {
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
/// reference — anything else (`arr[i]`, `arr[@]`, `!p`, `#n`, `@`, `*`,
/// `1`, a dotted/bracketed name) keeps its strip spelling, because the
/// operand then means a positional / indirect / array element, not a
/// scalar string.
fn is_plain_ident(n: &str) -> bool {
    let mut cs = n.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// A magic-free literal pattern: no glob metacharacter (`*`, `?`, `[`)
/// and no backslash escape. Such a pattern matches exactly one string, so
/// shortest ≡ longest and the strip removes at most ONE occurrence.
fn is_literal_pattern(p: &str) -> bool {
    !p.is_empty() && !p.contains('*') && !p.contains('?') && !p.contains('[') && !p.contains('\\')
}

/// Fold one node: `param(OP, NAME, PAT)` → `AffixStrip(param("",NAME),
/// PAT, prefix)` for OP ∈ {#,#,%,%} on a magic-free literal PAT.
fn fold(e: &mut IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if func != "param" {
        return false;
    }
    // The target gate lives HERE (not only in `transform`) so no entry
    // point — a direct `expr()`/`fold()` call, a nested pass — can bypass
    // it and fold for a backend without a verified AffixStrip arm.
    if !fold_target_ok() {
        return false;
    }
    // The strip form is exactly 3 args: (op, name, pattern). A 4th arg is
    // the `/`-substitute shape (`param("/", name, pat, repl)`) — refuse.
    if args.len() != 3 {
        return false;
    }
    let (IrExpr::Str(op, op_style), IrExpr::Str(name, _), IrExpr::Str(pat, pat_style)) =
        (&args[0], &args[1], &args[2])
    else {
        return false;
    };
    let prefix = match op.as_str() {
        "#" | "##" => true,
        "%" | "%%" => false,
        _ => return false,
    };
    if !is_plain_ident(name) || !is_literal_pattern(pat) {
        return false;
    }
    // Rebuild as AffixStrip(text = the plain read, pattern = the literal,
    // prefix). The plain read uses the SAME op-style slot the parser
    // filled, so the export round-trip stays stable.
    let text = IrExpr::Call {
        func: "param".to_string(),
        args: vec![
            IrExpr::Str(String::new(), op_style.clone()),
            IrExpr::Str(name.clone(), StrStyle::DoubleQuoted),
        ],
    };
    *e = IrExpr::Ext(Box::new(AffixStrip {
        text,
        pattern: IrExpr::Str(pat.clone(), pat_style.clone()),
        prefix,
    }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn s(v: &str) -> IrExpr {
        IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)
    }

    fn strip(op: &str, name: &str, pat: &str) -> IrExpr {
        IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(op), s(name), s(pat)],
        }
    }

    fn with_target<T>(t: Option<&str>, f: impl FnOnce() -> T) -> T {
        crate::transforms::set_target(t);
        let r = f();
        crate::transforms::set_target(None);
        r
    }

    fn is_affix(e: &IrExpr, prefix: bool, pat: &str) -> bool {
        match e {
            IrExpr::Ext(n) if n.tag() == "AffixStrip" => {
                let j = n.to_json();
                j["prefix"] == serde_json::Value::from(prefix)
                    && j["pattern"]["value"].as_str() == Some(pat)
            }
            _ => false,
        }
    }

    #[test]
    fn literal_prefix_strip_folds_on_rust() {
        with_target(Some("rs"), || {
            let mut e = strip("#", "v", "git-");
            assert!(expr(&mut e));
            assert!(is_affix(&e, true, "git-"));
        });
    }

    #[test]
    fn literal_suffix_strip_folds_on_java() {
        with_target(Some("java"), || {
            let mut e = strip("%", "v", "-branch");
            assert!(expr(&mut e));
            assert!(is_affix(&e, false, "-branch"));
        });
    }

    #[test]
    fn go_is_refused_until_its_ext_path_is_wired() {
        // Go's Ext fallback emits a sh2.AffixStrip stub (not a refusal),
        // so the fold must NOT fire there yet.
        with_target(Some("go"), || {
            let mut e = strip("#", "v", "lit");
            let before = format!("{e:?}");
            assert!(!expr(&mut e), "go must refuse until its Ext arm is wired");
            assert_eq!(format!("{e:?}"), before);
        });
    }

    #[test]
    fn greedy_and_shortest_are_identical_for_a_literal() {
        // `##` (longest) on a magic-free literal == `#` (shortest): the
        // fold maps both to the same single-occurrence AffixStrip.
        with_target(Some("java"), || {
            let mut e = strip("##", "v", "git-");
            assert!(expr(&mut e));
            assert!(is_affix(&e, true, "git-"));
            let mut e = strip("%%", "v", "-branch");
            assert!(expr(&mut e));
            assert!(is_affix(&e, false, "-branch"));
        });
    }

    #[test]
    fn glob_magic_pattern_is_kept() {
        // A `*`/`?`/`[` pattern NEEDS the glob engine — never fold.
        with_target(Some("rs"), || {
            for pat in ["*-", "*o", "a*b", "a?b", "[a-z]*", "*/"] {
                let mut e = strip("#", "v", pat);
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "must keep glob pattern {pat:?}");
                assert_eq!(format!("{e:?}"), before);
            }
        });
    }

    #[test]
    fn non_identifier_name_is_kept() {
        with_target(Some("rs"), || {
            for name in ["arr[i]", "arr[@]", "@", "*", "#arr", "!pre", "1", "x.y", ""] {
                let mut e = strip("#", name, "lit");
                assert!(!expr(&mut e), "must keep non-identifier name {name:?}");
            }
        });
    }

    #[test]
    fn substitute_op_is_kept() {
        // `/` and `//` carry a replacement arg and mean substitute, not
        // strip — never fold (and their arity differs anyway).
        with_target(Some("rs"), || {
            for op in ["/", "//"] {
                let mut e = IrExpr::Call {
                    func: "param".to_string(),
                    args: vec![s(op), s("v"), s("lit")],
                };
                assert!(!expr(&mut e), "must keep substitute op {op:?}");
            }
        });
    }

    #[test]
    fn non_affix_target_is_refused() {
        // estree/perl/python inline literals optimally; c/sh/zig have no
        // AffixStrip arm. All refuse — nothing changes.
        for t in ["js", "estree", "pl", "perl", "py", "python", "c", "sh", "zig", "go"] {
            with_target(Some(t), || {
                let mut e = strip("#", "v", "lit");
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "target {t} must refuse the fold");
                assert_eq!(format!("{e:?}"), before);
            });
        }
    }

    #[test]
    fn unset_target_is_refused() {
        with_target(None, || {
            let mut e = strip("#", "v", "lit");
            assert!(!expr(&mut e));
        });
    }

    #[test]
    fn traversal_reaches_nested_positions() {
        with_target(Some("rs"), || {
            let inner = strip("#", "v", "lit");
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
            assert!(is_affix(&items[0], true, "lit"), "folded through nesting");
        });
    }

    #[test]
    fn transform_reports_false_when_nothing_matches() {
        with_target(Some("rs"), || {
            let mut stmts = vec![IrStmt::Expr(strip("#", "v", "*o"))];
            assert!(!transform(&mut stmts));
        });
    }

    #[test]
    fn transform_reports_true_and_rewrites() {
        with_target(Some("rs"), || {
            let mut stmts = vec![IrStmt::Expr(strip("#", "v", "lit"))];
            assert!(transform(&mut stmts));
            let IrStmt::Expr(e) = &stmts[0] else {
                panic!()
            };
            assert!(is_affix(e, true, "lit"));
        });
    }

    /// End-to-end through the REAL pipeline: the parser lowers `${v#git-}`
    /// to `param("#", "v", "git-")` and, with the target set to rust,
    /// `ast_to_ir` (which runs the registry) folds it to AffixStrip. Proves
    /// the registry wiring, not just `fold()`.
    #[test]
    fn parser_to_registry_folds_literal_strip() {
        with_target(Some("rs"), || {
            let cmds = crate::Parser::new("v=git-x\necho \"${v#git-}\"\n")
                .parse()
                .expect("parse");
            let prog = crate::shir::ast_to_ir(&cmds);
            fn has_affix(e: &IrExpr) -> bool {
                match e {
                    IrExpr::Ext(n) if n.tag() == "AffixStrip" => true,
                    IrExpr::Call { args, .. } => args.iter().any(has_affix),
                    IrExpr::Array(items) => items.iter().any(has_affix),
                    IrExpr::Interpolate(parts) => parts
                        .iter()
                        .any(|p| matches!(p, crate::ir::InterpPart::Expr(x) if has_affix(x))),
                    _ => false,
                }
            }
            fn walk(sts: &[IrStmt]) -> bool {
                sts.iter().any(|s| match s {
                    IrStmt::Expr(e) | IrStmt::Output { value: e, .. } | IrStmt::Assign { expr: e, .. } => has_affix(e),
                    IrStmt::If { then, else_, .. } => walk(then) || walk(else_),
                    _ => false,
                })
            }
            assert!(
                walk(&prog.stmts),
                "literal strip must fold to AffixStrip through the registry"
            );
        });
    }
}
