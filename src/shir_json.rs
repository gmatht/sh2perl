//! ShIR JSON export (ask A1) — language-neutral serialized IR.
//!
//! The cross-backend contract for non-Rust consumers (C, python, zig, go),
//! mirroring the ESTree-JSON decision (PLAN.md §1.2): core lowers once
//! (`ast_to_ir`), every backend renders. Serializes `IrProgram` with two
//! metadata layers:
//!   - ask A2: `var_types` — conservative type verdicts (numeric/string lift
//!     analyses serialized; `Any` = runtime store, omitted from the list).
//!   - ask A3: `purity` on Exec/Pipeline — `PureCpu` | `Emulable` | `Spawn`
//!     (builtin vs external classification, conservative).
//!   - const-markup: `var_const` — conservative const/var verdicts per
//!     assigned variable (`Const` | `Var`; the C backend emits `const`).
//!   - lifetime: `var_lifetimes` — per-variable live spans (first/last
//!     access positions) + the escape bit (the C backend's per-point
//!     buffer sizing and copy-vs-move input).
//! Deterministic: same input → byte-identical JSON.
//!
//! Usage: `debashc file --shir foo.sh` (or `debashc --shir <input>`).
//! See docs/backend-c-core-needs.md §8 (A1) and docs/estree-contract.md.

use crate::ir::*;
use serde_json::{json, Value};

/// Serialize an `IrProgram` to compact ShIR JSON (deterministic).
pub fn shir_to_shir_json(prog: &IrProgram) -> String {
    // Ask A2: attach the type verdicts before serializing — unless the
    // caller (the C frontend) already populated them from its own type
    // analysis (the shell lift's verdicts don't apply to a C-produced
    // IR).
    let mut prog = prog.clone();
    if prog.var_types.is_empty() {
        prog.var_types = crate::shir::analyze_var_types(&prog);
    }
    if prog.var_lengths.is_empty() {
        prog.var_lengths = crate::shir::analyze_string_lengths(&prog);
    }
    if prog.var_const.is_empty() {
        prog.var_const = crate::shir::analyze_var_const(&prog);
    }
    if prog.var_lifetimes.is_empty() {
        prog.var_lifetimes = crate::shir_passes::lifetime::analyze_var_lifetimes(&prog);
    }
    if prog.var_nospace.is_empty() {
        prog.var_nospace = crate::shir::analyze_var_nospace(&prog);
    }
    if prog.var_bash_env.is_empty() {
        prog.var_bash_env = crate::shir::analyze_var_bash_env(&prog);
    }
    // shIR markup: mark loops provably run at least once (`"runs": true`)
    // so every backend consuming the A1 contract knows the body always
    // runs (the estree backend uses it to skip its ran/last tracking).
    crate::shir::set_provably_running_loops(&prog.stmts);
    program_json(&prog, CONTRACT_VERSION).to_string()
}

/// "Raw" export for the frontend/optimizer boundary (plan §2.3):
/// no `analyze_var_types` (no A2 annotations); the caller builds the IR
/// with `ast_to_ir_raw` to also skip `optimize_stmts`. Same node
/// serialization (purity is a per-node property, not a post-attach).
/// Use to pin `F(S)_raw == C(S)_raw` and `O(F(S)) == C(S)`.
pub fn shir_to_shir_json_raw(prog: &IrProgram) -> String {
    program_json(prog, CONTRACT_VERSION).to_string()
}

/// Current contract version (plan §2.1). Bump on any breaking shape change.
pub const CONTRACT_VERSION: u32 = 1;

// ── Program / subs ───────────────────────────────────────────────────

fn program_json(p: &IrProgram, contract_version: u32) -> Value {
    json!({
        "type": "Program",
        "contract_version": contract_version,
        "imports": p.imports,
        "requires": p.requires,
        "var_types": p.var_types.iter().map(|(n, t)| json!({"name": n, "type": t})).collect::<Vec<_>>(),
        "stmt_lines": p.stmt_lines.iter().map(|(i, l)| json!({"stmt": i, "line": l})).collect::<Vec<_>>(),
        "var_lengths": p.var_lengths.iter().map(|(n, l)| json!({"name": n, "max_len": l})).collect::<Vec<_>>(),
        "var_const": p.var_const.iter().map(|(n, k)| json!({"name": n, "kind": k})).collect::<Vec<_>>(),
        "var_lifetimes": p.var_lifetimes.iter().map(|(n, l)| json!({"name": n, "first": l.first, "last": l.last, "escapes": l.escapes})).collect::<Vec<_>>(),
        "var_nospace": p.var_nospace.iter().map(|(n, b)| json!({"name": n, "nospace": b})).collect::<Vec<_>>(),
        "var_bash_env": p.var_bash_env,
        "subs": p.subs.iter().map(sub_json).collect::<Vec<_>>(),
        "stmts": p.stmts.iter().map(stmt_json).collect::<Vec<_>>(),
    })
}

fn sub_json(s: &IrSub) -> Value {
    json!({
        "type": "Sub",
        "name": s.name,
        "params": s.params,
        "body": s.body.iter().map(stmt_json).collect::<Vec<_>>(),
    })
}

// ── Statements ───────────────────────────────────────────────────────

fn stmt_json(s: &IrStmt) -> Value {
    match s {
        IrStmt::Output {
            value,
            newline,
            target,
        } => json!({
            "type": "Output", "value": expr_json(value),
            "newline": newline, "target": target,
        }),
        IrStmt::WriteFile {
            path,
            content,
            append,
        } => json!({
            "type": "WriteFile", "path": expr_json(path),
            "content": expr_json(content), "append": append,
        }),
        IrStmt::Assign { targets, expr } => json!({
            "type": "Assign",
            "targets": targets.iter().map(target_json).collect::<Vec<_>>(),
            "expr": expr_json(expr),
        }),
        IrStmt::Declare { vars, init, local } => json!({
            "type": "Declare",
            "vars": vars.iter().map(decl_json).collect::<Vec<_>>(),
            "init": init.as_ref().map(|e| expr_json(e)),
            "local": local,
        }),
        IrStmt::DeclareArray {
            var,
            sigil,
            elements,
        } => json!({
            "type": "DeclareArray", "var": var,
            "sigil": sigil_json(*sigil),
            "elements": elements.iter().map(expr_json).collect::<Vec<_>>(),
        }),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => json!({
            "type": "If", "cond": expr_json(cond),
            "then": stmts_json(then),
            "elsifs": elsifs.iter().map(|(c, b)| json!({"cond": expr_json(c), "body": stmts_json(b)})).collect::<Vec<_>>(),
            "else": stmts_json(else_),
        }),
        IrStmt::For { var, iter, body } => json!({
            "type": "For", "var": var, "iter": expr_json(iter),
            "body": stmts_json(body),
            "runs": crate::shir::stmt_provably_runs(s),
        }),
        IrStmt::ForInit { init, cond, step, body } => json!({
            "type": "ForInit", "init": stmts_json(init), "cond": expr_json(cond),
            "step": stmts_json(step), "body": stmts_json(body),
            "runs": crate::shir::stmt_provably_runs(s),
        }),
        IrStmt::Continue => json!({ "type": "Continue", "runs": crate::shir::stmt_provably_runs(s) }),
        IrStmt::Break => json!({ "type": "Break", "runs": crate::shir::stmt_provably_runs(s) }),
        IrStmt::While { cond, body } => json!({
            "type": "While", "cond": expr_json(cond), "body": stmts_json(body),
            "runs": crate::shir::stmt_provably_runs(s),
        }),
        IrStmt::DoWhile { body, cond, until } => json!({
            "type": "DoWhile", "body": stmts_json(body),
            "cond": expr_json(cond), "until": until,
            "runs": crate::shir::stmt_provably_runs(s),
        }),
        IrStmt::Die { expr, carp } => json!({
            "type": "Die", "expr": expr_json(expr), "carp": carp,
        }),
        IrStmt::Warn { expr, carp } => json!({
            "type": "Warn", "expr": expr_json(expr), "carp": carp,
        }),
        IrStmt::Exec {
            cmd,
            args,
            capture,
            redirects,
            env,
        } => {
            let mut o = serde_json::Map::new();
            o.insert("type".into(), "Exec".into());
            o.insert("cmd".into(), expr_json(cmd));
            o.insert(
                "args".into(),
                json!(args.iter().map(expr_json).collect::<Vec<_>>()),
            );
            o.insert("capture".into(), json!(capture));
            o.insert(
                "redirects".into(),
                json!(redirects.iter().map(expr_json).collect::<Vec<_>>()),
            );
            o.insert(
                "env".into(),
                json!(env
                    .iter()
                    .map(|(k, v)| json!({"name": k, "value": expr_json(v)}))
                    .collect::<Vec<_>>()),
            );
            // ask A3: purity classification (builtin vs external, conservative)
            o.insert("purity".into(), exec_purity(cmd, capture).into());
            Value::Object(o)
        }
        IrStmt::Pipeline {
            stages,
            last_output,
            capture,
            cmd_str,
        } => json!({
            "type": "Pipeline",
            "stages": stages.iter().map(|s| stmts_json(s)).collect::<Vec<_>>(),
            "last_output": last_output,
            "capture": capture,
            "cmd_str": cmd_str,
            "purity": "Spawn", // conservative: real pipes/processes; C can fork/exec
        }),
        IrStmt::Return(e) => json!({ "type": "Return", "value": e.as_ref().map(expr_json) }),
        IrStmt::Exit(e) => json!({ "type": "Exit", "value": e.as_ref().map(expr_json) }),
        IrStmt::SetChildError(e) => json!({ "type": "SetChildError", "expr": expr_json(e) }),
        IrStmt::Require(m) => json!({ "type": "Require", "module": m }),
        IrStmt::RawText(t) => json!({ "type": "RawText", "text": t }),
        IrStmt::Case {
            discriminant,
            clauses,
        } => json!({
            "type": "Case", "discriminant": expr_json(discriminant),
            "clauses": clauses.iter().map(|c| json!({
                "patterns": c.patterns, "body": stmts_json(&c.body),
            })).collect::<Vec<_>>(),
        }),
        IrStmt::Redirect { inner, redirects } => json!({
            "type": "Redirect", "inner": stmts_json(inner),
            "redirects": redirects.iter().map(redirect_json).collect::<Vec<_>>(),
        }),
        IrStmt::Function { name, body } => json!({
            "type": "Function", "name": name, "body": stmts_json(body),
        }),
        IrStmt::Subshell(body) => json!({ "type": "Subshell", "body": stmts_json(body) }),
        IrStmt::Background(body) => json!({ "type": "Background", "body": stmts_json(body) }),
        IrStmt::Block(body) => json!({ "type": "Block", "body": stmts_json(body) }),
        IrStmt::Expr(e) => json!({ "type": "Expr", "expr": expr_json(e) }),
        IrStmt::Label(name) => json!({ "type": "Label", "name": name }),
        IrStmt::Goto(name) => json!({ "type": "Goto", "name": name }),
    }
}

fn stmts_json(v: &[IrStmt]) -> Value {
    json!(v.iter().map(stmt_json).collect::<Vec<_>>())
}

// ── Expressions ──────────────────────────────────────────────────────

fn expr_json(e: &IrExpr) -> Value {
    match e {
        IrExpr::Int(i) => json!({ "type": "Int", "value": i }),
        IrExpr::Str(s, style) => json!({
            "type": "Str", "value": s, "style": style_json(style),
        }),
        IrExpr::Var(name, sigil) => json!({
            "type": "Var", "name": name, "sigil": sigil_json(*sigil),
        }),
        IrExpr::Index { var, key } => json!({
            "type": "Index", "var": var, "key": expr_json(key),
        }),
        IrExpr::BinOp { lhs, op, rhs } => json!({
            "type": "BinOp", "op": binop_json(op),
            "lhs": expr_json(lhs), "rhs": expr_json(rhs),
        }),
        IrExpr::Call { func, args } => {
            let mut o = serde_json::Map::new();
            o.insert("type".into(), "Call".into());
            o.insert("func".into(), func.clone().into());
            o.insert(
                "args".into(),
                json!(args.iter().map(expr_json).collect::<Vec<_>>()),
            );
            // ask A3: purity classification per the A4 namespace spec
            // (harness/sh2-namespace.json). `exec` refines by cmd name:
            // builtin → Emulable, external → Spawn.
            o.insert("purity".into(), call_purity(func, args).into());
            Value::Object(o)
        }
        IrExpr::MethodCall { obj, method, args } => json!({
            "type": "MethodCall", "object": expr_json(obj), "method": method,
            "args": args.iter().map(expr_json).collect::<Vec<_>>(),
        }),
        IrExpr::Ternary { cond, then, else_ } => json!({
            "type": "Ternary", "cond": expr_json(cond),
            "then": expr_json(then), "else": expr_json(else_),
        }),
        IrExpr::DefinedOr { expr, default } => json!({
            "type": "DefinedOr", "expr": expr_json(expr),
            "default": expr_json(default),
        }),
        IrExpr::Interpolate(parts) => json!({
            "type": "Interpolate",
            "parts": parts.iter().map(|p| match p {
                InterpPart::Lit(s) => json!({ "kind": "lit", "text": s }),
                InterpPart::Expr(e) => json!({ "kind": "expr", "expr": expr_json(e) }),
            }).collect::<Vec<_>>(),
        }),
        IrExpr::Capture { expr, native } => json!({
            "type": "Capture", "expr": expr_json(expr), "native": native,
        }),
        IrExpr::Regex { pattern, flags } => json!({
            "type": "Regex", "pattern": pattern, "flags": flags,
        }),
        IrExpr::Range { start, end } => json!({
            "type": "Range", "start": start, "end": end,
        }),
        IrExpr::RawExpr(t) => json!({ "type": "RawExpr", "text": t }),
        IrExpr::Arrow(body) => json!({ "type": "Arrow", "body": stmts_json(body) }),
        IrExpr::Array(items) => json!({
            "type": "Array", "elements": items.iter().map(expr_json).collect::<Vec<_>>(),
        }),
        IrExpr::Arith(a) => json!({ "type": "Arith", "ast": arith_json(a) }),
        IrExpr::Bool(b) => json!({ "type": "Bool", "value": b }),
        IrExpr::Json(v) => json!({ "type": "Json", "value": v }),
        IrExpr::Ident(name) => json!({ "type": "Ident", "name": name }),
        IrExpr::Object(props) => json!({
            "type": "Object",
            "properties": props.iter().map(|(k, v)| json!({"key": k, "value": expr_json(v)})).collect::<Vec<_>>(),
        }),
    }
}

fn arith_json(a: &ArithAst) -> Value {
    match a {
        ArithAst::Num(n) => json!({ "type": "Num", "value": n }),
        ArithAst::Var(name) => json!({ "type": "Var", "name": name }),
        ArithAst::Index { var, key } => json!({
            "type": "Index", "var": var, "key": arith_json(key),
        }),
        ArithAst::Bin { op, lhs, rhs } => json!({
            "type": "Bin", "op": op, "lhs": arith_json(lhs), "rhs": arith_json(rhs),
        }),
        ArithAst::Un { op, arg } => json!({
            "type": "Un", "op": op, "arg": arith_json(arg),
        }),
        ArithAst::Cond { test, then, else_ } => json!({
            "type": "Cond", "test": arith_json(test),
            "then": arith_json(then), "else": arith_json(else_),
        }),
        ArithAst::Assign { var, op, rhs } => json!({
            "type": "Assign", "var": var, "op": op, "rhs": arith_json(rhs),
        }),
        ArithAst::IncDec { var, delta, prefix } => json!({
            "type": "IncDec", "var": var, "delta": delta, "prefix": prefix,
        }),
        ArithAst::Sizeof(ty) => json!({ "type": "Sizeof", "ty": ty }),
        ArithAst::Cast { ty, arg } => json!({
            "type": "Cast", "ty": ty, "arg": arith_json(arg),
        }),
    }
}

// ── Leaf helpers ─────────────────────────────────────────────────────

fn target_json(t: &AssignTarget) -> Value {
    json!({
        "var": t.var, "sigil": sigil_json(t.sigil),
        "indices": t.indices.iter().map(expr_json).collect::<Vec<_>>(),
    })
}

fn decl_json(d: &Decl) -> Value {
    json!({ "name": d.name, "sigil": sigil_json(d.sigil) })
}

fn redirect_json(r: &IrRedirect) -> Value {
    json!({
        "fd": r.fd, "mode": r.mode,
        "target": expr_json(&r.target), "interpolate": r.interpolate,
    })
}

fn sigil_json(s: Option<Sigil>) -> Value {
    match s {
        None => Value::Null,
        Some(Sigil::Scalar) => "Scalar".into(),
        Some(Sigil::Array) => "Array".into(),
        Some(Sigil::Hash) => "Hash".into(),
    }
}

fn style_json(s: &StrStyle) -> &'static str {
    match s {
        StrStyle::SingleQuoted => "SingleQuoted",
        StrStyle::DoubleQuoted => "DoubleQuoted",
        StrStyle::Command => "Command",
        StrStyle::Heredoc => "Heredoc",
    }
}

fn binop_json(op: &BinOpKind) -> &'static str {
    match op {
        BinOpKind::Add => "Add",
        BinOpKind::Sub => "Sub",
        BinOpKind::Mul => "Mul",
        BinOpKind::Div => "Div",
        BinOpKind::Mod => "Mod",
        BinOpKind::Pow => "Pow",
        BinOpKind::Concat => "Concat",
        BinOpKind::Eq => "Eq",
        BinOpKind::Ne => "Ne",
        BinOpKind::Lt => "Lt",
        BinOpKind::Gt => "Gt",
        BinOpKind::Le => "Le",
        BinOpKind::Ge => "Ge",
        BinOpKind::And => "And",
        BinOpKind::Or => "Or",
        BinOpKind::Not => "Not",
        BinOpKind::BitAnd => "BitAnd",
        BinOpKind::BitOr => "BitOr",
        BinOpKind::BitXor => "BitXor",
        BinOpKind::ShiftL => "ShiftL",
        BinOpKind::ShiftR => "ShiftR",
    }
}

/// Ask A3: purity class for an `sh2.*`-style call (the A4 namespace
/// mapping, harvest from harness/sh2-namespace.json). `exec` refines by
/// the first arg (the command name): known builtin → Emulable, else Spawn.
fn call_purity(func: &str, args: &[IrExpr]) -> &'static str {
    match func {
        // PureCpu (namespace spec): no I/O, no state beyond args
        "contains" | "join" | "brace" | "idiv" | "imod" | "arith" | "arithEval" | "trimCapture"
        | "dirname" | "basename" | "not" | "guard" | "caseMatch" | "split" | "param"
        | "callDirect" => "PureCpu",
        // Emulable: implementable in a backend runtime (state/string/glob/fs-tests)
        "getVar" | "setVar" | "setLastExit" | "assign" | "test" | "grepText" | "listVar"
        | "setArray" | "setArrayAppend" | "arrayItems" | "arrayKeys" | "arrayLen"
        | "arrayIndex" | "fnCall" | "fnValue" | "define" | "forLoop" | "whileLoop" | "block" | "shopt"
        | "builtin" | "bcSqrt" | "ternary" | "arrayStore" | "memAdvance" | "memTest" | "line" => "Emulable",
        // Fs: file I/O, no process spawn
        _ if func.starts_with("fs.") => "Fs",
        // Spawn: must fork/exec or connect processes
        "exec" => match args.first() {
            Some(IrExpr::Str(name, _)) | Some(IrExpr::Ident(name))
                if crate::shir::SYNC_BUILTINS.contains(&name.as_str()) =>
            {
                "Emulable"
            }
            _ => "Spawn",
        },
        "capture" | "captureWords" | "pipeline" | "redirect" | "subshell" | "background"
        | "callUndefined" | "unsupported" => "Spawn",
        // Control-flow signals
        "return" | "break" | "continue" | "exit" => "Control",
        _ => "Spawn", // unknown → conservative
    }
}

/// Ask A3: conservative purity classification for an IrStmt::Exec.
/// (The current ast_to_ir lowers commands to `Call("exec", ...)` — this
/// arm covers the future direct-Exec lowering.)
fn exec_purity(cmd: &IrExpr, capture: &Option<String>) -> &'static str {
    let name = match cmd {
        IrExpr::Str(s, _) => Some(s.as_str()),
        IrExpr::Ident(s) => Some(s.as_str()),
        _ => None,
    };
    match name {
        Some(n) if crate::shir::SYNC_BUILTINS.contains(&n) && capture.is_none() => "Emulable",
        _ => "Spawn",
    }
}
