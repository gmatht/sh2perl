//! grep-pcre — `grep -P 'PCRE'` → portable `grep -E 'ERE'`.
//!
//! `grep -P` is a GNU-ONLY extension (absent on busybox, BSD, macOS).
//! This transform rewrites the pattern to POSIX ERE wherever the PCRE
//! constructs are translatable, so the emitted program runs grep -E
//! anywhere. REFUSE > GUESS: any construct outside the supported subset
//! keeps the original `grep -P` exec (correct, unoptimized).
//!
//! ## Supported translations
//!
//! | PCRE | ERE | status |
//! |---|---|---|
//! | `(?:X)` non-capturing | `(X)` | exact (groups are only scoping) |
//! | `(?>X)` atomic | `(X)` | exact — atomic groups recognize the SAME language (perf only) |
//! | `X*+`/`X++`/`X?+` possessive | `X*`/`X+`/`X?` | exact — same language (perf only) |
//! | `\d \D \s \S \w \W` | `[0-9]`, `[^0-9]`, `[[:space:]]`, `[^[:space:]]`, `[[:alnum:]_]`, `[^[:alnum:]_]` | exact |
//! | `(?<=X)Y` leading lookbehind, X a fixed literal | `XY` | filter/tests: exact; `-o`: exact ONCE a `sed 's/^X//'` strip stage follows (only inside command substitution) |
//!
//! ## Refused (kept as `grep -P`)
//! Lookaheads (`(?=`/`(?!`) and negative lookbehind — same-position
//! semantics ERE cannot express; non-literal (variable-length / regex)
//! lookbehind content — the -o absorption is only faithful for a fixed
//! literal prefix; `\b \B \K` (GNU-only / PCRE), backreferences `\1…`,
//! lazy quantifiers `*?` (semantics differ), control/hex/unicode escapes
//! (`\t \x41 \p{…} …`), `-E`/`-F` mixed with `-P`, unknown flags, and
//! multiple patterns. All refused greps keep their `grep -P`.
//!
//! ## Variable patterns
//! A pattern argument that reads a variable (`grep -P "$PAT"`) is NOT
//! refused — the variable is reduced to its static value first. The
//! transform maintains a sequential constant map of the surrounding
//! program: a single static `Assign`/`Declare` of a literal string
//! (including interpolations of already-known constants) records the
//! variable; any dynamic write (`read`, `unset`, a non-static
//! reassignment, arrays) invalidates it. Reads are resolved with the map
//! state AT the read position (def-before-read order), and branch bodies
//! are walked with a copy so their writes never leak past the branch.
//!
//! ## Interaction with the other grep transforms
//! text-ops and grep-o only touch flag-less / `-o`-only grep shapes;
//! `-P` greps are refused by both and fall through to this transform.

use crate::ir::{InterpPart, IrExpr, IrStmt, StrStyle};
use std::collections::HashMap;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut consts = Consts::new();
    body(stmts, &mut consts, false)
}

// ── Sequential constant map ─────────────────────────────────────────
//
// var → statically-known string value. Kept in statement order by the
// walker: an `Assign`/`Declare` records (or, when non-static, removes)
// its target; a branch/loop body is walked with a clone so its writes
// don't leak past its scope.

#[derive(Clone, Default)]
struct Consts {
    vals: HashMap<String, String>,
}

impl Consts {
    fn new() -> Self {
        Consts { vals: HashMap::new() }
    }
    fn resolve(&self, var: &str) -> Option<String> {
        self.vals.get(var).cloned()
    }
    fn clear(&mut self) {
        self.vals.clear();
    }
    /// Record a static value; `None` invalidates (removes) the entry.
    fn record(&mut self, var: &str, value: Option<String>) {
        match value {
            Some(v) => {
                self.vals.insert(var.to_string(), v);
            }
            None => {
                self.vals.remove(var);
            }
        }
    }
}

/// The static-string value of an expression: a literal, an all-literal
/// interpolation, or a read of an already-resolved constant variable.
fn static_string_of(e: &IrExpr, consts: &Consts) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(inner) => {
                        out.push_str(&getvar_const(inner, consts)?);
                    }
                }
            }
            Some(out)
        }
        _ => getvar_const(e, consts),
    }
}

fn getvar_const(e: &IrExpr, consts: &Consts) -> Option<String> {
    match e {
        IrExpr::Call { func, args } if func == "getVar" && args.len() == 1 => {
            if let IrExpr::Str(v, _) = &args[0] {
                consts.resolve(v)
            } else {
                None
            }
        }
        IrExpr::Var(v, _) => consts.resolve(v),
        _ => None,
    }
}

// ── Statement walker ────────────────────────────────────────────────

fn body(stmts: &mut [IrStmt], consts: &mut Consts, in_capture: bool) -> bool {
    let mut changed = false;
    for s in stmts.iter_mut() {
        changed |= stmt(s, consts, in_capture);
    }
    changed
}

fn stmt(st: &mut IrStmt, consts: &mut Consts, in_capture: bool) -> bool {
    match st {
        IrStmt::Expr(e) => expr(e, consts, in_capture),
        IrStmt::Assign { targets, expr: rhs, .. } => {
            // record constants (invalidating on any non-static write)
            for t in targets.iter() {
                if t.indices.is_empty() {
                    consts.record(&t.var, static_string_of(rhs, consts));
                } else {
                    consts.record(&t.var, None);
                }
            }
            // the RHS may contain a capture wrapping a grep
            expr(rhs, consts, in_capture)
        }
        IrStmt::Declare { vars, init: Some(rhs), .. } => {
            for v in vars.iter() {
                consts.record(&v.name, static_string_of(rhs, consts));
            }
            expr(rhs, consts, in_capture)
        }
        IrStmt::Declare { vars, init: None, .. } => {
            for v in vars.iter() {
                consts.record(&v.name, None);
            }
            false
        }
        IrStmt::DeclareArray { var, .. } => {
            consts.record(var, None);
            false
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b) => body(b, consts, in_capture),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = expr(cond, consts, in_capture);
            {
                let mut k = consts.clone();
                for s in then.iter_mut() {
                    c |= stmt(s, &mut k, in_capture);
                }
            }
            for (ce, b) in elsifs.iter_mut() {
                c |= expr(ce, consts, in_capture);
                let mut k = consts.clone();
                for s in b.iter_mut() {
                    c |= stmt(s, &mut k, in_capture);
                }
            }
            let mut k = consts.clone();
            for s in else_.iter_mut() {
                c |= stmt(s, &mut k, in_capture);
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = expr(cond, consts, in_capture);
            let mut k = consts.clone();
            for s in body.iter_mut() {
                c |= stmt(s, &mut k, in_capture);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = expr(cond, consts, in_capture);
            let mut k = consts.clone();
            for s in body.iter_mut() {
                c |= stmt(s, &mut k, in_capture);
            }
            c
        }
        IrStmt::For { var, iter, body } => {
            // a write to the loop var inside the body must not be const;
            // also the loop var itself is dynamic — drop it from the map.
            consts.record(var, None);
            let mut c = expr(iter, consts, in_capture);
            let mut k = consts.clone();
            for s in body.iter_mut() {
                c |= stmt(s, &mut k, in_capture);
            }
            c
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut c = false;
            for s in init.iter_mut() {
                c |= stmt(s, consts, in_capture);
            }
            c |= expr(cond, consts, in_capture);
            for s in step.iter_mut() {
                c |= stmt(s, consts, in_capture);
            }
            let mut k = consts.clone();
            for s in body.iter_mut() {
                c |= stmt(s, &mut k, in_capture);
            }
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = expr(discriminant, consts, in_capture);
            for cl in clauses.iter_mut() {
                let mut k = consts.clone();
                for s in cl.body.iter_mut() {
                    c |= stmt(s, &mut k, in_capture);
                }
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
                c |= stmt(s, consts, in_capture);
            }
            for e in excepts.iter_mut() {
                // except handlers run later — their writes must not leak
                let mut k = consts.clone();
                for s in e.body.iter_mut() {
                    c |= stmt(s, &mut k, in_capture);
                }
            }
            for s in else_body.iter_mut() {
                c |= stmt(s, consts, in_capture);
            }
            for s in finally_body.iter_mut() {
                c |= stmt(s, consts, in_capture);
            }
            c
        }
        IrStmt::Function { body, .. } => {
            // the body runs at CALL time (later); walk it with a copy so
            // its writes don't leak, but it CAN read prior consts.
            let mut k = consts.clone();
            self::body(body, &mut k, in_capture)
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut c = body(inner, consts, in_capture);
            for r in redirects.iter_mut() {
                // here-string/heredoc targets can carry captures
                c |= expr(&mut r.target, consts, true);
            }
            c
        }
        IrStmt::Pipeline {
            stages,
            capture,
            cmd_str: _,
            ..
        } => {
            // legacy shape (cmd_str-carrying qx pipelines): rewrite argv
            // in place, but never insert a sed stage (the reconstructed
            // command string would go stale). Strip-requiring greps simply
            // refuse inside (in_capture set false → lookbehind -o kept).
            let mut c = false;
            let ic = capture.is_some();
            for s in stages.iter_mut() {
                c |= body(s, consts, ic);
            }
            c
        }
        IrStmt::Output { value: v, .. } => expr(v, consts, true),
        IrStmt::WriteFile { content, .. } => {
            expr(content, consts, true)
        }
        _ => false,
    }
}

fn expr(e: &mut IrExpr, consts: &mut Consts, in_capture: bool) -> bool {
    match e {
        IrExpr::Call { func, args } => {
        match func.as_str() {
            "pipeline" => {
                let [IrExpr::Array(stages)] = args.as_mut_slice() else {
                    return false;
                };
                let mut changed = false;
                let mut i = 0;
                while i < stages.len() {
                    let strip_here: Option<String> = {
                        let stg = &mut stages[i];
                        if let IrExpr::Arrow(stmts) = stg {
                            if stmts.len() == 1 {
                                if let IrStmt::Expr(IrExpr::Call { func, args }) = &mut stmts[0] {
                                    if (func == "exec" || func == "builtin") && is_grep(args) {
                                        match rewrite_grep(func, args, consts, in_capture) {
                                            G::Leave => None,
                                            G::Rewritten => {
                                                changed = true;
                                                None
                                            }
                                            G::Strip(script) => {
                                                changed = true;
                                                Some(script)
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let Some(script) = strip_here {
                        stages.insert(i + 1, sed_stage(&script));
                        i += 1; // skip the stage we just inserted
                    } else if let IrExpr::Arrow(stmts) = &mut stages[i] {
                        // multi-statement / non-grep stage: walk it, but the
                        // -o lookbehind strip can't be placed here → refuse
                        // that case by passing in_capture=false.
                        let mut k = consts.clone();
                        changed |= body(stmts, &mut k, false);
                    }
                    i += 1;
                }
                changed
            }
            "capture" | "captureWords" | "commandSubstitution" => {
                let mut c = false;
                for a in args.iter_mut() {
                    c |= expr(a, consts, true);
                }
                c
            }
            "exec" | "builtin" => {
                let name = match &args[0] {
                    IrExpr::Str(n, _) => n.clone(),
                    _ => return false,
                };
                match name.as_str() {
                    "read" | "unset" => {
                        // dynamic writes — invalidate the named variables
                        if let Some(IrExpr::Array(argv)) = args.get(1) {
                            for a in argv.iter() {
                                match a {
                                    IrExpr::Str(s, _) => consts.record(s, None),
                                    IrExpr::Var(v, _) => consts.record(v, None),
                                    IrExpr::Ident(v) => consts.record(v, None),
                                    _ => {}
                                }
                            }
                        }
                        false
                    }
                    "eval" => {
                        // anything could be executed — drop all consts
                        consts.clear();
                        false
                    }
                    _ => {
                        matches!(
                            rewrite_grep(func, args, consts, in_capture),
                            G::Rewritten | G::Strip(_)
                        )
                    }
                }
            }
            _ => {
                let mut c = false;
                for a in args.iter_mut() {
                    c |= expr(a, consts, in_capture);
                }
                c
            }
        }
        },
        IrExpr::Capture { expr: inner, .. } => {
            // A capture may directly wrap the grep (`$(grep -P …)`) or a
            // pipeline. Direct exec → the -o strip can wrap it into a
            // two-stage pipeline; otherwise recurse with in_capture=true.
            let func_name = match inner.as_ref() {
                IrExpr::Call { func, .. } => func.clone(),
                _ => String::new(),
            };
            if func_name == "exec" || func_name == "builtin" {
                if let IrExpr::Call { func, args } = inner.as_mut() {
                    match rewrite_grep(func, args, consts, true) {
                        G::Leave => false,
                        G::Rewritten => true,
                        G::Strip(script) => {
                            let old = std::mem::replace(inner.as_mut(), IrExpr::Int(0));
                            **inner = IrExpr::Call {
                                func: "pipeline".to_string(),
                                args: vec![IrExpr::Array(vec![
                                    IrExpr::Arrow(vec![IrStmt::Expr(old)]),
                                    sed_stage(&script),
                                ])],
                            };
                            true
                        }
                    }
                } else {
                    false
                }
            } else {
                expr(inner, consts, true)
            }
        }
        IrExpr::Arrow(stmts) => body(stmts, consts, in_capture),
        IrExpr::Array(items) => {
            let mut c = false;
            for it in items.iter_mut() {
                c |= expr(it, consts, in_capture);
            }
            c
        }
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(inner) = p {
                    c |= expr(inner, consts, in_capture);
                }
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. }
        | IrExpr::DefinedOr { expr: lhs, default: rhs } => {
            expr(lhs, consts, in_capture) | expr(rhs, consts, in_capture)
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr(cond, consts, in_capture)
                | expr(then, consts, in_capture)
                | expr(else_, consts, in_capture)
        }
        IrExpr::Index { key, .. } => expr(key, consts, in_capture),
        _ => false,
    }
}

fn is_grep(args: &[IrExpr]) -> bool {
    matches!(&args[0], IrExpr::Str(n, _) if n == "grep")
}

// ── The grep argv rewrite ───────────────────────────────────────────

enum G {
    /// left unchanged (not grep -P / not translatable / -o strip not placeable)
    Leave,
    /// argv rewritten to -E; outputs unchanged
    Rewritten,
    /// argv rewritten; grep -o output needs a following `sed 's/^X//'`
    Strip(String),
}

/// Rewrite a `grep` exec argv: `-P` → `-E` with the pattern translated.
fn rewrite_grep(func: &str, args: &mut Vec<IrExpr>, consts: &Consts, in_capture: bool) -> G {
    if func != "exec" && func != "builtin" {
        return G::Leave;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(argv)] = args.as_slice() else {
        return G::Leave;
    };
    if name != "grep" {
        return G::Leave;
    }
    // staticize every arg (resolving $VAR patterns to their const value)
    let mut strs: Vec<String> = Vec::new();
    for a in argv.iter() {
        match static_string_of(a, consts) {
            Some(s) => strs.push(s),
            None => return G::Leave, // dynamic/unresolvable arg → keep grep -P
        }
    }
    // collect flags; `-P` triggers; first non-flag arg is the pattern
    let mut pass = String::new();
    let mut has_p = false;
    let mut has_e = false;
    let mut has_f = false;
    let mut pattern: Option<usize> = None;
    let mut after_dd = false;
    for (i, s) in strs.iter().enumerate() {
        if !after_dd && s == "--" {
            after_dd = true;
            continue;
        }
        if !after_dd && s.len() > 1 && s.starts_with('-') {
            for c in s[1..].chars() {
                match c {
                    'P' => has_p = true,
                    'E' => has_e = true,
                    'F' => has_f = true,
                    'o' | 'v' | 'c' | 'q' | 'l' | 'n' | 's' | 'w' | 'x' | 'i' => {
                        if !pass.contains(c) {
                            pass.push(c);
                        }
                    }
                    _ => return G::Leave, // unknown flag → keep grep -P
                }
            }
            continue;
        }
        if pattern.is_none() {
            pattern = Some(i);
        } else if s.len() > 1 && s.starts_with('-') {
            // an option-like arg after the pattern is ambiguous (getopt
            // permutation differs per grep) → refuse
            return G::Leave;
        }
    }
    let Some(pi) = pattern else {
        return G::Leave; // no pattern
    };
    if !has_p || has_e || has_f {
        return G::Leave; // not -P, or mixed modes
    }
    // translate the PCRE pattern
    let (ere, prefix) = match translate_pcre(&strs[pi]) {
        Some(t) => t,
        None => return G::Leave,
    };
    // -o with an absorbed lookbehind changes the printed bytes: the -o
    // output needs a sed strip stage, placeable only inside a capture.
    let strip = match prefix {
        Some(x) if pass.contains('o') => Some(format!("s/^{}//", sed_literal(&x))),
        _ => None,
    };
    if strip.is_some() && !in_capture {
        return G::Leave;
    }
    // rebuild argv: [flags, --?, pattern, files…]
    let mut fl = String::from("-E");
    for c in pass.chars() {
        if c != 'E' {
            fl.push(c);
        }
    }
    let mut new_argv: Vec<IrExpr> = vec![
        IrExpr::Str(fl, StrStyle::DoubleQuoted),
        IrExpr::Str(ere, StrStyle::SingleQuoted),
    ];
    for f in argv[pi + 1..].iter() {
        new_argv.push(f.clone());
    }
    let argv_slot = match args.get_mut(1) {
        Some(IrExpr::Array(a)) => a,
        _ => return G::Leave,
    };
    *argv_slot = new_argv;
    match strip {
        Some(s) => G::Strip(s),
        None => G::Rewritten,
    }
}

/// The sed strip stage: `sed 's/^X//'` (BRE, X escaped to a literal).
fn sed_stage(script: &str) -> IrExpr {
    IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![
            IrExpr::Str("sed".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(vec![IrExpr::Str(script.to_string(), StrStyle::DoubleQuoted)]),
        ],
    })])
}

/// Escape a literal string for the PATTERN side of a BRE `s///` script.
fn sed_literal(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '.' | '*' | '[' | ']' | '^' | '$' | '/' | '&' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

// ── PCRE → ERE pattern translation ──────────────────────────────────

/// Translate a PCRE pattern to ERE. Returns (ERE, absorbed fixed prefix).
/// The prefix is Some only for a leading `(?<=X)` with X a fixed literal.
fn translate_pcre(p: &str) -> Option<(String, Option<String>)> {
    if let Some(rest) = p.strip_prefix("(?<=") {
        let (prefix_lit, rest) = fixed_prefix(rest)?;
        let body = translate_inner(&rest)?;
        // absorbed: `(?<=X)Y` ≈ "XY". Filter/tests: exact. -o: exact
        // once a sed 's/^X//' stage follows (handled by the caller).
        return Some((format!("{}{}", prefix_lit, body), Some(prefix_lit)));
    }
    if p.starts_with("(?=") || p.starts_with("(?!") || p.starts_with("(?<!") {
        return None; // lookaheads / negative lookbehind — same-position
                     // semantics ERE cannot express → keep grep -P
    }
    let body = translate_inner(p)?;
    Some((body, None))
}

/// The literal text of a lookbehind prefix, or None (refused): chars and
/// single-char literal escapes only — no classes, quantifiers, alternation,
/// groups, or PCRE class escapes (variable-length / regex content would
/// break the exact `-o` absorption).
fn fixed_prefix(s: &str) -> Option<(String, String)> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        match c {
            '\\' => {
                if i + 1 >= n {
                    return None;
                }
                let c2 = chars[i + 1];
                if c2.is_alphanumeric() || c2.is_whitespace() || c2 == '_' {
                    return None; // \d \s \x \w … — not a fixed literal
                }
                out.push('\\');
                out.push(c2);
                i += 2;
            }
            ')' => return Some((out, s[i + 1..].to_string())),
            '[' | '(' | '|' | '*' | '+' | '?' | '{' | '.' | '^' | '$' => return None,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    None // unterminated lookbehind
}

/// The core scanner: translate translatable constructs, refuse the rest.
fn translate_inner(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut out = String::new();
    while i < n {
        let c = chars[i];
        match c {
            '(' => {
                if i + 1 < n && chars[i + 1] == '?' {
                    match chars.get(i + 2) {
                        Some(':') => {
                            out.push('('); // non-capturing → capturing
                            i += 3;
                        }
                        Some('>') => {
                            out.push('('); // atomic → plain group (same language)
                            i += 3;
                        }
                        _ => return None, // lookaround / other → refuse
                    }
                } else {
                    out.push('(');
                    i += 1;
                }
            }
            ')' => {
                out.push(')');
                i += 1;
            }
            '\\' => {
                let Some(&c2) = chars.get(i + 1) else {
                    return None;
                };
                match c2 {
                    'd' => {
                        out.push_str("[0-9]");
                        i += 2;
                    }
                    'D' => {
                        out.push_str("[^0-9]");
                        i += 2;
                    }
                    's' => {
                        out.push_str("[[:space:]]");
                        i += 2;
                    }
                    'S' => {
                        out.push_str("[^[:space:]]");
                        i += 2;
                    }
                    'w' => {
                        out.push_str("[[:alnum:]_]");
                        i += 2;
                    }
                    'W' => {
                        out.push_str("[^[:alnum:]_]");
                        i += 2;
                    }
                    // GNU-only word boundaries, \K, backrefs,
                    // hex/unicode/control/octal escapes → refuse
                    'b' | 'B' | 'K' | '1'..='9' | 'x' | 'X' | 'p' | 'P' | 'u' | 'U' | '0'
                    | 'n' | 't' | 'r' | 'f' | 'v' | 'a' | 'e' | 'c' | 'g' | 'k' | 'h' | 'H'
                    | 'R' | 'V' => return None,
                    _ => {
                        out.push('\\'); // escaped literal punctuation
                        out.push(c2);
                        i += 2;
                    }
                }
            }
            '[' => {
                // copy the char class (escapes, POSIX classes, `]` first)
                let cls = copy_class(&chars, &mut i)?;
                out.push_str(&cls);
            }
            '*' | '+' | '?' => {
                out.push(c);
                i += 1;
                // lazy `X?` / possessive `X+` follow the quantifier
                if i < n && chars[i] == '?' {
                    return None; // lazy — semantics differ in ERE
                }
                if i < n && chars[i] == '+' {
                    i += 1; // possessive — same language, drop the marker
                }
            }
            '{' => {
                // `{m}`, `{m,}`, `{m,n}` quantifier or a literal `{`
                let mut j = i + 1;
                let mut saw_digit = false;
                while j < n {
                    let d = chars[j];
                    if d.is_ascii_digit() {
                        saw_digit = true;
                        j += 1;
                    } else if d == ',' {
                        j += 1;
                    } else if d == '}' && saw_digit {
                        break;
                    } else {
                        saw_digit = false;
                        break;
                    }
                }
                if saw_digit && j < n && chars[j] == '}' {
                    for k in i..=j {
                        out.push(chars[k]);
                    }
                    i = j + 1;
                    if i < n && chars[i] == '?' {
                        return None; // lazy interval
                    }
                    if i < n && chars[i] == '+' {
                        i += 1; // possessive interval — drop
                    }
                } else {
                    out.push('{'); // literal
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(out)
}

/// Copy a character class starting at `chars[i] == '['` verbatim (handles
/// escapes, POSIX classes `[:…:]`, and `]` as the first character).
fn copy_class(chars: &[char], i: &mut usize) -> Option<String> {
    let n = chars.len();
    let mut cls = String::from("[");
    *i += 1;
    let mut first = true;
    loop {
        if *i >= n {
            return None; // unterminated class
        }
        let d = chars[*i];
        if !first && d == ']' {
            cls.push(']');
            *i += 1;
            return Some(cls);
        }
        if d == '\\' {
            if *i + 1 >= n {
                return None;
            }
            cls.push('\\');
            cls.push(chars[*i + 1]);
            *i += 2;
        } else if d == '[' && *i + 1 < n && (chars[*i + 1] == ':' || chars[*i + 1] == '.' || chars[*i + 1] == '=') {
            // POSIX class / collating element / equivalence class
            cls.push('[');
            cls.push(chars[*i + 1]);
            *i += 2;
            loop {
                if *i >= n {
                    return None;
                }
                let e = chars[*i];
                cls.push(e);
                *i += 1;
                if e == ']' {
                    break;
                }
            }
        } else {
            cls.push(d);
            *i += 1;
        }
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::AssignTarget;

    fn st(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }
    fn exec_grep(argv: Vec<IrExpr>) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![st("grep"), IrExpr::Array(argv)],
        })
    }
    fn getvar(v: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![st(v)],
        }
    }
    /// Human-readable arg (strings verbatim; getVar reads as `$NAME`).
    fn arg_repr(a: &IrExpr) -> String {
        match a {
            IrExpr::Str(s, _) => s.clone(),
            IrExpr::Call { func, args } if func == "getVar" => match args.get(0) {
                Some(IrExpr::Str(v, _)) => format!("${}", v),
                _ => format!("{:?}", a),
            },
            _ => format!("{:?}", a),
        }
    }
    /// argv (the Array) of the first Expr(exec grep) found in stmts.
    fn argv_of(stmts: &[IrStmt]) -> Vec<String> {
        fn walk(st: &IrStmt, out: &mut Vec<String>) {
            match st {
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" || func == "builtin" => {
                    if let [IrExpr::Str(name, _), IrExpr::Array(argv)] = args.as_slice() {
                        if name == "grep" {
                            for a in argv {
                                out.push(arg_repr(a));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for s in stmts {
            walk(s, &mut out);
        }
        out
    }
    /// Collect ALL exec-grep argv lists (per stage), including nested.
    fn all_argv(stmts: &[IrStmt]) -> Vec<Vec<String>> {
        fn walk(e: &IrExpr, out: &mut Vec<Vec<String>>) {
            match e {
                IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
                    if let [IrExpr::Str(name, _), IrExpr::Array(argv)] = args.as_slice() {
                        if name == "grep" || name == "sed" {
                            out.push(argv.iter().map(arg_repr).collect());
                        }
                    }
                }
                IrExpr::Call { args, .. } | IrExpr::Array(args) => {
                    for a in args {
                        walk(a, out);
                    }
                }
                IrExpr::Capture { expr, .. } => walk(expr, out),
                IrExpr::Arrow(stmts) => {
                    for st in stmts {
                        walk_all(st, out);
                    }
                }
                _ => {}
            }
        }
        fn walk_all(st: &IrStmt, out: &mut Vec<Vec<String>>) {
            match st {
                IrStmt::Expr(e) => walk(e, out),
                IrStmt::Assign { expr, .. } => walk(expr, out),
                IrStmt::Declare { init: Some(init), .. } => walk(init, out),
                IrStmt::Block(b)
                | IrStmt::Subshell(b)
                | IrStmt::Background(b) => {
                    for s in b {
                        walk_all(s, out);
                    }
                }
                IrStmt::If {
                    cond,
                    then,
                    elsifs,
                    else_,
                } => {
                    walk(cond, out);
                    for s in then {
                        walk_all(s, out);
                    }
                    for (c, b) in elsifs {
                        walk(c, out);
                        for s in b {
                            walk_all(s, out);
                        }
                    }
                    for s in else_ {
                        walk_all(s, out);
                    }
                }
                IrStmt::While { cond, body } => {
                    walk(cond, out);
                    for s in body {
                        walk_all(s, out);
                    }
                }
                IrStmt::DoWhile { cond, body, .. } => {
                    walk(cond, out);
                    for s in body {
                        walk_all(s, out);
                    }
                }
                IrStmt::For { iter, body, .. } => {
                    walk(iter, out);
                    for s in body {
                        walk_all(s, out);
                    }
                }
                IrStmt::Redirect { inner, redirects } => {
                    for s in inner {
                        walk_all(s, out);
                    }
                    for r in redirects {
                        walk(&r.target, out);
                    }
                }
                IrStmt::Pipeline {
                    stages, capture: _, ..
                } => {
                    for stage in stages {
                        for s in stage {
                            walk_all(s, out);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for s in stmts {
            walk_all(s, &mut out);
        }
        out
    }

    // ── pattern translation ──────────────────────────────────────────

    #[test]
    fn noncapture_and_whitespace() {
        let (ere, pre) = translate_pcre("(?:[0-9]+\\s+Doing)").unwrap();
        assert_eq!(ere, "([0-9]+[[:space:]]+Doing)");
        assert!(pre.is_none());
    }

    #[test]
    fn atomic_group_same_language() {
        let (ere, _) = translate_pcre("(?>(?:ab)+)c").unwrap();
        assert_eq!(ere, "((ab)+)c");
    }

    #[test]
    fn possessive_quantifier_stripped() {
        let (ere, _) = translate_pcre("a*+b").unwrap();
        assert_eq!(ere, "a*b");
        let (ere, _) = translate_pcre("a{2,3}+").unwrap();
        assert_eq!(ere, "a{2,3}");
    }

    #[test]
    fn escape_classes_to_posix() {
        let (ere, _) = translate_pcre("\\d+\\s*\\w\\D\\S\\W").unwrap();
        assert_eq!(
            ere,
            "[0-9]+[[:space:]]*[[:alnum:]_][^0-9][^[:space:]][^[:alnum:]_]"
        );
    }

    #[test]
    fn leading_fixed_lookbehind_absorbed() {
        let (ere, pre) = translate_pcre("(?<=dev )(\\S+)").unwrap();
        assert_eq!(ere, "dev ([^[:space:]]+)");
        assert_eq!(pre.as_deref(), Some("dev "));
    }

    #[test]
    fn lookbehind_nonliteral_refused() {
        assert!(translate_pcre("(?<=\\d)x").is_none());
        assert!(translate_pcre("(?<=a+)x").is_none());
        assert!(translate_pcre("(?<=[ab])x").is_none());
    }

    #[test]
    fn lookaheads_refused() {
        assert!(translate_pcre("(?=a)b").is_none());
        assert!(translate_pcre("a(?!b)c").is_none());
        assert!(translate_pcre("(?<!a)b").is_none());
        assert!(translate_pcre("a(?<=b)c").is_none());
    }

    #[test]
    fn refused_constructs() {
        assert!(translate_pcre("\\bword\\b").is_none());
        assert!(translate_pcre("(a)\\1").is_none());
        assert!(translate_pcre("a.*?b").is_none());
        assert!(translate_pcre("\\x41").is_none());
        assert!(translate_pcre("\\t").is_none());
    }

    #[test]
    fn class_copy_handles_posix_and_first_char() {
        let (ere, _) = translate_pcre("[[:alpha:]-]+").unwrap();
        assert_eq!(ere, "[[:alpha:]-]+");
        let (ere, _) = translate_pcre("[]a]").unwrap();
        assert_eq!(ere, "[]a]");
    }

    // ── the full transform ───────────────────────────────────────────

    #[test]
    fn snap_style_o_rewrite() {
        let mut stmts = vec![exec_grep(vec![
            st("-P"),
            st("-o"),
            st("(?:[0-9]+\\s+Doing)"),
        ])];
        assert!(transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-Eo", "([0-9]+[[:space:]]+Doing)"]);
    }

    #[test]
    fn plain_filter_lookbehind_absorbed_no_strip() {
        let mut stmts = vec![exec_grep(vec![st("-P"), st("(?<=dev )eth0")])];
        assert!(transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-E", "dev eth0"]);
    }

    #[test]
    fn variable_pattern_reduced_to_static() {
        // PAT='[0-9]+\s+Doing' then grep -P "$PAT" file
        let mut stmts = vec![
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "PAT".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: st("[0-9]+\\s+Doing"),
                asm: None,
            },
            exec_grep(vec![st("-P"), getvar("PAT"), st("file.txt")]),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(
            argv_of(&stmts),
            vec!["-E", "[0-9]+[[:space:]]+Doing", "file.txt"]
        );
    }

    #[test]
    fn variable_pattern_unresolved_refused() {
        // no assignment → $PAT stays dynamic → grep -P kept
        let mut stmts = vec![exec_grep(vec![st("-P"), getvar("PAT"), st("f")])];
        assert!(!transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-P", "$PAT", "f"]);
    }

    #[test]
    fn dynamic_reassignment_invalidates() {
        // PAT assigned statically, then reassigned dynamically → refused
        let mut stmts = vec![
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "PAT".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: st("^x$"),
                asm: None,
            },
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "PAT".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: getvar("1"),
                asm: None,
            },
            exec_grep(vec![st("-P"), getvar("PAT")]),
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn non_p_grep_untouched() {
        let mut stmts = vec![exec_grep(vec![st("-E"), st("foo[0-9]+")])];
        assert!(!transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-E", "foo[0-9]+"]);
    }

    #[test]
    fn unknown_flag_refused() {
        let mut stmts = vec![exec_grep(vec![st("-P"), st("-z"), st("x")])];
        assert!(!transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-P", "-z", "x"]);
    }

    #[test]
    fn combined_short_flags() {
        let mut stmts = vec![exec_grep(vec![st("-Po"), st("(?:x\\s+y)")])];
        assert!(transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-Eo", "(x[[:space:]]+y)"]);
    }

    // ── capture + -o lookbehind → sed strip stage ────────────────────

    fn capture_pipeline(grep_argv: Vec<IrExpr>) -> IrStmt {
        // A=$(echo hi | grep <argv>)
        let pipeline = IrExpr::Call {
            func: "pipeline".to_string(),
            args: vec![IrExpr::Array(vec![
                IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![
                        st("echo"),
                        IrExpr::Array(vec![st("default via 10.0.0.1 dev eth0")]),
                    ],
                })]),
                IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![st("grep"), IrExpr::Array(grep_argv)],
                })]),
            ])],
        };
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "A".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Capture {
                expr: Box::new(pipeline),
                native: false,
            },
            asm: None,
        }
    }

    #[test]
    fn capture_o_lookbehind_gets_sed_stage() {
        let mut stmts = vec![capture_pipeline(vec![
            st("-P"),
            st("-o"),
            st("(?<=dev )(\\S+)"),
        ])];
        assert!(transform(&mut stmts));
        let av = all_argv(&stmts);
        assert_eq!(av.len(), 2, "grep + sed stages: {:?}", av);
        assert_eq!(av[0], vec!["-Eo", "dev ([^[:space:]]+)"]);
        assert_eq!(av[1], vec!["s/^dev //"]);
    }

    #[test]
    fn bare_o_lookbehind_refused_outside_capture() {
        // not in a capture → the sed strip can't be placed → grep -P kept
        let mut stmts = vec![exec_grep(vec![
            st("-P"),
            st("-o"),
            st("(?<=dev )(\\S+)"),
        ])];
        assert!(!transform(&mut stmts));
        assert_eq!(argv_of(&stmts), vec!["-P", "-o", "(?<=dev )(\\S+)"]);
    }

    #[test]
    fn direct_capture_wrapping_grep_gets_wrapped() {
        // A=$(grep -Po '(?<=dev )(\S+)' file)
        let inner = IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                st("grep"),
                IrExpr::Array(vec![
                    st("-P"),
                    st("-o"),
                    st("(?<=dev )(\\S+)"),
                    st("f"),
                ]),
            ],
        };
        let mut stmts = vec![IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "A".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Capture {
                expr: Box::new(inner),
                native: false,
            },
            asm: None,
        }];
        assert!(transform(&mut stmts));
        let av = all_argv(&stmts);
        assert_eq!(av.len(), 2);
        assert_eq!(av[0], vec!["-Eo", "dev ([^[:space:]]+)", "f"]);
        assert_eq!(av[1], vec!["s/^dev //"]);
    }
}
