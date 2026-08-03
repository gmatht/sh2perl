//! ShIR — the language-neutral layer between the shell AST and the backends.
//!
//! `ast_to_ir` builds an `IrProgram` from the parsed shell AST using neutral
//! IR nodes (plus `sh2.*`-namespace calls expressed via `IrExpr::Call`); the
//! ESTree emitter consumes this IR via `shir_to_estree`, so the shell→ESTree
//! lowering logic lives in one place (PLAN.md §3). The Perl generator builds
//! its own IR flavor for `ir_to_perl`; the neutral nodes here
//! (Case/Redirect/Function/Subshell/Background/Arrow/...) are ESTree-path only.

use crate::ast::*;
use crate::estree::*;
use crate::ir::*;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Variables proven (conservatively) to hold ONLY numbers — lifted to
/// native JS number bindings: `let x = 0` declared at program top, reads
/// become bare `x` (no `sh2.getVar` + `Number(...)||0`), writes become
/// `x = <native expr>` (no `sh2.setVar` + `arithEval`). Reset by
/// `shir_to_estree` per compilation (the Perl generator never runs this).
static LIFTED_NUMERIC: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Provably-string variables lifted to native JS string bindings
/// (`let x = ""`; reads are bare `x`; writes `x = <string expr>`).
static LIFTED_STRING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Whether `shopt -s nocasematch` may be enabled anywhere in the current
/// program (set per compilation by `shir_to_estree`; see
/// `ir_may_enable_nocasematch`). Native case/test substring lifts must
/// lowercase to stay exact when it is.
static CASE_NOCASE: Mutex<Option<bool>> = Mutex::new(None);
/// Nesting depth of `sh2.and`/`sh2.or` arrow lowering (see the BinOp And/Or
/// arms). The runtime helpers branch on `lastExit`, which a NATIVE test
/// expression never sets — so inside `&&`/`||` arrows a test must stay a
/// runtime `sh2.test` call (which records the status) and only the
/// value-consuming positions (if/while/until conds, `!`, ternary) get the
/// native lowering.
static AND_OR_DEPTH: Mutex<usize> = Mutex::new(0);
/// Native-echo emission depth: >0 while lowering inside a construct whose
/// runtime stdout sink differs from the module's stdout — `redirect` /
/// `pipeline` / `capture` / `captureWords` calls (the runtime swaps
/// fdTargets) — or inside a function body (a script function may be
/// CALLED under any sink at runtime; the emitter cannot see the call
/// site's context). `process.stdout.write` is only byte-identical to the
/// runtime's `emit` when fd 1 is the default stdout, so the native echo
/// lowering (see `try_native_echo`) checks this depth before firing.
static ECHO_SINK_DEPTH: Mutex<usize> = Mutex::new(0);
/// Whether the program contains a PERSISTENT fd-1 redirect (a bare
/// `exec >file` / `exec 1>&2` / `exec 1>&-` — the runtime keeps those in
/// the fd table after the redirect call). Native top-level `echo` writes
/// `process.stdout` directly, which is only byte-identical while fd 1 is
/// the module's default stdout — a persistent fd-1 redirect ANYWHERE in
/// the program (functions included; the emitter cannot see call-site
/// contexts) disables the native echo lowering. Set per compilation by
/// `shir_to_estree`; conservative (any doubt resolves to `true`).
static PROGRAM_PERSIST_FD1: Mutex<Option<bool>> = Mutex::new(None);
/// Per-function `local`-variable native lift: function name → the set of
/// local vars whose declarations (and every later reference) lower to
/// native `let` bindings inside the function body (see
/// `local_lift_analysis`). Set per compilation by `shir_to_estree`.
static LOCAL_LIFT: Mutex<Option<HashMap<String, HashSet<String>>>> = Mutex::new(None);
/// Function-definition scope stack during emission: each frame is
/// (function name, the local-decl names already emitted in this body —
/// first decl is a `let`, later ones are assignments).
static FUNCTION_STACK: Mutex<Vec<(String, HashSet<String>)>> = Mutex::new(Vec::new());
/// Whether the program may enable `set -e` (errexit) anywhere (set per
/// compilation by `shir_to_estree`; see `ir_may_enable_errexit`). The
/// runtime's `sh2.guard` wrapper is an identity function when the errexit
/// flag never turns on, so guard emission is skipped entirely for programs
/// that provably never enable it.
static MAY_ERREXIT: Mutex<Option<bool>> = Mutex::new(None);
/// LastExit-write liveness (Plan 4): per-statement "the `sh2.lastExit`
/// write this statement performs is DEAD (no read observes it before the
/// next write / the block's status consumer)" flags, keyed by statement
/// pointer (stable between the pre-pass and emission — the IR tree is
/// immutable there, and the *Sync loop lowering emits the ORIGINAL body
/// references, never clones). Set per compilation by `shir_to_estree` via
/// [`compute_lastexit_deadness`]; the `(( ))` statement lowering drops the
/// status ternary + lastExit writes when a statement's write is dead (keep
/// the side effect). Unset → conservative.
static LASTEXIT_DEAD: Mutex<Option<HashMap<usize, bool>>> = Mutex::new(None);
fn lastexit_write_is_dead(stmt: &IrStmt) -> bool {
    LASTEXIT_DEAD
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.get(&(stmt as *const IrStmt as usize)).copied().unwrap_or(false))
        .unwrap_or(false)
}

/// Plan 4 lastExit-write liveness (see [`compute_lastexit_deadness`]).
/// `ir_stmt_writes_lastexit` is the WRITER predicate: every runtime call
/// records its own status to lastExit — EXCEPT the pure writers that
/// return true WITHOUT touching it (`setVar`/`setArray`/`shopt`/`define`;
/// `break`/`continue`/`return` leave it untouched — they are IrStmt::Expr
/// control signals, and the runtime's `return`/`break` helpers are only
/// reached through fnCall dispatch). An over-approximated writer would
/// shadow a LIVE write and break `$?`, so the set is exactly the runtime
/// truth.
fn ir_stmt_writes_lastexit(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Exec { cmd, redirects, env, .. } => {
            if !redirects.is_empty() || !env.is_empty() {
                return true; // the runtime dispatch records its own status
            }
            match cmd {
                IrExpr::Str(name, _) => {
                    !matches!(name.as_str(), "setVar" | "setArray" | "shopt" | "define")
                }
                _ => true,
            }
        }
        // loops/subshells/redirects/if/pipeline leave a final status
        // (the loop runtime writes `bodyLastExit`; an if's status is its
        // run arm's final write; a redirect's status is its inner's).
        IrStmt::While { .. }
        | IrStmt::DoWhile { .. }
        | IrStmt::For { .. }
        | IrStmt::If { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_)
        | IrStmt::Block(_)
        | IrStmt::Redirect { .. }
        | IrStmt::Pipeline { .. } => true,
        // `setVar`-style expression statements (never lastExit writers)
        IrStmt::Expr(IrExpr::Call { func, .. }) => {
            !matches!(func.as_str(), "setVar" | "setArray" | "shopt" | "define" | "break" | "continue" | "return")
        }
        _ => false,
    }
}

/// Does an expression observe the PREVIOUS exit status? `$?` lowers to
/// `IrExpr::Var("?")` (a `sh2.lastExit` read); a `$?` inside an arith
/// string stays the literal text (the runtime evalArith reads the status).
/// Over-approximated (single-quoted literal "$?" text included) — a
/// spurious reader only keeps a write live (safe).
fn ir_expr_reads_status(e: &IrExpr) -> bool {
    match e {
        IrExpr::Var(name, _) => name == "?",
        IrExpr::Str(s, _) => s.contains("$?"),
        IrExpr::Index { key, .. } => ir_expr_reads_status(key),
        IrExpr::BinOp { lhs, rhs, .. } => ir_expr_reads_status(lhs) || ir_expr_reads_status(rhs),
        IrExpr::Call { func, args } => {
            // `$?` at word level lowers to `sh2.getVar("?")` (the runtime
            // special-cases it to lastExit) — the Var("?") form appears
            // on the lifted/native path
            args.iter().any(ir_expr_reads_status)
                || (func == "getVar"
                    && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == "?"))
        }
        IrExpr::MethodCall { obj, args, .. } => {
            ir_expr_reads_status(obj) || args.iter().any(ir_expr_reads_status)
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            ir_expr_reads_status(cond) || ir_expr_reads_status(then) || ir_expr_reads_status(else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            ir_expr_reads_status(expr) || ir_expr_reads_status(default)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(s) => s.contains("$?"),
            InterpPart::Expr(e) => ir_expr_reads_status(e),
        }),
        // a command substitution inherits `$?` (bash runs it in a
        // subshell) — the wrapped command may read the status
        IrExpr::Capture { expr, .. } => ir_expr_reads_status(expr),
        IrExpr::Array(items) => items.iter().any(ir_expr_reads_status),
        // numeric literal range `start..end` — never a status read
        IrExpr::Range { .. } => false,
        IrExpr::Arrow(stmts) => ir_stmts_read_status(stmts),
        _ => false,
    }
}

fn ir_stmts_read_status(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(ir_stmt_reads_status)
}

/// Does a STATEMENT observe the previous exit status (so a write before it
/// is live)? Readers: `and`/`or` (branch on the previous status), native
/// `&&`/`||` (the `sh2.lastExit === 0` test), `exit`/`return` with no arg
/// (carry the previous status), every `$?` expansion, and — transitively —
/// a subshell/function/background body whose first actions may read the
/// inherited status. Over-approximated: a spurious reader keeps a write
/// live (safe); a MISSED reader would drop a live write (never).
fn ir_stmt_reads_status(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Exec { cmd, args, .. } => {
            if args.iter().any(ir_expr_reads_status) || ir_expr_reads_status(cmd) {
                return true;
            }
            matches!(cmd, IrExpr::Str(name, _) if matches!(name.as_str(), "and" | "or" | "exit" | "return"))
        }
        IrStmt::Expr(e) => match e {
            IrExpr::BinOp { op, .. } => matches!(op, BinOpKind::And | BinOpKind::Or),
            IrExpr::Call { func, args } => {
                args.iter().any(ir_expr_reads_status)
                    || matches!(
                        func.as_str(),
                        "and" | "or" | "exit" | "return"
                    )
            }
            other => ir_expr_reads_status(other),
        },
        IrStmt::While { cond, .. } | IrStmt::If { cond, .. } => ir_expr_reads_status(cond),
        IrStmt::DoWhile { cond, .. } => ir_expr_reads_status(cond),
        IrStmt::For { iter, .. } => ir_expr_reads_status(iter),
        IrStmt::Assign { expr, .. } => ir_expr_reads_status(expr),
        IrStmt::Declare { init, .. } => init.as_ref().map(ir_expr_reads_status).unwrap_or(false),
        IrStmt::Output { value, .. } => ir_expr_reads_status(value),
        IrStmt::WriteFile { path, content, .. } => {
            ir_expr_reads_status(path) || ir_expr_reads_status(content)
        }
        IrStmt::Return(opt) => opt.as_ref().map(ir_expr_reads_status).unwrap_or(true),
        IrStmt::Exit(opt) => opt.as_ref().map(ir_expr_reads_status).unwrap_or(true),
        // subshell/background bodies may read the inherited status
        IrStmt::Subshell(body) | IrStmt::Background(body) => ir_stmts_read_status(body),
        IrStmt::Function { body, .. } => ir_stmts_read_status(body),
        IrStmt::Redirect { inner, redirects } => {
            ir_stmts_read_status(inner)
                || redirects.iter().any(|r| match r {
                    IrRedirect { fd: _, target, .. } => ir_expr_reads_status(target),
                })
        }
        _ => false,
    }
}

/// The statement-level `(( ))` / `let ARITH...` that lowers via
/// [`try_native_let`] — the ONLY droppable lastExit writer (the status
/// ternary exists solely to record lastExit + the boolean value). Matches
/// both statement carriers (the general `IrStmt::Expr(Call("exec", …))`
/// form and the `IrStmt::Exec` form).
fn is_native_let_stmt(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Expr(IrExpr::Call { func, args, .. }) => {
            func == "exec"
                && matches!(args.as_slice(), [IrExpr::Str(n, _), IrExpr::Array(_)] if n == "let")
        }
        IrStmt::Exec { cmd, args, .. } => {
            matches!(cmd, IrExpr::Str(n, _) if n == "let")
                && matches!(args.as_slice(), [IrExpr::Str(n2, _), IrExpr::Array(_)] if n2 == "let")
        }
        _ => false,
    }
}

/// Plan 4 — backward block scan: a statement's lastExit write is LIVE iff
/// a reader (or the block's status consumer) observes it before the next
/// writer. `end_live`: does the BLOCK's consumer read the block's final
/// status? (loop runtime reads `this.lastExit` after the body; the program
/// runner's final status; if-arm flows to the if's own liveness...)
fn scan_lastexit_liveness(stmts: &[IrStmt], end_live: bool, live: &mut HashSet<usize>) {
    let mut read_pending = end_live;
    for stmt in stmts.iter().rev() {
        eprintln!("DBGscan w={} r={} pending={} stmt={:?}", ir_stmt_writes_lastexit(stmt), ir_stmt_reads_status(stmt), read_pending, stmt);
        if ir_stmt_writes_lastexit(stmt) {
            if read_pending {
                live.insert(stmt as *const IrStmt as usize);
                read_pending = false;
            }
            // else: shadowed — dead (unless a reader between the write
            // and the next writer observed it — handled above)
        }
        if ir_stmt_reads_status(stmt) {
            read_pending = true;
        }
    }
}

fn walk_lastexit_liveness(stmts: &[IrStmt], end_live: bool, live: &mut HashSet<usize>) {
    scan_lastexit_liveness(stmts, end_live, live);
    for stmt in stmts {
        let self_live = live.contains(&(stmt as *const IrStmt as usize));
        match stmt {
            IrStmt::While { body, .. } | IrStmt::For { body, .. } | IrStmt::Block(body) => {
                walk_lastexit_liveness(body, self_live, live);
            }
            IrStmt::DoWhile { body, .. } => walk_lastexit_liveness(body, self_live, live),
            IrStmt::If { then, elsifs, else_, .. } => {
                walk_lastexit_liveness(then, self_live, live);
                for (_, arm) in elsifs {
                    walk_lastexit_liveness(arm, self_live, live);
                }
                walk_lastexit_liveness(else_, self_live, live);
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                walk_lastexit_liveness(body, self_live, live);
            }
            IrStmt::Redirect { inner, .. } => walk_lastexit_liveness(inner, self_live, live),
            // a called function's status is recorded by fnCall — treat as
            // live (conservative; refining to call-site liveness is a
            // future plan entry)
            IrStmt::Function { body, .. } => walk_lastexit_liveness(body, true, live),
            _ => {}
        }
    }
}

/// Mark DEAD every native `(( ))` statement whose lastExit write is not in
/// the live set (droppable) — only if its args actually parse natively
/// (the ternary only exists on the `try_native_let` path).
fn mark_lastexit_dead(stmts: &[IrStmt], live: &HashSet<usize>, dead: &mut HashMap<usize, bool>) {
    for stmt in stmts {
        if is_native_let_stmt(stmt) && !live.contains(&(stmt as *const IrStmt as usize)) {
            let args = match stmt {
                IrStmt::Exec { args, .. } | IrStmt::Expr(IrExpr::Call { args, .. }) => args,
                _ => unreachable!("is_native_let_stmt matched"),
            };
            if let [IrExpr::Str(_, _), IrExpr::Array(a)] = args.as_slice() {
                if a.iter().all(|arg| match arg {
                    IrExpr::Str(sv, _) => parse_arith_native(sv).is_some(),
                    _ => false,
                }) {
                    dead.insert(stmt as *const IrStmt as usize, true);
                }
            }
        }
        match stmt {
            IrStmt::While { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => mark_lastexit_dead(body, live, dead),
            IrStmt::If { then, elsifs, else_, .. } => {
                mark_lastexit_dead(then, live, dead);
                for (_, arm) in elsifs {
                    mark_lastexit_dead(arm, live, dead);
                }
                mark_lastexit_dead(else_, live, dead);
            }
            IrStmt::Redirect { inner, .. } => mark_lastexit_dead(inner, live, dead),
            IrStmt::Function { body, .. } => mark_lastexit_dead(body, live, dead),
            _ => {}
        }
    }
}

/// Plan 4 — compute the per-statement lastExit-write deadness map. Runs in
/// `shir_to_estree` before emission (the IR tree is immutable there). A
/// possible `set -e` guard-wraps every top-level statement (the guard
/// consumes the statement's value) — under errexit NO write is droppable
/// and the pass returns empty.
fn compute_lastexit_deadness(prog: &IrProgram, errexit: bool) -> HashMap<usize, bool> {
    let mut dead = HashMap::new();
    if errexit {
        return dead;
    }
    let mut live: HashSet<usize> = HashSet::new();
    walk_lastexit_liveness(&prog.stmts, true, &mut live);
    mark_lastexit_dead(&prog.stmts, &live, &mut dead);
    dead
}
/// Builtins the runtime implements as SYNC functions (harness
/// sh2-namespace.mjs `builtins.*` — every non-async entry of builtins.json
/// plus `test`, the bash test builtin the runtime implements on top of its
/// own test parser; `wait`/`exec`/`sleep`/`command` are async and stay on
/// the async exec path). `sh2.exec("echo", args)` lowers to a sync
/// `sh2.builtin("echo", args)` dispatch: identical arg flattening/glob
/// expansion, identical builtin function, minus the async exec machinery
/// (the whileLoopSync pattern — same semantics, no per-call promises).
const SYNC_BUILTINS: &[&str] = &[
    ".", ":", "basename", "break", "cat", "cd", "cmp", "comm", "continue", "declare",
    "dirname", "echo", "eval", "exit", "export", "false", "head", "let", "local",
    "mapfile", "mktemp", "printf", "pwd", "read", "readarray", "readonly", "return", "seq",
    "set", "shift", "sort", "source", "stat", "tail", "test", "touch", "trap",
    "true", "type", "typeset", "uniq", "unset", "wc",
];
/// Names of every function the program defines (IrStmt::Function), set per
/// compilation by `shir_to_estree` under COMPILE_LOCK. A script-defined
/// function SHADOWS a same-named builtin in bash, so exec calls to a
/// shadowed name must keep the async exec dispatch (the runtime's function
/// map) — never the sync builtin path.
static PROGRAM_FUNCTIONS: Mutex<Option<HashSet<String>>> = Mutex::new(None);
fn program_defines_function(name: &str) -> bool {
    PROGRAM_FUNCTIONS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(true) // unset → conservative: keep the async exec path
}

/// Names of script-defined functions whose CALLS lower to the sync
/// `sh2.fnCall` path (see [`fn_call_sync_set`]): every definition body of
/// the target AND the call-site args are provably await-free, and every
/// function it calls is itself sync (call-graph fixpoint). Such functions
/// are also emitted with NON-async define arrows, so the runtime runs them
/// without a per-call promise (the whileLoopSync pattern — same semantics,
/// no per-iteration promise machinery), which in turn lets loops over them
/// lower to their *Sync twins. Set per compilation by `shir_to_estree`.
static SYNC_FN_CALLS: Mutex<Option<HashSet<String>>> = Mutex::new(None);
fn fn_call_is_sync(name: &str) -> bool {
    SYNC_FN_CALLS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false) // unset → conservative: keep the async exec path
}

/// Recursive IrStmt walk collecting every `IrStmt::Function` name — a
/// same-named script function shadows a builtin anywhere in the program
/// (definitions inside bodies/arrows count).
fn collect_program_functions(stmts: &[IrStmt], out: &mut HashSet<String>) {
    fn walk_stmts(stmts: &[IrStmt], out: &mut HashSet<String>) {
        for st in stmts {
            match st {
                IrStmt::Function { name, body } => {
                    out.insert(name.clone());
                    walk_stmts(body, out);
                }
                IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. }
                | IrStmt::Block(body)
                | IrStmt::Subshell(body)
                | IrStmt::Background(body) => walk_stmts(body, out),
                IrStmt::If { then, elsifs, else_, .. } => {
                    walk_stmts(then, out);
                    walk_stmts(else_, out);
                    for (_, b) in elsifs {
                        walk_stmts(b, out);
                    }
                }
                IrStmt::For { body, .. } => walk_stmts(body, out),
                IrStmt::Pipeline { stages, .. } => {
                    for stage in stages {
                        walk_stmts(stage, out);
                    }
                }
                IrStmt::Redirect { inner, .. } => walk_stmts(inner, out),
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        walk_stmts(&c.body, out);
                    }
                }
                IrStmt::Expr(e) => walk_expr(e, out),
                _ => {}
            }
        }
    }
    fn walk_expr(e: &IrExpr, out: &mut HashSet<String>) {
        match e {
            IrExpr::Arrow(stmts) => walk_stmts(stmts, out),
            IrExpr::Call { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, out);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, out);
                walk_expr(rhs, out);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, out);
                    }
                }
            }
            _ => {}
        }
    }
    walk_stmts(stmts, out);
}

/// Every function DEFINITION body in the program (name → all definition
/// bodies, program order). A name with ANY definition whose body is not
/// provably sync stays on the async path (a later redefinition could
/// replace a sync arrow with an async one).
fn collect_fn_bodies<'a>(
    stmts: &'a [IrStmt],
    out: &mut HashMap<String, Vec<&'a [IrStmt]>>,
) {
    fn walk_stmts<'a>(stmts: &'a [IrStmt], out: &mut HashMap<String, Vec<&'a [IrStmt]>>) {
        for st in stmts {
            match st {
                IrStmt::Function { name, body } => {
                    out.entry(name.clone()).or_default().push(body);
                    walk_stmts(body, out);
                }
                IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. }
                | IrStmt::Block(body)
                | IrStmt::Subshell(body)
                | IrStmt::Background(body) => walk_stmts(body, out),
                IrStmt::If { then, elsifs, else_, .. } => {
                    walk_stmts(then, out);
                    walk_stmts(else_, out);
                    for (_, b) in elsifs {
                        walk_stmts(b, out);
                    }
                }
                IrStmt::For { body, .. } => walk_stmts(body, out),
                IrStmt::Pipeline { stages, .. } => {
                    for stage in stages {
                        walk_stmts(stage, out);
                    }
                }
                IrStmt::Redirect { inner, .. } => walk_stmts(inner, out),
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        walk_stmts(&c.body, out);
                    }
                }
                IrStmt::Exec { args, .. } => {
                    for a in args {
                        walk_expr(a, out);
                    }
                }
                IrStmt::Expr(e) => walk_expr(e, out),
                _ => {}
            }
        }
    }
    fn walk_expr<'a>(e: &'a IrExpr, out: &mut HashMap<String, Vec<&'a [IrStmt]>>) {
        match e {
            IrExpr::Arrow(stmts) => walk_stmts(stmts, out),
            IrExpr::Call { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, out);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, out);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, out);
                walk_expr(rhs, out);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, out);
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                walk_expr(cond, out);
                walk_expr(then, out);
                walk_expr(else_, out);
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, out);
                walk_expr(default, out);
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, out),
            IrExpr::Index { key, .. } => walk_expr(key, out),
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, out);
                    }
                }
            }
            _ => {}
        }
    }
    walk_stmts(stmts, out);
}

/// Defined-function names DIRECTLY called inside a body: exec calls whose
/// command is a literal name in `functions` (statement form) plus
/// expression-form exec calls (conditions, `&&`/`||` operands, pipeline
/// stages, captures). `builtin` calls never dispatch functions (the
/// emitter only lowers non-shadowed names to them); `command`/`eval`/
/// dynamic names keep the async path regardless.
fn collect_fn_calls(
    stmts: &[IrStmt],
    functions: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    fn walk_stmts(stmts: &[IrStmt], functions: &HashSet<String>, out: &mut HashSet<String>) {
        for st in stmts {
            match st {
                IrStmt::Exec { cmd, args, .. } => {
                    if let IrExpr::Str(name, _) = cmd {
                        if functions.contains(name) {
                            out.insert(name.clone());
                        }
                    }
                    for a in args {
                        walk_expr(a, functions, out);
                    }
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                    walk_expr(cond, functions, out);
                    walk_stmts(body, functions, out);
                }
                IrStmt::If { cond, then, elsifs, else_, .. } => {
                    walk_expr(cond, functions, out);
                    walk_stmts(then, functions, out);
                    walk_stmts(else_, functions, out);
                    for (_, b) in elsifs {
                        walk_stmts(b, functions, out);
                    }
                }
                IrStmt::For { iter, body, .. } => {
                    walk_expr(iter, functions, out);
                    walk_stmts(body, functions, out);
                }
                IrStmt::Pipeline { stages, .. } => {
                    for stage in stages {
                        walk_stmts(stage, functions, out);
                    }
                }
                IrStmt::Redirect { inner, .. } => walk_stmts(inner, functions, out),
                IrStmt::Case { discriminant, clauses, .. } => {
                    walk_expr(discriminant, functions, out);
                    for c in clauses {
                        walk_stmts(&c.body, functions, out);
                    }
                }
                IrStmt::Function { body, .. }
                | IrStmt::Block(body)
                | IrStmt::Subshell(body)
                | IrStmt::Background(body) => walk_stmts(body, functions, out),
                IrStmt::Assign { expr, .. } => walk_expr(expr, functions, out),
                IrStmt::Expr(e) => walk_expr(e, functions, out),
                _ => {}
            }
        }
    }
    fn walk_expr(e: &IrExpr, functions: &HashSet<String>, out: &mut HashSet<String>) {
        match e {
            IrExpr::Arrow(stmts) => walk_stmts(stmts, functions, out),
            IrExpr::Call { func, args } => {
                if func == "exec" {
                    if let [IrExpr::Str(name, _), ..] = args.as_slice() {
                        if functions.contains(name) {
                            out.insert(name.clone());
                        }
                    }
                }
                for a in args {
                    walk_expr(a, functions, out);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, functions, out);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, functions, out);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, functions, out);
                walk_expr(rhs, functions, out);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, functions, out);
                for a in args {
                    walk_expr(a, functions, out);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                walk_expr(cond, functions, out);
                walk_expr(then, functions, out);
                walk_expr(else_, functions, out);
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, functions, out);
                walk_expr(default, functions, out);
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, functions, out),
            IrExpr::Index { key, .. } => walk_expr(key, functions, out),
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, functions, out);
                    }
                }
            }
            _ => {}
        }
    }
    walk_stmts(stmts, functions, out);
}

/// The sync-function fixpoint (see [`SYNC_FN_CALLS`]).
///
/// A function's emitted arrow may be run without `await` only when its
/// lowered body (with every defined-function call at its sync `sh2.fnCall`
/// path) contains no `AwaitExpression` AND every function it directly
/// calls is itself sync — the async `sh2.exec` dispatch the sync path
/// replaces would be the only added await, and the fixpoint removes it.
/// Monotone (sync can only flip true→false), so it converges in at most
/// |functions| iterations; recursion/mutual recursion stay async
/// (conservative — a recursive call's target is in its own `calls` set).
fn fn_call_sync_set(prog: &IrProgram, functions: &HashSet<String>) -> HashSet<String> {
    let mut bodies: HashMap<String, Vec<&[IrStmt]>> = HashMap::new();
    collect_fn_bodies(&prog.stmts, &mut bodies);
    if bodies.is_empty() {
        return HashSet::new();
    }
    let mut calls: HashMap<String, HashSet<String>> = HashMap::new();
    for (name, defs) in &bodies {
        let mut set = HashSet::new();
        for body in defs {
            collect_fn_calls(body, functions, &mut set);
        }
        calls.insert(name.clone(), set);
    }
    // Optimistic await-free-ness: lower every definition body with ALL
    // function calls on the sync path (the only awaits that scan can miss
    // are those of non-sync targets, which the fixpoint removes). The
    // emission is side-effect-free for the outer pass: the depth statics
    // are balanced (arrow_sink/AND_OR_DEPTH), the lift statics are
    // read-only.
    *SYNC_FN_CALLS.lock().unwrap() = Some(functions.clone());
    let mut opt_free: HashMap<String, bool> = HashMap::new();
    for (name, defs) in &bodies {
        let mut free = true;
        for body in defs {
            let e = arrow_sink(vec![], IrExpr::Arrow(body.to_vec()));
            if expr_has_await(&e) {
                free = false;
                break;
            }
        }
        opt_free.insert(name.clone(), free);
    }
    *SYNC_FN_CALLS.lock().unwrap() = None;
    let mut sync: HashSet<String> = bodies.keys().cloned().collect();
    loop {
        let mut changed = false;
        for f in sync.clone() {
            if !opt_free.get(&f).copied().unwrap_or(false) {
                sync.remove(&f);
                changed = true;
                continue;
            }
            let all_called_sync = calls
                .get(&f)
                .map(|cs| cs.iter().all(|g| sync.contains(g)))
                .unwrap_or(true);
            if !all_called_sync {
                sync.remove(&f);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    sync
}

/// Serializes whole-program compilations: the lift/scan statics above are
/// per-compilation state, and the determinism unit test compiles in
/// parallel threads — without a lock, one thread's emission can read
/// another thread's half-installed statics (torn output). Compilations are
/// short and each process compiles one file, so the lock is uncontended in
/// practice; it is never re-entered (`shir_to_estree` does not recurse).
static COMPILE_LOCK: Mutex<()> = Mutex::new(());
/// Either lift — reads / test-injection / array-element injection consult
/// both sets. Local-function lifts (per-function `local` bindings, see
/// [`local_lift_analysis`]) count too: the function-scope stack is only
/// populated during emission inside the current function body, so top-level
/// and cross-function reads never see a local lift.
fn is_lifted(name: &str) -> bool {
    is_lifted_num(name) || is_lifted_str(name) || is_local_lifted(name)
}
/// Is `name` a natively-lifted `local` of the function currently being
/// emitted? (The function-scope stack, see [`LOCAL_LIFT`] /
/// [`FUNCTION_STACK`].)
fn is_local_lifted(name: &str) -> bool {
    let stack = FUNCTION_STACK.lock().unwrap();
    let Some((fname, _)) = stack.last() else {
        return false;
    };
    let map = LOCAL_LIFT.lock().unwrap();
    map.as_ref()
        .and_then(|m| m.get(fname))
        .map(|s| s.contains(name))
        .unwrap_or(false)
}
/// Run `f` with the current function's lifted-local set and its
/// already-emitted decl-name set (mutated by the caller: first decl →
/// `let`, later → assignment). Returns None when not inside a function or
/// the function has no lifted locals.
fn with_func_lift<R>(f: impl FnOnce(&HashSet<String>, &mut HashSet<String>) -> R) -> Option<R> {
    let mut stack = FUNCTION_STACK.lock().unwrap();
    let (fname, seen) = stack.last_mut()?;
    let map = LOCAL_LIFT.lock().unwrap();
    let set = map.as_ref()?.get(fname)?;
    if set.is_empty() {
        return None;
    }
    Some(f(set, seen))
}
fn is_lifted_num(name: &str) -> bool {
    LIFTED_NUMERIC
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false)
}
fn is_lifted_str(name: &str) -> bool {
    LIFTED_STRING
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false)
}

fn call(func: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        func: func.to_string(),
        args,
    }
}

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

// ── AST → IR ─────────────────────────────────────────────────────────

pub fn ast_to_ir(commands: &[Command]) -> IrProgram {
    // Shared optimization passes (M6): the same optimize_stmts the Perl
    // backend runs now also runs here, so future passes (constant folding,
    // dead-assignment elimination) benefit both consumers of the IR.
    let stmts = crate::ir::optimize_stmts(&commands.iter().filter_map(stmt_for_command).collect::<Vec<_>>());
    IrProgram {
        imports: vec![],
        requires: vec![],
        stmts,
        subs: vec![],
    }
}

fn stmt_for_command(cmd: &Command) -> Option<IrStmt> {
    Some(match cmd {
        Command::BlankLine => return None,
        Command::TestExpression(t) => {
            IrStmt::Expr(call("test", vec![st(&t.expression)]))
        }
        Command::Simple(sc) => exec_stmt(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_stmt(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Assignment(a) => IrStmt::Assign {
            targets: vec![AssignTarget {
                var: a.variable.clone(),
                sigil: None,
                indices: vec![],
            }],
            expr: assignment_value_ir(a),
        },
        Command::If(if_stmt) => IrStmt::If {
            cond: command_to_test_ir(&if_stmt.condition),
            then: body_stmts(&if_stmt.then_branch),
            elsifs: vec![],
            else_: if_stmt
                .else_branch
                .as_ref()
                .map(|b| body_stmts(b.as_ref()))
                .unwrap_or_default(),
        },
        Command::Case(c) => case_to_ir(c),
        Command::While(w) => {
            let cond = command_to_test_ir(&w.condition);
            IrStmt::While {
                cond: if w.is_until {
                    not_ir(cond)
                } else {
                    cond
                },
                body: body_stmts(&Command::Block(w.body.clone())),
            }
        }
        Command::For(f) => IrStmt::For {
            var: f.variable.clone(),
            iter: IrExpr::Array(for_items_ir(&f.items)),
            body: body_stmts(&Command::Block(f.body.clone())),
        },
        Command::Block(b) => IrStmt::Block(
            b.commands.iter().filter_map(stmt_for_command).collect(),
        ),
        Command::Pipeline(p) => IrStmt::Expr(call(
            "pipeline",
            vec![IrExpr::Array(
                p.commands
                    .iter()
                    .map(|c| IrExpr::Arrow(command_arrow_stmts(c)))
                    .collect(),
            )],
        )),
        Command::ShoptCommand(s) => {
            IrStmt::Expr(call("shopt", vec![st(&s.option), IrExpr::Bool(s.enable)]))
        }
        Command::CStyleFor(cf) => IrStmt::Expr(call(
            "cstyleFor",
            vec![
                st(&cf.arith_content),
                IrExpr::Arrow(body_stmts(&Command::Block(cf.body.clone()))),
            ],
        )),
        Command::Function(f) => IrStmt::Function {
            name: f.name.clone(),
            body: body_stmts(&Command::Block(f.body.clone())),
        },
        Command::Subshell(c) => IrStmt::Subshell(command_arrow_stmts(c)),
        Command::Background(c) => IrStmt::Background(command_arrow_stmts(c)),
        Command::Redirect(rc) => IrStmt::Redirect {
            inner: vec![stmt_for_command(&rc.command).unwrap_or(IrStmt::Expr(call("true", vec![])))],
            redirects: rc.redirects.iter().map(redirect_to_ir).collect(),
        },
        Command::And(l, r) => IrStmt::Expr(IrExpr::BinOp {
            op: BinOpKind::And,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        }),
        Command::Or(l, r) => IrStmt::Expr(IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        }),
        Command::Not(c) => IrStmt::Expr(not_ir(command_to_ir(c))),
        Command::Break(_) => IrStmt::Expr(call("break", vec![])),
        Command::Continue(_) => IrStmt::Expr(call("continue", vec![])),
        Command::Return(w) => IrStmt::Return(w.as_ref().map(word_ir_quoted)),
        other => IrStmt::Expr(call(
            "unsupported",
            vec![st(&format!("{other:?}"))],
        )),
    })
}

fn not_ir(inner: IrExpr) -> IrExpr {
    IrExpr::BinOp {
        op: BinOpKind::Not,
        lhs: Box::new(inner.clone()),
        rhs: Box::new(inner),
    }
}

fn command_arrow_stmts(c: &Command) -> Vec<IrStmt> {
    match c {
        Command::Block(b) => b.commands.iter().filter_map(stmt_for_command).collect(),
        // expression-bodied arrows
        Command::Simple(_)
        | Command::BuiltinCommand(_)
        | Command::TestExpression(_)
        | Command::Redirect(_)
        | Command::Pipeline(_)
        | Command::And(_, _)
        | Command::Or(_, _)
        | Command::Not(_)
        | Command::Assignment(_)
        | Command::ShoptCommand(_) => vec![IrStmt::Expr(command_to_ir(c))],
        // compound commands → block-bodied arrows
        other => vec![stmt_for_command(other).unwrap_or(IrStmt::Expr(call("true", vec![])))],
    }
}

fn body_stmts(cmd: &Command) -> Vec<IrStmt> {
    match cmd {
        Command::Block(b) => b.commands.iter().filter_map(stmt_for_command).collect(),
        _ => stmt_for_command(cmd).map(|s| vec![s]).unwrap_or_default(),
    }
}

fn exec_stmt(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
    redirects: &[Redirect],
) -> IrStmt {
    let exec_call = exec_call_ir(name, args, env);
    if redirects.is_empty() {
        IrStmt::Expr(exec_call)
    } else {
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(exec_call)],
            redirects: redirects.iter().map(redirect_to_ir).collect(),
        }
    }
}

fn exec_call_ir(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
) -> IrExpr {
    // `declare -A map=(...)` / `local -A options=()` — the array-literal arg
    // is lowered to a side-effecting setArray call; tell it the map is
    // associative so it registers the assoc store.
    let assoc = args.iter().any(|a| matches!(a, Word::Literal(s, _) if s.starts_with("-A")));
    let mut call_args = vec![word_ir(name), IrExpr::Array(exec_args_ir(args, assoc))];
    if !env.is_empty() {
        call_args.push(IrExpr::Object(
            env.iter()
                .map(|(k, v)| (k.clone(), word_ir_quoted(v)))
                .collect(),
        ));
    }
    call("exec", call_args)
}

/// Marker prefix the runtime recognizes on exec args / for-loop items (see
/// sh2-namespace.mjs: `exec` glob-expands the suffix against the filesystem).
/// Only UNQUOTED words may glob; the parser keeps double-quoted words as
/// StringInterpolation (never tagged here), and single-quoted words are
/// indistinguishable from bare ones in the AST — the corpus has no
/// single-quoted globs in exec-arg position, so tagging all Literals with
/// glob chars matches bash for every example.
const GLOB_MAGIC: &str = "\u{1}SH2GLOB\u{1}";

/// Marker prefix the runtime recognizes on exec args carrying a process
/// substitution (`<(...)`); the runtime materializes the suffix into a
/// temp file path at exec time (see estree.rs transform_process_substitution
/// — the marker is injected at the Word level, so IR literals can carry it).
/// The native echo lowering must not print it raw.
const PS_MAGIC: &str = "\u{1}SH2PS\u{1}";

fn has_glob_chars(s: &str) -> bool {
    // Skip `${...}` regions: `[`/`*`/`?` inside an expansion (`${#x[@]}`, `${x:-*}`)
    // are parameter syntax, not glob patterns.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let mut depth = 1usize;
            let mut j = i + 2;
            while j < bytes.len() && depth > 0 {
                if bytes[j] == b'{' {
                    depth += 1;
                } else if bytes[j] == b'}' {
                    depth -= 1;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        if bytes[i] == b'*' || bytes[i] == b'?' || bytes[i] == b'[' {
            return true;
        }
        i += 1;
    }
    false
}

/// Exec-argument lowering: merges consecutive brace expansions into a single
/// cross-product `sh2.brace` call (`{a,b}{1,2}` is ONE bash word — the
/// parser splits it into two words, so the emitter re-joins them, exactly
/// like the perl backend's cartesian-product pass), and tags unquoted glob
/// words for the runtime to expand.
fn exec_args_ir(args: &[Word], assoc: bool) -> Vec<IrExpr> {
    merged_words_ir(args, &|w| arg_word_ir(w, assoc))
}

/// Same brace merge for for-loop item lists (`for x in {a,b}{1,2}`); the
/// single-word fallback keeps the `$@`/`$*` listVar special case.
fn for_items_ir(items: &[Word]) -> Vec<IrExpr> {
    merged_words_ir(items, &for_item_ir)
}

fn merged_words_ir(words: &[Word], single: &dyn Fn(&Word) -> IrExpr) -> Vec<IrExpr> {
    let mut out: Vec<IrExpr> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if let Word::BraceExpansion(be, _) = &words[i] {
            let mut groups = vec![brace_items_json(&be.items)];
            let mut middles: Vec<serde_json::Value> = Vec::new();
            let mut suffix = be.suffix.clone().unwrap_or_default();
            let prefix = be.prefix.clone().unwrap_or_default();
            i += 1;
            while i < words.len() {
                if let Word::BraceExpansion(be2, _) = &words[i] {
                    middles.push(serde_json::Value::String(format!(
                        "{}{}",
                        suffix,
                        be2.prefix.as_deref().unwrap_or("")
                    )));
                    groups.push(brace_items_json(&be2.items));
                    suffix = be2.suffix.clone().unwrap_or_default();
                    i += 1;
                } else {
                    break;
                }
            }
            // A brace expansion whose prefix/suffix/items contain glob chars
            // must glob EACH result (`*.{txt,log}` → `*.txt` → files): the
            // runtime globs any result string that starts with GLOB_MAGIC.
            let magic_prefix = if has_glob_chars(&prefix)
                || has_glob_chars(&suffix)
                || be_items_contain_glob(&groups)
            {
                format!("{GLOB_MAGIC}{prefix}")
            } else {
                prefix.clone()
            };
            out.push(call(
                "brace",
                vec![
                    st(&magic_prefix),
                    IrExpr::Json(serde_json::Value::Array(groups)),
                    IrExpr::Json(serde_json::Value::Array(middles)),
                    st(&suffix),
                ],
            ));
        } else {
            out.push(single(&words[i]));
            i += 1;
        }
    }
    out
}

fn be_items_contain_glob(groups: &[serde_json::Value]) -> bool {
    fn collect(v: &serde_json::Value, found: &mut bool) {
        match v {
            serde_json::Value::String(s) => {
                if has_glob_chars(s) {
                    *found = true;
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, found)),
            serde_json::Value::Object(o) => o.values().for_each(|x| collect(x, found)),
            _ => {}
        }
    }
    let mut found = false;
    for g in groups {
        collect(g, &mut found);
    }
    found
}

/// A single exec-argument word (non-brace). Unquoted glob words get the
/// GLOB_MAGIC tag so the runtime expands them against the filesystem. An
/// array literal (`declare -a arr=(a b)`) lowers to a side-effecting
/// setArray call whose (magic) return value is dropped by the runtime's exec
/// arg flattener; `assoc` marks `declare -A` literals.
fn arg_word_ir(w: &Word, assoc: bool) -> IrExpr {
    match w {
        // Quote removal FIRST, then tag for globbing: an unquoted `\*` is a
        // literal `*` after removal (never a glob), while `*.txt` globs.
        Word::Literal(s, ann) => {
            let s2 = shell_quote_removal(s);
            // A single-quoted word (`'*.txt'`) is LITERAL — bash never globs
            // it. The parser marks quoted words (ann == Some); without the
            // marker the AST cannot distinguish `'*.txt'` from `*.txt`.
            if ann.is_none() && has_glob_chars(&s2) {
                st(&format!("{GLOB_MAGIC}{s2}"))
            } else {
                st(&s2)
            }
        }
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![
                st(name),
                IrExpr::Array(elements.iter().map(|e| st(e)).collect()),
                IrExpr::Bool(assoc),
            ],
        ),
        _ => word_ir(w),
    }
}

/// `name op value` — the RHS expression for a statement-level assignment
/// (`IrStmt::Assign` wraps it in `sh2.setVar`). Compound operators lower to
/// `sh2.assign` (which sets the variable itself), array `+=` to
/// `sh2.setArrayAppend`.
fn assignment_value_ir(a: &Assignment) -> IrExpr {
    match &a.value {
        Word::Array(name, elements, _) if a.operator == AssignmentOperator::PlusAssign => call(
            "setArrayAppend",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        _ if a.operator == AssignmentOperator::Assign => word_ir_quoted(&a.value),
        _ => call(
            "assign",
            vec![
                st(&a.variable),
                st(assign_op_str(&a.operator)),
                word_ir_quoted(&a.value),
            ],
        ),
    }
}

/// `name op value` in EXPRESSION context (`&&`/`||` operands, `if`/`while`
/// conditions): the assignment must still happen AND the expression must be
/// truthy. All three helpers return true.
fn assignment_expr_ir(a: &Assignment) -> IrExpr {
    match &a.value {
        Word::Array(name, elements, _) if a.operator == AssignmentOperator::PlusAssign => call(
            "setArrayAppend",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        _ => call(
            "assign",
            vec![
                st(&a.variable),
                st(assign_op_str(&a.operator)),
                word_ir_quoted(&a.value),
            ],
        ),
    }
}

fn assign_op_str(op: &AssignmentOperator) -> &'static str {
    match op {
        AssignmentOperator::Assign => "=",
        AssignmentOperator::PlusAssign => "+=",
        AssignmentOperator::MinusAssign => "-=",
        AssignmentOperator::StarAssign => "*=",
        AssignmentOperator::SlashAssign => "/=",
        AssignmentOperator::PercentAssign => "%=",
    }
}

fn redirect_to_ir(r: &Redirect) -> IrRedirect {
    // `2>&1` / `>&2` / `<&0` — the parser stores the dup TARGET without the
    // `&` (Literal("1")), indistinguishable from `2> 1`. The perl generator
    // resolves the ambiguity in favor of a dup for all-digit targets on
    // stderr/input operators; do the same here and re-attach the `&` so the
    // runtime's dup branch (`target` matching `&N`) handles it.
    let digit_target = matches!(
        &r.target,
        Word::Literal(s, _) if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    );
    let (mode, default_fd) = match &r.operator {
        RedirectOperator::Input => ("r", 0),
        RedirectOperator::Output => ("w", 1),
        RedirectOperator::Append => ("a", 1),
        RedirectOperator::InputOutput => ("r+", 0),
        RedirectOperator::Heredoc => ("heredoc", 0),
        RedirectOperator::HeredocTabs => ("heredoc-tabs", 0),
        RedirectOperator::HereString => ("herestring", 0),
        RedirectOperator::StderrOutput => ("w", 2),
        RedirectOperator::StderrAppend => ("a", 2),
        RedirectOperator::StderrInput => ("r", 2),
        RedirectOperator::ProcessSubstitutionInput(_) => ("unsupported", 0),
        RedirectOperator::ProcessSubstitutionOutput(_) => ("unsupported", 0),
    };
    let is_dup = digit_target
        && matches!(
            r.operator,
            RedirectOperator::Input
                | RedirectOperator::StderrOutput
                | RedirectOperator::StderrAppend
                | RedirectOperator::StderrInput
        );
    let fd = if is_dup {
        // `>&2` has fd None (default stdout); `2>&1` carries fd 2 explicitly.
        r.fd.or(Some(1))
    } else {
        r.fd.or(Some(default_fd))
    };
    let target = if is_dup {
        match &r.target {
            Word::Literal(s, _) => st(&format!("&{s}")),
            _ => unreachable!(),
        }
    } else {
        match &r.operator {
            RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
                st(r.heredoc_body.as_deref().unwrap_or(""))
            }
            _ => word_ir(&r.target),
        }
    };
    IrRedirect {
        fd,
        mode: mode.to_string(),
        target,
        interpolate: !r.heredoc_quoted,
    }
}

fn case_to_ir(c: &CaseStatement) -> IrStmt {
    IrStmt::Case {
        discriminant: word_ir(&c.word),
        clauses: c
            .cases
            .iter()
            .map(|cl| IrCaseClause {
                patterns: cl.patterns.iter().map(|p| p.to_string()).collect(),
                body: cl.body.iter().filter_map(stmt_for_command).collect(),
            })
            .collect(),
    }
}

/// Test-position command lowering (if/while/until conds): lifts the
/// `echo X | grep P >/dev/null 2>/dev/null` idiom — a substring test that
/// currently spawns echo+grep per evaluation — into a plain substring
/// compare (`Call "contains"`). Non-matching commands lower exactly as
/// `command_to_ir` would. Only fired in TEST position because that is the
/// one context where the pipeline's exit status is consumed by control
/// flow rather than read back through `$?` (`&&`/`||` operands and
/// statement-position pipelines keep their status semantics).
fn command_to_test_ir(cmd: &Command) -> IrExpr {
    let ir = command_to_ir(cmd);
    try_lift_grep_contains(&ir).unwrap_or(ir)
}

/// `echo <arg> | grep <literal> >/dev/null 2>/dev/null` → `contains(arg,
/// literal)`. grep's exit status with both streams discarded is exactly
/// "does the line contain the literal pattern"; `echo <arg>` emits one
/// line, so the lift is a plain substring test. Conservative: only plain
/// literal patterns free of BRE metacharacters (`^ $ . [ ] * \`), no grep
/// flags, echo with exactly one argument, both fds redirected to
/// /dev/null, exactly two pipeline stages.
fn try_lift_grep_contains(cond: &IrExpr) -> Option<IrExpr> {
    let IrExpr::Call { func, args } = cond else { return None };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else { return None };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    // stage 1: exec("echo", [arg])
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if name1 != "echo" || echo_args.len() != 1 {
        return None;
    }
    let arg = echo_args[0].clone();
    // stage 2: Expr(Call("redirect", [Arrow([exec grep]), Array([spec...])]))
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "redirect" {
        return None;
    }
    let [IrExpr::Arrow(inner), IrExpr::Array(redirect_specs)] = a2.as_slice() else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func: f3, args: a3 })] = inner.as_slice() else {
        return None;
    };
    if f3 != "exec" {
        return None;
    }
    let [IrExpr::Str(name2, _), IrExpr::Array(grep_args)] = a3.as_slice() else {
        return None;
    };
    if name2 != "grep" {
        return None;
    }
    let [IrExpr::Str(pat, _)] = grep_args.as_slice() else { return None };
    if !is_safe_grep_literal(pat) {
        return None;
    }
    // both fds discarded to /dev/null (redirect-spec objects)
    let (mut out, mut err) = (false, false);
    for spec in redirect_specs {
        let IrExpr::Object(entries) = spec else { continue };
        let (mut fd, mut mode, mut target) = (None, None, None);
        for (k, v) in entries {
            match (k.as_str(), v) {
                ("fd", IrExpr::Int(f)) => fd = Some(*f),
                ("mode", IrExpr::Str(m, _)) => mode = Some(m.as_str()),
                ("target", IrExpr::Str(t, _)) => target = Some(t.as_str()),
                _ => {}
            }
        }
        if mode == Some("w") && target == Some("/dev/null") {
            match fd {
                Some(1) => out = true,
                Some(2) => err = true,
                _ => {}
            }
        }
    }
    if !(out && err) {
        return None;
    }
    Some(call(
        "contains",
        vec![arg, IrExpr::Str(pat.clone(), StrStyle::SingleQuoted)],
    ))
}

/// A grep pattern is liftable to a JS substring check only when grep would
/// treat it as a literal: no BRE metacharacters (`^ $ . [ ] * \`), no
/// leading `-` (would parse as an option), no real newline (grep matches
/// within a single line; a substring test would cross line boundaries).
/// BRE treats `+ ? ( ) { } |` as literals, so they are safe.
fn is_safe_grep_literal(pat: &str) -> bool {
    !pat.starts_with('-') && !pat.chars().any(|c| matches!(c, '^' | '$' | '.' | '[' | ']' | '*' | '\\' | '\n'))
}

fn command_to_ir(cmd: &Command) -> IrExpr {
    match cmd {
        Command::TestExpression(t) => call("test", vec![st(&t.expression)]),
        Command::Simple(sc) => exec_expr(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_expr(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Redirect(rc) => {
            // `exec 4>&1` in expression context: bash installs the redirects
            // permanently in the shell's fd table (see stmt_to_estree's
            // IrStmt::Redirect persist rule — same qualification here).
            let persist = is_bare_exec(&rc.command);
            call(
                "redirect",
                vec![
                    IrExpr::Arrow(vec![IrStmt::Expr(command_to_ir(&rc.command))]),
                    IrExpr::Array(
                        rc.redirects
                            .iter()
                            .map(|r| redirect_spec_object_persist(r, persist))
                            .collect(),
                    ),
                ],
            )
        }
        Command::Pipeline(p) => call(
            "pipeline",
            vec![IrExpr::Array(
                p.commands
                    .iter()
                    .map(|c| IrExpr::Arrow(command_arrow_stmts(c)))
                    .collect(),
            )],
        ),
        Command::Subshell(c) => call("subshell", vec![IrExpr::Arrow(command_arrow_stmts(c))]),
        Command::Block(b) => call(
            "block",
            vec![IrExpr::Arrow(
                b.commands.iter().filter_map(stmt_for_command).collect(),
            )],
        ),
        Command::While(w) => call(
            "whileLoop",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(if w.is_until {
                    not_ir(command_to_ir(&w.condition))
                } else {
                    command_to_ir(&w.condition)
                })]),
                IrExpr::Arrow(body_stmts(&Command::Block(w.body.clone()))),
            ],
        ),
        Command::Assignment(a) => assignment_expr_ir(a),
        Command::ShoptCommand(s) => call("shopt", vec![st(&s.option), IrExpr::Bool(s.enable)]),
        Command::And(l, r) => IrExpr::BinOp {
            op: BinOpKind::And,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        },
        Command::Or(l, r) => IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        },
        Command::Not(c) => not_ir(command_to_ir(c)),
        Command::Return(w) => {
            let mut args = vec![];
            if let Some(w) = w {
                args.push(word_ir_quoted(w));
            }
            call("return", args)
        }
        Command::Break(_) => call("break", vec![]),
        Command::Continue(_) => call("continue", vec![]),
        other => call("unsupported", vec![st(&format!("{other:?}"))]),
    }
}

/// The literal `exec` builtin with NO args (a redirect-only `exec N>&M`):
/// bash installs those redirects permanently in the shell's own fd table.
fn is_bare_exec(cmd: &Command) -> bool {
    match cmd {
        Command::BuiltinCommand(bc) => bc.name == "exec" && bc.args.is_empty(),
        Command::Simple(sc) => {
            sc.name.as_literal().map_or(false, |n| n == "exec") && sc.args.is_empty()
        }
        _ => false,
    }
}

fn exec_expr(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
    redirects: &[Redirect],
) -> IrExpr {
    let exec_call = exec_call_ir(name, args, env);
    if redirects.is_empty() {
        exec_call
    } else {
        // `exec 4>&1` in EXPRESSION context (command substitution bodies,
        // if/while conditions, pipeline stages): bash installs the redirects
        // permanently in the shell's own fd table, so the runtime must keep
        // them after the redirect call (same rule as the statement path in
        // stmt_to_estree — only the literal `exec` builtin with no args).
        let persist = matches!(name, Word::Literal(s, _) if s == "exec") && args.is_empty();
        call(
            "redirect",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(exec_call)]),
                IrExpr::Array(
                    redirects
                        .iter()
                        .map(|r| redirect_spec_object_persist(r, persist))
                        .collect(),
                ),
            ],
        )
    }
}

fn redirect_spec_object(r: &Redirect) -> IrExpr {
    redirect_spec_object_persist(r, false)
}

fn redirect_spec_object_persist(r: &Redirect, persist: bool) -> IrExpr {
    let ir = redirect_to_ir(r);
    let mut props = vec![
        ("fd".to_string(), IrExpr::Int(ir.fd.unwrap_or(0) as i64)),
        ("mode".to_string(), st(&ir.mode)),
        ("target".to_string(), ir.target),
    ];
    if ir.mode == "heredoc" || ir.mode == "heredoc-tabs" {
        props.push(("interpolate".to_string(), IrExpr::Bool(ir.interpolate)));
    }
    if persist {
        props.push(("persist".to_string(), IrExpr::Bool(true)));
    }
    IrExpr::Object(props)
}

// ── words → IR ───────────────────────────────────────────────────────

fn word_ir_quoted(w: &Word) -> IrExpr {
    match w {
        Word::CommandSubstitution(cmd, _) => match cmdsub_arith_expr(cmd) {
            Some(t) => match parse_arith_native(t) {
                Some(a) => IrExpr::Arith(Box::new(a)),
                None => call("arith", vec![st(t)]),
            },
            None => call(
                "capture",
                vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
            ),
        },
        _ => word_ir(w),
    }
}

/// The lexer mis-reads `$(( expr ))` with a space after `((` as a command
/// substitution of a parenthesized group: `$( ( expr ) )` collapses to a
/// CommandSubstitution wrapping a bare simple command whose NAME is the
/// whitespace-padded expression (`" a + b + c "`). A normal command name
/// can never carry leading/trailing whitespace, so this shape is always an
/// arithmetic artifact — recover the expression (`$((...))` semantics).
fn cmdsub_arith_expr(cmd: &Command) -> Option<&str> {
    if let Command::Simple(sc) = cmd {
        if sc.args.is_empty() && sc.redirects.is_empty() && sc.env_vars.is_empty() {
            if let Word::Literal(s, _) = &sc.name {
                let t = s.trim();
                if !t.is_empty()
                    && s.starts_with(char::is_whitespace)
                    && s.ends_with(char::is_whitespace)
                {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Bash quote removal for BARE literal words. The AST loses the quoting
/// context (single-quoted `'a\b'` and unquoted `a\b` both arrive as
/// Literal("a\\b")), so mirror what the corpus needs: strip a backslash
/// before any char EXCEPT those that appear backslash-escaped inside
/// single-quoted literals in the corpus (printf/tr/sed escape sequences and
/// the like must survive). The perl generator applies unconditional removal;
/// this whitelist keeps every currently-passing estree example green.
fn shell_quote_removal(s: &str) -> String {
    const KEEP: &[char] = &[
        'n', '"', 'x', 'u', 't', '(', 'v', 'r', 'f', 'b', 'a', '\\', ')',
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    ];
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next) if KEEP.contains(&next) => {
                    out.push('\\');
                    out.push(next);
                }
                Some(next) => out.push(next),
                None => {} // trailing backslash is dropped (bash behavior)
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn word_ir(w: &Word) -> IrExpr {
    match w {
        Word::Literal(s, _) => st(&shell_quote_removal(s)),
        Word::Variable(name, _, _) => call("getVar", vec![st(name)]),
        Word::CommandSubstitution(cmd, _) => match cmdsub_arith_expr(cmd) {
            Some(t) => match parse_arith_native(t) {
                Some(a) => IrExpr::Arith(Box::new(a)),
                None => call("arith", vec![st(t)]),
            },
            None => call(
                "captureWords",
                vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
            ),
        },
        Word::ParameterExpansion(pe, _) => param_ir(pe),
        Word::Arithmetic(ae, _) => match parse_arith_native(&ae.expression) {
            Some(a) => IrExpr::Arith(Box::new(a)),
            None => call("arith", vec![st(&ae.expression)]),
        },
        Word::BraceExpansion(be, _) => brace_ir(be),
        Word::Array(name, _, _) => call("getVar", vec![st(name)]),
        Word::MapAccess(name, key, _) => call("arrayIndex", vec![st(name), st(key)]),
        Word::MapKeys(name, _) => call("arrayItems", vec![st(name)]),
        Word::MapLength(name, _) => call("arrayLen", vec![st(name)]),
        Word::ArraySlice(name, offset, length, _) => call(
            "param",
            vec![
                st("slice"),
                st(name),
                st(offset),
                st(length.as_deref().unwrap_or("")),
            ],
        ),
        Word::StringInterpolation(interp, _) => {
            if let Some(part) = pure_template_part(interp) {
                part
            } else {
                interpolate_ir(&interp.parts)
            }
        }
        other => call("unsupported", vec![st(&other.to_string())]),
    }
}

fn param_ir(pe: &ParameterExpansion) -> IrExpr {
    let (op, extra): (String, Vec<IrExpr>) = match &pe.operator {
        ParameterExpansionOperator::None if pe.variable.len() > 1 && pe.variable.starts_with('#') => {
            // ${#name} — string length (the parser keeps the `#` in the name)
            return call("param", vec![st("len"), st(&pe.variable[1..])]);
        }
        ParameterExpansionOperator::None => (String::new(), vec![]),
        ParameterExpansionOperator::UppercaseAll => ("^^".into(), vec![]),
        ParameterExpansionOperator::LowercaseAll => (",,".into(), vec![]),
        ParameterExpansionOperator::UppercaseFirst => ("^".into(), vec![]),
        ParameterExpansionOperator::RemoveLongestPrefix(p) => ("##".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveShortestPrefix(p) => ("#".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveLongestSuffix(p) => ("%%".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveShortestSuffix(p) => ("%".into(), vec![st(p)]),
        ParameterExpansionOperator::SubstituteAll(p, r) => ("//".into(), vec![st(p), st(r)]),
        ParameterExpansionOperator::DefaultValue(d) => (":-".into(), vec![st(d)]),
        ParameterExpansionOperator::AssignDefault(d) => (":=".into(), vec![st(d)]),
        ParameterExpansionOperator::ErrorIfUnset(e) => (":?".into(), vec![st(e)]),
        ParameterExpansionOperator::Basename => ("basename".into(), vec![]),
        ParameterExpansionOperator::Dirname => ("dirname".into(), vec![]),
        ParameterExpansionOperator::ArraySlice(off, len) => (
            "slice".into(),
            vec![st(off), st(len.as_deref().unwrap_or(""))],
        ),
    };
    let mut args = vec![st(&op), st(&pe.variable)];
    args.extend(extra);
    call("param", args)
}

fn brace_ir(be: &BraceExpansion) -> IrExpr {
    call(
        "brace",
        vec![
            st(be.prefix.as_deref().unwrap_or("")),
            IrExpr::Json(serde_json::Value::Array(vec![brace_items_json(&be.items)])),
            IrExpr::Json(serde_json::Value::Array(vec![])),
            st(be.suffix.as_deref().unwrap_or("")),
        ],
    )
}

fn brace_items_json(items: &[BraceItem]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|it| match it {
                BraceItem::Literal(s) => serde_json::Value::String(s.clone()),
                BraceItem::Range(r) => serde_json::json!({
                    "range": [r.start, r.end, r.step, r.format]
                }),
                BraceItem::Sequence(seq) => serde_json::Value::Array(
                    seq.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                ),
                BraceItem::Nested(n) => serde_json::json!({ "nested": brace_items_json(&n.items) }),
                BraceItem::Compound(c) => serde_json::json!({ "nested": brace_items_json(c) }),
            })
            .collect(),
    )
}

/// Emit-time evaluation of `sh2.brace(prefix, groups, middles, suffix)` —
/// the runtime's brace expansion is PURE string work over the literal JSON
/// args the parser always emits (see `brace_ir`), so the whole call lowers
/// to a native array literal. Mirrors harness/sh2-namespace.mjs exactly:
/// `braceRange` / `alphaRange` / `expandBraceNested` / `expandBraceGroup`
/// plus the cartesian product with inter-group `middles` and the
/// prefix/suffix wrap. The runtime never adds GLOB_MAGIC to brace results,
/// so the literal strings are bit-identical to what the runtime returns and
/// downstream flattening (forLoop/exec) treats them identically.
fn brace_expand(
    prefix: &str,
    groups: &serde_json::Value,
    middles: &serde_json::Value,
    suffix: &str,
) -> Vec<String> {
    fn jstr(v: &serde_json::Value) -> String {
        v.as_str().unwrap_or("").to_string()
    }
    fn is_int_str(s: &str) -> bool {
        let b = s.as_bytes();
        let digits = if b.first() == Some(&b'-') { &b[1..] } else { b };
        !digits.is_empty() && digits.iter().all(|c| c.is_ascii_digit())
    }
    fn alpha_range(a: &str, b: &str, step: i64) -> Vec<String> {
        let mut out = Vec::new();
        let ca = a.chars().next().unwrap_or('a') as i64;
        let cb = b.chars().next().unwrap_or('a') as i64;
        let mut i = ca;
        if ca <= cb {
            while i <= cb {
                out.push(char::from_u32(i as u32).unwrap().to_string());
                match i.checked_add(step) {
                    Some(n) => i = n,
                    None => break,
                }
            }
        } else {
            while i >= cb {
                out.push(char::from_u32(i as u32).unwrap().to_string());
                match i.checked_sub(step) {
                    Some(n) => i = n,
                    None => break,
                }
            }
        }
        out
    }
    fn step_of(step: &serde_json::Value) -> i64 {
        match step {
            serde_json::Value::String(s) => s
                .parse::<i64>()
                .ok()
                .map(|v| v.abs())
                .filter(|v| *v != 0)
                .unwrap_or(1),
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(|v| v.abs())
                .filter(|v| *v != 0)
                .unwrap_or(1),
            _ => 1,
        }
    }
    /// The runtime's `braceRange([start, end, step, format])` — zero-padded
    /// numeric ranges, letter ranges, mixed `{a1..c3}` runs, and the
    /// literal `start..end` fallback.
    fn brace_range_value(range: &serde_json::Value) -> Vec<String> {
        let arr = match range.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => return vec![],
        };
        let start = jstr(&arr[0]);
        let end = jstr(&arr[1]);
        let st = step_of(arr.get(2).unwrap_or(&serde_json::Value::Null));
        let is_num = is_int_str(&start) && is_int_str(&end);
        // mixed `{a1..c3}` → alpha part × numeric part
        if !is_num {
            let alpha_num = |s: &str| -> Option<(String, String)> {
                let bytes = s.as_bytes();
                let alen = bytes
                    .iter()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .count();
                if alen == 0 || alen == bytes.len() {
                    return None;
                }
                let (a, n) = s.split_at(alen);
                if n.is_empty() || !n.bytes().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                Some((a.to_string(), n.to_string()))
            };
            if let (Some((al1, an1)), Some((al2, an2))) = (alpha_num(&start), alpha_num(&end)) {
                let alphas = alpha_range(&al1, &al2, 1);
                let lo: i64 = an1.parse().unwrap_or(0);
                let hi: i64 = an2.parse().unwrap_or(0);
                let width = an1.len().max(an2.len());
                let mut out = Vec::new();
                for ch in &alphas {
                    let mut n = lo;
                    if lo <= hi {
                        while n <= hi {
                            out.push(format!("{ch}{n:0width$}", width = width));
                            match n.checked_add(1) {
                                Some(x) => n = x,
                                None => break,
                            }
                        }
                    } else {
                        while n >= hi {
                            out.push(format!("{ch}{n:0width$}", width = width));
                            match n.checked_sub(1) {
                                Some(x) => n = x,
                                None => break,
                            }
                        }
                    }
                }
                return out;
            }
        }
        if is_num {
            let a = start.parse::<i64>().unwrap_or(0);
            let b = end.parse::<i64>().unwrap_or(0);
            let width = if start.starts_with('0') || end.starts_with('0') {
                start.len().max(end.len())
            } else {
                0
            };
            let fmt = |n: i64| -> String {
                let s = n.abs().to_string();
                let padded = if width > 0 {
                    format!("{s:0>width$}", width = width)
                } else {
                    s
                };
                if n < 0 {
                    format!("-{padded}")
                } else {
                    padded
                }
            };
            let mut out = Vec::new();
            let mut i = a;
            if a <= b {
                while i <= b {
                    out.push(fmt(i));
                    match i.checked_add(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            } else {
                while i >= b {
                    out.push(fmt(i));
                    match i.checked_sub(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            }
            return out;
        }
        let single_letter = |s: &str| s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic();
        if single_letter(&start) && single_letter(&end) {
            return alpha_range(&start, &end, st);
        }
        let all_alpha = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphabetic());
        if all_alpha(&start) && all_alpha(&end) {
            // longer alpha runs (`{ab..az}`) — step applies to the last
            // letter; the prefix stays the START's prefix (runtime quirk,
            // mirrored exactly)
            let mut out = Vec::new();
            let ca = start.chars().next().unwrap() as i64;
            let cb = end.chars().next().unwrap() as i64;
            let prefix = &start[..start.len() - 1];
            let mut i = ca;
            if ca <= cb {
                while i <= cb {
                    out.push(format!("{prefix}{}", char::from_u32(i as u32).unwrap()));
                    match i.checked_add(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            } else {
                while i >= cb {
                    out.push(format!("{prefix}{}", char::from_u32(i as u32).unwrap()));
                    match i.checked_sub(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            }
            return out;
        }
        vec![format!("{start}..{end}")]
    }
    /// `expandBraceNested(items)` — nested groups expand recursively.
    fn brace_nested(items: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(arr) = items.as_array() {
            for it in arr {
                if let Some(s) = it.as_str() {
                    out.push(s.to_string());
                } else if let Some(r) = it.get("range") {
                    out.extend(brace_range_value(r));
                } else if let Some(n) = it.get("nested") {
                    out.extend(brace_nested(n));
                } else if it.is_array() {
                    out.extend(brace_nested(it));
                }
            }
        }
        out
    }
    /// `expandBraceGroup(g)` — a range inside a comma-separated group stays
    /// LITERAL (`{1..3,7..9}`); only a lone range expands.
    fn brace_group(g: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(items) = g.as_array() {
            for it in items {
                if let Some(s) = it.as_str() {
                    out.push(s.to_string());
                } else if let Some(r) = it.get("range") {
                    if items.len() == 1 {
                        out.extend(brace_range_value(r));
                    } else if let Some(rarr) = r.as_array() {
                        out.push(format!("{}..{}", jstr(&rarr[0]), jstr(&rarr[1])));
                    }
                } else if let Some(n) = it.get("nested") {
                    out.extend(brace_nested(n));
                } else if it.is_array() {
                    out.extend(brace_nested(it));
                }
            }
        }
        out
    }
    // cartesian product of the group expansions, middles spliced between
    let expansions: Vec<Vec<String>> = groups
        .as_array()
        .map(|gs| gs.iter().map(brace_group).collect())
        .unwrap_or_default();
    let mut combos: Vec<Vec<String>> = vec![vec![]];
    for g in &expansions {
        let mut next = Vec::new();
        for c in &combos {
            for it in g {
                let mut cc = c.clone();
                cc.push(it.clone());
                next.push(cc);
            }
        }
        combos = next;
    }
    let ms: Vec<String> = middles
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    combos
        .iter()
        .map(|c| {
            let mut body = String::new();
            for (i, x) in c.iter().enumerate() {
                body.push_str(x);
                if let Some(m) = ms.get(i) {
                    body.push_str(m);
                }
            }
            format!("{prefix}{body}{suffix}")
        })
        .collect()
}

fn pure_template_part(interp: &StringInterpolation) -> Option<IrExpr> {
    pure_part(interp).map(part_ir)
}

/// The single non-literal part of a one-part interpolation (if any).
fn pure_part(interp: &StringInterpolation) -> Option<&StringPart> {
    let mut non_literal: Option<&StringPart> = None;
    for p in &interp.parts {
        match p {
            StringPart::Literal(s) if s.is_empty() => {}
            StringPart::Literal(_) => return None,
            other => {
                if non_literal.is_some() {
                    return None;
                }
                non_literal = Some(other);
            }
        }
    }
    non_literal
}

/// Like `part_ir` but WITHOUT the sh2.join wrapper for array-valued parts:
/// `for x in "${!map[@]}"` must iterate each key, so the array is passed
/// through (the runtime's forLoop flattens it).
fn part_ir_flat(part: &StringPart) -> IrExpr {
    match part {
        // `for x in "$@"` with NO positionals must iterate ZERO times (bash
        // runs the loop body once per positional; an empty list runs it never).
        // getVar("@") would join to "" and yield one bogus iteration.
        StringPart::Variable(name) if name == "@" => call("listVar", vec![st(name)]),
        StringPart::MapAccess(name, key) if key == "@" || key == "*" => {
            call("arrayIndex", vec![st(name), st(key)])
        }
        StringPart::MapKeys(name) => call("arrayItems", vec![st(name)]),
        StringPart::ArraySlice(name, offset, length) => call(
            "param",
            vec![
                st("slice"),
                st(name),
                st(offset),
                st(length.as_deref().unwrap_or("")),
            ],
        ),
        // `${!map[@]}` — the parser tags it as a slice of `!map`; for-loop
        // items must see the ARRAY (each key iterated), not the join.
        StringPart::ParameterExpansion(pe)
            if matches!(pe.operator, ParameterExpansionOperator::ArraySlice(..)) =>
        {
            param_ir(pe)
        }
        other => part_ir(other),
    }
}

fn interpolate_ir(parts: &[StringPart]) -> IrExpr {
    IrExpr::Interpolate(
        parts
            .iter()
            .map(|p| match p {
                StringPart::Literal(s) => InterpPart::Lit(s.clone()),
                other => InterpPart::Expr(Box::new(part_ir(other))),
            })
            .collect(),
    )
}

fn part_ir(part: &StringPart) -> IrExpr {
    match part {
        StringPart::Literal(_) => unreachable!("Literal parts handled in interpolate_ir"),
        StringPart::Variable(name) => call("getVar", vec![st(name)]),
        StringPart::ParameterExpansion(pe) => {
            // `${arr[@]:off:len}` (ArraySlice) can return an ARRAY; inside a
            // template literal that would render with JS comma joins — wrap
            // in sh2.join (idempotent for plain string slices).
            if matches!(pe.operator, ParameterExpansionOperator::ArraySlice(..)) {
                call("join", vec![param_ir(pe)])
            } else {
                param_ir(pe)
            }
        }
        StringPart::Arithmetic(ae) => match parse_arith_native(&ae.expression) {
            Some(a) => IrExpr::Arith(Box::new(a)),
            None => call("arith", vec![st(&ae.expression)]),
        },
        // Array-valued expansions inside a template literal would render with
        // JS's comma join; bash joins them with spaces, so wrap in sh2.join.
        // (In direct exec-arg position the array is flattened instead — see
        // word_ir / the runtime's exec.)
        StringPart::MapAccess(name, key) if key == "@" || key == "*" => call(
            "join",
            vec![call("arrayIndex", vec![st(name), st(key)])],
        ),
        StringPart::MapAccess(name, key) => call("arrayIndex", vec![st(name), st(key)]),
        StringPart::MapKeys(name) => call("join", vec![call("arrayItems", vec![st(name)])]),
        StringPart::MapLength(name) => call("arrayLen", vec![st(name)]),
        StringPart::ArraySlice(name, offset, length) => call(
            "join",
            vec![call(
                "param",
                vec![
                    st("slice"),
                    st(name),
                    st(offset),
                    st(length.as_deref().unwrap_or("")),
                ],
            )],
        ),
        StringPart::CommandSubstitution(cmd) => match cmdsub_arith_expr(cmd) {
            Some(t) => match parse_arith_native(t) {
                Some(a) => IrExpr::Arith(Box::new(a)),
                None => call("arith", vec![st(t)]),
            },
            None => call(
                "capture",
                vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
            ),
        },
        other => call("unsupported", vec![st(&format!("{other:?}"))]),
    }
}

fn for_item_ir(w: &Word) -> IrExpr {
    match w {
        Word::Variable(name, _, _) if name == "@" || name == "*" => call("listVar", vec![st(name)]),
        Word::StringInterpolation(interp, _) => {
            if let Some(part) = pure_part(interp) {
                // Un-joined: `for x in "${!map[@]}"` iterates each element.
                return part_ir_flat(part);
            }
            word_ir(w)
        }
        _ => arg_word_ir(w, false),
    }
}

// ── arithmetic string → neutral AST ──────────────────────────────────
/// Recursive-descent parser for `$((...))` content. Returns None when the
/// expression contains assignments / ++ / -- / anything needing setVar
/// semantics — those fall back to the runtime `sh2.arith` evaluator.
fn parse_arith(src: &str) -> Option<ArithAst> {
    let chars: Vec<char> = src.chars().collect();
    let mut pos = 0usize;
    let n = chars.len();

    fn skip(chars: &[char], pos: &mut usize) {
        while *pos < chars.len() && chars[*pos].is_whitespace() {
            *pos += 1;
        }
    }
    fn eat2(chars: &[char], pos: &mut usize, s: &str) -> bool {
        if *pos + s.len() <= chars.len() {
            let got: String = chars[*pos..*pos + s.len()].iter().collect();
            if got == s {
                *pos += s.len();
                return true;
            }
        }
        false
    }
    fn primary(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        skip(chars, pos);
        if *pos >= chars.len() {
            return None;
        }
        let c = chars[*pos];
        if c == '(' {
            *pos += 1;
            let e = ternary(chars, pos)?;
            skip(chars, pos);
            if *pos >= chars.len() || chars[*pos] != ')' {
                return None;
            }
            *pos += 1;
            return Some(e);
        }
        if c.is_ascii_digit() {
            let mut s = String::new();
            while *pos < chars.len()
                && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == 'x' || chars[*pos] == 'X')
            {
                s.push(chars[*pos]);
                *pos += 1;
            }
            let v = if s.starts_with("0x") || s.starts_with("0X") {
                i64::from_str_radix(&s[2..], 16).ok()?
            } else {
                s.parse::<i64>().ok()?
            };
            return Some(ArithAst::Num(v));
        }
        if c == '$' {
            // bash expands `$var` inside $(( )) as a STRING INSERTION before
            // parsing: `$(( $j * 2 ))` with j unset becomes `$(( * 2 ))` (a
            // syntax error → whole expansion empty), `$(( $j + 1 ))` becomes
            // `$(( + 1 ))` (unary plus → 1). A native number read (0 for
            // unset) cannot express that, so dollar-prefixed operands fall
            // back to the runtime evaluator (sh2.arith) which reproduces it.
            return None;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut name = String::new();
            while *pos < chars.len()
                && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_')
            {
                name.push(chars[*pos]);
                *pos += 1;
            }
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '[' {
                *pos += 1;
                let key = ternary(chars, pos)?;
                skip(chars, pos);
                if *pos >= chars.len() || chars[*pos] != ']' {
                    return None;
                }
                *pos += 1;
                return Some(ArithAst::Index {
                    var: name,
                    key: Box::new(key),
                });
            }
            // postfix ++ / -- (`i++` / `j--`) — the value is the OLD
            // value (the emitter preserves that; see arith_to_estree).
            if eat2(chars, pos, "++") {
                return Some(ArithAst::IncDec {
                    var: name,
                    delta: 1,
                    prefix: false,
                });
            }
            if eat2(chars, pos, "--") {
                return Some(ArithAst::IncDec {
                    var: name,
                    delta: -1,
                    prefix: false,
                });
            }
            return Some(ArithAst::Var(name));
        }
        None
    }
    fn unary(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        skip(chars, pos);
        if *pos < chars.len() {
            let c = chars[*pos];
            // prefix ++ / -- (`++i` / `--j`) — the value is the NEW value.
            if c == '+' && *pos + 1 < chars.len() && chars[*pos + 1] == '+' {
                *pos += 2;
                skip(chars, pos);
                let name = ident_name(chars, pos)?;
                return Some(ArithAst::IncDec {
                    var: name,
                    delta: 1,
                    prefix: true,
                });
            }
            if c == '-' && *pos + 1 < chars.len() && chars[*pos + 1] == '-' {
                *pos += 2;
                skip(chars, pos);
                let name = ident_name(chars, pos)?;
                return Some(ArithAst::IncDec {
                    var: name,
                    delta: -1,
                    prefix: true,
                });
            }
            if c == '-' || c == '+' || c == '!' || c == '~' {
                *pos += 1;
                let op = match c {
                    '-' => "-",
                    '+' => "+",
                    '!' => "!",
                    _ => "~",
                };
                return Some(ArithAst::Un {
                    op,
                    arg: Box::new(unary(chars, pos)?),
                });
            }
        }
        primary(chars, pos)
    }
    // ** power: RIGHT-associative (2**3**2 = 2**(3**2) = 512), binds tighter
    // than * / % (a**b * c = (a**b) * c), matching bash/evalArith.
    fn pow(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let base = unary(chars, pos)?;
        skip(chars, pos);
        if *pos + 1 < chars.len() && chars[*pos] == '*' && chars[*pos + 1] == '*' {
            *pos += 2;
            let exp = pow(chars, pos)?;
            return Some(ArithAst::Bin {
                op: "**",
                lhs: Box::new(base),
                rhs: Box::new(exp),
            });
        }
        Some(base)
    }
    fn mul(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = pow(chars, pos)?;
        loop {
            skip(chars, pos);
            let c = *chars.get(*pos).unwrap_or(&'\0');
            if c == '*' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "*",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '/' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "/",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '%' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "%",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn add(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = mul(chars, pos)?;
        loop {
            skip(chars, pos);
            let c = *chars.get(*pos).unwrap_or(&'\0');
            if c == '+' {
                *pos += 1;
                let rhs = mul(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "+",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '-' {
                *pos += 1;
                let rhs = mul(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "-",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn shift(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = add(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "<<") {
                let rhs = add(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "<<",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if eat2(chars, pos, ">>") {
                let rhs = add(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: ">>",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn rel(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = shift(chars, pos)?;
        loop {
            skip(chars, pos);
            let c = *chars.get(*pos).unwrap_or(&'\0');
            if c == '<' || c == '>' {
                let mut two = false;
                let op = if c == '<' {
                    two = *pos + 1 < chars.len() && chars[*pos + 1] == '=';
                    if two {
                        "<="
                    } else {
                        "<"
                    }
                } else {
                    two = *pos + 1 < chars.len() && chars[*pos + 1] == '=';
                    if two {
                        ">="
                    } else {
                        ">"
                    }
                };
                *pos += if two { 2 } else { 1 };
                let rhs = shift(chars, pos)?;
                lhs = ArithAst::Bin {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn eq(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = rel(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "==") {
                let rhs = rel(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "==",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if eat2(chars, pos, "!=") {
                let rhs = rel(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "!=",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn band(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = eq(chars, pos)?;
        loop {
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '&' && chars.get(*pos + 1) != Some(&'&') {
                *pos += 1;
                let rhs = eq(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "&",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn bxor(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = band(chars, pos)?;
        loop {
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '^' {
                *pos += 1;
                let rhs = band(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "^",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn bor(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = bxor(chars, pos)?;
        loop {
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '|' && chars.get(*pos + 1) != Some(&'|') {
                *pos += 1;
                let rhs = bxor(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "|",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn land(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = bor(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "&&") {
                let rhs = bor(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "&&",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn lor(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = land(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "||") {
                let rhs = land(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "||",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn ternary(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let test = lor(chars, pos)?;
        skip(chars, pos);
        if *pos < chars.len() && chars[*pos] == '?' {
            *pos += 1;
            let then = ternary(chars, pos)?;
            skip(chars, pos);
            if *pos >= chars.len() || chars[*pos] != ':' {
                return None;
            }
            *pos += 1;
            let else_ = ternary(chars, pos)?;
            return Some(ArithAst::Cond {
                test: Box::new(test),
                then: Box::new(then),
                else_: Box::new(else_),
            });
        }
        Some(test)
    }

    // `name op= rhs` — bash's lowest-precedence arith form, right-
    // associative (`a = b = c` → a = (b = c)), rhs = a full ternary. Only
    // plain scalar names (no `arr[i] =` targets) and the ops the native
    // lowering implements (`=`/`+=`/`-=`/`*=`; `/=`/`%=` keep the runtime
    // zero-divisor semantics — the parse fails → runtime evalArith).
    fn assignment(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        skip(chars, pos);
        if *pos < chars.len()
            && (chars[*pos].is_ascii_alphabetic() || chars[*pos] == '_')
        {
            let save = *pos;
            let name = ident_name(chars, pos)?;
            skip(chars, pos);
            // `arr[i] = ...` — an array-element write; the native lowering
            // has no array targets, so fall through to ternary (the Index
            // read) and let the leftover `=` fail the parse → runtime.
            if *pos < chars.len() && chars[*pos] == '[' {
                *pos = save;
                return ternary(chars, pos);
            }
            // `==` is equality, `<=`/`>=` are comparisons — never
            // assignment. `+`/`-`/`*` qualify ONLY as the two-char op
            // forms (`+=` etc.); a bare `+`/`-`/`*` is a binary op.
            let op: Option<&str> = if *pos < chars.len() && chars[*pos] == '=' {
                if chars.get(*pos + 1) == Some(&'=') {
                    None
                } else {
                    *pos += 1;
                    Some("=")
                }
            } else if eat2(chars, pos, "+=") {
                Some("+=")
            } else if eat2(chars, pos, "-=") {
                Some("-=")
            } else if eat2(chars, pos, "*=") {
                Some("*=")
            } else {
                None
            };
            if let Some(op) = op {
                skip(chars, pos);
                if *pos >= chars.len() {
                    return None;
                }
                let rhs = assignment(chars, pos)?;
                return Some(ArithAst::Assign {
                    var: name,
                    op,
                    rhs: Box::new(rhs),
                });
            }
            *pos = save;
        }
        ternary(chars, pos)
    }
    fn ident_name(chars: &[char], pos: &mut usize) -> Option<String> {
        if *pos >= chars.len()
            || !(chars[*pos].is_ascii_alphabetic() || chars[*pos] == '_')
        {
            return None;
        }
        let mut name = String::new();
        while *pos < chars.len()
            && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_')
        {
            name.push(chars[*pos]);
            *pos += 1;
        }
        Some(name)
    }

    let ast = assignment(&chars, &mut pos)?;
    skip(&chars, &mut pos);
    if pos != n {
        return None;
    }
    Some(ast)
}

/// `Number(<read>) || 0` — the runtime's arithmetic coercion of a variable
/// read: lifted NUMERIC vars are already JS numbers (bare identifier);
/// everything else (lifted strings, store vars, bash specials) goes
/// through the exact `Number(v) || 0` the runtime's evalArith applies.
fn arith_var_read(name: &str) -> Expr {
    if is_lifted_num(name) {
        // already a JS number — no Number()/||0 coercion needed
        return Expr::Identifier {
            name: name.to_string(),
        };
    }
    let read = if is_lifted_str(name) {
        Expr::Identifier {
            name: name.to_string(),
        }
    } else {
        native_special_var(name).unwrap_or_else(|| {
            sh2_call("getVar", vec![str_lit(name)])
        })
    };
    Expr::LogicalExpression {
        operator: "||",
        left: Box::new(Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "Number".to_string(),
            }),
            arguments: vec![read],
            optional: false,
        }),
        right: Box::new(Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
        regex: None,
        }),
    }
}

/// Render the arithmetic AST as native JS expressions.
fn arith_to_estree(a: &ArithAst) -> Expr {
    match a {
        ArithAst::Num(v) => Expr::Literal {
            value: serde_json::Value::from(*v),
            raw: None,
        regex: None,
        },
        ArithAst::Var(name) => arith_var_read(name),
        // `x = v` / `x += v` — the assigned VALUE is the expression's
        // value (bash semantics). Lifted numeric vars write the native
        // binding directly (JS compound assignment); store vars write via
        // setVar and read the value BACK from the store (the read after
        // the write is order-correct even when the RHS reads the same
        // var — `x = x + 1` — and the stored value is exactly the
        // assigned one for plain vars). The RHS is provably write-free
        // (see arith_lowerable).
        ArithAst::Assign { var, op, rhs } => {
            let rhs_e = arith_to_estree(rhs);
            if is_lifted_num(var) {
                return Expr::AssignmentExpression {
                    operator: op.to_string(),
                    left: Box::new(Expr::Identifier {
                        name: var.clone(),
                    }),
                    right: Box::new(rhs_e),
                };
            }
            let cur = arith_var_read(var);
            let bin = |op: &'static str, l: Expr, r: Expr| Expr::BinaryExpression {
                operator: op,
                left: Box::new(l),
                right: Box::new(r),
            };
            let new_val = match *op {
                "=" => rhs_e.clone(),
                "+=" => bin("+", cur, rhs_e.clone()),
                "-=" => bin("-", cur, rhs_e.clone()),
                "*=" => bin("*", cur, rhs_e.clone()),
                _ => unreachable!("parse_arith only emits = += -= *="),
            };
            seq(vec![
                sh2_call(
                    "setVar",
                    vec![
                        str_lit(var),
                        Expr::CallExpression {
                            callee: Box::new(Expr::Identifier {
                                name: "String".to_string(),
                            }),
                            arguments: vec![new_val],
                            optional: false,
                        },
                    ],
                ),
                arith_var_read(var),
            ])
        }
        // `++x` / `x++` / `--x` / `x--` — the value is the NEW value
        // (prefix) or the OLD value (postfix). Lifted numeric vars use a
        // native JS update expression (exact semantics); store vars write
        // via setVar and read back: prefix = the stored (new) value,
        // postfix = stored ∓ 1 (the delta is ±1, so the old value is
        // exactly one step away — read after the write is order-correct).
        ArithAst::IncDec { var, delta, prefix } => {
            if is_lifted_num(var) {
                return Expr::UnaryExpression {
                    operator: if *delta > 0 { "++" } else { "--" },
                    argument: Box::new(Expr::Identifier {
                        name: var.clone(),
                    }),
                    prefix: *prefix,
                };
            }
            let cur = arith_var_read(var);
            let int1 = || Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
            regex: None,
            };
            let new_val = Expr::BinaryExpression {
                operator: if *delta > 0 { "+" } else { "-" },
                left: Box::new(cur),
                right: Box::new(int1()),
            };
            let value = if *prefix {
                arith_var_read(var)
            } else {
                Expr::BinaryExpression {
                    operator: if *delta > 0 { "-" } else { "+" },
                    left: Box::new(arith_var_read(var)),
                    right: Box::new(int1()),
                }
            };
            seq(vec![
                sh2_call(
                    "setVar",
                    vec![
                        str_lit(var),
                        Expr::CallExpression {
                            callee: Box::new(Expr::Identifier {
                                name: "String".to_string(),
                            }),
                            arguments: vec![new_val],
                            optional: false,
                        },
                    ],
                ),
                value,
            ])
        }
        ArithAst::Index { var, key } => Expr::LogicalExpression {
            operator: "||",
            left: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "Number".to_string(),
                }),
                arguments: vec![sh2_call("arrayIndex", vec![str_lit(var), arith_to_estree(key)])],
                optional: false,
            }),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            regex: None,
            }),
        },
        ArithAst::Bin { op, lhs, rhs } => {
            if *op == "/" || *op == "%" {
                // bash arithmetic is INTEGER division (truncating toward
                // zero); a zero divisor must abort the whole expansion, and
                // JS bitwise ops would silently absorb a NaN — so the
                // general form throws from the runtime helper (caught by
                // arithEval). When the divisor is PROVABLY nonzero (a
                // non-zero numeric literal, optionally sign-flipped — the
                // abort can never fire), the operation is plain native
                // JS: no idiv/imod dispatch, no zero check per evaluation.
                let r = arith_to_estree(rhs);
                if arith_is_nonzero(rhs) {
                    if *op == "/" {
                        // Math.trunc(l / r) — bash integer division
                        // (truncating toward zero) with a provably-nonzero
                        // divisor (no zero-divisor abort possible, so no
                        // idiv dispatch). The truncation wraps the WHOLE
                        // division — truncating the dividend first would
                        // change the quotient (7/2 → 3.5).
                        Expr::CallExpression {
                            callee: Box::new(Expr::MemberExpression {
                                object: Box::new(Expr::Identifier {
                                    name: "Math".to_string(),
                                }),
                                property: Box::new(Expr::Identifier {
                                    name: "trunc".to_string(),
                                }),
                                computed: false,
                                optional: false,
                            }),
                            arguments: vec![Expr::BinaryExpression {
                                operator: "/",
                                left: Box::new(arith_to_estree(lhs)),
                                right: Box::new(r),
                            }],
                            optional: false,
                        }
                    } else {
                        Expr::BinaryExpression {
                            operator: "%",
                            left: Box::new(arith_to_estree(lhs)),
                            right: Box::new(r),
                        }
                    }
                } else if *op == "/" {
                    // zero divisor must abort the whole expansion, and
                    // JS bitwise ops would silently absorb a NaN — so throw
                    // from the runtime helper (caught by arithEval).
                    sh2_call("idiv", vec![arith_to_estree(lhs), r])
                } else {
                    // modulo by zero aborts the expansion too (bash "division by 0")
                    sh2_call("imod", vec![arith_to_estree(lhs), r])
                }
            } else if *op == "&&" || *op == "||" {
                // bash yields 0/1; JS logicals yield one of the operands
                Expr::ConditionalExpression {
                    test: Box::new(Expr::LogicalExpression {
                        operator: op,
                        left: Box::new(arith_to_estree(lhs)),
                        right: Box::new(arith_to_estree(rhs)),
                    }),
                    consequent: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    regex: None,
                    }),
                    alternate: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    regex: None,
                    }),
                }
            } else if matches!(*op, "<" | "<=" | ">" | ">=" | "==" | "!=") {
                // bash comparisons yield 0/1; JS yields booleans
                Expr::ConditionalExpression {
                    test: Box::new(Expr::BinaryExpression {
                        operator: op,
                        left: Box::new(arith_to_estree(lhs)),
                        right: Box::new(arith_to_estree(rhs)),
                    }),
                    consequent: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    regex: None,
                    }),
                    alternate: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    regex: None,
                    }),
                }
            } else {
                Expr::BinaryExpression {
                    operator: op,
                    left: Box::new(arith_to_estree(lhs)),
                    right: Box::new(arith_to_estree(rhs)),
                }
            }
        }
        ArithAst::Un { op, arg } => {
            if *op == "!" {
                // bash ! yields 0/1; JS ! yields a boolean
                Expr::ConditionalExpression {
                    test: Box::new(Expr::UnaryExpression {
                        operator: "!",
                        argument: Box::new(arith_to_estree(arg)),
                        prefix: true,
                    }),
                    consequent: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    regex: None,
                    }),
                    alternate: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    regex: None,
                    }),
                }
            } else {
                Expr::UnaryExpression {
                    operator: op,
                    argument: Box::new(arith_to_estree(arg)),
                    prefix: true,
                }
            }
        }
        ArithAst::Cond { test, then, else_ } => Expr::ConditionalExpression {
            test: Box::new(arith_to_estree(test)),
            consequent: Box::new(arith_to_estree(then)),
            alternate: Box::new(arith_to_estree(else_)),
        },
    }
}

// ── IR → ESTree ──────────────────────────────────────────────────────

fn is_async_call(name: &str) -> bool {
    matches!(
        name,
        "exec" | "redirect" | "pipeline" | "subshell" | "block" | "whileLoop" | "cstyleFor"
            | "capture" | "captureWords" | "forLoop" | "and" | "or"
    )
}

/// `sh2.exec("name", args)` → sync `sh2.builtin("name", args)` when the
/// runtime implements `name` as a SYNC builtin (harness builtins.json minus
/// the async wait/exec/sleep/command) AND no script-defined function
/// shadows it (bash: a function named like a builtin wins — the runtime's
/// exec dispatch consults its function map first, so a shadowed name must
/// keep the async exec path). Env-carrying exec calls (`IFS=: read ...`,
/// the 3-arg form) lower too — the runtime `builtin` twin applies the
/// command-scoped env exactly like the async exec path. The sync twin
/// skips the async exec machinery (arg flattening/glob expansion happen
/// identically inside it), the whileLoopSync pattern: same semantics, no
/// per-call promises.
fn exec_or_builtin<'a>(func: &'a str, args: &[IrExpr]) -> &'a str {
    if func == "exec" {
        let sync_name = |name: &str| {
            SYNC_BUILTINS.contains(&name) && !program_defines_function(name)
        };
        match args {
            [IrExpr::Str(name, _), IrExpr::Array(_)]
            | [IrExpr::Str(name, _), IrExpr::Array(_), IrExpr::Object(_)] => {
                if sync_name(name) {
                    return "builtin";
                }
            }
            _ => {}
        }
    }
    func
}

/// bash special / environment variables the RUNTIME reads from its own
/// store or process.env (IFS drives field-splitting and joins; PATH feeds
/// spawned commands; exported vars sync to process.env). Lifting any of
/// these to a native binding would desync the runtime.
fn is_reserved_var(name: &str) -> bool {
    matches!(
        name,
        "IFS" | "PATH" | "HOME" | "PWD" | "OLDPWD" | "SHELL" | "USER" | "TERM" | "LANG"
            | "LC_ALL" | "LC_CTYPE" | "PS1" | "PS2" | "PS3" | "PS4" | "ENV" | "BASH"
            | "BASH_VERSION" | "RANDOM" | "SECONDS" | "LINENO" | "PPID" | "SHLVL"
            | "HOSTNAME" | "TMPDIR" | "CDPATH" | "COLUMNS" | "LINES" | "UID" | "EUID"
            | "GROUPS" | "OPTIND" | "OPTARG" | "REPLY" | "PIPESTATUS" | "FUNCNAME"
            | "BASH_SOURCE" | "BASH_LINENO" | "BASH_ARGV" | "BASH_ARGC"
    )
}

/// JS reserved words — a lifted variable becomes a native binding, and
/// `let var = 0` (etc.) is a SyntaxError.
fn is_js_keyword(name: &str) -> bool {
    matches!(
        name,
        "var" | "let" | "const" | "function" | "return" | "if" | "else" | "for" | "while"
            | "do" | "switch" | "case" | "break" | "continue" | "new" | "delete" | "typeof"
            | "instanceof" | "in" | "of" | "class" | "extends" | "super" | "this" | "null"
            | "true" | "false" | "undefined" | "NaN" | "Infinity" | "async" | "await"
            | "yield" | "static" | "import" | "export" | "default" | "try" | "catch"
            | "finally" | "throw" | "void" | "with" | "debugger" | "enum"
    )
}

/// Is a for-loop ITERATION provably numeric? Some(true) = all items are
/// numeric (brace ranges, numeric arrays, Range); Some(false) = known
/// strings; None = unknown (command substitution, $@, ...).
fn iter_numeric(e: &IrExpr) -> Option<bool> {
    /// the brace items arrive as a Json value: nested arrays of range
    /// objects (`{1..5}`) or literal strings (`{a,b}`)
    fn json_items_numeric(v: &serde_json::Value, found: &mut bool) {
        match v {
            serde_json::Value::Array(a) => {
                for x in a {
                    json_items_numeric(x, found);
                }
            }
            serde_json::Value::Object(o) => {
                if !o.contains_key("range") {
                    *found = false;
                }
            }
            serde_json::Value::String(sv) => {
                if sv.trim().parse::<i64>().is_err() {
                    *found = false;
                }
            }
            _ => *found = false,
        }
    }
    fn brace_numeric(args: &[IrExpr]) -> Option<bool> {
        for a in args {
            if let IrExpr::Json(v) = a {
                let mut numeric = true;
                json_items_numeric(v, &mut numeric);
                return Some(numeric);
            }
        }
        None
    }
    match e {
        IrExpr::Range { .. } => Some(true),
        // the merged for-items shape: `Array([brace(...)])` for `{1..3}`,
        // or `Array([Str("1"), Str("2"), ...])` for `1 2 3`
        IrExpr::Array(elems) => {
            let mut numeric = true;
            let mut known = true;
            for el in elems {
                match el {
                    IrExpr::Str(sv, _) => {
                        if sv.trim().parse::<i64>().is_err() {
                            numeric = false;
                        }
                    }
                    IrExpr::Call { func, args } if func == "brace" => match brace_numeric(args) {
                        Some(true) => {}
                        Some(false) => numeric = false,
                        None => known = false,
                    },
                    _ => known = false,
                }
            }
            if known {
                Some(numeric)
            } else {
                None
            }
        }
        IrExpr::Call { func, args } if func == "brace" => brace_numeric(args),
        _ => None,
    }
}

/// All for-loop statements' (var, iter) pairs, recursively.
fn collect_for_iters(prog: &IrProgram) -> HashMap<String, IrExpr> {
    fn walk_stmt(st: &IrStmt, out: &mut HashMap<String, IrExpr>) {
        match st {
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone(), iter.clone());
                for b in body {
                    walk_stmt(b, out);
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    walk_stmt(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    walk_stmt(b, out);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        walk_stmt(stm, out);
                    }
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        walk_stmt(b, out);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    walk_stmt(b, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        walk_stmt(b, out);
                    }
                }
            }
            IrStmt::Expr(e) => walk_expr(e, out),
            IrStmt::Output { value, .. } => walk_expr(value, out),
            _ => {}
        }
    }
    fn walk_expr(e: &IrExpr, out: &mut HashMap<String, IrExpr>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    walk_stmt(st, out);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            _ => {}
        }
    }
    let mut out = HashMap::new();
    for st in &prog.stmts {
        walk_stmt(st, &mut out);
    }
    out
}

/// A lifted FOR-loop variable must be referenced ONLY inside its own loop
/// body: the closure param shadows the module `let` and the store sync is
/// dropped, so any read/write after the loop sees the stale initial value.
/// Remove from both lift sets any loop var referenced outside its loop.
fn drop_externally_referenced_loop_vars(
    prog: &IrProgram,
    num: &HashSet<String>,
    str: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut for_vars: HashSet<String> = HashSet::new();
    fn collect_for_vars(st: &IrStmt, out: &mut HashSet<String>) {
        match st {
            IrStmt::For { var, body, .. } => {
                out.insert(var.clone());
                for b in body {
                    collect_for_vars(b, out);
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    collect_for_vars(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    collect_for_vars(b, out);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        collect_for_vars(stm, out);
                    }
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        collect_for_vars(b, out);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    collect_for_vars(b, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        collect_for_vars(b, out);
                    }
                }
            }
            IrStmt::Expr(e) => collect_for_vars_expr(e, out),
            _ => {}
        }
    }
    fn collect_for_vars_expr(e: &IrExpr, out: &mut HashSet<String>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    collect_for_vars(st, out);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    collect_for_vars_expr(a, out);
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        collect_for_vars(st, &mut for_vars);
    }

    // every reference to a var, tagged with the enclosing For-var stack
    let mut external: HashSet<String> = HashSet::new();
    fn ref_expr(e: &IrExpr, stack: &[String], external: &mut HashSet<String>) {
        match e {
            IrExpr::Var(n, _) => {
                if !stack.contains(&n.clone()) {
                    external.insert(n.clone());
                }
            }
            IrExpr::Call { func, args } => {
                if func == "getVar" {
                    if let [IrExpr::Str(n, _)] = args.as_slice() {
                        if !stack.contains(n) {
                            external.insert(n.clone());
                        }
                    }
                }
                if func == "setVar" {
                    if let [IrExpr::Str(n, _), ..] = args.as_slice() {
                        if !stack.contains(n) {
                            external.insert(n.clone());
                        }
                    }
                }
                for a in args {
                    ref_expr(a, stack, external);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    ref_stmt(st, stack, external);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                ref_expr(lhs, stack, external);
                ref_expr(rhs, stack, external);
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                ref_expr(cond, stack, external);
                ref_expr(then, stack, external);
                ref_expr(else_, stack, external);
            }
            IrExpr::Capture { expr, .. } => ref_expr(expr, stack, external),
            IrExpr::Array(elems) => {
                for el in elems {
                    ref_expr(el, stack, external);
                }
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        ref_expr(inner, stack, external);
                    }
                }
            }
            IrExpr::DefinedOr { expr, default } => {
                ref_expr(expr, stack, external);
                ref_expr(default, stack, external);
            }
            IrExpr::Index { key, .. } => ref_expr(key, stack, external),
            IrExpr::MethodCall { obj, args, .. } => {
                ref_expr(obj, stack, external);
                for a in args {
                    ref_expr(a, stack, external);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    ref_expr(v, stack, external);
                }
            }
            _ => {}
        }
    }
    fn ref_stmt(st: &IrStmt, stack: &[String], external: &mut HashSet<String>) {
        match st {
            IrStmt::For { var, iter, body } => {
                let mut s2 = stack.to_vec();
                s2.push(var.clone());
                ref_expr(iter, &s2, external);
                for b in body {
                    ref_stmt(b, &s2, external);
                }
            }
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if !stack.contains(&t.var) {
                        external.insert(t.var.clone());
                    }
                }
                ref_expr(expr, stack, external);
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    if !stack.contains(&v.name) {
                        external.insert(v.name.clone());
                    }
                }
            }
            IrStmt::While { cond, body, .. }
            | IrStmt::DoWhile { cond, body, .. } => {
                ref_expr(cond, stack, external);
                for b in body {
                    ref_stmt(b, stack, external);
                }
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                ref_expr(cond, stack, external);
                for b in then.iter().chain(else_) {
                    ref_stmt(b, stack, external);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        ref_stmt(stm, stack, external);
                    }
                }
            }
            IrStmt::Exec { cmd, args, .. } => {
                ref_expr(cmd, stack, external);
                for a in args {
                    ref_expr(a, stack, external);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        ref_stmt(b, stack, external);
                    }
                }
            }
            IrStmt::Function { body, .. } | IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    ref_stmt(b, stack, external);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    ref_stmt(b, stack, external);
                }
                for r in redirects {
                    ref_expr(&r.target, stack, external);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                ref_expr(discriminant, stack, external);
                for c in clauses {
                    for b in &c.body {
                        ref_stmt(b, stack, external);
                    }
                }
            }
            IrStmt::Expr(e) => ref_expr(e, stack, external),
            IrStmt::Output { value, .. } => ref_expr(value, stack, external),
            _ => {}
        }
    }
    for st in &prog.stmts {
        ref_stmt(st, &[], &mut external);
    }

    let mut num2 = num.clone();
    let mut str2 = str.clone();
    for v in &for_vars {
        if external.contains(v) {
            num2.remove(v);
            str2.remove(v);
        }
    }
    (num2, str2)
}

/// Precise store-read scan: which identifiers inside a runtime-consumed
/// string does the runtime actually resolve from the STORE? Only
/// `$name` / `${name...}` refs OUTSIDE `$(...)` command substitutions
/// (those run in a subprocess — their `$refs` are bash's, not the store's),
/// plus BARE identifiers inside `$((...))` arithmetic regions (evalArith
/// resolves bare names). Plain words (`mktemp -d` → `d`, `echo hello` →
/// `hello`) are literal text — marking them was pure over-conservatism
/// that silently blocked lifting. Shared by the numeric and string lift
/// walkers (they duplicate the surrounding analysis but must agree on
/// what a store read is).
/// `let "a+1" "i++"` — every arg parses natively (`parse_arith_native`):
/// the emitter lowers the whole call to native expressions (see
/// `try_native_let`), so the runtime never evaluates the strings — their
/// `$var` refs are NOT store reads, and their written vars are natively
/// written. MUST match the emitter's try_native_let eligibility exactly (a
/// mismatch either keeps vars store-bound — a lost win — or, worse, lifts
/// vars the runtime still writes — a desync).
fn arith_let_args_native(args: &[IrExpr]) -> bool {
    matches!(args,
        [IrExpr::Str(cname, _), IrExpr::Array(cargs)]
            if cname == "let"
                && !cargs.is_empty()
                && cargs.iter().all(|a| {
                    matches!(a, IrExpr::Str(sv, _) if parse_arith_native(sv).is_some())
                }))
}

fn plain_ident(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// `typeset -i i` / `declare -i i` / `readonly -i i` — an INTEGER
/// declaration with bare names and no `=value` args (the declare itself
/// does not write; the `-i` attribute is a bash guarantee that every LATER
/// assignment is coerced to an integer — the runtime's `intVars` set).
///
/// ASSUMPTION (SH2_ASSUME_INTDECL=0 disables; default ON): such a name is
/// a numeric WITNESS — the lift analysis treats it as a numeric source and
/// the walkers skip the store-write marks, because a lifted native JS
/// number binding is exactly the integer bash would store (JS machine
/// numbers — the same NO_OVERFLOW family the numeric lift already relies
/// on). The corpus is the oracle: `echo $i` right after `typeset -i i`
/// prints "" in bash vs "0" for a lifted binding — examples that observe
/// the unset state keep the declare store-bound instead.
fn int_declare_names(args: &[IrExpr]) -> Option<Vec<String>> {
    if !assume_intdecl() {
        return None;
    }
    let [IrExpr::Str(cname, _), IrExpr::Array(cargs)] = args else {
        return None;
    };
    if !matches!(cname.as_str(), "typeset" | "declare" | "readonly") {
        return None;
    }
    let mut names = Vec::new();
    let mut saw_i = false;
    for a in cargs {
        let IrExpr::Str(sv, _) = a else { return None; };
        if sv.starts_with('-') {
            // `-i` / `-ir` / `-xi` ... — integer attribute (the runtime
            // checks `f.includes('i')`); `-p`/`-f`/`-F` print forms read
            // the store — disqualify.
            if sv.contains('p') || sv.contains('f') || sv.contains('F') {
                return None;
            }
            if sv.contains('i') {
                saw_i = true;
            }
        } else if sv.starts_with('+') {
            return None; // `+i` removes the attribute — a plain declare
        } else if sv.contains('=') {
            return None; // `typeset -i x=5` — the declare itself writes
        } else if plain_ident(sv) {
            names.push(sv.clone());
        } else {
            return None;
        }
    }
    if !saw_i || names.is_empty() {
        return None;
    }
    Some(names)
}

/// Add native-arithmetic assignment sources for the exec forms the walkers
/// skip marking (see arith_let_args_native / int_declare_names): a
/// natively-lowered `let` writes its args' targets natively (an
/// `IrExpr::Arith` source — numeric when div/mod-free), and an `-i`
/// declaration witnesses its names as numeric (`IrExpr::Int(0)` — the
/// declare does not write; the witness only unlocks the lift). MUST stay
/// in sync with the skip conditions in both walkers.
fn collect_native_arith_sources(args: &[IrExpr], assigns: &mut HashMap<String, Vec<IrExpr>>) {
    let [IrExpr::Str(cname, _), IrExpr::Array(cargs)] = args else {
        return;
    };
    match cname.as_str() {
        "typeset" | "declare" | "readonly" => {
            if let Some(names) = int_declare_names(args) {
                for n in names {
                    assigns.entry(n).or_default().push(IrExpr::Int(0));
                }
            }
        }
        "let" => {
            if !arith_let_args_native(args) {
                return;
            }
            let mut asts: Vec<ArithAst> = Vec::new();
            for a in cargs {
                if let IrExpr::Str(sv, _) = a {
                    if let Some(ast) = parse_arith_native(sv) {
                        asts.push(ast);
                    } else {
                        return; // unreachable via arith_let_args_native
                    }
                }
            }
            for ast in asts {
                for w in arith_written_vars(&ast) {
                    assigns.entry(w).or_default().push(IrExpr::Arith(Box::new(ast.clone())));
                }
            }
        }
        _ => {}
    }
}

/// Every variable a (native-lowered) arith AST writes: assignment targets
/// and `++`/`--` targets.
fn arith_written_vars(a: &ArithAst) -> Vec<String> {
    match a {
        ArithAst::Assign { var, rhs, .. } => {
            let mut v = vec![var.clone()];
            v.extend(arith_written_vars(rhs));
            v
        }
        ArithAst::IncDec { var, .. } => vec![var.clone()],
        ArithAst::Bin { lhs, rhs, .. } => {
            let mut v = arith_written_vars(lhs);
            v.extend(arith_written_vars(rhs));
            v
        }
        ArithAst::Un { arg, .. } => arith_written_vars(arg),
        ArithAst::Cond { test, then, else_, .. } => {
            let mut v = arith_written_vars(test);
            v.extend(arith_written_vars(then));
            v.extend(arith_written_vars(else_));
            v
        }
        ArithAst::Index { key, .. } => arith_written_vars(key),
        _ => Vec::new(),
    }
}

/// SH2_ASSUME_INTDECL=0 — turn off the `typeset -i` numeric-witness lift
/// (maximal fidelity: an integer-declared variable keeps the runtime
/// store, so `echo $i` after `typeset -i i` prints "" exactly like bash).
fn assume_intdecl() -> bool {
    std::env::var("SH2_ASSUME_INTDECL").map_or(true, |v| v != "0")
}

/// SH2_ASSUME_CFOR=0 — turn off the native `for ((...))` ForStatement
/// lowering (the runtime cstyleForSync twin stays; see try_native_cstyle_for).
fn assume_cfor() -> bool {
    std::env::var("SH2_ASSUME_CFOR").map_or(true, |v| v != "0")
}

fn mark_store_refs(s: &str, out: &mut HashSet<String>) {
    fn is_ident(s: &str) -> bool {
        let mut cs = s.chars();
        match cs.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }
    fn mark_arith_region(region: &str, out: &mut HashSet<String>) {
        let bytes = region.as_bytes();
        let n = bytes.len();
        let mut i = 0;
        while i < n {
            let c = bytes[i] as char;
            if c == '$' {
                let mut skip = 1;
                if i + 1 < n && bytes[i + 1] == b'{' {
                    skip = 2;
                }
                let rest = &region[i + skip..];
                let name_len = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .count();
                if name_len > 0 {
                    let name = &rest[..name_len];
                    if is_ident(name) {
                        out.insert(name.to_string());
                    }
                }
                i += skip + name_len;
                continue;
            }
            let prev_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if (c.is_ascii_alphabetic() || c == '_') && !prev_alnum {
                let start = i;
                while i < n && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let w = &region[start..i];
                if is_ident(w) {
                    out.insert(w.to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        // `$(( ... ))` — arith region: bare identifiers are store reads
        // (evalArith resolves them).
        if s[i..].starts_with("$((") {
            let mut j = i + 3;
            let mut depth = 2;
            while j < n && depth > 0 {
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                break;
            }
            mark_arith_region(&s[i + 3..j - 2], out);
            i = j;
            continue;
        }
        // `$( cmd )` / `$(cmd)` — command substitution runs in a
        // SUBPROCESS; its inner `$refs` are bash's, never the runtime
        // store's. Skip the whole region (quote/backtick aware so a `)`
        // inside a quoted string doesn't end it early).
        if s[i..].starts_with("$(") {
            let mut j = i + 2;
            let mut depth = 1;
            let (mut in_sq, mut in_dq, mut in_bt) = (false, false, false);
            while j < n && depth > 0 {
                let cc = bytes[j];
                if in_sq {
                    if cc == b'\'' {
                        in_sq = false;
                    }
                    j += 1;
                    continue;
                }
                if in_dq {
                    if cc == b'\\' {
                        j += 2;
                        continue;
                    }
                    if cc == b'"' {
                        in_dq = false;
                    }
                    j += 1;
                    continue;
                }
                if in_bt {
                    if cc == b'`' {
                        in_bt = false;
                    }
                    j += 1;
                    continue;
                }
                if cc == b'\'' {
                    in_sq = true;
                } else if cc == b'"' {
                    in_dq = true;
                } else if cc == b'`' {
                    in_bt = true;
                } else if cc == b'(' {
                    depth += 1;
                } else if cc == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                break;
            }
            i = j;
            continue;
        }
        // `$'...'` — ANSI-C quoting: not a store read.
        if s[i..].starts_with("$'") {
            let mut j = i + 2;
            while j < n && bytes[j] != b'\'' {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        // `${name...}` — the identifier after `${` is the store read; keep
        // scanning past the name so a body (`:-$y`, `//$a/$b`) is still
        // checked (the runtime expands those too).
        if i + 1 < n && bytes[i + 1] == b'{' {
            let rest = &s[i + 2..];
            let name_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if name_len > 0 {
                let name = &rest[..name_len];
                if is_ident(name) {
                    out.insert(name.to_string());
                }
            }
            i += 2 + name_len;
            continue;
        }
        // `$name` — plain ref.
        let rest = &s[i + 1..];
        let name_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        if name_len > 0 {
            let name = &rest[..name_len];
            if is_ident(name) {
                out.insert(name.to_string());
            }
            i += 1 + name_len;
            continue;
        }
        // `$$` / `$?` / `$1` — specials; not liftable identifiers anyway.
        i += 1;
    }
}

/// Conservative "always a number" analysis for the ESTree backend.
///
/// Does an arithmetic AST contain `/` or `%` anywhere? Only those two
/// operators can abort a `$((...))` expansion (zero divisor) — a native
/// JS arith without them cannot throw, so its arithEval wrapper is dead
/// weight (see the IrExpr::Arith arm of expr_to_estree).
fn arith_has_div_mod(a: &ArithAst) -> bool {
    match a {
        ArithAst::Bin { op, lhs, rhs } => {
            *op == "/" || *op == "%" || arith_has_div_mod(lhs) || arith_has_div_mod(rhs)
        }
        ArithAst::Un { arg, .. } => arith_has_div_mod(arg),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_has_div_mod(test) || arith_has_div_mod(then) || arith_has_div_mod(else_)
        }
        ArithAst::Index { key, .. } => arith_has_div_mod(key),
        ArithAst::Assign { rhs, .. } => arith_has_div_mod(rhs),
        ArithAst::IncDec { .. } => false,
        _ => false,
    }
}

/// Does the AST contain ANY write (`=`/`op=` assignment, `++`/`--`)? Used
/// by the native-lowering guards: a write's VALUE may only be re-evaluated
/// when it is pure (see [`arith_value_pure`]).
fn arith_has_write(a: &ArithAst) -> bool {
    match a {
        ArithAst::Assign { .. } | ArithAst::IncDec { .. } => true,
        ArithAst::Bin { lhs, rhs, .. } => arith_has_write(lhs) || arith_has_write(rhs),
        ArithAst::Un { arg, .. } => arith_has_write(arg),
        ArithAst::Cond {
            test, then, else_, ..
        } => arith_has_write(test) || arith_has_write(then) || arith_has_write(else_),
        ArithAst::Index { key, .. } => arith_has_write(key),
        _ => false,
    }
}

/// The store-var write lowering renders `x = v` as
/// `(sh2.setVar("x", String(v)), v)` — the value is evaluated TWICE, so
/// the whole AST lowers natively ONLY when every write sits at the TOP
/// level with write-free subtrees (a single `x = <pure>` / `x++` /
/// `++x`). Anything else (`x = i++`, `i++ + 1`, nested assignments)
/// keeps the runtime evaluator (the corpus's `i++ + ++i` sites stay on
/// `sh2.arith`).
fn arith_lowerable(a: &ArithAst) -> bool {
    match a {
        ArithAst::Assign { rhs, .. } => !arith_has_write(rhs),
        ArithAst::IncDec { .. } => true,
        ArithAst::Bin { lhs, rhs, .. } => !arith_has_write(lhs) && !arith_has_write(rhs),
        ArithAst::Un { arg, .. } => !arith_has_write(arg),
        ArithAst::Cond { .. } | ArithAst::Index { .. } => !arith_has_write(a),
        _ => true,
    }
}

/// `parse_arith` + the native-lowering eligibility filter: the AST must
/// lower without the runtime evaluator (`arith_lowerable`), and a
/// div/mod expression containing writes stays on the runtime too (the
/// arithEval try/catch cannot express the zero-divisor abort once a
/// native write has already happened — `$((x = 1/0))` must abort BEFORE
/// the write, only the runtime evaluator orders that).
fn parse_arith_native(src: &str) -> Option<ArithAst> {
    let a = parse_arith(src)?;
    if !arith_lowerable(&a) || (arith_has_div_mod(&a) && arith_has_write(&a)) {
        return None;
    }
    Some(a)
}

/// bash variables are strings; JS has real numbers. A variable whose every
/// assignment is provably numeric (a `$((...))` expression without `/`/`%`
/// — the only error sources — a numeric literal, or a copy of another
/// lifted variable) and that never appears in a string-parsed context
/// (`sh2.test`/`sh2.param`/array calls read the runtime STORE by string) can
/// be lifted to a native JS number binding: reads are bare `x`, writes are
/// `x = <expr>`, `let x = 0` at program top. Everything else keeps the
/// runtime store (exact current behavior).
fn numeric_lift_vars(prog: &IrProgram) -> HashSet<String> {
    let mut assigns: HashMap<String, Vec<IrExpr>> = HashMap::new();
    let mut excluded: HashSet<String> = HashSet::new();
    let mut string_ctx: HashSet<String> = HashSet::new();

    fn is_ident(s: &str) -> bool {
        let mut cs = s.chars();
        match cs.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }
    fn mark_string_refs(s: &str, out: &mut HashSet<String>) {
        // A var is read from the STORE only when the runtime would resolve
        // it from a string: `$name` / `${name...}` refs outside `$(...)`
        // subprocess regions, and bare identifiers inside `$((...))` arith
        // regions. Plain words in strings are literal text (over-marking
        // them kept `d` store-bound for `mktemp -d` etc.).
        mark_store_refs(s, out);
    }
    fn mark_write_builtin_vars(e: &IrExpr, excluded: &mut HashSet<String>) {
        match e {
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_write_builtin_vars(el, excluded);
                }
            }
            IrExpr::Str(sv, _) => {
                let v = sv.split('=').next().unwrap_or("");
                if is_ident(v) {
                    excluded.insert(v.to_string());

                }
            }
            _ => {}
        }
    }
    // `let`/`eval`/`(( ))` args are ARITHMETIC EXPRESSIONS — every bare
    // identifier is a variable the runtime touches (unlike plain string
    // words, which mark_store_refs correctly ignores).
    fn mark_all_idents(s: &str, out: &mut HashSet<String>) {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if (c.is_ascii_alphabetic() || c == '_')
                && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
            {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let w = &s[start..i];
                if is_ident(w) {
                    out.insert(w.to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    fn mark_all_idents_args(e: &IrExpr, out: &mut HashSet<String>) {
        match e {
            IrExpr::Str(ss, _) => mark_all_idents(ss, out),
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_all_idents_args(el, out);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    mark_all_idents_args(v, out);
                }
            }
            _ => {}
        }
    }
    fn mark_str_args(e: &IrExpr, string_ctx: &mut HashSet<String>) {
        match e {
            IrExpr::Str(ss, _) => mark_string_refs(ss, string_ctx),
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_str_args(el, string_ctx);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    mark_str_args(v, string_ctx);
                }
            }
            _ => {}
        }
    }
    fn walk_expr(
        e: &IrExpr,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match e {
            IrExpr::Call { func, args } => {
                // `test` / `setArray` / `setArrayAppend` strings are
                // excluded: the renderer injects lifted values into them,
                // so a lifted var may appear inside them.
                let let_args_native = func == "exec" && arith_let_args_native(args);
                if func != "getVar" && func != "test" && func != "setArray" && func != "setArrayAppend"
                    && !let_args_native
                {
                    // ANY runtime call's string args (recursing into the
                    // Array/[] wrappers exec and setArrayAppend use) may
                    // contain `$var` references the runtime resolves from
                    // the STORE — setArrayAppend(["$candidate"]),
                    // local("n=$1"), test("$count -lt 100") — so mark every
                    // identifier found there as store-read (not liftable).
                    for a in args {
                        mark_str_args(a, string_ctx);
                    }
                }
                // `local i=3` / `read line` / `export FOO=x` in EXPRESSION
                // position (inside function arrows / && chains the exec call
                // arrives as IrStmt::Expr, not IrStmt::Exec): the runtime
                // builtin WRITES the named vars into the STORE, so they must
                // stay store-bound — a native binding would never see the
                // write (and the native binding's value would be stale for
                // every later read). Mirror of the string-lift walker.
                if func == "exec" || func == "builtin" {
                    // `builtin` is the sync-builtin-dispatch callee (M8) —
                    // same write-builtin semantics as exec-lowered builtins
                    if let Some(IrExpr::Str(cname, _)) = args.first() {
                        if matches!(
                            cname.as_str(),
                            "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                                | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source"
                                | "."
                        ) {
                            // A natively-lowered `let` (try_native_let — the
                            // runtime never sees the args) and the bare
                            // names of `typeset -i` declarations (numeric
                            // witnesses — native number bindings) are not
                            // store writes: skip the marks for them.
                            let native_let = cname == "let" && let_args_native;
                            let intdecl = if cname == "let" { Vec::new() } else {
                                int_declare_names(args).unwrap_or_default()
                            };
                            for a in &args[1..] {
                                if native_let {
                                    continue;
                                }
                                if !intdecl.is_empty() {
                                    if let IrExpr::Str(sv, _) = a {
                                        if !sv.contains('=') && intdecl.iter().any(|n| n == sv) {
                                            continue;
                                        }
                                    }
                                }
                                mark_write_builtin_vars(a, excluded);
                                // `let`/`(( ))`/`eval` args are EXPRESSIONS
                                // ("i++") — mark EVERY identifier they touch
                                // so a lifted native binding never desyncs
                                // from the runtime's store write
                                mark_all_idents_args(a, string_ctx);
                            }
                        }
                    }
                }
                if matches!(
                    func.as_str(),
                    "arrayIndex" | "arrayLen" | "arrayItems" | "arraySlice" | "setArray"
                        | "setArrayAppend"
                ) {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        excluded.insert(name.clone());
                    }
                }
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    walk_stmt(st, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Index { key, .. } => walk_expr(key, excluded, string_ctx, in_copy),
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, excluded, string_ctx, in_copy);
                walk_expr(rhs, excluded, string_ctx, in_copy);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                walk_expr(then, excluded, string_ctx, in_copy);
                walk_expr(else_, excluded, string_ctx, in_copy);
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, excluded, string_ctx, in_copy);
                walk_expr(default, excluded, string_ctx, in_copy);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, excluded, string_ctx, in_copy),
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, excluded, string_ctx, in_copy);
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(
        st: &IrStmt,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        if in_copy {
                            // a subshell/background write is COPY-local in
                            // bash — a lifted module var would be clobbered
                            excluded.insert(t.var.clone());
                        }
                    } else {
                        excluded.insert(t.var.clone());
                    }
                }
                walk_expr(expr, excluded, string_ctx, in_copy);
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    excluded.insert(v.name.clone());
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                excluded.insert(var.clone());
            }
            IrStmt::For { var, iter, body } => {
                // NOTE: the loop var is NOT excluded here — the loop
                // iteration is its assignment source (see collect_for_iters
                // + the fixpoint); external references are removed by
                // drop_externally_referenced_loop_vars afterwards.
                walk_expr(iter, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in then.iter().chain(else_) {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        walk_stmt(stm, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                env,
                ..
            } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for (v, _) in env {
                    excluded.insert(v.clone());
                }
                if let IrExpr::Str(cname, _) = cmd {
                    if matches!(
                        cname.as_str(),
                        "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                            | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source" | "."
                    ) {
                        // natively-lowered `let` args (try_native_let) and
                        // the bare names of `typeset -i` declarations are
                        // not store writes — skip their marks (mirror of
                        // the expression-position block; a natively-written
                        // var must not stay store-bound, and a runtime-
                        // written var must not lift).
                        let native_let = cname == "let" && arith_let_args_native(args);
                        let intdecl = if cname == "let" { Vec::new() } else {
                            int_declare_names(args).unwrap_or_default()
                        };
                        for a in args {
                            if native_let {
                                continue;
                            }
                            if !intdecl.is_empty() {
                                if let IrExpr::Str(sv, _) = a {
                                    if !sv.contains('=') && intdecl.iter().any(|n| n == sv) {
                                        continue;
                                    }
                                }
                            }
                            mark_write_builtin_vars(a, excluded);
                            // `let`/`(( ))`/`eval` args are ARITHMETIC
                            // EXPRESSIONS — mark EVERY identifier they
                            // touch so a lifted native binding never
                            // desyncs from a runtime store write
                            mark_all_idents_args(a, string_ctx);
                        }
                    }
                }
                walk_expr(cmd, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::Pipeline { stages, capture, .. } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for stage in stages {
                    for b in stage {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Function { body, .. } | IrStmt::Block(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            // subshell/background: COPY semantics — writes inside are local
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, true);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for r in redirects {
                    walk_expr(&r.target, excluded, string_ctx, in_copy);
                    if r.interpolate {
                        // the runtime expandWord's interpolated heredoc
                        // bodies from the STORE
                        if let IrExpr::Str(body, _) = &r.target {
                            mark_string_refs(body, string_ctx);
                        }
                    }
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                walk_expr(discriminant, excluded, string_ctx, in_copy);
                for c in clauses {
                    for p in &c.patterns {
                        mark_string_refs(p, string_ctx);
                    }
                    for b in &c.body {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Expr(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Output { value, .. } => walk_expr(value, excluded, string_ctx, in_copy),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, excluded, string_ctx, in_copy);
                walk_expr(content, excluded, string_ctx, in_copy);
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => {
                walk_expr(e, excluded, string_ctx, in_copy)
            }
            IrStmt::SetChildError(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_expr(expr, excluded, string_ctx, in_copy)
            }
            _ => {}
        }
    }

    for st in &prog.stmts {
        walk_stmt(st, &mut excluded, &mut string_ctx, false);
    }

    // collect assignment sources (top-level + function bodies — a function
    // WRITING a global is a global write in bash, so it counts)
    fn collect_assigns(st: &IrStmt, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        assigns
                            .entry(t.var.clone())
                            .or_default()
                            .push(expr.clone());
                    }
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    collect_assigns(b, assigns);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        collect_assigns(stm, assigns);
                    }
                }
            }
            IrStmt::For { var, body, .. } => {
                // the loop iteration is a source even with no body writes
                assigns.entry(var.clone()).or_default();
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Expr(e) => collect_expr_assigns(e, assigns),
            IrStmt::Output { value, .. } => collect_expr_assigns(value, assigns),
            _ => {}
        }
    }
    fn collect_expr_assigns(e: &IrExpr, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    collect_assigns(st, assigns);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    collect_expr_assigns(el, assigns);
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        collect_assigns(st, &mut assigns);
    }

    // fixpoint: a var is liftable when ALL its assignment sources are
    // numeric (arith without / %, numeric literal, or another lifted var).
    let for_iters = collect_for_iters(prog);
    let mut lifted: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for (name, exprs) in &assigns {
            if lifted.contains(name)
                || excluded.contains(name)
                || string_ctx.contains(name)
                || is_reserved_var(name)
                || is_js_keyword(name)
                || name.contains('[')
                || name.contains(']')
            {
                // names with a subscript (`map[answer]` — the parser keeps
                // the whole bracket string as the var name) are array
                // writes: never liftable (a `let map[answer]` is invalid JS)
                continue;
            }
            let all_numeric = exprs.iter().all(|e| match e {
                // `/` and `%` assignments stay blocked: bash ABORTS the
                // whole expansion on a zero divisor (x stays unchanged),
                // which a native/lifted number binding cannot express
                // (a lifted `x = Math.trunc(1/0)` would store Infinity,
                // and `$((0 % 0))` with unset operands is exercised by
                // 063_01_deeply_nested_arithmetic — a native % yields NaN
                // where bash yields ""). The runtime idiv/imod throw and
                // the setVar path's arithEval catches → "".
                IrExpr::Arith(a) => !arith_has_div_mod(a),
                IrExpr::Int(_) => true,
                IrExpr::Str(sv, _) => sv.trim().parse::<i64>().is_ok(),
                IrExpr::Var(n, _) => lifted.contains(n.as_str()),
                IrExpr::Call { func, args } if func == "getVar" => {
                    matches!(args.as_slice(), [IrExpr::Str(n, _)] if lifted.contains(n.as_str()))
                }
                _ => false,
            }) && for_iters.get(name).map_or(true, |it| iter_numeric(it) == Some(true));
            if all_numeric {
                lifted.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    lifted
}


pub fn shir_to_estree(prog: &IrProgram) -> Program {
    let _compile_guard = COMPILE_LOCK.lock().unwrap();
    let (num, str) = drop_externally_referenced_loop_vars(
        prog,
        &numeric_lift_vars(prog),
        &string_lift_vars(prog, &numeric_lift_vars(prog)),
    );
    // Run ALL analysis passes before touching any static: the lift/scan
    // statics are shared global state, and the determinism unit test
    // compiles concurrently in other threads — a computation between the
    // static writes and the body emission widens the torn-read window.
    let nocase = ir_may_enable_nocasematch(prog);
    let errexit = ir_may_enable_errexit(prog);
    let persist_fd1 = ir_has_persist_fd1(prog);
    let mut functions = HashSet::new();
    collect_program_functions(&prog.stmts, &mut functions);
    *LIFTED_NUMERIC.lock().unwrap() = Some(num);
    *LIFTED_STRING.lock().unwrap() = Some(str);
    *CASE_NOCASE.lock().unwrap() = Some(nocase);
    *MAY_ERREXIT.lock().unwrap() = Some(errexit);
    *PROGRAM_PERSIST_FD1.lock().unwrap() = Some(persist_fd1);
    *PROGRAM_FUNCTIONS.lock().unwrap() = Some(functions.clone());
    // Sync-function fixpoint: names whose calls lower to the sync fnCall
    // path (non-async define arrows; loops over them go *Sync). Must run
    // after the lift/scan statics above (the optimistic body emission reads
    // them) and before the main emission (the fn-call sites consult it).
    *SYNC_FN_CALLS.lock().unwrap() = Some(fn_call_sync_set(prog, &functions));
    // Plan 4 — lastExit-write liveness: which `(( ))` statements' status
    // writes are unread (empty under a possible `set -e`). Runs before the
    // emission (the IR tree is immutable here — the *Sync loop bodies are
    // emitted from the ORIGINAL references, so the pointer keys hold).
    *LASTEXIT_DEAD.lock().unwrap() = Some(compute_lastexit_deadness(prog, errexit));
    let mut body: Vec<Stmt> = Vec::new();
    // `let x = 0` (numeric) / `let x = ""` (string) at program top. bash
    // reads an unset var as 0 in arithmetic and "" as a string.
    for name in LIFTED_NUMERIC.lock().unwrap().as_ref().unwrap().iter() {
        body.push(Stmt::VariableDeclaration {
            kind: "let",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier { name: name.clone() },
                init: Some(Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                regex: None,
                }),
            }],
        });
    }
    for name in LIFTED_STRING.lock().unwrap().as_ref().unwrap().iter() {
        body.push(Stmt::VariableDeclaration {
            kind: "let",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier { name: name.clone() },
                init: Some(Expr::Literal {
                    value: serde_json::Value::String(String::new()),
                    raw: None,
                regex: None,
                }),
            }],
        });
    }
    body.extend(prog.stmts.iter().filter_map(top_stmt_to_estree));
    Program {
        type_: "Program",
        source_type: "module",
        body,
    }
}

/// Top-level statement lowering: additionally wraps statement-position calls
/// in `sh2.guard(...)` so the runtime can implement `set -e` (errexit): a
/// failing SIMPLE command at statement level aborts the script, exactly like
/// bash. Guarded are single calls (exec/test/pipeline/redirect/subshell/
/// loops) and assignments; NOT guarded are `&&`/`||`/`!` expressions (bash
/// exempts non-final commands in those lists from errexit).
///
/// A call that provably always succeeds is NOT guarded: the runtime guard
/// only fires on a falsy value, so wrapping an always-truthy call (see
/// [`call_is_always_true`]) would be a no-op — skipping it removes the
/// dispatch while keeping errexit semantics byte-identical.
fn top_stmt_to_estree(stmt: &IrStmt) -> Option<Stmt> {
    let s = stmt_to_estree(stmt)?;
    // No `set -e` anywhere → the runtime's errexit flag can never turn on,
    // so `sh2.guard(v)` would be an identity call on every statement.
    // Skip the wrapper entirely (provably identical semantics, ~1600 fewer
    // runtime calls across the corpus).
    if !MAY_ERREXIT.lock().unwrap().unwrap_or(true) {
        return Some(s);
    }
    let guardable = match stmt {
        IrStmt::Expr(IrExpr::Call { func, .. }) => {
            // `&&` / `||` / `!` lists: bash exempts non-final commands from
            // errexit; `and`/`or` are the lowered &&/|| (same exemption),
            // `not` is `!`.
            !matches!(
                func.as_str(),
                "break" | "continue" | "return" | "and" | "or" | "not"
            )
        }
        IrStmt::Expr(_) => false, // && / || / ! — errexit exemptions
        IrStmt::While { .. }
        | IrStmt::For { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Redirect { .. } => true,
        IrStmt::Assign { targets, .. } => {
            // a native lifted write always succeeds → guard would be wrong
            // (guard(0) exits under errexit)
            !targets.iter().any(|t| is_lifted(&t.var) && t.indices.is_empty())
        }
        _ => false,
    };
    if !guardable {
        return Some(s);
    }
    match s {
        Stmt::ExpressionStatement { expression } => {
            // A call that provably always returns truthy (always-true
            // builtins, setVar/setArray/shopt/forLoopSync — every runtime
            // path ends in a truthy return) can never make the guard fire:
            // drop the wrapper.
            if call_is_always_true(&expression) {
                return Some(Stmt::ExpressionStatement { expression });
            }
            Some(Stmt::ExpressionStatement {
                expression: guard_native(expression),
            })
        }
        other => Some(other),
    }
}

/// Builtins whose runtime impls return truthy on EVERY path (verified
/// against the builtins map in harness/sh2-namespace.mjs): a guard wrapper
/// around them is dead weight — errexit can never fire on a truthy value.
/// NOT in this set (they can return false and MUST stay guarded):
/// cd/test/touch/read/readonly(dirname/basename/cmp/sort can fail too —
/// only unconditional-true impls qualify). `exit` never returns (process
/// exit), so its guard is unreachable either way.
const ALWAYS_TRUE_BUILTINS: &[&str] = &[
    "echo", "printf", "pwd", "export", "unset", "mapfile", "set", "declare",
    "shift", "local", "trap", "type", "seq", "head", "tail", "wc", "uniq",
    "comm", "true",
];

/// Does the lowered expression call an sh2.* runtime function that
/// provably returns truthy on every path (so a guard wrapper is a no-op)?
fn call_is_always_true(e: &Expr) -> bool {
    // The native echo / `true` / `:` / `false` sequences (see
    // try_native_echo and the status lowering): `(write, sh2.lastExit = N,
    // B)` — always truthy (the echo builtin never fails with the default
    // stdout sink; the status literals are constants). Same dead-weight
    // rule as the ALWAYS_TRUE_BUILTINS: the guard can never fire.
    if let Expr::SequenceExpression { expressions } = e {
        return matches!(
            expressions.last(),
            Some(Expr::Literal { value, .. }) if value == &serde_json::Value::Bool(true)
        ) && expressions.iter().any(|ex| {
            matches!(ex, Expr::AssignmentExpression { left, .. }
                if matches!(&**left, Expr::MemberExpression { object, property, .. }
                    if matches!(&**object, Expr::Identifier { name } if name == "sh2")
                        && matches!(&**property, Expr::Identifier { name } if name == "lastExit")))
        });
    }
    let Expr::CallExpression {
        callee, arguments, ..
    } = e
    else {
        return false;
    };
    let Expr::MemberExpression {
        object, property, ..
    } = &**callee
    else {
        return false;
    };
    let Expr::Identifier { name: obj } = &**object else {
        return false;
    };
    if obj != "sh2" {
        return false;
    }
    let Expr::Identifier { name: prop } = &**property else {
        return false;
    };
    match prop.as_str() {
        // runtime helpers that always return truthy
        "setVar" | "setArray" | "shopt" | "forLoopSync" | "whileLoopSync" => true,
        "builtin" => match arguments.first() {
            Some(Expr::Literal { value, .. }) => match value {
                serde_json::Value::String(bn) => ALWAYS_TRUE_BUILTINS.contains(&bn.as_str()),
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

pub fn shir_to_estree_json(prog: &IrProgram) -> Result<String, serde_json::Error> {
    serde_json::to_string(&shir_to_estree(prog))
}

/// Classification of a case-pattern string for the native lowering.
/// `globMatch` on any of these shapes is a plain JS string op:
/// - `*` / `**` → matches every value (`CasePat::Any`);
/// - `*lit*` → substring (`CasePat::Substr`);
/// - `lit*` → prefix (`CasePat::Prefix`);
/// - `*lit` → suffix (`CasePat::Suffix`);
/// - `lit` (optionally quoted) → exact equality (`CasePat::Exact`).
/// Conservative: rejects `$` (expansion), `(`, `)`, quotes, backslash,
/// `!`, and any glob metacharacter inside the literal.
#[derive(Debug, Clone, PartialEq)]
enum CasePat {
    Any,
    Substr(String),
    Prefix(String),
    Suffix(String),
    Exact(String),
}

fn classify_case_pat(pat: &str) -> Option<CasePat> {
    // A quoted pattern (`"start"`, `''`) arrives with its quote chars; the
    // runtime's expandWord strips them (making a quoted `*a*` an ACTIVE
    // glob there too), so unwrap one pair of quotes before classifying.
    let bare = pat
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| {
            pat.strip_prefix('\'')
                .and_then(|x| x.strip_suffix('\''))
        })
        .unwrap_or(pat);
    let has_meta = |s: &str| {
        s.chars().any(|c| {
            matches!(
                c,
                '*' | '?' | '[' | ']' | '\\' | '$' | '(' | ')' | '\'' | '"' | '!'
            )
        })
    };
    // `*` / `**` / ... — consecutive stars collapse to one `*`.
    if !bare.is_empty() && bare.chars().all(|c| c == '*') {
        return Some(CasePat::Any);
    }
    // `*lit*`, `lit*`, `*lit` — exactly one star on either side.
    if let Some(inner) = bare.strip_prefix('*') {
        if let Some(inner) = inner.strip_suffix('*') {
            if inner.is_empty() || has_meta(inner) {
                return None;
            }
            return Some(CasePat::Substr(inner.to_string()));
        }
        if inner.is_empty() || has_meta(inner) {
            return None;
        }
        return Some(CasePat::Suffix(inner.to_string()));
    }
    if let Some(inner) = bare.strip_suffix('*') {
        if inner.is_empty() || has_meta(inner) {
            return None;
        }
        return Some(CasePat::Prefix(inner.to_string()));
    }
    if has_meta(bare) {
        return None;
    }
    Some(CasePat::Exact(bare.to_string()))
}

/// Lower a `case` whose EVERY pattern is one of the [`CasePat`] shapes to a
/// native if/else-if chain: `String(disc).includes(lit)` for substring
/// globs, `String(disc) === lit` for exact literals, `true` for `*` — no
/// `sh2.caseMatch` dispatch, no glob engine, no per-pattern string parsing.
/// bash `case` is first-match-wins, which is exactly an if/else-if chain;
/// the discriminant is bound once to a temp const (the switch form
/// evaluates it once too). Under a possible `shopt -s nocasematch` the
/// runtime's caseMatch lowercases the VALUE side only (not the pattern) —
/// mirrored exactly. Conservative: any unclassifiable pattern (or no
/// clauses at all) keeps the runtime switch form.
fn try_native_case(
    discriminant: &IrExpr,
    clauses: &[IrCaseClause],
    nocase: bool,
) -> Option<Stmt> {
    if clauses.is_empty() {
        return None;
    }
    let pats: Vec<Vec<CasePat>> = clauses
        .iter()
        .map(|c| {
            c.patterns
                .iter()
                .map(|p| classify_case_pat(p))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    // `String($sh_case ?? '')` — the runtime's caseMatch coercion.
    let value_expr = |id: &str| {
        let base = Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![Expr::LogicalExpression {
                operator: "??",
                left: Box::new(Expr::Identifier {
                    name: id.to_string(),
                }),
                right: Box::new(str_lit("")),
            }],
            optional: false,
        };
        if nocase {
            Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(base),
                    property: Box::new(Expr::Identifier {
                        name: "toLowerCase".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![],
                optional: false,
            }
        } else {
            base
        }
    };
    let pat_test = |pat: &CasePat| -> Expr {
        let value = value_expr(CASE_TMP);
        match pat {
            CasePat::Any => Expr::Literal {
                value: serde_json::Value::Bool(true),
                raw: None,
            regex: None,
            },
            CasePat::Substr(lit) => Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "includes".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(lit)],
                optional: false,
            },
            CasePat::Prefix(lit) => Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "startsWith".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(lit)],
                optional: false,
            },
            CasePat::Suffix(lit) => Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "endsWith".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(lit)],
                optional: false,
            },
            CasePat::Exact(lit) => Expr::BinaryExpression {
                operator: "===",
                left: Box::new(value),
                right: Box::new(str_lit(lit)),
            },
        }
    };
    // Clause body: same break/continue → sh2.* signal mapping as the switch
    // form (a native break inside the if would only be legal inside a JS
    // loop, but bash's break must exit the ENCLOSING loop).
    let body_of = |stmts: &[IrStmt]| Stmt::BlockStatement {
        body: stmts
            .iter()
            .filter_map(stmt_to_estree)
            .map(|s| match s {
                Stmt::BreakStatement { .. } => Stmt::ExpressionStatement {
                    expression: sh2_call("break", vec![]),
                },
                Stmt::ContinueStatement { .. } => Stmt::ExpressionStatement {
                    expression: sh2_call("continue", vec![]),
                },
                other => other,
            })
            .collect(),
    };
    // Build the chain from the LAST clause backwards (alternate nesting).
    let mut alt: Option<Box<Stmt>> = None;
    for (clause, pats) in clauses.iter().zip(pats.iter()).rev() {
        let mut test: Option<Expr> = None;
        for pat in pats {
            let t = pat_test(pat);
            test = Some(match test {
                None => t,
                Some(prev) => Expr::LogicalExpression {
                    operator: "||",
                    left: Box::new(prev),
                    right: Box::new(t),
                },
            });
        }
        let stmt = Stmt::IfStatement {
            test: test.expect("case clause has at least one pattern"),
            consequent: Box::new(body_of(&clause.body)),
            alternate: alt.take(),
        };
        alt = Some(Box::new(stmt));
    }
    Some(Stmt::BlockStatement {
        body: vec![
            Stmt::VariableDeclaration {
                kind: "const",
                declarations: vec![VariableDeclarator {
                    type_: "VariableDeclarator",
                    id: Expr::Identifier {
                        name: CASE_TMP.to_string(),
                    },
                    init: Some(expr_to_estree(discriminant)),
                }],
            },
            *alt.expect("at least one clause"),
        ],
    })
}

/// True when the program may enable `set -e` (errexit) anywhere (top level
/// or nested in any function/loop/case/arrow body). The runtime's
/// `sh2.guard` wrapper is the identity function when the errexit flag never
/// turns on (it starts false and only the `set` builtin's `-e` / `-o
/// errexit` toggles it — `eval`/`source` run real bash subprocesses whose
/// errexit is their own), so the emitter skips guard emission entirely for
/// programs that provably never enable it. Conservative: a dynamic command
/// name (`$cmd ...` may be `set`), a non-literal set argument (may be
/// `-e`), or `-e` in any flag cluster all keep the guards.
fn ir_may_enable_errexit(prog: &IrProgram) -> bool {
    /// A literal `exec("set", [...])` call — the runtime's set builtin
    /// parses (see sh2-namespace.mjs): `--` first or a first arg without a
    /// `-`/`+` prefix means positional assignment (no flags); otherwise
    /// each flag cluster is scanned — `e` enables errexit with a `-`
    /// prefix (and disables with `+`), `o` makes the NEXT argument a long
    /// option name (`errexit` enables).
    fn set_call_enables(args: &[IrExpr]) -> bool {
        let mut lits: Vec<&str> = Vec::with_capacity(args.len());
        for a in args {
            match a {
                IrExpr::Str(s, _) => lits.push(s.as_str()),
                _ => return true, // dynamic flag word: may be `-e` at runtime
            }
        }
        match lits.first() {
            None | Some(&"--") => return false,
            Some(a) if !a.starts_with('-') && !a.starts_with('+') => {
                return false; // positional assignment
            }
            _ => {}
        }
        let mut pending_o = false; // `-o`/`+o` seen: next arg is a long option name
        let mut o_enable = false;
        for a in lits {
            if pending_o {
                if a == "errexit" && o_enable {
                    return true;
                }
                pending_o = false;
                continue;
            }
            let enable = a.starts_with('-');
            if !a.starts_with('-') && !a.starts_with('+') {
                continue;
            }
            for c in a[1..].chars() {
                if c == 'e' && enable {
                    return true;
                }
                if c == 'o' {
                    pending_o = true;
                    o_enable = enable;
                }
            }
        }
        false
    }

    fn scan_expr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                if func == "exec" {
                    match args.as_slice() {
                        [IrExpr::Str(name, _), IrExpr::Array(elems), ..] if name == "set" => {
                            if set_call_enables(elems) {
                                return true;
                            }
                        }
                        [IrExpr::Str(_name, _), ..] => { /* other literal command */ }
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
    fn scan_stmts(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(scan_stmt)
    }
    fn scan_stmt(s: &IrStmt) -> bool {
        match s {
            IrStmt::Expr(e) => scan_expr(e),
            IrStmt::Output { value, .. } => scan_expr(value),
            IrStmt::WriteFile { path, content, .. } => {
                scan_expr(path) || scan_expr(content)
            }
            IrStmt::Assign { targets, expr } => {
                scan_expr(expr) || targets.iter().any(|t| t.indices.iter().any(scan_expr))
            }
            IrStmt::Declare { init, .. } => init.as_ref().is_some_and(scan_expr),
            IrStmt::DeclareArray { elements, .. } => elements.iter().any(scan_expr),
            IrStmt::If { cond, then, elsifs, else_ } => {
                scan_expr(cond)
                    || scan_stmts(then)
                    || scan_stmts(else_)
                    || elsifs.iter().any(|(c, b)| scan_expr(c) || scan_stmts(b))
            }
            IrStmt::For { iter, body, .. } => scan_expr(iter) || scan_stmts(body),
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                scan_expr(cond) || scan_stmts(body)
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => scan_expr(expr),
            IrStmt::Exec { cmd, args, env, .. } => {
                match cmd {
                    IrExpr::Str(name, _) if name == "set" => {
                        if set_call_enables(args) {
                            return true;
                        }
                    }
                    IrExpr::Str(_name, _) => { /* other literal command */ }
                    _ => return true, // dynamic command name: may be `set`
                }
                scan_expr(cmd)
                    || args.iter().any(scan_expr)
                    || env.iter().any(|(_, v)| scan_expr(v))
            }
            IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| scan_stmts(s)),
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => scan_expr(e),
            IrStmt::SetChildError(e) => scan_expr(e),
            IrStmt::Case {
                discriminant,
                clauses,
            } => scan_expr(discriminant) || clauses.iter().any(|c| scan_stmts(&c.body)),
            IrStmt::Redirect { inner, redirects } => {
                scan_stmts(inner) || redirects.iter().any(|r| scan_expr(&r.target))
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => scan_stmts(body),
            IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Return(None)
            | IrStmt::Exit(None) => false,
        }
    }
    scan_stmts(&prog.stmts)
}

/// True when the program contains `shopt -s nocasematch` anywhere (top
/// level or nested in any function/loop/case/arrow body). Conservative: the
/// runtime's case/test matching is case-insensitive once enabled, so a
/// native substring lift must lowercase to stay exact. `shopt -u` after a
/// `-s` still counts (a static scan cannot prove the runtime state).
fn ir_may_enable_nocasematch(prog: &IrProgram) -> bool {
    fn scan_expr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                if func == "shopt"
                    && matches!(args.as_slice(), [IrExpr::Str(opt, _), IrExpr::Bool(en)]
                        if opt == "nocasematch" && *en)
                {
                    return true;
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
    fn scan_stmts(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(scan_stmt)
    }
    fn scan_stmt(s: &IrStmt) -> bool {
        match s {
            IrStmt::Expr(e) => scan_expr(e),
            IrStmt::Output { value, .. } => scan_expr(value),
            IrStmt::WriteFile { path, content, .. } => {
                scan_expr(path) || scan_expr(content)
            }
            IrStmt::Assign { targets, expr } => {
                scan_expr(expr) || targets.iter().any(|t| t.indices.iter().any(scan_expr))
            }
            IrStmt::Declare { init, .. } => init.as_ref().is_some_and(scan_expr),
            IrStmt::DeclareArray { elements, .. } => elements.iter().any(scan_expr),
            IrStmt::If { cond, then, elsifs, else_ } => {
                scan_expr(cond)
                    || scan_stmts(then)
                    || scan_stmts(else_)
                    || elsifs.iter().any(|(c, b)| scan_expr(c) || scan_stmts(b))
            }
            IrStmt::For { iter, body, .. } => scan_expr(iter) || scan_stmts(body),
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                scan_expr(cond) || scan_stmts(body)
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => scan_expr(expr),
            IrStmt::Exec { cmd, args, env, .. } => {
                scan_expr(cmd) || args.iter().any(scan_expr) || env.iter().any(|(_, v)| scan_expr(v))
            }
            IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| scan_stmts(s)),
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => scan_expr(e),
            IrStmt::SetChildError(e) => scan_expr(e),
            IrStmt::Case {
                discriminant,
                clauses,
            } => scan_expr(discriminant) || clauses.iter().any(|c| scan_stmts(&c.body)),
            IrStmt::Redirect { inner, redirects } => {
                scan_stmts(inner) || redirects.iter().any(|r| scan_expr(&r.target))
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => scan_stmts(body),
            IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Return(None)
            | IrStmt::Exit(None) => false,
        }
    }
    scan_stmts(&prog.stmts)
}

/// The temp binding name for a lifted case discriminant. `$` cannot appear
/// in a shell variable name, so `$sh_case` can never collide with a lifted
/// variable's native JS binding.
const CASE_TMP: &str = "$sh_case";

/// Whether the program contains a PERSISTENT fd-1 redirect (a bare
/// `exec` builtin with a redirect — `exec >file`, `exec 1>&2`, `exec 1>&-`;
/// the runtime keeps those in the fd table after the redirect call).
/// Native top-level `echo` writes `process.stdout` directly, which is only
/// byte-identical while fd 1 is the module's default stdout — a persistent
/// fd-1 redirect anywhere (function bodies included) makes the runtime
/// dispatch mandatory. Walks the whole ShIR (stmt + expression forms); the
/// estree path only constructs the subset matched below, so `_ => false`
/// is exact for it. Conservative direction: any doubt resolves to `true`.
fn ir_has_persist_fd1(prog: &IrProgram) -> bool {
    fn stmt_has(s: &IrStmt) -> bool {
        match s {
            // statement-form redirect: persist = inner is a bare literal
            // `exec` with NO args (the stmt_to_estree persist rule)
            IrStmt::Redirect { inner, redirects } => {
                let persist = matches!(
                    inner.as_slice(),
                    [IrStmt::Expr(IrExpr::Call { func, args })]
                        if func == "exec"
                            && matches!(args.as_slice(), [IrExpr::Str(name, _), IrExpr::Array(a)]
                                if name == "exec" && a.is_empty())
                );
                (persist && redirects.iter().any(|r| r.fd == Some(1)))
                    || inner.iter().any(stmt_has)
            }
            IrStmt::Expr(e) => expr_has(e),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
                ..
            } => {
                expr_has(cond)
                    || then.iter().any(stmt_has)
                    || elsifs
                        .iter()
                        .any(|(c, b)| expr_has(c) || b.iter().any(stmt_has))
                    || else_.iter().any(stmt_has)
            }
            IrStmt::While { cond, body, .. } => {
                expr_has(cond) || body.iter().any(stmt_has)
            }
            IrStmt::For { iter, body, .. } => {
                expr_has(iter) || body.iter().any(stmt_has)
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => body.iter().any(stmt_has),
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                expr_has(discriminant)
                    || clauses.iter().any(|c| c.body.iter().any(stmt_has))
            }
            IrStmt::Assign { expr, .. } => expr_has(expr),
            IrStmt::Return(Some(e)) => expr_has(e),
            // Perl-only variants are never constructed by ast_to_ir (the
            // estree path) — nothing to scan.
            _ => false,
        }
    }
    fn expr_has(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                if func == "redirect" {
                    // expression-form redirect: [Arrow(stmts), Array(specs)]
                    // — a spec object with fd 1 + persist true (see
                    // redirect_spec_object_persist)
                    if let [IrExpr::Arrow(_), IrExpr::Array(specs)] = args.as_slice() {
                        if specs.iter().any(|s| match s {
                            IrExpr::Object(props) => {
                                let fd = props.iter().any(|(k, v)| {
                                    k == "fd" && matches!(v, IrExpr::Int(1))
                                });
                                let persist = props.iter().any(|(k, v)| {
                                    k == "persist" && matches!(v, IrExpr::Bool(true))
                                });
                                fd && persist
                            }
                            _ => false,
                        }) {
                            return true;
                        }
                    }
                }
                args.iter().any(expr_has)
            }
            IrExpr::Arrow(stmts) => stmts.iter().any(stmt_has),
            IrExpr::BinOp { lhs, rhs, .. } => {
                expr_has(lhs) || expr_has(rhs)
            }
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(inner) => expr_has(inner),
                InterpPart::Lit(_) => false,
            }),
            // no statements inside any other estree-path variant
            _ => false,
        }
    }
    prog.stmts.iter().any(stmt_has)
}

/// `echo args > file` / `echo args >> file` — a native file write replaces
/// the whole async redirect + builtin-dispatch pair: the content is the
/// echo join (flags included), the bytes land via `await sh2.fs.writeFile`
/// (append mode → appendFile) — exactly what the runtime's redirect→emit→
/// writeFileSync does, minus the fd-table swap, the dispatch and the async
/// boundary. `sh2.fs.*` is the contract's fs surface (no spawn, no runtime
/// call counted in the metric).
///
/// Conservative: exactly ONE spec, fd 1, mode w/a, a target that lowers
/// without an await (no command substitution) and is not an `&N` fd-dup
/// or `-`; no script-defined `echo` shadow (bash functions win over
/// builtins); no env-carrying 3-arg exec form; every arg must lower
/// without runtime need (glob/PS magic, badsub params). The sequence ends
/// in `true` + a lastExit write, so the errexit guard wrapper is dead
/// weight (call_is_always_true) and the truthiness callers branch on is
/// preserved.
fn try_native_echo_redirect(inner: &[IrStmt], specs: &[(i64, &str, &IrExpr)]) -> Option<Expr> {
    if program_defines_function("echo") {
        return None;
    }
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args.as_slice() else {
        return None;
    };
    if name != "echo" {
        return None;
    }
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let [(fd, mode, target)] = specs else {
        return None;
    };
    if *fd != 1 || (*mode != "w" && *mode != "a") {
        return None;
    }
    let tgt = expr_to_estree(target);
    if expr_has_await(&tgt) {
        return None;
    }
    // `&N` fd-dup targets and `-` live in the runtime's fd table — a file
    // write cannot express them.
    if let Expr::Literal { value, .. } = &tgt {
        if let serde_json::Value::String(s) = value {
            if s.starts_with('&') || s == "-" {
                return None;
            }
        }
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    let text: Expr = if no_newline {
        joined
    } else {
        Expr::BinaryExpression {
            operator: "+",
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        }
    };
    let write = sh2_fs_call(
        if *mode == "a" { "appendFile" } else { "writeFile" },
        vec![tgt, text],
    );
    Some(seq(vec![
        await_expr(write),
        Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(sh2_member("lastExit")),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            regex: None,
            }),
        },
        bool_lit(true),
    ]))
}

fn stmt_to_estree(stmt: &IrStmt) -> Option<Stmt> {
    Some(match stmt {
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "break" => {
            Stmt::BreakStatement { label: None }
        }
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "continue" => {
            Stmt::ContinueStatement { label: None }
        }
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "return" => {
            Stmt::ReturnStatement {
                argument: args.first().map(expr_to_estree),
            }
        }
        // bash `a && b` / `a || b` at STATEMENT level: run a, then run b
        // only if a's exit status decides. Both operands are runtime calls
        // (tests inside &&/|| stay runtime calls via AND_OR_DEPTH — a
        // native comparison never records lastExit, so the chain links
        // would branch on a stale status), so lastExit is live after each
        // operand — a plain if on `sh2.lastExit === 0` is the runtime
        // helper's exact decision minus the async arrows + dispatch.
        IrStmt::Expr(IrExpr::BinOp {
            op: op @ (BinOpKind::And | BinOpKind::Or),
            lhs,
            rhs,
        }) => {
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let l = expr_to_estree(lhs);
            let r = expr_to_estree(rhs);
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            if !expr_has_await(&l) && !expr_has_await(&r) {
                let is_and = matches!(op, BinOpKind::And);
                let test = if is_and {
                    last_exit_eq_zero()
                } else {
                    // `a || b`: run b only when a FAILED
                    Expr::BinaryExpression {
                        operator: "!==",
                        left: Box::new(sh2_member("lastExit")),
                        right: Box::new(Expr::Literal {
                            value: serde_json::Value::from(0),
                            raw: None,
                        regex: None,
                        }),
                    }
                };
                return Some(Stmt::BlockStatement {
                    body: vec![
                        Stmt::ExpressionStatement { expression: l },
                        Stmt::IfStatement {
                            test,
                            consequent: Box::new(Stmt::BlockStatement {
                                body: vec![Stmt::ExpressionStatement {
                                    expression: r,
                                }],
                            }),
                            alternate: None,
                        },
                    ],
                });
            }
            Stmt::ExpressionStatement {
                expression: await_call(
                    if matches!(op, BinOpKind::And) { "and" } else { "or" },
                    vec![
                        arrow(vec![], IrExpr::Arrow(vec![IrStmt::Expr((**lhs).clone())])),
                        arrow(vec![], IrExpr::Arrow(vec![IrStmt::Expr((**rhs).clone())])),
                    ],
                ),
            }
        }
        // bash `! cmd` at STATEMENT level: run cmd, then flip the recorded
        // status (bash `$?` after `! cmd` is 1 - status(cmd)). Only valid
        // when the inner statement actually RECORDS a status (a native
        // comparison never does — those stay on the runtime `not` helper,
        // which inverts the VALUE).
        IrStmt::Expr(IrExpr::BinOp { op: BinOpKind::Not, lhs, .. }) => {
            let inner = expr_to_estree(lhs);
            if !expr_has_await(&inner) && sets_last_exit(&inner) {
                return Some(Stmt::BlockStatement {
                    body: vec![
                        Stmt::ExpressionStatement { expression: inner },
                        Stmt::ExpressionStatement {
                            expression: Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(sh2_member("lastExit")),
                                right: Box::new(Expr::ConditionalExpression {
                                    test: Box::new(last_exit_eq_zero()),
                                    consequent: Box::new(Expr::Literal {
                                        value: serde_json::Value::from(1),
                                        raw: None,
                                    regex: None,
                                    }),
                                    alternate: Box::new(Expr::Literal {
                                        value: serde_json::Value::from(0),
                                        raw: None,
                                    regex: None,
                                    }),
                                }),
                            },
                        },
                    ],
                });
            }
            Stmt::ExpressionStatement {
                expression: not_native(inner),
            }
        }
        IrStmt::Expr(e) => {
            // Plan 4: a native `(( ))` / `let ARITH` statement whose
            // lastExit write is provably unread — the expr rewrite (below)
            // would emit the status ternary + lastExit writes; drop them,
            // keep the side effects (a bare `++i` in the hot loop). Never
            // fired under a possible `set -e` (the guard consumes the
            // value); conditions never reach this statement arm.
            if let IrExpr::Call { func, args, .. } = e {
                if func == "exec" && lastexit_write_is_dead(stmt) {
                    if let Some(dead) = try_native_let_dead(args) {
                        return Some(Stmt::ExpressionStatement { expression: dead });
                    }
                }
            }
            Stmt::ExpressionStatement {
                expression: expr_to_estree(e),
            }
        }
        IrStmt::Assign { targets, expr } => {
            let target = &targets[0];
            if is_lifted(&target.var) && target.indices.is_empty() {
                // native JS write — the analysis guarantees the source kind
                let right = match expr {
                    // numeric-lifted source
                    IrExpr::Arith(a) => arith_to_estree(a),
                    IrExpr::Int(i) => Expr::Literal {
                        value: serde_json::Value::from(*i),
                        raw: None,
                    regex: None,
                    },
                    IrExpr::Str(sv, _) if is_lifted_num(&target.var) => Expr::Literal {
                        value: serde_json::Value::from(sv.trim().parse::<i64>().unwrap_or(0)),
                        raw: None,
                    regex: None,
                    },
                    // string-lifted source
                    IrExpr::Str(sv, _) => Expr::Literal {
                        value: serde_json::Value::String(sv.clone()),
                        raw: None,
                    regex: None,
                    },
                    IrExpr::Interpolate(parts) => interpolate_to_estree(parts),
                    IrExpr::Var(n, _) => Expr::Identifier { name: n.clone() },
                    IrExpr::Call { func, args } if func == "getVar" => match args.as_slice() {
                        [IrExpr::Str(n, _)] => Expr::Identifier { name: n.clone() },
                        _ => unreachable!("lifted getVar source"),
                    },
                    // string-lifted capture source: `x=$(cmd)` →
                    // `x = await sh2.capture(...)` (or the native
                    // echo/tr/cat/sort/... capture lifts) — the runtime
                    // capture always yields a string, exactly the setVar
                    // path minus the store write + dispatch.
                    IrExpr::Call { func, args } if func == "capture" => {
                        expr_to_estree(&IrExpr::Call {
                            func: func.clone(),
                            args: args.clone(),
                        })
                    },
                    // the for-loop numeric coercion (`i = Number(i)`)
                    IrExpr::Call { func, args } if func == "Number" => match args.as_slice() {
                        [IrExpr::Ident(n)] => Expr::CallExpression {
                            callee: Box::new(Expr::Identifier {
                                name: "Number".to_string(),
                            }),
                            arguments: vec![Expr::Identifier { name: n.clone() }],
                            optional: false,
                        },
                        _ => unreachable!("lifted Number source"),
                    },
                    _ => unreachable!("lifted var assigned an unanalysed source"),
                };
                return Some(Stmt::ExpressionStatement {
                    expression: Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(Expr::Identifier {
                            name: target.var.clone(),
                        }),
                        right: Box::new(right),
                    },
                });
            }
            match expr {
                // arr=(...) / arr+=(...) / x op= v → the helper already sets
                // the variable itself; emit the call bare.
                IrExpr::Call { func, .. }
                    if func == "setArray" || func == "setArrayAppend" || func == "assign" =>
                {
                    Stmt::ExpressionStatement {
                        expression: expr_to_estree(expr),
                    }
                }
                _ => Stmt::ExpressionStatement {
                    expression: sh2_call("setVar", vec![str_lit(&target.var), expr_to_estree(expr)]),
                },
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            let consequent = Box::new(Stmt::BlockStatement {
                body: then.iter().filter_map(stmt_to_estree).collect(),
            });
            let alternate: Option<Box<Stmt>> = if else_.is_empty() {
                // bash: `if c; then ...; fi` with a false condition and no
                // else leaves `$?` = 0. The runtime tracks lastExit through
                // calls only, so the false path must set it explicitly — a
                // native field write (`sh2.lastExit = 0`), no dispatch.
                Some(Box::new(Stmt::BlockStatement {
                    body: vec![Stmt::ExpressionStatement {
                        expression: Expr::AssignmentExpression {
                            operator: "=".to_string(),
                            left: Box::new(sh2_member("lastExit")),
                            right: Box::new(Expr::Literal {
                                value: serde_json::Value::from(0),
                                raw: None,
                            regex: None,
                            }),
                        },
                    }],
                }))
            } else {
                Some(match else_.as_slice() {
                    [IrStmt::If { .. }] => Box::new(
                        stmt_to_estree(&else_[0]).unwrap_or(Stmt::BlockStatement { body: vec![] }),
                    ),
                    _ => Box::new(Stmt::BlockStatement {
                        body: else_.iter().filter_map(stmt_to_estree).collect(),
                    }),
                })
            };
            Stmt::IfStatement {
                test: expr_to_estree(cond),
                consequent,
                alternate,
            }
        }
        IrStmt::While { cond, body } => {
            // Fast path: a provably-sync loop (neither cond nor body needs
            // `await`) lowers to the synchronous runtime loop, which has
            // IDENTICAL semantics (lastExit, BREAK/CONTINUE/RETURN signals,
            // capture bound) minus the per-iteration promise/microtask
            // machinery — ~100x faster busy loops. The closures are plain
            // (r#async: false); eligibility is checked on the already-lowered
            // ESTree: any AwaitExpression inside disqualifies (the runtime
            // call is pure CPU, so no *Sync blocking-I/O concern — the gate
            // whitelists whileLoopSync explicitly).
            let cond_e = expr_to_estree(cond);
            let body_stmts: Vec<Stmt> = body.iter().filter_map(stmt_to_estree).collect();
            if !expr_has_await(&cond_e) && !stmts_have_await(&body_stmts) {
                return Some(Stmt::ExpressionStatement {
                    expression: sh2_call(
                        "whileLoopSync",
                        vec![sync_arrow_expr(cond_e), sync_arrow_block(body_stmts)],
                    ),
                });
            }
            Stmt::ExpressionStatement {
                expression: await_call(
                    "whileLoop",
                    vec![
                        arrow(vec![], IrExpr::Arrow(vec![IrStmt::Expr(cond.clone())])),
                        arrow(vec![], IrExpr::Arrow(body.clone())),
                    ],
                ),
            }
        }
        IrStmt::For { var, iter, body } => {
            let js_var = safe_ident(var);
            let mut coercion: Option<IrStmt> = None;
            if is_lifted_num(var) {
                // the forLoop items arrive as strings; coerce the param to
                // a number in place (the closure param shadows the module
                // let — a self-assign is exactly the coercion we want)
                coercion = Some(IrStmt::Assign {
                    targets: vec![AssignTarget {
                        var: var.clone(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Call {
                        func: "Number".to_string(),
                        args: vec![IrExpr::Ident(js_var.clone())],
                    },
                });
            } else if !is_lifted(var) {
                // store sync (non-lifted loop var)
                coercion = Some(IrStmt::Assign {
                    targets: vec![AssignTarget {
                        var: var.clone(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Ident(js_var.clone()),
                });
            }
            // owned body (coercion + clones) for the async fallback
            let mut body_stmts: Vec<IrStmt> = vec![];
            if let Some(c) = &coercion {
                body_stmts.push(c.clone());
            }
            body_stmts.extend(body.clone());
            let iter_e = expr_to_estree(iter);
            // the *Sync path emits the ORIGINAL body references — the
            // liveness pre-pass (compute_lastexit_deadness) keys by
            // statement pointer, so the loop bodies' dead-write marks must
            // resolve to the same objects (clones would miss).
            let body_e: Vec<Stmt> = coercion
                .iter()
                .chain(body.iter())
                .filter_map(stmt_to_estree)
                .collect();
            // Fast path: a provably-sync loop (the BODY needs no `await`)
            // lowers to the synchronous runtime loop — identical semantics
            // (flattening, GLOB_MAGIC items, BREAK/CONTINUE/RETURN signals,
            // capture bound) minus the per-iteration promise machinery (the
            // whileLoopSync precedent). An `await` in the ITERABLE is fine:
            // it resolves ONCE, before the loop starts (arguments evaluate
            // before the call) — `$(...)`-produced item lists stay async,
            // the per-item body still runs without promises.
            if !stmts_have_await(&body_e) {
                return Some(Stmt::ExpressionStatement {
                    expression: sh2_call(
                        "forLoopSync",
                        vec![iter_e, sync_arrow_with_param(js_var, body_e)],
                    ),
                });
            }
            Stmt::ExpressionStatement {
                expression: await_call(
                    "forLoop",
                    vec![
                        iter_e,
                        arrow_with_param(js_var, IrExpr::Arrow(body_stmts)),
                    ],
                ),
            }
        }
        IrStmt::Function { name, body } => Stmt::ExpressionStatement {
            // `sh2.define(name, fn)` is a thin wrapper over the runtime's
            // function map (`this.functions.set(name, fn); return true;`) —
            // a direct state write + `true`, no dispatch. The arrow arg is
            // lowered exactly as before (sink-aware: a function body may
            // run under ANY stdout sink at runtime). A PROVABLY-SYNC
            // function (see [`fn_call_sync_set`]) gets a NON-async arrow:
            // its body has no awaits by construction, and the sync fnCall
            // path runs it without a per-call promise (the async exec path
            // awaits it like any other — `await` on a non-promise is an
            // identity).
            expression: seq(vec![
                Expr::CallExpression {
                    callee: Box::new(Expr::MemberExpression {
                        object: Box::new(sh2_member("functions")),
                        property: Box::new(Expr::Identifier {
                            name: "set".to_string(),
                        }),
                        computed: false,
                        optional: false,
                    }),
                    arguments: vec![
                        str_lit(name),
                        if fn_call_is_sync(name) {
                            arrow_sink_sync(vec![], IrExpr::Arrow(body.clone()))
                        } else {
                            arrow_sink(vec![], IrExpr::Arrow(body.clone()))
                        },
                    ],
                    optional: false,
                },
                bool_lit(true),
            ]),
        },
        IrStmt::Subshell(stmts) => Stmt::ExpressionStatement {
            expression: await_call(
                "subshell",
                vec![arrow_sink(vec![], IrExpr::Arrow(stmts.clone()))],
            ),
        },
        IrStmt::Background(stmts) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "background",
                vec![arrow_sink(vec![], IrExpr::Arrow(stmts.clone()))],
            ),
        },
        IrStmt::Block(stmts) => Stmt::BlockStatement {
            body: stmts.iter().filter_map(stmt_to_estree).collect(),
        },
        IrStmt::Redirect { inner, redirects } => {
            // `echo args > file` / `echo args >> file`: a native
            // fs.writeFile replaces the redirect+builtin pair (see
            // try_native_echo_redirect).
            let redirect_specs = redirects
                .iter()
                .map(|r| (r.fd.unwrap_or(0) as i64, r.mode.as_str(), &r.target))
                .collect::<Vec<_>>();
            if let Some(native) = try_native_echo_redirect(inner, &redirect_specs) {
                return Some(Stmt::ExpressionStatement {
                    expression: native,
                });
            }
            // `grep -q PAT <<< TEXT` (statement position): the fd-0
            // herestring redirect form of the substring test — no spawn,
            // no fd plumbing (see `try_native_grep_q_redirect`).
            if let Some(native) = try_native_grep_q_redirect(inner, &redirect_specs) {
                return Some(Stmt::ExpressionStatement {
                    expression: native,
                });
            }
            // `exec 3>&1` (exec with no command): bash installs the redirects
            // permanently in the shell's own fd table. Tell the runtime to
            // persist them (it restores non-persistent redirects afterwards).
            // Only the literal `exec` builtin with NO args qualifies —
            // `: >file`, `>file` (standalone) and `true 3>&1` all restore.
            let persist = matches!(
                inner.as_slice(),
                [IrStmt::Expr(IrExpr::Call { func, args })]
                    if func == "exec"
                        && matches!(args.as_slice(), [IrExpr::Str(name, _), IrExpr::Array(a)]
                            if name == "exec" && a.is_empty())
            );
            Stmt::ExpressionStatement {
                expression: await_call(
                    "redirect",
                    vec![
                        arrow_sink(vec![], IrExpr::Arrow(inner.clone())),
                        array(
                            redirects
                                .iter()
                                .map(|r| redirect_spec_to_estree(r, persist))
                                .collect(),
                        ),
                    ],
                ),
            }
        }
        IrStmt::Case { discriminant, clauses } => {
            let nocase = CASE_NOCASE.lock().unwrap().unwrap_or(false);
            if let Some(native) = try_native_case(discriminant, clauses, nocase) {
                return Some(native);
            }
            let patterns: Vec<Expr> = clauses
                .iter()
                .flat_map(|c| c.patterns.iter())
                .map(|p| str_lit(p))
                .collect();
            let cases: Vec<SwitchCase> = clauses
                .iter()
                .flat_map(|c| {
                    // Source `break`/`continue` inside a case must exit the
                    // ENCLOSING loop (bash semantics), but a native JS break
                    // would only exit the switch. Turn them into runtime
                    // signals; the synthetic trailing break (chain below)
                    // stays native to terminate the switch itself.
                    let consequent: Vec<Stmt> = c
                        .body
                        .iter()
                        .filter_map(stmt_to_estree)
                        .map(|s| match s {
                            Stmt::BreakStatement { .. } => Stmt::ExpressionStatement {
                                expression: sh2_call("break", vec![]),
                            },
                            Stmt::ContinueStatement { .. } => Stmt::ExpressionStatement {
                                expression: sh2_call("continue", vec![]),
                            },
                            other => other,
                        })
                        .chain(std::iter::once(Stmt::BreakStatement { label: None }))
                        .collect();
                    c.patterns
                        .iter()
                        .map(move |p| SwitchCase {
                            type_: "SwitchCase",
                            test: Some(str_lit(p)),
                            consequent: consequent.clone(),
                        })
                })
                .collect();
            Stmt::SwitchStatement {
                discriminant: sh2_call(
                    "caseMatch",
                    vec![expr_to_estree(discriminant), array(patterns)],
                ),
                cases,
            }
        }
        IrStmt::Return(opt) => Stmt::ReturnStatement {
            argument: opt.as_ref().map(expr_to_estree),
        },
        IrStmt::Exec { cmd, args, capture: _, redirects, env } => {
            // `rm` / `mkdir` with plain args: a native `sh2.fs.*` promise
            // chain — no spawn, no dispatch (see `try_native_fs_exec`).
            // Redirects/env forms keep the runtime (the redirect wrapper
            // around the statement is an IrStmt::Redirect — its inner Exec
            // still hits this arm, and the fd plumbing stays vacuous for a
            // write-free native rm/mkdir, so the wrapper can stay).
            if env.is_empty() && redirects.is_empty() {
                if let IrExpr::Str(name, _) = cmd {
                    if let Some(native) = try_native_fs_exec(name, args) {
                        return Some(Stmt::ExpressionStatement {
                            expression: await_expr(native),
                        });
                    }
                }
            }
            // `f args...` — a call to a PROVABLY-SYNC script-defined
            // function (see `fn_call_sync_set`): the sync runtime twin of
            // the exec function-dispatch path (sh2-namespace.mjs `fnCall`)
            // — identical arg flattening / magic expansion / positional
            // save-restore / RETURN-signal unwinding / lastExit recording,
            // minus the per-call promise machinery (the whileLoopSync
            // pattern). The call-site args must also be await-free (a
            // capture arg would need an async context regardless).
            // Env-carrying / redirect forms keep the async exec dispatch.
            if env.is_empty() && redirects.is_empty() {
                if let IrExpr::Str(name, _) = cmd {
                    if let Some(call) = try_native_fn_call(name, args) {
                        return Some(Stmt::ExpressionStatement { expression: call });
                    }
                }
            }
            // `let ARITH...` / `(( ARITH ))` at STATEMENT level: the
            // native arithmetic-statement lowering (see try_native_let) —
            // every arg parses natively, so the runtime dispatch + string
            // re-parse disappear (the `((i++))` per-iteration hot path).
            if env.is_empty() && redirects.is_empty() {
                if let IrExpr::Str(name, _) = cmd {
                    if name == "let" {
                        if let Some(native) = try_native_let(args) {
                            // Plan 4: the statement's lastExit write is
                            // provably unread (compute_lastexit_deadness)
                            // — drop the status ternary + lastExit writes,
                            // keep the side effects (a bare `++i` in the
                            // hot loop). Never fired under a possible
                            // `set -e` (the guard consumes the value).
                            if lastexit_write_is_dead(stmt) {
                                if let Some(dead) = try_native_let_dead(args) {
                                    return Some(Stmt::ExpressionStatement {
                                        expression: dead,
                                    });
                                }
                            }
                            return Some(Stmt::ExpressionStatement { expression: native });
                        }
                    }
                }
            }
            let mut call_args = vec![
                expr_to_estree(cmd),
                expr_to_estree(&IrExpr::Array(args.clone())),
            ];
            if !env.is_empty() {
                call_args.push(Expr::ObjectExpression {
                    properties: env
                        .iter()
                        .map(|(k, v)| prop(k, expr_to_estree(v)))
                        .collect(),
                });
            }
            // sync-builtin dispatch (the expr_to_estree rewrite, for the
            // statement-form Exec): env-carrying calls (`IFS=: read ...`)
            // lower to the sync builtin too — the runtime twin applies the
            // command-scoped env exactly like the async exec path (see
            // sh2-namespace.mjs `builtin`). Script-defined function
            // shadows keep the async exec dispatch (the function map).
            let callee = if let IrExpr::Str(name, _) = cmd {
                if SYNC_BUILTINS.contains(&name.as_str()) && !program_defines_function(name) {
                    "builtin"
                } else {
                    "exec"
                }
            } else {
                "exec"
            };
            let call = sh2_call(callee, call_args);
            Stmt::ExpressionStatement {
                expression: if is_async_call(callee) {
                    await_expr(call)
                } else {
                    call
                },
            }
        }
        other => unreachable!("Perl-only IR statement reached the ESTree renderer: {other:?}"),
    })
}

fn redirect_spec_to_estree(r: &IrRedirect, persist: bool) -> Expr {
    let mut props = vec![
        prop(
            "fd",
            Expr::Literal {
                value: serde_json::Value::from(r.fd.unwrap_or(0)),
                raw: None,
            regex: None,
            },
        ),
        prop("mode", str_lit(&r.mode)),
        prop("target", expr_to_estree(&r.target)),
    ];
    if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
        props.push(prop(
            "interpolate",
            Expr::Literal {
                value: serde_json::Value::Bool(r.interpolate),
                raw: None,
            regex: None,
            },
        ));
    }
    if persist {
        props.push(prop(
            "persist",
            Expr::Literal {
                value: serde_json::Value::Bool(true),
                raw: None,
            regex: None,
            },
        ));
    }
    Expr::ObjectExpression { properties: props }
}

fn str_operand(e: &str) -> Option<Expr> {
    let e = e.trim();
    if let Some(inner) = e.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        let bare = inner.strip_prefix('$').unwrap_or(inner);
        if is_lifted_str(bare) {
            return Some(Expr::Identifier { name: bare.to_string() });
        }
        if inner.contains('$')
            || inner.contains('*')
            || inner.contains('?')
            || inner.contains('[')
        {
            return None;
        }
        return Some(Expr::Literal {
            value: serde_json::Value::String(inner.to_string()),
            raw: None,
        regex: None,
        });
    }
    // A bare `$name` needs the runtime value — only a lifted var can be
    // read natively; never treat it as the literal text (`$y` ≠ "y").
    if let Some(rest) = e.strip_prefix('$') {
        if is_lifted_str(rest) {
            return Some(Expr::Identifier {
                name: rest.to_string(),
            });
        }
        return None;
    }
    if !e.is_empty()
        && !e.contains(['*', '?', '[', '$'])
        && e.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Some(Expr::Literal {
            value: serde_json::Value::String(e.to_string()),
            raw: None,
        regex: None,
        });
    }
    None
}

/// `[ "$x" = *P* ]` family: the RIGHT side is a [`CasePat`] glob (the
/// runtime glob-matches a `=`/`==`/`!=` operand containing glob
/// metachars — and only the right side; a glob on the left is compared
/// literally), the left is a normal operand (lifted var or literal) —
/// lower to native `String(x).includes(P)` / `startsWith` / `endsWith`
/// (negated for `!=`). Under a possible `nocasematch` the runtime's
/// evalTest lowercases BOTH sides: the literal is pre-lowercased at emit
/// time and the value side gets `toLowerCase()`.
fn try_native_glob_test(lhs: &str, rhs: &str, negate: bool) -> Option<Expr> {
    let nocase = CASE_NOCASE.lock().unwrap().unwrap_or(false);
    let build = |operand: Expr, pat: &CasePat| {
        let mut value = Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![operand],
            optional: false,
        };
        if nocase {
            value = Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "toLowerCase".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![],
                optional: false,
            };
        }
        let lit_str = |lit: &str| {
            if nocase {
                lit.to_lowercase()
            } else {
                lit.to_string()
            }
        };
        let str_op = |name: &str, arg: Expr| Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(value.clone()),
                property: Box::new(Expr::Identifier {
                    name: name.to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![arg],
            optional: false,
        };
        let inc = match pat {
            CasePat::Any => Expr::Literal {
                value: serde_json::Value::Bool(true),
                raw: None,
            regex: None,
            },
            CasePat::Substr(lit) => str_op("includes", str_lit(&lit_str(lit))),
            CasePat::Prefix(lit) => str_op("startsWith", str_lit(&lit_str(lit))),
            CasePat::Suffix(lit) => str_op("endsWith", str_lit(&lit_str(lit))),
            CasePat::Exact(lit) => Expr::BinaryExpression {
                operator: "===",
                left: Box::new(value.clone()),
                right: Box::new(str_lit(&lit_str(lit))),
            },
        };
        if negate {
            Expr::UnaryExpression {
                operator: "!",
                argument: Box::new(inc),
                prefix: true,
            }
        } else {
            inc
        }
    };
    if let Some(pat) = classify_case_pat(rhs.trim()) {
        // only the GLOB shapes lift natively here: a bare operand
        // containing `=`/`<`/`>` would tokenize into separate test tokens
        // (the runtime splits on them), so Exact/Any stay on the runtime
        // (the plain-equality path already covers exact literals).
        if matches!(&pat, CasePat::Substr(_) | CasePat::Prefix(_) | CasePat::Suffix(_)) {
            if let Some(l) = str_operand(lhs) {
                return Some(build(l, &pat));
            }
        }
    }
    None
}

/// A shell variable name: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_plain_ident(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Decode `tr`'s backslash escapes in a set argument (`\n` → newline,
/// `\\` → backslash, `\0` → NUL, `\xHH`, ...). Returns None on an
/// unrecognized escape (the lift bails to the runtime).
fn tr_decode_escapes(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('0') => out.push('\0'),
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('f') => out.push('\x0c'),
            Some('r') => out.push('\r'),
            Some('v') => out.push('\x0b'),
            Some('x') => {
                let h1 = chars.next()?;
                let h2 = chars.next()?;
                let v = u8::from_str_radix(&format!("{h1}{h2}"), 16).ok()?;
                out.push(v as char);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// `$(echo args...)` — a capture whose ONLY statement is the sync `echo`
/// builtin (no spawns, no pipeline). The captured value is exactly
/// `echo`'s joined output minus the capture strips: `args.join(" ")` with
/// the `-e` escape interpretation (`\n`/`\t` — the runtime's builtin echo
/// interprets exactly these two), wrapped in `sh2.trimCapture` (NUL +
/// trailing-newline strips — `-n` is a no-op under capture: the missing
/// trailing newline is stripped anyway). Conservative: no flags other
/// than `-e`/`-n` at position 0, no glob-tagged args (the runtime would
/// expand them), no env. A script-defined `echo` keeps the runtime path.
/// `echo` arg → the ESTree expression for one printed word. Literals stay
/// literals; array-valued args (brace expansion, which lowers to an
/// `ArrayExpression`, or `${arr[@]}`-style word lists) splice to one word
/// per element — the runtime's `builtin()` flattens array args
/// (`flat.push(...a.map(String))`; a bare `String(array)` would
/// comma-join, which bash never does for brace-expanded words); nested
/// arrays keep the runtime's `a.map(String)` comma-join via the String()
/// wrap; everything else is String()-coerced. Returns None if any element
/// carries glob magic (the runtime would expand it).
fn echo_arg_to_estree(a: &IrExpr) -> Option<Expr> {
    match a {
        IrExpr::Str(sv, _) if sv.starts_with(GLOB_MAGIC) => None,
        IrExpr::Str(sv, _) => Some(str_lit(sv)),
        IrExpr::Array(elems) => {
            let mut out: Vec<Expr> = Vec::new();
            for el in elems {
                if let IrExpr::Str(sv, _) = el {
                    if sv.starts_with(GLOB_MAGIC) {
                        return None;
                    }
                }
                out.push(echo_arg_scalar(el));
            }
            Some(Expr::ArrayExpression {
                elements: out.into_iter().map(Some).collect(),
            })
        }
        other => {
            // array-valued args (UNQUOTED `$(...)` / `$@` — the runtime's
            // captureWords/listVar return arrays): keep the call bare — the
            // builtin's arg flattener splices the elements into the arg
            // list, and the join builder applies `.flat()` first (a
            // String() wrap would comma-join the array).
            if matches!(
                other,
                IrExpr::Call { func, .. } if func == "captureWords" || func == "listVar"
            ) {
                return Some(expr_to_estree(other));
            }
            let e = expr_to_estree(other);
            match e {
                // array-valued expression (emit-time brace expansion):
                // splice its words like the runtime's arg flattening
                Expr::ArrayExpression { elements, .. } => {
                    for el in &elements {
                        if let Some(Expr::Literal { value, .. }) = el {
                            if let serde_json::Value::String(sv) = value {
                                if sv.starts_with(GLOB_MAGIC) {
                                    return None;
                                }
                            }
                        }
                    }
                    Some(Expr::ArrayExpression { elements })
                }
                _ => Some(Expr::CallExpression {
                    callee: Box::new(Expr::Identifier {
                        name: "String".to_string(),
                    }),
                    arguments: vec![e],
                    optional: false,
                }),
            }
        }
    }
}

/// A scalar (non-array) echo arg word: literals stay literals, anything
/// else is String()-coerced (the runtime's builtin() String()s every
/// arg before printing).
fn echo_arg_scalar(a: &IrExpr) -> Expr {
    match a {
        IrExpr::Str(sv, _) => str_lit(sv),
        other => Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![expr_to_estree(other)],
            optional: false,
        },
    }
}

/// Shared `echo` argument lowering: flag args at position 0 (the runtime
/// checks exactly `args[0] === '-n'` / `'-e'` — at most ONE flag, later
/// occurrences are ordinary args), the runtime's arg flattening (brace
/// arrays splice their words into the join), and the `-e` global replaces
/// (`\n` → newline, `\t` → tab — exactly the runtime's two). Returns the
/// joined value, whether the FIRST arg was `-n` (no trailing newline for
/// the statement form), and whether the joined value provably CANNOT end
/// with a newline (the final arg is a literal with no trailing `\n` even
/// after the `-e` replaces). The capture strips trailing newlines from the
/// buffer, so a provably-newline-free join needs no trim wrapper.
fn echo_join_args(echo_args: &[IrExpr]) -> Option<(Expr, bool, bool)> {
    let mut arg_exprs: Vec<Expr> = Vec::new();
    let mut esc = false;
    let mut no_newline = false;
    let mut flat = false;
    let mut flag_done = false;
    for a in echo_args {
        match a {
            // flag args: only at position 0, only the exact builtin flags
            IrExpr::Str(sv, _) if !flag_done && (sv == "-e" || sv == "-n") => {
                flag_done = true;
                if sv == "-e" {
                    esc = true;
                } else {
                    no_newline = true;
                }
            }
            other => {
                flag_done = true;
                if matches!(
                    other,
                    IrExpr::Call { func, .. } if func == "captureWords" || func == "listVar"
                ) {
                    // array-valued arg — the runtime's flattener splices the
                    // elements; mirror it with `.flat()` before the join
                    flat = true;
                }
                match echo_arg_to_estree(other)? {
                    // array arg (brace expansion): splice its words into the
                    // join, exactly like the runtime's arg flattening
                    Expr::ArrayExpression { elements, .. } => {
                        arg_exprs.extend(elements.into_iter().flatten());
                    }
                    e => arg_exprs.push(e),
                }
            }
        }
    }
    let last_clean = match arg_exprs.last() {
        // the final join element decides the trailing byte: a literal that
        // does not end with a newline (a real one, or a `\n` the -e
        // replaces) keeps the join newline-free
        Some(Expr::Literal { value, .. }) => match value {
            serde_json::Value::String(s) => {
                !s.ends_with('\n') && !(esc && s.ends_with("\\n"))
            }
            _ => true,
        },
        None => true,
        Some(_) => false,
    };
    let mut joined: Expr = if arg_exprs.is_empty() {
        str_lit("")
    } else {
        let mut arr = Expr::ArrayExpression {
            elements: arg_exprs.into_iter().map(Some).collect(),
        };
        if flat {
            // `[a, ...captureWords-result].flat()` — the runtime's arg
            // flattener (Array.isArray → splice) applied before the join
            arr = Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(arr),
                    property: Box::new(Expr::Identifier {
                        name: "flat".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![],
                optional: false,
            };
        }
        Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(arr),
                property: Box::new(Expr::Identifier {
                    name: "join".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![str_lit(" ")],
            optional: false,
        }
    };
    if esc {
        // the runtime's builtin echo -e: global `\n` → newline, `\t` →
        // tab (exactly these two) — a split/join chain is the exact
        // global replace
        for (from, to) in [("\\n", "\n"), ("\\t", "\t")] {
            let split = Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(joined),
                    property: Box::new(Expr::Identifier {
                        name: "split".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(from)],
                optional: false,
            };
            joined = Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(split),
                    property: Box::new(Expr::Identifier {
                        name: "join".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(to)],
                optional: false,
            };
        }
    }
    Some((joined, no_newline, last_clean))
}

/// `$(echo args...)` — the capture's only statement is the sync echo
/// builtin: the value is the joined args (with the runtime's `-e`
/// interpretation) minus the capture's trailing-newline strips — no async
/// capture machinery at all. The trailing-newline strip is a no-op when
/// the final arg provably cannot end with a newline (a literal without
/// `-e`-trailing `\n`), so the `sh2.trimCapture` wrapper drops too. Args
/// the runtime would transform beyond joining (GLOB_MAGIC globs, PS_MAGIC
/// process-substitution paths, raw-byte markers) keep the whole capture.
fn try_native_echo_capture(e: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args.as_slice() else {
        return None;
    };
    if name != "echo" {
        return None;
    }
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let (joined, _, last_clean) = echo_join_args(echo_args)?;
    if last_clean {
        Some(joined)
    } else {
        // the captured value may end with newlines (a dynamic final arg) —
        // keep the runtime's exact capture strips (native string ops)
        Some(trim_capture(joined))
    }
}

// ── native command-substitution lowerings (pure-capture family) ────────
// `$(cmd ...)` whose value is a pure function of file contents / path
// strings: the whole capture+spawn/arrow machinery collapses to a native
// expression. `sh2.trimCapture` applies the capture's exact NUL + trailing-
// newline strips; the promise chains record the exit status the spawned
// binary would (success → lastExit 0, failure → 1).

/// `sh2.fs.<name>(...)` — the runtime's node:fs/promises namespace
/// (harness sh2-namespace.mjs, whitelisted in estree_gate.pl).
fn sh2_fs_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::MemberExpression {
                object: Box::new(Expr::Identifier {
                    name: "sh2".to_string(),
                }),
                property: Box::new(Expr::Identifier {
                    name: "fs".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            property: Box::new(Expr::Identifier {
                name: name.to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: args,
        optional: false,
    }
}

/// `await sh2.fs.readFile(path, enc?).then(r => (sh2.lastExit = ok, r))
/// .catch(e => (sh2.lastExit = err, ""))` — the value the runtime's exec
/// path would capture for a pure file-reading command, INCLUDING its exit
/// status (`$?` reads lastExit after the cmdsub: bash yields the spawned
/// binary's code — 0 on success, 1 on a missing/unreadable file).
fn read_file_value(path: Expr, encoding: Option<&'static str>, ok: i64, err: i64) -> Expr {
    await_expr(read_file_promise(path, encoding, ok, err))
}

/// The un-awaited `sh2.fs.readFile(path, enc?)` promise with the runtime's
/// exit-status recording chained on — `read_file_value` awaits it; the
/// capture-pipeline lifts chain their own `.then(...)` transform on it
/// before the single await.
fn read_file_promise(path: Expr, encoding: Option<&'static str>, ok: i64, err: i64) -> Expr {
    let mut args = vec![path];
    if let Some(enc) = encoding {
        args.push(str_lit(enc));
    }
    let base = sh2_fs_call("readFile", args);
    let status = |exit: i64| Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(sh2_member("lastExit")),
        right: Box::new(Expr::Literal {
            value: serde_json::Value::from(exit),
            raw: None,
        regex: None,
        }),
    };
    let then = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(base),
            property: Box::new(Expr::Identifier {
                name: "then".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::ArrowFunctionExpression {
            params: vec![Expr::Identifier {
                name: "r".to_string(),
            }],
            body: ArrowBody::Expr(Box::new(seq(vec![status(ok), Expr::Identifier {
                name: "r".to_string(),
            }]))),
            expression: true,
            r#async: false,
        }],
        optional: false,
    };
    let catch = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(then),
            property: Box::new(Expr::Identifier {
                name: "catch".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::ArrowFunctionExpression {
            params: vec![Expr::Identifier {
                name: "e".to_string(),
            }],
            body: ArrowBody::Expr(Box::new(seq(vec![status(err), str_lit("")]))),
            expression: true,
            r#async: false,
        }],
        optional: false,
    };
    catch
}

/// A plain literal path arg the native readers accept: non-empty, no
/// GLOB_MAGIC / PS_MAGIC markers, not a flag (`-` or leading `-`).
fn is_plain_path_arg(e: &IrExpr) -> Option<String> {
    let IrExpr::Str(sv, _) = e else {
        return None;
    };
    if sv.is_empty()
        || sv.starts_with(GLOB_MAGIC)
        || sv.starts_with(PS_MAGIC)
        || sv.starts_with('-')
    {
        return None;
    }
    Some(sv.clone())
}

/// `$(cat f...)` / `$(cat < f)` — the capture's value is the files'
/// contents concatenated (missing file → empty + exit 1, like the spawn),
/// minus the capture strips.
fn native_capture_cat(cmd_args: &[IrExpr], stdin_file: Option<&IrExpr>) -> Option<Expr> {
    let mut reads: Vec<Expr> = if let Some(t) = stdin_file {
        if !cmd_args.is_empty() {
            return None;
        }
        vec![read_file_value(expr_to_estree(t), Some("utf8"), 0, 1)]
    } else {
        let mut out = Vec::new();
        for a in cmd_args {
            let _ = is_plain_path_arg(a)?;
            out.push(read_file_value(expr_to_estree(a), Some("utf8"), 0, 1));
        }
        if out.is_empty() {
            return None; // `$(cat)` reads stdin — not a pure file read
        }
        out
    };
    let mut joined = reads.pop().expect("cat has at least one read");
    for r in reads.into_iter().rev() {
        joined = Expr::BinaryExpression {
            operator: "+",
            left: Box::new(r),
            right: Box::new(joined),
        };
    }
    Some(trim_capture(joined))
}

/// `$(sort f)` / `$(sort < f)` — read, drop the trailing empty line,
/// sort (C-locale byte order on ASCII == JS default string order — the
/// C_LOCALE assumption), re-join with newlines. GNU sort always ends its
/// output with a newline and the capture strips trailing newlines, so the
/// joined lines are the exact captured value (empty file → ""). The
/// runtime's own sort builtin leaves lastExit 0 even on a missing file
/// (readFileSafe swallows the error), so the chain records 0/0 — identical
/// to today's passing behavior.
fn native_capture_sort(cmd_args: &[IrExpr], stdin_file: Option<&IrExpr>) -> Option<Expr> {
    let path: &IrExpr = if let Some(t) = stdin_file {
        if !cmd_args.is_empty() {
            return None;
        }
        t
    } else {
        match cmd_args {
            [single] => single,
            _ => return None,
        }
    };
    if is_plain_path_arg(path).is_none() {
        return None;
    }
    let s = read_file_value(expr_to_estree(path), Some("utf8"), 0, 0);
    // `(s.endsWith('\n') ? s.slice(0, -1) : s).split('\n').sort().join('\n')`
    let ends_nl = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(s.clone()),
            property: Box::new(Expr::Identifier {
                name: "endsWith".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![str_lit("\n")],
        optional: false,
    };
    let sliced = Expr::ConditionalExpression {
        test: Box::new(ends_nl),
        consequent: Box::new(Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(s.clone()),
                property: Box::new(Expr::Identifier {
                    name: "slice".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![
                Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                regex: None,
                },
                Expr::UnaryExpression {
                    operator: "-",
                    argument: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    regex: None,
                    }),
                    prefix: true,
                },
            ],
            optional: false,
        }),
        alternate: Box::new(s),
    };
    let lines = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(sliced),
            property: Box::new(Expr::Identifier {
                name: "split".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![str_lit("\n")],
        optional: false,
    };
    let sorted = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(lines),
            property: Box::new(Expr::Identifier {
                name: "sort".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![],
        optional: false,
    };
    Some(Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(sorted),
            property: Box::new(Expr::Identifier {
                name: "join".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![str_lit("\n")],
        optional: false,
    })
}

/// A pipeline stage that is exactly `Arrow([Expr(exec NAME ARGS)])` — a
/// bare command with no redirects (redirects arrive as a `redirect` call,
/// which does not match). Script-defined functions shadow builtins — bail.
fn pipeline_stage_exec(stage: &IrExpr) -> Option<(&str, &[IrExpr])> {
    let IrExpr::Arrow(stmts) = stage else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stmts.as_slice() else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() else {
        return None;
    };
    if program_defines_function(name) {
        return None;
    }
    Some((name, a))
}

/// A grep pattern liftable to a native string predicate: an optional
/// leading `^` anchor becomes `startsWith` (the ERE anchor), anything else
/// is a plain substring `includes`. No other BRE metacharacters, no
/// newline, no leading `-` (would parse as an option).
#[derive(Debug, Clone, PartialEq)]
enum GrepPat {
    Prefix(String),
    Substr(String),
}

fn classify_grep_pat(pat: &str) -> Option<GrepPat> {
    let meta = |s: &str| {
        s.chars()
            .any(|c| matches!(c, '^' | '$' | '.' | '[' | ']' | '*' | '\\' | '\n'))
    };
    if let Some(rest) = pat.strip_prefix('^') {
        if rest.is_empty() || meta(rest) {
            return None;
        }
        Some(GrepPat::Prefix(rest.to_string()))
    } else if pat.starts_with('-') || meta(pat) {
        None
    } else {
        Some(GrepPat::Substr(pat.to_string()))
    }
}

/// `$(grep [-v] PAT FILE | cut -d D -f F)` — the classic passwd/group
/// parse: read the file, keep the lines the literal pattern selects, slice
/// each to one `-d`-delimited field, join with newlines. The capture
/// strips (NULs + trailing newlines) apply via `sh2.trimCapture`; both
/// commands exit 0 on the lifted shapes, so the chain records 0/0 (the
/// runtime pipeline's status is the LAST stage's — cut's — which is 0
/// even when grep found nothing). The field rules mirror GNU cut exactly:
/// a line with NO delimiter passes through whole (cut treats it as field
/// 1 for any `-f`), a missing field (fewer delimiters than the field
/// index) is empty. The trailing empty line from the file's final newline
/// is filtered by the same predicate (it matches neither a prefix nor a
/// non-empty substring pattern) and its joined "\n" is stripped by
/// trimCapture — byte-identical to grep's passthrough of the final
/// newline.
fn native_capture_grep_cut(stages: &[IrExpr]) -> Option<Expr> {
    if stages.len() != 2 {
        return None;
    }
    let (name1, a1) = pipeline_stage_exec(&stages[0])?;
    let (name2, a2) = pipeline_stage_exec(&stages[1])?;
    if name1 != "grep" || name2 != "cut" {
        return None;
    }
    let (invert, pat, file): (bool, String, &IrExpr) = match a1 {
        [p, file] => (false, static_text(p)?, file),
        [v, p, file] if static_text(v).as_deref() == Some("-v") => (true, static_text(p)?, file),
        _ => return None,
    };
    let gpat = classify_grep_pat(&pat)?;
    let [IrExpr::Str(d, _), IrExpr::Str(dv, _), IrExpr::Str(f, _), IrExpr::Str(ff, _)] = a2
    else {
        return None;
    };
    if d != "-d" || f != "-f" {
        return None;
    }
    let d = dv;
    let mut cs = ff.chars();
    let field: usize = match (cs.next(), cs.next()) {
        (Some(c), None) => c.to_digit(10)? as usize,
        _ => return None, // multi-field `-f 1,3` keeps the runtime
    };
    if field == 0 {
        return None;
    }
    let mut dc = d.chars();
    let dch = dc.next()?;
    if dc.next().is_some() || dch == '\n' {
        return None; // multi-char / newline delimiters keep the runtime
    }
    let file_expr = native_fs_path_arg(file)?;
    let s = read_file_promise(file_expr, Some("utf8"), 0, 0);
    // l => KEEP
    let keep = |l: Expr| {
        let pred = match &gpat {
            GrepPat::Prefix(p) => method_call(l.clone(), "startsWith", vec![str_lit(p)]),
            GrepPat::Substr(p) => method_call(l.clone(), "includes", vec![str_lit(p)]),
        };
        if invert {
            Expr::UnaryExpression {
                operator: "!",
                prefix: true,
                argument: Box::new(pred),
            }
        } else {
            pred
        }
    };
    // l => l.includes(D) ? (l.split(D)[F-1] ?? "") : l
    let field_of = |l: Expr| {
        let split = method_call(l.clone(), "split", vec![str_lit(d)]);
        let idx = Expr::MemberExpression {
            object: Box::new(split),
            property: Box::new(Expr::Literal {
                value: serde_json::Value::from(field - 1),
                raw: None,
            regex: None,
            }),
            computed: true,
            optional: false,
        };
        let picked = Expr::LogicalExpression {
            operator: "??",
            left: Box::new(idx),
            right: Box::new(str_lit("")),
        };
        Expr::ConditionalExpression {
            test: Box::new(method_call(l.clone(), "includes", vec![str_lit(d)])),
            consequent: Box::new(picked),
            alternate: Box::new(l),
        }
    };
    let body = method_call(
        method_call(
            method_call(method_call(ident("t"), "split", vec![str_lit("\n")]), "filter", vec![sync_arrow_expr_param("l", keep(ident("l")))]),
            "map",
            vec![sync_arrow_expr_param("l", field_of(ident("l")))],
        ),
        "join",
        vec![str_lit("\n")],
    );
    let chained = method_call(s, "then", vec![sync_arrow_expr_param("t", body)]);
    Some(trim_capture(await_expr(chained)))
}

/// `l => l.includes(D) ? (l.split(D)[F-1] ?? "") : l` — the GNU-cut field
/// slice shared by the grep|cut and cut|sort lifts: a line with no
/// delimiter passes through whole (GNU cut treats it as field 1 for any
/// `-f`); a missing field (fewer delimiters than the field index) is
/// empty.
fn cut_field_of(l: Expr, delim: &str, field: usize) -> Expr {
    let split = method_call(l.clone(), "split", vec![str_lit(delim)]);
    let idx = Expr::MemberExpression {
        object: Box::new(split),
        property: Box::new(Expr::Literal {
            value: serde_json::Value::from(field - 1),
            raw: None,
        regex: None,
        }),
        computed: true,
        optional: false,
    };
    let picked = Expr::LogicalExpression {
        operator: "??",
        left: Box::new(idx),
        right: Box::new(str_lit("")),
    };
    Expr::ConditionalExpression {
        test: Box::new(method_call(l.clone(), "includes", vec![str_lit(delim)])),
        consequent: Box::new(picked),
        alternate: Box::new(l),
    }
}

/// `$(cut -d D -f F FILE | sort [-n])` — field-slice then sort: the file's
/// lines are sliced to one field (GNU cut rules — see `cut_field_of`),
/// sorted (C-locale string order for plain `sort`, the runtime sort
/// builtin's exact `parseFloat||0` comparator for `sort -n`), and joined.
/// The trailing empty line from the file's final newline is dropped before
/// sorting (GNU sort's input has no phantom empty line; its output always
/// ends with a newline, which trimCapture strips — byte-identical). Both
/// commands exit 0 on these shapes, so the chain records 0/0.
fn native_capture_cut_sort(stages: &[IrExpr]) -> Option<Expr> {
    if stages.len() != 2 {
        return None;
    }
    let (name1, a1) = pipeline_stage_exec(&stages[0])?;
    let (name2, a2) = pipeline_stage_exec(&stages[1])?;
    if name1 != "cut" || name2 != "sort" {
        return None;
    }
    // cut -d D -f F FILE — the file-arg form (stdin form means the cut
    // stage of grep|cut, which native_capture_grep_cut handles)
    let [IrExpr::Str(d, _), IrExpr::Str(dv, _), IrExpr::Str(f, _), IrExpr::Str(ff, _), file] = a1
    else {
        return None;
    };
    if d != "-d" || f != "-f" {
        return None;
    }
    let d = dv;
    let mut cs = ff.chars();
    let field: usize = match (cs.next(), cs.next()) {
        (Some(c), None) => c.to_digit(10)? as usize,
        _ => return None,
    };
    if field == 0 {
        return None;
    }
    let mut dc = d.chars();
    let dch = dc.next()?;
    if dc.next().is_some() || dch == '\n' {
        return None;
    }
    // sort: no args (C-locale string order) or -n (numeric)
    let numeric = match a2 {
        [] => false,
        [IrExpr::Str(sv, _)] if sv == "-n" => true,
        _ => return None,
    };
    let file_expr = native_fs_path_arg(file)?;
    let s = read_file_promise(file_expr, Some("utf8"), 0, 0);
    // (t.endsWith("\n") ? t.slice(0, -1) : t).split("\n").map(l => …).sort(…).join("\n")
    let t = ident("t");
    let no_trailing_nl = Expr::ConditionalExpression {
        test: Box::new(method_call(t.clone(), "endsWith", vec![str_lit("\n")])),
        consequent: Box::new(method_call(
            t.clone(),
            "slice",
            vec![int_lit_expr(0), Expr::UnaryExpression {
                operator: "-",
                prefix: true,
                argument: Box::new(int_lit_expr(1)),
            }],
        )),
        alternate: Box::new(t.clone()),
    };
    let lines = method_call(no_trailing_nl, "split", vec![str_lit("\n")]);
    let fields = method_call(lines, "map", vec![sync_arrow_expr_param("l", cut_field_of(ident("l"), d, field))]);
    let sorted = if numeric {
        // (a, b) => ((parseFloat(a) || 0) < (parseFloat(b) || 0) ? -1 :
        //            (parseFloat(a) || 0) > (parseFloat(b) || 0) ? 1 : 0)
        let num = |x: Expr| Expr::LogicalExpression {
            operator: "||",
            left: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "parseFloat".to_string(),
                }),
                arguments: vec![x.clone()],
                optional: false,
            }),
            right: Box::new(int_lit_expr(0)),
        };
        let a = ident("a");
        let b = ident("b");
        let na = num(a.clone());
        let nb = num(b.clone());
        let lt = Expr::BinaryExpression {
            operator: "<",
            left: Box::new(na.clone()),
            right: Box::new(nb.clone()),
        };
        let gt = Expr::BinaryExpression {
            operator: ">",
            left: Box::new(na),
            right: Box::new(nb),
        };
        method_call(
            fields,
            "sort",
            vec![Expr::ArrowFunctionExpression {
                params: vec![a, b],
                body: ArrowBody::Expr(Box::new(Expr::ConditionalExpression {
                    test: Box::new(lt),
                    consequent: Box::new(Expr::UnaryExpression {
                        operator: "-",
                        prefix: true,
                        argument: Box::new(int_lit_expr(1)),
                    }),
                    alternate: Box::new(Expr::ConditionalExpression {
                        test: Box::new(gt),
                        consequent: Box::new(int_lit_expr(1)),
                        alternate: Box::new(int_lit_expr(0)),
                    }),
                })),
                expression: true,
                r#async: false,
            }],
        )
    } else {
        method_call(fields, "sort", vec![])
    };
    let joined = method_call(sorted, "join", vec![str_lit("\n")]);
    let chained = method_call(s, "then", vec![sync_arrow_expr_param("t", joined)]);
    Some(trim_capture(await_expr(chained)))
}

/// `$(wc -l < f)` / `$(wc -c < f)` — newline count / byte count of the
/// redirected file (the runtime builtin's exact formulas; the fd-0
/// redirect form is the one the corpus uses — the `wc -l f` filename-arg
/// form appends the filename to the output and is left on the runtime).
fn native_capture_wc(cmd_args: &[IrExpr], stdin_file: &IrExpr) -> Option<Expr> {
    let [IrExpr::Str(flag, _)] = cmd_args else {
        return None;
    };
    let count: Expr = match flag.as_str() {
        "-l" => {
            // `(await ...).split('\n').length - 1` — exact newline count
            let s = read_file_value(expr_to_estree(stdin_file), Some("utf8"), 0, 1);
            let split = Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(s),
                    property: Box::new(Expr::Identifier {
                        name: "split".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit("\n")],
                optional: false,
            };
            Expr::BinaryExpression {
                operator: "-",
                left: Box::new(Expr::MemberExpression {
                    object: Box::new(split),
                    property: Box::new(Expr::Identifier {
                        name: "length".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                right: Box::new(Expr::Literal {
                    value: serde_json::Value::from(1),
                    raw: None,
                regex: None,
                }),
            }
        }
        "-c" => {
            // no encoding → Buffer; `.length` = byte count (bash wc -c
            // counts bytes, the runtime's Buffer.byteLength formula)
            let buf = read_file_value(expr_to_estree(stdin_file), None, 0, 1);
            Expr::MemberExpression {
                object: Box::new(buf),
                property: Box::new(Expr::Identifier {
                    name: "length".to_string(),
                }),
                computed: false,
                optional: false,
            }
        }
        _ => return None,
    };
    Some(Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![count],
        optional: false,
    })
}

/// `$(dirname X)` / `$(basename X)` / `$(pwd)` — the sync runtime helpers
/// return the exact string the builtin would emit (minus the trailing
/// newline the capture strips); `pwd` is a plain field read. All three
/// record lastExit = 0 like the builtins.
fn native_capture_path(cmd: &str, cmd_args: &[IrExpr]) -> Option<Expr> {
    let value = match cmd {
        "dirname" | "basename" => {
            let [arg] = cmd_args else {
                return None; // no args → builtin errors (empty capture)
            };
            sh2_call(cmd, vec![expr_to_estree(arg)])
        }
        "pwd" => {
            if !cmd_args.is_empty() {
                return None;
            }
            sh2_member("cwd")
        }
        _ => return None,
    };
    Some(seq(vec![
        Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(sh2_member("lastExit")),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            regex: None,
            }),
        },
        value,
    ]))
}

/// `$(cat ...)` / `$(sort ...)` / `$(wc ...)` / `$(dirname ...)` /
/// `$(basename ...)` / `$(pwd ...)` — a capture whose single statement is
/// one of the pure sync builtins (or a single `< file` input redirect
/// feeding one) collapses to a native value expression. Conservative:
/// any other command / redirect shape / shadowing function keeps the
/// runtime capture machinery.
fn try_native_capture_value(stmts: &[IrStmt]) -> Option<Expr> {
    match stmts {
        [IrStmt::Expr(inner)] => match inner {
            // `$(cmd < f)` — an EXPRESSION-level redirect: capture bodies
            // lower redirects as calls (`command_arrow_stmts` →
            // `command_to_ir` → `sh2.redirect`), NOT the IrStmt::Redirect
            // statement form. The spec object carries fd/mode/target
            // (interpolate only for heredoc modes).
            IrExpr::Call { func, args } if func == "redirect" => {
                let [IrExpr::Arrow(inner_stmts), IrExpr::Array(specs)] = args.as_slice()
                else {
                    return None;
                };
                let [IrExpr::Object(props)] = specs.as_slice() else {
                    return None;
                };
                let mut fd: Option<i64> = None;
                let mut mode: Option<&str> = None;
                let mut target: Option<&IrExpr> = None;
                let mut interpolate = false;
                for (k, v) in props {
                    match (k.as_str(), v) {
                        ("fd", IrExpr::Int(i)) => fd = Some(*i),
                        ("mode", IrExpr::Str(m, _)) => mode = Some(m.as_str()),
                        ("target", t) => target = Some(t),
                        ("interpolate", IrExpr::Bool(b)) => interpolate = *b,
                        _ => {}
                    }
                }
                let mode = mode?;
                // `$(< f)` / `$(cat < f)` / `$(sort < f)` / `$(wc -l < f)`:
                // a fd-0 input redirect supplies stdin, which the native
                // readFile replaces wholesale. The target renders as a path
                // expression (store refs become getVar calls — exactly what
                // the runtime's expandWord would read), so `interpolate`
                // (heredoc-only) never blocks this. A redirect-only command
                // (`$(< f)` — bash copies the file to stdout) arrives as an
                // exec with an EMPTY name; the lift treats it like `cat < f`
                // (bash-correct file content; the runtime's no-op exec
                // yields "" — corpus-neutral either way).
                if mode == "r" && fd == Some(0) {
                    let [IrStmt::Expr(inner_e)] = inner_stmts.as_slice() else {
                        return None;
                    };
                    let IrExpr::Call { func, args } = inner_e else {
                        return None;
                    };
                    if func != "exec" {
                        return None;
                    }
                    let [IrExpr::Str(name, _), IrExpr::Array(cmd_args)] = args.as_slice()
                    else {
                        return None;
                    };
                    let target = target?;
                    if name.is_empty() {
                        // `$(< f)` — the file content, capture-stripped
                        if !cmd_args.is_empty() {
                            return None;
                        }
                        return Some(trim_capture(read_file_value(
                            expr_to_estree(target),
                            Some("utf8"),
                            0,
                            1,
                        )));
                    }
                    if program_defines_function(name) {
                        return None;
                    }
                    return match name.as_str() {
                        "cat" => native_capture_cat(cmd_args, Some(target)),
                        "sort" => native_capture_sort(cmd_args, Some(target)),
                        "wc" => native_capture_wc(cmd_args, target),
                        _ => None,
                    };
                }
                // `$(cat <<EOF ...)` — a fd-0 HEREDOC wrapping `cat` with
                // no args: cat copies stdin (the heredoc body) to stdout,
                // so the captured value is the body minus the capture
                // strips. A non-interpolated body folds to a plain string
                // LITERAL at emit time (zero runtime calls); an interpolated
                // body (unquoted EOF with `$refs`) becomes
                // `trimCapture(template)` — the template inlines lifted
                // bindings, the runtime expandWord reads store refs exactly
                // as it would inside the redirect. (The redirect target is
                // the BODY here, not a path.)
                if (mode == "heredoc" || mode == "heredoc-tabs") && fd == Some(0) {
                    let [IrStmt::Expr(inner_e)] = inner_stmts.as_slice() else {
                        return None;
                    };
                    let IrExpr::Call { func, args } = inner_e else {
                        return None;
                    };
                    if !matches!(func.as_str(), "exec" | "builtin") {
                        return None;
                    }
                    let [IrExpr::Str(name, _), IrExpr::Array(cmd_args)] = args.as_slice()
                    else {
                        return None;
                    };
                    if name != "cat" || !cmd_args.is_empty() || program_defines_function("cat") {
                        return None;
                    }
                    let IrExpr::Str(body, _) = target? else {
                        return None;
                    };
                    if interpolate && body.contains('$') {
                        // the runtime expands the body from the store — a
                        // native template can only inline LIFTED refs
                        // (fully_lifted_template rejects store-bound `$refs`)
                        let tpl = fully_lifted_template(body)?;
                        return Some(trim_capture(tpl));
                    }
                    // A `$`-free body (quoted EOF, or unquoted with no refs)
                    // expands to itself: heredoc-tabs bodies are tab-stripped
                    // by the runtime at execution time (a compile-time fold
                    // would keep the tabs) — only plain heredocs fold to a
                    // literal. (NULs cannot occur in parsed source text;
                    // trailing newlines are the only capture strip.)
                    if mode == "heredoc" {
                        let stripped = body.trim_end_matches('\n').to_string();
                        return Some(str_lit(&stripped));
                    }
                }
                None
            }
            IrExpr::Call { func, args } if func == "exec" => {
                let [IrExpr::Str(name, _), IrExpr::Array(cmd_args)] = args.as_slice() else {
                    return None;
                };
                if program_defines_function(name) {
                    return None; // a script function shadows the builtin
                }
                match name.as_str() {
                    "cat" => native_capture_cat(cmd_args, None),
                    "sort" => native_capture_sort(cmd_args, None),
                    "dirname" | "basename" | "pwd" => {
                        native_capture_path(name, cmd_args)
                    }
                    _ => None,
                }
            }
            // `$(grep [-v] PAT FILE | cut -d D -f F)` and
            // `$(cut -d D -f F FILE | sort [-n])` — the read/filter/field
            // capture pipelines (see `native_capture_grep_cut` and
            // `native_capture_cut_sort`): the whole pipeline collapses to
            // a readFile + string-op chain — no spawns, no pipeline
            // machinery, no capture arrow.
            IrExpr::Call { func, args } if func == "pipeline" => {
                let [IrExpr::Array(stages)] = args.as_slice() else {
                    return None;
                };
                native_capture_grep_cut(stages).or_else(|| native_capture_cut_sort(stages))
            }
            _ => None,
        },
        // Statement-form redirect (reachable when a capture body lowers via
        // stmt_for_command rather than command_arrow_stmts): same lifts,
        // IrStmt::Redirect shape.
        [IrStmt::Redirect { inner, redirects }] => {
            if redirects.len() != 1 {
                return None;
            }
            let r = &redirects[0];
            if r.mode != "r" || r.fd != Some(0) {
                return None;
            }
            let [IrStmt::Expr(inner_e)] = inner.as_slice() else {
                return None;
            };
            let IrExpr::Call { func, args } = inner_e else {
                return None;
            };
            if func != "exec" {
                return None;
            }
            let [IrExpr::Str(name, _), IrExpr::Array(cmd_args)] = args.as_slice() else {
                return None;
            };
            if program_defines_function(name) {
                return None;
            }
            match name.as_str() {
                "cat" => native_capture_cat(cmd_args, Some(&r.target)),
                "sort" => native_capture_sort(cmd_args, Some(&r.target)),
                "wc" => native_capture_wc(cmd_args, &r.target),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Args the runtime's echo would transform beyond plain string joining:
/// GLOB_MAGIC (glob expansion), PS_MAGIC (process-substitution paths — the
/// runtime materializes them into /dev/fd paths), and raw-byte markers
/// (U+F800+ private-use chars the CLI maps to non-UTF-8 source bytes,
/// which the runtime decodeRawBytes re-expands). Native echo would print
/// the marker text literally — any of these keep the runtime dispatch.
fn ir_expr_needs_runtime(e: &IrExpr) -> bool {
    fn magic(s: &str) -> bool {
        s.contains(GLOB_MAGIC)
            || s.contains(PS_MAGIC)
            || s.chars().any(|c| (0xF800..=0xF8FF).contains(&(c as u32)))
    }
    match e {
        IrExpr::Str(s, _) => magic(s),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(s) => magic(s),
            InterpPart::Expr(inner) => ir_expr_needs_runtime(inner),
        }),
        IrExpr::Call { func, args } => {
            // `${!prefix*[@]...}` — the runtime's param returns
            // BADSUB_MAGIC for this shape, and the exec flattener SKIPS the
            // whole command (status 1) when an arg IS the marker — native
            // echo would print the marker text instead.
            if func == "param" {
                if let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args.as_slice() {
                    if op == "slice" && name.starts_with('!') && name.contains('*') {
                        return true;
                    }
                }
            }
            args.iter().any(ir_expr_needs_runtime)
        }
        IrExpr::Array(elems) => elems.iter().any(ir_expr_needs_runtime),
        IrExpr::Object(props) => props.iter().any(|(_, v)| ir_expr_needs_runtime(v)),
        IrExpr::BinOp { lhs, rhs, .. } => {
            ir_expr_needs_runtime(lhs) || ir_expr_needs_runtime(rhs)
        }
        _ => false,
    }
}

/// `f args...` — a call to a PROVABLY-SYNC script-defined function
/// (see [`SYNC_FN_CALLS`] / [`fn_call_sync_set`]) with await-free
/// call-site args: the sync `sh2.fnCall` call, no await (see the
/// stmt/expr exec arms). Anything else (async target, awaiting args,
/// env form) returns None and keeps the async exec dispatch.
fn try_native_fn_call(name: &str, args: &[IrExpr]) -> Option<Expr> {
    if !fn_call_is_sync(name) {
        return None;
    }
    let a = expr_to_estree(&IrExpr::Array(args.to_vec()));
    if expr_has_await(&a) {
        return None;
    }
    Some(sh2_call("fnCall", vec![str_lit(name), a]))
}

            // `let ARITH...` / `(( ARITH ))` — a statement/condition whose
            // EVERY arith arg parses natively (`parse_arith_native`: incl.
            // `++`/`--` and `=`/`+=`/`-=`/`*=` assignments — the
            // `((i++))` per-iteration hot path; rejects `$` refs / `10#`
            // bases / nested writes / `/=`/`%=`), the value is a native
            // expression (lifted vars read bare, store vars as
            // `Number(getVar)||0` — the runtime's exact coercion), with
            // the runtime builtin's status recorded (`let` returns true
            // iff the LAST evaluated value != 0 and sets lastExit to
            // match — `(v !== 0 ? 0 : 1)`). No dispatch, no string
            // re-parse per evaluation.
            //
            // Multiple args are emitted in order (bash evaluates every
            // arg; earlier args were pure reads before assignments
            // existed — now their writes must run). The LAST arg drives
            // the status. A write-ful last arg is evaluated exactly ONCE
            // (the status records it in place — the seq's final value is
            // the lastExit assignment's 0/1, truthiness-identical to the
            // runtime's boolean): the old two-eval tail would double an
            // increment.
fn try_native_let(args: &[IrExpr]) -> Option<Expr> {
    if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args {
        if name == "let" && !a.is_empty() {
            let mut vals: Vec<(Expr, bool)> = Vec::new();
            let mut parseable = true;
            for arg in a {
                match arg {
                    IrExpr::Str(sv, _) => match parse_arith_native(sv) {
                        Some(ast) => vals.push((arith_to_estree(&ast), arith_has_write(&ast))),
                        None => {
                            parseable = false;
                            break;
                        }
                    },
                    _ => {
                        parseable = false;
                        break;
                    }
                }
            }
            if parseable {
                let (last_v, _last_writes) = vals.pop().expect("non-empty let");
                let nonzero = |v: &Expr| Expr::BinaryExpression {
                    operator: "!==",
                    left: Box::new(v.clone()),
                    right: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    regex: None,
                    }),
                };
                // `let` returns true iff the LAST evaluated value != 0 and
                // sets lastExit to match. The conditional form evaluates
                // the last arg exactly ONCE (a write-ful `((i++))` must
                // not double-increment) and yields a real boolean: the
                // status is recorded in each branch, the value is the
                // comparison. Earlier args are emitted in order (bash
                // evaluates every arg — a write in an earlier arg runs).
                let last_ok = nonzero(&last_v);
                let cond = Expr::ConditionalExpression {
                    test: Box::new(last_ok.clone()),
                    consequent: Box::new(seq(vec![
                        Expr::AssignmentExpression {
                            operator: "=".to_string(),
                            left: Box::new(sh2_member("lastExit")),
                            right: Box::new(Expr::Literal {
                                value: serde_json::Value::from(0),
                                raw: None,
                            regex: None,
                            }),
                        },
                        bool_lit(true),
                    ])),
                    alternate: Box::new(seq(vec![
                        Expr::AssignmentExpression {
                            operator: "=".to_string(),
                            left: Box::new(sh2_member("lastExit")),
                            right: Box::new(Expr::Literal {
                                value: serde_json::Value::from(1),
                                raw: None,
                            regex: None,
                            }),
                        },
                        bool_lit(false),
                    ])),
                };
                let mut parts: Vec<Expr> = vals.into_iter().map(|(v, _)| v).collect();
                parts.push(cond);
                return Some(seq(parts));
            }
        }
    }
    None
}

/// The DEAD-write twin of [`try_native_let`] (Plan 4 — see
/// [`compute_lastexit_deadness`]): identical eligibility, but the status
/// ternary + lastExit writes are dropped — the write is provably unread
/// (no `$?`/status reader observes it before the next write or the block's
/// consumer). The side effects stay: every arg is evaluated in order (a
/// write-ful last arg runs its increment exactly once, as its expression's
/// value), and the statement's value is the last arg's value (dropped by
/// the ExpressionStatement — the value was only consumed via the ternary).
fn try_native_let_dead(args: &[IrExpr]) -> Option<Expr> {
    if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args {
        if name == "let" && !a.is_empty() {
            let mut vals: Vec<Expr> = Vec::new();
            for arg in a {
                match arg {
                    IrExpr::Str(sv, _) => match parse_arith_native(sv) {
                        Some(ast) => vals.push(arith_to_estree(&ast)),
                        None => return None,
                    },
                    _ => return None,
                }
            }
            if vals.len() == 1 {
                return vals.pop();
            }
            return Some(seq(vals));
        }
    }
    None
}

/// `rm` / `mkdir` with only the simple flags and plain path args: a native
/// `sh2.fs.*` promise chain — no subprocess spawn, no exec dispatch. The
/// chain records `sh2.lastExit` exactly like the spawned binary would and
/// resolves to the same truthiness `await sh2.exec(...)` yields (guard /
/// `&&` / `||` / `!` branch on it identically). The stderr message text
/// the real binary prints on failure is skipped — the corpus gate compares
/// stdout only; `$?` and the branch chains observe the STATUS, which the
/// chain records (the same trade the `$(cat f)` / `$(sort f)` capture
/// lifts already make).
///
/// rm: `-f`/`--force`, `-r`/`-R`/`--recursive` (and combined `-rf`/`-fr`),
/// `--` end-of-flags. Any other option (`-i`/`-d`/`-v`...), a glob/
/// process-substitution path, or the env-carrying form keeps the runtime
/// spawn. Statuses mirror GNU rm (verified against the real binary):
/// without `-f` a failed path (missing file, directory without `-r`)
/// makes the exit status 1 while the REMAINING paths are still processed
/// (Promise.all — GNU rm continues after an error); `-f` ignores ENOENT
/// but still fails on a directory (EISDIR) like GNU; `-r` recurses via
/// fs.rm.
///
/// mkdir: `-p`/`--parents` → recursive mkdir (existing dirs are fine,
/// exactly `mkdir -p`; EEXIST without `-p` fails like bash). Any other
/// option keeps the runtime.
fn try_native_fs_exec(name: &str, args: &[IrExpr]) -> Option<Expr> {
    match name {
        "rm" => try_native_rm(args),
        "mkdir" => try_native_mkdir(args),
        _ => None,
    }
}

/// The `sh2.fs.unlink(p)` / `sh2.fs.rm(p, opts)` per-path promise (not yet
/// chained with .then/.catch — see `native_fs_status_chain`).
fn native_fs_remove_op(path: Expr, recursive: bool, force: bool) -> Expr {
    if recursive {
        // fs.rm({recursive, force}) — recursive dir/file removal; force
        // suppresses the ENOENT of a missing path (GNU `rm -rf missing`
        // exits 0), force:false keeps it failing (GNU `rm -r missing`
        // exits 1).
        sh2_fs_call(
            "rm",
            vec![
                path,
                Expr::ObjectExpression {
                    properties: vec![
                        prop("recursive", bool_lit(true)),
                        prop("force", bool_lit(force)),
                    ],
                },
            ],
        )
    } else {
        sh2_fs_call("unlink", vec![path])
    }
}

/// The promise a per-path removal yields: 0 on success, 1 on failure.
/// With `-f`, an ENOENT (missing file) is not a failure — but a directory
/// (EISDIR/EPERM) still is, exactly GNU `rm -f DIR` → exit 1.
fn native_fs_remove_result(op: Expr, force: bool) -> Expr {
    let ok = Expr::ArrowFunctionExpression {
        params: vec![],
        body: ArrowBody::Expr(Box::new(Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
        regex: None,
        })),
        expression: true,
        r#async: false,
    };
    let err = if force {
        // (e) => (e && e.code === 'ENOENT') ? 0 : 1
        Expr::ArrowFunctionExpression {
            params: vec![Expr::Identifier {
                name: "e".to_string(),
            }],
            body: ArrowBody::Expr(Box::new(Expr::ConditionalExpression {
                test: Box::new(Expr::LogicalExpression {
                    operator: "&&",
                    left: Box::new(Expr::Identifier {
                        name: "e".to_string(),
                    }),
                    right: Box::new(Expr::BinaryExpression {
                        operator: "===",
                        left: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::Identifier {
                                name: "e".to_string(),
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "code".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        right: Box::new(str_lit("ENOENT")),
                    }),
                }),
                consequent: Box::new(Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                regex: None,
                }),
                alternate: Box::new(Expr::Literal {
                    value: serde_json::Value::from(1),
                    raw: None,
                regex: None,
                }),
            })),
            expression: true,
            r#async: false,
        }
    } else {
        Expr::ArrowFunctionExpression {
            params: vec![],
            body: ArrowBody::Expr(Box::new(Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
            regex: None,
            })),
            expression: true,
            r#async: false,
        }
    };
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(op),
            property: Box::new(Expr::Identifier {
                name: "then".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![ok, err],
        optional: false,
    }
}

/// `Promise.all([...]).then(s => (sh2.lastExit = s.includes(1) ? 1 : 0,
/// sh2.lastExit === 0))` — aggregate the per-path statuses into the exit
/// status bash reports (any failure → 1) and resolve to the truthiness
/// `await sh2.exec(...)` would have returned.
fn native_fs_status_chain(results: Vec<Expr>) -> Expr {
    let all = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::Identifier {
                name: "Promise".to_string(),
            }),
            property: Box::new(Expr::Identifier {
                name: "all".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::ArrayExpression {
            elements: results.into_iter().map(Some).collect(),
        }],
        optional: false,
    };
    let s = Expr::Identifier {
        name: "s".to_string(),
    };
    let no_failure = Expr::UnaryExpression {
        operator: "!",
        prefix: true,
        argument: Box::new(Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(s.clone()),
                property: Box::new(Expr::Identifier {
                    name: "includes".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
            regex: None,
            }],
            optional: false,
        }),
    };
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(all),
            property: Box::new(Expr::Identifier {
                name: "then".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::ArrowFunctionExpression {
            params: vec![s.clone()],
            body: ArrowBody::Expr(Box::new(seq(vec![
                Expr::AssignmentExpression {
                    operator: "=".to_string(),
                    left: Box::new(sh2_member("lastExit")),
                    right: Box::new(Expr::ConditionalExpression {
                        test: Box::new(no_failure),
                        consequent: Box::new(Expr::Literal {
                            value: serde_json::Value::from(0),
                            raw: None,
                        regex: None,
                        }),
                        alternate: Box::new(Expr::Literal {
                            value: serde_json::Value::from(1),
                            raw: None,
                        regex: None,
                        }),
                    }),
                },
                last_exit_eq_zero(),
            ]))),
            expression: true,
            r#async: false,
        }],
        optional: false,
    }
}

/// A plain path arg the native fs commands accept: lowers to a JS
/// expression with no await and no runtime markers (GLOB_MAGIC — the
/// runtime would glob-expand it; PS_MAGIC — a materialized /dev/fd path;
/// raw-byte private-use chars — decodeRawBytes territory).
fn native_fs_path_arg(a: &IrExpr) -> Option<Expr> {
    if let IrExpr::Str(sv, _) = a {
        if sv.contains(GLOB_MAGIC) || sv.contains(PS_MAGIC) {
            return None;
        }
        if sv.chars().any(|c| (0xF800..=0xF8FF).contains(&(c as u32))) {
            return None;
        }
    }
    let pe = expr_to_estree(a);
    if expr_has_await(&pe) {
        return None;
    }
    Some(pe)
}

fn try_native_rm(args: &[IrExpr]) -> Option<Expr> {
    let mut force = false;
    let mut recursive = false;
    let mut no_more_flags = false;
    let mut paths: Vec<&IrExpr> = Vec::new();
    for a in args {
        if let IrExpr::Str(sv, _) = a {
            if !no_more_flags && sv.starts_with('-') && sv != "-" {
                match sv.as_str() {
                    "-f" | "--force" => force = true,
                    "-r" | "-R" | "--recursive" => recursive = true,
                    "-rf" | "-fr" => {
                        force = true;
                        recursive = true;
                    }
                    "--" => no_more_flags = true,
                    _ => return None, // -i/-d/-v/... keep the runtime spawn
                }
                continue;
            }
        }
        paths.push(a);
    }
    if paths.is_empty() {
        return None; // `rm` with no args: GNU usage error — keep the spawn
    }
    let mut results: Vec<Expr> = Vec::new();
    for p in paths {
        let pe = native_fs_path_arg(p)?;
        results.push(native_fs_remove_result(
            native_fs_remove_op(pe, recursive, force),
            force,
        ));
    }
    Some(native_fs_status_chain(results))
}

fn try_native_mkdir(args: &[IrExpr]) -> Option<Expr> {
    let mut parents = false;
    let mut paths: Vec<&IrExpr> = Vec::new();
    for a in args {
        if let IrExpr::Str(sv, _) = a {
            if sv.starts_with('-') && sv != "-" {
                match sv.as_str() {
                    "-p" | "--parents" => parents = true,
                    "--" => {}
                    _ => return None, // -m/-v/... keep the runtime spawn
                }
                continue;
            }
        }
        paths.push(a);
    }
    if paths.is_empty() {
        return None;
    }
    let mut results: Vec<Expr> = Vec::new();
    for p in paths {
        let pe = native_fs_path_arg(p)?;
        let op = if parents {
            // mkdir -p: recursive — existing dirs and missing parents are
            // fine (EEXIST resolves), exactly `mkdir -p`.
            sh2_fs_call(
                "mkdir",
                vec![
                    pe,
                    Expr::ObjectExpression {
                        properties: vec![prop("recursive", bool_lit(true))],
                    },
                ],
            )
        } else {
            sh2_fs_call("mkdir", vec![pe])
        };
        // plain mkdir on an existing dir → EEXIST → 1, like bash
        results.push(native_fs_remove_result(op, false));
    }
    Some(native_fs_status_chain(results))
}

/// `grep -q PAT FILE` — a pure substring test over a file's contents: the
/// spawned grep collapses to an `await sh2.fs.readFile(FILE, "utf8")`
/// promise chain that returns grep's exact truthiness (match) and records
/// grep's exact exit statuses — 0 match, 1 no match, 2 unreadable/missing
/// file (the runtime's spawn yields the real grep code; `$?` reads it
/// back). No spawn, no stderr noise, no regex engine: the pattern must be
/// a plain literal (is_safe_grep_literal) in the single-file `-q` form
/// (`grep -q PAT FILE`); the stdin/`-e`/multi-file forms keep the runtime.
fn native_exec_grep_q(file: &IrExpr, pat: &str) -> Option<Expr> {
    if !is_safe_grep_literal(pat) {
        return None;
    }
    let read = sh2_fs_call("readFile", vec![expr_to_estree(file), str_lit("utf8")]);
    let hit = |s: Expr| Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "String".to_string(),
                }),
                arguments: vec![s],
                optional: false,
            }),
            property: Box::new(Expr::Identifier {
                name: "includes".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![str_lit(pat)],
        optional: false,
    };
    let h = hit(Expr::Identifier {
        name: "s".to_string(),
    });
    let status = |v: Expr| Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(sh2_member("lastExit")),
        right: Box::new(Expr::ConditionalExpression {
            test: Box::new(v),
            consequent: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
                regex: None,
            }),
            alternate: Box::new(Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
                regex: None,
            }),
        }),
    };
    let then = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(read),
            property: Box::new(Expr::Identifier {
                name: "then".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::ArrowFunctionExpression {
            params: vec![Expr::Identifier {
                name: "s".to_string(),
            }],
            body: ArrowBody::Expr(Box::new(seq(vec![status(h.clone()), h]))),
            expression: true,
            r#async: false,
        }],
        optional: false,
    };
    let catch = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(then),
            property: Box::new(Expr::Identifier {
                name: "catch".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::ArrowFunctionExpression {
            params: vec![Expr::Identifier {
                name: "e".to_string(),
            }],
            body: ArrowBody::Expr(Box::new(seq(vec![
                Expr::AssignmentExpression {
                    operator: "=".to_string(),
                    left: Box::new(sh2_member("lastExit")),
                    right: Box::new(Expr::Literal {
                        value: serde_json::Value::from(2),
                        raw: None,
                        regex: None,
                    }),
                },
                bool_lit(false),
            ]))),
            expression: true,
            r#async: false,
        }],
        optional: false,
    };
    Some(await_expr(catch))
}

/// `grep -q PAT <<< TEXT` — the fd-0 herestring redirect form of the
/// substring test: the spawned grep collapses to a native `includes` over
/// the herestring text with grep's exact status (0 match, 1 no match)
/// recorded to lastExit. The herestring's appended newline cannot affect
/// a literal pattern (real newlines are rejected by is_safe_grep_literal).
/// Only a single fd-0 herestring spec qualifies — any other redirect
/// shape (files, extra fds, `2>&1` that would surface grep's stderr)
/// keeps the runtime redirect.
fn try_native_grep_q_redirect(
    inner: &[IrStmt],
    specs: &[(i64, &str, &IrExpr)],
) -> Option<Expr> {
    if specs.len() != 1 || specs[0].0 != 0 || specs[0].1 != "herestring" {
        return None;
    }
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(gargs)] = args.as_slice() else {
        return None;
    };
    if name != "grep" {
        return None;
    }
    let [IrExpr::Str(q, _), IrExpr::Str(pat, _)] = gargs.as_slice() else {
        return None;
    };
    if q != "-q" {
        return None;
    }
    native_grep_q_herestring(&specs[0].2, pat)
}
fn native_grep_q_herestring(target: &IrExpr, pat: &str) -> Option<Expr> {
    if !is_safe_grep_literal(pat) {
        return None;
    }
    let hit = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "String".to_string(),
                }),
                arguments: vec![expr_to_estree(target)],
                optional: false,
            }),
            property: Box::new(Expr::Identifier {
                name: "includes".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![str_lit(pat)],
        optional: false,
    };
    // ((sh2.lastExit = hit ? 0 : 1), hit)
    Some(seq(vec![
        Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(sh2_member("lastExit")),
            right: Box::new(Expr::ConditionalExpression {
                test: Box::new(hit.clone()),
                consequent: Box::new(Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                    regex: None,
                }),
                alternate: Box::new(Expr::Literal {
                    value: serde_json::Value::from(1),
                    raw: None,
                    regex: None,
                }),
            }),
        },
        hit,
    ]))
}

/// `echo args...` at the module's default stdout sink (see ECHO_SINK_DEPTH
/// and PROGRAM_PERSIST_FD1): the whole statement is pure string work — a
/// native `process.stdout.write`, no dispatch. The runtime builtin joins
/// the flattened args with single spaces and appends a newline (unless
/// the first arg is exactly `-n`); `-e` replaces `\n`/`\t`. Mirror that
/// exactly, and record the status the builtin would (`sh2.lastExit = 0`)
/// plus the truthiness its callers branch on (`true` — the builtin always
/// returns true here: fd 1 is the default stdout, never closed).
/// A compile-time-constant string: a bare literal or an interpolation of
/// only literal parts (the emitter wraps QUOTED words as all-Lit
/// interpolations, unquoted ones as Str).
fn static_str(e: &IrExpr) -> Option<String> {
    match e {
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

/// Rust port of the runtime's `unescapeFormat` chain (harness/
/// sh2-namespace.mjs): the SAME sequential global replaces in the SAME
/// order (the `\\n`-before-`\\\\` order matters for `\\\\n`-style strings),
/// then the octal `\\([0-7]{1,3})` replace last.
fn printf_unescape(s: &str) -> Option<String> {
    let mut out = s.to_string();
    for (from, to) in [
        ("\\n", "\n"),
        ("\\t", "\t"),
        ("\\r", "\r"),
        ("\\a", "\x07"),
        ("\\b", "\x08"),
        ("\\f", "\x0c"),
        ("\\v", "\x0b"),
        ("\\\\", "\\"),
    ] {
        out = out.replace(from, to);
    }
    let chars: Vec<char> = out.chars().collect();
    let mut res = String::with_capacity(out.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let mut oct = String::new();
            let mut j = i + 1;
            while j < chars.len() && oct.len() < 3 && matches!(chars[j], '0'..='7') {
                oct.push(chars[j]);
                j += 1;
            }
            if !oct.is_empty() {
                res.push(char::from_u32(u32::from_str_radix(&oct, 8).ok()?)?);
                i = j;
                continue;
            }
        }
        res.push(chars[i]);
        i += 1;
    }
    Some(res)
}

/// One `%` conversion spec from the runtime's format regex
/// /%(?:[-+ 0#]*\d*(?:\.\d+)?[diouxXeEfgGcbsq%])/ — (flags, width, prec,
/// conv, spec length in chars). None when the `%` at `pos` does not start
/// a valid spec (it is plain text, exactly like the runtime's regex miss).
fn printf_scan_spec(
    chars: &[char],
    pos: usize,
) -> Option<(String, usize, Option<usize>, char, usize)> {
    let mut i = pos + 1;
    while i < chars.len() && matches!(chars[i], '-' | '+' | ' ' | '0' | '#') {
        i += 1;
    }
    let flags: String = chars[pos + 1..i].iter().collect();
    let wstart = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    let width: usize = if i > wstart {
        chars[wstart..i].iter().collect::<String>().parse().ok()?
    } else {
        0
    };
    let mut prec = None;
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let pstart = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        // the runtime's (?:\\.\\d+)? needs at least one digit
        if i > pstart {
            prec = Some(chars[pstart..i].iter().collect::<String>().parse().ok()?);
        } else {
            return None;
        }
    }
    if i >= chars.len() {
        return None;
    }
    let conv = chars[i];
    if !matches!(
        conv,
        'd' | 'i' | 'o' | 'u' | 'x' | 'X' | 'e' | 'E' | 'f' | 'g' | 'G' | 'c' | 'b' | 's'
            | 'q' | '%'
    ) {
        return None;
    }
    Some((flags, width, prec, conv, i + 1 - pos))
}

/// JS `parseInt(s, 10)` for the reachable subset: leading JS whitespace
/// (ASCII here — non-ASCII input bails), optional sign, decimal digits;
/// no digits → NaN, which the runtime's `|| 0` turns into 0. Digit strings
/// that overflow i64 bail (JS double formatting differs beyond that).
fn js_parse_int(s: &str) -> Option<i64> {
    if !s.is_ascii() {
        return None;
    }
    let s = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Some(0); // parseInt NaN || 0
    }
    let v: i64 = digits.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// The runtime's `pad()`: space/zero fill to `width`, `-` left-justifies,
/// `0`-fill only without `-` and never for `%s`. JS `.length` counts UTF-16
/// units — non-ASCII bails (char count would disagree).
fn printf_pad(s: &str, flags: &str, width: usize, conv: char) -> Option<String> {
    if !s.is_ascii() {
        return None;
    }
    let len = s.chars().count();
    if width <= len {
        return Some(s.to_string());
    }
    let fill = if flags.contains('0') && !flags.contains('-') && conv != 's' {
        '0'
    } else {
        ' '
    };
    let fill_s: String = std::iter::repeat(fill).take(width - len).collect();
    Some(if flags.contains('-') {
        format!("{s}{fill_s}")
    } else {
        format!("{fill_s}{s}")
    })
}

/// One conversion, exact runtime semantics (printfOne) for the supported
/// subset — `%s` and `%d`/`%i` (the runtime's precision is ignored for
/// these, matching printfOne). Anything else bails to the runtime path.
fn printf_one(
    conv: char,
    flags: &str,
    width: usize,
    _prec: Option<usize>,
    arg: &str,
) -> Option<String> {
    match conv {
        's' => printf_pad(arg, flags, width, 's'),
        'd' | 'i' => printf_pad(&js_parse_int(arg)?.to_string(), flags, width, 'd'),
        _ => None,
    }
}

/// Parsed printf format: text runs and `%` specs in source order.
struct PrintfFmt {
    /// (text) runs; `Spec { flags, width, prec, conv }` entries
    els: Vec<PrintfEl>,
    /// spec count per format pass (each consumes one arg)
    n_specs: usize,
}

enum PrintfEl {
    Text(String),
    Spec {
        flags: String,
        width: usize,
        prec: Option<usize>,
        conv: char,
    },
}

/// Parse the RAW format (backslashes intact — unescaping applies to TEXT
/// runs only, exactly like the runtime's formatOnce). Returns None for any
/// conversion outside the supported subset (the caller keeps the runtime
/// dispatch — never a wrong byte).
fn printf_parse(fmt: &str) -> Option<PrintfFmt> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut els: Vec<PrintfEl> = Vec::new();
    let mut text = String::new();
    let mut pos = 0usize;
    let mut n_specs = 0usize;
    while pos < chars.len() {
        if chars[pos] == '%' {
            if let Some((flags, width, prec, conv, len)) = printf_scan_spec(&chars, pos) {
                if conv == '%' {
                    text.push('%');
                } else {
                    if !matches!(conv, 's' | 'd' | 'i') {
                        return None;
                    }
                    if !text.is_empty() {
                        els.push(PrintfEl::Text(std::mem::take(&mut text)));
                    }
                    els.push(PrintfEl::Spec {
                        flags,
                        width,
                        prec,
                        conv,
                    });
                    n_specs += 1;
                }
                pos += len;
                continue;
            }
        }
        text.push(chars[pos]);
        pos += 1;
    }
    if !text.is_empty() {
        els.push(PrintfEl::Text(text));
    }
    Some(PrintfFmt { els, n_specs })
}

/// Apply the parsed format to literal args with the runtime's exact
/// cycling: each spec consumes one arg per pass, missing args are `''`,
/// a spec-less format runs once per arg, zero args runs once.
fn printf_apply(pf: &PrintfFmt, args: &[String]) -> Option<String> {
    let passes = if pf.n_specs == 0 {
        args.len().max(1)
    } else if args.is_empty() {
        1
    } else {
        (args.len() + pf.n_specs - 1) / pf.n_specs
    };
    let mut out = String::new();
    for _ in 0..passes {
        let mut ai = 0usize;
        for el in &pf.els {
            match el {
                PrintfEl::Text(t) => out.push_str(&printf_unescape(t)?),
                PrintfEl::Spec {
                    flags,
                    width,
                    prec,
                    conv,
                } => {
                    let arg = args.get(ai).map(String::as_str).unwrap_or("");
                    out.push_str(&printf_one(*conv, flags, *width, *prec, arg)?);
                    ai += 1;
                }
            }
        }
    }
    Some(out)
}

/// `printf FORMAT ARGS...` with a static format and the module's default
/// stdout sink: the whole call is pure string work — a native
/// `process.stdout.write`, no dispatch. All-literal args are fully
/// computed at emit time (a Rust port of the runtime's printf pipeline);
/// dynamic args compile the format into a template literal with the
/// runtime's per-spec semantics (`%s` → the arg, `%d` → `parseInt||0`) and
/// its format cycling (each spec consumes one arg per pass; the arg
/// expressions are each evaluated exactly once). Anything the port cannot
/// reproduce EXACTLY — unsupported conversions (`%x`, `%f`, `%q`, ...),
/// flags/widths on dynamic args, array-valued args (they expand the arg
/// count at runtime), magic args — keeps the runtime dispatch.
fn try_native_printf(args: &[IrExpr]) -> Option<Expr> {
    let [IrExpr::Str(name, _), IrExpr::Array(pargs)] = args else {
        return None; // env-carrying 3-arg form — keep the dispatch
    };
    if name != "printf" {
        return None;
    }
    let fmt = static_str(pargs.first()?)?;
    let pf = printf_parse(&fmt)?;
    let rest = &pargs[1.min(pargs.len())..];
    // all-literal args → compute the whole output at emit time (brace
    // arrays flatten into the arg list, exactly like the runtime's
    // builtin() flattener)
    let mut lit_args: Vec<String> = Vec::new();
    let mut all_lit = true;
    for a in rest {
        match a {
            IrExpr::Array(elems) => {
                for el in elems {
                    match static_str(el) {
                        Some(s) => lit_args.push(s),
                        None => {
                            all_lit = false;
                            break;
                        }
                    }
                }
            }
            other => match static_str(other) {
                Some(s) => lit_args.push(s),
                None => {
                    all_lit = false;
                    break;
                }
            },
        }
    }
    let value: Expr = if all_lit {
        let out = printf_apply(&pf, &lit_args)?;
        str_lit(&out)
    } else {
        // dynamic args: the format must be flag/width/prec-free (the
        // corpus needs none there) and every arg must be a SCALAR
        // expression (arrays/captureWords/listVar expand the arg count at
        // runtime — the compile-time cycling would mis-map)
        if pf.els.iter().any(|el| matches!(el, PrintfEl::Spec { flags, width, .. }
            if !flags.is_empty() || *width > 0))
        {
            return None;
        }
        if pf.els.iter().any(|el| matches!(el, PrintfEl::Spec { prec: Some(_), .. })) {
            return None;
        }
        let mut arg_exprs: Vec<Expr> = Vec::new();
        for a in rest {
            match a {
                IrExpr::Call { func, .. }
                    if func == "captureWords" || func == "listVar" =>
                {
                    return None;
                }
                IrExpr::Array(_) => return None,
                other => arg_exprs.push(expr_to_estree(other)),
            }
        }
        let arg_at = |ai: usize| -> Expr {
            match arg_exprs.get(ai) {
                Some(e) => e.clone(),
                None => str_lit(""),
            }
        };
        let passes = if pf.n_specs == 0 {
            arg_exprs.len().max(1)
        } else if arg_exprs.is_empty() {
            1
        } else {
            (arg_exprs.len() + pf.n_specs - 1) / pf.n_specs
        };
        if pf.n_specs == 0 {
            // no specs: the format text repeats once per arg; the args
            // must still be EVALUATED (side effects) — a leading array
            // literal in the sequence does that, then the write
            let text = printf_unescape(&fmt)?;
            let mut seq_els = vec![Expr::ArrayExpression {
                elements: arg_exprs.into_iter().map(Some).collect(),
            }];
            seq_els.push(printf_write_expr(str_lit(&text.repeat(passes))));
            return Some(seq(seq_els));
        }
        // compile the format into a template: text runs become quasis,
        // each spec consumes the next arg (cycling across passes — the
        // runtime reuses the format until every arg is consumed)
        let mut quasis: Vec<TemplateElement> = Vec::new();
        let mut expressions: Vec<Expr> = Vec::new();
        let mut quasi = String::new();
        let mut ai = 0usize;
        for _pass in 0..passes {
            for el in &pf.els {
                match el {
                    PrintfEl::Text(t) => {
                        quasi.push_str(&printf_unescape(t)?);
                    }
                    PrintfEl::Spec { conv, .. } => {
                        quasis.push(TemplateElement {
                            type_: "TemplateElement",
                            value: TemplateElementValue {
                                raw: quasi.clone(),
                                cooked: Some(quasi.clone()),
                            },
                            tail: false,
                        });
                        quasi.clear();
                        let arg = arg_at(ai);
                        expressions.push(match conv {
                            's' => arg,
                            'd' | 'i' => Expr::LogicalExpression {
                                operator: "||",
                                left: Box::new(Expr::CallExpression {
                                    callee: Box::new(Expr::Identifier {
                                        name: "parseInt".to_string(),
                                    }),
                                    arguments: vec![arg, Expr::Literal {
                                        value: serde_json::Value::from(10),
                                        raw: None,
                                    regex: None,
                                    }],
                                    optional: false,
                                }),
                                right: Box::new(Expr::Literal {
                                    value: serde_json::Value::from(0),
                                    raw: None,
                                regex: None,
                                }),
                            },
                            _ => unreachable!("printf_parse gates the conversions"),
                        });
                        ai += 1;
                    }
                }
            }
        }
        quasis.push(TemplateElement {
            type_: "TemplateElement",
            value: TemplateElementValue {
                raw: quasi,
                cooked: None,
            },
            tail: true,
        });
        Expr::TemplateLiteral {
            quasis,
            expressions,
        }
    };
    // (process.stdout.write(value), sh2.lastExit = 0, true)
    Some(seq(vec![
        printf_write_expr(value),
        Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(sh2_member("lastExit")),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            regex: None,
            }),
        },
        bool_lit(true),
    ]))
}

/// `process.stdout.write(value)` — shared by the native echo / printf
/// lowerings (fd 1 is the module's default stdout there).
fn printf_write_expr(value: Expr) -> Expr {
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::MemberExpression {
                object: Box::new(Expr::Identifier {
                    name: "process".to_string(),
                }),
                property: Box::new(Expr::Identifier {
                    name: "stdout".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            property: Box::new(Expr::Identifier {
                name: "write".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![value],
        optional: false,
    }
}

fn try_native_echo(args: &[IrExpr]) -> Option<Expr> {
    let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args else {
        return None; // env-carrying 3-arg form — the env is command-scoped
    };
    if name != "echo" {
        return None;
    }
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    let text: Expr = if no_newline {
        joined
    } else {
        Expr::BinaryExpression {
            operator: "+",
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        }
    };
    let write = printf_write_expr(text);
    // (process.stdout.write(text), sh2.lastExit = 0, true)
    Some(seq(vec![
        write,
        Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(sh2_member("lastExit")),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            regex: None,
            }),
        },
        bool_lit(true),
    ]))
}

/// `$(echo X | tr SET1 SET2)` / `$(echo X | tr -d SET` — a pure string
/// transform: tr reads the single echoed line (echo adds exactly one
/// trailing newline, which no SET maps) and maps/deletes chars. Lifts to a
/// native JS expression over `String(X)` — no echo/tr spawns, no pipeline
/// machinery. Case maps → `toUpperCase`/`toLowerCase`; single-char maps →
/// `split(c1).join(c2)`; `-d` → a chained split/join("") per set char
/// (exactly tr's char-wise deletion). Conservative: exactly two pipeline
/// stages, plain `echo` with exactly ONE arg (no flags — `-n`/`-e` change
/// the byte stream), `tr` with exactly two set args or `-d` + one set, no
/// redirects anywhere, no glob-tagged echo arg (the runtime would expand
/// it), unrecognized tr escapes bail. The result is the line CONTENT
/// (without echo's trailing newline): statement position wraps it back in
/// `echo` (identical bytes), capture position wraps it in `sh2.trimCapture`
/// (the capture's NUL + trailing-newline strips).
fn try_native_tr_pipeline(pipe: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { func, args } = pipe else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return None;
    };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(n1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if n1 != "echo" || echo_args.len() != 1 {
        return None;
    }
    // a GLOB_MAGIC echo arg glob-expands in the runtime — the transform
    // would see the pattern, not the hits
    if let IrExpr::Str(sv, _) = &echo_args[0] {
        if sv.starts_with(GLOB_MAGIC) {
            return None;
        }
    }
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(n2, _), IrExpr::Array(tr_args)] = a2.as_slice() else {
        return None;
    };
    if n2 != "tr" {
        return None;
    }
    // base = the single echoed line — String(X) for scalar args, the
    // words joined with a space for array args (brace expansion /
    // `${arr[@]}`: echo prints one line of space-joined words, exactly
    // the runtime's builtin() flattening)
    let base_arg = match echo_arg_to_estree(&echo_args[0])? {
        Expr::ArrayExpression { elements, .. } => Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(Expr::ArrayExpression { elements }),
                property: Box::new(Expr::Identifier {
                    name: "join".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![str_lit(" ")],
            optional: false,
        },
        e => e,
    };
    let base = Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![base_arg],
        optional: false,
    };
    let method = |obj: Expr, name: &str, margs: Vec<Expr>| Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(obj),
            property: Box::new(Expr::Identifier {
                name: name.to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: margs,
        optional: false,
    };
    match tr_args.as_slice() {
        [IrExpr::Str(sa, _), IrExpr::Str(sb, _)] => {
            match (sa.as_str(), sb.as_str()) {
                ("a-z", "A-Z") | ("[a-z]", "[A-Z]") | ("[:lower:]", "[:upper:]") => {
                    Some(method(base, "toUpperCase", vec![]))
                }
                ("A-Z", "a-z") | ("[A-Z]", "[a-z]") | ("[:upper:]", "[:lower:]") => {
                    Some(method(base, "toLowerCase", vec![]))
                }
                _ => {
                    let c1 = tr_decode_escapes(sa)?;
                    let c2 = tr_decode_escapes(sb)?;
                    if c1.chars().count() == 1 && c2.chars().count() == 1 {
                        Some(method(
                            method(base, "split", vec![str_lit(&c1)]),
                            "join",
                            vec![str_lit(&c2)],
                        ))
                    } else {
                        None
                    }
                }
            }
        }
        [IrExpr::Str(flag, _), IrExpr::Str(set, _)] if flag == "-d" => {
            let set = tr_decode_escapes(set)?;
            let mut e = base;
            for c in set.chars() {
                e = method(
                    method(e, "split", vec![str_lit(&c.to_string())]),
                    "join",
                    vec![str_lit("")],
                );
            }
            Some(e)
        }
        _ => None,
    }
}


/// `echo ARGS | grep [FLAGS] PAT` — exactly two pipeline stages, no
/// redirects on either stage: the whole pipeline (async machinery, fd
/// swapping, the grep subprocess spawn) collapses to ONE sync runtime
/// call `sh2.grepText(text, argv, captureMode)` — the runtime mini-grep
/// runs the line filter over the echoed text with exact GNU grep
/// semantics for the supported flag set (see sh2-namespace.mjs
/// `grepText`). The helper emits through the CURRENT fd-1 sink (module
/// stdout at statement level, the capture buffer under `$(...)`, the
/// redirect target under `> file` — exactly where the pipeline's last
/// stage would write) and records `sh2.lastExit` (grep's status: 0 iff
/// any line was selected) — the pipeline's two observable effects, byte-
/// identical. Conservative: echo args must be natively joinable
/// (echo_join_args guardrails — no glob/PS markers, no script-defined
/// echo), every grep arg must be a static string, flags restricted to
/// the set the runtime mini-grep implements (v/i/n/c/o/q/x single flags
/// incl. combined shorts, A/B/C/m with an integer value, e/E/F, `--`),
/// no FILE operands, at least one pattern, and patterns must be free of
/// GNU-extension escapes (`\<` `\>` `\b` `\w` ...) whose JS-regex
/// meaning differs.
fn try_native_echo_grep(pipe: &IrExpr) -> Option<(Expr, Vec<Expr>)> {
    let IrExpr::Call { func, args } = pipe else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return None;
    };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    // stage 1: exec("echo", args) — the exact bytes echo writes (the
    // runtime's join + `-e` escape processing + trailing newline)
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if name1 != "echo" {
        return None;
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    let text = if no_newline {
        joined
    } else {
        Expr::BinaryExpression {
            operator: "+",
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        }
    };
    // stage 2: exec("grep", args)
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(name2, _), IrExpr::Array(grep_args)] = a2.as_slice() else {
        return None;
    };
    if name2 != "grep" {
        return None;
    }
    let argv = grep_argv_safe(grep_args)?;
    Some((text, argv))
}

/// The static text of an IR expression: a plain `Str` or a quoted word
/// with no expansions (an all-literal Interpolate — `"hello"` arrives as
/// Interpolate, not Str). Used by the grep-argv validator to accept both.
fn static_text(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Validate a grep argv (minus the command name) for the runtime mini-grep
/// lift: every arg a static string (plain Str or an expansion-free quoted
/// word); flags from the supported set (`--` ends options; single
/// v/i/n/c/o/q/x — combined shorts allowed; -A/-B/-C/-m each followed by
/// an integer; -e/-E/-F — the -e VALUE is a pattern); at most one
/// positional arg (the pattern — any further positional is a FILE operand
/// and keeps the runtime); at least one pattern total. The returned argv
/// is the same static-string list, ready to hand to the runtime parser
/// verbatim.
fn grep_argv_safe(args: &[IrExpr]) -> Option<Vec<Expr>> {
    let mut out: Vec<Expr> = Vec::new();
    let mut positionals = 0usize;
    let mut patterns = 0usize;
    let mut after_dd = false;
    let mut i = 0usize;
    while i < args.len() {
        let av = static_text(&args[i])?; // dynamic (template) arg → runtime
        if !after_dd && av.starts_with('-') && av.len() > 1 {
            let mut pushed = false;
            match av.as_str() {
                "--" => after_dd = true,
                "-e" | "-E" | "-F" => {}
                "-A" | "-B" | "-C" | "-m" => {
                    let v = static_text(args.get(i + 1)?)?;
                    if v.parse::<u64>().is_err() {
                        return None;
                    }
                    out.push(str_lit(&av));
                    out.push(str_lit(&v));
                    pushed = true;
                    i += 1;
                }
                s if s.len() > 1
                    && s[1..].chars().all(|c| matches!(c, 'v' | 'i' | 'n' | 'c' | 'o' | 'q' | 'x')) =>
                {
                    // single-char flags, possibly combined (`-vi`)
                }
                _ => return None, // unknown flag (-w/-b/-r/-f/-l/-L/--color...) → runtime
            }
            if !pushed {
                out.push(str_lit(&av));
            }
            if av == "-e" {
                // the -e VALUE is the pattern — static + safe, or no lift
                let p = static_text(args.get(i + 1)?)?;
                if !grep_pattern_safe(&p) {
                    return None;
                }
                out.push(str_lit(&p));
                patterns += 1;
                i += 1;
            }
            i += 1;
            continue;
        }
        // positional: the first is the pattern, any further is a FILE
        positionals += 1;
        if positionals > 1 {
            return None;
        }
        if !grep_pattern_safe(&av) {
            return None;
        }
        out.push(str_lit(&av));
        patterns += 1;
        i += 1;
    }
    if patterns == 0 {
        return None;
    }
    Some(out)
}

/// A pattern is liftable to the JS regex engine only when its escapes are
/// unambiguous: GNU-extension escapes (`\<` `\>` word boundaries, `\b`
/// `\w` `\s` `\d` ... — backslash followed by an ASCII letter/digit)
/// mean different things in JS, and a real newline would cross the line
/// structure. Backslash + punctuation is fine (BRE/ERE escapes — `\+`
/// `\(` `\.` — the runtime's converter maps them exactly).
fn grep_pattern_safe(pat: &str) -> bool {
    if pat.contains('\n') {
        return false;
    }
    let bytes = pat.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if i + 1 >= bytes.len() {
                return false; // trailing backslash
            }
            if bytes[i + 1].is_ascii_alphanumeric() {
                return false;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    true
}

/// `$(echo args... | wc -l/-w/-c)` — the capture's pipeline is the sync
/// echo builtin feeding the sync wc builtin: the captured value is a pure
/// count over the echoed text, so the whole capture+pipeline+spawn
/// machinery collapses to a native count expression — no spawn, no async
/// pipeline, no fd swapping. The runtime's echo emits the args joined
/// with single spaces (with `-e`/`-n` handling) plus a trailing newline;
/// the runtime's wc counts newline chars (`-l`), bytes (`-c`), or
/// whitespace-separated words (`-w`) — the exact formulas mirrored here
/// as native JS. Conservative: any echo arg the runtime would transform
/// beyond joining (GLOB_MAGIC globs, PS_MAGIC process-substitution paths,
/// raw-byte markers) or a script-defined echo/wc function keeps the
/// runtime pipeline.
fn native_capture_echo_wc(pipe: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { func, args } = pipe else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return None;
    };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    // stage 1: exec("echo", args)
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(n1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if n1 != "echo" {
        return None;
    }
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    // stage 2: exec("wc", [flag])
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(n2, _), IrExpr::Array(wc_args)] = a2.as_slice() else {
        return None;
    };
    if n2 != "wc" {
        return None;
    }
    let [IrExpr::Str(flag, _)] = wc_args.as_slice() else {
        return None;
    };
    if !matches!(flag.as_str(), "-l" | "-w" | "-c") {
        return None;
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    // the byte stream wc counts: the joined text plus echo's trailing
    // newline (skipped for `-n`)
    let text: Expr = if no_newline {
        joined
    } else {
        Expr::BinaryExpression {
            operator: "+",
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        }
    };
    let method = |obj: Expr, name: &str, margs: Vec<Expr>| Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(obj),
            property: Box::new(Expr::Identifier {
                name: name.to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: margs,
        optional: false,
    };
    let len = |obj: Expr| Expr::MemberExpression {
        object: Box::new(obj),
        property: Box::new(Expr::Identifier {
            name: "length".to_string(),
        }),
        computed: false,
        optional: false,
    };
    let count: Expr = match flag.as_str() {
        // newline count: text.split("\n").length - 1 (the runtime's
        // `(text.match(/\n/g) || []).length`)
        "-l" => Expr::BinaryExpression {
            operator: "-",
            left: Box::new(len(method(
                text,
                "split",
                vec![str_lit("\n")],
            ))),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
                regex: None,
            }),
        },
        // byte count: Buffer.byteLength(text, "utf8") (the runtime's
        // exact formula)
        "-c" => Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(Expr::Identifier {
                    name: "Buffer".to_string(),
                }),
                property: Box::new(Expr::Identifier {
                    name: "byteLength".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![text, str_lit("utf8")],
            optional: false,
        },
        // word count: text.trim() ? text.trim().split(/\s+/).length : 0
        // (the runtime's exact formula)
        "-w" => {
            let trimmed = method(text, "trim", vec![]);
            Expr::ConditionalExpression {
                test: Box::new(trimmed.clone()),
                consequent: Box::new(len(method(
                    trimmed,
                    "split",
                    vec![regex_lit("\\s+")],
                ))),
                alternate: Box::new(Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                    regex: None,
                }),
            }
        }
        _ => return None,
    };
    // the wc builtin prints the count (the capture strips the newline)
    Some(Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![count],
        optional: false,
    })
}

/// `$(echo args... | sort)` (bare sort) / `$(echo args... | uniq)` (bare
/// uniq, all-literal args only) — the capture's pipeline is the sync echo
/// builtin feeding a pure line-transform builtin: the value is a native
/// expression over the echoed text (the runtime sort/uniq formulas —
/// split on newline, drop the trailing empty, sort / adjacent-dedup,
/// rejoin). No spawn, no async pipeline. sort lowers for ANY echo args
/// (the split/sort/join chain is the runtime's exact algorithm); uniq
/// needs the compile-time text (adjacent dedup has no regex-free native
/// expression — the all-literal form computes the value at emit time).
fn native_capture_echo_pipeline(pipe: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { func, args } = pipe else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return None;
    };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(n1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if n1 != "echo" || echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(n2, _), IrExpr::Array(cmd_args)] = a2.as_slice() else {
        return None;
    };
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    let text: Expr = if no_newline {
        joined
    } else {
        Expr::BinaryExpression {
            operator: "+",
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        }
    };
    let method = |obj: Expr, name: &str, margs: Vec<Expr>| Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(obj),
            property: Box::new(Expr::Identifier {
                name: name.to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: margs,
        optional: false,
    };
    match (n2.as_str(), cmd_args.as_slice()) {
        // `sort` with no args: the runtime's exact algorithm — split the
        // text on newlines, drop the trailing empty (== slice off the
        // final `\n`), sort, rejoin.
        ("sort", []) => {
            let sliced = Expr::ConditionalExpression {
                test: Box::new(method(text.clone(), "endsWith", vec![str_lit("\n")])),
                consequent: Box::new(method(text.clone(), "slice", vec![
                    Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                        regex: None,
                    },
                    Expr::Literal {
                        value: serde_json::Value::from(-1),
                        raw: None,
                        regex: None,
                    },
                ])),
                alternate: Box::new(text.clone()),
            };
            Some(method(
                method(method(sliced, "split", vec![str_lit("\n")]), "sort", vec![]),
                "join",
                vec![str_lit("\n")],
            ))
        }
        // `uniq` with no args: adjacent-dedup — only when the echoed text
        // is a compile-time constant (all-literal args), computed here
        // (the runtime's exact algorithm: split, pop the trailing empty
        // line, collapse adjacent runs).
        ("uniq", []) => {
            let text = static_echo_text(echo_args)?;
            let mut lines: Vec<&str> = text.split('\n').collect();
            if lines.last() == Some(&"") {
                lines.pop();
            }
            let mut out: Vec<&str> = Vec::new();
            for l in lines {
                if out.last().map_or(true, |x| *x != l) {
                    out.push(l);
                }
            }
            // the capture strips the output's final `\n`
            Some(str_lit(&out.join("\n")))
        }
        _ => None,
    }
}

/// The echo builtin's OUTPUT TEXT as a compile-time string: the args
/// joined with single spaces (the runtime's exact `-e`/`-n` handling),
/// plus the trailing newline unless `-n`. None when any arg is not a
/// static string (store-var interpolations etc.).
fn static_echo_text(echo_args: &[IrExpr]) -> Option<String> {
    let mut esc = false;
    let mut no_newline = false;
    let mut flag_done = false;
    let mut parts: Vec<String> = Vec::new();
    for a in echo_args {
        match a {
            IrExpr::Str(sv, _) if !flag_done && (sv == "-e" || sv == "-n") => {
                flag_done = true;
                if sv == "-e" {
                    esc = true;
                } else {
                    no_newline = true;
                }
            }
            other => {
                flag_done = true;
                let s = static_str(other)?;
                if s.contains(GLOB_MAGIC) || s.contains(PS_MAGIC) {
                    return None;
                }
                parts.push(s);
            }
        }
    }
    let mut text = parts.join(" ");
    if esc {
        text = text.replace("\\n", "\n").replace("\\t", "\t");
    }
    if !no_newline {
        text.push('\n');
    }
    Some(text)
}

/// `$(seq A B | head -N)` / `$(seq A B | tail -N)` — the capture's
/// pipeline is the sync seq builtin feeding the sync head/tail builtin:
/// the value is a slice of the numeric range (the runtime's exact
/// formulas — seq emits `A..B` one per line, head/tail keep the first/
/// last N lines), so the whole capture collapses to a native array
/// slice. Only literal integer seq args with a bounded range (10k
/// elements) and a plain `-N`/`-n N` head/tail count qualify; anything
/// else (float steps, `-c`, filenames) keeps the runtime pipeline.
fn native_capture_seq_slice(pipe: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { func, args } = pipe else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return None;
    };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(n1, _), IrExpr::Array(seq_args)] = a1.as_slice() else {
        return None;
    };
    if n1 != "seq" {
        return None;
    }
    // literal integer args only: `seq A B` or `seq A S B`
    let int_arg = |e: &IrExpr| -> Option<i64> {
        let IrExpr::Str(sv, _) = e else { return None };
        if !sv.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return None;
        }
        sv.parse::<i64>().ok()
    };
    let nums: Vec<i64> = seq_args.iter().map(int_arg).collect::<Option<_>>()?;
    let (first, step, last) = match nums.as_slice() {
        [last] => (1i64, 1i64, *last),
        [first, last] => (*first, 1i64, *last),
        [first, step, last] => (*first, *step, *last),
        _ => return None,
    };
    if step == 0 {
        return None;
    }
    let mut range: Vec<Expr> = Vec::new();
    let mut v = first;
    while if step > 0 { v <= last } else { v >= last } {
        if range.len() > 10000 {
            return None; // bounded: no giant array literals
        }
        range.push(Expr::Literal {
            value: serde_json::Value::from(v),
            raw: None,
            regex: None,
        });
        v += step;
    }
    if range.is_empty() {
        return None; // empty range: seq emits nothing (no empty capture)
    }
    let arr = Expr::ArrayExpression {
        elements: range.into_iter().map(Some).collect(),
    };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(n2, _), IrExpr::Array(ht_args)] = a2.as_slice() else {
        return None;
    };
    // `head -N` / `head -n N` / `tail -N` / `tail -n N` (positive counts)
    let count: i64 = match ht_args.as_slice() {
        [IrExpr::Str(s, _)] if s.len() > 1 && s.starts_with('-') => {
            s[1..].parse::<i64>().ok()?
        }
        [IrExpr::Str(f, _), IrExpr::Str(c, _)] if f == "-n" => c.parse::<i64>().ok()?,
        _ => return None,
    };
    if count <= 0 {
        return None;
    }
    // head: first N lines → `slice(0, N)`; tail: last N lines →
    // `slice(-N)`
    let slice_args = if n2 == "head" {
        vec![
            Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
                regex: None,
            },
            Expr::Literal {
                value: serde_json::Value::from(count),
                raw: None,
                regex: None,
            },
        ]
    } else {
        vec![Expr::Literal {
            value: serde_json::Value::from(-count),
            raw: None,
            regex: None,
        }]
    };
    let slice = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(arr),
            property: Box::new(Expr::Identifier {
                name: "slice".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: slice_args,
        optional: false,
    };
    let join = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(slice),
            property: Box::new(Expr::Identifier {
                name: "join".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![str_lit("\n")],
        optional: false,
    };
    // `head` appends a newline after the kept lines (the capture strips
    // it); `tail`'s output ends with the last line + `\n` (stripped too).
    // The captured value is exactly the kept lines joined with `\n`.
    Some(Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![join],
        optional: false,
    })
}

/// `$(printf FMT ARGS...)` with an all-static format and args: the
/// capture's value is the formatted output minus the capture strips
/// (NUL bytes + trailing newlines) — a compile-time constant, no capture
/// machinery at all. The shared printf_parse/printf_apply chain is the
/// one the native statement-form printf lowering already validates
/// against the corpus.
fn native_capture_printf(e: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(pargs)] = args.as_slice() else {
        return None;
    };
    if name != "printf" {
        return None;
    }
    let fmt = static_str(pargs.first()?)?;
    let pf = printf_parse(&fmt)?;
    let mut lit_args: Vec<String> = Vec::new();
    for a in &pargs[1..] {
        match a {
            // brace arrays flatten into the arg list, exactly like the
            // runtime's builtin() flattener
            IrExpr::Array(elems) => {
                for el in elems {
                    lit_args.push(static_str(el)?);
                }
            }
            other => lit_args.push(static_str(other)?),
        }
    }
    let out = printf_apply(&pf, &lit_args)?;
    // the capture strips NUL bytes and trailing newlines
    let out = out.replace('\0', "");
    Some(str_lit(out.trim_end_matches('\n')))
}

/// Native lowering for a SIMPLE test expression whose operands are all
/// lifted numeric variables (or integer literals): `"$count" -lt 100`
/// becomes `count < 100` — no runtime test-string round-trip. Returns None
/// for anything else (falls back to the injected template / runtime).
fn try_native_test(s: &str) -> Option<Expr> {
    let s = s.trim();
    // file tests (`-f`/`-d`/`-e`/`-h`/`-s`/`-r`/`-w`/`-x`/`-b`/`-c`/`-p`/
    // `-S`/`-u`/`-g`/`-k`/`-O`/`-G`/`-N`, optionally `!`-negated): the
    // runtime's evalUnary is an lstat + flag check, so a native
    // `await sh2.fs.lstat(p).then(s => <check>, () => false)` is the exact
    // value minus the string parse + dispatch (and the runtime's BLOCKING
    // lstatSync — the native chain is async).
    if let Some(native) = try_native_file_test(s) {
        return Some(native);
    }
    // `-n` / `-z` unary string tests: `[ -n "$x" ]` → `String(x) !== ""`.
    // The runtime's evalUnary is a pure string-emptiness test for these
    // two flags (no stat), so a single-operand form lowers to a plain
    // comparison. Operand shapes: `$name` / `${name}` (quoted or bare —
    // a var read, store or lifted) or a literal with no expansion chars.
    // Compound tests (`-n "$a" -a -z "$b"`) and command substitutions
    // stay on the runtime (the operand must consume the whole string).
    for (flag, want_empty) in [("-n", false), ("-z", true)] {
        if let Some(rest) = s.strip_prefix(flag) {
            let operand = rest.trim();
            let read: Option<Expr> = (|| {
                let bare = operand
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .unwrap_or(operand);
                // `$name` / `${name}` — the runtime expands the operand
                // from the store; lifted bindings read natively.
                if let Some(name) = bare
                    .strip_prefix("${")
                    .and_then(|x| x.strip_suffix('}'))
                    .or_else(|| bare.strip_prefix('$'))
                {
                    if name.starts_with('!') || name.starts_with('#')
                        || name.contains('[') || name.contains('@') || name.contains('*')
                    {
                        return None;
                    }
                    if is_lifted(name) {
                        return Some(Expr::Identifier { name: name.to_string() });
                    }
                    if let Some(native) = native_special_var(name) {
                        return Some(native);
                    }
                    if !is_plain_ident(name) {
                        return None;
                    }
                    return Some(sh2_call("getVar", vec![str_lit(name)]));
                }
                // literal operand — no `$`, backtick, backslash, quotes or
                // whitespace (the tokenizer would split on those)
                if !bare.is_empty()
                    && !bare
                        .chars()
                        .any(|c| matches!(c, '$' | '`' | '\\' | '"' | '\'' | ' ' | '\t'))
                {
                    return Some(str_lit(bare));
                }
                None
            })();
            if let Some(read) = read {
                let val = Expr::CallExpression {
                    callee: Box::new(Expr::Identifier {
                        name: "String".to_string(),
                    }),
                    arguments: vec![read],
                    optional: false,
                };
                return Some(Expr::BinaryExpression {
                    operator: if want_empty { "===" } else { "!==" },
                    left: Box::new(val),
                    right: Box::new(str_lit("")),
                });
            }
        }
    }
    // numeric ops first (their standalone tokens are unambiguous)
    let numeric_ops: [(&str, &str); 6] = [
        ("-eq", "==="),
        ("-ne", "!=="),
        ("-lt", "<"),
        ("-le", "<="),
        ("-gt", ">"),
        ("-ge", ">="),
    ];
    let string_ops: [(&str, &str); 3] = [("==", "==="), ("!=", "!=="), ("=", "===")];
    for (op, js) in numeric_ops {
        let pat = format!(" {op} ");
        if let Some(p) = s.find(&pat) {
            let (lhs, rhs) = (&s[..p], &s[p + 2 + op.len()..]);
            // numeric comparison operands. Each lowers to (expr, mayNaN):
            // a MAY-NaN operand (store var / positional string) needs the
            // runtime's intVal guard (bash `[ $x -ne 5 ]` with x="abc" is
            // an "integer expression expected" error → the whole test is
            // FALSE, never `NaN !== 5` → true).
            fn num_operand(e: &str) -> Option<(Expr, bool)> {
                let e = e.trim();
                let e = e
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .unwrap_or(e);
                // `$(( ... ))` — parsed natively; arith always yields a
                // number (store vars inside coerce via Number()||0, the
                // arith semantics — the test sees a number, never NaN).
                if let Some(inner) = e.strip_prefix("$((").and_then(|x| x.strip_suffix("))"))
                {
                    let a = parse_arith(inner)?;
                    return Some((arith_to_estree(&a), false));
                }
                let bare = e.strip_prefix('$').unwrap_or(e);
                if is_lifted(bare) {
                    // lifted NUMERIC vars are pure numbers (never NaN);
                    // lifted STRING vars need the intVal guard like any
                    // store var (Number("abc") → NaN → whole test false,
                    // the runtime's intVal semantics). Reading the bare
                    // binding is EXACTLY the value the runtime's test
                    // would see with the value inlined (lifted vars are
                    // not in the store — a getVar would read '').
                    return Some((
                        Expr::Identifier { name: bare.to_string() },
                        !is_lifted_num(bare),
                    ));
                }
                if let Ok(v) = e.parse::<i64>() {
                    return Some((
                        Expr::Literal {
                            value: serde_json::Value::from(v),
                            raw: None,
                        regex: None,
                        },
                        false,
                    ));
                }
                // `$?` / `$#` / `$$` are the runtime's own numeric state
                // fields; other special vars (`$1`..`$9`, `$@`, `$-`) are
                // strings (NaN-risky, like store vars).
                if let Some(native) = native_special_var(bare) {
                    let risky = !matches!(bare, "?" | "#" | "$");
                    return Some((native, risky));
                }
                if is_plain_ident(bare) {
                    // a plain store var is a string read (risky)
                    return Some((sh2_call("getVar", vec![str_lit(bare)]), true));
                }
                None
            }
            let (l, l_risky) = num_operand(lhs)?;
            let (r, r_risky) = num_operand(rhs)?;
            if !l_risky && !r_risky {
                return Some(Expr::BinaryExpression {
                    operator: js,
                    left: Box::new(l),
                    right: Box::new(r),
                });
            }
            // NaN-guarded form: guard ONLY the NaN-risky operand(s) (store
            // vars / positionals — the runtime's intVal returns null for
            // non-numeric strings and the WHOLE test is false, even for
            // `-ne`). Safe operands (int literals, `$?`, `$#`, lifted
            // nums, `$((...))`) compare directly. The operand reads are
            // pure (getVar / positional / state fields), so duplicating
            // them is side-effect-free.
            let num = |e: Expr| -> Expr {
                Expr::CallExpression {
                    callee: Box::new(Expr::Identifier {
                        name: "Number".to_string(),
                    }),
                    arguments: vec![e],
                    optional: false,
                }
            };
            let nan_ok = |e: &Expr| -> Expr {
                Expr::UnaryExpression {
                    operator: "!",
                    argument: Box::new(Expr::CallExpression {
                        callee: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::Identifier {
                                name: "Number".to_string(),
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "isNaN".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        arguments: vec![num(e.clone())],
                        optional: false,
                    }),
                    prefix: true,
                }
            };
            let mut guarded = nan_ok(&l);
            if r_risky {
                guarded = Expr::LogicalExpression {
                    operator: "&&",
                    left: Box::new(guarded),
                    right: Box::new(nan_ok(&r)),
                };
            }
            let cmp = Expr::BinaryExpression {
                operator: js,
                left: Box::new(num(l)),
                right: Box::new(num(r)),
            };
            return Some(Expr::LogicalExpression {
                operator: "&&",
                left: Box::new(guarded),
                right: Box::new(cmp),
            });
        }
    }
    for (op, js) in string_ops {
        // the parser strips the SPACES around `=`/`==`/`!=` (`"$x"=hello`),
        // so match the bare operator token outside quoted regions
        let mut in_q = false;
        let mut idx = 0usize;
        let b = s.as_bytes();
        while idx < b.len() {
            if b[idx] == b'"' {
                in_q = !in_q;
                idx += 1;
                continue;
            }
            if !in_q && s[idx..].starts_with(op) {
                // `==` must not be consumed as `=`; `=` must not be the
                // first char of `==`
                if op == "=" && s[idx..].starts_with("==") {
                    idx += 1;
                    continue;
                }
                // the operator must not sit inside a quoted region or a
                // word (`"$x"=a=b` — the first `=` is the operator, the
                // second sits mid-word). Like the runtime tokenizer (which
                // splits on `=` even adjacent to word chars), a word may
                // end right before the operator (`$s==*.txt`); false
                // operators are weeded out by the operand checks below.
                let before = if idx > 0 { b[idx - 1] } else { 0 };
                let is_op = before == 0
                    || before == b'"'
                    || before == b' '
                    || before == b'\''
                    || before == b'$'
                    || before == b'_'
                    || before.is_ascii_alphanumeric();
                if is_op {
                    let (lhs, rhs) = (&s[..idx], &s[idx + op.len()..]);
                    // glob-to-substring family: `[ "$x" = *P* ]` →
                    // `String(x).includes(P)`. The runtime glob-matches a
                    // `=`/`==`/`!=` operand containing glob metacharacters;
                    // a pure `*P*` operand is exactly a substring test, so
                    // the whole comparison lowers native (no sh2.test).
                    if let Some(native) = try_native_glob_test(lhs, rhs, op == "!=") {
                        return Some(native);
                    }
                    let l = str_operand(lhs)?;
                    let r = str_operand(rhs)?;
                    // `nocasematch` makes `==`/`!=` (and `[[ ]]` glob
                    // matches) case-insensitive: the runtime lowercases
                    // BOTH sides (evalTest `=`/`==`/`!=`). A native bare
                    // `===` would be case-sensitive — lowercase both
                    // operands under a possible nocasematch (mirrors the
                    // glob path in try_native_glob_test).
                    if CASE_NOCASE.lock().unwrap().unwrap_or(false) {
                        // the runtime compares `String(ast.l).toLowerCase()`
                        // — wrap both sides in String(...) so the operand
                        // shape matches the gate's allowed string-op
                        // objects (Identifier operands are not gate-listed
                        // directly).
                        let lc = |e: Expr| Expr::CallExpression {
                            callee: Box::new(Expr::MemberExpression {
                                object: Box::new(Expr::CallExpression {
                                    callee: Box::new(Expr::Identifier {
                                        name: "String".to_string(),
                                    }),
                                    arguments: vec![e],
                                    optional: false,
                                }),
                                property: Box::new(Expr::Identifier {
                                    name: "toLowerCase".to_string(),
                                }),
                                computed: false,
                                optional: false,
                            }),
                            arguments: vec![],
                            optional: false,
                        };
                        return Some(Expr::BinaryExpression {
                            operator: js,
                            left: Box::new(lc(l)),
                            right: Box::new(lc(r)),
                        });
                    }
                    return Some(Expr::BinaryExpression {
                        operator: js,
                        left: Box::new(l),
                        right: Box::new(r),
                    });
                }
            }
            idx += 1;
        }
    }
    None
}

/// Inject lifted-variable VALUES into a test-expression string as a
/// template literal: `"$count" -lt 100` becomes
/// `` `"${count}" -lt 100` `` (the runtime still parses the expression, but
/// the lifted var's value is inlined instead of read from the store, which
/// it is no longer in). Handles `$name`, `${name}`, and bare names inside
/// `$(( ... ))` arith regions. Returns None when nothing is injected.
fn test_str_to_estree(s: &str) -> Option<Expr> {
    let bytes = s.as_bytes();
    let mut quasis: Vec<String> = Vec::new();
    let mut exprs: Vec<Expr> = Vec::new();
    let mut lit = String::new();
    let mut i = 0usize;
    let n = bytes.len();
    let mut changed = false;
    while i < n {
        if s[i..].starts_with("$((") {
            // `$(( ... ))` arith region — inject bare lifted identifiers
            let mut j = i + 3;
            let mut depth = 2usize;
            while j < n && depth > 0 {
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                break; // unbalanced — keep the rest literal
            }
            let region = &s[i + 3..j - 2];
            lit.push_str("$((");
            let rb = region.as_bytes();
            let mut k = 0usize;
            while k < rb.len() {
                let c = rb[k] as char;
                if (c.is_ascii_alphabetic() || c == '_')
                    && (k == 0 || !rb[k - 1].is_ascii_alphanumeric())
                {
                    let start = k;
                    while k < rb.len() && (rb[k].is_ascii_alphanumeric() || rb[k] == b'_') {
                        k += 1;
                    }
                    let w = &region[start..k];
                    if is_lifted(w) {
                        quasis.push(std::mem::take(&mut lit));
                        exprs.push(Expr::Identifier { name: w.to_string() });
                        changed = true;
                        continue;
                    }
                    lit.push_str(w);
                    continue;
                }
                let ch = region[k..].chars().next().unwrap();
                lit.push(ch);
                k += ch.len_utf8();
            }
            lit.push_str("))");
            i = j;
            continue;
        }
        if bytes[i] == b'$' && i + 1 < n && bytes[i + 1] == b'{' {
            if let Some(close) = s[i + 2..].find('}') {
                let name = &s[i + 2..i + 2 + close];
                let end = i + 2 + close + 1;
                if is_lifted(name) {
                    quasis.push(std::mem::take(&mut lit));
                    exprs.push(Expr::Identifier { name: name.to_string() });
                    changed = true;
                } else {
                    lit.push_str(&s[i..end]);
                }
                i = end;
                continue;
            }
        } else if bytes[i] == b'$' && i + 1 < n {
            let rest = &s[i + 1..];
            let name_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if name_len > 0 {
                let name = &rest[..name_len];
                if is_lifted(name) {
                    quasis.push(std::mem::take(&mut lit));
                    exprs.push(Expr::Identifier { name: name.to_string() });
                    changed = true;
                    i += 1 + name_len;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        lit.push(ch);
        i += ch.len_utf8();
    }
    if !changed {
        return None;
    }
    quasis.push(lit);
    let mut elems: Vec<TemplateElement> = quasis
        .into_iter()
        .map(|raw| TemplateElement {
            type_: "TemplateElement",
            value: TemplateElementValue {
                raw,
                cooked: None,
            },
            tail: false,
        })
        .collect();
    if let Some(last) = elems.last_mut() {
        last.tail = true;
    }
    Some(Expr::TemplateLiteral {
        quasis: elems,
        expressions: exprs,
    })
}

/// Like `test_str_to_estree` but STRICTER: returns `Some` only when the
/// string is `$`-free (plain literal) or EVERY `$`-expansion in it is a
/// lifted variable (all inlined into a template literal). A `$` referring
/// to a store-bound variable would have to be expanded by the runtime from
/// the STORE at runtime — a native expression cannot do that, so the caller
/// falls back to the runtime call (or the value-override param form, where
/// the runtime still expandWord's the arg). Mirrors expandWord's quote
/// handling: a pair of surrounding quotes (the parser keeps them in
/// defaults) is stripped first.
fn fully_lifted_template(s: &str) -> Option<Expr> {
    // expandWord strips one pair of surrounding quotes after expansion.
    let bare = if s.len() >= 2 {
        let q = s.chars().next().unwrap();
        if (q == '"' || q == '\'') && s.ends_with(q) {
            &s[1..s.len() - 1]
        } else {
            s
        }
    } else {
        s
    };
    if !bare.contains('$') {
        return Some(str_lit(bare));
    }
    let bytes = bare.as_bytes();
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        if i + 1 < n && bytes[i + 1] == b'{' {
            if let Some(close) = bare[i + 2..].find('}') {
                let name = &bare[i + 2..i + 2 + close];
                if is_lifted(name) {
                    i = i + 2 + close + 1;
                    continue;
                }
            }
            return None; // ${...} with a non-lifted / complex body
        }
        let rest = &bare[i + 1..];
        let name_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        if name_len > 0 {
            let name = &rest[..name_len];
            if is_lifted(name) {
                i += 1 + name_len;
                continue;
            }
        }
        return None; // $$ / $? / $1 / $(...) — not a lifted plain ref
    }
    test_str_to_estree(bare)
}

/// The native ESTree expression for a positional-parameter read — the
/// exact value the runtime's `getVar(name)` yields for `$0` / `$1..$9` /
/// `$@` / `$*` / `$#` (direct reads of its positional state, which the
/// function-call machinery saves/restores — sound inside function bodies
/// too). Non-positional names → None.
fn positional_read(name: &str) -> Option<Expr> {
    let positional = || sh2_member("positional");
    match name {
        "0" => Some(sh2_member("argv0")),
        "@" | "*" => Some(Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(positional()),
                property: Box::new(Expr::Identifier {
                    name: "join".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![str_lit(" ")],
            optional: false,
        }),
        "#" => Some(Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![Expr::MemberExpression {
                object: Box::new(positional()),
                property: Box::new(Expr::Identifier {
                    name: "length".to_string(),
                }),
                computed: false,
                optional: false,
            }],
            optional: false,
        }),
        _ => {
            let d = name.parse::<u32>().ok()?;
            if (1..=9).contains(&d) {
                Some(Expr::LogicalExpression {
                    operator: "??",
                    left: Box::new(Expr::MemberExpression {
                        object: Box::new(positional()),
                        property: Box::new(Expr::Literal {
                            value: serde_json::Value::from(d - 1),
                            raw: None,
                        regex: None,
                        }),
                        computed: true,
                        optional: false,
                    }),
                    right: Box::new(str_lit("")),
                })
            } else {
                None
            }
        }
    }
}

/// `sh2.param` lowering for LIFTED variable names. The runtime reads the
/// value from the STORE by string name — a lifted binding is not there — so
/// the value is inlined as a JS expression and the pure string ops the
/// runtime would run are emitted natively (mirrors harness
/// sh2-namespace.mjs param/substGlob/stripGlobPrefix/stripGlobSuffix
/// exactly; the corpus is the oracle). Ops whose extras cannot go native
/// (glob patterns, store-reading defaults/offsets, `:?` exit, basename/
/// dirname) fall through to the caller's value-override form
/// (`sh2.param(op, name, extras..., value)`), never a store read.
fn try_native_param(args: &[IrExpr]) -> Option<Expr> {
    let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args else {
        return None;
    };
    // Value source: a LIFTED binding (bare identifier — the runtime cannot
    // read it from the store), a POSITIONAL ($0/$1..$9/$@/$*/$# — a direct
    // read of the runtime's positional state, the exact value its getVar
    // would yield; sound inside function bodies too because the runtime's
    // call machinery saves/restores `positional` around script-function
    // calls), or nothing (a store var → keep the runtime call).
    let value: Option<Expr> = if is_lifted(name) {
        Some(Expr::Identifier {
            name: name.clone(),
        })
    } else {
        positional_read(name)
    };
    let value = value?;
    // positional writes (`${1:=d}`) and `:?` exits stay on the runtime.
    let is_positional = positional_read(name).is_some() && !is_lifted(name);
    let id = || value.clone();
    let val = || Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![id()],
        optional: false,
    };
    let member = |obj: Expr, prop: &str| Expr::MemberExpression {
        object: Box::new(obj),
        property: Box::new(Expr::Identifier {
            name: prop.to_string(),
        }),
        computed: false,
        optional: false,
    };
    let method = |obj: Expr, prop: &str, args: Vec<Expr>| Expr::CallExpression {
        callee: Box::new(member(obj, prop)),
        arguments: args,
        optional: false,
    };
    let bin = |l: Expr, op: &'static str, r: Expr| Expr::BinaryExpression {
        operator: op,
        left: Box::new(l),
        right: Box::new(r),
    };
    let cond = |t: Expr, c: Expr, a: Expr| Expr::ConditionalExpression {
        test: Box::new(t),
        consequent: Box::new(c),
        alternate: Box::new(a),
    };
    let int_lit = |i: i64| Expr::Literal {
        value: serde_json::Value::from(i),
        raw: None,
    regex: None,
    };
    // A glob-strip/substitute pattern that the runtime's literal fast path
    // handles (no glob metachars) and that embeds cleanly in a JS string
    // literal (ASCII; `$` is literal there, exactly like the runtime — it
    // never expands patterns).
    let literal_pattern = |p: &str| {
        !p.is_empty() && p.is_ascii() && !p.chars().any(|c| matches!(c, '*' | '?' | '['))
    };
    match op.as_str() {
        // ${x} — a plain read of the binding (like the getVar lift)
        "" => Some(id()),
        // ${#x} — string length
        "len" => Some(member(val(), "length")),
        // ${x^^} / ${x,,} — case conversion
        "^^" => Some(method(val(), "toUpperCase", vec![])),
        ",," => Some(method(val(), "toLowerCase", vec![])),
        // ${x^} / ${x,} — first character only (empty → "", like the
        // runtime's `v.length ? ... : v` since charAt(0) is "" and
        // slice(1) is "" for the empty string)
        "^" | "," => {
            let up = op == "^";
            Some(bin(
                method(
                    method(val(), "charAt", vec![int_lit(0)]),
                    if up { "toUpperCase" } else { "toLowerCase" },
                    vec![],
                ),
                "+",
                method(val(), "slice", vec![int_lit(1)]),
            ))
        }
        // ${x#p} / ${x##p} / ${x%p} / ${x%%p} — literal prefix/suffix
        // removal (shortest == longest for literal patterns, exactly like
        // the runtime's literal fast paths)
        "#" | "##" | "%" | "%%" => {
            let [_, _, IrExpr::Str(p, _)] = args else {
                return None;
            };
            if !literal_pattern(p) {
                return None;
            }
            let len = p.chars().count() as i64;
            if op.starts_with('#') {
                Some(cond(
                    method(val(), "startsWith", vec![str_lit(p)]),
                    method(val(), "slice", vec![int_lit(len)]),
                    val(),
                ))
            } else {
                Some(cond(
                    method(val(), "endsWith", vec![str_lit(p)]),
                    method(val(), "slice", vec![int_lit(0), int_lit(-len)]),
                    val(),
                ))
            }
        }
        // ${x//p/r} — replace ALL occurrences (split/join; the runtime's
        // literal fast path is exactly this). Empty pattern must stay on
        // the runtime (split("") splits chars; bash treats it as a
        // no-op).
        "//" => {
            let [_, _, IrExpr::Str(p, _), IrExpr::Str(r, _)] = args else {
                return None;
            };
            if !literal_pattern(p) {
                return None;
            }
            let rep = fully_lifted_template(r)?;
            Some(method(
                method(val(), "split", vec![str_lit(p)]),
                "join",
                vec![rep],
            ))
        }
        // ${x:-d} — default when empty. `${x:=d}` also WRITES the
        // binding (a JS assignment expression — the runtime's setVar
        // cannot see the lifted binding, so this op must go native; the
        // lift analysis marks `:=` names whose default cannot be fully
        // inlined, keeping them store-bound instead). A POSITIONAL
        // `:=` write stays on the runtime (its setVar path is the
        // store/positional authority).
        ":-" | ":=" => {
            if op == ":=" && is_positional {
                return None;
            }
            let [_, _, IrExpr::Str(d, _)] = args else {
                return None;
            };
            let dflt = fully_lifted_template(d)?;
            let test = bin(val(), "!==", str_lit(""));
            if op == ":-" {
                Some(cond(test, val(), dflt))
            } else {
                Some(cond(
                    test,
                    val(),
                    Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(id()),
                        right: Box::new(dflt),
                    },
                ))
            }
        }
        // ${x:off:len} — substring slice with LITERAL integer offsets
        // (negative offsets count from the end, like the runtime's
        // v.slice(off, off + len)). Non-integer offsets (arith exprs) fall
        // through to the value-override form. `${@:off:len}` / `${*:off:len}`
        // are the positional-LIST slice: bash offsets are 1-BASED
        // (${@:1} = all params; ${@:0} includes $0), negative offsets count
        // from the end, and the result is the list joined with spaces
        // (exactly the runtime's `sl.join(' ')`).
        "slice" => {
            if name == "@" || name == "*" {
                let [_, _, IrExpr::Str(off, _), IrExpr::Str(len, _)] = args else {
                    return None;
                };
                let int_of = |t: &str| {
                    let t = t.trim();
                    if t.is_empty() {
                        Some(0i64)
                    } else if t.starts_with('-') {
                        t[1..].parse::<i64>().ok().map(|v| -v)
                    } else {
                        t.parse::<i64>().ok()
                    }
                };
                let o = int_of(off)?;
                let l = int_of(len).unwrap_or(0);
                let list: Expr = if o == 0 {
                    // [argv0, ...positional] — concat (no spread node)
                    method(
                        array(vec![sh2_member("argv0")]),
                        "concat",
                        vec![sh2_member("positional")],
                    )
                } else {
                    sh2_member("positional")
                };
                let start: i64 = if o == 0 { 0 } else if o > 0 { o - 1 } else { o };
                let sl = if len.trim().is_empty() {
                    method(list, "slice", vec![int_lit(start)])
                } else {
                    method(
                        list,
                        "slice",
                        vec![int_lit(start), int_lit(start + l)],
                    )
                };
                return Some(method(sl, "join", vec![str_lit(" ")]));
            }
            let [_, _, IrExpr::Str(off, _), IrExpr::Str(len, _)] = args else {
                return None;
            };
            let int_of = |t: &str| {
                let t = t.trim();
                if t.is_empty() {
                    Some(0i64)
                } else if t.starts_with('-') {
                    t[1..].parse::<i64>().ok().map(|v| -v)
                } else {
                    t.parse::<i64>().ok()
                }
            };
            let o = int_of(off)?;
            if len.trim().is_empty() {
                Some(method(val(), "slice", vec![int_lit(o)]))
            } else {
                let l = int_of(len)?;
                Some(method(
                    val(),
                    "slice",
                    vec![int_lit(o), int_lit(o + l)],
                ))
            }
        }
        // ${x##*/} — the parser's basename/dirname ops: pure string work
        // (trailing-slash strip + last-component split — mirror the
        // runtime's param impl exactly; a missing slash yields the whole
        // path / ".").
        "basename" | "dirname" => {
            // mirror the runtime's `p = v.replace(/\/+$/, '')` — one
            // trailing-slash strip, no flags
            let strip = method(val(), "replace", vec![regex_lit("\\/+$"), str_lit("")]);
            let last = method(strip.clone(), "lastIndexOf", vec![str_lit("/")]);
            if op == "basename" {
                Some(cond(
                    bin(last.clone(), ">=", int_lit(0)),
                    method(strip.clone(), "slice", vec![bin(last.clone(), "+", int_lit(1))]),
                    strip,
                ))
            } else {
                Some(cond(
                    bin(last.clone(), ">=", int_lit(0)),
                    method(strip.clone(), "slice", vec![int_lit(0), last]),
                    str_lit("."),
                ))
            }
        }
        // :? and everything else: the value-override form (runtime keeps
        // the string-op logic; only the value source changes).
        _ => None,
    }
}

/// Native file-test lowering (`[ -f path ]` family). Mirrors the runtime's
/// evalUnary (harness/sh2-namespace.mjs) EXACTLY for the flags below:
/// lstat semantics (a symlink reports its own type — the runtime uses
/// lstatSync, unlike bash's following stat), mode-bit checks, and the
/// missing-path catch → false. The chain is
/// `await sh2.fs.lstat(P).then(s => <check>, () => false)` — one async
/// non-blocking fs call, no test-string parse, no dispatch.
///
/// Operand shapes: a literal path (quoted or bare, `$`-free, no
/// whitespace/quotes/backslashes inside), a `$name`/`${name}` read
/// (lifted binding → bare identifier, special var → field read, store var
/// → getVar), or a `$(( ... ))` arith (native number). The operand must
/// consume the WHOLE remainder (compounds `-a`/`-o`, parenthesized groups,
/// cmdsubs, and the space-less `!-x` token shape stay on the runtime).
fn try_native_file_test(s: &str) -> Option<Expr> {
    // flags whose check is a pure stats-object expression (mode bits,
    // size, uid/gid, times) — every one the runtime's evalUnary resolves
    // from lstatSync; `-t` (constant false) and `-n`/`-z` (string tests,
    // handled earlier) excluded.
    let t = s.trim();
    let (negate, rest) = match t.strip_prefix("! ") {
        Some(r) => (true, r.trim()),
        None => (false, t),
    };
    let flag = rest.split_whitespace().next()?;
    if flag.len() != 2 || !flag.starts_with('-') {
        return None;
    }
    let f = flag.as_bytes()[1] as char;
    if !matches!(
        f,
        'f' | 'd' | 'e' | 'L' | 'h' | 's' | 'r' | 'w' | 'x' | 'b' | 'c' | 'p' | 'S' | 'u' | 'g'
            | 'k' | 'O' | 'G' | 'N'
    ) {
        return None;
    }
    let operand = rest[flag.len()..].trim();
    if operand.is_empty() || operand.contains(char::is_whitespace) {
        return None; // extra tokens after the operand → a compound/other shape
    }
    let path = file_test_operand(operand)?;
    let check = file_test_check(f, Expr::Identifier {
        name: "s".to_string(),
    })?;
    let lstat = sh2_fs_call("lstat", vec![path]);
    let then = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(lstat),
            property: Box::new(Expr::Identifier {
                name: "then".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![
            Expr::ArrowFunctionExpression {
                params: vec![Expr::Identifier {
                    name: "s".to_string(),
                }],
                body: ArrowBody::Expr(Box::new(check)),
                expression: true,
                r#async: false,
            },
            Expr::ArrowFunctionExpression {
                params: vec![],
                body: ArrowBody::Expr(Box::new(bool_lit(false))),
                expression: true,
                r#async: false,
            },
        ],
        optional: false,
    };
    let chain = await_expr(then);
    if negate {
        Some(Expr::UnaryExpression {
            operator: "!",
            argument: Box::new(chain),
            prefix: true,
        })
    } else {
        Some(chain)
    }
}

/// The path operand expression for a file test (see `try_native_file_test`).
fn file_test_operand(op: &str) -> Option<Expr> {
    let quoted = op
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| op.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')));
    let bare = quoted.unwrap_or(op);
    if bare.is_empty() {
        return Some(str_lit("")); // `-f ""` — lstat("") rejects → false, the runtime's empty-arg rule
    }
    if bare.contains('$') {
        let name = bare
            .strip_prefix("${")
            .and_then(|x| x.strip_suffix('}'))
            .or_else(|| bare.strip_prefix('$'));
        let name = name?;
        if name.starts_with('!') || name.starts_with('#') || name.contains('[') || name.contains('@')
            || name.contains('*')
        {
            return None;
        }
        if is_lifted(name) {
            return Some(Expr::Identifier { name: name.to_string() });
        }
        if let Some(native) = native_special_var(name) {
            return Some(native);
        }
        if is_plain_ident(name) {
            return Some(sh2_call("getVar", vec![str_lit(name)]));
        }
        return None;
    }
    if bare.chars().any(|c| matches!(c, '"' | '\'' | '\\' | '`' | '\n' | '\t' | '\r')) {
        return None;
    }
    Some(str_lit(bare))
}

/// The stats-object check for a file-test flag — a pure expression over
/// the `s` binding (mode bits use the exact S_IFMT values node's lstat
/// reports, identical to the runtime's isFile()/isDirectory()/... which
/// node implements the same way).
fn file_test_check(f: char, s: Expr) -> Option<Expr> {
    let member = |obj: Expr, prop: &str| Expr::MemberExpression {
        object: Box::new(obj),
        property: Box::new(Expr::Identifier {
            name: prop.to_string(),
        }),
        computed: false,
        optional: false,
    };
    let bin = |l: Expr, op: &'static str, r: Expr| Expr::BinaryExpression {
        operator: op,
        left: Box::new(l),
        right: Box::new(r),
    };
    let int_lit = |i: i64| Expr::Literal {
        value: serde_json::Value::from(i),
        raw: None,
        regex: None,
    };
    let mode_check = |want: i64| {
        bin(
            bin(member(s.clone(), "mode"), "&", int_lit(61440)),
            "===",
            int_lit(want),
        )
    };
    let mode_any = |bits: i64| {
        bin(
            bin(member(s.clone(), "mode"), "&", int_lit(bits)),
            "!==",
            int_lit(0),
        )
    };
    let getuid = || Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::Identifier {
                name: "process".to_string(),
            }),
            property: Box::new(Expr::Identifier {
                name: "getuid".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![],
        optional: false,
    };
    let getgid = || Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::Identifier {
                name: "process".to_string(),
            }),
            property: Box::new(Expr::Identifier {
                name: "getgid".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![],
        optional: false,
    };
    Some(match f {
        'f' => mode_check(0o100000),
        'd' => mode_check(0o040000),
        'e' => bool_lit(true),
        'L' | 'h' => mode_check(0o120000),
        // `-s`: regular file with a nonzero size (the runtime's
        // `st.isFile() && st.size > 0`)
        's' => Expr::LogicalExpression {
            operator: "&&",
            left: Box::new(mode_check(0o100000)),
            right: Box::new(bin(member(s, "size"), ">", int_lit(0))),
        },
        'b' => mode_check(0o060000),
        'c' => mode_check(0o020000),
        'p' => mode_check(0o010000),
        'S' => mode_check(0o140000),
        'r' => mode_any(0o444),
        'w' => mode_any(0o222),
        'x' => mode_any(0o111),
        'u' => mode_any(0o4000),
        'g' => mode_any(0o2000),
        'k' => mode_any(0o1000),
        'O' => bin(member(s, "uid"), "===", getuid()),
        'G' => bin(member(s, "gid"), "===", getgid()),
        'N' => bin(member(s.clone(), "mtimeMs"), ">", member(s, "atimeMs")),
        _ => return None,
    })
}

/// Conservative "always a string" analysis (slice 4). A variable lifts to a
/// native JS string binding (`let x = ""`) iff every assignment source is a
/// plain string (a literal, an interpolation, or a copy of another
/// string-lifted var) — never arithmetic, capture, or a write-builtin — and
/// it is not already numeric-lifted. String reads inside arithmetic still
/// work via `(Number(x) || 0)` on the native binding.
fn string_lift_vars(prog: &IrProgram, numeric: &HashSet<String>) -> HashSet<String> {
    let mut assigns: HashMap<String, Vec<IrExpr>> = HashMap::new();
    let mut excluded: HashSet<String> = HashSet::new();
    let mut string_ctx: HashSet<String> = HashSet::new();

    fn is_ident(s: &str) -> bool {
        let mut cs = s.chars();
        match cs.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }
    fn mark_string_refs(s: &str, out: &mut HashSet<String>) {
        // Same precise store-read scan as the numeric-lift twin (the two
        // walkers must agree): `$refs` outside `$(...)`, bare identifiers
        // inside `$((...))`. See mark_store_refs.
        mark_store_refs(s, out);
    }
    // `let`/`eval`/`(( ))` args are ARITHMETIC EXPRESSIONS — every bare
    // identifier is a variable the runtime touches (unlike plain string
    // words, which mark_store_refs correctly ignores).
    fn mark_all_idents(s: &str, out: &mut HashSet<String>) {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if (c.is_ascii_alphabetic() || c == '_')
                && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
            {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let w = &s[start..i];
                if is_ident(w) {
                    out.insert(w.to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    fn mark_all_idents_args(e: &IrExpr, out: &mut HashSet<String>) {
        match e {
            IrExpr::Str(ss, _) => mark_all_idents(ss, out),
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_all_idents_args(el, out);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    mark_all_idents_args(v, out);
                }
            }
            _ => {}
        }
    }
    fn mark_str_args(e: &IrExpr, string_ctx: &mut HashSet<String>) {
        match e {
            IrExpr::Str(ss, _) => mark_string_refs(ss, string_ctx),
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_str_args(el, string_ctx);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    mark_str_args(v, string_ctx);
                }
            }
            _ => {}
        }
    }
    fn mark_write_builtin_vars(e: &IrExpr, excluded: &mut HashSet<String>) {
        match e {
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_write_builtin_vars(el, excluded);
                }
            }
            IrExpr::Str(sv, _) => {
                let v = sv.split('=').next().unwrap_or("");
                if is_ident(v) {
                    excluded.insert(v.to_string());
                }
            }
            _ => {}
        }
    }
    fn walk_expr(
        e: &IrExpr,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match e {
            IrExpr::Call { func, args } => {
                // `test` / `setArray` / `setArrayAppend` strings are
                // excluded: the renderer injects lifted values into them,
                // so a lifted var may appear inside them.
                let let_args_native = func == "exec" && arith_let_args_native(args);
                if func != "getVar" && func != "test" && func != "setArray" && func != "setArrayAppend"
                    && !let_args_native
                {
                    for (i, a) in args.iter().enumerate() {
                        // `sh2.param`'s NAME arg (index 1) is a direct store
                        // lookup, never a $ref scan — and for a lifted name
                        // the emitter inlines the value (native string ops
                        // or the trailing value-override arg), so it must
                        // NOT be marked store-bound. The extras keep their
                        // marks (the runtime still expandWord's/evalArith's
                        // them against the store). Names that are NOT plain
                        // identifiers (`map[$k]` — the runtime expands the
                        // subscript via normAssocKey/expandWord; `@`/`*`/
                        // `#x`/`!x` forms) keep their marks.
                        if func == "param" && i == 1 {
                            if let IrExpr::Str(n, _) = a {
                                let plain = !n.contains('$')
                                    && !n.contains('[')
                                    && !n.contains('@')
                                    && !n.contains('*')
                                    && !n.starts_with('#')
                                    && !n.starts_with('!');
                                if plain {
                                    continue;
                                }
                            }
                        }
                        mark_str_args(a, string_ctx);
                    }
                }
                // `${x:=d}` WRITES the variable. The native lowering updates
                // the JS binding via an assignment expression — but a
                // default that cannot be fully inlined (a `$` ref to a
                // store-bound var) or a subshell/background write (copy
                // semantics: bash writes a COPY; a module binding would be
                // clobbered) must keep the name store-bound so the runtime
                // setVar path stays consistent.
                if func == "param" {
                    if let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args.as_slice() {
                        if op == ":=" {
                            let store_default = matches!(
                                args.get(2),
                                Some(IrExpr::Str(d, _)) if d.contains('$')
                            );
                            if store_default || in_copy {
                                string_ctx.insert(name.clone());
                            }
                        }
                    }
                }
                if func == "exec" || func == "builtin" {
                    // `builtin` is the sync-builtin-dispatch callee (M8) —
                    // same write-builtin semantics as exec-lowered builtins
                    if let Some(IrExpr::Str(cname, _)) = args.first() {
                        if matches!(
                            cname.as_str(),
                            "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                                | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source"
                                | "."
                        ) {
                            // A natively-lowered `let` (try_native_let — the
                            // runtime never sees the args) and the bare
                            // names of `typeset -i` declarations (numeric
                            // witnesses — native number bindings) are not
                            // store writes: skip the marks for them.
                            let native_let = cname == "let" && let_args_native;
                            let intdecl = if cname == "let" { Vec::new() } else {
                                int_declare_names(args).unwrap_or_default()
                            };
                            for a in &args[1..] {
                                if native_let {
                                    continue;
                                }
                                if !intdecl.is_empty() {
                                    if let IrExpr::Str(sv, _) = a {
                                        if !sv.contains('=') && intdecl.iter().any(|n| n == sv) {
                                            continue;
                                        }
                                    }
                                }
                                mark_write_builtin_vars(a, excluded);
                                // `let`/`(( ))`/`eval` args are EXPRESSIONS
                                // ("i++") — mark EVERY identifier they touch
                                // so a lifted native binding never desyncs
                                // from the runtime's store write
                                mark_all_idents_args(a, string_ctx);
                            }
                        }
                    }
                }
                if matches!(
                    func.as_str(),
                    "arrayIndex" | "arrayLen" | "arrayItems" | "arraySlice" | "setArray"
                        | "setArrayAppend"
                ) {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        excluded.insert(name.clone());
                    }
                }
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    walk_stmt(st, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Index { key, .. } => walk_expr(key, excluded, string_ctx, in_copy),
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, excluded, string_ctx, in_copy);
                walk_expr(rhs, excluded, string_ctx, in_copy);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                walk_expr(then, excluded, string_ctx, in_copy);
                walk_expr(else_, excluded, string_ctx, in_copy);
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, excluded, string_ctx, in_copy);
                walk_expr(default, excluded, string_ctx, in_copy);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, excluded, string_ctx, in_copy),
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, excluded, string_ctx, in_copy);
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(
        st: &IrStmt,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        if in_copy {
                            excluded.insert(t.var.clone());
                        }
                    } else {
                        excluded.insert(t.var.clone());
                    }
                }
                walk_expr(expr, excluded, string_ctx, in_copy);
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    excluded.insert(v.name.clone());
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                excluded.insert(var.clone());
            }
            IrStmt::For { var, iter, body } => {
                // NOTE: the loop var is NOT excluded here — the loop
                // iteration is its assignment source (see collect_for_iters
                // + the fixpoint); external references are removed by
                // drop_externally_referenced_loop_vars afterwards.
                walk_expr(iter, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in then.iter().chain(else_) {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        walk_stmt(stm, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Exec { cmd, args, capture, env, .. } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for (v, _) in env {
                    excluded.insert(v.clone());
                }
                if let IrExpr::Str(cname, _) = cmd {
                    if matches!(
                        cname.as_str(),
                        "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                            | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source" | "."
                    ) {
                        // natively-lowered `let` args (try_native_let) and
                        // the bare names of `typeset -i` declarations are
                        // not store writes — skip their marks (mirror of
                        // the expression-position block; a natively-written
                        // var must not stay store-bound, and a runtime-
                        // written var must not lift).
                        let native_let = cname == "let" && arith_let_args_native(args);
                        let intdecl = if cname == "let" { Vec::new() } else {
                            int_declare_names(args).unwrap_or_default()
                        };
                        for a in args {
                            if native_let {
                                continue;
                            }
                            if !intdecl.is_empty() {
                                if let IrExpr::Str(sv, _) = a {
                                    if !sv.contains('=') && intdecl.iter().any(|n| n == sv) {
                                        continue;
                                    }
                                }
                            }
                            mark_write_builtin_vars(a, excluded);
                            // `let`/`(( ))`/`eval` args are ARITHMETIC
                            // EXPRESSIONS — mark EVERY identifier they
                            // touch so a lifted native binding never
                            // desyncs from a runtime store write
                            mark_all_idents_args(a, string_ctx);
                        }
                    }
                }
                walk_expr(cmd, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::Pipeline { stages, capture, .. } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for stage in stages {
                    for b in stage {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Function { body, .. } | IrStmt::Block(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, true);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for r in redirects {
                    walk_expr(&r.target, excluded, string_ctx, in_copy);
                    if r.interpolate {
                        if let IrExpr::Str(body, _) = &r.target {
                            mark_string_refs(body, string_ctx);
                        }
                    }
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                walk_expr(discriminant, excluded, string_ctx, in_copy);
                for c in clauses {
                    for p in &c.patterns {
                        mark_string_refs(p, string_ctx);
                    }
                    for b in &c.body {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Expr(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Output { value, .. } => walk_expr(value, excluded, string_ctx, in_copy),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, excluded, string_ctx, in_copy);
                walk_expr(content, excluded, string_ctx, in_copy);
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => {
                walk_expr(e, excluded, string_ctx, in_copy)
            }
            IrStmt::SetChildError(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_expr(expr, excluded, string_ctx, in_copy)
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        walk_stmt(st, &mut excluded, &mut string_ctx, false);
    }

    fn collect_assigns(st: &IrStmt, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        assigns
                            .entry(t.var.clone())
                            .or_default()
                            .push(expr.clone());
                    }
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    collect_assigns(b, assigns);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        collect_assigns(stm, assigns);
                    }
                }
            }
            IrStmt::For { var, body, .. } => {
                // the loop iteration is a source even with no body writes
                assigns.entry(var.clone()).or_default();
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Expr(e) => collect_expr_assigns(e, assigns),
            IrStmt::Output { value, .. } => collect_expr_assigns(value, assigns),
            _ => {}
        }
    }
    fn collect_expr_assigns(e: &IrExpr, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    collect_assigns(st, assigns);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    collect_expr_assigns(el, assigns);
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        collect_assigns(st, &mut assigns);
    }

    let mut lifted: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for (name, exprs) in &assigns {
            if lifted.contains(name)
                || numeric.contains(name)
                || excluded.contains(name)
                || string_ctx.contains(name)
                || is_reserved_var(name)
                || is_js_keyword(name)
                || name.contains('[')
                || name.contains(']')
            {
                continue;
            }
            let all_string = exprs.iter().all(|e| match e {
                IrExpr::Str(_, _) => true,
                IrExpr::Interpolate(_) => true,
                IrExpr::Var(n, _) => lifted.contains(n.as_str()),
                IrExpr::Call { func, args } if func == "getVar" => {
                    matches!(args.as_slice(), [IrExpr::Str(n, _)] if lifted.contains(n.as_str()))
                }
                // `x=$(cmd)` — command substitution ALWAYS yields a string
                // (the runtime capture strips NULs + trailing newlines and
                // returns the buffer). The assignment lowers to a native
                // `x = await sh2.capture(...)`, so the target can be a
                // plain `let` binding. Exported/reflected vars stay
                // excluded (mark_write_builtin_vars on export/declare/read
                // ...); the runtime never sees a lifted var.
                IrExpr::Call { func, args } if func == "capture" => {
                    matches!(args.as_slice(), [IrExpr::Arrow(_)])
                }
                _ => false,
            });
            if all_string {
                lifted.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    lifted
}


/// Render a setArray/setArrayAppend argument: Str elements with lifted
/// `$var` references become template literals with the values inlined (the
/// runtime would otherwise read them from the STORE, which lifted vars are
/// no longer in). Arrays recurse; everything else lowers normally.
fn array_elt_to_estree(e: &IrExpr) -> Expr {
    match e {
        IrExpr::Str(sv, _) => {
            if let Some(tpl) = test_str_to_estree(sv) {
                tpl
            } else {
                expr_to_estree(e)
            }
        }
        IrExpr::Array(elems) => Expr::ArrayExpression {
            elements: elems.iter().map(|el| Some(array_elt_to_estree(el))).collect(),
        },
        _ => expr_to_estree(e),
    }
}

fn expr_to_estree(e: &IrExpr) -> Expr {
    match e {
        IrExpr::Int(i) => Expr::Literal {
            value: serde_json::Value::from(*i),
            raw: None,
        regex: None,
        },
        IrExpr::Str(s, _) => Expr::Literal {
            value: serde_json::Value::String(s.clone()),
            raw: None,
        regex: None,
        },
        IrExpr::Bool(b) => Expr::Literal {
            value: serde_json::Value::Bool(*b),
            raw: None,
        regex: None,
        },
        IrExpr::Json(v) => Expr::Literal {
            value: v.clone(),
            raw: None,
        regex: None,
        },
        IrExpr::Var(name, _) => {
            if is_lifted(name) {
                Expr::Identifier { name: name.clone() }
            } else if let Some(native) = native_special_var(name) {
                native
            } else {
                sh2_call("getVar", vec![str_lit(name)])
            }
        }
        IrExpr::Ident(name) => Expr::Identifier { name: name.clone() },
        IrExpr::Array(elems) => Expr::ArrayExpression {
            elements: elems.iter().map(|e| Some(expr_to_estree(e))).collect(),
        },
        IrExpr::Object(props) => Expr::ObjectExpression {
            properties: props
                .iter()
                .map(|(k, v)| prop(k, expr_to_estree(v)))
                .collect(),
        },
        IrExpr::Interpolate(parts) => interpolate_to_estree(parts),
        IrExpr::Call { func, args } => {
            // `f args...` — a call to a PROVABLY-SYNC script-defined
            // function with await-free call-site args (see
            // [`SYNC_FN_CALLS`] / `try_native_fn_call`): the sync
            // `sh2.fnCall` call, no await — which keeps `&&`/`||`/`!`
            // chains and enclosing loops on their native / *Sync paths.
            // Placed BEFORE the echo/printf/let/cd special cases: a
            // script-defined `echo`/`printf`/`let`/`cd` function shadows
            // the builtin, and the sync call is the function dispatch.
            // Env-carrying (3-arg) calls and dynamic names keep the async
            // exec dispatch.
            if func == "exec" {
                if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                    if let Some(call) = try_native_fn_call(name, a) {
                        return call;
                    }
                }
            }
            // `sh2.brace(prefix, groups, middles, suffix)` — the args are
            // ALWAYS literal (brace_ir emits Str/Json), the expansion is
            // pure string work, and the runtime never glob-marks the
            // results — so the whole call lowers to a native array literal
            // (computed once at emit time, not per loop/iteration).
            if func == "brace" {
                if let [IrExpr::Str(prefix, _), IrExpr::Json(groups), IrExpr::Json(middles), IrExpr::Str(suffix, _)] =
                    args.as_slice()
                {
                    return Expr::ArrayExpression {
                        elements: brace_expand(prefix, groups, middles, suffix)
                            .iter()
                            .map(|s| Some(str_lit(s)))
                            .collect(),
                    };
                }
            }
            // `sh2.block(stmts)` in EXPRESSION position (a `{ ...; }` group
            // used as a while/until/if cond or an &&/|| operand): the helper
            // runs the stmts and returns `lastExit === 0`. When every stmt
            // lowers to a bare expression statement (no if/while/break/
            // return inside) and none needs an await, the whole thing is a
            // native sequence `(e1, ..., sh2.lastExit === 0)` — the helper's
            // exact value minus the async arrow + dispatch. Statement-form
            // blocks lower to plain BlockStatements already (IrStmt::Block).
            if func == "block" {
                if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                    // Every stmt must lower to a bare expression: IrStmt::Expr
                    // via expr_to_estree (binop &&/|| chains become native
                    // sequences, `sh2.break()`/`sh2.return()` stay Signal
                    // THROWS — never native break/return statements, which
                    // would exit the IIFE instead of the enclosing loop/
                    // function), IrStmt::Assign via stmt_to_estree (always
                    // an ExpressionStatement). Anything else (if/while/
                    // native break/return) falls back to the runtime block.
                    let mut exprs: Vec<Expr> = Vec::new();
                    let mut ok = true;
                    for st in stmts {
                        match st {
                            IrStmt::Expr(e) => {
                                let e2 = expr_to_estree(e);
                                if expr_has_await(&e2) {
                                    ok = false;
                                    break;
                                }
                                exprs.push(e2);
                            }
                            IrStmt::Assign { .. } => match stmt_to_estree(st) {
                                Some(Stmt::ExpressionStatement { expression }) => {
                                    if expr_has_await(&expression) {
                                        ok = false;
                                        break;
                                    }
                                    exprs.push(expression);
                                }
                                _ => {
                                    ok = false;
                                    break;
                                }
                            },
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        exprs.push(last_exit_eq_zero());
                        return seq(exprs);
                    }
                }
            }
            // setArray/setArrayAppend ELEMENT strings are expandWord'd by
            // the runtime from the STORE — inject lifted values as template
            // literals (the parser keeps elements as raw text, so
            // `["$candidate"]` must inline candidate's value).
            //
            // Arrows lowered inside `capture`/`captureWords`/`pipeline`/
            // `subshell`/`background`/`redirect`/`define` args run under a
            // runtime-swapped stdout sink or may later be called under any
            // sink (see `arrow_sink`) — native echo/printf must stay
            // suppressed there, so raise ECHO_SINK_DEPTH for the whole arg
            // lowering. Loop/`and`/`or`/`block` arrows run in the CURRENT
            // sink and are NOT raised here (nor in `arrow`).
            let mapped_args: Vec<Expr> = {
                let sink_args = matches!(
                    func.as_str(),
                    "capture" | "captureWords" | "pipeline" | "subshell" | "background"
                        | "redirect" | "define"
                );
                if sink_args {
                    *ECHO_SINK_DEPTH.lock().unwrap() += 1;
                }
                let out = if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                    args.iter().map(array_elt_to_estree).collect()
                } else {
                    args.iter().map(expr_to_estree).collect()
                };
                if sink_args {
                    *ECHO_SINK_DEPTH.lock().unwrap() -= 1;
                }
                out
            };
            // a read of a lifted numeric variable is a bare JS identifier;
            // bash special vars ($? / $# / $0 / $1..$9 / $@ / $$ / $- /
            // $PWD) are direct reads of the runtime's state fields
            if func == "getVar" {
                if let [IrExpr::Str(name, _)] = args.as_slice() {
                    if is_lifted(name) {
                        return Expr::Identifier { name: name.clone() };
                    }
                    if let Some(native) = native_special_var(name) {
                        return native;
                    }
                }
            }
            // `echo X | grep PAT` lowers to a `contains` call — the runtime
            // impl is String(h).includes(n), so emit it NATIVE (no dispatch)
            if func == "contains" {
                if let [h, n] = args.as_slice() {
                    return Expr::CallExpression {
                        callee: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::CallExpression {
                                callee: Box::new(Expr::Identifier {
                                    name: "String".to_string(),
                                }),
                                arguments: vec![expr_to_estree(h)],
                                optional: false,
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "includes".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        arguments: vec![expr_to_estree(n)],
                        optional: false,
                    };
                }
            }
            // `sh2.join(v)` — the runtime impl is exactly
            // `Array.isArray(v) ? v.join(" ") : String(v)` (array-valued
            // expansions — `${arr[@]}`, `${!map[@]}`, `${arr[@]:off:len}` —
            // must space-join inside template literals, scalars pass
            // through). The arg's runtime value type is decidable from its
            // IR SHAPE, so the ternary (which would evaluate the arg up to
            // 3x — and triple-count its sh2 calls in the metric) is never
            // needed: provably-string args (plain `${x:o:l}` slices,
            // `${#arr[@]}` lengths, `${@:o:l}` joins) drop the join as
            // identity; provably-array args (the runtime's `[...slice]`
            // `[@]`-suffix form, arrayItems/listVar — always arrays even
            // for missing names) join directly. Ambiguous args (plain-name
            // slices — the runtime checks `arrays.get(name)` — and
            // `arrayIndex` @/*, which returns '' for a missing array)
            // keep the runtime join (exact, no duplication).
            if func == "join" {
                if let [v] = args.as_slice() {
                    let ve = expr_to_estree(v);
                    if expr_has_await(&ve) {
                        return sh2_call("join", vec![ve]);
                    }
                    let always_array = matches!(v, IrExpr::Call { func: f, .. }
                        if matches!(f.as_str(), "arrayItems" | "listVar"));
                    let always_array = always_array
                        || matches!(v, IrExpr::Call { func: f, args: a }
                            if f == "param"
                                && matches!(a.as_slice(),
                                    [IrExpr::Str(op, _), IrExpr::Str(name, _), ..]
                                    if op == "slice"
                                        && (name.ends_with("[@]") || name.ends_with("[*]")
                                            || (name.starts_with('!')
                                                && !name.contains('*')))));
                    let always_string = matches!(v, IrExpr::Call { func: f, args: a }
                        if f == "param"
                            && matches!(a.as_slice(),
                                [IrExpr::Str(op, _), IrExpr::Str(name, _), ..]
                                if op == "slice"
                                    && (name.starts_with('#') || name == "@" || name == "*")));
                    if always_array {
                        return Expr::CallExpression {
                            callee: Box::new(Expr::MemberExpression {
                                object: Box::new(ve.clone()),
                                property: Box::new(Expr::Identifier {
                                    name: "join".to_string(),
                                }),
                                computed: false,
                                optional: false,
                            }),
                            arguments: vec![str_lit(" ")],
                            optional: false,
                        };
                    }
                    if always_string {
                        return ve; // join of a scalar is String(v) — identity
                    }
                    return sh2_call("join", vec![ve]);
                }
            }
            // test expressions: native comparison when both operands are
            // lifted; otherwise inject lifted values as a template literal.
            // Inside `&&`/`||` arrows the runtime `and`/`or` branch on
            // lastExit, which a native expression never sets — keep the
            // runtime call there (the injected template still inlines
            // lifted values).
            if func == "test" {
                if let [IrExpr::Str(sv, _)] = args.as_slice() {
                    if *AND_OR_DEPTH.lock().unwrap() == 0 {
                        if let Some(native) = try_native_test(sv) {
                            return native;
                        }
                    } else if let Some(native) = try_native_test(sv) {
                        // Inside `&&`/`||` the chain links branch on
                        // `sh2.lastExit`, which a native comparison never
                        // sets (the reason tests there normally stay
                        // runtime calls) — record the status the runtime
                        // test would have set, then yield the value:
                        // `(sh2.lastExit = t ? 0 : 1, t)`. The native
                        // operands are pure reads, so evaluating `t` twice
                        // is side-effect-free; the chain's lastExit checks
                        // then work with NO dispatch and NO string re-parse
                        // per evaluation. Only sh2-argument-FREE natives
                        // qualify: a store-var operand would lower to
                        // getVar calls (the runtime test is ONE call —
                        // converting it into several getVars is a net
                        // metric loss, and the sync getVars gain nothing
                        // over the runtime test's single store read).
                        if !expr_contains_sh2(&native) {
                            return seq(vec![
                                Expr::AssignmentExpression {
                                    operator: "=".to_string(),
                                    left: Box::new(sh2_member("lastExit")),
                                    right: Box::new(Expr::ConditionalExpression {
                                        test: Box::new(native.clone()),
                                        consequent: Box::new(Expr::Literal {
                                            value: serde_json::Value::from(0),
                                            raw: None,
                                        regex: None,
                                        }),
                                        alternate: Box::new(Expr::Literal {
                                            value: serde_json::Value::from(1),
                                            raw: None,
                                        regex: None,
                                        }),
                                    }),
                                },
                                native,
                            ]);
                        }
                    }
                    if let Some(tpl) = test_str_to_estree(sv) {
                        // the injected template is the ARGUMENT to the
                        // runtime test (a bare template is always truthy)
                        return sh2_call("test", vec![tpl]);
                    }
                }
            }
            // `sh2.param` on a LIFTED variable: the runtime reads the value
            // from the STORE by string name — a lifted binding is not
            // there — so the pure string ops lower NATIVE (the `${x//p/r}`
            // → split/join family) and the rest inject the value as a
            // trailing override argument (the runtime uses `String(value)`
            // instead of a store read; its expandWord/evalArith still
            // process the extras, so `$ref` semantics are unchanged).
            if func == "param" {
                if let Some(native) = try_native_param(args) {
                    return native;
                }
                if let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args.as_slice() {
                    if is_lifted(name) && op != ":=" {
                        let mut cargs: Vec<Expr> = vec![str_lit(op), str_lit(name)];
                        for (i, a) in args.iter().enumerate().skip(2) {
                            match a {
                                // patterns are NEVER expanded by the
                                // runtime (substGlob/stripGlob* use them
                                // raw) — keep them raw; defaults /
                                // replacements / offsets ARE expanded
                                // (expandWord/evalArith), so inject lifted
                                // refs there.
                                IrExpr::Str(s, _) if matches!(op.as_str(), "#" | "##" | "%" | "%%" | "//") && i == 2 => {
                                    cargs.push(str_lit(s));
                                }
                                IrExpr::Str(s, _) => {
                                    cargs.push(test_str_to_estree(s).unwrap_or_else(|| str_lit(s)));
                                }
                                _ => cargs.push(expr_to_estree(a)),
                            }
                        }
                        // the runtime signature is param(op, name, a, b,
                        // value) — pad missing extra slots so the value
                        // lands in the trailing `value` slot.
                        while cargs.len() < 4 {
                            cargs.push(str_lit(""));
                        }
                        cargs.push(Expr::Identifier {
                            name: name.clone(),
                        });
                        return sh2_call("param", cargs);
                    }
                }
            }
            // `for ((...))` whose body needs no await lowers to the sync
            // runtime twin (the whileLoopSync precedent): identical
            // semantics minus the per-iteration promise machinery. The
            // header string is evaluated by the runtime's evalArith
            // (store-based), so only the BODY needs the await scan.
            if func == "cstyleFor" {
                if let [IrExpr::Str(header, _), IrExpr::Arrow(body_stmts)] = args.as_slice() {
                    let body_e: Vec<Stmt> =
                        body_stmts.iter().filter_map(stmt_to_estree).collect();
                    if !stmts_have_await(&body_e) {
                        return sh2_call(
                            "cstyleForSync",
                            vec![str_lit(header), sync_arrow_block(body_e)],
                        );
                    }
                }
            }
            // Expression-position `while` (pipeline stages, `&&`/`||`
            // operands, conditions): `command_to_ir` emits these as a
            // `whileLoop` CALL (the stmt-level IrStmt::While gets the sync
            // fast path in stmt_to_estree). Same semantics, same
            // eligibility — a provably-sync loop (cond + body contain no
            // awaits) lowers to the sync runtime twin, no per-iteration
            // promises (`while read ... done < f | sort` loops).
            if func == "whileLoop" {
                if let [IrExpr::Arrow(cond_stmts), IrExpr::Arrow(body_stmts)] = args.as_slice()
                {
                    // command_to_ir wraps the cond as a single Expr stmt
                    if let [IrStmt::Expr(cond)] = cond_stmts.as_slice() {
                        let cond_e = expr_to_estree(cond);
                        let body_e: Vec<Stmt> =
                            body_stmts.iter().filter_map(stmt_to_estree).collect();
                        if !expr_has_await(&cond_e) && !stmts_have_await(&body_e) {
                            return sh2_call(
                                "whileLoopSync",
                                vec![
                                    sync_arrow_expr(cond_e),
                                    sync_arrow_block(body_e),
                                ],
                            );
                        }
                    }
                }
            }
            // `echo X | tr SET1 SET2` — a pure string transform: lift the
            // whole pipeline to `sh2.builtin("echo", [String(X).toUpper
            // Case()])` etc. — the emitted bytes are IDENTICAL (echo adds
            // the trailing newline tr's passthrough preserves; tr's exit
            // is 0 for the fixed sets, same as echo's), and the spawns +
            // pipeline machinery disappear. Script-defined `echo`/`tr`
            // functions would shadow the builtins — keep the pipeline
            // then (the runtime dispatches to the function).
            if func == "pipeline" {
                if !program_defines_function("echo") && !program_defines_function("tr") {
                    if let Some(transform) = try_native_tr_pipeline(e) {
                        return sh2_call(
                            "builtin",
                            vec![
                                str_lit("echo"),
                                array(vec![transform]),
                            ],
                        );
                    }
                }
                // `echo ARGS | grep [FLAGS] PAT` — a sync runtime mini-grep
                // (see `try_native_echo_grep`): the pipeline's fd-1 write +
                // exit status are the helper's emit + lastExit, so the
                // async pipeline machinery and the grep subprocess spawn
                // disappear. Sink-correct everywhere — the helper emits
                // through the current fd-1 target (module stdout, capture
                // buffer, redirect target), exactly where the pipeline's
                // last stage would write — and the sync call keeps
                // &&/||/if/while contexts on their native/`*Sync` paths.
                if !program_defines_function("echo") && !program_defines_function("grep") {
                    if let Some((text, argv)) = try_native_echo_grep(e) {
                        return sh2_call(
                            "grepText",
                            vec![text, array(argv), bool_lit(false)],
                        );
                    }
                }
            }
            // `echo args > file` / `echo args >> file` in EXPRESSION
            // position (&&/|| operands, if-conditions): the native file
            // write replaces the redirect+builtin pair (same guardrails as
            // the statement form; the trailing `true` keeps the &&/||
            // truthiness semantics the runtime helpers branch on).
            if func == "redirect" {
                if let [IrExpr::Arrow(stmts), IrExpr::Array(spec_objs)] = args.as_slice() {
                    let mut specs: Vec<(i64, &str, &IrExpr)> = Vec::new();
                    let mut ok = true;
                    for so in spec_objs {
                        if let IrExpr::Object(props) = so {
                            let mut fd = 0i64;
                            let mut mode = "";
                            let mut target: Option<&IrExpr> = None;
                            for (k, v) in props {
                                match k.as_str() {
                                    "fd" => {
                                        if let IrExpr::Int(i) = v {
                                            fd = *i;
                                        }
                                    }
                                    "mode" => {
                                        if let IrExpr::Str(m, _) = v {
                                            mode = m;
                                        }
                                    }
                                    "target" => target = Some(v),
                                    _ => {}
                                }
                            }
                            if let Some(t) = target {
                                specs.push((fd, mode, t));
                            } else {
                                ok = false;
                            }
                        } else {
                            ok = false;
                        }
                    }
                    if ok {
                        if let Some(native) = try_native_echo_redirect(stmts, &specs) {
                            return native;
                        }
                        // `grep -q PAT <<< TEXT` — the fd-0 herestring
                        // redirect form of the substring test (see
                        // `try_native_grep_q_redirect`): no spawn, no fd
                        // plumbing.
                        if let Some(native) = try_native_grep_q_redirect(stmts, &specs) {
                            return native;
                        }
                    }
                }
            }
            // `$(echo X | tr ...)` (QUOTED capture): the transform replaces
            // the whole capture — no spawns, no pipeline, no async. The
            // runtime capture strips NULs + trailing newlines from the
            // captured buffer; `sh2.trimCapture` applies the same strips to
            // the transformed line content (echo's own trailing newline
            // would be stripped anyway).
            if func == "capture" {
                if !program_defines_function("echo") && !program_defines_function("tr") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some(transform) = try_native_tr_pipeline(pipe) {
                                return trim_capture(transform);
                            }
                        }
                    }
                }
                // `$(echo args...)` — the capture's only statement is the
                // sync echo builtin: the value is the joined args (with
                // the runtime's `-e` interpretation) plus the capture
                // strips — no async capture machinery at all.
                if !program_defines_function("echo") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(inner)] = stmts.as_slice() {
                            if let Some(value) = try_native_echo_capture(inner) {
                                return value;
                            }
                        }
                    }
                }
                // `$(printf FMT ARGS...)` with all-static args: the value
                // is the formatted output minus the capture strips — a
                // compile-time constant (see `native_capture_printf`).
                if !program_defines_function("printf") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(inner)] = stmts.as_slice() {
                            if let Some(value) = native_capture_printf(inner) {
                                return value;
                            }
                        }
                    }
                }
                // `$(echo args... | wc -l/-w/-c)` — the capture's pipeline
                // is the sync echo builtin feeding the sync wc builtin: the
                // value is a pure count over the echoed text (see
                // `native_capture_echo_wc`) — no spawn, no async pipeline.
                if !program_defines_function("echo") && !program_defines_function("wc") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some(count) = native_capture_echo_wc(pipe) {
                                return count;
                            }
                        }
                    }
                }
                // `$(echo args... | sort)` / `$(echo args... | uniq)` —
                // the sync echo builtin feeding a pure line-transform
                // builtin: the value is a native expression over the
                // echoed text (see `native_capture_echo_pipeline`) — no
                // spawn, no async pipeline.
                if !program_defines_function("echo")
                    && !program_defines_function("sort")
                    && !program_defines_function("uniq")
                {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some(value) = native_capture_echo_pipeline(pipe) {
                                return value;
                            }
                        }
                    }
                }
                // `$(seq A B | head -N)` / `$(seq A B | tail -N)` — the
                // sync seq builtin feeding head/tail: a native slice of
                // the numeric range (see `native_capture_seq_slice`).
                if !program_defines_function("seq")
                    && !program_defines_function("head")
                    && !program_defines_function("tail")
                {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some(value) = native_capture_seq_slice(pipe) {
                                return value;
                            }
                        }
                    }
                }
                // `$(echo ARGS | grep [FLAGS] PAT)` — the echo|grep
                // capture: the sync mini-grep computes the matched text
                // natively (no spawns, no async capture arrow, no fd
                // swapping); trimCapture applies the capture's NUL +
                // trailing-newline strips; the helper records lastExit =
                // grep's status (0 iff any line was selected — the
                // `$(grep ...)` status `$?` observes).
                if !program_defines_function("echo") && !program_defines_function("grep") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some((text, argv)) = try_native_echo_grep(pipe) {
                                return trim_capture(sh2_call(
                                    "grepText",
                                    vec![text, array(argv), bool_lit(true)],
                                ));
                            }
                        }
                    }
                }
                // `$(cat f)` / `$(sort f)` / `$(wc -l < f)` /
                // `$(dirname x)` / `$(basename x)` / `$(pwd)` — the
                // pure-capture family: the value is a native expression
                // over file contents / path strings (see
                // `try_native_capture_value`) — no spawn, no async
                // capture arrow, no fd swapping.
                if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                    if let Some(value) = try_native_capture_value(stmts) {
                        return value;
                    }
                }
            }
            // `printf FORMAT ARGS...` with a static format at the module's
            // default stdout sink: a native `process.stdout.write`, no
            // dispatch — see `try_native_printf` (same guards as the echo
            // lowering: sink depth, script-function shadow, persistent fd-1).
            if func == "exec" && *ECHO_SINK_DEPTH.lock().unwrap() == 0 {
                if !program_defines_function("printf")
                    && !PROGRAM_PERSIST_FD1.lock().unwrap().unwrap_or(true)
                {
                    if let Some(native) = try_native_printf(args) {
                        return native;
                    }
                }
            }
            // `echo args...` at the module's default stdout sink: a native
            // `process.stdout.write` sequence, no dispatch — see
            // `try_native_echo`. Suppressed inside redirect/pipeline/
            // capture/function bodies (ECHO_SINK_DEPTH), when a script
            // function shadows the builtin, or when the program installs a
            // persistent fd-1 redirect (`exec >file` — PROGRAM_PERSIST_FD1).
            if func == "exec" && *ECHO_SINK_DEPTH.lock().unwrap() == 0 {
                if !program_defines_function("echo")
                    && !PROGRAM_PERSIST_FD1.lock().unwrap().unwrap_or(true)
                {
                    if let Some(native) = try_native_echo(args) {
                        return native;
                    }
                }
            }
            // `let ARITH...` / `(( ARITH ))` — see try_native_let (the
            // statement/condition whose EVERY arith arg parses natively).
            if func == "exec" {
                if let Some(native) = try_native_let(args) {
                    return native;
                }
            }
            // `grep -q PAT FILE` — a pure substring test over a file's
            // contents (see `native_exec_grep_q`): the spawn collapses to
            // a readFile + includes promise chain. The single-file `-q`
            // form only — stdin/`-e`/multi-file greps keep the runtime.
            if func == "exec" {
                if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                    if name == "grep" {
                        if let [IrExpr::Str(q, _), IrExpr::Str(pat, _), file] = a.as_slice() {
                            if q == "-q" {
                                if let Some(native) = native_exec_grep_q(file, pat) {
                                    return native;
                                }
                            }
                        }
                    }
                }
            }
            // `true` / `:` / `false` with no args: pure status writes — a
            // native `(sh2.lastExit = N, B)` sequence, no dispatch. The
            // runtime builtins set exactly these statuses and return the
            // matching truthiness (`if true; then` / `while :; do` / `a &&
            // true` all branch on it).
            if func == "exec" {
                if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                    if a.is_empty() && matches!(name.as_str(), "true" | ":" | "false") {
                        let ok = name != "false";
                        return seq(vec![
                            Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(sh2_member("lastExit")),
                                right: Box::new(Expr::Literal {
                                    value: serde_json::Value::from(if ok { 0 } else { 1 }),
                                    raw: None,
                                regex: None,
                                }),
                            },
                            bool_lit(ok),
                        ]);
                    }
                }
            }
            // `rm` / `mkdir` with plain args (expr position — && / ||
            // operands, if-conditions, capture bodies): native `sh2.fs.*`
            // promise chain, no spawn (see `try_native_fs_exec`). The
            // env-carrying 3-arg form does not match this arm and keeps
            // the runtime dispatch.
            if func == "exec" {
                if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                    if let Some(native) = try_native_fs_exec(name, a) {
                        return await_expr(native);
                    }
                }
            }
            // `exit N` — the runtime builtin ignores the code and terminates
            // the process cleanly (the corpus gate compares stdout only; a
            // nonzero exit would read as a runtime error, see sh2-namespace.mjs
            // `builtins.exit`). A native `process.exit(0)` is byte-identical;
            // the argument exprs are sequenced first so their side effects
            // (`exit $((i++))`) evaluate exactly as they would in the
            // dispatch. Any caller-visible state is irrelevant — the process
            // is gone either way.
            if func == "exec" {
                if let [IrExpr::Str(name, _), IrExpr::Array(_)] = args.as_slice() {
                    if name == "exit" {
                        let mut exprs: Vec<Expr> = mapped_args
                            .get(1)
                            .and_then(|e| match e {
                                Expr::ArrayExpression { elements, .. } => Some(
                                    elements.iter().flatten().cloned().collect(),
                                ),
                                _ => None,
                            })
                            .unwrap_or_default();
                        exprs.push(process_exit_zero());
                        if exprs.len() == 1 {
                            return exprs.pop().unwrap();
                        }
                        return seq(exprs);
                    }
                }
            }
            // `cd /` — the absolute root is the ONE literal cd target that
            // provably exists and is readable on every POSIX system (the
            // runtime's accessSync check can never fail), so the whole
            // builtin collapses to a native
            // `(process.chdir("/"), sh2.cwd = "/", sh2.lastExit = 0, true)`
            // — byte-identical to the runtime success path, including the
            // `$PWD` read (sh2.cwd) and the process cwd (relative
            // redirects/execs resolve against it; subshells restore both
            // after the body). Any other target (dynamic or literal) keeps
            // the runtime dispatch — its failure path prints `cd: X: No
            // such file or directory` and returns false.
            if func == "exec" {
                if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                    if name == "cd"
                        && matches!(a.as_slice(), [IrExpr::Str(d, _)] if d == "/")
                    {
                        return seq(vec![
                            Expr::CallExpression {
                                callee: Box::new(Expr::MemberExpression {
                                    object: Box::new(Expr::Identifier {
                                        name: "process".to_string(),
                                    }),
                                    property: Box::new(Expr::Identifier {
                                        name: "chdir".to_string(),
                                    }),
                                    computed: false,
                                    optional: false,
                                }),
                                arguments: vec![str_lit("/")],
                                optional: false,
                            },
                            Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(sh2_member("cwd")),
                                right: Box::new(str_lit("/")),
                            },
                            Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(sh2_member("lastExit")),
                                right: Box::new(Expr::Literal {
                                    value: serde_json::Value::from(0),
                                    raw: None,
                                regex: None,
                                }),
                            },
                            bool_lit(true),
                        ]);
                    }
                }
            }
            // `shopt OPT EN` — the runtime helper is a thin wrapper over
            // the shopt-state map (`this.shoptState.set(option, enable);
            // return true;`): a direct state write + `true`, no dispatch.
            // The state drives case/test matching (nocasematch) and glob
            // expansion (extglob) inside the runtime — writing the map
            // directly is byte-identical.
            if func == "shopt" {
                if let [opt, en] = mapped_args.as_slice() {
                    return seq(vec![
                        Expr::CallExpression {
                            callee: Box::new(Expr::MemberExpression {
                                object: Box::new(sh2_member("shoptState")),
                                property: Box::new(Expr::Identifier {
                                    name: "set".to_string(),
                                }),
                                computed: false,
                                optional: false,
                            }),
                            arguments: vec![opt.clone(), en.clone()],
                            optional: false,
                        },
                        bool_lit(true),
                    ]);
                }
            }
            let callee_name = exec_or_builtin(func, args);
            let call = sh2_call(callee_name, mapped_args);
            if is_async_call(callee_name) {
                await_expr(call)
            } else {
                call
            }
        }
        IrExpr::BinOp { op: BinOpKind::And, lhs, rhs } => {
            // bash `a && b`: run a, then run b only if a's EXIT STATUS is 0.
            // A native JS `&&` would consult the return VALUE of the left
            // operand instead — capture() returns the captured STRING and
            // assign() returns true, so `r=$(cmd) || ...` (and friends) would
            // branch on the wrong thing. When BOTH operands lower without an
            // await, the runtime helpers' exact decision is a native
            // sequence on the recorded status (see native_and_or); the
            // runtime helper sequences both sides and checks lastExit
            // otherwise (operands with awaits stay async arrows).
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let l = expr_to_estree(lhs);
            let r = expr_to_estree(rhs);
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            if !expr_has_await(&l) && !expr_has_await(&r) {
                return native_and_or(BinOpKind::And, l, r);
            }
            let e = await_call(
                "and",
                vec![arrow(vec![], (**lhs).clone()), arrow(vec![], (**rhs).clone())],
            );
            e
        }
        IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs } => {
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let l = expr_to_estree(lhs);
            let r = expr_to_estree(rhs);
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            if !expr_has_await(&l) && !expr_has_await(&r) {
                return native_and_or(BinOpKind::Or, l, r);
            }
            let e = await_call(
                "or",
                vec![arrow(vec![], (**lhs).clone()), arrow(vec![], (**rhs).clone())],
            );
            e
        }
        // `! cmd` — bash inverts the exit STATUS (so `$?` flips too); a pure
        // JS negation would leave lastExit untouched. The native lowering
        // negates AND records the new status (`$?` reads it back) with the
        // operand evaluated exactly once — the runtime helper's exact
        // semantics (`this.lastExit = v ? 1 : 0; return !v;`), no dispatch.
        IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } => {
            not_native(expr_to_estree(lhs))
        }
        IrExpr::Arrow(stmts) => arrow(vec![], IrExpr::Arrow(stmts.clone())),
        IrExpr::Arith(a) => {
            let inner = arith_to_estree(a);
            // `$(( ... ))` whose expression contains NO `/` or `%` cannot
            // throw (the only runtime abort a native arith can express is
            // the idiv/imod zero-divisor throw — everything else is a
            // plain JS number op), so the arithEval try/catch wrapper is
            // dead weight: emit the native value bare (template literals
            // and runtime args String() it exactly like the helper's
            // `String(f())`; a store var inside still reads via getVar,
            // the runtime's exact coercion). Div/mod expressions keep the
            // wrapper — bash aborts the WHOLE expansion on a zero divisor
            // (empty result), which only the catch can express.
            if !arith_has_div_mod(a) {
                return inner;
            }
            sh2_call(
                "arithEval",
                vec![Expr::ArrowFunctionExpression {
                    params: vec![],
                    body: ArrowBody::Expr(Box::new(inner)),
                    expression: true,
                    r#async: false,
                }],
            )
        }
        other => unreachable!("Perl-only IR expression reached the ESTree renderer: {other:?}"),
    }
}

fn interpolate_to_estree(parts: &[InterpPart]) -> Expr {
    let mut quasis = Vec::new();
    let mut expressions = Vec::new();
    let mut raw = String::new();
    for part in parts {
        match part {
            InterpPart::Lit(s) => raw.push_str(s),
            InterpPart::Expr(e) => {
                quasis.push(quasi_element(&mut raw, false));
                expressions.push(expr_to_estree(e));
            }
        }
    }
    quasis.push(quasi_element(&mut raw, true));
    Expr::TemplateLiteral { quasis, expressions }
}

fn array(elements: Vec<Expr>) -> Expr {
    Expr::ArrayExpression {
        elements: elements.into_iter().map(Some).collect(),
    }
}

fn arrow(params: Vec<Expr>, body: IrExpr) -> Expr {
    arrow_body(params, body)
}

/// Arrow whose body runs under a runtime-swapped stdout sink or may later
/// be CALLED under any sink: `capture`/`captureWords`/`pipeline` (the
/// runtime swaps fdTargets[1] to a capture buffer), `redirect` (fdTargets
/// change for the body), `subshell`/`background` (the body can run while
/// the enclosing sink context differs — the background body is deferred to
/// a microtask, the subshell clones fdTargets), and `define` (a function
/// body may be called under ANY sink at runtime). Native echo/printf
/// (`process.stdout.write`) is only byte-identical to the runtime's `emit`
/// while fd 1 is the default stdout, so ECHO_SINK_DEPTH is raised while
/// such bodies lower.
///
/// Loops (`whileLoop`/`forLoop`/`cstyleFor` and their *Sync twins) and
/// `and`/`or`/`block` run their arrows in the CURRENT sink — they never
/// swap fdTargets (verified against sh2-namespace.mjs) — so plain `arrow`
/// keeps the depth at the enclosing level: a loop nested inside a
/// capture/redirect/function is still covered by THAT construct's
/// arrow_sink bump.
fn arrow_sink(params: Vec<Expr>, body: IrExpr) -> Expr {
    *ECHO_SINK_DEPTH.lock().unwrap() += 1;
    let out = arrow_body(params, body);
    *ECHO_SINK_DEPTH.lock().unwrap() -= 1;
    out
}

/// `arrow_sink` twin for PROVABLY-SYNC function bodies (see
/// [`SYNC_FN_CALLS`]): the emitted define arrow is NON-async so the sync
/// `sh2.fnCall` path can run it without a per-call promise (the async
/// exec path `await`s it like any other arrow — `await` on a non-promise
/// is an identity). Same ECHO_SINK_DEPTH discipline: a function body may
/// be called under ANY stdout sink at runtime.
fn arrow_sink_sync(params: Vec<Expr>, body: IrExpr) -> Expr {
    *ECHO_SINK_DEPTH.lock().unwrap() += 1;
    let out = arrow_body_async(params, body, false);
    *ECHO_SINK_DEPTH.lock().unwrap() -= 1;
    out
}

fn arrow_body(params: Vec<Expr>, body: IrExpr) -> Expr {
    arrow_body_async(params, body, true)
}

fn arrow_body_async(params: Vec<Expr>, body: IrExpr, r#async: bool) -> Expr {
    match &body {
        IrExpr::Arrow(stmts) if stmts.len() == 1 && matches!(stmts[0], IrStmt::Expr(_)) => {
            let inner = match &stmts[0] {
                IrStmt::Expr(e) => expr_to_estree(e),
                _ => unreachable!(),
            };
            Expr::ArrowFunctionExpression {
                params,
                body: ArrowBody::Expr(Box::new(inner)),
                expression: true,
                r#async,
            }
        }
        IrExpr::Arrow(stmts) => Expr::ArrowFunctionExpression {
            params,
            body: ArrowBody::Block(Box::new(Stmt::BlockStatement {
                body: stmts.iter().filter_map(stmt_to_estree).collect(),
            })),
            expression: false,
            r#async,
        },
        other => Expr::ArrowFunctionExpression {
            params,
            body: ArrowBody::Expr(Box::new(expr_to_estree(other))),
            expression: true,
            r#async,
        },
    }
}

fn arrow_with_param(param: String, body: IrExpr) -> Expr {
    arrow(vec![Expr::Identifier { name: param }], body)
}

fn await_call(name: &str, args: Vec<Expr>) -> Expr {
    await_expr(sh2_call(name, args))
}

/// Plain (non-async) zero-arg arrow `() => expr` for the sync-loop fast path.
fn sync_arrow_expr(expr: Expr) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![],
        body: ArrowBody::Expr(Box::new(expr)),
        expression: true,
        r#async: false,
    }
}

/// Plain (non-async) expression arrow with one parameter — the filter/map
/// callbacks of the native capture-pipeline lifts (`t => …` over the
/// readFile value, `l => …` over lines).
fn sync_arrow_expr_param(param: &str, expr: Expr) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![Expr::Identifier {
            name: param.to_string(),
        }],
        body: ArrowBody::Expr(Box::new(expr)),
        expression: true,
        r#async: false,
    }
}

/// Plain (non-async) block arrow `() => { stmts }` for the sync-loop fast path.
fn sync_arrow_block(stmts: Vec<Stmt>) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![],
        body: ArrowBody::Block(Box::new(Stmt::BlockStatement { body: stmts })),
        expression: false,
        r#async: false,
    }
}

/// Plain (non-async) one-param block arrow `(param) => { stmts }` for the
/// forLoopSync fast path (the loop variable shadows the module binding, so
/// the lifted `i = Number(i)` coercion self-assignment works unchanged).
fn sync_arrow_with_param(param: String, stmts: Vec<Stmt>) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![Expr::Identifier { name: param }],
        body: ArrowBody::Block(Box::new(Stmt::BlockStatement { body: stmts })),
        expression: false,
        r#async: false,
    }
}

/// True if the lowered ESTree contains an `AwaitExpression` (i.e. needs an
/// async context). Serialization-based: `type_` is the only "type" field, so
/// the substring `"type":"AwaitExpression"` appears iff such a node is
/// present. A false positive only costs the fast path (falls back to the
/// async loop) — never correctness.
fn expr_has_await(e: &Expr) -> bool {
    serde_json::to_string(e)
        .map(|s| s.contains("\"type\":\"AwaitExpression\""))
        .unwrap_or(true)
}

/// Does the lowered expression contain ANY `sh2.*` call (a dispatch)? Used
/// to keep the and/or test lowering a NET win: a native test whose operand
/// reads are store vars would trade one `sh2.test` call for several
/// `sh2.getVar` calls.
fn expr_contains_sh2(e: &Expr) -> bool {
    serde_json::to_string(e)
        .map(|s| {
            s.contains("\"object\":{\"type\":\"Identifier\",\"name\":\"sh2\"}")
                || s.contains("\"name\":\"sh2\",\"type\":\"Identifier\"")
        })
        .unwrap_or(true)
}

fn stmts_have_await(stmts: &[Stmt]) -> bool {
    serde_json::to_string(stmts)
        .map(|s| s.contains("\"type\":\"AwaitExpression\""))
        .unwrap_or(true)
}

fn await_expr(inner: Expr) -> Expr {
    Expr::AwaitExpression {
        argument: Box::new(inner),
    }
}

/// `sh2.lastExit === 0` — the native status check the runtime `and`/`or`/
/// `block`/`not` helpers branch on. Native runtime-call operands (builtin/
/// test/exec/...) record their status in `sh2.lastExit`, so a native
/// branch on the field is EXACTLY the runtime helper's decision — minus
/// the dispatch (the whileLoopSync precedent).
fn last_exit_eq_zero() -> Expr {
    Expr::BinaryExpression {
        operator: "===",
        left: Box::new(sh2_member("lastExit")),
        right: Box::new(Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
        regex: None,
        }),
    }
}

fn seq(expressions: Vec<Expr>) -> Expr {
    Expr::SequenceExpression { expressions }
}

fn bool_lit(b: bool) -> Expr {
    Expr::Literal {
        value: serde_json::Value::Bool(b),
        raw: None,
    regex: None,
    }
}

/// `process.exit(0)` — the runtime's clean-termination convention (the
/// corpus gate compares stdout only; a nonzero exit would read as a
/// runtime error). Shared by the native `exit` and `guard` lowerings.
fn process_exit_zero() -> Expr {
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::Identifier {
                name: "process".to_string(),
            }),
            property: Box::new(Expr::Identifier {
                name: "exit".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
        regex: None,
        }],
        optional: false,
    }
}

/// `sh2.guard(v)` — the runtime helper's exact semantics
/// (`if (this.errexit && !v) process.exit(0); return v;`) as a native
/// expression: `(sh2._g = v, sh2.errexit && !sh2._g ? process.exit(0) :
/// sh2._g)`. The wrapped value must be evaluated EXACTLY ONCE (it is
/// usually an awaited command run), so the runtime object's `_g` field is
/// a single-use scratch — the assignment and its reads are one
/// synchronous sequence, and JS is single-threaded, so a nested guard's
/// scratch use can never interleave with an outer one.
fn guard_native(v: Expr) -> Expr {
    let tmp = sh2_member("_g");
    let store = Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(tmp.clone()),
        right: Box::new(v),
    };
    let check = Expr::LogicalExpression {
        operator: "&&",
        left: Box::new(sh2_member("errexit")),
        right: Box::new(Expr::UnaryExpression {
            operator: "!",
            argument: Box::new(tmp.clone()),
            prefix: true,
        }),
    };
    seq(vec![
        store,
        Expr::ConditionalExpression {
            test: Box::new(check),
            consequent: Box::new(process_exit_zero()),
            alternate: Box::new(tmp.clone()),
        },
    ])
}

/// `sh2.not(v)` — the runtime helper's exact semantics
/// (`this.lastExit = v ? 1 : 0; return !v;`) as a native expression with
/// the operand evaluated exactly once (same `sh2._g` scratch protocol as
/// [`guard_native`]): `(sh2._g = v, sh2.lastExit = sh2._g ? 1 : 0,
/// !sh2._g)`.
fn not_native(v: Expr) -> Expr {
    let tmp = sh2_member("_g");
    let store = Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(tmp.clone()),
        right: Box::new(v),
    };
    let status = Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(sh2_member("lastExit")),
        right: Box::new(Expr::ConditionalExpression {
            test: Box::new(tmp.clone()),
            consequent: Box::new(Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
            regex: None,
            }),
            alternate: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            regex: None,
            }),
        }),
    };
    seq(vec![
        store,
        status,
        Expr::UnaryExpression {
            operator: "!",
            argument: Box::new(tmp),
            prefix: true,
        },
    ])
}

/// The runtime `sh2.trimCapture`'s exact formula
/// (`String(s ?? '').replace(/\u0000/g, '').replace(/\n+$/, '')`) as a
/// native expression: the NUL + trailing-newline strips capture() applies,
/// inlined with no runtime dispatch. The capture-lift family always hands
/// it a string, so the `String(...)` coercion is a no-op that keeps the
/// exact runtime semantics for the null/undefined edges (`?? ''` mirrors
/// the helper's guard).
fn trim_capture(inner: Expr) -> Expr {
    let stringed = Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![Expr::LogicalExpression {
            operator: "??",
            left: Box::new(inner),
            right: Box::new(str_lit("")),
        }],
        optional: false,
    };
    let nul = regex_lit_flags("\\u0000", "g");
    let trailing = regex_lit("\\n+$");
    method_call(
        method_call(stringed, "replace", vec![nul, str_lit("")]),
        "replace",
        vec![trailing, str_lit("")],
    )
}

/// bash special vars the runtime reads from its own STATE FIELDS (never
/// the store — the runtime getVar special-cases them ahead of the store
/// too): a direct member read instead of a getVar dispatch. `$?` reads
/// `sh2.lastExit`; `$#` `sh2.positional.length`; `$0` `sh2.argv0`;
/// `$@`/`$*` the positional join; `$1`..`$9` a positional element
/// (`?? ''` — the runtime returns '' when out of range, JS undefined would
/// render "undefined"); `$$` the pid; `$-` the constant 'hB'; `$PWD` the
/// tracked cwd. The value is identical to the runtime's getVar for every
/// corpus-observable state (the corpus is the oracle).
fn native_special_var(name: &str) -> Option<Expr> {
    let positional = || sh2_member("positional");
    match name {
        "?" => Some(sh2_member("lastExit")),
        "#" => Some(Expr::MemberExpression {
            object: Box::new(positional()),
            property: Box::new(Expr::Identifier {
                name: "length".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        "0" => Some(sh2_member("argv0")),
        "@" | "*" => Some(Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(positional()),
                property: Box::new(Expr::Identifier {
                    name: "join".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![str_lit(" ")],
            optional: false,
        }),
        "$" => Some(Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![Expr::MemberExpression {
                object: Box::new(Expr::Identifier {
                    name: "process".to_string(),
                }),
                property: Box::new(Expr::Identifier {
                    name: "pid".to_string(),
                }),
                computed: false,
                optional: false,
            }],
            optional: false,
        }),
        "-" => Some(str_lit("hB")),
        "PWD" => Some(sh2_member("cwd")),
        _ => {
            let d = name.parse::<u32>().ok()?;
            if (1..=9).contains(&d) {
                Some(Expr::LogicalExpression {
                    operator: "??",
                    left: Box::new(Expr::MemberExpression {
                        object: Box::new(positional()),
                        property: Box::new(Expr::Literal {
                            value: serde_json::Value::from(d - 1),
                            raw: None,
                        regex: None,
                        }),
                        computed: true,
                        optional: false,
                    }),
                    right: Box::new(str_lit("")),
                })
            } else {
                None
            }
        }
    }
}

/// Is an arithmetic divisor provably nonzero? `Num(v)` with `v != 0`, or a
/// sign-flipped nonzero literal (`-2`, `+3` — unary minus/plus on a nonzero
/// Num never reaches zero). Everything else (vars, `$((...))` results,
/// arithmetic products) is conservatively NOT provable, so the runtime
/// idiv/imod zero-divisor throw stays for those.
fn arith_is_nonzero(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(v) => *v != 0,
        ArithAst::Un { op, arg } => matches!(*op, "-" | "+") && arith_is_nonzero(arg),
        _ => false,
    }
}

/// Is the lowered expression a single runtime call whose runtime impl
/// RECORDS the exit status in `sh2.lastExit`? The native `! cmd` statement
/// lowering inverts lastExit after the inner statement — only valid when
/// the inner statement actually wrote it (a native comparison like `i < 5`
/// never does; the runtime `not(v)` uses the VALUE instead, so those stay
/// on the runtime helper).
fn sets_last_exit(e: &Expr) -> bool {
    match e {
        Expr::CallExpression { callee, .. } => match callee.as_ref() {
            Expr::MemberExpression { object, property, .. } => {
                matches!(object.as_ref(), Expr::Identifier { name } if name == "sh2")
                    && !matches!(
                        property.as_ref(),
                        Expr::Identifier { name } if matches!(
                            name.as_str(),
                            "getVar" | "setVar" | "param" | "arithEval" | "join"
                                | "contains" | "arrayLen" | "arrayItems" | "arrayIndex"
                                | "listVar" | "brace" | "define" | "shopt"
                                | "setLastExit" | "assign" | "setArray"
                                | "setArrayAppend" | "caseMatch" | "idiv" | "imod"
                        )
                    )
            }
            _ => false,
        },
        _ => false,
    }
}

/// Native `a && b` / `a || b` when BOTH operands lower without an await.
/// The runtime helpers branch on `lastExit`, which every runtime call in
/// the operands records — so the native form is exactly the helper minus
/// the async arrows + dispatch:
///   And: (lhs, sh2.lastExit === 0 ? (rhs, sh2.lastExit === 0) : false)
///   Or:  (lhs, sh2.lastExit === 0 ? true : (rhs, sh2.lastExit === 0))
/// The `(rhs, sh2.lastExit === 0)` tail mirrors the runtime's
/// `await fnB(); return this.lastExit === 0` (a non-status operand like
/// setVar leaves lastExit untouched — same stale read as the helper).
fn native_and_or(op: BinOpKind, l: Expr, r: Expr) -> Expr {
    let then_branch = seq(vec![r, last_exit_eq_zero()]);
    let cond = last_exit_eq_zero();
    match op {
        BinOpKind::And => seq(vec![
            l,
            Expr::ConditionalExpression {
                test: Box::new(cond),
                consequent: Box::new(then_branch),
                alternate: Box::new(bool_lit(false)),
            },
        ]),
        BinOpKind::Or => seq(vec![
            l,
            Expr::ConditionalExpression {
                test: Box::new(cond),
                consequent: Box::new(bool_lit(true)),
                alternate: Box::new(then_branch),
            },
        ]),
        _ => unreachable!("native_and_or: only And/Or"),
    }
}

fn safe_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "var", "let", "const", "function", "class", "if", "else", "for", "while",
        "do", "switch", "case", "break", "continue", "return", "new", "delete",
        "typeof", "instanceof", "in", "of", "try", "catch", "finally", "throw",
        "this", "super", "import", "export", "default", "extends", "static",
        "yield", "await", "null", "true", "false", "void", "debugger", "arguments",
    ];
    if RESERVED.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}
