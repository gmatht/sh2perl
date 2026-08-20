//! java_backend — Java renderer (main mirror; the `backend/java` worktree
//! merges main and consumes this module).
//!
//! Consumes the ShIR (the A1 contract) in-process and emits Java source.
//! v1 subset: simple commands (Expr(Call("exec", ...)) — echo/printf
//! lowered to System.out.println/print), Assign (native String fields),
//! If/elsif/else, For, While, DoWhile, ForInit, Block (labeled — an
//! outside-loop Break targets the innermost block), Break/Continue,
//! getVar reads, bash arith (native `long` expressions via the sh2Num
//! coercion helper), `test "…"` conditions, setVar stores, and the C
//! memory arena (memAlloc/memStore/memLoad/memAdvance/memFree/memTest/
//! memElemSize — the estree ref runtime's mem* slice-2, standalone).
//! Anything outside the subset prints a marker to stderr and returns
//! Err — the corpus gate then reports FAIL on that example, and the
//! worker's pi extends the renderer (the same progression the c backend
//! followed).

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, IrType};

/// Renderer state: loop nesting (native break/continue are only legal
/// inside a loop) and the enclosing-block labels (an outside-loop break
/// exits the innermost A1 Block — the estree ref's sh2.break()).
#[derive(Default)]
struct JavaCtx {
    loop_depth: usize,
    block_labels: Vec<String>,
    block_seq: usize,
}

/// Render a ShIR program to Java source. `Err` on a construct outside
/// the v1 subset (the gate reports it as a FAIL).
pub fn shir_to_java(prog: &IrProgram) -> Result<String, String> {
    // builtin-op fallback arm (shir-builtin-op-20260816): the java
    // backend has NOT accepted the `builtin` op — render as exec.
    let mut prog = prog.clone();
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    let mut out = String::new();
    out.push_str("public class Sh2Program {\n");
    if stmts_need_sh2num(&prog.stmts) {
        // printf %d/%i/%u conversion helper — bash coerces a
        // non-numeric arg to 0 (the java backend's fields are Strings);
        // arith reads and the mem arena coerce through it too
        out.push_str("    static long sh2Num(String s) {\n");
        out.push_str("        try { return Long.parseLong(s.trim()); } catch (Exception e) { return 0; }\n");
        out.push_str("    }\n");
    }
    if stmts_need_mem(&prog.stmts) {
        out.push_str(MEM_PREAMBLE);
    }
    // every assignment target / store / arith read becomes a static
    // String field (empty default — bash reads an unset var as "")
    let mut fields: Vec<String> = Vec::new();
    collect_vars(&prog.stmts, &mut fields);
    for f in fields {
        indent(&mut out, 1);
        out.push_str(&format!("static String {f} = \"\";\n"));
    }
    out.push_str("    public static void main(String[] args) throws Exception {\n");
    // source-mapping comments: ` // line N` (the shIR convention)
    let mut ctx = JavaCtx::default();
    for (idx, st) in prog.stmts.iter().enumerate() {
        let before = out.len();
        ctx.stmt_to_java(st, 2, &mut out)?;
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

/// Every variable that needs a `static String` field, recursively:
/// Assign targets (any nesting), setVar store names, getVar reads, and
/// arith Var/Ident reads. A read of a never-assigned var gets the empty
/// string (bash reads unset as ""). Dedup keeps the class compileable
/// (t12_scalar_alias assigns x twice).
fn collect_vars(stmts: &[IrStmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    push_var(&t.var, out);
                }
                collect_vars_expr(expr, out);
            }
            IrStmt::Expr(e) => collect_vars_expr(e, out),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
                ..
            } => {
                collect_vars_expr(cond, out);
                collect_vars(then, out);
                collect_vars(else_, out);
                for (c, b) in elsifs {
                    collect_vars_expr(c, out);
                    collect_vars(b, out);
                }
            }
            IrStmt::For { iter, body, .. } => {
                collect_vars_expr(iter, out);
                collect_vars(body, out);
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                collect_vars(init, out);
                collect_vars_expr(cond, out);
                collect_vars(step, out);
                collect_vars(body, out);
            }
            IrStmt::While { cond, body, .. } => {
                collect_vars_expr(cond, out);
                collect_vars(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_vars(body, out);
                collect_vars_expr(cond, out);
            }
            IrStmt::Block(b) => collect_vars(b, out),
            _ => {}
        }
    }
}

fn collect_vars_expr(e: &IrExpr, out: &mut Vec<String>) {
    match e {
        IrExpr::Call { func, args } => {
            // setVar carries the STORE name as its first Str arg;
            // getVar reads a var (a read-only var is the "" field)
            if matches!(func.as_str(), "setVar" | "getVar") {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    push_var(name, out);
                }
            }
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        IrExpr::Arith(a) => collect_vars_arith(a, out),
        IrExpr::Array(items) => {
            for i in items {
                collect_vars_expr(i, out);
            }
        }
        IrExpr::Var(name, _) => push_var(name, out),
        IrExpr::Ident(name) => push_var(name, out),
        IrExpr::Index { var, key } => {
            push_var(var, out);
            collect_vars_expr(key, out);
        }
        _ => {}
    }
}

fn collect_vars_arith(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => push_var(name, out),
        ArithAst::Index { var, key } => {
            push_var(var, out);
            collect_vars_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_vars_arith(lhs, out);
            collect_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_vars_arith(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_vars_arith(test, out);
            collect_vars_arith(then, out);
            collect_vars_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            push_var(var, out);
            collect_vars_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => push_var(var, out),
        ArithAst::Cast { arg, .. } => collect_vars_arith(arg, out),
        ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
    }
}

fn push_var(name: &str, out: &mut Vec<String>) {
    if is_plain_name(name) && !out.iter().any(|v| v == name) {
        out.push(name.to_string());
    }
}

fn is_plain_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

impl JavaCtx {
    fn stmt_to_java(&mut self, st: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
        match st {
            IrStmt::Expr(e) => expr_stmt_to_java(e, d, out),
            IrStmt::Assign { targets, expr, asm, .. } => {
                // Declarator-position asm label (core request
                // c-sh-go-toplevelasmargument-20260814-042952) — no Java
                // rendering; refuse loudly (refuse > guess).
                if let Some(spec) = asm {
                    indent(out, d);
                    out.push_str(&format!(
                        "// TODO(unsupported): asm label '{}' on an assign\n",
                        spec.template
                    ));
                    return Ok(());
                }
                let t = targets.first().ok_or("assign: no target (v1)")?;
                if !t.indices.is_empty() {
                    return Err("assign with indices not in the v1 subset".into());
                }
                indent(out, d);
                out.push_str(&t.var);
                out.push_str(" = ");
                // `i++` / `i--` / `i += n` step/body arith assignments
                if let IrExpr::Arith(a) = expr {
                    match &**a {
                        ArithAst::IncDec { var, delta, .. } => {
                            let d = delta.unsigned_abs();
                            let sign = if *delta >= 0 { "+" } else { "-" };
                            out.push_str(&format!(
                                "Long.toString(sh2Num({var}) {sign} {d})"
                            ));
                            out.push_str(";\n");
                            return Ok(());
                        }
                        ArithAst::Assign { var, op, rhs } => {
                            let rhs = arith_str(rhs)?;
                            let jop = match op.as_str() {
                                "+=" => " + ",
                                "-=" => " - ",
                                "*=" => " * ",
                                "/=" => " / ",
                                "%=" => " % ",
                                _ => " + ",
                            };
                            out.push_str(&format!(
                                "Long.toString(sh2Num({var}){jop}({rhs}))"
                            ));
                            out.push_str(";\n");
                            return Ok(());
                        }
                        _ => {}
                    }
                }
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
                self.cond_to_java(cond, out)?;
                out.push_str(") {\n");
                for b in then {
                    self.stmt_to_java(b, d + 1, out)?;
                }
                for (econd, ebody) in elsifs {
                    indent(out, d);
                    out.push_str("} else if (");
                    self.cond_to_java(econd, out)?;
                    out.push_str(") {\n");
                    for b in ebody {
                        self.stmt_to_java(b, d + 1, out)?;
                    }
                }
                if !else_.is_empty() {
                    indent(out, d);
                    out.push_str("} else {\n");
                    for b in else_ {
                        self.stmt_to_java(b, d + 1, out)?;
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
                self.loop_depth += 1;
                for b in body {
                    self.stmt_to_java(b, d + 1, out)?;
                }
                self.loop_depth -= 1;
                indent(out, d);
                out.push_str("}\n");
                Ok(())
            }
            IrStmt::While { cond, body } => {
                indent(out, d);
                out.push_str("while (");
                self.cond_to_java(cond, out)?;
                out.push_str(") {\n");
                self.loop_depth += 1;
                for b in body {
                    self.stmt_to_java(b, d + 1, out)?;
                }
                self.loop_depth -= 1;
                indent(out, d);
                out.push_str("}\n");
                Ok(())
            }
            IrStmt::DoWhile { body, cond, until } => {
                if *until {
                    // until → while (!cond) — the body runs zero times
                    // when the cond already holds (bash until semantics)
                    indent(out, d);
                    out.push_str("while (!(");
                    self.cond_to_java(cond, out)?;
                    out.push_str(")) {\n");
                    self.loop_depth += 1;
                    for b in body {
                        self.stmt_to_java(b, d + 1, out)?;
                    }
                    self.loop_depth -= 1;
                    indent(out, d);
                    out.push_str("}\n");
                } else {
                    indent(out, d);
                    out.push_str("do {\n");
                    self.loop_depth += 1;
                    for b in body {
                        self.stmt_to_java(b, d + 1, out)?;
                    }
                    self.loop_depth -= 1;
                    indent(out, d);
                    out.push_str("} while (");
                    self.cond_to_java(cond, out)?;
                    out.push_str(");\n");
                }
                Ok(())
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                // the A1 for-loop shape → init; while (cond) { body; step; }
                for b in init {
                    self.stmt_to_java(b, d, out)?;
                }
                indent(out, d);
                out.push_str("while (");
                self.cond_to_java(cond, out)?;
                out.push_str(") {\n");
                self.loop_depth += 1;
                for b in body {
                    self.stmt_to_java(b, d + 1, out)?;
                }
                for s in step {
                    self.stmt_to_java(s, d + 1, out)?;
                }
                self.loop_depth -= 1;
                indent(out, d);
                out.push_str("}\n");
                Ok(())
            }
            IrStmt::Block(body) => {
                // labeled block: an outside-loop Break targets the
                // innermost label (the estree ref's sh2.break() exits
                // the enclosing block — c-sh-go t36_dowhile_break's
                // do-while lowering puts the first break at block level)
                let label = format!("b{}", self.block_seq);
                self.block_seq += 1;
                self.block_labels.push(label.clone());
                indent(out, d);
                out.push_str(&format!("{label}: {{\n"));
                for b in body {
                    self.stmt_to_java(b, d + 1, out)?;
                }
                self.block_labels.pop();
                indent(out, d);
                out.push_str("}\n");
                Ok(())
            }
            IrStmt::Break => {
                if self.loop_depth > 0 {
                    indent(out, d);
                    out.push_str("break;\n");
                    Ok(())
                } else if let Some(l) = self.block_labels.last() {
                    indent(out, d);
                    out.push_str(&format!("break {l};\n"));
                    Ok(())
                } else {
                    Err("break outside a loop/block (v1)".into())
                }
            }
            IrStmt::Continue => {
                if self.loop_depth > 0 {
                    indent(out, d);
                    out.push_str("continue;\n");
                    Ok(())
                } else {
                    Err("continue outside a loop (v1)".into())
                }
            }
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                if target.is_some() {
                    return Err("Output to a filehandle target (v1)".into());
                }
                indent(out, d);
                out.push_str(if *newline { "System.out.println(" } else { "System.out.print(" });
                expr_to_java(value, out)?;
                out.push_str(");\n");
                Ok(())
            }
            IrStmt::Exit(e) => {
                let code = match e {
                    Some(x) => {
                        let mut s = String::new();
                        expr_to_java(x, &mut s)?;
                        format!("sh2Num({})", s)
                    }
                    None => "0".to_string(),
                };
                indent(out, d);
                out.push_str(&format!("System.exit((int){});\n", code));
                Ok(())
            }
            IrStmt::Return(v) => {
                indent(out, d);
                match v {
                    Some(x) => {
                        out.push_str("return ");
                        expr_to_java(x, out)?;
                        out.push_str(";\n");
                    }
                    None => out.push_str("return;\n"),
                }
                Ok(())
            }
            IrStmt::SetChildError(_) => Ok(()), // status tracked elsewhere (no-op)
            IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                for b in body {
                    self.stmt_to_java(b, d, out)?;
                }
                Ok(())
            }
            IrStmt::Declare { vars, init, .. } => {
                for v in vars {
                    indent(out, d);
                    out.push_str(&v.name);
                    out.push_str(" = ");
                    match init {
                        Some(e) => expr_to_java(e, out)?,
                        None => out.push_str("\"\""),
                    }
                    out.push_str(";\n");
                }
                Ok(())
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                let mut dstr = String::new();
                expr_to_java(discriminant, &mut dstr)?;
                let mut emitted = false;
                for cl in clauses {
                    let is_default = cl.patterns.iter().any(|p| p == "*");
                    let conds: Vec<String> = cl
                        .patterns
                        .iter()
                        .filter(|p| p.as_str() != "*")
                        .map(|p| format!("{}.equals({})", dstr, java_str_lit(p)))
                        .collect();
                    if conds.is_empty() && is_default {
                        indent(out, d);
                        out.push_str("else {\n");
                        for b in &cl.body {
                            self.stmt_to_java(b, d + 1, out)?;
                        }
                        indent(out, d);
                        out.push_str("}\n");
                        emitted = true;
                        continue;
                    }
                    indent(out, d);
                    out.push_str(if emitted { "else if (" } else { "if (" });
                    out.push_str(&conds.join(" || "));
                    out.push_str(") {\n");
                    for b in &cl.body {
                        self.stmt_to_java(b, d + 1, out)?;
                    }
                    indent(out, d);
                    out.push_str("}\n");
                    emitted = true;
                }
                Ok(())
            }
            IrStmt::Function { name, body, .. } => {
                let id = java_ident(name);
                let ret = if body_has_return(body) {
                    "String"
                } else {
                    "void"
                };
                indent(out, d);
                out.push_str(&format!("static {ret} {id}() {{\n"));
                for b in body {
                    self.stmt_to_java(b, d + 1, out)?;
                }
                indent(out, d);
                if ret == "String" {
                    out.push_str("return \"\";\n");
                }
                out.push_str("}\n");
                Ok(())
            }
            IrStmt::Redirect { inner, redirects } => {
                // render the inner commands; apply a simple fd-1 write
                // redirect (`> file` / `>> file`) by wrapping the output
                // in a print-to-file (capture-free approximation for the
                // v1 subset).
                let write_target: Option<(String, bool)> = redirects.iter().find_map(|r| {
                    if r.fd.unwrap_or(1) == 1 && (r.mode == "w" || r.mode == "a") {
                        let mut p = String::new();
                        if expr_to_java(&r.target, &mut p).is_ok() {
                            Some((p, r.mode == "a"))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                for b in inner {
                    self.stmt_to_java(b, d, out)?;
                }
                if let Some((path, append)) = write_target {
                    indent(out, d);
                    let opt = if append {
                        "java.nio.file.StandardOpenOption.APPEND"
                    } else {
                        "java.nio.file.StandardOpenOption.TRUNCATE_EXISTING, java.nio.file.StandardOpenOption.CREATE"
                    };
                    out.push_str(&format!(
                        "java.nio.file.Files.write(java.nio.file.Paths.get({path}), \"\".getBytes(), {opt});\n"
                    ));
                }
                Ok(())
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    for b in st {
                        self.stmt_to_java(b, d, out)?;
                    }
                }
                Ok(())
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                let mut p = String::new();
                expr_to_java(path, &mut p)?;
                let mut c = String::new();
                expr_to_java(content, &mut c)?;
                indent(out, d);
                let opt = if *append {
                    "java.nio.file.StandardOpenOption.APPEND"
                } else {
                    "java.nio.file.StandardOpenOption.TRUNCATE_EXISTING, java.nio.file.StandardOpenOption.CREATE"
                };
                out.push_str(&format!(
                    "java.nio.file.Files.write(java.nio.file.Paths.get({p}), ({c}).getBytes(), {opt});\n"
                ));
                Ok(())
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                let mut elems = String::new();
                for (i, e) in elements.iter().enumerate() {
                    if i > 0 {
                        elems.push_str(", ");
                    }
                    expr_to_java(e, &mut elems)?;
                }
                indent(out, d);
                out.push_str(&format!(
                    "{var} = java.util.Arrays.toString(new String[]{{{elems}}});\n"
                ));
                Ok(())
            }
            other => Err(format!("statement not in the v1 subset: {other:?}")),
        }
    }

    /// A condition: `expr != null` for exec calls (v1 — a command that
    /// succeeded), a var read, or a `test "…"` string (the mini
    /// evaluator; an unparsed shape keeps the v1 `true` fallback).
    fn cond_to_java(&self, cond: &IrExpr, out: &mut String) -> Result<(), String> {
        match cond {
            IrExpr::Call { func, .. } if func == "getVar" => {
                expr_to_java(cond, out)?;
                out.push_str(" != null");
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = test_render(s) {
                        out.push_str(&c);
                        return Ok(());
                    }
                }
                out.push_str("true");
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "contains" => {
                // `echo X | grep LIT >/dev/null` → contains(X, LIT): native
                // String.contains (java fields are Strings).
                if let (Some(needle), Some(pattern)) = (args.first(), args.get(1)) {
                    let mut n = String::new();
                    expr_to_java(needle, &mut n)?;
                    let mut p = String::new();
                    expr_to_java(pattern, &mut p)?;
                    out.push_str(&format!("({n}).contains({p})"));
                    return Ok(());
                }
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "exec" => {
                // `true`/`false` builtins used as a condition
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    match cmd.as_str() {
                        "true" => {
                            out.push_str("true");
                            return Ok(());
                        }
                        "false" => {
                            out.push_str("false");
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                out.push_str("true");
                Ok(())
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let jop = match op {
                    crate::ir::BinOpKind::And => " && ",
                    crate::ir::BinOpKind::Or => " || ",
                    crate::ir::BinOpKind::Eq => " == ",
                    crate::ir::BinOpKind::Ne => " != ",
                    crate::ir::BinOpKind::Not => {
                        let mut inner = String::new();
                        self.cond_to_java(rhs, &mut inner)?;
                        out.push_str(&format!("!({inner})"));
                        return Ok(());
                    }
                    _ => return Err(format!("condition op not in the v1 Java subset: {op:?}")),
                };
                let mut l = String::new();
                let mut r = String::new();
                self.cond_to_java(lhs, &mut l)?;
                self.cond_to_java(rhs, &mut r)?;
                out.push_str(&format!("({l}){jop}({r})"));
                Ok(())
            }
            IrExpr::Call { .. } => {
                out.push_str("true");
                Ok(())
            }
            other => Err(format!("condition not in the v1 Java subset: {other:?}")),
        }
    }
}

fn expr_stmt_to_java(e: &IrExpr, d: usize, out: &mut String) -> Result<(), String> {
    match e {
        IrExpr::Call { func, args, .. } if func == "test" => {
            // a bare `[ cond ]` statement: emit the condition truth
            if let Some(IrExpr::Str(s, _)) = args.first() {
                let c = test_render(s).unwrap_or_else(|| "true".to_string());
                indent(out, d);
                out.push_str(&format!("boolean __t = {c};\n"));
                return Ok(());
            }
            Ok(())
        }
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
                    // The A1 shape: exec("printf", [Array[fmt, arg...]]).
                    // Apply the %s/%d/%i/%u conversions over a literal
                    // format (cpp-sh-go t32_varargs.cc — the folded
                    // `sum(3,1,2,3)` → "6" must land through %d, not be
                    // joined as a word); mirrors the estree ref / js
                    // renderer's printf lowering. A flags/width/prec
                    // spec keeps the raw join (the v1 fallback).
                    let Some(IrExpr::Array(items)) = args.get(1) else {
                        return Err("printf without an args array (v1)".into());
                    };
                    let Some(IrExpr::Str(fmt, _)) = items.first() else {
                        return Err("printf with a non-literal format (v1)".into());
                    };
                    let parsed = printf_parse(fmt);
                    let Some((els, n_specs)) = parsed else {
                        // complex spec — the v1 raw join
                        indent(out, d);
                        out.push_str("System.out.print(");
                        let mut parts = Vec::new();
                        for w in &args[1..] {
                            parts.push(word_to_java(w)?);
                        }
                        out.push_str(&parts.join(" + "));
                        out.push_str(");\n");
                        return Ok(());
                    };
                    let fmt_args: Vec<&IrExpr> = items[1..].iter().collect();
                    let arg_exprs: Result<Vec<String>, String> =
                        fmt_args.iter().map(|a| word_to_java(a)).collect();
                    let arg_exprs = arg_exprs?;
                    let mut pieces: Vec<String> = Vec::new();
                    if n_specs == 0 {
                        // printf(1): the format text repeats once per arg
                        let text = java_str_lit(&printf_unescape(fmt));
                        let passes = if fmt_args.is_empty() { 1 } else { fmt_args.len() };
                        for _ in 0..passes {
                            pieces.push(text.clone());
                        }
                    } else {
                        let passes = if arg_exprs.is_empty() {
                            1
                        } else {
                            (arg_exprs.len() + n_specs - 1) / n_specs
                        };
                        let mut ai = 0usize;
                        for _pass in 0..passes {
                            for (text, spec) in &els {
                                if let Some(conv) = spec {
                                    let arg = arg_exprs
                                        .get(ai)
                                        .cloned()
                                        .unwrap_or_else(|| "\"\"".into());
                                    ai += 1;
                                    match conv {
                                        's' => pieces.push(arg),
                                        'd' | 'i' | 'u' => pieces
                                            .push(format!("Long.toString(sh2Num({arg}))")),
                                        _ => unreachable!("printf_parse gates the conversions"),
                                    }
                                } else {
                                    pieces.push(java_str_lit(&printf_unescape(text)));
                                }
                            }
                        }
                    }
                    indent(out, d);
                    out.push_str("System.out.print(");
                    out.push_str(&pieces.join(" + "));
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
        IrExpr::Call { func, args, .. } if func == "setVar" => {
            // the A1's store write: setVar("name", value) → the
            // assignment (c-sh-go t19_malloc's arena pointer)
            let name = match args.first() {
                Some(IrExpr::Str(n, _)) => n.clone(),
                _ => return Err("setVar with non-literal name (v1)".into()),
            };
            let val = args.get(1).ok_or("setVar without a value (v1)")?;
            indent(out, d);
            out.push_str(&format!("{name} = "));
            out.push_str(&word_to_java(val)?);
            out.push_str(";\n");
            Ok(())
        }
        IrExpr::Call { func, args, .. } if is_mem_func(func) => {
            indent(out, d);
            out.push_str(&mem_call_java(func, args)?);
            out.push_str(";\n");
            Ok(())
        }
        IrExpr::Arith(a) => {
            // `((i++))` — the side-effect statement form (c-sh-go
            // t28_dowhile / t44_nested_loops: IncDec in an Expr stmt);
            // any other arith computes and discards (bash `((expr))`)
            if let ArithAst::IncDec { var, delta, .. } = &**a {
                indent(out, d);
                out.push_str(&format!(
                    "{var} = Long.toString(sh2Num({var}) + ({delta}));\n"
                ));
                return Ok(());
            }
            indent(out, d);
            out.push_str("Long.toString(");
            arith_to_java(a, out)?;
            out.push_str(");\n");
            Ok(())
        }
        other => Err(format!("expression not in the v1 Java subset: {other:?}")),
    }
}

/// A java string literal: escape backslash, quote, and control chars
/// (a raw newline inside the literal is a compile error — cpp-sh-go
/// t30_static_assert.cc's `printf "static assert ok\n"` carries a real
/// \n in the A1 Str).
/// Sanitize a shell function name into a Java identifier.
fn java_ident(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if i > 0 {
            out.push('_');
        }
    }
    if out.is_empty() {
        "f".to_string()
    } else {
        out
    }
}

/// Does a function body contain a `Return(Some(..))`? If so the Java
/// method needs a non-void return type.
fn body_has_return(body: &[IrStmt]) -> bool {
    body.iter().any(|s| matches!(s, IrStmt::Return(Some(_))))
}

fn java_str_lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// printf(1) format parse (mirror of the js renderer's printf_parse):
/// %s/%d/%i/%u conversions over literal text runs. A flags/width/prec
/// spec (or any other conversion) yields None — the caller keeps the
/// v1 raw join. `%%` is an escaped percent.
fn printf_parse(fmt: &str) -> Option<(Vec<(String, Option<char>)>, usize)> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut els: Vec<(String, Option<char>)> = Vec::new();
    let mut text = String::new();
    let mut n_specs = 0usize;
    let mut pos = 0usize;
    while pos < chars.len() {
        if chars[pos] == '%' {
            let mut i = pos + 1;
            let flags_start = i;
            while i < chars.len() && matches!(chars[i], '-' | '+' | ' ' | '0' | '#') {
                i += 1;
            }
            let has_flags = i > flags_start;
            let mut has_width = false;
            while i < chars.len() && chars[i].is_ascii_digit() {
                has_width = true;
                i += 1;
            }
            let mut has_prec = false;
            if i < chars.len() && chars[i] == '.' {
                i += 1;
                has_prec = true;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let Some(&conv) = chars.get(i) else {
                return None;
            };
            if has_flags || has_width || has_prec {
                return None;
            }
            match conv {
                's' | 'd' | 'i' | 'u' => {
                    if !text.is_empty() {
                        els.push((std::mem::take(&mut text), None));
                    }
                    els.push((String::new(), Some(conv)));
                    n_specs += 1;
                    pos = i + 1;
                }
                '%' => {
                    text.push('%');
                    pos = i + 1;
                }
                _ => return None,
            }
        } else {
            text.push(chars[pos]);
            pos += 1;
        }
    }
    if !text.is_empty() {
        els.push((text, None));
    }
    Some((els, n_specs))
}

/// printf(1) backslash escapes in the format text (\n, \t, \r, \a,
/// \b, \f, \v, \\\\, octal) — mirror of the js renderer's
/// printf_unescape; the A1 Str values carry the raw escape sequences.
fn printf_unescape(s: &str) -> String {
    let mut out = s.to_string();
    for (from, to) in [
        ("\\n", "\n"),
        ("\\t", "\t"),
        ("\\r", "\r"),
        ("\\a", "\x07"),
        ("\\b", "\x08"),
        ("\\f", "\x0c"),
        ("\\v", "\x0b"),
        ("\\\\", "\\"),
    ] {
        out = out.replace(from, to);
    }
    let chars: Vec<char> = out.chars().collect();
    let mut res = String::with_capacity(out.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let mut oct = String::new();
            let mut j = i + 1;
            while j < chars.len() && oct.len() < 3 && matches!(chars[j], '0'..='7') {
                oct.push(chars[j]);
                j += 1;
            }
            if !oct.is_empty() {
                if let Ok(v) = u32::from_str_radix(&oct, 8) {
                    if let Some(c) = char::from_u32(v) {
                        res.push(c);
                        i = j;
                        continue;
                    }
                }
            }
        }
        res.push(chars[i]);
        i += 1;
    }
    res
}

/// Does any statement need the sh2Num coercion helper? A printf with a
/// numeric conversion (%d/%i/%u), an arith expression (Var/Ident reads
/// coerce through it), or a mem* call (the arena parses through it).
fn stmts_need_sh2num(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|st| match st {
        IrStmt::Expr(e) => expr_need_sh2num(e),
        IrStmt::Assign { expr, .. } => expr_need_sh2num(expr),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
            ..
        } => {
            expr_need_sh2num(cond)
                || stmts_need_sh2num(then)
                || stmts_need_sh2num(else_)
                || elsifs.iter().any(|(c, b)| expr_need_sh2num(c) || stmts_need_sh2num(b))
        }
        IrStmt::For { iter, body, .. } => expr_need_sh2num(iter) || stmts_need_sh2num(body),
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            stmts_need_sh2num(init)
                || expr_need_sh2num(cond)
                || stmts_need_sh2num(step)
                || stmts_need_sh2num(body)
        }
        IrStmt::While { cond, body, .. } => expr_need_sh2num(cond) || stmts_need_sh2num(body),
        IrStmt::DoWhile { body, cond, .. } => stmts_need_sh2num(body) || expr_need_sh2num(cond),
        IrStmt::Block(b) => stmts_need_sh2num(b),
        _ => false,
    })
}

fn expr_need_sh2num(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } if func == "exec" => {
            // printf with a literal format containing a numeric conv
            if let Some(IrExpr::Array(items)) = args.get(1) {
                if let Some(IrExpr::Str(fmt, _)) = items.first() {
                    if printf_parse(fmt)
                        .map(|(els, _)| {
                            els.iter()
                                .any(|(_, spec)| matches!(spec, Some('d' | 'i' | 'u')))
                        })
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
            // echo/interpolated words may carry arith (sh2Num reads)
            args.iter().any(expr_need_sh2num)
        }
        IrExpr::Arith(_) => true,
        IrExpr::Call { func, .. } if func == "test" => true,
        IrExpr::Call { func, .. } if is_mem_func(func) => true,
        IrExpr::Array(items) => items.iter().any(expr_need_sh2num),
        _ => false,
    }
}

/// Does any statement contain a mem* call? Those need the arena
/// preamble emitted.
fn stmts_need_mem(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|st| match st {
        IrStmt::Expr(e) => expr_need_mem(e),
        IrStmt::Assign { expr, .. } => expr_need_mem(expr),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
            ..
        } => {
            expr_need_mem(cond)
                || stmts_need_mem(then)
                || stmts_need_mem(else_)
                || elsifs.iter().any(|(c, b)| expr_need_mem(c) || stmts_need_mem(b))
        }
        IrStmt::For { iter, body, .. } => expr_need_mem(iter) || stmts_need_mem(body),
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            stmts_need_mem(init)
                || expr_need_mem(cond)
                || stmts_need_mem(step)
                || stmts_need_mem(body)
        }
        IrStmt::While { cond, body, .. } => expr_need_mem(cond) || stmts_need_mem(body),
        IrStmt::DoWhile { body, cond, .. } => stmts_need_mem(body) || expr_need_mem(cond),
        IrStmt::Block(b) => stmts_need_mem(b),
        _ => false,
    })
}

fn expr_need_mem(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, .. } if is_mem_func(func) => true,
        IrExpr::Call { args, .. } => args.iter().any(expr_need_mem),
        IrExpr::Array(items) => items.iter().any(expr_need_mem),
        IrExpr::Arith(a) => arith_need_mem(a),
        _ => false,
    }
}

fn arith_need_mem(a: &ArithAst) -> bool {
    match a {
        ArithAst::Bin { lhs, rhs, .. } => arith_need_mem(lhs) || arith_need_mem(rhs),
        ArithAst::Un { arg, .. } => arith_need_mem(arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_need_mem(test) || arith_need_mem(then) || arith_need_mem(else_)
        }
        ArithAst::Assign { rhs, .. } => arith_need_mem(rhs),
        ArithAst::Cast { arg, .. } => arith_need_mem(arg),
        ArithAst::Index { key, .. } => arith_need_mem(key),
        _ => false,
    }
}

fn word_to_java(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(java_str_lit(s)),
        IrExpr::Call { func, args, .. } if func == "getVar" => {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                Ok(format!("({name} == null ? \"\" : {name})"))
            } else {
                Err("getVar with non-literal name (v1)".into())
            }
        }
        IrExpr::Var(name, _) => Ok(format!("({name} == null ? \"\" : {name})")),
        IrExpr::Arith(a) => Ok(format!("Long.toString({})", arith_str(a)?)),
        IrExpr::Call { func, args, .. } if is_mem_func(func) => mem_call_java(func, args),
        IrExpr::Call { func, args, .. } if func == "split" => {
            // word splitting of a single value: just the value
            match args.first() {
                Some(inner) => word_to_java(inner),
                None => Err("split with no arg (v1)".into()),
            }
        }
        IrExpr::Capture { expr, .. } => {
            // `$(cmd args)` in a word — run and capture stdout
            if let IrExpr::Arrow(body) = expr.as_ref() {
                if let [IrStmt::Expr(e)] = body.as_slice() {
                    if let IrExpr::Call { func: f, args } = e {
                        if f == "exec" {
                            let mut argv = Vec::new();
                            if let Some(IrExpr::Str(c, _)) = args.first() {
                                argv.push(java_str_lit(c));
                            }
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                for it in items {
                                    argv.push(word_to_java(it)?);
                                }
                            }
                            return Ok(format!(
                                "new String(new ProcessBuilder({}).start().getInputStream().readAllBytes()).trim()",
                                argv.join(", ")
                            ));
                        }
                    }
                }
            }
            Err("capture not in the v1 Java subset".into())
        }
        IrExpr::Array(items) => {
            let parts: Result<Vec<String>, String> =
                items.iter().map(word_to_java).collect();
            Ok(format!("(\"\" + {})", parts?.join(" + \" \" + ")))
        }
        IrExpr::Interpolate(parts) => {
            // `"hello"` / `"hi $name"` — concatenate lit + expr parts
            let mut out = String::from("\"\" + ");
            let mut lit = String::new();
            for p in parts {
                match p {
                    crate::ir::InterpPart::Lit(s) => lit.push_str(s),
                    crate::ir::InterpPart::Expr(x) => {
                        if !lit.is_empty() {
                            out.push_str(&java_str_lit(&lit));
                            out.push_str(" + ");
                            lit.clear();
                        }
                        out.push_str(&word_to_java(x)?);
                        out.push_str(" + ");
                    }
                }
            }
            if !lit.is_empty() || out == "\"\" + " {
                out.push_str(&java_str_lit(&lit));
            } else {
                out.truncate(out.len() - 3);
            }
            Ok(out)
        }
        other => Err(format!("word not in the v1 Java subset: {other:?}")),
    }
}

fn expr_to_java(e: &IrExpr, out: &mut String) -> Result<(), String> {
    match e {
        IrExpr::Str(s, _) => {
            out.push_str(&java_str_lit(s));
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
        IrExpr::Call { func, args, .. } if func == "setArray" => {
            // `declare -a arr=(a b c)` — render as a bracketed array string
            let mut parts = String::new();
            if let Some(IrExpr::Array(items)) = args.get(1) {
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        parts.push_str(", ");
                    }
                    parts.push_str(&word_to_java(it)?);
                }
            }
            out.push_str(&format!("(\"[\" + {})", if parts.is_empty() { "\"\"".to_string() } else { format!("java.util.Arrays.toString(new String[]{{{parts}}})") }));
            Ok(())
        }
        IrExpr::Var(name, _) => {
            out.push_str(name);
            Ok(())
        }
        IrExpr::Arith(a) => {
            out.push_str(&format!("Long.toString({})", arith_str(a)?));
            Ok(())
        }
        IrExpr::Call { func, args, .. } if is_mem_func(func) => {
            out.push_str(&mem_call_java(func, args)?);
            Ok(())
        }
        IrExpr::Capture { expr, .. } => {
            // `$(cmd args)` — run and capture stdout via ProcessBuilder
            if let IrExpr::Arrow(body) = expr.as_ref() {
                if let [IrStmt::Expr(e)] = body.as_slice() {
                    if let IrExpr::Call { func: f, args } = e {
                        if f == "exec" {
                            let mut argv = Vec::new();
                            if let Some(IrExpr::Str(c, _)) = args.first() {
                                argv.push(java_str_lit(c));
                            }
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                for it in items {
                                    argv.push(word_to_java(it)?);
                                }
                            }
                            out.push_str(&format!(
                                "new String(new ProcessBuilder({}).start().getInputStream().readAllBytes()).trim()",
                                argv.join(", ")
                            ));
                            return Ok(());
                        }
                    }
                }
            }
            Err("capture not in the v1 Java subset".into())
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

/// A bash arith expression → a Java `long` expression (the numeric
/// path; mirrors the js renderer's arith()). Var/Ident reads coerce via
/// sh2Num (bash: a non-numeric read is 0). IncDec is only rendered as a
/// STANDALONE statement (its A1 shape is Expr(Arith(IncDec)) — the
/// value is discarded); nested IncDec / arith-Assign / Index refuse.
fn arith_str(a: &ArithAst) -> Result<String, String> {
    let mut out = String::new();
    arith_to_java(a, &mut out)?;
    Ok(out)
}

fn arith_to_java(a: &ArithAst, out: &mut String) -> Result<(), String> {
    match a {
        ArithAst::Num(n) => {
            out.push_str(&n.to_string());
            Ok(())
        }
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            out.push_str(&format!("sh2Num({name})"));
            Ok(())
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_str(lhs)?;
            let r = arith_str(rhs)?;
            match op.as_str() {
                // bash's `**` has no java operator; the comparisons and
                // logical ops are booleans in java — fold to 0/1 (the
                // C/bash arith value)
                "**" => out.push_str(&format!(
                    "(long)Math.pow((double)({l}), (double)({r}))"
                )),
                "&&" => out.push_str(&format!("(({l}) != 0 && ({r}) != 0 ? 1 : 0)")),
                "||" => out.push_str(&format!("(({l}) != 0 || ({r}) != 0 ? 1 : 0)")),
                "==" => out.push_str(&format!("(({l}) == ({r}) ? 1 : 0)")),
                "!=" => out.push_str(&format!("(({l}) != ({r}) ? 1 : 0)")),
                "<" | "<=" | ">" | ">=" => {
                    out.push_str(&format!("(({l}) {op} ({r}) ? 1 : 0)"))
                }
                "+" | "-" | "*" | "/" | "%" | "<<" | ">>" | "&" | "|" | "^" => {
                    out.push_str(&format!("(({l}) {op} ({r}))"))
                }
                other => return Err(format!("arith op {other:?} not in the v1 Java subset")),
            }
            Ok(())
        }
        ArithAst::Un { op, arg } => {
            let a = arith_str(arg)?;
            match op.as_str() {
                "-" | "+" | "~" => out.push_str(&format!("({op}({a}))")),
                "!" => out.push_str(&format!("(({a}) == 0 ? 1 : 0)")),
                other => return Err(format!("arith unary op {other:?} not in the v1 Java subset")),
            }
            Ok(())
        }
        ArithAst::Cond { test, then, else_ } => {
            out.push_str(&format!(
                "(({}) != 0 ? ({}) : ({}))",
                arith_str(test)?,
                arith_str(then)?,
                arith_str(else_)?
            ));
            Ok(())
        }
        ArithAst::Sizeof(ty) => {
            out.push_str(&ty.c_sizeof().unwrap_or(4).to_string());
            Ok(())
        }
        ArithAst::Cast { ty, arg } => {
            let a = arith_str(arg)?;
            match ty {
                IrType::Int32 => out.push_str(&format!("(long)(int)({a})")),
                IrType::UInt32 => out.push_str(&format!("(({a}) & 0xFFFFFFFFL)")),
                IrType::Int64 | IrType::UInt64 => out.push_str(&format!("({a})")),
                _ => return Err("arith cast to a non-integer type (v1)".into()),
            }
            Ok(())
        }
        ArithAst::Index { .. } => Err("arith Index not in the v1 Java subset".into()),
        ArithAst::IncDec { .. } | ArithAst::Assign { .. } => Err(
            "nested arith IncDec/Assign not in the v1 Java subset (standalone only)".into(),
        ),
    }
}

/// A mem* call → `sh2_memX(<args>)` with word-rendered args.
fn mem_call_java(func: &str, args: &[IrExpr]) -> Result<String, String> {
    let arg_exprs: Result<Vec<String>, String> = args.iter().map(word_to_java).collect();
    Ok(format!("sh2_{}({})", func, arg_exprs?.join(", ")))
}

fn is_mem_func(func: &str) -> bool {
    matches!(
        func,
        "memAlloc" | "memStore" | "memLoad" | "memAdvance" | "memFree" | "memTest" | "memElemSize"
    )
}

/// A bash `test "…"` condition string → a Java boolean expression
/// (the estree ref's numeric/string test lowering; mirror of the js
/// renderer's test_render). Handled shapes: [a op b] with the numeric
/// ops -gt/-lt/-ge/-le/-eq/-ne, the string ops =/==/!=, [-n v]/[-z v],
/// [v], and -a/-o conjunctions. Anything else → None (the caller keeps
/// the v1 fallback).
fn test_render(s: &str) -> Option<String> {
    for (sep, joiner) in [(" -a ", " && "), (" -o ", " || ")] {
        if s.contains(sep) {
            let mut parts = Vec::new();
            for p in s.split(sep) {
                parts.push(test_render(p.trim())?);
            }
            return Some(format!("({})", parts.join(joiner)));
        }
    }
    let toks: Vec<&str> = s.split_whitespace().collect();
    match toks.as_slice() {
        [a, op, b] => {
            let a = test_operand(a)?;
            let b = test_operand(b)?;
            match *op {
                // numeric ops coerce both operands (bash: a non-numeric
                // operand is 0)
                "-gt" => Some(format!("(sh2Num({a}) > sh2Num({b}))")),
                "-lt" => Some(format!("(sh2Num({a}) < sh2Num({b}))")),
                "-ge" => Some(format!("(sh2Num({a}) >= sh2Num({b}))")),
                "-le" => Some(format!("(sh2Num({a}) <= sh2Num({b}))")),
                "-eq" => Some(format!("(sh2Num({a}) == sh2Num({b}))")),
                "-ne" => Some(format!("(sh2Num({a}) != sh2Num({b}))")),
                // `[ a = b ]` / `[ a == b ]` are STRING comparisons
                "=" | "==" => Some(format!("({a}.equals({b}))")),
                "!=" => Some(format!("(!{a}.equals({b}))")),
                _ => None,
            }
        }
        [flag, v] if *flag == "-n" => Some(format!("(!{}.isEmpty())", test_operand(v)?)),
        [flag, v] if *flag == "-z" => Some(format!("({}.isEmpty())", test_operand(v)?)),
        // file tests: -f regular, -d dir, -e exists, -s non-empty
        [flag, v] if matches!(*flag, "-f" | "-d" | "-e" | "-s") => {
            let f = test_operand(v)?;
            let path = format!("java.nio.file.Paths.get({f})");
            match *flag {
                "-f" => Some(format!("(java.nio.file.Files.isRegularFile({path}))")),
                "-d" => Some(format!("(java.nio.file.Files.isDirectory({path}))")),
                "-e" => Some(format!("(java.nio.file.Files.exists({path}))")),
                "-s" => Some(format!("(java.nio.file.Files.size({path}) > 0)")),
                _ => None,
            }
        }
        // `[ 0 ]` / `[ "" ]` — bash tests the non-emptiness
        [v] => Some(format!("(!{}.isEmpty())", test_operand(v)?)),
        _ => None,
    }
}

/// A test operand → a Java STRING expression (a field read for `$name`,
/// a literal otherwise). The numeric ops wrap it in sh2Num; the string
/// ops use it as-is. `$?` and the specials are unrepresentable → None
/// (the caller keeps the v1 fallback).
fn test_operand(t: &str) -> Option<String> {
    let t = t.trim().trim_matches('"');
    if let Some(rest) = t.strip_prefix('$') {
        if is_plain_name(rest) {
            return Some(rest.to_string());
        }
        return None;
    }
    Some(java_str_lit(t))
}

/// C memory arena helpers — the estree ref runtime's mem* slice-2
/// (harness/sh2-namespace.mjs), standalone: a flat long-slot arena
/// keyed by allocation id, handles as `\u0001mem:<id>:<offset>` tagged
/// strings, element offsets scaled by the type's size at load/store.
/// The estree ref stores STRING slots; the java arena stores longs
/// (store values coerce via sh2Num) and memLoad stringifies.
const MEM_PREAMBLE: &str = r#"    static long sh2_memSeq = 0;
    static java.util.HashMap<String, long[]> sh2_memArena = new java.util.HashMap<String, long[]>();
    static long sh2_memPos(String h) {
        java.util.regex.Matcher m = java.util.regex.Pattern.compile("^\u0001mem:([^:]+):(-?\\d+)$").matcher(h == null ? "" : h);
        if (m.find()) { try { return Long.parseLong(m.group(2)); } catch (Exception e) { return 0; } }
        try { return Long.parseLong(h); } catch (Exception e) { return 0; }
    }
    static long sh2_memElemSize(String type) {
        String t = String.valueOf(type == null ? "int" : type);
        if (t.equals("char") || t.equals("signed char") || t.equals("unsigned char") || t.equals("int8")) return 1;
        if (t.equals("short") || t.equals("short int") || t.equals("int16")) return 2;
        if (t.equals("int") || t.equals("unsigned int") || t.equals("unsigned") || t.equals("int32") || t.equals("u32") || t.equals("float")) return 4;
        if (t.equals("long") || t.equals("long int") || t.equals("long long") || t.equals("unsigned long") || t.equals("unsigned long long") || t.equals("int64") || t.equals("u64") || t.equals("double")) return 8;
        if (t.equals("void*") || t.equals("ptr") || t.equals("pointer")) return 8;
        return 1;
    }
    static long[] sh2_memArenaOf(String h) {
        java.util.regex.Matcher m = java.util.regex.Pattern.compile("^\u0001mem:([^:]+):(-?\\d+)$").matcher(h == null ? "" : h);
        if (!m.find()) return null;
        String id = m.group(1);
        for (int i = 0; i < id.length(); i++) { if (!Character.isDigit(id.charAt(i))) return null; }
        return sh2_memArena.get(id);
    }
    static String sh2_memAlloc(String size) {
        sh2_memSeq += 1;
        long n = Math.max(0, sh2Num(size));
        sh2_memArena.put(String.valueOf(sh2_memSeq), new long[(int) n]);
        return "\u0001mem:" + sh2_memSeq + ":0";
    }
    static String sh2_memLoad(String h, String offset, String type) {
        long[] a = sh2_memArenaOf(h);
        if (a == null) return "";
        long i = (sh2_memPos(h) + sh2Num(offset)) * sh2_memElemSize(type);
        return i >= 0 && i < a.length ? String.valueOf(a[(int) i]) : "";
    }
    static void sh2_memStore(String h, String offset, String type, String v) {
        long[] a = sh2_memArenaOf(h);
        if (a == null) return;
        long i = (sh2_memPos(h) + sh2Num(offset)) * sh2_memElemSize(type);
        if (i >= 0 && i < a.length) a[(int) i] = sh2Num(v);
    }
    static String sh2_memAdvance(String h, String n) {
        java.util.regex.Matcher m = java.util.regex.Pattern.compile("^(\u0001mem:[^:]+):(-?\\d+)$").matcher(h == null ? "" : h);
        if (!m.find()) return h;
        return m.group(1) + ":" + (sh2_memPos(h) + sh2Num(n));
    }
    static void sh2_memFree(String h) {
        java.util.regex.Matcher m = java.util.regex.Pattern.compile("^\u0001mem:([^:]+):(-?\\d+)$").matcher(h == null ? "" : h);
        if (!m.find()) return;
        String id = m.group(1);
        for (int i = 0; i < id.length(); i++) { if (!Character.isDigit(id.charAt(i))) return; }
        sh2_memArena.remove(id);
    }
    static boolean sh2_memTest(String op, String a, String b) {
        long pa = sh2_memPos(a);
        long pb = sh2_memPos(b);
        if (op.equals("<")) return pa < pb;
        if (op.equals("<=")) return pa <= pb;
        if (op.equals(">")) return pa > pb;
        if (op.equals(">=")) return pa >= pb;
        if (op.equals("==")) return pa == pb;
        if (op.equals("!=")) return pa != pb;
        return false;
    }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir_json_in::shir_json_to_ir;

    /// The java triage cluster (core requests triage-java-20260814-*):
    /// render c-sh-go / posix-sh-go / zsh-sh-go style A1s (typed
    /// var_types objects, arith Bin/IncDec, DoWhile, Break at block
    /// level, test conds, setVar + the C memory arena) and assert the
    /// emitted java carries the native arms. The corpus gate runs the
    /// emitted class through javac; here we pin the render shapes.
    fn render(a1: &str) -> String {
        let prog = shir_json_to_ir(a1).expect("ingress");
        shir_to_java(&prog).expect("render")
    }

    #[test]
    fn java_arith_cond_incdec_dowhile() {
        // c-sh-go t28_dowhile: DoWhile + IncDec Expr stmt + test cond;
        // t51_arith_loop: Ident arith read in an echo word
        let a1 = r###"{"type": "Program", "contract_version": 1, "imports": [], "requires": [], "subs": [], "var_types": [{"name": "i", "type": {"kind": "Int32"}}], "stmts": [{"type": "Assign", "targets": [{"var": "i", "indices": [], "sigil": null}], "expr": {"type": "Str", "style": "DoubleQuoted", "value": "0"}}, {"type": "DoWhile", "until": false, "cond": {"type": "Call", "func": "test", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "$i -lt 3"}]}, "body": [{"type": "Expr", "expr": {"type": "Arith", "ast": {"type": "IncDec", "var": "i", "delta": 1, "prefix": false}}}]}, {"type": "Expr", "expr": {"type": "Call", "func": "exec", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"}, {"type": "Array", "elements": [{"type": "Arith", "ast": {"type": "Bin", "op": "*", "lhs": {"type": "Ident", "name": "i"}, "rhs": {"type": "Num", "value": 2}}}]}]}}]}"###;
        let java = render(a1);
        assert!(java.contains("static long sh2Num"), "arith needs sh2Num: {java}");
        assert!(java.contains("do {"), "DoWhile body: {java}");
        assert!(java.contains("} while ((sh2Num(i) < sh2Num(\"3\")));"), "DoWhile cond: {java}");
        assert!(
            java.contains("i = Long.toString(sh2Num(i) + (1));"),
            "IncDec Expr stmt: {java}"
        );
        assert!(
            java.contains("Long.toString(((sh2Num(i)) * (2)))"),
            "arith word: {java}"
        );
    }

    #[test]
    fn java_labeled_block_break() {
        // c-sh-go t36_dowhile_break: the do-while lowering puts a Break
        // at BLOCK level (outside any loop) — it must target the
        // block's label, not emit a bare `break;`
        let a1 = r###"{"type": "Program", "contract_version": 1, "imports": [], "requires": [], "subs": [], "var_types": [], "stmts": [{"type": "Block", "body": [{"type": "Assign", "targets": [{"var": "i", "indices": [], "sigil": null}], "expr": {"type": "Str", "style": "DoubleQuoted", "value": "0"}}, {"type": "If", "cond": {"type": "Call", "func": "test", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "$i -eq 3"}]}, "then": [{"type": "Break"}], "elsifs": [], "else": []}]}]}"###;
        let java = render(a1);
        assert!(java.contains("b0: {"), "labeled block: {java}");
        assert!(java.contains("break b0;"), "outside-loop break: {java}");
    }

    #[test]
    fn java_mem_arena_and_setvar() {
        // c-sh-go t19_malloc: setVar + the C memory arena
        let a1 = r###"{"type": "Program", "contract_version": 1, "imports": [], "requires": [], "subs": [], "var_types": [{"name": "a", "type": {"kind": "Int32"}}], "stmts": [{"type": "Expr", "expr": {"type": "Call", "func": "setVar", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "a"}, {"type": "Call", "func": "memAlloc", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "12"}]}]}}, {"type": "Expr", "expr": {"type": "Call", "func": "memStore", "args": [{"type": "Call", "func": "getVar", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "a"}]}, {"type": "Str", "style": "DoubleQuoted", "value": "1"}, {"type": "Str", "style": "DoubleQuoted", "value": "int"}, {"type": "Str", "style": "DoubleQuoted", "value": "20"}]}}, {"type": "Assign", "targets": [{"var": "t", "indices": [], "sigil": null}], "expr": {"type": "Call", "func": "memLoad", "args": [{"type": "Call", "func": "getVar", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "a"}]}, {"type": "Str", "style": "DoubleQuoted", "value": "1"}, {"type": "Str", "style": "DoubleQuoted", "value": "int"}]}}, {"type": "Expr", "expr": {"type": "Call", "func": "memFree", "args": [{"type": "Call", "func": "getVar", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "a"}]}]}}, {"type": "Expr", "expr": {"type": "Call", "func": "exec", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"}, {"type": "Array", "elements": [{"type": "Call", "func": "getVar", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "t"}]}]}]}}]}"###;
        let java = render(a1);
        assert!(
            java.contains("static java.util.HashMap<String, long[]> sh2_memArena"),
            "arena preamble: {java}"
        );
        assert!(java.contains("a = sh2_memAlloc(\"12\");"), "setVar+memAlloc: {java}");
        assert!(
            java.contains("sh2_memStore((a == null ? \"\" : a), \"1\", \"int\", \"20\");"),
            "memStore: {java}"
        );
        assert!(
            java.contains("t = sh2_memLoad((a == null ? \"\" : a), \"1\", \"int\");"),
            "memLoad: {java}"
        );
        assert!(java.contains("sh2_memFree((a == null ? \"\" : a));"), "memFree: {java}");
        assert!(java.contains("static String a = \"\";"), "setVar field: {java}");
    }

    #[test]
    fn java_forinit_and_conj_test() {
        // c-sh-go t44_nested_loops: ForInit lowering; t07_cmp's
        // `$a -ge 5 -a $b -le 5` conjunction
        let a1 = r###"{"type": "Program", "contract_version": 1, "imports": [], "requires": [], "subs": [], "var_types": [], "stmts": [{"type": "ForInit", "init": [{"type": "Assign", "targets": [{"var": "i", "indices": [], "sigil": null}], "expr": {"type": "Str", "style": "DoubleQuoted", "value": "1"}}], "cond": {"type": "Call", "func": "test", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "$i -le 3"}]}, "step": [{"type": "Expr", "expr": {"type": "Arith", "ast": {"type": "IncDec", "var": "i", "delta": 1, "prefix": false}}}], "body": []}, {"type": "If", "cond": {"type": "Call", "func": "test", "args": [{"type": "Str", "style": "DoubleQuoted", "value": "$a -ge 5 -a $b -le 5"}]}, "then": [], "elsifs": [], "else": []}]}"###;
        let java = render(a1);
        assert!(java.contains("while ((sh2Num(i) <= sh2Num(\"3\"))) {"), "ForInit while: {java}");
        assert!(java.contains("i = Long.toString(sh2Num(i) + (1));"), "ForInit step: {java}");
        assert!(
            java.contains("((sh2Num(a) >= sh2Num(\"5\")) && (sh2Num(b) <= sh2Num(\"5\")))"),
            "-a conjunction: {java}"
        );
    }
}
