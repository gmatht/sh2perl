//! Rust backend renderer — library interface (worktree-local, branch
//! `backend/rust`). Consumes the ShIR directly in-process:
//! `shir_to_rust(&IrProgram) -> String`.
//!
//! Lowering model (mirrors the proven C backend, `backends/c/src/c_backend.rs`):
//!   - every shell variable becomes a `thread_local!` static (Cell<i64> for
//!     A2-Int vars, RefCell<String> for Str/Any, RefCell<Vec<String>> for
//!     index arrays, RefCell<BTreeMap<String,String>> for assoc arrays) so
//!     function bodies can read/write the same globals as main;
//!   - native lowering for the builtins the corpus actually uses: echo
//!     (with -n/-e/-E), printf (full conversion spec), cd/export/local/
//!     declare/unset/set/shift/read (IFS field split)/let/sleep/pwd/wait/
//!     true/false/exit, `[ ... ]`/`[[ ... ]]` tests (file tests, numeric
//!     compares, pattern match via a hand-rolled glob matcher, `=~` via
//!     `bash -c '[[ $1 =~ $2 ]]'`), parameter expansion (`:-` `:=` `#` `%`
//!     `/` `//` slice case ops len basename dirname), command substitution
//!     (native statement blocks captured via a runtime output buffer, or
//!     `bash -c` for shell-out bodies), pipelines (mixed native/shell
//!     stages threaded through byte buffers), redirects (fd 0/1/2 file,
//!     heredoc, herestring, process substitution), brace expansion, glob
//!     words (`\x01SH2GLOB\x01` sentinel), arrays (setArray/index/len/
//!     join/split), case (fnmatch dispatch), functions (positionals via a
//!     runtime argv), subshells (save/restore), background jobs (real
//!     child processes for text-reconstructable bodies, threads otherwise);
//!   - everything outside the lowable subset emits a `// TODO(unsupported)`
//!     marker — the output ALWAYS compiles with bare `rustc`.
//!
//! Shell-outs run through `bash -c` (the reference shell — the corpus gate
//! diffs against bash) with word-quoting applied at runtime
//! (`__sh_q`/`__sh_wq`). The runtime helper namespace is `__sh_*` /
//! `__SH_*` — deliberately NOT `sh2.*`, which the stub gate greps for.

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap};

/// The core's marker for an UNQUOTED glob word (`` `*.txt` `` arrives as
/// `"\u{1}SH2GLOB\u{1}*.txt"`).
const GLOB_SENTINEL: &str = "\u{1}SH2GLOB\u{1}";

/// Rust keywords (edition 2021) — identifiers mangled against these, plus
/// the `sh2_`/`_sh2`/`__sh_`/`__SH` helper prefixes the renderer owns.
const RUST_RESERVED: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await",
];

/// Native (no shell-out) exec builtins — pipeline stages containing only
/// these stay in-process and capture through the runtime output buffer.
const NATIVE_CMDS: &[&str] = &[
    "echo", "printf", "exit", "cd", "export", "local", "declare", "typeset", "readonly",
    "unset", "set", "shift", "read", "let", "true", ":", "false", "pwd", "sleep", "wait",
    "break", "continue", "return", "test", "eval", "source", ".", "exec",
];

/// Word part: literal text or a runtime word-list (`Vec<String>` expr).
enum Part {
    Lit(String),
    Words(String),
}

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// vars written anywhere (declared at the top of main)
    written: BTreeSet<String>,
    /// index-array vars (Vec<String>)
    arrays: BTreeSet<String>,
    /// assoc-array vars (BTreeMap<String, String>)
    assoc: BTreeSet<String>,
    /// shell functions defined in the program
    functions: BTreeSet<String>,
    /// `typeset -i` vars (integer attribute — text assigns are arith)
    int_vars: BTreeSet<String>,
    /// `typeset -l` vars (lowercase attribute)
    lower_vars: BTreeSet<String>,
    /// `typeset -u` vars (uppercase attribute)
    upper_vars: BTreeSet<String>,
    /// var -> captured local (background-thread bodies)
    captured: HashMap<String, String>,
    /// runtime helper fns needed (dependency closure)
    helpers: BTreeSet<String>,
    /// Rust identifier per shell var name (sanitize + de-dup)
    mangle: HashMap<String, String>,
    loop_depth: usize,
    /// gensym counter for loop/block temporaries (`__sh_t0`, …)
    gensym: usize,
    /// inside a For-over-words body: the index var — a `continue` must
    /// advance it first (bash's for-iteration)
    for_index: Option<String>,
    /// the last var consumed by the arith-text parser (for ++/--)
    last_arith_var: Option<String>,
    todo: usize,
}

/// Render an `IrProgram` to Rust source (fn main()).
pub fn shir_to_rust(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
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

    fn add_helper(&mut self, h: &str) {
        if self.helpers.insert(h.to_string()) {
            for dep in helper_deps(h) {
                self.add_helper(dep);
            }
        }
    }

    /// A fresh block/loop temporary name (kept out of the `sh2*` stub-gate
    /// pattern and the user-var namespace).
    fn gensym(&mut self, base: &str) -> String {
        let n = self.gensym;
        self.gensym += 1;
        format!("{base}{n}")
    }

    fn mark_written(&mut self, name: &str) {
        self.written.insert(name.to_string());
    }

    /// Emit a `continue` — inside a For-over-words body the index must
    /// advance first (bash's for-iteration).
    fn emit_continue(&mut self) {
        if let Some(idx) = self.for_index.clone() {
            self.emit(&format!("{idx} += 1;"));
        }
        self.emit("continue;");
    }

    /// A function's Rust identifier — distinct from the var namespace
    /// (`myfunc` the var vs `myfunc()` the function can coexist).
    /// Wrap a value expr with the var's -l/-u attribute conversion.
    fn case_attr(&mut self, name: &str, v: &str) -> String {
        if self.lower_vars.contains(name) {
            format!("({v}).to_lowercase()")
        } else if self.upper_vars.contains(name) {
            format!("({v}).to_uppercase()")
        } else {
            v.to_string()
        }
    }

    fn fn_ident(&mut self, name: &str) -> String {
        format!("{}_fn", self.rust_ident(name))
    }

    /// Write an i64 value to a var (num cell, str cell or array elem 0).
    fn write_num_or_str(&mut self, name: &str, e: &str) -> String {
        if self.is_num(name) {
            self.write_num(name, e)
        } else if self.is_array(name) {
            self.array_elem_set(name, "0", &format!("({e}).to_string()"))
        } else {
            self.write_str(name, &format!("({e}).to_string()"))
        }
    }

    // ── identifiers ──────────────────────────────────────────────────

    /// Sanitize a shell var name to a valid Rust identifier and mangle
    /// reserved names. De-duplicates collisions and keeps the renderer's
    /// helper prefixes (`sh2_*`, `_sh2*`, `__sh_*`, `__SH*`) out of user
    /// vars.
    fn rust_ident(&mut self, name: &str) -> String {
        if let Some(m) = self.mangle.get(name) {
            return m.clone();
        }
        let mut m = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_ascii_alphanumeric() || c == '_' {
                m.push(c);
            } else {
                m.push('_');
            }
        }
        if m.is_empty() || m.chars().next().unwrap().is_ascii_digit() {
            m.insert_str(0, "v_");
        }
        if RUST_RESERVED.contains(&m.as_str())
            || m.starts_with("sh2_")
            || m.starts_with("_sh2")
            || m.to_ascii_lowercase().starts_with("__sh")
        {
            m.push('_');
        }
        // Every user var becomes a module-level thread_local static named
        // `__SHV_…` — case-distinct from ALL generated locals (`__sh_*`,
        // `__sh_tN`), which may never shadow a static (E0530).
        m = format!("__SHV_{m}");
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

    // ── literals ─────────────────────────────────────────────────────

    /// A Rust string literal (value context — callers append
    /// `.to_string()` where a `String` value is required).
    fn rust_str(s: &str) -> String {
        let mut out = String::new();
        out.push('"');
        for c in s.chars() {
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

    fn rust_str_expr(s: &str) -> String {
        format!("{}.to_string()", Self::rust_str(s))
    }

    // ── var classification ───────────────────────────────────────────

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int) || self.int_vars.contains(name)
    }

    fn is_array(&self, name: &str) -> bool {
        self.arrays.contains(name)
    }

    fn is_assoc(&self, name: &str) -> bool {
        self.assoc.contains(name)
    }

    fn declared(&self, name: &str) -> bool {
        self.written.contains(name)
    }

    /// The thread_local static name for a var.
    fn tls(&mut self, name: &str) -> String {
        self.rust_ident(name)
    }

    // ── var reads/writes ─────────────────────────────────────────────

    fn read_num(&mut self, name: &str) -> String {
        if let Some(l) = self.captured.get(name) {
            return l.clone();
        }
        let m = self.tls(name);
        format!("{m}.with(|v| v.get())")
    }

    fn read_str(&mut self, name: &str) -> String {
        if let Some(l) = self.captured.get(name) {
            return l.clone();
        }
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow().clone())")
    }

    fn read_num_of_str(&mut self, name: &str) -> String {
        format!("{}.trim().parse::<i64>().unwrap_or(0)", self.read_str(name))
    }

    fn read_arr(&mut self, name: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow().clone())")
    }

    fn write_num(&mut self, name: &str, e: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.set({e}));")
    }

    fn write_str(&mut self, name: &str, e: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| *v.borrow_mut() = {e});")
    }

    fn write_arr(&mut self, name: &str, e: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| *v.borrow_mut() = {e});")
    }

    fn push_arr(&mut self, name: &str, e: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow_mut().push({e}));")
    }

    fn clear_var(&mut self, name: &str) -> String {
        if self.is_assoc(name) {
            let m = self.tls(name);
            format!("{m}.with(|v| v.borrow_mut().clear());")
        } else if self.is_array(name) {
            let m = self.tls(name);
            format!("{m}.with(|v| v.borrow_mut().clear());")
        } else if self.is_num(name) {
            self.write_num(name, "0")
        } else {
            self.write_str(name, "String::new()")
        }
    }

    /// Array element read: `ARR[i]` (index arrays).
    fn array_elem(&mut self, name: &str, key: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow().get({key} as usize).cloned().unwrap_or_default())")
    }

    /// Array element write (index arrays; grows as needed). The value is
    /// evaluated BEFORE the borrow (a self-referential read would panic).
    fn array_elem_set(&mut self, name: &str, key: &str, val: &str) -> String {
        let m = self.tls(name);
        format!(
            "{{ let __val = {val}; {m}.with(|v| {{ let mut b = v.borrow_mut(); let i = {key} as usize; \
             if b.len() <= i {{ b.resize(i + 1, String::new()); }} b[i] = __val; }}); }}"
        )
    }

    /// Assoc-map read.
    fn assoc_get(&mut self, name: &str, key: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow().get(&{key}).cloned().unwrap_or_default())")
    }

    fn assoc_set(&mut self, name: &str, key: &str, val: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| {{ v.borrow_mut().insert({key}, {val}); }});")
    }

    /// `${!map[@]}` keys.
    fn assoc_keys(&mut self, name: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow().keys().cloned().collect::<Vec<String>>())")
    }

    fn array_len(&mut self, name: &str) -> String {
        let m = self.tls(name);
        format!("{m}.with(|v| v.borrow().len() as i64)")
    }

    /// Declare an array var with the given element expressions.
    fn declare_array(&mut self, name: &str, items: &str) {
        let stmt = self.write_arr(name, items);
        self.emit(&stmt);
    }

    // ── special-var reads ($?, $#, $1, env …) ───────────────────────

    /// A shell variable read as a String-typed expression. Handles the
    /// declared vars, positional params, special vars and env vars.
    fn getvar_str(&mut self, name: &str) -> String {
        if let Some(l) = self.captured.get(name) {
            return if self.is_num(name) {
                format!("{l}.to_string()")
            } else {
                l.clone()
            };
        }
        if self.declared(name) {
            if self.is_assoc(name) {
                // a bare $map read — bash gives element 0 of the keyspace;
                // the corpus uses ${map[key]} instead
                self.assoc_get(name, "\"\"")
            } else if self.is_array(name) {
                self.array_elem(name, "0")
            } else if self.is_num(name) {
                format!("{}.to_string()", self.read_num(name))
            } else {
                self.read_str(name)
            }
        } else {
            match name {
                "?" => "__SH_RC.load(Ordering::SeqCst).to_string()".to_string(),
                "$" => "std::process::id().to_string()".to_string(),
                "#" => "__SH_ARGV.lock().unwrap().len().to_string()".to_string(),
                "@" | "*" => "__SH_ARGV.lock().unwrap().join(\" \")".to_string(),
                "0" => "std::env::args().next().unwrap_or_default()".to_string(),
                "!" => {
                    // empty until a background job starts (bash's $!)
                    "{{ let __p = __SH_BGPID.load(Ordering::SeqCst); if __p == 0 {{ String::new() }} else {{ __p.to_string() }} }}".to_string()
                }
                "RANDOM" => {
                    self.add_helper("rand");
                    "__sh_rand()".to_string()
                }
                "UID" | "EUID" => {
                    self.add_helper("capture");
                    "__sh_capture(\"id -u\")".to_string()
                }
                "-" => "\"hB\".to_string()".to_string(),
                "LINENO" | "SECONDS" | "BASH_VERSION" | "BASH_SOURCE" | "FUNCNAME"
                | "BASH_LINENO" | "PPID" | "EPOCHSECONDS" | "EPOCHREALTIME"
                | "BASHPID" | "GROUPS" | "HOSTTYPE" | "MACHTYPE" | "OSTYPE"
                | "SHELLOPTS" | "BASHOPTS" | "SHLVL" | "PIPESTATUS" => {
                    self.mark_todo(&format!("special var ${name}"));
                    "String::new()".to_string()
                }
                n if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
                    self.add_helper("arg");
                    let i = n.parse::<usize>().unwrap_or(1).saturating_sub(1);
                    format!("__sh_arg({i})")
                }
                _ => {
                    self.add_helper("env");
                    format!("__sh_env({})", Self::rust_str(name))
                }
            }
        }
    }

    /// A shell variable read as an i64-typed expression.
    fn getvar_num(&mut self, name: &str) -> String {
        if let Some(l) = self.captured.get(name) {
            return if self.is_num(name) {
                l.clone()
            } else {
                format!("{l}.trim().parse::<i64>().unwrap_or(0)")
            };
        }
        if self.declared(name) {
            if self.is_num(name) {
                self.read_num(name)
            } else if self.is_array(name) {
                format!("{}.trim().parse::<i64>().unwrap_or(0)", self.array_elem(name, "0"))
            } else {
                self.read_num_of_str(name)
            }
        } else {
            match name {
                "?" => "__SH_RC.load(Ordering::SeqCst)".to_string(),
                "$" => "std::process::id() as i64".to_string(),
                "#" => "__SH_ARGV.lock().unwrap().len() as i64".to_string(),
                n if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
                    self.add_helper("arg");
                    let i = n.parse::<usize>().unwrap_or(1).saturating_sub(1);
                    format!("__sh_arg({i}).trim().parse::<i64>().unwrap_or(0)")
                }
                _ => {
                    self.add_helper("env");
                    format!("__sh_env({}).trim().parse::<i64>().unwrap_or(0)", Self::rust_str(name))
                }
            }
        }
    }

    // ── typed expressions ────────────────────────────────────────────

    /// Statically-numeric check (for comparison typing).
    fn static_num(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_) => true,
            IrExpr::Str(s, _) => s.trim().parse::<i64>().is_ok(),
            IrExpr::Var(name, _) => self.is_num(name),
            IrExpr::Arith(_) => true,
            IrExpr::BinOp { op, .. } => !matches!(op, BinOpKind::Concat),
            IrExpr::Call { func, args } if func == "getVar" => {
                matches!(args.first(), Some(IrExpr::Str(name, _)) if self.is_num(name))
            }
            _ => false,
        }
    }

    /// Render as an i64-typed expression.
    fn expr_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => {
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else {
                    format!("{}.trim().parse::<i64>().unwrap_or(0)", Self::rust_str_expr(s))
                }
            }
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.declared(name) || self.captured.contains_key(name) {
                    if self.is_num(name) {
                        self.read_num(name)
                    } else {
                        self.read_num_of_str(name)
                    }
                } else {
                    self.getvar_num(name)
                }
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            IrExpr::BinOp { lhs, op, rhs }
                if matches!(
                    op,
                    BinOpKind::Eq
                        | BinOpKind::Ne
                        | BinOpKind::Lt
                        | BinOpKind::Gt
                        | BinOpKind::Le
                        | BinOpKind::Ge
                        | BinOpKind::And
                        | BinOpKind::Or
                        | BinOpKind::Not
                ) =>
            {
                // comparisons/logicals render as bool; bash needs 1/0
                format!("({} as i64)", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Pow => {
                self.add_helper("pow");
                format!("__sh_pow({}, {})", self.expr_num(lhs), self.expr_num(rhs))
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                format!(
                    "({} {} {})",
                    self.expr_num(lhs),
                    self.arith_op(op),
                    self.expr_num(rhs)
                )
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.getvar_num(name)
                } else {
                    "0".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "arrayLen" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.is_assoc(name) {
                        let m = self.tls(name);
                        format!("{m}.with(|v| v.borrow().len() as i64)")
                    } else {
                        self.array_len(name)
                    }
                } else {
                    "0".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "arith" => {
                let text = str_arg(args, 0).unwrap_or("").replace(GLOB_SENTINEL, "");
                if let Some(e) = self.arith_text(&text) {
                    e
                } else {
                    "0".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "assign" => self.assign_call_num(args),
            other => format!("{}.trim().parse::<i64>().unwrap_or(0)", self.expr_str(other)),
        }
    }

    /// Render as a String-typed expression.
    fn expr_str(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::rust_str_expr(s),
            IrExpr::Int(i) => format!("({i}).to_string()"),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.declared(name) || self.captured.contains_key(name) {
                    if self.is_num(name) {
                        format!("{}.to_string()", self.read_num(name))
                    } else if self.is_array(name) {
                        self.array_elem(name, "0")
                    } else {
                        self.read_str(name)
                    }
                } else {
                    self.getvar_str(name)
                }
            }
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Arith(a) => format!("({}).to_string()", self.arith(a)),
            IrExpr::Bool(b) => {
                if *b { "(true).to_string()".into() } else { "(false).to_string()".into() }
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                format!("(format!(\"{{}}{{}}\", {}, {}))", self.expr_str(lhs), self.expr_str(rhs))
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.getvar_str(name)
                } else {
                    "String::new()".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "param" => self.param_str(args),
            IrExpr::Call { func, args } if func == "arrayIndex" => {
                self.array_index_str(args)
            }
            IrExpr::Call { func, args } if func == "arrayLen" => {
                format!("({}).to_string()", self.expr_num(e))
            }
            IrExpr::Call { func, args } if func == "join" => self.join_str(args),
            IrExpr::Call { func, args } if func == "capture" => self.capture_expr(args),
            IrExpr::Call { func, args } if func == "listVar" => self.listvar_joined(args),
            IrExpr::Call { func, args } if func == "split" => {
                let s = args.first().map(|a| self.expr_str(a)).unwrap_or_else(|| "String::new()".to_string());
                self.add_helper("split_ifs");
                format!("__sh_split_ifs(&{s}, \" \\t\\n\").join(\" \")")
            }
            IrExpr::Call { func, args } if func == "brace" => {
                let w = self.brace_words(args);
                self.add_helper("cat");
                format!("__sh_cat(&[{w}]).join(\" \")")
            }
            IrExpr::Call { func, args } if func == "arrayItems" => {
                args.first().map(|a| self.expr_str(a)).unwrap_or_else(|| "String::new()".to_string())
            }
            IrExpr::Call { func, args } if func == "test" => {
                let _ = args;
                // a `[ ... ]` test in a string context produces no output
                "String::new()".to_string()
            }
            IrExpr::Call { func, args } if func == "exec" => self.exec_value(args),
            IrExpr::Index { var, key } => {
                let k = self.expr_num(key);
                self.array_elem(var, &k)
            }
            other => self.expr_any(other),
        }
    }

    /// Render as a bool-typed expression (conditions).
    fn expr_bool(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Bool(b) => {
                if *b { "true".into() } else { "false".into() }
            }
            IrExpr::Int(i) => format!("({i} != 0)"),
            IrExpr::Str(s, _) => format!("(!{}.is_empty())", Self::rust_str_expr(s)),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.declared(name) || self.captured.contains_key(name) {
                    if self.is_num(name) {
                        format!("({} != 0)", self.read_num(name))
                    } else {
                        format!("(!{}.is_empty())", self.read_str(name))
                    }
                } else {
                    let s = self.getvar_str(name);
                    format!("(!{s}.is_empty())")
                }
            }
            IrExpr::BinOp { lhs, op, rhs } => match op {
                BinOpKind::And => format!("({} && {})", self.expr_bool(lhs), self.expr_bool(rhs)),
                BinOpKind::Or => format!("({} || {})", self.expr_bool(lhs), self.expr_bool(rhs)),
                BinOpKind::Not => format!("(!{})", self.expr_bool(lhs)),
                BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt
                | BinOpKind::Le | BinOpKind::Ge => {
                    let rs_op = self.cmp_op(op);
                    if self.static_num(lhs) && self.static_num(rhs) {
                        let (l, r) = (self.expr_num(lhs), self.expr_num(rhs));
                        format!("({l} {rs_op} {r})")
                    } else {
                        let (l, r) = (self.expr_str(lhs), self.expr_str(rhs));
                        format!("({l} {rs_op} {r})")
                    }
                }
                _ => format!("({} != 0)", self.expr_num(e)),
            },
            IrExpr::Arith(a) => format!("({} != 0)", self.arith(a)),
            IrExpr::Call { func, args } if func == "test" => self.test_call_bool(args),
            IrExpr::Call { func, args } if func == "grepMatches" => {
                self.add_helper("grepmatches");
                let text = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
                let pat = args.get(1).map(|a| self.expr_str(a)).unwrap_or_default();
                let flags = args.get(2).map(|a| self.expr_str(a)).unwrap_or_default();
                format!(
                    "{{ let (__m, __r) = __sh_grepmatches(&{text}, &{pat}, &{flags}); __SH_RC.store(__r, Ordering::SeqCst); __r == 0 }}"
                )
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) || self.captured.contains_key(name) {
                        if self.is_num(name) {
                            format!("({} != 0)", self.read_num(name))
                        } else {
                            format!("(!{}.is_empty())", self.read_str(name))
                        }
                    } else {
                        let s = self.getvar_str(name);
                        format!("(!{s}.is_empty())")
                    }
                } else {
                    "false".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "exec" => self.exec_bool(args),
            IrExpr::Call { func, args } if func == "capture" => {
                format!("(!{}.is_empty())", self.capture_expr(args))
            }
            IrExpr::Call { func, args } if func == "block" => self.block_bool(args),
            IrExpr::Call { func, args } if func == "redirect" => self.redirect_bool(args),
            IrExpr::Call { func, args } if func == "pipeline" => self.pipeline_bool(args),
            IrExpr::Call { func, args } if func == "whileLoop" => self.whileloop_bool(args),
            IrExpr::Call { func, args } if func == "contains" => {
                self.contains_bool(args)
            }
            IrExpr::Call { func, args } if func == "arith" => {
                let text = str_arg(args, 0).unwrap_or("").replace(GLOB_SENTINEL, "");
                if let Some(e) = self.arith_text(&text) {
                    format!("{{ let __v = ({e}); __SH_RC.store(if __v != 0 {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __v != 0 }}")
                } else {
                    "{{ __SH_RC.store(1, Ordering::SeqCst); false }}".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "assign" => {
                // run the assignment; the lhs's rc (e.g. a capture's)
                // decides the condition
                let block = self.assign_call_str(args);
                format!("{{ let _ = {block}; __SH_RC.load(Ordering::SeqCst) == 0 }}")
            }
            IrExpr::Call { func, args } if func == "return" => {
                "{ __SH_RC.store(0, Ordering::SeqCst); return; }".to_string()
            }
            IrExpr::Call { func, args } if func == "break" => {
                if self.loop_depth > 0 {
                    "{ break; false }".to_string()
                } else {
                    "false".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "continue" => {
                if self.loop_depth > 0 {
                    "{ continue; false }".to_string()
                } else {
                    "false".to_string()
                }
            }
            IrExpr::Call { func, args } if func == "arrayIndex" => {
                format!("(!{}.is_empty())", self.array_index_str(args))
            }
            IrExpr::Call { func, args } if func == "param" => {
                format!("(!{}.is_empty())", self.param_str(args))
            }
            IrExpr::Call { func, args } if func == "subshell" => self.subshell_bool(args),
            IrExpr::Call { func, args } if func == "and" => self.and_bool(args),
            other => format!("(!{}.is_empty())", self.expr_any(other)),
        }
    }

    fn cmp_op(&self, op: &BinOpKind) -> &'static str {
        match op {
            BinOpKind::Eq => "==",
            BinOpKind::Ne => "!=",
            BinOpKind::Lt => "<",
            BinOpKind::Gt => ">",
            BinOpKind::Le => "<=",
            BinOpKind::Ge => ">=",
            _ => "==",
        }
    }

    /// Render as a String-typed expression (the general form — the
    /// runtime store is a String in this draft, so "any" == String).
    fn expr_any(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::rust_str_expr(s),
            IrExpr::Int(i) => format!("({i}).to_string()"),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.declared(name) || self.captured.contains_key(name) {
                    if self.is_num(name) {
                        format!("{}.to_string()", self.read_num(name))
                    } else if self.is_array(name) {
                        self.array_elem(name, "0")
                    } else {
                        self.read_str(name)
                    }
                } else {
                    self.getvar_str(name)
                }
            }
            IrExpr::Bool(b) => {
                if *b { "(true).to_string()".into() } else { "(false).to_string()".into() }
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::rust_str_expr(s),
                serde_json::Value::Number(n) => format!("({n}).to_string()"),
                serde_json::Value::Bool(b) => {
                    if *b { "(true).to_string()".into() } else { "(false).to_string()".into() }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "String::new()".into()
                }
            },
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                format!("(format!(\"{{}}{{}}\", {}, {}))", self.expr_str(lhs), self.expr_str(rhs))
            }
            IrExpr::BinOp { op, .. }
                if matches!(op, BinOpKind::And | BinOpKind::Or | BinOpKind::Not)
                    || matches!(
                        op,
                        BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt
                        | BinOpKind::Gt | BinOpKind::Le | BinOpKind::Ge
                    ) =>
            {
                format!("({} as i64).to_string()", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Pow => {
                format!("__sh_pow({}, {}).to_string()", self.expr_num(lhs), self.expr_num(rhs))
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                format!(
                    "({} {} {}).to_string()",
                    self.expr_num(lhs),
                    self.arith_op(op),
                    self.expr_num(rhs)
                )
            }
            IrExpr::Arith(a) => format!("({}).to_string()", self.arith(a)),
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Ternary { cond, then, else_ } => format!(
                "(if {} {{ {} }} else {{ {} }})",
                self.expr_bool(cond),
                self.expr_any(then),
                self.expr_any(else_)
            ),
            IrExpr::DefinedOr { .. } => {
                self.mark_todo("DefinedOr");
                "String::new()".to_string()
            }
            IrExpr::Index { var, key } => {
                let k = self.expr_num(key);
                self.array_elem(var, &k)
            }
            IrExpr::Capture { .. } => self.capture_expr_single(e),
            IrExpr::Regex { .. } => {
                self.mark_todo("Regex expr");
                "String::new()".to_string()
            }
            IrExpr::Range { .. } => {
                self.mark_todo("Range expr");
                "String::new()".into()
            }
            IrExpr::RawExpr(_) => {
                self.mark_todo("RawExpr");
                "String::new()".into()
            }
            IrExpr::Arrow(_) => {
                self.mark_todo("Arrow");
                "String::new()".into()
            }
            IrExpr::Array(_) => {
                self.mark_todo("Array expr");
                "String::new()".into()
            }
            IrExpr::Object(_) => {
                self.mark_todo("Object");
                "String::new()".into()
            }
            IrExpr::Call { func, args } => self.call_str(func, args),
            IrExpr::MethodCall { .. } => {
                self.mark_todo("MethodCall");
                "String::new()".to_string()
            }
            IrExpr::ArrayComp { .. } => {
                self.mark_todo("ArrayComp expr");
                "String::new()".to_string()
            }
            IrExpr::Lambda { .. } => {
                self.mark_todo("Lambda expr");
                "String::new()".to_string()
            }
            IrExpr::Splice(_) => {
                self.mark_todo("Splice expr");
                "String::new()".to_string()
            }
        }
    }

    /// String interpolation: "hello $name" → format!(...) (String).
    fn interpolate(&mut self, parts: &[InterpPart]) -> String {
        let mut fmt = String::new();
        let mut raw = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => {
                    fmt.push_str(&s.replace('{', "{{").replace('}', "}}"));
                    raw.push_str(s);
                }
                InterpPart::Expr(x) => {
                    fmt.push_str("{}");
                    args.push(self.expr_any(x));
                }
            }
        }
        if args.is_empty() {
            Self::rust_str_expr(&raw)
        } else {
            format!("format!({}, {})", Self::rust_fmt(&fmt), args.join(", "))
        }
    }

    fn rust_fmt(s: &str) -> String {
        Self::rust_str(s)
    }

    // ── arithmetic (native i64) ──────────────────────────────────────

    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) | ArithAst::Ident(name) => self.getvar_num(name),
            ArithAst::Index { var, key } => {
                let k = self.arith(key);
                let e = self.array_elem(var, &k);
                format!("{e}.trim().parse::<i64>().unwrap_or(0)")
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                match op.as_str() {
                    "**" => {
                        self.add_helper("pow");
                        format!("__sh_pow({l},{r})")
                    }
                    "&&" => format!("(({l} != 0 && {r} != 0) as i64)"),
                    "||" => format!("(({l} != 0 || {r} != 0) as i64)"),
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        format!("(({l} {op} {r}) as i64)")
                    }
                    ">>" | "<<" => format!("({l} {op} {r})"),
                    _ => format!("({l} {op} {r})"),
                }
            }
            ArithAst::Un { op, arg } => {
                let a = self.arith(arg);
                match op.as_str() {
                    "!" => format!("(({a} == 0) as i64)"),
                    "~" => format!("(!{a})"),
                    _ => format!("({op}{a})"),
                }
            }
            ArithAst::Cond { test, then, else_ } => format!(
                "(if ({} != 0) {{ {} }} else {{ {} }})",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::Assign { var, op, rhs } => {
                let r = self.arith(rhs);
                let cur = self.getvar_num(var);
                match op.as_str() {
                    "=" => format!("{{ let __v = {r}; {} __v }}", self.write_num_or_str(var, "__v")),
                    _ => {
                        let aop = op.trim_end_matches('=');
                        let stmt = self.write_num_or_str(var, &format!("({cur} {aop} {r})"));
                        let new = self.getvar_num(var);
                        format!("{{ {stmt} {new} }}")
                    }
                }
            }
            ArithAst::IncDec { var, delta, prefix } => {
                let d = if *delta >= 0 {
                    format!("+{delta}")
                } else {
                    format!("{delta}")
                };
                let cur = self.getvar_num(var);
                let m = self.tls(var);
                let stmt = self.write_num_or_str(var, &format!("({cur} {d})"));
                if *prefix {
                    let new = self.getvar_num(var);
                    format!("{{ {stmt} {new} }}")
                } else {
                    format!("{{ let __o = {cur}; {stmt} __o }}")
                }
            }
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
            _ => {
                self.mark_todo("arith node");
                "0".into()
            }
        }
    }

    fn arith_op(&self, op: &BinOpKind) -> &'static str {
        match op {
            BinOpKind::Add => "+",
            BinOpKind::Sub => "-",
            BinOpKind::Mul => "*",
            BinOpKind::Div => "/",
            BinOpKind::Mod => "%",
            BinOpKind::BitAnd => "&",
            BinOpKind::BitOr => "|",
            BinOpKind::BitXor => "^",
            BinOpKind::ShiftL => "<<",
            BinOpKind::ShiftR => ">>",
            BinOpKind::Pow => "**",
            _ => "+",
        }
    }

    // ── exec dispatch ────────────────────────────────────────────────

    /// An `exec` call rendered as a statement (with rc update).
    fn exec_stmt(&mut self, args: &[IrExpr]) {
        let Some(cmd) = str_arg(args, 0) else {
            self.mark_todo("exec without cmd");
            return;
        };
        let words: Vec<&IrExpr> = match args.get(1) {
            Some(IrExpr::Array(items)) => items.iter().collect(),
            _ => vec![],
        };
        let env = exec_env(args);
        if self.functions.contains(cmd) {
            let call = self.fn_call_stmt(cmd, &words);
            self.emit(&call);
            return;
        }
        match cmd {
            "echo" => self.echo_stmt(&words),
            "printf" => self.printf_stmt(&words),
            "exit" => {
                let code = match words.first() {
                    Some(w) => self.expr_num(w),
                    None => "0".to_string(),
                };
                self.emit(&format!("std::process::exit(({code}) as i32);"));
            }
            "cd" => {
                let e = self.cd_expr(&words);
                self.emit(&format!("let _ = {e};"));
            }
            "export" | "local" | "declare" | "typeset" | "readonly" => {
                let exported = matches!(cmd, "export" | "readonly");
                self.decl_words(&words, exported);
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "unset" => {
                self.unset_words(&words);
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "set" => {
                self.set_words(&words);
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "shift" => {
                let n = match words.first() {
                    Some(w) => self.expr_num(w),
                    None => "1".to_string(),
                };
                self.emit(&format!(
                    "{{ let mut __v = __SH_ARGV.lock().unwrap(); let __n = ({n}).min(__v.len() as i64) as usize; __v.drain(0..__n); }}"
                ));
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "read" => {
                let ifs = env_ifs(&env);
                let e = self.read_expr(&words, ifs);
                self.emit(&format!("let _ = {e};"));
            }
            "let" => {
                let text = words
                    .iter()
                    .map(|w| word_source_text(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(e) = self.let_expr(&text) {
                    self.emit(&format!(
                        "let _ = {{ let __v = ({e}); __SH_RC.store(if __v != 0 {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __v }};"
                    ));
                } else {
                    self.mark_todo("let");
                    self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                }
            }
            "shopt" => {
                // `shopt -s/-u opt` — option toggles (nullglob, extglob…)
                // don't affect the native lowering; a no-op is faithful
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "true" | ":" => {
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "false" => {
                self.emit("__SH_RC.store(1, Ordering::SeqCst);");
            }
            "pwd" => {
                self.add_helper("print_words");
                self.emit(&format!(
                    "__sh_print_words(&[vec![std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()]], true, false);"
                ));
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "sleep" => {
                let v = match words.first() {
                    Some(w) => self.expr_str(w),
                    None => "\"0\"".to_string(),
                };
                self.add_helper("sleep");
                self.emit(&format!("__sh_sleep(&{v});"));
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "wait" => {
                self.add_helper("wait_all");
                self.emit("__sh_wait_all();");
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            "break" => {
                if self.loop_depth > 0 {
                    self.emit("break;");
                }
            }
            "continue" => {
                if self.loop_depth > 0 {
                    self.emit_continue();
                }
            }
            "test" => {
                let text = words
                    .iter()
                    .map(|w| word_source_text(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(c) = self.test_expr(&text, "[") {
                    self.emit(&format!(
                        "let _ = {{ let __b = {c}; __SH_RC.store(if __b {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __b }};"
                    ));
                } else {
                    self.mark_todo("test");
                    self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                }
            }
            "eval" => {
                // `eval "y=$x+1"` — a plain assignment string evaluates
                // in the CURRENT shell (a shell-out would lose it)
                let joined = words
                    .iter()
                    .map(|w| word_source_text(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some((name, rest)) = joined.trim().split_once('=') {
                    if !name.is_empty()
                        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        let v = self.dollar_interp(rest);
                        self.mark_written(name);
                        let st = if self.is_num(name) {
                            self.write_num(name, &format!("{v}.trim().parse::<i64>().unwrap_or(0)"))
                        } else if self.is_array(name) {
                            self.array_elem_set(name, "0", &v)
                        } else {
                            self.write_str(name, &v)
                        };
                        self.emit(&st);
                        self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                        return;
                    }
                }
                let text = self.cmd_text(&words, None);
                self.add_helper("run");
                self.emit(&format!("__SH_RC.store(__sh_run(&{text}), Ordering::SeqCst);"));
            }
            "command" => {
                let text = self.cmd_text(&words, None);
                self.add_helper("run");
                self.emit(&format!("__SH_RC.store(__sh_run(&{text}), Ordering::SeqCst);"));
            }
            "source" | "." => {
                // `. file args` — the cmd word itself is part of the text
                let cmd_word = IrExpr::Str(cmd.to_string(), crate::ir::StrStyle::DoubleQuoted);
                let mut all: Vec<&IrExpr> = vec![&cmd_word];
                all.extend(words.iter());
                let text = self.cmd_text(&all, None);
                self.add_helper("run");
                self.emit(&format!("__SH_RC.store(__sh_run(&{text}), Ordering::SeqCst);"));
            }
            "exec" => {
                // `exec cmd args` — run the command, then exit with its rc
                let text = self.cmd_text(&words, None);
                self.add_helper("run");
                self.emit(&format!("std::process::exit(__sh_run(&{text}));"));
            }
            _ => {
                // unknown builtin or external command — bash -c shell-out
                let cmd_word = IrExpr::Str(cmd.to_string(), crate::ir::StrStyle::DoubleQuoted);
                let mut all: Vec<&IrExpr> = vec![&cmd_word];
                all.extend(words.iter());
                let text = self.cmd_text(&all, env.as_deref());
                self.add_helper("run");
                self.emit(&format!("__SH_RC.store(__sh_run(&{text}), Ordering::SeqCst);"));
            }
        }
    }

    /// An `exec` call rendered as a bool expression (conditions).
    fn exec_bool(&mut self, args: &[IrExpr]) -> String {
        let Some(cmd) = str_arg(args, 0) else {
            return "false".to_string();
        };
        let words: Vec<&IrExpr> = match args.get(1) {
            Some(IrExpr::Array(items)) => items.iter().collect(),
            _ => vec![],
        };
        let env = exec_env(args);
        if self.functions.contains(cmd) {
            return self.fn_call_bool(cmd, &words);
        }
        match cmd {
            "cd" => self.cd_expr(&words),
            "read" => {
                let ifs = env_ifs(&env);
                self.read_expr(&words, ifs)
            }
            "let" => {
                let text = words
                    .iter()
                    .map(|w| word_source_text(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(e) = self.let_expr(&text) {
                    format!("{{ let __v = ({e}); __SH_RC.store(if __v != 0 {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __v != 0 }}")
                } else {
                    self.mark_todo("let");
                    "true".to_string()
                }
            }
            "test" => {
                let text = words
                    .iter()
                    .map(|w| word_source_text(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(c) = self.test_expr(&text, "[") {
                    format!("{{ let __b = {c}; __SH_RC.store(if __b {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __b }}")
                } else {
                    self.mark_todo("test");
                    "true".to_string()
                }
            }
            "true" | ":" => {
                "({ __SH_RC.store(0, Ordering::SeqCst); true })".to_string()
            }
            "false" => {
                "({ __SH_RC.store(1, Ordering::SeqCst); false })".to_string()
            }
            "sleep" => {
                let v = match words.first() {
                    Some(w) => self.expr_str(w),
                    None => "\"0\"".to_string(),
                };
                self.add_helper("sleep");
                format!("{{ __sh_sleep(&{v}); __SH_RC.store(0, Ordering::SeqCst); true }}")
            }
            "exit" => {
                let code = match words.first() {
                    Some(w) => self.expr_num(w),
                    None => "0".to_string(),
                };
                format!("{{ std::process::exit(({code}) as i32); }}")
            }
            "echo" | "printf" | "export" | "local" | "declare" | "typeset" | "readonly"
            | "unset" | "set" | "shift" | "pwd" | "wait" | "eval" | "source" | "."
            | "command" | "exec" | "break" | "continue" => {
                // a builtin in a condition: run it, rc decides
                let mut saved = std::mem::take(&mut self.out);
                let old_depth = self.depth;
                self.depth = 0;
                self.exec_stmt(args);
                self.emit("__SH_RC.load(Ordering::SeqCst) == 0");
                let block = self.out.join("\n");
                self.out = saved;
                self.depth = old_depth;
                format!("{{\n{block}\n}}")
            }
            _ => {
                let cmd_word = IrExpr::Str(cmd.to_string(), crate::ir::StrStyle::DoubleQuoted);
                let mut all: Vec<&IrExpr> = vec![&cmd_word];
                all.extend(words.iter());
                let text = self.cmd_text(&all, env.as_deref());
                self.add_helper("run");
                format!("{{ let __r = __sh_run(&{text}); __SH_RC.store(__r, Ordering::SeqCst); __r == 0 }}")
            }
        }
    }

    /// An `exec` call rendered as a String value (rare: `echo`/`printf`
    /// return their formatted text, other commands run and yield "").
    fn exec_value(&mut self, args: &[IrExpr]) -> String {
        let Some(cmd) = str_arg(args, 0) else {
            return "String::new()".to_string();
        };
        let words: Vec<&IrExpr> = match args.get(1) {
            Some(IrExpr::Array(items)) => items.iter().collect(),
            _ => vec![],
        };
        match cmd {
            "echo" => {
                let (parts, nl, esc) = self.echo_parts(&words);
                let ws = parts
                    .into_iter()
                    .map(|p| match p {
                        Part::Lit(t) => format!("vec![{}]", Self::rust_str_expr(&t)),
                        Part::Words(w) => w,
                    })
                    .collect::<Vec<_>>();
                self.add_helper("cat");
                let joined = format!("__sh_cat(&[{}])", ws.join(", "));
                if esc {
                    self.add_helper("echo_esc");
                    let mut s = format!("__sh_echo_esc(&{joined})");
                    if nl {
                        s = format!("format!(\"{{}}\\n\", {s})");
                    }
                    s
                } else if nl {
                    format!("format!(\"{{}}\\n\", {joined})")
                } else {
                    joined
                }
            }
            "printf" => {
                let fmt = words.first().map(|w| self.expr_str(w)).unwrap_or_default();
                let arg_exprs: Vec<String> =
                    words.iter().skip(1).map(|w| self.expr_str(w)).collect();
                self.add_helper("printf");
                format!("__sh_printf(&{fmt}, &[{}])", arg_exprs.join(", "))
            }
            _ => {
                let cmd_word = IrExpr::Str(cmd.to_string(), crate::ir::StrStyle::DoubleQuoted);
                let mut all: Vec<&IrExpr> = vec![&cmd_word];
                all.extend(words.iter());
                let text = self.cmd_text(&all, None);
                self.add_helper("run");
                format!("{{ let _ = __sh_run(&{text}); String::new() }}")
            }
        }
    }

    /// bash `cd` — chdir + PWD sync; returns a bool block expr.
    fn cd_expr(&mut self, words: &[&IrExpr]) -> String {
        // `cd -- dir` / `cd -` — skip the flag words
        let mut rest = words;
        while let Some(IrExpr::Str(f, _)) = rest.first().copied() {
            if f == "--" || f == "-" {
                rest = &rest[1..];
            } else {
                break;
            }
        }
        let dir = match rest.first() {
            Some(w) => self.expr_str(w),
            None => {
                self.add_helper("env");
                "__sh_env(\"HOME\")".to_string()
            }
        };
        format!(
            "{{ let __d = {dir}; let __r = std::env::set_current_dir(&__d); \
             if __r.is_ok() {{ std::env::set_var(\"PWD\", std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()); }} \
             __SH_RC.store(if __r.is_ok() {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __r.is_ok() }}"
        )
    }

    /// `export X=1` / `declare -a arr=(...)` / `local x=$1` — word assigns.
    /// `exported` — also write the value into the process env so shell-out
    /// children (bash -c) see it.
    fn decl_words(&mut self, words: &[&IrExpr], exported: bool) {
        let mut i = 0;
        while i < words.len() {
            if let Some(ws) = str_arg(&[(*words[i]).clone()], 0) {
                if ws.starts_with('-') {
                    // -a / -A / -x / -r / -i / -l / -u / -n / -p — the
                    // -a/-A mark the NEXT name as an array/assoc; -i/-l/-u
                    // set attributes.
                    if ws == "-a" || ws == "-A" || ws == "-i" || ws == "-l" || ws == "-u" {
                        if let Some(n) = words.get(i + 1).and_then(|w| {
                            str_arg(&[(*w).clone()], 0)
                                .map(|s| s.split('=').next().unwrap_or(s).to_string())
                        }) {
                            if ws == "-A" {
                                self.assoc.insert(n.clone());
                            } else if ws == "-a" {
                                self.arrays.insert(n.clone());
                            } else if ws == "-i" {
                                self.int_vars.insert(n.clone());
                            } else if ws == "-l" {
                                self.lower_vars.insert(n.clone());
                            } else if ws == "-u" {
                                self.upper_vars.insert(n.clone());
                            }
                            self.mark_written(&n);
                        }
                    }
                    i += 1;
                    continue;
                }
                if ws == "setArray" || ws == "setArrayAppend" {
                    // `local -a files=(...)` — the array value as a call
                    if let IrExpr::Call { func, args } = words[i] {
                        if func == "setArray" {
                            self.array_call_stmt(&args[..]);
                        } else {
                            self.array_append_stmt(&args[..]);
                        }
                    }
                    i += 1;
                    continue;
                }
                if let Some((name, val)) = ws.split_once('=') {
                    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        // `local x=$1` — the core splits `x=` and the VALUE
                        // EXPR into separate word args
                        let value_expr: Option<&IrExpr> = if val.is_empty()
                            && i + 1 < words.len()
                            && !matches!(words[i + 1], IrExpr::Str(_, _))
                        {
                            i += 1;
                            Some(words[i])
                        } else {
                            None
                        };
                        let stmt = if let Some(e) = value_expr {
                            self.assign_stmt_for(&name.to_string(), e)
                        } else if self.is_num(&name) {
                            match val.trim().parse::<i64>() {
                                Ok(n) => self.write_num(&name, &n.to_string()),
                                Err(_) => self.write_num(&name, "0"),
                            }
                        } else if self.is_array(&name) {
                            self.mark_todo("array word assign");
                            String::new()
                        } else {
                            let v = if val.contains('$') {
                                // `local file=$1` — the value is source text
                                self.dollar_interp(val)
                            } else {
                                Self::rust_str_expr(val)
                            };
                            let cv = self.case_attr(&name, &v);
                            self.write_str(&name, &cv)
                        };
                        self.emit(&stmt);
                        self.mark_written(&name.to_string());
                        if exported {
                            // export: the value must reach bash -c children
                            let ve = if let Some(e) = value_expr {
                                self.expr_any(e)
                            } else {
                                Self::rust_str_expr(val)
                            };
                            self.emit(&format!(
                                "std::env::set_var({}, &{ve});",
                                Self::rust_str(&name)
                            ));
                        }
                    }
                } else if ws.contains('[') {
                    // `declare -A matrix` bare decl handled above; an
                    // array-name decl
                    self.mark_written(ws);
                } else if !ws.is_empty() {
                    // a bare name — already declared by the hoist
                    self.mark_written(ws);
                }
            }
            i += 1;
        }
    }

    /// `unset x` / `unset arr[i]`.
    fn unset_words(&mut self, words: &[&IrExpr]) {
        for w in words {
            if let Some(name) = str_arg(&[(*w).clone()], 0) {
                if let Some(open) = name.find('[') {
                    if name.ends_with(']') {
                        let var = name[..open].to_string();
                        let key = name[open + 1..name.len() - 1].to_string();
                        self.mark_written(&var);
                        if self.is_assoc(&var) {
                            let k = self.assoc_key_expr(&key);
                            let m = self.tls(&var);
                            let st = format!("{m}.with(|v| {{ v.borrow_mut().remove(&{k}); }});");
                            self.emit(&st);
                        } else if self.is_array(&var) {
                            let m = self.tls(&var);
                            let ki = self.key_num_expr(&key);
                            let st = format!(
                                "{m}.with(|v| {{ let mut b = v.borrow_mut(); let i = {ki} as usize; if i < b.len() {{ b[i] = String::new(); }} }});"
                            );
                            self.emit(&st);
                        }
                        continue;
                    }
                }
                if name.starts_with('-') {
                    continue;
                }
                self.mark_written(name);
                let st = self.clear_var(name);
                self.emit(&st);
                std::env::remove_var(name);
            }
        }
    }

    /// `set -e` etc. (no-ops) / `set -- args` (positionals).
    fn set_words(&mut self, words: &[&IrExpr]) {
        if let Some(IrExpr::Str(flag, _)) = words.first().copied() {
            if flag == "--" {
                let ws: Vec<String> = words
                    .iter()
                    .skip(1)
                    .map(|w| self.words_expr(w))
                    .collect();
                self.add_helper("cat");
                self.emit(&format!(
                    "let __new = __sh_cat(&[{}]); *__SH_ARGV.lock().unwrap() = __new;",
                    ws.join(", ")
                ));
                return;
            }
            if flag == "-" {
                return;
            }
        }
        // flags / bare set — no-ops (the corpus scripts succeed without
        // errexit/pipefail emulation)
    }

    /// `read [-r] var...` — reads a line (respecting the current
    /// __SH_STDIN), splits on IFS, assigns the vars; bool block expr.
    fn read_expr(&mut self, words: &[&IrExpr], ifs: Option<&str>) -> String {
        let mut targets: Vec<&IrExpr> = Vec::new();
        let mut i = 0;
        while i < words.len() {
            if let Some(f) = str_arg(&[(*words[i]).clone()], 0) {
                if f.starts_with('-') && f != "-" {
                    if f == "-p" || f == "-n" || f == "-N" || f == "-d" || f == "-t" {
                        i += 2; // flag + its argument
                    } else {
                        i += 1; // -r -s -a -e etc.
                    }
                    continue;
                }
            }
            targets.push(words[i]);
            i += 1;
        }
        let n = targets.len();
        let ifs_lit = ifs.unwrap_or(" \t\n");
        let mut lines = Vec::new();
        self.add_helper("readline");
        self.add_helper("read_fields");
        lines.push("let (__ln, __any) = __sh_readline();".to_string());
        lines.push(format!(
            "let __f = __sh_read_fields(&__ln, {}, {n});",
            Self::rust_str(ifs_lit)
        ));
        for (idx, t) in targets.iter().enumerate() {
            if let Some(name) = str_arg(&[(*t).clone()], 0) {
                if name.starts_with('-') || name.is_empty() {
                    continue;
                }
                self.mark_written(name);
                if self.is_assoc(name) || self.is_array(name) {
                    // a map/array var can't take a read line — skip
                    continue;
                }
                let get = format!("__f.get({idx}).cloned().unwrap_or_default()");
                if self.is_num(name) {
                    lines.push(self.write_num(name, &format!("{get}.trim().parse::<i64>().unwrap_or(0)")));
                } else {
                    lines.push(self.write_str(name, &get));
                }
            }
        }
        lines.push("__SH_RC.store(if __any { 0 } else { 1 }, Ordering::SeqCst);".to_string());
        lines.push("__any".to_string());
        format!("{{\n{}\n}}", lines.join("\n"))
    }

    /// Render an expression as a statement (exec/pipeline dispatch).
    fn expr_stmt_value(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Call { func, args } if func == "exec" => self.exec_stmt(args),
            IrExpr::Call { func, args } if func == "pipeline" => {
                let stages = pipeline_stages(args);
                self.pipeline_stmt(&stages);
            }
            IrExpr::Call { func, args } if func == "setArray" || func == "setArrayAppend" => {
                self.array_call_stmt_by_name(func, args);
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            IrExpr::Call { func, args } if func == "redirect" => {
                let mut stmts: Vec<IrStmt> = Vec::new();
                let mut redirs: Vec<IrRedirectInfo> = Vec::new();
                if let Some(IrExpr::Arrow(b)) = args.first() {
                    stmts = b.clone();
                }
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        if let Some(r) = self.redirect_info(it) {
                            redirs.push(r);
                        }
                    }
                }
                self.redirect_render(&stmts, &redirs);
            }
            IrExpr::Call { func, args } if func == "whileLoop" => {
                let b = self.whileloop_bool(args);
                self.emit(&format!("let _ = {b};"));
            }
            IrExpr::Call { func, args } if func == "subshell" => {
                let mut stmts: Vec<IrStmt> = Vec::new();
                if let Some(IrExpr::Arrow(b)) = args.first() {
                    stmts = b.clone();
                }
                self.subshell_render(&stmts);
            }
            IrExpr::Call { func, args } if func == "test" => {
                let b = self.test_call_bool(args);
                self.emit(&format!("let _ = {b};"));
            }
            IrExpr::Call { func, args } if func == "grepMatches" => {
                self.grepmatches_stmt(args);
            }
            IrExpr::Call { func, args } if func == "and" => {
                self.and_stmt(args);
            }
            IrExpr::Call { func, .. } if func == "break" => {
                if self.loop_depth > 0 {
                    self.emit("break;");
                }
            }
            IrExpr::Call { func, .. } if func == "continue" => {
                if self.loop_depth > 0 {
                    self.emit_continue();
                }
            }
            IrExpr::Call { func, .. } if func == "return" => {
                self.emit("__SH_RC.store(0, Ordering::SeqCst); return;");
            }
            _ => {
                let x = self.expr_any(e);
                self.emit(&format!("let _ = {x};"));
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
        }
    }

    /// `let x=1+2` / `let x++` / `let i < n` — a mini arithmetic-text
    /// parser producing a native i64 expression.
    fn let_expr(&mut self, text: &str) -> Option<String> {
        let t = text.trim().replace(GLOB_SENTINEL, "");
        let t = t.trim();
        if t.is_empty() {
            return Some("0".to_string());
        }
        // `name op= rhs` / `name++` / `++name` / `name--` / `--name`
        if let Some((name, rest0)) = split_ident_rest(t) {
            let rest = rest0.trim_start();
            if !name.is_empty() {
                if let Some(rhs) = rest.strip_prefix("++") {
                    if rhs.trim().is_empty() {
                        return Some(self.incdec(name, 1, false));
                    }
                }
                if let Some(rhs) = rest.strip_prefix("--") {
                    if rhs.trim().is_empty() {
                        return Some(self.incdec(name, -1, false));
                    }
                }
                if rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("!=")
                    && !rest.starts_with("<=") && !rest.starts_with(">=")
                {
                    let rhs = rest[1..].trim();
                    if !rhs.is_empty() {
                        let re = self.arith_text(rhs)?;
                        let stmt = self.write_num_or_str(name, &re);
                        return Some(format!("{{ {stmt} {} }}", self.getvar_num(name)));
                    }
                    return None;
                }
                for op in ["+=", "-=", "*=", "/=", "%=", "<<=", ">>=", "&=", "|=", "^="] {
                    if let Some(rhs) = rest.strip_prefix(op) {
                        let rhs = rhs.trim();
                        if rhs.is_empty() {
                            return None;
                        }
                        let re = self.arith_text(rhs)?;
                        let cur = self.getvar_num(name);
                        let aop = op.trim_end_matches('=');
                        let stmt = self.write_num_or_str(name, &format!("({cur} {aop} {re})"));
                        return Some(format!("{{ {stmt} {} }}", self.getvar_num(name)));
                    }
                }
            }
        }
        if let Some(rest) = t.strip_prefix("++") {
            let name = rest.trim();
            if is_ident(name) {
                return Some(self.incdec(name, 1, true));
            }
        }
        if let Some(rest) = t.strip_prefix("--") {
            let name = rest.trim();
            if is_ident(name) {
                return Some(self.incdec(name, -1, true));
            }
        }
        self.arith_text(t)
    }

    fn incdec(&mut self, name: &str, delta: i64, prefix: bool) -> String {
        let d = if delta >= 0 { format!("+{delta}") } else { format!("{delta}") };
        let cur = self.getvar_num(name);
        let stmt = self.write_num_or_str(name, &format!("({cur} {d})"));
        if prefix {
            let new = self.getvar_num(name);
            format!("{{ {stmt} {new} }}")
        } else {
            format!("{{ let __o = {cur}; {stmt} __o }}")
        }
    }

    /// A full arithmetic TEXT expression → i64 expr (numbers, idents,
    /// $idents, parens, unary, binary, ternary).
    fn arith_text(&mut self, t: &str) -> Option<String> {
        let toks = arith_tokens(t)?;
        let mut p = ArithParser { render: self, toks: &toks, pos: 0 };
        let e = p.parse_ternary()?;
        if p.pos == toks.len() { Some(e) } else { None }
    }

    // ── command-text reconstruction (shell-outs) ─────────────────────

    /// Build the bash command text for an exec call's words (a String
    /// expression; runtime values are shell-quoted).
    fn cmd_text(&mut self, words: &[&IrExpr], env: Option<&[(String, IrExpr)]>) -> String {
        let ws: Vec<String> = words.iter().map(|w| self.words_expr(w)).collect();
        self.add_helper("cat");
        self.add_helper("wq");
        let body = format!("__sh_wq(&__sh_cat(&[{}]))", ws.join(", "));
        if let Some(env) = env {
            let mut parts = Vec::new();
            for (k, v) in env {
                let ve = self.expr_str(v);
                self.add_helper("q");
                parts.push(format!("format!(\"{}={{}}\", __sh_q(&{ve}))", Self::rust_str(k)));
            }
            parts.push(body);
            format!("format!(\"{{}} {{}}\", {}, {})", parts.join(", "), "{}")
        } else {
            body
        }
    }

    // ── word expansion (Vec<String>-typed expressions) ───────────────

    /// Render a WORD as a Vec<String> runtime expression. Unquoted globs
    /// (SH2GLOB), split/brace/listVar/capture words expand to multiple
    /// elements; everything else is a single element.
    fn words_expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => {
                if let Some(pat) = s.strip_prefix(GLOB_SENTINEL) {
                    self.add_helper("glob");
                    format!("__sh_glob({})", Self::rust_str(pat))
                } else {
                    format!("vec![{}]", Self::rust_str_expr(s))
                }
            }
            IrExpr::Int(i) => format!("vec![({i}).to_string()]"),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.declared(name) || self.captured.contains_key(name) {
                    if self.is_num(name) {
                        format!("vec![{}.to_string()]", self.read_num(name))
                    } else if self.is_array(name) {
                        self.read_arr(name)
                    } else {
                        format!("vec![{}]", self.read_str(name))
                    }
                } else {
                    format!("vec![{}]", self.getvar_str(name))
                }
            }
            IrExpr::Interpolate(parts) => format!("vec![{}]", self.interpolate(parts)),
            IrExpr::Arith(a) => format!("vec![({}).to_string()]", self.arith(a)),
            IrExpr::Bool(b) => {
                let _ = b;
                "Vec::new()".to_string()
            }
            IrExpr::Call { func, args } => self.call_words(func, args),
            IrExpr::Capture { .. } => {
                let c = self.capture_expr_single(e);
                self.add_helper("split_ifs");
                format!("__sh_split_ifs(&{c}, \" \\t\\n\")")
            }
            IrExpr::Array(items) => {
                // a word LIST (`echo {1..5}` — the brace call wrapped in
                // an Array) — expand each element
                let ws: Vec<String> = items.iter().map(|i| self.words_expr(i)).collect();
                self.add_helper("cat");
                format!("__sh_cat(&[{}])", ws.join(", "))
            }
            IrExpr::BinOp { .. } => format!("vec![{}]", self.expr_str(e)),
            IrExpr::Index { var, key } => {
                let k = self.expr_num(key);
                format!("vec![{}]", self.array_elem(var, &k))
            }
            other => {
                self.mark_todo(&format!("word {:?}", other));
                "vec![String::new()]".to_string()
            }
        }
    }

    /// Call funcs in WORD context (multi-word capable).
    fn call_words(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    format!("vec![{}]", self.getvar_str(name))
                } else {
                    "vec![String::new()]".to_string()
                }
            }
            "param" => self.param_words(args),
            "listVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.listvar_words(name)
                } else {
                    "Vec::new()".to_string()
                }
            }
            "split" => {
                let s = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
                self.add_helper("split_ifs");
                format!("__sh_split_ifs(&{s}, \" \\t\\n\")")
            }
            "brace" => self.brace_words(args),
            "capture" => {
                let c = self.capture_expr(args);
                self.add_helper("split_ifs");
                format!("__sh_split_ifs(&{c}, \" \\t\\n\")")
            }
            "join" => format!("vec![{}]", self.join_str(args)),
            "arrayIndex" => format!("vec![{}]", self.array_index_str(args)),
            "arrayLen" => format!("vec![{}]", self.expr_str(&IrExpr::Call {
                func: func.to_string(),
                args: args.to_vec(),
            })),
            "arrayItems" => args
                .first()
                .map(|a| format!("vec![{}]", self.expr_str(a)))
                .unwrap_or_else(|| "Vec::new()".to_string()),
            "arith" => {
                let text = str_arg(args, 0).unwrap_or("").replace(GLOB_SENTINEL, "");
                if let Some(e) = self.arith_text(&text) {
                    format!("vec![({e}).to_string()]")
                } else {
                    "vec![\"0\".to_string()]".to_string()
                }
            }
            "captureWords" => self.capture_words_expr(args),
            "assign" => format!("vec![{}]", self.assign_call_str(args)),
            "test" => "Vec::new()".to_string(),
            "exec" => "Vec::new()".to_string(),
            "grepMatches" => "Vec::new()".to_string(),
            "contains" => "Vec::new()".to_string(),
            _ => {
                self.mark_todo(&format!("word call {func}"));
                "vec![String::new()]".to_string()
            }
        }
    }

    /// `${arr[@]}` / `${!map[@]}` / `${#arr[@]}` in word context.
    fn param_words(&mut self, args: &[IrExpr]) -> String {
        let op = str_arg(args, 0).unwrap_or("");
        let name = str_arg(args, 1).unwrap_or("");
        let idx_at = matches!(args.get(2), Some(IrExpr::Str(s, _)) if s == "@" || s == "*");
        let off_num = matches!(args.get(2), Some(IrExpr::Str(s, _)) if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
        let name_at = name.ends_with("[@]") || name.ends_with("[*]");
        if let Some(keys) = name.strip_prefix('!') {
            let keys = array_base_name(keys);
            if !keys.is_empty() {
                self.mark_written(&keys);
                if self.is_assoc(&keys) {
                    return self.assoc_keys(&keys);
                }
            }
        }
        if let Some(len_name) = name.strip_prefix('#') {
            let len_name = len_name
                .strip_suffix("[@]")
                .or_else(|| len_name.strip_suffix("[*]"))
                .unwrap_or(len_name);
            if !len_name.is_empty() && (idx_at || name_at) {
                self.mark_written(len_name);
                let l = if self.is_assoc(len_name) {
                    let m = self.tls(len_name);
                    format!("{m}.with(|v| v.borrow().len() as i64)")
                } else {
                    self.array_len(len_name)
                };
                return format!("vec![({l}).to_string()]");
            }
        }
        let _ = op;
        if name == "@" || name == "*" {
            if op == "slice" {
                let off = args.get(2).map(|x| self.expr_num(x)).unwrap_or_else(|| "0".to_string());
                let len = match args.get(3) {
                    None => "-1".to_string(),
                    Some(IrExpr::Str(s, _)) if s.is_empty() => "-1".to_string(),
                    Some(x) => self.expr_num(x),
                };
                return format!(
                    "{{ let __v = __SH_ARGV.lock().unwrap().clone(); let __o = ({off} - 1).max(0) as usize; \
                     let __l = if {len} < 0 {{ __v.len().saturating_sub(__o) as i64 }} else {{ {len} }}; \
                     let __e = ((__o as i64 + __l).max(__o as i64)).min(__v.len() as i64) as usize; \
                     __v[__o..__e].to_vec() }}"
                );
            }
            return "__SH_ARGV.lock().unwrap().clone()".to_string();
        }
        if (name_at || idx_at || off_num) && !name.is_empty() {
            let var = name
                .strip_suffix("[@]")
                .or_else(|| name.strip_suffix("[*]"))
                .unwrap_or(name);
            if self.is_array(var) || self.is_assoc(var) {
                self.mark_written(var);
                if op == "slice" {
                    let off = args.get(2).map(|x| self.expr_num(x)).unwrap_or_else(|| "0".to_string());
                    let len = match args.get(3) {
                        None => "-1".to_string(),
                        Some(IrExpr::Str(s, _)) if s.is_empty() => "-1".to_string(),
                        Some(x) => self.expr_num(x),
                    };
                    let arr = self.read_arr(var);
                    return format!(
                        "{{ let __v = {arr}; let __o = if {off} < 0 {{ (__v.len() as i64 + {off}).max(0) }} else {{ {off} }} as usize; let __l = if {len} < 0 {{ __v.len() as i64 - __o as i64 }} else {{ {len} }};                          let __s = __o.max(0) as usize; let __e = ((__s as i64 + __l).max(__s as i64)).min(__v.len() as i64) as usize;                          __v[__s..__e].to_vec() }}"
                    );
                }
                return self.read_arr(var);
            }
        }
        if name_at || idx_at {
            let var = name
                .strip_suffix("[@]")
                .or_else(|| name.strip_suffix("[*]"))
                .unwrap_or(name);
            if self.is_assoc(var) {
                self.mark_written(var);
                return self.assoc_keys(var);
            }
            if self.is_array(var) {
                self.mark_written(var);
                return self.read_arr(var);
            }
            if var == "@" || var == "*" {
                return "__SH_ARGV.lock().unwrap().clone()".to_string();
            }
        }
        format!("vec![{}]", self.param_str(args))
    }

    /// listVar: the positional args or an array's elements.
    fn listvar_words(&mut self, name: &str) -> String {
        match name {
            "@" | "*" => "__SH_ARGV.lock().unwrap().clone()".to_string(),
            _ => {
                self.mark_written(name);
                if self.is_assoc(name) {
                    self.assoc_keys(name)
                } else {
                    self.read_arr(name)
                }
            }
        }
    }

    fn listvar_joined(&mut self, args: &[IrExpr]) -> String {
        if let Some(IrExpr::Str(name, _)) = args.first() {
            match name.as_str() {
                "@" | "*" => "__SH_ARGV.lock().unwrap().join(\" \")".to_string(),
                _ => {
                    self.mark_written(name);
                    if self.is_assoc(name) {
                        self.assoc_keys(name).replace("collect::<Vec<String>>()", "join(\" \")")
                    } else {
                        format!("{}.join(\" \")", self.read_arr(name))
                    }
                }
            }
        } else {
            "String::new()".to_string()
        }
    }

    /// join(x): `${arr[*]}` quoted — the elements joined with a space
    /// (identity for scalar strings).
    fn join_str(&mut self, args: &[IrExpr]) -> String {
        if let Some(IrExpr::Str(name, _)) = args.first() {
            if self.is_array(name) || self.is_assoc(name) {
                self.mark_written(name);
                if self.is_assoc(name) {
                    return self.assoc_keys(name).replace("collect::<Vec<String>>()", "join(\" \")");
                }
                return format!("{}.join(\" \")", self.read_arr(name));
            }
        }
        args.first().map(|a| self.expr_str(a)).unwrap_or_else(|| "String::new()".to_string())
    }

    /// arrayIndex(var, key) — element read.
    fn array_index_str(&mut self, args: &[IrExpr]) -> String {
        let Some(name) = str_arg(args, 0) else {
            return "String::new()".to_string();
        };
        self.mark_written(name);
        let key = match args.get(1) {
            Some(IrExpr::Str(k, _)) => self.key_num_expr(k),
            Some(k) => self.expr_num(k),
            None => "0".to_string(),
        };
        if self.is_assoc(name) {
            // assoc read with a numeric-looking key — use the literal text
            if let Some(IrExpr::Str(k, _)) = args.get(1) {
                return self.assoc_get(name, &Self::rust_str_expr(k));
            }
            self.assoc_get(name, &key)
        } else {
            self.array_elem(name, &key)
        }
    }

    /// A param default/value text — strip the source quoting
    /// (`${@:-"default"}` carries the quotes in the Str).
    fn param_val_str(&mut self, x: &IrExpr) -> String {
        if let IrExpr::Str(s, _) = x {
            let t = s.trim();
            if t.len() >= 2
                && ((t.starts_with('"') && t.ends_with('"'))
                    || (t.starts_with('\'') && t.ends_with('\'')))
            {
                let inner = &t[1..t.len() - 1];
                if inner.contains('$') {
                    return self.dollar_interp(inner);
                }
                return Self::rust_str_expr(inner);
            }
            if t.contains('$') {
                // `${NAME:-${OTHER:-PC}}` — a nested expansion default
                return self.dollar_interp(t);
            }
        }
        self.expr_str(x)
    }

    /// `${arr[i]}` / `${info[$key]}` element read by name/key text.
    fn array_index_name(&mut self, name: &str) -> String {
        if let Some(open) = name.find('[') {
            if name.ends_with(']') {
                let var = &name[..open];
                let key = &name[open + 1..name.len() - 1];
                self.mark_written(var);
                if self.is_assoc(var) {
                    let k = self.assoc_key_expr(key);
                    return self.assoc_get(var, &k);
                }
                let k = self.key_num_expr(key);
                return self.array_elem(var, &k);
            }
        }
        "String::new()".to_string()
    }

    /// A numeric key expr from source text (may be `$var` / `${x}` /
    /// an arithmetic expression like `(2*$i)-1`).
    fn key_num_expr(&mut self, key: &str) -> String {
        let k = key.trim();
        if let Ok(n) = k.parse::<i64>() {
            return n.to_string();
        }
        if k.contains('$') || k.contains('*') || k.contains('+') || k.contains('-')
            || k.contains('/') || k.contains('%') || k.starts_with('(')
        {
            // an arithmetic index expression
            if let Some(e) = self.arith_text(k) {
                return e;
            }
        }
        let k = k.strip_prefix('$').unwrap_or(k);
        let k = k.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(k);
        if self.declared(k) || self.captured.contains_key(k) {
            self.getvar_num(k)
        } else if !k.is_empty() && k.chars().all(|c| c.is_ascii_digit()) {
            format!("{}", k.parse::<i64>().unwrap_or(0))
        } else {
            "0".to_string()
        }
    }

    /// An assoc key expr from source text (`info[$key]` → runtime value).
    fn assoc_key_expr(&mut self, key: &str) -> String {
        let k = key.trim();
        if k.starts_with('"') && k.ends_with('"') && k.len() >= 2 {
            return Self::rust_str_expr(&k[1..k.len() - 1]);
        }
        if k.starts_with('$') || k.starts_with('{') || k.contains('$') {
            // interpolate the key text at runtime
            let inner = k.strip_prefix('$').unwrap_or(k);
            let inner = inner.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(inner);
            if self.declared(inner) || self.captured.contains_key(inner) {
                return self.getvar_str(inner);
            }
            Self::rust_str_expr(k)
        } else {
            Self::rust_str_expr(k)
        }
    }

    // ── brace expansion ──────────────────────────────────────────────

    /// brace(pre, groups-json, seps-json, suf) → Vec<String> words.
    fn brace_words(&mut self, args: &[IrExpr]) -> String {
        let pre = args
            .first()
            .and_then(|a| str_arg(&[(*a).clone()], 0).map(|s| s.to_string()))
            .unwrap_or_default();
        let groups: Vec<Vec<String>> = match args.get(1) {
            Some(IrExpr::Json(serde_json::Value::Array(items))) => items
                .iter()
                .map(|g| self.brace_group(g))
                .collect(),
            _ => vec![],
        };
        let seps: Vec<String> = match args.get(2) {
            Some(IrExpr::Json(serde_json::Value::Array(items))) => items
                .iter()
                .filter_map(|s| s.as_str().map(|x| x.to_string()))
                .collect(),
            _ => vec![],
        };
        let suf = args
            .get(3)
            .and_then(|a| str_arg(&[(*a).clone()], 0).map(|s| s.to_string()))
            .unwrap_or_default();
        self.add_helper("brace");
        let glob = pre.contains(GLOB_SENTINEL);
        let pre = pre.replace(GLOB_SENTINEL, "");
        let groups_lit = format!(
            "&[{}]",
            groups
                .iter()
                .map(|g| format!("&[{}]", g.iter().map(|x| Self::rust_str(x)).collect::<Vec<_>>().join(", ")))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let seps_lit = format!(
            "&[{}]",
            seps.iter().map(|x| Self::rust_str(x)).collect::<Vec<_>>().join(", ")
        );
        if glob {
            self.add_helper("glob");
            format!(
                "{{ let __b = __sh_brace({}, {groups_lit}, {seps_lit}, {}).iter().map(|p| __sh_cat(&[__sh_glob(p)])).collect::<Vec<_>>(); __sh_cat(&__b) }}",
                Self::rust_str(&pre),
                Self::rust_str(&suf)
            )
        } else {
            format!("__sh_brace({}, {groups_lit}, {seps_lit}, {})", Self::rust_str(&pre), Self::rust_str(&suf))
        }
    }

    /// Flatten one brace group: a list of ALTERNATIVES (strings and
    /// ranges — `{a,b,c}` / `{1..5}`). Nested braces are not expressible
    /// statically and are skipped (mirrors the C backend). The cartesian
    /// product happens ACROSS groups, at runtime.
    fn brace_group(&mut self, g: &serde_json::Value) -> Vec<String> {
        let mut items: Vec<String> = Vec::new();
        if let Some(es) = g.as_array() {
            for e in es {
                if let Some(s) = e.as_str() {
                    items.push(s.to_string());
                } else if let Some(r) = e.get("range").and_then(|r| r.as_array()) {
                    let start = r.first().and_then(|x| x.as_str()).unwrap_or("");
                    let end = r.get(1).and_then(|x| x.as_str()).unwrap_or("");
                    let step: i64 = r.get(2).and_then(|x| x.as_str()).and_then(|s| s.parse().ok()).unwrap_or(1);
                    if let (Ok(a), Ok(b)) = (start.parse::<i64>(), end.parse::<i64>()) {
                        // zero-padding follows bash: only when the FIRST
                        // operand has a leading zero; width = its width
                        let pad: Option<usize> = if start.len() > 1 && start.starts_with('0') {
                            Some(start.len())
                        } else {
                            None
                        };
                        let padv = |n: i64| -> String {
                            let s = n.to_string();
                            match pad {
                                Some(w) if s.len() < w => format!("{}{}", "0".repeat(w - s.len()), s),
                                _ => s,
                            }
                        };
                        if step > 0 {
                            let mut v = a;
                            while v <= b {
                                items.push(padv(v));
                                v += step;
                            }
                        } else {
                            let mut v = a;
                            while v >= b {
                                items.push(padv(v));
                                v += step;
                            }
                        }
                    } else if start.chars().count() == 1 && end.chars().count() == 1 {
                        let ca = start.chars().next().unwrap() as u32;
                        let cb = end.chars().next().unwrap() as u32;
                        if step > 0 {
                            let mut c = ca;
                            while c <= cb {
                                items.push(char::from_u32(c).unwrap_or('?').to_string());
                                c = (c as i64 + step).max(0) as u32;
                            }
                        } else {
                            let mut c = ca as i64;
                            let end = cb as i64;
                            while c >= end {
                                items.push(char::from_u32(c as u32).unwrap_or('?').to_string());
                                c += step;
                            }
                        }
                    } else {
                        items.push(start.to_string());
                    }
                }
                // nested braces / objects: skipped (mirrors the C backend)
            }
        }
        items
    }

    // ── capture `$(...)` ─────────────────────────────────────────────

    /// Capture expr: single-exec arrows shell out directly; anything
    /// else renders as a block with the runtime output buffer.
    fn capture_expr(&mut self, args: &[IrExpr]) -> String {
        let mut found = false;
        for a in args {
            if let IrExpr::Arrow(stmts) = a {
                found = true;
                // fast path: [Expr(Call exec cmd …)] → bash -c capture
                if let Some(text) = self.stage_text(stmts) {
                    self.add_helper("capture_rc");
                    return format!(
                        "{{ let (__c, __r) = __sh_capture_rc(&{text}); __SH_RC.store(__r, Ordering::SeqCst); __c }}"
                    );
                }
                return self.capture_block(stmts);
            }
        }
        if !found {
            self.mark_todo("empty capture");
        }
        "String::new()".to_string()
    }

    /// Same, for a bare `IrExpr::Capture` node.
    fn capture_expr_single(&mut self, e: &IrExpr) -> String {
        if let IrExpr::Capture { expr, .. } = e {
            if let IrExpr::Arrow(stmts) = expr.as_ref() {
                if let Some(text) = self.stage_text(stmts) {
                    self.add_helper("capture_rc");
                    return format!(
                        "{{ let (__c, __r) = __sh_capture_rc(&{text}); __SH_RC.store(__r, Ordering::SeqCst); __c }}"
                    );
                }
                return self.capture_block(stmts);
            }
            let inner = self.expr_str(expr);
            let _ = inner;
        }
        "String::new()".to_string()
    }

    /// Native capture: run the statements with the output buffer active.
    fn capture_block(&mut self, stmts: &[IrStmt]) -> String {
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 1;
        self.emit("let __old = __SH_OUT.lock().unwrap().take();");
        self.emit("*__SH_OUT.lock().unwrap() = Some(Vec::new());");
        for s in stmts {
            self.stmt(s);
        }
        self.emit("let __cap = __SH_OUT.lock().unwrap().take().unwrap();");
        self.emit("*__SH_OUT.lock().unwrap() = __old;");
        self.emit("let mut __s = String::from_utf8_lossy(&__cap).to_string();");
        self.emit("while __s.ends_with('\\n') { __s.pop(); }");
        self.emit("__s");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// captureWords(Arrow) — the captured output split into words.
    fn capture_words_expr(&mut self, args: &[IrExpr]) -> String {
        let c = self.capture_expr(args);
        self.add_helper("split_ifs");
        format!("__sh_split_ifs(&{c}, \" \\t\\n\")")
    }

    // ── test `[ ... ]` / `[[ ... ]]` ─────────────────────────────────

    /// A test Call in bool context.
    fn test_call_bool(&mut self, args: &[IrExpr]) -> String {
        let style = args
            .get(1)
            .and_then(|a| str_arg(&[(*a).clone()], 0).map(|s| s.to_string()))
            .unwrap_or_else(|| "[".to_string());
        if let Some(s) = str_arg(args, 0).map(|s| s.to_string()) {
            if let Some(c) = self.test_expr(&s, &style) {
                return format!(
                    "{{ let __b = {c}; __SH_RC.store(if __b {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __b }}"
                );
            }
        }
        self.mark_todo("test");
        "true".to_string()
    }

    /// Parse a `[ ... ]` / `[[ ... ]]` expression text into a bool expr.
    fn test_expr(&mut self, s: &str, style: &str) -> Option<String> {
        let toks = test_tokens(s)?;
        let mut p = TestParser { render: self, toks: &toks, pos: 0, style };
        let e = p.parse_or()?;
        if p.pos == toks.len() { Some(e) } else { None }
    }

    // ── pipelines ────────────────────────────────────────────────────

    /// A pipeline (stmt or Call) — mixed native/shell stages threaded
    /// through byte buffers. Statement form.
    fn pipeline_stmt(&mut self, stages: &[Vec<IrStmt>]) {
        let n = stages.len();
        if n == 0 {
            return;
        }
        self.add_helper("spawn");
        self.add_helper("cap_bytes");
        let buf = self.gensym("__sh_pbuf");
        self.emit(&format!("let mut {buf}: Vec<u8> = Vec::new();"));
        for (idx, st) in stages.iter().enumerate() {
            let last = idx + 1 == n;
            if let Some(text) = self.stage_text(st) {
                // shell stage
                if last {
                    self.emit(&format!("__SH_RC.store(__sh_spawn(&{text}, Some(&{buf})), Ordering::SeqCst);"));
                } else {
                    self.emit(&format!("{buf} = __sh_cap_bytes(&{text}, Some(&{buf}));"));
                }
            } else {
                // native stage
                let mut saved = std::mem::take(&mut self.out);
                let old_depth = self.depth;
                self.depth = 1;
                if idx > 0 {
                    self.emit(&format!(
                        "let __oldin = __SH_STDIN.lock().unwrap().take();"
                    ));
                    self.emit(&format!(
                        "*__SH_STDIN.lock().unwrap() = Some(Box::new(std::io::Cursor::new({buf}.clone())));"
                    ));
                }
                if !last {
                    self.emit("let __old = __SH_OUT.lock().unwrap().take();");
                    self.emit("*__SH_OUT.lock().unwrap() = Some(Vec::new());");
                }
                for s in st {
                    self.stmt(s);
                }
                if !last {
                    self.emit(&format!("{buf} = __SH_OUT.lock().unwrap().take().unwrap();"));
                    self.emit("*__SH_OUT.lock().unwrap() = __old;");
                }
                if idx > 0 {
                    self.emit("*__SH_STDIN.lock().unwrap() = __oldin;");
                }
                let block = self.out.join("\n");
                self.out = saved;
                self.depth = old_depth;
                self.emit(&format!("{{\n{block}\n}}"));
            }
        }
        if n > 1 {
            // bash: the pipeline rc is the LAST stage's — already stored
            // by the final stage
        }
    }

    /// pipeline Call as a bool block (used in conditions).
    fn pipeline_bool(&mut self, args: &[IrExpr]) -> String {
        let stages = pipeline_stages(args);
        if stages.is_empty() {
            return "true".to_string();
        }
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        self.pipeline_stmt(&stages);
        self.emit("__SH_RC.load(Ordering::SeqCst) == 0");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// whileLoop(Arrow([Arrow(cond), Arrow(body)])) as a bool block —
    /// used in pipeline stages (`while read …; done | sort`).
    fn whileloop_bool(&mut self, args: &[IrExpr]) -> String {
        let mut cond: Vec<IrStmt> = Vec::new();
        let mut body: Vec<IrStmt> = Vec::new();
        if let Some(IrExpr::Arrow(outer)) = args.first() {
            for a in outer {
                if let IrStmt::Expr(IrExpr::Arrow(stmts)) = a {
                    if cond.is_empty() {
                        cond = stmts.clone();
                    } else {
                        body = stmts.clone();
                    }
                }
            }
        }
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        let cond_block = self.cond_block(&cond);
        self.emit(&format!("while {cond_block} {{"));
        self.loop_depth += 1;
        self.depth += 1;
        for s in &body {
            self.stmt(s);
        }
        self.depth -= 1;
        self.loop_depth -= 1;
        self.emit("}");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// A list of statements as a condition (the last statement's rc).
    fn cond_block(&mut self, stmts: &[IrStmt]) -> String {
        if stmts.len() == 1 {
            if let IrStmt::Expr(e) = &stmts[0] {
                return self.expr_bool(e);
            }
        }
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        for s in stmts {
            self.stmt(s);
        }
        self.emit("__SH_RC.load(Ordering::SeqCst) == 0");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// block(Arrow(stmts)) as bool — run the statements, rc decides.
    fn block_bool(&mut self, args: &[IrExpr]) -> String {
        if let Some(IrExpr::Arrow(stmts)) = args.first() {
            self.cond_block(stmts)
        } else {
            "true".to_string()
        }
    }

    /// `and(Arrow(stmts), Arrow(stmts), …)` — the lifted `a && b` form:
    /// run each block while the previous succeeded.
    fn and_blocks(&mut self, args: &[IrExpr]) -> Vec<Vec<IrStmt>> {
        let mut blocks: Vec<Vec<IrStmt>> = Vec::new();
        for a in args {
            if let IrExpr::Arrow(b) = a {
                blocks.push(b.clone());
            }
        }
        blocks
    }

    /// grepMatches as a statement — the `grep -o` lift: print each
    /// match on its own line, rc = 0 iff any match.
    fn grepmatches_stmt(&mut self, args: &[IrExpr]) {
        self.add_helper("grepmatches");
        self.add_helper("print_words");
        let text = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
        let pat = args.get(1).map(|a| self.expr_str(a)).unwrap_or_default();
        let flags = args.get(2).map(|a| self.expr_str(a)).unwrap_or_default();
        self.emit(&format!("let (__m, __r) = __sh_grepmatches(&{text}, &{pat}, &{flags});"));
        self.emit("if !__m.is_empty() { __sh_print_words(&[vec![__m]], true, false); }");
        self.emit("__SH_RC.store(__r, Ordering::SeqCst);");
    }

    /// and as a statement.
    fn and_stmt(&mut self, args: &[IrExpr]) {
        let blocks = self.and_blocks(args);
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                self.emit("if __SH_RC.load(Ordering::SeqCst) == 0 {");
                self.depth += 1;
            }
            for s in b {
                self.stmt(s);
            }
            if i > 0 {
                self.depth -= 1;
                self.emit("}");
            }
        }
    }

    /// and as a bool block.
    fn and_bool(&mut self, args: &[IrExpr]) -> String {
        let blocks = self.and_blocks(args);
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                self.emit("if __SH_RC.load(Ordering::SeqCst) == 0 {");
                self.depth += 1;
            }
            for s in b {
                self.stmt(s);
            }
            if i > 0 {
                self.depth -= 1;
                self.emit("}");
            }
        }
        self.emit("__SH_RC.load(Ordering::SeqCst) == 0");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// contains(text, needle) as bool.
    fn contains_bool(&mut self, args: &[IrExpr]) -> String {
        let text = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
        let needle = args.get(1).map(|a| self.expr_str(a)).unwrap_or_default();
        format!("({text}.contains(&{needle}))")
    }

    /// redirect(Arrow(stmts), [fd/mode/target…]) as bool.
    fn redirect_bool(&mut self, args: &[IrExpr]) -> String {
        let mut stmts: Vec<IrStmt> = Vec::new();
        let mut redirs: Vec<IrRedirectInfo> = Vec::new();
        if let Some(IrExpr::Arrow(b)) = args.first() {
            stmts = b.clone();
        }
        if let Some(IrExpr::Array(items)) = args.get(1) {
            for it in items {
                if let Some(r) = self.redirect_info(it) {
                    redirs.push(r);
                }
            }
        }
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        self.redirect_render(&stmts, &redirs);
        self.emit("__SH_RC.load(Ordering::SeqCst) == 0");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// subshell(Arrow(stmts)) as bool — save/restore assigned vars.
    fn subshell_bool(&mut self, args: &[IrExpr]) -> String {
        let mut stmts: Vec<IrStmt> = Vec::new();
        if let Some(IrExpr::Arrow(b)) = args.first() {
            stmts = b.clone();
        }
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        self.subshell_render(&stmts);
        self.emit("__SH_RC.load(Ordering::SeqCst) == 0");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    // ── redirects ────────────────────────────────────────────────────

    /// Parse one redirect Object arg.
    fn redirect_info(&mut self, it: &IrExpr) -> Option<IrRedirectInfo> {
        let IrExpr::Object(props) = it else {
            return None;
        };
        let mut fd: Option<i64> = None;
        let mut mode = String::new();
        let mut target: Option<IrExpr> = None;
        for (k, v) in props {
            match k.as_str() {
                "fd" => {
                    if let IrExpr::Int(n) = v {
                        fd = Some(*n);
                    }
                }
                "mode" => {
                    if let IrExpr::Str(s, _) = v {
                        mode = s.clone();
                    }
                }
                "target" => target = Some(v.clone()),
                _ => {}
            }
        }
        Some(IrRedirectInfo { fd: fd.unwrap_or(1), mode, target })
    }

    /// Render a redirect statement (IrStmt::Redirect or the Call form).
    fn redirect_render(&mut self, inner: &[IrStmt], redirs: &[IrRedirectInfo]) {
        // split the redirects by effect
        let mut stdin_redir: Option<&IrRedirectInfo> = None; // fd 0
        let mut out_redirs: Vec<&IrRedirectInfo> = Vec::new(); // fd 1/2 file
        for r in redirs {
            match r.fd {
                0 => stdin_redir = Some(r),
                1 | 2 => out_redirs.push(r),
                _ => {}
            }
        }
        // shell-out the whole thing when the inner is text-reconstructable;
        // anything else (pipelines, while loops, `and` blocks) renders
        // natively — its shell-outs already respect __SH_OUTFILE/__SH_STDIN
        if let Some(text) = self.stage_text(inner) {
            let mut full = text;
                let mut input: Option<String> = None;
                for r in redirs {
                    match r.mode.as_str() {
                        "w" | "a" => {
                            if let Some(t) = &r.target {
                                let te = self.expr_str(t);
                                // `2>&1` — a dup target
                                if let IrExpr::Str(ts, _) = t {
                                    if let Some(rest) = ts.strip_prefix('&') {
                                        full = format!(
                                            "format!(\"{{}} {}&{{}}\", {full}, {})",
                                            if r.fd == 2 { "2>" } else { ">" },
                                            Self::rust_str(rest)
                                        );
                                        continue;
                                    }
                                }
                                let op = if r.mode == "w" { ">" } else { ">>" };
                                let fd = if r.fd == 2 { "2" } else { "" };
                                full = format!("format!(\"{{}} {fd}{op} {{}}\", {full}, __sh_q(&{te}))");
                                self.add_helper("q");
                            }
                        }
                        "r" => {
                            if let Some(t) = &r.target {
                                let te = self.expr_str(t);
                                let fd = if r.fd == 2 { "2" } else { "" };
                                full = format!("format!(\"{{}} {fd}< {{}}\", {full}, __sh_q(&{te}))");
                                self.add_helper("q");
                            }
                        }
                        "process-in" => {
                            if let Some(t) = &r.target {
                                let te = self.expr_str(t);
                                full = format!("format!(\"{{}} < <({{}})\", {full}, {te})");
                            }
                        }
                        "heredoc" | "heredoc-tabs" | "herestring" => {
                            if let Some(t) = &r.target {
                                let mut te = self.expr_str(t);
                                if r.mode == "herestring" {
                                    te = format!("format!(\"{{}}\\n\", {te})");
                                }
                                input = Some(te);
                            }
                        }
                        _ => {
                            // dup targets ("2>&1") and unknown modes
                            if let Some(t) = &r.target {
                                if let IrExpr::Str(ts, _) = t {
                                    if let Some(rest) = ts.strip_prefix('&') {
                                        full = format!(
                                            "format!(\"{{}} {}&{{}}\", {full}, {})",
                                            if r.fd == 2 { "2>" } else { ">" },
                                            Self::rust_str(rest)
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                self.add_helper("spawn");
                if let Some(inp) = input {
                    self.emit(&format!(
                        "__SH_RC.store(__sh_spawn(&{full}, Some({inp}.as_bytes())), Ordering::SeqCst);"
                    ));
                } else {
                    self.emit(&format!("__SH_RC.store(__sh_spawn(&{full}, None), Ordering::SeqCst);"));
                }
            return;
        }
        // native inner: fd0 via __SH_STDIN, fd1 via __SH_OUTFILE
        let mut pre = Vec::new();
        let mut post = Vec::new();
        if let Some(r) = stdin_redir {
            match r.mode.as_str() {
                "r" => {
                    if let Some(t) = &r.target {
                        let te = self.expr_str(t);
                        pre.push("let __oldin = __SH_STDIN.lock().unwrap().take();".to_string());
                        pre.push(format!(
                            "*__SH_STDIN.lock().unwrap() = Some(Box::new(std::fs::File::open(&{te}).unwrap_or_else(|_| {{ let f = std::fs::File::open(\"/dev/null\").unwrap(); f }})));"
                        ));
                        post.push("*__SH_STDIN.lock().unwrap() = __oldin;".to_string());
                    }
                }
                "heredoc" | "heredoc-tabs" | "herestring" => {
                    if let Some(t) = &r.target {
                        let mut te = self.expr_str(t);
                        if r.mode == "herestring" {
                            te = format!("format!(\"{{}}\\n\", {te})");
                        }
                        pre.push("let __oldin = __SH_STDIN.lock().unwrap().take();".to_string());
                        pre.push(format!(
                            "*__SH_STDIN.lock().unwrap() = Some(Box::new(std::io::Cursor::new({te}.into_bytes())));"
                        ));
                        post.push("*__SH_STDIN.lock().unwrap() = __oldin;".to_string());
                    }
                }
                "process-in" => {
                    if let Some(t) = &r.target {
                        let te = self.expr_str(t);
                        self.add_helper("cap_bytes");
                        pre.push("let __oldin = __SH_STDIN.lock().unwrap().take();".to_string());
                        pre.push(format!(
                            "*__SH_STDIN.lock().unwrap() = Some(Box::new(std::io::Cursor::new(__sh_cap_bytes(&{te}, None))));"
                        ));
                        post.push("*__SH_STDIN.lock().unwrap() = __oldin;".to_string());
                    }
                }
                _ => {}
            }
        }
        // fd 1/2 file redirects for native output
        let file_redirs: Vec<&IrRedirectInfo> = out_redirs
            .iter()
            .filter(|r| matches!(r.mode.as_str(), "w" | "a") && r.target.is_some())
            .copied()
            .collect();
        let mut used_outfile = false;
        if let Some(r) = file_redirs.first() {
            let t = r.target.as_ref().unwrap();
            let te = self.expr_str(t);
            used_outfile = true;
            pre.push("let __oldout = __SH_OUTFILE.lock().unwrap().take();".to_string());
            if r.mode == "w" {
                pre.push(format!(
                    "*__SH_OUTFILE.lock().unwrap() = Some(std::fs::File::create(&{te}).unwrap_or_else(|_| std::fs::File::open(\"/dev/null\").unwrap()));"
                ));
            } else {
                pre.push(format!(
                    "*__SH_OUTFILE.lock().unwrap() = Some(std::fs::OpenOptions::new().append(true).create(true).open(&{te}).unwrap_or_else(|_| std::fs::File::open(\"/dev/null\").unwrap()));"
                ));
            }
            post.push("*__SH_OUTFILE.lock().unwrap() = __oldout;".to_string());
        }
        let _ = used_outfile;
        for p in pre {
            self.emit(&p);
        }
        for s in inner {
            self.stmt(s);
        }
        for p in post {
            self.emit(&p);
        }
    }

    /// The IrStmt::Redirect statement.
    fn redirect_stmt(&mut self, s: &IrStmt) {
        let IrStmt::Redirect { inner, redirects } = s else { return };
        let infos: Vec<IrRedirectInfo> = redirects.iter().filter_map(|r| {
            Some(IrRedirectInfo {
                fd: r.fd.unwrap_or(1) as i64,
                mode: r.mode.clone(),
                target: Some(r.target.clone()),
            })
        }).collect();
        self.redirect_render(inner, &infos);
    }

    // ── functions ────────────────────────────────────────────────────

    fn fn_call_stmt(&mut self, name: &str, words: &[&IrExpr]) -> String {
        let ws: Vec<String> = words.iter().map(|w| self.words_expr(w)).collect();
        self.add_helper("cat");
        format!(
            "{{ let __old = __SH_ARGV.lock().unwrap().clone(); let __new = __sh_cat(&[{}]); *__SH_ARGV.lock().unwrap() = __new; {}(); *__SH_ARGV.lock().unwrap() = __old; }}",
            ws.join(", "),
            self.fn_ident(name)
        )
    }

    fn fn_call_bool(&mut self, name: &str, words: &[&IrExpr]) -> String {
        let ws: Vec<String> = words.iter().map(|w| self.words_expr(w)).collect();
        self.add_helper("cat");
        format!(
            "{{ let __old = __SH_ARGV.lock().unwrap().clone(); let __new = __sh_cat(&[{}]); *__SH_ARGV.lock().unwrap() = __new; {}(); *__SH_ARGV.lock().unwrap() = __old; __SH_RC.load(Ordering::SeqCst) == 0 }}",
            ws.join(", "),
            self.fn_ident(name)
        )
    }

    // ── arrays (setArray & co.) ──────────────────────────────────────

    fn array_call_stmt(&mut self, args: &[IrExpr]) {
        let Some(name) = str_arg(args, 0) else {
            return;
        };
        self.mark_written(name);
        if self.is_assoc(name) {
            self.mark_todo("assoc setArray");
            return;
        }
        self.arrays.insert(name.to_string());
        let items: Vec<String> = match args.get(1) {
            Some(IrExpr::Array(items)) => items.iter().map(|i| self.words_expr(i)).collect(),
            _ => vec![],
        };
        self.add_helper("cat");
        let stmt = self.write_arr(name, &format!("__sh_cat(&[{}])", items.join(", ")));
        self.emit(&stmt);
    }

    fn array_append_stmt(&mut self, args: &[IrExpr]) {
        let Some(name) = str_arg(args, 0) else {
            return;
        };
        self.mark_written(name);
        self.arrays.insert(name.to_string());
        let items: Vec<String> = match args.get(1) {
            Some(IrExpr::Array(items)) => items.iter().map(|i| self.words_expr(i)).collect(),
            Some(other) => vec![self.words_expr(other)],
            None => vec![],
        };
        self.add_helper("cat");
        let m = self.tls(name);
        self.emit(&format!(
            "{m}.with(|v| {{ let mut b = v.borrow_mut(); for __w in __sh_cat(&[{}]) {{ b.push(__w); }} }});",
            items.join(", ")
        ));
    }

    // ── echo / printf ────────────────────────────────────────────────

    /// Split echo words into parts + flags.
    fn echo_parts(&mut self, words: &[&IrExpr]) -> (Vec<Part>, bool, bool) {
        let mut items: Vec<&IrExpr> = words.to_vec();
        let mut nl = true;
        let mut esc = false;
        loop {
            match items.first() {
                Some(IrExpr::Str(f, _)) if *f == "-n" => {
                    nl = false;
                    items.remove(0);
                }
                Some(IrExpr::Str(f, _)) if *f == "-e" => {
                    esc = true;
                    items.remove(0);
                }
                Some(IrExpr::Str(f, _)) if *f == "-E" => {
                    esc = false;
                    items.remove(0);
                }
                Some(IrExpr::Str(f, _)) if *f == "--" => {
                    items.remove(0);
                    break;
                }
                _ => break,
            }
        }
        let mut parts = Vec::new();
        for w in items {
            parts.push(Part::Words(self.words_expr(w)));
        }
        (parts, nl, esc)
    }

    fn echo_stmt(&mut self, words: &[&IrExpr]) {
        let (parts, nl, esc) = self.echo_parts(words);
        let ws: Vec<String> = parts
            .into_iter()
            .map(|p| match p {
                Part::Lit(t) => format!("vec![{}]", Self::rust_str_expr(&t)),
                Part::Words(w) => w,
            })
            .collect();
        self.add_helper("print_words");
        self.emit(&format!("__sh_print_words(&[{}], {}, {});", ws.join(", "), nl, esc));
        self.emit("__SH_RC.store(0, Ordering::SeqCst);");
    }

    fn printf_stmt(&mut self, words: &[&IrExpr]) {
        // `printf [-v var] format [args…]`
        let mut idx = 0;
        let mut target: Option<String> = None;
        if let Some(IrExpr::Str(f, _)) = words.first().copied() {
            if f == "-v" {
                if let Some(n) = words.get(1).and_then(|w| {
                    str_arg(&[(*w).clone()], 0).map(|s| s.to_string())
                }) {
                    target = Some(n);
                    idx = 2;
                }
            }
        }
        if words.len() <= idx {
            return;
        }
        let fmt = self.expr_str(words[idx]);
        let arg_exprs: Vec<String> =
            words.iter().skip(idx + 1).map(|w| self.expr_str(w)).collect();
        self.add_helper("printf");
        let call = format!("__sh_printf(&{fmt}, &[{}])", arg_exprs.join(", "));
        if let Some(t) = target {
            self.mark_written(&t);
            if self.is_num(&t) {
                let st = self.write_num(&t, &format!("{call}.trim().parse::<i64>().unwrap_or(0)"));
                self.emit(&st);
            } else {
                let st = self.write_str(&t, &call);
                self.emit(&st);
            }
        } else {
            self.add_helper("print_words");
            self.emit(&format!("__sh_print_words(&[vec![{call}]], false, false);"));
        }
        self.emit("__SH_RC.store(0, Ordering::SeqCst);");
    }

    // ── param expansion ──────────────────────────────────────────────

    /// param(op, name, [val], [repl]) → String expr.
    fn param_str(&mut self, args: &[IrExpr]) -> String {
        let op = str_arg(args, 0).unwrap_or("");
        let name = str_arg(args, 1).unwrap_or("");
        // array-length / keys forms first
        let idx_at = matches!(args.get(2), Some(IrExpr::Str(s, _)) if s == "@" || s == "*");
        let off_num = matches!(args.get(2), Some(IrExpr::Str(s, _)) if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
        let name_at = name.ends_with("[@]") || name.ends_with("[*]");
        if let Some(keys) = name.strip_prefix('!') {
            let keys = array_base_name(keys);
            if !keys.is_empty() && (idx_at || name_at) {
                self.mark_written(&keys);
                if self.is_assoc(&keys) {
                    return self.assoc_keys(&keys).replace("collect::<Vec<String>>()", "join(\" \")");
                }
                return self.array_elem(&keys, "0");
            }
        }
        if let Some(len_name) = name.strip_prefix('#') {
            let len_name = len_name
                .strip_suffix("[@]")
                .or_else(|| len_name.strip_suffix("[*]"))
                .unwrap_or(len_name);
            if !len_name.is_empty() && (idx_at || name_at) {
                self.mark_written(len_name);
                let l = if self.is_assoc(len_name) {
                    let m = self.tls(len_name);
                    format!("{m}.with(|v| v.borrow().len() as i64)")
                } else {
                    self.array_len(len_name)
                };
                return format!("({l}).to_string()");
            }
        }
        if name.contains('[') && name.ends_with(']') {
            return self.array_index_name(name);
        }
        if name_at || idx_at {
            let var = name
                .strip_suffix("[@]")
                .or_else(|| name.strip_suffix("[*]"))
                .unwrap_or(name);
            if self.is_assoc(var) {
                self.mark_written(var);
                return self.assoc_keys(var).replace("collect::<Vec<String>>()", "join(\" \")");
            }
            if self.is_array(var) {
                self.mark_written(var);
                return format!("{}.join(\" \")", self.read_arr(var));
            }
        }
        // scalar ops
        let var_expr = if name.is_empty() {
            "String::new()".to_string()
        } else if name == "#" {
            "__SH_ARGV.lock().unwrap().len().to_string()".to_string()
        } else if name == "@" || name == "*" {
            "__SH_ARGV.lock().unwrap().join(\" \")".to_string()
        } else if self.declared(name) || self.captured.contains_key(name) {
            self.getvar_str(name)
        } else {
            self.getvar_str(name)
        };
        // `${#x}` — string length (the `#` op WITHOUT a pattern arg)
        if op == "len" || (op == "#" && args.len() < 3) {
            self.add_helper("len");
            return format!("__sh_len(&{var_expr}).to_string()");
        }
        let val = args.get(2).map(|x| self.param_val_str(x)).unwrap_or_else(|| "String::new()".to_string());
        let repl = args.get(3).map(|x| self.param_val_str(x)).unwrap_or_else(|| "String::new()".to_string());
        match op {
            "" => var_expr,
            ":" | "+" | ":+" => {
                // `:+` — the default when non-empty (`+` ≈ `:+` in the
                // store model — empty-vs-unset is not distinguished)
                format!("{{ let __v = {var_expr}; if __v.is_empty() {{ String::new() }} else {{ {val} }} }}")
            }
            "-" | ":-" => {
                format!("{{ let __v = {var_expr}; if __v.is_empty() {{ {val} }} else {{ __v }} }}")
            }
            "=" | ":=" => {
                // assign the default back (declared runtime vars only)
                if !name.is_empty() && !name.starts_with('$')
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && self.declared(name)
                    && !self.is_num(name)
                    && !self.is_array(name)
                {
                    format!(
                        "{{ let __v = {var_expr}; if __v.is_empty() {{ let __d = {val}; {} __d.clone() }} else {{ __v }} }}",
                        self.write_str(name, "__d.clone()")
                    )
                } else {
                    format!("{{ let __v = {var_expr}; if __v.is_empty() {{ {val} }} else {{ __v }} }}")
                }
            }
            ":?" => {
                format!(
                    "{{ let __v = {var_expr}; if __v.is_empty() {{ eprintln!(\"sh2: {{}}: {{}}\", {}, {val}); std::process::exit(1); }} __v }}",
                    Self::rust_str(name)
                )
            }
            "#" | "##" | "#:" | "##:" => {
                let pat = str_arg(args, 2).unwrap_or("");
                let greedy = op.starts_with("##");
                self.add_helper("strippre");
                format!("__sh_strippre(&{var_expr}, {}, {greedy})", Self::rust_str(pat))
            }
            "%" | "%%" | "%:" | "%%:" => {
                let pat = str_arg(args, 2).unwrap_or("");
                let greedy = op.starts_with("%%");
                self.add_helper("stripsuf");
                format!("__sh_stripsuf(&{var_expr}, {}, {greedy})", Self::rust_str(pat))
            }
            "/" | "//" => {
                let pat = str_arg(args, 2).unwrap_or("");
                let all = op == "//";
                self.add_helper("replace");
                format!("__sh_replace(&{var_expr}, {}, &{repl}, {all})", Self::rust_str(pat))
            }
            "slice" => {
                // `${arr[@]:off:len}` — an element slice; scalar strings
                // get a char slice
                let off = args.get(2).map(|x| self.expr_num(x)).unwrap_or_else(|| "0".to_string());
                let len = match args.get(3) {
                    None => "-1".to_string(),
                    Some(IrExpr::Str(s, _)) if s.is_empty() => "-1".to_string(),
                    Some(x) => self.expr_num(x),
                };
                if name == "@" || name == "*" {
                    // `${@:off:len}` — the positional params (1-based off)
                    return format!(
                        "{{ let __v = __SH_ARGV.lock().unwrap().clone(); let __o = ({off} - 1).max(0) as usize; \
                         let __l = if {len} < 0 {{ __v.len().saturating_sub(__o) as i64 }} else {{ {len} }}; \
                         let __e = ((__o as i64 + __l).max(__o as i64)).min(__v.len() as i64) as usize; \
                         __v[__o..__e].join(\" \") }}"
                    );
                }
                if !name.is_empty() && !name.starts_with('$')
                    && (self.is_array(&name.to_string()) || self.is_assoc(&name.to_string()))
                {
                    let arr = self.read_arr(&name.to_string());
                    return format!(
                        "{{ let __v = {arr}; let __o = if {off} < 0 {{ (__v.len() as i64 + {off}).max(0) }} else {{ {off} }} as usize; let __l = if {len} < 0 {{ __v.len() as i64 - __o as i64 }} else {{ {len} }};                          let __s = __o.max(0) as usize; let __e = ((__s as i64 + __l).max(__s as i64)).min(__v.len() as i64) as usize;                          __v[__s..__e].join(\" \") }}"
                    );
                }
                self.add_helper("substr");
                format!("__sh_substr(&{var_expr}, {off}, {len})")
            }
            "len" => {
                self.add_helper("len");
                format!("__sh_len(&{var_expr}).to_string()")
            }
            "^^" | "^^:" | ",," | ",,:" | "^" | "^:" | "," | ",:" => {
                let mode = op.trim_end_matches(':');
                self.add_helper("case");
                format!("__sh_case(&{var_expr}, {})", Self::rust_str(mode))
            }
            "basename" => {
                self.add_helper("basename");
                format!("__sh_basename(&{var_expr})")
            }
            "dirname" => {
                self.add_helper("dirname");
                format!("__sh_dirname(&{var_expr})")
            }
            _ => {
                self.mark_todo(&format!("param op {op}"));
                var_expr
            }
        }
    }

    // ── call (expr context) ──────────────────────────────────────────

    /// Non-exec Calls in String context.
    fn call_str(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "capture" => self.capture_expr(args),
            "captureWords" => {
                let w = self.capture_words_expr(args);
                self.add_helper("cat");
                format!("__sh_cat(&[{w}]).join(\" \")")
            }
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.getvar_str(name)
                } else {
                    "String::new()".to_string()
                }
            }
            "param" => self.param_str(args),
            "test" => "String::new()".to_string(),
            "exec" => self.exec_value(args),
            "arrayIndex" => self.array_index_str(args),
            "arrayLen" => format!(
                "({}).to_string()",
                self.expr_num(&IrExpr::Call {
                    func: func.to_string(),
                    args: args.to_vec(),
                })
            ),
            "join" => self.join_str(args),
            "listVar" => self.listvar_joined(args),
            "split" => {
                let s = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
                self.add_helper("split_ifs");
                format!("__sh_split_ifs(&{s}, \" \\t\\n\").join(\" \")")
            }
            "brace" => {
                let w = self.brace_words(args);
                self.add_helper("cat");
                format!("__sh_cat(&[{w}]).join(\" \")")
            }
            "arrayItems" => args
                .first()
                .map(|a| self.expr_str(a))
                .unwrap_or_else(|| "String::new()".to_string()),
            "grepMatches" => {
                self.add_helper("grepmatches");
                let text = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
                let pat = args.get(1).map(|a| self.expr_str(a)).unwrap_or_default();
                let flags = args.get(2).map(|a| self.expr_str(a)).unwrap_or_default();
                format!(
                    "{{ let (__m, __r) = __sh_grepmatches(&{text}, &{pat}, &{flags}); __SH_RC.store(__r, Ordering::SeqCst); __m }}"
                )
            }
            "redirect" => {
                // a redirect in value context — run it, yield ""
                let b = self.redirect_bool(args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            "pipeline" => {
                let b = self.pipeline_bool(args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            "whileLoop" => {
                let b = self.whileloop_bool(args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            "block" => {
                let b = self.block_bool(args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            "subshell" => {
                let b = self.subshell_bool(args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            "contains" => {
                let text = args.first().map(|a| self.expr_str(a)).unwrap_or_default();
                let needle = args.get(1).map(|a| self.expr_str(a)).unwrap_or_default();
                format!("(if {text}.contains(&{needle}) {{ \"1\" }} else {{ \"0\" }}).to_string()")
            }
            "setArray" | "setArrayAppend" => {
                // an array assign in expr context — run it, yield ""
                let b = self.setarray_bool(func, args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            "arith" => {
                let text = str_arg(args, 0).unwrap_or("").replace(GLOB_SENTINEL, "");
                if let Some(e) = self.arith_text(&text) {
                    format!("({e}).to_string()")
                } else {
                    // `$((echo \"test\"))` — bash errors, the result is
                    // unset → an empty string is faithful
                    "String::new()".to_string()
                }
            }
            "shopt" => {
                // option toggles — no-ops for the native lowering
                "String::new()".to_string()
            }
            "assign" => self.assign_call_str(args),
            "return" => {
                "{ __SH_RC.store(0, Ordering::SeqCst); return; }".to_string()
            }
            "and" => {
                let b = self.and_bool(args);
                format!("{{ let _ = {b}; String::new() }}")
            }
            _ => {
                self.mark_todo(&format!("call {func}"));
                "String::new()".to_string()
            }
        }
    }

    /// `assign(name, op, value)` — arith assignment as an i64 block.
    fn assign_call_num(&mut self, args: &[IrExpr]) -> String {
        let Some(name) = str_arg(args, 0).map(|s| s.to_string()) else {
            return "0".to_string();
        };
        let op = str_arg(args, 1).unwrap_or("=").to_string();
        let val = args.get(2).map(|v| self.expr_num(v)).unwrap_or_else(|| "0".to_string());
        self.mark_written(&name);
        let cur = self.getvar_num(&name);
        let aop = op.trim_end_matches('=');
        let new = if op == "=" {
            val.clone()
        } else {
            format!("({cur} {aop} {val})")
        };
        if self.is_num(&name) {
            let stmt = self.write_num(&name, &new);
            format!("{{ {stmt} {} }}", self.read_num(&name))
        } else if self.is_array(&name) {
            // an array var — the arith targets element 0
            let stmt = self.array_elem_set(&name, "0", &format!("({new}).to_string()"));
            format!("{{ let __v = {new}; {stmt} __v }}")
        } else {
            // a Str-typed var — the arith result written as a string
            let stmt = self.write_str(&name, &format!("({new}).to_string()"));
            format!("{{ let __v = {new}; {stmt} __v }}")
        }
    }

    fn assign_call_str(&mut self, args: &[IrExpr]) -> String {
        let Some(name) = str_arg(args, 0).map(|s| s.to_string()) else {
            return "String::new()".to_string();
        };
        let op = str_arg(args, 1).unwrap_or("=").to_string();
        let val = args
            .get(2)
            .map(|v| self.expr_str(v))
            .unwrap_or_else(|| "String::new()".to_string());
        self.mark_written(&name);
        // a num-typed target keeps the arith write (`x=$(…)` where x is
        // Int); everything else gets the string value
        if self.is_num(&name) || op != "=" {
            let n = self.assign_call_num(args);
            format!("({n}).to_string()")
        } else if self.is_array(&name) {
            let stmt = self.array_elem_set(&name, "0", &val);
            format!("{{ let __v = {val}; {stmt} __v }}")
        } else {
            let stmt = self.write_str(&name, &val);
            format!("{{ let __v = {val}; {stmt} __v }}")
        }
    }

    /// setArray as a bool block.
    fn setarray_bool(&mut self, func: &str, args: &[IrExpr]) -> String {
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 0;
        self.array_call_stmt_by_name(func, args);
        self.emit("__SH_RC.store(0, Ordering::SeqCst); __SH_RC.load(Ordering::SeqCst) == 0");
        let block = self.out.join("\n");
        self.out = saved;
        self.depth = old_depth;
        format!("{{\n{block}\n}}")
    }

    /// setArray / setArrayAppend as statements (also inside `local`).
    fn array_call_stmt_by_name(&mut self, func: &str, args: &[IrExpr]) {
        if func == "setArray" {
            self.array_call_stmt(args);
        } else if func == "setArrayAppend" {
            self.array_append_stmt(args);
        }
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => match e {
                IrExpr::Call { func, args } if func == "exec" => self.exec_stmt(args),
                IrExpr::Call { func, args } if func == "setArray" || func == "setArrayAppend" => {
                    self.array_call_stmt_by_name(func, args);
                    self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                }
                IrExpr::Call { func, args } if func == "pipeline" => {
                    let stages = pipeline_stages(args);
                    self.pipeline_stmt(&stages);
                }
                IrExpr::Call { func, args } if func == "redirect" => {
                    let mut stmts: Vec<IrStmt> = Vec::new();
                    let mut redirs: Vec<IrRedirectInfo> = Vec::new();
                    if let Some(IrExpr::Arrow(b)) = args.first() {
                        stmts = b.clone();
                    }
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        for it in items {
                            if let Some(r) = self.redirect_info(it) {
                                redirs.push(r);
                            }
                        }
                    }
                    self.redirect_render(&stmts, &redirs);
                }
                IrExpr::Call { func, args } if func == "whileLoop" => {
                    let b = self.whileloop_bool(args);
                    self.emit(&format!("let _ = {b};"));
                }
                IrExpr::Call { func, args } if func == "subshell" => {
                    let mut stmts: Vec<IrStmt> = Vec::new();
                    if let Some(IrExpr::Arrow(b)) = args.first() {
                        stmts = b.clone();
                    }
                    self.subshell_render(&stmts);
                }
                IrExpr::BinOp { lhs, op: BinOpKind::And, rhs } => {
                    let l = self.expr_bool(lhs);
                    self.emit(&format!("if {l} {{"));
                    self.depth += 1;
                    self.expr_stmt_value(rhs);
                    self.depth -= 1;
                    self.emit("}");
                }
                IrExpr::BinOp { lhs, op: BinOpKind::Or, rhs } => {
                    let l = self.expr_bool(lhs);
                    self.emit(&format!("if !{l} {{"));
                    self.depth += 1;
                    self.expr_stmt_value(rhs);
                    self.depth -= 1;
                    self.emit("}");
                }
                IrExpr::Call { func, args } if func == "test" => {
                    let b = self.test_call_bool(args);
                    self.emit(&format!("let _ = {b};"));
                }
                IrExpr::Call { func, args } if func == "grepMatches" => {
                    self.grepmatches_stmt(args);
                }
                IrExpr::Call { func, args } if func == "break" => {
                    if self.loop_depth > 0 {
                        self.emit("break;");
                    }
                }
                IrExpr::Call { func, args } if func == "continue" => {
                    if self.loop_depth > 0 {
                        self.emit("continue;");
                    }
                }
                IrExpr::Call { func, args } if func == "return" => {
                    self.emit("__SH_RC.store(0, Ordering::SeqCst); return;");
                }
                _ => {
                    let x = self.expr_any(e);
                    self.emit(&format!("let _ = {x};"));
                    self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                }
            },
            IrStmt::Assign { targets, expr, asm } => {
                if let Some(a) = asm {
                    self.mark_todo(&format!("asm label '{}' on an assign", a.template));
                }
                // `arr=(...)` — the RHS is a setArray call
                if let IrExpr::Call { func, args } = expr {
                    if func == "setArray" || func == "setArrayAppend" {
                        self.array_call_stmt_by_name(func, args);
                        self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                        return;
                    }
                }
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                if targets.len() > 1 {
                    self.mark_todo("multi-target assign");
                }
                let has_capture = expr_mentions_capture(expr);
                // `((i++))` arrives as Assign{i, Arith(IncDec)} — the
                // arith block ALREADY writes the var; the outer write
                // would clobber it with the OLD value
                if arith_has_side_effects(expr) {
                    let x = self.expr_any(expr);
                    self.emit(&format!(
                        "let _ = {{ let __v = {x}; let __n = __v.trim().parse::<i64>().unwrap_or(0); __SH_RC.store(if __n != 0 {{ 0 }} else {{ 1 }}, Ordering::SeqCst); __v }};"
                    ));
                    return;
                }
                let rhs = self.expr_any(expr);
                // `arr[i]=v` — the var text carries the index
                if let Some(open) = t.var.find('[') {
                    if t.var.ends_with(']') {
                        let var = t.var[..open].to_string();
                        let key = t.var[open + 1..t.var.len() - 1].to_string();
                        self.mark_written(&var);
                        if self.is_assoc(&var) {
                            let k = self.assoc_key_expr(&key);
                            let st = self.assoc_set(&var, &k, &rhs);
                            self.emit(&st);
                        } else {
                            self.arrays.insert(var.clone());
                            let k = self.key_num_expr(&key);
                            let st = self.array_elem_set(&var, &k, &rhs);
                            self.emit(&st);
                        }
                        if !has_capture {
                            self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                        }
                        return;
                    }
                }
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
                }
                self.mark_written(&t.var);
                if self.is_array(&t.var) {
                    // a scalar assign to an array var — bash sets element 0
                    self.add_helper("cat");
                    let st = self.write_arr(&t.var, &format!("__sh_cat(&[vec![{rhs}]])"));
                    self.emit(&st);
                } else if self.is_assoc(&t.var) {
                    self.mark_todo("assoc bare assign");
                } else if self.is_num(&t.var) {
                    // `typeset -i n; n='n+1'` — the -i attribute makes the
                    // text an arithmetic expression
                    let n = match expr {
                        IrExpr::Str(s, _) => {
                            if let Some(e) = self.arith_text(s) {
                                e
                            } else {
                                self.expr_num(expr)
                            }
                        }
                        _ => self.expr_num(expr),
                    };
                    let st = self.write_num(&t.var, &n);
                    self.emit(&st);
                } else {
                    let v = self.case_attr(&t.var, &rhs);
                    let st = self.write_str(&t.var, &v);
                    self.emit(&st);
                }
                if !has_capture {
                    self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    self.mark_written(&d.name);
                    if let Some(e) = init {
                        if self.is_num(&d.name) {
                            let n = self.expr_num(e);
                            let st = self.write_num(&d.name, &n);
                            self.emit(&st);
                        } else {
                            let v = self.expr_any(e);
                            let st = self.write_str(&d.name, &v);
                            self.emit(&st);
                        }
                    }
                }
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.mark_written(var);
                self.arrays.insert(var.clone());
                let items: Vec<String> = elements.iter().map(|i| self.words_expr(i)).collect();
                self.add_helper("cat");
                let st = self.write_arr(var, &format!("__sh_cat(&[{}])", items.join(", ")));
                self.emit(&st);
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            IrStmt::Output { value, newline, target } => {
                if target.is_some() {
                    self.mark_todo("Output to filehandle");
                    return;
                }
                let w = self.words_expr(value);
                self.add_helper("print_words");
                self.emit(&format!("__sh_print_words(&[{w}], {}, false);", newline));
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            IrStmt::WriteFile { path, content, append } => {
                let p = self.expr_str(path);
                let c = self.expr_str(content);
                if *append {
                    self.emit(&format!(
                        "{{ let mut __f = std::fs::OpenOptions::new().append(true).create(true).open(&{p}).unwrap_or_else(|_| {{ let f = std::fs::File::open(\"/dev/null\").unwrap(); f }}); use std::io::Write; let _ = __f.write_all({c}.as_bytes()); }}"
                    ));
                } else {
                    self.emit(&format!("let _ = std::fs::write({p}, {c});"));
                }
                self.emit("__SH_RC.store(0, Ordering::SeqCst);");
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("if {c} {{"));
                self.depth += 1;
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.expr_bool(ec);
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
                } else {
                    // no branch ran — bash's if rc is 0
                    self.emit("} else {");
                    self.depth += 1;
                    self.emit("__SH_RC.store(0, Ordering::SeqCst);");
                    self.depth -= 1;
                }
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("while {c} {{"));
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("}");
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.emit("loop {");
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                let c = self.expr_bool(cond);
                if *until {
                    self.emit(&format!("if {c} {{ break; }}"));
                } else {
                    self.emit(&format!("if !{c} {{ break; }}"));
                }
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => {
                self.mark_written(var);
                match iter {
                    IrExpr::Range { start, end } if self.is_num(var) => {
                        let init = self.write_num(var, &start.to_string()).trim_end_matches(';').to_string();
                        // the trailing `;` must survive the comment (the
                        // `// for i in N..=M` note would otherwise swallow
                        // it and the loop body fails to compile — py-sh-go
                        // t58_seq_range, zsh/posix t58_seq_range)
                        self.emit(&format!("{init}; // for {var} in {start}..={end}"));
                        let cond = self.read_num(var);
                        self.emit(&format!("loop {{ if !({cond} <= {end}) {{ break; }}"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        for s in body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        let cur = self.read_num(var);
                        let step = self.write_num(var, &format!("{cur} + 1")).trim_end_matches(';').to_string();
                        self.emit(&format!("{step};"));
                        self.emit("}");
                    }
                    other => {
                        let items = self.words_expr(other);
                        let items_g = self.gensym("__sh_items");
                        let idx_g = self.gensym("__sh_i");
                        self.emit(&format!("let {items_g} = {items};"));
                        self.emit(&format!("let mut {idx_g}: usize = 0;"));
                        // `loop` so a body `continue` still runs the
                        // index increment (bash's for-iteration)
                        self.emit(&format!("loop {{"));
                        self.emit(&format!("if !({idx_g} < {items_g}.len()) {{ break; }}"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        if self.is_num(var) {
                            let st = self.write_num(var, &format!("{items_g}[{idx_g}].trim().parse::<i64>().unwrap_or(0)"));
                            self.emit(&st);
                        } else {
                            let st = self.write_str(var, &format!("{items_g}[{idx_g}].clone()"));
                            self.emit(&st);
                        }
                        let old_for = self.for_index.replace(idx_g.clone());
                        for s in body {
                            self.stmt(s);
                        }
                        self.for_index = old_for;
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit(&format!("{idx_g} += 1;"));
                        self.emit("}");
                    }
                }
            }
            IrStmt::Exit(e) => {
                let code = e.as_ref().map(|x| self.expr_num(x)).unwrap_or_else(|| "0".into());
                self.emit(&format!("std::process::exit(({code}) as i32);"));
            }
            IrStmt::Return(e) => {
                if let Some(x) = e {
                    let n = self.expr_num(x);
                    self.emit(&format!("__SH_RC.store(({n}) as i32, Ordering::SeqCst); return;"));
                } else {
                    self.emit("return;");
                }
            }
            IrStmt::Continue => {
                if self.loop_depth > 0 {
                    self.emit_continue();
                }
            }
            IrStmt::Break => {
                if self.loop_depth > 0 {
                    self.emit("break;");
                }
            }
            IrStmt::Block(b) => {
                for s in b {
                    self.stmt(s);
                }
            }
            IrStmt::Subshell(b) => self.subshell_render(b),
            IrStmt::Background(b) => self.background_render(b),
            IrStmt::Case { discriminant, clauses } => {
                let d = self.expr_str(discriminant);
                let dg = self.gensym("__sh_case");
                self.emit(&format!("let {dg} = {d};"));
                let mut first = true;
                for c in clauses {
                    let mut pats: Vec<&String> = c.patterns.iter().filter(|p| !p.is_empty() && p.as_str() != "*").collect();
                    let is_default = pats.is_empty();
                    let cond = if is_default {
                        "true".to_string()
                    } else {
                        self.add_helper("fnmatch");
                        let alts = pats
                            .iter()
                            .map(|p| format!("__sh_fnmatch({}, &{dg})", Self::rust_str(p)))
                            .collect::<Vec<_>>()
                            .join(" || ");
                        format!("({alts})")
                    };
                    if first {
                        self.emit(&format!("if {cond} {{"));
                        first = false;
                    } else {
                        self.emit(&format!("}} else if {cond} {{"));
                    }
                    self.depth += 1;
                    for s in &c.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                if first {
                    // no clauses at all — nothing
                } else {
                    self.emit("}");
                }
            }
            IrStmt::Redirect { .. } => self.redirect_stmt(s),
            IrStmt::Function { name, body, .. } => {
                // definitions are emitted after main (see program())
                let _ = (name, body);
            }
            IrStmt::Exec { cmd, args, redirects, env, .. } => {
                // ESTree-path exec: shell-out cmd + args (+ redirect text)
                let mut words: Vec<IrExpr> = vec![cmd.clone()];
                words.extend(args.clone());
                let env: Vec<(String, IrExpr)> = env.clone();
                let text = self.cmd_text(
                    &words.iter().collect::<Vec<_>>(),
                    if env.is_empty() { None } else { Some(&env) },
                );
                let mut full = text;
                for r in redirects {
                    if let IrExpr::Object(props) = r {
                        for (k, v) in props {
                            if k == "mode" {
                                if let IrExpr::Str(m, _) = v {
                                    if m == "w" || m == "a" {
                                        // target is the next Object prop — handled below
                                    }
                                }
                            }
                        }
                    }
                }
                self.add_helper("run");
                self.emit(&format!("__SH_RC.store(__sh_run(&{full}), Ordering::SeqCst);"));
            }
            IrStmt::Pipeline { stages, .. } => {
                self.pipeline_stmt(stages);
            }
            IrStmt::Try { .. } => self.mark_todo("try"),
            IrStmt::Select { .. } => self.mark_todo("select"),
            IrStmt::Asm { .. } => self.mark_todo("asm"),
            IrStmt::ForInit { init, cond, step, body } => {
                for s in init {
                    self.stmt(s);
                }
                let c = self.expr_bool(cond);
                self.emit(&format!("while {c} {{"));
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("if true {");
                self.depth += 1;
                for s in step {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
                self.emit("}");
            }
            IrStmt::Die { .. } | IrStmt::Warn { .. } | IrStmt::SetChildError(_)
            | IrStmt::Require(_) | IrStmt::RawText(_) | IrStmt::Goto(_)
            | IrStmt::Label(_) => {
                self.mark_todo(&format!("stmt {:?}", s));
            }
        }
    }

    /// Subshell: run the body with assigned vars saved/restored.
    fn subshell_render(&mut self, body: &[IrStmt]) {
        let mut written: BTreeSet<String> = BTreeSet::new();
        collect_written(body, &mut written);
        written.retain(|v| self.declared(v));
        let mut pre = Vec::new();
        let mut post = Vec::new();
        for v in &written {
            let m = self.tls(v);
            let sv = self.gensym("__sh_sv");
            if self.is_num(v) {
                pre.push(format!("let {sv} = {m}.with(|v| v.get());"));
                post.push(format!("{m}.with(|v| v.set({sv}));"));
            } else {
                pre.push(format!("let {sv} = {m}.with(|v| v.borrow().clone());"));
                post.push(format!("{m}.with(|v| *v.borrow_mut() = {sv});"));
            }
        }
        for p in pre {
            self.emit(&p);
        }
        for s in body {
            self.stmt(s);
        }
        for p in post {
            self.emit(&p);
        }
    }

    /// Background: text-reconstructable bodies run as real child
    /// processes (so `$!`/`wait` work); native bodies run on a thread
    /// with the referenced vars captured.
    fn background_render(&mut self, body: &[IrStmt]) {
        if let Some(text) = self.stage_text(body) {
            self.add_helper("bg");
            self.emit(&format!("__sh_bg(&{text});"));
            return;
        }
        // native body → thread with captured vars
        let mut refs: BTreeSet<String> = BTreeSet::new();
        collect_reads(body, &mut refs);
        let mut pre = Vec::new();
        let mut captured = self.captured.clone();
        for v in &refs {
            if !self.declared(v) {
                continue;
            }
            let m = self.tls(v);
            let cap = self.gensym("__sh_cap");
            if self.is_num(v) {
                pre.push(format!("let mut {cap} = {m}.with(|v| v.get());"));
            } else {
                pre.push(format!("let mut {cap} = {m}.with(|v| v.borrow().clone());"));
            }
            captured.insert(v.clone(), cap.clone());
        }
        let mut saved = std::mem::take(&mut self.out);
        let old_depth = self.depth;
        self.depth = 1;
        let old_captured = std::mem::replace(&mut self.captured, captured);
        for s in body {
            self.stmt(s);
        }
        let body_src = self.out.join("\n");
        self.captured = old_captured;
        self.out = saved;
        self.depth = old_depth;
        for p in pre {
            self.emit(&p);
        }
        // detached thread — the body runs on a copy (bash `{ ... } &`
        // forks; the handle is dropped, the thread is never joined)
        self.emit(&format!(
            "let _ = std::thread::spawn(move || {{\n{body_src}\n}});"
        ));
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect written vars + arrays + assoc + functions
        let mut written: BTreeSet<String> = BTreeSet::new();
        collect_written(&prog.stmts, &mut written);
        collect_arrays(&prog.stmts, &mut self.arrays, &mut self.assoc);
        collect_attrs(&prog.stmts, &mut self.int_vars, &mut self.lower_vars, &mut self.upper_vars);
        collect_functions(&prog.stmts, &mut self.functions);
        for (n, _) in &prog.var_types {
            written.insert(n.clone());
        }
        // array vars are declared even when only READ (arrayIndex/arrayLen/
        // param slice) — a read-only array still needs its static
        for a in &self.arrays {
            written.insert(a.clone());
        }
        for a in &self.assoc {
            written.insert(a.clone());
        }
        self.written = written.clone();

        // Pass 2: render the body first (helper flags known before preamble).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        for s in &prog.stmts {
            self.stmt(s);
        }
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        // The thread_local var declarations are MODULE-level statics (the
        // function bodies reference them) — emitted before main.
        let mut decl_out = Vec::new();
        std::mem::swap(&mut self.out, &mut decl_out);
        self.depth = 0;
        for v in &written {
            let d = self.decl_stmt(v);
            self.emit(&d);
        }
        if !written.is_empty() {
            self.emit("");
        }
        std::mem::swap(&mut self.out, &mut decl_out);
        self.depth = 0;

        // Pass 3: render the functions (after main). Nested function
        // definitions (a fn inside a fn body) are still GLOBAL in bash —
        // collected from the whole tree and emitted at module level.
        let mut fns: Vec<(String, Vec<IrStmt>, bool)> = Vec::new();
        collect_fn_defs(&prog.stmts, &mut fns);
        let mut fn_out = Vec::new();
        std::mem::swap(&mut self.out, &mut fn_out);
        self.depth = 0;
        for (name, body, named) in &fns {
            if *named {
                self.mark_todo(&format!("function {name} named blocks"));
            }
            let m = self.fn_ident(name);
            self.emit(&format!("fn {m}() {{"));
            self.depth += 1;
            for st in body {
                self.stmt(st);
            }
            self.depth -= 1;
            self.emit("}");
            self.emit("");
        }
        std::mem::swap(&mut self.out, &mut fn_out);
        self.depth = 0;

        // Preamble: statics + helpers, then main with the rendered body.
        self.emit("#![allow(unused_imports)]");
        self.emit("#![allow(non_upper_case_globals)]");
        self.emit("use std::sync::atomic::Ordering;");
        self.emit("use std::io::Read;");
        self.emit("use std::io::Write;");
        self.emit("");
        self.emit("static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);");
        self.emit("static __SH_ARGV: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());");
        self.emit("static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);");
        self.emit("static __SH_OUTFILE: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);");
        self.emit("static __SH_STDIN: std::sync::Mutex<Option<Box<dyn std::io::Read + Send>>> = std::sync::Mutex::new(None);");
        self.emit("static __SH_BG: std::sync::Mutex<Vec<(u32, std::process::Child)>> = std::sync::Mutex::new(Vec::new());");
        self.emit("static __SH_BGPID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);");
        self.emit("");
        self.out.extend(decl_out.iter().cloned());
        for h in HELPER_ORDER {
            if self.helpers.contains(*h) {
                let src = helper_source(h);
                self.emit(src);
                self.emit("");
            }
        }
        self.emit("#[allow(non_upper_case_globals)]");
        self.emit("#[allow(dead_code)]");
        self.emit("fn main() {");
        self.out.extend(body_out.iter().cloned());
        self.emit("}");
        if !fn_out.is_empty() {
            self.emit("");
            self.out.extend(fn_out.iter().cloned());
        }
        if self.todo > 0 {
            self.emit(&format!("// {} construct(s) lowered to TODO markers", self.todo));
        }
    }

    /// The thread_local declaration for one var.
    fn decl_stmt(&mut self, v: &str) -> String {
        let m = self.tls(v);
        if self.is_assoc(v) {
            format!(
                "thread_local! {{ static {m}: std::cell::RefCell<std::collections::BTreeMap<String, String>> = const {{ std::cell::RefCell::new(std::collections::BTreeMap::new()) }}; }}"
            )
        } else if self.is_array(v) {
            format!(
                "thread_local! {{ static {m}: std::cell::RefCell<Vec<String>> = const {{ std::cell::RefCell::new(Vec::new()) }}; }}"
            )
        } else if self.is_num(v) {
            format!(
                "thread_local! {{ static {m}: std::cell::Cell<i64> = const {{ std::cell::Cell::new(0) }}; }}"
            )
        } else {
            format!(
                "thread_local! {{ static {m}: std::cell::RefCell<String> = const {{ std::cell::RefCell::new(String::new()) }}; }}"
            )
        }
    }
}

impl Render {
    fn is_str(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Str)
    }

    /// Assign statement for one target (used by decl_words).
    fn assign_stmt_for(&mut self, name: &str, e: &IrExpr) -> String {
        if self.is_array(name) {
            let w = self.words_expr(e);
            self.add_helper("cat");
            self.write_arr(name, &format!("__sh_cat(&[{w}])"))
        } else if self.is_num(name) {
            let n = self.expr_num(e);
            self.write_num(name, &n)
        } else {
            let v = self.expr_any(e);
            let cv = self.case_attr(name, &v);
            self.write_str(name, &cv)
        }
    }
}

/// One parsed redirect.
struct IrRedirectInfo {
    fd: i64,
    mode: String,
    target: Option<IrExpr>,
}

// ── helpers registry ─────────────────────────────────────────────────

const HELPER_ORDER: &[&str] = &[
    "q", "wq", "cat", "pow", "echo_esc", "atoi", "atou", "atof", "print_words", "printf",
    "cap_bytes", "capture_rc", "spawn", "run", "readline", "read_fields", "split_ifs",
    "fnmatch", "strippre", "stripsuf", "replace", "substr", "case", "len", "basename",
    "dirname", "env", "arg", "glob", "brace", "sleep", "rand", "grepmatches", "regex",
    "mtime", "samefile", "fmode", "fowner", "fgroup", "fnewer", "wait_all", "bg",
    "fexists", "fdir", "freg", "fsym", "fread", "fwrite", "fexec", "fsize", "aindex",
];

fn helper_deps(h: &str) -> &'static [&'static str] {
    match h {
        "wq" => &["q"],
        "print_words" => &["echo_esc"],
        "printf" => &["q", "echo_esc", "atoi", "atou", "atof"],
        "capture_rc" => &["cap_bytes"],
        "run" => &["spawn"],
        "strippre" | "stripsuf" | "replace" => &["fnmatch"],
        "glob" => &["cap_bytes"],
        "rand" => &["cap_bytes"],
        "grepmatches" => &["cap_bytes"],
        _ => &[],
    }
}

fn helper_source(h: &str) -> &'static str {
    match h {
        "q" => r#"fn __sh_q(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    o.push('\'');
    for c in s.chars() {
        if c == '\'' { o.push_str("'\\''"); } else { o.push(c); }
    }
    o.push('\'');
    o
}"#,
        "wq" => r#"fn __sh_wq(ws: &[String]) -> String {
    let mut o = String::new();
    for (i, w) in ws.iter().enumerate() {
        if i > 0 { o.push(' '); }
        o.push_str(&__sh_q(w));
    }
    o
}"#,
        "cat" => r#"fn __sh_cat(ws: &[Vec<String>]) -> Vec<String> {
    let mut o = Vec::new();
    for w in ws { o.extend(w.iter().cloned()); }
    o
}"#,
        "pow" => r#"fn __sh_pow(a: i64, b: i64) -> i64 {
    let mut r: i64 = 1;
    let mut i: i64 = 0;
    while i < b { r = r.wrapping_mul(a); i += 1; }
    r
}"#,
        "echo_esc" => r#"fn __sh_echo_esc(s: &str) -> String {
    let mut o = String::new();
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '\\' && i + 1 < ch.len() {
            i += 1;
            match ch[i] {
                'n' => o.push('\n'), 't' => o.push('\t'), 'r' => o.push('\r'),
                'a' => o.push('\x07'), 'b' => o.push('\x08'), 'f' => o.push('\x0c'),
                'v' => o.push('\x0b'), 'e' => o.push('\x1b'), '\\' => o.push('\\'),
                'c' => return o,
                '0'..='7' => {
                    let mut n = 0; let mut k = 0;
                    while k < 3 && i + k < ch.len() && ch[i + k].is_ascii_digit() && ch[i + k] <= '7' {
                        n = n * 8 + (ch[i + k] as u32 - '0' as u32); k += 1;
                    }
                    o.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                    i += k - 1;
                }
                'x' => {
                    let mut n = 0; let mut k = 0;
                    while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                        n = n * 16 + ch[i + 1 + k].to_digit(16).unwrap(); k += 1;
                    }
                    if k > 0 { o.push(char::from_u32(n).unwrap_or('\0')); i += k; }
                    else { o.push('x'); }
                }
                c => { o.push('\\'); o.push(c); }
            }
            i += 1;
        } else { o.push(ch[i]); i += 1; }
    }
    o
}"#,
        "atoi" => r#"fn __sh_atoi(s: &str) -> i64 {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).unwrap_or(0);
    }
    t.parse::<i64>().unwrap_or(0)
}"#,
        "atou" => r#"fn __sh_atou(s: &str) -> u64 {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).unwrap_or(0);
    }
    t.parse::<u64>().unwrap_or(0)
}"#,
        "atof" => r#"fn __sh_atof(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}"#,
        "print_words" => r#"fn __sh_print_words(ws: &[Vec<String>], nl: bool, esc: bool) {
    let mut s = String::new();
    let mut first = true;
    for w in ws {
        for x in w {
            if !first { s.push(' '); }
            first = false;
            s.push_str(x);
        }
    }
    if esc { s = __sh_echo_esc(&s); }
    if nl { s.push('\n'); }
    let of = __SH_OUTFILE.lock().unwrap();
    if let Some(f) = of.as_ref() {
        let _ = (&*f).write_all(s.as_bytes());
        return;
    }
    drop(of);
    if let Some(b) = __SH_OUT.lock().unwrap().as_mut() {
        b.extend_from_slice(s.as_bytes());
        return;
    }
    print!("{}", s);
}"#,
        "printf" => r#"fn __sh_printf(fmt: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut ai = 0usize;
    loop {
        let start_ai = ai;
        let ch: Vec<char> = fmt.chars().collect();
        let mut i = 0;
        while i < ch.len() {
            let c = ch[i];
            if c == '\\' && i + 1 < ch.len() {
                i += 1;
                match ch[i] {
                    'n' => out.push('\n'), 't' => out.push('\t'), 'r' => out.push('\r'),
                    'a' => out.push('\x07'), 'b' => out.push('\x08'), 'f' => out.push('\x0c'),
                    'v' => out.push('\x0b'), 'e' => out.push('\x1b'), '\\' => out.push('\\'),
                    'c' => return out,
                    '0'..='7' => {
                        let mut n = 0; let mut k = 0;
                        while k < 3 && i + k < ch.len() && ch[i + k].is_ascii_digit() && ch[i + k] <= '7' {
                            n = n * 8 + (ch[i + k] as u32 - '0' as u32); k += 1;
                        }
                        out.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                        i += k - 1;
                    }
                    'x' => {
                        let mut n = 0; let mut k = 0;
                        while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                            n = n * 16 + ch[i + 1 + k].to_digit(16).unwrap(); k += 1;
                        }
                        if k > 0 { out.push(char::from_u32(n).unwrap_or('\0')); i += k; }
                        else { out.push('x'); }
                    }
                    _ => { out.push('\\'); out.push(ch[i]); }
                }
                i += 1;
                continue;
            }
            if c == '%' && i + 1 < ch.len() {
                let mut j = i + 1;
                let mut flags = String::new();
                while j < ch.len() && "-+ 0#".contains(ch[j]) { flags.push(ch[j]); j += 1; }
                let mut width = String::new();
                while j < ch.len() && ch[j].is_ascii_digit() { width.push(ch[j]); j += 1; }
                let mut prec: Option<String> = None;
                if j < ch.len() && ch[j] == '.' {
                    j += 1;
                    let mut p = String::new();
                    while j < ch.len() && ch[j].is_ascii_digit() { p.push(ch[j]); j += 1; }
                    prec = Some(p);
                }
                if j >= ch.len() { out.push('%'); break; }
                let conv = ch[j];
                i = j + 1;
                if conv == '%' { out.push('%'); continue; }
                let arg = args.get(ai).cloned().unwrap_or_default();
                ai += 1;
                let w: usize = width.parse().unwrap_or(0);
                let left = flags.contains('-');
                let zero = flags.contains('0');
                let p: i64 = prec.as_ref().map(|s| s.parse().unwrap_or(0)).unwrap_or(-1);
                let mut piece = String::new();
                match conv {
                    's' => {
                        piece = arg.clone();
                        if p >= 0 { piece = piece.chars().take(p as usize).collect(); }
                    }
                    'b' => {
                        piece = __sh_echo_esc(&arg);
                        if p >= 0 { piece = piece.chars().take(p as usize).collect(); }
                    }
                    'q' => { piece = __sh_q(&arg); }
                    'c' => { piece = arg.chars().next().map(|c| c.to_string()).unwrap_or_default(); }
                    'd' | 'i' => { piece = format!("{}", __sh_atoi(&arg)); }
                    'u' => { piece = format!("{}", __sh_atou(&arg)); }
                    'o' => { piece = format!("{:o}", __sh_atou(&arg)); }
                    'x' => { piece = format!("{:x}", __sh_atou(&arg)); }
                    'X' => { piece = format!("{:X}", __sh_atou(&arg)); }
                    'f' | 'F' => {
                        let fv = __sh_atof(&arg);
                        let p2 = if p < 0 { 6 } else { p as usize };
                        piece = format!("{:.*}", p2, fv);
                    }
                    'e' | 'E' => {
                        let fv = __sh_atof(&arg);
                        let p2 = if p < 0 { 6 } else { p as usize };
                        piece = format!("{:.*e}", p2, fv);
                        if conv == 'E' { piece = piece.to_uppercase(); }
                    }
                    'g' | 'G' => {
                        let fv = __sh_atof(&arg);
                        let p2 = if p < 0 { 6 } else { p as usize };
                        piece = format!("{:.*}", p2, fv);
                    }
                    _ => { piece = arg.clone(); }
                }
                if w > piece.chars().count() {
                    let pad = w - piece.chars().count();
                    if left { piece = format!("{}{}", piece, " ".repeat(pad)); }
                    else if zero { piece = format!("{}{}", "0".repeat(pad), piece); }
                    else { piece = format!("{}{}", " ".repeat(pad), piece); }
                }
                out.push_str(&piece);
                continue;
            }
            out.push(c);
            i += 1;
        }
        if ai >= args.len() || ai == start_ai { break; }
    }
    out
}"#,
        "cap_bytes" => r#"fn __sh_cap_bytes(cmd: &str, input: Option<&[u8]>) -> Vec<u8> {
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(cmd);
    c.stdout(std::process::Stdio::piped());
    if input.is_some() { c.stdin(std::process::Stdio::piped()); }
    let mut ch = match c.spawn() { Ok(x) => x, Err(_) => return Vec::new() };
    if let Some(data) = input {
        if let Some(mut si) = ch.stdin.take() { let _ = si.write_all(data); }
    }
    let mut out = Vec::new();
    if let Some(mut so) = ch.stdout.take() {
        let _ = so.read_to_end(&mut out);
    }
    let _ = ch.wait();
    out
}"#,
        "capture_rc" => r#"fn __sh_capture_rc(cmd: &str) -> (String, i32) {
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(cmd);
    c.stdout(std::process::Stdio::piped());
    let mut ch = match c.spawn() { Ok(x) => x, Err(_) => return (String::new(), 1) };
    let mut out = Vec::new();
    if let Some(mut so) = ch.stdout.take() {
        let _ = so.read_to_end(&mut out);
    }
    let rc = ch.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    let mut s = String::from_utf8_lossy(&out).to_string();
    while s.ends_with('\n') { s.pop(); }
    (s, rc)
}"#,
        "spawn" => r#"fn __sh_spawn(cmd: &str, input: Option<&[u8]>) -> i32 {
    let want_out = __SH_OUT.lock().unwrap().is_some() || __SH_OUTFILE.lock().unwrap().is_some();
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(cmd);
    if want_out { c.stdout(std::process::Stdio::piped()); }
    if input.is_some() { c.stdin(std::process::Stdio::piped()); }
    let mut ch = match c.spawn() { Ok(x) => x, Err(_) => return 1 };
    if let Some(data) = input {
        if let Some(mut si) = ch.stdin.take() { let _ = si.write_all(data); }
    }
    let mut out = Vec::new();
    if let Some(mut so) = ch.stdout.take() {
        let _ = so.read_to_end(&mut out);
    }
    let rc = ch.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    if !out.is_empty() {
        let of = __SH_OUTFILE.lock().unwrap();
        if let Some(f) = of.as_ref() {
            let _ = (&*f).write_all(&out);
        } else {
            drop(of);
            if let Some(b) = __SH_OUT.lock().unwrap().as_mut() {
                b.extend_from_slice(&out);
            } else {
                let _ = std::io::stdout().write_all(&out);
            }
        }
    }
    rc
}"#,
        "run" => r#"fn __sh_run(cmd: &str) -> i32 {
    __sh_spawn(cmd, None)
}"#,
        "readline" => r#"fn __sh_readline() -> (String, bool) {
    let mut buf: Vec<u8> = Vec::new();
    let mut one = [0u8; 1];
    let mut any = false;
    let src: Box<dyn std::io::Read + Send> = match __SH_STDIN.lock().unwrap().take() {
        Some(r) => r,
        None => Box::new(std::io::stdin()),
    };
    let mut r = src;
    loop {
        match r.read(&mut one) {
            Ok(0) => break,
            Ok(_) => {
                if one[0] == b'\n' { any = true; break; }
                buf.push(one[0]);
                any = true;
            }
            Err(_) => break,
        }
    }
    *__SH_STDIN.lock().unwrap() = Some(r);
    (String::from_utf8_lossy(&buf).to_string(), any)
}"#,
        "read_fields" => r#"fn __sh_read_fields(line: &str, ifs: &str, n: usize) -> Vec<String> {
    if n <= 1 || ifs.is_empty() { return vec![line.to_string()]; }
    let ws = ifs.chars().all(|c| c == ' ' || c == '\t' || c == '\n');
    let ch: Vec<char> = line.chars().collect();
    let is_sep = |c: char| ifs.contains(c);
    let mut fields: Vec<String> = Vec::new();
    let mut i = 0;
    while i < ch.len() && is_sep(ch[i]) { i += 1; }
    while fields.len() < n - 1 && i < ch.len() {
        let start = i;
        while i < ch.len() && !is_sep(ch[i]) { i += 1; }
        fields.push(ch[start..i].iter().collect());
        if i >= ch.len() { break; }
        i += 1;
        if ws { while i < ch.len() && is_sep(ch[i]) { i += 1; } }
    }
    let rest: String = ch[i.min(ch.len())..].iter().collect();
    fields.push(rest);
    fields
}"#,
        "split_ifs" => r#"fn __sh_split_ifs(s: &str, ifs: &str) -> Vec<String> {
    if ifs.is_empty() { return vec![s.to_string()]; }
    let ws = ifs.chars().all(|c| c == ' ' || c == '\t' || c == '\n');
    let ch: Vec<char> = s.chars().collect();
    let is_sep = |c: char| ifs.contains(c);
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() && is_sep(ch[i]) { i += 1; }
    while i < ch.len() {
        let start = i;
        while i < ch.len() && !is_sep(ch[i]) { i += 1; }
        out.push(ch[start..i].iter().collect());
        if i >= ch.len() { break; }
        i += 1;
        if ws { while i < ch.len() && is_sep(ch[i]) { i += 1; } }
    }
    out
}"#,
        "fnmatch" => r#"fn __sh_fnmatch(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = s.chars().collect();
    fn m(p: &[char], t: &[char]) -> bool {
        if p.is_empty() { return t.is_empty(); }
        match p[0] {
            '*' => {
                for i in 0..=t.len() { if m(&p[1..], &t[i..]) { return true; } }
                false
            }
            '?' => !t.is_empty() && m(&p[1..], &t[1..]),
            '[' => {
                if t.is_empty() { return false; }
                let mut j = 1;
                let neg = j < p.len() && (p[j] == '!' || p[j] == '^');
                if neg { j += 1; }
                let mut matched = false;
                let mut first = true;
                while j < p.len() && (p[j] != ']' || first) {
                    first = false;
                    if j + 2 < p.len() && p[j + 1] == '-' && p[j + 2] != ']' {
                        if t[0] >= p[j] && t[0] <= p[j + 2] { matched = true; }
                        j += 3;
                    } else {
                        if t[0] == p[j] { matched = true; }
                        j += 1;
                    }
                }
                if j < p.len() && p[j] == ']' { j += 1; }
                if matched != neg { m(&p[j..], &t[1..]) } else { false }
            }
            '\\' => p.len() > 1 && !t.is_empty() && t[0] == p[1] && m(&p[2..], &t[1..]),
            c if (c == '@' || c == '?' || c == '*' || c == '+' || c == '!')
                && p.len() > 1 && p[1] == '(' => {
                // extglob: parse the group's alternatives
                let mut depth = 1;
                let mut j = 2;
                let mut alts: Vec<Vec<char>> = Vec::new();
                let mut cur: Vec<char> = Vec::new();
                while j < p.len() && depth > 0 {
                    match p[j] {
                        '(' => { depth += 1; cur.push(p[j]); }
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                alts.push(cur.clone());
                            } else { cur.push(p[j]); }
                        }
                        '|' if depth == 1 => { alts.push(std::mem::take(&mut cur)); }
                        ch => cur.push(ch),
                    }
                    j += 1;
                }
                if depth != 0 { return false; }
                let rest = &p[j..];
                match c {
                    '@' => {
                        for a in &alts {
                            let mut joined = a.clone();
                            joined.extend_from_slice(rest);
                            if m(&joined, t) { return true; }
                        }
                        false
                    }
                    '?' => {
                        if m(rest, t) { return true; }
                        for a in &alts {
                            let mut joined = a.clone();
                            joined.extend_from_slice(rest);
                            if m(&joined, t) { return true; }
                        }
                        false
                    }
                    '*' => {
                        if m(rest, t) { return true; }
                        for a in &alts {
                            let mut joined = a.clone();
                            joined.push('*');
                            joined.extend_from_slice(&p[..p.len() - rest.len()]);
                            if m(&joined, t) { return true; }
                        }
                        false
                    }
                    '+' => {
                        for a in &alts {
                            let mut joined = a.clone();
                            joined.push('*');
                            joined.extend_from_slice(&p[..p.len() - rest.len()]);
                            if m(&joined, t) { return true; }
                        }
                        false
                    }
                    _ => {
                        // !(alts)rest: t = t1 ++ t2 with t2 matching rest
                        // and t1 matching NO alternative
                        for i in 0..=t.len() {
                            if m(rest, &t[i..]) {
                                let mut ok = true;
                                for a in &alts {
                                    if m(a, &t[..i]) { ok = false; break; }
                                }
                                if ok { return true; }
                            }
                        }
                        false
                    }
                }
            }
            c => !t.is_empty() && t[0] == c && m(&p[1..], &t[1..]),
        }
    }
    m(&p, &t)
}"#,
        "strippre" => r#"fn __sh_strippre(s: &str, pat: &str, greedy: bool) -> String {
    let ch: Vec<char> = s.chars().collect();
    if greedy {
        let mut l = ch.len();
        loop {
            let pre: String = ch[..l].iter().collect();
            if __sh_fnmatch(pat, &pre) { return ch[l..].iter().collect(); }
            if l == 0 { break; }
            l -= 1;
        }
    } else {
        let mut l = 0;
        while l <= ch.len() {
            let pre: String = ch[..l].iter().collect();
            if __sh_fnmatch(pat, &pre) { return ch[l..].iter().collect(); }
            l += 1;
        }
    }
    s.to_string()
}"#,
        "stripsuf" => r#"fn __sh_stripsuf(s: &str, pat: &str, greedy: bool) -> String {
    let ch: Vec<char> = s.chars().collect();
    if greedy {
        // longest suffix = smallest prefix index
        let mut l = 0;
        while l <= ch.len() {
            let suf: String = ch[l..].iter().collect();
            if __sh_fnmatch(pat, &suf) { return ch[..l].iter().collect(); }
            l += 1;
        }
    } else {
        let mut l = ch.len();
        loop {
            let suf: String = ch[l..].iter().collect();
            if __sh_fnmatch(pat, &suf) { return ch[..l].iter().collect(); }
            if l == 0 { break; }
            l -= 1;
        }
    }
    s.to_string()
}"#,
        "replace" => r#"fn __sh_replace(s: &str, pat0: &str, repl: &str, all: bool) -> String {
    let anchored_s = pat0.starts_with('#');
    let anchored_e = pat0.starts_with('%');
    let pat = pat0.trim_start_matches('#').trim_start_matches('%');
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut done = false;
    while i <= ch.len() {
        if anchored_s && i > 0 { break; }
        let mut matched: Option<usize> = None;
        let mut end = i;
        while end <= ch.len() {
            let sub: String = ch[i..end].iter().collect();
            if __sh_fnmatch(pat, &sub) { matched = Some(end); break; }
            end += 1;
        }
        if let Some(e) = matched {
            if anchored_e && e != ch.len() { matched = None; }
        }
        if let Some(e) = matched {
            out.push_str(repl);
            i = e;
            done = true;
            if !all {
                out.extend(ch[i..].iter());
                break;
            }
            if i >= ch.len() { break; }
        } else {
            if i < ch.len() { out.push(ch[i]); }
            i += 1;
            if anchored_s { break; }
        }
    }
    if done { out } else { s.to_string() }
}"#,
        "substr" => r#"fn __sh_substr(s: &str, off: i64, len: i64) -> String {
    let ch: Vec<char> = s.chars().collect();
    let n = ch.len() as i64;
    let mut o = if off < 0 { n + off } else { off };
    if o < 0 { o = 0; }
    if o > n { return String::new(); }
    let mut l = if len < 0 { n - o } else { len };
    if l < 0 { l = 0; }
    if o + l > n { l = n - o; }
    ch[o as usize..(o + l) as usize].iter().collect()
}"#,
        "case" => r#"fn __sh_case(s: &str, mode: &str) -> String {
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::new();
    match mode {
        "^^" => { for c in ch { for u in c.to_uppercase() { out.push(u); } } }
        ",," => { for c in ch { for l in c.to_lowercase() { out.push(l); } } }
        "^" => {
            let mut it = ch.into_iter();
            if let Some(c) = it.next() { for u in c.to_uppercase() { out.push(u); } }
            for c in it { out.push(c); }
        }
        "," => {
            let mut it = ch.into_iter();
            if let Some(c) = it.next() { for l in c.to_lowercase() { out.push(l); } }
            for c in it { out.push(c); }
        }
        _ => out = s.to_string(),
    }
    out
}"#,
        "len" => r#"fn __sh_len(s: &str) -> i64 {
    s.chars().count() as i64
}"#,
        "basename" => r#"fn __sh_basename(s: &str) -> String {
    match s.rfind('/') { Some(i) => s[i + 1..].to_string(), None => s.to_string() }
}"#,
        "dirname" => r#"fn __sh_dirname(s: &str) -> String {
    match s.rfind('/') {
        Some(i) if i > 0 => s[..i].to_string(),
        Some(_) => "/".to_string(),
        None => ".".to_string(),
    }
}"#,
        "env" => r#"fn __sh_env(n: &str) -> String {
    std::env::var(n).unwrap_or_default()
}"#,
        "arg" => r#"fn __sh_arg(i: usize) -> String {
    __SH_ARGV.lock().unwrap().get(i).cloned().unwrap_or_default()
}"#,
        "glob" => r#"fn __sh_glob(pat: &str) -> Vec<String> {
    let cmd = format!("printf '%s\\n' {}", pat);
    let out = __sh_cap_bytes(&cmd, None);
    let s = String::from_utf8_lossy(&out);
    let v: Vec<String> = s.lines().map(|x| x.to_string()).collect();
    if v.is_empty() { vec![pat.to_string()] } else { v }
}"#,
        "brace" => r#"fn __sh_brace(pre: &str, groups: &[&[&str]], seps: &[&str], suf: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![pre.to_string()];
    for (gi, g) in groups.iter().enumerate() {
        let mut next = Vec::new();
        for o in &out {
            for item in *g {
                next.push(format!("{}{}", o, item));
            }
        }
        out = next;
        if gi + 1 < groups.len() && gi < seps.len() {
            let sep = seps[gi];
            if !sep.is_empty() {
                for o in out.iter_mut() {
                    o.push_str(sep);
                }
            }
        }
    }
    for o in out.iter_mut() {
        o.push_str(suf);
    }
    out
}"#,
        "sleep" => r#"fn __sh_sleep(s: &str) {
    let f: f64 = s.trim().parse().unwrap_or(0.0);
    std::thread::sleep(std::time::Duration::from_secs_f64(f.max(0.0)));
}"#,
        "rand" => r#"fn __sh_rand() -> String {
    String::from_utf8_lossy(&__sh_cap_bytes("echo $RANDOM", None)).trim().to_string()
}"#,
        "mtime" => r#"fn __sh_mtime(s: &str) -> i64 {
    std::fs::metadata(s).and_then(|m| m.modified()).map(|t| {
        t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    }).unwrap_or(0)
}"#,
        "fmode" => r#"fn __sh_fmode(s: &str, bits: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(s).map(|m| (m.mode() & bits) == bits).unwrap_or(false)
}"#,
        "fowner" => r#"fn __sh_fowner(s: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let uid = std::process::Command::new("id").arg("-u").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    std::fs::metadata(s).map(|m| m.uid().to_string() == uid).unwrap_or(false)
}"#,
        "fgroup" => r#"fn __sh_fgroup(s: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let gid = std::process::Command::new("id").arg("-g").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    std::fs::metadata(s).map(|m| m.gid().to_string() == gid).unwrap_or(false)
}"#,
        "fnewer" => r#"fn __sh_fnewer(s: &str) -> bool {
    // -N: modified after last read — approximate with mtime > atime
    std::fs::metadata(s).map(|m| {
        m.modified().and_then(|mt| m.accessed().map(|at| mt > at)).unwrap_or(false)
    }).unwrap_or(false)
}"#,
        "samefile" => r#"fn __sh_samefile(a: &str, b: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
        _ => false,
    }
}"#,
        "grepmatches" => r#"fn __sh_grepmatches(text: &str, pat: &str, flags: &str) -> (String, i32) {
    let mut opts = String::from("-o");
    if flags.contains('i') { opts.push('i'); }
    if flags.contains('E') { opts.push('E'); }
    let cmd = format!("printf '%s' \"$1\" | grep {opts} \"$2\"");
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(&cmd).arg("sh").arg(text).arg(pat);
    c.stdout(std::process::Stdio::piped());
    let mut ch = match c.spawn() { Ok(x) => x, Err(_) => return (String::new(), 1) };
    let mut out = Vec::new();
    if let Some(mut so) = ch.stdout.take() {
        let _ = so.read_to_end(&mut out);
    }
    let rc = ch.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    let mut s = String::from_utf8_lossy(&out).to_string();
    while s.ends_with('\n') { s.pop(); }
    (s, rc)
}"#,
        "regex" => r#"fn __sh_regex(text: &str, pat: &str, flags: &str) -> bool {
    let cmd = if flags.contains('i') {
        "shopt -s nocasematch; [[ $1 =~ $2 ]]"
    } else {
        "[[ $1 =~ $2 ]]"
    };
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(cmd).arg("sh").arg(text).arg(pat);
    c.status().map(|s| s.success()).unwrap_or(false)
}"#,
        "wait_all" => r#"fn __sh_wait_all() {
    let hs = std::mem::take(&mut *__SH_BG.lock().unwrap());
    for (_, mut h) in hs { let _ = h.wait(); }
}"#,
        "bg" => r#"fn __sh_bg(cmd: &str) {
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(cmd);
    match c.spawn() {
        Ok(ch) => {
            let pid = ch.id();
            __SH_BGPID.store(pid as i32, Ordering::SeqCst);
            __SH_BG.lock().unwrap().push((pid, ch));
        }
        Err(_) => {}
    }
}"#,
        "fexists" => r#"fn __sh_fexists(s: &str) -> bool { std::path::Path::new(s).exists() }"#,
        "fdir" => r#"fn __sh_fdir(s: &str) -> bool { std::path::Path::new(s).is_dir() }"#,
        "freg" => r#"fn __sh_freg(s: &str) -> bool { std::path::Path::new(s).is_file() }"#,
        "fsym" => r#"fn __sh_fsym(s: &str) -> bool {
    std::fs::symlink_metadata(s).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}"#,
        "fread" => r#"fn __sh_fread(s: &str) -> bool { std::fs::File::open(s).is_ok() }"#,
        "fwrite" => r#"fn __sh_fwrite(s: &str) -> bool {
    std::fs::OpenOptions::new().write(true).open(s).is_ok()
}"#,
        "fexec" => r#"fn __sh_fexec(s: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(s).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}"#,
        "fsize" => r#"fn __sh_fsize(s: &str) -> i64 {
    std::fs::metadata(s).map(|m| m.len() as i64).unwrap_or(0)
}"#,
        "aindex" => r#"fn __sh_aindex(v: &[String], k: i64) -> String {
    v.get(k as usize).cloned().unwrap_or_default()
}"#,
        _ => "fn __sh_unused() {}",
    }
}

// ── static helpers (renderer-side) ───────────────────────────────────


/// The SOURCE text of a word (for test-expression reconstruction):
/// Str → its value, all-literal Interpolate → the literal text.
fn word_source_text(e: &IrExpr) -> String {
    match e {
        IrExpr::Str(s, _) => s.clone(),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(x) => match x.as_ref() {
                        IrExpr::Call { func, args } if func == "getVar" => {
                            if let Some(IrExpr::Str(name, _)) = args.first() {
                                out.push_str(&format!("${name}"));
                            } else {
                                return String::new();
                            }
                        }
                        IrExpr::Call { func, args } if func == "param" => {
                            let op = str_arg(args, 0).unwrap_or("");
                            let name = str_arg(args, 1).unwrap_or("");
                            if name.is_empty() {
                                return String::new();
                            }
                            let rest: Vec<String> = args
                                .iter()
                                .skip(2)
                                .filter_map(|a| str_arg(&[(*a).clone()], 0).map(|s| s.to_string()))
                                .collect();
                            if rest.is_empty() {
                                out.push_str(&format!("${{{name}}}"));
                            } else if op.is_empty() {
                                out.push_str(&format!("${{{name}{}}}", rest.join(":")));
                            } else {
                                out.push_str(&format!("${{{name}{op}{}}}", rest.join(":")));
                            }
                        }
                        _ => return String::new(),
                    },
                }
            }
            out
        }
        IrExpr::Var(name, _) | IrExpr::Ident(name) => format!("${name}"),
        IrExpr::Call { func, args } if func == "getVar" => {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                format!("${name}")
            } else {
                String::new()
            }
        }
        IrExpr::Call { func, args } if func == "split" => {
            // `$var` unquoted — the split wrapper carries the read
            if let Some(inner) = args.first() {
                word_source_text(inner)
            } else {
                String::new()
            }
        }
        IrExpr::Call { func, args } if func == "param" => {
            // reconstruct `${name...}` — the test parser re-expands it
            let op = str_arg(args, 0).unwrap_or("");
            let name = str_arg(args, 1).unwrap_or("");
            if name.is_empty() {
                return String::new();
            }
            let rest: Vec<String> = args
                .iter()
                .skip(2)
                .filter_map(|a| str_arg(&[(*a).clone()], 0).map(|s| s.to_string()))
                .collect();
            if rest.is_empty() {
                format!("${{{name}}}")
            } else if op.is_empty() {
                format!("${{{name}{}}}", rest.join(":"))
            } else {
                format!("${{{name}{op}{}}}", rest.join(":"))
            }
        }
        _ => String::new(),
    }
}

/// Normalize an array/param name (`!map[@]`, `prefix*`, `arr[i]`,
/// `#arr`) to the bare var name.
fn array_base_name(name: &str) -> String {
    let mut n = name;
    if let Some(rest) = n.strip_prefix('!') {
        n = rest;
    }
    if let Some(rest) = n.strip_prefix('#') {
        n = rest;
    }
    if let Some(rest) = n.strip_suffix("[@]") {
        n = rest;
    } else if let Some(rest) = n.strip_suffix("[*]") {
        n = rest;
    }
    n.split('[')
        .next()
        .unwrap_or(n)
        .trim_end_matches('@')
        .trim_end_matches('*')
        .to_string()
}

fn str_arg(args: &[IrExpr], i: usize) -> Option<&str> {
    match args.get(i) {
        Some(IrExpr::Str(s, _)) => Some(s.as_str()),
        _ => None,
    }
}

/// The env Object on an exec call: args[2] may be Object([(k, v)]).
fn exec_env(args: &[IrExpr]) -> Option<Vec<(String, IrExpr)>> {
    match args.get(2) {
        Some(IrExpr::Object(props)) => Some(props.clone()),
        _ => None,
    }
}

fn env_ifs(env: &Option<Vec<(String, IrExpr)>>) -> Option<&str> {
    if let Some(props) = env {
        for (k, v) in props {
            if k == "IFS" {
                if let IrExpr::Str(s, _) = v {
                    return Some(s.as_str());
                }
            }
        }
    }
    None
}

/// The pipeline stages of a pipeline Call.
fn pipeline_stages(args: &[IrExpr]) -> Vec<Vec<IrStmt>> {
    let mut stages = Vec::new();
    if let Some(IrExpr::Array(items)) = args.first() {
        for it in items {
            if let IrExpr::Arrow(stmts) = it {
                stages.push(stmts.clone());
            }
        }
    }
    stages
}

/// Can this stage be reconstructed as bash command text?
fn stage_text_ok(stmts: &[IrStmt]) -> bool {
    match stmts {
        [] => true,
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "exec" => {
            let cmd = str_arg(args, 0).unwrap_or("");
            !is_native_cmd(cmd)
        }
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "redirect" => {
            let mut inner: Vec<IrStmt> = Vec::new();
            if let Some(IrExpr::Arrow(b)) = args.first() {
                inner = b.clone();
            }
            stage_text_ok(&inner)
        }
        [IrStmt::Redirect { inner, .. }] => stage_text_ok(inner),
        [IrStmt::Subshell(b)] => b.iter().all(|s| matches!(s, IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec")),
        [IrStmt::Expr(IrExpr::BinOp { op: BinOpKind::And | BinOpKind::Or, lhs, rhs })] => {
            exec_call_text_ok(lhs) && exec_call_text_ok(rhs)
        }
        [IrStmt::Block(b)] => stage_text_ok(b),
        _ => false,
    }
}

fn exec_call_text_ok(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args } if func == "exec" && !is_native_cmd(str_arg(args, 0).unwrap_or("")))
}

fn is_native_cmd(cmd: &str) -> bool {
    NATIVE_CMDS.contains(&cmd)
        || cmd == "break"
        || cmd == "continue"
        || cmd == "return"
        || cmd == "source"
        || cmd == "."
}

/// A single shell-out exec as command text (String expr).
fn single_exec_text(r: &mut Render, e: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = e {
        if func == "exec" {
            // the command word (args[0]) + the argument words (args[1])
            let mut words: Vec<&IrExpr> = Vec::new();
            if let Some(first) = args.first() {
                words.push(first);
            }
            if let Some(IrExpr::Array(items)) = args.get(1) {
                words.extend(items.iter());
            }
            let env = exec_env(args);
            return Some(r.cmd_text(&words, env.as_deref()));
        }
    }
    None
}

impl Render {
    /// Reconstruct a stage (or whole statement list) as bash command text.
    fn stage_text(&mut self, stmts: &[IrStmt]) -> Option<String> {
        match stmts {
            [] => Some(":".to_string()),
            [IrStmt::Expr(IrExpr::Call { func, args })] if func == "redirect" => {
                // the redirect CALL form (`cmd > file` as a stage)
                let mut inner: Vec<IrStmt> = Vec::new();
                let mut redirs: Vec<IrRedirectInfo> = Vec::new();
                if let Some(IrExpr::Arrow(b)) = args.first() {
                    inner = b.clone();
                }
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        if let Some(r) = self.redirect_info(it) {
                            redirs.push(r);
                        }
                    }
                }
                // a heredoc/herestring needs its content as stdin — route
                // through the native path (which passes it as input)
                if redirs.iter().any(|r| {
                    matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring")
                }) {
                    return None;
                }
                let mut full = self.stage_text(&inner)?;
                for r in &redirs {
                    let mode = r.mode.clone();
                    let te = self.expr_str(r.target.as_ref().unwrap_or(&IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted)));
                    match mode.as_str() {
                        "w" | "a" => {
                            // `2>&1` — a dup target
                            if let Some(t) = r.target.as_ref() {
                                if let IrExpr::Str(ts, _) = t {
                                    if let Some(rest) = ts.strip_prefix('&') {
                                        full = format!(
                                            "format!(\"{{}} {}&{{}}\", {full}, {})",
                                            if r.fd == 2 { "2>" } else { ">" },
                                            Self::rust_str(rest)
                                        );
                                        continue;
                                    }
                                }
                            }
                            let op = if mode == "w" { ">" } else { ">>" };
                            let fd = if r.fd == 2 { "2" } else { "" };
                            self.add_helper("q");
                            full = format!("format!(\"{{}} {fd}{op} {{}}\", {full}, __sh_q(&{te}))");
                        }
                        "r" => {
                            let fd = if r.fd == 2 { "2" } else { "" };
                            self.add_helper("q");
                            full = format!("format!(\"{{}} {fd}< {{}}\", {full}, __sh_q(&{te}))");
                        }
                        "process-in" => {
                            full = format!("format!(\"{{}} < <({{}})\", {full}, {te})");
                        }
                        _ => {
                            if let Some(t) = r.target.as_ref() {
                                if let IrExpr::Str(ts, _) = t {
                                    if let Some(rest) = ts.strip_prefix('&') {
                                        full = format!(
                                            "format!(\"{{}} {}&{{}}\", {full}, {})",
                                            if r.fd == 2 { "2>" } else { ">" },
                                            Self::rust_str(rest)
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Some(full)
            }
            [IrStmt::Expr(e)] => single_exec_text(self, e),
            [IrStmt::Redirect { inner, redirects }] => {
                // a heredoc/herestring stage needs its content as stdin —
                // route through the native path (which passes it as input)
                if redirects.iter().any(|r| {
                    matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring")
                }) {
                    return None;
                }
                let base = self.stage_text(inner)?;
                let mut full = base;
                for r in redirects {
                    let mode = r.mode.clone();
                    let te = self.expr_str(&r.target);
                    match mode.as_str() {
                        "w" | "a" => {
                            let op = if mode == "w" { ">" } else { ">>" };
                            let fd = if r.fd == Some(2) { "2" } else { "" };
                            self.add_helper("q");
                            full = format!("format!(\"{{}} {fd}{op} {{}}\", {full}, __sh_q(&{te}))");
                        }
                        "r" => {
                            let fd = if r.fd == Some(2) { "2" } else { "" };
                            self.add_helper("q");
                            full = format!("format!(\"{{}} {fd}< {{}}\", {full}, __sh_q(&{te}))");
                        }
                        "process-in" => {
                            full = format!("format!(\"{{}} < <({{}})\", {full}, {te})");
                        }
                        _ => {
                            if let IrExpr::Str(ts, _) = &r.target {
                                if let Some(rest) = ts.strip_prefix('&') {
                                    full = format!(
                                        "format!(\"{{}} {}{{}}\", {full}, {})",
                                        if r.fd == Some(2) { "2>" } else { ">" },
                                        Self::rust_str(rest)
                                    );
                                }
                            }
                        }
                    }
                }
                Some(full)
            }
            [IrStmt::Subshell(b)] => {
                let parts: Vec<String> = b
                    .iter()
                    .filter_map(|s| match s {
                        IrStmt::Expr(e) => single_exec_text(self, e),
                        _ => None,
                    })
                    .collect();
                if parts.len() == b.len() {
                    if parts.len() == 1 {
                        Some(format!("format!(\"({{}})\", {})", parts[0]))
                    } else {
                        Some(format!("format!(\"({{}}; {{}})\", {})", parts.join(", ")))
                    }
                } else {
                    None
                }
            }
            [IrStmt::Expr(IrExpr::BinOp { op, lhs, rhs })] => {
                let l = single_exec_text(self, lhs)?;
                let r = single_exec_text(self, rhs)?;
                let joiner = if matches!(op, BinOpKind::And) { "&&" } else { "||" };
                Some(format!("format!(\"{{}} {joiner} {{}}\", {l}, {r})"))
            }
            _ => None,
        }
    }
}

/// Does the statement list contain a shell-out (needs text reconstruction)?
fn contains_shell(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| match s {
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
            let cmd = str_arg(args, 0).unwrap_or("");
            !is_native_cmd(cmd)
        }
        IrStmt::Expr(IrExpr::Call { func, .. }) if func == "pipeline" => true,
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "redirect" => {
            let mut inner: Vec<IrStmt> = Vec::new();
            if let Some(IrExpr::Arrow(b)) = args.first() {
                inner = b.clone();
            }
            contains_shell(&inner)
        }
        IrStmt::Redirect { inner, .. } => contains_shell(inner),
        IrStmt::Expr(IrExpr::BinOp { lhs, rhs, .. }) => {
            expr_shell(lhs) || expr_shell(rhs)
        }
        IrStmt::Subshell(b) | IrStmt::Block(b) => contains_shell(b),
        _ => false,
    })
}

fn expr_shell(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } if func == "exec" => {
            !is_native_cmd(str_arg(args, 0).unwrap_or(""))
        }
        IrExpr::Call { func, .. } if func == "pipeline" => true,
        IrExpr::BinOp { lhs, rhs, .. } => expr_shell(lhs) || expr_shell(rhs),
        _ => false,
    }
}

// ── test parser ──────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
enum TTok {
    LParen,
    RParen,
    Not,
    And,
    Or,
    Unary(String),
    Bin(String),
    Operand(String),
}

/// An extglob token `@(a|b).js` — scan the balanced group plus any
/// trailing word chars as one operand.
fn extglob_tok(ch: &[char], i: &mut usize) -> TTok {
    let start = *i;
    let mut depth = 0;
    while *i < ch.len() {
        if ch[*i] == '(' { depth += 1; }
        else if ch[*i] == ')' { depth -= 1; if depth == 0 { *i += 1; break; } }
        *i += 1;
    }
    while *i < ch.len() {
        let c = ch[*i];
        if c == ' ' || c == '\t' || c == '\n' || c == '(' || c == ')' || c == '!'
            || c == '=' || c == '<' || c == '>' || c == '"' || c == '\'' || c == '$'
        {
            break;
        }
        *i += 1;
    }
    TTok::Operand(ch[start..*i].iter().collect())
}

fn test_tokens(s: &str) -> Option<Vec<TTok>> {
    let ch: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        match c {
            ' ' | '\t' | '\n' => i += 1,
            '(' => { out.push(TTok::LParen); i += 1; }
            ')' => { out.push(TTok::RParen); i += 1; }
            '!' => {
                if i + 1 < ch.len() && ch[i + 1] == '=' {
                    out.push(TTok::Bin("!=".to_string()));
                    i += 2;
                } else if i + 1 < ch.len() && ch[i + 1] == '(' {
                    out.push(extglob_tok(&ch, &mut i));
                } else {
                    out.push(TTok::Not);
                    i += 1;
                }
            }
            '@' | '?' | '*' | '+' => {
                // extglob `@(pat)` / `?(pat)` / `*(pat)` / `+(pat)` — an
                // operand (also `*` as a plain glob word)
                if i + 1 < ch.len() && ch[i + 1] == '(' {
                    out.push(extglob_tok(&ch, &mut i));
                } else {
                    let start = i;
                    while i < ch.len() {
                        let c2 = ch[i];
                        if c2 == ' ' || c2 == '\t' || c2 == '\n' || c2 == '(' || c2 == ')'
                            || c2 == '!' || c2 == '=' || c2 == '<' || c2 == '>' || c2 == '"'
                            || c2 == '\''
                        {
                            break;
                        }
                        // `$` breaks the word only for references; `$'…'`
                        // ANSI-C strings stay inside the word
                        if c2 == '$' {
                            let next = ch.get(i + 1).copied();
                            match next {
                                Some('{') | Some('(') => break,
                                Some(n) if n.is_alphanumeric() || n == '_' => break,
                                Some('\'') => {
                                    i += 2;
                                    while i < ch.len() && ch[i] != '\'' {
                                        if ch[i] == '\\' { i += 1; }
                                        i += 1;
                                    }
                                    i += 1;
                                    continue;
                                }
                                _ => {
                                    i += 1;
                                    continue;
                                }
                            }
                        }
                        i += 1;
                    }
                    out.push(TTok::Operand(ch[start..i].iter().collect()));
                }
            }
            '=' => {
                if i + 1 < ch.len() && ch[i + 1] == '=' {
                    out.push(TTok::Bin("==".to_string()));
                    i += 2;
                } else if i + 1 < ch.len() && ch[i + 1] == '~' {
                    out.push(TTok::Bin("=~".to_string()));
                    i += 2;
                } else {
                    out.push(TTok::Bin("=".to_string()));
                    i += 1;
                }
            }
            '<' | '>' => {
                if i + 1 < ch.len() && ch[i + 1] == '=' {
                    out.push(TTok::Bin(format!("{c}=")));
                    i += 2;
                } else {
                    out.push(TTok::Bin(c.to_string()));
                    i += 1;
                }
            }
            '"' | '\'' => {
                // quoted operand — includes the quotes (for interpolation);
                // skip `$(…)` groups and `$var` refs inside the quotes
                let start = i;
                i += 1;
                while i < ch.len() && ch[i] != c {
                    if ch[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if ch[i] == '$' && i + 1 < ch.len() && ch[i + 1] == '(' {
                        let mut depth = 0;
                        while i < ch.len() {
                            if ch[i] == '(' { depth += 1; }
                            else if ch[i] == ')' { depth -= 1; if depth == 0 { i += 1; break; } }
                            i += 1;
                        }
                        continue;
                    }
                    if ch[i] == '$' && i + 1 < ch.len()
                        && (ch[i + 1].is_alphanumeric() || ch[i + 1] == '_' || ch[i + 1] == '{')
                    {
                        i += 1;
                        while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_') {
                            i += 1;
                        }
                        continue;
                    }
                    i += 1;
                }
                if i >= ch.len() { return None; }
                i += 1;
                out.push(TTok::Operand(ch[start..i].iter().collect()));
            }
            '$' => {
                let start = i;
                if i + 1 < ch.len() && ch[i + 1] == '\'' {
                    // $'...' ANSI-C quoting — one operand
                    i += 2;
                    while i < ch.len() && ch[i] != '\'' {
                        if ch[i] == '\\' { i += 1; }
                        i += 1;
                    }
                    i += 1;
                    out.push(TTok::Operand(ch[start..i].iter().collect()));
                    continue;
                }
                if i + 1 < ch.len() && ch[i + 1] == '{' {
                    let mut depth = 0;
                    while i < ch.len() {
                        if ch[i] == '{' { depth += 1; }
                        else if ch[i] == '}' { depth -= 1; if depth == 0 { i += 1; break; } }
                        i += 1;
                    }
                } else if i + 1 < ch.len() && ch[i + 1] == '(' {
                    let mut depth = 0;
                    while i < ch.len() {
                        if ch[i] == '(' { depth += 1; }
                        else if ch[i] == ')' { depth -= 1; if depth == 0 { i += 1; break; } }
                        i += 1;
                    }
                } else {
                    i += 1;
                    while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_') {
                        i += 1;
                    }
                    if i == start + 1 && i < ch.len() && matches!(ch[i], '#' | '@' | '*' | '?' | '$' | '!') {
                        i += 1;
                    }
                    // word continuation: `$d/a`, `$x.txt` — one operand
                    while i < ch.len()
                        && !matches!(
                            ch[i],
                            ' ' | '\t' | '\n' | '(' | ')' | '!' | '=' | '<' | '>' | '"' | '\'' | '$'
                        )
                    {
                        i += 1;
                    }
                }
                out.push(TTok::Operand(ch[start..i].iter().collect()));
            }
            c if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/' || c == '+'
                || c == '*' || c == '?' || c == '[' || c == ']' || c == '^' || c == '{'
                || c == '}' || c == ':' || c == ',' || c == '#' || c == '@' || c == '%' => {
                let start = i;
                while i < ch.len() {
                    let c2 = ch[i];
                    if c2 == ' ' || c2 == '\t' || c2 == '\n' || c2 == '(' || c2 == ')'
                        || c2 == '!' || c2 == '=' || c2 == '<' || c2 == '>' || c2 == '"'
                        || c2 == '\''
                    {
                        break;
                    }
                    // `$` only breaks the word when it starts a reference
                    // ($var / ${…} / $(…)) — a trailing `$` (regex anchor)
                    // stays in the word
                    if c2 == '$' {
                        let next = ch.get(i + 1).copied();
                        match next {
                            Some('{') | Some('(') => break,
                            Some(n) if n.is_alphanumeric() || n == '_' => break,
                            Some('\'') => {
                                // $'...' ANSI-C string inside the word
                                i += 2;
                                while i < ch.len() && ch[i] != '\'' {
                                    if ch[i] == '\\' { i += 1; }
                                    i += 1;
                                }
                                i += 1;
                                continue;
                            }
                            _ => {
                                i += 1;
                                continue;
                            }
                        }
                    }
                    i += 1;
                }
                let word: String = ch[start..i].iter().collect();
                match word.as_str() {
                    "-a" => out.push(TTok::And),
                    "-o" => out.push(TTok::Or),
                    "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "-nt" | "-ot" | "-ef" => {
                        out.push(TTok::Bin(word));
                    }
                    "-f" | "-d" | "-e" | "-r" | "-w" | "-x" | "-s" | "-L" | "-h" | "-n"
                    | "-z" | "-t" | "-p" | "-S" | "-b" | "-c" | "-u" | "-g" | "-k" | "-N"
                    | "-O" | "-G" => {
                        out.push(TTok::Unary(word));
                    }
                    _ => out.push(TTok::Operand(word)),
                }
            }
            '\\' => {
                // `\(` / `\)` — escaped parens are grouping; `\>` a
                // literal-escaped operator (string compare)
                if i + 1 < ch.len() && ch[i + 1] == '(' {
                    out.push(TTok::LParen);
                    i += 2;
                } else if i + 1 < ch.len() && ch[i + 1] == ')' {
                    out.push(TTok::RParen);
                    i += 2;
                } else if i + 1 < ch.len() && matches!(ch[i + 1], '>' | '<') {
                    out.push(TTok::Bin(ch[i + 1].to_string()));
                    i += 2;
                } else {
                    let start = i;
                    while i < ch.len() && ch[i] != ' ' && ch[i] != '\t' && ch[i] != '\n' {
                        i += 1;
                    }
                    out.push(TTok::Operand(ch[start..i].iter().collect()));
                }
            }
            _ => {
                let start = i;
                while i < ch.len() && ch[i] != ' ' && ch[i] != '\t' && ch[i] != '\n' {
                    i += 1;
                }
                out.push(TTok::Operand(ch[start..i].iter().collect()));
            }
        }
    }
    Some(out)
}

struct TestParser<'a, 'r> {
    render: &'r mut Render,
    toks: &'a [TTok],
    pos: usize,
    style: &'a str,
}

impl<'a, 'r> TestParser<'a, 'r> {
    fn parse_or(&mut self) -> Option<String> {
        let mut lhs = self.parse_and()?;
        while self.peek_is(&TTok::Or) {
            self.pos += 1;
            let rhs = self.parse_and()?;
            lhs = format!("({lhs} || {rhs})");
        }
        Some(lhs)
    }

    fn parse_and(&mut self) -> Option<String> {
        let mut lhs = self.parse_not()?;
        while self.peek_is(&TTok::And) {
            self.pos += 1;
            let rhs = self.parse_not()?;
            lhs = format!("({lhs} && {rhs})");
        }
        Some(lhs)
    }

    fn parse_not(&mut self) -> Option<String> {
        if self.peek_is(&TTok::Not) {
            self.pos += 1;
            let e = self.parse_not()?;
            return Some(format!("(!{e})"));
        }
        if self.peek_is(&TTok::LParen) {
            self.pos += 1;
            let e = self.parse_or()?;
            if !self.peek_is(&TTok::RParen) {
                return None;
            }
            self.pos += 1;
            return Some(e);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<String> {
        let tok = self.toks.get(self.pos)?.clone();
        match tok {
            TTok::Unary(op) => {
                self.pos += 1;
                let (operand, _) = self.operand()?;
                Some(self.unary_test(&op, &operand))
            }
            TTok::Operand(_) => {
                let (lhs, lhs_num) = self.operand()?;
                if let Some(TTok::Bin(op)) = self.toks.get(self.pos).cloned() {
                    self.pos += 1;
                    let (rhs, rhs_num) = self.operand()?;
                    Some(self.binary_test(&op, &lhs, lhs_num, &rhs, rhs_num))
                } else {
                    // single operand — non-empty string
                    Some(format!("(!{lhs}.is_empty())"))
                }
            }
            _ => None,
        }
    }

    /// An operand token → (String expr, is_numeric).
    fn operand(&mut self) -> Option<(String, bool)> {
        let tok = self.toks.get(self.pos)?.clone();
        let TTok::Operand(text) = tok else { return None };
        self.pos += 1;
        Some(self.render.test_operand_expr(&text, self.style))
    }

    fn unary_test(&mut self, op: &str, operand: &str) -> String {
        match op {
            "-n" => format!("(!{operand}.is_empty())"),
            "-z" => format!("({operand}.is_empty())"),
            "-f" => {
                self.render.add_helper("freg");
                format!("__sh_freg(&{operand})")
            }
            "-d" => {
                self.render.add_helper("fdir");
                format!("__sh_fdir(&{operand})")
            }
            "-e" => {
                self.render.add_helper("fexists");
                format!("__sh_fexists(&{operand})")
            }
            "-r" => {
                self.render.add_helper("fread");
                format!("__sh_fread(&{operand})")
            }
            "-w" => {
                self.render.add_helper("fwrite");
                format!("__sh_fwrite(&{operand})")
            }
            "-x" => {
                self.render.add_helper("fexec");
                format!("__sh_fexec(&{operand})")
            }
            "-s" => {
                self.render.add_helper("fsize");
                format!("(__sh_fsize(&{operand}) > 0)")
            }
            "-L" | "-h" => {
                self.render.add_helper("fsym");
                format!("__sh_fsym(&{operand})")
            }
            "-p" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o010000)")
            }
            "-S" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o140000)")
            }
            "-b" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o060000)")
            }
            "-c" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o020000)")
            }
            "-u" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o004000)")
            }
            "-g" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o002000)")
            }
            "-k" => {
                self.render.add_helper("fmode");
                format!("__sh_fmode(&{operand}, 0o001000)")
            }
            "-O" => {
                self.render.add_helper("fowner");
                format!("__sh_fowner(&{operand})")
            }
            "-G" => {
                self.render.add_helper("fgroup");
                format!("__sh_fgroup(&{operand})")
            }
            "-N" => {
                self.render.add_helper("fnewer");
                format!("__sh_fnewer(&{operand})")
            }
            "-t" => {
                // `[ -t N ]` — a TTY test; the gate's fds are not TTYs
                "false".to_string()
            }
            _ => format!("(!{operand}.is_empty())"),
        }
    }

    fn binary_test(&mut self, op: &str, lhs: &str, lhs_num: bool, rhs: &str, rhs_num: bool) -> String {
        match op {
            "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                let rs = match op {
                    "-eq" => "==",
                    "-ne" => "!=",
                    "-lt" => "<",
                    "-le" => "<=",
                    "-gt" => ">",
                    _ => ">=",
                };
                let l = if lhs_num { lhs.to_string() } else { format!("{lhs}.trim().parse::<i64>().unwrap_or(0)") };
                let r = if rhs_num { rhs.to_string() } else { format!("{rhs}.trim().parse::<i64>().unwrap_or(0)") };
                format!("({l} {rs} {r})")
            }
            "=" | "==" | "!=" => {
                let eq = op != "!=";
                if self.style == "[[" {
                    // pattern match (glob) in [[ ]]
                    self.render.add_helper("fnmatch");
                    format!("({} __sh_fnmatch(&{rhs}, &{lhs}))", if eq { "" } else { "!" })
                } else {
                    let rs = if eq { "==" } else { "!=" };
                    let l = if lhs_num { format!("{lhs}.to_string()") } else { lhs.to_string() };
                    let r = if rhs_num { format!("{rhs}.to_string()") } else { rhs.to_string() };
                    format!("({l} {rs} {r})")
                }
            }
            "=~" => {
                self.render.add_helper("regex");
                format!("__sh_regex(&{lhs}, &{rhs}, \"\")")
            }
            "-nt" | "-ot" => {
                self.render.add_helper("mtime");
                let rs = if op == "-nt" { ">" } else { "<" };
                format!("(__sh_mtime(&{lhs}) {rs} __sh_mtime(&{rhs}))")
            }
            "-ef" => {
                self.render.add_helper("samefile");
                format!("__sh_samefile(&{lhs}, &{rhs})")
            }
            "<" | ">" => {
                format!("({lhs} {op} {rhs})")
            }
            _ => format!("(!{lhs}.is_empty())"),
        }
    }

    fn peek_is(&self, t: &TTok) -> bool {
        self.toks.get(self.pos).map(|x| x == t).unwrap_or(false)
    }
}

impl Render {
    /// Resolve one test operand's source text (`"$x"`, `$x`, `"a$x"`,
    /// numbers, `$(cmd)`, `${x:-d}`, bare words) → (String expr, is_num).
    fn test_operand_expr(&mut self, text: &str, _style: &str) -> (String, bool) {
        let t = text.trim();
        if t.is_empty() {
            return ("String::new()".to_string(), false);
        }
        let quoted = (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
            || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2);
        let inner = if quoted { &t[1..t.len() - 1] } else { t };
        // pure `$var`?
        if !quoted {
            if let Some(rest) = inner.strip_prefix('$') {
                let name = rest
                    .strip_prefix('{')
                    .and_then(|s| s.strip_suffix('}'))
                    .unwrap_or(rest);
                let bare = !name.is_empty()
                    && !name.contains('$')
                    && !name.contains(':')
                    && !name.contains('(')
                    && !name.contains('/')
                    && !name.contains('[')
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_');
                if bare {
                    if self.is_num(name) {
                        return (self.getvar_num(name), true);
                    }
                    return (self.getvar_str(name), false);
                }
            }
            if let Ok(n) = t.parse::<i64>() {
                return (n.to_string(), true);
            }
        }
        (self.dollar_interp(inner), false)
    }

    /// Interpolate a test-operand text: `$var`, `${...}`, `$(...)` → a
    /// format!()-style String expr; literal text with braces escaped.
    fn dollar_interp(&mut self, text: &str) -> String {
        // `$'...'` — ANSI-C quoting (NUL/control chars in patterns)
        if let Some(rest) = text.strip_prefix("$'") {
            if let Some(end) = rest.find('\'') {
                let body = &rest[..end];
                let unescaped = unescape_ansi_c(body);
                return Self::rust_str_expr(&unescaped);
            }
        }
        let ch: Vec<char> = text.chars().collect();
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        let mut i = 0;
        while i < ch.len() {
            if ch[i] == '$' && i + 1 < ch.len() {
                if matches!(ch[i + 1], '?' | '#' | '@' | '*' | '$' | '!')
                    || ch[i + 1].is_ascii_digit()
                {
                    // special vars ($? $# $@ $$ $!) and positionals
                    let name: String = ch[i + 1..i + 2].iter().collect();
                    fmt.push_str("{}");
                    args.push(self.getvar_str(&name));
                    i += 2;
                    continue;
                }
                if ch[i + 1] == '\'' {
                    // $'...' ANSI-C string inside the text
                    let mut j = i + 2;
                    while j < ch.len() && ch[j] != '\'' {
                        if ch[j] == '\\' { j += 1; }
                        j += 1;
                    }
                    if j < ch.len() {
                        let body: String = ch[i + 2..j].iter().collect();
                        let unescaped = unescape_ansi_c(&body);
                        fmt.push_str("{}");
                        args.push(Self::rust_str_expr(&unescaped));
                        i = j + 1;
                        continue;
                    }
                }
                if ch[i + 1] == '{' {
                    let start = i + 2;
                    let mut depth = 1;
                    let mut j = start;
                    while j < ch.len() && depth > 0 {
                        if ch[j] == '{' { depth += 1; }
                        else if ch[j] == '}' { depth -= 1; if depth == 0 { break; } }
                        j += 1;
                    }
                    if depth == 0 {
                        let body: String = ch[start..j].iter().collect();
                        fmt.push_str("{}");
                        args.push(self.param_text_str(&body));
                        i = j + 1;
                        continue;
                    }
                } else if ch[i + 1] == '(' {
                    // `$(( arith ))` — arithmetic expansion
                    if i + 2 < ch.len() && ch[i + 2] == '(' {
                        let start = i + 3;
                        // the two openers are consumed — find the first
                        // closer at depth 0 (the body has none of its own)
                        let mut depth = 0;
                        let mut j = start;
                        while j < ch.len() {
                            if ch[j] == '(' { depth += 1; }
                            else if ch[j] == ')' {
                                if depth == 0 { break; }
                                depth -= 1;
                            }
                            j += 1;
                        }
                        if j < ch.len() {
                            let body: String = ch[start..j].iter().collect();
                            fmt.push_str("{}");
                            if let Some(e) = self.arith_text(&body) {
                                args.push(format!("({e}).to_string()"));
                            } else {
                                args.push("String::new()".to_string());
                            }
                            i = j + 2;
                            continue;
                        }
                    }
                    let start = i + 2;
                    let mut depth = 1;
                    let mut j = start;
                    while j < ch.len() && depth > 0 {
                        if ch[j] == '(' { depth += 1; }
                        else if ch[j] == ')' { depth -= 1; if depth == 0 { break; } }
                        j += 1;
                    }
                    if depth == 0 {
                        let body: String = ch[start..j].iter().collect();
                        fmt.push_str("{}");
                        self.add_helper("capture_rc");
                        args.push(format!("__sh_capture_rc(&{}).0", Self::rust_str(&body)));
                        i = j + 1;
                        continue;
                    }
                } else {
                    let start = i + 1;
                    let mut j = start;
                    while j < ch.len() && (ch[j].is_alphanumeric() || ch[j] == '_') {
                        j += 1;
                    }
                    if j > start {
                        let name: String = ch[start..j].iter().collect();
                        fmt.push_str("{}");
                        args.push(self.getvar_str(&name));
                        i = j;
                        continue;
                    }
                }
            }
            fmt.push(ch[i]);
            i += 1;
        }
        if args.is_empty() {
            Self::rust_str_expr(text)
        } else {
            format!("format!({}, {})", Self::rust_str(&fmt), args.join(", "))
        }
    }

    /// `${name:op:...}` source text → param str expr.
    fn param_text_str(&mut self, body: &str) -> String {
        // split off the leading name
        let ch: Vec<char> = body.chars().collect();
        let mut i = 0;
        while i < ch.len() && ch[i] != ':' && ch[i] != '/' && ch[i] != '#' && ch[i] != '%' {
            i += 1;
        }
        let name: String = ch[..i].iter().collect();
        let rest: String = ch[i..].iter().collect();
        if rest.is_empty() {
            return self.getvar_str(&name);
        }
        // split the operator off the rest
        let (op, arg): (String, String) = if rest.starts_with(":-") {
            (":-".into(), rest[2..].to_string())
        } else if rest.starts_with(":=") {
            (":=".into(), rest[2..].to_string())
        } else if rest.starts_with(":?") {
            (":?".into(), rest[2..].to_string())
        } else if rest.starts_with(":+") {
            (":+".into(), rest[2..].to_string())
        } else if rest.starts_with('+') {
            ("+".into(), rest[1..].to_string())
        } else if rest.starts_with("##") {
            ("##".into(), rest[2..].to_string())
        } else if rest.starts_with("%%") {
            ("%%".into(), rest[2..].to_string())
        } else if rest.starts_with("//") {
            ("//".into(), rest[2..].to_string())
        } else if rest.starts_with("#") || rest.starts_with("%") || rest.starts_with("/") {
            (rest[..1].to_string(), rest[1..].to_string())
        } else if rest.starts_with(":") {
            // ${x:off} / ${x:off:len} — a slice
            let rest = &rest[1..];
            let mut parts = rest.splitn(2, ':');
            let off = parts.next().unwrap_or("");
            let len = parts.next().unwrap_or("");
            let mut args: Vec<IrExpr> = vec![
                IrExpr::Str("slice".to_string(), crate::ir::StrStyle::DoubleQuoted),
                IrExpr::Str(name.clone(), crate::ir::StrStyle::DoubleQuoted),
            ];
            if !off.is_empty() {
                args.push(IrExpr::Str(off.to_string(), crate::ir::StrStyle::DoubleQuoted));
            }
            if !len.is_empty() {
                args.push(IrExpr::Str(len.to_string(), crate::ir::StrStyle::DoubleQuoted));
            }
            return self.param_str(&args);
        } else {
            (rest, String::new())
        };
        let mut args: Vec<IrExpr> = vec![
            IrExpr::Str(op, crate::ir::StrStyle::DoubleQuoted),
            IrExpr::Str(name.clone(), crate::ir::StrStyle::DoubleQuoted),
        ];
        if !arg.is_empty() {
            args.push(IrExpr::Str(arg, crate::ir::StrStyle::DoubleQuoted));
        }
        self.param_str(&args)
    }
}

// ── arithmetic text parser (let) ─────────────────────────────────────

fn arith_tokens(t: &str) -> Option<Vec<String>> {
    let ch: Vec<char> = t.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if c == ' ' || c == '\t' || c == '\n' {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            if c == '0' && i + 1 < ch.len() && (ch[i + 1] == 'x' || ch[i + 1] == 'X') {
                i += 2;
                while i < ch.len() && ch[i].is_ascii_hexdigit() { i += 1; }
            } else {
                while i < ch.len() && ch[i].is_ascii_digit() { i += 1; }
                // base-N literal `10#x`
                if i < ch.len() && ch[i] == '#' {
                    i += 1;
                    while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_') { i += 1; }
                }
            }
            out.push(ch[start..i].iter().collect());
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_') { i += 1; }
            out.push(ch[start..i].iter().collect());
            continue;
        }
        if c == '$' {
            i += 1;
            if i < ch.len() && ch[i] == '{' {
                i += 1;
                let start = i;
                while i < ch.len() && ch[i] != '}' { i += 1; }
                let name: String = ch[start..i].iter().collect();
                out.push(format!("${{{name}}}"));
                i += 1;
            } else if i < ch.len() && ch[i] == '(' {
                // $(cmd) — command substitution inside arithmetic
                let start = i;
                let mut depth = 1;
                while i < ch.len() && depth > 0 {
                    if ch[i] == '(' { depth += 1; }
                    else if ch[i] == ')' { depth -= 1; if depth == 0 { break; } }
                    i += 1;
                }
                if depth == 0 {
                    i += 1;
                }
                out.push(format!("$({})", ch[start..i].iter().collect::<String>()));
            } else {
                let start = i;
                while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_') { i += 1; }
                out.push(format!("${}", ch[start..i].iter().collect::<String>()));
            }
            continue;
        }
        // operators (longest match)
        let rest: String = ch[i..].iter().collect();
        let mut matched = false;
        for op in ["<<=", ">>=", "++", "--", "**", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "=", "+", "-", "*", "/", "%", "<", ">", "&", "|", "^", "!", "~", "(", ")", "[", "]", "?", ":"] {
            if rest.starts_with(op) {
                out.push(op.to_string());
                i += op.len();
                matched = true;
                break;
            }
        }
        if !matched {
            return None;
        }
    }
    Some(out)
}

struct ArithParser<'a, 'r> {
    render: &'r mut Render,
    toks: &'a [String],
    pos: usize,
}

impl<'a, 'r> ArithParser<'a, 'r> {
    fn peek(&self) -> Option<String> {
        self.toks.get(self.pos).cloned()
    }

    fn parse_ternary(&mut self) -> Option<String> {
        let c = self.parse_or()?;
        if self.peek().as_deref() == Some("?") {
            self.pos += 1;
            let t = self.parse_ternary()?;
            if self.peek().as_deref() != Some(":") {
                return None;
            }
            self.pos += 1;
            let e = self.parse_ternary()?;
            return Some(format!("(if ({c} != 0) {{ {t} }} else {{ {e} }})"));
        }
        Some(c)
    }

    fn parse_or(&mut self) -> Option<String> {
        let mut l = self.parse_and()?;
        while self.peek().as_deref() == Some("||") {
            self.pos += 1;
            let r = self.parse_and()?;
            l = format!("(({l} != 0 || {r} != 0) as i64)");
        }
        Some(l)
    }

    fn parse_and(&mut self) -> Option<String> {
        let mut l = self.parse_bitor()?;
        while self.peek().as_deref() == Some("&&") {
            self.pos += 1;
            let r = self.parse_bitor()?;
            l = format!("(({l} != 0 && {r} != 0) as i64)");
        }
        Some(l)
    }

    fn parse_bitor(&mut self) -> Option<String> {
        let mut l = self.parse_bitxor()?;
        while self.peek().as_deref() == Some("|") {
            self.pos += 1;
            let r = self.parse_bitxor()?;
            l = format!("({l} | {r})");
        }
        Some(l)
    }

    fn parse_bitxor(&mut self) -> Option<String> {
        let mut l = self.parse_bitand()?;
        while self.peek().as_deref() == Some("^") {
            self.pos += 1;
            let r = self.parse_bitand()?;
            l = format!("({l} ^ {r})");
        }
        Some(l)
    }

    fn parse_bitand(&mut self) -> Option<String> {
        let mut l = self.parse_eq()?;
        while self.peek().as_deref() == Some("&") {
            self.pos += 1;
            let r = self.parse_eq()?;
            l = format!("({l} & {r})");
        }
        Some(l)
    }

    fn parse_eq(&mut self) -> Option<String> {
        let mut l = self.parse_rel()?;
        while let Some(op) = self.peek() {
            if op == "==" || op == "!=" {
                self.pos += 1;
                let r = self.parse_rel()?;
                l = format!("(({l} {op} {r}) as i64)");
            } else {
                break;
            }
        }
        Some(l)
    }

    fn parse_rel(&mut self) -> Option<String> {
        let mut l = self.parse_shift()?;
        while let Some(op) = self.peek() {
            if op == "<" || op == ">" || op == "<=" || op == ">=" {
                self.pos += 1;
                let r = self.parse_shift()?;
                l = format!("(({l} {op} {r}) as i64)");
            } else {
                break;
            }
        }
        Some(l)
    }

    fn parse_shift(&mut self) -> Option<String> {
        let mut l = self.parse_add()?;
        while let Some(op) = self.peek() {
            if op == "<<" || op == ">>" {
                self.pos += 1;
                let r = self.parse_add()?;
                l = format!("({l} {op} {r})");
            } else {
                break;
            }
        }
        Some(l)
    }

    fn parse_add(&mut self) -> Option<String> {
        let mut l = self.parse_mul()?;
        while let Some(op) = self.peek() {
            if op == "+" || op == "-" {
                self.pos += 1;
                let r = self.parse_mul()?;
                l = format!("({l} {op} {r})");
            } else {
                break;
            }
        }
        Some(l)
    }

    fn parse_mul(&mut self) -> Option<String> {
        let mut l = self.parse_unary()?;
        while let Some(op) = self.peek() {
            if op == "*" || op == "/" || op == "%" {
                self.pos += 1;
                let r = self.parse_unary()?;
                l = format!("({l} {op} {r})");
            } else {
                break;
            }
        }
        Some(l)
    }

    fn parse_unary(&mut self) -> Option<String> {
        match self.peek().as_deref() {
            Some("!") => {
                self.pos += 1;
                let a = self.parse_unary()?;
                Some(format!("(({a} == 0) as i64)"))
            }
            Some("~") => {
                self.pos += 1;
                let a = self.parse_unary()?;
                Some(format!("(!{a})"))
            }
            Some("-") => {
                self.pos += 1;
                let a = self.parse_unary()?;
                Some(format!("(-{a})"))
            }
            Some("+") => {
                self.pos += 1;
                let a = self.parse_unary()?;
                Some(a)
            }
            Some("++") => {
                self.pos += 1;
                let n = self.parse_primary()?;
                if let Some(name) = self.render.last_arith_var.clone() {
                    Some(self.render.incdec(&name, 1, true))
                } else {
                    Some(n)
                }
            }
            Some("--") => {
                self.pos += 1;
                let n = self.parse_primary()?;
                if let Some(name) = self.render.last_arith_var.clone() {
                    Some(self.render.incdec(&name, -1, true))
                } else {
                    Some(n)
                }
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<String> {
        match self.peek().as_deref() {
            Some("(") => {
                self.pos += 1;
                let e = self.parse_ternary()?;
                if self.peek().as_deref() != Some(")") {
                    return None;
                }
                self.pos += 1;
                Some(e)
            }
            Some(t) if t.chars().all(|c| c.is_ascii_digit()) => {
                let v = t.to_string();
                self.pos += 1;
                Some(v)
            }
            Some(t) if t.starts_with("0x") || t.starts_with("0X") => {
                let v = t.to_string();
                self.pos += 1;
                Some(format!("0x{}", &v[2..]))
            }
            Some(t) if t.starts_with("$(") => {
                // $(cmd) — command substitution as a number
                let inner = t
                    .strip_prefix("$(")
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or(t);
                self.pos += 1;
                self.render.add_helper("capture_rc");
                Some(format!(
                    "__sh_capture_rc(&{}).0.trim().parse::<i64>().unwrap_or(0)",
                    Render::rust_str(inner)
                ))
            }
            Some(t) if t.starts_with("${#") => {
                // `${#arr[@]}` — array length
                let inner = t.trim_start_matches("${#").trim_end_matches('}');
                let inner = inner
                    .strip_suffix("[@]")
                    .or_else(|| inner.strip_suffix("[*]"))
                    .unwrap_or(inner);
                self.pos += 1;
                if self.render.is_assoc(inner) {
                    let m = self.render.tls(inner);
                    Some(format!("{m}.with(|v| v.borrow().len() as i64)"))
                } else if self.render.is_array(inner) {
                    Some(self.render.array_len(inner))
                } else {
                    self.render.add_helper("len");
                    let v = self.render.getvar_str(inner);
                    Some(format!("__sh_len(&{v})"))
                }
            }
            Some(t) if t.starts_with('$') && t.len() > 1 && t[1..].chars().all(|c| c.is_ascii_digit()) => {
                // $1, $2 … positional params in arithmetic
                let name = t[1..].to_string();
                self.pos += 1;
                Some(self.render.getvar_num(&name))
            }
            Some(t) if t.contains('#') && !t.starts_with("${#") && !t.starts_with("0x") => {
                // base-N literal `10#x` — the value of x parsed in base N
                let (base, rest) = t.split_once('#').unwrap();
                let (name, _) = rest.split_once(']').unwrap_or((rest, ""));
                let b: u32 = base.trim().parse().unwrap_or(10);
                self.pos += 1;
                let v = self.render.getvar_str(name);
                Some(format!("i64::from_str_radix(&{v}.trim(), {b}).unwrap_or(0)"))
            }
            Some(t) => {
                // var or $var (possibly array-indexed `name[i]`)
                let name = t.strip_prefix('$').unwrap_or(t);
                if !is_ident(name) {
                    return None;
                }
                self.pos += 1;
                self.render.last_arith_var = Some(name.to_string());
                // array index `name[expr]`
                if self.peek().as_deref() == Some("[") {
                    self.pos += 1;
                    let k = self.parse_ternary()?;
                    if self.peek().as_deref() != Some("]") {
                        return None;
                    }
                    self.pos += 1;
                    // chain further indices (`a[i][j]`) — consume them
                    while self.peek().as_deref() == Some("[") {
                        self.pos += 1;
                        let _ = self.parse_ternary()?;
                        if self.peek().as_deref() != Some("]") {
                            return None;
                        }
                        self.pos += 1;
                    }
                    let e = self.render.array_elem(name, &k);
                    return Some(format!("{e}.trim().parse::<i64>().unwrap_or(0)"));
                }
                let v = self.render.getvar_num(name);
                // postfix ++/--
                match self.peek().as_deref() {
                    Some("++") => {
                        self.pos += 1;
                        Some(self.render.incdec(name, 1, false))
                    }
                    Some("--") => {
                        self.pos += 1;
                        Some(self.render.incdec(name, -1, false))
                    }
                    _ => Some(v),
                }
            }
            None => None,
        }
    }
}

// ── misc renderer-side helpers ───────────────────────────────────────

/// `$'...'` ANSI-C escapes: \n \t \\ \xHH \0NNN.
fn unescape_ansi_c(s: &str) -> String {
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '\\' && i + 1 < ch.len() {
            i += 1;
            match ch[i] {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'a' => out.push('\x07'),
                'b' => out.push('\x08'),
                'f' => out.push('\x0c'),
                'v' => out.push('\x0b'),
                'e' => out.push('\x1b'),
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                'x' => {
                    let mut n = 0;
                    let mut k = 0;
                    while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                        n = n * 16 + ch[i + 1 + k].to_digit(16).unwrap();
                        k += 1;
                    }
                    if k > 0 {
                        out.push(char::from_u32(n).unwrap_or('\0'));
                        i += k;
                    } else {
                        out.push('x');
                    }
                }
                c => {
                    out.push('\\');
                    out.push(c);
                }
            }
            i += 1;
        } else {
            out.push(ch[i]);
            i += 1;
        }
    }
    out
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && (s.chars().next().unwrap().is_alphabetic() || s.chars().next().unwrap() == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn split_ident_rest(t: &str) -> Option<(&str, &str)> {
    let mut i = 0;
    for c in t.chars() {
        if c.is_alphanumeric() || c == '_' {
            i += 1;
        } else {
            break;
        }
    }
    if i == 0 {
        return Some(("", t));
    }
    Some((&t[..i], &t[i..]))
}

// ── collection passes ────────────────────────────────────────────────

/// Collect every variable written by statements (assign targets, declare
/// lists, For loop vars, read targets, array calls) — the hoisted set.
fn collect_written(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    let var = t.var.split('[').next().unwrap_or(&t.var).to_string();
                    out.insert(var);
                }
                collect_written_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_written_expr(e, out);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                out.insert(var.clone());
                for e in elements {
                    collect_written_expr(e, out);
                }
            }
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone());
                collect_written_expr(iter, out);
                collect_written(body, out);
            }
            IrStmt::Expr(e) => collect_written_expr(e, out),
            IrStmt::Output { value, .. } => collect_written_expr(value, out),
            IrStmt::WriteFile { path, content, .. } => {
                collect_written_expr(path, out);
                collect_written_expr(content, out);
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_written_expr(cond, out);
                collect_written(then, out);
                for (c, b) in elsifs {
                    collect_written_expr(c, out);
                    collect_written(b, out);
                }
                collect_written(else_, out);
            }
            IrStmt::While { cond, body } => {
                collect_written_expr(cond, out);
                collect_written(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_written(body, out);
                collect_written_expr(cond, out);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_written_expr(x, out);
                }
            }
            IrStmt::Return(e) => {
                if let Some(x) = e {
                    collect_written_expr(x, out);
                }
            }
            IrStmt::ForInit { init, cond, step, body } => {
                collect_written(init, out);
                collect_written_expr(cond, out);
                collect_written(step, out);
                collect_written(body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_written(b, out);
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_written(inner, out);
                for r in redirects {
                    collect_written_expr(&r.target, out);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                collect_written_expr(discriminant, out);
                for c in clauses {
                    collect_written(&c.body, out);
                }
            }
            IrStmt::Function { body, .. } => collect_written(body, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_written(st, out);
                }
            }
            IrStmt::Exec { cmd, args, env, redirects, .. } => {
                collect_written_expr(cmd, out);
                for a in args {
                    collect_written_expr(a, out);
                }
                for (_, v) in env {
                    collect_written_expr(v, out);
                }
                for r in redirects {
                    collect_written_expr(r, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_written_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_written_expr(lhs, out);
            collect_written_expr(rhs, out);
        }
        IrExpr::Arith(a) => collect_written_arith(a, out),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_written_expr(x, out);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_written_expr(i, out);
            }
        }
        IrExpr::Call { func, args } => {
            // `assign(name, op, val)` — the arith-assignment writes name
            if func == "assign" {
                if let Some(name) = str_arg(args, 0) {
                    out.insert(name.to_string());
                }
            }
            // `${x:=default}` — the assignment writes x
            if func == "param" {
                let op = str_arg(args, 0).unwrap_or("");
                if op == "=" || op == ":=" {
                    if let Some(name) = str_arg(args, 1) {
                        if !name.is_empty()
                            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            out.insert(name.to_string());
                        }
                    }
                }
            }
            // exec builtins that WRITE vars: read targets, let targets,
            // declaration word-assigns, unset — the hoist must know them
            if func == "exec" {
                if let Some(cmd) = str_arg(args, 0) {
                        let words: Vec<&IrExpr> = match args.get(1) {
                            Some(IrExpr::Array(items)) => items.iter().collect(),
                            _ => vec![],
                        };
                        if cmd == "read" {
                            for w in &words {
                                if let Some(n) = str_arg(&[(*w).clone()], 0) {
                                    if !n.starts_with('-') && !n.is_empty() {
                                        out.insert(n.to_string());
                                    }
                                }
                            }
                        } else if cmd == "eval" {
                            // `eval "y=$x+1"` — the native-assign target
                            for w in &words {
                                let t = word_source_text(w);
                                if !t.is_empty() {
                                    if let Some((name, _)) = t.trim().split_once('=') {
                                        if !name.is_empty()
                                            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                                        {
                                            out.insert(name.to_string());
                                        }
                                    }
                                }
                            }
                        } else if cmd == "printf" {
                            // `printf -v name ...` — the target is written
                            if let Some(IrExpr::Str(f, _)) = words.first() {
                                if f == "-v" {
                                    if let Some(n) = words.get(1).and_then(|w| {
                                        str_arg(&[(*w).clone()], 0).map(|s| s.to_string())
                                    }) {
                                        out.insert(n);
                                    }
                                }
                            }
                        } else if cmd == "let" {
                            for w in &words {
                                if let Some(t) = str_arg(&[(*w).clone()], 0) {
                                    collect_array_names_from_text(t, out);
                                    if let Some((name, rest)) = split_ident_rest(t.trim()) {
                                        let rest = rest.trim_start();
                                        if !name.is_empty()
                                            && (rest.starts_with('=') || rest.starts_with("+=")
                                                || rest.starts_with("-=")
                                                || rest.starts_with("*=")
                                                || rest.starts_with("/=")
                                                || rest.starts_with("%=")
                                                || rest.starts_with("++")
                                                || rest.starts_with("--"))
                                        {
                                            out.insert(name.to_string());
                                        }
                                    }
                                }
                            }
                        } else if matches!(cmd, "export" | "local" | "declare" | "typeset" | "readonly" | "unset") {
                            for w in &words {
                                if let Some(n) = str_arg(&[(*w).clone()], 0) {
                                    if n.starts_with('-') {
                                        continue;
                                    }
                                    if n.contains('[') && n.ends_with(']') {
                                        out.insert(n.split('[').next().unwrap_or(n).to_string());
                                    } else if let Some((name, _)) = n.split_once('=') {
                                        if !name.is_empty() {
                                            out.insert(name.to_string());
                                        }
                                    } else if !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                        out.insert(n.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            for a in args {
                collect_written_expr(a, out);
            }
        }
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            collect_written_expr(key, out);
        }
        IrExpr::Capture { expr, .. } => {
            if let IrExpr::Arrow(stmts) = expr.as_ref() {
                collect_written(stmts, out);
            } else {
                collect_written_expr(expr, out);
            }
        }
        IrExpr::Arrow(stmts) => collect_written(stmts, out),
        IrExpr::Ternary { cond, then, else_ } => {
            collect_written_expr(cond, out);
            collect_written_expr(then, out);
            collect_written_expr(else_, out);
        }
        _ => {}
    }
}

fn collect_written_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_written_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_written_arith(lhs, out);
            collect_written_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_written_arith(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            collect_written_arith(test, out);
            collect_written_arith(then, out);
            collect_written_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            collect_written_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
}

/// Collect array/assoc vars from setArray / DeclareArray / declare -A /
/// arrayIndex / listVar / param array forms.
fn collect_arrays(stmts: &[IrStmt], arrays: &mut BTreeSet<String>, assoc: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Expr(e) => collect_arrays_expr(e, arrays, assoc),
            IrStmt::Output { value, .. } => collect_arrays_expr(value, arrays, assoc),
            IrStmt::WriteFile { path, content, .. } => {
                collect_arrays_expr(path, arrays, assoc);
                collect_arrays_expr(content, arrays, assoc);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_arrays_expr(x, arrays, assoc);
                }
            }
            IrStmt::Expr(IrExpr::Call { func, args }) => match func.as_str() {
                "setArray" | "setArrayAppend" => {
                    if let Some(name) = str_arg(args, 0) {
                        arrays.insert(name.to_string());
                    }
                }
                "listVar" => {
                    if let Some(name) = str_arg(args, 0) {
                        if name != "@" && name != "*" {
                            arrays.insert(name.to_string());
                        }
                    }
                }
                "arrayIndex" => {
                    if let Some(name) = str_arg(args, 0) {
                        arrays.insert(name.to_string());
                    }
                }
                "exec" => {
                    // `(( array[i] ... ))` — array refs inside let texts
                    if let Some(cmd) = str_arg(args, 0) {
                        if cmd == "let" {
                            if let Some(IrExpr::Array(words)) = args.get(1) {
                                for w in words {
                                    if let Some(t) = str_arg(&[(*w).clone()], 0) {
                                        let mut tmp = BTreeSet::new();
                                        collect_array_names_from_text(t, &mut tmp);
                                        arrays.extend(tmp);
                                    }
                                }
                            }
                        }
                    }
                    // `declare -A map` / `local -A map` / `declare -a arr`
                    if let Some(IrExpr::Array(words)) = args.get(1) {
                        let mut flags: Vec<String> = Vec::new();
                        let mut names: Vec<String> = Vec::new();
                        for w in words {
                            if let Some(t) = str_arg(&[(*w).clone()], 0) {
                                if t.starts_with('-') {
                                    flags.push(t.to_string());
                                } else {
                                    names.push(t.to_string());
                                }
                            }
                        }
                        for n in names {
                            if flags.iter().any(|f| f == "-A" || f == "-aA") {
                                assoc.insert(n);
                            } else if flags.iter().any(|f| f == "-a") {
                                arrays.insert(n);
                            }
                        }
                    }
                }
                _ => {}
            },
            IrStmt::DeclareArray { var, .. } => {
                arrays.insert(var.clone());
            }
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    if let Some(open) = t.var.find('[') {
                        if t.var.ends_with(']') {
                            let var = t.var[..open].to_string();
                            // an index-array write (the assoc set, built
                            // from `declare -A`, decides the map flavor)
                            arrays.insert(var);
                        }
                    }
                }
                collect_arrays_expr(expr, arrays, assoc);
            }
            IrStmt::For { iter, body, .. } => {
                collect_arrays_expr(iter, arrays, assoc);
                collect_arrays(body, arrays, assoc);
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_arrays_expr(cond, arrays, assoc);
                collect_arrays(then, arrays, assoc);
                for (c, b) in elsifs {
                    collect_arrays_expr(c, arrays, assoc);
                    collect_arrays(b, arrays, assoc);
                }
                collect_arrays(else_, arrays, assoc);
            }
            IrStmt::While { cond, body } => {
                collect_arrays_expr(cond, arrays, assoc);
                collect_arrays(body, arrays, assoc);
            }
            IrStmt::ForInit { init, cond, step, body } => {
                collect_arrays(init, arrays, assoc);
                collect_arrays_expr(cond, arrays, assoc);
                collect_arrays(step, arrays, assoc);
                collect_arrays(body, arrays, assoc);
            }
            IrStmt::DoWhile { body, .. } => collect_arrays(body, arrays, assoc),
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_arrays(b, arrays, assoc)
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_arrays(st, arrays, assoc);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_arrays(inner, arrays, assoc);
                for r in redirects {
                    collect_arrays_expr(&r.target, arrays, assoc);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                collect_arrays_expr(discriminant, arrays, assoc);
                for c in clauses {
                    collect_arrays(&c.body, arrays, assoc);
                }
            }
            IrStmt::Function { body, .. } => collect_arrays(body, arrays, assoc),
            IrStmt::Return(e) => {
                if let Some(x) = e {
                    collect_arrays_expr(x, arrays, assoc);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_arrays(inner, arrays, assoc);
                for r in redirects {
                    collect_arrays_expr(&r.target, arrays, assoc);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                collect_arrays_expr(discriminant, arrays, assoc);
                for c in clauses {
                    collect_arrays(&c.body, arrays, assoc);
                }
            }
            IrStmt::Function { body, .. } => collect_arrays(body, arrays, assoc),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_arrays(st, arrays, assoc);
                }
            }
            _ => {}
        }
    }
}

fn collect_arrays_expr(e: &IrExpr, arrays: &mut BTreeSet<String>, assoc: &mut BTreeSet<String>) {
    match e {
        IrExpr::Call { func, args } => {
            match func.as_str() {
                "setArray" | "setArrayAppend" => {
                    if let Some(name) = str_arg(args, 0) {
                        arrays.insert(name.to_string());
                    }
                }
                "listVar" => {
                    if let Some(name) = str_arg(args, 0) {
                        if name != "@" && name != "*" {
                            arrays.insert(name.to_string());
                        }
                    }
                }
                "arrayIndex" => {
                    if let Some(name) = str_arg(args, 0) {
                        arrays.insert(name.to_string());
                    }
                }
                "exec" => {
                    // `(( array[i] ... ))` — array refs inside let texts
                    if let Some(cmd) = str_arg(args, 0) {
                        if cmd == "let" {
                            if let Some(IrExpr::Array(words)) = args.get(1) {
                                for w in words {
                                    if let Some(t) = str_arg(&[(*w).clone()], 0) {
                                        let mut tmp = BTreeSet::new();
                                        collect_array_names_from_text(t, &mut tmp);
                                        arrays.extend(tmp);
                                    }
                                }
                            }
                        }
                        // `declare -A map` / `local -A map` / `declare -a arr`
                        if let Some(IrExpr::Array(words)) = args.get(1) {
                            let mut flags: Vec<String> = Vec::new();
                            let mut names: Vec<String> = Vec::new();
                            for w in words {
                                if let Some(t) = str_arg(&[(*w).clone()], 0) {
                                    if t.starts_with('-') {
                                        flags.push(t.to_string());
                                    } else {
                                        names.push(t.to_string());
                                    }
                                }
                            }
                            for n in names {
                                if flags.iter().any(|f| f == "-A" || f == "-aA") {
                                    assoc.insert(n);
                                } else if flags.iter().any(|f| f == "-a") {
                                    arrays.insert(n);
                                }
                            }
                        }
                    }
                }
                "param" => {
                    let raw = str_arg(args, 1).unwrap_or("");
                    let idx_at = matches!(args.get(2), Some(IrExpr::Str(s, _)) if s == "@" || s == "*");
                    // only ARRAY-shaped params (`${arr[@]}`, `${!map[@]}`,
                    // `${arr[i]}`, `${#arr[@]}`) hoist an array var — a
                    // bare scalar `${x}` / `${x:2:3}` is just a var read
                    // (the element-slice decision is the renderer's, based
                    // on the OTHER array evidence)
                    let array_shaped = raw.contains('[')
                        || raw.ends_with("[@]")
                        || raw.ends_with("[*]")
                        || raw.starts_with('!')
                        || raw.starts_with('#')
                        || idx_at;
                    if array_shaped {
                        if let Some(name) = str_arg(args, 1) {
                            // the base name — before any `[index]` and
                            // without the `@`/`*` suffix (`prefix*`,
                            // `array[$i]`)
                            let base = array_base_name(name);
                            if !base.is_empty()
                                && !base.starts_with('$')
                                && !base.chars().all(|c| c.is_ascii_digit())
                            {
                                if base.starts_with('#') {
                                    arrays.insert(base[1..].to_string());
                                } else {
                                    arrays.insert(base);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            for a in args {
                collect_arrays_expr(a, arrays, assoc);
            }
        }
        IrExpr::Index { var, key } => {
            arrays.insert(var.clone());
            collect_arrays_expr(key, arrays, assoc);
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_arrays_expr(i, arrays, assoc);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_arrays_expr(lhs, arrays, assoc);
            collect_arrays_expr(rhs, arrays, assoc);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_arrays_expr(x, arrays, assoc);
                }
            }
        }
        IrExpr::Arith(a) => collect_arrays_arith(a, arrays, assoc),
        IrExpr::Arrow(stmts) => collect_arrays(stmts, arrays, assoc),
        IrExpr::Capture { expr, .. } => {
            if let IrExpr::Arrow(stmts) = expr.as_ref() {
                collect_arrays(stmts, arrays, assoc);
            }
        }
        _ => {}
    }
}

/// `name[` inside a let/arith TEXT → the var name (hoisted arrays).
fn arith_has_side_effects(e: &IrExpr) -> bool {
    match e {
        IrExpr::Arith(a) => arith_side_effects(a),
        IrExpr::Call { func, args } if func == "assign" => true,
        IrExpr::Call { func, args } if func == "arith" => args.iter().any(|a| {
            matches!(a, IrExpr::Str(s, _) if s.contains('=') || s.contains("++") || s.contains("--"))
        }),
        IrExpr::Call { args, .. } => args.iter().any(arith_has_side_effects),
        _ => false,
    }
}

fn arith_side_effects(a: &ArithAst) -> bool {
    match a {
        ArithAst::Assign { .. } | ArithAst::IncDec { .. } => true,
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_side_effects(lhs) || arith_side_effects(rhs)
        }
        ArithAst::Un { arg, .. } => arith_side_effects(arg),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_side_effects(test) || arith_side_effects(then) || arith_side_effects(else_)
        }
        _ => false,
    }
}

fn expr_mentions_capture(e: &IrExpr) -> bool {
        match e {
            IrExpr::Capture { .. } => true,
            IrExpr::Call { args, .. } => args.iter().any(expr_mentions_capture),
            IrExpr::BinOp { lhs, rhs, .. } => {
                expr_mentions_capture(lhs) || expr_mentions_capture(rhs)
            }
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => expr_mentions_capture(x),
                InterpPart::Lit(_) => false,
            }),
            IrExpr::Ternary { cond, then, else_ } => {
                expr_mentions_capture(cond)
                    || expr_mentions_capture(then)
                    || expr_mentions_capture(else_)
            }
            _ => false,
        }
    }

    fn collect_array_names_from_text(t: &str, out: &mut BTreeSet<String>) {
    let ch: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i].is_alphabetic() || ch[i] == '_' {
            let start = i;
            while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_') {
                i += 1;
            }
            let name: String = ch[start..i].iter().collect();
            // `name[` — an array reference
            if i < ch.len() && ch[i] == '[' {
                out.insert(name);
            }
        } else {
            i += 1;
        }
    }
}

fn collect_arrays_arith(a: &ArithAst, arrays: &mut BTreeSet<String>, assoc: &mut BTreeSet<String>) {
    match a {
        ArithAst::Index { var, key } => {
            arrays.insert(var.clone());
            collect_arrays_arith(key, arrays, assoc);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_arrays_arith(lhs, arrays, assoc);
            collect_arrays_arith(rhs, arrays, assoc);
        }
        ArithAst::Un { arg, .. } => collect_arrays_arith(arg, arrays, assoc),
        ArithAst::Cond { test, then, else_, .. } => {
            collect_arrays_arith(test, arrays, assoc);
            collect_arrays_arith(then, arrays, assoc);
            collect_arrays_arith(else_, arrays, assoc);
        }
        ArithAst::Assign { rhs, .. } => collect_arrays_arith(rhs, arrays, assoc),
        _ => {}
    }
}

fn collect_functions(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body, .. } => {
                out.insert(name.clone());
                collect_functions(body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_functions(b, out)
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_functions(then, out);
                for (_, b) in elsifs {
                    collect_functions(b, out);
                }
                collect_functions(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => collect_functions(body, out),
            IrStmt::For { body, .. } => collect_functions(body, out),
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_functions(&c.body, out);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_functions(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_functions(st, out);
                }
            }
            _ => {}
        }
    }
}

/// `typeset -i/-l/-u name` — the attribute flags, collected up front
/// so the DECLARATION (and any earlier statement) sees the right type.
fn collect_attrs(
    stmts: &[IrStmt],
    ints: &mut BTreeSet<String>,
    lowers: &mut BTreeSet<String>,
    uppers: &mut BTreeSet<String>,
) {
    for s in stmts {
        match s {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                if let Some(cmd) = str_arg(args, 0) {
                    if matches!(cmd, "declare" | "typeset" | "local") {
                        if let Some(IrExpr::Array(words)) = args.get(1) {
                            let mut flags: Vec<String> = Vec::new();
                            for w in words {
                                if let Some(t) = str_arg(&[(*w).clone()], 0) {
                                    if t.starts_with('-') {
                                        flags.push(t.to_string());
                                    } else {
                                        let n = t.split('=').next().unwrap_or(t).to_string();
                                        if flags.iter().any(|f| f == "-i") {
                                            ints.insert(n.clone());
                                        }
                                        if flags.iter().any(|f| f == "-l") {
                                            lowers.insert(n.clone());
                                        }
                                        if flags.iter().any(|f| f == "-u") {
                                            uppers.insert(n);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_attrs(b, ints, lowers, uppers)
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_attrs(then, ints, lowers, uppers);
                for (_, b) in elsifs {
                    collect_attrs(b, ints, lowers, uppers);
                }
                collect_attrs(else_, ints, lowers, uppers);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_attrs(body, ints, lowers, uppers)
            }
            IrStmt::For { body, .. } => collect_attrs(body, ints, lowers, uppers),
            IrStmt::ForInit { init, step, body, .. } => {
                collect_attrs(init, ints, lowers, uppers);
                collect_attrs(step, ints, lowers, uppers);
                collect_attrs(body, ints, lowers, uppers);
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_attrs(&c.body, ints, lowers, uppers);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_attrs(inner, ints, lowers, uppers),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_attrs(st, ints, lowers, uppers);
                }
            }
            IrStmt::Function { body, .. } => collect_attrs(body, ints, lowers, uppers),
            _ => {}
        }
    }
}

/// Collect every function DEFINITION (top-level and nested).
fn collect_fn_defs(stmts: &[IrStmt], out: &mut Vec<(String, Vec<IrStmt>, bool)>) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body, named_blocks } => {
                out.push((name.clone(), body.clone(), !named_blocks.is_empty()));
                collect_fn_defs(body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_fn_defs(b, out)
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_fn_defs(then, out);
                for (_, b) in elsifs {
                    collect_fn_defs(b, out);
                }
                collect_fn_defs(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => collect_fn_defs(body, out),
            IrStmt::ForInit { init, step, body, .. } => {
                collect_fn_defs(init, out);
                collect_fn_defs(step, out);
                collect_fn_defs(body, out);
            }
            IrStmt::For { body, .. } => collect_fn_defs(body, out),
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_fn_defs(&c.body, out);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_fn_defs(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_fn_defs(st, out);
                }
            }
            _ => {}
        }
    }
}

/// Collect var READS (for background-thread capture).
fn collect_reads(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Expr(e) => collect_reads_expr(e, out),
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    if let Some(open) = t.var.find('[') {
                        if t.var.ends_with(']') {
                            let key = &t.var[open + 1..t.var.len() - 1];
                            if key.starts_with('$') {
                                out.insert(key.trim_start_matches('$').trim_start_matches('{').trim_end_matches('}').to_string());
                            }
                        }
                    }
                }
                collect_reads_expr(expr, out);
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_reads_expr(cond, out);
                collect_reads(then, out);
                for (c, b) in elsifs {
                    collect_reads_expr(c, out);
                    collect_reads(b, out);
                }
                collect_reads(else_, out);
            }
            IrStmt::While { cond, body } => {
                collect_reads_expr(cond, out);
                collect_reads(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_reads(body, out);
                collect_reads_expr(cond, out);
            }
            IrStmt::ForInit { init, cond, step, body } => {
                collect_reads(init, out);
                collect_reads_expr(cond, out);
                collect_reads(step, out);
                collect_reads(body, out);
            }
            IrStmt::For { iter, body, .. } => {
                collect_reads_expr(iter, out);
                collect_reads(body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => collect_reads(b, out),
            IrStmt::Case { discriminant, clauses } => {
                collect_reads_expr(discriminant, out);
                for c in clauses {
                    collect_reads(&c.body, out);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_reads(inner, out);
                for r in redirects {
                    collect_reads_expr(&r.target, out);
                }
            }
            IrStmt::Function { body, .. } => collect_reads(body, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_reads(st, out);
                }
            }
            IrStmt::Output { value, .. } | IrStmt::WriteFile { path: value, .. } => {
                collect_reads_expr(value, out);
            }
            _ => {}
        }
    }
}

fn collect_reads_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            out.insert(name.clone());
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_reads_expr(lhs, out);
            collect_reads_expr(rhs, out);
        }
        IrExpr::Arith(a) => collect_reads_arith(a, out),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_reads_expr(x, out);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_reads_expr(i, out);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_reads_expr(a, out);
            }
        }
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            collect_reads_expr(key, out);
        }
        IrExpr::Capture { expr, .. } => {
            if let IrExpr::Arrow(stmts) = expr.as_ref() {
                collect_reads(stmts, out);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_reads_expr(cond, out);
            collect_reads_expr(then, out);
            collect_reads_expr(else_, out);
        }
        _ => {}
    }
}

fn collect_reads_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_reads_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_reads_arith(lhs, out);
            collect_reads_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_reads_arith(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            collect_reads_arith(test, out);
            collect_reads_arith(then, out);
            collect_reads_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            collect_reads_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn render(src: &str) -> String {
        let commands = Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&commands);
        shir_to_rust(&prog)
    }

    #[test]
    fn assigns_and_echo() {
        let out = render("x=5\necho \"x is $x\"\n");
        assert!(out.contains("fn main() {"), "{out}");
        assert!(out.contains("thread_local! { static __SHV_x: std::cell::Cell<i64>"), "{out}");
        assert!(out.contains("x.with(|v| v.set(5));"), "{out}");
        assert!(out.contains("__sh_print_words"), "{out}");
    }

    #[test]
    fn if_arith_test() {
        let out = render("x=3\nif [ \"$x\" -gt 3 ]; then\ny=$((x+1))\necho \"$y\"\nfi\n");
        assert!(out.contains("> 3"), "{out}");
        assert!(out.contains("x.with(|v| v.get()) + 1"), "{out}");
        assert!(!out.contains("TODO"), "{out}");
        assert!(!out.contains("sh2"), "{out}");
    }

    #[test]
    fn no_stub_markers() {
        let out = render("x=5\necho \"x is $x\"\n");
        assert!(!out.contains("sh2"), "stub-gate marker leaked: {out}");
        assert!(!out.contains("TODO(unsupported)"), "{out}");
    }

    #[test]
    fn rust_keyword_mangled() {
        let out = render("type=1\necho \"$type\"\n");
        assert!(out.contains("static __SHV_type_: std::cell::Cell<i64>"), "{out}");
        assert!(!out.contains("static __SHV_type: std::cell::Cell"), "{out}");
    }
}

#[cfg(test)]
mod tok_tests {
    use super::*;
    use crate::Parser;

    fn render(src: &str) -> String {
        let commands = Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&commands);
        shir_to_rust(&prog)
    }

    #[test]
    fn escaped_parens_test() {
        let out = render("if [ \\( ! -h \"$1\" -a -d \"$1\" \\) -o \\( -h \"$1\" \\) ]; then echo x; fi\n");
        assert!(!out.contains("TODO"), "{out}");
    }

    #[test]
    fn dollar_hash_test() {
        let out = render("if [ $# -lt 2 ]; then echo few; fi\n");
        assert!(!out.contains("TODO"), "{out}");
    }

    #[test]
    fn extglob_test() {
        let out = render("f1=x.js\nif [[ $f1 == !(*.min).js ]]; then echo ok; fi\n");
        assert!(!out.contains("TODO"), "{out}");
    }

    #[test]
    fn arith_text_probe() {
        let toks = arith_tokens("i < ${\u{23}args[@]}").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        let mut p = ArithParser { render: &mut r, toks: &toks, pos: 0 };
        let e = p.parse_ternary();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "arith parse failed");
    }

    #[test]
    fn arith_text_parses() {
        let out = render("x=5\nif (( x > 3 && x < 10 )); then echo m; fi\n");
        assert!(!out.contains("TODO"), "{out}");
    }

    #[test]
    fn key_arith_probe() {
        let toks = arith_tokens("(2*$i)-1").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        let mut p = ArithParser { render: &mut r, toks: &toks, pos: 0 };
        let e = p.parse_ternary();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "key arith failed");
    }

    #[test]
    fn hard_arith_probe() {
        let toks = arith_tokens(" result[i] + $(wc -l < \"${files[i]}\") ").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        let mut p = ArithParser { render: &mut r, toks: &toks, pos: 0 };
        let e = p.parse_ternary();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "hard arith failed");
    }

    #[test]
    fn arith_index_probe() {
        let toks = arith_tokens(" a[1] + a[2] ").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        let mut p = ArithParser { render: &mut r, toks: &toks, pos: 0 };
        let e = p.parse_ternary();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "arith index parse failed");
    }

    #[test]
    fn test_arith_operand_probe() {
        let toks = test_tokens("\"$n\" -lt $(( a[1] + a[2] ))").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        r.arrays.insert("a".to_string());
        r.written.insert("a".to_string());
        r.written.insert("n".to_string());
        let mut p = TestParser { render: &mut r, toks: &toks, pos: 0, style: "[" };
        let e = p.parse_or();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "test arith operand failed");
    }

    #[test]
    fn base_n_probe() {
        let toks = arith_tokens("10#x > 5").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        let mut p = ArithParser { render: &mut r, toks: &toks, pos: 0 };
        let e = p.parse_ternary();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "base-N parse failed");
    }

    #[test]
    fn let_and_arith_call() {
        let out = render("i=1\nlet i++\nj=$((i*2))\necho $j\n");
        assert!(!out.contains("TODO"), "{out}");
    }
}

#[cfg(test)]
mod tok2_tests {
    use super::*;

    #[test]
    fn nul_probe() {
        let toks = test_tokens("\"$stdout\"==*$'\\x00'*").unwrap();
        eprintln!("TOKS: {:?}", toks);
        assert_eq!(toks.len(), 3, "{toks:?}");
    }

    #[test]
    fn ef_probe() {
        let toks = test_tokens("\"$d/a\" -ef \"$d/b\"").unwrap();
        eprintln!("TOKS: {:?}", toks);
        let mut r = Render::default();
        let mut p = TestParser { render: &mut r, toks: &toks, pos: 0, style: "[" };
        let e = p.parse_or();
        eprintln!("PARSE: {:?} pos={} len={}", e.is_some(), p.pos, toks.len());
        assert!(e.is_some() && p.pos == toks.len(), "ef parse failed");
    }

    #[test]
    fn mangle_probe() {
        let mut r = Render::default();
        let a = r.rust_ident("prefix");
        eprintln!("A: {a}");
        let b = r.rust_ident("prefix*");
        eprintln!("B: {b}");
        let c = r.rust_ident("!prefix*");
        eprintln!("C: {c}");
        let d = r.rust_ident("prefix@");
        eprintln!("D: {d}");
        let e = r.rust_ident("prefix");
        eprintln!("E: {e}");
    }

    #[test]
    fn base_name_probe() {
        eprintln!("B1: {}", array_base_name("!prefix*"));
        eprintln!("B2: {}", array_base_name("!prefix*[@]"));
        eprintln!("B3: {}", array_base_name("array[$\u{7b}index\u{7d}]"));
        eprintln!("B4: {}", array_base_name("#arr[@]"));
    }

    #[test]
    fn h_unary_tokens() {
        let toks = test_tokens(" -h /dev/null").unwrap();
        eprintln!("TOKS: {:?}", toks);
        assert_eq!(toks.len(), 2, "{toks:?}");
        assert!(matches!(&toks[0], TTok::Unary(u) if u == "-h"), "{toks:?}");
    }
}
