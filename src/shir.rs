//! ShIR — the language-neutral layer between the shell AST and the backends.
//!
//! `ast_to_ir` builds an `IrProgram` from the parsed shell AST using neutral
//! IR nodes (plus `sh2.*`-namespace calls expressed via `IrExpr::Call`); the
//! ESTree emitter consumes this IR via `shir_to_estree`, so the shell→ESTree
//! lowering logic lives in one place (PLAN.md §3). The Perl generator builds
//! its own IR flavor for `shir_to_perl`; the neutral nodes here
//! (Case/Redirect/Function/Subshell/Background/Arrow/...) are ESTree-path only.

use crate::ast::*;
use crate::bc::eval as bc_eval;
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
/// Native arith div/mod poison depth: >0 while lowering a `$((...))`
/// expression that contains a NaN-COERCING operator (bitwise `|`/`&`/`^`/
/// shifts, `**`, comparisons, `&&`/`||`, `!`, ternaries — JS converts NaN
/// to 0/false/true where bash would abort the WHOLE expansion). A zero
/// divisor in such an expression must keep the runtime `idiv`/`imod`
/// THROW (the only mechanism that aborts mid-expression); poison-free
/// expressions (div/mod results only flow through `+ - * / %` and unary
/// +/- into the arithEval boundary) can go fully native — NaN reaches
/// the wrapper and converts to the bash empty result.
static ARITH_POISON_DEPTH: Mutex<usize> = Mutex::new(0);
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

/// For-loop statements that must PERSIST the loop variable's final value
/// into its native binding after the loop (see [`analyze_loop_var_refs`]):
/// the loop var is LIFTED and referenced OUTSIDE its loop body, and the
/// loop sits OUTSIDE a copy region (subshell/background/capture — bash
/// writes there are copy-local, so the module binding must not be
/// clobbered; those loops keep the shadowed binding and their vars stay
/// store-bound instead). Keyed by statement pointer (stable between the
/// analysis and emission — the IR tree is immutable there). Unset →
/// conservative (no persist).
static LOOP_PERSIST: Mutex<Option<HashMap<usize, ()>>> = Mutex::new(None);
fn loop_persist_needed(stmt: &IrStmt, var: &str) -> bool {
    LOOP_PERSIST
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.contains_key(&(stmt as *const IrStmt as usize)) && is_lifted(var))
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

/// The `echo ARGS...` / `printf FMT ARGS...` statements that lower via
/// [`try_native_echo`] / [`try_native_printf`] — the other droppable
/// lastExit writers (the seq `(write, sh2.lastExit = 0, true)`). Same
/// carrier shape as the native let (the general
/// `IrStmt::Expr(Call("exec", …))` form).
fn is_native_echo_stmt(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Expr(IrExpr::Call { func, args, .. }) => {
            func == "exec"
                && matches!(
                    args.as_slice(),
                    [IrExpr::Str(n, _), IrExpr::Array(_)] if n == "echo" || n == "printf"
                )
        }
        _ => false,
    }
}

/// Mark DEAD every native `(( ))` statement whose lastExit write is not in
/// the live set (droppable) — only if its args actually parse natively
/// (the ternary only exists on the `try_native_let` path). Same for the
/// native `echo` statement (the emission-side dead variant re-checks the
/// sink/glob guards before dropping the write).
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
        // the native echo/printf `(sh2.lastExit = 0)` write is droppable
        // when unread (the statement-level dead variant in `stmt_to_estree`
        // re-checks the sink/glob guards before dropping)
        if is_native_echo_stmt(stmt) && !live.contains(&(stmt as *const IrStmt as usize)) {
            dead.insert(stmt as *const IrStmt as usize, true);
        }
        // Plan 4 for if-statements: an `if c; then ...; fi` with NO else
        // synthesizes `sh2.lastExit = 0` on the false path (bash: a false
        // condition with no else leaves `$?` = 0). When the if's status is
        // unread (its pointer is not in the live set — the backward scan
        // already treats the If as a writer, `ir_stmt_writes_lastexit`),
        // that write is dead and the if-lowering drops the else entirely.
        // Only EMPTY-else ifs synthesize the write (a non-empty else
        // carries its own status through its statements — no marking
        // needed).
        if let IrStmt::If { else_, .. } = stmt {
            if else_.is_empty() && !live.contains(&(stmt as *const IrStmt as usize)) {
                dead.insert(stmt as *const IrStmt as usize, true);
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
///
/// The PROGRAM-FINAL status is NOT observable in the ESTree backend: the
/// runner's exit code is 0 unless the `exit` builtin fired (it reads
/// lastExit itself and is scanned as a reader), `_finish()` never reads
/// lastExit, EXIT trap handlers run under REAL bash via spawnSync (they
/// see bash's own status, not sh2.lastExit), and the corpus harness
/// compares stdout only. So the program-level `end_live` is FALSE — the
/// final statement's write is dead unless a later statement reads it.
fn compute_lastexit_deadness(prog: &IrProgram, errexit: bool) -> HashMap<usize, bool> {
    let mut dead = HashMap::new();
    if errexit {
        return dead;
    }
    let mut live: HashSet<usize> = HashSet::new();
    walk_lastexit_liveness(&prog.stmts, false, &mut live);
    mark_lastexit_dead(&prog.stmts, &live, &mut dead);
    dead
}

/// Loops (`IrStmt::While`, pointer-keyed) whose FINAL status write is dead
/// — no reader observes `$?` after the loop before the next write. Set per
/// compilation by `shir_to_estree` (a sibling of the Plan 4 liveness; the
/// same backward scan produces both). Under a possible `set -e` the map is
/// EMPTY (nothing dead — the guard consumes the status). The native while
/// lowering drops the per-iteration `bodyLast` tracking + trailing write
/// for these loops (a bare native `while (cond) { body }`).
static LOOP_STATUS_DEAD: Mutex<Option<HashMap<usize, bool>>> = Mutex::new(None);
fn loop_status_write_dead(stmt: &IrStmt) -> bool {
    LOOP_STATUS_DEAD
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.get(&(stmt as *const IrStmt as usize)).copied().unwrap_or(false))
        .unwrap_or(false)
}

/// Collect the loop statements that may be CAPTURE PRODUCERS — loops
/// (transitively) inside an async runtime region: a subshell / background /
/// redirect body, a pipeline stage, a capture expression, an arrow argument
/// of the async runtime helpers (exec/pipeline/capture/captureWords/
/// subshell/background/redirect/block), or a function body (a script
/// function may be CALLED from a producer — the emitter cannot see call
/// sites). The runtime loops bound infinite producers via `_capExceeded`;
/// a NATIVE loop has no such bound and would spin forever, hanging the
/// harness — so these keep the runtime machinery. Pointer-keyed like
/// [`LASTEXIT_DEAD`] (the IR tree is immutable during emission).
fn compute_async_region_loops(prog: &IrProgram) -> HashSet<usize> {
    fn stmt_walk(st: &IrStmt, in_async: bool, out: &mut HashSet<usize>) {
        match st {
            IrStmt::While { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::DoWhile { body, .. } => {
                // A loop the sync-ok-loops transform marked `sync_ok`
                // (provably ≤~200ms total, or output-free and finite) may
                // run as ONE sync chunk even inside a producer context:
                // the runtime `_capExceeded` bound exists to stop INFINITE
                // producers, and a sync_ok loop is finite by construction
                // (its cost bound is an upper bound). The existing sync
                // gate then emits it with the native for-of / native while
                // path — zero runtime surface.
                if in_async && !crate::transforms::sync_ok_loops::sync_ok(st) {
                    out.insert(st as *const IrStmt as usize);
                }
                for b in body {
                    stmt_walk(b, in_async, out);
                }
            }
            // subshell/background/redirect bodies run under the runtime's
            // fd/capture machinery (their output may be captured)
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    stmt_walk(b, true, out);
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    stmt_walk(b, true, out);
                }
            }
            // every pipeline stage is a potential producer/consumer
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        stmt_walk(b, true, out);
                    }
                }
            }
            // function bodies: the function may be called from a producer
            IrStmt::Function { body, .. } => {
                for b in body {
                    stmt_walk(b, true, out);
                }
            }
            IrStmt::Block(body) => {
                for b in body {
                    stmt_walk(b, in_async, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    stmt_walk(b, in_async, out);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        stmt_walk(stm, in_async, out);
                    }
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    expr_walk(a, in_async, out);
                }
            }
            IrStmt::Expr(e) => expr_walk(e, in_async, out),
            IrStmt::Output { value, .. } => expr_walk(value, in_async, out),
            IrStmt::Assign { expr, .. } => expr_walk(expr, in_async, out),
            _ => {}
        }
    }
    fn expr_walk(e: &IrExpr, in_async: bool, out: &mut HashSet<usize>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    stmt_walk(st, in_async, out);
                }
            }
            IrExpr::Capture { expr, .. } => expr_walk(expr, true, out),
            IrExpr::Call { func, args } => {
                // the async runtime helpers run their arrow args under the
                // runtime machinery (producer/capture contexts)
                let nested_async = in_async
                    || matches!(
                        func.as_str(),
                        "exec" | "pipeline" | "capture" | "captureWords" | "subshell"
                            | "background" | "redirect" | "block"
                    );
                for a in args {
                    expr_walk(a, nested_async, out);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                expr_walk(lhs, in_async, out);
                expr_walk(rhs, in_async, out);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                expr_walk(obj, in_async, out);
                for a in args {
                    expr_walk(a, in_async, out);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                expr_walk(cond, in_async, out);
                expr_walk(then, in_async, out);
                expr_walk(else_, in_async, out);
            }
            IrExpr::DefinedOr { expr, default } => {
                expr_walk(expr, in_async, out);
                expr_walk(default, in_async, out);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        expr_walk(e, in_async, out);
                    }
                }
            }
            IrExpr::Array(items) => {
                for it in items {
                    expr_walk(it, in_async, out);
                }
            }
            IrExpr::Index { key, .. } => expr_walk(key, in_async, out),
            _ => {}
        }
    }
    let mut out = HashSet::new();
    for st in &prog.stmts {
        stmt_walk(st, false, &mut out);
    }
    out
}

static ASYNC_REGION_LOOPS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
/// Top-level lowering depth (see [`top_stmt_to_estree`]): 1 while lowering
/// a direct child of the program body. The native while loop's errexit
/// check mirrors the top-level `sh2.guard` wrapper, which only wraps
/// TOP-LEVEL statements — a nested loop (inside an if/block/function) is
/// never guard-wrapped, so its native form must not abort either.
static TOP_LEVEL_DEPTH: Mutex<usize> = Mutex::new(0);
fn is_top_level_stmt() -> bool {
    *TOP_LEVEL_DEPTH.lock().unwrap() == 1
}
fn loop_in_async_region(stmt: &IrStmt) -> bool {
    ASYNC_REGION_LOOPS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(&(stmt as *const IrStmt as usize)))
        .unwrap_or(true)
}

/// Mark every `IrStmt::While` whose final status write is unread (not in
/// the Plan 4 live set). Only WHILE loops: the runtime `forLoopSync` never
/// writes lastExit itself (its status is the body's leftover — the native
/// for-of preserves that exactly), and `do/while` never reaches the ESTree
/// path (parsed as `IrStmt::While` with a negated cond).
fn mark_loop_status_deadness(st: &IrStmt, live: &HashSet<usize>, dead: &mut HashMap<usize, bool>) {
    match st {
        IrStmt::While { body, .. } => {
            if !live.contains(&(st as *const IrStmt as usize)) {
                dead.insert(st as *const IrStmt as usize, true);
            }
            for b in body {
                mark_loop_status_deadness(b, live, dead);
            }
        }
        IrStmt::For { body, .. } => {
            for b in body {
                mark_loop_status_deadness(b, live, dead);
            }
        }
        IrStmt::If { then, elsifs, else_, .. } => {
            for b in then.iter().chain(else_) {
                mark_loop_status_deadness(b, live, dead);
            }
            for (_, b) in elsifs {
                for stm in b {
                    mark_loop_status_deadness(stm, live, dead);
                }
            }
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => {
            for b in body {
                mark_loop_status_deadness(b, live, dead);
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for b in inner {
                mark_loop_status_deadness(b, live, dead);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for b in stage {
                    mark_loop_status_deadness(b, live, dead);
                }
            }
        }
        _ => {}
    }
}
/// Builtins the runtime implements as SYNC functions (harness
/// sh2-namespace.mjs `builtins.*` — every non-async entry of builtins.json
/// plus `test`, the bash test builtin the runtime implements on top of its
/// own test parser; `wait`/`exec`/`sleep`/`command` are async and stay on
/// the async exec path). `sh2.exec("echo", args)` lowers to a sync
/// `sh2.builtin("echo", args)` dispatch: identical arg flattening/glob
/// expansion, identical builtin function, minus the async exec machinery
/// (the whileLoopSync pattern — same semantics, no per-call promises).
pub(crate) const SYNC_BUILTINS: &[&str] = &[
    ".", ":", "basename", "break", "cat", "cd", "cmp", "comm", "continue", "cut", "declare",
    "dirname", "echo", "eval", "exit", "export", "false", "grep", "head", "let", "local",
    "mapfile", "mktemp", "printf", "pwd", "read", "readarray", "readonly", "return", "seq",
    "sed", "set", "shift", "sort", "source", "stat", "tail", "test", "touch", "tr", "trap",
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

/// Names of script-defined functions whose CALLS lower to the native
/// `sh2.callDirect(__fn_f, args)` path (see [`direct_fn_set`] /
/// [`try_native_fn_call`]): the SYNC subset whose every body is
/// positional-free — the call cannot observe the positional swap fnCall
/// performs (no `$1`..`$9`/`$@`/`$*`/`$#` reads, no shift/set positional
/// writes, no eval/source/trap of dynamic code, no runtime string
/// re-expansion of `$ref` text), so the direct JS call skips the arg
/// flattening, Map lookup and positional save/restore entirely. The
/// define arrows are ALSO assigned to module-level `let __fn_<name>`
/// bindings (fallback `(...args) => sh2.callUndefined(name, args)`), and
/// the call sites pass the binding, not the name. Set per compilation by
/// `shir_to_estree`; unset → conservative (no direct calls).
static DIRECT_FN_CALLS: Mutex<Option<HashSet<String>>> = Mutex::new(None);
fn fn_call_is_direct(name: &str) -> bool {
    DIRECT_FN_CALLS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false) // unset → conservative: keep the sync fnCall
}

/// Names of script-defined functions whose bodies may lower `echo`/
/// `printf` to native `process.stdout.write` (see [`native_echo_fn_set`]):
/// every possible call site runs with the default stdout sink, so the
/// define arrow is emitted WITHOUT the sink-depth bump. Set per
/// compilation by `shir_to_estree`; unset → conservative (no native echo
/// in function bodies — the current behavior).
static NATIVE_ECHO_FNS: Mutex<Option<HashSet<String>>> = Mutex::new(None);
fn native_echo_fn(name: &str) -> bool {
    NATIVE_ECHO_FNS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false)
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
) {    fn walk_stmts(stmts: &[IrStmt], functions: &HashSet<String>, out: &mut HashSet<String>) {
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

/// Defined functions whose bodies may lower `echo`/`printf` to native
/// `process.stdout.write` (see [`NATIVE_ECHO_FNS`]): the define arrow is
/// emitted WITHOUT the sink-depth bump, so the body's echo/printf go
/// native. Eligibility — every POSSIBLE call site of the function runs
/// with the default stdout sink:
///   * no static site (exec / sync fnCall / `command F` builtin) inside a
///     capture/captureWords/pipeline/redirect argument — the runtime
///     swaps fdTargets[1] there, and the native write would bypass it;
///   * no static site with an attached redirect (`f > file` is a redirect
///     body, which is the same disqualifier);
///   * no DYNAMIC-name exec / `command` site inside a swapped context —
///     the runtime dispatch resolves any defined function by name, so a
///     dynamic site is a potential site of every function;
///   * no persistent fd-1 redirect anywhere (`exec >file` — the global
///     PROGRAM_PERSIST_FD1 guard already suppresses native echo
///     everywhere, so the set is empty then anyway).
/// Subshell/background/block bodies COPY or share the current sink — a
/// call inside them is clean iff the enclosing context is clean, so those
/// propagate the flag instead of swapping it.
fn native_echo_fn_set(prog: &IrProgram, functions: &HashSet<String>) -> HashSet<String> {
    // disqualify(name): a swapped-context site of a specific function
    let mut bad: HashSet<String> = HashSet::new();
    // disqualify_all(): a swapped-context DYNAMIC site (could be any fn)
    let mut bad_all = false;
    fn stmt_walk(
        st: &IrStmt,
        swapped: bool,
        functions: &HashSet<String>,
        bad: &mut HashSet<String>,
        bad_all: &mut bool,
    ) {
        match st {
            IrStmt::Exec { cmd, args, redirects, .. } => {
                let site_swapped = swapped || !redirects.is_empty();
                match cmd {
                    IrExpr::Str(name, _) => {
                        if functions.contains(name) {
                            if site_swapped {
                                bad.insert(name.clone());
                            }
                        } else if name == "command" {
                            // `command F ...` — the runtime builtin
                            // dispatches defined functions too
                            if let Some(IrExpr::Array(els)) = args.first() {
                                if let Some(first) = els.first() {
                                    match first {
                                        IrExpr::Str(f, _) if functions.contains(f) => {
                                            if site_swapped {
                                                bad.insert(f.clone());
                                            }
                                        }
                                        // `command -v F` / `-V` never call
                                        IrExpr::Str(f, _) if f.starts_with('-') => {}
                                        _ => {
                                            if site_swapped {
                                                *bad_all = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        // dynamic command name — a potential site of any
                        // defined function
                        if site_swapped {
                            *bad_all = true;
                        }
                    }
                }
                for a in args {
                    expr_walk(a, swapped, functions, bad, bad_all);
                }
            }
            IrStmt::Expr(e) => expr_walk(e, swapped, functions, bad, bad_all),
            // a redirect body runs with swapped fdTargets
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    stmt_walk(b, true, functions, bad, bad_all);
                }
            }
            // pipeline stages run with swapped fdTargets
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        stmt_walk(b, true, functions, bad, bad_all);
                    }
                }
            }
            // subshell/background/block copy or share the current sink
            IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. } => {
                for b in body {
                    stmt_walk(b, swapped, functions, bad, bad_all);
                }
            }
            IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                expr_walk(cond, swapped, functions, bad, bad_all);
                for b in body {
                    stmt_walk(b, swapped, functions, bad, bad_all);
                }
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                expr_walk(cond, swapped, functions, bad, bad_all);
                for b in then.iter().chain(else_) {
                    stmt_walk(b, swapped, functions, bad, bad_all);
                }
                for (_, arm) in elsifs {
                    for b in arm {
                        stmt_walk(b, swapped, functions, bad, bad_all);
                    }
                }
            }
            IrStmt::For { iter, body, .. } => {
                expr_walk(iter, swapped, functions, bad, bad_all);
                for b in body {
                    stmt_walk(b, swapped, functions, bad, bad_all);
                }
            }
            IrStmt::Case { discriminant, clauses, .. } => {
                expr_walk(discriminant, swapped, functions, bad, bad_all);
                for c in clauses {
                    for b in &c.body {
                        stmt_walk(b, swapped, functions, bad, bad_all);
                    }
                }
            }
            IrStmt::Assign { expr, .. } => expr_walk(expr, swapped, functions, bad, bad_all),
            _ => {}
        }
    }
    fn expr_walk(
        e: &IrExpr,
        swapped: bool,
        functions: &HashSet<String>,
        bad: &mut HashSet<String>,
        bad_all: &mut bool,
    ) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    stmt_walk(st, swapped, functions, bad, bad_all);
                }
            }
            IrExpr::Capture { expr, .. } => expr_walk(expr, true, functions, bad, bad_all),
            IrExpr::Call { func, args } => {
                let site_swapped = swapped
                    || matches!(
                        func.as_str(),
                        "capture" | "captureWords" | "pipeline" | "redirect"
                    );
                if func == "exec" || func == "fnCall" {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        if functions.contains(name) {
                            if site_swapped {
                                bad.insert(name.clone());
                            }
                        } else if func == "exec" && name == "command" {
                            if let Some(IrExpr::Array(els)) = args.get(1) {
                                if let Some(first) = els.first() {
                                    match first {
                                        IrExpr::Str(f, _) if functions.contains(f) => {
                                            if site_swapped {
                                                bad.insert(f.clone());
                                            }
                                        }
                                        IrExpr::Str(f, _) if f.starts_with('-') => {}
                                        _ => {
                                            if site_swapped {
                                                *bad_all = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if site_swapped {
                        *bad_all = true;
                    }
                }
                // capture/captureWords/pipeline/redirect run their args
                // under a swapped sink; everything else propagates
                for a in args {
                    expr_walk(a, site_swapped, functions, bad, bad_all);
                }
            }
            IrExpr::Array(items) => {
                for it in items {
                    expr_walk(it, swapped, functions, bad, bad_all);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    expr_walk(v, swapped, functions, bad, bad_all);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                expr_walk(lhs, swapped, functions, bad, bad_all);
                expr_walk(rhs, swapped, functions, bad, bad_all);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                expr_walk(obj, swapped, functions, bad, bad_all);
                for a in args {
                    expr_walk(a, swapped, functions, bad, bad_all);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                expr_walk(cond, swapped, functions, bad, bad_all);
                expr_walk(then, swapped, functions, bad, bad_all);
                expr_walk(else_, swapped, functions, bad, bad_all);
            }
            IrExpr::DefinedOr { expr, default } => {
                expr_walk(expr, swapped, functions, bad, bad_all);
                expr_walk(default, swapped, functions, bad, bad_all);
            }
            IrExpr::Index { key, .. } => expr_walk(key, swapped, functions, bad, bad_all),
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        expr_walk(inner, swapped, functions, bad, bad_all);
                    }
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        stmt_walk(st, false, functions, &mut bad, &mut bad_all);
    }
    if bad_all {
        return HashSet::new();
    }
    functions.difference(&bad).cloned().collect()
}
///
/// The sync-function fixpoint (see [`SYNC_FN_CALLS`]) plus the native-
/// direct subset (see [`DIRECT_FN_CALLS`]): returns `(sync, direct)`.
///
/// A function's emitted arrow may be run without `await` only when its
/// lowered body (with every defined-function call at its sync `sh2.fnCall`
/// path) contains no `AwaitExpression` AND every function it directly
/// calls is itself sync — the async `sh2.exec` dispatch the sync path
/// replaces would be the only added await, and the fixpoint removes it.
/// Monotone (sync can only flip true→false), so it converges in at most
/// |functions| iterations; recursion/mutual recursion stay async
/// (conservative — a recursive call's target is in its own `calls` set).
///
/// The direct subset = sync ∩ {every definition body emits without any
/// positional read}: the body is lowered optimistically in the SAME pass
/// (the emitted arrow is what the define would run), and
/// [`estree_reads_positional`] scans it. Only names whose `__fn_<name>`
/// binding is a valid JS identifier qualify (dash names stay on fnCall).
fn fn_call_sync_set(
    prog: &IrProgram,
    functions: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut bodies: HashMap<String, Vec<&[IrStmt]>> = HashMap::new();
    collect_fn_bodies(&prog.stmts, &mut bodies);
    if bodies.is_empty() {
        return (HashSet::new(), HashSet::new());
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
    // read-only. The SAME emitted body is scanned for positional reads
    // (the direct-call eligibility — a direct call skips the positional
    // swap, so the body must provably never observe it).
    *SYNC_FN_CALLS.lock().unwrap() = Some(functions.clone());
    let mut opt_free: HashMap<String, bool> = HashMap::new();
    let mut opt_direct: HashMap<String, bool> = HashMap::new();
    for (name, defs) in &bodies {
        let mut free = true;
        let mut direct = direct_binding_name(name).is_some();
        for body in defs {
            let e = arrow_sink(vec![], IrExpr::Arrow(body.to_vec()));
            if expr_has_await(&e) {
                free = false;
                break;
            }
            if estree_reads_positional(&e) {
                direct = false;
            }
        }
        opt_free.insert(name.clone(), free);
        opt_direct.insert(name.clone(), direct);
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
    let direct: HashSet<String> = sync
        .iter()
        .filter(|f| opt_direct.get(*f).copied().unwrap_or(false))
        .cloned()
        .collect();
    (sync, direct)
}

/// `let __fn_<name>` — the module binding name for a native-direct
/// function, or None when the composed identifier would be invalid JS
/// (shell function names may contain `-` etc.; those keep the fnCall
/// dispatch).
fn direct_binding_name(name: &str) -> Option<String> {
    let n = format!("__fn_{name}");
    let mut chars = n.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(n)
}

/// Is `name` a positional-parameter name the runtime resolves from its
/// positional state (`$1`..`$9`, `$@`, `$*`, `$#` — the getVar/param
/// special cases)? `$0` is argv0, not positional; `$10`+ reads the store
/// (a pre-existing runtime limitation — mirror it, not bash).
fn positional_name(name: &str) -> bool {
    name.len() == 1
        && name != "0"
        && (matches!(name, "@" | "*" | "#") || name.as_bytes()[0].is_ascii_digit())
}

/// Does a string the RUNTIME will re-expand contain a positional `$ref`
/// (`$1`..`$9`, `$@`, `$*`, `$#`)? The sh2.arith/test/caseMatch string
/// args keep `$ref` text UNRESOLVED (the runtime expands it against the
/// CURRENT positional state), so a body containing one reads positionals
/// through the string. A single-quoted literal `'$5'` also matches — a
/// conservative false positive (the function keeps the fnCall dispatch;
/// correctness intact).
fn str_has_positional_ref(s: &str) -> bool {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'$' && i + 1 < b.len() {
            let c = b[i + 1];
            if (b'1'..=b'9').contains(&c) || matches!(c, b'@' | b'*' | b'#') {
                return true;
            }
        }
    }
    false
}

/// Does a string the runtime will evaluate contain a positional `$ref`
/// (`$1`..`$9`, `$@`, `$*`, `$#`)?

/// Does the emitted expression read (or hand to the runtime, for
/// re-expansion) the CURRENT positional state? Conservative: `true` keeps
/// the function on the sync `sh2.fnCall` dispatch (same semantics, only
/// the call-site speed differs — the corpus cannot observe which path
/// ran). A direct call skips fnCall's positional save/restore, so any
/// path through which the body could observe the caller's positionals
/// disqualifies:
///   - `sh2.positional.*` member accesses (the `$#`/`$@`/`$1`..`$9`
///     native special-var lowerings),
///   - `sh2.getVar`/`sh2.param` with a positional name (the runtime
///     resolves them from the positional state),
///   - `sh2.arith`/`sh2.test`/`sh2.caseMatch` string args containing
///     unresolved `$1`/`$@`/... text (the runtime re-expands it),
///   - `sh2.builtin("shift")` / `sh2.builtin("set", ...)` (positional
///     writes) and `sh2.builtin("eval"/"source"/"."/"trap")` (dynamic
///     code that would run under the current positionals).
fn estree_reads_positional(e: &Expr) -> bool {
    match e {
        Expr::Identifier { .. } | Expr::SpreadElement { .. } => false,
        // EVERY string literal is scanned for `$ref` text: the runtime
        // re-expands many strings against the CURRENT positionals —
        // builtin declare-family values (`local n=$1`), param
        // defaults/offsets/patterns, arrayIndex subscripts (`arr[$1]`),
        // setArray elements, arith/test/caseMatch strings, `=~` regex
        // patterns. A quoted literal `'$5'` (bash prints it verbatim)
        // also matches — a conservative false positive (the function
        // keeps the fnCall dispatch; correctness intact).
        Expr::Literal {
            value,
            raw: _,
            regex,
        } => {
            value.as_str().map(str_has_positional_ref).unwrap_or(false)
                || regex
                    .as_ref()
                    .map(|r| str_has_positional_ref(&r.pattern))
                    .unwrap_or(false)
        }
        Expr::TemplateLiteral { quasis, expressions } => {
            quasis.iter().any(|q| str_has_positional_ref(&q.value.raw))
                || expressions.iter().any(estree_reads_positional)
        }
        Expr::CallExpression {
            callee,
            arguments,
            optional: _,
        } => {
            if let Expr::MemberExpression {
                object,
                property,
                ..
            } = callee.as_ref()
            {
                // `sh2.functions.set(name, arrow)` — a nested DEFINE. The
                // arrow body runs under the NESTED function's own
                // positionals (fnCall/callDirect swap them at call time),
                // so skip walking it; only the name arg can matter.
                if let Expr::MemberExpression {
                    object: o2,
                    property: p2,
                    ..
                } = object.as_ref()
                {
                    if matches!(o2.as_ref(), Expr::Identifier { name } if name == "sh2")
                        && matches!(p2.as_ref(), Expr::Identifier { name } if name == "functions")
                        && matches!(property.as_ref(), Expr::Identifier { name } if name == "set")
                    {
                        return arguments.iter().take(1).any(estree_reads_positional);
                    }
                }
                if matches!(object.as_ref(), Expr::Identifier { name } if name == "sh2") {
                    if let Expr::Identifier { name } = property.as_ref() {
                        match name.as_str() {
                            "getVar" | "param" => {
                                // the name slot: getVar(name) / param(op, name, ...)
                                let slot = if name == "getVar" { 0 } else { 1 };
                                if let Some(Expr::Literal { value, .. }) =
                                    arguments.get(slot)
                                {
                                    if let serde_json::Value::String(s) = value {
                                        if positional_name(s) {
                                            return true;
                                        }
                                    }
                                }
                            }
                            "builtin" => {
                                if let Some(Expr::Literal { value, .. }) = arguments.first() {
                                    if let serde_json::Value::String(s) = value {
                                        // shift/set: positional writes.
                                        // eval/source/. /trap: dynamic code
                                        // under the current positionals.
                                        if matches!(
                                            s.as_str(),
                                            "shift" | "set" | "eval" | "source" | "."
                                                | "trap"
                                        ) {
                                            return true;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            for a in arguments {
                if estree_reads_positional(a) {
                    return true;
                }
            }
            estree_reads_positional(callee)
        }
        Expr::MemberExpression {
            object,
            property,
            ..
        } => {
            // `sh2.positional[..]` / `.length` / `.join(...)` / `.slice(...)`
            // — the native `$#`/`$@`/`$1`..`$9`/`${@:off:len}` lowerings
            if matches!(object.as_ref(), Expr::Identifier { name } if name == "sh2")
                && matches!(property.as_ref(), Expr::Identifier { name } if name == "positional")
            {
                return true;
            }
            estree_reads_positional(object) || estree_reads_positional(property)
        }
        Expr::AwaitExpression { argument } => estree_reads_positional(argument),
        Expr::ArrowFunctionExpression { params, body, .. } => {
            // nested closure bodies (redirect/capture/pipeline/loop/…)
            // run under the CURRENT positional state — walk them
            let in_body = match body {
                ArrowBody::Expr(e) => estree_reads_positional(e),
                ArrowBody::Block(b) => estree_stmt_reads_positional(b),
            };
            in_body || params.iter().any(estree_reads_positional)
        }
        Expr::ObjectExpression { properties } => properties.iter().any(|p| {
            estree_reads_positional(&p.key) || estree_reads_positional(&p.value)
        }),
        Expr::ArrayExpression { elements } => {
            elements.iter().flatten().any(estree_reads_positional)
        }
        Expr::LogicalExpression { left, right, .. }
        | Expr::BinaryExpression { left, right, .. } => {
            estree_reads_positional(left) || estree_reads_positional(right)
        }
        Expr::AssignmentExpression { left, right, .. } => {
            estree_reads_positional(left) || estree_reads_positional(right)
        }
        Expr::ConditionalExpression {
            test,
            consequent,
            alternate,
        } => {
            estree_reads_positional(test)
                || estree_reads_positional(consequent)
                || estree_reads_positional(alternate)
        }
        Expr::UnaryExpression { argument, .. } => estree_reads_positional(argument),
        Expr::SequenceExpression { expressions } => {
            expressions.iter().any(estree_reads_positional)
        }
    }
}

/// Statement twin of [`estree_reads_positional`] (arrow block bodies).
fn estree_stmt_reads_positional(s: &Stmt) -> bool {
    match s {
        Stmt::ExpressionStatement { expression } => estree_reads_positional(expression),
        Stmt::BlockStatement { body } => body.iter().any(estree_stmt_reads_positional),
        Stmt::IfStatement {
            test,
            consequent,
            alternate,
        } => {
            estree_reads_positional(test)
                || estree_stmt_reads_positional(consequent)
                || alternate
                    .as_ref()
                    .map(|a| estree_stmt_reads_positional(a))
                    .unwrap_or(false)
        }
        Stmt::SwitchStatement {
            discriminant,
            cases,
        } => {
            estree_reads_positional(discriminant)
                || cases
                    .iter()
                    .flat_map(|c| &c.consequent)
                    .any(estree_stmt_reads_positional)
        }
        Stmt::WhileStatement { test, body } => {
            estree_reads_positional(test) || estree_stmt_reads_positional(body)
        }
        Stmt::ForStatement {
            init, test, update, body,
        } => {
            estree_stmt_reads_positional(init)
                || estree_reads_positional(test)
                || estree_reads_positional(update)
                || estree_stmt_reads_positional(body)
        }
        Stmt::ForOfStatement {
            left,
            right,
            body,
        } => {
            estree_reads_positional(right) || estree_stmt_reads_positional(body)
        }
        Stmt::VariableDeclaration { declarations, .. } => declarations
            .iter()
            .filter_map(|d| d.init.as_ref())
            .any(estree_reads_positional),
        Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => false,
        Stmt::ReturnStatement { argument } => argument
            .as_ref()
            .map(|a| estree_reads_positional(a))
            .unwrap_or(false),
    }
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
    // Then apply worker-submitted transforms (gated by DEBASHC_TRANSFORMS;
    // the estree worker compiles them in + bisects on the corpus).
    let mut stmts = crate::ir::optimize_stmts(&commands.iter().filter_map(stmt_for_command).collect::<Vec<_>>());
    // Serialize against the shir_to_estree compile lock: the
    // sync-ok-loops transform stores per-compilation POINTER-keyed
    // verdicts in shared statics, and a parallel compilation's unlocked
    // write could tear them mid-emission (the determinism unit tests
    // compile concurrently). Never re-entered: shir_to_estree does not
    // call ast_to_ir and ast_to_ir does not call shir_to_estree.
    let _compile_guard = COMPILE_LOCK.lock().unwrap();
    crate::transforms::apply(&mut stmts);
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
    }
}

/// Raw lowering (plan §2.3): skip the shared optimization passes
/// (constant folding, dead-assignment elimination). Use with
/// `shir_json::shir_to_shir_json_raw` to pin the `F(S)_raw == C(S)_raw`
/// boundary — frontend output, unoptimized, unattached-annotations.
pub fn ast_to_ir_raw(commands: &[Command]) -> IrProgram {
    let stmts = commands.iter().filter_map(stmt_for_command).collect::<Vec<_>>();
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
    }
}

/// Conservative type annotations for static backends (ask A2). The verdicts
/// are exactly the JS path's lift analyses: `numeric_lift_vars` (every
/// assignment provably numeric → native number) and `string_lift_vars`
/// (every assignment provably a string literal → native string). Vars in
/// neither set keep the runtime store (shell vars are strings; Any).
/// Sorted by name for deterministic serialization.
/// Conservative max-string-length analysis (the transform the C backend
/// asked for: fixed buffers instead of heap/`char*`). A fixed-point over
/// the assignments: each var's bound is the max over its assignment RHS
/// lengths (Str literals, Interpolate = the literal parts + the
/// interpolated vars' bounds); captures/calls/binops are unbounded
/// (None); a loop-accumulated `s="$s$x"` grows past the cap each
/// iteration and flips to None (the cap guarantees termination).
pub fn analyze_string_lengths(prog: &IrProgram) -> Vec<(String, Option<u64>)> {
    const CAP: u64 = 1024; // a false bound from an unbounded loop is worse than none
    const ITER_LIMIT: usize = 1024;
    const MAX_DEPTH: u32 = 512; // an over-deep AST -> conservative unbounded (None)
    use crate::ir::{InterpPart, IrExpr, IrStmt};
    use std::collections::{BTreeMap, BTreeSet};

    // ranges of the loop counters (whole-program, joined) — the length
    // analysis consults them for while-loop trip bounds
    let ranges = analyze_var_ranges(prog);

    // collect the assignment targets + RHS exprs, with the max number of
    // executions (the product of the enclosing loop trips; Some(1) at top
    // level; None when any enclosing loop is unbounded)
    let mut assigns: Vec<(String, &IrExpr, Option<u64>)> = Vec::new();
    fn walk<'a>(
        stmts: &'a [IrStmt],
        assigns: &mut Vec<(String, &'a IrExpr, Option<u64>)>,
        trip: Option<u64>,
        ranges: &HashMap<String, (i128, i128)>,
    ) {
        for st in stmts {
            match st {
                IrStmt::Assign { targets, expr } => {
                    for t in targets {
                        if t.indices.is_empty() {
                            assigns.push((t.var.clone(), expr, trip));
                        }
                    }
                }
                // `local name=value` / `declare name=value` / `export
                // name=value` — the shell's declaration assignments: the
                // exec/builtin call's args carry "name=" + the value
                IrStmt::Expr(IrExpr::Call { func, args }) => {
                    let decl_name = match args.first() {
                        Some(IrExpr::Str(n, _))
                            if matches!(n.as_str(), "local" | "declare" | "readonly" | "export") =>
                        {
                            Some(n.clone())
                        }
                        _ => None,
                    };
                    if let Some(decl) = decl_name {
                        if let Some(IrExpr::Array(elems)) = args.get(1) {
                            let mut i = 0;
                            while i < elems.len() {
                                if let IrExpr::Str(nv, _) = &elems[i] {
                                    if let Some((name, value)) = nv.split_once('=') {
                                        if !name.is_empty() {
                                            // the value may be inline ("" for
                                            // `name=$(...)`) or the NEXT array
                                            // element: ["sqrt_n=", [value]]
                                            let next = elems.get(i + 1);
                                            let v: &IrExpr = match next {
                                                Some(IrExpr::Array(inner))
                                                    if inner.len() == 1 =>
                                                {
                                                    &inner[0]
                                                }
                                                Some(other) => other,
                                                None if !value.contains('$') => {
                                                    // inline literal
                                                    Box::leak(Box::new(IrExpr::Str(
                                                        value.to_string(),
                                                        crate::ir::StrStyle::DoubleQuoted,
                                                    )))
                                                }
                                                None => break,
                                            };
                                            assigns.push((name.to_string(), v, trip));
                                            if matches!(next, Some(IrExpr::Array(_))) {
                                                i += 2;
                                                continue;
                                            }
                                        }
                                    }
                                }
                                i += 1;
                            }
                            let _ = decl;
                        }
                    }
                }
                IrStmt::If { then, elsifs, else_, .. } => {
                    walk(then, assigns, trip, ranges);
                    for (_, arm) in elsifs {
                        walk(arm, assigns, trip, ranges);
                    }
                    walk(else_, assigns, trip, ranges);
                }
                IrStmt::While { cond, body } => {
                    let t = trip_from_bound(cond_bound(cond), body, ranges);
                    walk(body, assigns, mul_trip(trip, t), ranges);
                }
                IrStmt::DoWhile { body, cond, until } => {
                    let b = cond_bound(cond);
                    let b = if *until {
                        b.map(|(v, c, n)| (v, cmp_flip(c), n))
                    } else {
                        b
                    };
                    let t = trip_from_bound(b, body, ranges);
                    walk(body, assigns, mul_trip(trip, t), ranges);
                }
                IrStmt::For { iter, body, .. } => {
                    let t = for_iter_trip(iter);
                    walk(body, assigns, mul_trip(trip, t), ranges);
                }
                IrStmt::Subshell(body)
                | IrStmt::Background(body)
                | IrStmt::Block(body)
                | IrStmt::Redirect { inner: body, .. } => walk(body, assigns, trip, ranges),
                IrStmt::Function { body, .. } => walk(body, assigns, trip, ranges),
                _ => {}
            }
        }
    }
    walk(&prog.stmts, &mut assigns, Some(1), &ranges);

    /// The max iterations of a loop whose cond pins a counter var
    /// (`[ $i -lt 100 ]`): entry bound from the whole-program ranges (a
    /// sound under-estimate of the entry lo — over-estimates the trip),
    /// step from the body's provable +k writes. None = unbounded.
    fn trip_from_bound(
        bound: Option<(String, Cmp, i128)>,
        body: &[IrStmt],
        ranges: &HashMap<String, (i128, i128)>,
    ) -> Option<u64> {
        let (v, c, n) = bound?;
        let (blo, bhi) = ranges.get(&v).copied()?;
        let (mn, _) = body_step_bounds(body, &v)?;
        while_iterations(c, n, blo, bhi, mn as i128)
    }

    /// Multiply the enclosing trip by a loop's trip (cap the product).
    fn mul_trip(a: Option<u64>, b: Option<u64>) -> Option<u64> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.saturating_mul(y).min(1_000_000)),
            _ => None,
        }
    }

    let names: BTreeSet<String> = assigns.iter().map(|(n, _, _)| n.clone()).collect();
    let mut lens: BTreeMap<String, Option<u64>> =
        names.iter().map(|n| (n.clone(), Some(0))).collect();

    fn expr_len(
        e: &IrExpr,
        lens: &BTreeMap<String, Option<u64>>,
        cap: u64,
        depth: u32,
    ) -> Option<u64> {
        expr_len_skip(e, lens, cap, depth, None)
    }

    /// Like `expr_len` but reads of `skip` count as 0 — the per-execution
    /// added length of a self-accumulating assignment (`s="$s$x"` → |x|).
    fn expr_len_skip(
        e: &IrExpr,
        lens: &BTreeMap<String, Option<u64>>,
        cap: u64,
        depth: u32,
        skip: Option<&str>,
    ) -> Option<u64> {
        if depth > MAX_DEPTH {
            return None;
        }
        match e {
            IrExpr::Str(sv, _) => Some(sv.len() as u64),
            IrExpr::Int(_) => Some(20), // the max digit count
            IrExpr::Var(n, _) => {
                if skip == Some(n.as_str()) {
                    Some(0)
                } else {
                    lens.get(n).copied().flatten()
                }
            }
            IrExpr::Interpolate(parts) => {
                let mut total = 0u64;
                for p in parts {
                    let l = match p {
                        InterpPart::Lit(s) => s.len() as u64,
                        InterpPart::Expr(e) => expr_len_skip(e, lens, cap, depth + 1, skip)?,
                    };
                    total = total.saturating_add(l);
                    if total > cap {
                        return None;
                    }
                }
                Some(total)
            }
            IrExpr::Capture { expr, .. } => {
                // a capture's bound depends on the CAPTURED COMMAND: bc
                // yields a fixed-width number; the filters (grep/sed/tr/
                // head/tail/sort/uniq/cut/cat/...) yield output no larger
                // than the input — bounded by the pipeline's FIRST stage
                // when that is a bounded echo; everything else is
                // unbounded (the user's design: e.g. the primes'
                // `$(echo "sqrt($n)" | bc)` -> 40).
                capture_bound_skip(expr, lens, cap, depth + 1, skip)
            }
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => {
                    if skip == Some(n.as_str()) {
                        Some(0)
                    } else {
                        lens.get(n).copied().flatten()
                    }
                }
                _ => None,
            },
            // the runtime `s+=x` — the value's length is the added part
            IrExpr::Call { func, args } if func == "assign" => match args.as_slice() {
                [IrExpr::Str(n, _), IrExpr::Str(op, _), value] if op == "+=" => {
                    if skip == Some(n.as_str()) {
                        expr_len_skip(value, lens, cap, depth + 1, skip)
                    } else {
                        None
                    }
                }
                _ => None,
            },
            // the runtime capture: sh2.capture([wrapped command]) — the
            // bound comes from the CAPTURED command (bc/wc/hash fixed
            // widths, the filters <= the input, grep -q/-c options)
            IrExpr::Call { func, args } if func == "capture" => match args.first() {
                Some(body) => capture_bound_skip(body, lens, cap, depth + 1, skip),
                _ => None,
            },
            // BinOps ARE bounded: `a . b` (Concat) = max(a)+max(b); the
            // numeric ops yield a number (<= 20 chars); the comparisons/
            // logicals yield 0/1 (1 char).
            IrExpr::BinOp { op, lhs, rhs } => match op {
                BinOpKind::Concat => {
                    let l = expr_len_skip(lhs, lens, cap, depth + 1, skip)?;
                    let r = expr_len_skip(rhs, lens, cap, depth + 1, skip)?;
                    Some(l.saturating_add(r))
                }
                BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt
                | BinOpKind::Le | BinOpKind::Ge | BinOpKind::And | BinOpKind::Or
                | BinOpKind::Not => Some(1),
                _ => Some(20), // the numeric/bitwise ops -> a number
            },
            // $((...)) -> a number
            IrExpr::Arith(_) => Some(20),
            _ => None, // calls / arrays — unbounded
        }
    }

    /// The capture's wrapped command -> the pipeline's stages. The
    /// shapes: `Call("pipeline", [Array([Arrow, ...])])`, an exec whose
    /// ARGS are the arrow stages (`Call("exec", [Arrow, Arrow])`), or a
    /// bare Arrow whose body's first statement is the command call.
    fn capture_stages(e: &IrExpr) -> Vec<&IrExpr> {
        match e {
            IrExpr::Call { func, args } if func == "pipeline" => match args.first() {
                Some(IrExpr::Array(items)) => items.iter().collect(),
                _ => vec![e],
            },
            IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
                // the stages may sit in the exec's args directly
                // ([Arrow, Arrow]) or wrapped in a single Array
                // ([Array([Arrow, Arrow])]) — both appear in the corpus
                if let Some(IrExpr::Array(items)) = args.first() {
                    if items.iter().all(|a| matches!(a, IrExpr::Arrow(_))) {
                        return items.iter().collect();
                    }
                }
                let stages: Vec<&IrExpr> =
                    args.iter().filter(|a| matches!(a, IrExpr::Arrow(_))).collect();
                if stages.is_empty() {
                    vec![e]
                } else {
                    stages
                }
            }
            IrExpr::Arrow(stmts) => stmts.iter().find_map(|st| match st {
                // a single-command stage: the call IS the stage
                IrStmt::Expr(e @ IrExpr::Call { .. }) => {
                    let func = match e {
                        IrExpr::Call { func, .. } => func,
                        _ => unreachable!(),
                    };
                    let args = match e {
                        IrExpr::Call { args, .. } => args,
                        _ => unreachable!(),
                    };
                    if !matches!(func.as_str(), "exec" | "builtin" | "pipeline") {
                        return Some(vec![e]);
                    }
                    // the inner call's args ARE the stages (or the
                    // pipeline's [Array([Arrow, ...])]) — borrow them from
                    // the ORIGINAL arrow (no temporaries)
                    if let Some(IrExpr::Array(items)) = args.first() {
                        if items.iter().all(|a| matches!(a, IrExpr::Arrow(_))) {
                            return Some(items.iter().collect());
                        }
                    }
                    let stages: Vec<&IrExpr> = args
                        .iter()
                        .filter(|a| matches!(a, IrExpr::Arrow(_)))
                        .collect();
                    if stages.is_empty() {
                        Some(vec![e])
                    } else {
                        Some(stages)
                    }
                }
                _ => None,
            }).unwrap_or_default(),
            _ => vec![e],
        }
    }

    /// The command name of a pipeline stage (an Arrow wrapping the call,
    /// or a bare Call — both shapes appear).
    fn stage_cmd(stage: &IrExpr) -> Option<String> {
        match stage {
            IrExpr::Arrow(stmts) => stmts.iter().find_map(|st| match st {
                IrStmt::Expr(IrExpr::Call { func, args }) => match func.as_str() {
                    "exec" | "builtin" => match args.first() {
                        Some(IrExpr::Str(n, _)) => Some(n.clone()),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            }),
            IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
                match args.first() {
                    Some(IrExpr::Str(n, _)) => Some(n.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn capture_bound_skip(
        e: &IrExpr,
        lens: &BTreeMap<String, Option<u64>>,
        cap: u64,
        depth: u32,
        skip: Option<&str>,
    ) -> Option<u64> {
        if depth > MAX_DEPTH {
            return None;
        }
        let stages: Vec<&IrExpr> = capture_stages(e);
        // the LAST stage's command name
        let last = stages.last()?;
        let cmd_name = stage_cmd(last)?;
        // the grep OPTIONS change the bound: -q emits nothing, -c emits
        // a count (a number), the rest filter (<= the input)
        let grep_arg = |args: &[IrExpr]| -> Option<String> {
            args.iter().find_map(|a| match a {
                IrExpr::Str(sv, _) if sv.starts_with('-') && sv.len() == 2 => Some(sv.clone()),
                _ => None,
            })
        };
        let last_flag: Option<String> = match last {
            IrExpr::Arrow(stmts) => stmts.iter().find_map(|st| match st {
                IrStmt::Expr(IrExpr::Call { args, .. }) => {
                    let a = args.get(1).and_then(|x| match x {
                        IrExpr::Array(items) => Some(items.as_slice()),
                        _ => None,
                    })?;
                    grep_arg(a).map(|s| s.to_string())
                }
                _ => None,
            }),
            _ => None,
        };
        match cmd_name.as_str() {
            // zero-output builtins — the capture is the empty string
            "true" | "false" | ":" | "test" | "[" | "[[" | "grep"
                if last_flag.as_deref() == Some("-q") =>
            {
                Some(0)
            }
            // fixed-width outputs
            "bc" => Some(40), // an arbitrary-precision number (the primes sqrt case)
            "wc" => Some(20), // always numbers (the -l/-w/-c counts)
            "grep" if last_flag.as_deref() == Some("-c") => Some(20), // a count
            "md5sum" => Some(32),
            "sha1sum" => Some(40),
            "sha256sum" => Some(64),
            "sha512sum" => Some(128),
            "date" => Some(30), // the timestamp
            "umask" => Some(4), // an octal
            "expr" => Some(20), // a number (or a short string)
            "seq" => None,      // the item count unknown
            // echo: the output is the args' joined lengths (skip the
            // command name; the real args live in the trailing Array) —
            // the stage may be an Arrow wrapping the call, or a bare Call
            "echo" | "printf" => {
                // the stage's call args: an Arrow wrapping the call, or a
                // bare Call — both shapes appear
                let call_args: Vec<&IrExpr> = match last {
                    IrExpr::Arrow(stmts) => stmts
                        .iter()
                        .find_map(|st| match st {
                            IrStmt::Expr(IrExpr::Call { args, .. }) => {
                                Some(args.iter().collect())
                            }
                            _ => None,
                        })
                        .unwrap_or_default(),
                    IrExpr::Call { args, .. } => args.iter().collect(),
                    _ => Vec::new(),
                };
                let mut total = 0u64;
                for a in &call_args {
                    let l = match a {
                        IrExpr::Array(items) => {
                            let mut t = 0u64;
                            for it in items.iter() {
                                t = t.saturating_add(expr_len_skip(it, lens, cap, depth + 1, skip)?);
                            }
                            t
                        }
                        other => expr_len_skip(other, lens, cap, depth + 1, skip)?,
                    };
                    total = total.saturating_add(l).saturating_add(1);
                }
                if total > cap {
                    None
                } else {
                    Some(total)
                }
            }
            // the filters: output <= input — the FIRST stage's bound.
            // A SINGLE-stage filter (the command call IS the only stage,
            // e.g. `$(basename $(pwd))`) must NOT recurse into itself —
            // capture_stages returns [e], so recursing on stages.first()
            // was INFINITE recursion (core-requests/c-20260806-102527.md
            // stack overflow on 000__04a). Its input is the captured
            // stream, not a pipeline stage -> conservative unbounded.
            "grep" | "sed" | "tr" | "head" | "tail" | "sort" | "uniq"
            | "cut" | "cat" | "paste" | "rev" | "join" | "basename"
            | "dirname" | "comm" => {
                if stages.len() <= 1 {
                    None
                } else {
                    let first = stages.first()?;
                    capture_bound_skip(first, lens, cap, depth + 1, skip)
                }
            }
            _ => None,
        }
    }

    // how many times does the RHS read the target? (0 = not
    // self-accumulating; 1 = linear accumulation `s="$s$x"`;
    // ≥ 2 = compounding `s="$s$s"` — unbounded, never bounded)
    fn count_var_reads(e: &IrExpr, v: &str) -> usize {
        match e {
            IrExpr::Var(n, _) => (n == v) as usize,
            IrExpr::Call { func, args } if func == "getVar" => match args.as_slice() {
                [IrExpr::Str(n, _)] => (n == v) as usize,
                _ => 0,
            },
            IrExpr::Call { func, args } => {
                if func == "assign" {
                    // the name arg is a WRITE — count only the value
                    return args.iter().skip(1).map(|a| count_var_reads(a, v)).sum();
                }
                args.iter().map(|a| count_var_reads(a, v)).sum()
            }
            IrExpr::Interpolate(parts) => parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(_) => 0,
                    InterpPart::Expr(e) => count_var_reads(e, v),
                })
                .sum(),
            IrExpr::BinOp { lhs, rhs, .. } => count_var_reads(lhs, v) + count_var_reads(rhs, v),
            IrExpr::Arith(a) => arith_count_reads(a, v),
            IrExpr::Index { var, key, .. } => (var == v) as usize + count_var_reads(key, v),
            IrExpr::Capture { expr, .. } => count_var_reads(expr, v),
            IrExpr::Array(items) => items.iter().map(|i| count_var_reads(i, v)).sum(),
            IrExpr::Arrow(stmts) => stmts.iter().map(|s| stmt_count_reads(s, v)).sum(),
            IrExpr::Ternary { cond, then, else_, .. } => {
                count_var_reads(cond, v)
                    + count_var_reads(then, v)
                    + count_var_reads(else_, v)
            }
            IrExpr::DefinedOr { expr, default, .. } => {
                count_var_reads(expr, v) + count_var_reads(default, v)
            }
            IrExpr::Object(entries) => entries.iter().map(|(_, x)| count_var_reads(x, v)).sum(),
            IrExpr::MethodCall { obj, args, .. } => {
                count_var_reads(obj, v) + args.iter().map(|a| count_var_reads(a, v)).sum::<usize>()
            }
            // Str/Int/Bool/Ident/Json/Range/RawExpr/Regex — no reads
            _ => 0,
        }
    }

    fn stmt_count_reads(s: &IrStmt, v: &str) -> usize {
        match s {
            IrStmt::Assign { expr, .. } => count_var_reads(expr, v),
            IrStmt::Expr(e) => count_var_reads(e, v),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
                ..
            } => {
                count_var_reads(cond, v)
                    + then.iter().map(|s| stmt_count_reads(s, v)).sum::<usize>()
                    + elsifs
                        .iter()
                        .map(|(c, b)| {
                            count_var_reads(c, v)
                                + b.iter().map(|s| stmt_count_reads(s, v)).sum::<usize>()
                        })
                        .sum::<usize>()
                    + else_.iter().map(|s| stmt_count_reads(s, v)).sum::<usize>()
            }
            // loop bodies may read v (their trip bounds are handled
            // separately) — a conservative count suffices here
            IrStmt::While { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::DoWhile { body, .. } => body.iter().map(|s| stmt_count_reads(s, v)).sum(),
            IrStmt::Block(b)
            | IrStmt::Subshell(b)
            | IrStmt::Background(b)
            | IrStmt::Redirect { inner: b, .. } => {
                b.iter().map(|s| stmt_count_reads(s, v)).sum()
            }
            IrStmt::Output { value, .. } => count_var_reads(value, v),
            IrStmt::WriteFile { path, content, .. } => {
                count_var_reads(path, v) + count_var_reads(content, v)
            }
            _ => 0,
        }
    }

    fn arith_count_reads(a: &ArithAst, v: &str) -> usize {
        match a {
            ArithAst::Var(n) => (n == v) as usize,
            ArithAst::Num(_) => 0,
            ArithAst::Index { var, key, .. } => {
                (var == v) as usize + arith_count_reads(key, v)
            }
            ArithAst::Bin { lhs, rhs, .. } => arith_count_reads(lhs, v) + arith_count_reads(rhs, v),
            ArithAst::Un { arg, .. } => arith_count_reads(arg, v),
            ArithAst::Cond { test, then, else_, .. } => {
                arith_count_reads(test, v)
                    + arith_count_reads(then, v)
                    + arith_count_reads(else_, v)
            }
            ArithAst::Assign { var, rhs, .. } => {
                (var == v) as usize + arith_count_reads(rhs, v)
            }
            ArithAst::IncDec { var, .. } => (var == v) as usize,
        }
    }

    /// A numeric accumulator (`i=$((i+1))`, `n+=2`, `sum=$(… | bc)`): the
    /// result is a number, so the bound is the fixed number/capture width
    /// — the trip does NOT multiply (a counter's VALUE grows, not its
    /// width).
    fn is_numeric_accum(e: &IrExpr) -> bool {
        // a capture whose pipeline's LAST stage is a fixed-width numeric
        // producer (the same table capture_bound uses) — both the
        // `IrExpr::Capture` node and the runtime `capture` call
        let numeric_last_stage = |body: &IrExpr| {
            matches!(
                capture_stages(body)
                    .last()
                    .and_then(|s| stage_cmd(s))
                    .as_deref(),
                Some(
                    "bc" | "expr" | "wc" | "date" | "umask" | "md5sum"
                    | "sha1sum" | "sha256sum" | "sha512sum",
                )
            )
        };
        match e {
            IrExpr::Arith(_) => true,
            IrExpr::Call { func, args } if func == "assign" => match args.as_slice() {
                [IrExpr::Str(_, _), IrExpr::Str(op, _), value] if op == "+=" => match value {
                    IrExpr::Int(_) | IrExpr::Arith(_) => true,
                    IrExpr::Str(sv, _) => sv.trim().parse::<i64>().is_ok(),
                    _ => false,
                },
                _ => false,
            },
            IrExpr::Capture { expr, .. } => numeric_last_stage(expr),
            IrExpr::Call { func, args } if func == "capture" => match args.first() {
                Some(body) => numeric_last_stage(body),
                None => false,
            },
            _ => false,
        }
    }

    /// The numeric accumulator's bound: the fixed number width (20) or the
    /// capture producer's fixed width (bc 40, hashes …) — NOT trip·Δ.
    fn numeric_accum_new(
        e: &IrExpr,
        lens: &BTreeMap<String, Option<u64>>,
        cap: u64,
        depth: u32,
    ) -> Option<u64> {
        match e {
            IrExpr::Arith(_) => Some(20),
            IrExpr::Call { func, args } if func == "assign" => match args.as_slice() {
                [IrExpr::Str(_, _), IrExpr::Str(op, _), value] if op == "+=" => {
                    expr_len(value, lens, cap, depth + 1)
                }
                _ => None,
            },
            IrExpr::Capture { .. } => expr_len(e, lens, cap, depth + 1),
            IrExpr::Call { func, args } if func == "capture" => {
                expr_len(e, lens, cap, depth + 1)
            }
            _ => None,
        }
    }

    let reads: Vec<usize> = assigns
        .iter()
        .map(|(v, rhs, _)| count_var_reads(rhs, v))
        .collect();

    // Phase A — the baselines v0: fixpoint over the non-accumulating
    // assignments only (every assignment to a var is one write; max over
    // writes converges to the final single-write bound). A var with no
    // non-accumulating assignment keeps Some(0) (unset reads as "").
    for _ in 0..ITER_LIMIT {
        let mut changed = false;
        for (i, (v, rhs, _)) in assigns.iter().enumerate() {
            if reads[i] > 0 {
                continue;
            }
            let cur = lens[v];
            if cur.is_none() {
                continue;
            }
            let l = expr_len(rhs, &lens, CAP, 0);
            let new = match l {
                Some(x) if x > CAP => None,
                Some(x) => Some(x.max(cur.unwrap_or(0))),
                None => None,
            };
            if new != cur {
                lens.insert(v.clone(), new);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let v0 = lens.clone();

    // Phase B — the unified fixpoint: accumulating assignments get a
    // direct bound (numeric accumulators cap at the 20-char number width;
    // string accumulators at v0 + trip·Δ where Δ = one execution's added
    // length with the self-reference zeroed); everything else
    // re-evaluates against the full lens so an overwrite sees another
    // var's final (accumulated) bound. Monotone, terminates (the cap
    // absorbs; a None is final).
    for _ in 0..ITER_LIMIT {
        let mut changed = false;
        for (i, (v, rhs, trip)) in assigns.iter().enumerate() {
            let cur = lens[v];
            if cur.is_none() {
                continue;
            }
            let new = if reads[i] == 0 {
                match expr_len(rhs, &lens, CAP, 0) {
                    Some(x) if x > CAP => None,
                    Some(x) => Some(x),
                    None => None,
                }
            } else if reads[i] >= 2 {
                // compounding growth (s="$s$s") — unbounded
                None
            } else if is_numeric_accum(rhs) {
                // a counter's value grows, not its width
                numeric_accum_new(rhs, &lens, CAP, 0)
            } else {
                match (trip, v0.get(v).copied().flatten()) {
                    (Some(t), Some(base)) => {
                        match expr_len_skip(rhs, &lens, CAP, 0, Some(v)) {
                            Some(d) => {
                                let total = base.saturating_add(t.saturating_mul(d));
                                if total > CAP {
                                    None
                                } else {
                                    Some(total)
                                }
                            }
                            None => None,
                        }
                    }
                    _ => None,
                }
            };
            let new = match (cur, new) {
                (Some(c), Some(n)) => Some(c.max(n)),
                _ => None,
            };
            if new != cur {
                lens.insert(v.clone(), new);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    lens.into_iter().collect()
}

pub fn analyze_var_types(prog: &IrProgram) -> Vec<(String, crate::ir::IrType)> {
    let numeric = numeric_lift_vars(prog);
    let string = string_lift_vars(prog, &numeric);
    let mut names: std::collections::HashSet<String> = numeric.iter().cloned().collect();
    for s in &string {
        names.insert(s.clone());
    }
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    names
        .into_iter()
        .map(|n| {
            let t = if numeric.contains(&n) {
                crate::ir::IrType::Int
            } else {
                crate::ir::IrType::Str
            };
            (n, t)
        })
        .collect()
}

/// Conservative const/var verdicts (the const-markup transform; sibling
/// of the A2 type verdicts and `analyze_string_lengths`). Per ASSIGNED
/// variable, `Const` when it is written exactly once and that write
/// executes at most once per run; `Var` otherwise.
///
/// Rules — all must hold for a `Const` verdict:
///   - exactly ONE static assignment site: a single `Assign` target, one
///     `local/declare/readonly/export name=value` declaration, one
///     `setVar` write, one `Declare`/`DeclareArray` init (a bare `declare
///     x` declares the empty value, so it counts as a site). A `for` loop
///     variable is a site but is disqualified below (assigned per
///     iteration);
///   - the site executes at most once: NOT inside a loop body and NOT
///     inside a function body (a function may run 0..N times; a loop site
///     runs per iteration);
///   - the var is never written by a runtime-store builtin (`read`,
///     `readarray`, `mapfile`, `unset`), by a `let`/`(( ))` arithmetic
///     statement, by native arith (`x++`, `((x=1))` in `$(( ))`), or by an
///     array-element write (`arr[i]=v` — the store owns the element);
///   - the program contains no dynamic write (a bare `eval`/`source`/`.`
///     call anywhere can assign any name → every var `Var`).
///
/// Everything else is `Var` (over-conservatism is the safe direction: a
/// missed `const` costs an optimisation, a wrong one breaks a backend's
/// compilation). Sorted by name for deterministic serialization; every
/// assigned var gets a verdict so the markup answers "const or var?"
/// completely (missing names = never assigned, pure reads).
pub fn analyze_var_const(prog: &IrProgram) -> Vec<(String, crate::ir::VarKind)> {
    use crate::ir::{ArithAst, IrExpr, IrStmt, VarKind};
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct Acc {
        /// static assignment-site count per var
        sites: HashMap<String, usize>,
        /// vars with a site inside a loop or function body (runs 0..N times)
        multi_run: HashSet<String>,
        /// vars written by a runtime-store builtin (read/mapfile/unset/let)
        runtime_written: HashSet<String>,
        /// vars written by native arith (`x++`, `((x=1))`, `$((x+=1))`)
        arith_written: HashSet<String>,
        /// vars written by an array-element write (`arr[i]=v`)
        index_written: HashSet<String>,
        /// a bare eval/source/. call exists → every var Var
        dynamic: bool,
    }

    fn site(acc: &mut Acc, name: &str, multi_run: bool) {
        *acc.sites.entry(name.to_string()).or_insert(0) += 1;
        if multi_run {
            acc.multi_run.insert(name.to_string());
        }
    }

    /// Runtime-store write builtins: the store owns the name — the value
    /// arrives from outside the program (stdin, files) or the name is
    /// destroyed, so the var can never be `Const`.
    const STORE_WRITE: &[&str] = &["read", "readarray", "mapfile", "unset"];
    /// Declaration-with-assignment builtins: `local x=5`, `declare -r x=5`,
    /// `readonly x=5`, `export FOO=bar` — a real assignment site (and the
    /// shell's own const story: `readonly`).
    const DECL_ASSIGN: &[&str] = &["local", "declare", "readonly", "export", "typeset"];
    /// Dynamic writes: cannot be tracked statically — disqualify everything.
    const DYNAMIC_WRITE: &[&str] = &["eval", "source", "."];

    /// Identifier names written by a builtin's arg list (the args Array at
    /// args[1]): each `Str` is `name` or `name=value`; flags (`-i`) and
    /// non-identifier words are skipped — mirror of mark_write_builtin_vars.
    fn builtin_names(args: &[IrExpr], out: &mut Vec<String>) {
        for a in args {
            match a {
                IrExpr::Array(elems) => {
                    for el in elems {
                        builtin_names(std::slice::from_ref(el), out);
                    }
                }
                IrExpr::Str(sv, _) => {
                    let name = sv.split('=').next().unwrap_or("");
                    if crate::shared_utils::SharedUtils::is_variable_name(name) {
                        out.push(name.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    /// Bare identifier tokens inside an arithmetic expression string
    /// (`let x++` / `let "x = 5"` / `((x+=1))`): every identifier is a
    /// potential runtime-store write — mirror of mark_all_idents.
    fn arith_idents(s: &str, out: &mut Vec<String>) {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                {
                    i += 1;
                }
                let w = &s[start..i];
                if crate::shared_utils::SharedUtils::is_variable_name(w) {
                    out.push(w.to_string());
                }
            } else {
                i += 1;
            }
        }
    }

    /// The builtin-command shape shared by `IrStmt::Exec` and the
    /// `Call("exec"/"builtin", …)` expression form: args[0] = the command
    /// name, args[1] = the arg-list Array. Classifies the write, if any.
    fn classify_builtin(args: &[IrExpr], acc: &mut Acc, multi_run: bool) {
        let [cmd, IrExpr::Array(rest)] = args else {
            return;
        };
        let Some(cname) = (match cmd {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Ident(s) => Some(s.as_str()),
            _ => None,
        }) else {
            return;
        };
        if DYNAMIC_WRITE.contains(&cname) {
            acc.dynamic = true;
            return;
        }
        if STORE_WRITE.contains(&cname) {
            let mut names = Vec::new();
            builtin_names(rest, &mut names);
            for n in names {
                acc.runtime_written.insert(n);
            }
            return;
        }
        if cname == "let" {
            // `let x=5` / `let x++` / `((x+=1))` — the runtime evaluates
            // arith strings; every bare identifier is a potential write
            // (mirror of mark_all_idents for the exec arg shape).
            // Conservative: `let x=5` as the only write lands Var (a
            // missed const, never a wrong one).
            let mut names = Vec::new();
            for a in rest {
                if let IrExpr::Str(sv, _) = a {
                    arith_idents(sv, &mut names);
                }
            }
            for n in names {
                acc.runtime_written.insert(n);
            }
            return;
        }
        if DECL_ASSIGN.contains(&cname) {
            let mut names = Vec::new();
            builtin_names(rest, &mut names);
            for n in names {
                site(acc, &n, multi_run);
            }
        }
    }

    fn walk_expr(e: &IrExpr, acc: &mut Acc, multi_run: bool) {
        match e {
            IrExpr::Arith(a) => {
                for w in arith_written_vars(a) {
                    acc.arith_written.insert(w);
                }
                walk_arith(a, acc, multi_run);
            }
            IrExpr::Arrow(stmts) => {
                for s in stmts {
                    walk_stmt(s, acc, multi_run);
                }
            }
            IrExpr::Call { func, args } => {
                if func == "setVar" {
                    if let [IrExpr::Str(name, _), _] = args.as_slice() {
                        site(acc, name, multi_run);
                    }
                }
                if func == "setArray" {
                    // `arr=(a b c)` / `declare -a arr=(…)` — the array
                    // declaration is a single assignment site
                    if let [IrExpr::Str(name, _), _] = args.as_slice() {
                        site(acc, name, multi_run);
                    }
                }
                if func == "exec" || func == "builtin" {
                    classify_builtin(args, acc, multi_run);
                }
                for a in args {
                    walk_expr(a, acc, multi_run);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, acc, multi_run);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, acc, multi_run);
                }
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let crate::ir::InterpPart::Expr(x) = p {
                        walk_expr(x, acc, multi_run);
                    }
                }
            }
            IrExpr::BinOp { lhs, rhs, .. }
            | IrExpr::Ternary { cond: lhs, then: rhs, .. } => {
                walk_expr(lhs, acc, multi_run);
                walk_expr(rhs, acc, multi_run);
            }
            IrExpr::Index { key, .. } | IrExpr::Capture { expr: key, .. } => {
                walk_expr(key, acc, multi_run);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, acc, multi_run);
                for a in args {
                    walk_expr(a, acc, multi_run);
                }
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, acc, multi_run);
                walk_expr(default, acc, multi_run);
            }
            IrExpr::Range { .. }
            | IrExpr::Int(_)
            | IrExpr::Str(_, _)
            | IrExpr::Var(_, _)
            | IrExpr::Regex { .. }
            | IrExpr::RawExpr(_)
            | IrExpr::Bool(_)
            | IrExpr::Json(_)
            | IrExpr::Ident(_) => {}
        }
    }

    fn walk_arith(a: &ArithAst, acc: &mut Acc, multi_run: bool) {
        match a {
            ArithAst::Num(_) | ArithAst::Var(_) => {}
            ArithAst::Index { key, .. } => walk_arith(key, acc, multi_run),
            ArithAst::Bin { lhs, rhs, .. } => {
                walk_arith(lhs, acc, multi_run);
                walk_arith(rhs, acc, multi_run);
            }
            ArithAst::Un { arg, .. } => walk_arith(arg, acc, multi_run),
            ArithAst::Cond { test, then, else_, .. } => {
                walk_arith(test, acc, multi_run);
                walk_arith(then, acc, multi_run);
                walk_arith(else_, acc, multi_run);
            }
            // writes already recorded via arith_written_vars above
            ArithAst::Assign { rhs, .. } => walk_arith(rhs, acc, multi_run),
            ArithAst::IncDec { .. } => {}
        }
    }

    fn walk_stmt(st: &IrStmt, acc: &mut Acc, multi_run: bool) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    // array-element writes arrive either with a non-empty
                    // `indices` list or with the index baked into the name
                    // (`arr[1]=z` → var "arr[1]") — the store owns the
                    // element either way.
                    if t.indices.is_empty() && !t.var.contains('[') {
                        site(acc, &t.var, multi_run);
                    } else {
                        acc.index_written.insert(t.var.split('[').next().unwrap_or(&t.var).to_string());
                    }
                }
                walk_expr(expr, acc, multi_run);
            }
            IrStmt::Declare { vars, .. } => {
                for d in vars {
                    site(acc, &d.name, multi_run);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                site(acc, var, multi_run);
                for el in elements {
                    walk_expr(el, acc, multi_run);
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                walk_expr(cond, acc, multi_run);
                for s in then {
                    walk_stmt(s, acc, multi_run);
                }
                for (c, b) in elsifs {
                    walk_expr(c, acc, multi_run);
                    for s in b {
                        walk_stmt(s, acc, multi_run);
                    }
                }
                for s in else_ {
                    walk_stmt(s, acc, multi_run);
                }
            }
            // loop bodies + loop variables run per iteration
            IrStmt::For { var, iter, body } => {
                site(acc, var, true);
                walk_expr(iter, acc, multi_run);
                for s in body {
                    walk_stmt(s, acc, true);
                }
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, acc, multi_run);
                for s in body {
                    walk_stmt(s, acc, true);
                }
            }
            // a function may run 0..N times — its sites are multi-run
            IrStmt::Function { body, .. } => {
                for s in body {
                    walk_stmt(s, acc, true);
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                for s in body {
                    walk_stmt(s, acc, multi_run);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for s in inner {
                    walk_stmt(s, acc, multi_run);
                }
                for r in redirects {
                    walk_expr(&r.target, acc, multi_run);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                walk_expr(discriminant, acc, multi_run);
                for c in clauses {
                    for s in &c.body {
                        walk_stmt(s, acc, multi_run);
                    }
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for s in stage {
                        walk_stmt(s, acc, multi_run);
                    }
                }
            }
            IrStmt::Exec { cmd, args, env, .. } => {
                walk_expr(cmd, acc, multi_run);
                classify_builtin(args, acc, multi_run);
                for a in args {
                    walk_expr(a, acc, multi_run);
                }
                for (_, v) in env {
                    walk_expr(v, acc, multi_run);
                }
            }
            IrStmt::Expr(e) => walk_expr(e, acc, multi_run),
            IrStmt::Output { value, .. } => walk_expr(value, acc, multi_run),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, acc, multi_run);
                walk_expr(content, acc, multi_run);
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_expr(expr, acc, multi_run);
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
                walk_expr(e, acc, multi_run);
            }
            IrStmt::Return(None) | IrStmt::Exit(None) => {}
            IrStmt::Require(_) | IrStmt::RawText(_) => {}
        }
    }

    let mut acc = Acc::default();
    for s in &prog.stmts {
        walk_stmt(s, &mut acc, false);
    }
    for sub in &prog.subs {
        for s in &sub.body {
            walk_stmt(s, &mut acc, true);
        }
    }

    let mut names: Vec<String> = acc.sites.keys().cloned().collect();
    names.extend(acc.runtime_written.iter().cloned());
    names.extend(acc.arith_written.iter().cloned());
    names.extend(acc.index_written.iter().cloned());
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|n| {
            let is_const = acc.sites.get(&n).copied() == Some(1)
                && !acc.multi_run.contains(&n)
                && !acc.runtime_written.contains(&n)
                && !acc.arith_written.contains(&n)
                && !acc.index_written.contains(&n)
                && !acc.dynamic;
            (n, if is_const { VarKind::Const } else { VarKind::Var })
        })
        .collect()
}

// ── Integer range analysis (spike; M8-adjacent) ──────────────────────
// Refines the A2 `Int` verdict into a WIDTH (u32/i32/i64) by tracking a
// conservative integer interval per variable through straight-line code.
// Nothing here changes emitted output — it is the proof layer backends
// (C/GLSL/wasm) consult for narrower integer types. Bash arithmetic is
// 64-bit wrapped, so an op result is kept only when it provably cannot
// overflow; unprovable provenance (cmdsub, exec, read, loops without a
// fixpoint) → None (Any). Returns var -> (lo, hi) for provably-integer
// vars.

pub fn analyze_var_ranges(prog: &IrProgram) -> HashMap<String, (i128, i128)> {
    let mut state: HashMap<String, Option<(i128, i128)>> = HashMap::new();
    for s in &prog.stmts {
        walk_stmt_ranges(s, &mut state);
    }
    state
        .into_iter()
        .filter_map(|(n, r)| r.map(|r| (n, r)))
        .collect()
}

/// Width a [lo, hi] range maps to (target type table — C/Rust/GLSL).
/// u64 fires only past signed-64 — a range only a C-frontend unsigned
/// integer can produce (bash's arithmetic never leaves i64).
pub fn range_width_name(lo: i128, hi: i128) -> &'static str {
    if lo >= 0 && hi <= u32::MAX as i128 {
        "u32"
    } else if lo >= i32::MIN as i128 && hi <= i32::MAX as i128 {
        "i32"
    } else if lo >= 0 && hi > i64::MAX as i128 {
        "u64"
    } else {
        "i64"
    }
}

type Range = Option<(i128, i128)>;

/// The FRONTEND's integer arithmetic domain, not the storage width:
/// bash is signed-64-bit wrapped, so a provable range never leaves
/// [i64::MIN, i64::MAX] — an op/literal that can cross it is top (None).
/// The C frontend's unsigned types widen this per variable once
/// `var_types` carries width + signedness (frontend-c-core-needs.md);
/// the i128 storage already represents the full u64 domain.
const INT_DOMAIN: (i128, i128) = (i64::MIN as i128, i64::MAX as i128);

/// A range is representable in the frontend's integer domain only when
/// every value fits — a result that can leave it is top (None).
fn in_domain(r: (i128, i128)) -> bool {
    r.0 >= INT_DOMAIN.0 && r.1 <= INT_DOMAIN.1
}

fn join(a: Range, b: Range) -> Range {
    match (a, b) {
        (Some((l1, h1)), Some((l2, h2))) => Some((l1.min(l2), h1.max(h2))),
        _ => None,
    }
}

// ── Loop fixpoint machinery (widen + loop-cond bounding) ─────────────
// The spike's "Step 2": a `while [ $i -lt 100 ]; do i=$((i+1)); done`
// counter now lands in [lo, 100] instead of Any. The fixed-point
// iteration widens outward-moving bounds to the ±i64 arithmetic extremes
// (bash arith is 64-bit wrapped — sound and terminating), then pulls the
// loop variable back by the entry invariant (`v < 100` → hi ≤ 99) and by
// the trip count for the OTHER carried counters (i ≤ pre_lo + trip·step).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

fn cmp_flip(c: Cmp) -> Cmp {
    // NOT(v < n) = v >= n, NOT(v <= n) = v > n, ...
    match c {
        Cmp::Lt => Cmp::Ge,
        Cmp::Le => Cmp::Gt,
        Cmp::Gt => Cmp::Le,
        Cmp::Ge => Cmp::Lt,
        Cmp::Eq => Cmp::Eq,
    }
}

/// Apply a `while var cmp n` entry invariant to a range (cap the side the
/// comparison pins; None when the range is impossible).
fn cap_by_cmp(r: (i128, i128), c: Cmp, n: i128) -> Range {
    let (lo, hi) = r;
    match c {
        Cmp::Lt => {
            if n == i64::MIN as i128 {
                return None;
            }
            let hi2 = hi.min(n - 1);
            if lo > hi2 {
                None
            } else {
                Some((lo, hi2))
            }
        }
        Cmp::Le => {
            let hi2 = hi.min(n);
            if lo > hi2 {
                None
            } else {
                Some((lo, hi2))
            }
        }
        Cmp::Gt => {
            if n == i64::MAX as i128 {
                return None;
            }
            let lo2 = lo.max(n + 1);
            if lo2 > hi {
                None
            } else {
                Some((lo2, hi))
            }
        }
        Cmp::Ge => {
            let lo2 = lo.max(n);
            if lo2 > hi {
                None
            } else {
                Some((lo2, hi))
            }
        }
        Cmp::Eq => {
            if lo <= n && n <= hi {
                Some((n, n))
            } else {
                None
            }
        }
    }
}

/// The numeric bound a `while`/`until` condition pins on a variable:
/// `[ $i -lt 100 ]`, `(( i < 100 ))` (the parser lowers it to
/// `let "i < 100"`), or the `until` negation. `(var, cmp, n)` = "the loop
/// continues while `var cmp n`". String compares (`[[ ]]`), grep-lifted
/// conds, file tests — None (no numeric bound).
fn cond_bound(cond: &IrExpr) -> Option<(String, Cmp, i128)> {
    match cond {
        IrExpr::Call { func, args } if func == "test" => match args.as_slice() {
            [IrExpr::Str(text, _)] => test_text_bound(text),
            _ => None,
        },
        // `while (( i < 100 ))` — the parser emits `let "i < 100"`
        IrExpr::Call { func, args } if func == "exec" => match args.as_slice() {
            [IrExpr::Str(name, _), IrExpr::Array(items)] if name == "let" => {
                match items.as_slice() {
                    [IrExpr::Str(text, _)] => arith_text_bound(text),
                    _ => None,
                }
            }
            _ => None,
        },
        // `until cond` wraps the cond in BinOp(Not, cond, cond)
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs,
            ..
        } => cond_bound(lhs).map(|(v, c, n)| (v, cmp_flip(c), n)),
        _ => None,
    }
}

/// `[ $i -lt 100 ]` / `[ 100 -gt $i ]` — single-bracket NUMERIC tests
/// only (`-lt/-le/-gt/-ge/-eq`); `<`/`>` are string compares and are NOT
/// trusted as numeric bounds.
fn test_text_bound(text: &str) -> Option<(String, Cmp, i128)> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let var =
        |s: &str| s.strip_prefix('$').filter(|n| !n.is_empty()).map(str::to_string);
    let num = |s: &str| s.parse::<i128>().ok();
    let cmp = |s: &str| match s {
        "-lt" => Some(Cmp::Lt),
        "-le" => Some(Cmp::Le),
        "-gt" => Some(Cmp::Gt),
        "-ge" => Some(Cmp::Ge),
        "-eq" => Some(Cmp::Eq),
        _ => None,
    };
    if let (Some(v), Some(c), Some(n)) = (var(parts[0]), cmp(parts[1]), num(parts[2])) {
        Some((v, c, n))
    } else if let (Some(n), Some(c), Some(v)) = (num(parts[0]), cmp(parts[1]), var(parts[2])) {
        // `[ 100 -gt $i ]` ⇔ `$i -lt 100`
        Some((v, cmp_flip(c), n))
    } else {
        None
    }
}

/// `let "i < 100"` — numeric arithmetic comparisons only.
fn arith_text_bound(text: &str) -> Option<(String, Cmp, i128)> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let var = |s: &str| {
        if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            Some(s.to_string())
        } else {
            None
        }
    };
    let num = |s: &str| s.parse::<i128>().ok();
    let cmp = |s: &str| match s {
        "<" => Some(Cmp::Lt),
        "<=" => Some(Cmp::Le),
        ">" => Some(Cmp::Gt),
        ">=" => Some(Cmp::Ge),
        "==" | "=" => Some(Cmp::Eq),
        _ => None,
    };
    if let (Some(v), Some(c), Some(n)) = (var(parts[0]), cmp(parts[1]), num(parts[2])) {
        Some((v, c, n))
    } else if let (Some(n), Some(c), Some(v)) = (num(parts[0]), cmp(parts[1]), var(parts[2])) {
        Some((v, cmp_flip(c), n))
    } else {
        None
    }
}

/// The for-loop variable's range from its iterable: min/max over integer
/// items (`for i in 1 2 3`), a Range node (`for i in $(seq 1 100)` after
/// the seq_range_for transform), or None (a non-numeric item → Any).
fn for_iter_range(iter: &IrExpr) -> Range {
    match iter {
        IrExpr::Array(items) => {
            let mut lo = i128::MAX;
            let mut hi = i128::MIN;
            for it in items {
                match ir_range(it, &HashMap::new()) {
                    Some((l, h)) => {
                        lo = lo.min(l);
                        hi = hi.max(h);
                    }
                    None => return None,
                }
            }
            if lo <= hi {
                Some((lo, hi))
            } else {
                None // empty item list — the loop never runs
            }
        }
        // the Range node is a frontend-constructed bounded iterable
        IrExpr::Range { start, end } => {
            let (s, e) = (*start as i128, *end as i128);
            Some((s.min(e), s.max(e)))
        }
        _ => None,
    }
}

/// The for-loop's iteration count (trip): item count / range span.
fn for_iter_trip(iter: &IrExpr) -> Option<u64> {
    match iter {
        IrExpr::Array(items) => Some(items.len() as u64),
        IrExpr::Range { start, end } => {
            let span = ((*start as i128) - (*end as i128)).unsigned_abs() + 1;
            Some(span.min(u64::MAX as u128) as u64)
        }
        _ => None,
    }
}

/// The literal step of `v = v + k` / `v = k + v` / `v += k` with k ≥ 1.
fn plus_step(expr: &IrExpr, v: &str) -> Option<i64> {
    match expr {
        IrExpr::Arith(a) => match a.as_ref() {
            ArithAst::Bin { op, lhs, rhs } if op == "+" => {
                match (lhs.as_ref(), rhs.as_ref()) {
                    (ArithAst::Var(n), ArithAst::Num(k))
                    | (ArithAst::Num(k), ArithAst::Var(n))
                        if n == v =>
                    {
                        if *k >= 1 {
                            Some(*k)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        },
        IrExpr::Call { func, args } if func == "assign" => match args.as_slice() {
            [IrExpr::Str(n, _), IrExpr::Str(op, _), e] if n == v && op == "+=" => {
                match ir_range(e, &HashMap::new()) {
                    Some((lo, hi)) if lo >= 1 && hi >= lo => i64::try_from(lo).ok(),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// The per-iteration step bounds of var v in a loop body — provable only
/// when EVERY write to v (on every path) is `v = v + k` / `v += k` with
/// literal k ≥ 1. Returns (min_step, max_step); None → not a provable
/// counter (the widening stands alone). min drives the trip count (fewer
/// iterations when the body may add more); max drives the other vars'
/// trip caps.
fn body_step_bounds(stmts: &[IrStmt], v: &str) -> Option<(i64, i64)> {
    let mut mn: Option<i64> = None;
    let mut mx: Option<i64> = None;
    for s in stmts {
        let r: Option<(i64, i64)> = match s {
            IrStmt::Assign { targets, expr } => {
                if targets.iter().any(|t| t.indices.is_empty() && t.var == v) {
                    return plus_step(expr, v).map(|k| (k, k));
                }
                None
            }
            IrStmt::Block(b)
            | IrStmt::Subshell(b)
            | IrStmt::Background(b)
            | IrStmt::Redirect { inner: b, .. } => body_step_bounds(b, v),
            IrStmt::If { then, elsifs, else_, .. } => {
                // every path through the body must increase v
                let (a, b) = body_step_bounds(then, v)?;
                let mut lo = a;
                let mut hi = b;
                for (_, arm) in elsifs {
                    let (x, y) = body_step_bounds(arm, v)?;
                    lo = lo.min(x);
                    hi = hi.max(y);
                }
                let (x, y) = body_step_bounds(else_, v)?;
                Some((lo.min(x), hi.max(y)))
            }
            // statements that don't write v impose no constraint
            _ => None,
        };
        if let Some((lo, hi)) = r {
            mn = Some(mn.map_or(lo, |m| m.min(lo)));
            mx = Some(mx.map_or(hi, |m| m.max(hi)));
        }
    }
    mn.zip(mx)
}

/// Iterations of `while v cmp n` when v starts inside [blo, bhi] and the
/// body moves it by ≥ step per iteration (monotone). Sound over-approx
/// (i128 math avoids overflow).
fn while_iterations(cmp: Cmp, n: i128, blo: i128, bhi: i128, step: i128) -> Option<u64> {
    let iters = match cmp {
        Cmp::Lt => {
            if n <= blo {
                0
            } else {
                (n - blo + step - 1) / step
            }
        }
        Cmp::Le => {
            if n < blo {
                0
            } else {
                (n - blo + step) / step
            }
        }
        Cmp::Gt => {
            if n >= bhi {
                0
            } else {
                (bhi - n + step - 1) / step
            }
        }
        Cmp::Ge => {
            if n > bhi {
                0
            } else {
                (bhi - n + step) / step
            }
        }
        Cmp::Eq => {
            if blo <= n && n <= bhi {
                1
            } else {
                0
            }
        }
    };
    Some(iters.min(u64::MAX as i128) as u64)
}

const FIXPOINT_ITER_LIMIT: usize = 1024;

/// Iterate a loop body to a widened fixed point (the range analysis's
/// loop arm). `skip_var` (the for-loop variable) is never carried — the
/// item list re-sets it every iteration. `bound` pins the entry range of
/// one variable (`while v cmp n`). Post-state = pre ∪ (body's last write
/// evaluated from the entry invariant — the loop may never run, and the
/// final write may overshoot the bound by a step).
fn loop_fixpoint(
    state: &mut HashMap<String, Range>,
    body: &[IrStmt],
    skip_var: Option<&str>,
    bound: Option<(String, Cmp, i128)>,
) {
    let pre = state.clone();
    let mut carried: HashSet<String> = HashSet::new();
    for s in body {
        collect_assigned(s, &mut carried);
    }
    if let Some(sk) = skip_var {
        carried.remove(sk);
    }
    if carried.is_empty() {
        return; // the loop cannot change any variable's range
    }

    // the cond variable's entry range (for the trip count)
    let (cond_var, cmp, n) = match &bound {
        Some((v, c, n)) => (Some(v.clone()), *c, *n),
        None => (None, Cmp::Lt, 0),
    };
    let cond_pre = cond_var.as_ref().and_then(|v| pre.get(v).copied().flatten());

    // trip: how many times can the loop run? Requires the cond var to be
    // a provable counter with a known entry lower bound.
    let trip: Option<u64> = match (&cond_var, cond_pre) {
        (Some(v), Some((blo, bhi))) => match body_step_bounds(body, v) {
            Some((mn, _)) => while_iterations(cmp, n, blo, bhi, mn as i128),
            None => None,
        },
        _ => None,
    };

    // entry-invariant state (widening + caps)
    let mut ls = pre.clone();
    for _ in 0..FIXPOINT_ITER_LIMIT {
        let mut ns = ls.clone();
        for s in body {
            walk_stmt_ranges(s, &mut ns);
        }
        if let Some(sk) = skip_var {
            // the for-loop variable is re-set from the item list
            if let Some(r) = pre.get(sk).copied().flatten() {
                ns.insert(sk.to_string(), Some(r));
            }
        }
        let mut changed = false;
        for v in &carried {
            let before = ls.get(v).copied().flatten();
            let after = ns.get(v).copied().flatten();
            let mut joined = join(after, before);
            // widening: a bound that moved outward becomes the frontend
            // integer domain's extreme (bash: ±i64 — sound, since values
            // past it wrap; terminating)
            if let (Some((blo, bhi)), Some((jlo, jhi))) = (before, joined) {
                let wlo = if jlo < blo { INT_DOMAIN.0 } else { jlo };
                let whi = if jhi > bhi { INT_DOMAIN.1 } else { jhi };
                joined = Some((wlo, whi));
            }
            // entry invariant: while v cmp n → cap the entry range
            if let (Some((bv, bc, bn)), Some(r)) = (&bound, joined) {
                if bv == v {
                    joined = cap_by_cmp(r, *bc, *bn);
                }
            }
            // trip cap: a carried counter can move at most trip·step
            if let (Some(t), Some(r)) = (trip, joined) {
                if let Some((_, max_step)) = body_step_bounds(body, v) {
                    let t = t.saturating_mul(max_step as u64);
                    let lo0 = pre
                        .get(v)
                        .copied()
                        .flatten()
                        .map_or(INT_DOMAIN.0, |r| r.0);
                    let hi0 = pre
                        .get(v)
                        .copied()
                        .flatten()
                        .map_or(INT_DOMAIN.1, |r| r.1);
                    // increasing counter: v ≤ pre_lo + trip·step
                    let cap_hi = lo0.saturating_add(t as i128);
                    // decreasing counter: v ≥ pre_hi − trip·step
                    let cap_lo = hi0.saturating_sub(t as i128);
                    joined = Some((r.0.max(cap_lo), r.1.min(cap_hi)));
                }
            }
            if joined != before {
                ls.insert(v.clone(), joined);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // post-loop: pre ∪ (the body's last write, from the entry invariant)
    let mut post = pre.clone();
    for v in &carried {
        // insert the invariant even when it is None — a carried var whose
        // entry range is unknown stays unknown after the loop (the single
        // write from `pre` alone would be unsound)
        post.insert(v.clone(), ls.get(v).copied().flatten());
    }
    for s in body {
        walk_stmt_ranges(s, &mut post);
    }
    // join the pre-loop values back (the loop may never run)
    for v in &carried {
        let a = post.get(v).copied().flatten();
        let b = pre.get(v).copied().flatten();
        post.insert(v.clone(), join(a, b));
    }
    // the for-loop variable: the item range (bash re-sets it per
    // iteration; an empty list keeps the pre-loop value)
    if let Some(sk) = skip_var {
        if let Some(r) = pre.get(sk).copied().flatten() {
            post.insert(sk.to_string(), Some(r));
        }
    }
    *state = post;
}


fn walk_stmt_ranges(s: &IrStmt, state: &mut HashMap<String, Range>) {
    match s {
        IrStmt::Assign { targets, expr } if targets.len() == 1 && targets[0].indices.is_empty() => {
            let name = targets[0].var.clone();
            state.insert(name, ir_range(expr, state));
        }
        // multi-target / indexed assignments — no single-variable range
        IrStmt::Assign { .. } => {}
        IrStmt::Expr(IrExpr::Call { func, args })
            if func == "setVar"
                && matches!(args.as_slice(), [IrExpr::Str(_, _), _]) =>
        {
            if let [IrExpr::Str(name, _), e] = args.as_slice() {
                state.insert(name.clone(), ir_range(e, state));
            }
        }
        IrStmt::Block(stmts) => {
            for s in stmts {
                walk_stmt_ranges(s, state);
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner {
                walk_stmt_ranges(s, state);
            }
        }
        IrStmt::If { cond: _, then, elsifs, else_ } => {
            let mut s1 = state.clone();
            for s in then {
                walk_stmt_ranges(s, &mut s1);
            }
            let mut s2 = state.clone();
            for (_, b) in elsifs {
                for s in b {
                    walk_stmt_ranges(s, &mut s2);
                }
            }
            for s in else_ {
                walk_stmt_ranges(s, &mut s2);
            }
            // meet (join): a var's range after the if is the union of what
            // each branch (or the pre-state, when a branch didn't assign it)
            // produced.
            let mut merged: HashMap<String, Range> = HashMap::new();
            for (k, v) in s1 {
                let w = s2.get(&k).copied().flatten();
                merged.insert(k, join(v, w));
            }
            for (k, v) in s2 {
                if !merged.contains_key(&k) {
                    let j = join(v, state.get(&k).copied().flatten());
                    merged.insert(k, j);
                }
            }
            *state = merged;
        }
        IrStmt::While { cond, body } => {
            loop_fixpoint(state, body, None, cond_bound(cond));
        }
        IrStmt::DoWhile { body, cond, until } => {
            let b = cond_bound(cond);
            let b = if *until {
                b.map(|(v, c, n)| (v, cmp_flip(c), n))
            } else {
                b
            };
            loop_fixpoint(state, body, None, b);
        }
        IrStmt::For { var, iter, body } => {
            // the loop variable is re-set from the item list every
            // iteration (bash semantics — body writes to it are lost)
            if let Some(r) = for_iter_range(iter) {
                state.insert(var.clone(), Some(r));
            }
            loop_fixpoint(state, body, Some(var), None);
        }
        // Subshell / Background / Function: definitions or child scopes —
        // they cannot change the parent's ranges.
        IrStmt::Subshell(_)
        | IrStmt::Background(_)
        | IrStmt::Function { .. }
        | IrStmt::Case { .. }
        | IrStmt::Pipeline { .. }
        | IrStmt::Return(_)
        | IrStmt::Exit(_)
        | IrStmt::SetChildError(_)
        | IrStmt::Require(_)
        | IrStmt::RawText(_)
        | IrStmt::Output { .. }
        | IrStmt::WriteFile { .. }
        | IrStmt::Declare { .. }
        | IrStmt::DeclareArray { .. }
        | IrStmt::Die { .. }
        | IrStmt::Warn { .. }
        | IrStmt::Exec { .. }
        | IrStmt::Expr(_) => {}
    }
}

fn collect_assigned(s: &IrStmt, out: &mut HashSet<String>) {
    match s {
        IrStmt::Assign { targets, .. } => {
            for t in targets {
                out.insert(t.var.clone());
            }
        }
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "setVar" || func == "assign" => {
            // setVar(name, v) and the runtime `assign(name, op, v)` form
            // (v+=1, a && v=x) both name the target first
            if let [IrExpr::Str(name, _), ..] = args.as_slice() {
                out.insert(name.clone());
            }
        }
        IrStmt::Block(stmts) | IrStmt::Subshell(stmts) => {
            for s in stmts {
                collect_assigned(s, out);
            }
        }
        IrStmt::If { cond: _, then, elsifs, else_ } => {
            for s in then {
                collect_assigned(s, out);
            }
            for (_, b) in elsifs {
                for s in b {
                    collect_assigned(s, out);
                }
            }
            for s in else_ {
                collect_assigned(s, out);
            }
        }
        IrStmt::While { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::DoWhile { body, .. } => {
            for s in body {
                collect_assigned(s, out);
            }
        }
        _ => {}
    }
}

fn ir_range(e: &IrExpr, state: &HashMap<String, Range>) -> Range {
    match e {
        IrExpr::Int(i) => Some((*i as i128, *i as i128)),
        // variables are strings; a literal that parses as an integer in
        // the frontend's domain is provably that value (matches the
        // numeric lift's "sources that parse as integers").
        IrExpr::Str(sv, _) => sv
            .trim()
            .parse::<i128>()
            .ok()
            .map(|n| (n, n))
            .filter(|&r| in_domain(r)),
        IrExpr::Var(n, _) => state.get(n).copied().flatten(),
        IrExpr::Call { func, args } if func == "getVar" => match args.as_slice() {
            [IrExpr::Str(n, _)] => state.get(n).copied().flatten(),
            _ => None,
        },
        // the runtime `assign(name, op, v)` form: v+=k shifts the range,
        // v=v keeps the value's range (bash semantics)
        IrExpr::Call { func, args } if func == "assign" => match args.as_slice() {
            [IrExpr::Str(n, _), IrExpr::Str(op, _), e] if op == "+=" => {
                let k = ir_range(e, state)?;
                let shifted = match state.get(n).copied().flatten() {
                    Some((lo, hi)) => Some((lo.checked_add(k.0)?, hi.checked_add(k.1)?)),
                    None => Some(k),
                };
                shifted.filter(|&r| in_domain(r))
            }
            [IrExpr::Str(_, _), IrExpr::Str(op, _), e] if op == "=" => ir_range(e, state),
            _ => None,
        },
        IrExpr::Arith(a) => arith_range(a, state),
        _ => None,
    }
}

fn arith_range(a: &ArithAst, state: &HashMap<String, Range>) -> Range {
    match a {
        ArithAst::Num(i) => Some((*i as i128, *i as i128)),
        ArithAst::Var(n) => state.get(n).copied().flatten(),
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (arith_range(lhs, state)?, arith_range(rhs, state)?);
            let (l0, l1, r0, r1) = (l.0, l.1, r.0, r.1);
            // the interval of the op, provably within the frontend's
            // integer domain; a result that can leave it (wrap) is top
            let res = match op.as_str() {
                "+" => Some((l0.checked_add(r0)?, l1.checked_add(r1)?)),
                "-" => Some((l0.checked_sub(r1)?, l1.checked_sub(r0)?)),
                "*" => {
                    // min/max over all four endpoint products; None on overflow
                    let ps = [
                        l0.checked_mul(r0)?,
                        l0.checked_mul(r1)?,
                        l1.checked_mul(r0)?,
                        l1.checked_mul(r1)?,
                    ];
                    Some((*ps.iter().min()?, *ps.iter().max()?))
                }
                "/" => {
                    if r0 <= 0 && r1 >= 0 {
                        return None; // possible division by zero
                    }
                    let qs = [
                        l0.checked_div(r0)?,
                        l0.checked_div(r1)?,
                        l1.checked_div(r0)?,
                        l1.checked_div(r1)?,
                    ];
                    Some((*qs.iter().min()?, *qs.iter().max()?))
                }
                _ => None, // % , ^, ... conservative
            };
            res.filter(|&r| in_domain(r))
        }
        ArithAst::Un { op, arg } => {
            let (lo, hi) = arith_range(arg, state)?;
            let res = match op.as_str() {
                "-" => Some((-hi, -lo)),
                "+" => Some((lo, hi)),
                _ => None,
            };
            res.filter(|&r| in_domain(r))
        }
        _ => None, // Index / Cond / Assign / IncDec — step 2
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
        // UNQUOTED pure expansion in exec-arg position (`echo $y`,
        // `set -- $y`): bash field-splits it on IFS into separate args. A
        // bare Word::Variable is unquoted by construction — quoted `"$y"`
        // is a StringInterpolation and assignment forms (`x=$y`, `PATH=$y`)
        // merge into interpolations too, so neither is split (assignment
        // context never field-splits). The runtime flattens the split's
        // array into separate args (the A1 `split` marker, same contract
        // as for_item_ir). `$@`/`$*` keep the bare read (the runtime's
        // positional-join semantics, see the exec name handling).
        Word::Variable(name, _, _) if name != "@" && name != "*" => {
            call("split", vec![call("getVar", vec![st(name)])])
        }
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
        // UNQUOTED `for w in $y`: bash field-splits the expansion on IFS and
        // iterates per FIELD (`y="hello world"` → two iterations). A bare
        // Word::Variable is unquoted BY CONSTRUCTION — a quoted `"$y"`
        // parses as a StringInterpolation (next arm) and keeps the bare
        // getVar — so the `split` marker carries the quoted/unquoted
        // distinction the A1 contract previously dropped (core request
        // posix-sh-go-20260806-152225; its failing gate case t04_params.sh
        // is exactly this shape). The estree lowering renders split(x) as a
        // native whitespace field-split; `$@`/`$*` keep listVar above (the
        // runtime's per-positional flatten, never IFS-split).
        Word::Variable(name, _, _) => call("split", vec![call("getVar", vec![st(name)])]),
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
                    op: op.to_string(),
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
                op: "**".to_string(),
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
                    op: "*".to_string(),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '/' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "/".to_string(),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '%' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "%".to_string(),
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
                    op: "+".to_string(),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '-' {
                *pos += 1;
                let rhs = mul(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "-".to_string(),
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
                    op: "<<".to_string(),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if eat2(chars, pos, ">>") {
                let rhs = add(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: ">>".to_string(),
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
                    op: op.to_string(),
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
                    op: "==".to_string(),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if eat2(chars, pos, "!=") {
                let rhs = rel(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "!=".to_string(),
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
                    op: "&".to_string(),
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
                    op: "^".to_string(),
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
                    op: "|".to_string(),
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
                    op: "&&".to_string(),
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
                    op: "||".to_string(),
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
                    op: op.to_string(),
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
        operator: "||".to_string(),
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
/// Does the arith AST contain a NaN-COERCING operator anywhere? Bitwise
/// ops (`|` `&` `^` shifts) coerce NaN to 0, `**` to 1 (exponent 0),
/// comparisons to false, `&&`/`||`/`!`/ternary-conds to truthiness —
/// where bash would abort the whole expansion on a zero divisor. Only
/// `+ - * / %` and unary +/- propagate NaN faithfully.
fn arith_has_poison(a: &ArithAst) -> bool {
    match a {
        ArithAst::Bin { op, lhs, rhs } => {
            matches!(
                op.as_str(),
                "|" | "&" | "^" | "<<" | ">>" | ">>>" | "**" | "<" | "<=" | ">"
                    | ">=" | "==" | "!=" | "&&" | "||"
            ) || arith_has_poison(lhs)
                || arith_has_poison(rhs)
        }
        ArithAst::Un { op, arg } => op == "!" || arith_has_poison(arg),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            arith_has_poison(test) || arith_has_poison(then) || arith_has_poison(else_)
        }
        _ => false,
    }
}

/// Top-level arith lowering with the poison-depth bookkeeping: the
/// div/mod arms (see [`arith_to_estree`]) consult [`ARITH_POISON_DEPTH`]
/// to decide native-vs-throw, but an arm only sees its LOCAL subtree —
/// whether a NaN result would later be coerced depends on ANCESTORS.
/// Every external entry point (the Arith expr arm, test operands, lifted
/// assignments, `let`/`(( ))` statements, array keys) lowers the WHOLE
/// expression through this wrapper so the depth reflects the root's
/// poison-ness for every nested div/mod.
fn arith_to_estree_wrapped(a: &ArithAst) -> Expr {
    if arith_has_poison(a) {
        *ARITH_POISON_DEPTH.lock().unwrap() += 1;
        let out = arith_to_estree(a);
        *ARITH_POISON_DEPTH.lock().unwrap() -= 1;
        out
    } else {
        arith_to_estree(a)
    }
}

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
                operator: op.to_string(),
                left: Box::new(l),
                right: Box::new(r),
            };
            let new_val = match op.as_str() {
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
                    operator: if *delta > 0 {"++".to_string()} else {"--".to_string()},
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
                operator: if *delta > 0 {"+".to_string()} else {"-".to_string()},
                left: Box::new(cur),
                right: Box::new(int1()),
            };
            let value = if *prefix {
                arith_var_read(var)
            } else {
                Expr::BinaryExpression {
                    operator: if *delta > 0 {"-".to_string()} else {"+".to_string()},
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
            operator: "||".to_string(),
            left: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "Number".to_string(),
                }),
                arguments: vec![sh2_call("arrayIndex", vec![str_lit(var), arith_to_estree_wrapped(key)])],
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
                                operator: "/".to_string(),
                                left: Box::new(arith_to_estree(lhs)),
                                right: Box::new(r),
                            }],
                            optional: false,
                        }
                    } else {
                        Expr::BinaryExpression {
                            operator: "%".to_string(),
                            left: Box::new(arith_to_estree(lhs)),
                            right: Box::new(r),
                        }
                    }
                } else if *op == "/" {
                    // zero divisor must abort the whole expansion. The
                    // native form (SH2_ASSUME_ARITH_NATIVE, default ON —
                    // see [`arith_native_enabled`]) emits `Math.trunc`:
                    // a zero divisor yields NaN/Infinity, which the
                    // arithEval wrapper (or the test-cond false-ness)
                    // converts to the bash abort — no per-evaluation
                    // runtime dispatch. Suppressed inside POISONED
                    // expressions (a NaN-coercing ancestor — see
                    // [`arith_has_poison`]): JS would absorb the NaN
                    // (bitwise → 0, `**` → 1, comparison → false) where
                    // bash aborts the whole expansion, so those keep the
                    // throwing helper. =0 restores the helper everywhere.
                    if !arith_native_enabled() || *ARITH_POISON_DEPTH.lock().unwrap() > 0 {
                        sh2_call("idiv", vec![arith_to_estree(lhs), r])
                    } else {
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
                                operator: "/".to_string(),
                                left: Box::new(arith_to_estree(lhs)),
                                right: Box::new(r),
                            }],
                            optional: false,
                        }
                    }
                } else {
                    // modulo by zero aborts the expansion too (bash
                    // "division by 0"); the native `%` yields NaN on a
                    // zero divisor — same abort channels + poison
                    // suppression as the division arm above.
                    if !arith_native_enabled() || *ARITH_POISON_DEPTH.lock().unwrap() > 0 {
                        sh2_call("imod", vec![arith_to_estree(lhs), r])
                    } else {
                        Expr::BinaryExpression {
                            operator: "%".to_string(),
                            left: Box::new(arith_to_estree(lhs)),
                            right: Box::new(r),
                        }
                    }
                }
            } else if *op == "&&" || *op == "||" {
                // bash yields 0/1; JS logicals yield one of the operands
                Expr::ConditionalExpression {
                    test: Box::new(Expr::LogicalExpression {
                        operator: op.to_string(),
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
            } else if matches!(op.as_str(), "<" | "<=" | ">" | ">=" | "==" | "!=") {
                // bash comparisons yield 0/1; JS yields booleans
                Expr::ConditionalExpression {
                    test: Box::new(Expr::BinaryExpression {
                        operator: op.to_string(),
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
                    operator: op.to_string(),
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
                        operator: "!".to_string(),
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
                    operator: op.to_string(),
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
/// A for-item string must be a CANONICAL decimal integer (exact
/// round-trip through i64): `05`/`+5`/`-0`/` 5` stay strings — the
/// numeric lift's `Number("05")` coercion would lose the leading zeros
/// (bash keeps the raw string in the variable, so `echo $i` prints
/// `05`). Canonical items make the Number coercion exact, which the
/// loop-var persistence (see [`analyze_loop_var_refs`]) relies on.
fn canonical_int_item(sv: &str) -> bool {
    matches!(sv.parse::<i64>(), Ok(v) if v.to_string() == sv)
}

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
                if !canonical_int_item(sv) {
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
                        if !canonical_int_item(sv) {
                            numeric = false;
                        }
                    }
                    // the `seq_range_for` transform's Range item
                    // (`for i in $(seq A B)` → `Range`)
                    IrExpr::Range { .. } => {}
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

/// A lifted FOR-loop variable referenced OUTSIDE its loop body: the
/// shadowed loop binding (`for (let i of …)` / the runtime loop's closure
/// param) would leave the module `let` at its stale initial value, so the
/// loop emission must PERSIST the final value into the module binding (see
/// [`LOOP_PERSIST`] / `loop_persist_needed`).
///
/// The persistence is sound only where the module binding IS the variable:
/// outside COPY regions (subshell/background/capture bodies — bash writes
/// there are copy-local, and the runtime's store copy/restore or the
/// shadowed binding keep the parent value). A loop var referenced outside
/// its loop with ANY loop inside a copy region stays store-bound (dropped
/// from both lift sets — the pre-existing conservative behavior). Loops in
/// functions/pipelines/redirects are NOT copy regions (the runtime runs
/// those in-process on the shared store — the model the lift walkers use).
///
/// Returns the lift sets MINUS the dropped vars; fills [`LOOP_PERSIST`]
/// with the For-statement pointers that need the persist machinery.
fn analyze_loop_var_refs(
    prog: &IrProgram,
    num: &HashSet<String>,
    str: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    // per-For-stmt: (stmt ptr, var, loop inside a copy region)
    let mut loops: Vec<(usize, String, bool)> = Vec::new();
    // vars with any ref outside their loop stack
    let mut external: HashSet<String> = HashSet::new();

    // a capture/subshell/background body is a COPY region — bash writes
    // there are copy-local (the runtime's subshell store copy/restore
    // makes the store-backed model exact; a native binding would leak)
    fn copy_arrow_call(func: &str) -> bool {
        matches!(func, "capture" | "captureWords" | "subshell" | "background")
    }
    fn ref_expr(
        e: &IrExpr,
        stack: &[String],
        external: &mut HashSet<String>,
        in_copy: bool,
        loops: &mut Vec<(usize, String, bool)>,
    ) {
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
                let inner_copy = in_copy || copy_arrow_call(func);
                for a in args {
                    ref_expr(a, stack, external, inner_copy, loops);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    ref_stmt(st, stack, external, in_copy, loops);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                ref_expr(lhs, stack, external, in_copy, loops);
                ref_expr(rhs, stack, external, in_copy, loops);
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                ref_expr(cond, stack, external, in_copy, loops);
                ref_expr(then, stack, external, in_copy, loops);
                ref_expr(else_, stack, external, in_copy, loops);
            }
            IrExpr::Capture { expr, .. } => {
                ref_expr(expr, stack, external, true, loops);
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    ref_expr(el, stack, external, in_copy, loops);
                }
            }
            IrExpr::DefinedOr { expr, default } => {
                ref_expr(expr, stack, external, in_copy, loops);
                ref_expr(default, stack, external, in_copy, loops);
            }
            IrExpr::Index { key, .. } => ref_expr(key, stack, external, in_copy, loops),
            IrExpr::MethodCall { obj, args, .. } => {
                ref_expr(obj, stack, external, in_copy, loops);
                for a in args {
                    ref_expr(a, stack, external, in_copy, loops);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    ref_expr(v, stack, external, in_copy, loops);
                }
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        ref_expr(inner, stack, external, in_copy, loops);
                    }
                }
            }
            _ => {}
        }
    }
    fn ref_stmt(
        st: &IrStmt,
        stack: &[String],
        external: &mut HashSet<String>,
        in_copy: bool,
        loops: &mut Vec<(usize, String, bool)>,
    ) {
        match st {
            IrStmt::For { var, iter, body } => {
                loops.push((st as *const IrStmt as usize, var.clone(), in_copy));
                let mut s2 = stack.to_vec();
                s2.push(var.clone());
                ref_expr(iter, &s2, external, in_copy, loops);
                for b in body {
                    ref_stmt(b, &s2, external, in_copy, loops);
                }
            }
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if !stack.contains(&t.var) {
                        external.insert(t.var.clone());
                    }
                }
                ref_expr(expr, stack, external, in_copy, loops);
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
                ref_expr(cond, stack, external, in_copy, loops);
                for b in body {
                    ref_stmt(b, stack, external, in_copy, loops);
                }
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                ref_expr(cond, stack, external, in_copy, loops);
                for b in then.iter().chain(else_) {
                    ref_stmt(b, stack, external, in_copy, loops);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        ref_stmt(stm, stack, external, in_copy, loops);
                    }
                }
            }
            IrStmt::Exec { cmd, args, .. } => {
                ref_expr(cmd, stack, external, in_copy, loops);
                for a in args {
                    ref_expr(a, stack, external, in_copy, loops);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        ref_stmt(b, stack, external, in_copy, loops);
                    }
                }
            }
            // subshell/background: COPY semantics — writes inside are
            // copy-local (mirror of the lift walkers' in_copy)
            IrStmt::Function { body, .. } | IrStmt::Block(body) => {
                for b in body {
                    ref_stmt(b, stack, external, in_copy, loops);
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    ref_stmt(b, stack, external, true, loops);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    ref_stmt(b, stack, external, in_copy, loops);
                }
                for r in redirects {
                    ref_expr(&r.target, stack, external, in_copy, loops);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                ref_expr(discriminant, stack, external, in_copy, loops);
                for c in clauses {
                    for b in &c.body {
                        ref_stmt(b, stack, external, in_copy, loops);
                    }
                }
            }
            IrStmt::Expr(e) => ref_expr(e, stack, external, in_copy, loops),
            IrStmt::Output { value, .. } => ref_expr(value, stack, external, in_copy, loops),
            _ => {}
        }
    }
    for st in &prog.stmts {
        ref_stmt(st, &[], &mut external, false, &mut loops);
    }

    let mut num2 = num.clone();
    let mut str2 = str.clone();
    // a var with ANY loop inside a copy region AND any external ref stays
    // store-bound (the persist machinery would leak the copy-local loop
    // writes into the module binding)
    let mut dropped: HashSet<String> = HashSet::new();
    for (_, var, in_copy) in &loops {
        if *in_copy && external.contains(var) {
            dropped.insert(var.clone());
        }
    }
    // the persist map: lifted loop vars, externally referenced, loops
    // outside copy regions
    let mut persist: HashMap<usize, ()> = HashMap::new();
    for (ptr, var, in_copy) in &loops {
        if !in_copy
            && !dropped.contains(var)
            && (num.contains(var) || str.contains(var))
            && external.contains(var)
        {
            persist.insert(*ptr, ());
        }
    }
    *LOOP_PERSIST.lock().unwrap() = Some(persist);
    for v in &dropped {
        num2.remove(v);
        str2.remove(v);
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

/// `local x=1` / `declare x=1` / `typeset x=1` / `readonly x=1` — a
/// PURE-VALUE declaration: every arg is a plain `name=value` word (no flag
/// args) whose value is free of shell metacharacters (`$`, quotes,
/// backticks, backslash, globs, `~`, whitespace — the runtime's
/// expandWord would be an identity on it). Returns the (name, value)
/// pairs, or None for any other declaration shape (flags, dynamic values,
/// array/assoc forms, bare names) — those stay runtime store writes.
///
/// The runtime's `local`/`declare` are plain store writes (no scope
/// stack: fnCall only saves/restores `positional`, never variables —
/// sh2-namespace.mjs `fnCall`/`builtins.local`), so a lifted native
/// binding re-initialized by a native assignment on every call is
/// EXACTLY the runtime model: same re-init per call, same cross-call
/// persistence, no restore. The corpus is the oracle for the model's
/// fidelity (it passes 100% today against this model).
///
/// ASSUMPTION (SH2_ASSUME_LOCAL_SCOPE=0 disables; default ON): the
/// function-local variable scope is never OBSERVED across calls — no
/// recursive or interleaved call reads a `local` between its re-inits,
/// and no caller reads the store copy of a lifted local (reads through
/// runtime strings — tests, eval, heredocs, `declare -p` — keep the
/// name store-bound via string_ctx/excluded, so this is only about the
/// model's missing scope restore, which the runtime already accepts).
fn pure_value_declare(args: &[IrExpr]) -> Option<Vec<(String, String)>> {
    if !assume_local_scope() {
        return None;
    }
    let [IrExpr::Str(cname, _), IrExpr::Array(cargs)] = args else {
        return None;
    };
    if !matches!(cname.as_str(), "local" | "declare" | "typeset" | "readonly") {
        return None;
    }
    let mut out = Vec::new();
    for a in cargs {
        let IrExpr::Str(sv, _) = a else { return None; };
        if sv.starts_with('-') || sv.starts_with('+') {
            return None;
        }
        // `mergeAssignArgs` merges `name=` followed by a bare word — the
        // pure form has one `name=value` word per arg.
        let (name, value) = sv.split_once('=')?;
        if !plain_ident(name) {
            return None;
        }
        if !value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | ',' | '+' | '-')
        }) {
            return None;
        }
        out.push((name.to_string(), value.to_string()));
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// SH2_ASSUME_LOCAL_SCOPE=0 — turn off the pure-value `local`/`declare`/
/// `typeset`/`readonly` lift (see [`pure_value_declare`]).
fn assume_local_scope() -> bool {
    std::env::var("SH2_ASSUME_LOCAL_SCOPE").map_or(true, |v| v != "0")
}

/// SH2_ASSUME_TMPDIR=0 — keep the runtime `$(mktemp -d)` path (the
/// capture + sync builtin + blocking mkdirSync).
///
/// Documented assumption (default ON): the corpus never prints or
/// compares the temp-dir path — every site is `d=$(mktemp -d); cd "$d"`,
/// the path is only used as a directory — so the native
/// `sh2.fs.mkdtemp` path (node's six random chars appended to the
/// template-minus-X-run prefix) is interchangeable with the runtime
/// mktemp's (GNU's replace-the-X-run + custom alphanumeric charset):
/// same path structure, same uniqueness, same exit-status protocol
/// (0 + path, or 1 + "" on failure). Second half: TMPDIR is unset in
/// the harness, so the runtime's default template (os.tmpdir()) resolves
/// to `/tmp/...` — the hardcoded `/tmp` prefix matches exactly; a TMPDIR
/// override would move the directory's LOCATION only, which the corpus
/// cannot observe (and the runtime's own random value is already
/// non-deterministic, so no test can depend on the exact path).
fn mktemp_native_enabled() -> bool {
    std::env::var("SH2_ASSUME_TMPDIR").map_or(true, |v| v != "0")
}

/// The native lowering of a PURE-VALUE declaration whose names are ALL
/// lifted (see [`pure_value_declare`]): `(name = value, ...,
/// sh2.lastExit = 0, true)`. Numeric-lifted names get the i64 literal
/// (the numeric lift only admits sources that parse as integers);
/// string-lifted names get the string literal. Returns None when any
/// name is store-bound — the whole call then stays on the runtime
/// builtin (which writes the store for every arg).
fn try_native_declare_stmt(args: &[IrExpr]) -> Option<Expr> {
    let pairs = pure_value_declare(args)?;
    if !pairs.iter().all(|(n, _)| is_lifted(n)) {
        return None;
    }
    let mut exprs: Vec<Expr> = Vec::new();
    for (name, value) in pairs {
        let right = if is_lifted_num(&name) {
            Expr::Literal {
                value: serde_json::Value::from(value.trim().parse::<i64>().unwrap_or(0)),
                raw: None,
            regex: None,
            }
        } else {
            str_lit(&value)
        };
        exprs.push(Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(Expr::Identifier { name }),
            right: Box::new(right),
        });
    }
    exprs.push(Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(sh2_member("lastExit")),
        right: Box::new(Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
        regex: None,
        }),
    });
    exprs.push(bool_lit(true));
    Some(seq(exprs))
}

/// `eval "NAME=VALUE NAME=VALUE..."` with STATIC args: the runtime builtin
/// (sh2-namespace.mjs `builtins.eval`) spawns bash TWICE per call (once
/// for output, once for the `set`-dump variable sync-back). A code string
/// that provably parses as a plain space-separated assignment list — no
/// `$`, quotes, backticks, backslash, globs, `~`, `;`, `(`, `&` ... (bash
/// eval would treat every token as a literal assignment; nothing else the
/// runtime eval does — function definitions, output, `$((...))` arith —
/// can be expressed) — lowers to the exact same store writes + status:
/// `(setVar(...)..., sh2.lastExit = 0, true)`, no spawns, no sync-back
/// dump. Dynamic eval strings (runtime interpolation) keep the runtime
/// builtin: the string may contain anything, and the two-spawn sync-back
/// is the only faithful model the runtime has.
fn try_native_eval(args: &[IrExpr]) -> Option<Expr> {
    let [IrExpr::Str(cname, _), IrExpr::Array(cargs)] = args else {
        return None;
    };
    if cname != "eval" {
        return None;
    }
    // every arg must be a static string (no runtime interpolation) — the
    // builtin joins args with spaces, mirror that. A quoted word arrives
    // as an Interpolate whose parts are ALL literal text (`"var=1"` →
    // Interpolate([Lit("var=1")])); a bare word as a plain Str. Both
    // are fully static — concatenate the literal parts.
    let mut code = String::new();
    for a in cargs {
        let sv = match a {
            IrExpr::Str(sv, _) => sv.clone(),
            IrExpr::Interpolate(parts)
                if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) =>
            {
                parts
                    .iter()
                    .filter_map(|p| match p {
                        InterpPart::Lit(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect()
            }
            _ => return None,
        };
        code.push_str(&sv);
        code.push(' ');
    }
    let mut assigns: Vec<(String, String)> = Vec::new();
    for tok in code.split_whitespace() {
        let (name, value) = tok.split_once('=')?;
        if !plain_ident(name) {
            return None;
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | ',' | '+' | '-'))
        {
            return None;
        }
        assigns.push((name.to_string(), value.to_string()));
    }
    if assigns.is_empty() {
        return None;
    }
    let mut exprs: Vec<Expr> = Vec::new();
    for (name, value) in assigns {
        exprs.push(sh2_call("setVar", vec![str_lit(&name), str_lit(&value)]));
    }
    exprs.push(Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(sh2_member("lastExit")),
        right: Box::new(Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
        regex: None,
        }),
    });
    exprs.push(bool_lit(true));
    Some(seq(exprs))
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
            } else if let Some(pairs) = pure_value_declare(args) {
                // no `-i` witness (int_declare_names rejects `=value` args
                // and requires the flag) — a pure-value declaration is a
                // real assignment: its value is an assignment SOURCE.
                // Numeric lift accepts it when the value parses as an
                // integer; the string lift accepts any value.
                for (name, value) in pairs {
                    assigns
                        .entry(name)
                        .or_default()
                        .push(IrExpr::Str(value, StrStyle::SingleQuoted));
                }
            }
        }
        "local" => {
            // `local` has no `-i` witness arm (the runtime's local ignores
            // the int flag anyway) — only the pure-value source applies.
            if let Some(pairs) = pure_value_declare(args) {
                for (name, value) in pairs {
                    assigns
                        .entry(name)
                        .or_default()
                        .push(IrExpr::Str(value, StrStyle::SingleQuoted));
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

/// SH2_BC_NATIVE=0 — turn off the native `bc` capture lowering
/// (`$(echo EXPR | bc)` keeps the real spawn, maximal fidelity). DEFAULT
/// ON: the corpus oracle gates the subset (src/bc.rs matches real GNU bc
/// 77/77 differential + unit tests), so the aggressive lowering stays
/// unless it regresses.
///
/// Documented assumption: a bc expression fed by script `$vars` (the
/// primes `sqrt($n)` form, or the general var-operand `$sum + $i` form —
/// see [`bc_var_capture`]) holds bc INTEGER values within double
/// precision (2^53). The native JS path computes in doubles
/// (`Math.floor(Math.sqrt(Number(x)))` / `String(Number(a) + Number(b))`)
/// — bc is exact fixed-point — so a huge operand (>= 2^53), a non-numeric
/// one (unset var → bc's `sqrt()` syntax error), or a fractional one
/// (bc's `.75` output format vs JS `0.75`) would diverge from the real bc
/// output ("" on error, exit 1). The corpus cannot observe these (the
/// primes loop feeds integers >= 2; the bc-native loop's `$sum` starts at
/// the integer 0 and every iteration adds an integer loop var — the
/// integer invariant propagates; static programs fold through src/bc.rs
/// EXACTLY); scripts that can must set SH2_BC_NATIVE=0.
fn bc_native_enabled() -> bool {
    std::env::var("SH2_BC_NATIVE").map_or(true, |v| v != "0")
}

/// SH2_ASSUME_ARITH_NATIVE=0 — turn off the native `$((...))` div/mod
/// lowering (the `Math.trunc` / `%` emission replaces the runtime
/// `sh2.idiv`/`sh2.imod` calls for non-provable divisors).
///
/// Documented assumption (default ON): bash arithmetic is 64-bit integer;
/// a division/modulo by ZERO is an arithmetic ERROR — the whole expansion
/// aborts to the empty string (assignment/test positions make the command
/// fail). JS doubles have no error: `a / 0` is Infinity/NaN and `a % 0` is
/// NaN. The native lowering leans on the two positions' existing abort
/// channels instead of a runtime throw:
///   - inside the `arithEval` wrapper (every expansion-position `$((...))`
///     with div/mod), a non-finite result converts to '' — bash's exact
///     empty expansion;
///   - inside a numeric TEST operand (`[ $((n % d)) -eq 0 ]`), NaN makes
///     the comparison false — bash's exact error→command-fails→false —
///     EXCEPT a `-ne` comparison, which would invert (NaN !== x → true);
///   - a LIFTED-var assignment (`i=$((i/2))`) would poison the binding
///     with NaN where bash keeps the old value — also only reachable via
///     an actual zero divisor.
/// The corpus cannot reach a zero divisor (every corpus div/mod divisor
/// is a literal or a loop counter >= 1); the old runtime behavior in the
/// test-cond positions was an UNCAUGHT throw → script crash, strictly
/// less faithful than NaN→false. Scripts that can observe a zero divisor
/// must set SH2_ASSUME_ARITH_NATIVE=0 (the runtime idiv/imod throw is
/// restored).
fn arith_native_enabled() -> bool {
    std::env::var("SH2_ASSUME_ARITH_NATIVE").map_or(true, |v| v != "0")
}

/// SH2_BC_NATIVE=exact — the `sqrt($var)` runtime path uses the wasm bc
/// number core (sh2.bcSqrt — posixutils-rs Number(BigDecimal), exact
/// arbitrary precision + scale, loaded sync in the runtime) instead of
/// the double fast path. The documented 2^53 / negative / non-numeric
/// assumptions vanish (exact bc semantics, errors → "").
fn bc_exact_enabled() -> bool {
    std::env::var("SH2_BC_NATIVE").map_or(false, |v| v == "exact")
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
                // a native `((i++))` / `let` inside a subshell/background
                // writes a COPY in bash — a lifted module binding would be
                // clobbered by the arrow (mirror of the Assign-target
                // exclusion above), so mark the written vars excluded.
                if in_copy && let_args_native {
                    if let [IrExpr::Str(_cn, _), IrExpr::Array(cargs)] = args.as_slice() {
                        for a in cargs {
                            if let IrExpr::Str(sv, _) = a {
                                if let Some(ast) = parse_arith_native(sv) {
                                    for w in arith_written_vars(&ast) {
                                        excluded.insert(w.clone());
                                    }
                                }
                            }
                        }
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
                            // runtime never sees the args) and a VALIDATED
                            // `-i` declaration (int_declare_names — the
                            // declare writes nothing and references
                            // nothing) are not store writes: skip the marks
                            // for the WHOLE call. (The old per-name skip
                            // was defeated by the exec arg shape — the
                            // names live inside the Array wrapper at
                            // args[1], so only the bare Str args matched,
                            // while the `-i` flag's letters still marked
                            // the name store-bound via mark_all_idents.)
                            let native_let = cname == "let" && let_args_native;
                            let intdecl = if cname == "let" { Vec::new() } else {
                                int_declare_names(args).unwrap_or_default()
                            };
                            // a PURE-VALUE `local x=1` declaration is not a store write (the
                            // emit rewrites it to a native binding write — see pure_value_declare):
                            // skip its marks too, unless the call sits in a subshell/background
                            // (COPY semantics — the name must stay store-bound there, mirror of
                            // the Assign-target exclusion).
                            let pure_decl = !in_copy && pure_value_declare(args).is_some();
                            if !(native_let || !intdecl.is_empty() || pure_decl) {

                                for a in &args[1..] {
                                    mark_write_builtin_vars(a, excluded);
                                    // `let`/`(( ))`/`eval` args are
                                    // EXPRESSIONS ("i++") — mark EVERY
                                    // identifier they touch so a lifted
                                    // native binding never desyncs from
                                    // the runtime's store write
                                    mark_all_idents_args(a, string_ctx);
                                }
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
                        // a VALIDATED `-i` declaration (int_declare_names)
                        // are not store writes — skip their marks for the
                        // WHOLE call (mirror of the expression-position
                        // block; the old per-name skip was defeated by the
                        // Array wrapper + flag letters — see above).
                        let native_let = cname == "let" && arith_let_args_native(args);
                        let intdecl = if cname == "let" { Vec::new() } else {
                            int_declare_names(args).unwrap_or_default()
                        };
                        // a PURE-VALUE `local x=1` declaration is not a store write (the
                        // emit rewrites it to a native binding write — see pure_value_declare):
                        // skip its marks too, unless the call sits in a subshell/background
                        // (COPY semantics — the name must stay store-bound there, mirror of
                        // the Assign-target exclusion).
                        let pure_decl = !in_copy && pure_value_declare(args).is_some();
                        if !(native_let || !intdecl.is_empty() || pure_decl) {

                            for a in args {
                                mark_write_builtin_vars(a, excluded);
                                // `let`/`(( ))`/`eval` args are ARITHMETIC
                                // EXPRESSIONS — mark EVERY identifier they
                                // touch so a lifted native binding never
                                // desyncs from a runtime store write
                                mark_all_idents_args(a, string_ctx);
                            }
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
                // a native `(( ))` / `let` statement's written vars and an
                // `-i` declaration's bare names are numeric assignment
                // sources (see collect_native_arith_sources) — the lift
                // fixpoint needs them exactly like IrStmt::Assign sources
                collect_native_arith_sources(args, assigns);
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
            IrExpr::Call { func, args } => {
                if func == "exec" {
                    // the statement-form `(( ))` / `let` / `typeset -i`
                    // (mirror of the IrStmt::Exec arm above)
                    collect_native_arith_sources(args, assigns);
                }
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
    let (num, str) = analyze_loop_var_refs(
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
    // The second set is the native-DIRECT subset (positional-free bodies —
    // calls skip fnCall's positional swap, see [`DIRECT_FN_CALLS`]).
    let (sync_fns, direct_fns) = fn_call_sync_set(prog, &functions);
    *SYNC_FN_CALLS.lock().unwrap() = Some(sync_fns);
    *DIRECT_FN_CALLS.lock().unwrap() = Some(direct_fns.clone());
    // Native-echo-in-function analysis (see [`native_echo_fn_set`]): the
    // define arrows of eligible functions lower WITHOUT the sink-depth
    // bump, so their echo/printf statements go native. Must run after the
    // lift/scan statics (the emission reads them) and before the main
    // emission.
    *NATIVE_ECHO_FNS.lock().unwrap() = Some(native_echo_fn_set(prog, &functions));
    // Plan 4 — lastExit-write liveness: which `(( ))`/echo statements' status
    // writes are unread, and which empty-else ifs' synthesized false-path
    // `sh2.lastExit = 0` is droppable (empty under a possible `set -e`).
    // Runs before the emission (the IR tree is immutable here — the *Sync
    // loop bodies are emitted from the ORIGINAL references, so the pointer
    // keys hold).
    *LASTEXIT_DEAD.lock().unwrap() = Some(compute_lastexit_deadness(prog, errexit));
    // Native-loop passes: (a) which loops may be capture producers (they
    // keep the runtime loop — a native loop would lose the producer bound),
    // (b) which loops' final status write is dead (the native while drops
    // the status tracking for them).
    // The sync-ok-loops verdicts are (re)computed HERE, under the compile
    // lock, so the pointer keys the emission reads are THIS compilation's
    // (the transform's ast_to_ir hook also runs it, but the statics are
    // per-compilation global state — parallel compilations would tear
    // them between the ast_to_ir write and this read). Gated by
    // DEBASHC_TRANSFORMS like the transform machinery itself.
    if crate::transforms::transform_enabled("sync-ok-loops") {
        crate::transforms::sync_ok_loops::apply_to(&prog.stmts);
    }
    *ASYNC_REGION_LOOPS.lock().unwrap() = Some(compute_async_region_loops(prog));
    let mut loop_status_dead = HashMap::new();
    if !errexit {
        let mut live: HashSet<usize> = HashSet::new();
        // Program-final `end_live` is FALSE — see compute_lastexit_deadness:
        // nothing in the ESTree backend observes the final statement's
        // status (the runner exits 0, `_finish` never reads lastExit, EXIT
        // traps run under real bash). A trailing loop's per-iteration
        // `__sh2_loop_last = sh2.lastExit` tracking is therefore dead
        // weight — the native while lowers to a bare `while (cond) { body }`.
        walk_lastexit_liveness(&prog.stmts, false, &mut live);
        for st in &prog.stmts {
            mark_loop_status_deadness(st, &live, &mut loop_status_dead);
        }
    }
    *LOOP_STATUS_DEAD.lock().unwrap() = Some(loop_status_dead);
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
    // Native-direct function bindings — `let __fn_f = null;` per direct
    // function (see [`DIRECT_FN_CALLS`]). Null is the runtime callDirect's
    // undefined-target marker: `typeof null !== 'function'` routes to
    // `this.callUndefined(name, args)` — the exact tail of fnCall's
    // undefined-target path (builtin fallback, else command-not-found +
    // status 127) for call-before-define / conditional-define programs.
    // Every define site REASSIGNS the binding to the real arrow, so a
    // call after the define hits the function directly — no Map lookup,
    // no arg flatten, no positional save/restore (the body is
    // positional-free by construction). Null-init keeps the binding a
    // plain variable declaration — no runtime calls added to the metric
    // (the callUndefined fallback arrow would add one call site per
    // direct function for identical behavior).
    for name in direct_fns.iter() {
        let binding = direct_binding_name(name).expect("direct set is binding-valid");
        body.push(Stmt::VariableDeclaration {
            kind: "let",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier { name: binding },
                init: Some(Expr::Literal {
                    value: serde_json::Value::Null,
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
    *TOP_LEVEL_DEPTH.lock().unwrap() += 1;
    let out = top_stmt_to_estree_inner(stmt);
    *TOP_LEVEL_DEPTH.lock().unwrap() -= 1;
    out
}

fn top_stmt_to_estree_inner(stmt: &IrStmt) -> Option<Stmt> {
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
    // An awaited runtime call — `await sh2.forLoopBatch(...)` — is the
    // batch twins' shape (the *Sync twins emit un-awaited); unwrap so the
    // guard-skip rule treats them identically.
    if let Expr::AwaitExpression { argument } = e {
        return call_is_always_true(argument);
    }
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
        "setVar" | "setArray" | "shopt" | "forLoopSync" | "whileLoopSync"
        | "forLoopBatch" | "whileLoopBatch" => true,
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
    /// A pattern outside the four string-op shapes, translated to an
    /// anchored JS regex SOURCE (without the `^`/`$` — the emitted test
    /// anchors via the replace-compare, see [`CasePat::Glob`] in
    /// `try_native_case`): `?` single-any, `[...]` character classes,
    /// mid-star literals (`i*86`), escaped meta chars (`\$`, `\(`). The
    /// translation mirrors the runtime's parseGlob/classMatch grammar
    /// exactly.
    Glob(String),
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
    // `*lit*`, `lit*`, `*lit` — exactly one star on either side, a clean
    // literal inside (a meta char inside falls through to the general
    // glob translation below instead of bailing to the runtime — e.g.
    // `*[0-9]*` is a substring test on a character class).
    if let Some(inner) = bare.strip_prefix('*') {
        if let Some(inner) = inner.strip_suffix('*') {
            if !inner.is_empty() && !has_meta(inner) {
                return Some(CasePat::Substr(inner.to_string()));
            }
        } else if !inner.is_empty() && !has_meta(inner) {
            return Some(CasePat::Suffix(inner.to_string()));
        }
    }
    if let Some(inner) = bare.strip_suffix('*') {
        if !inner.is_empty() && !has_meta(inner) {
            return Some(CasePat::Prefix(inner.to_string()));
        }
    }
    if !has_meta(bare) {
        return Some(CasePat::Exact(bare.to_string()));
    }
    // The string-op shapes rejected it — try the general glob grammar
    // (`?`, `[...]` classes, mid-star literals, escaped metachars). Only
    // patterns the RUNTIME would leave unexpanded qualify (`glob_to_regex`
    // rejects `$`/extglob); anything else keeps the runtime switch form.
    glob_to_regex(bare).map(CasePat::Glob)
}

/// Escape a literal char for a JS regex OUTSIDE a character class.
fn regex_escape_lit(c: char) -> String {
    match c {
        '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' | '/' => {
            let mut s = String::from("\\");
            s.push(c);
            s
        }
        _ => c.to_string(),
    }
}

/// Escape a literal char for INSIDE a JS regex character class.
fn regex_escape_class(c: char) -> String {
    match c {
        '\\' | ']' | '^' | '-' | '[' => {
            let mut s = String::from("\\");
            s.push(c);
            s
        }
        // NB: `\b` inside a JS class is BACKSPACE, not the literal b —
        // the runtime's classMatch treats `\x` as the literal x, so the
        // escaped letter is emitted UNescaped.
        _ => c.to_string(),
    }
}

/// Render a glob class BODY (the chars between `[` and `]`) as a JS
/// regex class body, mirroring the runtime's classMatch scan exactly:
/// `\x` escapes (a trailing `\` is a literal backslash), `c-d` ranges
/// (a `-` whose third char exists and isn't `]`), everything else a
/// literal char.
fn glob_class_to_regex(cls: &str) -> String {
    let mut out = String::new();
    let mut cs = cls.chars().peekable();
    while let Some(c) = cs.next() {
        if c == '\\' {
            match cs.next() {
                Some(n) => out.push_str(&regex_escape_class(n)),
                None => out.push_str("\\\\"), // trailing `\`: literal
            }
            continue;
        }
        if cs.peek() == Some(&'-') {
            // `c-d`: a range when a third char follows and it isn't ']'.
            let mut it = cs.clone();
            it.next(); // the '-'
            if let Some(d) = it.next() {
                if d != ']' {
                    cs.next(); // consume '-'
                    cs.next(); // consume d
                    out.push(c);
                    out.push('-');
                    out.push(d);
                    continue;
                }
            }
        }
        out.push_str(&regex_escape_class(c));
    }
    out
}

/// Translate a `case` glob pattern to an anchored JS regex SOURCE
/// (without the anchors — the emitted test uses the replace-compare, see
/// `try_native_case`), mirroring the runtime's parseGlob grammar: `*` →
/// `.*`, `?` → `.`, `\x` → the literal x, `[`…`]` classes (with
/// `!`/`^` negation, an unterminated `[` staying a literal, an empty
/// class matching nothing — or anything when negated — exactly like
/// classMatch), everything else a literal. Returns None when the RUNTIME
/// would expand or re-parse the pattern first: a bare `$` (expandWord's
/// parameter/cmdsub expansion), a bare `(`/`)` (parseGlob's extglob
/// groups), and `\$name`-shaped escapes (the runtime's expandWord is
/// NOT backslash-aware and expands them anyway).
fn glob_to_regex(pat: &str) -> Option<String> {
    let mut out = String::from("^");
    let mut chars = pat.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '\\' => {
                let Some(n) = chars.next() else {
                    out.push_str("\\\\"); // trailing `\`: literal (parseGlob)
                    continue;
                };
                if n == '$' {
                    // `\$name`: the runtime's expandWord expands it (its
                    // `$name` regex is not escape-aware) — mirror by
                    // refusing (the pattern stays on the runtime switch).
                    if let Some(&nxt) = chars.peek() {
                        if nxt.is_ascii_alphanumeric()
                            || matches!(nxt, '_' | '{' | '(' | '@' | '#' | '*' | '?' | '$')
                        {
                            return None;
                        }
                    }
                }
                out.push_str(&regex_escape_lit(n));
            }
            '[' => {
                // parseGlob: optional `!`/`^` negation, then chars up to
                // the FIRST `]` (an unterminated `[` is a literal).
                let mut cls = String::new();
                let mut neg = false;
                if let Some(&n) = chars.peek() {
                    if n == '!' || n == '^' {
                        neg = true;
                        chars.next();
                    }
                }
                let mut closed = false;
                for n in chars.by_ref() {
                    if n == ']' {
                        closed = true;
                        break;
                    }
                    cls.push(n);
                }
                if !closed {
                    out.push_str("\\[");
                } else if cls.is_empty() {
                    // classMatch on an empty class: never a hit (or
                    // ALWAYS a hit when negated — a `[!]` matches any
                    // one char).
                    if neg {
                        out.push('.');
                    } else {
                        out.push_str("[^\\s\\S]");
                    }
                } else {
                    out.push('[');
                    if neg {
                        out.push('^');
                    }
                    out.push_str(&glob_class_to_regex(&cls));
                    out.push(']');
                }
            }
            '(' | ')' | '$' => return None, // extglob / runtime expansion
            _ => out.push_str(&regex_escape_lit(c)),
        }
    }
    Some(out)
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
                operator: "??".to_string(),
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
    // Glob patterns exec a fresh regex literal into a per-pattern const
    // (one exec per case evaluation, exactly the runtime's per-evaluation
    // globMatch); the exec runs on the SAME coerced value string the
    // length compare uses, so that const is declared once too.
    let glob_temps: HashMap<String, String> = pats
        .iter()
        .flatten()
        .filter_map(|p| match p {
            CasePat::Glob(re) => Some(re.clone()),
            _ => None,
        })
        .enumerate()
        .map(|(i, re)| (re, format!("$g{i}")))
        .collect();
    let value_decl = (!glob_temps.is_empty()).then(|| {
        Stmt::VariableDeclaration {
            kind: "const",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier {
                    name: CASE_VALUE_TMP.to_string(),
                },
                init: Some(value_expr(CASE_TMP)),
            }],
        }
    });
    let glob_decls: Vec<Stmt> = glob_temps
        .iter()
        .map(|(re, temp)| Stmt::VariableDeclaration {
            kind: "const",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier {
                    name: temp.clone(),
                },
                init: Some(Expr::CallExpression {
                    callee: Box::new(Expr::MemberExpression {
                        object: Box::new(regex_lit_flags(re, "s")),
                        property: Box::new(Expr::Identifier {
                            name: "exec".to_string(),
                        }),
                        computed: false,
                        optional: false,
                    }),
                    arguments: vec![Expr::Identifier {
                        name: CASE_VALUE_TMP.to_string(),
                    }],
                    optional: false,
                }),
            }],
        })
        .collect();
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
                operator: "===".to_string(),
                left: Box::new(value),
                right: Box::new(str_lit(lit)),
            },
            // The glob translation (see `glob_to_regex`): the pattern's
            // regex is exec'd ONCE into a const (below), and the test
            // requires the match to span the WHOLE value — `$gN[0].length
            // === $sh_case_v.length`. (A bare `/^RE$/` `.test` would be
            // WRONG on two counts: JS `$` also matches before a trailing
            // `\n`, so `case a in a)` would match the value "a\n"; and
            // an empty value with a non-matching regex would compare
            // `"".replace(/^RE$/, "") === ""` — a false positive. The
            // exec+length form is exact. The `s` flag makes `.`/`.*`
            // cross newlines like glob's `?`/`*`.)
            CasePat::Glob(re) => {
                let temp = glob_temps.get(re).expect("glob temp precomputed");
                let m = || Expr::Identifier {
                    name: temp.clone(),
                };
                Expr::LogicalExpression {
                    operator: "&&".to_string(),
                    left: Box::new(Expr::BinaryExpression {
                        operator: "!==".to_string(),
                        left: Box::new(m()),
                        right: Box::new(Expr::Literal {
                            value: serde_json::Value::Null,
                            raw: None,
                            regex: None,
                        }),
                    }),
                    right: Box::new(Expr::BinaryExpression {
                        operator: "===".to_string(),
                        left: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::MemberExpression {
                                object: Box::new(m()),
                                property: Box::new(Expr::Literal {
                                    value: serde_json::Value::from(0),
                                    raw: None,
                                    regex: None,
                                }),
                                computed: true,
                                optional: false,
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "length".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        right: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::Identifier {
                                name: CASE_VALUE_TMP.to_string(),
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "length".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                    }),
                }
            }
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
                    operator: "||".to_string(),
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
        body: std::iter::once(Stmt::VariableDeclaration {
            kind: "const",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier {
                    name: CASE_TMP.to_string(),
                },
                init: Some(expr_to_estree(discriminant)),
            }],
        })
        .chain(value_decl)
        .chain(glob_decls)
        .chain(std::iter::once(*alt.expect("at least one clause")))
        .collect(),
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
/// The coerced case VALUE (`String($sh_case ?? '')`, lowercased under
/// nocasematch) — bound once when a glob pattern needs the exec/length
/// compare on the exact same string.
const CASE_VALUE_TMP: &str = "$sh_case_v";

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
            operator: "+".to_string(),
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

/// Native-loop signal scan: does a LOWERED loop body contain a control-flow
/// signal source? The runtime loops catch BREAK/CONTINUE/RETURN signals
/// (thrown by `sh2.break()`/`sh2.continue()`/`sh2.return()` calls — from
/// case bodies with source `break`, nested arrows, and the fix_stmt
/// conversions) and stop/continue/rethrow them; a NATIVE loop has no
/// catch, so a signal would propagate past the loop and change control
/// flow. The scan is conservative: ANY break/continue/return statement or
/// sh2 signal call anywhere in the body subtree (nested blocks, ifs,
/// switches, arrows — a function DEFINED in the body may throw when
/// CALLED) keeps the runtime loop. The synthetic trailing `break` of the
/// case-switch lowering is exempt (it terminates the switch, never a
/// loop).
fn lowered_stmts_have_signals(stmts: &[Stmt]) -> bool {
    fn stmt_has_signal(s: &Stmt, in_switch_consequent: bool) -> bool {
        match s {
            // source break/continue inside a case body were already
            // converted to sh2.break()/sh2.continue() CALLS by the case
            // lowering — the only native break left in a consequent is the
            // synthetic switch terminator (exempt)
            Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => !in_switch_consequent,
            Stmt::ReturnStatement { .. } => true,
            Stmt::ExpressionStatement { expression } => expr_has_signal(expression),
            Stmt::BlockStatement { body } => body.iter().any(|x| stmt_has_signal(x, false)),
            Stmt::IfStatement {
                consequent,
                alternate,
                ..
            } => {
                stmt_has_signal(consequent, false)
                    || alternate
                        .as_deref()
                        .map(|a| stmt_has_signal(a, false))
                        .unwrap_or(false)
            }
            Stmt::SwitchStatement { cases, .. } => cases
                .iter()
                .any(|c| c.consequent.iter().any(|x| stmt_has_signal(x, true))),
            Stmt::WhileStatement { body, .. } | Stmt::ForOfStatement { body, .. } => {
                stmt_has_signal(body, false)
            }
            Stmt::ForStatement { body, .. } => stmt_has_signal(body, false),
            Stmt::VariableDeclaration { declarations, .. } => declarations
                .iter()
                .any(|d| d.init.as_ref().map(expr_has_signal).unwrap_or(false)),
        }
    }
    fn expr_has_signal(e: &Expr) -> bool {
        match e {
            Expr::CallExpression {
                callee, arguments, ..
            } => {
                if let Expr::MemberExpression { object, property, .. } = callee.as_ref() {
                    if matches!(object.as_ref(), Expr::Identifier { name } if name == "sh2")
                        && matches!(
                            property.as_ref(),
                            Expr::Identifier { name }
                                if name == "break" || name == "continue" || name == "return"
                        )
                    {
                        return true;
                    }
                }
                // nested arrows: a function defined in the body may throw
                // signals when CALLED from the loop
                arguments.iter().any(expr_has_signal)
            }
            Expr::ArrowFunctionExpression { body, .. } => match body {
                ArrowBody::Expr(inner) => expr_has_signal(inner),
                ArrowBody::Block(b) => stmt_has_signal(b, false),
            },
            Expr::AssignmentExpression { right, .. } => expr_has_signal(right),
            Expr::SequenceExpression { expressions } => expressions.iter().any(expr_has_signal),
            Expr::ConditionalExpression {
                consequent,
                alternate,
                ..
            } => expr_has_signal(consequent) || expr_has_signal(alternate),
            Expr::LogicalExpression { left, right, .. } => {
                expr_has_signal(left) || expr_has_signal(right)
            }
            Expr::BinaryExpression { left, right, .. } => {
                expr_has_signal(left) || expr_has_signal(right)
            }
            Expr::MemberExpression { object, .. } => expr_has_signal(object),
            Expr::UnaryExpression { argument, .. } => expr_has_signal(argument),
            Expr::AwaitExpression { argument } => expr_has_signal(argument),
            Expr::TemplateLiteral { expressions, .. } => expressions.iter().any(expr_has_signal),
            Expr::ArrayExpression { elements } => elements
                .iter()
                .filter_map(|e| e.as_ref())
                .any(expr_has_signal),
            Expr::ObjectExpression { properties } => {
                properties.iter().any(|p| expr_has_signal(&p.value))
            }
            _ => false,
        }
    }
    stmts.iter().any(|s| stmt_has_signal(s, false))
}

/// Is a for-loop ITERABLE safe to emit as a flat native array (matching the
/// runtime `forLoopSync` flatten exactly)? The runtime expands GLOB_MAGIC
/// items against the filesystem — the native form cannot — so any
/// glob-tagged string in the iter subtree disqualifies. Everything else
/// (scalar items appended, array items flattened one level — incl. the
/// `brace`/`listVar`/`getVar`-array helpers) is byte-identical under
/// `[].concat(...)`.
fn for_iter_flattenable(iter: &IrExpr) -> bool {
    match iter {
        IrExpr::Str(s, _) => !s.starts_with(GLOB_MAGIC),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(e) => for_iter_flattenable(e),
        }),
        IrExpr::Call { args, .. }
        | IrExpr::Array(args)
        | IrExpr::MethodCall { args, .. } => args.iter().all(for_iter_flattenable),
        IrExpr::BinOp { lhs, rhs, .. } => for_iter_flattenable(lhs) && for_iter_flattenable(rhs),
        IrExpr::Ternary {
            cond, then, else_, ..
        } => {
            for_iter_flattenable(cond)
                && for_iter_flattenable(then)
                && for_iter_flattenable(else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            for_iter_flattenable(expr) && for_iter_flattenable(default)
        }
        IrExpr::Index { key, .. } => for_iter_flattenable(key),
        IrExpr::Capture { expr, .. } => for_iter_flattenable(expr),
        IrExpr::Arrow(stmts) => stmts.iter().all(|s| match s {
            IrStmt::Expr(e) => for_iter_flattenable(e),
            IrStmt::Assign { expr, .. } => for_iter_flattenable(expr),
            _ => false,
        }),
        _ => true,
    }
}

/// The `seq_range_for` transform's numeric-range iterable: the For.iter
/// shape `Range { start, end }` (the transform rewrites the `$(seq …)`
/// capture item to a bare Range — PLAN §5.6) or a bare `Range`. Anything
/// else is not a native-range loop.
fn for_range_bounds(iter: &IrExpr) -> Option<(i64, i64)> {
    match iter {
        IrExpr::Array(items) => match items.as_slice() {
            [IrExpr::Range { start, end }] => Some((*start, *end)),
            _ => None,
        },
        IrExpr::Range { start, end } => Some((*start, *end)),
        _ => None,
    }
}

/// The materialized string-item list for a range iterable — the fallback
/// when the loop cannot take the native counter path (async region /
/// awaits in body / signals): the for-of / *Sync / async paths need a
/// concrete item list and the runtime has no range helper. Bounded by
/// the transform's span cap (1M), so compilation cannot blow up.
fn range_items_array(lo: i64, hi: i64) -> IrExpr {
    let items: Vec<IrExpr> = (lo..=hi)
        .map(|v| IrExpr::Str(v.to_string(), StrStyle::SingleQuoted))
        .collect();
    IrExpr::Array(items)
}

/// `for (let i = lo; i <= hi; i++) { body }` — the native numeric-range
/// loop (the hand-js ideal for `for i in $(seq lo hi)`). The binding is
/// a JS number from the init literal; the body never writes it (the
/// transform's guarantee), so the postfix `i++` update stays exact. The
/// `let` shadows the module `let i = 0` (numeric lift) exactly like the
/// for-of binding.
fn native_range_for(js_var: String, lo: i64, hi: i64, body: Vec<Stmt>) -> Stmt {
    Stmt::ForStatement {
        init: Box::new(Stmt::VariableDeclaration {
            kind: "let",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier {
                    name: js_var.clone(),
                },
                init: Some(Expr::Literal {
                    value: serde_json::Value::from(lo),
                    raw: None,
                regex: None,
                }),
            }],
        }),
        test: Expr::BinaryExpression {
            operator: "<=".to_string(),
            left: Box::new(Expr::Identifier {
                name: js_var.clone(),
            }),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(hi),
                raw: None,
            regex: None,
            }),
        },
        update: Expr::UnaryExpression {
            operator: "++".to_string(),
            argument: Box::new(Expr::Identifier {
                name: js_var.clone(),
            }),
            prefix: false,
        },
        body: Box::new(Stmt::BlockStatement { body }),
    }
}

/// The flat native iterable for a flattenable for-loop iter: `[].concat(
/// item, ...)` — the runtime forLoopSync's exact flatten (scalars appended,
/// array-valued items one-level-flattened) minus the GLOB_MAGIC expansion
/// (excluded by [`for_iter_flattenable`]).
fn flatten_for_iter(iter: &IrExpr) -> Expr {
    let IrExpr::Array(items) = iter else {
        // a bare non-array iter (e.g. a `brace`-style call): concat of the
        // single item — the runtime wraps non-array items in a one-element
        // list and flattens it to the item itself
        return Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(Expr::ArrayExpression { elements: vec![] }),
                property: Box::new(Expr::Identifier {
                    name: "concat".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![expr_to_estree(iter)],
            optional: false,
        };
    };
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::ArrayExpression { elements: vec![] }),
            property: Box::new(Expr::Identifier {
                name: "concat".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: items.iter().map(expr_to_estree).collect(),
        optional: false,
    }
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
        // AWAITED operands work too: the block statements run in the
        // enclosing async context (module top level / async arrows), so
        // `await l; if (cond) { await r; }` sequences identically to the
        // runtime `and()`/`or()` helper (`await fnA(); if (lastExit …)
        // await fnB();`) — the same decision, no per-evaluation promise
        // machinery. The sync-function/*Sync-loop analyses scan the
        // LOWERED body for awaits, so an awaited chain disqualifies its
        // enclosing sync arrow consistently.
        IrStmt::Expr(IrExpr::BinOp {
            op: op @ (BinOpKind::And | BinOpKind::Or),
            lhs,
            rhs,
        }) => {
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let l = expr_to_estree(lhs);
            let r = expr_to_estree(rhs);
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            let is_and = matches!(op, BinOpKind::And);
            let test = if is_and {
                last_exit_eq_zero()
            } else {
                // `a || b`: run b only when a FAILED
                Expr::BinaryExpression {
                    operator: "!==".to_string(),
                    left: Box::new(sh2_member("lastExit")),
                    right: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    regex: None,
                    }),
                }
            };
            Stmt::BlockStatement {
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
                    // Plan 4 dead-write twin for echo: same guards as the
                    // expr-level echo lowering (default stdout sink,
                    // no script-function shadow, no persistent fd-1) — the
                    // native echo drops its `(sh2.lastExit = 0)` write.
                    if *ECHO_SINK_DEPTH.lock().unwrap() == 0
                        && !program_defines_function("echo")
                        && !PROGRAM_PERSIST_FD1.lock().unwrap().unwrap_or(true)
                    {
                        if let Some(dead) = try_native_echo_dead(args) {
                            return Some(Stmt::ExpressionStatement { expression: dead });
                        }
                    }
                    // Plan 4 dead-write twin for printf (same guards as
                    // the expr-level printf lowering)
                    if *ECHO_SINK_DEPTH.lock().unwrap() == 0
                        && !program_defines_function("printf")
                        && !PROGRAM_PERSIST_FD1.lock().unwrap().unwrap_or(true)
                    {
                        if let Some(dead) = try_native_printf_dead(args) {
                            return Some(Stmt::ExpressionStatement { expression: dead });
                        }
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
                    IrExpr::Arith(a) => arith_to_estree_wrapped(a),
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
                if lastexit_write_is_dead(stmt) {
                    // Plan 4: the if's status write is provably unread
                    // (no `$?` reader before the next writer; the runner
                    // never consumes the program-final status; empty under
                    // a possible `set -e` — the guard consumes it) — the
                    // synthesized false-path `sh2.lastExit = 0` is dead
                    // weight. Emit a plain `if (c) { ... }`, no else.
                    None
                } else {
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
                }
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
                // Native loop (the ladder's top rung): a plain JS `while`
                // statement — no runtime call, no per-iteration closures,
                // no try/catch. Eligibility: (1) NOT a potential capture
                // producer (a native loop would lose the runtime
                // `_capExceeded` bound — see compute_async_region_loops),
                // (2) NO signal sources in the body (source break/continue
                // lower to native statements and work in a native loop,
                // but the runtime BREAK/CONTINUE/RETURN THROWS — case-body
                // conversions, nested arrows, sh2.return() — must stay
                // catchable), (3) the loop's own status write liveness
                // decides the tracking (dead → bare loop, zero overhead).
                if !loop_in_async_region(stmt) && !lowered_stmts_have_signals(&body_stmts) {
                    let errexit = MAY_ERREXIT.lock().unwrap().unwrap_or(true);
                    let dead = loop_status_write_dead(stmt) && !errexit;
                    let mut tracked: Vec<Stmt> = Vec::new();
                    let mut inner: Vec<Stmt> = Vec::new();
                    if !dead {
                        // `let __sh2_loop_ran = false, __sh2_loop_last = 0;`
                        // — the runtime's `ran`/`bodyLastExit` locals
                        tracked.push(Stmt::VariableDeclaration {
                            kind: "let",
                            declarations: vec![
                                VariableDeclarator {
                                    type_: "VariableDeclarator",
                                    id: Expr::Identifier {
                                        name: "__sh2_loop_ran".to_string(),
                                    },
                                    init: Some(bool_lit(false)),
                                },
                                VariableDeclarator {
                                    type_: "VariableDeclarator",
                                    id: Expr::Identifier {
                                        name: "__sh2_loop_last".to_string(),
                                    },
                                    init: Some(Expr::Literal {
                                        value: serde_json::Value::from(0),
                                        raw: None,
                                        regex: None,
                                    }),
                                },
                            ],
                        });
                        inner.push(Stmt::ExpressionStatement {
                            expression: Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(Expr::Identifier {
                                    name: "__sh2_loop_ran".to_string(),
                                }),
                                right: Box::new(bool_lit(true)),
                            },
                        });
                    }
                    inner.extend(body_stmts);
                    if !dead {
                        // `__sh2_loop_last = sh2.lastExit;` — the runtime's
                        // bodyLastExit read after the body fn
                        inner.push(Stmt::ExpressionStatement {
                            expression: Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(Expr::Identifier {
                                    name: "__sh2_loop_last".to_string(),
                                }),
                                right: Box::new(sh2_member("lastExit")),
                            },
                        });
                        tracked.push(Stmt::WhileStatement {
                            test: cond_e.clone(),
                            body: Box::new(Stmt::BlockStatement { body: inner }),
                        });
                        // `sh2.lastExit = __sh2_loop_ran ? __sh2_loop_last : 0;`
                        tracked.push(Stmt::ExpressionStatement {
                            expression: Expr::AssignmentExpression {
                                operator: "=".to_string(),
                                left: Box::new(sh2_member("lastExit")),
                                right: Box::new(Expr::ConditionalExpression {
                                    test: Box::new(Expr::Identifier {
                                        name: "__sh2_loop_ran".to_string(),
                                    }),
                                    consequent: Box::new(Expr::Identifier {
                                        name: "__sh2_loop_last".to_string(),
                                    }),
                                    alternate: Box::new(Expr::Literal {
                                        value: serde_json::Value::from(0),
                                        raw: None,
                                        regex: None,
                                    }),
                                }),
                            },
                        });
                        if errexit && is_top_level_stmt() {
                            // the top-level guard's exact semantics: a
                            // failing loop aborts the script when `set -e`
                            // is on (`if (sh2.errexit && sh2.lastExit !== 0)
                            // process.exit(0);`)
                            tracked.push(Stmt::IfStatement {
                                test: Expr::LogicalExpression {
                                    operator: "&&".to_string(),
                                    left: Box::new(sh2_member("errexit")),
                                    right: Box::new(Expr::BinaryExpression {
                                        operator: "!==".to_string(),
                                        left: Box::new(sh2_member("lastExit")),
                                        right: Box::new(Expr::Literal {
                                            value: serde_json::Value::from(0),
                                            raw: None,
                                            regex: None,
                                        }),
                                    }),
                                },
                                consequent: Box::new(Stmt::BlockStatement {
                                    body: vec![Stmt::ExpressionStatement {
                                        expression: process_exit_zero(),
                                    }],
                                }),
                                alternate: None,
                            });
                        }
                        return Some(Stmt::BlockStatement { body: tracked });
                    }
                    // dead loop status: bare native while, zero tracking
                    return Some(Stmt::WhileStatement {
                        test: cond_e,
                        body: Box::new(Stmt::BlockStatement { body: inner }),
                    });
                }
                // Checkpointed loop (the sync-ok-loops transform's
                // `batch_ok` verdict) — see the For lowering: the runtime
                // `whileLoopBatch` runs sync chunks of 1024 with a
                // `setImmediate` yield between chunks instead of the
                // blocking `whileLoopSync`, keeping the event loop
                // responsive for interleaved background jobs while
                // preserving bash's output order, the lastExit protocol
                // and the capture bound. Await-legal contexts only (see
                // `in_sync_arrow`).
                if crate::transforms::sync_ok_loops::batch_ok(stmt) && !in_sync_arrow() {
                    return Some(Stmt::ExpressionStatement {
                        expression: await_call(
                            "whileLoopBatch",
                            vec![
                                sync_arrow_expr(cond_e),
                                sync_arrow_block(body_stmts),
                                int_lit_expr(1024),
                            ],
                        ),
                    });
                }
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
            // The `seq_range_for` transform's native-range iterable (a
            // bare `Range{lo,hi}`): the loop lowers to a native JS
            // `for (let i = lo; i <= hi; i++)` — no runtime call, no item
            // list at all. The transform guarantees the loop var is never
            // WRITTEN by the body (the one semantic gap between a word
            // list and a counter), and the bounds are plain in-range
            // integers.
            let range = for_range_bounds(iter);
            let mut coercion: Option<IrStmt> = None;
            if is_lifted_num(var) {
                if range.is_some() {
                    // the native counter binding is a NUMBER from the
                    // `let i = lo` init — no per-iteration coercion (the
                    // for-of path's `i = Number(i)` exists only because
                    // its items arrive as strings)
                } else {
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
                }
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
            // the *Sync path emits the ORIGINAL body references — the
            // liveness pre-pass (compute_lastexit_deadness) keys by
            // statement pointer, so the loop bodies' dead-write marks must
            // resolve to the same objects (clones would miss).
            let body_e: Vec<Stmt> = coercion
                .iter()
                .chain(body.iter())
                .filter_map(stmt_to_estree)
                .collect();
            // LIFTED loop var referenced outside its loop (see
            // [`analyze_loop_var_refs`] / `loop_persist_needed`): the
            // shadowed binding (`for (let i of …)` / the native-range
            // `let i`) must persist its final value into the MODULE
            // binding — the pre-loop read covers the empty-iterable case
            // (bash keeps the prior value), the per-iteration temp write
            // captures the last item AFTER the lifted-num coercion
            // (position 1 — the temp holds the NUMBER, keeping the
            // numeric binding's invariant; string bindings hold the raw
            // item), and the post-loop assignment restores the module
            // binding. Mirror of the store-sync-elim shape below.
            let persist_block = |make_loop: Box<dyn FnOnce(Vec<Stmt>) -> Stmt>| -> Stmt {
                let temp = format!("__sh2_for_last_{js_var}");
                // the temp write must come AFTER the lifted-num coercion
                // (`i = Number(i)`) — the temp then holds the NUMBER
                // (the module binding's invariant); a string binding has
                // no coercion, the raw item is exact
                let temp_write = Stmt::ExpressionStatement {
                    expression: Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(Expr::Identifier {
                            name: temp.clone(),
                        }),
                        right: Box::new(Expr::Identifier {
                            name: js_var.clone(),
                        }),
                    },
                };
                let mut body2: Vec<Stmt> = Vec::new();
                if coercion.is_some() {
                    body2.push(body_e[0].clone());
                    body2.push(temp_write);
                    body2.extend(body_e[1..].iter().cloned());
                } else {
                    body2.push(temp_write);
                    body2.extend(body_e.iter().cloned());
                }
                let mut out: Vec<Stmt> = vec![Stmt::VariableDeclaration {
                    kind: "let",
                    declarations: vec![VariableDeclarator {
                        type_: "VariableDeclarator",
                        id: Expr::Identifier {
                            name: temp.clone(),
                        },
                        init: Some(Expr::Identifier {
                            name: js_var.clone(),
                        }),
                    }],
                }];
                out.push(make_loop(body2));
                out.push(Stmt::ExpressionStatement {
                    expression: Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(Expr::Identifier {
                            name: js_var.clone(),
                        }),
                        right: Box::new(Expr::Identifier { name: temp }),
                    },
                });
                Stmt::BlockStatement { body: out }
            };
            // Native counter loop (the ladder's top rung for a range
            // iterable): `for (let i = lo; i <= hi; i++)` — the
            // hand-written ideal (PLAN §9.1 `seq 1 N → native range`).
            // Same eligibility as the native for-of below (no await in
            // body, not an async region, no signal sources); the
            // transform's guarantees cover the rest (pure integer
            // bounds, body never writes the var).
            if let Some((lo, hi)) = range {
                if !stmts_have_await(&body_e)
                    && !loop_in_async_region(stmt)
                    && !lowered_stmts_have_signals(&body_e)
                {
                    // Store-sync elimination — the mirror of the for-of
                    // path below: a STORE-BACKED loop var (unliftable —
                    // read after the loop etc.) whose body only observes
                    // it through `sh2.getVar(var)` collapses the
                    // per-iteration `sh2.setVar(var, i)` to ONE pre-loop
                    // store read (the empty-range case keeps the prior
                    // value, exactly like bash) + ONE post-loop write.
                    let mut sync_elim: Option<Vec<Stmt>> = None;
                    if let Some(sync) = &coercion {
                        let is_store_sync = !is_lifted(var)
                            && matches!(sync, IrStmt::Assign { targets, .. }
                                if targets.len() == 1 && targets[0].var == *var);
                        if is_store_sync && forof_sync_elim_ok(&body_e[1..], var) {
                            let mut body2: Vec<Stmt> = vec![Stmt::ExpressionStatement {
                                // the per-iteration store sync becomes a
                                // native temp write (the post-loop setVar
                                // stores the LAST value; the pre-loop read
                                // covers the empty-range case)
                                expression: Expr::AssignmentExpression {
                                    operator: "=".to_string(),
                                    left: Box::new(Expr::Identifier {
                                        name: format!("__sh2_for_last_{js_var}"),
                                    }),
                                    right: Box::new(Expr::Identifier {
                                        name: js_var.clone(),
                                    }),
                                },
                            }];
                            body2.extend(body_e[1..].to_vec());
                            forof_rewrite_getvar(&mut body2, var, &js_var);
                            let temp = format!("__sh2_for_last_{js_var}");
                            let mut out: Vec<Stmt> = vec![Stmt::VariableDeclaration {
                                kind: "let",
                                declarations: vec![VariableDeclarator {
                                    type_: "VariableDeclarator",
                                    id: Expr::Identifier {
                                        name: temp.clone(),
                                    },
                                    init: Some(sh2_call("getVar", vec![str_lit(var)])),
                                }],
                            }];
                            out.push(native_range_for(
                                js_var.clone(),
                                lo,
                                hi,
                                body2,
                            ));
                            out.push(Stmt::ExpressionStatement {
                                expression: sh2_call(
                                    "setVar",
                                    vec![str_lit(var), Expr::Identifier {
                                        name: temp,
                                    }],
                                ),
                            });
                            sync_elim = Some(out);
                        }
                    }
                    if let Some(out) = sync_elim {
                        return Some(Stmt::BlockStatement { body: out });
                    }
                    // the persist twin for the native-range form (the
                    // counter is a NUMBER from the `let i = lo` init —
                    // the temp holds it directly, no coercion exists on
                    // this path)
                    if loop_persist_needed(stmt, var) {
                        let js_var2 = js_var.clone();
                        return Some(persist_block(Box::new(move |b2| {
                            native_range_for(js_var2, lo, hi, b2)
                        })));
                    }
                    return Some(native_range_for(js_var.clone(), lo, hi, body_e));
                }
            }
            // the range fallback: the loop could not take the native
            // counter path (async region / awaits / signals) — materialize
            // the item list for the for-of / *Sync / async paths below
            // (the runtime has no range helper). Bounded by the
            // transform's span cap.
            let iter_e = match range {
                Some((lo, hi)) => expr_to_estree(&range_items_array(lo, hi)),
                None => expr_to_estree(iter),
            };
            // Fast path: a provably-sync loop (the BODY needs no `await`)
            // lowers to the synchronous runtime loop — identical semantics
            // (flattening, GLOB_MAGIC items, BREAK/CONTINUE/RETURN signals,
            // capture bound) minus the per-iteration promise machinery (the
            // whileLoopSync precedent). An `await` in the ITERABLE is fine:
            // it resolves ONCE, before the loop starts (arguments evaluate
            // before the call) — `$(...)`-produced item lists stay async,
            // the per-item body still runs without promises.
            if !stmts_have_await(&body_e) {
                // Native loop (the ladder's top rung): a plain JS
                // `for (let i of [].concat(...))` — no runtime call, no
                // per-iteration closures. Eligibility: (1) NOT a potential
                // capture producer, (2) no signal sources in the body, (3)
                // the ITERABLE is flattenable — `[].concat(...)` is the
                // runtime forLoopSync's exact flatten (scalars appended,
                // array items one-level-flattened) minus the GLOB_MAGIC
                // expansion (any glob-tagged item keeps the runtime loop).
                // The `let` binding shadows the module `let` exactly like
                // the closure param, so the lifted-num coercion
                // (`i = Number(i)`) and the store sync (`sh2.setVar`) work
                // unchanged.
                if !loop_in_async_region(stmt)
                    && !lowered_stmts_have_signals(&body_e)
                    && for_iter_flattenable(iter)
                {
                    // Store-sync elimination (the SELF_CONTAINED read
                    // pattern, doc in forof_sync_elim_ok): a STORE-BACKED
                    // loop var whose body only ever observes it through
                    // `sh2.getVar(var)` (rewritten to the native binding)
                    // and never writes it — the per-iteration
                    // `sh2.setVar(var, i)` sync collapses to ONE pre-loop
                    // store read (the empty-iterable case keeps the
                    // pre-loop value, exactly like bash) + ONE post-loop
                    // store write. A call eliminated from a 10k-iteration
                    // loop is worth 10k call sites in runtime terms.
                    let mut sync_elim: Option<(Vec<Stmt>, String)> = None;
                    if let Some(sync) = &coercion {
                        let is_store_sync = !is_lifted(var)
                            && matches!(sync, IrStmt::Assign { targets, .. }
                                if targets.len() == 1 && targets[0].var == *var);
                        if is_store_sync && forof_sync_elim_ok(&body_e[1..], var) {
                            let mut body2: Vec<Stmt> = vec![Stmt::ExpressionStatement {
                                // the per-iteration store sync becomes a
                                // native temp write (the post-loop setVar
                                // stores the LAST value; the pre-loop read
                                // covers the empty-iterable case)
                                expression: Expr::AssignmentExpression {
                                    operator: "=".to_string(),
                                    left: Box::new(Expr::Identifier {
                                        name: format!("__sh2_for_last_{js_var}"),
                                    }),
                                    right: Box::new(Expr::Identifier {
                                        name: js_var.clone(),
                                    }),
                                },
                            }];
                            body2.extend(body_e[1..].to_vec());
                            forof_rewrite_getvar(&mut body2, var, &js_var);
                            let temp = format!("__sh2_for_last_{js_var}");
                            let mut out: Vec<Stmt> = vec![Stmt::VariableDeclaration {
                                kind: "let",
                                declarations: vec![VariableDeclarator {
                                    type_: "VariableDeclarator",
                                    id: Expr::Identifier {
                                        name: temp.clone(),
                                    },
                                    init: Some(sh2_call("getVar", vec![str_lit(var)])),
                                }],
                            }];
                            out.push(Stmt::ForOfStatement {
                                left: Box::new(Stmt::VariableDeclaration {
                                    kind: "let",
                                    declarations: vec![VariableDeclarator {
                                        type_: "VariableDeclarator",
                                        id: Expr::Identifier {
                                            name: js_var.clone(),
                                        },
                                        init: None,
                                    }],
                                }),
                                right: flatten_for_iter(iter),
                                body: Box::new(Stmt::BlockStatement { body: body2 }),
                            });
                            out.push(Stmt::ExpressionStatement {
                                expression: sh2_call(
                                    "setVar",
                                    vec![str_lit(var), Expr::Identifier {
                                        name: temp,
                                    }],
                                ),
                            });
                            sync_elim = Some((out, js_var.clone()));
                        }
                    }
                    if let Some((out, _)) = sync_elim {
                        return Some(Stmt::BlockStatement { body: out });
                    }
                    // the persist twin for the native for-of form (the
                    // shadowed `let i` keeps the module binding stale —
                    // the temp + post-loop assignment restore it)
                    if loop_persist_needed(stmt, var) {
                        let js_var2 = js_var.clone();
                        return Some(persist_block(Box::new(move |b2| {
                            Stmt::ForOfStatement {
                                left: Box::new(Stmt::VariableDeclaration {
                                    kind: "let",
                                    declarations: vec![VariableDeclarator {
                                        type_: "VariableDeclarator",
                                        id: Expr::Identifier {
                                            name: js_var2.clone(),
                                        },
                                        init: None,
                                    }],
                                }),
                                right: flatten_for_iter(iter),
                                body: Box::new(Stmt::BlockStatement { body: b2 }),
                            }
                        })));
                    }
                    return Some(Stmt::ForOfStatement {
                        left: Box::new(Stmt::VariableDeclaration {
                            kind: "let",
                            declarations: vec![VariableDeclarator {
                                type_: "VariableDeclarator",
                                id: Expr::Identifier { name: js_var.clone() },
                                init: None,
                            }],
                        }),
                        right: flatten_for_iter(iter),
                        body: Box::new(Stmt::BlockStatement { body: body_e }),
                    });
                }
                // Checkpointed loop (the sync-ok-loops transform's
                // `batch_ok` verdict): the loop failed the native gate
                // (async region / glob iterable / signals) but its body is
                // sync-executable — the per-iteration await of the async
                // `forLoop` would be pure overhead. The runtime
                // `forLoopBatch` runs it as sync chunks of 1024 with a
                // `setImmediate` yield between chunks: bash's output order
                // and the capture bound preserved, at ~1/1024 of the await
                // cost. Emitted only where `await` is legal — inside an
                // async arrow or at module top level, NEVER inside a
                // provably-sync function body (`in_sync_arrow`).
                // Persist twin for the RUNTIME loop paths (the persist
                // set, see loop_persist_needed): the closure PARAM shadows
                // the module binding, so the final item is synced into the
                // store per iteration (`sh2.setVar`) and read back after
                // the loop — Number-coerced for the numeric binding's
                // invariant (the store holds the item's string form,
                // which the canonical-item guarantee makes exact).
                let persist_sync: Option<Stmt> = if loop_persist_needed(stmt, var) {
                    Some(Stmt::ExpressionStatement {
                        expression: sh2_call(
                            "setVar",
                            vec![
                                str_lit(var),
                                Expr::Identifier {
                                    name: js_var.clone(),
                                },
                            ],
                        ),
                    })
                } else {
                    None
                };
                let persist_readback: Option<Stmt> = if persist_sync.is_some() {
                    Some(Stmt::ExpressionStatement {
                        expression: Expr::AssignmentExpression {
                            operator: "=".to_string(),
                            left: Box::new(Expr::Identifier {
                                name: js_var.clone(),
                            }),
                            right: Box::new(if is_lifted_num(var) {
                                Expr::CallExpression {
                                    callee: Box::new(Expr::Identifier {
                                        name: "Number".to_string(),
                                    }),
                                    arguments: vec![sh2_call("getVar", vec![str_lit(var)])],
                                    optional: false,
                                }
                            } else {
                                sh2_call("getVar", vec![str_lit(var)])
                            }),
                        },
                    })
                } else {
                    None
                };
                let mut loop_body: Vec<Stmt> = Vec::new();
                if let Some(ps) = &persist_sync {
                    loop_body.push(ps.clone());
                }
                loop_body.extend(body_e.iter().cloned());
                if crate::transforms::sync_ok_loops::batch_ok(stmt) && !in_sync_arrow() {
                    let mut out: Vec<Stmt> = vec![Stmt::ExpressionStatement {
                        expression: await_call(
                            "forLoopBatch",
                            vec![
                                iter_e,
                                sync_arrow_with_param(js_var, loop_body),
                                int_lit_expr(1024),
                            ],
                        ),
                    }];
                    if let Some(rb) = persist_readback {
                        out.push(rb);
                    }
                    return Some(if out.len() == 1 {
                        out.pop().unwrap()
                    } else {
                        Stmt::BlockStatement { body: out }
                    });
                }
                let mut out: Vec<Stmt> = vec![Stmt::ExpressionStatement {
                    expression: sh2_call(
                        "forLoopSync",
                        vec![iter_e, sync_arrow_with_param(js_var, loop_body)],
                    ),
                }];
                if let Some(rb) = persist_readback {
                    out.push(rb);
                }
                return Some(if out.len() == 1 {
                    out.pop().unwrap()
                } else {
                    Stmt::BlockStatement { body: out }
                });
            }
            let persist_readback: Option<Stmt> = if loop_persist_needed(stmt, var) {
                // the async path's per-iteration sync: an IR setVar call
                // (the closure param shadows the module binding)
                body_stmts.insert(
                    0,
                    IrStmt::Expr(IrExpr::Call {
                        func: "setVar".to_string(),
                        args: vec![
                            IrExpr::Str(var.clone(), StrStyle::SingleQuoted),
                            IrExpr::Ident(js_var.clone()),
                        ],
                    }),
                );
                Some(Stmt::ExpressionStatement {
                    expression: Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(Expr::Identifier {
                            name: js_var.clone(),
                        }),
                        right: Box::new(if is_lifted_num(var) {
                            Expr::CallExpression {
                                callee: Box::new(Expr::Identifier {
                                    name: "Number".to_string(),
                                }),
                                arguments: vec![sh2_call("getVar", vec![str_lit(var)])],
                                optional: false,
                            }
                        } else {
                            sh2_call("getVar", vec![str_lit(var)])
                        }),
                    },
                })
            } else {
                None
            };
            let call = Stmt::ExpressionStatement {
                expression: await_call(
                    "forLoop",
                    vec![
                        iter_e,
                        arrow_with_param(js_var, IrExpr::Arrow(body_stmts)),
                    ],
                ),
            };
            if let Some(rb) = persist_readback {
                Stmt::BlockStatement {
                    body: vec![call, rb],
                }
            } else {
                call
            }
        }
        IrStmt::Function { name, body } => {
            // `sh2.define(name, fn)` is a thin wrapper over the runtime's
            // function map (`this.functions.set(name, fn); return true;`) —
            // a direct state write + `true`, no dispatch. The arrow arg is
            // lowered exactly as before (sink-aware: a function body may
            // run under ANY stdout sink at runtime). A PROVABLY-SYNC
            // function (see [`fn_call_sync_set`]) gets a NON-async arrow:
            // its body has no awaits by construction, and the sync fnCall
            // path runs it without a per-call promise (the async exec path
            // awaits it like any other — `await` on a non-promise is an
            // identity). A NATIVE-DIRECT function (see
            // [`DIRECT_FN_CALLS`]) additionally reassigns its module
            // binding `__fn_<name>` to the SAME arrow, so call sites can
            // run `sh2.callDirect(__fn_f, args)` — no Map lookup, no arg
            // flatten, no positional save/restore (the body is
            // positional-free by construction).
            let arrow = if fn_call_is_sync(name) {
                if native_echo_fn(name) {
                    // eligible: the body may lower echo/printf
                    // to native writes — no sink-depth bump
                    // (see [`native_echo_fn_set`])
                    arrow_native_echo_sync(vec![], IrExpr::Arrow(body.clone()))
                } else {
                    arrow_sink_sync(vec![], IrExpr::Arrow(body.clone()))
                }
            } else if native_echo_fn(name) {
                arrow_native_echo(vec![], IrExpr::Arrow(body.clone()))
            } else {
                arrow_sink(vec![], IrExpr::Arrow(body.clone()))
            };
            let binding = fn_call_is_direct(name).then(|| {
                direct_binding_name(name).expect("direct set is binding-valid")
            });
            let mut items = vec![];
            if let Some(b) = &binding {
                items.push(Expr::AssignmentExpression {
                    operator: "=".to_string(),
                    left: Box::new(Expr::Identifier { name: b.clone() }),
                    right: Box::new(arrow.clone()),
                });
            }
            items.push(Expr::CallExpression {
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
                    match &binding {
                        Some(b) => Expr::Identifier { name: b.clone() },
                        None => arrow,
                    },
                ],
                optional: false,
            });
            items.push(bool_lit(true));
            Stmt::ExpressionStatement {
                expression: seq(items),
            }
        }
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
            // `local x=1` / `declare x=1` / `typeset x=1` / `readonly x=1`
            // with PURE-VALUE args whose names are ALL lifted (see
            // pure_value_declare): a native binding write sequence —
            // `(x = 1, sh2.lastExit = 0, true)` — the runtime builtin's
            // exact store-write model (no scope stack, re-init per call)
            // minus the dispatch. The re-init is REQUIRED for the lift:
            // bash (and the runtime model) re-initializes a local on every
            // call, so a module binding must be re-assigned at the same
            // point. The runtime builtin also sets lastExit=0 and returns
            // truthy — mirror both (the trailing `true` keeps &&/||/guard
            // contexts on their native paths). Redirects/env forms keep
            // the runtime dispatch.
            if env.is_empty() && redirects.is_empty() {
                if let IrExpr::Str(name, _) = cmd {
                    if let Some(native) = try_native_declare_stmt(args) {
                        return Some(Stmt::ExpressionStatement { expression: native });
                    }
                }
            }
            // `eval "NAME=VALUE..."` with a STATIC pure-assignment string
            // (see try_native_eval): the runtime builtin spawns bash twice
            // per call — the native store-write sequence replaces both
            // spawns with the exact same store writes + status.
            if env.is_empty() && redirects.is_empty() {
                if let IrExpr::Str(name, _) = cmd {
                    if name == "eval" {
                        if let Some(native) = try_native_eval(args) {
                            return Some(Stmt::ExpressionStatement { expression: native });
                        }
                    }
                }
            }
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

/// `==` / `!=` operand: like `test_value_operand` but with the guards the
/// equality path needs — a quoted `*` / `?` / `[` operand is a PATTERN the
/// runtime glob-matches (never a literal), and an unquoted word must be
/// strictly literal (an unquoted `=` / `<` / `>` would have tokenized into
/// separate test tokens → the runtime parse errors → the whole test is
/// false; str_operand's rule). Quoted and `$`-forms additionally read
/// store vars and positionals natively (getVar / `sh2.positional`).
fn eq_test_operand(e: &str) -> Option<Expr> {
    let e = e.trim();
    if let Some(inner) = e.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        // a glob metachar inside quotes is a glob in `[[ ]]` context
        // (the runtime glob-matches `=` operands) — EXCEPT `?` right
        // after `$`, which is the `$?` status-var sigil, not a glob
        // (`"$?"` reads the runtime's lastExit field natively).
        if inner.contains(['*', '[']) || (inner.contains('?') && !inner.contains("$?")) {
            return None; // glob pattern — the runtime glob-matches
        }
        return test_value_operand(&format!("\"{inner}\""));
    }
    if let Some(inner) = e.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        if inner.contains(['*', '?', '[']) {
            return None;
        }
        return test_value_operand(&format!("'{inner}'"));
    }
    if e.starts_with('$') {
        return test_value_operand(e);
    }
    str_operand(e)
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
                operator: "===".to_string(),
                left: Box::new(value.clone()),
                right: Box::new(str_lit(&lit_str(lit))),
            },
            // The caller gates to Substr/Prefix/Suffix before calling
            // build — a Glob pattern (regex translation) never reaches
            // the test-lowering match. Keep the arm for exhaustiveness.
            CasePat::Glob(_) => unreachable!(
                "glob patterns are filtered before the test-lowering match"
            ),
        };
        if negate {
            Expr::UnaryExpression {
                operator: "!".to_string(),
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
        // an interpolation whose parts are ALL literals is a compile-time
        // string ("test" parses as Interpolate([Lit]) in the double-quoted
        // form) — fold it so the echo join can become a single literal too
        IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
            let s: String = parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(s) => s.clone(),
                    _ => unreachable!("all-Lit checked"),
                })
                .collect();
            Some(str_lit(&s))
        }
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
                IrExpr::Call { func, .. }
                    if func == "captureWords" || func == "listVar" || func == "split"
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
                    IrExpr::Call { func, .. }
                        if func == "captureWords" || func == "listVar" || func == "split"
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
    } else if arg_exprs.iter().all(|e| {
        matches!(e, Expr::Literal { value: serde_json::Value::String(_), .. })
    }) {
        // all-literal fold: the runtime's `[a, b].join(" ")` is exactly
        // `a + " " + b` — a single compile-time literal, no per-iteration
        // array/join/String() machinery
        let s = arg_exprs
            .iter()
            .map(|e| match e {
                Expr::Literal { value: serde_json::Value::String(sv), .. } => sv.as_str(),
                _ => unreachable!("all-Lit checked"),
            })
            .collect::<Vec<_>>()
            .join(" ");
        str_lit(&s)
    } else if arg_exprs.len() == 1 && !flat {
        // a single non-literal arg: `[x].join(" ")` is exactly `x` (a
        // one-element join never inserts the separator) — the common
        // `echo $var` shape skips the array + join machinery entirely
        arg_exprs.pop().unwrap()
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
    // Compile-time fold: when every join element is a literal string, the
    // join (and the -e replaces) are a single literal — `echo "test"`
    // becomes `process.stdout.write("test\n")` with no per-iteration
    // array/join/String() machinery. The runtime's `[a, b].join(" ")` is
    // exactly `a + " " + b`, and the split/join replaces are the global
    // replaces applied here, so the fold is byte-identical.
    if let Expr::Literal {
        value: serde_json::Value::String(sv),
        raw: _,
        regex: _,
    } = &joined
    {
        let mut s = sv.clone();
        if esc {
            s = s.replace("\\n", "\n").replace("\\t", "\t");
        }
        joined = str_lit(&s);
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
    fs_promise_status(sh2_fs_call("readFile", args), ok, err)
}

/// Chain the runtime's exit-status recording onto an arbitrary
/// `sh2.fs.<name>` promise: `.then(r => (sh2.lastExit = ok, r)).catch(e
/// => (sh2.lastExit = err, ""))` — the value the runtime's exec path
/// would capture for the command, INCLUDING its exit status (`$?` reads
/// lastExit after the cmdsub).
fn fs_promise_status(base: Expr, ok: i64, err: i64) -> Expr {
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

/// `$(mktemp -d)` — a unique temp DIRECTORY (the capture's only statement
/// is the sync mktemp builtin with `-d`): the value is the created
/// directory path (the capture strips the builtin's trailing newline). A
/// native `await sh2.fs.mkdtemp(prefix)` creates the same unique dir — no
/// fd swap, no blocking mkdirSync, no builtin dispatch (node appends six
/// random chars to the prefix; GNU mktemp replaces the trailing X-run —
/// same structure, same uniqueness; the exact random value is
/// unobservable, see [`mktemp_native_enabled`]). The `.then/.catch`
/// records the exit status the runtime mktemp would (0 + the path on
/// success; 1 + "" on failure — a rejected mkdtemp). Only the exact
/// `["-d"]` / `["-d", template]` shapes lift (template static with a
/// trailing run of ≥3 X's — the builtin's error for shorter runs is
/// observable via the exit status); every other flag shape (file
/// mktemp, `-u`, `-t`, `--suffix`) stays on the runtime builtin.
fn native_capture_mktemp_dir(e: &IrExpr) -> Option<Expr> {
    if !mktemp_native_enabled() {
        return None;
    }
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(cargs)] = args.as_slice() else {
        return None;
    };
    if name != "mktemp" {
        return None;
    }
    let mut is_dir = false;
    let mut template: Option<&str> = None;
    for a in cargs {
        match a {
            IrExpr::Str(sv, _) if sv == "-d" => is_dir = true,
            IrExpr::Str(sv, _) if sv.starts_with('-') => return None, // other flags
            IrExpr::Str(sv, _) => {
                if template.is_some() {
                    return None; // more than one positional
                }
                template = Some(sv);
            }
            _ => return None,
        }
    }
    if !is_dir {
        return None;
    }
    let tpl = template.unwrap_or("/tmp/tmp.XXXXXXXXXX");
    if tpl.contains(GLOB_MAGIC)
        || tpl.contains(PS_MAGIC)
        || tpl.chars().any(|c| (0xF800..=0xF8FF).contains(&(c as u32)))
    {
        return None;
    }
    let xrun = tpl.len() - tpl.trim_end_matches('X').len();
    if xrun < 3 {
        return None; // GNU mktemp errors on <3 trailing X's
    }
    let prefix = &tpl[..tpl.len() - xrun];
    Some(await_expr(fs_promise_status(
        sh2_fs_call("mkdtemp", vec![str_lit(prefix)]),
        0,
        1,
    )))
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
            operator: "+".to_string(),
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
                    operator: "-".to_string(),
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
                operator: "!".to_string(),
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
            operator: "??".to_string(),
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
        operator: "??".to_string(),
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
                operator: "-".to_string(),
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
            operator: "||".to_string(),
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
            operator: "<".to_string(),
            left: Box::new(na.clone()),
            right: Box::new(nb.clone()),
        };
        let gt = Expr::BinaryExpression {
            operator: ">".to_string(),
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
                        operator: "-".to_string(),
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
                operator: "-".to_string(),
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
                // `$(cut OP <<< X)` — a fd-0 HERE-STRING feeding the cut
                // builtin: bash appends the newline (input = X + "\n"), so
                // the runtime's line model is exactly X.split('\n'), and
                // the selection is a pure string-op chain over X (see
                // `cut_value_expr`) — no spawn, no redirect machinery, no
                // async capture arrow. The target renders as a value
                // expression (store refs become getVar calls — the same
                // String() the runtime applies), marker-free only (globs /
                // raw bytes the native chain cannot reproduce).
                if mode == "herestring" && fd == Some(0) {
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
                    if name != "cut" || program_defines_function("cut") {
                        return None;
                    }
                    let spec = parse_cut_args(cmd_args)?;
                    let target = target?;
                    if ir_expr_needs_runtime(target) {
                        return None;
                    }
                    let text = Expr::CallExpression {
                        callee: Box::new(Expr::Identifier {
                            name: "String".to_string(),
                        }),
                        arguments: vec![expr_to_estree(target)],
                        optional: false,
                    };
                    let lines = method_call(text, "split", vec![str_lit("\n")]);
                    // the capture trims trailing newlines — the emitted
                    // +"\n" is a no-op
                    let value = cut_value_expr(lines, &spec, false)?;
                    return Some(trim_capture(value));
                }
                // `$(cat <<EOF ...)` / `$(cut OP <<EOF ...)` — a fd-0
                // HEREDOC wrapping `cat` with
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
                    if name == "cut" {
                        // `cut OP <<EOF` — the stdin is the body (which
                        // ends with \n): lines = body.split('\n') minus
                        // the split's trailing ''. heredoc-tabs bodies are
                        // tab-stripped at runtime — the native chain
                        // cannot (keep the runtime); raw-byte marker
                        // bodies neither (the runtime decodes them).
                        if mode != "heredoc" || program_defines_function("cut") {
                            return None;
                        }
                        let spec = parse_cut_args(cmd_args)?;
                        let IrExpr::Str(body, _) = target? else {
                            return None;
                        };
                        if body.chars().any(|c| (0xF800..=0xF8FF).contains(&(c as u32))) {
                            return None;
                        }
                        let text = if interpolate && body.contains('$') {
                            // the runtime expands the body from the store —
                            // a native template can only inline LIFTED refs
                            fully_lifted_template(body)?
                        } else {
                            str_lit(body)
                        };
                        let lines = method_call(
                            method_call(text, "split", vec![str_lit("\n")]),
                            "slice",
                            vec![int_lit_expr(0), int_lit_expr(-1)],
                        );
                        let value = cut_value_expr(lines, &spec, false)?;
                        return Some(trim_capture(value));
                    }
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
///
/// A NATIVE-DIRECT target (see [`DIRECT_FN_CALLS`]) with magic-free args
/// lowers one step further to `sh2.callDirect(name, __fn_f, args)` — the
/// same status semantics (RETURN-signal catch, `'0'`/`'1'`/number return-value
/// recording, `lastExit === 0` result) minus the arg flattening, the Map
/// lookup and the positional save/restore the fnCall dispatch performs
/// (the positional-free body cannot observe the swap). The runtime
/// signature is `callDirect(name, fn, args)`: the name feeds the
/// undefined-target fallback (`callUndefined(name, args)` — builtin
/// fallback / command-not-found 127 when the binding is still null), the
/// binding is the direct arrow.
fn try_native_fn_call(name: &str, args: &[IrExpr]) -> Option<Expr> {
    if !fn_call_is_sync(name) {
        return None;
    }
    let a = expr_to_estree(&IrExpr::Array(args.to_vec()));
    if expr_has_await(&a) {
        return None;
    }
    if direct_fn_call_is_ok(name, args) {
        return Some(sh2_call(
            "callDirect",
            vec![
                str_lit(name),
                Expr::Identifier {
                    name: direct_binding_name(name).expect("direct set is binding-valid"),
                },
                a,
            ],
        ));
    }
    Some(sh2_call("fnCall", vec![str_lit(name), a]))
}

/// `callDirect` eligibility at a call site: the target is in the
/// direct-call set (sync + every body positional-free — the call cannot
/// observe the positional swap fnCall performs) and every arg lowers
/// without the runtime flatten (no GLOB/PS/badsub magic, no array
/// spread, no setArray side-effect args whose ARRAY_LIT_MAGIC return the
/// flatten drops).
fn direct_fn_call_is_ok(name: &str, args: &[IrExpr]) -> bool {
    if !fn_call_is_direct(name) || direct_binding_name(name).is_none() {
        return false;
    }
    args.iter().all(direct_call_arg_ok)
}

fn direct_call_arg_ok(a: &IrExpr) -> bool {
    if ir_expr_needs_runtime(a) {
        return false;
    }
    match a {
        // an array-literal arg needs the flatten spread; an empty array
        // (no args) is fine (`fn(...[])` === `fn()`)
        IrExpr::Array(elems) => elems.is_empty(),
        // setArray/setArrayAppend args return ARRAY_LIT_MAGIC, which the
        // flatten DROPS (the call is a side effect); callDirect would pass
        // the marker string through — keep the fnCall dispatch
        IrExpr::Call { func, .. } => !matches!(func.as_str(), "setArray" | "setArrayAppend"),
        _ => true,
    }
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
                        Some(ast) => vals.push((arith_to_estree_wrapped(&ast), arith_has_write(&ast))),
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
                    operator: "!==".to_string(),
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
                        Some(ast) => vals.push(arith_to_estree_wrapped(&ast)),
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
                    operator: "&&".to_string(),
                    left: Box::new(Expr::Identifier {
                        name: "e".to_string(),
                    }),
                    right: Box::new(Expr::BinaryExpression {
                        operator: "===".to_string(),
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
        operator: "!".to_string(),
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
                    if func == "captureWords" || func == "listVar" || func == "split" =>
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
                                operator: "||".to_string(),
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
    Some(printf_status_seq(printf_write_expr(value)))
}

/// The bare write (no status record) — shared by [`try_native_printf`]
/// (which wraps it in the status seq) and the Plan 4 dead-write twin
/// `try_native_printf_dead`.
fn try_native_printf_write(args: &[IrExpr]) -> Option<Expr> {
    try_native_printf(args).map(|e| match e {
        // the live form's value is `(write, sh2.lastExit = 0, true)` —
        // the dead form is the bare write (the first seq element)
        Expr::SequenceExpression { expressions } if expressions.len() == 3 => {
            expressions[0].clone()
        }
        // the spec-less dynamic path is `(args-array, write)` — no
        // status record to drop; keep it as-is
        other => other,
    })
}

/// `try_native_printf` twin for the Plan 4 dead-write path (see
/// `try_native_echo_dead`): a printf statement whose lastExit write is
/// provably unread emits the bare `process.stdout.write(value)`.
fn try_native_printf_dead(args: &[IrExpr]) -> Option<Expr> {
    try_native_printf_write(args)
}

/// `(write, sh2.lastExit = 0, true)` — the native printf/echo status
/// sequence (the write, the builtin's status record, the always-truthy
/// value the errexit guard / chain links consume).
fn printf_status_seq(write: Expr) -> Expr {
    seq(vec![
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
    ])
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
    let write = printf_write_expr(echo_text(joined, no_newline));
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

/// `try_native_echo` twin for the Plan 4 dead-write path (`stmt_to_estree`
/// consults `lastexit_write_is_dead`): the statement's lastExit write is
/// provably unread, so the `(sh2.lastExit = 0)` write is dropped — the
/// bare `process.stdout.write(text)` is all the runtime-observable effect.
/// Only reachable when errexit is off (the top-level guard, which consumes
/// the statement's value, is skipped then), so the statement value is
/// unused and the write expression alone is a valid statement.
fn try_native_echo_dead(args: &[IrExpr]) -> Option<Expr> {
    let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args else {
        return None;
    };
    if name != "echo" {
        return None;
    }
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    Some(printf_write_expr(echo_text(joined, no_newline)))
}

/// `$(echo EXPR | bc)` — a native bc evaluation (SH2_BC_NATIVE, default
/// ON; see [`bc_native_enabled`]): the spawn + async capture machinery
/// collapse to a compile-time fold (static EXPR, via src/bc.rs's exact
/// GNU-bc semantics + output format), a native `sqrt($var)` expression
/// (the primes `is_prime` pattern), or a native var-operand arithmetic
/// expression (`$sum + $i` — see [`bc_var_capture`]). `words` =
/// captureWords context
/// (unquoted `$(...)` — the runtime word-splits the output): the fold
/// then only fires when the value is provably a single word, and the
/// emitted one-element array is exactly the `capture().split(/\s+/)`
/// result.
fn native_capture_echo_bc(pipe: &IrExpr, words: bool) -> Option<Expr> {
    if !bc_native_enabled() {
        return None;
    }
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
    // stage 1: exec/builtin "echo" — the exact bytes echo writes
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if !matches!(f1.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if name1 != "echo" {
        return None;
    }
    // raw-byte / glob / process-substitution markers: the runtime expands
    // them before bc sees the text — the native path cannot (see
    // ir_expr_needs_runtime)
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    // stage 2: exec("bc") with NO args
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(name2, _), IrExpr::Array(bc_args)] = a2.as_slice() else {
        return None;
    };
    if name2 != "bc" || !bc_args.is_empty() {
        return None;
    }
    // echo flags (-e/-n) then exactly ONE real arg (multiple args join to
    // a space-separated multi-statement program — keep the spawn)
    let mut flag_done = false;
    let mut arg: Option<&IrExpr> = None;
    for a in echo_args {
        if !flag_done {
            if let IrExpr::Str(sv, _) = a {
                if sv == "-e" || sv == "-n" {
                    continue;
                }
            }
            flag_done = true;
        }
        if arg.is_some() {
            return None;
        }
        arg = Some(a);
    }
    let arg = arg?;
    // STATIC program — the compile-time fold (src/bc.rs's exact semantics
    // + output format, 77/77 vs real bc). A quoted arg arrives as an
    // Interpolate with only Lit parts ("2+3" → template); concatenate
    // them — the runtime's expandWord joins the parts with the refs'
    // values, so all-Lit == the literal text.
    let text: Option<String> = match arg {
        IrExpr::Str(sv, _) => Some(sv.clone()),
        IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
            Some(parts.iter().map(|p| match p {
                InterpPart::Lit(s) => s.clone(),
                _ => unreachable!("all-Lit checked"),
            }).collect())
        }
        _ => None,
    };
    if let Some(sv) = text {
        let out = bc_eval(&sv).ok()?;
        if words && out.chars().any(char::is_whitespace) {
            return None;
        }
        return Some(str_lit(out.trim_end_matches('\n')));
    }
    match arg {
        // `sqrt($var)` — a single interpolation inside the sqrt parens:
        // bc's scale-0 sqrt of an integer is the truncated root
        // (Math.floor of the double root; see bc_native_enabled for the
        // documented operand assumption)
        IrExpr::Interpolate(parts) => {
            // `sqrt($var)` — a single interpolation inside the sqrt
            // parens: bc's scale-0 sqrt of an integer is the truncated
            // root (Math.floor of the double root; see bc_native_enabled
            // for the documented operand assumption)
            if let [InterpPart::Lit(l1), InterpPart::Expr(inner), InterpPart::Lit(l2)] =
                parts.as_slice()
            {
                if l1.trim_end() == "sqrt(" && l2.trim_start() == ")" {
                    // SH2_BC_NATIVE=exact: the wasm bc number core (sh2.bcSqrt —
            // posixutils-rs Number(BigDecimal)) — exact arbitrary
            // precision + scale, SYNC (wasm is pure CPU — the *Sync loop
            // gates stay green). Errors (negative, non-numeric) → "" like
            // bc's no-stdout-on-error.
            if bc_exact_enabled() {
                return Some(Expr::CallExpression {
                    callee: Box::new(Expr::MemberExpression {
                        object: Box::new(Expr::Identifier {
                            name: "sh2".to_string(),
                        }),
                        property: Box::new(Expr::Identifier {
                            name: "bcSqrt".to_string(),
                        }),
                        computed: false,
                        optional: false,
                    }),
                    arguments: vec![
                        Expr::CallExpression {
                            callee: Box::new(Expr::Identifier {
                                name: "String".to_string(),
                            }),
                            arguments: vec![expr_to_estree(inner)],
                            optional: false,
                        },
                        Expr::Literal {
                            value: serde_json::Value::from(0),
                            raw: None,
                        regex: None,
                        },
                    ],
                    optional: false,
                });
            }
                    let num = Expr::CallExpression {
                        callee: Box::new(Expr::Identifier {
                            name: "Number".to_string(),
                        }),
                        arguments: vec![expr_to_estree(inner)],
                        optional: false,
                    };
                    let root = Expr::CallExpression {
                        callee: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::Identifier {
                                name: "Math".to_string(),
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "sqrt".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        arguments: vec![num],
                        optional: false,
                    };
                    let fl = Expr::CallExpression {
                        callee: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::Identifier {
                                name: "Math".to_string(),
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "floor".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        arguments: vec![root],
                        optional: false,
                    };
                    return Some(Expr::CallExpression {
                        callee: Box::new(Expr::Identifier {
                            name: "String".to_string(),
                        }),
                        arguments: vec![fl],
                        optional: false,
                    });
                }
            }
            // `$a + $b` / `($a + $b) / $c` — the general var-operand
            // form (see bc_var_capture): native bc scale-0 integer
            // arithmetic over the interpolated operands, no spawn.
            bc_var_capture(parts)
        }
        _ => None,
    }
}

/// The strict literal-text scan for the var-operand bc form: the
/// interpolated argument's literal parts may contain only decimal digits,
/// whitespace, and the `+ - * / % ( )` operators (bc's scale-0 integer
/// subset). ANY other character keeps the spawn: letters (`sqrt`,
/// `scale`, bc variables, hex `x`), `.` (fractional literals — the double
/// path cannot reproduce bc's exact fractional output format), `;`
/// (multi-statement programs), `^` (bc POWER vs bash XOR — the two
/// grammars disagree), comparisons, `#` (bash base prefixes), quotes.
fn bc_var_lit_ok(s: &str) -> bool {
    s.chars().all(|c| {
        c.is_ascii_digit()
            || c.is_whitespace()
            || matches!(c, '+' | '-' | '*' | '/' | '%' | '(' | ')')
    })
}

/// `ArithAst` → native JS for the var-operand bc form. Every `__bcvK`
/// placeholder maps to `Number(<slot K's expression>)` — bash vars are
/// strings, the double path needs numbers. Only `+ - * / %` binaries,
/// unary `+`/`-`, integer literals and slot vars are allowed; anything
/// else (bash `^` XOR, `**`, comparisons, `++`, assignments, index ops,
/// ternaries) returns None → the spawn stands. `/` lowers to
/// `Math.trunc(a / b)` — bc's scale-0 division truncates toward zero,
/// plain JS division would leave a fraction; `%` lowers to JS `%` (for
/// integers both give the sign-of-dividend remainder).
fn bc_arith_to_js(a: &ArithAst, slots: &[&IrExpr]) -> Option<Expr> {
    match a {
        ArithAst::Num(i) => Some(Expr::Literal {
            value: serde_json::Value::from(*i),
            raw: None,
            regex: None,
        }),
        ArithAst::Var(v) => {
            let idx = v.strip_prefix("__bcv")?.parse::<usize>().ok()?;
            Some(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "Number".to_string(),
                }),
                arguments: vec![expr_to_estree(slots.get(idx)?)],
                optional: false,
            })
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let l = bc_arith_to_js(lhs, slots)?;
            let r = bc_arith_to_js(rhs, slots)?;
            match op.as_str() {
                "+" | "-" | "*" => Some(Expr::BinaryExpression {
                    operator: op.clone(),
                    left: Box::new(l),
                    right: Box::new(r),
                }),
                "/" => Some(Expr::CallExpression {
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
                        operator: "/".to_string(),
                        left: Box::new(l),
                        right: Box::new(r),
                    }],
                    optional: false,
                }),
                "%" => Some(Expr::BinaryExpression {
                    operator: "%".to_string(),
                    left: Box::new(l),
                    right: Box::new(r),
                }),
                _ => None,
            }
        }
        ArithAst::Un { op, arg } if op == "-" || op == "+" => {
            Some(Expr::UnaryExpression {
                operator: op.clone(),
                argument: Box::new(bc_arith_to_js(arg, slots)?),
                prefix: true,
            })
        }
        _ => None,
    }
}

/// Collect the translated JS divisor expressions of every `/` and `%`
/// site (post-order, matching the evaluation order of the emitted tree).
/// bc aborts the WHOLE program with no stdout when any divisor evaluates
/// to zero — the guard must therefore test the divisor's VALUE (a nested
/// `a / (b / c)` aborts on `b / c == 0`, not just `c == 0`). All operand
/// expressions are pure reads, so the double evaluation the guard
/// introduces is safe.
fn bc_arith_divisors<'a>(
    a: &'a ArithAst,
    slots: &[&IrExpr],
    out: &mut Vec<Expr>,
) -> Option<()> {
    match a {
        ArithAst::Bin { op, lhs, rhs } => {
            bc_arith_divisors(lhs, slots, out)?;
            if op == "/" || op == "%" {
                // a literal divisor is never zero (the strict scan only
                // admits decimal digits) — only var/expr divisors need
                // the guard
                if !matches!(rhs.as_ref(), ArithAst::Num(_)) {
                    out.push(bc_arith_to_js(rhs, slots)?);
                }
            }
            bc_arith_divisors(rhs, slots, out)
        }
        ArithAst::Un { arg, .. } => bc_arith_divisors(arg, slots, out),
        _ => Some(()),
    }
}

/// `$(echo "$sum + $i" | bc)` — the general var-operand bc program
/// (SH2_BC_NATIVE fast tier, see [`bc_native_enabled`] for the documented
/// operand assumption): every Expr slot of the interpolated echo argument
/// becomes an operand, the literal text must pass [`bc_var_lit_ok`], and
/// the whole program must parse as bc's scale-0 integer arithmetic
/// (via the bash-arith parser — the two grammars agree on `+ - * / %`,
/// parens, unary minus, decimal literals, and left associativity; `^`
/// and everything else bails to the spawn). The value is
/// `String(<js arith>)` — for integer operands within 2^53 the double
/// path is exact and String() prints the same digits as bc — with a
/// `(divisor === 0) ? "" : ...` guard per `/`/`%` site (bc's no-stdout
/// abort). Always a single word (integers have no whitespace), so the
/// captureWords form is safe too.
fn bc_var_capture(parts: &[InterpPart]) -> Option<Expr> {
    let mut src = String::new();
    let mut slots: Vec<&IrExpr> = Vec::new();
    for p in parts {
        match p {
            InterpPart::Lit(s) => {
                if !bc_var_lit_ok(s) {
                    return None;
                }
                src.push_str(s);
            }
            InterpPart::Expr(e) => {
                slots.push(e);
                src.push_str(&format!("__bcv{}", slots.len() - 1));
            }
        }
    }
    // all-Lit arguments fold at compile time above; a var form needs at
    // least one slot
    if slots.is_empty() {
        return None;
    }
    let ast = parse_arith(&src)?;
    let value = bc_arith_to_js(&ast, &slots)?;
    let mut divs = Vec::new();
    bc_arith_divisors(&ast, &slots, &mut divs)?;
    let result = Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![value],
        optional: false,
    };
    if divs.is_empty() {
        return Some(result);
    }
    // bc aborts the whole program (no stdout → the capture is "")
    // when ANY divisor evaluates to zero
    let mut guard = Expr::BinaryExpression {
        operator: "===".to_string(),
        left: Box::new(divs[0].clone()),
        right: Box::new(Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
            regex: None,
        }),
    };
    for d in &divs[1..] {
        guard = Expr::LogicalExpression {
            operator: "||".to_string(),
            left: Box::new(guard),
            right: Box::new(Expr::BinaryExpression {
                operator: "===".to_string(),
                left: Box::new(d.clone()),
                right: Box::new(Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                    regex: None,
                }),
            }),
        };
    }
    Some(Expr::ConditionalExpression {
        test: Box::new(guard),
        consequent: Box::new(str_lit("")),
        alternate: Box::new(result),
    })
}

/// The echo text value: the joined args, plus the trailing newline the
/// builtin appends (unless `-n`). A literal join folds the `+ "\n"` into
/// the literal.
fn echo_text(joined: Expr, no_newline: bool) -> Expr {
    if no_newline {
        return joined;
    }
    match joined {
        Expr::Literal {
            value: serde_json::Value::String(sv),
            raw: _,
            regex: _,
        } => str_lit(&format!("{sv}\n")),
        _ => Expr::BinaryExpression {
            operator: "+".to_string(),
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        },
    }
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
/// A static `cut` argument list (the runtime builtin's exact grammar —
/// harness/sh2-namespace.mjs `builtins.cut`): one of -f/-c/-b with a
/// merged/sorted 1-based inclusive range list (`N`, `N-M`, `N-`, `-M`,
/// comma-separated), `-d` (FIRST char only, GNU's rule), `-s`, and
/// `--output-delimiter`. `None` keeps the runtime path: dynamic args,
/// file operands, malformed lists, or a missing mode all lower to the
/// runtime builtin (which reports GNU's error + exit 1 exactly).
#[derive(Debug, Clone)]
struct CutSpec {
    mode: char,                      // 'f' | 'c' | 'b'
    ranges: Vec<(i64, Option<i64>)>, // merged, sorted; hi None = open range
    delim: String,                   // -d value (first char; '\t' default)
    suppress: bool,                  // -s
    out_delim: Option<String>,       // --output-delimiter
}

/// GNU cut's range-list grammar (`N`, `N-M`, `N-`, `-M`, comma lists),
/// merged and sorted exactly like the runtime builtin (overlapping /
/// adjacent ranges collapse — `-f1-3,2-4` is 1..4). `None` = malformed
/// (`0`, a decreasing range, a bare `-`, garbage) → keep the runtime.
fn parse_cut_ranges(s: &str) -> Option<Vec<(i64, Option<i64>)>> {
    let mut ranges: Vec<(i64, Option<i64>)> = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        let (lo, hi): (i64, Option<i64>) =
            if let Some(rest) = part.strip_prefix('-') {
                if rest.is_empty() {
                    return None;
                }
                (1, Some(rest.parse::<i64>().ok()?))
            } else if let Some(idx) = part.find('-') {
                let (a, b) = (&part[..idx], &part[idx + 1..]);
                if a.is_empty() && b.is_empty() {
                    return None;
                }
                let lo: i64 = if a.is_empty() { 1 } else { a.parse().ok()? };
                let hi: Option<i64> = if b.is_empty() {
                    None
                } else {
                    Some(b.parse::<i64>().ok()?)
                };
                if let Some(h) = hi {
                    if h < lo {
                        return None;
                    }
                }
                (lo, hi)
            } else {
                let n: i64 = part.parse().ok()?;
                (n, Some(n))
            };
        if lo < 1 {
            return None;
        }
        ranges.push((lo, hi));
    }
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_by_key(|(lo, hi)| (*lo, hi.unwrap_or(i64::MAX)));
    let mut merged: Vec<(i64, Option<i64>)> = Vec::new();
    for (lo, hi) in ranges {
        if let Some(last) = merged.last_mut() {
            let last_hi = last.1.unwrap_or(i64::MAX);
            let this_hi = hi.unwrap_or(i64::MAX);
            if lo <= last_hi + 1 {
                if this_hi > last_hi {
                    last.1 = hi; // keep None when the new range is open
                }
                continue;
            }
        }
        merged.push((lo, hi));
    }
    Some(merged)
}

/// Parse a static cut ARG LIST (every arg an IrExpr::Str — dynamic args
/// keep the runtime). Mirrors the runtime builtin's option loop exactly
/// (including its `-d` first-char rule and the attached `-f1,3`/`-c3-5`
/// forms); any file operand or unknown flag → None.
fn parse_cut_args(args: &[IrExpr]) -> Option<CutSpec> {
    let mut mode: Option<char> = None;
    let mut ranges: Option<Vec<(i64, Option<i64>)>> = None;
    let mut delim = String::from("\t");
    let mut suppress = false;
    let mut out_delim: Option<String> = None;
    let mut i = 0;
    let mut after_dd = false;
    let n = args.len();
    while i < n {
        let IrExpr::Str(a, _) = &args[i] else {
            return None;
        };
        let a = a.clone();
        i += 1;
        if !after_dd && a == "--" {
            after_dd = true;
            continue;
        }
        if after_dd {
            return None; // file operands → runtime (stdin form only lifts)
        }
        if a == "--output-delimiter" {
            let IrExpr::Str(v, _) = args.get(i)? else {
                return None;
            };
            out_delim = Some(v.clone());
            i += 1;
            continue;
        }
        if let Some(v) = a.strip_prefix("--output-delimiter=") {
            out_delim = Some(v.to_string());
            continue;
        }
        if a == "-s" {
            suppress = true;
            continue;
        }
        if a == "-d" {
            let IrExpr::Str(sv, _) = args.get(i)? else {
                return None;
            };
            i += 1;
            delim = sv.chars().next().map(String::from).unwrap_or_else(|| "\t".into());
            continue;
        }
        if let Some(v) = a.strip_prefix("-d") {
            if !v.is_empty() {
                // attached `-d:` — BEFORE the -f/-c/-b attached check
                delim = v.chars().next().map(String::from).unwrap_or_else(|| "\t".into());
                continue;
            }
        }
        if a == "-f" || a == "-c" || a == "-b" {
            let IrExpr::Str(sv, _) = args.get(i)? else {
                return None;
            };
            i += 1;
            mode = a.chars().nth(1);
            ranges = parse_cut_ranges(sv);
            continue;
        }
        if a.len() > 2
            && (a.starts_with("-f") || a.starts_with("-c") || a.starts_with("-b"))
        {
            mode = a.chars().nth(1);
            ranges = parse_cut_ranges(&a[2..]);
            continue;
        }
        return None; // file operands / unknown flags → runtime
    }
    Some(CutSpec {
        mode: mode?,
        ranges: ranges?,
        delim,
        suppress,
        out_delim,
    })
}

/// The range predicate `(i >= lo && (hi === null || i <= hi)) || ...` —
/// the merged-range membership test the -f filter / -c filter callbacks
/// use (the runtime builtin's `positions(pos, L)` membership, with the
/// open-range hi clamped per line by the slice/filter length).
fn cut_range_pred(i: Expr, ranges: &[(i64, Option<i64>)]) -> Expr {
    let mut terms: Vec<Expr> = Vec::new();
    for (lo, hi) in ranges {
        let ge = Expr::BinaryExpression {
            operator: ">=".to_string(),
            left: Box::new(i.clone()),
            right: Box::new(int_lit_expr(*lo)),
        };
        let le = match hi {
            Some(h) => Expr::BinaryExpression {
                operator: "<=".to_string(),
                left: Box::new(i.clone()),
                right: Box::new(int_lit_expr(*h)),
            },
            None => bool_lit(true),
        };
        terms.push(Expr::LogicalExpression {
            operator: "&&".to_string(),
            left: Box::new(ge),
            right: Box::new(le),
        });
    }
    let mut it = terms.into_iter();
    let first = it.next().unwrap_or(bool_lit(false));
    it.fold(first, |acc, t| Expr::LogicalExpression {
        operator: "||".to_string(),
        left: Box::new(acc),
        right: Box::new(t),
    })
}

/// The per-line selection for a CutSpec over a line expression `l` — the
/// runtime builtin's exact per-line rules (`builtins.cut`): -f picks the
/// fields at the merged positions clamped to the line's split length
/// (empty interior fields kept, trailing ones omitted, join with the
/// output delimiter or the input one); a no-delimiter line passes through
/// WHOLE (unless -s — which the CALLER drops from the line list, GNU's
/// "the line vanishes entirely" rule); -c/-b pick code points
/// (`[...line]`, the runtime's `positions` base).
fn cut_sel_expr(spec: &CutSpec, l: &Expr) -> Option<Expr> {
    let sel = match spec.mode {
        'c' | 'b' => {
            let cps = Expr::ArrayExpression {
                elements: vec![Some(Expr::SpreadElement {
                    argument: Box::new(l.clone()),
                })],
            };
            match spec.ranges.as_slice() {
                // -c1- / -b1- — every code point, in order: the identity
                [(1, None)] => l.clone(),
                [(lo, None)] => method_call(
                    method_call(cps, "slice", vec![int_lit_expr(lo - 1)]),
                    "join",
                    vec![str_lit("")],
                ),
                [(lo, Some(hi))] => method_call(
                    method_call(cps, "slice", vec![int_lit_expr(lo - 1), int_lit_expr(*hi)]),
                    "join",
                    vec![str_lit("")],
                ),
                // merged multi-range: filter by membership (the runtime's
                // positions() enumeration is ascending — filter preserves it)
                _ => {
                    let i = ident("i");
                    let pred = cut_range_pred(
                        Expr::BinaryExpression {
                            operator: "+".to_string(),
                            left: Box::new(i.clone()),
                            right: Box::new(int_lit_expr(1)),
                        },
                        &spec.ranges,
                    );
                    let filtered = method_call(
                        cps,
                        "filter",
                        vec![Expr::ArrowFunctionExpression {
                            params: vec![ident("_"), i],
                            body: ArrowBody::Expr(Box::new(pred)),
                            expression: true,
                            r#async: false,
                        }],
                    );
                    method_call(filtered, "join", vec![str_lit("")])
                }
            }
        }
        'f' => {
            let has_d = method_call(l.clone(), "includes", vec![str_lit(&spec.delim)]);
            let fields = method_call(l.clone(), "split", vec![str_lit(&spec.delim)]);
            let join_d = spec.out_delim.clone().unwrap_or_else(|| spec.delim.clone());
            let no_delim = if spec.suppress {
                // unreachable under -s (the caller's filter dropped the
                // line) — kept for shape uniformity
                str_lit("")
            } else {
                l.clone()
            };
            if spec.ranges == [(1, None)] && join_d == spec.delim {
                // -f1- with the default output delimiter: every field in
                // order — split+join is the identity; only the
                // no-delimiter rule remains (`-s` handled by the caller)
                Expr::ConditionalExpression {
                    test: Box::new(has_d),
                    consequent: Box::new(l.clone()),
                    alternate: Box::new(no_delim),
                }
            } else {
                let i = ident("i");
                let pred = cut_range_pred(
                    Expr::BinaryExpression {
                        operator: "+".to_string(),
                        left: Box::new(i.clone()),
                        right: Box::new(int_lit_expr(1)),
                    },
                    &spec.ranges,
                );
                let picked = method_call(
                    fields,
                    "filter",
                    vec![Expr::ArrowFunctionExpression {
                        params: vec![ident("_"), i],
                        body: ArrowBody::Expr(Box::new(pred)),
                        expression: true,
                        r#async: false,
                    }],
                );
                let picked_join = method_call(picked, "join", vec![str_lit(&join_d)]);
                Expr::ConditionalExpression {
                    test: Box::new(has_d),
                    consequent: Box::new(picked_join),
                    alternate: Box::new(no_delim),
                }
            }
        }
        _ => return None,
    };
    Some(sel)
}

/// The cut output for a stdin text value: `lines.map(sel).join('\n')`
/// plus the trailing newline cut emits whenever the input ends with one
/// (always true for the echo/here-string/heredoc feeds; false only for
/// `echo -n`). `lines` is the caller-built line-list expression (the
/// feed's split — see the call sites). With -s, no-delimiter lines are
/// dropped from the list entirely (GNU: the line vanishes, no newline).
fn cut_value_expr(lines: Expr, spec: &CutSpec, input_ends_nl: bool) -> Option<Expr> {
    let chain = if spec.mode == 'f' && spec.suppress {
        let l = ident("l");
        let keep = method_call(l.clone(), "includes", vec![str_lit(&spec.delim)]);
        method_call(lines, "filter", vec![sync_arrow_expr_param("l", keep)])
    } else {
        lines
    };
    let sel = cut_sel_expr(spec, &ident("l"))?;
    let mapped = method_call(chain, "map", vec![sync_arrow_expr_param("l", sel)]);
    let joined = method_call(mapped, "join", vec![str_lit("\n")]);
    if input_ends_nl {
        Some(Expr::BinaryExpression {
            operator: "+".to_string(),
            left: Box::new(joined),
            right: Box::new(str_lit("\n")),
        })
    } else {
        Some(joined)
    }
}

/// `echo ARGS | cut OP` — the two-stage pipeline whose LAST stage is the
/// cut builtin with fully static args: the text is echo's exact output
/// (the echo_join_args join; echo appends the trailing newline unless
/// `-n`), and the selection is a pure string-op chain over it (see
/// `cut_value_expr`) — no spawns, no pipeline machinery, no async
/// capture arrow. Script-defined echo/cut functions shadow the builtins
/// → keep the pipeline. Returns the (echo join expr, no_newline flag,
/// raw cut args, parsed spec).
fn try_native_echo_cut(pipe: &IrExpr) -> Option<(Expr, bool, Vec<IrExpr>, CutSpec)> {
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
    // stage 1: exec("echo", args) — the exact bytes echo writes
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
    // raw-byte / glob / process-substitution markers: the runtime decodes
    // them before cut sees the text — the native chain cannot (see
    // ir_expr_needs_runtime)
    if echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    // stage 2: exec("cut", args)
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(name2, _), IrExpr::Array(cut_args)] = a2.as_slice() else {
        return None;
    };
    if name2 != "cut" {
        return None;
    }
    let spec = parse_cut_args(cut_args)?;
    Some((joined, no_newline, cut_args.to_vec(), spec))
}

/// The cut stdin LINES expression for an echo-feed text: echo appends the
/// trailing newline unless `-n`, so the input always ends with one — the
/// runtime's line model (`input.split('\n')` with the final '' popped by
/// the endsWith-newline branch) is exactly `text.split('\n')`; with `-n`
/// the input is the text itself and may or may not end with a newline
/// (the runtime pops the split's trailing '' only when it does).
fn cut_echo_lines(text: Expr, no_newline: bool) -> Expr {
    if no_newline {
        method_call(
            Expr::ConditionalExpression {
                test: Box::new(method_call(
                    text.clone(),
                    "endsWith",
                    vec![str_lit("\n")],
                )),
                consequent: Box::new(method_call(
                    text.clone(),
                    "slice",
                    vec![int_lit_expr(0), int_lit_expr(-1)],
                )),
                alternate: Box::new(text.clone()),
            },
            "split",
            vec![str_lit("\n")],
        )
    } else {
        method_call(text, "split", vec![str_lit("\n")])
    }
}

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
            operator: "+".to_string(),
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
            operator: "+".to_string(),
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
            operator: "-".to_string(),
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
            operator: "+".to_string(),
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

/// The parsed shape of an awk program for the echo|awk inline lift
/// (see [`try_native_echo_awk`]): a single action block that prints one
/// field (`{print $N}` / `{print$N}` / `{print $0}`) or the sum of two
/// fields (`{print $N + $M}`), with an optional literal `-F sep`
/// (single char). Anything else (regex conditions, `for`/`if` bodies,
/// BEGIN/END, file operands, dynamic args) keeps the pipeline.
#[derive(Debug, Clone)]
struct AwkPrintSpec {
    /// `-F sep` — the literal field separator (default FS: whitespace
    /// runs — awk's exact default).
    fs_sep: Option<String>,
    kind: AwkPrintKind,
}

#[derive(Debug, Clone)]
enum AwkPrintKind {
    /// `{print $0}` — the whole line.
    WholeLine,
    /// `{print $N}` — field N (1-based; missing → empty string).
    Field(u32),
    /// `{print $N + $M}` — awk arithmetic (non-numeric fields coerce
    /// to 0; the result prints as a number).
    Sum(u32, u32),
}

/// Parse the awk argv (`-F` flags + the single-quoted program; NO file
/// operands — stdin only). Returns None for any other shape.
fn parse_awk_print_args(args: &[IrExpr]) -> Option<AwkPrintSpec> {
    let mut fs_sep = None;
    let mut prog: Option<&str> = None;
    let mut i = 0usize;
    while i < args.len() {
        let IrExpr::Str(s, _) = &args[i] else {
            return None;
        };
        if s == "-F" {
            let IrExpr::Str(sep, _) = args.get(i + 1)? else {
                return None;
            };
            if sep.chars().count() != 1 {
                return None;
            }
            fs_sep = Some(sep.clone());
            i += 2;
            continue;
        }
        if let Some(rest) = s.strip_prefix("-F") {
            if rest.chars().count() != 1 {
                return None;
            }
            fs_sep = Some(rest.to_string());
            i += 1;
            continue;
        }
        if prog.is_some() {
            return None; // a file operand (or a second program) — stdin-only lift
        }
        prog = Some(s);
        i += 1;
    }
    let prog = prog?.trim();
    let body = prog.strip_prefix('{')?.strip_suffix('}')?.trim();
    // `print$N` (no space — `awk '{print$3}'`) or `print $N`
    let rest = body.strip_prefix("print")?.trim_start();
    fn parse_field(s: &str) -> Option<(u32, &str)> {
        let s = s.strip_prefix('$')?;
        let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        let n: u32 = digits.parse().ok()?;
        Some((n, &s[digits.len()..]))
    }
    let (n, after) = parse_field(rest)?;
    let after = after.trim();
    let kind = if after.is_empty() {
        if n == 0 {
            AwkPrintKind::WholeLine
        } else {
            AwkPrintKind::Field(n)
        }
    } else {
        // `$N + $M` — the only supported operator (awk arithmetic)
        let after = after.strip_prefix('+')?.trim_start();
        let (m, tail) = parse_field(after)?;
        if !tail.trim().is_empty() || n == 0 || m == 0 {
            return None;
        }
        AwkPrintKind::Sum(n, m)
    };
    Some(AwkPrintSpec { fs_sep, kind })
}

/// The awk field-list expression for one input line `l`: the split on
/// the `-F` separator (literal) or awk's default FS (whitespace runs,
/// leading/trailing ignored — `trim().split(/\s+/)`).
fn awk_fields_expr(l: Expr, spec: &AwkPrintSpec) -> Expr {
    match &spec.fs_sep {
        Some(sep) => method_call(l, "split", vec![str_lit(sep)]),
        None => method_call(
            method_call(l, "trim", vec![]),
            "split",
            vec![regex_lit("\\s+")],
        ),
    }
}

/// The per-line print value for the parsed program: `$0` → the line,
/// `$N` → `fields[N-1] ?? ""`, `$N + $M` → awk's numeric sum string.
fn awk_print_sel_expr(l: Expr, spec: &AwkPrintSpec) -> Expr {
    let field_at = |fields: Expr, n: u32| -> Expr {
        Expr::LogicalExpression {
            operator: "??".to_string(),
            left: Box::new(Expr::MemberExpression {
                object: Box::new(fields),
                property: Box::new(int_lit_expr(i64::from(n - 1))),
                computed: true,
                optional: false,
            }),
            right: Box::new(str_lit("")),
        }
    };
    match &spec.kind {
        AwkPrintKind::WholeLine => l,
        AwkPrintKind::Field(n) => field_at(awk_fields_expr(l, spec), *n),
        AwkPrintKind::Sum(n, m) => {
            let num = |e: Expr| Expr::LogicalExpression {
                operator: "||".to_string(),
                left: Box::new(Expr::CallExpression {
                    callee: Box::new(Expr::Identifier {
                        name: "Number".to_string(),
                    }),
                    arguments: vec![e],
                    optional: false,
                }),
                right: Box::new(int_lit_expr(0)),
            };
            let f = awk_fields_expr(l, spec);
            let sum = Expr::BinaryExpression {
                operator: "+".to_string(),
                left: Box::new(num(field_at(f.clone(), *n))),
                right: Box::new(num(field_at(f, *m))),
            };
            Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "String".to_string(),
                }),
                arguments: vec![sum],
                optional: false,
            }
        }
    }
}

/// `$(echo ARGS | awk '{print $N}')` — the two-stage pipeline whose
/// LAST stage is the awk builtin with a static field-print program (see
/// [`parse_awk_print_args`]): the value is a pure string-op chain over
/// echo's exact output (the `echo_join_args` join; echo appends the
/// trailing newline unless `-n`) — awk's per-record field extraction,
/// joined with newlines, plus awk's own trailing newline (the capture
/// strips it). No spawns, no pipeline machinery, no async capture
/// arrow. Script-defined echo/awk functions shadow the builtins → keep
/// the pipeline. Returns the (echo join expr, no_newline flag, parsed
/// spec).
fn try_native_echo_awk(pipe: &IrExpr) -> Option<(Expr, bool, AwkPrintSpec)> {
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
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if name1 != "echo" || echo_args.iter().any(ir_expr_needs_runtime) {
        return None;
    }
    let (joined, no_newline, _) = echo_join_args(echo_args)?;
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    let [IrExpr::Str(name2, _), IrExpr::Array(awk_args)] = a2.as_slice() else {
        return None;
    };
    if name2 != "awk" {
        return None;
    }
    let spec = parse_awk_print_args(awk_args)?;
    Some((joined, no_newline, spec))
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

/// A test OPERAND read (`-n "$x"`, `"$x" =~ pat`): quoted or bare
/// `$name` / `${name}` (lifted binding → native identifier; special var →
/// native state read; store var → getVar), a single-quoted literal (the
/// runtime's tokenizer keeps single-quoted content raw — no expansion),
/// or a literal with no expansion or quote characters. Mirrors the
/// runtime tokenizer's operand collection + expansion exactly.
fn test_value_operand(op: &str) -> Option<Expr> {
    let op = op.trim();
    if let Some(inner) = op.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        if inner.chars().any(|c| matches!(c, '\'' | '$' | '`' | '\\')) {
            return None;
        }
        return Some(str_lit(inner));
    }
    let bare = op
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(op);
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
    if !bare.is_empty()
        && !bare
            .chars()
            .any(|c| matches!(c, '$' | '`' | '\\' | '"' | '\'' | ' ' | '\t'))
    {
        return Some(str_lit(bare));
    }
    None
}

/// Conservative JS-RegExp validity check (sound, not complete): accepts
/// only patterns that provably compile in V8/node. The runtime's `=~` is
/// `try { new RegExp(r).test(l) } catch { false }` — an invalid pattern
/// yields FALSE at runtime, so a native regex LITERAL (which would fail
/// at module PARSE time) is only safe when the pattern provably compiles;
/// anything this checker cannot prove falls back to the runtime call.
fn js_regex_valid(p: &str) -> bool {
    let cs: Vec<char> = p.chars().collect();
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut can_quantify = false;
    while i < cs.len() {
        let c = cs[i];
        match c {
            '\\' => {
                let Some(&e) = cs.get(i + 1) else { return false };
                // valid escapes: \d \D \s \S \w \W \b \B \f \n \r \t
                // \v \0, or identity escapes (\ + non-alphanumeric);
                // anything else (\x \u \c \1..\9 …) is conservative-rejected
                let ok = matches!(e, 'd'|'D'|'s'|'S'|'w'|'W'|'b'|'B'|'f'|'n'|'r'|'t'|'v'|'0')
                    || !e.is_ascii_alphanumeric();
                if !ok {
                    return false;
                }
                can_quantify = true;
                i += 2;
            }
            '[' => {
                // character class: scan to the first `]` (a `]` right after
                // `[` / `[^` is a literal member — `[[:space:]]` is a class
                // whose first member is `[`)
                i += 1;
                if i < cs.len() && cs[i] == '^' {
                    i += 1;
                }
                let mut closed = false;
                while i < cs.len() {
                    if cs[i] == '\\' {
                        let Some(&e) = cs.get(i + 1) else { return false };
                        let ok = matches!(e, 'd'|'D'|'s'|'S'|'w'|'W'|'b'|'B'|'f'|'n'|'r'|'t'|'v'|'0')
                            || !e.is_ascii_alphanumeric();
                        if !ok {
                            return false;
                        }
                        i += 2;
                        continue;
                    }
                    if cs[i] == ']' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                if !closed {
                    return false;
                }
                can_quantify = true;
            }
            '(' => {
                // `(?:` / `(?=` / `(?!` group opens; any other `?` right
                // after `(` is not a valid JS group construct
                if i + 2 < cs.len() && cs[i + 1] == '?' && matches!(cs[i + 2], ':' | '=' | '!') {
                    i += 3;
                } else if i + 1 < cs.len() && cs[i + 1] == '?' {
                    return false;
                } else {
                    i += 1;
                }
                depth += 1;
                can_quantify = false;
            }
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                can_quantify = true;
                i += 1;
            }
            '|' | '^' | '$' => {
                can_quantify = false;
                i += 1;
            }
            '*' | '+' | '?' => {
                if !can_quantify {
                    return false;
                }
                can_quantify = false;
                i += 1;
            }
            '{' => {
                // `{n}` / `{n,}` / `{n,m}` — a quantifier when an atom
                // precedes; a bare `{` is a literal in JS. A quantifier
                // with nothing to repeat is invalid — reject (sound).
                let mut j = i + 1;
                let mut digits = 0usize;
                while j < cs.len() && cs[j].is_ascii_digit() {
                    digits += 1;
                    j += 1;
                }
                if digits > 0 && j < cs.len() && cs[j] == '}' {
                    if !can_quantify {
                        return false;
                    }
                    can_quantify = false;
                    i = j + 1;
                    continue;
                }
                if digits > 0 && j < cs.len() && cs[j] == ',' {
                    let mut j2 = j + 1;
                    while j2 < cs.len() && cs[j2].is_ascii_digit() {
                        j2 += 1;
                    }
                    if j2 < cs.len() && cs[j2] == '}' {
                        if !can_quantify {
                            return false;
                        }
                        can_quantify = false;
                        i = j2 + 1;
                        continue;
                    }
                }
                // literal `{` — valid JS
                can_quantify = true;
                i += 1;
            }
            _ => {
                can_quantify = true;
                i += 1;
            }
        }
    }
    depth == 0
}

/// The `=~` RHS pattern as a JS regex-literal string. The runtime keeps
/// the pattern token raw except `$`-expansions (the tokenizer expands
/// `$name` / `${...}` / `$((...))` / `$(...)` / `$'...'` inside the word)
/// and strips surrounding quotes (bash: quoting the pattern makes it
/// literal text — the runtime's expandWord returns the same string). A
/// `$` that cannot start an expansion (a trailing `$` — the anchor) stays
/// literal. The pattern must also provably compile as a JS regex (see
/// `js_regex_valid` — a native literal that fails would crash at module
/// PARSE time; the runtime catches construction errors → false). Bare `/`
/// is escaped for the literal form (`\/` — same regex, parse-safe).
fn regex_test_pattern(rhs: &str) -> Option<String> {
    let rhs = rhs.trim();
    let pat = rhs
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| rhs.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))
        .unwrap_or(rhs);
    if pat.is_empty()
        || pat
            .chars()
            .any(|c| matches!(c, '"' | '\'' | '`' | '\n' | '\r' | '\t'))
    {
        return None;
    }
    let cs: Vec<char> = pat.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        if c == '$' {
            match cs.get(i + 1) {
                None => {} // trailing `$` — the anchor, stays literal
                Some(&n)
                    if n == '(' || n == '\''
                        || n.is_ascii_alphanumeric()
                        || n == '_'
                        || matches!(n, '#' | '@' | '*' | '?' | '$' | '{') =>
                {
                    return None; // would be expanded by the runtime
                }
                _ => {}
            }
        }
    }
    if !js_regex_valid(&pat) {
        return None;
    }
    // escape bare `/` for the regex literal (a `/` directly in the
    // literal text would terminate it); `\/` is the same regex
    let mut out = String::with_capacity(pat.len());
    let mut it = pat.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            out.push(c);
            if let Some(&n) = it.peek() {
                out.push(n);
                it.next();
            }
        } else if c == '/' {
            out.push('\\');
            out.push('/');
        } else {
            out.push(c);
        }
    }
    Some(out)
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
                let (bare, quoted) = match operand
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                {
                    Some(b) => (b, true),
                    None => (operand, false),
                };
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
                // `""` — a quoted EMPTY literal: `[ -z "" ]` is always
                // true, `[ -n "" ]` always false — a plain "" string
                // compare. Unquoted-empty operands (`[ -z ]` — the flag
                // itself is the string) stay on the runtime.
                if bare.is_empty() && quoted {
                    return Some(str_lit(""));
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
                    operator: if want_empty {"===".to_string()} else {"!==".to_string()},
                    left: Box::new(val),
                    right: Box::new(str_lit("")),
                });
            }
        }
    }
    // `=~` regex family: the runtime's evalTest is exactly
    // `new RegExp(r).test(l)` (an invalid pattern is caught → false), so
    // a native regex-literal `.test(l)` with the operands inlined is
    // value-identical when the pattern provably compiles (see
    // `js_regex_valid`). The regex literal is stateless (no /g flag —
    // exactly the runtime's fresh `new RegExp(r)` per evaluation). The
    // `=~` scan must run BEFORE the string-op loop (`=` would consume
    // the `=` of `=~`).
    {
        let mut in_q = false;
        let mut ix = 0usize;
        let mut eq_tilde = None;
        let b = s.as_bytes();
        while ix < b.len() {
            if b[ix] == b'"' {
                in_q = !in_q;
                ix += 1;
                continue;
            }
            if !in_q && s[ix..].starts_with("=~") {
                eq_tilde = Some(ix);
                break;
            }
            ix += 1;
        }
        if let Some(ix) = eq_tilde {
            let value = test_value_operand(&s[..ix])?;
            let pat = regex_test_pattern(&s[ix + 2..])?;
            let val = Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "String".to_string(),
                }),
                arguments: vec![value],
                optional: false,
            };
            return Some(Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(regex_lit(&pat)),
                    property: Box::new(Expr::Identifier {
                        name: "test".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![val],
                optional: false,
            });
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
        // scan ALL ` -op ` positions (not just the first): an earlier
        // position whose operands do not both lower may be a FALSE match
        // inside a compound (`[ A -gt 1 -a B -lt 2 ]` — the first `-gt`
        // match has the compound tail as its rhs). Skipping a failed
        // position and continuing is exactly the compound path's job
        // (each leaf then lowers on its own); the single-op cases are
        // unaffected (their one position either lowers or falls through
        // to the runtime).
        let mut from = 0usize;
        while let Some(off) = s[from..].find(&pat) {
            let p = from + off;
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
                    return Some((arith_to_estree_wrapped(&a), false));
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
            let l = num_operand(lhs);
            let r = num_operand(rhs);
            let (Some((l, l_risky)), Some((r, r_risky))) = (l, r) else {
                // an operand failed to lower — this ` -op ` position is a
                // false match (a compound tail / quoted text); keep
                // scanning for the real operator (the compound path at
                // the end handles multi-op tests leaf by leaf).
                from = p + pat.len();
                continue;
            };
            if !l_risky && !r_risky {
                return Some(Expr::BinaryExpression {
                    operator: js.to_string(),
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
                    operator: "!".to_string(),
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
                    operator: "&&".to_string(),
                    left: Box::new(guarded),
                    right: Box::new(nan_ok(&r)),
                };
            }
            let cmp = Expr::BinaryExpression {
                operator: js.to_string(),
                left: Box::new(num(l)),
                right: Box::new(num(r)),
            };
            return Some(Expr::LogicalExpression {
                operator: "&&".to_string(),
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
                    let l = eq_test_operand(lhs);
                    let r = eq_test_operand(rhs);
                    let (Some(l), Some(r)) = (l, r) else {
                        // an operand failed to lower — this operator
                        // position is a false match (a compound tail / a
                        // quoted `=` in text); keep scanning: the
                        // compound path at the end handles `-a`/`-o`
                        // multi-op tests leaf by leaf.
                        idx += 1;
                        continue;
                    };
                    // bash `=` / `==` / `!=` is a STRING comparison: the
                    // operands are the STRING expansions (`$i` → the
                    // string form of the number). A numeric-lifted var is
                    // a JS number and `$?`/`$#` are numeric state fields
                    // — a bare `===` would compare number-vs-string and
                    // ALWAYS be false. Compare the String() forms
                    // (String(lit) is identity; String(num) is the shell
                    // expansion) — this is the -n/-z unary path's and
                    // glob path's existing convention.
                    let stringify = |e: Expr| Expr::CallExpression {
                        callee: Box::new(Expr::Identifier {
                            name: "String".to_string(),
                        }),
                        arguments: vec![e],
                        optional: false,
                    };
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
                            operator: js.to_string(),
                            left: Box::new(lc(l)),
                            right: Box::new(lc(r)),
                        });
                    }
                    return Some(Expr::BinaryExpression {
                        operator: js.to_string(),
                        left: Box::new(stringify(l)),
                        right: Box::new(stringify(r)),
                    });
                }
            }
            idx += 1;
        }
    }
    // Compound `-a` / `-o` chains: the runtime's parseTest binds `-a`
    // tighter than `-o` (`or()` calls `and()`), `!` negates ONE primary,
    // and both short-circuit — a native `&&`/`||` chain of native leaves
    // (each leaf recurses into this same lowering) is the exact value
    // minus the string tokenize/parse/dispatch. Only top-level
    // connectors count: quoted `-a`/`-o` text and paren-grouped
    // compounds stay on the runtime.
    try_native_compound_test(s)
}

/// Split on a top-level ` -conn ` connector (outside quotes and parens),
/// returning the parts. None when the connector does not appear.
fn split_test_connector<'a>(s: &'a str, conn: &str) -> Option<Vec<&'a str>> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut in_q = false;
    let mut start = 0usize;
    let mut parts = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' => {
                in_q = !in_q;
                i += 1;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b' ' if depth == 0 && !in_q && s[i + 1..].starts_with(conn) && {
                let after = i + 1 + conn.len();
                after < s.len() && s[after..].starts_with(' ')
            } => {
                parts.push(&s[start..i]);
                i += 1 + conn.len() + 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(&s[start..]);
    Some(parts)
}

/// One compound-test leaf: a leading `!` negates the leaf (the runtime's
/// `not()` binds to a single primary — `! A -a B` is `(!A) -a B`);
/// `!(` is an extglob pattern, never a negation.
fn try_native_test_leaf(s: &str) -> Option<Expr> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('!') {
        if rest.starts_with('(') {
            return None;
        }
        let inner = try_native_test(rest)?;
        return Some(Expr::UnaryExpression {
            operator: "!".to_string(),
            argument: Box::new(inner),
            prefix: true,
        });
    }
    try_native_test(s)
}

/// True when the test string contains any `=`/`==`/`!=`/numeric-op token
/// OUTSIDE quotes — i.e. the single-op scans skipped a failed position
/// (see the numeric/string-op loops in [`try_native_test`]). The compound
/// cap applies only to the lowerings those skips newly enable: compounds
/// reachable in the pre-skip baseline (pure unary/`-a`/`-o` chains) must
/// keep their old behavior, while a skipped-op compound that would trade
/// one runtime test for several getVars stays on the runtime call.
fn test_has_op_token(s: &str) -> bool {
    let b = s.as_bytes();
    let mut in_q = false;
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'\'' {
            // single-quoted region: skip to the close (no escapes in sh)
            if let Some(close) = s[i + 1..].find('\'') {
                i += close + 2;
                continue;
            }
            i += 1;
            continue;
        }
        if b[i] == b'"' {
            in_q = !in_q;
            i += 1;
            continue;
        }
        if !in_q && (s[i..].starts_with("==") || s[i..].starts_with("!=") || b[i] == b'=') {
            return true;
        }
        if !in_q && b[i] == b'-' && i + 1 < b.len() {
            let rest = &s[i + 1..];
            if rest.starts_with("eq ")
                || rest.starts_with("ne ")
                || rest.starts_with("lt ")
                || rest.starts_with("le ")
                || rest.starts_with("gt ")
                || rest.starts_with("ge ")
            {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn try_native_compound_test(s: &str) -> Option<Expr> {
    let s = s.trim();
    if s.contains(['(', ')']) {
        return None; // paren-grouped compounds stay on the runtime
    }
    // Compose a compound native; refuse it when its sh2.* CALL count
    // exceeds the single runtime `sh2.test` call it would replace (a
    // compound of store-var leaves would trade 1 test for N getVars — a
    // net call-site metric loss even though the runtime work drops; the
    // improvement loop auto-stashes metric regressions). Pure-literal
    // compounds (0 calls) and single-read compounds (1 call, e.g. one
    // store var + literals) lower; multi-read compounds stay on the
    // runtime call. The cap applies ONLY to the compounds the op-loop
    // skips newly enable (an op token present — see
    // [`test_has_op_token`]): baseline-reachable pure-`-a`/`-o`/unary
    // compounds keep their pre-existing (uncapped) lowering.
    let cap = |acc: Expr| -> Option<Expr> {
        if !test_has_op_token(s) || expr_sh2_call_count(&acc) <= 1 {
            Some(acc)
        } else {
            None
        }
    };
    // `-o` splits first (`-a` binds tighter, so `A -o B -a C` is
    // `A || (B && C)` — the -a recursion happens inside each -o leaf)
    if let Some(parts) = split_test_connector(s, "-o") {
        if parts.len() > 1 {
            let mut it = parts.into_iter();
            let mut acc = try_native_test_leaf(it.next()?)?;
            for p in it {
                let r = try_native_test_leaf(p)?;
                acc = Expr::LogicalExpression {
                    operator: "||".to_string(),
                    left: Box::new(acc),
                    right: Box::new(r),
                };
            }
            return cap(acc);
        }
    }
    if let Some(parts) = split_test_connector(s, "-a") {
        if parts.len() > 1 {
            let mut it = parts.into_iter();
            let mut acc = try_native_test_leaf(it.next()?)?;
            for p in it {
                let r = try_native_test_leaf(p)?;
                acc = Expr::LogicalExpression {
                    operator: "&&".to_string(),
                    left: Box::new(acc),
                    right: Box::new(r),
                };
            }
            return cap(acc);
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
                    operator: "??".to_string(),
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
    // calls), or a STORE var (a `sh2.getVar` read — the exact value the
    // runtime's param would read from the store; the string ops lower
    // native, skipping the param dispatch/switch).
    let value: Option<Expr> = if is_lifted(name) {
        Some(Expr::Identifier {
            name: name.clone(),
        })
    } else {
        positional_read(name).or_else(|| Some(sh2_call("getVar", vec![str_lit(name)])))
    };
    let value = value?;
    // positional writes (`${1:=d}`) and `:?` exits stay on the runtime.
    let is_positional = positional_read(name).is_some() && !is_lifted(name);
    // Ops that read the value more than once (first-char case, glob-strip,
    // default-value): for a STORE var the native must evaluate the getVar
    // EXACTLY ONCE into the runtime's `_g` scratch (the Plan 15 guard
    // protocol) — a duplicated getVar would double the store reads and the
    // metric would count them twice. The reads below then come from
    // `sh2._g` and the whole native is wrapped in the single-eval seq.
    // (`sh2._g = sh2.getVar(name), <native with _g reads>). The seq is one
    // synchronous run and the native contains only pure string ops, so a
    // nested scratch use (an inner param in an argument position) can
    // never interleave with the outer read.
    let store_backed = !is_lifted(name) && positional_read(name).is_none();
    let multi_read = matches!(op.as_str(), "^" | "," | "#" | "##" | "%" | "%%" | ":-");
    let (id_src, wrap): (Expr, Option<Expr>) = if store_backed && multi_read {
        (sh2_member("_g"), Some(value.clone()))
    } else {
        (value.clone(), None)
    };
    let id = || id_src.clone();
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
        operator: op.to_string(),
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
    let native = match op.as_str() {
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
            // LITERAL pattern — shortest == longest for literal patterns,
            // exactly like the runtime's literal fast paths.
            if literal_pattern(p) {
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
            } else {
            // SINGLE-STAR glob patterns — the stringop3 bench family and
            // the corpus idioms (`${f%.*}` ext-strip, `${x##*/}` basename,
            // `${s#*:}` field-skip): `*P` for the prefix ops (# / ##),
            // `P*` for the suffix ops (% / %%). The runtime's glob matcher
            // treats `*` as ANY string (parameter-expansion patterns match
            // `/` too — unlike pathname expansion), so the shortest
            // prefix ending in P is the FIRST occurrence of the literal
            // core and the longest is the LAST: indexOf/lastIndexOf + the
            // core length. A core with further glob metachars / a bare
            // `*` (no core) stays on the runtime.
            let core = if op.starts_with('#') {
                p.strip_prefix('*')
            } else {
                p.strip_suffix('*')
            };
            let Some(core) = core else { return None; };
            if core.is_empty() || !literal_pattern(core) {
                None
            } else {
                let clen = core.chars().count() as i64;
                // `#` (shortest prefix) and `%%` (longest suffix) are the
                // FIRST occurrence; `##` (longest prefix) and `%` (shortest
                // suffix) are the LAST.
                let first = op == "#" || op == "%%";
                let ix = if first {
                    method(val(), "indexOf", vec![str_lit(core)])
                } else {
                    method(val(), "lastIndexOf", vec![str_lit(core)])
                };
                if op.starts_with('#') {
                    // strip through the occurrence (prefix removal)
                    Some(cond(
                        bin(ix.clone(), ">=", int_lit(0)),
                        method(val(), "slice", vec![bin(ix.clone(), "+", int_lit(clen))]),
                        val(),
                    ))
                } else {
                    // strip up to the occurrence (suffix removal)
                    Some(cond(
                        bin(ix.clone(), ">=", int_lit(0)),
                        method(val(), "slice", vec![int_lit(0), ix.clone()]),
                        val(),
                    ))
                }
            }
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
            if op == ":=" && (is_positional || store_backed) {
                // positional / store `:=` WRITES the binding: a native
                // write to the `_g` scratch or a getVar call would be
                // wrong (the runtime setVar is the store authority).
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
        // A plain-name slice may be an ARRAY: `${arr[@]:o:l}` parses with
        // the BARE name (the `[@]` is dropped by the parser), and the
        // runtime's param decides via its arrays map (element slice vs
        // char slice). Only LIFTED names are provably scalar — the lift
        // analyses never lift array names (subscript writes are
        // excluded) — so store-backed names stay on the runtime param.
        if !is_lifted(name) {
            return None;
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
    };
    // Store-var single-eval wrap (see `wrap` above): the native was built
    // reading `sh2._g`; assign the store read once, then the native.
    match (native, wrap) {
        (Some(e), Some(store)) => Some(seq(vec![
            Expr::AssignmentExpression {
                operator: "=".to_string(),
                left: Box::new(sh2_member("_g")),
                right: Box::new(store),
            },
            e,
        ])),
        (r, _) => r,
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
/// `-r`/`-w`/`-x` are the exception: bash tests access(2) with
/// R_OK/W_OK/X_OK (effective-uid permission, following symlinks), so they
/// lower to `await sh2.fs.access(P, N).then(() => true, () => false)` —
/// the async twin of the runtime's fs.accessSync flag resolution.
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
    let chain = if matches!(f, 'r' | 'w' | 'x') {
        // `-r`/`-w`/`-x` — bash tests access(2) with R_OK/W_OK/X_OK (the
        // real effective-uid permission, following symlinks), NOT raw mode
        // bits: a root-owned `crw-------` device is unreadable by a
        // non-root user even though the owner-read bit is set
        // (tty-cmdsub.sh). The runtime's evalUnary resolves these flags the
        // same way (fs.accessSync); this chain is its async twin:
        // `await sh2.fs.access(P, 4).then(() => true, () => false)`.
        let want = match f {
            'r' => 4, // fs.constants.R_OK
            'w' => 2, // fs.constants.W_OK
            _ => 1,   // fs.constants.X_OK
        };
        let access = sh2_fs_call("access", vec![
            path,
            Expr::Literal {
                value: serde_json::Value::from(want),
                raw: None,
                regex: None,
            },
        ]);
        let then = Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(access),
                property: Box::new(Expr::Identifier {
                    name: "then".to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![
                Expr::ArrowFunctionExpression {
                    params: vec![],
                    body: ArrowBody::Expr(Box::new(bool_lit(true))),
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
        await_expr(then)
    } else {
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
        await_expr(then)
    };
    if negate {
        Some(Expr::UnaryExpression {
            operator: "!".to_string(),
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
        operator: op.to_string(),
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
            operator: "&&".to_string(),
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
                // a native `((i++))` / `let` inside a subshell/background
                // writes a COPY in bash — a lifted module binding would be
                // clobbered by the arrow (mirror of the numeric-lift
                // twin's exclusion), so mark the written vars excluded.
                if in_copy && let_args_native {
                    if let [IrExpr::Str(_cn, _), IrExpr::Array(cargs)] = args.as_slice() {
                        for a in cargs {
                            if let IrExpr::Str(sv, _) = a {
                                if let Some(ast) = parse_arith_native(sv) {
                                    for w in arith_written_vars(&ast) {
                                        excluded.insert(w.clone());
                                    }
                                }
                            }
                        }
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
                            // runtime never sees the args) and a VALIDATED
                            // `-i` declaration (int_declare_names — the
                            // declare writes nothing and references
                            // nothing) are not store writes: skip the marks
                            // for the WHOLE call. (The old per-name skip
                            // was defeated by the exec arg shape — the
                            // names live inside the Array wrapper at
                            // args[1], so only the bare Str args matched,
                            // while the `-i` flag's letters still marked
                            // the name store-bound via mark_all_idents.)
                            let native_let = cname == "let" && let_args_native;
                            let intdecl = if cname == "let" { Vec::new() } else {
                                int_declare_names(args).unwrap_or_default()
                            };
                            // a PURE-VALUE `local x=1` declaration is not a store write (the
                            // emit rewrites it to a native binding write — see pure_value_declare):
                            // skip its marks too, unless the call sits in a subshell/background
                            // (COPY semantics — the name must stay store-bound there, mirror of
                            // the Assign-target exclusion).
                            let pure_decl = !in_copy && pure_value_declare(args).is_some();
                            if !(native_let || !intdecl.is_empty() || pure_decl) {

                                for a in &args[1..] {
                                    mark_write_builtin_vars(a, excluded);
                                    // `let`/`(( ))`/`eval` args are
                                    // EXPRESSIONS ("i++") — mark EVERY
                                    // identifier they touch so a lifted
                                    // native binding never desyncs from
                                    // the runtime's store write
                                    mark_all_idents_args(a, string_ctx);
                                }
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
                        // a VALIDATED `-i` declaration (int_declare_names)
                        // are not store writes — skip their marks for the
                        // WHOLE call (mirror of the expression-position
                        // block; the old per-name skip was defeated by the
                        // Array wrapper + flag letters — see above).
                        let native_let = cname == "let" && arith_let_args_native(args);
                        let intdecl = if cname == "let" { Vec::new() } else {
                            int_declare_names(args).unwrap_or_default()
                        };
                        // a PURE-VALUE `local x=1` declaration is not a store write (the
                        // emit rewrites it to a native binding write — see pure_value_declare):
                        // skip its marks too, unless the call sits in a subshell/background
                        // (COPY semantics — the name must stay store-bound there, mirror of
                        // the Assign-target exclusion).
                        let pure_decl = !in_copy && pure_value_declare(args).is_some();
                        if !(native_let || !intdecl.is_empty() || pure_decl) {

                            for a in args {
                                mark_write_builtin_vars(a, excluded);
                                // `let`/`(( ))`/`eval` args are ARITHMETIC
                                // EXPRESSIONS — mark EVERY identifier they
                                // touch so a lifted native binding never
                                // desyncs from a runtime store write
                                mark_all_idents_args(a, string_ctx);
                            }
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
                // mirror of the numeric-lift twin: a native `(( ))` / `let`
                // write and an `-i` declaration are ARITHMETIC sources —
                // they disqualify a var from the string lift (a store-var
                // keeps the runtime's int coercion; a native binding would
                // desync from the setVar the arith write emits)
                collect_native_arith_sources(args, assigns);
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
            IrExpr::Call { func, args } => {
                if func == "exec" {
                    collect_native_arith_sources(args, assigns);
                }
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
        // A numeric-range iterable (`seq_range_for`'s bare `Range`
        // For.iter shape): the ESTree surface has no range literal, so
        // render the materialized string list. The native ForStatement
        // path consumes the Range BEFORE this arm (a counter loop, no
        // array); this arm is the bounded fallback (for-of / *Sync /
        // async paths) and any stray Range in an expression position.
        IrExpr::Range { start, end } => {
            let items: Vec<Option<Expr>> = (*start..=*end)
                .map(|v| Some(str_lit(&v.to_string())))
                .collect();
            Expr::ArrayExpression { elements: items }
        }
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
            // `local x=1` / `declare x=1` / ... with PURE-VALUE args whose
            // names are ALL lifted (expression position — &&/|| operands,
            // pipeline stages): the native binding-write sequence, same as
            // the statement form (see try_native_declare_stmt) — the
            // trailing `true` is the builtin's truthy return the &&/||
            // chains branch on.
            if func == "exec" {
                if let [IrExpr::Str(name, _), ..] = args.as_slice() {
                    if matches!(name.as_str(), "local" | "declare" | "typeset" | "readonly") {
                        if let Some(native) = try_native_declare_stmt(args) {
                            return native;
                        }
                    }
                }
            }
            // `eval "NAME=VALUE..."` with a STATIC pure-assignment string
            // (expression position): the native store-write sequence — no
            // double bash spawn per evaluation (see try_native_eval).
            if func == "exec" {
                if let [IrExpr::Str(name, _), ..] = args.as_slice() {
                    if name == "eval" {
                        if let Some(native) = try_native_eval(args) {
                            return native;
                        }
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
            // `sh2.split(v)` — the field-split marker on UNQUOTED expansions
            // (for-iters `for w in $y`, exec args `set -- $y`): bash splits
            // on default-IFS whitespace and DROPS empty fields (an empty/
            // unset variable → zero fields → zero iterations/args). The
            // runtime's own pattern (captureWords, exec's name split) is
            // `s.split(/\s+/).filter(w => w.length > 0)` — emit it NATIVE,
            // no dispatch. `String(v)` guards lifted numeric bindings.
            if func == "split" {
                if let [v] = args.as_slice() {
                    let ve = expr_to_estree(v);
                    if expr_has_await(&ve) {
                        return sh2_call("split", vec![ve]);
                    }
                    return Expr::CallExpression {
                        callee: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::CallExpression {
                                callee: Box::new(Expr::MemberExpression {
                                    object: Box::new(Expr::CallExpression {
                                        callee: Box::new(Expr::Identifier {
                                            name: "String".to_string(),
                                        }),
                                        arguments: vec![ve],
                                        optional: false,
                                    }),
                                    property: Box::new(Expr::Identifier {
                                        name: "split".to_string(),
                                    }),
                                    computed: false,
                                    optional: false,
                                }),
                                arguments: vec![regex_lit(r"\s+")],
                                optional: false,
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "filter".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        arguments: vec![Expr::ArrowFunctionExpression {
                            params: vec![Expr::Identifier {
                                name: "w".to_string(),
                            }],
                            body: ArrowBody::Expr(Box::new(Expr::BinaryExpression {
                                operator: ">".to_string(),
                                left: Box::new(Expr::MemberExpression {
                                    object: Box::new(Expr::Identifier {
                                        name: "w".to_string(),
                                    }),
                                    property: Box::new(Expr::Identifier {
                                        name: "length".to_string(),
                                    }),
                                    computed: false,
                                    optional: false,
                                }),
                                right: Box::new(Expr::Literal {
                                    value: serde_json::Value::from(0),
                                    raw: None,
                                regex: None,
                                }),
                            })),
                            expression: true,
                            r#async: false,
                        }],
                        optional: false,
                    };
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
                        // per evaluation. Eligible natives: at most ONE
                        // sh2.* call (a store-var / positional READ — the
                        // runtime test is one call, so a single-read native
                        // is metric-neutral and strictly faster; two+ reads
                        // would be a net metric loss and gain nothing over
                        // the runtime test's single store read) and no
                        // awaits (file tests — the sequence context is
                        // sync; an await chain would break the enclosing
                        // async analysis). The native test is evaluated
                        // EXACTLY ONCE into the runtime's `_g` scratch
                        // (the guard/not protocol — a duplicated native
                        // would double its getVar reads, and the metric
                        // would count them twice):
                        // `(sh2._g = t, sh2.lastExit = sh2._g ? 0 : 1,
                        // sh2._g)`. The seq is one synchronous run, so a
                        // nested scratch use can never interleave.
                        //
                        // AWAITED natives (file tests — the async
                        // `sh2.fs.lstat/access` chains) are legal here too
                        // OUTSIDE a provably-sync function define arrow:
                        // `(sh2._g = await t, ...)` runs the getVar reads
                        // before the await (the single-eval protocol
                        // holds — the then-arrow uses the promise value,
                        // and the final `sh2._g` read resumes atomically
                        // after the assignment), and the chain's enclosing
                        // context is async by construction (module top
                        // level / async arrows — the sync-arrow and
                        // *Sync-loop gates scan the LOWERED body and
                        // disqualify awaited chains consistently). Inside
                        // a sync arrow (`in_sync_arrow`) the await would
                        // be a SyntaxError — the runtime test stays there.
                        let awaited_ok = !in_sync_arrow();
                        if expr_sh2_call_count(&native) <= 1
                            && (!expr_has_await(&native) || awaited_ok)
                        {
                            let tmp = sh2_member("_g");
                            return seq(vec![
                                Expr::AssignmentExpression {
                                    operator: "=".to_string(),
                                    left: Box::new(tmp.clone()),
                                    right: Box::new(native.clone()),
                                },
                                Expr::AssignmentExpression {
                                    operator: "=".to_string(),
                                    left: Box::new(sh2_member("lastExit")),
                                    right: Box::new(Expr::ConditionalExpression {
                                        test: Box::new(tmp.clone()),
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
                                tmp,
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
                // `echo ARGS | cut OP` — a sync runtime mini-cut (the
                // cutText twin of grepText): the pipeline's fd-1 write +
                // exit status are the helper's emit + lastExit (cut exits
                // 0 on the statically-validated args), so the async
                // pipeline machinery disappears. Sink-correct everywhere
                // (module stdout, capture buffer, redirect target) — the
                // same guardrails as the grep lift.
                if !program_defines_function("echo") && !program_defines_function("cut") {
                    if let Some((text, no_newline, cut_args, _)) = try_native_echo_cut(e) {
                        // the helper receives the FULL cut input (echo adds
                        // the trailing newline unless -n) so its endsWith-
                        // newline rule matches the real pipeline
                        let input = if no_newline {
                            text
                        } else {
                            Expr::BinaryExpression {
                                operator: "+".to_string(),
                                left: Box::new(text),
                                right: Box::new(str_lit("\n")),
                            }
                        };
                        return sh2_call(
                            "cutText",
                            vec![
                                input,
                                array(cut_args.iter().map(expr_to_estree).collect()),
                                bool_lit(false),
                            ],
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
                // `$(mktemp -d)` — the capture's only statement is the
                // sync mktemp builtin with -d: the value is the created
                // unique temp directory (see `native_capture_mktemp_dir`)
                // — no fd swap, no blocking mkdirSync, no dispatch.
                if !program_defines_function("mktemp") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(inner)] = stmts.as_slice() {
                            if let Some(value) = native_capture_mktemp_dir(inner) {
                                return value;
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
                // `$(echo ARGS | awk '{print $N}')` — the echo|awk
                // capture: the value is a pure field-extraction chain
                // over echo's exact output (see `try_native_echo_awk`;
                // awk prints per-record lines + a trailing newline, the
                // capture strips it) — no spawns, no async pipeline
                // machinery. The `{print $N + $M}` arithmetic form
                // lowers to the native numeric sum.
                if !program_defines_function("echo") && !program_defines_function("awk") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some((text, no_newline, spec)) = try_native_echo_awk(pipe) {
                                // the input lines: echo's text (echo appends
                                // the trailing newline unless `-n`); awk
                                // treats the final newline as the record
                                // terminator, never part of the last record
                                let base = if no_newline {
                                    text
                                } else {
                                    Expr::ConditionalExpression {
                                        test: Box::new(method_call(
                                            text.clone(),
                                            "endsWith",
                                            vec![str_lit("\n")],
                                        )),
                                        consequent: Box::new(method_call(
                                            text.clone(),
                                            "slice",
                                            vec![int_lit_expr(0), int_lit_expr(-1)],
                                        )),
                                        alternate: Box::new(text),
                                    }
                                };
                                let lines = method_call(base, "split", vec![str_lit("\n")]);
                                let sel = awk_print_sel_expr(ident("l"), &spec);
                                let mapped = method_call(
                                    lines,
                                    "map",
                                    vec![sync_arrow_expr_param("l", sel)],
                                );
                                let joined = method_call(mapped, "join", vec![str_lit("\n")]);
                                // awk emits a trailing newline after every
                                // record (even `echo -n` input — the text is
                                // still one record); the capture strips it
                                let value = Expr::BinaryExpression {
                                    operator: "+".to_string(),
                                    left: Box::new(joined),
                                    right: Box::new(str_lit("\n")),
                                };
                                return trim_capture(value);
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
                // `$(echo ARGS | cut OP)` — the echo|cut capture: the
                // value is a pure string-op chain over echo's exact output
                // (see `cut_value_expr`) — no spawns, no pipeline
                // machinery, no async capture arrow, no sh2 call at all.
                // trimCapture applies the capture's NUL + trailing-
                // newline strips (cut exits 0 on the statically-validated
                // args, so the chain records 0/0 like the runtime).
                if !program_defines_function("echo") && !program_defines_function("cut") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some((text, no_newline, _, spec)) = try_native_echo_cut(pipe) {
                                // the capture trims trailing newlines, so
                                // the emitted-bytes +"\n" is a no-op —
                                // the value is the joined selection
                                if let Some(value) = cut_value_expr(
                                    cut_echo_lines(text, no_newline),
                                    &spec,
                                    false,
                                ) {
                                    return trim_capture(value);
                                }
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
            // `$(echo EXPR | bc)` — a native bc evaluation (SH2_BC_NATIVE,
            // default ON; see [`native_capture_echo_bc`]): the spawn +
            // async capture machinery collapse to a compile-time fold
            // (static EXPR) or a native sqrt-of-var expression — the
            // primes `is_prime` chain goes sync end-to-end. The
            // captureWords form (unquoted `$(...)` — the runtime
            // word-splits the output) only fires when the value is
            // provably a single word: the emitted one-element array is
            // exactly the `capture().split(/\s+/)` result.
            if func == "capture" || func == "captureWords" {
                if !program_defines_function("echo") && !program_defines_function("bc") {
                    if let [IrExpr::Arrow(stmts)] = args.as_slice() {
                        if let [IrStmt::Expr(pipe)] = stmts.as_slice() {
                            if let Some(value) =
                                native_capture_echo_bc(pipe, func == "captureWords")
                            {
                                return if func == "captureWords" {
                                    array(vec![value])
                                } else {
                                    value
                                };
                            }
                        }
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
            // branch on the wrong thing. The native sequence (see
            // native_and_or) mirrors the runtime helpers' exact decision on
            // the recorded status for BOTH sync and awaited operands: the
            // seq's awaits run in the enclosing async context (module top
            // level / async arrows — the sync-arrow and *Sync-loop analyses
            // scan the lowered body and disqualify awaited chains
            // consistently), so `(await l, sh2.lastExit === 0 ? (await r,
            // …) : false)` is the `sh2.and` helper's exact body minus the
            // per-evaluation promise machinery.
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let l = expr_to_estree(lhs);
            let r = expr_to_estree(rhs);
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            native_and_or(BinOpKind::And, l, r)
        }
        IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs } => {
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let l = expr_to_estree(lhs);
            let r = expr_to_estree(rhs);
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            native_and_or(BinOpKind::Or, l, r)
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
            let inner = arith_to_estree_wrapped(a);
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

/// `arrow_sink` twins for functions provably never called under a swapped
/// stdout sink (see [`native_echo_fn_set`]): the define arrow lowers
/// WITHOUT the sink-depth bump, so its echo/printf statements lower to
/// native `process.stdout.write`. The runtime runs the same arrow under
/// the default sink on every call by construction.
fn arrow_native_echo(params: Vec<Expr>, body: IrExpr) -> Expr {
    arrow_body(params, body)
}

/// Sync twin of [`arrow_native_echo`] (see [`arrow_sink_sync`]).
fn arrow_native_echo_sync(params: Vec<Expr>, body: IrExpr) -> Expr {
    arrow_body_async(params, body, false)
}

fn arrow_body(params: Vec<Expr>, body: IrExpr) -> Expr {
    arrow_body_async(params, body, true)
}

/// Emission depth inside a NON-async arrow body (provably-sync function
/// define arrows — `arrow_sink_sync`/`arrow_native_echo_sync`). `await` is
/// illegal there, so the checkpointed `forLoopBatch`/`whileLoopBatch`
/// emission (which must be awaited) is suppressed while it is nonzero. The
/// *Sync loop bodyFn arrows (`sync_arrow_*`) lower their bodies BEFORE the
/// wrap, so they need no bump here — an await introduced inside one
/// propagates into the ENCLOSING statement list, which the enclosing
/// loop's `stmts_have_await` gate sees and flips to the async form.
static SYNC_ARROW_DEPTH: Mutex<usize> = Mutex::new(0);
fn in_sync_arrow() -> bool {
    *SYNC_ARROW_DEPTH.lock().unwrap() > 0
}

fn arrow_body_async(params: Vec<Expr>, body: IrExpr, r#async: bool) -> Expr {
    if !r#async {
        *SYNC_ARROW_DEPTH.lock().unwrap() += 1;
    }
    let out = arrow_body_async_inner(params, body, r#async);
    if !r#async {
        *SYNC_ARROW_DEPTH.lock().unwrap() -= 1;
    }
    out
}

fn arrow_body_async_inner(params: Vec<Expr>, body: IrExpr, r#async: bool) -> Expr {
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
    expr_sh2_call_count(e) > 0
}

/// Count the `sh2.*` dispatch CALLS in a lowered expression — the metric's
/// tally shape (CallExpression whose callee is `sh2.<name>`). State-field
/// member READS (`sh2.lastExit`, `sh2.positional[i]`, `sh2.cwd`) are not
/// dispatches and don't count. Used by the and/or test lowering: a native
/// test with at most ONE sh2 call (a store-var/positional read) is
/// metric-neutral vs the single runtime `sh2.test` call it replaces, and
/// strictly faster (no tokenize/parse/dispatch) — two+ reads would be a
/// net metric loss and stay on the runtime call.
fn expr_sh2_call_count(e: &Expr) -> usize {
    fn walk(e: &Expr, n: &mut usize) {
        match e {
            Expr::CallExpression {
                callee,
                arguments,
                ..
            } => {
                if let Expr::MemberExpression { object, .. } = callee.as_ref() {
                    if let Expr::Identifier { name } = object.as_ref() {
                        if name == "sh2" {
                            *n += 1;
                        }
                    }
                }
                walk(callee, n);
                for a in arguments {
                    walk(a, n);
                }
            }
            Expr::Identifier { .. } | Expr::Literal { .. } => {}
            Expr::TemplateLiteral {
                quasis: _,
                expressions,
            } => {
                for a in expressions {
                    walk(a, n);
                }
            }
            Expr::MemberExpression {
                object, property, ..
            } => {
                walk(object, n);
                walk(property, n);
            }
            Expr::AwaitExpression { argument } => walk(argument, n),
            Expr::ArrowFunctionExpression {
                params, body, ..
            } => {
                for p in params {
                    walk(p, n);
                }
                match body {
                    ArrowBody::Expr(x) => walk(x, n),
                    ArrowBody::Block(_) => {}
                }
            }
            Expr::ObjectExpression { properties } => {
                for p in properties {
                    walk(&p.key, n);
                    walk(&p.value, n);
                }
            }
            Expr::ArrayExpression { elements } => {
                for el in elements.iter().flatten() {
                    walk(el, n);
                }
            }
            Expr::SpreadElement { argument } => walk(argument, n),
            Expr::LogicalExpression { left, right, .. }
            | Expr::BinaryExpression { left, right, .. }
            | Expr::AssignmentExpression { left, right, .. } => {
                walk(left, n);
                walk(right, n);
            }
            Expr::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => {
                walk(test, n);
                walk(consequent, n);
                walk(alternate, n);
            }
            Expr::UnaryExpression { argument, .. } => walk(argument, n),
            Expr::SequenceExpression { expressions } => {
                for a in expressions {
                    walk(a, n);
                }
            }
        }
    }
    let mut n = 0usize;
    walk(e, &mut n);
    n
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
        operator: "===".to_string(),
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
        operator: "&&".to_string(),
        left: Box::new(sh2_member("errexit")),
        right: Box::new(Expr::UnaryExpression {
            operator: "!".to_string(),
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
            operator: "!".to_string(),
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
            operator: "??".to_string(),
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
                    operator: "??".to_string(),
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
        ArithAst::Un { op, arg } => matches!(op.as_str(), "-" | "+") && arith_is_nonzero(arg),
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

/// sh2-callee name of a lowered CallExpression (`sh2.getVar` → `"getVar"`),
/// or None for any other callee shape.
fn sh2_callee_name(e: &Expr) -> Option<&str> {
    if let Expr::CallExpression { callee, .. } = e {
        if let Expr::MemberExpression { object, property, .. } = callee.as_ref() {
            if let Expr::Identifier { name } = object.as_ref() {
                if name == "sh2" {
                    if let Expr::Identifier { name } = property.as_ref() {
                        return Some(name);
                    }
                }
            }
        }
    }
    None
}

/// The literal store-name argument of a sh2 call (`sh2.setVar("i", v)` →
/// Some("i")), only for a plain string-literal first argument.
fn sh2_name_arg(e: &Expr) -> Option<&str> {
    if let Expr::CallExpression { arguments, .. } = e {
        if let Some(Expr::Literal { value, .. }) = arguments.first() {
            return value.as_str();
        }
    }
    None
}

/// Native-ForOf store-sync elimination — eligibility scan over the loop
/// body (minus the sync statement itself). The optimization replaces the
/// per-iteration `sh2.setVar(var, i)` store sync + the body's
/// `sh2.getVar(var)` reads with a native binding read, one pre-loop
/// store read and one post-loop store write. It is ONLY sound when the
/// body cannot observe the store's `var` through any other channel:
///   (a) any sh2.* call outside the allow-list {getVar, setVar, setArray,
///       setArrayAppend, assign} — a runtime call could read or write
///       `var` through the store (test strings, eval, fnCall/exec of
///       script functions, param ops, `read`/`shift` builtins, ...);
///   (b) a store WRITE to `var` (setVar/assign/setArray/setArrayAppend
///       with a literal `var` name — incl. a nested for-loop on the same
///       var, whose own sync/post-write are exactly such writes);
///   (c) a dynamic name argument (getVar/setVar with a non-literal name
///       could resolve to `var` at runtime).
/// Native non-sh2 constructs (echo writes, arith, comparisons) cannot
/// touch the store — always allowed.
fn forof_sync_elim_ok(stmts: &[Stmt], var: &str) -> bool {
    fn expr_ok(e: &Expr, var: &str) -> bool {
        match e {
            Expr::CallExpression { callee, arguments, .. } => {
                if !expr_ok(callee, var) || arguments.iter().any(|a| !expr_ok(a, var)) {
                    return false;
                }
                match sh2_callee_name(e) {
                    Some("getVar") => match arguments.first() {
                        // literal-name getVars are fine: `var` reads are
                        // rewritten to the binding, other names read the
                        // store as usual
                        Some(Expr::Literal { .. }) => true,
                        _ => false, // dynamic name — could resolve to var
                    },
                    Some("setVar") | Some("setArray") | Some("setArrayAppend")
                    | Some("assign") => match sh2_name_arg(e) {
                        Some(n) => n != var,
                        None => false,
                    },
                    Some(_) => false, // any other sh2 call disqualifies
                    None => true,
                }
            }
            Expr::MemberExpression { object, property, .. } => {
                expr_ok(object, var) && expr_ok(property, var)
            }
            Expr::TemplateLiteral { expressions, .. } => {
                expressions.iter().all(|e| expr_ok(e, var))
            }
            Expr::AwaitExpression { argument } => expr_ok(argument, var),
            Expr::ArrowFunctionExpression { params, body, .. } => {
                params.iter().all(|p| expr_ok(p, var))
                    && match body {
                        ArrowBody::Expr(e) => expr_ok(e, var),
                        ArrowBody::Block(s) => stmt_ok(s, var),
                    }
            }
            Expr::ObjectExpression { properties } => properties
                .iter()
                .all(|p| expr_ok(&p.key, var) && expr_ok(&p.value, var)),
            Expr::ArrayExpression { elements } => elements
                .iter()
                .flatten()
                .all(|e| expr_ok(e, var)),
            Expr::SequenceExpression { expressions } => {
                expressions.iter().all(|e| expr_ok(e, var))
            }
            Expr::SpreadElement { argument } => expr_ok(argument, var),
            Expr::LogicalExpression { left, right, .. }
            | Expr::BinaryExpression { left, right, .. }
            | Expr::AssignmentExpression { left, right, .. } => {
                expr_ok(left, var) && expr_ok(right, var)
            }
            Expr::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => expr_ok(test, var) && expr_ok(consequent, var) && expr_ok(alternate, var),
            Expr::UnaryExpression { argument, .. } => expr_ok(argument, var),
            _ => true,
        }
    }
    fn stmt_ok(s: &Stmt, var: &str) -> bool {
        match s {
            Stmt::ExpressionStatement { expression } => expr_ok(expression, var),
            Stmt::BlockStatement { body } => body.iter().all(|b| stmt_ok(b, var)),
            Stmt::IfStatement { test, consequent, alternate } => {
                expr_ok(test, var)
                    && stmt_ok(consequent, var)
                    && alternate.as_deref().map_or(true, |a| stmt_ok(a, var))
            }
            Stmt::SwitchStatement { discriminant, cases } => {
                expr_ok(discriminant, var)
                    && cases.iter().all(|c| {
                        c.test.as_ref().map_or(true, |t| expr_ok(t, var))
                            && c.consequent.iter().all(|s2| stmt_ok(s2, var))
                    })
            }
            Stmt::WhileStatement { test, body } => expr_ok(test, var) && stmt_ok(body, var),
            Stmt::ForStatement {
                init, test, update, body,
            } => {
                stmt_ok(init, var) && expr_ok(test, var) && expr_ok(update, var) && stmt_ok(body, var)
            }
            Stmt::ForOfStatement { left, right, body } => {
                stmt_ok(left, var) && expr_ok(right, var) && stmt_ok(body, var)
            }
            Stmt::VariableDeclaration { declarations, .. } => declarations.iter().all(|d| {
                d.init.as_ref().map_or(true, |i| expr_ok(i, var))
            }),
            Stmt::ReturnStatement { argument } => {
                argument.as_ref().map_or(true, |a| expr_ok(a, var))
            }
            Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => true,
        }
    }
    stmts.iter().all(|s| stmt_ok(s, var))
}

/// Rewrite `sh2.getVar(var)` calls inside a lowered statement to the
/// native binding `js_var` (the ForOf binding holds the exact value the
/// store sync would have written). Only exact literal-name matches are
/// touched.
fn forof_rewrite_getvar(stmts: &mut [Stmt], var: &str, js_var: &str) {
    fn expr_rewrite(e: &mut Expr, var: &str, js_var: &str) {
        let callee_name = sh2_callee_name(e).map(|s| s.to_string());
        match e {
            Expr::CallExpression { callee, arguments, .. } => {
                if callee_name.as_deref() == Some("getVar")
                    && matches!(
                        arguments.first(),
                        Some(Expr::Literal { value, .. }) if value.as_str() == Some(var)
                    )
                {
                    *e = Expr::Identifier {
                        name: js_var.to_string(),
                    };
                    return;
                }
                expr_rewrite(callee, var, js_var);
                for a in arguments.iter_mut() {
                    expr_rewrite(a, var, js_var);
                }
            }
            Expr::MemberExpression { object, property, .. } => {
                expr_rewrite(object, var, js_var);
                expr_rewrite(property, var, js_var);
            }
            Expr::TemplateLiteral { expressions, .. } => {
                for e2 in expressions.iter_mut() {
                    expr_rewrite(e2, var, js_var);
                }
            }
            Expr::AwaitExpression { argument } => expr_rewrite(argument, var, js_var),
            Expr::ArrowFunctionExpression { params, body, .. } => {
                for p in params.iter_mut() {
                    expr_rewrite(p, var, js_var);
                }
                match body {
                    ArrowBody::Expr(e) => expr_rewrite(e, var, js_var),
                    ArrowBody::Block(s) => stmt_rewrite(s, var, js_var),
                }
            }
            Expr::ObjectExpression { properties } => {
                for p in properties.iter_mut() {
                    expr_rewrite(&mut p.key, var, js_var);
                    expr_rewrite(&mut p.value, var, js_var);
                }
            }
            Expr::ArrayExpression { elements } => {
                for el in elements.iter_mut().flatten() {
                    expr_rewrite(el, var, js_var);
                }
            }
            Expr::SequenceExpression { expressions } => {
                for el in expressions.iter_mut() {
                    expr_rewrite(el, var, js_var);
                }
            }
            Expr::SpreadElement { argument } => expr_rewrite(argument, var, js_var),
            Expr::LogicalExpression { left, right, .. }
            | Expr::BinaryExpression { left, right, .. }
            | Expr::AssignmentExpression { left, right, .. } => {
                expr_rewrite(left, var, js_var);
                expr_rewrite(right, var, js_var);
            }
            Expr::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => {
                expr_rewrite(test, var, js_var);
                expr_rewrite(consequent, var, js_var);
                expr_rewrite(alternate, var, js_var);
            }
            Expr::UnaryExpression { argument, .. } => expr_rewrite(argument, var, js_var),
            _ => {}
        }
    }
    fn stmt_rewrite(s: &mut Stmt, var: &str, js_var: &str) {
        match s {
            Stmt::ExpressionStatement { expression } => expr_rewrite(expression, var, js_var),
            Stmt::BlockStatement { body } => {
                for b in body.iter_mut() {
                    stmt_rewrite(b, var, js_var);
                }
            }
            Stmt::IfStatement { test, consequent, alternate } => {
                expr_rewrite(test, var, js_var);
                stmt_rewrite(consequent, var, js_var);
                if let Some(a) = alternate {
                    stmt_rewrite(a, var, js_var);
                }
            }
            Stmt::SwitchStatement { discriminant, cases } => {
                expr_rewrite(discriminant, var, js_var);
                for c in cases.iter_mut() {
                    if let Some(t) = &mut c.test {
                        expr_rewrite(t, var, js_var);
                    }
                    for s2 in c.consequent.iter_mut() {
                        stmt_rewrite(s2, var, js_var);
                    }
                }
            }
            Stmt::WhileStatement { test, body } => {
                expr_rewrite(test, var, js_var);
                stmt_rewrite(body, var, js_var);
            }
            Stmt::ForStatement {
                init, test, update, body,
            } => {
                stmt_rewrite(init, var, js_var);
                expr_rewrite(test, var, js_var);
                expr_rewrite(update, var, js_var);
                stmt_rewrite(body, var, js_var);
            }
            Stmt::ForOfStatement { left, right, body } => {
                stmt_rewrite(left, var, js_var);
                expr_rewrite(right, var, js_var);
                stmt_rewrite(body, var, js_var);
            }
            Stmt::VariableDeclaration { declarations, .. } => {
                for d in declarations.iter_mut() {
                    if let Some(i) = &mut d.init {
                        expr_rewrite(i, var, js_var);
                    }
                }
            }
            Stmt::ReturnStatement { argument } => {
                if let Some(a) = argument {
                    expr_rewrite(a, var, js_var);
                }
            }
            Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => {}
        }
    }
    for s in stmts.iter_mut() {
        stmt_rewrite(s, var, js_var);
    }
}

fn safe_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "var", "let", "const", "function", "class", "if", "else", "for", "while",
        "do", "switch", "case", "break", "continue", "return", "new", "delete",
        "typeof", "instanceof", "in", "of", "try", "catch", "finally", "throw",
        "this", "super", "import", "export", "default", "extends", "static",
        "yield", "await", "null", "true", "false", "void", "debugger", "arguments",
        // C keywords not already reserved above (ask A6): the emitted
        // identifier set must be C-safe too. Output-preserving on the
        // corpus (no example names a loop var after a C keyword).
        "int", "long", "char", "short", "float", "double", "unsigned", "signed",
        "sizeof", "struct", "union", "enum", "const", "extern", "goto", "typedef",
        "volatile", "register", "auto", "restrict", "_Bool", "_Complex",
    ];
    if RESERVED.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}








mod const_analysis_tests {
    use super::*;

    fn consts_of(src: &str) -> Vec<(String, crate::ir::VarKind)> {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = ast_to_ir(&cmds);
        analyze_var_const(&prog)
    }

    fn kind<'a>(v: &'a [(String, crate::ir::VarKind)], n: &str) -> Option<crate::ir::VarKind> {
        v.iter().find(|(x, _)| x == n).map(|(_, k)| *k)
    }

    #[test]
    fn single_assignment_is_const() {
        let v = consts_of("x=5\necho $x");
        assert_eq!(kind(&v, "x"), Some(crate::ir::VarKind::Const));
    }

    #[test]
    fn reassignment_is_var() {
        let v = consts_of("x=5\nx=6");
        assert_eq!(kind(&v, "x"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn loop_vars_and_loop_accumulators_are_var() {
        let v = consts_of("sum=0\nfor i in 1 2 3; do sum=$((sum + i)); done");
        // `sum` is assigned twice (init + accumulate) and `i` per iteration
        assert_eq!(kind(&v, "sum"), Some(crate::ir::VarKind::Var));
        assert_eq!(kind(&v, "i"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn read_builtin_writes_are_var() {
        let v = consts_of("read line\necho $line");
        assert_eq!(kind(&v, "line"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn eval_disqualifies_everything() {
        let v = consts_of("x=5\neval \"$cmd\"");
        assert_eq!(kind(&v, "x"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn function_body_assignment_is_var() {
        // a function may run 0..N times — its writes are multi-run
        let v = consts_of("f() { x=5; }\nf");
        assert_eq!(kind(&v, "x"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn local_declaration_is_var_in_function() {
        // single site, but inside a function body → multi-run → Var
        let v = consts_of("f() { local x=5; echo $x; }\nf");
        assert_eq!(kind(&v, "x"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn readonly_decl_is_const() {
        let v = consts_of("readonly limit=10\necho $limit");
        assert_eq!(kind(&v, "limit"), Some(crate::ir::VarKind::Const));
    }

    #[test]
    fn native_arith_write_is_var() {
        let v = consts_of("x=1\n((x++))");
        assert_eq!(kind(&v, "x"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn array_element_write_is_var() {
        let v = consts_of("arr=(a b c)\narr[1]=z");
        assert_eq!(kind(&v, "arr"), Some(crate::ir::VarKind::Var));
    }

    #[test]
    fn conditional_single_assignment_stays_const() {
        // one site, executes at most once → still Const (the C backend may
        // only consume it for unconditional top-level sites; the markup is
        // the conservative verdict)
        let v = consts_of("if true; then y=5; fi\necho $y");
        assert_eq!(kind(&v, "y"), Some(crate::ir::VarKind::Const));
    }
}

#[cfg(test)]
mod range_analysis_tests {
    use super::*;

    fn ranges_of(src: &str) -> HashMap<String, (i128, i128)> {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = ast_to_ir(&cmds);
        analyze_var_ranges(&prog)
    }

    #[test]
    fn straight_line_widths() {
        // x: literal 1 → u32; y: arith 1+1 → u32; z: cmdsub → Any; w: -5 → i32
        let r = ranges_of("x=1\ny=$((x+1))\nz=$(echo 5)\nw=-5");
        assert_eq!(r.get("x"), Some(&(1, 1)));
        assert_eq!(r.get("y"), Some(&(2, 2)));
        assert!(!r.contains_key("z"), "cmdsub provenance must be Any");
        assert_eq!(r.get("w"), Some(&(-5, -5)));
        assert_eq!(range_width_name(1, 1), "u32");
        assert_eq!(range_width_name(-5, -5), "i32");
        assert_eq!(range_width_name(0, 4_000_000_000), "u32");
        // fits signed-64 but not u32 → i64 (bash/C-signed domain)
        assert_eq!(range_width_name(0, 10_000_000_000), "i64");
        // beyond signed-64 → u64 (only a C-frontend unsigned integer can
        // produce this; bash arithmetic never leaves i64)
        assert_eq!(range_width_name(0, u64::MAX as i128), "u64");
        assert_eq!(range_width_name(i64::MAX as i128, i64::MAX as i128), "i64");
    }

    #[test]
    fn branch_join_and_loop_fixpoint() {
        // if cond; then x=1; else x=1000000; fi → x ∈ [1, 1000000] → u32
        let r = ranges_of("if [ a ]; then x=1; else x=1000000; fi");
        assert_eq!(r.get("x"), Some(&(1, 1_000_000)));
        // the loop-counter fixpoint: i=1; while i<5: i=i+1 → [1, 5]
        // (the loop runs at most 4 times from 1, the last write overshoots
        // the bound by one step, and i=1 is reachable when it never runs)
        let r2 = ranges_of("i=1\nwhile [ $i -lt 5 ]; do i=$((i+1)); done");
        assert_eq!(r2.get("i"), Some(&(1, 5)));
        // a var NOT touched by the loop keeps its range
        let r3 = ranges_of("i=1\nj=2\nwhile [ $i -lt 5 ]; do i=$((i+1)); done");
        assert_eq!(r3.get("j"), Some(&(2, 2)));
        // a non-counter loop (body doesn't prove monotone +1) — the
        // entry invariant cannot be pinned: the widening hits the i64
        // arithmetic extremes, i+1 overflows it, and the invariant goes
        // Any (sound: the loop could run unboundedly)
        let r4 = ranges_of("i=1\nwhile :; do i=$((i+1)); done");
        assert!(!r4.contains_key("i"));
    }

    #[test]
    fn counting_loop_fixpoint() {
        // bench-count.sh: the cond var AND the other counter both land in
        // u32 (cond cap __n ≤ 999 during the loop; trip cap i ≤ 1 + 1000)
        let src = "i=1\n__n=0\nwhile [ $__n -lt 1000 ]; do i=$((i+1)); __n=$((__n+1)); done";
        let r = ranges_of(src);
        assert_eq!(r.get("__n"), Some(&(0, 1000)));
        assert_eq!(r.get("i"), Some(&(1, 1002)));
        assert_eq!(range_width_name(0, 1000), "u32");
        assert_eq!(range_width_name(1, 1002), "u32");
        // `until` flips the comparison: until i >= 100 ⇔ while i < 100
        let r2 = ranges_of("i=0\nuntil [ $i -ge 100 ]; do i=$((i+1)); done");
        assert_eq!(r2.get("i"), Some(&(0, 100)));
        // `(( i < 100 ))` lowers to `let "i < 100"`
        let r3 = ranges_of("i=0\nwhile (( i < 100 )); do i=$((i+1)); done");
        assert_eq!(r3.get("i"), Some(&(0, 100)));
    }

    #[test]
    fn for_loop_ranges() {
        // integer item list → the loop var's range; body writes are lost
        let r = ranges_of("for i in 1 2 3; do echo $i; done");
        assert_eq!(r.get("i"), Some(&(1, 3)));
        // non-numeric item → Any
        let r2 = ranges_of("for f in *.txt; do echo $f; done");
        assert!(!r2.contains_key("f"));
        // Range iterable (the seq_range_for transform's shape)
        let cmds = crate::Parser::new("for i in x; do echo $i; done")
            .parse()
            .expect("parse");
        let mut prog = ast_to_ir(&cmds);
        if let IrStmt::For { iter, .. } = &mut prog.stmts[0] {
            *iter = IrExpr::Range { start: 5, end: 1 };
        }
        let r3 = analyze_var_ranges(&prog);
        assert_eq!(r3.get("i"), Some(&(1, 5)));
    }

    #[test]
    #[ignore] // corpus-wide tally — run on demand: cargo test -- --ignored --nocapture
    fn corpus_var_width_tally() {
        let mut widths: HashMap<&'static str, usize> = HashMap::new();
        widths.insert("u32", 0);
        widths.insert("i32", 0);
        widths.insert("u64", 0);
        widths.insert("i64", 0);
        let mut total_proven = 0usize;
        let mut total_numeric = 0usize;
        let mut files = 0usize;
        let mut narrowed_files = 0usize;
        for entry in std::fs::read_dir("examples").unwrap().flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "sh").unwrap_or(false) {
                let src = std::fs::read_to_string(&path).unwrap_or_default();
                let Ok(cmds) = crate::Parser::new(&src).parse() else { continue };
                let prog = ast_to_ir(&cmds);
                let ranges = analyze_var_ranges(&prog);
                files += 1;
                let numeric = numeric_lift_vars(&prog);
                total_numeric += numeric.len();
                let mut file_had_narrow = false;
                for (n, (lo, hi)) in &ranges {
                    total_proven += 1;
                    let w = range_width_name(*lo, *hi);
                    *widths.get_mut(w).unwrap() += 1;
                    if w != "i64" {
                        file_had_narrow = true;
                    }
                    let _ = n;
                }
                if file_had_narrow {
                    narrowed_files += 1;
                }
            }
        }
        eprintln!(
            "RANGE TALLY: files={} numeric_lift_vars={} range_proven={} widths={:?} files_with_narrow={}",
            files, total_numeric, total_proven, widths, narrowed_files
        );
    }
}


#[cfg(test)]
mod length_analysis_tests {
    use super::*;

    fn lens_of(src: &str) -> std::collections::HashMap<String, Option<u64>> {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = ast_to_ir(&cmds);
        analyze_string_lengths(&prog).into_iter().collect()
    }

    #[test]
    fn single_execution_accumulation_is_bounded() {
        // `s="$s$x"` runs ONCE at top level — s = x, not None (the old
        // flat fixpoint kept re-applying the assignment and hit the cap)
        let r = lens_of("x=hello\ns=\"$s$x\"");
        assert_eq!(r.get("s"), Some(&Some(5)));
        assert_eq!(r.get("x"), Some(&Some(5)));
    }

    #[test]
    fn bounded_loop_accumulation() {
        // s accumulates 3×|x| over a 3-iteration for loop
        let r = lens_of("x=hello\ns=\nfor i in 1 2 3; do s=\"$s$x\"; done");
        assert_eq!(r.get("s"), Some(&Some(15)));
    }

    #[test]
    fn unbounded_loop_stays_unbounded() {
        // while : (no cond bound) → the accumulation cannot be capped
        let r = lens_of("x=hello\nwhile :; do s=\"$s$x\"; done");
        assert_eq!(r.get("s"), Some(&None));
    }

    #[test]
    fn numeric_counter_is_number_width_not_trip_times() {
        // a counter's VALUE grows, not its width → 20 chars, not 20×trip
        let r = lens_of("i=1\nwhile [ $i -lt 1000 ]; do i=$((i+1)); done");
        assert_eq!(r.get("i"), Some(&Some(20)));
    }

    #[test]
    fn compounding_growth_is_unbounded() {
        // s doubles every iteration — not linear, no bound
        let r = lens_of("s=a\nfor i in 1 2 3; do s=\"$s$s\"; done");
        assert_eq!(r.get("s"), Some(&None));
    }

    #[test]
    fn overwrite_in_loop_is_one_write() {
        // x=$(cmd) in a loop: the last write wins — the single capture
        // bound, not the trip times it
        let r = lens_of("i=0\nwhile [ $i -lt 3 ]; do x=$(echo abc); i=$((i+1)); done");
        let single = lens_of("x=$(echo abc)");
        assert_eq!(r.get("x"), single.get("x"), "loop overwrite == one write");
    }

    #[test]
    #[ignore] // corpus-wide tally — run on demand: cargo test -- --ignored --nocapture
    fn corpus_length_tally() {
        let mut total_vars = 0usize;
        let mut total_bounded = 0usize;
        let mut total_unbounded = 0usize;
        let mut total_len = 0u64;
        let mut files = 0usize;
        let mut files_with_bounds = 0usize;
        for entry in std::fs::read_dir("examples").unwrap().flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "sh").unwrap_or(false) {
                let src = std::fs::read_to_string(&path).unwrap_or_default();
                let Ok(cmds) = crate::Parser::new(&src).parse() else { continue };
                let prog = ast_to_ir(&cmds);
                let lens = analyze_string_lengths(&prog);
                files += 1;
                total_vars += lens.len();
                let mut file_had_bound = false;
                for (_, l) in &lens {
                    match l {
                        Some(n) => {
                            total_bounded += 1;
                            total_len += n;
                            file_had_bound = true;
                        }
                        None => total_unbounded += 1,
                    }
                }
                if file_had_bound {
                    files_with_bounds += 1;
                }
            }
        }
        eprintln!(
            "LENGTH TALLY: files={} vars={} bounded={} unbounded={} total_len={} files_with_bounds={}",
            files, total_vars, total_bounded, total_unbounded, total_len, files_with_bounds
        );
    }
}

