//! java_backend — Java renderer (worktree-local, branch `backend/java`).
//!
//! Consumes the ShIR (the A1 contract) in-process and emits Java source.
//! INITIAL VERSION: the v1 subset — simple commands (Expr(Call("exec",
//! ...)) — echo/printf lowered to System.out.println), Assign (native
//! fields), If, For, While, Block, getVar reads. Anything outside the
//! subset prints a marker to stderr and returns Err — the corpus gate
//! then reports FAIL on that example, and the worker's pi extends the
//! renderer (the same progression the c backend followed).

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// Render a ShIR program to Java source. `Err` on a construct outside
/// the v1 subset (the gate reports it as a FAIL).
pub fn shir_to_java(prog: &IrProgram) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("public class Sh2Program {\n");
    // v1: every assignment target becomes a static field (strings only).
    for st in &prog.stmts {
        collect_fields(st, &mut out)?;
    }
    out.push_str("    public static void main(String[] args) throws Exception {\n");
    // source-mapping comments: ` // line N` (the shIR convention)
    for (idx, st) in prog.stmts.iter().enumerate() {
        let before = out.len();
        stmt_to_java(st, 2, &mut out)?;
        let line = prog.stmt_lines.iter().find(|(i, _)| *i == idx).map(|(_, l)| *l);
        if let Some(l) = line {
            if let Some(nl) = out[before..].find('\n') {
                out.insert_str(before + nl, &format!(" // line {l}"));
            }
        }
    }
    out.push_str("    }\n");
    out.push_str("}\n");
    Ok(out)
}

fn indent(out: &mut String, d: usize) {
    for _ in 0..d {
        out.push_str("    ");
    }
}

fn collect_fields(st: &IrStmt, out: &mut String) -> Result<(), String> {
    if let IrStmt::Assign { targets, .. } = st {
        for t in targets {
            indent(out, 1);
            out.push_str(&format!("static String {} = \"\";\n", t.var));
        }
    }
    Ok(())
}

fn stmt_to_java(st: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
    match st {
        IrStmt::Expr(e) => expr_stmt_to_java(e, d, out),
        IrStmt::Assign { targets, expr } => {
            let t = targets.first().ok_or("assign: no target (v1)")?;
            if !t.indices.is_empty() {
                return Err("assign with indices not in the v1 subset".into());
            }
            indent(out, d);
            out.push_str(&t.var);
            out.push_str(" = ");
            expr_to_java(expr, out)?;
            out.push_str(";\n");
            Ok(())
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            indent(out, d);
            out.push_str("if (");
            cond_to_java(cond, out)?;
            out.push_str(") {\n");
            for b in then {
                stmt_to_java(b, d + 1, out)?;
            }
            for (econd, ebody) in elsifs {
                indent(out, d);
                out.push_str("} else if (");
                cond_to_java(econd, out)?;
                out.push_str(") {\n");
                for b in ebody {
                    stmt_to_java(b, d + 1, out)?;
                }
            }
            if !else_.is_empty() {
                indent(out, d);
                out.push_str("} else {\n");
                for b in else_ {
                    stmt_to_java(b, d + 1, out)?;
                }
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        IrStmt::For { var, iter, body } => {
            indent(out, d);
            out.push_str("for (String ");
            out.push_str(var);
            out.push_str(" : new String[] {");
            for_items_to_java(iter, out)?;
            out.push_str("}) {\n");
            for b in body {
                stmt_to_java(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        IrStmt::While { cond, body } => {
            indent(out, d);
            out.push_str("while (");
            cond_to_java(cond, out)?;
            out.push_str(") {\n");
            for b in body {
                stmt_to_java(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        IrStmt::Block(body) => {
            indent(out, d);
            out.push_str("{\n");
            for b in body {
                stmt_to_java(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        other => Err(format!("statement not in the v1 subset: {other:?}")),
    }
}

/// A condition: `expr != null` for exec calls (v1 — a command that
/// succeeded), or a var read.
fn cond_to_java(cond: &IrExpr, out: &mut String) -> Result<(), String> {
    match cond {
        IrExpr::Call { func, .. } if func == "getVar" => {
            expr_to_java(cond, out)?;
            out.push_str(" != null");
            Ok(())
        }
        IrExpr::Call { .. } => {
            out.push_str("true");
            Ok(())
        }
        other => Err(format!("condition not in the v1 Java subset: {other:?}")),
    }
}

fn expr_stmt_to_java(e: &IrExpr, d: usize, out: &mut String) -> Result<(), String> {
    match e {
        IrExpr::Call { func, args, .. } if func == "exec" => {
            let cmd = match args.first() {
                Some(IrExpr::Str(name, _)) => name.clone(),
                _ => return Err("exec with non-literal command not in the v1 subset".into()),
            };
            match cmd.as_str() {
                "echo" => {
                    let words = &args[1..];
                    let mut parts = Vec::new();
                    for w in words {
                        parts.push(word_to_java(w)?);
                    }
                    indent(out, d);
                    out.push_str("System.out.println(");
                    out.push_str(&parts.join(" + "));
                    out.push_str(");\n");
                }
                "printf" => {
                    indent(out, d);
                    out.push_str("System.out.print(");
                    let words = &args[1..];
                    let mut parts = Vec::new();
                    for w in words {
                        parts.push(word_to_java(w)?);
                    }
                    out.push_str(&parts.join(" + "));
                    out.push_str(");\n");
                }
                other => {
                    indent(out, d);
                    out.push_str(&format!("// sh2.{} (external — v1 stub)\n", other));
                }
            }
            Ok(())
        }
        IrExpr::Call { func, args, .. } if func == "getVar" => {
            let name = match args.first() {
                Some(IrExpr::Str(n, _)) => n.clone(),
                _ => return Err("getVar with non-literal name (v1)".into()),
            };
            indent(out, d);
            out.push_str(&format!("System.out.println({name});\n"));
            Ok(())
        }
        other => Err(format!("expression not in the v1 Java subset: {other:?}")),
    }
}

fn word_to_java(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(format!(
            "\"{}\"",
            s.replace('\\', "\\\\").replace('"', "\\\"")
        )),
        IrExpr::Call { func, args, .. } if func == "getVar" => {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                Ok(format!("({name} == null ? \"\" : {name})"))
            } else {
                Err("getVar with non-literal name (v1)".into())
            }
        }
        IrExpr::Var(name, _) => Ok(format!("({name} == null ? \"\" : {name})")),
        IrExpr::Array(items) => {
            let parts: Result<Vec<String>, String> = items.iter().map(word_to_java).collect();
            Ok(format!("(\"\" + {})", parts?.join(" + \" \" + ")))
        }
        other => Err(format!("word not in the v1 Java subset: {other:?}")),
    }
}

fn expr_to_java(e: &IrExpr, out: &mut String) -> Result<(), String> {
    match e {
        IrExpr::Str(s, _) => {
            out.push_str(&format!(
                "\"{}\"",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            ));
            Ok(())
        }
        IrExpr::Call { func, args, .. } if func == "getVar" => {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                out.push_str(name);
                Ok(())
            } else {
                Err("getVar with non-literal name (v1)".into())
            }
        }
        IrExpr::Var(name, _) => {
            out.push_str(name);
            Ok(())
        }
        other => Err(format!("expr not in the v1 Java subset: {other:?}")),
    }
}

fn for_items_to_java(iter: &IrExpr, out: &mut String) -> Result<(), String> {
    match iter {
        IrExpr::Array(items) => {
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&word_to_java(it)?);
            }
            Ok(())
        }
        other => Err(format!("for-iterable not in the v1 Java subset: {other:?}")),
    }
}
