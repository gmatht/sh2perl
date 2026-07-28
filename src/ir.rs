/// Perl IR — Intermediate Representation for code generation.
///
/// The generator produces an `IrProgram` from the shell AST.  A single
/// backend `ir_to_perl()` converts it to Perl text.  `RawText`/`RawExpr`
/// hold unimigrated code so conversion can happen function by function.
///
/// See docs/ir-design.md for full documentation.

// ── Sigils ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Sigil {
    Scalar,
    Array,
    Hash,
}

// ── Binary operators ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BinOpKind {
    Add, Sub, Mul, Div, Mod, Pow,
    Concat,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or, Not,
    BitAnd, BitOr, BitXor,
    ShiftL, ShiftR,
}

// ── String style ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum StrStyle {
    SingleQuoted,
    DoubleQuoted,
    Command,
}

// ── Interpolation parts ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InterpPart {
    Lit(String),
    Expr(Box<IrExpr>),
}

// ── Expressions ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum IrExpr {
    /// Integer literal
    Int(i64),
    /// String literal
    Str(String, StrStyle),
    /// Variable: $name, @name, %name
    Var(String, Sigil),
    /// Array/hash element: $arr[idx], $map{key}
    Index {
        var: String,
        key: Box<IrExpr>,
    },
    /// Binary operation
    BinOp {
        lhs: Box<IrExpr>,
        op: BinOpKind,
        rhs: Box<IrExpr>,
    },
    /// Function call
    Call {
        func: String,
        args: Vec<IrExpr>,
    },
    /// Method call
    MethodCall {
        obj: Box<IrExpr>,
        method: String,
        args: Vec<IrExpr>,
    },
    /// Ternary: cond ? then : else
    Ternary {
        cond: Box<IrExpr>,
        then: Box<IrExpr>,
        else_: Box<IrExpr>,
    },
    /// Defined-or: expr // default
    DefinedOr {
        expr: Box<IrExpr>,
        default: Box<IrExpr>,
    },
    /// String interpolation: "hello $name"
    Interpolate(Vec<InterpPart>),
    /// Raw Perl expression text (migration bridge)
    RawExpr(String),
}

// ── Assignment target ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct AssignTarget {
    pub var: String,
    pub sigil: Sigil,
    pub indices: Vec<IrExpr>,
}

// ── Variable declaration ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    pub sigil: Sigil,
}

// ── Statements ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum IrStmt {
    /// Output: print/say with optional trailing newline
    Output {
        value: IrExpr,
        newline: bool,
    },
    /// Assignment
    Assign {
        targets: Vec<AssignTarget>,
        expr: IrExpr,
    },
    /// Local variable declaration
    Declare {
        vars: Vec<Decl>,
        init: Option<IrExpr>,
    },
    /// Array/hash assignment
    DeclareArray {
        var: String,
        sigil: Sigil,
        elements: Vec<IrExpr>,
    },
    /// if/elsif/else
    If {
        cond: IrExpr,
        then: Vec<IrStmt>,
        elsifs: Vec<(IrExpr, Vec<IrStmt>)>,
        else_: Vec<IrStmt>,
    },
    /// for loop
    For {
        var: String,
        iter: IrExpr,
        body: Vec<IrStmt>,
    },
    /// while loop
    While {
        cond: IrExpr,
        body: Vec<IrStmt>,
    },
    /// do { } while/until
    DoWhile {
        body: Vec<IrStmt>,
        cond: IrExpr,
        until: bool,
    },
    /// System call
    System {
        cmd: IrExpr,
        args: Vec<IrExpr>,
        capture: Option<String>,
    },
    /// Pipeline
    Pipeline {
        stages: Vec<Vec<IrStmt>>,
        last_output: Option<String>,
    },
    /// Return
    Return(Option<IrExpr>),
    /// Raw Perl text (migration bridge)
    RawText(String),
}

// ── Subroutine ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct IrSub {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<IrStmt>,
}

// ── Program ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct IrProgram {
    /// use statements — auto-derived from constructs used
    pub imports: Vec<String>,
    /// Top-level statements
    pub stmts: Vec<IrStmt>,
    /// Subroutine definitions
    pub subs: Vec<IrSub>,
}

// ── Backend: IR → Perl text ─────────────────────────────────────────

/// Convert a single IR statement to a Perl source string.
/// This is the public entry point for the generator to produce clean
/// Perl from IR nodes without constructing a full IrProgram.
pub fn stmt_to_perl(stmt: &IrStmt, indent: usize) -> String {
    let mut out = String::new();
    emit_stmt(&mut out, stmt, indent);
    out
}

/// Convert a single IR expression to a Perl source string.
pub fn expr_to_perl(expr: &IrExpr) -> String {
    ir_expr_to_perl(expr)
}

/// Convert an `IrProgram` to a Perl source string.
///
/// Style decisions (say vs print, parentheses style, indentation) are
/// made here, not in the generator.
pub fn ir_to_perl(prog: &IrProgram) -> String {
    let mut out = String::new();

    // Shebang
    out.push_str("#!/usr/bin/env perl\n");
    out.push_str("use strict;\n");
    out.push_str("use warnings;\n");

    // Imports
    for import in &prog.imports {
        out.push_str(&format!("use {};\n", import));
    }
    if !prog.imports.is_empty() {
        out.push('\n');
    }

    // Top-level variable declarations from usage analysis
    // (emitted by generator as Declare stmts, handled below)

    // Top-level statements
    for stmt in &prog.stmts {
        emit_stmt(&mut out, stmt, 0);
    }
    out.push('\n');

    // Subroutines
    for sub in &prog.subs {
        emit_sub(&mut out, sub);
        out.push('\n');
    }

    // Exit
    out.push_str("exit $main_exit_code;\n");

    out
}

// ── Statement emitter ────────────────────────────────────────────────

pub(crate) fn emit_stmt(out: &mut String, stmt: &IrStmt, indent: usize) {
    match stmt {
        IrStmt::RawText(text) => {
            // Splice verbatim — no transformation
            out.push_str(text);
        }

        IrStmt::Output { value, newline } => {
            let expr = ir_expr_to_perl(value);
            if *newline {
                // Use `say` for newline-terminated output (cleaner than
                // print + manual newline check).
                emit_indent(out, indent);
                out.push_str(&format!("say {};\n", expr));
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("print {};\n", expr));
            }
        }

        IrStmt::Assign { targets, expr } => {
            let lhs = targets
                .iter()
                .map(|t| format!("${}", t.var))
                .collect::<Vec<_>>()
                .join(", ");
            let rhs = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("({}) = ({});\n", lhs, rhs));
        }

        IrStmt::Declare { vars, init } => {
            let decls = vars
                .iter()
                .map(|d| match d.sigil {
                    Sigil::Scalar => format!("${}", d.name),
                    Sigil::Array => format!("@{}", d.name),
                    Sigil::Hash => format!("%{}", d.name),
                })
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(init_expr) = init {
                let rhs = ir_expr_to_perl(init_expr);
                emit_indent(out, indent);
                out.push_str(&format!("my ({}) = ({});\n", decls, rhs));
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("my {};\n", decls));
            }
        }

        IrStmt::DeclareArray { var, sigil, elements } => {
            let elems = elements
                .iter()
                .map(|e| ir_expr_to_perl(e))
                .collect::<Vec<_>>()
                .join(", ");
            let sigil_char = match sigil {
                Sigil::Array => '@',
                Sigil::Hash => '%',
                _ => '$',
            };
            emit_indent(out, indent);
            out.push_str(&format!("my {}{} = ({});\n", sigil_char, var, elems));
        }

        IrStmt::If { cond, then, elsifs, else_ } => {
            let cond_str = ir_expr_to_perl(cond);
            emit_indent(out, indent);
            out.push_str(&format!("if ({}) {{\n", cond_str));
            for s in then {
                emit_stmt(out, s, indent + 1);
            }
            for (econd, ebody) in elsifs {
                let estr = ir_expr_to_perl(econd);
                emit_indent(out, indent);
                out.push_str(&format!("}} elsif ({}) {{\n", estr));
                for s in ebody {
                    emit_stmt(out, s, indent + 1);
                }
            }
            if !else_.is_empty() {
                emit_indent(out, indent);
                out.push_str("} else {\n");
                for s in else_ {
                    emit_stmt(out, s, indent + 1);
                }
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }

        IrStmt::For { var, iter, body } => {
            let iter_str = ir_expr_to_perl(iter);
            emit_indent(out, indent);
            out.push_str(&format!("for my ${} ({}) {{\n", var, iter_str));
            for s in body {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }

        IrStmt::While { cond, body } => {
            let cond_str = ir_expr_to_perl(cond);
            emit_indent(out, indent);
            out.push_str(&format!("while ({}) {{\n", cond_str));
            for s in body {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }

        IrStmt::System { cmd, args, capture } => {
            let cmd_str = ir_expr_to_perl(cmd);
            if let Some(var) = capture {
                // With capture: emit qx{...} assignment
                // Build the full command string with args
                let mut full_cmd = cmd_str.clone();
                if !args.is_empty() {
                    let arg_strs: Vec<String> = args.iter()
                        .map(|a| ir_expr_to_perl(a))
                        .collect();
                    // For qx{}, we build a single string command
                    full_cmd = format!("{} {}", cmd_str, arg_strs.join(" "));
                }
                // Remove surrounding backticks if already present from StrStyle::Command
                let inner = if full_cmd.starts_with('`') && full_cmd.ends_with('`') {
                    &full_cmd[1..full_cmd.len()-1]
                } else {
                    &full_cmd
                };
                emit_indent(out, indent);
                out.push_str(&format!("my ${} = qx{{{}}};\n", var, inner));
                out.push_str(&format!("$CHILD_ERROR = $? >> 8;\n"));
            } else {
                // Without capture: use system()
                emit_indent(out, indent);
                if args.is_empty() {
                    out.push_str(&format!("system {};\n", cmd_str));
                } else {
                    let arg_strs: Vec<String> = args.iter()
                        .map(|a| ir_expr_to_perl(a))
                        .collect();
                    out.push_str(&format!("system({}, {});\n", cmd_str, arg_strs.join(", ")));
                }
            }
        }

        IrStmt::Return(Some(expr)) => {
            let e = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("return {};\n", e));
        }
        IrStmt::Return(None) => {
            emit_indent(out, indent);
            out.push_str("return;\n");
        }

        IrStmt::Pipeline { stages, .. } => {
            // Simple pipeline: for now fall back to RawText-style output
            // until proper pipeline IR is fully designed.
            for stage in stages {
                for s in stage {
                    emit_stmt(out, s, indent);
                }
            }
        }

        IrStmt::DoWhile { body, cond, until } => {
            let kw = if *until { "until" } else { "while" };
            let cond_str = ir_expr_to_perl(cond);
            emit_indent(out, indent);
            out.push_str("do {\n");
            for s in body {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str(&format!("}} {} ({});\n", kw, cond_str));
        }
    }
}

// ── Subroutine emitter ───────────────────────────────────────────────

pub(crate) fn emit_sub(out: &mut String, sub: &IrSub) {
    out.push_str(&format!("sub {} {{\n", sub.name));
    for s in &sub.body {
        emit_stmt(out, s, 1);
    }
    out.push_str("}\n");
}

// ── Expression emitter ───────────────────────────────────────────────

pub(crate) fn ir_expr_to_perl(expr: &IrExpr) -> String {
    match expr {
        IrExpr::RawExpr(text) => text.clone(),

        IrExpr::Int(n) => n.to_string(),

        IrExpr::Str(s, style) => match style {
            StrStyle::SingleQuoted => format!("'{}'", s.replace('\'', "\\'")),
            StrStyle::DoubleQuoted => format!("\"{}\"", s.replace('"', "\\\"")),
            StrStyle::Command => format!("`{}`", s),
        },

        IrExpr::Var(name, sigil) => match sigil {
            Sigil::Scalar => format!("${}", name),
            Sigil::Array => format!("@{}", name),
            Sigil::Hash => format!("%{}", name),
        },

        IrExpr::Index { var, key } => {
            let k = ir_expr_to_perl(key);
            format!("${{{}}}[{}]", var, k)
        }

        IrExpr::BinOp { lhs, op, rhs } => {
            let l = ir_expr_to_perl(lhs);
            let r = ir_expr_to_perl(rhs);
            let op_str = match op {
                BinOpKind::Add => "+",
                BinOpKind::Sub => "-",
                BinOpKind::Mul => "*",
                BinOpKind::Div => "/",
                BinOpKind::Mod => "%",
                BinOpKind::Pow => "**",
                BinOpKind::Concat => ".",
                BinOpKind::Eq => "==",
                BinOpKind::Ne => "!=",
                BinOpKind::Lt => "<",
                BinOpKind::Gt => ">",
                BinOpKind::Le => "<=",
                BinOpKind::Ge => ">=",
                BinOpKind::And => "&&",
                BinOpKind::Or => "||",
                BinOpKind::Not => "!",
                BinOpKind::BitAnd => "&",
                BinOpKind::BitOr => "|",
                BinOpKind::BitXor => "^",
                BinOpKind::ShiftL => "<<",
                BinOpKind::ShiftR => ">>",
            };
            format!("({} {} {})", l, op_str, r)
        }

        IrExpr::Call { func, args } => {
            let a = args.iter().map(|a| ir_expr_to_perl(a)).collect::<Vec<_>>().join(", ");
            format!("{}({})", func, a)
        }

        IrExpr::MethodCall { obj, method, args } => {
            let o = ir_expr_to_perl(obj);
            let a = args.iter().map(|a| ir_expr_to_perl(a)).collect::<Vec<_>>().join(", ");
            format!("{}->{}({})", o, method, a)
        }

        IrExpr::Ternary { cond, then, else_ } => {
            let c = ir_expr_to_perl(cond);
            let t = ir_expr_to_perl(then);
            let e = ir_expr_to_perl(else_);
            format!("({} ? {} : {})", c, t, e)
        }

        IrExpr::DefinedOr { expr, default } => {
            let e = ir_expr_to_perl(expr);
            let d = ir_expr_to_perl(default);
            format!("({} // {})", e, d)
        }

        IrExpr::Interpolate(parts) => {
            let mut s = String::from("\"");
            for part in parts {
                match part {
                    InterpPart::Lit(text) => {
                        // Escape special Perl characters in double-quoted strings
                        for ch in text.chars() {
                            match ch {
                                '"' => s.push_str("\\\""),
                                '\\' => s.push_str("\\\\"),
                                '$' => s.push_str("\\$"),
                                '@' => s.push_str("\\@"),
                                '\n' => s.push_str("\\n"),
                                '\t' => s.push_str("\\t"),
                                '\r' => s.push_str("\\r"),
                                c => s.push(c),
                            }
                        }
                    },
                    InterpPart::Expr(e) => {
                        // If the expression is a simple variable, emit $varname
                        // directly without the ${...} wrapper.
                        match e.as_ref() {
                            IrExpr::Var(name, Sigil::Scalar) => {
                                s.push_str(&format!("${}", name));
                            }
                            IrExpr::Var(name, Sigil::Array) => {
                                s.push_str(&format!("@{}[\"\"]", name));
                            }
                            _ => {
                                let ev = ir_expr_to_perl(e);
                                s.push_str(&format!("${{{}}}", ev));
                            }
                        }
                    }
                }
            }
            s.push('"');
            s
        }
    }
}

// ── Helper ───────────────────────────────────────────────────────────

pub(crate) fn emit_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

// ── Bridge: wrap current generator output in RawText ────────────────

impl IrProgram {
    /// Create an IrProgram from the current text-based generator output.
    /// This is the migration bridge: once all generator functions produce
    /// IR nodes, this wrapper becomes unnecessary.
    pub fn from_raw_perl(code: &str) -> Self {
        IrProgram {
            imports: vec![
                "Carp".to_string(),
                "English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME)".to_string(),
                "locale".to_string(),
                "IPC::Open3".to_string(),
            ],
            stmts: vec![IrStmt::RawText(code.to_string())],
            subs: vec![],
        }
    }
}
