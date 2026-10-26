//! default-empty-fold — `${v:-}` (an EMPTY default) IS `${v}`: retire the
//! per-backend "value or default" conditional entirely.
//!
//! name: default-empty-fold
//! prereqs: [none — a pure local substitution of an already-lowered node]
//! invariant: param(":-"/"-", NAME, "") folds to the plain read
//!   param("", NAME) ONLY when (a) NAME is a plain identifier and (b) the
//!   program provably never enables nounset. Everything else refuses.
//! scope: offered to all backends (estree, c, go, java, perl, python,
//!   rust, sh, fish, zig, zsh, glsl)
//! updates: none (first offer)
//!
//! ## Need
//! `${v:-}` with an EMPTY default is the identity: bash returns `v` when
//! v is set-and-non-null, and the default (which is the empty string)
//! when it is not — exactly what a plain `$v` already yields. The parser
//! nonetheless emits the full three-argument `param(":-", NAME, "")`
//! form, and then EVERY backend re-derives — at every render — that the
//! default arm can never differ from the value:
//!
//!   * Rust `{ let __v = u; if __v.is_empty() { "".to_string() } else { __v.to_string() } }`
//!     — a conditional block that allocates on both arms to return `u`;
//!     on an int-homed array (`arr=(1 2 3); echo "${arr:-}"`) it does not
//!     even COMPILE (`can't call method is_empty on ambiguous numeric
//!     type`), while the plain `${arr}` renders and runs fine;
//!   * Perl `((defined($u) && length($u)) ? $u : "")`
//!   * JS    `String(u) !== "" ? String(u) : ""`
//!   * C     `((u)[0]) ? (u) : ("")`
//!   * Go    `pExp(":-", "u", "")`
//!
//! Whether an empty default is a no-op is a PROGRAM property, not a
//! backend detail — so the rewrite belongs in the shared shIR→shIR
//! channel: one fold retires the conditional (and, on rust, fixes the
//! compile) for every backend at once, which is exactly the marketplace
//! §11 shape — and it feeds later transforms (copy-propagation sees a
//! plain read).
//!
//! Measured on the 562-example corpus: 36 sites in 30 files.
//!
//! ## Scope — the sound rewrite
//!   * op `:-` or `-` (the parser normalises both to `:-`)
//!   * third argument an EMPTY literal
//!   * NAME a plain identifier `[A-Za-z_][A-Za-z0-9_]*`
//!
//! ### Why the plain-identifier guard matters
//! The `[@]`/`[*]` spellings keep their subscript IN the name
//! (`param(":-", "arr[@]", "")`), and there they are NOT the identity:
//! `arr=(); f "${arr[@]:-}"` passes ONE empty word while
//! `f "${arr[@]}"` passes ZERO — so an empty array's arity differs even
//! though the printed text matches. Positional (`$1`), indirect (`!p`)
//! and `#n` spellings are refused for the same "the slice/expansion form
//! then means something different from the plain read" reason that
//! `slice_identity` refuses them. REFUSE > GUESS.
//!
//! ### Why the nounset gate matters
//! Under `set -u`, `${v:-}` on an unset `v` yields the empty string but
//! `${v}` is an "unbound variable" error — the ONE context where the two
//! differ. `sh`/`zsh` pass `set -u` through verbatim to a real shell that
//! enforces it, so the fold must not fire when the program may enable
//! nounset. (estree tracks the flag without enforcing it; rust never
//! implements it; the gate is the conservative answer that is right for
//! EVERY target.) No corpus file both sets `-u` and uses `${v:-}`, so
//! the gate costs zero coverage.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS-gated, default ON —
//! empty = all). A pure local substitution of an already-lowered node:
//! no analysis counts on seeing `param(":-", …)` afterwards — every
//! consumer of the node is a renderer arm that already handles the
//! plain-read shape it becomes.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
/// Signature matches `transforms::TransformFn` (`fn(&mut Vec<IrStmt>)`).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // The nounset gate is program-wide: bail out BEFORE touching anything
    // so a program that may enable `set -u` keeps every `${v:-}` intact.
    if may_enable_nounset(stmts) {
        return false;
    }
    // Array-homed names are a per-NAME gate (see `collect_array_names`).
    ARRAY_HOMED.with(|a| {
        a.borrow_mut().clear();
        collect_array_names(stmts, &mut a.borrow_mut());
    });
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    ARRAY_HOMED.with(|a| a.borrow_mut().clear());
    c
}

/// The names this program declares as ARRAYS. `${arr:-}` must NOT fold for
/// them: the identity holds element-wise in bash (`${arr:-}` and `${arr}`
/// are both ELEMENT 0), but the plain read a fold produces renders as the
/// WHOLE LIST on some backends — fish printed `[item1 item2]` for
/// `LIST=(item1 item2); echo "${LIST:-}"` where bash prints `[item1]`.
/// The per-backend "value or default" guard is what kept the subscript,
/// so folding it away traded a redundant conditional for a wrong value.
/// Mirrors `c_backend::collect_array_names`; conservative in the safe
/// direction — an unrecognised declaring form just leaves the name in the
/// set (kept, never folded).
fn collect_array_names(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    // `arr[i]=x` — an indexed write is an array declaration.
                    if !t.indices.is_empty() {
                        out.insert(t.var.clone());
                    }
                    // the core also flattens the index into the name
                    if let Some(open) = t.var.find('[') {
                        if t.var.ends_with(']') {
                            out.insert(t.var[..open].to_string());
                        }
                    }
                    if let IrExpr::Call { func, .. } = expr {
                        if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                            out.insert(t.var.clone());
                        }
                    }
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                out.insert(var.clone());
            }
            _ => {}
        }
    }
    // Nested statement lists carry their own declarations, and a function
    // may declare the array its caller reads — walk everything.
    for s in stmts {
        let nested: Vec<&Vec<IrStmt>> = match s {
            IrStmt::If { then, elsifs, else_, .. } => {
                let mut v: Vec<&Vec<IrStmt>> = vec![then, else_];
                v.extend(elsifs.iter().map(|(_, b)| b));
                v
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => vec![body],
            IrStmt::For { body, .. } => vec![body],
            IrStmt::ForInit {
                init,
                step,
                body,
                ..
            } => vec![init, step, body],
            IrStmt::Function { body, named_blocks, .. } => {
                let mut v: Vec<&Vec<IrStmt>> = vec![body];
                v.extend(named_blocks.iter().map(|(_, b)| b));
                v
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => vec![b],
            IrStmt::Redirect { inner, .. } => vec![inner],
            IrStmt::Pipeline { stages, .. } => stages.iter().collect(),
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                let mut v: Vec<&Vec<IrStmt>> = vec![body, else_body, finally_body];
                v.extend(excepts.iter().map(|e| &e.body));
                v
            }
            IrStmt::Case { clauses, .. } => clauses.iter().map(|c| &c.body).collect(),
            _ => vec![],
        };
        for b in nested {
            collect_array_names(b, out);
        }
    }
}

thread_local! {
    /// Names to KEEP (never fold): this program declares them as arrays.
    static ARRAY_HOMED: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

// ── nounset gate ─────────────────────────────────────────────────────

/// Does the program enable `set -u` (nounset) anywhere reachable?
///
/// Conservative, mirroring `shir::ir_may_enable_errexit`: a literal
/// `set` call whose flag cluster carries `u` under a `-` prefix (or
/// `-o nounset`) returns true; a dynamic command name or a non-literal
/// flag word returns true ("may be `set -u` at runtime"). `set +u`
/// disables, and the LAST toggle wins, so the scan tracks the flag's
/// running value rather than just its presence.
fn may_enable_nounset(stmts: &[IrStmt]) -> bool {
    /// A literal set-builtin argument list → is nounset ON at the end?
    fn set_call_enables(args: &[IrExpr]) -> bool {
        let mut lits: Vec<&str> = Vec::with_capacity(args.len());
        for a in args {
            match a {
                IrExpr::Str(s, _) => lits.push(s.as_str()),
                IrExpr::Interpolate(parts)
                    if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) =>
                {
                    let mut s = String::new();
                    for p in parts {
                        if let InterpPart::Lit(t) = p {
                            s.push_str(t);
                        }
                    }
                    // leaked-free: keep the joined text alive for the scan
                    lits.push(s.leak());
                }
                _ => return true, // dynamic flag word: may be `-u` at runtime
            }
        }
        match lits.first() {
            None | Some(&"--") => return false,
            Some(a) if !a.starts_with('-') && !a.starts_with('+') => {
                return false; // positional assignment, no flags
            }
            _ => {}
        }
        let mut nounset = false;
        let mut pending_o = false; // `-o`/`+o` seen: next arg is a long option name
        let mut o_enable = false;
        for a in lits {
            if pending_o {
                if a == "nounset" {
                    nounset = o_enable;
                }
                pending_o = false;
                continue;
            }
            if !a.starts_with('-') && !a.starts_with('+') {
                continue; // the runtime skips non-flag args in a flag list
            }
            let enable = a.starts_with('-');
            for c in a[1..].chars() {
                if c == 'u' {
                    nounset = enable;
                }
                if c == 'o' {
                    pending_o = true;
                    o_enable = enable;
                }
            }
        }
        nounset
    }

    fn scan_expr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                if func == "exec" || func == "builtin" {
                    match args.first() {
                        Some(IrExpr::Str(name, _)) if name == "set" => {
                            let rest = &args[1..];
                            // `set` with a single Array arg carries the words
                            // there; otherwise the words follow directly.
                            let words: &[IrExpr] = match rest {
                                [IrExpr::Array(elems)] => elems.as_slice(),
                                other => other,
                            };
                            if set_call_enables(words) {
                                return true;
                            }
                        }
                        Some(IrExpr::Str(_, _)) => { /* other literal command */ }
                        _ => return true, // dynamic command name: may be `set`
                    }
                }
                args.iter().any(scan_expr)
            }
            IrExpr::Arrow(stmts) => scan_stmts(stmts),
            IrExpr::Array(elems) => elems.iter().any(scan_expr),
            IrExpr::Object(props) => props.iter().any(|(_, v)| scan_expr(v)),
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(e) => scan_expr(e),
                InterpPart::Lit(_) => false,
            }),
            IrExpr::Capture { expr, .. } => scan_expr(expr),
            IrExpr::Index { key, .. } => scan_expr(key),
            IrExpr::BinOp { lhs, rhs, .. } => scan_expr(lhs) || scan_expr(rhs),
            IrExpr::MethodCall { obj, args, .. } => {
                scan_expr(obj) || args.iter().any(scan_expr)
            }
            IrExpr::Ternary { cond, then, else_ } => {
                scan_expr(cond) || scan_expr(then) || scan_expr(else_)
            }
            IrExpr::DefinedOr { expr, default } => scan_expr(expr) || scan_expr(default),
            IrExpr::Arith(a) => scan_arith(a),
            _ => false,
        }
    }

    fn scan_arith(a: &ArithAst) -> bool {
        match a {
            ArithAst::Bin { lhs, rhs, .. } => scan_arith(lhs) || scan_arith(rhs),
            ArithAst::Un { arg, .. } => scan_arith(arg),
            ArithAst::Cond { test, then, else_ } => {
                scan_arith(test) || scan_arith(then) || scan_arith(else_)
            }
            ArithAst::Index { key, .. } => scan_arith(key),
            _ => false,
        }
    }

    fn scan_stmts(v: &[IrStmt]) -> bool {
        v.iter().any(scan_stmt)
    }

    fn scan_stmt(s: &IrStmt) -> bool {
        match s {
            IrStmt::Ext(n) => crate::shir_nodes::ExtNode::children(&**n)
                .into_iter()
                .any(scan_stmt),
            IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => scan_expr(e),
            IrStmt::WriteFile { path, content, .. } => {
                scan_expr(path) || scan_expr(content)
            }
            IrStmt::Assign { targets, expr, .. } => {
                scan_expr(expr) || targets.iter().any(|t| t.indices.iter().any(scan_expr))
            }
            IrStmt::Declare { init, .. } => init.as_ref().is_some_and(scan_expr),
            IrStmt::DeclareArray { elements, .. } => elements.iter().any(scan_expr),
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => scan_expr(e),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                scan_expr(cond)
                    || scan_stmts(then)
                    || elsifs.iter().any(|(c, b)| scan_expr(c) || scan_stmts(b))
                    || scan_stmts(else_)
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                scan_expr(cond) || scan_stmts(body)
            }
            IrStmt::For { iter, body, .. } => scan_expr(iter) || scan_stmts(body),
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                scan_stmts(init)
                    || scan_expr(cond)
                    || scan_stmts(step)
                    || scan_stmts(body)
            }
            IrStmt::Function { body, named_blocks, .. } => {
                scan_stmts(body)
                    || named_blocks.iter().any(|(_, b)| scan_stmts(b))
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => scan_stmts(b),
            IrStmt::Redirect { inner, .. } => scan_stmts(inner),
            IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| scan_stmts(s)),
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                scan_stmts(body)
                    || excepts.iter().any(|e| scan_stmts(&e.body))
                    || scan_stmts(else_body)
                    || scan_stmts(finally_body)
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                scan_expr(discriminant) || clauses.iter().any(|c| scan_stmts(&c.body))
            }
            _ => false,
        }
    }

    scan_stmts(stmts)
}

// ── the fold ─────────────────────────────────────────────────────────

fn stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr: rhs, .. } => expr(rhs),
        IrStmt::Declare { init: Some(e), .. } => expr(e),
        IrStmt::Expr(e) => expr(e),
        IrStmt::Output { value, .. } => expr(value),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => expr(e),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
            ..
        } => {
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
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut c = transform(init);
            c |= expr(cond);
            c |= transform(step);
            c | transform(body)
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
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
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = transform(body);
            for e in excepts.iter_mut() {
                c |= transform(&mut e.body);
            }
            c |= transform(else_body);
            c |= transform(finally_body);
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
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
        IrStmt::Ext(n) => {
            // A transform-declared statement node: reach its children so
            // the fold applies inside it too (structural traversal).
            let mut c = false;
            for child in n.children_mut() {
                c |= stmt(child);
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
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
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
/// reference — anything else (`arr[@]`, `arr[*]`, `!p`, `#n`, `@`, `*`,
/// `1`, a dotted/bracketed name, an interpolated string) keeps its
/// default-expansion spelling, because the list/positional/indirect form
/// then means something different from the plain read.
fn is_plain_ident(n: &str) -> bool {
    let mut cs = n.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Fold one node: `param(":-", NAME, "")` → `param("", NAME)` when NAME
/// is a plain identifier and the default is the empty literal.
fn fold(e: &mut IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if func != "param" {
        return false;
    }
    // Exactly (op, name, default). A 4th arg is a form this fold has no
    // story for — refuse-on-doubt rather than reason about it here.
    if args.len() != 3 {
        return false;
    }
    let (IrExpr::Str(op, op_style), IrExpr::Str(name, name_style)) = (&args[0], &args[1]) else {
        return false;
    };
    if op != ":-" && op != "-" {
        return false;
    }
    if !is_plain_ident(name) {
        return false;
    }
    // An array-homed name keeps its per-backend default GUARD: the plain
    // read a fold would leave behind is element 0 in bash but the whole
    // list on some backends (fish), so the conditional is load-bearing.
    if ARRAY_HOMED.with(|a| a.borrow().contains(name)) {
        return false;
    }
    // default must be the EMPTY literal — any real default is a real
    // substitution and is never folded.
    match &args[2] {
        IrExpr::Str(d, _) if d.is_empty() => {}
        _ => return false,
    }
    // Rebuild as the parser's plain-read shape (styles preserved so the
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

    fn op(name: &str, default: &str) -> IrExpr {
        IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(":-"), s(name), s(default)],
        }
    }

    fn plain_read(name: &str) -> IrExpr {
        IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(""), s(name)],
        }
    }

    fn fold_ok(e: &mut IrExpr) -> bool {
        fold(e)
    }

    #[test]
    fn empty_default_folds_to_plain_read() {
        for name in ["v", "u", "IFS", "_x9", "HOME", "p"] {
            let mut e = op(name, "");
            assert!(fold_ok(&mut e), "{name}: must fold");
            assert_eq!(e, plain_read(name), "{name}: becomes the plain read");
        }
    }

    #[test]
    fn non_empty_default_never_folds() {
        for d in ["default", "0", " ", "$(pwd)", "${OTHER}"] {
            let mut e = op("v", d);
            assert!(!fold_ok(&mut e), "default {d:?} must be kept");
        }
    }

    #[test]
    fn list_and_positional_spellings_refuse() {
        // `[@]`/`[*]` keep the subscript in the name and are NOT the
        // identity: an empty array's WORD ARITY differs (`arr[@]:-` yields
        // one empty word, `arr[@]` yields zero). A single letter is a
        // plain parameter — `p` folds like `v` — so it belongs above.
        for name in ["arr[@]", "arr[*]", "@", "*", "1", "!", "x.y", "a[0]"] {
            let mut e = op(name, "");
            assert!(!fold_ok(&mut e), "name {name:?} must refuse");
        }
    }

    /// An array-homed name keeps its guard: the plain read a fold leaves
    /// behind is ELEMENT 0 in bash but the WHOLE LIST on fish, so the
    /// per-backend conditional is load-bearing (the fish backend's
    /// `array_default_subject_keeps_its_subscript` caught it —
    /// `LIST=(item1 item2)` printed `[item1 item2]` where bash prints
    /// `[item1]`).
    #[test]
    fn array_homed_names_refuse() {
        let arr_read = |name: &str| IrStmt::Output {
            value: op(name, ""),
            newline: true,
            target: None,
        };
        // declared via the `LIST=()` / `LIST+=( … )` spelling
        let mut via_declare = vec![
            IrStmt::DeclareArray {
                var: "LIST".to_string(),
                sigil: None,
                elements: vec![],
            },
            arr_read("LIST"),
        ];
        assert!(
            !transform(&mut via_declare),
            "a DeclareArray's name must keep its guard: {via_declare:?}"
        );
        assert_eq!(via_declare[1], arr_read("LIST"), "shape untouched");

        // declared via the `setArray` call the parser lowers it to
        let mut via_set_array = vec![
            IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: "LIST".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Call {
                    func: "setArray".to_string(),
                    args: vec![s("LIST"), IrExpr::Array(vec![])],
                },
                asm: None,
            },
            arr_read("LIST"),
        ];
        assert!(
            !transform(&mut via_set_array),
            "a setArray's name must keep its guard: {via_set_array:?}"
        );

        // …and a SCALAR declared alongside it still folds
        let mut mixed = vec![
            IrStmt::DeclareArray {
                var: "LIST".to_string(),
                sigil: None,
                elements: vec![],
            },
            arr_read("LIST"),
            IrStmt::Output {
                value: op("v", ""),
                newline: true,
                target: None,
            },
        ];
        assert!(
            transform(&mut mixed),
            "the scalar in the same program still folds: {mixed:?}"
        );
        assert_eq!(mixed[1], arr_read("LIST"), "the array read is kept");
        assert_eq!(
            mixed[2],
            IrStmt::Output {
                value: plain_read("v"),
                newline: true,
                target: None,
            },
            "the scalar folded"
        );
    }

    #[test]
    fn wrong_arity_and_shape_refuse() {
        // 2-arg (no default slot) and 4-arg forms are not this fold's shape
        let mut two = IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(":-"), s("v")],
        };
        assert!(!fold_ok(&mut two));

        let mut four = IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(":-"), s("v"), s(""), s("")],
        };
        assert!(!fold_ok(&mut four));

        // a non-param call is untouched
        let mut other = IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![s(":-"), s("v"), s("")],
        };
        assert!(!fold_ok(&mut other));
    }

    #[test]
    fn op_minus_is_normalised_to_the_same_fold() {
        // the parser normalises `${v-}` to the `:-` op; the fold accepts
        // the un-normalised spelling too, with the same guard.
        let mut e = IrExpr::Call {
            func: "param".to_string(),
            args: vec![s("-"), s("v"), s("")],
        };
        assert!(fold_ok(&mut e));
        assert_eq!(e, plain_read("v"));
    }

    // ── nounset gate ────────────────────────────────────────────────

    fn echo_param(e: IrExpr) -> IrStmt {
        IrStmt::Expr(e)
    }

    #[test]
    fn nounset_gate_blocks_the_whole_fold() {
        let set_u = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![s("set"), IrExpr::Array(vec![s("-u")])],
        });
        let mut stmts = vec![set_u, echo_param(op("v", ""))];
        assert!(may_enable_nounset(&stmts), "`set -u` must be detected");
        assert!(!transform(&mut stmts), "fold must be suppressed");
        assert_eq!(stmts[1], echo_param(op("v", "")), "`${{v:-}}` kept");
    }

    #[test]
    fn errexit_alone_does_not_block_the_fold() {
        let set_e = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![s("set"), IrExpr::Array(vec![s("-e")])],
        });
        let mut stmts = vec![set_e, echo_param(op("v", ""))];
        assert!(!may_enable_nounset(&stmts), "`-e` is not nounset");
        assert!(transform(&mut stmts), "fold proceeds");
        assert_eq!(stmts[1], echo_param(plain_read("v")));
    }

    #[test]
    fn nounset_via_long_option_and_cluster_is_detected() {
        for flag in [s("-u"), s("-euo"), s("-o"), s("nounset")] {
            let _ = flag;
        }
        // `-o nounset`
        let mut long = vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                s("set"),
                IrExpr::Array(vec![s("-o"), s("nounset")]),
            ],
        })];
        assert!(may_enable_nounset(&long), "-o nounset");

        // `-euo` carries the `u` letter
        let mut cluster = vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![s("set"), IrExpr::Array(vec![s("-euo"), s("pipefail")])],
        })];
        assert!(may_enable_nounset(&cluster), "-euo carries u");

        // a DYNAMIC set argument is conservatively "may be -u"
        let mut dyn_arg = vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![s("set"), IrExpr::Var("flags".to_string(), None)],
        })];
        assert!(may_enable_nounset(&dyn_arg), "dynamic flag word");

        // a dynamic COMMAND name may be `set`
        let mut dyn_cmd = vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![IrExpr::Var("cmd".to_string(), None), IrExpr::Array(vec![s("-u")])],
        })];
        assert!(may_enable_nounset(&dyn_cmd), "dynamic command name");
    }

    #[test]
    fn set_plus_u_does_not_count_as_enabling() {
        let mut stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![s("set"), IrExpr::Array(vec![s("+u")])],
        })];
        assert!(!may_enable_nounset(&stmts), "`set +u` disables nounset");
    }

    #[test]
    fn program_without_set_is_not_gated() {
        let mut stmts = vec![
            IrStmt::Expr(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![s("echo"), IrExpr::Array(vec![s("hi")])],
            }),
            echo_param(op("v", "")),
        ];
        assert!(!may_enable_nounset(&stmts));
        assert!(transform(&mut stmts));
        assert_eq!(stmts[1], echo_param(plain_read("v")));
    }

    #[test]
    fn the_identity_holds_in_bash_for_the_folded_cases() {
        use std::process::Command;
        // The three observable cases for a PLAIN name: unset, empty, set.
        let script = "v=\"\"; a=\"${v:-}\"; unset v; b=\"${v:-}\"; v=x; c=\"${v:-}\"\n\
                      v=\"\"; a2=\"${v}\"; unset v; b2=\"${v}\"; v=x; c2=\"${v}\"\n\
                      printf '%s|%s|%s|%s|%s|%s\\n' \"$a\" \"$b\" \"$c\" \"$a2\" \"$b2\" \"$c2\"\n";
        let out = Command::new("bash").arg("-c").arg(script).output().expect("bash");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // all six must be the empty/empty/x triple: `${v:-}` ≡ `${v}`
        assert_eq!(stdout, "||x|||x\n", "bash says the fold is the identity");
    }
}
