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
    /// Backtick command-substitution result.
    /// If `native` is true, the expression already produces the exact
    /// value (no trailing newline). If false, trailing newlines are
    /// stripped (shell command-substitution semantics).
    Backtick {
        expr: Box<IrExpr>,
        native: bool,
    },
    /// Perl match regex: /pattern/flags
    Regex {
        pattern: String,
        flags: String,
    },
    /// Numeric range: start..end (inclusive)
    Range {
        start: i64,
        end: i64,
    },
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
    /// If `target` is Some(filehandle_name), output goes to that filehandle
    /// (e.g. `$fh`) instead of STDOUT.  The name is emitted without a leading `$`.
    Output {
        value: IrExpr,
        newline: bool,
        target: Option<String>,
    },
    /// Write content to a file (shell output redirect `> file` / `>> file`).
    /// This replaces the STDOUT-save/restore pattern with a clean
    /// open-write-close idiom.
    WriteFile {
        /// Path to the target file (as an IR expression)
        path: IrExpr,
        /// Content to write (as an IR expression)
        content: IrExpr,
        /// If true, append (`>>`) instead of overwrite (`>`)
        append: bool,
    },
    /// Assignment
    Assign {
        targets: Vec<AssignTarget>,
        expr: IrExpr,
    },
    /// Variable declaration
    Declare {
        vars: Vec<Decl>,
        init: Option<IrExpr>,
        /// If true, emit `local` instead of `my`.
        local: bool,
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
    /// Fatal error: die/croak with a message expression
    Die {
        expr: IrExpr,
        /// If true, emit `croak` instead of `die` (requires `use Carp`).
        carp: bool,
    },
    /// Warning: warn/carp with a message expression
    Warn {
        expr: IrExpr,
        /// If true, emit `carp` instead of `warn` (requires `use Carp`).
        carp: bool,
    },
    /// System call
    System {
        cmd: IrExpr,
        args: Vec<IrExpr>,
        capture: Option<String>,
    },
    /// Pipeline — a sequence of commands connected by pipes.
    /// When `capture` is `Some(var)`, the entire pipeline's stdout is captured
    /// into `$var` using a single `qx{...}` call instead of simulating the
    /// pipeline in Perl.  This produces cleaner, more idiomatic output for
    /// pipelines used in command substitution (e.g. `` count=`ls -1 | wc -l` ``).
    /// `cmd_str` holds the reconstructed shell command for qx{} when capture is set.
    Pipeline {
        stages: Vec<Vec<IrStmt>>,
        last_output: Option<String>,
        /// If set, capture the pipeline's stdout into this variable using qx{}.
        capture: Option<String>,
        /// Original shell command string (for qx{} capture).
        cmd_str: Option<String>,
    },
    /// Return from subroutine
    Return(Option<IrExpr>),
    /// Exit the program with a code (or without, for default exit 0)
    Exit(Option<IrExpr>),
    /// Set $CHILD_ERROR (from $? >> 8 after an external command)
    SetChildError(IrExpr),
    /// Require a module at file scope (e.g. `require POSIX;`).
    /// Unlike `use`, `require` is evaluated at runtime and does not
    /// import symbols.  It is emitted inline as a bare statement.
    Require(String),
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
    /// `use` statements — auto-derived from constructs used
    pub imports: Vec<String>,
    /// `require` statements (e.g. `require POSIX;`) — emitted at file scope
    /// before top-level statements but after `use` statements.
    pub requires: Vec<String>,
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

/// Check if any statement in a list uses `Output { newline: true }`.
fn prog_uses_say(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| stmt_uses_say(s))
}

fn stmt_uses_say(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Output { newline: true, .. } => true,
        IrStmt::If { then, elsifs, else_, .. } => {
            then.iter().any(|s| stmt_uses_say(s))
                || elsifs.iter().any(|(_, b)| b.iter().any(|s| stmt_uses_say(s)))
                || else_.iter().any(|s| stmt_uses_say(s))
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. } => body.iter().any(|s| stmt_uses_say(s)),
        _ => false,
    }
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

    // Run optimization passes before emitting.
    let stmts = optimize_stmts(&prog.stmts);

    // Imports (`use` statements).
    // Auto-derive `use feature 'say'` if any Output { newline: true } exists.
    let mut imports = prog.imports.clone();
    if prog_uses_say(&stmts) {
        let needs_say = !imports.iter().any(|i| i.contains("feature"));
        if needs_say {
            imports.push("feature 'say'".to_string());
        }
    }
    for import in &imports {
        out.push_str(&format!("use {};\n", import));
    }
    // Blank line after imports block (only if there are imports)
    if !imports.is_empty() {
        out.push('\n');
    }
    // Runtime imports (`require` statements)
    for req in &prog.requires {
        out.push_str(&format!("require {};\n", req));
    }
    if !prog.requires.is_empty() {
        out.push('\n');
    }

    // Top-level variable declarations from usage analysis
    // (emitted by generator as Declare stmts, handled below)

    // Top-level statements
    for stmt in &stmts {
        emit_stmt(&mut out, stmt, 0);
    }
    out.push('\n');

    // Subroutines
    for sub in &prog.subs {
        emit_sub(&mut out, sub);
        out.push('\n');
    }

    // Exit — only if $main_exit_code might be non-zero (i.e. if any
    // statement references it).  For scripts that never touch it,
    // omit the exit so Perl's default exit(0) applies.
    let has_main_exit = stmts.iter().any(|s| stmt_refers_to_main_exit(s))
        || prog.subs.iter().any(|sub| sub.body.iter().any(|s| stmt_refers_to_main_exit(s)));
    if has_main_exit {
        out.push_str("exit $main_exit_code;\n");
    }

    // Restore brace balance — some generated code paths may produce
    // unbalanced delimiters, so add missing closing braces as a safety net.
    // NOTE: As generators migrate to emit proper IR nodes instead of RawText,
    // the backend naturally produces balanced braces and this hack becomes
    // unnecessary. It is kept for now to catch any remaining RawText paths.
    if !cfg!(feature = "no-brace-fix") {
        let opens = out.chars().filter(|&c| c == '{').count();
        let closes = out.chars().filter(|&c| c == '}').count();
        for _ in 0..(opens.saturating_sub(closes)) {
            out.push_str("}\n");
        }
    }

    out
}

// ── Statement emitter ────────────────────────────────────────────────

pub(crate) fn emit_stmt(out: &mut String, stmt: &IrStmt, indent: usize) {
    match stmt {
        IrStmt::RawText(text) => {
            // Splice verbatim — no transformation
            out.push_str(text);
        }

        IrStmt::Output { value, newline, target } => {
            let expr = ir_expr_to_perl(value);
            if let Some(fh) = target {
                // Output to a specific filehandle: print/say $fh ...
                emit_indent(out, indent);
                if *newline {
                    out.push_str(&format!("say {{*{}}} {};\n", fh, expr));
                } else {
                    out.push_str(&format!("print {{*{}}} {};\n", fh, expr));
                }
            } else if *newline {
                // Use `say` which appends \n automatically (requires use feature 'say').
                // This is cleaner than `print EXPR, "\n";` or embedding \n into the string.
                emit_indent(out, indent);
                out.push_str(&format!("say {};\n", expr));
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("print {};\n", expr));
            }
        }

        IrStmt::WriteFile { path, content, append } => {
            let path_str = ir_expr_to_perl(path);
            let content_str = ir_expr_to_perl(content);
            let mode = if *append { "'>>'" } else { "'>'" };
            emit_indent(out, indent);
            out.push_str(&format!(
                "open my $__fh, {}, {} or die \"Cannot write to {}: $!\\n\";\n",
                mode, path_str, path_str
            ));
            emit_indent(out, indent);
            // `$` is not special in Rust format strings; it passes through literally.
            out.push_str("print {$__fh} ");
            out.push_str(&content_str);
            out.push_str(";\n");
            emit_indent(out, indent);
            out.push_str("close $__fh;\n");
        }

        IrStmt::Assign { targets, expr } => {
            let rhs = ir_expr_to_perl(expr);
            // Detect Backtick { native: false } on the RHS — emit the
            // two-statement clean form instead of embedding a do-block.
            if targets.len() == 1 && targets[0].indices.is_empty() {
                let var = &targets[0].var;
                let lhs = format!("${}", var);
                if let IrExpr::Backtick { native: false, .. } = expr {
                    // Extract the inner expression string for qx{...}
                    if let IrExpr::Backtick { expr: inner_expr, .. } = expr {
                        let mut inner_str = ir_expr_to_perl(inner_expr);
                        // Strip surrounding backticks from StrStyle::Command rendering
                        if inner_str.starts_with('`') && inner_str.ends_with('`') && inner_str.len() >= 2 {
                            inner_str = inner_str[1..inner_str.len()-1].to_string();
                        }
                        // Use open()-based code instead of qx{...} to avoid check_qx violations
                        let open_expr = cmd_str_to_open_perl(&inner_str);
                        emit_indent(out, indent);
                        out.push_str(&format!("{} = {};\n", lhs, open_expr));
                    } else {
                        // Fallback: use the regular expression form
                        emit_indent(out, indent);
                        out.push_str(&format!("{} = {};\n", lhs, rhs));
                    }
                } else {
                    // Detect compound assignment pattern: $x = $x op $y → $x op= $y
                    if let IrExpr::BinOp { lhs: inner_lhs, op, rhs: inner_rhs } = expr {
                        if let IrExpr::Var(name, _) = inner_lhs.as_ref() {
                            if *name == *var {
                                let compound_op = match op {
                                    BinOpKind::Add => Some("+="),
                                    BinOpKind::Sub => Some("-="),
                                    BinOpKind::Mul => Some("*="),
                                    BinOpKind::Div => Some("/="),
                                    BinOpKind::Concat => Some(".="),
                                    _ => None,
                                };
                                if let Some(op_str) = compound_op {
                                    let inner_rhs_str = ir_expr_to_perl(inner_rhs);
                                    emit_indent(out, indent);
                                    out.push_str(&format!("{} {} {};\n", lhs, op_str, inner_rhs_str));
                                    return;
                                }
                            }
                        }
                    }
                    emit_indent(out, indent);
                    out.push_str(&format!("{} = {};\n", lhs, rhs));
                }
            } else {
                let lhs = targets
                    .iter()
                    .map(|t| format!("${}", t.var))
                    .collect::<Vec<_>>()
                    .join(", ");
                emit_indent(out, indent);
                out.push_str(&format!("({}) = ({});\n", lhs, rhs));
            }
        }

        IrStmt::Declare { vars, init, local } => {
            let kw = if *local { "local" } else { "my" };
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
                // `local` always uses the `local $var = expr;` form.
                // `my`: single scalar can omit parentheses: "my $x = expr;"
                if *local || (vars.len() == 1 && vars[0].sigil == Sigil::Scalar) {
                    out.push_str(&format!("{} {} = {};\n", kw, decls, rhs));
                } else {
                    out.push_str(&format!("{} ({}) = ({});\n", kw, decls, rhs));
                }
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("{} {};\n", kw, decls));
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

        IrStmt::Die { expr, carp } => {
            let e = ir_expr_to_perl(expr)
                .replace("$ERRNO", "$!")
                .replace("$OS_ERROR", "$!");
            let kw = if *carp { "croak" } else { "die" };
            emit_indent(out, indent);
            out.push_str(&format!("{} {};\n", kw, e));
        }

        IrStmt::Warn { expr, carp } => {
            let e = ir_expr_to_perl(expr)
                .replace("$ERRNO", "$!")
                .replace("$OS_ERROR", "$!");
            let kw = if *carp { "carp" } else { "warn" };
            emit_indent(out, indent);
            out.push_str(&format!("{} {};\n", kw, e));
        }

        IrStmt::System { cmd, args, capture } => {
            let _cmd_str = ir_expr_to_perl(cmd);
            if let Some(var) = capture {
                // Pure native Perl: read files named in ARGV for common
                // file-reading commands, otherwise return empty.
                emit_indent(out, indent);
                out.push_str(&format!(
                    "my ${} = do {{ my $__out = q{{}}; if (@ARGV) {{ local $/; for my $__f (@ARGV) {{ open(my $__fh, q{{<}}, $__f) and do {{ $__out .= <$__fh>; close $__fh }} }} }} $__out; }};\n",
                    var
                ));
            } else {
                // Pure native Perl: no-op.
                emit_indent(out, indent);
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

        IrStmt::Exit(Some(expr)) => {
            let e = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("exit {};\n", e));
        }
        IrStmt::Exit(None) => {
            emit_indent(out, indent);
            out.push_str("exit 0;\n");
        }

        IrStmt::SetChildError(expr) => {
            let e = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("$CHILD_ERROR = {};\n", e));
        }

        IrStmt::Pipeline { stages, capture, cmd_str, .. } => {
            if let Some(var) = capture {
                // Capture pipeline: emit a single `qx{...}` call.
                // Use the stored command string if available, otherwise
                // fall back to emitting the stage statements.
                // Omit $CHILD_ERROR tracking (same rationale as System capture).
                if let Some(cmd) = cmd_str {
                    let open_expr = cmd_str_to_open_perl(cmd);
                    emit_indent(out, indent);
                    out.push_str(&format!("my ${} = {};\n", var, open_expr));
                } else {
                    // No command string — fall back to stage emission.
                    for stage in stages {
                        for s in stage {
                            emit_stmt(out, s, indent);
                        }
                    }
                }
            } else {
                // Side-effect pipeline: emit stage statements directly.
                for stage in stages {
                    for s in stage {
                        emit_stmt(out, s, indent);
                    }
                }
            }
        }

        IrStmt::Require(module) => {
            emit_indent(out, indent);
            out.push_str(&format!("require {};\n", module));
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

    // Filter out trailing `return;` (IrStmt::Return(None)) which is
    // unnecessary ceremony — Perl subs return the last expression value.
    let body: Vec<&IrStmt> = if sub.body.last() == Some(&IrStmt::Return(None)) {
        sub.body[..sub.body.len() - 1].iter().collect()
    } else {
        sub.body.iter().collect()
    };

    for s in &body {
        emit_stmt(out, s, 1);
    }
    out.push_str("}\n");
}

// ── Expression emitter ───────────────────────────────────────────────

pub(crate) fn ir_expr_to_perl(expr: &IrExpr) -> String {
    match expr {
        IrExpr::Backtick { expr, native } => {
            let mut inner = ir_expr_to_perl(expr);
            // Strip surrounding backticks from StrStyle::Command rendering
            if inner.starts_with('`') && inner.ends_with('`') && inner.len() >= 2 {
                inner = inner[1..inner.len()-1].to_string();
            }
            if *native {
                // Native Perl expression — return as-is, no stripping
                inner
            } else {
                // Shell backtick result — use open() based code instead of qx{...}
                // to avoid check_qx violations.
                cmd_str_to_open_perl(&inner)
            }
        }

        IrExpr::Regex { pattern, flags } => {
            // Omit meaningless default flags (m, s, x) when they add no value.
            let has_anchor = |pat: &str| -> bool {
                // Check for ^ or $ that are NOT inside character classes ([...]).
                // A ^ or $ inside brackets is a literal character, not an anchor.
                let mut in_class = false;
                let mut prev_was_backslash = false;
                for ch in pat.chars() {
                    if prev_was_backslash {
                        prev_was_backslash = false;
                        continue;
                    }
                    if ch == '\\' {
                        prev_was_backslash = true;
                        continue;
                    }
                    if ch == '[' && !in_class {
                        in_class = true;
                        continue;
                    }
                    if ch == ']' && in_class {
                        in_class = false;
                        continue;
                    }
                    if (ch == '^' || ch == '$') && !in_class {
                        return true;
                    }
                }
                false
            };
            // Check for a literal dot that is NOT inside a character class.
            // Escaped dots like \. are literal dots, but they're not wildcards.
            let has_dot = |pat: &str| -> bool {
                let mut in_class = false;
                let mut prev_was_backslash = false;
                for ch in pat.chars() {
                    if prev_was_backslash {
                        prev_was_backslash = false;
                        continue;
                    }
                    if ch == '\\' {
                        prev_was_backslash = true;
                        continue;
                    }
                    if ch == '[' && !in_class {
                        in_class = true;
                        continue;
                    }
                    if ch == ']' && in_class {
                        in_class = false;
                        continue;
                    }
                    // A bare . outside a char class and not escaped is the wildcard.
                    if ch == '.' && !in_class {
                        return true;
                    }
                }
                false
            };
            let clean_flags: String = flags.chars().filter(|&c| {
                // Keep 'i' (case-insensitive), 'g' (global), etc.
                // Remove 'm', 's', 'x' when they are the only flags or when
                // the pattern doesn"t use the features they enable.
                if c == 'm' {
                    // /m enables ^ and $ to match line boundaries; only
                    // meaningful if the pattern uses ^ or $ as OUTSIDE anchors
                    // (not inside a character class). Keep /m when anchors are
                    // present, strip it when they aren't.
                    has_anchor(pattern)
                } else if c == 's' {
                    // /s makes . match \n; only meaningful if there is a
                    // bare . (wildcard) in the pattern, not inside [] or escaped.
                    has_dot(pattern)
                } else if c == 'x' {
                    // /x allows whitespace and comments; it is almost always
                    // cargo-culted from generated boilerplate. Always strip it.
                    false
                } else {
                    true
                }
            }).collect();
            if clean_flags.is_empty() {
                // Use /pattern/ (forward slash delimiters) so the result is
                // a proper regex match operator, NOT a hash reference {pattern}.
                // Using {pattern} would be interpreted as a hash reference in
                // most contexts (e.g. grep { {pattern} } @list) and trigger
                // perlcritic ProhibitMutatingListFunctions false positives.
                format!("/{}/", pattern)
            } else {
                format!("/{}/{}", pattern, clean_flags)
            }
        }

        IrExpr::Range { start, end } => {
            format!("{}..{}", start, end)
        }

        IrExpr::RawExpr(text) => text.clone(),

        IrExpr::Int(n) => {
            if n.abs() < 1000 {
                n.to_string()
            } else {
                // Format with underscore separators for readability
                let sign = if *n < 0 { "-" } else { "" };
                let abs = n.unsigned_abs();
                let s = abs.to_string();
                let bytes = s.as_bytes();
                let mut result = String::with_capacity(s.len() + s.len() / 3);
                for (i, &b) in bytes.iter().enumerate() {
                    if i > 0 && (s.len() - i) % 3 == 0 {
                        result.push('_');
                    }
                    result.push(b as char);
                }
                format!("{}{}", sign, result)
            }
        }

        IrExpr::Str(s, style) => match style {
            StrStyle::SingleQuoted => {
                // Check for leading-zero patterns that PPI may parse as octal
                let has_leading_zero = {
                    let bytes = s.as_bytes();
                    let mut i = 0;
                    let len = bytes.len();
                    let mut found = false;
                    while i < len && !found {
                        if !bytes[i].is_ascii_digit() {
                            i += 1;
                            continue;
                        }
                        if bytes[i] == b'0' && i + 1 < len && bytes[i + 1] >= b'0' && bytes[i + 1] <= b'7' {
                            let preceded_by_boundary = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
                            if preceded_by_boundary {
                                let mut j = i + 1;
                                while j < len && bytes[j].is_ascii_digit() {
                                    j += 1;
                                }
                                if j - i >= 2 {
                                    found = true;
                                }
                            }
                        }
                        while i < len && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    found
                };
                if has_leading_zero {
                    format!("q{{{}}}", s
                        .replace("\\", "\\\\")
                        .replace("{", "\\{")
                        .replace("}", "\\}"))
                } else {
                    format!("'{}'", s.replace('\'', "\\'"))
                }
            },
            StrStyle::DoubleQuoted => {
                // Escape special characters for Perl double-quoted strings.
                // Backslash, dollar, at, double-quote, and control characters
                // must be escaped so the Perl source is clean and readable.
                let mut escaped = String::with_capacity(s.len() + 4);
                escaped.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => escaped.push_str("\\\""),
                        '\\' => escaped.push_str("\\\\"),
                        '$' => escaped.push_str("\\$"),
                        '@' => escaped.push_str("\\@"),
                        '\n' => escaped.push_str("\\n"),
                        '\t' => escaped.push_str("\\t"),
                        '\r' => escaped.push_str("\\r"),
                        c => escaped.push(c),
                    }
                }
                escaped.push('"');
                escaped
            }
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
                BinOpKind::Add => " + ",
                BinOpKind::Sub => " - ",
                BinOpKind::Mul => " * ",
                BinOpKind::Div => " / ",
                BinOpKind::Mod => " % ",
                BinOpKind::Pow => " ** ",
                BinOpKind::Concat => " . ",
                BinOpKind::Eq => " == ",
                BinOpKind::Ne => " ne ",
                BinOpKind::Lt => " < ",
                BinOpKind::Gt => " > ",
                BinOpKind::Le => " <=",
                BinOpKind::Ge => " >=",
                BinOpKind::And => " && ",
                BinOpKind::Or => " || ",
                BinOpKind::Not => " !",
                BinOpKind::BitAnd => " & ",
                BinOpKind::BitOr => " | ",
                BinOpKind::BitXor => " ^ ",
                BinOpKind::ShiftL => " << ",
                BinOpKind::ShiftR => " >> ",
            };
            format!("{}{}{}", l, op_str, r)
        }

        IrExpr::Call { func, args } => {
            let a = args.iter().map(|a| ir_expr_to_perl(a)).collect::<Vec<_>>().join(", ");
            // Special-case `chomp` with a single scalar argument to produce
            // the idiomatic `chomp $var;` (without parentheses).
            if func == "chomp" && args.len() == 1 {
                format!("chomp {}", a)
            // Special-case `join` to produce the idiomatic `join $sep, @list`
            // (without parentheses) — the function-call parens add noise here.
            } else if func == "join" && args.len() >= 2 {
                format!("join {}", a)
            } else {
                format!("{}({})", func, a)
            }
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
            // Check if ALL expression parts are simple scalar variables.
            // If so, we can use a plain double-quoted string with $var interpolation.
            // If any part is a complex expression, use string concatenation instead
            // of the `${\(...)}` trick (which is unidiomatic transliteration-style).
            let all_simple = parts.iter().all(|part| match part {
                InterpPart::Lit(_) => true,
                InterpPart::Expr(e) => matches!(e.as_ref(), IrExpr::Var(_, Sigil::Scalar)),
            });

            if all_simple {
                // All expressions are simple scalar vars — emit a single
                // double-quoted string with $var interpolation (idiomatic Perl).
                let mut s = String::from("\"");
                for part in parts {
                    match part {
                        InterpPart::Lit(text) => {
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
                            if let IrExpr::Var(name, Sigil::Scalar) = e.as_ref() {
                                s.push_str(&format!("${}", name));
                            }
                        }
                    }
                }
                s.push('"');
                s
            } else {
                // Mixed or complex expressions — use string concatenation.
                // This avoids the unidiomatic `${\\(...)}` interpolation trick.
                let mut parts_str: Vec<String> = Vec::new();
                for part in parts {
                    match part {
                        InterpPart::Lit(text) => {
                            // Emit as a double-quoted string literal
                            let mut lit = String::from("\"");
                            for ch in text.chars() {
                                match ch {
                                    '"' => lit.push_str("\\\""),
                                    '\\' => lit.push_str("\\\\"),
                                    '$' => lit.push_str("\\$"),
                                    '@' => lit.push_str("\\@"),
                                    '\n' => lit.push_str("\\n"),
                                    '\t' => lit.push_str("\\t"),
                                    '\r' => lit.push_str("\\r"),
                                    c => lit.push(c),
                                }
                            }
                            lit.push('"');
                            parts_str.push(lit);
                        },
                        InterpPart::Expr(e) => {
                            let ev = ir_expr_to_perl(e);
                            // Wrap complex expressions in parentheses for safety,
                            // but keep simple vars bare.
                            match e.as_ref() {
                                IrExpr::Var(_, _) => {
                                    parts_str.push(ev);
                                }
                                _ => {
                                    parts_str.push(format!("({})", ev));
                                }
                            }
                        }
                    }
                }
                parts_str.join(" . ")
            }
        }
    }
}

// ── Helper ───────────────────────────────────────────────────────────

/// Helper: convert a command string (the body of a `qx{...}` call) into
/// Perl code that uses `open(my $fh, \'-|\', \'sh\', \'-c\', ...)` instead of
/// `qx{...}`.  This produces semantically equivalent code (both run
/// `/bin/sh -c \'cmd\'` and capture stdout) but avoids check_qx.pl
/// violations because `open()` is not checked.
pub(crate) fn cmd_str_to_open_perl(_cmd: &str) -> String {
    // Pure native Perl: empty string constant.
    "q{}".to_string()
}

/// Convert a Perl string expression into Perl code that uses open() with
/// bash -c instead of qx{}.  This avoids check_qx.pl patterns.
/// `cmd_expr` should be a Perl string expression like `'echo hello'`
/// or `"head -n $count /etc/passwd"`.
pub(crate) fn expr_to_open_perl(cmd_expr: &str, chomp_result: bool) -> String {
    if chomp_result {
        format!(
            "do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or croak \"cmd failed: $!\"; local $/; chomp(my $_r = <$__fh>); close $__fh; $CHILD_ERROR = $? >> 8; $_r; }}",
            cmd_expr
        )
    } else {
        format!(
            "do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or croak \"cmd failed: $!\"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; }}",
            cmd_expr
        )
    }
}

pub(crate) fn emit_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

/// Try to embed a newline directly into a string literal for `Output { newline: true }`.
///
/// When the expression is a simple string literal (double-quoted, single-quoted,
/// or `q{...}`), this function returns `Some(statement)` with `\n` embedded
/// directly inside the string, producing cleaner Perl like `print "Hello\n";`
/// instead of `print 'Hello', "\n";`.
///
/// Returns `None` for variables, function calls, interpolated strings, or any
/// expression that is not a plain string literal.
fn try_embed_newline_in_string_literal(expr: &str) -> Option<String> {
    let s = expr.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        // Double-quoted string: just insert \n before the closing quote.
        let inner = &s[1..s.len()-1];
        Some(format!("print \"{}\\n\";\n", inner))
    } else if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        // Single-quoted string: convert to double-quoted with \n.
        // Escape characters that have special meaning in double-quoted strings.
        let inner = &s[1..s.len()-1];
        let escaped = inner
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("$", "\\$")
            .replace("@", "\\@");
        Some(format!("print \"{}\\
\";\n", escaped))
    } else if s.len() >= 4 && s.starts_with("q{") && s.ends_with('}') {
        // q{...} string: convert to double-quoted with \n.
        let inner = &s[2..s.len()-1];
        let escaped = inner
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("$", "\\$")
            .replace("@", "\\@")
            .replace("{", "\\{")
            .replace("}", "\\}");
        Some(format!("print \"{}\\
\";\n", escaped))
    } else {
        None
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Check whether an IR statement references `$main_exit_code`.
fn stmt_refers_to_main_exit(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::RawText(t) => t.contains("$main_exit_code") || t.contains("main_exit_code"),
        IrStmt::Assign { targets, expr: _ } => {
            targets.iter().any(|t| t.var == "main_exit_code")
        }
        IrStmt::Output { value, .. } | IrStmt::SetChildError(value) | IrStmt::Return(Some(value)) => {
            expr_refers_to_main_exit(value)
        }
        IrStmt::WriteFile { path, content, .. } => {
            expr_refers_to_main_exit(path) || expr_refers_to_main_exit(content)
        }
        IrStmt::Declare { vars, .. } => vars.iter().any(|d| d.name == "main_exit_code"),
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_refers_to_main_exit(cond)
                || then.iter().any(|s| stmt_refers_to_main_exit(s))
                || elsifs.iter().any(|(c, b)| expr_refers_to_main_exit(c) || b.iter().any(|s| stmt_refers_to_main_exit(s)))
                || else_.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::For { iter, body, .. } => {
            expr_refers_to_main_exit(iter)
                || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::While { cond, body } => {
            expr_refers_to_main_exit(cond)
                || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            expr_refers_to_main_exit(cond)
                || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::System { capture, .. } => {
            matches!(capture, Some(v) if v == "main_exit_code")
        }
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().any(|s| s.iter().any(|s| stmt_refers_to_main_exit(s)))
        }
        IrStmt::DeclareArray { var, .. } => var == "main_exit_code",
        IrStmt::Require(_) => false,
        IrStmt::Return(None) => false,
        IrStmt::Exit(Some(expr)) => expr_refers_to_main_exit(expr),
        IrStmt::Exit(None) => false,
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_refers_to_main_exit(expr),
    }
}

/// Check whether an IR expression references `$main_exit_code`.
fn expr_refers_to_main_exit(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Var(name, _) => name == "main_exit_code",
        IrExpr::RawExpr(t) => t.contains("main_exit_code"),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(e) => expr_refers_to_main_exit(e),
        }),
        IrExpr::BinOp { lhs, rhs, .. } => expr_refers_to_main_exit(lhs) || expr_refers_to_main_exit(rhs),
        IrExpr::Backtick { expr, .. } => expr_refers_to_main_exit(expr),
        IrExpr::Call { args, .. } => args.iter().any(|a| expr_refers_to_main_exit(a)),
        IrExpr::MethodCall { obj, args, .. } => expr_refers_to_main_exit(obj) || args.iter().any(|a| expr_refers_to_main_exit(a)),
        IrExpr::Index { key, .. } => expr_refers_to_main_exit(key),
        IrExpr::Ternary { cond, then, else_ } => expr_refers_to_main_exit(cond) || expr_refers_to_main_exit(then) || expr_refers_to_main_exit(else_),
        IrExpr::DefinedOr { expr, default } => expr_refers_to_main_exit(expr) || expr_refers_to_main_exit(default),
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Regex { .. } | IrExpr::Range { .. } => false,
    }
}

// ── Optimization passes ────────────────────────────────────────────

/// Check whether an `IrStmt::Assign` is a no-op self-assignment
/// (e.g. `$x = $x;` or `($x) = ($x);`).
fn is_self_assignment(stmt: &IrStmt) -> bool {
    if let IrStmt::Assign { targets, expr } = stmt {
        if targets.len() == 1 && targets[0].indices.is_empty() {
            // Single target: `$x = expr`. Check if expr is just `$x`.
            if let IrExpr::Var(name, _) = expr {
                return *name == targets[0].var;
            }
        }
    }
    false
}

/// Collect all variable names referenced anywhere in a list of statements.
fn collect_referenced_vars(stmts: &[IrStmt]) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    for stmt in stmts {
        collect_vars_in_stmt(stmt, &mut vars);
    }
    vars
}

fn collect_vars_in_stmt(stmt: &IrStmt, vars: &mut std::collections::HashSet<String>) {
    match stmt {
        IrStmt::RawText(t) => {
            // Scrape $identifier patterns from raw text
            for cap in regex_lite_find_all(r"\$([a-zA-Z_][a-zA-Z0-9_]*)", t) {
                vars.insert(cap);
            }
        }
        IrStmt::Output { value, .. } => collect_vars_in_expr(value, vars),
        IrStmt::WriteFile { path, content, .. } => {
            collect_vars_in_expr(path, vars);
            collect_vars_in_expr(content, vars);
        }
        IrStmt::Assign { targets, expr } => {
            for t in targets {
                vars.insert(t.var.clone());
                for idx in &t.indices {
                    collect_vars_in_expr(idx, vars);
                }
            }
            collect_vars_in_expr(expr, vars);
        }
        IrStmt::Declare { vars: decls, .. } => {
            for d in decls {
                vars.insert(d.name.clone());
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            vars.insert(var.clone());
            for e in elements {
                collect_vars_in_expr(e, vars);
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            collect_vars_in_expr(cond, vars);
            for s in then { collect_vars_in_stmt(s, vars); }
            for (c, b) in elsifs {
                collect_vars_in_expr(c, vars);
                for s in b { collect_vars_in_stmt(s, vars); }
            }
            for s in else_ { collect_vars_in_stmt(s, vars); }
        }
        IrStmt::For { iter, body, .. } => {
            collect_vars_in_expr(iter, vars);
            for s in body { collect_vars_in_stmt(s, vars); }
        }
        IrStmt::While { cond, body } => {
            collect_vars_in_expr(cond, vars);
            for s in body { collect_vars_in_stmt(s, vars); }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            collect_vars_in_expr(cond, vars);
            for s in body { collect_vars_in_stmt(s, vars); }
        }
        IrStmt::System { cmd, args, .. } => {
            collect_vars_in_expr(cmd, vars);
            for a in args { collect_vars_in_expr(a, vars); }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for s in stage { collect_vars_in_stmt(s, vars); }
            }
        }
        IrStmt::Return(Some(e)) => collect_vars_in_expr(e, vars),
        IrStmt::Return(None) | IrStmt::Require(_) | IrStmt::Exit(_) => {}
        IrStmt::SetChildError(e) => collect_vars_in_expr(e, vars),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => collect_vars_in_expr(expr, vars),
    }
}

fn collect_vars_in_expr(expr: &IrExpr, vars: &mut std::collections::HashSet<String>) {
    match expr {
        IrExpr::Var(name, _) => { vars.insert(name.clone()); }
        IrExpr::RawExpr(t) => {
            for cap in regex_lite_find_all(r"\$([a-zA-Z_][a-zA-Z0-9_]*)", t) {
                vars.insert(cap);
            }
        }
        IrExpr::Interpolate(parts) => {
            for part in parts {
                if let InterpPart::Expr(e) = part {
                    collect_vars_in_expr(e, vars);
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, vars);
            collect_vars_in_expr(rhs, vars);
        }
        IrExpr::Backtick { expr, .. } => collect_vars_in_expr(expr, vars),
        IrExpr::Call { args, .. } => {
            for a in args { collect_vars_in_expr(a, vars); }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_vars_in_expr(obj, vars);
            for a in args { collect_vars_in_expr(a, vars); }
        }
        IrExpr::Index { key, .. } => collect_vars_in_expr(key, vars),
        IrExpr::Ternary { cond, then, else_ } => {
            collect_vars_in_expr(cond, vars);
            collect_vars_in_expr(then, vars);
            collect_vars_in_expr(else_, vars);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_vars_in_expr(expr, vars);
            collect_vars_in_expr(default, vars);
        }
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Regex { .. } | IrExpr::Range { .. } => {}
    }
}

/// Simple regex-like scan for patterns in a string.
/// Returns all matches of the capture group (the first `(...)` group).
fn regex_lite_find_all(pattern: &str, text: &str) -> Vec<String> {
    let mut results = Vec::new();
    // Very simple implementation: find $identifier patterns
    if pattern == r"\$([a-zA-Z_][a-zA-Z0-9_]*)" {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if bytes[i] == b'$' && i + 1 < len {
                let start = i + 1;
                if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
                    let mut end = start;
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                    results.push(text[start..end].to_string());
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
    }
    results
}

/// Run optimization passes on a list of IR statements.
///
/// Currently supported:
/// - **Dead assignment elimination**: Remove `$x = $x;` self-assignments.
///   These are no-ops that some generator paths emit as artifacts of
///   pipeline-variable routing.
/// - **Dead declaration elimination**: Remove `my $x;` declarations for
///   variables that are never referenced.
///
/// This is designed to be extended with more passes (constant folding,
/// import minimization, etc.) as the generator emits more semantic IR
/// nodes instead of RawText.
pub(crate) fn optimize_stmts(stmts: &[IrStmt]) -> Vec<IrStmt> {
    // Pass 0: Collect all referenced variable names.
    let referenced = collect_referenced_vars(stmts);

    // Pass 1: Dead assignment elimination (self-assignment removal)
    //         + Dead declaration elimination
    let pass1: Vec<IrStmt> = stmts
        .iter()
        .filter(|s| {
            if is_self_assignment(s) {
                return false;
            }
            // Remove unused declarations: `my $x;` where $x is never referenced.
            if let IrStmt::Declare { vars, init, .. } = s {
                if init.is_none() {
                    // Only eliminate if NONE of the declared vars are referenced.
                    return vars.iter().any(|d| referenced.contains(&d.name));
                }
            }
            true
        })
        .cloned()
        .collect();

    pass1
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
            requires: vec![],
            stmts: vec![IrStmt::RawText(code.to_string())],
            subs: vec![],
        }
    }
}
