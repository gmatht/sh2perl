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
        IrStmt::Assign { targets, expr, .. } => json!({
            "stmt": "Assign",
            "targets": targets.iter().map(|t| json!({
                "var": t.var,
                "indices": t.indices.iter().map(expr_to_json).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "expr": expr_to_json(expr),
        }),
        IrStmt::If { cond, then, else_, .. } => json!({
            "stmt": "If", "cond": expr_to_json(cond),
            "then": stmts_to_json(then), "else": stmts_to_json(else_),
        }),
        IrStmt::SetChildError(e) => json!({"stmt": "SetChildError", "expr": expr_to_json(e)}),
        IrStmt::Block(b) => json!({"stmt": "Block", "body": stmts_to_json(b)}),
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
        Some("Assign") => Ok(IrStmt::Assign {
            targets: v["targets"].as_array().ok_or("Assign.targets")?.iter()
                .map(|t| Ok(crate::ir::AssignTarget {
                    var: t["var"].as_str().ok_or("target.var")?.to_string(),
                    sigil: None,
                    indices: json_to_exprs(&t["indices"])?,
                })).collect::<Result<Vec<_>, String>>()?,
            expr: json_to_expr(&v["expr"])?,
            asm: None,
        }),
        Some("If") => Ok(IrStmt::If {
            cond: json_to_expr(&v["cond"])?,
            then: json_to_stmts(&v["then"])?,
            elsifs: vec![],
            else_: json_to_stmts(&v["else"])?,
        }),
        Some("SetChildError") => Ok(IrStmt::SetChildError(json_to_expr(&v["expr"])?)),
        Some("Block") => Ok(IrStmt::Block(json_to_stmts(&v["body"])?)),
        _ => Err(format!("unknown stmt encoding: {v}")),
    }
}

pub fn expr_to_json(e: &IrExpr) -> Value {
    match e {
        IrExpr::Int(n) => json!({"kind": "Int", "value": n}),
        IrExpr::Bool(b) => json!({"kind": "Bool", "value": b}),
        IrExpr::Str(s, _) => json!({"kind": "Str", "value": s}),
        IrExpr::Var(name, _) => json!({"kind": "Var", "name": name}),
        IrExpr::BinOp { lhs, op, rhs } => json!({
            "kind": "BinOp", "op": format!("{op:?}"),
            "lhs": expr_to_json(lhs), "rhs": expr_to_json(rhs),
        }),
        IrExpr::Call { func, args } => json!({
            "kind": "Call", "func": func,
            "args": args.iter().map(expr_to_json).collect::<Vec<_>>(),
        }),
        IrExpr::Ext(n) => json!({"kind": "Ext", "tag": n.tag(), "node": n.to_json()}),
        other => json!({"kind": "Other", "repr": format!("{other:?}")}),
    }
}

pub fn json_to_expr(v: &Value) -> Result<IrExpr, String> {
    match v.get("kind").and_then(Value::as_str) {
        Some("Ext") => {
            let tag = v["tag"].as_str().ok_or("Ext.tag")?;
            let ctor = crate::shir_nodes::expr_node_ctor(tag)
                .ok_or_else(|| format!("unknown ext expr tag {tag}"))?;
            Ok(IrExpr::Ext(ctor(&v["node"])?))
        }
        Some("Call") => Ok(IrExpr::Call {
            func: v["func"].as_str().ok_or("Call.func")?.to_string(),
            args: json_to_exprs(&v["args"])?
        }),
        Some("BinOp") => {
            let op = match v["op"].as_str() {
                Some("Add") => crate::ir::BinOpKind::Add,
                Some("Sub") => crate::ir::BinOpKind::Sub,
                Some("Mul") => crate::ir::BinOpKind::Mul,
                other => return Err(format!("enc BinOp {other:?}")),
            };
            Ok(IrExpr::BinOp {
                lhs: Box::new(json_to_expr(&v["lhs"])?),
                op,
                rhs: Box::new(json_to_expr(&v["rhs"])?),
            })
        }
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

fn json_to_exprs(v: &Value) -> Result<Vec<IrExpr>, String> {
    v.as_array().ok_or("expected expr array".to_string())?
        .iter().map(json_to_expr).collect()
}
