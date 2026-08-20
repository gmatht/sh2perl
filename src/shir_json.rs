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
    // A1 `Ident` arith reads (core request zsh-sh-go-20260813-155123):
    // in-body arith reads of a NUMERIC-LIFTED `for` loop variable are
    // exported as `{"type":"Ident","name":…}` — the estree renderer
    // derives a bare `Identifier` from a lifted `Var` read, so the A1
    // carries the node the backends actually render. Gate: the SAME
    // lift verdicts the estree renderer computes (numeric_lift_vars +
    // string_lift_vars + analyze_loop_var_refs), so every backend's
    // output is unchanged (Ident renders like a lifted Var read
    // everywhere; the estree arm emits the identifier directly).
    let numeric = crate::shir::numeric_lift_vars(&prog);
    let string = crate::shir::string_lift_vars(&prog, &numeric);
    let (num, str) = crate::shir::analyze_loop_var_refs(&prog, &numeric, &string);
    let lifted: std::collections::HashSet<String> =
        num.union(&str).cloned().collect();
    rewrite_loop_var_idents(&mut prog.stmts, &lifted);
    // shIR markup: mark loops provably run at least once (`"runs": true`)
    // so every backend consuming the A1 contract knows the body always
    // runs (the estree backend uses it to skip its ran/last tracking).
    crate::shir::set_provably_running_loops(&prog.stmts);
    // shir-builtin-op-20260816: the A1 CONTRACT carries the native
    // `builtin` op — every exec of a builtins.json command is rewritten
    // AT EXPORT (after the analyses above, which stay exec-shaped), so
    // the shared A1 check ("no exec of a builtins.json command in the
    // A1") holds and the backends see the native verdict. The renderers
    // erase/accept at their entries (self-fallback); the internal IR and
    // the analyses are untouched.
    crate::transforms::builtin::transform(&mut prog.stmts);
    for sub in prog.subs.iter_mut() {
        crate::transforms::builtin::transform(&mut sub.body);
    }
    program_json(&prog, CONTRACT_VERSION).to_string()
}

/// "Raw" export for the frontend/optimizer boundary (plan §2.3):
/// no `analyze_var_types` (no A2 annotations); the caller builds the IR
/// with `ast_to_ir_raw` to also skip `optimize_stmts`. Same node
/// serialization (purity is a per-node property, not a post-attach).
/// Use to pin `F(S)_raw == C(S)_raw` and `O(F(S)) == C(S)`.
pub fn shir_to_shir_json_raw(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // shir-builtin-op-20260816: the raw export carries the same native
    // `builtin` op (the frontend/optimizer boundary contract — the
    // shared check gates the raw A1).
    crate::transforms::builtin::transform(&mut prog.stmts);
    for sub in prog.subs.iter_mut() {
        crate::transforms::builtin::transform(&mut sub.body);
    }
    program_json(&prog, CONTRACT_VERSION).to_string()
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
        IrStmt::Ext(n) => n.to_json(),
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
        IrStmt::Assign {
            targets,
            expr,
            asm,
        } => {
            let mut o = serde_json::Map::new();
            o.insert("type".to_string(), json!("Assign"));
            o.insert(
                "targets".to_string(),
                json!(targets.iter().map(target_json).collect::<Vec<_>>()),
            );
            o.insert("expr".to_string(), expr_json(expr));
            // Optional GCC asm-label spec on a DECLARATION-position assign
            // (`int x asm("myx") = 7;` — core request
            // c-sh-go-toplevelasmargument-20260814-042952). Absent when
            // None, so the A1 bytes of plain assigns are unchanged.
            if let Some(spec) = asm {
                o.insert("asm".to_string(), asm_spec_json(spec));
            }
            serde_json::Value::Object(o)
        }
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
        // try/except/else/finally (core request py-sh-go 20260813): the
        // guarded suite + except clauses (match: expr|null, as:
        // string|null, body), else/finally as plain statement lists
        // (empty arrays when absent).
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => json!({
            "type": "Try",
            "body": stmts_json(body),
            "excepts": excepts.iter().map(|e| json!({
                "type": "TryExcept",
                "match": e.match_expr.as_ref().map(expr_json),
                "as": e.as_name,
                "body": stmts_json(&e.body),
            })).collect::<Vec<_>>(),
            "else": stmts_json(else_body),
            "finally": stmts_json(finally_body),
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
        IrStmt::Function {
            name,
            body,
            named_blocks,
        } => {
            // `named_blocks` is emitted ONLY when non-empty: bash
            // functions never have them, so every existing emit (and
            // the frontends' byte-identical oracles) stays byte-identical.
            let mut m = serde_json::Map::new();
            m.insert("type".to_string(), json!("Function"));
            m.insert("name".to_string(), json!(name));
            m.insert("body".to_string(), json!(stmts_json(body)));
            if !named_blocks.is_empty() {
                let blocks: serde_json::Map<String, serde_json::Value> = named_blocks
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(stmts_json(v))))
                    .collect();
                m.insert("named_blocks".to_string(), serde_json::Value::Object(blocks));
            }
            serde_json::Value::Object(m)
        }
        IrStmt::Subshell(body) => json!({ "type": "Subshell", "body": stmts_json(body) }),
        IrStmt::Background(body) => json!({ "type": "Background", "body": stmts_json(body) }),
        IrStmt::Block(body) => json!({ "type": "Block", "body": stmts_json(body) }),
        // Go-style select over channel comm clauses (core requests
        // go-sh-commclause / go-sh-recvstmt). Each clause serializes its
        // comm kind plus the optional channel expr / recv target var /
        // send value expr (None fields serialize as null).
        IrStmt::Select { clauses } => json!({
            "type": "Select",
            "clauses": clauses.iter().map(|c| json!({
                "comm": c.comm,
                "target": c.target,
                "ch": c.ch.as_ref().map(expr_json),
                "value": c.value.as_ref().map(expr_json),
                "body": stmts_json(&c.body),
            })).collect::<Vec<_>>(),
        }),
        // Inline assembly (core requests c-sh-go-asm / asmargument /
        // asmqualifier): the raw template + operand bindings + clobbers.
        // outputs/inputs serialize the constraint string + the lowered
        // operand expr (the existing value-node serialization). The
        // deserializer also accepts a plain store-name STRING in place of
        // the value node (the request's minimal shape).
        IrStmt::Asm { template, volatile, outputs, inputs, clobbers } =>
            asm_spec_json(&AsmSpec {
                template: template.clone(),
                volatile: *volatile,
                outputs: outputs.clone(),
                inputs: inputs.clone(),
                clobbers: clobbers.clone(),
            }),
        IrStmt::Expr(e) => json!({ "type": "Expr", "expr": expr_json(e) }),
        IrStmt::Label(name) => json!({ "type": "Label", "name": name }),
        IrStmt::Goto(name) => json!({ "type": "Goto", "name": name }),
    }
}

/// Inline-assembly / asm-label spec JSON — the shared shape of the
/// `Asm` statement and the declarator-position `Assign.asm` field
/// (core request c-sh-go-toplevelasmargument-20260814-042952):
/// `{"template","volatile","outputs","inputs","clobbers"}` with the
/// operand value-node serialization.
fn asm_spec_json(spec: &AsmSpec) -> Value {
    json!({
        "type": "Asm",
        "template": spec.template,
        "volatile": spec.volatile,
        "outputs": spec.outputs.iter().map(|(c, t)| json!({
            "constraint": c, "target": expr_json(t),
        })).collect::<Vec<_>>(),
        "inputs": spec.inputs.iter().map(|(c, e)| json!({
            "constraint": c, "expr": expr_json(e),
        })).collect::<Vec<_>>(),
        "clobbers": spec.clobbers,
    })
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
        // Comprehension expr (core request py-sh-go-comp-if): var/iter/
        // elem + the optional comp_if filter (`cond`, null = no filter).
        IrExpr::ArrayComp { var, iter, elem, cond } => json!({
            "type": "ArrayComp", "var": var, "iter": expr_json(iter),
            "elem": expr_json(elem),
            "cond": cond.as_ref().map(|c| expr_json(c)),
        }),
        // Parameterized function-literal expr (core request
        // py-sh-go-lambdef): the sibling of `Arrow` with explicit params.
        IrExpr::Lambda { params, body } => json!({
            "type": "Lambda", "params": params, "body": stmts_json(body),
        }),
        IrExpr::Array(items) => json!({
            "type": "Array", "elements": items.iter().map(expr_json).collect::<Vec<_>>(),
        }),
        IrExpr::Arith(a) => json!({ "type": "Arith", "ast": arith_json(a) }),
        IrExpr::Bool(b) => json!({ "type": "Bool", "value": b }),
        IrExpr::Json(v) => json!({ "type": "Json", "value": v }),
        IrExpr::Ident(name) => json!({ "type": "Ident", "name": name }),
        // Starred-expression splice (core request py-sh-go-star-expr):
        // `[*a]` / `f(*a)` — the wrapped expr's elements splice into the
        // enclosing Array/Call (the estree renderer emits a JS spread).
        IrExpr::Splice(e) => json!({ "type": "Splice", "expr": expr_json(e) }),
        IrExpr::Ext(n) => n.to_json(),
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
        ArithAst::Ident(name) => json!({ "type": "Ident", "name": name }),
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
        StrStyle::Raw => "Raw",
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
        | "builtin" | "bcSqrt" | "ternary" | "arrayStore" | "memAdvance" | "memTest" | "line"
        // associative-array helpers (core request py-sh-go-20260806-144303-b):
        // assocSet/assocGet/assocNames/assocValues — runtime store ops on
        // the assoc Map, CPU-only, no process spawn (mirror setArray/
        // arrayItems).
        | "assocSet" | "assocSet2" | "assocGet" | "assocNames" | "assocValues"
        // channel/select vocabulary (core requests go-sh-commclause /
        // go-sh-recvstmt): FIFO channels + blocking recv/send + the
        // round-robin select poll — all implementable in the runtime
        // (arrays), no process spawn.
        | "makeChan" | "recv" | "send" | "select" => "Emulable",
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

// ── A1 `Ident` arith reads (core request zsh-sh-go-20260813-155123) ──

/// Rewrite `ArithAst::Var(var)` → `ArithAst::Ident(var)` inside the
/// body of every `IrStmt::For` whose loop variable is numeric-lifted.
/// The rewrite is export-only (the renderers consume the pre-rewrite
/// IR from `ast_to_ir`; the ingested A1 carries the nodes itself).
fn rewrite_loop_var_idents(stmts: &mut [IrStmt], lifted: &std::collections::HashSet<String>) {
    for s in stmts.iter_mut() {
        if let IrStmt::For { var, body, .. } = s {
            if lifted.contains(var) {
                let v = var.clone();
                for b in body.iter_mut() {
                    rewrite_stmt_arith_ident(b, &v);
                }
                continue;
            }
        }
        // recurse into statement containers (a nested For inside an
        // If/While/Block/Case/... body gets its own rewrite)
        match s {
            IrStmt::Block(b)
            | IrStmt::Subshell(b)
            | IrStmt::Background(b) => rewrite_loop_var_idents(b, lifted),
            IrStmt::If { then, elsifs, else_, .. } => {
                rewrite_loop_var_idents(then, lifted);
                for (_, b) in elsifs.iter_mut() {
                    rewrite_loop_var_idents(b, lifted);
                }
                rewrite_loop_var_idents(else_, lifted);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                rewrite_loop_var_idents(body, lifted);
            }
            IrStmt::ForInit { init, step, body, .. } => {
                rewrite_loop_var_idents(init, lifted);
                rewrite_loop_var_idents(step, lifted);
                rewrite_loop_var_idents(body, lifted);
            }
            IrStmt::Function { body, .. } => rewrite_loop_var_idents(body, lifted),
            IrStmt::Redirect { inner, .. } => rewrite_loop_var_idents(inner, lifted),
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                rewrite_loop_var_idents(body, lifted);
                for e in excepts.iter_mut() {
                    rewrite_loop_var_idents(&mut e.body, lifted);
                }
                rewrite_loop_var_idents(else_body, lifted);
                rewrite_loop_var_idents(finally_body, lifted);
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses.iter_mut() {
                    rewrite_loop_var_idents(&mut c.body, lifted);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages.iter_mut() {
                    rewrite_loop_var_idents(stage, lifted);
                }
            }
            IrStmt::Expr(e) | IrStmt::Assign { expr: e, .. } => {
                rewrite_expr_arith_ident(e, &None)
            }
            _ => {}
        }
    }
}

/// Rewrite `ArithAst::Var(var)` → `ArithAst::Ident(var)` throughout a
/// statement (recursing into expression arrows).
fn rewrite_stmt_arith_ident(stmt: &mut IrStmt, var: &str) {
    match stmt {
        IrStmt::Expr(e) => rewrite_expr_arith_ident(e, &Some(var.to_string())),
        IrStmt::Assign { expr, .. } => rewrite_expr_arith_ident(expr, &Some(var.to_string())),
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                rewrite_expr_arith_ident(i, &Some(var.to_string()));
            }
        }
        IrStmt::Output { value, .. } => rewrite_expr_arith_ident(value, &Some(var.to_string())),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            rewrite_expr_arith_ident(cond, &Some(var.to_string()));
            for s in then.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
            for (_, b) in elsifs.iter_mut() {
                for s in b.iter_mut() {
                    rewrite_stmt_arith_ident(s, var);
                }
            }
            for s in else_.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
            rewrite_expr_arith_ident(cond, &Some(var.to_string()));
            for s in body.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
        }
        IrStmt::For { iter, body, .. } => {
            rewrite_expr_arith_ident(iter, &Some(var.to_string()));
            for s in body.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
        }
        IrStmt::ForInit { init, cond, step, body } => {
            for s in init.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
            rewrite_expr_arith_ident(cond, &Some(var.to_string()));
            for s in step.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
            for s in body.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => {
            for s in b.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            for s in inner.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
            for r in redirects.iter_mut() {
                rewrite_expr_arith_ident(&mut r.target, &Some(var.to_string()));
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            for s in body.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
            for e in excepts.iter_mut() {
                for s in e.body.iter_mut() {
                    rewrite_stmt_arith_ident(s, var);
                }
            }
            for s in else_body.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
            for s in finally_body.iter_mut() {
                rewrite_stmt_arith_ident(s, var);
            }
        }
        IrStmt::Case { discriminant, clauses } => {
            rewrite_expr_arith_ident(discriminant, &Some(var.to_string()));
            for c in clauses.iter_mut() {
                for s in c.body.iter_mut() {
                    rewrite_stmt_arith_ident(s, var);
                }
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages.iter_mut() {
                for s in stage.iter_mut() {
                    rewrite_stmt_arith_ident(s, var);
                }
            }
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
            rewrite_expr_arith_ident(e, &Some(var.to_string()));
        }
        IrStmt::Exec { cmd, args, env, .. } => {
            rewrite_expr_arith_ident(cmd, &Some(var.to_string()));
            for a in args.iter_mut() {
                rewrite_expr_arith_ident(a, &Some(var.to_string()));
            }
            for (_, v) in env.iter_mut() {
                rewrite_expr_arith_ident(v, &Some(var.to_string()));
            }
        }
        _ => {}
    }
}

/// Rewrite `ArithAst::Var(var)` → `ArithAst::Ident(var)` throughout an
/// expression. `var` is None in the generic driver (no rewrite — only
/// the structural walk for nested `IrStmt::For` handling is needed).
fn rewrite_expr_arith_ident(e: &mut IrExpr, var: &Option<String>) {
    match e {
        IrExpr::Arith(a) => {
            if let Some(v) = var {
                rewrite_arith_ident(a, v);
            }
        }
        IrExpr::Arrow(stmts) => {
            if let Some(v) = var {
                for s in stmts.iter_mut() {
                    rewrite_stmt_arith_ident(s, v);
                }
            }
        }
        IrExpr::Call { args, .. } | IrExpr::MethodCall { args, .. } => {
            for a in args.iter_mut() {
                rewrite_expr_arith_ident(a, var);
            }
        }
        IrExpr::Array(items) => {
            for a in items.iter_mut() {
                rewrite_expr_arith_ident(a, var);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props.iter_mut() {
                rewrite_expr_arith_ident(v, var);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            rewrite_expr_arith_ident(lhs, var);
            rewrite_expr_arith_ident(rhs, var);
        }
        IrExpr::Ternary {
            cond, then, else_, ..
        } => {
            rewrite_expr_arith_ident(cond, var);
            rewrite_expr_arith_ident(then, var);
            rewrite_expr_arith_ident(else_, var);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            rewrite_expr_arith_ident(expr, var);
            rewrite_expr_arith_ident(default, var);
        }
        IrExpr::Index { key, .. } => rewrite_expr_arith_ident(key, var),
        IrExpr::Capture { expr, .. } => rewrite_expr_arith_ident(expr, var),
        _ => {}
    }
}

/// Rewrite `ArithAst::Var(var)` → `ArithAst::Ident(var)` in an arith
/// tree (reads only — Assign/IncDec TARGETS keep their var name; the
/// node's `name` field is the read).
fn rewrite_arith_ident(a: &mut ArithAst, var: &str) {
    match a {
        ArithAst::Var(n) if n == var => *a = ArithAst::Ident(n.clone()),
        ArithAst::Index { key, .. } => rewrite_arith_ident(key, var),
        ArithAst::Bin { lhs, rhs, .. } => {
            rewrite_arith_ident(lhs, var);
            rewrite_arith_ident(rhs, var);
        }
        ArithAst::Un { arg, .. } => rewrite_arith_ident(arg, var),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            rewrite_arith_ident(test, var);
            rewrite_arith_ident(then, var);
            rewrite_arith_ident(else_, var);
        }
        ArithAst::Assign { rhs, .. } => rewrite_arith_ident(rhs, var),
        ArithAst::Cast { arg, .. } => rewrite_arith_ident(arg, var),
        _ => {}
    }
}
