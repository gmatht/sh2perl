//! enc — a small, clean JSON encoder for the CORE expr/stmt types used
//! inside generated shir_nodes. NOT the A1 contract shape (backward
//! compatibility is explicitly out of scope for the spike): it exists so
//! declared nodes can embed `expr`/`stmts` fields and round-trip them.

use crate::ir::{IrExpr, IrStmt, StrStyle};
use serde_json::json;
use serde_json::Value;

/// A `stmts` field → JSON array.
pub fn stmts_to_json(stmts: &[IrStmt]) -> Value {
    Value::Array(stmts.iter().map(stmt_to_json).collect())
}

pub fn stmt_to_json(s: &IrStmt) -> Value {
    match s {
        IrStmt::Expr(e) => json!({"stmt": "Expr", "expr": expr_to_json(e)}),
        IrStmt::Output { value, newline, .. } => {
            json!({"stmt": "Output", "value": expr_to_json(value), "newline": newline})
        }
        other => json!({"stmt": "Other", "repr": format!("{other:?}")}),
    }
}

pub fn json_to_stmts(v: &Value) -> Result<Vec<IrStmt>, String> {
    v.as_array()
        .ok_or("stmts field must be an array".to_string())?
        .iter()
        .map(json_to_stmt)
        .collect()
}

pub fn json_to_stmt(v: &Value) -> Result<IrStmt, String> {
    match v.get("stmt").and_then(Value::as_str) {
        Some("Expr") => Ok(IrStmt::Expr(json_to_expr(&v["expr"])?)),
        Some("Output") => Ok(IrStmt::Output {
            value: json_to_expr(&v["value"])?,
            newline: v["newline"].as_bool().unwrap_or(true),
            target: None,
        }),
        _ => Err(format!("unknown stmt encoding: {v}")),
    }
}

pub fn expr_to_json(e: &IrExpr) -> Value {
    match e {
        IrExpr::Int(n) => json!({"kind": "Int", "value": n}),
        IrExpr::Bool(b) => json!({"kind": "Bool", "value": b}),
        IrExpr::Str(s, _) => json!({"kind": "Str", "value": s}),
        IrExpr::Var(name, _) => json!({"kind": "Var", "name": name}),
        other => json!({"kind": "Other", "repr": format!("{other:?}")}),
    }
}

pub fn json_to_expr(v: &Value) -> Result<IrExpr, String> {
    match v.get("kind").and_then(Value::as_str) {
        Some("Int") => Ok(IrExpr::Int(v["value"].as_i64().ok_or("Int.value")?)),
        Some("Bool") => Ok(IrExpr::Bool(v["value"].as_bool().ok_or("Bool.value")?)),
        Some("Str") => Ok(IrExpr::Str(
            v["value"].as_str().ok_or("Str.value")?.to_string(),
            StrStyle::DoubleQuoted,
        )),
        Some("Var") => Ok(IrExpr::Var(
            v["name"].as_str().ok_or("Var.name")?.to_string(),
            None,
        )),
        _ => Err(format!("unknown expr encoding: {v}")),
    }
}
