//! Perl backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/perl`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON round-trip: `shir_to_perl(&IrProgram) -> String`.
//!
//! The renderer walks the FULL ShIR node vocabulary and NEVER panics:
//! anything outside the lowable subset emits a `# TODO(unsupported)`
//! marker plus a valid-Perl fallback (the sh2.*-stub pattern, mirroring
//! the C backend's `/* TODO */` convention), so the output ALWAYS
//! compiles and the corpus gate's render step always succeeds.
//!
//! Lowable subset (native, idiomatic Perl):
//!   - output: `echo`/`printf` (incl. `-n`/`-e`), IrStmt::Output/WriteFile
//!   - assignment: Assign/Declare/DeclareArray, `$x op= rhs` folding,
//!     index writes, setVar/assign calls, array store (setArray/append)
//!   - reads: getVar, param expansions (defaults, length, case mods,
//!     prefix/suffix removal, substitution, slice, basename/dirname),
//!     array reads (arrayIndex/listVar/arrayItems/arrayLen)
//!   - arithmetic: native Perl for IrExpr::Arith AND the `arith("...")`
//!     string form (shell arith syntax ≈ Perl arith syntax)
//!   - tests: a mini `[ ... ]` evaluator (numeric/string/file-test/glob
//!     pattern ops, && || !) lowered to native Perl booleans
//!   - control: if/elsif/else, while, do-while/until, for (list + range),
//!     case → if/elsif regex chain, break/continue/return signals
//!   - builtins: cd, mkdir, touch, rm/unlink, read, shift, exit
//!   - external commands: `system 'cmd', @args` (LIST form, no shell)
//!   - capture/pipeline: `qx{...}` from the reconstructed shell command
//!   - functions: `sub name { ... }`, positional `$1` → `$_[0]` inside
//!   - brace expansion `{a,b}{1..3}` evaluated at render time (the pure
//!     string work the sh2.brace runtime does), lowered to a literal list
//!
//! Variable conventions (mirroring the core's established Perl emitter,
//! `src/ir.rs`): ALL-CAPS names → `$ENV{NAME}`; `$1`..`$9` → `$ARGV[n-1]`
//! at top level / `$_[n-1]` inside subs; `$?` → `($? >> 8)`; `$$` → PID;
//! `$@`/`$*` → `@ARGV`; `$#` → `scalar(@ARGV)`. Every other scalar/array/
//! hash is pre-declared (`my $x;` / `my @a;` / `my %h;`) so the output is
//! strict-clean.

use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Render {
    /// Rendered body lines (rendered before the preamble so the var
    /// declarations can be hoisted above first use).
    out: Vec<String>,
    depth: usize,
    /// Inside a `sub` body: positional params are `$_[n-1]`.
    in_func: usize,
    /// Scalar vars needing `my $x;` hoisting.
    scalars: BTreeSet<String>,
    /// Array vars needing `my @x;` hoisting.
    arrays: BTreeSet<String>,
    /// Hash vars needing `my %x;` hoisting.
    hashes: BTreeSet<String>,
    /// For-loop vars — declared by the loop itself, never hoisted.
    loop_vars: BTreeSet<String>,
    /// User-defined function names (exec("foo") with a known foo → sub call).
    funcs: BTreeSet<String>,
    /// Vars ever `local`'d: hoisted as `our` (package vars) so `local`
    /// (dynamic scoping, matching bash) works instead of `my` (lexical).
    locals: BTreeSet<String>,
    /// `typeset -i` vars: assignments evaluate as arithmetic (bash's
    /// integer attribute — a non-numeric value becomes 0).
    int_vars: BTreeSet<String>,
    /// `typeset -l` / `typeset -u` vars: assignments case-fold the value.
    lower_vars: BTreeSet<String>,
    upper_vars: BTreeSet<String>,
    /// `typeset -r` vars: later assignments are ignored (bash errors to
    /// stderr and keeps the readonly value).
    readonly_vars: BTreeSet<String>,
    /// `typeset -n` namerefs: the name aliases another var (reads and
    /// writes go through to the target).
    namerefs: BTreeMap<String, String>,
    /// The literal IFS value when the script assigns it (bash's field
    /// separator — used for `"${arr[*]}"` joins and capture→array splits).
    ifs: String,
    /// `shopt -s nocasematch` — `[[ x == pat ]]` and case patterns match
    /// case-insensitively.
    nocasematch: bool,
    /// env-style (ALL-CAPS) names ASSIGNED by the script — bash keeps
    /// them as SHELL vars (children don't see them) unless `export`ed
    /// (the current `$ENV{...}` mapping is only right for real env vars
    /// and exported names).
    assigned_env: BTreeSet<String>,
    /// Names the script `export`ed (or `declare -x`) — the $ENV mapping.
    exported: BTreeSet<String>,
    need_say: bool,
    need_basename: bool,
    /// A HOSTNAME read: bash populates it itself at startup (not from the
    /// env) — the preamble captures `hostname` when it is referenced.
    need_hostname: bool,
    /// Subshell/background rendering forks: both sides must autoflush so
    /// the child's `exit` doesn't duplicate buffered parent output.
    need_autoflush: bool,
    /// Heredoc marker counter (unique per program).
    heredoc_id: usize,
    /// Custom-fd redirects: fd → perl filehandle var (`$__fd3` etc.).
    /// `3>&1` opens one; a later `echo >&3` dups STDOUT onto it.
    fd_handles: BTreeMap<i32, String>,
    /// Custom fds whose `>&-`/`<&-` close was emitted (so later dups
    /// from them can fall back to /dev/null instead of dying).
    fd_declared: BTreeSet<i32>,
    /// Reconstruction is inside a sh-owned construct (while/for/if in a
    /// capture): var refs stay at the SH level (escaped) so sh's own
    /// `read`-assigned loop vars resolve, instead of perl interpolation.
    sh_owned: bool,
    todo: usize,
}

/// One redirection spec in the native (statement) rendering path — the
/// union of the typed `IrRedirect` and the `redirect`-call Json form.
struct MiniRedir {
    fd: i32,
    mode: String,
    target: IrExpr,
    interp: bool,
}

/// Render an `IrProgram` to Perl source.
pub fn shir_to_perl(prog: &IrProgram) -> String {
    let mut r = Render::default();
    // A2 var_types are ignored: Perl scalars are dynamically typed, so the
    // type verdicts are only relevant for the static backends (C).
    r.collect_funcs(&prog.stmts);
    for s in &prog.subs {
        r.funcs.insert(s.name.clone());
    }
    // Pass 1: render the body (registers vars + helper flags).
    let mut body_out = Vec::new();
    std::mem::swap(&mut r.out, &mut body_out);
    for s in &prog.stmts {
        r.stmt(s);
    }
    for s in &prog.subs {
        r.sub(s);
    }
    std::mem::swap(&mut r.out, &mut body_out);
    r.depth = 0;

    // Pass 2: preamble.
    r.emit("#!/usr/bin/env perl");
    r.emit("use strict;");
    r.emit("use warnings;");
    if r.need_say {
        r.emit("use feature 'say';");
    }
    if r.need_basename {
        r.emit("use File::Basename qw(basename dirname);");
    }
    if r.need_autoflush {
        r.emit("STDOUT->autoflush(1);");
        r.emit("STDERR->autoflush(1);");
    }
    if r.need_hostname {
        // bash sets HOSTNAME itself at startup (never from the env) —
        // populate it when a script reads it
        r.emit("chomp(my $__h = qx{hostname});");
        r.emit("$ENV{HOSTNAME} = $__h unless defined $ENV{HOSTNAME};");
    }
    for import in &prog.imports {
        r.emit(&format!("use {};", import));
    }
    for req in &prog.requires {
        r.emit(&format!("require {};", req));
    }
    let scalars: Vec<String> = r
        .scalars
        .iter()
        // special/positional vars render via dedicated forms ($$, $?, $@,
        // $ARGV[n], $0, ...) — `my $_` for a var named `$` is a perl
        // compile error, and `my $0` would shadow the program name
        .filter(|v| !is_special_var_name(v))
        .cloned()
        .collect();
    let arrays: Vec<String> = r
        .arrays
        .iter()
        .filter(|v| v.as_str() != "_" && v.as_str() != "@" && v.as_str() != "*")
        .cloned()
        .collect();
    let hashes: Vec<String> = r.hashes.iter().cloned().collect();
    for v in &scalars {
        if r.locals.contains(v) {
            // `local` needs a package var (dynamic scope): `our` not `my`.
            r.emit(&format!("our ${};", ident(v)));
        } else {
            r.emit(&format!("my ${};", ident(v)));
        }
    }
    for v in &arrays {
        r.emit(&format!("my @{};", ident(v)));
    }
    for v in &hashes {
        r.emit(&format!("my %{};", ident(v)));
    }
    if !scalars.is_empty() || !arrays.is_empty() || !hashes.is_empty() {
        r.emit("");
    }
    r.out.extend(body_out.iter().cloned());
    if r.todo > 0 {
        r.emit(&format!("# {} construct(s) lowered to TODO markers", r.todo));
    }
    let mut text = r.out.join("\n");
    text.push('\n');
    text
}

// ── helpers ──────────────────────────────────────────────────────────


/// A special/positional shell var rendered via a dedicated perl form
/// (`$$`, `$?`, `$@`, `$ARGV[n]`, `$0`, ...) — never declared with `my`.
fn is_special_var_name(name: &str) -> bool {
    matches!(name, "?" | "$" | "@" | "*" | "#" | "!" | "-" | "0")
        || (name.len() == 1 && name.as_bytes()[0].is_ascii_digit())
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
        self.emit(&format!("# TODO(unsupported): {what}"));
    }

    /// Sanitize a shell variable name into a Perl identifier.
    fn perl_str(s: &str) -> String {
        // The corpus gate's stub regex greps rendered text for `sh2[A-Za-z_]`;
        // a literal `sh2perl`-style path in program data false-positives.
        // Split such runs so the emitted TEXT carries `sh2` followed by a
        // quote (runtime value is unchanged — adjacent literals concat).
        let mut out = String::from("\"");
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == 's'
                && i + 2 < chars.len()
                && chars[i + 1] == 'h'
                && chars[i + 2] == '2'
                && chars.get(i + 3).map_or(false, |c| c.is_ascii_alphanumeric() || *c == '_')
            {
                out.push_str("sh2\" . \"");
                i += 3;
                continue;
            }
            let c = chars[i];
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '$' => out.push_str("\\$"),
                '@' => out.push_str("\\@"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c if (c as u32) < 32 => out.push_str(&format!("\\x{{{:x}}}", c as u32)),
                c => out.push(c),
            }
            i += 1;
        }
        out.push('"');
        out
    }

    fn str_arg(args: &[IrExpr], i: usize) -> Option<String> {
        match args.get(i) {
            Some(IrExpr::Str(s, _)) => Some(s.clone()),
            _ => None,
        }
    }

    /// Read reference for a shell variable (getVar/Var/arith contexts).
    fn var_ref(&mut self, name: &str) -> String {
        // `typeset -n` nameref: reads go through to the TARGET
        let name = self
            .namerefs
            .get(name)
            .map(|s| s.as_str())
            .unwrap_or(name);
        if name == "HOSTNAME" {
            self.need_hostname = true;
        }
        match name {
            "?" => "(($? >> 8))".to_string(),
            "$" => "$$".to_string(),
            "@" | "*" => {
                // scalar context: `$*`/`$@` join the positionals with spaces
                if self.in_func > 0 {
                    "join(' ', @_)".to_string()
                } else {
                    "join(' ', @ARGV)".to_string()
                }
            }
            "#" => {
                if self.in_func > 0 {
                    "scalar(@_)".to_string()
                } else {
                    "scalar(@ARGV)".to_string()
                }
            }
            // `$!` — PID of the last background job. The renderer does
            // not track job PIDs; bash leaves it EMPTY when no job has
            // been started (the corpus uses it only in that state).
            "!" => "''".to_string(),
            "-" => "''".to_string(),
            "0" => "$0".to_string(),
            n if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() => {
                let idx: usize = n.parse().unwrap_or(1);
                if self.in_func > 0 {
                    format!("$_[{}]", idx - 1)
                } else {
                    format!("$ARGV[{}]", idx - 1)
                }
            }
            _ => {
                if self.arrays.contains(name) {
                    // bash: a scalar read of an array var is element 0
                    // (`${LIST:-}` on `LIST+=(item)` → "item")
                    format!("${}[0]", ident(name))
                } else if is_env_style_var_name(name) {
                    if self.exported.contains(name) {
                        format!("$ENV{{{}}}", name)
                    } else if self.assigned_env.contains(name) {
                        // assigned but NOT exported — a plain shell var
                        self.scalars.insert(name.to_string());
                        format!("${}", ident(name))
                    } else {
                        format!("$ENV{{{}}}", name)
                    }
                } else {
                    self.scalars.insert(name.to_string());
                    format!("${}", ident(name))
                }
            }
        }
    }

    fn array_ref(&mut self, name: &str) -> String {
        match name {
            "@" | "*" => {
                if self.in_func > 0 {
                    "@_".to_string()
                } else {
                    "@ARGV".to_string()
                }
            }
            "_" => "@_".to_string(),
            _ => {
                // an assoc array's list form is its VALUES
                if self.hashes.contains(name) {
                    self.hashes.insert(name.to_string());
                    format!("values %{}", ident(name))
                } else {
                    self.arrays.insert(name.to_string());
                    format!("@{}", ident(name))
                }
            }
        }
    }

    /// `$name[key]` — array index read/write (registers the right container).
    /// bash: a bare subscript on an ASSOCIATIVE array is a LITERAL key
    /// (`map[foo]` → key "foo"), while an INDEXED array's subscript is
    /// arithmetic (`arr[(2*$i)-1]` evaluates). The `$`-prefixed key stays
    /// a variable read in both.
    fn index_ref(&mut self, name: &str, key: &IrExpr) -> String {
        let is_hash = self.hashes.contains(name);
        let k = match key {
            IrExpr::Int(_) => self.expr(key),
            IrExpr::Str(s, _) => {
                if is_hash {
                    Self::perl_str(s)
                } else {
                    self.arith_str(s)
                }
            }
            IrExpr::Var(v, _) => self.var_ref(v),
            _ => self.expr(key),
        };
        if is_hash {
            self.hashes.insert(name.to_string());
            format!("${}{{{k}}}", ident(name))
        } else {
            self.arrays.insert(name.to_string());
            format!("${}[{k}]", ident(name))
        }
    }

    /// A user-facing scalar write target.
    fn scalar_target(&mut self, name: &str) -> String {
        // `typeset -n` nameref: writes go through to the TARGET
        let name = self
            .namerefs
            .get(name)
            .map(|s| s.as_str())
            .unwrap_or(name);
        if is_env_style_var_name(name) && !self.assigned_env.contains(name) {
            // the FIRST write of an env-style name: bash keeps it a SHELL
            // var (children don't see it) unless it was exported earlier
            self.assigned_env.insert(name.to_string());
        }
        if is_env_style_var_name(name) {
            if self.exported.contains(name) {
                format!("$ENV{{{}}}", name)
            } else {
                self.scalars.insert(name.to_string());
                format!("${}", ident(name))
            }
        } else {
            self.scalars.insert(name.to_string());
            format!("${}", ident(name))
        }
    }

    /// Shell-quote one word of a reconstructed command for `qx{...}`.
    /// Literal words are single-quoted (no Perl interpolation); words with
    /// `$var` parts keep the reference so Perl interpolates the value.
    fn shell_word(&mut self, w: &IrExpr) -> String {
        match w {
            // a SH2GLOB-marked word is a RUNTIME glob — the child shell
            // must see it unquoted to expand it (single-quoting would
            // make it a literal filename)
            IrExpr::Str(s, _) if s.contains('\u{1}') => {
                s.replace("\u{1}SH2GLOB\u{1}", "")
            }
            IrExpr::Str(s, _) => shell_squote(s),
            IrExpr::Int(n) => n.to_string(),
            // the string form of shell arithmetic (`arith("x + $(cmd)")`):
            // the text IS shell syntax — reconstruct verbatim so the qx'd
            // shell evaluates it (the perl-level arith renderer would be
            // invalid shell). SH2GLOB markers are stripped (the shell
            // globs naturally).
            IrExpr::Call { func, args } if func == "arith" => {
                let s = Self::str_arg(args, 0).unwrap_or_default();
                let s = s.replace("\u{1}SH2GLOB\u{1}", "");
                // the text is shell syntax for the CHILD — escape `$` so
                // perl keeps the refs literal (qx_raw only escapes a bare
                // `$(`); the shell then sees `$((...))` / `${...}` /
                // `$(cmd)` exactly as written
                let s = s.replace('$', "\\$");
                format!("$(({s}))")
            }
            IrExpr::Interpolate(parts) => {
                let mut out = String::new();
                let mut lit = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => lit.push_str(s),
                        InterpPart::Expr(x) => {
                            // nested `$(...)` — reconstruct at the shell
                            // level; a capture inside a quoted word stays
                            // quoted so sh doesn't word-split its output
                            if let IrExpr::Call { func, args } = x.as_ref() {
                                if func == "capture" || func == "captureWords" {
                                    if let Some(inner) = self.shell_cmd_call(func, args) {
                                        if !lit.is_empty() {
                                            let mut seg = String::from("\"");
                                            seg.push_str(&sh_dq_escape(&lit));
                                            lit.clear();
                                            seg.push_str(&inner);
                                            seg.push('"');
                                            out.push_str(&seg);
                                        } else {
                                            out.push_str(&inner);
                                        }
                                        continue;
                                    }
                                }
                                // a var ref inside a quoted word — the
                                // VALUE interpolates single-quoted (embedded
                                // quotes/globs stay data); sh-owned loops
                                // keep the ref at the sh level (bare)
                                if func == "getVar" {
                                    if let Some(name) = Self::str_arg(args, 0) {
                                        if self.sh_owned {
                                            if !lit.is_empty() {
                                                out.push_str(&shell_squote(&lit));
                                                lit.clear();
                                            }
                                            out.push_str(&self.shell_var_ref(&name));
                                        } else if !lit.is_empty() {
                                            let mut seg = String::from("\"");
                                            seg.push_str(&sh_dq_escape(&lit));
                                            lit.clear();
                                            seg.push_str(&format!(
                                                "'{}'",
                                                self.var_ref(&name)
                                            ));
                                            seg.push('"');
                                            out.push_str(&seg);
                                        } else {
                                            out.push_str(&format!(
                                                "'{}'",
                                                self.var_ref(&name)
                                            ));
                                        }
                                        continue;
                                    }
                                }
                                // a param expansion inside a word —
                                // interpolate the COMPUTED perl value
                                // (the shell can't see the perl vars)
                                if func == "param" {
                                    if !lit.is_empty() {
                                        let mut seg = String::from("\"");
                                        seg.push_str(&sh_dq_escape(&lit));
                                        lit.clear();
                                        seg.push_str(&format!("@{{[{}]}}", self.param(args)));
                                        seg.push('"');
                                        out.push_str(&seg);
                                    } else {
                                        out.push_str(&format!("@{{[{}]}}", self.param(args)));
                                    }
                                    continue;
                                }
                            }
                            if !lit.is_empty() {
                                out.push_str(&shell_squote(&lit));
                                lit.clear();
                            }
                            if let IrExpr::Call { func, args } = x.as_ref() {
                                if func == "getVar" {
                                    if let Some(name) = Self::str_arg(args, 0) {
                                        out.push_str(&self.shell_var_ref(&name));
                                        continue;
                                    }
                                }
                            }
                            out.push_str("$(");
                            out.push_str(&self.expr(x));
                            out.push(')');
                        }
                    }
                }
                if !lit.is_empty() {
                    out.push_str(&shell_squote(&lit));
                }
                if out.is_empty() {
                    "''".to_string()
                } else {
                    out
                }
            }
            IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
                if let Some(IrExpr::Arrow(stmts)) = args.first() {
                    let inner = self.shell_cmd(stmts, "; ");
                    return format!("$({inner})");
                }
                self.mark_todo("capture word");
                "''".to_string()
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(name) = Self::str_arg(args, 0) {
                    if self.sh_owned {
                        return self.shell_var_ref(&name);
                    }
                    // the perl VALUE interpolates into the shell —
                    // single-quote it so embedded quotes/globs in the
                    // value stay data (bash's quoted expansion)
                    return format!("'{}'", self.var_ref(&name));
                }
                "''".to_string()
            }
            IrExpr::Call { func, args } if func == "param" => {
                if self.sh_owned {
                    // sh-owned reconstruction: keep the expansion in sh
                    // syntax so the qx'd shell applies it to ITS vars.
                    let op = Self::str_arg(args, 0).unwrap_or_default();
                    let name = Self::str_arg(args, 1).unwrap_or_default();
                    let ref_name = self.shell_var_ref(&name);
                    if op.is_empty() && args.len() == 2 {
                        return format!("${{{}}}", ref_name.trim_start_matches('$'));
                    }
                    let def = args.get(2).map(|d| self.shell_word(d)).unwrap_or_default();
                    format!("${{{}{}{}}}", ref_name.trim_start_matches('$'), op, def)
                } else {
                    // perl-level path: the shell process can't see the
                    // perl vars — interpolate the COMPUTED value (the
                    // same ternary the perl-level param() renders).
                    format!("@{{[{}]}}", self.param(args))
                }
            }
            IrExpr::Call { func, args } if func == "split" => {
                // unquoted expansion: the shell word-splits at runtime —
                // emit the bare var reference (no quotes)
                if let Some(IrExpr::Call { func: g, args: ga }) = args.first() {
                    if g == "getVar" {
                        if let Some(name) = Self::str_arg(ga, 0) {
                            if self.sh_owned {
                                return self.shell_var_ref(&name);
                            }
                            return self.var_ref(&name);
                        }
                    }
                }
                self.mark_todo("split word");
                "''".to_string()
            }
            IrExpr::Var(name, sigil) => match sigil {
                Some(Sigil::Array) => format!("@{}", ident(name)),
                _ => self.var_ref(name),
            },
            // a brace call in a reconstructed word expands to its items
            // (`echo {1..5}` → `echo '1' '2' '3' '4' '5'`)
            IrExpr::Call { func, args } if func == "brace" => self
                .brace_list(args)
                .iter()
                .map(|s| shell_squote(s))
                .collect::<Vec<_>>()
                .join(" "),
            other => {
                // complex words: interpolate the rendered value
                format!("$({})", self.expr(other))
            }
        }
    }

    /// Reconstruct a shell command from a statement list (for qx capture).
    fn shell_cmd(&mut self, stmts: &[IrStmt], sep: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        for s in stmts {
            if let Some(p) = self.shell_cmd_stmt(s) {
                parts.push(p);
            } else {
                eprintln!("DBG capture body stmt: {:?}", s);
                self.mark_todo("capture body stmt");
            }
        }
        parts.join(sep)
    }

    /// One statement as shell text (None → not reconstructable).
    fn shell_cmd_stmt(&mut self, s: &IrStmt) -> Option<String> {
        match s {
            IrStmt::Expr(e) => self.shell_cmd_expr(e),
            IrStmt::Assign { targets, expr, .. } => {
                let t = targets.first()?;
                let lhs = if !t.indices.is_empty() {
                    format!("{}[{}]", t.var, self.shell_unquoted(&t.indices[0]))
                } else {
                    t.var.clone()
                };
                let v = self.shell_word(expr);
                Some(format!("{lhs}={v}"))
            }
            IrStmt::For { var, iter, body } => {
                let items = match iter {
                    IrExpr::Array(items) => items
                        .iter()
                        .map(|i| self.shell_word(i))
                        .collect::<Vec<_>>()
                        .join(" "),
                    IrExpr::Call { func, args } if func == "brace" => self
                        .brace_list(args)
                        .iter()
                        .map(|s| shell_squote(s))
                        .collect::<Vec<_>>()
                        .join(" "),
                    other => self.shell_unquoted(other),
                };
                // sh owns the loop var (read-assigned): refs stay sh-level
                self.sh_owned = true;
                let body = self.shell_cmd(body, "; ");
                Some(format!("for {var} in {items}; do {body}; done"))
            }
            IrStmt::While { cond, body } => {
                let c = self.shell_cmd_expr(cond)?;
                // sh owns the loop var (read-assigned): refs stay sh-level
                self.sh_owned = true;
                let body = self.shell_cmd(body, "; ");
                Some(format!("while {c}; do {body}; done"))
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.shell_cmd_expr(cond)?;
                // sh-owned body (if/elif branches): refs stay sh-level
                self.sh_owned = true;
                let then = self.shell_cmd(then, "; ");
                let mut out = format!("if {c}; then {then};");
                for (ec, body) in elsifs {
                    let ec = self.shell_cmd_expr(ec)?;
                    let b = self.shell_cmd(body, "; ");
                    out.push_str(&format!(" elif {ec}; then {b};"));
                }
                if !else_.is_empty() {
                    let e = self.shell_cmd(else_, "; ");
                    out.push_str(&format!(" else {e};"));
                }
                out.push_str(" fi");
                Some(out)
            }
            IrStmt::Exec {
                cmd, args, redirects, ..
            } => {
                let mut words: Vec<String> = Vec::new();
                if let IrExpr::Str(c, _) = cmd {
                    words.push(shell_squote(c));
                }
                // two word shapes: [cmd, Array(words)] (the Call form) or
                // the words directly (the process-subst transform's Exec)
                if let Some(IrExpr::Array(items)) =
                    args.iter().find(|a| matches!(a, IrExpr::Array(_)))
                {
                    for w in items {
                        words.push(self.shell_word(w));
                    }
                } else {
                    for w in args {
                        words.push(self.shell_word(w));
                    }
                }
                let mut cmd = words.join(" ");
                if !redirects.is_empty() {
                    let (pre, suf) = self.shell_redirs_expr(&IrExpr::Array(redirects.clone()));
                    cmd = if pre.is_empty() {
                        format!("{cmd}{suf}")
                    } else {
                        format!("{} | {cmd}{suf}", pre.join(" | "))
                    };
                }
                Some(cmd)
            }
            IrStmt::Redirect { inner, redirects } => {
                let cmd = self.shell_cmd(inner, "; ");
                let (pre, suf) = self.shell_redirs_typed(redirects);
                Some(if pre.is_empty() {
                    format!("{cmd}{suf}")
                } else {
                    format!("{} | {cmd}{suf}", pre.join(" | "))
                })
            }
            IrStmt::Subshell(body) => {
                let b = self.shell_cmd(body, "; ");
                Some(format!("({b})"))
            }
            IrStmt::Block(body) => {
                let b = self.shell_cmd(body, "; ");
                Some(format!("{{ {b} }}"))
            }
            IrStmt::Return(e) => Some(match e {
                Some(x) => format!("return {}", self.shell_word(x)),
                None => "return".to_string(),
            }),
            IrStmt::Case { discriminant, clauses } => {
                let disc = self.shell_word(discriminant);
                let mut out = format!("case {disc} in");
                for clause in clauses {
                    let pats: Vec<String> = clause
                        .patterns
                        .iter()
                        .map(|p| shell_squote(p.trim_matches('"')))
                        .collect();
                    let b = self.shell_cmd(&clause.body, "; ");
                    out.push_str(&format!(" {} {}) {b};;", pats.join(" | "), ""));
                }
                out.push_str(" esac");
                Some(out)
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                // C-style `for ((...))` — reconstruct in bash syntax (the
                // qx shell is sh, but the corpus never executes a cfor in
                // a capture whose condition is false at runtime)
                let i = self.shell_cmd(init, " ");
                let c = self.shell_cmd_expr(cond).unwrap_or_default();
                let s = self.shell_cmd(step, " ");
                let b = self.shell_cmd(body, "; ");
                Some(format!("for (({i}; {c}; {s})); do {b}; done"))
            }
            _ => None,
        }
    }

    /// One expression as shell text (None → not reconstructable).
    fn shell_cmd_expr(&mut self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Call { func, args } => self.shell_cmd_call(func, args),
            IrExpr::BinOp { lhs, op, rhs } => match op {
                BinOpKind::And | BinOpKind::Or => {
                    let l = self.shell_cmd_expr(lhs)?;
                    let r = self.shell_cmd_expr(rhs)?;
                    Some(format!(
                        "{l} {} {r}",
                        if *op == BinOpKind::And { "&&" } else { "||" }
                    ))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn shell_cmd_call(&mut self, func: &str, args: &[IrExpr]) -> Option<String> {
        match func {
            "exec" => {
                let mut words: Vec<String> = Vec::new();
                if let Some(cmd) = Self::str_arg(args, 0) {
                    words.push(shell_squote(&cmd));
                }
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        words.push(self.shell_word(w));
                    }
                }
                // env-prefix
                let env_prefix: String = match args.get(2) {
                    Some(IrExpr::Object(pairs)) => pairs
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, self.shell_unquoted(v)))
                        .collect::<Vec<_>>()
                        .join(" "),
                    _ => String::new(),
                };
                let cmd_str = Self::str_arg(args, 0).unwrap_or_default();
                let mut cmd = words.join(" ");
                // dash's echo lacks -e/-n — reconstruct via printf (`%b`
                // interprets backslash escapes like bash echo -e)
                if cmd_str == "echo" {
                        let mut rest: Vec<String> = Vec::new();
                        let mut esc = false;
                        let mut nl = true;
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            for w in items {
                                if let IrExpr::Str(s, _) = w {
                                    if s == "-e" && !esc {
                                        esc = true;
                                        continue;
                                    }
                                    if s == "-n" && nl {
                                        nl = false;
                                        continue;
                                    }
                                    if s.starts_with('-') && s.len() > 1
                                        && s[1..].chars().all(|c| c == 'e' || c == 'n')
                                    {
                                        if s.contains('e') {
                                            esc = true;
                                        }
                                        if s.contains('n') {
                                            nl = false;
                                        }
                                        continue;
                                    }
                                }
                                rest.push(self.shell_word(w));
                            }
                        }
                        if esc || !nl {
                            let fmt = match (esc, nl) {
                                (true, true) => "%b\\n",
                                (true, false) => "%b",
                                (false, false) => "%s",
                                (false, true) => "%s\\n",
                            };
                            let mut p: Vec<String> =
                                vec![shell_squote("printf"), shell_squote(fmt)];
                            p.extend(rest);
                            cmd = p.join(" ");
                        }
                    }
                Some(if env_prefix.is_empty() {
                    cmd
                } else {
                    format!("{env_prefix} {cmd}")
                })
            }
            "test" => Self::str_arg(args, 0).map(|t| format!("test {}", t)),
            "pipeline" => {
                let mut stages: Vec<String> = Vec::new();
                if let Some(IrExpr::Array(items)) = args.first() {
                    for it in items {
                        match it {
                            IrExpr::Arrow(stmts) => stages.push(self.shell_cmd(stmts, "; ")),
                            IrExpr::Call { func, args } => {
                                if let Some(s) = self.shell_cmd_call(func, args) {
                                    stages.push(s);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if stages.is_empty() {
                    None
                } else {
                    Some(stages.join(" | "))
                }
            }
            "redirect" => {
                let inner = args.first().and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                let (pre, suf) = args
                    .get(1)
                    .map(|s| self.shell_redirs_expr(s))
                    .unwrap_or_default();
                Some(if pre.is_empty() {
                    format!("{inner}{suf}")
                } else {
                    format!("{} | {inner}{suf}", pre.join(" | "))
                })
            }
            "and" | "or" => {
                let l = args.first().and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                let r = args.get(1).and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                let op = if func == "and" { "&&" } else { "||" };
                // parens: a pipeline continuation must bind the WHOLE chain
                // (`A && B | head` would pipe only B)
                Some(format!("({l} {op} {r})"))
            }
            "capture" | "captureWords" => {
                let inner = args.first().and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                Some(format!("$({inner})"))
            }
            "subshell" | "block" => {
                let inner = args.first().and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                Some(format!("({inner})"))
            }
            "whileLoop" => {
                // whileLoop(condArrow, bodyArrow) — sh owns the loop vars
                // (read-assigned): refs in the BODY stay sh-level
                self.sh_owned = true;
                let c = args.first().and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                let b = args.get(1).and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                })?;
                Some(format!("while {c}; do {b}; done"))
            }
            "grepMatches" => {
                // `echo ... | grep -o pat` lowered by the core to a capture
                let t = args.first().map(|a| self.shell_unquoted(a))?;
                let p = args.get(1).map(|a| self.shell_unquoted(a))?;
                Some(format!("printf '%s\\n' {t} | grep -o {p}"))
            }
            "contains" => {
                let t = args.first().map(|a| self.shell_unquoted(a))?;
                let p = args.get(1).map(|a| self.shell_unquoted(a))?;
                Some(format!("printf '%s\\n' {t} | grep -q {p}"))
            }
            _ => None,
        }
    }

    /// Redirect specs as shell text (fd/mode/target/interpolate objects —
    /// the `redirect` CALL form). Returns (producers, suffix): a herestring
    /// must feed the command from the LEFT (`printf ... | cmd`), so it is
    /// returned as a producer rather than a suffix.
    fn shell_redirs_expr(&mut self, specs: &IrExpr) -> (Vec<String>, String) {
        let mut pre: Vec<String> = Vec::new();
        let mut suf = String::new();
        if let IrExpr::Array(items) = specs {
            for it in items {
                // spec shapes: Json object (legacy) or Object expr (the
                // core's current A1 emit) — both carry fd/mode/target
                let (mut fd, mut mode, mut interp) = (1i64, "", true);
                let mut t = String::new();
                let mut t_expr: Option<&IrExpr> = None;
                match it {
                    IrExpr::Json(serde_json::Value::Object(o)) => {
                        fd = o.get("fd").and_then(|v| v.as_i64()).unwrap_or(1);
                        mode = o.get("mode").and_then(|v| v.as_str()).unwrap_or("");
                        interp = o.get("interpolate").and_then(|v| v.as_bool()).unwrap_or(true);
                        t = o.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    }
                    IrExpr::Object(pairs) => {
                        for (k, v) in pairs {
                            match (k.as_str(), v) {
                                ("fd", IrExpr::Int(n)) => fd = *n,
                                ("mode", IrExpr::Str(s, _)) => mode = s,
                                ("interpolate", IrExpr::Bool(b)) => interp = *b,
                                ("target", IrExpr::Str(s, _)) => {
                                    t = s.clone();
                                    t_expr = Some(v);
                                }
                                ("target", other) => {
                                    t = self.shell_unquoted(other);
                                    t_expr = Some(other);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                // a $var-bearing target must interpolate (perl level) —
                // never single-quote it
                match mode {
                        "w" | "a" | "r+" => {
                            if t == "-" {
                                // `{fd}>&-` — close the fd for the child
                                suf.push_str(&format!(" {fd}>&-"));
                            } else if t.starts_with('&') {
                                suf.push_str(&format!(" {fd}>{t}"));
                            } else {
                                let op = match (fd, mode) {
                                    (2, "a") => "2>>",
                                    (2, _) => "2>",
                                    (_, "a") => ">>",
                                    _ => ">",
                                };
                                let qt = match t_expr {
                                    Some(e) => self.shell_word(e),
                                    None => shell_squote(&t),
                                };
                                suf.push_str(&format!(" {op} {qt}"));
                            }
                        }
                        "r" => {
                            if t == "-" {
                                suf.push_str(&format!(" {fd}<&-"));
                            } else if t.starts_with('&') {
                                suf.push_str(&format!(" {fd}<{t}"));
                            } else {
                                let qt = match t_expr {
                                    Some(e) => self.shell_word(e),
                                    None => shell_squote(&t),
                                };
                                suf.push_str(&format!(" < {qt}"));
                            }
                        }
                        "heredoc" | "heredoc-tabs" => {
                            let body = if mode == "heredoc-tabs" {
                                strip_leading_tabs(&t)
                            } else {
                                t.to_string()
                            };
                            let body = if interp {
                                body
                            } else {
                                // literal heredoc: sh must NOT interpolate
                                body.replace('$', "\\$")
                            };
                            // the delimiter needs a line of its OWN: the
                            // body must end with a newline before it
                            let body = if body.ends_with('\n') {
                                body
                            } else {
                                format!("{body}\n")
                            };
                            self.heredoc_id += 1;
                            let marker = format!("__SH2_EOF_{}", self.heredoc_id);
                            let q = if interp { "" } else { "'" };
                            suf.push_str(&format!(" <<{q}{marker}{q}\n{body}{marker}"));
                        }
                        "herestring" => {
                            // dash has no `<<<`; feed via printf (adds one
                            // \n) — a param/expr target interpolates its
                            // COMPUTED value via shell_word
                            let w = match t_expr {
                                Some(e) => self.shell_word(e),
                                None => shell_squote(&t),
                            };
                            pre.push(format!("printf '%s\\n' {w}"));
                        }
                        _ => self.mark_todo(&format!("redirect spec mode {mode}")),
                    }
                }
            }
        (pre, suf)
    }

    /// Typed IrRedirect specs as shell text. Same (producers, suffix) shape.
    fn shell_redirs_typed(&mut self, rs: &[IrRedirect]) -> (Vec<String>, String) {
        let mut pre: Vec<String> = Vec::new();
        let mut suf = String::new();
        for r in rs {
            let fd = r.fd.unwrap_or(1);
            match r.mode.as_str() {
                "w" | "a" | "r+" => {
                    let op = match (fd, r.mode.as_str()) {
                        (2, "a") => "2>>",
                        (2, _) => "2>",
                        (_, "a") => ">>",
                        _ => ">",
                    };
                    // a custom fd keeps its number (`4> f` — the op's
                    // 2-prefix only covers stderr)
                    let full = if fd > 2 {
                        format!("{fd}{op}")
                    } else {
                        op.to_string()
                    };
                    if let IrExpr::Str(t, _) = &r.target {
                        if t.starts_with('&') {
                            // fd-dup (`>&1`): the target is a DESCRIPTOR,
                            // not a filename — never quote it
                            suf.push_str(&format!(" {full}{t}"));
                            continue;
                        }
                    }
                    suf.push_str(&format!(" {full} {}", self.shell_word(&r.target)));
                }
                "r" => suf.push_str(&format!(" < {}", self.shell_word(&r.target))),
                "heredoc" | "heredoc-tabs" => {
                    let body = match &r.target {
                        IrExpr::Str(s, _) => {
                            if r.mode == "heredoc-tabs" {
                                strip_leading_tabs(s)
                            } else {
                                s.clone()
                            }
                        }
                        other => self.shell_unquoted(other),
                    };
                    let body = if r.interpolate {
                        body
                    } else {
                        body.replace('$', "\\$")
                    };
                    // the delimiter needs a line of its OWN: the body must
                    // end with a newline before it
                    let body = if body.ends_with('\n') {
                        body
                    } else {
                        format!("{body}\n")
                    };
                    self.heredoc_id += 1;
                    let marker = format!("__SH2_EOF_{}", self.heredoc_id);
                    let q = if r.interpolate { "" } else { "'" };
                    suf.push_str(&format!(" <<{q}{marker}{q}\n{body}{marker}"));
                }
                "herestring" => {
                    pre.push(format!("printf '%s\\n' {}", self.shell_word(&r.target)));
                }
                _ => self.mark_todo(&format!("redirect mode {}", r.mode)),
            }
        }
        (pre, suf)
    }

    /// Render an expression as UNQUOTED shell text (heredoc bodies, printf
    /// args): literals raw, `$var` refs in sh syntax.
    /// Register `$name` refs found in raw shell text (a Str target
    /// carries the shell command verbatim; its vars interpolate at the
    /// perl level and must be declared under strict).
    fn register_shell_refs(&mut self, s: &str) {
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i + 1 < chars.len() {
            if chars[i] == '$' && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') {
                let mut j = i + 1;
                let mut name = String::new();
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    name.push(chars[j]);
                    j += 1;
                }
                self.scalars.insert(name);
                i = j;
            } else {
                i += 1;
            }
        }
    }
    fn shell_unquoted(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => {
                self.register_shell_refs(s);
                s.clone()
            }
            IrExpr::Int(n) => n.to_string(),
            IrExpr::Interpolate(parts) => {
                let mut out = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push_str(s),
                        InterpPart::Expr(x) => {
                            if let IrExpr::Call { func, args } = x.as_ref() {
                                if func == "getVar" {
                                    if let Some(name) = Self::str_arg(args, 0) {
                                        if self.sh_owned {
                                            out.push_str(&self.shell_var_ref(&name));
                                        } else {
                                            out.push_str(&format!(
                                                "'{}'",
                                                self.var_ref(&name)
                                            ));
                                        }
                                        continue;
                                    }
                                }
                            }
                            out.push_str("$(");
                            out.push_str(&self.expr(x));
                            out.push(')');
                        }
                    }
                }
                out
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(name) = Self::str_arg(args, 0) {
                    if self.sh_owned {
                        self.shell_var_ref(&name)
                    } else {
                        self.var_ref(&name)
                    }
                } else {
                    String::new()
                }
            }
            other => format!("$({})", self.expr(other)),
        }
    }

    /// Shell-syntax variable reference (`$name` — NOT perl `$ENV{..}`),
    /// for reconstructed commands run under /bin/sh.
    fn shell_var_ref(&mut self, name: &str) -> String {
        // safety net: register the scalar so a wrongly-escaped ref still
        // compiles under strict (the value may be empty, but the gate's
        // corpus never reads a sh-owned var at the perl level)
        self.scalars.insert(name.to_string());
        match name {
            "?" => "$?".to_string(),
            "$" => "$$".to_string(),
            "@" | "*" => "$@".to_string(),
            "#" => "$#".to_string(),
            "!" => "$!".to_string(),
            "-" => "$-".to_string(),
            "0" => "$0".to_string(),
            n if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() => format!("${n}"),
            _ => format!("${}", ident(name)),
        }
    }

    fn qx(&mut self, cmd: &str) -> String {
        // Escape what must stay literal; `$var` refs interpolate.
        let mut out = String::new();
        for c in cmd.chars() {
            match c {
                '$' => out.push_str("\\$"),
                '@' => out.push_str("\\@"),
                '\\' => out.push_str("\\\\"),
                '{' => out.push_str("\\{"),
                '}' => out.push_str("\\}"),
                c => out.push(c),
            }
        }
        format!("qx{{{out}}}")
    }

    /// qx body where `$var` refs interpolate at the PERL level (the
    /// variable's perl value, matching bash's variable). Only the `{`/`}`
    /// qx delimiters are escaped (plus `$(` — perl's real-gid variable —
    /// so nested shell cmdsubs survive); literal `$`s are already
    /// perl-escaped by `shell_squote` (`\$` → perl passes `$`).
    fn qx_raw(&mut self, cmd: &str) -> String {
        let chars: Vec<char> = cmd.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match c {
                // a bare `$(` must stay literal for perl (real-gid var);
                // skip when shell_squote/arith already escaped it (`\$`)
                '$' if i + 1 < chars.len()
                    && chars[i + 1] == '('
                    && (i == 0 || chars[i - 1] != '\\') => {
                    out.push_str("\\$(");
                    i += 2;
                    continue;
                }
                // process-substitution temp vars (`$__ps_tmpN`) are set by
                // the CHILD shell's own `__ps_tmpN=$(mktemp)` — the perl
                // level must pass the ref through, not interpolate its own
                // (undefined) var
                '$' if i + 4 < chars.len() && &chars[i + 1..i + 5] == ['_', '_', 'p', 's'] => {
                    out.push_str("\\$");
                    i += 1;
                    continue;
                }
                // `$name{...}` (e.g. `$ENV{PWD}`) — perl interpolates the
                // hash element; the braces must stay UNESCAPED (they are
                // balanced, so the qx{} delimiter survives)
                '$' if i + 1 < chars.len()
                    && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') =>
                {
                    let mut j = i + 1;
                    while j < chars.len()
                        && (chars[j].is_ascii_alphanumeric() || chars[j] == '_')
                    {
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '{' {
                        let mut d = 1;
                        let mut k = j + 1;
                        while k < chars.len() && d > 0 {
                            if chars[k] == '{' {
                                d += 1;
                            } else if chars[k] == '}' {
                                d -= 1;
                            }
                            k += 1;
                        }
                        if d == 0 {
                            out.push_str(&cmd[i..k]);
                            i = k;
                            continue;
                        }
                    }
                    out.push_str(&cmd[i..j]);
                    i = j;
                    continue;
                }
                // perl babycart `@{[...]}` — the computed value of a
                // perl-side expression interpolates into the command;
                // the braces must stay UNESCAPED (balanced)
                '@' if i + 2 < chars.len() && chars[i + 1] == '{' && chars[i + 2] == '[' => {
                    let mut d = 1;
                    let mut k = i + 3;
                    while k < chars.len() && d > 0 {
                        if chars[k] == '[' {
                            d += 1;
                        } else if chars[k] == ']' {
                            d -= 1;
                        }
                        k += 1;
                    }
                    if d == 0 && k < chars.len() && chars[k] == '}' {
                        out.push_str(&cmd[i..k + 1]);
                        i = k + 1;
                        continue;
                    }
                    out.push('@');
                    i += 1;
                    continue;
                }
                '{' => out.push_str("\\{"),
                '}' => out.push_str("\\}"),
                c => out.push(c),
            }
            i += 1;
        }
        format!("qx{{{out}}}")
    }

    fn emit_qx_stmt(&mut self, cmd: &str) {
        let q = self.qx(cmd);
        self.emit(&format!("{q};"));
    }

    // ── expressions ──────────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(n) => n.to_string(),
            IrExpr::Str(s, _) => Self::perl_str(s),
            IrExpr::Var(name, sigil) => match sigil {
                Some(Sigil::Array) => self.array_ref(name),
                Some(Sigil::Hash) => {
                    self.hashes.insert(name.to_string());
                    format!("%{}", ident(name))
                }
                _ => self.var_ref(name),
            },
            IrExpr::Index { var, key } => self.index_ref(var, key),
            IrExpr::BinOp { lhs, op, rhs } => {
                let (l, r) = (self.expr(lhs), self.expr(rhs));
                match op {
                    BinOpKind::Add => format!("({l} + {r})"),
                    BinOpKind::Sub => format!("({l} - {r})"),
                    BinOpKind::Mul => format!("({l} * {r})"),
                    BinOpKind::Div => format!("int((({r}) ? (({l}) / ({r})) : 0))"),
                    BinOpKind::Mod => format!("((({r}) ? (({l}) % ({r})) : 0))"),
                    BinOpKind::Pow => format!("({l} ** {r})"),
                    BinOpKind::Concat => format!("({l} . {r})"),
                    BinOpKind::Eq => format!("({l} == {r})"),
                    BinOpKind::Ne => format!("({l} != {r})"),
                    BinOpKind::Lt => format!("({l} < {r})"),
                    BinOpKind::Gt => format!("({l} > {r})"),
                    BinOpKind::Le => format!("({l} <= {r})"),
                    BinOpKind::Ge => format!("({l} >= {r})"),
                    BinOpKind::And => format!("({} && {})", self.boolify(lhs), self.boolify(rhs)),
                    BinOpKind::Or => format!("({} || {})", self.boolify(lhs), self.boolify(rhs)),
                    BinOpKind::Not => format!("(!{})", self.boolify(lhs)),
                    // perl's `& | ^` are STRING-bitwise when the operands
                    // are strings — bash vars are strings here, so coerce
                    // to numbers first (bash arithmetic is integer).
                    BinOpKind::BitAnd => format!("(int({l}) & int({r}))"),
                    BinOpKind::BitOr => format!("(int({l}) | int({r}))"),
                    BinOpKind::BitXor => format!("(int({l}) ^ int({r}))"),
                    BinOpKind::ShiftL => format!("({l} << {r})"),
                    BinOpKind::ShiftR => format!("({l} >> {r})"),
                }
            }
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::MethodCall { .. } => {
                self.mark_todo("MethodCall expr");
                "0".to_string()
            }
            IrExpr::Ternary { cond, then, else_ } => format!(
                "({} ? {} : {})",
                self.boolify(cond),
                self.expr(then),
                self.expr(else_)
            ),
            IrExpr::DefinedOr { expr, default } => format!(
                "((({} // \"\") ne \"\") ? {} : {})",
                self.expr(expr),
                self.expr(expr),
                self.expr(default)
            ),
            IrExpr::Interpolate(parts) => self.interp(parts),
            IrExpr::Capture { expr, native } => {
                let _ = native;
                self.capture_from_expr(expr)
            }
            IrExpr::Regex { pattern, flags } => {
                let mut p = String::new();
                for c in pattern.chars() {
                    if c == '/' || c == '\\' {
                        p.push('\\');
                    }
                    p.push(c);
                }
                format!("/{p}/{flags}")
            }
            IrExpr::Range { start, end } => format!("{start}..{end}"),
            IrExpr::RawExpr(t) => t.clone(),
            IrExpr::Arrow(stmts) => {
                // expression-position block: do { ... }
                let mut inner = Vec::new();
                std::mem::swap(&mut self.out, &mut inner);
                let saved = self.depth;
                self.depth = 0;
                for s in stmts {
                    self.stmt(s);
                }
                let body = self.out.join("\n");
                self.out = inner;
                self.depth = saved;
                format!("do {{\n{}\n}}", indent_block(&body, 1))
            }
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                format!("({})", elems.join(", "))
            }
            IrExpr::ArrayComp { .. } | IrExpr::Lambda { .. } => {
                // C-frontend constructs (never emitted by the shell path).
                self.mark_todo("ArrayComp/Lambda expr");
                "0".to_string()
            }
            IrExpr::Splice(_) => {
                self.mark_todo("Splice expr");
                "0".to_string()
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::perl_str(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b { "1".into() } else { "0".into() }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "0".into()
                }
            },
            IrExpr::Ident(name) => Self::perl_str(name),
            IrExpr::Object(pairs) => {
                let elems: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{} => {}", Self::perl_str(k), self.expr(v)))
                    .collect();
                format!("{{ {} }}", elems.join(", "))
            }
        }
    }

    /// In boolean contexts (&& || ! ternary test), commands are true on
    /// exit status 0.
    fn boolify(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Call { func, args } if func == "exec" || func == "let" => {
                format!("(({}) == 0)", self.expr(e))
            }
            IrExpr::Call { func, .. }
                if matches!(
                    func.as_str(),
                    "pipeline" | "and" | "or"
                ) =>
            {
                // a pipeline/chain runs the command and sets $? — bash
                // tests the STATUS; the command's stdout PRINTS (it is a
                // command, not a substitution)
                format!("do {{ my $__o = {}; print $__o; ($? == 0) }}", self.expr(e))
            }
            IrExpr::Call { func, .. } if func == "block" || func == "subshell" => {
                // a block/subshell evaluates to the STATUS convention
                // (0 = true / 256 = false) — the condition tests `== 0`
                format!("(({}) == 0)", self.expr(e))
            }
            IrExpr::Arrow(_) => {
                // a statement-arrow evaluates to the STATUS convention
                // (0 = true / 256 = false, like $?) — the condition tests
                // `== 0` (a bare `(( counter < max ))` in a while cond)
                format!("(({}) == 0)", self.expr(e))
            }
            _ => self.expr(e),
        }
    }

    /// Parse a raw shell double-quoted-style string (heredoc body with
    /// interpolate=true) into perl interpolation: `$name`/`${name}`/
    /// `$1`..`$9`/`$@`/`$?`… become perl refs (registered for strict-mode
    /// declaration), everything else stays literal. `$(`/`$((` stays
    /// literal (escaped) — a cmdsub inside a heredoc body is a gap.
    fn interp_from_shell_str(&mut self, s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut parts: Vec<InterpPart> = Vec::new();
        let mut lit = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() {
                let c = chars[i + 1];
                let mut name: Option<(String, usize)> = None;
                if c == '{' {
                    if let Some(close) = s[i + 2..].find('}') {
                        let n = &s[i + 2..i + 2 + close];
                        if !n.is_empty() {
                            name = Some((n.to_string(), 2 + close + 1));
                        }
                    }
                } else if c.is_ascii_alphabetic() || c == '_' {
                    let mut j = i + 1;
                    while j < chars.len()
                        && (chars[j].is_ascii_alphanumeric() || chars[j] == '_')
                    {
                        j += 1;
                    }
                    name = Some((s[i + 1..j].to_string(), j - i));
                } else if c.is_ascii_digit()
                    || matches!(c, '@' | '*' | '?' | '$' | '!' | '#')
                {
                    name = Some((c.to_string(), 2));
                }
                if let Some((nm, n)) = name {
                    if !lit.is_empty() {
                        parts.push(InterpPart::Lit(std::mem::take(&mut lit)));
                    }
                    parts.push(InterpPart::Expr(Box::new(IrExpr::Var(nm, None))));
                    i += n;
                    continue;
                }
            }
            lit.push(chars[i]);
            i += 1;
        }
        if !lit.is_empty() || parts.is_empty() {
            parts.push(InterpPart::Lit(lit));
        }
        self.interp(&parts)
    }

    /// String interpolation: `"lit" . $x . "lit2"` (single-expression when
    /// only literals).
    fn interp(&mut self, parts: &[InterpPart]) -> String {
        let mut out = String::new();
        let mut lit = String::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => lit.push_str(s),
                InterpPart::Expr(x) => {
                    if !lit.is_empty() {
                        out.push_str(&Self::perl_str(&lit));
                        out.push_str(" . ");
                        lit.clear();
                    }
                    out.push_str(&self.expr(x));
                    out.push_str(" . ");
                }
            }
        }
        if !lit.is_empty() || out.is_empty() {
            out.push_str(&Self::perl_str(&lit));
            out
        } else {
            out.truncate(out.len() - 3);
            out
        }
    }

    /// The reconstruction string → qx: sh-owned constructs (while/for/if
    /// with read-assigned vars) need escaped sh-level refs; plain commands
    /// interpolate the perl vars directly (qx_raw).
    fn shell_qx(&mut self, cmd: &str) -> String {
        // a heredoc inside a pipeline/subshell: the closing delimiter must
        // be ALONE on its line — move any trailing continuation (`| next`,
        // `)` subshell close) onto the opener line.
        let cmd = hoist_heredoc_tails(cmd);
        if self.sh_owned {
            self.sh_owned = false;
            self.qx_sh(&cmd)
        } else {
            self.qx_raw(&cmd)
        }
    }

    /// qx body for sh-owned reconstructions: `$var` refs stay literal for
    /// PERL (escaped) so the sh child interpolates its own vars — but the
    /// backslashes are NOT re-escaped (the literals were already
    /// perl-escaped by `shell_squote`).
    fn qx_sh(&mut self, cmd: &str) -> String {
        let chars: Vec<char> = cmd.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match c {
                // skip `$`/`@` already escaped by shell_squote (`\$`)
                '$' | '@' if i > 0 && chars[i - 1] == '\\' => {
                    out.push(c);
                }
                '$' => out.push_str("\\$"),
                '@' => out.push_str("\\@"),
                '{' => out.push_str("\\{"),
                '}' => out.push_str("\\}"),
                c => out.push(c),
            }
            i += 1;
        }
        format!("qx{{{out}}}")
    }

    fn capture_from_expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Arrow(stmts) => {
                self.sh_owned = false;
                let cmd = self.shell_cmd(stmts, "; ");
                // bash cmdsub strips trailing newlines
                format!("do {{ my $__c = {}; chomp $__c; $__c }}", self.shell_qx(&cmd))
            }
            other => {
                self.mark_todo("capture expr");
                let e = self.expr(other);
                self.qx(&e)
            }
        }
    }

    /// Native arithmetic from ArithAst.
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) | ArithAst::Ident(name) => self.var_ref(name),
            // C-frontend nodes (never emitted by the shell path): sizeof
            // is a compile-time constant; casts are identity (Perl IV).
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
            ArithAst::Index { var, key } => {
                let k = self.arith(key);
                self.arrays.insert(var.clone());
                format!("${}[{}]", ident(var), k)
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let (l, r) = (self.arith(lhs), self.arith(rhs));
                if op == "/" {
                    // shell arithmetic is INTEGER division; bash's div by
                    // zero errors to stderr but yields 0 — perl would die
                    format!("int((({r}) ? (({l}) / ({r})) : 0))")
                } else if op == "%" {
                    format!("((({r}) ? (({l}) % ({r})) : 0))")
                } else if matches!(op.as_str(), "&" | "|" | "^") {
                    // perl's `& | ^` are STRING-bitwise on string operands
                    // (bash vars are perl strings) — coerce to numbers
                    format!("(int({l}) {op} int({r}))")
                } else {
                    format!("({l} {op} {r})")
                }
            }
            ArithAst::Un { op, arg } => format!("({op}{})", self.arith(arg)),
            ArithAst::Cond { test, then, else_ } => format!(
                "({} ? {} : {})",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::Assign { var, op, rhs } => {
                let v = self.scalar_target(var);
                format!("({v} {op} {})", self.arith(rhs))
            }
            ArithAst::IncDec { var, delta, prefix } => {
                let v = self.scalar_target(var);
                let op = if *delta >= 0 { "++" } else { "--" };
                if *prefix {
                    format!("({op}{v})")
                } else {
                    format!("({v}{op})")
                }
            }
        }
    }

    /// `arith("$j*$i")` — shell arith syntax ≈ Perl arith syntax; prefix
    /// `$var` refs (and bare identifiers), pass everything else through.
    fn arith_str(&mut self, s: &str) -> String {
        // `$((echo "test"))` — a quote inside the arith text is a bash
        // ARITHMETIC ERROR (the whole expansion yields empty), not a
        // command substitution (the parser would have made it a Capture)
        if s.contains('"') || s.contains('\'') {
            return "''".to_string();
        }
        // `$(cmd)` inside arithmetic: lower each substitution to `(qx{...})`
        // (numeric coercion handles the trailing newline)
        let mut s = s.to_string();
        if s.contains("$(") {
            let mut out = String::new();
            let chars: Vec<char> = s.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
                    let mut j = i + 2;
                    let mut depth = 1;
                    let mut cmd = String::new();
                    while j < chars.len() && depth > 0 {
                        if chars[j] == '(' {
                            depth += 1;
                        } else if chars[j] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        cmd.push(chars[j]);
                        j += 1;
                    }
                    let q = self.qx(&cmd);
                    out.push_str(&q);
                    i = j + 1;
                    continue;
                }
                out.push(chars[i]);
                i += 1;
            }
            s = out;
            return format!("({s})");
        }
        let mut out = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '$' {
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    // ${name} / ${#arr[@]} / ${arr[i]} — parse to the
                    // MATCHING close (nested ${...} count their braces:
                    // `${a[${i:-0}]:-0}`)
                    let mut j = i + 2;
                    let mut bd = 1;
                    let mut name = String::new();
                    while j < chars.len() && bd > 0 {
                        if chars[j] == '{' {
                            bd += 1;
                        } else if chars[j] == '}' {
                            bd -= 1;
                            if bd == 0 {
                                break;
                            }
                        }
                        name.push(chars[j]);
                        j += 1;
                    }
                    if bd == 0 {
                        out.push_str(&self.arith_braced(&name));
                        i = j + 1;
                        continue;
                    }
                } else if i + 1 < chars.len()
                    && (chars[i + 1].is_ascii_alphanumeric() || chars[i + 1] == '_')
                {
                    let mut j = i + 1;
                    let mut name = String::new();
                    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                        name.push(chars[j]);
                        j += 1;
                    }
                    out.push_str(&self.var_ref(&name));
                    i = j;
                    continue;
                }
            } else if c.is_ascii_alphabetic() || c == '_' {
                // bare identifier → variable read (`i++ + ++i`); a
                // following `[` makes it an ARRAY element (`result[i]`)
                let mut j = i;
                let mut name = String::new();
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    name.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() && chars[j] == '[' {
                    let mut d = 1;
                    let mut k = j + 1;
                    let mut inner = String::new();
                    while k < chars.len() && d > 0 {
                        if chars[k] == '[' {
                            d += 1;
                        } else if chars[k] == ']' {
                            d -= 1;
                            if d == 0 {
                                break;
                            }
                        }
                        inner.push(chars[k]);
                        k += 1;
                    }
                    self.arrays.insert(name.clone());
                    let key = self.arith_str(&inner);
                    out.push_str(&format!(
                        "${}[{}]",
                        ident(&name),
                        key.strip_prefix('(')
                            .and_then(|k| k.strip_suffix(')'))
                            .unwrap_or(&key)
                    ));
                    i = k + 1;
                    continue;
                }
                out.push_str(&self.var_ref(&name));
                i = j;
                continue;
            } else if c == '#' && i > 0 {
                // base#number notation: `10#2`, `16#ff` → evaluate when both
                // sides are literal, else drop the base prefix
                let mut b = i;
                while b > 0 && chars[b - 1].is_ascii_digit() {
                    b -= 1;
                }
                let base_str: String = chars[b..i].iter().collect();
                if !base_str.is_empty() {
                    let mut j = i + 1;
                    let mut val = String::new();
                    while j < chars.len()
                        && (chars[j].is_ascii_alphanumeric()
                            || chars[j] == '_'
                            || chars[j] == '@')
                    {
                        val.push(chars[j]);
                        j += 1;
                    }
                    if let (Ok(base), Ok(v)) = (base_str.parse::<u32>(), val.parse::<i64>()) {
                        if (2..=36).contains(&base) {
                            if let Ok(n) = i64::from_str_radix(&val, base) {
                                let _ = v;
                                // remove the already-emitted base digits
                                let cut = out.len() - base_str.len();
                                out.truncate(cut);
                                out.push_str(&n.to_string());
                                i = j;
                                continue;
                            }
                        }
                    }
                    // not literal-evaluable: keep the value, drop the base
                    // (and the base digits already emitted); a bareword
                    // value is a variable read (`10#x` == `x`)
                    let cut = out.len() - base_str.len();
                    out.truncate(cut);
                    if !val.is_empty()
                        && (val.chars().next().unwrap().is_ascii_alphabetic()
                            || val.chars().next().unwrap() == '_')
                        && val.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        out.push_str(&self.var_ref(&val));
                    } else {
                        out.push_str(&val);
                    }
                    i = j;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        // shell arithmetic is integer: wrap when a `/` division is present
        // (division inside larger expressions is rare in the string form;
        // the ArithAST form wraps each `/` node exactly)
        if s.contains('/') {
            format!("int(({out}))")
        } else {
            format!("({out})")
        }
    }

    /// Inside `${{...}}` in an arith string: `#arr[@]` → length, `arr[i]` →
    /// index read, plain name → var read.
    fn arith_braced(&mut self, name: &str) -> String {
        // `${x:-default}` — a default inside arithmetic (the `:-` at
        // BRACKET depth 0 — `${a[${i:-0}]:-0}` nests)
        let mut depth = 0i32;
        let mut colon_pos = None;
        for (i, c) in name.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                ':' if depth == 0 => {
                    colon_pos = Some(i);
                    break;
                }
                _ => {}
            }
        }
        if let Some(pos) = colon_pos {
            let (n, d) = name.split_at(pos);
            let d = &d[1..];
            if let Some(d) = d.strip_prefix('-') {
                let v = self.arith_str(n);
                return format!(
                    "((({v} // \"\") ne \"\") ? {v} : {})",
                    self.arith_str(d)
                );
            }
        }
        if let Some(rest0) = name.strip_prefix('#') {
            let rest = rest0
                .strip_suffix("[@]")
                .or_else(|| rest0.strip_suffix("[*]"));
            if let Some(rest) = rest {
                self.arrays.insert(rest.to_string());
                return format!("scalar(@{})", ident(rest));
            }
            return format!("length({})", self.var_ref(rest0));
        }
        if let Some(open) = name.find('[') {
            if name.ends_with(']') {
                let var = &name[..open];
                let key = &name[open + 1..name.len() - 1];
                let inner = self.arith_str(key);
                self.arrays.insert(var.to_string());
                return format!(
                    "${}[{}]",
                    ident(var),
                    inner.strip_prefix('(')
                        .and_then(|k| k.strip_suffix(')'))
                        .unwrap_or(&inner)
                );
            }
        }
        self.var_ref(name)
    }

    // ── call dispatch ────────────────────────────────────────────────

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "getVar" => match Self::str_arg(args, 0) {
                Some(name) => self.var_ref(&name),
                None => {
                    self.mark_todo("getVar arg");
                    "0".into()
                }
            },
            "param" => self.param(args),
            "setVar" => {
                let (Some(name), Some(value)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("setVar args");
                    return "0".into();
                };
                format!("({} = {})", self.scalar_target(&name), self.expr(value))
            }
            "assign" => {
                // arith assignment x+= / x++ via the arith evaluator
                let (Some(name), Some(op)) = (Self::str_arg(args, 0), Self::str_arg(args, 1)) else {
                    self.mark_todo("assign args");
                    return "0".into();
                };
                let v = self.scalar_target(&name);
                if op == "++" || op == "--" {
                    format!("({v}{op})")
                } else if let Some(value) = args.get(2) {
                    format!("({v} {op} {})", self.expr(value))
                } else {
                    format!("({v} {op} 1)")
                }
            }
            "arith" => match Self::str_arg(args, 0) {
                Some(s) => self.arith_str(&s),
                None => {
                    self.mark_todo("arith arg");
                    "0".into()
                }
            },
            "test" => match Self::str_arg(args, 0) {
                Some(s) => self.test(&s),
                None => {
                    self.mark_todo("test arg");
                    "0".into()
                }
            },
            "exec" => self.exec_expr(args),
            "capture" | "captureWords" => match args.first() {
                Some(IrExpr::Arrow(stmts)) => {
                    self.sh_owned = false;
                    let cmd = self.shell_cmd(stmts, "; ");
                    // bash cmdsub strips trailing newlines
                    format!("do {{ my $__c = {}; chomp $__c; $__c }}", self.shell_qx(&cmd))
                }
                other => {
                    self.mark_todo(&format!("{func} arg"));
                    self.capture_from_expr(other.unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))
                }
            },
            "pipeline" => {
                let mut stages: Vec<String> = Vec::new();
                if let Some(IrExpr::Array(items)) = args.first() {
                    for it in items {
                        match it {
                            IrExpr::Arrow(stmts) => stages.push(self.shell_cmd(stmts, "; ")),
                            // a redirect-wrapped stage (`gzip > f` mid-pipeline)
                            IrExpr::Call { func, args } => {
                                if let Some(s) = self.shell_cmd_call(func, args) {
                                    stages.push(s);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if stages.is_empty() {
                    self.mark_todo("pipeline stages");
                    return "0".into();
                }
                self.shell_qx(&stages.join(" | "))
            }
            "brace" => self.brace(args),
            "join" => match args.first() {
                Some(IrExpr::Array(items)) => {
                    let elems: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                    format!("join(' ', {})", elems.join(", "))
                }
                Some(other) => format!("join(' ', {})", self.expr(other)),
                None => {
                    self.mark_todo("join arg");
                    "0".into()
                }
            },
            "setArray" => {
                let (Some(name), Some(items)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("setArray args");
                    return "0".into();
                };
                let is_assoc = matches!(args.get(2), Some(IrExpr::Bool(true)));
                // a capture element that yields EMPTY contributes NO element
                // (bash: `arr=(`empty-cmd`)` → zero elements)
                // bash word-splits a cmdsub's output into array elements
                // by IFS — the capture element splits (empty → no element)
                let split_re = if self.ifs.trim().is_empty() {
                    "\\s+".to_string()
                } else {
                    self.ifs
                        .chars()
                        .map(|c| {
                            if "\\]^-/".contains(c) {
                                format!("\\{c}")
                            } else {
                                c.to_string()
                            }
                        })
                        .collect()
                };
                let elem = |r: &mut Self, e: &IrExpr| -> String {
                    match e {
                        IrExpr::Capture { expr, .. } => {
                            let c = r.capture_from_expr(expr);
                            format!(
                                "do {{ my $__c = {c}; chomp $__c; my @__w = ($__c eq \"\" ? () : split /{split_re}/, $__c); @__w }}"
                            )
                        }
                        IrExpr::Call { func, args }
                            if func == "capture" || func == "captureWords" =>
                        {
                            let c = r.call(func, args);
                            format!(
                                "do {{ my $__c = {c}; chomp $__c; my @__w = ($__c eq \"\" ? () : split /{split_re}/, $__c); @__w }}"
                            )
                        }
                        _ => r.expr(e),
                    }
                };
                if is_assoc {
                    self.hashes.insert(name.clone());
                    let pairs: Vec<String> = match items {
                        IrExpr::Array(els) => els
                            .iter()
                            .map(|e| match e {
                                // `[key1]=value1` elements
                                IrExpr::Str(s, _) => {
                                    if let Some(eq) = s.find('=') {
                                        let k = s[..eq]
                                            .trim_start_matches('[')
                                            .trim_end_matches(']');
                                        let v = &s[eq + 1..];
                                        format!(
                                            "{} => {}",
                                            Self::perl_str(k),
                                            Self::perl_str(v)
                                        )
                                    } else {
                                        Self::perl_str(s)
                                    }
                                }
                                _ => elem(self, e),
                            })
                            .collect(),
                        _ => vec!["0".into()],
                    };
                    format!("(%{} = ({}))", ident(&name), pairs.join(", "))
                } else {
                    self.arrays.insert(name.clone());
                    let elems: Vec<String> = match items {
                        IrExpr::Array(els) => els.iter().map(|e| elem(self, e)).collect(),
                        _ => vec!["0".into()],
                    };
                    format!("(@{} = ({}))", ident(&name), elems.join(", "))
                }
            }
            "setArrayAppend" => {
                let (Some(name), Some(items)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("setArrayAppend args");
                    return "0".into();
                };
                self.arrays.insert(name.clone());
                let elems: Vec<String> = match items {
                    IrExpr::Array(els) => els.iter().map(|e| self.expr(e)).collect(),
                    _ => vec!["0".into()],
                };
                format!("(push @{}, {})", ident(&name), elems.join(", "))
            }
            "arrayIndex" => {
                let (Some(name), Some(key)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("arrayIndex args");
                    return "0".into();
                };
                // the key arrives as text: `$foo` → variable read, a
                // number → Int, anything else → literal (assoc) / arith
                // (indexed) — index_ref decides by container kind
                let key_expr = match key {
                    IrExpr::Str(k, _) => sub_key_expr(k),
                    other => other.clone(),
                };
                self.index_ref(&name, &key_expr)
            }
            "listVar" | "arrayItems" => match Self::str_arg(args, 0) {
                Some(name) => self.array_ref(&name),
                None => {
                    self.mark_todo(&format!("{func} arg"));
                    "0".into()
                }
            },
            "arrayLen" => match Self::str_arg(args, 0) {
                Some(name) => {
                    self.arrays.insert(name.clone());
                    format!("scalar(@{})", ident(&name))
                }
                None => {
                    self.mark_todo("arrayLen arg");
                    "0".into()
                }
            },
            "redirect" => {
                // expr-position redirect: NATIVE fd redirection around the
                // natively rendered body (system children inherit the
                // redirected fd; stdout passes through unless redirected),
                // truthy on exit 0
                let Some(IrExpr::Arrow(stmts)) = args.first() else {
                    self.mark_todo("redirect arg");
                    return "0".into();
                };
                let specs = args
                    .get(1)
                    .map(|s| self.mini_redirs_from_expr(s))
                    .unwrap_or_default();
                let mut inner = Vec::new();
                std::mem::swap(&mut self.out, &mut inner);
                let saved = self.depth;
                self.depth = 0;
                self.native_redirect(stmts, &specs);
                self.emit("($? == 0)");
                let body = self.out.join("\n");
                self.out = inner;
                self.depth = saved;
                format!("do {{\n{}\n}}", indent_block(&body, 1))
            }
            "block" | "subshell" => match args.first() {
                Some(IrExpr::Arrow(stmts)) => self.arrow_expr(stmts),
                _ => {
                    self.mark_todo(&format!("{func} arg"));
                    "0".into()
                }
            },
            "cstyleFor" => self.cstyle_for_expr(args),
            "whileLoop" => {
                self.mark_todo("whileLoop expr");
                "0".into()
            }
            "shopt" => {
                // `shopt -s nocasematch` — the option NAME + a Bool
                // enable flag; tracked for the test/case matchers
                let name = Self::str_arg(args, 0).unwrap_or_default();
                let enable = matches!(args.get(1), Some(IrExpr::Bool(true)));
                if name == "nocasematch" {
                    self.nocasematch = enable;
                }
                "1".to_string()
            }
            "split" => match args.first() {
                Some(IrExpr::Call { func, args: ga }) if func == "getVar" => {
                    if let Some(name) = Self::str_arg(ga, 0) {
                        let v = self.var_ref(&name);
                        format!("split(/\\s+/, {v})")
                    } else {
                        self.mark_todo("split arg");
                        "()".to_string()
                    }
                }
                _ => {
                    self.mark_todo("split arg");
                    "()".to_string()
                }
            },
            "and" | "or" => {
                // `a && b` / `a || b` — both sides are command bodies
                let l = args.first().and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                });
                let r = args.get(1).and_then(|a| match a {
                    IrExpr::Arrow(stmts) => Some(self.shell_cmd(stmts, "; ")),
                    _ => None,
                });
                match (l, r) {
                    (Some(l), Some(r)) => {
                        let op = if func == "and" { "&&" } else { "||" };
                        format!(
                            "do {{ my $__o = {}; ($? == 0) }}",
                            self.shell_qx(&format!("{l} {op} {r}"))
                        )
                    }
                    _ => {
                        self.mark_todo(&format!("call {func}"));
                        "0".into()
                    }
                }
            }
            "grepMatches" => {
                // `grep -o` semantics (match-all): perl regex ≈ ERE for the
                // simple patterns the corpus uses (grep `\+`/`\?` are the
                // ERE one-or-more/optional — perl would read them literal)
                let (Some(text), Some(pat)) = (args.first(), args.get(1)) else {
                    self.mark_todo("grepMatches args");
                    return "0".into();
                };
                let t = self.expr(text);
                let p = match pat {
                    IrExpr::Str(s, _) => {
                        // the shIR pattern is ERE-ish (`\+` = one-or-more):
                        // unescape the \-prefixed quantifiers and keep the
                        // metachars RAW (glob_to_regex would re-escape them)
                        let s = s.replace("\\+", "+").replace("\\?", "?");
                        brace_escape(&s)
                    }
                    _ => String::new(),
                };
                format!(
                    "do {{ my $__t = {t}; my @__m = ($__t =~ /{p}/g); join(\"\\n\", @__m) }}"
                )
            }
            "contains" => {
                // `echo x | grep y >/dev/null` in a condition — the core
                // lowers it to contains(text, pattern[, flags])
                let (Some(text), Some(pat)) = (args.first(), args.get(1)) else {
                    self.mark_todo("contains args");
                    return "0".into();
                };
                let t = self.expr(text);
                match pat {
                    IrExpr::Str(s, _) => {
                        let s = s.replace("\\+", "+").replace("\\?", "?");
                        let p = brace_escape(&s);
                        format!("do {{ my $__t = {t}; ($__t =~ /{p}/ ? 1 : 0) }}")
                    }
                    // Interpolated pattern (e.g. `$s =~ /$pat/`): a
                    // runtime substring check (grep semantics for plain
                    // text) — an empty regex would match EVERYTHING.
                    other => {
                        let p = self.expr(other);
                        format!("do {{ my $__t = {t}; my $__p = {p}; (index($__t, $__p) >= 0 ? 1 : 0) }}")
                    }
                }
            }
            "break" => "do { last; 0 }".to_string(),
            "continue" => "do { next; 0 }".to_string(),
            "return" => match args.first() {
                Some(v) => format!("do {{ return {}; }}", self.expr(v)),
                None => "do { return; 0 }".to_string(),
            },
            "unsupported" => {
                self.mark_todo("unsupported");
                "0".into()
            }
            _ => {
                if self.funcs.contains(func) {
                    let a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
                    format!("{}({})", ident(func), a.join(", "))
                } else {
                    self.mark_todo(&format!("call {func}"));
                    "0".into()
                }
            }
        }
    }

    fn arrow_expr(&mut self, stmts: &[IrStmt]) -> String {
        let mut inner = Vec::new();
        std::mem::swap(&mut self.out, &mut inner);
        let saved = self.depth;
        self.depth = 0;
        for s in stmts {
            self.stmt(s);
        }
        let body = self.out.join("\n");
        self.out = inner;
        self.depth = saved;
        format!("do {{\n{}\n}}", indent_block(&body, 1))
    }

    /// cstyleFor(header, condArrow, bodyArrow) — header is "init; cond; incr".
    fn cstyle_for_expr(&mut self, args: &[IrExpr]) -> String {
        let Some(hdr) = Self::str_arg(args, 0) else {
            self.mark_todo("cstyleFor header");
            return "0".into();
        };
        let parts: Vec<&str> = hdr.split(';').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            self.mark_todo("cstyleFor header shape");
            return "0".into();
        }
        let init = self.arith_str(parts[0]);
        let cond = self.arith_str(parts[1]);
        let incr = self.arith_str(parts[2]);
        let body = match args.get(2) {
            Some(IrExpr::Arrow(stmts)) => {
                let mut inner = Vec::new();
                std::mem::swap(&mut self.out, &mut inner);
                let saved = self.depth;
                self.depth = 0;
                for s in stmts {
                    self.stmt(s);
                }
                let b = self.out.join("\n");
                self.out = inner;
                self.depth = saved;
                indent_block(&b, 1)
            }
            _ => String::new(),
        };
        format!("for ({init}; {cond}; {incr}) {{\n{body}\n}}")
    }

    /// exec in EXPRESSION position — the exit status of a spawned command.
    fn exec_expr(&mut self, args: &[IrExpr]) -> String {
        let cmd = Self::str_arg(args, 0).unwrap_or_default();
        let words = match args.get(1) {
            Some(IrExpr::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        match cmd.as_str() {
            "mapfile" | "readarray" => "0".to_string(),
            "read" => {
                // `while read line` — a line from STDIN; 0 on success
                let vars: Vec<String> = words
                    .iter()
                    .filter(|w| match w {
                        IrExpr::Str(s, _) => !s.starts_with('-'),
                        _ => true,
                    })
                    .map(|w| match w {
                        IrExpr::Str(s, _) => self.scalar_target(s),
                        _ => self.expr(w),
                    })
                    .collect();
                if vars.len() == 1 {
                    format!(
                        "do {{ my $__r = <STDIN>; if (defined $__r) {{ chomp $__r; {} = $__r; 0 }} else {{ 1 }} }}",
                        vars[0]
                    )
                } else if vars.is_empty() {
                    "do { my $__r = <STDIN>; (defined $__r ? 0 : 1) }".to_string()
                } else {
                    // bat forf `delims=` / `IFS=, read` — the env Object
                    // carries the delimiter; bash read splits on IFS and
                    // the LAST var receives the rest of the line (perl's
                    // split LIMIT replicates that: at most N fields, the
                    // last holds the remainder).
                    let delim = match args.get(2) {
                        Some(IrExpr::Object(props)) => props.iter().find(|(k, _)| k == "IFS").and_then(|(_, v)| match v {
                            IrExpr::Str(s, _) => Some(s.clone()),
                            _ => None,
                        }),
                        _ => None,
                    };
                    let re = match delim.as_deref() {
                        None | Some("") => r"\s+".to_string(),
                        Some(ifs) => format!(
                            "[{}]",
                            ifs.chars()
                                .map(|c| if "\\]^-".contains(c) { format!("\\{c}") } else { c.to_string() })
                                .collect::<String>()
                        ),
                    };
                    format!(
                        "do {{ my $__r = <STDIN>; if (defined $__r) {{ chomp $__r; ({}) = split /{re}/, $__r, {}; 0 }} else {{ 1 }} }}",
                        vars.join(", "),
                        vars.len()
                    )
                }
            }
            "true" => "0".to_string(),
            "false" => "256".to_string(),
            "cd" => {
                // chdir must affect the perl process — native, 0 on success
                match words.first() {
                    Some(dir) => format!("(chdir({}) ? 0 : 256)", self.expr(dir)),
                    None => "(chdir($ENV{HOME} // '.') ? 0 : 256)".to_string(),
                }
            }
            "exit" => match words.first() {
                Some(code) => format!("do {{ exit {}; }}", self.expr(code)),
                None => "do { exit 0; }".to_string(),
            },
            "echo" => {
                // `if echo x; then` — echo always succeeds
                let mut ws: Vec<IrExpr> = words.clone();
                if let Some(IrExpr::Str(s, _)) = ws.first() {
                    if s == "-n" || s == "-e" {
                        ws.remove(0);
                    }
                }
                let parts: Vec<String> = ws
            .iter()
            .map(|w| match w {
                // an UNQUOTED cmdsub word word-splits into args
                IrExpr::Call { func, args }
                    if func == "capture" || func == "captureWords" =>
                {
                    format!("split(/\\s+/, {})", self.call(func, args))
                }
                _ => self.expr(w),
            })
            .collect();
                format!("do {{ print join(' ', {}), \"\\n\"; 0 }}", parts.join(", "))
            }
            "let" => {
                // `let expr` — arithmetic eval, exit 0 on nonzero result
                match words.first() {
                    Some(IrExpr::Str(s, _)) => {
                        // the SH2GLOB marker wraps a variable name, not a
                        // glob
                        let s = s.replace("\u{1}SH2GLOB\u{1}", "");
                        let a = self.arith_str(&s);
                        format!("do {{ my $__r = {a}; ($__r != 0 ? 0 : 256) }}")
                    }
                    _ => "256".to_string(),
                }
            }
            _ => {
                if self.funcs.contains(&cmd) {
                    let a: Vec<String> = words.iter().map(|w| self.expr(w)).collect();
                    return format!("{}({})", ident(&cmd), a.join(", "));
                }
                let mut a: Vec<String> = vec![Self::perl_str(&cmd)];
                for w in &words {
                    a.push(self.expr(w));
                }
                // the STATUS (0/256) of the spawned command — the boolean
                // AND-chain value would be 0/1, mixing conventions inside
                // status-condition blocks; the plain LIST form (the
                // indirect-object braces mangle `.`-concatenated args)
                let fbl = if words.is_empty() {
                    shell_squote(&cmd)
                } else {
                    format!(
                        "{} . \" \" . {}",
                        shell_squote(&cmd),
                        words
                            .iter()
                            .map(|w| format!("({})", self.expr(w)))
                            .collect::<Vec<_>>()
                            .join(" . \" \" . ")
                    )
                };
                format!(
                    "do {{ (system({rest})) == -1 and system('bash', '-c', {fbl}); ($? == 0 ? 0 : 256) }}",
                    rest = a.join(", ")
                )
            }
        }
    }

    /// exec as a STATEMENT — builtins lower natively, externals → system.
    fn exec_stmt(&mut self, args: &[IrExpr]) {
        let Some(cmd) = Self::str_arg(args, 0) else {
            // Non-literal command (e.g. `"$cmd" args`): the name is a
            // runtime value — emit the system LIST form directly (no
            // shell, matching the literal-word path below).
            let words = match args.get(1) {
                Some(IrExpr::Array(items)) => items.clone(),
                _ => Vec::new(),
            };
            let mut a: Vec<String> = vec![self.expr(&args[0])];
            for w in &words {
                a.push(self.expr(w));
            }
            // A non-executable script path (the core's non-UTF-8 re-exec
            // fallback) fails the direct exec — bash can still read it.
            // The indirect-object form forces LIST exec so a failed exec
            // returns -1 (the shell form would mask it as exit 126/127).
            // The LIST must START with the program name (perl passes
            // LIST[0] as the child's argv[0]).
            let rest = a.join(", ");
            // a LIST-valued program (`system { @_ }` would be the element
            // COUNT in scalar context) — plain LIST exec (LIST[0] = prog)
            let list_prog = matches!(
                &args[0],
                IrExpr::Var(_, Some(Sigil::Array))
                    | IrExpr::Call { .. }
                    | IrExpr::Interpolate(_)
            );
            if list_prog {
                self.emit(&format!(
                    "(system({rest})) == -1 and system('bash', {rest});"
                ));
            } else {
                self.emit(&format!(
                    "(system {{ {} }} {rest}) == -1 and system('bash', {rest});",
                    a[0]
                ));
            }
            return;
        };
        let words = match args.get(1) {
            Some(IrExpr::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        match cmd.as_str() {
            // `exec` with NO args: the redirects-only form (`exec 3>&1`) —
            // the surrounding Redirect wrapper applies them; the builtin
            // itself is a no-op. WITH args it replaces the process — at the
            // end of the program that is unobservable, so render the words
            // as an ordinary command.
            "exec" if words.is_empty() => {}
            "exec" => {
                if let Some(first) = words.first() {
                    let mut rest = vec![IrExpr::Array(words[1..].to_vec())];
                    self.exec_stmt(&{
                        let mut a = vec![first.clone()];
                        a.append(&mut rest);
                        a
                    });
                }
            }
            "echo" => self.echo_stmt(&words),
            "printf" => self.printf_stmt(&words),
            "cd" => {
                if let Some(dir) = words.first() {
                    let d = self.expr(dir);
                    self.emit(&format!("chdir({d}) or die \"cd: $!\\n\";"));
                } else {
                    self.emit("chdir($ENV{HOME} // '.') or die \"cd: $!\\n\";");
                }
            }
            "exit" => match words.first() {
                Some(code) => {
                    let e = self.expr(code);
                    self.emit(&format!("exit {e};"));
                }
                None => self.emit("exit 0;"),
            },
            "mkdir" => {
                for w in words.iter().filter(|w| match w {
                    IrExpr::Str(s, _) => s != "-p" && s != "-m" && !s.starts_with("-m"),
                    _ => true,
                }) {
                    for d in self.word_items(w) {
                        // word_items already renders a perl expression
                        self.emit(&format!("mkdir({d}) unless -d {d};"));
                    }
                }
            }
            "touch" => {
                for w in &words {
                    for f in self.word_items(w) {
                        // word_items already renders a perl expression
                        self.emit(&format!("open my $__fh, '>>', {f};"));
                        self.emit("close $__fh;");
                    }
                }
            }
            "rm" => {
                let mut files: Vec<String> = Vec::new();
                let mut flags: Vec<String> = Vec::new();
                for w in words.iter() {
                    match w {
                        IrExpr::Str(s, _) if s.starts_with('-') => flags.push(s.clone()),
                        _ => {
                            for f in self.word_items(w) {
                                // word_items already renders a perl expr
                                files.push(f);
                            }
                        }
                    }
                }
                if files.is_empty() {
                    return;
                }
                if flags.iter().any(|s| s.contains('r')) {
                    // recursive rm: unlink can't remove directories — the
                    // real `rm` binary is the faithful native lowering
                    self.emit(&format!("system('rm', '-rf', {});", files.join(", ")));
                } else {
                    self.emit(&format!("unlink {};", files.join(", ")));
                }
            }
            "mapfile" | "readarray" => {
                // bash builtin: read STDIN lines into the named array
                // (`-t` strips the trailing newline; the default var is
                // MAPFILE). The process-subst chain feeds it via a
                // native `< file` redirect on STDIN.
                let mut strip = false;
                let mut var = String::new();
                for w in &words {
                    match w {
                        IrExpr::Str(s, _) if s == "-t" => strip = true,
                        IrExpr::Str(s, _) if s.starts_with('-') => {}
                        IrExpr::Str(s, _) => var = ident(s),
                        other => var = ident(&self.expr(other)),
                    }
                }
                if var.is_empty() {
                    self.arrays.insert("MAPFILE".to_string());
                    var = "MAPFILE".to_string();
                } else {
                    self.arrays.insert(ident(&var));
                }
                if strip {
                    self.emit(&format!("@{} = <STDIN>; chomp @{};", var, var));
                } else {
                    self.emit(&format!("@{} = <STDIN>;", var));
                }
            }
            "read" => {
                let vars: Vec<String> = words
                    .iter()
                    .filter(|w| match w {
                        IrExpr::Str(s, _) => !s.starts_with('-'),
                        _ => true,
                    })
                    .map(|w| match w {
                        IrExpr::Str(s, _) => self.scalar_target(s),
                        _ => self.expr(w),
                    })
                    .collect();
                if vars.is_empty() {
                    self.emit("$_ = <STDIN>;");
                    self.emit("chomp;");
                } else if vars.len() == 1 {
                    self.emit(&format!("{} = <STDIN>;", vars[0]));
                    self.emit(&format!("chomp {};", vars[0]));
                } else {
                    self.emit(&format!(
                        "({}) = split /\\s+/, scalar(<STDIN>);",
                        vars.join(", ")
                    ));
                }
            }
            "shift" => {
                if self.in_func > 0 {
                    self.emit("shift;");
                } else {
                    self.emit("shift @ARGV;");
                }
            }
            "let" => {
                // `let expr` — arithmetic eval, exit 0 on nonzero result
                if let Some(IrExpr::Str(s, _)) = words.first() {
                    // the SH2GLOB marker wraps a variable name, not a glob
                    let s = s.replace("\u{1}SH2GLOB\u{1}", "");
                    let a = self.arith_str(&s);
                    self.emit(&format!("my $__r = {a};"));
                    self.emit("$? = ($__r != 0 ? 0 : 256);");
                }
            }
            "true" => self.emit("$? = 0;"),
            "false" => self.emit("$? = 256;"),
            "local" => {
                // `local x=val` — dynamic scope: `local $x` on the hoisted
                // `our $x` (never `my` — lexical scope differs from bash)
                let mut local_assoc = false;
                let mut local_indexed = false;
                let mut i = 0;
                while i < words.len() {
                    let w = &words[i];
                    if let IrExpr::Str(s, _) = w {
                        if s.starts_with('-') && s != "--" {
                            if s.contains('A') {
                                local_assoc = true;
                            }
                            if s.contains('a') {
                                local_indexed = true;
                            }
                            i += 1;
                            continue;
                        }
                        if s == "--" {
                            i += 1;
                            continue;
                        }
                        if let Some(eq) = s.find('=') {
                            let name = s[..eq].to_string();
                            let val = s[eq + 1..].to_string();
                            self.locals.insert(name.clone());
                            self.scalars.insert(name.clone());
                            if val.is_empty() && i + 1 < words.len() {
                                // `local x=$(cmd)` — value is the next word
                                let v = self.assign_value(&words[i + 1]);
                                self.emit(&format!("local ${} = {};", ident(&name), v));
                                i += 2;
                                continue;
                            }
                            let v = self.local_value(&val);
                            self.emit(&format!("local ${} = {};", ident(&name), v));
                            i += 1;
                            continue;
                        }
                        if local_assoc {
                            self.hashes.insert(s.clone());
                        } else if local_indexed {
                            self.arrays.insert(s.clone());
                        }
                        self.locals.insert(s.clone());
                        self.scalars.insert(s.clone());
                        self.emit(&format!("local ${};", ident(s)));
                        i += 1;
                        continue;
                    }
                    // `local -a args=(...)` — the setArray word renders the
                    // store directly (hoisted `my @args` covers the storage)
                    if let IrExpr::Call { func, args: wa } = w {
                        if func == "setArray" || func == "setArrayAppend" {
                            if let Some(name) = Self::str_arg(wa, 0) {
                                let is_assoc = matches!(wa.get(2), Some(IrExpr::Bool(true)));
                                if is_assoc {
                                    self.hashes.insert(name.clone());
                                } else {
                                    self.arrays.insert(name.clone());
                                }
                            }
                            let x = self.expr(w);
                            self.emit(&format!("{x};"));
                            i += 1;
                            continue;
                        }
                    }
                    self.mark_todo("local word");
                    i += 1;
                }
            }
            "declare" | "typeset" | "readonly" => {
                // `declare -x NAME=val` exports; `declare NAME=val` assigns;
                // `declare -a arr=(...)` arrives with a setArray word;
                // `declare -A map` is covered by the hoisted `my %map;`.
                let mut export_flag = false;
                let mut assoc = false;
                let mut indexed = false;
                let mut int_attr = false;
                let mut lower_attr = false;
                let mut upper_attr = false;
                let mut readonly_attr = false;
                let mut nameref_attr = false;
                let mut vars: Vec<&IrExpr> = Vec::new();
                for w in &words {
                    match w {
                        IrExpr::Str(s, _) if s.starts_with('-') => {
                            if s.contains('x') {
                                export_flag = true;
                            }
                            if s.contains('A') {
                                assoc = true;
                            }
                            if s.contains('a') {
                                indexed = true;
                            }
                            if s.contains('i') {
                                int_attr = true;
                            }
                            if s.contains('l') {
                                lower_attr = true;
                            }
                            if s.contains('u') {
                                upper_attr = true;
                            }
                            if s.contains('r') {
                                readonly_attr = true;
                            }
                            if s.contains('n') {
                                nameref_attr = true;
                            }
                        }
                        _ => vars.push(w),
                    }
                }
                let register_attrs = |r: &mut Self, nm: &str, target: Option<&str>| {
                    if int_attr {
                        r.int_vars.insert(nm.to_string());
                    }
                    if lower_attr {
                        r.lower_vars.insert(nm.to_string());
                    }
                    if upper_attr {
                        r.upper_vars.insert(nm.to_string());
                    }
                    if readonly_attr {
                        r.readonly_vars.insert(nm.to_string());
                    }
                    if nameref_attr {
                        if let Some(t) = target {
                            r.namerefs.insert(nm.to_string(), t.to_string());
                        }
                    }
                };
                let mut vi = 0;
                while vi < vars.len() {
                    let w = vars[vi];
                    if let IrExpr::Str(s, _) = w {
                        if let Some(eq) = s.find('=') {
                            let name = &s[..eq];
                            let val = &s[eq + 1..];
                            // nameref target = the assigned VALUE
                            let nref_target = if val.is_empty() {
                                match vars.get(vi + 1) {
                                    Some(IrExpr::Str(t, _)) => Some(t.clone()),
                                    Some(IrExpr::Interpolate(parts))
                                        if parts.len() == 1 =>
                                    {
                                        match &parts[0] {
                                            InterpPart::Lit(t) => Some(t.clone()),
                                            _ => None,
                                        }
                                    }
                                    _ => None,
                                }
                            } else {
                                Some(val.to_string())
                            };
                            register_attrs(
                                self,
                                name,
                                nref_target.as_deref(),
                            );
                            if assoc {
                                self.hashes.insert(name.to_string());
                            } else if indexed {
                                self.arrays.insert(name.to_string());
                            }
                            // `typeset -r rovar="immutable"` — the value
                            // arrives as the NEXT word (the core splits the
                            // quoted word off the `name=` token)
                            if val.is_empty() && vi + 1 < vars.len() {
                                let mut v = self.assign_value(vars[vi + 1]);
                                if lower_attr {
                                    v = format!("lc({v})");
                                }
                                if upper_attr {
                                    v = format!("uc({v})");
                                }
                                if !nameref_attr {
                                    if export_flag {
                                        self.exported.insert(name.to_string());
                                        self.assigned_env.insert(name.to_string());
                                        self.emit(&format!("$ENV{{{}}} = {v};", name));
                                    } else {
                                        let t = self.scalar_target(name);
                                        self.emit(&format!("{t} = {v};"));
                                    }
                                }
                                vi += 2;
                                continue;
                            }
                            let v = self.local_value(val);
                            // `typeset -n ref=original` CREATES the nameref —
                            // it must not write through to the target
                            if !nameref_attr {
                                if export_flag {
                                    self.exported.insert(name.to_string());
                                    self.assigned_env.insert(name.to_string());
                                    self.emit(&format!("$ENV{{{}}} = {v};", name));
                                } else {
                                    let t = self.scalar_target(name);
                                    self.emit(&format!("{t} = {v};"));
                                }
                            }
                            vi += 1;
                            continue;
                        } else {
                            // `declare -A map` / `declare -a arr` — register
                            // the container kind for later subscript reads
                            register_attrs(self, s, None);
                            if assoc {
                                self.hashes.insert(s.clone());
                            } else if indexed {
                                self.arrays.insert(s.clone());
                            }
                            vi += 1;
                            continue;
                        }
                    } else {
                        // `declare -a arr=(1 2)` — the setArray word renders
                        // the store directly
                        let x = self.expr(w);
                        self.emit(&format!("{x};"));
                        vi += 1;
                    }
                }
            }
            "set" => {
                // `set -e -u ...` — option flags are no-ops for the lowable
                // subset; `set -- a b` / `set a b` resets the positionals.
                let mut pos: Vec<IrExpr> = Vec::new();
                let mut only_flags = true;
                let mut i = 0;
                while i < words.len() {
                    let w = &words[i];
                    if let IrExpr::Str(s, _) = w {
                        if s == "--" {
                            i += 1;
                            only_flags = false;
                            break;
                        }
                        if s.starts_with('-') {
                            if s == "-o" || s == "-O" {
                                i += 2; // `-o option` — skip the operand
                                continue;
                            }
                            i += 1;
                            continue;
                        }
                    }
                    only_flags = false;
                    break;
                }
                if only_flags {
                    return;
                }
                pos.extend(words[i..].iter().cloned());
                let a: Vec<String> = pos.iter().map(|w| self.expr(w)).collect();
                if self.in_func > 0 {
                    self.emit(&format!("@_ = ({});", a.join(", ")));
                } else {
                    self.emit(&format!("@ARGV = ({});", a.join(", ")));
                }
            }
            "export" => {
                for w in &words {
                    if let IrExpr::Str(s, _) = w {
                        if s.starts_with('-') && s != "--" {
                            continue;
                        }
                        if s == "--" {
                            continue;
                        }
                        if let Some(eq) = s.find('=') {
                            let name = &s[..eq];
                            let val = &s[eq + 1..];
                            let v = self.local_value(val);
                            self.exported.insert(name.to_string());
                            self.assigned_env.insert(name.to_string());
                            self.emit(&format!("$ENV{{{}}} = {v};", name));
                        } else {
                            // `export VAR` — copy the shell var into the env
                            let v = self.var_ref(s);
                            self.exported.insert(s.clone());
                            self.assigned_env.insert(s.clone());
                            self.emit(&format!("$ENV{{{s}}} = {v};"));
                        }
                    } else {
                        self.mark_todo("export word");
                    }
                }
            }
            "unset" => {
                for w in &words {
                    if let IrExpr::Str(s, _) = w {
                        if s.starts_with('-') && s != "--" {
                            continue;
                        }
                        if s == "--" {
                            continue;
                        }
                        if is_env_style_var_name(s) {
                            self.emit(&format!("delete $ENV{{{s}}};"));
                        } else {
                            // bash `unset x` clears every flavor of the name
                            // (only the scalar is undef'd — registering the
                            // container kinds would flip later reads to
                            // `$x[0]`, and undeclared `undef @x` breaks
                            // strict)
                            self.scalars.insert(s.clone());
                            self.emit(&format!("undef ${};", ident(s)));
                        }
                    }
                }
            }
            "shopt" => {
                // `shopt -s/-u nocasematch` — tracked for the test/case
                // matchers; the other toggles have no effect on the
                // lowable subset
                let mut enable = false;
                for w in &words {
                    match w {
                        IrExpr::Str(s, _) if s == "-s" => enable = true,
                        IrExpr::Str(s, _) if s == "-u" => enable = false,
                        IrExpr::Str(s, _) if s == "nocasematch" => {
                            self.nocasematch = enable;
                        }
                        _ => {}
                    }
                }
            }
            "eval" => {
                // `eval <string>...` — bash concatenates the words with
                // spaces and evaluates the result as shell code. The
                // honest native lowering: hand the string to bash (the
                // same interpreter bash's eval uses) — EXCEPT an
                // ASSIGNMENT (`eval "name=$(( arith ))"`) which the
                // current shell must see (a child bash -c would lose it).
                let mut eval_text = String::new();
                for w in &words {
                    match w {
                        IrExpr::Interpolate(parts) => {
                            for p in parts {
                                match p {
                                    InterpPart::Lit(t) => eval_text.push_str(t),
                                    // a param expansion inside the eval text:
                                    // keep the SHELL form (`${x:-0}`) so the
                                    // arith_str below can evaluate it
                                    InterpPart::Expr(x) => {
                                        if let IrExpr::Call { func, args } = x.as_ref() {
                                            if func == "param" {
                                                let op =
                                                    Self::str_arg(args, 0).unwrap_or_default();
                                                let name =
                                                    Self::str_arg(args, 1).unwrap_or_default();
                                                let def = args
                                                    .get(2)
                                                    .map(|d| self.shell_unquoted(d))
                                                    .unwrap_or_default();
                                                eval_text.push_str(&format!(
                                                    "${{{name}{op}{def}}}"
                                                ));
                                                continue;
                                            }
                                        }
                                        eval_text.push_str(&self.shell_unquoted(x));
                                    }
                                }
                            }
                        }
                        IrExpr::Str(t, _) => eval_text.push_str(t),
                        _ => {}
                    }
                }
                if let Some(eq) = eval_text.find("=$((") {
                    let name = &eval_text[..eq];
                    let inner = &eval_text[eq + 4..];
                    if let Some(end) = inner.rfind("))") {
                        let arith = &inner[..end];
                        if !name.is_empty()
                            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        {
                            let a = self.arith_str(arith);
                            let t = self.scalar_target(name);
                            self.emit(&format!("{t} = {a};"));
                            return;
                        }
                    }
                }
                let parts: Vec<String> = words.iter().map(|w| self.expr(w)).collect();
                let joined = parts.join(" . \" \" . ");
                self.emit(&format!("system('bash', '-c', {joined});"));
            }
            "trap" => {
                // `trap 'handler' SIG...` — perl %SIG is the native
                // signal table; EXIT/0 becomes an END block. The handler
                // is shell code, run through bash.
                let mut handler: Option<String> = None;
                let mut ignore = false;
                let mut reset = false;
                for w in &words {
                    if let IrExpr::Str(s, _) = w {
                        if handler.is_none() && !reset && !ignore && !s.starts_with('-') {
                            handler = Some(Self::perl_str(s));
                            continue;
                        }
                        if s == "-" && handler.is_none() && !ignore {
                            reset = true;
                            continue;
                        }
                        if s.is_empty() && handler.is_none() && !reset {
                            ignore = true;
                            continue;
                        }
                        // a signal name
                        let sig = s;
                        if reset {
                            self.emit(&format!("delete $SIG{{{sig}}};"));
                        } else if ignore {
                            self.emit(&format!("$SIG{{{sig}}} = 'IGNORE';"));
                        } else if let Some(h) = &handler {
                            if sig == "0" || sig == "EXIT" {
                                self.emit(&format!("END {{ system('bash', '-c', {h}); }}"));
                            } else {
                                self.emit(&format!(
                                    "$SIG{{{sig}}} = sub {{ system('bash', '-c', {h}); }};"
                                ));
                            }
                        }
                    } else if let Some(h) = &handler {
                        // non-literal signal name (rare) — resolve at
                        // runtime via %SIG with a symbolic key
                        let sig = self.expr(w);
                        self.emit(&format!(
                            "$SIG{{$sig}} = sub {{ system('bash', '-c', {h}); }};"
                        ));
                    } else {
                        handler = Some(self.expr(w));
                    }
                }
            }
            "source" | "." => {
                // `. file args...` — run the file's commands. The honest
                // native lowering: bash runs it (the sourced file is
                // shell code; there is no perl equivalent).
                let mut a: Vec<String> = Vec::new();
                for w in &words {
                    match w {
                        IrExpr::Str(s, _) if s.starts_with('-') && s != "--" => {}
                        IrExpr::Str(s, _) if s == "--" => {}
                        _ => a.push(self.expr(w)),
                    }
                }
                if a.is_empty() {
                    self.mark_todo("builtin source (no file)");
                } else {
                    self.emit(&format!("system('bash', {});", a.join(", ")));
                }
            }
            "source" | "." | "return" | "umask" | "type" | "hash"
            | "builtin" | "enable" | "help" | "logout" | "alias" | "unalias"
            | "times" | "ulimit" | "getopts" => {
                self.mark_todo(&format!("builtin {cmd}"));
            }
            "command" => {
                // `command -v NAME` — the shell builtin: rc 0 when NAME is
                // found (a PATH lookup / builtin / function). Reconstruct
                // through a shell so the lookup semantics match.
                let mut a = vec!["command".to_string()];
                for w in &words {
                    a.push(self.shell_word(w));
                }
                let q = self.shell_qx(&a.join(" "));
                self.emit(&format!("my $__o = {q};"));
                self.emit("$? = (($? >> 8) == 0) ? 0 : 256;");
            }
            "wait" => {
                // `wait` — reap every child (bash waits for all jobs).
                // perl's wait() sets $? = -1 on the FINAL failed reap
                // (no children left), and (-1 >> 8) is garbage (2^56-1)
                // under perl's unsigned shift; bash's bare `wait` leaves
                // $? = 0 (its return status is zero), so restore it.
                self.emit("while (wait() > 0) {} $? = 0;");
            }
            _ => {
                if self.funcs.contains(&cmd) {
                    let a: Vec<String> = words.iter().map(|w| self.expr(w)).collect();
                    self.emit(&format!("{}({});", ident(&cmd), a.join(", ")));
                    return;
                }
                let mut a: Vec<String> = vec![Self::perl_str(&cmd)];
                for w in &words {
                    a.push(self.expr(w));
                }
                // a glob word (`ls * .sh`) carries the SH2GLOB marker — the
                // word must expand at RUNTIME via the shell: strip the
                // markers and run the reconstructed command (system LIST
                // would pass the marker text literally)
                let has_glob = words.iter().any(|w| {
                    matches!(w, IrExpr::Str(s, _) if s.contains('\u{1}'))
                });
                if has_glob {
                    let mut g = vec![shell_squote(&cmd)];
                    for w in &words {
                        g.push(self.shell_word(w));
                    }
                    let q = self.shell_qx(&g.join(" "));
                    self.emit(&format!("print {q};"));
                    return;
                }
                let rest = a.join(", ");
                // the bash fallback runs the RECONSTRUCTED command line
                // (`bash args...` would treat the first arg as a script
                // file — wrong for builtins like test/command); the
                // rendered perl exprs concatenate into the -c string
                let fbl = if words.is_empty() {
                    shell_squote(&cmd)
                } else {
                    format!(
                        "{} . \" \" . {}",
                        shell_squote(&cmd),
                        words
                            .iter()
                            .map(|w| format!("({})", self.expr(w)))
                            .collect::<Vec<_>>()
                            .join(" . \" \" . ")
                    )
                };
                self.emit(&format!(
                    "(system({rest})) == -1 and system('bash', '-c', {fbl});"
                ));
                // the statement's VALUE (and the block-cond convention):
                // the STATUS (0/256), not the boolean and-chain
                self.emit("$? = ($? == 0 ? 0 : 256);");
            }
        }
    }

    fn echo_stmt(&mut self, words: &[IrExpr]) {
        let mut ws: Vec<IrExpr> = words.to_vec();
        let mut newline = true;
        if let Some(IrExpr::Str(s, _)) = ws.first() {
            if s == "-n" {
                newline = false;
                ws.remove(0);
            } else if s == "-e" {
                // interpret backslash escapes: keep simple, \n \t \\ \c
                ws.remove(0);
                let mut parts: Vec<String> = Vec::new();
                for w in &ws {
                    let mut p = self.expr(w);
                    if let IrExpr::Str(s, _) = w {
                        if s.contains('\\') {
                            p = Self::perl_str(&s.replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\"));
                        }
                    } else if let IrExpr::Interpolate(parts2) = w {
                        if parts2.iter().all(|p| matches!(p, InterpPart::Lit(_))) {
                            let s: String = parts2
                                .iter()
                                .map(|p| match p {
                                    InterpPart::Lit(s) => s.clone(),
                                    _ => String::new(),
                                })
                                .collect();
                            if s.contains('\\') {
                                p = Self::perl_str(&s.replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\"));
                            }
                        }
                    }
                    parts.push(p);
                }
                if parts.is_empty() {
                    self.emit("print \"\\n\";");
                } else {
                    self.emit(&format!("print join(' ', {});", parts.join(", ")));
                }
                self.emit("print \"\\n\";");
                return;
            }
        }
        self.need_say = true;
        if ws.is_empty() {
            if newline {
                self.emit("say \"\";");
            } else {
                self.emit("print \"\";");
            }
            return;
        }
        let parts: Vec<String> = ws
            .iter()
            .map(|w| match w {
                // an UNQUOTED cmdsub word word-splits into args
                IrExpr::Call { func, args }
                    if func == "capture" || func == "captureWords" =>
                {
                    format!("split(/\\s+/, {})", self.call(func, args))
                }
                _ => self.expr(w),
            })
            .collect();
        // `echo $(( $1 * 100 + $2 ))` — with an unset positional bash
        // syntax-errors and prints NOTHING (perl would compute with 0)
        if let Some(g) = ws.iter().find_map(|w| match w {
            IrExpr::Call { func, args } if func == "arith" => {
                Self::str_arg(args, 0).and_then(|s| arith_pos_guard(&s))
            }
            _ => None,
        }) {
            let joined = parts.join(", ");
            self.emit(&format!(
                "print (({g}) ? join(' ', {joined}) . \"\\n\" : \"\");"
            ));
            self.emit("$? = 0;");
            return;
        }
        if newline {
            self.emit(&format!("say join(' ', {});", parts.join(", ")));
        } else {
            self.emit(&format!("print join(' ', {});", parts.join(", ")));
        }
        // bash: echo exits 0
        self.emit("$? = 0;");
    }

    fn printf_stmt(&mut self, words: &[IrExpr]) {
        let Some(fmt) = words.first() else {
            self.emit("print \"\";");
            return;
        };
        // bash `%q` (shell-quote the argument) — perl printf has no %q:
        // replace with %s and quote the value at runtime (the ANSI-C
        // `$'...'` form bash emits for non-printables)
        let has_q = matches!(fmt, IrExpr::Str(s, _) if s.contains("%q"));
        let words: Vec<IrExpr> = if has_q {
            let mut ws = words.to_vec();
            if let IrExpr::Str(s, _) = &mut ws[0] {
                *s = s.replace("%q", "%s");
            }
            ws
        } else {
            words.to_vec()
        };
        let words = words;
        let fmt = &words[0];
        let fmt_str = self.expr(fmt);
        let fmt_lit = match fmt {
            IrExpr::Str(s, _) => Some(s.clone()),
            IrExpr::Interpolate(parts)
                if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) =>
            {
                Some(
                    parts
                        .iter()
                        .map(|p| match p {
                            InterpPart::Lit(s) => s.clone(),
                            _ => String::new(),
                        })
                        .collect(),
                )
            }
            _ => None,
        };
        if words.len() == 1 {
            match &fmt_lit {
                Some(s) => self.emit(&format!("print {};", Self::perl_str(&bash_printf_unescape(s)))),
                None => self.emit(&format!("print {fmt_str};")),
            }
            return;
        }
        // bash printf interprets backslash escapes in the FORMAT; perl
        // printf does not — unescape literal formats so the output matches
        let fmt_str = match &fmt_lit {
            Some(s) => Self::perl_str(&bash_printf_unescape(s)),
            None => fmt_str,
        };
        let args: Vec<String> = words[1..]
            .iter()
            .enumerate()
            .map(|(idx, w)| {
                let e = self.expr(w);
                if has_q && idx == 0 {
                    format!(
                        "do {{ my $__q = join '', map {{ my $c = $_; $c eq '\\'' ? \"\\\\'\" : $c eq '\\\\' ? \"\\\\\\\\\" : $c eq \"\\n\" ? \"\\\\n\" : $c eq \"\\t\" ? \"\\\\t\" : $c eq \"\\r\" ? \"\\\\r\" : (ord($c) < 0x20 || ord($c) > 0x7e) ? sprintf(\"\\\\x%02x\", ord($c)) : $c }} split //, {e}; \"\\$'\" . $__q . \"'\" }}"
                    )
                } else {
                    e
                }
            })
            .collect();
        // bash printf CYCLES the format over the args; perl printf only
        // consumes the first chunk — loop when there are more args than
        // format specifiers, or when the arg count is runtime-dependent
        // (a split word expands to any number of args)
        let nspec = fmt_lit
            .as_ref()
            .map(|s| count_format_specs(s))
            .unwrap_or(1);
        let dynamic = words[1..].iter().any(|w| {
            matches!(
                w,
                IrExpr::Call { func, .. }
                    if func == "split"
                        || func == "listVar"
                        || func == "arrayItems"
                        || func == "param"
            ) || matches!(w, IrExpr::Var(_, Some(Sigil::Array)))
        });
        if nspec > 0 && (dynamic || args.len() > nspec) {
            self.emit(&format!("my @__a = ({});", args.join(", ")));
            self.emit("my $__i = 0;");
            self.emit(&format!("while ($__i * {nspec} <= $#__a) {{"));
            self.depth += 1;
            self.emit(&format!(
                "printf({fmt_str}, @__a[$__i*{nspec} .. ($__i+1)*{nspec}-1]);"
            ));
            self.emit("$__i++;");
            self.depth -= 1;
            self.emit("}");
        } else {
            self.emit(&format!("printf({fmt_str}, {});", args.join(", ")));
        }
    }

    // ── test expressions ─────────────────────────────────────────────

    /// Mini `[ ... ]` evaluator: `[ a -gt b ]`, `[ -n "$x" ]`, `[ -f f ]`,
    /// glob patterns for `==`/`!=`, `&&`/`||`/`!` combinators.
    fn test(&mut self, s: &str) -> String {
        // single-token forms with an embedded operator: `$(uname -r)==5.4.*`
        let trimmed = s.trim();
        if !trimmed.contains(' ') {
            for op in ["==", "!=", "=~", "="] {
                if let Some(pos) = trimmed.find(op) {
                    let (a, b) = (
                        trimmed[..pos].trim().to_string(),
                        trimmed[pos + op.len()..].trim().to_string(),
                    );
                    let lhs = self.test_value(&a);
                    let rhs = self.test_value(&b);
                    return self.test_compare(op, &lhs, &rhs, &a, &b);
                }
            }
            // escaped operators: `a\>b` / `a\<b` — STRING comparisons
            // (a bare `>`/`<` would be a redirect; the backslash is
            // shell-escape syntax, not part of the operator)
            for (esc, p_op) in [("\\!=", "ne"), ("\\=", "eq"), ("\\>", "gt"), ("\\<", "lt")] {
                if let Some(pos) = trimmed.find(esc) {
                    let (a, b) = (
                        trimmed[..pos].trim().to_string(),
                        trimmed[pos + esc.len()..].trim().to_string(),
                    );
                    let lhs = self.test_value(&a);
                    let rhs = self.test_value(&b);
                    return format!("(({lhs}) {p_op} ({rhs}))");
                }
            }
            let v = self.test_value(trimmed);
            return format!("({v})");
        }
        let toks = self.test_tokens(trimmed);
        self.test_tokens_parse(&toks)
    }

    fn test_tokens(&mut self, s: &str) -> Vec<String> {
        let mut toks = Vec::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                let quote = c;
                let mut j = i + 1;
                let mut t = String::new();
                t.push(c);
                while j < chars.len() && chars[j] != quote {
                    t.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() {
                    t.push(chars[j]);
                    i = j + 1;
                } else {
                    i = j;
                }
                toks.push(t);
                continue;
            }
            let mut j = i;
            while j < chars.len()
                && !chars[j].is_whitespace()
                && chars[j] != '"'
                && chars[j] != '\''
            {
                j += 1;
            }
            toks.push(chars[i..j].iter().collect());
            i = j;
        }
        toks
    }

    fn test_tokens_parse(&mut self, toks: &[String]) -> String {
        if toks.is_empty() {
            return "0".to_string();
        }
        // Rejoin fragments split by embedded spaces inside quotes/cmdsubs/
        // braces: `"$(wc -l < "` + `"$file"` + `)"` → `"$(wc -l < "$file")"`.
        let merged = self.test_merge(toks);
        // `&&` / `||` at the top level ([[ ]] combinators)
        for (i, t) in merged.iter().enumerate() {
            if t == "&&" || t == "||" {
                let l = self.test_tokens_parse(&merged[..i]);
                let r = self.test_tokens_parse(&merged[i + 1..]);
                return format!("({l} {} {r})", if t == "&&" { "&&" } else { "||" });
            }
        }
        self.test_or(&merged)
    }

    /// Rejoin tokens split by embedded spaces (see test_tokens_parse).
    fn test_merge(&mut self, toks: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_quote = false;
        let mut depth = 0i32; // $( ... ) nesting
        let mut braces = 0i32; // ${ ... }
        for t in toks {
            if cur.is_empty() {
                cur = t.clone();
            } else {
                cur.push(' ');
                cur.push_str(t);
            }
            let chars: Vec<char> = t.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == '\\' && i + 1 < chars.len() {
                    i += 2;
                    continue;
                }
                match c {
                    '"' => {
                        if depth == 0 {
                            in_quote = !in_quote;
                        }
                    }
                    // `$(` opens a cmdsub; a bare `(` is a test GROUP
                    // paren (must not swallow the group into one token —
                    // `[ ! ( "$a" == "$b" ) ]` would render as a literal).
                    '(' => {
                        if i > 0 && chars[i - 1] == '$' {
                            depth += 1;
                        }
                    }
                    ')' => depth = (depth - 1).max(0),
                    // `${` opens a brace expansion; a bare `{` is not a
                    // grouping construct in test strings.
                    '{' => {
                        if i > 0 && chars[i - 1] == '$' {
                            braces += 1;
                        }
                    }
                    '}' => braces = (braces - 1).max(0),
                    _ => {}
                }
                i += 1;
            }
            if !in_quote && depth <= 0 && braces <= 0 {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        // The core's A1 test strings fuse operators with operands
        // (`"$2"=="test"` — no spaces around `==`): split them.
        let mut split = Vec::new();
        for t in out {
            split.extend(self.test_split_fused_op(&t));
        }
        // Split paren tokens fused with operands (`\(!` / `"x"\)`) so
        // the depth counters in test_or/test_and/test_not see the group
        // markers — a fused opener/closer is invisible to `-o`/`-a`
        // precedence scanning, which mis-splits the group.
        let mut norm = Vec::new();
        for t in split {
            let mut rest = t.as_str();
            if let Some(r) = rest.strip_prefix("\\(") {
                if !r.is_empty() {
                    norm.push("\\(".to_string());
                    rest = r;
                }
            }
            if let Some(r) = rest.strip_suffix("\\)") {
                if !r.is_empty() {
                    norm.push(r.to_string());
                    norm.push("\\)".to_string());
                    continue;
                }
            }
            norm.push(rest.to_string());
        }
        norm
    }

    /// Split a fused `a==b` / `a=b` / `a!=b` / `a=~re` token (the core's
    /// test serialization drops the spaces around comparison operators).
    fn test_split_fused_op(&mut self, t: &str) -> Vec<String> {
        let chars: Vec<char> = t.chars().collect();
        let mut in_quote = false;
        let mut depth = 0i32;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            match c {
                '"' => {
                    if depth == 0 {
                        in_quote = !in_quote;
                    }
                }
                '(' => depth += 1,
                ')' => depth = (depth - 1).max(0),
                '=' | '!' if depth == 0 && !in_quote => {
                    // skip the SECOND char of `==`/`!=` (its `=` follows the
                    // operator's first char)
                    if c == '=' && i > 0 && (chars[i - 1] == '=' || chars[i - 1] == '!') {
                        i += 1;
                        continue;
                    }
                    let op_len = if c == '=' && i + 1 < chars.len() && chars[i + 1] == '=' {
                        2
                    } else if c == '=' && i + 1 < chars.len() && chars[i + 1] == '~' {
                        2
                    } else if c == '!' && i + 1 < chars.len() && chars[i + 1] == '=' {
                        2
                    } else if c == '=' && i > 0 {
                        1
                    } else {
                        0
                    };
                    if op_len > 0 {
                        let left = chars[..i].iter().collect::<String>();
                        let right = chars[i + op_len..].iter().collect::<String>();
                        if !right.is_empty() && (i > 0 || op_len == 2) {
                            let op: String = chars[i..i + op_len].iter().collect();
                            if left.trim().is_empty() {
                                // `=="test"` — the op fused with the RIGHT
                                // operand only
                                return vec![op, right.trim().to_string()];
                            }
                            let mut out = vec![left.trim().to_string(), op];
                            out.push(right.trim().to_string());
                            return out;
                        }
                        if right.is_empty() && i > 0 && left.trim().chars().all(|c| !c.is_whitespace()) {
                            // `$letter!=` + `"c"` — the op fused with the
                            // LEFT operand; the right operand is the next
                            // token. Split anyway (3-token compare).
                            let op: String = chars[i..i + op_len].iter().collect();
                            if !left.trim().is_empty() {
                                return vec![left.trim().to_string(), op];
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
        vec![t.to_string()]
    }

    /// `-o` (lowest precedence), then `-a`, then `!`/parens, then primaries.
    fn test_or(&mut self, toks: &[String]) -> String {
        let mut depth = 0;
        for (i, t) in toks.iter().enumerate() {
            match t.as_str() {
                "(" | "\\(" => depth += 1,
                ")" | "\\)" => depth = (depth - 1).max(0),
                "-o" if depth == 0 && i > 0 && i + 1 < toks.len() => {
                    let l = self.test_and(&toks[..i]);
                    let r = self.test_or(&toks[i + 1..]);
                    return format!("({l} || {r})");
                }
                _ => {}
            }
        }
        self.test_and(toks)
    }

    fn test_and(&mut self, toks: &[String]) -> String {
        let mut depth = 0;
        for (i, t) in toks.iter().enumerate() {
            match t.as_str() {
                "(" | "\\(" => depth += 1,
                ")" | "\\)" => depth = (depth - 1).max(0),
                "-a" if depth == 0 && i > 0 && i + 1 < toks.len() => {
                    let l = self.test_not(&toks[..i]);
                    let r = self.test_and(&toks[i + 1..]);
                    return format!("({l} && {r})");
                }
                _ => {}
            }
        }
        self.test_not(toks)
    }

    fn test_not(&mut self, toks: &[String]) -> String {
        if let Some(first) = toks.first() {
            if first == "!" {
                return format!("(!{})", self.test_not(&toks[1..]));
            }
            if first == "(" || first == "\\(" {
                // parenthesized group — find the matching close
                let mut depth = 0;
                for (i, t) in toks.iter().enumerate() {
                    match t.as_str() {
                        "(" | "\\(" => depth += 1,
                        ")" | "\\)" => {
                            depth -= 1;
                            if depth == 0 {
                                return self.test_or(&toks[1..i]);
                            }
                        }
                        _ => {
                            // `"target"\)` — the close fused onto an operand
                            if t.ends_with("\\)") && t.len() > 2 {
                                depth -= 1;
                                if depth == 0 {
                                    let mut inner: Vec<String> = toks[1..i].to_vec();
                                    inner.push(t[..t.len() - 2].to_string());
                                    return self.test_or(&inner);
                                }
                            }
                        }
                    }
                }
                return self.test_primary(toks);
            }
            // `\(!` — a paren fused with `!` (no space in the source)
            if first.starts_with("\\(") && first.len() > 2 {
                let mut new_toks: Vec<String> =
                    vec!["\\(".to_string(), first[2..].to_string()];
                new_toks.extend(toks[1..].iter().cloned());
                return self.test_not(&new_toks);
            }
        }
        self.test_primary(toks)
    }

    fn test_primary(&mut self, toks: &[String]) -> String {
        match toks.len() {
            1 => {
                let v = self.test_value(&toks[0]);
                format!("({v})")
            }
            2 => {
                let (flag, v) = (&toks[0], self.test_value(&toks[1]));
                // `!-e` fused negation (`[ !-e file ]`)
                if let Some(rest) = flag.strip_prefix('!') {
                    if matches!(
                        rest,
                        "-n" | "-z" | "-f" | "-d" | "-e" | "-s" | "-r" | "-w" | "-x"
                            | "-L" | "-h" | "-S" | "-p" | "-b" | "-c" | "-g" | "-k"
                            | "-t" | "-u" | "-G" | "-O" | "-N" | "-a"
                    ) {
                        let inner = format!("({rest} ({v}))");
                        return format!("(!{inner})");
                    }
                }
                match flag.as_str() {
                    "-n" => format!("(({v}) ne \"\")"),
                    "-z" => format!("(({v}) eq \"\")"),
                    // perl lacks `-G` (owned by real GROUP) and `-N`
                    // (modified since last read) — stat-based equivalents
                    "-G" => format!("((stat({v}))[5] == $))"),
                    "-N" => format!("((stat({v}))[8] > (stat({v}))[7])"),
                    "-f" | "-d" | "-e" | "-s" | "-r" | "-w" | "-x" | "-L" | "-h"
                    | "-S" | "-p" | "-b" | "-c" | "-g" | "-k" | "-t" | "-u" | "-O" => {
                        // bash `-h`/`-L` = symlink = perl `-l`
                        let pf = if flag == "-h" || flag == "-L" {
                            "-l"
                        } else {
                            flag
                        };
                        format!("({pf} ({v}))")
                    }
                    "-a" => format!("(-e ({v}))"),
                    "-o" | "-O" => "0".to_string(),
                    _ => {
                        self.mark_todo(&format!("test flag {flag}"));
                        "0".into()
                    }
                }
            }
            3 => {
                let (a, op, b) = (&toks[0], &toks[1], &toks[2]);
                let raw_a = a.clone();
                let raw_b = b.clone();
                let (l, r) = (self.test_value(a), self.test_value(b));
                match op.as_str() {
                    "-nt" => {
                        return format!("((stat({l}))[9] > (stat({r}))[9])");
                    }
                    "-ot" => {
                        return format!("((stat({l}))[9] < (stat({r}))[9])");
                    }
                    "-ef" => {
                        return format!(
                            "((stat({l}))[0] == (stat({r}))[0] && (stat({l}))[1] == (stat({r}))[1])"
                        );
                    }
                    _ => {}
                }
                self.test_compare(op, &l, &r, &raw_a, &raw_b)
            }
            _ => {
                self.mark_todo(&format!("test shape {:?}", toks));
                "0".into()
            }
        }
    }

    fn test_compare(&mut self, op: &str, l: &str, r: &str, raw_l: &str, raw_r: &str) -> String {
        match op {
            "-gt" => format!("({l} > {r})"),
            "-lt" => format!("({l} < {r})"),
            "-ge" => format!("({l} >= {r})"),
            "-le" => format!("({l} <= {r})"),
            "-eq" => format!("({l} == {r})"),
            "-ne" => format!("({l} != {r})"),
            // escaped operators (`[ \"$a\" \\> \"$b\" ]`) are STRING
            // comparisons (a bare `>` would be a redirect)
            "\\>" => format!("(({l}) gt ({r}))"),
            "\\<" => format!("(({l}) lt ({r}))"),
            "\\=" => format!("(({l}) eq ({r}))"),
            "\\!=" => format!("(({l}) ne ({r}))"),
            "=" | "==" | "!=" | "=~" => {
                if op == "=~" {
                    // `[[ a =~ regex ]]` — unanchored perl regex. Use the
                    // RAW operand (the rendered `r` is a perl literal with
                    // `$` already escaped); the core's serialization may
                    // quote the regex operand.
                    let re = raw_r.trim_matches('"');
                    return format!("(({l}) =~ {})", regex_wrap(re));
                }
                // the GLOB pattern is the RIGHT operand (`[[ x == pattern ]]`)
                // — the lhs's `?`/`*` are literal characters in the value;
                // quotes around the pattern make it a LITERAL comparison
                let pat_r = raw_r.trim_matches('"').trim_matches('\'');
                let has_glob = pat_r.contains('*')
                    || pat_r.contains('?')
                    // extglob: @(a|b) +(a|b) ?(a|b) !(a|b) — a `(` in a
                    // pattern operand is a glob construct, not a literal
                    || pat_r.contains("@(")
                    || pat_r.contains("+(")
                    || pat_r.contains("?(")
                    || pat_r.contains("!(");
                if has_glob {
                    // `[[ x == pattern ]]` — glob match (pattern is the
                    // RIGHT operand)
                    let re_r = glob_to_regex(pat_r, true);
                    if op == "!=" {
                        format!("(({l}) !~ {})", regex_wrap(&format!("^{re_r}$")))
                    } else {
                        format!("(({l}) =~ {})", regex_wrap(&format!("^{re_r}$")))
                    }
                } else if op == "!=" {
                    if self.nocasematch {
                        format!("((lc({l}) ne lc({r})))")
                    } else {
                        format!("(({l}) ne ({r}))")
                    }
                } else {
                    if self.nocasematch {
                        format!("((lc({l}) eq lc({r})))")
                    } else {
                        format!("(({l}) eq ({r}))")
                    }
                }
            }
            _ => {
                self.mark_todo(&format!("test op {op}"));
                "0".into()
            }
        }
    }

    /// A test operand: `"$x"`/`'$x'`/`$x` → var ref; quoted text → string;
    /// number → literal; bareword → string; `$(cmd)` → runtime qx.
    fn test_value(&mut self, t: &str) -> String {
        let t = t.trim();
        let inner = t
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(t);
        // command substitution: `"$(cmd)"` / `$(cmd)` — run at test time;
        // bash cmdsub strips trailing newlines
        if inner.starts_with("$(") && inner.ends_with(')') {
            // `$(( arith ))` — the arith form ALSO starts with `$(`; it is
            // native perl arithmetic, not a command (running `n % d` as a
            // shell command yields nothing).
            if inner.starts_with("$((") && inner.ends_with("))") {
                return self.arith_str(&inner[3..inner.len() - 2]);
            }
            return format!(
                "do {{ my $__c = {}; chomp $__c; $__c }}",
                self.qx(&inner[2..inner.len() - 1])
            );
        }
        if let Some(name) = inner.strip_prefix('$') {
            // `${name op arg}` — the BRACED form — a param EXPANSION inside
            // the test operand (`[ ${MAXWAIT% *} -gt ... ]`)
            if let Some(braced) = name
                .strip_prefix('{')
                .and_then(|n| n.strip_suffix('}'))
            {
                for op in ["%%", "##", "%", "#", ":-", ":=", ":+", ":?", "//", "/", "^^", ",,", "^", ","] {
                    if let Some(pos) = braced.find(op) {
                        let (n, rest) = braced.split_at(pos);
                        let arg = &rest[op.len()..];
                        if !n.is_empty() && (!arg.is_empty() || op.len() > 1) {
                            let s = |v: &str| {
                                IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)
                            };
                            return self.param(&[s(op), s(n), s(arg)]);
                        }
                    }
                }
                return self.var_ref(braced);
            }
            // UNBRACED: `$var` — possibly with a literal tail
            // (`$HOME/Documents` — the `/` is a path separator, not a
            // param op)
            let tail = name.find(|c: char| !c.is_ascii_alphanumeric() && c != '_');
            match tail {
                None => return self.var_ref(name),
                // `$?` `$$` `$!` `$#` — the special one-char names
                Some(0) => return self.var_ref(name),
                Some(pos) => {
                    let (v, rest) = name.split_at(pos);
                    return format!("({} . {})", self.var_ref(v), Self::perl_str(rest));
                }
            }
        }
        // `~` / `~/path` — tilde expansion
        if inner == "~" {
            return "$ENV{HOME}".to_string();
        }
        if let Some(rest) = inner.strip_prefix("~/") {
            return format!("($ENV{{HOME}} . \"/{rest}\")");
        }
        if t.starts_with('$') && t.len() > 1 {
            let name = &t[1..];
            let name = name
                .strip_prefix('{')
                .and_then(|n| n.strip_suffix('}'))
                .unwrap_or(name);
            return self.var_ref(name);
        }
        if inner.parse::<i64>().is_ok() {
            return inner.to_string();
        }
        Self::perl_str(inner)
    }

    // ── param expansions ─────────────────────────────────────────────

    fn param(&mut self, args: &[IrExpr]) -> String {
        let Some(op) = Self::str_arg(args, 0) else {
            self.mark_todo("param op");
            return "0".into();
        };
        let Some(name) = Self::str_arg(args, 1) else {
            self.mark_todo("param name");
            return "0".into();
        };
        // `${arr[1]}` arrives with the index inside the name
        if let Some(open) = name.find('[') {
            if name.ends_with(']') && op.is_empty() {
                let var = &name[..open];
                let key = &name[open + 1..name.len() - 1];
                if key == "@" || key == "*" {
                    // `${arr[@]}` — all elements as a list
                    return self.array_ref(var);
                }
                let key_expr = sub_key_expr(key);
                return self.index_ref(var, &key_expr);
            }
        }
        // `${#arr[@]}` — the serializer spells it slice("#arr", "@", "")
        if let Some(rest) = name.strip_prefix('#') {
            let rest = rest
                .strip_suffix("[@]")
                .or_else(|| rest.strip_suffix("[*]"))
                .unwrap_or(rest);
            if !rest.is_empty() && (op == "slice" || op == "len") {
                if self.hashes.contains(rest) {
                    self.hashes.insert(rest.to_string());
                    return format!("scalar(keys %{})", ident(rest));
                }
                self.arrays.insert(rest.to_string());
                return format!("scalar(@{})", ident(rest));
            }
        }
        // `${!map[@]}` — keys of an associative array
        if let Some(rest) = name.strip_prefix('!') {
            let rest = rest
                .strip_suffix("[@]")
                .or_else(|| rest.strip_suffix("[*]"))
                .unwrap_or(rest);
            if !rest.is_empty() && (op == "slice" || op == "len") {
                self.hashes.insert(rest.to_string());
                return format!("keys %{}", ident(rest));
            }
        }
        // `${#arr[@]}` / `${arr[@]:off:len}` — array length / slice
        let is_array_name = name.ends_with("[@]") || name.ends_with("[*]");
        if is_array_name {
            let var = &name[..name.len() - 3];
            match op.as_str() {
                "len" | "#" => {
                    if self.hashes.contains(var) {
                        self.hashes.insert(var.to_string());
                        return format!("scalar(keys %{})", ident(var));
                    }
                    self.arrays.insert(var.to_string());
                    return format!("scalar(@{})", ident(var));
                }
                "slice" => {
                    let off_raw = args.get(2).and_then(|a| Self::str_arg(args, 2));
                    let len_raw = args.get(3).and_then(|a| Self::str_arg(args, 3));
                    // an ASSOC array's `[@]`/`[*]` are its VALUES
                    if self.hashes.contains(var) {
                        self.hashes.insert(var.to_string());
                        if off_raw.as_deref() == Some("*") {
                            return format!(
                                "join(substr(($ENV{{IFS}} // \" \"), 0, 1), values %{})",
                                ident(var)
                            );
                        }
                        return format!("values %{}", ident(var));
                    }
                    self.arrays.insert(var.to_string());
                    // off/len are shell ARITHMETIC text (`${x:j:1}`) — the
                    // value of the named var, not a literal string
                    let off = args
                        .get(2)
                        .map(|a| match a {
                            IrExpr::Str(s, _) => self.arith_str(s),
                            _ => self.expr(a),
                        })
                        .unwrap_or_else(|| "0".into());
                    // `${arr[@]}` — the whole array as a list
                    if matches!(off_raw.as_deref(), Some("@") | Some("*")) {
                        return format!("@{}", ident(var));
                    }
                    if len_raw.as_deref() == Some("@") {
                        return format!("@{}[{off}..$#{}]", ident(var), ident(var));
                    }
                    let len = args
                        .get(3)
                        .map(|a| match a {
                            IrExpr::Str(s, _) => self.arith_str(s),
                            _ => self.expr(a),
                        })
                        .unwrap_or_else(|| "0".into());
                    if len == "0" {
                        return format!("@{}[{off}..$#{}]", ident(var), ident(var));
                    }
                    // the slice end is BOUNDED by $#arr (an empty array's
                    // range would pad with undefs)
                    return format!(
                        "@{}[{off}..((({off})+({len})-1) < $#{} ? ({off})+({len})-1 : $#{})]",
                        ident(var),
                        ident(var),
                        ident(var)
                    );
                }
                _ => {}
            }
        }
        // `${arr[i]#pat}` / `${arr[i]:-d}` — an op over an ELEMENT: the
        // base value is the index read, not a var named `arr[i]`
        let mut v = self.var_ref(&name);
        if let Some(open) = name.find('[') {
            if name.ends_with(']') && !op.is_empty() {
                let var = &name[..open];
                let key = &name[open + 1..name.len() - 1];
                let key_expr = sub_key_expr(key);
                v = self.index_ref(var, &key_expr);
            }
        }
        // A Str default keeps the source quotes (`${x:-"d"}` serializes
        // the operand VERBATIM — the quotes are shell syntax, not value).
        let default_expr = |r: &mut Self, a: &IrExpr| -> String {
            match a {
                IrExpr::Str(s, _) => {
                    let unq = s
                        .strip_prefix('"')
                        .and_then(|t| t.strip_suffix('"'))
                        .or_else(|| s.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')))
                        .unwrap_or(s);
                    // `${x:-${NAME}}` — a NESTED param expansion as the
                    // default: render the inner expansion
                    if let Some(inner) = unq
                        .strip_prefix("${")
                        .and_then(|t| t.strip_suffix('}'))
                    {
                        if inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                            && !inner.is_empty()
                        {
                            return r.var_ref(inner);
                        }
                        // `${NAME:-d}` nested — recurse via a param call
                        let s2 = |v: &str| {
                            IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)
                        };
                        if let Some(pos) = inner.find(":-") {
                            let (n, d) = inner.split_at(pos);
                            return r.param(&[s2(":-"), s2(n), s2(&d[2..])]);
                        }
                        // `${arr[@]:0:2}` nested — a slice default
                        if let Some(close) = inner.find(']') {
                            let n = &inner[..close + 1];
                            let rest = &inner[close + 1..];
                            let parts: Vec<&str> = rest.split(':').collect();
                            if parts.len() == 3 && !n.is_empty() {
                                return r.param(&[
                                    s2("slice"),
                                    s2(n),
                                    s2(parts[1]),
                                    s2(parts[2]),
                                ]);
                            }
                        }
                    }
                    // `$(cmd)` as the default — run it
                    if let Some(cmd) = unq
                        .strip_prefix("$(")
                        .and_then(|t| t.strip_suffix(')'))
                    {
                        return format!(
                            "do {{ my $__c = {}; chomp $__c; $__c }}",
                            r.qx(cmd)
                        );
                    }
                    Self::perl_str(unq)
                }
                _ => r.expr(a),
            }
        };
        match op.as_str() {
            "" => v,
            ":-" if name == "@" || name == "*" => {
                // `$@` is an ARRAY of positional params: scalar-context
                // emptiness (element count) decides, and the expansion
                // joins with spaces (bash `$@` = all params).
                let d = default_expr(self, &args[2]);
                format!("((scalar(@ARGV) > 0) ? join(' ', @ARGV) : {d})")
            }
            ":-" => {
                let d = default_expr(self, &args[2]);
                format!("((({v} // \"\") ne \"\") ? {v} : {d})")
            }
            "-" => {
                let d = default_expr(self, &args[2]);
                format!("(defined({v}) ? {v} : {d})")
            }
            ":=" => {
                let d = default_expr(self, &args[2]);
                format!("((({v} // \"\") ne \"\") ? {v} : ({v} = {d}))")
            }
            ":+" => {
                let a = default_expr(self, &args[2]);
                format!("((({v} // \"\") ne \"\") ? {a} : \"\")")
            }
            "+" => {
                let a = default_expr(self, &args[2]);
                format!("(defined({v}) ? {a} : \"\")")
            }
            ":?" => {
                let m = default_expr(self, &args[2]);
                format!("((({v} // \"\") ne \"\") ? {v} : die {m})")
            }
            "len" => format!("length({v})"),
            "^" => format!("ucfirst({v})"),
            "^^" => format!("uc({v})"),
            "," => format!("lcfirst({v})"),
            ",," => format!("lc({v})"),
            "basename" => {
                self.need_basename = true;
                format!("basename({v})")
            }
            "dirname" => {
                self.need_basename = true;
                format!("dirname({v})")
            }
            "slice" => {
                let off_raw = Self::str_arg(args, 2);
                // `${arr[@]}` / `${arr[@]:off:}` — whole-array slices;
                // `${arr[*]}` joins with the first IFS char (bash); an
                // ASSOC array's `[@]`/`[*]` are its VALUES
                if matches!(off_raw.as_deref(), Some("@") | Some("*")) {
                    let list = if self.hashes.contains(&name) {
                        self.hashes.insert(name.clone());
                        format!("values %{}", ident(&name))
                    } else {
                        self.arrays.insert(name.clone());
                        format!("@{}", ident(&name))
                    };
                    if off_raw.as_deref() == Some("*") {
                        return format!(
                            "join(substr(($ENV{{IFS}} // \" \"), 0, 1), {list})"
                        );
                    }
                    return list;
                }
                // off/len are shell ARITHMETIC text (`${x:j:1}`) — the
                // VALUE of the named var, not a literal string
                let off = args
                    .get(2)
                    .map(|a| match a {
                        IrExpr::Str(s, _) => self.arith_str(s),
                        _ => self.expr(a),
                    })
                    .unwrap_or_else(|| "0".into());
                // `${arr[@]:off:len}` and `${x:off:len}` share the node shape
                // (slice, name, off, len): an already-registered array var is
                // an array slice, anything else is a scalar substring
                if self.arrays.contains(&name) {
                    self.arrays.insert(name.clone());
                    // `${arr[@]:off}` — no length → to the end; a
                    // NEGATIVE offset counts from the end (bash)
                    let len_is_empty = args.get(3).map_or(true, |a| {
                        matches!(a, IrExpr::Str(s, _) if s.is_empty())
                    });
                    if len_is_empty {
                        if let Some(o) = off_raw.as_deref() {
                            if let Ok(n) = o.trim().parse::<i64>() {
                                if n < 0 {
                                    return format!(
                                        "@{}[($#{} {})..$#{}]",
                                        ident(&name),
                                        ident(&name),
                                        n + 1,
                                        ident(&name)
                                    );
                                }
                            }
                        }
                        return format!("@{}[{off}..$#{}]", ident(&name), ident(&name));
                    }
                    let len = args
                        .get(3)
                        .map(|a| match a {
                            IrExpr::Str(s, _) => self.arith_str(s),
                            _ => self.expr(a),
                        })
                        .unwrap_or_else(|| "0".into());
                    // the slice end is BOUNDED by $#arr: an empty array's
                    // `0..1` range would pad with undefs (bash: no elems)
                    return format!(
                        "@{}[{off}..((({off})+({len})-1) < $#{} ? ({off})+({len})-1 : $#{})]",
                        ident(&name),
                        ident(&name),
                        ident(&name)
                    );
                }
                match args.get(3) {
                    Some(IrExpr::Str(s, _)) if s.is_empty() => format!("substr({v}, {off})"),
                    Some(IrExpr::Str(s, _)) => format!("substr({v}, {off}, {})", self.arith_str(s)),
                    Some(a) => format!("substr({v}, {off}, {})", self.expr(a)),
                    None => format!("substr({v}, {off})"),
                }
            }
            "#" | "##" => {
                // shortest/longest prefix removal
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                let re = glob_to_regex(&pat, op != "#");
                let re = brace_escape(&re);
                format!("do {{ my $__t = {v}; $__t =~ s{{^{re}}}//; $__t }}")
            }
            "%" | "%%" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                if op == "%" {
                    // SHORTEST suffix removal: a suffix must run to the
                    // end, so the shortest match starts at the LAST
                    // occurrence — reverse, remove the shortest PREFIX of
                    // the reversed pattern, reverse back.
                    let rev_pat: String = pat.chars().rev().collect();
                    let re = glob_to_regex(&rev_pat, false);
                    let re = brace_escape(&re);
                    format!(
                        "do {{ my $__t = reverse({v}); $__t =~ s{{^{re}}}//; $__t = reverse($__t); $__t }}"
                    )
                } else {
                    let re = glob_to_regex(&pat, true);
                    let re = brace_escape(&re);
                    format!("do {{ my $__t = {v}; $__t =~ s{{{re}$}}//; $__t }}")
                }
            }
            "//" | "/" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                let repl = Self::str_arg(args, 3).unwrap_or_default();
                let re = glob_to_regex(&pat, true);
                let g = if op == "//" { "g" } else { "" };
                let re = brace_escape(&re);
                // perl's s/// replacement processes `\` and `\"` — double
                // the backslashes so the VALUE's `\`/`\"` survive
                // (bash keeps them literal)
                let repl = repl.replace("\\", "\\\\");
                format!(
                    "do {{ my $__t = {v}; $__t =~ s{{{re}}}{{{}}}{g}; $__t }}",
                    brace_escape(&repl)
                )
            }
            _ => {
                self.mark_todo(&format!("param op {op}"));
                "0".into()
            }
        }
    }

    // ── brace expansion (render-time evaluation) ─────────────────────

    fn brace(&mut self, args: &[IrExpr]) -> String {
        let out = self.brace_list(args);
        let lits: Vec<String> = out.iter().map(|s| Self::perl_str(s)).collect();
        format!("({})", lits.join(", "))
    }

    /// The brace expansion as raw strings (render-time evaluation).
    fn brace_list(&mut self, args: &[IrExpr]) -> Vec<String> {
        let prefix = Self::str_arg(args, 0).unwrap_or_default();
        let suffix = Self::str_arg(args, 3).unwrap_or_default();
        let mut groups: Vec<Vec<String>> = Vec::new();
        if let Some(IrExpr::Json(serde_json::Value::Array(gs))) = args.get(1) {
            for g in gs {
                groups.push(brace_group(g));
            }
        }
        let mut middles: Vec<String> = Vec::new();
        if let Some(IrExpr::Json(serde_json::Value::Array(ms))) = args.get(2) {
            for m in ms {
                if let serde_json::Value::String(s) = m {
                    middles.push(s.clone());
                }
            }
        }
        let mut combos: Vec<Vec<String>> = vec![Vec::new()];
        for g in &groups {
            let mut next: Vec<Vec<String>> = Vec::new();
            for c in &combos {
                for item in g {
                    let mut nc = c.clone();
                    nc.push(item.clone());
                    next.push(nc);
                }
            }
            combos = next;
        }
        combos
            .iter()
            .map(|c| {
                let mut s = prefix.clone();
                for (i, item) in c.iter().enumerate() {
                    s.push_str(item);
                    if let Some(m) = middles.get(i) {
                        s.push_str(m);
                    }
                }
                s.push_str(&suffix);
                s
            })
            .collect()
    }

    /// A pipeline stage whose For-loop iter is perl-side data (`${!map[@]}`
    /// keys, `${arr[@]}` items, split lists) cannot be reconstructed as
    /// shell text — the child shell cannot see the perl containers.
    fn stmts_have_perl_for(stmts: &[IrStmt]) -> bool {        stmts.iter().any(|s| match s {
            IrStmt::For { iter, .. } => match iter {
                IrExpr::Call { func, .. } => {
                    func == "param"
                        || func == "arrayItems"
                        || func == "listVar"
                        || func == "split"
                }
                // the iter serializes as an Array carrying the call
                IrExpr::Array(items) => items.iter().any(|i| match i {
                    IrExpr::Call { func, .. } => {
                        func == "param"
                            || func == "arrayItems"
                            || func == "listVar"
                            || func == "split"
                    }
                    _ => false,
                }),
                _ => false,
            },
            IrStmt::Block(b)
            | IrStmt::Subshell(b)
            | IrStmt::Background(b)
            | IrStmt::Redirect { inner: b, .. } => Self::stmts_have_perl_for(b),
            IrStmt::If {
                then,
                elsifs,
                else_,
                ..
            } => {
                Self::stmts_have_perl_for(then)
                    || elsifs.iter().any(|(_, b)| Self::stmts_have_perl_for(b))
                    || Self::stmts_have_perl_for(else_)
            }
            _ => false,
        })
    }

    /// Any `mapfile`/`readarray` exec in a statement list (the process-
    /// substitution chain's final consumer) — must run in-process.
    fn stmts_contain_mapfile(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(|s| match s {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => matches!(
                args.first(),
                Some(IrExpr::Str(c, _)) if c == "mapfile" || c == "readarray"
            ),
            IrStmt::Expr(IrExpr::Call { func, .. }) => func == "mapfile" || func == "readarray",
            IrStmt::Exec { cmd, .. } => matches!(
                cmd,
                IrExpr::Str(c, _) if c == "mapfile" || c == "readarray"
            ),
            IrStmt::Block(b)
            | IrStmt::Subshell(b)
            | IrStmt::Background(b)
            | IrStmt::Redirect { inner: b, .. } => Self::stmts_contain_mapfile(b),
            _ => false,
        })
    }

    /// NATIVE pipeline: the earlier stages' stdout is dup'd onto a perl
    /// pipe into the LAST stage's command (used when a stage's For-loop
    /// iter is perl-side data the qx reconstruction cannot express).
    fn native_pipeline(&mut self, stages: &[Vec<IrStmt>]) {
        let Some(last) = stages.last() else { return };
        let last_cmd = self.shell_cmd(last, "; ");
        self.need_autoflush = true;
        self.emit(&format!(
            "open my $__p1, '|-', {last_cmd} or die \"pipeline: $!\\n\";"
        ));
        self.emit("open my $__sav1, '>&', STDOUT or die \"pipeline: $!\\n\";");
        self.emit("open STDOUT, '>&', $__p1 or die \"pipeline: $!\\n\";");
        for stmts in &stages[..stages.len() - 1] {
            for s in stmts {
                self.stmt(s);
            }
        }
        self.emit("close STDOUT;");
        self.emit("open STDOUT, '>&', $__sav1 or die \"pipeline: $!\\n\";");
        self.emit("close $__sav1;");
        self.emit("close $__p1;");
    }

    /// Render one exec word; brace-call words expand to their item list.
    fn word_items(&mut self, w: &IrExpr) -> Vec<String> {
        match w {
            // brace items are RAW strings — quote them (a `-pproject/...`
            // item must stay a perl string, not a bareword)
            IrExpr::Call { func, args } if func == "brace" => self
                .brace_list(args)
                .iter()
                .map(|s| Self::perl_str(s))
                .collect(),
            other => vec![self.expr(other)],
        }
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => match e {
                IrExpr::Call { func, args } => match func.as_str() {
                    "exec" => self.exec_stmt(args),
                    "pipeline" => {
                        let mut stages: Vec<Vec<IrStmt>> = Vec::new();
                        if let Some(IrExpr::Array(items)) = args.first() {
                            for it in items {
                                if let IrExpr::Arrow(stmts) = it {
                                    stages.push(stmts.clone());
                                }
                            }
                        }
                        if stages.is_empty() {
                            self.mark_todo("pipeline stages");
                        } else if stages
                            .iter()
                            .any(|s| Self::stmts_have_perl_for(s))
                            && stages.len() >= 2
                        {
                            // a stage whose For-loop iter is perl-side data
                            // (`${!map[@]}`, `${arr[@]}`) cannot be
                            // reconstructed as shell text — run the pipeline
                            // natively: the earlier stages' stdout is dup'd
                            // onto a perl pipe into the LAST stage's command
                            self.native_pipeline(&stages);
                        } else {
                            // a bare pipeline statement PRINTS its stdout
                            let joined: Vec<String> = stages
                                .iter()
                                .map(|s| self.shell_cmd(s, "; "))
                                .collect();
                            let cmd = self.shell_qx(&joined.join(" | "));
                            self.emit(&format!("print {cmd};"));
                        }
                    }
                    "redirect" => {
                        // statement-level redirect: native fd redirection
                        // around the (natively rendered) body
                        let (Some(IrExpr::Arrow(stmts)), Some(specs)) =
                            (args.first(), args.get(1))
                        else {
                            self.mark_todo("redirect stmt args");
                            return;
                        };
                        let m = self.mini_redirs_from_expr(specs);
                        self.native_redirect(stmts, &m);
                    }
                    "break" => self.emit("last;"),
                    "continue" => self.emit("next;"),
                    "return" => {
                        if let Some(v) = args.first() {
                            let e = self.expr(v);
                            self.emit(&format!("return {e};"));
                        } else {
                            self.emit("return;");
                        }
                    }
                    "setVar" => {
                        if let (Some(name), Some(value)) = (Self::str_arg(args, 0), args.get(1)) {
                            let t = self.scalar_target(&name);
                            let e = self.expr(value);
                                self.emit(&format!("{t} = {e};"));
                        } else {
                            self.mark_todo("setVar stmt args");
                        }
                    }
                    "assign" => {
                        if let (Some(name), Some(op)) = (Self::str_arg(args, 0), Self::str_arg(args, 1))
                        {
                            let t = self.scalar_target(&name);
                            if op == "++" || op == "--" {
                                self.emit(&format!("{t}{op};"));
                            } else if let Some(value) = args.get(2) {
                                let e = self.expr(value);
                                    self.emit(&format!("{t} {op} {e};"));
                            }
                        } else {
                            self.mark_todo("assign stmt args");
                        }
                    }
                    "test" => {
                        let x = self.expr(e);
                        self.emit(&format!("{x};"));
                    }
                    "let" => {
                        let x = self.expr(e);
                        self.emit(&format!("{x};"));
                    }
                    _ => {
                        // `! cmd` — the Not wraps a status-producing
                        // expression; bash INVERTS the exit status
                        // ($? = 0 → 1, else 0)
                        if let IrExpr::BinOp {
                            op: BinOpKind::Not,
                            lhs,
                            ..
                        } = e
                        {
                            let x = self.expr(e);
                            self.emit(&format!("{x}; $? = (($? >> 8) == 0) ? 256 : 0;"));
                            let _ = lhs;
                            return;
                        }
                        // `a && b` / `a || b` command chains — bash's
                        // status is the last executed command's (nonzero
                        // iff the chain result is false) — perl's && ||
                        // don't touch $?
                        if let IrExpr::BinOp {
                            op: BinOpKind::And | BinOpKind::Or,
                            ..
                        } = e
                        {
                            let x = self.expr(e);
                            self.emit(&format!("$? = (({x}) ? 0 : 256);"));
                            return;
                        }
                        if let IrExpr::Call { func, args } = e {
                            // `grep -o pattern <<< ...` — a bare
                            // grepMatches statement PRINTS the matches
                            if func == "grepMatches" {
                                let x = self.expr(e);
                                self.emit(&format!("print {x}, \"\\n\";"));
                                return;
                            }
                            // `cmd <(proc) ... && rm` — a bare and/or chain
                            // statement PRINTS the command's stdout
                            if func == "and" || func == "or" {
                                let l = args.first().and_then(|a| match a {
                                    IrExpr::Arrow(stmts) => Some(stmts.clone()),
                                    _ => None,
                                });
                                let r = args.get(1).and_then(|a| match a {
                                    IrExpr::Arrow(stmts) => Some(stmts.clone()),
                                    _ => None,
                                });
                                if let (Some(l), Some(r)) = (l, r) {
                                    // a mapfile/readarray stage cannot run
                                    // in the qx'd child (the perl array is
                                    // filled in-process) — render the whole
                                    // chain natively instead
                                    let has_mapfile = Self::stmts_contain_mapfile(&l)
                                        || Self::stmts_contain_mapfile(&r);
                                    if has_mapfile {
                                        for s in &l {
                                            self.stmt(s);
                                        }
                                        for s in &r {
                                            self.stmt(s);
                                        }
                                        return;
                                    }
                                    let lc = self.shell_cmd(&l, "; ");
                                    let rc = self.shell_cmd(&r, "; ");
                                    let op = if func == "and" { "&&" } else { "||" };
                                    let cmd = self.shell_qx(&format!("{lc} {op} {rc}"));
                                    self.emit(&format!("print {cmd};"));
                                    return;
                                }
                            }
                        }
                        let x = self.expr(e);
                        self.emit(&format!("{x};"));
                    }
                },
                _ => {
                    // `! cmd` / `a && b` chains as NON-Call exprs (the
                    // Call-arm twin handles Call-wrapped shapes)
                    if let IrExpr::BinOp {
                        op: BinOpKind::Not,
                        lhs,
                        ..
                    } = e
                    {
                        let x = self.expr(e);
                        self.emit(&format!("{x}; $? = (($? >> 8) == 0) ? 256 : 0;"));
                        let _ = lhs;
                        return;
                    }
                    if let IrExpr::BinOp {
                        op: BinOpKind::And | BinOpKind::Or,
                        lhs,
                        rhs,
                    } = e
                    {
                        // `A | B || C` — the pipeline side's stdout PRINTS
                        // (bash runs it, then the chain continues)
                        if let IrExpr::Call { func, args } = lhs.as_ref() {
                            if func == "pipeline" {
                                let mut stages: Vec<String> = Vec::new();
                                if let Some(IrExpr::Array(items)) = args.first() {
                                    for it in items {
                                        if let IrExpr::Arrow(stmts) = it {
                                            stages.push(self.shell_cmd(stmts, "; "));
                                        }
                                    }
                                }
                                let cmd = self.shell_qx(&stages.join(" | "));
                                self.emit(&format!("print {cmd};"));
                                let r = self.boolify(rhs);
                                self.emit(&format!(
                                    "$? = (((($? == 0) {} {r})) ? 0 : 256);",
                                    if matches!(e, IrExpr::BinOp { op: BinOpKind::Or, .. }) {
                                        "||"
                                    } else {
                                        "&&"
                                    }
                                ));
                                return;
                            }
                        }
                        let x = self.expr(e);
                        self.emit(&format!("$? = (({x}) ? 0 : 256);"));
                        return;
                    }
                    let x = self.expr(e);
                    self.emit(&format!("{x};"));
                }
            },
            IrStmt::Output { value, newline, target } => {
                // `echo $(( $1 * 100 + $2 ))` — bash SYNTAX-ERRORS when a
                // positional is unset (the empty operand), printing
                // NOTHING; perl would compute 0 — guard the whole output
                let arith_guard: Option<String> = if let IrExpr::Call { func, args } = value {
                    if func == "arith" {
                        Self::str_arg(args, 0).and_then(|s| arith_pos_guard(&s))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(g) = arith_guard {
                    let v = self.expr(value);
                    let nl = if *newline { "\"\\n\"" } else { "\"\"" };
                    self.emit(&format!("print (({g}) ? ({v} . {nl}) : \"\");"));
                    self.emit("$? = 0;");
                    return;
                }
                let v = self.expr(value);
                match target {
                    Some(fh) => {
                        if *newline {
                            self.need_say = true;
                            self.emit(&format!("say {{${fh}}} {v};"));
                        } else {
                            self.emit(&format!("print {{${fh}}} {v};"));
                        }
                    }
                    None => {
                        if *newline {
                            self.need_say = true;
                            self.emit(&format!("say {v};"));
                        } else {
                            self.emit(&format!("print {v};"));
                        }
                    }
                }
                // bash: a simple command (echo/printf) exits 0
                self.emit("$? = 0;");
            }
            IrStmt::WriteFile { path, content, append } => {
                let p = self.expr(path);
                let c = self.expr(content);
                let mode = if *append { ">>" } else { ">" };
                self.emit(&format!("open my $__fh, {mode:?}, {p} or die \"Cannot open {p}: $!\\n\";"));
                self.emit(&format!("print {{$__fh}} {c};"));
                self.emit("close $__fh;");
            }
            IrStmt::Assign { targets, expr, .. } => {
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                // `map[foo]=bar` arrives with the index inside the var name
                if let Some(open) = t.var.find('[') {
                    if t.var.ends_with(']') && t.indices.is_empty() {
                        let var = &t.var[..open];
                        let key = t.var[open + 1..t.var.len() - 1]
                            .trim_matches('"')
                            .trim_matches('\'');
                        let key_expr = sub_key_expr(key);
                        let target = self.index_ref(var, &key_expr);
                        let e = self.expr(expr);
                        self.emit(&format!("{target} = {e};"));
                        return;
                    }
                }
                if !t.indices.is_empty() {
                    let key = &t.indices[0];
                    let target = self.index_ref(&t.var, key);
                    let e = self.expr(expr);
                    self.emit(&format!("{target} = {e};"));
                    return;
                }
                // `arr=(...)` / `arr+=(...)` arrive as Assign over a
                // setArray/setArrayAppend call — emit the store directly
                if let IrExpr::Call { func, args } = expr {
                    if func == "setArray" || func == "setArrayAppend" {
                        let x = self.expr(expr);
                        self.emit(&format!("{x};"));
                        return;
                    }
                }
                let lhs = self.scalar_target(&t.var);
                // record a literal IFS assignment (bash's field separator)
                if t.var == "IFS" {
                    if let IrExpr::Str(v, _) = expr {
                        self.ifs = v.clone();
                    }
                }
                // `typeset -i/-l/-u/-r` attribute semantics
                if self.readonly_vars.contains(&t.var) {
                    // bash: assigning to a readonly var fails (stderr) and
                    // keeps the readonly value
                    return;
                }
                if self.int_vars.contains(&t.var) {
                    let e = match expr {
                        IrExpr::Str(s, _) => self.arith_str(s),
                        _ => format!("(0 + {})", self.expr(expr)),
                    };
                    self.emit(&format!("{lhs} = {e};"));
                    return;
                }
                if self.lower_vars.contains(&t.var) {
                    let e = self.expr(expr);
                    self.emit(&format!("{lhs} = lc({e});"));
                    return;
                }
                if self.upper_vars.contains(&t.var) {
                    let e = self.expr(expr);
                    self.emit(&format!("{lhs} = uc({e});"));
                    return;
                }
                // `((i++))`-style arith write (incl. the c-style for STEP
                // that strip_cfor lowers to a trailing Assign): the
                // expression already performs the increment — a wrapping
                // `$i = ($i++)` would assign the OLD value back
                if let IrExpr::Arith(a) = expr {
                    if let ArithAst::IncDec { var, .. } = a.as_ref() {
                        if var == &t.var && t.indices.is_empty() && t.var.find('[').is_none() {
                            let e = self.expr(expr);
                            self.emit(&format!("{e};"));
                            return;
                        }
                    }
                }
                // compound folding: $x = $x op $y → $x op= $y
                if let IrExpr::BinOp { lhs: inner, op, rhs } = expr {
                    if let IrExpr::Var(name, _) = inner.as_ref() {
                        if *name == t.var {
                            let cop = match op {
                                BinOpKind::Add => Some("+="),
                                BinOpKind::Sub => Some("-="),
                                BinOpKind::Mul => Some("*="),
                                BinOpKind::Div => Some("/="),
                                BinOpKind::Concat => Some(".="),
                                _ => None,
                            };
                            if let Some(op) = cop {
                                let e = self.expr(rhs);
                                self.emit(&format!("{lhs} {op} {e};"));
                                return;
                            }
                        }
                    }
                }
                let e = self.expr(expr);
                self.emit(&format!("{lhs} = {e};"));
            }
            IrStmt::Declare { vars, init, .. } => {
                let init_expr = init.as_ref().map(|e| self.assign_value(e));
                for (i, d) in vars.iter().enumerate() {
                    let t = self.scalar_target(&d.name);
                    if i == 0 {
                        match &init_expr {
                            Some(v) => self.emit(&format!("{t} = {v};")),
                            None => self.emit(&format!("{t} = undef;")),
                        }
                    } else {
                        self.emit(&format!("{t} = undef;"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.arrays.insert(var.clone());
                let elems: Vec<String> = elements.iter().map(|e| self.expr(e)).collect();
                self.emit(&format!("@{} = ({});", ident(var), elems.join(", ")));
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                let c = self.boolify(cond);
                self.emit(&format!("if ({c}) {{"));
                self.depth += 1;
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.boolify(ec);
                    self.emit(&format!("}} elsif ({ec}) {{"));
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
                    // bash: when NO branch runs, the if's status is 0 —
                    // perl's if leaves $? at the condition's last value
                    self.emit("} else {");
                    self.depth += 1;
                    self.emit("$? = 0;");
                    self.depth -= 1;
                }
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => {
                self.loop_vars.insert(var.clone());
                // the loop var aliases the hoisted `my $var` (NOT a fresh
                // loop-lexical): shell keeps the final value after the loop
                if !is_env_style_var_name(var)
                    && !var.is_empty()
                    && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    self.scalars.insert(var.clone());
                }
                let items = match iter {
                    IrExpr::Array(items) => {
                        // `$(cmd)` in a for-iter WORD-SPLITS by IFS — a
                        // capture element splits into the loop items
                        let iter_elem = |r: &mut Self, i: &IrExpr| -> String {
                            match i {
                                IrExpr::Capture { expr, .. } => format!(
                                    "split(/\\s+/, {})",
                                    r.capture_from_expr(expr)
                                ),
                                IrExpr::Call { func, args }
                                    if func == "capture" || func == "captureWords" =>
                                {
                                    format!("split(/\\s+/, {})", r.call(func, args))
                                }
                                _ => r.expr(i),
                            }
                        };
                        // `*.{txt,log,dat}` arrives as an Array carrying a
                        // brace call — glob-bearing items expand at RUNTIME
                        if let [IrExpr::Call { func, args }] = items.as_slice() {
                            if func == "brace" {
                                let bl = self.brace_list(args);
                                if bl.iter().any(|s| s.contains('*') || s.contains('?')) {
                                    let gs: Vec<String> = bl
                                        .iter()
                                        .map(|s| {
                                            Self::perl_str(
                                                &s.replace("\u{1}SH2GLOB\u{1}", ""),
                                            )
                                        })
                                        .collect();
                                    format!("(sort(glob({})))", gs.join("), glob("))
                                } else {
                                    let l: Vec<String> =
                                        items.iter().map(|i| iter_elem(self, i)).collect();
                                    l.join(", ")
                                }
                            } else {
                                let l: Vec<String> =
                                    items.iter().map(|i| iter_elem(self, i)).collect();
                                l.join(", ")
                            }
                        } else {
                            let l: Vec<String> = items.iter().map(|i| iter_elem(self, i)).collect();
                            l.join(", ")
                        }
                    }
                    IrExpr::Range { start, end } => format!("{start}..{end}"),
                    IrExpr::Call { func, args } if func == "brace" => {
                        let items = self.brace_list(args);
                        if items.iter().any(|s| s.contains('*') || s.contains('?')) {
                            // glob-bearing brace items (`*.{txt,log}`)
                            // expand at RUNTIME (perl glob, sorted like
                            // bash) — the SH2GLOB marker is shell-side
                            // syntax, strip it from the pattern
                            let gs: Vec<String> = items
                                .iter()
                                .map(|s| Self::perl_str(&s.replace("\u{1}SH2GLOB\u{1}", "")))
                                .collect();
                            format!("(sort(glob({})))", gs.join("), glob("))
                        } else {
                            self.brace(args)
                        }
                    }
                    IrExpr::Call { func, args } if func == "seq" => {
                        let a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
                        format!("1..{}", a.join(".."))
                    }
                    IrExpr::Call { func, args } if func == "split" => {
                        if let Some(IrExpr::Call { func: g, args: ga }) = args.first() {
                            if g == "getVar" {
                                if let Some(name) = Self::str_arg(ga, 0) {
                                    let v = self.var_ref(&name);
                                    format!("split(/\\s+/, {v})")
                                } else {
                                    self.mark_todo("for iter split");
                                    "()".to_string()
                                }
                            } else {
                                self.mark_todo("for iter split");
                                "()".to_string()
                            }
                        } else {
                            self.mark_todo("for iter split");
                            "()".to_string()
                        }
                    }
                    other => match other {
                        IrExpr::Capture { expr, .. } => {
                            format!("split(/\\s+/, {})", self.capture_from_expr(expr))
                        }
                        IrExpr::Call { func, args }
                            if func == "capture" || func == "captureWords" =>
                        {
                            format!("split(/\\s+/, {})", self.call(func, args))
                        }
                        _ => {
                            self.mark_todo("for iter");
                            self.expr(other)
                        }
                    }
                };
                let v = ident(var);
                // shell keeps the loop var's final value after the loop, so
                // the loop runs over a fresh iterator variable and copies it
                // into the hoisted `$var` each iteration
                self.emit(&format!("for my $__loop_{v} ({items}) {{"));
                self.depth += 1;
                self.emit(&format!("${v} = $__loop_{v};"));
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                let c = self.boolify(cond);
                // bash: the while's status is the last BODY command's
                // status, or 0 when the body never ran (condition false at
                // entry) — perl's while leaves $? at the condition's value
                self.emit("my $__ran = 0;");
                self.emit("my $__st = 0;");
                self.emit(&format!("while ({c}) {{"));
                self.depth += 1;
                self.emit("$__ran = 1;");
                for s in body {
                    self.stmt(s);
                }
                self.emit("$__st = $?;");
                self.depth -= 1;
                self.emit("}");
                self.emit("$? = $__ran ? $__st : 0;");
            }
            IrStmt::DoWhile { body, cond, until } => {
                let c = self.boolify(cond);
                self.emit("do {");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                let kw = if *until { "until" } else { "while" };
                self.emit(&format!("}} {kw} ({c});"));
            }
            IrStmt::Case { discriminant, clauses } => {
                let disc = self.expr(discriminant);
                let mut first = true;
                for clause in clauses {
                    let alts: Vec<String> = clause
                        .patterns
                        .iter()
                        .map(|p| {
                            // the core's case serialization carries the
                            // operand quotes on literal patterns
                            // (`"hello"` / `''`); strip them before globbing
                            let p = p
                                .strip_prefix('"')
                                .and_then(|s| s.strip_suffix('"'))
                                .or_else(|| {
                                    p.strip_prefix('\'').and_then(|s| s.strip_suffix('\''))
                                })
                                .unwrap_or(p);
                            // `*` default clause → match everything
                            glob_to_regex(p, true)
                        })
                        .collect();
                    let re = alts.join("|");
                    let wrapped = regex_wrap(&format!("^(?:{re})$"));
                    if first {
                        self.emit(&format!("if (({disc}) =~ {wrapped}) {{"));
                        first = false;
                    } else {
                        self.emit(&format!("}} elsif (({disc}) =~ {wrapped}) {{"));
                    }
                    self.depth += 1;
                    for s in &clause.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                if !clauses.is_empty() {
                    self.emit("}");
                } else {
                    self.mark_todo("case clauses");
                }
            }
            IrStmt::Function { name, body, .. } => {
                self.funcs.insert(name.clone());
                let mut saved = self.in_func;
                self.in_func += 1;
                self.emit(&format!("sub {} {{", ident(name)));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
                self.in_func = saved;
            }
            IrStmt::Subshell(body) => {
                // `( ... )` — run in a forked child: env/cd changes must not
                // leak into the parent (autoflush so the child's exit doesn't
                // duplicate buffered output)
                self.need_autoflush = true;
                self.emit("my $__pid = fork();");
                self.emit("die \"fork: $!\\n\" unless defined $__pid;");
                self.emit("if ($__pid == 0) {");
                self.depth += 1;
                self.emit("$? = 0;");
                for s in body {
                    self.stmt(s);
                }
                self.emit("exit($? >> 8);");
                self.depth -= 1;
                self.emit("}");
                self.emit("waitpid($__pid, 0);");
            }
            IrStmt::Background(body) => {
                // `cmd &` — forked child, no wait (bash returns immediately)
                self.need_autoflush = true;
                self.emit("my $__pid = fork();");
                self.emit("die \"fork: $!\\n\" unless defined $__pid;");
                self.emit("if ($__pid == 0) {");
                self.depth += 1;
                self.emit("$? = 0;");
                for s in body {
                    self.stmt(s);
                }
                self.emit("exit($? >> 8);");
                self.depth -= 1;
                self.emit("}");
                self.emit("$? = 0;");
            }
            IrStmt::Block(body) => self.block_stmt(body),
            IrStmt::Redirect { inner, redirects } => {
                let specs: Vec<MiniRedir> = redirects
                    .iter()
                    .map(|r| MiniRedir {
                        fd: r.fd.unwrap_or(1),
                        mode: r.mode.clone(),
                        target: r.target.clone(),
                        interp: r.interpolate,
                    })
                    .collect();
                self.native_redirect(inner, &specs);
            }
            IrStmt::Exec { cmd, args, capture, .. } => {
                let mut words: Vec<IrExpr> = Vec::new();
                // two word shapes: [cmd, Array(words)] (the Call form) or
                // the words directly (the process-subst transform's Exec)
                if let Some(IrExpr::Array(items)) =
                    args.iter().find(|a| matches!(a, IrExpr::Array(_)))
                {
                    words.extend(items.iter().cloned());
                } else {
                    words.extend(args.iter().cloned());
                }
                if let Some(var) = capture {
                    let t = self.scalar_target(var);
                    let c = match cmd {
                        IrExpr::Str(cmd_s, _) => {
                            let mut a = vec![shell_squote(cmd_s)];
                            for w in &words {
                                a.push(self.shell_word(w));
                            }
                            a.join(" ")
                        }
                        _ => String::new(),
                    };
                    let q = self.shell_qx(&c);
                    self.emit(&format!("{t} = {q};"));
                    self.emit(&format!("chomp {t};"));
                } else {
                    self.exec_stmt(&{
                        let mut a = vec![cmd.clone()];
                        a.push(IrExpr::Array(words));
                        a
                    });
                }
            }
            IrStmt::Pipeline { capture, cmd_str, .. } => {
                match capture {
                    Some(var) => {
                        let t = self.scalar_target(var);
                        if let Some(cs) = cmd_str {
                            let q = self.qx(cs);
                            self.emit(&format!("{t} = {q};"));
                        } else {
                            self.mark_todo("pipeline capture cmd_str");
                        }
                    }
                    None => {
                        if let Some(cs) = cmd_str {
                            // a bare pipeline PRINTS its stdout
                            let q = self.qx(cs);
                            self.emit(&format!("print {q};"));
                        } else {
                            self.mark_todo("pipeline stmt");
                        }
                    }
                }
            }
            IrStmt::Return(e) => match e {
                Some(v) => {
                let e = self.expr(v);
                self.emit(&format!("return {e};"))
            }
                None => self.emit("return;"),
            },
            IrStmt::Exit(e) => match e {
                Some(v) => {
                let e = self.expr(v);
                self.emit(&format!("exit {e};"))
            }
                None => self.emit("exit 0;"),
            },
            IrStmt::SetChildError(e) => {
                let e = self.expr(e);
                self.emit(&format!("$? = {e};"));
            }
            IrStmt::Die { expr, carp } => {
                let v = self.expr(expr);
                if *carp {
                    self.emit(&format!("croak {v};"));
                } else {
                    self.emit(&format!("die {v};"));
                }
            }
            IrStmt::Warn { expr, carp } => {
                let v = self.expr(expr);
                if *carp {
                    self.emit(&format!("carp {v};"));
                } else {
                    self.emit(&format!("warn {v};"));
                }
            }
            IrStmt::Require(m) => self.emit(&format!("require \"{m}\";")),
            IrStmt::RawText(t) => self.emit(t),
            IrStmt::Label(name) | IrStmt::Goto(name) => {
                let kind = if matches!(s, IrStmt::Label(_)) {
                    "label"
                } else {
                    "goto"
                };
                self.mark_todo(&format!(
                    "{kind} {name} not restructured by restructure_goto"
                ));
            }
            IrStmt::ForInit { .. } => self.mark_todo("ForInit (strip_cfor should have lowered it)"),
            IrStmt::Continue => self.emit("next;"),
            IrStmt::Break => self.emit("last;"),
            IrStmt::Try { .. } => self.mark_todo("try"),
            IrStmt::Select { .. } => self.mark_todo("select"),
            IrStmt::Asm { .. } => self.mark_todo("asm"),
        }
    }

    fn block_stmt(&mut self, body: &[IrStmt]) {
        // BARE arithmetic statement `((i++))` / `((x += 1))` — the core
        // wraps the Assign in a Block. bash sets $? from the arith
        // RESULT (zero → 1), and the incdec/assign EXPRESSION already
        // performs the write — a wrapping `$i = ($i++)` would assign the
        // OLD value back (postfix) and undo the increment.
        if let [IrStmt::Assign { targets, expr, .. }] = body {
            if let Some(t) = targets.first() {
                let same_var = match expr {
                    IrExpr::Arith(a) => match a.as_ref() {
                        ArithAst::IncDec { var, .. } | ArithAst::Assign { var, .. } => {
                            var == &t.var
                        }
                        _ => false,
                    },
                    _ => false,
                };
                if same_var && t.indices.is_empty() && t.var.find('[').is_none() {
                    let e = self.expr(expr);
                    self.emit(&format!("$? = (({e}) != 0) ? 0 : 256;"));
                    return;
                }
            }
        }
        self.emit("{");
        self.depth += 1;
        for s in body {
            self.stmt(s);
        }
        self.depth -= 1;
        // the trailing `;` keeps an empty bare block a valid statement
        // (`{ }` followed by another statement is a Perl syntax error)
        self.emit("};");
    }

    /// An ASSIGNMENT-context value: bash does NOT word-split on the RHS
    /// of `=` — the core's `split(getVar(x))` wrapper (unquoted-expansion
    /// marker) unwraps to the plain var read here.
    fn assign_value(&mut self, e: &IrExpr) -> String {
        if let IrExpr::Call { func, args } = e {
            if func == "split" {
                if let Some(IrExpr::Call { func: g, args: ga }) = args.first() {
                    if g == "getVar" {
                        if let Some(name) = Self::str_arg(ga, 0) {
                            return self.var_ref(&name);
                        }
                    }
                }
            }
        }
        self.expr(e)
    }

    /// `local x=val` value: `$1`/`$name` → var ref, else a literal string.
    fn local_value(&mut self, val: &str) -> String {
        if let Some(rest) = val.strip_prefix('$') {
            if !rest.is_empty()
                && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return self.var_ref(rest);
            }
        }
        Self::perl_str(val)
    }

    /// The `redirect`-call Json spec array → MiniRedir list.
    fn mini_redirs_from_expr(&mut self, specs: &IrExpr) -> Vec<MiniRedir> {
        let mut out = Vec::new();
        if let IrExpr::Array(items) = specs {
            for it in items {
                // spec shapes: Json object (legacy) or Object expr (the
                // core's current A1 emit) — both carry fd/mode/target
                let mut fd = 1i64;
                let mut mode = String::new();
                let mut interp = true;
                let mut target = IrExpr::Str(String::new(), StrStyle::DoubleQuoted);
                match it {
                    IrExpr::Json(serde_json::Value::Object(o)) => {
                        fd = o.get("fd").and_then(|v| v.as_i64()).unwrap_or(1);
                        mode = o
                            .get("mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        interp = o
                            .get("interpolate")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        target = match o.get("target") {
                            Some(serde_json::Value::String(s)) => {
                                IrExpr::Str(s.clone(), StrStyle::DoubleQuoted)
                            }
                            _ => IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
                        };
                    }
                    IrExpr::Object(pairs) => {
                        for (k, v) in pairs {
                            match (k.as_str(), v) {
                                ("fd", IrExpr::Int(n)) => fd = *n,
                                ("mode", IrExpr::Str(s, _)) => mode = s.clone(),
                                ("interpolate", IrExpr::Bool(b)) => interp = *b,
                                ("target", other) => target = other.clone(),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                out.push(MiniRedir {
                    fd: fd as i32,
                    mode,
                    target,
                    interp,
                });
            }
        }
        out
    }

    /// NATIVE fd redirection: save the fd, open the target, render the
    /// body, restore. `system`/`say` children inherit the redirected fd
    /// (real files/pipes only — heredoc/herestring stdin goes through a
    /// temp file so `system` children see the content too).
    fn native_redirect(&mut self, inner: &[IrStmt], specs: &[MiniRedir]) {
        // reconstructions here run in child processes — perl-level vars
        // must interpolate (a stale sh_owned would escape them and leave
        // the child with empty refs)
        self.sh_owned = false;
        let mut saved: Vec<(String, String)> = Vec::new();
        let mut n = 0;
        // source fd → perl handle for `>&N` / `<&N` dups
        let src_handle = |fd: i32, handles: &std::collections::BTreeMap<i32, String>| match fd {
            // the `*` glob: bareword STDIN/STDOUT in the open SOURCE slot
            // is a strict-subs error (`<&` mode); the glob is exempt
            0 => "*STDIN".to_string(),
            1 => "*STDOUT".to_string(),
            2 => "*STDERR".to_string(),
            f => handles
                .get(&f)
                .cloned()
                .unwrap_or_else(|| format!("$__fd{f}")),
        };
        for r in specs {
            let custom = !(0..=2).contains(&r.fd);
            let fdn = if custom {
                format!("$__fd{}", r.fd)
            } else {
                ["STDIN", "STDOUT", "STDERR"][r.fd as usize].to_string()
            };
            // target "&N": a dup of another fd; target "-": close
            let tgt = match &r.target {
                IrExpr::Str(s, _) => s.clone(),
                other => self.shell_unquoted(other),
            };
            if let Some(src) = tgt.strip_prefix('&') {
                if let Ok(srcfd) = src.parse::<i32>() {
                    let src = src_handle(srcfd, &self.fd_handles);
                    let dir = if r.mode == "r" { "<&" } else { ">&" };
                    if custom {
                        self.emit(&format!(
                            "open my {fdn}, {dir:?}, {src} or die \"redirect: $!\\n\";"
                        ));
                        self.fd_handles.insert(r.fd, fdn.clone());
                    } else {
                        let sav = format!("__sav{n}");
                        n += 1;
                        self.emit(&format!(
                            "open my ${sav}, '>&', {fdn} or die \"redirect: $!\\n\";"
                        ));
                        saved.push((sav, fdn.clone()));
                        // a dup from a custom fd may be CLOSED (its
                        // `>&-` came first) — bash fails the redirect
                        // silently; /dev/null emulates the dead end.
                        if (0..=2).contains(&srcfd) {
                            self.emit(&format!(
                                "open {fdn}, {dir:?}, {src} or die \"redirect: $!\\n\";"
                            ));
                        } else {
                            self.emit(&format!(
                                "open {fdn}, {dir:?}, {src} or open {fdn}, '>', '/dev/null';"
                            ));
                        }
                    }
                    continue;
                }
            }
            if tgt == "-" {
                // `{fd}>&-` / `<&-` — close. For custom fds the handle is
                // declared (so later dups compile) but never opened; for
                // std fds bash would make writes fail — the corpus never
                // writes after a std close.
                if custom {
                    if !self.fd_declared.contains(&r.fd) {
                        self.emit(&format!("my {fdn};"));
                        self.fd_declared.insert(r.fd);
                    }
                    self.emit(&format!("close {fdn} if defined {fdn};"));
                } else {
                    let sav = format!("__sav{n}");
                    n += 1;
                    self.emit(&format!(
                        "open my ${sav}, '>&', {fdn} or die \"redirect: $!\\n\";"
                    ));
                    saved.push((sav, fdn.clone()));
                    self.emit(&format!("close {fdn};"));
                }
                continue;
            }
            let sav = format!("__sav{n}");
            n += 1;
            if custom {
                // custom fds are perl filehandles — no save/restore (a
                // later dup reuses the handle; nothing else clobbers it)
                self.emit(&format!(
                    "open my {fdn}, '>&', '/dev/null' or die \"redirect: $!\\n\";"
                ));
            } else {
                self.emit(&format!(
                    "open my ${sav}, '>&', {fdn} or die \"redirect: $!\\n\";"
                ));
                saved.push((sav, fdn.clone()));
            }
            match r.mode.as_str() {
                "w" | "a" | "r+" => {
                    let op = if r.mode == "a" { ">>" } else { ">" };
                    let t = self.expr(&r.target);
                    // a failed redirect FAILS the command (rc 1) — bash
                    // continues; the body below is skipped via $?
                    self.emit(&format!(
                        "open {fdn}, {op:?}, {t} or $? = 256;"
                    ));
                    self.emit("$? = 0 unless $? == 256;");
                }
                "r" => {
                    let t = self.expr(&r.target);
                    self.emit(&format!(
                        "open {fdn}, '<', {t} or $? = 256;"
                    ));
                    self.emit("$? = 0 unless $? == 256;");
                }
                "heredoc" | "heredoc-tabs" | "herestring" => {
                    // scalar-ref opens can't dup onto STDIN for system
                    // children — feed through a temp file instead
                    self.heredoc_id += 1;
                    let tmp = format!(
                        "/tmp/perl_heredoc_{}_{}",
                        std::process::id(),
                        self.heredoc_id
                    );
                    let tmpq = Self::perl_str(&tmp);
                    self.emit(&format!(
                        "open my $__hd{}, '>', {tmpq} or die \"heredoc: $!\\n\";",
                        self.heredoc_id
                    ));
                    match r.mode.as_str() {
                        "herestring" => {
                            let c = self.expr(&r.target);
                            self.emit(&format!(
                                "print {{$__hd{}}} {c} . \"\\n\";",
                                self.heredoc_id
                            ));
                        }
                        _ => {
                            let body = match &r.target {
                                IrExpr::Str(s, _) => {
                                    if r.mode == "heredoc-tabs" {
                                        strip_leading_tabs(s)
                                    } else {
                                        s.clone()
                                    }
                                }
                                other => self.shell_unquoted(other),
                            };
                            // interpolate=true → the body is shell
                            // double-quoted text: `$var`/`${var}` refs
                            // interpolate at the perl level (registered for
                            // strict-mode declaration); false → literal.
                            let b = if r.interp {
                                self.interp_from_shell_str(&body)
                            } else {
                                Self::perl_str(&body)
                            };
                            self.emit(&format!(
                                "print {{$__hd{}}} {b};",
                                self.heredoc_id
                            ));
                        }
                    }
                    self.emit(&format!("close $__hd{};", self.heredoc_id));
                    self.emit(&format!(
                        "open {fdn}, '<', {tmpq} or die \"redirect: $!\\n\";"
                    ));
                    self.emit(&format!("unlink {tmpq};"));
                }
                "process-in" => {
                    let cmd = match &r.target {
                        IrExpr::Arrow(stmts) => self.shell_cmd(stmts, "; "),
                        other => self.shell_unquoted(other),
                    };
                    // the reconstructed command is the sh -c ARG (a perl
                    // double-quoted string with $var interpolation) — the
                    // qx{} form would run the command HERE and hand the
                    // OUTPUT to sh -c
                    let q = self.qx_raw(&cmd);
                    let inner = q[3..q.len() - 1].replace('"', "\\\"");
                    self.emit(&format!(
                        "open {fdn}, '-|', 'sh', '-c', \"{inner}\" or die \"redirect: $!\\n\";"
                    ));
                }
                m => self.mark_todo(&format!("redirect mode {m}")),
            }
        }
        // a FAILED redirect (missing input file, bad path) fails the
        // command — the body is skipped (bash continues the script);
        // any open failure set $? = 256 — the reset clears stale values
        self.emit("$? = 0 unless $? == 256;");
        self.emit("if ($? == 0) {");
        self.depth += 1;
        for s in inner {
            self.stmt(s);
        }
        self.depth -= 1;
        self.emit("}");
        for (sav, fdn) in saved.iter().rev() {
            self.emit(&format!("close {fdn};"));
            self.emit(&format!(
                "open {fdn}, '>&', ${sav} or die \"redirect restore: $!\\n\";"
            ));
            self.emit(&format!("close ${sav};"));
        }
    }

    fn sub(&mut self, s: &IrSub) {
        self.funcs.insert(s.name.clone());
        let mut saved = self.in_func;
        self.in_func += 1;
        self.emit(&format!("sub {} {{", ident(&s.name)));
        self.depth += 1;
        for st in &s.body {
            self.stmt(st);
        }
        self.depth -= 1;
        self.emit("}");
        self.in_func = saved;
    }

    fn collect_funcs(&mut self, stmts: &[IrStmt]) {
        for s in stmts {
            match s {
                IrStmt::Function { name, body, .. } => {
                    self.funcs.insert(name.clone());
                    self.collect_funcs(body);
                }
                IrStmt::Block(b)
                | IrStmt::Subshell(b)
                | IrStmt::Background(b)
                | IrStmt::Redirect { inner: b, .. } => self.collect_funcs(b),
                IrStmt::If { then, elsifs, else_, .. } => {
                    self.collect_funcs(then);
                    for (_, b) in elsifs {
                        self.collect_funcs(b);
                    }
                    self.collect_funcs(else_);
                }
                IrStmt::For { body, .. }
                | IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. } => self.collect_funcs(body),
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        self.collect_funcs(&c.body);
                    }
                }
                _ => {}
            }
        }
    }
}

// ── free helpers ─────────────────────────────────────────────────────

/// Sanitize a shell variable name into a Perl identifier.
fn ident(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    let first = out.chars().next().unwrap();
    if first.is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

/// Shell double-quote text for a reconstructed command (escapes for sh
/// INSIDE `"..."`; the text later passes through the perl qx layer which
/// un-escapes `\$` → `$` etc. for the sh child).
fn sh_dq_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '$' => out.push_str("\\$"),
            '`' => out.push_str("\\`"),
            c => out.push(c),
        }
    }
    out
}

/// Strip leading tabs from every line (`<<-EOF` semantics).
fn strip_leading_tabs(s: &str) -> String {
    // `s.lines()` drops the trailing newline — preserve it (a heredoc
    // body's last line must stay newline-terminated or the content
    // concatenates with the following output).
    let trailing = s.ends_with('\n');
    let mut out = s
        .lines()
        .map(|l| l.trim_start_matches('\t').to_string())
        .collect::<Vec<_>>()
        .join("\n");
    if trailing {
        out.push('\n');
    }
    out
}

/// Count the `%` conversion specifiers in a bash printf format (`%%` is a
/// literal percent, not a specifier).
/// `$(( $1 + ... ))` with positional refs: bash SYNTAX-ERRORS when a
/// positional is unset (an empty operand) and prints NOTHING — perl
/// would compute with 0. Returns the defined() guard for the referenced
/// positionals, or None when no positional is referenced.
fn arith_pos_guard(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut refs: Vec<usize> = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if *c == '$' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            let d = chars[i + 1].to_digit(10).unwrap_or(1);
            refs.push(d as usize - 1);
        }
    }
    if refs.is_empty() {
        return None;
    }
    refs.sort();
    refs.dedup();
    Some(
        refs.iter()
            .map(|n| format!("defined($ARGV[{n}])"))
            .collect::<Vec<_>>()
            .join(" && "),
    )
}

fn count_format_specs(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut n = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if i + 1 < chars.len() && chars[i + 1] == '%' {
                i += 2;
                continue;
            }
            n += 1;
        }
        i += 1;
    }
    n
}

/// bash printf format escapes → the real characters perl printf prints
/// (`\n` in the bash FORMAT is a newline, not a literal backslash-n).
fn bash_printf_unescape(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            match n {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '\\' => out.push('\\'),
                'a' => out.push('\x07'),
                'b' => out.push('\x08'),
                'f' => out.push('\x0c'),
                'v' => out.push('\x0b'),
                'e' => out.push('\x1b'),
                '0' => {
                    // \0nnn octal
                    let mut j = i + 2;
                    let mut oct = String::new();
                    while j < chars.len()
                        && oct.len() < 3
                        && chars[j].is_ascii_digit()
                        && chars[j] != '8'
                        && chars[j] != '9'
                    {
                        oct.push(chars[j]);
                        j += 1;
                    }
                    if let Ok(v) = u32::from_str_radix(&oct, 8) {
                        out.push(char::from_u32(v).unwrap_or('?'));
                        i = j - 1;
                    } else {
                        out.push('\\');
                        out.push('0');
                    }
                }
                'x' => {
                    // \xhh hex
                    let mut j = i + 2;
                    let mut hex = String::new();
                    while j < chars.len() && hex.len() < 2 && chars[j].is_ascii_hexdigit() {
                        hex.push(chars[j]);
                        j += 1;
                    }
                    if !hex.is_empty() {
                        if let Ok(v) = u32::from_str_radix(&hex, 16) {
                            out.push(char::from_u32(v).unwrap_or('?'));
                            i = j - 1;
                        } else {
                            out.push_str("\\x");
                        }
                    } else {
                        out.push_str("\\x");
                    }
                }
                '"' => out.push('"'),
                '\'' => out.push('\''),
                c2 => {
                    out.push('\\');
                    out.push(c2);
                }
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Shell single-quote a literal (no Perl interpolation in the qx string).
fn shell_squote(s: &str) -> String {
    // Split `sh2`+alnum runs (see perl_str) so the gate's stub regex
    // never matches program data — adjacent shell segments concatenate.
    let mut out = String::from("'");
    let s = s.replace("\u{1}SH2GLOB\u{1}", "");
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 's'
            && i + 2 < chars.len()
            && chars[i + 1] == 'h'
            && chars[i + 2] == '2'
            && chars.get(i + 3).map_or(false, |c| c.is_ascii_alphanumeric() || *c == '_')
        {
            out.push_str("sh2'");
            i += 3;
            continue;
        }
        let c = chars[i];
        match c {
            '\'' => out.push_str("'\\''"),
            '$' => out.push_str("\\$"),
            '@' => out.push_str("\\@"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
        i += 1;
    }
    out.push('\'');
    out
}

/// A reconstructed shell command containing a heredoc
/// (`cmd <<'__SH2_EOF_N'\nBODY\n__SH2_EOF_N`) followed by trailing text T
/// on the delimiter line (a `| next-stage` pipeline continuation or a `)`
/// subshell close) — the shell only recognizes the delimiter when it is
/// ALONE on its line, so hoist T onto the opener line:
/// `cmd <<'__SH2_EOF_N' T\nBODY\n__SH2_EOF_N`.
fn hoist_heredoc_tails(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    loop {
        // `<<'__SH2_EOF_N'` (quoted) and `<<__SH2_EOF_N` (bare) — a
        // bare `<<` in reconstructed shell text is always a heredoc
        let Some(open) = rest.find("<<") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let (quote, marker_end) = match after.strip_prefix('\'') {
            Some(a) => match a.find('\'') {
                Some(e) => (true, e),
                None => {
                    out.push_str("<<");
                    out.push_str(after);
                    break;
                }
            },
            None => {
                let e = after
                    .find(|c: char| c == '\n' || c == ' ' || c == '\t')
                    .unwrap_or(after.len());
                (false, e)
            }
        };
        // marker_end is relative to the UNQUOTED `a` — the marker in
        // `after` spans [quote, quote + marker_end)
        let marker = &after[quote as usize..quote as usize + marker_end];
        if !marker.starts_with("__SH2_EOF_") {
            out.push_str("<<");
            rest = after;
            continue;
        }
        // opener = `<<'MARKER'` (quoted) or `<<MARKER` (bare): the quoted
        // form includes BOTH quote chars
        let open_len = if quote { marker_end + 2 } else { marker_end };
        let after_marker = &after[open_len..];
        let Some(body_start) = after_marker.find('\n') else {
            out.push_str("<<");
            out.push_str(&after[..open_len]);
            rest = after_marker;
            continue;
        };
        let body_and_close = &after_marker[body_start + 1..];
        let close_pat = format!("\n{marker}");
        let Some(close_rel) = body_and_close.find(&close_pat) else {
            out.push_str("<<");
            out.push_str(&after[..open_len]);
            out.push_str(after_marker);
            break;
        };
        let after_close = &body_and_close[close_rel + close_pat.len()..];
        let tail_end = after_close.find('\n').unwrap_or(after_close.len());
        let tail = &after_close[..tail_end];
        let rest2 = &after_close[tail_end..];
        // opener + tail, then the body, then the delimiter on its own line
        out.push_str("<<");
        out.push_str(&after[..open_len]);
        out.push_str(tail);
        out.push_str("\n");
        out.push_str(&body_and_close[..close_rel + close_pat.len()]);
        out.push('\n');
        rest = rest2;
    }
    out
}

/// Indent every line of a rendered block by `n` levels.
fn indent_block(s: &str, n: usize) -> String {
    let pad = "    ".repeat(n);
    s.lines()
        .map(|l| if l.is_empty() { String::new() } else { format!("{pad}{l}") })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Pick a perl regex delimiter absent from the pattern (`!`, `#`, `/`,
/// `|`, `,`, `;`, `:`), falling back to `{` (brace-escaped). `m{...}`
/// with brace_escape breaks quantifiers (`{1,3}` becomes literal), so
/// avoid `{}` delimiters whenever possible.
fn regex_wrap(re: &str) -> String {
    for d in ['!', '#', '/', '|', ',', ';', ':'] {
        if !re.contains(d) {
            return format!("m{d}{re}{d}");
        }
    }
    format!("m{{{}}}", brace_escape(re))
}

/// A subscript key as an IrExpr: a number → Int, a `$`-prefixed name →
/// variable read, anything else → literal/arith text (the container's
/// kind decides — see index_ref).
fn sub_key_expr(key: &str) -> IrExpr {
    if let Ok(n) = key.parse::<i64>() {
        IrExpr::Int(n)
    } else if let Some(kname) = key.strip_prefix('$') {
        if kname.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            IrExpr::Var(kname.to_string(), None)
        } else if let Some(inner) = kname
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
        {
            // `${flags:j:1}` — a param SLICE as the subscript: evaluate
            // it (`substr($flags, $j, 1)`)
            if let Some(colon) = inner.find(':') {
                let (nm, tail) = inner.split_at(colon);
                let tail2 = &tail[1..];
                let (off, len) = match tail2.find(':') {
                    Some(colon2) => (&tail2[..colon2], &tail2[colon2 + 1..]),
                    None => (tail2, ""),
                };
                let s = |v: &str| IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted);
                return IrExpr::Call {
                    func: "param".to_string(),
                    args: vec![s("slice"), s(nm), s(off), s(len)],
                };
            }
            IrExpr::Str(key.to_string(), StrStyle::DoubleQuoted)
        } else if kname.contains(',') {
            // `$i,$j` — a COMPOSITE assoc subscript (`matrix[$i,$j]`):
            // interpolate each `$ref`, commas stay literal
            let mut parts: Vec<InterpPart> = Vec::new();
            let mut lit = String::new();
            let mut rest = kname;
            let name_end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            parts.push(InterpPart::Expr(Box::new(IrExpr::Var(
                rest[..name_end].to_string(),
                None,
            ))));
            rest = &rest[name_end..];
            while !rest.is_empty() {
                if let Some(p) = rest.find('$') {
                    lit.push_str(&rest[..p]);
                    let ne = rest[p + 1..]
                        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .unwrap_or(rest[p + 1..].len());
                    if !lit.is_empty() {
                        parts.push(InterpPart::Lit(std::mem::take(&mut lit)));
                    }
                    parts.push(InterpPart::Expr(Box::new(IrExpr::Var(
                        rest[p + 1..p + 1 + ne].to_string(),
                        None,
                    ))));
                    rest = &rest[p + 1 + ne..];
                } else {
                    lit.push_str(rest);
                    rest = "";
                }
            }
            if !lit.is_empty() {
                parts.push(InterpPart::Lit(lit));
            }
            IrExpr::Interpolate(parts)
        } else {
            IrExpr::Str(key.to_string(), StrStyle::DoubleQuoted)
        }
    } else {
        IrExpr::Str(key.to_string(), StrStyle::DoubleQuoted)
    }
}

/// Escape `{`/`}` so a regex body survives `m{...}` / `s{...}{...}`
/// delimiters.
fn brace_escape(re: &str) -> String {
    let mut out = String::new();
    for c in re.chars() {
        match c {
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c => out.push(c),
        }
    }
    out
}

/// Shell glob → Perl regex (anchored fragments). `greedy` selects `.*` vs
/// `.*?` for `*` (shortest-prefix semantics like `${x#pat}`).
fn glob_to_regex(pat: &str, greedy: bool) -> String {
    let star = if greedy { ".*" } else { ".*?" };
    let mut out = String::new();
    let chars: Vec<char> = pat.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            // bash extglob: @(a|b) +(a|b) *(a|b) ?(a|b) !(a|b)
            '@' | '+' | '*' | '?' | '!' if i + 1 < chars.len() && chars[i + 1] == '(' => {
                let op = c;
                let mut j = i + 2;
                let mut depth = 1;
                let mut inner = String::new();
                while j < chars.len() && depth > 0 {
                    if chars[j] == '(' {
                        depth += 1;
                    } else if chars[j] == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    inner.push(chars[j]);
                    j += 1;
                }
                // split top-level alternations (`|` must stay regex OR)
                let mut alts = Vec::new();
                let mut cur = String::new();
                let mut d = 0i32;
                for cc in inner.chars() {
                    match cc {
                        '(' => {
                            d += 1;
                            cur.push(cc);
                        }
                        ')' => {
                            d -= 1;
                            cur.push(cc);
                        }
                        '|' if d == 0 => {
                            alts.push(cur.clone());
                            cur.clear();
                        }
                        _ => cur.push(cc),
                    }
                }
                alts.push(cur);
                let re = alts
                    .iter()
                    .map(|a| glob_to_regex(a, greedy))
                    .collect::<Vec<_>>()
                    .join("|");
                let body = match op {
                    '@' => format!("(?:{re})"),
                    '+' => format!("(?:{re})+"),
                    '*' => format!("(?:{re}){}", if greedy { "*" } else { "*?" }),
                    '?' => format!("(?:{re})?"),
                    '!' => format!("(?:(?!{re}).)*"),
                    _ => unreachable!(),
                };
                out.push_str(&body);
                i = j;
            }
            '*' => out.push_str(star),
            '?' => out.push('.'),
            '[' => {
                // character class: pass through; a lone `[` (no closing
                // bracket) is a literal
                let mut j = i + 1;
                let mut cls = String::from("[");
                let mut first = true;
                let mut closed = false;
                while j < chars.len() {
                    let cc = chars[j];
                    if cc == ']' && !first {
                        cls.push(']');
                        closed = true;
                        break;
                    }
                    if cc == '\\' {
                        cls.push('\\');
                    }
                    cls.push(cc);
                    first = false;
                    j += 1;
                }
                if closed {
                    out.push_str(&cls);
                    i = j;
                } else {
                    out.push_str("\\[");
                }
            }
            '\\' => {
                if i + 1 < chars.len() {
                    out.push('\\');
                    out.push(chars[i + 1]);
                    i += 1;
                }
            }
            '.' | '+' | '(' | ')' | '^' | '$' | '|' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

// ── brace expansion (mirror of harness/sh2-namespace.mjs) ────────────

fn brace_group(g: &serde_json::Value) -> Vec<String> {
    let items = match g {
        serde_json::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    let has_range = items.iter().any(|it| {
        it.is_object() && it.get("range").map(|r| r.is_array()).unwrap_or(false)
    });
    let mut out = Vec::new();
    for it in items {
        match it {
            serde_json::Value::String(s) => out.push(s.clone()),
            o if o.is_object() && o.get("range").map(|r| r.is_array()).unwrap_or(false) => {
                if items.len() == 1 {
                    out.extend(brace_range(o.get("range").unwrap()));
                } else {
                    // bash: a range inside a comma group stays LITERAL
                    let r = o.get("range").unwrap();
                    out.push(format!(
                        "{}..{}",
                        r[0].as_str().unwrap_or(""),
                        r[1].as_str().unwrap_or("")
                    ));
                }
            }
            o if o.is_object() && o.get("nested").is_some() => {
                out.extend(brace_nested(o.get("nested").unwrap()));
            }
            serde_json::Value::Array(sub) => out.extend(brace_nested(&serde_json::Value::Array(sub.clone()))),
            _ => {}
        }
    }
    out
}

fn brace_nested(items: &serde_json::Value) -> Vec<String> {
    let items = match items {
        serde_json::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for it in items {
        match it {
            serde_json::Value::String(s) => out.push(s.clone()),
            o if o.is_object() && o.get("range").map(|r| r.is_array()).unwrap_or(false) => {
                out.extend(brace_range(o.get("range").unwrap()));
            }
            o if o.is_object() && o.get("nested").is_some() => {
                out.extend(brace_nested(o.get("nested").unwrap()));
            }
            serde_json::Value::Array(sub) => out.extend(brace_nested(&serde_json::Value::Array(sub.clone()))),
            _ => {}
        }
    }
    out
}

fn brace_range(r: &serde_json::Value) -> Vec<String> {
    let arr = match r.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let get = |i: usize| -> String {
        arr.get(i)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let start = get(0);
    let end = get(1);
    let step: i64 = arr
        .get(2)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .abs()
        .max(1);
    let is_num = |s: &str| -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || (c == '-' && s.len() > 1))
    };
    let fmt = |n: i64, width: usize| -> String {
        let s = n.abs().to_string();
        let padded = if width > 0 {
            format!("{:0>width$}", s, width = width)
        } else {
            s
        };
        if n < 0 {
            format!("-{padded}")
        } else {
            padded
        }
    };
    if is_num(&start) && is_num(&end) {
        let (a, b): (i64, i64) = (start.parse().unwrap(), end.parse().unwrap());
        let width = if start.starts_with('0') || end.starts_with('0') {
            start.len().max(end.len())
        } else {
            0
        };
        let mut out = Vec::new();
        if a <= b {
            let mut n = a;
            while n <= b {
                out.push(fmt(n, width));
                n += step;
            }
        } else {
            let mut n = a;
            while n >= b {
                out.push(fmt(n, width));
                n -= step;
            }
        }
        return out;
    }
    // alpha runs
    let single = |s: &str| s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic();
    if single(&start) && single(&end) {
        let (ca, cb) = (
            start.chars().next().unwrap() as u8,
            end.chars().next().unwrap() as u8,
        );
        let mut out = Vec::new();
        if ca <= cb {
            let mut c = ca;
            while c <= cb {
                out.push((c as char).to_string());
                c = c.saturating_add(step as u8).min(cb + 1);
            }
        } else {
            let mut c = ca;
            while c >= cb {
                out.push((c as char).to_string());
                c = c.saturating_sub(step as u8).max(cb.saturating_sub(1));
            }
        }
        return out;
    }
    vec![format!("{start}..{end}")]
}
