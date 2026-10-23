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

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt, IrType, Sigil};
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
    /// vars used as associative arrays (declare as map[string]any)
    assoc: BTreeSet<String>,
    /// Collection homes proved by an EARLIER render of this program. `assoc`
    /// fills up WHILE rendering (and `arrays` only from the statement shapes
    /// the pre-scan walks), so a statement that comes before the collection's
    /// first write — `unset arr` above `typeset -a arr=(10 20 30)` — cannot
    /// see the home, and `unset`'s zero value depends on it (`nil` for a
    /// collection, "" for a scalar). Carrying the completed answer in from the
    /// previous render removes that order dependence; getting it wrong is a
    /// compile error, not a wrong value (`arr = ""` into a `[]any`).
    unset_collections: BTreeSet<String>,
    /// vars written anywhere (declared at the top of main)
    written: BTreeSet<String>,
    /// write accounting for the string-scalar inference (see shir_to_go):
    /// every noted write per var, and how many of those stored a Go string
    writes: HashMap<String, usize>,
    writes_str: HashMap<String, usize>,
    /// writes whose value is an INTEGER expression (`x=$?`, `x=$(( … ))`,
    /// `x=5`, `x=<int var>`). bash stores the decimal TEXT of those, so the
    /// target is still a string scalar (see `int_text_expr`).
    writes_inttext: HashMap<String, usize>,
    /// writes whose value is NEITHER a Go string NOR integer text (`x=$(( … ))`
    /// is text, but `x` from a generic `any` render is genuinely any). One
    /// such write disqualifies the string-scalar promotion. Counting these
    /// directly is what makes the verdict independent of how many times a
    /// caller happens to call `mark_written` for one write.
    writes_plain: HashMap<String, usize>,
    /// vars actually read (skip the `_ = x` dead-var guard)
    read: BTreeSet<String>,
    /// Go identifier per shell var name (sanitize + de-dup)
    mangle: HashMap<String, String>,
    /// shell function names (callable via `exec`)
    functions: BTreeSet<String>,
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
    /// background-body var saves pending a `wait` restore (fork isolation):
    /// (go ident, temp holding the pre-spawn value), oldest first.
    bg_saves: Vec<(String, String)>,
    /// `shopt -s nocasematch` seen — [[ == ]] compares case-insensitively
    nocase: bool,
    /// vars map + write-sync emissions needed (param-expansion ops)
    need_vars: bool,
    /// function calls pass positional args via `fArgs`
    need_fargs: bool,
    /// depth inside a user-function closure: a `Return` there is the
    /// function's VALUE (the frontend models a shell function's output as
    /// `Return(Interpolate(…))`), not a bare status return.
    in_fn: usize,
    loop_depth: usize,
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
    // the other stdlib packages the runtime preamble imports: a shell var
    // of the same name would shadow the package and break the preamble
    // (measured: `exec="/usr/sbin/dkms"` turned every `exec.Command` into a
    // string field access).
    "exec",
    "io",
    "bufio",
    "regexp",
    "filepath",
    "path",
    "strings",
    "strconv",
    "sync",
    "utf8",
    "syscall",
    "error",
    // runtime helpers / globals the preamble declares
    "st",
    "vars",
    "fArgs",
    "__wgd",
    "__wgds",
    "splitTop",
    "hasExtglob",
    "arrKeys",
    "assocIdx",
    "ifsJoin",
    "appendStrings",
    "splitLines",
    "shArgOff",
    "doShiftF",
    "doShiftS",
    "shArgs",
    "shStdIn",
    "shStdErr",
    "origStdin",
    "origStdout",
    "origStderr",
    "isDigits",
    "atoi",
    "pSliceIdx",
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
    "capPipe",
    "capPipeRaw",
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

/// An `echo`/`printf` invocation by absolute coreutils path (`/bin/echo`,
/// `/usr/bin/printf`, …) — the same program the builtin policy was measured
/// against, so every `echo`/`printf` lowering applies to it unchanged.
/// Returns the bare name, or `None` for anything else.
fn echo_cmd(cmd: &str) -> Option<&str> {
    let base = cmd.rsplit('/').next().unwrap_or(cmd);
    if (base == "echo" || base == "printf")
        && matches!(
            cmd,
            "echo"
                | "printf"
                | "/bin/echo"
                | "/usr/bin/echo"
                | "/bin/printf"
                | "/usr/bin/printf"
        )
    {
        Some(base)
    } else {
        None
    }
}

/// Commands bash resolves to a BUILTIN rather than to an executable on
/// PATH. `exec.Command(name, …)` does not go through bash at all, so a
/// native stage naming one of these is only sound when
/// `native_stage_builtin` proves the external counterpart agrees.
fn is_shell_builtin(cmd: &str) -> bool {
    matches!(
        cmd,
        "echo" | "printf" | "exit" | "cd" | "let" | "local" | "read" | "test"
    )
}

/// May `cmd` run as a native pipeline STAGE (a real process) instead of
/// inside a `bash -c` reconstruction?
///
/// Only `echo` and `printf` qualify, and only because the corpus's uses of
/// them are argv-identical between bash's builtin and coreutils:
/// measured against this box, `/usr/bin/echo` agrees with the builtin for
/// plain words, empty words, `-n`, `-e` (both escape classes), a literal
/// `-n`/`-e` operand, `--`, and a lone `-`.
///
/// `printf`'s `%q` is the one verb coreutils' printf does NOT implement
/// (it shell-quotes its argument, bash-style), so a format that mentions it
/// — or a format this renderer cannot inspect — keeps the `bash -c` stage.
/// `%b`, `%s`, `%d`, `%%` are common to both.
///
/// The remaining builtins are refused outright: `read`/`cd`/`let`/`local`/
/// `exit` have no external executable to exec, and `test`'s argv form is
/// not the builtin's semantics (`[` and `test` differ on argument count,
/// and bash's is the one the IR was lowered against).
fn native_stage_builtin(cmd: &str, argv: &[IrExpr]) -> bool {
    match echo_cmd(cmd).unwrap_or(cmd) {
        "echo" => true,
        "printf" => match argv.first() {
            Some(IrExpr::Str(fmt, _)) => !fmt.contains("%q"),
            _ => false,
        },
        _ => false,
    }
}

/// An operand word is a literal path — not a glob splice, which expands to an
/// argv only the shell knows, and not `-`, which is stdin, not a filename.
/// A `brace` operand is a splice too (`rm -f {a,b}` is two paths, one word).
fn fs_literal_operand(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(s, style) => {
            s != "-" && !str_expands_glob(s, style)
        }
        IrExpr::Call { func, .. } => func != "brace",
        _ => true,
    }
}

/// `rm`'s accepted argv shape → (recursive, force, operands).
///
/// REFUSE (None) anything outside it rather than guess: an unknown flag has
/// no os.* counterpart (`-v` writes to stdout, `-i` prompts, `-d`/`--one-file-
/// system` differ), `--` changes what counts as a flag. A glob operand is
/// admitted (it expands at run time through `globArgs`, exactly like the
/// exec argv does); `-` and anything else the renderer cannot see keep the
/// fork/exec.
fn rm_native_shape(argv: &[IrExpr]) -> Option<(bool, bool, Vec<&IrExpr>)> {
    let (mut recursive, mut force) = (false, false);
    let mut ops: Vec<&IrExpr> = Vec::new();
    let mut operands = false;
    for a in argv {
        match a {
            IrExpr::Str(s, _) if !operands && s.len() > 1 && s.starts_with('-') => {
                let Some(long) = s.strip_prefix("--") else {
                    for c in s[1..].chars() {
                        match c {
                            'f' => force = true,
                            'r' | 'R' => recursive = true,
                            _ => return None,
                        }
                    }
                    continue;
                };
                // --preserve-root is rm's default, so it is a no-op here
                if long != "preserve-root" {
                    return None;
                }
            }
            // A glob splices its matches at run time (`rm file_*.txt`
            // removes whatever the pattern matches then) — the emitter
            // expands it through `globArgs`, which also reproduces the
            // no-match-passes-literally rule, so it stays native.
            IrExpr::Str(s, _) if s != "-" => {
                operands = true;
                ops.push(a);
            }
            _ if !fs_literal_operand(a) => return None,
            _ => {
                operands = true;
                ops.push(a);
            }
        }
    }
    if ops.is_empty() {
        None
    } else {
        Some((recursive, force, ops))
    }
}

/// `mkdir`'s accepted argv shape → (parents, operands). `-m MODE` takes an
/// operand of its own and `-v` prints to stdout, so both refuse.
fn mkdir_native_shape(argv: &[IrExpr]) -> Option<(bool, Vec<&IrExpr>)> {
    let mut parents = false;
    let mut ops: Vec<&IrExpr> = Vec::new();
    let mut operands = false;
    for a in argv {
        match a {
            IrExpr::Str(s, _) if !operands && s.len() > 1 && s.starts_with('-') => {
                if s != "-p" && s != "--parents" {
                    return None;
                }
                parents = true;
            }
            _ if !fs_literal_operand(a) => return None,
            _ => {
                operands = true;
                ops.push(a);
            }
        }
    }
    if ops.is_empty() {
        None
    } else {
        Some((parents, ops))
    }
}

/// `ls`'s accepted argv shape → (all, almost-all, directory-itself, operands).
///
/// `-1` is admitted as a no-op: `ls` only lays out columns on a tty, and the
/// render's stdout never is one. `-l`/`-F`/`-R`/`-t`/`-S`/`-i` change the
/// output itself and refuse, as does any long option outside the three the
/// template reproduces. An empty operand list is valid (`ls` = `ls .`).
fn ls_native_shape(argv: &[IrExpr]) -> Option<(bool, bool, bool, Vec<&IrExpr>)> {
    let (mut all, mut almost, mut dir_itself) = (false, false, false);
    let mut ops: Vec<&IrExpr> = Vec::new();
    let mut operands = false;
    for a in argv {
        match a {
            IrExpr::Str(s, _) if !operands && s == "--" => operands = true,
            IrExpr::Str(s, _) if !operands && s.len() > 1 && s.starts_with('-') => {
                let Some(long) = s.strip_prefix("--") else {
                    for c in s[1..].chars() {
                        match c {
                            '1' => {}
                            'a' => all = true,
                            'A' => almost = true,
                            'd' => dir_itself = true,
                            _ => return None,
                        }
                    }
                    continue;
                };
                match long {
                    "all" => all = true,
                    "almost-all" => almost = true,
                    "directory" => dir_itself = true,
                    _ => return None,
                }
            }
            _ if !fs_literal_operand(a) => return None,
            _ => {
                operands = true;
                ops.push(a);
            }
        }
    }
    Some((all, almost, dir_itself, ops))
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
    // Shell scalars hold TEXT, so an untyped var (A2 has no verdict) whose
    // EVERY write stored a Go string is a Go `string`, not `any` + an
    // `s2s(v)` coercion at each read. The write kinds are only known once
    // the body has been rendered, so a render notes the writes and the next
    // one hoists the promoted vars as `string`.
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.program(&prog);
    // Promotion is a FIXED POINT, not one round: making `n` a `string`
    // turns `factors = n` from an `any` write into a `string` write, which
    // is what lets `factors` promote in the next round. The promoted set
    // only grows — a promoted name carries a `Str` verdict afterwards and
    // every predicate below refuses a verdict that is not `Any` — so each
    // round adds at least one name and the loop terminates.
    //
    // The same loop carries the COLLECTION homes forward. `assoc` is only
    // discovered while rendering, so `unset arr` above `typeset -a arr=(…)`
    // saw no home in this first pass; the set only grows and is bounded by
    // the program's variables, so both halves terminate together.
    let mut unset_collections: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut changed = false;
        let seen: BTreeSet<String> = r.arrays.union(&r.assoc).cloned().collect();
        if !seen.is_subset(&unset_collections) {
            unset_collections.extend(seen);
            changed = true;
        }
        let mut promoted = promotable_scalars(&prog, &r);
        if !promoted.is_empty() {
            // `Render::writes`/`written` are hash maps, so the candidate order
            // is arbitrary; sorting keeps the hoisted `var` block byte-identical
            // across runs.
            promoted.sort_by(|a, b| a.0.cmp(&b.0));
            // A promoted name may already carry an `Any` entry; drop it so the
            // program never holds two verdicts for one var — `var_types` is read
            // as a map downstream and a duplicate would make the answer depend
            // on entry order.
            let names: BTreeSet<String> = promoted.iter().map(|(v, _)| v.clone()).collect();
            prog.var_types
                .retain(|(t, ty)| !(*ty == IrType::Any && names.contains(t)));
            prog.var_types.extend(promoted);
            changed = true;
        }
        if !changed {
            break;
        }
        r = Render::default();
        r.unset_collections = unset_collections.clone();
        r.var_types = prog.var_types.iter().cloned().collect();
        r.program(&prog);
    }
    r.out.join("\n")
}

/// The untyped scalars of one render that bash semantics say are `string`:
/// every write stored Go text (a string value, or an integer's decimal
/// TEXT), so the Go home can be `string` instead of `any` plus an `s2s(v)`
/// coercion at every read.
fn promotable_scalars(prog: &IrProgram, r: &Render) -> Vec<(String, IrType)> {
    let mut promoted: Vec<(String, IrType)> = Vec::new();
    for (v, n) in &r.writes {
        // every write must have been a string write, and the var must not
        // already have a verdict or a non-scalar home (decl_lines' type
        // order: functions, assoc, array, int64, string)
        if *n == 0
            || r.writes_str.get(v).copied().unwrap_or(0)
                + r.writes_inttext.get(v).copied().unwrap_or(0)
                != *n
            || r.functions.contains(v)
            || r.assoc.contains(v)
            || r.arrays.contains(v)
            || r.arith_ints.contains(v)
            // a BAKED array-element target (`options["$k"]`): the name is a
            // store KEY, and declaring a var_types entry for it is exactly
            // what makes decl_lines' `n.contains('[')` rule hoist `[]any`
            || v.contains('[')
            // A verdict that NAMES a Go scalar home is A2's answer and is
            // kept. `Any` names none — it is the annotation for "no
            // scalar type known" — so the write shapes above are strictly
            // better evidence, and promoting it is what removes the
            // `s2s(v)` coercion from every read.
            || prog
                .var_types
                .iter()
                .any(|(t, ty)| t == v && *ty != IrType::Any)
        {
            continue;
        }
        promoted.push((v.clone(), IrType::Str));
    }
    // A var the program only READS holds bash's value for an unset variable
    // ("") — and the reads already resolve the hoisted Go var, not the
    // environment (`getvar_str`/`expr_str` only call `os.Getenv` for a name
    // that is NOT declared). A2 still hands such a name a verdict with no Go
    // scalar home (`Any`), which hoisted it as `var v any` — nil, formatted
    // by `s2s` at EVERY read. `string` is the faithful hoist: the same ""
    // value with no coercion, so the read is the identifier itself.
    for v in &r.written {
        // Verdict-less or `Any` only: every OTHER verdict names a scalar home
        // (int64 for Int, `string` for Str/AsciiStr) and overriding it here
        // would fight A2 instead of completing it.
        let verdict = prog.var_types.iter().find(|(t, _)| t == v).map(|(_, ty)| ty);
        if r.writes.contains_key(v)
            || r.functions.contains(v)
            || r.assoc.contains(v)
            || r.arrays.contains(v)
            || r.arith_ints.contains(v)
            || v.contains('[')
            || !matches!(verdict, None | Some(IrType::Any))
        {
            continue;
        }
        promoted.push((v.clone(), IrType::Str));
    }
    promoted
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
        if GO_RESERVED.contains(&m.as_str()) {
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

    /// Undo shell backslash-quoting in a pattern word (`\"` → `"`,
    /// `\\` → `\`): inside `${…}` a backslash quotes the next char, but
    /// the runtime matchers would read the backslash literally.
    fn unescape_pat(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut it = s.chars();
        while let Some(c) = it.next() {
            if c == '\\' {
                if let Some(n) = it.next() {
                    out.push(n);
                } else {
                    out.push('\\');
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int)
    }

    fn is_str(&self, name: &str) -> bool {
        // AsciiStr is an ADDITIVE annotation ("backends that ignore it
        // treat it as Str", src/ir.rs) — Go has one string type, so both
        // verdicts hoist as `string`. Ignoring AsciiStr left every var the
        // C/frontend paths annotate as `any` + `s2s(v)` at each read.
        matches!(
            self.var_types.get(name),
            Some(IrType::Str) | Some(IrType::AsciiStr)
        )
    }

    fn is_arr(&self, name: &str) -> bool {
        self.arrays.contains(name)
    }

    /// True when the var is an associative array (map storage).
    fn is_assoc(&self, name: &str) -> bool {
        self.assoc.contains(name)
    }

    /// True when `decl_lines` will hoist `v` as Go `string` — the one case
    /// where an `s2s(v)` coercion is a no-op (s2s of a string is the
    /// string). Mirrors decl_lines' type order exactly, so the answer can
    /// never name a var that hoists as int64/[]any/func().
    fn declared_go_string(&self, v: &str) -> bool {
        !self.is_arr(v)
            && !self.is_num(v)
            && !self.arith_ints.contains(v)
            && !self.functions.contains(v)
            && self.is_str(v)
    }

    /// True when `decl_lines` will hoist `v` as Go `int64` — the case where
    /// a string-context read is a native `strconv.FormatInt` and not an
    /// `s2s(any)` coercion (which boxes the value and `%v`-formats it).
    /// Mirrors decl_lines' type order exactly, so the answer can never name
    /// a var that hoists as string/[]any/func().
    fn declared_go_int(&self, v: &str) -> bool {
        !self.is_arr(v)
            && !self.functions.contains(v)
            && (self.is_num(v) || self.arith_ints.contains(v))
    }

    /// True when `expr_any` renders this expression as a Go `string` — the
    /// write is a string write. Purely syntactic: no rendering, no state.
    fn expr_yields_go_string(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Str(_, _) | IrExpr::Interpolate(_) => true,
            IrExpr::BinOp { op, .. } => *op == crate::ir::BinOpKind::Concat,
            // `$( … )` lowers to a capCmd/capRun value — a Go string (the
            // stub path renders the int 0 and raises a TODO, which the
            // caller's TODO-delta check rejects)
            IrExpr::Capture { .. } => true,
            // A positional read (`$1`, `$@`) is `paramAt`/`strings.Join`,
            // both Go strings; a var hoists as a string when it has the
            // verdict. A var that is NOT declared anywhere is `os.Getenv(…)`
            // in `expr_any` — a Go string too, which is what makes
            // `cpu="$basic_machine"` a string write instead of leaving
            // `cpu` an `any` boxed by `s2s` at every read.
            IrExpr::Var(v, _) | IrExpr::Ident(v) => {
                Self::positional_str(v) || self.declared_go_string(v) || !self.declared(v)
            }
            IrExpr::Call { func, args } => match func.as_str() {
                // every one of these renders through a `string`-returning
                // helper (see `call`): a capture, a `${…}` expansion, a
                // join/brace list, an array/map element, a user-function
                // value. The stub fallbacks render an int and raise a TODO,
                // which the caller's TODO-delta check rejects.
                "capture" | "captureWords" | "param" | "join" | "brace" | "arrayIndex"
                | "fnValue"
                // `split` is `strings.Join(strings.Fields(…), " ")` and
                // `arrayItems` is `strings.Join(pList(…), " ")` — both Go
                // strings, so the caller's `s2s` would only box them back.
                | "split" | "arrayItems" => true,
                "getVar" => match args.first() {
                    // `getvar_any` returns `os.Getenv(…)` — a Go string —
                    // for a name that is not declared anywhere.
                    Some(IrExpr::Str(n, _)) => {
                        Self::positional_str(n) || self.declared_go_string(n) || !self.declared(n)
                    }
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    }

    /// A positional/splat parameter name (`1`, `@`, `*`): bash holds it as
    /// TEXT, and `getvar_any`/`getvar_str` both render it as a Go string
    /// (`paramAt(i)` / `strings.Join(...)`). `#`/`$`/`?` stay out — those
    /// are int64.
    fn positional_str(name: &str) -> bool {
        matches!(name, "@" | "*")
            || (!name.is_empty() && name.chars().all(|c| c.is_ascii_digit()))
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
        *self.writes.entry(name.to_string()).or_insert(0) += 1;
        self.written.insert(name.to_string());
    }

    /// A write whose value is a Go `string` (a command substitution, a
    /// builtin's capture value, …). shir_to_go's second pass hoists a var
    /// whose EVERY write is one of these as Go `string` instead of `any` +
    /// an `s2s(v)` coercion at each read.
    fn mark_str_written(&mut self, name: &str) {
        self.mark_written(name);
        *self.writes_str.entry(name.to_string()).or_insert(0) += 1;
    }

    fn mark_arr(&mut self, name: &str) {
        self.arrays.insert(name.to_string());
        self.mark_written(name);
    }

    /// Mark a var as assoc-written (declared map[string]any).
    fn mark_assoc(&mut self, name: &str) {
        self.assoc.insert(name.to_string());
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

    /// `vars["x"] = x` inline (for the one-line assign form).
    fn sync_inline(&mut self, name: &str) -> String {
        if self.need_vars {
            let m = self.go_ident(name);
            return format!(" vars[{}] = {m};", Self::go_str(name));
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
                matches!(args.first(), Some(IrExpr::Str(name, _)) if self.is_num(name))
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
                if !self.declared(name) {
                    return format!("s2i(os.Getenv({}))", Self::go_str(name));
                }
                let m = self.ident_of(name);
                if self.is_num(name) {
                    m
                } else {
                    format!("s2i({m})")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    return format!("s2i(os.Getenv({}))", Self::go_str(name));
                }
                self.ident_of(name)
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
        match name {
            "?" => {
                self.need_st = true;
                self.st_read = true;
                "strconv.FormatInt(st, 10)".to_string()
            }
            "$" => "strconv.FormatInt(int64(os.Getpid()), 10)".to_string(),
            "#" => "strconv.FormatInt(int64(len(argsList())), 10)".to_string(),
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
                    } else if self.declared_go_int(name) {
                        format!("strconv.FormatInt({m}, 10)")
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
            // The `\x01SH2GLOB\x01` prefix is core-internal metadata, never
            // part of the value: emitting it raw put the marker INSIDE a
            // filename (`os.Remove("\x01SH2GLOB\x01file_*.txt")`).
            IrExpr::Str(s, _) => Self::go_str(&strip_sh2glob(s)),
            // a numeric literal's string form is known at render time —
            // bash never sees a separate value here.
            IrExpr::Int(i) => Self::go_str(&i.to_string()),
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    return format!("os.Getenv({})", Self::go_str(name));
                }
                let m = self.ident_of(name);
                if self.is_str(name) {
                    m
                } else if self.declared_go_int(name) {
                    format!("strconv.FormatInt({m}, 10)")
                } else {
                    format!("s2s({m})")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    return format!("os.Getenv({})", Self::go_str(name));
                }
                self.ident_of(name)
            }
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            // arithmetic is int64 throughout, so its decimal form is
            // strconv's, not the `any`-boxing helper's
            IrExpr::Arith(a) => format!("strconv.FormatInt({}, 10)", self.arith(a)),
            IrExpr::Bool(b) => {
                if *b {
                    "\"1\"".into()
                } else {
                    "\"0\"".into()
                }
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    return self.getvar_str(name);
                }
                format!("s2s({})", self.call("getVar", args))
            }
            // `$(…)` lowers to capCmd/capRun — already a Go string; only
            // the (stub) fallback renders an int and needs the coercion.
            IrExpr::Capture { expr, native } => match self.capture_str(expr, *native) {
                Some(s) => s,
                None => {
                    let s = self.sh2_stub("capture");
                    format!("s2s({s})")
                }
            },
            // An array/map element or length read renders as `s2s(arrIdx(…))`
            // etc. — already a Go string, so the `other` arm's coercion would
            // double it (`s2s(s2s(…))`). Only the stub fallback (an int `0`
            // plus a TODO marker) needs the coercion, so the marker decides,
            // not the shape — the same rule as `capture` below.
            IrExpr::Call { func, args } if func == "arrayIndex" || func == "arrayLen" => {
                let todo = self.todo;
                let v = self.call(func, args);
                if self.todo != todo {
                    format!("s2s({v})")
                } else {
                    v
                }
            }
            // A `capture`/`captureWords` CALL lowers through the same
            // capture paths (capCmd/capRun/capPipe or a native value) —
            // already a Go string, so the coercion is a no-op. Only the
            // stub fallback (an int `0` plus a TODO marker) needs it, so
            // the marker decides, not the shape.
            IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
                let todo = self.todo;
                let v = self.call(func, args);
                if self.todo != todo {
                    format!("s2s({v})")
                } else {
                    v
                }
            }
            other => {
                if self.any_is_go_string(other) {
                    // already a Go string — the coercion is a no-op
                    self.expr_any(other)
                } else {
                    let s = self.expr_any(other);
                    format!("s2s({s})")
                }
            }
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
                if !self.declared(name) {
                    return format!("(os.Getenv({}) != \"\")", Self::go_str(name));
                }
                let m = self.ident_of(name);
                if self.is_num(name) {
                    format!("({m} != 0)")
                } else if self.declared_go_string(name) {
                    // hoisted as Go `string` — the coercion is a no-op
                    format!("({m} != \"\")")
                } else {
                    format!("(s2s({m}) != \"\")")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    return format!("(os.Getenv({}) != \"\")", Self::go_str(name));
                }
                let m = self.ident_of(name);
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
                // bash -c fallback for a test go can't parse natively.
                // The script text is known (`[ s ]`), so the child gets
                // only the vars it reads (see `env_lit_for`).
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return c;
                    }
                    let text = format!("[ {s} ]");
                    let env = self.env_lit_for(Some(&text));
                    return format!("(redirRun({}, {env}) == 0)", Self::go_str(&text));
                }
                self.sh2_stub("test")
            }
            IrExpr::Call { func, args }
                if matches!(func.as_str(), "contains" | "strHasPrefix" | "strHasSuffix") =>
            {
                // `echo X | grep LIT >/dev/null` → contains(X, LIT), and the
                // test-lowering transform's glob-affix twins: native strings
                // predicates (the import is auto-derived from the text). In
                // CONDITION position they are already a Go bool, so no
                // `truthy` wrapper.
                if args.len() >= 2 {
                    let needle = self.expr_str(&args[0]);
                    let pattern = self.expr_str(&args[1]);
                    let f = match func.as_str() {
                        "strHasPrefix" => "HasPrefix",
                        "strHasSuffix" => "HasSuffix",
                        _ => "Contains",
                    };
                    return format!("strings.{f}({needle}, {pattern})");
                }
                self.sh2_stub(func)
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
                        // bash rejects the expression, so `let` fails and
                        // the condition is false.
                        if self.arith_fail(s) == ArithFail::Invalid {
                            return "false".to_string();
                        }
                    }
                }
                self.sh2_stub("let-cond")
            }
            IrExpr::Call { func, args }
                if func == "exec" || func == "pipeline" || func == "redirect" =>
            {
                // A user FUNCTION here (`if func …`, `while func …`) calls
                // the closure and tests the shared `st` — same contract as
                // the chain_operand arm above.
                if func == "exec" {
                    if let Some(IrExpr::Str(cmd, _)) = args.first() {
                        if self.functions.contains(cmd) {
                            let m = self.go_ident(cmd);
                            self.mark_read(cmd);
                            self.need_fargs = true;
                            self.need_st = true;
                            let vals: Vec<String> = match args.get(1) {
                                Some(IrExpr::Array(items)) => {
                                    items.iter().map(|i| self.expr_str(i)).collect()
                                }
                                _ => Vec::new(),
                            };
                            if vals.is_empty() {
                                self.sides.push(format!("{m}();"));
                            } else {
                                self.sides.push(format!(
                                    "fArgs = []string{{{}}};",
                                    vals.join(", ")
                                ));
                                self.sides.push(format!("{m}();"));
                                self.sides.push("fArgs = nil;".to_string());
                            }
                            return "(st == 0)".to_string();
                        }
                    }
                }
                // a command is TRUE when it succeeds (status 0) — the
                // && / || short-circuit then runs the rhs exactly like bash
                let v = self.call(func, args);
                format!("({v} == 0)")
            }
            IrExpr::Call { func, args } if func == "arith" => {
                // `while (( i < 5 ))` — arith string as condition
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((_, v)) = self.arith_str(s) {
                        return format!("({v} != 0)");
                    }
                    // bash rejects the expression: `(( ))` is false.
                    if self.arith_fail(s) == ArithFail::Invalid {
                        return "false".to_string();
                    }
                }
                self.sh2_stub("arith-cond")
            }
            IrExpr::Call { func, args } if func == "let" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((_, v)) = self.arith_str(s) {
                        return format!("({v} != 0)");
                    }
                    // bash rejects the expression: `let` returns 1.
                    if self.arith_fail(s) == ArithFail::Invalid {
                        return "false".to_string();
                    }
                }
                self.sh2_stub("let-cond")
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
                    } else if self.declared_go_string(name) {
                        // hoisted as Go `string` — the coercion is a no-op
                        format!("({m} != \"\")")
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
                if !self.declared(name) {
                    return format!("os.Getenv({})", Self::go_str(name));
                }
                self.ident_of(name)
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    return format!("os.Getenv({})", Self::go_str(name));
                }
                self.ident_of(name)
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
                self.mark_arr(var);
                self.ident_of(var);
                let k = self.expr_num(key);
                format!("arrIdx({}, {k})", self.ident_of(var))
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
        if let Some(s) = self.capture_str(expr, native) {
            return s;
        }
        self.sh2_stub("capture")
    }

    /// `capture_expr` when the lowering yields a Go `string`
    /// (`capCmd*`/`capRun`). `None` when the capture falls back to the
    /// `sh2_stub` marker, which renders as the int `0` — a caller in a
    /// string context must coerce that one.
    fn capture_str(&mut self, expr: &IrExpr, native: bool) -> Option<String> {
        if let IrExpr::Arrow(body) = expr {
            // The core already lowered the command inside `$( … )` to a
            // value-producing op (`$(echo … | grep -o PAT)` arrives as
            // `capture(Arrow[Expr(grepMatches(…)])`): the op IS the command
            // and yields the string natively — no subprocess at all.
            if let Some(v) = self.capture_value_op(body, native) {
                return Some(v);
            }
            // `$(cmd <<< s)` / `$(cmd <<EOF … EOF)`: stdin fed from the
            // string — more specific than the builtins below (it carries a
            // redirect they refuse), so it goes first.
            if let Some(v) = self.stdin_str_capture(body) {
                return Some(v);
            }
            // `$(echo …)` / `$(printf …)`: the builtin's VALUE is a native
            // string expression (no subprocess at all) — cheaper and more
            // faithful than exec'ing coreutils for it (AGENTS.md §2).
            if let Some(v) = self.echo_printf_capture_value(body) {
                return Some(v);
            }
            // `$(wc -c|-l|-w|-m < f)`: the count, not a subprocess.
            if let Some(v) = self.wc_capture_body(body) {
                return Some(v);
            }
            // `$(grep …)`: the same os.ReadFile+regexp scan the statement
            // form runs, answering the hits instead of the status.
            if let Some(v) = self.grep_capture_body(body) {
                return Some(v);
            }
            // `$(ls …)` / `$(cat …)`: the same native reads/listing the
            // statement forms run, answering the bytes instead of printing.
            if let Some(v) = self.fs_capture_body(body) {
                return Some(v);
            }
            // `allow_builtin`: a lone `echo`/`printf` exec's natively when
            // coreutils agrees with the builtin (native_stage_builtin), so
            // `$(printf '%s\n' "$a")` costs one process instead of a
            // `bash -c` child plus its whole env literal.
            if let Some((cmd, argv)) = self.body_single_exec_opt(body, true) {
                // Spread-aware: a mid-list glob (`ls * .sh`) folds through
                // `append`, which a raw `join` cannot express (see
                // `exec_cmd_call`).
                let call = Self::exec_cmd_call(&cmd, &argv);
                if native {
                    return Some(format!("capCmdRaw({call})"));
                }
                return Some(format!("capCmd({call})"));
            }
            // `$(A && B)` / `$(A || B)` where both operands are single
            // external commands: two `exec.Cmd`s in a closure, instead of a
            // `bash -c` child handed the reconstructed script.
            if let [IrStmt::Expr(IrExpr::BinOp { op, lhs, rhs })] = &body[..] {
                if let Some(v) = self.and_or_capture(op.clone(), lhs, rhs) {
                    return Some(v);
                }
            }
            if let Some(text) = self.cmd_text_stmts(body) {
                // Same env narrowing the `redirRun` reconstructions use: the
                // child runs `text`, so only the vars that text READS are
                // worth marshalling (see `env_lit_for`).
                let env = self.env_lit_for(Some(&text));
                return Some(format!("capRun({}, {env})", Self::go_str(&text)));
            }
        }
        None
    }

    /// A `&&`/`||` operand as a native `exec.Command(…)` call, or `None`.
    ///
    /// The operands come from inside a command substitution, so they are
    /// real processes: an `exec` call with a literal command word and a
    /// literal argv Array. A shell builtin is admitted only when the target
    /// has a faithful external counterpart (`native_stage_builtin`) — the
    /// same policy `body_single_exec_opt` applies to pipeline stages.
    fn operand_exec_call(&mut self, e: &IrExpr) -> Option<String> {
        let IrExpr::Call { func, args } = e else {
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let Some(IrExpr::Str(cmd, _)) = args.first() else {
            return None;
        };
        let argv_items: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items,
            _ => return None,
        };
        if is_shell_builtin(cmd) && !native_stage_builtin(cmd, argv_items) {
            return None;
        }
        let cmd = Self::go_str(cmd);
        let argv = self.argv_strings(argv_items)?;
        Some(Self::exec_cmd_call(&cmd, &argv))
    }

    /// `$(A && B)` / `$(A || B)` → a native closure running both stages.
    ///
    /// The captured value is the CONCATENATION of what the stages that ran
    /// wrote to stdout (bash's substitution reads one shared stream), and
    /// it is trimmed once at the end exactly like `capCmd` does — a stage
    /// that exits nonzero still contributes its output, since bash keeps it.
    fn and_or_capture(
        &mut self,
        op: crate::ir::BinOpKind,
        lhs: &IrExpr,
        rhs: &IrExpr,
    ) -> Option<String> {
        if op != crate::ir::BinOpKind::And && op != crate::ir::BinOpKind::Or {
            return None;
        }
        let l = self.operand_exec_call(lhs)?;
        let r = self.operand_exec_call(rhs)?;
        let id = self.new_tmp();
        let (ca, oa, ea) = (
            format!("__aoca{id}"),
            format!("__aoa{id}"),
            format!("__aoe{id}"),
        );
        let (cb, ob, s) = (
            format!("__aocb{id}"),
            format!("__aob{id}"),
            format!("__aos{id}"),
        );
        // `&&` runs the rhs when the first command SUCCEEDED, `||` when it
        // failed.
        let cond = if op == crate::ir::BinOpKind::And {
            format!("{ea} == nil")
        } else {
            format!("{ea} != nil")
        };
        Some(format!(
            "func() string {{ {ca} := {l}; shStdIn({ca}); shStdErr({ca}); \
             {oa}, {ea} := {ca}.Output(); {s} := string({oa}); \
             if {cond} {{ {cb} := {r}; shStdIn({cb}); shStdErr({cb}); \
             {ob}, _ := {cb}.Output(); {s} += string({ob}) }}; \
             return strings.TrimRight({s}, \"\\n\") }}()"
        ))
    }

    /// If the body is exactly one external `exec` call, return its
    /// (cmd expr, argv expr list). Shell builtins stay refused — see
    /// `body_single_exec_opt`.
    fn body_single_exec(&mut self, body: &[IrStmt]) -> Option<(String, Vec<String>)> {
        self.body_single_exec_opt(body, false)
    }

    /// `body_single_exec` with the PIPELINE STAGE's builtin policy.
    ///
    /// A pipeline stage is exec'd as a real process here, so a builtin
    /// stage is only admissible when the target has a faithful external
    /// counterpart (`native_stage_builtin`). `read`/`cd`/`let`/`local`/
    /// `exit` have NO external executable at all — exec'ing them would
    /// fail outright — and `test`'s argv form differs from the builtin's,
    /// so those keep the `bash -c` stage.
    ///
    /// A lone `cmd 2>/dev/null` (or `2>>/dev/null`) unwraps to the inner
    /// command: discarding stderr is what the native `runPipe`/`capPipe`/
    /// `runCmd`/`capCmd` helpers do with every child anyway, so the
    /// redirect is a no-op under them. Any other redirect keeps the
    /// `bash -c` stage (REFUSE > GUESS).
    fn body_single_exec_opt(
        &mut self,
        body: &[IrStmt],
        allow_builtin: bool,
    ) -> Option<(String, Vec<String>)> {
        let mut execs = Vec::new();
        for s in body {
            match s {
                IrStmt::Expr(e) => {
                    if let IrExpr::Call { func, args } = e {
                        // `builtin` is what the frontend emits for echo and
                        // friends; the prelude rewrites most of those to
                        // `exec`, but pipeline stages (and other nested
                        // positions) still arrive spelled `builtin` — the
                        // builtin POLICY below (admission check) applies to
                        // both spellings alike, so extraction accepts both.
                        // The redirect spelling (`builtin(Arrow, redirs)`,
                        // same as `redirect(Arrow, redirs)`) unwraps only
                        // for stderr discards — see `lone_exec_args`.
                        if func == "exec" || func == "builtin" {
                            match args.first() {
                                Some(IrExpr::Str(..)) | Some(IrExpr::Ident(..)) => {
                                    execs.push(args.clone());
                                    continue;
                                }
                                _ => {
                                    if let Some(a) = lone_exec_stmt(s) {
                                        execs.push(a);
                                        continue;
                                    }
                                }
                            }
                        }
                        // `( cmd ) 2>/dev/null` in expression position.
                        if func == "redirect" {
                            if let (Some(IrExpr::Arrow(inner)), Some(IrExpr::Array(redirs))) =
                                (args.first(), args.get(1))
                            {
                                if redirs.iter().all(null_stderr_obj) {
                                    if let Some(a) = lone_exec_args(inner) {
                                        execs.push(a);
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
                // The core's `Exec` node is the same command in a richer
                // shape (flat argv, plus the env/redirect/capture fields).
                // Synthesize the `exec` Call shape so a single-command body
                // lowers to a native capCmd/runCmd instead of a bash -c
                // fallback; env/redirect/capture keep the fallback.
                IrStmt::Exec {
                    cmd,
                    args,
                    capture: None,
                    redirects,
                    env,
                } if redirects.is_empty() && env.is_empty() => {
                    let mut call_args = Vec::with_capacity(args.len() + 1);
                    call_args.push(cmd.clone());
                    call_args.push(IrExpr::Array(args.clone()));
                    execs.push(call_args);
                    continue;
                }
                // `cmd 2>/dev/null` in statement position — see above.
                IrStmt::Redirect { inner, redirects } => {
                    if redirects.iter().all(|r| null_stderr_spec(r)) {
                        if let Some(a) = lone_exec_args(inner) {
                            execs.push(a);
                            continue;
                        }
                    }
                }
                _ => {}
            }
            return None;
        }
        if execs.len() != 1 {
            return None;
        }
        let args = &execs[0];
        if args.is_empty() {
            return None;
        }
        if let Some(IrExpr::Str(cmd, _)) = args.first() {
            // A shell builtin is refused unless the caller allows builtins
            // AND the external counterpart agrees (`native_stage_builtin`:
            // echo, and printf without `%q`). The plain calls (capCmd for
            // `$(…)`) keep `allow_builtin = false`, which is the historical
            // behaviour.
            let argv_items: &[IrExpr] = match args.get(1) {
                Some(IrExpr::Array(items)) => items,
                _ => &[],
            };
            if is_shell_builtin(cmd)
                && !(allow_builtin && native_stage_builtin(cmd, argv_items))
            {
                return None;
            }
            // A stage `echo`/`printf` whose argv contains a lone `%` (as in
            // `echo "7 % 3" | bc`) must run the EXTERNAL coreutils binary —
            // the builtin policy above admits it only when the external
            // counterpart agrees, and for `%`-formats only printf's
            // no-`%q` rule was checked. Rather than reasoning about echo's
            // `%`-handling here, refuse the stage: the pipeline falls back
            // to the `bash -c` weld whose argv is exactly bash's. Plain
            // literals arrive either bare or wrapped in an all-literal
            // Interpolate; anything dynamic keeps the native path (its
            // value is computed, not re-parsed).
            if (cmd == "echo" || cmd == "printf")
                && argv_items.iter().any(|a| match a {
                    IrExpr::Str(s, _) => s.contains('%'),
                    IrExpr::Interpolate(parts) => {
                        !parts.is_empty()
                            && parts.iter().all(|p| match p {
                                InterpPart::Lit(t) => !t.contains('$'),
                                _ => false,
                            }) && parts
                                .iter()
                                .map(|p| match p {
                                    InterpPart::Lit(t) => t.as_str(),
                                    _ => "",
                                })
                                .collect::<String>()
                                .contains('%')
                    }
                    _ => false,
                })
            {
                return None;
            }
            let cmd = Self::go_str(cmd);
            let argv = match args.get(1) {
                Some(IrExpr::Array(items)) => self.argv_strings(items)?,
                _ => Vec::new(),
            };
            Some((cmd, argv))
        } else {
            None
        }
    }

    /// `$(echo …)` / `$(printf …)` / `$(mktemp …)` as a native Go string:
    /// the builtin's VALUE, not a subprocess. Uses the same options/parts/
    /// format code the statement forms use (`echo_stmt` / `printf_stmt`), so
    /// the two can never disagree; the trailing newlines a command
    /// substitution strips are trimmed exactly like the `capCmd` path does.
    /// `None` keeps the exec path. REFUSE > GUESS: argv the statement forms
    /// cannot render, a non-literal command word, or an `Exec` node with
    /// env/redirects (a `VAR=v` prefix or output redirect changes the value)
    /// all refuse.
    fn echo_printf_capture_value(&mut self, body: &[IrStmt]) -> Option<String> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = body else {
            // The core's `Exec` node shape (flat argv). Env/redirects change
            // the captured value, so only the bare form qualifies — the same
            // condition `body_single_exec_opt` applies.
            if let [IrStmt::Exec {
                cmd: IrExpr::Str(cmd, _),
                args: argv,
                capture: None,
                redirects,
                env,
            }] = body
            {
                if !redirects.is_empty() || !env.is_empty() {
                    return None;
                }
                return self.builtin_capture_value(cmd, argv);
            }
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(argv))) =
            (args.first(), args.get(1))
        else {
            return None;
        };
        self.builtin_capture_value(cmd, argv)
    }

    /// The captured value of one `echo`/`printf` call (see
    /// `echo_printf_capture_value`). `None` for any other command.
    fn echo_printf_value(&mut self, cmd: &str, argv: &[IrExpr]) -> Option<String> {
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let out = match echo_cmd(cmd).unwrap_or(cmd) {
            "echo" => {
                let (items, newline, escape) = Self::echo_options(argv);
                let parts = self.echo_parts(items, escape)?;
                if self.todo != todo {
                    None
                } else {
                    let text = self.parts_str(parts);
                    // The substitution strips trailing newlines, whatever
                    // produced them (the echo terminator or `-e` escapes) —
                    // the same `TrimRight` the `capCmd` path applies.
                    Some(if newline {
                        format!("strings.TrimRight(({text} + \"\\n\"), \"\\n\")")
                    } else {
                        format!("strings.TrimRight({text}, \"\\n\")")
                    })
                }
            }
            "printf" => {
                let Some(fmt_expr) = argv.first() else {
                    return None;
                };
                let fmt = self.printf_fmt(fmt_expr);
                if self.todo != todo {
                    None
                } else {
                    let args: Vec<String> =
                        argv[1..].iter().map(|a| self.expr_str(a)).collect();
                    if self.todo != todo {
                        None
                    } else {
                        let call = if args.is_empty() {
                            format!("bprintfStr({fmt})")
                        } else {
                            format!("bprintfStr({fmt}, {})", args.join(", "))
                        };
                        Some(format!("strings.TrimRight({call}, \"\\n\")"))
                    }
                }
            }
            _ => None,
        };
        if out.is_none() {
            self.sides.truncate(sides);
            self.tmp = tmp;
            // `echo_parts`/`printf_fmt` signal refusal with a TODO marker;
            // that marker belongs to the untaken path, not the output.
            self.todo = todo;
        }
        out
    }

    /// `$(mktemp)` / `$(mktemp -d)` / `$(mktemp template)` as a native Go
    /// string: `os.CreateTemp` / `os.MkdirTemp` create the entry AND yield
    /// its name, exactly what the corpus uses mktemp for (a scratch path),
    /// with no subprocess. A template's trailing `XXX…` run maps to Go's
    /// `*` (both mean "random here"); the directory part must be literal
    /// (Go fails — `""` — when it is missing, just like mktemp).
    /// `None` keeps the exec path. REFUSE > GUESS: `-t`/`-p`/`--tmpdir`
    /// (a different directory), `-u` (dry-run: no entry is created), a
    /// template without a trailing `XXX` run, `X`s outside it, or a second
    /// operand all refuse.
    fn mktemp_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        enum Kind {
            File,
            Dir,
        }
        let (kind, pattern) = match argv {
            [] => (Kind::File, ("".to_string(), "tmp*".to_string())),
            [IrExpr::Str(f, _)] if f == "-d" || f == "--directory" => {
                (Kind::Dir, ("".to_string(), "tmp*".to_string()))
            }
            [IrExpr::Str(t, _)] => (Kind::File, mktemp_pattern(t)?),
            [IrExpr::Str(f, _), IrExpr::Str(t, _)]
                if f == "-d" || f == "--directory" =>
            {
                (Kind::Dir, mktemp_pattern(t)?)
            }
            _ => return None,
        };
        let id = self.new_tmp();
        let (dir, pat) = pattern;
        let (dir, pat) = (Self::go_str(&dir), Self::go_str(&pat));
        match kind {
            Kind::Dir => Some(format!(
                "func() string {{ __mkn{id}, __mke{id} := os.MkdirTemp({dir}, {pat}); \
                 if __mke{id} != nil {{ return \"\" }}; return __mkn{id} }}()"
            )),
            Kind::File => Some(format!(
                "func() string {{ __mkf{id}, __mke{id} := os.CreateTemp({dir}, {pat}); \
                 if __mke{id} != nil {{ return \"\" }}; \
                 __mkn{id} := __mkf{id}.Name(); __mkf{id}.Close(); return __mkn{id} }}()"
            )),
        }
    }

    /// `$(wc -c|-l|-w|-m < f)` as a native Go string: the count, with no
    /// subprocess. `-c` stats the file (bytes), `-l` counts newlines,
    /// `-w` counts words, `-m` counts runes — each `""` when the file
    /// cannot be read, exactly like the capturing helpers' empty-on-error
    /// contract (stderr discarded). `None` keeps the exec path.
    /// REFUSE > GUESS: exactly one flag (no `wc`, no `-cl`, no long
    /// `--bytes`), exactly one stdin-file redirect (no heredoc/herestring
    /// — those are different modes — no second redirect, no env prefix).
    fn wc_capture_value(
        &mut self,
        cmd: &str,
        argv: &[IrExpr],
        stdin_path: &IrExpr,
    ) -> Option<String> {
        if cmd != "wc" {
            return None;
        }
        let [IrExpr::Str(flag, _)] = argv else {
            return None;
        };
        if flag != "-c" && flag != "-l" && flag != "-w" && flag != "-m" {
            return None;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let path = self.expr_str(stdin_path);
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return None;
        }
        let id = self.new_tmp();
        let out = match flag.as_str() {
            // Bytes: the file size. Opened for reading first so
            // permission-denied fails like the shell's redirect does; a
            // directory opens fine but is not a byte count.
            "-c" => format!(
                "func() string {{ __wf{id}, __wfe{id} := os.Open({path}); \
                 if __wfe{id} != nil {{ return \"\" }}; \
                 __ws{id}, __wse{id} := __wf{id}.Stat(); __wf{id}.Close(); \
                 if __wse{id} != nil || __ws{id}.IsDir() {{ return \"\" }}; \
                 return strconv.FormatInt(__ws{id}.Size(), 10) }}()"
            ),
            // Lines: newline bytes (`wc -l` counts `\n`, so a missing
            // trailing newline is not a line — `bytes.Count`, not lines).
            "-l" => format!(
                "func() string {{ __wd{id}, __wde{id} := os.ReadFile({path}); \
                 if __wde{id} != nil {{ return \"\" }}; \
                 return strconv.FormatInt(int64(strings.Count(string(__wd{id}), \"\\n\")), 10) }}()"
            ),
            "-w" => format!(
                "func() string {{ __wd{id}, __wde{id} := os.ReadFile({path}); \
                 if __wde{id} != nil {{ return \"\" }}; \
                 return strconv.FormatInt(int64(len(strings.Fields(string(__wd{id})))), 10) }}()"
            ),
            "-m" => format!(
                "func() string {{ __wd{id}, __wde{id} := os.ReadFile({path}); \
                 if __wde{id} != nil {{ return \"\" }}; \
                 return strconv.FormatInt(int64(utf8.RuneCountInString(string(__wd{id}))), 10) }}()"
            ),
            _ => unreachable!(),
        };
        Some(out)
    }

    /// A capture body's `wc` + stdin-file shape → [`Render::wc_capture_value`].
    /// Accepts the statement `Redirect` form, the `redirect` call form and
    /// the frontend's `builtin(Arrow, redirs)` stage spelling. `None` keeps
    /// the current lowering.
    fn wc_capture_body(&mut self, body: &[IrStmt]) -> Option<String> {
        // Exactly one statement, one stdin-file redirect, no env.
        if let [IrStmt::Redirect { inner, redirects }] = body {
            if redirects.len() != 1 {
                return None;
            }
            let r = &redirects[0];
            if r.mode != "r" || !matches!(r.fd, None | Some(0)) {
                return None;
            }
            let (cmd, argv) = self.wc_body_exec(inner)?;
            return self.wc_capture_value(&cmd, &argv, &r.target.clone());
        }
        if let [IrStmt::Expr(IrExpr::Call { func, args })] = body {
            if func != "redirect" && func != "builtin" {
                return None;
            }
            let (Some(IrExpr::Arrow(inner)), Some(IrExpr::Array(objs))) =
                (args.first(), args.get(1))
            else {
                return None;
            };
            if objs.len() != 1 {
                return None;
            }
            let IrExpr::Object(props) = &objs[0] else {
                return None;
            };
            let (mut fd_ok, mut mode_ok, mut target) = (false, false, None);
            for (k, v) in props {
                match k.as_str() {
                    "fd" => {
                        if let IrExpr::Int(0) = v {
                            fd_ok = true;
                        }
                    }
                    "mode" => {
                        if let IrExpr::Str(s, _) = v {
                            if s == "r" {
                                mode_ok = true;
                            }
                        }
                    }
                    "target" => target = Some(v.clone()),
                    _ => {}
                }
            }
            // A missing fd defaults to stdin for mode `r` (see
            // `native_redirect`), so it qualifies too.
            let missing_fd = !props.iter().any(|(k, _)| k == "fd");
            if !(mode_ok && target.is_some() && (fd_ok || missing_fd)) {
                return None;
            }
            let (cmd, argv) = self.wc_body_exec(inner)?;
            return self.wc_capture_value(&cmd, &argv, &target.unwrap());
        }
        None
    }

    /// The single `wc` command inside a capture body → (cmd, argv items).
    /// Reuses the lone-exec extraction (which drops `2>/dev/null`
    /// wrappers); the caller checks the shape is really `wc`.
    fn wc_body_exec(&mut self, inner: &[IrStmt]) -> Option<(String, Vec<IrExpr>)> {
        let args = lone_exec_args(inner)?;
        let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(argv))) =
            (args.first(), args.get(1))
        else {
            return None;
        };
        if cmd != "wc" {
            return None;
        }
        Some((cmd.clone(), argv.clone()))
    }

    /// `$(date +fmt)` as a native Go string: `time.Now().Format` with the
    /// strftime verbs mapped to Go's layout (`%Y` → `2006`, …) — no
    /// subprocess. `time.Now()` and `date` share the local clock, and the
    /// value carries no trailing newline in either form (the substitution
    /// strips `date`'s). `None` keeps the exec path. REFUSE > GUESS: a bare
    /// `date` (locale-dependent default format), any flag (`-u`, `-d`,
    /// …), a dynamic format, or a verb outside the mapped set all refuse.
    fn date_capture_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        let [one] = argv else {
            return None;
        };
        let fmt = date_format_lit(one)?;
        let layout = date_layout(&fmt)?;
        Some(format!("time.Now().Format({})", Self::go_str(&layout)))
    }

    /// `$(uname -s|-m|-r|-n|-v)` as a native Go string: `syscall.Uname`
    /// fields — no subprocess. `None` keeps the exec path. REFUSE > GUESS:
    /// exactly one flag from the mapped set (bare `uname` is `-s`; anything
    /// else, including `-a` whose multi-field layout has no single-field
    /// form, refuses).
    fn uname_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        let field = match argv {
            [] => "Sysname",
            [IrExpr::Str(f, _)] if f == "-s" || f == "--kernel-name" => "Sysname",
            [IrExpr::Str(f, _)] if f == "-m" || f == "--machine" => "Machine",
            [IrExpr::Str(f, _)] if f == "-r" || f == "--kernel-release" => "Release",
            [IrExpr::Str(f, _)] if f == "-n" || f == "--nodename" => "Nodename",
            [IrExpr::Str(f, _)] if f == "-v" || f == "--kernel-version" => "Version",
            _ => return None,
        };
        let id = self.new_tmp();
        Some(format!(
            "func() string {{ var __un{id} syscall.Utsname; \
             if syscall.Uname(&__un{id}) != nil {{ return \"\" }}; \
             __ub{id} := make([]byte, 0, 65); \
             for _, __uc{id} := range __un{id}.{field} {{ \
             if __uc{id} == 0 {{ break }}; \
             __ub{id} = append(__ub{id}, byte(__uc{id})) }}; \
             return string(__ub{id}) }}()"
        ))
    }

    /// `$(cmd <<< s)` / `$(cmd <<EOF … EOF)` as a native Go string: the
    /// command run with stdin fed from the string, its output captured —
    /// no `bash -c` child and no env literal. The builtin policy is the
    /// same one captures use (echo/printf run as external processes).
    /// `None` keeps the current lowering. Exactly one stdin-string
    /// redirect; anything else keeps the fallback.
    fn stdin_str_capture(&mut self, body: &[IrStmt]) -> Option<String> {
        let [s] = body else {
            return None;
        };
        // Normalize the three wrappings to (inner, redir-objects).
        let (inner, objs): (&[IrStmt], Vec<IrExpr>) = match s {
            IrStmt::Redirect { inner, redirects } => {
                let mut objs = Vec::with_capacity(redirects.len());
                for r in redirects {
                    let mut props: Vec<(String, IrExpr)> = Vec::new();
                    if let Some(fd) = r.fd {
                        props.push(("fd".to_string(), IrExpr::Int(fd as i64)));
                    }
                    props.push((
                        "mode".to_string(),
                        IrExpr::Str(r.mode.clone(), crate::ir::StrStyle::DoubleQuoted),
                    ));
                    props.push(("target".to_string(), r.target.clone()));
                    objs.push(IrExpr::Object(props));
                }
                (inner, objs)
            }
            IrStmt::Expr(IrExpr::Call { func, args })
                if func == "redirect" || func == "builtin" =>
            {
                let (Some(IrExpr::Arrow(inner)), Some(IrExpr::Array(objs))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                // The plain `builtin cmd argv…` call is not this shape.
                if func == "builtin"
                    && !matches!(args.first(), Some(IrExpr::Arrow(_)))
                {
                    return None;
                }
                (inner, objs.clone())
            }
            _ => return None,
        };
        if objs.len() != 1 {
            return None;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let fail = |r: &mut Render| {
            r.sides.truncate(sides);
            r.tmp = tmp;
            r.todo = todo;
        };
        let (cmd, argv) = match self.body_single_exec_opt(inner, true) {
            Some(x) if self.todo == todo => x,
            _ => {
                fail(self);
                return None;
            }
        };
        let text = match self.native_redirect(&objs[0]) {
            Some(NativeRedir::StdinStr(t)) if self.todo == todo => t,
            _ => {
                fail(self);
                return None;
            }
        };
        if self.todo != todo {
            fail(self);
            return None;
        }
        // `$(cat <<EOF … EOF)` / `$(cat <<< s)`: `cat` with no FILE operand
        // reproduces its stdin byte for byte, and the capture contract is
        // that same text with the trailing newlines trimmed — so the value
        // is the text we already hold. Spawning a process to copy it is pure
        // marshalling (`strings.NewReader` in, `TrimRight` out).
        if lone_cat_stdin(inner) {
            return Some(format!("strings.TrimRight({text}, \"\\n\")"));
        }
        let call = Self::exec_cmd_call(&cmd, &argv);
        let id = self.new_tmp();
        Some(format!(
            "func() string {{ __hs{id}_c := {call}; \
             __hs{id}_c.Stdin = strings.NewReader({text}); \
             __hs{id}_c.Stderr = io.Discard; \
             __hs{id}_o, __hs{id}_e := __hs{id}_c.Output(); \
             _ = __hs{id}_e; \
             if len(__hs{id}_o) == 0 {{ return \"\" }}; \
             return strings.TrimRight(string(__hs{id}_o), \"\\n\") }}()"
        ))
    }

    /// `x=$(cmd <<< s)` / `x=$(cmd <<EOF … EOF)` as an `Exec`-statement
    /// capture (see `stdin_str_capture`). Emits the assignment directly;
    /// `true` when handled.
    fn stdin_str_exec_capture(
        &mut self,
        cmd: &IrExpr,
        args: &[IrExpr],
        capture: &Option<String>,
        redirects: &[IrExpr],
        env: &[(String, IrExpr)],
    ) -> bool {
        if !env.is_empty() || redirects.len() != 1 {
            return false;
        }
        let Some(var) = capture else {
            return false;
        };
        // The single exec, under the capture builtin policy.
        let mut call_args = vec![cmd.clone(), IrExpr::Array(args.to_vec())];
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let (c, argv) = match self.body_single_exec_opt(
            &[IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: std::mem::take(&mut call_args),
            })],
            true,
        ) {
            Some(x) if self.todo == todo => x,
            _ => {
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
                return false;
            }
        };
        let text = match self.native_redirect(&redirects[0]) {
            Some(NativeRedir::StdinStr(t)) if self.todo == todo => t,
            _ => {
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
                return false;
            }
        };
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return false;
        }
        let sync = self.sync_inline(var);
        // Same `cat`-copies-stdin rule as `stdin_str_capture`: the value is
        // the text the redirect already holds.
        if lone_cat_stdin(&[IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![cmd.clone(), IrExpr::Array(args.to_vec())],
        })]) {
            self.flush_sides();
            self.mark_str_written(var);
            let m = self.go_ident(var);
            self.emit(&format!("{m} = strings.TrimRight({text}, \"\\n\");{sync}"));
            return true;
        }
        let call = Self::exec_cmd_call(&c, &argv);
        let id = self.new_tmp();
        self.flush_sides();
        self.mark_str_written(var);
        let m = self.go_ident(var);
        self.emit(&format!(
            "{m} = func() string {{ __hs{id}_c := {call}; \
             __hs{id}_c.Stdin = strings.NewReader({text}); \
             __hs{id}_c.Stderr = io.Discard; \
             __hs{id}_o, __hs{id}_e := __hs{id}_c.Output(); \
             if __hs{id}_e != nil && len(__hs{id}_o) == 0 {{ return \"\" }}; \
             return strings.TrimRight(string(__hs{id}_o), \"\\n\") }}();{sync}"
        ));
        true
    }

    /// `$(hostname)` as a native Go string: `os.Hostname` — no subprocess.
    /// `None` keeps the exec path (any operand: `-f`/`-s` select different
    /// names, which a bare call cannot reproduce).
    fn hostname_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        if !argv.is_empty() {
            return None;
        }
        let id = self.new_tmp();
        Some(format!(
            "func() string {{ __hn{id}, __hne{id} := os.Hostname(); \
             if __hne{id} != nil {{ return \"\" }}; return __hn{id} }}()"
        ))
    }

    /// `$(sha256sum f)` / `$(sha512sum f)` as a native Go string: the hex
    /// digest plus the two-space-plus-name trailer coreutils prints —
    /// `crypto/sha256` + `hex`, not a child. `""` when the file cannot be
    /// read, like the capturing helpers' empty-on-error contract.
    /// `None` keeps the exec path. REFUSE > GUESS: exactly one operand, no
    /// flags (`-b` marks binaries with `*`, `--check` verifies instead of
    /// hashing, `-` reads stdin which has no single-expression form here).
    fn checksum_value(&mut self, cmd: &str, argv: &[IrExpr]) -> Option<String> {
        let [one] = argv else {
            return None;
        };
        if matches!(one, IrExpr::Str(s, _) if s == "-" || s.starts_with('-')) {
            return None;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let path = self.expr_str(one);
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return None;
        }
        let id = self.new_tmp();
        let sum = if cmd == "sha256sum" {
            "sha256.Sum256"
        } else {
            "sha512.Sum512"
        };
        Some(format!(
            "func() string {{ __cd{id}, __cde{id} := os.ReadFile({path}); \
             if __cde{id} != nil {{ return \"\" }}; \
             __ch{id} := {sum}(__cd{id}); \
             return hex.EncodeToString(__ch{id}[:]) + \"  \" + {path} }}()"
        ))
    }

    /// `command -v NAME`'s NAME operand, or `None` for any other `command`
    /// form. Only `-v` (print the path, status = found) lowers natively:
    /// `command ls …` runs the builtin bypass and `-V` prints a
    /// description — neither has a single-call counterpart here. A
    /// dash-word NAME keeps the exec (it would resolve as a flag).
    fn command_v_name(argv: &[IrExpr]) -> Option<&IrExpr> {
        let [one, two] = argv else {
            return None;
        };
        let IrExpr::Str(flag, _) = one else {
            return None;
        };
        if flag != "-v" {
            return None;
        }
        if matches!(two, IrExpr::Str(s, _) if s.starts_with('-')) {
            return None;
        }
        Some(two)
    }

    /// `$(command -v NAME)` as a native Go string: the resolved path, or
    /// `""` when there is none — `exec.LookPath`, not a child. `None`
    /// keeps the exec path (see `command_v_name`).
    fn command_v_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        let name_expr = Self::command_v_name(argv)?;
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let name = self.expr_str(name_expr);
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return None;
        }
        let id = self.new_tmp();
        Some(format!(
            "func() string {{ __cl{id}, __cle{id} := exec.LookPath({name}); \
             if __cle{id} != nil {{ return \"\" }}; return __cl{id} }}()"
        ))
    }

    /// One captured command's native VALUE (`echo`/`printf`/`mktemp`/`wc`/
    /// `date`/`command`/`cat`/`readlink`/`pwd`/`dirname`/`basename`), or
    /// `None` when the command is anything else. The caller extracts the
    /// (cmd, argv) pair; the shapes mirror `body_single_exec_opt` (a lone
    /// `exec`/`builtin` call, or the core's bare `Exec` node).
    fn builtin_capture_value(&mut self, cmd: &str, argv: &[IrExpr]) -> Option<String> {
        if cmd == "mktemp" {
            return self.mktemp_value(argv);
        }
        if cmd == "uname" {
            return self.uname_value(argv);
        }
        if cmd == "date" {
            return self.date_capture_value(argv);
        }
        if cmd == "hostname" {
            return self.hostname_value(argv);
        }
        if cmd == "sha256sum" || cmd == "sha512sum" {
            return self.checksum_value(cmd, argv);
        }
        if cmd == "command" {
            return self.command_v_value(argv);
        }
        if cmd == "cat" || cmd == "readlink" || cmd == "pwd" || cmd == "dirname" || cmd == "basename" {
            return self.path_capture_value(cmd, argv);
        }
        self.echo_printf_value(cmd, argv)
    }

    /// `$(cat f)` / `$(readlink [-f|-e] f)` / `$(pwd)` / `$(dirname p)` /
    /// `$(basename p [suf])` as native Go strings: `os.ReadFile`,
    /// `filepath.EvalSymlinks`/`os.Readlink`, `os.Getwd`, `filepath.Dir`,
    /// `filepath.Base` — the stdlib calls with the same empty-on-error
    /// contract the capturing helpers have (stderr discarded, `""` on
    /// failure). `None` keeps the exec path. REFUSE > GUESS: flags the
    /// stdlib cannot reproduce (`cat -n`, `readlink -m`, any `pwd`/`mkdir`
    /// operand, `-` for stdin, multiple `cat` operands whose partial
    /// output on a missing middle file has no single-expression form).
    fn path_capture_value(&mut self, cmd: &str, argv: &[IrExpr]) -> Option<String> {
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let out = match cmd {
            "pwd" => {
                if !argv.is_empty() {
                    None
                } else {
                    let id = self.new_tmp();
                    Some(format!(
                        "func() string {{ __pwd{id}, __pwde{id} := os.Getwd(); \
                         if __pwde{id} != nil {{ return \"\" }}; return __pwd{id} }}()"
                    ))
                }
            }
            "cat" => {
                let [one] = argv else { return None };
                if matches!(one, IrExpr::Str(s, _) if s == "-" || s.starts_with('-')) {
                    None
                } else {
                    let path = self.expr_str(one);
                    if self.todo != todo {
                        None
                    } else {
                        let id = self.new_tmp();
                        Some(format!(
                            "func() string {{ __cd{id}, __cde{id} := os.ReadFile({path}); \
                             if __cde{id} != nil {{ return \"\" }}; \
                             return strings.TrimRight(string(__cd{id}), \"\\n\") }}()"
                        ))
                    }
                }
            }
            "readlink" => {
                // `readlink f`: the link's target (`os.Readlink`, `""`
                // unless it IS a link). `readlink -f|-e f`: the canonical
                // path (`filepath.EvalSymlinks`, `""` unless it resolves).
                // `-m` (no existence requirement) has no counterpart here.
                let (canonical, path_expr) = match argv {
                    [] => return None,
                    [IrExpr::Str(f, _)] if f == "-f" || f == "-e" => return None,
                    [one] => (false, one),
                    [IrExpr::Str(f, _), one] if f == "-f" || f == "-e" => (true, one),
                    _ => return None,
                };
                if matches!(path_expr, IrExpr::Str(s, _) if s == "-") {
                    return None;
                }
                let path = self.expr_str(path_expr);
                if self.todo != todo {
                    None
                } else {
                    let id = self.new_tmp();
                    let call = if canonical {
                        format!("filepath.EvalSymlinks({path})")
                    } else {
                        format!("os.Readlink({path})")
                    };
                    Some(format!(
                        "func() string {{ __rl{id}, __rle{id} := {call}; \
                         if __rle{id} != nil {{ return \"\" }}; \
                         return strings.TrimRight(__rl{id}, \"\\n\") }}()"
                    ))
                }
            }
            "dirname" => {
                let [one] = argv else { return None };
                // A lone `-` is an OPERAND to dirname (prints `-`), but
                // `filepath.Dir("-")` is `"."` — refuse it with the flags.
                if matches!(one, IrExpr::Str(s, _) if s.starts_with('-')) {
                    None
                } else {
                    let path = self.expr_str(one);
                    if self.todo != todo {
                        None
                    } else {
                        Some(format!("filepath.Dir({path})"))
                    }
                }
            }
            "basename" => {
                // `basename p` / `basename p suf`: `filepath.Base` plus an
                // optional `TrimSuffix`. An empty path is `""` in bash but
                // `"."` from `filepath.Base`, so the guard reproduces it.
                let (path_expr, suf) = match argv {
                    [one] => (one, None),
                    [one, IrExpr::Str(suf, _)] => (one, Some(suf.clone())),
                    _ => return None,
                };
                // A lone `-` is an operand (`-`); longer dash-words are
                // flags (`--` ends them, but then the path is missing).
                if matches!(path_expr, IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1) {
                    return None;
                }
                let path = self.expr_str(path_expr);
                if self.todo != todo {
                    None
                } else {
                    let id = self.new_tmp();
                    let base = format!("filepath.Base({path})");
                    let val = match suf {
                        Some(s) => format!(
                            "strings.TrimSuffix({base}, {})",
                            Self::go_str(&s)
                        ),
                        None => base,
                    };
                    Some(format!(
                        "func() string {{ if {path} == \"\" {{ return \"\" }}; return {val} }}()"
                    ))
                }
            }
            _ => None,
        };
        if out.is_none() {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
        }
        out
    }

    /// String interpolation: "hello $name" → fmt.Sprintf(...) (string).
    fn interpolate(&mut self, parts: &[InterpPart]) -> String {
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => {
                    // escape % at push time; %v markers stay raw
                    fmt.push_str(&s.replace('%', "%%"));
                }
                InterpPart::Expr(x) => {
                    fmt.push_str("%v");
                    args.push(self.expr_any(x));
                }
            }
        }
        if args.is_empty() {
            Self::go_str(&fmt)
        } else {
            format!("fmt.Sprintf({}, {})", Self::go_str(&fmt), args.join(", "))
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
                    // `$(( … ))` nested inside the expression is a nested
                    // arithmetic expansion, which this tokenizer does not
                    // model — refuse it rather than mis-read the inner `(`.
                    if i + 2 < b.len() && b[i + 2] == '(' {
                        return None;
                    }
                    // `$( … )` — bash substitutes the command FIRST and
                    // evaluates its output as an operand, so it is a single
                    // atom here; `MA::CmdSub` carries its TEXT.
                    let end = arith_cmdsub_end(&b, i + 1)?;
                    out.push((b[i..=end].iter().collect(), false));
                    i = end + 1;
                    continue;
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
            if c == '"' {
                // bash removes double quotes during expansion, so the
                // arithmetic evaluator sees their CONTENT (`$(( "5" + 1 ))`
                // is 6). Splice the content in as one operand; a quoted
                // operand that is not a single arithmetic atom (`"a b"`)
                // stays outside the lowable subset.
                let mut j = i + 1;
                while j < b.len() && b[j] != '"' {
                    j += 1;
                }
                if j >= b.len() {
                    return None;
                }
                let inner: String = b[i + 1..j].iter().collect();
                if !arith_quoted_operand(&inner) {
                    return None;
                }
                out.push((inner, false));
                i = j + 1;
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
            if "+-*/%<>&|^!~=?:()[]".contains(c) {
                out.push((c.to_string(), true));
                i += 1;
                continue;
            }
            return None;
        }
        Some(out)
    }

    /// Parse + render an arith string → (side stmts, value expr).
    /// Returns None for anything outside the lowable subset; `arith_fail`
    /// then says WHY, which decides between reproducing bash's arithmetic
    /// error and keeping the refusal loud.
    fn arith_str(&mut self, s: &str) -> Option<(Vec<String>, String)> {
        let toks = self.arith_tokens(s)?;
        let mut p = MAParser { toks, i: 0 };
        let ast = p.expr()?;
        if p.i != p.toks.len() {
            return None;
        }
        self.ma_render(&ast)
    }

    /// Why `arith_str` refused `s`. The tokenizer and the parser touch no
    /// render state, so calling this alongside `arith_str` is free.
    fn arith_fail(&self, s: &str) -> ArithFail {
        let Some(toks) = self.arith_tokens(s) else {
            return ArithFail::Unsupported;
        };
        let mut p = MAParser { toks, i: 0 };
        match p.expr() {
            // Tokens parse and are fully consumed, so the refusal came from
            // the RENDERER (a shape outside the lowable subset) — not from
            // bash's grammar. Never claim bash's error here.
            Some(_) if p.i == p.toks.len() => ArithFail::Unsupported,
            // Tokens are arithmetic but do not form one expression (`echo
            // "test"` — two juxtaposed operands; `a[i][j]` — a subscript on
            // a subscript). bash rejects exactly these.
            _ => ArithFail::Invalid,
        }
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
                self.mark_arr(name);
                let m = self.ident_of(name);
                Some((Vec::new(), format!("arrLen({m})")))
            }
            MA::StrLen(name) => {
                // `${#name}` is a length over the value's TEXT, so the
                // operand is the string form — `arith_var` here produced
                // `runeLen(s2i(flags))` (int64 into a string parameter),
                // which does not compile. `getvar_str` applies the same
                // decl/type rules as every other string context.
                let v = self.getvar_str(name.strip_prefix('$').unwrap_or(name));
                Some((Vec::new(), format!("runeLen({v})")))
            }
            MA::ArrIdx(name, key) => {
                self.mark_arr(name);
                self.ident_of(name);
                let (ks, kv) = self.ma_render(key)?;
                Some((ks, format!("s2i(arrIdx({}, {kv}))", self.ident_of(name))))
            }
            MA::CmdSub(text) => {
                let v = self.arith_cmdsub(text)?;
                Some((Vec::new(), format!("s2i({v})")))
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

    /// A `$( … )` used as an arithmetic operand.
    ///
    /// bash performs the command substitution BEFORE it evaluates the
    /// expression, and the substitution's output then stands in the text as
    /// an operand — so this is the ordinary command-substitution lowering
    /// (`capture_str`) with the same string→int coercion the other string
    /// operands use (`s2i`, the `arrIdx` element read).
    ///
    /// The payload is raw shell TEXT rather than IR: the core does not
    /// descend into the arithmetic string, so the inner command is
    /// re-parsed here with the same frontend that produced the program and
    /// wrapped in the `Arrow` shape `capture_str` consumes. That makes the
    /// in-arith substitution lower EXACTLY like the same command written
    /// outside `$(( … ))`. REFUSE > GUESS: unparseable text, or a capture
    /// `capture_str` declines, returns `None` and the call site keeps its
    /// refusal.
    fn arith_cmdsub(&mut self, text: &str) -> Option<String> {
        let mut src = text.to_string();
        src.push('\n');
        let commands = crate::Parser::new(&src).parse().ok()?;
        if commands.is_empty() {
            return None;
        }
        let prog = crate::shir::ast_to_ir(&commands);
        if prog.stmts.is_empty() {
            return None;
        }
        self.capture_str(&IrExpr::Arrow(prog.stmts), false)
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
                // a user-function call in EXPRESSION position. The
                // positional-param convention reads the shared `fArgs`, and
                // Go has no statement-expression, so the
                // set-call-restore sequence is hoisted into a side-effect
                // statement (flushed by the enclosing statement) and the
                // call's string result is bound to a temporary.
                let nm = args.first().and_then(|a| match a {
                    IrExpr::Str(nm, _) => Some(nm.clone()),
                    _ => None,
                });
                let Some(nm) = nm.filter(|n| self.functions.contains(n)) else {
                    return self.sh2_stub("fnValue");
                };
                self.mark_read(&nm);
                self.need_fargs = true;
                let m = self.go_ident(&nm);
                let call_args: Vec<String> = args
                    .get(1)
                    .and_then(|a| match a {
                        IrExpr::Array(elems) => {
                            Some(elems.iter().map(|e| self.expr_str(e)).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let t = self.new_tmp();
                self.sides.push(format!(
                    "fArgs = []string{{{}}}; {t} := {m}(); fArgs = nil;",
                    call_args.join(", ")
                ));
                t
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
                    // Same narrowed-env fallback as the bool form above.
                    let text = format!("[ {s} ]");
                    let env = self.env_lit_for(Some(&text));
                    return format!("(redirRun({}, {env}) == 0)", Self::go_str(&text));
                }
                self.sh2_stub("test")
            }
            "exec" => self.exec_expr(args),
            "pipeline" => self.pipeline_expr(args),
            "redirect" => self.redirect_expr(args),
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
                let m = self.ident_of(&name);
                // An assoc read (`matrix[$i,$j]`, `map[$k]`) keys the map
                // by the key's STRING value — never an arrIdx into []any.
                if self.is_assoc(&name) {
                    let key = match args.get(1) {
                        Some(IrExpr::Int(i)) => Self::go_str(&i.to_string()),
                        Some(IrExpr::Str(s, _)) => self.expand_str(s),
                        Some(other) => self.expr_str(other),
                        None => return self.sh2_stub("arrayIndex"),
                    };
                    return format!("s2s(assocIdx({m}, {key}))");
                }
                self.mark_arr(&name);
                let key = match args.get(1) {
                    Some(IrExpr::Int(i)) => i.to_string(),
                    Some(IrExpr::Str(s, _)) => {
                        if let Some((_, v)) = self.arith_str(s) {
                            v
                        } else {
                            format!("s2i({})", self.expand_str(s))
                        }
                    }
                    Some(other) => self.expr_num(other),
                    None => return self.sh2_stub("arrayIndex"),
                };
                format!("s2s(arrIdx({m}, {key}))")
            }
            "arrayItems" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return self.sh2_stub("arrayItems"),
                };
                self.mark_arr(&name);
                self.need_vars = true;
                format!("strings.Join(pList(\"\", {}), \" \")", Self::go_str(&name))
            }
            "arrayLen" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return self.sh2_stub("arrayLen"),
                };
                let m = self.ident_of(&name);
                self.mark_arr(&name);
                // `${#arr[@]}` in a string context: the count rendered as a
                // Go string directly. `s2s(arrLen(m))` boxed an int64 into
                // `any` and let fmt `%v` it back out — and a caller that
                // ALSO coerced (join/printf) produced `s2s(s2s(…))`.
                format!("strconv.FormatInt(arrLen({m}), 10)")
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
                    let sep = if func == "and" { " && " } else { " || " };
                    let text = texts.join(sep);
                    let env = self.env_lit_for(Some(&text));
                    return format!("redirRun({}, {env})", Self::go_str(&text));
                }
                self.sh2_stub(func)
            }
            "arith" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some((_, v)) = self.arith_str(s) {
                        return v;
                    }
                }
                self.sh2_stub("arith")
            }
            // `grep -o PAT` (transforms/grep_o.rs): the native Go regexp
            // engine, no runtime helper — `grepMatches` yields the array of
            // matches, and the string value is them joined one per line.
            "grepMatches" => match self.grep_matches_value(args) {
                Some(v) => v,
                None => self.sh2_stub("grepMatches"),
            },
            // The test-lowering transform's glob-affix primitives
            // (`[[ $s == *.txt ]]` → strHasSuffix): the stdlib strings
            // predicates, which already agree with bash on the empty affix
            // (`*""` / `""*` / `*""*` always match).
            "strHasPrefix" | "strHasSuffix" | "contains" => {
                let (Some(subject), Some(affix)) = (args.first(), args.get(1)) else {
                    return self.sh2_stub(func);
                };
                let s = self.expr_str(subject);
                let p = self.expr_str(affix);
                let f = match func {
                    "strHasPrefix" => "HasPrefix",
                    "strHasSuffix" => "HasSuffix",
                    _ => "Contains",
                };
                format!("strings.{f}({s}, {p})")
            }
            // `typeset -i` / `-l` / `-u` assignment wrappers: the frontend
            // hands the attribute's coercion as a Call around the RHS.
            //   int(x) — the integer value. Under `typeset -i` the RHS text
            //     is ARITHMETIC (`n=n+1` means `n = n + 1`), which the
            //     frontend already wrapped in `arith`, so the value is that
            //     int64.
            //   lc(x)/uc(x) — bash stores the case-folded string.
            "int" => match args.first() {
                Some(IrExpr::Call { func: f, args: a }) if f == "arith" => self.call("arith", a),
                Some(a) => format!("s2i({})", self.expr_str(a)),
                None => self.sh2_stub("int"),
            },
            "lc" | "uc" => match args.first() {
                Some(a) => {
                    let v = self.expr_str(a);
                    if func == "lc" {
                        format!("strings.ToLower({v})")
                    } else {
                        format!("strings.ToUpper({v})")
                    }
                }
                None => self.sh2_stub(func),
            },
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
        // Builtins with no external executable (or none worth exec'ing)
        // lower to their status directly — exec'ing them would fail outright
        // (`true` has no binary behind it) or fork a child for a constant.
        if let Some(IrExpr::Str(cmd, _)) = args.first() {
            if cmd == "true" || cmd == ":" {
                return "0".to_string();
            }
            if cmd == "false" {
                return "1".to_string();
            }
            if cmd == "test" {
                let argv: &[IrExpr] = match args.get(1) {
                    Some(IrExpr::Array(items)) => items.as_slice(),
                    _ => &[],
                };
                // `test` with no args is false (status 1).
                if argv.is_empty() {
                    return "1".to_string();
                }
                let todo = self.todo;
                let sides = self.sides.len();
                let tmp = self.tmp;
                if let Some(c) = self.test_argv_bool(argv) {
                    if self.todo == todo {
                        return format!("b2i(!({c}))");
                    }
                }
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
            }
            // `command -v NAME`: status = found (`exec.LookPath`).
            if cmd == "command" {
                let argv: &[IrExpr] = match args.get(1) {
                    Some(IrExpr::Array(items)) => items.as_slice(),
                    _ => &[],
                };
                if let Some(name_expr) = Self::command_v_name(argv) {
                    let todo = self.todo;
                    let sides = self.sides.len();
                    let tmp = self.tmp;
                    let name = self.expr_str(name_expr);
                    if self.todo == todo {
                        let id = self.new_tmp();
                        // An IIFE status: the lookup prints nothing here
                        // (the statement form prints); the value is 0/1.
                        return format!(
                            "func() int64 {{ if _, __cle{id} := exec.LookPath({name}); \
                             __cle{id} != nil {{ return 1 }}; return 0 }}()"
                        );
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
            }
        }
        // `grep …` in status position (`if grep -q PAT FILE`, `x && grep …`):
        // the same native scan the statement path uses, as an IIFE status.
        if matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "grep") {
            let argv: &[IrExpr] = match args.get(1) {
                Some(IrExpr::Array(items)) => items.as_slice(),
                _ => &[],
            };
            let todo = self.todo;
            let sides = self.sides.len();
            let tmp = self.tmp;
            if let Some(v) = self.grep_expr(argv) {
                if self.todo == todo {
                    return v;
                }
            }
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
        }
        if let Some((cmd, argv)) = self.exec_cmd_argv(args) {
            let call = Self::exec_cmd_call(&cmd, &argv);
            return format!("runCmd({call})");
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
        for it in items.iter() {
            if let IrExpr::Call { func, args } = it {
                if func == "brace" {
                    let expanded = self.brace_expand(args)?;
                    for s in expanded {
                        out.push(Self::go_str(&s));
                    }
                    continue;
                }
            }
            if let IrExpr::Str(s, style) = it {
                // Decide on the RAW text: the `\x01SH2GLOB\x01` marker is
                // the core's authoritative "expand this word" flag, and it
                // lives in the prefix {@link strip_sh2glob} removes.
                let glob = str_expands_glob(s, style);
                let s = strip_sh2glob(s);
                // The shell expands a word only where its metacharacter is
                // UNQUOTED (`grep -c ".*"` is one regex argument; `*.txt` is
                // a glob splice) — see `str_expands_glob`. The ARGUMENT keeps
                // its text untouched either way: the escape sequence `\n`
                // inside a quoted word survives expansion, so stripping
                // quotes here would eat the backslash and turn echo's `-e`
                // newlines into literal `n`s.
                if glob {
                    // A glob splices its matches into the argv, as a spread
                    // element. Go allows a `…` spread only in FINAL position
                    // of a call or composite literal, so a mid-list glob is
                    // not expressible by `join(", ")` alone — `argv_slice`
                    // folds the element list through `append` instead. A
                    // glob in ANY position therefore stays native; it is no
                    // longer a reason to fall back to shell text.
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

    /// A Go `[]string` EXPRESSION for argv/stage parts. A part ending in
    /// `...` is a slice spread (`globArgs("*")...`); because Go legalizes a
    /// spread only in final position, any spread folds the whole list
    /// through `append`:
    ///   ["grep", "globArgs(*)..."]        -> append([]string{"grep"}, globArgs("*")...)
    ///   ["ls", "globArgs(*)...", ".sh"]   -> append(append([]string{"ls"}, globArgs("*")...), ".sh")
    /// so a NON-final glob (`ls * .sh`) is expressible instead of refused.
    fn argv_slice(parts: &[String]) -> String {
        if !parts.iter().any(|p| p.ends_with("...")) {
            return format!("[]string{{{}}}", parts.join(", "));
        }
        let mut acc = String::new();
        for p in parts {
            acc = match acc.as_str() {
                "" if !p.ends_with("...") => format!("[]string{{{p}}}"),
                "" => format!("append([]string{{}}, {p})"),
                _ => format!("append({acc}, {p})"),
            };
        }
        acc
    }

    /// `exec.Command(cmd, argv…)`. A spread anywhere in the argv is
    /// materialized into a slice and spread into the call, which is the one
    /// place Go does allow it:
    ///   exec.Command("ls", append(append([]string{}, globArgs("*")...), ".sh")...)
    fn exec_cmd_call(cmd: &str, argv: &[String]) -> String {
        if argv.iter().any(|p| p.ends_with("...")) {
            return format!("exec.Command({cmd}, {}...)", Self::argv_slice(argv));
        }
        let mut parts = vec![cmd.to_string()];
        parts.extend(argv.iter().cloned());
        format!("exec.Command({})", parts.join(", "))
    }

    /// One pipeline stage as a Go `[]string` EXPRESSION, for a
    /// composite-literal context (`runPipe([][]string{ … })`) where Go
    /// forbids a `…` spread. `argv_slice` folds it through `append`:
    /// `append([]string{"grep"}, globArgs("*.txt")...)`.
    fn stage_lit(&mut self, st: &[String]) -> String {
        Self::argv_slice(st)
    }

    /// A pipeline's stages as Go `[]string` part lists, or `None` when any
    /// stage is not a single execut-able command (a redirect, a subshell, a
    /// loop …) — then the caller keeps the `bash -c` reconstruction.
    ///
    /// Builtins are admitted (`allow_builtin`): every stage becomes a real
    /// process here, which is what removes the `bash -c` child AND its env
    /// literal. See `native_stage_builtin` for which builtins qualify.
    fn pipeline_stages(&mut self, args: &[IrExpr]) -> Option<Vec<Vec<String>>> {
        let mut stages: Vec<Vec<String>> = Vec::new();
        if let Some(IrExpr::Array(items)) = args.first() {
            for it in items {
                if let IrExpr::Arrow(body) = it {
                    if let Some((cmd, argv)) = self.body_single_exec_opt(body, true) {
                        let mut st = vec![cmd];
                        st.extend(argv);
                        stages.push(st);
                        continue;
                    }
                }
                return None;
            }
        }
        if stages.is_empty() {
            return None;
        }
        Some(stages)
    }

    /// `pipeline` Call → runPipe or bash -c reconstruction. Value = status.
    ///
    /// Stage-level stdio (`( b > f )` as the last stage, `2>/dev/null`
    /// anywhere) welds into the chain instead of refusing it: without wires
    /// the value is a `runPipe` call, with wires a status-IIFE over the
    /// same welded emission the statement form uses (see `pipeline_chain`).
    fn pipeline_expr(&mut self, args: &[IrExpr]) -> String {
        if let Some(stages) = self.pipeline_stages(args) {
            let mut parts: Vec<String> = Vec::new();
            for st in &stages {
                parts.push(self.stage_lit(st));
            }
            return format!("runPipe([][]string{{{}}})", parts.join(", "));
        }
        {
            let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
            let bodies: Option<Vec<&[IrStmt]>> = match args.first() {
                Some(IrExpr::Array(items)) => {
                    let mut out = Vec::with_capacity(items.len());
                    let mut ok = true;
                    for it in items {
                        match it {
                            IrExpr::Arrow(body) => out.push(&body[..]),
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        Some(out)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(bodies) = bodies {
                if let Some((stagess, fi, lo, le, dup)) =
                    self.pipeline_chain(&bodies, Vec::new())
                {
                    if self.todo == todo
                        && (fi.is_some() || lo.is_some() || le.is_some() || dup)
                    {
                        if let Some(v) = self.welded_value(stagess, fi, lo, le, dup) {
                            if self.todo == todo {
                                return v;
                            }
                        }
                    }
                }
            }
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
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
                // Hoist heredoc bodies past the whole pipeline (a body
                // inline in a stage breaks at the following ` | `).
                let mut heads = Vec::new();
                let mut tails = String::new();
                for p in texts {
                    let (h, b) = split_heredoc_tail(&p);
                    heads.push(h);
                    tails.push_str(&b);
                }
                let joined = heads.join(" | ");
                let text = format!("{joined}{tails}");
                let env = self.env_lit_for(Some(&text));
                return format!("redirRun({}, {env})", Self::go_str(&text));
            }
        }
        self.sh2_stub("pipeline")
    }

    /// `$(cmd | cmd …)` — the pipeline's CAPTURE form. Same stage analysis
    /// as `pipeline_expr`, but the stages' stdout is READ instead of
    /// inherited, so the substitution yields a Go string with no shell
    /// reconstruction and no env literal.
    fn pipeline_capture(&mut self, args: &[IrExpr], native: bool) -> Option<String> {
        let stages = self.pipeline_stages(args)?;
        let mut parts: Vec<String> = Vec::new();
        for st in &stages {
            parts.push(self.stage_lit(st));
        }
        // `native` = the value already needs no trailing-newline strip.
        let f = if native { "capPipeRaw" } else { "capPipe" };
        Some(format!("{f}([][]string{{{}}})", parts.join(", ")))
    }

    /// `pipeline_capture` for the statement `Pipeline` NODE's stage bodies
    /// (`stages: Vec<Vec<IrStmt>>`) instead of the call form's Arrow list.
    /// Refuses — leaving the render state untouched — the moment a stage is
    /// not a single plain command, so the caller keeps its fallback.
    fn pipeline_capture_nodes(
        &mut self,
        stages: &[Vec<IrStmt>],
        native: bool,
    ) -> Option<String> {
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let mut parts: Vec<String> = Vec::with_capacity(stages.len());
        for stage in stages {
            let got = self.body_single_exec_opt(stage, true);
            let (cmd, argv) = match got {
                Some(x) if self.todo == todo => x,
                _ => {
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                    return None;
                }
            };
            let mut s = vec![cmd];
            s.extend(argv);
            parts.push(self.stage_lit(&s));
        }
        if parts.is_empty() {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return None;
        }
        let f = if native { "capPipeRaw" } else { "capPipe" };
        Some(format!("{f}([][]string{{{}}})", parts.join(", ")))
    }

    /// The stage bodies of a grouped command (`( a | b )`, `( while … )`):
    /// a statement `Pipeline`/`Subshell` node, or an Arrow holding exactly
    /// one such expression statement. `None` for anything else.
    fn group_stage_bodies(inner: &[IrStmt]) -> Option<Vec<Vec<String>>> {
        let stages: Vec<&IrStmt> = match inner {
            [IrStmt::Pipeline {
                stages,
                capture: None,
                ..
            }] => stages.iter().flatten().collect(),
            [IrStmt::Subshell(body)] | [IrStmt::Block(body)] => {
                return Self::group_stage_bodies(body)
            }
            [IrStmt::Expr(IrExpr::Call { func, args })] if func == "pipeline" => {
                let Some(IrExpr::Array(items)) = args.first() else {
                    return None;
                };
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    let IrExpr::Arrow(body) = it else {
                        return None;
                    };
                    out.extend(Self::group_stage_bodies(body)?);
                }
                return Some(out);
            }
            [IrStmt::Expr(e)] => {
                return Self::stage_text_of_expr(e).map(|st| vec![st])
            }
            _ => return None,
        };
        // Each stage becomes shell text; a construct the argv form cannot
        // name declines the whole group (REFUSE > GUESS).
        let mut out = Vec::with_capacity(stages.len());
        for st in stages {
            // A loop or any other statement-shaped stage keeps its generic
            // statement text (the `redirect`-wrapped `exec` the frontend
            // builds around `while read` comes out as the bare command);
            // anything the text reconstruction cannot express declines.
            match st {
                IrStmt::Expr(e) => out.push(Self::stage_text_of_expr(e)?),
                other => out.push(Self::stage_text_of_stmt(other)?),
            }
        }
        Some(out)
    }

    /// A non-command statement → its bare shell text (`while read …; do …;
    /// done`) for a pipeline stage. Only the generic statement-text forms
    /// that carry NO redirections of their own are admitted.
    fn stage_text_of_stmt(st: &IrStmt) -> Option<Vec<String>> {
        let text = match st {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "redirect" => {
                let (Some(IrExpr::Arrow(inner)), Some(IrExpr::Array(objs))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                // One stdin-file feed, and nothing else: that is the group's
                // own feed, which the chain wires instead.
                if inner.len() != 1 || objs.len() != 1 {
                    return None;
                }
                let IrExpr::Object(props) = &objs[0] else {
                    return None;
                };
                let mut fd_ok = false;
                let mut mode = String::new();
                for (k, v) in props {
                    match (k.as_str(), v) {
                        ("fd", IrExpr::Int(0)) => fd_ok = true,
                        ("mode", IrExpr::Str(m, _)) if m == "r" => mode = m.clone(),
                        _ => {}
                    }
                }
                if !fd_ok || mode != "r" {
                    return None;
                }
                return Self::stage_text_of_stmt(&inner[0]);
            }
            IrStmt::While { cond, body } => {
                let c = Self::loop_head_text(cond)?;
                let b = Self::stage_text_list(body)?;
                format!("{c} do {b}; done")
            }
            _ => return None,
        };
        Some(vec![quote_sh(&text)])
    }

    /// A loop condition (`read -r name num`, `[ … ]`) → its bare shell text,
    /// with a per-command `IFS=:` prefix kept (bash's `IFS=x read`).
    fn loop_head_text(cond: &IrExpr) -> Option<String> {
        let IrExpr::Call { func, args } = cond else {
            return None;
        };
        if func == "builtin" || func == "exec" {
            let cmd = match args.first() {
                Some(IrExpr::Str(c, _)) => c,
                _ => return None,
            };
            let items: &[IrExpr] = match args.get(1) {
                Some(IrExpr::Array(items)) => items,
                _ => return None,
            };
            let mut words = vec![quote_sh(cmd)];
            for it in items {
                let s = match it {
                    IrExpr::Str(s, _) | IrExpr::Ident(s) => s,
                    _ => return None,
                };
                words.push(quote_sh(&strip_sh2glob(s)));
            }
            // `IFS=… cmd …` — the third argument is the command-scoped env.
            let mut prefix = String::new();
            if let Some(IrExpr::Object(props)) = args.get(2) {
                for (k, v) in props {
                    let IrExpr::Str(val, _) = v else {
                        return None;
                    };
                    prefix.push_str(&format!("{k}={} ", quote_sh(val)));
                }
            }
            return Some(format!("{prefix}while {}", words.join(" ")));
        }
        if func == "testBracket" || func == "test" {
            let mut words = vec!["[".to_string()];
            for it in args {
                match it {
                    IrExpr::Str(s, _) | IrExpr::Ident(s) => words.push(quote_sh(s)),
                    _ => return None,
                }
            }
            words.push("]".to_string());
            return Some(format!("while {}", words.join(" ")));
        }
        None
    }

    /// A loop body → its `; `-joined bare text.
    fn stage_text_list(body: &[IrStmt]) -> Option<String> {
        let mut parts = Vec::with_capacity(body.len());
        for st in body {
            let one = Self::stage_text_of_stmt(st)?;
            if one.len() != 1 {
                return None;
            }
            // Quote the whole stage command, not each word.
            parts.push(unquote_sh(&one[0]));
        }
        Some(parts.join("; "))
    }

    /// One stage expression → the argv token list its bare command renders
    /// to (`["sort", "-n"]`). Anything that is not a plain word list
    /// (subshell, nested pipeline, redirect call) declines.
    fn stage_text_of_expr(e: &IrExpr) -> Option<Vec<String>> {
        let IrExpr::Call { func, args } = e else {
            return None;
        };
        if func != "builtin" && func != "exec" {
            return None;
        }
        match args.first() {
            Some(IrExpr::Str(_, _)) => {}
            _ => return None,
        }
        let Some(IrExpr::Array(items)) = args.get(1) else {
            return None;
        };
        let mut out = Vec::with_capacity(items.len() + 1);
        for it in items {
            let s = match it {
                IrExpr::Str(s, _) | IrExpr::Ident(s) => s,
                _ => return None,
            };
            out.push(quote_sh(&strip_sh2glob(s)));
        }
        Some(out)
    }

    /// `( … ) < file` (or `<<< text`, `<<EOF`) as a native chain: every
    /// stage of the group joined by ` | `, stdin wired to the redirect.
    /// The stages stay SHELL TEXT for now (the group can hold a `while`
    /// loop, which has no argv form) but there is no `bash -c` child and no
    /// environment literal — the parent runs the chain and feeds it.
    fn group_pipeline_stdin(
        &mut self,
        body: &[IrStmt],
        redirs: &[IrExpr],
    ) -> Option<String> {
        if redirs.len() != 1 {
            return None;
        }
        let first_in = self.native_redirect(&redirs[0])?;
        if !matches!(
            first_in,
            NativeRedir::File { fd: 0, .. } | NativeRedir::StdinStr(_)
        ) {
            return None;
        }
        // A plain `cmd < f` (no group) is the single-command path's job.
        let stages = Self::group_stage_bodies(body)?;
        if stages.iter().any(|st| st.is_empty()) {
            return None;
        }
        let script = stages
            .iter()
            .map(|st| st.join(" "))
            .collect::<Vec<_>>()
            .join(" | ");
        let id = self.new_tmp();
        self.need_st = true;
        let lines = self.emit_native_pipeline_lines_to(
            &[vec![Self::go_str(&script)]],
            Some(first_in),
            None,
            None,
            false,
            &format!("__pg{id}"),
        );
        Some(format!(
            "func() int64 {{ {}; return __pg{id} }}()",
            // `; ` between the lines: they are separate STATEMENTS, and on a
            // single line Go inserts no semicolons, so a `}`-terminated
            // `if … else …` line would swallow the next one.
            lines.join("; ")
        ))
    }

    /// `redirect` Call: args[0] = Arrow(body), args[1] = Array of
    /// {fd, mode, target} objects. Value = exit status.
    fn redirect_expr(&mut self, args: &[IrExpr]) -> String {
        let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) = (args.first(), args.get(1))
        else {
            return self.sh2_stub("redirect");
        };
        // `( … ) < f` — a SUBSHELL group with a stdin-file redirect and no
        // pipeline inside: the group's own stages are the chain, fed from
        // the file (`while read … done < f`). Same native chain as
        // `a | b < f`; without this arm the whole loop went to `redirRun`
        // and iterated nothing.
        if let Some(text) = self.group_pipeline_stdin(body, redirs) {
            return text;
        }
        // `cmd < a > b` as a status VALUE: the same welded chain the
        // statement form lowers to (`stmt_redirect_native`), wrapped so the
        // status reads out instead of landing in `st`. Expression position
        // is what `! cmd <a >b` / `x=$? cmd …` produce, and without this
        // arm the whole thing stubbed even though the statement twin was
        // already native.
        {
            let todo = self.todo;
            let sides = self.sides.len();
            let tmp = self.tmp;
            if let Some((cmd, argv)) = self.body_single_exec(body) {
                if let Some(lines) = self.native_redirect_lines(&cmd, &argv, redirs) {
                    if self.todo == todo {
                        return format!(
                            "func() int64 {{ {}; return st }}()",
                            // See `group_pipeline_stdin`: on one line the
                            // lines need explicit `; ` separators, or a
                            // `}`-terminated `if … else …` line runs into
                            // the next statement.
                            lines.join("; ")
                        );
                    }
                }
            }
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
        }
        // `cmd 2>/dev/null` (and nothing else): discarding stderr is the
        // native helpers' default, so this is one exec with no `bash -c`
        // child and no env literal — the builtin policy is the same one the
        // pipeline stages use (see `body_single_exec_opt`).
        if !redirs.is_empty() && redirs.iter().all(null_stderr_obj) {
            // A builtin / natively-lowered fs command needs no exec at
            // all: its native statements run as side effects with the
            // status read out of `st` (see `builtin_fs_speculative`).
            if let Some(call_args) = lone_exec_args(body) {
                if let Some(lines) = self.builtin_fs_speculative(&call_args) {
                    for l in lines {
                        self.sides.push(l);
                    }
                    return "st".to_string();
                }
            }
            let todo = self.todo;
            let sides = self.sides.len();
            let tmp = self.tmp;
            if let Some((cmd, argv)) = self.body_single_exec_opt(body, true) {
                if self.todo == todo {
                    let call = Self::exec_cmd_call(&cmd, &argv);
                    return format!("runCmd({call})");
                }
            }
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
        }
        // `a | b > f` as a status VALUE: the same welded chain the
        // statement form lowers to (see `stmt_pipeline_redirect`), wrapped
        // so the value reads out instead of landing in `st`.
        {
            let todo = self.todo;
            let sides = self.sides.len();
            let tmp = self.tmp;
            if let Some(v) = self.value_pipeline_redirect(body, redirs) {
                if self.todo == todo {
                    return v;
                }
            }
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
        }
        if let Some(text) = self.cmd_text_stmts(body) {
            let mut with_redirs = format!("( {text} )");
            let mut bodies = String::new();
            for r in redirs {
                if let Some(rt) = self.redirect_text(r) {
                    with_redirs.push(' ');
                    with_redirs.push_str(&rt);
                    // A heredoc body must trail the COMPLETE command.
                    if let Some(b) = heredoc_body_spec_text(r) {
                        bodies.push_str(&b);
                    }
                } else {
                    return self.sh2_stub("redirect");
                }
            }
            with_redirs.push_str(&bodies);
            let env = self.env_lit_for(Some(&with_redirs));
            return format!("redirRun({}, {env})", Self::go_str(&with_redirs));
        }
        self.sh2_stub("redirect")
    }

    /// A redirect object {fd, mode, target} → shell text `2>/dev/null`.
    fn redirect_text(&mut self, r: &IrExpr) -> Option<String> {
        let IrExpr::Object(props) = r else {
            return None;
        };
        let mut fd = String::new();
        let mut mode = "w".to_string();
        let mut target = None;
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
                    // `> "$f"`, `> "$d/f1"` — a word built from variables:
                    // render it so the bash -c child (redirRun) resolves it
                    // through env_lit (every written var ships as
                    // `name=<value>`).
                    target = Some(self.redirect_word(v)?);
                }
                _ => {}
            }
        }
        let t = target?;
        Some(redirect_op_frag(&fd, &mode, &t)?)
    }

    /// One `IrRedirect` spec as shell text. Handles the fd-dup / fd-close
    /// forms (`2>&1`, `3>&-`, `5<&0` — the target carries the second fd, so
    /// the operator is the DIRECTION arrow and never the file mode: `2>>&1`
    /// is a bash syntax error) and every target word the renderer can
    /// express, including a command substitution (`<<< "$(echo …)"`).
    fn redirect_spec_text(&mut self, r: &crate::ir::IrRedirect) -> Option<String> {
        let fd = r.fd.unwrap_or(if r.mode == "r" { 0 } else { 1 });
        if let IrExpr::Str(s, _) = &r.target {
            if let Some(m) = s.strip_prefix('&').or_else(|| (s == "-").then_some(s.as_str())) {
                let arrow = if r.mode == "r" { "<&" } else { ">&" };
                return Some(format!("{fd}{arrow}{m}"));
            }
        }
        let word = self.redirect_word(&r.target)?;
        redirect_op_frag(&fd.to_string(), &r.mode, &word)
    }

    /// A redirect target as a shell word. A literal stays bare; a word
    /// built from variables is double-quoted so the `bash -c` child never
    /// word-splits or globs the path.
    fn redirect_word(&mut self, t: &IrExpr) -> Option<String> {
        match t {
            IrExpr::Str(s, _) => Some(s.clone()),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => Some(format!("${{{n}}}")),
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => Some(format!("${{{n}}}")),
                _ => None,
            },
            // `> "$d/f1"` — a path built from variables.
            IrExpr::Interpolate(parts) => {
                let mut w = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => w.push_str(s),
                        InterpPart::Expr(x) => w.push_str(&redirect_ref_word(x)?),
                    }
                }
                Some(quote_sh_dq(&w))
            }
            // `<<< "$(echo …)"` — a command substitution or any other
            // word-shaped expression the arms above do not name.
            _ => Some(quote_sh_dq(&self.cmd_text_expr(t)?)),
        }
    }

    /// One `{fd, mode, target}` redirect object as a native stdio wiring,
    /// or `None` for every shape the native path cannot reproduce exactly
    /// (fd 3+, `<&`/`>&-`, fd duplication from a descriptor the native path
    /// does not model). REFUSE > GUESS: the caller then keeps the `bash -c`
    /// reconstruction.
    fn native_redirect(&mut self, r: &IrExpr) -> Option<NativeRedir> {
        let IrExpr::Object(props) = r else {
            return None;
        };
        let mut fd: Option<i64> = None;
        let mut mode = String::from("w");
        let mut target: Option<&IrExpr> = None;
        for (k, v) in props {
            match k.as_str() {
                "fd" => {
                    fd = Some(match v {
                        IrExpr::Int(i) => *i,
                        _ => return None,
                    })
                }
                "mode" => {
                    mode = match v {
                        IrExpr::Str(s, _) => s.clone(),
                        _ => return None,
                    }
                }
                "target" => target = Some(v),
                _ => {}
            }
        }
        let target = target?;
        if let IrExpr::Str(s, _) = target {
            if let Some(rest) = s.strip_prefix('&') {
                // `2>&1` and nothing else: any other fd-dup names a
                // descriptor Go's exec.Cmd does not carry.
                return match (fd, rest) {
                    (Some(2), "1") => Some(NativeRedir::DupStdoutToStderr),
                    _ => None,
                };
            }
            if s == "-" {
                // `2>&-` closes the fd — exec.Cmd has no closed-stdio state.
                return None;
            }
        }
        // `<<< word` / `<<EOF … EOF`: stdin fed from a string. Only fd 0
        // (the default for these modes) — any other fd keeps the fallback.
        if mode == "herestring" {
            if !matches!(fd, None | Some(0)) {
                return None;
            }
            // A herestring always feeds the word plus ONE newline (even
            // `<<< ""` feeds `"\n"`) — that newline is part of the input,
            // so `wc -l` and `read` agree with bash.
            let text = self.expr_str(target);
            return Some(NativeRedir::StdinStr(format!("({text} + \"\\n\")")));
        }
        if mode == "heredoc" || mode == "heredoc-tabs" {
            if !matches!(fd, None | Some(0)) {
                return None;
            }
            let IrExpr::Str(content, _) = target else {
                return None;
            };
            // `<<-` strips leading TABs (only tabs) from every line — at
            // render time, since the content is static text.
            let text = if mode == "heredoc-tabs" {
                content
                    .split('\n')
                    .map(|l| l.trim_start_matches('\t'))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                content.clone()
            };
            return Some(NativeRedir::StdinStr(Self::go_str(&text)));
        }
        let path = self.expr_str(target);
        let fd = fd.unwrap_or(if mode == "r" { 0 } else { 1 });
        match (fd, mode.as_str()) {
            (0, "r") => Some(NativeRedir::File {
                fd,
                path,
                read: true,
                append: false,
            }),
            (1 | 2, "w") => Some(NativeRedir::File {
                fd,
                path,
                read: false,
                append: false,
            }),
            (1 | 2, "a") => Some(NativeRedir::File {
                fd,
                path,
                read: false,
                append: true,
            }),
            _ => None,
        }
    }

    /// The same wiring for the core's `IrRedirect` statement spec.
    fn native_redirect_spec(&mut self, r: &crate::ir::IrRedirect) -> Option<NativeRedir> {
        let mut props: Vec<(String, IrExpr)> = Vec::new();
        if let Some(fd) = r.fd {
            props.push(("fd".to_string(), IrExpr::Int(fd as i64)));
        }
        props.push((
            "mode".to_string(),
            IrExpr::Str(r.mode.clone(), crate::ir::StrStyle::DoubleQuoted),
        ));
        props.push(("target".to_string(), r.target.clone()));
        self.native_redirect(&IrExpr::Object(props))
    }

    /// Native `exec.Command` + `os` fd wiring for `cmd argv…` + `redirs`.
    /// Returns the Go lines that leave the command's exit status in `st`
    /// (the caller writes `st` itself only through these lines), or `None`
    /// when a redirect is outside the native subset.
    ///
    /// This is the native counterpart of `redirRun(script, env)`: instead
    /// of rebuilding the command as shell text and shipping every written
    /// shell variable as `name=<value>` env entries for a `bash -c` child,
    /// the command runs under `exec.Command` with its stdio welded to the
    /// redirect targets (AGENTS.md §2 — native idioms over fork/exec).
    fn native_redirect_lines(
        &mut self,
        cmd: &str,
        argv: &[String],
        redirs: &[IrExpr],
    ) -> Option<Vec<String>> {
        let mut plan: Vec<NativeRedir> = Vec::with_capacity(redirs.len());
        // `2>&1` is only faithful once stdout is attached to something
        // concrete — before that bash duplicates the inherited descriptor
        // (os.Stdout), which a `c.Stderr = c.Stdout` with a nil Stdout
        // cannot express.
        let mut stdout_attached = false;
        for r in redirs {
            let nr = self.native_redirect(r)?;
            match &nr {
                NativeRedir::File { fd: 1, .. } => stdout_attached = true,
                NativeRedir::DupStdoutToStderr if !stdout_attached => return None,
                _ => {}
            }
            plan.push(nr);
        }
        Some(self.emit_native_redirect_lines(cmd, argv, &plan))
    }

    /// The spec-typed twin of [`Self::native_redirect_lines`].
    fn native_redirect_spec_lines(
        &mut self,
        cmd: &str,
        argv: &[String],
        redirs: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        let mut plan: Vec<NativeRedir> = Vec::with_capacity(redirs.len());
        let mut stdout_attached = false;
        for r in redirs {
            let nr = self.native_redirect_spec(r)?;
            match &nr {
                NativeRedir::File { fd: 1, .. } => stdout_attached = true,
                NativeRedir::DupStdoutToStderr if !stdout_attached => return None,
                _ => {}
            }
            plan.push(nr);
        }
        Some(self.emit_native_redirect_lines(cmd, argv, &plan))
    }

    fn emit_native_redirect_lines(
        &mut self,
        cmd: &str,
        argv: &[String],
        plan: &[NativeRedir],
    ) -> Vec<String> {
        let id = self.new_tmp();
        let cvar = format!("__rd{id}");
        let okvar = format!("__rdok{id}");
        // `command < a > b` (no operand): bash's `command` is a no-op that
        // SUCCEEDS, so there is no process to spawn — only the redirects
        // happen (each one still opens left to right, and an unopenable
        // input leaves the status at 1 with the output never created).
        // Exec'ing a `command` binary would invent a failure bash does not
        // have.
        let noop = cmd == "command" && argv.is_empty();
        // `cat <<EOF … EOF` / `cat <<< s`: with no FILE operand cat copies
        // its stdin byte for byte, so the command IS the text — printing it
        // needs neither a child nor a descriptor. Only when the heredoc is
        // the ONE redirect: with an output target as well, the bytes belong
        // in that file and the general wiring below handles them.
        if cmd == "cat" && argv.is_empty() && plan.len() == 1 {
            if let NativeRedir::StdinStr(t) = &plan[0] {
                return vec![format!("fmt.Print({t});"), "st = 0;".to_string()];
            }
        }
        if noop && plan.iter().any(|r| matches!(r, NativeRedir::DupStdoutToStderr)) {
            // `2>&1` needs a command's descriptors to duplicate.
            return vec!["st = 1;".to_string()];
        }
        // Spread-aware: a mid-list glob folds through `append` (see
        // `exec_cmd_call`) — a raw `join` would not compile.
        let call = Self::exec_cmd_call(cmd, argv);
        let mut lines = if noop {
            vec![format!("{okvar} := true;")]
        } else {
            vec![
                format!("{cvar} := {call};"),
                format!("{okvar} := true;"),
            ]
        };
        let mut closes: Vec<String> = Vec::new();
        for (i, nr) in plan.iter().enumerate() {
            match nr {
                NativeRedir::File {
                    fd,
                    path,
                    read,
                    append,
                } => {
                    let fv = format!("__rdf{id}_{i}");
                    let ev = format!("__rde{id}_{i}");
                    lines.push(if *read {
                        format!("{fv}, {ev} := os.Open({path});")
                    } else {
                        let flags = if *append {
                            "os.O_WRONLY|os.O_CREATE|os.O_APPEND"
                        } else {
                            "os.O_WRONLY|os.O_CREATE|os.O_TRUNC"
                        };
                        format!("{fv}, {ev} := os.OpenFile({path}, {flags}, 0644);")
                    });
                    let field = match fd {
                        0 => "Stdin",
                        2 => "Stderr",
                        _ => "Stdout",
                    };
                    // A redirect that cannot be opened is a status-1 failure
                    // in bash and the command does NOT run — the run below is
                    // guarded, not merely best-effort.
                    if noop {
                        lines.push(format!("if {ev} != nil {{ {okvar} = false }}"));
                    } else {
                        lines.push(format!(
                            "if {ev} == nil {{ {cvar}.{field} = {fv} }} else {{ {okvar} = false }}"
                        ));
                    }
                    closes.push(format!("if {ev} == nil {{ {fv}.Close() }}"));
                }
                NativeRedir::DupStdoutToStderr => {
                    lines.push(format!("{cvar}.Stderr = {cvar}.Stdout;"));
                }
                // `<<<`/`<<EOF`: stdin fed from a string — infallible, so
                // no guard and no close.
                NativeRedir::StdinStr(text) => {
                    if !noop {
                        lines.push(format!("{cvar}.Stdin = strings.NewReader({text});"));
                    }
                }
            }
        }
        if !noop {
            lines.push(format!("shStdIn({cvar});"));
            lines.push(format!(
                "if {okvar} {{ st = runCmd({cvar}) }} else {{ st = 1 }}"
            ));
        } else {
            lines.push(format!("if {okvar} {{ st = 0 }} else {{ st = 1 }}"));
        }
        lines.extend(closes);
        self.need_st = true;
        lines
    }

    /// A user FUNCTION call with welded stdio redirects (`f < in`, `f >
    /// out`, `f 2>err`, `f 2>&1`, `f <<< s`): run the closure with the
    /// process-wide os.Stdin/Stdout/Stderr temporarily swapped. Children the
    /// closure spawns inherit the swapped descriptors (they are set before
    /// the call); Go-level prints (`fmt.Println`) write through the same
    /// vars. Only the redirect shapes `native_redirect_spec` accepts, with
    /// the same `2>&1`-after-stdout rule; anything else (or an unopenable
    /// file, which is status 1 with the call skipped — exactly like bash)
    /// refuses and the caller keeps the fallback. Returns the Go lines, or
    /// `None`. The render state is rolled back on refusal.
    fn function_redirect_lines(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let (Some(IrExpr::Str(head, _)), Some(IrExpr::Array(_))) =
            (args.first(), args.get(1))
        else {
            return None;
        };
        if !self.functions.contains(head) {
            return None;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let fail = |r: &mut Render| {
            r.sides.truncate(sides);
            r.tmp = tmp;
            r.todo = todo;
            None
        };
        let mut plan: Vec<NativeRedir> = Vec::with_capacity(redirects.len());
        let mut stdout_attached = false;
        for r in redirects {
            let Some(nr) = self.native_redirect_spec(r) else {
                return fail(self);
            };
            if self.todo != todo {
                return fail(self);
            }
            match &nr {
                NativeRedir::File { fd: 1, .. } => stdout_attached = true,
                NativeRedir::DupStdoutToStderr if !stdout_attached => {
                    return fail(self);
                }
                _ => {}
            }
            plan.push(nr);
        }
        let args = args.clone();
        let head = head.clone();
        let id = self.new_tmp();
        let okvar = format!("__rfok{id}");
        let mut lines = vec![format!("{okvar} := true;")];
        let mut swapped = [false, false, false];
        let mut closes: Vec<String> = Vec::new();
        for (i, nr) in plan.iter().enumerate() {
            match nr {
                NativeRedir::File {
                    fd,
                    path,
                    read,
                    append,
                } => {
                    let fv = format!("__rff{id}_{i}");
                    let ev = format!("__rfe{id}_{i}");
                    let (slot, old) = match fd {
                        0 => ("Stdin", format!("__rfoi{id}")),
                        2 => ("Stderr", format!("__rfoe{id}")),
                        _ => ("Stdout", format!("__rfoo{id}")),
                    };
                    if !swapped[*fd as usize] {
                        lines.push(format!("{old} := os.{slot};"));
                        swapped[*fd as usize] = true;
                    }
                    lines.push(if *read {
                        format!("{fv}, {ev} := os.Open({path});")
                    } else {
                        let flags = if *append {
                            "os.O_WRONLY|os.O_CREATE|os.O_APPEND"
                        } else {
                            "os.O_WRONLY|os.O_CREATE|os.O_TRUNC"
                        };
                        format!("{fv}, {ev} := os.OpenFile({path}, {flags}, 0644);")
                    });
                    lines.push(format!(
                        "if {ev} == nil {{ os.{slot} = {fv} }} else {{ {okvar} = false }}"
                    ));
                    closes.push(format!("if {ev} == nil {{ {fv}.Close() }}"));
                }
                NativeRedir::DupStdoutToStderr => {
                    if !swapped[2] {
                        lines.push(format!("__rfoe{id} := os.Stderr;"));
                        swapped[2] = true;
                    }
                    lines.push("os.Stderr = os.Stdout;".to_string());
                }
                NativeRedir::StdinStr(text) => {
                    if !swapped[0] {
                        lines.push(format!("__rfoi{id} := os.Stdin;"));
                        swapped[0] = true;
                    }
                    lines.push(format!("os.Stdin = strings.NewReader({text});"));
                }
            }
        }
        // The closure call itself: fArgs + call + restore, under the
        // open-ok guard. `call_function` emits directly (not into sides),
        // so snapshot the line count and splice the call between the
        // swap-in and the restore.
        let call_at = self.out.len();
        self.call_function(&head, &args);
        if self.todo != todo {
            self.out.truncate(call_at);
            return fail(self);
        }
        let call_lines: Vec<String> = self.out.drain(call_at..).collect();
        lines.push(format!("if {okvar} {{"));
        lines.extend(call_lines.iter().map(|l| format!("    {l}")));
        lines.push("} else {".to_string());
        lines.push("    st = 1;".to_string());
        lines.push("}".to_string());
        if swapped[0] {
            lines.push(format!("os.Stdin = __rfoi{id};"));
        }
        if swapped[1] {
            lines.push(format!("os.Stdout = __rfoo{id};"));
        }
        if swapped[2] {
            lines.push(format!("os.Stderr = __rfoe{id};"));
        }
        lines.extend(closes);
        self.need_st = true;
        Some(lines)
    }

    /// `cat > f <<EOF … EOF` / `cat >> f <<EOF … EOF` → a native file
    /// write of the heredoc content. The content interpolates exactly when
    /// the heredoc is unquoted (`interpolate`), via the runtime `expand`.
    /// `None` keeps the fallback. Only the heredoc plus one fd-1 plain
    /// file write/append; the render state is rolled back on refusal.
    fn heredoc_to_file(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<String> {
        if !is_single_cat(inner) {
            return None;
        }
        if redirects.len() != 2 {
            return None;
        }
        let heredoc = redirects
            .iter()
            .find(|r| r.mode == "heredoc" && r.fd.unwrap_or(0) == 0)?;
        let target = match &heredoc.target {
            IrExpr::Str(text, _) => {
                if heredoc.interpolate {
                    format!("expand({})", Self::go_str(text))
                } else {
                    Self::go_str(text)
                }
            }
            _ => return None,
        };
        let file = redirects.iter().find(|r| {
            (r.mode == "w" || r.mode == "a") && r.fd.unwrap_or(1) == 1
        })?;
        let append = file.mode == "a";
        let path = self.expr_str(&file.target.clone());
        let id = self.new_tmp();
        let op = if append {
            format!(
                "if __hwf{id}, __hwe{id} := os.OpenFile({path}, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0644); \
                 __hwe{id} == nil {{ __hwf{id}.WriteString({target}); __hwf{id}.Close(); st = 0 }} else {{ st = 1 }}"
            )
        } else {
            format!(
                "if __hwe{id} := os.WriteFile({path}, []byte({target}), 0644); __hwe{id} == nil {{ st = 0 }} else {{ st = 1 }}"
            )
        };
        self.need_st = true;
        Some(op)
    }

    /// One pipeline stage → (argv parts, stdio plan). Accepts the bare
    /// exec shapes (via `body_single_exec_opt`, which already drops
    /// `2>/dev/null` wrappers) and the `builtin(Arrow, redirs)` /
    /// `redirect(Arrow, redirs)` stage forms with any plan the chain can
    /// place (see `pipeline_chain`). `None` keeps the `bash -c` fallback.
    /// The render state is rolled back on refusal (never a partial render).
    fn pipeline_stage(
        &mut self,
        body: &[IrStmt],
    ) -> Option<(Vec<String>, Vec<NativeRedir>)> {
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        // Bare command (null-stderr wrappers already dropped inside).
        if let Some((cmd, argv)) = self.body_single_exec_opt(body, true) {
            if self.todo == todo {
                let mut s = vec![cmd];
                s.extend(argv);
                return Some((s, Vec::new()));
            }
        }
        self.sides.truncate(sides);
        self.tmp = tmp;
        self.todo = todo;
        // `cmd > f 2>e` / `( cmd ) 2>/dev/null` stage forms.
        let (inner, objs): (&[IrStmt], &[IrExpr]) = match body {
            [IrStmt::Expr(IrExpr::Call { func, args })]
                if func == "builtin" || func == "redirect" =>
            {
                match (args.first(), args.get(1)) {
                    (Some(IrExpr::Arrow(inner)), Some(IrExpr::Array(objs))) => (inner, objs),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let (cmd, argv) = match self.body_single_exec_opt(inner, true) {
            Some(x) if self.todo == todo => x,
            _ => {
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
                return None;
            }
        };
        let mut s = vec![cmd];
        s.extend(argv);
        let mut plan = Vec::with_capacity(objs.len());
        for r in objs {
            match self.native_redirect(r) {
                Some(nr) if self.todo == todo => plan.push(nr),
                _ => {
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                    return None;
                }
            }
        }
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return None;
        }
        Some((s, plan))
    }

    /// A whole stage list plus the pipeline-LEVEL stdio plan → welded-chain
    /// inputs: stage argv lists plus the edge wires. Middle-stage plans
    /// must be discard-only (`2>/dev/null` — the helpers' own default, so
    /// they drop out); the first stage may also read stdin from a file, and
    /// the last may write stdout/stderr to files (or `2>&1` once stdout is
    /// attached) — those join the outer plan under one affinity
    /// (`pipeline_plan`). Anything unplaceable is `None` (the `bash -c`
    /// fallback). The render state is rolled back on refusal.
    fn pipeline_chain(
        &mut self,
        bodies: &[&[IrStmt]],
        outer: Vec<NativeRedir>,
    ) -> Option<(
        Vec<Vec<String>>,
        Option<NativeRedir>,
        Option<NativeRedir>,
        Option<NativeRedir>,
        bool,
    )> {
        if bodies.is_empty() {
            return None;
        }
        let n = bodies.len();
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let fail = |r: &mut Render| {
            r.sides.truncate(sides);
            r.tmp = tmp;
            r.todo = todo;
        };
        let mut stagess: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut combined: Vec<NativeRedir> = Vec::new();
        for (i, b) in bodies.iter().enumerate() {
            let (parts, plan) = match self.pipeline_stage(b) {
                Some(x) => x,
                None => {
                    fail(self);
                    return None;
                }
            };
            for nr in plan {
                match &nr {
                    NativeRedir::File { fd: 0, .. } => {
                        if i != 0 {
                            fail(self);
                            return None;
                        }
                        combined.push(nr);
                    }
                    // A stage-level `<<<`/`<<EOF`: stdin text, first stage
                    // only (anything later would override its pipe feed,
                    // which the fixed emission cannot express).
                    NativeRedir::StdinStr(_) => {
                        if i != 0 {
                            fail(self);
                            return None;
                        }
                        combined.push(nr);
                    }
                    NativeRedir::File { .. } | NativeRedir::DupStdoutToStderr => {
                        if i != n - 1 {
                            // Mid-chain: only a stderr discard survives
                            // (the helpers' own default — it drops out).
                            if !is_discard_redir(&nr) {
                                fail(self);
                                return None;
                            }
                        } else {
                            combined.push(nr);
                        }
                    }
                }
            }
            stagess.push(parts);
        }
        combined.extend(outer);
        let out = match Self::pipeline_plan(combined) {
            Some(x) => x,
            None => {
                fail(self);
                return None;
            }
        };
        if self.todo != todo {
            fail(self);
            return None;
        }
        Some((stagess, out.0, out.1, out.2, out.3))
    }

    /// `pipeline > f` / `pipeline 2>e` / `pipeline < f` in statement
    /// position (see the `IrStmt::Redirect` arm): the stages run as real
    /// processes with the edge stdio welded to the redirect targets. Only
    /// stdin-file on the FIRST stage and stdout/stderr files (or `2>&1`
    /// once stdout is attached) on the LAST stage are expressible — a
    /// middle-stage stdio redirect has no counterpart in a welded chain.
    /// `None` keeps the `bash -c` fallback. The render state is rolled back
    /// by the caller, so this only returns `None` (never a partial render).
    fn stmt_pipeline_redirect(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        // The `redirect` CALL form (`redirect(arrow, [objs])`) reaches here
        // as an expression statement once `IrStmt::Redirect` has been
        // re-shaped — unwrap it so both spellings share one analysis.
        if let [IrStmt::Expr(IrExpr::Call { func, args })] = inner {
            if func == "redirect" {
                let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(objs))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                if !redirects.is_empty() {
                    return None;
                }
                let plan: Vec<NativeRedir> =
                    objs.iter().map(|o| self.native_redirect(o)).collect::<Option<_>>()?;
                let bodies = Self::group_stage_bodies(body)?;
                let (first_in, last_out, last_err, last_dup) = Self::pipeline_plan(plan)?;
                return Some(self.emit_native_pipeline_lines(
                    &bodies, first_in, last_out, last_err, last_dup,
                ));
            }
        }
        // The pipeline's stage bodies: a statement `Pipeline` node's
        // stages, or the Arrow bodies of an expression-form `pipeline`
        // call in statement position.
        let bodies: Vec<&[IrStmt]> = if let [IrStmt::Pipeline {
            stages,
            capture: None,
            ..
        }] = inner
        {
            stages.iter().map(|st| &st[..]).collect()
        } else if let [IrStmt::Expr(IrExpr::Call { func, args })] = inner {
            if func != "pipeline" {
                return None;
            }
            let Some(IrExpr::Array(items)) = args.first() else {
                return None;
            };
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                let IrExpr::Arrow(body) = it else {
                    return None;
                };
                out.push(&body[..]);
            }
            out
        } else {
            return None;
        };
        if bodies.is_empty() {
            return None;
        }
        // A bare subshell group around a pipeline (`( a | b )` — no
        // redirects at all) is the pipeline itself: no `bash -c` child.
        // Stage-level discards drop out inside the chain; anything wired
        // keeps the welded emission below.
        let mut native: Vec<NativeRedir> = Vec::with_capacity(redirects.len());
        for r in redirects {
            let mut props: Vec<(String, IrExpr)> = Vec::new();
            if let Some(fd) = r.fd {
                props.push(("fd".to_string(), IrExpr::Int(fd as i64)));
            }
            props.push((
                "mode".to_string(),
                IrExpr::Str(r.mode.clone(), crate::ir::StrStyle::DoubleQuoted),
            ));
            props.push(("target".to_string(), r.target.clone()));
            native.push(self.native_redirect(&IrExpr::Object(props))?);
        }
        // A pipeline-LEVEL stdin redirect (`a | b < f`, `a | b <<< s`)
        // feeds the LAST stage in bash (it overrides the pipe feed) — but
        // the chain wires edge-stdin onto the FIRST. One stage is both, so
        // only multi-stage refuses here; single-stage stays native below.
        if bodies.len() > 1
            && native.iter().any(|nr| {
                matches!(
                    nr,
                    NativeRedir::File { fd: 0, .. } | NativeRedir::StdinStr(_)
                )
            })
        {
            return None;
        }
        let (stagess, first_in, last_out, last_err, last_dup) =
            self.pipeline_chain(&bodies, native)?;
        if redirects.is_empty()
            && first_in.is_none()
            && last_out.is_none()
            && last_err.is_none()
            && !last_dup
        {
            let parts: Vec<String> = stagess.iter().map(|st| Self::argv_slice(st)).collect();
            self.need_st = true;
            return Some(vec![format!(
                "st = runPipe([][]string{{{}}});",
                parts.join(", ")
            )]);
        }
        if first_in.is_none() && last_out.is_none() && last_err.is_none() && !last_dup {
            return None;
        }
        Some(self.emit_native_pipeline_lines(&stagess, first_in, last_out, last_err, last_dup))
    }

    /// Edge-stdio affinity for a welded pipeline chain: stdin-file only on
    /// the first stage, stdout/stderr files (or `2>&1` once stdout is
    /// attached) only on the last. At most one wire per stdio field — `>a
    /// >b` (last wins in bash) keeps the fallback, as does a `2>&1`
    /// combined with an explicit stderr file (their order decides the
    /// winner, which the fixed emission order cannot reproduce). `None`
    /// keeps the `bash -c` fallback.
    fn pipeline_plan(
        plan: Vec<NativeRedir>,
    ) -> Option<(
        Option<NativeRedir>,
        Option<NativeRedir>,
        Option<NativeRedir>,
        bool,
    )> {
        let mut first_in: Option<NativeRedir> = None;
        let mut last_out: Option<NativeRedir> = None;
        let mut last_err: Option<NativeRedir> = None;
        let mut last_dup = false;
        let mut stdout_attached = false;
        for nr in plan {
            match &nr {
                NativeRedir::File { fd: 0, .. } => {
                    if first_in.is_some() {
                        return None;
                    }
                    first_in = Some(nr);
                }
                // `<<<`/`<<EOF` stdin text joins the same first-stage
                // affinity (one stdin source — a second keeps the fallback,
                // since bash's last-wins has no fixed emission order here).
                NativeRedir::StdinStr(_) => {
                    if first_in.is_some() {
                        return None;
                    }
                    first_in = Some(nr);
                }
                NativeRedir::File { fd: 1, .. } => {
                    if last_out.is_some() {
                        return None;
                    }
                    stdout_attached = true;
                    last_out = Some(nr);
                }
                NativeRedir::File { fd: 2, .. } => {
                    if last_err.is_some() || last_dup {
                        return None;
                    }
                    last_err = Some(nr);
                }
                NativeRedir::DupStdoutToStderr => {
                    if !stdout_attached || last_dup || last_err.is_some() {
                        return None;
                    }
                    last_dup = true;
                }
                _ => return None,
            }
        }
        Some((first_in, last_out, last_err, last_dup))
    }

    /// `redirect(pipeline, redirs)` as a status VALUE (see `redirect_expr`):
    /// the same welded chain the statement form lowers to, wrapped in an
    /// IIFE so the value reads out. A redirect-less `redirect(pipeline,
    /// [])` is the pipeline's own status. `None` keeps the `bash -c`
    /// fallback.
    fn value_pipeline_redirect(
        &mut self,
        body: &[IrStmt],
        redirs: &[IrExpr],
    ) -> Option<String> {
        // The pipeline's stage bodies (see `stmt_pipeline_redirect`).
        let bodies: Vec<&[IrStmt]> = if let [IrStmt::Expr(IrExpr::Call { func, args })] = body
        {
            if func != "pipeline" {
                return None;
            }
            // A redirect-less `redirect(pipeline, [])` is the pipeline's
            // own status, no IIFE needed.
            if redirs.is_empty() {
                return Some(self.pipeline_expr(args));
            }
            let Some(IrExpr::Array(items)) = args.first() else {
                return None;
            };
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                let IrExpr::Arrow(stage) = it else {
                    return None;
                };
                out.push(&stage[..]);
            }
            out
        } else if let [IrStmt::Pipeline {
            stages,
            capture: None,
            ..
        }] = body
        {
            if redirs.is_empty() {
                let bodies: Vec<&[IrStmt]> = stages.iter().map(|st| &st[..]).collect();
                let (stagess, fi, lo, le, dup) = self.pipeline_chain(&bodies, Vec::new())?;
                if fi.is_none() && lo.is_none() && le.is_none() && !dup {
                    let parts: Vec<String> =
                        stagess.iter().map(|st| Self::argv_slice(st)).collect();
                    return Some(format!("runPipe([][]string{{{}}})", parts.join(", ")));
                }
                return self.welded_value(stagess, fi, lo, le, dup);
            }
            stages.iter().map(|st| &st[..]).collect()
        } else {
            return None;
        };
        if bodies.is_empty() {
            return None;
        }
        let mut native: Vec<NativeRedir> = Vec::with_capacity(redirs.len());
        for r in redirs {
            native.push(self.native_redirect(r)?);
        }
        // Same last-stage-stdin rule as the statement form (see
        // `stmt_pipeline_redirect`): a value-level stdin redirect on a
        // multi-stage chain keeps the fallback.
        if bodies.len() > 1
            && native.iter().any(|nr| {
                matches!(
                    nr,
                    NativeRedir::File { fd: 0, .. } | NativeRedir::StdinStr(_)
                )
            })
        {
            return None;
        }
        let (stagess, first_in, last_out, last_err, last_dup) =
            self.pipeline_chain(&bodies, native)?;
        if first_in.is_none() && last_out.is_none() && last_err.is_none() && !last_dup {
            return None;
        }
        self.welded_value(stagess, first_in, last_out, last_err, last_dup)
    }

    /// A welded chain as a status-VALUE IIFE (see
    /// `value_pipeline_redirect`).
    fn welded_value(
        &mut self,
        stagess: Vec<Vec<String>>,
        first_in: Option<NativeRedir>,
        last_out: Option<NativeRedir>,
        last_err: Option<NativeRedir>,
        last_dup: bool,
    ) -> Option<String> {
        let id = self.new_tmp();
        let result = format!("__pqv{id}");
        let mut lines = self.emit_native_pipeline_lines_to(
            &stagess,
            first_in,
            last_out,
            last_err,
            last_dup,
            &result,
        );
        // The emitter's lines declare with `:=` — inside the IIFE the
        // result temp needs its declaration too. Newline-joined: Go
        // separates same-line statements with `;`, not spaces.
        lines.insert(0, format!("var {result} int64;"));
        lines.push(format!("return {result};"));
        Some(format!("func() int64 {{\n{}\n}}()", lines.join("\n")))
    }

    /// The welded chain for [`Render::stmt_pipeline_redirect`]: one
    /// `exec.Command` per stage, `StdoutPipe` links between them, the edge
    /// stdio welded to the redirect targets, every other stderr discarded —
    /// the same stdio contract `runPipe` implements, with the last stage's
    /// stdout (and opt stderr) going to files instead of the terminal. The
    /// exit status is the last stage's, exactly like `runPipe` (and bash
    /// without `pipefail`): a wiring/start failure is status 1 with nothing
    /// run, mirroring the guarded single-command lowering.
    fn emit_native_pipeline_lines(
        &mut self,
        stages: &[Vec<String>],
        first_in: Option<NativeRedir>,
        last_out: Option<NativeRedir>,
        last_err: Option<NativeRedir>,
        last_dup: bool,
    ) -> Vec<String> {
        self.emit_native_pipeline_lines_to(stages, first_in, last_out, last_err, last_dup, "st")
    }

    /// [`Render::emit_native_pipeline_lines`], assigning the status to any
    /// Go variable: `"st"` for statements (which also sets the `$?` flag),
    /// or an IIFE-local for value contexts (see `value_pipeline_redirect`).
    fn emit_native_pipeline_lines_to(
        &mut self,
        stages: &[Vec<String>],
        first_in: Option<NativeRedir>,
        last_out: Option<NativeRedir>,
        last_err: Option<NativeRedir>,
        last_dup: bool,
        result: &str,
    ) -> Vec<String> {
        let id = self.new_tmp();
        let okvar = format!("__pqok{id}");
        let stvar = format!("__pqst{id}");
        let mut lines = vec![format!("{okvar} := true;"), format!("{stvar} := int64(0);")];
        let mut closes: Vec<String> = Vec::new();
        let n = stages.len();
        for (i, st) in stages.iter().enumerate() {
            // Spread-aware (see `exec_cmd_call`): a mid-list glob folds
            // through `append`, which a raw `join` cannot express.
            let call = Self::exec_cmd_call(&st[0], &st[1..]);
            lines.push(format!("__pq{id}_{i} := {call};"));
        }
        // Inter-stage links: stage i's stdout feeds stage i+1's stdin.
        for j in 0..n.saturating_sub(1) {
            lines.push(format!(
                "__pq{id}_p{j}, __pq{id}_pe{j} := __pq{id}_{j}.StdoutPipe();"
            ));
            lines.push(format!(
                "if __pq{id}_pe{j} != nil {{ {okvar} = false }} else {{ __pq{id}_{}.Stdin = __pq{id}_p{j} }}",
                j + 1
            ));
        }
        // Edge wires: stdin-file on the first stage…
        if let Some(NativeRedir::File { path, read: true, .. }) = &first_in {
            lines.push(format!("__pq{id}_fi, __pq{id}_fe := os.Open({path});"));
            lines.push(format!(
                "if __pq{id}_fe == nil {{ __pq{id}_0.Stdin = __pq{id}_fi }} else {{ {okvar} = false }}"
            ));
            closes.push(format!("if __pq{id}_fe == nil {{ __pq{id}_fi.Close() }}"));
        }
        // …or stdin-from-string (`<<<`/`<<EOF`) on the first stage.
        if let Some(NativeRedir::StdinStr(text)) = &first_in {
            lines.push(format!("__pq{id}_0.Stdin = strings.NewReader({text});"));
        }
        // …stdout/stderr files on the last stage.
        let last = n - 1;
        let wire_file = |lines: &mut Vec<String>,
                         closes: &mut Vec<String>,
                         field: &str,
                         path: &str,
                         append: bool,
                         k: usize| {
            let flags = if append {
                "os.O_WRONLY|os.O_CREATE|os.O_APPEND"
            } else {
                "os.O_WRONLY|os.O_CREATE|os.O_TRUNC"
            };
            lines.push(format!(
                "__pq{id}_f{k}, __pq{id}_fe{k} := os.OpenFile({path}, {flags}, 0644);"
            ));
            lines.push(format!(
                "if __pq{id}_fe{k} == nil {{ __pq{id}_{last}.{field} = __pq{id}_f{k} }} else {{ {okvar} = false }}"
            ));
            closes.push(format!("if __pq{id}_fe{k} == nil {{ __pq{id}_f{k}.Close() }}"));
        };
        if let Some(NativeRedir::File { path, append, .. }) = &last_out {
            wire_file(&mut lines, &mut closes, "Stdout", path, *append, 0);
        }
        if let Some(NativeRedir::File { path, append, .. }) = &last_err {
            wire_file(&mut lines, &mut closes, "Stderr", path, *append, 1);
        }
        if last_dup {
            lines.push(format!("__pq{id}_{last}.Stderr = __pq{id}_{last}.Stdout;"));
        }
        // runPipe's stdio contract for everything left unwired: every
        // stage's stderr discarded, the last stage's stdout inherited.
        let err_wired = last_err.is_some() || last_dup;
        let out_wired = last_out.is_some();
        for i in 0..n {
            let stderr_wired = (i == last) && err_wired;
            if !stderr_wired {
                lines.push(format!("__pq{id}_{i}.Stderr = io.Discard;"));
            }
            if i == last && !out_wired {
                lines.push(format!("__pq{id}_{last}.Stdout = os.Stdout;"));
            }
        }
        // Start all but the last, Run the last, Wait the rest — runPipe's
        // own sequence, so the status is the last stage's. A start failure
        // skips the run (status 1), exactly like runPipe's early return.
        // The parent's copies of the inter-stage pipes close after the
        // last stage is reaped (same deadlock runPipe documents: an
        // early-exiting downstream stage must retire the producers).
        lines.push(format!("if {okvar} {{"));
        lines.push(format!("{stvar} = func() int64 {{"));
        lines.push(format!(
            "if __pq{id}_0.Stdin == nil {{ __pq{id}_0.Stdin = os.Stdin }}"
        ));
        for i in 0..n.saturating_sub(1) {
            lines.push(format!(
                "if __e := __pq{id}_{i}.Start(); __e != nil {{ return 1 }}"
            ));
        }
        lines.push(format!(
            "if __e := __pq{id}_{last}.Run(); __e != nil {{ \
             if __ee, __ok := __e.(*exec.ExitError); __ok {{ return int64(__ee.ExitCode()) }}; \
             return 1 }}"
        ));
        for j in 0..n.saturating_sub(1) {
            lines.push(format!("__pq{id}_p{j}.Close();"));
        }
        for i in 0..n.saturating_sub(1) {
            lines.push(format!("__pq{id}_{i}.Wait();"));
        }
        lines.push("return 0;".to_string());
        lines.push("}();".to_string());
        lines.push("} else {".to_string());
        lines.push(format!("{stvar} = 1;"));
        lines.push("}".to_string());
        lines.extend(closes);
        lines.push(format!("{result} = {stvar};"));
        if result == "st" {
            self.need_st = true;
        }
        lines
    }

    /// `echo [-n|-e|-E] … > f` / `… >> f` in statement position, lowered to
    /// a native file write. bash's `echo` here is the renderer's own string
    /// expression and the redirect is a plain write, so the whole command
    /// needs NO subprocess — the alternative was `redirRun`, one `bash -c`
    /// child per echo (AGENTS.md §2). Returns the lines, or `None` with the
    /// render state rolled back so the caller keeps the shell-text
    /// reconstruction. REFUSE > GUESS: exactly one fd-1 write/append
    /// redirect and a literal `builtin` echo call, nothing else.
    fn echo_redirect_write(
        &mut self,
        inner: &[IrStmt],
        redirs: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return None;
        };
        // `builtin` is what the frontend emits; `shir_to_go`'s prelude
        // rewrites it to `exec` (this backend has not accepted the
        // `builtin` op), so both spellings reach here.
        if func != "builtin" && func != "exec" {
            return None;
        }
        let (Some(IrExpr::Str(name, _)), Some(IrExpr::Array(items))) = (args.first(), args.get(1))
        else {
            return None;
        };
        if name != "echo" {
            return None;
        }
        // One redirect, and it must be fd 1 opened for writing. `2>&1`,
        // `>|`, an fd-dup or a second redirect all keep the fallback.
        let [one] = redirs else { return None };
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let (path, append) = match self.native_redirect_spec(one) {
            Some(NativeRedir::File {
                fd: 1,
                path,
                read: false,
                append,
            }) => (path, append),
            _ => {
                self.sides.truncate(sides);
                self.todo = todo;
                self.tmp = tmp;
                return None;
            }
        };
        let (items, newline, escape) = Self::echo_options(items);
        let Some(mut parts) = self.echo_parts(items, escape) else {
            self.sides.truncate(sides);
            self.todo = todo;
            self.tmp = tmp;
            return None;
        };
        if newline {
            parts.push(Part::Lit("\n".to_string()));
        }
        let text = self.parts_str(parts);
        self.sides.truncate(sides);
        self.flush_sides();
        let id = self.new_tmp();
        let lines = if append {
            let (fv, ev) = (format!("__awf{id}"), format!("__awe{id}"));
            let tail = if self.need_st { "; st = 0" } else { "" };
            let mut l = format!(
                "if {fv}, {ev} := os.OpenFile({path}, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0644); \
                 {ev} == nil {{ {fv}.WriteString({text}); {fv}.Close(){tail} }}"
            );
            if self.need_st {
                l.push_str(" else { st = 1 }");
            }
            vec![l]
        } else if self.need_st {
            let wv = format!("__we{id}");
            vec![format!(
                "if {wv} := os.WriteFile({path}, []byte({text}), 0644); {wv} != nil {{ st = 1 }} else {{ st = 0 }}"
            )]
        } else {
            // a bare call statement discards the error result — no
            // temp binding, and no unused-variable diagnostic
            vec![format!("os.WriteFile({path}, []byte({text}), 0644)")]
        };
        Some(lines)
    }

    /// `printf fmt args… > f` (`>> f`): write the formatted output to the
    /// file — the same `bprintfStr` the statement (`printf_stmt`) and
    /// capture (`echo_printf_value`) forms use, so the three can never
    /// disagree, with the redirect target opened exactly like
    /// `echo_redirect_write` above. `None` keeps the fork/exec weld.
    /// REFUSE > GUESS: anything but one fd-1 file redirect; an empty,
    /// dynamic, dash-leading (`-v` assigns a variable instead of printing,
    /// and `--` is an option, not a format), or `%q` format (the one verb
    /// the external printf does not implement — see
    /// `native_stage_builtin`, the same external-agreement gate the weld
    /// path applies). A refused shape keeps today's `exec.Command` weld,
    /// never a stub.
    fn printf_redirect_write(
        &mut self,
        inner: &[IrStmt],
        redirs: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return None;
        };
        // Same spelling rule as `echo_redirect_write`: the frontend emits
        // `builtin`, the prelude rewrites most of those to `exec`.
        if func != "builtin" && func != "exec" {
            return None;
        }
        let (Some(IrExpr::Str(name, _)), Some(IrExpr::Array(items))) = (args.first(), args.get(1))
        else {
            return None;
        };
        if echo_cmd(name).unwrap_or(name) != "printf" {
            return None;
        }
        // One redirect, and it must be fd 1 opened for writing — the same
        // admission as `echo_redirect_write` (`>&`, `2>&1`, a second
        // redirect all keep the weld).
        let [one] = redirs else { return None };
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let (path, append) = match self.native_redirect_spec(one) {
            Some(NativeRedir::File {
                fd: 1,
                path,
                read: false,
                append,
            }) => (path, append),
            _ => {
                self.sides.truncate(sides);
                self.todo = todo;
                self.tmp = tmp;
                return None;
            }
        };
        let Some(fmt_expr) = items.first() else {
            self.sides.truncate(sides);
            self.todo = todo;
            self.tmp = tmp;
            return None;
        };
        // The format must be a static string the external counterpart was
        // measured against: dynamic formats cannot prove agreement, and a
        // dash-leading word is an option (`-v`, `--`), not a format.
        let IrExpr::Str(fmt, _) = fmt_expr else {
            self.sides.truncate(sides);
            self.todo = todo;
            self.tmp = tmp;
            return None;
        };
        if fmt.starts_with('-') || fmt.contains("%q") {
            self.sides.truncate(sides);
            self.todo = todo;
            self.tmp = tmp;
            return None;
        }
        let fmt_go = self.printf_fmt(fmt_expr);
        let fargs: Vec<String> = items[1..].iter().map(|a| self.expr_str(a)).collect();
        if self.todo != todo {
            self.sides.truncate(sides);
            self.todo = todo;
            self.tmp = tmp;
            return None;
        }
        // No trailing-newline strip: a redirect preserves every byte, while
        // a command substitution (`echo_printf_value`) strips them.
        let text = if fargs.is_empty() {
            format!("bprintfStr({fmt_go})")
        } else {
            format!("bprintfStr({fmt_go}, {})", fargs.join(", "))
        };
        self.sides.truncate(sides);
        self.flush_sides();
        let id = self.new_tmp();
        let lines = if append {
            let (fv, ev) = (format!("__awf{id}"), format!("__awe{id}"));
            let tail = if self.need_st { "; st = 0" } else { "" };
            let mut l = format!(
                "if {fv}, {ev} := os.OpenFile({path}, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0644); \
                 {ev} == nil {{ {fv}.WriteString({text}); {fv}.Close(){tail} }}"
            );
            if self.need_st {
                l.push_str(" else { st = 1 }");
            }
            vec![l]
        } else if self.need_st {
            let wv = format!("__we{id}");
            vec![format!(
                "if {wv} := os.WriteFile({path}, []byte({text}), 0644); {wv} != nil {{ st = 1 }} else {{ st = 0 }}"
            )]
        } else {
            vec![format!("os.WriteFile({path}, []byte({text}), 0644)")]
        };
        Some(lines)
    }

    /// A lone builtin/fs command's native statements, rendered
    /// speculatively (UNINDENTED, exactly like `chain_builtin`) for a
    /// stderr-discard context: the statement path's own dispatch
    /// (`exec_builtin_stmt`, then `fs_argcmd_stmt`), so a refusal lands
    /// exactly where the unredirected command would have gone. `None`
    /// rolls everything back and keeps the caller fallback. The status
    /// default mirrors `chain_builtin`.
    fn builtin_fs_speculative(&mut self, call_args: &[IrExpr]) -> Option<Vec<String>> {
        let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(_))) =
            (call_args.first(), call_args.get(1))
        else {
            return None;
        };
        let cmd = cmd.clone();
        let todo_before = self.todo;
        let need_st_before = self.need_st;
        let sides_before = self.sides.len();
        let tmp_before = self.tmp;
        // The status is assigned below, so it is unconditionally needed.
        self.need_st = true;
        let saved_depth = self.depth;
        self.depth = 0;
        let saved = std::mem::take(&mut self.out);
        let handled =
            self.exec_builtin_stmt(&cmd, call_args) || self.fs_argcmd_stmt(&cmd, call_args);
        let stmts = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        if !handled || self.todo != todo_before {
            self.todo = todo_before;
            self.need_st = need_st_before;
            self.sides.truncate(sides_before);
            self.tmp = tmp_before;
            return None;
        }
        self.need_st = true;
        let mut stmts = stmts;
        if !stmts.iter().any(|s| s.contains("os.Exit("))
            && !stmts.iter().any(|s| s.contains("st ="))
        {
            stmts.push("st = 0;".to_string());
        }
        Some(stmts)
    }

    /// `cmd 2>/dev/null` (and nothing else) in statement position, where
    /// `cmd` is a builtin or natively-lowered fs command: capture the same
    /// native statements the plain-statement path emits, UNINDENTED, exactly
    /// like `chain_builtin`, and reject the attempt wholesale on refusal —
    /// so a refusal leaves no partial render behind. `None` keeps the
    /// `exec.Command` weld below. The status default mirrors
    /// `chain_builtin`: builtins that never assign `st` (and a taken path
    /// that did not need it before) still leave `$?` at 0, as the weld's
    /// `st = runCmd(…)` would on success.
    fn discard_redirect_native(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        if redirects.is_empty() || !redirects.iter().all(null_stderr_spec) {
            return None;
        }
        let call_args = lone_exec_args(inner)?;
        self.builtin_fs_speculative(&call_args)
    }

    /// `eval "name=value …"` where every word is a plain assignment:
    /// lower each natively (see the `eval` arm). The words arrive as
    /// Interpolate pieces; a word counts iff it is exactly `lit(name=)` +
    /// value-parts with NO spaces, quotes, or substitutions beyond plain
    /// var reads and arithmetic (those need the shell). Returns the Go
    /// lines, or `None` to keep the `bash -c` fallback.
    fn eval_assign_native(&mut self, code: &IrExpr) -> Option<Vec<String>> {
        let words: Vec<&IrExpr> = match code {
            IrExpr::Array(items) => items.iter().collect(),
            single => vec![single],
        };
        if words.is_empty() {
            return None;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let fail = |r: &mut Render| {
            r.sides.truncate(sides);
            r.tmp = tmp;
            r.todo = todo;
            None
        };
        let mut lines = Vec::new();
        for w in words {
            let (name, val) = match w {
                IrExpr::Interpolate(parts) => {
                    let Some(InterpPart::Lit(first)) = parts.first() else {
                        return fail(self);
                    };
                    let Some(eq) = first.find('=') else {
                        return fail(self);
                    };
                    let nm = &first[..eq];
                    if nm.is_empty()
                        || !nm
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                        || nm.chars().next().map_or(true, |c| c.is_ascii_digit())
                    {
                        return fail(self);
                    }
                    if first[eq + 1..].chars().any(|c| {
                        c.is_whitespace()
                            || matches!(
                                c,
                                '\'' | '"' | '`' | ';' | '|' | '&' | '(' | ')' | '<' | '>'
                            )
                    }) {
                        return fail(self);
                    }
                    let mut rest = vec![InterpPart::Lit(first[eq + 1..].to_string())];
                    rest.extend(parts[1..].iter().cloned());
                    (nm.to_string(), IrExpr::Interpolate(rest))
                }
                _ => return fail(self),
            };
            let mut ok = true;
            if let IrExpr::Interpolate(parts) = &val {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        let plain = match x.as_ref() {
                            IrExpr::Call { func, .. } => {
                                func == "getVar"
                                    || func == "param"
                                    || func == "arith"
                                    || func == "arithEval"
                            }
                            IrExpr::Arith(_) => true,
                            IrExpr::Var(_, _)
                            | IrExpr::Ident(_)
                            | IrExpr::Str(_, _)
                            | IrExpr::Int(_) => true,
                            _ => false,
                        };
                        if !plain {
                            ok = false;
                            break;
                        }
                    }
                }
            }
            if !ok {
                return fail(self);
            }
            self.mark_written(&name);
            let m = self.go_ident(&name);
            let rhs = if self.is_num(&name) {
                self.expr_num(&val)
            } else {
                self.expr_str(&val)
            };
            if self.todo != todo {
                return fail(self);
            }
            let sync = self.sync_inline(&name);
            lines.push(format!("{m} = {rhs};{sync}"));
        }
        self.flush_sides();
        Some(lines)
    }

    /// True when an assignment RHS is pure (no child, no capture, no
    /// external command): arithmetic, literals, var reads, parameter
    /// expansion, and string concatenations thereof. Such a RHS writes no
    /// stderr, so stderr-discard redirects on the assignment are no-ops.
    fn is_pure_assign_rhs(e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_)
            | IrExpr::Str(_, _)
            | IrExpr::Bool(_)
            | IrExpr::Var(_, _)
            | IrExpr::Ident(_)
            | IrExpr::Arith(_) => true,
            IrExpr::Call { func, args } => {
                (func == "getVar" || func == "param" || func == "arith" || func == "arithEval")
                    && args.iter().all(Self::is_pure_assign_rhs)
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                Self::is_pure_assign_rhs(lhs) && Self::is_pure_assign_rhs(rhs)
            }
            IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
                InterpPart::Lit(_) => true,
                InterpPart::Expr(x) => Self::is_pure_assign_rhs(x),
            }),
            IrExpr::Ternary { cond, then, else_ } => {
                Self::is_pure_assign_rhs(cond)
                    && Self::is_pure_assign_rhs(then)
                    && Self::is_pure_assign_rhs(else_)
            }
            _ => false,
        }
    }

    /// `mapfile [-t] [-d delim] [array] < file` / `readarray …` — read the
    /// file's lines into the array with NO subprocess. Only one stdin-file
    /// redirect (a plain path or var); no bare-stdin form (it would read
    /// the terminal). REFUSE > GUESS: `-u` (another fd), `-O`/`-C`/`-s`/`-n`
    /// (origin/callback/skip/count), or anything the shape does not cover —
    /// the caller keeps the fallback.
    fn mapfile_redirect_lines(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> Option<Vec<String>> {
        let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(argv))) =
            (args.first(), args.get(1))
        else {
            return None;
        };
        if cmd != "mapfile" && cmd != "readarray" {
            return None;
        }
        if redirects.len() != 1 {
            return None;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let r = &redirects[0];
        let from_stdin_string: Option<String> =
            if r.mode == "r" && matches!(r.fd, None | Some(0)) {
                None
            } else if r.mode == "herestring" && matches!(r.fd, None | Some(0)) {
                // `mapfile <<< text`: the text plus the one newline a
                // herestring always feeds (even `<<< ""` feeds `"\n"`).
                let text = self.expr_str(&r.target.clone());
                if self.todo != todo {
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                    return None;
                }
                Some(format!("({text} + \"\\n\")"))
            } else if r.mode == "heredoc" && matches!(r.fd, None | Some(0)) {
                // A heredoc body is static text; it interpolates exactly
                // when unquoted.
                match &r.target {
                    IrExpr::Str(text, _) => {
                        if r.interpolate {
                            Some(format!("expand({})", Self::go_str(text)))
                        } else {
                            Some(Self::go_str(text))
                        }
                    }
                    _ => return None,
                }
            } else {
                return None;
            };
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let fail = |rr: &mut Render| {
            rr.sides.truncate(sides);
            rr.tmp = tmp;
            rr.todo = todo;
            None
        };
        let mut chomp = false;
        let mut delim = String::from("\n");
        let mut array: Option<String> = None;
        let mut it = argv.iter().peekable();
        while let Some(a) = it.next() {
            let IrExpr::Str(s, _) = a else {
                return fail(self);
            };
            if s == "-t" {
                chomp = true;
            } else if s == "-d" {
                match it.next() {
                    Some(IrExpr::Str(d, _)) => delim = d.clone(),
                    _ => return fail(self),
                }
            } else if let Some(d) = s.strip_prefix("-d") {
                delim = d.to_string();
            } else if s.starts_with('-') {
                return fail(self);
            } else if array.is_none() {
                array = Some(s.clone());
            } else {
                return fail(self);
            }
        }
        let var = array.unwrap_or_else(|| "MAPFILE".to_string());
        self.mark_arr(&var);
        let m = self.go_ident(&var);
        let sync = self.sync_inline(&var);
        self.flush_sides();
        self.need_st = true;
        let id = self.new_tmp();
        // A herestring/heredoc feeds literal text; a file is read.
        if let Some(text) = from_stdin_string {
            return Some(vec![format!(
                "{m} = splitLines({text}, {}, {chomp});{sync} st = 0;",
                Self::go_str(&delim)
            )]);
        }
        let path = self.expr_str(&r.target.clone());
        if self.todo != todo {
            return fail(self);
        }
        Some(vec![format!(
            "if __mfd{id}, __mfe{id} := os.ReadFile({path}); __mfe{id} == nil {{ \
             {m} = splitLines(string(__mfd{id}), {}, {chomp});{sync} st = 0 }} else {{ st = 1 }}",
            Self::go_str(&delim)
        )])
    }

    /// `while read [-r] VAR…; do …; done < FILE` — a native `bufio.Scanner`
    /// iteration whose BODY renders as Go statements. This is the shell's
    /// line-reading idiom, and forking a whole `bash -c` loop (the previous
    /// lowering) meant re-parsing and re-running the body as shell text: no
    /// scoping, no types, and one process per loop.
    ///
    /// `read`'s semantics are far wider than what this template reproduces,
    /// so REFUSE > GUESS — everything below keeps the fork:
    ///   * a missing `-r` (bash then eats backslash-newline continuations),
    ///   * any flag other than `-r` (`-n`/`-d`/`-p`/`-t`/`-u`/`-a`/`-s`),
    ///   * an `IFS` that is neither empty nor a SINGLE non-blank character,
    ///   * a non-literal `IFS`, or any other `VAR=v` prefix on `read`,
    ///   * a non-literal variable name, or an array/hash target,
    ///   * SEVERAL variables under a whitespace IFS (bash collapses runs
    ///     of IFS whitespace and lets the last variable take the remainder;
    ///     only the single-variable shape is reproduced here).
    fn while_read_redirect(
        &mut self,
        inner: &[IrStmt],
        redirects: &[crate::ir::IrRedirect],
    ) -> bool {
        let [IrStmt::While { cond, body }] = inner else {
            return false;
        };
        if redirects.len() != 1 {
            return false;
        }
        let r = &redirects[0];
        if r.mode != "r" || !matches!(r.fd, None | Some(0)) {
            return false;
        }
        let IrExpr::Call { func, args } = cond else {
            return false;
        };
        if func != "builtin" && func != "exec" {
            return false;
        }
        let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(argv))) = (args.first(), args.get(1))
        else {
            return false;
        };
        if cmd != "read" {
            return false;
        }
        // `read -r a b` — every flag is refused except `-r`, which is
        // REQUIRED: without it bash interprets backslashes, and this
        // template hands the line over verbatim.
        let mut dash_r = false;
        let mut names: Vec<String> = Vec::new();
        for a in argv {
            let IrExpr::Str(s, _) = a else {
                return false;
            };
            if s == "-r" {
                dash_r = true;
            } else if s.starts_with('-') {
                return false;
            } else {
                names.push(s.clone());
            }
        }
        if !dash_r || names.is_empty() {
            return false;
        }
        // `IFS=: read …` arrives as the command-scoped env prefix.
        let mut ifs: Option<String> = None;
        if let Some(IrExpr::Object(props)) = args.get(2) {
            for (k, v) in props {
                let IrExpr::Str(val, _) = v else {
                    return false;
                };
                if k != "IFS" {
                    return false;
                }
                ifs = Some(val.clone());
            }
        }
        // `None` = the default IFS (whitespace), `Some(None)` = `IFS=`
        // (no splitting at all), `Some(Some(c))` = split on one char.
        let sep: Option<Option<char>> = match ifs.as_deref() {
            None => Some(None),
            Some("") => Some(Some('\0')),
            Some(s) => {
                let mut cs = s.chars();
                match (cs.next(), cs.next()) {
                    (Some(c), None) if c != ' ' && c != '\t' && c != '\n' => Some(Some(c)),
                    _ => None,
                }
            }
        };
        let Some(sep) = sep else {
            return false;
        };
        if sep.is_none() && names.len() > 1 {
            return false;
        }
        for n in &names {
            if self.is_arr(n) || self.functions.contains(n) {
                return false;
            }
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let path = self.expr_str(&r.target.clone());
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            return false;
        }
        let id = self.new_tmp();
        let (rf, sc, ln) = (
            format!("__wrf{id}"),
            format!("__wsc{id}"),
            format!("__wln{id}"),
        );
        let run = format!("__run{}", self.new_tmp());
        self.flush_sides();
        self.need_st = true;
        self.emit(&format!(
            "if {rf}, __wre{id} := os.Open({path}); __wre{id} != nil {{ st = 1 }} else {{"
        ));
        self.depth += 1;
        self.emit(&format!("{sc} := bufio.NewScanner({rf})"));
        // bash's `read` has no line-length limit; Scanner's default token
        // cap is 64 KiB, so raise it to the same order as a shell variable.
        self.emit(&format!(
            "{sc}.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)"
        ));
        self.emit(&format!("{run} := false"));
        self.emit(&format!("for {sc}.Scan() {{"));
        self.depth += 1;
        self.emit(&format!("{run} = true"));
        self.emit(&format!("{ln} := {sc}.Text()"));
        self.emit_read_assigns(&names, &ln, sep, &id);
        for s in body {
            self.stmt(s);
        }
        self.depth -= 1;
        self.emit("}");
        // bash's `while` status is the last body command's, or 0 when the
        // body never ran — the same rule the generic While arm applies.
        self.emit(&format!("if !{run} {{ st = 0 }}"));
        self.emit(&format!("if {sc}.Err() != nil {{ st = 1 }}"));
        self.emit(&format!("{rf}.Close()"));
        self.depth -= 1;
        self.emit("}");
        true
    }

    /// The `read` assignments of one line: `names` (already validated) from
    /// the line text `ln`, under a resolved IFS (`sep`, see
    /// `while_read_redirect`).
    fn emit_read_assigns(
        &mut self,
        names: &[String],
        ln: &str,
        sep: Option<char>,
        id: &str,
    ) {
        // One literal-value store, typed by the variable's home: `any`
        // boxes the string, `int64` coerces it (bash's `read x` on an
        // arithmetic variable still stores the TEXT, but every read of it
        // is numeric — s2i is the same conversion those reads perform).
        // The write is counted as the STRING it really is (or as an arith
        // write for the int home), which is what lets an otherwise
        // string-only variable hoist as Go `string` instead of `any` plus
        // an `s2s(v)` at every read.
        let store = |this: &mut Self, n: &str, val: &str| {
            let m = this.go_ident(n);
            if this.declared_go_int(n) {
                this.mark_arith(n);
                this.emit(&format!("{m} = s2i({val});"));
            } else {
                this.mark_str_written(n);
                this.emit(&format!("{m} = {val};"));
            }
            let sync = this.sync_inline(n);
            if !sync.is_empty() {
                this.emit(&sync);
            }
        };
        match sep {
            // Default IFS: leading and trailing IFS whitespace is stripped
            // and the rest is the value (one variable takes the remainder).
            None => store(self, &names[0], &format!("strings.Trim({ln}, \" \\t\")")),
            // `IFS=` — no field splitting and no stripping at all.
            Some('\0') => store(self, &names[0], ln),
            // `IFS=c`: SplitN keeps the remainder (separators intact) in the
            // last field and yields nothing for the fields past the end,
            // which is exactly bash's `read a b c` assignment.
            Some(c) => {
                self.emit(&format!(
                    "__wrv{id} := strings.SplitN({ln}, {}, {})",
                    Self::go_str(&c.to_string()),
                    names.len()
                ));
                for (i, n) in names.iter().enumerate() {
                    let m = self.go_ident(n);
                    let int_home = self.declared_go_int(n);
                    if int_home {
                        self.mark_arith(n);
                    } else {
                        self.mark_str_written(n);
                    }
                    let zero = if int_home { "0" } else { "\"\"" };
                    self.emit(&format!("{m} = {zero};"));
                    let val = format!("__wrv{id}[{i}]");
                    if int_home {
                        self.emit(&format!(
                            "if len(__wrv{id}) > {i} {{ {m} = s2i({val}) }}"
                        ));
                    } else {
                        self.emit(&format!("if len(__wrv{id}) > {i} {{ {m} = {val} }}"));
                    }
                    let sync = self.sync_inline(n);
                    if !sync.is_empty() {
                        self.emit(&sync);
                    }
                }
            }
        }
    }

    /// `redirect(Arrow(body), [redirs])` in statement position, lowered
    /// natively when the body is exactly one external exec. Speculative:
    /// the render state is rolled back when it refuses.
    fn stmt_redirect_native(&mut self, args: &[IrExpr]) -> bool {
        let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
            (args.first(), args.get(1))
        else {
            return false;
        };
        if redirs.is_empty() {
            return false;
        }
        let todo = self.todo;
        let sides = self.sides.len();
        let tmp = self.tmp;
        let (cmd, argv) = match self.body_single_exec(body) {
            Some(x) => x,
            None => return false,
        };
        match self.native_redirect_lines(&cmd, &argv, redirs) {
            Some(lines) if self.todo == todo => {
                for l in lines {
                    self.emit(&l);
                }
                true
            }
            _ => {
                self.sides.truncate(sides);
                self.tmp = tmp;
                false
            }
        }
    }

    /// `capture(Arrow)` body → capture lowering.
    fn capture_arrow(&mut self, body: &[IrStmt]) -> String {
        // The core's value-producing ops (`$(echo … | grep -o PAT)` arrives
        // as `capture(Arrow[Expr(grepMatches(…)])`) ARE the command: the op
        // yields the string natively, with no subprocess. Same fast path as
        // capture_str — without it a `capture` CALL (the shape the core
        // emits for these lifts) fell through to the shell-text
        // reconstruction, which has no text for `grepMatches` and stubbed.
        //
        // `native = false`: this path's fallbacks below all trim (capCmd),
        // so its value-producing ops are asked for the trimmed form too.
        if let Some(v) = self.capture_value_op(body, false) {
            return v;
        }
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
        // `$(echo …)` / `$(printf …)`: the builtin's VALUE is a native
        // string expression (no subprocess at all) — same fast path as
        // capture_str.
        if let Some(v) = self.echo_printf_capture_value(body) {
            return v;
        }
        // `$(cmd <<< s)` / `$(cmd <<EOF … EOF)`: stdin fed from the string.
        if let Some(v) = self.stdin_str_capture(body) {
            return v;
        }
        // `$(wc -c|-l|-w|-m < f)`: the count, not a subprocess.
        if let Some(v) = self.wc_capture_body(body) {
            return v;
        }
        // `$(grep …)`: the same os.ReadFile+regexp scan the statement form
        // runs, answering the hits instead of the status.
        if let Some(v) = self.grep_capture_body(body) {
            return v;
        }
        // Same builtin policy as capture_str: a lone `echo`/`printf` exec's
        // natively when coreutils agrees with the builtin.
        if let Some((cmd, argv)) = self.body_single_exec_opt(body, true) {
            // Spread-aware (see capture_str).
            let call = Self::exec_cmd_call(&cmd, &argv);
            return format!("capCmd({call})");
        }
        if let Some(text) = self.cmd_text_stmts(body) {
            let env = self.env_lit_for(Some(&text));
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
        // `${!name…}` sliced is a bad substitution in bash (indirect
        // expansion admits no offset/length) — the shell aborts the script
        // with status 1, printing nothing for the enclosing command. Emit
        // the same abort (stderr is discarded by the gate; stdout and the
        // exit code are what must match).
        if op == "slice" && name.starts_with('!') {
            return "func() string { fmt.Fprintln(os.Stderr, \"bad substitution\"); os.Exit(1); return \"\" }()".to_string();
        }
        // Shell quoting inside the expansion: a backslash quotes the next
        // char (`${x//\"/\\\"}` matches `"` and substitutes `\"`). The
        // runtime matchers treat backslashes literally, so unescape here.
        // Only for the pattern-shaped ops (`#`/`%` patterns, `/` pattern
        // and replacement) and only for literal words — dynamic defaults
        // (`${x:-$y}`) expand at runtime through `expand`.
        let mut extra: Vec<String> = args[2..].iter().map(|a| self.expr_str(a)).collect();
        if matches!(op.as_str(), "#" | "##" | "%" | "%%" | "/" | "//") {
            for (i, a) in args[2..].iter().enumerate() {
                if let IrExpr::Str(s, _) = a {
                    if s.contains('\\') {
                        extra[i] = Self::go_str(&Self::unescape_pat(s));
                    }
                }
            }
        }
        // A plain variable name with a literal word: the expansion is
        // expressible in Go itself, so `pExp`/`pVal` never enter the picture.
        if !matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@") {
            if let Some(text) = self.param_native(op, name, &args[2..]) {
                return text;
            }
            // `param_native` names only PLAIN vars; a subscripted
            // `${arr[<literal>]}` is the same single element read, and
            // `braced_plain` spells it natively (`arrIdx`).
            if op.is_empty() && args.len() == 2 {
                if let Some(text) = self.braced_plain(name) {
                    return text;
                }
            }
        }
        match op.as_str() {
            "@" | "*" => {
                self.need_vars = true;
                "strings.Join(argsList(), \" \")".to_string()
            }
            _ => {
                // `${arr[@]...}` — the `@` arrives as the first extra arg
                let name = if matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@") {
                    format!("{name}[@]")
                } else {
                    name.clone()
                };
                self.need_vars = true;
                let extra = if name.ends_with("[@]") {
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

    /// `${name}`, `${name:-word}` and their siblings spelled in Go.
    ///
    /// `None` for every shape `pExp` has to answer at run time: a subscripted
    /// / array / associative name, an indirect (`!`) name, and any word that
    /// still carries shell syntax — `$`, a backtick, a backslash or a leading
    /// `~` are text `expand` re-parses, and a Go string literal would freeze
    /// them. Every arm below mirrors `pExp` byte for byte, so the value is
    /// unchanged; what disappears is the `pVal` lookup into the runtime store
    /// and the helper bundle (`pExp` → `pVal` → `expand` → `capRun`) it drags
    /// into the program.
    fn param_native(&mut self, op: &str, name: &str, extra: &[IrExpr]) -> Option<String> {
        let mut chars = name.chars();
        let plain = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !self.is_arr(name)
            && !self.is_assoc(name);
        if !plain {
            return None;
        }
        let word = |extra: &[IrExpr]| -> Option<String> {
            let [w] = extra else { return None };
            let IrExpr::Str(s, _) = w else { return None };
            if s.contains('$') || s.contains('`') || s.contains('\\') || s.starts_with('~') {
                return None;
            }
            Some(Self::go_str(s))
        };
        // Decide BEFORE touching the render state: `getvar_str` marks the
        // variable read (which drops its `_ = x` dead-var guard), so asking
        // for a value that a declined op throws away leaves the guard gone
        // and Go rejects the program for an unused variable.
        let w = match op {
            ":-" | "-" | "+" => Some(word(extra)?),
            "" | "^^" | ",," | "^" | "," | "len" | "basename" | "dirname" => None,
            _ => return None,
        };
        let v = self.getvar_str(name);
        match op {
            "" => Some(v),
            // case ops: `pExp` folds one BYTE for `^`/`,`, the whole string
            // for `^^`/`,,` — mirrored exactly (bytes, not runes).
            "^^" => Some(format!("strings.ToUpper({v})")),
            ",," => Some(format!("strings.ToLower({v})")),
            "^" | "," => {
                let f = if op == "^" { "ToUpper" } else { "ToLower" };
                Some(format!(
                    "func() string {{ v := {v}; if v == \"\" {{ return v }}; return strings.{f}(v[:1]) + v[1:] }}()"
                ))
            }
            // `${#x}` counts RUNES (runeLen), `${x##*/}`-style patterns and
            // `slice` stay with the runtime's glob matcher.
            "len" => Some(format!("strconv.FormatInt(runeLen({v}), 10)")),
            "basename" => Some(format!("filepath.Base({v})")),
            "dirname" => Some(format!("filepath.Dir({v})")),
            ":-" | "-" => {
                let w = w?;
                Some(format!(
                    "func() string {{ v := {v}; if v == \"\" {{ return {w} }}; return v }}()"
                ))
            }
            "+" => {
                let w = w?;
                Some(format!(
                    "func() string {{ v := {v}; if v == \"\" {{ return \"\" }}; return {w} }}()"
                ))
            }
            _ => None,
        }
    }

    /// `join(list, sep)` Call — sep defaults to a space.
    fn join_call(&mut self, args: &[IrExpr]) -> String {
        let Some(list) = args.first() else {
            return self.sh2_stub("join");
        };
        let sep = match args.get(1) {
            Some(s) => self.expr_str(s),
            None => "\" \"".to_string(),
        };
        match list {
            IrExpr::Call { func, args } if func == "param" => {
                if let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) =
                    (args.first(), args.get(1))
                {
                    self.need_vars = true;
                    // `${arr[@]...}` — the `@` arrives as the first extra
                    // arg; pList joins on the `[@]`-suffixed name exactly
                    // like param_call does, so the join sees every element
                    // instead of element zero.
                    let (name, extra): (String, Vec<String>) =
                        if matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@" || a == "*")
                        {
                            (
                                format!("{name}[@]"),
                                args[3..].iter().map(|a| self.expr_str(a)).collect(),
                            )
                        } else {
                            (
                                name.clone(),
                                args[2..].iter().map(|a| self.expr_str(a)).collect(),
                            )
                        };
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
                    return format!("strings.Join(pList(\"\", {}), {sep})", Self::go_str(name));
                }
                self.sh2_stub("join")
            }
            other => {
                if self.any_is_go_string(other) {
                    // already a Go string — the coercion is a no-op
                    self.expr_any(other)
                } else {
                    let s = self.expr_any(other);
                    format!("s2s({s})")
                }
            }
        }
    }

    /// True when `expr_any` already yields a Go `string`, so a string
    /// context needs no `s2s` coercion around it. Deliberately narrow —
    /// only the shapes whose emitted Go type is a string on EVERY path
    /// (a helper that can stub out to an int must not qualify).
    fn any_is_go_string(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Str(..) | IrExpr::Interpolate(_) => true,
            IrExpr::Json(serde_json::Value::String(_)) => true,
            IrExpr::BinOp { op, .. } => *op == crate::ir::BinOpKind::Concat,
            // param_call renders pExp/strings.Join — strings — but only on
            // the arm where op and name are literal; the stub arm is an int.
            IrExpr::Call { func, args } => match func.as_str() {
                "param" => {
                    matches!(args.first(), Some(IrExpr::Str(..)))
                        && matches!(args.get(1), Some(IrExpr::Str(..)))
                }
                "arrayItems" => matches!(args.first(), Some(IrExpr::Str(..))),
                // `arrayLen` renders `strconv.FormatInt(arrLen(m), 10)` — a
                // Go string on both paths (the non-literal name stubs to the
                // int 0, which is why the literal-name test is here).
                "arrayLen" => matches!(args.first(), Some(IrExpr::Str(..))),
                _ => false,
            },
            _ => false,
        }
    }

    fn sh2_stub(&mut self, note: &str) -> String {
        self.mark_todo(&format!("{note}"));
        "0".to_string()
    }

    /// `grepMatches(text, pat, flags)` as a Go string — the matches, one
    /// per line, exactly what `grep -o` writes. `None` keeps the stub.
    fn grep_matches_value(&mut self, args: &[IrExpr]) -> Option<String> {
        if args.is_empty() {
            return None;
        }
        let re = self.grep_matches_re(args)?;
        let text = self.expr_str(&args[0]);
        Some(format!(
            "strings.Join(regexp.MustCompile({re}).FindAllString({text}, -1), \"\\n\")"
        ))
    }

    /// `$( … )` whose body is exactly ONE value-producing lifted op: the
    /// op stands in for the whole command, so the capture is its native
    /// value. Only side-effect-free ops qualify — anything else keeps the
    /// subprocess path.
    fn capture_value_op(&mut self, body: &[IrStmt], native: bool) -> Option<String> {
        // `$(a | b)` also arrives as a statement `Pipeline` NODE (the
        // frontends emit the call form for some inputs and the node for
        // others). Same analysis, same native `capPipe` — without this arm
        // the node form fell through to the `bash -c` reconstruction even
        // though every stage was a plain command.
        if let [IrStmt::Pipeline {
            stages,
            capture: None,
            ..
        }] = body
        {
            if let Some(v) = self.pipeline_capture_nodes(stages, native) {
                return Some(v);
            }
        }
        // A group around the command (`$( ( a | b ) )`) is not a command of
        // its own: unwrap it and re-run the same analysis.
        if let [IrStmt::Subshell(inner)] | [IrStmt::Block(inner)] = body {
            if let Some(v) = self.capture_value_op(inner, native) {
                return Some(v);
            }
        }
        let [IrStmt::Expr(IrExpr::Call { func, args })] = body else {
            return None;
        };
        match func.as_str() {
            "grepMatches" => self.grep_matches_value(args),
            // `$(cmd | cmd …)`: the pipeline is the command, and every stage
            // is a real process — no shell text, no env literal. Declines
            // (None) when a stage is not a plain command, which keeps the
            // `bash -c` reconstruction in the caller.
            "pipeline" => self.pipeline_capture(args, native),
            _ => None,
        }
    }

    /// The Go string literal for the `grepMatches` regex body, or `None`
    /// when the pattern (or flags) is not a static string — the lift only
    /// fires on static patterns, so `None` means "keep the stub".
    fn grep_matches_re(&mut self, args: &[IrExpr]) -> Option<String> {
        let pat = match args.get(1) {
            Some(IrExpr::Str(p, _)) => p.clone(),
            _ => return None,
        };
        let flags = match args.get(2) {
            Some(IrExpr::Str(s, _)) => s.clone(),
            _ => String::new(),
        };
        Some(Self::go_str(&grep_re_body(&pat, &flags)))
    }

    /// The env literal for `bash -c` calls: every written scalar var.
    fn env_lit(&mut self) -> String {
        self.env_lit_for(None)
    }

    /// The env literal for a `bash -c` child whose SCRIPT TEXT is known:
    /// only the written vars that text reads.
    ///
    /// A reconstructed command text names every variable it can reach, so
    /// handing the child the whole written namespace is marshalling for a
    /// process that will not look at most of it — and each entry an `any`
    /// var contributes is another `s2s` boxing coercion. `None`, or a text
    /// whose reads cannot be enumerated statically (see `sh_text_refs`),
    /// keeps the complete list: the filter may only ever shrink an env the
    /// child provably does not need (REFUSE > GUESS).
    ///
    /// `eval`/`source` callers must pass `None` — the code they run is not
    /// in the text, so every var is reachable.
    fn env_lit_for(&mut self, script: Option<&str>) -> String {
        let refs: Option<BTreeSet<String>> = script.and_then(sh_text_refs);
        let mut parts = Vec::new();
        let written: Vec<String> = self.written.iter().cloned().collect();
        for v in &written {
            if refs.as_ref().is_some_and(|r| !r.contains(v)) {
                continue;
            }
            if v == "?" || self.is_arr(v) || v.starts_with('#') {
                continue;
            }
            // a user-function name is not a shell variable at all — hoisting
            // it put `fn=0x…` (the func value `%v`-formatted) into the child
            // env, which is never what bash would export.
            if self.functions.contains(v) {
                continue;
            }
            if !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let m = self.go_ident(v);
            // Choose the conversion by hoisted type, exactly as `getvar_str`
            // does: a string var needs none, an int64 var is `strconv`'s
            // decimal (native, and what the rest of the renderer emits for
            // it), and only an `any` var still needs the boxing coercion.
            let val = if self.declared_go_string(v) {
                m
            } else if self.declared_go_int(v) {
                format!("strconv.FormatInt({m}, 10)")
            } else {
                format!("s2s({m})")
            };
            parts.push(format!("{} + {val}", Self::go_str(&format!("{v}="))));
        }
        format!("[]string{{{}}}", parts.join(", "))
    }

    /// Render-time expansion of `$var`/`${var}`/`$1`/`$(…)` inside a
    /// literal string → a Go string expression.
    fn expand_str(&mut self, s: &str) -> String {
        // Outer quotes are syntax, not data (`"$HOME/x"` expands the
        // inside; the quotes themselves vanish).
        let s = {
            let t = s.trim();
            if t.len() >= 2
                && ((t.starts_with('"') && t.ends_with('"'))
                    || (t.starts_with('\'') && t.ends_with('\'')))
            {
                &t[1..t.len() - 1]
            } else {
                t
            }
        };
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
                        let env = self.env_lit_for(Some(&cmd));
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
        if let Some(v) = self.braced_plain(inner) {
            return v;
        }
        self.need_vars = true;
        format!("pExp(\"\", {})", Self::go_str(inner))
    }

    /// `${name}` and `${name[<literal index>]}` — the expansions whose value
    /// is ONE read of the same store `pVal` consults (`vars`, then the
    /// environment), so no run-time interpreter has to DECOMPOSE the name.
    ///
    /// Everything the interpreter exists for keeps it (REFUSE > GUESS): any
    /// operator (`${x:-d}`, `${#x}`, `${x%p}`), an indirect `!name`, an
    /// `[@]`/`[*]` join, and a subscript that is itself an expansion.
    fn braced_plain(&mut self, inner: &str) -> Option<String> {
        if inner.is_empty() {
            return None;
        }
        // `${2}` — a positional read, never a vars-map key.
        if inner.chars().all(|c| c.is_ascii_digit()) {
            return Some(self.getvar_str(inner));
        }
        let mut chars = inner.chars();
        let plain_ident = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        // `pVal` decomposes `name[k]` to an element; `getvar_str` on the
        // bare name would emit the whole slice instead. Only a LITERAL
        // index on a known array can be spelled natively.
        if let Some((base, key)) = inner.split_once('[').and_then(|(b, r)| r.strip_suffix(']').map(|k| (b, k))) {
            let mut bc = base.chars();
            let base_ident = matches!(bc.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                && bc.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !base_ident || key.is_empty() {
                return None;
            }
            // Only a var this render hoists has a Go home. A shell-supplied
            // array (PIPESTATUS, which the runtime maintains in `vars`) has
            // none, and naming it would emit an undefined identifier.
            if !self.declared(base) {
                return None;
            }
            // Same precedence as the `arrayIndex` arm: an assoc write marks
            // the base in BOTH sets (the pre-pass counts an indexed target
            // as an array), so `is_assoc` decides first.
            //
            // An assoc key is the key's STRING value, never an index; a
            // numeric one stays with the interpreter, whose decomposition
            // tries the integer form first.
            if self.is_assoc(base) {
                if key.chars().all(|c| c.is_ascii_digit())
                    || key.contains(['$', '`', '\\', '['])
                {
                    return None;
                }
                let m = self.ident_of(base);
                return Some(format!("s2s(assocIdx({m}, {}))", Self::go_str(key)));
            }
            if !key.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let m = self.ident_of(base);
            let k: i64 = key.parse().ok()?;
            return Some(format!("s2s(arrIdx({m}, {k}))"));
        }
        if !plain_ident {
            return None;
        }
        // A whole array read under its bare name is element zero (bash);
        // `getvar_str` would emit the slice. Leave both array stores to
        // the interpreter, which owns that rule.
        if self.is_arr(inner) || self.is_assoc(inner) {
            return None;
        }
        Some(self.getvar_str(inner))
    }

    // ── shell-text reconstruction (for bash -c fallbacks) ────────────

    fn cmd_text_stmts(&mut self, stmts: &[IrStmt]) -> Option<String> {
        let mut parts = Vec::new();
        let mut tails = String::new();
        for s in stmts {
            match self.cmd_text_stmt(s) {
                Some(t) => {
                    // A statement's text may still end in a heredoc BODY
                    // (the `redirect` CALL form appends it inline). Hold it
                    // aside: `;`-joining is fine, but a caller that welds a
                    // pipeline may put ` | …` right after this text, and a
                    // body must never precede that. Re-appended below.
                    let (h, b) = split_heredoc_tail(&t);
                    parts.push(h);
                    tails.push_str(&b);
                }
                None => {
                    if std::env::var_os("SH2_GO_DEBUG").is_some() {
                        eprintln!("SH2DEBUG cmd_text_stmt refuse: {}", debug_stmt(s));
                    }
                    return None;
                }
            }
        }
        let joined = parts.join("; ");
        if tails.is_empty() {
            Some(joined)
        } else {
            Some(format!("{joined}{tails}"))
        }
    }

    fn cmd_text_stmt(&mut self, s: &IrStmt) -> Option<String> {
        match s {
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
                // `(( i++ ))` / `(( i += 2 ))` arrive as an Assign whose
                // value is the SELF-REFERENTIAL arithmetic node. The
                // arithmetic COMMAND is the faithful shell text: writing
                // `i=$((i++))` instead would store the OLD value, so the
                // counter never moves and a `while (( i < N ))` loop never
                // ends. `(( … ))` reproduces bash's side effect exactly.
                if t.indices.is_empty() {
                    if let IrExpr::Arith(a) = expr {
                        let selfref = match a.as_ref() {
                            crate::ir::ArithAst::IncDec { var, .. }
                            | crate::ir::ArithAst::Assign { var, .. } => *var == t.var,
                            _ => false,
                        };
                        if selfref {
                            let text = self.cmd_text_arith(a)?;
                            return Some(format!("(( {text} ))"));
                        }
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
            IrStmt::Declare { vars, init, .. } => {
                let d = vars.first()?;
                match init {
                    Some(e) => {
                        let v = self.cmd_text_expr(e)?;
                        Some(format!("{}={}", d.name, v))
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
                        let mut heads = Vec::new();
                        let mut tails = String::new();
                        for st in stages {
                            // A stage's text keeps its own heredoc bodies
                            // trailing (NUL-marked) — hoist them past the
                            // ` | ` that will follow this stage.
                            let t = self.cmd_text_stmts(st)?;
                            let (h, b) = split_heredoc_tail(&t);
                            heads.push(h);
                            tails.push_str(&b);
                        }
                        format!("{}{tails}", heads.join(" | "))
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
                let mut bodies = String::new();
                for r in redirects {
                    with.push(' ');
                    with.push_str(&self.redirect_spec_text(r)?);
                    if let Some(b) = heredoc_body_text(r) {
                        bodies.push_str(&b);
                    }
                }
                // Bodies trail the complete statement text; a NUL marker
                // keeps them apart so `split_heredoc_tail` can hoist them
                // past a ` | ` a welding caller appends.
                if bodies.is_empty() {
                    Some(with)
                } else {
                    Some(format!("{with}\u{0}{bodies}"))
                }
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
            } => self.exec_stmt_text(cmd, args, capture, redirects, env),
            IrStmt::Require(_) | IrStmt::RawText(_) => None,
            IrStmt::Label(_) | IrStmt::Goto(_) => None,
            IrStmt::Shaderize { .. } => None,
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

    /// The core's `Exec` node as shell text: `cmd args…`, with `VAR=v`
    /// command-scoped env prefixes, the redirect list, and a `var=$(…)`
    /// wrapper when the statement captures. This is the reconstruction the
    /// bash -c fallbacks (`redirRun`/`capRun`) and `cmd_text_stmt` share —
    /// refuse (None) rather than guess an unprintable word.
    fn exec_stmt_text(
        &mut self,
        cmd: &IrExpr,
        args: &[IrExpr],
        capture: &Option<String>,
        redirects: &[IrExpr],
        env: &[(String, IrExpr)],
    ) -> Option<String> {
        let c = self.cmd_text_expr(cmd)?;
        let mut words = vec![cmd_quote(cmd, &c)];
        for a in args {
            let t = self.cmd_text_expr(a)?;
            words.push(cmd_quote(a, &t));
        }
        let mut line = words.join(" ");
        if !env.is_empty() {
            let mut pre = Vec::new();
            for (k, v) in env {
                let t = self.cmd_text_expr(v)?;
                pre.push(format!("{k}={}", cmd_quote(v, &t)));
            }
            line = format!("{} {line}", pre.join(" "));
        }
        if !redirects.is_empty() {
            let mut grouped = format!("( {line} )");
            for r in redirects {
                grouped.push(' ');
                grouped.push_str(&self.redirect_text(r)?);
            }
            line = grouped;
        }
        Some(match capture {
            Some(name) => format!("{name}=$({line})"),
            None => line,
        })
    }

    /// `IrStmt::Exec` — the core's command statement. Native
    /// `runCmd`/`capCmd` when the argv is expressible without a shell;
    /// otherwise a bash -c reconstruction (the catalogued last-resort tier).
    fn stmt_exec(
        &mut self,
        cmd: &IrExpr,
        args: &[IrExpr],
        capture: &Option<String>,
        redirects: &[IrExpr],
        env: &[(String, IrExpr)],
    ) {
        // The `exec` Call shape is (cmd, [argv…]) — synthesize it so the
        // tested builtin/function/argv paths apply unchanged.
        let mut call_args = Vec::with_capacity(args.len() + 1);
        call_args.push(cmd.clone());
        call_args.push(IrExpr::Array(args.to_vec()));
        if redirects.is_empty() && env.is_empty() {
            if capture.is_none() {
                if let IrExpr::Str(name, _) = cmd {
                    if self.exec_builtin_stmt(name, &call_args) {
                        return;
                    }
                    if self.fs_argcmd_stmt(name, &call_args) {
                        return;
                    }
                    if self.functions.contains(name) {
                        self.call_function(name, &call_args);
                        return;
                    }
                }
            }
            // `x=$(echo …)` / `x=$(printf …)` / `x=$(mktemp …)` as an
            // `Exec`-statement capture: the builtin's VALUE, not a
            // subprocess — the same lowering the `capture` paths use.
            if let Some(var) = capture {
                if let IrExpr::Str(name, _) = cmd {
                    if name == "mktemp" || echo_cmd(name).is_some() {
                        let todo = self.todo;
                        if let Some(v) = self.builtin_capture_value(name, args) {
                            if self.todo == todo {
                                self.flush_sides();
                                self.mark_str_written(var);
                                let m = self.go_ident(var);
                                let sync = self.sync_inline(var);
                                self.emit(&format!("{m} = {v};{sync}"));
                                return;
                            }
                        }
                        self.todo = todo;
                    }
                }
            }
            if let Some((c, argv)) = self.exec_cmd_argv(&call_args) {
                let call = Self::exec_cmd_call(&c, &argv);
                self.flush_sides();
                match capture {
                    Some(var) => {
                        self.mark_str_written(var);
                        let m = self.go_ident(var);
                        let sync = self.sync_inline(var);
                        self.emit(&format!("{m} = capCmd({call});{sync}"));
                    }
                    None => {
                        self.emit(&format!("st = runCmd({call});"));
                        self.need_st = true;
                    }
                }
                return;
            }
        }
        // `cat <<EOF … EOF` / `cat <<< s` as a plain STATEMENT: with no FILE
        // operand cat copies its stdin byte for byte, so the command is the
        // text the redirect already holds — formatting it is the lowering.
        if capture.is_none() && env.is_empty() && self.stdin_cat_stmt(cmd, args, redirects) {
            return;
        }
        // `x=$(cmd <<< s)` / `x=$(cmd <<EOF … EOF)` as an Exec-statement
        // capture: stdin fed from the string, output captured.
        if self.stdin_str_exec_capture(cmd, args, capture, redirects, env) {
            return;
        }
        // `x=$(wc -c|-l|-w|-m < f)` as an `Exec`-statement capture: the
        // count, not a subprocess — the same lowering the `capture` paths
        // use. Exactly one stdin-file redirect and no env prefix.
        if env.is_empty() {
            if let (Some(var), IrExpr::Str(name, _), [one]) = (capture, cmd, redirects) {
                if name == "wc" {
                    if let IrExpr::Object(props) = one {
                        let (mut fd_ok, mut mode_ok, mut target) = (false, false, None);
                        for (k, v) in props {
                            match k.as_str() {
                                "fd" => {
                                    if let IrExpr::Int(0) = v {
                                        fd_ok = true;
                                    }
                                }
                                "mode" => {
                                    if let IrExpr::Str(s, _) = v {
                                        if s == "r" {
                                            mode_ok = true;
                                        }
                                    }
                                }
                                "target" => target = Some(v.clone()),
                                _ => {}
                            }
                        }
                        let missing_fd = !props.iter().any(|(k, _)| k == "fd");
                        if mode_ok && target.is_some() && (fd_ok || missing_fd) {
                            let todo = self.todo;
                            if let Some(v) =
                                self.wc_capture_value(name, args, &target.unwrap())
                            {
                                if self.todo == todo {
                                    self.flush_sides();
                                    self.mark_str_written(var);
                                    let m = self.go_ident(var);
                                    let sync = self.sync_inline(var);
                                    self.emit(&format!("{m} = {v};{sync}"));
                                    return;
                                }
                            }
                            self.todo = todo;
                        }
                    }
                }
            }
        }
        if let Some(text) = self.exec_stmt_text(cmd, args, capture, redirects, env) {
            let envf = self.env_lit_for(Some(&text));
            self.flush_sides();
            match capture {
                Some(var) => {
                    self.mark_str_written(var);
                    let m = self.go_ident(var);
                    let sync = self.sync_inline(var);
                    self.emit(&format!(
                        "{m} = capRun({}, {envf});{sync}",
                        Self::go_str(&text)
                    ));
                }
                None => {
                    self.emit(&format!("st = redirRun({}, {envf});", Self::go_str(&text)));
                    self.need_st = true;
                }
            }
            return;
        }
        self.mark_todo("stmt Exec");
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
            // `i = …` / `i += …` — the assignment is bash arithmetic, so the
            // text is the same assignment.
            crate::ir::ArithAst::Assign { var, op, rhs } => {
                Some(format!("{var}{op}{}", self.cmd_text_arith(rhs)?))
            }
            // `i++` / `++i` — delta is ±1, so the text is `i++`/`i--` (or
            // the prefix form), never `i+=1` (the value differs).
            crate::ir::ArithAst::IncDec { var, delta, prefix } => {
                let op = if *delta >= 0 { "++" } else { "--" };
                if *prefix {
                    Some(format!("{op}{var}"))
                } else {
                    Some(format!("{var}{op}"))
                }
            }
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
            IrExpr::Call { func, args } if func == "exec" => {
                let cmd_expr = args.first()?;
                let cmd = self.cmd_text_expr(cmd_expr)?;
                let mut parts = vec![cmd_quote(cmd_expr, &cmd)];
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        let text = self.cmd_text_expr(it)?;
                        parts.push(cmd_quote(it, &text));
                    }
                }
                Some(parts.join(" "))
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
                Some(texts.join(" | "))
            }
            IrExpr::Call { func, args } if func == "capture" => {
                // `$( … )` nested inside a reconstructed pipeline/redirect:
                // the body's shell text in a command substitution. Without
                // this arm a capture holding the process-substitution
                // lowering (mktemp + redirects + the command) had no text
                // and poisoned the whole enclosing statement.
                let Some(IrExpr::Arrow(body)) = args.first() else {
                    return None;
                };
                let t = self.cmd_text_stmts(body)?;
                Some(format!("$({t})"))
            }
            IrExpr::Call { func, args } if func == "test" => {
                let s = match args.first() {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return None,
                };
                Some(format!("[ {s} ]"))
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
                let mut bodies = String::new();
                for r in redirs {
                    let rt = self.redirect_text(r)?;
                    out.push(' ');
                    out.push_str(&rt);
                    // Heredoc bodies follow the complete command (never
                    // inline — a closing paren or a pipe would break them).
                    if let IrExpr::Object(props) = r {
                        let mode = props
                            .iter()
                            .find(|(k, _)| k == "mode")
                            .and_then(|(_, v)| match v {
                                IrExpr::Str(s, _) => Some(s.as_str()),
                                _ => None,
                            });
                        let target = props
                            .iter()
                            .find(|(k, _)| k == "target")
                            .map(|(_, v)| v);
                        if matches!(mode, Some("heredoc") | Some("heredoc-tabs")) {
                            if let Some(IrExpr::Str(s, _)) = target {
                                bodies.push_str(&format!("\n{s}\nEOF"));
                            }
                        }
                    }
                }
                out.push_str(&bodies);
                Some(out)
            }
            IrExpr::Call { func, args } if func == "subshell" || func == "block" => {
                // `( body )` / `{ body }` groups in an `and`/`or`/capture
                // operand — reconstruct as a subshell group. A heredoc body
                // inside must stay outside the closing paren.
                let Some(IrExpr::Arrow(body)) = args.first() else {
                    return None;
                };
                let t = self.cmd_text_stmts(body)?;
                let (h, b) = split_heredoc_tail(&t);
                Some(format!("( {h} ){b}"))
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
                // `param(slice, NAME, …)` is the core's array/string slice
                // spelling: `${NAME[@]}` / `${NAME:off:len}`, never the
                // juxtaposed `NAMEslice…` the generic path below builds.
                // Array-ness decides the form (`${a[@]:1}` vs `${s:1}`) —
                // the core uses one encoding for both.
                if op == "slice" {
                    if matches!(extra.first().map(String::as_str), Some("@") | Some("*")) {
                        let sigil = &extra[0];
                        let rest = extra[1..].join("");
                        if rest.is_empty() {
                            return Some(format!("${{{name}[{sigil}]}}"));
                        }
                        return Some(format!("${{{name}[{sigil}]:{rest}}}"));
                    }
                    if !extra.is_empty() {
                        if self.is_arr(name) {
                            return Some(format!("${{{name}[@]:{}}}", extra.join(":")));
                        }
                        return Some(format!("${{{name}:{}}}", extra.join(":")));
                    }
                    return Some(format!("${{{name}}}"));
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
                // `$(…)` — nested command substitution inside an
                // expression. The body is an Arrow (statement list), the
                // same shape the `capture` Call arm handles below; rendering
                // it as a bare expression refused every nested
                // `$(echo "… $(…)")` (063_03_nested_command_substitutions).
                let t = match expr.as_ref() {
                    IrExpr::Arrow(body) => self.cmd_text_stmts(body)?,
                    other => self.cmd_text_expr(other)?,
                };
                Some(format!("$({t})"))
            }
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
            // The process-substitution lowering wraps each side of its
            // `and`/`or` in an Arrow, and the `and`/`or` itself can sit in
            // word position (`… | grep -f <(…)` puts one in a pipeline
            // stage). Both are command lists: a brace group is the shell
            // text for them.
            IrExpr::Arrow(body) => {
                let t = self.cmd_text_stmts(body)?;
                Some(format!("{{ {t}; }}"))
            }
            IrExpr::Call { func, args } if func == "and" || func == "or" => {
                let sep = if func == "and" { " && " } else { " || " };
                let mut parts = Vec::new();
                for a in args {
                    parts.push(self.cmd_text_expr(a)?);
                }
                Some(parts.join(sep))
            }
            // `$@` / `$*` / `$N` in word position. cmd_quote double-quotes
            // the result, so `"$@"` reaches bash with its quoting intact.
            IrExpr::Call { func, args } if func == "listVar" => {
                let name = match args.first() {
                    Some(IrExpr::Str(n, _)) => n.clone(),
                    _ => return None,
                };
                match name.as_str() {
                    "@" => Some("$@".to_string()),
                    "*" => Some("$*".to_string()),
                    n if n.chars().all(|c| c.is_ascii_digit()) => Some(format!("${n}")),
                    _ => Some(format!("${{{name}[@]}}")),
                }
            }
            // `split(x)` is bash's default word-splitting: in UNQUOTED shell
            // text that is exactly what the bare word does, so the operand's
            // text is the reconstruction.
            IrExpr::Call { func, args } if func == "split" => match args.first() {
                Some(a) => self.cmd_text_expr(a),
                None => Some(String::new()),
            },
            _ => {
                if std::env::var_os("SH2_GO_DEBUG").is_some() {
                    eprintln!("SH2DEBUG cmd_text_expr refuse: {}", debug_expr(e));
                }
                None
            }
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

    /// A `test` command's argv → native Go bool expression, via the same
    /// `test_render` the `[ ]` conditions use. Words that would not survive
    /// space-splitting (whitespace, quotes, `$`…) are single-quoted; every
    /// other word — operators (`=`, `-f`), flags, plain operands — stays
    /// bare so the parser still recognizes it. `None` keeps the
    /// `/usr/bin/test` exec. An empty operand has no text form (it would
    /// vanish from the join), so it refuses too. A dynamic word is only
    /// admissible as exactly one simple reference (`$v`, `${v}`, `$1`,
    /// `$?`, …) — anything composed (`$v/suffix`, `pre$v`) has no operand
    /// form `test_operand` resolves faithfully, so it keeps the exec.
    fn test_argv_bool(&mut self, argv: &[IrExpr]) -> Option<String> {
        let mut words = Vec::with_capacity(argv.len());
        for a in argv {
            match a {
                IrExpr::Str(s, _) => {
                    if s.is_empty() {
                        return None;
                    }
                    if s.chars().any(|c| {
                        c.is_whitespace()
                            || matches!(c, '\'' | '"' | '\\' | '$' | '`')
                    }) {
                        words.push(quote_sh(s));
                    } else {
                        words.push(s.clone());
                    }
                }
                _ => {
                    let t = self.cmd_text_expr(a)?;
                    if is_simple_test_ref(&t) {
                        words.push(t);
                        continue;
                    }
                    // A single `${…}` expansion without nesting resolves
                    // through `test_operand` → `expand_braced` — exactly the
                    // general expansion path — so it is admissible as-is
                    // (`${MYVAR+set}`, `${x[0]}`…; see `test_operand`).
                    if t.starts_with("${") && t.ends_with('}') {
                        let inner = &t[2..t.len() - 1];
                        if inner.is_empty() || inner.contains('{') || inner.contains('}') {
                            return None;
                        }
                        words.push(t);
                        continue;
                    }
                    // A fully static word is a literal operand — quote it
                    // exactly like a `Str` item above. REFUSE > GUESS:
                    // anything that could have expanded or reparsed in the
                    // shell keeps the exec — `$`, backticks, backslashes
                    // (expansion syntax), single quotes (quoting is already
                    // resolved), glob metachars (an unquoted `*` expands to
                    // filenames), and the test parser's own metacharacters
                    // (`!`, `(`, `)`).
                    if t.is_empty()
                        || t.contains('$')
                        || t.contains('`')
                        || t.contains('\\')
                        || t.contains('\'')
                        || t.contains('*')
                        || t.contains('?')
                        || t.contains('[')
                        || matches!(t.as_str(), "!" | "(" | ")")
                    {
                        return None;
                    }
                    if t.chars().any(|c| c.is_whitespace() || c == '"') {
                        words.push(quote_sh(&t));
                    } else {
                        words.push(t);
                    }
                }
            }
        }
        self.test_render(&words.join(" "))
    }

    /// Binary test operator → Go bool expr. `lraw`/`rraw` are the raw
    /// operand texts (for glob/quoting decisions).
    fn test_binop(
        &mut self,
        op: &str,
        _lraw: &str,
        l: String,
        lk: Opnd,
        rraw: &str,
        r: String,
        rk: Opnd,
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
    /// number → literal. Returns (expr, Go type) — `Opnd::Str` means the
    /// expression is already a Go string, so `strify` need not coerce.
    fn test_operand(&mut self, t: &str) -> Option<(String, Opnd)> {
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
        // `$(( … ))` is arithmetic, not command substitution: lower it
        // through the arith renderer instead of exec'ing `(…)` (which the
        // shell reads as a subshell and fails).
        if let Some(rest) = inner.strip_prefix("$((") {
            if let Some(close) = rest.rfind("))") {
                let arith_text = rest[..close].to_string();
                if let Some((sides, v)) = self.arith_str(&arith_text) {
                    for s in sides {
                        self.sides.push(s);
                    }
                    return Some((v, Opnd::Num));
                }
                return None;
            }
            return None;
        }
        if let Some(rest) = inner.strip_prefix("$(") {
            if let Some(close) = rest.rfind(')') {
                let cmd = rest[..close].to_string();
                let env = self.env_lit_for(Some(&cmd));
                return Some((format!("capRun({}, {env})", Self::go_str(&cmd)), Opnd::Str));
            }
            return None;
        }
        // `${name% *}` / `${x:-d}` — a param expansion as the operand
        if t.starts_with("${") && t.ends_with('}') {
            let inner2 = &t[2..t.len() - 1];
            self.need_vars = true;
            return Some((self.expand_braced(inner2), Opnd::Str));
        }
        // A leading `~` (bare or `~/…`) expands to $HOME — but only
        // unquoted; `"~"` is a literal tilde. `~user` (another user's
        // home) has no native form and refuses below.
        if !quoted {
            if t == "~" {
                return Some(("os.Getenv(\"HOME\")".to_string(), Opnd::Str));
            }
            if let Some(rest) = t.strip_prefix("~/") {
                return Some((
                    format!("(os.Getenv(\"HOME\") + {})", Self::go_str(&format!("/{rest}"))),
                    Opnd::Str,
                ));
            }
        }
        let inner = inner.strip_prefix('$').unwrap_or(inner);
        let inner = inner
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(inner);
        match inner {
            "#" => Some(("int64(len(argsList()))".to_string(), Opnd::Num)),
            "?" => {
                self.need_st = true;
                self.st_read = true;
                Some(("st".to_string(), Opnd::Num))
            }
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                // `$10` is the tenth positional parameter.
                if has_dollar {
                    let i: i64 = n.parse().unwrap_or(1);
                    return Some((format!("paramAt({i})"), Opnd::Str));
                }
                // A plain canonical decimal is a literal number. A QUOTED
                // operand is TEXT (`[ "10" = "010" ]` compares the strings;
                // re-parsing both sides as numbers made them agree), and a
                // LEADING ZERO is octal in bash's arithmetic (`[ 010 -eq 8 ]`
                // is true) while `parse` reads decimal — both stay strings so
                // `numify`'s `s2i` (ParseInt base 0, leading 0 = octal) is
                // what decides the value, not this parser.
                if !quoted && (!n.starts_with('0') || n == "0") {
                    if let Ok(v) = n.parse::<i64>() {
                        return Some((v.to_string(), Opnd::Num));
                    }
                }
                Some((Self::go_str(n), Opnd::Str))
            }
            _ => {
                if let Some(base) = inner.strip_suffix("[@]") {
                    if self.declared(base) {
                        self.need_vars = true;
                        return Some((
                            format!("strings.Join(pList(\"\", {}), \" \")", Self::go_str(inner)),
                            Opnd::Str,
                        ));
                    }
                }
                if self.declared(inner) && !(quoted && !has_dollar) {
                    // `"c"` is a literal, not the var c. The Go type follows
                    // the hoisted decl: int64 for an Int verdict, string for
                    // a Str one, `any` otherwise.
                    let k = if self.is_num(inner) {
                        Opnd::Num
                    } else if self.declared_go_string(inner) {
                        Opnd::Str
                    } else {
                        Opnd::Dyn
                    };
                    let m = self.ident_of(inner);
                    Some((m, k))
                } else if !quoted && inner.parse::<i64>().is_ok() {
                    // A QUOTED operand is a literal STRING, never a number:
                    // `[ "10" = "010" ]` compares the text, and re-parsing it
                    // as an integer turned both sides into `s2s(10)` — a
                    // wrong-but-compiling compare that agreed with itself.
                    Some((inner.parse::<i64>().unwrap().to_string(), Opnd::Num))
                } else if has_dollar {
                    // A word mixing literal text with expansions
                    // (`$HOME/Documents`, `pre$d`, `$a$b`): expand it
                    // through the general expander instead of a single
                    // getenv of the whole word.
                    if inner.chars().any(|c| c != '$' && !c.is_ascii_alphanumeric() && c != '_' && c != '{' && c != '}') {
                        Some((self.expand_str(t), Opnd::Str))
                    } else {
                        Some((format!("os.Getenv({})", Self::go_str(inner)), Opnd::Str))
                    }
                } else {
                    Some((Self::go_str(inner), Opnd::Str))
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
                    self.emit(&format!("for _sc{k}.Scan() {{"));
                    self.depth += 1;
                    self.emit(&format!("{} = _sc{k}.Text()", fl.var));
                    self.mark_written(&fl.var);
                    // Go errors on unused locals — silence when the body
                    // doesn't read the line (pure counters).
                    self.emit(&format!("_ = {}", fl.var));
                    for b in &fl.body {
                        self.stmt(b);
                    }
                    self.depth -= 1;
                    self.emit("}");
                    self.depth -= 1;
                    self.emit("}");
                    self.need_st = true;
                } else {
                    panic!("go backend: Ext node {} unsupported", node.tag());
                }
            }
            IrStmt::Assign { targets, expr, .. } => self.stmt_assign(targets, expr),
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    let m = self.go_ident(&d.name);
                    if init.is_none() {
                        // `local x` with no value leaves an EMPTY STRING in
                        // the shell (only `-i`/`-a`/`-A` make it something
                        // else, and those arrive as an int/array home, not a
                        // scalar decl), so it is a string write — not a
                        // non-string write that would veto the var's
                        // promotion to a Go `string`.
                        if matches!(d.sigil, Some(Sigil::Array) | Some(Sigil::Hash)) {
                            self.mark_written(&d.name);
                        } else {
                            self.mark_str_written(&d.name);
                        }
                        self.emit(&format!("// declare {m}"));
                        continue;
                    }
                    // Same verdict and the same STRING-write accounting
                    // as `stmt_assign` (see `scalar_assign`): an
                    // arith-written var hoists as int64 whatever A2 said
                    // the shell type is, so its init must coerce
                    // numerically too — else `local i=0` + `((i+=2))`
                    // emits `i = "0"` against an `int64` declaration
                    // (063_hard_to_parse). The write is counted ONCE, after
                    // scalar_assign, so a string init matches its string
                    // write (`mark_written` before it counted a second,
                    // non-string write and vetoed the promotion).
                    let rhs = self.scalar_assign(&d.name, init.as_ref().expect("checked above"));
                    self.mark_written(&d.name);
                    self.flush_sides();
                    let sync = self.sync_inline(&d.name);
                    self.emit(&format!("{m} = {rhs};{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
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
                } else {
                    // No else taken and no condition true: bash sets $?
                    // to 0 (a stale failure before the `if` must not leak
                    // through; the gate compares exit codes).
                    self.emit("} else {");
                    self.depth += 1;
                    self.emit("st = 0;");
                    self.need_st = true;
                    self.depth -= 1;
                }
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                // A loop that never runs leaves $? at 0 in bash — track
                // iterations so a stale failure before the loop cannot leak
                // through (the gate compares exit codes).
                let runflag = format!("__run{}", self.new_tmp());
                self.emit(&format!("{runflag} := false;"));
                // multi-command condition: `while cmd1; cmd2; do …` — run
                // the cond commands each iteration, loop on the LAST status.
                if let IrExpr::Call { func, args } = cond {
                    if func == "block" || func == "subshell" {
                        if let Some(IrExpr::Arrow(conds)) = args.first() {
                            self.emit("for {");
                            self.loop_depth += 1;
                            self.depth += 1;
                            for s in conds {
                                self.stmt(s);
                            }
                            self.emit("if st != 0 { break; }");
                            self.need_st = true;
                            self.emit(&format!("{runflag} = true;"));
                            for s in body {
                                self.stmt(s);
                            }
                            self.depth -= 1;
                            self.loop_depth -= 1;
                            self.emit("}");
                            self.emit(&format!("if !{runflag} {{ st = 0; }}"));
                            self.need_st = true;
                            return;
                        }
                    }
                }
                let c = self.expr_bool(cond);
                self.flush_sides();
                self.emit(&format!("for {c} {{"));
                self.loop_depth += 1;
                self.depth += 1;
                self.emit(&format!("{runflag} = true;"));
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("}");
                self.emit(&format!("if !{runflag} {{ st = 0; }}"));
                self.need_st = true;
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.emit("for {");
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                let c = self.expr_bool(cond);
                self.flush_sides();
                if *until {
                    self.emit(&format!("if {c} {{ break; }}"));
                } else {
                    self.emit(&format!("if !({c}) {{ break; }}"));
                }
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => self.stmt_for(var, iter, body),
            IrStmt::Exit(e) => {
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
                // A shell function yields its last command's output text and
                // the frontend models that as `Return(Interpolate(…))`, so
                // the Go closure is `func() string`; the caller's fArgs
                // holds the positional params.
                self.emit(&format!("{m} = func() string {{"));
                self.depth += 1;
                self.in_fn += 1;
                for s in body {
                    self.stmt(s);
                }
                self.in_fn -= 1;
                self.flush_sides();
                // a body whose LAST statement is a Return already reaches
                // the closure's exit on every path; anything else (an if,
                // a loop, an empty body) needs the fallthrough value so the
                // closure always type-checks.
                if !matches!(body.last(), Some(IrStmt::Return(_))) {
                    self.emit("return \"\";");
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                let d = self.expr_str(discriminant);
                self.flush_sides();
                self.emit("switch {");
                self.depth += 1;
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
                self.emit("}");
            }
            IrStmt::Redirect { inner, redirects } => {
                // heredoc into `cat` → print the content. Only when the
                // heredoc is the SOLE redirect: `cat > f <<EOF` must write
                // the file, not the terminal.
                if redirects.len() == 1 {
                    if let Some(content) = heredoc_content(redirects) {
                        if is_single_cat(inner) {
                            if let IrExpr::Str(text, _) = content {
                                let interp = redirects
                                    .iter()
                                    .any(|r| r.mode == "heredoc" && r.interpolate);
                                // An unquoted heredoc interpolates
                                // (`Hello $name`); expand it at runtime.
                                // A quoted one (`<<'EOF'`) is literal.
                                let body = if interp {
                                    format!("expand({})", Self::go_str(text))
                                } else {
                                    Self::go_str(text)
                                };
                                self.emit(&format!("fmt.Print({body});"));
                                if self.need_st {
                                    self.emit("st = 0;");
                                }
                                return;
                            }
                        }
                    }
                }
                // `cat > f <<EOF … EOF` (or `>> f`): write the (possibly
                // interpolated) content straight to the file — no child.
                // REFUSE > GUESS: exactly the heredoc plus one fd-1 file
                // write/append; anything else keeps the fallback.
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if let Some(line) = self.heredoc_to_file(inner, redirects) {
                        if self.todo == todo {
                            self.flush_sides();
                            self.emit(&line);
                            return;
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                // `while read -r VAR; do …; done < file` — the loop becomes
                // a native scanner iteration over the file, with the body
                // rendered as Go statements (see `while_read_redirect`).
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if self.while_read_redirect(inner, redirects) {
                        return;
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                // `mapfile [-t] [-d delim] [array] < file` — native line
                // read, no child.
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if let Some(lines) = self.mapfile_redirect_lines(inner, redirects) {
                        if self.todo == todo {
                            for l in lines {
                                self.emit(&l);
                            }
                            return;
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                // `x=… 2>/dev/null` — a pure assignment with only stderr
                // discards: the redirect is a no-op (a pure RHS writes no
                // stderr and every capture helper discards it anyway), so
                // the assignment lowers natively instead of forcing the
                // `bash -c` fallback that loses the assigned value.
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    let mut handled = false;
                    if redirects.iter().all(null_stderr_spec) {
                        if let [IrStmt::Assign { targets, expr, .. }] = &inner[..] {
                            if targets.len() == 1 && Self::is_pure_assign_rhs(expr) {
                                self.stmt_assign(targets, expr);
                                if self.todo == todo {
                                    self.emit("st = 0;");
                                    self.need_st = true;
                                    handled = true;
                                }
                            }
                        }
                        // The core also spells assignments as `assign`
                        // CALLS (`n=$(($1 + 0))`): same pure-RHS rule, via
                        // the chain operand's assign arm (status "0").
                        if !handled {
                            if let [IrStmt::Expr(e)] = &inner[..] {
                                if let IrExpr::Call { func, args } = e {
                                    if func == "assign" {
                                        let value = match args.len() {
                                            2 => &args[1],
                                            _ if args.len() > 2 => &args[2],
                                            _ => &args[0],
                                        };
                                        if Self::is_pure_assign_rhs(value) {
                                            if let Some((stmts, status)) =
                                                self.chain_operand(e)
                                            {
                                                if self.todo == todo && status == "0" {
                                                    for s in stmts {
                                                        self.emit(&s);
                                                    }
                                                    self.emit("st = 0;");
                                                    self.need_st = true;
                                                    handled = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if handled {
                        return;
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                if let Some(lines) = self.echo_redirect_write(inner, redirects) {
                    for l in lines {
                        self.emit(&l);
                    }
                    return;
                }
                if let Some(lines) = self.printf_redirect_write(inner, redirects) {
                    for l in lines {
                        self.emit(&l);
                    }
                    return;
                }
                // `builtin 2>/dev/null` / `fs-cmd 2>/dev/null`: the native
                // helpers never write stderr (it is discarded by default —
                // see `redirect_expr`'s null-stderr arm), so a lone
                // builtin/fs command with only stderr discards runs as its
                // native statements with the redirects dropped — no
                // `exec.Command` weld and no `bash -c` child. The dispatch
                // is the statement path's own (`exec_builtin_stmt`, then
                // `fs_argcmd_stmt`), so a refusal lands exactly where the
                // unredirected command would have gone (REFUSE > GUESS).
                if let Some(lines) = self.discard_redirect_native(inner, redirects) {
                    for l in lines {
                        self.emit(&l);
                    }
                    return;
                }
                // `cmd argv… > f` / `cmd … 2>/dev/null` in statement
                // position: the native counterpart of `redirRun` is
                // `exec.Command` with its stdio welded to the redirect
                // targets — no `bash -c` child and no env literal
                // (AGENTS.md §2). `native_redirect_spec` accepts only the
                // fd 0/1/2 file redirects and `2>&1` it can reproduce
                // exactly, and `body_single_exec_opt` only a lone external
                // command, so anything richer keeps the fallback below
                // (REFUSE > GUESS).
                // A user FUNCTION with welded redirects runs as the closure
                // with os.Stdin/Stdout/Stderr temporarily swapped — before
                // the external-command weld below, which would exec a binary
                // named after the function and fail.
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if let Some(lines) = self.function_redirect_lines(inner, redirects) {
                        if self.todo == todo {
                            for l in lines {
                                self.emit(&l);
                            }
                            return;
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if let Some((cmd, argv)) = self.body_single_exec_opt(inner, true) {
                        if let Some(lines) = self.native_redirect_spec_lines(&cmd, &argv, redirects)
                        {
                            if self.todo == todo {
                                for l in lines {
                                    self.emit(&l);
                                }
                                return;
                            }
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                }
                // `( cmd ) > f` — a GROUP around one external command. The
                // group adds nothing here: bash's subshell discards variable
                // writes, and a lone external command writes none, so the
                // redirect welds onto the same native exec instead of
                // reconstructing `( cmd ) > f` for a `bash -c` child.
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if let [IrStmt::Subshell(group)] | [IrStmt::Block(group)] = &inner[..] {
                        if let Some((cmd, argv)) = self.body_single_exec_opt(group, true) {
                            if let Some(lines) =
                                self.native_redirect_spec_lines(&cmd, &argv, redirects)
                            {
                                if self.todo == todo {
                                    for l in lines {
                                        self.emit(&l);
                                    }
                                    return;
                                }
                            }
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                // `a | b > f` / `a | b 2>e` / `a < f | b` in statement
                // position: the stages run as real processes (the same
                // admission as statement pipelines) with the edge stdio
                // welded to the redirect targets — no `bash -c` child and
                // no env literal. Only the shapes the wiring reproduces
                // exactly (stdin-file on the first stage, stdout/stderr
                // files or `2>&1` on the last); anything else keeps the
                // fallback below (REFUSE > GUESS).
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    if let Some(lines) = self.stmt_pipeline_redirect(inner, redirects) {
                        if self.todo == todo {
                            for l in lines {
                                self.emit(&l);
                            }
                            return;
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                }
                if let Some(text) = self.cmd_text_stmts(inner) {
                    let mut with_redirs = format!("( {text} )");
                    let mut bodies = String::new();
                    let mut ok = true;
                    for r in redirects {
                        if let Some(rt) = self.redirect_spec_text(r) {
                            with_redirs.push(' ');
                            with_redirs.push_str(&rt);
                            if let Some(b) = heredoc_body_text(r) {
                                bodies.push_str(&b);
                            }
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    with_redirs.push_str(&bodies);
                    if ok {
                        let env = self.env_lit_for(Some(&with_redirs));
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
                self.emit("os.Exit(1);");
            }
            IrStmt::Warn { .. } => {
                // bash writes warnings to stderr (discarded by the gate)
            }
            IrStmt::Return(e) => {
                if self.in_fn > 0 {
                    // inside a function closure: the value IS the function's
                    // result (bash prints it through the caller's echo).
                    // An int literal ALSO sets the status (`return 1` fails
                    // the caller's `if func` exactly like bash) — anything
                    // else leaves `$?` at the last command's status.
                    if let Some(IrExpr::Str(s, _)) = e.as_ref() {
                        if let Ok(n) = s.trim().parse::<i64>() {
                            self.need_st = true;
                            self.emit(&format!("st = {n};"));
                        }
                    }
                    let v = match e {
                        Some(x) => self.expr_str(x),
                        None => "\"\"".to_string(),
                    };
                    self.flush_sides();
                    self.emit(&format!("return {v};"));
                } else {
                    self.emit("return;");
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
                    self.mark_str_written(name);
                    let m = self.go_ident(name);
                    // Native first: every stage a real process (echo/printf
                    // admitted when coreutils agrees — the same policy the
                    // expression-level `pipeline_capture` uses), so `x=$(a |
                    // b)` reads stdout with no `bash -c` child and no env
                    // literal (AGENTS.md §2). Stage-level stdio keeps the
                    // reconstruction below: a capture must read the pipe.
                    {
                        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                        let bodies: Vec<&[IrStmt]> =
                            stages.iter().map(|st| &st[..]).collect();
                        if let Some((stagess, fi, lo, le, dup)) =
                            self.pipeline_chain(&bodies, Vec::new())
                        {
                            if fi.is_none()
                                && lo.is_none()
                                && le.is_none()
                                && !dup
                                && self.todo == todo
                            {
                                let parts: Vec<String> = stagess
                                    .iter()
                                    .map(|st| self.stage_lit(st))
                                    .collect();
                                let sync = self.sync_inline(name);
                                self.emit(&format!(
                                    "{m} = capPipe([][]string{{{}}});{sync}",
                                    parts.join(", ")
                                ));
                                return;
                            }
                        }
                        self.sides.truncate(sides);
                        self.tmp = tmp;
                    }
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
                            parts.join(" | ")
                        }
                    };
                    let env = self.env_lit_for(Some(&text));
                    let sync = self.sync_inline(name);
                    self.emit(&format!(
                        "{m} = capRun({}, {env});{sync}",
                        Self::go_str(&text)
                    ));
                    return;
                }
                // plain pipeline: native chain. Builtin stages are admitted
                // exactly like the expression-level `pipeline_stages` (echo /
                // printf exec as real processes when coreutils agrees with
                // the builtin — see `native_stage_builtin`), so `echo … |
                // grep …` is a runPipe chain instead of a `bash -c` child
                // with an env literal (AGENTS.md §2). Stage-level stdio
                // (`2>/dev/null`, `> f` on an edge) welds into the chain
                // instead of refusing it (see `pipeline_chain`).
                // `<loop printing lines> | sort` — the exec chain below can
                // only admit single-command stages, and the `bash -c`
                // reconstruction after it hands the child an env literal of
                // SCALARS only, so a loop over a collection expanded to
                // nothing there. Both stages are native Go
                // (see `stmt_print_loop_pipeline`), so this is tried first.
                {
                    let bodies: Vec<&[IrStmt]> = stages.iter().map(|st| &st[..]).collect();
                    let (todo, sides, tmp, out_len) =
                        (self.todo, self.sides.len(), self.tmp, self.out.len());
                    if self.stmt_print_loop_pipeline(&bodies) && self.todo == todo {
                        return;
                    }
                    self.out.truncate(out_len);
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                    self.todo = todo;
                }
                {
                    let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
                    let bodies: Vec<&[IrStmt]> = stages.iter().map(|st| &st[..]).collect();
                    if let Some((stagess, fi, lo, le, dup)) =
                        self.pipeline_chain(&bodies, Vec::new())
                    {
                        if self.todo == todo {
                            if fi.is_none() && lo.is_none() && le.is_none() && !dup {
                                let parts: Vec<String> = stagess
                                    .iter()
                                    .map(|st| self.stage_lit(st))
                                    .collect();
                                self.emit(&format!(
                                    "st = runPipe([][]string{{{}}});",
                                    parts.join(", ")
                                ));
                                self.need_st = true;
                                return;
                            }
                            let lines = self.emit_native_pipeline_lines(
                                &stagess, fi, lo, le, dup,
                            );
                            if self.todo == todo {
                                for l in lines {
                                    self.emit(&l);
                                }
                                return;
                            }
                        }
                    }
                    self.sides.truncate(sides);
                    self.tmp = tmp;
                }
                {
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
                        // `cmd_text_stmts`/`cmd_text_expr` keep heredoc
                        // bodies trailing (NUL-marked) — they already sit
                        // at the end of the LAST stage's text, which is the
                        // end of the pipeline.
                        let mut heads = Vec::new();
                        let mut tails = String::new();
                        for p in parts {
                            let (h, b) = split_heredoc_tail(&p);
                            heads.push(h);
                            tails.push_str(&b);
                        }
                        let joined = heads.join(" | ");
                        let text = format!("{joined}{tails}");
                        let env = self.env_lit_for(Some(&text));
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
                // `cmd &` runs as a goroutine; `wait` joins them. The
                // background statements render exactly like foreground
                // ones (same status/side-effect semantics); only the
                // execution is concurrent.
                // Fork isolation: a background job runs on a COPY of the
                // shell state, so vars it assigns must not leak into the
                // parent. Snapshot every assigned var before spawning; the
                // `wait` arm restores them after joining.
                let mut assigned = BTreeSet::new();
                let mut stack: Vec<&IrStmt> = b.iter().collect();
                while let Some(s) = stack.pop() {
                    match s {
                        IrStmt::Assign { targets, .. } => {
                            for t in targets {
                                let base = t.var.split('[').next().unwrap_or(&t.var);
                                assigned.insert(base.to_string());
                            }
                        }
                        IrStmt::Block(x) | IrStmt::Subshell(x) | IrStmt::Background(x) => {
                            stack.extend(x.iter());
                        }
                        IrStmt::If { then, elsifs, else_, .. } => {
                            stack.extend(then.iter());
                            for (_, bb) in elsifs {
                                stack.extend(bb.iter());
                            }
                            stack.extend(else_.iter());
                        }
                        IrStmt::While { body, .. }
                        | IrStmt::DoWhile { body, .. }
                        | IrStmt::For { body, .. } => {
                            stack.extend(body.iter());
                        }
                        IrStmt::Case { clauses, .. } => {
                            for c in clauses {
                                stack.extend(c.body.iter());
                            }
                        }
                        IrStmt::Pipeline { stages, .. } => {
                            for st in stages {
                                stack.extend(st.iter());
                            }
                        }
                        _ => {}
                    }
                }
                self.emit("if __wgd == nil { __wgd = &sync.WaitGroup{}; __wgds = append(__wgds, __wgd); }");
                for v in &assigned {
                    if self.written.contains(v) {
                        let m = self.go_ident(v);
                        let t = format!("__bgv{}", self.new_tmp());
                        self.emit(&format!("{t} := {m};"));
                        self.bg_saves.push((m, t));
                    }
                }
                self.emit("__wgd.Add(1);");
                self.emit("go func() {");
                self.depth += 1;
                self.emit("defer __wgd.Done();");
                for s in b {
                    self.stmt(s);
                }
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
            } => self.stmt_exec(cmd, args, capture, redirects, env),
            IrStmt::Require(_) | IrStmt::RawText(_) | IrStmt::Label(_) | IrStmt::Goto(_) => {
                self.mark_todo(&format!("stmt {:?}", s));
            }
            IrStmt::ForInit { .. } => self.mark_todo("ForInit (strip_cfor should have lowered it)"),
            IrStmt::Continue => {
                if self.loop_depth > 0 {
                    // `continue`/`break` SUCCEED — bash resets $? to 0
                    // (a stale failure before them must not leak past the
                    // loop; the gate compares exit codes).
                    self.emit("st = 0; continue;");
                    self.need_st = true;
                } else {
                    self.mark_todo("continue outside a loop");
                }
            }
            IrStmt::Break => {
                if self.loop_depth > 0 {
                    self.emit("st = 0; break;");
                    self.need_st = true;
                } else {
                    self.mark_todo("break outside a loop");
                }
            }
            IrStmt::Shaderize { .. } => {
                self.mark_todo("shaderize (autoshaderization) not supported by the Go backend");
            }
        }
    }

    fn stmt_assign(&mut self, targets: &[crate::ir::AssignTarget], expr: &IrExpr) {
        let Some(t) = targets.first() else {
            self.mark_todo("multi-target assign");
            return;
        };
        // `x=$(( … ))` / `a[i]=$(( … ))` whose expression bash itself
        // rejects: the arithmetic error means the assignment NEVER HAPPENS —
        // the target keeps its previous value (or stays unset) and `$?` is
        // 1. Reproduce that instead of assigning a guessed value. This runs
        // BEFORE the element-target arms so a subscripted target gets the
        // same verdict (063_hard_to_parse's `array[i][j]`: bash rejects the
        // nested subscript, so `result[i]` is never written).
        //
        // `arith_fail` is the side-effect-free question here: it answers
        // `Invalid` exactly when the tokenizer/parser reject the text, which
        // is also when `arith_str` below would return None. Probing with
        // `arith_str` instead would re-render the expression and shift every
        // later temporary's number.
        if let IrExpr::Call { func, args } = expr {
            if matches!(func.as_str(), "arith" | "arithEval") {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if self.arith_fail(s) == ArithFail::Invalid {
                        if self.need_st {
                            self.emit("st = 1;");
                        }
                        return;
                    }
                }
            }
        }
        if !t.indices.is_empty() {
            // array element assign: arr[key] = value
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
                // A BAKED assoc-element target (`config["user"]` — the
                // quotes are part of the core's spelling): strip one layer
                // of quotes and store into the base's map, which the
                // `pVal` map lookup reads back (REFUSE > GUESS: a dynamic
                // key keeps the scalarized spelling below).
                if !key_text.contains('$') && !key_text.contains('`') {
                    let key = key_text
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    self.mark_assoc(base);
                    self.mark_written(&t.var);
                    let rhs = self.expr_any(expr);
                    self.flush_sides();
                    let m = self.go_ident(base);
                    let sync = self.sync_inline(base);
                    self.emit(&format!(
                        "if {m} == nil {{ {m} = map[string]any{{}} }};{sync}"
                    ));
                    let gk = Self::go_str(&key);
                    self.emit(&format!("{m}[{gk}] = {rhs};{sync}"));
                    return;
                }
            }
        }
        // `x=$(( … ))` whose expression bash itself rejects was handled at
        // the top (the store never happens); from here on the assignment is
        // a real write.
        let m = self.go_ident(&t.var);
        self.mark_written(&t.var);
        // array assignment forms
        if let IrExpr::Call { func, args } = expr {
            if func == "setArray" || func == "setArrayAppend" {
                self.mark_arr(&t.var);
                let rhs = self.setarray_rhs(&m, func == "setArrayAppend", args);
                self.flush_sides();
                let sync = self.sync_inline(&t.var);
                self.emit(&format!("{m} = {rhs};{sync}"));
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
                let rhs = if op == "+=" && self.is_arr(&t.var) {
                    // array += element
                    self.expr_any(value)
                } else {
                    // the same write accounting `Assign` uses, so a var
                    // written only through the `assign` builtin is still
                    // typed (string vs int64) instead of collapsing to `any`
                    self.scalar_assign(&t.var, value)
                };
                self.flush_sides();
                let sync = self.sync_inline(&t.var);
                if op == "+=" && self.is_arr(&t.var) {
                    self.emit(&format!("{m} = append({m}, {rhs});{sync}"));
                } else if op == "+=" && self.is_str(&t.var) {
                    self.emit(&format!("{m} = {m} + {rhs};{sync}"));
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
                ArithAst::IncDec { var, delta, prefix } => {
                    let target = if var == &t.var {
                        m.clone()
                    } else {
                        self.go_ident(var)
                    };
                    self.mark_arith(var);
                    self.flush_sides();
                    let sync = self.sync_inline(var);
                    if *prefix {
                        // `((++i))` status: 0 iff the NEW value != 0.
                        let v = if *delta == 1 {
                            format!("{target} + 1")
                        } else {
                            format!("{target} - 1")
                        };
                        self.emit(&format!("{target} = {target} + {delta};{sync}"));
                        self.emit(&format!("st = b2i(({v}) == 0);"));
                    } else {
                        // `((i++))` status: 0 iff the OLD value != 0.
                        self.emit(&format!("st = b2i(({target}) == 0);"));
                        self.emit(&format!("{target} = {target} + {delta};{sync}"));
                    }
                    self.need_st = true;
                    return;
                }
                _ => {}
            }
        }
        // an arith-written var (the goto restructure's flags) hoists as
        // int64 — its assignments must coerce string values numerically
        let rhs = self.scalar_assign(&t.var, expr);
        self.flush_sides();
        let sync = self.sync_inline(&t.var);
        self.emit(&format!("{m} = {rhs};{sync}"));
        if self.need_st {
            self.emit("st = 0;");
        }
    }

    /// Render the right-hand side of a scalar `var = expr`, choosing the
    /// form from the var's verdict and recording a STRING write (see
    /// `mark_str_written`) when the value is a Go string. `Assign` and
    /// `Declare` share this so `local x=$(…)` and `x=$(…)` infer the same
    /// type.
    fn scalar_assign(&mut self, var: &str, expr: &IrExpr) -> String {
        let mut str_write = self.is_str(var);
        let rhs = if self.is_num(var) || self.arith_ints.contains(var) {
            self.expr_num(expr)
        } else if self.is_str(var) {
            self.expr_str(expr)
        } else {
            let todo = self.todo;
            let v = self.expr_any(expr);
            // An untyped scalar stored from a Go string is still a STRING
            // write: bash vars hold text. shir_to_go's second pass hoists
            // such a var as Go `string` instead of `any` + `s2s(v)` at
            // every read. A stubbed value renders as the int 0, so the
            // TODO delta disqualifies it.
            str_write = self.expr_yields_go_string(expr) && self.todo == todo;
            // bash stores an integer write as its decimal TEXT (`x=$?` is
            // "0", not the int 0), so an untyped target whose only writes
            // are integer-valued is still a string scalar. The second pass
            // renders it as `string` and the write through `expr_str`
            // (`strconv.FormatInt`), which drops the `s2s(v)` coercion from
            // every read.
            if !str_write && self.todo == todo && self.int_text_expr(expr) {
                *self.writes_inttext.entry(var.to_string()).or_insert(0) += 1;
            }
            v
        };
        if str_write {
            *self.writes_str.entry(var.to_string()).or_insert(0) += 1;
        }
        rhs
    }

    /// True when the expression renders as a Go int64 whose bash TEXT form
    /// is exactly its decimal (`strconv.FormatInt`): `x=$?`, `x=$#`, an
    /// `$(( … ))`, an int literal, an int-typed var, a numeric BinOp. Those
    /// writes do not disqualify an untyped target from being a string
    /// scalar — bash has no integer variables, it stores the digits.
    fn int_text_expr(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_) | IrExpr::Arith(_) => true,
            IrExpr::Var(n, _) | IrExpr::Ident(n) => self.is_num(n),
            IrExpr::BinOp { op, .. } => !matches!(op, crate::ir::BinOpKind::Concat),
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => matches!(n.as_str(), "?" | "#" | "$") || self.is_num(n),
                _ => false,
            },
            _ => false,
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
    ///
    /// `A && B` leaves `st` at A's status when A FAILS (and at B's when it
    /// runs), so the guarded form above is exactly bash — unlike a bool
    /// expression, which would set `st` to the chain's truthiness.
    #[allow(clippy::question_mark)]
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
                None => {
                    return false;
                }
            };
            ops_go.push((op, stmts, status));
        }
        if self.todo != before {
            if std::env::var_os("SH2_GO_DEBUG").is_some() {
            }
            return false;
        }
        let (_, first_stmts, first_status) = &ops_go[0];
        for s in first_stmts {
            self.emit(s);
        }
        // a native operand already wrote `st` itself
        if first_status != "st" {
            self.emit(&format!("st = {first_status};"));
        }
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
            if status != "st" {
                self.emit(&format!("st = {status};"));
            }
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
                // A bash BUILTIN operand — `cd "$d" || exit 1`,
                // `ls … || echo missing`, `x || true` — lowers to the same
                // native statements the plain-statement path emits. Running
                // it as a child is not merely a delegation: cd/exit/true/:
                // are builtins with no binary behind them, so exec.Command
                // fails with 127 and the chain takes the wrong arm.
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if let Some(r) = self.chain_builtin(cmd, args) {
                        return Some(r);
                    }
                    if let Some(r) = self.chain_fs_argcmd(cmd, args) {
                        return Some(r);
                    }
                    // A user FUNCTION in condition position (`if is_prime
                    // $n`, `x && func`) — call the closure with its fArgs
                    // and read the shared `st` it maintains, instead of
                    // exec'ing a binary named after the function (which
                    // fails and poisons loops into hanging forever).
                    if self.functions.contains(cmd) {
                        let m = self.go_ident(cmd);
                        self.mark_read(cmd);
                        self.need_fargs = true;
                        self.need_st = true;
                        let vals: Vec<String> = match args.get(1) {
                            Some(IrExpr::Array(items)) => {
                                items.iter().map(|i| self.expr_str(i)).collect()
                            }
                            _ => Vec::new(),
                        };
                        let lines = if vals.is_empty() {
                            vec![format!("{m}();")]
                        } else {
                            vec![
                                format!("fArgs = []string{{{}}};", vals.join(", ")),
                                format!("{m}();"),
                                "fArgs = nil;".to_string(),
                            ]
                        };
                        return Some((lines, "st".to_string()));
                    }
                    // `test` is not in the shared builtins namespace, so the
                    // arms above never see it — but it lowers through the
                    // same `test_render` as `[ ]` (see `exec_builtin_stmt`).
                    if cmd == "test" {
                        let argv: &[IrExpr] = match args.get(1) {
                            Some(IrExpr::Array(items)) => items.as_slice(),
                            _ => &[],
                        };
                        // `test` with no args is false (status 1).
                        if argv.is_empty() {
                            return Some((Vec::new(), "1".to_string()));
                        }
                        let todo = self.todo;
                        let sides = self.sides.len();
                        let tmp = self.tmp;
                        if let Some(c) = self.test_argv_bool(argv) {
                            if self.todo == todo {
                                return Some((Vec::new(), format!("b2i(!({c}))")));
                            }
                        }
                        self.sides.truncate(sides);
                        self.tmp = tmp;
                        self.todo = todo;
                    }
                }
                if let Some((cmd, argv)) = self.exec_cmd_argv(args) {
                    let call = Self::exec_cmd_call(&cmd, &argv);
                    return Some((Vec::new(), format!("runCmd({call})")));
                }
                None
            }
            // A plain `[ … ]` test is a CONDITION, not a command: as a
            // chain operand it contributes its status (`0` when true) with
            // no side effects, and the guard around the next operand then
            // reproduces bash's short-circuit. The core spells the test as
            // a bare `Ne`/`Eq` BinOp far more often than as a `test` Call.
            IrExpr::BinOp { op, .. }
                if !matches!(
                    op,
                    crate::ir::BinOpKind::And
                        | crate::ir::BinOpKind::Or
                        | crate::ir::BinOpKind::Concat
                ) =>
            {
                // A test operand is a `test`-like CONDITION, never an
                // ARITHMETIC comparison: its own failure is the arm's
                // status (`0` = true), and it has no side effects. Only
                // the comparison/negation ops qualify -- `+`/`*` operands
                // stay on the value path below.
                if !matches!(
                    op,
                    crate::ir::BinOpKind::Eq
                        | crate::ir::BinOpKind::Ne
                        | crate::ir::BinOpKind::Lt
                        | crate::ir::BinOpKind::Gt
                        | crate::ir::BinOpKind::Le
                        | crate::ir::BinOpKind::Ge
                        | crate::ir::BinOpKind::Not
                ) {
                    return None;
                }
                return Some((Vec::new(), format!("b2i(!({}))", self.expr_bool(e))));
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
                } else {
                    // the same write accounting `Assign` uses, so a var
                    // written only through the `assign` builtin is still
                    // typed (string vs int64) instead of collapsing to `any`
                    self.scalar_assign(&name, value)
                };
                self.flush_sides();
                let sync = self.sync_inline(&name);
                let stmt = if op == "+=" && self.is_arr(&name) {
                    format!("{m} = append({m}, {rhs});{sync}")
                } else if op == "+=" && self.is_str(&name) {
                    format!("{m} = {m} + {rhs};{sync}")
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
                let rhs = self.setarray_rhs(&m, false, args);
                self.flush_sides();
                let sync = self.sync_inline(&name);
                Some((
                    vec![format!("{m} = {rhs};{sync}")],
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
                None
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
                None
            }
            IrExpr::Call { func, args } if func == "pipeline" => {
                let v = self.pipeline_expr(args);
                Some((Vec::new(), v))
            }
            IrExpr::Call { func, args } if func == "redirect" => {
                // A builtin / natively-lowered fs command with only stderr
                // discards needs no exec at all: its native statements run
                // inline with the status in `st` (see
                // `builtin_fs_speculative`) — before the weld below, which
                // would fork an `exec.Command` for the same shape.
                if let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
                    (args.first(), args.get(1))
                {
                    if !redirs.is_empty() && redirs.iter().all(null_stderr_obj) {
                        if let Some(call_args) = lone_exec_args(body) {
                            if let Some(lines) = self.builtin_fs_speculative(&call_args) {
                                return Some((lines, "st".to_string()));
                            }
                        }
                    }
                }
                // `x=… 2>/dev/null` — a pure assignment with only stderr
                // discards: the redirect is a no-op, so the assignment
                // lowers natively instead of forcing the `bash -c` weld
                // that loses the assigned value (see the statement-level
                // twin in the Redirect arm).
                if let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
                    (args.first(), args.get(1))
                {
                    if !redirs.is_empty() && redirs.iter().all(null_stderr_obj) {
                        if let [IrStmt::Expr(IrExpr::Call { func, args })] = &body[..] {
                            if func == "assign" {
                                let value = match args.len() {
                                    2 => &args[1],
                                    _ if args.len() > 2 => &args[2],
                                    _ => &args[0],
                                };
                                if Self::is_pure_assign_rhs(value) {
                                    let (todo, sides, tmp) =
                                        (self.todo, self.sides.len(), self.tmp);
                                    if let Some((mut stmts, status)) =
                                        self.chain_operand(&IrExpr::Call {
                                            func: func.clone(),
                                            args: args.clone(),
                                        })
                                    {
                                        if self.todo == todo && status == "0" {
                                            stmts.push("st = 0;".to_string());
                                            self.need_st = true;
                                            return Some((stmts, "st".to_string()));
                                        }
                                    }
                                    self.sides.truncate(sides);
                                    self.tmp = tmp;
                                    self.todo = todo;
                                }
                            }
                        }
                    }
                }
                // native first: a single external command with stdio-welded
                // redirects runs under exec.Command, not a reconstructed
                // `bash -c` string. The lines leave the status in `st`.
                if let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
                    (args.first(), args.get(1))
                {
                    if !redirs.is_empty() {
                        let todo = self.todo;
                        let sides = self.sides.len();
                        let tmp = self.tmp;
                        if let Some((cmd, argv)) = self.body_single_exec(body) {
                            if let Some(lines) = self.native_redirect_lines(&cmd, &argv, redirs) {
                                if self.todo == todo {
                                    return Some((lines, "st".to_string()));
                                }
                            }
                        }
                        self.sides.truncate(sides);
                        self.tmp = tmp;
                    }
                }
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
                    Some((vec!["st = 0; break;".to_string()], "st".to_string()))
                } else {
                    None
                }
            }
            IrExpr::Call { func, args } if func == "continue" => {
                let _ = args;
                if self.loop_depth > 0 {
                    Some((vec!["st = 0; continue;".to_string()], "st".to_string()))
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

    /// A bash builtin as an `&&`/`||` chain operand. The native lowering
    /// already exists in `exec_builtin_stmt` (echo/cd/exit/true/:/…) — this
    /// captures its statements and reports the status through `st`, the
    /// operand contract `stmt_chain` expects. Refuses (None) for anything
    /// that arm does not lower, so the caller keeps the exec path.
    fn chain_builtin(&mut self, cmd: &str, args: &[IrExpr]) -> Option<(Vec<String>, String)> {
        if !crate::transforms::builtin::is_builtin(cmd) {
            return None;
        }
        let todo_before = self.todo;
        let need_st_before = self.need_st;
        let sides_before = self.sides.len();
        let tmp_before = self.tmp;
        // the status arm writes `st` unconditionally when it is needed
        self.need_st = true;
        // capture UNINDENTED, exactly like the subshell arm: the caller
        // re-emits every line through `emit`, which applies the indentation
        // of the guard it lands in.
        let saved_depth = self.depth;
        self.depth = 0;
        let saved = std::mem::take(&mut self.out);
        let handled = self.exec_builtin_stmt(cmd, args);
        let stmts = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        if !handled || self.todo != todo_before {
            self.todo = todo_before;
            self.need_st = need_st_before;
            self.sides.truncate(sides_before);
            self.tmp = tmp_before;
            return None;
        }
        self.need_st = true;
        let mut stmts = stmts;
        // `echo`/`printf`/`export`/… succeed without touching `st`; bash's
        // status for them is 0. `exit` never returns, so it needs none.
        if !stmts.iter().any(|s| s.contains("os.Exit("))
            && !stmts.iter().any(|s| s.contains("st ="))
        {
            stmts.push("st = 0;".to_string());
        }
        Some((stmts, "st".to_string()))
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
            IrExpr::Call { func, args: _ } if func == "fnCall" || func == "fnValue" => {
                // user-function call in statement position
                let v = self.expr_any(e);
                self.flush_sides();
                self.emit(&format!("_ = {v};"));
            }
            IrExpr::Call { func, args } if func == "exec" => {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if self.exec_builtin_stmt(cmd, args) {
                        return;
                    }
                    if self.fs_argcmd_stmt(cmd, args) {
                        return;
                    }
                    if self.functions.contains(cmd) {
                        self.call_function(cmd, args);
                        return;
                    }
                }
                if let Some((cmd, argv)) = self.exec_cmd_argv(args) {
                    let call = Self::exec_cmd_call(&cmd, &argv);
                    self.emit(&format!("st = runCmd({call});"));
                    self.need_st = true;
                    return;
                }
                self.mark_todo("exec");
            }
            IrExpr::Call { func, args } if func == "pipeline" => {
                // `<loop printing lines> | sort` in STATEMENT position: the
                // lowering is a statement block, not a value, so it has to
                // happen here rather than inside `pipeline_expr` (see
                // `stmt_print_loop_pipeline`).
                if let Some(IrExpr::Array(items)) = args.first() {
                    let mut bodies: Vec<&[IrStmt]> = Vec::with_capacity(items.len());
                    let mut ok = true;
                    for it in items {
                        match it {
                            IrExpr::Arrow(b) => bodies.push(&b[..]),
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        let (todo, sides, tmp, out_len) =
                            (self.todo, self.sides.len(), self.tmp, self.out.len());
                        if self.stmt_print_loop_pipeline(&bodies) && self.todo == todo {
                            return;
                        }
                        self.out.truncate(out_len);
                        self.sides.truncate(sides);
                        self.tmp = tmp;
                        self.todo = todo;
                    }
                }
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
                // `grep -o PAT` in statement position WRITES its matches,
                // one per line — that is grep's stdout contract, so the
                // native regexp path must print them, not discard them.
                // `$?` is 0 iff something matched.
                if args.is_empty() {
                    self.mark_todo("grepMatches");
                    return;
                }
                if let Some(re) = self.grep_matches_re(args) {
                    let text = self.expr_str(&args[0]);
                    self.flush_sides();
                    self.emit("{");
                    self.depth += 1;
                    self.emit(&format!(
                        "__gm := regexp.MustCompile({re}).FindAllString({text}, -1)"
                    ));
                    self.emit("for _, __m := range __gm { if __m != \"\" { fmt.Println(__m) } }");
                    self.emit("st = b2i(len(__gm) == 0);");
                    self.depth -= 1;
                    self.emit("}");
                    self.need_st = true;
                    return;
                }
                let x = self.expr_any(e);
                self.flush_sides();
                self.emit(&format!("_ = {x};"));
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
                    // bash rejects the expression: `(( ))` reports the error
                    // and exits 1.
                    if self.arith_fail(s) == ArithFail::Invalid {
                        self.emit("st = 1;");
                        self.need_st = true;
                        return;
                    }
                }
                self.mark_todo("arith stmt");
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
                            self.mark_todo("let");
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
                        self.emit(&format!("for {c} {{"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        for s in rest {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit("}");
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
            IrExpr::BinOp { lhs, op, .. }
                if matches!(op, crate::ir::BinOpKind::And | crate::ir::BinOpKind::Or) =>
            {
                // The chain's guarded form (`stmt_chain`: `st = A; if st
                // == 0 { B }`) is what makes the RHS CONDITIONAL and keeps
                // `$?` at the status of the last command run. Take it for
                // every shape it can lower — the bool expression below is
                // the fallback, which runs both sides unconditionally.
                if self.stmt_chain(e) {
                    return;
                }
                // A `&&`/`||` CHAIN in statement position SHORT-CIRCUITS:
                // `cmd1 && cmd2` runs cmd2 only on success, and bash's
                // status is that of the last command run. Rendering it as
                // one bool expression sets `st` to the CHAIN's truthiness
                // and runs both sides, so `[ 1 = 2 ] && echo hit; echo $?`
                // prints 0 instead of 1 and prints `hit` unconditionally.
                //
                // `stmt_chain` refuses a chain whose FIRST operand is a
                // `test` (`[ "$?" = 0 ] && …`): `$?` must be read BEFORE
                // `st` is overwritten with the test's own status, so it
                // takes the other stmt_chain call site (the `IrStmt::Expr`
                // head arm) instead of this one.
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

    /// `rm` rendered with Go's own filesystem calls — no child process.
    /// Returns false, having emitted nothing, when the argv is outside
    /// `rm_native_shape`, so the caller keeps the fork/exec.
    fn rm_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(expanded) = self.expand_brace_argv(argv) else {
            return false;
        };
        let Some((recursive, force, list)) = rm_native_shape(&expanded) else {
            return false;
        };
        // Process-substitution desugar cleanup (`rm -f __ps_tmp*`): bash
        // runs `<(...)` via /dev/fd with NO cleanup command, so this rm
        // must not disturb `$?` — save and restore the status around it.
        // (A user `rm -f` of their own temp still reports its own status;
        // only the core's `__ps_tmp` names take this path.)
        let desugar_cleanup = force
            && !recursive
            && !list.is_empty()
            && list.iter().all(|e| match e {
                IrExpr::Str(s, _) => s == "-f" || s == "-rf" || s == "-fr",
                IrExpr::Var(name, _) | IrExpr::Ident(name) => name.starts_with("__ps_tmp"),
                _ => false,
            });
        let ops: Vec<(String, bool)> = list
            .iter()
            .map(|e| {
                let glob = expr_glob(e);
                (self.expr_str(e), glob)
            })
            .collect();
        self.flush_sides();
        // rm's status is the last failure, so it starts clean and only ever
        // rises to 1 — one failing operand fails the whole command. A glob
        // operand expands at run time (`globArgs`, with the same
        // no-match-passes-literally rule as the shell) and removes each
        // match under the same per-file semantics. Desugar cleanup (above)
        // instead preserves the incoming status: plain removes, no status
        // lines at all.
        if desugar_cleanup {
            for (_, (op, _)) in list.iter().zip(ops.iter()) {
                self.emit(&format!("os.Remove({op});"));
            }
            return true;
        }
        self.emit("st = 0;");
        self.need_st = true;
        let id = self.new_tmp();
        for (item, (op, glob)) in list.iter().zip(ops.iter()) {
            let one = |target: &str| match (recursive, force) {
                // -f is "say nothing about a path that is not there", the one
                // error rm swallows; every other failure keeps rc 1.
                (false, true) => format!(
                    "if err := os.Remove({target}); err != nil && !os.IsNotExist(err) {{ st = 1 }}"
                ),
                (false, false) => {
                    format!("if err := os.Remove({target}); err != nil {{ st = 1 }}")
                }
                // -rf succeeds on a missing path, but -r alone does not, and
                // os.RemoveAll cannot tell those two apart.
                (true, true) => {
                    format!("if err := os.RemoveAll({target}); err != nil {{ st = 1 }}")
                }
                (true, false) => format!(
                    "if _, err := os.Lstat({target}); err != nil {{ st = 1 }} else if err := os.RemoveAll({target}); err != nil {{ st = 1 }}"
                ),
            };
            if *glob {
                // The glob pattern is a static literal (only `Str`
                // operands glob) — render it raw, not through the
                // `any`-boxing coercion of the value above.
                let IrExpr::Str(s, _) = item else {
                    unreachable!()
                };
                let pat = Self::go_str(&strip_sh2glob(s));
                self.emit(&format!("for _, __rp{id} := range globArgs({pat}) {{"));
                self.depth += 1;
                self.emit(&one(&format!("__rp{id}")));
                self.depth -= 1;
                self.emit("}");
            } else {
                self.emit(&one(op));
            }
        }
        self.need_st = true;
        true
    }

    /// `mkdir` rendered with Go's own filesystem calls.
    fn mkdir_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(expanded) = self.expand_brace_argv(argv) else {
            return false;
        };
        let Some((parents, list)) = mkdir_native_shape(&expanded) else {
            return false;
        };
        let ops: Vec<String> = list.iter().map(|e| self.expr_str(e)).collect();
        self.flush_sides();
        self.emit("st = 0;");
        for op in &ops {
            // bash's mkdir is mode 0777 & ~umask, and os.Mkdir applies the
            // umask to the mode it is given — so 0o777 keeps the umask in
            // charge of the result, exactly as the child would have been.
            self.emit(&if parents {
                format!("if err := os.MkdirAll({op}, 0o777); err != nil {{ st = 1 }}")
            } else {
                format!("if err := os.Mkdir({op}, 0o777); err != nil {{ st = 1 }}")
            });
        }
        self.need_st = true;
        true
    }

    /// Brace-expand argv items into plain words — the same expansion
    /// `argv_strings` applies when building the exec argv, so `touch
    /// file_{001..005}.txt` validates as its five expansions instead of
    /// refusing on the splice. `None` when an item is not expandable —
    /// the caller keeps the exec. (Globs still refuse downstream: an
    /// expansion is only literal text, never a runtime splice.)
    fn expand_brace_argv(&mut self, argv: &[IrExpr]) -> Option<Vec<IrExpr>> {
        let mut out = Vec::with_capacity(argv.len());
        for it in argv {
            if let IrExpr::Call { func, args } = it {
                if func == "brace" {
                    for s in self.brace_expand(args)? {
                        out.push(IrExpr::Str(s, crate::ir::StrStyle::DoubleQuoted));
                    }
                    continue;
                }
            }
            out.push(it.clone());
        }
        Some(out)
    }

    /// `touch` rendered with Go's own filesystem calls: each operand is
    /// created empty when missing and stamped with the current time
    /// otherwise — `os.Chtimes`, falling back to create-then-stamp. Any
    /// dash operand (`-c` skips creation, `-d`/`-t`/`-r` set explicit
    /// times, `-a`/`-m` stamp one clock) has no counterpart here and keeps
    /// the exec, as does `-` (stdin, not a path) or a glob splice.
    fn touch_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(expanded) = self.expand_brace_argv(argv) else {
            return false;
        };
        let argv: &[IrExpr] = &expanded;
        if argv.is_empty() {
            return false;
        }
        for a in argv {
            match a {
                IrExpr::Str(s, style) => {
                    if s == "-" || s.starts_with('-') || str_expands_glob(s, style) {
                        return false;
                    }
                }
                IrExpr::Call { func, .. } => {
                    if func == "brace" {
                        return false;
                    }
                }
                _ => {}
            }
        }
        let ops: Vec<String> = argv.iter().map(|e| self.expr_str(e)).collect();
        self.flush_sides();
        let id = self.new_tmp();
        self.emit(&format!("__tt{id} := time.Now();"));
        self.emit("st = 0;");
        for op in &ops {
            // Missing → create empty (0644, umask-applied like the child),
            // then stamp; present → stamp. A failed create or stamp is
            // status 1, and one failing operand fails the command.
            self.emit(&format!(
                "if __e := os.Chtimes({op}, __tt{id}, __tt{id}); __e != nil {{ \
                 if __f, __fe := os.OpenFile({op}, os.O_WRONLY|os.O_CREATE, 0644); __fe != nil {{ st = 1 }} \
                 else {{ __f.Close(); if __e := os.Chtimes({op}, __tt{id}, __tt{id}); __e != nil {{ st = 1 }} }} }}"
            ));
        }
        self.need_st = true;
        true
    }

    /// `rmdir` rendered with Go's own filesystem calls: each operand must
    /// be an empty directory. `os.Remove` alone is not enough (it also
    /// removes files, which `rmdir` refuses), so an `Lstat`+`IsDir` guard
    /// precedes it. Any dash operand, `-`, a glob splice or an empty list
    /// keeps the exec.
    fn rmdir_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(expanded) = self.expand_brace_argv(argv) else {
            return false;
        };
        let argv: &[IrExpr] = &expanded;
        if argv.is_empty() {
            return false;
        }
        for a in argv {
            match a {
                IrExpr::Str(s, style) => {
                    if s == "-" || s.starts_with('-') || str_expands_glob(s, style) {
                        return false;
                    }
                }
                IrExpr::Call { func, .. } => {
                    if func == "brace" {
                        return false;
                    }
                }
                _ => {}
            }
        }
        let ops: Vec<String> = argv.iter().map(|e| self.expr_str(e)).collect();
        self.flush_sides();
        self.emit("st = 0;");
        for op in &ops {
            self.emit(&format!(
                "if __fi, __e := os.Lstat({op}); __e != nil || !__fi.IsDir() {{ st = 1 }} \
                 else if __e := os.Remove({op}); __e != nil {{ st = 1 }}"
            ));
        }
        self.need_st = true;
        true
    }

    /// `ln` rendered with Go's own filesystem calls: `ln a b` is
    /// `os.Link`, `ln -s a b` is `os.Symlink`, and `-f` removes the
    /// destination first (errors ignored, exactly like `rm -f` there).
    /// Only the `s`/`f` short flags combine; anything else (`-n`, `-b`,
    /// `-v`, long flags), a non-2 operand count, `-`, or a glob splice
    /// keeps the exec.
    fn ln_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(expanded) = self.expand_brace_argv(argv) else {
            return false;
        };
        let argv: &[IrExpr] = &expanded;
        let (mut symbolic, mut force) = (false, false);
        let mut ops: Vec<&IrExpr> = Vec::new();
        let mut operands = false;
        for a in argv {
            match a {
                IrExpr::Str(s, _) if !operands && s.len() > 1 && s.starts_with('-') => {
                    if s.starts_with("--") {
                        return false;
                    }
                    for c in s[1..].chars() {
                        match c {
                            's' => symbolic = true,
                            'f' => force = true,
                            _ => return false,
                        }
                    }
                    continue;
                }
                _ if !fs_literal_operand(a) => return false,
                _ => {
                    operands = true;
                    ops.push(a);
                }
            }
        }
        if ops.len() != 2 {
            return false;
        }
        let src = self.expr_str(ops[0]);
        let dst = self.expr_str(ops[1]);
        self.flush_sides();
        self.emit("st = 0;");
        if force {
            self.emit(&format!("os.Remove({dst});"));
        }
        if symbolic {
            self.emit(&format!(
                "if err := os.Symlink({src}, {dst}); err != nil {{ st = 1 }}"
            ));
        } else {
            self.emit(&format!(
                "if err := os.Link({src}, {dst}); err != nil {{ st = 1 }}"
            ));
        }
        self.need_st = true;
        true
    }

    /// `cmp` rendered with `cmpFiles` (see RUNTIME_HELPERS): the byte-exact
    /// GNU cmp the corpus compares against, without a child. Accepts the
    /// option set the helper reproduces — `-s`, `-l`, `-b`, `-i SKIP[:SKIP2]`
    /// and `-n LIMIT` in any order, `--` ending the flags — and exactly two
    /// operands. REFUSE > GUESS: a long option, an unknown short flag, a
    /// non-decimal limit/skip, stdin (`-`), a glob splice or a brace keeps
    /// the fork (GNU's suffix forms `-n 1K`, `-i 2k` included).
    fn cmp_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let (mut silent, mut list, mut bytes_mode) = (false, false, false);
        let (mut limit, mut skip1, mut skip2) = (-1i64, 0i64, 0i64);
        let mut ops: Vec<&IrExpr> = Vec::new();
        let mut flags_done = false;
        let mut i = 0;
        while i < argv.len() {
            let word = match &argv[i] {
                IrExpr::Str(s, _) if !flags_done && ops.is_empty() => Some(s.clone()),
                _ => None,
            };
            let Some(w) = word else {
                flags_done = true;
                ops.push(&argv[i]);
                i += 1;
                continue;
            };
            if !w.starts_with('-') || w == "-" {
                flags_done = true;
                ops.push(&argv[i]);
                i += 1;
                continue;
            }
            if w == "--" {
                flags_done = true;
                i += 1;
                continue;
            }
            if w.starts_with("--") {
                return false;
            }
            if w == "-n" || w == "-i" {
                let Some(IrExpr::Str(v, _)) = argv.get(i + 1) else {
                    return false;
                };
                if w == "-n" {
                    let Some(n) = cmp_dec(v) else { return false };
                    limit = n;
                } else {
                    let (a, b) = match v.split_once(':') {
                        Some((a, b)) => (a, b),
                        None => (v.as_str(), v.as_str()),
                    };
                    let (Some(x), Some(y)) = (cmp_dec(a), cmp_dec(b)) else {
                        return false;
                    };
                    skip1 = x;
                    skip2 = y;
                }
                i += 2;
                continue;
            }
            for c in w[1..].chars() {
                match c {
                    's' => silent = true,
                    'l' => list = true,
                    'b' => bytes_mode = true,
                    _ => return false,
                }
            }
            i += 1;
        }
        if ops.len() != 2 {
            return false;
        }
        for op in &ops {
            if !fs_literal_operand(op) {
                return false;
            }
            if let IrExpr::Str(s, _) = op {
                if s == "-" || s.starts_with('-') {
                    return false;
                }
            }
        }
        let a = self.expr_str(ops[0]);
        let b = self.expr_str(ops[1]);
        self.flush_sides();
        self.emit(&format!(
            "st = cmpFiles({a}, {b}, {silent}, {list}, {bytes_mode}, {limit}, {skip1}, {skip2});"
        ));
        self.need_st = true;
        true
    }

    /// `sleep` rendered with `time.Sleep`: no child process for a call that
    /// is pure waiting. GNU's operand grammar is what the corpus writes —
    /// plain decimal seconds, an optional `s`/`m`/`h`/`d` suffix, a
    /// fractional part, and several operands summed. Anything else (`inf`,
    /// `nan`, an option, a dynamic operand) keeps the fork. REFUSE > GUESS:
    /// a mis-parsed interval would sleep for the wrong time and silently
    /// change the program's behaviour.
    fn sleep_stmt(&mut self, argv: &[IrExpr]) -> bool {
        if argv.is_empty() {
            return false;
        }
        let mut total = 0i64;
        for a in argv {
            let IrExpr::Str(s, _) = a else {
                return false;
            };
            let Some(ns) = sleep_ns(s) else {
                return false;
            };
            let Some(sum) = total.checked_add(ns) else {
                return false;
            };
            total = sum;
        }
        self.flush_sides();
        self.emit(&format!("{};", sleep_call(total)));
        if self.need_st {
            self.emit("st = 0;");
        }
        true
    }

    /// `ls [-1aAd] [OPERAND…]` — the names, printed from `os.Stat`/`os.ReadDir`.
    ///
    /// What this reproduces exactly is the shape a script SEES: `ls` only
    /// lays out columns on a tty, and the render's stdout is never one, so
    /// the output is one name per line. GNU's remaining rules are kept:
    /// operands sorted, non-directories listed first, directories expanded
    /// under a `name:` header (with a blank line between blocks) once more
    /// than one operand was given, dotfiles hidden unless `-a`/`-A`, and
    /// status 2 for an operand that cannot be stat'ed.
    ///
    /// REFUSE > GUESS: `-l` (long format), `-F` (classify), `-R`, and the
    /// sort/selection flags have no counterpart in this template, and a glob
    /// operand splices at run time — all keep the fork.
    fn ls_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let id = self.new_tmp();
        self.ls_emit(argv, None, &id)
    }

    /// `$(ls …)`: the SAME emitter writing into a `strings.Builder`, trimmed
    /// the way the substitution trims. A statement and a capture that
    /// disagree about ordering or headers would be a silent wrong-output
    /// bug, so there is one template, two sinks.
    fn ls_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        let id = self.new_tmp();
        let buf = format!("__lsb{id}");
        // strings.Builder's Write has a pointer receiver, so the sink is the
        // builder's ADDRESS.
        let sink = format!("&{buf}");
        let saved_depth = self.depth;
        self.depth = 0;
        let saved = std::mem::take(&mut self.out);
        let ok = self.ls_emit(argv, Some(&sink), &id);
        let lines = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        if !ok {
            return None;
        }
        Some(format!(
            "func() string {{ var {buf} strings.Builder; {} ; return strings.TrimRight({buf}.String(), \"\\n\") }}()",
            lines.join("; ")
        ))
    }

    /// The single `ls` template. `sink` is `None` for stdout or the name of a
    /// `strings.Builder` to append to.
    fn ls_emit(&mut self, argv: &[IrExpr], sink: Option<&str>, id: &str) -> bool {
        let Some((all, almost, dir_itself, ops)) = ls_native_shape(argv) else {
            return false;
        };
        let mut items = Vec::with_capacity(ops.len());
        for op in &ops {
            items.push(self.expr_str(op));
        }
        self.flush_sides();
        // No operand is `.` — the same directory `ls` defaults to, and a
        // single directory operand never gets a header.
        let list = if items.is_empty() {
            "[]string{\".\"}".to_string()
        } else {
            format!("[]string{{{}}}", items.join(", "))
        };
        let (f, d) = (format!("__lsf{id}"), format!("__lsd{id}"));
        let (x, n) = (format!("__lsx{id}"), format!("__lsn{id}"));
        let nm = format!("__lsnm{id}");
        let pr = |a: &str| match sink {
            Some(w) => format!("fmt.Fprintln({w}, {a})"),
            None => format!("fmt.Println({a})"),
        };
        self.emit("st = 0;");
        self.need_st = true;
        self.emit(&format!("{f} := []string{{}}; {d} := []string{{}}"));
        self.emit(&format!("for _, {x} := range {list} {{"));
        self.depth += 1;
        self.emit(&format!("__lsi{id}, __lse{id} := os.Stat({x})"));
        // A directory operand is expanded unless -d asks for the entry
        // itself; anything else is printed as the operand was written.
        self.emit(&format!(
            "if __lse{id} != nil {{ st = 2 }} else if __lsi{id}.IsDir() && {} {{ {d} = append({d}, {x}) }} else {{ {f} = append({f}, {x}) }}",
            if dir_itself { "false" } else { "true" }
        ));
        self.depth -= 1;
        self.emit("}");
        self.emit(&format!("sort.Strings({f}); sort.Strings({d})"));
        self.emit(&format!("for _, {x} := range {f} {{ {} }}", pr(&x)));
        self.emit(&format!("for __lsk{id}, {x} := range {d} {{"));
        self.depth += 1;
        self.emit(&format!(
            "if len({f})+len({d}) > 1 {{ if __lsk{id} > 0 || len({f}) > 0 {{ {} }}; {} }}",
            pr("\"\""),
            pr(&format!("{x} + \":\""))
        ));
        self.emit(&format!("__lsr{id}, __lsre{id} := os.ReadDir({x})"));
        self.emit(&format!("if __lsre{id} != nil {{ st = 2; continue }}"));
        self.emit(&format!("{n} := []string{{}}"));
        self.emit(&format!("for _, __lse{id} := range __lsr{id} {{"));
        self.depth += 1;
        self.emit(&format!("{nm} := __lse{id}.Name()"));
        // os.ReadDir never yields `.` or `..`, so -A needs no filter and
        // -a only has to prepend them below.
        if !all && !almost {
            self.emit(&format!("if strings.HasPrefix({nm}, \".\") {{ continue }}"));
        }
        self.emit(&format!("{n} = append({n}, {nm})"));
        self.depth -= 1;
        self.emit("}");
        if all {
            self.emit(&format!("{n} = append([]string{{\".\", \"..\"}}, {n}...)"));
        }
        self.emit(&format!("sort.Strings({n})"));
        self.emit(&format!("for _, {nm} := range {n} {{ {} }}", pr(&nm)));
        self.depth -= 1;
        self.emit("}");
        true
    }

    /// `cat FILE…` — `os.ReadFile` and a write, with cat's own status rule:
    /// an unreadable operand reports 1 and the remaining files are still
    /// copied.
    fn cat_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let id = self.new_tmp();
        self.cat_emit(argv, None, &id)
    }

    /// `$(cat FILE…)`: the same reads accumulating into a builder.
    fn cat_value(&mut self, argv: &[IrExpr]) -> Option<String> {
        let id = self.new_tmp();
        let buf = format!("__catb{id}");
        let saved_depth = self.depth;
        self.depth = 0;
        let saved = std::mem::take(&mut self.out);
        let sink = format!("&{buf}");
        let ok = self.cat_emit(argv, Some(&sink), &id);
        let lines = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        if !ok {
            return None;
        }
        Some(format!(
            "func() string {{ var {buf} strings.Builder; {} ; return strings.TrimRight({buf}.String(), \"\\n\") }}()",
            lines.join("; ")
        ))
    }

    fn cat_emit(&mut self, argv: &[IrExpr], sink: Option<&str>, id: &str) -> bool {
        // No operand and a lone `-` are stdin, and an option (`-A`/`-n`/`-v`)
        // transforms the bytes: both keep whatever path reached them.
        if argv.is_empty() {
            return false;
        }
        for a in argv {
            match a {
                IrExpr::Str(s, style) => {
                    if s == "-" || (s.len() > 1 && s.starts_with('-')) || str_expands_glob(s, style)
                    {
                        return false;
                    }
                }
                IrExpr::Call { func, .. } if func == "brace" => return false,
                _ => {}
            }
        }
        let ops: Vec<String> = argv.iter().map(|e| self.expr_str(e)).collect();
        self.flush_sides();
        self.emit("st = 0;");
        self.need_st = true;
        let w = |b: &str| match sink {
            Some(s) => format!("fmt.Fprint({s}, string({b}))"),
            None => format!("fmt.Print(string({b}))"),
        };
        for (i, op) in ops.iter().enumerate() {
            let b = format!("__catb{id}_{i}");
            let e = format!("__cate{id}_{i}");
            let wr = w(&b);
            self.emit(&format!(
                "if {b}, {e} := os.ReadFile({op}); {e} != nil {{ st = 1 }} else {{ {wr} }}"
            ));
        }
        true
    }

    /// `cat <<EOF … EOF` / `cat <<< s` in statement position: with no FILE
    /// operand cat reproduces its stdin byte for byte, and the redirect
    /// already holds that text — printing it is the whole command. Any FILE
    /// operand, any other redirect, or an option refuses and keeps the fork.
    fn stdin_cat_stmt(&mut self, cmd: &IrExpr, args: &[IrExpr], redirects: &[IrExpr]) -> bool {
        if !matches!(cmd, IrExpr::Str(s, _) if s == "cat") {
            return false;
        }
        let stdin_operand = args.is_empty()
            || (args.len() == 1 && matches!(&args[0], IrExpr::Str(s, _) if s == "-"));
        if !stdin_operand || redirects.len() != 1 {
            return false;
        }
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let Some(NativeRedir::StdinStr(text)) = self.native_redirect(&redirects[0]) else {
            return false;
        };
        if self.todo != todo {
            self.sides.truncate(sides);
            self.tmp = tmp;
            self.todo = todo;
            return false;
        }
        self.flush_sides();
        self.emit(&format!("fmt.Print({text});"));
        if self.need_st {
            self.emit("st = 0;");
        }
        true
    }

    /// `cp SRC DST` — `os.Open`/`os.Create`/`io.Copy`, keeping GNU's two
    /// observable rules the corpus relies on: the destination is created
    /// with the SOURCE's mode, and a DST that is an existing directory
    /// receives `SRC`'s basename.
    fn cp_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(ops) = self.copy_pair(argv) else {
            return false;
        };
        let id = self.new_tmp();
        let (src, dst) = (format!("__cpsrc{id}"), format!("__cpdst{id}"));
        let (d, f, e, c) = (
            format!("__cpf{id}"),
            format!("__cpg{id}"),
            format!("__cpe{id}"),
            format!("__cpc{id}"),
        );
        self.emit(&format!("{src} := {}; {dst} := {};", ops.0, ops.1));
        self.emit("st = 0;");
        self.emit(&format!(
            "if __cpi{id}, __cpie{id} := os.Stat({dst}); __cpie{id} == nil && __cpi{id}.IsDir() {{ {dst} = filepath.Join({dst}, filepath.Base({src})) }}"
        ));
        self.emit(&format!("if {f}, {e} := os.Open({src}); {e} != nil {{ st = 1 }} else {{"));
        self.depth += 1;
        self.emit(&format!("if {d}, {e} := os.Create({dst}); {e} != nil {{ st = 1 }} else {{"));
        self.depth += 1;
        self.emit(&format!(
            "if _, {c} := io.Copy({d}, {f}); {c} != nil {{ st = 1 }}"
        ));
        self.emit(&format!("{d}.Close()"));
        self.emit(&format!(
            "if __csi{id}, __csie{id} := {f}.Stat(); __csie{id} == nil {{ os.Chmod({dst}, __csi{id}.Mode()) }}"
        ));
        self.depth -= 1;
        self.emit("}");
        self.emit(&format!("{f}.Close()"));
        self.depth -= 1;
        self.emit("}");
        self.need_st = true;
        true
    }

    /// `mv SRC DST` — `os.Rename`, with GNU's directory operand rule and its
    /// cross-device fallback (copy, then unlink the source).
    fn mv_stmt(&mut self, argv: &[IrExpr]) -> bool {
        let Some(ops) = self.copy_pair(argv) else {
            return false;
        };
        let id = self.new_tmp();
        let (src, dst) = (format!("__mvsrc{id}"), format!("__mvdst{id}"));
        let (s, d, e, c) = (
            format!("__mvf{id}"),
            format!("__mvg{id}"),
            format!("__mve{id}"),
            format!("__mvc{id}"),
        );
        self.emit(&format!("{src} := {}; {dst} := {};", ops.0, ops.1));
        self.emit("st = 0;");
        self.emit(&format!(
            "if __mvi{id}, __mvie{id} := os.Stat({dst}); __mvie{id} == nil && __mvi{id}.IsDir() {{ {dst} = filepath.Join({dst}, filepath.Base({src})) }}"
        ));
        // A rename that fails for any reason (EXDEV being the interesting
        // one) falls back to the copy GNU mv performs; if that fails too the
        // status is 1 either way, so the fallback costs nothing.
        self.emit(&format!("if {e} := os.Rename({src}, {dst}); {e} != nil {{"));
        self.depth += 1;
        self.emit(&format!("if {s}, {e} := os.Open({src}); {e} != nil {{ st = 1 }} else {{"));
        self.depth += 1;
        self.emit(&format!("if {d}, {e} := os.Create({dst}); {e} != nil {{ st = 1 }} else {{"));
        self.depth += 1;
        self.emit(&format!(
            "if _, {c} := io.Copy({d}, {s}); {c} != nil {{ st = 1 }} else if os.Remove({src}) != nil {{ st = 1 }}"
        ));
        self.emit(&format!("{d}.Close()"));
        self.depth -= 1;
        self.emit("}");
        self.emit(&format!("{s}.Close()"));
        self.depth -= 1;
        self.emit("}");
        self.depth -= 1;
        self.emit("}");
        self.need_st = true;
        true
    }

    /// The `SRC DST` operand pair `cp`/`mv` accept here: exactly two literal
    /// paths. A third operand means "move these into a directory", `-t`/`-t`
    /// take operands of their own, and any other flag changes what the
    /// command means — all keep the fork.
    fn copy_pair(&mut self, argv: &[IrExpr]) -> Option<(String, String)> {
        let mut ops: Vec<&IrExpr> = Vec::new();
        for a in argv {
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') => return None,
                _ if !fs_literal_operand(a) => return None,
                _ => ops.push(a),
            }
        }
        if ops.len() != 2 {
            return None;
        }
        let src = self.expr_str(ops[0]);
        let dst = self.expr_str(ops[1]);
        self.flush_sides();
        Some((src, dst))
    }

    /// External commands Go expresses natively, so the renderer drops the
    /// fork/exec entirely (AGENTS.md §2). Currently `rm`, `mkdir`, `touch`,
    /// `rmdir`, `ln`, `cmp`, `sleep`, `ls`, `cat`, `cp` and `mv`; any argv
    /// outside their exact shape falls through to `runCmd`.
    fn fs_argcmd_stmt(&mut self, cmd: &str, args: &[IrExpr]) -> bool {
        // `exec` Call shape is (cmd, [argv…]), same as exec_builtin_stmt
        let argv: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items.as_slice(),
            _ => &[],
        };
        match cmd {
            "rm" => self.rm_stmt(argv),
            "mkdir" => self.mkdir_stmt(argv),
            "touch" => self.touch_stmt(argv),
            "rmdir" => self.rmdir_stmt(argv),
            "ln" => self.ln_stmt(argv),
            "cmp" => self.cmp_stmt(argv),
            "sleep" => self.sleep_stmt(argv),
            "ls" => self.ls_stmt(argv),
            "cat" => self.cat_stmt(argv),
            "cp" => self.cp_stmt(argv),
            "mv" => self.mv_stmt(argv),
            "grep" => {
                let Some(v) = self.grep_expr(argv) else {
                    return false;
                };
                self.flush_sides();
                self.emit(&format!("st = {v};"));
                self.need_st = true;
                true
            }
            _ => false,
        }
    }

    /// `grep [flags] PAT FILE` as one Go expression (an IIFE yielding grep's
    /// STATUS) that writes grep's stdout as it scans: `os.ReadFile` +
    /// `regexp` instead of forking grep (AGENTS.md §2 — the stdlib call is
    /// the target-native lowering, the child is the fallback).
    fn grep_expr(&mut self, argv: &[IrExpr]) -> Option<String> {
        self.grep_scan(argv, GrepOut::Status)
    }

    /// `$(grep …)` — the same scan as `grep_expr`, yielding grep's stdout as
    /// a Go string instead of printing it. A capture and a statement form
    /// that disagree about matching are a silent wrong-value bug, so this
    /// must stay ONE template (`grep_scan`), never a second copy.
    fn grep_capture_body(&mut self, body: &[IrStmt]) -> Option<String> {
        let args = lone_exec_args(body)?;
        match args.first() {
            Some(IrExpr::Str(c, _)) if c == "grep" => {}
            _ => return None,
        }
        let argv: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items.as_slice(),
            _ => &[],
        };
        self.grep_scan(argv, GrepOut::Value)
    }

    /// `$(ls …)` / `$(cat …)` as a Go string, or `None` when the body is
    /// anything else. Delegates to the statement template so the capture and
    /// the statement can never disagree.
    fn fs_capture_body(&mut self, body: &[IrStmt]) -> Option<String> {
        let args = lone_exec_args(body)?;
        let Some(IrExpr::Str(cmd, _)) = args.first() else {
            return None;
        };
        let cmd = cmd.clone();
        let argv: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items.as_slice(),
            _ => &[],
        };
        match cmd.as_str() {
            "ls" => self.ls_value(argv),
            "cat" => self.cat_value(argv),
            _ => None,
        }
    }

    /// REFUSE > GUESS, because a wrong status silently flips an `if`/`&&`
    /// arm's direction:
    ///   * flags — only `-q -n -c -v -i -w -x -E -F` and the single-operand
    ///     name flags `-l -L -h -H` (the ones this template reproduces
    ///     exactly). `-o`/`-r`/`-A`/`-B`/`-C`/`-m`/`-f`/`-e` have no exact
    ///     counterpart here.
    ///   * operands — exactly one FILE (grep prefixes `name:` for several),
    ///     no `-` operand (stdin), no glob metacharacters (the child would
    ///     get the WORD, the glob is the shell's job), no option-like path.
    fn grep_scan(&mut self, argv: &[IrExpr], out: GrepOut) -> Option<String> {
        let (mut quiet, mut numbered, mut counted) = (false, false, false);
        let (mut invert, mut word, mut whole) = (false, false, false);
        let (mut no_prefix, mut force_prefix) = (false, false);
        let mut list_mode = 0u8;
        let mut flags = String::new();
        let mut i = 0usize;
        while let Some(IrExpr::Str(a, _)) = argv.get(i) {
            if a == "--" {
                i += 1;
                break;
            }
            if !a.starts_with('-') || a.len() < 2 {
                break;
            }
            for c in a[1..].chars() {
                match c {
                    'q' => quiet = true,
                    'n' => numbered = true,
                    'c' => counted = true,
                    'v' => invert = true,
                    'i' => flags.push('i'),
                    'E' => flags.push('E'),
                    'F' => flags.push('F'),
                    'w' => word = true,
                    'x' => whole = true,
                    'l' => list_mode = 1,
                    'L' => list_mode = 2,
                    'h' => no_prefix = true,
                    'H' => force_prefix = true,
                    _ => return None,
                }
            }
            i += 1;
        }
        // `-l` answers "which file", not "which lines": the per-hit text and
        // the `-c` total are both superseded. `-L` (files WITHOUT a match)
        // additionally prints the name for a file grep could not READ, which
        // this template reports as status 2 instead — so `-L` stays refused.
        if list_mode == 2 || (list_mode == 1 && (counted || numbered)) {
            return None;
        }
        let only_name = list_mode == 1;
        let pat = match argv.get(i) {
            Some(IrExpr::Str(p, _)) if !p.starts_with('-') || flags.contains('E') => p.clone(),
            _ => return None,
        };
        i += 1;
        // one operand, and it must be the LAST word (multi-file grep prefixes
        // every hit with the file name)
        if argv.len() != i + 1 {
            return None;
        }
        let file_arg = &argv[i];
        if let IrExpr::Str(s, _) = file_arg {
            if s == "-" || s.starts_with('-') || s.contains(['*', '?', '[', '~']) {
                return None;
            }
        }
        let mut re = grep_re_body(&pat, &flags);
        if word {
            re = format!("\\b(?:{re})\\b");
        }
        if whole {
            re = format!("^(?:{re})$");
        }
        let file = self.expr_str(file_arg);
        let idx = if numbered { "__gi, __gl" } else { "_, __gl" };
        let inv = if invert { "!" } else { "" };
        // `-H` prefixes every hit with the operand as written; with one
        // operand grep does NOT prefix by default, and `-h` never does.
        let line = if numbered {
            "fmt.Sprintf(\"%d:%s\\n\", __gi+1, __gl)".to_string()
        } else {
            "__gl + \"\\n\"".to_string()
        };
        let body = if force_prefix && !no_prefix {
            format!("__go += {file} + \":\" + {line}")
        } else {
            format!("__go += {line}")
        };
        // `-q`/`-c` print nothing per hit; `-c` prints the total afterwards;
        // `-l` prints the name once, after the scan.
        let hit = if quiet || counted || only_name {
            ""
        } else {
            body.as_str()
        };
        let count_out = if counted {
            "; __go = fmt.Sprintf(\"%d\\n\", __gc)"
        } else {
            ""
        };
        let name_out = if only_name {
            format!("; if __gc > 0 {{ __go = {file} + \"\\n\" }}")
        } else {
            String::new()
        };
        // Status mode prints the hits and answers grep's exit status; value
        // mode answers the hits themselves (`$(grep …)` strips the trailing
        // newlines the printer would have written anyway).
        let (sig, tail) = match out {
            GrepOut::Status => (
                "func() int64",
                "fmt.Print(__go); if __gc > 0 { return 0 }; return 1",
            ),
            GrepOut::Value => (
                "func() string",
                "return strings.TrimRight(__go, \"\\n\")",
            ),
        };
        let err = if out == GrepOut::Value { "\"\"" } else { "2" };
        Some(format!(
            "{sig} {{ __gre, __gce := regexp.Compile({re}); if __gce != nil {{ return {err} }}; \
             __gd, __gde := os.ReadFile({file}); if __gde != nil {{ return {err} }}; \
             __gls := strings.Split(string(__gd), \"\\n\"); \
             if len(__gls) > 0 && __gls[len(__gls)-1] == \"\" {{ __gls = __gls[:len(__gls)-1] }}; \
             __gc := 0; __go := \"\"; \
             for {idx} := range __gls {{ __gm := __gre.MatchString(__gl); \
             if {inv}__gm {{ __gc++{hit} }} }}{count_out}{name_out}; \
             {tail} }}()",
            sig = sig,
            re = Self::go_str(&re),
            file = file,
            idx = idx,
            inv = inv,
            err = err,
            // the per-hit statement only exists when something is printed
            hit = if hit.is_empty() {
                String::new()
            } else {
                format!("; {hit}")
            },
            count_out = count_out,
            name_out = name_out,
            tail = tail,
        ))
    }

    /// `chain_operand`'s arm for the natively-lowered external commands:
    /// capture the statements UNINDENTED, exactly like `chain_builtin`, and
    /// reject the attempt wholesale if the lowerer refuses — so a refusal
    /// leaves no read marked and no side statement queued.
    fn chain_fs_argcmd(&mut self, cmd: &str, args: &[IrExpr]) -> Option<(Vec<String>, String)> {
        let need_st_before = self.need_st;
        let sides_before = self.sides.len();
        let saved_depth = self.depth;
        self.depth = 0;
        let saved = std::mem::take(&mut self.out);
        let handled = self.fs_argcmd_stmt(cmd, args);
        let stmts = std::mem::replace(&mut self.out, saved);
        self.depth = saved_depth;
        if !handled {
            self.need_st = need_st_before;
            self.sides.truncate(sides_before);
            return None;
        }
        Some((stmts, "st".to_string()))
    }

    /// Builtin exec statements: echo/printf/exit/cd/let/local/… Returns
    /// true if handled.
    fn exec_builtin_stmt(&mut self, cmd: &str, args: &[IrExpr]) -> bool {
        let argv: &[IrExpr] = match args.get(1) {
            Some(IrExpr::Array(items)) => items.as_slice(),
            _ => &[],
        };
        // Absolute coreutils paths lower exactly like the builtins (see
        // `echo_cmd`).
        match echo_cmd(cmd).unwrap_or(cmd) {
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
                // `cd` with no operand goes to $HOME, and its status is the
                // chdir result — a failed cd is exactly what `cd X || exit 1`
                // tests, so it must reach `st` rather than a fixed 0.
                let path = match argv.first() {
                    Some(p) => self.expr_str(p),
                    None => "os.Getenv(\"HOME\")".to_string(),
                };
                self.flush_sides();
                self.emit(&format!(
                    "if err := os.Chdir({path}); err != nil {{ st = 1 }} else {{ st = 0 }};"
                ));
                self.need_st = true;
                true
            }
            "pwd" => {
                // `pwd` with no operands prints the working directory —
                // `os.Getwd`, not a `/bin/pwd` child (AGENTS.md §2). Any
                // operand (`-P`/`-L`, which bash mostly ignores anyway)
                // keeps the exec.
                if !argv.is_empty() {
                    return false;
                }
                self.flush_sides();
                let id = self.new_tmp();
                self.emit(&format!(
                    "if __pwd{id}, __pwde{id} := os.Getwd(); __pwde{id} == nil {{ \
                     fmt.Println(__pwd{id}); st = 0 }} else {{ st = 1 }};"
                ));
                self.need_st = true;
                true
            }
            "date" => {
                // `date +fmt` prints the time — `time.Now().Format` with
                // the mapped layout, not a child (AGENTS.md §2). Same
                // refusal set as the capture form.
                if let [one] = argv {
                    if let Some(fmt) = date_format_lit(one) {
                        if let Some(layout) = date_layout(&fmt) {
                            self.flush_sides();
                            self.emit(&format!(
                                "fmt.Println(time.Now().Format({}));",
                                Self::go_str(&layout)
                            ));
                            if self.need_st {
                                self.emit("st = 0;");
                            }
                            return true;
                        }
                    }
                }
                false
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
                self.declare_builtin(argv);
                true
            }
            // `echo`, and the `printf` forms whose format the native
            // formatter reproduces: the statement writes its words itself
            // instead of spawning `/bin/echo` (no child, no env literal).
            // Anything richer keeps the exec path below.
            "echo" | "printf" if self.stmt_builtin_output(cmd, argv) => true,
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
                            // Convert by hoisted type, exactly as `env_lit_for`
                            // does: only an `any` var needs the boxing
                            // coercion.
                            let v = if self.declared_go_string(arg) {
                                m
                            } else if self.declared_go_int(arg) {
                                format!("strconv.FormatInt({m}, 10)")
                            } else {
                                format!("s2s({m})")
                            };
                            self.emit(&format!("os.Setenv({}, {v});", Self::go_str(arg)));
                        }
                    }
                }
                true
            }
            "unset" => {
                for a in argv {
                    if let IrExpr::Str(name, _) = a {
                        let m = self.go_ident(name);
                        // bash has no null: an unset scalar reads as the
                        // EMPTY STRING (`${x}` → "", `${x:-default}` →
                        // "default"). Storing `nil` in an untyped slot forced
                        // `s2s(v)` at every read to recover that same "",
                        // so the slot could never hoist as a Go `string` —
                        // storing "" directly is the native form AND the
                        // faithful value (see `promotable_scalars`).
                        // Collections and function values have no "" zero, so
                        // those keep `nil`.
                        if self.is_arr(name)
                            || self.is_assoc(name)
                            || self.unset_collections.contains(name)
                            || self.functions.contains(name)
                        {
                            self.mark_written(name);
                            self.emit(&format!("{m} = nil;"));
                        } else if self.declared_go_int(name) {
                            self.mark_written(name);
                            let sync = self.sync_inline(name);
                            self.emit(&format!("{m} = 0;{sync}"));
                        } else {
                            self.mark_str_written(name);
                            let sync = self.sync_inline(name);
                            self.emit(&format!("{m} = \"\";{sync}"));
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
                // `readInto` writes through `*string`, so only a var that
                // hoists as Go `string` can be passed by address. An
                // untyped (`any`) or int64 slot — `var line any` is what an
                // unannotated `read -r line; echo "$line"` produces — used
                // to be passed as `&line` and the render did not COMPILE
                // (`cannot use &line (value of type *any) as *string`).
                // Read the field into a temp string and store it after,
                // boxing exactly as the declared slot expects.
                let mut stores: Vec<(String, String, String)> = Vec::new();
                for n in &names {
                    self.mark_written(n);
                    let m = self.go_ident(n);
                    if self.declared_go_string(n) {
                        ps.push(format!("&{m}"));
                    } else {
                        let id = self.new_tmp();
                        ps.push(format!("&__rv{id}"));
                        stores.push((n.clone(), m, id));
                    }
                }
                self.flush_sides();
                for (_, _, id) in &stores {
                    self.emit(&format!("var __rv{id} string;"));
                }
                self.emit(&format!("_ = readInto({});", ps.join(", ")));
                for (n, m, id) in &stores {
                    if self.is_arr(n) {
                        // `read a` on an array assigns element 0 and leaves
                        // the other elements alone.
                        self.emit(&format!(
                            "if len({m}) > 0 {{ {m}[0] = __rv{id} }} else {{ {m} = []any{{__rv{id}}} }};"
                        ));
                    } else if self.declared_go_int(n) {
                        let sync = self.sync_inline(n);
                        self.emit(&format!("{m} = s2i(__rv{id});{sync}"));
                    } else {
                        let sync = self.sync_inline(n);
                        self.emit(&format!("{m} = __rv{id};{sync}"));
                    }
                }
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            "shift" => {
                // `shift [n]` — drop the first n positional params. In a
                // function that is the shared fArgs stack; at top level it
                // is the script-arg offset. n defaults to 1; shifting past
                // the end (or a negative count) fails with status 1 and
                // leaves the params unchanged — exactly like bash.
                let n = match argv.first() {
                    Some(IrExpr::Str(s, _)) => s.trim().to_string(),
                    Some(other) => format!("strconv.Itoa(int(s2i({})))", self.expr_str(other)),
                    None => "1".to_string(),
                };
                let n = if n.parse::<i64>().map_or(false, |v| v >= 0) {
                    n
                } else if argv.first().is_some_and(|a| matches!(a, IrExpr::Str(..))) {
                    // a literal bash would reject (`shift foo`, `shift -1`)
                    self.flush_sides();
                    self.emit("st = 1;");
                    self.need_st = true;
                    return true;
                } else {
                    format!("({n})")
                };
                self.flush_sides();
                if self.in_fn > 0 {
                    self.emit(&format!("st = doShiftF({n});"));
                } else {
                    self.emit(&format!("st = doShiftS({n});"));
                }
                self.need_st = true;
                true
            }
            "true" | ":" => {
                if self.need_st {
                    self.emit("st = 0;");
                }
                true
            }
            "false" => {
                self.flush_sides();
                self.emit("st = 1;");
                self.need_st = true;
                true
            }
            "test" => {
                // `test …` — the same `test_render` the `[ ]` conditions
                // use, so `test -f "$f"` is a native file check instead of
                // a `/usr/bin/test` child (AGENTS.md §2). The argv joins
                // exactly like the shell-text reconstruction (literals
                // single-quoted), so operands with spaces stay one word.
                // REFUSE > GUESS: anything `test_render` cannot parse keeps
                // the exec (and a user-defined `test` function still wins —
                // `false` below routes there).
                if argv.is_empty() {
                    // `test` with no args is false (status 1).
                    self.flush_sides();
                    self.emit("st = 1;");
                    self.need_st = true;
                    return true;
                }
                let todo = self.todo;
                let sides = self.sides.len();
                let tmp = self.tmp;
                if let Some(c) = self.test_argv_bool(argv) {
                    if self.todo == todo {
                        self.flush_sides();
                        self.emit(&format!("st = b2i(!({c}));"));
                        self.need_st = true;
                        return true;
                    }
                }
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
                false
            }
            "set" | "umask" | "trap" | "return" => true,
            "wait" => {
                // `wait` joins all background jobs (no operands in the
                // corpus subset); a bare `wait` with no jobs is status 0.
                // Fork isolation: restore every var a background body
                // saved at spawn, oldest save wins (it holds the
                // pre-background value).
                self.flush_sides();
                self.emit("for _, __wgd := range __wgds { __wgd.Wait() }; __wgds = nil;");
                for (m, t) in std::mem::take(&mut self.bg_saves) {
                    self.emit(&format!("{m} = {t};"));
                }
                self.emit("st = 0;");
                self.need_st = true;
                true
            }
            "eval" => {
                // eval 'code' — run the code in a bash -c child (native
                // evaluation is impossible; the child inherits the written
                // vars via env_lit). EXCEPTION: a lone `name=value …`
                // assignment lowers natively — the words are already
                // structured IR, so each value renders through the same
                // expression paths a direct assignment uses, and the
                // child's assignments (which would die with it) are
                // unnecessary. REFUSE > GUESS: anything but words shaped
                // like assignments (flags, redirections, compound words).
                if let Some(code) = argv.first() {
                    if let Some(lines) = self.eval_assign_native(code) {
                        for l in lines {
                            self.emit(&l);
                        }
                        if self.need_st {
                            self.emit("st = 0;");
                        }
                        self.need_st = true;
                        return true;
                    }
                    let text = if let IrExpr::Str(c, _) = code {
                        c.clone()
                    } else if let Some(t) = self.cmd_text_expr(code) {
                        t
                    } else {
                        self.mark_todo("builtin eval");
                        return true;
                    };
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
                // Bare `command` (no operand): a no-op that SUCCEEDS — bash
                // has nothing to spawn, and there is no `command` executable
                // for exec.Command to find.
                if cmd == "command" && argv.is_empty() {
                    self.emit("st = 0;");
                    self.need_st = true;
                    return true;
                }
                // `command -v NAME` prints the resolved path (`exec.LookPath`
                // reports the same found/missing) — no `bash -c` child.
                if cmd == "command" {
                    if let Some(name_expr) = Self::command_v_name(argv) {
                        let name = self.expr_str(name_expr);
                        self.flush_sides();
                        let id = self.new_tmp();
                        self.emit(&format!(
                            "if __cl{id}, __cle{id} := exec.LookPath({name}); __cle{id} == nil {{ \
                             fmt.Println(__cl{id}); st = 0 }} else {{ st = 1 }};"
                        ));
                        self.need_st = true;
                        return true;
                    }
                }
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
                    let text = parts.join(" ");
                    let env = self.env_lit_for(Some(&text));
                    self.emit(&format!("st = redirRun({}, {env});", Self::go_str(&text)));
                    self.need_st = true;
                    true
                } else {
                    self.mark_todo(&format!("builtin {cmd}"));
                    true
                }
            }
            "exec" => {
                // `exec cmd …` — replace the shell: run and exit.
                // The head word is `exec` itself; the REAL command is the
                // first argv word. Re-dispatch on it (REFUSE > GUESS: flags
                // like -c/-a/-l, an empty argv, or a non-literal command
                // keep the historical shape).
                if cmd == "exec" {
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        if let Some(IrExpr::Str(real, _)) = items.first() {
                            if !real.starts_with('-') && real != "exec" {
                                let mut tail =
                                    Vec::with_capacity(items.len());
                                tail.push(items[0].clone());
                                tail.push(IrExpr::Array(items[1..].to_vec()));
                                if self.functions.contains(real) {
                                    self.call_function(real, &tail);
                                    self.need_st = true;
                                    self.emit("os.Exit(int(st));");
                                    return true;
                                }
                                let (todo, sides, tmp) =
                                    (self.todo, self.sides.len(), self.tmp);
                                self.need_st = true;
                                if self.exec_builtin_stmt(real, &tail)
                                    && self.todo == todo
                                {
                                    self.emit("os.Exit(int(st));");
                                    return true;
                                }
                                self.sides.truncate(sides);
                                self.tmp = tmp;
                                self.todo = todo;
                                if let Some((cmd_go, argv_go)) =
                                    self.exec_cmd_argv(&tail)
                                {
                                    let call = Self::exec_cmd_call(&cmd_go, &argv_go);
                                    self.emit(&format!(
                                        "st = runCmd({call}); os.Exit(int(st));"
                                    ));
                                    self.need_st = true;
                                    return true;
                                }
                                self.mark_todo("builtin exec");
                                return true;
                            }
                        }
                    }
                }
                if let Some((cmd_go, argv_go)) = self.exec_cmd_argv(args) {
                    let call = Self::exec_cmd_call(&cmd_go, &argv_go);
                    self.emit(&format!(
                        "st = runCmd({call}); os.Exit(int(st));"
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

    /// `echo`'s leading option words → (args, newline, escape). Shared by
    /// the printing statement and the native redirect write so the two can
    /// never disagree about what `echo -en` means.
    fn echo_options<'a>(argv: &'a [IrExpr]) -> (&'a [IrExpr], bool, bool) {
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
        (items, newline, escape)
    }

    /// echo's argument words as output parts, with `-e` escapes applied.
    fn echo_parts(&mut self, items: &[IrExpr], escape: bool) -> Option<Vec<Part>> {
        let parts = self.argv_parts(items)?;
        Some(if escape {
            parts
                .into_iter()
                .map(|p| match p {
                    Part::Lit(t) => Part::Arg(format!("eEsc({})", Self::go_str(&t))),
                    other => other,
                })
                .collect()
        } else {
            parts
        })
    }

    /// `echo [-n|-e|-E] args…` statement.
    fn echo_stmt(&mut self, argv: &[IrExpr]) {
        let (items, newline, escape) = Self::echo_options(argv);
        let parts = match self.echo_parts(items, escape) {
            Some(p) => p,
            None => {
                self.mark_todo("echo argv");
                return;
            }
        };
        let call = self.print_from_parts(parts, newline);
        self.flush_sides();
        self.emit(&format!("{call};"));
        if self.need_st {
            self.emit("st = 0;");
        }
    }

    /// `printf format args…` statement → bprintf.
    fn printf_stmt(&mut self, argv: &[IrExpr]) {
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
    fn declare_builtin(&mut self, argv: &[IrExpr]) {
        let mut last_flag = String::new();
        let mut i = 0usize;
        while i < argv.len() {
            let a = &argv[i];
            if let IrExpr::Str(arg, _) = a {
                if arg.starts_with('-') {
                    last_flag = arg.clone();
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
                    self.mark_written(name);
                    if last_flag == "-a" || last_flag == "-A" {
                        self.mark_arr(name);
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
                    let v = if self.is_num(name) {
                        format!("s2i({v})")
                    } else {
                        v
                    };
                    // `local x=…` stores TEXT unless the flag/verdict made
                    // the var numeric (`declare -i`, A2 Int) — the same
                    // string-write accounting `Assign` uses, so
                    // `local x="$(…)"` hoists as a Go `string`.
                    if last_flag != "-i" && !self.is_num(name) {
                        *self.writes_str.entry(name.to_string()).or_insert(0) += 1;
                    }
                    let sync = self.sync_inline(name);
                    self.emit(&format!("{m} = {v};{sync}"));
                    if self.need_st {
                        self.emit("st = 0;");
                    }
                    i = j;
                } else {
                    self.mark_written(arg);
                    if last_flag == "-a" || last_flag == "-A" {
                        self.mark_arr(arg);
                    }
                    let m = self.go_ident(arg);
                    self.emit(&format!("// declare {m}"));
                    i += 1;
                }
                last_flag.clear();
            } else if let IrExpr::Call { func, args: ca } = a {
                // `declare -a arr=(…)` / `typeset -a arr+=(…)` (and the
                // single-element `declare name=value` shape the core emits
                // as assign): the values arrive as a nested Call, not as
                // `name=(…)` text, so lower them exactly like a plain
                // array assignment.
                if func == "setArray" || func == "setArrayAppend" {
                    if let Some(IrExpr::Str(name, _)) = ca.first() {
                        self.mark_arr(name);
                        let m = self.go_ident(name);
                        let rhs =
                            self.setarray_rhs(&m, func == "setArrayAppend", ca);
                        self.flush_sides();
                        let sync = self.sync_inline(name);
                        self.emit(&format!("{m} = {rhs};{sync}"));
                        if self.need_st {
                            self.emit("st = 0;");
                        }
                    }
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
        self.emit(&format!("for {init_go}; ({cval} != 0); {update_go} {{"));
        self.loop_depth += 1;
        self.depth += 1;
        for s in body {
            self.stmt(s);
        }
        self.depth -= 1;
        self.loop_depth -= 1;
        self.emit("}");
        Some(String::new())
    }

    /// for-loop lowering.
    fn stmt_for(&mut self, var: &str, iter: &IrExpr, body: &[IrStmt]) {
        let m = self.go_ident(var);
        self.mark_written(var);
        // Like `while`: an empty iteration leaves $? at 0 in bash.
        let runflag = format!("__run{}", self.new_tmp());
        self.emit(&format!("{runflag} := false;"));
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
                self.emit(&format!("for _, {m} = range []{t}{{{inner}}} {{"));
                self.loop_depth += 1;
                self.depth += 1;
                self.emit(&format!("{runflag} = true;"));
                self.emit_sync(var);
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("}");
                self.emit(&format!("if !{runflag} {{ st = 0; }}"));
                self.need_st = true;
            }
            Some(ForList::Runtime { pre, slice }) => {
                self.emit(&pre);
                let vn = self.new_tmp();
                self.emit(&format!("for _, {vn} := range {slice} {{"));
                self.loop_depth += 1;
                self.depth += 1;
                self.emit(&format!("{runflag} = true;"));
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
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("}");
                self.emit(&format!("if !{runflag} {{ st = 0; }}"));
                self.need_st = true;
            }
            None => {
                if let IrExpr::Range { start, end } = iter {
                    self.emit(&format!("for {m} = {start}; {m} <= {end}; {m}++ {{"));
                    self.loop_depth += 1;
                    self.depth += 1;
                    self.emit(&format!("{runflag} = true;"));
                    self.emit_sync(var);
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.loop_depth -= 1;
                    self.emit("}");
                    self.emit(&format!("if !{runflag} {{ st = 0; }}"));
                    self.need_st = true;
                    return;
                }
                self.mark_todo(&format!("For iter {:?}", iter));
            }
        }
    }

    /// `<loop printing lines> | sort` — a pipeline whose first stage is a
    /// loop and whose only other stage is a bare `sort`.
    ///
    /// Both halves have native Go forms, so neither needs a child: the loop
    /// output is a `[]string` with one entry per printed `echo` line, and
    /// `sort.Strings` is the ordering a flagless `sort` applies. The `bash
    /// -c` fallback this replaces was not merely slower — it was WRONG for a
    /// loop over a collection, because the reconstructed script's env
    /// literal carries only scalars, so `${!map[@]}` expanded to nothing and
    /// the pipeline printed no lines at all.
    ///
    /// REFUSE > GUESS: exactly two stages; the second is `sort` with NO
    /// operand (`-n`/`-r`/`-u`/`-k`/a file all change the result); the body
    /// is nothing but `echo` (one line each, so `Println` reproduces it, and
    /// a flag such as `-n`/`-e` is refused for the same reason); and the
    /// iter must resolve to a list `stmt_for` already renders natively.
    /// Everything else keeps the fallback.
    fn stmt_print_loop_pipeline(&mut self, stages: &[&[IrStmt]]) -> bool {
        let [first, second] = stages else {
            return false;
        };
        let [IrStmt::For { var, iter, body }] = *first else {
            return false;
        };
        let [IrStmt::Expr(IrExpr::Call { func, args })] = *second else {
            return false;
        };
        if !matches!(func.as_str(), "builtin" | "exec") {
            return false;
        }
        if !matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "sort") {
            return false;
        }
        if !matches!(args.get(1), Some(IrExpr::Array(items)) if items.is_empty()) {
            return false;
        }
        if body.is_empty() {
            return false;
        }
        let Some(list) = self.for_list(iter) else {
            return false;
        };
        // Render the line texts BEFORE emitting anything: a refusal below
        // must leave the caller's rollback nothing to undo, and a body word
        // that has no native spelling is a refusal, not a `runCmd` (which
        // RETURNS AN EXIT CODE and writes to stdout — appending that code
        // and then sorting it is what printed three `0`s instead of the
        // three lines).
        let (todo, sides, tmp) = (self.todo, self.sides.len(), self.tmp);
        let mut lines: Vec<String> = Vec::with_capacity(body.len());
        for s in body {
            let IrStmt::Expr(call) = s else {
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
                return false;
            };
            let Some(text) = self.echo_line_text(call) else {
                self.sides.truncate(sides);
                self.tmp = tmp;
                self.todo = todo;
                return false;
            };
            lines.push(text);
        }
        let m = self.go_ident(var);
        self.mark_written(var);
        let pl = format!("__pl{}", self.new_tmp());
        let vn = format!("__v{}", self.new_tmp());
        let ln = format!("__l{}", self.new_tmp());
        self.emit(&format!("{pl} := []string{{}};"));
        match list {
            ForList::Static(items) => {
                let (t, elems) = if self.is_num(var) {
                    (
                        "int64",
                        items
                            .iter()
                            .map(|e| format!("s2i({e})"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                } else if self.is_str(var) {
                    ("string", items.join(", "))
                } else {
                    ("any", items.join(", "))
                };
                self.emit(&format!("for _, {vn} := range []{t}{{{elems}}} {{"));
            }
            ForList::Runtime { pre, slice } => {
                self.emit(&pre);
                self.emit(&format!("for _, {vn} := range {slice} {{"));
            }
        }
        self.loop_depth += 1;
        self.depth += 1;
        if self.is_num(var) {
            self.emit(&format!("{m} = s2i({vn});"));
        } else if self.is_str(var) {
            self.emit(&format!("{m} = {vn};"));
        } else {
            self.emit(&format!("{m} = s2s({vn});"));
        }
        self.emit_sync(var);
        for text in lines {
            self.flush_sides();
            self.emit(&format!("{pl} = append({pl}, {text});"));
        }
        self.depth -= 1;
        self.loop_depth -= 1;
        self.emit("}");
        self.emit(&format!("sort.Strings({pl});"));
        self.emit(&format!(
            "for _, {ln} := range {pl} {{ fmt.Println({ln}) }}"
        ));
        self.emit("st = 0;");
        self.need_st = true;
        true
    }

    /// The value of one `echo` line — exactly the words `echo` would write,
    /// without its terminating newline (the caller's `Println` supplies it).
    ///
    /// REFUSE > GUESS: `-n` suppresses the terminator (the caller would add a
    /// newline bash never wrote) and `-e` can expand one argument into
    /// several lines (which `sort` orders separately, not as one entry);
    /// `printf` is excluded because a format without a trailing newline is
    /// not a line either. A word the renderer cannot spell natively —
    /// `echo_parts` signals that with a TODO marker — refuses too, instead of
    /// falling back to a subprocess that would print to OUR stdout.
    fn echo_line_text(&mut self, e: &IrExpr) -> Option<String> {
        let IrExpr::Call { func, args } = e else {
            return None;
        };
        if !matches!(func.as_str(), "builtin" | "exec") {
            return None;
        }
        let (Some(IrExpr::Str(cmd, _)), Some(IrExpr::Array(argv))) = (args.first(), args.get(1))
        else {
            return None;
        };
        if echo_cmd(cmd) != Some("echo") {
            return None;
        }
        let (items, newline, escape) = Self::echo_options(argv);
        if !newline || escape {
            return None;
        }
        let todo = self.todo;
        let parts = self.echo_parts(items, false);
        match parts {
            Some(parts) if self.todo == todo => Some(self.parts_str(parts)),
            _ => {
                self.todo = todo;
                None
            }
        }
    }

    /// `echo words…` / `printf fmt args…` in statement position as a direct
    /// stdout write. Exactly the argv shapes `expr_str`'s own echo/printf
    /// lowering produces (the capture path uses the same rule), so the
    /// statement and `$(…)` forms agree byte for byte. `false` keeps the
    /// exec path (a flag the builtin policy does not model, a `%q` format).
    fn stmt_builtin_output(&mut self, cmd: &str, argv: &[IrExpr]) -> bool {
        if cmd == "printf" && !native_stage_builtin("printf", argv) {
            return false;
        }
        if cmd == "echo" && !native_stage_builtin("echo", argv) {
            return false;
        }
        let call = IrExpr::Call {
            func: "builtin".to_string(),
            args: std::iter::once(IrExpr::Str(
                cmd.to_string(),
                crate::ir::StrStyle::DoubleQuoted,
            ))
            .chain(std::iter::once(IrExpr::Array(argv.to_vec())))
            .collect(),
        };
        let text = self.expr_str(&call);
        self.flush_sides();
        if self.need_st {
            self.emit("st = 0;");
        }
        self.emit(&format!("fmt.Print({text});"));
        true
    }

    fn emit_sync(&mut self, var: &str) {
        if self.need_vars {
            let m = self.go_ident(var);
            self.emit(&format!("vars[{}] = {m};", Self::go_str(var)));
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
                                    let arr_name = if matches!(args.get(2), Some(IrExpr::Str(a, _)) if a == "@")
                                    {
                                        format!("{name}[@]")
                                    } else {
                                        name.clone()
                                    };
                                    if arr_name.ends_with("[@]") {
                                        self.need_vars = true;
                                        let t = self.new_tmp();
                                        // `${!map[@]}` iterates the assoc
                                        // KEYS (or an indexed array's
                                        // indices), not the values.
                                        if let Some(base) = arr_name
                                            .strip_suffix("[@]")
                                            .and_then(|n| n.strip_prefix('!'))
                                        {
                                            return Some(ForList::Runtime {
                                                pre: format!(
                                                    "{t} := arrKeys({});",
                                                    Self::go_str(base)
                                                ),
                                                slice: t,
                                            });
                                        }
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
                    // An unquoted glob word expands to its MATCHES at run
                    // time, so a list carrying one is not a static list:
                    // `for f in *.{txt,log}` iterates files, not the literal
                    // pattern (measured: the marker text was printed).
                    if let IrExpr::Str(s, _) = it {
                        if expr_glob(it) {
                            let pat = Self::go_str(&strip_sh2glob(s));
                            let t = self.new_tmp();
                            return Some(ForList::Runtime {
                                pre: format!("{t} := globArgs({pat});"),
                                slice: t,
                            });
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
                if !self.declared(name) {
                    return vec![Part::Arg(format!("os.Getenv({})", Self::go_str(name)))];
                }
                let m = self.ident_of(name);
                vec![Part::Arg(m)]
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    return vec![Part::Arg(format!("os.Getenv({})", Self::go_str(name)))];
                }
                let m = self.ident_of(name);
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

    /// Parts → one Go string EXPRESSION (a `fmt.Sprintf` when any part is
    /// an argument, a plain literal otherwise).
    ///
    /// The `%%` escape is applied ONLY on the Sprintf arm: in the literal
    /// arm the text is emitted verbatim, so escaping it there doubled every
    /// percent in the output (`echo "100%"` printed `100%%` — the literal
    /// `"100%%"` never goes through a format parser).
    fn parts_str(&mut self, parts: Vec<Part>) -> String {
        let has_args = parts.iter().any(|p| matches!(p, Part::Arg(_)));
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                Part::Lit(t) => {
                    if has_args {
                        fmt.push_str(&t.replace('%', "%%"));
                    } else {
                        fmt.push_str(&t);
                    }
                }
                Part::Arg(v) => {
                    fmt.push_str("%v");
                    args.push(v);
                }
            }
        }
        if args.is_empty() {
            Self::go_str(&fmt)
        } else {
            format!("fmt.Sprintf({}, {})", Self::go_str(&fmt), args.join(", "))
        }
    }

    /// Parts → one fmt.Println(fmt.Sprintf(...)) / fmt.Print(...) string.
    fn print_from_parts(&mut self, parts: Vec<Part>, newline: bool) -> String {
        let inner = self.parts_str(parts);
        if newline {
            format!("fmt.Println({inner})")
        } else {
            format!("fmt.Print({inner})")
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect written vars so declarations are hoisted.
        let mut written: BTreeSet<String> = BTreeSet::new();
        let mut arrays: BTreeSet<String> = BTreeSet::new();
        collect_written(&prog.stmts, &mut written, &mut arrays);
        self.written = written;
        self.arrays = arrays;
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
                    If { then, elsifs, else_, .. } => {
                        for b in then {
                            stack.push(b);
                        }
                        for (_, b) in elsifs {
                            for b in b {
                                stack.push(b);
                            }
                        }
                        for b in else_ {
                            stack.push(b);
                        }
                    }
                    While { body, .. } | DoWhile { body, .. } | Block(body)
                    | Subshell(body) | Function { body, .. } | For { body, .. } => {
                        for b in body {
                            stack.push(b);
                        }
                    }
                    // Every OTHER statement that carries a nested body must
                    // be walked too: a `((i+=2))` inside a `case` arm, a
                    // `for` body, a redirect group or a pipeline stage was
                    // invisible to this scan, so the var was NOT hoisted as
                    // int64 while its first (string) assignment rendered as
                    // `i = "0"` — `cannot use "0" as int64 value in
                    // assignment` (063_hard_to_parse). The scan's answer must
                    // not depend on which container the arith write sits in.
                    Try {
                        body,
                        excepts,
                        else_body,
                        finally_body,
                    } => {
                        for b in body.iter().chain(else_body).chain(finally_body) {
                            stack.push(b);
                        }
                        for e in excepts {
                            for b in &e.body {
                                stack.push(b);
                            }
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
                        for s in stages {
                            for b in s {
                                stack.push(b);
                            }
                        }
                    }
                    Ext(x) => {
                        for b in x.children() {
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
        // The gate compares exit codes, so the status is ALWAYS tracked —
        // the program exits with the last command's status (see the
        // epilogue). Unconditional: a `$?` read anywhere, or nowhere, must
        // agree with bash.
        self.need_st = true;
        let _ = flags.need_st;

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
        // rc-faithful exit: the program's status is the last command's,
        // exactly like bash — the gate compares exit codes, not just
        // stdout, so falling off the end must not collapse to 0.
        if self.need_st {
            self.emit("os.Exit(int(st));");
        } else {
            self.emit("return;");
        }
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
            "\"crypto/sha256\"",
            "\"crypto/sha512\"",
            "\"encoding/hex\"",
            "\"fmt\"",
            "\"io\"",
            "\"os\"",
            "\"os/exec\"",
            "\"path\"",
            "\"path/filepath\"",
            "\"regexp\"",
            "\"sort\"",
            "\"strconv\"",
            "\"strings\"",
            "\"sync\"",
            "\"syscall\"",
            "\"time\"",
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
            "\"bufio\"", "\"crypto/sha256\"", "\"crypto/sha512\"",
            "\"encoding/hex\"", "\"fmt\"", "\"io\"", "\"os\"", "\"os/exec\"",
            "\"path\"", "\"path/filepath\"", "\"regexp\"", "\"sort\"", "\"strconv\"", "\"strings\"",
            "\"sync\"", "\"syscall\"", "\"time\"", "\"unicode/utf8\"",
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
            if self.functions.contains(v) {
                // a user function's value is its output text (see in_fn)
                out.push(format!("    var {m} func() string"));
            } else if self.is_assoc(v) {
                out.push(format!("    var {m} map[string]any"));
            } else if self.is_arr(v) {
                out.push(format!("    var {m} []any"));
            } else if self.is_num(v) || self.arith_ints.contains(v) {
                out.push(format!("    var {m} int64"));
            } else if self.is_str(v) {
                out.push(format!("    var {m} string"));
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
    /// `$( … )` — a command substitution used as an arithmetic operand. The
    /// payload is the INNER command text, still unparsed (see
    /// `Render::arith_cmdsub`).
    CmdSub(String),
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
        // `name[key]` — exactly ONE subscript. A second group (`a[i][j]`) is
        // left UNCONSUMED on purpose: bash has no nested subscripting in
        // arithmetic ("invalid arithmetic operator"), and the leftover-token
        // check in the caller turns it into that same refusal instead of a
        // meaning we invented.
        let p = self.primary()?;
        if self.eat("[") {
            let key = self.expr()?;
            if !self.eat("]") {
                return None;
            }
            let MA::Var(name) = p else {
                return None;
            };
            return Some(MA::ArrIdx(name, Box::new(key)));
        }
        Some(p)
    }

    fn primary(&mut self) -> Option<MA> {
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
        // `$( … )` — checked BEFORE the `${`/`#`/`[` arms below, any of
        // which would otherwise read the command text as a name (`$(grep
        // '#')` would look like base notation, `$(x[0])` like an index).
        if let Some(inner) = t.strip_prefix("$(").and_then(|r| r.strip_suffix(')')) {
            return Some(MA::CmdSub(inner.to_string()));
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
            if let Some(arr) = rest.strip_suffix("[@]") {
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

/// bash's verdict on an arithmetic expression this renderer refused.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArithFail {
    /// bash rejects the expression itself — "syntax error in expression".
    /// Its outcome is well defined and reproducible: `(( … ))` is false,
    /// `let` returns 1, and `x=$(( … ))` leaves `x` UNSET (the assignment
    /// never happens, `$?` is 1). Lowering that is faithful, not a guess.
    Invalid,
    /// Valid bash arithmetic this renderer does not cover yet (a command
    /// substitution, a `${x:-y}` expansion, a quoted operand that is not a
    /// single atom). Keep the refusal and its TODO marker loud.
    Unsupported,
}

/// A double-quoted arithmetic operand survives quote removal only when it
/// is one atom: a number, `name`, `$name`, `${name}`. `"a b"` does not —
/// bash rejects it ("syntax error in expression").
fn arith_quoted_operand(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if parse_arith_num(t).is_some() {
        return true;
    }
    let t = t.strip_prefix('$').unwrap_or(t);
    let t = t
        .strip_prefix('{')
        .and_then(|x| x.strip_suffix('}'))
        .unwrap_or(t);
    !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Index of the `)` matching the `(` at `open`, or `None` when the span is
/// unbalanced or holds a construct this scan cannot account for.
///
/// The scan must survive the shapes a real command line has inside
/// `$( … )`: a subshell (`( … )`), a `)` that is QUOTED or escaped (it does
/// not close), and a nested `$( … )`. Quotes are skipped as spans — a
/// double-quoted span may contain `$(` whose own `)` is balanced inside it,
/// and skipping the whole span keeps the outer depth right. REFUSE > GUESS:
/// anything unterminated returns `None`, which leaves the arith refusal in
/// place instead of emitting a mis-parsed operand.
fn arith_cmdsub_end(b: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = open;
    while j < b.len() {
        match b[j] {
            '\\' => j += 2,
            '\'' | '"' => {
                let q = b[j];
                j += 1;
                while j < b.len() && b[j] != q {
                    if q == '"' && b[j] == '\\' {
                        j += 1;
                    }
                    j += 1;
                }
                if j >= b.len() {
                    return None;
                }
                j += 1;
            }
            '(' => {
                depth += 1;
                j += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
                j += 1;
            }
            _ => j += 1,
        }
    }
    None
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

/// Decimal non-negative integer — `cmp`'s `-n`/`-i` operands without GNU's
/// size suffixes. Anything else refuses the native lowering.
fn cmp_dec(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok()
}

/// A `sleep` operand → nanoseconds, GNU style: a decimal number with an
/// optional fractional part and an optional `s`/`m`/`h`/`d` unit. Done in
/// integer arithmetic on purpose — `0.1 * 1e9` in f64 is not exactly 1e8,
/// and a sleep that is 20ns off is a bug that only shows up as a flake.
fn sleep_ns(operand: &str) -> Option<i64> {
    let s = operand.trim();
    let (num, unit_ns): (&str, i64) = match s.as_bytes().last().copied() {
        Some(b's') => (&s[..s.len() - 1], 1_000_000_000),
        Some(b'm') => (&s[..s.len() - 1], 60_000_000_000),
        Some(b'h') => (&s[..s.len() - 1], 3_600_000_000_000),
        Some(b'd') => (&s[..s.len() - 1], 86_400_000_000_000),
        _ => (s, 1_000_000_000),
    };
    let (whole_txt, frac) = match num.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (num, None),
    };
    if whole_txt.is_empty() || !whole_txt.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    let mut total = whole_txt.parse::<i64>().ok()?.checked_mul(unit_ns)?;
    if let Some(f) = frac {
        // digits past the ninth are below nanosecond resolution — the shell
        // truncates there too
        let mut scale = unit_ns;
        for b in f.bytes().take(9) {
            scale /= 10;
            total = total.checked_add((b - b'0') as i64 * scale)?;
        }
    }
    Some(total)
}

/// `time.Sleep` call for a nanosecond total, in the largest unit that
/// divides it exactly (`time.Sleep(time.Second)`, not
/// `time.Sleep(time.Duration(1000000000))`).
fn sleep_call(ns: i64) -> String {
    for (unit, size) in [
        ("time.Hour", 3_600_000_000_000i64),
        ("time.Minute", 60_000_000_000),
        ("time.Second", 1_000_000_000),
        ("time.Millisecond", 1_000_000),
        ("time.Microsecond", 1_000),
        ("time.Nanosecond", 1),
    ] {
        if ns % size == 0 {
            return match ns / size {
                1 => format!("time.Sleep({unit})"),
                n => format!("time.Sleep({n} * {unit})"),
            };
        }
    }
    unreachable!("1ns divides every value")
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

/// The Go type a `test_operand` renders to. `Num` is an int64 expression,
/// `Str` is already a Go string (so `strify`'s coercion would be a no-op),
/// `Dyn` is `any`/unknown and needs the runtime coercion on either side.
#[derive(Clone, Copy, PartialEq)]
enum Opnd {
    Num,
    Str,
    Dyn,
}

/// What a `grep` scan answers: the exit STATUS (`grep …` as a command /
/// condition) or grep's STDOUT as a Go string (`$(grep …)`).
#[derive(Clone, Copy, PartialEq)]
enum GrepOut {
    Status,
    Value,
}

fn numify(e: String, k: Opnd, _r: &mut Render) -> String {
    if k == Opnd::Num {
        e
    } else {
        format!("s2i({e})")
    }
}

fn strify(e: String, k: Opnd, _r: &mut Render) -> String {
    // `Dyn` operands reach here already run through a rendering path that
    // may itself have coerced (`s2s(v)` → `s2s(s2s(v))`); the outer call is
    // the same identity the helper performs, so it would only box twice.
    if k == Opnd::Str || e.starts_with("s2s(") {
        e
    } else if k == Opnd::Num && e.parse::<i64>().is_ok() {
        // A numeric LITERAL's text is known at render time — `s2s(10)` boxed
        // an int64 into an `any` only to `%v`-format it back to "10".
        format!("\"{e}\"")
    } else if k == Opnd::Num {
        // `Opnd::Num` is Go int64 by construction, so its decimal text is
        // `strconv`'s — `s2s` boxed the value into an `any` (`%v`) for the
        // same string.
        format!("strconv.FormatInt({e}, 10)")
    } else {
        format!("s2s({e})")
    }
}

/// Does a string contain glob metacharacters?
/// Does a glob word actually EXPAND under bash's pathname-expansion
/// grammar? `has_glob` answers "is there a glob char", which is what
/// decides whether the word may be expanded — but a pattern written
/// inside QUOTES (`grep -E "([a-z]+)([0-9]+)\.([a-z]+)"`) reaches the
/// renderer with its metacharacters intact, and `grep -E "(foo|bar)[0-9]+"`
/// must reach grep as ONE argument, not as `globArgs(...)` spread into
/// whatever the CWD happens to match.
///
/// Every `*`, `?` or `[` that appears INSIDE a quote run makes the word
/// non-expanding: an unquoted metacharacter would have been bare, so a
/// quoted one is literal by construction. A word with no quote pairing at
/// all (`*.txt`, `file?.sh`) expands.

/// Drop a word's quote characters, keeping everything inside them: what the
/// shell hands a command (`"*.txt"` → `*.txt`, `a"b c"d` → `ab cd`).
fn strip_word_quotes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '\\' if i + 1 < b.len() => {
                out.push(b[i + 1]);
                i += 2;
            }
            '"' | '\'' => i += 1,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}
fn glob_expands(pat: &str) -> bool {
    // A word the shell already EXPANDED arrives with its metacharacters
    // gone; one whose metacharacters are still here was QUOTED in the
    // source (`grep -E "(foo|bar)[0-9]+"`, `grep -c ".*"`). Quoted
    // metacharacters are literal to the shell, so such a word must never
    // be spread through `globArgs` -- measured: spreading `.*` turned
    // `grep -c ".*"` into `grep -c` plus every file in the CWD and the
    // count came back empty.
    // Any metacharacter at all: the word was quoted, so it is literal.
    !(pat.contains('*') || pat.contains('?') || pat.contains('['))
}
fn has_glob(s: &str) -> bool {
    s.contains('*')
        || s.contains('?')
        || s.contains('[')
        || has_extglob(s)
}

/// Does this word undergo PATHNAME EXPANSION in the shell?
///
/// `has_glob` answers "does the text carry a metacharacter", which is NOT the
/// same question: quoting makes a metacharacter literal. The frontend records
/// whole-word quoting in `StrStyle`, so `grep -c ".*"` arrives as
/// `Str(".*", DoubleQuoted)` — a REGEX argument, not a glob. Spreading it
/// through `globArgs` handed grep every dotfile in the CWD as a separate
/// operand (measured: `016_grep_basic.sh` printed an `ls -A`-shaped listing
/// instead of a count).
///
/// A partially quoted word keeps its quote characters in the text, so the
/// scan tracks quote runs too: `a"*"b` is literal, `'*'*` still expands on
/// its unquoted `*`. Refusing to expand a word that the shell would expand is
/// a robustness win only if the pattern is fully quoted, which the style
/// check already covers — the text scan is the finer-grained twin.
fn str_expands_glob(s: &str, style: &crate::ir::StrStyle) -> bool {
    // The core prefixes an UNQUOTED glob word with `\x01SH2GLOB\x01`
    // (shir.rs GLOB_MAGIC) and that marker is AUTHORITATIVE: the style is
    // not reliable for these words — they arrive as `DoubleQuoted`, so the
    // style test below said "quoted, no expansion" and `ls file_*.txt`
    // passed the pattern through literally while `rm file_*.txt` even
    // leaked the marker into the filename.
    if s.starts_with('\u{1}') {
        return true;
    }
    if matches!(
        style,
        crate::ir::StrStyle::DoubleQuoted | crate::ir::StrStyle::SingleQuoted
    ) {
        return false;
    }
    let b: Vec<char> = s.chars().collect();
    let (mut sq, mut dq) = (false, false);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '\\' if !sq => i += 2,
            '\'' if !dq => {
                sq = !sq;
                i += 1;
            }
            '"' if !sq => {
                dq = !dq;
                i += 1;
            }
            '*' | '?' | '[' if !sq && !dq => return true,
            '@' | '+' | '!' if !sq && !dq && i + 1 < b.len() && b[i + 1] == '(' => return true,
            _ => i += 1,
        }
    }
    false
}

/// `strip_sh2glob` + `str_expands_glob` for the `IrExpr::Str` shape every
/// argv/operand scan uses.
fn expr_glob(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Str(s, st) if str_expands_glob(s, st))
}

/// An extglob head anywhere in the pattern (`@(…)`, `!(…)`, `?(…)`,
/// `+(…)`, `*(…)`): a sigil immediately followed by `(`. A literal
/// `@(`/`?(` (quoted paren in shell) still counts — the renderer treats
/// it as a pattern, matching bash's unquoted expansion.
fn has_extglob(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if matches!(b[i], b'@' | b'!' | b'?' | b'+' | b'*')
            && i + 1 < b.len()
            && b[i + 1] == b'('
        {
            return true;
        }
        i += 1;
    }
    false
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

/// The inverse of [`quote_sh`] for a single-quoted shell word.
fn unquote_sh(s: &str) -> String {
    let inner = s.strip_prefix('\'').and_then(|x| x.strip_suffix('\''));
    match inner {
        Some(x) => x.replace("'\\''", "'"),
        None => s.to_string(),
    }
}

fn quote_sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A mktemp template → (dir, Go pattern), mapping the trailing `XXX…` run
/// to Go's `*`. `None` refuses: no trailing run of 3+ `X`s, `X`s anywhere
/// else (both parts — Go would randomize what mktemp keeps literal), or an
/// empty file part.
fn mktemp_pattern(t: &str) -> Option<(String, String)> {
    if t.is_empty() || t.ends_with('/') {
        return None;
    }
    let (mut dir, file) = match t.rfind('/') {
        Some(i) => (t[..i].to_string(), &t[i + 1..]),
        // No directory part: mktemp creates in the CWD, so Go must too
        // (`""` would mean the temp dir instead).
        None => (".".to_string(), t),
    };
    // A root-level template (`/x.XXXXXX`): the directory is `/`, not the
    // temp dir.
    if dir.is_empty() && t.starts_with('/') {
        dir = "/".to_string();
    }
    if file.is_empty() || dir.contains('X') {
        return None;
    }
    let run = file.len() - file.trim_end_matches('X').len();
    let stem = &file[..file.len() - run];
    if run < 3 || stem.contains('X') {
        return None;
    }
    Some((dir, format!("{stem}*")))
}

/// A `date` format operand as literal text: a plain string, or an
/// interpolation of nothing but literals. Anything dynamic refuses (the
/// layout mapping needs the verbs statically).
fn date_format_lit(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => out.push_str(t),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// A `date +fmt` format → Go layout, mapping the unambiguous numeric
/// verbs (`%Y` year, `%m` month, `%d` day, `%H` hour, `%M` minute, `%S`
/// second; `%%` a literal percent). `None` refuses everything else —
/// locale names (`%A`, `%b`), week/year numbers (`%U`, `%V`, `%j`),
/// timezone (`%Z`, `%z`), epoch (`%s`) and any other verb.
fn date_layout(fmt: &str) -> Option<String> {
    let body = fmt.strip_prefix('+')?;
    if body.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'Y' => out.push_str("2006"),
            'm' => out.push_str("01"),
            'd' => out.push_str("02"),
            'H' => out.push_str("15"),
            'M' => out.push_str("04"),
            'S' => out.push_str("05"),
            '%' => out.push('%'),
            _ => return None,
        }
    }
    Some(out)
}

/// True when `t` is exactly one simple shell reference — `$v`, `${v}`,
/// `$1`, `$?`, `$#`, `$$`, `$@`, `$*` — the operand shapes `test_operand`
/// resolves faithfully. Anything composed (`$v/suffix`, `pre$v`, `$(…)`)
/// is `false`: re-parsing it as an operand would misread the name.
fn is_simple_test_ref(t: &str) -> bool {
    let inner = match t.strip_prefix('$') {
        Some(rest) => rest,
        None => return false,
    };
    if inner.is_empty() {
        return false;
    }
    if let Some(braced) = inner.strip_prefix('{') {
        let Some(name) = braced.strip_suffix('}') else {
            return false;
        };
        return !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
    }
    inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || matches!(inner, "?" | "#" | "$" | "@" | "*")
}

/// Double-quote a shell word, preserving `${...}`, `$name`, `$((…))` and
/// `$(...)` for bash expansion inside a `bash -c` fallback.
fn quote_sh_dq(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' => out.push('$'),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Temporary diagnostic: a compact label for an IrExpr that the shell-text
/// reconstruction refused, so a refusal names the NODE and not just the
/// enclosing construct.
fn debug_expr(e: &IrExpr) -> String {
    let d = format!("{e:?}");
    d.chars().take(400).collect()
}

/// Temporary diagnostic: the same for a statement the reconstruction refused.
fn debug_stmt(s: &IrStmt) -> String {
    let d = format!("{s:?}");
    d.chars().take(400).collect()
}

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
            IrStmt::Redirect { inner, redirects } => {
                // An unquoted heredoc interpolates at render time through
                // the vars map (`expand`), so its reads need write-syncs —
                // and the wrapped statements need scanning too.
                if redirects
                    .iter()
                    .any(|rd| rd.mode == "heredoc" && rd.interpolate)
                {
                    r.need_vars = true;
                }
                scan_flags(inner, r);
            }
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
            // A `test` condition can be a single shell-text string the
            // renderer parses at render time (`[ ${x% *} -gt … ]`); any
            // `${…}` inside reads through the vars map, so flag it here —
            // the structured scan above cannot see inside the string.
            if func == "test" {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if s.contains("${") {
                        r.need_vars = true;
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
        // A block-shaped expression (`$( … )` is `Capture{Arrow(stmts)}`, a
        // closure is `Lambda{body}`): the scan MUST descend into the nested
        // statements, or a `param` read inside the block leaves need_vars
        // false and an EARLIER plain assignment skips its `vars[…]` sync —
        // `p=/home/user/script; echo "${p#/}"` then reads pExp → pVal →
        // vars["p"] and finds nothing (parse-heredoc-dollar-paren).
        IrExpr::Arrow(stmts) => scan_flags(stmts, r),
        IrExpr::Lambda { body, .. } => scan_flags(body, r),
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
        [_, IrExpr::Array(items), ..] => items.iter().collect(),
        [..] => args.iter().collect(),
    }
}

impl Render {
    /// The RHS Go expression for `arr=(…)` / `arr+=(…)` (and the declare
    /// forms): `[]any{…}` normally, or `append(…)` for `+=`. Elements that
    /// are `captureWords` / `split` expand to MULTIPLE items (bash word
    /// splitting) — they cannot sit in the literal (which would collapse
    /// them into one joined string, or a single empty string for an empty
    /// capture), so any splice switches the whole RHS to a builder IIFE
    /// that appends plain items and `appendFields` splices in order.
    fn setarray_rhs(&mut self, var_go: &str, is_append: bool, args: &[IrExpr]) -> String {
        enum Elem {
            Plain(String),
            Splice(String),
        }
        let mut elems = Vec::new();
        let mut spliced = false;
        for e in setarray_elems(args) {
            if let IrExpr::Call { func, args: ea } = e {
                if func == "captureWords" {
                    if let Some(IrExpr::Arrow(body)) = ea.first() {
                        let cap = self.capture_arrow(body);
                        elems.push(Elem::Splice(format!("strings.Fields({cap})")));
                        spliced = true;
                        continue;
                    }
                }
                if func == "split" {
                    if let Some(x) = ea.first() {
                        let v = self.expr_str(x);
                        elems.push(Elem::Splice(format!("strings.Fields({v})")));
                        spliced = true;
                        continue;
                    }
                }
                // `local -a files=("$@")`: the whole positional list.
                if func == "listVar" {
                    if let Some(IrExpr::Str(nm, _)) = ea.first() {
                        if nm == "@" || nm == "*" {
                            self.need_fargs = true;
                            elems.push(Elem::Splice("argsList()".to_string()));
                            spliced = true;
                            continue;
                        }
                    }
                }
            }
            elems.push(Elem::Plain(self.expr_any(e)));
        }
        if !spliced {
            let plain: Vec<String> = elems
                .into_iter()
                .map(|e| match e {
                    Elem::Plain(s) => s,
                    Elem::Splice(_) => unreachable!(),
                })
                .collect();
            if is_append {
                return format!("append({var_go}, {})", plain.join(", "));
            }
            return format!("[]any{{{}}}", plain.join(", "));
        }
        let id = self.new_tmp();
        let mut lines = if is_append {
            format!("__sa{id} := {var_go};")
        } else {
            format!("__sa{id} := []any{{}};")
        };
        for e in elems {
            match e {
                Elem::Plain(s) => {
                    lines.push_str(&format!("__sa{id} = append(__sa{id}, {s});"));
                }
                Elem::Splice(s) => {
                    lines.push_str(&format!("__sa{id} = appendStrings(__sa{id}, {s});"));
                }
            }
        }
        lines.push_str(&format!("return __sa{id};"));
        format!("func() []any {{ {lines} }}()")
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

/// A bare `${var}` reference as shell text (no quoting) — the pieces a
/// redirect-target interpolation is built from.
fn redirect_ref_word(t: &IrExpr) -> Option<String> {
    match t {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => Some(format!("${{{n}}}")),
        IrExpr::Call { func, args } if func == "getVar" => match args.first() {
            Some(IrExpr::Str(n, _)) => Some(format!("${{{n}}}")),
            _ => None,
        },
        _ => None,
    }
}

/// One redirect as native Go stdio wiring instead of shell text.
enum NativeRedir {
    /// `> f` / `>> f` / `< f` welded onto stdio fd 0/1/2.
    File {
        fd: i64,
        path: String,
        read: bool,
        append: bool,
    },
    /// `<<< word` / `<<EOF … EOF` — stdin fed from a string. The Go
    /// expression evaluates the word exactly once, like the shell.
    StdinStr(String),
    /// `2>&1` — stderr duplicated from the already-attached stdout.
    DupStdoutToStderr,
}

/// One redirect spec's operator fragment — the mode→operator table is the
/// core's canonical one (`append_redirect_frag`, ir.rs), so a mode the
/// renderer has not seen REFUSES rather than emitting a wrong operator.
/// `fd` is the already-rendered file descriptor ("" means the fd-1 default
/// form). `word` is the already-rendered target.
fn redirect_op_frag(fd: &str, mode: &str, word: &str) -> Option<String> {
    // `wc` is `>|` — the POSIX noclobber-bypassing truncate. Rendering it
    // as `>` silently changed `: >| "$f"` into a plain overwrite.
    let op = match mode {
        "w" => ">",
        "wc" => ">|",
        "a" => ">>",
        "r" => "<",
        "r+" => "<> ",
        "herestring" => "<<<",
        // Heredocs split: the operator stays inline (`cmd 0<<'EOF'`) but
        // the BODY must follow the COMPLETE command (`cmd …\nbody\nEOF`) —
        // an inline body breaks as soon as anything (a subshell close, a
        // pipe) follows it. `heredoc_body_text` supplies the trailing part;
        // every site that assembles redirect text must append it at the end.
        "heredoc" => return Some(format!("{fd}<<'EOF'")),
        "heredoc-tabs" => return Some(format!("{fd}<<-'EOF'")),
        _ => return None,
    };
    Some(format!("{fd}{op}{word}"))
}

/// The trailing body of a heredoc redirect (`\nbody\nEOF`), or `None` for
/// non-heredoc modes. Appended AFTER the complete reconstructed command.
///
/// Split a trailing heredoc body off a reconstructed STAGE text: a stage
/// like `( cat ) 0<<'EOF'\nbody\nEOF` carries its body inline, which breaks
/// as soon as the pipeline joins it with ` | …`. Returns (head, bodies);
/// the caller re-appends bodies after the full join.
fn split_heredoc_tail(text: &str) -> (String, String) {
    // Bodies already split out by the Redirect builders carry a NUL
    // marker: `<head>\0<body>`.
    if let Some(i) = text.find('\u{0}') {
        let head = text[..i].to_string();
        let bodies = text[i + 1..].to_string();
        return (head, bodies);
    }
    // Bodies already split out by the Redirect text builder carry a NUL
    // marker: `<head>\0<body>`.
    // Bodies end the stage text (`\n<body>\nEOF`, before the closing
    // paren(s) a subshell wrapper added). Walk back over `<<'EOF'`-opened
    // blocks; a body never contains a line that is exactly `EOF`.
    let mut head = text.to_string();
    let mut bodies = String::new();
    loop {
        // Trailing `)`s belong to the wrapper, not to the body: strip them
        // for the body search and re-append after the head.
        let trimmed = head.trim_end_matches(')').trim_end();
        let closers = &head[trimmed.len()..];
        let eof = match trimmed.rfind("\nEOF") {
            Some(e) => e,
            None => break,
        };
        if !trimmed[eof + 4..].is_empty() {
            break;
        }
        let mut found: Option<(usize, &str)> = None;
        for op in ["<<-'EOF'", "<<'EOF'"] {
            let pos = trimmed[..eof].rfind(op);
            let nl = pos.map(|p| trimmed[p..].find('\n'));
            if let Some(pos) = pos {
                if trimmed[pos..].find('\n').is_some() {
                    found = Some((pos, op));
                    break;
                }
            }
        }
        let Some((op_pos, op)) = found else {
            break;
        };
        let body = trimmed[op_pos + op.len()..eof + 4].to_string();
        let mut new_head = trimmed[..op_pos + op.len()].to_string();
        new_head.push_str(closers);
        head = new_head;
        bodies = body + &bodies;
        if !head.contains("<<'EOF'") && !head.contains("<<-'EOF'") {
            break;
        }
    }
    (head, bodies)
}

/// The trailing body of a heredoc redirect (`\nbody\nEOF`), or `None` for
/// non-heredoc modes. Appended AFTER the complete reconstructed command.
/// The trailing body of a `{fd, mode, target}` redirect OBJECT (the
/// `redirect`/`builtin` call spelling of [`heredoc_body_text`]).
fn heredoc_body_spec_text(r: &IrExpr) -> Option<String> {
    let IrExpr::Object(props) = r else {
        return None;
    };
    let mode = props.iter().find(|(k, _)| k == "mode").map(|(_, v)| v);
    let target = props.iter().find(|(k, _)| k == "target").map(|(_, v)| v);
    if !matches!(mode, Some(IrExpr::Str(m, _)) if m == "heredoc" || m == "heredoc-tabs") {
        return None;
    }
    match target {
        Some(IrExpr::Str(s, _)) => Some(format!("\n{s}\nEOF")),
        _ => None,
    }
}

fn heredoc_body_text(r: &crate::ir::IrRedirect) -> Option<String> {
    if r.mode != "heredoc" && r.mode != "heredoc-tabs" {
        return None;
    }
    let word = match &r.target {
        crate::ir::IrExpr::Str(s, _) => s.clone(),
        _ => return None,
    };
    Some(format!("\n{word}\nEOF"))
}

/// True when a `{fd, mode, target}` redirect object is exactly
/// `2>/dev/null` (or `2>>/dev/null`): discarding stderr, which is what the
/// native `runPipe`/`capPipe`/`runCmd`/`capCmd` helpers do with every child
/// anyway — so the redirect is a no-op under them and unwraps (see
/// `body_single_exec_opt`). Anything else (stdout/file/stdin redirects,
/// fd-dups, a missing fd which defaults to stdout) is `false` and keeps the
/// `bash -c` fallback. The target must be the literal `/dev/null`: a
/// dynamic word could name any file.
fn null_stderr_obj(r: &crate::ir::IrExpr) -> bool {
    let crate::ir::IrExpr::Object(props) = r else {
        return false;
    };
    let (mut fd_ok, mut mode_ok, mut target_ok) = (false, false, false);
    for (k, v) in props {
        match k.as_str() {
            "fd" => {
                if let crate::ir::IrExpr::Int(2) = v {
                    fd_ok = true;
                }
            }
            "mode" => {
                if let crate::ir::IrExpr::Str(s, _) = v {
                    if s == "w" || s == "a" {
                        mode_ok = true;
                    }
                }
            }
            "target" => {
                if let crate::ir::IrExpr::Str(s, _) = v {
                    if s == "/dev/null" {
                        target_ok = true;
                    }
                }
            }
            _ => {}
        }
    }
    fd_ok && mode_ok && target_ok
}

/// [`null_stderr_obj`] for the core's `IrRedirect` statement spec.
fn null_stderr_spec(r: &crate::ir::IrRedirect) -> bool {
    if r.fd != Some(2) {
        return false;
    }
    if r.mode != "w" && r.mode != "a" {
        return false;
    }
    matches!(&r.target, crate::ir::IrExpr::Str(s, _) if s == "/dev/null")
}

/// True when a RESOLVED redirect discards without wiring: exactly
/// `2>/dev/null` (or `2>>/dev/null`). A middle pipeline stage carrying only
/// these keeps the native chain (the helpers discard every stage's stderr
/// anyway); anything else placed mid-chain refuses.
fn is_discard_redir(nr: &NativeRedir) -> bool {
    matches!(
        nr,
        NativeRedir::File {
            fd: 2,
            path,
            read: false,
            ..
        } if path == "/dev/null"
    )
}

/// The `exec` Call-args shape for a lone command statement: the `exec`
/// call's own args, or the synthesized equivalent for the core's bare
/// `Exec` node. Unwraps one `2>/dev/null`-only redirect layer (see
/// `body_single_exec_opt`). `None` for anything else.
fn lone_exec_args(body: &[crate::ir::IrStmt]) -> Option<Vec<crate::ir::IrExpr>> {
    let [s] = body else {
        return None;
    };
    lone_exec_stmt(s)
}

/// `cat` with no FILE operand (or a lone `-`): it copies stdin to stdout
/// unchanged, so when the caller already knows the stdin text the command
/// IS that text — no child process is needed to prove it.
///
/// Anything else refuses: a file operand reads the filesystem, and an option
/// (`-A`/`-n`/`-v`) transforms the bytes on the way through.
fn lone_cat_stdin(body: &[crate::ir::IrStmt]) -> bool {
    let Some(args) = lone_exec_args(body) else {
        return false;
    };
    let Some(crate::ir::IrExpr::Str(cmd, _)) = args.first() else {
        return false;
    };
    if cmd != "cat" {
        return false;
    }
    match args.get(1) {
        None => true,
        Some(crate::ir::IrExpr::Array(items)) => {
            items.is_empty()
                || (items.len() == 1
                    && matches!(&items[0], crate::ir::IrExpr::Str(s, _) if s == "-"))
        }
        _ => false,
    }
}

/// The `exec` Call-args shape for a lone command STATEMENT (see
/// `lone_exec_args`).
fn lone_exec_stmt(s: &crate::ir::IrStmt) -> Option<Vec<crate::ir::IrExpr>> {
    match s {
        crate::ir::IrStmt::Expr(crate::ir::IrExpr::Call { func, args })
            if func == "exec" || func == "builtin" =>
        {
            match args.first() {
                // A plain command call: the command word plus argv.
                Some(crate::ir::IrExpr::Str(..)) | Some(crate::ir::IrExpr::Ident(..)) => {
                    Some(args.clone())
                }
                // The frontend's redirect spelling: a `builtin` call of an
                // Arrow body with a redirect list (`( cmd ) 2>/dev/null`
                // as a stage arrives as `builtin(Arrow[cmd],
                // [2>/dev/null])`). Same no-op unwrap as the `redirect`
                // form below — but only when every redirect is a stderr
                // discard; anything else keeps the `bash -c` fallback.
                Some(crate::ir::IrExpr::Arrow(inner)) if func == "builtin" => {
                    let Some(crate::ir::IrExpr::Array(redirs)) = args.get(1) else {
                        return None;
                    };
                    if !redirs.iter().all(null_stderr_obj) {
                        return None;
                    }
                    lone_exec_args(inner)
                }
                _ => None,
            }
        }
        crate::ir::IrStmt::Exec {
            cmd,
            args,
            capture: None,
            redirects,
            env,
        } if redirects.is_empty() && env.is_empty() => {
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(cmd.clone());
            call_args.push(crate::ir::IrExpr::Array(args.clone()));
            Some(call_args)
        }
        crate::ir::IrStmt::Redirect { inner, redirects }
            if redirects.iter().all(null_stderr_spec) =>
        {
            lone_exec_args(inner)
        }
        crate::ir::IrStmt::Expr(crate::ir::IrExpr::Call { func, args })
            if func == "redirect" =>
        {
            let (Some(crate::ir::IrExpr::Arrow(inner)), Some(crate::ir::IrExpr::Array(redirs))) =
                (args.first(), args.get(1))
            else {
                return None;
            };
            if !redirs.iter().all(null_stderr_obj) {
                return None;
            }
            lone_exec_args(inner)
        }
        _ => None,
    }
}

/// Redirect stmt → shell text (`2>/dev/null`, `<file`, `>>file`).
/// The pre-rendered-word form of [`Render::redirect_spec_text`], for the
/// targets whose word is expressible without the renderer (`> "$f"`, a
/// synthetic `__ps_tmpN` var).
fn redirect_stmt_text(r: &crate::ir::IrRedirect) -> Option<String> {
    let fd = r.fd.unwrap_or(0).to_string();
    let t = match &r.target {
        // a synthetic temp-var target (the process-substitution lowering's
        // `cmd > $__ps_tmpN`) or a variable redirect target
        // (`echo hi > "$f"` — bat-sh-go t36_redirect_var): render `$name` —
        // redirRun's bash child sees the var's value through env_lit (every
        // written var ships as `name=<value>` env).
        IrExpr::Str(s, _) => s.clone(),
        other => match redirect_ref_word(other) {
            Some(w) => w,
            None => return None,
        },
    };
    redirect_op_frag(&fd, &r.mode, &t)
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
            ("sync.", "sync"),
            ("io.", "io"),
            ("bufio.", "bufio"),
            ("regexp.", "regexp"),
            ("sort.", "sort"),
            ("path.", "path"),
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
                if let Some(c) = capture {
                    out.insert(c.clone());
                }
                collect_written_expr(cmd, out, arrays);
                for a in args {
                    collect_written_expr(a, out, arrays);
                }
                for (_, v) in env {
                    collect_written_expr(v, out, arrays);
                }
                for r in redirects {
                    collect_written_expr(r, out, arrays);
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
/// True when a declaration in the text asks for a NAMEREF (`declare -n`,
/// `local -n`, `typeset -n`): after it every read of the name goes through
/// the name it holds, so the text's spelling no longer says what is read.
/// Erring towards `true` only keeps the complete env (see `sh_text_refs`).
fn nameref_flag(text: &str) -> bool {
    let declares = ["declare", "local", "typeset"]
        .iter()
        .any(|w| go_contains_word(text, w));
    declares && go_contains_word(text, "-n")
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
/// The shell variable names a RECONSTRUCTED command text reads, or `None`
/// when the text can read one this scan cannot name.
///
/// Used to shrink the `name=<value>` env list a `bash -c` child is handed:
/// the child only ever sees a variable the text actually mentions, so
/// shipping the whole written namespace is pure marshalling. `None` is the
/// REFUSE > GUESS arm — it keeps the complete list — and is reserved for the
/// forms whose reads are NOT recoverable from the text:
///
///   * `${!ref}`      — an indirect expansion names its target at run time.
///   * `eval`/`source`/`. f` — the code that runs is not in the text.
///   * `trap`         — the saved code runs later, outside this text.
///   * `declare -n` / `local -n` / `typeset -n` — a nameref turns every
///     later read into an indirect one.
///
/// The forms that DO spell their reads in some other syntax — `$((…))` and
/// `$[…]` arithmetic (bare names), `${#name}`/`$#` (`#` is not an identifier
/// char), and the declarations/`set`/`shift`/`read` family — take the COARSE
/// scan instead: every identifier in the text is returned, which can only
/// over-approximate (plus `IFS` for the splitting `read` family).
fn sh_text_refs(text: &str) -> Option<BTreeSet<String>> {
    // The words that make the text run code it does not spell, or resolve a
    // name at run time. Only these keep the complete env — for them a subset
    // is not provable. Everything else is handled by OVER-APPROXIMATION
    // below, which is sound by construction: keeping a `name=<value>` the
    // child never reads costs marshalling, while dropping one it does read
    // would change behaviour. The old list refused ~55 `s2s` coercions worth
    // of exports for forms whose reads ARE recoverable.
    const DELEGATING_WORDS: &[&str] = &[
        "eval", "source",
        // `trap` SAVES code that runs later (a signal, or the EXIT trap):
        // its reads are not in this text at all.
        "trap",
    ];
    // A NAMEREF makes every later read of the name indirect (`local -n r=x;
    // echo "$r"` reads `x`), so no spelling in the text proves what it reads.
    if text.contains("${!") || nameref_flag(text) {
        return None;
    }
    for w in DELEGATING_WORDS {
        if go_contains_word(text, w) {
            return None;
        }
    }
    // `. f` (dot-sources a file): a `.` in command position — at the text
    // start or after a separator — followed by whitespace, `/`, another
    // separator, or the end. `a.b`, `..`, and `.foo` never match (the char
    // after must not be `.` or an identifier char); `./x` as an ARGUMENT
    // over-approximates to `None`, which only keeps the full env.
    {
        let b = text.as_bytes();
        let is_sep = |c: u8| matches!(c, b';' | b'&' | b'|' | b'(' | b')') || c.is_ascii_whitespace();
        for (i, &c) in b.iter().enumerate() {
            if c != b'.' {
                continue;
            }
            if i > 0 && !is_sep(b[i - 1]) {
                continue;
            }
            match b.get(i + 1) {
                None => return None,
                Some(&n) if n == b'/' || (is_sep(n) && n != b'.') => return None,
                _ => {}
            }
        }
    }
    let b = text.as_bytes();
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out: BTreeSet<String> = BTreeSet::new();
    // COARSE forms: the text reads names the `$name` scan cannot see, but
    // every one of them IS spelled somewhere in the text — so the scan
    // returns every identifier instead, which can only over-approximate.
    //
    //   `$((…))` / `$[…]`  arithmetic resolves BARE names.
    //   `${#name}` / `$#`  `#` is not part of an identifier, so the name
    //                      inside a length expansion would be missed.
    //   `declare -i x=a+2` / `local x=$((…))`-style declarations and
    //   `set`'s positional forms: the reads ride a syntax, not a `$name`.
    //   `read`/`mapfile`/`readarray`/`getopts` assign their own names
    //                      (nothing to prove) — but they split on IFS,
    //                      which the text need not spell, so it is added.
    const COARSE_WORDS: &[&str] = &[
        "declare", "typeset", "local", "export", "readonly", "unset", "set", "read", "mapfile",
        "readarray", "getopts", "shift", "nameref", "IFS",
    ];
    let coarse = text.contains("$((")
        || text.contains("$[")
        || text.contains("${#")
        || text.contains("$#")
        || COARSE_WORDS.iter().any(|w| go_contains_word(text, w));
    if coarse {
        let mut i = 0;
        while i < b.len() {
            if is_id(b[i]) && (i == 0 || !is_id(b[i - 1])) {
                let mut j = i;
                while j < b.len() && is_id(b[j]) {
                    j += 1;
                }
                if !b[i].is_ascii_digit() {
                    out.insert(String::from_utf8_lossy(&b[i..j]).into_owned());
                }
                i = j;
                continue;
            }
            i += 1;
        }
        // The splitting `read`-family consults the child's IFS, which the
        // text may never name (it is usually inherited from the parent).
        if ["read", "mapfile", "readarray"]
            .iter()
            .any(|w| go_contains_word(text, w))
        {
            out.insert("IFS".to_string());
        }
        return Some(out);
    }
    let mut i = 0;
    while i < b.len() {
        // `$name` / `${name…}` — an expansion. Also accept a name only when
        // it starts with an identifier char, so `$$`/`$@`/`$1` add nothing
        // (none of them name a shell variable).
        if b[i] == b'$' && i + 1 < b.len() {
            let mut j = i + 1;
            if b[j] == b'{' {
                j += 1;
            }
            let start = j;
            while j < b.len() && is_id(b[j]) {
                j += 1;
            }
            if j > start && !b[start].is_ascii_digit() {
                out.insert(String::from_utf8_lossy(&b[start..j]).into_owned());
            }
            i = if j > i { j } else { i + 1 };
            continue;
        }
        // `name=` — an ASSIGNMENT (or a `--flag=value` operand, which only
        // over-approximates). A child that assigns a var needs it in scope
        // for the rest of its own text, so the entry must survive.
        if is_id(b[i]) && (i == 0 || !is_id(b[i - 1])) {
            let mut j = i;
            while j < b.len() && is_id(b[j]) {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' && !b[i].is_ascii_digit() {
                out.insert(String::from_utf8_lossy(&b[i..j]).into_owned());
            }
            i = j;
            continue;
        }
        i += 1;
    }
    Some(out)
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
    // A helper's source is the line that DEFINES it plus everything up to
    // the next definition — which means the next helper's doc comment and
    // the blank spacer currently sit at the END of this segment. They
    // belong to the next one: emitting them here dangles a comment when
    // the next helper is dropped, and (worse) the doc prose is then part
    // of THIS helper's dependency text — `globArgs` documents that bash
    // can "expand" a word, which kept `expand` → `pExp` → the whole
    // parameter library in every render that merely touched a glob.
    for i in 0..segs.len().saturating_sub(1) {
        let mut moved: Vec<String> = Vec::new();
        while let Some(last) = segs[i].1.last() {
            if last.trim().is_empty() || last.trim_start().starts_with("//") {
                moved.push(segs[i].1.pop().unwrap());
            } else {
                break;
            }
        }
        moved.reverse();
        for l in moved.into_iter().rev() {
            segs[i + 1].1.insert(0, l);
        }
    }
    segs
}
fn go_used_helpers(body: &str) -> Vec<String> {
    if std::env::var_os("SH2_HELPERS_DUMP").is_some() {
        let segs = go_split_helpers();
        for (n, l) in &segs {
            if n == "globArgs" {
                for x in l { eprintln!("SEGL {}", x); }
            }
        }
    }
    let segs = go_split_helpers();
    // Comments are prose, not code: a helper name mentioned in one is not
    // a call. Scanning them made the dependency graph depend on wording
    // (see the note in go_split_helpers).
    let code_only = |lines: &[String]| -> String {
        lines
            .iter()
            .filter(|l| !l.trim_start().starts_with("//"))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    };
    let body_code: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for (name, _) in &segs {
        if !name.is_empty() && go_contains_word(&body_code, name) { needed.insert(name.clone()); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (name, lines) in &segs {
            if name.is_empty() || !needed.contains(name) { continue; }
            let text = code_only(lines);
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
    "    r := int64(1)",
    "    for i := int64(0); i < b; i++ { r *= a }",
    "    return r",
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
    "func assocIdx(m map[string]any, k string) any {",
    "    if m == nil { return nil }",
    "    return m[k]",
    "}",
    "",
    "func appendStrings(m []any, ss []string) []any {",
    "    for _, s := range ss { m = append(m, s) }",
    "    return m",
    "}",
    "",
    "func splitLines(text, delim string, chomp bool) []any {",
    "    if delim == \"\" { delim = \"\\n\" }",
    "    if text == \"\" { return []any{} }",
    "    parts := strings.Split(text, delim)",
    "    if strings.HasSuffix(text, delim) { parts = parts[:len(parts)-1] }",
    "    out := make([]any, 0, len(parts))",
    "    for _, p := range parts {",
    "        if chomp { out = append(out, p) } else { out = append(out, p+delim) }",
    "    }",
    "    return out",
    "}",
    "",
    "func ifsJoin(out []string, isStar bool) string {",
    "    if !isStar { return strings.Join(out, \" \") }",
    "    // STAR joins on the first char of IFS (empty IFS joins empty).",
    "    if ifs, ok := vars[\"IFS\"].(string); ok {",
    "        if ifs == \"\" { return strings.Join(out, \"\") }",
    "        return strings.Join(out, string([]rune(ifs)[0]))",
    "    }",
    "    return strings.Join(out, \" \")",
    "}",
    "",
    // Keys come back SORTED: bash hands out hash order, which no translator
    // can reproduce, and every corpus consumer of `${!map[@]}` pipes the
    // loop through `sort` -- hash order would only add a second ordering
    // step on top of the sorted one.
    "func arrKeys(name string) []string {",
    "    if m, ok := pValRaw(name).(map[string]any); ok {",
    "        out := make([]string, 0, len(m))",
    "        for k := range m { out = append(out, k) }",
    "        sort.Strings(out)",
    "        return out",
    "    }",
    "    if a, ok := pValRaw(name).([]any); ok {",
    "        out := make([]string, 0, len(a))",
    "        for i := range a { out = append(out, strconv.FormatInt(int64(i), 10)) }",
    "        return out",
    "    }",
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
    "var fArgs []string",
    "",
    "var __wgd *sync.WaitGroup",
    "",
    "var __wgds []*sync.WaitGroup",
    "",
    "var shArgOff int",
    "",
    "func doShiftF(n int) int64 {",
    "    if n < 0 || n > len(fArgs) { return 1 }",
    "    fArgs = fArgs[n:]",
    "    return 0",
    "}",
    "",
    "func doShiftS(n int) int64 {",
    "    if n < 0 || shArgOff+n > len(os.Args)-1 { return 1 }",
    "    shArgOff += n",
    "    return 0",
    "}",
    "",
    "func argsList() []string {",
    "    if len(fArgs) > 0 { return fArgs }",
    "    if len(os.Args) > 1+shArgOff { return os.Args[1+shArgOff:] }",
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
    "    if i == 0 {",
    "        if len(os.Args) > 0 { return os.Args[0] }",
    "        return \"\"",
    "    }",
    "    a := argsList()",
    "    if i >= 1 && i <= len(a) { return a[i-1] }",
    "    return \"\"",
    "}",
    "",
    // A PRE-WIRED stdio is left alone: the native redirect lowering welds
    // c.Stdout/c.Stderr to the redirect targets (os.OpenFile, a dup) before
    // calling this, and an unconditional assignment here would silently
    // discard that wiring and send the command's output to the terminal.
    // The defaults (inherit stdout, drop stderr) apply only where the
    // caller left the field unset — the historical behaviour.
    "func runCmd(c *exec.Cmd) int64 {",
    "    shStdIn(c)",
    "    shStdErr(c)",
    "    if c.Stdout == nil { c.Stdout = os.Stdout }",
    "    if err := c.Run(); err != nil {",
    "        if ee, ok := err.(*exec.ExitError); ok { return int64(ee.ExitCode()) }",
    "        // No ExitError: the program never started (not found,",
    "        // permission denied) — bash reports 127 for those.",
    "        return 127",
    "    }",
    "    return 0",
    "}",
    "",
    "func capCmd(c *exec.Cmd) string {",
    "    shStdIn(c)",
    "    shStdErr(c)",
    "    out, err := c.Output()",
    "    // bash keeps what the command WROTE even when it exits nonzero",
    "    // (`$(grep -c pat f1 f2)` prints the counts and takes status 1), so",
    "    // Output()'s partial output is the value; a command that never",
    "    // started simply has none.",
    "    if err != nil && len(out) == 0 { return \"\" }",
    "    return strings.TrimRight(string(out), \"\\n\")",
    "}",
    "",
    "func capCmdRaw(c *exec.Cmd) string {",
    "    shStdIn(c)",
    "    shStdErr(c)",
    "    out, err := c.Output()",
    "    if err != nil { return \"\" }",
    "    return string(out)",
    "}",
    "",
    "func capRun(script string, env []string) string {",
    "    c := exec.Command(\"bash\", append([]string{\"-c\", script}, shArgs()...)...)",
    "    shStdIn(c)",
    "    shStdErr(c)",
    "    c.Env = append(os.Environ(), env...)",
    "    out, err := c.Output()",
    "    if err != nil { return \"\" }",
    "    return strings.TrimRight(string(out), \"\\n\")",
    "}",
    "",
    "func redirRun(script string, env []string) int64 {",
    "    c := exec.Command(\"bash\", append([]string{\"-c\", script}, shArgs()...)...)",
    "    shStdIn(c)",
    "    shStdErr(c)",
    "    c.Env = append(os.Environ(), env...)",
    "    return runCmd(c)",
    "}",
    "",
    "// shArgs: the parent positionals as a `bash -c` argv tail (the",
    "// parent's $0, then argsList(): fArgs in a function, script args",
    "// otherwise), so a reconstructed fragment naming `$@` observes the",
    "// same params the parent does.",
    "func shArgs() []string {",
    "    if len(os.Args) > 0 {",
    "        return append([]string{os.Args[0]}, argsList()...)",
    "    }",
    "    return append([]string{\"sh2\"}, argsList()...)",
    "}",
    "",
    "func runPipe(stages [][]string) int64 {",
    "    var cmds []*exec.Cmd",
    "    for _, st := range stages {",
    "        c := exec.Command(st[0], st[1:]...)",
    "        cmds = append(cmds, c)",
    "    }",
    "    if len(cmds) == 0 { return 0 }",
    "    var pipes []io.Closer",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        p, err := cmds[i].StdoutPipe()",
    "        if err != nil { return 1 }",
    "        cmds[i+1].Stdin = p",
    "        pipes = append(pipes, p)",
    "    }",
    "    cmds[len(cmds)-1].Stdout = os.Stdout",
    "    // After the pipe wiring (StdoutPipe refuses an already-set Stdout):",
    "    // unwired Stdin ends follow the current process stdin.",
    "    for _, c := range cmds { if c.Stdin == nil { c.Stdin = os.Stdin } }",
    "    for i, c := range cmds {",
    "        if i < len(cmds)-1 {",
    "            if err := c.Start(); err != nil { return 1 }",
    "        } else if err := c.Run(); err != nil {",
    "            if ee, ok := err.(*exec.ExitError); ok { return int64(ee.ExitCode()) }",
    "            return 1",
    "        }",
    "    }",
    "    // The parent holds a read end of every inter-stage pipe: close",
    "    // the parent copies after Run so an early-exiting downstream",
    "    // stage (head) retires the producers instead of deadlocking them.",
    "    for _, p := range pipes { p.Close() }",
    "    for i := 0; i < len(cmds)-1; i++ { cmds[i].Wait() }",
    "    return 0",
    "}",
    "",
    "func capPipe(stages [][]string) string {",
    "    return strings.TrimRight(capPipeRaw(stages), \"\\n\")",
    "}",
    "",
    "// The capture form of runPipe: same native stage chain, but the LAST",
    "// stage's stdout is read instead of inherited, so `$(cmd | cmd ...)` needs",
    "// no `bash -c` child. Both the read and the Waits must happen before the",
    "// last stage is reaped - a StdoutPipe is not valid after Wait.",
    "func capPipeRaw(stages [][]string) string {",
    "    var cmds []*exec.Cmd",
    "    for _, st := range stages {",
    "        if len(st) == 0 { return \"\" }",
    "        c := exec.Command(st[0], st[1:]...)",
    "        cmds = append(cmds, c)",
    "    }",
    "    if len(cmds) == 0 { return \"\" }",
    "    var pipes []io.Closer",
    "    for i := 0; i < len(cmds)-1; i++ {",
    "        p, err := cmds[i].StdoutPipe()",
    "        if err != nil { return \"\" }",
    "        cmds[i+1].Stdin = p",
    "        pipes = append(pipes, p)",
    "    }",
    "    last, err := cmds[len(cmds)-1].StdoutPipe()",
    "    if err != nil { return \"\" }",
    "    // After the pipe wiring: unwired Stdin ends follow the process stdin.",
    "    for _, c := range cmds { if c.Stdin == nil { c.Stdin = os.Stdin } }",
    "    for _, c := range cmds {",
    "        if err := c.Start(); err != nil {",
    "            for _, d := range cmds { if d.Process != nil { d.Process.Kill() } }",
    "            for _, d := range cmds { d.Wait() }",
    "            return \"\"",
    "        }",
    "    }",
    "    data, err := io.ReadAll(last)",
    "    if err != nil { data = nil }",
    "    // Same parent-holds-the-pipe deadlock as runPipe (see above).",
    "    for _, p := range pipes { p.Close() }",
    "    for _, c := range cmds { c.Wait() }",
    "    return string(data)",
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
    "    if hasExtglob(pattern) {",
    "        re, err := regexp.Compile(\"^(?:\" + globRe(pattern, true) + \")$\")",
    "        if err != nil { return false }",
    "        return re.MatchString(s)",
    "    }",
    "    ok, _ := filepath.Match(pattern, s)",
    "    return ok",
    "}",
    "",
    "func hasExtglob(s string) bool {",
    "    bb := []byte(s)",
    "    for i := 0; i+1 < len(bb); i++ {",
    "        if (bb[i] == '@' || bb[i] == '!' || bb[i] == '?' || bb[i] == '+' || bb[i] == '*') && bb[i+1] == '(' {",
    "            return true",
    "        }",
    "    }",
    "    return false",
    "}",
    "",
    "// fsGlob walks the filesystem like the shell's pathname expansion:",
    "// every component is matched by filepath.Match, a leading dot in a",
    "// name never matches a wildcard, and the result is sorted. Go's",
    "// filepath.Glob cannot express `./*.sh` (its Match REFUSES the `.` and",
    "// `/` components outright), so a recursive walk is the only faithful",
    "// form.",
    "func fsGlob(pat string) []string {",
    "    var out []string",
    "    if pat == \"\" { return out }",
    "    var walk func(dir, rest string)",
    "    walk = func(dir, rest string) {",
    "        for {",
    "            i := strings.IndexByte(rest, '/')",
    "            if i < 0 { break }",
    "            comp := rest[:i]",
    "            rest = rest[i+1:]",
    "            if rest == \"\" { break }",
    "            if !strings.ContainsAny(comp, \"*?[\\\\\") {",
    "                if comp == \".\" { continue }",
    "                if comp == \"..\" {",
    "                    if dir == \".\" { dir = \"..\" } else { dir = filepath.Dir(dir) }",
    "                    continue",
    "                }",
    "                dir = filepath.Join(dir, comp)",
    "                continue",
    "            }",
    "            ents, _ := os.ReadDir(dir)",
    "            for _, e := range ents {",
    "                name := e.Name()",
    "                if strings.HasPrefix(name, \".\") && !strings.HasPrefix(comp, \".\") { continue }",
    "                if ok, _ := path.Match(comp, name); ok {",
    "                    walk(filepath.Join(dir, name), rest)",
    "                }",
    "            }",
    "            return",
    "        }",
    "        if !strings.ContainsAny(rest, \"*?[\\\\\") {",
    "            p := rest",
    "            if dir != \".\" || strings.HasPrefix(pat, \"./\") { p = filepath.Join(dir, rest) }",
    "            if _, err := os.Lstat(p); err == nil { out = append(out, p) }",
    "            return",
    "        }",
    "        entries, err := os.ReadDir(dir)",
    "        if err != nil { return }",
    "        for _, e := range entries {",
    "            name := e.Name()",
    "            if strings.HasPrefix(name, \".\") && !strings.HasPrefix(rest, \".\") { continue }",
    "            if ok, _ := path.Match(rest, name); ok {",
    "                p := filepath.Join(dir, name)",
    "                if dir == \".\" && !strings.HasPrefix(pat, \"./\") { p = name }",
    "                out = append(out, p)",
    "            }",
    "        }",
    "    }",
    "    start := \".\"",
    "    if strings.HasPrefix(pat, \"/\") { start = \"/\"; pat = strings.TrimPrefix(pat, \"/\") }",
    "    walk(start, pat)",
    "    sort.Strings(out)",
    "    return out",
    "}",
    "",
    "// globArgs expands a glob word into argv entries. filepath.Glob is tried",
    "// FIRST: it is Go's own expansion and handles every pattern shape,",
    "// including the dotfile rule (`.*` matches dotfiles, `*` does not).",
    "// fsGlob is the fallback for the shapes filepath refuses outright",
    "// (`.//x`, `dir/../x`).",
    "func globArgs(pat string) []string {",
    "    if m, err := filepath.Glob(pat); err == nil {",
    "        // bash's dotfile rule: a wildcard never matches a LEADING dot",
    "        // unless the pattern's own component starts with one (`.*`,",
    "        // `.sh`). Go's filepath.Match has no such rule, so `*` returns",
    "        // `.sh` where bash's `*` does not -- drop those.",
    "        out := m[:0:0]",
    "        for _, x := range m {",
    "            if globDotOk(pat, x) { out = append(out, x) }",
    "        }",
    "        if len(out) > 0 { return out }",
    "        // a no-match pattern is a LITERAL argument in bash (nullglob off)",
    "        return []string{pat}",
    "    }",
    "    // filepath.Glob refused the pattern outright (a dot path component",
    "    // or a wildcard in a directory position): walk it ourselves.",
    "    // A pattern whose FIRST component begins with `.` (`.*`, `.??*`)",
    "    // matches dotfiles: bash's rule is only that a wildcard never",
    "    // matches a LEADING dot, and here the pattern asked for one. Without",
    "    // this the dotfile filter below would UNDO filepath.Glob's correct",
    "    // answer (measured: `grep -c \".*\"` came back with every file).",
    "    if p := pat; strings.HasPrefix(p, \".\") && strings.ContainsAny(p, \"*?[\") {",
    "        if m := fsGlob(pat); len(m) > 0 { return m }",
    "    }",
    "    // Absolute and `./`-rooted patterns walk the filesystem (bash can",
    "    // expand them; filepath.Glob refuses them outright).",
    "    if strings.HasPrefix(pat, \"./\") || strings.HasPrefix(pat, \"../\") {",
    "        m := fsGlob(pat)",
    "        if len(m) == 0 { return []string{pat} }",
    "        return m",
    "    }",
    "    m, err := filepath.Glob(pat)",
    "    if err != nil || m == nil { return []string{pat} }",
    "    // bash does not let a wildcard match a leading dot: `*` yields no",
    "    // dotfile unless the pattern's own component starts with `.`. Go's",
    "    // filepath.Match HAS no such rule, so `ls * .sh` listed `.sh`",
    "    // twice (once from `*`, once from the literal). Filter it back out;",
    "    // an all-dotfile match leaves the pattern unexpanded, exactly as",
    "    // bash does with nullglob off.",
    "    // A pattern whose FIRST component begins with `.` (`.*`, `.??*`)",
    "    // matches dotfiles: bash's rule is only that a wildcard never",
    "    // matches a LEADING dot, and here the pattern asked for one. Without",
    "    // this the dotfile filter below would UNDO filepath.Glob's correct",
    "    // answer (measured: grep -c on a dotted pattern returned files).",
    "    if p := pat; strings.HasPrefix(p, \".\") && strings.ContainsAny(p, \"*?[\") {",
    "        if m := fsGlob(pat); len(m) > 0 { return m }",
    "    }",
    "",
    "    // A pattern whose LAST component begins with `.` (`.sh`, `.*`)",
    "    // matches dotfiles: bash's dot rule is about a LEADING dot in the",
    "    // name, and the pattern asked for one explicitly. filepath.Glob",
    "    // already returned them, so the dotfile filter must leave them.",
    "    last := pat",
    "    if i := strings.LastIndexByte(pat, '/'); i >= 0 { last = pat[i+1:] }",
    "    // A pattern component that BEGINS with `.` asked for dotfiles (bash",
    "    // only forbids a wildcard matching a leading dot), so filepath.Glob",
    "    // already answered correctly -- the filter below must not undo it.",
    "    if strings.HasPrefix(last, \".\") { return m }",
    "    for _, comp := range strings.Split(pat, \"/\") {",
    "        if strings.HasPrefix(comp, \".\") { return m }",
    "    }",
    "    if !strings.HasPrefix(last, \".\") {",
    "        out := m[:0:0]",
    "        for _, s := range m {",
    "            base := s",
    "            if i := strings.LastIndexByte(s, '/'); i >= 0 { base = s[i+1:] }",
    "            if !strings.HasPrefix(base, \".\") { out = append(out, s) }",
    "        }",
    "        if len(out) == 0 { return []string{pat} }",
    "        return out",
    "    }",
    "    return m",
    "}",
    "",
    "// globDotOk: does `pat` legitimately match the leading-dot path `x`?",
    "// bash expands a wildcard over a name's leading dot only when the",
    "// pattern's corresponding component itself starts with a dot.",
    "func globDotOk(pat, x string) bool {",
    "    pc := strings.Split(pat, \"/\")",
    "    xc := strings.Split(x, \"/\")",
    "    if len(pc) != len(xc) { return true }",
    "    for i := range pc {",
    "        if strings.HasPrefix(xc[i], \".\") && !strings.HasPrefix(pc[i], \".\") { return false }",
    "    }",
    "    return true",
    "}",
    "",
    "func reMatch(pattern, s string) bool {",
    "    ok, _ := regexp.MatchString(pattern, s)",
    "    return ok",
    "}",
    "",
    "func splitTop(s string) []string {",
    "    var out []string",
    "    depth := 0",
    "    start := 0",
    "    for i := 0; i < len(s); i++ {",
    "        if s[i] == '(' { depth++ }",
    "        if s[i] == ')' { depth-- }",
    "        if s[i] == '|' && depth == 0 { out = append(out, s[start:i]); start = i+1 }",
    "    }",
    "    return append(out, s[start:])",
    "}",
    "",
    "func globRe(pat string, greedy bool) string {",
    "    // An extglob head (`@(a|b)`, `*(x)`, `+(y)`, `?(z)`, `!(w)`) compiles",
    "    // to its regex directly: `@` = one of, `*`/`+` = repeated, `?` =",
    "    // optional, `!` = none-of (no lookahead — the caller splits the",
    "    // `!(X)rest` shape out before calling). Alternation depth is",
    "    // tracked so nested parens survive.",
    "    if len(pat) >= 2 && pat[1] == '(' {",
    "        switch pat[0] {",
    "        case '@', '*', '+', '?', '!':",
    "            depth := 0",
    "            end := -1",
    "            for i := 1; i < len(pat); i++ {",
    "                if pat[i] == '(' { depth++ }",
    "                if pat[i] == ')' { depth--; if depth == 0 { end = i; break } }",
    "            }",
    "            if end > 0 {",
    "                inner := pat[2:end]",
    "                rest := pat[end+1:]",
    "                alts := splitTop(inner)",
    "                var bx strings.Builder",
    "                bx.WriteString(\"(?:\")",
    "                for ai, a := range alts {",
    "                    if ai > 0 { bx.WriteString(\"|\") }",
    "                    bx.WriteString(globRe(a, true))",
    "                }",
    "                bx.WriteString(\")\")",
    "                switch pat[0] {",
    "                case '@': {}",
    "                case '*': bx.WriteString(\"*\")",
    "                case '+': bx.WriteString(\"+\")",
    "                case '?': bx.WriteString(\"?\")",
    "                case '!': {",
    "                    return \"(?:(?!\" + bx.String()[3:] + \")\" + globRe(rest, greedy) + \")\"",
    "                }",
    "                }",
    "                bx.WriteString(globRe(rest, greedy))",
    "                return bx.String()",
    "            }",
    "        default: {}",
    "        }",
    "    }",
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
    "    // `%`/`%%` remove a suffix, so the pattern is ANCHORED AT THE END:",
    "    // `${HOME%/}` on /root keeps the whole value (there is no trailing",
    "    // slash). An unanchored `^(.*)pat.*?$` matched the LEADING slash",
    "    // and returned the empty string instead — the same shape broke every",
    "    // `${v%…}`/`${v%%…}` whose pattern also occurs earlier in the value.",
    "    pre := \"(.*)\"",
    "    if !shortest { pre = \"(.*?)\" }",
    "    re := regexp.MustCompile(\"(?s)^\" + pre + globRe(pat, true) + \"$\")",
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
    "// cmpDigits is the offset column width GNU cmp uses for -l: the number of",
    "// decimal digits in the largest offset it can print (the compared length).",
    "func cmpDigits(n int64) int {",
    "    d := 1",
    "    for n >= 10 { n /= 10; d++ }",
    "    return d",
    "}",
    "",
    "// cmpErrno spells a Go filesystem error the way GNU cmp's strerror does",
    "// (capitalised, no path prefix — the caller adds `cmp: PATH: `).",
    "func cmpErrno(err error) string {",
    "    if err == nil { return \"\" }",
    "    s := strings.ToLower(err.Error())",
    "    switch {",
    "    case strings.Contains(s, \"no such file\"): return \"No such file or directory\"",
    "    case strings.Contains(s, \"permission denied\"): return \"Permission denied\"",
    "    case strings.Contains(s, \"is a directory\"): return \"Is a directory\"",
    "    }",
    "    return err.Error()",
    "}",
    "",
    "// cmpChar is cmp -b's rendering of a differing byte: printable ASCII as",
    "// itself, control bytes as ^X, DEL as ^?, high bytes with the M- prefix.",
    "func cmpChar(b byte) string {",
    "    switch {",
    "    case b >= 32 && b < 127: return string(rune(b))",
    "    case b == 127: return \"^?\"",
    "    case b < 32: return \"^\" + string(rune(b+64))",
    "    }",
    "    return fmt.Sprintf(\"M-%s\", cmpChar(b&0x7f))",
    "}",
    "",
    "func cmpSlice(d []byte, skip int64) []byte {",
    "    if skip <= 0 { return d }",
    "    if skip >= int64(len(d)) { return nil }",
    "    return d[skip:]",
    "}",
    "",
    "// cmpFiles is GNU cmp over two files: default `differ: byte N, line M`,",
    "// -s (silent), -l (one `OFFSET OCT1 OCT2` line per difference), -b",
    "// (printable bytes), -n LIMIT and -i SKIP[:SKIP2]. Status is cmp's own:",
    "// 0 equal, 1 differ/EOF, 2 unreadable. EOF diagnostics go to stderr, the",
    "// differ lines to stdout, exactly like the program it replaces.",
    "func cmpFiles(a, b string, silent, list, bytesMode bool, limit, skip1, skip2 int64) int64 {",
    "    d1, e1 := os.ReadFile(a)",
    "    if e1 != nil {",
    "        if !silent { fmt.Fprintf(os.Stderr, \"cmp: %s: %s\\n\", a, cmpErrno(e1)) }",
    "        return 2",
    "    }",
    "    d2, e2 := os.ReadFile(b)",
    "    if e2 != nil {",
    "        if !silent { fmt.Fprintf(os.Stderr, \"cmp: %s: %s\\n\", b, cmpErrno(e2)) }",
    "        return 2",
    "    }",
    "    d1 = cmpSlice(d1, skip1)",
    "    d2 = cmpSlice(d2, skip2)",
    "    n1, n2 := int64(len(d1)), int64(len(d2))",
    "    minlen := n1",
    "    if n2 < n1 { minlen = n2 }",
    "    n := minlen",
    "    if limit >= 0 && limit < n { n = limit }",
    "    width := cmpDigits(n)",
    "    line := int64(1)",
    "    differ := false",
    "    for i := int64(0); i < n; i++ {",
    "        if d1[i] == d2[i] {",
    "            if d1[i] == '\\n' { line++ }",
    "            continue",
    "        }",
    "        if silent { return 1 }",
    "        if list {",
    "            differ = true",
    "            fmt.Printf(\"%*d %3o %3o\\n\", width, i+1, d1[i], d2[i])",
    "            continue",
    "        }",
    "        if bytesMode {",
    "            fmt.Printf(\"%s %s differ: byte %d, line %d is %3o %s %3o %s\\n\", a, b, i+1, line, d1[i], cmpChar(d1[i]), d2[i], cmpChar(d2[i]))",
    "        } else {",
    "            fmt.Printf(\"%s %s differ: byte %d, line %d\\n\", a, b, i+1, line)",
    "        }",
    "        return 1",
    "    }",
    "    if n1 != n2 && n == minlen && (limit < 0 || n < limit) {",
    "        shorter, sl := a, n1",
    "        if n2 < n1 { shorter, sl = b, n2 }",
    "        if silent { return 1 }",
    "        if sl == 0 {",
    "            fmt.Fprintf(os.Stderr, \"cmp: EOF on %s which is empty\\n\", shorter)",
    "        } else if list {",
    "            fmt.Fprintf(os.Stderr, \"cmp: EOF on %s after byte %d\\n\", shorter, sl)",
    "        } else {",
    "            fmt.Fprintf(os.Stderr, \"cmp: EOF on %s after byte %d, in line %d\\n\", shorter, sl, line)",
    "        }",
    "        return 1",
    "    }",
    "    if differ { return 1 }",
    "    return 0",
    "}",
    "",
    "var vars = map[string]any{}",
    "",
    "var origStdin = os.Stdin",
    "",
    "var origStdout = os.Stdout",
    "",
    "var origStderr = os.Stderr",
    "",
    "// shStd wires a child to the CURRENT process stdio: a nil Stdin",
    "    // follows the (possibly swapped) os.Stdin var, and a nil Stderr",
    "    // follows a swapped os.Stderr but stays discarded otherwise",
    "    // (native helpers never write stderr by default). Stdout is left",
    "    // alone: capturers attach their own pipe (pre-setting it breaks",
    "    // Output), and runners default it explicitly. Function bodies run",
    "    // with these vars temporarily swapped for welded redirects, so",
    "    // every spawn site funnels through here instead of inheriting the",
    "    // raw fds (which the swap does not change).",
    "func shStdIn(c *exec.Cmd) {",
    "    if c.Stdin == nil { c.Stdin = os.Stdin }",
    "}",
    "",
    "func shStdErr(c *exec.Cmd) {",
    "    if c.Stderr == nil {",
    "        if os.Stderr != origStderr { c.Stderr = os.Stderr } else { c.Stderr = io.Discard }",
    "    }",
    "}",
    "func isDigits(s string) bool {",
    "    if s == \"\" { return false }",
    "    for i := 0; i < len(s); i++ {",
    "        if s[i] < '0' || s[i] > '9' { return false }",
    "    }",
    "    return true",
    "}",
    "",
    "func atoi(s string) int {",
    "    n, _ := strconv.Atoi(s)",
    "    return n",
    "}",
    "",
    "func pValRaw(name string) any {",
    "    if v, ok := vars[name]; ok { return v }",
    "    return os.Getenv(name)",
    "}",
    "",
    "func pVal(name string) string {",
    "    // A bare positional (`${2:-d}` arrives as name \"2\"): shell vars",
    "    // are never all-digits, so this is always a positional read — and",
    "    // the value lives in fArgs/os.Args, never in the vars map.",
    "    if isDigits(name) { return paramAt(atoi(name)) }",
    "    if strings.HasPrefix(name, \"#\") {",
    "        n := name[1:]",
    "        if strings.HasSuffix(n, \"[@]\") {",
    "            a, _ := pValRaw(n[:len(n)-3]).([]any)",
    "            return strconv.FormatInt(int64(len(a)), 10)",
    "        }",
    "        if isDigits(n) { return strconv.FormatInt(runeLen(paramAt(atoi(n))), 10) }",
    "        if a, ok := pValRaw(n).([]any); ok {",
    "            return strconv.FormatInt(int64(len(a)), 10)",
    "        }",
    "        return strconv.FormatInt(runeLen(s2s(pValRaw(n))), 10)",
    "    }",
    "    if strings.HasSuffix(name, \"[@]\") || strings.HasSuffix(name, \"[*]\") {",
    "        isStar := strings.HasSuffix(name, \"[*]\")",
    "        base := name[:len(name)-3]",
    "        if m, ok := pValRaw(base).(map[string]any); ok {",
    "            keys := make([]string, 0, len(m))",
    "            for k := range m { keys = append(keys, k) }",
    "            sort.Strings(keys)",
    "            out := make([]string, 0, len(m))",
    "            for _, k := range keys { out = append(out, s2s(m[k])) }",
    "            return ifsJoin(out, isStar)",
    "        }",
    "        a, _ := pValRaw(base).([]any)",
    "        out := make([]string, 0, len(a))",
    "        for _, v := range a { out = append(out, s2s(v)) }",
    "        return ifsJoin(out, isStar)",
    "    }",
    "    // The vars map is keyed by FULL name (`vars[\"map[foo]\"]` is what",
    "    // the assoc write stores), so consult it before decomposing — the",
    "    // decomposition below only knows whole-array lookups. A whole",
    "    // array under its bare name reads as element zero (bash), not the",
    "    // joined list — the [@]/[*] forms are the ones that join.",
    "    if v, ok := vars[name]; ok {",
    "        if a, ok := v.([]any); ok { return s2s(arrIdx(a, 0)) }",
    "        return s2s(v)",
    "    }",
    "    if i := strings.Index(name, \"[\"); i > 0 && strings.HasSuffix(name, \"]\") {",
    "        a, _ := pValRaw(name[:i]).([]any)",
    "        k, err := strconv.ParseInt(name[i+1:len(name)-1], 0, 64)",
    "        if err != nil {",
    "            if e := expand(name[i+1 : len(name)-1]); e != name[i+1:len(name)-1] {",
    "                if k2, err2 := strconv.ParseInt(strings.TrimSpace(e), 0, 64); err2 == nil {",
    "                    return s2s(arrIdx(a, k2))",
    "                }",
    "                m, _ := pValRaw(name[:i]).(map[string]any)",
    "                return s2s(m[strings.TrimSpace(e)])",
    "            }",
    "            m, _ := pValRaw(name[:i]).(map[string]any)",
    "            return s2s(m[name[i+1:len(name)-1]])",
    "        }",
    "        return s2s(arrIdx(a, k))",
    "    }",
    "    return s2s(pValRaw(name))",
    "}",
    "",
    "func pSliceIdx(s string) (int64, bool) {",
    "    if v, err := strconv.ParseInt(strings.TrimSpace(s), 0, 64); err == nil {",
    "        return v, true",
    "    }",
    "    // A dynamic offset: expand it through the live store, then parse.",
    "    if e := strings.TrimSpace(expand(s)); e != strings.TrimSpace(s) {",
    "        if v, err := strconv.ParseInt(e, 0, 64); err == nil {",
    "            return v, true",
    "        }",
    "    }",
    "    return 0, false",
    "}",
    "",
    "func pList(op, name string, args ...string) []string {",
    "    if op == \"@\" || op == \"*\" { return argsList() }",
    "    if strings.HasPrefix(name, \"#\") {",
    "        return []string{pVal(name)}",
    "    }",
    "    if strings.HasSuffix(name, \"[@]\") || strings.HasSuffix(name, \"[*]\") {",
    "        base := name[:len(name)-3]",
    "        a, _ := pValRaw(base).([]any)",
    "        if op == \"slice\" && len(args) >= 1 {",
    "            st, _ := pSliceIdx(args[0])",
    "            e := int64(len(a))",
    "            if len(args) > 1 && strings.TrimSpace(args[1]) != \"\" {",
    "                if ln, ok := pSliceIdx(args[1]); ok { e = st+ln } else { e = st }",
    "            }",
    "            a = arrSlice(a, st, e)",
    "        }",
    "        out := make([]string, 0, len(a))",
    "        for _, v := range a { out = append(out, s2s(v)) }",
    "        return out",
    "    }",
    "    return []string{pExp(op, name, args...)}",
    "}",
    "",
    "func pExp(op, name string, args ...string) string {",
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
    "        // An array value slices by ELEMENT: the bare name arrives for",
    "        // both spellings, so branch on the runtime value.",
    "        if a, ok := pValRaw(name).([]any); ok {",
    "            rest := args",
    "            if len(rest) > 0 && (rest[0] == \"@\" || rest[0] == \"*\") { rest = rest[1:] }",
    "            st, _ := pSliceIdx(\"\")",
    "            if len(rest) > 0 { st, _ = pSliceIdx(rest[0]) }",
    "            e := int64(len(a))",
    "            if len(rest) > 1 && strings.TrimSpace(rest[1]) != \"\" {",
    "                if ln, ok := pSliceIdx(rest[1]); ok { e = st+ln } else { e = st }",
    "            }",
    "            a = arrSlice(a, st, e)",
    "            out := make([]string, 0, len(a))",
    "            for _, v := range a { out = append(out, s2s(v)) }",
    "            return strings.Join(out, \" \")",
    "        }",
    "        st := int64(0)",
    "        if len(args) > 0 { st, _ = pSliceIdx(args[0]) }",
    "        if st < 0 { st = int64(runeLen(v)) + st }",
    "        if st > int64(runeLen(v)) { st = int64(runeLen(v)) }",
    "        if st < 0 { st = 0 }",
    "        rs := []rune(v)",
    "        if len(args) < 2 || strings.TrimSpace(args[1]) == \"\" { return string(rs[st:]) }",
    "        en := st",
    "        if ln, ok := pSliceIdx(args[1]); ok { en = st + ln }",
    "        if en < 0 { en = int64(len(rs)) + en }",
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
    "    // A quoted default arrives with its quotes as data: strip one layer.",
    "    if len(s) >= 2 {",
    "        if (s[0] == '\"' && s[len(s)-1] == '\"') || (s[0] == '\\'' && s[len(s)-1] == '\\'') {",
    "            s = s[1 : len(s)-1]",
    "        }",
    "    }",
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
    "                depth := 1",
    "                for j < len(s) && depth > 0 {",
    "                    if s[j] == '$' && j+1 < len(s) && s[j+1] == '{' { depth++; j += 2; continue }",
    "                    if s[j] == '}' { depth-- };",
    "                    if depth == 0 { break };",
    "                    j++",
    "                }",
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
    "    best := -1",
    "    depth := 0",
    "    i := 0",
    "    for i < len(inner) {",
    "        if inner[i] == '$' && i+1 < len(inner) && inner[i+1] == '{' { depth++; i += 2; continue }",
    "        if inner[i] == '}' {",
    "            if depth > 0 { depth--; i++; continue }",
    "            break",
    "        }",
    "        if depth == 0 && i > 0 {",
    "            for _, op := range []string{\":-\", \":=\", \":?\", \"##\", \"%%\", \"//\", \"^^\", \",,\", \"#\", \"%\", \"/\", \"^\", \",\"} {",
    "                if strings.HasPrefix(inner[i:], op) { best = i; break }",
    "            }",
    "            if best >= 0 { break }",
    "        }",
    "        i++",
    "    }",
    "    // The table is longest-first within each family, so the first",
    "    // depth-zero match wins — except `#`/`%`/`/` also prefix their",
    "    // doubles: prefer the longest op at the winning spot.",
    "    if best >= 0 {",
    "        bestOp := \"\"",
    "        for _, op := range []string{\"##\", \"%%\", \"//\", \"^^\", \",,\", \":-\", \":=\", \":?\", \"#\", \"%\", \"/\", \"^\", \",\"} {",
    "            if strings.HasPrefix(inner[best:], op) { bestOp = op; break }",
    "        }",
    "        return pExp(bestOp, inner[:best], inner[best+len(bestOp):])",
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
        // `$(ls)` reads the directory through the stdlib now — the capture
        // and the statement form share the `ls_emit` template (a second
        // copy would let them disagree about ordering or headers).
        assert!(!out.contains("exec.Command(\"ls\")"), "{out}");
        assert!(out.contains("os.ReadDir("), "{out}");
        assert!(out.contains("fmt.Println(fmt.Sprintf(\"%v\", y))"), "{out}");
    }

    #[test]
    fn go_keyword_mangled() {
        let out = render("type=1\ntype=2\necho \"$type\"\n");
        assert!(out.contains("var type_ int64"), "{out}");
        assert!(!out.contains("var type int64"), "{out}");
    }

    #[test]
    fn sleep_renders_as_time_sleep() {
        // one operand, and GNU's summed form
        let out = render("sleep 1\nsleep 0.1 2s\n");
        assert!(out.contains("time.Sleep(time.Second);"), "{out}");
        assert!(out.contains("time.Sleep(2100 * time.Millisecond);"), "{out}");
        assert!(!out.contains("exec.Command(\"sleep\""), "{out}");
        // a statically-known variable folds into the same native sleep
        let out = render("d=3\nsleep \"$d\"\n");
        assert!(out.contains("time.Sleep(3 * time.Second);"), "{out}");
        // a non-static interval keeps the fork
        let out = render("read d\nsleep \"$d\"\n");
        assert!(out.contains("exec.Command(\"sleep\""), "{out}");
    }

    #[test]
    fn read_into_untyped_var_compiles() {
        // `readInto` takes `...*string`; an unannotated var hoists as `any`,
        // so passing its address produced *any and the render did not build.
        let out = render("read -r line\necho \"got $line\"\n");
        assert!(out.contains("var line any"), "{out}");
        assert!(!out.contains("readInto(&line)"), "{out}");
        assert!(out.contains("line = __rvt"), "{out}");
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

    #[test]
    fn sleep_operand_grammar() {
        assert_eq!(sleep_ns("1"), Some(1_000_000_000));
        assert_eq!(sleep_ns("1s"), Some(1_000_000_000));
        assert_eq!(sleep_ns("2m"), Some(120_000_000_000));
        assert_eq!(sleep_ns("1h"), Some(3_600_000_000_000));
        assert_eq!(sleep_ns("0.1"), Some(100_000_000));
        assert_eq!(sleep_ns("0.01"), Some(10_000_000));
        assert_eq!(sleep_ns("1.5s"), Some(1_500_000_000));
        // GNU accepts `inf`/`nan` and suffixes on the *interval*, but neither
        // has a static Go duration — the fork stays.
        assert_eq!(sleep_ns("inf"), None);
        assert_eq!(sleep_ns("nan"), None);
        assert_eq!(sleep_ns("1x"), None);
        assert_eq!(sleep_ns(""), None);
        assert_eq!(sleep_ns("."), None);
        assert_eq!(sleep_ns("1."), None);
        assert_eq!(sleep_ns(".5"), None);
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

/// The regex body for the `grep -o` lift's `grepMatches(text, pat, flags)`
/// (transforms/grep_o.rs): the pattern is a BRE by default, `-E` keeps it
/// as-is, `-F` makes it a literal, `-i` adds case-insensitivity. Go's
/// regexp package speaks ERE, so the BRE literals (`\+`, `\?`, `\(` …)
/// are unwrapped the same way the python backend does it.
fn grep_re_body(pat: &str, flags: &str) -> String {
    let mut body = pat.to_string();
    if flags.contains('F') {
        let mut lit = String::new();
        for c in body.chars() {
            if matches!(
                c,
                '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\'
            ) {
                lit.push('\\');
            }
            lit.push(c);
        }
        body = lit;
    } else if !flags.contains('E') {
        body = body
            .replace("\\+", "+")
            .replace("\\?", "?")
            .replace("\\(", "(")
            .replace("\\)", ")")
            .replace("\\|", "|")
            .replace("\\{", "{")
            .replace("\\}", "}");
    }
    if flags.contains('i') {
        format!("(?i){body}")
    } else {
        body
    }
}
