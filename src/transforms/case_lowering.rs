//! case-lowering — lower an all-static `case` dispatch to an if/elif
//! chain over the SHARED native string primitives.
//!
//! ## Need
//! Every code-generating backend re-derives `case` pattern predicates on
//! its own: rust builds `__sh_fnmatch(pat, value)` calls per clause (the
//! corpus carries ~242 of them — ~88 plain literals and ~38 affix shapes
//! among them), perl translates each pattern to a quotemeta'd regex, the
//! C backend re-implements the glob grammar again. For the four
//! string-op shapes (`lit`, `lit*`, `*lit`, `*lit*`) plus the `*`
//! catch-all there is nothing to glob-match: it is an equality or an
//! affix test, and every backend ALREADY renders the shared primitives
//! natively (estree `.startsWith`/`.includes`/`===`, perl
//! `index >= 0`, C `strstr`, go `strings.Contains`, python `in`, zig
//! `std.mem.indexOf`, rust `.starts_with`) — the exact vocabulary
//! `test-lowering` established for `[[ ]]` glob-affixes. First-match-wins
//! dispatch IS an if/else-if chain (the estree path's `try_native_case`
//! already lowers case exactly that way — this transform generalises
//! that landed lowering into the shared registry so EVERY backend
//! benefits instead of one).
//!
//! ## Scope — refuse > guess
//! The whole `case` is lowered only when EVERY pattern of EVERY clause
//! classifies (through the shared `classify_case_pat`, after splitting
//! `a|b` alternations with the shared `split_case_alternatives`):
//!   * `*` (all stars) → the ELSE branch; clauses after it are
//!     unreachable and dropped (bash's own first-match semantics);
//!   * `lit` → `Not(value ne lit)` — the canonical both-sides `Not`
//!     (test_lowering's `not_wrap`): `BinOpKind::Eq` is perl's NUMERIC
//!     `==` (`'abc' == 'xyz'` is TRUE there — both numify to 0), while
//!     `Ne` is every backend's STRING inequality, so exact equality is
//!     its negation, exactly as `string_eq_primitive` does;
//!   * `lit*` → `strHasPrefix`, `*lit` → `strHasSuffix`, `*lit*` →
//!     `contains`, each over the discriminant and a `SingleQuoted`
//!     (byte-verbatim) literal;
//!   * any other glob (`i*86`, `?`, `[a-z]`, escapes, mid-star) → the
//!     shared `globMatch(value, pattern)` predicate — ONE op rendered
//!     by each accepting backend's own glob engine (estree → the
//!     runtime's `sh2.globMatch`, the exact matcher `sh2.caseMatch`
//!     dispatches with; rust → `__sh_fnmatch`; java → its `shFnmatch`),
//!     so the pattern passes through VERBATIM and no glob is
//!     re-translated. Targets without an arm refuse the glob clauses
//!     (the whole case then stays on the backend's own render, exactly
//!     as before) — the accepting set grows one verified arm at a time.
//! Refusals (the case stays on each backend's own render, unchanged):
//!   * a QUOTED pattern carrying `$`/backtick (runtime-expanded) or a
//!     backslash escape — but a plain quoted pattern IS its literal
//!     text (`"*"` is a LITERAL star, not the wildcard — the rust
//!     backend's case render makes the same unwrap, so lowering keeps
//!     both sides byte-identical);
//!     a partially-quoted pattern (`a"b`) refuses outright;
//!   * an impure or runtime-live discriminant (`$(…)`, `$((…))`,
//!     `RANDOM`/`SECONDS`/`LINENO`/`PPID`/… — anything `pure_subject`
//!     refuses) is bound to a fresh `__sh2_casesubj_N` temp BEFORE the
//!     chain: bash evaluates the case word ONCE at entry, so one
//!     pre-chain binding reproduces that exactly — the conds read only
//!     the temp, no capture re-runs and no special re-draws. The bind
//!     is TEXT-typed (`bind_init`): a non-text subject (`$((…))`, a
//!     reserved `Var`) is wrapped in the `Interpolate` that is its
//!     expansion, because bash's case word is a string — an `Arith`
//!     init types the temp `Int`, which the C renderer turns into
//!     `atoll((char*)(<native int>))` (the value read as a pointer →
//!     SIGSEGV) and a `long long != "pat"` pointer compare. The name
//!     is probe-checked against the whole statement tree (the same
//!     Debug-substring conservative probe this pass uses for
//!     nocasematch), so it cannot collide with a source or transform
//!     name. A pure subject (`Var`/`Str`/`Int`/`Interpolate` over those
//!     / nested `Index` of those) skips the bind; `case "$1" in` is the
//!     common pure case.
//!   * the program mentioning `nocasematch` anywhere — the same
//!     conservative whole-program Debug probe test_lowering uses: the
//!     typed `ne`/primitive compares are case-sensitive while bash's
//!     `case` honors `shopt nocasematch` dynamically (the runtime
//!     case/test path is the shopt-aware one);
//!   * a `*` in FIRST position (an unconditional first clause has no
//!     condition to bind; the chain would need a vacuous cond — the
//!     rare guard-first shape keeps the backend render).
//!
//! ## Status
//! bash: a `case` whose patterns don't match leaves `$?` at 0 (the
//! subject's own status must not leak) — the same as an `If` with no
//! matching branch, whose empty-else lowering already synthesizes the
//! status write each backend uses (the estree path says so explicitly:
//! "mirroring the IrStmt::If empty-else lowering"). A match yields the
//! clause body's status through the ordinary `then` path. `break`/
//! `continue` in clause bodies are plain `IrStmt::Break`/`Continue` —
//! each backend's If rendering already maps them to loop signals.
//!
//! ## Placement
//! Registered in `transforms.rs` (`case-lowering`), target-gated in
//! `transform_applies` to the code-generating backends (c/js/estree/
//! perl/go/rs/rust/zig/java/python): the shell-family targets render
//! `case…esac` natively and idiomatically (no rewrite wanted), and an
//! unset/`shir` target is the byte-pinned embed/A1-export path (the
//! split-in-place precedent — never rewrite there).

use crate::ir::{BinOpKind, Decl, InterpPart, IrCaseClause, IrExpr, IrStmt, StrStyle};
use crate::shir::{classify_case_pat, is_reserved_var, split_case_alternatives, CasePat};

use super::test_lowering::not_wrap;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // Conservative whole-program nocasematch probe — the SAME shape
    // test_lowering uses (its guard 3): Debug-formatting the statement
    // tree and substring-searching catches `shopt("nocasematch", …)`
    // calls, `set -o` payloads, eval'd text and mere string mentions
    // alike; refusing a lift is free (the backends keep their
    // shopt-aware render).
    let nocase = format!("{stmts:?}").contains("nocasematch");
    if nocase {
        dbg("refuse: nocasematch mention");
        return false;
    }
    dbg(&format!("enter target={:?} stmts={}", crate::transforms::target(), stmts.len()));
    // Fresh-name probe: the whole tree AT ENTRY (see `FRESH`).
    let probe = format!("{stmts:?}");
    FRESH.with(|f| *f.borrow_mut() = (probe, 0));
    let mut changed = false;
    walk(stmts, &mut changed);
    changed
}

/// Temp diagnostics (CASE_DBG=1), removed after triage.
fn dbg(msg: &str) {
    if std::env::var_os("CASE_DBG").is_some() {
        eprintln!("[case-lowering] {msg}");
    }
}

fn no(reason: &str) -> Option<IrStmt> {
    dbg(reason);
    None
}

/// Fresh temp names for impure-subject binding: `(probe, next_index)`.
/// Reset at every `transform()` entry with the whole statement tree
/// Debug-formatted, so the probe both guards against any pre-existing
/// source/transform name AND — on a re-run — against names this pass
/// allocated earlier (they are in the tree by then).
thread_local! {
    static FRESH: std::cell::RefCell<(String, usize)> =
        std::cell::RefCell::new((String::new(), 0));
}

/// The next `__sh2_casesubj_N` not mentioned anywhere in the probe.
/// The counter never repeats within a run; the probe skips taken names.
fn fresh_subject_name() -> String {
    FRESH.with(|f| {
        let mut st = f.borrow_mut();
        loop {
            let cand = format!("__sh2_casesubj_{}", st.1);
            st.1 += 1;
            if !st.0.contains(&cand) {
                return cand;
            }
        }
    })
}

/// In-place walk: find every `Case` (nested in any statement container
/// or clause body) and lower it where all its patterns qualify.
fn walk(stmts: &mut Vec<IrStmt>, changed: &mut bool) {
    for st in stmts.iter_mut() {
        match st {
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                dbg(&format!("see Case subject={discriminant:?} pats={:?}",
                    clauses.iter().map(|c| c.patterns.clone()).collect::<Vec<_>>()));
                // nested cases inside clause bodies lower first
                for cl in clauses.iter_mut() {
                    walk(&mut cl.body, changed);
                }
                if let Some(new_st) = lower_case(discriminant, clauses) {
                    *st = new_st;
                    *changed = true;
                }
            }
            IrStmt::If {
                then,
                elsifs,
                else_,
                ..
            } => {
                walk(then, changed);
                for (_, b) in elsifs.iter_mut() {
                    walk(b, changed);
                }
                walk(else_, changed);
            }
            IrStmt::Function { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => walk(body, changed),
            IrStmt::For { body, .. }
            | IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. } => walk(body, changed),
            IrStmt::ForInit { init, step, body, .. } => {
                walk(init, changed);
                walk(step, changed);
                walk(body, changed);
            }
            IrStmt::Redirect { inner, .. } => walk(inner, changed),
            IrStmt::Pipeline { stages, .. } => {
                for s in stages.iter_mut() {
                    walk(s, changed);
                }
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                walk(body, changed);
                for e in excepts.iter_mut() {
                    walk(&mut e.body, changed);
                }
                walk(else_body, changed);
                walk(finally_body, changed);
            }
            IrStmt::Select { clauses } => {
                for c in clauses.iter_mut() {
                    walk(&mut c.body, changed);
                }
            }
            // expression-bearing statements: no nested statement list
            // except an estree-path Arrow body (pipeline stages bind
            // statements there) — descend it so a case inside still
            // lowers.
            IrStmt::Output { value, .. }
            | IrStmt::Expr(value)
            | IrStmt::Die { expr: value, .. }
            | IrStmt::Warn { expr: value, .. }
            | IrStmt::Return(Some(value))
            | IrStmt::Exit(Some(value))
            | IrStmt::SetChildError(value) => walk_expr(value, changed),
            IrStmt::Assign { expr, .. } | IrStmt::Declare { init: Some(expr), .. } => {
                walk_expr(expr, changed)
            }
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, changed);
                walk_expr(content, changed);
            }
            // Ext nodes hold statement children for some shapes, but
            // walking them mutably needs the ExtNode mut-surface this
            // transform does not require: a case left unlowered inside
            // one simply keeps the backend's own render (safe).
            _ => {}
        }
    }
}

/// Descend an expression only for statement-carrying forms.
fn walk_expr(e: &mut IrExpr, changed: &mut bool) {
    match e {
        IrExpr::Arrow(stmts) => walk(stmts, changed),
        _ => {}
    }
}

/// Build the if/else-if chain for one `case`, or `None` to refuse
/// (refusal leaves the `IrStmt::Case` untouched for the backends).
fn lower_case(disc: &IrExpr, clauses: &[IrCaseClause]) -> Option<IrStmt> {
    if clauses.is_empty() {
        return no("empty clauses");
    }
    // A subject `pure_subject` refuses is bound to a fresh temp BEFORE
    // the chain (module docs): bash evaluates the case word ONCE at
    // entry — one pre-chain binding reproduces that exactly, where the
    // per-condition reads the pure path relies on would re-run a
    // capture or re-draw a runtime-live special. Conds read the temp.
    let bind;
    let bound_disc;
    if pure_subject(disc) {
        bind = None;
        bound_disc = disc.clone();
    } else {
        let name = fresh_subject_name();
        bind = Some(IrStmt::Declare {
            vars: vec![Decl {
                name: name.clone(),
                sigil: None,
            }],
            init: Some(bind_init(disc)),
            local: false,
        });
        bound_disc = IrExpr::Var(name, None);
    }
    let disc = &bound_disc;
    let mut chain: Vec<(IrExpr, Vec<IrStmt>)> = Vec::new();
    let mut else_body: Option<Vec<IrStmt>> = None;
    for cl in clauses {
        if else_body.is_some() {
            break; // unreachable after a `*` clause — drop it (bash's first-match)
        }
        let mut alts: Vec<IrExpr> = Vec::new();
        let mut uncond = false;
        for p in &cl.patterns {
            // `?` on classify propagates a refusal of the WHOLE case
        for alt in split_case_alternatives(p) {
            let cls = match classify_pattern(&alt) {
                Some(c) => c,
                None => return no(&format!("pattern refuses: {alt:?} (pat {p:?})")),
            };
            match cls {
                    CasePat::Any => uncond = true,
                    CasePat::Exact(s) => alts.push(exact_cond(disc, &s)),
                    CasePat::Prefix(s) => alts.push(affix_cond(disc, "strHasPrefix", &s)),
                    CasePat::Suffix(s) => alts.push(affix_cond(disc, "strHasSuffix", &s)),
                    CasePat::Substr(s) => alts.push(affix_cond(disc, "contains", &s)),
                    // A glob beyond the four string-op shapes (char
                    // classes, `?`, mid-star): ONE shared predicate over
                    // the backend's own glob engine. Targets without a
                    // `globMatch` arm refuse the whole case, exactly the
                    // pre-globMatch behaviour — the backend's own case
                    // render (rust fnmatch / perl regex / estree
                    // caseMatch) keeps it.
                    CasePat::Glob(_) => {
                        if glob_target_ok() {
                            alts.push(glob_cond(disc, &alt));
                        } else {
                            return no(&format!("glob pattern (no globMatch arm for target): {alt:?}"));
                        }
                    }
                }
            }
        }
        if uncond {
            // `*` (possibly among alternatives): everything from here
            // matches. First position has no condition to bind — refuse
            // rather than invent a vacuous cond; later position is the
            // ordinary else branch.
            if chain.is_empty() {
                return no("leading catch-all");
            }
            else_body = Some(cl.body.clone());
            continue;
        }
        if alts.is_empty() {
            return no(&format!("unclassifiable clause patterns: {:?}", cl.patterns));
        }
        let mut it = alts.into_iter();
        let cond = it.next().expect("alts non-empty").clone();
        let cond = it.fold(cond, |a, b| IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(a),
            rhs: Box::new(b),
        });
        chain.push((cond, cl.body.clone()));
    }
    let (cond, then) = chain.first()?.clone();
    dbg(&format!("LOWERS subject={disc:?} clauses={} conds={}", clauses.len(), chain.len()));
    let elsifs = chain.into_iter().skip(1).collect();
    let if_stmt = IrStmt::If {
        cond,
        then,
        elsifs,
        else_: else_body.unwrap_or_default(),
    };
    match bind {
        Some(decl) => Some(IrStmt::Block(vec![decl, if_stmt])),
        None => Some(if_stmt),
    }
}

/// Classify ONE pattern alternative for the lowering. Quote-aware on
/// purpose: the shared `classify_case_pat` unwraps quotes and treats the
/// remainder as a glob (the estree RUNTIME's expandWord does that too),
/// but bash — and the rust backend's own case render — treat a fully
/// quoted pattern as its LITERAL text (`"*"` is a literal star). Taking
/// the quoted text literally matches the rust/perl renders this
/// transform's output must stay byte-equivalent with.
fn classify_pattern(raw: &str) -> Option<CasePat> {
    let len = raw.len();
    if len >= 2 {
        let inner = if let Some(i) = raw.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
            (i.len() == len - 2).then_some(i)
        } else if let Some(i) = raw.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
            (i.len() == len - 2).then_some(i)
        } else {
            None
        };
        if let Some(inner) = inner {
            // A quoted `$`/backtick is a runtime-expanded pattern, a
            // backslash escape needs the glob engine's unescaping —
            // both stay on the backend render.
            if inner.contains(['$', '`', '\\']) {
                return None;
            }
            return Some(CasePat::Exact(inner.to_string()));
        }
    }
    // partially quoted (`a"b`, `"a"b`) — the backends read those their
    // own way; refuse rather than half-guess.
    if raw.contains(['"', '\'']) {
        return None;
    }
    classify_case_pat(raw)
}

/// The bind's init: TEXT-shaped subjects bind verbatim, everything
/// else is wrapped in the `Interpolate` that is its expansion
/// (`$((…))` → `"$((…))"`, `$RANDOM` → `"$RANDOM"`). bash's case word
/// is a STRING, so the temp must stay text-typed on every backend:
/// an `Arith` init types it `Int`, and the C renderer then wraps the
/// native integer init in `atoll((char*)(…))` — the VALUE read as a
/// pointer (the MIMEcroft demo's `atoll(0x1)` SIGSEGV) — and renders
/// each cond as `long long != "pat"`, a pointer compare where a
/// strcmp belongs (`str_cmp_valued` refuses an Int-typed temp, so the
/// string-compare arm is never taken). `Capture`/`Interpolate`/`Str`
/// already render as text everywhere (and the
/// `impure_discriminant_binds_once` test pins the Capture shape).
fn bind_init(disc: &IrExpr) -> IrExpr {
    match disc {
        IrExpr::Str(..) | IrExpr::Interpolate(..) | IrExpr::Capture { .. } => disc.clone(),
        other => IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(other.clone()))]),
    }
}

/// `Not(disc ne lit)` — exact string equality (see module docs).
fn exact_cond(disc: &IrExpr, lit: &str) -> IrExpr {
    not_wrap(IrExpr::BinOp {
        op: BinOpKind::Ne,
        lhs: Box::new(disc.clone()),
        rhs: Box::new(IrExpr::Str(lit.to_string(), StrStyle::SingleQuoted)),
    })
}

/// `strHasPrefix` / `strHasSuffix` / `contains` over the discriminant
/// and a byte-verbatim literal — the shared test-lowering primitives.
fn affix_cond(disc: &IrExpr, func: &str, lit: &str) -> IrExpr {
    IrExpr::Call {
        func: func.to_string(),
        args: vec![
            disc.clone(),
            IrExpr::Str(lit.to_string(), StrStyle::SingleQuoted),
        ],
    }
}

/// `globMatch(value, pattern)` — the shared glob predicate (module
/// docs). The PATTERN is the raw (unquoted — a quoted pattern is an
/// Exact) glob text: it passes verbatim to whichever glob engine the
/// accepting backend already owns, so no translation can diverge.
fn glob_cond(disc: &IrExpr, pat: &str) -> IrExpr {
    IrExpr::Call {
        func: "globMatch".to_string(),
        args: vec![
            disc.clone(),
            IrExpr::Str(pat.to_string(), StrStyle::SingleQuoted),
        ],
    }
}

/// Targets whose renderers carry a verified `globMatch` arm. Everything
/// else — perl/go/zig/python/js, the shell-family, and the unset/`shir`
/// byte-pinned embed path — refuses the glob clauses and keeps the
/// backend's own case render (rejection is safe: nothing changes for a
/// target that never sees the op).
fn glob_target_ok() -> bool {
    match crate::transforms::target() {
        Some(t) => matches!(t.as_str(), "estree" | "rs" | "rust" | "java"),
        None => false,
    }
}

/// True when duplicating the discriminant once per condition is safe:
/// a cheap read with no side effect and no command/arithmetic
/// evaluation (bash evaluates the case word ONCE). Reserved bash
/// specials refuse — the runtime re-reads them live (`RANDOM` draws a
/// fresh number per read), where bash pins ONE value at case entry.
fn pure_subject(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(..) | IrExpr::Int(_) => true,
        IrExpr::Var(name, _) => !is_reserved_var(name),
        IrExpr::Index { key, .. } => pure_subject(key),
        // `"$1"` — the dominant case subject: quoted interpolation of
        // pure reads is a pure read (each Expr part must be one too —
        // `"${v:-$(cmd)}"` carries a Capture and refuses through it)
        IrExpr::Interpolate(parts) => parts
            .iter()
            .all(|p| match p {
                InterpPart::Lit(_) => true,
                InterpPart::Expr(inner) => pure_subject(inner),
            }),
        // The optimizer's DCE-safe lowering of a plain `$name` read:
        // `getVar("name")`. A read, not a draw — every backend renders
        // it back as the variable itself (rust's var-tracking sees
        // through it via its `getVar` var-read extractor, perl renders
        // `var_read`), so re-reading per condition observes the same
        // value a single pre-case binding would. Volatile specials are
        // refused by name, exactly like the `Var` arm above.
        IrExpr::Call { func, args } => {
            func == "getVar"
                && match args.as_slice() {
                    [IrExpr::Str(name, _)] => !name.is_empty() && !is_reserved_var(name),
                    _ => false,
                }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrCaseClause;

    fn var() -> IrExpr {
        IrExpr::Var("x".to_string(), None)
    }
    fn pat_case(patterns: &[&str]) -> IrStmt {
        IrStmt::Case {
            discriminant: var(),
            clauses: vec![IrCaseClause {
                patterns: patterns.iter().map(|s| s.to_string()).collect(),
                body: vec![IrStmt::Break],
            }],
        }
    }
    fn lower(stmt: IrStmt) -> Option<IrStmt> {
        let mut v = vec![stmt];
        let changed = transform(&mut v);
        let out = v.into_iter().next().expect("one stmt");
        match out {
            IrStmt::Case { .. } if changed => unreachable!("changed but not lowered"),
            IrStmt::Case { .. } => None,
            other => Some(other),
        }
    }

    /// The optimizer lowers a plain `$1` read to `getVar("1")` before
    /// transforms run — the transform must see through it, or every
    /// corpus case refuses on the subject (the first real-IR surprise).
    #[test]
    fn optimizer_getvar_subject_lowers() {
        let stmt = IrStmt::Case {
            discriminant: IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("1".to_string(), StrStyle::DoubleQuoted)],
            },
            clauses: vec![IrCaseClause {
                patterns: vec!["\"start\"".into()],
                body: vec![IrStmt::Break],
            }],
        };
        let IrStmt::If { cond, .. } = lower(stmt).expect("getvar subject must lower") else {
            panic!("not an If")
        };
        assert!(format!("{cond:?}").contains("start"), "{cond:?}");
    }

    /// …and a volatile special BINDS once through getVar — `RANDOM`
    /// draws a single fresh value into the temp at chain entry, exactly
    /// the one value bash pins at case entry (the old refusal kept the
    /// backend's own render, which some backends re-read per clause; a
    /// temp is strictly closer to bash).
    #[test]
    fn getvar_reserved_name_binds_once() {
        let stmt = IrStmt::Case {
            discriminant: IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("RANDOM".to_string(), StrStyle::DoubleQuoted)],
            },
            clauses: vec![IrCaseClause {
                patterns: vec!["1".into()],
                body: vec![IrStmt::Break],
            }],
        };
        let IrStmt::Block(stmts) = lower(stmt).expect("RANDOM must lower via a binding")
        else {
            panic!("expected Block[Declare, If]");
        };
        assert_eq!(stmts.len(), 2, "{stmts:?}");
        assert!(
            matches!(&stmts[0], IrStmt::Declare { init: Some(_), .. }),
            "bind first: {:?}",
            stmts[0]
        );
        assert!(matches!(&stmts[1], IrStmt::If { .. }), "chain second");
    }

    #[test]
    fn literal_and_affix_clauses_lower_to_native_tests() {
        let stmt = IrStmt::Case {
            discriminant: var(),
            clauses: vec![
                IrCaseClause {
                    patterns: vec!["start".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["*mid*".into()],
                    body: vec![IrStmt::Continue],
                },
                IrCaseClause {
                    patterns: vec!["end*".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["*tail".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["*".into()],
                    body: vec![IrStmt::Continue],
                },
            ],
        };
        let IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } = lower(stmt).expect("must lower")
        else {
            panic!("not an If")
        };
        // exact → Not(Ne(...)); affix → the shared primitives
        let c = format!("{cond:?}");
        assert!(c.contains("Not") && c.contains("Ne") && c.contains("\"start\""), "{c}");
        assert_eq!(elsifs.len(), 3, "three elif clauses: {elsifs:?}");
        assert!(format!("{:?}", elsifs[0].0).contains("contains"), "mid* → contains");
        assert!(format!("{:?}", elsifs[1].0).contains("strHasPrefix"), "end* → prefix");
        assert!(format!("{:?}", elsifs[2].0).contains("strHasSuffix"), "*tail → suffix");
        assert_eq!(else_.len(), 1, "`*` becomes the else branch");
        assert!(matches!(else_[0], IrStmt::Continue));
        assert_eq!(then.len(), 1);
    }

    #[test]
    fn quoted_star_is_a_literal_not_the_catchall() {
        // `"*"` matches the literal star — never the else branch
        let IrStmt::If { cond, else_, .. } = lower(pat_case(&["\"*\""])).expect("lowers")
        else {
            panic!("not an If")
        };
        let c = format!("{cond:?}");
        assert!(c.contains("Ne") && c.contains("\"*\""), "{c}");
        assert!(else_.is_empty(), "quoted star is not a catch-all");
    }

    #[test]
    fn glob_class_refuses_whole_case() {
        // `i*86` needs a glob engine — the backend's own render keeps it
        // (the test thread has no target set: the globMatch arm is
        // target-gated, so unset/`shir` still refuses)
        assert!(lower(pat_case(&["i*86"])).is_none());
        assert!(lower(pat_case(&["[a-z]*"])).is_none());
        assert!(lower(pat_case(&["?"])).is_none());
        // one bad clause refuses the WHOLE dispatch
        let stmt = IrStmt::Case {
            discriminant: var(),
            clauses: vec![
                IrCaseClause {
                    patterns: vec!["ok".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["*?".into()],
                    body: vec![IrStmt::Break],
                },
            ],
        };
        assert!(lower(stmt).is_none());
    }

    /// RAII target pin: the target is a THREAD-local, and the suite runs
    /// on one harness thread (workspace `.cargo/config.toml`), so a leak
    /// would silently retarget every later test. Restore on drop, even
    /// under panic.
    struct TgtGuard;
    impl TgtGuard {
        fn set(t: &str) -> Self {
            crate::transforms::set_target(Some(t));
            TgtGuard
        }
    }
    impl Drop for TgtGuard {
        fn drop(&mut self) {
            crate::transforms::set_target(None);
        }
    }

    #[test]
    fn glob_clause_lowers_via_globmatch_on_accepting_target() {
        let _g = TgtGuard::set("rust");
        let stmt = IrStmt::Case {
            discriminant: var(),
            clauses: vec![
                IrCaseClause {
                    patterns: vec!["[a-z]*".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["i*86".into()],
                    body: vec![IrStmt::Continue],
                },
                IrCaseClause {
                    patterns: vec!["*".into()],
                    body: vec![IrStmt::Break],
                },
            ],
        };
        let IrStmt::If { cond, elsifs, else_, .. } = lower(stmt).expect("glob case lowers on an accepting target") else {
            panic!("not an If")
        };
        let c = format!("{cond:?}");
        assert!(c.contains("globMatch") && c.contains("[a-z]*"), "first cond is the class glob: {c}");
        let c2 = format!("{:?}", elsifs[0].0);
        assert!(c2.contains("globMatch") && c2.contains("i*86"), "mid-star glob predicate: {c2}");
        assert!(!else_.is_empty(), "catch-all becomes the else branch");
        // the pattern rides VERBATIM (SingleQuoted, no regex re-translation)
        assert!(c.contains("SingleQuoted"), "byte-verbatim pattern: {c}");
    }

    #[test]
    fn glob_clause_refuses_on_non_accepting_targets() {
        for t in ["perl", "go", "zig", "python", "sh"] {
            let _g = TgtGuard::set(t);
            assert!(
                lower(pat_case(&["[a-z]*"])).is_none(),
                "target {t} has no globMatch arm — the case must stay on its own render"
            );
        }
        // …and mixed cases refuse WHOLE (the exact/affix clauses of a
        // case containing a glob must not half-lower for such a target)
        let _g = TgtGuard::set("perl");
        let stmt = IrStmt::Case {
            discriminant: var(),
            clauses: vec![
                IrCaseClause {
                    patterns: vec!["ok".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["[0-9]*".into()],
                    body: vec![IrStmt::Break],
                },
            ],
        };
        assert!(lower(stmt).is_none(), "one non-armed glob refuses the whole dispatch");
    }

    #[test]
    fn dynamic_and_partial_quote_patterns_refuse() {
        assert!(lower(pat_case(&["$pat"])).is_none());
        assert!(lower(pat_case(&["\"$pat\""])).is_none());
        assert!(lower(pat_case(&["a\"b"])).is_none());
        assert!(lower(pat_case(&["x\\y"])).is_none());
    }

    #[test]
    fn impure_discriminant_binds_once() {
        let stmt = IrStmt::Case {
            discriminant: IrExpr::Capture {
                expr: Box::new(IrExpr::Str("cmd".into(), StrStyle::DoubleQuoted)),
                native: false,
            },
            clauses: vec![IrCaseClause {
                patterns: vec!["a".into()],
                body: vec![IrStmt::Break],
            }],
        };
        let IrStmt::Block(stmts) = lower(stmt).expect("capture subject binds and lowers")
        else {
            panic!("expected Block[Declare, If]");
        };
        assert_eq!(stmts.len(), 2, "{stmts:?}");
        assert!(
            matches!(&stmts[0], IrStmt::Declare { init: Some(IrExpr::Capture { .. }), .. }),
            "temp binds the capture: {:?}",
            stmts[0]
        );
        let IrStmt::If { cond, .. } = &stmts[1] else {
            panic!("second stmt is the chain")
        };
        let c = format!("{cond:?}");
        assert!(c.contains("__sh2_casesubj_"), "cond reads the temp: {c}");
        assert!(!c.contains("Capture"), "capture must not repeat per condition: {c}");
    }

    /// An `Arith` subject (`case $((frame % 200)) in`) must bind its
    /// TEXT form: the temp stays string-typed, so every backend's
    /// string conds (strcmp/fnmatch/regex) apply. Binding the bare
    /// `Arith` types the temp `Int` and the C render emits
    /// `atoll((char*)(<native int>))` — the value read as a pointer —
    /// plus `long long != "pat"` pointer compares (the MIMEcroft demo
    /// SIGSEGV this fixes).
    #[test]
    fn arith_subject_binds_text_form() {
        use crate::ir::ArithAst;
        let stmt = IrStmt::Case {
            discriminant: IrExpr::Arith(Box::new(ArithAst::Bin {
                op: "%".into(),
                lhs: Box::new(ArithAst::Var("frame".into())),
                rhs: Box::new(ArithAst::Num(200)),
            })),
            clauses: vec![IrCaseClause {
                patterns: vec!["25".into()],
                body: vec![IrStmt::Break],
            }],
        };
        let IrStmt::Block(stmts) = lower(stmt).expect("arith subject binds and lowers")
        else {
            panic!("expected Block[Declare, If]");
        };
        let IrStmt::Declare { init: Some(init), .. } = &stmts[0] else {
            panic!("bind first: {:?}", stmts[0])
        };
        assert!(
            matches!(init, IrExpr::Interpolate(_)),
            "text-form bind (not the bare Arith): {init:?}"
        );
    }

    /// A name already in the tree (`__sh2_casesubj_0`) is probe-skipped
    /// — the bind allocates the next free index instead.
    #[test]
    fn fresh_name_skips_probe_collisions() {
        let mut stmts = vec![
            IrStmt::Declare {
                vars: vec![Decl {
                    name: "__sh2_casesubj_0".into(),
                    sigil: None,
                }],
                init: None,
                local: false,
            },
            IrStmt::Case {
                discriminant: IrExpr::Capture {
                    expr: Box::new(IrExpr::Str("cmd".into(), StrStyle::DoubleQuoted)),
                    native: false,
                },
                clauses: vec![IrCaseClause {
                    patterns: vec!["a".into()],
                    body: vec![IrStmt::Break],
                }],
            },
        ];
        assert!(transform(&mut stmts));
        let IrStmt::Block(inner) = &stmts[1] else {
            panic!("case lowered to a Block")
        };
        let IrStmt::Declare { vars, .. } = &inner[0] else {
            panic!("bind first: {inner:?}")
        };
        assert_eq!(vars[0].name, "__sh2_casesubj_1", "probe skips the taken name");
    }

    #[test]
    fn leading_catchall_refuses() {
        let stmt = IrStmt::Case {
            discriminant: var(),
            clauses: vec![
                IrCaseClause {
                    patterns: vec!["*".into()],
                    body: vec![IrStmt::Break],
                },
                IrCaseClause {
                    patterns: vec!["never".into()],
                    body: vec![IrStmt::Break],
                },
            ],
        };
        assert!(lower(stmt).is_none());
    }

    #[test]
    fn alternations_split_into_ored_conditions() {
        let IrStmt::If { cond, .. } = lower(pat_case(&["on|start"])).expect("lowers") else {
            panic!("not an If")
        };
        let c = format!("{cond:?}");
        assert!(c.contains("Or"), "alternatives OR together: {c}");
        assert!(c.contains("\"on\"") && c.contains("\"start\""), "{c}");
    }

    #[test]
    fn nocasematch_mention_refuses_the_whole_program() {
        let mut stmts = vec![pat_case(&["abc"])];
        stmts.push(IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: "s".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Str("nocasematch".into(), StrStyle::DoubleQuoted),
            asm: None,
        });
        assert!(!transform(&mut stmts));
        assert!(matches!(stmts[0], IrStmt::Case { .. }), "case untouched");
    }

    #[test]
    fn quoted_positional_subject_lowers_volatile_binds() {
        // `case "$1" in …` — Interpolate over a pure Var: the DOMINANT
        // subject shape; must lower.
        let stmt = IrStmt::Case {
            discriminant: IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(IrExpr::Var("1".into(), None))),
            ]),
            clauses: vec![IrCaseClause {
                patterns: vec!["ok".into()],
                body: vec![IrStmt::Break],
            }],
        };
        assert!(lower(stmt).is_some(), "\"$1\" subject must lower");
        // `case $RANDOM in` — a runtime-live special: the chain binds
        // it ONCE into the temp (the single value bash pins at entry)
        // instead of re-drawing per condition.
        for subj in [
            IrExpr::Var("RANDOM".into(), None),
            IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
                "SECONDS".into(),
                None,
            )))]),
        ] {
            let stmt = IrStmt::Case {
                discriminant: subj,
                clauses: vec![IrCaseClause {
                    patterns: vec!["5".into()],
                    body: vec![IrStmt::Break],
                }],
            };
            let IrStmt::Block(stmts) = lower(stmt).expect("volatile subject binds")
            else {
                panic!("expected Block[Declare, If]")
            };
            assert!(
                matches!(&stmts[0], IrStmt::Declare { init: Some(_), .. }),
                "bind first: {:?}",
                stmts[0]
            );
            assert!(matches!(&stmts[1], IrStmt::If { .. }), "chain second");
        }
    }

    #[test]
    fn nested_case_in_function_body_lowers() {
        let inner = pat_case(&["lit"]);
        let mut stmts = vec![IrStmt::Function {
            name: "f".to_string(),
            params: vec![],
            body: vec![inner],
            named_blocks: Default::default(),
        }];
        assert!(transform(&mut stmts));
        let IrStmt::Function { body, .. } = &stmts[0] else {
            panic!("still a function")
        };
        assert!(matches!(body[0], IrStmt::If { .. }), "nested case lowered");
    }
}
