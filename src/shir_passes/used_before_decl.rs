//! Use-before-declaration analysis — a conservative "definitely
//! uninitialized" dataflow over `IrProgram`.
//!
//! This is the sibling the existing passes were missing:
//!
//! - [`crate::shir_passes::lifetime`] answers *WHERE* a value lives
//!   (live spans + escape set) — but its `first` is the first *access*,
//!   def OR use, so it cannot tell whether the first touch was a read.
//! - [`crate::shir_passes::analysis::ConstVar`] answers *HOW MANY*
//!   static assignment sites a variable has (one → `Const`), not their
//!   ordering.
//!
//! This pass answers the third question: **is a variable READ on a
//! control-flow path that has no prior definition?** — the check behind
//! PowerShell's `Set-StrictMode` undeclared-variable error (and a useful
//! lint everywhere, since shell `$x` reads on an unset var are silently
//! empty rather than errors). Because it runs on the language-neutral
//! `IrProgram`, it is automatically available to every backend (Perl,
//! Java, Python, ESTree, and a future PowerShell frontend) and to the
//! `--shir` JSON contract.
//!
//! # Method
//!
//! A forward **must-defined** dataflow. Each statement carries a
//! `must_def: HashSet<String>` — the variables *definitely* assigned on
//! every path reaching that point. A read (any `IrExpr::Var` / `Ident` /
//! `Index` / arith `Var` / `getVar`) whose name is **not** in `must_def`
//! is reported at that statement position.
//!
//! Control flow:
//!
//! - **Branches** (`If` / `Case` / `Try` / `Select`) meet with
//!   *intersection*: a variable is must-defined after the branch only if
//!   it is must-defined through *every* outcome — including the
//!   skip-all-branches path, which is just the entry set (an `if` with no
//!   `else` may not run at all).
//! - **Loops** (`While` / `DoWhile` / `For` / `ForInit`) fall back to the
//!   entry set on exit: a loop may run zero times, so nothing defined
//!   only inside the body is must-defined after it.
//! - **`Subshell` / `Background` / `Pipeline` stages** do not propagate
//!   writes back to the parent (bash fork semantics) — the body is
//!   analyzed with the entry set and the parent set is restored.
//! - **Functions / closures** are entered with the entry set unioned
//!   with the program-wide top-level-assigned set (see below).
//!
//! # Soundness / false-positive policy
//!
//! Conservative in the SAFE direction for a *warning*: we only report a
//! read as uninitialized when we can prove no definition reaches it on
//! ANY path. Therefore:
//!
//! - We may **miss** real bugs (under-approximate the warnings) when
//!   control flow is too dynamic to prove a def — never wrong, just
//!   silent.
//! - We never **falsely** warn on a path that has a def (the meet is an
//!   intersection, never a union).
//!
//! Two deliberate over-approximations keep the false-positive rate down,
//! mirroring the over-approximate "escape" philosophy in `lifetime`:
//!
//! 1. **Function / closure bodies** are entered with `globals_defined`
//!    (every variable assigned at top level anywhere in the program)
//!    unioned in. A function called after a top-level assignment sees
//!   that var as defined — e.g. `x=5; f(){ echo $x }` must not warn —
//!   even though call order cannot be proven statically. A variable that
//!   is NEVER assigned anywhere in the program is still flagged (that is
//!   the genuine `echo $typo` typo).
//! 2. **Dynamic stores** (`eval`, `source`, nameref/`!var`, indirect
//!   assignment) are treated as defining every name they touch. They
//!   cannot be enumerated statically, so we over-approximate by not
//!   warning on anything they could reach (a future refined `sh2.eval` /
//!   nameref model can narrow this).
//!
//! # Determinism
//!
//! Findings are deduplicated (earliest position per variable) and sorted
//! by variable name, then position, so `--shir` JSON and the test harness
//! see byte-identical output for the same input.

use std::collections::HashSet;

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt};
use crate::shir_passes::PassContext;

/// One report: `var` is read at `stmt_pos` before any definition on the
/// reaching path. `stmt_pos` is the pre-order statement position (the
/// same numbering family as `lifetime::analyze_var_lifetimes`, computed
/// independently by this pass).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseBeforeDeclFinding {
    pub var: String,
    pub stmt_pos: usize,
}

/// The pipeline analysis: populates `ctx.use_before_decl`.
pub struct UseBeforeDecl;

impl super::Analysis for UseBeforeDecl {
    fn name(&self) -> &'static str {
        "use_before_decl"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.use_before_decl = analyze_use_before_decl(prog);
    }
}

/// Run the analysis. See the module docs for the method and the
/// soundness policy.
pub fn analyze_use_before_decl(prog: &IrProgram) -> Vec<UseBeforeDeclFinding> {
    // Variables assigned at top level anywhere in the program (incl. subs).
    // Unioned into function/closure entry sets so top-level-then-call
    // patterns don't false-positive.
    let globals_defined = collect_program_assigned(prog);

    let mut report: Vec<UseBeforeDeclFinding> = Vec::new();
    let mut pos: usize = 0;
    let mut entry: HashSet<String> = HashSet::new();

    analyze_block(&prog.stmts, &mut pos, &mut entry, &globals_defined, &mut report);
    for sub in &prog.subs {
        // A sub is its own scope; reset the entry set but keep the
        // program-wide globals so cross-sub reads don't false-positive.
        let mut sub_entry = globals_defined.clone();
        let mut sub_pos = pos; // continue numbering across subs (deterministic)
        analyze_block(&sub.body, &mut sub_pos, &mut sub_entry, &globals_defined, &mut report);
        pos = sub_pos;
    }

    // Dedupe: keep the earliest position per var (a var read uninitialized
    // in several spots is one finding). Sort by (var, pos) for determinism.
    let mut by_var: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in report {
        by_var.entry(f.var.clone()).or_insert(f.stmt_pos);
    }
    let mut out: Vec<UseBeforeDeclFinding> = by_var
        .into_iter()
        .map(|(var, stmt_pos)| UseBeforeDeclFinding { var, stmt_pos })
        .collect();
    out.sort_by(|a, b| a.var.cmp(&b.var).then(a.stmt_pos.cmp(&b.stmt_pos)));
    out
}

/// Analyze a statement list. Returns nothing; `must_def` is mutated to the
/// post-block set. Stops walking after a terminating statement
/// (`Return` / `Exit` / `Die` / `Break` / `Continue` / `Goto`) so dead
/// code after it is never flagged.
fn analyze_block(
    stmts: &[IrStmt],
    pos: &mut usize,
    must_def: &mut HashSet<String>,
    globals: &HashSet<String>,
    report: &mut Vec<UseBeforeDeclFinding>,
) {
    for st in stmts {
        *pos += 1;
        let terminated = analyze_stmt(st, *pos, must_def, globals, report);
        if terminated {
            break;
        }
    }
}

/// `true` if the statement terminates the block (unreachable code follows).
fn analyze_stmt(
    st: &IrStmt,
    pos: usize,
    must_def: &mut HashSet<String>,
    globals: &HashSet<String>,
    report: &mut Vec<UseBeforeDeclFinding>,
) -> bool {
    match st {
        IrStmt::Ext(n) => {
            // Walk the node's children as a sub-block (mirrors lifetime.rs).
            let mut ep = pos;
            for c in crate::shir_nodes::ExtNode::children(&**n) {
                ep += 1;
                if analyze_stmt(c, ep, must_def, globals, report) {
                    break;
                }
            }
            false
        }

        // ── Assignments: RHS reads use the pre-assignment set; the
        //    targets become defined AFTER evaluating the RHS (so `x=$x`
        //    reads the prior value, and `x=1` then defines x). ────────
        IrStmt::Assign { targets, expr, asm } => {
            walk_expr_reads(expr, pos, must_def, globals, report);
            for t in targets {
                if t.indices.is_empty() {
                    must_def.insert(t.var.clone());
                } else {
                    // array-element write: the array becomes defined.
                    must_def.insert(t.var.clone());
                    for k in &t.indices {
                        walk_expr_reads(k, pos, must_def, globals, report);
                    }
                }
            }
            if let Some(spec) = asm {
                for (_, t) in &spec.outputs {
                    // output operand: the asm WRITES `name` (not a read);
                    // any nested expr is a read, but a bare Var target is
                    // the destination, so don't flag it as a use.
                    if let IrExpr::Var(name, _) = t {
                        must_def.insert(name.clone());
                    } else {
                        walk_expr_reads(t, pos, must_def, globals, report);
                    }
                }
                for (_, e) in &spec.inputs {
                    walk_expr_reads(e, pos, must_def, globals, report);
                }
            }
            false
        }

        IrStmt::Declare { vars, init, .. } => {
            if let Some(e) = init {
                // `declare x=$x` reads the prior value; define after.
                walk_expr_reads(e, pos, must_def, globals, report);
            }
            for d in vars {
                must_def.insert(d.name.clone());
            }
            false
        }

        IrStmt::DeclareArray { var, elements, .. } => {
            for e in elements {
                walk_expr_reads(e, pos, must_def, globals, report);
            }
            must_def.insert(var.clone());
            false
        }

        // ── Conditionals: meet over branches (see module docs). ──────
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            // cond may define vars (arith assign / setVar) — visible to the
            // taken branch only. Evaluate against a clone so prior-branch
            // defs don't leak into later cond evaluations.
            let mut cond_md = must_def.clone();
            walk_expr_reads(cond, pos, &mut cond_md, globals, report);
            let mut then_md = cond_md;
            analyze_block(then, &mut { pos }, &mut then_md, globals, report);

            let mut branches = vec![then_md];
            for (ec, eb) in elsifs {
                let mut econd_md = must_def.clone();
                walk_expr_reads(ec, pos, &mut econd_md, globals, report);
                let mut body_md = econd_md;
                analyze_block(eb, &mut { pos }, &mut body_md, globals, report);
                branches.push(body_md);
            }
            let mut else_md = must_def.clone();
            if !else_.is_empty() {
                analyze_block(else_, &mut { pos }, &mut else_md, globals, report);
                branches.push(else_md);
            }
            // A trailing `else` (not an `elsif`) removes the skip path; an
            // `elsif` alone still leaves the skip-when-all-conditions-false
            // path, which is just the entry set.
            let has_skip = else_.is_empty();
            *must_def = meet_branches(&branches, &must_def.clone(), has_skip);
            false
        }

        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let entry = must_def.clone();
            walk_expr_reads(discriminant, pos, must_def, globals, report);
            let mut branches = Vec::new();
            for cl in clauses {
                let mut body_md = entry.clone();
                analyze_block(&cl.body, &mut { pos }, &mut body_md, globals, report);
                branches.push(body_md);
            }
            // A `*` default clause removes the skip path.
            let has_skip = !clauses.iter().any(|cl| cl.patterns.iter().any(|p| p == "*"));
            *must_def = meet_branches(&branches, &entry, has_skip);
            false
        }

        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let entry = must_def.clone();
            let mut body_md = entry.clone();
            analyze_block(body, &mut { pos }, &mut body_md, globals, report);
            let mut exit = body_md; // non-finally exit baseline
            for ex in excepts {
                let mut ex_md = entry.clone();
                if let Some(m) = &ex.match_expr {
                    walk_expr_reads(m, pos, &mut ex_md, globals, report);
                }
                if let Some(n) = &ex.as_name {
                    ex_md.insert(n.clone());
                }
                analyze_block(&ex.body, &mut { pos }, &mut ex_md, globals, report);
                exit = meet(&exit, &ex_md);
            }
            if !else_body.is_empty() {
                let mut else_md = entry.clone();
                analyze_block(else_body, &mut { pos }, &mut else_md, globals, report);
                exit = meet(&exit, &else_md);
            }
            // finally runs on every path.
            if !finally_body.is_empty() {
                let mut fin_md = entry.clone();
                analyze_block(finally_body, &mut { pos }, &mut fin_md, globals, report);
                exit = fin_md;
            }
            *must_def = exit;
            false
        }

        IrStmt::Select { clauses } => {
            let entry = must_def.clone();
            let mut branches = Vec::new();
            for cl in clauses {
                let mut body_md = entry.clone();
                if let Some(ch) = &cl.ch {
                    walk_expr_reads(ch, pos, &mut body_md, globals, report);
                }
                if let Some(v) = &cl.value {
                    walk_expr_reads(v, pos, &mut body_md, globals, report);
                }
                if let Some(t) = &cl.target {
                    body_md.insert(t.clone()); // received value is defined
                }
                analyze_block(&cl.body, &mut { pos }, &mut body_md, globals, report);
                branches.push(body_md);
            }
            // A `default` comm clause removes the skip path.
            let has_skip = !clauses.iter().any(|cl| cl.comm == "default");
            *must_def = meet_branches(&branches, &entry, has_skip);
            false
        }

        // ── Loops: body entered with the entry set; exit falls back to
        //    the entry set (a loop may run zero times). ────────────────
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            let entry = must_def.clone();
            let mut cond_md = entry.clone();
            walk_expr_reads(cond, pos, &mut cond_md, globals, report);
            let mut body_md = cond_md; // body sees entry + cond defs
            analyze_block(body, &mut { pos }, &mut body_md, globals, report);
            *must_def = entry; // zero-iteration path dominates
            false
        }

        IrStmt::For { var, iter, body } => {
            let entry = must_def.clone();
            walk_expr_reads(iter, pos, must_def, globals, report); // iter runs first
            let mut body_md = must_def.clone();
            body_md.insert(var.clone()); // loop var defined each iteration
            analyze_block(body, &mut { pos }, &mut body_md, globals, report);
            *must_def = entry; // for-in may run zero times
            false
        }

        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            // init runs unconditionally once.
            analyze_block(init, &mut { pos }, must_def, globals, report);
            let post_init = must_def.clone();
            let mut cond_md = post_init.clone();
            walk_expr_reads(cond, pos, &mut cond_md, globals, report);
            let mut body_md = cond_md;
            analyze_block(body, &mut { pos }, &mut body_md, globals, report);
            // step runs each iteration; we don't model the fixed point, so
            // its defs don't leak out (zero-iteration path = post_init).
            let mut step_md = post_init.clone();
            analyze_block(step, &mut { pos }, &mut step_md, globals, report);
            *must_def = post_init;
            false
        }

        // ── Scoping wrappers. ───────────────────────────────────────
        IrStmt::Block(b) => {
            analyze_block(b, &mut { pos }, must_def, globals, report);
            false
        }
        IrStmt::Subshell(b) | IrStmt::Background(b) => {
            // fork: child observes entry, writes don't propagate back.
            let entry = must_def.clone();
            let mut md = entry.clone();
            analyze_block(b, &mut { pos }, &mut md, globals, report);
            *must_def = entry;
            false
        }
        IrStmt::Pipeline { stages, .. } => {
            // each stage is a subshell; defs don't propagate between stages.
            let entry = must_def.clone();
            for stg in stages {
                let mut md = entry.clone();
                analyze_block(stg, &mut { pos }, &mut md, globals, report);
            }
            *must_def = entry;
            false
        }
        IrStmt::Redirect { inner, redirects } => {
            for r in redirects {
                walk_expr_reads(&r.target, pos, must_def, globals, report);
            }
            analyze_block(inner, &mut { pos }, must_def, globals, report);
            false
        }

        IrStmt::Function {
            name,
            body,
            named_blocks,
        } => {
            // the name is now callable/defined in the enclosing scope.
            must_def.insert(name.clone());
            // body entered with entry ∪ program globals (capture assumption).
            let mut body_md: HashSet<String> = must_def.clone();
            body_md.extend(globals.iter().cloned());
            analyze_block(body, &mut { pos }, &mut body_md, globals, report);
            for (_, nb) in named_blocks {
                let mut nb_md = must_def.clone();
                nb_md.extend(globals.iter().cloned());
                analyze_block(nb, &mut { pos }, &mut nb_md, globals, report);
            }
            // defs inside the function do not propagate to the caller scope.
            false
        }

        // ── Plain reads / terminators. ──────────────────────────────
        IrStmt::Output { value, .. } => {
            walk_expr_reads(value, pos, must_def, globals, report);
            false
        }
        IrStmt::WriteFile { path, content, .. } => {
            walk_expr_reads(path, pos, must_def, globals, report);
            walk_expr_reads(content, pos, must_def, globals, report);
            false
        }
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            walk_expr_reads(cmd, pos, must_def, globals, report);
            for a in args {
                walk_expr_reads(a, pos, must_def, globals, report);
            }
            for r in redirects {
                walk_expr_reads(r, pos, must_def, globals, report);
            }
            for (_, v) in env {
                walk_expr_reads(v, pos, must_def, globals, report);
            }
            false
        }
        IrStmt::SetChildError(e) => {
            walk_expr_reads(e, pos, must_def, globals, report);
            false
        }
        IrStmt::Warn { expr, .. } => {
            walk_expr_reads(expr, pos, must_def, globals, report);
            false
        }
        IrStmt::Die { expr, .. } => {
            walk_expr_reads(expr, pos, must_def, globals, report);
            true // terminator
        }
        IrStmt::Return(Some(e)) => {
            walk_expr_reads(e, pos, must_def, globals, report);
            true
        }
        IrStmt::Exit(Some(e)) => {
            walk_expr_reads(e, pos, must_def, globals, report);
            true
        }
        IrStmt::Return(None) | IrStmt::Exit(None) => true,
        IrStmt::Continue | IrStmt::Break => true,
        IrStmt::Label(_) => false, // marker, no dataflow
        IrStmt::Goto(_) => true,    // terminator (unreachable after)

        IrStmt::Expr(e) => {
            walk_expr_reads(e, pos, must_def, globals, report);
            false
        }

        IrStmt::Asm {
            outputs, inputs, ..
        } => {
            for (_, t) in outputs {
                if let IrExpr::Var(n, _) = t {
                    must_def.insert(n.clone()); // write target
                } else {
                    walk_expr_reads(t, pos, must_def, globals, report);
                }
            }
            for (_, e) in inputs {
                walk_expr_reads(e, pos, must_def, globals, report);
            }
            false
        }

        // `Require` / `RawText` carry no dataflow we can see.
        IrStmt::Require(_) | IrStmt::RawText(_) => false,
    }
}

/// Walk an expression, reporting uninitialized reads and applying any
/// definitions it contains (arith `Assign`/`IncDec`, `setVar`,
/// `setArray`/`setArrayAppend`). Mutates `must_def`.
fn walk_expr_reads(
    e: &IrExpr,
    pos: usize,
    must_def: &mut HashSet<String>,
    globals: &HashSet<String>,
    report: &mut Vec<UseBeforeDeclFinding>,
) {
    match e {
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            if !must_def.contains(name) {
                report.push(UseBeforeDeclFinding {
                    var: name.clone(),
                    stmt_pos: pos,
                });
            }
        }
        IrExpr::Index { var, key } => {
            if !must_def.contains(var) {
                report.push(UseBeforeDeclFinding {
                    var: var.clone(),
                    stmt_pos: pos,
                });
            }
            walk_expr_reads(key, pos, must_def, globals, report);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_expr_reads(lhs, pos, must_def, globals, report);
            walk_expr_reads(rhs, pos, must_def, globals, report);
        }
        IrExpr::Call { func, args } => match func.as_str() {
            // getVar("x") — a $x read
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if !must_def.contains(name) {
                        report.push(UseBeforeDeclFinding {
                            var: name.clone(),
                            stmt_pos: pos,
                        });
                    }
                }
            }
            // setVar("x", v) — a $x write (after evaluating v)
            "setVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if let Some(v) = args.get(1) {
                        walk_expr_reads(v, pos, must_def, globals, report);
                    }
                    must_def.insert(name.clone());
                }
            }
            // setArray / setArrayAppend("a", ...) — array write
            "setArray" | "setArrayAppend" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    for a in args.iter().skip(1) {
                        walk_expr_reads(a, pos, must_def, globals, report);
                    }
                    must_def.insert(name.clone());
                }
            }
            // define(name, Arrow) — Arrow is a closure; fnCall is a call.
            "define" | "fnCall" => {
                for a in args {
                    walk_expr_reads(a, pos, must_def, globals, report);
                }
                if func == "define" {
                    if let Some(IrExpr::Arrow(body)) = args.get(1) {
                        walk_arrow(body, pos, must_def, globals, report);
                    }
                }
            }
            // subprocess boundaries: args are uses (kernel copies)
            "exec" | "pipeline" | "capture" | "captureWords" | "redirect" | "subshell"
            | "background" | "commandSubstitution" | "forLoop" | "whileLoop"
            | "whileLoopSync" | "cstyleFor" | "cstyleForSync" | "forIn" | "forOf" => {
                for a in args {
                    walk_expr_reads(a, pos, must_def, globals, report);
                }
            }
            // everything else (param/test/contains/arith/brace/...):
            // walk args as ordinary expressions.
            _ => {
                for a in args {
                    walk_expr_reads(a, pos, must_def, globals, report);
                }
            }
        },
        IrExpr::MethodCall { obj, args, .. } => {
            walk_expr_reads(obj, pos, must_def, globals, report);
            for a in args {
                walk_expr_reads(a, pos, must_def, globals, report);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_expr_reads(cond, pos, must_def, globals, report);
            walk_expr_reads(then, pos, must_def, globals, report);
            walk_expr_reads(else_, pos, must_def, globals, report);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_expr_reads(expr, pos, must_def, globals, report);
            walk_expr_reads(default, pos, must_def, globals, report);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    walk_expr_reads(x, pos, must_def, globals, report);
                }
            }
        }
        IrExpr::Capture { expr, .. } => {
            walk_expr_reads(expr, pos, must_def, globals, report);
        }
        IrExpr::Arrow(body) => {
            walk_arrow(body, pos, must_def, globals, report);
        }
        IrExpr::Array(items) => {
            for i in items {
                walk_expr_reads(i, pos, must_def, globals, report);
            }
        }
        IrExpr::Arith(a) => walk_arith_reads(a, pos, must_def, globals, report),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            walk_expr_reads(iter, pos, must_def, globals, report);
            walk_expr_reads(elem, pos, must_def, globals, report);
            if let Some(c) = cond {
                walk_expr_reads(c, pos, must_def, globals, report);
            }
        }
        IrExpr::Lambda { params, body } => {
            // params are bound inside the lambda body
            let mut md = must_def.clone();
            md.extend(globals.iter().cloned());
            for p in params {
                md.insert(p.clone());
            }
            walk_arrow(body, pos, &mut md, globals, report);
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                walk_expr_reads(v, pos, must_def, globals, report);
            }
        }
        IrExpr::Splice(e) => walk_expr_reads(e, pos, must_def, globals, report),
        IrExpr::Ext(n) => {
            for c in n.children() {
                walk_expr_reads(c, pos, must_def, globals, report);
            }
        }
        IrExpr::Str(_, _)
        | IrExpr::Int(_)
        | IrExpr::Bool(_)
        | IrExpr::Json(_)
        | IrExpr::Regex { .. }
        | IrExpr::Range { .. }
        | IrExpr::RawExpr(_) => {}
    }
}

/// Analyze a closure/arrow/lambda body. The body is its own scope entered
/// with the current `must_def` unioned with `globals` (capture
/// assumption); defs inside do not propagate back to the caller.
fn walk_arrow(
    body: &[IrStmt],
    pos: usize,
    must_def: &mut HashSet<String>,
    globals: &HashSet<String>,
    report: &mut Vec<UseBeforeDeclFinding>,
) {
    let mut md = must_def.clone();
    md.extend(globals.iter().cloned());
    let mut p = pos;
    analyze_block(body, &mut p, &mut md, globals, report);
}

fn walk_arith_reads(
    a: &ArithAst,
    pos: usize,
    must_def: &mut HashSet<String>,
    globals: &HashSet<String>,
    report: &mut Vec<UseBeforeDeclFinding>,
) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            if !must_def.contains(name) {
                report.push(UseBeforeDeclFinding {
                    var: name.clone(),
                    stmt_pos: pos,
                });
            }
        }
        ArithAst::Index { var, key } => {
            if !must_def.contains(var) {
                report.push(UseBeforeDeclFinding {
                    var: var.clone(),
                    stmt_pos: pos,
                });
            }
            walk_arith_reads(key, pos, must_def, globals, report);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            walk_arith_reads(lhs, pos, must_def, globals, report);
            walk_arith_reads(rhs, pos, must_def, globals, report);
        }
        ArithAst::Un { arg, .. } => walk_arith_reads(arg, pos, must_def, globals, report),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            walk_arith_reads(test, pos, must_def, globals, report);
            walk_arith_reads(then, pos, must_def, globals, report);
            walk_arith_reads(else_, pos, must_def, globals, report);
        }
        ArithAst::Assign { var, rhs, .. } => {
            // read-modify-write: evaluate rhs FIRST (reads use the current
            // must_def), THEN the assignment defines `var`.
            walk_arith_reads(rhs, pos, must_def, globals, report);
            must_def.insert(var.clone());
        }
        ArithAst::IncDec { var, .. } => {
            // x++ / x-- : reads the old value, then writes.
            if !must_def.contains(var) {
                report.push(UseBeforeDeclFinding {
                    var: var.clone(),
                    stmt_pos: pos,
                });
            }
            must_def.insert(var.clone());
        }
        ArithAst::Num(_) => {}
        ArithAst::Sizeof(_) => {}
        ArithAst::Cast { arg, .. } => walk_arith_reads(arg, pos, must_def, globals, report),
    }
}

/// Set intersection (the dataflow meet).
fn meet(a: &HashSet<String>, b: &HashSet<String>) -> HashSet<String> {
    a.intersection(b).cloned().collect()
}

/// Meet over a list of branch exit sets, with an optional *skip* path (the
/// `entry` set, taken when no branch runs — a bare `if`/`case`/`select` with
/// no `else`/`*`-default/`default` clause). When `has_skip` is true the entry
/// set participates in the meet (a variable is must-defined after the branch
/// only if it was already must-defined before it); when false the entry is
/// excluded (some branch always runs, so a variable defined on *every* branch
/// is must-defined even if it was undeclared before the branch).
fn meet_branches(
    branches: &[HashSet<String>],
    entry: &HashSet<String>,
    has_skip: bool,
) -> HashSet<String> {
    let mut acc: Option<HashSet<String>> = if has_skip { Some(entry.clone()) } else { None };
    for b in branches {
        acc = Some(match acc {
            Some(a) => meet(&a, b),
            None => b.clone(),
        });
    }
    acc.unwrap_or_else(|| entry.clone())
}

/// Pre-scan: every variable assigned at top level anywhere in the program
/// (top-level statements + sub bodies), used as the capture assumption for
/// function/closure bodies. Assignment sites: `Assign` targets, `Declare` /
/// `DeclareArray` vars, `For` loop vars, `ForInit` init statements, and
/// runtime-store calls (`setVar` / `setArray`) and arith `Assign` / `IncDec`
/// nested in top-level expressions.
fn collect_program_assigned(prog: &IrProgram) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut collect_stmts = |stmts: &[IrStmt]| {
        for st in stmts {
            collect_assign_in_stmt(st, &mut out);
        }
    };
    collect_stmts(&prog.stmts);
    for sub in &prog.subs {
        // a sub's body is its own scope, but its top-level assigns are
        // treat-as-global so functions that capture them don't false-positive.
        collect_stmts(&sub.body);
    }
    out
}

fn collect_assign_in_stmt(st: &IrStmt, out: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                out.insert(t.var.clone());
            }
            collect_assign_in_expr(expr, out);
        }
        IrStmt::Declare { vars, init, .. } => {
            for d in vars {
                out.insert(d.name.clone());
            }
            if let Some(e) = init {
                collect_assign_in_expr(e, out);
            }
        }
        IrStmt::DeclareArray { var, .. } => {
            out.insert(var.clone());
        }
        IrStmt::For { var, .. } => {
            out.insert(var.clone());
        }
        IrStmt::ForInit { init, .. } => {
            for s in init {
                collect_assign_in_stmt(s, out);
            }
        }
        _ => {}
    }
}

fn collect_assign_in_expr(e: &IrExpr, out: &mut HashSet<String>) {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "setVar" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.insert(n.clone());
                }
            }
            "setArray" | "setArrayAppend" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.insert(n.clone());
                }
            }
            _ => {
                for a in args {
                    collect_assign_in_expr(a, out);
                }
            }
        },
        IrExpr::Arith(a) => collect_assign_in_arith(a, out),
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_assign_in_expr(lhs, out);
            collect_assign_in_expr(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_assign_in_expr(cond, out);
            collect_assign_in_expr(then, out);
            collect_assign_in_expr(else_, out);
        }
        IrExpr::Index { key, .. } => collect_assign_in_expr(key, out),
        IrExpr::MethodCall { obj, args, .. } => {
            collect_assign_in_expr(obj, out);
            for a in args {
                collect_assign_in_expr(a, out);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_assign_in_expr(i, out);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    collect_assign_in_expr(x, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_assign_in_arith(a: &ArithAst, out: &mut HashSet<String>) {
    match a {
        ArithAst::Assign { var, rhs, .. } => {
            collect_assign_in_arith(rhs, out);
            out.insert(var.clone());
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_assign_in_arith(lhs, out);
            collect_assign_in_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_assign_in_arith(arg, out),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_assign_in_arith(test, out);
            collect_assign_in_arith(then, out);
            collect_assign_in_arith(else_, out);
        }
        ArithAst::Index { key, .. } => collect_assign_in_arith(key, out),
        ArithAst::Cast { arg, .. } => collect_assign_in_arith(arg, out),
        _ => {}
    }
}

/// Collect every variable DEF and READ across the whole program (no
/// dataflow — a plain set union). Used by the lint backend for the
/// "assigned-but-never-read" lint. The read/write classification is the
/// SAME one [`analyze_use_before_decl`] uses; this is the cheaper sibling
/// (a single set-difference, not a must-defined dataflow).
///
/// `IrProgram` subs (`IrSub`) are separate scopes but for a lint we treat
/// the whole program's defs/reads as one universe (an unused top-level
/// helper is still unused).
pub fn collect_var_defs_reads(prog: &IrProgram) -> (HashSet<String>, HashSet<String>) {
    let mut defs = HashSet::new();
    let mut reads = HashSet::new();
    let mut roots: Vec<&[IrStmt]> = vec![&prog.stmts];
    for sub in &prog.subs {
        roots.push(&sub.body);
    }
    for s in &roots {
        walk_defs_reads_stmts(s, &mut defs, &mut reads);
    }
    (defs, reads)
}

fn walk_defs_reads_stmts(stmts: &[IrStmt], defs: &mut HashSet<String>, reads: &mut HashSet<String>) {
    for st in stmts {
        walk_defs_reads_stmt(st, defs, reads);
    }
}

fn walk_defs_reads_stmt(st: &IrStmt, defs: &mut HashSet<String>, reads: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { targets, expr, asm } => {
            walk_defs_reads_expr(expr, defs, reads);
            for t in targets {
                defs.insert(t.var.clone());
                for k in &t.indices {
                    walk_defs_reads_expr(k, defs, reads);
                }
            }
            if let Some(spec) = asm {
                for (_, t) in &spec.outputs {
                    if let IrExpr::Var(n, _) = t {
                        defs.insert(n.clone());
                    } else {
                        walk_defs_reads_expr(t, defs, reads);
                    }
                }
                for (_, e) in &spec.inputs {
                    walk_defs_reads_expr(e, defs, reads);
                }
            }
        }
        IrStmt::Declare { vars, init, .. } => {
            if let Some(e) = init {
                walk_defs_reads_expr(e, defs, reads);
            }
            for d in vars {
                defs.insert(d.name.clone());
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            for e in elements {
                walk_defs_reads_expr(e, defs, reads);
            }
            defs.insert(var.clone());
        }
        IrStmt::For { var, iter, body } => {
            walk_defs_reads_expr(iter, defs, reads);
            defs.insert(var.clone());
            walk_defs_reads_stmts(body, defs, reads);
        }
        IrStmt::ForInit { init, cond, step, body } => {
            walk_defs_reads_stmts(init, defs, reads);
            walk_defs_reads_expr(cond, defs, reads);
            walk_defs_reads_stmts(step, defs, reads);
            walk_defs_reads_stmts(body, defs, reads);
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            walk_defs_reads_expr(cond, defs, reads);
            walk_defs_reads_stmts(then, defs, reads);
            for (ec, eb) in elsifs {
                walk_defs_reads_expr(ec, defs, reads);
                walk_defs_reads_stmts(eb, defs, reads);
            }
            walk_defs_reads_stmts(else_, defs, reads);
        }
        IrStmt::Case { discriminant, clauses } => {
            walk_defs_reads_expr(discriminant, defs, reads);
            for cl in clauses {
                walk_defs_reads_stmts(&cl.body, defs, reads);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            walk_defs_reads_stmts(body, defs, reads);
            for ex in excepts {
                if let Some(m) = &ex.match_expr {
                    walk_defs_reads_expr(m, defs, reads);
                }
                walk_defs_reads_stmts(&ex.body, defs, reads);
            }
            walk_defs_reads_stmts(else_body, defs, reads);
            walk_defs_reads_stmts(finally_body, defs, reads);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses {
                if let Some(ch) = &cl.ch {
                    walk_defs_reads_expr(ch, defs, reads);
                }
                if let Some(v) = &cl.value {
                    walk_defs_reads_expr(v, defs, reads);
                }
                if let Some(t) = &cl.target {
                    defs.insert(t.clone());
                }
                walk_defs_reads_stmts(&cl.body, defs, reads);
            }
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            walk_defs_reads_expr(cond, defs, reads);
            walk_defs_reads_stmts(body, defs, reads);
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            walk_defs_reads_stmts(b, defs, reads);
        }
        IrStmt::Pipeline { stages, .. } => {
            for stg in stages {
                walk_defs_reads_stmts(stg, defs, reads);
            }
        }
        IrStmt::Function { name, body, named_blocks } => {
            defs.insert(name.clone());
            walk_defs_reads_stmts(body, defs, reads);
            for (_, nb) in named_blocks {
                walk_defs_reads_stmts(nb, defs, reads);
            }
        }
        IrStmt::Output { value, .. } => walk_defs_reads_expr(value, defs, reads),
        IrStmt::WriteFile { path, content, .. } => {
            walk_defs_reads_expr(path, defs, reads);
            walk_defs_reads_expr(content, defs, reads);
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            walk_defs_reads_expr(cmd, defs, reads);
            for a in args {
                walk_defs_reads_expr(a, defs, reads);
            }
            for r in redirects {
                walk_defs_reads_expr(r, defs, reads);
            }
            for (_, v) in env {
                walk_defs_reads_expr(v, defs, reads);
            }
        }
        IrStmt::SetChildError(e) | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. }
        | IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => walk_defs_reads_expr(e, defs, reads),
        IrStmt::Expr(e) => walk_defs_reads_expr(e, defs, reads),
        IrStmt::Asm { outputs, inputs, .. } => {
            for (_, t) in outputs {
                if let IrExpr::Var(n, _) = t {
                    defs.insert(n.clone());
                } else {
                    walk_defs_reads_expr(t, defs, reads);
                }
            }
            for (_, e) in inputs {
                walk_defs_reads_expr(e, defs, reads);
            }
        }
        IrStmt::Ext(n) => {
            for c in crate::shir_nodes::ExtNode::children(&**n) {
                walk_defs_reads_stmt(c, defs, reads);
            }
        }
        _ => {}
    }
}

fn walk_defs_reads_expr(e: &IrExpr, defs: &mut HashSet<String>, reads: &mut HashSet<String>) {
    match e {
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            reads.insert(name.clone());
        }
        IrExpr::Index { var, key } => {
            reads.insert(var.clone());
            walk_defs_reads_expr(key, defs, reads);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_defs_reads_expr(lhs, defs, reads);
            walk_defs_reads_expr(rhs, defs, reads);
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    reads.insert(name.clone());
                }
            }
            "setVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    defs.insert(name.clone());
                    if let Some(v) = args.get(1) {
                        walk_defs_reads_expr(v, defs, reads);
                    }
                }
            }
            "setArray" | "setArrayAppend" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    defs.insert(name.clone());
                }
                for a in args.iter().skip(1) {
                    walk_defs_reads_expr(a, defs, reads);
                }
            }
            "define" | "fnCall" => {
                for a in args {
                    walk_defs_reads_expr(a, defs, reads);
                }
                if func == "define" {
                    if let Some(IrExpr::Arrow(body)) = args.get(1) {
                        walk_defs_reads_stmts(body, defs, reads);
                    }
                }
            }
            _ => {
                for a in args {
                    walk_defs_reads_expr(a, defs, reads);
                }
            }
        },
        IrExpr::MethodCall { obj, args, .. } => {
            walk_defs_reads_expr(obj, defs, reads);
            for a in args {
                walk_defs_reads_expr(a, defs, reads);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_defs_reads_expr(cond, defs, reads);
            walk_defs_reads_expr(then, defs, reads);
            walk_defs_reads_expr(else_, defs, reads);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_defs_reads_expr(expr, defs, reads);
            walk_defs_reads_expr(default, defs, reads);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    walk_defs_reads_expr(x, defs, reads);
                }
            }
        }
        IrExpr::Capture { expr, .. } => walk_defs_reads_expr(expr, defs, reads),
        IrExpr::Arrow(body) => walk_defs_reads_stmts(body, defs, reads),
        IrExpr::Array(items) => {
            for i in items {
                walk_defs_reads_expr(i, defs, reads);
            }
        }
        IrExpr::Arith(a) => walk_defs_reads_arith(a, defs, reads),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            walk_defs_reads_expr(iter, defs, reads);
            walk_defs_reads_expr(elem, defs, reads);
            if let Some(c) = cond {
                walk_defs_reads_expr(c, defs, reads);
            }
        }
        IrExpr::Lambda { params, body } => {
            for p in params {
                defs.insert(p.clone());
            }
            walk_defs_reads_stmts(body, defs, reads);
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                walk_defs_reads_expr(v, defs, reads);
            }
        }
        IrExpr::Splice(x) => walk_defs_reads_expr(x, defs, reads),
        IrExpr::Ext(n) => {
            for c in n.children() {
                walk_defs_reads_expr(c, defs, reads);
            }
        }
        _ => {}
    }
}

fn walk_defs_reads_arith(a: &ArithAst, defs: &mut HashSet<String>, reads: &mut HashSet<String>) {
    use crate::ir::ArithAst::*;
    match a {
        Var(name) | Ident(name) => {
            reads.insert(name.clone());
        }
        Index { var, key } => {
            reads.insert(var.clone());
            walk_defs_reads_arith(key, defs, reads);
        }
        Bin { lhs, rhs, .. } => {
            walk_defs_reads_arith(lhs, defs, reads);
            walk_defs_reads_arith(rhs, defs, reads);
        }
        Un { arg, .. } => walk_defs_reads_arith(arg, defs, reads),
        Cond { test, then, else_, .. } => {
            walk_defs_reads_arith(test, defs, reads);
            walk_defs_reads_arith(then, defs, reads);
            walk_defs_reads_arith(else_, defs, reads);
        }
        Assign { var, rhs, .. } => {
            walk_defs_reads_arith(rhs, defs, reads);
            defs.insert(var.clone());
        }
        IncDec { var, .. } => {
            reads.insert(var.clone());
            defs.insert(var.clone());
        }
        Num(_) | Sizeof(_) => {}
        Cast { arg, .. } => walk_defs_reads_arith(arg, defs, reads),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn empty_prog() -> IrProgram {
        IrProgram {
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn read(var: &str) -> IrExpr {
        IrExpr::Var(var.to_string(), None)
    }

    #[test]
    fn empty_program_no_findings() {
        assert!(analyze_use_before_decl(&empty_prog()).is_empty());
    }

    #[test]
    fn straight_line_use_before_decl() {
        // echo $x; x=1  →  x read at pos 1 before its def at pos 2.
        let prog = IrProgram {
            stmts: vec![
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
                assign("x", IrExpr::Int(1)),
            ],
            ..empty_prog()
        };
        let f = analyze_use_before_decl(&prog);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].var, "x");
        assert_eq!(f[0].stmt_pos, 1);
    }

    #[test]
    fn defined_before_use_is_clean() {
        // x=1; echo $x  →  no finding.
        let prog = IrProgram {
            stmts: vec![
                assign("x", IrExpr::Int(1)),
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        assert!(analyze_use_before_decl(&prog).is_empty());
    }

    #[test]
    fn branch_meet_suppresses_when_one_side_undefs() {
        // if true; then x=1; fi; echo $x  →  x defined on only one path,
        // so NOT must-defined after the if → finding at the echo.
        let prog = IrProgram {
            stmts: vec![
                IrStmt::If {
                    cond: IrExpr::Bool(true),
                    then: vec![assign("x", IrExpr::Int(1))],
                    elsifs: vec![],
                    else_: vec![],
                },
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        let f = analyze_use_before_decl(&prog);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].var, "x");
        assert_eq!(f[0].stmt_pos, 2);
    }

    #[test]
    fn branch_both_sides_define_is_clean() {
        // if true; then x=1; else x=2; fi; echo $x  →  x is must-defined
        // on every path → no finding.
        let prog = IrProgram {
            stmts: vec![
                IrStmt::If {
                    cond: IrExpr::Bool(true),
                    then: vec![assign("x", IrExpr::Int(1))],
                    elsifs: vec![],
                    else_: vec![assign("x", IrExpr::Int(2))],
                },
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        assert!(analyze_use_before_decl(&prog).is_empty());
    }

    #[test]
    fn loop_body_def_does_not_propagate() {
        // while true; do x=1; done; echo $x  →  loop may run 0 times →
        // x not must-defined after → finding at the echo.
        let prog = IrProgram {
            stmts: vec![
                IrStmt::While {
                    cond: IrExpr::Bool(true),
                    body: vec![assign("x", IrExpr::Int(1))],
                },
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        let f = analyze_use_before_decl(&prog);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].var, "x");
        assert_eq!(f[0].stmt_pos, 2);
    }

    #[test]
    fn function_capture_of_top_level_is_clean() {
        // x=5; f(){ echo $x }  →  x is a program global, so the read
        // inside the function body must not warn.
        let prog = IrProgram {
            stmts: vec![
                assign("x", IrExpr::Int(5)),
                IrStmt::Function {
                    name: "f".to_string(),
                    body: vec![IrStmt::Output {
                        value: read("x"),
                        newline: true,
                        target: None,
                    }],
                    named_blocks: vec![],
                },
            ],
            ..empty_prog()
        };
        assert!(analyze_use_before_decl(&prog).is_empty());
    }

    #[test]
    fn function_body_read_of_never_assigned_warns() {
        // f(){ echo $typo }  →  $typo is never assigned anywhere → finding.
        let prog = IrProgram {
            stmts: vec![IrStmt::Function {
                name: "f".to_string(),
                body: vec![IrStmt::Output {
                    value: IrExpr::Var("typo".to_string(), None),
                    newline: true,
                    target: None,
                }],
                named_blocks: vec![],
            }],
            ..empty_prog()
        };
        let f = analyze_use_before_decl(&prog);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].var, "typo");
    }

    #[test]
    fn arith_read_before_assign() {
        // echo $((y + 1))   (y never assigned)  →  finding at pos 1.
        use crate::ir::ArithAst;
        let prog = IrProgram {
            stmts: vec![IrStmt::Output {
                value: IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(ArithAst::Var("y".to_string())),
                    rhs: Box::new(ArithAst::Num(1)),
                })),
                newline: true,
                target: None,
            }],
            ..empty_prog()
        };
        let f = analyze_use_before_decl(&prog);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].var, "y");
        assert_eq!(f[0].stmt_pos, 1);
    }

    #[test]
    fn deterministic_sorted_output() {
        // two undeclared reads → sorted by var name.
        let prog = IrProgram {
            stmts: vec![
                IrStmt::Output {
                    value: read("zeta"),
                    newline: true,
                    target: None,
                },
                IrStmt::Output {
                    value: read("alpha"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        let v1 = analyze_use_before_decl(&prog);
        let v2 = analyze_use_before_decl(&prog);
        assert_eq!(v1, v2);
        let names: Vec<&str> = v1.iter().map(|f| f.var.as_str()).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[test]
    fn getvar_treated_as_read() {
        // getVar("x") before any setVar("x") → finding.
        let prog = IrProgram {
            stmts: vec![IrStmt::Expr(IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("x".to_string(), StrStyle::SingleQuoted)],
            })],
            ..empty_prog()
        };
        let f = analyze_use_before_decl(&prog);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].var, "x");
    }
}
