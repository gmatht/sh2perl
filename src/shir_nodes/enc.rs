//! enc — a small, clean JSON encoder for the CORE expr/stmt types used
//! inside generated shir_nodes. NOT the A1 contract shape (backward
//! compatibility is explicitly out of scope for the spike): it exists so
//! declared nodes can embed `expr`/`stmts` fields and round-trip them.

use crate::ir::{InterpPart, IrExpr, IrStmt, StrStyle};
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
        // The composite exprs the reductions embed in node fields — a
        // variable read is Call{func:"param"}, a mixed string is
        // Interpolate, a command substitution is Capture. Encoding them
        // structurally (not as an opaque Debug repr) keeps the A1 export
        // of a reduced program parseable by the ingress.
        IrExpr::Call { func, args } => json!({
            "kind": "Call",
            "func": func,
            "args": args.iter().map(expr_to_json).collect::<Vec<_>>(),
        }),
        IrExpr::Interpolate(parts) => json!({
            "kind": "Interpolate",
            "parts": parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(s) => json!({"lit": s}),
                    InterpPart::Expr(e) => json!({"expr": expr_to_json(e)}),
                })
                .collect::<Vec<_>>(),
        }),
        IrExpr::Capture { expr, native } => json!({
            "kind": "Capture",
            "expr": expr_to_json(expr),
            "native": native,
        }),
        IrExpr::Arrow(body) => json!({
            "kind": "Arrow",
            "body": stmts_to_json(body),
        }),
        IrExpr::Array(items) => json!({
            "kind": "Array",
            "items": items.iter().map(expr_to_json).collect::<Vec<_>>(),
        }),
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
        Some("Call") => Ok(IrExpr::Call {
            func: v["func"].as_str().ok_or("Call.func")?.to_string(),
            args: v["args"]
                .as_array()
                .ok_or("Call.args")?
                .iter()
                .map(json_to_expr)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Some("Interpolate") => Ok(IrExpr::Interpolate(
            v["parts"]
                .as_array()
                .ok_or("Interpolate.parts")?
                .iter()
                .map(|p| {
                    if let Some(s) = p.get("lit").and_then(Value::as_str) {
                        Ok(InterpPart::Lit(s.to_string()))
                    } else if let Some(e) = p.get("expr") {
                        Ok(InterpPart::Expr(Box::new(json_to_expr(e)?)))
                    } else {
                        Err(format!("Interpolate part: {p}"))
                    }
                })
                .collect::<Result<Vec<_>, String>>()?,
        )),
        Some("Capture") => Ok(IrExpr::Capture {
            expr: Box::new(json_to_expr(&v["expr"])?),
            native: v["native"].as_bool().unwrap_or(false),
        }),
        Some("Arrow") => Ok(IrExpr::Arrow(json_to_stmts(&v["body"])?)),
        Some("Array") => Ok(IrExpr::Array(
            v["items"]
                .as_array()
                .ok_or("Array.items")?
                .iter()
                .map(json_to_expr)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        _ => Err(format!("unknown expr encoding: {v}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(e: IrExpr) {
        let j = expr_to_json(&e);
        let back = json_to_expr(&j).expect("decode");
        assert_eq!(back, e, "round-trip");
    }

    #[test]
    fn composite_exprs_round_trip() {
        // The shapes reductions embed in node fields: a variable read
        // (param call), a mixed interpolation, a capture.
        rt(IrExpr::Call {
            func: "param".to_string(),
            args: vec![
                IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
                IrExpr::Str("s".to_string(), StrStyle::DoubleQuoted),
            ],
        });
        rt(IrExpr::Interpolate(vec![
            InterpPart::Lit("a ".to_string()),
            InterpPart::Expr(Box::new(IrExpr::Var("x".to_string(), None))),
            InterpPart::Lit("\n".to_string()),
        ]));
        rt(IrExpr::Capture {
            expr: Box::new(IrExpr::Str("pwd".to_string(), StrStyle::DoubleQuoted)),
            native: false,
        });
        // A capture body: Capture(Arrow([Expr(exec pwd)])) — the shape
        // `basename "$(pwd)"` embeds in PathName.text (999_pwd.sh).
        rt(IrExpr::Capture {
            expr: Box::new(IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![IrExpr::Str("pwd".to_string(), StrStyle::DoubleQuoted)],
            })])),
            native: false,
        });
    }
}
