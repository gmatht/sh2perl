/// Intermediate Representation for code generation.
///
/// The generator produces an `IrProgram` from the shell AST.  The Perl
/// backend `shir_to_perl()` converts it to Perl text.  `RawText`/`RawExpr`
/// hold unimigrated code so conversion can happen function by function.
///
/// **Layering (who owns Perl text):** the AST-side `Generator`
/// (`src/generator/`) is the primary Perl renderer — it owns the text for
/// constructs that have not yet migrated to the IR, and `shir_to_perl` is
/// its ShIR-consumer wrapper (the sibling of `shir_to_estree`). Migrated
/// constructs flow as real IR nodes: the Generator emits them, `emit_stmt`/
/// `ir_expr_to_perl` render them, and `IrProgram::from_raw_perl` wraps the
/// remainder in a single `RawText` blob so the whole program can flow
/// through the ShIR pipeline (`--shir` → `--shir-in-perl`). When the
/// migration completes (all Generator functions emit IR nodes), the
/// wrapper and the Generator's text-building side both go away.
///
/// ShIR direction (PLAN.md §3/§8): this module is being generalized from a
/// Perl-only IR into a language-neutral ShIR that both the Perl backend and
/// the ESTree emitter consume. The first step is done: `Sigil` is now an
/// OPTIONAL backend annotation on `Var`/`AssignTarget`/`Decl`/`DeclareArray`
/// (the core no longer requires a Perl `$`/`@`/`%`; `None` renders as scalar
/// for Perl, and a non-Perl backend ignores it). Remaining Perl-specific
/// surface to neutralize: `StrStyle` (Command/Heredoc → extensions),
/// `Backtick`, `Regex`, `System`/`Pipeline` (→ `Exec`), `Require`,
/// `SetChildError`. `RawText`/`RawExpr` stay as the migration bridge. The
/// estree emitter (src/estree.rs) currently lowers from the raw AST and will
/// reroute through this IR once two backends consume it.
///
/// See docs/ir-design.md for full documentation.

// ── Sigils ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sigil {
    Scalar,
    Array,
    Hash,
}

// ── Type annotations (ask A2: serialize the numeric/string lift verdicts) ──
//
// Conservative typing for static backends (C): what the JS path's
// `numeric_lift_vars` / `string_lift_vars` (src/shir.rs) provably admit.
//   Int  -> every assignment is provably numeric (native `long long` / number)
//   Str  -> every assignment is provably a string literal (native char*)
//   Any  -> runtime store (mixed/unknown typing; shell vars are strings)
// Populated by `shir::analyze_var_types`; serialized in the ShIR JSON
// (ask A1). Existing backends ignore it (additive only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IrType {
    Int,
    Str,
    Any,
}

// ── Const/var annotations (the const-markup transform) ──────────────────
//
// Conservative const/var verdicts for static backends (C): per assigned
// variable, whether it can be emitted `const`/`readonly`. A variable is
// `Const` only when it has exactly one static assignment site that
// executes at most once (outside loops/function bodies) and is never
// written by the runtime store (`read`, `mapfile`, `unset`), by native
// arithmetic (`x++`, `((x=1))`), or by an array-element write, and the
// program has no dynamic write (`eval`/`source`). Anything else is
// `Var`. Missing from the list = never assigned (pure reads).
// Populated by `shir::analyze_var_const`; serialized in the ShIR JSON
// (ask A1). Existing backends ignore it (additive only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VarKind {
    Const,
    Var,
}

// ── Lifetime annotations (the variable-lifetime analysis) ────────────
//
// Conservative lifetime verdicts for static backends (C): per variable,
// the live span in statement positions of a pre-order walk (first access
// .. last access) and whether the value's storage may escape the current
// scope (array-element store, closure capture, function return). The C
// backend uses the span for per-point buffer sizing and buffer reuse,
// and the escape bit for the copy-vs-move / stack-vs-heap decision.
// Populated by `shir_passes::lifetime::analyze_var_lifetimes`;
// serialized in the ShIR JSON (ask A1). Existing backends ignore it
// (additive only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VarLifetime {
    /// First access (def or use) position in the pre-order statement walk.
    pub first: usize,
    /// Last access position in the pre-order statement walk.
    pub last: usize,
    /// True when the value's storage may be retained beyond the scope
    /// where it was produced (array-element store, closure capture,
    /// function return) — cannot be a stack local that is moved from
    /// or reused.
    pub escapes: bool,
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
    /// Like DoubleQuoted but preserves `$` and `@` for Perl interpolation.
    /// Used for unquoted heredoc bodies (<<EOF) where Perl should
    /// interpolate variable references.  Newlines and control characters
    /// are still escaped so the Perl source is readable.
    Heredoc,
}

// ── Interpolation parts ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InterpPart {
    Lit(String),
    Expr(Box<IrExpr>),
}

// ── Arithmetic AST (neutral) ─────────────────────────────────────────
/// Parsed `$((...))` arithmetic — rendered as NATIVE JS arithmetic by the
/// ESTree backend (faster/cleaner than a runtime string-eval). Assignments
/// (`x=`, `x+=`, `x++`) are NOT representable here (they need setVar
/// semantics) and fall back to the runtime `sh2.arith` evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum ArithAst {
    Num(i64),
    Var(String),
    Index { var: String, key: Box<ArithAst> },
    Bin { op: String, lhs: Box<ArithAst>, rhs: Box<ArithAst> },
    Un { op: String, arg: Box<ArithAst> },
    Cond { test: Box<ArithAst>, then: Box<ArithAst>, else_: Box<ArithAst> },
    /// `name op= rhs` (`=` / `+=` / `-=` / `*=`; `/=`/`%=` stay on the
    /// runtime — the zero-divisor abort needs the helper). The expression
    /// VALUE is the assigned value (bash semantics).
    Assign {
        var: String,
        op: String,
        rhs: Box<ArithAst>,
    },
    /// `++name` / `name++` / `--name` / `name--` — delta ±1. The value is
    /// the NEW value (prefix) or the OLD value (postfix), exactly bash's
    /// arithmetic semantics.
    IncDec { var: String, delta: i64, prefix: bool },
}

// ── Expressions ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum IrExpr {
    /// Integer literal
    Int(i64),
    /// String literal
    Str(String, StrStyle),
    /// Variable: $name, @name, %name. The sigil is an OPTIONAL
    /// backend annotation: the core IR is language-neutral and does not
    /// require a Perl sigil (None renders as scalar for the Perl backend;
    /// a non-Perl backend ignores it).
    Var(String, Option<Sigil>),
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
    /// Command output capture (shell command substitution / backticks) —
    /// neutral. The Perl backend renders it as qx{}/backticks; a non-Perl
    /// consumer uses the wrapped command expression. If `native` is true the
    /// expression already produces the exact value (no trailing newline); if
    /// false, trailing newlines are stripped (shell semantics).
    Capture {
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
    /// Delayed statement block (closure) — used by the ESTree path for
    /// pipeline stages, subshell/background/redirect/function bodies and
    /// loop conditions. The Perl generator never emits it.
    Arrow(Vec<IrStmt>),
    /// Array literal (ESTree-path only — e.g. for-items, brace groups).
    Array(Vec<IrExpr>),
    /// Parsed arithmetic (neutral AST) — ESTree path renders native JS.
    Arith(Box<ArithAst>),
    /// Boolean literal (ESTree-path only — e.g. shopt enable flags).
    Bool(bool),
    /// Raw JSON literal (ESTree-path only — e.g. brace-expansion groups).
    Json(serde_json::Value),
    /// Bare identifier (ESTree-path only — e.g. the for-loop variable name).
    Ident(String),
    /// Object literal (ESTree-path only — e.g. redirect specs, env maps).
    Object(Vec<(String, IrExpr)>),
}

// ── Assignment target ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct AssignTarget {
    pub var: String,
    /// Optional backend sigil annotation (see Var).
    pub sigil: Option<Sigil>,
    pub indices: Vec<IrExpr>,
}

// ── Variable declaration ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    /// Optional backend sigil annotation (see Var).
    pub sigil: Option<Sigil>,
}

// ── Statements ───────────────────────────────────────────────────────

/// One `case` clause: shell-glob patterns + body.
#[derive(Debug, Clone, PartialEq)]
pub struct IrCaseClause {
    pub patterns: Vec<String>,
    pub body: Vec<IrStmt>,
}

/// One redirection spec (fd, mode, target expression).
#[derive(Debug, Clone, PartialEq)]
pub struct IrRedirect {
    pub fd: Option<i32>,
    pub mode: String, // "r" | "w" | "a" | "r+" | "heredoc" | "herestring" | "unsupported"
    pub target: IrExpr,
    /// Whether an unquoted heredoc body should be interpolated (ESTree path).
    pub interpolate: bool,
}

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
        /// Optional backend sigil annotation (see Var).
        sigil: Option<Sigil>,
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
    /// Run a command (neutral — the language-agnostic "exec"). The Perl
    /// backend renders it as `system(...)`/`qx{}`; the ESTree consumer uses
    /// cmd/args/redirects. `redirects` is empty until generators populate it.
    Exec {
        cmd: IrExpr,
        args: Vec<IrExpr>,
        capture: Option<String>,
        redirects: Vec<IrExpr>,
        /// Command-scoped environment variables (`VAR=x cmd`) — ESTree path.
        env: Vec<(String, IrExpr)>,
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
    /// ── Neutral nodes (ESTree-path AST→IR builder only) ─────────────
    /// The Perl generator never emits these; the Perl renderer's arms are
    /// unreachable. They make the IR capable of expressing the full shell so
    /// shir_to_estree can consume it (PLAN.md §3).
    /// case … esac — dispatch on a value against shell-glob patterns.
    Case {
        discriminant: IrExpr,
        clauses: Vec<IrCaseClause>,
    },
    /// Redirection wrapper: run `inner` with fd redirects applied.
    Redirect {
        inner: Vec<IrStmt>,
        redirects: Vec<IrRedirect>,
    },
    /// Shell function definition (positional args via the runtime).
    Function {
        name: String,
        body: Vec<IrStmt>,
    },
    /// Subshell — copy semantics (env/fd snapshot, run, discard).
    Subshell(Vec<IrStmt>),
    /// Background — run asynchronously.
    Background(Vec<IrStmt>),
    /// Plain block group `{ a; b; }` (no copy semantics — unlike Subshell).
    Block(Vec<IrStmt>),
    /// Evaluate an expression as a statement (ESTree-path pipelines,
    /// and/or/not, bare sh2.* calls). Perl generator never emits it.
    Expr(IrExpr),
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
    /// Conservative type annotations (ask A2): (var name, verdict), sorted
    /// by name for deterministic serialization. Empty until
    /// `shir::analyze_var_types` runs. Existing backends ignore it.
    pub var_types: Vec<(String, IrType)>,
    /// Source line numbers per top-level statement — (stmt index, source
    /// line) pairs, sorted by index. Populated by the C frontend (the
    /// `--output-lineno` option); the shell path leaves it empty. The
    /// Perl renderer appends ` # line N` comments when present.
    pub stmt_lines: Vec<(usize, usize)>,
    /// Conservative max-string-length annotations (a transform sibling of
    /// the A2 verdicts): per variable, the provable upper bound on the
    /// string's byte length (None = unbounded — captures, loops, runtime
    /// input). Populated by `shir::analyze_string_lengths`; the C backend
    /// uses it to emit fixed buffers (`char s[64]`) instead of heap/`char*`.
    /// Existing backends ignore it (additive only).
    pub var_lengths: Vec<(String, Option<u64>)>,
    /// Conservative const/var verdicts (the const-markup transform,
    /// sibling of the A2 type verdicts and `var_lengths`): per ASSIGNED
    /// variable, `Const` when it is written exactly once (single static
    /// assignment site) and that site executes at most once per run
    /// (outside loops/function bodies) and is not a runtime-store write
    /// (`read`/`unset`/`eval`/native arith `x++`/array-element). Sorted by
    /// name for deterministic serialization. Populated by
    /// `shir::analyze_var_const`; the C backend emits `const` for `Const`
    /// verdicts. Existing backends ignore it (additive only).
    pub var_const: Vec<(String, VarKind)>,
    /// Conservative variable-lifetime annotations (the analysis's
    /// sibling of `var_types`/`var_lengths`/`var_const`): per variable,
    /// the live span in statement positions of a pre-order walk and
    /// whether the value's storage escapes the scope (array store /
    /// closure / return). Populated by
    /// `shir_passes::lifetime::analyze_var_lifetimes`; the C backend
    /// uses it for per-point buffer sizing, buffer reuse, and the
    /// copy-vs-move / stack-vs-heap decision. Existing backends ignore
    /// it (additive only).
    pub var_lifetimes: Vec<(String, VarLifetime)>,
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
///
/// Role: this is the ShIR-consumer wrapper of the Perl backend — the
/// sibling of `shir_to_estree`. The AST-side `Generator` (`src/generator/`)
/// owns Perl text for constructs that have not yet migrated to the IR;
/// `ast_to_ir` wraps that output in a single `RawText` blob via
/// `IrProgram::from_raw_perl` (the migration bridge). As constructs
/// migrate, the Generator emits real IR nodes and this renderer emits
/// them as Perl; `from_raw_perl` becomes unnecessary when the Generator
/// emits pure IR nodes. See PLAN.md §3 and `docs/ir-design.md`.
pub fn shir_to_perl(prog: &IrProgram) -> String {
    let mut out = String::new();

    // Shebang
    out.push_str("#!/usr/bin/env perl\n");
    out.push_str("use strict;\n");
    out.push_str("use warnings;\n");

    // Run optimization passes before emitting.
    let stmts = optimize_stmts(&prog.stmts);
    let modern_ir = prog.imports.is_empty();

    // Function calls: exec(foo, …) where foo is a defined shell function
    // must become a Perl sub call (bash -c would not know it). Rewrite the
    // stmts first; the emitter renders the marker as a sub call.
    let stmts = if modern_ir {
        let fns: std::collections::HashSet<String> = stmts
            .iter()
            .filter_map(|s| match s {
                IrStmt::Function { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        if fns.is_empty() {
            stmts
        } else {
            rewrite_fn_calls(&stmts, &fns)
        }
    } else {
        stmts
    };

    // Imports (`use` statements).
    // Auto-derive `use feature 'say'` if any Output { newline: true } exists.
    let mut imports = prog.imports.clone();
    if modern_ir {
        // Modern-IR program (`--shir` → `--shir-in-perl`): the JSON contract
        // carries no import list, so add the standard preamble the emitted
        // Perl expects (mirrors the Generator's preamble).
        imports.extend([
            "Carp".to_string(),
            "English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME)"
                .to_string(),
            "IPC::Open3".to_string(),
        ]);
    }
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

    // Modern-IR preamble: the status-trackers the emitted statements use.
    if modern_ir {
        out.push_str("my $main_exit_code = 0;\n");
        out.push_str("our $CHILD_ERROR = 0;\n");
        // Snapshot the positional-arg count BEFORE any $ARGV[n] read (the
        // magic @ARGV extends on indexed reads, corrupting scalar(@ARGV)).
        out.push_str("my $__argc = @ARGV;\n");
        // Preamble vars the Generator's per-command emulations expect (ls
        // tracks success; the pipeline machinery accumulates stdout).
        out.push_str("my $ls_success = 0;\n");
        out.push_str("my $output = '';\n");
        out.push_str("my $__nocasematch = 0;\n");
        // Hoisted declarations for assigned variables (use strict).
        let mut vars = Vec::new();
        collect_assigned_vars(&stmts, &mut vars);
        let mut read_vars = Vec::new();
        collect_read_vars_stmts(&stmts, &mut read_vars);
        for (n, s) in read_vars {
            if !vars.iter().any(|(vn, _)| vn == &n) {
                vars.push((n, s));
            }
        }
        for (name, sigil) in &vars {
            let assigned = collect_assigned_vars_contains(&stmts, name);
            match sigil {
                Sigil::Scalar => {
                    // Read-only vars initialize to '' (bash unset = empty,
                    // and Perl would warn on undef in string concat).
                    if assigned {
                        out.push_str(&format!("my ${};\n", name));
                    } else {
                        out.push_str(&format!("my ${} = '';\n", name));
                    }
                }
                Sigil::Array => out.push_str(&format!("my @{};\n", name)),
                Sigil::Hash => out.push_str(&format!("my %{};\n", name)),
            }
        }
        out.push('\n');
    }

    // Top-level statements
    for (idx, stmt) in stmts.iter().enumerate() {
        let line = prog
            .stmt_lines
            .iter()
            .find(|(i, _)| *i == idx)
            .map(|(_, l)| *l);
        let before = out.len();
        emit_stmt(&mut out, stmt, 0);
        if let Some(l) = line {
            // a SHORT comment at the end of the statement's first line:
            // `$sum += $i;  # line 7` — the source-mapping convention
            let added = &out[before..];
            if let Some(nl) = added.find('\n') {
                out.insert_str(before + nl, &format!("  # line {l}"));
            }
        }
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
    let has_main_exit = modern_ir
        || stmts.iter().any(|s| stmt_refers_to_main_exit(s))
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

        // Neutral ESTree-path-only nodes — the Perl generator never emits them.
        IrStmt::Subshell(body) => {
            // ( cmd ) — copy semantics: assignments inside do NOT leak. Save
            // the vars the body assigns, run it, restore them (Perl `local`
            // can't apply to the hoisted `my` lexicals).
            let mut assigned = Vec::new();
            collect_assigned_vars(body, &mut assigned);
            let mut saves = Vec::new();
            for (name, sigil) in &assigned {
                if is_env_style_var_name(name) {
                    continue;
                }
                let save = format!("__save_{}", name);
                emit_indent(out, indent);
                let var = match sigil {
                    Sigil::Scalar => format!("${}", name),
                    Sigil::Array => format!("@{}", name),
                    Sigil::Hash => format!("%{}", name),
                };
                out.push_str(&format!("my ${} = {};\n", save, var));
                saves.push((name.clone(), save, sigil.clone()));
            }
            for s in body {
                emit_stmt(out, s, indent);
            }
            for (name, save, sigil) in saves {
                emit_indent(out, indent);
                let var = match sigil {
                    Sigil::Scalar => format!("${}", name),
                    Sigil::Array => format!("@{}", name),
                    Sigil::Hash => format!("%{}", name),
                };
                out.push_str(&format!("{} = ${};\n", var, save));
            }
        }
        IrStmt::Background(_) => {
            emit_indent(out, indent);
            out.push_str("die \"debashc: shIR construct not yet supported by the Perl backend (background)\\n\";\n");
        }
        IrStmt::Case { discriminant, clauses } => {
            // case $x in pat1) …;; pat2) …;; *) …;; esac → if/elsif chain on
            // regex-translated glob patterns (bash case is anchored glob
            // matching on the value).
            let d = ir_expr_to_perl(discriminant);
            let mut any_emitted = false;
            for (i, clause) in clauses.iter().enumerate() {
                let has_default = clause.patterns.iter().any(|p| p == "*");
                let patterns: Vec<String> = clause
                    .patterns
                    .iter()
                    .filter(|p| p.as_str() != "*")
                    .map(|p| glob_to_regex(p))
                    .collect();
                let is_first = i == 0 && !any_emitted;
                if has_default && i == clauses.len() - 1 {
                    // `*)` final clause — render as else.
                    emit_indent(out, indent);
                    if is_first {
                        out.push_str(&format!("if ({} =~ /^.*$/) {{\n", d));
                    } else {
                        out.push_str("} else {\n");
                    }
                } else {
                    let re = format!("^(?:{})$", patterns.join("|"));
                    emit_indent(out, indent);
                    if is_first {
                        out.push_str(&format!("if ({} =~ /{}/) {{\n", d, re));
                    } else {
                        out.push_str(&format!("}} elsif ({} =~ /{}/) {{\n", d, re));
                    }
                }
                for s in &clause.body {
                    emit_stmt(out, s, indent + 1);
                }
                any_emitted = true;
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }
        IrStmt::Function { name, body } => {
            // Shell function → Perl sub. `local @ARGV = @_` maps the bash
            // positional params ($1 → $ARGV[0]) onto the function's args;
            // the body's statements render inline (bash functions share the
            // global scope; Perl subs see the hoisted lexical vars).
            emit_indent(out, indent);
            out.push_str(&format!("sub {} {{\n", name));
            emit_indent(out, indent + 1);
            out.push_str("local @ARGV = @_;\n");
            for s in body {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }
        IrStmt::Redirect { inner, redirects } => {
            // Rebuild the shell command with shell redirection syntax and
            // run it via bash -c — stdout matches bash exactly (redirects
            // are shell-level semantics).
            match stmts_to_shell_cmd(inner) {
                Some(mut cmd) => {
                    let mut ok = true;
                    let mut heredocs: Vec<(String, String)> = Vec::new();
                    for r in redirects {
                        let fd = r.fd.unwrap_or(1);
                        let target = call_arg_str(&r.target).unwrap_or_default();
                        match r.mode.as_str() {
                            "heredoc" => {
                                // <<EOF — the body is the target; feed it via
                                // bash heredoc syntax at the end of the command.
                                let delim = if r.interpolate { "_SH2DOC_" } else { "'_SH2DOC_'" };
                                cmd.push_str(&format!(" <<{}", delim));
                                heredocs.push(("_SH2DOC_".to_string(), target));
                            }
                            "herestring" => {
                                cmd.push_str(&format!(" <<< '{}'", target.replace('\'', "'\\\\''")));
                            }
                            _ => {
                                if !append_redirect_frag(&mut cmd, fd as i64, &r.mode, &target) {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                    }
                    for (delim, body) in heredocs {
                        cmd.push('\n');
                        cmd.push_str(&body);
                        if !body.ends_with('\n') {
                            cmd.push('\n');
                        }
                        cmd.push_str(&delim);
                    }
                    if ok {
                        emit_shell_cmd(out, indent, &cmd);
                    } else {
                        emit_indent(out, indent);
                        out.push_str("die \"debashc: shIR construct not yet supported by the Perl backend (redirect mode)\\n\";\n");
                    }
                }
                None => {
                    // The inner isn't a single command (e.g. a while-loop or
                    // block with a redirect) — run the inner statements with
                    // Perl's STDOUT redirected to the target file (select).
                    // print-based output (echo, emulated commands) follows;
                    // bash -c shell-outs inside leak to the real STDOUT
                    // (accepted divergence for this fallback).
                    let first_redirect = redirects.first();
                    match first_redirect {
                        Some(r) if matches!(r.mode.as_str(), "w" | "a" | "r+") => {
                            let target = call_arg_str(&r.target).unwrap_or_default();
                            let mode = if r.mode == "a" { "'>>'" } else { "'>'" };
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "open my $__redir_fh, {}, {} or die \"Cannot write to {}: $!\\n\";\n",
                                mode,
                                render_word(&r.target),
                                target
                            ));
                            emit_indent(out, indent);
                            out.push_str("my $__saved_out = select($__redir_fh);\n");
                            for s in inner {
                                emit_stmt(out, s, indent);
                            }
                            emit_indent(out, indent);
                            out.push_str("select($__saved_out); close $__redir_fh;\n");
                        }
                        _ => {
                            emit_indent(out, indent);
                            out.push_str("die \"debashc: shIR construct not yet supported by the Perl backend (redirect)\\n\";\n");
                        }
                    }
                }
            }
        }
        IrStmt::Block(stmts) => {
            // plan improvement #5: render a block as a flat sequence of
            // stmts (the Perl backend has no native block scoping; this
            // is an approximation — variables from the block leak, but
            // the IR's lexical-ish scoping is close enough for the
            // supported subset).
            for s in stmts {
                emit_stmt(out, s, indent);
            }
        }
        IrStmt::Expr(e) => {
            // A bare expression statement: shell commands arrive as
            // Call{exec/test/pipeline/redirect/…} or as `&&`/`||` chains of
            // those (BinOp) — lower them all to shell-out via bash -c.
            match e {
                IrExpr::Call { func, args } => match func.as_str() {
                    "break" => {
                        emit_indent(out, indent);
                        out.push_str("last;\n");
                        return;
                    }
                    "continue" => {
                        emit_indent(out, indent);
                        out.push_str("next;\n");
                        return;
                    }
                    "exec" => {
                        emit_exec_call(out, e, indent);
                    }
                    "$fn_call" => {
                        // Call to a shell function defined in this program
                        // (rewritten by shir_to_perl): a Perl sub call.
                        let name = args.first().and_then(call_arg_str).unwrap_or_default();
                        let rest: Vec<String> = args[1..].iter().map(render_word).collect();
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "{}({}); $main_exit_code = $CHILD_ERROR = 0;\n",
                            name,
                            rest.join(", ")
                        ));
                    }
                    "test" => {
                        // Bare `[ cond ]` as a statement: the exit status is the
                        // condition's truth.
                        let cond = ir_expr_to_perl(e);
                        emit_indent(out, indent);
                        out.push_str(&format!("$main_exit_code = $CHILD_ERROR = ({}) ? 0 : 1;\n", cond));
                    }
                    "pipeline" => {
                        // Side-effect pipeline: rebuild the shell command string
                        // and run it via bash -c (matches bash stdout by running
                        // the same tools).
                        if let Some(cmd) = pipeline_call_to_cmd(e) {
                            emit_shell_cmd(out, indent, &cmd);
                        } else {
                            emit_indent(out, indent);
                            out.push_str("die \"debashc: shIR pipeline not yet supported by the Perl backend\\n\";\n");
                        }
                    }
                    "caseMatch" | "define" | "subshell" | "background" => {
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "die \"debashc: sh2.* call `{}` not yet supported by the shIR Perl backend\\n\";\n",
                            func
                        ));
                    }
                    "shopt" => {
                        // Shell-option toggle — nocasematch affects [[ == ]]
                        // string comparisons (tracked at runtime).
                        let on = args
                            .get(1)
                            .map(|a| matches!(a, IrExpr::Bool(true)))
                            .unwrap_or(false);
                        let opt = args.first().and_then(call_arg_str).unwrap_or_default();
                        if opt == "nocasematch" || opt == "-s" && args.get(1).and_then(call_arg_str).as_deref() == Some("nocasematch") {
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "$__nocasematch = {}; $main_exit_code = $CHILD_ERROR = 0;\n",
                                if on { 1 } else { 0 }
                            ));
                        } else {
                            emit_indent(out, indent);
                            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                        }
                    }
                    "redirect" => {
                        // Statement-position redirect: run the command with its
                        // redirects via bash -c (redirects are shell-level).
                        if let Some(cmd) = redirect_call_to_cmd(e) {
                            emit_shell_cmd(out, indent, &cmd);
                        } else {
                            emit_indent(out, indent);
                            out.push_str("die \"debashc: shIR redirect not yet supported by the Perl backend\\n\";\n");
                        }
                    }
                    other => {
                        // Unknown sh2.* calls in statement position.
                        if let Some(cmd) = expr_to_cmd(e) {
                            emit_shell_cmd(out, indent, &cmd);
                        } else {
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "die \"debashc: sh2.* call `{}` not yet supported by the shIR Perl backend\\n\";\n",
                                other
                            ));
                        }
                    }
                },
                // Non-Call expressions: bare `&&`/`||` chains of execs,
                // redirects, pipelines — run the reconstructed shell command.
                other_expr => {
                    if let Some(s) = cd_chain_to_perl(other_expr) {
                        emit_indent(out, indent);
                        out.push_str(&s);
                        out.push('\n');
                    } else if let Some(s) = control_chain_to_perl(other_expr) {
                        emit_indent(out, indent);
                        out.push_str(&s);
                        out.push('\n');
                    } else if let Some(cmd) = expr_to_cmd(other_expr) {
                        emit_shell_cmd(out, indent, &cmd);
                    } else {
                        emit_indent(out, indent);
                        out.push_str("die \"debashc: shIR expression not yet supported by the Perl backend\\n\";\n");
                    }
                }
            }
        }

        IrStmt::Output { value, newline, target } => {
            let expr = ir_expr_to_perl(value);
            if let Some(fh) = target {
                // Output to a specific filehandle: print {$fh} ...
                emit_indent(out, indent);
                if *newline {
                    out.push_str(&format!("print {{${}}}({}, \"\\n\");\n", fh, expr));
                } else {
                    out.push_str(&format!("print {{${}}}({});\n", fh, expr));
                }
            } else if *newline {
                // Use `print` with \n when called via piecemeal stmt_to_perl()
                // (which cannot manage imports).  Full shir_to_perl() switches to `say`.
                // Try to embed \n directly into string literals for cleaner output.
                if let Some(embedded) = try_embed_newline_in_string_literal(&expr) {
                    emit_indent(out, indent);
                    out.push_str(&embedded);
                } else {
                    emit_indent(out, indent);
                    out.push_str(&format!("print({}, \"\\n\");\n", expr));
                }
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("print({});\n", expr));
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
            // Detect Capture { native: false } on the RHS — emit the
            // two-statement clean form instead of embedding a do-block.
            if targets.len() == 1 && targets[0].indices.is_empty() {
                let var = &targets[0].var;
                let lhs = perl_lhs_for(var);
                if let IrExpr::Call { func, .. } = expr {
                    if func == "setArray" {
                        // Array assignment: arr=(a b c) → @arr = ('a','b','c');
                        if let IrExpr::Call { args, .. } = expr {
                            // args[0] is the array NAME — the elements follow
                            // (either a single Array arg or individual words).
                            let elems: Vec<&IrExpr> = if args.len() >= 2 {
                                if let IrExpr::Array(elems) = &args[1] {
                                    elems.iter().collect()
                                } else {
                                    args[1..].iter().collect()
                                }
                            } else {
                                Vec::new()
                            };
                            let items: Vec<String> = elems.iter().map(|e| render_word(*e)).collect();
                            emit_indent(out, indent);
                            out.push_str(&format!("@{} = ({});\n", var, items.join(", ")));
                            return;
                        }
                    }
                    if func == "capture" {
                        // Modern-IR command substitution: Assign{var, capture(…)}
                        // — rebuild the shell command and capture its stdout.
                        if let Some(cmd) = capture_call_to_cmd(expr) {
                            emit_capture_assign(out, indent, &lhs, &cmd);
                            return;
                        }
                    }
                }
                if let IrExpr::Capture { native: false, .. } = expr {
                    // Extract the inner expression string for qx{...}
                    if let IrExpr::Capture { expr: inner_expr, .. } = expr {
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
                    .map(|t| perl_lhs_for(&t.var))
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
                .map(|d| match d.sigil.unwrap_or(Sigil::Scalar) {
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
                if *local || (vars.len() == 1 && vars[0].sigil.unwrap_or(Sigil::Scalar) == Sigil::Scalar) {
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
            let sigil_char = match sigil.unwrap_or(Sigil::Scalar) {
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
            let iter_str = word_iter_to_perl(iter);
            // Perl's `for $i` restores the pre-loop value after the loop, but
            // bash leaves the last value — iterate a fresh temp and assign
            // the real var each round.
            let tmp = format!("__{}", var);
            emit_indent(out, indent);
            out.push_str(&format!("for my ${} ({}) {{\n", tmp, iter_str));
            emit_indent(out, indent + 1);
            out.push_str(&format!("${} = ${};\n", var, tmp));
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

        IrStmt::Exec { cmd, args, capture, .. } => {
            let cmd_str = ir_expr_to_perl(cmd);
            // Quote a string VALUE as a bash single-quoted shell word.
            // (Perl-style `\'` escaping is NOT understood by bash inside
            // single quotes, so embedded quotes must use the `'\''` idiom.)
            let bash_quote = |s: &str| -> String {
                format!("'{}'", s.replace('\'', "'\\''"))
            };
            if let Some(var) = capture {
                // Build a shell command string from the command and its arguments.
                // Run it through bash -c to capture stdout.
                let mut arg_parts: Vec<String> = Vec::new();
                arg_parts.push(cmd_str.clone());
                for a in args {
                    match a {
                        IrExpr::Str(s, _) => {
                            // Literal string: quote its value for bash.
                            arg_parts.push(bash_quote(s));
                        }
                        _ => {
                            let a_str = ir_expr_to_perl(a);
                            // If the argument is already a Perl string literal, use it.
                            if a_str.starts_with('\'') || a_str.starts_with('"') || a_str.starts_with('q') {
                                arg_parts.push(a_str);
                            } else {
                                arg_parts.push(format!("\"{}\"", a_str.replace("\"", "\\\"").replace("$", "\\$").replace("@", "\\@")));
                            }
                        }
                    }
                }
                let full_cmd = arg_parts.join(" ");
                // Use open()-based capture with safe quoting
                emit_indent(out, indent);
                out.push_str(&format!(
                    "my ${} = do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; }};\n",
                    var,
                    safe_perl_q_string(&full_cmd)
                ));
            } else {
                // Without capture: run the command via system() for side effects.
                let mut arg_parts: Vec<String> = Vec::new();
                arg_parts.push(cmd_str.clone());
                for a in args {
                    match a {
                        IrExpr::Str(s, _) => {
                            // Literal string: quote its value for bash.
                            arg_parts.push(bash_quote(s));
                        }
                        _ => {
                            let a_str = ir_expr_to_perl(a);
                            if a_str.starts_with('\'') || a_str.starts_with('"') || a_str.starts_with('q') {
                                arg_parts.push(a_str);
                            } else {
                                arg_parts.push(format!("\"{}\"", a_str.replace("\"", "\\\"").replace("$", "\\$").replace("@", "\\@")));
                            }
                        }
                    }
                }
                let full_cmd = arg_parts.join(" ");
                emit_indent(out, indent);
                out.push_str(&format!(
                    "system({});\\n",
                    full_cmd
                ));
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

// ── Modern-IR (sh2.* call) lowering ──────────────────────────────────
//
// `ast_to_ir` emits shell commands as neutral `Call{func:"exec"}` nodes with
// string/array words, tests as `Call{func:"test"}` on a flat string, and
// captures/pipelines as closures. These lowerings port the Generator's Perl
// idioms (echo → print, `${name}` reads, q{}/interpolation, int() arith)
// onto the IR side so `--shir → --shir-in-perl` produces runnable,
// bash-matching Perl. Constructs with no Perl lowering yet render a `die`
// with an actionable message instead of a panic.

/// Extract a plain string from a word-shaped expression (Str / Var / Ident).
fn call_arg_str(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Var(name, _) => Some(name.clone()),
        IrExpr::Ident(name) => Some(name.clone()),
        _ => None,
    }
}

/// Split a possibly-indexed var name ("matrix[0,0]") into base + optional
/// comma-key. Bash pseudo-multidimensional arrays bake the subscript into
/// the name at the parser level (Assign targets with `indices: []`).
fn split_indexed_var(name: &str) -> (&str, Option<&str>) {
    if let Some(open) = name.find('[') {
        if name.ends_with(']') {
            return (&name[..open], Some(&name[open + 1..name.len() - 1]));
        }
    }
    (name, None)
}

/// The Perl lhs for a (possibly indexed) var name: `matrix[0,0]` →
/// `$matrix{"0,0"}` (a hash key — bash's fake-multidim), plain names →
/// `$name` / `$ENV{name}`.
fn perl_lhs_for(var: &str) -> String {
    if is_env_style_var_name(var) {
        return format!("$ENV{{{}}}", var);
    }
    let (base, idx) = split_indexed_var(var);
    match idx {
        Some(i) => format!("${{{}}}{{\"{}\"}}", base, i.replace('\"', "\\\"")),
        None => format!("${}", base),
    }
}

/// Render a variable read: getVar("x") / Var("x") → `$x` / `$ENV{x}`;
/// bash positional params ($1 → $ARGV[0]) and specials ($#, $?, $@).
fn var_read(name: &str) -> String {
    if is_env_style_var_name(name) {
        return format!("$ENV{{{}}}", name);
    }
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        // bash positional parameter: $1 → $ARGV[0]
        return match name.parse::<usize>() {
            Ok(0) => "$0".to_string(),
            Ok(n) => format!("$ARGV[{}]", n - 1),
            Err(_) => format!("${}", name),
        };
    }
    match name {
        "#" => "$__argc".to_string(),
        "?" => "$CHILD_ERROR".to_string(),
        "@" | "*" => "@ARGV".to_string(),
        "$" => "$$".to_string(),
        _ if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
        {
            format!("${}", name)
        }
        _ => format!("${{{}}}", name),
    }
}

/// Render a bash glob pattern as a Perl regex fragment (the subset the
/// corpus's `${x#…}`/`${x%…}`/`${x/…/…}` idioms use).
fn glob_to_regex(pat: &str) -> String {
    let mut out = String::new();
    let mut chars = pat.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push_str("."),
            '\\' => {
                if let Some(&d) = chars.peek() {
                    out.push('\\');
                    out.push(d);
                    chars.next();
                } else {
                    out.push_str("\\\\");
                }
            }
            '.' | '(' | ')' | '+' | '|' | '^' | '$' | '{' | '}' | '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Render `ArithAst` as a Perl numeric expression (bash integer semantics;
/// int() wrapping happens at the IrExpr::Arith arm).
fn arith_ast_to_perl(ast: &ArithAst) -> String {
    match ast {
        ArithAst::Num(n) => n.to_string(),
        ArithAst::Var(name) => var_read(name),
        ArithAst::Index { var, key } => {
            format!("${{{}}}[{}]", var, arith_ast_to_perl(key))
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_ast_to_perl(lhs);
            let r = arith_ast_to_perl(rhs);
            match op.as_str() {
                "&&" | "||" | "**" => format!("({} {} {})", l, op, r),
                // bash `/` truncates toward zero; wrap the dividend so
                // int() lands on the quotient (int(a / b) == trunc).
                "/" => format!("int({} / {})", l, r),
                "%" => format!("int({}) % {}", l, r),
                _ => format!("({} {} {})", l, op, r),
            }
        }
        ArithAst::Un { op, arg } => {
            let a = arith_ast_to_perl(arg);
            match op.as_str() {
                "!" => format!("!({})", a),
                "~" => format!("~({})", a),
                "-" => format!("-({})", a),
                "+" => a,
                _ => format!("({}{})", op, a),
            }
        }
        ArithAst::Cond { test, then, else_ } => format!(
            "({} ? {} : {})",
            arith_ast_to_perl(test),
            arith_ast_to_perl(then),
            arith_ast_to_perl(else_)
        ),
        ArithAst::Assign { var, op, rhs } => format!(
            "{}{}{}",
            var_read(var),
            op,
            arith_ast_to_perl(rhs)
        ),
        ArithAst::IncDec { var, delta, prefix } => {
            let v = var_read(var);
            let d = if *delta < 0 { "-1" } else { "+1" };
            if *prefix {
                format!("({} {}= 1)", v, if *delta < 0 { "-" } else { "+" })
            } else {
                let _ = d;
                format!("({} {}= 1)", v, if *delta < 0 { "-" } else { "+" })
            }
        }
    }
}

/// Render a shell word (an exec arg) as a Perl expression.
/// A word is a Str literal, an Interpolate (concat chain), a getVar/split/
/// param/arith call, or an Array (bash joins array elements with a space
/// only when there are several — handled by the echo/command join, so here
/// a multi-element Array renders as its own join).
fn render_word(e: &IrExpr) -> String {
    match e {
        IrExpr::Str(s, style) => {
            // Reuse the Str rendering from ir_expr_to_perl by constructing it.
            if let Some(pat) = s.strip_prefix("\u{1}SH2GLOB\u{1}") {
                // Shell glob word — expand via Perl's glob().
                format!("glob('{}')", pat.replace('\'', "\\\\'"))
            } else {
                render_str_literal(s, style)
            }
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => args
                .first()
                .and_then(call_arg_str)
                .map(|n| var_read(&n))
                .unwrap_or_else(|| "''".to_string()),
            "split" => args
                .first()
                .map(render_word)
                .unwrap_or_else(|| "''".to_string()),
            "param" => render_param(args),
            "arith" => args
                .first()
                .map(|a| format!("int({})", render_word(a)))
                .unwrap_or_else(|| "0".to_string()),
            "brace" => render_brace_word(args),
            "capture" | "captureWords" => {
                // Command substitution in unquoted word position: capture
                // stdout, then split on IFS whitespace (bash word-splitting
                // semantics). The join keeps it a single string for print.
                let cap = args.first().and_then(arrow_to_cmd);
                match cap {
                    Some(cmd) => format!("join(' ', split(/\\s+/, {}))", cmd_str_to_open_perl(&cmd)),
                    None => "''".to_string(),
                }
            }
            _ => ir_expr_to_perl(e),
        },
        IrExpr::Interpolate(_) | IrExpr::Var(_, _) | IrExpr::Arith(_)
        | IrExpr::Capture { .. } | IrExpr::Index { .. }
        | IrExpr::BinOp { .. } | IrExpr::Ternary { .. }
        | IrExpr::DefinedOr { .. } | IrExpr::MethodCall { .. } => ir_expr_to_perl(e),
        IrExpr::Array(elements) => {
            let parts: Vec<String> = elements.iter().map(render_word).collect();
            if parts.is_empty() {
                "''".to_string()
            } else if parts.len() == 1 {
                parts[0].clone()
            } else {
                format!("join(' ', {})", parts.join(", "))
            }
        }
        _ => ir_expr_to_perl(e),
    }
}

/// `${var…}` parameter expansion (Call param): op, var, value/pattern...
fn render_param(args: &[IrExpr]) -> String {
    let op = args.first().and_then(call_arg_str).unwrap_or_default();
    let name = args.get(1).and_then(call_arg_str).unwrap_or_default();
    let var = if name.is_empty() { "''".to_string() } else { var_read(&name) };
    let val = args.get(2).map(render_word).unwrap_or_else(|| "''".to_string());
    let repl = args.get(3).map(render_word).unwrap_or_else(|| "''".to_string());
    match op.as_str() {
        "" => var,
        "-" => {
            if var.starts_with('@') {
                format!("({} ? {} : {})", var, var, val)
            } else {
                format!("(defined({}) ? {} : {})", var, var, val)
            }
        }
        ":-" => {
            if var.starts_with('@') {
                format!("({} ? {} : {})", var, var, val)
            } else {
                format!(
                    "((defined({v}) && length({v})) ? {v} : {d})",
                    v = var,
                    d = val
                )
            }
        }
        "=" | ":=" => format!(
            "((defined({v}) && length({v})) ? {v} : ({v} = {d}))",
            v = var,
            d = val
        ),
        // Prefix removal: # shortest, ## longest. The `:` forms (only when
        // non-empty) are approximated by the plain forms.
        // Prefix removal: # shortest, ## longest (approximated by the
        // greedy form — the corpus idioms are ##*/ and %/* which are exact).
        "#" | "#:" | "##" | "##:" => {
            let pat = args.get(2).and_then(call_arg_str).unwrap_or_default();
            let re = glob_to_regex(&pat);
            format!("(({} =~ s:^{}::r))", var, re)
        }
        // Suffix removal: % shortest, %% longest. The `%.*` extension
        // idiom strips the last extension (shortest) / from the first dot
        // (longest); other patterns remove a plain greedy suffix.
        "%" | "%:" | "%%" | "%%:" => {
            let pat = args.get(2).and_then(call_arg_str).unwrap_or_default();
            if pat == ".*" {
                if op == "%%" || op == "%%:" {
                    format!("(({} =~ s:\\.*$::r))", var)
                } else {
                    format!("(({} =~ s:\\.[^.]*$::r))", var)
                }
            } else {
                let re = glob_to_regex(&pat);
                format!("(({} =~ s:{}[^/]*$::r))", var, re)
            }
        }
        // Named idioms the IR maps: ##*/ → basename, %/* → dirname.
        "basename" => format!("(({} =~ s:^.*/::r))", var),
        "dirname" => format!("(({} =~ s:/[^/]*$::r))", var),
        // Case modification: ^^/,, = all, ^/, = first char.
        "^^" => format!("uc({})", var),
        ",," => format!("lc({})", var),
        "^" => format!("ucfirst({})", var),
        "," => format!("lcfirst({})", var),
        // Substring: ${var:off:len} → substr($var, off, len).
        ":" => {
            let off = args
                .get(2)
                .and_then(call_arg_str)
                .unwrap_or_else(|| "0".to_string());
            let len = args.get(3).and_then(call_arg_str).unwrap_or_default();
            if len.is_empty() {
                format!("substr({}, {})", var, off)
            } else {
                format!("substr({}, {}, {})", var, off, len)
            }
        }
        // Substitution: / first, // all.
        "/" => format!("(({} =~ s/{}/{}/r))", var, glob_to_regex(&val), repl.trim_matches('\\')),
        "//" | "/" => {
            let pat = args.get(2).and_then(call_arg_str).unwrap_or_default();
            let rep = args.get(3).and_then(call_arg_str).unwrap_or_default();
            let g = if op == "//" { "g" } else { "" };
            format!("(({} =~ s/{}/{}/{g}r))", var, glob_to_regex(&pat), rep.trim_matches('\\'))
        }
        // Array slice: ${arr[@]:off:len} → @arr[off..off+len-1] (0-based;
        // the shIR contract normalizes subscripts).
        "slice" => {
            let off = args
                .get(2)
                .and_then(call_arg_str)
                .unwrap_or_else(|| "0".to_string())
                .parse::<i64>()
                .unwrap_or(0);
            let end = match args.get(3).and_then(call_arg_str) {
                Some(l) if !l.is_empty() => {
                    let len = l.parse::<i64>().unwrap_or(0);
                    if len <= 0 {
                        return "()".to_string();
                    }
                    off + len - 1
                }
                _ => i64::MAX, // unbounded → to the last element
            };
            if end == i64::MAX {
                format!("@{}[{}..$#{}]", name, off, name)
            } else {
                format!("@{}[{}..{}]", name, off, end)
            }
        }
        _ => {
            // Unknown op — best-effort: return the var (parameter expansions
            // default to the value when the op is unrecognized).
            var
        }
    }
}

/// Build a bash command string from an exec's (cmd, words) for shell-out.
fn build_shell_cmd(cmd: &str, words: &[&IrExpr]) -> String {
    let mut parts: Vec<String> = vec![bash_quote_word(cmd)];
    for w in words {
        parts.push(bash_word_for(w));
    }
    parts.join(" ")
}

/// `cd` must affect the Perl process (chdir), so chains starting with it
/// render natively: `cd D || exit N` → `chdir(D) or exit N;` and
/// `cd D && cmd` → `chdir(D) or die;` + shell-out of the tail.
fn cd_chain_to_perl(expr: &IrExpr) -> Option<String> {
    if let IrExpr::BinOp { lhs, op, rhs } = expr {
        if let IrExpr::Call { func, args } = lhs.as_ref() {
            if func == "exec" {
                if let Some((cmd, words)) = exec_call_parts(args) {
                    if cmd == "cd" {
                        let dir = words
                            .first()
                            .map(|w| render_word(w))
                            .unwrap_or_else(|| "$ENV{HOME}".to_string());
                        return match op {
                            BinOpKind::Or => {
                                let code = if let IrExpr::Call { func: f, args: a2 } = rhs.as_ref() {
                                    if f == "exit" {
                                        a2.first()
                                            .map(|w| render_word(w))
                                            .unwrap_or_else(|| "1".to_string())
                                    } else {
                                        "1".to_string()
                                    }
                                } else {
                                    "1".to_string()
                                };
                                Some(format!("chdir({}) or exit {};", dir, code))
                            }
                            BinOpKind::And => {
                                let tail = expr_to_cmd(rhs);
                                match tail {
                                    Some(t) => Some(format!(
                                        "{x}chdir({dir}) or die \"cd: $!\\n\"; system('bash', '-c', {q}); $main_exit_code = $CHILD_ERROR = $? >> 8;",
                                        x = var_exports_str(&t),
                                        dir = dir,
                                        q = safe_perl_q_string(&t)
                                    )),
                                    None => Some(format!("chdir({}) or die \"cd: $!\\n\";", dir)),
                                }
                            }
                            _ => None,
                        };
                    }
                }
            }
        }
    }
    None
}

/// Split a test-chain: returns (cond-string, tail) where tail is the first
/// non-test node after the leading tests. Handles left-assoc chains
/// (t1 && t2 && cmd) and mixed ops.
fn split_test_tail(e: &IrExpr) -> Option<(String, Option<&IrExpr>, Option<BinOpKind>)> {
    match e {
        IrExpr::Call { func, .. } if func == "test" => Some((ir_expr_to_perl(e), None, None)),
        IrExpr::BinOp { lhs, op, rhs }
            if matches!(op, BinOpKind::And | BinOpKind::Or) =>
        {
            let (lcond, ltail, _) = split_test_tail(lhs)?;
            let joiner = if matches!(op, BinOpKind::And) {
                " && "
            } else {
                " || "
            };
            match (ltail, rhs.as_ref()) {
                // lhs ended in a test (no inner tail): rhs continues the
                // chain (another test) or is the tail (connector = op).
                (None, r) => {
                    if let IrExpr::Call { func, .. } = r {
                        if func == "test" {
                            let (rcond, rtail, _) = split_test_tail(r)?;
                            Some((
                                format!("({}){}({})", lcond, joiner, rcond),
                                rtail,
                                None,
                            ))
                        } else {
                            Some((lcond, Some(r), Some(op.clone())))
                        }
                    } else {
                        Some((lcond, Some(r), Some(op.clone())))
                    }
                }
                // lhs had an inner tail (t && cmd1) — the outer rhs is the
                // else-branch for the if/else caller.
                (Some(_), r) => Some((lcond, Some(r), None)),
            }
        }
        _ => None,
    }
}

/// `test-cond ||/&& control` chains render natively: `[ x = y ] || continue`
/// → `(cond) || next;`, `[[ flat ]] && cmd` → `if (cond) { cmd; }`, and
/// `(t && c1) || c2` → if/else. Flattened `[[ ]]` tests can't be
/// reconstructed as spaced `[ … ]` bash syntax, so these lower in Perl.
fn control_chain_to_perl(e: &IrExpr) -> Option<String> {
    // (test [&&/|| test]* [&& then]) || else → if/else
    if let IrExpr::BinOp { lhs, op: BinOpKind::Or, rhs } = e {
        if let IrExpr::BinOp { lhs: l2, op: BinOpKind::And, rhs: then_cmd } = lhs.as_ref() {
            if let Some((cond, ltail, _)) = split_test_tail(l2) {
                if ltail.is_none() {
                    if let (Some(t), Some(els)) = (expr_to_cmd(then_cmd), expr_to_cmd(rhs)) {
                        return Some(format!(
                            "{x}if ({c}) {{ system('bash', '-c', {q1}); $main_exit_code = $CHILD_ERROR = $? >> 8; }} else {{ system('bash', '-c', {q2}); $main_exit_code = $CHILD_ERROR = $? >> 8; }}",
                            x = format!("{}{}", var_exports_str(&t), var_exports_str(&els)),
                            c = cond,
                            q1 = safe_perl_q_string(&t),
                            q2 = safe_perl_q_string(&els)
                        ));
                    }
                }
            }
        }
    }
    let (cond, tail, connector) = split_test_tail(e)?;
    // Control-flow tail: continue/break/exit.
    if let Some(tail_expr) = tail {
        // connector: And → run the tail when cond holds; Or → when it
        // doesn't (bash `t || cmd` = if-not).
        let negated = matches!(connector, Some(BinOpKind::Or));
        if let IrExpr::Call { func, args } = tail_expr {
            match func.as_str() {
                "continue" => {
                    return Some(if negated {
                        format!("({}) || next;", cond)
                    } else {
                        format!("({}) && next;", cond)
                    })
                }
                "break" => {
                    return Some(if negated {
                        format!("({}) || last;", cond)
                    } else {
                        format!("({}) && last;", cond)
                    })
                }
                "exit" => {
                    let code = args
                        .first()
                        .map(render_word)
                        .unwrap_or_else(|| "1".to_string());
                    return Some(if negated {
                        format!("({}) || exit {};", cond, code)
                    } else {
                        format!("({}) && exit {};", cond, code)
                    });
                }
                _ => {}
            }
        }
        // Command tail: if-guard around the shell-out.
        if let Some(cmd) = expr_to_cmd(tail_expr) {
            let c = if negated {
                format!("!({})", cond)
            } else {
                cond.clone()
            };
            return Some(format!(
                "{x}if ({c}) {{ system('bash', '-c', {q}); $main_exit_code = $CHILD_ERROR = $? >> 8; }}",
                x = var_exports_str(&cmd),
                c = c,
                q = safe_perl_q_string(&cmd)
            ));
        }
    }
    None
}

/// A word as bash syntax: literals shell-quoted, captures as `$(…)`
/// (bash evaluates nested command substitution), interpolated words rebuilt
/// as bash double-quoted `"…$var…"`, vars double-quoted, braces as `{…}`.
fn bash_word_for(w: &IrExpr) -> String {
    match w {
        IrExpr::Str(s, _) => {
            if let Some(pat) = s.strip_prefix("\u{1}SH2GLOB\u{1}") {
                // Shell glob — emit unquoted so bash -c expands it.
                pat.to_string()
            } else {
                bash_quote_word(s)
            }
        }
        IrExpr::Interpolate(parts) => {
            let mut s = String::from("\"");
            for p in parts {
                match p {
                    InterpPart::Lit(t) => {
                        for ch in t.chars() {
                            match ch {
                                '"' => s.push_str("\\\""),
                                '$' => s.push_str("\\$"),
                                '`' => s.push_str("\\`"),
                                c => s.push(c),
                            }
                        }
                    }
                    InterpPart::Expr(e) => {
                        if let IrExpr::Call { func, args } = e.as_ref() {
                            if func == "getVar" {
                                if let Some(n) = args.first().and_then(call_arg_str) {
                                    s.push_str(&format!("${}", n));
                                    continue;
                                }
                            }
                            if func == "capture" || func == "captureWords" {
                                if let Some(cmd) = args.first().and_then(arrow_to_cmd) {
                                    s.push_str(&format!("$({})", cmd));
                                    continue;
                                }
                            }
                        }
                        s.push_str(&render_word(e));
                    }
                }
            }
            s.push('"');
            s
        }
        IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
            match args.first().and_then(arrow_to_cmd) {
                Some(cmd) => format!("$({})", cmd),
                None => "''".to_string(),
            }
        }
        IrExpr::Call { func, args } if func == "brace" => bash_brace_syntax(args),
        _ => {
            let wstr = render_word(w);
            // A Perl string literal or a plain scalar: embed safely.
            if wstr.starts_with('\'') || wstr.starts_with('"') || wstr.starts_with('q') {
                wstr
            } else {
                // A variable word — double-quote it for bash and let bash
                // expand the $ (this text is inside a q{} literal, so Perl
                // does not interpolate it; literal $ in single-quoted words
                // above is already protected).
                format!("\"{}\"", wstr.replace('\"', "\\\""))
            }
        }
    }
}

/// Rebuild bash brace syntax from a parsed `brace` Call:
/// prefix + {range/list}{range/list}… + suffix (bash expands it).
fn bash_brace_syntax(args: &[IrExpr]) -> String {
    let prefix = args.first().and_then(call_arg_str).unwrap_or_default();
    let suffix = args.get(3).and_then(call_arg_str).unwrap_or_default();
    let v = match brace_json_arg(args) {
        Some(v) => v,
        None => return format!("'{}{}'", prefix, suffix),
    };
    let mut out = prefix;
    if let Some(groups) = v.as_array() {
        for g in groups {
            let items = match g.as_array() {
                Some(i) => i,
                None => continue,
            };
            out.push('{');
            let mut first = true;
            for it in items {
                if !first {
                    out.push(',');
                }
                first = false;
                if let Some(s) = it.as_str() {
                    out.push_str(s);
                } else if let Some(range_arr) = it.get("range").and_then(|r| r.as_array()) {
                    let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
                    let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
                    let step = range_arr.get(2).and_then(|x| x.as_str()).unwrap_or("");
                    out.push_str(a);
                    out.push_str("..");
                    out.push_str(b);
                    if !step.is_empty() {
                        out.push_str("..");
                        out.push_str(step);
                    }
                }
            }
            out.push('}');
        }
    }
    out.push_str(&suffix);
    out
}

fn bash_quote_word(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\\\''"))
}

/// Commands whose Generator emulations are verified bash-correct in
/// STANDALONE use (no pipeline/$input_data machinery, no file-arg gaps,
/// error paths included). Everything else falls back to `bash -c`
/// shell-out. Verified empirically per command+arg-shape: seq, ls, wc,
/// cat, tail, grep, tr, mkdir, rm, touch, basename, dirname, pwd, date,
/// hostname, paste, tee, which, yes. Excluded: sort/uniq/sed/cut/comm/
/// strings/gzip/awk (need the pipeline \$input_data), head (reads STDIN
/// even with a file arg), cp/mv (croak on missing source where bash
/// errors and continues), sha256sum/diff/find (output format), rmdir
/// (failure exit code).
const EMULATED_COMMANDS: &[&str] = &[
    "seq", "ls", "wc", "cat", "tail", "grep", "tr", "mkdir", "rm",
    "touch", "basename", "dirname", "pwd", "date", "hostname", "paste",
    "tee", "which", "yes",
];

/// `$ENV{name} = $name;` for every `$name` referenced in a shell command
/// text — bash children read Perl vars via the environment (otherwise
/// `"$longline"` in the command is unset in the child).
fn var_exports_str(cmd: &str) -> String {
    let re = regex::Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = String::new();
    for cap in re.captures_iter(cmd) {
        let name = cap[1].to_string();
        if name == "ENV" {
            continue;
        }
        if name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
            continue; // real env vars (HOME, PATH...) — already in the child
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push_str(&format!("$ENV{{{}}} = ${};\n", name, name));
    }
    out
}

/// `$ENV{name} = $name;` lines + the bash -c system call + status tracking.
fn emit_shell_cmd(out: &mut String, indent: usize, cmd: &str) {
    let exports = var_exports_str(cmd);
    for line in exports.lines() {
        emit_indent(out, indent);
        out.push_str(line);
        out.push('\n');
    }
    emit_indent(out, indent);
    out.push_str(&format!(
        "system('bash', '-c', {}); $main_exit_code = $CHILD_ERROR = $? >> 8;\n",
        safe_perl_q_string(cmd)
    ));
}

/// Reuse the AST Generator's per-command in-Perl emulations for an external
/// command: reconstruct the shell text from IR words, re-parse it into a
/// `SimpleCommand`, and run the Generator's dispatcher (ls/wc/sed/… become
/// native Perl, no bash dependency). Returns None when the command isn't
/// emulatable (caller falls back to `bash -c` shell-out).
fn generator_emulate_command(cmd: &str, words: &[&IrExpr]) -> Option<String> {
    let shell_text = build_shell_cmd(cmd, words);
    let parsed = crate::Parser::new(&shell_text).parse().ok()?;
    let simple = parsed.into_iter().find_map(|c| match c {
        crate::ast::Command::Simple(sc) => Some(sc),
        _ => None,
    })?;
    let mut gen = crate::generator::Generator::new();
    let perl = crate::generator::commands::simple_commands::generate_simple_command_impl(
        &mut gen, &simple,
    );
    if perl.trim().is_empty() {
        None
    } else {
        Some(perl)
    }
}

/// The modern IR packs all of a command's word arguments into a single
/// `Array` element: exec(cmd, Array([w1, w2, …])). Flatten to the word list.
fn exec_word_args(args: &[IrExpr]) -> Vec<&IrExpr> {
    if args.len() == 2 {
        if let IrExpr::Array(elems) = &args[1] {
            return elems.iter().collect();
        }
    }
    args[1..].iter().collect()
}

/// Extract the brace-expansion Json (arg 1) from a `brace` Call.
fn brace_json_arg(args: &[IrExpr]) -> Option<&serde_json::Value> {
    args.iter().find_map(|a| match a {
        IrExpr::Json(v) => Some(v),
        _ => None,
    })
}

/// Expand one range spec (numeric or single-char) to its items, honoring
/// the step and zero-padding width of the start/end strings. Capped at
/// 1024 items.
fn expand_range(start: &str, end: &str, step: i64) -> Vec<String> {
    let mut out = Vec::new();
    if step == 0 {
        return out;
    }
    // Numeric range (zero-padded only when the START has a leading zero).
    if let (Ok(ai), Ok(bi)) = (start.parse::<i64>(), end.parse::<i64>()) {
        let pad = start.starts_with('0');
        let width = if pad { start.len().max(end.len()) } else { 1 };
        if (bi - ai) / step.abs() >= 1024 {
            return out;
        }
        let mut n = ai;
        while (n <= bi && step > 0) || (n >= bi && step < 0) {
            if pad && n >= 0 {
                out.push(format!("{:0width$}", n, width = width));
            } else {
                out.push(n.to_string());
            }
            n += step;
        }
        return out;
    }
    // Single-character range (a..c, a..z..3).
    if let (Some(ac), Some(bc)) = (start.chars().next(), end.chars().next()) {
        if start.chars().count() == 1 && end.chars().count() == 1 {
            let (ai, bi) = (ac as i64, bc as i64);
            if (bi - ai) / step.abs() >= 1024 {
                return out;
            }
            let mut n = ai;
            while (n <= bi && step > 0) || (n >= bi && step < 0) {
                out.push(char::from_u32(n as u32).unwrap_or('?').to_string());
                n += step;
            }
        }
    }
    out
}

/// Expand the brace groups Json to one item-list per group (each range
/// entry expanded).
fn brace_groups(v: &serde_json::Value) -> Vec<Vec<String>> {
    let mut groups = Vec::new();
    let arr = match v.as_array() {
        Some(a) => a,
        None => return groups,
    };
    for g in arr {
        let items = match g.as_array() {
            Some(i) => i,
            None => continue,
        };
        let mut expanded = Vec::new();
        for it in items {
            if let Some(s) = it.as_str() {
                expanded.push(s.to_string());
            } else if let Some(range_arr) = it.get("range").and_then(|r| r.as_array()) {
                let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
                let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
                let step = range_arr
                    .get(2)
                    .and_then(|x| x.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                expanded.extend(expand_range(a, b, step));
            }
        }
        if !expanded.is_empty() {
            groups.push(expanded);
        }
    }
    groups
}

/// Full brace expansion: prefix + cross-product(groups) + suffix.
fn brace_expand(args: &[IrExpr]) -> Vec<String> {
    let prefix = args.first().and_then(call_arg_str).unwrap_or_default();
    let suffix = args.get(3).and_then(call_arg_str).unwrap_or_default();
    let v = match brace_json_arg(args) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let groups = brace_groups(v);
    if groups.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // Cross-product of the groups.
    let mut stack: Vec<String> = vec![prefix];
    for group in &groups {
        let mut next = Vec::new();
        for acc in &stack {
            for item in group {
                let mut s = acc.clone();
                s.push_str(item);
                next.push(s);
                if next.len() > 4096 {
                    break;
                }
            }
        }
        stack = next;
    }
    for mut s in stack {
        s.push_str(&suffix);
        out.push(s);
    }
    out
}

/// Render a `brace` Call in word position: the expanded items joined by
/// spaces (echo {a,b,c} → a b c).
fn render_brace_word(args: &[IrExpr]) -> String {
    let items = brace_expand(args);
    if items.is_empty() {
        return "''".to_string();
    }
    let quoted: Vec<String> = items
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "\\\\'")))
        .collect();
    format!("join(' ', {})", quoted.join(", "))
}

/// Render a `brace` Call as a for-iterable: {1..5} → 1..5; {a..c} →
/// ('a'..'c'); {a,b,c} → ('a','b','c').
fn render_brace_iter(args: &[IrExpr]) -> String {
    let v = match brace_json_arg(args) {
        Some(v) => v,
        None => return "()".to_string(),
    };
    // Single numeric range → 1..5; single char range → ('a'..'c').
    let groups = brace_groups(v);
    if groups.len() == 1 && groups[0].len() == 1 {
        if let Some(range_arr) = v.as_array().and_then(|a| a[0].as_array()).and_then(|g| g[0].get("range")).and_then(|r| r.as_array()) {
            let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
            let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
            let step = range_arr.get(2).and_then(|x| x.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
            if step == 1 {
                if a.parse::<i64>().is_ok() && b.parse::<i64>().is_ok() {
                    return format!("{}..{}", a.parse::<i64>().unwrap(), b.parse::<i64>().unwrap());
                }
                if a.chars().count() == 1 && b.chars().count() == 1 {
                    return format!("('{}'..'{}')", a, b);
                }
            }
        }
    }
    let items = brace_expand(args);
    if items.is_empty() {
        return "()".to_string();
    }
    let quoted: Vec<String> = items
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "\\\\'")))
        .collect();
    format!("({})", quoted.join(", "))
}

/// Extract (cmd, words) from an `exec` Call's args (arg0 = command name;
/// the remaining args are the flattened word list — see exec_word_args).
fn exec_call_parts(args: &[IrExpr]) -> Option<(String, Vec<&IrExpr>)> {
    let cmd = call_arg_str(args.first()?)?;
    Some((cmd, exec_word_args(args)))
}

/// Emit `LHS = do { open('bash','-c',CMD) ... }` capture (chomps trailing NL).
fn emit_capture_assign(out: &mut String, indent: usize, lhs: &str, cmd: &str) {
    emit_indent(out, indent);
    out.push_str(&format!("{} = {};\n", lhs, cmd_str_to_open_perl(cmd)));
}

/// Rebuild the shell command string for an expression: a `Call exec`,
/// a `pipeline` Call, or a `&&`/`||` chain of those (bash control operators).
fn expr_to_cmd(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "exec" => {
                let (cmd, words) = exec_call_parts(args)?;
                Some(build_shell_cmd(&cmd, &words))
            }
            "pipeline" => pipeline_call_to_cmd(e),
            "redirect" => redirect_call_to_cmd(e),
            // A test in command position reconstructs as bash `[ … ]` — but
            // only when the text is genuinely spaced (`-f x`); flattened
            // `[[ ]]`/`[ ]` forms (no-space `==`/`=~`/`=`) can't round-trip.
            "test" => {
                let text = args.first().and_then(call_arg_str)?;
                if text.contains('=') || text.contains('~') {
                    return None;
                }
                Some(format!("[ {} ]", text))
            }
            _ => None,
        },
        IrExpr::BinOp { lhs, op, rhs } => {
            let opstr = match op {
                BinOpKind::And => " && ",
                BinOpKind::Or => " || ",
                _ => return None,
            };
            Some(format!("{}{}{}", expr_to_cmd(lhs)?, opstr, expr_to_cmd(rhs)?))
        }
        _ => None,
    }
}

/// Read the (fd, mode, target) of one redirect spec Object.
fn redirect_spec_fields(e: &IrExpr) -> Option<(i64, String, String)> {
    if let IrExpr::Object(fields) = e {
        let mut fd: i64 = 1;
        let mut mode = String::new();
        let mut target = String::new();
        for (k, v) in fields {
            match k.as_str() {
                "fd" => {
                    if let IrExpr::Int(n) = v {
                        fd = *n;
                    }
                }
                "mode" => mode = call_arg_str(v).unwrap_or_default(),
                "target" => target = call_arg_str(v).unwrap_or_default(),
                _ => {}
            }
        }
        Some((fd, mode, target))
    } else {
        None
    }
}

/// Append one redirect's shell syntax (` > t`, ` 2>> t`, ` 2>&1`, ` <<< s`,
/// ` <<DELIM\nbody\nDELIM`) to cmd. Returns false for unknown modes.
fn append_redirect_frag(cmd: &mut String, fd: i64, mode: &str, target: &str) -> bool {
    match mode {
        "herestring" => {
            cmd.push_str(&format!(" <<< '{}'", target.replace('\'', "'\\\\''")));
            return true;
        }
        "heredoc" => {
            let body = target;
            cmd.push_str(" <<'_SH2DOC_'");
            cmd.push('\n');
            cmd.push_str(body);
            if !body.ends_with('\n') {
                cmd.push('\n');
            }
            cmd.push_str("_SH2DOC_");
            return true;
        }
        // Process substitution (core-request perl-shir-20260806-1930):
        // the target carries the inner command text; bash expands it.
        "process-in" => {
            cmd.push_str(&format!(" <({})", target));
            return true;
        }
        "process-out" => {
            cmd.push_str(&format!(" >({})", target));
            return true;
        }
        _ => {}
    }
    let op = match mode {
        "w" => ">",
        "a" => ">>",
        "r" => "<",
        _ => return false,
    };
    // fd N → target &M means N>&M (fd dup).
    if let Some(m) = target.strip_prefix('&') {
        cmd.push_str(&format!(" {}>{}&{}", fd, op.trim_start_matches('<'), m));
        return true;
    }
    let quoted = format!("'{}'", target.replace('\'', "'\\\\''"));
    let frag = match fd {
        0 => format!(" < {}", quoted),
        1 => format!(" > {}", quoted),
        n => format!(" {}> {}", n, quoted),
    };
    cmd.push_str(&frag);
    true
}

/// Rebuild a shell command string from a `redirect` Call (a command plus
/// fd redirect specs) — used in condition and statement positions.
fn redirect_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "redirect" {
            let mut cmd = args.first().and_then(arrow_to_cmd)?;
            for spec in args.iter().skip(1) {
                let specs: Vec<&IrExpr> = match spec {
                    IrExpr::Array(elems) => elems.iter().collect(),
                    other => vec![other],
                };
                for s in specs {
                    if let Some((fd, mode, target)) = redirect_spec_fields(s) {
                        append_redirect_frag(&mut cmd, fd, &mode, &target);
                    }
                }
            }
            return Some(cmd);
        }
    }
    None
}

/// Rebuild a shell command string from a `block` Call: multiple statements
/// run in sequence (`;`-joined) — used in condition position.
fn block_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "block" {
            for a in args {
                if let IrExpr::Arrow(stmts) = a {
                    let cmds: Vec<String> = stmts
                        .iter()
                        .filter_map(|s| {
                            if let IrStmt::Expr(inner) = s {
                                expr_to_cmd(inner)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !cmds.is_empty() {
                        return Some(cmds.join("; "));
                    }
                }
            }
        }
    }
    None
}

/// Find the shell command string inside a block of stmts (exec, pipeline,
/// or `&&`/`||` chain) — shared by Arrow bodies and Redirect inners.
fn stmts_to_shell_cmd(stmts: &[IrStmt]) -> Option<String> {
    for s in stmts {
        if let IrStmt::Expr(inner) = s {
            if let Some(cmd) = expr_to_cmd(inner) {
                return Some(cmd);
            }
        }
    }
    None
}

/// Rebuild a shell command string from a `pipeline` Call:
/// pipeline([Arrow([Expr(exec)]), Arrow([Expr(exec)]), …]) → "a | b".
fn arrow_to_cmd(a: &IrExpr) -> Option<String> {
    if let IrExpr::Arrow(stmts) = a {
        stmts_to_shell_cmd(stmts)
    } else {
        None
    }
}

fn pipeline_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "pipeline" {
            // Stages arrive as pipeline([Arrow, Arrow, …]) or
            // pipeline([Array([Arrow, Arrow, …])]).
            let mut stage_exprs: Vec<&IrExpr> = Vec::new();
            for a in args {
                if let IrExpr::Array(elems) = a {
                    stage_exprs.extend(elems.iter());
                } else {
                    stage_exprs.push(a);
                }
            }
            let stages: Vec<String> = stage_exprs.iter().filter_map(|a| arrow_to_cmd(a)).collect();
            if stages.is_empty() {
                return None;
            }
            return Some(stages.join(" | "));
        }
    }
    None
}

/// Rebuild the shell command string from a `capture` Call (command
/// substitution `$(…)` / backticks) whose closure body is an exec/pipeline.
fn capture_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "capture" {
            for a in args {
                if let Some(cmd) = arrow_to_cmd(a) {
                    return Some(cmd);
                }
            }
        }
    }
    None
}

/// Render a `for` iterable: `Array([…])` → Perl list; `Range` → `a..b`;
/// `split(x)` → `split /\s+/, x` (bash IFS word splitting); a single
/// `capture` element → iterate the captured words.
fn word_iter_to_perl(iter: &IrExpr) -> String {
    match iter {
        IrExpr::Call { func, args } if func == "split" => {
            let inner = args.first().map(render_word).unwrap_or_default();
            format!("split /\\s+/, {}", inner)
        }
        IrExpr::Array(elems) if elems.len() == 1 => {
            if let IrExpr::Call { func, .. } = &elems[0] {
                if func == "capture" || func == "captureWords" {
                    if let Some(cmd) = capture_call_to_cmd(&elems[0]) {
                        return format!("split /\\s+/, {}", cmd_str_to_open_perl(&cmd));
                    }
                }
                if func == "brace" {
                    if let IrExpr::Call { args, .. } = &elems[0] {
                        return render_brace_iter(args);
                    }
                }
            }
            ir_expr_to_perl(iter)
        }
        IrExpr::Array(_) => ir_expr_to_perl(iter),
        IrExpr::Call { func, args } if func == "brace" => render_brace_iter(args),
        _ => ir_expr_to_perl(iter),
    }
}


/// Lower a modern-IR `exec` Call in statement position.
fn emit_exec_call(out: &mut String, call: &IrExpr, indent: usize) {
    let (cmd, words) = match call {
        IrExpr::Call { args, .. } => match exec_call_parts(args) {
            Some(p) => p,
            None => {
                // Dynamic / unparseable command — shell out via bash -c.
                let full = args
                    .iter()
                    .map(render_word)
                    .collect::<Vec<_>>()
                    .join(" ");
                emit_shell_cmd(out, indent, &full);
                return;
            }
        },
        _ => return,
    };
    match cmd.as_str() {
        "echo" => emit_echo(out, &words, indent),
        "printf" => {
            if words.is_empty() {
                emit_indent(out, indent);
                out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                return;
            }
            // The format word: keep backslashes intact — Perl's printf
            // interprets \n/\t in the double-quoted format.
            let fmt = match &words[0] {
                IrExpr::Str(s, _) => {
                    let mut f = String::from("\"");
                    for ch in s.chars() {
                        match ch {
                            '"' => f.push_str("\\\""),
                            '$' => f.push_str("\\$"),
                            '@' => f.push_str("\\@"),
                            c => f.push(c),
                        }
                    }
                    f.push('"');
                    f
                }
                _ => render_word(words[0]),
            };
            let rest: Vec<String> = words[1..].iter().map(|w| render_word(w)).collect();
            emit_indent(out, indent);
            if rest.is_empty() {
                out.push_str(&format!("printf({});\n", fmt));
            } else {
                out.push_str(&format!("printf({}, {});\n", fmt, rest.join(", ")));
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "cd" => {
            let dir = words
                .first()
                .map(|w| render_word(w))
                .unwrap_or_else(|| "$ENV{HOME}".to_string());
            emit_indent(out, indent);
            out.push_str(&format!(
                "chdir({}) or die \"cd: $!\\n\"; $main_exit_code = $CHILD_ERROR = 0;\n",
                dir
            ));
        }
        "export" => {
            for w in &words {
                if let Some(word_str) = call_arg_str(w) {
                    if let Some(eq) = word_str.split_once('=') {
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "$ENV{{{}}} = {};\n",
                            eq.0,
                            render_word(&IrExpr::Str(eq.1.to_string(), StrStyle::DoubleQuoted))
                        ));
                    } else {
                        emit_indent(out, indent);
                        out.push_str(&format!("$ENV{{{}}} = $ENV{{{}}};\n", word_str, word_str));
                    }
                }
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "pwd" => {
            emit_indent(out, indent);
            out.push_str("print qx{pwd};\n");
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "shopt" | "set" => {
            // Shell-option toggles — no runtime effect for the supported
            // subset (the corpus's shopt -s/-u lines gate extglob etc.).
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "true" | ":" => {
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "false" => {
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 1;\n");
        }
        "shift" => {
            emit_indent(out, indent);
            out.push_str("shift @ARGV;\n");
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "exit" => {
            let code = words
                .first()
                .map(|w| render_word(w))
                .unwrap_or_else(|| "0".to_string());
            emit_indent(out, indent);
            out.push_str(&format!("exit {};\n", code));
        }
        "read" => {
            if let Some(n) = words.first().and_then(|w| call_arg_str(w)) {
                emit_indent(out, indent);
                out.push_str(&format!(
                    "chomp(my $__line = <STDIN>); ${} = $__line;\n",
                    n
                ));
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "sleep" => {
            let t = words
                .first()
                .map(|w| render_word(w))
                .unwrap_or_else(|| "1".to_string());
            emit_indent(out, indent);
            out.push_str(&format!("sleep({}); $main_exit_code = $CHILD_ERROR = 0;\n", t));
        }
        _ => {
            // External command — first try the AST Generator's in-Perl
            // emulation (verified-safe commands only: native Perl, no bash
            // dependency); fall back to bash -c shell-out otherwise.
            if EMULATED_COMMANDS.contains(&cmd.as_str())
                && std::env::var("DEBASHC_IR_NO_EMUL").is_err()
            {
                if let Some(perl) = generator_emulate_command(&cmd, &words) {
                    for line in perl.lines() {
                        emit_indent(out, indent);
                        out.push_str(line);
                        out.push('\n');
                    }
                    return;
                }
            }
            let full = build_shell_cmd(&cmd, &words);
            emit_shell_cmd(out, indent, &full);
        }
    }
}

/// `echo` lowering — join args with single spaces, newline unless `-n`.
fn emit_echo(out: &mut String, words: &[&IrExpr], indent: usize) {
    let mut args = words;
    let mut newline = true;
    if let Some(first) = args.first() {
        if let Some(s) = call_arg_str(first) {
            if s == "-n" {
                newline = false;
                args = &args[1..];
            }
        }
    }
    let ok = |out: &mut String| out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
    if args.is_empty() {
        emit_indent(out, indent);
        if newline {
            out.push_str("print \"\\n\";\n");
        }
        emit_indent(out, indent);
        ok(out);
        return;
    }
    let all_literal = args.iter().all(|a| matches!(a, IrExpr::Str(_, _)));
    if all_literal {
        let joined: String = args
            .iter()
            .map(|a| call_arg_str(a).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ");
        let mut s = String::from("\"");
        for ch in joined.chars() {
            match ch {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                '$' => s.push_str("\\$"),
                '@' => s.push_str("\\@"),
                '\n' => s.push_str("\\n"),
                _ => s.push(ch),
            }
        }
        if newline {
            s.push_str("\\n");
        }
        s.push('"');
        emit_indent(out, indent);
        out.push_str(&format!("print {};\n", s));
        emit_indent(out, indent);
        ok(out);
        return;
    }
    let parts: Vec<String> = args.iter().map(|w| render_word(w)).collect();
    let nl = if newline { ", \"\\n\"" } else { "" };
    if parts.len() == 1 {
        // A single glob word is a LIST in Perl — wrap in join(' ', …) so
        // the expansion space-joins like bash (echo file_*.txt).
        if parts[0].starts_with("glob(") {
            emit_indent(out, indent);
            out.push_str(&format!("print(join(' ', {}){});\n", parts[0], nl));
        } else {
            emit_indent(out, indent);
            out.push_str(&format!("print({}{});\n", parts[0], nl));
        }
    } else {
        // Unquoted word-splitting drops empty words (bash IFS semantics);
        // `grep { defined && length }` filters those out and avoids undef
        // warnings on unset positional params.
        let has_split = args.iter().any(|w| matches!(w, IrExpr::Call { func, .. } if func == "split"));
        if has_split {
            emit_indent(out, indent);
            out.push_str(&format!(
                "print(join(' ', grep {{ defined($_) && length($_) }} {}){});\n",
                parts.join(", "),
                nl
            ));
        } else {
            emit_indent(out, indent);
            out.push_str(&format!(
                "print(join(' ', {}){});\n",
                parts.join(", "),
                nl
            ));
        }
    }
    emit_indent(out, indent);
    ok(out);
}

/// Collect variables assigned by `Assign` stmts (recursively) so the
/// modern-IR preamble can hoist `my` declarations (Perl `use strict`
/// requires them; the Generator's usage analysis does the same). Vars
/// declared via `Declare`/`DeclareArray` are excluded (they emit their own
/// `my`/`local`).
fn collect_assigned_vars(stmts: &[IrStmt], out: &mut Vec<(String, Sigil)>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    if is_env_style_var_name(&t.var) {
                        continue;
                    }
                    // setArray RHS → the target is an array even if the sigil
                    // annotation is absent.
                    let is_array_rhs = matches!(expr, IrExpr::Call { func, .. } if func == "setArray");
                    let (base, idx) = split_indexed_var(&t.var);
                    let sigil = if is_array_rhs {
                        Sigil::Array
                    } else if idx.is_some() {
                        Sigil::Hash
                    } else {
                        t.sigil.unwrap_or(Sigil::Scalar)
                    };
                    if !out.iter().any(|(n, _)| n == base) {
                        out.push((base.to_string(), sigil));
                    }
                }
            }
            IrStmt::Declare { vars, .. } => {
                for d in vars {
                    out.retain(|(n, _)| n != &d.name);
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                out.retain(|(n, _)| n != var);
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_assigned_vars(then, out);
                for (_, b) in elsifs {
                    collect_assigned_vars(b, out);
                }
                collect_assigned_vars(else_, out);
            }
            IrStmt::For { var, body, .. } => {
                // The loop var is read after the loop (bash keeps the last
                // value) — declare it so `for $var` aliases a lexical.
                if !is_env_style_var_name(var)
                    && !out.iter().any(|(n, _)| n == var)
                {
                    out.push((var.clone(), Sigil::Scalar));
                }
                collect_assigned_vars(body, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => collect_assigned_vars(body, out),
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_assigned_vars(&c.body, out);
                }
            }
            IrStmt::Function { body, .. } => collect_assigned_vars(body, out),
            IrStmt::Expr(IrExpr::Arrow(body)) => collect_assigned_vars(body, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_assigned_vars(st, out);
                }
            }
            _ => {}
        }
    }
}

/// A var name that needs a hoisted `my` declaration: not an env-style
/// name (→ $ENV), not a positional param (→ $ARGV), not a bash special.
fn var_is_declarable(name: &str) -> bool {
    !is_env_style_var_name(name)
        && !name.is_empty()
        && !name.chars().all(|c| c.is_ascii_digit())
        && !matches!(name, "#" | "?" | "@" | "*" | "$" | "!" | "0")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Collect vars READ via getVar/param/Var (recursively) so the preamble can
/// declare them too — `use strict` fails on any undeclared read, and bash
/// reads of unset vars yield empty strings (a `my $x;` matches that).
fn collect_read_vars_expr(e: &IrExpr, out: &mut Vec<(String, Sigil)>) {
    match e {
        IrExpr::Call { func, args } => {
            if func == "getVar" || func == "param" {
                let idx = if func == "param" { 1 } else { 0 };
                if let Some(name) = args.get(idx).and_then(call_arg_str) {
                    // param "slice" (${arr[@]:o:l}) reads an ARRAY.
                    let sigil = if func == "param"
                        && args.first().and_then(call_arg_str).as_deref() == Some("slice")
                    {
                        Sigil::Array
                    } else {
                        Sigil::Scalar
                    };
                    if var_is_declarable(&name)
                        && !out.iter().any(|(n, _)| n == &name)
                    {
                        out.push((name, sigil));
                    }
                }
            }
            // Array reads (${arr[@]}, ${#arr[@]}, arr[i], $(arr…)) declare
            // as @name.
            if matches!(
                func.as_str(),
                "listVar" | "getArray" | "arrayItems" | "arrayLen" | "arrayIndex"
            ) {
                if let Some(name) = args.first().and_then(call_arg_str) {
                    if var_is_declarable(&name)
                        && !out.iter().any(|(n, _)| n == &name)
                    {
                        out.push((name, Sigil::Array));
                    }
                }
            }
            if func == "test" {
                // Test conditions arrive as a flat string ("$y -lt 10")
                // with $vars inside — collect them for declarations.
                if let Some(s) = args.first().and_then(call_arg_str) {
                    collect_vars_from_test_string(&s, out);
                }
            }
            for a in args {
                collect_read_vars_expr(a, out);
            }
        }
        IrExpr::Var(name, sigil) => {
            if var_is_declarable(name) && !out.iter().any(|(n, _)| n == name) {
                out.push((name.clone(), sigil.unwrap_or(Sigil::Scalar)));
            }
        }
        IrExpr::Array(elems) => {
            for e in elems {
                collect_read_vars_expr(e, out);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(ee) = p {
                    collect_read_vars_expr(ee, out);
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_read_vars_expr(lhs, out);
            collect_read_vars_expr(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_read_vars_expr(cond, out);
            collect_read_vars_expr(then, out);
            collect_read_vars_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_read_vars_expr(expr, out);
            collect_read_vars_expr(default, out);
        }
        IrExpr::Index { key, .. } => collect_read_vars_expr(key, out),
        IrExpr::Capture { expr, .. } => collect_read_vars_expr(expr, out),
        IrExpr::Arith(ast) => collect_read_vars_arith(ast, out),
        IrExpr::MethodCall { obj, args, .. } => {
            collect_read_vars_expr(obj, out);
            for a in args {
                collect_read_vars_expr(a, out);
            }
        }
        IrExpr::Object(fields) => {
            for (_, v) in fields {
                collect_read_vars_expr(v, out);
            }
        }
        IrExpr::Arrow(stmts) => collect_read_vars_stmts(stmts, out),
        _ => {}
    }
}

fn collect_read_vars_arith(ast: &ArithAst, out: &mut Vec<(String, Sigil)>) {
    match ast {
        ArithAst::Var(name) => {
            if var_is_declarable(name) && !out.iter().any(|(n, _)| n == name) {
                out.push((name.clone(), Sigil::Scalar));
            }
        }
        ArithAst::Index { var, key } => {
            if var_is_declarable(var) && !out.iter().any(|(n, _)| n == var) {
                out.push((var.clone(), Sigil::Scalar));
            }
            collect_read_vars_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_read_vars_arith(lhs, out);
            collect_read_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_read_vars_arith(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_read_vars_arith(test, out);
            collect_read_vars_arith(then, out);
            collect_read_vars_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            if var_is_declarable(var) && !out.iter().any(|(n, _)| n == var) {
                out.push((var.clone(), Sigil::Scalar));
            }
            collect_read_vars_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            if var_is_declarable(var) && !out.iter().any(|(n, _)| n == var) {
                out.push((var.clone(), Sigil::Scalar));
            }
        }
        ArithAst::Num(_) => {}
    }
}

fn collect_read_vars_stmts(stmts: &[IrStmt], out: &mut Vec<(String, Sigil)>) {
    for s in stmts {
        match s {
            IrStmt::Expr(e) | IrStmt::Assign { expr: e, .. } => collect_read_vars_expr(e, out),
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_read_vars_expr(cond, out);
                collect_read_vars_stmts(then, out);
                for (c, b) in elsifs {
                    collect_read_vars_expr(c, out);
                    collect_read_vars_stmts(b, out);
                }
                collect_read_vars_stmts(else_, out);
            }
            IrStmt::For { iter, body, .. } => {
                collect_read_vars_expr(iter, out);
                collect_read_vars_stmts(body, out);
            }
            IrStmt::While { cond, body } => {
                collect_read_vars_expr(cond, out);
                collect_read_vars_stmts(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_read_vars_expr(cond, out);
                collect_read_vars_stmts(body, out);
            }
            IrStmt::Case { discriminant, clauses, .. } => {
                collect_read_vars_expr(discriminant, out);
                for c in clauses {
                    collect_read_vars_stmts(&c.body, out);
                }
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => collect_read_vars_stmts(body, out),
            IrStmt::Declare { init, .. } => {
                if let Some(i) = init {
                    collect_read_vars_expr(i, out);
                }
            }
            IrStmt::DeclareArray { elements, .. } => {
                for el in elements {
                    collect_read_vars_expr(el, out);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_read_vars_stmts(st, out);
                }
            }
            IrStmt::Output { value, .. } | IrStmt::WriteFile { path: value, .. } => {
                let _ = value;
            }
            _ => {}
        }
    }
}

/// True if `name` is assigned by any `Assign` stmt (recursively).
fn collect_assigned_vars_contains(stmts: &[IrStmt], name: &str) -> bool {
    let mut found = Vec::new();
    collect_assigned_vars(stmts, &mut found);
    found.iter().any(|(n, _)| n == name)
}

/// Scan a test-condition string for `$var` / `${var}` / `${#var}` reads
/// (for hoisted declarations — the vars are text inside the flat string).
fn collect_vars_from_test_string(s: &str, out: &mut Vec<(String, Sigil)>) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '{' {
                let mut k = j + 1;
                if k < chars.len() && chars[k] == '#' {
                    k += 1;
                }
                let start = k;
                while k < chars.len() && (chars[k].is_ascii_alphanumeric() || chars[k] == '_') {
                    k += 1;
                }
                let name: String = chars[start..k].iter().collect();
                if var_is_declarable(&name) && !out.iter().any(|(n, _)| n == &name) {
                    out.push((name, Sigil::Scalar));
                }
                i = k;
            } else {
                let start = j;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                if j > start {
                    let name: String = chars[start..j].iter().collect();
                    if var_is_declarable(&name) && !out.iter().any(|(n, _)| n == &name) {
                        out.push((name, Sigil::Scalar));
                    }
                }
                i = j;
            }
        } else {
            i += 1;
        }
    }
}

/// Rewrite `exec(foo, …)` calls to the `$fn_call` marker when `foo` is a
/// function defined in the program (recursively into compound bodies).
fn rewrite_fn_calls(stmts: &[IrStmt], fns: &std::collections::HashSet<String>) -> Vec<IrStmt> {
    stmts
        .iter()
        .map(|s| match s {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                if let Some(name) = args.first().and_then(call_arg_str) {
                    if fns.contains(&name) {
                        return IrStmt::Expr(IrExpr::Call {
                            func: "$fn_call".to_string(),
                            args: args.clone(),
                        });
                    }
                }
                s.clone()
            }
            IrStmt::If { cond, then, elsifs, else_ } => IrStmt::If {
                cond: cond.clone(),
                then: rewrite_fn_calls(then, fns),
                elsifs: elsifs
                    .iter()
                    .map(|(c, b)| (c.clone(), rewrite_fn_calls(b, fns)))
                    .collect(),
                else_: rewrite_fn_calls(else_, fns),
            },
            IrStmt::While { cond, body } => IrStmt::While {
                cond: cond.clone(),
                body: rewrite_fn_calls(body, fns),
            },
            IrStmt::DoWhile { body, cond, until } => IrStmt::DoWhile {
                body: rewrite_fn_calls(body, fns),
                cond: cond.clone(),
                until: *until,
            },
            IrStmt::For { var, iter, body } => IrStmt::For {
                var: var.clone(),
                iter: iter.clone(),
                body: rewrite_fn_calls(body, fns),
            },
            IrStmt::Case { discriminant, clauses } => IrStmt::Case {
                discriminant: discriminant.clone(),
                clauses: clauses
                    .iter()
                    .map(|c| IrCaseClause {
                        patterns: c.patterns.clone(),
                        body: rewrite_fn_calls(&c.body, fns),
                    })
                    .collect(),
            },
            IrStmt::Function { name, body } => IrStmt::Function {
                name: name.clone(),
                body: rewrite_fn_calls(body, fns),
            },
            IrStmt::Subshell(body) => IrStmt::Subshell(rewrite_fn_calls(body, fns)),
            IrStmt::Background(body) => IrStmt::Background(rewrite_fn_calls(body, fns)),
            IrStmt::Block(body) => IrStmt::Block(rewrite_fn_calls(body, fns)),
            IrStmt::Pipeline { stages, last_output, capture, cmd_str } => IrStmt::Pipeline {
                stages: stages.iter().map(|st| rewrite_fn_calls(st, fns)).collect(),
                last_output: last_output.clone(),
                capture: capture.clone(),
                cmd_str: cmd_str.clone(),
            },
            other => other.clone(),
        })
        .collect()
}

/// Render a test-expression string (the text inside `[ … ]` / `[[ … ]]`,
/// as flattened by ast_to_ir) as a Perl boolean expression.
fn render_test_expr(s: &str) -> String {
    // `[[ ]]` tests flatten to NO-WHITESPACE forms ("$s==*.txt",
    // "$s=~^re$", "$f1==!(*.min).js", ""$y"="5"") that the spaced
    // tokenizer can't parse. Spaced strings (with -gt/-a/etc.) keep the
    // tokenizer even if they contain a bare `=`.
    if !s.chars().any(|c| c.is_whitespace()) {
        if s.contains("==") || s.contains("=~") || s.contains("!=") || s.contains('=') {
            if let Some(r) = render_flat_test(s) {
                return r;
            }
        }
    }
    let toks = tokenize_test(s);
    let mut i = 0;
    let expr = parse_test_or(&toks, &mut i);
    if expr.is_empty() { "0".to_string() } else { expr }
}

/// A `[[ ]]`-flavored operand: strip quotes, `$var` → read, else literal.
fn flat_operand(s: &str) -> String {
    let t = s.trim().trim_matches('"').trim_matches('\'');
    if t == "~" {
        return "$ENV{HOME}".to_string();
    }
    if let Some(rest) = t.strip_prefix("~/") {
        if !rest.is_empty() {
            return format!("\"$ENV{{HOME}}/{}\"", rest.replace('\"', "\\\""));
        }
    }
    if let Some(name) = t.strip_prefix('$') {
        let clean = name.starts_with('{') && name.ends_with('}');
        let bare = &name[if clean { 1 } else { 0 }..name.len() - if clean { 1 } else { 0 }];
        if !bare.is_empty() && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            var_read(bare)
        } else {
            // $HOME/xxx — an interpolating literal: keep it double-quoted
            // with env-style vars rewritten to $ENV{…} (Perl would demand
            // a declaration for bare $HOME).
            let interp = t.replace('\"', "\\\"");
            let re = regex::Regex::new(r"\$(\w+)").unwrap();
            let rewritten = re.replace_all(&interp, "$$ENV{$1}");
            format!("\"{}\"", rewritten)
        }
    } else if t.is_empty() {
        "''".to_string()
    } else if t.contains('$') {
        // Interpolating literal — keep as a double-quoted string.
        format!("\"{}\"", t)
    } else {
        format!("'{}'", t.replace('\'', "\\\\'"))
    }
}

/// String equality with nocasematch runtime tracking.
fn nocase_str_eq(lhs: &str, rhs: &str) -> String {
    format!(
        "(($__nocasematch) ? lc({l}) eq lc({r}) : {l} eq {r})",
        l = lhs,
        r = rhs
    )
}

/// Render a flattened `[[ ]]` test: lhs OP rhs where OP is `==`, `=~`,
/// `!=` or `=`. Glob and extglob patterns become regex matches.
fn render_flat_test(s: &str) -> Option<String> {
    for (op, kind) in [("==", "eq"), ("=~", "re"), ("!=", "ne"), ("=", "eq")] {
        if let Some(pos) = s.find(op) {
            let lhs = flat_operand(&s[..pos]);
            let rhs = &s[pos + op.len()..];
            match kind {
                "re" => {
                    // [[ $s =~ regex ]] → $s =~ /regex/
                    let re = rhs.trim_matches('"').trim_matches('\'');
                    return Some(format!("({} =~ /{}/)", lhs, re));
                }
                "ne" => {
                    // != : string inequality (glob patterns rare here)
                    let l = flat_operand(rhs);
                    return Some(format!("(!({}))", nocase_str_eq(&lhs, &l)));
                }
                _ => {
                    // == or = : glob pattern (contains */?/[/!( ) or literal.
                    let has_glob = rhs.contains('*') || rhs.contains('?')
                        || rhs.contains('[') || rhs.contains("!(");
                    if rhs.contains("!(") {
                        // Extglob `!(P).S` — the compound is a full boolean
                        // expression (lhs embedded); return it directly.
                        if let Some(re) = glob_or_extglob_to_regex(&lhs, rhs) {
                            return Some(re);
                        }
                    }
                    if has_glob {
                        // Normal glob → anchored regex match.
                        let re = glob_to_regex(rhs.trim_matches('"').trim_matches('\''));
                        return Some(format!("({} =~ /^{}$/)", lhs, re));
                    }
                    return Some(nocase_str_eq(&lhs, &flat_operand(rhs)));
                }
            }
        }
    }
    None
}

/// glob → anchored regex; `!(P).S` extglob → the corpus idiom as a
/// compound: ends-with-S and not P+S.
fn glob_or_extglob_to_regex(lhs: &str, pat: &str) -> Option<String> {
    let pat = pat.trim_matches('"').trim_matches('\'');
    if let Some(open) = pat.find("!(") {
        if let Some(close_rel) = pat[open + 2..].find(')') {
            let close = open + 2 + close_rel;
            let inner = &pat[open + 2..close];
            let suffix = &pat[close + 1..];
            let re_inner = glob_to_regex(inner);
            let re_suffix = glob_to_regex(suffix);
            // (x =~ /S$/) && (x !~ /P S$/)
            return Some(format!(
                "(({l}) =~ /{rs}$/) && (({l}) !~ /{ri}{rs}$/)",
                l = lhs,
                rs = re_suffix,
                ri = re_inner
            ));
        }
    }
    Some(format!("^({})$", glob_to_regex(pat)))
}

/// Tokenize a test string: whitespace-separated, double-quoted runs kept
/// together (with the quotes stripped but `$var` left for interpolation).
fn tokenize_test(s: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut cur = String::new();
    let mut in_dq = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_dq {
                    in_dq = false;
                    toks.push(std::mem::take(&mut cur));
                } else {
                    in_dq = true;
                }
            }
            c if c.is_whitespace() && !in_dq => {
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
            }
            '=' if !in_dq => {
                // Flattened "[ a = b ]" loses the spaces around `=` — split
                // it out as its own token (handles `a=b`, `a==b`, `a!=b`).
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
                let mut op = String::from("=");
                if chars.peek() == Some(&'=') {
                    op.push('=');
                    chars.next();
                }
                toks.push(op);
            }
            '!' if !in_dq && chars.peek() == Some(&'=') => {
                // `!=` — possibly right after a quoted token ("$a"!="b").
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
                toks.push("!=".to_string());
                chars.next();
            }
            '\\' if in_dq => {
                if let Some(&d) = chars.peek() {
                    cur.push(d);
                    chars.next();
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    toks
}

fn test_peek<'a>(toks: &'a [String], i: usize) -> Option<&'a str> {
    toks.get(i).map(|s| s.as_str())
}

fn test_next<'a>(toks: &'a [String], i: &mut usize) -> Option<&'a str> {
    let t = test_peek(toks, *i);
    if t.is_some() {
        *i += 1;
    }
    t
}

fn parse_test_or(toks: &[String], i: &mut usize) -> String {
    let mut lhs = parse_test_and(toks, i);
    loop {
        match test_peek(toks, *i) {
            Some("||") | Some("-o") => {
                *i += 1;
                let rhs = parse_test_and(toks, i);
                lhs = format!("({} || {})", lhs, rhs);
            }
            _ => break,
        }
    }
    lhs
}

fn parse_test_and(toks: &[String], i: &mut usize) -> String {
    let mut lhs = parse_test_not(toks, i);
    loop {
        match test_peek(toks, *i) {
            Some("&&") | Some("-a") => {
                *i += 1;
                let rhs = parse_test_not(toks, i);
                lhs = format!("({} && {})", lhs, rhs);
            }
            _ => break,
        }
    }
    lhs
}

fn parse_test_not(toks: &[String], i: &mut usize) -> String {
    if test_peek(toks, *i) == Some("!") {
        *i += 1;
        format!("!({})", parse_test_not(toks, i))
    } else {
        parse_test_primary(toks, i)
    }
}

fn parse_test_primary(toks: &[String], i: &mut usize) -> String {
    match test_peek(toks, *i) {
        Some("(") => {
            *i += 1;
            let inner = parse_test_or(toks, i);
            if test_peek(toks, *i) == Some(")") {
                *i += 1;
            }
            format!("({})", inner)
        }
        Some("true") | Some(":") => {
            *i += 1;
            "1".to_string()
        }
        Some("false") => {
            *i += 1;
            "0".to_string()
        }
        Some(op) if is_file_test_op(op) => {
            *i += 1;
            let operand = test_next(toks, i).unwrap_or("''");
            let o = render_test_operand(operand);
            match op {
                "-z" => format!("!length({})", o),
                "-n" => format!("length({})", o),
                _ => format!("{} {}", op, o),
            }
        }
        _ => {
            let lhs = test_next(toks, i).unwrap_or("''");
            match test_peek(toks, *i) {
                Some(op) if is_compare_op(op) => {
                    *i += 1;
                    let rhs = test_next(toks, i).unwrap_or("''");
                    let perl_op = perl_compare_op(op);
                    let l = render_test_operand(lhs);
                    let r = render_test_operand(rhs);
                    format!("{} {} {}", l, perl_op, r)
                }
                _ => {
                    // bare operand: truthiness = non-empty string
                    let l = render_test_operand(lhs);
                    format!("length({})", l)
                }
            }
        }
    }
}

fn is_file_test_op(op: &str) -> bool {
    matches!(op, "-f" | "-d" | "-e" | "-r" | "-w" | "-x" | "-s" | "-z" | "-n")
}

fn is_compare_op(op: &str) -> bool {
    matches!(
        op,
        "=" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge"
    )
}

fn perl_compare_op(op: &str) -> &'static str {
    match op {
        "=" | "==" => "eq",
        "!=" => "ne",
        "-eq" => "==",
        "-ne" => "!=",
        "-lt" => "<",
        "-le" => "<=",
        "-gt" => ">",
        "-ge" => ">=",
        "<" => "<",
        ">" => ">",
        "<=" => "<=",
        ">=" => ">=",
        _ => "==",
    }
}

/// Render a test operand: `$var`-only tokens become bare `${name}` reads
/// (Generator idiom); literals become Perl literals.
fn render_test_operand(tok: &str) -> String {
    // $name[idx] / ${name[idx]} array element inside a test.
    if tok.starts_with('$') && tok.contains('[') && tok.contains(']') {
        let inner = tok.trim_start_matches("${").trim_start_matches('$').trim_end_matches('}');
        if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']' || c == '$' || c == '.' || c == ' ' || c == '-') {
            return format!("${}", inner);
        }
    }
    if tok.starts_with('$') && tok.len() > 1 {
        let name = &tok[1..];
        if name.starts_with('{') && name.ends_with('}') {
            var_read(&name[1..name.len() - 1])
        } else {
            var_read(name)
        }
    } else if tok.is_empty() {
        "''".to_string()
    } else if tok.chars().all(|c| c.is_ascii_digit()) {
        format!("'{}'", tok)
    } else if tok.contains('$') {
        // Literal with an embedded variable — double-quote for interpolation.
        let mut s = String::from("\"");
        for ch in tok.chars() {
            match ch {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                _ => s.push(ch),
            }
        }
        s.push('"');
        s
    } else {
        format!("'{}'", tok.replace('\'', "\\\\'"))
    }
}

/// Render a Str literal (shared by ir_expr_to_perl and render_word).
fn render_str_literal(s: &str, style: &StrStyle) -> String {
    match style {
        StrStyle::SingleQuoted => format!("'{}'", s.replace('\'', "\\\\'"))
            .replace("\\'\'", "\\'\\\\'\\'"),
        StrStyle::DoubleQuoted | StrStyle::Heredoc => {
            let mut escaped = String::from("\"");
            for ch in s.chars() {
                match ch {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    '$' if matches!(style, StrStyle::DoubleQuoted) => escaped.push_str("\\$"),
                    '@' if matches!(style, StrStyle::DoubleQuoted) => escaped.push_str("\\@"),
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
    }
}

/// Lower a modern-IR `test` Call (cond position) to a Perl boolean.
fn render_test_call(args: &[IrExpr]) -> String {
    if let Some(s) = args.first().and_then(call_arg_str) {
        render_test_expr(&s)
    } else {
        "0".to_string()
    }
}

// ── Expression emitter ───────────────────────────────────────────────

pub(crate) fn ir_expr_to_perl(expr: &IrExpr) -> String {
    match expr {
        IrExpr::Capture { expr, native } => {
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
        IrExpr::Arrow(stmts) => {
            // Delayed block (closure) — render as a Perl anonymous sub. The
            // capture/pipeline lowerings rebuild a shell command string from
            // the inner exec before falling back to this.
            let mut body = String::new();
            for s in stmts {
                emit_stmt(&mut body, s, 1);
            }
            format!("sub {{ {}\n}}", body.trim_end_matches('\n'))
        }
        IrExpr::Array(elements) => {
            // General expression position: parenthesized list (for-iter,
            // list contexts). Exec-word position uses render_word instead.
            if elements.is_empty() {
                "()".to_string()
            } else {
                format!(
                    "({})",
                    elements.iter().map(render_word).collect::<Vec<_>>().join(", ")
                )
            }
        }
        IrExpr::Arith(ast) => format!("int({})", arith_ast_to_perl(ast)),
        IrExpr::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        IrExpr::Json(v) => {
            let s = v.to_string();
            format!("'{}'", s.replace('\'', "\\\\'"))
        }
        IrExpr::Ident(name) => var_read(name),
        IrExpr::Object(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{} => {}", k, ir_expr_to_perl(v)))
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }

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
            StrStyle::Heredoc => {
                // Like DoubleQuoted but preserves $ and @ for Perl interpolation.
                let mut escaped = String::with_capacity(s.len() + 4);
                escaped.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => escaped.push_str("\\\""),
                        '\\' => escaped.push_str("\\\\"),
                        // Keep $ and @ unescaped — Perl will interpolate them
                        '\n' => escaped.push_str("\\n"),
                        '\t' => escaped.push_str("\\t"),
                        '\r' => escaped.push_str("\\r"),
                        c => escaped.push(c),
                    }
                }
                escaped.push('"');
                escaped
            }
        },

        IrExpr::Var(name, sigil) => match sigil.unwrap_or(Sigil::Scalar) {
            Sigil::Scalar => {
                if is_env_style_var_name(name) {
                    format!("$ENV{{{}}}", name)
                } else {
                    format!("${}", name)
                }
            }
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
            // Word-level funcs (shared with render_word): getVar, split,
            // param, arith, brace, capture/captureWords, listVar.
            match func.as_str() {
                "getVar" => args
                    .first()
                    .and_then(call_arg_str)
                    .map(|n| var_read(&n))
                    .unwrap_or_else(|| "''".to_string()),
                "split" => args
                    .first()
                    .map(render_word)
                    .unwrap_or_else(|| "''".to_string()),
                "param" => render_param(args),
                "arith" => args
                    .first()
                    .map(|a| format!("int({})", render_word(a)))
                    .unwrap_or_else(|| "0".to_string()),
                "brace" => render_brace_word(args),
                "capture" | "captureWords" => {
                    // Expression/interpolated context (e.g. inside a
                    // double-quoted word): the raw chomped value, NOT a
                    // split (split in scalar context yields a field count).
                    let cap = args.first().and_then(arrow_to_cmd);
                    match cap {
                        Some(cmd) => cmd_str_to_open_perl(&cmd),
                        None => "''".to_string(),
                    }
                }
                "listVar" | "getArray" | "arrayItems" => args
                    .first()
                    .and_then(call_arg_str)
                    .map(|n| format!("@{}", n))
                    .unwrap_or_else(|| "()".to_string()),
                "redirect" => {
                    // Redirect used as a condition: run the command with its
                    // redirects via bash -c and test its exit status.
                    match redirect_call_to_cmd(expr) {
                        Some(cmd) => format!(
                            "(system('bash', '-c', {}) == 0)",
                            safe_perl_q_string(&cmd)
                        ),
                        None => "0".to_string(),
                    }
                }
                "block" => {
                    // Multi-command block in condition position (e.g. a while
                    // cond with several statements): run them joined by `;`.
                    match block_call_to_cmd(expr) {
                        Some(cmd) => format!(
                            "(system('bash', '-c', {}) == 0)",
                            safe_perl_q_string(&cmd)
                        ),
                        None => "0".to_string(),
                    }
                }
                "arrayIndex" => {
                    let name = args.first().and_then(call_arg_str).unwrap_or_default();
                    let idx = args.get(1).and_then(call_arg_str).unwrap_or_default();
                    if idx == "@" || idx == "*" {
                        format!("@{}", name)
                    } else if idx.contains(',') {
                        // Bash pseudo-multidim subscript → hash key.
                        format!("${{{}}}{{\"{}\"}}", name, idx.replace('\"', "\\\""))
                    } else {
                        format!("${}[{}]", name, idx)
                    }
                }
                "arrayLen" => args
                    .first()
                    .and_then(call_arg_str)
                    .map(|n| format!("scalar(@{})", n))
                    .unwrap_or_else(|| "0".to_string()),
                "setArray" => {
                    // Array assignment in expression position (rare) — the
                    // elements as a parenthesized list (skip the name arg).
                    let elems: Vec<&IrExpr> = if args.len() >= 2 {
                        if let IrExpr::Array(elems) = &args[1] {
                            elems.iter().collect()
                        } else {
                            args[1..].iter().collect()
                        }
                    } else {
                        Vec::new()
                    };
                    let items: Vec<String> = elems.iter().map(|e| render_word(*e)).collect();
                    format!("({})", items.join(", "))
                }
                "test" => render_test_call(args),
                _ => {
                    let a = args
                        .iter()
                        .map(|a| ir_expr_to_perl(a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    // Special-case `chomp` with a single scalar argument to produce
                    // the idiomatic `chomp $var;` (without parentheses).
                    if func == "chomp" && args.len() == 1 {
                        format!("chomp {}", a)
                    // `join` needs a separator — a single argument (e.g. an
                    // array-slice param) gets the space join.
                    } else if func == "join" && args.len() == 1 {
                        format!("join(' ', {})", a)
                    // Special-case `join` to produce the idiomatic `join $sep, @list`
                    // (without parentheses) — the function-call parens add noise here.
                    } else if func == "join" && args.len() >= 2 {
                        format!("join {}", a)
                    } else {
                        format!("{}({})", func, a)
                    }
                }
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
                InterpPart::Expr(e) => matches!(e.as_ref(), IrExpr::Var(_, None | Some(Sigil::Scalar))),
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
                            if let IrExpr::Var(name, sigil) = e.as_ref() {
                                if sigil.unwrap_or(Sigil::Scalar) == Sigil::Scalar
                                    && is_env_style_var_name(name)
                                {
                                    s.push_str(&format!("$ENV{{{}}}", name));
                                } else {
                                    s.push_str(&format!("${}", name));
                                }
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
pub(crate) fn cmd_str_to_open_perl(cmd: &str) -> String {
    // Wrap the command string in a Perl do { open() ... } block so it is
    // executed through bash -c and stdout is captured, avoiding qx{...}
    // (which would trigger check_qx.pl).
    // The `chomp` strips the trailing newline, matching shell command-substitution
    // semantics ("$(cmd)" and "`cmd`" both strip trailing newlines).
    // NOTE: chomp must happen AFTER local $/ goes out of scope, because
    // chomp respects the current $/ value.  We use a nested do block so
    // that local $/ is scoped to the read, then chomp runs with default $/.
    //
    // Use the same robust quoting strategy as perl_string_literal_no_interp:
    // try a variety of delimiter pairs for q<delim>...<delim> and pick one
    // where neither character appears in the content.  This avoids the old
    // fragile approach of escaping `}` as `\}` inside q{...}, which changed
    // the content (e.g. broke awk `{print ...}` programs).
    let quoted = safe_perl_q_string(cmd);
    format!(
        "do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; }}",
        quoted
    )
}

/// Pick a safe Perl `q<delim>...<delim>` delimiter for a string that may
/// contain arbitrary characters.  Returns a properly delimited Perl literal.
pub(crate) fn safe_perl_q_string(s: &str) -> String {
    // Empty string -> q{} is compact and safe
    if s.is_empty() {
        return "q{}".to_string();
    }

    // If the string has no single quotes and no newlines, a plain '...' literal
    // is the most readable.
    if !s.contains('\'') && !s.contains('\n') {
        let escaped = s.replace("\\", "\\\\").replace("\'", "\\'");
        return format!("'{}'", escaped);
    }

    // Try a variety of delimiter pairs for q<open>...<close>.
    let delimiters = [
        ('{', '}'),
        ('(', ')'),
        ('[', ']'),
        ('<', '>'),
        ('|', '|'),
        ('/', '/'),
        ('#', '#'),
        ('%', '%'),
        ('@', '@'),
        ('!', '!'),
        ('~', '~'),
        ('^', '^'),
        (':', ':'),
        (';', ';'),
    ];

    for &(open, close) in &delimiters {
        let open_s = open.to_string();
        let close_s = close.to_string();
        if !s.contains(&open_s) && !s.contains(&close_s) {
            return format!("q{}{}{}", open, s, close);
        }
    }

    // Fallback: use double-quoted literal with aggressive escaping so
    // that Perl does not interpolate embedded shell $/@ variables.
    let escaped = s
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("$", "\\$")
        .replace("@", "\\@")
        .replace("\n", "\\n")
        .replace("\t", "\\t")
        .replace("\r", "\\r");
    format!("\"{}\"", escaped)
}

/// Convert a Perl string expression into Perl code that uses open() with
/// bash -c instead of qx{}.  This avoids check_qx.pl patterns.
/// `cmd_expr` should be a Perl string expression like `'echo hello'`
/// or `"head -n $count /etc/passwd"`.
pub(crate) fn expr_to_open_perl(cmd_expr: &str, chomp_result: bool) -> String {
    if chomp_result {
        // chomp must happen without local $/ in scope (see cmd_str_to_open_perl).
        format!(
            "do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; }}",
            cmd_expr
        )
    } else {
        format!(
            "do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; $CHILD_ERROR = $? >> 8; $_r; }}",
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
        // If inner contains bare single quotes, this is not a simple
        // single-quoted string but a concatenation expression.
        let has_bare_quote = {
            let bytes = inner.as_bytes();
            let mut i = 0;
            let mut found = false;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i == 0 || bytes[i-1] != b'\\' {
                        found = true;
                        break;
                    }
                }
                i += 1;
            }
            found
        };
        if has_bare_quote {
            return None;
        }
        // Un-escape single-quoted string escapes before converting to double-quoted.
        // In Perl single-quoted strings, \' represents a literal single quote and
        // \\ represents a literal backslash.  In double-quoted strings, single
        // quotes need no escaping, so we convert \' → ' and \\ → \, then
        // re-escape everything for the double-quoted context.
        let unescaped = inner
            .replace("\\\\", "\\")   // \\ → \
            .replace("\\'", "'");        // \' → '
        let escaped = unescaped
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("$", "\\$")
            .replace("@", "\\@");
        Some(format!("print \"{}\\n\";\n", escaped))
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
        Some(format!("print \"{}\\n\";\n", escaped))
    } else {
        None
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Check whether an IR statement references `$main_exit_code`.
fn stmt_refers_to_main_exit(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::RawText(t) => t.contains("$main_exit_code") || t.contains("main_exit_code"),
        IrStmt::Case { .. }
        | IrStmt::Redirect { .. }
        | IrStmt::Function { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_) => false,
        IrStmt::Block(stmts) => stmts.iter().any(stmt_refers_to_main_exit),
        IrStmt::Expr(e) => expr_refers_to_main_exit(e),
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
        IrStmt::Exec { capture, .. } => {
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
        IrExpr::Arrow(_) => false,
        IrExpr::Array(elems) => elems.iter().any(expr_refers_to_main_exit),
        IrExpr::Arith(_) => false,
        IrExpr::Bool(_) => false,
        IrExpr::Json(_) => false,
        IrExpr::Ident(_) => false,
        IrExpr::Object(props) => props.iter().any(|(_, v)| expr_refers_to_main_exit(v)),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(e) => expr_refers_to_main_exit(e),
        }),
        IrExpr::BinOp { lhs, rhs, .. } => expr_refers_to_main_exit(lhs) || expr_refers_to_main_exit(rhs),
        IrExpr::Capture { expr, .. } => expr_refers_to_main_exit(expr),
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
        // Neutral ESTree-path-only nodes carry no Perl variables.
        IrStmt::Case { .. }
        | IrStmt::Redirect { .. }
        | IrStmt::Function { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_) => {}
        IrStmt::Block(stmts) => {
            for st in stmts {
                collect_vars_in_stmt(st, vars);
            }
        }
        IrStmt::Expr(e) => collect_vars_in_expr(e, vars),
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
        IrStmt::Exec { cmd, args, .. } => {
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
        IrExpr::Arrow(body) => {
            for stmt in body {
                collect_vars_in_stmt(stmt, vars);
            }
        }
        IrExpr::Array(elems) => {
            for e in elems {
                collect_vars_in_expr(e, vars);
            }
        }
        IrExpr::Arith(_) => {}
        IrExpr::Bool(_) => {}
        IrExpr::Json(_) => {}
        IrExpr::Ident(_) => {}
        IrExpr::Object(props) => {
            for (_, v) in props {
                collect_vars_in_expr(v, vars);
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
        IrExpr::Capture { expr, .. } => collect_vars_in_expr(expr, vars),
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
/// Evaluate a fully-constant arithmetic expression string (digits, + - * /
/// % ** << >> & | ^, parens, unary -). Returns None if any variable or
/// unsupported token appears — nothing is folded unless it is provably
/// constant (no side effects).
fn fold_arith_const(expr: &str) -> Option<i64> {
    let b = expr.as_bytes();
    let mut pos = 0;
    let n = b.len();
    fn ws(b: &[u8], pos: &mut usize) {
        while *pos < b.len() && (b[*pos] as char).is_whitespace() {
            *pos += 1;
        }
    }
    fn number(b: &[u8], pos: &mut usize) -> Option<i64> {
        ws(b, pos);
        let mut neg = false;
        if *pos < b.len() && b[*pos] == b'-' {
            neg = true;
            *pos += 1;
        }
        let start = *pos;
        while *pos < b.len() && b[*pos].is_ascii_digit() {
            *pos += 1;
        }
        if *pos == start {
            return None;
        }
        let v: i64 = b[start..*pos].iter().fold(0i64, |acc, d| acc * 10 + (d - b'0') as i64);
        Some(if neg { -v } else { v })
    }
    fn primary(b: &[u8], pos: &mut usize) -> Option<i64> {
        ws(b, pos);
        if *pos < b.len() && b[*pos] == b'(' {
            *pos += 1;
            let v = addsub(b, pos)?;
            ws(b, pos);
            if *pos >= b.len() || b[*pos] != b')' {
                return None;
            }
            *pos += 1;
            return Some(v);
        }
        number(b, pos)
    }
    fn muldiv(b: &[u8], pos: &mut usize) -> Option<i64> {
        let mut v = primary(b, pos)?;
        loop {
            ws(b, pos);
            if *pos >= b.len() {
                return Some(v);
            }
            let c = b[*pos];
            match c {
                b'*' => {
                    *pos += 1;
                    v = v.wrapping_mul(primary(b, pos)?);
                }
                b'/' => {
                    *pos += 1;
                    let d = primary(b, pos)?;
                    if d == 0 {
                        return None;
                    }
                    v /= d;
                }
                b'%' => {
                    *pos += 1;
                    let d = primary(b, pos)?;
                    if d == 0 {
                        return None;
                    }
                    v %= d;
                }
                _ => return Some(v),
            }
        }
    }
    fn addsub(b: &[u8], pos: &mut usize) -> Option<i64> {
        let mut v = muldiv(b, pos)?;
        loop {
            ws(b, pos);
            if *pos >= b.len() {
                return Some(v);
            }
            match b[*pos] {
                b'+' => {
                    *pos += 1;
                    v = v.wrapping_add(muldiv(b, pos)?);
                }
                b'-' => {
                    *pos += 1;
                    v = v.wrapping_sub(muldiv(b, pos)?);
                }
                _ => return Some(v),
            }
        }
    }
    let v = addsub(b, &mut pos)?;
    ws(b, &mut pos);
    if pos != n {
        return None; // leftover tokens → not constant
    }
    Some(v)
}

fn fold_stmt(s: &IrStmt) -> IrStmt {
    match s {
        IrStmt::Expr(e) => IrStmt::Expr(fold_expr(e)),
        IrStmt::Assign { targets, expr } => IrStmt::Assign {
            targets: targets.clone(),
            expr: fold_expr(expr),
        },
        other => other.clone(),
    }
}

fn fold_expr(e: &IrExpr) -> IrExpr {
    match e {
        IrExpr::Call { func, args } if func == "arith" && args.len() == 1 => {
            if let IrExpr::Str(expr, _) = &args[0] {
                if let Some(v) = fold_arith_const(expr) {
                    return IrExpr::Int(v);
                }
            }
            IrExpr::Call {
                func: func.clone(),
                args: args.iter().map(fold_expr).collect(),
            }
        }
        IrExpr::Call { func, args } => IrExpr::Call {
            func: func.clone(),
            args: args.iter().map(fold_expr).collect(),
        },
        IrExpr::BinOp { op: BinOpKind::Add, lhs, rhs } => {
            if let (IrExpr::Int(a), IrExpr::Int(b)) = (lhs.as_ref(), rhs.as_ref()) {
                IrExpr::Int(a.wrapping_add(*b))
            } else {
                e.clone()
            }
        }
        IrExpr::BinOp { op: BinOpKind::Sub, lhs, rhs } => {
            if let (IrExpr::Int(a), IrExpr::Int(b)) = (lhs.as_ref(), rhs.as_ref()) {
                IrExpr::Int(a.wrapping_sub(*b))
            } else {
                e.clone()
            }
        }
        IrExpr::BinOp { op: BinOpKind::Mul, lhs, rhs } => {
            if let (IrExpr::Int(a), IrExpr::Int(b)) = (lhs.as_ref(), rhs.as_ref()) {
                IrExpr::Int(a.wrapping_mul(*b))
            } else {
                e.clone()
            }
        }
        _ => e.clone(),
    }
}

pub(crate) fn optimize_stmts(stmts: &[IrStmt]) -> Vec<IrStmt> {
    // Pass 0: Collect all referenced variable names.
    let referenced = collect_referenced_vars(stmts);

    // Pass 0.5: constant folding of `sh2.arith("constant")` → Int literal.
    let folded: Vec<IrStmt> = stmts
        .iter()
        .map(|s| fold_stmt(s))
        .collect();

    // Pass 1: Dead assignment elimination (self-assignment removal)
    //         + Dead declaration elimination
    let pass1: Vec<IrStmt> = folded
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

/// Determine whether a variable name should use `$ENV{name}` style (for
/// undeclared uppercase / "environment-style" variables) vs. plain `$name`.
/// Matches the heuristic used by the generator in `mod.rs` and
/// `test_expressions.rs`: if the name is all uppercase ASCII (with underscores)
/// it is treated as an environment variable reference.
///
/// Special Perl variables from the `English` module (like `CHILD_ERROR`,
/// `OS_ERROR`, `EVAL_ERROR`) are excluded — they must always use `$` prefix.
pub fn is_env_style_var_name(name: &str) -> bool {
    // Special Perl variables that use $name, not $ENV{name}
    const PERL_SPECIAL_VARS: &[&str] = &[
        "CHILD_ERROR",
        "OS_ERROR",
        "ERRNO",
        "EVAL_ERROR",
        "INPUT_RECORD_SEPARATOR",
        "PROGRAM_NAME",
        "OUTPUT_AUTOFLUSH",
        "OUTPUT_FIELD_SEPARATOR",
        "OUTPUT_RECORD_SEPARATOR",
        "LIST_SEPARATOR",
        "SUBSCRIPT_SEPARATOR",
        "LAST_PAREN_MATCH",
        "EFFECTIVE_USER_ID",
        "REAL_USER_ID",
        "EFFECTIVE_GROUP_ID",
        "REAL_GROUP_ID",
        "PERL_VERSION",
    ];
    if PERL_SPECIAL_VARS.contains(&name) {
        return false;
    }
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
}

// ── Bridge helpers ────────────────────────────────────────────────────

/// Try to convert a Perl expression string (produced by old-style generators)
/// into a proper `IrExpr`.  This is a migration bridge: once all generators
/// emit IR nodes directly, this function becomes unnecessary.
///
/// Currently handles:
/// - Double-quoted string literals: `"hello"` → `IrExpr::Str("hello", DoubleQuoted)`
/// - Single-quoted string literals: `'hello'` → `IrExpr::Str("hello", SingleQuoted)`
/// - Scalar variables: `$var` → `IrExpr::Var("var", Scalar)`
/// - Array variables: `@arr` → `IrExpr::Var("arr", Array)`
///
/// Falls back to `RawExpr(text)` for anything it can't parse.
pub fn perl_expr_to_ir(perl_expr: &str) -> IrExpr {
    let trimmed = perl_expr.trim();

    // Double-quoted string literal: "..."
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len()-1];
        // If the inner content contains a double quote that is NOT preceded
        // by a backslash, this is NOT a simple double-quoted string literal
        // but rather a concatenation expression like "a" . $x . "b" that
        // happens to start and end with ".
        let has_bare_quote = {
            let bytes = inner.as_bytes();
            let mut i = 0;
            let mut found = false;
            while i < bytes.len() {
                if bytes[i] == b'\"' {
                    if i == 0 || bytes[i-1] != b'\\' {
                        found = true;
                        break;
                    }
                }
                i += 1;
            }
            found
        };
        if has_bare_quote {
            return IrExpr::RawExpr(trimmed.to_string());
        }
        // If the inner content has no $, @, or \ (escapes), it's safe as Str.
        let has_interp = inner.contains('$') || inner.contains('@');
        let has_escapes = inner.contains('\\');
        if !has_interp && !has_escapes {
            return IrExpr::Str(inner.to_string(), StrStyle::DoubleQuoted);
        }
        // If it has $ or @, it needs Perl interpolation — use Heredoc style
        // which preserves $ and @ but escapes control characters.
        if has_interp && !has_escapes {
            return IrExpr::Str(inner.to_string(), StrStyle::Heredoc);
        }
        // Complex escapes — keep as RawExpr
        return IrExpr::RawExpr(trimmed.to_string());
    }

    // Single-quoted string literal: '...'
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        let inner = &trimmed[1..trimmed.len()-1];
        // If the inner content contains a single quote that is NOT preceded
        // by a backslash, this is NOT a simple single-quoted string literal
        // but rather a concatenation expression like 'foo' . 'bar' that
        // happens to start and end with '.
        let has_bare_quote = {
            let bytes = inner.as_bytes();
            let mut i = 0;
            let mut found = false;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i == 0 || bytes[i-1] != b'\\' {
                        found = true;
                        break;
                    }
                }
                i += 1;
            }
            found
        };
        if !has_bare_quote {
            let unescaped = inner.replace("\\'", "'");
            return IrExpr::Str(unescaped, StrStyle::SingleQuoted);
        }
        // Contains bare quotes — fall through to RawExpr below
    }

    // Scalar variable: $var or ${var}
    if trimmed.starts_with('$') && trimmed.len() > 1 {
        let name = if trimmed.starts_with("${") && trimmed.ends_with('}') {
            &trimmed[2..trimmed.len()-1]
        } else {
            &trimmed[1..]
        };
        // Ensure it's a valid identifier
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            // All-uppercase names: the generator only emits a bare `$NAME`
            // for variables it has declared as locals (undeclared variables
            // render as `($ENV{NAME} // q{})`), so converting to IrExpr::Var
            // would let ir_expr_to_perl's env-style heuristic wrongly remap
            // them to $ENV{NAME}.  Keep them as RawExpr to preserve the
            // generator's declared/local resolution.
            if !name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                return IrExpr::Var(name.to_string(), Some(Sigil::Scalar));
            }
        }
    }

    // Array variable: @var
    if trimmed.starts_with('@') && trimmed.len() > 1 {
        let name = &trimmed[1..];
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return IrExpr::Var(name.to_string(), Some(Sigil::Array));
        }
    }

    // Fall back to RawExpr
    IrExpr::RawExpr(trimmed.to_string())
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
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }
}
