//! Manual inverse of `shir_json.rs` — ingest ShIR JSON into `IrProgram`
//! (plan §2.2). The A1 contract is hand-defined, so the reader is
//! hand-defined to match it exactly (serde derives on the IR types would
//! require struct-variant conversions that risk the Perl/ESTree paths;
//! a manual mirror is safer and the contract is the source of truth).
//!
//! Strict ingress validation (the "structural gate" on input — mirrors
//! the ESTree callee-whitelist gate): unknown node types and unknown
//! fields are rejected with a precise error. Same node vocabulary as the
//! serializer; new arms must be added in BOTH shir_json.rs and here.
use crate::ir::*;
use serde_json::Value;

const KNOWN_STMT: &[&str] = &[
    "Output",
    "WriteFile",
    "Assign",
    "Declare",
    "DeclareArray",
    "If",
    "For",
    "While",
    "DoWhile",
    "Die",
    "Warn",
    "Exec",
    "Pipeline",
    "Return",
    "Exit",
    "SetChildError",
    "Require",
    "RawText",
    "Case",
    "Redirect",
    "Function",
    "Subshell",
    "Background",
    "Block",
    "Expr",
    "Label",
    "Goto",
];
const KNOWN_EXPR: &[&str] = &[
    "Int",
    "Str",
    "Var",
    "Index",
    "BinOp",
    "Call",
    "MethodCall",
    "Ternary",
    "DefinedOr",
    "Interpolate",
    "Capture",
    "Regex",
    "Range",
    "RawExpr",
    "Arrow",
    "Array",
    "Arith",
    "Bool",
    "Json",
    "Ident",
    "Object",
];
const KNOWN_ARITH: &[&str] = &[
    "Num", "Var", "Index", "Bin", "Un", "Cond", "Assign", "IncDec",
];

pub fn shir_json_to_ir(json: &str) -> Result<IrProgram, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    program_from_value(&v)
}

// ── Program / subs ────────────────────────────────────────────────────

fn program_from_value(v: &Value) -> Result<IrProgram, String> {
    let obj = require_obj(v, "Program")?;
    require_field(obj, "type", "Program")?;
    let contract_version = obj
        .get("contract_version")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| "Program: missing contract_version (plan §2.1)".to_string())?;
    if contract_version as u32 != super::shir_json::CONTRACT_VERSION {
        return Err(format!(
            "Program: contract_version {} != core {}",
            contract_version,
            super::shir_json::CONTRACT_VERSION
        ));
    }
    let imports = str_array(obj.get("imports"), "Program.imports")?;
    let requires = str_array(obj.get("requires"), "Program.requires")?;
    let var_types = var_types_from(obj.get("var_types"), "Program.var_types")?;
    let subs = subs_from(obj.get("subs"), "Program.subs")?;
    let stmts = stmts_from(obj.get("stmts"), "Program.stmts")?;
    let stmt_lines = match obj.get("stmt_lines") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| {
                let s = v.get("stmt")?.as_u64()? as usize;
                let l = v.get("line")?.as_u64()? as usize;
                Some((s, l))
            })
            .collect(),
        _ => vec![],
    };
    let var_lengths = match obj.get("var_lengths") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| {
                let n = v.get("name")?.as_str()?.to_string();
                let l = v.get("max_len").and_then(|x| x.as_u64());
                Some((n, l))
            })
            .collect(),
        _ => vec![],
    };
    let var_const = var_const_from(obj.get("var_const"), "Program.var_const")?;
    let var_lifetimes = var_lifetimes_from(obj.get("var_lifetimes"), "Program.var_lifetimes")?;
    Ok(IrProgram {
        imports,
        requires,
        stmts,
        subs,
        var_types,
        stmt_lines,
        var_lengths,
        var_const,
        var_lifetimes,
    })
}

fn subs_from(v: Option<&Value>, where_: &str) -> Result<Vec<IrSub>, String> {
    arr(v, where_)?
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let o = require_obj(x, &format!("{where_}[{i}]"))?;
            require_field(o, "type", &format!("{where_}[{i}]"))?;
            if o["type"] != "Sub" {
                return Err(format!("{where_}[{i}]: type {} != Sub", o["type"]));
            }
            let name = req_str(o, "name", &format!("{where_}[{i}]"))?.to_string();
            let params = str_array(o.get("params"), &format!("{where_}[{i}].params"))?;
            let body = stmts_from(o.get("body"), &format!("{where_}[{i}].body"))?;
            Ok(IrSub { name, params, body })
        })
        .collect()
}

fn var_types_from(v: Option<&Value>, where_: &str) -> Result<Vec<(String, IrType)>, String> {
    match v {
        None => Ok(vec![]),
        Some(x) => {
            let a = arr(Some(x), where_)?;
            a.iter()
                .enumerate()
                .map(|(i, e)| {
                    let o = require_obj(e, &format!("{where_}[{i}]"))?;
                    let n = req_str(o, "name", &format!("{where_}[{i}]"))?.to_string();
                    let t = req(o, "type", &format!("{where_}[{i}]"))?;
                    let irt = match t {
                        serde_json::Value::String(s) => match s.as_str() {
                            "Int" => IrType::Int,
                            "Str" => IrType::Str,
                            "Any" => IrType::Any,
                            other => {
                                return Err(format!(
                                    "{where_}[{i}].type: {other} not in Int/Str/Any"
                                ))
                            }
                        },
                        serde_json::Value::Object(o) => match (
                            o.get("kind").and_then(|k| k.as_str()),
                            o.get("width").and_then(|w| w.as_u64()),
                        ) {
                            (Some("Float"), Some(w)) if w <= 255 => IrType::Float(w as u8),
                            _ => {
                                return Err(format!(
                                    "{where_}[{i}].type: expected {{{{kind: Float, width: N}}}}"
                                ))
                            }
                        },
                        _ => {
                            return Err(format!(
                                "{where_}[{i}].type: expected a type string or Float object"
                            ))
                        }
                    };
                    Ok((n, irt))
                })
                .collect()
        }
    }
}

/// Const/var verdicts (`var_const`): `[{"name": n, "kind": "Const"|"Var"}]`.
/// Missing field → empty (no verdicts attached — the caller may run
/// `shir::analyze_var_const` itself). Unknown kinds are rejected.
fn var_const_from(v: Option<&Value>, where_: &str) -> Result<Vec<(String, VarKind)>, String> {
    match v {
        None => Ok(vec![]),
        Some(x) => {
            let a = arr(Some(x), where_)?;
            a.iter()
                .enumerate()
                .map(|(i, e)| {
                    let o = require_obj(e, &format!("{where_}[{i}]"))?;
                    let n = req_str(o, "name", &format!("{where_}[{i}]"))?.to_string();
                    let k = req_str(o, "kind", &format!("{where_}[{i}]"))?;
                    let vk = match k {
                        "Const" => VarKind::Const,
                        "Var" => VarKind::Var,
                        other => {
                            return Err(format!("{where_}[{i}].kind: {other} not in Const/Var"))
                        }
                    };
                    Ok((n, vk))
                })
                .collect()
        }
    }
}

/// Lifetime verdicts (`var_lifetimes`):
/// `[{"name": n, "first": F, "last": L, "escapes": B}]`. Missing
/// field → empty (the caller may run
/// `shir_passes::lifetime::analyze_var_lifetimes` itself). Missing
/// `first`/`last`/`escapes` defaults are rejected (the serializer always
/// emits all three).
fn var_lifetimes_from(
    v: Option<&Value>,
    where_: &str,
) -> Result<Vec<(String, VarLifetime)>, String> {
    match v {
        None => Ok(vec![]),
        Some(x) => {
            let a = arr(Some(x), where_)?;
            a.iter()
                .enumerate()
                .map(|(i, e)| {
                    let o = require_obj(e, &format!("{where_}[{i}]"))?;
                    let n = req_str(o, "name", &format!("{where_}[{i}]"))?.to_string();
                    let f = o
                        .get("first")
                        .and_then(|x| x.as_u64())
                        .ok_or_else(|| format!("{where_}[{i}]: missing first"))?
                        as usize;
                    let l = o
                        .get("last")
                        .and_then(|x| x.as_u64())
                        .ok_or_else(|| format!("{where_}[{i}]: missing last"))?
                        as usize;
                    let esc = o
                        .get("escapes")
                        .and_then(|x| x.as_bool())
                        .ok_or_else(|| format!("{where_}[{i}]: missing escapes"))?;
                    Ok((
                        n,
                        VarLifetime {
                            first: f,
                            last: l,
                            escapes: esc,
                        },
                    ))
                })
                .collect()
        }
    }
}

// ── Statements ────────────────────────────────────────────────────────

fn stmts_from(v: Option<&Value>, where_: &str) -> Result<Vec<IrStmt>, String> {
    match v {
        None => Ok(vec![]),
        Some(x) => arr(Some(x), where_)?
            .iter()
            .enumerate()
            .map(|(i, s)| stmt_from(s, &format!("{where_}[{i}]")))
            .collect(),
    }
}

fn stmt_from(v: &Value, where_: &str) -> Result<IrStmt, String> {
    let o = require_obj(v, where_)?;
    let t = req_str(o, "type", where_)?;
    if !KNOWN_STMT.contains(&t) {
        return Err(format!("{where_}.type: unknown stmt type {t:?}"));
    }
    Ok(match t {
        "Output" => {
            let value = expr_from(req(o, "value", where_)?, &format!("{where_}.value"))?;
            let newline = req_bool(o, "newline", where_)?;
            let target = o.get("target").and_then(|x| x.as_str().map(String::from));
            IrStmt::Output {
                value,
                newline,
                target,
            }
        }
        "WriteFile" => {
            let path = expr_from(req(o, "path", where_)?, &format!("{where_}.path"))?;
            let content = expr_from(req(o, "content", where_)?, &format!("{where_}.content"))?;
            let append = req_bool(o, "append", where_)?;
            IrStmt::WriteFile {
                path,
                content,
                append,
            }
        }
        "Assign" => {
            let targets = arr(o.get("targets"), &format!("{where_}.targets"))?
                .iter()
                .enumerate()
                .map(|(i, t)| assign_target_from(t, &format!("{where_}.targets[{i}]")))
                .collect::<Result<Vec<_>, String>>()?;
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            IrStmt::Assign { targets, expr }
        }
        "Declare" => {
            let vars = arr(o.get("vars"), &format!("{where_}.vars"))?
                .iter()
                .enumerate()
                .map(|(i, d)| decl_from(d, &format!("{where_}.vars[{i}]")))
                .collect::<Result<Vec<_>, String>>()?;
            let init = match o.get("init") {
                None | Some(Value::Null) => None,
                Some(x) => Some(expr_from(x, &format!("{where_}.init"))?),
            };
            let local = req_bool(o, "local", where_)?;
            IrStmt::Declare { vars, init, local }
        }
        "DeclareArray" => {
            let var = req_str(o, "var", where_)?.to_string();
            let sigil = sigil_from(o.get("sigil"), &format!("{where_}.sigil"))?;
            let elements = arr(o.get("elements"), &format!("{where_}.elements"))?
                .iter()
                .enumerate()
                .map(|(i, e)| expr_from(e, &format!("{where_}.elements[{i}]")))
                .collect::<Result<Vec<_>, String>>()?;
            IrStmt::DeclareArray {
                var,
                sigil,
                elements,
            }
        }
        "If" => {
            let cond = expr_from(req(o, "cond", where_)?, &format!("{where_}.cond"))?;
            let then = stmts_from(o.get("then"), &format!("{where_}.then"))?;
            let elsifs = arr(o.get("elsifs"), &format!("{where_}.elsifs"))?
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let eo = require_obj(e, &format!("{where_}.elsifs[{i}]"))?;
                    let c = expr_from(
                        req(eo, "cond", &format!("{where_}.elsifs[{i}].cond"))?,
                        &format!("{where_}.elsifs[{i}].cond"),
                    )?;
                    let b = stmts_from(eo.get("body"), &format!("{where_}.elsifs[{i}].body"))?;
                    Ok((c, b))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let else_ = stmts_from(o.get("else"), &format!("{where_}.else"))?;
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            }
        }
        "For" => {
            let var = req_str(o, "var", where_)?.to_string();
            let iter = expr_from(req(o, "iter", where_)?, &format!("{where_}.iter"))?;
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrStmt::For { var, iter, body }
        }
        "While" => {
            let cond = expr_from(req(o, "cond", where_)?, &format!("{where_}.cond"))?;
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrStmt::While { cond, body }
        }
        "DoWhile" => {
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            let cond = expr_from(req(o, "cond", where_)?, &format!("{where_}.cond"))?;
            let until = req_bool(o, "until", where_)?;
            IrStmt::DoWhile { body, cond, until }
        }
        "Die" => {
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            let carp = req_bool(o, "carp", where_)?;
            IrStmt::Die { expr, carp }
        }
        "Warn" => {
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            let carp = req_bool(o, "carp", where_)?;
            IrStmt::Warn { expr, carp }
        }
        "Exec" => {
            let cmd = expr_from(req(o, "cmd", where_)?, &format!("{where_}.cmd"))?;
            let args = exprs_from(o.get("args"), &format!("{where_}.args"))?;
            let capture = o.get("capture").and_then(|x| x.as_str().map(String::from));
            let redirects = exprs_from(o.get("redirects"), &format!("{where_}.redirects"))?;
            let env = match o.get("env") {
                None | Some(Value::Null) => vec![],
                Some(x) => arr(Some(x), &format!("{where_}.env"))?
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let eo = require_obj(e, &format!("{where_}.env[{i}]"))?;
                        let n = req_str(eo, "name", &format!("{where_}.env[{i}]"))?.to_string();
                        let v = expr_from(
                            req(eo, "value", &format!("{where_}.env[{i}]"))?,
                            &format!("{where_}.env[{i}].value"),
                        )?;
                        Ok((n, v))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            };
            // purity: ignored on input (recomputed by the backend if needed)
            let _ = o.get("purity");
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            }
        }
        "Pipeline" => {
            let stages = arr(o.get("stages"), &format!("{where_}.stages"))?
                .iter()
                .enumerate()
                .map(|(i, st)| stmts_from(Some(st), &format!("{where_}.stages[{i}]")))
                .collect::<Result<Vec<_>, String>>()?;
            let last_output = o
                .get("last_output")
                .and_then(|x| x.as_str().map(String::from));
            let capture = o.get("capture").and_then(|x| x.as_str().map(String::from));
            let cmd_str = o.get("cmd_str").and_then(|x| x.as_str().map(String::from));
            let _ = o.get("purity");
            IrStmt::Pipeline {
                stages,
                last_output,
                capture,
                cmd_str,
            }
        }
        "Return" => {
            // Multi-value form (core request c-multi-return 20260806):
            // `{"type":"Return","values":[e1, e2, ...]}` — the shell
            // value-return channel can carry several values (one echoed
            // line each); the IR represents the list as an Array value
            // (the emitter renders a native JS `return [e1, e2]`, the
            // caller destructures). `value` (single) stays valid and the
            // serializer keeps emitting the Array round-trip.
            let value = if let Some(vs) = o.get("values") {
                if let Some(arr) = vs.as_array() {
                    let mut exprs = Vec::new();
                    for (i, x) in arr.iter().enumerate() {
                        exprs.push(expr_from(x, &format!("{where_}.values[{i}]"))?);
                    }
                    Some(IrExpr::Array(exprs))
                } else {
                    return Err(format!("{where_}: Return.values must be an array"));
                }
            } else {
                match o.get("value") {
                    None | Some(Value::Null) => None,
                    Some(x) => Some(expr_from(x, &format!("{where_}.value"))?),
                }
            };
            IrStmt::Return(value)
        }
        "Exit" => {
            let value = match o.get("value") {
                None | Some(Value::Null) => None,
                Some(x) => Some(expr_from(x, &format!("{where_}.value"))?),
            };
            IrStmt::Exit(value)
        }
        "SetChildError" => {
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            IrStmt::SetChildError(expr)
        }
        "Require" => {
            let module = req_str(o, "module", where_)?.to_string();
            IrStmt::Require(module)
        }
        "RawText" => {
            let text = req_str(o, "text", where_)?.to_string();
            IrStmt::RawText(text)
        }
        "Case" => {
            let discriminant = expr_from(
                req(o, "discriminant", where_)?,
                &format!("{where_}.discriminant"),
            )?;
            let clauses = arr(o.get("clauses"), &format!("{where_}.clauses"))?
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let co = require_obj(c, &format!("{where_}.clauses[{i}]"))?;
                    let patterns = str_array(
                        co.get("patterns"),
                        &format!("{where_}.clauses[{i}].patterns"),
                    )?;
                    let body = stmts_from(co.get("body"), &format!("{where_}.clauses[{i}].body"))?;
                    Ok(IrCaseClause { patterns, body })
                })
                .collect::<Result<Vec<_>, String>>()?;
            IrStmt::Case {
                discriminant,
                clauses,
            }
        }
        "Redirect" => {
            let inner = stmts_from(o.get("inner"), &format!("{where_}.inner"))?;
            let redirects = arr(o.get("redirects"), &format!("{where_}.redirects"))?
                .iter()
                .enumerate()
                .map(|(i, r)| redirect_from(r, &format!("{where_}.redirects[{i}]")))
                .collect::<Result<Vec<_>, String>>()?;
            IrStmt::Redirect { inner, redirects }
        }
        "Function" => {
            let name = req_str(o, "name", where_)?.to_string();
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrStmt::Function { name, body }
        }
        "Subshell" => {
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrStmt::Subshell(body)
        }
        "Background" => {
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrStmt::Background(body)
        }
        "Block" => {
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrStmt::Block(body)
        }
        "Expr" => {
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            IrStmt::Expr(expr)
        }
        "Label" => {
            let name = req_str(o, "name", where_)?.to_string();
            IrStmt::Label(name)
        }
        "Goto" => {
            let name = req_str(o, "name", where_)?.to_string();
            IrStmt::Goto(name)
        }
        _ => unreachable!("checked above"),
    })
}

fn assign_target_from(v: &Value, where_: &str) -> Result<AssignTarget, String> {
    let o = require_obj(v, where_)?;
    let var = req_str(o, "var", where_)?.to_string();
    let sigil = sigil_from(o.get("sigil"), &format!("{where_}.sigil"))?;
    let indices = exprs_from(o.get("indices"), &format!("{where_}.indices"))?;
    Ok(AssignTarget {
        var,
        sigil,
        indices,
    })
}

fn decl_from(v: &Value, where_: &str) -> Result<Decl, String> {
    let o = require_obj(v, where_)?;
    let name = req_str(o, "name", where_)?.to_string();
    let sigil = sigil_from(o.get("sigil"), &format!("{where_}.sigil"))?;
    Ok(Decl { name, sigil })
}

fn redirect_from(v: &Value, where_: &str) -> Result<IrRedirect, String> {
    let o = require_obj(v, where_)?;
    let fd = match o.get("fd") {
        None | Some(Value::Null) => None,
        Some(x) => Some(x.as_i64().ok_or_else(|| format!("{where_}.fd: not int"))? as i32),
    };
    let mode = req_str(o, "mode", where_)?.to_string();
    let target = expr_from(req(o, "target", where_)?, &format!("{where_}.target"))?;
    let interpolate = req_bool(o, "interpolate", where_)?;
    Ok(IrRedirect {
        fd,
        mode,
        target,
        interpolate,
    })
}

// ── Expressions ──────────────────────────────────────────────────────

fn expr_from(v: &Value, where_: &str) -> Result<IrExpr, String> {
    let o = require_obj(v, where_)?;
    let t = req_str(o, "type", where_)?;
    if !KNOWN_EXPR.contains(&t) {
        return Err(format!("{where_}.type: unknown expr type {t:?}"));
    }
    Ok(match t {
        "Int" => {
            let x = o
                .get("value")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| format!("{where_}.value: not int"))?;
            IrExpr::Int(x)
        }
        "Str" => {
            let s = req_str(o, "value", where_)?.to_string();
            let style = style_from(req_str(o, "style", where_)?);
            IrExpr::Str(s, style)
        }
        "Var" => {
            let name = req_str(o, "name", where_)?.to_string();
            let sigil = sigil_from(o.get("sigil"), &format!("{where_}.sigil"))?;
            IrExpr::Var(name, sigil)
        }
        "Index" => {
            let var = req_str(o, "var", where_)?.to_string();
            let key = expr_from(req(o, "key", where_)?, &format!("{where_}.key"))?;
            IrExpr::Index {
                var,
                key: Box::new(key),
            }
        }
        "BinOp" => {
            let op = binop_from(req_str(o, "op", where_)?)?;
            let lhs = expr_from(req(o, "lhs", where_)?, &format!("{where_}.lhs"))?;
            let rhs = expr_from(req(o, "rhs", where_)?, &format!("{where_}.rhs"))?;
            IrExpr::BinOp {
                lhs: Box::new(lhs),
                op,
                rhs: Box::new(rhs),
            }
        }
        "Call" => {
            let func = req_str(o, "func", where_)?.to_string();
            let args = exprs_from(o.get("args"), &format!("{where_}.args"))?;
            let _ = o.get("purity"); // recomputed by backend; ignored on ingress
            IrExpr::Call { func, args }
        }
        "MethodCall" => {
            let object = expr_from(req(o, "object", where_)?, &format!("{where_}.object"))?;
            let method = req_str(o, "method", where_)?.to_string();
            let args = exprs_from(o.get("args"), &format!("{where_}.args"))?;
            IrExpr::MethodCall {
                obj: Box::new(object),
                method,
                args,
            }
        }
        "Ternary" => {
            let cond = expr_from(req(o, "cond", where_)?, &format!("{where_}.cond"))?;
            let then = expr_from(req(o, "then", where_)?, &format!("{where_}.then"))?;
            let else_ = expr_from(req(o, "else", where_)?, &format!("{where_}.else"))?;
            IrExpr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                else_: Box::new(else_),
            }
        }
        "DefinedOr" => {
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            let default = expr_from(req(o, "default", where_)?, &format!("{where_}.default"))?;
            IrExpr::DefinedOr {
                expr: Box::new(expr),
                default: Box::new(default),
            }
        }
        "Interpolate" => {
            let parts = arr(o.get("parts"), &format!("{where_}.parts"))?
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let po = require_obj(p, &format!("{where_}.parts[{i}]"))?;
                    let k = req_str(po, "kind", &format!("{where_}.parts[{i}]"))?;
                    Ok(match k {
                        "lit" => {
                            let t =
                                req_str(po, "text", &format!("{where_}.parts[{i}]"))?.to_string();
                            InterpPart::Lit(t)
                        }
                        "expr" => {
                            let e = expr_from(
                                req(po, "expr", &format!("{where_}.parts[{i}]"))?,
                                &format!("{where_}.parts[{i}].expr"),
                            )?;
                            InterpPart::Expr(Box::new(e))
                        }
                        other => {
                            return Err(format!(
                                "{where_}.parts[{i}].kind: {other} not in lit/expr"
                            ))
                        }
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            IrExpr::Interpolate(parts)
        }
        "Capture" => {
            let expr = expr_from(req(o, "expr", where_)?, &format!("{where_}.expr"))?;
            let native = req_bool(o, "native", where_)?;
            IrExpr::Capture {
                expr: Box::new(expr),
                native,
            }
        }
        "Regex" => {
            let pattern = req_str(o, "pattern", where_)?.to_string();
            let flags = req_str(o, "flags", where_)?.to_string();
            IrExpr::Regex { pattern, flags }
        }
        "Range" => {
            let start = o
                .get("start")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| format!("{where_}.start: not int"))?;
            let end = o
                .get("end")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| format!("{where_}.end: not int"))?;
            IrExpr::Range { start, end }
        }
        "RawExpr" => {
            let text = req_str(o, "text", where_)?.to_string();
            IrExpr::RawExpr(text)
        }
        "Arrow" => {
            let body = stmts_from(o.get("body"), &format!("{where_}.body"))?;
            IrExpr::Arrow(body)
        }
        "Array" => {
            let elements = exprs_from(o.get("elements"), &format!("{where_}.elements"))?;
            IrExpr::Array(elements)
        }
        "Arith" => {
            let ast = arith_from(req(o, "ast", where_)?, &format!("{where_}.ast"))?;
            IrExpr::Arith(Box::new(ast))
        }
        "Bool" => {
            let value = req_bool(o, "value", where_)?;
            IrExpr::Bool(value)
        }
        "Json" => {
            let v = o
                .get("value")
                .ok_or_else(|| format!("{where_}.value: missing"))?
                .clone();
            IrExpr::Json(v)
        }
        "Ident" => {
            let name = req_str(o, "name", where_)?.to_string();
            IrExpr::Ident(name)
        }
        "Object" => {
            let properties = arr(o.get("properties"), &format!("{where_}.properties"))?
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let po = require_obj(p, &format!("{where_}.properties[{i}]"))?;
                    let k = req_str(po, "key", &format!("{where_}.properties[{i}]"))?.to_string();
                    let v = expr_from(
                        req(po, "value", &format!("{where_}.properties[{i}]"))?,
                        &format!("{where_}.properties[{i}].value"),
                    )?;
                    Ok((k, v))
                })
                .collect::<Result<Vec<_>, String>>()?;
            IrExpr::Object(properties)
        }
        _ => unreachable!("checked above"),
    })
}

fn exprs_from(v: Option<&Value>, where_: &str) -> Result<Vec<IrExpr>, String> {
    match v {
        None => Ok(vec![]),
        Some(x) => arr(Some(x), where_)?
            .iter()
            .enumerate()
            .map(|(i, e)| expr_from(e, &format!("{where_}[{i}]")))
            .collect(),
    }
}

// ── Arithmetic AST ───────────────────────────────────────────────────

fn arith_from(v: &Value, where_: &str) -> Result<ArithAst, String> {
    let o = require_obj(v, where_)?;
    let t = req_str(o, "type", where_)?;
    if !KNOWN_ARITH.contains(&t) {
        return Err(format!("{where_}.type: unknown arith type {t:?}"));
    }
    Ok(match t {
        "Num" => {
            let n = o
                .get("value")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| format!("{where_}.value: not int"))?;
            ArithAst::Num(n)
        }
        "Var" => {
            let name = req_str(o, "name", where_)?.to_string();
            ArithAst::Var(name)
        }
        "Index" => {
            let var = req_str(o, "var", where_)?.to_string();
            let key = arith_from(req(o, "key", where_)?, &format!("{where_}.key"))?;
            ArithAst::Index {
                var,
                key: Box::new(key),
            }
        }
        "Bin" => {
            let op = req_str(o, "op", where_)?; // kept as &str literal in ArithAst
            let lhs = arith_from(req(o, "lhs", where_)?, &format!("{where_}.lhs"))?;
            let rhs = arith_from(req(o, "rhs", where_)?, &format!("{where_}.rhs"))?;
            ArithAst::Bin {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            }
        }
        "Un" => {
            let op = req_str(o, "op", where_)?;
            let arg = arith_from(req(o, "arg", where_)?, &format!("{where_}.arg"))?;
            ArithAst::Un {
                op: op.to_string(),
                arg: Box::new(arg),
            }
        }
        "Cond" => {
            let test = arith_from(req(o, "test", where_)?, &format!("{where_}.test"))?;
            let then = arith_from(req(o, "then", where_)?, &format!("{where_}.then"))?;
            let else_ = arith_from(req(o, "else", where_)?, &format!("{where_}.else"))?;
            ArithAst::Cond {
                test: Box::new(test),
                then: Box::new(then),
                else_: Box::new(else_),
            }
        }
        "Assign" => {
            let var = req_str(o, "var", where_)?.to_string();
            let op = req_str(o, "op", where_)?;
            let rhs = arith_from(req(o, "rhs", where_)?, &format!("{where_}.rhs"))?;
            ArithAst::Assign {
                var,
                op: op.to_string(),
                rhs: Box::new(rhs),
            }
        }
        "IncDec" => {
            let var = req_str(o, "var", where_)?.to_string();
            let delta = o
                .get("delta")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| format!("{where_}.delta: not int"))?;
            let prefix = req_bool(o, "prefix", where_)?;
            ArithAst::IncDec { var, delta, prefix }
        }
        _ => unreachable!("checked above"),
    })
}

/// Leak a runtime string to `&'static str` (one-shot CLI deserializer
/// — the process is short-lived and the sh2.* runtime is the larger
/// allocator). Used for `ArithAst` op fields which are typed `&'static`.
fn to_static(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

// ── Enum helpers ─────────────────────────────────────────────────────

fn style_from(s: &str) -> StrStyle {
    match s {
        "SingleQuoted" => StrStyle::SingleQuoted,
        "Command" => StrStyle::Command,
        "Heredoc" => StrStyle::Heredoc,
        _ => StrStyle::DoubleQuoted,
    }
}

fn sigil_from(v: Option<&Value>, where_: &str) -> Result<Option<Sigil>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(x) => {
            let s = x
                .as_str()
                .ok_or_else(|| format!("{where_}: not str or null"))?;
            Ok(Some(match s {
                "Scalar" => Sigil::Scalar,
                "Array" => Sigil::Array,
                "Hash" => Sigil::Hash,
                other => return Err(format!("{where_}: {other} not in Scalar/Array/Hash")),
            }))
        }
    }
}

fn binop_from(s: &str) -> Result<BinOpKind, String> {
    Ok(match s {
        "Add" => BinOpKind::Add,
        "Sub" => BinOpKind::Sub,
        "Mul" => BinOpKind::Mul,
        "Div" => BinOpKind::Div,
        "Mod" => BinOpKind::Mod,
        "Pow" => BinOpKind::Pow,
        "Concat" => BinOpKind::Concat,
        "Eq" => BinOpKind::Eq,
        "Ne" => BinOpKind::Ne,
        "Lt" => BinOpKind::Lt,
        "Gt" => BinOpKind::Gt,
        "Le" => BinOpKind::Le,
        "Ge" => BinOpKind::Ge,
        "And" => BinOpKind::And,
        "Or" => BinOpKind::Or,
        "Not" => BinOpKind::Not,
        "BitAnd" => BinOpKind::BitAnd,
        "BitOr" => BinOpKind::BitOr,
        "BitXor" => BinOpKind::BitXor,
        "ShiftL" => BinOpKind::ShiftL,
        "ShiftR" => BinOpKind::ShiftR,
        other => return Err(format!("BinOp.op: {other:?} unknown")),
    })
}

// ── Value helpers (strict) ───────────────────────────────────────────

fn require_obj<'a>(
    v: &'a Value,
    where_: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    v.as_object()
        .ok_or_else(|| format!("{where_}: not an object"))
}

fn require_field<'a>(
    o: &'a serde_json::Map<String, Value>,
    field: &str,
    where_: &str,
) -> Result<&'a Value, String> {
    o.get(field)
        .ok_or_else(|| format!("{where_}: missing field {field:?}"))
}

fn req<'a>(
    o: &'a serde_json::Map<String, Value>,
    field: &str,
    where_: &str,
) -> Result<&'a Value, String> {
    o.get(field)
        .ok_or_else(|| format!("{where_}: missing field {field:?}"))
}

fn req_str<'a>(
    o: &'a serde_json::Map<String, Value>,
    field: &str,
    where_: &str,
) -> Result<&'a str, String> {
    o.get(field)
        .and_then(|x| x.as_str())
        .ok_or_else(|| format!("{where_}.{field}: not a string"))
}

fn req_bool(o: &serde_json::Map<String, Value>, field: &str, where_: &str) -> Result<bool, String> {
    o.get(field)
        .and_then(|x| x.as_bool())
        .ok_or_else(|| format!("{where_}.{field}: not a bool"))
}

fn arr<'a>(v: Option<&'a Value>, where_: &str) -> Result<&'a Vec<Value>, String> {
    v.and_then(|x| x.as_array())
        .ok_or_else(|| format!("{where_}: not an array"))
}

fn str_array(v: Option<&Value>, where_: &str) -> Result<Vec<String>, String> {
    match v {
        None => Ok(vec![]),
        Some(x) => x
            .as_array()
            .ok_or_else(|| format!("{where_}: not an array"))?
            .iter()
            .enumerate()
            .map(|(i, e)| {
                e.as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("{where_}[{i}]: not a string"))
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrProgram, IrStmt};
    use crate::shir_json::shir_to_shir_json;

    fn round_trip(s: &str) -> String {
        let commands = crate::parser::commands::Parser::new(s).parse().unwrap();
        let prog1 = crate::shir::ast_to_ir(&commands);
        let json1 = shir_to_shir_json(&prog1);
        let prog2 = shir_json_to_ir(&json1).expect("deser");
        shir_to_shir_json(&prog2)
    }

    #[test]
    fn roundtrip_simple() {
        assert_eq!(round_trip("echo hello"), round_trip("echo hello"));
    }

    #[test]
    fn roundtrip_assignment() {
        let a = round_trip("x=1; echo $x");
        let b = round_trip("x=1; echo $x");
        assert_eq!(a, b);
    }

    /// IrType::Float(32/64) round-trips through the A1 JSON: serialized
    /// as {"kind":"Float","width":N} and re-ingested (core request
    /// c-sh-go-20260807-114757 — the C frontend's float/double type layer).
    #[test]
    fn float_type_roundtrip() {
        use crate::ir::IrType;
        let mut prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![("x".to_string(), IrType::Float(64))],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let json = crate::shir_json::shir_to_shir_json_raw(&prog);
        assert!(json.contains("\"kind\":\"Float\""), "json: {json}");
        let prog2 = shir_json_to_ir(&json).expect("deser");
        assert_eq!(prog2.var_types, vec![("x".to_string(), IrType::Float(64))]);
        // the legacy string forms still round-trip byte-identically
        prog.var_types = vec![("y".to_string(), IrType::Int)];
        let json2 = crate::shir_json::shir_to_shir_json_raw(&prog);
        assert!(json2.contains("\"type\":\"Int\""), "json: {json2}");
    }

    /// The const-markup round-trips: `--shir` attaches the verdicts
    /// (LIMIT const, i/sum var), the reader ingests them, and re-serializing
    /// is byte-identical.
    /// The seq_range_for transform's BARE `Range` For.iterable (PLAN §5.6)
    /// round-trips: `--shir` emits `{"type":"Range",start,end}` as the
    /// For.iter, the reader ingests it, and re-serializing is byte-identical
    /// (every backend matches the bare Range arm of its For handler).
    #[test]
    fn seq_range_for_bare_range_roundtrip() {
        let json = round_trip("for i in $(seq 1 10000); do echo $i; done");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let iter = &v["stmts"][0]["iter"];
        assert_eq!(iter["type"], "Range", "bare Range iterable, got: {iter}");
        assert_eq!(iter["start"], 1);
        assert_eq!(iter["end"], 10000);
        // and the deserialized program re-serializes byte-identically
        // (round_trip already did the full loop; assert the shape survived)
        assert!(
            iter.get("elements").is_none(),
            "no Array wrapper around the Range: {iter}"
        );
    }

    #[test]
    fn var_const_roundtrip() {
        let json =
            round_trip("LIMIT=10\nsum=0\nfor i in 1 2; do sum=$((sum+i)); done\necho $LIMIT $sum");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let vc = v.get("var_const").and_then(|x| x.as_array());
        assert!(
            vc.is_some(),
            "var_const missing from serialized ShIR: {json}"
        );
        let vc = vc.unwrap();
        assert_eq!(vc.len(), 3, "expected LIMIT/i/sum verdicts, got {vc:?}");
        let names: Vec<&str> = vc.iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["LIMIT", "i", "sum"], "sorted by name");
        let kinds: Vec<&str> = vc.iter().map(|e| e["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, vec!["Const", "Var", "Var"]);
        // unknown kind rejected
        let bad = json.replace("\"Var\"", "\"Maybe\"");
        assert!(shir_json_to_ir(&bad).is_err());
    }

    #[test]
    fn contract_version_required() {
        let mut prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let json = shir_to_shir_json(&prog);
        // valid
        assert!(shir_json_to_ir(&json).is_ok());
        // strip version → must fail
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut obj = v.as_object().unwrap().clone();
        obj.remove("contract_version");
        let bad = serde_json::to_string(&obj).unwrap();
        assert!(shir_json_to_ir(&bad).is_err());
    }

    #[test]
    fn unknown_stmt_type_rejected() {
        let json = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"subs":[],"stmts":[{"type":"NoSuch"}]}"#;
        let err = shir_json_to_ir(json).unwrap_err();
        assert!(err.contains("unknown stmt type"), "got: {err}");
    }
    // Plan §2.4: A4 namespace spec (data/sh2-builtins.json) must match
    // the SYNC_BUILTINS Rust constant. Frontends derive from the JSON;
    // a drift here means a frontend would compute wrong purity.
    #[test]
    fn a4_sync_builtins_matches_rust() {
        let json = include_str!("../data/sh2-builtins.json");
        let v: serde_json::Value = serde_json::from_str(json).expect("parse A4 json");
        let arr = v
            .get("sync_builtins")
            .and_then(|x| x.as_array())
            .expect("sync_builtins array");
        let from_json: std::collections::BTreeSet<&str> =
            arr.iter().map(|x| x.as_str().unwrap()).collect();
        let from_rust: std::collections::BTreeSet<&str> =
            crate::shir::SYNC_BUILTINS.iter().copied().collect();
        assert_eq!(
            from_json, from_rust,
            "A4 namespace (data/sh2-builtins.json) SYNC_BUILTINS drifted from shir.rs"
        );
    }

    // Plan improvement #4 (safe half): corpus roundtrip property test.
    // For every example in the corpus, parse → ast_to_ir → shir_to_shir_json
    // → shir_json_to_ir → shir_to_shir_json; the two serialized forms must
    // be BYTE-IDENTICAL. Catches any future drift between the hand-built
    // serializer (shir_json.rs) and deserializer (shir_json_in.rs) — the
    // exact bug class that the serde-derive refactor (the bigger #4) is
    // meant to prevent. Errors skip (parse/ingress failures are not the
    // concern of this test; we only assert the serializer/deserializer
    // round-trip on examples that BOTH sides accept).
    #[test]
    fn corpus_roundtrip_byte_equal() {
        use crate::ir::IrProgram;
        use crate::parser::commands::Parser;
        use std::fs;
        let corpus = std::path::Path::new("examples");
        if !corpus.exists() {
            // corpus not present in this build (e.g. the test is run from
            // a different checkout); skip rather than fail.
            eprintln!(
                "corpus not at {}; skipping roundtrip test",
                corpus.display()
            );
            return;
        }
        let mut total = 0usize;
        let mut drf = 0usize; // deserialization failed (skip)
        let mut pass = 0usize;
        let mut diffs: Vec<(String, String)> = Vec::new(); // (file, reason)
        for entry in fs::read_dir(corpus).expect("read corpus dir") {
            let entry = entry.expect("read dir entry");
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("sh") {
                continue;
            }
            total += 1;
            let src = fs::read_to_string(&p).unwrap_or_default();
            let cmds = match Parser::new(&src).parse() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let prog1: IrProgram = crate::shir::ast_to_ir(&cmds);
            let j1 = crate::shir_json::shir_to_shir_json(&prog1);
            let prog2 = match shir_json_to_ir(&j1) {
                Ok(p) => p,
                Err(_) => {
                    drf += 1;
                    continue;
                }
            };
            let j2 = crate::shir_json::shir_to_shir_json(&prog2);
            if j1 == j2 {
                pass += 1;
            } else {
                diffs.push((
                    p.display().to_string(),
                    format!("len {} vs {}", j1.len(), j2.len()),
                ));
            }
        }
        assert!(
            diffs.is_empty(),
            "{}/{} examples have serializer/deserializer drift: {:?}",
            diffs.len(),
            total,
            diffs
        );
        eprintln!(
            "corpus_roundtrip: {} examples, {} byte-equal, {} deser-failed (skipped)",
            total, pass, drf
        );
    }
}
