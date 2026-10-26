//! arith-identity — simplify arithmetic identities (`$((x+0))` → `x`,
//! `$((x*1))` → `x`, `$((x/1))` → `x`, …) over `Arith` trees. Distinct
//! from the landed const-fold-arith (which folds LITERAL subtrees): this
//! folds identities on NON-constant operands, which appears in generated
//! math (`$((idx + 0))`, `$((v * 1))`, the `+ 0` residue every backend
//! renders verbatim).
//!
//! ## Need
//! Renderers emit `$((x+0))` verbatim; each costs an op and a temp in
//! every backend. Folding feeds const-condition-elim and
//! copy-propagation (a folded identity is a plain read a later pass can
//! use).
//!
//! ## Scope — the registered rule set (two tiers)
//!
//! UNCONDITIONAL (any source language — exact in every domain):
//!   `x * 1` → `x`, `1 * x` → `x`, `x / 1` → `x`
//!     (IEEE-exact for -0/Inf/NaN, and the operand survives verbatim,
//!     so its parse/coercion fate is unchanged);
//!   `x - 0` → `x` (`x - +0.0` rounds to exactly `x` in IEEE — -0
//!     included — and in shell arithmetic the operand alone fails or
//!     succeeds exactly as it did inside the expression);
//!   `0 / positive-literal` → `0` — both operands are already-parsed
//!     `Num` ints (exact `+0` from ints through IEEE). A `0 / 0`
//!     divisor is divide-by-zero and a negative one flips `-0.0`;
//!     non-literal divisors are unprovable here — both refused.
//!
//! SHELL-INTEGER DOMAIN ONLY ([`int_domain`]):
//!   `x + 0` → `x`, `0 + x` → `x`.
//!     Both preserve the operand (a non-integer string like `1.5` fails
//!     to parse in `$((x+0))` and in `$((x))` alike, so bash's error
//!     behavior is unchanged), but the FLOAT value of `+0` differs on
//!     `-0.0` (`-0.0 + 0.0` is `+0.0`): only sources whose arithmetic
//!     is integer-by-construction (the bash family — zsh/fish promote
//!     to float, frontends carry var types) get these.
//!
//! ## Deliberately NOT folded: the operand-ELIMINATING rules
//! `x % 1 → 0`, `x * 0 → 0`, `0 * x → 0`, `x - x → 0` REPLACE the
//! operand with a literal, which silently deletes its coercion/parse
//! fate: bash re-parses the variable's value inside `$((…))`, so
//! `x=1.5; $((x*0))` is a syntax error (empty output) while the folded
//! `$((0))` prints `0` — and a backend that numifies where the shell
//! errors diverges the other way. Sound only when the operand is
//! already a literal `Num`, which is exactly const-fold-arith's job, so
//! the rules are gone here rather than parked behind a guard that
//! cannot see var types (this closes the 2026-09-09 registration
//! review's OPEN item with a stronger answer than var-type gating).
//!
//! ## Placement
//! REGISTERED in `transforms::all()` (every backend benefits; shell IR
//! is the input `ast_to_ir` ever feeds it). [`int_domain`] is the
//! defensive layer: it reads the source language recorded before
//! `transforms::apply` — unset means the shell pipeline (the only
//! production caller that runs apply without a recorded language), any
//! non-shell recording gets the float-exact subset.
//!
//! ## Why RAW arith text is never folded
//! `n=$((…))` assignments can't be `ArithAst`s (assignments aren't
//! representable), so they reach every backend as
//! `Call(func:"arith", [Str(raw)])` — text the renderer tokenizes at
//! render time, after all transforms. An identity fold in that text
//! looks tempting and is UNSOUND in every shape tried, because the
//! operand survives only as EXPANDED VALUE, not as a node:
//!   * `X + 0 → X` / `X * 1 → X` (suffix): a value-EMPTY operand
//!     changes fate — measured on dollar-positional-arithmetic.sh
//!     (no args): bash `$(($1 + 0))` → `0` via unary-`+`, while the
//!     backend renders the folded bare `$1` as an un-numified empty
//!     STRING → stdout mismatch (./fail caught it, 561/562);
//!   * `0 + X → X` / `1 * X → X` (prefix): measured in bash — empty X
//!     makes the ORIGINAL a trailing-operator syntax error
//!     (`0 + : syntax error`) where the folded form evaluates to `0`;
//!   * `X * 1 → X` (suffix, empty X): bash errors on the leading `*`
//!     where the folded empty expression is `0`.
//! The tree fold has none of these holes: a backend's `Var` read
//! coerces empty→0 (and `05`→5 through the same int path as the `+ 0`
//! it replaces), so operand fate is preserved. Raw text would need
//! nonempty-value proofs this layer cannot make — refuse > guess.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= rewrite_stmt(st);
    }
    changed
}

/// Integer arithmetic by construction — the domain where `x + 0 → x`
/// and `0 + x → x` are exact (no `-0.0` whose sign `+0.0` erases).
///
/// `None` (unset) is the shell default, mirroring
/// `shir::source_has_exit_status`'s reading of unset: the production
/// caller that runs apply with no recorded language is the shell
/// pipeline (`ast_to_ir`). The bash family evaluates `$((…))` as
/// integers; `zsh`/`fish` promote to float and every frontend language
/// carries var types — all conservative. `shir` (A1 passthrough) is
/// origin-unknown, so it is conservative too.
pub fn int_domain() -> bool {
    match crate::shir::source_lang() {
        None => true,
        Some(l) => {
            let l = l.trim().trim_start_matches('.');
            matches!(
                l,
                "" | "sh" | "bash" | "dash" | "posix" | "posix-sh" | "ksh"
            )
        }
    }
}

fn rewrite_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Ext(e) => {
            let mut c = false;
            for s in e.children_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Output { value, .. } => rewrite_expr(value),
        IrStmt::WriteFile { path, content, .. } => rewrite_expr(path) | rewrite_expr(content),
        IrStmt::Assign { targets, expr, .. } => {
            let mut c = rewrite_expr(expr);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    c |= rewrite_expr(i);
                }
            }
            c
        }
        IrStmt::Declare { init, .. } => match init {
            Some(e) => rewrite_expr(e),
            None => false,
        },
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= rewrite_expr(e);
            }
            c
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = rewrite_expr(cond);
            for s in then.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= rewrite_expr(ec);
                for s in eb.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            for s in else_.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for ex in excepts.iter_mut() {
                if let Some(m) = ex.match_expr.as_mut() {
                    c |= rewrite_expr(m);
                }
                for s in ex.body.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            for s in else_body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for s in finally_body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
            ..
        } => {
            let mut c = rewrite_expr(cond);
            for v in [init, step, body] {
                for s in v.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = rewrite_expr(cond);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = rewrite_expr(cond);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = rewrite_expr(iter);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
            rewrite_expr(expr)
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => rewrite_expr(e),
        IrStmt::Return(None) | IrStmt::Exit(None) => false,
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            let mut c = rewrite_expr(cmd);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                c |= rewrite_expr(a);
            }
            for (_, e) in env.iter_mut() {
                c |= rewrite_expr(e);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stg in stages.iter_mut() {
                for s in stg.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = rewrite_expr(discriminant);
            for cl in clauses.iter_mut() {
                for s in cl.body.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for r in redirects.iter_mut() {
                c |= rewrite_expr(&mut r.target);
            }
            c
        }
        // Function bodies are production IR (every `name() { … }` in the
        // shell corpus lands here) — the arm whose absence left `+ 0`
        // residue inside functions.
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for (_, blk) in named_blocks.iter_mut() {
                for s in blk.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Select { clauses } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                for e in cl.ch.iter_mut().chain(cl.value.iter_mut()) {
                    c |= rewrite_expr(e);
                }
                for s in cl.body.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            let mut c = false;
            for (_, e) in outputs.iter_mut().chain(inputs.iter_mut()) {
                c |= rewrite_expr(e);
            }
            c
        }
        IrStmt::Expr(e) => rewrite_expr(e),
        IrStmt::Shaderize { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        // no expression children
        IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Label(_)
        | IrStmt::Goto(_)
        | IrStmt::Require(_)
        | IrStmt::RawText(_) => false,
    }
}

fn rewrite_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arith(a) => {
            let before = format!("{:?}", a);
            simplify(a);
            format!("{:?}", a) != before
        }
        IrExpr::Index { key, .. } => rewrite_expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => rewrite_expr(lhs) | rewrite_expr(rhs),
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= rewrite_expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = rewrite_expr(obj);
            for a in args.iter_mut() {
                c |= rewrite_expr(a);
            }
            c
        }
        IrExpr::Ternary { cond, then, else_ } => {
            rewrite_expr(cond) | rewrite_expr(then) | rewrite_expr(else_)
        }
        IrExpr::DefinedOr { expr, default } => rewrite_expr(expr) | rewrite_expr(default),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= rewrite_expr(x);
                }
            }
            c
        }
        IrExpr::Capture { expr, .. } => rewrite_expr(expr),
        IrExpr::Arrow(v) | IrExpr::Lambda { body: v, .. } => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            let mut c = rewrite_expr(iter) | rewrite_expr(elem);
            if let Some(x) = cond.as_mut() {
                c |= rewrite_expr(x);
            }
            c
        }
        IrExpr::Splice(x) => rewrite_expr(x),
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, x) in pairs.iter_mut() {
                c |= rewrite_expr(x);
            }
            c
        }
        IrExpr::Ext(x) => {
            let mut c = false;
            for x in x.children_mut() {
                c |= rewrite_expr(x);
            }
            c
        }
        // no children / no Arith reachable
        IrExpr::Int(_)
        | IrExpr::Str(..)
        | IrExpr::Var(..)
        | IrExpr::Regex { .. }
        | IrExpr::Range { .. }
        | IrExpr::RawExpr(_)
        | IrExpr::Bool(_)
        | IrExpr::Json(_)
        | IrExpr::Ident(_) => false,
    }
}

/// In-place identity simplification of an Arith tree.
fn simplify(a: &mut ArithAst) {
    simplify_in(a, int_domain())
}

fn simplify_in(a: &mut ArithAst, full: bool) {
    match a {
        ArithAst::Bin { op, lhs, rhs } => {
            simplify_in(lhs, full);
            simplify_in(rhs, full);
            let num = |x: &ArithAst| match x {
                ArithAst::Num(n) => Some(*n),
                _ => None,
            };
            match op.as_str() {
                // `x + 0` / `0 + x`: operand-preserving, but `+0.0`
                // erases a `-0.0` sign — integer domains only.
                "+" if full => {
                    if num(rhs) == Some(0) {
                        *a = lhs.as_ref().clone();
                    } else if num(lhs) == Some(0) {
                        *a = rhs.as_ref().clone();
                    }
                }
                // `x - 0` → `x`: exact everywhere (IEEE rounds
                // `x - +0.0` to `x`, -0 included; the operand's parse
                // fate is unchanged in shell).
                "-" => {
                    if num(rhs) == Some(0) {
                        *a = lhs.as_ref().clone();
                    }
                }
                "*" => {
                    if num(rhs) == Some(1) {
                        *a = lhs.as_ref().clone();
                    } else if num(lhs) == Some(1) {
                        *a = rhs.as_ref().clone();
                    }
                }
                "/" => {
                    if num(rhs) == Some(1) {
                        *a = lhs.as_ref().clone();
                    } else if num(lhs) == Some(0)
                        && matches!(num(rhs), Some(n) if n > 0)
                        && arith_no_err(rhs)
                    {
                        // `0 / positive-literal` → `0` (exact in every
                        // domain: `0 / positive` is `+0` from ints
                        // through IEEE). A zero divisor (`0 / 0`) is
                        // divide-by-zero (bash abort / NaN / SIGFPE —
                        // never foldable), and a negative one flips
                        // `-0.0` to `+0.0`; non-literals stay
                        // (unprovable without var types at this layer).
                        *a = ArithAst::Num(0);
                    }
                }
                // `%`, and the operand-ELIMINATING `*0`/`0*x`/`x-x`
                // forms: never (see the module doc — replacing the
                // operand drops its coercion/parse fate).
                _ => {}
            }
        }
        ArithAst::Un { arg, .. } => simplify_in(arg, full),
        ArithAst::Cond {
            test,
            then,
            else_,
            ..
        } => {
            simplify_in(test, full);
            simplify_in(then, full);
            simplify_in(else_, full);
        }
        _ => {}
    }
}

/// True when the tree cannot raise a divide-by-zero (no `/`/`%`).
fn arith_no_err(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) => true,
        ArithAst::Bin { op, lhs, rhs } => {
            !matches!(op.as_str(), "/" | "%") && arith_no_err(lhs) && arith_no_err(rhs)
        }
        ArithAst::Un { arg, .. } => arith_no_err(arg),
        ArithAst::Cond {
            test, then, else_, ..
        } => arith_no_err(test) && arith_no_err(then) && arith_no_err(else_),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ArithAst;

    fn num(n: i64) -> ArithAst {
        ArithAst::Num(n)
    }
    fn var(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }
    fn bin(op: &str, lhs: ArithAst, rhs: ArithAst) -> ArithAst {
        ArithAst::Bin {
            op: op.to_string(),
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }
    fn simplified(mut a: ArithAst) -> ArithAst {
        simplify(&mut a);
        a
    }
    fn is_num(a: &ArithAst, n: i64) -> bool {
        matches!(a, ArithAst::Num(m) if *m == n)
    }

    #[test]
    fn zero_divided_by_zero_refuses() {
        // `0 / 0` is divide-by-zero in every domain (bash abort, NaN,
        // SIGFPE) — folding it to `0` would swallow the error.
        let out = simplified(bin("/", num(0), num(0)));
        assert!(
            matches!(out, ArithAst::Bin { .. }),
            "0/0 must stay unfolded: {out:?}"
        );
    }

    #[test]
    fn zero_divided_by_positive_literal_folds() {
        // `0 / 5` → `0`: exact from ints through IEEE (`+0` either way).
        let out = simplified(bin("/", num(0), num(5)));
        assert!(is_num(&out, 0), "0/5 folds to 0: {out:?}");
    }

    #[test]
    fn zero_divided_by_var_or_negative_refuses() {
        // Unprovable (var) or sign-flipping (negative divisor turns
        // `-0.0` into `+0.0`) — keep the division.
        let out = simplified(bin("/", num(0), var("x")));
        assert!(
            matches!(out, ArithAst::Bin { .. }),
            "0/x-var stays unfolded: {out:?}"
        );
        let out = simplified(bin("/", num(0), num(-5)));
        assert!(
            matches!(out, ArithAst::Bin { .. }),
            "0/-5 stays unfolded: {out:?}"
        );
    }

    #[test]
    fn unconditional_identities_fold() {
        // `* 1` and `/ 1` are exact in every domain (IEEE preserves -0,
        // Inf and NaN through both) — no divisor/value proof needed
        // beyond the literal itself. `x - 0` likewise: `x - +0.0`
        // rounds to exactly `x`, -0 included.
        let out = simplified(bin("*", var("x"), num(1)));
        assert!(matches!(out, ArithAst::Var(_)), "x*1 folds to x: {out:?}");
        let out = simplified(bin("*", num(1), var("x")));
        assert!(matches!(out, ArithAst::Var(_)), "1*x folds to x: {out:?}");
        let out = simplified(bin("/", var("x"), num(1)));
        assert!(matches!(out, ArithAst::Var(_)), "x/1 folds to x: {out:?}");
        let out = simplified(bin("-", var("x"), num(0)));
        assert!(matches!(out, ArithAst::Var(_)), "x-0 folds to x: {out:?}");
    }

    #[test]
    fn int_domain_gates_plus_zero() {
        use crate::shir::set_source_lang;
        // shell family (and unset = the shell default) → full rules
        set_source_lang(Some("bash"));
        let out = simplified(bin("+", var("x"), num(0)));
        assert!(matches!(out, ArithAst::Var(_)), "bash: x+0 folds: {out:?}");
        set_source_lang(Some(".sh"));
        let out = simplified(bin("+", num(0), var("x")));
        assert!(matches!(out, ArithAst::Var(_)), ".sh: 0+x folds: {out:?}");
        set_source_lang(None);
        let out = simplified(bin("+", var("x"), num(0)));
        assert!(matches!(out, ArithAst::Var(_)), "unset folds: {out:?}");

        // float-capable / origin-unknown sources keep `+ 0`
        // (`-0.0 + 0.0` is `+0.0`, not `-0.0`) …
        for lang in ["py", "zsh", "fish", "c", "go", "shir", "unknown"] {
            set_source_lang(Some(lang));
            let out = simplified(bin("+", var("x"), num(0)));
            assert!(
                matches!(out, ArithAst::Bin { .. }),
                "{lang} must keep x+0: {out:?}"
            );
            let out = simplified(bin("+", num(0), var("x")));
            assert!(
                matches!(out, ArithAst::Bin { .. }),
                "{lang} must keep 0+x: {out:?}"
            );
            // …while the float-exact subset still fires there
            let out = simplified(bin("*", var("x"), num(1)));
            assert!(matches!(out, ArithAst::Var(_)), "{lang}: x*1 folds");
            let out = simplified(bin("/", var("x"), num(1)));
            assert!(matches!(out, ArithAst::Var(_)), "{lang}: x/1 folds");
            let out = simplified(bin("-", var("x"), num(0)));
            assert!(matches!(out, ArithAst::Var(_)), "{lang}: x-0 folds");
        }
        set_source_lang(None);
    }

    #[test]
    fn operand_eliminating_rules_never_fold() {
        // `x%1`/`x*0`/`0*x`/`x-x` replace the operand — and with it its
        // coercion/parse fate (`x=1.5; $((x*0))` is a bash syntax error
        // where `$((0))` prints `0`). Literal-only instances are
        // const-fold-arith's; the var forms stay unfolded.
        for out in [
            simplified(bin("%", var("x"), num(1))),
            simplified(bin("*", var("x"), num(0))),
            simplified(bin("*", num(0), var("x"))),
            simplified(bin("-", var("x"), var("x"))),
        ] {
            assert!(
                matches!(out, ArithAst::Bin { .. }),
                "operand-eliminating rule must stay: {out:?}"
            );
        }
    }

    #[test]
    fn registered_in_all() {
        assert!(
            crate::transforms::all().iter().any(|(n, _)| *n == "arith-identity"),
            "arith-identity must be registered in transforms::all()"
        );
    }

    #[test]
    fn function_bodies_are_walked() {
        // the 002_control_flow greet() shape: an identity inside a
        // Function body must fold (the arm whose absence left residue).
        let mut stmts = vec![IrStmt::Function {
            name: "f".to_string(),
            params: vec![],
            body: vec![IrStmt::Output {
                value: IrExpr::Arith(Box::new(bin("+", var("x"), num(0)))),
                newline: true,
                target: None,
            }],
            named_blocks: vec![],
        }];
        assert!(transform(&mut stmts), "Function body fold reported");
        let IrStmt::Function { body, .. } = &stmts[0] else {
            panic!("still a Function");
        };
        let IrStmt::Output { value, .. } = &body[0] else {
            panic!("still an Output");
        };
        let IrExpr::Arith(a) = value else {
            panic!("still an Arith");
        };
        assert!(matches!(a.as_ref(), ArithAst::Var(_)), "folded: {a:?}");
    }

    #[test]
    fn exec_pipeline_case_and_redirect_arms_are_walked() {
        let mk = || {
            IrStmt::Exec {
                cmd: IrExpr::Arith(Box::new(bin("+", var("x"), num(0)))),
                args: vec![IrExpr::Arith(Box::new(bin("*", num(1), var("y"))))],
                capture: None,
                redirects: vec![IrExpr::Arith(Box::new(bin("-", var("z"), num(0))))],
                env: vec![(
                    "V".to_string(),
                    IrExpr::Arith(Box::new(bin("+", num(0), var("w")))),
                )],
            }
        };
        let mut stmts = vec![
            mk(),
            IrStmt::Pipeline {
                stages: vec![vec![mk()]],
                last_output: None,
                capture: None,
                cmd_str: None,
            },
            IrStmt::Case {
                discriminant: IrExpr::Arith(Box::new(bin("+", var("c"), num(0)))),
                clauses: vec![crate::ir::IrCaseClause {
                    patterns: vec!["*".to_string()],
                    body: vec![mk()],
                }],
            },
            IrStmt::Redirect {
                inner: vec![mk()],
                redirects: vec![crate::ir::IrRedirect {
                    fd: None,
                    mode: "w".to_string(),
                    target: IrExpr::Arith(Box::new(bin("+", var("t"), num(0)))),
                    interpolate: false,
                }],
            },
        ];
        assert!(transform(&mut stmts), "deep arms fold");
    }
}
