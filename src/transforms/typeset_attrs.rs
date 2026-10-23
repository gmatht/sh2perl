//! typeset-attrs — persist the attributes bash attaches to a VARIABLE to
//! every LATER assignment to it: `-i` (integer) and `-l`/`-u` (case fold).
//!
//! ## Need
//! A declaration is not a one-shot assignment. It marks the name for the
//! rest of the scope, so every later assignment must honour the attribute:
//!
//! ```sh
//! typeset -i n=42   # declares n integer
//! n=n+1             # n = 43   (not the string "n+1")
//! n="hello"         # n = 0    (an unknown name is 0 in bash arithmetic)
//!
//! typeset -l lc="HELLO WORLD"   # lowercases the initial value
//! lc="ANOTHER TEST"             # lowercased AGAIN — the attribute is live
//! ```
//!
//! Both declaration sites were already lowered correctly, but the attribute
//! was dropped on the floor, so every later assignment stored the raw text:
//! `typeset-cmdsub.sh` printed `n='n+1'`, `n='hello'`, `lc='ANOTHER TEST'`
//! and `uc='another test'` where bash printed `43`, `0`, `another test` and
//! `ANOTHER TEST`.
//!
//! ## Scope
//! - `typeset|declare|local [-i…][-l|-u…] VAR` / `… VAR=VAL` records VAR's
//!   attributes (every name on the command line).
//! - A later single-target `VAR = EXPR` becomes `int(arith(EXPR))` and/or
//!   `lc(EXPR)` / `uc(EXPR)`.
//!
//! ## Why a transform and not a renderer arm
//! These are bash semantics, not a Perl rendering detail, so they belong in
//! the shared A1 channel where every backend inherits them. The rewrite
//! emits `arith`, `int`, `lc` and `uc` — existing neutral IR nodes each
//! backend already lowers to its own native arithmetic and case folding —
//! so no backend changes.
//!
//! ## Refuse > guess
//! - Only `typeset`/`declare`/`local` declarations are read: the flag words
//!   of any other builtin (`find -iname PAT`, `grep -i PAT`) do not declare
//!   variables, and mis-reading them would turn a later assignment into
//!   arithmetic.
//! - Only a bare single-target assignment (no indices, no sigil) to a
//!   recorded name is rewritten.
//! - The integer attribute needs the RHS as TEXT, so an expanded or
//!   interpolated RHS is left untouched (the existing path keeps handling
//!   it) — a partial rewrite would be a wrong answer, not a slower one.
//!   The case attribute needs none of that: the fold wraps the expression
//!   itself, exactly like bash folding the value AFTER expansion.
//! - `-l` and `-u` together are refused (which one wins is not something
//!   this pass is willing to guess).
//! - `readonly`, `nameref`, `export` and the array forms are deliberately
//!   not handled here — the declaration site owns those.

use crate::ir::{exec_word_args, AssignTarget, InterpPart, IrExpr, IrStmt, StrStyle};
use std::collections::HashMap;

/// One variable's live attributes.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Attr {
    int: bool,
    /// `Some('l')` / `Some('u')`.
    fold: Option<char>,
}

impl Attr {
    fn is_empty(&self) -> bool {
        !self.int && self.fold.is_none()
    }
}

/// `typeset [-i…][-l|-u…] VAR…` attributes → rewrite of later assignments.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut attrs: HashMap<String, Attr> = HashMap::new();
    let mut changed = false;
    for st in stmts.iter_mut() {
        for (name, a) in declarations(st) {
            let slot = attrs.entry(name).or_default();
            slot.int |= a.int;
            if slot.fold.is_none() {
                slot.fold = a.fold;
            }
        }
        changed |= apply_attributes(st, &attrs);
    }
    changed
}

/// The variable names a `typeset`/`declare`/`local` command declares, with
/// the attributes it pins on them. Anything not obviously a declaration
/// (another builtin, a flag we cannot honour, a non-word argument) yields
/// nothing rather than a guess.
fn declarations(st: &IrStmt) -> Vec<(String, Attr)> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return Vec::new();
    };
    if !matches!(func.as_str(), "builtin" | "exec") {
        return Vec::new();
    }
    // The command name: only the declaration builtins declare anything.
    // `args[0]` is the name, `args[1]` the word list (`exec_word_args`).
    let Some(IrExpr::Str(cmd, _)) = args.first() else {
        return Vec::new();
    };
    if !matches!(cmd.as_str(), "typeset" | "declare" | "local") {
        return Vec::new();
    }
    let mut attr = Attr::default();
    let mut names = Vec::new();
    for w in exec_word_args(args) {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                if s[1..].contains('i') {
                    attr.int = true;
                }
                let has_l = s[1..].contains('l');
                let has_u = s[1..].contains('u');
                // Both case attributes at once (`-lu`): refuse to guess
                // which one bash would apply last.
                if has_l && has_u {
                    return Vec::new();
                }
                if attr.fold.is_none() {
                    if has_l {
                        attr.fold = Some('l');
                    } else if has_u {
                        attr.fold = Some('u');
                    }
                }
            }
            IrExpr::Str(s, _) => {
                // `VAR=VAL` or bare `VAR`; the value is the declaration
                // site's business — only the name is recorded here.
                let name = s.split('=').next().unwrap_or("");
                if is_plain_name(name) {
                    names.push(name.to_string());
                }
            }
            // array initialisers, interpolated values — the name (if any)
            // is still worth recording, but not worth guessing at.
            _ => {}
        }
    }
    if attr.is_empty() || names.is_empty() {
        return Vec::new();
    }
    names.into_iter().map(|n| (n, attr)).collect()
}

/// `VAR = EXPR` for a recorded VAR → honour the attributes.
fn apply_attributes(st: &mut IrStmt, attrs: &HashMap<String, Attr>) -> bool {
    let IrStmt::Assign { targets, expr, .. } = st else {
        return false;
    };
    let [AssignTarget {
        var,
        sigil: None,
        indices,
    }] = targets.as_slice()
    else {
        return false;
    };
    if !indices.is_empty() {
        return false;
    }
    let Some(attr) = attrs.get(var) else {
        return false;
    };
    let mut out = expr.clone();
    if attr.int {
        let Some(text) = literal_text(&out) else {
            // The attribute is not merely a fold: the TEXT must be an
            // arithmetic expression, and this pass must not invent one.
            return false;
        };
        // A canonical signed decimal IS its value — fold it straight
        // to the literal, no runtime `arith` call for any backend to
        // render. Everything bash would read differently refuses: `042`
        // is OCTAL 34 there, `0x…`/`1_000` are other forms again, and
        // an i64 overflow belongs to the bigint runtime path. What
        // refuses keeps the exact shape below.
        out = match strict_int(&text) {
            Some(n) => IrExpr::Int(n),
            // `int(arith(EXPR))`: `arith` evaluates the expression, and the
            // surrounding `int` is the ATTRIBUTE itself — bash's integer
            // variable stores an integer, so `n=hello` becomes 0 (an unset name
            // is 0 in bash arithmetic). Without the coercion `arith("hello")`
            // lowers to a bare `$hello`, which is undef and stringifies to `''`.
            None => IrExpr::Call {
                func: "int".to_string(),
                args: vec![IrExpr::Call {
                    func: "arith".to_string(),
                    args: vec![IrExpr::Str(text, StrStyle::DoubleQuoted)],
                }],
            },
        };
    }
    let changed_fold = if let Some(f) = attr.fold {
        // A constant ASCII value can be case-folded HERE instead of
        // wrapped in an `lc`/`uc` call that every backend must render
        // and every runtime must provide: for ASCII the fold is
        // byte-identical to bash's (C locale) and to the backends' own
        // to_lowercase/to_uppercase. Non-ASCII keeps the Call — a
        // Unicode fold can differ per locale and backend, and this
        // pass refuses to guess.
        out = match literal_text(&out) {
            Some(t) if t.is_ascii() => IrExpr::Str(
                if f == 'l' { t.to_lowercase() } else { t.to_uppercase() },
                StrStyle::DoubleQuoted,
            ),
            _ => IrExpr::Call {
                func: if f == 'l' { "lc" } else { "uc" }.to_string(),
                args: vec![out],
            },
        };
        true
    } else {
        false
    };
    if !attr.int && !changed_fold {
        return false;
    }
    if out == *expr {
        return false;
    }
    *expr = out;
    true
}

/// The literal text of a word, if it has no expansions in it.
fn literal_text(expr: &IrExpr) -> Option<String> {
    match expr {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// The value of a compile-time DECIMAL integer literal, or `None`.
///
/// Only the unambiguous bash forms fold: optional sign, then digits,
/// with no octal-looking leading zero (`042` is 34 in bash arithmetic)
/// and no overflow past i64 (bigint stays a runtime concern). `0x…`,
/// `1_000`, `42.0`, empty — all `None`, all keeping the runtime
/// `int(arith(…))` coercion. REFUSE > GUESS.
fn strict_int(text: &str) -> Option<i64> {
    let t = text.trim();
    let body = t
        .strip_prefix('-')
        .or_else(|| t.strip_prefix('+'))
        .unwrap_or(t);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if body.len() > 1 && body.starts_with('0') {
        return None;
    }
    t.parse::<i64>().ok()
}

fn is_plain_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrExpr, IrStmt, StrStyle};

    fn builtin(cmd: &str, words: &[&str]) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str(cmd.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    words
                        .iter()
                        .map(|w| IrExpr::Str(w.to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        })
    }

    fn assign(var: &str, text: &str) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Str(text.to_string(), StrStyle::DoubleQuoted),
            asm: None,
        }
    }

    fn assign_interpolate(var: &str) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Interpolate(vec![
                InterpPart::Lit("n+".to_string()),
                InterpPart::Expr(Box::new(IrExpr::Call {
                    func: "getVar".to_string(),
                    args: vec![IrExpr::Str("x".to_string(), StrStyle::DoubleQuoted)],
                })),
            ]),
            asm: None,
        }
    }

    fn expr_of(st: &IrStmt) -> &IrExpr {
        let IrStmt::Assign { expr, .. } = st else {
            panic!("not an assignment");
        };
        expr
    }

    /// The call shape `f(g(EXPR))` → (f, g, EXPR-as-text) for assertions.
    fn nested(st: &IrStmt) -> (String, String, Option<String>) {
        let IrExpr::Call { func: outer, args } = expr_of(st) else {
            return ("none".into(), "none".into(), None);
        };
        match args.as_slice() {
            [IrExpr::Call { func: inner, args: iargs }] => {
                let text = match iargs.as_slice() {
                    [IrExpr::Str(s, _)] => Some(s.clone()),
                    _ => None,
                };
                (outer.clone(), inner.clone(), text)
            }
            [IrExpr::Str(s, _)] => (outer.clone(), String::new(), Some(s.clone())),
            _ => (outer.clone(), String::new(), None),
        }
    }

    #[test]
    fn integer_attribute_makes_later_assignments_arithmetic() {
        let mut stmts = vec![
            builtin("typeset", &["-i", "n=42"]),
            assign("n", "n+1"),
            assign("n", "hello"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(nested(&stmts[1]), ("int".into(), "arith".into(), Some("n+1".into())));
        assert_eq!(nested(&stmts[2]), ("int".into(), "arith".into(), Some("hello".into())));
    }

    #[test]
    fn other_variables_are_left_alone() {
        let mut stmts = vec![builtin("typeset", &["-i", "n=42"]), assign("m", "m+1")];
        assert!(!transform(&mut stmts));
        assert_eq!(nested(&stmts[1]).0, "none");
    }

    #[test]
    fn a_non_attribute_typeset_records_nothing() {
        let mut stmts = vec![builtin("typeset", &["-r", "ro=1"]), assign("ro", "ro+1")];
        assert!(!transform(&mut stmts));
        assert_eq!(nested(&stmts[1]).0, "none");
    }

    #[test]
    fn an_expanded_rhs_refuses_rather_than_guessing() {
        let mut stmts = vec![builtin("typeset", &["-i", "n=42"]), assign_interpolate("n")];
        assert!(!transform(&mut stmts));
        assert_eq!(nested(&stmts[1]).0, "none");
    }

    #[test]
    fn an_indexed_target_is_not_rewritten() {
        let mut stmts = vec![
            builtin("typeset", &["-i", "n=42"]),
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "n".to_string(),
                    sigil: None,
                    indices: vec![IrExpr::Str("0".to_string(), StrStyle::DoubleQuoted)],
                }],
                expr: IrExpr::Str("n+1".to_string(), StrStyle::DoubleQuoted),
                asm: None,
            },
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(nested(&stmts[1]).0, "none");
    }

    #[test]
    fn lowercase_attribute_folds_every_later_assignment() {
        let mut stmts = vec![
            builtin("typeset", &["-l", "lc=HELLO WORLD"]),
            assign("lc", "ANOTHER TEST"),
            assign("plain", "STAYED"),
        ];
        assert!(transform(&mut stmts));
        // constant + ASCII → folded HERE to the literal; no lc call
        // left for any backend to render
        assert_eq!(
            expr_of(&stmts[1]),
            &IrExpr::Str("another test".to_string(), StrStyle::DoubleQuoted)
        );
        assert_eq!(nested(&stmts[2]).0, "none");
    }

    #[test]
    fn uppercase_attribute_folds_every_later_assignment() {
        let mut stmts = vec![builtin("declare", &["-u", "uc=hello"]), assign("uc", "another")];
        assert!(transform(&mut stmts));
        assert_eq!(
            expr_of(&stmts[1]),
            &IrExpr::Str("ANOTHER".to_string(), StrStyle::DoubleQuoted)
        );
    }

    #[test]
    fn a_non_ascii_case_fold_keeps_the_runtime_call() {
        // Unicode case-folding can differ per locale and backend
        // (Rust to_lowercase('İ') ≠ C-locale bash) — refuse, keep lc().
        let mut stmts = vec![
            builtin("typeset", &["-l", "lc=x"]),
            assign("lc", "ÉCOLE"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(nested(&stmts[1]), ("lc".into(), String::new(), Some("ÉCOLE".into())));
    }

    #[test]
    fn a_canonical_decimal_integer_folds_to_the_literal() {
        let mut stmts = vec![
            builtin("typeset", &["-i", "n=42"]),
            assign("n", "7"),
            assign("n", "-3"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(expr_of(&stmts[1]), &IrExpr::Int(7));
        assert_eq!(expr_of(&stmts[2]), &IrExpr::Int(-3));
    }

    #[test]
    fn an_octal_looking_integer_refuses_the_fold() {
        // bash arithmetic reads 042 as 34; Rust parse reads it as 42 —
        // a wrong fold would be a silent value change, so keep the
        // runtime int(arith(…)) coercion for it.
        let mut stmts = vec![builtin("typeset", &["-i", "n=42"]), assign("n", "042")];
        assert!(transform(&mut stmts));
        assert_eq!(nested(&stmts[1]), ("int".into(), "arith".into(), Some("042".into())));
    }

    #[test]
    fn a_case_attribute_folds_an_expanded_rhs_too() {
        // bash folds the VALUE AFTER expansion, so a non-literal RHS is
        // still foldable — only `arith` needs literal text.
        let mut stmts = vec![
            builtin("typeset", &["-l", "lc=Hi"]),
            builtin("typeset", &["-i", "n=42"]),
            assign_interpolate("lc"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(nested(&stmts[2]), ("lc".into(), String::new(), None));
    }

    #[test]
    fn both_case_attributes_are_refused_not_guessed() {
        let mut stmts = vec![builtin("typeset", &["-lu", "v=ABC"]), assign("v", "XYZ")];
        assert!(!transform(&mut stmts));
        assert_eq!(nested(&stmts[1]).0, "none");
    }

    #[test]
    fn an_integer_plus_lowercase_declaration_coerces_then_folds() {
        let mut stmts = vec![builtin("typeset", &["-il", "comb=42"]), assign("comb", "comb+1")];
        assert!(transform(&mut stmts));
        let (outer, _, _) = nested(&stmts[1]);
        assert_eq!(outer, "lc".to_string() + "");
        // inner-inner: int → arith
        let IrExpr::Call { args, .. } = expr_of(&stmts[1]) else { panic!() };
        let Some(IrExpr::Call { func: inner, args: iargs }) = args.first() else {
            panic!()
        };
        assert_eq!(inner, "int");
        let Some(IrExpr::Call { func: arith, args: aargs }) = iargs.first() else {
            panic!()
        };
        assert_eq!(arith, "arith");
        assert!(matches!(aargs.as_slice(), [IrExpr::Str(s, _)] if s == "comb+1"));
    }

    #[test]
    fn another_builtins_flags_do_not_declare_variables() {
        // `find` IS in the builtins namespace, so `find -iname PAT` lowers
        // to a `builtin(-iname, PAT)` call — `-iname` contains `i` and PAT
        // is a plain name. Without the command gate PAT would be recorded as
        // an INTEGER variable and a later `PAT=word` would become arithmetic.
        let mut stmts = vec![
            builtin("find", &["-i", "PAT"]),
            assign("PAT", "word"),
            builtin("grep", &["-i", "pat"]),
            assign("pat", "2+3"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(nested(&stmts[1]).0, "none");
        assert_eq!(nested(&stmts[3]).0, "none");
    }

    #[test]
    fn every_name_on_the_declaration_line_is_recorded() {
        let mut stmts = vec![
            builtin("typeset", &["-i", "a=1", "b=2"]),
            assign("a", "a+1"),
            assign("b", "b+1"),
            assign("c", "c+1"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(nested(&stmts[1]), ("int".into(), "arith".into(), Some("a+1".into())));
        assert_eq!(nested(&stmts[2]), ("int".into(), "arith".into(), Some("b+1".into())));
        assert_eq!(nested(&stmts[3]).0, "none");
    }
}