//! C backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/c`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-c-core-needs.md §1):
//! `shir_to_c(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! C `long long` narrowed by the range analysis (`analyze_var_ranges` +
//! `range_width_name`, M8 spike) to `unsigned int`/`int` when the
//! conservative [lo, hi] provably fits — the var AND every arith expr
//! mentioning it must stay in width (a var's width covers its arithmetic
//! RESULTS, not just its own values). `Str` vars → `char*`, anything
//! else → runtime store (`char*` + sh2.* stubs in this draft).
//! Identifiers are mangled against C keywords (A6-consistent). Everything
//! outside the lowable subset (numeric arith, echo/printf, if/else,
//! simple assignment) emits a compile-able `sh2.*` stub or a
//! `/* TODO(unsupported) */` marker, so the draft always compiles.
//!
//! Also consumes the core's conservative string-length analysis
//! (`IrProgram.var_lengths`, fbedac4): a Str var with a known bound N
//! gets a FIXED buffer `char v[N+1]` (the fixed-buffer transform the
//! analysis was built for), with DEBUG-ONLY length asserts (`assert()`,
//! compiled out under NDEBUG) at the function boundary and BEFORE every
//! copy into the buffer — the write that would overflow is UB, and the
//! assert is the debug-mode tripwire; under NDEBUG `strncpy` truncates.
//! Unbounded (None) vars stay `char*`.
//!
//! The naive string/number coercion here is the residual "C needs type
//! inference" gap PLAN.md v2 flagged (the numeric width side is now
//! covered by the range analysis; the string side stays open).

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, IrType, InterpPart, VarKind};
use std::collections::{BTreeSet, HashMap};

enum Part {
    Lit(String),
    /// Arg(cexpr, spec) — the printf specifier for the operand,
    /// precomputed at construction where the IrExpr is in scope.
    Arg(String, NumSpec),
}

/// How to print a printf/snprintf argument.
///
/// `Num(spec, cast)`: the operand is numeric. `spec` matches the
/// operand's PROVEN width (`%u`/`%d`/`%lld` for u32/i32/i64) and `cast`
/// says whether a `(long long)` wrap is still required. The pair is
/// always consistent: cast == true implies spec == "%lld" (the cast pins
/// the vararg type to long long), and cast == false implies the operand's
/// C type is provably exactly the spec's expected type.
/// `Str`: non-numeric — `%s` + a `(char*)` cast (stub calls return
/// `long long`; printf("%s", long long) is UB).
#[derive(Debug)]
enum NumSpec {
    Num(&'static str, bool),
    Str,
}

impl PartialEq for NumSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (NumSpec::Num(a, ca), NumSpec::Num(b, cb)) => a == b && ca == cb,
            (NumSpec::Str, NumSpec::Str) => true,
            _ => false,
        }
    }
}

/// C width from the core's range analysis (`range_width_name`): an
/// Int-typed var whose conservative [lo, hi] value range provably fits
/// a narrower type than `long long` is declared at that width. Ordering
/// for widening: I64 > I32 > U32 (by signed capacity — the analysis
/// stays consistent, so a var's own range and the ranges of the arith
/// exprs mentioning it always share a common width).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Width {
    U32,
    I32,
    I64,
}

impl Width {
    fn c_type(self) -> &'static str {
        match self {
            Width::U32 => "unsigned int",
            Width::I32 => "int",
            Width::I64 => "long long",
        }
    }

    /// printf-family format for the width's C type. The format matches
    /// the DECLARED type exactly, so a cast is only needed when the
    /// operand's actual C type can't be proven to be it (see
    /// [`Render::expr_type_matches`] / [`Render::num_spec`]).
    fn format(self) -> &'static str {
        match self {
            Width::U32 => "%u",
            Width::I32 => "%d",
            Width::I64 => "%lld",
        }
    }

    fn from_range_name(name: &str) -> Width {
        match name {
            "u32" => Width::U32,
            "i32" => Width::I32,
            _ => Width::I64,
        }
    }

    fn widen(self, other: Width) -> Width {
        match (self, other) {
            (Width::I64, _) | (_, Width::I64) => Width::I64,
            (Width::I32, _) | (_, Width::I32) => Width::I32,
            _ => Width::U32,
        }
    }
}

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// var name -> conservative max string length (fbedac4's
    /// analyze_string_lengths); None = unbounded. Only vars in the
    /// analysis' assign set appear.
    var_lengths: HashMap<String, Option<u64>>,
    /// var name -> conservative [lo, hi] (analyze_var_ranges + the
    /// Range/seq for-iter seeds the analysis doesn't track).
    var_ranges: HashMap<String, (i128, i128)>,
    /// var name -> const/var verdict (the const-markup analysis): `Const`
    /// vars with a single literal top-level assignment render as C
    /// `const` declarations initialized from that literal, and the
    /// assignment statement is dropped.
    const_vars: HashMap<String, VarKind>,
    /// name -> the single top-level `Assign` RHS of a `Const` var (the
    /// hoisted `const` declaration's initializer).
    const_rhs: HashMap<String, IrExpr>,
    /// names already emitted as `const` (the matching Assign stmt is
    /// skipped at emission time).
    const_lifted: BTreeSet<String>,
    /// var name -> effective C width: the widest of the var's own range
    /// and every arith-expr result range mentioning it (a var's width
    /// must cover its arithmetic results, not just its own values).
    var_widths: HashMap<String, Width>,
    /// shell function names defined in the program (Function stmts) —
    /// calls to these render as `name();` instead of a sh2.* stub.
    functions: BTreeSet<String>,
    /// the definitions themselves (name, body) — emitted in the
    /// preamble (BEFORE main: C has no nested function definitions).
    fn_defs: Vec<(String, Vec<IrStmt>)>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    need_upper: bool,
    need_lower: bool,
    need_slice: bool,
    /// named-temp counter for statement-level snprintf buffers
    /// (`char _sN[cap]; snprintf(_sN, ...)` before the enclosing stmt)
    temp_seq: usize,
    todo: usize,
    /// untyped var names (A2 verdict missing) — the native `char*` store.
    /// getVar/param reads of these render `(name ? name : "")`; Assign
    /// targets render `name = value;` (pointer semantics).
    store: BTreeSet<String>,
    /// shell-out runtime needed (the _sh_* preamble helpers)
    need_sh: bool,
    /// sys/stat.h file tests (test_render -f/-d/...)
    need_stat: bool,
    /// fnmatch.h (test glob `==`/`!=` with * or ?)
    need_fnmatch: bool,
    /// regex.h (test `=~`)
    need_regex: bool,
    /// time.h nanosleep (the sleep builtin)
    need_time: bool,
    /// counter for _sh_site_N() / _cap_N() helper ids
    site_seq: usize,
    /// emitted shell-out site helper bodies (`static int _sh_site_N(void) {...}`)
    site_bodies: Vec<String>,
    /// emitted capture helper bodies (`static char *_cap_N(void) {...}`)
    cap_bodies: Vec<String>,
    /// `shopt -s nocasematch` → [[ == ]] globs match case-insensitively
    nocasematch: bool,
    /// rendering inside a shell function body (Return emits `return;`)
    in_function: bool,
}

/// Which command-text buffer a word append targets: the SHARED
/// builder (statement-level sites) or a capture site's private one.
#[derive(Clone, Copy)]
enum CmdBuf {
    Shared,
    Private(usize),
}

/// Bounded Str vars get a fixed buffer of bound+1 bytes; unbounded or
/// over-cap vars stay `char*`. Aligned with the analysis' own CAP.
const FIXED_BUF_CAP: u64 = 1024;

/// Per-capture-site result buffer size (command substitution output).
const CAP_BUF: usize = 65536;

/// Render an `IrProgram` to C source (main() body).
pub fn shir_to_c(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // A2 + var_lengths: the analyses run at serialization time in the
    // JSON path; the library path must run the same ones.
    prog.var_types = crate::shir::analyze_var_types(&prog);
    prog.var_lengths = crate::shir::analyze_string_lengths(&prog);
    prog.var_const = crate::shir::analyze_var_const(&prog);
    // Range analysis (M8 spike): conservative [lo, hi] per assigned var,
    // + the Range/seq for-iter seeds the analysis doesn't track (loop
    // vars are excluded from its assign set).
    let mut ranges = crate::shir::analyze_var_ranges(&prog);
    seed_loop_var_ranges(&prog.stmts, &mut ranges);
    // Effective widths: a var's width must cover every arith-expr result
    // mentioning it (e.g. i in [1, 70000] is u32, but i*i needs i64), so
    // narrow only when the var AND all its arithmetic stay in width.
    let widths = effective_widths(&prog, &ranges);
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.var_lengths = prog.var_lengths.iter().cloned().collect();
    r.const_vars = prog.var_const.iter().cloned().collect();
    r.const_rhs = const_assign_rhs(&prog.stmts, &r.const_vars);
    r.var_ranges = ranges;
    r.var_widths = widths;
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
        self.emit(&format!("/* TODO(unsupported): {what} */"));
    }

    /// A6-consistent C-keyword mangling (mirrors the emitter's safe_ident,
    /// which only covers loop vars — renderers mangle the rest).
    fn c_ident(&self, name: &str) -> String {
        const C_KEYWORDS: &[&str] = &[
            "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
            "else", "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long",
            "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct",
            "switch", "typedef", "union", "unsigned", "void", "volatile", "while", "_Bool",
            "_Complex", "true", "false",
        ];
        if C_KEYWORDS.contains(&name) {
            format!("{name}_")
        } else {
            name.to_string()
        }
    }

    fn cstr(s: &str) -> String {
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

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int)
    }

    /// The fixed-buffer bound for a Str var (Some(N) -> `char v[N+1]`),
    /// or None (stay `char*`). INT vars are excluded — the length
    /// analysis also bounds numeric RHS (`i=$((i+1))` -> 20), and
    /// `strlen`/`strncpy` on a `long long` is itself UB.
    fn buf_bound(&self, name: &str) -> Option<u64> {
        if self.is_num(name) {
            return None;
        }
        self.var_lengths
            .get(name)
            .copied()
            .flatten()
            .filter(|&b| b <= FIXED_BUF_CAP)
    }

    /// `name = rhs` into a fixed buffer of size b+1: the DEBUG-ONLY
    /// length assert fires BEFORE the copy (the UB-triggering write);
    /// NDEBUG compiles it out and strncpy truncates (null-terminated).
    /// Non-string RHS exprs (the "0" placeholder for unlowered
    /// Interpolate etc.) lower to the empty string — copying a bogus
    /// pointer would itself be UB.
    fn emit_guarded_copy(&mut self, name: &str, b: u64, rhs: &str) {
        let rhs_c = format!("(char*)({rhs})");
        let stringy = rhs.starts_with('"')
            || rhs.starts_with("(char*)")
            || rhs.starts_with("sh2_")
            || rhs.starts_with("_cap_")
            || is_ident(rhs);
        if stringy {
            self.emit(&format!("assert(strlen({rhs_c}) <= {b});"));
            self.emit(&format!("strncpy({name}, {rhs_c}, {b} + 1);"));
            self.emit(&format!("{name}[{b}] = '\\0';"));
        } else {
            self.emit(&format!("{name}[0] = '\\0';"));
        }
    }

    // ── shell-out runtime (native lowering via `bash -c` shell-outs) ──
    //
    // The C program reconstructs shell command text from the IR words and
    // runs it through `bash -c` (the reference shell — the corpus gate
    // diffs against `bash`), so external commands, pipelines, redirects
    // and command substitutions get EXACT bash semantics (glob, builtin
    // echo -e, pipelines, heredocs) without per-command emulations. The
    // runtime helpers are plain `_sh_*` functions (NOT `sh2.*` — the
    // gate's stub probe would fail on that prefix).
    //
    // Every shell-out SITE (a statement/expr exec, pipeline, redirect,
    // capture, arith-string) is emitted as a static helper function
    // (`_sh_site_N` for command runs, `_cap_N` for captures) so that:
    //   - expr-position commands short-circuit correctly in `&&`/`||`
    //     chains (the whole build+run happens at the point of the call),
    //   - `while (cond)` re-runs the command each iteration (no temps
    //     hoisted before the loop),
    //   - captures get a private command-text buffer per site (nested
    //     captures can't clobber an outer build).

    /// Emit the runtime preamble pieces the renderer has flagged.
    fn emit_runtime(&mut self) {
        self.emit("#include <stdio.h>");
        self.emit("#include <stdlib.h>");
        self.emit("#include <string.h>");
        self.emit("#include <sys/wait.h>");
        self.emit("#include <unistd.h>"); // chdir/access/getcwd
        if self.need_stat {
            self.emit("#include <sys/stat.h>");
        }
        if self.need_fnmatch || self.need_sh {
            // the shell-out runtime's ${s#pat} strip helpers use fnmatch
            self.emit("#include <fnmatch.h>");
        }
        if self.need_regex {
            self.emit("#include <regex.h>");
        }
        if self.need_regex {
            self.emit("/* [[ s =~ re ]] — POSIX ERE match */");
            self.emit("static int _sh_regex_match(const char *s, const char *re) {");
            self.emit("  regex_t rx;");
            self.emit("  if (regcomp(&rx, re, REG_EXTENDED | REG_NOSUB) != 0) return 0;");
            self.emit("  int rc = regexec(&rx, s, 0, 0, 0);");
            self.emit("  regfree(&rx);");
            self.emit("  return rc == 0;");
            self.emit("}");
            self.emit("");
        }
        if self.need_time {
            self.emit("#include <time.h>");
        }
        self.emit("#include <math.h>");
        self.emit("#include <assert.h>"); // debug-only length asserts (NDEBUG compiles out)
        self.emit("");
        if self.need_sh {
            self.emit("/* shell-out runtime: build a command line, run it via bash -c */");
            self.emit("static int _sh_rc = 0;");
            self.emit("static int _sh_argc = 0; static char **_sh_argv = 0;");
            self.emit("static char *_sh_argv_join(char *d, size_t cap) {");
            self.emit("  d[0] = 0; size_t n = 0;");
            self.emit("  for (int i = 1; i < _sh_argc; i++) {");
            self.emit("    if (i > 1 && n + 1 < cap) d[n++] = ' ';");
            self.emit("    const char *s = _sh_argv[i];");
            self.emit("    while (s && *s && n + 1 < cap) d[n++] = *s++;");
            self.emit("  }");
            self.emit("  d[n] = 0; return d;");
            self.emit("}");
            self.emit("static char *_sh_cmd = 0; static size_t _sh_cap = 0;");
            self.emit("static char *_sh_wb = 0; static size_t _sh_wcap = 0;");
            self.emit("static char *_sh_wrap = 0; static size_t _sh_wrapcap = 0;");
            self.emit("static void _sh_grow(char **b, size_t *cap, size_t need) {");
            self.emit("  if (need <= *cap) return;");
            self.emit("  *cap = need * 2; *b = (char*)realloc(*b, *cap);");
            self.emit("}");
            self.emit("static void _sh_add(const char *s) {");
            self.emit("  size_t l = _sh_cmd ? strlen(_sh_cmd) : 0, n = strlen(s);");
            self.emit("  _sh_grow(&_sh_cmd, &_sh_cap, l + n + 1);");
            self.emit("  memcpy(_sh_cmd + l, s, n + 1);");
            self.emit("}");
            self.emit("static void _sh_addc(char c) {");
            self.emit("  size_t l = _sh_cmd ? strlen(_sh_cmd) : 0;");
            self.emit("  _sh_grow(&_sh_cmd, &_sh_cap, l + 2);");
            self.emit("  _sh_cmd[l] = c; _sh_cmd[l + 1] = 0;");
            self.emit("}");
            self.emit("static void _sh_reset(void) { _sh_grow(&_sh_cmd, &_sh_cap, 1); _sh_cmd[0] = 0; }");
            self.emit("static void _sh_wb_add(const char *s) {");
            self.emit("  size_t l = _sh_wb ? strlen(_sh_wb) : 0, n = strlen(s);");
            self.emit("  _sh_grow(&_sh_wb, &_sh_wcap, l + n + 1);");
            self.emit("  memcpy(_sh_wb + l, s, n + 1);");
            self.emit("}");
            self.emit("static void _sh_wb_reset(void) { _sh_grow(&_sh_wb, &_sh_wcap, 1); _sh_wb[0] = 0; }");
            self.emit("/* append s as ONE shell word (single-quoted, so no re-expansion) */");
            self.emit("static void _sh_word(const char *s) {");
            self.emit("  _sh_add(\" '\");");
            self.emit("  for (const char *p = s; *p; p++) {");
            self.emit("    if (*p == '\\'') _sh_add(\"'\\\\''\"); else _sh_addc(*p);");
            self.emit("  }");
            self.emit("  _sh_addc('\\'');");
            self.emit("}");
            self.emit("/* append raw text (no quoting) - for already-shell-safe pieces */");
            self.emit("static void _sh_addraw(const char *s) { _sh_add(\" \"); _sh_add(s); }");
            self.emit("/* buffer-parameterized variants (capture sites' private buffers) */");
            self.emit("static void _sh_badd(char **b, size_t *cap, const char *s) {");
            self.emit("  size_t l = *b ? strlen(*b) : 0, n = strlen(s);");
            self.emit("  _sh_grow(b, cap, l + n + 1);");
            self.emit("  memcpy(*b + l, s, n + 1);");
            self.emit("}");
            self.emit("static void _sh_baddc(char **b, size_t *cap, char c) {");
            self.emit("  size_t l = *b ? strlen(*b) : 0;");
            self.emit("  _sh_grow(b, cap, l + 2);");
            self.emit("  (*b)[l] = c; (*b)[l + 1] = 0;");
            self.emit("}");
            self.emit("static void _sh_bres(char **b, size_t *cap) { _sh_grow(b, cap, 1); (*b)[0] = 0; }");
            self.emit("static void _sh_bword(char **b, size_t *cap, const char *s) {");
            self.emit("  _sh_badd(b, cap, \" '\");");
            self.emit("  for (const char *p = s; *p; p++) {");
            self.emit("    if (*p == '\\'') _sh_badd(b, cap, \"'\\\\''\"); else _sh_baddc(b, cap, *p);");
            self.emit("  }");
            self.emit("  _sh_baddc(b, cap, '\\'');");
            self.emit("}");
            self.emit("/* wrap the built command as `bash -c '<cmd>'` (single-quote escaped) */");
            self.emit("static void _sh_wrap_cmd(const char *cmd) {");
            self.emit("  size_t n = strlen(cmd), need = n * 2 + 16;");
            self.emit("  _sh_grow(&_sh_wrap, &_sh_wrapcap, need);");
            self.emit("  char *p = _sh_wrap; strcpy(p, \"bash -c '\"); p += 9;");
            self.emit("  for (const char *c = cmd; *c; c++) {");
            self.emit("    if (*c == '\\'') { memcpy(p, \"'\\\\''\", 4); p += 4; }");
            self.emit("    else *p++ = *c;");
            self.emit("  }");
            self.emit("  *p++ = '\\''; *p = 0;");
            self.emit("}");
            self.emit("static int _sh_system_rc(void) {");
            self.emit("  _sh_wrap_cmd(_sh_cmd ? _sh_cmd : \"\");");
            self.emit("  int rc = system(_sh_wrap);");
            self.emit("  _sh_rc = (rc == -1) ? 127 : (WIFEXITED(rc) ? WEXITSTATUS(rc) : 1);");
            self.emit("  return _sh_rc;");
            self.emit("}");
            self.emit("static void _sh_run(void) { (void)_sh_system_rc(); }");
            self.emit("/* run the built command, capture stdout, strip trailing newlines */");
            self.emit("static void _sh_capture(char *buf, size_t cap, const char *cmd) {");
            self.emit("  if (!cmd || !*cmd) { buf[0] = 0; _sh_rc = 0; return; }");
            self.emit("  _sh_wrap_cmd(cmd);");
            self.emit("  FILE *p = popen(_sh_wrap, \"r\");");
            self.emit("  if (!p) { buf[0] = 0; _sh_rc = 127; return; }");
            self.emit("  size_t n = fread(buf, 1, cap - 1, p); buf[n] = 0;");
            self.emit("  int rc = pclose(p);");
            self.emit("  _sh_rc = (rc == -1) ? 127 : (WIFEXITED(rc) ? WEXITSTATUS(rc) : 1);");
            self.emit("  while (n > 0 && (buf[n - 1] == '\\n' || buf[n - 1] == '\\r')) buf[--n] = 0;");
            self.emit("}");
            self.emit("/* split a captured string on IFS whitespace into words */");
            self.emit("static size_t _sh_split(char *buf, char **words, size_t max) {");
            self.emit("  size_t n = 0; char *p = buf;");
            self.emit("  while (*p) {");
            self.emit("    while (*p == ' ' || *p == '\\t' || *p == '\\n') p++;");
            self.emit("    if (!*p) break;");
            self.emit("    if (n >= max) break;");
            self.emit("    words[n++] = p;");
            self.emit("    while (*p && *p != ' ' && *p != '\\t' && *p != '\\n') p++;");
            self.emit("    if (*p) *p++ = 0;");
            self.emit("  }");
            self.emit("  return n;");
            self.emit("}");
            self.emit("/* export a var value so `$name` in a bash -c child sees it */");
            self.emit("static void _sh_export(const char *name, const char *val) {");
            self.emit("  setenv(name, val ? val : \"\", 1);");
            self.emit("}");
            self.emit("/* `read` builtin: one line from stdin into a static buffer */");
            self.emit("static char *_sh_readline(void) {");
            self.emit("  static char *_sh_rd = 0; static size_t _sh_rdcap = 0;");
            self.emit("  _sh_grow(&_sh_rd, &_sh_rdcap, 4096);");
            self.emit("  if (!fgets(_sh_rd, 4096, stdin)) { _sh_rd[0] = 0; return _sh_rd; }");
            self.emit("  size_t n = strlen(_sh_rd);");
            self.emit("  while (n > 0 && (_sh_rd[n - 1] == '\\n' || _sh_rd[n - 1] == '\\r')) _sh_rd[--n] = 0;");
            self.emit("  return _sh_rd;");
            self.emit("}");
            self.emit("/* ${x:off:len} substring (bash: off<0 counts from the end) */");
            self.emit("static char *_sh_substr(char *d, size_t cap, const char *s, long long off, long long len) {");
            self.emit("  size_t n = strlen(s);");
            self.emit("  long long b = off < 0 ? (long long)n + off : off;");
            self.emit("  if (b < 0) b = 0; if (b > (long long)n) b = (long long)n;");
            self.emit("  long long e = (len < 0) ? (long long)n : b + len;");
            self.emit("  if (e > (long long)n) e = (long long)n; if (e < b) e = b;");
            self.emit("  size_t out = (size_t)(e - b);");
            self.emit("  if (out >= cap) out = cap - 1;");
            self.emit("  memcpy(d, s + b, out); d[out] = 0;");
            self.emit("  return d;");
            self.emit("}");
            self.emit("/* ${s//pat/repl} - literal replace-all */");
            self.emit("static char *_sh_replace(char *d, size_t cap, const char *s, const char *pat, const char *repl) {");
            self.emit("  size_t pn = strlen(pat), rn = strlen(repl), dn = 0;");
            self.emit("  const char *p = s;");
            self.emit("  if (!pn) { strncpy(d, s, cap - 1); d[cap - 1] = 0; return d; }");
            self.emit("  while (*p) {");
            self.emit("    const char *hit = strstr(p, pat);");
            self.emit("    if (!hit) break;");
            self.emit("    size_t pre = (size_t)(hit - p);");
            self.emit("    while (pre-- && dn + 1 < cap) d[dn++] = *p++;");
            self.emit("    for (size_t i = 0; i < rn && dn + 1 < cap; i++) d[dn++] = repl[i];");
            self.emit("    p = hit + pn;");
            self.emit("  }");
            self.emit("  while (*p && dn + 1 < cap) d[dn++] = *p++;");
            self.emit("  d[dn] = 0;");
            self.emit("  return d;");
            self.emit("}");
            self.emit("/* ${s#pat}/${s##pat} prefix strip (glob-aware, greedy = longest) */");
            self.emit("static char *_sh_strippre(char *d, size_t cap, const char *s, const char *pat, int greedy) {");
            self.emit("  static char sc[65536];");
            self.emit("  strncpy(sc, s, sizeof sc - 1); sc[sizeof sc - 1] = 0;");
            self.emit("  size_t n = strlen(sc), best = 0;");
            self.emit("  for (size_t i = 0; i <= n; i++) {");
            self.emit("    char c = sc[i]; sc[i] = 0;");
            self.emit("    if (fnmatch(pat, sc, 0) == 0) best = i;");
            self.emit("    sc[i] = c;");
            self.emit("    if (!greedy && best) break;");
            self.emit("  }");
            self.emit("  strncpy(d, sc + best, cap - 1); d[cap - 1] = 0;");
            self.emit("  return d;");
            self.emit("}");
            self.emit("/* ${s%pat}/${s%%pat} suffix strip (greedy = longest prefix removed) */");
            self.emit("static char *_sh_stripsuf(char *d, size_t cap, const char *s, const char *pat, int greedy) {");
            self.emit("  static char sc[65536];");
            self.emit("  strncpy(sc, s, sizeof sc - 1); sc[sizeof sc - 1] = 0;");
            self.emit("  size_t n = strlen(sc), best = greedy ? 0 : n;");
            self.emit("  if (greedy) {");
            self.emit("    for (size_t i = n; i > 0; i--) {");
            self.emit("      char c = sc[i]; sc[i] = 0;");
            self.emit("      if (fnmatch(pat, sc, 0) == 0) { best = i; break; }");
            self.emit("      sc[i] = c;");
            self.emit("    }");
            self.emit("  } else {");
            self.emit("    for (size_t i = 0; i < n; i++) {");
            self.emit("      char c = sc[i]; sc[i] = 0;");
            self.emit("      if (fnmatch(pat, sc, 0) == 0) { best = i; break; }");
            self.emit("      sc[i] = c;");
            self.emit("    }");
            self.emit("  }");
            self.emit("  strncpy(d, sc + best, cap - 1); d[cap - 1] = 0;");
            self.emit("  return d;");
            self.emit("}");
            self.emit("");
        }
        if self.need_stat {
            self.emit("/* [ -f/-d/-e/-s/... ] file tests */");
            self.emit("static int _sh_is_f(const char *p) { struct stat st; return stat(p, &st) == 0 && S_ISREG(st.st_mode); }");
            self.emit("static int _sh_is_d(const char *p) { struct stat st; return stat(p, &st) == 0 && S_ISDIR(st.st_mode); }");
            self.emit("static int _sh_is_e(const char *p) { struct stat st; return stat(p, &st) == 0; }");
            self.emit("static int _sh_is_s(const char *p) { struct stat st; return stat(p, &st) == 0 && st.st_size > 0; }");
            self.emit("static int _sh_is_l(const char *p) { struct stat st; return lstat(p, &st) == 0 && S_ISLNK(st.st_mode); }");
            self.emit("static int _sh_is_r(const char *p) { return access(p, R_OK) == 0; }");
            self.emit("static int _sh_is_w(const char *p) { return access(p, W_OK) == 0; }");
            self.emit("static int _sh_is_x(const char *p) { return access(p, X_OK) == 0; }");
            self.emit("");
        }
    }

    // ── expressions ──────────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => Self::cstr(s),
            IrExpr::Var(name, _) => self.c_ident(name),
            IrExpr::Ident(name) => self.c_ident(name),
            IrExpr::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                let c_op = match op {
                    crate::ir::BinOpKind::Add => "+",
                    crate::ir::BinOpKind::Sub => "-",
                    crate::ir::BinOpKind::Mul => "*",
                    crate::ir::BinOpKind::Div => "/",
                    crate::ir::BinOpKind::Mod => "%",
                    crate::ir::BinOpKind::Eq => "==",
                    crate::ir::BinOpKind::Ne => "!=",
                    crate::ir::BinOpKind::Lt => "<",
                    crate::ir::BinOpKind::Gt => ">",
                    crate::ir::BinOpKind::Le => "<=",
                    crate::ir::BinOpKind::Ge => ">=",
                    crate::ir::BinOpKind::And => "&&",
                    crate::ir::BinOpKind::Or => "||",
                    // `!` is UNARY — the ShIR duplicates the operand
                    // (until loops: BinOp{Not, test, test}), so render
                    // the negation of the lhs and ignore the rhs copy.
                    crate::ir::BinOpKind::Not => return format!("(!({l}))"),
                    crate::ir::BinOpKind::Pow => {
                        return format!("pow({l},{r})");
                    }
                    _ => {
                        self.mark_todo(&format!("BinOp {:?}", op));
                        "0".into()
                    }
                };
                format!("({l} {c_op} {r})")
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Interpolate(_) => self.value_c(e),
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|e| self.expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::cstr(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b { "1".into() } else { "0".into() }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "0".into()
                }
            },
            other => {
                self.mark_todo(&format!("expr {:?}", other));
                "0".into()
            }
        }
    }

    /// Native C arithmetic from ArithAst (the numeric path).
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) => self.c_ident(name),
            ArithAst::Index { .. } => {
                self.mark_todo("arith Index");
                "0".into()
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                if *op == "**" {
                    format!("pow({l},{r})")
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
                // `x op= rhs` — native (the zero-divisor /%= cases are
                // kept on the runtime by the core, so op is safe here)
                format!("{} {op}= {}", self.c_ident(var), self.arith(rhs))
            }
            ArithAst::IncDec { var, delta, prefix } => {
                // `++x` / `x++` / `--x` / `x--` (delta ±1)
                let name = self.c_ident(var);
                if *prefix {
                    format!("{}{}", if *delta >= 0 { "++" } else { "--" }, name)
                } else {
                    format!("{}{}", name, if *delta >= 0 { "++" } else { "--" })
                }
            }
        }
    }



    fn sh2_stub(&mut self, name: &str, _args: &[IrExpr], note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("sh2_{name}()")
    }

    /// A sh2.*-free no-op call: `exec ":"` / `exec "true"` (and the
    /// always-false `exec "false"`). Setup/cleanup wrappers in the
    /// shellbench runners are exactly these — skipping them (instead of
    /// a sh2_exec stub) is what makes the loop body render natively.
    fn noop_value(&self, func: &str, args: &[IrExpr]) -> Option<&'static str> {
        if func == "exec" {
            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                return match cmd.as_str() {
                    ":" | "true" => Some("1"),
                    "false" => Some("0"),
                    // declaration builtins — the hoist already declares
                    // the vars (`local x` / `typeset x` / `declare x`
                    // with no initializer are pure declarations).
                    "local" | "declare" | "typeset" | "export" | "readonly" => Some("1"),
                    _ => None,
                };
            }
        }
        None
    }

    fn noop_value_call(&self, e: &IrExpr) -> bool {
        matches!(
            e,
            IrExpr::Call { func, args } if self.noop_value(func, args).is_some()
        )
    }

    /// Render `var`'s declaration (Int -> the narrowed width, bounded Str
    /// -> the fixed buffer, else char*). Shared by the main hoist and the
    /// per-function hoists.
    fn emit_var_decl(&mut self, v: &str) {
        let name = self.c_ident(v);
        // const-markup lift: a Const var whose single top-level
        // assignment is a literal renders as a const declaration
        // initialized from that literal; the Assign stmt is dropped
        // (see the Assign arm). Only literal RHSs are lifted — a
        // non-literal init would need a runtime write (and possibly a
        // var reference declared later in the hoist order).
        if let Some(rhs) = self.const_rhs.get(v).cloned() {
            // numeric vars need a numeric initializer: the Str RHS parses
            // as an integer (that is exactly the numeric lift's criterion)
            let init = if self.is_num(v) {
                match &rhs {
                    IrExpr::Int(i) => Some(i.to_string()),
                    IrExpr::Str(s, _) => s.trim().parse::<i128>().ok().map(|n| n.to_string()),
                    _ => None,
                }
            } else {
                self.literal_init(&rhs)
            };
            if let Some(init) = init {
                self.const_lifted.insert(v.to_string());
                if self.is_num(v) {
                    self.emit(&format!("const {} {name} = {init};", self.width_of_var(v).c_type()));
                } else if let Some(b) = self.buf_bound(v) {
                    self.emit(&format!("const char {name}[{}] = {init};", b + 1));
                } else {
                    self.emit(&format!("const char* {name} = {init};"));
                }
                return;
            }
        }
        if self.is_num(v) {
            self.emit(&format!("{} {name} = 0;", self.width_of_var(v).c_type()));
        } else if let Some(b) = self.buf_bound(v) {
            // the fixed-buffer transform: the var_lengths analysis
            // proves len(v) <= b, so the buffer is b+1 bytes
            self.emit(&format!("char {name}[{}] = \"\";", b + 1));
        } else {
            self.emit(&format!("char* {name} = NULL;"));
        }
    }

    /// Render an expression as a C compile-time constant initializer, or
    /// None when it isn't one (var refs, calls, captures, interpolation
    /// with expression parts). Ints and string literals (incl. pure-
    /// literal interpolations) qualify.
    fn literal_init(&mut self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Int(i) => Some(i.to_string()),
            IrExpr::Str(s, _) => Some(Self::cstr(s)),
            IrExpr::Interpolate(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(l) => s.push_str(l),
                        InterpPart::Expr(_) => return None,
                    }
                }
                Some(Self::cstr(&s))
            }
            _ => None,
        }
    }

    /// Emit the DEBUG-ONLY length invariants (assert, NDEBUG-out) for the
    /// bounded vars among `vars` — at function boundaries.
    fn emit_bound_asserts(&mut self, vars: &BTreeSet<String>) {
        for v in vars {
            if let Some(b) = self.buf_bound(v) {
                let name = self.c_ident(v);
                self.emit(&format!("assert(strlen({name}) <= {b});"));
            }
        }
    }

    /// Emit one shell function as `static void NAME(void) { ... }`
    /// (preamble position — C has no nested function definitions).
    /// Always emitted: a `:`-body function may be CALLED (shellbench
    /// func:func wraps the call in @begin/@end) — dropping the
    /// definition would make the call an undefined symbol.
    fn emit_function(&mut self, name: &str, body: &[IrStmt], main_vars: &BTreeSet<String>) {
        let fname = self.c_ident(name);
        self.emit(&format!("static void {fname}(void) {{"));
        self.depth += 1;
        let prev_fn = self.in_function;
        self.in_function = true;
        // per-function hoist: vars ASSIGNED inside the function that are
        // NOT assigned at top level (main's hoist owns those — bash
        // function assigns touch the GLOBAL) and NOT `local`-declared in
        // the body (the Declare stmt declares those at its position).
        let mut fvars: BTreeSet<String> = BTreeSet::new();
        collect_assigned_vars(body, &mut fvars);
        let mut declared = BTreeSet::new();
        collect_declare_names(body, &mut declared);
        let fvars: BTreeSet<String> = fvars
            .iter()
            .filter(|v| !main_vars.contains(*v) && !declared.contains(*v))
            .cloned()
            .collect();
        for n in &fvars {
            self.emit_var_decl(n);
        }
        if !fvars.is_empty() {
            self.emit("");
            self.emit_bound_asserts(&fvars);
            self.emit("");
        }
        for st in body {
            self.stmt(st);
        }
        self.depth -= 1;
        self.emit("}");
        self.in_function = prev_fn;
    }

    fn str_arg(args: &[IrExpr], i: usize) -> Option<String> {
        match args.get(i) {
            Some(IrExpr::Str(s, _)) => Some(s.clone()),
            _ => None,
        }
    }

    /// A char* read of a string-typed/untyped var (NULL-safe: unset vars
    /// render as the empty string, matching bash's unset semantics in
    /// expansions).
    fn store_ref(&self, name: &str) -> String {
        let id = self.c_ident(name);
        format!("({id} ? {id} : \"\")")
    }

    /// Emit `char _sN[32]; snprintf(_sN, ..., "%lld", (long long)(v));`
    /// and return `_sN` — the string form of a numeric C expression.
    fn num_temp(&mut self, v: &str) -> String {
        let t = format!("_s{}", self.temp_seq);
        self.temp_seq += 1;
        self.emit(&format!("char {t}[32];"));
        self.emit(&format!("snprintf({t}, sizeof {t}, \"%lld\", (long long)({v}));"));
        t
    }

    /// Emit `char _sN[cap];` and return `_sN` — a per-use string temp.
    fn str_temp(&mut self, cap: usize) -> String {
        let t = format!("_s{}", self.temp_seq);
        self.temp_seq += 1;
        self.emit(&format!("char {t}[{cap}];"));
        t
    }

    /// The char* C expression for a word's VALUE (numeric operands get a
    /// snprintf temp emitted as statements before the current one).
    fn value_c(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::cstr(s),
            IrExpr::Int(i) => self.num_temp(&i.to_string()),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.is_num(name) {
                    self.num_temp(&self.c_ident(name))
                } else {
                    self.store_ref(name)
                }
            }
            IrExpr::Arith(a) => {
                let a = self.arith(a);
                self.num_temp(&a)
            }
            IrExpr::BinOp { .. } | IrExpr::Bool(_) => {
                let x = self.expr(e);
                self.num_temp(&x)
            }
            IrExpr::Interpolate(parts) => {
                // value context: snprintf the flattened parts into a temp
                // (NOT the shared word buffer — callers may be mid-assembly)
                let parts = flatten_parts(parts);
                let mut fmt = String::new();
                let mut cargs: Vec<String> = Vec::new();
                for p in &parts {
                    match p {
                        InterpPart::Lit(s) => fmt.push_str(&Self::cstr(s)),
                        InterpPart::Expr(x) => {
                            fmt.push_str("%s");
                            cargs.push(self.value_c(x));
                        }
                    }
                }
                let t = self.str_temp(4096);
                let args = if cargs.is_empty() {
                    String::new()
                } else {
                    format!(", {}", cargs.join(", "))
                };
                self.emit(&format!(
                    "snprintf({t}, sizeof {t}, {fmt}{args});"
                ));
                t
            }
            IrExpr::Call { func, args } => match func.as_str() {
                "getVar" => {
                    let Some(name) = Self::str_arg(args, 0) else {
                        return "0".into();
                    };
                    if name == "?" {
                        return self.num_temp("_sh_rc");
                    }
                    if name == "#" {
                        return "(_sh_argc - 1)".into();
                    }
                    if name == "@" || name == "*" {
                        let t = self.str_temp(4096);
                        self.emit(&format!("_sh_argv_join({t}, sizeof {t});"));
                        return t;
                    }
                    if name.chars().all(|c| c.is_ascii_digit()) {
                        return format!(
                            "(({name} < _sh_argc && _sh_argv[{name}]) ? _sh_argv[{name}] : \"\")"
                        );
                    }
                    if self.is_num(&name) {
                        self.num_temp(&self.c_ident(&name))
                    } else if self.store.contains(&name) {
                        self.store_ref(&name)
                    } else {
                        format!(
                            "(getenv({}) ? getenv({}) : \"\")",
                            Self::cstr(&name),
                            Self::cstr(&name)
                        )
                    }
                }
                "param" => self.param_call(args),
                "capture" | "captureWords" => self.capture_call(args),
                "brace" => {
                    let items = brace_expand(args);
                    Self::cstr(&items.join(" "))
                }
                _ => self.expr(e),
            },
            _ => self.expr(e),
        }
    }

    /// The C int expression for a word used in a numeric context
    /// (`exit $n`, `sleep $x`): numeric vars stay numeric, strings atoll.
    fn value_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => match s.trim().parse::<i64>() {
                Ok(n) => n.to_string(),
                Err(_) => format!("(int)atoll({})", Self::cstr(s)),
            },
            IrExpr::Var(name, _) | IrExpr::Ident(name) if self.is_num(name) => self.c_ident(name),
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(name) = Self::str_arg(args, 0) {
                    if name == "?" {
                        return "_sh_rc".into();
                    }
                    if self.is_num(&name) {
                        return self.c_ident(&name);
                    }
                }
                format!("(int)atoll({})", self.value_c(e))
            }
            _ => format!("(int)atoll({})", self.value_c(e)),
        }
    }

    // ── shell-out site machinery ─────────────────────────────────────

    /// Register a command-run site; `body` renders the helper body (the
    /// command-text build). Returns the call expression `_sh_site_N()`.
    /// `invert` makes the helper return `!rc` (the `(( ))` truth value).
    fn shell_site(&mut self, body: impl FnOnce(&mut Render), invert: bool) -> String {
        self.need_sh = true;
        let id = self.site_seq;
        self.site_seq += 1;
        let saved = std::mem::take(&mut self.out);
        let saved_depth = self.depth;
        self.depth = 0;
        body(self);
        let body_out = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        let ret = if invert {
            "  return !_sh_system_rc();"
        } else {
            "  return _sh_system_rc();"
        };
        let mut s = format!("static int _sh_site_{id}(void) {{\n");
        for line in body_out {
            s.push_str(&line);
            s.push('\n');
        }
        s.push_str(ret);
        s.push_str("\n}");
        self.site_bodies.push(s);
        format!("_sh_site_{id}()")
    }

    /// Wrap a condition expression in a helper: temps the cond emits
    /// refresh every call — `while (cond)` must re-evaluate them per
    /// iteration (a hoisted temp would go stale).
    fn cond_site(&mut self, cond: &IrExpr) -> String {
        let cond = cond.clone();
        self.shell_site(
            |r| {
                let v = r.expr(&cond);
                r.emit(&format!("return ({v});"));
            },
            false,
        )
    }

    /// Register a capture site (private command/wb buffers + result buf).
    /// Returns the call expression `_cap_N()` (a char*).
    fn cap_site(&mut self, body: impl FnOnce(&mut Render, usize)) -> String {
        self.need_sh = true;
        let id = self.site_seq;
        self.site_seq += 1;
        let saved = std::mem::take(&mut self.out);
        let saved_depth = self.depth;
        self.depth = 0;
        body(self, id);
        let body_out = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        let mut s = format!("static char *_cap_{id}(void) {{\n");
        s.push_str(&format!("  static char buf[{CAP_BUF}];\n"));
        s.push_str(&format!(
            "  static char *_c{id}_cmd = 0; static size_t _c{id}_cap = 0;\n"
        ));
        s.push_str(&format!(
            "  static char *_c{id}_wb = 0; static size_t _c{id}_wcap = 0;\n"
        ));
        for line in body_out {
            s.push_str(&line);
            s.push('\n');
        }
        s.push_str("}");
        self.cap_bodies.push(s);
        format!("_cap_{id}()")
    }

    /// Append `e` as ONE shell word to the command buffer `buf`
    /// (single-quoted — the value cannot be re-expanded). Captures in a
    /// word evaluate into a temp (their sites have private buffers, so
    /// the build never interleaves).
    fn sh_word(&mut self, buf: CmdBuf, e: &IrExpr) {
        let word = |r: &mut Render, v: String| match buf {
            CmdBuf::Shared => r.emit(&format!("_sh_word({v});")),
            CmdBuf::Private(id) => r.emit(&format!("_sh_bword(&_c{id}_cmd, &_c{id}_cap, {v});")),
        };
        match e {
            IrExpr::Str(s, _) => word(self, Self::cstr(s)),
            IrExpr::Int(i) => word(self, format!("\"{i}\"")),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                if self.is_num(name) {
                    let t = self.num_temp(&self.c_ident(name));
                    word(self, t);
                } else {
                    word(self, self.store_ref(name));
                }
            }
            IrExpr::Call { func, args } => match func.as_str() {
                "getVar" => {
                    let name = Self::str_arg(args, 0);
                    match name.as_deref() {
                        Some("?") => {
                            let t = self.num_temp("_sh_rc");
                            word(self, t);
                        }
                        Some(n) if self.is_num(n) => {
                            let t = self.num_temp(&self.c_ident(n));
                            word(self, t);
                        }
                        Some(n) => word(self, self.store_ref(n)),
                        None => word(self, "0".into()),
                    }
                }
                "split" => {
                    // unquoted $var: the value must be re-SPLIT by the
                    // child shell — export it and append the bare ref
                    let v = match args.first() {
                        Some(x) => self.value_c(x),
                        None => "\"\"".into(),
                    };
                    match buf {
                        CmdBuf::Shared => {
                            self.emit(&format!("_sh_export(\"_SHSPLIT\", {v});"));
                            self.emit("_sh_addraw(\"$_SHSPLIT\");");
                        }
                        CmdBuf::Private(id) => {
                            self.emit(&format!("_sh_export(\"_SHSPLIT\", {v});"));
                            self.emit(&format!(
                                "_sh_badd(&_c{id}_cmd, &_c{id}_cap, \" $_SHSPLIT\");"
                            ));
                        }
                    }
                }
                "brace" => {
                    // compile-time expansion → one word per item
                    for item in brace_expand(args) {
                        word(self, Self::cstr(&item));
                    }
                }
                "capture" | "captureWords" => {
                    let t = format!("_t{}", self.temp_seq);
                    self.temp_seq += 1;
                    let cap = self.capture_call(args);
                    self.emit(&format!("char *{t} = {cap};"));
                    word(self, t);
                }
                _ => {
                    let v = self.value_c(e);
                    word(self, v);
                }
            },
            IrExpr::Interpolate(parts) => {
                // assemble the word content (flattened parts) into the
                // word buffer, then quote the whole as one word
                let parts = flatten_parts(parts);
                match buf {
                    CmdBuf::Shared => {
                        self.emit("_sh_wb_reset();");
                        for p in parts {
                            match p {
                                InterpPart::Lit(s) => {
                                    self.emit(&format!("_sh_wb_add({});", Self::cstr(&s)))
                                }
                                InterpPart::Expr(x) => {
                                    let v = self.value_c(&x);
                                    self.emit(&format!("_sh_wb_add({v});"));
                                }
                            }
                        }
                        word(self, "_sh_wb".into());
                    }
                    CmdBuf::Private(id) => {
                        self.emit(&format!("_sh_bres(&_c{id}_wb, &_c{id}_wcap);"));
                        for p in parts {
                            match p {
                                InterpPart::Lit(s) => self.emit(&format!(
                                    "_sh_badd(&_c{id}_wb, &_c{id}_wcap, {});",
                                    Self::cstr(&s)
                                )),
                                InterpPart::Expr(x) => {
                                    let v = self.value_c(&x);
                                    self.emit(&format!(
                                        "_sh_badd(&_c{id}_wb, &_c{id}_wcap, {v});"
                                    ));
                                }
                            }
                        }
                        word(self, format!("_c{id}_wb"));
                    }
                }
            }
            other => {
                let v = self.value_c(other);
                word(self, v);
            }
        }
    }

    /// Append raw separator text to a command buffer (buf-parameterized).
    fn sh_raw(&mut self, buf: CmdBuf, s: &str) {
        match buf {
            CmdBuf::Shared => self.emit(&format!("_sh_addraw({});", Self::cstr(s))),
            CmdBuf::Private(id) => self.emit(&format!(
                "_sh_badd(&_c{id}_cmd, &_c{id}_cap, {});",
                Self::cstr(&format!(" {s}"))
            )),
        }
    }

    /// Append raw text (no leading space) to a command buffer.
    fn sh_add(&mut self, buf: CmdBuf, s: &str) {
        match buf {
            CmdBuf::Shared => self.emit(&format!("_sh_add({});", Self::cstr(s))),
            CmdBuf::Private(id) => self.emit(&format!(
                "_sh_badd(&_c{id}_cmd, &_c{id}_cap, {});",
                Self::cstr(s)
            )),
        }
    }

    /// Append a `[ ... ]` test to a command buffer.
    fn sh_test_text(&mut self, buf: CmdBuf, t: &str) {
        match buf {
            CmdBuf::Shared => self.emit(&format!("_sh_addraw({});", Self::cstr(&format!("[ {t} ]")))),
            CmdBuf::Private(id) => self.emit(&format!(
                "_sh_badd(&_c{id}_cmd, &_c{id}_cap, {});",
                Self::cstr(&format!(" [ {t} ]"))
            )),
        }
    }

    /// Reconstruct the command text of one Arrow body (an exec call, a
    /// test, a redirect, a nested pipeline). Emits word appends.
    fn sh_stage(&mut self, buf: CmdBuf, stmts: &[IrStmt]) {
        let mut first_stmt = true;
        for s in stmts {
            if !first_stmt {
                self.sh_raw(buf, ";");
            }
            first_stmt = false;
            match s {
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                    // env prefix: `IFS=: cmd ...` (the Object arg)
                    for a in args {
                        if let IrExpr::Object(fields) = a {
                            for (k, v) in fields {
                                let key = k.clone();
                                let val = v.clone();
                                self.sh_raw(buf, &key);
                                self.sh_raw(buf, "=");
                                self.sh_word(buf, &val);
                            }
                        }
                    }
                    if let Some(cmd) = Self::str_arg(args, 0) {
                        self.sh_word(
                            buf,
                            &IrExpr::Str(cmd, crate::ir::StrStyle::DoubleQuoted),
                        );
                    }
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        for w in items {
                            self.sh_word(buf, w);
                        }
                    }
                }
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "whileLoop" => {
                    // `while C; do B; done` — args[0] = cond Arrow, args[1] = body
                    if let (Some(IrExpr::Arrow(cond)), Some(IrExpr::Arrow(body))) =
                        (args.first(), args.get(1))
                    {
                        self.emit("_sh_addraw(\"while\");");
                        self.sh_stage(buf, cond);
                        self.emit("_sh_addraw(\"; do\");");
                        self.sh_stage(buf, body);
                        self.emit("_sh_addraw(\"; done\");");
                    }
                }
                IrStmt::Assign { targets, expr } => {
                    // shell text form: NAME=$(( ... )) / NAME='value'
                    if let Some(t) = targets.first() {
                        if t.indices.is_empty() {
                            self.emit(&format!(
                                "_sh_addraw({});",
                                Self::cstr(&format!("{}=", t.var))
                            ));
                            match expr {
                                IrExpr::Arith(a) => {
                                    self.emit(&format!(
                                        "_sh_addraw({});",
                                        Self::cstr(&format!("$(({}))", arith_shell(a)))
                                    ));
                                }
                                _ => {
                                    self.sh_word(buf, expr);
                                }
                            }
                        }
                    }
                }
                IrStmt::If { cond, then, elsifs, else_ } => {
                    self.emit("_sh_addraw(\"if\");");
                    self.sh_stage_expr(buf, cond);
                    self.emit("_sh_addraw(\"; then\");");
                    self.sh_stage(buf, then);
                    for (ec, body) in elsifs {
                        self.emit("_sh_addraw(\"; elif\");");
                        self.sh_stage_expr(buf, ec);
                        self.emit("_sh_addraw(\"; then\");");
                        self.sh_stage(buf, body);
                    }
                    if !else_.is_empty() {
                        self.emit("_sh_addraw(\"; else\");");
                        self.sh_stage(buf, else_);
                    }
                    self.emit("_sh_addraw(\"; fi\");");
                }
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "test" => {
                    if let Some(t) = Self::str_arg(args, 0) {
                        self.sh_export_vars(&t);
                        self.sh_test_text(buf, &t);
                    }
                }
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
                    self.sh_pipeline_text(buf, args);
                }
                IrStmt::Redirect { inner, redirects } => {
                    self.sh_stage(buf, inner);
                    self.sh_redirect_text(buf, redirects);
                }
                _ => {
                    self.mark_todo(&format!("capture body stmt {:?}", s));
                }
            }
        }
    }

    /// Render a condition expr as shell text inside a stage (a `[ ]`
    /// test, or a command).
    fn sh_stage_expr(&mut self, buf: CmdBuf, e: &IrExpr) {
        match e {
            IrExpr::Call { func, args } if func == "test" => {
                if let Some(t) = Self::str_arg(args, 0) {
                    self.sh_export_vars(&t);
                    self.sh_test_text(buf, &t);
                }
            }
            IrExpr::Call { func, args } if func == "exec" => {
                self.sh_stage(buf, &[IrStmt::Expr(IrExpr::Call {
                    func: func.clone(),
                    args: args.clone(),
                })]);
            }
            _ => {
                self.mark_todo(&format!("stage cond {:?}", e));
            }
        }
    }

    /// `_sh_export("name", value)` for every `$name` referenced in shell
    /// text — the bash -c child reads program vars via the environment.
    fn sh_export_vars(&mut self, text: &str) {
        let mut i = 0;
        let chars: Vec<char> = text.chars().collect();
        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1].is_ascii_alphabetic() {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                if self.var_types.contains_key(&name) || self.store.contains(&name) {
                    if self.is_num(&name) {
                        let t = self.num_temp(&self.c_ident(&name));
                        self.emit(&format!("_sh_export({}, {t});", Self::cstr(&name)));
                    } else {
                        self.emit(&format!(
                            "_sh_export({}, {});",
                            Self::cstr(&name),
                            self.store_ref(&name)
                        ));
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
    }

    /// Append the redirect text (`> f`, `>> f`, `< f`, heredoc, ...).
    fn sh_redirect_text(&mut self, buf: CmdBuf, redirects: &[crate::ir::IrRedirect]) {
        for rd in redirects {
            let mode = rd.mode.as_str();
            let fd = rd.fd.unwrap_or(1);
            let fd_pre = if fd == 1 { String::new() } else { format!("{fd}") };
            match mode {
                "w" => {
                    self.emit("_sh_addraw(\">\");");
                    self.sh_word(CmdBuf::Shared, &rd.target);
                }
                "a" => {
                    self.emit("_sh_addraw(\">>\");");
                    self.sh_word(CmdBuf::Shared, &rd.target);
                }
                "r" | "r+" => {
                    self.emit("_sh_addraw(\"<\");");
                    self.sh_word(CmdBuf::Shared, &rd.target);
                }
                "heredoc" => {
                    // target = the body content (already interpolated by
                    // the core); a quoted delimiter keeps it literal
                    self.emit("_sh_addraw(\"<<'_SH2EOF_'\\n\");");
                    let v = self.value_c(&rd.target);
                    self.emit(&format!("_sh_add({v});"));
                    self.emit("_sh_add(\"\\n_SH2EOF_\");");
                }
                "herestring" => {
                    self.emit("_sh_addraw(\"<<<\");");
                    self.sh_word(CmdBuf::Shared, &rd.target);
                }
                "process-in" => {
                    self.emit("_sh_addraw(\"<\");");
                    let v = self.value_c(&rd.target);
                    self.emit(&format!("_sh_add({v});"));
                }
                "process-out" => {
                    self.emit("_sh_addraw(\">\");");
                    let v = self.value_c(&rd.target);
                    self.emit(&format!("_sh_add({v});"));
                }
                _ => {
                    self.mark_todo(&format!("redirect mode {mode}"));
                }
            }
            let _ = fd_pre;
        }
    }

    /// Append a pipeline call's stage text (`a | b | c`).
    fn sh_pipeline_text(&mut self, buf: CmdBuf, args: &[IrExpr]) {
        let mut first = true;
        if let Some(IrExpr::Array(items)) = args.first() {
            for it in items {
                if let IrExpr::Arrow(stmts) = it {
                    if !first {
                        self.sh_raw(buf, "|");
                    }
                    first = false;
                    self.sh_stage(buf, stmts);
                }
            }
        }
    }

    /// The `exec` command dispatch (expr position — returns a C expr).
    fn exec_call(&mut self, args: &[IrExpr]) -> String {
        let Some(cmd) = Self::str_arg(args, 0) else {
            return self.shell_exec(args);
        };
        let words: Vec<&IrExpr> = match args.get(1) {
            Some(IrExpr::Array(items)) => items.iter().collect(),
            _ => vec![],
        };
        match cmd.as_str() {
            "echo" => {
                // native echo iff every word is a plain string value
                // (a split word would collapse whitespace in bash)
                if words.iter().all(|w| self.echo_native_ok(w)) {
                    let mut parts = Vec::new();
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        for (i, item) in items.iter().enumerate() {
                            if i > 0 {
                                parts.push(Part::Lit(" ".to_string()));
                            }
                            parts.extend(self.parts_of(item));
                        }
                    }
                    parts.push(Part::Lit("\n".to_string()));
                    let p = self.printf_from_parts(parts);
                    self.need_sh = true;
                    return format!("(_sh_rc = 0, {p})");
                }
                self.shell_exec(args)
            }
            "printf" => self.shell_exec(args),
            "cd" => {
                let dir = match words.first() {
                    Some(w) => self.value_c(w),
                    None => "\"\"".into(),
                };
                self.need_sh = true;
                // bash cd: silent failure (stderr is /dev/null in the
                // gate), rc 0 on success — chdir's return is inverted;
                // PWD must follow (bash keeps it in sync)
                format!(
                    "(_sh_rc = ((chdir({dir}) == 0) ? (setenv(\"PWD\", getcwd(0, 0), 1), 0) : 1))"
                )
            }
            "exit" => {
                let code = match words.first() {
                    Some(w) => self.value_num(w),
                    None => "0".into(),
                };
                format!("(exit({code}), 0)")
            }
            ":" | "true" => {
                self.need_sh = true;
                "(_sh_rc = 0, 1)".into()
            }
            "false" => {
                self.need_sh = true;
                "(_sh_rc = 1, 0)".into()
            }
            "local" | "declare" | "typeset" | "export" | "readonly" => {
                // declaration builtins: `export X=1` assigns the store;
                // bare declarations are no-ops (the hoist declares all)
                self.need_sh = true;
                self.declare_words(&words);
                "(_sh_rc = 0, 1)".into()
            }
            "unset" => {
                self.need_sh = true;
                for w in &words {
                    if let Some(name) = Self::str_arg(&[(*w).clone()], 0) {
                        if !self.is_num(&name) {
                            let id = self.c_ident(&name);
                            self.emit(&format!("{id} = \"\";"));
                        }
                    }
                }
                "(_sh_rc = 0, 1)".into()
            }
            "set" | "shift" => {
                // set -euo pipefail etc. → no-op (errexit is not
                // implemented; the corpus scripts succeed under it);
                // `set -- args` / shift mutate positionals (not tracked)
                self.need_sh = true;
                "(_sh_rc = 0, 1)".into()
            }
            "sleep" => {
                let v = match words.first() {
                    Some(w) => self.value_c(w),
                    None => "\"0\"".into(),
                };
                self.need_sh = true;
                self.need_time = true;
                format!("(_sh_rc = ((_sh_sleep({v}) == 0) ? 0 : 1))")
            }
            "read" => {
                // `read [-r] var...` — read a line into the first var
                // (stdin is the gate's /dev/null → EOF → var = "", rc 1)
                self.need_sh = true;
                let mut target = String::new();
                for w in &words {
                    if let Some(n) = Self::str_arg(&[(*w).clone()], 0) {
                        if n.starts_with('-') {
                            continue;
                        }
                        target = n.clone();
                        break;
                    }
                }
                if target.is_empty() {
                    return "(_sh_rc = 1, 0)".into();
                }
                let id = self.c_ident(&target);
                self.store.insert(target.clone());
                self.emit(&format!("{id} = _sh_readline();"));
                format!("(_sh_rc = ({id}[0] ? 0 : 1))")
            }
            "let" => {
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    if let Some(IrExpr::Str(expr, _)) = items.first() {
                        if let Some(c) = self.let_render(expr) {
                            self.need_sh = true;
                            return format!("(_sh_rc = (({c}) ? 0 : 1))");
                        }
                    }
                }
                self.need_sh = true;
                "(_sh_rc = 0, 1)".into()
            }
            _ if self.functions.contains(&cmd) => {
                // a defined shell function's call; the body's last command
                // sets _sh_rc, so `(f(), _sh_rc)` is the function's status.
                // The call args become the function's positional params:
                // set _sh_argv (save/restore around the call for nesting).
                self.need_sh = true;
                let n = words.len() + 1;
                let av = format!("_sh_av{}", self.temp_seq);
                self.temp_seq += 1;
                let sv = format!("_sh_sv{}", self.temp_seq);
                self.temp_seq += 1;
                self.emit(&format!("char *{av}[{}];", n.max(2)));
                self.emit(&format!("{av}[0] = {};", Self::cstr(&cmd)));
                for (i, w) in words.iter().enumerate() {
                    let v = self.value_c(w);
                    self.emit(&format!("{av}[{}] = {v};", i + 1));
                }
                self.emit(&format!("char **{sv} = _sh_argv; int _sh_sc{} = _sh_argc;", self.temp_seq));
                self.temp_seq += 1;
                self.emit(&format!("_sh_argv = {av}; _sh_argc = {};", n));
                format!(
                    "({}(), _sh_argv = {sv}, _sh_argc = _sh_sc{}, _sh_rc)",
                    self.c_ident(&cmd),
                    self.temp_seq - 1
                )
            }
            _ => self.shell_exec(args),
        }
    }

    /// `export X=1` / `declare x=...` — apply the assignments to the store.
    fn declare_words(&mut self, words: &[&IrExpr]) {
        for w in words {
            if let Some(ws) = Self::str_arg(&[(*w).clone()], 0) {
                if let Some((name, val)) = ws.split_once('=') {
                    if !name.is_empty()
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        self.store.insert(name.to_string());
                        let id = self.c_ident(name);
                        if self.is_num(name) {
                            match val.trim().parse::<i64>() {
                                Ok(n) => self.emit(&format!("{id} = {n};")),
                                Err(_) => self.emit(&format!("{id} = 0;")),
                            }
                        } else {
                            self.emit(&format!("{id} = {};", Self::cstr(val)));
                        }
                    }
                }
            }
        }
    }

    /// Can this word be printed by the native echo (no split, no
    /// brace-multiword, no capture in shell-out-requiring position)?
    fn echo_native_ok(&self, w: &IrExpr) -> bool {
        match w {
            IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Var(_, _) | IrExpr::Ident(_)
            | IrExpr::Arith(_) | IrExpr::BinOp { .. } | IrExpr::Bool(_) => true,
            IrExpr::Call { func, .. } => {
                !matches!(func.as_str(), "split" | "capture" | "captureWords" | "pipeline")
            }
            IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
                InterpPart::Lit(_) => true,
                InterpPart::Expr(x) => match x.as_ref() {
                    IrExpr::Call { func, .. } => {
                        !matches!(func.as_str(), "split" | "capture" | "captureWords")
                    }
                    _ => true,
                },
            }),
            _ => false,
        }
    }

    /// `$(...)` / `` `...` `` — register a capture site and return the
    /// call expression. The site's command text is built in its own
    /// private buffers (nested captures can't clobber it).
    fn capture_call(&mut self, args: &[IrExpr]) -> String {
        let args = args.to_vec();
        self.cap_site(|r, id| {
            r.emit(&format!("_sh_bres(&_c{id}_cmd, &_c{id}_cap);"));
            // body: the Arrow's statements reconstructed as command text
            let mut found = false;
            for a in &args {
                if let IrExpr::Arrow(stmts) = a {
                    found = true;
                    r.sh_stage(CmdBuf::Private(id), stmts);
                }
            }
            if !found {
                r.emit("/* empty capture */");
            }
            r.emit(&format!("_sh_capture(buf, sizeof buf, _c{id}_cmd);"));
            r.emit("return buf;");
        })
    }

    /// A shell-out exec site (statement or expr position).
    fn shell_exec(&mut self, args: &[IrExpr]) -> String {
        let args = args.to_vec();
        self.shell_site(
            |r| {
                r.emit("_sh_reset();");
                if let Some(cmd) = Self::str_arg(&args, 0) {
                    r.sh_word(
                        CmdBuf::Shared,
                        &IrExpr::Str(cmd, crate::ir::StrStyle::DoubleQuoted),
                    );
                }
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        r.sh_word(CmdBuf::Shared, w);
                    }
                }
            },
            false,
        )
    }

    /// `(( expr ))` as a site: export the vars, run `(( expr ))` via
    /// bash -c; return the INVERTED rc (bash rc==0 ⟺ value nonzero).
    fn arith_string_site(&mut self, s: &str) -> String {
        let s = s.to_string();
        self.shell_site(
            |r| {
                r.sh_export_vars(&s);
                r.emit("_sh_reset();");
                r.emit(&format!(
                    "_sh_addraw({});",
                    Self::cstr(&format!("(( {s} ))"))
                ));
            },
            true,
        )
    }

    /// A test whose text contains `$(...)`: shell out `[[ <text> ]]`
    /// (flattened forms) or `[ <text> ]` (spaced forms) — the child bash
    /// runs the command substitutions itself.
    fn test_shell_site(&mut self, s: &str) -> String {
        let s = s.to_string();
        let flat = !s.contains(' ');
        self.shell_site(
            |r| {
                r.sh_export_vars(&s);
                r.emit("_sh_reset();");
                if flat {
                    r.emit(&format!("_sh_addraw({});", Self::cstr(&format!("[[ {s} ]]"))));
                } else {
                    r.emit(&format!("_sh_addraw({});", Self::cstr(&format!("[ {s} ]"))));
                }
            },
            false,
        )
    }

    /// `let` — the ((...)) builtin arrives as a STRING ("i++", "x+=1").
    /// Parse the common single-assignment shapes natively.
    fn let_render(&self, s: &str) -> Option<String> {
        let s = s.trim();
        if let Some(rest) = s.strip_suffix("++") {
            let n = rest.trim();
            if is_ident(n) {
                return Some(format!("{}++", self.c_ident(n)));
            }
        }
        if let Some(rest) = s.strip_suffix("--") {
            let n = rest.trim();
            if is_ident(n) {
                return Some(format!("{}--", self.c_ident(n)));
            }
        }
        if let Some(rest) = s.strip_prefix("++") {
            let n = rest.trim();
            if is_ident(n) {
                return Some(format!("++{}", self.c_ident(n)));
            }
        }
        if let Some(rest) = s.strip_prefix("--") {
            let n = rest.trim();
            if is_ident(n) {
                return Some(format!("--{}", self.c_ident(n)));
            }
        }
        for op in ["+=", "-=", "*=", "/=", "%="] {
            if let Some((l, r)) = s.split_once(op) {
                let l = l.trim();
                if is_ident(l) && r.trim().parse::<i64>().is_ok() {
                    return Some(format!("{} {op} {}", self.c_ident(l), r.trim()));
                }
            }
        }
        // `let "x=$x+1"` — rhs with a $var ref
        for op in ["+=", "-=", "*=", "/=", "%="] {
            if let Some((l, r)) = s.split_once(op) {
                let l = l.trim();
                if !is_ident(l) {
                    continue;
                }
                let r = r.trim();
                if let Some(rv) = r.strip_prefix('$') {
                    if is_ident(rv) {
                        return Some(format!("{} {op} {}", self.c_ident(l), self.c_ident(rv)));
                    }
                }
            }
        }
        None
    }

    // ── test lowering ────────────────────────────────────────────────

    /// Quote-aware test tokenizer (mirrors the perl renderer's).
    fn test_tokens(&self, s: &str) -> Vec<String> {
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
            while j < chars.len() && !chars[j].is_whitespace() {
                j += 1;
            }
            toks.push(chars[i..j].iter().collect());
            i = j;
        }
        toks
    }

    /// `[ ... ]` full evaluator — file tests, numeric/string compares,
    /// glob/regex, -a/-o/!/parens. Returns a C int expression.
    fn test_render(&mut self, s: &str) -> String {
        let trimmed = s.trim();
        // flattened forms: `$s==*.txt`, `"$x"="1"` — no spaces
        if !trimmed.contains(' ') {
            for op in ["==", "!=", "=~", "="] {
                if let Some(pos) = trimmed.find(op) {
                    let a = trimmed[..pos].trim().to_string();
                    let b = trimmed[pos + op.len()..].trim().to_string();
                    let l = self.test_value(&a);
                    let r = self.test_value(&b);
                    return self.test_compare(op, &l, &r, &a, &b);
                }
            }
            let v = self.test_value(trimmed);
            return format!("(({v}) && ({v})[0])");
        }
        let toks = self.test_tokens(trimmed);
        self.test_tokens_parse(&toks)
    }

    fn test_tokens_parse(&mut self, toks: &[String]) -> String {
        if toks.is_empty() {
            // `[ ]` with no args → false
            return "0".into();
        }
        // `\( ... \)` parens
        if toks[0] == "\\(" {
            if toks.last() == Some(&"\\)".to_string()) {
                return self.test_tokens_parse(&toks[1..toks.len() - 1]);
            }
        }
        // `!` prefix
        if toks[0] == "!" {
            let inner = self.test_tokens_parse(&toks[1..]);
            return format!("(!{inner})");
        }
        // lowest precedence: `||` / `-o` (split at the LAST one)
        for (i, t) in toks.iter().enumerate().rev() {
            if t == "||" || t == "-o" {
                let l = self.test_tokens_parse(&toks[..i]);
                let r = self.test_tokens_parse(&toks[i + 1..]);
                return format!("({l} || {r})");
            }
        }
        for (i, t) in toks.iter().enumerate().rev() {
            if t == "&&" || t == "-a" {
                let l = self.test_tokens_parse(&toks[..i]);
                let r = self.test_tokens_parse(&toks[i + 1..]);
                return format!("({l} && {r})");
            }
        }
        match toks.len() {
            1 => {
                let v = self.test_value(&toks[0]);
                format!("(({v}) && ({v})[0])")
            }
            2 => {
                let (flag, v) = (&toks[0], self.test_value(&toks[1]));
                match flag.as_str() {
                    "-n" => format!("(({v}) && ({v})[0])"),
                    "-z" => format!("(!({v}) || !({v})[0])"),
                    "-f" | "-d" | "-e" | "-s" | "-r" | "-w" | "-x" | "-L" => {
                        self.need_stat = true;
                        format!("_sh_is_{}({v})", &flag[1..])
                    }
                    "-a" => {
                        self.need_stat = true;
                        format!("_sh_is_e({v})")
                    }
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
            "-gt" => format!("(atoll({l}) > atoll({r}))"),
            "-lt" => format!("(atoll({l}) < atoll({r}))"),
            "-ge" => format!("(atoll({l}) >= atoll({r}))"),
            "-le" => format!("(atoll({l}) <= atoll({r}))"),
            "-eq" => format!("(atoll({l}) == atoll({r}))"),
            "-ne" => format!("(atoll({l}) != atoll({r}))"),
            "=" | "==" | "!=" => {
                let has_glob = raw_l.contains('*')
                    || raw_l.contains('?')
                    || raw_r.contains('*')
                    || raw_r.contains('?');
                if has_glob {
                    // `[[ x == pattern ]]` — glob match (fnmatch);
                    // `!(...)` extglob approximated (negated fnmatch)
                    self.need_fnmatch = true;
                    let neg = op == "!=";
                    let pat = if neg { raw_r.clone() } else { raw_r.clone() };
                    if let Some(inner) = pat.strip_prefix("!(") {
                        if let Some(rest) = inner.split_once(')') {
                            let inner_pat = format!("{}{}", rest.0, rest.1);
                            let flags = if self.nocasematch { ", FNM_CASEFOLD" } else { "" };
                            let m = format!("fnmatch({}, {l}, 0{flags}) == 0", Self::cstr(&inner_pat));
                            return if neg {
                                format!("(!{m})")
                            } else {
                                format!("({m})")
                            };
                        }
                    }
                    let flags = if self.nocasematch { ", FNM_CASEFOLD" } else { "" };
                    let m = format!("fnmatch({}, {l}, 0{flags}) == 0", Self::cstr(&pat));
                    if neg {
                        format!("(!{m})")
                    } else {
                        m
                    }
                } else if op == "!=" {
                    format!("(strcmp({l}, {r}) != 0)")
                } else {
                    format!("(strcmp({l}, {r}) == 0)")
                }
            }
            "=~" => {
                self.need_regex = true;
                let t = format!("_s{}", self.temp_seq);
                self.temp_seq += 1;
                self.emit(&format!("char {t}[1024];"));
                self.emit(&format!(
                    "snprintf({t}, sizeof {t}, \"^(?:%s)$\", {r});"
                ));
                format!("_sh_regex_match({l}, {t})")
            }
            _ => {
                self.mark_todo(&format!("test op {op}"));
                "0".into()
            }
        }
    }

    /// A test operand → char* C expression (numeric vars via num_temp).
    fn test_value(&mut self, t: &str) -> String {
        let raw = t.trim();
        let dequoted = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(raw);
        let stripped = dequoted.strip_prefix('$').unwrap_or(dequoted);
        if self.var_types.contains_key(stripped) {
            if self.is_num(stripped) {
                self.num_temp(&self.c_ident(stripped))
            } else {
                self.store_ref(stripped)
            }
        } else if dequoted.starts_with('$') && is_ident(stripped) {
            self.store.insert(stripped.to_string());
            self.store_ref(stripped)
        } else if raw.starts_with('$') && raw.len() > 1 {
            // positional / special ($1, $#, $@) — empty argv in the gate
            self.need_sh = true;
            "\"\"".into()
        } else {
            Self::cstr(stripped)
        }
    }

    // ── parameter expansion ──────────────────────────────────────────

    fn param_call(&mut self, args: &[IrExpr]) -> String {
        let Some(op) = Self::str_arg(args, 0) else {
            return "0".into();
        };
        let Some(name) = Self::str_arg(args, 1) else {
            return "0".into();
        };
        // `${arr[1]}` / `${#arr[@]}` — arrays (wave 2 lowers them; the
        // array machinery registers the name)
        if name.contains('[') || name.contains('@') || name.contains('*') {
            return self.param_array(&op, &name);
        }
        // `${#x}` — string length
        if op == "#" || op == "len" {
            let v = self.value_c(&IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str(name.clone(), crate::ir::StrStyle::DoubleQuoted)],
            });
            return self.num_temp(&format!("(long long)strlen({v})"));
        }
        let var_expr = if name.is_empty() {
            "\"\"".to_string()
        } else if name == "#" {
            self.need_sh = true;
            "(_sh_argc - 1)".into()
        } else if name == "@" || name == "*" {
            self.need_sh = true;
            let t = self.str_temp(4096);
            self.emit(&format!("_sh_argv_join({t}, sizeof {t});"));
            t
        } else if self.var_types.contains_key(&name) && self.is_num(&name) {
            self.num_temp(&self.c_ident(&name))
        } else if name.starts_with('$') || name.chars().all(|c| c.is_ascii_digit()) {
            // positional $N — the function-call argv (empty at top level)
            self.need_sh = true;
            format!("(({name} < _sh_argc && _sh_argv[{name}]) ? _sh_argv[{name}] : \"\")")
        } else {
            self.store.insert(name.clone());
            self.store_ref(&name)
        };
        let val = args.get(2).map(|x| self.value_c(x)).unwrap_or_else(|| "\"\"".into());
        let repl = args.get(3).map(|x| self.value_c(x)).unwrap_or_else(|| "\"\"".into());
        match op.as_str() {
            "" => var_expr,
            "-" => format!("(({var_expr}) ? ({var_expr}) : ({val}))"),
            ":-" => format!("((({var_expr}) && ({var_expr})[0]) ? ({var_expr}) : ({val}))"),
            ":?" => format!(
                "((({var_expr}) && ({var_expr})[0]) ? ({var_expr}) : (fprintf(stderr, \"%s\\n\", {val}), exit(1), (char*)0))"
            ),
            "=" | ":=" => format!(
                "((({var_expr}) && ({var_expr})[0]) ? ({var_expr}) : ({val}))"
            ),
            "#" | "#:" | "##" | "##:" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                self.need_sh = true;
                self.need_fnmatch = true;
                let t = self.str_temp(4096);
                let greedy = if op.starts_with("##") { "1" } else { "0" };
                self.emit(&format!(
                    "_sh_strippre({t}, sizeof {t}, {var_expr}, {}, {greedy});",
                    Self::cstr(&pat)
                ));
                t
            }
            "%" | "%:" | "%%" | "%%:" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                self.need_sh = true;
                self.need_fnmatch = true;
                let t = self.str_temp(4096);
                let greedy = if op.starts_with("%%") { "1" } else { "0" };
                self.emit(&format!(
                    "_sh_stripsuf({t}, sizeof {t}, {var_expr}, {}, {greedy});",
                    Self::cstr(&pat)
                ));
                t
            }
            "//" | "/" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                self.need_sh = true;
                let t = self.str_temp(4096);
                self.emit(&format!(
                    "_sh_replace({t}, sizeof {t}, {var_expr}, {}, {repl});",
                    Self::cstr(&pat)
                ));
                t
            }
            "slice" => {
                let off = args.get(2).map(|x| self.value_num(x)).unwrap_or_else(|| "0".into());
                let len = args.get(3).map(|x| self.value_num(x)).unwrap_or_else(|| "-1".into());
                self.need_sh = true;
                let t = self.str_temp(4096);
                self.emit(&format!(
                    "_sh_substr({t}, sizeof {t}, {var_expr}, {off}, {len});"
                ));
                t
            }
            _ => {
                self.mark_todo(&format!("param op {op}"));
                var_expr
            }
        }
    }

    /// `${arr[i]}` / `${#arr[@]}` / `${arr[@]:off:len}` — the array
    /// store (wave 2). Until the array machinery lands, register the
    /// name and render a stub-free empty (files with arrays fail the
    /// equivalence gate until then).
    fn param_array(&mut self, op: &str, name: &str) -> String {
        let _ = op;
        let _ = name;
        // strip the [..] suffix: `arr[1]` → (arr, 1)
        if let Some(open) = name.find('[') {
            let var = &name[..open];
            let key = &name[open + 1..name.len() - 1];
            return self.array_index_read(var, key);
        }
        // `${#arr[@]}` — length
        if let Some(rest) = name.strip_prefix('#') {
            let rest = rest
                .strip_suffix("[@]")
                .or_else(|| rest.strip_suffix("[*]"))
                .unwrap_or(rest);
            if !rest.is_empty() {
                return self.array_len(rest);
            }
        }
        self.store.insert(name.to_string());
        self.store_ref(name)
    }

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "exec" => self.exec_call(args),
            "getVar" => {
                let Some(name) = Self::str_arg(args, 0) else {
                    return "0".into();
                };
                if name == "?" {
                    self.need_sh = true;
                    return "_sh_rc".into();
                }
                if name == "#" {
                    self.need_sh = true;
                    return "(_sh_argc - 1)".into();
                }
                if name == "@" || name == "*" {
                    self.need_sh = true;
                    let t = self.str_temp(4096);
                    self.emit(&format!("_sh_argv_join({t}, sizeof {t});"));
                    return t;
                }
                if name.chars().all(|c| c.is_ascii_digit()) {
                    // positional $N — the function-call argv (empty at top)
                    self.need_sh = true;
                    return format!(
                        "(({name} < _sh_argc && _sh_argv[{name}]) ? _sh_argv[{name}] : \"\")"
                    );
                }
                if self.var_types.contains_key(&name) {
                    self.c_ident(&name)
                } else if self.store.contains(&name) {
                    self.store_ref(&name)
                } else {
                    // an environment variable (HOME, PATH, ...): read the
                    // real environment (the gate runs with the same env)
                    self.need_sh = true;
                    format!("(getenv({}) ? getenv({}) : \"\")", Self::cstr(&name), Self::cstr(&name))
                }
            }
            "param" => self.param_call(args),
            "setVar" => {
                let (Some(name), Some(value)) = (Self::str_arg(args, 0), args.get(1)) else {
                    return "0".into();
                };
                self.store.insert(name.clone());
                format!("({} = {})", self.c_ident(&name), self.value_c(value))
            }
            "assign" => {
                let (Some(name), Some(op)) = (Self::str_arg(args, 0), Self::str_arg(args, 1))
                else {
                    return "0".into();
                };
                self.store.insert(name.clone());
                let id = self.c_ident(&name);
                let Some(value) = args.get(2) else {
                    return "0".into();
                };
                if op == "=" {
                    format!("({id} = {})", self.value_c(value))
                } else if self.is_num(&name) {
                    format!("({id} {op} {})", self.expr(value))
                } else {
                    let v = self.value_c(value);
                    let t = self.num_temp(&format!("(atoll({id}) {op} atoll({v}))"));
                    format!("({id} = {t})")
                }
            }
            "arith" => match Self::str_arg(args, 0) {
                Some(s) => self.arith_string_site(&s),
                None => "0".into(),
            },
            "test" => match Self::str_arg(args, 0) {
                Some(s) => {
                    if s.contains("$(") {
                        return self.test_shell_site(&s);
                    }
                    self.test_render(&s)
                }
                None => "0".into(),
            },
            "capture" | "captureWords" => self.capture_call(args),
            "pipeline" => {
                let args = args.to_vec();
                self.shell_site(
                    |r| {
                        r.emit("_sh_reset();");
                        r.sh_pipeline_text(CmdBuf::Shared, &args);
                    },
                    false,
                )
            }
            "redirect" => self.redirect_expr(args),
            "let" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.let_render(s) {
                        self.need_sh = true;
                        return format!("(_sh_rc = (({c}) ? 0 : 1))");
                    }
                }
                self.need_sh = true;
                "(_sh_rc = 0, 1)".into()
            }
            "shopt" => {
                let s = Self::str_arg(args, 0).unwrap_or_default();
                if s.contains("nocasematch") && s.contains("-s") {
                    self.nocasematch = true;
                }
                self.need_sh = true;
                self.need_fnmatch = true;
                "(_sh_rc = 0, 1)".into()
            }
            "break" => {
                self.need_sh = true;
                "(_sh_rc = 0, 0)".into()
            }
            "continue" => {
                self.need_sh = true;
                "(_sh_rc = 0, 1)".into()
            }
            "return" => {
                let v = match args.first() {
                    Some(x) => self.value_num(x),
                    None => "0".into(),
                };
                self.need_sh = true;
                if self.in_function {
                    format!("(_sh_rc = {v}, 0)")
                } else {
                    format!("(return {v}, 0)")
                }
            }
            "contains" => {
                if let (Some(needle), Some(pattern)) = (args.first(), args.get(1)) {
                    let needle_c = if self.expr_is_num(needle) {
                        let t = format!("_s{}", self.temp_seq);
                        self.temp_seq += 1;
                        let width = self.expr_width(needle);
                        let cap = width_buf_len(width);
                        self.emit(&format!("char {t}[{cap}];"));
                        let e = self.expr(needle);
                        let NumSpec::Num(spec, cast) = self.num_spec(needle) else {
                            unreachable!("numeric needle → numeric spec")
                        };
                        let arg = if cast {
                            format!("(long long)({e})")
                        } else {
                            e
                        };
                        self.emit(&format!("snprintf({t}, sizeof {t}, \"{spec}\", {arg});"));
                        t
                    } else {
                        self.expr(needle)
                    };
                    return format!("strstr({needle_c}, {}) != NULL", self.expr(pattern));
                }
                "0".into()
            }
            "brace" => Self::cstr(&brace_expand(args).join(" ")),
            "split" => match args.first() {
                Some(x) => self.value_c(x),
                None => "\"\"".into(),
            },
            "block" | "subshell" => {
                if let Some(IrExpr::Arrow(stmts)) = args.first() {
                    let stmts = stmts.clone();
                    self.shell_site(
                        |r| {
                            r.emit("_sh_reset();");
                            r.sh_stage(CmdBuf::Shared, &stmts);
                        },
                        false,
                    )
                } else {
                    "0".into()
                }
            }
            "join" => {
                // `${arr[@]}` in print position — the elements joined by
                // a space (a single element is just its value)
                let items: Vec<IrExpr> = match args.first() {
                    Some(IrExpr::Array(items)) => items.clone(),
                    Some(other) => vec![other.clone()],
                    None => Vec::new(),
                };
                if items.is_empty() {
                    return "\"\"".into();
                }
                if items.len() == 1 {
                    return self.value_c(&items[0]);
                }
                let mut fmt = String::new();
                let mut cargs: Vec<String> = Vec::new();
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        fmt.push_str(" ");
                    }
                    fmt.push_str("%s");
                    cargs.push(self.value_c(it));
                }
                let t = self.str_temp(4096);
                self.emit(&format!(
                    "snprintf({t}, sizeof {t}, \"{fmt}\", {});",
                    cargs.join(", ")
                ));
                t
            }
            "whileLoop" => {
                if let (Some(IrExpr::Arrow(stmts)), Some(cond)) = (args.first(), args.get(1)) {
                    let stmts = stmts.clone();
                    let cond = cond.clone();
                    self.shell_site(
                        |r| {
                            r.emit("_sh_reset();");
                            r.sh_stage(CmdBuf::Shared, &stmts);
                            let _ = r.expr(&cond);
                        },
                        false,
                    )
                } else {
                    "0".into()
                }
            }
            _ if self.functions.contains(func) => {
                let id = self.c_ident(func);
                self.need_sh = true;
                format!("({id}(), _sh_rc)")
            }
            _ => self.sh2_stub(func, args, func),
        }
    }

    /// A redirect call (expr position): reconstruct `cmd <redirs>` and
    /// run it.
    fn redirect_expr(&mut self, args: &[IrExpr]) -> String {
        let args = args.to_vec();
        self.shell_site(
            |r| {
                r.emit("_sh_reset();");
                if let Some(IrExpr::Arrow(stmts)) = args.first() {
                    r.sh_stage(CmdBuf::Shared, stmts);
                }
                if let Some(IrExpr::Array(specs)) = args.get(1) {
                    for sp in specs {
                        if let IrExpr::Object(fields) = sp {
                            let mut fd = 1;
                            let mut mode = String::new();
                            let mut target =
                                IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted);
                            for (k, v) in fields {
                                match k.as_str() {
                                    "fd" => {
                                        if let IrExpr::Int(n) = v {
                                            fd = *n;
                                        }
                                    }
                                    "mode" => {
                                        mode = Self::str_arg(&[v.clone()], 0).unwrap_or_default();
                                    }
                                    "target" => target = v.clone(),
                                    _ => {}
                                }
                            }
                            let rds = [crate::ir::IrRedirect {
                                fd: Some(fd as i32),
                                mode,
                                target,
                                interpolate: true,
                            }];
                            r.sh_redirect_text(CmdBuf::Shared, &rds);
                        }
                    }
                }
            },
            false,
        )
    }

    // ── arrays (wave 2) ──────────────────────────────────────────────

    fn array_index_read(&mut self, var: &str, key: &str) -> String {
        // stub-free placeholder until the array store lands
        let _ = key;
        self.store.insert(var.to_string());
        self.store_ref(var)
    }

    fn array_len(&mut self, var: &str) -> String {
        let _ = var;
        self.need_sh = true;
        "0".into()
    }

    /// Split an expression into printf parts: Lit(text) | Arg(cexpr, is_num).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<Part> {
        match e {
            IrExpr::Str(s, _) => vec![Part::Lit(s.clone())],
            IrExpr::Int(i) => vec![Part::Arg(i.to_string(), self.num_spec(e))],
            IrExpr::Var(name, _) => {
                if self.is_num(name) {
                    vec![Part::Arg(self.c_ident(name), self.num_spec(e))]
                } else {
                    vec![Part::Arg(self.c_ident(name), NumSpec::Str)]
                }
            }
            IrExpr::Ident(name) => vec![Part::Arg(self.c_ident(name), NumSpec::Str)],
            IrExpr::Interpolate(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push(Part::Lit(s.clone())),
                        InterpPart::Expr(x) => {
                            let spec = if self.expr_is_num(x) {
                                self.num_spec(x)
                            } else {
                                NumSpec::Str
                            };
                            out.push(Part::Arg(self.expr(x), spec))
                        }
                    }
                }
                out
            }
            IrExpr::Arith(a) => vec![Part::Arg(self.arith(a), self.num_spec(e))],
            IrExpr::BinOp { .. } => vec![Part::Arg(self.expr(e), self.num_spec(e))],
            IrExpr::Call { func, args } => {
                let args = args.clone();
                // getVar("x") → ident if x is typed, else the store read
                if func == "getVar" {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        if name == "?" {
                            return vec![Part::Arg("_sh_rc".into(), NumSpec::Num("%d", true))];
                        }
                        if name == "#" {
                            return vec![Part::Arg("(_sh_argc - 1)".into(), NumSpec::Num("%d", true))];
                        }
                        if name == "@" || name == "*" {
                            self.need_sh = true;
                            let t = self.str_temp(4096);
                            self.emit(&format!("_sh_argv_join({t}, sizeof {t});"));
                            return vec![Part::Arg(t, NumSpec::Str)];
                        }
                        if name.chars().all(|c| c.is_ascii_digit()) {
                            self.need_sh = true;
                            return vec![Part::Arg(
                                format!(
                                    "(({name} < _sh_argc && _sh_argv[{name}]) ? _sh_argv[{name}] : \"\")"
                                ),
                                NumSpec::Str,
                            )];
                        }
                        if self.var_types.contains_key(name) {
                            let spec = if self.is_num(name) {
                                self.num_spec(e)
                            } else {
                                NumSpec::Str
                            };
                            return vec![Part::Arg(self.c_ident(name), spec)];
                        }
                        if self.store.contains(name) {
                            return vec![Part::Arg(self.store_ref(name), NumSpec::Str)];
                        }
                        return vec![Part::Arg(
                            format!(
                                "(getenv({}) ? getenv({}) : \"\")",
                                Self::cstr(name),
                                Self::cstr(name)
                            ),
                            NumSpec::Str,
                        )];
                    }
                    return vec![Part::Arg(self.call(func, &args), NumSpec::Str)];
                }
                // other calls: render the call expression; numeric iff the
                // expression is numeric (stubs return long long → %lld+cast)
                let spec = if self.expr_is_num(e) {
                    self.num_spec(e)
                } else {
                    NumSpec::Str
                };
                vec![Part::Arg(self.call(func, &args), spec)]
            }
            other => {
                self.mark_todo(&format!("echo arg {:?}", other));
                vec![Part::Arg("0".into(), NumSpec::Num("%lld", true))]
            }
        }
    }

    fn expr_is_num(&mut self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Var(name, _) => self.is_num(name),
            IrExpr::Int(_) | IrExpr::Arith(_) | IrExpr::BinOp { .. } => true,
            // `$y` reads arrive as getVar("y"); a typed-Int var is numeric
            IrExpr::Call { func, args } if func == "getVar" => {
                matches!(args.first(), Some(IrExpr::Str(name, _)) if name == "?" || self.is_num(name))
            }
            _ => false,
        }
    }

    fn printf_from_parts(&mut self, parts: Vec<Part>) -> String {
        let mut fmt = String::new();
        let mut cargs = Vec::new();
        for p in parts {
            match p {
                Part::Lit(t) => fmt.push_str(&t),
                Part::Arg(v, spec) => match spec {
                    // numeric: the spec already matches the operand's
                    // proven width — cast only when the type is unproven
                    // (num_spec's invariant: cast ⟺ spec == "%lld")
                    NumSpec::Num(spec, cast) => {
                        fmt.push_str(spec);
                        if cast {
                            cargs.push(format!("(long long)({v})"));
                        } else {
                            cargs.push(v);
                        }
                    }
                    NumSpec::Str => {
                        fmt.push_str("%s");
                        // cast: the arg may be a stub call returning
                        // long long — printf("%s", long long) is UB.
                        cargs.push(format!("(char*)({v})"));
                    }
                },
            }
        }
        if cargs.is_empty() {
            format!("fputs({}, stdout)", Self::cstr(&fmt))
        } else {
            format!("printf({}, {})", Self::cstr(&fmt), cargs.join(", "))
        }
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => {
                match e {
                    IrExpr::Call { func, args } if func == "break" => {
                        self.emit("break;");
                        return;
                    }
                    IrExpr::Call { func, args } if func == "continue" => {
                        self.emit("continue;");
                        return;
                    }
                    IrExpr::Call { func, args } if func == "return" => {
                        let v = match args.first() {
                            Some(x) => self.value_num(x),
                            None => "0".into(),
                        };
                        if self.in_function {
                            self.need_sh = true;
                            self.emit(&format!("_sh_rc = {v};"));
                            self.emit("return;");
                        } else {
                            self.emit(&format!("return {v};"));
                        }
                        return;
                    }
                    IrExpr::Call { func, args } if func == "whileLoop" => {
                        // whileLoop(cond-Arrow, body-Arrow): `while C; do B; done`
                        if let (Some(IrExpr::Arrow(cond)), Some(IrExpr::Arrow(body))) =
                            (args.first(), args.get(1))
                        {
                            let (cond_c, body_stmts) = (cond.clone(), body.clone());
                            let site = self.shell_site(
                                |r| {
                                    let mut v = "1".to_string();
                                    for st in &cond_c {
                                        match st {
                                            IrStmt::Expr(e) => v = r.expr(e),
                                            other => r.stmt(other),
                                        }
                                    }
                                    r.emit(&format!("return ({v});"));
                                },
                                false,
                            );
                            self.emit(&format!("while ({site}) {{"));
                            self.depth += 1;
                            for s in &body_stmts {
                                self.stmt(s);
                            }
                            self.depth -= 1;
                            self.emit("}");
                        }
                        return;
                    }
                    _ => {}
                }
                let x = self.expr(e);
                self.emit(&format!("{x};"));
            }
            IrStmt::Assign { targets, expr } => {
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
                }
                // const-markup lift: the declaration already carries the
                // literal initializer (emit_var_decl) — the assignment
                // statement is redundant (the verdict guarantees this is
                // the var's only write).
                if self.const_lifted.contains(&t.var) {
                    return;
                }
                let name = self.c_ident(&t.var);
                if let Some(b) = self.buf_bound(&t.var) {
                    // a bounded string var: the debug-only length assert
                    // fires BEFORE the write that would overflow the
                    // fixed buffer (see emit_guarded_copy).
                    let rhs = self.expr(expr);
                    self.emit_guarded_copy(&name, b, &rhs);
                    return;
                }
                let is_num = self.is_num(&t.var);
                let rhs = if is_num {
                    self.expr_as_num(expr)
                } else {
                    self.expr(expr)
                };
                // A stub call (sh2_*) returns long long; cast for char*
                // targets so the draft always compiles (the stub exits 2
                // before returning, so the value never matters).
                if !is_num && rhs.starts_with("sh2_") {
                    self.emit(&format!("{name} = (char*)({rhs});"));
                } else {
                    self.emit(&format!("{name} = {rhs};"));
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                let init_expr = init.as_ref().map(|e| self.expr(e));
                for d in vars {
                    let name = self.c_ident(&d.name);
                    if self.is_num(&d.name) {
                        let v = init_expr.clone().unwrap_or_else(|| "0".into());
                        self.emit(&format!("{} {name} = {v};", self.width_of_var(&d.name).c_type()));
                    } else if let Some(b) = self.buf_bound(&d.name) {
                        self.emit(&format!("char {name}[{}] = \"\";", b + 1));
                        if let Some(v) = init_expr.clone() {
                            self.emit_guarded_copy(&name, b, &v);
                        }
                    } else {
                        let v = init_expr.clone().unwrap_or_else(|| "NULL".into());
                        self.emit(&format!("char* {name} = {v};"));
                    }
                }
            }
            IrStmt::Output { value, newline, .. } => {
                let v = self.value_c(value);
                if *newline {
                    self.emit(&format!("printf(\"%s\\n\", (char*)({v}));"));
                } else {
                    self.emit(&format!("fputs((char*)({v}), stdout);"));
                }
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                let c = self.expr(cond);
                self.emit(&format!("if ({c}) {{"));
                self.depth += 1;
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.expr(ec);
                    self.emit(&format!("}} else if ({ec}) {{"));
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
            IrStmt::Exit(e) => {
                let code = e.as_ref().map(|x| self.expr(x)).unwrap_or_else(|| "0".into());
                self.emit(&format!("return {code};"));
            }
            IrStmt::For { var, iter, body } => {
                // seq-range lift: `for x in $(seq a b)` (captureWords →
                // arrow → exec "seq") OR a core-lowered IrExpr::Range →
                // traditional numeric C loop. The A2 verdict for the loop
                // var is usually Str (captureWords returns strings), so the
                // lift overrides it to Int for the loop scope and restores
                // it afterwards.
                if let Some((first, last, step)) = seq_iter_range(iter) {
                    let name = self.c_ident(var);
                    let prev_type = self.var_types.get(var).copied();
                    self.var_types.insert(var.clone(), IrType::Int);
                    let cmp = if step > 0 { "<=" } else { ">=" };
                    let upd = match step {
                        1 => format!("{name}++"),
                        -1 => format!("{name}--"),
                        s => format!("{name} += {s}"),
                    };
                    // the loop var's width comes from the range analysis
                    // (seed + every arith expr in the body that mentions
                    // it) — the hoist declared it at that width
                    self.emit(&format!(
                        "for ({name} = {first}; {name} {cmp} {last}; {upd}) {{"
                    ));
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                    match prev_type {
                        Some(t) => {
                            self.var_types.insert(var.clone(), t);
                        }
                        None => {
                            self.var_types.remove(var);
                        }
                    }
                    return;
                }
                // Emit a C for loop over an index variable; each iteration
                // assigns the loop var from a static items array. Supports
                // Int vars with numeric items and string vars with string
                // items; brace iters expand at compile time; captureWords
                // iters (`for x in $(cmd)`) capture once + split.
                let mut items = match iter {
                    IrExpr::Array(items) => items.clone(),
                    IrExpr::Call { func, args } if func == "brace" => brace_expand(args)
                        .into_iter()
                        .map(|s| IrExpr::Str(s, crate::ir::StrStyle::DoubleQuoted))
                        .collect(),
                    _ => {
                        self.mark_todo("for iter not Array");
                        return;
                    }
                };
                if items.len() == 1 {
                    if let IrExpr::Call { func, args } = &items[0] {
                        if func == "brace" {
                            items = brace_expand(args)
                                .into_iter()
                                .map(|s| IrExpr::Str(s, crate::ir::StrStyle::DoubleQuoted))
                                .collect();
                        }
                    }
                }
                // `for x in $(cmd)` — capture once, split on whitespace
                if items.len() == 1 {
                    if let IrExpr::Call { func, .. } = &items[0] {
                        if func == "captureWords" || func == "capture" {
                            let cap = self.capture_call(&[items[0].clone()]);
                            self.need_sh = true;
                            let wn = format!("_wn_{}", self.temp_seq);
                            self.temp_seq += 1;
                            let ws = format!("_ws_{}", self.temp_seq);
                            self.temp_seq += 1;
                            self.emit(&format!(
                                "char *{wn} = {cap}; char *{ws}[1024]; size_t _wc_{wn} = _sh_split({wn}, {ws}, 1024);"
                            ));
                            let var_name = self.c_ident(var);
                            self.emit(&format!(
                                "for (size_t _wi_{wn} = 0; _wi_{wn} < _wc_{wn}; _wi_{wn}++) {{"
                            ));
                            self.depth += 1;
                            self.emit(&format!(
                                "{var_name} = {ws}[_wi_{wn}];"
                            ));
                            for s in body {
                                self.stmt(s);
                            }
                            self.depth -= 1;
                            self.emit("}");
                            return;
                        }
                    }
                }
                let n = items.len();
                if n == 0 {
                    return;
                }
                let var_name = self.c_ident(var);
                let is_num = self.is_num(var);
                let arr_id = format!("_for_{var_name}");
                if is_num {
                    let mut values = Vec::new();
                    let mut ok = true;
                    for item in &items {
                        match item {
                            IrExpr::Int(i) => values.push(i.to_string()),
                            IrExpr::Str(s, _) => match s.trim().parse::<i64>() {
                                Ok(n) => values.push(n.to_string()),
                                Err(_) => {
                                    self.mark_todo("for item not numeric");
                                    ok = false;
                                    break;
                                }
                            },
                            _ => {
                                self.mark_todo("for item type");
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok { return; }
                    self.emit(&format!(
                        "static const long long {arr_id}[] = {{{}}};",
                        values.join(", ")
                    ));
                    self.emit(&format!(
                        "for (size_t _i_{var_name} = 0; _i_{var_name} < {n}; _i_{var_name}++) {{"
                    ));
                    self.depth += 1;
                    self.emit(&format!("{var_name} = {arr_id}[_i_{var_name}];"));
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                } else {
                    let mut values = Vec::new();
                    let mut ok = true;
                    for item in &items {
                        match item {
                            IrExpr::Str(s, _) => values.push(Self::cstr(s)),
                            _ => {
                                self.mark_todo("for item type");
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok { return; }
                    self.emit(&format!(
                        "static const char* {arr_id}[] = {{{}}};",
                        values.join(", ")
                    ));
                    self.emit(&format!(
                        "for (size_t _i_{var_name} = 0; _i_{var_name} < {n}; _i_{var_name}++) {{"
                    ));
                    self.depth += 1;
                    self.emit(&format!("{var_name} = (char*){arr_id}[_i_{var_name}];"));
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                }
            }
            IrStmt::While { cond, body } => {
                // the cond is an IrExpr (the ShIR's `[ ... ]` is a
                // Call("test") -> test_render, `while true` -> "1").
                // Wrapped in a site helper: any numeric temps the cond
                // emits must refresh EVERY iteration, not hoist before
                // the loop.
                let c = self.cond_site(cond);
                self.emit(&format!("while ({c}) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::DoWhile { body, cond, until } => {
                let c = self.cond_site(cond);
                self.emit("do {");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                if *until {
                    self.emit(&format!("}} while (!({c}));"));
                } else {
                    self.emit(&format!("}} while ({c});"));
                }
            }
            IrStmt::Function { .. } => {
                // definitions are emitted in the preamble (before main);
                // calls arrive as exec("<name>") and render `name();`.
            }
            IrStmt::Redirect { inner, redirects } => {
                // `cmd > file 2>&1` — reconstruct the full shell text and
                // run it (bash applies the redirections exactly)
                let inner = inner.clone();
                let redirects = redirects.clone();
                let site = self.shell_site(
                    |r| {
                        r.emit("_sh_reset();");
                        r.sh_stage(CmdBuf::Shared, &inner);
                        r.sh_redirect_text(CmdBuf::Shared, &redirects);
                    },
                    false,
                );
                self.emit(&format!("{site};"));
            }
            IrStmt::Pipeline { stages, capture, .. } => {
                let stages = stages.clone();
                let capture = capture.clone();
                if let Some(var) = capture {
                    // pipeline stdout captured into $var
                    self.store.insert(var.clone());
                    let id = self.c_ident(&var);
                    let args = vec![IrExpr::Array(
                        stages
                            .iter()
                            .map(|st| IrExpr::Arrow(st.clone()))
                            .collect(),
                    )];
                    let cap = self.capture_call(&args);
                    self.emit(&format!("{id} = {cap};"));
                } else {
                    let args = vec![IrExpr::Array(
                        stages
                            .iter()
                            .map(|st| IrExpr::Arrow(st.clone()))
                            .collect(),
                    )];
                    let site = self.shell_site(
                        |r| {
                            r.emit("_sh_reset();");
                            r.sh_pipeline_text(CmdBuf::Shared, &args);
                        },
                        false,
                    );
                    self.emit(&format!("{site};"));
                }
            }
            IrStmt::Block(body) | IrStmt::Background(body) => {
                self.emit("{");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Subshell(body) => {
                // copy semantics: vars assigned in the subshell are
                // saved and restored around the body
                let mut assigned: BTreeSet<String> = BTreeSet::new();
                collect_assigned_vars(body, &mut assigned);
                self.emit("{");
                self.depth += 1;
                let mut saves: Vec<String> = Vec::new();
                for v in &assigned {
                    let id = self.c_ident(v);
                    if self.is_num(v) {
                        saves.push(format!("long long _sv_{id} = {id};"));
                    } else {
                        saves.push(format!("char* _sv_{id} = {id};"));
                    }
                }
                for s in &saves {
                    self.emit(s);
                }
                if !saves.is_empty() {
                    self.emit("");
                }
                for s in body {
                    self.stmt(s);
                }
                if !saves.is_empty() {
                    self.emit("");
                    for v in &assigned {
                        let id = self.c_ident(v);
                        if self.is_num(v) {
                            self.emit(&format!("{id} = _sv_{id};"));
                        } else {
                            self.emit(&format!("{id} = _sv_{id};"));
                        }
                    }
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Case { discriminant, clauses } => {
                let d = self.value_c(discriminant);
                self.need_fnmatch = true;
                let mut first = true;
                for cl in clauses {
                    for pat in &cl.patterns {
                        let kw = if first { "if" } else { "else if" };
                        first = false;
                        let flags = if self.nocasematch { ", FNM_CASEFOLD" } else { "" };
                        let pat_c = Self::cstr(pat);
                        self.emit(&format!(
                            "{kw} (fnmatch({pat_c}, {d}, 0{flags}) == 0) {{"
                        ));
                        self.depth += 1;
                        for s in &cl.body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.emit("}");
                    }
                }
                self.emit("else {");
                self.depth += 1;
                self.emit("/* no default */");
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::WriteFile { path, content, append } => {
                let p = self.value_c(path);
                let c = self.value_c(content);
                let mode = if *append { "a" } else { "w" };
                self.emit(&format!(
                    "{{ FILE *_f = fopen({p}, \"{mode}\"); if (_f) {{ fputs({c}, _f); fclose(_f); }} }}"
                ));
            }
            IrStmt::Die { expr, .. } => {
                let v = self.value_c(expr);
                self.emit(&format!("fprintf(stderr, \"%s\\n\", (char*)({v}));"));
                self.emit("exit(1);");
            }
            IrStmt::Warn { expr, .. } => {
                let v = self.value_c(expr);
                self.emit(&format!("fprintf(stderr, \"%s\\n\", (char*)({v}));"));
            }
            IrStmt::SetChildError(e) => {
                self.need_sh = true;
                let v = self.expr(e);
                self.emit(&format!("_sh_rc = ({v});"));
            }
            IrStmt::Return(e) => {
                let v = e
                    .as_ref()
                    .map(|x| self.value_num(x))
                    .unwrap_or_else(|| "0".into());
                if self.in_function {
                    self.need_sh = true;
                    self.emit(&format!("_sh_rc = {v};"));
                    self.emit("return;");
                } else {
                    self.emit(&format!("return {v};"));
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                // the array store lands in wave 2 — render a stub-free
                // char* store entry so the file still compiles
                self.store.insert(var.clone());
                let _ = elements;
                self.emit(&format!("/* array {var} */"));
            }
            IrStmt::Exec { cmd, args, capture, redirects, .. } => {
                let mut call_args = vec![cmd.clone()];
                call_args.push(IrExpr::Array(args.clone()));
                if let Some(var) = capture {
                    self.store.insert(var.clone());
                    let id = self.c_ident(var);
                    let cap = self.capture_call(&call_args);
                    self.emit(&format!("{id} = {cap};"));
                } else if !redirects.is_empty() {
                    let stmts = vec![IrStmt::Expr(IrExpr::Call {
                        func: "exec".to_string(),
                        args: call_args,
                    })];
                    let site = self.shell_site(
                        |r| {
                            r.emit("_sh_reset();");
                            r.sh_stage(CmdBuf::Shared, &stmts);
                            let rds: Vec<crate::ir::IrRedirect> = redirects
                                .iter()
                                .filter_map(|e| match e {
                                    IrExpr::Object(fields) => {
                                        let mut fd = 1;
                                        let mut mode = String::new();
                                        let mut target = IrExpr::Str(
                                            String::new(),
                                            crate::ir::StrStyle::DoubleQuoted,
                                        );
                                        for (k, v) in fields {
                                            match k.as_str() {
                                                "fd" => {
                                                    if let IrExpr::Int(n) = v {
                                                        fd = *n;
                                                    }
                                                }
                                                "mode" => {
                                                    mode = Self::str_arg(&[v.clone()], 0)
                                                        .unwrap_or_default();
                                                }
                                                "target" => target = v.clone(),
                                                _ => {}
                                            }
                                        }
                                        Some(crate::ir::IrRedirect {
                                            fd: Some(fd as i32),
                                            mode,
                                            target,
                                            interpolate: true,
                                        })
                                    }
                                    _ => None,
                                })
                                .collect();
                            r.sh_redirect_text(CmdBuf::Shared, &rds);
                        },
                        false,
                    );
                    self.emit(&format!("{site};"));
                } else {
                    let site = self.shell_site(
                        |r| {
                            r.emit("_sh_reset();");
                            if let Some(c) = Self::str_arg(&call_args, 0) {
                                r.sh_word(
                                    CmdBuf::Shared,
                                    &IrExpr::Str(c, crate::ir::StrStyle::DoubleQuoted),
                                );
                            }
                            if let Some(IrExpr::Array(items)) = call_args.get(1) {
                                for w in items {
                                    r.sh_word(CmdBuf::Shared, w);
                                }
                            }
                        },
                        false,
                    );
                    self.emit(&format!("{site};"));
                }
            }
            other => self.mark_todo(&format!("stmt {:?}", other)),
        }
    }

    /// The effective C width of an Int-typed var (from the range
    /// analysis; missing = no proof → long long).
    fn width_of_var(&self, name: &str) -> Width {
        self.var_widths.get(name).copied().unwrap_or(Width::I64)
    }

    /// The width of a numeric expression: a typed var's width, or the
    /// range-derived width of an arith result (None range → i64). Used to
    /// size stringification temps exactly.
    fn expr_width(&self, e: &IrExpr) -> Width {
        match e {
            IrExpr::Var(name, _) | IrExpr::Ident(name) => self.width_of_var(name),
            IrExpr::Arith(a) => {
                let state: HashMap<String, Option<(i128, i128)>> = self
                    .var_ranges
                    .iter()
                    .map(|(k, v)| (k.clone(), Some(*v)))
                    .collect();
                match arith_range_local(a, &state) {
                    Some((lo, hi)) => {
                        Width::from_range_name(crate::shir::range_width_name(lo, hi))
                    }
                    None => Width::I64,
                }
            }
            // `$y` read of a typed var renders as the declared ident —
            // its width is the var's declared width, not the I64 fallback
            // (without this, `echo $i` would keep the %lld cast)
            IrExpr::Call { func, args } if func == "getVar" => {
                match args.first() {
                    Some(IrExpr::Str(name, _)) if self.var_types.contains_key(name) => {
                        self.width_of_var(name)
                    }
                    _ => Width::I64,
                }
            }
            _ => Width::I64,
        }
    }

    /// The printf spec for a numeric operand: format by the PROVEN width,
    /// cast only when the operand's C type can't be proven to match.
    ///
    /// Invariant: cast == true ⟺ spec == "%lld". When the type is known
    /// (a var read / arith over vars at that width), the spec matches the
    /// actual C type — `%u` on an `unsigned int`, `%d` on an `int`,
    /// `%lld` on a `long long` — and no cast is emitted. When it is not
    /// (int literals, stub calls, unproven arith), the `(long long)` cast
    /// pins the vararg type to match `%lld` — the pair is always
    /// consistent, so a casted operand never meets a `%u`/`%d`.
    fn num_spec(&self, e: &IrExpr) -> NumSpec {
        let w = self.expr_width(e);
        if self.expr_type_matches(e, w) {
            NumSpec::Num(w.format(), false)
        } else {
            NumSpec::Num("%lld", true)
        }
    }

    /// Can the rendered C expression of `e` be proven to have exactly the
    /// C type of width `w` — so the width's printf format matches without
    /// a cast? True for a read of a variable declared at `w` (Var/Ident,
    /// or getVar of a typed var, which renders as the declared ident), and
    /// for arithmetic whose variable leaves are all at `w` (C's usual
    /// arithmetic conversions keep the result at the leaf type; int
    /// literals convert up). Everything else — int literals, stubs,
    /// BinOp — is conservative (cast kept).
    fn expr_type_matches(&self, e: &IrExpr, w: Width) -> bool {
        match e {
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                // the var must be genuinely numeric (a string var's width
                // defaults to I64 but its C type is `char*` — never match)
                self.is_num(name) && self.width_of_var(name) == w
            }
            IrExpr::Arith(a) => {
                let mut has_var = false;
                arith_leaves_at_width(a, self, w, &mut has_var)
                    // a pure-Num arith renders as `int` — only matches I32
                    && (has_var || w == Width::I32)
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                // `$y` read of a typed var renders as the declared ident
                matches!(
                    args.first(),
                    Some(IrExpr::Str(name, _))
                        if self.is_num(name) && self.width_of_var(name) == w
                )
            }
            _ => false,
        }
    }

    /// Render an expression as a C integer (Int-typed assignment target).
    fn expr_as_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => {
                // numeric literal in the ShIR ("5" for x=5)
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else {
                    self.mark_todo(&format!("string→int coercion of {s:?}"));
                    "0".into()
                }
            }
            IrExpr::Int(i) => i.to_string(),
            _ => self.expr(e),
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect declared vars (assign targets, declare lists,
        // Var reads) so declarations can be hoisted before use. Also
        // collect for-loop variables so we can exclude them from the
        // top-level pre-declaration (they are declared inside the loop),
        // and the shell function names (their calls render `name();`).
        let mut vars: BTreeSet<String> = BTreeSet::new();
        let mut for_vars: BTreeSet<String> = BTreeSet::new();
        collect_vars_full(&prog.stmts, &mut vars, &mut for_vars);
        // collect function definitions at ANY depth (a function may be
        // defined inside a block/loop — the shellbench eval benches do).
        collect_fn_defs(&prog.stmts, &mut self.functions, &mut self.fn_defs);
        // vars that appear ONLY inside function bodies (var_types covers
        // them too, but they must NOT be hoisted into main — the
        // function declares its own copy).
        let mut fn_only: BTreeSet<String> = BTreeSet::new();
        for (_, body) in &self.fn_defs {
            let mut fv = BTreeSet::new();
            collect_vars(body, &mut fv);
            for v in &fv {
                if !vars.contains(v) {
                    fn_only.insert(v.clone());
                }
            }
        }
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }
        // seq-range for-loop vars: the loop renders them as Int — hoist
        // them at that type (bash leaves $i = LAST after the loop)
        for s in &prog.stmts {
            mark_seq_loop_vars(s, &mut self.var_types);
        }
        // collect the untyped store names (getVar/param reads, Assign
        // targets, Declare lists) — they hoist as `char*` entries
        collect_store_names(&prog.stmts, &mut self.store);
        for v in &self.store {
            vars.insert(v.clone());
        }
        for v in &fn_only {
            vars.remove(v);
        }
        let _ = &for_vars;

        // Pass 2: render the body first (helper flags known before
        // preamble). The var DECLARATIONS go into a separate preamble
        // buffer as FILE-SCOPE statics — the _sh_site_N/_cap_N helper
        // functions (emitted before main) must see every program var.
        let mut decl_out = Vec::new();
        std::mem::swap(&mut self.out, &mut decl_out);
        self.depth = 0;
        for v in &vars {
            self.emit_var_decl(v);
        }
        std::mem::swap(&mut self.out, &mut decl_out);
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        if !vars.is_empty() {
            // DEBUG-ONLY length invariants at the function boundary
            // (assert() compiles out under NDEBUG) — STATEMENTS, so they
            // live in main, not the file-scope decl block
            self.emit_bound_asserts(&vars);
            self.emit("");
        }
        for s in &prog.stmts {
            self.stmt(s);
        }
        self.emit("return 0;");
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        // Preamble: includes, runtime helpers, the global var decls,
        // then the site/capture helpers (definition-before-use: main +
        // functions call them), the sh2.* stubs (should be none), the
        // shell functions, main.
        self.emit_runtime();
        self.out.extend(decl_out.iter().cloned());
        self.emit("");
        // site/capture helper bodies (registered in emission order:
        // captures register before the sites that use them)
        let cap_bodies = std::mem::take(&mut self.cap_bodies);
        for b in &cap_bodies {
            for line in b.lines() {
                self.emit(line);
            }
            self.emit("");
        }
        let site_bodies = std::mem::take(&mut self.site_bodies);
        for b in &site_bodies {
            for line in b.lines() {
                self.emit(line);
            }
            self.emit("");
        }
        // shell functions rendered FIRST (into a side buffer) so the
        // sh2.* stub set is complete before the stubs are emitted —
        // definition-before-use: a function body calling a stub must
        // see its definition (an implicit declaration then the real
        // definition is a conflicting-types error).
        let fn_defs = std::mem::take(&mut self.fn_defs);
        let mut fn_out = Vec::new();
        let saved_out = std::mem::replace(&mut self.out, Vec::new());
        for (name, body) in &fn_defs {
            self.emit_function(name, body, &vars);
        }
        fn_out = std::mem::replace(&mut self.out, saved_out);
        self.fn_defs = fn_defs;
        if !self.sh2_calls.is_empty() {
            self.emit("/* sh2.* runtime stubs — TODO: implement (harness/sh2-namespace.json) */");
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                self.emit(&format!("static long long sh2_{name}(void) {{"));
                self.emit(&format!("  fprintf(stderr, \"TODO sh2.{name}\\n\");"));
                self.emit("  exit(2);");
                self.emit("  return 0;");
                self.emit("}");
            }
            self.emit("");
        }
        self.out.extend(fn_out.iter().cloned());
        if !fn_out.is_empty() {
            self.emit("");
        }
        self.emit("int main(void) {");
        if self.need_sh || !self.sh2_calls.is_empty() {
            self.emit("  freopen(\"/dev/null\", \"w\", stderr);");
            // unbuffered stdout: bash -c children share fd 1 — buffered
            // stdio would reorder their output after ours at flush time
            self.emit("  setvbuf(stdout, 0, _IONBF, 0);");
        }
        self.out.extend(body_out.iter().cloned());
        self.emit("}");
        if self.todo > 0 {
            self.emit(&format!("/* {} construct(s) lowered to TODO markers */", self.todo));
        }
    }
}

/// Collect every variable name referenced by statements (assign targets,
/// declare lists, Var reads).
fn collect_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    collect_vars_full(stmts, out, &mut BTreeSet::new());
}

/// Collect Function definitions at any depth (names + bodies).
fn collect_fn_defs(
    stmts: &[IrStmt],
    names: &mut BTreeSet<String>,
    defs: &mut Vec<(String, Vec<IrStmt>)>,
) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body } => {
                names.insert(name.clone());
                defs.push((name.clone(), body.clone()));
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_fn_defs(then, names, defs);
                for (_, b) in elsifs {
                    collect_fn_defs(b, names, defs);
                }
                collect_fn_defs(else_, names, defs);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_fn_defs(body, names, defs),
            _ => {}
        }
    }
}

/// For every `Const`-verdict var: the single TOP-LEVEL `Assign` targeting
/// it (straight-line, no indices). The const markup alone allows
/// conditional single sites; the C backend lifts only the unconditional
/// top-level ones (a hoisted initializer must always run). The verdict
/// guarantees at most one site, so the first match is the only one.
fn const_assign_rhs(stmts: &[IrStmt], const_vars: &HashMap<String, VarKind>) -> HashMap<String, IrExpr> {
    let mut out = HashMap::new();
    for s in stmts {
        if let IrStmt::Assign { targets, expr } = s {
            for t in targets {
                if t.indices.is_empty()
                    && const_vars.get(&t.var) == Some(&VarKind::Const)
                    && !out.contains_key(&t.var)
                {
                    out.insert(t.var.clone(), expr.clone());
                }
            }
        }
    }
    out
}

/// Seq-range for-loop vars render as Int loops — hoist them at that
/// type (the range analysis seeds their [lo, hi]).
fn mark_seq_loop_vars(s: &IrStmt, var_types: &mut HashMap<String, IrType>) {
    match s {
        IrStmt::For { var, iter, body } => {
            if seq_iter_range(iter).is_some() {
                var_types.insert(var.clone(), IrType::Int);
            }
            for b in body {
                mark_seq_loop_vars(b, var_types);
            }
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            for x in b {
                mark_seq_loop_vars(x, var_types);
            }
        }
        IrStmt::If { then, elsifs, else_, .. } => {
            for x in then {
                mark_seq_loop_vars(x, var_types);
            }
            for (_, b) in elsifs {
                for x in b {
                    mark_seq_loop_vars(x, var_types);
                }
            }
            for x in else_ {
                mark_seq_loop_vars(x, var_types);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            for x in body {
                mark_seq_loop_vars(x, var_types);
            }
        }
        IrStmt::Function { body, .. } => {
            for x in body {
                mark_seq_loop_vars(x, var_types);
            }
        }
        _ => {}
    }
}

/// Names declared by Declare stmts (the per-function hoist skips them —
/// the Declare stmt declares them at its position).
fn collect_declare_names(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Declare { vars, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_declare_names(then, out);
                for (_, b) in elsifs {
                    collect_declare_names(b, out);
                }
                collect_declare_names(else_, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_declare_names(body, out),
            IrStmt::Function { body, .. } => collect_declare_names(body, out),
            _ => {}
        }
    }
}

/// Collect the untyped store names (getVar/param reads, assign targets,
/// read/declare/unset builtin targets) so they hoist as `char*` entries.
fn collect_store_names(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
                collect_store_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_store_expr(e, out);
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                out.insert(var.clone());
            }
            IrStmt::Expr(e) => collect_store_expr(e, out),
            IrStmt::Output { value, .. } => collect_store_expr(value, out),
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_store_expr(cond, out);
                collect_store_names(then, out);
                for (c, b) in elsifs {
                    collect_store_expr(c, out);
                    collect_store_names(b, out);
                }
                collect_store_names(else_, out);
            }
            IrStmt::Exit(e) | IrStmt::Return(e) => {
                if let Some(x) = e {
                    collect_store_expr(x, out);
                }
            }
            IrStmt::For { iter, body, .. } => {
                collect_store_expr(iter, out);
                collect_store_names(body, out);
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                collect_store_expr(cond, out);
                collect_store_names(body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_store_names(b, out);
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_store_names(inner, out);
                for r in redirects {
                    collect_store_expr(&r.target, out);
                }
            }
            IrStmt::Function { body, .. } => collect_store_names(body, out),
            IrStmt::Case { discriminant, clauses } => {
                collect_store_expr(discriminant, out);
                for c in clauses {
                    collect_store_names(&c.body, out);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_store_names(st, out);
                }
            }
            IrStmt::WriteFile { path, content, .. } => {
                collect_store_expr(path, out);
                collect_store_expr(content, out);
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                collect_store_expr(expr, out);
            }
            IrStmt::SetChildError(e) => collect_store_expr(e, out),
            IrStmt::Exec { cmd, args, capture, .. } => {
                collect_store_expr(cmd, out);
                for a in args {
                    collect_store_expr(a, out);
                }
                if let Some(v) = capture {
                    out.insert(v.clone());
                }
            }
            _ => {}
        }
    }
}

fn collect_store_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Call { func, args } => {
            match func.as_str() {
                "setVar" | "assign" => {
                    if let Some(IrExpr::Str(n, _)) = args.first() {
                        out.insert(n.clone());
                    }
                }
                "exec" => {
                    if let Some(IrExpr::Str(cmd, _)) = args.first() {
                        if cmd == "read" {
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                for w in items {
                                    if let IrExpr::Str(n, _) = w {
                                        if !n.starts_with('-') {
                                            out.insert(n.clone());
                                        }
                                    }
                                }
                            }
                        }
                        if matches!(
                            cmd.as_str(),
                            "export" | "local" | "declare" | "typeset" | "readonly"
                        ) {
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                for w in items {
                                    if let IrExpr::Str(w, _) = w {
                                        if let Some((n, _)) = w.split_once('=') {
                                            out.insert(n.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        if cmd == "unset" {
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                for w in items {
                                    if let IrExpr::Str(n, _) = w {
                                        out.insert(n.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            for a in args {
                collect_store_expr(a, out);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_store_expr(lhs, out);
            collect_store_expr(rhs, out);
        }
        IrExpr::Arith(a) => collect_store_arith(a, out),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_store_expr(x, out);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_store_expr(i, out);
            }
        }
        IrExpr::Arrow(body) => collect_store_names(body, out),
        IrExpr::Index { var, key, .. } => {
            out.insert(var.clone());
            collect_store_expr(key, out);
        }
        IrExpr::Capture { expr, .. } => collect_store_expr(expr, out),
        IrExpr::Ternary { cond, then, else_ } => {
            collect_store_expr(cond, out);
            collect_store_expr(then, out);
            collect_store_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_store_expr(expr, out);
            collect_store_expr(default, out);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_store_expr(obj, out);
            for a in args {
                collect_store_expr(a, out);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                collect_store_expr(v, out);
            }
        }
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            out.insert(name.clone());
        }
        _ => {}
    }
}

fn collect_store_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_store_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_store_arith(lhs, out);
            collect_store_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_store_arith(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            collect_store_arith(test, out);
            collect_store_arith(then, out);
            collect_store_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            collect_store_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Num(_) => {}
    }
}

/// Like `collect_vars`, but also returns the set of for-loop variables
/// (which are declared inside the loop, not at function top).
fn collect_vars_full(
    stmts: &[IrStmt],
    out: &mut BTreeSet<String>,
    for_vars: &mut BTreeSet<String>,
) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
                collect_vars_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_vars_expr(e, out);
                }
            }
            IrStmt::Expr(e) => collect_vars_expr(e, out),
            IrStmt::Output { value, .. } => collect_vars_expr(value, out),
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_vars_expr(cond, out);
                collect_vars(then, out);
                for (c, b) in elsifs {
                    collect_vars_expr(c, out);
                    collect_vars(b, out);
                }
                collect_vars(else_, out);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::For { var, iter, body } => {
                // The for-loop variable is declared inside the loop; don't
                // pre-declare it at function top.
                for_vars.insert(var.clone());
                collect_vars_expr(iter, out);
                collect_vars_full(body, out, for_vars);
            }
            // loop bodies assign/read vars — hoist them (they are
            // ordinary top-level vars, unlike for-loop counters).
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_vars(body, out)
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => collect_vars(b, out),
            _ => {}
        }
    }
}

/// Collect vars ASSIGNED in a statement list (Assign/Declare targets,
/// arith x=/x++/x--), not mere reads — the per-function hoist declares
/// exactly these (a read-only var is the caller's).
fn collect_assigned_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
                collect_assigned_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_assigned_expr(e, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_assigned_vars(then, out);
                for (_, b) in elsifs {
                    collect_assigned_vars(b, out);
                }
                collect_assigned_vars(else_, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_assigned_vars(body, out),
            IrStmt::Expr(e) => collect_assigned_expr(e, out),
            _ => {}
        }
    }
}

fn collect_assigned_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Arith(a) => collect_assigned_arith(a, out),
        _ => {}
    }
}

fn collect_assigned_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
}

fn collect_vars_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_vars_expr(lhs, out);
            collect_vars_expr(rhs, out);
        }
        IrExpr::Arith(a) => collect_vars_arith(a, out),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_vars_expr(x, out);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_vars_expr(i, out);
            }
        }
        IrExpr::Call { func, args } if func == "exec" => {
            // `let "i++"` hides its var inside a STRING arg — the hoist
            // must see it or the loop var is undeclared in C.
            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                if cmd == "let" {
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        if let Some(IrExpr::Str(expr, _)) = items.first() {
                            if let Some(n) = let_var_name(expr) {
                                out.insert(n);
                            }
                        }
                    }
                }
            }
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        _ => {}
    }
}

/// The variable a `let` string operates on ("i++", "++i", "x+=1").
fn let_var_name(s: &str) -> Option<String> {
    let s = s.trim();
    let s = s
        .strip_prefix("++")
        .or_else(|| s.strip_prefix("--"))
        .unwrap_or(s)
        .trim();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end > 0 {
        Some(s[..end].to_string())
    } else {
        None
    }
}

fn collect_vars_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_vars_arith(lhs, out);
            collect_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_vars_arith(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            collect_vars_arith(test, out);
            collect_vars_arith(then, out);
            collect_vars_arith(else_, out);
        }
        _ => {}
    }
}

/// A plain C identifier (a mangled var name or a string-literal-less
/// expression is NOT — used to decide whether an RHS is a string value
/// that may be length-asserted before a guarded copy).
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && (s.chars().next().unwrap().is_ascii_alphabetic() || s.chars().next().unwrap() == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ── numeric-range wiring (core's analyze_var_ranges / range_width_name) ──

/// Byte size (chars + '\0') of a decimal string for the given width:
///   u32 `"4294967295"`        → 10 + 1 = 11
///   i32 `"-2147483648"`       → 11 + 1 = 12
///   i64 `"-9223372036854775808"` → 20 + 1 = 21 (u64's 20 digits too,
///      so 21 is the universal 64-bit bound)
/// All variable leaves of an arith tree must be declared at width `w`
/// (the rendered C expression then has `w`'s C type: `unsigned int ×
/// unsigned int → unsigned int`, `int × int → int`, `long long × long
/// long → long long`, and an `int` literal operand converts up). `has_var`
/// records whether any Var/Assign/IncDec leaf was seen (a pure-Num tree
/// renders as `int`, which only matches I32). Index leaves are stubbed to
/// `0` (`int`) — unprovable, returns false.
fn arith_leaves_at_width(a: &ArithAst, r: &Render, w: Width, has_var: &mut bool) -> bool {
    match a {
        ArithAst::Num(_) => true,
        ArithAst::Var(name) => {
            *has_var = true;
            // genuinely numeric (a string var's width default I64 must not
            // match; its rendered type is `char*`)
            r.is_num(name) && r.width_of_var(name) == w
        }
        ArithAst::Index { .. } => false,
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_leaves_at_width(lhs, r, w, has_var)
                && arith_leaves_at_width(rhs, r, w, has_var)
        }
        ArithAst::Un { arg, .. } => arith_leaves_at_width(arg, r, w, has_var),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            arith_leaves_at_width(test, r, w, has_var)
                && arith_leaves_at_width(then, r, w, has_var)
                && arith_leaves_at_width(else_, r, w, has_var)
        }
        ArithAst::Assign { var, rhs, .. } => {
            *has_var = true;
            r.is_num(var) && r.width_of_var(var) == w
                && arith_leaves_at_width(rhs, r, w, has_var)
        }
        ArithAst::IncDec { var, .. } => {
            *has_var = true;
            r.is_num(var) && r.width_of_var(var) == w
        }
    }
}

fn width_buf_len(w: Width) -> usize {
    match w {
        Width::U32 => 11,
        Width::I32 => 12,
        Width::I64 => 21,
    }
}

/// Detect a `Range` iterable and the shell `for x in $(seq a b)` shape
/// (core-lowered `Array([Range])` or pre-lift captureWords → arrow →
/// exec "seq"); returns (first, last, step). Anything else → None.
fn seq_iter_range(iter: &IrExpr) -> Option<(i128, i128, i128)> {
    match iter {
        IrExpr::Range { start, end } => Some((*start as i128, *end as i128, 1i128)),
        IrExpr::Array(items) if items.len() == 1 => match items.first() {
            Some(IrExpr::Range { start, end }) => Some((*start as i128, *end as i128, 1i128)),
            Some(cap) => seq_capture_words(cap),
            None => None,
        },
        _ => None,
    }
}

/// Parse the pre-lift `captureWords → arrow → exec "seq"` iterable
/// (seq [FIRST [INCREMENT]] LAST); None → not a numeric seq.
fn seq_capture_words(cap: &IrExpr) -> Option<(i128, i128, i128)> {
    let IrExpr::Call { func, args } = cap else {
        return None;
    };
    if func != "captureWords" {
        return None;
    }
    let arrow = args.first()?;
    let IrExpr::Arrow(body) = arrow else {
        return None;
    };
    if body.len() != 1 {
        return None;
    }
    let stmt = body.first()?;
    let exec_call = match stmt {
        IrStmt::Expr(e) => e,
        _ => return None,
    };
    let IrExpr::Call { func, args } = exec_call else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "seq" {
        return None;
    }
    let IrExpr::Array(seqargs) = args.get(1)? else {
        return None;
    };
    if seqargs.is_empty() || seqargs.len() > 3 {
        return None;
    }
    let num = |e: &IrExpr| -> Option<i128> {
        match e {
            IrExpr::Str(s, _) => s.trim().parse::<i128>().ok(),
            IrExpr::Int(n) => Some(*n as i128),
            _ => None,
        }
    };
    let last = num(seqargs.last()?)?;
    let (first, step) = match seqargs.len() {
        1 => (1, 1),
        2 => (num(&seqargs[0])?, 1),
        _ => (num(&seqargs[0])?, num(&seqargs[1])?),
    };
    if step == 0 {
        return None;
    }
    Some((first, last, step))
}

/// Seed loop-var ranges from Range/seq For iters — `analyze_var_ranges`
/// doesn't track for-loop bindings (its For arm marks body-assigned vars
/// unbounded). Nested loops and branches are walked; an existing range
/// joins (widens) with the seed.
fn seed_loop_var_ranges(stmts: &[IrStmt], ranges: &mut HashMap<String, (i128, i128)>) {
    for s in stmts {
        match s {
            IrStmt::For { var, iter, body } => {
                if let Some((first, last, _)) = seq_iter_range(iter) {
                    let (lo, hi) = (first.min(last), first.max(last));
                    match ranges.get(var) {
                        Some((l0, h0)) => {
                            ranges.insert(var.clone(), ((*l0).min(lo), (*h0).max(hi)));
                        }
                        None => {
                            ranges.insert(var.clone(), (lo, hi));
                        }
                    }
                }
                seed_loop_var_ranges(body, ranges);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                seed_loop_var_ranges(b, ranges);
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                seed_loop_var_ranges(then, ranges);
                for (_, b) in elsifs {
                    seed_loop_var_ranges(b, ranges);
                }
                seed_loop_var_ranges(else_, ranges);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                seed_loop_var_ranges(body, ranges);
            }
            IrStmt::Redirect { inner, .. } => seed_loop_var_ranges(inner, ranges),
            _ => {}
        }
    }
}

/// Effective C width per Int-typed var: the widest of the var's own
/// [lo, hi] (range_width_name) and every arith-expr result range that
/// mentions it. Sound: a var's width must cover the RESULTS of the
/// arithmetic computed on it, not just its own values — `i` in
/// [1, 70000] is u32, but `(i * i)` needs i64. An arith expr whose range
/// is unknown (None) forces i64 — no proof, no narrowing.
fn effective_widths(
    prog: &IrProgram,
    ranges: &HashMap<String, (i128, i128)>,
) -> HashMap<String, Width> {
    let state: HashMap<String, Option<(i128, i128)>> =
        ranges.iter().map(|(k, v)| (k.clone(), Some(*v))).collect();
    let mut widths: HashMap<String, Width> = HashMap::new();
    for (name, (lo, hi)) in ranges {
        widths.insert(
            name.clone(),
            Width::from_range_name(crate::shir::range_width_name(*lo, *hi)),
        );
    }
    walk_widths_stmts(&prog.stmts, &state, &mut widths);
    widths
}

fn walk_widths_stmts(
    stmts: &[IrStmt],
    state: &HashMap<String, Option<(i128, i128)>>,
    widths: &mut HashMap<String, Width>,
) {
    for s in stmts {
        match s {
            IrStmt::Assign { expr, .. } => walk_widths_expr(expr, state, widths),
            IrStmt::Declare { init, .. } => {
                if let Some(e) = init {
                    walk_widths_expr(e, state, widths);
                }
            }
            IrStmt::DeclareArray { elements, .. } => {
                for e in elements {
                    walk_widths_expr(e, state, widths);
                }
            }
            IrStmt::Output { value, .. } => walk_widths_expr(value, state, widths),
            IrStmt::WriteFile { path, content, .. } => {
                walk_widths_expr(path, state, widths);
                walk_widths_expr(content, state, widths);
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                walk_widths_expr(cond, state, widths);
                walk_widths_stmts(then, state, widths);
                for (c, b) in elsifs {
                    walk_widths_expr(c, state, widths);
                    walk_widths_stmts(b, state, widths);
                }
                walk_widths_stmts(else_, state, widths);
            }
            IrStmt::For { iter, body, .. } => {
                walk_widths_expr(iter, state, widths);
                walk_widths_stmts(body, state, widths);
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_widths_expr(cond, state, widths);
                walk_widths_stmts(body, state, widths);
            }
            IrStmt::Exit(e) | IrStmt::Return(e) => {
                if let Some(x) = e {
                    walk_widths_expr(x, state, widths);
                }
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_widths_expr(expr, state, widths);
            }
            IrStmt::SetChildError(e) => walk_widths_expr(e, state, widths),
            IrStmt::Expr(e) => walk_widths_expr(e, state, widths),
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                walk_widths_stmts(b, state, widths);
            }
            IrStmt::Redirect { inner, redirects } => {
                walk_widths_stmts(inner, state, widths);
                for r in redirects {
                    walk_widths_expr(&r.target, state, widths);
                }
            }
            IrStmt::Function { body, .. } => walk_widths_stmts(body, state, widths),
            IrStmt::Case { discriminant, clauses } => {
                walk_widths_expr(discriminant, state, widths);
                for c in clauses {
                    walk_widths_stmts(&c.body, state, widths);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    walk_widths_stmts(st, state, widths);
                }
            }
            _ => {}
        }
    }
}

fn walk_widths_expr(
    e: &IrExpr,
    state: &HashMap<String, Option<(i128, i128)>>,
    widths: &mut HashMap<String, Width>,
) {
    match e {
        IrExpr::Arith(a) => {
            let rng = arith_range_local(a, state);
            let mut vs = Vec::new();
            arith_vars(a, &mut vs);
            for v in vs {
                match rng {
                    Some((lo, hi)) => {
                        let w = Width::from_range_name(crate::shir::range_width_name(lo, hi));
                        let cur = widths.get(&v).copied().unwrap_or(Width::I64);
                        widths.insert(v, cur.widen(w));
                    }
                    None => {
                        // no proof the expr stays in width → no narrowing
                        widths.insert(v, Width::I64);
                    }
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_widths_expr(lhs, state, widths);
            walk_widths_expr(rhs, state, widths);
        }
        IrExpr::Index { key, .. } => walk_widths_expr(key, state, widths),
        IrExpr::Call { args, .. } => {
            for a in args {
                walk_widths_expr(a, state, widths);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            walk_widths_expr(obj, state, widths);
            for a in args {
                walk_widths_expr(a, state, widths);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_widths_expr(cond, state, widths);
            walk_widths_expr(then, state, widths);
            walk_widths_expr(else_, state, widths);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_widths_expr(expr, state, widths);
            walk_widths_expr(default, state, widths);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    walk_widths_expr(x, state, widths);
                }
            }
        }
        IrExpr::Capture { expr, .. } => walk_widths_expr(expr, state, widths),
        IrExpr::Array(items) => {
            for i in items {
                walk_widths_expr(i, state, widths);
            }
        }
        IrExpr::Arrow(body) => walk_widths_stmts(body, state, widths),
        IrExpr::Object(props) => {
            for (_, v) in props {
                walk_widths_expr(v, state, widths);
            }
        }
        _ => {}
    }
}

/// Local copy of the core's (private) `arith_range`: the conservative
/// [lo, hi] of an ArithAst over the per-var ranges. The renderer needs
/// it to prove an arith expr's RESULT fits its operands' widths before
/// narrowing; the core keeps it private (single-owner), so the copy
/// lives renderer-side. Mirrors shir.rs arith_range exactly.
fn arith_range_local(
    a: &ArithAst,
    state: &HashMap<String, Option<(i128, i128)>>,
) -> Option<(i128, i128)> {
    match a {
        ArithAst::Num(i) => Some((*i as i128, *i as i128)),
        ArithAst::Var(n) => state.get(n).copied().flatten(),
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (arith_range_local(lhs, state)?, arith_range_local(rhs, state)?);
            let (l0, l1, r0, r1) = (l.0, l.1, r.0, r.1);
            match op.as_str() {
                "+" => Some((l0.checked_add(r0)?, l1.checked_add(r1)?)),
                "-" => Some((l0.checked_sub(r1)?, l1.checked_sub(r0)?)),
                "*" => {
                    let ps = [
                        l0.checked_mul(r0)?,
                        l0.checked_mul(r1)?,
                        l1.checked_mul(r0)?,
                        l1.checked_mul(r1)?,
                    ];
                    Some((*ps.iter().min()?, *ps.iter().max()?))
                }
                "/" => {
                    if r0 <= 0 && r1 >= 0 {
                        return None; // possible division by zero
                    }
                    let qs = [
                        l0.checked_div(r0)?,
                        l0.checked_div(r1)?,
                        l1.checked_div(r0)?,
                        l1.checked_div(r1)?,
                    ];
                    Some((*qs.iter().min()?, *qs.iter().max()?))
                }
                _ => None, // %, ^, comparisons, ... conservative
            }
        }
        ArithAst::Un { op, arg } => {
            let (lo, hi) = arith_range_local(arg, state)?;
            match op.as_str() {
                "-" => Some((-hi, -lo)),
                "+" => Some((lo, hi)),
                _ => None,
            }
        }
        _ => None, // Index / Cond / Assign / IncDec
    }
}

/// Every variable name an ArithAst mentions (reads; a bare `var =` write
/// target is excluded — its RHS vars are included).
fn arith_vars(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Var(n) => out.push(n.clone()),
        ArithAst::Index { var, key } => {
            out.push(var.clone());
            arith_vars(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_vars(lhs, out);
            arith_vars(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_vars(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_vars(test, out);
            arith_vars(then, out);
            arith_vars(else_, out);
        }
        ArithAst::Assign { rhs, .. } => arith_vars(rhs, out),
        ArithAst::IncDec { var, .. } => out.push(var.clone()),
        ArithAst::Num(_) => {}
    }
}

/// Render an ArithAst as shell arithmetic text (`$(( ... ))` body) for
/// reconstructed shell commands.
fn arith_shell(a: &ArithAst) -> String {
    match a {
        ArithAst::Num(n) => n.to_string(),
        ArithAst::Var(name) => format!("${{{name}}}"),
        ArithAst::Index { var, key } => format!("${{{var}[{}]}}", arith_shell(key)),
        ArithAst::Bin { op, lhs, rhs } => format!("({} {} {})", arith_shell(lhs), op, arith_shell(rhs)),
        ArithAst::Un { op, arg } => format!("({op}{})", arith_shell(arg)),
        ArithAst::Cond { test, then, else_ } => format!(
            "({} ? {} : {})",
            arith_shell(test),
            arith_shell(then),
            arith_shell(else_)
        ),
        ArithAst::Assign { var, op, rhs } => format!("{var} {op} {}", arith_shell(rhs)),
        ArithAst::IncDec { var, delta, prefix } => {
            let d = if *delta >= 0 { "+1" } else { "-1" };
            let u = if *delta >= 0 { "++" } else { "--" };
            if *prefix {
                format!("{u}{var}")
            } else {
                format!("{var}{u}{d}")
            }
        }
    }
}

/// Flatten nested Interpolates into a single part list (bash
/// concatenation is flat).
fn flatten_parts(parts: &[InterpPart]) -> Vec<InterpPart> {
    let mut out = Vec::new();
    for p in parts {
        match p {
            InterpPart::Lit(s) => out.push(InterpPart::Lit(s.clone())),
            InterpPart::Expr(x) => match x.as_ref() {
                IrExpr::Interpolate(inner) => out.extend(flatten_parts(inner)),
                _ => out.push(InterpPart::Expr(x.clone())),
            },
        }
    }
    out
}

/// The Json brace-parts argument (`brace("pre", [[...]], [...], "suf")`).
fn brace_json_arg(args: &[IrExpr]) -> Option<&serde_json::Value> {
    for a in args {
        if let IrExpr::Json(v) = a {
            return Some(v);
        }
    }
    None
}

/// Expand a `{..}` group entry (a range ["a","b",step,null] or a list of
/// alternatives) into its strings.
fn brace_group_items(entry: &serde_json::Value) -> Vec<String> {
    if let Some(r) = entry.get("range").and_then(|r| r.as_array()) {
        let a = r.get(0).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let b = r.get(1).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let step = r
            .get(2)
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(1);
        if let (Ok(na), Ok(nb)) = (a.parse::<i64>(), b.parse::<i64>()) {
            // numeric range with zero-padding to the wider operand
            let width = a.len().max(b.len());
            let pad = |n: i64| -> String {
                let s = n.to_string();
                if s.len() < width {
                    format!("{}{}", "0".repeat(width - s.len()), s)
                } else {
                    s
                }
            };
            let mut out = Vec::new();
            if step > 0 {
                let mut n = na;
                while n <= nb {
                    out.push(pad(n));
                    n += step;
                }
            } else {
                let mut n = na;
                while n >= nb {
                    out.push(pad(n));
                    n += step;
                }
            }
            out
        } else {
            // char range a..z
            let ca = a.chars().next().unwrap_or('a');
            let cb = b.chars().next().unwrap_or('z');
            let mut out = Vec::new();
            if step > 0 {
                let mut c = ca as u32;
                let end = cb as u32;
                while c <= end {
                    out.push(char::from_u32(c).unwrap_or('?').to_string());
                    c = (c as i64 + step).max(0) as u32;
                }
            } else {
                let mut c = ca as i64;
                let end = cb as i64;
                while c >= end {
                    out.push(char::from_u32(c as u32).unwrap_or('?').to_string());
                    c += step;
                }
            }
            out
        }
    } else if let Some(s) = entry.as_str() {
        vec![s.to_string()]
    } else {
        Vec::new()
    }
}

/// Expand a `brace` Call: prefix + product(groups) + suffix.
fn brace_expand(args: &[IrExpr]) -> Vec<String> {
    let prefix = args
        .first()
        .and_then(|a| match a {
            IrExpr::Str(s, _) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let suffix = args
        .get(3)
        .and_then(|a| match a {
            IrExpr::Str(s, _) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let Some(v) = brace_json_arg(args) else {
        return vec![format!("{prefix}{suffix}")];
    };
    let mut groups: Vec<Vec<String>> = Vec::new();
    if let Some(gs) = v.as_array() {
        for g in gs {
            let mut items = Vec::new();
            if let Some(es) = g.as_array() {
                for e in es {
                    items.extend(brace_group_items(e));
                }
            }
            groups.push(items);
        }
    }
    if groups.is_empty() {
        return vec![format!("{prefix}{suffix}")];
    }
    let mut out: Vec<String> = vec![String::new()];
    for g in &groups {
        if g.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        for o in &out {
            for item in g {
                next.push(format!("{o}{item}"));
            }
        }
        out = next;
    }
    out.iter().map(|s| format!("{prefix}{s}{suffix}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A renderer with `name` declared as an Int var at width `w` (both
    /// the range the width analysis derives from, and the width itself —
    /// the shir_to_c flow's invariant).
    fn int_render(name: &str, lo: i64, hi: i64, w: Width) -> Render {
        let mut r = Render::default();
        r.var_types.insert(name.to_string(), IrType::Int);
        r.var_ranges.insert(name.to_string(), (lo, hi));
        r.var_widths.insert(name.to_string(), w);
        r
    }

    #[test]
    fn width_format_matches_c_type() {
        // the format must read exactly what the width's C type declares
        assert_eq!(Width::U32.format(), "%u");
        assert_eq!(Width::I32.format(), "%d");
        assert_eq!(Width::I64.format(), "%lld");
        assert_eq!(Width::U32.c_type(), "unsigned int");
        assert_eq!(Width::I32.c_type(), "int");
        assert_eq!(Width::I64.c_type(), "long long");
    }

    #[test]
    fn typed_var_reads_drop_the_cast() {
        // `echo $i` where i is proven u32 → `printf("%u\n", i)` — no cast
        // (the format matches the declared `unsigned int` exactly)
        for (name, lo, hi, w, fmt) in [
            ("i", 1, 10000, Width::U32, "%u"),
            ("x", -100, -100, Width::I32, "%d"),
            ("n", 1_000_000_000_000, 1_000_000_000_000, Width::I64, "%lld"),
        ] {
            let r = int_render(name, lo, hi, w);
            let e = IrExpr::Var(name.to_string(), None);
            assert_eq!(r.num_spec(&e), NumSpec::Num(fmt, false), "{name}");
        }
    }

    #[test]
    fn getvar_of_typed_var_matches_var() {
        // `$y` reads arrive as getVar("y"); the read renders as the
        // declared ident, so it gets the same cast-free spec
        let r = int_render("i", 1, 10000, Width::U32);
        let e = IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![IrExpr::Str("i".to_string(), crate::ir::StrStyle::DoubleQuoted)],
        };
        assert_eq!(r.num_spec(&e), NumSpec::Num("%u", false));
    }

    #[test]
    fn arith_over_same_width_leaves_drops_the_cast() {
        // `$((i * i))` — every var leaf at u32 → `snprintf(..., "%u",
        // (i * i))` — the usual arithmetic conversions keep `unsigned int`
        let r = int_render("i", 1, 10000, Width::U32);
        let e = IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "*".to_string(),
            lhs: Box::new(ArithAst::Var("i".to_string())),
            rhs: Box::new(ArithAst::Var("i".to_string())),
        }));
        assert_eq!(r.num_spec(&e), NumSpec::Num("%u", false));
    }

    #[test]
    fn mixed_width_arith_keeps_the_cast() {
        // a long long leaf in the tree → the result type is long long,
        // not the u32 the range might suggest → %lld + cast (the safe pair)
        let mut r = int_render("i", 1, 10000, Width::U32);
        r.var_types.insert("n".to_string(), IrType::Int);
        r.var_ranges.insert("n".to_string(), (1_000_000_000_000, 1_000_000_000_000));
        r.var_widths.insert("n".to_string(), Width::I64);
        let e = IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "*".to_string(),
            lhs: Box::new(ArithAst::Var("i".to_string())),
            rhs: Box::new(ArithAst::Var("n".to_string())),
        }));
        assert_eq!(r.num_spec(&e), NumSpec::Num("%lld", true));
    }

    #[test]
    fn string_var_and_literal_keep_the_cast() {
        // a Str var's width DEFAULTS to I64 but its C type is `char*` —
        // never type-matched; an int literal renders as `int` — never
        // long long. Both keep the %lld + (long long) pair.
        let mut r = Render::default();
        r.var_types.insert("s".to_string(), IrType::Str);
        assert_eq!(
            r.num_spec(&IrExpr::Var("s".to_string(), None)),
            NumSpec::Num("%lld", true)
        );
        assert_eq!(
            r.num_spec(&IrExpr::Int(42)),
            NumSpec::Num("%lld", true)
        );
    }
}
