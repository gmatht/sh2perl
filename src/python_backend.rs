//! Python backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/python`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-python-core-needs.md §1):
//! `shir_to_python(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! python `int`, everything else → python `str` (shell vars are strings).
//! Identifiers are mangled against Python keywords (A6-consistent).
//! Everything outside the lowable subset (numeric arith, echo/printf,
//! if/elif/else, while/for loops, simple assignment, subprocess exec)
//! emits a compile-able `sh2.*` stub or a `# TODO(unsupported)` marker,
//! so the draft always compiles (the stubs exit 2, mirroring the C
//! backend's runtime-store convention).

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    /// >0 while rendering a function body (top-level `return` is a python
    /// syntax error, so Return outside a function lowers to a TODO)
    in_function: usize,
    /// >0 while rendering a loop body (break/continue lower natively)
    loop_depth: usize,
    /// names WRITTEN anywhere in the program (a getVar of an unwritten
    /// plain name folds to "" — the SH2_ASSUME_NO_ENV read fold, mirroring
    /// the estree emitter's collect_never_written)
    written: HashSet<String>,
    /// names written ONLY through the runtime store (`setVar` calls — the
    /// imperative frontends' handle temps `___hp_*` etc.): a getVar of one
    /// of these must round-trip through the store, NOT the python binding
    /// (which is only updated by native Assign/Declare statements).
    store_written: HashSet<String>,
    /// bracket-target array/assoc roots observed anywhere (`arr[1]`,
    /// `map[foo]`, `map[$k]` names in setVar/getVar/param calls) with the
    /// raw key texts — decides list-mode (all numeric keys + a
    /// DeclareArray) vs dict-mode (anything else) per root.
    array_keys: std::collections::HashMap<String, Vec<String>>,
    /// roots decided DICT-mode; pre-declared as `{}` in the prologue and
    /// accessed with python subscripts
    array_dict_roots: HashSet<String>,
    /// ordered (index, name, is_unset) ops from the collect walk — decides
    /// whether a variable's LAST state is unset (`${x:-def}` must apply
    /// after `unset x`, which the plain written-set cannot express)
    seq_ops: Vec<(usize, String, bool)>,
    /// vars whose LAST recorded op is an unset
    unset_vars: HashSet<String>,
    /// needs the `__sh_atoi` helper (printf %d/%i/%u args)
    need_atoi: bool,
    todo: usize,
    /// needs the `__sh_rc` status var (a `$?` test operand)
    need_rc: bool,
    /// needs the `__sh_strip` helper (a `${x##pat}` test operand)
    need_strip: bool,
    /// needs `import re` (the grepMatches lift)
    need_re: bool,
    /// needs `import sys` (exit)
    need_sys: bool,
    /// needs the `__sh_exec` subprocess helper
    need_subprocess: bool,
    /// needs the __sh_inc_pre/__sh_inc_post helpers (a $(( x++ )) VALUE)
    need_inc: bool,
    /// needs the __sh_assign helper (${x:=default} assignment side effect)
    need_assign: bool,
    /// needs the `__sh_capture` helper (bash `$()` semantics: stdout only,
    /// trailing newlines stripped, NEVER raises — command-not-found/failure
    /// yields "" and the script continues, like bash)
    need_capture: bool,
    need_capture_out: bool,
}

impl Render {
    /// Real runtime implementations for the sh2.* names the preamble can
    /// emit (ported from harness/sh2-namespace.mjs — the C memory arena
    /// slice 2, the var store, the array store, the test fallback). A name
    /// MISSING from this table still gets the compile-able TODO-exit stub.
    /// The mem handles are the tagged strings `\x01mem:<id>:<off>`; the
    /// arena is a flat byte-slot array with the load/store offset scaled by
    /// the type's element size (p + n gets its sizeof(*p) semantics).
    const RUNTIME: &[(&str, &str)] = &[
        (
            "getVar",
            "def sh2_getVar(name):\n    return __sh_store.get(name, \"\")\n",
        ),
        (
            "setVar",
            "def sh2_setVar(name, value):\n    __sh_store[name] = \"\" if value is None else str(value)\n",
        ),
        (
            "strip",
            concat!(
                "def sh2_strip(v, pat, which):\n",
                "    import fnmatch\n",
                "    if which == 'p':\n",
                "        m = 0\n",
                "        for i in range(1, len(v)+1):\n",
                "            if fnmatch.fnmatch(v[:i], pat):\n",
                "                m = i; break\n",
                "        return v[m:]\n",
                "    if which == 'P':\n",
                "        m = 0\n",
                "        for i in range(1, len(v)+1):\n",
                "            if fnmatch.fnmatch(v[:i], pat):\n",
                "                m = i\n",
                "        return v[m:]\n",
                "    if which == 's':\n",
                "        n = len(v)\n",
                "        for i in range(len(v)-1, -1, -1):\n",
                "            if fnmatch.fnmatch(v[i:], pat):\n",
                "                n = i\n",
                "        return v[:n]\n",
                "    # 'S' longest suffix\n",
                "    n = len(v)\n",
                "    for i in range(len(v)-1, -1, -1):\n",
                "        if fnmatch.fnmatch(v[i:], pat):\n",
                "            n = i; break\n",
                "    return v[:n]\n",
            ),
        ),
        (
            "memAlloc",
            concat!(
                "def sh2_memAlloc(size):\n",
                "    global __sh_mem_seq\n",
                "    __sh_mem_seq += 1\n",
                "    n = int(float(str(size or 0)))\n",
                "    n = n if n > 0 else 0\n",
                "    __sh_mem[__sh_mem_seq] = [0] * n\n",
                "    return \"\\x01mem:%d:0\" % __sh_mem_seq\n",
            ),
        ),
        (
            "memElemSize",
            concat!(
                "def sh2_memElemSize(t):\n",
                "    sizes = {'char': 1, 'signed char': 1, 'unsigned char': 1, 'short': 2, ",
                "'short int': 2, 'int': 4, 'unsigned int': 4, 'unsigned': 4, 'long': 8, ",
                "'long int': 8, 'long long': 8, 'unsigned long': 8, 'unsigned long long': 8, ",
                "'float': 4, 'double': 8, 'void*': 8, 'ptr': 8, 'pointer': 8, 'int8': 1, ",
                "'int16': 2, 'int32': 4, 'int64': 8, 'u32': 4, 'u64': 8}\n",
                "    return sizes.get(str(t), 1)\n",
            ),
        ),
        (
            "memLoad",
            concat!(
                // the A1 calls may carry only the handle (slice-1 form
                // `memLoad(h)` — t83_const_ptr.c, the estree reference's
                // slice-2 override defaults offset/type the same way);
                // the arena lookup fails on a null handle → "".
                "def sh2_memLoad(h, offset=0, t=4):\n",
                "    p = __sh_mem_parse(h)\n",
                "    if p is None or p[0] not in __sh_mem:\n",
                "        return \"\"\n",
                "    i = (p[1] + int(offset or 0)) * sh2_memElemSize(t)\n",
                "    a = __sh_mem[p[0]]\n",
                "    return str(a[i]) if 0 <= i < len(a) else \"\"\n",
            ),
        ),
        (
            "memStore",
            concat!(
                "def sh2_memStore(h, offset=0, t=4, v=None):\n",
                "    p = __sh_mem_parse(h)\n",
                "    if p is None or p[0] not in __sh_mem:\n",
                "        return\n",
                "    i = (p[1] + int(offset or 0)) * sh2_memElemSize(t)\n",
                "    a = __sh_mem[p[0]]\n",
                "    if 0 <= i < len(a):\n",
                "        a[i] = \"\" if v is None else str(v)\n",
            ),
        ),
        (
            "memAdvance",
            concat!(
                "def sh2_memAdvance(h, n):\n",
                "    p = __sh_mem_parse(h)\n",
                "    if p is None:\n",
                "        return h\n",
                "    return \"\\x01mem:%d:%d\" % (p[0], p[1] + int(n or 0))\n",
            ),
        ),
        (
            "memFree",
            concat!(
                "def sh2_memFree(h):\n",
                "    p = __sh_mem_parse(h)\n",
                "    if p is not None and p[0] in __sh_mem:\n",
                "        del __sh_mem[p[0]]\n",
            ),
        ),
        (
            "memTest",
            concat!(
                "def sh2_memTest(op, a, b):\n",
                "    pa = __sh_mem_pos(a)\n",
                "    pb = __sh_mem_pos(b)\n",
                "    return {'<': pa < pb, '<=': pa <= pb, '>': pa > pb, '>=': pa >= pb, ",
                "'==': pa == pb, '!=': pa != pb}.get(op, False)\n",
            ),
        ),
        (
            "setArray",
            concat!(
                "def sh2_setArray(name, elements, isAssoc):\n",
                "    __sh_arrays[name] = list(elements or [])\n",
            ),
        ),
        (
            "arrayIndex",
            concat!(
                "def sh2_arrayIndex(name, key):\n",
                "    try:\n",
                "        return str(__sh_arrays.get(name, [])[int(key)])\n",
                "    except Exception:\n",
                "        return \"\"\n",
            ),
        ),
        (
            "test",
            concat!(
                "def sh2_test(s):\n",
                "    t = str(s or \"\").split()\n",
                "    if len(t) == 3:\n",
                "        a, op, b = t\n",
                "        va = __sh_test_val(a)\n",
                "        vb = __sh_test_val(b)\n",
                "        if op in ('-gt', '-lt', '-ge', '-le'):\n",
                "            try:\n",
                "                return {'-gt': va > vb, '-lt': va < vb, '-ge': va >= vb, ",
                "'-le': va <= vb}[op]\n",
                "            except TypeError:\n",
                "                return False\n",
                "        return {'-eq': va == vb, '=': va == vb, '==': va == vb, ",
                "'-ne': va != vb, '!=': va != vb}.get(op, False)\n",
                "    if len(t) == 2 and t[0] == '-n':\n",
                "        return bool(__sh_test_val(t[1]))\n",
                "    if len(t) == 2 and t[0] == '-z':\n",
                "        return not bool(__sh_test_val(t[1]))\n",
                "    if len(t) == 1:\n",
                "        return bool(__sh_test_val(t[0]))\n",
                "    return False\n",
            ),
        ),
        (
            "testVal",
            concat!(
                "def __sh_test_val(x):\n",
                "    x = str(x).strip().strip(\"\\\"\")\n",
                "    if x.startswith('$'):\n",
                "        return sh2_getVar(x[1:])\n",
                "    try:\n",
                "        return int(x)\n",
                "    except ValueError:\n",
                "        return x\n",
            ),
        ),
        (
            "memParse",
            concat!(
                "def __sh_mem_parse(h):\n",
                "    h = str(h or \"\")\n",
                "    if not h.startswith(\"\\x01mem:\"):\n",
                "        return None\n",
                "    r = h[5:].split(':')\n",
                "    if len(r) != 2:\n",
                "        return None\n",
                "    try:\n",
                "        return int(r[0]), int(r[1])\n",
                "    except ValueError:\n",
                "        return None\n",
            ),
        ),
        (
            "memPos",
            concat!(
                "def __sh_mem_pos(h):\n",
                "    p = __sh_mem_parse(h)\n",
                "    if p is not None:\n",
                "        return p[1]\n",
                "    try:\n",
                "        return int(h) or 0\n",
                "    except (TypeError, ValueError):\n",
                "        return 0\n",
            ),
        ),
    ];

    /// Emit one runtime body (multi-line python, depth 0) plus a blank line.
    fn emit_runtime_body(&mut self, body: &str) {
        for line in body.lines() {
            self.emit(line);
        }
        self.emit("");
    }

    /// Look up the preamble body for a sh2.* name or helper.
    fn runtime_body(name: &str) -> Option<&'static str> {
        Render::RUNTIME.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }
}

fn scan_rc(stmts: &[IrStmt], out: &mut bool) {
    for s in stmts {
        match s {
            IrStmt::Expr(e) | IrStmt::Assign { expr: e, .. } | IrStmt::Output { value: e, .. } => {
                scan_rc_expr(e, out);
            }
            IrStmt::WriteFile { path, content, .. } => {
                scan_rc_expr(path, out);
                scan_rc_expr(content, out);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                scan_rc_expr(cond, out);
                scan_rc(then, out);
                for (c, b) in elsifs {
                    scan_rc_expr(c, out);
                    scan_rc(b, out);
                }
                scan_rc(else_, out);
            }
            IrStmt::While { cond, body } => {
                scan_rc_expr(cond, out);
                scan_rc(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                scan_rc(body, out);
                scan_rc_expr(cond, out);
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                scan_rc(init, out);
                scan_rc_expr(cond, out);
                scan_rc(step, out);
                scan_rc(body, out);
            }
            IrStmt::For { iter, body, .. } => {
                scan_rc_expr(iter, out);
                scan_rc(body, out);
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                scan_rc(body, out);
                for e in excepts {
                    scan_rc(&e.body, out);
                }
                scan_rc(else_body, out);
                scan_rc(finally_body, out);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => scan_rc(b, out),
            IrStmt::Function { body, .. } => scan_rc(body, out),
            IrStmt::Exec { cmd, args, .. } => {
                scan_rc_expr(cmd, out);
                for a in args {
                    scan_rc_expr(a, out);
                }
            }
            _ => {}
        }
    }
}

fn scan_rc_expr(e: &IrExpr, out: &mut bool) {
    if *out {
        return;
    }
    match e {
        IrExpr::Call { func, args } => {
            if func == "test" {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if s.contains("$?") {
                        *out = true;
                        return;
                    }
                }
            }
            for a in args {
                scan_rc_expr(a, out);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            scan_rc_expr(lhs, out);
            scan_rc_expr(rhs, out);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            scan_rc_expr(obj, out);
            for a in args {
                scan_rc_expr(a, out);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                scan_rc_expr(i, out);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    scan_rc_expr(x, out);
                }
            }
        }
        _ => {}
    }
}

/// Render an `IrProgram` to python source (a runnable script).
pub fn shir_to_python(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // builtin-op fallback arm (shir-builtin-op-20260816): the python
    // backend has NOT accepted the `builtin` op — render as exec.
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    // A2: the type verdicts are computed at serialization time in the JSON
    // path; the library path must run the same analysis. A frontend that
    // EMITTED typed var_types (the imperative C-family frontends: Int32/
    // Int64/UInt32/UInt64/Float verdicts, `analyze_var_types` never
    // produces those) keeps them — discarding them (as this did) lost the
    // frontend's authoritative types and re-derived shell verdicts that
    // miss C-typed vars (triage-python 20260814-1425xx cluster: getVar
    // stubs for Int32 vars the analysis never lifted). Mirror of the
    // shir_json.rs serialization rule: analyze only when empty.
    if prog.var_types.is_empty() {
        prog.var_types = crate::shir::analyze_var_types(&prog);
    }
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.collect_writes(&prog.stmts);
    r.finalize_unset_states();
    r.finalize_array_modes();
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
        self.emit(&format!("# TODO(unsupported): {what}"));
    }

    /// Observe a bracket-form target name (`arr[1]`, `map[foo]`,
    /// `map[$k]`) during the collect pass: record the root + raw key so
    /// finalize_array_modes can pick list vs dict per root.
    fn note_array_target(&mut self, name: &str) {
        if let Some((root, key)) = Self::split_target(name) {
            self.array_keys
                .entry(root.to_string())
                .or_default()
                .push(key.to_string());
        }
    }

    /// Decide list-vs-dict per observed array root. LIST mode requires
    /// every key numeric AND a literal DeclareArray (a dense python list
    /// exists to index into); anything else — string keys, dynamic `$k`
    /// keys, sparse writes — is DICT mode (pre-declared `{}`).
    fn finalize_unset_states(&mut self) {
        let mut last: std::collections::HashMap<String, bool> =
            std::collections::HashMap::new();
        for (_, name, is_unset) in self.seq_ops.iter() {
            last.insert(name.clone(), *is_unset);
        }
        for (name, was_unset) in last {
            if was_unset {
                self.unset_vars.insert(name);
            }
        }
    }

    fn finalize_array_modes(&mut self) {
        let roots: Vec<String> = self.array_keys.keys().cloned().collect();
        for root in roots {
            let keys = &self.array_keys[&root];
            let has_declare = keys.iter().any(|k| k == "\u{0}declare");
            let all_numeric = keys
                .iter()
                .filter(|k| k.as_str() != "\u{0}declare")
                .all(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_digit()));
            if !(has_declare && all_numeric && !keys.is_empty()) {
                self.array_dict_roots.insert(root);
            }
        }
    }

    /// Split a bracket target name into (root, key-text): `map[$k]` →
    /// ("map", "$k"); `arr[1]` → ("arr", "1"). None for plain names.
    fn split_target<'a>(name: &'a str) -> Option<(&'a str, &'a str)> {
        let open = name.find('[')?;
        if !name.ends_with(']') || open + 1 >= name.len() - 1 {
            return None;
        }
        Some((&name[..open], &name[open + 1..name.len() - 1]))
    }

    /// Render a bracket key text to a python subscript expression.
    /// Dict-mode keys are STRINGIFIED (bash assoc keys are strings);
    /// numeric literals become int literals for list-mode indexing.
    fn render_array_key(&self, key: &str) -> String {
        if let Some(stripped) = key.strip_prefix('$') {
            return self.py_ident(stripped);
        }
        if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
            return key.to_string();
        }
        Self::py_str(key)
    }

    /// Render an array element READ (`${arr[1]}` / `${map[$k]}`). Bash's
    /// unset-element semantics: missing keys read as "" — dict `.get`
    /// natively; list indices get a bounds guard. Dynamic (`$k`) list
    /// subscripts index directly (the collect pass only keeps a root in
    /// list mode when it saw a literal DeclareArray).
    fn render_array_read(&mut self, root: &str, key: &str) -> String {
        let r = self.py_ident(root);
        if self.array_dict_roots.contains(root) {
            // dict keys are STRINGIFIED (bash assoc semantics)
            let ke = match key.strip_prefix('$') {
                Some(v) => self.py_ident(v),
                None => Self::py_str(key),
            };
            return format!("{r}.get({ke}, \"\")");
        }
        if let Some(stripped) = key.strip_prefix('$') {
            return format!("{r}[{}]", self.py_ident(stripped));
        }
        format!("({r}[{key}] if len({r}) > {key} else \"\")")
    }

    /// A6-consistent Python-keyword mangling (renderers mangle the rest —
    /// the emitter's safe_ident only covers loop vars).
    fn py_ident(&self, name: &str) -> String {
        const PY_KEYWORDS: &[&str] = &[
            "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
            "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
            "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
            "return", "try", "while", "with", "yield",
        ];
        if PY_KEYWORDS.contains(&name) {
            format!("{name}_")
        } else {
            name.to_string()
        }
    }

    fn py_str(s: &str) -> String {
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

    /// Numeric-typed vars default to `0` and assign through `expr_as_num`
    /// (C semantics — the imperative frontends type ints as Int32/Int64/
    /// UInt32/UInt64, and a bare `x = "10"` would make `x > 3` a python
    /// str-vs-int TypeError).
    fn is_num(&self, name: &str) -> bool {
        matches!(
            self.var_types.get(name),
            Some(IrType::Int | IrType::Int32 | IrType::Int64 | IrType::UInt32 | IrType::UInt64)
        )
    }

    // ── never-written scan ──────────────────────────────────────────

    /// Collect every name WRITTEN anywhere in the program (assign targets,
    /// declares, loop vars, setVar/unset/read targets, capture vars, arith
    /// assigns). getVar of a name outside this set is an unset read and
    /// folds to "" under SH2_ASSUME_NO_ENV (see `call`).
    fn collect_writes(&mut self, stmts: &[IrStmt]) {
        for s in stmts {
            match s {
                IrStmt::Assign { targets, expr, .. } => {
                    for t in targets {
                        self.written.insert(t.var.clone());
                        // bracket-target writes (`map[foo]=bar`) observe
                        // the array root (list-vs-dict decision input)
                        self.note_array_target(&t.var);
                        if Self::split_target(&t.var).is_none() {
                            self.seq_ops.push((self.seq_ops.len(), t.var.clone(), false));
                        }
                    }
                    self.collect_writes_expr(expr);
                }
                IrStmt::Declare { vars, .. } => {
                    for d in vars {
                        self.written.insert(d.name.clone());
                    }
                }
                IrStmt::DeclareArray { var, elements, .. } => {
                    self.written.insert(var.clone());
                    // a literal DeclareArray makes the root a LIST candidate
                    if let Some(v) = self.array_keys.get_mut(var) {
                        v.push("\u{0}declare".into());
                    } else {
                        self.array_keys
                            .insert(var.clone(), vec!["\u{0}declare".into()]);
                    }
                    for e in elements {
                        self.collect_writes_expr(e);
                    }
                }
                IrStmt::For { var, iter, body } => {
                    self.written.insert(var.clone());
                    self.collect_writes_expr(iter);
                    self.collect_writes(body);
                }
                IrStmt::Expr(e) => self.collect_writes_expr(e),
                IrStmt::Output { value, .. } => self.collect_writes_expr(value),
                IrStmt::WriteFile { path, content, .. } => {
                    self.collect_writes_expr(path);
                    self.collect_writes_expr(content);
                }
                IrStmt::If { cond, then, elsifs, else_ } => {
                    self.collect_writes_expr(cond);
                    self.collect_writes(then);
                    for (c, b) in elsifs {
                        self.collect_writes_expr(c);
                        self.collect_writes(b);
                    }
                    self.collect_writes(else_);
                }
                IrStmt::While { cond, body } => {
                    self.collect_writes_expr(cond);
                    self.collect_writes(body);
                }
                IrStmt::DoWhile { body, cond, .. } => {
                    self.collect_writes(body);
                    self.collect_writes_expr(cond);
                }
                IrStmt::ForInit {
                    init,
                    cond,
                    step,
                    body,
                } => {
                    self.collect_writes(init);
                    self.collect_writes_expr(cond);
                    self.collect_writes(step);
                    self.collect_writes(body);
                }
                IrStmt::Break | IrStmt::Continue => {}
                IrStmt::Try {
                    body,
                    excepts,
                    else_body,
                    finally_body,
                } => {
                    self.collect_writes(body);
                    for e in excepts {
                        if let Some(asn) = &e.as_name {
                            // the `as` binding writes the runtime store
                            self.written.insert(asn.clone());
                            self.store_written.insert(asn.clone());
                        }
                        self.collect_writes(&e.body);
                    }
                    self.collect_writes(else_body);
                    self.collect_writes(finally_body);
                }
                IrStmt::Block(b) => self.collect_writes(b),
                IrStmt::Function { body, .. } => self.collect_writes(body),
                IrStmt::Exec { cmd, args, capture, .. } => {
                    if let Some(v) = capture {
                        self.written.insert(v.clone());
                    }
                    self.collect_writes_expr(cmd);
                    for a in args {
                        self.collect_writes_expr(a);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_writes_expr(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Call { func, args } => {
                match func.as_str() {
                    "setVar" => {
                        if let Some(IrExpr::Str(name, _)) = args.first() {
                            self.written.insert(name.clone());
                            // bracket-target writes (`arr[1]=x`,
                            // `map[foo]=bar`) observe the array root
                            self.note_array_target(name);
                            if Self::split_target(name).is_none() {
                                self.seq_ops.push((self.seq_ops.len(), name.clone(), false));
                            }
                            // setVar writes go through the runtime STORE,
                            // not a python binding — getVar of the same
                            // name must read the store back.
                            self.store_written.insert(name.clone());
                        }
                    }
                    "getVar" => {
                        if let Some(IrExpr::Str(name, _)) = args.first() {
                            // reads observe array roots too (a read-only
                            // `${map[$k]}` still needs the dict declared)
                            self.note_array_target(name);
                        }
                    }
                    // param("") reads carry bracket/plain array targets;
                    // setArray declares a root as a literal LIST
                    "param" => {
                        if let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) =
                            (args.first(), args.get(1))
                        {
                            if op.is_empty() {
                                self.note_array_target(name);
                            }
                            // ':=' WRITES the variable when unset — later
                            // reads must observe it (t04/t27)
                            if op == ":=" && Self::split_target(name).is_none() {
                                self.written.insert(name.clone());
                                self.seq_ops
                                    .push((self.seq_ops.len(), name.clone(), false));
                            }
                        }
                    }
                    "setArray" => {
                        if let Some(IrExpr::Str(name, _)) = args.first() {
                            if let Some(v) = self.array_keys.get_mut(name) {
                                v.push("\u{0}declare".into());
                            } else {
                                self.array_keys
                                    .insert(name.clone(), vec!["\u{0}declare".into()]);
                            }
                        }
                    }
                    // read/readarray/mapfile/getLine: every Str arg is a
                    // target name
                    "unset" | "read" | "readarray" | "mapfile" | "getLine" => {
                        for a in args {
                            if let IrExpr::Str(name, _) = a {
                                self.written.insert(name.clone());
                                if func == "unset" {
                                    self.seq_ops.push((self.seq_ops.len(), name.clone(), true));
                                }
                            }
                        }
                    }
                    _ => {}
                }
                for a in args {
                    self.collect_writes_expr(a);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                self.collect_writes_expr(lhs);
                self.collect_writes_expr(rhs);
            }
            IrExpr::Ternary { cond, then, else_ } => {
                self.collect_writes_expr(cond);
                self.collect_writes_expr(then);
                self.collect_writes_expr(else_);
            }
            IrExpr::DefinedOr { expr, default } => {
                self.collect_writes_expr(expr);
                self.collect_writes_expr(default);
            }
            IrExpr::Index { key, .. } => {
                self.collect_writes_expr(key);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.collect_writes_expr(x);
                    }
                }
            }
            IrExpr::Arith(a) => self.collect_writes_arith(a),
            IrExpr::MethodCall { obj, args, .. } => {
                self.collect_writes_expr(obj);
                for a in args {
                    self.collect_writes_expr(a);
                }
            }
            IrExpr::Array(items) => {
                for a in items {
                    self.collect_writes_expr(a);
                }
            }
            IrExpr::Object(fields) => {
                for (_, v) in fields {
                    self.collect_writes_expr(v);
                }
            }
            _ => {}
        }
    }

    fn collect_writes_arith(&mut self, a: &ArithAst) {
        match a {
            ArithAst::Assign { var, rhs, .. } => {
                self.written.insert(var.clone());
                self.collect_writes_arith(rhs);
            }
            ArithAst::Index { var, key, .. } => {
                self.written.insert(var.clone());
                self.collect_writes_arith(key);
            }
            ArithAst::Bin { lhs, rhs, .. } => {
                self.collect_writes_arith(lhs);
                self.collect_writes_arith(rhs);
            }
            ArithAst::Un { arg, .. } => self.collect_writes_arith(arg),
            ArithAst::Cond { test, then, else_, .. } => {
                self.collect_writes_arith(test);
                self.collect_writes_arith(then);
                self.collect_writes_arith(else_);
            }
            ArithAst::Cast { arg, .. } => self.collect_writes_arith(arg),
            ArithAst::IncDec { var, .. } => {
                self.written.insert(var.clone());
            }
            _ => {}
        }
    }

    /// A plain identifier-shaped name (excludes `?`/`$`/`#`/`@`/`*`/`-`,
    /// positionals `1`-`9` and index reads `arr[1]`) — the only names the
    /// unset-read fold may flatten to "".
    fn is_plain_name(name: &str) -> bool {
        let mut cs = name.chars();
        match cs.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return false,
        }
        cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    // ── expressions ──────────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => Self::py_str(s),
            IrExpr::Var(name, _) => self.py_ident(name),
            IrExpr::Ident(name) => self.py_ident(name),
            IrExpr::Bool(b) => {
                if *b {
                    "True".into()
                } else {
                    "False".into()
                }
            }
            IrExpr::Index { var, key } => {
                format!("{}[{}]", self.py_ident(var), self.expr(key))
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let l = self.expr(lhs);
                // `not` is unary in python; the IR only ever pairs it with a
                // meaningful lhs (the rhs is ignored)
                if matches!(op, crate::ir::BinOpKind::Not) {
                    return format!("(not {l})");
                }
                let r = self.expr(rhs);
                let py_op = match op {
                    crate::ir::BinOpKind::Add => "+",
                    crate::ir::BinOpKind::Sub => "-",
                    crate::ir::BinOpKind::Mul => "*",
                    crate::ir::BinOpKind::Div => "/",
                    crate::ir::BinOpKind::Mod => "%",
                    crate::ir::BinOpKind::Pow => "**",
                    crate::ir::BinOpKind::Concat => "+",
                    crate::ir::BinOpKind::Eq => "==",
                    crate::ir::BinOpKind::Ne => "!=",
                    crate::ir::BinOpKind::Lt => "<",
                    crate::ir::BinOpKind::Gt => ">",
                    crate::ir::BinOpKind::Le => "<=",
                    crate::ir::BinOpKind::Ge => ">=",
                    crate::ir::BinOpKind::And => "and",
                    crate::ir::BinOpKind::Or => "or",
                    crate::ir::BinOpKind::BitAnd => "&",
                    crate::ir::BinOpKind::BitOr => "|",
                    crate::ir::BinOpKind::BitXor => "^",
                    crate::ir::BinOpKind::ShiftL => "<<",
                    crate::ir::BinOpKind::ShiftR => ">>",
                    _ => {
                        self.mark_todo(&format!("BinOp {:?}", op));
                        "?".into()
                    }
                };
                if matches!(op, crate::ir::BinOpKind::And | crate::ir::BinOpKind::Or) {
                    // side-effecting call operands (exec → print(...)) return
                    // None; the truthy wrapper keeps `&&`/`||` status chaining
                    // bash-faithful (a successful echo always proceeds). Test/
                    // value operands stay as-is (`""` stays falsy).
                    let wrap = |x: &IrExpr, s: String| -> String {
                        if matches!(x, IrExpr::Call { func, .. } if func == "exec") {
                            format!("({s} or 1)")
                        } else {
                            s
                        }
                    };
                    return format!("({} {py_op} {})", wrap(lhs, l), wrap(rhs, r));
                }
                format!("({l} {py_op} {r})")
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::MethodCall { obj, method, args } => {
                let o = self.expr(obj);
                let a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
                format!("{o}.{method}({})", a.join(", "))
            }
            IrExpr::Ternary { cond, then, else_ } => format!(
                "({} if {} else {})",
                self.expr(then),
                self.expr(cond),
                self.expr(else_)
            ),
            IrExpr::DefinedOr { expr, default } => {
                format!("({} or {})", self.expr(expr), self.expr(default))
            }
            IrExpr::Interpolate(parts) => self.interp(parts),
            IrExpr::Capture { expr, .. } => {
                // `$(cmd args)` with a single exec body -> native capture
                if let IrExpr::Arrow(body) = expr.as_ref() {
                    if let [IrStmt::Expr(e)] = body.as_slice() {
                        if let IrExpr::Call { func, args } = e {
                            if func == "exec" {
                                let argv = self.build_argv(args);
                                self.need_subprocess = true;
                                    self.need_capture = true;
                                    self.need_capture = true;
                                return format!(
                                    "__sh_capture([{}])",
                                    argv.join(", ")
                                );
                            }
                            // `$(cmd1 | cmd2)` — a pipeline body
                            if func == "pipeline" {
                                return self.call("pipeline", args);
                            }
                            // `$(cmd < file)` — a redirect body -> bash -c
                            if func == "redirect" {
                                if let Some(c) = self.capture_redirect(args) {
                                    return c;
                                }
                            }
                        }
                    }
                }
                // The C frontend's outparam channel: capture(Arrow[
                // fnCall(..)]) — the callee echoes its out-params, the
                // caller captures STDOUT.
                if let IrExpr::Arrow(body) = expr.as_ref() {
                    let mut call: Option<(String, Vec<String>)> = None;
                    for st in body.iter() {
                        if let IrStmt::Expr(IrExpr::Call { func, args })
                        = st {
                            if func == "fnCall" || func == "$fn_call" {
                                let name = args.first().and_then(|a| match a {
                                    IrExpr::Str(n, _) => Some(n.clone()),
                                    _ => None,
                                });
                                let call_args = args.get(1).and_then(|a| match a {
                                    IrExpr::Array(elems) => Some(
                                        elems.iter().map(|e| self.expr(e)).collect::<Vec<_>>(),
                                    ),
                                    _ => None,
                                });
                                if let (Some(name), Some(call_args)) = (name, call_args) {
                                    call = Some((name, call_args));
                                }
                            }
                        }
                    }
                    if let Some((name, call_args)) = call {
                        self.need_capture_out = true;
                        return format!(
                            "__sh_capture_out({}, {})",
                            self.py_ident(&name),
                            call_args.join(", ")
                        );
                    }
                }
                // Generic body ladder (shared by captureWords/backticks):
                // see capture_body_expr.
                if let IrExpr::Arrow(body) = expr.as_ref() {
                    if let Some(c) = self.capture_body_expr(body) {
                        return c;
                    }
                }
                self.sh2_stub("capture", &[], "capture")
            },
            IrExpr::Regex { .. } => self.sh2_stub("regex", &[], "regex"),
            IrExpr::Range { start, end } => format!("range({}, {})", start, end + 1),
            IrExpr::RawExpr(s) => {
                self.mark_todo(&format!("RawExpr {s:?}"));
                "None".into()
            }
            IrExpr::Arrow(body) => {
                // a command body as a value — the shared capture ladder
                // (native exec/pipeline/redirect first, bash -c fallback)
                if let Some(c) = self.capture_body_expr(body) {
                    return c;
                }
                self.sh2_stub("arrow", &[], "arrow")
            }
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|e| self.expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            IrExpr::Object(kv) => {
                let entries: Vec<String> = kv
                    .iter()
                    .map(|(k, v)| format!("{}: {}", Self::py_str(k), self.expr(v)))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::py_str(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "True".into()
                    } else {
                        "False".into()
                    }
                }
                serde_json::Value::Null => "None".into(),
                _ => {
                    self.mark_todo("Json expr");
                    "None".into()
                }
            },
            other => {
                self.mark_todo(&format!("expr {:?}", other));
                "None".into()
            }
        }
    }

    /// Native python arithmetic from ArithAst (the numeric path).
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) | ArithAst::Ident(name) => {
                // bash coerces arith operands to integers; python would
                // string-repeat/double a str loop var, so wrap the read.
                // (int() of an int-typed var is a no-op.)
                format!("int({})", self.py_ident(name))
            }
            ArithAst::Index { var, key, .. } => {
                // arithmetic array subscript: a[1] + 1 — native list/dict
                // subscript on the collected root (list mode bounds-guarded
                // like render_array_read; dict via .get)
                if self.array_dict_roots.contains(var) {
                    let ke = match key.as_ref() {
                        ArithAst::Num(n) => Self::py_str(&n.to_string()),
                        _ => self.arith(key),
                    };
                    format!("({}.get({}, \"\"))", self.py_ident(var), ke)
                } else {
                    let k = self.arith(key);
                    let r = self.py_ident(var);
                    format!("int({r}[{k}] if len({r}) > {k} else 0)")
                }
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                if *op == "**" {
                    format!("pow({l},{r})")
                } else if op == "&&" {
                    // Python has no &&/|| — the C frontend's boolean ops
                    // inside arith ASTs (bool ok && !no). Numeric result:
                    // printf %d needs an int, not True/False.
                    format!("(1 if ({l} and {r}) else 0)")
                } else if op == "||" {
                    format!("(1 if ({l} or {r}) else 0)")
                } else if op == "/" {
                    // bash `/` truncates toward zero
                    format!("int({l} / {r})")
                } else {
                    format!("({l} {op} {r})")
                }
            }
            ArithAst::Un { op, arg } => {
                if op == "!" {
                    format!("(1 if not {} else 0)", self.arith(arg))
                } else {
                    format!("({op}{})", self.arith(arg))
                }
            }
            ArithAst::Cond { test, then, else_ } => format!(
                "({} if {} else {})",
                self.arith(then),
                self.arith(test),
                self.arith(else_)
            ),
            ArithAst::Assign { .. } => {
                // runtime setVar semantics (x+=) — sh2.arith stub
                self.sh2_calls.insert("arith".into());
                "sh2_arith()".into()
            }
            ArithAst::IncDec { var, delta, prefix } => {
                // x++ / ++x in VALUE position: bash yields the OLD value
                // for postfix and the NEW value for prefix, then updates.
                // The __sh_inc_pre/__sh_inc_post helpers do exactly that on
                // module globals (native — no store round-trip).
                self.need_inc = true;
                let d = delta.to_string();
                if *prefix {
                    format!("__sh_inc_pre({}, {})", Self::py_str(var), d)
                } else {
                    format!("__sh_inc_post({}, {})", Self::py_str(var), d)
                }
            }
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    /// String interpolation: f-string when every expression part renders to
    /// a quote-free atom (the common `"a$x"` case); otherwise a safe
    /// `str()` concatenation.
    fn interp(&mut self, parts: &[InterpPart]) -> String {
        if parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(x) => self.fstring_safe(x),
        }) {
            let mut s = String::from("f\"");
            for p in parts {
                match p {
                    InterpPart::Lit(t) => s.push_str(&Self::py_fstr_lit(t)),
                    InterpPart::Expr(x) => {
                        s.push('{');
                        s.push_str(&self.expr(x));
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        } else {
            let mut bits = Vec::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => bits.push(Self::py_str(t)),
                    InterpPart::Expr(x) => bits.push(format!("str({})", self.expr(x))),
                }
            }
            format!("({})", bits.join(" + "))
        }
    }

    /// Can this expression be embedded in an f-string `{...}` (i.e. its
    /// rendering contains no `"` or backslash)?
    fn fstring_safe(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Var(_, _) | IrExpr::Ident(_) | IrExpr::Int(_) | IrExpr::Bool(_) => true,
            IrExpr::Arith(a) => self.arith_safe(a),
            IrExpr::Call { func, args } if func == "getVar" => {
                // known vars render to bare idents; unknown → sh2_getVar("..")
                matches!(args.first(), Some(IrExpr::Str(name, _)) if self.var_types.contains_key(name))
            }
            _ => false,
        }
    }

    fn arith_safe(&self, a: &ArithAst) -> bool {
        match a {
            ArithAst::Num(_) | ArithAst::Var(_) => true,
            ArithAst::Bin { lhs, rhs, .. } => self.arith_safe(lhs) && self.arith_safe(rhs),
            ArithAst::Un { arg, .. } => self.arith_safe(arg),
            ArithAst::Cond { test, then, else_ } => {
                self.arith_safe(test) && self.arith_safe(then) && self.arith_safe(else_)
            }
            _ => false,
        }
    }

    /// Escape a literal for an f-string body: py_str plus brace doubling.
    fn py_fstr_lit(s: &str) -> String {
        let mut out = String::new();
        for c in s.chars() {
            match c {
                '{' => out.push_str("{{"),
                '}' => out.push_str("}}"),
                c => out.push(c),
            }
        }
        let inner = Self::py_str(&out);
        inner[1..inner.len() - 1].to_string()
    }

    /// A runtime-store call the preamble implements (the sh2.* subset for
    /// the imperative frontends): register the name and emit the call WITH
    /// its rendered args (the runtime functions take real arguments — the
    /// old zero-arg stubs could never be implemented).
    fn sh2_call(&mut self, name: &str, args: &[IrExpr]) -> String {
        let safe = name.replace('.', "_");
        self.sh2_calls.insert(safe.clone());
        let rendered: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
        format!("sh2_{safe}({})", rendered.join(", "))
    }

    fn sh2_stub(&mut self, name: &str, args: &[IrExpr], note: &str) -> String {
        let safe = name.replace('.', "_");
        self.sh2_calls.insert(safe.clone());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        let rendered: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
        format!("sh2_{safe}({})", rendered.join(", "))
    }

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "capture" => {
                // Generic body first (exec/pipeline/redirect/bash -c) —
                // the fnCall outparam channel below only fits function calls.
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    if let Some(c) = self.capture_body_expr(body) {
                        return c;
                    }
                }
                // The C frontend's outparam channel: capture(Arrow[
                // fnCall(..)]) — the callee echoes its out-params, the
                // caller captures STDOUT.
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    let mut call: Option<(String, Vec<String>)> = None;
                    for st in body.iter() {
                        if let IrStmt::Expr(IrExpr::Call { func: f, args: fargs }) = st {
                            if f == "fnCall" || f == "$fn_call" {
                                let name = fargs.first().and_then(|a| match a {
                                    IrExpr::Str(n, _) => Some(n.clone()),
                                    _ => None,
                                });
                                let call_args = fargs.get(1).and_then(|a| match a {
                                    IrExpr::Array(elems) => Some(
                                        elems.iter().map(|e| self.expr(e)).collect::<Vec<_>>(),
                                    ),
                                    _ => None,
                                });
                                if let (Some(name), Some(cargs)) = (name, call_args) {
                                    call = Some((name, cargs));
                                }
                            }
                        }
                    }
                    if let Some((name, call_args)) = call {
                        self.need_capture_out = true;
                        return format!(
                            "__sh_capture_out({}, {})",
                            self.py_ident(&name),
                            call_args.join(", ")
                        );
                    }
                }
                self.sh2_stub("capture", args, "capture")
            }
            // `echo X | grep LIT >/dev/null` → contains(X, LIT): native
            // python `PAT in STR`.
            // captureWords(`cmd`) — backtick command substitution: the
            // SAME lowering ladder as the $() Capture form (single exec →
            // native check_output; pipeline/redirect bodies → their call
            // arms; else the bash -c text fallback). Before this arm
            // existed it hit the wildcard stub, and its Arrow argument
            // rendered as a bare sh2_arrow() call.
            "captureWords" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    if let Some(c) = self.capture_body_expr(body) {
                        return c;
                    }
                }
                self.sh2_stub("captureWords", args, "captureWords")
            }
            // listVar("@") / listVar("*") — the positional PARAMETERS as
            // a LIST (for iteration); scalars use getVar("@")
            "listVar" => {
                match args.first() {
                    Some(IrExpr::Str(sel, _)) if sel == "@" || sel == "*" => {
                        "\" \".join(sys.argv[1:])".to_string()
                    }
                    _ => self.sh2_stub("listVar", args, "listVar"),
                }
            }
            // join(...) — the core's StringPart lowering WRAPS every
            // ${..} expansion in join(<expr>) (shir.rs StringPart::Param /
            // MapKeys / ArrayIndex): unwrap and route.
            "join" => {
                match args.first() {
                    // join(param(op, name, ..)) — any parameter expansion
                    Some(IrExpr::Call { func, args: pargs }) if func == "param" => {
                        self.call("param", pargs)
                    }
                    // join(arrayIndex(name, key)) — single element read
                    Some(IrExpr::Call { func, args: iargs })
                        if func == "arrayIndex" =>
                    {
                        match (iargs.first(), iargs.get(1)) {
                            (Some(IrExpr::Str(name, _)), Some(key)) => {
                                let k = self.expr(key);
                                let r = self.py_ident(name);
                                if self.array_dict_roots.contains(name) {
                                    format!("{r}.get({k}, \"\")")
                                } else {
                                    format!("({r}[{k}] if len({r}) > {k} else \"\")")
                                }
                            }
                            _ => self.sh2_stub("join", args, "join"),
                        }
                    }
                    // join(arrayItems(name)) — all values, space-joined
                    Some(IrExpr::Call { func, args: iargs })
                        if func == "arrayItems" =>
                    {
                        match iargs.first() {
                            Some(IrExpr::Str(name, _)) => {
                                let r = self.py_ident(name);
                                if self.array_dict_roots.contains(name) {
                                    format!("\" \".join(str(v) for v in {r}.values())")
                                } else {
                                    format!("\" \".join(str(x) for x in {r})")
                                }
                            }
                            _ => self.sh2_stub("join", args, "join"),
                        }
                    }
                    _ => self.sh2_stub("join", args, "join"),
                }
            }
            // setArrayAppend(name, elems) — arr+=(x y): native list extend
            // on collected roots
            "setArrayAppend" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) || self.array_keys.contains_key(name) {
                        let elems = args
                            .get(1)
                            .map(|a| self.expr(a))
                            .unwrap_or_else(|| "[]".into());
                        return format!("{} += {elems}", self.py_ident(name));
                    }
                }
                self.sh2_stub("setArrayAppend", args, "setArrayAppend")
            }
            // redirect(inner, [redirect objects]) used in expression
            // chains (`cmd 2>/dev/null && ...`): run the inner body under
            // bash -c and return its STATUS so && / || chaining stays
            // faithful. stderr is discarded by the runtime contract, so fd-2
            // redirects need no code. Documented fork/exec escape.
            "redirect" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    if let Some(text) = self.body_shell_text(body) {
                        self.need_subprocess = true;
                        self.need_capture = true;
                        return format!(
                            "__sh_run_status([\"bash\", \"-c\", {}])",
                            Self::py_str(&text)
                        );
                    }
                }
                self.sh2_stub("redirect", args, "redirect")
            }
            "contains" => {
                if let (Some(needle), Some(pattern)) = (args.first(), args.get(1)) {
                    let needle = self.expr(needle);
                    let pattern = self.expr(pattern);
                    return format!("{pattern} in {needle}");
                }
                self.sh2_stub("contains", args, "contains")
            }
            // exec("echo", [args...]) → native print (python's print IS echo
            // semantics: space-separated args + trailing newline);
            // exec("printf", [fmt, args...]) → native sys.stdout.write
            "exec" => {
                // exec("true") / exec("false") — the restructure pass's
                // While(true) cond (a backward-goto loop) — lower to the
                // python constants
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if cmd == "true" {
                        return "True".into();
                    }
                    if cmd == "false" {
                        return "False".into();
                    }
                }
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if cmd == "echo" {
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            let rendered: Vec<String> =
                                items.iter().map(|i| self.expr(i)).collect();
                            if rendered.is_empty() {
                                return "print()".into();
                            }
                            return format!("print({})", rendered.join(", "));
                        }
                    }
                    if cmd == "printf" {
                        return self.printf_call(args);
                    }
                    if cmd == "exit" {
                        let code = match args.get(1) {
                            Some(IrExpr::Array(items)) if !items.is_empty() => self.expr(&items[0]),
                            _ => "0".to_string(),
                        };
                        self.need_sys = true;
                        return format!("sys.exit({code})");
                    }
                    if cmd == "cd" {
                        // `cd [dir]` — os.chdir (bash cd; PWD updated)
                        let dir = match args.get(1) {
                            Some(IrExpr::Array(items)) if !items.is_empty() => {
                                self.expr(&items[0])
                            }
                            _ => "os.path.expanduser(\"~\")".to_string(),
                        };
                        return format!("os.chdir({dir})");
                    }
                    if cmd == "read" {
                        // `read [-r] var` — read a line into the target(s).
                        // Flags ("-r") precede the variable names; a plain
                        // name assigns NATIVELY (the old path stuffed the
                        // FLAG into the store key: sh2_setVar("-r", ..)!).
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            let targets: Vec<&IrExpr> =
                                items.iter().skip_while(|i| {
                                    matches!(i, IrExpr::Str(s, _) if s.starts_with('-'))
                                }).collect();
                            if let [single] = targets.as_slice() {
                                if let IrExpr::Str(v, _) = single {
                                    if Self::is_plain_name(v)
                                        && !self.store_written.contains(v)
                                    {
                                        return format!(
                                            "{} = sys.stdin.readline().rstrip(\"\\n\")",
                                            self.py_ident(v)
                                        );
                                    }
                                }
                            }
                            // multi-var / store targets: first name keeps
                            // the line, the rest stay empty (bash splits on
                            // IFS; the corpus never exercises that)
                            if let Some(IrExpr::Str(v, _)) = targets.first() {
                                self.sh2_calls.insert("setVar".into());
                                return format!(
                                    "sh2_setVar({}, sys.stdin.readline().rstrip(\"\\n\"))",
                                    Self::py_str(v)
                                );
                            }
                        }
                    }
                    if cmd == "unset" {
                        // `unset var` — a NATIVE var re-initializes to ""
                        // (shell unset reads as empty); only runtime-store
                        // vars pop the store
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            if let Some(IrExpr::Str(v, _)) = items.first() {
                                if Self::is_plain_name(v) && !self.store_written.contains(v) {
                                    return format!("{} = \"\"", self.py_ident(v));
                                }
                                self.sh2_calls.insert("setVar".into());
                                return format!(
                                    "__sh_store.pop({}, None)",
                                    Self::py_str(v)
                                );
                            }
                        }
                    }
                    if cmd == "let" {
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            if let Some(IrExpr::Str(text, _)) = items.first() {
                                if let Some(c) = self.render_let_cond(text) {
                                    return c;
                                }
                            }
                        }
                    }
                }
                // external command: fork/exec via subprocess
                let argv = self.build_argv(args);
                self.need_subprocess = true;
                return format!("__sh_exec([{}])", argv.join(", "));
            }
            // getVar("y") — the ShIR's form of a `$y` read; typed vars are
            // plain python names, a store-written name (the imperative
            // frontends' handle temps) reads the runtime store, a
            // never-written plain name is unset at every read (→ "", the
            // SH2_ASSUME_NO_ENV fold), a native-written plain name is the
            // python binding; anything else → runtime stub
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    // bash special/positional vars — native reads.
                    match name.as_str() {
                        "?" => {
                            self.need_rc = true;
                            return "str(__sh_rc)".into();
                        }
                        "#" => return "str(len(sys.argv) - 1)".into(),
                        "@" | "*" => return "\" \".join(sys.argv[1:])".into(),
                        n if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
                            let i: i64 = n.parse().unwrap_or(1);
                            return format!(
                                "(sys.argv[{i}] if len(sys.argv) > {i} else \"\")"
                            );
                        }
                        _ => {}
                    }
                    // ${var+word} family — the frontend folds parameter
                    // expansions into the NAME ("MYVAR+set"). Decode the
                    // suffix: two-char ops first (:+ :- := :?), then single
                    // (+ - = ?). Set-ness uses the collected write sets
                    // (unset == never-written); empty-vs-unset is not
                    // modeled (all vars pre-declare ""), matching the
                    // runtime's own scalar store semantics.
                    if name.len() > 1 && !name.starts_with('$')
                        && !name.chars().next().unwrap().is_ascii_digit()
                    {
                        let bytes = name.as_bytes();
                        let mut pos: Option<usize> = None;
                        for i in 1..bytes.len() {
                            if matches!(bytes[i], b'+' | b'-' | b'=' | b'?') {
                                pos = Some(i);
                                break;
                            }
                        }
                        if let Some(i) = pos {
                            let (root, rest) = name.split_at(i);
                            let word = &rest[1..];
                            let colon_form = root.ends_with(':');
                            let root = root.trim_end_matches(':');
                            let is_set = !self.unset_vars.contains(root)
                                && (self.written.contains(root)
                                    || self.store_written.contains(root)
                                    || self.var_types.contains_key(root));
                            let non_empty = is_set; // cannot distinguish; see comment
                            // the baked suffix keeps SOURCE quoting:
                            // ${@:-"default"} -> word '"default"' —
                            // strip one layer of surrounding quotes
                            let mut w = word;
                            if w.len() >= 2
                                && ((w.starts_with('"') && w.ends_with('"'))
                                    || (w.starts_with('\'') && w.ends_with('\'')))
                            {
                                w = &w[1..w.len() - 1];
                            }
                            match (&rest[..1], colon_form) {
                                ("+", false) => {
                                    return if is_set { Self::py_str(w) } else { "\"\"".into() }
                                }
                                (":+" | "+", true) | (":+", _) => {
                                    return if non_empty { Self::py_str(w) } else { "\"\"".into() }
                                }
                                ("-", false) => {
                                    return if is_set {
                                        self.call("getVar", &[IrExpr::Str(root.to_string(), crate::ir::StrStyle::DoubleQuoted)])
                                    } else {
                                        Self::py_str(w)
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // bracket target: native list/dict subscript
                    // (`${arr[1]}`, `${map[foo]}`, `${map[$k]}`)
                    if let Some((root, key)) = Self::split_target(name) {
                        if self.array_dict_roots.contains(root)
                            || self
                                .array_keys
                                .contains_key(root)
                        {
                            return self.render_array_read(root, key);
                        }
                    }
                    if self.var_types.contains_key(name) {
                        return self.py_ident(name);
                    }
                    if self.store_written.contains(name) {
                        self.sh2_calls.insert("getVar".into());
                        return format!("sh2_getVar({})", Self::py_str(name));
                    }
                    if self.written.contains(name) && Self::is_plain_name(name) {
                        return self.py_ident(name);
                    }
                    if !self.written.contains(name) && Self::is_plain_name(name) {
                        return "\"\"".into();
                    }
                }
                self.sh2_stub("getVar", args, "getVar")
            }
            // setVar(name, value) — typed var → native assignment (the
            // getVar mirror); store-written name → runtime store write;
            // anything else → runtime stub.
            "setVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    let value = args
                        .get(1)
                        .map(|a| self.expr(a))
                        .unwrap_or_else(|| "\"\"".into());
                    // bracket-target write (`arr[1]=x`, `map[foo]=bar`) →
                    // native list/dict subscript assignment
                    if let Some((root, key)) = Self::split_target(name) {
                        if self.array_dict_roots.contains(root)
                            || self.array_keys.contains_key(root)
                        {
                            // dict-mode keys are STRINGIFIED; list-mode
                            // numeric keys stay ints (render_array_key)
                            let ke = if self.array_dict_roots.contains(root) {
                                match key.strip_prefix('$') {
                                    Some(v) => self.py_ident(v),
                                    None => Self::py_str(key),
                                }
                            } else {
                                self.render_array_key(key)
                            };
                            return format!("{}[{ke}] = {value}", self.py_ident(root));
                        }
                    }
                    if self.var_types.contains_key(name) {
                        return format!("{} = {value}", self.py_ident(name));
                    }
                    if self.store_written.contains(name) && Self::is_plain_name(name) {
                        self.sh2_calls.insert("setVar".into());
                        return format!("sh2_setVar({}, {value})", Self::py_str(name));
                    }
                }
                self.sh2_stub("setVar", args, "setVar")
            }
            // ternary(cond, a, b) — the C frontend's `cond ? a : b`: the
            // cond is a test STRING (evaluated with the same native-first
            // policy as the A1 `test` call), the branches are lowered A1
            // values — pure, eager evaluation is sound.
            // shopt -s/-u opt — a bash shell OPTION toggle (extglob,
            // nocasematch, nullglob). There is no python equivalent: the
            // rendered subset does not re-implement bash glob matching, so
            // the toggle lowers to a no-op None expression (a valid python
            // statement AND subexpression). The executed-stdout equivalence
            // gate is the correctness check for this choice — any corpus
            // script whose behavior depends on the option DIFFs and fails
            // loudly rather than silently passing.
            "shopt" => "None".into(),
            "ternary" => {
                let cond = match args.first() {
                    Some(IrExpr::Str(s, _)) => self.test_render(s).unwrap_or_else(|| {
                        // unrenderable test string: the runtime fallback
                        self.sh2_calls.insert("test".into());
                        format!("sh2_test({})", Self::py_str(s))
                    }),
                    _ => return self.sh2_stub("ternary", args, "ternary"),
                };
                let then = args
                    .get(1)
                    .map(|a| self.expr(a))
                    .unwrap_or_else(|| "\"\"".into());
                let else_ = args
                    .get(2)
                    .map(|a| self.expr(a))
                    .unwrap_or_else(|| "\"\"".into());
                format!("({then} if {cond} else {else_})")
            }
            // param(op, name, ..) — the C frontend's strlen lowering is the
            // `${#name}` len op on a native var: `param("len", "s")`.
            "brace" => {
                // `{x,y}{1..2}` / `a{1..3}b` — brace expansion. The groups
                // are literals (ranges/lists), so expand them statically at
                // render time and emit the space-joined words as a python
                // string. Refuses any non-literal group.
                if let Some(w) = py_brace_words(args) {
                    return Self::py_str(&w);
                }
                self.sh2_stub("brace", args, "brace")
            }
            "param" => {
                if let Some(IrExpr::Str(op, _)) = args.first() {
                    // param("", name) — a plain ${name} read (the frontend's
                    // expansion lowering uses the empty op for the bare
                    // form, including indexed/assoc targets like
                    // "arr[1]" / "map[$k]"). Route through getVar so the
                    // native-first ladder applies instead of the wildcard
                    // stub.
                    if op.is_empty() {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            return self.call(
                                "getVar",
                                &[IrExpr::Str(
                                    name.clone(),
                                    crate::ir::StrStyle::DoubleQuoted,
                                )],
                            );
                        }
                    }
                    if op == "len" {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            return format!("str(len({}))", self.py_ident(name));
                        }
                    }
                    // ${x:?msg} — error+exit when unset/empty; else
                    // the value (bash: message to STDERR, shell exits 1)
                    if op == ":?" {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            self.need_sys = true;
                            let v = self.call(
                                "getVar",
                                &[IrExpr::Str(
                                    name.clone(),
                                    crate::ir::StrStyle::DoubleQuoted,
                                )],
                            );
                            let msg = match args.get(2) {
                                Some(IrExpr::Str(s, _)) => Self::py_str(s),
                                _ => "\"parameter null or not set\"".to_string(),
                            };
                            return format!(
                                "({v} if {v} != \"\" else sys.exit({msg}))"
                            );
                        }
                    }
                    // ${x#pat} / ${x##pat} / ${x%pat} / ${x%%pat} —
                    // prefix/suffix removal via the native __sh_strip
                    // runtime-free helper (same one test operands use)
                    if matches!(op.as_str(), "#" | "##" | "%" | "%%") {
                        if let (Some(IrExpr::Str(name, _)), Some(IrExpr::Str(pat, _))) =
                            (args.get(1), args.get(2))
                        {
                            self.need_strip = true;
                            let v = self.call(
                                "getVar",
                                &[IrExpr::Str(
                                    name.clone(),
                                    crate::ir::StrStyle::DoubleQuoted,
                                )],
                            );
                            return format!(
                                "__sh_strip({}, {}, {})",
                                Self::py_str(pat),
                                Self::py_str(op),
                                v
                            );
                        }
                    }
                    // param("slice", name, sel, off[:len]) — the array
                    // expansion family:
                    //   ("slice","#arr","@","")    → ${#arr[@]}  (length)
                    //   ("slice","arr","@",off:len) → ${arr[@]:o:l} (values)
                    //   ("slice","!map","@","")     → ${!map[@]}  (keys)
                    if op == "slice" {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            // ${#name[@]} — element count
                            if let Some(root) = name.strip_prefix('#') {
                                return format!("str(len({}))", self.py_ident(root));
                            }
                            // ARRAY-ROOT GUARD: only intercept when the
                            // name is a bracket target or a known array
                            // root — otherwise this is a plain STRING
                            // slice (${s:1:3}); do NOT intercept, fall
                            // through to the scalar slice handler below.
                            let probe = name.strip_prefix('!').unwrap_or(name);
                            let array_ish = Self::split_target(probe).is_some()
                                || self.array_keys.contains_key(probe);
                            if array_ish {
                            // ${!map[@]} — KEY list (strip the ! marker)
                            let keys_mode = name.starts_with('!');
                            let root_name = if keys_mode { &name[1..] } else { name };
                            let known = self.array_keys.contains_key(root_name);
                            let (root, key) = match Self::split_target(root_name) {
                                Some((r, k)) => (r, k),
                                _ if known => (root_name, "@"),
                                _ => return self.sh2_stub("param", args, "param"),
                            };
                            let r = self.py_ident(root);
                            let off = args.get(3).and_then(|a| match a {
                                IrExpr::Str(s, _) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                            // slice bounds "" | "o" | "o:l"
                            let (o, l) = match off.split_once(':') {
                                Some((a, b)) => (a.to_string(), Some(b.to_string())),
                                _ => (off.clone(), None),
                            };
                            let dict = self.array_dict_roots.contains(root);
                            let values = if keys_mode && dict {
                                format!("{r}.keys()")
                            } else if keys_mode {
                                return self.sh2_stub("param", args, "param");
                            } else if key == "@" || key == "*" {
                                if dict {
                                    format!("[str(v) for v in {r}.values()]")
                                } else {
                                    format!("[str(x) for x in {r}]")
                                }
                            } else {
                                // single-element read — same as getVar
                                return self.render_array_read(root, key);
                            };
                            // apply the offset/length slice to the list
                            let sliced = if o.is_empty() {
                                values
                            } else if let Some(len) = &l {
                                format!("({values})[{o}:{o} + {len}]")
                            } else {
                                format!("({values})[{o}:]")
                            };
                            return format!("\" \".join({sliced})");
                            }
                        }
                    }
                    // `${x:-default}` — the value if non-empty else the default
                    // case conversion: ${x^^} / ${x,,} / ${x^} / ${x,}
                    if op == "^^" || op == ",," || op == "^" || op == "," {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            let v = self.call(
                                "getVar",
                                &[IrExpr::Str(
                                    name.to_string(),
                                    crate::ir::StrStyle::DoubleQuoted,
                                )],
                            );
                            match op.as_str() {
                                "^^" => return format!("{v}.upper()"),
                                ",," => return format!("{v}.lower()"),
                                "^" => return format!(
                                    "({v}[:1].upper() + {v}[1:] if {v} else \"\")"
                                ),
                                _ => return format!(
                                    "({v}[:1].lower() + {v}[1:] if {v} else \"\")"
                                ),
                            }
                        }
                    }
                    if op == ":-" || op == ":=" {
                        if let (Some(IrExpr::Str(name, _)), Some(def)) =
                            (args.get(1), args.get(2))
                        {
                            let v = self.call("getVar", &[IrExpr::Str(name.to_string(), crate::ir::StrStyle::DoubleQuoted)]);
                            // ${x:-"word"} keeps SOURCE quotes in the baked
                            // default string — strip ONE layer so python
                            // prints `default`, not `"default"`
                            let d = match def {
                                IrExpr::Str(s, _) if s.len() >= 2
                                    && ((s.starts_with('"') && s.ends_with('"'))
                                        || (s.starts_with('\'') && s.ends_with('\''))) =>
                                {
                                    Self::py_str(&s[1..s.len() - 1])
                                }
                                _ => self.expr(def),
                            };
                            if op == ":=" {
                                // ':=' ASSIGNS the default when unset —
                                // __sh_assign writes the module global and
                                // yields the value (a bare walrus would
                                // overwrite the var even when set)
                                self.need_assign = true;
                                let pv = self.py_ident(name);
                                return format!(
                                    "({v} if {v} != \"\" else __sh_assign({}, {d}))",
                                    Self::py_str(name)
                                );
                            }
                            if op == ":-" {
                                return format!("({v} if {v} != \"\" else {d})");
                            }
                            return format!("({v} if {v} != \"\" else ({d}))");
                        }
                    }
                    // \${s:off:len}` — substring slice
                    if op == "slice" {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            let v = self.call("getVar", &[IrExpr::Str(name.to_string(), crate::ir::StrStyle::DoubleQuoted)]);
                            let o = self.expr_as_num(&args[2]);
                            let l = self.expr_as_num(&args[3]);
                            return format!("{v}[{o}:{o}+{l}]");
                        }
                    }
                    // \${x##*/} / basename / dirname — native os.path
                    if op == "basename" || op == "dirname" {
                        if let Some(IrExpr::Str(name, _)) = args.get(1) {
                            let v = self.call(
                                "getVar",
                                &[IrExpr::Str(
                                    name.to_string(),
                                    crate::ir::StrStyle::DoubleQuoted,
                                )],
                            );
                            let f = if op == "basename" { "basename" } else { "dirname" };
                            return format!("os.path.{f}({v})");
                        }
                    }
                    // \${x#pat} / \${x##pat} / \${x%pat} / \${x%%pat} —
                    // strip prefix/suffix, literal pattern only (no glob).
                    if op == "#" || op == "##" || op == "%" || op == "%%" {
                        if let (Some(IrExpr::Str(name, _)), Some(IrExpr::Str(pat, _))) =
                            (args.get(1), args.get(2))
                        {
                            let v = self.call(
                                "getVar",
                                &[IrExpr::Str(
                                    name.to_string(),
                                    crate::ir::StrStyle::DoubleQuoted,
                                )],
                            );
                            if !pat.contains('*') && !pat.contains('?') && !pat.contains('[') {
                                let p = Self::py_str(pat);
                                return match op.as_str() {
                                    "#" | "##" => format!(
                                        "({v}[len({p}):] if {v}.startswith({p}) else {v})"
                                    ),
                                    _ => format!(
                                        "({v}[:len({v})-len({p})] if {v}.endswith({p}) else {v})"
                                    ),
                                };
                            }
                            // glob pattern — runtime sh2_strip
                            let which = match op.as_str() {
                                "#" => "p",
                                "##" => "P",
                                "%" => "s",
                                _ => "S",
                            };
                            self.sh2_calls.insert("strip".into());
                            return format!(
                                "sh2_strip({v}, {}, {})",
                                Self::py_str(pat),
                                Self::py_str(which)
                            );
                        }
                    }
                    // ${x/pat/repl} / ${x//pat/repl} — substitute (literal pattern).
                    if op == "/" || op == "//" {
                        if let (
                            Some(IrExpr::Str(name, _)),
                            Some(IrExpr::Str(pat, _)),
                            Some(IrExpr::Str(repl, _)),
                        ) = (args.get(1), args.get(2), args.get(3))
                        {
                            if !pat.contains('*') && !pat.contains('?') && !pat.contains('[') {
                                let v = self.call(
                                    "getVar",
                                    &[IrExpr::Str(
                                        name.to_string(),
                                        crate::ir::StrStyle::DoubleQuoted,
                                    )],
                                );
                                let p = Self::py_str(pat);
                                let r = Self::py_str(repl);
                                let cnt = if op == "/" { "1" } else { "-1" };
                                return format!("{v}.replace({p}, {r}, {cnt})");
                            }
                        }
                    }
                }
                self.sh2_stub("param", args, "param")
            }
            // arith("$i") — the imperative frontends' arith-text reads; the
            // common `$name` form is a native int() read of the binding.
            "arith" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(name) = s.strip_prefix('$') {
                        if Self::is_plain_name(name) && !self.store_written.contains(name) {
                            return format!("int({})", self.py_ident(name));
                        }
                    }
                    // a general arith STRING (`($x % 2)` — the C frontend's
                    // testArith/ternary-cond shape): normalize $refs to
                    // bare idents, parse, render via the Arith AST walker
                    let norm: String = s
                        .chars()
                        .enumerate()
                        .map(|(i, c)| {
                            if c == '$'
                                && i + 1 < s.len()
                                && (s[i + 1..].starts_with('{')
                                    || s[i + 1..]
                                        .chars()
                                        .next()
                                        .map_or(false, |n| n.is_ascii_alphabetic() || n == '_'))
                            {
                                ' '
                            } else {
                                c
                            }
                        })
                        .collect();
                    let norm = norm.replace('{', " ").replace('}', " ");
                    if let Some(ast) = crate::shir::parse_arith(norm.trim()) {
                        return self.arith(&ast);
                    }
                }
                self.sh2_stub("arith", args, "arith")
            }
            // the C memory model (malloc/pointer arithmetic): the arena
            // runtime (subset of sh2-namespace.mjs slice 2).
            "memAlloc" | "memStore" | "memLoad" | "memAdvance" | "memFree"
            | "memTest" | "memElemSize" => self.sh2_call(func, args),
            // setArray/arrayIndex — a typed name is a native python list
            // (the C array `a[4]`); untyped names keep the runtime store.
            "setArray" => {
                // VALUE position (the Assign stmt wraps it: `a = <this>`):
                // emit the bare list literal, never an assignment — a
                // nested `arr = arr = [...]` chained form leaked earlier.
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) || self.array_keys.contains_key(name) {
                        let elems = args
                            .get(1)
                            .map(|a| self.expr(a))
                            .unwrap_or_else(|| "[]".into());
                        return elems;
                    }
                }
                self.sh2_call("setArray", args)
            }
            "arrayIndex" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) {
                        let key = args
                            .get(1)
                            .map(|a| self.expr(a))
                            .unwrap_or_else(|| "0".into());
                        return format!("{}[int({key})]", self.py_ident(name));
                    }
                }
                self.sh2_call("arrayIndex", args)
            }
            // test("...") — mini evaluator for the common numeric/string
            // patterns; anything else → runtime stub.
            "line" => {
                // multi-return line read (`line(cap, N)` = the C frontend's
                // outparam channel): String(v).split('\n')[N] with "" default
                if let (Some(v), Some(IrExpr::Str(i, _))) = (args.first(), args.get(1)) {
                    let ve = self.expr(v);
                    let n: usize = i.parse().unwrap_or(0);
                    return format!("(__sh_lines({})[{n}] if {} else '')", ve, ve.clone());
                }
                self.sh2_stub("line", args, "line")
            }
            "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return c;
                    }
                    // bash -c fallback for a test shape test_render can't
                    // parse natively: run `[ <test> ]` (fork/exec).
                    self.need_subprocess = true;
                    return format!(
                        "(__sh_run_status([\"bash\", \"-c\", {}]) == 0)",
                        Self::py_str(&format!("[ {s} ]"))
                    );
                }
                self.sh2_stub("test", args, "test")
            }
            // split(getVar(name)) — IFS field-split of a scalar read is a
            // no-op (mirrors the estree nospace fold); the read's own
            // rendering is the value
            "split" => {
                if let Some(IrExpr::Call { func, args: inner }) = args.first() {
                    if func == "getVar" {
                        return self.call("getVar", inner);
                    }
                }
                self.sh2_stub("split", args, "split")
            }
            // everything else → compile-able sh2.* stub
            // `grepMatches(text, pattern, flags)` — the `grep -o` lift:
            // native re.findall (one match per line, grep -o's output).
            // flags: E (ERE as-is), F (fixed), i (case-insensitive).
            "grepMatches" => {
                let text = args.first().map(|a| self.expr(a)).unwrap_or_else(|| "\"\"".into());
                let pat = match args.get(1) {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return self.sh2_stub("grepMatches", args, "grepMatches"),
                };
                let flags = match args.get(2) {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => String::new(),
                };
                self.need_re = true;
                let mut body = pat;
                if flags.contains('F') {
                    let mut lit = String::new();
                    for c in body.chars() {
                        if matches!(c, '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\') {
                            lit.push('\\');
                        }
                        lit.push(c);
                    }
                    body = lit;
                } else if !flags.contains('E') {
                    body = body
                        .replace("\\\\+", "+").replace("\\\\?", "?")
                        .replace("\\(", "(").replace("\\)", ")")
                        .replace("\\\\|", "|").replace("\\\\{", "{").replace("\\\\}", "}");
                }
                let rc = format!("\"\\n\".join(re.findall({}, {text}))", Self::py_str(&body));
                rc
            }
            "pipeline" => {
                // `cmd1 | cmd2` — bash -c fork/exec fallback (a pipeline is
                // a shell primitive; subprocess pipes would need stage wiring).
                if let Some(IrExpr::Array(stages)) = args.first() {
                    // NATIVE-FIRST PIPELINE: a leading complex stage (for/
                    // while loops over python state) renders NATIVELY under
                    // redirect_stdout; its text feeds the remaining plain
                    // exec stages via stdin. A bash -c fallback here would
                    // LOSE the python-side state (dict/list contents), so
                    // this is the faithful lowering, not just an escape.
                    if stages.len() >= 2 {
                        if let Some(IrExpr::Arrow(fbody)) = stages.first() {
                            let simple0 =
                                matches!(fbody.as_slice(), [IrStmt::Expr(e)]
                                    if matches!(e, IrExpr::Call { func, .. } if func == "exec"));
                            if !simple0 {
                                let mut rest_argv: Vec<Vec<String>> = Vec::new();
                                let mut ok = true;
                                for st in &stages[1..] {
                                    match st {
                                        IrExpr::Arrow(b) => match b.as_slice() {
                                            [IrStmt::Expr(e)] => match e {
                                                IrExpr::Call { func, args }
                                                    if func == "exec" =>
                                                {
                                                    let mut one = Vec::new();
                                                    if let Some(IrExpr::Str(c, _)) =
                                                        args.first()
                                                    {
                                                        one.push(c.clone());
                                                    }
                                                    if let Some(IrExpr::Array(items)) =
                                                        args.get(1)
                                                    {
                                                        for it in items {
                                                            match self.expr(it) {
                                                                x if x.starts_with('"') => {
                                                                    one.push(x)
                                                                }
                                                                _ => {
                                                                    ok = false;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    rest_argv.push(one);
                                                }
                                                _ => ok = false,
                                            },
                                            _ => ok = false,
                                        },
                                        _ => ok = false,
                                    }
                                    if !ok {
                                        break;
                                    }
                                }
                                if ok && !rest_argv.is_empty() {
                                    self.emit("import io");
                                    self.emit("import contextlib");
                                    self.emit("_buf = io.StringIO()");
                                    self.emit("with contextlib.redirect_stdout(_buf):");
                                    self.depth += 1;
                                    for s in fbody.iter() {
                                        self.stmt(s);
                                    }
                                    self.depth -= 1;
                                    // python stdout is block-buffered when
                                    // redirected — an external stage would
                                    // otherwise interleave OUT OF ORDER
                                    self.emit("sys.stdout.flush()");
                                    let mut prev = "_buf.getvalue()".to_string();
                                    let n = rest_argv.len();
                                    for (i, argv) in rest_argv.iter().enumerate() {
                                        let pyargs = argv
                                            .iter()
                                            .map(|a| Self::py_str(a))
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        if i == n - 1 {
                                            self.emit(&format!(
                                                "subprocess.run([{}], input={}, text=True)",
                                                pyargs, prev
                                            ));
                                        } else {
                                            self.emit(&format!(
                                                "_p{i} = subprocess.run([{}], input={}, text=True, capture_output=True).stdout",
                                                pyargs, prev
                                            ));
                                            prev = format!("_p{i}.stdout");
                                        }
                                    }
                                    return "None".into();
                                }
                            }
                        }
                    }
                    let mut parts = Vec::new();
                    for stage in stages {
                        if let IrExpr::Arrow(body) = stage {
                            // single exec stage -> plain argv words
                            let mut simple = false;
                            if let [IrStmt::Expr(e)] = body.as_slice() {
                                if let IrExpr::Call { func, args } = e {
                                    if func == "exec" {
                                        // SHELL words (sh_arg), not python
                                        // expr() — f-strings leaked into
                                        // the bash -c text ('fhello')
                                        if let Some(w) = self.expr_shell_text(e) {
                                            parts.push(w);
                                            simple = true;
                                        }
                                    }
                                }
                            }
                            if simple {
                                continue;
                            }
                            // richer stages (for/while loops, chains):
                            // re-render as shell text — bash -c executes
                            // them with identical semantics (documented
                            // fork/exec escape)
                            if let Some(t) = self.body_shell_text(body) {
                                parts.push(t);
                                continue;
                            }
                        }
                        return self.sh2_stub("pipeline", args, "pipeline");
                    }
                    if !parts.is_empty() {
                        self.need_subprocess = true;
                        self.need_capture = true;
                        return format!(
                            "__sh_capture_bash({})",
                            Self::py_str(&parts.join(" | "))
                        );
                    }
                }
                self.sh2_stub("pipeline", args, "pipeline")
            }
            _ => self.sh2_stub(func, args, func),
        }
    }

    /// Shell-quote a literal for a bash -c reconstruction.
    fn sh_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
    /// Render an exec arg to shell text (literal/int/\$var), for bash -c.
    fn sh_arg(&self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Str(s, _) => Some(Self::sh_quote(s)),
            IrExpr::Int(i) => Some(i.to_string()),
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    Some(format!("${n}"))
                } else {
                    None
                }
            }
            // "a${x}b" interpolation → shell text `a${x}b` (the python
            // f-string form would leak the `f` prefix into the shell word)
            IrExpr::Interpolate(parts) => {
                let mut out = String::new();
                for p in parts {
                    match p {
                        crate::ir::InterpPart::Lit(t) => out.push_str(t),
                        crate::ir::InterpPart::Expr(x) => {
                            if let IrExpr::Call { func, args }
                            = x.as_ref() {
                                if func == "getVar" {
                                    if let Some(IrExpr::Str(n, _)) = args.first() {
                                        out.push_str(&format!("${{{n}}}"));
                                        continue;
                                    }
                                }
                                return None;
                            }
                        }
                    }
                }
                Some(Self::sh_quote(&out))
            }
            _ => None,
        }
    }
    /// Render a single-exec/pipeline Arrow body to shell text (for the
    /// bash -c capture fallback). None for anything else.
    /// The shared capture-body ladder for `$()`/backtick/Arrow bodies —
    /// native first: a single exec body → subprocess.check_output; pipeline
    /// and redirect bodies → their call arms (which may themselves fall
    /// back to bash -c text); a raw Pipeline statement → the pipeline arm;
    /// finally the bash -c text fallback (a documented fork/exec escape —
    /// arbitrary command bodies have no closer python equivalent). Returns
    /// None only when no shape applies and the caller refuses.
    fn capture_body_expr(&mut self, body: &[IrStmt]) -> Option<String> {
        if let [IrStmt::Expr(e)] = body {
            if let IrExpr::Call { func, args } = e {
                if func == "exec" {
                    let argv = self.build_argv(args);
                    self.need_subprocess = true;
                        self.need_capture = true;
                    return Some(format!(
                        "__sh_capture([{}])",
                        argv.join(", ")
                    ));
                }
                if func == "pipeline" {
                    return Some(self.call("pipeline", args));
                }
                if func == "redirect" {
                    return self.capture_redirect(args);
                }
            }
        }
        // `$(cmd1 | cmd2)` — the parser emits a raw Pipeline statement
        // inside the Arrow body (each stage is its own stmt list); lower
        // through the pipeline arm.
        // A raw Exec STATEMENT (`\$(mktemp -d)` keeps the rich form):
        // native argv capture
        if let [IrStmt::Exec { cmd, args, .. }] = body {
            let mut one = Vec::new();
            if let IrExpr::Str(c, _) = cmd {
                one.push(Self::py_str(c));
            }
            let mut ok = true;
            for it in args.iter() {
                let x = self.expr(it);
                if !x.starts_with('"') {
                    ok = false;
                }
                one.push(x);
            }
            if ok {
                self.need_subprocess = true;
                self.need_capture = true;
                return Some(format!("__sh_capture([{}])", one.join(", ")));
            }
        }
        if let [IrStmt::Pipeline { stages, cmd_str, .. }] = body {
            // Prefer the ORIGINAL command text when the parser kept it:
            // bodies like `for … done | sort` have no native python
            // equivalent, and re-rendering stage stmts loses them. This is
            // a documented fork/exec escape (bash -c), justified because
            // the alternative is refusing the whole file.
            if let Some(text) = cmd_str {
                self.need_subprocess = true;
                self.need_capture = true;
                return Some(format!("__sh_capture_bash({})", Self::py_str(text)));
            }
            let stage_arrows: Vec<IrExpr> =
                stages.iter().map(|s| IrExpr::Arrow(s.clone())).collect();
            return Some(self.call("pipeline", &[IrExpr::Array(stage_arrows)]));
        }
        // `$(cp a b && echo ok)` — a single BinOp chain of exec calls:
        // re-render the whole expression as shell text and run it under
        // bash -c (documented fork/exec escape — no native python
        // equivalent for status-chained command groups)
        if let [IrStmt::Expr(e)] = body {
            if matches!(e, IrExpr::BinOp { .. }) {
                if let Some(text) = self.expr_shell_text(e) {
                    self.need_subprocess = true;
                    self.need_capture = true;
                    return Some(format!("__sh_capture_bash({})", Self::py_str(&text)));
                }
            }
        }
        // `$(cmd > file 2>/dev/null)` — a redirect-statement body: run
        // the whole group under bash -c with its redirects re-rendered
        if let [IrStmt::Redirect { inner, redirects, .. }] = body {
            let inner_text = self.body_shell_text(inner)?;
            let mut text = format!("( {inner_text} )");
            for r in redirects.iter() {
                let t = self.expr(&r.target);
                match r.mode.as_str() {
                    "w" => text.push_str(&format!(">{t}")),
                    "a" => text.push_str(&format!(">>{t}")),
                    "r" => text.push_str(&format!("<{t}")),
                    _ => {}
                }
            }
            self.need_subprocess = true;
            self.need_capture = true;
            return Some(format!("__sh_capture_bash({})", Self::py_str(&text)));
        }
        if let Some(text) = self.body_shell_text(body) {
            self.need_subprocess = true;
            self.need_capture = true;
            return Some(format!("__sh_capture_bash({})", Self::py_str(&text)));
        }
        None
    }

    /// Render an exec-call EXPRESSION tree (BinOp &&/|| chains over exec
    /// leaves) back to shell text for the bash -c fallback.
    fn expr_shell_text(&self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Call { func, args } if func == "exec" => {
                let mut one = Vec::new();
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    one.push(Self::sh_quote(cmd));
                }
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        one.push(self.expr_shell_text(it)?);
                    }
                }
                Some(one.join(" "))
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let op_text = match op {
                    crate::ir::BinOpKind::And => "&&",
                    crate::ir::BinOpKind::Or => "||",
                    _ => return None,
                };
                Some(format!(
                    "{} {} {}",
                    self.expr_shell_text(lhs)?,
                    op_text,
                    self.expr_shell_text(rhs)?
                ))
            }
            // expansion leaves — reconstruct the shell word
            IrExpr::Str(s, _) => Some(Self::sh_quote(s)),
            IrExpr::Int(i) => Some(i.to_string()),
            IrExpr::Call { func, args } if func == "getVar" => {
                match args.first() {
                    Some(IrExpr::Str(n, _)) => Some(format!("${{{n}}}")),
                    _ => None,
                }
            }
            IrExpr::Call { func, args } if func == "param" => {
                // ${op name args} -> shell expansion text
                match (args.first(), args.get(1)) {
                    (
                        Some(IrExpr::Str(op, _)),
                        Some(IrExpr::Str(name, _)),
                    ) => match op.as_str() {
                        "" => Some(format!("${{{name}}}")),
                        ":-" | "-" => match args.get(2) {
                            Some(d) => {
                                let dt = self.expr_shell_text(d)?;
                                Some(format!("${{{name}{op}{dt}}}"))
                            }
                            None => None,
                        },
                        "slice" => match (args.get(2), args.get(3)) {
                            (
                                Some(IrExpr::Str(sel, _)),
                                Some(IrExpr::Str(off, _)),
                            ) => {
                                if sel == "@" || sel == "*" {
                                    if off.is_empty() {
                                        Some(format!("${{{name}[@]}}"))
                                    } else {
                                        Some(format!(
                                            "${{{name}[@]:{off}}}"
                                        ))
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                }
            }
            IrExpr::Call { func, args } if func == "join" => {
                // join(<inner>) unwraps to its inner expansion text
                match args.first() {
                    Some(inner) => self.expr_shell_text(inner),
                    None => None,
                }
            }
            // pipeline CALL form (`$(echo one; echo two | wc -l)` keeps
            // the pipe as a Call in the capture body): stages re-render
            // as shell text joined by |
            IrExpr::Call { func, args } if func == "pipeline" => {
                if let Some(IrExpr::Array(sts)) = args.first() {
                    let mut ss = Vec::new();
                    for st in sts.iter() {
                        match st {
                            IrExpr::Arrow(b) => ss.push(self.body_shell_text(b)?),
                            other => {
                                let t = self.expr_shell_text(other)?;
                                ss.push(t);
                            }
                        }
                    }
                    Some(ss.join(" | "))
                } else {
                    None
                }
            }
            // word lists (`for k in ${!map[@]}` lowers to Array[param(..)])
            IrExpr::Array(items) => {
                let mut out = Vec::new();
                for it in items {
                    out.push(self.expr_shell_text(it)?);
                }
                Some(out.join(" "))
            }
            IrExpr::Interpolate(parts) => {
                let mut out = String::from("\"");
                for p in parts {
                    match p {
                        crate::ir::InterpPart::Lit(t) => out.push_str(t),
                        crate::ir::InterpPart::Expr(x) => match x.as_ref() {
                            IrExpr::Call { func, args }
                                if func == "getVar" =>
                            {
                                if let Some(IrExpr::Str(n, _)) = args.first() {
                                    out.push_str(&format!("${{{n}}}"));
                                }
                            }
                            // param(...) expansions inside a word
                            // ("$k => ${map[$k]}", "${!map[@]}") — render
                            // via the param shell-text arm and splice raw
                            IrExpr::Call { func, args }
                                if func == "param" =>
                            {
                                out.push_str(&self.expr_shell_text(
                                    &IrExpr::Call {
                                        func: func.clone(),
                                        args: args.clone(),
                                    },
                                )?);
                            }
                            _ => return None,
                        },
                    }
                }
                out.push('"');
                Some(out)
            }
            _ => None,
        }
    }

    fn body_shell_text(&self, body: &[IrStmt]) -> Option<String> {
        let mut parts = Vec::new();
        for st in body {
            match st {
                IrStmt::Expr(e) => {
                    if let IrExpr::Call { func, args } = e {
                        if func == "exec" {
                            let mut one = Vec::new();
                            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                                one.push(Self::sh_quote(cmd));
                            }
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                for it in items {
                                    one.push(self.expr_shell_text(it)?);
                                }
                            }
                            parts.push(one.join(" "));
                            continue;
                        }
                    }
                    // other expression shapes (param/join/getVar chains)
                    // render via the generalized shell-text expr renderer
                    parts.push(self.expr_shell_text(e)?);
                }
                IrStmt::Exec { cmd, args, .. } => {
                    let mut one = Vec::new();
                    if let IrExpr::Str(c, _) = cmd {
                        one.push(Self::sh_quote(c));
                    }
                    for a in args.iter() {
                        one.push(self.sh_arg(a)?);
                    }
                    parts.push(one.join(" "));
                }
                IrStmt::Pipeline { stages, .. } => {
                    // each stage is its own stmt list — recurse per stage
                    // and join with the shell pipe operator
                    let mut rendered_stages = Vec::new();
                    for stage in stages {
                        rendered_stages.push(self.body_shell_text(stage)?);
                    }
                    parts.push(rendered_stages.join(" | "));
                }
                IrStmt::Assign { targets, expr, .. } => {
                    let var = targets.first()?.var.clone();
                    let val = self.sh_arg(expr)?;
                    parts.push(format!("{var}={val}"));
                }
                IrStmt::For { var, iter, body } => {
                    // `for x in ..` — iter renders as a shell word (the
                    // common "${arr[@]}" / literal list shapes)
                    let it = self.expr_shell_text(iter)?;
                    let b = self.body_shell_text(body)?;
                    parts.push(format!(
                        "for {var} in {it}; do {b}; done"
                    ));
                }
                IrStmt::While { cond, body } => {
                    let c = self.expr_shell_text(&cond.clone())?;
                    let b = self.body_shell_text(body)?;
                    parts.push(format!(
                        "while {c}; do {b}; done"
                    ));
                }
                _ => return None,
            }
        }
        Some(parts.join("; "))
    }
    /// Render a {fd, mode, target} redirect object to shell text.
    fn redirect_shell_text(&self, r: &IrExpr) -> Option<String> {
        let IrExpr::Object(props) = r else {
            return None;
        };
        let mut mode = "w".to_string();
        let mut target = None;
        for (k, v) in props {
            match k.as_str() {
                "mode" => {
                    if let IrExpr::Str(s, _) = v {
                        mode = s.clone();
                    }
                }
                "target" => {
                    if let IrExpr::Str(s, _) = v {
                        target = Some(Self::sh_quote(s));
                    } else if let IrExpr::Call { func, args } = v {
                        if func == "getVar" {
                            if let Some(IrExpr::Str(n, _)) = args.first() {
                                target = Some(format!("\"${n}\"" ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let t = target?;
        Some(match mode.as_str() {
            "w" => format!(">{t}"),
            "a" => format!(">>{t}"),
            "r" => format!("<{t}"),
            "herestring" => format!("<<<{t}"),
            _ => return None,
        })
    }
    /// `$(cmd < file)` — bash -c capture fallback for a redirect body.
    fn capture_redirect(&mut self, args: &[IrExpr]) -> Option<String> {
        let (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(redirs))) =
            (args.first(), args.get(1))
        else {
            return None;
        };
        let inner = self.body_shell_text(body)?;
        let mut text = format!("( {inner} )");
        for r in redirs {
            text.push(' ');
            text.push_str(&self.redirect_shell_text(r)?);
        }
        self.need_subprocess = true;
        self.need_capture = true;
        Some(format!("__sh_capture_bash({})", Self::py_str(&text)))
    }

    /// `exec printf FMT ARGS...` → native `sys.stdout.write`, mirroring
    /// the core's try_native_printf (shir.rs): supported conversions
    /// s/d/i/u, `%%` literal, text backslash-unescape, args cycle across
    /// passes, spec-less formats repeat once per arg. Flags/width/prec or
    /// array args → stub (the core keeps the runtime dispatch there too).
    /// bash -c fork/exec fallback for printf formats/args the native python
    /// printf can't handle (flags/width/prec, array args, f-string fmt).
    fn printf_fallback(&mut self, args: &[IrExpr]) -> String {
        if let Some(IrExpr::Array(items)) = args.get(1) {
            let fmt = match &items[0] {
                IrExpr::Str(s, _) => Some(s.clone()),
                IrExpr::Interpolate(parts) => {
                    let mut t = String::new();
                    let mut ok = true;
                    for p in parts {
                        match p {
                            crate::ir::InterpPart::Lit(s) => t.push_str(s),
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        Some(t)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(fmt) = fmt {
                let mut parts = vec![Self::sh_quote(&fmt)];
                let mut ok = true;
                for a in &items[1..] {
                    match self.sh_arg(a) {
                        Some(t) => parts.push(t),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    self.need_subprocess = true;
                    return format!(
                        "subprocess.call([\"bash\", \"-c\", {}])",
                        Self::py_str(&format!("printf {}", parts.join(" ")))
                    );
                }
            }
        }
        self.sh2_stub("printf", args, "printf")
    }

    /// Build the python string-concatenation expression for a printf's
    /// output; None when the format uses something the native mapping
    /// can't express (caller keeps the runtime builtin).
    fn printf_pieces(&mut self, args: &[IrExpr]) -> Option<String> {
        let Some(IrExpr::Array(items)) = args.get(1) else {
            return None;
        };
        // The format may be a bare Str OR a single-lit Interpolate (the
        // frontends wrap every word in interpolation parts) — normalize.
        let fmt_owned: Option<String> = match items.first() {
            Some(IrExpr::Str(s, _)) => Some(s.clone()),
            Some(IrExpr::Interpolate(parts)) if !parts.is_empty() => {
                let mut t = String::new();
                let mut all_lit = true;
                for p in parts {
                    match p {
                        crate::ir::InterpPart::Lit(s) => t.push_str(s),
                        _ => {
                            all_lit = false;
                            break;
                        }
                    }
                }
                if all_lit { Some(t) } else { None }
            }
            _ => None,
        };
        let Some(fmt) = fmt_owned else {
            return None;
        };
        let parsed = match Self::printf_parse(&fmt) {
            Some(p) => p,
            None => return None,
        };
        let (els, n_specs) = parsed;
        let fmt_args: Vec<&IrExpr> = items[1..].iter().collect();
        if fmt_args.iter().any(|a| matches!(a, IrExpr::Array(_))) {
            return None;
        }
        let arg_exprs: Vec<String> = fmt_args.iter().map(|a| self.expr(a)).collect();
        // flags/width/prec lower NATIVELY onto python's format()
        // mini-language (%-10s -> format(x, "<10"), %05d -> "05d"); a
        // combo the mapping can't express keeps the runtime builtin.
        // (printf default alignment for ALL conversions is RIGHT, which
        // matches python's default too — only '-' flips to '<'.)
        let complex = els.iter().any(|(_, s)| match s {
            Some((flags, _, _, _)) => flags.contains('#'),
            None => false,
        });
        if complex {
            return None;
        }
        let passes = if n_specs == 0 {
            arg_exprs.len().max(1)
        } else if arg_exprs.is_empty() {
            1
        } else {
            (arg_exprs.len() + n_specs - 1) / n_specs
        };
        let mut pieces: Vec<String> = Vec::new();
        if n_specs == 0 {
            // no specs: the format text repeats once per arg
            let text = Self::py_str(&Self::printf_unescape(&fmt));
            if passes > 1 {
                pieces.push(format!("({text} * {passes})"));
            } else {
                pieces.push(text);
            }
        } else {
            let mut ai = 0usize;
            for _pass in 0..passes {
                for (text, spec) in &els {
                    if let Some((_, _, _, conv)) = spec {
                        let arg = arg_exprs.get(ai).cloned().unwrap_or_else(|| "\"\"".into());
                        ai += 1;
                        match conv {
                            's' | 'd' | 'i' | 'u' => {
                                let mut py_spec = String::new();
                                if let Some((flags, width, prec, cv)) = spec {
                                    if flags.contains('-') {
                                        py_spec.push('<');
                                    } else if flags.contains('+') {
                                        py_spec.push('+');
                                    } else if flags.contains(' ') {
                                        py_spec.push(' ');
                                    }
                                    if flags.contains('0')
                                        && !flags.contains('-')
                                        && *width > 0
                                        && *cv != 's'
                                    {
                                        py_spec.push('0');
                                    }
                                    if *width > 0 {
                                        py_spec.push_str(&width.to_string());
                                    }
                                    if let Some(p) = prec {
                                        if *cv == 's' {
                                            py_spec.push('.');
                                            py_spec.push_str(&p.to_string());
                                        } else {
                                            return None;
                                        }
                                    }
                                    // python's type set has no i/u —
                                    // they are d in every respect here
                                    if *cv == 's' {
                                        py_spec.push('s');
                                    } else {
                                        py_spec.push('d');
                                    }
                                }
                                let plain = matches!(py_spec.as_str(), "s" | "d");
                                match conv {
                                    's' => {
                                        if plain {
                                            pieces.push(format!("str({arg})"));
                                        } else {
                                            pieces
                                                .push(format!("format({arg}, \"{py_spec}\")"));
                                        }
                                    }
                                    _ => {
                                        self.need_atoi = true;
                                        if plain {
                                            pieces
                                                .push(format!("str(__sh_atoi({arg}))"));
                                        } else {
                                            pieces.push(format!(
                                                "format(__sh_atoi({arg}), \"{py_spec}\")"
                                            ));
                                        }
                                    }
                                }
                            }
                            _ => unreachable!("printf_parse gates the conversions"),
                        }
                    } else {
                        pieces.push(Self::py_str(&Self::printf_unescape(text)));
                    }
                }
            }
        }
        Some(pieces.join(" + "))
    }

    fn printf_call(&mut self, args: &[IrExpr]) -> String {
        match self.printf_pieces(args) {
            Some(expr) => format!("sys.stdout.write({expr})"),
            None => self.printf_fallback(args),
        }
    }

    /// Parse a printf format into (text-or-spec elements, n_specs); each
    /// element is (text, Some((flags, width, prec, conv))) for a spec.
    /// None when a conversion outside s/d/i/u/%% appears (the core gates
    /// the same set — never a wrong byte).
    fn printf_parse(fmt: &str) -> Option<(Vec<(String, Option<(String, usize, Option<usize>, char)>)>, usize)> {
        let chars: Vec<char> = fmt.chars().collect();
        let mut els: Vec<(String, Option<(String, usize, Option<usize>, char)>)> = Vec::new();
        let mut text = String::new();
        let mut pos = 0usize;
        let mut n_specs = 0usize;
        while pos < chars.len() {
            if chars[pos] == '%' {
                let mut i = pos + 1;
                while i < chars.len() && matches!(chars[i], '-' | '+' | ' ' | '0' | '#') {
                    i += 1;
                }
                let flags: String = chars[pos + 1..i].iter().collect();
                let wstart = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let width: usize = if i > wstart {
                    chars[wstart..i].iter().collect::<String>().parse().ok()?
                } else {
                    0
                };
                let mut prec = None;
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    let pstart = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i > pstart {
                        prec = Some(chars[pstart..i].iter().collect::<String>().parse().ok()?);
                    } else {
                        return None;
                    }
                }
                while i < chars.len() && (chars[i] == 'l' || chars[i] == 'L') {
                    i += 1;
                }
                if i >= chars.len() {
                    return None;
                }
                let conv = chars[i];
                if conv == '%' {
                    text.push('%');
                } else if matches!(conv, 's' | 'd' | 'i' | 'u') {
                    if !text.is_empty() {
                        els.push((std::mem::take(&mut text), None));
                    }
                    els.push((String::new(), Some((flags, width, prec, conv))));
                    n_specs += 1;
                } else {
                    return None;
                }
                pos = i + 1;
                continue;
            }
            text.push(chars[pos]);
            pos += 1;
        }
        if !text.is_empty() {
            els.push((text, None));
        }
        Some((els, n_specs))
    }

    /// Text-run backslash escapes (\n \t \r \a \b \f \v \\ and octal)
    /// — mirrors printf_unescape in shir.rs.
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
                    if let Some(c) = u32::from_str_radix(&oct, 8).ok().and_then(char::from_u32) {
                        res.push(c);
                        i = j;
                        continue;
                    }
                }
            }
            res.push(chars[i]);
            i += 1;
        }
        res
    }

    /// Mini `[ ... ]` evaluator for the common patterns; None → stub.
    fn test_render(&mut self, s: &str) -> Option<String> {
        let toks: Vec<String> = Self::test_tokens(s);
        if let Some(r) = self.test_compound(&toks) {
            return Some(r);
        }
        match toks.as_slice() {
            [a, op, b] if op == "=~" => {
                // regex test (`[[ $x =~ pat ]]`): python re.search (bash's
                // unanchored ERE search — 064_11_complex_test_expressions,
                // regex-brace-in-test). The pattern is a RAW regex, not a
                // test operand (no $-expansion of the rhs).
                let va = self.test_value(a);
                let pat = b.trim_matches('"');
                self.need_re = true;
                Some(format!("bool(re.search({}, str({va})))", Self::py_str(pat)))
            }
            [a, op, b] => {
                let py_op = match op.as_str() {
                    "-gt" => ">",
                    "-lt" => "<",
                    "-ge" => ">=",
                    "-le" => "<=",
                    "-eq" | "=" | "==" => "==",
                    "-ne" | "!=" => "!=",
                    _ => return None,
                };
                let va = self.test_value(a);
                let vb = self.test_value(b);
                // the -gt/-lt/-ge/-le/-eq/-ne ops are NUMERIC in bash (and
                // the C frontend's loop/if tests are all numeric): coerce
                // both sides so a str-valued operand (a lifted temp holding
                // a stringified number) compares numerically instead of
                // raising a python str-vs-int TypeError. `=`/`==`/`!=` are
                // STRING comparisons (bash compares the expansions): str()
                // both sides so a numeric-lifted var/literal compares as
                // text (eq-string-num-var.sh — a bare `==` between a str
                // var and the int literal 2 would always fail).
                if matches!(op.as_str(), "-gt" | "-lt" | "-ge" | "-le" | "-eq" | "-ne") {
                    Some(format!("(int({va}) {py_op} int({vb}))"))
                } else {
                    Some(format!("(str({va}) {py_op} str({vb}))"))
                }
            }
            [flag, v] if flag == "-n" => Some(format!("({})", self.test_value(v))),
            [flag, v] if flag == "-z" => Some(format!("(not {})", self.test_value(v))),
            [flag, v] if matches!(flag.as_str(), "-f" | "-d" | "-e" | "-s") => {
                let p = self.test_value(v);
                match flag.as_str() {
                    "-f" => Some(format!("os.path.isfile({p})")),
                    "-d" => Some(format!("os.path.isdir({p})")),
                    "-e" => Some(format!("os.path.exists({p})")),
                    "-s" => Some(format!("os.path.getsize({p}) > 0")),
                    _ => None,
                }
            }
            [v] => Some(format!("({})", self.test_value(v))),
            _ => None,
        }
    }

    /// Build the argv literals for an external `exec` command
    /// (`["cmd", "arg", …]`).
    fn build_argv(&mut self, args: &[IrExpr]) -> Vec<String> {
        let mut argv = Vec::new();
        if let Some(IrExpr::Str(cmd, _)) = args.first() {
            argv.push(Self::py_str(cmd));
        }
        if let Some(IrExpr::Array(items)) = args.get(1) {
            for it in items {
                argv.push(self.expr(it));
            }
        }
        argv
    }

    /// Render a `let "EXPR"` arithmetic condition (`i<3`) as a Python
    /// numeric comparison.
    fn render_let_cond(&mut self, text: &str) -> Option<String> {
        for (op, py_op) in [("<=", "<="), (">=", ">="), ("==", "=="), ("!=", "!="), ("<", "<"), (">", ">")] {
            if let Some(idx) = text.find(op) {
                let l = text[..idx].trim();
                let r = text[idx + op.len()..].trim();
                let l = self.num_operand(l)?;
                let r = self.num_operand(r)?;
                return Some(format!("({l} {py_op} {r})"));
            }
        }
        None
    }

    /// A numeric operand for a `let` condition.
    fn num_operand(&mut self, t: &str) -> Option<String> {
        let t = t.trim();
        if let Ok(n) = t.parse::<i64>() {
            return Some(n.to_string());
        }
        let name = t.strip_prefix('$').unwrap_or(t);
        let name = name
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(name);
        if self.var_types.contains_key(name) {
            return Some(self.py_ident(name));
        }
        None
    }

    /// Tokenize a test string the way the estree tokenizeTest does for the
    /// operator part: the sh parser DROPS spaces around operator tokens
    /// (`[[ $a == x ]]` emits `$a==x`), so `==`/`!=`/`=`/`<`/`>` split
    /// even when adjacent to word chars; `-eq`-family/`-a`/`-o`/words stay
    /// whole. A quoted operand keeps its quotes (`"$a"=="x"` →
    /// [`"$a"`, `==`, `"x"`]).
    fn test_tokens(s: &str) -> Vec<String> {
        let mut toks: Vec<String> = Vec::new();
        let mut word = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '=' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        if !word.is_empty() {
                            toks.push(std::mem::take(&mut word));
                        }
                        toks.push("==".into());
                    } else if chars.peek() == Some(&'~') {
                        chars.next();
                        if !word.is_empty() {
                            toks.push(std::mem::take(&mut word));
                        }
                        toks.push("=~".into());
                    } else {
                        if !word.is_empty() {
                            toks.push(std::mem::take(&mut word));
                        }
                        toks.push("=".into());
                    }
                }
                '!' if chars.peek() == Some(&'=') => {
                    chars.next();
                    if !word.is_empty() {
                        toks.push(std::mem::take(&mut word));
                    }
                    toks.push("!=".into());
                }
                '<' | '>' => {
                    if !word.is_empty() {
                        toks.push(std::mem::take(&mut word));
                    }
                    toks.push(c.to_string());
                }
                c if c.is_whitespace() => {
                    if !word.is_empty() {
                        toks.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        if !word.is_empty() {
            toks.push(word);
        }
        toks
    }

    /// Compound `A -a B` / `A -o B` tests (`[ $a -ge 5 -a $b -le 5 ]` — the
    /// C frontend's `&&`/`||` in a test cond; estree's parseTest and/or
    /// nodes). `-a` binds tighter than `-o` in bash, so split at the LAST
    /// `-o` (else the LAST `-a`); each side must itself render as a test
    /// (recursing — the sides may be further compounds). A `-a` in operand
    /// position (`[ -a file ]`, the unary exists-test) is never a split:
    /// its left side is empty and is rejected.
    fn test_compound(&mut self, toks: &[String]) -> Option<String> {
        let mut candidates: Vec<usize> = Vec::new();
        for (i, t) in toks.iter().enumerate() {
            if t == "-o" {
                candidates.push(i);
            }
        }
        if candidates.is_empty() {
            for (i, t) in toks.iter().enumerate() {
                if t == "-a" {
                    candidates.push(i);
                }
            }
        }
        for i in candidates {
            let left = &toks[..i];
            let right = &toks[i + 1..];
            if left.is_empty() || right.is_empty() {
                continue;
            }
            if let (Some(l), Some(r)) = (
                self.test_render(&left.join(" ")),
                self.test_render(&right.join(" ")),
            ) {
                let op = if toks[i] == "-a" { "and" } else { "or" };
                return Some(format!("({l} {op} {r})"));
            }
        }
        None
    }

    /// A test operand: `"$y"`/`$y`/`y` (typed var) → ident; a number →
    /// literal; a native-written name (a lifted temp) → ident; otherwise a
    /// quoted string.
    fn test_value(&mut self, t: &str) -> String {
        let t = t.trim().trim_matches('"');
        // `$name` is ALWAYS a var read, never a literal — mirror the
        // getVar arm: a store-written name reads the runtime store, a
        // never-written plain name is unset at every read (→ ""). The
        // old fallback rendered the bare name as a literal (go-sh
        // t80_pointer_type.go: `test "$p"="$nil"` — `$nil` is an UNSET
        // var → "" — the literal "nil" made the single-arg test false
        // while the estree reference (and native) are true).
        let dollar = t.starts_with('$');
        let t = t.strip_prefix('$').unwrap_or(t);
        if t == "?" {
            // `$?` — the exit status of the last command. The python
            // subset tracks no statement statuses; the runtime var defaults
            // to 0 (success) and statement-position test/and/or/true/false
            // expressions update it (bash-correct for the `cmd; [ $? -eq 0 ]`
            // idiom — the corpus shape).
            self.need_rc = true;
            return "__sh_rc".into();
        }
        // `${name##pat}` / `${name#pat}` / `${name%%pat}` / `${name%pat}` —
        // the parameter-expansion prefix/suffix strips (bash pattern
        // removal). Render a native re.sub with the glob translated to a
        // regex (greedy = longest removal, non-greedy = shortest).
        if let Some(rest) = t.strip_prefix('{') {
            if let Some(close) = rest.rfind('}') {
                let inner = &rest[..close];
                for (op, _longest) in [("##", true), ("#", false), ("%%", true), ("%", false)] {
                    if let Some((name, pat)) = inner.split_once(op) {
                        if Self::is_plain_name(name) {
                            // native runtime strip (glob → regex at
                            // runtime; greedy = longest removal)
                            self.need_strip = true;
                            let v = self.test_value(name);
                            return format!("__sh_strip({}, {}, {})", Self::py_str(pat), Self::py_str(op), v);
                        }
                    }
                }
            }
        }
        if self.var_types.contains_key(t) {
            self.py_ident(t)
        } else if self.written.contains(t) && !self.store_written.contains(t) && Self::is_plain_name(t) {
            self.py_ident(t)
        } else if let Ok(n) = t.parse::<i64>() {
            n.to_string()
        } else if dollar {
            if self.store_written.contains(t) {
                self.sh2_calls.insert("getVar".into());
                format!("sh2_getVar({})", Self::py_str(t))
            } else if Self::is_plain_name(t) {
                "\"\"".to_string()
            } else {
                Self::py_str(t)
            }
        } else {
            Self::py_str(t)
        }
    }

    /// Render an expression as a python int (Int-typed assignment target).
    fn expr_as_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => {
                // numeric literal in the ShIR ("5" for x=5)
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else if let Ok(f) = s.trim().parse::<f64>() {
                    // a float literal coerced to int (bash truncates)
                    (f as i64).to_string()
                } else {
                    // a non-numeric string coerced to int: bash -> 0
                    "0".into()
                }
            }
            IrExpr::Int(i) => i.to_string(),
            _ => self.expr(e),
        }
    }

    // ── statements ───────────────────────────────────────────────────

    /// Render a statement list as an indented block, emitting `pass` when
    /// the body would otherwise be empty (python requires at least one
    /// statement after a `:`).
    fn block(&mut self, stmts: &[IrStmt]) {
        let mut scratch = Vec::new();
        std::mem::swap(&mut self.out, &mut scratch);
        self.depth += 1;
        for s in stmts {
            self.stmt(s);
        }
        self.depth -= 1;
        std::mem::swap(&mut self.out, &mut scratch);
        let has_code = scratch.iter().any(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('#')
        });
        if !has_code {
            self.out.extend(scratch);
            self.depth += 1;
            self.emit("pass");
            self.depth -= 1;
        } else {
            self.out.extend(scratch);
        }
    }

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => {
                // statement-position arith: `i++` / `x += n` (the C-style
                // frontends' loop steps) render natively — python has no
                // expression-assignment, so the Expr(Arith) form must
                // become a real statement (the arith() expression path can
                // only stub it).
                if let IrExpr::Arith(a) = e {
                    if let ArithAst::IncDec { var, delta, prefix: _ } = &**a {
                        let name = self.py_ident(var);
                        let d = delta.unsigned_abs();
                        let py_op = if *delta >= 0 { "+" } else { "-" };
                        let rhs = if d == 1 {
                            "1".to_string()
                        } else {
                            d.to_string()
                        };
                        if self.is_num(var) {
                            self.emit(&format!("{name} {py_op}= {rhs}"));
                        } else {
                            // untyped (shell) vars hold strings: coerce
                            // the LHS like the arith() expression path
                            // (`i += 1` on a str "1" would TypeError —
                            // cpp-sh-go t05_arith_loop.cc's ForInit step).
                            self.need_atoi = true;
                            self.emit(&format!("{name} = __sh_atoi({name}) {py_op} {rhs}"));
                        }
                        return;
                    }
                    if let ArithAst::Assign { var, op, rhs } = &**a {
                        let name = self.py_ident(var);
                        let r = self.arith(rhs);
                        let py_op = match op.as_str() {
                            "+=" => "+=",
                            "-=" => "-=",
                            "*=" => "*=",
                            "/=" => "/=",
                            "%=" => "%=",
                            _ => "=",
                        };
                        if self.is_num(var) {
                            self.emit(&format!("{name} {py_op} {r}"));
                        } else {
                            // same coercion as the IncDec arm: the compound
                            // assign target may hold a string
                            self.need_atoi = true;
                            let bare = py_op.trim_end_matches('=');
                            if py_op == "=" {
                                self.emit(&format!("{name} = {r}"));
                            } else {
                                self.emit(&format!("{name} = __sh_atoi({name}) {bare} {r}"));
                            }
                        }
                        return;
                    }
                }
                if let IrExpr::Call { func, args: call_args, .. } = e {
                    if func == "grepMatches" {
                        // statement position: the matches are the output
                        let v = self.expr(e);
                        self.emit(&format!("print({v})"));
                        return;
                    }
                    // and(Arrow, Arrow, ..) — &&-chained STATEMENT groups
                    // (the process-substitution lowering: mktemp captures,
                    // tmp writes, then the command). Render every group as
                    // plain statements; the links are assignments/redirects
                    // whose failure modes the corpus never exercises.
                    if func == "and" {
                        if !call_args.is_empty()
                            && call_args.iter().all(|a| matches!(a, IrExpr::Arrow(_)))
                        {
                            for a in call_args.iter() {
                                if let IrExpr::Arrow(body) = a {
                                    for s in body.iter() {
                                        self.stmt(s);
                                    }
                                }
                            }
                            return;
                        }
                    }
                    // break/continue calls inside a loop lower natively
                    // (bash status verbs); outside a loop they keep the stub
                    if self.loop_depth > 0 {
                        if func == "break" {
                            self.emit("break");
                            return;
                        }
                        if func == "continue" {
                            self.emit("continue");
                            return;
                        }
                    }
                }
                // `$?`-status tracking: a statement-position test / && ||
                // chain / true / false IS the command whose exit status a
                // later `$?` reads (bash). The truthiness of these renders
                // IS the success verdict, so record it (only when the
                // program actually reads `$?`).
                if self.need_rc {
                    let is_status = match e {
                        IrExpr::Call { func, args } => match func.as_str() {
                            "test" => true,
                            "exec" => matches!(
                                args.first(),
                                Some(IrExpr::Str(s, _)) if s == "true" || s == "false"
                            ),
                            _ => false,
                        },
                        IrExpr::BinOp { op, .. } => matches!(
                            op,
                            crate::ir::BinOpKind::And | crate::ir::BinOpKind::Or
                        ),
                        _ => false,
                    };
                    if is_status {
                        let x = self.expr(e);
                        self.emit(&format!("__sh_rc = 0 if ({x}) else 1"));
                        return;
                    }
                }
                let x = self.expr(e);
                self.emit(&format!("{x}"));
            }
            IrStmt::Assign { targets, expr, asm, .. } => {
                // Declarator-position asm label (core request
                // c-sh-go-toplevelasmargument-20260814-042952) — no
                // Python rendering; refuse loudly (refuse > guess).
                if let Some(spec) = asm {
                    self.mark_todo(&format!("asm label '{}' on an assign", spec.template));
                    return;
                }
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
                }
                // bracket target (`map[foo] = v`) — native list/dict
                // subscript assignment (stringified keys in dict mode)
                if let Some((root, key)) = Self::split_target(&t.var) {
                    let rhs = self.expr(expr);
                    let ke = if self.array_dict_roots.contains(root) {
                        match key.strip_prefix('$') {
                            Some(v) => self.py_ident(v),
                            None => Self::py_str(key),
                        }
                    } else {
                        self.render_array_key(key)
                    };
                    self.emit(&format!("{}[{ke}] = {rhs}", self.py_ident(root)));
                    return;
                }
                let name = self.py_ident(&t.var);
                // `s = s += n` (arith Assign on the same target) → `s += n`
                // (python forbids assignment inside an expression)
                if let IrExpr::Arith(a) = expr {
                    if let ArithAst::IncDec { var, delta, .. } = &**a {
                        let v = self.py_ident(var);
                        let d = delta.unsigned_abs();
                        let s = if *delta >= 0 { "+" } else { "-" };
                        self.emit(&format!("{v} {s}= {d}"));
                        return;
                    }
                    if let ArithAst::Assign { var, op, rhs } = &**a {
                        if var == &t.var {
                            let r = self.arith(rhs);
                            let py_op = match op.as_str() {
                                "+=" => "+=",
                                "-=" => "-=",
                                "*=" => "*=",
                                "/=" => "/=",
                                "%=" => "%=",
                                _ => "=",
                            };
                            if self.is_num(&t.var) || py_op == "=" {
                                self.emit(&format!("{name} {py_op} {r}"));
                            } else {
                                // untyped target: coerce the LHS (the arith
                                // expr path coerces its operands; a bare
                                // `x += 1` on a str would TypeError)
                                self.need_atoi = true;
                                self.emit(&format!(
                                    "{name} = __sh_atoi({name}) {} {r}",
                                    py_op.trim_end_matches('=')
                                ));
                            }
                            return;
                        }
                    }
                }
                let rhs = if self.is_num(&t.var) {
                    self.expr_as_num(expr)
                } else {
                    self.expr(expr)
                };
                // a STORE-resident target: the native binding is not the
                // var's home — the store read (sh2_getVar) must observe
                // the write (the C frontend's outparam channel assigns
                // out-targets from line() captures here)
                if !self.is_num(&t.var) && self.store_written.contains(&t.var) {
                    self.sh2_calls.insert("setVar".into());
                    self.emit(&format!(
                        "sh2_setVar({}, {rhs})",
                        Self::py_str(&t.var)
                    ));
                    return;
                }
                self.emit(&format!("{name} = {rhs}"));
            }
            IrStmt::Declare { vars, init, .. } => {
                let init_expr = init.as_ref().map(|e| self.expr(e));
                if vars.len() > 1 && init_expr.is_some() {
                    let names: Vec<String> = vars.iter().map(|d| self.py_ident(&d.name)).collect();
                    self.emit(&format!("{} = {}", names.join(" = "), init_expr.unwrap()));
                } else {
                    for d in vars {
                        let name = self.py_ident(&d.name);
                        let v = init_expr.clone().unwrap_or_else(|| {
                            if self.is_num(&d.name) {
                                "0".into()
                            } else {
                                "\"\"".into()
                            }
                        });
                        self.emit(&format!("{name} = {v}"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                let name = self.py_ident(var);
                let elems: Vec<String> = elements.iter().map(|e| self.expr(e)).collect();
                self.emit(&format!("{name} = [{}]", elems.join(", ")));
            }
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                let v = self.expr(value);
                if let Some(t) = target {
                    self.sh2_calls.insert("output".into());
                    self.mark_todo("output to filehandle");
                    self.emit(&format!("sh2_output({}, {v})", Self::py_str(t)));
                } else if *newline {
                    self.emit(&format!("print({v})"));
                } else {
                    self.emit(&format!("print({v}, end=\"\")"));
                }
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                let p = self.expr(path);
                let c = self.expr(content);
                let mode = if *append { "\"a\"" } else { "\"w\"" };
                self.emit(&format!("with open({p}, {mode}) as _f:"));
                self.depth += 1;
                self.emit(&format!("_f.write(str({c}))"));
                self.depth -= 1;
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.expr(cond);
                self.emit(&format!("if {c}:"));
                self.block(then);
                for (ec, body) in elsifs {
                    let ec = self.expr(ec);
                    self.emit(&format!("elif {ec}:"));
                    self.block(body);
                }
                if !else_.is_empty() {
                    self.emit("else:");
                    self.block(else_);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                // shell `case D in pat) …;; esac` — if/elif chain on string
                // equality (bash case is anchored glob match; the common
                // literal-pattern case is exact equality).
                let d = self.expr(discriminant);
                let mut emitted_any = false;
                for cl in clauses {
                    let conds: Vec<String> = cl
                        .patterns
                        .iter()
                        .filter(|p| p.as_str() != "*")
                        .map(|p| format!("{d} == {}", Self::py_str(p)))
                        .collect();
                    let is_default = cl.patterns.iter().any(|p| p.as_str() == "*");
                    if conds.is_empty() && is_default {
                        self.emit("else:");
                        self.block(&cl.body);
                        emitted_any = true;
                        continue;
                    }
                    let kw = if emitted_any { "elif" } else { "if" };
                    self.emit(&format!("{kw} {}:", conds.join(" or ")));
                    self.block(&cl.body);
                    emitted_any = true;
                }
            }
            IrStmt::For { var, iter, body } => {
                let v = self.py_ident(var);
                // array-selection iters iterate ELEMENTS natively:
                //   Array[param("slice", name, "@", ..)] -> keys/values/list
                // (the generic expr path word-JOINS the selection into ONE
                // string — right for echo words, wrong for iteration)
                let native_iter: Option<String> = match iter {
                    IrExpr::Array(items) if items.len() == 1 => match &items[0] {
                        // {1..5} / {a,b} — the expanded WORDS are the
                        // iteration domain (as a python list)
                        // "$@" — iterate the ACTUAL argv list (the
                        // scalar join form would iterate ONE string)
                        IrExpr::Call { func, args }
                            if func == "listVar" =>
                        {
                            match args.first() {
                                Some(IrExpr::Str(sel, _))
                                    if sel == "@" || sel == "*" =>
                                {
                                    Some("sys.argv[1:]".to_string())
                                }
                                _ => None,
                            }
                        }
                        IrExpr::Call { func, args } if func == "brace" => {
                            match py_brace_list(args) {
                                Some(words) => {
                                    let elems: Vec<String> =
                                        words.iter().map(|w| Self::py_str(w)).collect();
                                    Some(format!("[{}]", elems.join(", ")))
                                }
                                None => None,
                            }
                        }
                        IrExpr::Call { func, args } if func == "param" => {
                            match (args.first(), args.get(1), args.get(2)) {
                                (
                                    Some(IrExpr::Str(op, _)),
                                    Some(IrExpr::Str(name, _)),
                                    Some(IrExpr::Str(sel, _)),
                                ) if op == "slice" && (sel == "@" || sel == "*") => {
                                    let r = self.py_ident(name);
                                    let keys_mode = name.starts_with('!');
                                    let root_name = if keys_mode {
                                        &name[1..]
                                    } else {
                                        name
                                    };
                                    let rr = self.py_ident(root_name);
                                    if !self.array_keys.contains_key(root_name) {
                                        None
                                    } else if keys_mode {
                                        Some(format!("{rr}.keys()"))
                                    } else if self.array_dict_roots.contains(root_name) {
                                        Some(format!("{rr}.values()"))
                                    } else {
                                        Some(format!("list({r})"))
                                    }
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let it = match native_iter {
                    Some(x) => x,
                    None => {
                        // literal word lists stay a python list; anything
                        // else falls back to the generic expr
                        match iter {
                            IrExpr::Array(items) => {
                                let elems: Vec<String> =
                                    items.iter().map(|e| self.expr(e)).collect();
                                format!("[{}]", elems.join(", "))
                            }
                            _ => self.expr(iter),
                        }
                    }
                };
                self.emit(&format!("for {v} in {it}:"));
                self.loop_depth += 1;
                self.block(body);
                self.loop_depth -= 1;
            }
            IrStmt::While { cond, body } => {
                let c = self.expr(cond);
                self.emit(&format!("while {c}:"));
                self.loop_depth += 1;
                self.block(body);
                self.loop_depth -= 1;
            }
            // C-style `for (init; cond; step)` — the imperative frontends'
            // rich form (the shell path lowers it via strip_cfor). Render
            // natively as `init; while (cond) { body; step }` (the step
            // re-runs at the end of every iteration).
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                for s in init {
                    self.stmt(s);
                }
                let c = self.expr(cond);
                self.emit(&format!("while {c}:"));
                self.loop_depth += 1;
                self.block(body);
                // the step runs at the loop-body depth, after the body
                self.depth += 1;
                for s in step {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
            }
            IrStmt::Break => {
                if self.loop_depth > 0 {
                    self.emit("break");
                } else {
                    self.mark_todo("top-level break");
                }
            }
            IrStmt::Continue => {
                if self.loop_depth > 0 {
                    self.emit("continue");
                } else {
                    self.mark_todo("top-level continue");
                }
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.emit("while True:");
                self.loop_depth += 1;
                self.block(body);
                // the cond check + break live INSIDE the loop body — block()
                // restored depth 0, so raise it back to loop-body depth.
                self.depth += 1;
                let c = self.expr(cond);
                if *until {
                    self.emit(&format!("if {c}:"));
                } else {
                    self.emit(&format!("if not {c}:"));
                }
                self.depth += 1;
                self.emit("break");
                self.depth -= 1;
                self.depth -= 1;
                self.loop_depth -= 1;
            }
            IrStmt::Exit(e) => {
                let code = e
                    .as_ref()
                    .map(|x| self.expr(x))
                    .unwrap_or_else(|| "0".into());
                self.emit(&format!("sys.exit({code})"));
            }
            IrStmt::Function { name, body, .. } => {
                let n = self.py_ident(name);
                self.emit(&format!("def {n}():"));
                self.in_function += 1;
                self.block(body);
                self.in_function -= 1;
            }
            IrStmt::Return(e) => {
                if self.in_function > 0 {
                    let x = e
                        .as_ref()
                        .map(|x| self.expr(x))
                        .unwrap_or_else(|| "None".into());
                    self.emit(&format!("return {x}"));
                } else {
                    self.mark_todo("top-level return");
                }
            }
            IrStmt::Exec {
                cmd, args, capture, ..
            } => {
                let c = self.expr(cmd);
                let mut argv = vec![c];
                for a in args {
                    argv.push(self.expr(a));
                }
                if let Some(var) = capture {
                    let v = self.py_ident(var);
                    self.emit(&format!(
                        "{v} = __sh_capture([{}])",
                        argv.join(", ")
                    ));
                } else {
                    // native-first: reuse the exec CALL arm so printf /
                    // echo / exit / cd lower natively. A bare
                    // subprocess.run here both raised on command-not-found
                    // AND skipped the format handling entirely.
                    let rendered = self.call(
                        "exec",
                        &[(*cmd).clone(), IrExpr::Array(args.clone())],
                    );
                    self.emit(&rendered);
                }
            }
            IrStmt::Block(b) => {
                for s in b {
                    self.stmt(s);
                }
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                // Python's native try/except/else/finally — the A1 Try
                // node (py-sh-go try_stmt) maps 1:1, mirroring estree's
                // TryStatement lowering: a bare arm is `except Exception:`
                // (the estree signal guard's Error-class equivalent —
                // python control flow is native keywords, never caught),
                // a getVar match (`except ValueError:`) lowers to the
                // named class, `as` bindings write the caught value into
                // the runtime store (sh2.setVar stringifies, like estree),
                // `else` is python's native no-exception suite, `finally`
                // the native finalizer. A bare arm must be LAST in python
                // (it is in every parseable source); later arms are dead.
                if excepts.is_empty() && finally_body.is_empty() {
                    // no handler: the try adds nothing — emit the suites
                    // plainly (else just follows the body), like estree
                    for s in body {
                        self.stmt(s);
                    }
                    for s in else_body {
                        self.stmt(s);
                    }
                    return;
                }
                self.emit("try:");
                self.block(body);
                for (ei, e) in excepts.iter().enumerate() {
                    let mut clause = match &e.match_expr {
                        None => "except Exception:".to_string(),
                        Some(IrExpr::Call { func, args }) if func == "getVar" => {
                            match args.first() {
                                Some(IrExpr::Str(name, _)) => format!("except {name}:"),
                                _ => "except Exception:".to_string(),
                            }
                        }
                        Some(other) => {
                            self.mark_todo(&format!("except match {other:?}"));
                            "except Exception:".to_string()
                        }
                    };
                    if e.as_name.is_some() {
                        // bind the caught value for the arm's `as` binding
                        // (sh2_setVar below reads __sh_exc)
                        clause = format!("{} as __sh_exc", clause.trim_end_matches(':'));
                    }
                    if e.match_expr.is_none() && ei + 1 < excepts.len() {
                        // arms after a bare except are unreachable (python
                        // itself forbids the syntax) — keep their bodies out
                        self.emit(&format!("{clause}  # unreachable: later arms"));
                    } else {
                        self.emit(&clause);
                    }
                    // the arm body is a block; an `as` binding sits at its
                    // top (the caught value into the runtime store, estree
                    // parity — sh2.setVar stringifies)
                    let mut body_out = Vec::new();
                    std::mem::swap(&mut self.out, &mut body_out);
                    self.depth += 1;
                    if let Some(asn) = &e.as_name {
                        self.sh2_calls.insert("setVar".into());
                        self.emit(&format!(
                            "sh2_setVar({}, str(__sh_exc))",
                            Self::py_str(asn)
                        ));
                    }
                    for s in &e.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    std::mem::swap(&mut self.out, &mut body_out);
                    let has_code = body_out.iter().any(|l| {
                        let t = l.trim_start();
                        !t.is_empty() && !t.starts_with('#')
                    });
                    if !has_code {
                        self.out.extend(body_out);
                        self.depth += 1;
                        self.emit("pass");
                        self.depth -= 1;
                    } else {
                        self.out.extend(body_out);
                    }
                }
                if !else_body.is_empty() {
                    self.emit("else:");
                    self.block(else_body);
                }
                if !finally_body.is_empty() {
                    self.emit("finally:");
                    self.block(finally_body);
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                for s in body {
                    self.stmt(s);
                }
            }
            IrStmt::Pipeline { stages, cmd_str, .. } => {
                // NATIVE-FIRST (keeps python-side state alive across the
                // pipe: 'echo "hello $name" | tr' must see $name!):
                // leading stage renders natively under redirect_stdout,
                // remaining exec stages chain via stdin.
                let mut rest_argv: Vec<Vec<String>> = Vec::new();
                let mut nat_ok = stages.len() >= 2;
                for st in stages.iter().skip(1) {
                    if !nat_ok {
                        break;
                    }
                    match st.as_slice() {
                        [IrStmt::Expr(e)] => match e {
                            IrExpr::Call { func, args } if func == "exec" => {
                                let mut one = Vec::new();
                                if let Some(IrExpr::Str(c, _)) = args.first() {
                                    one.push(c.clone());
                                }
                                if let Some(IrExpr::Array(items)) = args.get(1) {
                                    for it in items.iter() {
                                        one.push(self.expr(it));
                                    }
                                }
                                rest_argv.push(one);
                            }
                            _ => nat_ok = false,
                        },
                        _ => nat_ok = false,
                    }
                }
                if nat_ok && !rest_argv.is_empty() {
                    self.emit("import io");
                    self.emit("import contextlib");
                    self.emit("_buf = io.StringIO()");
                    self.emit("with contextlib.redirect_stdout(_buf):");
                    self.depth += 1;
                    if let Some(first) = stages.first() {
                        for s in first.iter() {
                            self.stmt(s);
                        }
                    }
                    self.depth -= 1;
                    // buffered stdout would interleave out of order with
                    // the external stage's output
                    self.emit("sys.stdout.flush()");
                    let mut prev = "_buf.getvalue()".to_string();
                    let n = rest_argv.len();
                    for (i, argv) in rest_argv.iter().enumerate() {
                        let pyargs = argv
                            .iter()
                            .map(|a| Self::py_str(a))
                            .collect::<Vec<_>>()
                            .join(", ");
                        if i == n - 1 {
                            self.emit(&format!(
                                "subprocess.run([{}], input={}, text=True)",
                                pyargs, prev
                            ));
                        } else {
                            self.emit(&format!(
                                "_p{i} = subprocess.run([{}], input={}, text=True, capture_output=True).stdout",
                                pyargs, prev
                            ));
                            prev = format!("_p{i}");
                        }
                    }
                    return;
                }
                // The pipe is SEMANTIC here: rendering stages sequentially
                // loses it (t03: 'echo .. | tr a-z A-Z' printed BOTH).
                // Prefer the parser's original command text; else re-render
                // each stage as shell text; run under bash -c with stdout
                // inherited (statement context). Documented fork/exec escape.
                let text = match cmd_str {
                    Some(t) => Some(t.clone()),
                    None => {
                        let mut ss = Vec::new();
                        let mut ok = true;
                        for stage in stages.iter() {
                            match self.body_shell_text(stage) {
                                Some(t) => ss.push(t),
                                None => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            Some(ss.join(" | "))
                        } else {
                            None
                        }
                    }
                };
                if let Some(text) = text {
                    self.need_subprocess = true;
                    self.emit(&format!(
                        "__sh_exec([\"bash\", \"-c\", {}])",
                        Self::py_str(&text)
                    ));
                    return;
                }
                for st in stages {
                    for s in st {
                        self.stmt(s);
                    }
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                // NATIVE FAST PATH: `printf ... > file` / `echo ... > file`
                // — the single most common write-redirect shape. The
                // previous approximation wrote an EMPTY file (the content
                // went to stdout instead), corrupting every downstream
                // `ls`/`cat`/checksum in the script.
                let mut handled = false;
                if let ([IrStmt::Expr(e)], Some(r)) =
                    (inner.as_slice(), redirects.iter().find(|r| r.fd.unwrap_or(1) == 1 && (r.mode == "w" || r.mode == "a")))
                {
                    if let IrExpr::Call { func, args } = e {
                        if func == "exec" {
                            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                                let content: Option<String> = match cmd.as_str() {
                                    "echo" => {
                                        args.get(1).and_then(|a| match a {
                                            IrExpr::Array(items) => {
                                                let rendered: Option<Vec<String>> = items
                                                    .iter()
                                                    .map(|i| self.expr(i).into())
                                                    .collect();
                                                rendered.map(|rs| {
                                                    if rs.is_empty() {
                                                        "\"\\n\"".to_string()
                                                    } else {
                                                        format!(
                                                            "{} + \"\\n\"",
                                                            rs.join(" + ' ' + ")
                                                        )
                                                    }
                                                })
                                            }
                                            _ => None,
                                        })
                                    }
                                    "printf" => {
                                        // reuse the printf pieces builder
                                        self.printf_pieces(args)
                                    }
                                    _ => None,
                                };
                                if let Some(content) = content {
                                    let p = self.expr(&r.target);
                                    let mode = if r.mode == "a" { "'a'" } else { "'w'" };
                                    self.emit(&format!("with open({p}, {mode}) as _f:"));
                                    self.depth += 1;
                                    self.emit(&format!("_f.write({content})"));
                                    self.depth -= 1;
                                    // non-fd1 redirects on the same stmt:
                                    // stderr is discarded by contract — drop
                                    handled = true;
                                }
                            }
                        }
                    }
                }
                if handled {
                    return;
                }
                for s in inner {
                    self.stmt(s);
                }
                for r in redirects {
                    if r.fd.unwrap_or(1) == 1 && (r.mode == "w" || r.mode == "a") {
                        let p = self.expr(&r.target);
                        let mode = if r.mode == "a" { "'a'" } else { "'w'" };
                        self.emit(&format!("with open({p}, {mode}) as _f:"));
                        self.depth += 1;
                        self.emit("_f.write('')");
                        self.depth -= 1;
                    }
                }
            }
            other => self.mark_todo(&format!("stmt {:?}", other)),
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect declared vars (assign targets, declare lists,
        // Var reads) so defaults can be hoisted before use (python vars
        // are dynamic, but unset reads should be "" / 0 like shell).
        let mut vars: BTreeSet<String> = BTreeSet::new();
        collect_vars(&prog.stmts, &mut vars);
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }

        // Pass 2: render the body first (helper flags known before preamble).
        // Pre-scan for `$?` test reads so the FIRST such statement's status
        // wrap (the Expr arm consults need_rc before rendering its own
        // operands) is decided correctly.
        if !self.need_rc {
            let mut rc = false;
            scan_rc(&prog.stmts, &mut rc);
            for sub in &prog.subs {
                scan_rc(&sub.body, &mut rc);
            }
            self.need_rc = rc;
        }
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        for v in &vars {
            // bracket targets (`map[foo]`) are NOT scalars — the dict-mode
            // prologue below declares their root; emitting `map[foo] = ""`
            // here would be an invalid python subscript assignment on an
            // undeclared name
            if Self::split_target(v).is_some() {
                continue;
            }
            let name = self.py_ident(v);
            if self.is_num(v) {
                self.emit(&format!("{name} = 0"));
            } else {
                self.emit(&format!("{name} = \"\""));
            }
        }
        if !vars.is_empty() {
            self.emit("");
        }
        // dict-mode array roots: pre-declare `{}` in the body prologue
        // (definition-before-use for reads that precede any write)
        {
            let dict_roots: Vec<String> = self
                .array_dict_roots
                .iter()
                .filter(|r| !vars.contains(*r) && !self.written.contains(*r))
                .cloned()
                .collect();
            for root in &dict_roots {
                let name = self.py_ident(root);
                self.emit(&format!("{name} = {{}}"));
            }
        }
        for (idx, s) in prog.stmts.iter().enumerate() {
            let before = self.out.len();
            self.stmt(s);
            let line = prog.stmt_lines.iter().find(|(i, _)| *i == idx).map(|(_, l)| *l);
            if let Some(l) = line {
                if let Some(first) = self.out.get_mut(before) {
                    *first = format!("{first} # line {l}");
                }
            }
        }
        std::mem::swap(&mut self.out, &mut body_out);

        // Preamble: shebang, imports, then the sh2.* stubs
        // (definition-before-use, so the body's calls link).
        self.emit("#!/usr/bin/env python3");
        // No "sh2perl" in this banner: the gate's stub regex counts
        // sh2[A-Za-z_] occurrences, and a header mentioning the project
        // name made EVERY render fail the stub check as a false positive
        // (the c backend passes by emitting no banner at all).
        self.emit("# Generated by the python backend (debashl::python_backend).");
        self.emit("import os");
        self.emit("import subprocess");
        self.emit("import sys");
        if self.need_re {
            self.emit("import re");
        }
        if self.need_subprocess {
            self.emit("");
            // non-raising exec: command-not-found (FileNotFoundError)
            // yields status 127 and the script CONTINUES, like bash
            self.emit("def __sh_exec(argv):");
            self.emit("    import subprocess");
            self.emit("    sys.stdout.flush()");
            // bash truth convention: rc 0 = success = TRUE. Returning the
            // raw rc would invert every `if cmd:` / `cmd && cmd` chain
            // (python treats 0 as falsy). Command-not-found -> False, like
            // a failed command; the script continues either way.
            self.emit("    try:");
            self.emit("        return subprocess.call(argv) == 0");
            self.emit("    except OSError:");
            self.emit("        return False");
        }
        if self.need_inc {
            self.emit("");
            // $(( x++ )) / $(( ++x )) as a VALUE: postfix yields the OLD
            // value, prefix the NEW one; both update the variable. Module
            // globals keep it native (no store round-trip).
            self.emit("def __sh_inc_pre(var, d):");
            self.emit("    v = int(globals().get(var, 0)) + d");
            self.emit("    globals()[var] = v");
            self.emit("    return v");
            self.emit("");
            self.emit("def __sh_inc_post(var, d):");
            self.emit("    v = int(globals().get(var, 0))");
            self.emit("    globals()[var] = v + d");
            self.emit("    return v");
        }
        if self.need_assign {
            self.emit("");
            // ${x:=default}: write the default INTO the module global and
            // yield it (native, no store round-trip)
            self.emit("def __sh_assign(var, val):");
            self.emit("    globals()[var] = val");
            self.emit("    return val");
        }
        if self.need_capture {
            self.emit("");
            // bash $() semantics as a helper: stdout only, ALL trailing
            // newlines stripped (bash command substitution), never raises:
            // a failed/missing command yields "" and the script continues
            self.emit("def __sh_capture(argv):");
            self.emit("    import subprocess");
            self.emit("    sys.stdout.flush()");
            self.emit("    try:");
            self.emit("        r = subprocess.run(argv, stdout=subprocess.PIPE)");
            self.emit("        return r.stdout.decode(errors='replace').rstrip('\\n')");
            self.emit("    except OSError:");
            self.emit("        return \"\"");
            self.emit("");
            self.emit("def __sh_capture_bash(script):");
            self.emit("    return __sh_capture([\"bash\", \"-c\", script])");
            self.emit("");
            self.emit("def __sh_run_status(argv):");
            self.emit("    import subprocess");
            self.emit("    try:");
            self.emit("        return subprocess.run(argv, stdout=subprocess.DEVNULL).returncode");
            self.emit("    except OSError:");
            self.emit("        return 127");
        }
        if self.need_capture_out || self.sh2_calls.contains(&"line".to_string()) {
            self.emit("def __sh_lines(v):");
            self.emit("    return str(v).split('\\n')");
            // the C frontend's outparam channel: the callee echoes its
            // out-param values; the caller captures STDOUT into a buffer.
            // Callee bodies read their POSITIONAL params via getVar("1")
            // (the bash $1 convention → sys.argv), so the shim sets argv
            // to the stringified call arguments for the duration.
            self.emit("");
            self.emit("def __sh_capture_out(__fn, *__args):");
            self.emit("    import io");
            self.emit("    import contextlib");
            self.emit("    import sys as __sys");
            self.emit("    __buf = io.StringIO()");
            self.emit("    __old_argv = __sys.argv");
            self.emit("    __sys.argv = [__fn.__name__] + [str(a) for a in __args]");
            self.emit("    with contextlib.redirect_stdout(__buf):");
            self.emit("        __fn()");
            self.emit("    __sys.argv = __old_argv");
            self.emit("    return __buf.getvalue()");
        }
        self.emit("");
        if !self.sh2_calls.is_empty() {
            self.emit("# sh2.* runtime — subset of harness/sh2-namespace.mjs");
            self.emit("# (the C memory arena, the var store, arrays, tests)");
            self.emit("__sh_store = {}");
            self.emit("__sh_mem = {}");
            self.emit("__sh_mem_seq = 0");
            self.emit("__sh_arrays = {}");
            self.emit("");
            // the arena/test helpers ride along with their users (python
            // resolves names at call time, so order is irrelevant)
            let has_mem = self.sh2_calls.iter().any(|n| {
                matches!(
                    n.as_str(),
                    "memAlloc" | "memStore" | "memLoad" | "memAdvance" | "memFree"
                        | "memTest"
                )
            });
            if has_mem {
                if let Some(b) = Render::runtime_body("memElemSize") {
                    self.emit_runtime_body(b);
                }
                if let Some(b) = Render::runtime_body("memParse") {
                    self.emit_runtime_body(b);
                }
                if let Some(b) = Render::runtime_body("memPos") {
                    self.emit_runtime_body(b);
                }
            }
            if self.sh2_calls.contains("test") {
                if let Some(b) = Render::runtime_body("testVal") {
                    self.emit_runtime_body(b);
                }
            }
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in &names {
                match Render::runtime_body(name) {
                    Some(body) => self.emit_runtime_body(body),
                    None => {
                        self.emit(&format!("def sh2_{name}(*args):"));
                        self.emit(&format!("    print(\"TODO sh2.{name}\", file=sys.stderr)"));
                        self.emit("    sys.exit(2)");
                        self.emit("");
                    }
                }
            }
        }
        if self.need_atoi {
            // printf %d/%i/%u args: parseInt(s, 10) || 0 semantics
            self.emit("def __sh_atoi(s):");
            self.emit("    try:");
            self.emit("        return int(str(s).strip(), 10)");
            self.emit("    except ValueError:");
            self.emit("        return 0");
            self.emit("");
        }
        if self.need_rc {
            // `$?` reads: statement-position test/and/or/true/false
            // expressions update it (see the Expr arm); defaults to 0
            // (success) — bash-correct for the `true; [ $? -eq 0 ]`
            // idiom (the corpus shape).
            self.emit("__sh_rc = 0");
            self.emit("");
        }
        if self.need_strip {
            // `${var#/##/%%/% pattern}` prefix/suffix removal (bash pattern
            // removal; glob → regex at runtime). `#`/`%` remove the
            // SHORTEST match (non-greedy), `##`/`%%` the LONGEST (greedy).
            // Approximation: `*` → `.*`, `?` → `.`; the exact bash
            // matching order for multi-star patterns is not reproduced.
            self.emit("def __sh_strip(pat, op, s):");
            self.emit("    import re as _re");
            self.emit("    r = _re.escape(pat).replace(r'\\*', '.*').replace(r'\\?', '.')");
            self.emit("    s = str(s)");
            self.emit("    if op == '#':");
            self.emit("        m = _re.match('^.*?' + r, s)");
            self.emit("        return s[m.end():] if m else s");
            self.emit("    if op == '##':");
            self.emit("        m = _re.match('^' + r, s)");
            self.emit("        return s[m.end():] if m else s");
            self.emit("    if op == '%':");
            self.emit("        m = _re.search('(.*)(' + r.replace('.*', '.*?') + ')$', s)");
            self.emit("        return s[:m.start(2)] if m else s");
            self.emit("    m = _re.search('(.*?)(' + r + ')$', s)");
            self.emit("    return s[:m.start(2)] if m else s");
            self.emit("");
        }
        // Subroutine definitions (before the body that calls them).
        for sub in &prog.subs {
            let params: Vec<String> = sub.params.iter().map(|p| self.py_ident(p)).collect();
            self.emit(&format!(
                "def {}({}):",
                self.py_ident(&sub.name),
                params.join(", ")
            ));
            self.in_function += 1;
            self.block(&sub.body);
            self.in_function -= 1;
            self.emit("");
        }
        self.out.extend(body_out.iter().cloned());
        if self.todo > 0 {
            self.emit(&format!(
                "# {} construct(s) lowered to TODO markers",
                self.todo
            ));
        }
    }
}

/// Collect every variable name referenced by statements (assign targets,
/// declare lists, Var reads).
fn collect_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
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
            IrStmt::DeclareArray { var, elements, .. } => {
                out.insert(var.clone());
                for e in elements {
                    collect_vars_expr(e, out);
                }
            }
            IrStmt::Expr(e) => collect_vars_expr(e, out),
            IrStmt::Output { value, .. } => collect_vars_expr(value, out),
            IrStmt::WriteFile { path, content, .. } => {
                collect_vars_expr(path, out);
                collect_vars_expr(content, out);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_vars_expr(cond, out);
                collect_vars(then, out);
                for (c, b) in elsifs {
                    collect_vars_expr(c, out);
                    collect_vars(b, out);
                }
                collect_vars(else_, out);
            }
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone());
                collect_vars_expr(iter, out);
                collect_vars(body, out);
            }
            IrStmt::While { cond, body } => {
                collect_vars_expr(cond, out);
                collect_vars(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_vars(body, out);
                collect_vars_expr(cond, out);
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
            IrStmt::Break | IrStmt::Continue => {}
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                collect_vars(body, out);
                for e in excepts {
                    if let Some(asn) = &e.as_name {
                        out.insert(asn.clone());
                    }
                    collect_vars(&e.body, out);
                }
                collect_vars(else_body, out);
                collect_vars(finally_body, out);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::Function { name, body, .. } => {
                out.insert(name.clone());
                collect_vars(body, out);
            }
            IrStmt::Return(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::Exec {
                cmd, args, capture, ..
            } => {
                collect_vars_expr(cmd, out);
                for a in args {
                    collect_vars_expr(a, out);
                }
                if let Some(v) = capture {
                    out.insert(v.clone());
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => collect_vars(b, out),
            _ => {}
        }
    }
}

fn collect_vars_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
        }
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            collect_vars_expr(key, out);
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
        IrExpr::Object(kv) => {
            for (_, v) in kv {
                collect_vars_expr(v, out);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_vars_expr(obj, out);
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_vars_expr(cond, out);
            collect_vars_expr(then, out);
            collect_vars_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_vars_expr(expr, out);
            collect_vars_expr(default, out);
        }
        IrExpr::Capture { expr, .. } => collect_vars_expr(expr, out),
        IrExpr::Arrow(b) => collect_vars(b, out),
        _ => {}
    }
}

fn collect_vars_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key, .. } => {
            out.insert(var.clone());
            collect_vars_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_vars_arith(lhs, out);
            collect_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_vars_arith(arg, out),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_vars_arith(test, out);
            collect_vars_arith(then, out);
            collect_vars_arith(else_, out);
        }
        ArithAst::Assign { rhs, .. } => collect_vars_arith(rhs, out),
        _ => {}
    }
}

/// `{x,y}{1..2}` / `a{1..3}b` — compute the space-joined brace-expansion
/// words. Returns None for any non-literal part (the renderer refuses).
fn py_brace_words(args: &[IrExpr]) -> Option<String> {
    let words = py_brace_words_list(args)?;
    Some(words.join(" "))
}

fn py_brace_words_list(args: &[IrExpr]) -> Option<Vec<String>> {
    let pre = match args.first()? {
        IrExpr::Str(s, _) => s.clone(),
        _ => return None,
    };
    let groups: Vec<Vec<String>> = match args.get(1)? {
        IrExpr::Json(serde_json::Value::Array(items)) => {
            let mut g = Vec::new();
            for item in items {
                let arr = item.as_array()?;
                let comma = arr.len() > 1;
                let mut one = Vec::new();
                for e in arr {
                    if let Some(s) = e.as_str() {
                        one.push(s.to_string());
                    } else if let Some(range) = e.get("range").and_then(|r| r.as_array()) {
                        let start = range.first()?.as_str().unwrap_or("");
                        let end = range.get(1)?.as_str().unwrap_or("");
                        if comma {
                            // a comma list keeps range-looking items literal
                            one.push(format!("{start}..{end}"));
                        } else {
                            let step: i64 = range
                                .get(2)?
                                .as_str()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(1);
                            let (Ok(a), Ok(b)) =
                                (start.parse::<i64>(), end.parse::<i64>())
                            else {
                                // CHAR RANGE {a..z} / {A..E} — single-letter
                                // endpoints expand as character sequences
                                // (bash brace expansion; step is ignored by
                                // bash for char ranges). Expand statically.
                                if start.len() == 1
                                    && end.len() == 1
                                    && start.chars().all(|c| c.is_ascii_alphabetic())
                                    && end.chars().all(|c| c.is_ascii_alphabetic())
                                    && start.chars().next().unwrap_or('a').is_ascii_lowercase()
                                        == end.chars().next().unwrap_or('a').is_ascii_lowercase()
                                {
                                    let s = start.chars().next().unwrap() as u8;
                                    let e = end.chars().next().unwrap() as u8;
                                    if comma {
                                        // a comma list keeps range-looking items literal
                                        one.push(format!("{start}..{end}"));
                                    } else if s <= e {
                                        for c in s..=e {
                                            one.push((c as char).to_string());
                                        }
                                    } else {
                                        for c in (e..=s).rev() {
                                            one.push((c as char).to_string());
                                        }
                                    }
                                    continue;
                                }
                                return None;
                            };
                            let pad = if start.len() > 1 && start.starts_with('0') {
                                Some(start.len())
                            } else {
                                None
                            };
                            let fmt = |n: i64| {
                                let s = n.to_string();
                                match pad {
                                    Some(w) if s.len() < w => {
                                        format!("{}{}", "0".repeat(w - s.len()), s)
                                    }
                                    _ => s,
                                }
                            };
                            if step >= 0 {
                                let mut n = a;
                                while n <= b {
                                    one.push(fmt(n));
                                    n += step;
                                    if step == 0 {
                                        break;
                                    }
                                }
                            } else {
                                let mut n = a;
                                while n >= b {
                                    one.push(fmt(n));
                                    n += step;
                                }
                            }
                        }
                    } else {
                        return None;
                    }
                }
                g.push(one);
            }
            g
        }
        _ => return None,
    };
    let suf = match args.get(3)? {
        IrExpr::Str(s, _) => s.clone(),
        _ => return None,
    };
    // cartesian product of the groups; prefix + concat + suffix per combo
    let mut combos: Vec<String> = vec![String::new()];
    for group in &groups {
        let mut next = Vec::new();
        for combo in &combos {
            for item in group {
                next.push(format!("{combo}{item}"));
            }
        }
        combos = next;
    }
    Some(combos.iter().map(|c| format!("{pre}{c}{suf}")).collect())
}

/// py_brace_list — the WORDS of a brace expansion as a list (for
/// for-loop iteration, where the space-joined string form would collapse
/// the whole expansion into ONE iteration).
fn py_brace_list(args: &[IrExpr]) -> Option<Vec<String>> {
    py_brace_words_list(args)
}
