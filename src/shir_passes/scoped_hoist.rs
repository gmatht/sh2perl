//! ScopedHoist — hoist a MAIN-scope shIR local that every renderer must
//! otherwise home in global storage (a `thread_local!` cell in Rust, a
//! module static / `std::env::var` read elsewhere).
//!
//! The rust renderer already has a complete "pass-2 local" verdict
//! (`compute_local_vars`): a scalar confined to ONE scope, never captured,
//! never a dynamic name, first written at the top level, read at least
//! once. `list_profile::use_scopes`, the analysis that FEEDS it, records a
//! mention inside a conditional's TEST as `top: false` (the test is a
//! nested statement). A name assigned at the top level and then read only
//! from `if <test>` conditions therefore never satisfies "first mention is
//! a top-level write" and falls all the way back to global storage:
//!
//! ```text
//! s = "hello world"        thread_local! { static __SHV_s: RefCell<String> = … }
//! if <test reads s> …      __SHV_s.with(|v| *v.borrow_mut() = "hello world".to_string());
//!                          let __t0 = if __SHV_s.with(|v| …).contains(…) { … };
//! ```
//!
//! (py-sh-go `t60_contains`: five thread-local round trips for two
//! lines of python, and a `.clone()` per read.)
//!
//! This pass removes the ambiguity before any backend's scope analysis
//! runs: the main-scope top-level assignment becomes the explicit
//! DECLARATION node (`Declare`), the one form whose `use_scopes` record is
//! `write + decl`, which owns its binding at ANY nesting depth
//! (`first_is_decl`). The renderer then emits the hoisted `let mut s = …`
//! exactly where the assignment sat — the same shape it already emits for
//! a shell `local x=…`.
//!
//! # Scope of the rewrite (deliberately narrow)
//!
//! Only `prog.stmts` (the main body) is considered, and only when
//!
//! * the name is a plain identifier (no positional/special/array spelling),
//! * it is assigned exactly ONCE in the whole program — that top-level
//!   assignment, and no second write anywhere (nested bodies, subs,
//!   `setVar`, array stores), no `Declare` of it, no loop binding, and
//! * its value is a pure literal or a read of another name (`Str`/`Int`/
//!   `Bool`, an all-literal `Interpolate`, a `Var`): a value whose
//!   evaluation cannot be observed to move,
//! * the program contains no `eval`/`source`/`.` (they can write any name),
//! * the FIRST mention of the name is that assignment (a read before it
//!   sees the shell's unset empty string, which the global home models and
//!   a hoisted `let` would not), and
//! * at least one read sits in a conditional TEST — the shape this pass
//!   exists for. Everything else is left exactly as it was, so no program
//!   that lowers cleanly today changes shape.
//!
//! The rewrite keeps the assignment at its own position (the `Declare`
//! renders where the `Assign` was), so nothing can observe the binding
//! earlier than before and no read of an uninitialised `let` can appear.
//!
//! # Where it runs
//!
//! Registered in `otranspilerl`'s A1 ingress (`ingress_normalize`), which
//! is the pipeline that feeds a frontend's already-lowered shIR to the
//! renderers of every frontend target — bash-sourced programs never reach
//! this code path (they go through `ast_to_ir`), so their output is
//! untouched byte-for-byte.

use crate::ir::{AssignTarget, Decl, InterpPart, IrExpr, IrProgram, IrStmt};

use super::{PassContext, Transform};

pub struct ScopedHoist;

/// Is this expression a pure value — one whose evaluation cannot be
/// observed to move? Literals, reads of other names, a quoted string whose
/// every part is literal.
fn pure_value(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Bool(_) => true,
        IrExpr::Var(_, _) | IrExpr::Ident(_) => true,
        IrExpr::Interpolate(parts) => parts
            .iter()
            .all(|p| matches!(p, InterpPart::Lit(_))),
        _ => false,
    }
}

fn ident_word(w: &str) -> bool {
    let mut cs = w.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A name every renderer treats as a plain scalar shell variable.
fn plain_name(n: &str) -> bool {
    ident_word(n) && !n.starts_with("__") && !n.chars().next().unwrap().is_ascii_uppercase()
}

/// What this pass needs to know about one name in the whole program.
#[derive(Default, Debug)]
struct Census {
    /// `Assign` statement sites (top level or nested).
    assign_sites: usize,
    /// The assign sits at the MAIN body's top level, and is the whole
    /// statement (`s = …`, one target, no index).
    top_plain: bool,
    /// Assign sites that are not `top_plain` (nested, indexed, multi-target).
    impure_sites: usize,
    /// Sites that write the name in a form other than `IrStmt::Assign`
    /// (`setVar`, array stores, builtin writes, `Declare`).
    other_writes: usize,
    /// The name is a `Declare`'s binding (a `local`/`declare` owns it).
    decl: bool,
    /// A `for` binding.
    loop_bind: bool,
    /// A read.
    read: bool,
    /// A read sits in a conditional's TEST.
    read_in_cond: bool,
    /// A mention (read or write) precedes the top-level assignment.
    mention_before: bool,
    /// The name is a plain `$name` / `name` operand inside a shell word (a
    /// `test "-z \"$x\""` operand, a case pattern). Such a word is
    /// EVALUATED by the renderer at its own position, so a hoisted `let`
    /// is fine, but it is not a mention the IR walker can see.
    word_read_in_cond: bool,
}

fn census(prog: &IrProgram) -> (std::collections::HashMap<String, Census>, bool) {
    use std::collections::HashMap;
    let mut out: HashMap<String, Census> = HashMap::new();
    let mut dynamic = false;
    // walk the main body with the statement index of the enclosing
    // TOP-LEVEL statement, so "mention before the assignment" is decidable.
    for (idx, st) in prog.stmts.iter().enumerate() {
        let assign_here = match st {
            IrStmt::Assign { targets, .. } if targets.len() == 1 => targets
                .iter()
                .find(|t| t.indices.is_empty())
                .map(|t| t.var.clone()),
            _ => None,
        };
        walk_stmt(st, &mut out, true, &mut dynamic);
        let _ = idx;
    }
    for sub in &prog.subs {
        for st in &sub.body {
            walk_stmt(st, &mut out, false, &mut dynamic);
        }
    }
    (out, dynamic)
}

fn write_other(out: &mut std::collections::HashMap<String, Census>, n: &str) {
    out.entry(n.to_string()).or_default().other_writes += 1;
}

fn read(out: &mut std::collections::HashMap<String, Census>, n: &str, in_cond: bool) {
    if !ident_word(n) {
        return;
    }
    let c = out.entry(n.to_string()).or_default();
    c.read = true;
    c.read_in_cond |= in_cond;
}

/// Record a word-level read: `$name`, `${name}`, or a bare `name` token.
fn word_read(
    out: &mut std::collections::HashMap<String, Census>,
    names: &[String],
    in_cond: bool,
) {
    for n in names {
        let c = out.entry(n.clone()).or_default();
        c.read = true;
        if in_cond {
            c.word_read_in_cond = true;
        }
    }
}

/// Every name an all-literal `Interpolate` mentions as `$name`/`${name}`,
/// plus every plain word (an already-substituted operand).
fn interp_names(parts: &[InterpPart], out: &mut Vec<String>) {
    for p in parts {
        if let InterpPart::Lit(t) = p {
            out.extend(dollar_vars(t));
            for tok in t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if plain_name(tok) {
                    out.push(tok.to_string());
                }
            }
        }
    }
}

fn loop_vars(st: &IrStmt, out: &mut std::collections::HashMap<String, Census>) {
    if let IrStmt::For { var, .. } = st {
        out.entry(var.clone()).or_default().loop_bind = true;
    }
}

fn walk_stmt(
    st: &IrStmt,
    out: &mut std::collections::HashMap<String, Census>,
    top_level: bool,
    dynamic: &mut bool,
) {
    loop_vars(st, out);
    match st {
        IrStmt::Expr(IrExpr::Call { func, args }) => {
            // `getVar` is a READ and `setVar` a WRITE however they are
            // spelled — the frontends emit both as plain expressions
            // (`print(x)`, `os.environ[...] = …`), and a statement the
            // walker only sees as "a call" would make the name look
            // unwritten (or unread) and veto every hoist.
            walk_expr(&IrExpr::Call { func: func.clone(), args: args.clone() }, out, false, dynamic);
        }
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                let c = out.entry(t.var.clone()).or_default();
                c.assign_sites += 1;
                let plain = top_level && t.indices.is_empty() && targets.len() == 1;
                if plain {
                    c.top_plain = true;
                } else {
                    c.impure_sites += 1;
                }
            }
            walk_expr(expr, out, false, dynamic);
        }
        IrStmt::Declare { vars, init, .. } => {
            for d in vars {
                let c = out.entry(d.name.clone()).or_default();
                c.decl = true;
            }
            if let Some(e) = init {
                walk_expr(e, out, false, dynamic);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            write_other(out, var);
            for e in elements {
                walk_expr(e, out, false, dynamic);
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            walk_expr(cond, out, true, dynamic);
            for s in then {
                walk_stmt(s, out, false, dynamic);
            }
            for (c, b) in elsifs {
                walk_expr(c, out, true, dynamic);
                for s in b {
                    walk_stmt(s, out, false, dynamic);
                }
            }
            for s in else_ {
                walk_stmt(s, out, false, dynamic);
            }
        }
        IrStmt::While { cond, body } => {
            walk_expr(cond, out, true, dynamic);
            for s in body {
                walk_stmt(s, out, false, dynamic);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for s in body {
                walk_stmt(s, out, false, dynamic);
            }
            walk_expr(cond, out, true, dynamic);
        }
        IrStmt::For { iter, body, .. } => {
            walk_expr(iter, out, false, dynamic);
            for s in body {
                walk_stmt(s, out, false, dynamic);
            }
        }
        IrStmt::ForInit { init, cond, step, body } => {
            for s in init.iter().chain(step.iter()).chain(body.iter()) {
                walk_stmt(s, out, false, dynamic);
            }
            walk_expr(cond, out, true, dynamic);
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            for s in b {
                walk_stmt(s, out, false, dynamic);
            }
        }
        IrStmt::Function { body, named_blocks, .. } => {
            for s in body {
                walk_stmt(s, out, false, dynamic);
            }
            for (_, b) in named_blocks {
                for s in b {
                    walk_stmt(s, out, false, dynamic);
                }
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            for s in body.iter().chain(else_body.iter()).chain(finally_body.iter()) {
                walk_stmt(s, out, false, dynamic);
            }
            for e in excepts {
                for s in &e.body {
                    walk_stmt(s, out, false, dynamic);
                }
            }
        }
        IrStmt::Case { discriminant, clauses } => {
            walk_expr(discriminant, out, true, dynamic);
            for c in clauses {
                for s in &c.body {
                    walk_stmt(s, out, false, dynamic);
                }
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            for s in inner {
                walk_stmt(s, out, false, dynamic);
            }
            for r in redirects {
                walk_expr(&r.target, out, false, dynamic);
            }
        }
        IrStmt::Output { value, .. }
        | IrStmt::Die { expr: value, .. }
        | IrStmt::Warn { expr: value, .. }
        | IrStmt::SetChildError(value) => walk_expr(value, out, false, dynamic),
        IrStmt::WriteFile { path, content, .. } => {
            walk_expr(path, out, false, dynamic);
            walk_expr(content, out, false, dynamic);
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            walk_expr(cmd, out, false, dynamic);
            for a in args.iter().chain(redirects.iter()) {
                walk_expr(a, out, false, dynamic);
            }
            for (_, v) in env {
                walk_expr(v, out, false, dynamic);
            }
            if let Some(IrExpr::Str(head, _)) = Some(cmd) {
                if matches!(head.as_str(), "eval" | "source" | ".") {
                    *dynamic = true;
                }
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for st in stages {
                for s in st {
                    walk_stmt(s, out, false, dynamic);
                }
            }
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => walk_expr(e, out, false, dynamic),
        IrStmt::Expr(e) => walk_expr(e, out, false, dynamic),
        IrStmt::Ext(n) => {
            for c in n.children() {
                walk_stmt(c, out, false, dynamic);
            }
        }
        _ => {}
    }
}

fn walk_expr(
    e: &IrExpr,
    out: &mut std::collections::HashMap<String, Census>,
    in_cond: bool,
    dynamic: &mut bool,
) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => read(out, n, in_cond),
        IrExpr::Str(s, _) => {
            // a `$v` / `${v}` inside a double-quoted word is a read
            for v in dollar_vars(s) {
                read(out, &v, in_cond);
            }
        }
        IrExpr::Call { func, args } => {
            if func == "getVar" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    read(out, n, in_cond);
                    for a in args.iter().skip(1) {
                        walk_expr(a, out, in_cond, dynamic);
                    }
                    return;
                }
            }
            if func == "setVar" || func == "assign" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    write_other(out, n);
                    for a in args.iter().skip(1) {
                        walk_expr(a, out, in_cond, dynamic);
                    }
                    return;
                }
            }
            if matches!(func.as_str(), "builtin" | "exec" | "callDirect") {
                if let Some(IrExpr::Str(head, _)) = args.first() {
                    if matches!(head.as_str(), "eval" | "source" | ".") {
                        *dynamic = true;
                    }
                }
            }
            for a in args {
                walk_expr(a, out, in_cond, dynamic);
            }
        }
        IrExpr::Array(items) => {
            for a in items {
                walk_expr(a, out, in_cond, dynamic);
            }
        }
        IrExpr::Arrow(body) => {
            for s in body {
                walk_stmt(s, out, false, dynamic);
            }
        }
        IrExpr::Interpolate(parts) => {
            let all_lit = parts.iter().all(|p| matches!(p, InterpPart::Lit(_)));
            if all_lit {
                let mut names = Vec::new();
                interp_names(parts, &mut names);
                word_read(out, &names, in_cond);
                // a bare `$VAR` spelling of a test operand (`-z "$X"`) is
                // still a CONDITION read of the name
            }
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    walk_expr(x, out, in_cond, dynamic);
                }
            }
        }
        IrExpr::Ext(n) => {
            for c in n.children() {
                walk_expr(c, out, in_cond, dynamic);
            }
        }
        _ => {}
    }
}

/// `$name` references in a word's TEXT. The frontends hand most operands
/// over already substituted, so this is a conservative extra: an
/// over-detected read only keeps the name global.
fn dollar_vars(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '$' && i + 1 < b.len() {
            let j = if b[i + 1] == '{' { i + 2 } else { i + 1 };
            let mut k = j;
            while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == '_') {
                k += 1;
            }
            if k > j {
                let n: String = b[j..k].iter().collect();
                if plain_name(&n) {
                    out.push(n);
                }
            }
            i = k.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

impl Transform for ScopedHoist {
    fn name(&self) -> &'static str {
        "scoped_hoist"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let (mut names, dynamic) = census(prog);
        if dynamic {
            return;
        }
        // "first mention is the assignment": any statement BEFORE the
        // assignment that mentions the name vetoes the hoist. Only main
        // top-level statements are ordered; a mention in a sub is
        // unordered (it can run anywhere) but a sub's mention is an
        // `impure` write or a read in another scope, handled below.
        let mut first_mention: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (idx, st) in prog.stmts.iter().enumerate() {
            for n in mentioned_names(st) {
                first_mention.entry(n).or_insert(idx);
            }
        }
        let assign_at: std::collections::HashMap<String, usize> = prog
            .stmts
            .iter()
            .enumerate()
            .filter_map(|(idx, st)| match st {
                IrStmt::Assign { targets, .. } if targets.len() == 1 && targets[0].indices.is_empty() => {
                    Some((targets[0].var.clone(), idx))
                }
                _ => None,
            })
            .collect();
        let mut hoisted: Vec<String> = Vec::new();
        for (name, at) in &assign_at {
            let Some(c) = names.get(name) else { continue };
            if !plain_name(name)
                || !c.top_plain
                || c.assign_sites != 1
                || c.impure_sites != 0
                || c.other_writes != 0
                || c.decl
                || c.loop_bind
                || !c.read
                || !c.read_in_cond
                || first_mention.get(name) != Some(at)
            {
                continue;
            }
            hoisted.push(name.clone());
        }
        if hoisted.is_empty() {
            return;
        }
        for st in prog.stmts.iter_mut() {
            let (target, value) = match st {
                IrStmt::Assign { targets, expr, .. }
                    if targets.len() == 1
                        && targets[0].indices.is_empty()
                        && hoisted.contains(&targets[0].var) =>
                {
                    (targets[0].clone(), expr.clone())
                }
                _ => continue,
            };
            if !pure_value(&value) || mentions_name(&value, &target.var) {
                continue;
            }
            let _: &AssignTarget = &target;
            *st = IrStmt::Declare {
                vars: vec![Decl { name: target.var, sigil: target.sigil }],
                init: Some(value),
                local: false,
            };
        }
        let _ = &mut names;
    }
}

/// Does this expression read `name`? A self-referential RHS
/// (`x = x or "default"`) needs the value x held BEFORE the assignment
/// executes — a hoisted `let` would read the binding it is defining.
fn mentions_name(e: &IrExpr, name: &str) -> bool {
    use std::collections::HashMap;
    let mut out: HashMap<String, Census> = HashMap::new();
    let mut dynamic = false;
    read_only_probe(e, &mut out, &mut dynamic);
    out.get(name).map(|c| c.read).unwrap_or(false)
}

fn read_only_probe(
    e: &IrExpr,
    out: &mut std::collections::HashMap<String, Census>,
    dynamic: &mut bool,
) {
    // the general statement walker only descends statements; for an
    // expression RHS the same reader applies with no condition context
    walk_expr(e, out, false, dynamic);
}

/// Every name a statement mentions (read or write), for the "first
/// mention is the assignment" rule. Conservative: it may over-report.
fn mentioned_names(st: &IrStmt) -> Vec<String> {
    let mut out = std::collections::HashMap::new();
    let mut dynamic = false;
    walk_stmt(st, &mut out, true, &mut dynamic);
    out.into_iter()
        .filter(|(_, c)| c.read || c.assign_sites > 0 || c.other_writes > 0 || c.decl)
        .map(|(n, _)| n)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn str_(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn target(v: &str) -> AssignTarget {
        AssignTarget {
            var: v.to_string(),
            sigil: None,
            indices: vec![],
        }
    }

    fn read(v: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![str_(v)],
        }
    }

    fn program(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        }
    }

    /// `s = "hello world"; if <test reads s> …` — the t60 shape.
    fn cond_read_program() -> IrProgram {
        program(vec![
            IrStmt::Assign {
                targets: vec![target("s")],
                expr: str_("hello world"),
                asm: None,
            },
            IrStmt::If {
                cond: IrExpr::Call {
                    func: "contains".to_string(),
                    args: vec![read("s"), str_("world")],
                },
                then: vec![IrStmt::Assign {
                    targets: vec![target("t0")],
                    expr: str_("has"),
                    asm: None,
                }],
                elsifs: vec![],
                else_: vec![IrStmt::Assign {
                    targets: vec![target("t0")],
                    expr: str_("no"),
                    asm: None,
                }],
            },
            IrStmt::Output { value: read("t0"), newline: true, target: None },
        ])
    }

    #[test]
    fn hoists_a_value_read_only_from_a_condition() {
        let mut p = cond_read_program();
        ScopedHoist.run(&mut p, &PassContext::default());
        assert!(
            matches!(&p.stmts[0], IrStmt::Declare { vars, init: Some(_), .. } if vars[0].name == "s"),
            "assignment became a declaration: {:?}",
            p.stmts[0]
        );
    }

    #[test]
    fn a_name_never_read_from_a_condition_is_untouched() {
        // `s = "x"` read only by a top-level statement: the renderer
        // already declares it; the pass must not change the program.
        let mut p = program(vec![
            IrStmt::Assign { targets: vec![target("s")], expr: str_("x"), asm: None },
            IrStmt::Output { value: read("s"), newline: true, target: None },
        ]);
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_second_write_keeps_global_storage() {
        let mut p = cond_read_program();
        p.stmts.push(IrStmt::Assign {
            targets: vec![target("s")],
            expr: str_("again"),
            asm: None,
        });
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_nested_write_vetoes_the_hoist() {
        let mut p = cond_read_program();
        if let IrStmt::If { then, .. } = &mut p.stmts[1] {
            then.push(IrStmt::Assign {
                targets: vec![target("s")],
                expr: str_("z"),
                asm: None,
            });
        }
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_read_before_the_assignment_vetoes_the_hoist() {
        let mut p = cond_read_program();
        p.stmts.insert(
            0,
            IrStmt::Output { value: read("s"), newline: true, target: None },
        );
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn an_environment_named_var_keeps_its_environment_home() {
        // `HOME = "/x"` is not a pass-2 local: the name came from the
        // environment, and a `let` would shadow a later `$HOME` read.
        let mut p = cond_read_program();
        if let IrStmt::Assign { targets, .. } = &mut p.stmts[0] {
            targets[0].var = "HOME".to_string();
        }
        if let IrStmt::If { cond, .. } = &mut p.stmts[1] {
            *cond = IrExpr::Call { func: "contains".to_string(), args: vec![read("HOME"), str_("x")] };
        }
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_getvar_statement_counts_as_a_read() {
        // py-sh-go spells `print(x)` as `Expr(Call("exec", [echo, [getVar(x)]]))`:
        // the read sits inside an expression STATEMENT, which the walker
        // must still see as a read of `x`.
        let mut p = cond_read_program();
        // make `s` the printed value instead of a conditional operand
        p.stmts[2] = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                str_("echo"),
                IrExpr::Array(vec![read("s")]),
            ],
        });
        if let IrStmt::If { cond, .. } = &mut p.stmts[1] {
            *cond = IrExpr::Call { func: "contains".to_string(), args: vec![read("s"), str_("w")] };
        }
        ScopedHoist.run(&mut p, &PassContext::default());
        assert!(
            matches!(&p.stmts[0], IrStmt::Declare { vars, .. } if vars[0].name == "s"),
            "hoisted: {:?}",
            p.stmts[0]
        );
    }

    #[test]
    fn a_shell_word_operand_read_is_a_condition_read() {
        // The shell test spelling: the operand is one literal word carrying
        // the reference (`test("-z \"$s\"")`), not a `getVar` call.
        let mut p = cond_read_program();
        if let IrStmt::If { cond, .. } = &mut p.stmts[1] {
            *cond = IrExpr::Call {
                func: "test".to_string(),
                args: vec![str_("-z \"$s\"")],
            };
        }
        ScopedHoist.run(&mut p, &PassContext::default());
        assert!(
            matches!(&p.stmts[0], IrStmt::Declare { vars, .. } if vars[0].name == "s"),
            "word-operand read hoists the binding: {:?}",
            p.stmts[0]
        );
    }

    #[test]
    fn a_self_referential_rhs_is_never_hoisted() {
        // `x = x or "default"` reads the binding the declaration defines.
        let mut p = cond_read_program();
        p.stmts[0] = IrStmt::Assign {
            targets: vec![target("s")],
            expr: IrExpr::Call {
                func: "or".to_string(),
                args: vec![read("s"), str_("d")],
            },
            asm: None,
        };
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn eval_vetoes_every_hoist() {
        let mut p = cond_read_program();
        p.stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![str_("eval"), IrExpr::Array(vec![str_("s=1")])],
        }));
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_capture_rhs_keeps_global_storage() {
        let mut p = cond_read_program();
        if let IrStmt::Assign { expr, .. } = &mut p.stmts[0] {
            *expr = IrExpr::Call {
                func: "capture".to_string(),
                args: vec![IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![str_("hostname")],
                })])],
            };
        }
        let before = format!("{:?}", p.stmts);
        ScopedHoist.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }
}
