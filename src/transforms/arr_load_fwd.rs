//! arr-load-fwd — **array-element-load copy propagation**.
//!
//! A scalar that holds a COPY of one array element —
//!
//! ```sh
//! rbm_bh=${RBHEX[$rbm_b]}      # v = arr[key]   (a full value copy)
//! …
//! rb_pxhex="${rbm_bh}…"         # the only read of v
//! ```
//!
//! — folds back to the element load itself: the read becomes
//! `RBHEX[$rbm_b]` again, and the scalar store is left with no reader, so
//! it is removed and no backend ever materialises the copy. Removal is
//! normally `dead-store-elim`'s (it runs directly after this pass); for
//! a store DSE's `Const` verdict can never accept — every site inside a
//! function body is multi-run — this pass drops its own orphan instead
//! (see `drop_orphans`), proven with DSE's census and purity rules so
//! the two cannot disagree.
//!
//! This is the shIR→shIR form of the rewrite, not a C trick: every
//! backend that runs it emits one array read instead of a store plus a
//! load. On a backend whose scalar copy is already cheap (Rust `String`,
//! Perl SV) it is neutral; on C it is the whole win — MIMEcroft's
//! `rb_mkhex` made three 212 k-call stores out of a 256-entry constant
//! table per frame that only ever fed one interpolation.
//!
//! The sibling `copy-propagation` folds a single-def LITERAL into its
//! reads with the same dominance argument; a single-def element load is
//! that argument with an array-read payload instead of a literal, so it
//! is registered right beside it.
//!
//! ## Soundness — refuse over guess
//! The def must be
//!   - a single-target `Assign` of a bare element load — `Index { var,
//!     key }` OR its `Call { "arrayIndex", [Str(arr), key] }` spelling,
//!     which is what `${arr[$k]}` lowers to (`Word::MapAccess`), so a
//!     program that spells its reads that way reaches this pass at all —
//!     with `key` pure and
//!     idempotent (it is re-evaluated at every forwarded read: no call,
//!     no capture/subshell, no `++`/`--`/`=` arithmetic), and `key` must
//!     not mention the scalar itself;
//!   - `v`'s ONLY write in the whole program (counting base-of-array
//!     writes and nested bodies) — a second write makes the copied value
//!     ambiguous, and an `eval` / `source` anywhere disqualifies
//!     everything (copy-propagation's precedent);
//!   - and every read we forward must sit in the SAME statement list, in
//!     a statement strictly AFTER the def (a read before the def observes
//!     the shell's empty-string default, never the element).
//!
//! Between the def and each forwarded read, every intervening statement
//! must be inert with respect to the array. A **read** of the array is
//! inert — and so is a call to a known read-only helper (`arrayIndex`,
//! `getVar`, `param`, …), which is the same read in call form; without
//! that one recognition the pass cannot cross the sibling statements of
//! the very idiom above (`gh=${RBHEX[$g]}` sits between `bh`'s def and
//! `bh`'s read, and it is a call). A write to the array (own name or an
//! element), any other call (a callee may write the array, and modelling
//! callee effects is refuse-worthy, not worth guessing), a subshell /
//! background job / any other command (`Exec`, possibly `eval`) and any
//! node we do not know all refuse, and forwarding stops at the first such
//! statement.
//!
//! The key's own read set gets the same treatment: it is re-evaluated at
//! the read site, so an intervening write to any variable it mentions
//! stops the forward too (`key_vars` — an unbounded key, a `$(…)` inside
//! a `Str`, refuses the candidate outright).
//!
//! Only a statement's OWN expressions are folded — never its bodies. A
//! loop/if body, a function body, or a closure (`Arrow`/`Lambda`) can run
//! its reads in an order the statement position does not describe, and a
//! function body can run before the def entirely.
//!
//! Partial forwarding is safe: any read that is not forwarded keeps its
//! scalar, which keeps the def alive, so the value it observes is
//! unchanged.

use std::collections::{HashMap, HashSet};

use super::READ_ONLY_CALLS;
use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};
use crate::transforms::string_accumulator::{callee_write_sets, Cw};

/// Value constructors that write NO name-based state (the PureCpu text
/// ops): they compute from their arguments and nothing else, so they are
/// inert in a span exactly as `READ_ONLY_CALLS` reads are. Kept separate
/// from the shared list because that one is also DSE's *drop-the-store*
/// verdict ("skipping this RHS is unobservable"), which needs a
/// different — stronger — guarantee than "cannot write the array".
const SPAN_PURE_CALLS: &[&str] = &["split", "join"];

/// Can evaluating this raw test text WRITE a shell name? A test itself
/// writes nothing (it only sets status), but its text is expanded first:
/// `${name=…}` (incl. the `${!name=…}` indirection form) assigns,
/// `$(( … ))` arithmetic carries assignments, and a command substitution
/// / backtick runs an arbitrary command list. Bare `$name` reads pass.
fn text_can_write(s: &str) -> bool {
    s.contains("${") || s.contains("$(") || s.contains('`')
}

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // A/B seam, same spelling as the other gates (SH2_NO_STR_CHOP_CURSOR,
    // SH2_NO_NATIVE_LIFT): set to `1` to render the program exactly as
    // before this pass existed, for a perf/corpus comparison.
    if std::env::var("SH2_NO_ARR_LOAD_FWD").as_deref() == Ok("1") {
        return false;
    }
    if has_dynamic(stmts) {
        return false;
    }
    let mut writes: HashMap<String, usize> = HashMap::new();
    count_writes(stmts, &mut writes);
    if writes.is_empty() {
        return false;
    }
    // The forward only MOVES the read; the store it can leave behind is
    // normally `dead-store-elim`'s to remove (it runs directly after
    // this pass in the registry). But DSE only drops `Const` sites, and
    // `analyze_var_const` marks every site inside a function body
    // multi-run ("a function may run 0..N times"), so a function-local
    // copy — the exact shape this pass exists to kill (MIMEcroft's
    // `rb_mkhex` holds three of them per call, 212 k calls per frame) —
    // would survive forever with no reader at all. So after the fold,
    // any scalar it orphaned completely is removed HERE, under DSE's own
    // standards: DSE's `census` (the read/escape rules, so both passes
    // agree by construction), DSE's `expr_pure` (skipping the store's
    // RHS must be unobservable) and the `export` list (children read
    // env-visible names). Refuse > guess on every axis.
    // Whole-program callee write sets: a span statement that CALLS a
    // program function is inert iff that function (transitively) does
    // not write the array — the exact proof `string_accumulator` needs
    // for its own append verdicts, computed the same way here so the two
    // can never disagree. An unmodelled callee keeps the old refusal.
    let cw = callee_write_sets(stmts);
    let mut fold_vars: HashSet<String> = HashSet::new();
    let changed = fwd_list(stmts, &writes, &mut fold_vars, &cw);
    let pruned = drop_orphans(stmts, &fold_vars);
    changed || pruned
}

/// Remove the def of every scalar the forward orphaned *completely*:
/// DSE's census says nothing reads or escapes it any more, it is not
/// exported (env-visible = read by children), it is not a shell-implicit
/// name, and its RHS passes DSE's own purity test — an element load with
/// a `pure_key`/`key_vars`-vetted key (the fold only ran under those
/// gates), so deleting the store cannot skip, move or observe anything.
fn drop_orphans(stmts: &mut Vec<IrStmt>, fold_vars: &HashSet<String>) -> bool {
    if fold_vars.is_empty() {
        return false;
    }
    let mut reads = HashSet::new();
    let mut writes = HashSet::new();
    let mut escapes = HashSet::new();
    crate::transforms::dead_store_elim::census(stmts, &mut reads, &mut writes, &mut escapes);
    let exported = crate::shir_passes::split_inplace::exported_names(stmts);
    let orphans: HashSet<String> = fold_vars
        .iter()
        .filter(|v| {
            !reads.contains(*v)
                && !escapes.contains(*v)
                && !exported.contains(*v)
                && !crate::transforms::dead_store_elim::is_shell_implicit(v)
        })
        .cloned()
        .collect();
    if orphans.is_empty() {
        return false;
    }
    prune_list(stmts, &orphans)
}

fn prune_list(list: &mut Vec<IrStmt>, orphans: &HashSet<String>) -> bool {
    let before = list.len();
    list.retain(|st| {
        // `candidate` pins the shape to a bare single-target element
        // load (the only def this pass ever folds from); `expr_pure` is
        // the same standard DSE's purge applies before dropping a store.
        let Some((v, _, _)) = candidate(st) else {
            return true;
        };
        if !orphans.contains(&v) {
            return true;
        }
        match st {
            IrStmt::Assign { expr, .. } => {
                !crate::transforms::dead_store_elim::expr_pure(expr)
            }
            _ => true,
        }
    });
    let mut changed = list.len() != before;
    for st in list.iter_mut() {
        for l in child_lists_mut(st) {
            changed |= prune_list(l, orphans);
        }
    }
    changed
}

// ── disqualifiers ────────────────────────────────────────────────────

/// `eval` / `source` / `.` can reach any assignment the static census
/// never saw — one anywhere disqualifies the whole pass (and
/// `dead-store-elim` has the same rule, so the orphaned stores would
/// not be removable there either).
///
/// Closures are NOT a program-wide disqualifier: a `Capture{Arrow}` /
/// `Arrow` / `Lambda` can only *run* through the statement that holds
/// it, and `stmt_refuses` refuses every statement form that carries
/// one (see `expr_refuses`'s opaque arm), so a closure between the def
/// and a read already stops that forward at the span level — without
/// one capture in the middle of a 7 000-line program disabling the
/// pass everywhere (MIMEcroft's `sl_x=$(exec cat …)` did exactly that).
fn has_dynamic(stmts: &[IrStmt]) -> bool {
    for st in stmts {
        if let IrStmt::Exec { cmd, .. } = st {
            if matches!(cmd, IrExpr::Str(s, _) if s == "eval" || s == "source" || s == ".") {
                return true;
            }
        }
        for l in st_child_lists(st) {
            if has_dynamic(l) {
                return true;
            }
        }
    }
    false
}

// ── write census ─────────────────────────────────────────────────────

fn bump(writes: &mut HashMap<String, usize>, name: &str) {
    *writes.entry(name.to_string()).or_insert(0) += 1;
}

fn count_writes(stmts: &[IrStmt], writes: &mut HashMap<String, usize>) {
    for st in stmts {
        count_stmt_writes(st, writes);
    }
}

fn count_stmt_writes(st: &IrStmt, writes: &mut HashMap<String, usize>) {
    match st {
        IrStmt::Assign { targets, .. } => {
            for t in targets {
                if t.indices.is_empty() && !t.var.contains('[') {
                    bump(writes, &t.var);
                } else {
                    // an element write writes the BASE too
                    let base = t.var.split('[').next().unwrap_or(&t.var).to_string();
                    bump(writes, &base);
                }
            }
        }
        IrStmt::Declare { vars, .. } => {
            for v in vars {
                bump(writes, &v.name);
            }
        }
        IrStmt::DeclareArray { var, .. } => bump(writes, var),
        IrStmt::For { var, .. } => bump(writes, var),
        _ => {}
    }
    // an arithmetic assignment/inc-dec in an expression writes too
    for e in stmt_exprs(st) {
        expr_arith_writes(e, writes);
    }
    for l in st_child_lists(st) {
        count_writes(l, writes);
    }
}

fn expr_arith_writes(e: &IrExpr, writes: &mut HashMap<String, usize>) {
    match e {
        IrExpr::Arith(a) => arith_writes(a, writes),
        IrExpr::Call { args, .. } | IrExpr::MethodCall { args, .. } => {
            for a in args {
                expr_arith_writes(a, writes);
            }
        }
        IrExpr::MethodCall { obj, .. } => expr_arith_writes(obj, writes),
        IrExpr::Capture { expr, .. } => expr_arith_writes(expr, writes),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_arith_writes(lhs, writes);
            expr_arith_writes(rhs, writes);
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_arith_writes(cond, writes);
            expr_arith_writes(then, writes);
            expr_arith_writes(else_, writes);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_arith_writes(expr, writes);
            expr_arith_writes(default, writes);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    expr_arith_writes(x, writes);
                }
            }
        }
        IrExpr::Array(es) => {
            for x in es {
                expr_arith_writes(x, writes);
            }
        }
        IrExpr::Index { key, .. } => expr_arith_writes(key, writes),
        // a transform node's operands are ordinary expressions: an
        // `i++` inside a splice's offsets is a write like any other
        // (missing it would let the span cross an unmodelled write).
        IrExpr::Ext(n) => {
            for c in n.children() {
                expr_arith_writes(c, writes);
            }
        }
        _ => {}
    }
}

fn arith_writes(a: &ArithAst, writes: &mut HashMap<String, usize>) {
    match a {
        ArithAst::Assign { var, rhs, .. } => {
            bump(writes, var);
            arith_writes(rhs, writes);
        }
        ArithAst::IncDec { var, .. } => bump(writes, var),
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_writes(lhs, writes);
            arith_writes(rhs, writes);
        }
        ArithAst::Un { arg, .. } => arith_writes(arg, writes),
        ArithAst::Cond { test, then, else_ } => {
            arith_writes(test, writes);
            arith_writes(then, writes);
            arith_writes(else_, writes);
        }
        ArithAst::Index { key, .. } => arith_writes(key, writes),
        ArithAst::Cast { arg, .. } => arith_writes(arg, writes),
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => {}
    }
}

/// Every nested statement list a statement owns.
fn st_child_lists(st: &IrStmt) -> Vec<&Vec<IrStmt>> {
    let mut out: Vec<&Vec<IrStmt>> = Vec::new();
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            out.push(then);
            for (_, b) in elsifs {
                out.push(b);
            }
            out.push(else_);
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::Function { body, .. } => out.push(body),
        IrStmt::ForInit { init, step, body, .. } => {
            out.push(init);
            out.push(step);
            out.push(body);
        }
        IrStmt::Block(b) => out.push(b),
        IrStmt::Subshell(b) | IrStmt::Background(b) => out.push(b),
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            out.push(body);
            out.push(else_body);
            out.push(finally_body);
            for x in excepts {
                out.push(&x.body);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                out.push(&c.body);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                out.push(s);
            }
        }
        IrStmt::Redirect { inner, .. } => out.push(inner),
        _ => {}
    }
    // `Function` owns both its body and its named blocks (the single arm
    // above pushed the body; the named blocks are the same `Function`).
    if let IrStmt::Function { named_blocks, .. } = st {
        for (_, b) in named_blocks {
            out.push(b);
        }
    }
    out
}

// ── the fold ─────────────────────────────────────────────────────────

fn fwd_list(
    list: &mut Vec<IrStmt>,
    writes: &HashMap<String, usize>,
    fold_vars: &mut HashSet<String>,
    cw: &Cw,
) -> bool {
    let mut changed = false;
    for d in 0..list.len() {
        let Some((v, arr, key)) = candidate(&list[d]) else {
            continue;
        };
        if writes.get(&v).copied().unwrap_or(0) != 1 {
            continue;
        }
        if !pure_key(&key) || expr_mentions(&key, &v) {
            continue;
        }
        // The key is re-evaluated at every read we move it to, so its
        // own read set has to survive the statements in between: a write
        // to one of those variables would make the moved read observe a
        // different element than the store did.
        let Some(kv) = key_vars(&key) else {
            continue;
        };
        // Forward the reads of `v` in every statement AFTER the def,
        // stopping at the first statement that is not provably inert
        // with respect to `arr` (or with respect to the key).
        let dbg = std::env::var("SH2_ARR_LOAD_FWD_DEBUG").as_deref() == Ok("1");
        if dbg {
            eprintln!("[alf] candidate v={} arr={} key={:?} d={} kv={:?} writes={:?}",
                v, arr, key, d, kv, writes.get(&v));
        }
        let mut folded = false;
        for i in (d + 1)..list.len() {
            let w = stmt_writes_any(&list[i], &kv);
            let refuses = stmt_refuses(&list[i], &arr, cw);
            if dbg {
                let s: String = format!("{:?}", list[i]).chars().take(400).collect();
                eprintln!("[alf]   i={} writes_key={} refuses={} stores={} :: {}",
                    i, w, refuses, stmt_stores(&list[i], &arr), s);
            }
            if w || refuses {
                break;
            }
            if fold_top(&mut list[i], &v, &arr, &key) {
                folded = true;
                if dbg {
                    eprintln!("[alf]   FOLD v={} into i={}", v, i);
                }
            }
            // Folding INTO a statement that stores `arr` is sound: its
            // RHS — and with it the moved read — is fully evaluated
            // before the store writes the element (shell semantics and
            // every renderer's own emit order agree on that). Crossing
            // it is not: after it, `arr` may hold a different element,
            // so a later read would no longer observe the def-time
            // value. Stop here; partial forwarding stays safe because
            // any unfolded read keeps the def alive.
            if stmt_stores(&list[i], &arr) {
                break;
            }
        }
        if folded {
            fold_vars.insert(v.clone());
        }
        if dbg {
            eprintln!("[alf] result v={} folded={}", v, folded);
        }
        changed |= folded;
    }

    // recurse: candidates also live in function/if/loop bodies
    for st in list.iter_mut() {
        for l in child_lists_mut(st) {
            changed |= fwd_list(l, writes, fold_vars, cw);
        }
    }
    changed
}

#[cfg_attr(not(test), allow(dead_code))]
fn child_lists_read(st: &IrStmt, v: &str) -> bool {
    st_child_lists(st).iter().any(|l| any_read(l, v))
}
/// The statement is a bare `v = arr[key]` single-target element load.
fn candidate(st: &IrStmt) -> Option<(String, String, IrExpr)> {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            if targets.len() != 1 {
                return None;
            }
            let t = &targets[0];
            if !t.indices.is_empty() || t.var.contains('[') {
                return None;
            }
            match expr {
                IrExpr::Index { var, key } if !var.is_empty() => {
                    Some((t.var.clone(), var.clone(), (**key).clone()))
                }
                // `${arr[$k]}`'s lowering: `Word::MapAccess` builds the
                // call node directly, and the A1 export spells `Index`
                // the same way — a renderer round-trip and a fresh parse
                // of one source differ only in this node. Same payload,
                // two spellings; refuse a shape we do not map 1:1.
                IrExpr::Call { func, args, .. }
                    if func == "arrayIndex" && args.len() == 2 =>
                {
                    match (&args[0], &args[1]) {
                        (IrExpr::Str(a, _), k) if !a.is_empty() => {
                            Some((t.var.clone(), a.clone(), k.clone()))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The forwarded payload must be re-evaluable at every read: no call,
/// no capture/subshell, no `++`/`--`/`=` arithmetic (those are writes,
/// and re-running them per read is a different program).
fn pure_key(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Ident(_) => true,
        IrExpr::Index { key, .. } => pure_key(key),
        IrExpr::BinOp { lhs, rhs, .. } => pure_key(lhs) && pure_key(rhs),
        IrExpr::Ternary { cond, then, else_, .. } => {
            pure_key(cond) && pure_key(then) && pure_key(else_)
        }
        IrExpr::DefinedOr { expr, default } => pure_key(expr) && pure_key(default),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(x) => pure_key(x),
        }),
        IrExpr::Arith(a) => pure_arith(a),
        _ => false,
    }
}

fn pure_arith(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => true,
        ArithAst::Index { key, .. } => pure_arith(key),
        ArithAst::Bin { lhs, rhs, .. } => pure_arith(lhs) && pure_arith(rhs),
        ArithAst::Un { arg, .. } => pure_arith(arg),
        ArithAst::Cond { test, then, else_ } => {
            pure_arith(test) && pure_arith(then) && pure_arith(else_)
        }
        ArithAst::Cast { arg, .. } => pure_arith(arg),
        ArithAst::Assign { .. } | ArithAst::IncDec { .. } => false,
    }
}

// ── guards ───────────────────────────────────────────────────────────

/// Does this expression mention `name` at all? (used for the key-purity
/// check, where a read counts). Unknown nodes say YES (refuse).
fn expr_mentions(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) => false,
        IrExpr::Var(n, _) | IrExpr::Ident(n) => n == name,
        IrExpr::Index { var, key } => var == name || expr_mentions(key, name),
        IrExpr::BinOp { lhs, rhs, .. } => expr_mentions(lhs, name) || expr_mentions(rhs, name),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_mentions(cond, name) || expr_mentions(then, name) || expr_mentions(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_mentions(expr, name) || expr_mentions(default, name)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(x) => expr_mentions(x, name),
        }),
        IrExpr::Array(es) => es.iter().any(|x| expr_mentions(x, name)),
        IrExpr::Ext(n) => n.children().iter().any(|c| expr_mentions(c, name)),
        IrExpr::Arith(a) => arith_mentions(a, name),
        _ => true,
    }
}

fn arith_mentions(a: &ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Num(_) | ArithAst::Sizeof(_) => false,
        ArithAst::Var(n) | ArithAst::Ident(n) => n == name,
        ArithAst::Index { var, key } => var == name || arith_mentions(key, name),
        ArithAst::Bin { lhs, rhs, .. } => arith_mentions(lhs, name) || arith_mentions(rhs, name),
        ArithAst::Un { arg, .. } => arith_mentions(arg, name),
        ArithAst::Cond { test, then, else_ } => {
            arith_mentions(test, name) || arith_mentions(then, name) || arith_mentions(else_, name)
        }
        ArithAst::Assign { var, rhs, .. } => var == name || arith_mentions(rhs, name),
        ArithAst::IncDec { var, .. } => var == name,
        ArithAst::Cast { arg, .. } => arith_mentions(arg, name),
    }
}

/// Can this expression make `arr` differ from what it was at the def, or
/// run something we do not model? A plain READ of `arr` is inert (that is
/// exactly what the forwarded read does); a call, a capture, a closure,
/// an arithmetic assignment/inc-dec to `arr`, or an unknown node refuses.
fn expr_refuses(e: &IrExpr, arr: &str, cw: &Cw) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Ident(_) => false,
        // A transform node (StrSplice, FieldGrab/Strip, CutAt, …) is the
        // composition its operands already state: inert exactly when its
        // children are. Refusing here would make a folded node
        // un-crossable — one splice store would void every forward that
        // reaches it.
        IrExpr::Ext(n) => n.children().iter().any(|c| expr_refuses(c, arr, cw)),
        IrExpr::Index { key, .. } => expr_refuses(key, arr, cw),
        IrExpr::BinOp { lhs, rhs, .. } => expr_refuses(lhs, arr, cw) || expr_refuses(rhs, arr, cw),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_refuses(cond, arr, cw) || expr_refuses(then, arr, cw) || expr_refuses(else_, arr, cw)
        }
        IrExpr::DefinedOr { expr, default } => expr_refuses(expr, arr, cw) || expr_refuses(default, arr, cw),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(x) => expr_refuses(x, arr, cw),
        }),
        IrExpr::Array(es) => es.iter().any(|x| expr_refuses(x, arr, cw)),
        IrExpr::Arith(a) => arith_refuses(a, arr),
        IrExpr::Range { .. } | IrExpr::Bool(_) | IrExpr::Json(_) => false,
        // A known read-only helper is the same inert read in call form:
        // `arrayIndex` is what the def itself is made of, and
        // `getVar`/`param` are plain scalar reads (module doc: without
        // this one recognition the pass cannot cross the sibling
        // statements of the very idiom it exists for — `gh=${RBHEX[$g]}`
        // sits between `bh`'s def and `bh`'s read, and it is a call).
        // A program definition of the same name shadows the builtin, so
        // a `cw` entry for the name wins the verdict instead.
        IrExpr::Call { func, args, .. }
            if READ_ONLY_CALLS.contains(&func.as_str()) && !cw.contains_key(func.as_str()) =>
        {
            args.iter().any(|a| expr_refuses(a, arr, cw))
        }
        // Everything else that calls — a builtin, an exec, a script
        // function — may write `arr`, and modelling callee effects is
        // refuse-worthy… except that `string_accumulator::callee_write_sets`
        // DOES model the program half exactly: a callee with a known write
        // set that misses `arr` is inert (MIMEcroft's `if n<=1; then … else
        // rb_mkrun fi` sits between `rbp_row=${RFB_ROWS[$y]}` and its only
        // read). `exec(NAME, …)` runs program fn NAME; a bare `Call{F}`
        // is the direct script-call spelling of the same thing.
        IrExpr::Call { func, args, .. } if func == "exec" || cw.contains_key(func.as_str()) => {
            let (name, rest): (&str, &[IrExpr]) = if func == "exec" {
                match args.split_first() {
                    Some((IrExpr::Str(n, _), r)) => (n.as_str(), r),
                    Some((IrExpr::Ident(n), r)) => (n.as_str(), r),
                    _ => return true, // opaque command — refuse
                }
            } else {
                (func.as_str(), args.as_slice())
            };
            match callee_writes(name, cw, arr) {
                Some(true) => true,  // the callee writes `arr`
                Some(false) => rest.iter().any(|a| expr_refuses(a, arr, cw)),
                None => true,        // builtin/unknown — refuse > guess
            }
        }
        // Pure value constructors: they compute from their arguments and
        // write no name-based state (module doc: SPAN_PURE_CALLS).
        IrExpr::Call { func, args, .. } if SPAN_PURE_CALLS.contains(&func.as_str()) => {
            args.iter().any(|a| expr_refuses(a, arr, cw))
        }
        // A shell test writes no variable — its TEXT is expanded first,
        // and `text_can_write` refuses the expansion forms that could
        // (a default assignment, arithmetic, a substitution).
        IrExpr::Call { func, args, .. } if func == "test" => args.iter().any(|a| match a {
            IrExpr::Str(s, _) => text_can_write(s),
            other => expr_refuses(other, arr, cw),
        }),
        IrExpr::Call { .. } => true,
        // a capture/regex/raw/closure/opaque node: refuse.
        _ => true,
    }
}

fn arith_refuses(a: &ArithAst, arr: &str) -> bool {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => false,
        ArithAst::Index { var, key } => var == arr || arith_refuses(key, arr),
        ArithAst::Assign { var, rhs, .. } => var == arr || arith_refuses(rhs, arr),
        ArithAst::IncDec { var, .. } => var == arr,
        ArithAst::Bin { lhs, rhs, .. } => arith_refuses(lhs, arr) || arith_refuses(rhs, arr),
        ArithAst::Un { arg, .. } => arith_refuses(arg, arr),
        ArithAst::Cond { test, then, else_ } => {
            arith_refuses(test, arr) || arith_refuses(then, arr) || arith_refuses(else_, arr)
        }
        ArithAst::Cast { arg, .. } => arith_refuses(arg, arr),
    }
}

/// Does this statement REFUSE to be crossed? TRUE when its own
/// expressions (or its bodies) could write `arr` or run something we do
/// not model — a read folded into one of them would no longer observe
/// the def-time element. FALSE = provably inert. Unknown statement
/// forms refuse.
///
/// NOTE the deliberate absence of a *target* check: `arr[k] = …`'s own
/// RHS (and target indices) evaluate BEFORE the store writes, so folding
/// a read into them is sound even though the statement stores `arr` —
/// crossing it is what must stop, and that verdict lives in
/// [`stmt_stores`] (the fwd loop consults both).
fn stmt_refuses(st: &IrStmt, arr: &str, cw: &Cw) -> bool {
    match st {
        // a function DEFINITION does not execute here; a call to it is an
        // `Exec`/`Call` and refuses on its own.
        IrStmt::Function { .. } => false,
        IrStmt::Assign { targets, expr, .. } => {
            if expr_refuses(expr, arr, cw) {
                return true;
            }
            for t in targets {
                if t.indices.iter().any(|i| expr_refuses(i, arr, cw)) {
                    return true;
                }
            }
            false
        }
        IrStmt::Declare { init, .. } => init.as_ref().is_some_and(|e| expr_refuses(e, arr, cw)),
        IrStmt::DeclareArray { var, elements, .. } => {
            var == arr || elements.iter().any(|e| expr_refuses(e, arr, cw))
        }
        IrStmt::Output { value, .. } => expr_refuses(value, arr, cw),
        IrStmt::WriteFile { path, content, .. } => {
            expr_refuses(path, arr, cw) || expr_refuses(content, arr, cw)
        }
        IrStmt::Expr(e) => expr_refuses(e, arr, cw),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
            expr_refuses(e, arr, cw)
        }
        IrStmt::Return(None) | IrStmt::Exit(None) => false,
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_refuses(cond, arr, cw)
                || then.iter().any(|s| stmt_refuses(s, arr, cw))
                || elsifs.iter().any(|(c, b)| {
                    expr_refuses(c, arr, cw) || b.iter().any(|s| stmt_refuses(s, arr, cw))
                })
                || else_.iter().any(|s| stmt_refuses(s, arr, cw))
        }
        IrStmt::While { cond, body } => {
            expr_refuses(cond, arr, cw) || body.iter().any(|s| stmt_refuses(s, arr, cw))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            expr_refuses(cond, arr, cw) || body.iter().any(|s| stmt_refuses(s, arr, cw))
        }
        IrStmt::For { iter, body, .. } => {
            expr_refuses(iter, arr, cw) || body.iter().any(|s| stmt_refuses(s, arr, cw))
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            expr_refuses(cond, arr, cw)
                || init.iter().any(|s| stmt_refuses(s, arr, cw))
                || step.iter().any(|s| stmt_refuses(s, arr, cw))
                || body.iter().any(|s| stmt_refuses(s, arr, cw))
        }
        IrStmt::Block(b) => b.iter().any(|s| stmt_refuses(s, arr, cw)),
        IrStmt::Case { discriminant, clauses, .. } => {
            expr_refuses(discriminant, arr, cw)
                || clauses.iter().any(|c| c.body.iter().any(|s| stmt_refuses(s, arr, cw)))
        }
        // a command (`Exec`) writes through its CALLEE: a program
        // function whose known write set misses `arr` is exactly as
        // inert as a read-only builtin (the expression arm's own proof
        // — same `cw`); an eval/source, a builtin, an unknown callee, a
        // subshell/background job, a pipeline/redirect, or a node we did
        // not model all still refuse.
        IrStmt::Exec {
            cmd,
            args,
            env,
            redirects,
            ..
        } => {
            let callee = match cmd {
                IrExpr::Str(f, _) | IrExpr::Ident(f) => callee_writes(f.as_str(), cw, arr),
                _ => Some(true), // opaque cmd → refuse
            };
            callee.unwrap_or(true)
                || args.iter().any(|a| expr_refuses(a, arr, cw))
                || env.iter().any(|(_, e)| expr_refuses(e, arr, cw))
                || redirects.iter().any(|r| expr_refuses(r, arr, cw))
        }
        IrStmt::Subshell(_)
        | IrStmt::Background(_)
        | IrStmt::Pipeline { .. }
        | IrStmt::Redirect { .. }
        | IrStmt::Try { .. }
        | IrStmt::Ext(_)
        | IrStmt::Asm { .. }
        | IrStmt::Select { .. }
        | IrStmt::Die { .. }
        | IrStmt::Warn { .. }
        | IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Label(_)
        | IrStmt::Goto(_)
        | IrStmt::Shaderize { .. }
        | IrStmt::Require(_)
        | IrStmt::RawText(_) => true,
        _ => true,
    }
}

/// The statement STORES `arr` through its own target — the verdict that
/// stops the forward from crossing it (its expressions may still be
/// folded; see the fwd loop's note).
fn stmt_stores(st: &IrStmt, arr: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => targets
            .iter()
            .any(|t| t.var.split('[').next().unwrap_or(&t.var) == arr),
        _ => false,
    }
}

/// Does the program function `f` write `arr`? `None` when the verdict
/// cannot be pinned down (no write set for `f` — a builtin or helper,
/// or an opaque callee whose body we could not model), in which case the
/// caller must refuse rather than trust a partial set.
fn callee_writes(f: &str, cw: &Cw, arr: &str) -> Option<bool> {
    match cw.get(f) {
        Some(Some(w)) => Some(w.contains(arr)),
        _ => None,
    }
}

/// The variables the forwarded key READS — re-evaluated at every read the
/// forward moves it to, so an intervening write to any of them must stop
/// the forward. `None` = the read set cannot be bounded (a `$(…)`/backtick
/// inside a `Str`, or a node `pure_key` would have refused anyway) and the
/// candidate is dropped: refuse > guess.
fn key_vars(e: &IrExpr) -> Option<std::collections::HashSet<String>> {
    fn union(a: Option<Set>, b: Option<Set>) -> Option<Set> {
        let mut a = a?;
        a.extend(b?);
        Some(a)
    }
    type Set = std::collections::HashSet<String>;
    fn str_deps(s: &str) -> Option<Set> {
        if s.contains("$(") || s.contains('`') {
            return None; // a command substitution hiding in the text
        }
        let mut out = Set::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'$' {
                let mut j = i + 1;
                if j < b.len() && b[j] == b'{' {
                    j += 1;
                }
                let start = j;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j > start {
                    out.insert(s[start..j].to_string());
                }
                i = j;
            } else {
                i += 1;
            }
        }
        Some(out)
    }
    fn arith(a: &ArithAst) -> Option<Set> {
        match a {
            ArithAst::Num(_) | ArithAst::Sizeof(_) => Some(Set::new()),
            ArithAst::Var(n) | ArithAst::Ident(n) => Some([n.clone()].into_iter().collect()),
            ArithAst::Index { var, key } => union(Some([var.clone()].into_iter().collect()), arith(key)),
            ArithAst::Bin { lhs, rhs, .. } => union(arith(lhs), arith(rhs)),
            ArithAst::Cast { arg, .. } => arith(arg),
            ArithAst::Un { arg, .. } => arith(arg),
            ArithAst::Cond { test, then, else_ } => {
                union(union(arith(test), arith(then)), arith(else_))
            }
            // `pure_key` refuses these; a key that reached us with one is
            // a gate we do not understand — drop the candidate.
            ArithAst::Assign { .. } | ArithAst::IncDec { .. } => None,
        }
    }
    match e {
        IrExpr::Int(_) | IrExpr::Bool(_) => Some(std::collections::HashSet::new()),
        IrExpr::Str(s, _) => str_deps(s),
        IrExpr::Var(n, _) | IrExpr::Ident(n) => {
            Some([n.clone()].into_iter().collect())
        }
        IrExpr::Index { var, key } => union(
            Some([var.clone()].into_iter().collect()),
            key_vars(key),
        ),
        IrExpr::BinOp { lhs, rhs, .. } => union(key_vars(lhs), key_vars(rhs)),
        IrExpr::Ternary { cond, then, else_ } => {
            union(union(key_vars(cond), key_vars(then)), key_vars(else_))
        }
        IrExpr::DefinedOr { expr, default } => union(key_vars(expr), key_vars(default)),
        IrExpr::Interpolate(parts) => {
            let mut out: Set = Set::new();
            for p in parts {
                match p {
                    InterpPart::Lit(_) => {}
                    InterpPart::Expr(x) => out.extend(key_vars(x)?),
                }
            }
            Some(out)
        }
        IrExpr::Arith(a) => arith(a),
        // a transform node's operands carry the read set (a splice's
        // base/start/end/repl are the reads its Interpolate had).
        IrExpr::Ext(n) => {
            let mut out: Set = Set::new();
            for c in n.children() {
                out.extend(key_vars(c)?);
            }
            Some(out)
        }
        // anything else is a node `pure_key` rejects — belt and braces.
        _ => None,
    }
}

/// Does this statement (bodies included) write any of `names`?
fn stmt_writes_any(st: &IrStmt, names: &std::collections::HashSet<String>) -> bool {
    let mut w: HashMap<String, usize> = HashMap::new();
    count_stmt_writes(st, &mut w);
    w.keys().any(|k| names.contains(k))
}

/// Every expression a statement reads at its OWN level (not its bodies).
fn stmt_exprs(st: &IrStmt) -> Vec<&IrExpr> {
    let mut out: Vec<&IrExpr> = Vec::new();
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            out.push(expr);
            for t in targets {
                out.extend(t.indices.iter());
            }
        }
        IrStmt::Declare { init, .. } => out.extend(init.iter()),
        IrStmt::DeclareArray { elements, .. } => out.extend(elements.iter()),
        IrStmt::Output { value, .. } => out.push(value),
        IrStmt::WriteFile { path, content, .. } => {
            out.push(path);
            out.push(content);
        }
        IrStmt::Expr(e) | IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => out.push(e),
        IrStmt::SetChildError(e) => out.push(e),
        IrStmt::If { cond, .. } => out.push(cond),
        IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => out.push(cond),
        IrStmt::For { iter, .. } => out.push(iter),
        IrStmt::Case { discriminant, .. } => out.push(discriminant),
        IrStmt::ForInit { cond, .. } => out.push(cond),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => out.push(expr),
        IrStmt::Exec {
            cmd,
            args,
            env,
            redirects,
            ..
        } => {
            out.push(cmd);
            out.extend(args.iter());
            for (_, e) in env {
                out.push(e);
            }
            out.extend(redirects.iter());
        }
        IrStmt::Asm {
            outputs,
            inputs,
            ..
        } => {
            for (_, e) in outputs.iter().chain(inputs.iter()) {
                out.push(e);
            }
        }
        _ => {}
    }
    out
}

// ── the rewrite ──────────────────────────────────────────────────────

/// The element read, in the SAME `Call { "arrayIndex", [Str(arr), key] }`
/// spelling the def itself is made of (`Word::MapAccess`'s lowering of
/// `${arr[$k]}`). Not `IrExpr::Index`: a shIR node's operands cross a
/// JSON codec (`shir_nodes::enc`) that encodes Call/Str/Var/… and maps
/// everything else to `Other`, which the decode side rejects — the
/// folded read lives inside StrSplice's `base`, so the spelling has to
/// be one the codec round-trips.
fn element_load(arr: &str, key: &IrExpr) -> IrExpr {
    IrExpr::Call {
        func: "arrayIndex".to_string(),
        args: vec![
            IrExpr::Str(arr.to_string(), crate::ir::StrStyle::DoubleQuoted),
            key.clone(),
        ],
    }
}

/// Replace every read of `v` in a statement's OWN expressions with the
/// element load. Bodies are left alone (they are never folded).
fn fold_top(st: &mut IrStmt, v: &str, arr: &str, key: &IrExpr) -> bool {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            let mut c = fold_expr(expr, v, arr, key);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    c |= fold_expr(i, v, arr, key);
                }
            }
            c
        }
        IrStmt::Declare { init: Some(i), .. } => fold_expr(i, v, arr, key),
        IrStmt::DeclareArray { elements, .. } => elements
            .iter_mut()
            .fold(false, |c, e| c | fold_expr(e, v, arr, key)),
        IrStmt::Output { value, .. } => fold_expr(value, v, arr, key),
        IrStmt::WriteFile { path, content, .. } => {
            let c = fold_expr(path, v, arr, key);
            c | fold_expr(content, v, arr, key)
        }
        IrStmt::Expr(e) | IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => {
            fold_expr(e, v, arr, key)
        }
        IrStmt::If { cond, .. }
        | IrStmt::While { cond, .. }
        | IrStmt::DoWhile { cond, .. }
        | IrStmt::For { iter: cond, .. }
        | IrStmt::Case { discriminant: cond, .. }
        | IrStmt::ForInit { cond, .. } => fold_expr(cond, v, arr, key),
        IrStmt::Exec {
            cmd,
            args,
            env,
            redirects,
            ..
        } => {
            let mut c = fold_expr(cmd, v, arr, key);
            for a in args.iter_mut() {
                c |= fold_expr(a, v, arr, key);
            }
            for (_, e) in env.iter_mut() {
                c |= fold_expr(e, v, arr, key);
            }
            for r in redirects.iter_mut() {
                c |= fold_expr(r, v, arr, key);
            }
            c
        }
        _ => false,
    }
}

fn fold_expr(e: &mut IrExpr, v: &str, arr: &str, key: &IrExpr) -> bool {
    match e {
        IrExpr::Var(n, _) if n == v => {
            *e = element_load(arr, key);
            true
        }
        // The two CALL spellings of a plain read: `param("", "v")` is
        // `${v}` (word expansion) and `getVar("v")` is `$v`. An identity
        // expansion IS the read, so the whole node becomes the element
        // load; a non-empty `param` op applies a string operator to the
        // value (only its operand list may fold) and the NAME slot is a
        // name, never a value read.
        IrExpr::Call { func, args, .. } if func == "param" || func == "getVar" => {
            let name_slot = if func == "param" { 1 } else { 0 };
            let identity = func == "getVar"
                || (args.len() == 2 && matches!(&args[0], IrExpr::Str(op, _) if op.is_empty()));
            if identity {
                if let Some(IrExpr::Str(n, _)) = args.get(name_slot) {
                    if n == v {
                        *e = element_load(arr, key);
                        return true;
                    }
                }
            }
            args.iter_mut()
                .enumerate()
                .fold(false, |c, (i, a)| c | (i != name_slot && fold_expr(a, v, arr, key)))
        }
        IrExpr::BinOp { lhs, rhs, .. } => fold_expr(lhs, v, arr, key) | fold_expr(rhs, v, arr, key),
        IrExpr::Ternary { cond, then, else_, .. } => {
            fold_expr(cond, v, arr, key)
                | fold_expr(then, v, arr, key)
                | fold_expr(else_, v, arr, key)
        }
        IrExpr::DefinedOr { expr, default } => {
            fold_expr(expr, v, arr, key) | fold_expr(default, v, arr, key)
        }
        IrExpr::Interpolate(parts) => parts.iter_mut().fold(false, |c, p| match p {
            InterpPart::Expr(x) => c | fold_expr(x, v, arr, key),
            InterpPart::Lit(_) => c,
        }),
        IrExpr::Array(es) => es
            .iter_mut()
            .fold(false, |c, x| c | fold_expr(x, v, arr, key)),
        // A transform node is its operand composition — fold through the
        // children IN PLACE (`children_mut` hands back the node's own
        // fields, so a replaced base/stored operand is kept by the node
        // itself; this is how a folded read reaches a StrSplice base).
        IrExpr::Ext(n) => n
            .children_mut()
            .into_iter()
            .fold(false, |c, x| c | fold_expr(x, v, arr, key)),
        // an arithmetic assignment/inc-dec to `v` is a write, not a read;
        // `stmt_refuses` only lets us here when the statement has none of
        // those w.r.t. `arr`, but `v` is a different name — leave it.
        IrExpr::Arith(_) | IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Var(_, _) => false,
        // calls/captures/closures were refused by `stmt_refuses`.
        _ => false,
    }
}

/// Any surviving read of `name` in this list or below (function bodies
/// included — a read there keeps the def alive). Used by the tests to
/// pin "the fold left no reader", which is DCE's removal precondition.
#[cfg_attr(not(test), allow(dead_code))]
fn any_read(list: &[IrStmt], name: &str) -> bool {
    for st in list {
        if stmt_exprs(st).iter().any(|e| expr_reads(e, name)) {
            return true;
        }
        if child_lists_read(st, name) {
            return true;
        }
    }
    false
}

fn expr_reads(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => n == name,
        IrExpr::Int(_) | IrExpr::Str(_, _) => false,
        // The CALL spellings of a name read — the same shapes
        // `fold_expr` rewrites: `getVar("v")`, identity
        // `param("", "v")` read the NAME in their string slot, and
        // `arrayIndex("v", k)` reads the array named there — exactly
        // like `Index { var: "v", .. }` below. Everything else reads
        // only what its own arguments read (the conservative `_`
        // arm would call the folded `arrayIndex` a possible read of
        // the scalar and mask a broken fold in these tests).
        IrExpr::Call { func, args, .. } => {
            let name_slot = if func == "getVar" {
                Some(0)
            } else if func == "param"
                && args.len() == 2
                && matches!(&args[0], IrExpr::Str(op, _) if op.is_empty())
            {
                Some(1)
            } else if func == "arrayIndex" {
                Some(0)
            } else {
                None
            };
            if let Some(slot) = name_slot {
                if matches!(args.get(slot), Some(IrExpr::Str(n, _)) if n == name) {
                    return true;
                }
            }
            args.iter().any(|a| expr_reads(a, name))
        }
        IrExpr::Index { var, key } => var == name || expr_reads(key, name),
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(lhs, name) || expr_reads(rhs, name),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_reads(cond, name) || expr_reads(then, name) || expr_reads(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => expr_reads(expr, name) || expr_reads(default, name),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(x) => expr_reads(x, name),
        }),
        IrExpr::Array(es) => es.iter().any(|x| expr_reads(x, name)),
        IrExpr::Arith(a) => arith_mentions(a, name),
        IrExpr::Ext(n) => n.children().iter().any(|c| expr_reads(c, name)),
        // conservative: an opaque node may read it
        _ => true,
    }
}

fn child_lists_mut(st: &mut IrStmt) -> Vec<&mut Vec<IrStmt>> {
    let mut out: Vec<&mut Vec<IrStmt>> = Vec::new();
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            out.push(then);
            for (_, b) in elsifs.iter_mut() {
                out.push(b);
            }
            out.push(else_);
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. } => out.push(body),
        // `Function` owns BOTH its body and its named blocks — one arm.
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            out.push(body);
            for (_, b) in named_blocks.iter_mut() {
                out.push(b);
            }
        }
        IrStmt::ForInit { init, step, body, .. } => {
            out.push(init);
            out.push(step);
            out.push(body);
        }
        IrStmt::Block(b) => out.push(b),
        IrStmt::Subshell(b) | IrStmt::Background(b) => out.push(b),
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            out.push(body);
            out.push(else_body);
            out.push(finally_body);
            for x in excepts.iter_mut() {
                out.push(&mut x.body);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses.iter_mut() {
                out.push(&mut c.body);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages.iter_mut() {
                out.push(s);
            }
        }
        IrStmt::Redirect { inner, .. } => out.push(inner),
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn tgt(v: &str) -> AssignTarget {
        AssignTarget {
            var: v.to_string(),
            sigil: None,
            indices: vec![],
        }
    }

    fn assign(v: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![tgt(v)],
            expr,
            asm: None,
        }
    }

    fn load(v: &str, arr: &str, idx: i64) -> IrStmt {
        assign(
            v,
            IrExpr::Index {
                var: arr.to_string(),
                key: Box::new(IrExpr::Int(idx)),
            },
        )
    }

    fn out(v: &str) -> IrStmt {
        IrStmt::Output {
            value: IrExpr::Var(v.to_string(), None),
            newline: true,
            target: None,
        }
    }

    #[test]
    fn forwards_a_single_def_element_load_and_drops_the_store() {
        let mut stmts = vec![
            load("bh", "TAB", 3),
            IrStmt::Output {
                value: IrExpr::Interpolate(vec![
                    InterpPart::Lit("x".into()),
                    InterpPart::Expr(Box::new(IrExpr::Var("bh".into(), None))),
                ]),
                newline: true,
                target: None,
            },
        ];
        assert!(transform(&mut stmts));
        // The read moved AND the orphaned store went with it: nothing
        // reads `bh` any more, so the fold's orphan sweep drops the def
        // itself (DSE's Const verdict would accept this one too — it is
        // a top-level site — but function-body sites never are, which is
        // why the sweep exists here).
        assert_eq!(stmts.len(), 1);
        assert!(!any_read(&stmts, "bh"));
        match &stmts[0] {
            IrStmt::Output { value, .. } => {
                let t = format!("{:?}", value);
                assert!(t.contains("Index"), "{}", t);
                assert!(!t.contains("\"bh\""), "{}", t);
            }
            other => panic!("{:?}", other),
        }
        // DCE, running directly after us, then has nothing left to do:
        assert!(!crate::transforms::dead_store_elim::transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn an_intervening_read_of_the_array_is_inert() {
        let mut stmts = vec![
            load("bh", "TAB", 3),
            load("gh", "TAB", 4), // reads TAB — harmless, does not stop the fold
            out("bh"),
        ];
        assert!(transform(&mut stmts));
        // `bh` lost its last reader, so the orphan sweep removed its
        // store; `gh` was never read at all — it was never FOLDED, so it
        // is not this pass's orphan to remove (DCE's verdict below).
        assert_eq!(stmts.len(), 2);
        assert!(!any_read(&stmts, "bh"));
        match &stmts[1] {
            IrStmt::Output { value, .. } => {
                let t = format!("{:?}", value);
                assert!(t.contains("Index"), "{}", t);
                assert!(!t.contains("\"bh\""), "{}", t);
            }
            other => panic!("{:?}", other),
        }
        // and DCE clears the untouched readerless store
        assert!(crate::transforms::dead_store_elim::transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn refuses_when_the_array_is_written_in_between() {
        let write_arr = IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "TAB".to_string(),
                sigil: None,
                indices: vec![IrExpr::Int(0)],
            }],
            expr: IrExpr::Str("z".into(), StrStyle::DoubleQuoted),
            asm: None,
        };
        let mut stmts = vec![load("bh", "TAB", 3), write_arr, out("bh")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn refuses_a_second_write_to_the_scalar() {
        let mut stmts = vec![load("bh", "TAB", 3), assign("bh", IrExpr::Int(7)), out("bh")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn refuses_a_call_between_def_and_read() {
        let mut stmts = vec![
            load("bh", "TAB", 3),
            IrStmt::Output {
                value: IrExpr::Call {
                    func: "f".into(),
                    args: vec![],
                },
                newline: true,
                target: None,
            },
            out("bh"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn a_read_before_the_def_never_folds() {
        // `bh` is read at the top (shell default: empty) and again after
        // the def — only the later one may fold, so the def must stay.
        let mut stmts = vec![out("bh"), load("bh", "TAB", 3), out("bh")];
        assert!(transform(&mut stmts));
        // the def survives (a read still needs it), the read after it folded
        assert_eq!(stmts.len(), 3);
        assert!(matches!(stmts[1], IrStmt::Assign { .. }));
        // and the FIRST read still sees the empty default, untouched
        match &stmts[0] {
            IrStmt::Output {
                value: IrExpr::Var(n, _),
                ..
            } => assert_eq!(n, "bh"),
            other => panic!("{:?}", other),
        }
    }

    // The shapes a real program carries: `${arr[$k]}` lowers to a
    // `Call { "arrayIndex" }` node (`Word::MapAccess`), and `${v}` reads
    // through `Call { "param" }` — MIMEcroft's rb_mkhex is this shape
    // three times over, and until both spellings were recognised the
    // pass never fired on it.
    fn arr_idx(arr: &str, key: &str) -> IrExpr {
        IrExpr::Call {
            func: "arrayIndex".into(),
            args: vec![
                IrExpr::Str(arr.into(), StrStyle::DoubleQuoted),
                IrExpr::Str(format!("${}", key), StrStyle::DoubleQuoted),
            ],
        }
    }

    fn param_read(n: &str) -> IrExpr {
        IrExpr::Call {
            func: "param".into(),
            args: vec![
                IrExpr::Str("".into(), StrStyle::DoubleQuoted),
                IrExpr::Str(n.into(), StrStyle::DoubleQuoted),
            ],
        }
    }

    #[test]
    fn forwards_the_call_spelling_across_sibling_element_loads() {
        // `bh=HEX[$b]; gh=HEX[$g]; px="$bh$gh"` — the two sibling loads
        // are read-only calls and must NOT stop the forward.
        let mut stmts = vec![
            assign("bh", arr_idx("HEX", "b")),
            assign("gh", arr_idx("HEX", "g")),
            assign(
                "px",
                IrExpr::Interpolate(vec![
                    InterpPart::Expr(Box::new(param_read("bh"))),
                    InterpPart::Expr(Box::new(param_read("gh"))),
                ]),
            ),
            out("px"), // the consumer of the consumer keeps px itself live
        ];
        assert!(transform(&mut stmts));
        // Both element copies lost their last reader, so the fold's
        // orphan sweep removed both Call-RHS stores; the consumer reads
        // the elements directly (`any_read` is deliberately conservative
        // about call nodes, so assert on the consumer itself):
        assert_eq!(stmts.len(), 2, "{:?}", stmts);
        let t = format!("{:?}", stmts[0]);
        assert!(t.contains("Index"), "{}", t);
        assert!(!t.contains("\"bh\"") && !t.contains("\"gh\""), "{}", t);
        // `px` itself is read by the trailing out(), so nothing is left
        // for DCE to remove:
        assert!(!crate::transforms::dead_store_elim::transform(&mut stmts));
        assert_eq!(stmts.len(), 2, "{:?}", stmts);
        let t = format!("{:?}", stmts[0]);
        assert!(t.contains("Index"), "{}", t);
        assert!(!t.contains("\"bh\"") && !t.contains("\"gh\""), "{}", t);
    }

    /// The `rb_mkhex` shape — a function-local copy whose only reader
    /// the forward moved away. `analyze_var_const` marks every site
    /// inside a function body multi-run ("a function may run 0..N
    /// times") → `Var` → `dead-store-elim` refuses it forever, so this
    /// pass's own orphan sweep is the only thing that can drop it.
    #[test]
    fn removes_a_function_body_orphan_dse_can_never_take() {
        let body = vec![
            assign("bh", arr_idx("HEX", "b")),
            assign(
                "px",
                IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(param_read("bh")))]),
            ),
            out("px"),
        ];
        let mut stmts = vec![IrStmt::Function {
            name: "mkhex".into(),
            params: vec![],
            body,
            named_blocks: vec![],
        }];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Function { body, .. } => {
                // the orphaned def is gone; the consumer and its own
                // reader stay:
                assert_eq!(body.len(), 2, "{:?}", body);
                let t = format!("{:?}", body[0]);
                assert!(t.contains("Index"), "{}", t);
                assert!(!t.contains("\"bh\""), "{}", t);
            }
            other => panic!("{:?}", other),
        }
        // and DSE — which refuses every multi-run site — confirms there
        // is nothing left it would need to touch:
        assert!(!crate::transforms::dead_store_elim::transform(&mut stmts));
    }

    #[test]
    fn refuses_when_the_key_variable_is_written_in_between() {
        // the key is re-evaluated at the forwarded read: a write to `k`
        // would make the read observe a different element than `bh` did.
        let mut stmts = vec![
            assign("bh", arr_idx("HEX", "k")),
            assign("k", IrExpr::Int(9)),
            out("bh"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }
}