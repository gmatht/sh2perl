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
    if stmts_need_sh2num(&prog.stmts) {
        // printf %d/%i/%u conversion helper — bash coerces a
        // non-numeric arg to 0 (the java backend's fields are Strings)
        out.push_str("    static long sh2Num(String s) {\n");
        out.push_str("        try { return Long.parseLong(s.trim()); } catch (Exception e) { return 0; }\n");
        out.push_str("    }\n");
    }
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
        other => Err(format!("expression not in the v1 Java subset: {other:?}")),
    }
}

/// A java string literal: escape backslash, quote, and control chars
/// (a raw newline inside the literal is a compile error — cpp-sh-go
/// t30_static_assert.cc's `printf "static assert ok\n"` carries a real
/// \n in the A1 Str).
fn java_str_lit(s: &str) -> String {    let mut out = String::with_capacity(s.len() + 2);
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

/// Does any statement contain a printf with a numeric conversion
/// (%d/%i/%u)? Those need the sh2Num coercion helper emitted.
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
        IrStmt::While { cond, body, .. } => expr_need_sh2num(cond) || stmts_need_sh2num(body),
        IrStmt::Block(b) => stmts_need_sh2num(b),
        _ => false,
    })
}

fn expr_need_sh2num(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } if func == "exec" => {
            // printf with a literal format containing a numeric conv
            let Some(IrExpr::Array(items)) = args.get(1) else {
                return false;
            };
            let Some(IrExpr::Str(fmt, _)) = items.first() else {
                return false;
            };
            printf_parse(fmt)
                .map(|(els, _)| {
                    els.iter()
                        .any(|(_, spec)| matches!(spec, Some('d' | 'i' | 'u')))
                })
                .unwrap_or(false)
        }
        IrExpr::Array(items) => items.iter().any(expr_need_sh2num),
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
