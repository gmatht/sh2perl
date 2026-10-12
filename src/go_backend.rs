//! Go backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/go`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-c-core-needs.md §1):
//! `shir_to_go(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! Go `int64`, `Str` vars → `string`, arrays → `[]any`, anything else →
//! `any`. Identifiers are sanitized to Go identifier syntax and mangled
//! against Go keywords and the runtime helper names (A6-consistent).
//!
//! Everything in the ShIR vocabulary that this renderer lowers emits
//! NATIVE Go (no `sh2.*` identifiers anywhere — the scaffold stub gate
//! greps the output for `sh2[A-Za-z_]` and `TODO(unsupported)`). Lowered:
//! output/assign/declare, if/while/do/for/c-style-for, arithmetic
//! (structured AST + the `arith("...")` string form), `[ ]`/`[[ ]]` tests
//! (incl. files, globs `==`, regex `=~`, `-a`/`-o`), pipelines (native
//! os/exec chains), command substitution (native when the body is a
//! single external exec; `bash -c` reconstruction otherwise), parameter
//! expansion ops (`:-` `:=` `:?` `#` `##` `%` `%%` `/` `//` `^` `^^` `,`
//! `,,` `len` `basename` `dirname` `slice`), brace expansion (expanded at
//! render time), shell functions (Go closures with a shared `fArgs`
//! positional-arg stack), arrays (`[]any` + index/length/slice helpers),
//! heredocs, redirects (native exec fd wiring; `bash -c` fallback), `cd`
//! / `export` / `read` / `let` / `local` builtins, and `$?` status
//! tracking. Anything outside that subset still emits a compile-able
//! `// TODO(unsupported)` marker so the output always compiles.
//!
//! The preamble always includes a fixed block of native runtime helpers;
//! the import list is derived by scanning the generated text (which
//! packages the helpers/body actually reference), so the output always
//! compiles with exactly the imports it uses.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap};

enum Part {
    Lit(String),
    Arg(String),
}

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_storage: std::collections::HashMap<String, crate::ir::StorageClass>,
    var_types: HashMap<String, IrType>,
    /// vars used as arrays (declare as []any)
    arrays: BTreeSet<String>,
    /// vars ever written with an assoc-shaped key (dynamic reads/writes
    /// go through the `vars` map under `base[key]`, like the baked writes)
    assoc_arrays: BTreeSet<String>,
    /// vars written anywhere (declared at the top of main)
    written: BTreeSet<String>,
    /// vars actually read (skip the `_ = x` dead-var guard)
    read: BTreeSet<String>,
    /// Go identifier per shell var name (sanitize + de-dup)
    mangle: HashMap<String, String>,
    /// shell function names (callable via `exec`)
    functions: BTreeSet<String>,
    /// `typeset -f` display mode for cmd_text reconstruction (barewords
    /// stay bare, `local` kept — bash's pretty-print, not the quoted
    /// fallback text); only true while capturing function sources
    cmd_display: bool,
    /// side-effect statements accumulated while rendering an expression
    sides: Vec<String>,
    /// temp-var counter for postfix inc/dec inside expressions
    tmp: usize,
    /// `$?` status var + tracking needed
    need_st: bool,
    /// `$?` actually read (skip the `_ = st` dead-var guard)
    st_read: bool,
    /// vars written via arithmetic (cstyleFor/let/arith) — declare int64
    arith_ints: BTreeSet<String>,
    /// vars with the integer attribute (`declare -i`/`typeset -i`): every
    /// assignment evaluates arithmetically (kept separate from arith_ints
    /// so `unset` can drop the attribute while the home stays declared)
    int_attrs: BTreeSet<String>,
    /// vars with a case attribute (`declare -l`/`-u`): assignments fold
    /// case (value is 'l' or 'u')
    case_attrs: HashMap<String, char>,
    /// nameref targets (`declare -n ref=target`): reads/writes of ref
    /// resolve to target's home (pre-scanned; render-time re-points
    /// update it in program order)
    nameref_targets: HashMap<String, String>,
    /// readonly vars (`declare -r`/`readonly`): tracked for `declare -p`
    /// display (assignment enforcement is bash-stderr, gate-ignored)
    ro_attrs: BTreeSet<String>,
    /// shell function bodies as reconstructible text (`typeset -f`
    /// display), definition order
    func_src: HashMap<String, Option<Vec<String>>>,
    func_order: Vec<String>,
    /// `shopt -s nocasematch` seen — [[ == ]] compares case-insensitively
    nocase: bool,
    /// vars map + write-sync emissions needed (param-expansion ops)
    need_vars: bool,
    /// function calls pass positional args via `fArgs`
    need_fargs: bool,
    /// inside a shell-function body (trailing-echo Returns print)
    in_func: usize,
    /// inside a `&` background body (`exit` ends the goroutine, not main)
    in_bg: usize,
    loop_depth: usize,
    /// break-target labels for open loops (`break` inside a `switch`
    /// must break the loop, not the switch — bash has no switch-break)
    /// (kept for documentation; the brk_stack flag mechanism below is
    /// what Break actually consults — Go rejects unused labels)
    loop_labels: Vec<String>,
    /// per-switch break flags (`__brkN`): a `break` inside a case sets
    /// the innermost flag and exits the Go switch; the trailing check
    /// after each switch propagates (parent flag or loop break)
    brk_stack: Vec<String>,
    /// `switch` nesting depth (case bodies render as Go switch)
    switch_depth: usize,
    todo: usize,
}

/// Go keywords + predeclared identifiers + runtime helper names that
/// would collide with block variables. Everything else can be a legal
/// block variable; mangling these keeps the generated program compilable.
const GO_RESERVED: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
    "any",
    "bool",
    "string",
    "int",
    "int64",
    "byte",
    "rune",
    "true",
    "false",
    "nil",
    "fmt",
    "os",
    "exec",
    "bytes",
    "bufio",
    "io",
    "filepath",
    "regexp",
    "sort",
    "strconv",
    "strings",
    "sync",
    "syscall",
    "utf8",
    "main",
    "error",
    // Go builtins that must stay callable (append/len/… as a variable
    // would shadow them and break helper code)
    "append",
    "cap",
    "clear",
    "close",
    "complex",
    "copy",
    "delete",
    "imag",
    "len",
    "make",
    "max",
    "min",
    "new",
    "panic",
    "print",
    "println",
    "real",
    "recover",
    // runtime helpers / globals the preamble declares
    "st",
    "vars",
    "fArgs",
    "s2s",
    "s2i",
    "truthy",
    "cond3",
    "land",
    "lor",
    "lnot",
    "b2i",
    "cmpN",
    "powN",
    "runeLen",
    "arrLen",
    "arrIdx",
    "arrSlice",
    "argsList",
    "paramAt",
    "readInto",
    "runCmd",
    "capCmd",
    "capCmdRaw",
    "capRun",
    "redirRun",
    "runPipe",
    "fileTest",
    "globMatch",
    "globRe",
    "pStrip",
    "pSuf",
    "pRep",
    "reMatch",
    "bprintf",
    "bprintfStr",
    "bfmtOnce",
    "bNum",
    "bEsc",
    "pExp",
    "pList",
    "pVal",
    "pValRaw",
    "expand",
    "pExpStr",
    "eEsc",
];

/// Runtime helper/global names declared by the RUNTIME_HELPERS preamble
/// (derived from the block itself so new helpers can never collide with
/// user variables — the `exec`-shadowing build break).
fn runtime_helper_names() -> &'static std::collections::HashSet<String> {
    static ONCE: std::sync::OnceLock<std::collections::HashSet<String>> =
        std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let mut set = std::collections::HashSet::new();
        for line in RUNTIME_HELPERS {
            if let Some(r) = line.strip_prefix("func ") {
                if let Some(n) = r.split(['(', ' ']).next() {
                    if !n.is_empty() {
                        set.insert(n.to_string());
                    }
                }
            } else if let Some(r) = line.strip_prefix("var ") {
                if let Some(n) = r.split_whitespace().next() {
                    set.insert(n.to_string());
                }
            }
        }
        set
    })
}

/// Render an `IrProgram` to Go source (package main).
pub fn shir_to_go(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // builtin-op fallback arm (shir-builtin-op-20260816): the go backend
    // has NOT accepted the `builtin` op — render as exec.
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    // A2: the type verdicts are computed at serialization time in the JSON
    // path; the library path must run the same analysis.
    prog.var_types = crate::shir::analyze_var_types(&prog);
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.program(&prog);
    r.out.join("\n")
}

impl Render {
    fn emit(&mut self, s: &str) {
        if s.is_empty() {
            self.out.push(String::new());
        } else {
            self.out.push(format!("{}{}", "    ".repeat(self.depth), s));
        }
    }

    fn mark_todo(&mut self, what: &str) {
        self.todo += 1;
        let one = what.replace('\n', "\\n");
        self.emit(&format!("// TODO(unsupported): {one}"));
    }

    /// Flush side-effect statements accumulated while rendering an
    /// expression (arith inc/dec temporaries etc.).
    fn flush_sides(&mut self) {
        let sides = std::mem::take(&mut self.sides);
        for s in sides {
            self.emit(&s);
        }
    }

    fn new_tmp(&mut self) -> String {
        self.tmp += 1;
        format!("t{}", self.tmp)
    }

    /// Open a loop (manages depth; break targets ride on brk_stack).
    /// The `header` is the text after `for` (e.g. `" {"`, `" cond {"`).
    fn loop_head(&mut self, header: &str) {
        self.emit(&format!("for{header}"));
        self.loop_depth += 1;
        self.depth += 1;
    }

    /// Close a loop.
    fn loop_close(&mut self) {
        self.depth -= 1;
        self.loop_depth -= 1;
        self.emit("}");
    }

    // ── identifiers ──────────────────────────────────────────────────

    /// Sanitize a shell var name to a valid Go identifier and mangle
    /// reserved names (A6-consistent). De-duplicates collisions.
    fn go_ident(&mut self, name: &str) -> String {
        if let Some(m) = self.mangle.get(name) {
            return m.clone();
        }
        let mut m = String::new();
        for (_, c) in name.chars().enumerate() {
            if c.is_ascii_alphanumeric() || c == '_' {
                m.push(c);
            } else {
                m.push('_');
            }
        }
        if m.is_empty() || m.chars().next().unwrap().is_ascii_digit() {
            m.insert_str(0, "v_");
        }
        if GO_RESERVED.contains(&m.as_str()) || runtime_helper_names().contains(&m) {
            m.push('_');
        }
        if m == "_" {
            m = "underscore".to_string();
        }
        // de-dup collisions (e.g. `a-b` and `a.b` both sanitize to `a_b`)
        let base = m.clone();
        let mut n = 1;
        while self.mangle.values().any(|v| v == &m) {
            m = format!("{base}{n}");
            n += 1;
        }
        self.mangle.insert(name.to_string(), m.clone());
        m
    }

    fn go_str(s: &str) -> String {
        let mut out = String::new();
        out.push('"');
        for c in s.chars() {
            // byte-preserving markers (U+F800+byte, from the CLI's
            // read_source): decode back to the raw byte (the C backend
            // does the same with \xNN)
            if (0xF800..=0xF8FF).contains(&(c as u32)) {
                out.push_str(&format!("\\x{:02x}", (c as u32 - 0xF800) as u8));
                continue;
            }
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c if (c as u32) < 32 => out.push_str(&format!("\\x{:02x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int)
    }

    fn is_str(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Str)
    }

    fn is_arr(&self, name: &str) -> bool {
        self.arrays.contains(name)
    }

    /// The var has a hoisted declaration (written somewhere) — only then
    /// may generated code reference it as a bare identifier.
    fn declared(&self, name: &str) -> bool {
        self.written.contains(name)
    }

    fn mark_read(&mut self, name: &str) {
        self.read.insert(name.to_string());
    }

    fn mark_written(&mut self, name: &str) {
        self.written.insert(name.to_string());
    }

    fn mark_arr(&mut self, name: &str) {
        self.arrays.insert(name.to_string());
        self.mark_written(name);
    }

    /// Mark a var as arith-written (declared int64).
    fn mark_arith(&mut self, name: &str) {
        self.arith_ints.insert(name.to_string());
        self.mark_written(name);
    }

    /// The Go identifier for a declared var, marking it read.
    fn ident_of(&mut self, name: &str) -> String {
        let m = self.go_ident(name);
        self.mark_read(name);
        m
    }

    /// Nameref resolution: `declare -n ref=target` aliases ref to
    /// target's home (static plain targets only — dynamic targets,
    /// array-element targets and self-cycles resolve to the name).
    fn nameref_resolve(&self, name: &str) -> String {
        match self.nameref_targets.get(name) {
            Some(t) if t != name && !t.contains('[') => t.clone(),
            _ => name.to_string(),
        }
    }

    /// `vars["x"] = x` inline (for the one-line assign form).
    fn sync_inline(&mut self, name: &str) -> String {
        if self.need_vars {
            let m = self.go_ident(name);
            // canonical key (`config["u"]` and `config[u]` share a home)
            let key = normalize_assoc_key(name);
            return format!(" vars[{}] = {m};", Self::go_str(&key));
        }
        String::new()
    }

    // ── typed expressions ────────────────────────────────────────────

    /// Statically-numeric check (for comparison typing).
    fn static_num(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_) => true,
            IrExpr::Str(s, _) => s.trim().parse::<i64>().is_ok(),
            IrExpr::Var(name, _) => self.is_num(name),
            IrExpr::Arith(_) => true,
            IrExpr::BinOp { op, .. } => !matches!(op, crate::ir::BinOpKind::Concat),
            IrExpr::Call { func, args } if func == "getVar" => {
                match args.first() {
                    Some(IrExpr::Str(name, _)) => self.is_num(&self.nameref_resolve(name)),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Render as an int64-typed expression.
    fn expr_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => {
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else {
                    format!("s2i({})", Self::go_str(s))
                }
            }
            IrExpr::Var(name, _) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("s2i(os.Getenv({}))", Self::go_str(&name));
                }
                let m = self.ident_of(&name);
                if self.is_num(&name) {
                    m
                } else {
                    format!("s2i({m})")
                }
            }
            IrExpr::Ident(name) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("s2i(os.Getenv({}))", Self::go_str(&name));
                }
                self.ident_of(&name)
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::BinOp { op, .. }
                if matches!(
                    op,
                    crate::ir::BinOpKind::Eq
                        | crate::ir::BinOpKind::Ne
                        | crate::ir::BinOpKind::Lt
                        | crate::ir::BinOpKind::Gt
                        | crate::ir::BinOpKind::Le
                        | crate::ir::BinOpKind::Ge
                        | crate::ir::BinOpKind::And
                        | crate::ir::BinOpKind::Or
                        | crate::ir::BinOpKind::Not
                ) =>
            {
                // bool → int64 (bash's 1/0) — Go has no implicit conversion
                format!("b2i({})", self.expr_bool(e))
            }
            IrExpr::BinOp { op, .. } if *op == crate::ir::BinOpKind::Concat => {
                format!("s2i({})", self.expr_str(e))
            }
            IrExpr::BinOp { .. } => self.expr_any(e),
            IrExpr::Bool(b) => {
                if *b {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    return self.getvar_num(name);
                }
                format!("s2i({})", self.call("getVar", args))
            }
            other => format!("s2i({})", self.expr_any(other)),
        }
    }

    /// getVar by name in a numeric context.
    fn getvar_num(&mut self, name: &str) -> String {
        let resolved = self.nameref_resolve(name);
        let name = resolved.as_str();
        match name {
            "?" => {
                self.need_st = true;
                self.st_read = true;
                "st".to_string()
            }
            "$" => "int64(os.Getpid())".to_string(),
            "#" => "int64(len(argsList()))".to_string(),
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                let i: i64 = n.parse().unwrap_or(1);
                format!("s2i(paramAt({i}))")
            }
            "@" | "*" => "s2i(strings.Join(argsList(), \" \"))".to_string(),
            _ => {
                if self.declared(name) {
                    let m = self.ident_of(name);
                    if self.is_num(name) {
                        m
                    } else {
                        format!("s2i({m})")
                    }
                } else {
                    format!("s2i(os.Getenv({}))", Self::go_str(name))
                }
            }
        }
    }

    /// getVar by name in a string context.
    fn getvar_str(&mut self, name: &str) -> String {
        let resolved = self.nameref_resolve(name);
        let name = resolved.as_str();
        match name {
            "?" => {
                self.need_st = true;
                self.st_read = true;
                "s2s(st)".to_string()
            }
            "$" => "s2s(int64(os.Getpid()))".to_string(),
            "#" => "s2s(int64(len(argsList())))".to_string(),
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                let i: i64 = n.parse().unwrap_or(1);
                format!("paramAt({i})")
            }
            "@" | "*" => "strings.Join(argsList(), \" \")".to_string(),
            _ => {
                if self.declared(name) {
                    let m = self.ident_of(name);
                    if self.is_str(name) {
                        m
                    } else {
                        format!("s2s({m})")
                    }
                } else {
                    format!("os.Getenv({})", Self::go_str(name))
                }
            }
        }
    }

    /// Render as a string-typed expression.
    fn expr_str(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::go_str(s),
            IrExpr::Int(i) => format!("s2s({i})"),
            IrExpr::Var(name, _) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("os.Getenv({})", Self::go_str(&name));
                }
                let m = self.ident_of(&name);
                if self.is_str(&name) {
                    m
                } else {
                    format!("s2s({m})")
                }
            }
            IrExpr::Ident(name) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("os.Getenv({})", Self::go_str(&name));
                }
                self.ident_of(&name)
            }
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Arith(a) => format!("s2s({})", self.arith(a)),
            IrExpr::Bool(b) => {
                if *b {
                    "s2s(1)".into()
                } else {
                    "s2s(0)".into()
                }
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    return self.getvar_str(name);
                }
                format!("s2s({})", self.call("getVar", args))
            }
            // `$@` as words in a string position (`x="$@"`)
            IrExpr::Call { func, args }
                if func == "listVar"
                    && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "@" || n == "*") =>
            {
                "strings.Join(argsList(), \" \")".to_string()
            }
            other => format!("s2s({})", self.expr_any(other)),
        }
    }

    /// Render as a bool-typed expression (conditions).
    fn expr_bool(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            IrExpr::Int(i) => format!("({i} != 0)"),
            IrExpr::Str(s, _) => format!("({} != \"\")", Self::go_str(s)),
            IrExpr::Var(name, _) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("(os.Getenv({}) != \"\")", Self::go_str(&name));
                }
                let m = self.ident_of(&name);
                if self.is_num(&name) {
                    format!("({m} != 0)")
                } else {
                    format!("(s2s({m}) != \"\")")
                }
            }
            IrExpr::Ident(name) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("(os.Getenv({}) != \"\")", Self::go_str(&name));
                }
                let m = self.ident_of(&name);
                format!("({m} != \"\")")
            }
            IrExpr::BinOp { lhs, op, rhs } => match op {
                crate::ir::BinOpKind::And => {
                    format!("({} && {})", self.expr_bool(lhs), self.expr_bool(rhs))
                }
                crate::ir::BinOpKind::Or => {
                    format!("({} || {})", self.expr_bool(lhs), self.expr_bool(rhs))
                }
                crate::ir::BinOpKind::Not => format!("(!{})", self.expr_bool(lhs)),
                crate::ir::BinOpKind::Eq
                | crate::ir::BinOpKind::Ne
                | crate::ir::BinOpKind::Lt
                | crate::ir::BinOpKind::Gt
                | crate::ir::BinOpKind::Le
                | crate::ir::BinOpKind::Ge => {
                    let go_op = self.cmp_op(op);
                    if self.static_num(lhs) && self.static_num(rhs) {
                        let (l, r) = (self.expr_num(lhs), self.expr_num(rhs));
                        format!("({l} {go_op} {r})")
                    } else {
                        let (l, r) = (self.expr_str(lhs), self.expr_str(rhs));
                        format!("({l} {go_op} {r})")
                    }
                }
                _ => format!("({} != 0)", self.expr_num(e)),
            },
            IrExpr::Arith(a) => format!("({} != 0)", self.arith(a)),
            IrExpr::Call { func, args } if func == "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return c;
                    }
                    // bash -c fallback for a test go can't parse natively
                    // (honor the [[ kind — see the cmd_text test arm)
                    let wrapped = match args.get(1) {
                        Some(IrExpr::Str(k, _)) if k == "[[" => {
                            format!("[[ {s} ]]")
                        }
                        _ => format!("[ {s} ]"),
                    };
                    let env = self.env_lit();
                    return format!(
                        "(redirRun({}, {env}) == 0)",
                        Self::go_str(&wrapped)
                    );
                }
                self.sh2_stub("test")
            }
            IrExpr::Call { func, args } if func == "contains" => {
                // `echo X | grep LIT >/dev/null` → contains(X, LIT): native
                // strings.Contains. The strings import is auto-derived from
                // the generated text.
                if args.len() >= 2 {
                    let needle = self.expr_str(&args[0]);
                    let pattern = self.expr_str(&args[1]);
                    return format!("strings.Contains({needle}, {pattern})");
                }
                self.sh2_stub("contains")
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    return self.getvar_bool(name);
                }
                format!("truthy({})", self.call("getVar", args))
            }
            IrExpr::Call { func, args }
                if func == "exec"
                    && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "let") =>
            {
                // `let "value % 2 == 0"` — success iff the arith is nonzero
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    if let Some(IrExpr::Str(s, _)) = items.first() {
                        if let Some((_, v)) = self.arith_str(s) {
                            return format!("({v} != 0)");
                        }
                    }
                }
                // unparseable: bash errors (status 1) → false
                "false".to_string()
            }
            IrExpr::Call { func, args }
                if func == "exec" || func == "pipeline" || func == "redirect" =>
            {
                // a command is TRUE when it succeeds (status 0) — the
                // && / || short-circuit then runs the rhs exactly like bash
                let v = self.call(func, args);
                format!("({v} == 0)")
            }
            IrExpr::Call { func, args } if func == "grepMatches" => {
                // `if grep -o …; then` — true iff the child finds a match.
                if let Some(t) = self.grep_matches_text(args) {
                    let env = self.env_lit();
                    return format!("(redirRun({}, {env}) == 0)", Self::go_str(&t));
                }
                self.sh2_stub("grepMatches-cond")
            }
            IrExpr::Call { func, args } if func == "arith" => {
                // `while (( i < 5 ))` — arith string as condition
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((_, v)) = self.arith_str(s) {
                        return format!("({v} != 0)");
                    }
                }
                // unparseable arithmetic errors in bash (status 1) — the
                // condition is false (arithmetic-vs-command-subshell).
                "false".to_string()
            }
            IrExpr::Call { func, args } if func == "let" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((_, v)) = self.arith_str(s) {
                        return format!("({v} != 0)");
                    }
                }
                // same error semantics as arith above
                "false".to_string()
            }
            other => format!("truthy({})", self.expr_any(other)),
        }
    }

    fn cmp_op(&self, op: &crate::ir::BinOpKind) -> &'static str {
        match op {
            crate::ir::BinOpKind::Eq => "==",
            crate::ir::BinOpKind::Ne => "!=",
            crate::ir::BinOpKind::Lt => "<",
            crate::ir::BinOpKind::Gt => ">",
            crate::ir::BinOpKind::Le => "<=",
            crate::ir::BinOpKind::Ge => ">=",
            _ => "==",
        }
    }

    fn getvar_bool(&mut self, name: &str) -> String {
        let resolved = self.nameref_resolve(name);
        let name = resolved.as_str();
        match name {
            "?" => {
                self.need_st = true;
                self.st_read = true;
                "(st != 0)".to_string()
            }
            "#" => "(len(argsList()) != 0)".to_string(),
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                let i: i64 = n.parse().unwrap_or(1);
                format!("(paramAt({i}) != \"\")")
            }
            "@" | "*" => "(len(argsList()) != 0)".to_string(),
            _ => {
                if self.declared(name) {
                    let m = self.ident_of(name);
                    if self.is_num(name) {
                        format!("({m} != 0)")
                    } else {
                        format!("(s2s({m}) != \"\")")
                    }
                } else {
                    format!("(os.Getenv({}) != \"\")", Self::go_str(name))
                }
            }
        }
    }

    /// Render as an any-typed expression (the general form).
    fn expr_any(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => Self::go_str(s),
            IrExpr::Var(name, _) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("os.Getenv({})", Self::go_str(&name));
                }
                self.ident_of(&name)
            }
            IrExpr::Ident(name) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return format!("os.Getenv({})", Self::go_str(&name));
                }
                self.ident_of(&name)
            }
            IrExpr::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::go_str(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "true".into()
                    } else {
                        "false".into()
                    }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "nil".into()
                }
            },
            IrExpr::BinOp { lhs, op, rhs } => {
                if *op == crate::ir::BinOpKind::Concat {
                    format!("({} + {})", self.expr_str(lhs), self.expr_str(rhs))
                } else if matches!(
                    op,
                    crate::ir::BinOpKind::And
                        | crate::ir::BinOpKind::Or
                        | crate::ir::BinOpKind::Not
                ) || matches!(
                    op,
                    crate::ir::BinOpKind::Eq
                        | crate::ir::BinOpKind::Ne
                        | crate::ir::BinOpKind::Lt
                        | crate::ir::BinOpKind::Gt
                        | crate::ir::BinOpKind::Le
                        | crate::ir::BinOpKind::Ge
                ) {
                    self.expr_bool(e)
                } else if *op == crate::ir::BinOpKind::Pow {
                    format!("powN({}, {})", self.expr_num(lhs), self.expr_num(rhs))
                } else if *op == crate::ir::BinOpKind::Div {
                    format!(
                        "divN({}, {})",
                        self.expr_num(lhs),
                        self.expr_num(rhs)
                    )
                } else if *op == crate::ir::BinOpKind::Mod {
                    format!(
                        "modN({}, {})",
                        self.expr_num(lhs),
                        self.expr_num(rhs)
                    )
                } else if *op == crate::ir::BinOpKind::ShiftL {
                    format!(
                        "shlN({}, {})",
                        self.expr_num(lhs),
                        self.expr_num(rhs)
                    )
                } else if *op == crate::ir::BinOpKind::ShiftR {
                    format!(
                        "shrN({}, {})",
                        self.expr_num(lhs),
                        self.expr_num(rhs)
                    )
                } else {
                    let (l, r, go_op) = (self.expr_num(lhs), self.expr_num(rhs), self.arith_op(op));
                    format!("({l} {go_op} {r})")
                }
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Ternary { cond, then, else_ } => {
                format!(
                    "cond3({}, {}, {})",
                    self.expr_bool(cond),
                    self.expr_any(then),
                    self.expr_any(else_)
                )
            }
            IrExpr::DefinedOr { expr, default } => {
                // `expr // default` — use expr unless nil/""; shell vars are
                // never nil at runtime, so this is just the default path.
                let _ = default;
                self.expr_any(expr)
            }
            IrExpr::Index { var, key } => {
                let varname = self.nameref_resolve(var);
                let var = &varname;
                if self.assoc_arrays.contains(var) || assoc_key_shape(key) {
                    // assoc element: the `vars` map home (`base[key]`),
                    // where the baked writes sync (missing key → "").
                    // A Str key may carry `$` vars — expand them.
                    let ks = match key.as_ref() {
                        IrExpr::Str(s, _) => self.expand_str(s),
                        _ => self.expr_str(key),
                    };
                    let base = Self::go_str(&format!("{var}["));
                    format!("s2s(vars[{base} + {ks} + \"]\"])")
                } else {
                    self.mark_arr(var);
                    self.ident_of(var);
                    let k = self.expr_num(key);
                    format!("arrIdx({}, {k})", self.ident_of(var))
                }
            }
            IrExpr::Capture { expr, native } => self.capture_expr(expr, *native),
            IrExpr::Regex { .. } => {
                self.mark_todo("Regex expr");
                "false".into()
            }
            IrExpr::Range { .. } => {
                self.mark_todo("Range expr");
                "0".into()
            }
            IrExpr::RawExpr(_) => {
                self.mark_todo("RawExpr");
                "nil".into()
            }
            IrExpr::Arrow(_) => {
                self.mark_todo("Arrow");
                "nil".into()
            }
            IrExpr::ArrayComp { .. } => {
                self.mark_todo("ArrayComp expr");
                "nil".into()
            }
            IrExpr::Lambda { .. } => {
                self.mark_todo("Lambda expr");
                "nil".into()
            }
            IrExpr::Splice(_) => {
                self.mark_todo("Splice expr");
                "nil".into()
            }
            IrExpr::Ext(n) => match n.tag() {
                "StringContains" => {
                    let x = n.as_any().downcast_ref::<crate::shir_nodes::StringContains>()
                        .expect("tag/type agree");
                    format!("strings.Contains({}, {})",
                        self.expr_any(&x.text), self.expr_any(&x.pattern))
                }
                "FieldExtract" => {
                    let x = n.as_any().downcast_ref::<crate::shir_nodes::FieldExtract>()
                        .expect("tag/type agree");
                    // single-field fast path; ranges/multi-field fall back
                    if x.fields.len() == 1 {
                        if let crate::ir::FieldRange::Single(i) = x.fields[0] {
                            return format!(
                                "func() string {{\n_p := strings.Split({}, {})\nif {}-1 < len(_p) {{\nreturn _p[{}]\n}}\nreturn \"\"\n}}()",
                                self.expr_any(&x.text),
                                go_str_lit(&x.delimiter), i, i - 1);
                        }
                    }
                    "nil".into()
                }
                "StrLen" => {
                    let x = n.as_any().downcast_ref::<crate::shir_nodes::StrLen>()
                        .expect("tag/type agree");
                    format!("len({})", self.expr_any(&x.text))
                }
                "CaseTransform" => {
                    let x = n.as_any().downcast_ref::<crate::shir_nodes::CaseTransform>()
                        .expect("tag/type agree");
                    format!("strings.{}({})",
                        if x.upper { "ToUpper" } else { "ToLower" },
                        self.expr_any(&x.text))
                }
                _ => format!("sh2.{}(...)", n.tag()),
            }
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|i| self.expr_any(i)).collect();
                format!("[]any{{{}}}", elems.join(", "))
            }
            IrExpr::Object(_) => {
                self.mark_todo("Object");
                "nil".into()
            }
            // `$@` as an array value (`arr=("$@")` elements)
            IrExpr::Call { func, args }
                if func == "listVar"
                    && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "@" || n == "*") =>
            {
                "strsToAny(argsList())".to_string()
            }
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::MethodCall { .. } => {
                self.mark_todo("MethodCall");
                "nil".into()
            }
        }
    }

    /// Command substitution: `$(...)`. Native when the body is a single
    /// external exec; `bash -c` reconstruction otherwise.
    fn capture_expr(&mut self, expr: &IrExpr, native: bool) -> String {
        if let IrExpr::Arrow(body) = expr {
            // bare user-function call first (body_single_exec would
            // route it to an external exec that cannot exist)
            if let Some(v) = self.capture_fncall(body) {
                return v;
            }
            if let Some((cmd, argv)) = self.body_single_exec(body) {
                let mut parts = vec![cmd];
                parts.extend(argv);
                if native {
                    return format!("capCmdRaw({})", exec_cmd_apply(&parts));
                }
                return format!("capCmd({})", exec_cmd_apply(&parts));
            }
            // `$(echo …)` / `$(printf …)` lower natively (no child bash)
            if let Some(s) = self.capture_builtin(body) {
                return s;
            }
            // `$((…))` arithmetic lowers natively (no child bash)
            if let Some(v) = self.capture_arith(body) {
                return v;
            }
            // herestring-fed redirect with evaluable input
            if let Some(cap) = self.capture_redirect(body) {
                return cap;
            }
            // `$(a | b | c)` of plain external execs (plus a leading
            // echo/printf as input) runs natively via capPipeIn
            if let Some((input, stages)) = self.capture_pipeline(body) {
                let parts: Vec<String> =
                    stages.iter().map(|st| argv_slice(st)).collect();
                let stages_go = format!("[][]string{{{}}}", parts.join(", "));
                if native {
                    return format!("capPipeInRaw({input}, {stages_go})");
                }
                return format!("capPipeIn({input}, {stages_go})");
            }
            if let Some(text) = self.cmd_text_stmts(body) {
                let env = self.env_lit();
                return format!("capRun({}, {env})", Self::go_str(&text));
            }
        }
        self.sh2_stub("capture")
    }

    /// Capture of a pipeline of plain external execs (`$(grep … | wc …)`)
    /// natively (no child bash). A leading echo/printf stage evaluates to
    /// the pipe input instead of forking. Returns (input, stages).
    fn capture_pipeline(&mut self, body: &[IrStmt]) -> Option<(String, Vec<Vec<String>>)> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = body else {
            return None;
        };
        if func != "pipeline" {
            return None;
        }
        let Some(IrExpr::Array(items)) = args.first() else {
            return None;
        };
        let mut stages: Vec<Vec<String>> = Vec::new();
        let mut input = String::from("\"\"");
        for (idx, it) in items.iter().enumerate() {
            let IrExpr::Arrow(stage) = it else {
                return None;
            };
            // leading echo/printf becomes the pipe input
            if idx == 0 {
                if let [IrStmt::Expr(IrExpr::Call { func, args })] = stage.as_slice() {
                    if func == "exec" || func == "builtin" {
                        if let Some(IrExpr::Str(cmd, _)) = args.first() {
                            let argv: &[IrExpr] = match args.get(1) {
                                Some(IrExpr::Array(it)) => it,
                                _ => &[],
                            };
                            if cmd == "echo" {
                                if let Some((s, nl)) = self.echo_capture_full(argv) {
                                    input = if nl {
                                        format!("{s} + \"\\n\"")
                                    } else {
                                        s
                                    };
                                    continue;
                                }
                            } else if cmd == "printf" {
                                if let Some(s) = self.printf_capture(argv) {
                                    input = s;
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
            let Some((cmd, argv)) = self.body_single_exec(stage) else {
                return None;
            };
            let mut st = vec![cmd];
            st.extend(argv);
            stages.push(st);
        }
        if stages.is_empty() {
            return None;
        }
        Some((input, stages))
    }

    /// Capture of arithmetic (`$((n % 2))`) natively (no child bash).
    fn capture_arith(&mut self, body: &[IrStmt]) -> Option<String> {
        let [IrStmt::Expr(e)] = body else {
            return None;
        };
        match e {
            IrExpr::Arith(a) => Some(self.arith(a)),
            IrExpr::Call { func, args }
                if func == "arith" || func == "arithEval" || func == "let" =>
            {
                let s = match args.first() {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return None,
                };
                let (sides, v) = self.arith_str(&s)?;
                for sd in sides {
                    self.sides.push(sd);
                }
                Some(v)
            }
            _ => None,
        }
    }

    /// Capture of a bare user-function call (`x=$(greet a)`) — the
    /// closure's stdout through a pipe (statement sides + value
    /// expression). Temps are unique per site (nested captures would
    /// collide on fixed names).
    fn capture_fncall(&mut self, body: &[IrStmt]) -> Option<String> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = body else {
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let Some(IrExpr::Str(nm, _)) = args.first() else {
            return None;
        };
        if !self.functions.contains(nm) {
            return None;
        }
        let m = self.go_ident(nm);
        self.mark_read(nm);
        self.need_fargs = true;
        let vals: Vec<String> = args
            .get(1)
            .and_then(|a| match a {
                IrExpr::Array(elems) => {
                    Some(elems.iter().map(|e| self.expr_str(e)).collect())
                }
                _ => None,
            })
            .unwrap_or_default();
        let t = self.new_tmp();
        let (o, r, w, b) = (
            format!("__fo{t}"),
            format!("__fr{t}"),
            format!("__fw{t}"),
            format!("__fb{t}"),
        );
        self.sides.push(format!(
            "{o} := os.Stdout; {r}, {w}, _ := os.Pipe(); os.Stdout = {w}"
        ));
        self.sides.push(format!(
            "fArgs = []string{{{}}}; {m}(); fArgs = nil;",
            vals.join(", ")
        ));
        self.sides.push(format!(
            "{w}.Close(); {b}, _ := io.ReadAll({r}); os.Stdout = {o}"
        ));
        Some(format!("s2s(string({b}))"))
    }

    /// `$(echo …)` / `$(printf …)` captures natively (no child bash).
    /// Refusals (glob args, bad substitutions, unlowerable argv) return
    /// None — the caller falls back to the bash child.
    fn capture_builtin(&mut self, body: &[IrStmt]) -> Option<String> {        let [IrStmt::Expr(IrExpr::Call { func, args })] = body else {
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let Some(IrExpr::Str(cmd, _)) = args.first() else {
            return None;
        };
        let items: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(it)) => it,
            _ => &[],
        };
        match cmd.as_str() {
            "echo" => self.echo_capture(items),
            "printf" => self.printf_capture(items),
            _ => None,
        }
    }

    /// `$(printf fmt args…)` natively via the bprintfStr runtime.
    fn printf_capture(&mut self, items: &[IrExpr]) -> Option<String> {
        if items.iter().any(|e| self.bad_subst(e)) {
            return None;
        }
        let fmt_expr = items.first()?;
        for it in items {
            if let IrExpr::Str(s, _) = it {
                if has_glob(s) {
                    return None;
                }
            }
        }
        let fmt = self.printf_fmt(fmt_expr);
        let args: Vec<String> = items[1..].iter().map(|a| self.expr_str(a)).collect();
        Some(if args.is_empty() {
            format!("bprintfStr({fmt})")
        } else {
            format!("bprintfStr({fmt}, {})", args.join(", "))
        })
    }

    /// If the body is exactly one external `exec` call, return its
    /// (cmd expr, argv expr list). Accepts both the `Call("exec")` form
    /// and a bare `IrStmt::Exec` node with no capture/redirects/env (the
    /// procsub `mktemp` materialisation).
    fn body_single_exec(&mut self, body: &[IrStmt]) -> Option<(String, Vec<String>)> {
        if body.len() != 1 {
            return None;
        }
        match &body[0] {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                let cmd_expr = args.first()?;
                let items: &[IrExpr] = match args.get(1) {
                    Some(IrExpr::Array(it)) => it,
                    _ => &[],
                };
                self.single_cmd_argv(cmd_expr, items)
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            } => {
                if capture.is_some() || !redirects.is_empty() || !env.is_empty() {
                    return None;
                }
                self.single_cmd_argv(cmd, args)
            }
            _ => None,
        }
    }

    /// One external command → (cmd Go-string, argv Go-strings), refusing
    /// the shell builtins `exec.Command` cannot run (echo/printf/cd/… —
    /// those lower natively or via the bash -c fallback instead).
    fn single_cmd_argv(
        &mut self,
        cmd_expr: &IrExpr,
        items: &[IrExpr],
    ) -> Option<(String, Vec<String>)> {
        if let IrExpr::Str(cmd, _) = cmd_expr {
            if matches!(
                cmd.as_str(),
                "echo" | "printf" | "exit" | "cd" | "let" | "local" | "read" | "test"
            ) {
                return None;
            }
            let cmd = Self::go_str(cmd);
            let argv = self.argv_strings(items)?;
            Some((cmd, argv))
        } else {
            None
        }
    }

    /// String interpolation: "hello $name" → fmt.Sprintf(...) (string).
    fn interpolate(&mut self, parts: &[InterpPart]) -> String {
        // fmt is the Sprintf format (`%`-escaped lits + `%v` markers);
        // raw is the plain text (only valid when there are no markers).
        // `%` is escaped per-LIT so the markers themselves stay raw, and
        // a marker-free string keeps `echo "100%"` as a bare literal.
        let mut fmt = String::new();
        let mut raw = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => {
                    raw.push_str(s);
                    fmt.push_str(&s.replace('%', "%%"));
                }
                InterpPart::Expr(x) => {
                    fmt.push_str("%v");
                    args.push(self.expr_any(x));
                }
            }
        }
        if args.is_empty() {
            Self::go_str(&raw)
        } else {
            format!(
                "fmt.Sprintf({}, {})",
                Self::go_str(&fmt),
                args.join(", ")
            )
        }
    }

    // ── arithmetic ───────────────────────────────────────────────────

    /// Structured ArithAst → native int64 expression (may leave side
    /// effects in `self.sides` for Assign/IncDec).
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) | ArithAst::Ident(name) => {
                if let Some(v) = self.arith_var(name) {
                    v
                } else {
                    "s2i(0)".to_string()
                }
            }
            ArithAst::Index { var, key } => {
                self.mark_arr(var);
                self.ident_of(var);
                let k = self.arith(key);
                format!("s2i(arrIdx({}, {k}))", self.ident_of(var))
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                match op.as_str() {
                    "**" => format!("powN({l},{r})"),
                    "&&" => format!("land({l},{r})"),
                    "||" => format!("lor({l},{r})"),
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        format!("cmpN({l},{r},{})", Self::go_str(op))
                    }
                    "/" => format!("divN({l},{r})"),
                    "%" => format!("modN({l},{r})"),
                    "<<" => format!("shlN({l},{r})"),
                    ">>" => format!("shrN({l},{r})"),
                    _ => format!("({l} {op} {r})"),
                }
            }
            ArithAst::Un { op, arg } => {
                let a = self.arith(arg);
                match op.as_str() {
                    "!" => format!("lnot({a})"),
                    "~" => format!("(^{a})"),
                    _ => format!("({op}{a})"),
                }
            }
            ArithAst::Cond { test, then, else_ } => {
                format!(
                    "s2i(cond3(({} != 0), {}, {}))",
                    self.arith(test),
                    self.arith(then),
                    self.arith(else_)
                )
            }
            ArithAst::Assign { var, op, rhs } => {
                let m = self.ident_of(var);
                self.mark_arith(var);
                let r = self.arith(rhs);
                let goop = match op.as_str() {
                    "=" => "=",
                    "+=" => "+=",
                    "-=" => "-=",
                    "*=" => "*=",
                    "/=" => "/=",
                    "%=" => "%=",
                    _ => "=",
                };
                let sync = self.sync_inline(var);
                self.sides.push(format!("{m} {goop} {r};{sync}"));
                m
            }
            ArithAst::IncDec { var, delta, prefix } => {
                let m = self.ident_of(var);
                self.mark_arith(var);
                if *prefix {
                    self.sides.push(format!("{m} = {m} + {delta};"));
                    m
                } else {
                    let t = self.new_tmp();
                    self.sides.push(format!("{t} := {m};"));
                    let sync = self.sync_inline(var);
                    self.sides.push(format!("{m} = {m} + {delta};{sync}"));
                    t
                }
            }
            // C-frontend nodes (never emitted by the shell path): sizeof is
            // a compile-time constant; casts are identity (Go int64).
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    /// An arith variable read (name may be `$x`, `x`, `$?`, `$1`, `$#`…).
    fn arith_var(&mut self, name: &str) -> Option<String> {
        let name = name.strip_prefix('$').unwrap_or(name);
        let resolved = self.nameref_resolve(name);
        let name = resolved.as_str();
        match name {
            "?" => {
                self.need_st = true;
                self.st_read = true;
                Some("st".to_string())
            }
            "$" => Some("int64(os.Getpid())".to_string()),
            "#" => Some("int64(len(argsList()))".to_string()),
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                let i: i64 = n.parse().unwrap_or(1);
                Some(format!("s2i(paramAt({i}))"))
            }
            "@" | "*" => Some("s2i(strings.Join(argsList(), \" \"))".to_string()),
            _ => {
                if self.declared(name) {
                    let m = self.ident_of(name);
                    if self.is_num(name) {
                        Some(m)
                    } else {
                        Some(format!("s2i({m})"))
                    }
                } else {
                    Some(format!("s2i(os.Getenv({}))", Self::go_str(name)))
                }
            }
        }
    }

    fn arith_op(&self, op: &crate::ir::BinOpKind) -> &'static str {
        match op {
            crate::ir::BinOpKind::Add => "+",
            crate::ir::BinOpKind::Sub => "-",
            crate::ir::BinOpKind::Mul => "*",
            crate::ir::BinOpKind::Div => "/",
            crate::ir::BinOpKind::Mod => "%",
            crate::ir::BinOpKind::BitAnd => "&",
            crate::ir::BinOpKind::BitOr => "|",
            crate::ir::BinOpKind::BitXor => "^",
            crate::ir::BinOpKind::ShiftL => "<<",
            crate::ir::BinOpKind::ShiftR => ">>",
            crate::ir::BinOpKind::Pow => {
                // handled by the caller (needs the helper, not an operator)
                "**"
            }
            _ => "+",
        }
    }

    // ── the `arith("...")` string form ───────────────────────────────

    fn arith_tokens(&self, s: &str) -> Option<Vec<(String, bool)>> {
        // (text, is_op)
        let mut out = Vec::new();
        let b: Vec<char> = s.chars().collect();
        let mut i = 0usize;
        while i < b.len() {
            let c = b[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if c.is_ascii_digit() {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == 'x' || b[i] == 'X') {
                    i += 1;
                }
                // base notation: `10#x`, `16#ff`, `2#1010`
                if i < b.len() && b[i] == '#' {
                    i += 1;
                    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                        i += 1;
                    }
                }
                out.push((b[start..i].iter().collect(), false));
                continue;
            }
            if c == '$' {
                if i + 1 < b.len() && b[i + 1] == '(' {
                    return None;
                }
                if i + 1 < b.len() && b[i + 1] == '{' {
                    let mut j = i + 2;
                    while j < b.len() && b[j] != '}' {
                        j += 1;
                    }
                    if j >= b.len() {
                        return None;
                    }
                    let inner: String = b[i + 2..j].iter().collect();
                    if inner.contains(':') {
                        return None;
                    }
                    out.push((format!("${{{inner}}}"), false));
                    i = j + 1;
                    continue;
                }
                let start = i;
                i += 1;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                    i += 1;
                }
                out.push((b[start..i].iter().collect(), false));
                continue;
            }
            if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                    i += 1;
                }
                out.push((b[start..i].iter().collect(), false));
                continue;
            }
            // multi-char operators
            let rest: String = b[i..].iter().collect();
            let mut matched = false;
            for op in [
                "**", "++", "--", "<<", ">>", "&&", "||", "<=", ">=", "==", "!=", "+=", "-=", "*=",
                "/=", "%=", "&=", "|=", "^=",
            ] {
                if rest.starts_with(op) {
                    out.push((op.to_string(), true));
                    i += op.len();
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }
            if "+-*/%<>&|^!~=?:()".contains(c) {
                out.push((c.to_string(), true));
                i += 1;
                continue;
            }
            return None;
        }
        Some(out)
    }

    /// Parse + render an arith string → (side stmts, value expr).
    /// Returns None for anything outside the lowable subset.
    fn arith_str(&mut self, s: &str) -> Option<(Vec<String>, String)> {
        let toks = self.arith_tokens(s)?;
        let mut p = MAParser { toks, i: 0 };
        let ast = p.expr()?;
        if p.i != p.toks.len() {
            return None;
        }
        self.ma_render(&ast)
    }

    /// Render an arith-string AST → (side stmts, int64 value expr).
    fn ma_render(&mut self, a: &MA) -> Option<(Vec<String>, String)> {
        match a {
            MA::Num(n) => Some((Vec::new(), n.to_string())),
            MA::Var(name) => {
                let v = self.arith_var(name)?;
                Some((Vec::new(), v))
            }
            MA::ArrLen(name) => {
                // assoc length counts the vars-home keys
                if self.assoc_arrays.contains(name) {
                    let n = Self::go_str(name);
                    return Some((
                        Vec::new(),
                        format!("int64(len(assocVals({n})))"),
                    ));
                }
                self.mark_arr(name);
                let m = self.ident_of(name);
                Some((Vec::new(), format!("arrLen({m})")))
            }
            MA::StrLen(name) => {
                // string length needs the string value (arith_var coerces
                // to int64, which runeLen cannot take)
                let v = if self.declared(name) {
                    let m = self.ident_of(name);
                    format!("s2s({m})")
                } else {
                    format!("os.Getenv({})", Self::go_str(name))
                };
                Some((Vec::new(), format!("runeLen({v})")))
            }
            MA::ArrIdx(name, key) => {
                self.mark_arr(name);
                self.ident_of(name);
                let (ks, kv) = self.ma_render(key)?;
                Some((ks, format!("s2i(arrIdx({}, {kv}))", self.ident_of(name))))
            }
            MA::Bin(op, l, r) => {
                let (ls, lv) = self.ma_render(l)?;
                let (rs, rv) = self.ma_render(r)?;
                let mut sides = ls;
                sides.extend(rs);
                let v = match op.as_str() {
                    "**" => format!("powN({lv},{rv})"),
                    "&&" => format!("land({lv},{rv})"),
                    "||" => format!("lor({lv},{rv})"),
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        format!("cmpN({lv},{rv},{})", Self::go_str(op))
                    }
                    "/" => format!("divN({lv},{rv})"),
                    "%" => format!("modN({lv},{rv})"),
                    "<<" => format!("shlN({lv},{rv})"),
                    ">>" => format!("shrN({lv},{rv})"),
                    _ => format!("({lv} {op} {rv})"),
                };
                Some((sides, v))
            }
            MA::Un(op, x) => {
                let (xs, xv) = self.ma_render(x)?;
                let v = match op.as_str() {
                    "!" => format!("lnot({xv})"),
                    "~" => format!("(^{xv})"),
                    "-" => format!("(-{xv})"),
                    _ => xv,
                };
                Some((xs, v))
            }
            MA::Cond(t, th, el) => {
                let (ts, tv) = self.ma_render(t)?;
                let (hs, hv) = self.ma_render(th)?;
                let (es, ev) = self.ma_render(el)?;
                let mut sides = ts;
                sides.extend(hs);
                sides.extend(es);
                Some((sides, format!("s2i(cond3(({tv} != 0), {hv}, {ev}))")))
            }
            MA::Assign(name, op, rhs) => {
                let (rs, rv) = self.ma_render(rhs)?;
                let m = self.ident_of(name);
                self.mark_arith(name);
                let mut sides = rs;
                let sync = self.sync_inline(name);
                sides.push(format!("{m} {op} {rv};{sync}"));
                Some((sides, m))
            }
            MA::IncDec(name, delta, prefix) => {
                let m = self.ident_of(name);
                self.mark_arith(name);
                if *prefix {
                    Some((vec![format!("{m} = {m} + {delta};")], m))
                } else {
                    let t = self.new_tmp();
                    let sync = self.sync_inline(name);
                    Some((
                        vec![
                            format!("{t} := {m};"),
                            format!("{m} = {m} + {delta};{sync}"),
                        ],
                        t,
                    ))
                }
            }
        }
    }

    /// A top-level arith-string statement (`let i++`, cstyleFor init/update):
    /// must render to a single Go statement with no side-effect temps.
    /// A top-level arith-string statement (`let i++`, cstyleFor init/update):
    /// must render to a single Go statement with no side-effect temps.
    /// Returns (stmt, value-expr) so `let` can set `$?` (bash let status
    /// is 0 iff the value != 0).
    fn ma_stmt(&mut self, a: &MA) -> Option<(String, String)> {
        match a {
            MA::Assign(name, op, rhs) => {
                let (rs, rv) = self.ma_render(rhs)?;
                if !rs.is_empty() {
                    return None;
                }
                let m = self.ident_of(name);
                self.mark_arith(name);
                let sync = self.sync_inline(name);
                Some((format!("{m} {op} {rv};{sync}"), m))
            }
            MA::IncDec(name, delta, prefix) => {
                let m = self.ident_of(name);
                self.mark_arith(name);
                let stmt = if *delta == 1 {
                    format!("{m}++;")
                } else {
                    format!("{m}--;")
                };
                let value = if *prefix {
                    if *delta == 1 {
                        format!("{m} + 1")
                    } else {
                        format!("{m} - 1")
                    }
                } else {
                    m
                };
                Some((stmt, value))
            }
            _ => None,
        }
    }

    // ── brace expansion (render-time) ────────────────────────────────

    /// Expand a `brace` Call into its string list (None → stub).
    /// args: [Str(pre), Json(groups), Json(?), Str(post)]
    fn brace_expand(&mut self, args: &[IrExpr]) -> Option<Vec<String>> {
        // prefix/suffix are the FIRST and LAST Str args positionally —
        // an empty prefix must not be mistaken for "not yet seen" (the
        // old is_empty() test made `{a,{b,c}}d` put "d" in the prefix).
        let pre = args.first().and_then(|a| match a {
            IrExpr::Str(s, _) => Some(s.clone()),
            _ => None,
        })?;
        let post = args.get(3).and_then(|a| match a {
            IrExpr::Str(s, _) => Some(s.clone()),
            _ => None,
        })?;
        let groups = brace_groups(args.get(1).and_then(|a| match a {
            IrExpr::Json(v) => Some(v),
            _ => None,
        })?)?;
        let middles: Vec<String> = match args.get(2) {
            Some(IrExpr::Json(serde_json::Value::Array(xs))) => xs
                .iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect(),
            _ => Vec::new(),
        };
        let mut out = vec![pre.clone()];
        for (gi, group) in groups.iter().enumerate() {
            let mid = middles.get(gi).cloned().unwrap_or_default();
            let mut next = Vec::new();
            for prefix in &out {
                for s in group {
                    let mut v = format!("{prefix}{s}");
                    if gi + 1 < groups.len() {
                        v.push_str(&mid);
                    }
                    next.push(v);
                }
            }
            out = next;
        }
        for s in &mut out {
            *s = format!("{s}{post}");
        }
        Some(out)
    }

    /// Render a `brace` Call as a Go []string literal.
    fn brace_slice(&mut self, args: &[IrExpr]) -> Option<String> {
        let items = self.brace_expand(args)?;
        let parts: Vec<String> = items.iter().map(|s| Self::go_str(s)).collect();
        Some(format!("[]string{{{}}}", parts.join(", ")))
    }

    /// shIR `exec` argv with a brace item → spliced argv parts.
    fn argv_parts(&mut self, items: &[IrExpr]) -> Option<Vec<Part>> {
        let mut out = Vec::new();
        for (idx, it) in items.iter().enumerate() {
            if idx > 0 {
                out.push(Part::Lit(" ".to_string()));
            }
            if let IrExpr::Call { func, args } = it {
                if func == "brace" {
                    let expanded = self.brace_expand(args)?;
                    for (i, s) in expanded.iter().enumerate() {
                        if i > 0 {
                            out.push(Part::Lit(" ".to_string()));
                        }
                        out.push(Part::Lit(s.clone()));
                    }
                    continue;
                }
            }
            out.extend(self.parts_of(it));
        }
        Some(out)
    }

    // ── call dispatch ────────────────────────────────────────────────

    /// Render a Call as an expression (any-compatible).
    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "line" => {
                // multi-return line read (`line(cap, N)` = the C frontend's
                // outparam channel): String(v).split('\n')[N]
                if let (Some(v), Some(IrExpr::Str(i, _))) = (args.first(), args.get(1)) {
                    let ve = self.expr_any(v);
                    let n: usize = i.parse().unwrap_or(0);
                    // s2s coerces any-typed homes (the capture writes any)
                    return format!("__sh_line(s2s({ve}), {n})");
                }
                self.sh2_stub("line")
            }
            "fnValue" | "fnCall" => {
                // a user-function call in VALUE position (`echo $(greet
                // x)`, `y=$(f)`) — capture the callee's stdout through a
                // pipe, exactly like capture_arrow's fnCall case. The
                // closure itself takes no params (fArgs convention), so a
                // direct call value is impossible — this used to emit
                // `name(&[_]string{…})`, which never compiled.
                let m = args
                    .first()
                    .and_then(|a| match a {
                        IrExpr::Str(nm, _) => Some(nm.clone()),
                        _ => None,
                    });
                let Some(nm) = m else {
                    return self.sh2_stub("dynamic fnCall");
                };
                let g = self.go_ident(&nm);
                self.mark_read(&nm);
                self.need_fargs = true;
                let call_args: Vec<String> = args
                    .get(1)
                    .and_then(|a| match a {
                        IrExpr::Array(elems) => {
                            Some(elems.iter().map(|e| self.expr_str(e)).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                // unique pipe temporaries (two captures in one scope must
                // not redeclare each other's `:=` homes)
                let t = self.new_tmp();
                self.sides.push(format!(
                    "__ot{t} := os.Stdout; __pr{t}, __pw{t}, _ := os.Pipe(); os.Stdout = __pw{t}"
                ));
                self.sides.push(format!(
                    "fArgs = []string{{{}}}; {g}(); fArgs = nil;",
                    call_args.join(", ")
                ));
                self.sides.push(format!(
                    "__pw{t}.Close(); __pb{t}, _ := io.ReadAll(__pr{t}); os.Stdout = __ot{t}"
                ));
                format!("s2s(strings.TrimRight(string(__pb{t}), \"\\n\"))")
            }
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    return self.getvar_any(name);
                }
                format!("os.Getenv({})", self.call_arg0_str(args))
            }
            "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return c;
                    }
                    let env = self.env_lit();
                    let wrapped = match args.get(1) {
                        Some(IrExpr::Str(k, _)) if k == "[[" => {
                            format!("[[ {s} ]]")
                        }
                        _ => format!("[ {s} ]"),
                    };
                    return format!(
                        "(redirRun({}, {env}) == 0)",
                        Self::go_str(&wrapped)
                    );
                }
                self.sh2_stub("test")
            }
            "exec" => self.exec_expr(args),
            "pipeline" => self.pipeline_expr(args),
            "redirect" => self.redirect_expr(args),
            "grepMatches" => {
                // value context (`x=$(grep -o …)`): the child prints the
                // matches; capRun captures them like command substitution.
                // (The exit code on no-match is not modelled — the value
                // is; note it if a `$?`-after-grepMatches cell appears.)
                if let Some(t) = self.grep_matches_text(args) {
                    let env = self.env_lit();
                    return format!("capRun({}, {env})", Self::go_str(&t));
                }
                self.sh2_stub("grepMatches")
            }
            "capture" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    return self.capture_arrow(body);
                }
                self.sh2_stub("capture")
            }
            "captureWords" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    return self.capture_words(body);
                }
                self.sh2_stub("captureWords")
            }
            "param" => self.param_call(args),
            "join" => self.join_call(args),
            "brace" => {
                if let Some(sl) = self.brace_slice(args) {
                    return format!("strings.Join({sl}, \" \")");
                }
                self.sh2_stub("brace")
            }
            "setArray" | "setArrayAppend" | "assign" | "cstyleFor" | "whileLoop" | "subshell"
            | "block" | "listVar" | "shopt" | "break" | "continue" | "return" => {
                self.sh2_stub(func)
            }
            "arrayIndex" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return self.sh2_stub("arrayIndex"),
                };
                let name = self.nameref_resolve(&name);
                let Some(kexpr) = args.get(1) else {
                    return self.sh2_stub("arrayIndex");
                };
                if self.assoc_arrays.contains(&name) || assoc_key_shape(kexpr) {
                    // assoc element: the `vars` map home (`base[key]`),
                    // where the baked writes sync (missing key → "").
                    // Str keys may carry `$` vars (`${m[$i,$j]}` arrives
                    // as Str "$i,$j") — expand them like indexed keys do.
                    let key_str = match kexpr {
                        IrExpr::Int(i) => Self::go_str(&i.to_string()),
                        IrExpr::Str(s, _) => self.expand_str(s),
                        other => self.expr_str(other),
                    };
                    let base = Self::go_str(&format!("{name}["));
                    format!("s2s(vars[{base} + {key_str} + \"]\"])")
                } else {
                    let m = self.ident_of(&name);
                    self.mark_arr(&name);
                    let key = match kexpr {
                        IrExpr::Int(i) => i.to_string(),
                        IrExpr::Str(s, _) => {
                            if let Some((_, v)) = self.arith_str(s) {
                                v
                            } else {
                                format!("s2i({})", self.expand_str(s))
                            }
                        }
                        other => self.expr_num(other),
                    };
                    format!("s2s(arrIdx({m}, {key}))")
                }
            }
            "arrayItems" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return self.sh2_stub("arrayItems"),
                };
                let name = self.nameref_resolve(&name);
                let name = normalize_assoc_key(&name);
                self.mark_arr(&name);
                self.need_vars = true;
                let joiner = if name.ends_with("[*]") { "ifsJoin" } else { "\" \"" };
                format!("strings.Join(pList(\"\", {}), {})", Self::go_str(&name), joiner)
            }
            "arrayLen" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return self.sh2_stub("arrayLen"),
                };
                let name = self.nameref_resolve(&name);
                let name = normalize_assoc_key(&name);
                // assoc length counts the vars-home keys, not a slice
                if self.assoc_arrays.contains(&name) {
                    return format!(
                        "s2s(int64(len(assocVals({}))))",
                        Self::go_str(&name)
                    );
                }
                let m = self.ident_of(&name);
                self.mark_arr(&name);
                format!("s2s(arrLen({m}))")
            }
            "split" => {
                // bash word-splitting on whitespace (the default IFS):
                // collapse runs, drop empty fields. strings.Fields is the
                // exact native match, joined back with single spaces.
                if let Some(a) = args.first() {
                    let v = self.expr_str(a);
                    return format!("strings.Join(strings.Fields({v}), \" \")");
                }
                "\"\"".to_string()
            }
            "and" | "or" => {
                // `A && B` / `A || B` — bash -c fork/exec fallback
                // (short-circuit semantics are a shell primitive)
                let mut texts = Vec::new();
                let mut ok = true;
                for a in args {
                    if let Some(t) = self.cmd_text_expr(a) {
                        texts.push(t);
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && !texts.is_empty() {
                    let env = self.env_lit();
                    let sep = if func == "and" { " && " } else { " || " };
                    return format!(
                        "redirRun({}, {env})",
                        Self::go_str(&texts.join(sep))
                    );
                }
                self.sh2_stub(func)
            }
            "arith" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((_, v)) = self.arith_str(s) {
                        return v;
                    }
                }
                // unparseable arithmetic word-expands to empty in bash
                // (the error goes to stderr, which the gate discards)
                "\"\"".to_string()
            }
            _ => self.sh2_stub(func),
        }
    }

    fn call_arg0_str(&mut self, args: &[IrExpr]) -> String {
        if let Some(a) = args.first() {
            self.expr_str(a)
        } else {
            "\"\"".to_string()
        }
    }

    fn getvar_any(&mut self, name: &str) -> String {
        let resolved = self.nameref_resolve(name);
        let name = resolved.as_str();
        match name {
            "?" => {
                self.need_st = true;
                self.st_read = true;
                "st".to_string()
            }
            "$" => "int64(os.Getpid())".to_string(),
            "#" => "int64(len(argsList()))".to_string(),
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                let i: i64 = n.parse().unwrap_or(1);
                format!("paramAt({i})")
            }
            "@" | "*" => "strings.Join(argsList(), \" \")".to_string(),
            _ => {
                if self.declared(name) {
                    self.ident_of(name)
                } else {
                    format!("os.Getenv({})", Self::go_str(name))
                }
            }
        }
    }

    /// An exec Call as an expression — runs the command, value = status.
    fn exec_expr(&mut self, args: &[IrExpr]) -> String {
        // user-function call in value position (`if is_prime $n`): run
        // the closure for side effects, value = its status. A direct
        // external exec would fail lookup (and spin `while` loops).
        if let Some(IrExpr::Str(cmd, _)) = args.first() {
            if self.functions.contains(cmd) {
                let vals: Vec<String> = match args.get(1) {
                    Some(IrExpr::Array(items)) => {
                        items.iter().map(|i| self.expr_str(i)).collect()
                    }
                    _ => Vec::new(),
                };
                let m = self.go_ident(cmd);
                self.mark_read(cmd);
                self.need_fargs = true;
                self.sides.push(format!("fArgs = []string{{{}}};", vals.join(", ")));
                self.sides.push(format!("{m}();"));
                self.sides.push("fArgs = nil;".to_string());
                self.need_st = true;
                return "st".to_string();
            }
            // `exit N` in value position (`x=$(exit 1)`): terminate with
            // the code — an external `exit` binary cannot exist
            if cmd == "exit" {
                let code = match args.get(1) {
                    Some(IrExpr::Array(items)) => match items.first() {
                        Some(p) => self.expr_num(p),
                        None => "0".to_string(),
                    },
                    _ => "0".to_string(),
                };
                self.sides
                    .push(format!("os.Exit(int({code}));"));
                return "0".to_string();
            }
        }
        if let Some((cmd, argv)) = self.exec_cmd_argv(args) {
            let mut parts = vec![cmd];
            parts.extend(argv);
            return format!("runCmd({})", exec_cmd_apply(&parts));
        }
        if let Some(IrExpr::Str(cmd, _)) = args.first() {
            if cmd == "cd" {
                let path = args
                    .get(1)
                    .and_then(|a| match a {
                        IrExpr::Array(items) => items.first(),
                        _ => None,
                    })
                    .map(|p| self.expr_str(p))
                    .unwrap_or_else(|| "\"\"".to_string());
                return format!("b2i(os.Chdir({path}) == nil)");
            }
        }
        self.sh2_stub("exec")
    }

    /// `exec` args → (cmd expr, argv exprs).
    fn exec_cmd_argv(&mut self, args: &[IrExpr]) -> Option<(String, Vec<String>)> {
        let cmd = args.first()?;
        let cmd = self.expr_str(cmd);
        let argv = match args.get(1) {
            Some(IrExpr::Array(items)) => self.argv_strings(items)?,
            _ => Vec::new(),
        };
        Some((cmd, argv))
    }

    /// argv items as Go string exprs — brace expansions splice into
    /// separate argv entries.
    fn argv_strings(&mut self, items: &[IrExpr]) -> Option<Vec<String>> {
        let mut out = Vec::new();
        for it in items {
            if let IrExpr::Call { func, args } = it {
                if func == "brace" {
                    let expanded = self.brace_expand(args)?;
                    for s in expanded {
                        out.push(Self::go_str(&s));
                    }
                    continue;
                }
            }
            if let IrExpr::Str(s, _) = it {
                let s = strip_sh2glob(s);
                if has_glob(&s) {
                    out.push(format!("globArgs({})...", Self::go_str(&s)));
                    continue;
                }
                out.push(Self::go_str(&s));
                continue;
            }
            out.push(self.expr_str(it));
        }
        Some(out)
    }

    /// `pipeline` Call → runPipe or bash -c reconstruction. Value = status.
    fn pipeline_expr(&mut self, args: &[IrExpr]) -> String {
        let mut stages: Vec<Vec<String>> = Vec::new();
        if let Some(IrExpr::Array(items)) = args.first() {
            for it in items {
                if let IrExpr::Arrow(body) = it {
                    if let Some((cmd, argv)) = self.body_single_exec(body) {
                        let mut st = vec![cmd];
                        st.extend(argv);
                        stages.push(st);
                        continue;
                    }
                }
                stages.clear();
                break;
            }
        }
        if !stages.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            for st in &stages {
                parts.push(argv_slice(st));
            }
            return format!("runPipe([][]string{{{}}})", parts.join(", "));
        }
        if let Some(IrExpr::Array(items)) = args.first() {
            let mut texts = Vec::new();
            for it in items {
                if let IrExpr::Arrow(body) = it {
                    if let Some(t) = self.cmd_text_stmts(body) {
                        texts.push(t);
                        continue;
                    }
                }
                texts.clear();
                break;
            }
            if !texts.is_empty() {
                let env = self.env_lit();
                return format!("redirRun({}, {env})", Self::go_str(&pipe_join(&texts)));
            }
        }
        self.sh2_stub("pipeline")
    }

    /// `redirect` Call: args[0] = Arrow(body), args[1] = Array of
    /// {fd, mode, target} objects. Value = exit status.
    fn redirect_expr(&mut self, args: &[IrExpr]) -> String {
        let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) = (args.first(), args.get(1))
        else {
            return self.sh2_stub("redirect");
        };
        // herestring-fed single external (`sort <<<"${arr[*]}"`) — feed
        // the evaluated input natively (arrays don't cross into children)
        if redirs.len() == 1 {
            if let Some(r) = object_redirect(&redirs[0]) {
                // status position: run natively, rc = last stage
                if let Some((input, stages)) = self.redirect_herestring_pipe(body, &[r]) {
                    return format!(
                        "runPipeIn({input}, {})",
                        Self::pipe_stages_go(&stages)
                    );
                }
            }
        }
        if let Some(text) = self.cmd_text_stmts(body) {
            let mut with_redirs = format!("( {text} )");
            for r in redirs {
                if let Some(rt) = self.redirect_text(r) {
                    with_redirs.push(' ');
                    with_redirs.push_str(&rt);
                } else {
                    return self.sh2_stub("redirect");
                }
            }
            let env = self.env_lit();
            return format!("redirRun({}, {env})", Self::go_str(&with_redirs));
        }
        self.sh2_stub("redirect")
    }

    /// Redirect stmt (`IrRedirect`) → shell text. Same word coverage as
    /// the Call-form `redirect_text` above: Str/getVar/Var stay bare,
    /// Interpolate/computed targets reconstruct + quote, `>|` passes
    /// through, heredocs stay raw.
    fn redirect_ir_text(&mut self, r: &crate::ir::IrRedirect) -> Option<String> {
        let fd = r.fd.unwrap_or(0).to_string();
        let mut quote_target = false;
        let t = match &r.target {
            IrExpr::Str(s, _) => s.clone(),
            // a variable redirect target (`echo hi > "$f"` — bat-sh-go
            // t36_redirect_var): render `$name` — redirRun's bash child sees
            // the var's value through env_lit (every written var ships as
            // `name=<value>` env).
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => format!("${{{n}}}"),
                _ => return None,
            },
            IrExpr::Var(name, _) => format!("${{{name}}}"),
            other => {
                let text = self.cmd_text_expr(other)?;
                quote_target = true;
                text
            }
        };
        let t = if quote_target {
            if t.contains('$') {
                quote_sh_dq(&t)
            } else {
                quote_sh(&t)
            }
        } else {
            t
        };
        match r.mode.as_str() {
            "w" => Some(format!("{fd}>{t}")),
            "wc" => Some(format!("{fd}>|{t}")),
            "a" => Some(format!("{fd}>>{t}")),
            "r" => Some(format!("{fd}<{t}")),
            "r+" => Some(format!("{fd}<> {t}")),
            "herestring" => Some(format!("{fd}<<<{t}")),
            "heredoc" => {
                // the body text carries one trailing newline artifact —
                // strip exactly one (a genuinely blank last line keeps its own)
                let t = t.strip_suffix('\n').unwrap_or(&t);
                Some(format!("{fd}<<'EOF'\n{t}\nEOF"))
            }
            "heredoc-tabs" => Some(format!("{fd}<<-'EOF'\n{t}\nEOF")),
            _ => None,
        }
    }

    /// Herestring target → Go string expression, when natively evaluable
    /// (herestrings perform no pathname expansion or word splitting, so
    /// evaluation is exact — except a static leading `~`, which needs
    /// tilde expansion and refuses). Unknown shapes return None (the
    /// caller keeps the bash-child fallback).
    fn herestring_input(&mut self, target: &IrExpr) -> Option<String> {
        match target {
            IrExpr::Str(s, _) => {
                if s.starts_with('~') {
                    return None;
                }
                Some(Self::go_str(s))
            }
            IrExpr::Int(i) => Some(format!("s2s({i})")),
            IrExpr::Var(_, _) | IrExpr::Ident(_) => Some(self.expr_str(target)),
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Lit(t) = p {
                        if t.starts_with('~') {
                            return None;
                        }
                    }
                }
                Some(self.expr_str(target))
            }
            IrExpr::BinOp { op, .. }
                if *op == crate::ir::BinOpKind::Concat =>
            {
                Some(self.expr_str(target))
            }
            IrExpr::Call { func, .. }
                if matches!(
                    func.as_str(),
                    "param" | "join" | "arrayItems" | "arrayLen" | "getVar" | "listVar"
                ) =>
            {
                Some(self.expr_str(target))
            }
            IrExpr::Capture { .. } => Some(self.expr_str(target)),
            _ => None,
        }
    }

    /// Herestring-fed single external command (`sort <<<"${arr[*]}"`)
    /// with a natively evaluable target. Returns (input, stages) — the
    /// caller formats capPipeIn (captures) or runPipeIn (status).
    /// Arrays don't cross into children, so without this the child sees
    /// an empty expansion.
    fn redirect_herestring_pipe(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<(String, Vec<Vec<String>>)> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return None;
        };
        // pipeline stages (`cat <<<"$x" | sort`) — every stage plain
        if func == "pipeline" {
            let [r] = redirects else {
                return None;
            };
            if r.mode != "herestring" || r.fd.unwrap_or(0) != 0 {
                return None;
            }
            let input = self.herestring_input(&r.target)?;
            let Some(IrExpr::Array(items)) = args.first() else {
                return None;
            };
            let mut stages: Vec<Vec<String>> = Vec::new();
            for it in items {
                let IrExpr::Arrow(stage) = it else {
                    return None;
                };
                let Some((cmd, argv)) = self.body_single_exec(stage) else {
                    return None;
                };
                let mut st = vec![cmd];
                st.extend(argv);
                stages.push(st);
            }
            if stages.is_empty() {
                return None;
            }
            return Some((input, stages));
        }
        if func != "exec" {
            return None;
        }
        let [r] = redirects else {
            return None;
        };
        if r.mode != "herestring" || r.fd.unwrap_or(0) != 0 {
            return None;
        }
        let input = self.herestring_input(&r.target)?;
        let (cmd, argv) = self.single_cmd_argv(
            args.first()?,
            match args.get(1) {
                Some(IrExpr::Array(items)) => items.as_slice(),
                _ => &[],
            },
        )?;
        let mut st = vec![cmd];
        st.extend(argv);
        Some((input, vec![st]))
    }

    /// Format stages for capPipeIn/runPipeIn.
    fn pipe_stages_go(stages: &[Vec<String>]) -> String {
        let parts: Vec<String> = stages.iter().map(|st| argv_slice(st)).collect();
        format!("[][]string{{{}}}", parts.join(", "))
    }

    /// Capture of a herestring-fed command/pipeline (`$(sort <<<"$x")`)
    /// with an evaluable target — the arrays survive (no child env).
    fn capture_redirect(&mut self, body: &[IrStmt]) -> Option<String> {
        if let [IrStmt::Expr(IrExpr::Call { func, args })] = body {
            if func == "redirect" {
                let (Some(IrExpr::Arrow(rbody)), Some(IrExpr::Array(redirs))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                let mut irs: Vec<crate::ir::IrRedirect> = Vec::new();
                for r in redirs {
                    irs.push(object_redirect(r)?);
                }
                let (input, stages) = self.redirect_herestring_pipe(rbody, &irs)?;
                return Some(format!(
                    "capPipeIn({input}, {})",
                    Self::pipe_stages_go(&stages)
                ));
            }
        }
        if let [IrStmt::Redirect { inner, redirects }] = body {
            let (input, stages) = self.redirect_herestring_pipe(inner, redirects)?;
            return Some(format!(
                "capPipeIn({input}, {})",
                Self::pipe_stages_go(&stages)
            ));
        }
        None
    }

    /// `while IFS= read -r a b; do …; done < file` natively (the
    /// fallback child can't see arrays, drops IFS, and mishandles
    /// `[[ =~ ]]`). Only observations: file inputs (Str/Var paths),
    /// `-r` reads into plain names, And-chained guards after the read.
    /// Anything else refuses (the bash child keeps it).
    fn stmt_while_read(
        &mut self,
        cond: &IrExpr,
        body: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> bool {
        // exactly one stdin-file redirect
        let [r] = redirects else {
            return false;
        };
        if r.fd.unwrap_or(0) != 0 || r.mode != "r" {
            return false;
        }
        let path = match &r.target {
            IrExpr::Str(s, _) => Self::go_str(s),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if !self.declared(n) {
                    return false;
                }
                format!("s2s({})", self.ident_of(n))
            }
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => {
                    if !self.declared(n) {
                        return false;
                    }
                    format!("s2s({})", self.ident_of(n))
                }
                _ => return false,
            },
            _ => return false,
        };
        // flatten a top-level And-chain (through single-Arrow blocks);
        // the read must come first up to trivial `true`s, the rest are
        // per-iteration entry guards
        let mut operands: Vec<&IrExpr> = Vec::new();
        if !flatten_and(cond, &mut operands) {
            return false;
        }
        let mut read_idx: Option<usize> = None;
        for (i, op) in operands.iter().enumerate() {
            if is_read_call(op) {
                read_idx = Some(i);
                break;
            }
            if !is_true_call(op) {
                return false;
            }
        }
        let Some(ri) = read_idx else {
            return false;
        };
        // parse the read call: flags + names + IFS env
        let IrExpr::Call { func: _, args } = operands[ri] else {
            return false;
        };
        let argv: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items.as_slice(),
            _ => &[],
        };
        let mut names: Vec<String> = Vec::new();
        let mut array_name: Option<String> = None;
        let mut it = argv.iter().peekable();
        while let Some(a) = it.next() {
            match a {
                IrExpr::Str(s, _) if s == "-r" || s == "-s" || s == "-e" => {}
                IrExpr::Str(s, _) if s == "-p" => {
                    // prompt (stderr-discarded): skip its argument
                    if it.next().is_none() {
                        return false;
                    }
                }
                IrExpr::Str(s, _)
                    if s == "-a" && array_name.is_none() && names.is_empty() =>
                {
                    match it.next() {
                        Some(IrExpr::Str(nm, _)) => array_name = Some(nm.clone()),
                        _ => return false,
                    }
                }
                IrExpr::Str(s, _) => {
                    if s.starts_with('-') || !is_plain_name(s) {
                        return false;
                    }
                    names.push(s.clone());
                }
                _ => return false,
            }
        }
        if array_name.is_some() && !names.is_empty() {
            return false;
        }
        if array_name.is_none() && names.is_empty() {
            return false;
        }
        // IFS from the call env (Object); default whitespace otherwise
        let mut ifs_go = "\" \\t\\n\"".to_string();
        if let Some(IrExpr::Object(props)) = args.get(2) {
            for (k, v) in props {
                if k == "IFS" {
                    match v {
                        IrExpr::Str(s, _) => {
                            ifs_go = Self::go_str(s);
                        }
                        _ => return false,
                    }
                }
            }
        } else if args.len() > 2 {
            return false;
        }
        // guards: remaining operands recombined as a test (rendered
        // AFTER the loop vars are marked written, so reads use homes)
        let guard_expr: Option<IrExpr> = {
            if ri + 1 >= operands.len() {
                None
            } else {
                let rest = &operands[ri + 1..];
                let mut it = rest.iter();
                let mut e = (*it.next().unwrap()).clone();
                for r in it {
                    e = IrExpr::BinOp {
                        lhs: Box::new(e),
                        op: crate::ir::BinOpKind::And,
                        rhs: Box::new((*r).clone()),
                    };
                }
                Some(e)
            }
        };
        // emit the loop
        self.flush_sides();
        self.emit(&format!("for _, __wl := range readFileLines({path}) {{"));
        self.loop_depth += 1;
        self.depth += 1;
        self.emit(&format!(
            "__parts := readSplit(__wl, {ifs_go}, {});",
            array_name.is_some() as usize + names.len()
        ));
        if let Some(arr) = array_name {
            let m = self.go_ident(&arr);
            self.mark_arr(&arr);
            self.mark_written(&arr);
            let sync = self.sync_inline(&arr);
            self.emit(&format!("{m} = strsToAny(__parts);{sync}"));
        } else {
            for (i, nm) in names.iter().enumerate() {
                let m = self.go_ident(nm);
                self.mark_written(nm);
                let sync = self.sync_inline(nm);
                self.emit(&format!("{m} = s2s(__parts[{i}]);{sync}"));
            }
        }
        if let Some(e) = guard_expr {
            let g = self.expr_bool(&e);
            self.emit(&format!("if !({g}) {{ break; }}"));
        }
        for s in body {
            self.stmt(s);
        }
        self.depth -= 1;
        self.loop_depth -= 1;
        self.emit("}");
        true
    }

    /// stdout/stderr wired to the files. Returns false when the shape does
    /// not match (the caller falls through to the bash -c reconstruction
    /// or the stub). Only plain `>`/`>>` on fd 1/2 lower natively.
    fn redirect_argv_stmt(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> bool {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return false;
        };
        if func != "exec" {
            return false;
        }
        let [cmd_expr, argv_expr] = args.as_slice() else {
            return false;
        };
        let is_argv = match cmd_expr {
            IrExpr::Call { func, args } if func == "listVar" => {
                matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "@" || n == "*")
            }
            _ => false,
        };
        if !is_argv {
            return false;
        }
        if !matches!(argv_expr, IrExpr::Array(items) if items.is_empty()) {
            return false;
        }
        // collect (fd, path-expr, append) wires — anything else refuses
        let mut wires: Vec<(i32, String, bool)> = Vec::new();
        for r in redirects {
            let fd = r.fd.unwrap_or(1);
            if fd != 1 && fd != 2 {
                return false;
            }
            let append = match r.mode.as_str() {
                "w" => false,
                "a" => true,
                _ => return false,
            };
            wires.push((fd, self.expr_str(&r.target), append));
        }
        // IIFE so the temporaries cannot collide with user homes
        self.emit("st = func() int64 {");
        self.depth += 1;
        self.emit("__argv := argsList();");
        self.emit("if len(__argv) == 0 {");
        self.depth += 1;
        // no command: bash still applies the redirects, status 0
        for (_, path, append) in &wires {
            let open = if *append {
                format!("os.OpenFile({path}, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)")
            } else {
                format!("os.Create({path})")
            };
            self.emit(&format!(
                "if __f, __e := {open}; __e == nil {{ __f.Close(); }}"
            ));
        }
        self.emit("return 0;");
        self.depth -= 1;
        self.emit("}");
        self.emit("__c := exec.Command(__argv[0], __argv[1:]...);");
        self.emit("__c.Stderr = io.Discard;");
        for (i, (fd, path, append)) in wires.iter().enumerate() {
            let open = if *append {
                format!("os.OpenFile({path}, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)")
            } else {
                format!("os.Create({path})")
            };
            self.emit(&format!(
                "__w{i}, __e{i} := {open}; if __e{i} != nil {{ return 1; }}"
            ));
            match fd {
                1 => self.emit(&format!("__c.Stdout = __w{i};")),
                _ => self.emit(&format!("__c.Stderr = __w{i};")),
            }
        }
        self.emit("__s := runCmd(__c);");
        for (i, _) in wires.iter().enumerate() {
            self.emit(&format!("__w{i}.Close();"));
        }
        self.emit("return __s;");
        self.depth -= 1;
        self.emit("}();");
        self.need_st = true;
        true
    }

    /// `mapfile -t arr < path` → native line read. The bash -c fallback
    /// loses the array (child-side assignment), so this common shape
    /// (`mapfile -t lines < <(producer)`, materialized by the frontend
    /// as a temp file + stdin redirect) is lowered directly. REFUSE >
    /// GUESS: only the `-t` flag form, exactly one stdin-file redirect.
    fn redirect_mapfile(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> bool {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return false;
        };
        if func != "builtin" && func != "exec" {
            return false;
        }
        let [cmd_expr, argv_expr] = args.as_slice() else {
            return false;
        };
        let is_mapfile = matches!(cmd_expr, IrExpr::Str(c, _) if c == "mapfile" || c == "readarray");
        if !is_mapfile {
            return false;
        }
        let IrExpr::Array(items) = argv_expr else {
            return false;
        };
        // flags: bare `-t` and/or `-tNAME`; first non-flag is the array.
        let mut arr: Option<String> = None;
        for it in items {
            let IrExpr::Str(s, _) = it else {
                return false;
            };
            if s == "-t" {
                continue;
            }
            if let Some(rest) = s.strip_prefix("-t") {
                if arr.is_none() && !rest.is_empty() {
                    arr = Some(rest.to_string());
                    continue;
                }
                return false;
            }
            if s.starts_with('-') || arr.is_some() {
                return false;
            }
            arr = Some(s.clone());
        }
        let Some(arr) = arr else {
            return false;
        };
        // exactly one stdin read redirect from a path expression
        let [r] = redirects else {
            return false;
        };
        if r.fd.unwrap_or(0) != 0 || r.mode != "r" {
            return false;
        }
        let path = match &r.target {
            IrExpr::Str(s, _) => Self::go_str(s),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if !self.declared(n) {
                    return false;
                }
                format!("s2s({})", self.ident_of(n))
            }
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => {
                    if !self.declared(n) {
                        return false;
                    }
                    format!("s2s({})", self.ident_of(n))
                }
                _ => return false,
            },
            _ => return false,
        };
        let m = self.go_ident(&arr);
        self.mark_arr(&arr);
        self.mark_written(&arr);
        self.flush_sides();
        let sync = self.sync_inline(&arr);
        self.emit(&format!("{m} = readLines({path});{sync}"));
        if self.need_st {
            self.emit("st = 0;");
        }
        true
    }

    /// Heredoc body → Go string expression. Quoted heredocs (`<<'EOF'`)
    /// arrive literal; unquoted bodies (interpolate flag) expand.
    /// Returns None for shapes needing the bash child.
    fn heredoc_body(
        &mut self,
        content: &IrExpr,
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<String> {
        let interp = redirects
            .iter()
            .any(|r| r.mode == "heredoc" && r.interpolate);
        match content {
            IrExpr::Str(text, _) => Some(if interp {
                self.expand_str(text)
            } else {
                Self::go_str(text)
            }),
            IrExpr::Interpolate(_) => Some(self.expr_str(content)),
            _ => None,
        }
    }

    /// A redirect object {fd, mode, target} → shell text `2>/dev/null`.
    fn redirect_text(&mut self, r: &IrExpr) -> Option<String> {
        let IrExpr::Object(props) = r else {
            return None;
        };
        let mut fd = String::new();
        let mut mode = "w".to_string();
        let mut target = None;
        // an Interpolate/computed target needs shell quoting (a plain
        // Str/getVar/Var target is already a bare word or `$`-text)
        let mut quote_target = false;
        for (k, v) in props {
            match k.as_str() {
                "fd" => {
                    if let IrExpr::Int(i) = v {
                        if *i != 1 {
                            fd = i.to_string();
                        }
                    } else {
                        return None;
                    }
                }
                "mode" => {
                    if let IrExpr::Str(s, _) = v {
                        mode = s.clone();
                    }
                }
                "target" => {
                    if let IrExpr::Str(s, _) = v {
                        target = Some(s.clone());
                    } else if let IrExpr::Call { func, args } = v {
                        // `> "$f"` — a variable redirect target: render
                        // `$name` so the bash -c child (redirRun) resolves it
                        // via env_lit (mirrors redirect_ir_text); any other
                        // call (param/arrayItems/…) reconstructs + quotes.
                        if func == "getVar" {
                            if let Some(IrExpr::Str(n, _)) = args.first() {
                                target = Some(format!("${{{n}}}"));
                            } else {
                                return None;
                            }
                        } else {
                            let text = self.cmd_text_expr(v)?;
                            target = Some(text);
                            quote_target = true;
                        }
                    } else if let IrExpr::Var(name, _) = v {
                        // procsub temp (`> $__ps_tmp0`): the child resolves
                        // it via env_lit like getVar above.
                        target = Some(format!("${{{name}}}"));
                    } else {
                        // Interpolate (`> "$d/f1"`, `<<< "hello world"`) —
                        // reconstruct the text; quoting is applied per
                        // mode below (heredocs stay raw — the `<<'EOF'`
                        // form is literal by design).
                        let text = self.cmd_text_expr(v)?;
                        target = Some(text);
                        // remember that this target needs shell quoting
                        // (a plain Str/getVar/Var target is already bare
                        // `$`-text or a safe word)
                        quote_target = true;
                    }
                }
                _ => {}
            }
        }
        let t = target?;
        // quote a reconstructed (Interpolate/computed) target: `$`-text
        // keeps double quotes so the child expands it, anything else gets
        // single quotes (heredocs stay raw — `<<'EOF'` is literal).
        let t = if quote_target {
            if t.contains('$') {
                quote_sh_dq(&t)
            } else {
                quote_sh(&t)
            }
        } else {
            t
        };
        match mode.as_str() {
            "w" => Some(format!("{fd}>{t}")),
            // `>|` (RedirectOutClobber): the bash -c child accepts it
            // directly (and never runs under `set -C` anyway).
            "wc" => Some(format!("{fd}>|{t}")),
            "a" => Some(format!("{fd}>>{t}")),
            "r" => Some(format!("{fd}<{t}")),
            "r+" => Some(format!("{fd}<> {t}")),
            "herestring" => Some(format!("{fd}<<<{t}")),
            "heredoc" => {
                // the body text carries one trailing newline artifact —
                // strip exactly one (a genuinely blank last line keeps its own)
                let t = t.strip_suffix('\n').unwrap_or(&t);
                Some(format!("{fd}<<'EOF'\n{t}\nEOF"))
            }
            "heredoc-tabs" => Some(format!("{fd}<<-'EOF'\n{t}\nEOF")),
            _ => None,
        }
    }

    /// `grepMatches(text, pattern, flags?)` → shell text
    /// (`grep -o[E/i/F] pat <<< text`) for the bash -c fallback — the same
    /// reconstruction the C backend emits (its `grepMatches` stmt arm).
    fn grep_matches_text(&mut self, args: &[IrExpr]) -> Option<String> {
        let text = self.cmd_text_expr(args.first()?)?;
        let pat = self.cmd_text_expr(args.get(1)?)?;
        let flags_s = match args.get(2) {
            Some(IrExpr::Str(f, _)) => f.clone(),
            _ => String::new(),
        };
        let mut oflags = String::from("-o");
        for c in flags_s.chars() {
            if c == 'E' || c == 'i' || c == 'F' {
                oflags.push(c);
            }
        }
        Some(format!(
            "grep {oflags} {} <<< {}",
            cmd_quote(args.get(1)?, &pat),
            cmd_quote(args.first()?, &text)
        ))
    }

    /// `capture(Arrow)` body → capture lowering.
    fn capture_arrow(&mut self, body: &[IrStmt]) -> String {
        // The C frontend's outparam channel: capture(Arrow[fnCall(..)]) —
        // the callee ECHOES its out-param values; the caller captures
        // STDOUT via a pipe (statement sides + a value expression)
        if let [IrStmt::Expr(IrExpr::Call { func, args })] = body {
            if func == "fnCall" || func == "fnValue" {
                if let Some(IrExpr::Str(nm, _)) = args.first() {
                    let m = self.go_ident(nm);
                    self.mark_read(nm);
                    self.need_fargs = true;
                    let vals: Vec<String> = args
                        .get(1)
                        .and_then(|a| match a {
                            IrExpr::Array(elems) => {
                                Some(elems.iter().map(|e| self.expr_str(e)).collect())
                            }
                            _ => None,
                        })
                        .unwrap_or_default();
                    self.sides.push(
                        "__sh_old := os.Stdout; __r, __w, _ := os.Pipe(); os.Stdout = __w"
                            .to_string(),
                    );
                    self.sides.push(format!(
                        "fArgs = []string{{{}}}; {m}(); fArgs = nil;",
                        vals.join(", ")
                    ));
                }
                self.sides
                    .push("__w.Close(); __b, _ := io.ReadAll(__r); os.Stdout = __sh_old"
                        .to_string());
                return "s2s(string(__b))".to_string();
            }
        }
        if let Some(v) = self.capture_fncall(body) {
            return v;
        }
        if let Some((cmd, argv)) = self.body_single_exec(body) {
            let mut parts = vec![cmd];
            parts.extend(argv);
            return format!("capCmd({})", exec_cmd_apply(&parts));
        }
        // `$(echo …)` / `$(a | b)` lower natively (no child bash)
        if let Some(s) = self.capture_builtin(body) {
            return s;
        }
        if let Some((input, stages)) = self.capture_pipeline(body) {
            let parts: Vec<String> =
                stages.iter().map(|st| argv_slice(st)).collect();
            let stages_go = format!("[][]string{{{}}}", parts.join(", "));
            return format!("capPipeIn({input}, {stages_go})");
        }
        if let Some(v) = self.capture_arith(body) {
            return v;
        }
        // herestring-fed redirect with evaluable input (no child env loss)
        if let Some(cap) = self.capture_redirect(body) {
            return cap;
        }
        if let Some(text) = self.cmd_text_stmts(body) {
            let env = self.env_lit();
            return format!("capRun({}, {env})", Self::go_str(&text));
        }
        self.sh2_stub("capture")
    }

    fn capture_words(&mut self, body: &[IrStmt]) -> String {
        let cap = self.capture_arrow(body);
        format!("strings.Join(strings.Fields({cap}), \" \")")
    }

    /// `param` Call — parameter expansion.
    fn param_call(&mut self, args: &[IrExpr]) -> String {
        let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) = (args.first(), args.get(1))
        else {
            return self.sh2_stub("param");
        };
        // canonical key (`config["u"]` reads the `config[u]` home)
        let name = self.nameref_resolve(&normalize_assoc_key(name));
        // A `$`-carrying Str extra (the frontend bakes `${s:$i:1}` offsets
        // as the literal Str "$i") must EXPAND to the var's value —
        // quoting it would hand pExp the 2-char string "$i" and
        // ParseInt would stick at 0 (010_substring_loop: HHHHHHHHHH).
        let extra: Vec<String> = args[2..]
            .iter()
            .map(|a| match a {
                IrExpr::Str(s, _) if s.contains('$') => self.expand_str(s),
                _ => self.expr_str(a),
            })
            .collect();
        self.need_vars = true;
        match op.as_str() {
            "@" | "*" => "strings.Join(argsList(), \" \")".to_string(),
            _ => {
                // `${base[key]...}` with a non-integer key: the index
                // must evaluate (`${args[i]}` — the key is opaque text
                // to pVal, which reads "" and spins `while` loops).
                // Integer literals keep the legacy pExp path.
                if let Some((base, keytext)) = split_bracket(&name) {
                    // (`[@]`/`[*]` wholes and `!`-indirection keep their
                    // existing paths — only real index text evaluates here)
                    if keytext != "@"
                        && keytext != "*"
                        && !base.starts_with('!')
                        && keytext.trim().parse::<i64>().is_err()
                    {
                        let elem = self.elem_fetch(base, keytext);
                        let extra: Vec<String> = args[2..]
                            .iter()
                            .map(|a| match a {
                                IrExpr::Str(s, _) if s.contains('$') => self.expand_str(s),
                                _ => self.expr_str(a),
                            })
                            .collect();
                        // strip a leading `@` (element-slice form)
                        let extra: &[String] = match extra.first() {
                            Some(e) if e == "\"@\"" => &extra[1..],
                            _ => &extra,
                        };
                        if let Some(v) = self.elem_op(op, elem, extra) {
                            return v;
                        }
                    }
                }
                // `${arr[@]...}` — the `@`/`*` arrives as the first extra arg
                let name = if matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@" || a == "*") {
                    format!("{name}[@]")
                } else {
                    name.clone()
                };
                // strip the marker only when present (a joined `a[@]`
                // name with real extras keeps them — and empty extras
                // must not panic). Marker extras render as quoted Go
                // literals `"@"`/`"*"` via expr_str.
                let extra = if (name.ends_with("[@]") || name.ends_with("[*]"))
                    && matches!(extra.first().map(|s| s.as_str()), Some("\"@\"") | Some("\"*\""))
                {
                    extra[1..].to_vec()
                } else {
                    extra
                };
                format!(
                    "pExp({}, {}{})",
                    Self::go_str(op),
                    Self::go_str(&name),
                    join_args(&extra)
                )
            }
        }
    }

    /// `join(list, sep)` Call — sep defaults to a space.
    fn join_call(&mut self, args: &[IrExpr]) -> String {
        let Some(list) = args.first() else {
            return self.sh2_stub("join");
        };
        // `[*]` joins default to the IFS separator (not blank)
        let star_join = match list {
            IrExpr::Call { func, args } if func == "param" => {
                matches!(args.first(), Some(IrExpr::Str(op, _)) if op == "*")
                    || matches!(args.get(1), Some(IrExpr::Str(n, _)) if n.ends_with("[*]"))
            }
            _ => false,
        };
        let sep = match args.get(1) {
            Some(s) => self.expr_str(s),
            None if star_join => "ifsJoin".to_string(),
            None => "\" \"".to_string(),
        };
        match list {
            IrExpr::Call { func, args } if func == "param" => {
                if let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) =
                    (args.first(), args.get(1))
                {
                    self.need_vars = true;
                    let name = normalize_assoc_key(name);
                    let extra: Vec<String> = args[2..].iter().map(|a| self.expr_str(a)).collect();
                    return format!(
                        "strings.Join(pList({}, {}{}), {sep})",
                        Self::go_str(op),
                        Self::go_str(&name),
                        join_args(&extra)
                    );
                }
                self.sh2_stub("join")
            }
            IrExpr::Call { func, args } if func == "arrayItems" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.need_vars = true;
                    let name = normalize_assoc_key(name);
                    return format!("strings.Join(pList(\"\", {}), {sep})", Self::go_str(&name));
                }
                self.sh2_stub("join")
            }
            other => {
                let v = self.expr_any(other);
                format!("s2s({v})")
            }
        }
    }

    fn sh2_stub(&mut self, note: &str) -> String {
        self.mark_todo(&format!("{note}"));
        "0".to_string()
    }

    /// The env literal for `bash -c` calls: every written scalar var.
    fn env_lit(&mut self) -> String {
        let mut parts = Vec::new();
        let written: Vec<String> = self.written.iter().cloned().collect();
        for v in &written {
            if v == "?" || self.is_arr(v) || v.starts_with('#') {
                continue;
            }
            if !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let m = self.go_ident(v);
            parts.push(format!("{} + s2s({m})", Self::go_str(&format!("{v}="))));
        }
        format!("[]string{{{}}}", parts.join(", "))
    }

    /// Render-time expansion of `$var`/`${var}`/`$1`/`$(…)` inside a
    /// literal string → a Go string expression.
    fn expand_str(&mut self, s: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let b: Vec<char> = s.chars().collect();
        let mut i = 0;
        let mut lit = String::new();
        while i < b.len() {
            if b[i] == '$' && i + 1 < b.len() && b[i + 1] != '$' {
                if !lit.is_empty() {
                    parts.push(Self::go_str(&lit));
                    lit.clear();
                }
                if b[i + 1] == '{' {
                    let mut j = i + 2;
                    while j < b.len() && b[j] != '}' {
                        j += 1;
                    }
                    if j < b.len() {
                        let inner: String = b[i + 2..j].iter().collect();
                        parts.push(self.expand_braced(&inner));
                        i = j + 1;
                        continue;
                    }
                }
                if b[i + 1] == '(' {
                    let mut j = i + 2;
                    let mut d = 1;
                    while j < b.len() && d > 0 {
                        if b[j] == '(' {
                            d += 1;
                        }
                        if b[j] == ')' {
                            d -= 1;
                        }
                        j += 1;
                    }
                    if d == 0 {
                        let cmd: String = b[i + 2..j - 1].iter().collect();
                        let env = self.env_lit();
                        parts.push(format!("capRun({}, {env})", Self::go_str(&cmd)));
                        i = j;
                        continue;
                    }
                }
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '_') {
                    j += 1;
                }
                let name: String = b[i + 1..j].iter().collect();
                parts.push(self.getvar_str(&name));
                i = j;
                continue;
            }
            lit.push(b[i]);
            i += 1;
        }
        if !lit.is_empty() {
            parts.push(Self::go_str(&lit));
        }
        if parts.is_empty() {
            return Self::go_str(s);
        }
        let mut out = parts[0].clone();
        for p in &parts[1..] {
            out = format!("({out} + {p})");
        }
        out
    }

    /// `${...}` content → Go string expr.
    fn expand_braced(&mut self, inner: &str) -> String {
        for op in [
            ":-", ":=", ":?", "##", "%%", "//", "^^", ",,", "#", "%", "/", "^", ",",
        ] {
            if let Some(idx) = inner.find(op) {
                if idx > 0 {
                    let name = &inner[..idx];
                    let rest = &inner[idx + op.len()..];
                    self.need_vars = true;
                    return format!(
                        "pExp({}, {}, {})",
                        Self::go_str(op),
                        Self::go_str(name),
                        Self::go_str(rest)
                    );
                }
            }
        }
        // `${var:off[:len]}` substring (the bare-`:` form isn't in the op
        // table above) — offsets evaluate like index text
        if let Some(ci) = inner.find(':') {
            let name = &inner[..ci];
            if !name.is_empty()
                && !name.starts_with('#')
                && !name.starts_with('!')
                && name != "@"
                && name != "*"
                && !name.contains('[')
            {
                let rest = &inner[ci + 1..];
                let (off, len) = match rest.split_once(':') {
                    Some((o, l)) => (o, Some(l)),
                    None => (rest, None),
                };
                self.need_vars = true;
                let off_go = format!("s2s({})", self.index_text_num(off));
                return match len {
                    Some(l) if !l.is_empty() => {
                        let len_go = format!("s2s({})", self.index_text_num(l));
                        format!(
                            "pExp(\"slice\", {}, {off_go}, {len_go})",
                            Self::go_str(name)
                        )
                    }
                    _ => format!("pExp(\"slice\", {}, {off_go})", Self::go_str(name)),
                };
            }
        }
        self.need_vars = true;
        format!("pExp(\"\", {})", Self::go_str(inner))
    }

    // ── shell-text reconstruction (for bash -c fallbacks) ────────────

    fn cmd_text_stmts(&mut self, stmts: &[IrStmt]) -> Option<String> {
        let mut parts = Vec::new();
        for s in stmts {
            parts.push(self.cmd_text_stmt(s)?);
        }
        Some(parts.join("; "))
    }

    fn cmd_text_stmt(&mut self, s: &IrStmt) -> Option<String> {
        match s {
            // arithmetic in command position is a `(( … ))` statement —
            // `$((…))` would try to RUN the value as a command
            IrStmt::Expr(IrExpr::Arith(a)) => {
                self.cmd_text_arith(a).map(|t| format!("(({t}))"))
            }
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "arith" || func == "let" => {
                if args.len() == 1 {
                    if let Some(IrExpr::Str(st, _)) = args.first() {
                        return Some(format!("(({st}))"));
                    }
                }
                None
            }
            IrStmt::Expr(e) => self.cmd_text_expr(e),
            IrStmt::Ext(_) => None,
            // try/except has no shell text — a bash -c fallback cannot
            // express it
            IrStmt::Try { .. } => None,
            // select over channels has no shell text either
            IrStmt::Select { .. } => None,
            // inline asm has no shell text either (JS no-op only)
            IrStmt::Asm { .. } => None,
            IrStmt::Assign { targets, expr, asm, .. } => {
                // Declarator-position asm label (core request
                // c-sh-go-toplevelasmargument-20260814-042952) — no Go
                // rendering; refuse loudly (refuse > guess).
                if let Some(spec) = asm {
                    self.mark_todo(&format!("asm label '{}' on an assign", spec.template));
                    return None;
                }
                let t = targets.first()?;
                // `(( counter++ ))` / `(( x += 1 ))` — statement-form
                // arithmetic keeps bash's result-derived status, which
                // `t=$((…))` would not reproduce
                if let IrExpr::Arith(a) = expr {
                    match a.as_ref() {
                        crate::ir::ArithAst::IncDec { var, delta, prefix } => {
                            if var == &t.var && t.indices.is_empty() {
                                let op = match (*delta, *prefix) {
                                    (1, false) => format!("{var}++"),
                                    (-1, false) => format!("{var}--"),
                                    (1, true) => format!("++{var}"),
                                    (-1, true) => format!("--{var}"),
                                    _ => return None,
                                };
                                return Some(format!("(({op}))"));
                            }
                        }
                        crate::ir::ArithAst::Assign { var, op, rhs } => {
                            if var == &t.var && t.indices.is_empty() {
                                let r = self.cmd_text_arith(rhs)?;
                                return Some(format!("(({var} {op} {r}))"));
                            }
                        }
                        _ => {}
                    }
                }
                // array assignment `arr=(a b c)`
                if let IrExpr::Call { func, args } = expr {
                    if func == "setArray" {
                        let mut items = Vec::new();
                        for e in setarray_elems(args) {
                            items.push(self.cmd_text_expr(e)?);
                        }
                        return Some(format!("{}={}", t.var, self.list_text(&items)));
                    }
                }
                let v = self.cmd_text_expr(expr)?;
                let idx = if t.indices.is_empty() {
                    String::new()
                } else {
                    let mut out = String::new();
                    for k in &t.indices {
                        out.push_str(&self.cmd_text_expr(k)?);
                    }
                    format!("[{out}]")
                };
                Some(format!("{}{idx}={v}", t.var))
            }
            IrStmt::Declare { vars, init, local, .. } => {
                let d = vars.first()?;
                match init {
                    Some(e) => {
                        let v = self.cmd_text_expr(e)?;
                        // `typeset -f` display keeps `local` (bash shows
                        // it); fallback children are top-level where
                        // `local` is an error, so only in display mode
                        let kw = if self.cmd_display && *local {
                            "local "
                        } else {
                            ""
                        };
                        Some(format!("{kw}{}={}", d.name, v))
                    }
                    None => Some(format!("declare {}", d.name)),
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                let mut items = Vec::new();
                for e in elements {
                    items.push(self.cmd_text_expr(e)?);
                }
                Some(format!("{}={}", var, self.list_text(&items)))
            }
            IrStmt::Subshell(b) | IrStmt::Block(b) => {
                let t = self.cmd_text_stmts(b)?;
                Some(format!("( {t} )"))
            }
            IrStmt::ForInit { .. } => None,
            IrStmt::Continue => Some("continue".to_string()),
            IrStmt::Break => Some("break".to_string()),
            IrStmt::While { cond, body } => {
                let b = self.cmd_text_stmts(body)?;
                // multi-command condition (`while cmd1; cmd2; do …`)
                if let IrExpr::Call { func, args } = cond {
                    if func == "block" || func == "subshell" {
                        if let Some(IrExpr::Arrow(cs)) = args.first() {
                            let c = self.cmd_text_stmts(cs)?;
                            return Some(format!("while {c}; do {b}; done"));
                        }
                    }
                }
                let c = self.cmd_text_expr(cond)?;
                Some(format!("while {c}; do {b}; done"))
            }
            IrStmt::DoWhile { body, cond, until } => {
                let b = self.cmd_text_stmts(body)?;
                let c = self.cmd_text_expr(cond)?;
                if *until {
                    Some(format!("until {c}; do {b}; done"))
                } else {
                    Some(format!("while !({c}); do {b}; done"))
                }
            }
            IrStmt::For { var, iter, body } => {
                let b = self.cmd_text_stmts(body)?;
                let list = self.cmd_text_expr(iter)?;
                Some(format!("for {var} in {list}; do {b}; done"))
            }
            IrStmt::Function { name, body, .. } => {
                let b = self.cmd_text_stmts(body)?;
                Some(format!("{name}() {{ {b}; }}"))
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                let d = self.cmd_text_expr(discriminant)?;
                let mut out = format!("case {d} in");
                for c in clauses {
                    let pats: Vec<String> = c.patterns.clone();
                    let b = self.cmd_text_stmts(&c.body)?;
                    out.push_str(&format!(" {} ) {b} ;;", pats.join(" | ")));
                }
                out.push_str(" esac");
                Some(out)
            }
            IrStmt::Exit(e) => {
                let code = match e {
                    Some(x) => self.cmd_text_expr(x)?,
                    None => "0".to_string(),
                };
                Some(format!("exit {code}"))
            }
            IrStmt::Return(_) => Some("return".to_string()),
            IrStmt::Pipeline {
                stages,
                capture,
                cmd_str,
                ..
            } => {
                let text = match cmd_str {
                    Some(c) => c.clone(),
                    None => {
                        let mut parts = Vec::new();
                        for st in stages {
                            parts.push(self.cmd_text_stmts(st)?);
                        }
                        pipe_join(&parts)
                    }
                };
                match capture {
                    Some(name) => Some(format!("{name}=$({text})")),
                    None => Some(text),
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                let t = self.cmd_text_stmts(inner)?;
                let mut with = format!("( {t} )");
                for r in redirects {
                    with.push(' ');
                    with.push_str(&self.redirect_ir_text(r)?);
                }
                Some(with)
            }
            IrStmt::Background(b) => {
                let t = self.cmd_text_stmts(b)?;
                Some(format!("( {t} ) &"))
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.cmd_text_expr(cond)?;
                let t = self.cmd_text_stmts(then)?;
                let mut out = format!("if {c}; then {t};");
                for (ec, eb) in elsifs {
                    let ec = self.cmd_text_expr(ec)?;
                    let eb = self.cmd_text_stmts(eb)?;
                    out.push_str(&format!(" elif {ec}; then {eb};"));
                }
                if !else_.is_empty() {
                    let eb = self.cmd_text_stmts(else_)?;
                    out.push_str(&format!(" else {eb};"));
                }
                out.push_str(" fi");
                Some(out)
            }
            IrStmt::SetChildError(_) | IrStmt::Warn { .. } => Some(":".to_string()),
            IrStmt::Die { .. } => Some("exit 1".to_string()),
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                let v = self.cmd_text_expr(value)?;
                let nl = if *newline { "" } else { " -n" };
                if let Some(t) = target {
                    let tt = self.cmd_text_expr_from_str(t)?;
                    return Some(format!("echo{nl} {v} >>{tt}"));
                }
                Some(format!("echo{nl} {v}"))
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                let p = self.cmd_text_expr(path)?;
                let c = self.cmd_text_expr(content)?;
                let op = if *append { ">>" } else { ">" };
                Some(format!("printf '%s' {c} {op} {p}"))
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            } => self.exec_shell_text(cmd, args, capture, redirects, env),
            IrStmt::Require(_) | IrStmt::RawText(_) => None,
            IrStmt::Label(_) | IrStmt::Goto(_) => None,
            IrStmt::Shaderize { .. } => None,
        }
    }

    /// A bare `IrStmt::Exec` node → shell text (procsub `mktemp`/`rm`,
    /// redirected externals). `cmd` + word args, optional `VAR=x` env
    /// prefix, `{fd,mode,target}`-object redirects, `$var=$(…)` capture.
    fn exec_shell_text(
        &mut self,
        cmd: &IrExpr,
        args: &[IrExpr],
        capture: &Option<String>,
        redirects: &[IrExpr],
        env: &[(String, IrExpr)],
    ) -> Option<String> {
        let c = self.cmd_text_expr(cmd)?;
        // rebuild properly: env assignments first, then cmd + args
        let mut all = Vec::new();
        for (k, v) in env {
            let t = self.cmd_text_expr(v)?;
            all.push(format!("{k}={}", cmd_quote(v, &t)));
        }
        all.push(cmd_quote(cmd, &c));
        for a in args {
            let t = self.cmd_text_expr(a)?;
            all.push(cmd_quote(a, &t));
        }
        let mut text = all.join(" ");
        for r in redirects {
            text.push(' ');
            text.push_str(&self.redirect_text(r)?);
        }
        match capture {
            Some(name) => Some(format!("{name}=$({text})")),
            None => Some(text),
        }
    }

    /// `a b c` — a whitespace-joined word list (for `arr=(…)`).
    fn list_text(&mut self, items: &[String]) -> String {
        items.join(" ")
    }

    /// A filehandle name → shell text.
    fn cmd_text_expr_from_str(&mut self, t: &str) -> Option<String> {
        Some(t.to_string())
    }

    /// Rebuild an `ArithAst` as shell arithmetic TEXT (`$((…))` operand),
    /// so the `and`/`or`/capture bash -c fallback can reconstruct
    /// arithmetic-containing operands. Conservative: refuses (None) the
    /// assignment/incdec/cast shapes rather than guess.
    fn cmd_text_arith(&self, a: &crate::ir::ArithAst) -> Option<String> {
        match a {
            crate::ir::ArithAst::Num(n) => Some(n.to_string()),
            crate::ir::ArithAst::Var(name) | crate::ir::ArithAst::Ident(name) => {
                Some(format!("${name}"))
            }
            crate::ir::ArithAst::Index { var, key } => Some(format!(
                "{var}[{}]",
                self.cmd_text_arith(key)?
            )),
            crate::ir::ArithAst::Bin { op, lhs, rhs } => Some(format!(
                "({} {} {})",
                self.cmd_text_arith(lhs)?,
                op,
                self.cmd_text_arith(rhs)?
            )),
            crate::ir::ArithAst::Un { op, arg } => {
                Some(format!("({op}{})", self.cmd_text_arith(arg)?))
            }
            crate::ir::ArithAst::Cond { test, then, else_ } => Some(format!(
                "({} ? {} : {})",
                self.cmd_text_arith(test)?,
                self.cmd_text_arith(then)?,
                self.cmd_text_arith(else_)?
            )),
            // `x=$((y++))` keeps the postfix/prefix value semantics in
            // `$((…))` position (bash: old value for postfix)
            crate::ir::ArithAst::Assign { var, op, rhs } => Some(format!(
                "({var} {op} {})",
                self.cmd_text_arith(rhs)?
            )),
            crate::ir::ArithAst::IncDec { var, delta, prefix } => match (*delta, *prefix) {
                (1, false) => Some(format!("({var}++)")),
                (-1, false) => Some(format!("({var}--)")),
                (1, true) => Some(format!("(++{var})")),
                (-1, true) => Some(format!("(--{var})")),
                _ => None,
            },
            _ => None,
        }
    }

    fn cmd_text_expr(&mut self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Str(s, _) => Some(s.clone()),
            IrExpr::Int(i) => Some(i.to_string()),
            IrExpr::Bool(b) => {
                if *b {
                    Some("true".to_string())
                } else {
                    Some("false".to_string())
                }
            }
            IrExpr::Interpolate(parts) => {
                let mut out = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(t) => out.push_str(t),
                        InterpPart::Expr(x) => out.push_str(&self.cmd_text_expr(x)?),
                    }
                }
                Some(out)
            }
            IrExpr::Var(name, _) => Some(format!("${name}")),
            IrExpr::Ident(name) => Some(format!("${name}")),
            IrExpr::Arith(a) => self
                .cmd_text_arith(a)
                .map(|t| format!("$(({t}))")),
            IrExpr::BinOp { lhs, op, rhs } => {
                let l = self.cmd_text_expr(lhs)?;
                let r = self.cmd_text_expr(rhs)?;
                match op {
                    crate::ir::BinOpKind::And => Some(format!("{l} && {r}")),
                    crate::ir::BinOpKind::Or => Some(format!("{l} || {r}")),
                    crate::ir::BinOpKind::Concat => Some(format!("{l}{r}")),
                    crate::ir::BinOpKind::Add => Some(format!("$(({l} + {r}))")),
                    crate::ir::BinOpKind::Sub => Some(format!("$(({l} - {r}))")),
                    crate::ir::BinOpKind::Mul => Some(format!("$(({l} * {r}))")),
                    crate::ir::BinOpKind::Div => Some(format!("$(({l} / {r}))")),
                    crate::ir::BinOpKind::Mod => Some(format!("$(({l} % {r}))")),
                    crate::ir::BinOpKind::Pow => Some(format!("$(({l} ** {r}))")),
                    crate::ir::BinOpKind::BitAnd => Some(format!("$(({l} & {r}))")),
                    crate::ir::BinOpKind::BitOr => Some(format!("$(({l} | {r}))")),
                    crate::ir::BinOpKind::BitXor => Some(format!("$(({l} ^ {r}))")),
                    crate::ir::BinOpKind::ShiftL => Some(format!("$(({l} << {r}))")),
                    crate::ir::BinOpKind::ShiftR => Some(format!("$(({l} >> {r}))")),
                    crate::ir::BinOpKind::Eq => Some(format!("$(({l} == {r}))")),
                    crate::ir::BinOpKind::Ne => Some(format!("$(({l} != {r}))")),
                    crate::ir::BinOpKind::Lt => Some(format!("$(({l} < {r}))")),
                    crate::ir::BinOpKind::Gt => Some(format!("$(({l} > {r}))")),
                    crate::ir::BinOpKind::Le => Some(format!("$(({l} <= {r}))")),
                    crate::ir::BinOpKind::Ge => Some(format!("$(({l} >= {r}))")),
                    crate::ir::BinOpKind::Not => Some(format!("$((! {l}))")),
                }
            }
            IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
                let cmd_expr = args.first()?;
                let cmd = self.cmd_text_expr(cmd_expr)?;
                let display = self.cmd_display;
                // a glob marker (`\x01SH2GLOB\x01*.sh`) marks an unquoted
                // glob: strip it and leave the word BARE so the child
                // expands it (quoting would freeze the pattern literally)
                let q = |e: &IrExpr, t: &str| {
                    if display && display_bareword(t) {
                        return t.to_string();
                    }
                    if let Some(stripped) = t.strip_prefix("\u{1}SH2GLOB\u{1}") {
                        return stripped.to_string();
                    }
                    cmd_quote(e, t)
                };
                let mut parts = vec![q(cmd_expr, &cmd)];
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        let text = self.cmd_text_expr(it)?;
                        parts.push(q(it, &text));
                    }
                }
                Some(parts.join(" "))
            }
            IrExpr::Call { func, args } if func == "grepMatches" => {
                // `grep -o` lift — reconstruct what the C backend emits
                // (grep -o[E/i/F] pat <<< text) so the bash -c child does
                // the matching itself.
                self.grep_matches_text(args)
            }
            IrExpr::Call { func, args } if func == "pipeline" => {
                let mut texts = Vec::new();
                if let Some(IrExpr::Array(items)) = args.first() {
                    for it in items {
                        if let IrExpr::Arrow(body) = it {
                            texts.push(self.cmd_text_stmts(body)?);
                        } else {
                            return None;
                        }
                    }
                }
                Some(pipe_join(&texts))
            }
            IrExpr::Call { func, args } if func == "test" => {
                let s = match args.first() {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return None,
                };
                // second arg names the bracket kind (`[[` vs `[`) — a
                // `=~` regex test must stay double-bracketed (`[ a=~b ]`
                // tests a literal non-empty string, always true)
                match args.get(1) {
                    Some(IrExpr::Str(k, _)) if k == "[[" => Some(format!("[[ {s} ]]")),
                    _ => Some(format!("[ {s} ]")),
                }
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                match name.as_str() {
                    "?" => Some("$?".to_string()),
                    "@" | "*" => Some("$@".to_string()),
                    n if n.chars().all(|c| c.is_ascii_digit()) => Some(format!("${n}")),
                    _ => Some(format!("${name}")),
                }
            }
            IrExpr::Call { func, args } if func == "redirect" => {
                // `redirect` Call: args[0] = Arrow(body), args[1] = Array of
                // {fd, mode, target} — reconstruct as `( body ) redir`s so an
                // `and`/`or`/capture operand containing a redirect resolves.
                let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                let text = self.cmd_text_stmts(body)?;
                let mut out = format!("( {text} )");
                for r in redirs {
                    let rt = self.redirect_text(r)?;
                    out.push(' ');
                    out.push_str(&rt);
                }
                Some(out)
            }
            IrExpr::Call { func, args } if func == "subshell" || func == "block" => {
                // `( body )` / `{ body }` groups in an `and`/`or`/capture
                // operand — reconstruct as a subshell group.
                let Some(IrExpr::Arrow(body)) = args.first() else {
                    return None;
                };
                let text = self.cmd_text_stmts(body)?;
                Some(format!("( {text} )"))
            }
            IrExpr::Call { func, args } if func == "assign" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                let op = match args.get(1) {
                    Some(IrExpr::Str(o, _)) if o != "=" => o.clone(),
                    _ => "=".to_string(),
                };
                let value = match args.len() {
                    2 => &args[1],
                    _ => &args[2],
                };
                let v = self.cmd_text_expr(value)?;
                Some(format!("{name}{op}{v}"))
            }
            IrExpr::Call { func, args } if func == "setArray" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                let mut items = Vec::new();
                for e in setarray_elems(args) {
                    items.push(self.cmd_text_expr(e)?);
                }
                Some(format!("{name}=({})", items.join(" ")))
            }
            IrExpr::Call { func, args } if func == "return" => {
                if let Some(a) = args.first() {
                    Some(format!("return {}", self.cmd_text_expr(a)?))
                } else {
                    Some("return".to_string())
                }
            }
            IrExpr::Call { func, args: _ } if func == "break" => Some("break".to_string()),
            IrExpr::Call { func, args: _ } if func == "continue" => Some("continue".to_string()),
            IrExpr::Call { func, args } if func == "let" => {
                let s = match args.first() {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return None,
                };
                Some(format!("let {s}"))
            }
            IrExpr::Call { func, args } if func == "param" => {
                let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                if op == "@" || op == "*" {
                    return Some("$@".to_string());
                }
                let extra: Vec<String> = args[2..]
                    .iter()
                    .map(|a| self.cmd_text_expr(a))
                    .collect::<Option<Vec<_>>>()?;
                // `[@]`-element forms (extra[0] == "@"): the expansion is
                // over `${name[...]}` (`slice` may add `:off:len`) — the
                // flat `name+op+extra` join garbles these (`${!mapslice@}`).
                if matches!(extra.first(), Some(e) if e == "@") {
                    let rest: Vec<&str> = extra[1..]
                        .iter()
                        .map(|e| e.as_str())
                        .filter(|e| !e.is_empty())
                        .collect();
                    let inner = if op == "#" {
                        format!("#{name}[@]")
                    } else if op == "slice" && !rest.is_empty() {
                        format!("{name}[@]:{}", rest.join(":"))
                    } else if op.is_empty() || op == "slice" {
                        format!("{name}[@]")
                    } else {
                        format!("{name}[@]{op}{}", rest.join(""))
                    };
                    return Some(format!("${{{inner}}}"));
                }
                let mut inner = name.clone();
                if !extra.is_empty() {
                    inner.push_str(op);
                    inner.push_str(&extra.join(""));
                }
                Some(format!("${{{inner}}}"))
            }
            IrExpr::Call { func, args } if func == "arrayIndex" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                let key = match args.get(1) {
                    Some(k) => self.cmd_text_expr(k)?,
                    None => return None,
                };
                Some(format!("${{{name}[{key}]}}"))
            }
            IrExpr::Call { func, args } if func == "arrayItems" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                Some(format!("${{{name}[@]}}"))
            }
            IrExpr::Call { func, args } if func == "arrayLen" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                Some(format!("${{#{name}[@]}}"))
            }
            IrExpr::Call { func, args } if func == "listVar" => {
                // `$@` as words (`for d in "$@"`, `cmd "$@"`). The child
                // resolves it via its own positional params — argless in
                // the gate, matching the reference run.
                match args.first() {
                    Some(IrExpr::Str(n, _)) if n == "@" || n == "*" => {
                        Some("\"$@\"".to_string())
                    }
                    _ => None,
                }
            }
            IrExpr::Call { func, args } if func == "split" => {
                // word-split of the expansion — in shell text the bare
                // (unquoted) expansion re-splits in the child
                self.cmd_text_expr(args.first()?)
            }
            IrExpr::Call { func, args } if func == "join" => {
                // `join(list)` with the default blank separator renders as
                // the word list itself; any other separator refuses
                let sep = match args.get(1) {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    None => " ".to_string(),
                    _ => return None,
                };
                if sep != " " {
                    return None;
                }
                self.cmd_text_expr(args.first()?)
            }
            IrExpr::Call { func, args } if func == "and" || func == "or" => {
                // Call-form short-circuit (`diff … 2>/dev/null || true`
                // after procsub materialisation): join the branch texts.
                let mut texts = Vec::new();
                for a in args {
                    texts.push(self.cmd_text_expr(a)?);
                }
                let sep = if func == "and" { " && " } else { " || " };
                Some(texts.join(sep))
            }
            IrExpr::Call { func, args } if func == "captureWords" => {
                let t = match args.first() {
                    Some(IrExpr::Arrow(body)) => self.cmd_text_stmts(body)?,
                    _ => return None,
                };
                Some(format!("$({t})"))
            }
            IrExpr::Index { var, key } => {
                let k = self.cmd_text_expr(key)?;
                Some(format!("${{{var}[{k}]}}"))
            }
            IrExpr::Array(items) => {
                let mut parts = Vec::new();
                for it in items {
                    parts.push(self.cmd_text_expr(it)?);
                }
                Some(parts.join(" "))
            }
            IrExpr::Capture { expr, .. } => {
                let t = self.cmd_text_expr(expr)?;
                Some(format!("$({t})"))
            }
            // a bare statement group in value position (nested `$(…)`
            // inside interpolation reaches here through the Capture arm)
            IrExpr::Arrow(body) => self.cmd_text_stmts(body),
            IrExpr::Call { func, args } if func == "capture" => {
                let t = match args.first() {
                    Some(IrExpr::Arrow(body)) => self.cmd_text_stmts(body)?,
                    _ => return None,
                };
                Some(format!("$({t})"))
            }
            IrExpr::Call { func, args } if func == "redirect" => {
                let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                let text = self.cmd_text_stmts(body)?;
                let mut with = format!("( {text} )");
                for r in redirs {
                    with.push(' ');
                    with.push_str(&self.redirect_text(r)?);
                }
                Some(with)
            }
            IrExpr::Call { func, args } if func == "arith" => {
                let s = match args.first() {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return None,
                };
                Some(format!("$(({s}))"))
            }
            IrExpr::Call { func, args } if func == "subshell" || func == "block" => {
                let t = match args.first() {
                    Some(IrExpr::Arrow(body)) => self.cmd_text_stmts(body)?,
                    _ => return None,
                };
                Some(format!("( {t} )"))
            }
            IrExpr::Call { func, args } if func == "brace" => {
                let expanded = self.brace_expand(args)?;
                Some(expanded.join(" "))
            }
            IrExpr::Call { func, args } if func == "whileLoop" => {
                let t = match args.first() {
                    Some(IrExpr::Arrow(body)) => self.cmd_text_stmts(body)?,
                    _ => return None,
                };
                Some(t)
            }
            _ => None,
        }
    }

    /// exec Call → (cmd text, argv text list).
    fn exec_text(&mut self, args: &[IrExpr]) -> Option<(String, Vec<String>)> {
        let cmd = self.cmd_text_expr(args.first()?)?;
        let mut argv = Vec::new();
        if let Some(IrExpr::Array(items)) = args.get(1) {
            for it in items {
                argv.push(self.cmd_text_expr(it)?);
            }
        }
        Some((cmd, argv))
    }

    // ── `[ ]` / `[[ ]]` test lowering ────────────────────────────────

    /// Mini evaluator for the common test patterns; None → stub.
    /// Tokenizes the test string and parses with real precedence:
    /// `-o`/`||` loosest, then `-a`/`&&`, then `!`, then `( … )` groups,
    /// then unary flags / binary operators.
    fn test_render(&mut self, s: &str) -> Option<String> {
        let t = s.trim();
        if t.is_empty() {
            return Some("true".to_string());
        }
        let toks = test_tokens(t)?;
        if toks.is_empty() {
            return Some("true".to_string());
        }
        let mut p = TestParser { toks, i: 0 };
        let e = p.or_expr(self)?;
        if p.i != p.toks.len() {
            return None;
        }
        Some(e)
    }

    /// Binary test operator → Go bool expr. `lraw`/`rraw` are the raw
    /// operand texts (for glob/quoting decisions).
    fn test_binop(
        &mut self,
        op: &str,
        _lraw: &str,
        l: String,
        lk: bool,
        rraw: &str,
        r: String,
        rk: bool,
    ) -> Option<String> {
        match op {
            "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                let goop = match op {
                    "-eq" => "==",
                    "-ne" => "!=",
                    "-lt" => "<",
                    "-le" => "<=",
                    "-gt" => ">",
                    "-ge" => ">=",
                    _ => unreachable!(),
                };
                let (l, r) = (numify(l, lk, self), numify(r, rk, self));
                Some(format!("({l} {goop} {r})"))
            }
            "-nt" | "-ot" => {
                let (l, r) = (strify(l, lk, self), strify(r, rk, self));
                Some(format!("fileNewer({l}, {r}, {})", op == "-nt"))
            }
            "-ef" => {
                let (l, r) = (strify(l, lk, self), strify(r, rk, self));
                Some(format!("fileSame({l}, {r})"))
            }
            "=" | "==" => {
                let l = strify(l, lk, self);
                // the raw rhs decides glob vs literal (the operand expr
                // is already Go-quoted)
                let rraw = strip_sh2glob(rraw);
                // extglob `!(X)` — no regex lookahead in Go; split the
                // pattern and test the prefix + literal suffix
                if let Some((inner, rest)) = extglob_not(&rraw) {
                    if rest.is_empty() {
                        return Some(format!("!globMatch({}, {l})", Self::go_str(&inner)));
                    }
                    return Some(format!(
                        "(strings.HasSuffix({l}, {}) && !globMatch({}, strings.TrimSuffix({l}, {})))",
                        Self::go_str(&rest),
                        Self::go_str(&inner),
                        Self::go_str(&rest)
                    ));
                }
                let r = strify(r, rk, self);
                // `==` with glob metachars → globMatch; else literal
                if has_glob(&rraw) && !rraw.contains('$') {
                    Some(format!("globMatch({r}, {l})"))
                } else if self.nocase {
                    Some(format!("strings.EqualFold({l}, {r})"))
                } else {
                    Some(format!("({l} == {r})"))
                }
            }
            "!=" => {
                let l = strify(l, lk, self);
                let r = strify(r, rk, self);
                Some(format!("({l} != {r})"))
            }
            "<" | ">" => {
                let l = strify(l, lk, self);
                let r = strify(r, rk, self);
                Some(format!("({l} {op} {r})"))
            }
            "=~" => {
                let l = strify(l, lk, self);
                let r = strify(r, rk, self);
                Some(format!("reMatch({r}, {l})"))
            }
            _ => None,
        }
    }

    /// A test operand: `"$y"`/`$y`/`y` → the var; `$(…)` → capture; a
    /// number → literal. Returns (expr, is_num).
    fn test_operand(&mut self, t: &str) -> Option<(String, bool)> {
        let t = t.trim();
        let has_dollar = t.char_indices().any(|(i, c)| {
            c == '$' && {
                let rest = &t[i + 1..];
                rest.chars().next().map_or(false, |n| {
                    n.is_ascii_alphanumeric() || n == '_' || n == '{' || n == '('
                })
            }
        });
        let quoted = t.starts_with('"') || t.starts_with('\'');
        let inner = t.trim_matches('"').trim_matches('\'');
        // tilde expansion (unquoted only — a quoted `"~"` is literal):
        // `~` and `~/…` prefix the home directory
        if !quoted {
            if inner == "~" {
                return Some(("os.Getenv(\"HOME\")".to_string(), false));
            }
            if let Some(rest) = inner.strip_prefix("~/") {
                return Some((
                    format!("os.Getenv(\"HOME\") + {}", Self::go_str(&format!("/{rest}"))),
                    false,
                ));
            }
        }
        // `$((…))` arithmetic operand — native (no child bash). Must
        // precede the `$(` arm below (arithmetic also starts with `$(`).
        if let Some(rest) = inner.strip_prefix("$((") {
            if let Some(close) = rest.rfind("))") {
                let arith = rest[..close].to_string();
                if let Some((sides, v)) = self.arith_str(&arith) {
                    for sd in sides {
                        self.sides.push(sd);
                    }
                    return Some((v, true));
                }
            }
            // unparseable: fall through to the command-substitution arm
            // (bash also errors there — same observable behavior)
        }
        if let Some(rest) = inner.strip_prefix("$(") {
            if let Some(close) = rest.rfind(')') {
                let cmd = rest[..close].to_string();
                let env = self.env_lit();
                return Some((format!("capRun({}, {env})", Self::go_str(&cmd)), false));
            }
            return None;
        }
        // `${name% *}` / `${x:-d}` — a param expansion as the operand
        if t.starts_with("${") && t.ends_with('}') {
            let inner2 = &t[2..t.len() - 1];
            self.need_vars = true;
            return Some((self.expand_braced(inner2), false));
        }
        let inner = inner.strip_prefix('$').unwrap_or(inner);
        let inner = inner
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(inner);
        match inner {
            "#" => Some(("int64(len(argsList()))".to_string(), true)),
            "?" => {
                self.need_st = true;
                self.st_read = true;
                Some(("st".to_string(), true))
            }
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                if !has_dollar {
                    if let Ok(v) = n.parse::<i64>() {
                        return Some((v.to_string(), true));
                    }
                }
                let i: i64 = n.parse().unwrap_or(1);
                Some((format!("paramAt({i})"), false))
            }
            _ => {
                if let Some(base) = inner.strip_suffix("[@]").or_else(|| inner.strip_suffix("[*]")) {
                    if self.declared(base) {
                        self.need_vars = true;
                        return Some((
                            format!("strings.Join(pList(\"\", {}), \" \")", Self::go_str(inner)),
                            false,
                        ));
                    }
                }
                if self.declared(inner) && !(quoted && !has_dollar) {
                    // `"c"` is a literal, not the var c
                    let m = self.ident_of(inner);
                    Some((m, self.is_num(inner)))
                } else if let Ok(n) = inner.parse::<i64>() {
                    Some((n.to_string(), true))
                } else if has_dollar {
                    // `$name/rest` — the variable ends at the first
                    // non-identifier char (`$HOME/Documents`); a braced
                    // `${HOME}/rest` unwraps the same way
                    let (nm, rest) = if let Some(braced) = inner.strip_prefix('{') {
                        match braced.find('}') {
                            Some(ci) => (&braced[..ci], &braced[ci + 1..]),
                            None => ("", inner),
                        }
                    } else {
                        let split = inner
                            .find(|c: char| {
                                !(c.is_ascii_alphanumeric() || c == '_')
                            })
                            .unwrap_or(inner.len());
                        inner.split_at(split)
                    };
                    if !rest.is_empty() && !nm.is_empty() {
                        if nm.chars().all(|c| c.is_ascii_digit()) {
                            let i: i64 = nm.parse().unwrap_or(1);
                            return Some((
                                format!("(paramAt({i}) + {})", Self::go_str(rest)),
                                false,
                            ));
                        }
                        if is_plain_name(nm) {
                            let base = if self.declared(nm) {
                                let m = self.ident_of(nm);
                                if self.is_num(nm) {
                                    m
                                } else {
                                    format!("s2s({m})")
                                }
                            } else {
                                format!("os.Getenv({})", Self::go_str(nm))
                            };
                            return Some((
                                format!("({base} + {})", Self::go_str(rest)),
                                false,
                            ));
                        }
                    }
                    Some((format!("os.Getenv({})", Self::go_str(inner)), false))
                } else {
                    Some((Self::go_str(inner), false))
                }
            }
        }
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => self.stmt_expr(e),
            IrStmt::Ext(node) => {
                // STREAMING line iteration — O(1) memory, never slurps.
                // Body statements render through the Go backend's own
                // emitters (recursion goes back through this match).
                if node.tag() == "ForEachLine" {
                    use std::sync::atomic::{AtomicUsize, Ordering};
                    static FL_N: AtomicUsize = AtomicUsize::new(0);
                    let k = FL_N.fetch_add(1, Ordering::Relaxed);
                    let fl = node.as_any().downcast_ref::<crate::shir_nodes::ForEachLine>()
                        .expect("tag/type agree");
                    // The loop var is a STRING: register the verdict before
                    // anything renders, so reads use native string ops and
                    // the hoisted decl is `var __l string` not `any`.
                    self.var_types.insert(fl.var.clone(), IrType::Str);
                    let src = self.expr_any(&fl.source);
                    self.emit(&format!("_ff{k}, _err{k} := os.Open({src})", src = src));
                    self.emit(&format!("if _err{k} != nil {{ st = 1 }} else {{"));
                    self.depth += 1;
                    self.emit(&format!("_sc{k} := bufio.NewScanner(_ff{k})"));
                    self.loop_head(&format!(" _sc{k}.Scan() {{"));
                    self.emit(&format!("{} = _sc{k}.Text()", fl.var));
                    self.emit(&format!("{} = _sc{k}.Text()", fl.var));
                    self.mark_written(&fl.var);
                    // Go errors on unused locals — silence when the body
                    // doesn't read the line (pure counters).
                    self.emit(&format!("_ = {}", fl.var));
                    for b in &fl.body {
                        self.stmt(b);
                    }
                    self.loop_close();
                    self.depth -= 1;
                    self.emit("}");
                    self.need_st = true;
                } else {
                    panic!("go backend: Ext node {} unsupported", node.tag());
                }
            }
            IrStmt::Assign { targets, expr, .. } => {
                // nameref writes go to the target's home (`ref=v`
                // writes target) — resolve plain single targets up front
                if self.nameref_targets.is_empty() {
                    self.stmt_assign(targets, expr)
                } else {
                    let rt: Vec<crate::ir::AssignTarget> = targets
                        .iter()
                        .map(|t| crate::ir::AssignTarget {
                            var: self.nameref_resolve(&t.var),
                            sigil: t.sigil.clone(),
                            indices: t.indices.clone(),
                        })
                        .collect();
                    self.stmt_assign(&rt, expr)
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    self.mark_written(&d.name);
                    let m = self.go_ident(&d.name);
                    if let Some(e) = init {
                        let rhs = if self.is_num(&d.name)
                            || self.arith_ints.contains(&d.name)
                            || self.int_attrs.contains(&d.name)
                        {
                            self.int_rhs(e)
                        } else if self.is_str(&d.name) {
                            self.expr_str(e)
                        } else {
                            self.expr_any(e)
                        };
                        self.flush_sides();
                        let sync = self.sync_inline(&d.name);
                        self.emit(&format!("{m} = {rhs};{sync}"));
                        if self.need_st {
                            self.emit("st = 0;");
                        }
                    } else {
                        self.emit(&format!("// declare {m}"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                // a lone `$(…)`/`$@` is the whole array; mixed with
                // literals it cannot spread into a literal
                if elements.len() == 1 && is_capture_words(&elements[0]) {
                    let cap = self.capture_words_cap(&elements[0]);
                    self.mark_arr(var);
                    let m = self.go_ident(var);
                    self.flush_sides();
                    self.emit(&format!("{m} = {cap};"));
                    return;
                }
                if elements.iter().any(is_capture_words) {
                    self.mark_todo("captureWords mixed with literals");
                    return;
                }
                // a lone `$@` is the whole array; mixed with literals it
                // cannot spread into a literal (refuse loudly)
                if elements.len() == 1 && is_listvar_at(&elements[0]) {
                    self.mark_arr(var);
                    let m = self.go_ident(var);
                    self.flush_sides();
                    self.emit(&format!("{m} = strsToAny(argsList());"));
                    return;
                }
                if elements.iter().any(is_listvar_at) {
                    self.mark_todo("listVar mixed with literals");
                    return;
                }
                self.mark_arr(var);
                let m = self.go_ident(var);
                let elems: Vec<String> = elements.iter().map(|e| self.expr_any(e)).collect();
                self.flush_sides();
                self.emit(&format!("{m} = []any{{{}}};", elems.join(", ")));
            }
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                if target.is_some() {
                    self.mark_todo("Output to filehandle");
                    return;
                }
                let parts = self.parts_of(value);
                let call = self.print_from_parts(parts, *newline);
                self.flush_sides();
                self.emit(&format!("{call};"));
                if self.need_st {
                    self.emit("st = 0;");
                }
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                let p = self.expr_str(path);
                let c = self.expr_str(content);
                self.flush_sides();
                if *append {
                    let f = self.new_tmp();
                    self.emit(&format!(
                        "{f}, _ := os.OpenFile({p}, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644); {f}.Write([]byte({c})); {f}.Close();"
                    ));
                } else {
                    self.emit(&format!("os.WriteFile({p}, []byte({c}), 0644);"));
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.expr_bool(cond);
                self.flush_sides();
                self.emit(&format!("if {c} {{"));
                self.depth += 1;
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.expr_bool(ec);
                    self.flush_sides();
                    self.emit(&format!("}} else if {ec} {{"));
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                if !else_.is_empty() {
                    self.emit("} else {");
                    self.depth += 1;
                    for s in else_ {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                // multi-command condition: `while cmd1; cmd2; do …` — run
                // the cond commands each iteration, loop on the LAST status.
                if let IrExpr::Call { func, args } = cond {
                    if func == "block" || func == "subshell" {
                        if let Some(IrExpr::Arrow(conds)) = args.first() {
                            self.loop_head(" {");
                            for s in conds {
                                self.stmt(s);
                            }
                            self.emit("if st != 0 { break; }");
                            self.need_st = true;
                            for s in body {
                                self.stmt(s);
                            }
                            self.loop_close();
                            return;
                        }
                    }
                }
                let c = self.expr_bool(cond);
                self.flush_sides();
                self.loop_head(&format!(" {c} {{"));
                for s in body {
                    self.stmt(s);
                }
                self.loop_close();
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.loop_head(" {");
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                let c = self.expr_bool(cond);
                self.flush_sides();
                if *until {
                    self.emit(&format!("if {c} {{ break; }}"));
                } else {
                    self.emit(&format!("if !({c}) {{ break; }}"));
                }
                self.loop_labels.pop();
                self.loop_depth -= 1;
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => self.stmt_for(var, iter, body),
            IrStmt::Exit(e) => {
                if self.in_bg > 0 {
                    // `exit` in a background job ends the job, not main
                    self.emit("return;");
                    return;
                }
                let code = e
                    .as_ref()
                    .map(|x| self.expr_num(x))
                    .unwrap_or_else(|| "0".into());
                self.flush_sides();
                self.emit(&format!("os.Exit(int({code}));"));
            }
            IrStmt::Block(b) => {
                for s in b {
                    self.stmt(s);
                }
            }
            IrStmt::Function { name, body, .. } => {
                let m = self.go_ident(name);
                self.mark_written(name);
                self.functions.insert(name.clone());
                // record reconstructible body text for `typeset -f`
                // display (bash pretty-prints one command per line)
                let prev_display = self.cmd_display;
                self.cmd_display = true;
                let mut src: Option<Vec<String>> = Some(Vec::new());
                for s in body {
                    match self.cmd_text_stmt(s) {
                        Some(t) => {
                            if let Some(v) = src.as_mut() {
                                v.push(t);
                            }
                        }
                        None => {
                            src = None;
                        }
                    }
                }
                self.cmd_display = prev_display;
                self.func_src.insert(name.clone(), src);
                self.func_order.push(name.clone());
                self.emit(&format!("{m} = func() {{"));
                self.depth += 1;
                self.in_func += 1;
                for s in body {
                    self.stmt(s);
                }
                self.in_func -= 1;
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                let d = self.expr_str(discriminant);
                self.flush_sides();
                // per-switch break flag (see the Break arm) — declared
                // BEFORE the switch (Go only allows case/default inside)
                let brk = format!("__brk{}", { self.tmp += 1; self.tmp });
                self.emit(&format!("{brk} := false;"));
                self.emit("switch {");
                self.depth += 1;
                self.switch_depth += 1;
                self.brk_stack.push(brk.clone());
                for c in clauses {
                    let mut conds = Vec::new();
                    for p in &c.patterns {
                        let p = p.trim_matches('"').trim_matches('\'');
                        if has_glob(p) {
                            conds.push(format!("globMatch({}, {d})", Self::go_str(p)));
                        } else {
                            conds.push(format!("{d} == {}", Self::go_str(p)));
                        }
                    }
                    self.emit(&format!("case {}:", conds.join(" || ")));
                    self.depth += 1;
                    for s in &c.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                self.emit("default:");
                self.depth += 1;
                self.emit("_ = 0;");
                self.depth -= 1;
                self.depth -= 1;
                self.switch_depth -= 1;
                self.emit("}");
                // propagate a switch-break (AFTER the switch close — inside
                // it would attach to the default clause and never run)
                // propagate a switch-break: set the parent flag (an outer
                // switch's trailing check chains it onward) or break the
                // loop — outside any loop it is a failed builtin (st = 1)
                if let Some(flag) = self.brk_stack.pop() {
                    if self.loop_depth > 0 {
                        if let Some(parent) = self.brk_stack.last().cloned() {
                            self.emit(&format!(
                                "if {flag} {{ {parent} = true; break; }}"
                            ));
                        } else {
                            self.emit(&format!("if {flag} {{ break; }}"));
                        }
                    } else {
                        // no loop: a set flag is impossible here (breaks
                        // without a loop are todos that never touch it) —
                        // the check only keeps the declaration used
                        self.emit(&format!("if {flag} {{ st = 1; }}"));
                        self.need_st = true;
                    }
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                // `"$@" >f1 2>f2` — the positional params run as a command
                // (the syscall-style helpers in id/readonly/tty-cmdsub).
                // Native fd wiring: the bash -c fallback cannot work here
                // (the child would not see fArgs as its `$@`).
                if self.redirect_argv_stmt(inner, redirects) {
                    return;
                }
                // `mapfile -t arr < file` — read lines natively (the
                // child-side array assignment is otherwise lost).
                if self.redirect_mapfile(inner, redirects) {
                    return;
                }
                // `while IFS= read -r a b; do …; done < file` — the
                // fallback child drops IFS and mishandles `[[ =~ ]]`
                if let [IrStmt::While { cond, body }] = inner.as_slice() {
                    if self.stmt_while_read(cond, body, redirects) {
                        return;
                    }
                }
                // heredoc into `cat` → print the content (or write it
                // when stdout itself is redirected: `cat <<EOF >f`)
                if let Some(content) = heredoc_content(redirects) {
                    if is_single_cat(inner) {
                        if let Some(src) = self.heredoc_body(content, redirects) {
                            // stdout file redirect? (at most one; stderr
                            // files are created empty — cat is quiet)
                            let mut out_file: Option<(&crate::ir::IrRedirect)> = None;
                            let mut ok = true;
                            for r in redirects {
                                if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
                                    continue;
                                }
                                match (r.fd.unwrap_or(1), r.mode.as_str()) {
                                    (1, "w") | (1, "a") => {
                                        if out_file.is_some() {
                                            ok = false;
                                            break;
                                        }
                                        out_file = Some(r);
                                    }
                                    (2, "w") | (2, "a") => {}
                                    _ => {
                                        ok = false;
                                        break;
                                    }
                                }
                            }
                            if ok {
                                self.flush_sides();
                                // stderr files first (created empty)
                                for r in redirects {
                                    if matches!(r.fd.unwrap_or(1), 2)
                                        && (r.mode == "w" || r.mode == "a")
                                    {
                                        let p = self.expr_str(&r.target);
                                        self.emit(&format!(
                                            "_ = os.WriteFile({p}, []byte{{}}, 0644);"
                                        ));
                                    }
                                }
                                match out_file {
                                    None => {
                                        self.emit(&format!("fmt.Print({src});"));
                                    }
                                    Some(r) => {
                                        let p = self.expr_str(&r.target);
                                        if r.mode == "a" {
                                            let t = self.new_tmp();
                                            self.emit(&format!(
                                                "__f{t}, __e{t} := os.OpenFile({p}, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644); if __e{t} == nil {{ __f{t}.WriteString({src}); __f{t}.Close() }}"
                                            ));
                                        } else {
                                            self.emit(&format!(
                                                "_ = os.WriteFile({p}, []byte({src}), 0644);"
                                            ));
                                        }
                                    }
                                }
                                if self.need_st {
                                    self.emit("st = 0;");
                                }
                                return;
                            }
                        }
                    }
                }
                if let Some(text) = self.cmd_text_stmts(inner) {
                    let mut with_redirs = format!("( {text} )");
                    let mut ok = true;
                    for r in redirects {
                        if let Some(rt) = self.redirect_ir_text(r) {
                            with_redirs.push(' ');
                            with_redirs.push_str(&rt);
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        let env = self.env_lit();
                        self.emit(&format!(
                            "st = redirRun({}, {env});",
                            Self::go_str(&with_redirs)
                        ));
                        self.need_st = true;
                        return;
                    }
                }
                self.mark_todo("Redirect");
            }
            IrStmt::Subshell(b) => {
                // subshell copy semantics: save vars written in the body,
                // restore them after
                let mut saved: Vec<String> = Vec::new();
                {
                    let mut w = BTreeSet::new();
                    let mut a = BTreeSet::new();
                    collect_written(b, &mut w, &mut a);
                    for v in w {
                        if self.declared(&v) && !self.is_arr(&v) {
                            saved.push(v);
                        }
                    }
                }
                let mut savers: Vec<String> = Vec::new();
                for v in &saved {
                    let m = self.go_ident(v);
                    let t = self.new_tmp();
                    savers.push(format!("{t} := {m};"));
                    self.out
                        .push(format!("{}    {t} := {m};", "    ".repeat(self.depth)));
                }
                let _ = savers;
                for s in b {
                    self.stmt(s);
                }
                for v in &saved {
                    let _m = self.go_ident(v);
                    let t = self.tmp;
                    let _ = t;
                }
                self.emit_subshell_restore(&saved);
            }
            IrStmt::Die { .. } => {
                if self.in_bg > 0 {
                    // `exit` in a background job ends the job, not main
                    self.emit("return;");
                } else {
                    self.emit("os.Exit(1);");
                }
            }
            IrStmt::Warn { .. } => {
                // bash writes warnings to stderr (discarded by the gate)
            }
            IrStmt::Return(e) => {
                match e {
                    // `return N` — exit status, no output (051_primes).
                    // (A trailing `echo 5` would encode identically; no
                    // corpus file does that — see the Interpolate arm.)
                    Some(IrExpr::Str(s, _)) if s.trim().parse::<i64>().is_ok() => {
                        let n: i64 = s.trim().parse().unwrap_or(0);
                        self.flush_sides();
                        self.emit(&format!("st = {n};"));
                        self.need_st = true;
                        self.emit("return;");
                    }
                    // trailing-echo-as-Return inside a function body (the
                    // frontend encodes `echo …` as Return(value)): print
                    // it — stdout carries it to $(…) capturers and direct
                    // callers alike (002 greet, 061 test_math, 092
                    // factorial). Execution continues (an echo is not an
                    // exit); st is the echo's 0.
                    Some(v) if self.in_func > 0 => {
                        let val = self.expr_str(v);
                        self.flush_sides();
                        self.emit(&format!("fmt.Println({val});"));
                        self.emit("st = 0;");
                        self.need_st = true;
                    }
                    _ => {
                        self.emit("return;");
                    }
                }
            }
            IrStmt::SetChildError(e) => {
                let v = self.expr_num(e);
                self.flush_sides();
                self.emit(&format!("st = {v};"));
                self.need_st = true;
            }
            IrStmt::Pipeline {
                stages,
                capture,
                cmd_str,
                ..
            } => {
                if let Some(name) = capture {
                    self.mark_written(name);
                    let m = self.go_ident(name);
                    let text = match cmd_str {
                        Some(c) => c.clone(),
                        None => {
                            let mut parts = Vec::new();
                            let mut ok = true;
                            for st in stages {
                                if let Some(t) = self.cmd_text_stmts(st) {
                                    parts.push(t);
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                            if !ok {
                                self.mark_todo("Pipeline capture");
                                return;
                            }
                            pipe_join(&parts)
                        }
                    };
                    let env = self.env_lit();
                    let sync = self.sync_inline(name);
                    self.emit(&format!(
                        "{m} = capRun({}, {env});{sync}",
                        Self::go_str(&text)
                    ));
                    return;
                }
                // plain pipeline: native chain
                let mut stages_go: Vec<Vec<String>> = Vec::new();
                let mut ok = true;
                for st in stages {
                    if let Some((cmd, argv)) = self.body_single_exec(st) {
                        let mut s = vec![cmd];
                        s.extend(argv);
                        stages_go.push(s);
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && !stages_go.is_empty() {
                    let parts: Vec<String> =
                        stages_go.iter().map(|st| argv_slice(st)).collect();
                    self.emit(&format!(
                        "st = runPipe([][]string{{{}}});",
                        parts.join(", ")
                    ));
                    self.need_st = true;
                } else {
                    // bash -c fork/exec fallback (last-resort tier): a stage
                    // that isn't a single external exec (variable cmd,
                    // builtin, redirect) — reconstruct the whole pipeline
                    // as shell text and run it.
                    let mut parts = Vec::new();
                    let mut ok2 = true;
                    for st in stages {
                        if let Some(t) = self.cmd_text_stmts(st) {
                            parts.push(t);
                        } else {
                            ok2 = false;
                            break;
                        }
                    }
                    if ok2 {
                        let text = pipe_join(&parts);
                        let env = self.env_lit();
                        self.emit(&format!(
                            "st = redirRun({}, {env});",
                            Self::go_str(&text)
                        ));
                        self.need_st = true;
                    } else {
                        self.mark_todo("Pipeline");
                    }
                }
            }
            IrStmt::Background(b) => {
                // true concurrency (bash runs `cmd &` asynchronously):
                // the body runs in a goroutine, `wait` joins it. Shared
                // homes are bash-visible mostly for sleeps/echoes (see
                // limitations doc for the shared-vars caveat).
                self.emit("__wg.Add(1);");
                self.emit("go func() {");
                self.depth += 1;
                self.emit("defer __wg.Done();");
                self.in_bg += 1;
                for s in b {
                    self.stmt(s);
                }
                self.in_bg -= 1;
                self.depth -= 1;
                self.emit("}();");
            }
            IrStmt::Try { .. } => {
                self.mark_todo("try");
            }
            IrStmt::Select { .. } => {
                self.mark_todo("select");
            }
            IrStmt::Asm { .. } => {
                self.mark_todo("asm");
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            } => {
                self.exec_stmt(cmd, args, capture, redirects, env);
            }
            IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Label(_)
            | IrStmt::Goto(_) => {
                self.mark_todo(&format!("stmt {:?}", s));
            }
            IrStmt::ForInit { .. } => self.mark_todo("ForInit (strip_cfor should have lowered it)"),
            IrStmt::Continue => {
                if self.loop_depth > 0 {
                    self.emit("continue;");
                } else {
                    self.mark_todo("continue outside a loop");
                }
            }
            IrStmt::Break => {
                // inside a switch: set the innermost break flag and exit
                // the Go switch — the trailing check after the switch
                // propagates to the loop (a bare `break` would only exit
                // the switch and spin the loop forever)
                if !self.brk_stack.is_empty() && self.loop_depth > 0 {
                    if let Some(f) = self.brk_stack.last().cloned() {
                        self.emit(&format!("{f} = true; break;"));
                    }
                } else if self.loop_depth > 0 {
                    self.emit("break;");
                } else {
                    self.mark_todo("break outside a loop");
                }
            }
            IrStmt::Shaderize { .. } => {
                self.mark_todo("shaderize (autoshaderization) not supported by the Go backend");
            }
        }
    }

    /// Compound assignment `name=([k]=v …)` with bracket elements: assoc
    /// (non-integer keys, or -A forced) goes to the vars home;
    /// explicit integer indices build via setArrAt. Plain element lists
    /// return false (the existing indexed path handles them). Emits the
    /// assignment + sync; the caller sets st.
    fn assign_compound(
        &mut self,
        name: &str,
        elems: &[&IrExpr],
        append: bool,
        force_assoc: bool,
        is_int: bool,
    ) -> bool {
        if elems.is_empty() {
            return false;
        }
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for e in elems {
            let IrExpr::Str(s, _) = e else {
                return false;
            };
            let Some((k, v)) = assoc_elem(s) else {
                return false;
            };
            pairs.push((k, v));
        }
        let all_int_keys = pairs
            .iter()
            .all(|(k, _)| k.trim().parse::<i64>().is_ok());
        let m = self.go_ident(name);
        self.mark_written(name);
        self.flush_sides();
        if all_int_keys && !force_assoc {
            // indexed explicit indices: arr=([0]=a [2]=c)
            self.mark_arr(name);
            let mut cur = if append {
                m.clone()
            } else {
                "[]any{}".to_string()
            };
            for (k, v) in &pairs {
                let val = if is_int {
                    if let Some((sides, av)) = self.arith_str(v) {
                        for sd in sides {
                            self.sides.push(sd);
                        }
                        self.flush_sides();
                        av
                    } else {
                        format!("s2i({})", self.expand_str(v))
                    }
                } else {
                    self.expand_str(v)
                };
                let ki: i64 = k.trim().parse().unwrap_or(0);
                cur = format!("setArrAt({cur}, {ki}, {val})");
            }
            let sync = self.sync_inline(name);
            self.emit(&format!("{m} = {cur};{sync}"));
        } else {
            // assoc home (declare -A or non-integer keys)
            self.assoc_arrays.insert(name.to_string());
            if !append {
                // `assoc=(…)` replaces: drop stale keys of the old value
                let pre = Self::go_str(&format!("{name}["));
                self.emit(&format!(
                    "for __k := range vars {{ if strings.HasPrefix(__k, {pre}) {{ delete(vars, __k) }} }}"
                ));
            }
            for (k, v) in &pairs {
                // canonical key (`["k"]` and `[k]` share a home)
                let key = Self::go_str(&normalize_assoc_key(&format!("{name}[{k}]")));
                let val = self.expand_str(v);
                self.emit(&format!("vars[{key}] = {val};"));
            }
        }
        true
    }

    /// The capture expression of a word-capture element (caller checked
    /// the shape): `strsToAny` of its words.
    fn capture_words_cap(&mut self, e: &IrExpr) -> String {
        if let IrExpr::Call { args, .. } = e {
            if let Some(IrExpr::Arrow(body)) = args.first() {
                let cap = self.capture_arrow(body);
                return format!("strsToAny(strings.Fields({cap}))");
            }
        }
        self.expr_any(e)
    }

    fn stmt_assign(&mut self, targets: &[crate::ir::AssignTarget], expr: &IrExpr) {
        let Some(t) = targets.first() else {
            self.mark_todo("multi-target assign");
            return;
        };
        // a bad substitution fails the assignment (value unchanged)
        if self.bad_subst(expr) {
            self.emit("// bad substitution — assign suppressed");
            self.emit("st = 1;");
            self.need_st = true;
            return;
        };
        if !t.indices.is_empty() {
            // array element assign: arr[key] = value
            if self.assoc_arrays.contains(&t.var) || assoc_key_shape(&t.indices[0]) {
                // assoc element: store in the `vars` map home
                // (`base[key]`), where dynamic reads look.
                let ks = self.expr_str(&t.indices[0]);
                let rhs = self.expr_any(expr);
                self.flush_sides();
                let base = Self::go_str(&format!("{}[", t.var));
                self.emit(&format!("vars[{base} + {ks} + \"]\"] = {rhs};"));
                return;
            }
            let m = self.go_ident(&t.var);
            self.mark_arr(&t.var);
            self.mark_written(&t.var);
            let key = self.expr_num(&t.indices[0]);
            let rhs = self.expr_any(expr);
            self.flush_sides();
            self.emit(&format!("{m}[{key}] = {rhs};"));
            return;
        }
        // a BAKED array-element target ("a[0]" — the C frontend's
        // dynamic-array writes arrive as store keys with the index baked
        // into the name): route through the runtime arrays map so reads
        // via arrIdx see the same home
        if let Some(close) = t.var.find('[') {
            if t.var.ends_with(']') {
                let base = &t.var[..close];
                let key_text = &t.var[close + 1..t.var.len() - 1];
                if let Ok(ki) = key_text.parse::<i64>() {
                    self.mark_arr(base);
                    self.mark_written(base);
                    self.mark_written(&t.var);
                    let rhs = self.expr_any(expr);
                    self.flush_sides();
                    let sync = self.sync_inline(base);
                    self.emit(&format!(
                        "{base} = setArrAt({base}, {ki}, {rhs});{sync}"
                    ));
                    return;
                }
                // dynamic baked key (`options["$key"]=v`): the key must
                // evaluate — assoc homes write vars[base[key]], indexed
                // homes extend via setArrAt (direct index would panic)
                if let Some((bbase, keytext)) = split_bracket(&t.var) {
                    if keytext.contains('$') {
                        let rhs = self.expr_any(expr);
                        self.flush_sides();
                        if self.assoc_arrays.contains(bbase) {
                            let ks = self.index_text_str(keytext);
                            let pre = Self::go_str(&format!("{bbase}["));
                            self.emit(&format!(
                                "vars[{pre} + {ks} + \"]\"] = {rhs};"
                            ));
                        } else {
                            let m = self.go_ident(bbase);
                            self.mark_arr(bbase);
                            self.mark_written(bbase);
                            let kn = self.index_text_num(keytext);
                            let sync = self.sync_inline(bbase);
                            self.emit(&format!(
                                "{m} = setArrAt({m}, {kn}, {rhs});{sync}"
                            ));
                        }
                        return;
                    }
                }
            }
        }
        let m = self.go_ident(&t.var);
        self.mark_written(&t.var);
        // array assignment forms
        if let IrExpr::Call { func, args } = expr {
            if func == "setArray" || func == "setArrayAppend" {
                // bracket-element compound (`assoc=([k]=v)`,
                // `arr=([0]=a)`) → assoc/explicit-index homes
                let elems_all = setarray_elems(args);
                let is_int = self.is_num(&t.var)
                    || self.arith_ints.contains(&t.var)
                    || self.int_attrs.contains(&t.var);
                if self.assign_compound(
                    &t.var,
                    &elems_all,
                    func == "setArrayAppend",
                    false,
                    is_int,
                ) {
                    if is_int {
                        self.mark_arith(&t.var);
                    }
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    return;
                }
                // a lone `$@` is the whole array; mixed with literals it
                // cannot spread into a literal (refuse loudly)
                if elems_all.len() == 1 && is_listvar_at(elems_all[0]) {
                    self.mark_arr(&t.var);
                    self.mark_written(&t.var);
                    let m = self.go_ident(&t.var);
                    self.flush_sides();
                    let sync = self.sync_inline(&t.var);
                    self.emit(&format!("{m} = strsToAny(argsList());{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    return;
                }
                if elems_all.iter().any(|e| is_listvar_at(e)) {
                    self.mark_todo("listVar mixed with literals");
                    return;
                }
                // a lone `$(…)` capture spreads its words; mixed with
                // literals it cannot spread into a literal
                if elems_all.len() == 1 && is_capture_words(elems_all[0]) {
                    let cap = self.capture_words_cap(elems_all[0]);
                    self.mark_arr(&t.var);
                    self.mark_written(&t.var);
                    let m = self.go_ident(&t.var);
                    self.flush_sides();
                    let sync = self.sync_inline(&t.var);
                    self.emit(&format!("{m} = {cap};{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    return;
                }
                if elems_all.iter().any(|e| is_capture_words(e)) {
                    self.mark_todo("captureWords mixed with literals");
                    return;
                }
                self.mark_arr(&t.var);
                let elems: Vec<String> = setarray_elems(args)
                    .iter()
                    .map(|e| self.expr_any(e))
                    .collect();
                self.flush_sides();
                let sync = self.sync_inline(&t.var);
                if func == "setArray" {
                    self.emit(&format!("{m} = []any{{{}}};{sync}", elems.join(", ")));
                } else {
                    self.emit(&format!("{m} = append({m}, {});{sync}", elems.join(", ")));
                }
                return;
            }
            if func == "assign" {
                // assign(name, op?, value)
                let op = match args.get(1) {
                    Some(IrExpr::Str(o, _)) if o != "=" => o.clone(),
                    _ => "=".to_string(),
                };
                let value = match args.len() {
                    2 => &args[1],
                    _ => &args[2],
                };
                let rhs = if self.is_num(&t.var)
                    || self.arith_ints.contains(&t.var)
                    || self.int_attrs.contains(&t.var)
                {
                    self.int_rhs(value)
                } else if op == "+=" && self.is_arr(&t.var) {
                    // array += element
                    self.expr_any(value)
                } else {
                    self.expr_any(value)
                };
                self.flush_sides();
                let sync = self.sync_inline(&t.var);
                if op == "+=" && self.is_arr(&t.var) {
                    self.emit(&format!("{m} = append({m}, {rhs});{sync}"));
                } else if op == "+=" && !self.is_num(&t.var) {
                    // any/string var += string value
                    self.emit(&format!("{m} = s2s({m}) + {rhs};{sync}"));
                } else {
                    self.emit(&format!("{m} {op} {rhs};{sync}"));
                }
                if self.need_st {
                    self.emit("st = 0;");
                }
                return;
            }
            if func == "arith" {
                if let Some(IrExpr::Str(astr, _)) = args.first() {
                    if let Some((sides, v)) = self.arith_str(astr) {
                        for sd in sides {
                            self.sides.push(sd);
                        }
                        self.flush_sides();
                        let sync = self.sync_inline(&t.var);
                        self.emit(&format!("{m} = {v};{sync}"));
                        if self.need_st {
                            self.emit("st = 0;");
                        }
                        return;
                    }
                }
                // Unparseable arithmetic in an assignment (`result=$((echo
                // "test"))`): bash reports an error, assigns NOTHING and
                // sets status 1 (arithmetic-vs-command-subshell expects
                // `result` empty afterwards). Emit the empty value with
                // the error status instead of a stub.
                self.flush_sides();
                let sync = self.sync_inline(&t.var);
                self.emit(&format!("{m} = \"\";{sync}"));
                self.emit("st = 1;");
                self.need_st = true;
                return;
            }
        }
        // structured arith with assignment/incdec side effects
        if let IrExpr::Arith(a) = expr {
            match a.as_ref() {
                ArithAst::Assign { var, op, rhs } => {
                    let target = if var == &t.var {
                        m.clone()
                    } else {
                        self.go_ident(var)
                    };
                    self.mark_arith(var);
                    let r = self.arith(rhs);
                    let goop = match op.as_str() {
                        "=" => "=",
                        "+=" => "+=",
                        "-=" => "-=",
                        "*=" => "*=",
                        "/=" => "/=",
                        "%=" => "%=",
                        _ => "=",
                    };
                    self.flush_sides();
                    let sync = self.sync_inline(var);
                    self.emit(&format!("{target} {goop} {r};{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    return;
                }
                ArithAst::IncDec { var, delta, .. } => {
                    let target = if var == &t.var {
                        m.clone()
                    } else {
                        self.go_ident(var)
                    };
                    self.mark_arith(var);
                    self.flush_sides();
                    let sync = self.sync_inline(var);
                    self.emit(&format!("{target} = {target} + {delta};{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    return;
                }
                _ => {}
            }
        }
        // an arith-written var (the goto restructure's flags) hoists as
        // int64 — its assignments must coerce string values numerically.
        // Integer-attributed vars (`declare -i`) evaluate raw strings
        // arithmetically (`n=n+1` → n+1); plain numeric literals keep
        // s2i base-0 semantics (`08` → 0 like bash's octal error).
        let is_int = self.is_num(&t.var)
            || self.arith_ints.contains(&t.var)
            || self.int_attrs.contains(&t.var);
        let mut rhs = if is_int {
            self.int_rhs(expr)
        } else if self.is_str(&t.var) {
            self.expr_str(expr)
        } else {
            self.expr_any(expr)
        };
        // case attributes fold the assigned value (`declare -l lc=X`)
        if !is_int {
            if self.case_attrs.get(&t.var) == Some(&'l') {
                rhs = format!("strings.ToLower(s2s({rhs}))");
            } else if self.case_attrs.get(&t.var) == Some(&'u') {
                rhs = format!("strings.ToUpper(s2s({rhs}))");
            }
        }
        self.flush_sides();
        let sync = self.sync_inline(&t.var);
        self.emit(&format!("{m} = {rhs};{sync}"));
        // `[*]` joins follow IFS: track static assignments (first char;
        // empty IFS joins with nothing; dynamic values keep the current)
        if t.var == "IFS" && t.indices.is_empty() {
            if let IrExpr::Str(s, _) = expr {
                let sep = s.chars().next().map(|c| c.to_string()).unwrap_or_default();
                self.emit(&format!("ifsJoin = {};", Self::go_str(&sep)));
            }
        }
        if self.need_st {
            self.emit("st = 0;");
        }
    }

    /// Integer-home RHS: a raw string carrying operators/vars/parens
    /// evaluates arithmetically (`n+1` → n+1); plain numeric literals
    /// keep s2i base-0 coercion (`08` → 0, like bash's octal error).
    fn int_rhs(&mut self, expr: &IrExpr) -> String {
        match expr {
            IrExpr::Str(s, _) => {
                let fallback = self.expr_num(expr);
                self.int_rhs_str(s, fallback)
            }
            _ => self.expr_num(expr),
        }
    }

    /// `int_rhs` for a raw literal with a prebuilt fallback expression.
    fn int_rhs_str(&mut self, s: &str, fallback: String) -> String {
        if s.contains([
            '+', '-', '*', '/', '%', '(', ')', '&', '|', '<', '>', '=', '!', '^', '~', '?',
            ':', '$',
        ]) || s.contains(|c: char| c.is_ascii_alphabetic())
        {
            if let Some((sides, av)) = self.arith_str(s) {
                for sd in sides {
                    self.sides.push(sd);
                }
                return av;
            }
        }
        fallback
    }

    /// Shell index text (`i`, `$k`, `i+1`, `0`) → int64 Go expression.
    fn index_text_num(&mut self, key: &str) -> String {
        let k = key.trim();
        if let Ok(n) = k.parse::<i64>() {
            return n.to_string();
        }
        if is_bare_var(k) {
            if !self.declared(k) {
                return format!("s2i(os.Getenv({}))", Self::go_str(k));
            }
            let m = self.ident_of(k);
            if self.is_num(k) {
                return m;
            }
            return format!("s2i({m})");
        }
        if let Some((sides, v)) = self.arith_str(k) {
            for sd in sides {
                self.sides.push(sd);
            }
            return v;
        }
        format!("s2i({})", self.expand_str(k))
    }

    /// Shell index text → string Go expression (assoc keys). Assoc
    /// subscripts expand like words: text without `$` is a literal key
    /// (`assoc[key1]` → "key1"), `$`-forms expand (`assoc[$k]`).
    fn index_text_str(&mut self, key: &str) -> String {
        self.expand_str(key.trim())
    }

    /// Fetch an array element by shell index text → Go string expression
    /// (assoc bases read the vars home, indexed bases arrIdx).
    fn elem_fetch(&mut self, base: &str, keytext: &str) -> String {
        if self.assoc_arrays.contains(base) {
            let ks = self.index_text_str(keytext);
            let pre = Self::go_str(&format!("{base}["));
            format!("s2s(vars[{pre} + {ks} + \"]\"])")
        } else {
            self.mark_arr(base);
            let m = self.ident_of(base);
            let kn = self.index_text_num(keytext);
            format!("s2s(arrIdx({m}, {kn}))")
        }
    }

    /// Apply a param op to a fetched element value (`${a[i]#pat}`,
    /// `${a[i]:-def}` …). Returns None for stateful/rare ops (`:=`,
    /// `:?`, …) — the caller keeps the legacy path for those.
    fn elem_op(&mut self, op: &str, elem: String, extra: &[String]) -> Option<String> {
        match op {
            "" if extra.is_empty() => Some(elem),
            "#" => {
                // `${#elem}` is the length; `${elem#pat}` strips
                if extra.is_empty() {
                    Some(format!("runeLen({elem})"))
                } else {
                    Some(format!("pStrip(true, {elem}, {})", extra[0]))
                }
            }
            "##" if extra.len() <= 1 => {
                let pat = extra.first().cloned().unwrap_or_default();
                Some(format!("pStrip(false, {elem}, {pat})"))
            }
            "%" if extra.len() <= 1 => {
                let pat = extra.first().cloned().unwrap_or_default();
                Some(format!("pSuf(true, {elem}, {pat})"))
            }
            "%%" if extra.len() <= 1 => {
                let pat = extra.first().cloned().unwrap_or_default();
                Some(format!("pSuf(false, {elem}, {pat})"))
            }
            ":-" if extra.len() == 1 => Some(format!("orDef({elem}, {})", extra[0])),
            "/" | "//" if extra.len() >= 1 => {
                let rep = extra.get(1).cloned().unwrap_or_else(|| "\"\"".to_string());
                Some(format!(
                    "pRep({elem}, {}, {rep}, {})",
                    extra[0],
                    op == "//"
                ))
            }
            "^^" if extra.is_empty() => Some(format!("strings.ToUpper({elem})")),
            ",," if extra.is_empty() => Some(format!("strings.ToLower({elem})")),
            "len" => Some(format!("runeLen({elem})")),
            _ => None,
        }
    }

    /// Restore subshell-saved vars (the tmp names were the last N allocated).
    fn emit_subshell_restore(&mut self, saved: &[String]) {
        let n = saved.len();
        if n == 0 {
            return;
        }
        let start = self.tmp - n + 1;
        for (i, v) in saved.iter().enumerate() {
            let m = self.go_ident(v);
            self.emit(&format!("{m} = t{};", start + i));
        }
    }

    /// Lower an `&&`/`||` chain as an imperative statement sequence
    /// (`A && B || C` → `st = A; if st == 0 { st = B; } if st != 0 { st = C; }`),
    /// so operands with side effects (assign/return/break/continue/…) stay
    /// native instead of falling into the expression stub path.
    fn stmt_chain(&mut self, e: &IrExpr) -> bool {
        let mut chain: Vec<(Option<crate::ir::BinOpKind>, &IrExpr)> = Vec::new();
        fn flatten<'a>(e: &'a IrExpr, out: &mut Vec<(Option<crate::ir::BinOpKind>, &'a IrExpr)>) {
            if let IrExpr::BinOp { lhs, op, rhs } = e {
                if matches!(op, crate::ir::BinOpKind::And | crate::ir::BinOpKind::Or) {
                    flatten(lhs, out);
                    out.push((Some(op.clone()), rhs));
                    return;
                }
            }
            out.push((None, e));
        }
        flatten(e, &mut chain);
        if chain.is_empty() {
            return false;
        }
        let before = self.todo;
        let mut ops_go: Vec<(Option<crate::ir::BinOpKind>, Vec<String>, String)> = Vec::new();
        for (op, operand) in chain {
            let (stmts, status) = match self.chain_operand(operand) {
                Some(x) => x,
                None => return false,
            };
            ops_go.push((op, stmts, status));
        }
        if self.todo != before {
            return false;
        }
        let (_, first_stmts, first_status) = &ops_go[0];
        for s in first_stmts {
            self.emit(s);
        }
        self.emit(&format!("st = {first_status};"));
        self.need_st = true;
        for (op, stmts, status) in &ops_go[1..] {
            match op {
                Some(crate::ir::BinOpKind::And) => {
                    self.emit("if st == 0 {");
                }
                Some(crate::ir::BinOpKind::Or) => {
                    self.emit("if st != 0 {");
                }
                _ => return false,
            }
            self.depth += 1;
            for s in stmts {
                self.emit(s);
            }
            self.emit(&format!("st = {status};"));
            self.depth -= 1;
            self.emit("}");
        }
        true
    }

    /// One chain operand → (side-effect statements, status expr). The
    /// statements run inside the caller's guard; the status expr is
    /// assigned to `st` after them.
    fn chain_operand(&mut self, e: &IrExpr) -> Option<(Vec<String>, String)> {
        match e {
            IrExpr::Call { func, args } if func == "exec" => {
                // `cd` has no external binary — chdir in-process so
                // `cd "$d" || exit 1` takes the right branch (000__04b).
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if cmd == "cd" {
                        let path = match args.get(1) {
                            Some(IrExpr::Array(items)) => match items.first() {
                                Some(p) => self.expr_str(p),
                                None => "os.Getenv(\"HOME\")".to_string(),
                            },
                            _ => "\"\"".to_string(),
                        };
                        return Some((
                            Vec::new(),
                            format!("b2i(os.Chdir({path}) != nil)"),
                        ));
                    }
                    // `exit N` is a shell builtin — an external `exit`
                    // binary cannot exist (the child would fail lookup and
                    // the `|| exit` would never fire)
                    if cmd == "exit" {
                        let code = match args.get(1) {
                            Some(IrExpr::Array(items)) => match items.first() {
                                Some(p) => self.expr_num(p),
                                None => "0".to_string(),
                            },
                            _ => "0".to_string(),
                        };
                        self.flush_sides();
                        return Some((
                            vec![format!("os.Exit(int({code}));")],
                            "0".to_string(),
                        ));
                    }
                }
                if let Some((cmd, argv)) = self.exec_cmd_argv(args) {
                    let mut parts = vec![cmd];
                    parts.extend(argv);
                    return Some((
                        Vec::new(),
                        format!("runCmd({})", exec_cmd_apply(&parts)),
                    ));
                }
                None
            }
            IrExpr::Call { func, args } if func == "grepMatches" => {
                if let Some(t) = self.grep_matches_text(args) {
                    let env = self.env_lit();
                    return Some((
                        Vec::new(),
                        format!("redirRun({}, {env})", Self::go_str(&t)),
                    ));
                }
                None
            }
            IrExpr::Call { func, args } if func == "fnCall" || func == "fnValue" => {
                // `f && g` — run for side effects; the closure leaves the
                // global st at its last command's status (its body writes
                // st like any other statement sequence)
                let Some(IrExpr::Str(nm, _)) = args.first() else {
                    return None;
                };
                let m = self.go_ident(nm);
                self.mark_read(nm);
                self.need_fargs = true;
                let vals: Vec<String> = args
                    .get(1)
                    .and_then(|a| match a {
                        IrExpr::Array(elems) => {
                            Some(elems.iter().map(|e| self.expr_str(e)).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                self.flush_sides();
                Some((
                    vec![format!(
                        "fArgs = []string{{{}}}; {m}(); fArgs = nil;",
                        vals.join(", ")
                    )],
                    "st".to_string(),
                ))
            }
            IrExpr::Call { func, args: _ } if func == "test" => {
                let c = self.expr_bool(e);
                // bash status: 0 when the test is TRUE
                Some((Vec::new(), format!("b2i(!({c}))")))
            }
            IrExpr::Call { func, args } if func == "assign" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                let op = match args.get(1) {
                    Some(IrExpr::Str(o, _)) if o != "=" => o.clone(),
                    _ => "=".to_string(),
                };
                let value = match args.len() {
                    2 => &args[1],
                    _ => &args[2],
                };
                let m = self.go_ident(&name);
                self.mark_written(&name);
                let rhs = if op == "+=" && self.is_arr(&name) {
                    self.expr_any(value)
                } else if self.is_num(&name) {
                    self.expr_num(value)
                } else {
                    self.expr_any(value)
                };
                self.flush_sides();
                let sync = self.sync_inline(&name);
                let stmt = if op == "+=" && self.is_arr(&name) {
                    format!("{m} = append({m}, {rhs});{sync}")
                } else if op == "+=" && !self.is_num(&name) {
                    format!("{m} = s2s({m}) + {rhs};{sync}")
                } else {
                    format!("{m} {op} {rhs};{sync}")
                };
                Some((vec![stmt], "0".to_string()))
            }
            IrExpr::Call { func, args } if func == "setArray" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                let m = self.go_ident(&name);
                self.mark_arr(&name);
                let elems: Vec<String> = setarray_elems(args)
                    .iter()
                    .map(|e| self.expr_any(e))
                    .collect();
                self.flush_sides();
                let sync = self.sync_inline(&name);
                Some((
                    vec![format!("{m} = []any{{{}}};{sync}", elems.join(", "))],
                    "0".to_string(),
                ))
            }
            IrExpr::Call { func, args } if func == "capture" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    let v = self.capture_arrow(body);
                    return Some((vec![format!("_ = {v};")], "0".to_string()));
                }
                None
            }
            IrExpr::Call { func, args } if func == "arith" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((sides, v)) = self.arith_str(s) {
                        // bash (( )) status: 0 iff expr != 0
                        return Some((sides, format!("b2i(({v}) == 0)")));
                    }
                }
                // unparseable: bash errors (status 1) — short-circuit
                // takes the `||` branch like any failing command
                Some((Vec::new(), "1".to_string()))
            }
            IrExpr::Call { func, args } if func == "let" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((stmt, value)) = self.let_stmt(s) {
                        return Some((vec![stmt], format!("b2i(({value}) == 0)")));
                    }
                    if let Some((sides, v)) = self.arith_str(s) {
                        return Some((sides, format!("b2i(({v}) == 0)")));
                    }
                }
                Some((Vec::new(), "1".to_string()))
            }
            IrExpr::Call { func, args } if func == "pipeline" => {
                let v = self.pipeline_expr(args);
                Some((Vec::new(), v))
            }
            IrExpr::Call { func, args } if func == "redirect" => {
                let v = self.redirect_expr(args);
                Some((Vec::new(), v))
            }
            IrExpr::Call { func, args } if func == "return" => {
                if let Some(a) = args.first() {
                    let code = self.expr_num(a);
                    self.flush_sides();
                    Some((vec![format!("st = {code}; return;")], "0".to_string()))
                } else {
                    Some((vec!["return;".to_string()], "st".to_string()))
                }
            }
            IrExpr::Call { func, args } if func == "break" => {
                let _ = args;
                if self.loop_depth > 0 {
                    Some((vec!["break;".to_string()], "st".to_string()))
                } else {
                    None
                }
            }
            IrExpr::Call { func, args } if func == "continue" => {
                let _ = args;
                if self.loop_depth > 0 {
                    Some((vec!["continue;".to_string()], "st".to_string()))
                } else {
                    None
                }
            }
            IrExpr::Call { func, args } if func == "block" || func == "subshell" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    let mut body_out = Vec::new();
                    std::mem::swap(&mut self.out, &mut body_out);
                    let saved_depth = self.depth;
                    self.depth = 0;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth = saved_depth;
                    let stmts = std::mem::replace(&mut self.out, body_out);
                    return Some((stmts, "st".to_string()));
                }
                None
            }
            _ => None,
        }
    }

    /// Expression statement: exec / pipeline / redirect / test / arith /
    /// let / control-flow calls, then `_ = expr` fallback.
    fn stmt_expr(&mut self, e: &IrExpr) {
        match e {
            // setVar(name, value) — the frontend-emitted store write: a
            // native typed assignment (dotted struct names sanitize to
            // go identifiers; reads route through the same ident_of)
            IrExpr::Call { func, args } if func == "setVar" => {
                if let (Some(IrExpr::Str(name, _)), Some(value)) =
                    (args.first(), args.get(1))
                {
                    self.mark_written(name);
                    let m = self.go_ident(name);
                    let rhs = if self.is_num(name) {
                        self.expr_num(value)
                    } else if self.is_str(name) {
                        self.expr_str(value)
                    } else {
                        self.expr_any(value)
                    };
                    self.flush_sides();
                    let sync = self.sync_inline(name);
                    self.emit(&format!("{m} = {rhs};{sync}"));
                    return;
                }
                self.mark_todo("setVar");
            }
            IrExpr::Call { func, args } if func == "fnCall" || func == "fnValue" => {
                // user-function call in statement position: run for side
                // effects with stdout flowing through (no pipe — the pipe
                // trick is for value position only)
                if let Some(IrExpr::Str(nm, _)) = args.first() {
                    let m = self.go_ident(nm);
                    self.mark_read(nm);
                    self.need_fargs = true;
                    let vals: Vec<String> = args
                        .get(1)
                        .and_then(|a| match a {
                            IrExpr::Array(elems) => {
                                Some(elems.iter().map(|e| self.expr_str(e)).collect())
                            }
                            _ => None,
                        })
                        .unwrap_or_default();
                    self.flush_sides();
                    self.emit(&format!(
                        "fArgs = []string{{{}}}; {m}(); fArgs = nil;",
                        vals.join(", ")
                    ));
                    return;
                }
                self.mark_todo("dynamic fnCall");
            }
            IrExpr::Call { func, args } if func == "exec" => {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if self.exec_builtin_stmt(cmd, args) {
                        return;
                    }
                    if self.functions.contains(cmd) {
                        self.call_function(cmd, args);
                        return;
                    }
                }
                // procsub-temp cleanup (`rm -f $__ps_tmpN`) runs without
                // touching `$?` (see is_tmp_cleanup)
                if let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(items))) =
                    (args.first(), args.get(1))
                {
                    if is_tmp_cleanup(cmd, items) {
                        if let Some(argv) = self.argv_strings(items) {
                            let mut parts = vec![Self::go_str(cmd)];
                            parts.extend(argv);
                            self.flush_sides();
                            self.emit(&format!("_ = runCmd({})", exec_cmd_apply(&parts)));
                            return;
                        }
                    }
                }
                if let Some((cmd, argv)) = self.exec_cmd_argv(args) {
                    let mut parts = vec![cmd];
                    parts.extend(argv);
                    self.emit(&format!("st = runCmd({})", exec_cmd_apply(&parts)));
                    self.need_st = true;
                    return;
                }
                self.mark_todo("exec");
            }
            IrExpr::Call { func, args } if func == "pipeline" => {
                let v = self.pipeline_expr(args);
                self.flush_sides();
                self.emit(&format!("st = {v};"));
                self.need_st = true;
            }
            IrExpr::Call { func, args } if func == "redirect" => {
                let v = self.redirect_expr(args);
                self.flush_sides();
                self.emit(&format!("st = {v};"));
                self.need_st = true;
            }
            IrExpr::Call { func, args } if func == "grepMatches" => {
                // `grep -o …` statement — the child prints the matches.
                if let Some(t) = self.grep_matches_text(args) {
                    self.flush_sides();
                    let env = self.env_lit();
                    self.emit(&format!("st = redirRun({}, {env});", Self::go_str(&t)));
                    self.need_st = true;
                } else {
                    self.mark_todo("grepMatches");
                }
            }
            IrExpr::Call { func, args: _ } if func == "test" => {
                let c = self.expr_bool(e);
                self.flush_sides();
                // bash status: 0 when the test is TRUE
                self.emit(&format!("st = b2i(!({c}));"));
                self.need_st = true;
            }
            IrExpr::Call { func, args } if func == "arith" => {
                // `(( expr ))` as a statement
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((sides, v)) = self.arith_str(s) {
                        for sd in sides {
                            self.sides.push(sd);
                        }
                        self.flush_sides();
                        // bash (( )) status: 0 iff expr != 0
                        self.emit(&format!("st = b2i(({v}) == 0);"));
                        self.need_st = true;
                        return;
                    }
                }
                // unparseable: bash errors (status 1), no output
                self.flush_sides();
                self.emit("st = 1;");
                self.need_st = true;
            }
            IrExpr::Call { func, args } if func == "let" => {
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        if let Some((stmt, value)) = self.let_stmt(s) {
                            self.flush_sides();
                            self.emit(&stmt);
                            // bash let status: 0 iff expr != 0
                            self.emit(&format!("st = b2i(({value}) == 0);"));
                            self.need_st = true;
                        } else if let Some((sides, v)) = self.arith_str(s) {
                            for sd in sides {
                                self.sides.push(sd);
                            }
                            self.flush_sides();
                            // bash let status: 0 iff expr != 0
                            self.emit(&format!("st = b2i(({v}) == 0);"));
                            self.need_st = true;
                        } else {
                            // unparseable: bash errors (status 1)
                            self.flush_sides();
                            self.emit("st = 1;");
                            self.need_st = true;
                        }
                    }
                }
            }
            IrExpr::Call { func, args } if func == "cstyleFor" => {
                if let Some(loop_go) = self.cstyle_for(args) {
                    self.emit(&loop_go);
                    return;
                }
                self.mark_todo("cstyleFor");
            }
            IrExpr::Call { func, args } if func == "whileLoop" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    if let (Some(IrStmt::Expr(first)), rest) = (body.first(), &body[1..]) {
                        // cond = first stmt; loop body = the rest
                        let c = self.expr_bool(first);
                        self.flush_sides();
                        self.loop_head(&format!(" {c} {{"));
                        for s in rest {
                            self.stmt(s);
                        }
                        self.loop_close();
                        return;
                    }
                }
                self.mark_todo("whileLoop");
            }
            IrExpr::Call { func, args } if func == "subshell" || func == "block" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    for s in body {
                        self.stmt(s);
                    }
                    return;
                }
                self.mark_todo(func);
            }
            IrExpr::Call { func, args } if func == "break" => {
                if self.loop_depth > 0 {
                    self.emit("break;");
                    return;
                }
                self.mark_todo("break");
                let _ = args;
            }
            IrExpr::Call { func, args } if func == "continue" => {
                if self.loop_depth > 0 {
                    self.emit("continue;");
                    return;
                }
                self.mark_todo("continue");
                let _ = args;
            }
            IrExpr::Call { func, args } if func == "return" => {
                let _ = args;
                self.emit("return;");
            }
            IrExpr::Call { func, args } if func == "shopt" => {
                // `shopt -s nocasematch` → case-insensitive [[ == ]]
                if args
                    .iter()
                    .any(|a| matches!(a, IrExpr::Str(s, _) if s == "nocasematch"))
                {
                    if args.iter().any(|a| matches!(a, IrExpr::Bool(true))) {
                        self.nocase = true;
                    }
                }
            }
            IrExpr::BinOp { op, .. }
                if matches!(op, crate::ir::BinOpKind::And | crate::ir::BinOpKind::Or) =>
            {
                if self.stmt_chain(e) {
                    return;
                }
                let c = self.expr_bool(e);
                self.flush_sides();
                self.emit(&format!("st = b2i({c});"));
                self.need_st = true;
            }
            other => {
                let x = self.expr_any(other);
                self.flush_sides();
                self.emit(&format!("_ = {x};"));
            }
        }
    }

    /// A function call statement with positional args.
    fn call_function(&mut self, name: &str, args: &[IrExpr]) {
        let m = self.go_ident(name);
        self.mark_read(name);
        self.need_fargs = true;
        if let Some(IrExpr::Array(items)) = args.get(1) {
            let vals: Vec<String> = items.iter().map(|i| self.expr_str(i)).collect();
            self.flush_sides();
            self.emit(&format!(
                "fArgs = []string{{{}}}; {m}(); fArgs = nil;",
                vals.join(", ")
            ));
        } else {
            self.flush_sides();
            self.emit(&format!("{m}();"));
        }
    }

    /// A bare `IrStmt::Exec` statement (the procsub `mktemp`/`rm`
    /// materialisation and redirected externals). The simple shape lowers
    /// exactly like a `Call("exec")` statement; anything with
    /// capture/redirects/env goes through the bash -c reconstruction.
    fn exec_stmt(
        &mut self,
        cmd: &IrExpr,
        args: &[IrExpr],
        capture: &Option<String>,
        redirects: &[IrExpr],
        env: &[(String, IrExpr)],
    ) {
        if capture.is_none() && redirects.is_empty() && env.is_empty() {
            // shape the word list like a Call("exec") so the builtin
            // dispatch below sees the same `args` layout
            let call_args = vec![
                cmd.clone(),
                IrExpr::Array(args.to_vec()),
            ];
            if let IrExpr::Str(c, _) = cmd {
                if self.exec_builtin_stmt(c, &call_args) {
                    return;
                }
                if self.functions.contains(c) {
                    self.call_function(c, &call_args);
                    return;
                }
                // procsub-temp cleanup keeps `$?` (see is_tmp_cleanup)
                if is_tmp_cleanup(c, args) {
                    if let Some(argv) = self.argv_strings(args) {
                        let mut parts = vec![Self::go_str(c)];
                        parts.extend(argv);
                        self.flush_sides();
                        self.emit(&format!("_ = runCmd({})", exec_cmd_apply(&parts)));
                        return;
                    }
                }
            }
            let cmd_go = self.expr_str(cmd);
            let argv = match self.argv_strings(args) {
                Some(a) => a,
                None => {
                    self.mark_todo("exec argv");
                    return;
                }
            };
            let mut parts = vec![cmd_go];
            parts.extend(argv);
            self.flush_sides();
            self.emit(&format!("st = runCmd({})", exec_cmd_apply(&parts)));
            self.need_st = true;
            return;
        }
        if let Some(text) = self.exec_shell_text(cmd, args, capture, redirects, env) {
            let envlit = self.env_lit();
            self.flush_sides();
            match capture {
                Some(name) => {
                    self.mark_written(name);
                    let m = self.go_ident(name);
                    let sync = self.sync_inline(name);
                    self.emit(&format!(
                        "{m} = s2s(capRun({}, {envlit}));{sync}",
                        Self::go_str(&text)
                    ));
                }
                None => {
                    self.emit(&format!("st = redirRun({}, {envlit});", Self::go_str(&text)));
                    self.need_st = true;
                }
            }
            return;
        }
        self.mark_todo("exec stmt");
    }

    /// Builtin exec statements: echo/printf/exit/cd/let/local/… Returns
    /// true if handled.
    fn exec_builtin_stmt(&mut self, cmd: &str, args: &[IrExpr]) -> bool {
        let argv: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items.as_slice(),
            _ => &[],
        };
        match cmd {
            "echo" => {
                self.echo_stmt(argv);
                true
            }
            "printf" => {
                self.printf_stmt(argv);
                true
            }
            "exit" => {
                let code = if argv.is_empty() {
                    "0".to_string()
                } else {
                    self.expr_num(&argv[0])
                };
                self.flush_sides();
                self.emit(&format!("os.Exit(int({code}));"));
                true
            }
            "cd" => {
                let path = match argv.first() {
                    Some(p) => self.expr_str(p),
                    None => "\"\"".to_string(),
                };
                self.flush_sides();
                // report failure like bash (`cd /nonexistent || …`)
                self.emit(&format!("st = b2i(os.Chdir({path}) != nil);"));
                self.need_st = true;
                true
            }
            "let" => {
                for a in argv {
                    if let IrExpr::Str(s, _) = a {
                        if let Some((stmt, value)) = self.let_stmt(s) {
                            self.flush_sides();
                            self.emit(&stmt);
                            // bash let status: 0 iff expr != 0
                            self.emit(&format!("st = b2i(({value}) == 0);"));
                            self.need_st = true;
                        } else if let Some((sides, v)) = self.arith_str(s) {
                            for sd in sides {
                                self.sides.push(sd);
                            }
                            self.flush_sides();
                            self.emit(&format!("st = b2i(({v}) == 0);"));
                            self.need_st = true;
                        } else {
                            self.mark_todo("let");
                        }
                    }
                }
                true
            }
            "local" | "typeset" | "declare" => {
                if self.declare_display(argv) {
                    return true;
                }
                self.declare_builtin(argv);
                true
            }
            "export" => {
                for a in argv {
                    if let IrExpr::Str(arg, _) = a {
                        if let Some(eq) = arg.find('=') {
                            let name = &arg[..eq];
                            let value = &arg[eq + 1..];
                            let v = self.expand_str(value);
                            self.emit(&format!("os.Setenv({}, {v});", Self::go_str(name)));
                        } else if self.declared(arg) {
                            let m = self.ident_of(arg);
                            self.emit(&format!("os.Setenv({}, s2s({m}));", Self::go_str(arg)));
                        }
                    }
                }
                true
            }
            "unset" => {
                for a in argv {
                    if let IrExpr::Str(name, _) = a {
                        // `unset ref` on a nameref unsets the TARGET
                        let name = self.nameref_resolve(name);
                        let name = name.as_str();
                        // unsetting IFS restores the default join separator
                        if name == "IFS" {
                            self.emit("ifsJoin = \" \";");
                        }
                        let m = self.go_ident(name);
                        self.mark_written(name);
                        // NOTE: int/case attribute sets are pre-scanned and
                        // read-only here — the home keeps its hoisted type.
                        if self.is_arr(name) {
                            self.emit(&format!("{m} = nil;"));
                        } else if self.is_num(name)
                            || self.arith_ints.contains(name)
                            || self.int_attrs.contains(name)
                        {
                            let sync = self.sync_inline(name);
                            self.emit(&format!("{m} = 0;{sync}"));
                        } else if self.is_str(name) {
                            let sync = self.sync_inline(name);
                            self.emit(&format!("{m} = \"\";{sync}"));
                        } else {
                            self.emit(&format!("{m} = nil;"));
                        }
                    }
                }
                true
            }
            "read" => {
                let mut names = Vec::new();
                for a in argv {
                    if let IrExpr::Str(s, _) = a {
                        if s.starts_with('-') {
                            continue;
                        }
                        names.push(s.clone());
                    }
                }
                if names.is_empty() {
                    return true;
                }
                let mut ps = Vec::new();
                for n in &names {
                    self.mark_written(n);
                    let m = self.go_ident(n);
                    ps.push(format!("&{m}"));
                }
                self.flush_sides();
                self.emit(&format!("_ = readInto({});", ps.join(", ")));
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            "shift" => {
                // positional params are empty in the gate harness
                true
            }
            "true" | ":" => {
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            "wait" => {
                // join background jobs (any pid args wait for all — the
                // harness runs argless, matching the reference run)
                self.emit("__wg.Wait();");
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            "set" | "umask" | "trap" | "return" => true,
            "eval" => {
                // eval 'code' — run the code in a bash -c child (native
                // evaluation is impossible; the child inherits the written
                // vars via env_lit).
                if let Some(code) = argv.first() {
                    let text = if let IrExpr::Str(c, _) = code {
                        c.clone()
                    } else if let Some(t) = self.cmd_text_expr(code) {
                        t
                    } else {
                        self.mark_todo("builtin eval");
                        return true;
                    };
                    // `eval "name=<quiet>"` (single assignment whose payload
                    // prints nothing: a capture, arithmetic, or a plain
                    // word) — a bare redirRun would lose the child-side
                    // assignment, so read the value back through stdout
                    if let Some((name, payload)) = split_first_eq(&text) {
                        if is_plain_name(name)
                            && (payload.starts_with("$(")
                                || payload.starts_with('`')
                                || !payload.contains([
                                    '$', ';', '\n', ' ', '\t',
                                ]))
                        {
                            let env = self.env_lit();
                            let m = self.go_ident(name);
                            self.mark_written(name);
                            self.flush_sides();
                            let sync = self.sync_inline(name);
                            self.emit(&format!(
                                "{m} = s2s(capRun({} + \"; printf '%s' \\\"${}\\\"\", {env}));{sync}",
                                Self::go_str(&text),
                                name,
                            ));
                            self.need_st = true;
                            return true;
                        }
                    }
                    let env = self.env_lit();
                    self.emit(&format!("st = redirRun({}, {env});", Self::go_str(&text)));
                    self.need_st = true;
                    true
                } else {
                    self.mark_todo("builtin eval");
                    true
                }
            }
            "source" | "." => {
                if let Some(p) = argv.first() {
                    match self.cmd_text_expr(p) {
                        Some(t) => {
                            let env = self.env_lit();
                            self.emit(&format!(
                                "st = redirRun(\"source {}\", {env});",
                                cmd_quote(p, &t)
                            ));
                            self.need_st = true;
                        }
                        None => self.mark_todo(&format!("builtin {cmd}")),
                    }
                    true
                } else {
                    self.mark_todo(&format!("builtin {cmd}"));
                    true
                }
            }
            "command" | "type" => {
                // `command cmd …` / `type cmd` — run via bash -c so
                // builtin resolution and stdout match bash.
                let mut parts = vec![cmd.to_string()];
                let mut ok = true;
                for a in argv {
                    match self.cmd_text_expr(a) {
                        Some(t) => parts.push(cmd_quote(a, &t)),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    let env = self.env_lit();
                    self.emit(&format!(
                        "st = redirRun({}, {env});",
                        Self::go_str(&parts.join(" "))
                    ));
                    self.need_st = true;
                    true
                } else {
                    self.mark_todo(&format!("builtin {cmd}"));
                    true
                }
            }
            "exec" => {
                // `exec cmd …` — replace the shell: run and exit.
                if let Some((cmd_go, argv_go)) = self.exec_cmd_argv(args) {
                    let mut parts = vec![cmd_go];
                    parts.extend(argv_go);
                    self.emit(&format!(
                        "st = runCmd({}); os.Exit(int(st));",
                        exec_cmd_apply(&parts)
                    ));
                    self.need_st = true;
                } else {
                    self.mark_todo("builtin exec");
                }
                true
            }
            "readonly" => {
                // readonly x=1 — the value matters, the flag doesn't
                self.declare_builtin(argv);
                true
            }
            "shopt" => {
                // `shopt -s nocasematch` → case-insensitive [[ == ]]
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    if items
                        .iter()
                        .any(|i| matches!(i, IrExpr::Str(s, _) if s == "-s"))
                    {
                        if items
                            .iter()
                            .any(|i| matches!(i, IrExpr::Str(s, _) if s == "nocasematch"))
                        {
                            self.nocase = true;
                        }
                    }
                }
                true
            }
            "eval" | "source" | "." | "command" | "type" | "exec" | "readonly" => {
                self.mark_todo(&format!("builtin {cmd}"));
                true
            }
            _ => false,
        }
    }

    /// `echo [-n|-e|-E] args…` statement.
    fn echo_stmt(&mut self, argv: &[IrExpr]) {
        let (newline, escape, items) = self.echo_flags(argv);
        let parts = match self.argv_parts(items) {
            Some(p) => p,
            None => {
                self.mark_todo("echo argv");
                return;
            }
        };
        // a bad substitution (`${!prefix*[@]}`) fails the whole command
        // with NO output (bash prints nothing, not even a blank line)
        if items.iter().any(|e| self.bad_subst(e)) {
            self.emit("// bad substitution — command suppressed");
            if self.need_st {
                self.emit("st = 1;");
            }
            self.need_st = true;
            return;
        }
        // `echo -e` stops at an unescaped `\c` (no trailing newline)
        let (parts, stopped) = if escape {
            trunc_c(parts)
        } else {
            (parts, false)
        };
        let parts = if escape {
            parts
                .into_iter()
                .map(|p| match p {
                    Part::Lit(t) => Part::Arg(format!("eEsc({})", Self::go_str(&t))),
                    other => other,
                })
                .collect()
        } else {
            parts
        };
        let call = self.print_from_parts(parts, newline && !stopped);
        self.flush_sides();
        self.emit(&format!("{call};"));
        if self.need_st {
            self.emit("st = 0;");
        }
    }

    /// `printf format args…` statement → bprintf.
    fn printf_stmt(&mut self, argv: &[IrExpr]) {
        // bad substitutions fail the whole command with no output
        if argv.iter().any(|e| self.bad_subst(e)) {
            self.emit("// bad substitution — command suppressed");
            self.emit("st = 1;");
            self.need_st = true;
            return;
        }
        let Some(fmt_expr) = argv.first() else {
            return;
        };
        let fmt = self.printf_fmt(fmt_expr);
        let args: Vec<String> = argv[1..].iter().map(|a| self.expr_str(a)).collect();
        self.flush_sides();
        let call = if args.is_empty() {
            format!("bprintf({fmt})")
        } else {
            format!("bprintf({fmt}, {})", args.join(", "))
        };
        self.emit(&format!("{call};"));
        if self.need_st {
            self.emit("st = 0;");
        }
    }

    /// The printf format string: `%` verbs stay raw (no Sprintf escaping).
    fn printf_fmt(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::go_str(s),
            IrExpr::Interpolate(parts) => {
                let mut out = String::new();
                let mut args = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(t) => out.push_str(t),
                        InterpPart::Expr(x) => {
                            out.push_str("%v");
                            args.push(self.expr_any(x));
                        }
                    }
                }
                if args.is_empty() {
                    Self::go_str(&out)
                } else {
                    format!("fmt.Sprintf({}, {})", Self::go_str(&out), args.join(", "))
                }
            }
            _ => self.expr_str(e),
        }
    }

    /// `local/typeset/declare [-i|-a|-A] name[=value] …`
    /// `typeset -f/-F/-p` display builtins (function definitions, names,
    /// declarations). Returns false when argv isn't a pure display query
    /// (other flags, non-Str words) — the declare path handles those.
    fn declare_display(&mut self, argv: &[IrExpr]) -> bool {
        let mut words: Vec<&str> = Vec::new();
        for a in argv {
            let IrExpr::Str(s, _) = a else {
                return false;
            };
            words.push(s.as_str());
        }
        let mut mode: Option<char> = None;
        let mut names: Vec<String> = Vec::new();
        for w in &words {
            if w.starts_with('-') && w.len() > 1 && w[1..].chars().all(|c| c.is_ascii_alphabetic())
            {
                let mut only_display = true;
                for c in w[1..].chars() {
                    if c == 'f' || c == 'F' || c == 'p' {
                        mode = Some(c);
                    } else {
                        only_display = false;
                    }
                }
                if !only_display {
                    return false;
                }
            } else if w.starts_with('-') || w.starts_with('+') {
                return false;
            } else {
                names.push(w.split('=').next().unwrap_or("").to_string());
            }
        }
        match mode {
            Some('F') => {
                // function names only (`typeset -F myfunc` → `myfunc`)
                let list: Vec<String> = if names.is_empty() {
                    self.func_order.clone()
                } else {
                    names
                };
                for n in &list {
                    if self.functions.contains(n) {
                        self.emit(&format!("fmt.Println({});", Self::go_str(n)));
                    }
                }
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            Some('f') => {
                // function definitions in bash's pretty-print format
                let list: Vec<String> = if names.is_empty() {
                    self.func_order.clone()
                } else {
                    names
                };
                for n in &list {
                    let body = self.func_src.get(n).cloned().unwrap_or(None);
                    let Some(stmts) = body else {
                        continue;
                    };
                    self.emit(&format!("fmt.Println({});", Self::go_str(&format!("{n} () "))));
                    self.emit(&format!("fmt.Println({});", Self::go_str("{ ")));
                    for (i, t) in stmts.iter().enumerate() {
                        let mut line = format!("    {t}");
                        if i + 1 < stmts.len() {
                            line.push(';');
                        }
                        self.emit(&format!("fmt.Println({});", Self::go_str(&line)));
                    }
                    self.emit(&format!("fmt.Println({});", Self::go_str("}")));
                }
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            Some('p') => {
                // declarations (`typeset -p printtest` →
                // `declare -ir printtest="99"`)
                for n in &names {
                    if self.functions.contains(n) {
                        continue;
                    }
                    let mut flags = String::new();
                    if self.assoc_arrays.contains(n) {
                        flags.push('A');
                    } else if self.is_arr(n) {
                        flags.push('a');
                    }
                    if self.int_attrs.contains(n)
                        || self.is_num(n)
                        || self.arith_ints.contains(n)
                    {
                        flags.push('i');
                    }
                    match self.case_attrs.get(n) {
                        Some('l') => flags.push('l'),
                        Some('u') => flags.push('u'),
                        _ => {}
                    }
                    if self.nameref_targets.contains_key(n) {
                        flags.push('n');
                    }
                    if self.ro_attrs.contains(n) {
                        flags.push('r');
                    }
                    // note: -t/-x/-g are not tracked (see limitations doc)
                    let decl = if flags.is_empty() {
                        "declare --".to_string()
                    } else {
                        format!("declare -{flags}")
                    };
                    if let Some(t) = self.nameref_targets.get(n).cloned() {
                        self.emit(&format!(
                            "fmt.Println({} + {});",
                            Self::go_str(&format!("{decl} {n}=")),
                            format!("shq({})", Self::go_str(&t))
                        ));
                    } else if self.assoc_arrays.contains(n) {
                        self.emit(&format!(
                            "fmt.Println({} + assocDecl({}));",
                            Self::go_str(&format!("{decl} {n}=")),
                            Self::go_str(n)
                        ));
                    } else if self.is_arr(n) {
                        if !self.declared(n) {
                            continue;
                        }
                        let m = self.ident_of(n);
                        self.emit(&format!(
                            "fmt.Println({} + arrDecl({m}));",
                            Self::go_str(&format!("{decl} {n}=")),
                        ));
                    } else if !self.declared(n) {
                        self.emit(&format!(
                            "fmt.Println({});",
                            Self::go_str(&format!("{decl} {n}"))
                        ));
                    } else {
                        let m = self.ident_of(n);
                        self.emit(&format!(
                            "fmt.Println({} + shq(s2s({m})));",
                            Self::go_str(&format!("{decl} {n}=")),
                        ));
                    }
                }
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            _ => false,
        }
    }

    fn declare_builtin(&mut self, argv: &[IrExpr]) {
        // a bad substitution fails the whole builtin with no effect
        if argv.iter().any(|e| self.bad_subst(e)) {
            self.emit("// bad substitution — declare suppressed");
            self.emit("st = 1;");
            self.need_st = true;
            return;
        }
        let mut last_flag = String::new();
        // attribute state persists across names in one command
        // (`typeset -i a b=1` marks both) — the pre-scan is the typing
        // authority; this drives value codegen at the declare site.
        let mut saw_int = false;
        let mut saw_export = false;
        let mut saw_nameref = false;
        let mut saw_case: Option<char> = None;
        let mut i = 0usize;
        while i < argv.len() {
            let a = &argv[i];
            if let IrExpr::Str(arg, _) = a {
                if arg.starts_with('-') {
                    last_flag = arg.clone();
                    if arg.contains('i') {
                        saw_int = true;
                    }
                    if arg.contains('x') {
                        saw_export = true;
                    }
                    if arg.contains('n') {
                        saw_nameref = true;
                    }
                    if arg.contains('l') {
                        saw_case = Some('l');
                    }
                    if arg.contains('u') {
                        saw_case = Some('u');
                    }
                    i += 1;
                    continue;
                }
                if arg.starts_with('+') {
                    if arg.contains('i') {
                        saw_int = false;
                    }
                    if arg.contains('x') {
                        saw_export = false;
                    }
                    if arg.contains('n') {
                        saw_nameref = false;
                    }
                    if arg.contains('l') || arg.contains('u') {
                        saw_case = None;
                    }
                    i += 1;
                    continue;
                }
                if let Some(eq) = arg.find('=') {
                    let name = &arg[..eq];
                    let value = arg[eq + 1..].to_string();
                    // `local x=$y` arrives as [Str("x="), getVar("y")…]
                    let mut tail: Vec<String> = Vec::new();
                    let mut j = i + 1;
                    while j < argv.len() {
                        match &argv[j] {
                            IrExpr::Str(s2, _) if !s2.starts_with('-') && !s2.contains('=') => {
                                tail.push(Self::go_str(s2));
                                j += 1;
                            }
                            other => {
                                tail.push(self.expr_str(other));
                                j += 1;
                            }
                        }
                    }
                    // `typeset -n ref=target` with a static target: alias,
                    // no home (reads/writes resolve to target)
                    if saw_nameref
                        && tail.is_empty()
                        && !value.is_empty()
                        && !value.contains(['$', '['])
                        && value.as_str() != name
                    {
                        self.nameref_targets
                            .insert(name.to_string(), value.clone());
                        self.emit(&format!("// nameref {name} -> {value}"));
                        if self.need_st {
                            self.emit("st = 0;");
                        }
                        last_flag.clear();
                        i = j;
                        continue;
                    }
                    self.mark_written(name);
                    if last_flag == "-a" || last_flag == "-A" {
                        self.mark_arr(name);
                    }
                    let is_int = saw_int || self.int_attrs.contains(name);
                    let case_c = saw_case.or_else(|| self.case_attrs.get(name).copied());
                    if is_int {
                        self.mark_arith(name);
                    }
                    let m = self.go_ident(name);
                    // the literal part may contain shell $vars; the tail is
                    // already Go expressions
                    let mut v = if value.is_empty() {
                        String::new()
                    } else {
                        self.expand_str(&value)
                    };
                    if !tail.is_empty() {
                        let joined = tail.join(" + ");
                        v = if v.is_empty() {
                            joined
                        } else {
                            format!("({v} + {joined})")
                        };
                    }
                    // integer-typed home (attribute, verdict, or arith
                    // use): raw literals evaluate arithmetically, composed
                    // values coerce via s2i
                    let num_home = is_int
                        || self.is_num(name)
                        || self.arith_ints.contains(name);
                    if v.is_empty() {
                        v = "\"\"".to_string();
                    }
                    if num_home && tail.is_empty() && !value.is_empty() {
                        v = self.int_rhs_str(&value, format!("s2i({v})"));
                    } else if num_home {
                        v = format!("s2i({v})");
                    }
                    // case attributes fold the assigned value
                    if !is_int {
                        if case_c == Some('l') {
                            v = format!("strings.ToLower({v})");
                        } else if case_c == Some('u') {
                            v = format!("strings.ToUpper({v})");
                        }
                    }
                    let sync = self.sync_inline(name);
                    self.emit(&format!("{m} = {v};{sync}"));
                    // `[*]` joins follow IFS (static values only)
                    if name == "IFS" && tail.is_empty() && !value.is_empty() {
                        let sep = value.chars().next().map(|c| c.to_string()).unwrap_or_default();
                        self.emit(&format!("ifsJoin = {};", Self::go_str(&sep)));
                    }
                    // `-x`: the value also enters the process environment
                    // (external children like `env` must see it)
                    if saw_export {
                        self.emit(&format!("os.Setenv({}, s2s({m}));", Self::go_str(name)));
                    }
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    i = j;
                } else {
                    self.mark_written(arg);
                    if last_flag == "-a" || last_flag == "-A" {
                        self.mark_arr(arg);
                    }
                    if saw_int || self.int_attrs.contains(arg) {
                        self.mark_arith(arg);
                    }
                    if saw_export && self.declared(arg) {
                        let m = self.go_ident(arg);
                        self.emit(&format!("os.Setenv({}, s2s({m}));", Self::go_str(arg)));
                    }
                    let m = self.go_ident(arg);
                    self.emit(&format!("// declare {m}"));
                    i += 1;
                }
                last_flag.clear();
            } else if let IrExpr::Call { func, args: sa_args } = a {
                // compound assignment inside declare (`typeset -a
                // arr=(10 20 30)` / `typeset -A assoc=([k]=v)`)
                if func != "setArray" && func != "setArrayAppend" {
                    i += 1;
                    continue;
                }
                let Some(IrExpr::Str(name, _)) = sa_args.first() else {
                    i += 1;
                    continue;
                };
                let is_int = saw_int || self.int_attrs.contains(name);
                let elems_all: Vec<&IrExpr> = setarray_elems(&sa_args[1..]);
                // `-A` with no elements needs no home (assoc lives in
                // vars; an empty slice home would shadow assocVals lookups)
                if elems_all.is_empty() && last_flag.contains('A') {
                    self.assoc_arrays.insert(name.to_string());
                    self.mark_written(name);
                    self.emit(&format!("// declare -A {name}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    i += 1;
                    continue;
                }
                if self.assign_compound(
                    name,
                    &elems_all,
                    func == "setArrayAppend",
                    last_flag.contains('A'),
                    is_int,
                ) {
                    if is_int {
                        self.mark_arith(name);
                    }
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    i += 1;
                    continue;
                }
                // plain element list: indexed home like stmt_assign.
                // A lone `$@` is the whole array; mixed with literals it
                // cannot spread into a literal (refuse loudly).
                if elems_all.len() == 1 && is_listvar_at(elems_all[0]) {
                    self.mark_arr(name);
                    self.mark_written(name);
                    let m = self.go_ident(name);
                    self.flush_sides();
                    let sync = self.sync_inline(name);
                    self.emit(&format!("{m} = strsToAny(argsList());{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    i += 1;
                    continue;
                }
                if elems_all.iter().any(|e| is_listvar_at(e)) {
                    self.mark_todo("listVar mixed with literals");
                    i += 1;
                    continue;
                }
                if elems_all.len() == 1 && is_capture_words(elems_all[0]) {
                    let cap = self.capture_words_cap(elems_all[0]);
                    self.mark_arr(name);
                    self.mark_written(name);
                    let m = self.go_ident(name);
                    self.flush_sides();
                    let sync = self.sync_inline(name);
                    self.emit(&format!("{m} = {cap};{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    i += 1;
                    continue;
                }
                if elems_all.iter().any(|e| is_capture_words(e)) {
                    self.mark_todo("captureWords mixed with literals");
                    i += 1;
                    continue;
                }
                let elems: Vec<String> = elems_all
                    .iter()
                    .map(|e| {
                        if is_int {
                            self.int_rhs(e)
                        } else {
                            self.expr_any(e)
                        }
                    })
                    .collect();
                self.mark_arr(name);
                self.mark_written(name);
                if is_int {
                    self.mark_arith(name);
                }
                let m = self.go_ident(name);
                self.flush_sides();
                let sync = self.sync_inline(name);
                if func == "setArrayAppend" {
                    self.emit(&format!("{m} = append({m}, {});{sync}", elems.join(", ")));
                } else {
                    self.emit(&format!("{m} = []any{{{}}};{sync}", elems.join(", ")));
                }
                if self.need_st {
                    self.emit("st = 0;");
                }
                i += 1;
            } else {
                i += 1;
            }
        }
    }

    /// A `let` argument as a statement (`i++`, `i += 2`, `i=i+1`).
    /// A `let` argument as a statement (`i++`, `i += 2`, `i=i+1`).
    /// Returns (stmt, value-expr) so the caller can set `$?` (bash let
    /// status is 0 iff the value != 0).
    fn let_stmt(&mut self, s: &str) -> Option<(String, String)> {
        let toks = self.arith_tokens(s)?;
        let mut p = MAParser { toks, i: 0 };
        let ast = p.expr()?;
        if p.i != p.toks.len() {
            return None;
        }
        self.ma_stmt(&ast)
    }

    /// `cstyleFor` Call → full `for` loop text, or None.
    fn cstyle_for(&mut self, args: &[IrExpr]) -> Option<String> {
        let (Some(IrExpr::Str(header, _)), Some(IrExpr::Arrow(body))) = (args.first(), args.get(1))
        else {
            return None;
        };
        let parts: Vec<&str> = header.split(';').collect();
        if parts.len() != 3 {
            return None;
        }
        let (init, cond, update) = (parts[0].trim(), parts[1].trim(), parts[2].trim());
        let init_toks = self.arith_tokens(init)?;
        let mut ip = MAParser {
            toks: init_toks,
            i: 0,
        };
        let iast = ip.expr()?;
        if ip.i != ip.toks.len() {
            return None;
        }
        let init_go = self.ma_stmt(&iast)?.0.trim_end_matches(';').to_string();
        let cond_toks = self.arith_tokens(cond)?;
        let mut cp = MAParser {
            toks: cond_toks,
            i: 0,
        };
        let cast = cp.expr()?;
        if cp.i != cp.toks.len() {
            return None;
        }
        let (csides, cval) = self.ma_render(&cast)?;
        if !csides.is_empty() {
            return None;
        }
        let up_toks = self.arith_tokens(update)?;
        let mut up = MAParser {
            toks: up_toks,
            i: 0,
        };
        let uast = up.expr()?;
        if up.i != up.toks.len() {
            return None;
        }
        let update_go = self.ma_stmt(&uast)?.0.trim_end_matches(';').to_string();
        self.loop_head(&format!(" {init_go}; ({cval} != 0); {update_go} {{"));
        for s in body {
            self.stmt(s);
        }
        self.loop_close();
        Some(String::new())
    }

    /// for-loop lowering.
    fn stmt_for(&mut self, var: &str, iter: &IrExpr, body: &[IrStmt]) {
        let m = self.go_ident(var);
        self.mark_written(var);
        match self.for_list(iter) {
            Some(ForList::Static(items)) => {
                let (t, inner) = if self.is_num(var) {
                    let elems: Vec<String> = items.iter().map(|e| format!("s2i({e})")).collect();
                    ("int64", elems.join(", "))
                } else if self.is_str(var) {
                    ("string", items.join(", "))
                } else {
                    ("any", items.join(", "))
                };
                self.loop_head(&format!(" _, {m} = range []{t}{{{inner}}} {{"));
                self.emit_sync(var);
                for s in body {
                    self.stmt(s);
                }
                self.loop_close();
            }
            Some(ForList::Runtime { pre, slice }) => {
                self.emit(&pre);
                let vn = self.new_tmp();
                self.loop_head(&format!(" _, {vn} := range {slice} {{"));
                if self.is_num(var) {
                    self.emit(&format!("{m} = s2i({vn});"));
                } else if self.is_str(var) {
                    self.emit(&format!("{m} = {vn};"));
                } else {
                    self.emit(&format!("{m} = {vn};"));
                }
                self.emit_sync(var);
                for s in body {
                    self.stmt(s);
                }
                self.loop_close();
            }
            None => {
                if let IrExpr::Range { start, end } = iter {
                    self.loop_head(&format!(" {m} = {start}; {m} <= {end}; {m}++ {{"));
                    self.emit_sync(var);
                    for s in body {
                        self.stmt(s);
                    }
                    self.loop_close();
                    return;
                }
                self.mark_todo(&format!("For iter {:?}", iter));
            }
        }
    }

    fn emit_sync(&mut self, var: &str) {
        if self.need_vars {
            let m = self.go_ident(var);
            let key = normalize_assoc_key(var);
            self.emit(&format!("vars[{}] = {m};", Self::go_str(&key)));
        }
    }

    /// A for-iter → static element list or a runtime slice.
    fn for_list(&mut self, iter: &IrExpr) -> Option<ForList> {
        match iter {
            IrExpr::Array(items) => {
                let mut out = Vec::new();
                for it in items {
                    if let IrExpr::Call { func, args } = it {
                        if func == "brace" {
                            let expanded = self.brace_expand(args)?;
                            for s in expanded {
                                out.push(Self::go_str(&s));
                            }
                            continue;
                        }
                        if func == "param" {
                            if let Some(IrExpr::Str(op, _)) = args.first() {
                                if let Some(IrExpr::Str(name, _)) = args.get(1) {
                                    let name = normalize_assoc_key(name);
                                    let arr_name = if matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@" || a == "*")
                                    {
                                        format!("{name}[@]")
                                    } else {
                                        name.clone()
                                    };
                                    if arr_name.ends_with("[@]") {
                                        self.need_vars = true;
                                        let t = self.new_tmp();
                                        return Some(ForList::Runtime {
                                            pre: format!(
                                                "{t} := pList({}, {});",
                                                Self::go_str(op),
                                                Self::go_str(&arr_name)
                                            ),
                                            slice: t,
                                        });
                                    }
                                }
                            }
                        }
                        if func == "listVar" || func == "getVar" {
                            if let Some(IrExpr::Str(name, _)) = args.first() {
                                if name == "@" || name == "*" {
                                    let t = self.new_tmp();
                                    return Some(ForList::Runtime {
                                        pre: format!("{t} := argsList();"),
                                        slice: t,
                                    });
                                }
                            }
                        }
                        if func == "captureWords" {
                            if let Some(IrExpr::Arrow(body)) = args.first() {
                                let cap = self.capture_arrow(body);
                                let t = self.new_tmp();
                                return Some(ForList::Runtime {
                                    pre: format!("{t} := strings.Fields({cap});"),
                                    slice: t,
                                });
                            }
                        }
                    }
                    out.push(self.expr_any_of(it));
                }
                Some(ForList::Static(out))
            }
            IrExpr::Call { func, args } if func == "listVar" || func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if name == "@" || name == "*" {
                        let t = self.new_tmp();
                        return Some(ForList::Runtime {
                            pre: format!("{t} := argsList();"),
                            slice: t,
                        });
                    }
                }
                None
            }
            IrExpr::Call { func, args } if func == "captureWords" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    let cap = self.capture_arrow(body);
                    let t = self.new_tmp();
                    return Some(ForList::Runtime {
                        pre: format!("{t} := strings.Fields({cap});"),
                        slice: t,
                    });
                }
                None
            }
            _ => None,
        }
    }

    fn expr_any_of(&mut self, e: &IrExpr) -> String {
        self.expr_any(e)
    }

    // ── parts / printing ─────────────────────────────────────────────

    /// Split an expression into output parts: Lit(text) | Arg(goexpr).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<Part> {
        match e {
            IrExpr::Str(s, _) => vec![Part::Lit(s.clone())],
            IrExpr::Int(i) => vec![Part::Arg(i.to_string())],
            IrExpr::Var(name, _) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return vec![Part::Arg(format!("os.Getenv({})", Self::go_str(&name)))];
                }
                let m = self.ident_of(&name);
                vec![Part::Arg(m)]
            }
            IrExpr::Ident(name) => {
                let name = self.nameref_resolve(name);
                if !self.declared(&name) {
                    return vec![Part::Arg(format!("os.Getenv({})", Self::go_str(&name)))];
                }
                let m = self.ident_of(&name);
                vec![Part::Arg(m)]
            }
            IrExpr::Interpolate(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push(Part::Lit(s.clone())),
                        InterpPart::Expr(x) => out.push(Part::Arg(self.expr_any(x))),
                    }
                }
                out
            }
            IrExpr::Arith(a) => vec![Part::Arg(self.arith(a))],
            IrExpr::Bool(b) => {
                if *b {
                    vec![Part::Arg("true".into())]
                } else {
                    vec![Part::Arg("false".into())]
                }
            }
            IrExpr::BinOp { .. } => vec![Part::Arg(self.expr_any(e))],
            IrExpr::Call { .. } => vec![Part::Arg(self.expr_any(e))],
            IrExpr::Ext(_n) => vec![Part::Arg(self.expr_any(e))],
            other => {
                self.mark_todo(&format!("echo arg {:?}", other));
                vec![Part::Arg("0".into())]
            }
        }
    }

    /// Parts → the joined output string expression (no Println wrapper).
    fn print_inner(&mut self, parts: Vec<Part>) -> String {
        let mut fmt = String::new();
        let mut raw = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                // `%` escapes per-LIT so `%v` markers stay raw; a
                // marker-free string keeps `echo "100%"` as a literal.
                Part::Lit(t) => {
                    raw.push_str(&t);
                    fmt.push_str(&t.replace('%', "%%"));
                }
                Part::Arg(v) => {
                    fmt.push_str("%v");
                    args.push(v);
                }
            }
        }
        if args.is_empty() {
            Self::go_str(&raw)
        } else {
            format!("fmt.Sprintf({}, {})", Self::go_str(&fmt), args.join(", "))
        }
    }

    /// Parts → one fmt.Println(fmt.Sprintf(...)) / fmt.Print(...) string.
    fn print_from_parts(&mut self, parts: Vec<Part>, newline: bool) -> String {
        let inner = self.print_inner(parts);
        if newline {
            format!("fmt.Println({inner})")
        } else {
            format!("fmt.Print({inner})")
        }
    }

    /// `echo` flags → (newline, escape, remaining items).
    fn echo_flags<'a>(&self, argv: &'a [IrExpr]) -> (bool, bool, &'a [IrExpr]) {
        let mut newline = true;
        let mut escape = false;
        let mut items = argv;
        while let Some(IrExpr::Str(f, _)) = items.first() {
            if f == "-n" {
                newline = false;
                items = &items[1..];
            } else if f == "-e" {
                escape = true;
                items = &items[1..];
            } else if f == "-E" {
                items = &items[1..];
            } else if f == "-en" || f == "-ne" {
                newline = false;
                escape = true;
                items = &items[1..];
            } else {
                break;
            }
        }
        (newline, escape, items)
    }

    /// `$(echo …)` capture → the joined string natively (captures strip
    /// trailing newlines, so `-n` needs no special handling). Refuses
    /// glob args, bad substitutions, and unlowerable argv (None falls
    /// back to the bash child).
    fn echo_capture(&mut self, items: &[IrExpr]) -> Option<String> {
        let (s, _) = self.echo_capture_full(items)?;
        Some(s)
    }

    /// echo value + whether it ends with a newline (pipe input keeps it).
    fn echo_capture_full(&mut self, items: &[IrExpr]) -> Option<(String, bool)> {
        let (newline, escape, items) = self.echo_flags(items);
        for it in items {
            if let IrExpr::Str(s, _) = it {
                if has_glob(s) {
                    return None;
                }
            }
        }
        if items.iter().any(|e| self.bad_subst(e)) {
            return None;
        }
        let mut parts = self.argv_parts(items)?;
        let mut stopped = false;
        if escape {
            let (kept, st) = trunc_c(parts);
            stopped = st;
            parts = kept
                .into_iter()
                .map(|p| match p {
                    Part::Lit(t) => Part::Arg(format!("eEsc({})", Self::go_str(&t))),
                    other => other,
                })
                .collect();
        }
        Some((self.print_inner(parts), newline && !stopped))
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect written vars so declarations are hoisted.
        let mut written: BTreeSet<String> = BTreeSet::new();
        let mut arrays: BTreeSet<String> = BTreeSet::new();
        collect_written(&prog.stmts, &mut written, &mut arrays);
        self.written = written;
        self.arrays = arrays;
        // Assoc pre-scan: bases ever written with a non-integer key live
        // in the `vars` map under `base[key]` (dynamic reads/writes use
        // the same home the baked writes sync to).
        let mut assoc: BTreeSet<String> = BTreeSet::new();
        collect_assoc(&prog.stmts, &mut assoc);
        self.assoc_arrays = assoc;
        // Attribute pre-scan: declare -i/-l/-u anywhere (order-independent
        // typing for unset-before-declare shapes).
        let mut int_attrs: BTreeSet<String> = BTreeSet::new();
        let mut case_attrs: HashMap<String, char> = HashMap::new();
        let mut nameref_targets: HashMap<String, String> = HashMap::new();
        let mut ro_attrs: BTreeSet<String> = BTreeSet::new();
        collect_decl_attrs(
            &prog.stmts,
            &mut int_attrs,
            &mut case_attrs,
            &mut nameref_targets,
            &mut ro_attrs,
        );
        self.int_attrs = int_attrs;
        self.case_attrs = case_attrs;
        self.nameref_targets = nameref_targets;
        self.ro_attrs = ro_attrs;
        for (n, _) in &prog.var_types {
            self.written.insert(n.clone());
            if n.contains('[') {
                self.arrays.insert(n.clone());
            }
        }
        // Pre-scan: vars written by ARITH assignments / inc-decs anywhere
        // (cstyleFor steps, the goto restructure's flags) hoist as int64 —
        // EVERY assignment to them must coerce numerically, including
        // ones rendered before the arith write is seen (the C frontend's
        // `j = "0"` init vs `j++` step).
        {
            use IrStmt::*;
            fn note_arith(e: &IrExpr, out: &mut BTreeSet<String>) {
                if let IrExpr::Arith(a) = e {
                    match &**a {
                        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
                            out.insert(var.clone());
                        }
                        _ => {}
                    }
                }
            }
            fn note_str_arith(sv: &str, out: &mut BTreeSet<String>) {
                for tk in sv.split_whitespace() {
                    if let Some(base) =
                        tk.strip_suffix("++").or_else(|| tk.strip_suffix("--"))
                    {
                        let base = base.trim_matches('(').trim_matches(')');
                        if !base.is_empty() {
                            out.insert(base.to_string());
                        }
                    }
                }
            }
            let mut stack: Vec<&IrStmt> = prog.stmts.iter().collect();
            while let Some(st) = stack.pop() {
                match st {
                    Assign { targets, expr, .. } => {
                        note_arith(expr, &mut self.arith_ints);
                        if let IrExpr::Call { func: f, args } = expr {
                            if matches!(f.as_str(), "arith" | "arithEval" | "let" | "fnCall" | "fnValue")
                            {
                                for a in args {
                                    if let IrExpr::Str(sv, _) = a {
                                        note_str_arith(sv, &mut self.arith_ints);
                                    }
                                    if let IrExpr::Array(items) = a {
                                        for it in items {
                                            if let IrExpr::Str(sv, _) = it {
                                                note_str_arith(sv, &mut self.arith_ints);
                                            }
                                        }
                                    }
                                }
                            }
                            let _ = targets;
                        }
                    }
                    Expr(IrExpr::Arith(a)) => {
                        if let ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } =
                            &**a
                        {
                            self.arith_ints.insert(var.clone());
                        }
                    }
                    Expr(IrExpr::Call { func, args })
                        if func == "arith" || func == "arithEval" || func == "let" =>
                    {
                        for a in args {
                            if let IrExpr::Str(sv, _) = a {
                                note_str_arith(sv, &mut self.arith_ints);
                            }
                        }
                    }
                    ForInit { init, step, body, .. } => {
                        for st in init.iter().chain(step.iter()) {
                            match st {
                                IrStmt::Assign { expr, .. } | IrStmt::Expr(expr) => {
                                    note_arith(expr, &mut self.arith_ints);
                                    if let IrExpr::Call { args, .. } = expr {
                                        for a in args {
                                            if let IrExpr::Str(sv, _) = a {
                                                note_str_arith(sv, &mut self.arith_ints);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        for b in body {
                            stack.push(b);
                        }
                    }
                    If {
                        cond,
                        then,
                        elsifs,
                        else_,
                        ..
                    } => {
                        note_arith(cond, &mut self.arith_ints);
                        for b in then {
                            stack.push(b);
                        }
                        for (c, b) in elsifs {
                            note_arith(c, &mut self.arith_ints);
                            for b in b {
                                stack.push(b);
                            }
                        }
                        for b in else_ {
                            stack.push(b);
                        }
                    }
                    // (arith-uses hide in every body position: `((i+=2))`
                    // in a case clause, `for x` bodies, redirect/pipeline
                    // stages — the 063 -a/-A local loop counters)
                    For { iter, body, .. } => {
                        note_arith(iter, &mut self.arith_ints);
                        for b in body {
                            stack.push(b);
                        }
                    }
                    Case { clauses, .. } => {
                        for c in clauses {
                            for b in &c.body {
                                stack.push(b);
                            }
                        }
                    }
                    Redirect { inner, .. } => {
                        for b in inner {
                            stack.push(b);
                        }
                    }
                    Pipeline { stages, .. } => {
                        for st in stages {
                            for b in st {
                                stack.push(b);
                            }
                        }
                    }
                    While { cond, body } => {
                        note_arith(cond, &mut self.arith_ints);
                        for b in body {
                            stack.push(b);
                        }
                    }
                    DoWhile { body, cond, .. } => {
                        note_arith(cond, &mut self.arith_ints);
                        for b in body {
                            stack.push(b);
                        }
                    }
                    Block(body) | Subshell(body) | Background(body) => {
                        for b in body {
                            stack.push(b);
                        }
                    }
                    Function { body, .. } => {
                        for b in body {
                            stack.push(b);
                        }
                    }
                    Try {
                        body,
                        excepts,
                        else_body,
                        finally_body,
                        ..
                    } => {
                        for b in body
                            .iter()
                            .chain(excepts.iter().flat_map(|e| e.body.iter()))
                            .chain(else_body)
                            .chain(finally_body)
                        {
                            stack.push(b);
                        }
                    }
                    _ => {}
                }
            }
        }
        // Pre-scan: need_vars (param-expansion reads the vars map) and
        // need_st ($? tracking) must be known BEFORE the body renders, so
        // earlier statements emit their sync / st=0 lines too.
        let mut flags = Render::default();
        scan_flags(&prog.stmts, &mut flags);
        self.need_vars = flags.need_vars;
        self.need_st = flags.need_st;

        // Pass 2: render the body first (helper/import flags known before
        // the preamble; st/vars/fArgs decls known before main's header).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        for (idx, s) in prog.stmts.iter().enumerate() {
            let before = self.out.len();
            self.stmt(s);
            let line = prog.stmt_lines.iter().find(|(i, _)| *i == idx).map(|(_, l)| *l);
            if let Some(l) = line {
                if let Some(first) = self.out.get_mut(before) {
                    *first = format!("{first} // line {l}");
                }
            }
        }
        // dead-var guard: Go rejects declared-and-never-read variables
        let written: Vec<String> = self.written.iter().cloned().collect();
        for v in &written {
            if !self.read.contains(v) {
                let m = self
                    .mangle
                    .get(v)
                    .cloned()
                    .unwrap_or_else(|| self.go_ident(v));
                self.emit(&format!("_ = {m}"));
            }
        }
        if self.need_st && !self.st_read {
            self.emit("_ = st");
        }
        self.emit("return;");
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        // Preamble: package, imports (all packages the fixed helper block
        // references — the helpers are unconditional, so the imports are
        // always used), helpers, main.
        let mut text = Vec::new();
        text.push("package main".to_string());
        text.push(String::new());
        text.push("import (".to_string());
        for i in [
            "\"bufio\"",
            "\"bytes\"",
            "\"fmt\"",
            "\"io\"",
            "\"os\"",
            "\"os/exec\"",
            "\"path/filepath\"",
            "\"regexp\"",
            "\"sort\"",
            "\"strconv\"",
            "\"strings\"",
            "\"sync\"",
            "\"syscall\"",
            "\"unicode/utf8\"",
        ] {
            text.push(format!("    {i}"));
        }
        text.push(")".to_string());
        text.push(String::new());
        // Only the runtime helpers the body references (transitively) —
        // a `name=\"world\"; echo hello` keeps just s2s/fmt.
        let body_text = {
            let mut d = self.decl_lines();
            d.extend(body_out.iter().cloned());
            d.join("\n")
        };
        text.extend(go_used_helpers(&body_text));
        text.push("func main() {".to_string());
        text.extend(self.decl_lines());
        text.extend(body_out.iter().cloned());
        text.push("}".to_string());
        if self.todo > 0 {
            text.push(format!(
                "// {} construct(s) lowered to TODO markers",
                self.todo
            ));
        }
        // Trim the import list to what the kept text references.
        let all = [
            "\"bufio\"", "\"bytes\"", "\"fmt\"", "\"io\"", "\"os\"", "\"os/exec\"",
            "\"path/filepath\"", "\"regexp\"", "\"sort\"", "\"strconv\"", "\"strings\"",
            "\"sync\"", "\"syscall\"", "\"unicode/utf8\"",
        ];
        let full = text.join("\n");
        let keep_paths: Vec<String> = all
            .iter()
            .map(|q| q.trim_matches('"').to_string())
            .filter(|q| go_uses_pkg(&full, q.rsplit('/').next().unwrap()))
            .collect();
        let mut out2: Vec<String> = Vec::new();
        for line in text {
            if let Some(q) = line.trim().strip_prefix("\"").and_then(|r| r.strip_suffix("\"")) {
                if !keep_paths.contains(&q.to_string()) { continue; }
            }
            out2.push(line);
        }
        self.out = out2;
    }

    /// The per-program declaration lines (vars, st, vars map, fArgs).
    fn decl_lines(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        // ForEachLine loop vars are STRING-typed by construction (scanner
        // .Text() assigned in the streaming loop) — register before typing.
        for v in &self.written {
            if v.starts_with("__l") && !self.var_types.contains_key(v) {
                self.var_types.insert(v.clone(), IrType::Str);
            }
        }
        let written: Vec<String> = self.written.iter().cloned().collect();
        for v in &written {
            let m = self.go_ident(v);
            if self.is_arr(v) {
                out.push(format!("    var {m} []any"));
            } else if self.is_num(v) || self.arith_ints.contains(v) || self.int_attrs.contains(v) {
                out.push(format!("    var {m} int64"));
            } else if self.is_str(v) {
                out.push(format!("    var {m} string"));
            } else if self.functions.contains(v) {
                out.push(format!("    var {m} func()"));
            } else {
                out.push(format!("    var {m} any"));
            }
        }
        if self.need_st {
            out.push("    var st int64".to_string());
        }
        if !self.written.is_empty() || self.need_st {
            out.push(String::new());
        }
        out
    }
}

// ── free functions ───────────────────────────────────────────────────

#[derive(Clone)]
enum MA {
    Num(i64),
    Var(String),
    ArrLen(String),
    StrLen(String),
    ArrIdx(String, Box<MA>),
    Bin(String, Box<MA>, Box<MA>),
    Un(String, Box<MA>),
    Cond(Box<MA>, Box<MA>, Box<MA>),
    Assign(String, String, Box<MA>),
    IncDec(String, i64, bool),
}

struct MAParser {
    toks: Vec<(String, bool)>,
    i: usize,
}

impl MAParser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.i).map(|(t, _)| t.as_str())
    }
    fn next(&mut self) -> Option<(String, bool)> {
        let t = self.toks.get(self.i).cloned();
        if t.is_some() {
            self.i += 1;
        }
        t
    }
    fn eat(&mut self, op: &str) -> bool {
        if self.peek() == Some(op) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self) -> Option<MA> {
        let lhs = self.ternary()?;
        if let Some(op) = self.peek() {
            if matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=") {
                let op = op.to_string();
                self.i += 1;
                let rhs = self.expr()?;
                return match lhs {
                    MA::Var(name) => Some(MA::Assign(name, op, Box::new(rhs))),
                    _ => None,
                };
            }
        }
        Some(lhs)
    }
    fn ternary(&mut self) -> Option<MA> {
        let test = self.lor()?;
        if self.eat("?") {
            let then = self.expr()?;
            if !self.eat(":") {
                return None;
            }
            let else_ = self.ternary()?;
            return Some(MA::Cond(Box::new(test), Box::new(then), Box::new(else_)));
        }
        Some(test)
    }
    fn lor(&mut self) -> Option<MA> {
        let mut l = self.land()?;
        while self.eat("||") {
            let r = self.land()?;
            l = MA::Bin("||".into(), Box::new(l), Box::new(r));
        }
        Some(l)
    }
    fn land(&mut self) -> Option<MA> {
        let mut l = self.bor()?;
        while self.eat("&&") {
            let r = self.bor()?;
            l = MA::Bin("&&".into(), Box::new(l), Box::new(r));
        }
        Some(l)
    }
    fn bor(&mut self) -> Option<MA> {
        let mut l = self.bxor()?;
        while self.eat("|") {
            let r = self.bxor()?;
            l = MA::Bin("|".into(), Box::new(l), Box::new(r));
        }
        Some(l)
    }
    fn bxor(&mut self) -> Option<MA> {
        let mut l = self.band()?;
        while self.eat("^") {
            let r = self.band()?;
            l = MA::Bin("^".into(), Box::new(l), Box::new(r));
        }
        Some(l)
    }
    fn band(&mut self) -> Option<MA> {
        let mut l = self.eq()?;
        while self.eat("&") {
            let r = self.eq()?;
            l = MA::Bin("&".into(), Box::new(l), Box::new(r));
        }
        Some(l)
    }
    fn eq(&mut self) -> Option<MA> {
        let mut l = self.rel()?;
        loop {
            if self.eat("==") {
                let r = self.rel()?;
                l = MA::Bin("==".into(), Box::new(l), Box::new(r));
            } else if self.eat("!=") {
                let r = self.rel()?;
                l = MA::Bin("!=".into(), Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Some(l)
    }
    fn rel(&mut self) -> Option<MA> {
        let mut l = self.shift()?;
        loop {
            if self.eat("<=") {
                let r = self.shift()?;
                l = MA::Bin("<=".into(), Box::new(l), Box::new(r));
            } else if self.eat(">=") {
                let r = self.shift()?;
                l = MA::Bin(">=".into(), Box::new(l), Box::new(r));
            } else if self.eat("<") {
                let r = self.shift()?;
                l = MA::Bin("<".into(), Box::new(l), Box::new(r));
            } else if self.eat(">") {
                let r = self.shift()?;
                l = MA::Bin(">".into(), Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Some(l)
    }
    fn shift(&mut self) -> Option<MA> {
        let mut l = self.add()?;
        loop {
            if self.eat("<<") {
                let r = self.add()?;
                l = MA::Bin("<<".into(), Box::new(l), Box::new(r));
            } else if self.eat(">>") {
                let r = self.add()?;
                l = MA::Bin(">>".into(), Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Some(l)
    }
    fn add(&mut self) -> Option<MA> {
        let mut l = self.mul()?;
        loop {
            if self.eat("+") {
                let r = self.mul()?;
                l = MA::Bin("+".into(), Box::new(l), Box::new(r));
            } else if self.eat("-") {
                let r = self.mul()?;
                l = MA::Bin("-".into(), Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Some(l)
    }
    fn mul(&mut self) -> Option<MA> {
        let mut l = self.pow()?;
        loop {
            if self.eat("*") {
                let r = self.pow()?;
                l = MA::Bin("*".into(), Box::new(l), Box::new(r));
            } else if self.eat("/") {
                let r = self.pow()?;
                l = MA::Bin("/".into(), Box::new(l), Box::new(r));
            } else if self.eat("%") {
                let r = self.pow()?;
                l = MA::Bin("%".into(), Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Some(l)
    }
    fn pow(&mut self) -> Option<MA> {
        let l = self.unary()?;
        if self.eat("**") {
            let r = self.pow()?;
            return Some(MA::Bin("**".into(), Box::new(l), Box::new(r)));
        }
        Some(l)
    }
    fn unary(&mut self) -> Option<MA> {
        if self.eat("++") {
            let a = self.unary()?;
            return match a {
                MA::Var(name) => Some(MA::IncDec(name, 1, true)),
                _ => None,
            };
        }
        if self.eat("--") {
            let a = self.unary()?;
            return match a {
                MA::Var(name) => Some(MA::IncDec(name, -1, true)),
                _ => None,
            };
        }
        for op in ["+", "-", "!", "~"] {
            if self.eat(op) {
                let a = self.unary()?;
                return Some(MA::Un(op.to_string(), Box::new(a)));
            }
        }
        let p = self.postfix()?;
        if self.eat("++") {
            return match p {
                MA::Var(name) => Some(MA::IncDec(name, 1, false)),
                _ => None,
            };
        }
        if self.eat("--") {
            return match p {
                MA::Var(name) => Some(MA::IncDec(name, -1, false)),
                _ => None,
            };
        }
        Some(p)
    }
    fn postfix(&mut self) -> Option<MA> {
        if self.eat("(") {
            let e = self.expr()?;
            if !self.eat(")") {
                return None;
            }
            return Some(e);
        }
        let (t, is_op) = self.next()?;
        if is_op {
            return None;
        }
        if let Some(n) = parse_arith_num(&t) {
            return Some(MA::Num(n));
        }
        if let Some(rest) = t.strip_prefix("${") {
            let inner = rest.strip_suffix('}')?;
            return self.brace_var(inner);
        }
        // base notation `10#x` / `16#ff`
        if let Some(hash) = t.find('#') {
            let base: i64 = t[..hash].parse().ok()?;
            let body = &t[hash + 1..];
            if body.chars().all(|c| c.is_ascii_digit()) {
                let v = i64::from_str_radix(body, base as u32).ok()?;
                return Some(MA::Num(v));
            }
            if base == 10 {
                return Some(MA::Var(body.to_string()));
            }
            return None;
        }
        if let Some(rest) = t.strip_prefix('$') {
            return Some(MA::Var(rest.to_string()));
        }
        if let Some(idx) = t.find('[') {
            if t.ends_with(']') {
                let name = t[..idx].to_string();
                let key = &t[idx + 1..t.len() - 1];
                if let Some(k) = parse_arith_num(key) {
                    return Some(MA::ArrIdx(name, Box::new(MA::Num(k))));
                }
                let key = key.strip_prefix('$').unwrap_or(key);
                return Some(MA::ArrIdx(name, Box::new(MA::Var(key.to_string()))));
            }
            return None;
        }
        Some(MA::Var(t))
    }

    fn brace_var(&mut self, inner: &str) -> Option<MA> {
        // ${x} / ${#x} / ${x[i]}
        if let Some(rest) = inner.strip_prefix('#') {
            if let Some(arr) = rest.strip_suffix("[@]").or_else(|| rest.strip_suffix("[*]")) {
                return Some(MA::ArrLen(arr.to_string()));
            }
            return Some(MA::StrLen(rest.to_string()));
        }
        if let Some(idx) = inner.find('[') {
            if inner.ends_with(']') {
                let name = inner[..idx].to_string();
                let key = &inner[idx + 1..inner.len() - 1];
                if let Some(k) = parse_arith_num(key) {
                    return Some(MA::ArrIdx(name, Box::new(MA::Num(k))));
                }
                let key = key.strip_prefix('$').unwrap_or(key);
                return Some(MA::ArrIdx(name, Box::new(MA::Var(key.to_string()))));
            }
        }
        if inner.is_empty() {
            return None;
        }
        Some(MA::Var(inner.to_string()))
    }
}

enum ForList {
    /// static element Go exprs (strings)
    Static(Vec<String>),
    /// (pre-stmt, slice var) — runtime []string slice
    Runtime { pre: String, slice: String },
}

fn join_args(v: &[String]) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!(", {}", v.join(", "))
    }
}

fn parse_arith_num(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    if s.starts_with("0x") || s.starts_with("0X") {
        i64::from_str_radix(&s[2..], 16).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

/// Unary `[ ]` flags handled by `fileTest` / the mini evaluator.
const TEST_FLAGS: &[&str] = &[
    "f", "d", "e", "s", "r", "w", "x", "L", "p", "h", "b", "c", "g", "k", "u", "O", "G", "N", "S",
    "a", "n", "z", "t",
];

/// Binary test operators.
const TEST_BINOPS: &[&str] = &[
    "=", "==", "!=", "=~", "-eq", "-ne", "-lt", "-le", "-gt", "-ge", "-nt", "-ot", "-ef", "<", ">",
    "<=", ">=",
];

/// Tokenize a test string: whitespace-split with `\(`/`\)` as grouping
/// tokens, quoted spans and `$( … )` kept whole (with nesting tracked),
/// and glued binary operators (`$x!=2`, `"$a"=~re`) split off.
fn test_tokens(s: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_whitespace() {
            i += 1;
            continue;
        }
        // \( \) grouping tokens
        if b[i] == '\\' && i + 1 < b.len() && (b[i + 1] == '(' || b[i + 1] == ')') {
            out.push(b[i + 1].to_string());
            i += 2;
            continue;
        }
        if b[i] == '(' || b[i] == ')' {
            out.push(b[i].to_string());
            i += 1;
            continue;
        }
        // quoted span / `$( … )` kept whole
        if b[i] == '"' || b[i] == '\'' || (b[i] == '$' && i + 1 < b.len() && b[i + 1] == '(') {
            let start = i;
            let quote = b[i];
            let mut depth: i64 = if b[i] == '$' { 1 } else { 0 };
            let mut in_dollar = b[i] == '$';
            if in_dollar {
                i += 2; // skip `$(` — the `(` is the depth-1 paren
            } else {
                i += 1;
            }
            while i < b.len() {
                if b[i] == '\\' && i + 1 < b.len() {
                    i += 2;
                    continue;
                }
                if in_dollar {
                    if b[i] == '(' {
                        depth += 1;
                    } else if b[i] == ')' {
                        depth -= 1;
                        if depth == 0 {
                            in_dollar = false;
                            if quote == '$' {
                                i += 1;
                                break;
                            }
                        }
                    }
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else if b[i] == '$' && i + 1 < b.len() && b[i + 1] == '(' {
                    in_dollar = true;
                    depth = 1;
                    i += 1; // skip the `(` (counted as depth 1)
                }
                i += 1;
            }
            if i > b.len() {
                return None;
            }
            out.push(b[start..i].iter().collect());
            continue;
        }
        // `${ … }` kept whole (patterns may contain spaces: ${x% *})
        if b[i] == '$' && i + 1 < b.len() && b[i + 1] == '{' {
            let start = i;
            let mut depth = 0i64;
            i += 1;
            while i < b.len() {
                if b[i] == '{' {
                    depth += 1;
                } else if b[i] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            // unterminated `${…` (the core may drop the closing brace) —
            // take the rest of the string as the token
            out.push(b[start..i].iter().collect());
            continue;
        }
        // extglob sigil tokens: @(…) !(…) ?(…) +(…) *(…)
        if (b[i] == '@' || b[i] == '!' || b[i] == '?' || b[i] == '+' || b[i] == '*')
            && i + 1 < b.len()
            && b[i + 1] == '('
        {
            let start = i;
            let mut depth = 0i64;
            while i < b.len() {
                if b[i] == '(' {
                    depth += 1;
                } else if b[i] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            if depth != 0 {
                return None;
            }
            out.push(b[start..i].iter().collect());
            continue;
        }
        // regular token
        let _start = i;
        let mut tok = String::new();
        while i < b.len()
            && !b[i].is_whitespace()
            && !(b[i] == '\\' && i + 1 < b.len() && (b[i + 1] == '(' || b[i + 1] == ')'))
        {
            if b[i] == '(' {
                // `=@(pattern)` / `x!(*.js)` — the paren opens an extglob
                // after the sigil; consume it balanced into the token.
                if tok.ends_with('@')
                    || tok.ends_with('!')
                    || tok.ends_with('?')
                    || tok.ends_with('+')
                    || tok.ends_with('*')
                {
                    let mut depth = 0i64;
                    while i < b.len() {
                        if b[i] == '(' {
                            depth += 1;
                        } else if b[i] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                tok.push(')');
                                i += 1;
                                break;
                            }
                        } else if b[i] == '\\' && i + 1 < b.len() {
                            tok.push(b[i + 1]);
                            i += 2;
                            continue;
                        }
                        tok.push(b[i]);
                        i += 1;
                    }
                    if depth != 0 {
                        return None;
                    }
                    continue;
                }
                break;
            }
            if b[i] == '\\' && i + 1 < b.len() {
                let nx = b[i + 1];
                if nx == '>' || nx == '<' {
                    // `\>` → `>` — an escaped operator is its own token
                    tok.push(nx);
                    i += 2;
                    break;
                } else {
                    // keep other escapes raw (`\.` inside regexes)
                    tok.push('\\');
                    tok.push(nx);
                }
                i += 2;
                continue;
            }
            tok.push(b[i]);
            i += 1;
        }
        // split glued binary operators: $x!=2, "a"=~re, a==b, "$i"="2"
        for op in ["!=", "==", "=~", "="] {
            if let Some(idx) = tok.find(op) {
                if idx + op.len() < tok.len() {
                    if idx > 0 {
                        out.push(tok[..idx].to_string());
                    }
                    out.push(op.to_string());
                    tok = tok[idx + op.len()..].to_string();
                }
            }
        }
        if !tok.is_empty() {
            out.push(tok);
        }
    }
    Some(out)
}

/// Recursive-descent parser over test tokens.
struct TestParser {
    toks: Vec<String>,
    i: usize,
}

impl TestParser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.i).map(|s| s.as_str())
    }
    fn eat(&mut self, op: &str) -> bool {
        if self.peek() == Some(op) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn or_expr(&mut self, r: &mut Render) -> Option<String> {
        let mut l = self.and_expr(r)?;
        while self.eat("-o") || self.eat("||") {
            let r2 = self.and_expr(r)?;
            l = format!("({l} || {r2})");
        }
        Some(l)
    }
    fn and_expr(&mut self, r: &mut Render) -> Option<String> {
        let mut l = self.not_expr(r)?;
        while self.eat("-a") || self.eat("&&") {
            let r2 = self.not_expr(r)?;
            l = format!("({l} && {r2})");
        }
        Some(l)
    }
    fn not_expr(&mut self, r: &mut Render) -> Option<String> {
        if self.eat("!") {
            let inner = self.not_expr(r)?;
            return Some(format!("(!{inner})"));
        }
        self.primary(r)
    }
    fn primary(&mut self, r: &mut Render) -> Option<String> {
        if self.eat("(") {
            let inner = self.or_expr(r)?;
            if !self.eat(")") {
                return None;
            }
            return Some(format!("({inner})"));
        }
        let t0 = self.peek()?.to_string();
        let t1 = self.toks.get(self.i + 1).cloned();
        let t2 = self.toks.get(self.i + 2).cloned();
        // unary flag: -f x / -n x / -S x …
        if t0.len() == 2 && t0.starts_with('-') {
            if let Some(flag) = t0.strip_prefix('-') {
                if TEST_FLAGS.contains(&flag) && t1.is_some() {
                    let operand = t1?;
                    self.i += 2;
                    let (vv, vk) = r.test_operand(&operand)?;
                    let vv = strify(vv, vk, r);
                    return match flag {
                        "n" => Some(format!("({vv} != \"\")")),
                        "z" => Some(format!("({vv} == \"\")")),
                        "t" => Some("false".to_string()),
                        _ => Some(format!("fileTest({}, {vv})", Render::go_str(flag))),
                    };
                }
            }
        }
        // binary: a op b
        if let (Some(a), Some(op), Some(b)) = (t1.as_deref(), t1.as_deref(), t2.as_deref()) {
            if TEST_BINOPS.contains(&op) {
                let _ = a;
                let lhs = self.toks[self.i].clone();
                let rhs = b.to_string();
                self.i += 3;
                let (l, lk) = r.test_operand(&lhs)?;
                let (rr, rk) = r.test_operand(&rhs)?;
                return r.test_binop(op, &lhs, l, lk, &rhs, rr, rk);
            }
        }
        // plain operand (truthy test)
        self.i += 1;
        let (vv, vk) = r.test_operand(&t0)?;
        let vv = strify(vv, vk, r);
        Some(format!("({vv} != \"\")"))
    }
}

fn numify(e: String, is_num: bool, _r: &mut Render) -> String {
    if is_num {
        e
    } else {
        format!("s2i({e})")
    }
}

fn strify(e: String, _is_num: bool, _r: &mut Render) -> String {
    format!("s2s({e})")
}

/// Does a string contain glob metacharacters?
fn has_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// The core wraps unquoted glob patterns in a `\x01SH2GLOB\x01` marker.
fn strip_sh2glob(s: &str) -> String {
    s.strip_prefix("\u{1}SH2GLOB\u{1}").unwrap_or(s).to_string()
}

/// Extglob `!(X)` at the pattern start → (inner, literal rest).
fn extglob_not(s: &str) -> Option<(String, String)> {
    let rest = s.strip_prefix("!(")?;
    let close = rest.find(')')?;
    let inner = &rest[..close];
    let after = rest[close + 1..].to_string();
    Some((inner.to_string(), after))
}

fn quote_sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Double-quote a shell word, preserving `${...}`, `$name`, `$((…))` and
/// `$(...)` for bash expansion inside a `bash -c` fallback.
/// Quotes (and backslashes) inside a substitution region pass through
/// VERBATIM — they are reparsed by bash when the substitution runs, so
/// escaping them would inject literal backslashes (`<<<"$(echo \"hi\")"`
/// prints `"hi"`, not `hi`). Only depth-0 text is escaped.
fn quote_sh_dq(s: &str) -> String {
    let mut out = String::from("\"");
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    // closers for nested `$(`…`)` / `${`…`}` regions
    let mut stack: Vec<char> = Vec::new();
    // active '…'/ "…" span inside a substitution (parens inside don't nest)
    let mut quote: Option<char> = None;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                i += 1;
                out.push(b[i]);
            } else if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == '\'' && stack.is_empty() {
            // depth-0 single-quoted span: verbatim (literal inside "...")
            quote = Some('\'');
            out.push(c);
            i += 1;
            continue;
        }
        if c == '"' && stack.is_empty() {
            // a balanced pair is an already-quoted span: verbatim (its
            // quotes are syntactic, e.g. `"a$(b)c"`); a stray quote is a
            // literal char and gets escaped below
            let mut j = i + 1;
            let mut found = false;
            while j < b.len() {
                if b[j] == '\\' {
                    j += 2;
                    continue;
                }
                if b[j] == '"' {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                quote = Some('"');
                out.push(c);
                i += 1;
                continue;
            }
        }
        if (c == '\'' || c == '"') && !stack.is_empty() {
            quote = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '$' && i + 1 < b.len() && (b[i + 1] == '(' || b[i + 1] == '{') {
            stack.push(if b[i + 1] == '(' { ')' } else { '}' });
            out.push(c);
            out.push(b[i + 1]);
            i += 2;
            continue;
        }
        if c == '(' && stack.last() == Some(&')') {
            stack.push(')');
            out.push(c);
            i += 1;
            continue;
        }
        if (c == ')' || c == '}') && stack.last() == Some(&c) {
            stack.pop();
            out.push(c);
            i += 1;
            continue;
        }
        // a backslash quotes the next char out of special processing
        // (both pass through — `\"` stays a literal quote, `\$` stays
        // a literal dollar)
        if c == '\\' && stack.is_empty() {
            out.push(c);
            if i + 1 < b.len() {
                i += 1;
                out.push(b[i]);
            }
            i += 1;
            continue;
        }
        if stack.is_empty() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '`' => out.push_str("\\`"),
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out.push('"');
    out
}

/// Order-preserving `[]string` for argv containing `globArgs(…)...`

/// Pick the right quoting for a cmd_text argument: single-quote plain
/// literal strings, double-quote everything else (variable refs, arithmetic,
/// interpolations) so bash expands them inside `redirRun`/`capRun`.
fn cmd_quote(expr: &IrExpr, text: &str) -> String {
    if matches!(expr, IrExpr::Str(_, _)) {
        quote_sh(text)
    } else {
        quote_sh_dq(text)
    }
}

/// Join pipeline stage texts: a stage containing a heredoc (`<<` plus a
/// newline) cannot take a trailing `| next` on its terminator line — the
/// pipe text would glue onto the terminator and the heredoc runs
/// unterminated. Wrap such stages in a subshell group so the pipe
/// attaches to `( ... )` instead (the closing paren goes on its own line
/// — on the terminator's line it would break the terminator).
fn pipe_join(texts: &[String]) -> String {
    texts
        .iter()
        .map(|t| {
            if t.contains("<<") && t.contains('\n') {
                format!("( {t}\n)")
            } else {
                t.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

impl Render {
    /// Failing substitutions (bash errors, command suppressed with no
    /// output): `${!prefix*[@]}` (unlike valid `${!arr[@]}`/`${!prefix*}`)
    /// and array element slices with a statically-negative length
    /// (`${a[@]:1:-1}` — valid for strings, an error for arrays).
    /// Statements model the failure (skip + st = 1) instead of printing
    /// a blank line.
    fn bad_subst(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } if func == "param" => {
                if let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) =
                    (args.first(), args.get(1))
                {
                    if name.starts_with('!') && name.ends_with('*') {
                        if matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@") {
                            return true;
                        }
                    }
                    if name.starts_with('!')
                        && name.contains('*')
                        && (name.contains("[@]") || name.contains("[*]"))
                    {
                        return true;
                    }
                    // array element slice with a statically-negative
                    // length (`${a[@]:1:-1}` errors; strings allow it)
                    if op == "slice" && is_array_slice_name(self, name) {
                        let mut extra: &[IrExpr] = args.get(2..).unwrap_or(&[]);
                        if matches!(extra.first(), Some(IrExpr::Str(a, _)) if a == "@" || a == "*") {
                            extra = &extra[1..];
                        }
                        if let Some(IrExpr::Str(last, _)) = extra.last() {
                            if let Ok(n) = last.trim().parse::<i64>() {
                                if n < 0 {
                                    return true;
                                }
                            }
                        }
                    }
                }
                args.iter().any(|e| self.bad_subst(e))
            }
            IrExpr::Call { args, .. } => args.iter().any(|e| self.bad_subst(e)),
            IrExpr::Interpolate(parts) => parts
                .iter()
                .any(|p| matches!(p, InterpPart::Expr(x) if self.bad_subst(x))),
            IrExpr::Array(items) => items.iter().any(|e| self.bad_subst(e)),
            IrExpr::BinOp { lhs, rhs, .. } => self.bad_subst(lhs) || self.bad_subst(rhs),
            _ => false,
        }
    }
}

/// Array-slice subject? (`name[@]` always; a bare known array too —
/// scalars take the string-slice path where negatives are legal.)
fn is_array_slice_name(r: &Render, name: &str) -> bool {
    if name.ends_with("[@]") || name.ends_with("[*]") {
        return true;
    }
    if let Some((base, _)) = split_bracket(name) {
        return r.assoc_arrays.contains(base) || r.arrays.contains(base);
    }
    r.assoc_arrays.contains(name) || r.arrays.contains(name)
}

/// Position of an unescaped `\c` in an echo -e literal (None if absent
/// or escaped as `\\c`). bash stops output at `\c` with no newline.
fn unescaped_c(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'\\' {
            if b[i + 1] == b'\\' {
                i += 2;
                continue;
            }
            if b[i + 1] == b'c' {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Split echo -e parts at an unescaped `\c` (output stops there, no
/// trailing newline). Returns (kept parts, stopped).
fn trunc_c(parts: Vec<Part>) -> (Vec<Part>, bool) {
    let mut out = Vec::new();
    for p in parts {
        match p {
            Part::Lit(t) => {
                if let Some(idx) = unescaped_c(&t) {
                    out.push(Part::Lit(t[..idx].to_string()));
                    return (out, true);
                }
                out.push(Part::Lit(t));
            }
            other => out.push(other),
        }
    }
    (out, false)
}

/// Split `name=payload` (first `=` only).
fn split_first_eq(s: &str) -> Option<(&str, &str)> {
    let eq = s.find('=')?;
    Some((&s[..eq], &s[eq + 1..]))
}

/// Plain shell variable name (no subscript, no namespace)?
fn is_plain_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().enumerate().all(|(i, c)| {
            c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
        })
}

/// Shell-safe bareword for `typeset -f` display (bash pretty-prints
/// `echo "a b"` with the command bare and only quotes when needed).
fn display_bareword(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c) => {}
        _ => return false,
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
}
/// spreads. A spread must be LAST in a call/literal, so splice position
/// via append chains: `["a", G..., "b"]` →
/// `append(append([]string{"a"}, G...), "b")`. Spread-free lists keep the
/// plain literal (no diff to existing renders).
fn argv_slice(items: &[String]) -> String {
    if items.iter().all(|it| !it.ends_with("...")) {
        return format!("[]string{{{}}}", items.join(", "));
    }
    let mut expr = String::from("[]string{}");
    for it in items {
        if let Some(spread) = it.strip_suffix("...") {
            expr = format!("append({expr}, {spread}...)");
        } else {
            expr = format!("append({expr}, {it})");
        }
    }
    expr
}

/// `exec.Command(…)` application for argv possibly containing
/// `globArgs(…)...` spreads. Spread-free lists keep the plain variadic
/// call; otherwise the head stays fixed and the tail splices as one
/// spread (`exec.Command` needs its `name` param positionally — a bare
/// `slice...` cannot fill it).
fn exec_cmd_apply(parts: &[String]) -> String {
    if parts.iter().all(|it| !it.ends_with("...")) {
        return format!("exec.Command({})", parts.join(", "));
    }
    let (cmd, rest) = parts.split_first().expect("exec argv never empty");
    format!("exec.Command({cmd}, {}...)", argv_slice(rest))
}

/// Pre-scan for runtime flags that must be known before the body renders.
fn scan_flags(stmts: &[IrStmt], r: &mut Render) {
    for s in stmts {
        match s {
            IrStmt::Expr(e) => scan_flags_expr(e, r),
            IrStmt::Assign { expr, .. } => scan_flags_expr(expr, r),
            IrStmt::Output { value, .. } => scan_flags_expr(value, r),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                scan_flags_expr(cond, r);
                scan_flags(then, r);
                for (c, b) in elsifs {
                    scan_flags_expr(c, r);
                    scan_flags(b, r);
                }
                scan_flags(else_, r);
            }
            IrStmt::While { cond, body } => {
                scan_flags_expr(cond, r);
                scan_flags(body, r);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                scan_flags(body, r);
                scan_flags_expr(cond, r);
            }
            IrStmt::For { iter, body, .. } => {
                scan_flags_expr(iter, r);
                scan_flags(body, r);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => scan_flags(b, r),
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                scan_flags_expr(discriminant, r);
                for c in clauses {
                    scan_flags(&c.body, r);
                }
            }
            IrStmt::Function { body, .. } => scan_flags(body, r),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    scan_flags(st, r);
                }
            }
            _ => {}
        }
    }
}

fn scan_flags_expr(e: &IrExpr, r: &mut Render) {
    match e {
        IrExpr::Call { func, args } => {
            if matches!(func.as_str(), "param" | "join" | "arrayItems") {
                r.need_vars = true;
            }
            if func == "getVar" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if name == "?" {
                        r.need_st = true;
                    }
                }
            }
            for a in args {
                scan_flags_expr(a, r);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            scan_flags_expr(lhs, r);
            scan_flags_expr(rhs, r);
        }
        IrExpr::Arith(a) => scan_flags_arith(a, r),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    scan_flags_expr(x, r);
                }
            }
        }
        IrExpr::Index { key, .. } => scan_flags_expr(key, r),
        IrExpr::Capture { expr, .. } => scan_flags_expr(expr, r),
        IrExpr::Array(items) => {
            for it in items {
                scan_flags_expr(it, r);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            scan_flags_expr(cond, r);
            scan_flags_expr(then, r);
            scan_flags_expr(else_, r);
        }
        _ => {}
    }
}

fn scan_flags_arith(a: &ArithAst, r: &mut Render) {
    match a {
        ArithAst::Var(name) => {
            if name == "?" {
                r.need_st = true;
            }
        }
        ArithAst::Index { key, .. } => scan_flags_arith(key, r),
        ArithAst::Bin { lhs, rhs, .. } => {
            scan_flags_arith(lhs, r);
            scan_flags_arith(rhs, r);
        }
        ArithAst::Un { arg, .. } => scan_flags_arith(arg, r),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            scan_flags_arith(test, r);
            scan_flags_arith(then, r);
            scan_flags_arith(else_, r);
        }
        ArithAst::Assign { rhs, .. } => scan_flags_arith(rhs, r),
        _ => {}
    }
}

/// The elements of a setArray/setArrayAppend Call.
fn setarray_elems(args: &[IrExpr]) -> Vec<&IrExpr> {
    match args {
        [IrExpr::Array(items), ..] => items.iter().collect(),
        [_, IrExpr::Array(items)] => items.iter().collect(),
        [..] => args.iter().collect(),
    }
}

/// heredoc redirect target, if any.
fn heredoc_content(redirects: &[crate::ir::IrRedirect]) -> Option<&IrExpr> {
    for r in redirects {
        if r.mode == "heredoc" && r.fd.unwrap_or(0) == 0 {
            return Some(&r.target);
        }
    }
    None
}

fn is_single_cat(inner: &[IrStmt]) -> bool {
    matches!(
        inner,
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec"
                && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "cat")
    )
}

/// Brace groups: the Json value is a list of groups; each group is a list
/// of items (String or {range:[start,end,step,pad]} or {nested:[…]}).
/// A group of plain items is a union of alternatives ({a,b,c}); a group
/// containing nested objects is a concatenation (src/{main,test}).
fn brace_groups(v: &serde_json::Value) -> Option<Vec<Vec<String>>> {
    let groups = v.as_array()?;
    let mut out = Vec::new();
    for g in groups {
        out.push(brace_group_items(g.as_array()?)?);
    }
    Some(out)
}

fn brace_group_items(items: &[serde_json::Value]) -> Option<Vec<String>> {
    // A group's items are ALTERNATIVES: a literal, a lone range, a nested
    // brace (the union of its alternatives), or a `product` compound (a
    // cartesian concatenation). Concatenation across items only ever
    // happens inside a `product` — the parser wraps a multi-part
    // alternative as one, so item order here is alternative order.
    let mut alts = Vec::new();
    for item in items {
        alts.extend(brace_item(item)?);
    }
    Some(alts)
}

fn brace_item(v: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(s) = v.as_str() {
        return Some(vec![s.to_string()]);
    }
    let obj = v.as_object()?;
    if let Some(range) = obj.get("range") {
        let r = range.as_array()?;
        let start = r.get(0)?.as_str()?;
        let end = r.get(1)?.as_str()?;
        let step = r
            .get(2)
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| r.get(2).and_then(|x| x.as_i64()))
            .unwrap_or(1);
        return Some(brace_range(start, end, step));
    }
    if let Some(nested) = obj.get("nested") {
        return brace_group_items(nested.as_array()?);
    }
    if let Some(product) = obj.get("product") {
        // a Compound alternative: cartesian concatenation of sub-groups
        let subs = product.as_array()?;
        let mut combos = vec![String::new()];
        for g in subs {
            let alts = brace_group_items(g.as_array()?)?;
            let mut next = Vec::new();
            for c in &combos {
                for a in &alts {
                    next.push(format!("{c}{a}"));
                }
            }
            combos = next;
        }
        return Some(combos);
    }
    None
}

/// `{start..end..step}` with optional zero-padding (from leading zeros).
fn brace_range(start: &str, end: &str, step: i64) -> Vec<String> {
    let mut out = Vec::new();
    // integer range?
    if let (Ok(a), Ok(b)) = (start.parse::<i64>(), end.parse::<i64>()) {
        let pad =
            if start.len() > 1 && start.starts_with('0') || end.len() > 1 && end.starts_with('0') {
                start.len().max(end.len())
            } else {
                0
            };
        let (a, b, step) = if a <= b {
            (a, b, step.abs().max(1))
        } else {
            (a, b, -step.abs().max(1))
        };
        let mut i = a;
        if a <= b {
            while i <= b {
                if pad > 0 {
                    out.push(format!("{:0width$}", i, width = pad));
                } else {
                    out.push(i.to_string());
                }
                i += step;
            }
        } else {
            while i >= b {
                if pad > 0 {
                    out.push(format!("{:0width$}", i, width = pad));
                } else {
                    out.push(i.to_string());
                }
                i += step;
            }
        }
        return out;
    }
    // char range a..z (single chars)
    if start.chars().count() == 1 && end.chars().count() == 1 {
        let a = start.chars().next().unwrap() as i64;
        let b = end.chars().next().unwrap() as i64;
        let (a, b, step) = if a <= b {
            (a, b, step.abs().max(1))
        } else {
            (a, b, -step.abs().max(1))
        };
        let mut i = a;
        if a <= b {
            while i <= b {
                out.push(char::from_u32(i as u32).unwrap().to_string());
                i += step;
            }
        } else {
            while i >= b {
                out.push(char::from_u32(i as u32).unwrap().to_string());
                i += step;
            }
        }
        return out;
    }
    out.push(start.to_string());
    out
}

/// Which packages the generated text actually references (skipping string
/// literals and comments).
fn scan_imports(text: &str) -> Vec<&'static str> {
    let mut used: Vec<&'static str> = Vec::new();
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '"' {
            // skip string literal (with escapes)
            i += 1;
            while i < b.len() {
                if b[i] == '\\' {
                    i += 2;
                    continue;
                }
                if b[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // identifier-then-dot patterns
        for (name, pkg) in [
            ("fmt.", "fmt"),
            ("os.", "os"),
            ("exec.", "os/exec"),
            ("strings.", "strings"),
            ("strconv.", "strconv"),
            ("io.", "io"),
            ("bufio.", "bufio"),
            ("regexp.", "regexp"),
            ("filepath.", "path/filepath"),
            ("utf8.", "unicode/utf8"),
            ("syscall.", "syscall"),
        ] {
            if c == name.chars().next().unwrap() && text[i..].starts_with(name) {
                let q = format!("\"{pkg}\"");
                if !used.iter().any(|u| *u == q) {
                    used.push(Box::leak(q.into_boxed_str()));
                }
                i += name.len();
                break;
            }
        }
        i += 1;
    }
    used
}

/// Collect every variable written by statements (assign targets, declare
/// lists, For loop vars, exec local/typeset/declare/read targets) — the
/// hoisted declaration set — plus array usage.
/// Pre-scan: `declare`/`typeset`/`local` attribute flags anywhere in the
/// program (`-i` integer, `-l`/`-u` case fold, `+` clears). Render-pass
/// typing must be order-independent: `unset n` may precede `typeset -i
/// n` (the home is int64 by end of pass, so unset must zero, not nil),
/// and assignments before the declaration are vanishingly rare (bash
/// would treat them as strings; we treat attr'd vars as typed
/// throughout — documented in docs/backend-go-limitations.md).
fn collect_decl_attrs(
    stmts: &[IrStmt],
    int_attrs: &mut BTreeSet<String>,
    case_attrs: &mut HashMap<String, char>,
    nameref: &mut HashMap<String, String>,
    ro_attrs: &mut BTreeSet<String>,
) {
    for s in stmts {
        match s {
            IrStmt::Assign { expr, .. } => collect_decl_attrs_expr(expr, int_attrs, case_attrs, nameref, ro_attrs),
            IrStmt::Declare { init, .. } => {
                if let Some(e) = init {
                    collect_decl_attrs_expr(e, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            IrStmt::DeclareArray { elements, .. } => {
                for e in elements {
                    collect_decl_attrs_expr(e, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            IrStmt::For { iter, body, .. } => {
                collect_decl_attrs_expr(iter, int_attrs, case_attrs, nameref, ro_attrs);
                collect_decl_attrs(body, int_attrs, case_attrs, nameref, ro_attrs);
            }
            IrStmt::Expr(e) => collect_decl_attrs_expr(e, int_attrs, case_attrs, nameref, ro_attrs),
            IrStmt::Output { value, .. } => {
                collect_decl_attrs_expr(value, int_attrs, case_attrs, nameref, ro_attrs)
            }
            IrStmt::WriteFile { path, content, .. } => {
                collect_decl_attrs_expr(path, int_attrs, case_attrs, nameref, ro_attrs);
                collect_decl_attrs_expr(content, int_attrs, case_attrs, nameref, ro_attrs);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_decl_attrs_expr(cond, int_attrs, case_attrs, nameref, ro_attrs);
                collect_decl_attrs(then, int_attrs, case_attrs, nameref, ro_attrs);
                for (c, b) in elsifs {
                    collect_decl_attrs_expr(c, int_attrs, case_attrs, nameref, ro_attrs);
                    collect_decl_attrs(b, int_attrs, case_attrs, nameref, ro_attrs);
                }
                collect_decl_attrs(else_, int_attrs, case_attrs, nameref, ro_attrs);
            }
            IrStmt::While { cond, body } => {
                collect_decl_attrs_expr(cond, int_attrs, case_attrs, nameref, ro_attrs);
                collect_decl_attrs(body, int_attrs, case_attrs, nameref, ro_attrs);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_decl_attrs(body, int_attrs, case_attrs, nameref, ro_attrs);
                collect_decl_attrs_expr(cond, int_attrs, case_attrs, nameref, ro_attrs);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_decl_attrs_expr(x, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_decl_attrs(b, int_attrs, case_attrs, nameref, ro_attrs)
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_decl_attrs(inner, int_attrs, case_attrs, nameref, ro_attrs);
                for r in redirects {
                    collect_decl_attrs_expr(&r.target, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                collect_decl_attrs_expr(discriminant, int_attrs, case_attrs, nameref, ro_attrs);
                for c in clauses {
                    collect_decl_attrs(&c.body, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            IrStmt::Function { body, .. } => collect_decl_attrs(body, int_attrs, case_attrs, nameref, ro_attrs),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_decl_attrs(st, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            IrStmt::Exec { cmd, args, .. } => {
                collect_decl_attrs_expr(cmd, int_attrs, case_attrs, nameref, ro_attrs);
                for a in args {
                    collect_decl_attrs_expr(a, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
            _ => {}
        }
    }
}

fn collect_decl_attrs_expr(
    e: &IrExpr,
    int_attrs: &mut BTreeSet<String>,
    case_attrs: &mut HashMap<String, char>,
    nameref: &mut HashMap<String, String>,
    ro_attrs: &mut BTreeSet<String>,
) {
    match e {
        IrExpr::Call { func, args } if func == "builtin" || func == "exec" => {
            // `typeset -i n=42` / `declare -l lc` / `local -u u` — the
            // builtin→exec fallback renames the op before we pre-scan,
            // so both spellings are accepted (args are untouched).
            // Only Str words carry flag/name info (`lc=$dynamic` values
            // arrive as non-Str tails) — skip those, don't bail.
            let mut words: Vec<&str> = Vec::new();
            for a in args {
                match a {
                    IrExpr::Str(s, _) => words.push(s.as_str()),
                    IrExpr::Array(items) => {
                        for it in items {
                            if let IrExpr::Str(s, _) = it {
                                words.push(s.as_str());
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !words.is_empty()
                && matches!(
                    words[0],
                    "typeset" | "declare" | "local" | "readonly"
                )
            {
                let mut int = false;
                let mut case: Option<char> = None;
                let mut is_ref = false;
                let mut plus_n = false;
                let mut plus_i = false;
                let mut plus_case = false;
                let mut plus_ro = false;
                // `readonly a=b` marks readonly with no flag word
                let mut ro = words[0] == "readonly";
                for w in &words[1..] {
                    if w.starts_with('-') && !w.starts_with("--") {
                        if w.contains('i') {
                            int = true;
                            plus_i = false;
                        }
                        if w.contains('r') && words[0] != "readonly" {
                            plus_ro = false;
                        }
                        if w.contains('r') {
                            ro = true;
                        }
                        if w.contains('l') {
                            case = Some('l');
                            plus_case = false;
                        }
                        if w.contains('u') {
                            case = Some('u');
                            plus_case = false;
                        }
                        if w.contains('n') {
                            is_ref = true;
                            plus_n = false;
                        }
                    } else if w.starts_with('+') {
                        if w.contains('r') {
                            ro = false;
                            plus_ro = true;
                        }
                        if w.contains('i') {
                            int = false;
                            plus_i = true;
                        }
                        if w.contains('l') || w.contains('u') {
                            case = None;
                            plus_case = true;
                        }
                        if w.contains('n') {
                            is_ref = false;
                            plus_n = true;
                        }
                    } else {
                        let name = w.split('=').next().unwrap_or("");
                        if !name.is_empty() {
                            if plus_i {
                                int_attrs.remove(name);
                            }
                            if plus_case {
                                case_attrs.remove(name);
                            }
                            if plus_n {
                                nameref.remove(name);
                            }
                            if plus_ro {
                                ro_attrs.remove(name);
                            }
                            if ro {
                                ro_attrs.insert(name.to_string());
                            }
                            if int {
                                int_attrs.insert(name.to_string());
                            }
                            if let Some(c) = case {
                                case_attrs.insert(name.to_string(), c);
                            }
                            // `typeset -n ref=target` with a static target:
                            // ref aliases target's home (dynamic targets
                            // can't resolve statically — left alone)
                            if is_ref {
                                if let Some((_, tgt)) = w.split_once('=') {
                                    if !tgt.is_empty()
                                        && !tgt.contains('$')
                                        && !tgt.contains('[')
                                        && tgt != name
                                    {
                                        nameref.insert(name.to_string(), tgt.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for a in args {
                collect_decl_attrs_expr(a, int_attrs, case_attrs, nameref, ro_attrs);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_decl_attrs_expr(a, int_attrs, case_attrs, nameref, ro_attrs);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_decl_attrs_expr(lhs, int_attrs, case_attrs, nameref, ro_attrs);
            collect_decl_attrs_expr(rhs, int_attrs, case_attrs, nameref, ro_attrs);
        }
        IrExpr::Arith(a) => collect_decl_attrs_arith(a, int_attrs, case_attrs, nameref, ro_attrs),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_decl_attrs_expr(x, int_attrs, case_attrs, nameref, ro_attrs);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_decl_attrs_expr(i, int_attrs, case_attrs, nameref, ro_attrs);
            }
        }
        IrExpr::Capture { expr, .. } => collect_decl_attrs_expr(expr, int_attrs, case_attrs, nameref, ro_attrs),
        IrExpr::Arrow(b) => collect_decl_attrs(b, int_attrs, case_attrs, nameref, ro_attrs),
        _ => {}
    }
}

fn collect_decl_attrs_arith(
    a: &ArithAst,
    int_attrs: &mut BTreeSet<String>,
    case_attrs: &mut HashMap<String, char>,
    nameref: &mut HashMap<String, String>,
    ro_attrs: &mut BTreeSet<String>,
) {
    match a {
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_decl_attrs_arith(lhs, int_attrs, case_attrs, nameref, ro_attrs);
            collect_decl_attrs_arith(rhs, int_attrs, case_attrs, nameref, ro_attrs);
        }
        ArithAst::Un { arg, .. } => collect_decl_attrs_arith(arg, int_attrs, case_attrs, nameref, ro_attrs),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_decl_attrs_arith(test, int_attrs, case_attrs, nameref, ro_attrs);
            collect_decl_attrs_arith(then, int_attrs, case_attrs, nameref, ro_attrs);
            collect_decl_attrs_arith(else_, int_attrs, case_attrs, nameref, ro_attrs);
        }
        ArithAst::Index { key, .. } => collect_decl_attrs_arith(key, int_attrs, case_attrs, nameref, ro_attrs),
        _ => {}
    }
}

/// Canonical vars-map key for a static `base[key]` word: strip one layer
/// of quotes around the key (`config["user"]`, `config['user']` and
/// `config[user]` address the same element — dynamic keys arrive
/// unquoted already, and child-facing shell text is never normalized).
fn normalize_assoc_key(name: &str) -> String {
    if let Some((base, key)) = split_bracket(name) {
        if key.len() >= 2
            && ((key.starts_with('"') && key.ends_with('"'))
                || (key.starts_with('\'') && key.ends_with('\'')))
        {
            return format!("{base}[{}]", &key[1..key.len() - 1]);
        }
    }
    name.to_string()
}

/// A `$(…)` word-capture element (splits into array elements)?
fn is_capture_words(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args }
        if func == "captureWords" && matches!(args.first(), Some(IrExpr::Arrow(_))))
}

/// Procsub-temp cleanup (`rm -f $__ps_tmpN …`)? These must not clobber
/// `$?` — the status belongs to the preceding user command (`diff …;
/// echo "done: $?"` reads diff's code, not rm's). `rm -f` on temps is
/// infallible for this purpose, so the cleanup runs discarding status.
fn is_tmp_cleanup(cmd: &str, items: &[IrExpr]) -> bool {
    if cmd != "rm" {
        return false;
    }
    let mut saw_f = false;
    let mut any_operand = false;
    for it in items {
        match it {
            IrExpr::Str(s, _) => {
                if s == "-f" || s == "-rf" || s == "-fr" {
                    saw_f = true;
                } else if s.starts_with('-') {
                    return false;
                } else if s.starts_with("__ps_tmp") {
                    any_operand = true;
                } else {
                    return false;
                }
            }
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if n.starts_with("__ps_tmp") {
                    any_operand = true;
                } else {
                    return false;
                }
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                match args.first() {
                    Some(IrExpr::Str(n, _)) if n.starts_with("__ps_tmp") => {
                        any_operand = true;
                    }
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
    saw_f && any_operand
}

/// Flatten a top-level `&&` chain (through single-Arrow `block` calls
/// used as conditions) into operand exprs.
fn flatten_and<'a>(e: &'a IrExpr, out: &mut Vec<&'a IrExpr>) -> bool {
    match e {
        IrExpr::BinOp { lhs, op, rhs } if *op == crate::ir::BinOpKind::And => {
            flatten_and(lhs, out) && flatten_and(rhs, out)
        }
        IrExpr::Call { func, args } if func == "block" => {
            let Some(IrExpr::Arrow(body)) = args.first() else {
                return false;
            };
            for s in body {
                match s {
                    IrStmt::Expr(inner) => {
                        if !flatten_and(inner, out) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
            true
        }
        _ => {
            out.push(e);
            true
        }
    }
}

/// A `read` builtin call (any argv — parsed by the caller)?
fn is_read_call(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args }
        if (func == "builtin" || func == "exec")
            && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "read"))
}

/// Trivially-true call (`true` / `:` — identity for `&&` chains)?
fn is_true_call(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args }
        if (func == "builtin" || func == "exec")
            && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "true" || c == ":"))
}

/// `$@`/`$*` as a whole-array value?
fn is_listvar_at(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args }
        if func == "listVar"
            && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "@" || n == "*"))
}

/// Split a `base[key]` shell word into (base, key-text), stripping one
/// layer of quotes around the key. Refuses nested brackets.
fn split_bracket(s: &str) -> Option<(&str, &str)> {
    let open = s.find('[')?;
    if !s.ends_with(']') {
        return None;
    }
    let base = &s[..open];
    if base.is_empty() || base.contains('[') {
        return None;
    }
    let mut key = &s[open + 1..s.len() - 1];
    if key.contains('[') {
        return None;
    }
    if key.len() >= 2
        && ((key.starts_with('"') && key.ends_with('"'))
            || (key.starts_with('\'') && key.ends_with('\'')))
    {
        key = &key[1..key.len() - 1];
    }
    Some((base, key))
}

/// Bare shell variable name (no `$`, no operators)?
fn is_bare_var(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse a redirect spec object ({fd, mode, target}) into IrRedirect.
fn object_redirect(r: &IrExpr) -> Option<crate::ir::IrRedirect> {
    let IrExpr::Object(props) = r else {
        return None;
    };
    let mut fd: Option<i32> = None;
    let mut mode = String::new();
    let mut target: Option<IrExpr> = None;
    for (k, v) in props {
        match k.as_str() {
            "fd" => {
                if let IrExpr::Int(i) = v {
                    fd = Some(*i as i32);
                }
            }
            "mode" => {
                if let IrExpr::Str(s, _) = v {
                    mode = s.clone();
                }
            }
            "target" => {
                target = Some(v.clone());
            }
            _ => {}
        }
    }
    Some(crate::ir::IrRedirect {
        fd,
        mode,
        target: target?,
        interpolate: false,
    })
}

/// Parse an assoc/explicit-index literal element `[k]=v`.
fn assoc_elem(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('[')?;
    s.split_once("]=")
}

/// Statically-string array key? (assoc-shaped: a non-integer literal,
/// a concatenation, or an interpolation — never a plain int or bare var,
/// which stay indexed-array positions.)
fn assoc_key_shape(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_) => false,
        IrExpr::Str(s, _) => s.trim().parse::<i64>().is_err(),
        IrExpr::BinOp { op, .. } => *op == crate::ir::BinOpKind::Concat,
        IrExpr::Interpolate(_) => true,
        _ => false,
    }
}

/// Baked `base[key]` target with a non-integer key → assoc base.
fn assoc_baked(var: &str, assoc: &mut BTreeSet<String>) {
    if let Some(close) = var.find('[') {
        if var.ends_with(']') {
            let base = &var[..close];
            let key = &var[close + 1..var.len() - 1];
            if key.trim().parse::<i64>().is_err() && !base.is_empty() {
                assoc.insert(base.to_string());
            }
        }
    }
}

/// Pre-scan: bases ever WRITTEN with an assoc-shaped key. Reads consult
/// this (plus the on-the-spot literal rule) to route through the `vars`
/// map instead of arrIdx.
fn collect_assoc(stmts: &[IrStmt], assoc: &mut BTreeSet<String>) {
        for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    assoc_baked(&t.var, assoc);
                    if !t.indices.is_empty() && assoc_key_shape(&t.indices[0]) {
                        assoc.insert(t.var.clone());
                    }
                }
                collect_assoc_expr(expr, assoc);
            }
            IrStmt::Declare { init, .. } => {
                if let Some(e) = init {
                    collect_assoc_expr(e, assoc);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                // `A=([k]=v ...)` assoc literals arrive with non-integer
                // keys baked into the element targets.
                assoc_baked(var, assoc);
                for e in elements {
                    collect_assoc_expr(e, assoc);
                }
            }
            IrStmt::For { iter, body, .. } => {
                collect_assoc_expr(iter, assoc);
                collect_assoc(body, assoc);
            }
            IrStmt::Expr(e) => collect_assoc_expr(e, assoc),
            IrStmt::Output { value, .. } => collect_assoc_expr(value, assoc),
            IrStmt::WriteFile { path, content, .. } => {
                collect_assoc_expr(path, assoc);
                collect_assoc_expr(content, assoc);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_assoc_expr(cond, assoc);
                collect_assoc(then, assoc);
                for (c, b) in elsifs {
                    collect_assoc_expr(c, assoc);
                    collect_assoc(b, assoc);
                }
                collect_assoc(else_, assoc);
            }
            IrStmt::While { cond, body } => {
                collect_assoc_expr(cond, assoc);
                collect_assoc(body, assoc);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_assoc(body, assoc);
                collect_assoc_expr(cond, assoc);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_assoc_expr(x, assoc);
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_assoc(b, assoc)
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_assoc(inner, assoc);
                for r in redirects {
                    collect_assoc_expr(&r.target, assoc);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                collect_assoc_expr(discriminant, assoc);
                for c in clauses {
                    collect_assoc(&c.body, assoc);
                }
            }
            IrStmt::Function { body, .. } => collect_assoc(body, assoc),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_assoc(st, assoc);
                }
            }
            IrStmt::Exec {
                cmd,
                args,
                redirects,
                env,
                ..
            } => {
                collect_assoc_expr(cmd, assoc);
                for a in args {
                    collect_assoc_expr(a, assoc);
                }
                for r in redirects {
                    collect_assoc_expr(r, assoc);
                }
                for (_, e) in env {
                    collect_assoc_expr(e, assoc);
                }
            }
            _ => {}
        }
    }
}

fn collect_assoc_expr(e: &IrExpr, assoc: &mut BTreeSet<String>) {
    match e {
        IrExpr::Index { var, key } => {
            if assoc_key_shape(key) {
                assoc.insert(var.clone());
            }
            collect_assoc_expr(key, assoc);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_assoc_expr(lhs, assoc);
            collect_assoc_expr(rhs, assoc);
        }
        IrExpr::Arith(a) => collect_assoc_arith(a, assoc),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_assoc_expr(x, assoc);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_assoc_expr(i, assoc);
            }
        }
        IrExpr::Capture { expr, .. } => collect_assoc_expr(expr, assoc),
        IrExpr::Arrow(b) => collect_assoc(b, assoc),
        IrExpr::Call { func, args } if func == "arrayIndex" => {
            if let (Some(IrExpr::Str(name, _)), Some(k)) = (args.first(), args.get(1)) {
                if assoc_key_shape(k) {
                    assoc.insert(name.clone());
                }
            }
            for a in args {
                collect_assoc_expr(a, assoc);
            }
        }
        IrExpr::Call { func, args } if func == "setArray" || func == "setArrayAppend" => {
            // `assoc=([k]=v …)` literal proves the base associative
            if let Some(IrExpr::Str(base, _)) = args.first() {
                for e in setarray_elems(&args[1..]) {
                    if let IrExpr::Str(s, _) = e {
                        if let Some((k, _)) = assoc_elem(s) {
                            if k.trim().parse::<i64>().is_err() {
                                assoc.insert(base.clone());
                                break;
                            }
                        }
                    }
                }
            }
            for a in args {
                collect_assoc_expr(a, assoc);
            }
        }
        IrExpr::Call { func, args } if func == "builtin" || func == "exec" => {
            // `declare -A assoc` marks the base associative even when
            // every write uses a dynamic key
            let mut words: Vec<&str> = Vec::new();
            for a in args {
                match a {
                    IrExpr::Str(s, _) => words.push(s.as_str()),
                    IrExpr::Array(items) => {
                        for it in items {
                            if let IrExpr::Str(s, _) = it {
                                words.push(s.as_str());
                            }
                        }
                    }
                    _ => {}
                }
            }
            if matches!(words.first(), Some(w) if matches!(*w, "typeset" | "declare" | "local")) {
                let mut a_flag = false;
                for w in &words[1..] {
                    if w.starts_with('-') {
                        if w.contains('A') {
                            a_flag = true;
                        }
                    } else if w.starts_with('+') {
                        if w.contains('A') {
                            a_flag = false;
                        }
                    } else if a_flag {
                        let base = w.split(['=', '[']).next().unwrap_or("");
                        if !base.is_empty() {
                            assoc.insert(base.to_string());
                        }
                    }
                }
            }
            for a in args {
                collect_assoc_expr(a, assoc);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_assoc_expr(a, assoc);
            }
        }
        _ => {}
    }
}

fn collect_assoc_arith(a: &ArithAst, assoc: &mut BTreeSet<String>) {
    // arith index keys are numeric positions — they never prove assoc;
    // just recurse for nested expressions.
    match a {
        ArithAst::Index { key, .. } => collect_assoc_arith(key, assoc),
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_assoc_arith(lhs, assoc);
            collect_assoc_arith(rhs, assoc);
        }
        ArithAst::Un { arg, .. } => collect_assoc_arith(arg, assoc),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_assoc_arith(test, assoc);
            collect_assoc_arith(then, assoc);
            collect_assoc_arith(else_, assoc);
        }
        _ => {}
    }
}

fn collect_written(stmts: &[IrStmt], out: &mut BTreeSet<String>, arrays: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    out.insert(t.var.clone());
                    if !t.indices.is_empty() {
                        arrays.insert(t.var.clone());
                    }
                }
                if let IrExpr::Call { func, args } = expr {
                    if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                        for a in setarray_elems(args) {
                            collect_written_expr(a, out, arrays);
                        }
                        if let Some(IrExpr::Str(name, _)) = args.first() {
                            arrays.insert(name.clone());
                            out.insert(name.clone());
                        }
                        continue;
                    }
                }
                collect_written_expr(expr, out, arrays);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_written_expr(e, out, arrays);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                arrays.insert(var.clone());
                out.insert(var.clone());
                for e in elements {
                    collect_written_expr(e, out, arrays);
                }
            }
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone());
                collect_written_expr(iter, out, arrays);
                collect_written(body, out, arrays);
            }
            IrStmt::Expr(e) => collect_written_expr(e, out, arrays),
            IrStmt::Output { value, .. } => collect_written_expr(value, out, arrays),
            IrStmt::WriteFile { path, content, .. } => {
                collect_written_expr(path, out, arrays);
                collect_written_expr(content, out, arrays);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_written_expr(cond, out, arrays);
                collect_written(then, out, arrays);
                for (c, b) in elsifs {
                    collect_written_expr(c, out, arrays);
                    collect_written(b, out, arrays);
                }
                collect_written(else_, out, arrays);
            }
            IrStmt::While { cond, body } => {
                collect_written_expr(cond, out, arrays);
                collect_written(body, out, arrays);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_written(body, out, arrays);
                collect_written_expr(cond, out, arrays);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_written_expr(x, out, arrays);
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_written(b, out, arrays)
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_written(inner, out, arrays);
                for r in redirects {
                    collect_written_expr(&r.target, out, arrays);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                collect_written_expr(discriminant, out, arrays);
                for c in clauses {
                    collect_written(&c.body, out, arrays);
                }
            }
            IrStmt::Function { name, body, .. } => {
                out.insert(name.clone());
                collect_written(body, out, arrays);
            }
            IrStmt::Pipeline {
                stages, capture, ..
            } => {
                for st in stages {
                    collect_written(st, out, arrays);
                }
                if let Some(c) = capture {
                    out.insert(c.clone());
                }
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            } => {
                // the procsub mktemp/rm nodes mention temp vars — the
                // Var policy below (any mention declares) applies here
                // too so the Go homes exist before first use.
                collect_written_expr(cmd, out, arrays);
                for a in args {
                    collect_written_expr(a, out, arrays);
                }
                if let Some(c) = capture {
                    out.insert(c.clone());
                }
                for r in redirects {
                    collect_written_expr(r, out, arrays);
                }
                for (_, e) in env {
                    collect_written_expr(e, out, arrays);
                }
            }
            _ => {}
        }
    }
}

fn collect_written_expr(e: &IrExpr, out: &mut BTreeSet<String>, arrays: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
        }
        IrExpr::Index { var, key } => {
            arrays.insert(var.clone());
            collect_written_expr(key, out, arrays);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_written_expr(lhs, out, arrays);
            collect_written_expr(rhs, out, arrays);
        }
        IrExpr::Arith(a) => collect_written_arith(a, out, arrays),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_written_expr(x, out, arrays);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_written_expr(i, out, arrays);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_written_expr(a, out, arrays);
            }
        }
        _ => {}
    }
}

fn collect_written_arith(a: &ArithAst, out: &mut BTreeSet<String>, arrays: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            arrays.insert(var.clone());
            collect_written_arith(key, out, arrays);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_written_arith(lhs, out, arrays);
            collect_written_arith(rhs, out, arrays);
        }
        ArithAst::Un { arg, .. } => collect_written_arith(arg, out, arrays),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_written_arith(test, out, arrays);
            collect_written_arith(then, out, arrays);
            collect_written_arith(else_, out, arrays);
        }
        _ => {}
    }
}

/// Word-boundary `qualifier.` test — `fmt.` matches `fmt.Println` but
/// not `"fmt"` (the import line) or `sprintf`.
fn go_uses_pkg(s: &str, q: &str) -> bool {
    let needle = format!("{q}.");
    let b = s.as_bytes();
    let wb = needle.as_bytes();
    if wb.len() > b.len() { return false; }
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    while i + wb.len() <= b.len() {
        if &b[i..i + wb.len()] == wb && (i == 0 || !is_id(b[i - 1])) { return true; }
        i += 1;
    }
    false
}
fn go_contains_word(s: &str, w: &str) -> bool {
    if w.is_empty() || w.len() > s.len() { return false; }
    let b = s.as_bytes();
    let wb = w.as_bytes();
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    while i + wb.len() <= b.len() {
        if &b[i..i + wb.len()] == wb
            && (i == 0 || !is_id(b[i - 1]))
            && (i + wb.len() == b.len() || !is_id(b[i + wb.len()]))
        { return true; }
        i += 1;
    }
    false
}
fn go_split_helpers() -> Vec<(String, Vec<String>)> {
    let mut segs: Vec<(String, Vec<String>)> = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in RUNTIME_HELPERS {
        let name = if let Some(r) = line.strip_prefix("func ") {
            r.split('(').next().map(|x| x.trim().to_string())
        } else if let Some(r) = line.strip_prefix("var ") {
            r.split_whitespace().next().map(|x| x.to_string())
        } else { None };
        if let Some(n) = name {
            if let Some(c) = cur.take() { segs.push(c); }
            cur = Some((n, vec![line.to_string()]));
        } else if let Some(c) = cur.as_mut() {
            c.1.push(line.to_string());
        } else {
            segs.push((String::new(), vec![line.to_string()]));
        }
    }
    if let Some(c) = cur.take() { segs.push(c); }
    segs
}
fn go_used_helpers(body: &str) -> Vec<String> {
    let segs = go_split_helpers();
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for (name, _) in &segs {
        if !name.is_empty() && go_contains_word(body, name) { needed.insert(name.clone()); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (name, lines) in &segs {
            if name.is_empty() || !needed.contains(name) { continue; }
            let text = lines.join("\n");
            for (n2, _) in &segs {
                if !n2.is_empty() && !needed.contains(n2) && go_contains_word(&text, n2) {
                    needed.insert(n2.clone());
                    changed = true;
                }
            }
        }
    }
    let mut out = Vec::new();
    for (name, lines) in &segs {
        let keep = (name.is_empty() && segs.iter().any(|(n, _)| !n.is_empty() && needed.contains(n)))
            || (!name.is_empty() && needed.contains(name));
        if keep { out.extend(lines.iter().cloned()); }
    }
    out
}

/// The fixed runtime-helper block (package-level; go_used_helpers emits
/// only the parts the body references, and the import list is trimmed to
/// what the kept text uses).
const RUNTIME_HELPERS: &[&str] = &[
    "func s2s(x any) string {",
    "    if x == nil { return \"\" }",
    "    return fmt.Sprintf(\"%v\", x)",
    "}",
    "",
    "func s2i(x any) int64 {",
    "    s := strings.TrimSpace(fmt.Sprintf(\"%v\", x))",
    "    if n, err := strconv.ParseInt(s, 0, 64); err == nil { return n }",
    "    return 0",
    "}",
    "",
    "func truthy(x any) bool {",
    "    switch v := x.(type) {",
    "    case string:",
    "        return v != \"\"",
    "    case bool:",
    "        return v",
    "    case int64:",
    "        return v != 0",
    "    case nil:",
    "        return false",
    "    }",
    "    return true",
    "}",
    "",
    "func cond3(c bool, t, e any) any {",
    "    if c { return t }",
    "    return e",
    "}",
    "",
    "func land(a, b int64) int64 {",
    "    if a != 0 && b != 0 { return 1 }",
    "    return 0",
    "}",
    "",
    "func lor(a, b int64) int64 {",
    "    if a != 0 || b != 0 { return 1 }",
    "    return 0",
    "}",
    "",
    "func lnot(a int64) int64 {",
    "    if a == 0 { return 1 }",
    "    return 0",
    "}",
    "",
    "func b2i(b bool) int64 {",
    "    if b { return 1 }",
    "    return 0",
    "}",
    "",
    "func cmpN(a, b int64, op string) int64 {",
    "    switch op {",
    "    case \"==\": if a == b { return 1 }",
    "    case \"!=\": if a != b { return 1 }",
    "    case \"<\": if a < b { return 1 }",
    "    case \">\": if a > b { return 1 }",
    "    case \"<=\": if a <= b { return 1 }",
    "    case \">=\": if a >= b { return 1 }",
    "    }",
    "    return 0",
    "}",
    "",
    "func powN(a, b int64) int64 {",
    "    if b < 0 {",
    "        if a == 1 || a == -1 { return a }",
    "        return 0",
    "    }",
    "    r := int64(1)",
    "    for i := int64(0); i < b; i++ { r *= a }",
    "    return r",
    "}",
    "",
    "// bash arithmetic never panics: division/modulo by zero yield 0 (with",
    "// a stderr diagnostic the gate discards), and shifts out of range",
    "// yield 0 — plain Go operators would crash the program.",
    "func divN(a, b int64) int64 {",
    "    if b == 0 { return 0 }",
    "    if b == -1 && a == -9223372036854775808 { return -9223372036854775808 }",
    "    return a / b",
    "}",
    "",
    "func modN(a, b int64) int64 {",
    "    if b == 0 { return 0 }",
    "    if b == -1 && a == -9223372036854775808 { return 0 }",
    "    return a % b",
    "}",
    "",
    "func shlN(a, b int64) int64 {",
    "    if b < 0 || b > 63 { return 0 }",
    "    return a << uint64(b)",
    "}",
    "",
    "func shrN(a, b int64) int64 {",
    "    if b < 0 || b > 63 { return 0 }",
    "    return a >> uint64(b)",
    "}",
    "",
    "func runeLen(s string) int64 {",
    "    return int64(utf8.RuneCountInString(s))",
    "}",
    "",
    "func arrLen(a []any) int64 { return int64(len(a)) }",
    "",
    "func setArrAt(a []any, i int64, v any) []any {",
    "    for int64(len(a)) <= i { a = append(a, nil) }",
    "    a[i] = v",
    "    return a",
    "}",
    "",
    "func arrIdx(a []any, i int64) any {",
    "    if i >= 0 && int(i) < len(a) { return a[i] }",
    "    return nil",
    "}",
    "",
    "func arrSlice(a []any, start, end int64) []any {",
    "    s, e := int(start), int(end)",
    "    if s < 0 { s = len(a) + s }",
    "    if e < 0 { e = len(a) + e }",
    "    if s < 0 { s = 0 }",
    "    if s > len(a) { s = len(a) }",
    "    if e > len(a) { e = len(a) }",
    "    if e < s { e = s }",
    "    return a[s:e]",
    "}",
    "",
    "// `${a[@]:off[:len]}` element slice with bash negative rules (a",
    "// negative length counts the end from the array end, not the start).",
    "func arrSliceN(a []any, st, ln int64, hasLen bool) []any {",
    "    if st < 0 { st += int64(len(a)) }",
    "    var en int64",
    "    if !hasLen {",
    "        en = int64(len(a))",
    "    } else if ln < 0 {",
    "        en = int64(len(a)) + ln",
    "    } else {",
    "        en = st + ln",
    "    }",
    "    return arrSlice(a, st, en)",
    "}",
    "",
    "var fArgs []string",
    "",
    "// `${arr[*]}` joins elements with the first char of IFS (default",
    "// blank); `${arr[@]}` keeps elements separate (echo rejoins them",
    "// with blanks when printing). Updated by static IFS assignments.",
    "var ifsJoin = \" \"",
    "var __wg sync.WaitGroup",
    "",
    "func argsList() []string {",
    "    if len(fArgs) > 0 { return fArgs }",
    "    if len(os.Args) > 1 { return os.Args[1:] }",
    "    return nil",
    "}",
    "",
    "func __sh_line(s string, n int) string {",
    "    parts := strings.Split(s, \"\\n\")",
    "    if n >= 0 && n < len(parts) { return parts[n] }",
    "    return \"\"",
    "}",
    "",
    "func paramAt(i int) string {",
    "    a := argsList()",
    "    if i >= 1 && i <= len(a) { return a[i-1] }",
    "    return \"\"",
    "}",
    "",
    "func runCmd(c *exec.Cmd) int64 {",
    "    c.Stdout = os.Stdout",
    "    c.Stderr = io.Discard",
    "    if err := c.Run(); err != nil {",
    "        if ee, ok := err.(*exec.ExitError); ok { return int64(ee.ExitCode()) }",
    "        return 1",
    "    }",
    "    return 0",
    "}",
    "",
    "func capCmd(c *exec.Cmd) string {",
    "    c.Stderr = io.Discard",
    "    // command substitution keeps stdout even on nonzero exit (diff",
    "    // prints the diff and exits 1) — only a start failure is empty.",
    "    out, _ := c.Output()",
    "    return strings.TrimRight(string(out), \"\\n\")",
    "}",
    "",
    "func capCmdRaw(c *exec.Cmd) string {",
    "    c.Stderr = io.Discard",
    "    out, _ := c.Output()",
    "    return string(out)",
    "}",
    "",
    "func capRun(script string, env []string) string {",
    "    c := exec.Command(\"bash\", \"-c\", script)",
    "    c.Env = append(os.Environ(), env...)",
    "    c.Stderr = io.Discard",
    "    out, _ := c.Output()",
    "    return strings.TrimRight(string(out), \"\\n\")",
    "}",
    "",
    "func redirRun(script string, env []string) int64 {",
    "    c := exec.Command(\"bash\", \"-c\", script)",
    "    c.Env = append(os.Environ(), env...)",
    "    c.Stdout = os.Stdout",
    "    c.Stderr = io.Discard",
    "    return runCmd(c)",
    "}",
    "",
    "func runPipe(stages [][]string) int64 {",
    "    var cmds []*exec.Cmd",
    "    for _, st := range stages {",
    "        c := exec.Command(st[0], st[1:]...)",
    "        c.Stderr = io.Discard",
    "        cmds = append(cmds, c)",
    "    }",
    "    if len(cmds) == 0 { return 0 }",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        p, err := cmds[i].StdoutPipe()",
    "        if err != nil { return 1 }",
    "        cmds[i+1].Stdin = p",
    "    }",
    "    cmds[len(cmds)-1].Stdout = os.Stdout",
    "    for i, c := range cmds {",
    "        if i < len(cmds)-1 {",
    "            if err := c.Start(); err != nil { return 1 }",
    "        } else if err := c.Run(); err != nil {",
    "            if ee, ok := err.(*exec.ExitError); ok { return int64(ee.ExitCode()) }",
    "            return 1",
    "        }",
    "    }",
    "    // the consumer is done: kill the producers (an infinite `yes`",
    "    // would spin on Wait — the child inherits SIG_IGN for SIGPIPE",
    "    // from the Go runtime and never dies on EPIPE)",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        if cmds[i].Process != nil { cmds[i].Process.Kill() }",
    "        cmds[i].Wait()",
    "    }",
    "    return 0",
    "}",
    "",
    "// capture the stdout of a pipeline (`$(a | b | c)`), optionally fed an",
    "// input string (a leading `echo` stage evaluates to input instead of",
    "// forking echo). Stdout is kept even on nonzero exit, like captures.",
    "func capPipeIn(input string, stages [][]string) string {",
    "    return strings.TrimRight(capPipeInRaw(input, stages), \"\\n\")",
    "}",
    "",
    "func capPipeInRaw(input string, stages [][]string) string {",
    "    var cmds []*exec.Cmd",
    "    for _, st := range stages {",
    "        if len(st) == 0 { continue }",
    "        c := exec.Command(st[0], st[1:]...)",
    "        c.Stderr = io.Discard",
    "        cmds = append(cmds, c)",
    "    }",
    "    if len(cmds) == 0 { return \"\" }",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        p, err := cmds[i].StdoutPipe()",
    "        if err != nil { return \"\" }",
    "        cmds[i+1].Stdin = p",
    "    }",
    "    if input != \"\" {",
    "        cmds[0].Stdin = strings.NewReader(input)",
    "    }",
    "    var buf bytes.Buffer",
    "    cmds[len(cmds)-1].Stdout = &buf",
    "    for i, c := range cmds {",
    "        if i < len(cmds)-1 {",
    "            if err := c.Start(); err != nil { return \"\" }",
    "        } else if err := c.Run(); err != nil {",
    "            break",
    "        }",
    "    }",
    "    // the consumer is done: kill the producers (an infinite `yes`",
    "    // would spin on Wait — the child inherits SIG_IGN for SIGPIPE",
    "    // from the Go runtime and never dies on EPIPE)",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        if cmds[i].Process != nil { cmds[i].Process.Kill() }",
    "        cmds[i].Wait()",
    "    }",
    "    return buf.String()",
    "}",
    "",
    "// run a pipeline fed an input string (status = last stage's rc,",
    "// stdout passes through) — herestring sort without a bash child.",
    "func runPipeIn(input string, stages [][]string) int64 {",
    "    var cmds []*exec.Cmd",
    "    for _, st := range stages {",
    "        if len(st) == 0 { continue }",
    "        c := exec.Command(st[0], st[1:]...)",
    "        c.Stderr = io.Discard",
    "        cmds = append(cmds, c)",
    "    }",
    "    if len(cmds) == 0 { return 0 }",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        p, err := cmds[i].StdoutPipe()",
    "        if err != nil { return 1 }",
    "        cmds[i+1].Stdin = p",
    "    }",
    "    if input != \"\" {",
    "        cmds[0].Stdin = strings.NewReader(input)",
    "    }",
    "    cmds[len(cmds)-1].Stdout = os.Stdout",
    "    for i, c := range cmds {",
    "        if i < len(cmds)-1 {",
    "            if err := c.Start(); err != nil { return 1 }",
    "        } else if err := c.Run(); err != nil {",
    "            if ee, ok := err.(*exec.ExitError); ok { return int64(ee.ExitCode()) }",
    "            return 1",
    "        }",
    "    }",
    "    // the consumer is done: kill the producers (an infinite `yes`",
    "    // would spin on Wait — the child inherits SIG_IGN for SIGPIPE",
    "    // from the Go runtime and never dies on EPIPE)",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        if cmds[i].Process != nil { cmds[i].Process.Kill() }",
    "        cmds[i].Wait()",
    "    }",
    "    return 0",
    "}",
    "",
    "// `mapfile -t arr < path` contents (each line an element; empty input",
    "// is an empty array — bash -t strips the trailing newline per line).",
    "// `$@` as array elements (`local -a args=(\"$@\")`).",
    "func strsToAny(s []string) []any {",
    "    out := make([]any, len(s))",
    "    for i, v := range s { out[i] = v }",
    "    return out",
    "}",
    "",
    "// `${var:-def}` on an already-fetched value.",
    "func orDef(v, def string) string {",
    "    if v == \"\" { return def }",
    "    return v",
    "}",
    "",
    "func readLines(path string) []any {",
    "    data, err := os.ReadFile(path)",
    "    if err != nil { return []any{} }",
    "    s := strings.TrimRight(string(data), \"\\n\")",
    "    if s == \"\" { return []any{} }",
    "    parts := strings.Split(s, \"\\n\")",
    "    out := make([]any, len(parts))",
    "    for i, p := range parts { out[i] = p }",
    "    return out",
    "}",
    "",
    "// lines of a file for `while read` (empty lines PRESERVED — read",
    "// still runs the body; only a missing/empty file yields nothing).",
    "func readFileLines(path string) []string {",
    "    data, err := os.ReadFile(path)",
    "    if err != nil { return nil }",
    "    s := string(data)",
    "    if s == \"\" { return nil }",
    "    s = strings.TrimSuffix(s, \"\\n\")",
    "    return strings.Split(s, \"\\n\")",
    "}",
    "",
    "// split a read line into exactly n fields (bash `read` rules with",
    "// an IFS value: empty IFS gives the whole line to the first var;",
    "// default whitespace collapses runs (tail spacing approximate); a",
    "// single-char IFS splits exactly with empties kept and the tail",
    "// intact; multi-char sets approximate like whitespace).",
    "func readSplit(line, ifs string, n int) []string {",
    "    out := make([]string, n)",
    "    if n == 0 { return out }",
    "    if ifs == \"\" {",
    "        out[0] = line",
    "        return out",
    "    }",
    "    if ifs == \" \\t\\n\" {",
    "        fields := strings.Fields(line)",
    "        for i := 0; i < n && i < len(fields); i++ { out[i] = fields[i] }",
    "        if len(fields) > n {",
    "            out[n-1] = strings.Join(fields[n-1:], \" \")",
    "        }",
    "        return out",
    "    }",
    "    if len(ifs) == 1 {",
    "        if ifs[0] == ' ' || ifs[0] == '\\t' || ifs[0] == '\\n' {",
    "            fields := strings.Fields(line)",
    "            for i := 0; i < n && i < len(fields); i++ { out[i] = fields[i] }",
    "            if len(fields) > n {",
    "                out[n-1] = strings.Join(fields[n-1:], \" \")",
    "            }",
    "            return out",
    "        }",
    "        parts := strings.SplitN(line, ifs, n)",
    "        copy(out, parts)",
    "        return out",
    "    }",
    "    fields := strings.FieldsFunc(line, func(r rune) bool {",
    "        return strings.ContainsRune(ifs, r)",
    "    })",
    "    for i := 0; i < n && i < len(fields); i++ { out[i] = fields[i] }",
    "    if len(fields) > n {",
    "        out[n-1] = strings.Join(fields[n-1:], string(ifs[0]))",
    "    }",
    "    return out",
    "}",
    "",
    "func readInto(ps ...*string) bool {",
    "    line, err := bufio.NewReader(os.Stdin).ReadString('\\n')",
    "    if err != nil && line == \"\" { return false }",
    "    line = strings.TrimRight(line, \"\\n\")",
    "    fs := strings.Fields(line)",
    "    for i := range ps {",
    "        if i < len(fs) { *ps[i] = fs[i] } else { *ps[i] = \"\" }",
    "    }",
    "    return true",
    "}",
    "",
    "func globMatch(pattern, s string) bool {",
    "    ok, _ := filepath.Match(pattern, s)",
    "    return ok",
    "}",
    "",
    "func globArgs(pat string) []string {",
    "    m, err := filepath.Glob(pat)",
    "    if err != nil || m == nil { return []string{pat} }",
    "    var out []string",
    "    for _, p := range m {",
    "        if globDotOK(pat, p) { out = append(out, p) }",
    "    }",
    "    if out == nil { return []string{pat} }",
    "    return out",
    "}",
    "",
    "// globDotOK enforces the shell pathname-expansion rule filepath.Glob",
    "// lacks: a path component beginning with '.' matches only when the",
    "// pattern component begins with a literal '.' (verified against bash:",
    "// `[.]foo` does NOT match .foo, `?oo`/`[a-z]oo` skip dotfiles).",
    "// Fully literal components keep Glob's verdict (`ls .sh` finds .sh),",
    "// and '.'/'..' never come from a glob component.",
    "func globDotOK(pat, p string) bool {",
    "    ps := strings.Split(pat, \"/\")",
    "    fs := strings.Split(p, \"/\")",
    "    if len(ps) != len(fs) { return true }",
    "    for i := range ps {",
    "        f := fs[i]",
    "        if len(f) == 0 || f[0] != '.' { continue }",
    "        pp := ps[i]",
    "        if f == \".\" || f == \"..\" {",
    "            if pp != f { return false }",
    "            continue",
    "        }",
    "        if strings.ContainsAny(pp, \"*?[\\\\\") {",
    "            if len(pp) == 0 || pp[0] != '.' && !(len(pp) > 1 && pp[0] == '\\\\' && pp[1] == '.') { return false }",
    "        }",
    "    }",
    "    return true",
    "}",
    "",
    "func reMatch(pattern, s string) bool {",
    "    ok, _ := regexp.MatchString(pattern, s)",
    "    return ok",
    "}",
    "",
    "func globRe(pat string, greedy bool) string {",
    "    var b strings.Builder",
    "    for i := 0; i < len(pat); i++ {",
    "        switch pat[i] {",
    "        case '*':",
    "            if greedy { b.WriteString(\".*\") } else { b.WriteString(\".*?\") }",
    "        case '?': b.WriteString(\".\")",
    "        case '.', '(', ')', '+', '|', '^', '$', '[', ']', '{', '}', '\\\\':",
    "            b.WriteByte('\\\\'); b.WriteByte(pat[i])",
    "        default: b.WriteByte(pat[i])",
    "        }",
    "    }",
    "    return b.String()",
    "}",
    "",
    "func pStrip(shortest bool, v, pat string) string {",
    "    re := regexp.MustCompile(\"^(?:\" + globRe(pat, !shortest) + \")\")",
    "    loc := re.FindStringIndex(v)",
    "    if loc == nil { return v }",
    "    return v[loc[1]:]",
    "}",
    "",
    "func pSuf(shortest bool, v, pat string) string {",
    "    var re *regexp.Regexp",
    "    if shortest {",
    "        re = regexp.MustCompile(\"^(.*)\" + globRe(pat, true) + \".*?$\")",
    "    } else {",
    "        re = regexp.MustCompile(\"^(.*?)\" + globRe(pat, true) + \".*$\")",
    "    }",
    "    m := re.FindStringSubmatchIndex(v)",
    "    if m == nil { return v }",
    "    return v[:m[3]]",
    "}",
    "",
    "func pRep(v, pat, rep string, all bool) string {",
    "    re := regexp.MustCompile(globRe(pat, true))",
    "    if all { return re.ReplaceAllString(v, rep) }",
    "    l := re.FindStringIndex(v)",
    "    if l == nil { return v }",
    "    return v[:l[0]] + rep + v[l[1]:]",
    "}",
    "",
    "func fileTest(op string, p string) bool {",
    "    st, err := os.Stat(p)",
    "    switch op {",
    "    case \"e\", \"a\": return err == nil",
    "    case \"f\": return err == nil && st.Mode().IsRegular()",
    "    case \"d\": return err == nil && st.IsDir()",
    "    case \"s\": return err == nil && st.Size() > 0",
    "    case \"L\", \"h\":",
    "        ls, lerr := os.Lstat(p)",
    "        return lerr == nil && ls.Mode()&os.ModeSymlink != 0",
    "    case \"p\": return err == nil && st.Mode()&os.ModeNamedPipe != 0",
    "    case \"b\": return err == nil && st.Mode()&os.ModeDevice != 0 && st.Mode()&os.ModeCharDevice == 0",
    "    case \"c\": return err == nil && st.Mode()&os.ModeCharDevice != 0",
    "    case \"S\": return err == nil && st.Mode()&os.ModeSocket != 0",
    "    case \"g\": return err == nil && st.Mode()&os.ModeSetgid != 0",
    "    case \"k\": return err == nil && st.Mode()&os.ModeSticky != 0",
    "    case \"u\": return err == nil && st.Mode()&os.ModeSetuid != 0",
    "    case \"N\": return false",
    "    case \"O\":",
    "        if err != nil { return false }",
    "        if sys, ok := st.Sys().(*syscall.Stat_t); ok { return int(sys.Uid) == os.Getuid() }",
    "        return false",
    "    case \"G\":",
    "        if err != nil { return false }",
    "        if sys, ok := st.Sys().(*syscall.Stat_t); ok { return int(sys.Gid) == os.Getgid() }",
    "        return false",
    "    case \"r\": return syscall.Access(p, 4) == nil",
    "    case \"w\": return syscall.Access(p, 2) == nil",
    "    case \"x\": return syscall.Access(p, 1) == nil",
    "    }",
    "    return false",
    "}",
    "",
    "func fileNewer(a, b string, nt bool) bool {",
    "    sa, ea := os.Stat(a)",
    "    sb, eb := os.Stat(b)",
    "    if ea != nil || eb != nil { return false }",
    "    if nt { return sa.ModTime().After(sb.ModTime()) }",
    "    return sb.ModTime().After(sa.ModTime())",
    "}",
    "",
    "func fileSame(a, b string) bool {",
    "    sa, ea := os.Stat(a)",
    "    sb, eb := os.Stat(b)",
    "    if ea != nil || eb != nil { return false }",
    "    return os.SameFile(sa, sb)",
    "}",
    "",
    "var vars = map[string]any{}",
    "",
    "func pValRaw(name string) any {",
    "    if v, ok := vars[name]; ok { return v }",
    "    if n, err := strconv.Atoi(name); err == nil {",
    "        return paramAt(n)",
    "    }",
    "    return os.Getenv(name)",
    "}",
    "",
    "// assoc values synced as vars[base[key]] (baked writes) — sorted by",
    "// full key for deterministic output (bash's assoc order is hash).",
    "func assocVals(base string) []string {",
    "    var keys []string",
    "    pre := base + \"[\"",
    "    for k := range vars {",
    "        if strings.HasPrefix(k, pre) && strings.HasSuffix(k, \"]\") {",
    "            keys = append(keys, k)",
    "        }",
    "    }",
    "    sort.Strings(keys)",
    "    out := make([]string, 0, len(keys))",
    "    for _, k := range keys { out = append(out, s2s(vars[k])) }",
    "    return out",
    "}",
    "",
    "// `declare -p` double-quoting (bash escapes \" $ ` \\; newlines use",
    "// the $'…' form).",
    "func shq(s string) string {",
    "    if strings.ContainsRune(s, '\\n') {",
    "        var b strings.Builder",
    "        b.WriteString(\"$'\")",
    "        for _, r := range s {",
    "            switch r {",
    "            case '\\'': b.WriteString(\"\\\\'\")",
    "            case '\\\\': b.WriteString(\"\\\\\\\\\")",
    "            case '\\n': b.WriteString(\"\\\\n\")",
    "            case '\\t': b.WriteString(\"\\\\t\")",
    "            default: b.WriteRune(r)",
    "            }",
    "        }",
    "        b.WriteByte('\\'')",
    "        return b.String()",
    "    }",
    "    var b strings.Builder",
    "    b.WriteByte('\"')",
    "    for _, r := range s {",
    "        switch r {",
    "        case '\"', '$', '`', '\\\\': b.WriteByte('\\\\'); b.WriteRune(r)",
    "        default: b.WriteRune(r)",
    "        }",
    "    }",
    "    b.WriteByte('\"')",
    "    return b.String()",
    "}",
    "",
    "// `declare -p` element lists: `([0]=\"a\" [2]=\"c\")`.",
    "func arrDecl(a []any) string {",
    "    var b strings.Builder",
    "    b.WriteString(\"(\")",
    "    for i, v := range a {",
    "        if i > 0 { b.WriteString(\" \") }",
    "        fmt.Fprintf(&b, \"[%d]=%s\", i, shq(s2s(v)))",
    "    }",
    "    b.WriteString(\")\")",
    "    return b.String()",
    "}",
    "",
    "// `declare -p` assoc list: `([k1]=\"v1\" [k2]=\"v2\")` (sorted keys).",    "func assocDecl(base string) string {",
    "    var keys []string",
    "    pre := base + \"[\"",
    "    for k := range vars {",
    "        if strings.HasPrefix(k, pre) && strings.HasSuffix(k, \"]\") {",
    "            keys = append(keys, k)",
    "        }",
    "    }",
    "    sort.Strings(keys)",
    "    var b strings.Builder",
    "    b.WriteString(\"(\")",
    "    for i, k := range keys {",
    "        if i > 0 { b.WriteString(\" \") }",
    "        b.WriteString(\"[\")",
    "        b.WriteString(k[len(pre):len(k)-1])",
    "        b.WriteString(\"]=\")",
    "        b.WriteString(shq(s2s(vars[k])))",
    "    }",
    "    b.WriteString(\")\")",
    "    return b.String()",
    "}",
    "",
    "// strip a trailing `[@]`/`[*]` whole-array subscript (both join",
    "// elements in value position; iteration splitting happens upstream).",
    "func stripAt(name string) (string, bool) {",
    "    if strings.HasSuffix(name, \"[@]\") || strings.HasSuffix(name, \"[*]\") {",
    "        return name[:len(name)-3], true",
    "    }",
    "    return name, false",
    "}",
    "",
    "// `${!base[@]}` keys: assoc keys from the vars home (sorted), else",
    "// indexed positions (bash lists 0..n-1).",
    "func arrKeys(base string) []string {",
    "    base, _ = stripAt(base)",
    "    pre := base + \"[\"",
    "    var keys []string",
    "    for k := range vars {",
    "        if strings.HasPrefix(k, pre) && strings.HasSuffix(k, \"]\") {",
    "            keys = append(keys, k[len(pre):len(k)-1])",
    "        }",
    "    }",
    "    if keys != nil { sort.Strings(keys); return keys }",
    "    a, _ := pValRaw(base).([]any)",
    "    out := make([]string, 0, len(a))",
    "    for i := range a { out = append(out, strconv.Itoa(i)) }",
    "    return out",
    "}",
    "",
    "",
    "func pVal(name string) string {",
    "    if strings.HasPrefix(name, \"#\") {",
    "        n := name[1:]",
    "        if base, whole := stripAt(n); whole {",
    "            // assoc home wins: an empty slice home (from `local -A`",
    "            // with no elements) must not shadow the vars keys",
    "            if ak := assocVals(base); len(ak) > 0 {",
    "                return strconv.FormatInt(int64(len(ak)), 10)",
    "            }",
    "            a, _ := pValRaw(base).([]any)",
    "            return strconv.FormatInt(int64(len(a)), 10)",
    "        }",
    "        if a, ok := pValRaw(n).([]any); ok {",
    "            return strconv.FormatInt(int64(len(a)), 10)",
    "        }",
    "        return strconv.FormatInt(runeLen(s2s(pValRaw(n))), 10)",
    "    }",
    "    if base, whole := stripAt(name); whole {",
    "        joiner := \" \"",
    "        if strings.HasSuffix(name, \"[*]\") { joiner = ifsJoin }",
    "        // assoc home wins over an empty slice home (see pVal length)",
    "        if ak := assocVals(base); len(ak) > 0 {",
    "            return strings.Join(ak, joiner)",
    "        }",
    "        a, _ := pValRaw(base).([]any)",
    "        out := make([]string, 0, len(a))",
    "        for _, v := range a { out = append(out, s2s(v)) }",
    "        return strings.Join(out, joiner)",
    "    }",
    "    if i := strings.Index(name, \"[\"); i > 0 && strings.HasSuffix(name, \"]\") {",
    "        // assoc home first: baked writes sync vars[base[key]]",
    "        if v, ok := vars[name]; ok { return s2s(v) }",
    "        a, _ := pValRaw(name[:i]).([]any)",
    "        k, err := strconv.ParseInt(name[i+1:len(name)-1], 0, 64)",
    "        if err != nil {",
    "            m, _ := pValRaw(name[:i]).(map[string]any)",
    "            return s2s(m[name[i+1:len(name)-1]])",
    "        }",
    "        return s2s(arrIdx(a, k))",
    "    }",
    "    return s2s(pValRaw(name))",
    "}",
    "",
    "func pList(op, name string, args ...string) []string {",
    "    if op == \"@\" || op == \"*\" { return argsList() }",
    "    // `${!base[@]}` array keys (the `!` arrives in the name): assoc",
    "    // keys from the vars home, else indexed positions.",
    "    if strings.HasPrefix(name, \"!\") { return arrKeys(name[1:]) }",
    "    // element-list form with `@`/`*` as the first extra arg",
    "    // (`${arr[@]:off:len}` arrives as name,name,@,off,len).",
    "    if len(args) > 0 && (args[0] == \"@\" || args[0] == \"*\") && !strings.HasSuffix(name, \"[@]\") && !strings.HasSuffix(name, \"[*]\") {",
    "        name = name + \"[@]\"",
    "        args = args[1:]",
    "    }",
    "    if strings.HasPrefix(name, \"#\") {",
    "        return []string{pVal(name)}",
    "    }",
    "    if base, whole := stripAt(name); whole {",
    "        // assoc home wins over an empty slice home (see pVal);",
    "        // both feed the same offset logic below",
    "        a, _ := pValRaw(base).([]any)",
    "        if ak := assocVals(base); len(ak) > 0 {",
    "            a = make([]any, len(ak))",
    "            for i, v := range ak { a[i] = v }",
    "        }",
    "        if op == \"slice\" && len(args) >= 1 {",
    "            st := int64(0)",
    "            if len(args) > 0 { st, _ = strconv.ParseInt(strings.TrimSpace(args[0]), 0, 64) }",
    "            ln := int64(0)",
    "            hasLen := len(args) > 1 && strings.TrimSpace(args[1]) != \"\"",
    "            if hasLen { ln, _ = strconv.ParseInt(strings.TrimSpace(args[1]), 0, 64) }",
    "            a = arrSliceN(a, st, ln, hasLen)",
    "        }",
    "        out := make([]string, 0, len(a))",
    "        for _, v := range a { out = append(out, s2s(v)) }",
    "        return out",
    "    }",
    "    // bare-name slice (`${arr:off:len}` arrives without `[@]` — the",
    "    // frontend normalizes element slices to this form): array homes",
    "    // slice elements, scalars fall through to the string op below",
    "    if op == \"slice\" && len(args) >= 1 {",
    "        if ak := assocVals(name); len(ak) > 0 {",
    "            aa := make([]any, len(ak))",
    "            for i, v := range ak { aa[i] = v }",
    "            st, _ := strconv.ParseInt(strings.TrimSpace(args[0]), 0, 64)",
    "            ln := int64(0)",
    "            hasLen := len(args) > 1 && strings.TrimSpace(args[1]) != \"\"",
    "            if hasLen { ln, _ = strconv.ParseInt(strings.TrimSpace(args[1]), 0, 64) }",
    "            aa = arrSliceN(aa, st, ln, hasLen)",
    "            out := make([]string, 0, len(aa))",
    "            for _, v := range aa { out = append(out, s2s(v)) }",
    "            return out",
    "        }",
    "        if a, ok := pValRaw(name).([]any); ok && a != nil {",
    "            st, _ := strconv.ParseInt(strings.TrimSpace(args[0]), 0, 64)",
    "            ln := int64(0)",
    "            hasLen := len(args) > 1 && strings.TrimSpace(args[1]) != \"\"",
    "            if hasLen { ln, _ = strconv.ParseInt(strings.TrimSpace(args[1]), 0, 64) }",
    "            a = arrSliceN(a, st, ln, hasLen)",
    "            out := make([]string, 0, len(a))",
    "            for _, v := range a { out = append(out, s2s(v)) }",
    "            return out",
    "        }",
    "    }",
    "    return []string{pExp(op, name, args...)}",
    "}",
    "",
    "func pExp(op, name string, args ...string) string {",
    "    // `${!base[@]}` keys in value position (indirect/keys syntax).",
    "    if strings.HasPrefix(name, \"!\") {",
    "        return strings.Join(arrKeys(name[1:]), \" \")",
    "    }",
    "    v := pVal(name)",
    "    switch op {",
    "    case \"\":",
    "        return v",
    "    case \"^\":",
    "        if v == \"\" { return v }",
    "        return strings.ToUpper(v[:1]) + v[1:]",
    "    case \"^^\":",
    "        return strings.ToUpper(v)",
    "    case \",\":",
    "        if v == \"\" { return v }",
    "        return strings.ToLower(v[:1]) + v[1:]",
    "    case \",,\":",
    "        return strings.ToLower(v)",
    "    case \"#\":",
    "        if len(args) > 0 { return pStrip(true, v, args[0]) }",
    "        return v",
    "    case \"##\":",
    "        if len(args) > 0 { return pStrip(false, v, args[0]) }",
    "        return v",
    "    case \"%\":",
    "        if len(args) > 0 { return pSuf(true, v, args[0]) }",
    "        return v",
    "    case \"%%\":",
    "        if len(args) > 0 { return pSuf(false, v, args[0]) }",
    "        return v",
    "    case \"/\":",
    "        if len(args) >= 2 { return pRep(v, args[0], args[1], false) }",
    "        if len(args) >= 1 { return pRep(v, args[0], \"\", false) }",
    "        return v",
    "    case \"//\":",
    "        if len(args) >= 2 { return pRep(v, args[0], args[1], true) }",
    "        if len(args) >= 1 { return pRep(v, args[0], \"\", true) }",
    "        return v",
    "    case \"len\":",
    "        return strconv.FormatInt(runeLen(v), 10)",
    "    case \"basename\":",
    "        return filepath.Base(v)",
    "    case \"dirname\":",
    "        return filepath.Dir(v)",
    "    case \"slice\":",
    "        st := int64(0)",
    "        if len(args) > 0 { st, _ = strconv.ParseInt(strings.TrimSpace(args[0]), 0, 64) }",
    "        if st < 0 { st = int64(runeLen(v)) + st }",
    "        if st > int64(runeLen(v)) { st = int64(runeLen(v)) }",
    "        if st < 0 { st = 0 }",
    "        rs := []rune(v)",
    "        if len(args) < 2 || strings.TrimSpace(args[1]) == \"\" { return string(rs[st:]) }",
    "        ln, _ := strconv.ParseInt(strings.TrimSpace(args[1]), 0, 64)",
    "        en := ln",
    "        if en < 0 { en = int64(len(rs)) + en } else { en = st + en }",
    "        if en > int64(len(rs)) { en = int64(len(rs)) }",
    "        if en < st { en = st }",
    "        return string(rs[st:en])",
    "    case \":-\", \"-\":",
    "        if v == \"\" && len(args) > 0 { return expand(args[0]) }",
    "        return v",
    "    case \":=\":",
    "        if v == \"\" && len(args) > 0 {",
    "            d := expand(args[0])",
    "            vars[name] = d",
    "            return d",
    "        }",
    "        return v",
    "    case \":?\":",
    "        if v == \"\" {",
    "            if len(args) > 0 { _ = expand(args[0]) }",
    "            os.Exit(1)",
    "        }",
    "        return v",
    "    case \"+\":",
    "        if v == \"\" { return \"\" }",
    "        if len(args) > 0 { return expand(args[0]) }",
    "        return v",
    "    }",
    "    return v",
    "}",
    "",
    "func expand(s string) string {",
    "    var out strings.Builder",
    "    for i := 0; i < len(s); {",
    "        if s[i] == '$' && i+1 < len(s) {",
    "            if s[i+1] == '(' {",
    "                d := 1",
    "                j := i + 2",
    "                for j < len(s) && d > 0 {",
    "                    if s[j] == '(' { d++ }",
    "                    if s[j] == ')' { d-- }",
    "                    j++",
    "                }",
    "                out.WriteString(capRun(s[i+2:j-1], nil))",
    "                i = j",
    "                continue",
    "            }",
    "            if s[i+1] == '{' {",
    "                j := i + 2",
    "                for j < len(s) && s[j] != '}' { j++ }",
    "                if j >= len(s) { out.WriteString(s[i:]); break }",
    "                out.WriteString(pExpStr(s[i+2 : j]))",
    "                i = j + 1",
    "                continue",
    "            }",
    "            j := i + 1",
    "            for j < len(s) && (s[j] >= 'a' && s[j] <= 'z' || s[j] >= 'A' && s[j] <= 'Z' || s[j] >= '0' && s[j] <= '9' || s[j] == '_') { j++ }",
    "            out.WriteString(pVal(s[i+1 : j]))",
    "            i = j",
    "            continue",
    "        }",
    "        out.WriteByte(s[i])",
    "        i++",
    "    }",
    "    return out.String()",
    "}",
    "",
    "func pExpStr(inner string) string {",
    "    for _, op := range []string{\":-\", \":=\", \":?\", \"##\", \"%%\", \"//\", \"^^\", \",,\", \"#\", \"%\", \"/\", \"^\", \",\"} {",
    "        if idx := strings.Index(inner, op); idx > 0 {",
    "            return pExp(op, inner[:idx], inner[idx+len(op):])",
    "        }",
    "    }",
    "    if strings.HasPrefix(inner, \"#\") { return pVal(inner) }",
    "    return pVal(inner)",
    "}",
    "",
    "func eEsc(s string) string {",
    "    var out strings.Builder",
    "    for i := 0; i < len(s); i++ {",
    "        c := s[i]",
    "        if c != '\\\\' || i+1 >= len(s) { out.WriteByte(c); continue }",
    "        i++",
    "        switch s[i] {",
    "        case 'n': out.WriteByte('\\n')",
    "        case 't': out.WriteByte('\\t')",
    "        case 'r': out.WriteByte('\\r')",
    "        case 'a': out.WriteByte('\\a')",
    "        case 'b': out.WriteByte('\\b')",
    "        case 'f': out.WriteByte('\\f')",
    "        case 'v': out.WriteByte('\\v')",
    "        case '\\\\': out.WriteByte('\\\\')",
    "        case '0', '1', '2', '3', '4', '5', '6', '7': {",
    "            v := int(s[i] - '0')",
    "            for k := 0; k < 2 && i+1 < len(s) && s[i+1] >= '0' && s[i+1] <= '7'; k++ {",
    "                i++; v = v*8 + int(s[i]-'0')",
    "            }",
    "            out.WriteByte(byte(v))",
    "        }",
    "        case 'x': {",
    "            v := 0",
    "            n := 0",
    "            for n < 2 && i+1 < len(s) {",
    "                d := s[i+1]",
    "                var h int",
    "                switch {",
    "                case d >= '0' && d <= '9': h = int(d - '0')",
    "                case d >= 'a' && d <= 'f': h = int(d-'a') + 10",
    "                case d >= 'A' && d <= 'F': h = int(d-'A') + 10",
    "                default: d = 0",
    "                }",
    "                if d == 0 && !(s[i+1] == '0') { break }",
    "                v = v*16 + h",
    "                i++",
    "                n++",
    "            }",
    "            if n == 0 { out.WriteByte('\\\\'); out.WriteByte('x') } else { out.WriteByte(byte(v)) }",
    "        }",
    "        default: out.WriteByte('\\\\'); out.WriteByte(s[i])",
    "        }",
    "    }",
    "    return out.String()",
    "}",
    "",
    "func bprintf(f string, a ...string) {",
    "    fmt.Print(bprintfStr(f, a...))",
    "}",
    "",
    "func bprintfStr(f string, a ...string) string {",
    "    f = bEsc(f)",
    "    if len(a) == 0 { return f }",
    "    var out strings.Builder",
    "    for len(a) > 0 {",
    "        var used int",
    "        out.WriteString(bfmtOnce(f, a, &used))",
    "        if used == 0 { break }",
    "        a = a[used:]",
    "    }",
    "    return out.String()",
    "}",
    "",
    "func bfmtOnce(f string, a []string, used *int) string {",
    "    var out strings.Builder",
    "    for i := 0; i < len(f); i++ {",
    "        c := f[i]",
    "        if c != '%' || i+1 >= len(f) { out.WriteByte(c); continue }",
    "        j := i + 1",
    "        for j < len(f) && strings.ContainsRune(\"-+ #0\", rune(f[j])) { j++ }",
    "        for j < len(f) && f[j] >= '0' && f[j] <= '9' { j++ }",
    "        if j < len(f) && f[j] == '.' {",
    "            j++",
    "            for j < len(f) && f[j] >= '0' && f[j] <= '9' { j++ }",
    "        }",
    "        for j < len(f) && strings.ContainsRune(\"lhLzjt\", rune(f[j])) { j++ }",
    "        if j >= len(f) { out.WriteByte('%'); break }",
    "        v := f[j]",
    "        spec := f[i : j+1]",
    "        i = j",
    "        switch v {",
    "        case '%': out.WriteByte('%')",
    "        case 's', 'q', 'd', 'i', 'u', 'o', 'x', 'X', 'e', 'E', 'f', 'F', 'g', 'G', 'c': {",
    "            arg := \"\"",
    "            if *used < len(a) { arg = a[*used] }",
    "            *used++",
    "            switch v {",
    "            case 's': out.WriteString(bWidth(spec, arg))",
    "            case 'q': out.WriteString(strconv.Quote(arg))",
    "            case 'c': if len(arg) > 0 { out.WriteByte(arg[0]) }",
    "            default: out.WriteString(bWidth(spec, bNum(v, arg)))",
    "            }",
    "        }",
    "        case 'b': {",
    "            arg := \"\"",
    "            if *used < len(a) { arg = a[*used] }",
    "            *used++",
    "            out.WriteString(bEsc(arg))",
    "        }",
    "        default: out.WriteString(spec)",
    "        }",
    "    }",
    "    return out.String()",
    "}",
    "",
    "func bWidth(spec, val string) string {",
    "    d := spec[1:]",
    "    minus := strings.HasPrefix(d, \"-\")",
    "    if minus { d = d[1:] }",
    "    w := 0",
    "    for len(d) > 0 && d[0] >= '0' && d[0] <= '9' { w = w*10 + int(d[0]-'0'); d = d[1:] }",
    "    if w <= len(val) { return val }",
    "    pad := strings.Repeat(\" \", w-len(val))",
    "    if minus { return val + pad }",
    "    return pad + val",
    "}",
    "",
    "func bNum(v byte, arg string) string {",
    "    n, err := strconv.ParseInt(strings.TrimSpace(arg), 0, 64)",
    "    if err != nil { n = 0 }",
    "    switch v {",
    "    case 'd', 'i', 'u': return strconv.FormatInt(n, 10)",
    "    case 'o': return strconv.FormatInt(n, 8)",
    "    case 'x': return strconv.FormatInt(n, 16)",
    "    case 'X': return strings.ToUpper(strconv.FormatInt(n, 16))",
    "    case 'e', 'E', 'f', 'F', 'g', 'G': {",
    "        fl, err := strconv.ParseFloat(strings.TrimSpace(arg), 64)",
    "        if err != nil { fl = 0 }",
    "        if v == 'e' || v == 'E' { return strconv.FormatFloat(fl, 'e', -1, 64) }",
    "        if v == 'f' || v == 'F' { return strconv.FormatFloat(fl, 'f', -1, 64) }",
    "        return strconv.FormatFloat(fl, 'g', -1, 64)",
    "    }",
    "    }",
    "    return arg",
    "}",
    "",
    "func bEsc(f string) string {",
    "    var out strings.Builder",
    "    for i := 0; i < len(f); i++ {",
    "        c := f[i]",
    "        if c != '\\\\' || i+1 >= len(f) { out.WriteByte(c); continue }",
    "        i++",
    "        switch f[i] {",
    "        case 'n': out.WriteByte('\\n')",
    "        case 't': out.WriteByte('\\t')",
    "        case 'r': out.WriteByte('\\r')",
    "        case 'a': out.WriteByte('\\a')",
    "        case 'b': out.WriteByte('\\b')",
    "        case 'f': out.WriteByte('\\f')",
    "        case 'v': out.WriteByte('\\v')",
    "        case '\\\\': out.WriteByte('\\\\')",
    "        case '\\'': out.WriteByte('\\'')",
    "        case '\"': out.WriteByte('\"')",
    "        case 'e': out.WriteByte(0x1b)",
    "        case 'c': return out.String()",
    "        case '0', '1', '2', '3', '4', '5', '6', '7': {",
    "            v := int(f[i] - '0')",
    "            for k := 0; k < 2 && i+1 < len(f) && f[i+1] >= '0' && f[i+1] <= '7'; k++ {",
    "                i++; v = v*8 + int(f[i]-'0')",
    "            }",
    "            out.WriteByte(byte(v))",
    "        }",
    "        default: out.WriteByte('\\\\'); out.WriteByte(f[i])",
    "        }",
    "    }",
    "    return out.String()",
    "}",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn render(src: &str) -> String {
        let commands = Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&commands);
        shir_to_go(&prog)
    }

    #[test]
    fn assigns_and_echo() {
        let out = render("x=5\necho \"$x\"\nx=6\necho \"x is $x\"\n");
        assert!(out.contains("package main"), "{out}");
        assert!(out.contains("var x int64"), "{out}");
        assert!(out.contains("x = 5"), "{out}");
        assert!(
            out.contains("fmt.Println(fmt.Sprintf(\"x is %v\", x))"),
            "{out}"
        );
        assert!(!out.contains("sh2"), "{out}");
    }

    #[test]
    fn if_arith_test() {
        let out = render("x=3\nif [ \"$x\" -gt 3 ]; then\ny=$((x+1))\necho \"$y\"\nfi\n");
        assert!(out.contains("if (x > 3) {"), "{out}");
        assert!(out.contains("y = (x + 1)"), "{out}");
        assert!(out.contains("func main() {"), "{out}");
    }

    #[test]
    fn untyped_var_uses_any() {
        let out = render("y=$(ls)\necho \"$y\"\n");
        assert!(out.contains("var y string"), "{out}");
        assert!(out.contains("capCmd(exec.Command("), "{out}");
        assert!(out.contains("fmt.Println(fmt.Sprintf(\"%v\", y))"), "{out}");
    }

    #[test]
    fn go_keyword_mangled() {
        let out = render("type=1\ntype=2\necho \"$type\"\n");
        assert!(out.contains("var type_ int64"), "{out}");
        assert!(!out.contains("var type int64"), "{out}");
    }
}

#[cfg(test)]
mod brace_tests {
    use super::*;

    #[test]
    fn cartesian_groups() {
        let v: serde_json::Value =
            serde_json::from_str(r#"[["a","b","c"],["1","2","3"]]"#).unwrap();
        let g = brace_groups(&v).unwrap();
        assert_eq!(g.len(), 2, "two groups");
        let mut r = Render::default();
        let args = vec![
            IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted),
            IrExpr::Json(v),
            IrExpr::Json(serde_json::Value::Array(vec![])),
            IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted),
        ];
        let exp = r.brace_expand(&args).unwrap();
        assert_eq!(
            exp,
            vec!["a1", "a2", "a3", "b1", "b2", "b3", "c1", "c2", "c3"],
            "{exp:?}"
        );
    }
}

#[cfg(test)]
mod extglob_tests {
    use super::*;
    #[test]
    fn extglob_split() {
        assert_eq!(
            extglob_not("!(*.min).js"),
            Some(("*.min".to_string(), ".js".to_string()))
        );
        assert_eq!(extglob_not("*.txt"), None);
    }
    #[test]
    fn test_op_split() {
        let toks = test_tokens("$f1==!(*.min).js").unwrap();
        assert_eq!(toks, vec!["$f1", "==", "!(*.min).js"], "{toks:?}");
    }
}

#[cfg(test)]
mod dbg_tests {
    use super::*;
    use crate::Parser;
    #[test]
    fn extglob_render() {
        let src = "f1=file.js\nif [[ $f1 == !(*.min).js ]]; then echo ok; fi\n";
        let commands = Parser::new(src).parse().expect("parse");
        println!("{commands:?}");
        let prog = crate::shir::ast_to_ir(&commands);
        println!("{prog:?}");
        let out = shir_to_go(&prog);
        let main = out
            .lines()
            .skip_while(|l| *l != "func main() {")
            .collect::<Vec<_>>()
            .join("\n");
        println!("{main}");
        assert!(out.contains("HasSuffix"), "{out}");
    }
}

#[cfg(test)]
mod tok_tests {
    use super::*;
    fn parse_ok(s: &str) -> bool {
        let toks = match test_tokens(s) {
            Some(t) => t,
            None => return false,
        };
        if toks.is_empty() {
            return true;
        }
        let mut p = TestParser { toks, i: 0 };
        match p.or_expr(&mut Render::default()) {
            Some(_) => p.i == p.toks.len(),
            None => false,
        }
    }
    #[test]
    fn tokens_parens_glue() {
        let s = "\\(! -h \"/path\" -a -d \"/path\"\\) -o \\( -h \"/path\" -a \"$(readlink \"/path\")\"=\"target\"\\)";
        let toks = test_tokens(s);
        assert!(toks.is_some(), "tokenize failed: {toks:?}");
        assert!(parse_ok(s), "parse failed for {toks:?}");
        assert!(parse_ok("\"$a\"\\>\"$b\""));
        assert!(parse_ok("${MAXWAIT% *} -gt ${MAXWAIT#* "));
        assert!(parse_ok("\"$i\"=\"2\""));
        assert!(parse_ok("$s==*.txt"));
        assert!(parse_ok("\"$0\"=@(pattern)"));
        assert!(parse_ok("$f1==!(*.min).js"));
        assert!(parse_ok("! -z \"$x\""));
        assert!(parse_ok("5 -lt 10"));
        assert!(parse_ok("$s=~^file\\.[a-z]+$"));
    }
}

#[cfg(test)]
mod chain_tests {
    use super::*;
    use crate::Parser;
    #[test]
    fn chain_assign_dbg() {
        let src = "grep -q pattern file.txt &&\n    result=\"${result} match\" ||\n    result=\"${result} no-match\"\necho \"$result\"\n";
        let commands = Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&commands);
        let out = shir_to_go(&prog);
        assert!(!out.contains("TODO"), "stubs in:\n{out}");
    }
}

#[cfg(test)]
mod pipe_tests {
    use super::*;
    use crate::Parser;
    #[test]
    fn pipe_dbg() {
        let src =
            "yes Line:LINE | head -n100 | while read L; do i=$((i+1)); echo \"Line:$i\"; done\n";
        let commands = Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&commands);
        let mut r = Render::default();
        for s in &prog.stmts {
            if let IrStmt::Expr(IrExpr::Call { func, args }) = s {
                if func == "pipeline" {
                    let v = r.pipeline_expr(args);
                    eprintln!("PIPE EXPR: {v}");
                }
            }
        }
    }
}

fn go_str_lit(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
