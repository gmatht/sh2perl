//! sync-ok-loops — do not break up a loop when it is cheap, or when it
//! produces no output; checkpoint (yield every 1024 iterations) complex
//! loops instead of awaiting per iteration.
//!
//! ## The three criteria (architect-specified)
//!
//! 1. **~200ms confidence** — if we can be confident the WHOLE loop will
//!    complete in about 200ms, do not break it up at all (a ≤200ms
//!    event-loop block is acceptable; the per-iteration await is pure
//!    overhead). Verdict: `SYNC_OK`.
//! 2. **No observable output** — if the loop body has no echo/printf /
//!    fd-1/fd-2 writes / capture / spawn, there is nothing for a
//!    responsive event loop to interleave with, so breaking it up is
//!    pointless. Verdict: `SYNC_OK`.
//! 3. **Complex loops get a cheap checkpoint** — a long/unbounded loop
//!    that DOES produce output keeps bash's output order but yields to
//!    the event loop every [`BATCH`] iterations: `if (i & (BATCH-1)) ===
//!    0 { await tick; }` — the `if i % 1000 = 0` idea, sharpened: BATCH
//!    is a POWER OF TWO (1024) so the checkpoint is a BITMASK AND, one
//!    op per iteration (~ns, "without slowing them down too much"), not
//!    a modulo. (Even cheaper: a plain counter `if (++n === 1024) { n =
//!    0; await tick; }` — one compare + branch — the bitmask form is for
//!    the i-based phrasing.) Replaces the current per-iteration await.
//!    Verdict: `BATCH_OK` (the renderer emits the checkpointed runtime
//!    form).
//!
//! ## Mechanism (analysis-only; the verdicts drive the renderer)
//!
//! This transform does NOT structurally mutate the IR (the tree must stay
//! put so the verdict pointers match emission — the same stability
//! guarantee the `lastexit_dead`/`ASYNC_REGION_LOOPS` verdicts rely on,
//! shir.rs:97). It walks the tree (STATEMENTS AND EXPRESSIONS — loops
//! hide inside captures, arrows, brace calls, loop conds), computes the
//! verdict for every loop, and stores pointer keys in the module statics
//! below — mirroring the established `ASYNC_REGION_LOOPS` pattern (a
//! `Mutex<Option<HashSet>>` set once per compilation in `ast_to_ir`, read
//! by the renderer).
//!
//! Required renderer hooks (the minimal core change — estree worker):
//! - `shir.rs` `compute_async_region_loops` (~line 5595): a loop the
//!   transform marked `sync_ok` is NOT an async-region loop:
//!   `if in_async && !crate::transforms::sync_ok_loops::sync_ok(st) { … }`
//!   — the existing sync gate (shir.rs ~7440) then emits it with the
//!   EXISTING `forLoopSync`/native for-of path (zero new emission code
//!   for criterion 1+2).
//! - the For/While lowering: a `batch_ok` loop that still fails the sync
//!   gate (async region / glob iter / signals) is emitted as a
//!   checkpointed runtime call instead of the per-iteration `forLoop`:
//!   `sh2.forLoopBatch(iter, body, 1024)`.
//! - `harness/sh2-namespace.mjs`: `forLoopBatch` mirrors `forLoopSync`
//!   (same body execution, same flatten) but counts iterations and awaits
//!   `setImmediate` every 1024 — the `i % 1024 = 0` checkpoint (counter
//!   form, or `(i & 1023) === 0`). Whitelist the name in
//!   `harness/estree_gate.pl`.
//!
//! Corpus gate: `./fail-estree` must stay at the trusted baseline. The
//! batching changes only INTERLEAVING (yield cadence), not the bytes each
//! body emits, so sequential scripts are unaffected; background-job
//! timing edge cases are what the bisect blames on.
//!
//! Register: `("sync-ok-loops", sync_ok_loops::transform),` (the estree
//! worker appends this to `transforms::all()` when it compile-ins the
//! file). Gated by `DEBASHC_TRANSFORMS` like every transform.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::ir::{IrExpr, IrStmt};

/// The checkpoint cadence — a POWER OF TWO (1024) so the runtime's
/// per-iteration check is a bitmask `(i & (BATCH-1)) === 0` (or a plain
/// counter compare), NOT a modulo. 1024 sync iterations ≈ µs-ms of
/// event-loop block for a typical body; a runtime-side per-chunk timer
/// (shrink if a chunk exceeds ~5ms) is a later refinement.
pub const BATCH: u64 = 1024;

/// Cost upper bound for criterion 1: we only mark a loop SYNC_OK when
/// `trip_count × per_iter_cost ≤ 200ms` (the "about 200ms" budget).
const BUDGET_US: u64 = 200_000;
/// Per-statement upper-bound cost (µs): a sync builtin (echo/arith/
/// setVar) in the runtime is a function call + string op ≈ 1-5µs.
const STMT_COST_US: u64 = 3;

// ── Verdicts (pointer-keyed, mirroring ASYNC_REGION_LOOPS) ───────────
// Set once per compilation by `transform` (ast_to_ir), read by the
// renderer hooks. Empty when the transform is disabled (the
// DEBASHC_TRANSFORMS gate skips `transform`, so the statics stay None).
static SYNC_OK_LOOPS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
static BATCH_OK_LOOPS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

/// Renderer hook: is this loop marked "do not break up" (run as ONE sync
/// chunk)? Read by `compute_async_region_loops` + the loop sync gate.
pub fn sync_ok(st: &IrStmt) -> bool {
    SYNC_OK_LOOPS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(&(st as *const IrStmt as usize)))
        .unwrap_or(false)
}

/// Renderer hook: is this loop marked "checkpointed" (sync chunks of
/// [`BATCH`] with a yield)? Read by the For/While lowering.
pub fn batch_ok(st: &IrStmt) -> bool {
    BATCH_OK_LOOPS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(&(st as *const IrStmt as usize)))
        .unwrap_or(false)
}

/// Worker visibility: (sync_ok count, batch_ok count) — lets the estree
/// worker confirm the transform is LIVE (its verdicts reach the renderer)
/// rather than silently no-op'ing on a pointer mismatch.
pub fn stats() -> (usize, usize) {
    let s = SYNC_OK_LOOPS.lock().unwrap();
    let b = BATCH_OK_LOOPS.lock().unwrap();
    (
        s.as_ref().map(|x| x.len()).unwrap_or(0),
        b.as_ref().map(|x| x.len()).unwrap_or(0),
    )
}

/// The transform entry: walk the whole tree (stmts + exprs), compute the
/// verdict for every loop, store the pointer keys. ANALYSIS-ONLY — no
/// structural mutation (see the module doc: pointer stability). Returns
/// true when at least one loop was flagged (the transform is active).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    apply_to(stmts)
}

/// Shared verdict computation over an immutable statement list. Called by
/// [`transform`] (the `DEBASHC_TRANSFORMS`-gated ast_to_ir hook) AND
/// re-run by the ESTree renderer under the compile lock (shir.rs
/// `shir_to_estree`) so the pointer keys the emission reads are the
/// authoritative ones for THIS compilation — the statics are
/// per-compilation global state, and parallel compilations (the
/// determinism unit tests) would otherwise tear them between the
/// ast_to_ir write and the emission read.
pub fn apply_to(stmts: &[IrStmt]) -> bool {
    let mut sync_ok = HashSet::new();
    let mut batch_ok = HashSet::new();
    for st in stmts.iter() {
        walk(st, &mut sync_ok, &mut batch_ok);
    }
    *SYNC_OK_LOOPS.lock().unwrap() = Some(sync_ok.clone());
    *BATCH_OK_LOOPS.lock().unwrap() = Some(batch_ok.clone());
    !sync_ok.is_empty() || !batch_ok.is_empty()
}

// ── The analysis ─────────────────────────────────────────────────────

fn walk(st: &IrStmt, sync_ok: &mut HashSet<usize>, batch_ok: &mut HashSet<usize>) {
    match st {
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            // (b) no-output AND provably finite → full sync, regardless of
            // trip count (nothing to interleave with).
            if body_output_free(st) && loop_finite(st) {
                sync_ok.insert(st as *const IrStmt as usize);
            }
            // (a) statically cheap (trip count × cost ≤ ~200ms) → full sync.
            else if cheap_total(st) {
                sync_ok.insert(st as *const IrStmt as usize);
            }
            // (c) complex: sync-executable body that would otherwise get
            // the per-iteration async forLoop → checkpointed form.
            else if batch_executable(st) {
                batch_ok.insert(st as *const IrStmt as usize);
            }
            for b in body {
                walk(b, sync_ok, batch_ok);
            }
            match st {
                IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => {
                    walk_expr(cond, sync_ok, batch_ok)
                }
                IrStmt::For { iter, .. } => walk_expr(iter, sync_ok, batch_ok),
                _ => {}
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
            ..
        } => {
            walk_expr(cond, sync_ok, batch_ok);
            for b in then.iter().chain(else_) {
                walk(b, sync_ok, batch_ok);
            }
            for (_, b) in elsifs {
                for s in b {
                    walk(s, sync_ok, batch_ok);
                }
            }
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => {
            for s in b {
                walk(s, sync_ok, batch_ok);
            }
        }
        IrStmt::Redirect {
            inner, redirects, ..
        } => {
            for s in inner {
                walk(s, sync_ok, batch_ok);
            }
            for r in redirects {
                walk_expr(&r.target, sync_ok, batch_ok);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for s in stage {
                    walk(s, sync_ok, batch_ok);
                }
            }
        }
        IrStmt::Expr(e) => walk_expr(e, sync_ok, batch_ok),
        // expression-bearing statements: the tree continues inside them
        // (a capture can wrap an arrow with the real loop — `x=$(for …)`)
        IrStmt::Assign { targets, expr, .. } => {
            walk_expr(expr, sync_ok, batch_ok);
            for t in targets {
                for i in &t.indices {
                    walk_expr(i, sync_ok, batch_ok);
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                walk_expr(i, sync_ok, batch_ok);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                walk_expr(e, sync_ok, batch_ok);
            }
        }
        IrStmt::Output { value, .. } => walk_expr(value, sync_ok, batch_ok),
        IrStmt::WriteFile { path, content, .. } => {
            walk_expr(path, sync_ok, batch_ok);
            walk_expr(content, sync_ok, batch_ok);
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => walk_expr(expr, sync_ok, batch_ok),
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            walk_expr(cmd, sync_ok, batch_ok);
            for a in args {
                walk_expr(a, sync_ok, batch_ok);
            }
            for r in redirects {
                walk_expr(r, sync_ok, batch_ok);
            }
            for (_, v) in env {
                walk_expr(v, sync_ok, batch_ok);
            }
        }
        IrStmt::Case {
            discriminant,
            clauses,
            ..
        } => {
            walk_expr(discriminant, sync_ok, batch_ok);
            for c in clauses {
                for s in &c.body {
                    walk(s, sync_ok, batch_ok);
                }
            }
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => walk_expr(e, sync_ok, batch_ok),
        IrStmt::SetChildError(e) => walk_expr(e, sync_ok, batch_ok),
        _ => {}
    }
}

/// Descend into expression trees: loops hide inside capture arrows,
/// brace calls, pipeline-arrow args, interpolations, ternary branches...
fn walk_expr(e: &IrExpr, sync_ok: &mut HashSet<usize>, batch_ok: &mut HashSet<usize>) {
    match e {
        IrExpr::Arrow(stmts) => {
            for s in stmts {
                walk(s, sync_ok, batch_ok);
            }
        }
        IrExpr::Call { args, .. } | IrExpr::MethodCall { args, .. } => {
            for a in args {
                walk_expr(a, sync_ok, batch_ok);
            }
        }
        IrExpr::Capture { expr, .. } => walk_expr(expr, sync_ok, batch_ok),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(inner) = p {
                    walk_expr(inner, sync_ok, batch_ok);
                }
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_expr(cond, sync_ok, batch_ok);
            walk_expr(then, sync_ok, batch_ok);
            walk_expr(else_, sync_ok, batch_ok);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_expr(lhs, sync_ok, batch_ok);
            walk_expr(rhs, sync_ok, batch_ok);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_expr(expr, sync_ok, batch_ok);
            walk_expr(default, sync_ok, batch_ok);
        }
        IrExpr::Array(items) => {
            for i in items {
                walk_expr(i, sync_ok, batch_ok);
            }
        }
        IrExpr::Object(pairs) => {
            for (_, v) in pairs {
                walk_expr(v, sync_ok, batch_ok);
            }
        }
        _ => {}
    }
}

/// (b) Does the loop body produce NO observable output? Disqualifiers:
/// any Output/WriteFile/Warn (fd writes), any exec of a non-no-op
/// command (it can write — conservative), Capture/Pipeline/Redirect/
/// Subshell/Background, blocking builtins (read/sleep/wait/eval),
/// function calls (unknown). Allowed: Assign/Declare/DeclareArray, pure
/// arith/exprs, no-op commands (`:` / `true` / `false`), nested If/Block/
/// loops (checked recursively).
fn body_output_free(st: &IrStmt) -> bool {
    match st {
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            body.iter().all(body_output_free)
        }
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            then.iter().chain(else_).all(body_output_free)
                && elsifs.iter().all(|(_, b)| b.iter().all(body_output_free))
        }
        IrStmt::Block(b) => b.iter().all(body_output_free),
        IrStmt::Assign { .. } | IrStmt::Declare { .. } | IrStmt::DeclareArray { .. } => true,
        IrStmt::Expr(e) => match exec_cmd_name(st) {
            Some(name) => matches!(name, ":" | "true" | "false"),
            None => expr_pure(e),
        },
        _ => false,
    }
}

/// Can the loop terminate provably fast enough / at all for a FULL sync
/// run? A For with a statically-known non-glob iterable is finite.
/// While/DoWhile: a cond that cannot be constant `true` (data-dependent →
/// not a trivially-infinite literal loop; the corpus has no infinite
/// loops, and bash spins on them too).
fn loop_finite(st: &IrStmt) -> bool {
    match st {
        IrStmt::For { iter, .. } => static_trip_count(iter).is_some(),
        IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => !const_true_like(cond),
        _ => false,
    }
}

/// The static trip count of a for-iterable. Recognized forms (from real
/// shIR):
/// - `Array([…])` of scalar, glob-free items (incl. a single brace-call
///   item — `{1..N}` lowers to `Call{func:"brace", args:[Str(""),
///   Json([[…{range:[start,end,…]}…]]), …]}` → count = end-start+1);
/// - `Range { start, end }` (inclusive).
/// `None` when not statically known (list vars, getVar arrays, seq/
/// capture iters...).
fn static_trip_count(iter: &IrExpr) -> Option<u64> {
    match iter {
        IrExpr::Array(items) => {
            let mut total = 0u64;
            for i in items {
                if let Some(n) = brace_count(i) {
                    total += n;
                } else if expr_glob_free(i) {
                    total += 1;
                } else {
                    return None;
                }
            }
            Some(total)
        }
        IrExpr::Range { start, end } => {
            if end >= start {
                Some((end - start + 1) as u64)
            } else {
                Some(0)
            }
        }
        _ => None,
    }
}

/// The trip count of a brace-expansion item: `Call{func:"brace", args}`
/// whose args[1] is the range `Json([[{range:[start,end,null,null]}]])`.
fn brace_count(e: &IrExpr) -> Option<u64> {
    match e {
        IrExpr::Call { func, args, .. } if func == "brace" && args.len() >= 2 => match &args[1] {
            IrExpr::Json(serde_json::Value::Array(groups)) => {
                let range = groups.first()?.as_array()?.first()?.get("range")?;
                let start: i64 = range.as_array()?.first()?.as_str()?.parse().ok()?;
                let end: i64 = range.as_array()?.get(1)?.as_str()?.parse().ok()?;
                if end >= start {
                    Some((end - start + 1) as u64)
                } else {
                    Some(0)
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// (a) Is the WHOLE loop confident to complete in ~200ms? Requires a
/// static trip count AND a cost-bounded body (no spawns, no blocking
/// I/O, no unknown-cost calls). The estimate is an UPPER bound: missing a
/// cheap loop keeps current behavior; over-marking is what the corpus
/// gate + bisect would catch.
fn cheap_total(st: &IrStmt) -> bool {
    let Some(n) = static_trip_count_of(st) else {
        return false;
    };
    let Some(cost) = loop_body_cost(st) else {
        return false;
    };
    n.saturating_mul(cost) <= BUDGET_US
}

/// The per-iteration cost UPPER bound of a loop = the sum of its BODY's
/// statement costs (the loop arms of [`iter_cost`] stay "nested loop →
/// unbounded" — a nested loop's own trip count is not the outer's).
fn loop_body_cost(st: &IrStmt) -> Option<u64> {
    match st {
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            let mut c = 0u64;
            for b in body {
                c = c.checked_add(iter_cost(b)?)?;
            }
            Some(c)
        }
        _ => None,
    }
}

fn static_trip_count_of(st: &IrStmt) -> Option<u64> {
    match st {
        IrStmt::For { iter, .. } => static_trip_count(iter),
        _ => None, // while/do-while: no static count → criterion (a) can't apply
    }
}

/// Per-iteration cost UPPER bound (µs): count cheap sync statements;
/// DISQUALIFY (a) on anything async/unknown-cost: Exec, Capture,
/// Pipeline, Redirect, Subshell, Background, Function defs, Calls
/// (unknown target), blocking builtins, Output whose value isn't pure.
fn iter_cost(st: &IrStmt) -> Option<u64> {
    match st {
        IrStmt::For { .. } | IrStmt::While { .. } | IrStmt::DoWhile { .. } => {
            // a nested loop's per-iteration cost is unbounded (its own
            // trip count is not the outer's) → not cheap by composition
            None
        }
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            let mut c = 0u64;
            for b in then.iter().chain(else_) {
                c += iter_cost(b)?;
            }
            for (_, b) in elsifs {
                for s in b {
                    c += iter_cost(s)?;
                }
            }
            Some(c)
        }
        IrStmt::Block(b) => {
            let mut c = 0u64;
            for s in b {
                c += iter_cost(s)?;
            }
            Some(c)
        }
        IrStmt::Assign { .. } | IrStmt::Declare { .. } | IrStmt::DeclareArray { .. } => {
            Some(STMT_COST_US)
        }
        IrStmt::Output { value, .. } => {
            if expr_pure(value) {
                Some(STMT_COST_US)
            } else {
                None
            }
        }
        IrStmt::Expr(e) => match exec_cmd_name(st) {
            Some(name) if sync_builtin(name) && !SIGNAL_OR_BLOCKING.contains(&name) => {
                Some(STMT_COST_US)
            }
            _ => {
                if expr_pure(e) {
                    Some(STMT_COST_US)
                } else {
                    None
                }
            }
        },
        _ => None, // Exec/Capture/Pipeline/Redirect/Subshell/Background/Function/Case/Die/Warn/...
    }
}

/// (c) Complex loop → checkpointed form: the body is SYNC-EXECUTABLE
/// (every op the runtime can run in-process without awaiting: sync
/// builtins except signals + blocking I/O, pure exprs), so the only
/// reason it currently gets the per-iteration async `forLoop` is the
/// async region / glob iterable / signals gates. Chunked sync + a
/// `setImmediate` yield per [`BATCH`] keeps bash's output order and
/// responsiveness at ~1/1024 the await overhead.
fn batch_executable(st: &IrStmt) -> bool {
    match st {
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            body.iter().all(batch_body_stmt)
        }
        _ => false,
    }
}

fn batch_body_stmt(st: &IrStmt) -> bool {
    match st {
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            body.iter().all(batch_body_stmt)
        }
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            then.iter().chain(else_).all(batch_body_stmt)
                && elsifs.iter().all(|(_, b)| b.iter().all(batch_body_stmt))
        }
        IrStmt::Block(b) => b.iter().all(batch_body_stmt),
        IrStmt::Assign { .. } | IrStmt::Declare { .. } | IrStmt::DeclareArray { .. } => true,
        IrStmt::Output { value, .. } => expr_pure(value),
        IrStmt::Expr(e) => match exec_cmd_name(st) {
            Some(name) => !SIGNAL_OR_BLOCKING.contains(&name) && sync_builtin(name),
            None => expr_pure(e),
        },
        // the neutral Exec form
        IrStmt::Exec {
            cmd: IrExpr::Str(name, _),
            ..
        } => !SIGNAL_OR_BLOCKING.contains(&name.as_str()) && sync_builtin(name),
        _ => false,
    }
}

/// The command name when a statement is an exec of a simple command —
/// BOTH the neutral `IrStmt::Exec { cmd }` form and the ESTree-path
/// `IrStmt::Expr(Call { func: "exec", args: [cmd, argarray] })` form
/// (what `ast_to_ir` actually emits for simple commands).
fn exec_cmd_name(st: &IrStmt) -> Option<&str> {
    match st {
        IrStmt::Exec {
            cmd: IrExpr::Str(name, _),
            ..
        } => Some(name),
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "exec" => match args.first() {
            Some(IrExpr::Str(name, _)) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

/// Builtins that must disqualify a batch body: signals (break/continue/
/// return need the async signal delivery), blocking I/O (read/wait/sleep
/// would freeze a whole chunk), and eval/exit (unknown/process-level).
const SIGNAL_OR_BLOCKING: &[&str] = &[
    "break", "continue", "return", "read", "wait", "sleep", "eval", "exit", "exec",
];

fn sync_builtin(name: &str) -> bool {
    // mirrors crate::shir::SYNC_BUILTINS (the runtime's in-process
    // builtins); local copy so this module stays self-contained.
    const SYNC: &[&str] = &[
        ".",
        ":",
        "basename",
        "cat",
        "cd",
        "cmp",
        "comm",
        "cut",
        "declare",
        "dirname",
        "echo",
        "export",
        "false",
        "grep",
        "head",
        "let",
        "local",
        "mapfile",
        "mktemp",
        "printf",
        "pwd",
        "readarray",
        "readonly",
        "seq",
        "sed",
        "set",
        "shift",
        "sort",
        "source",
        "stat",
        "tail",
        "test",
        "touch",
        "tr",
        "trap",
        "true",
        "type",
        "typeset",
        "uniq",
        "unset",
        "wc",
    ];
    SYNC.contains(&name)
}

/// Pure expression: no capture, no call (unknown), no pipeline machinery —
/// plain literals / vars / arithmetic / interpolation of pure parts.
fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_)
        | IrExpr::Str(_, _)
        | IrExpr::Var(_, _)
        | IrExpr::Bool(_)
        | IrExpr::Ident(_) => true,
        IrExpr::Range { .. } | IrExpr::Json(_) => true,
        IrExpr::Array(items) => items.iter().all(expr_pure),
        IrExpr::Arith(a) => arith_pure(a),
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs) && expr_pure(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_pure(cond) && expr_pure(then) && expr_pure(else_)
        }
        IrExpr::DefinedOr { expr, default } => expr_pure(expr) && expr_pure(default),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(_) => true,
            crate::ir::InterpPart::Expr(e) => expr_pure(e),
        }),
        IrExpr::Call { .. } | IrExpr::MethodCall { .. } | IrExpr::Capture { .. } => false,
        _ => false,
    }
}

fn arith_pure(a: &crate::ir::ArithAst) -> bool {
    match a {
        crate::ir::ArithAst::Num(_)
        | crate::ir::ArithAst::Var(..)
        | crate::ir::ArithAst::Index { .. } => true,
        crate::ir::ArithAst::Bin { lhs, rhs, .. } => arith_pure(lhs) && arith_pure(rhs),
        crate::ir::ArithAst::Un { arg, .. } => arith_pure(arg),
        _ => false,
    }
}

/// Constant-true-like cond (`while true` lowers to `exec true` — the
/// `Expr(Call{func:"exec"})` form; `while 1` → Int). Such loops may never
/// terminate; never mark them full-sync (a sync infinite loop freezes
/// the harness).
fn const_true_like(e: &IrExpr) -> bool {
    match e {
        IrExpr::Bool(b) => *b,
        IrExpr::Int(i) => *i != 0,
        IrExpr::Str(s, _) => matches!(s.as_str(), "true" | "1" | "yes"),
        IrExpr::Call { func, args, .. } if func == "exec" => match args.first() {
            Some(IrExpr::Str(s, _)) => matches!(s.as_str(), "true" | "1" | ":"),
            _ => false,
        },
        _ => false,
    }
}

/// Glob-free expression (a for-item that expands to exactly itself).
fn expr_glob_free(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(s, _) => !s.contains(['*', '?', '[']),
        IrExpr::Int(_) => true,
        _ => false,
    }
}
