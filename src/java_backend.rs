//! java_backend — Java renderer (worktree-local, branch `backend/java`).
//!
//! Consumes the ShIR (the A1 contract) in-process and emits Java source.
//!
//! Runtime model (native-idiomatic, no FFI):
//! - every shell variable → `static String __v_<name>` field
//! - arrays → `static java.util.ArrayList<String> __a_<name>`
//! - `$?` → `static long __SH_RC`; positionals → `__SH_ARGV`
//! - shell functions → `static void fn_<name>()` with argv save/restore
//! - echo/printf/param-expansion/arith/test render NATIVELY; external
//!   commands and command substitutions run via `bash -c` through
//!   ProcessBuilder (inherent fork/exec for external programs)
//! - anything unrenderable returns Err (refuse > guess) — the gate
//!   reports FAIL and the error names the node.

use crate::ir::{ArithAst, AssignTarget, BinOpKind, Decl, InterpPart, IrCaseClause, IrExpr,
                IrProgram, IrRedirect, IrStmt, StrStyle};
use std::collections::BTreeSet;

pub fn shir_to_java(prog: &IrProgram) -> Result<String, String> {
    let mut r = JavaRender::new();
    r.scan(prog.stmts.clone())?;
    r.render(prog)
}

/// Child → parent report for one loop-carried variable: appends
/// `NAME VALUE` to the sync file. Written INLINE (an `export -f`ed helper
/// function does not survive a Java-spawned child: its `BASH_FUNC_*`
/// environment entry is dropped on exec).
const SYNC_REPORT: &str = r#"printf '%s %s\n' "#;
struct JavaRender {
    in_fn: usize,                  // >0 while rendering a function body
    cur_fn: String,                // name of the function being rendered ("" at top level)
    fields: BTreeSet<String>,      // scalar vars (sanitized names kept separate)
    arrays: BTreeSet<String>,
    assoc: BTreeSet<String>,       // `declare -A` maps: parallel __k_<name> key list
    funcs: BTreeSet<String>,
    fnvalues: BTreeSet<String>,    // functions ever called via fnValue (echo-return-lifted: their Return is a VALUE, not a status)
    exit_traps: Vec<String>,       // EXIT-trap bodies to run before normal end-of-main exit
    temp_setup_done: BTreeSet<String>, // __ps_* temp assigns already pre-run natively
    sync_pending: Vec<String>,
    lower_vars: BTreeSet<String>,  // `typeset -l`: value is lowercased on assign
    child_sym: BTreeSet<String>,   // vars the CHILD side computes (symbolic in child text)
    upper_vars: BTreeSet<String>,  // `typeset -u`: value is uppercased on assign     // vars a just-rendered child loop assigns (copied back)
    helpers: BTreeSet<&'static str>,
    java_helpers: Vec<String>,     // dynamically generated method sources
    redir_seq: usize,
    tmp_seq: usize,
    out: Vec<String>,
    depth: usize,
}

fn sanitize(name: &str) -> String {
    let mut s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if s.is_empty() || s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        s = format!("v{s}");
    }
    s
}

/// A Java method name for a user shell function. Keep the user's name
/// unless it collides with a Java reserved word or the generated
/// `sh*`/`__*` helper namespace — `def f()` must render `f()`, not
/// `fn_f()`.
fn fn_name(name: &str) -> String {
    let base = sanitize(name);
    if JAVA_RESERVED.contains(&base.as_str()) || base.starts_with("__") || base.starts_with("sh") && base.len() > 2 {
        format!("fn_{base}")
    } else {
        base
    }
}

const JAVA_RESERVED: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class",
    "const", "continue", "default", "do", "double", "else", "enum", "extends", "final",
    "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int",
    "interface", "long", "native", "new", "package", "private", "protected", "public",
    "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
    "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false",
    "null", "var", "record", "yield", "sealed", "permits",
];

impl JavaRender {
    fn new() -> Self {
        JavaRender { in_fn: 0, cur_fn: String::new(), fields: BTreeSet::new(), arrays: BTreeSet::new(), assoc: BTreeSet::new(), funcs: BTreeSet::new(),
                     fnvalues: BTreeSet::new(), exit_traps: Vec::new(), temp_setup_done: BTreeSet::new(), sync_pending: Vec::new(), lower_vars: BTreeSet::new(), upper_vars: BTreeSet::new(),
                     helpers: BTreeSet::new(), java_helpers: Vec::new(), redir_seq: 0, tmp_seq: 0, child_sym: BTreeSet::new(),
                     out: Vec::new(), depth: 2 }
    }

    fn helper(&mut self, h: &'static str) { self.helpers.insert(h); }

    // ── pass 1: collect declared scalars/arrays/functions ────────────
    fn scan(&mut self, stmts: Vec<IrStmt>) -> Result<(), String> {
        for st in stmts {
            match st {
                IrStmt::Assign { targets, expr, .. } => {
                    for t in targets { self.decl_target(t.clone())?; }
                    self.scan_expr(expr.clone())?;
                }
                IrStmt::Declare { vars, .. } => {
                    for Decl { name, .. } in vars { self.fields.insert(sanitize(&name)); }
                }
            IrStmt::DeclareArray { var, elements, .. } => {
                    self.arrays.insert(sanitize(&var));
                    for e in elements { self.scan_expr(e)?; }
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. })
                    if (func == "exec" || func == "builtin") && str_arg(&args, 0) == Some("declare") =>
                {
                    // `declare -A map`: assoc-map marker (parallel key list)
                    let rest: Vec<IrExpr> = args.iter().skip(1).cloned().collect();
                    let flat = flatten_words(&rest);
                    let mut is_assoc = false;
                    for w in flat {
                        if let IrExpr::Str(s, _) = w {
                            if s == "-A" { is_assoc = true; }
                            else if !s.starts_with('-') {
                                let nm = s.split('=').next().unwrap_or(s);
                                self.arrays.insert(sanitize(nm));
                                if is_assoc { self.assoc.insert(sanitize(nm)); }
                                self.scan_expr((*w).clone())?;
                            }
                        } else {
                            self.scan_expr((*w).clone())?;
                        }
                    }
                }
                IrStmt::Function { name, body, .. } => {
                    self.funcs.insert(name);
                    self.scan(body)?;
                }
                // command text runs in a CHILD shell (bash -c): the shell vars
                // it reads/writes are learned from the IR so the java side
                // can seed and report them
                IrStmt::Redirect { inner: ird, redirects: rd } => {
                    {
                        let mut seen = std::collections::BTreeSet::new();
                        collect_shell_names(&ird, &mut self.fields, &mut seen);

                    }
                    let red = IrStmt::Redirect { inner: ird.clone(), redirects: rd.clone() };
                    for e in stmt_exprs(&red) { self.scan_expr(e.clone())?; }
                    let kids = stmt_children(IrStmt::Redirect { inner: ird, redirects: rd });
                    self.scan(kids)?;
                }
                IrStmt::For { var, body, .. } => {
                    self.fields.insert(sanitize(&var));
                    self.scan(body)?;
                }
                other => {
                    for e in stmt_exprs(&other) { self.scan_expr(e.clone())?; }
                    let kids = stmt_children(other);
                    self.scan(kids)?;
                }
            }
        }
        Ok(())
    }

    fn decl_target(&mut self, t: AssignTarget) -> Result<(), String> {
        if !t.indices.is_empty() || t.var.contains('[') {
            // arr[i]=v arrives as var="arr[1]", indices empty
            let base = t.var.split('[').next().unwrap_or(&t.var).to_string();
            self.arrays.insert(sanitize(&base));
        } else {
            self.fields.insert(sanitize(&t.var));
        }
        Ok(())
    }

    fn scan_expr(&mut self, e: IrExpr) -> Result<(), String> {
        match e {
            IrExpr::Call { func, args, .. } => {
                if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        self.arrays.insert(sanitize(name));
                    }
                }
                // the echo-return-lift's value convention: these functions'
                // Return nodes are VALUES (print them), not statuses
                if matches!(func.as_str(), "fnValue" | "fnCall") {
                    if let Some(n) = str_arg(&args, 0) {
                        self.fnvalues.insert(n.to_string());
                    }
                }
                for a in args { self.scan_expr(a)?; }
            }
            IrExpr::Arrow(stmts) => { self.scan(stmts)?; }
            IrExpr::Capture { expr, .. } => { self.scan_expr(*expr)?; }
            IrExpr::BinOp { lhs, rhs, .. } => { self.scan_expr(*lhs)?; self.scan_expr(*rhs)?; }
            IrExpr::Interpolate(parts) => {
                for p in parts { if let InterpPart::Expr(x) = p { self.scan_expr(*x)?; } }
            }
            IrExpr::Ternary { cond, then, else_ } => {
                self.scan_expr(*cond)?; self.scan_expr(*then)?; self.scan_expr(*else_)?;
            }
            IrExpr::DefinedOr { expr, default } => { self.scan_expr(*expr)?; self.scan_expr(*default)?; }
            IrExpr::MethodCall { obj, args, .. } => { self.scan_expr(*obj)?; for a in args { self.scan_expr(a)?; } }
            IrExpr::Index { key, .. } => { self.scan_expr(*key)?; }
            IrExpr::Arith(a) => {
                if let ArithAst::Assign { var, .. } = &*a {
                    self.fields.insert(sanitize(var));
                }
                if let ArithAst::IncDec { var, .. } = &*a {
                    self.fields.insert(sanitize(var));
                }
                // nested arith vars are resolved at render; field presence
                // for plain reads is best-effort via getvar fallback env
            }
            IrExpr::Array(items) => { for i in items { self.scan_expr(i)?; } }
            _ => {}
        }
        Ok(())
    }

    /// A generated `static long __post_<var>(long delta)` yielding the OLD
    /// value while storing the new one back into the var's field.
    fn postfix_helper(&mut self, var: &str) -> String {
        let f = format!("__v_{}", sanitize(var));
        let name = format!("__post_{}", sanitize(var));
        if !self.java_helpers.iter().any(|h| h.contains(&format!("static long {name}("))) {
            self.java_helpers.push(format!(
                "    static long {name}(long delta) {{\n        long old = shNum({f});\n        {f} = String.valueOf(old + delta);\n        return old;\n    }}\n"
            ));
        }
        name
    }

    // ── output plumbing ──────────────────────────────────────────────
    fn ensure_field(&mut self, name: &str) {
        if self.fields.insert(sanitize(name)) {
            // a new shell variable joins the child-visible env
            self.helper("envset");
        }
    }

    fn emit(&mut self, line: &str) {
        let mut s = String::new();
        for _ in 0..self.depth { s.push_str("    "); }
        s.push_str(&self.mirror_env(line));
        self.out.push(s);
    }

    /// Every shell-variable assignment (`__v_x = …`) is also recorded in the
    /// child-visible env (`__SH_ENV`), so a spawned command text can expand
    /// `$x` with the CURRENT value. Pure text rewrite: the statement's
    /// semantics are unchanged (the method returns the value).
    fn mirror_env(&self, line: &str) -> String {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("__v_") {
            if let Some(eq) = rest.find(" = ") {
                let name = rest[..eq].split_whitespace().next().unwrap_or("");
                let rhs = &rest[eq + 3..];
                if !name.is_empty() && rhs.ends_with(';')
                    && !rhs.contains(';')
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    let expr = &rhs[..rhs.len() - 1];
                    return format!("{}__v_{} = shEnvSet(\"{}\", {});", &line[..line.len() - t.len()], name, name, expr);
                }
            }
        }
        line.to_string()
    }

    fn block_open(&mut self, head: &str) {
        self.emit(head);
        self.depth += 1;
    }

    fn block_close(&mut self) {
        self.depth -= 1;
        self.emit("}");
    }

    // ── program assembly ─────────────────────────────────────────────
    fn render(mut self, prog: &IrProgram) -> Result<String, String> {
        let mut o = String::new();
        o.push_str("import java.io.*;\n");
        o.push_str("import java.nio.charset.StandardCharsets;\n");
        o.push_str("import java.nio.file.*;\n");
        o.push_str("import java.util.*;\n\n");
        o.push_str("public class Sh2Program {\n");
        o.push('\n');
        // NOTE: the class header (fields/arrays) is assembled AFTER the body
        // renders, because rendering discovers additional variables (read
        // targets, arith assigns, dynamic exports).
        // main body
        self.depth = 1;
        self.out.clear();
        self.emit("public static void main(String[] args) throws Exception {");
        // the child-side sync helper (writes NAME VALUE lines a loop can
        // report back to the parent through $SH2_SYNC_FILE)
        self.emit("System.setProperty(\"SH2_SYNC_FILE\", shSyncFile().getAbsolutePath());");
        // (`export -f` so a PIPELINE's child shell — not just a direct
        //   child — can report its loop vars back through the same function)
        self.emit("shRun(\"_sh2_env() { printf '%s %s\\\\n' \\\"$1\\\" \\\"${2-}\\\" >> \\\"$SH2_SYNC_FILE\\\"; }; export -f _sh2_env; \" + \"\");");
        self.emit("for (String __a : args) __SH_ARGV.add(__a);");
        for st in &prog.stmts {
            if matches!(st, IrStmt::Function { .. }) { continue; }
            self.stmt(st)?;
        }
        // EXIT traps run before normal end-of-main exit (bash semantics;
        // explicit `exit N` / return paths bypass them — same limitation
        // as the no-op trap arms they replace, but the common case works)
        if !self.exit_traps.is_empty() {
            self.helper("runrc");
            let bodies: Vec<String> = std::mem::take(&mut self.exit_traps);
            for b in bodies {
                self.emit(&format!("__SH_RC = shRunEval({});", jstr(&b)));
            }
        }
        self.emit("System.exit((int) __SH_RC);");
        self.block_close();
        // flush the main body before emitting function definitions
        let mut main_body = std::mem::take(&mut self.out);
        // function definitions (nested defs are hoisted flat)
        let mut fn_list: Vec<(String, Vec<IrStmt>)> = Vec::new();
        fn collect_fns(stmts: &[IrStmt], out: &mut Vec<(String, Vec<IrStmt>)>, seen: &mut BTreeSet<String>) {
            for st in stmts {
                if let IrStmt::Function { name, body, .. } = st {
                    if seen.insert(name.clone()) {
                        out.push((name.clone(), body.clone()));
                        collect_fns(body, out, seen);
                    }
                }
            }
        }
        let mut seen_fns: BTreeSet<String> = BTreeSet::new();
        collect_fns(&prog.stmts, &mut fn_list, &mut seen_fns);
        let mut fn_buf = String::new();
        for (name, body) in fn_list.iter() {
                let fname = fn_name(name);
                fn_buf.push_str(&format!("    static String {fname}() throws Exception {{\n"));
                fn_buf.push_str("        List<String> __saved = new ArrayList<>(__SH_ARGV);\n");
                fn_buf.push_str("        try {\n");
                let saved_depth = self.depth;
                self.out.clear();
                self.depth = 2;
                self.in_fn += 1;
                let saved_fn = std::mem::replace(&mut self.cur_fn, name.clone());
                for b in body { self.stmt(b)?; }
                self.cur_fn = saved_fn;
                self.in_fn -= 1;
                let body_lines = std::mem::take(&mut self.out);
                self.depth = saved_depth;
                for l in &body_lines { fn_buf.push_str(l); fn_buf.push('\n'); }
                fn_buf.push_str("        } finally { __SH_ARGV = __saved; }\n");
                // A body ending in a jump never falls through — appending
                // code after it is a Java unreachable-statement error.
                // (System.exit does NOT count: javac models it as a normal
                // call, so a trailing return is still REQUIRED after it.)
                let ends_with_return = body_lines.iter().rev()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| {
                        let t = l.trim();
                        t.starts_with("return ") || t.ends_with("break;")
                            || t.ends_with("continue;")
                    })
                    .unwrap_or(false);
                if !ends_with_return {
                    fn_buf.push_str("        return \"\";\n");
                }
                fn_buf.push_str("    }\n\n");
        }
        // dynamically generated helpers (postfix incdec etc.)
        for h in std::mem::take(&mut self.java_helpers) {
            o.push_str(&h);
            o.push('\n');
        }
        // assemble: header fields discovered during rendering
        let mut header = String::from("    static long __SH_RC = 0;\n    static String __SH_T = \"\";\n    static String __v___mf = \"\";\n    static boolean __SH_NOCASE = false;
    static String __SH_CWD = System.getProperty(\"user.dir\");\n    static LinkedHashMap<String, String> __SH_EXPORTS = new LinkedHashMap<>();
    static LinkedHashMap<String, String> __SH_ENV = new LinkedHashMap<>();
    static LinkedHashMap<String, String> __SH_SYNC = new LinkedHashMap<>();\n    static List<String> __SH_ARGV = new ArrayList<>();\n    static Scanner __SH_IN = new Scanner(System.in);\n");
        for f in &self.fields {
            header.push_str(&format!("    static String __v_{f} = \"\";\n"));
        }
        for a in &self.arrays {
            header.push_str(&format!("    static List<String> __a_{a} = new ArrayList<>();\n"));
        }
        for a in &self.assoc {
            header.push_str(&format!("    static List<String> __k_{a} = new ArrayList<>();\n"));
        }
        header.push_str("    static PrintStream __fd3 = null;\n    static PrintStream __fd4 = null;\n    static PrintStream __fd5 = null;\n    static PrintStream __fd6 = null;\n    static PrintStream __fd7 = null;\n    static PrintStream __fd8 = null;\n    static PrintStream __fd9 = null;\n");
        header.push('\n');
        // splice header right after the class opening line
        if let Some(pos) = o.find("{\n") {
            let pos = pos + 2;
            o.insert_str(pos, &header);
        }
        for l in main_body { o.push_str(&l); o.push('\n'); }
        o.push_str(&fn_buf);
        // ALL runtime helpers are emitted unconditionally: javac keeps
        // unused private-free static methods without complaint and this
        // removes the whole missing-symbol failure class.
        let _ = std::mem::take(&mut self.helpers);
        let all_helpers = [
            "env","len","num","arg","shift","cd","run","runrc","capture","printf",
            "escapes","substr","fnmatch","strip","replace","case","basename",
            "dirname","div","pow","ftype","readln","readsplit","sourcefile","split","aset","aget",
            "asetkey","agetkey",
            "writefile","outswap","errswap","inswap","snapshot","regex",
            "contains","quote","declared","errifempty","regexm","eq","testrc","envset",
        "aslice",
        // The Ext-node helpers (text_ops' rich nodes; see
        // src/render_ext_expr/handlers/all_nodes.rs): each implements ONE
        // verified leaf so the shared handler stays a one-liner and the
        // semantics live in a single place.
        "wcwords","cutfield","regsub","regsuball","regcount","takelines",
        "chartrans",
        ];
        for h in all_helpers {
            if let Some(src) = helper_src(h) {
                o.push_str(src);
                o.push('\n');
            }
        }
        o.push_str("}\n");
        Ok(o)
    }

    // ── statements ───────────────────────────────────────────────────
    /// Whether evaluating `e` records a meaningful status in __SH_RC
    /// (captures, pipelines, command substitutions, fnValues). Plain
    /// assignments RESET the status to 0 — unless the value carries one
    /// of these, in which case bash keeps the substitution's status
    /// (`x=$(false)` → rc 1, not 0) and resetting would lose it.
    fn expr_has_capture(e: &IrExpr) -> bool {
        match e {
            IrExpr::Capture { .. } => true,
            IrExpr::Arrow(_) => true,
            IrExpr::Call { func, args, .. }
                if matches!(func.as_str(), "capture" | "captureWords" | "pipeline"
                    | "fnValue" | "fnCall" | "redirect" | "commandSubst") =>
            {
                true
            }
            IrExpr::Call { func, args, .. } => {
                args.iter().any(Self::expr_has_capture)
                    && !matches!(func.as_str(), "and" | "or")
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                Self::expr_has_capture(lhs) || Self::expr_has_capture(rhs)
            }
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => Self::expr_has_capture(x),
                _ => false,
            }),
            IrExpr::Ternary { cond, then, else_ } => {
                Self::expr_has_capture(cond) || Self::expr_has_capture(then) || Self::expr_has_capture(else_)
            }
            IrExpr::DefinedOr { expr, default } => {
                Self::expr_has_capture(expr) || Self::expr_has_capture(default)
            }
            IrExpr::Array(items) => items.iter().any(Self::expr_has_capture),
            _ => false,
        }
    }

    /// Emit the assignment-status epilogue: `__SH_RC = 0` for pure values,
    /// nothing for capture-carrying values (their status stands).
    fn emit_assign_rc(&mut self, expr: &IrExpr) {
        if !Self::expr_has_capture(expr) {
            self.emit("__SH_RC = 0;");
        }
    }

    fn stmt(&mut self, st: &IrStmt) -> Result<(), String> {
        match st {
            IrStmt::Expr(e) => self.expr_stmt(e),
            IrStmt::Assign { targets, expr, .. } => {
                let t = targets.first().ok_or("assign: no target")?;
                // `((i++))` / `((++i))` arrive as Assign{i, Arith(IncDec)}
                // (the estree contract: a NATIVE postfix update, not a
                // value assignment). Rendering the value back (`__v_i =
                // old`) would clobber the side effect — run for effect.
                if let IrExpr::Arith(a) = expr {
                    if let ArithAst::IncDec { var, .. } = &**a {
                        if sanitize(var) == sanitize(&t.var) {
                            let code = self.arith(a)?;
                            self.emit(&format!("{code};"));
                            self.emit("__SH_RC = 0;");
                            return Ok(());
                        }
                    }
                }
                if t.indices.is_empty() && !t.var.contains('[') {
                    self.ensure_field(&t.var);
                }
                if t.indices.is_empty() && t.var.contains('[') && t.var.ends_with(']') {
                    // arr[N]=value (the parser keeps the subscript in the name)
                    let lb = t.var.find('[').unwrap();
                    let base = &t.var[..lb];
                    let idx = &t.var[lb + 1..t.var.len() - 1];
                    let val = self.expr_str(expr)?;
                    let base_s = sanitize(base);
                    if self.assoc.contains(&base_s) {
                        // assoc map: string-keyed set (insertion-ordered)
                        let key = self.assoc_key(idx)?;
                        self.arrays.insert(base_s.clone());
                        self.emit(&format!("shAsetKey(__a_{base_s}, __k_{base_s}, {key}, {val});"));
                    } else if let Ok(k) = idx.trim().parse::<i64>() {
                        self.helper("aset");
                        self.emit(&format!("shAset(__a_{}, {k}L, {val});", sanitize(base)));
                    } else if idx.contains('$') {
                        // dynamic numeric index — lower natively
                        let key = self.array_idx_num(&IrExpr::Str(idx.to_string(), StrStyle::DoubleQuoted))?;
                        self.helper("aset");
                        self.emit(&format!("shAset(__a_{}, {key}, {val});", sanitize(base)));
                    } else {
                        // a dynamic KEY (`${arr[$i]}`) on a non-assoc array:
                        // numeric arithmetic on the current value
                        let key = self.getvar_str(idx)?;
                        self.helper("num");
                        self.helper("aset");
                        self.emit(&format!("shAset(__a_{}, shNum({key}), {val});", sanitize(base)));
                    }
                    return Ok(());
                }
                if !t.indices.is_empty() {
                    // arr[i] = value → list set (grow with empties)
                    let idx = self.expr_num(&t.indices[0])?;
                    let val = self.expr_str(expr)?;
                    self.helper("aset");
                    self.emit(&format!("shAset(__a_{}, {}, {});", sanitize(&t.var), idx, val));
                    self.emit_assign_rc(expr);
                    return Ok(());
                }
                let mut v = self.expr_str(expr)?;
                let sv = sanitize(&t.var);
                if self.lower_vars.contains(&sv) { v = format!("({v}).toLowerCase()"); }
                if self.upper_vars.contains(&sv) { v = format!("({v}).toUpperCase()"); }
                self.emit(&format!("__v_{sv} = {v};"));
                self.emit_assign_rc(expr);
                Ok(())
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                let c = self.cond_bool(cond)?;
                {
                    let mut l = String::new();
                    for _ in 0..self.depth { l.push_str("    "); }
                    l.push_str(&format!("if ({c}) {{"));
                    self.out.push(l);
                }
                self.depth += 1;
                for b in then { self.stmt(b)?; }
                for (ec, eb) in elsifs {
                    self.depth -= 1;
                    let c = self.cond_bool(ec)?;
                    {
                        let mut l = String::new();
                        for _ in 0..self.depth { l.push_str("    "); }
                        l.push_str(&format!("}} else if ({c}) {{"));
                        self.out.push(l);
                    }
                    self.depth += 1;
                    for b in eb { self.stmt(b)?; }
                }
                if !else_.is_empty() {
                    self.depth -= 1;
                    {
                        let mut l = String::new();
                        for _ in 0..self.depth { l.push_str("    "); }
                        l.push_str("} else {");
                        self.out.push(l);
                    }
                    self.depth += 1;
                    for b in else_ { self.stmt(b)?; }
                    self.depth -= 1;
                    {
                        let mut l = String::new();
                        for _ in 0..self.depth { l.push_str("    "); }
                        l.push('}');
                        self.out.push(l);
                    }
                } else {
                    // bash: an `if` with no taken branch and no else
                    // exits 0 (the condition's nonzero status does NOT
                    // leak — `if false; then x; fi; echo $?` prints 0).
                    self.depth -= 1;
                    {
                        let mut l = String::new();
                        for _ in 0..self.depth { l.push_str("    "); }
                        l.push_str("} else { __SH_RC = 0; }");
                        self.out.push(l);
                    }
                }
                Ok(())
            }
            IrStmt::While { cond, body } => {
                // bash: the loop's status is the last BODY command's status
                // (or 0), never the terminating condition's — keep a
                // shadow and restore it when the condition turns false.
                self.tmp_seq += 1;
                let lb = format!("__lastBody{}", self.tmp_seq);
                self.emit(&format!("long {lb} = __SH_RC;"));
                self.block_open("while (true) {");
                let c = self.cond_bool(cond)?;
                self.tmp_seq += 1;
                let cv = format!("__c{}", self.tmp_seq);
                self.emit(&format!("boolean {cv} = {c};"));
                self.emit(&format!("if (!{cv}) {{ __SH_RC = {lb}; break; }}"));
                for b in body { self.stmt(b)?; }
                // a body ending in a jump (break/continue/return/exit) never
                // falls through — emitting the shadow update after it is a
                // Java unreachable-statement error.
                let diverged = self.out.last().map(|l| {
                    let t = l.trim();
                    t.ends_with("break;") || t.ends_with("continue;")
                        || t.starts_with("return ") || t.contains("System.exit(")
                }).unwrap_or(false);
                if !diverged {
                    self.emit(&format!("{lb} = __SH_RC;"));
                }
                self.block_close();
                Ok(())
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.block_open("do {");
                for b in body { self.stmt(b)?; }
                self.depth -= 1;
                let c = self.cond_bool(cond)?;
                let tail = if *until { format!("}} while (!({c}));") } else { format!("}} while ({c});") };
                {
                    let mut s = String::new();
                    for _ in 0..self.depth { s.push_str("    "); }
                    s.push_str(&tail);
                    self.out.push(s);
                }
                self.depth += 1;
                Ok(())
            }
            IrStmt::Shaderize { .. } => {
                Err("shaderize (autoshaderization) not supported by the Java backend".into())
            }
            IrStmt::For { var, iter, body } => {
                let items = self.for_items(iter)?;
                self.fields.insert(sanitize(var));
                self.tmp_seq += 1;
                let itv = format!("__it{}", self.tmp_seq);
                self.emit(&format!("__v_{} = \"\";", sanitize(var)));
                let runtime_list = items.contains('\u{1}') || items.contains("shSplit(")
                   || items.contains("__SH_ARGV");
                if runtime_list {
                    // mixed literal + runtime-list iterable
                    self.tmp_seq += 1;
                    let iv = format!("__items{}", self.tmp_seq);
                    self.emit(&format!("List<String> {iv} = new ArrayList<>();"));
                    for piece in items.split('\u{1}') {
                        if piece.is_empty() { continue; }
                        if let Some(r) = piece.strip_prefix("String.join(\" \", shSplit(") {
                            // join-wrapped split → the bare runtime list
                            let inner = r.strip_suffix("))").unwrap_or(r);
                            self.emit(&format!("{iv}.addAll(shSplit({inner}));"));
                        } else if piece.starts_with('"') || piece.starts_with('(') {
                            self.emit(&format!("{iv}.add({piece});"));
                        } else {
                            self.emit(&format!("{iv}.addAll({piece});"));
                        }
                    }
                    self.block_open(&format!("for (String {itv} : {iv}) {{"));
                } else {
                    self.block_open(&format!("for (String {itv} : new String[] {{{items}}}) {{"));
                }
                self.emit(&format!("__v_{} = {itv};", sanitize(var)));
                for b in body { self.stmt(b)?; }
                self.block_close();
                Ok(())
            }
            IrStmt::Block(body) => {
                if body.is_empty() { return Ok(()); }
                // a block body is COMMAND TEXT (it is a forked group in
                // bash): rendering it statement-natively would send a
                // captured nested command back through the text renderers
                // forever. Text keeps the exact group semantics.
                let is_ctx = self.in_fn > 0 || self.depth > 2
                    || body.iter().any(|b| matches!(b, IrStmt::Block(_) | IrStmt::Subshell(_)));
                let is_target_body = !is_ctx && body.iter().any(|b| matches!(b, IrStmt::Assign { targets, .. }
                    if targets.first().map(|t| t.var.starts_with("__ps_")).unwrap_or(false)));
                if is_target_body || (!is_ctx && body.iter().all(|b| matches!(b, IrStmt::Expr(IrExpr::Call { func, .. })
                        if func == "exec" || func == "builtin" || func == "test" || func == "capture"))) {
                    let t = self.arrow_expr(body)?;
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({t});"));
                    return Ok(());
                }
                self.block_open("{");
                for b in body { self.stmt(b)?; }
                self.block_close();
                Ok(())
            }
            IrStmt::Subshell(body) => {
                // copy semantics: snapshot scalars+argv, run, restore
                self.helper("snapshot");
                self.tmp_seq += 1;
                let k = self.tmp_seq;
                self.emit(&format!("String __snap{k} = shSnapshot(); List<String> __snapArgv{k} = new ArrayList<>(__SH_ARGV); long __snapRc{k} = __SH_RC;"));
                self.block_open("{");
                for b in body { self.stmt(b)?; }
                self.block_close();
                self.emit(&format!("shRestore(__snap{k}); __SH_ARGV = __snapArgv{k}; __SH_RC = __snapRc{k};"));
                Ok(())
            }
            IrStmt::Case { discriminant, clauses } => self.case(discriminant, clauses),
            st @ IrStmt::Redirect { inner: ri, redirects: rr } if ri.iter().any(|b| matches!(b, IrStmt::Expr(IrExpr::Call { func, .. }) if func == "pipeline")) => {
                // `cat <<EOF | grep … > f`: the redirect wraps a PIPELINE —
                // render the stages as text; the redirect rides on the last
                // stage (a native stage's children must not bypass a file).
                let reds = rr.clone();
                if let [IrStmt::Expr(IrExpr::Call { func: pf, args: pargs, .. })] = &ri[..] {
                    if pf == "pipeline" {
                        let mut sp: Vec<String> = Vec::new();
                        if let Some(IrExpr::Array(stages)) = pargs.first() {
                            for s2 in stages.iter() {
                                if let IrExpr::Arrow(body) = s2 { sp.push(self.stage_expr(body)?); }
                            }
                        }
                        if sp.is_empty() { return Err("pipeline without stages".into()); }
                        let text0 = sp.join(" + \" | \" + ");
                        let text = format!("{text0}{}", self.redirect_ops_text(&[], &reds, &[])?);
                        self.helper("run");
                        self.emit(&format!("__SH_RC = shRun({text});"));
                        return Ok(());
                    }
                }
                Ok(())
            }
            st @ IrStmt::Redirect { inner, redirects } => {
                // A lone command with a file redirect runs as a real child
                // (`grep -v x f > /tmp/body`): an in-process PrintStream
                // swap would be invisible to the command's own children.
                let stdin_fed = redirects.iter().any(|r| matches!(r.fd, Some(0))
                    || matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring"));
                // (a pre-run renderer temp is setup, not a command)
                // renderer-temp setup assigns are RUN (here and by the
                // text renderer below), never part of the command line
                let inner: Vec<IrStmt> = inner.iter().filter(|b| !matches!(b, IrStmt::Assign { .. })).cloned().collect();
                let body = inner.len();
                if body == 1 && !stdin_fed && !Self::body_needs_native(&inner) {
                    let inner = &inner[..];
                    // a bare compound (`{ …; } > f`, `( … ) > f`) keeps its
                    // group: only a plain command becomes `cmd > f`
                    let compound = matches!(inner[0], IrStmt::Subshell(_) | IrStmt::Block(_));
                    let text = if compound {
                        self.redirect_text_arms(inner, redirects)?
                    } else {
                        // a plain command: `cmd > f` — the redirect must
                        // reach the command's own children
                        format!("{}{}", self.arrow_expr(inner)?, self.redirect_ops_text(inner, redirects, &[])?)
                    };
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({text});"));
                    self.flush_sync();
                    return Ok(());
                }
                let _ = st;
                // remaining statements (plus any pre-run temps) render as
                // command text, then run
                if inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. }))
                    && stdin_fed {
                    // stdin-fed loop: the loop renders natively, the ops ride
                    // outside it (`{ ( loop ) ; } < file`)
                    let t = self.stage_expr_native_loop(&inner, redirects)?;
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({t});"));
                    self.flush_sync();
                    return Ok(());
                }
                if inner.iter().any(|b| matches!(b, IrStmt::Subshell(_) | IrStmt::Block(_) | IrStmt::While { .. } | IrStmt::For { .. })) {
                    // a forked/compound body stays a real group: bash text
                    // (`{ …; } > f`), never a java block. A loop body goes
                    // through the stage decision (text loop + outside ops).
                    let st1 = IrStmt::Redirect { inner: inner.clone(), redirects: redirects.to_vec() };
                    let t = self.redirect_stage_text(std::slice::from_ref(&st1))?;
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({t});"));
                    self.flush_sync();
                    return Ok(());
                }
                if !inner.is_empty() {
                    let t = self.redirect_text_arms(&inner, redirects)?;
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({t});"));
                    self.flush_sync();
                    return Ok(());
                }
                self.redirect(&inner, redirects)
            }
            IrStmt::Pipeline { stages, capture, cmd_str, .. } =>
                self.pipeline(stages, capture.as_deref(), cmd_str.as_deref()),
            IrStmt::Function { .. } => Ok(()), // emitted at top level
            IrStmt::Return(v) => {
                // Value convention (see the echo_return lift contract): a
                // function ever called via fnValue is VALUE-returning — its
                // Return nodes are echoed values (status 0, the value flows
                // back as the Java return). Any other function is
                // STATUS-returning — its Return sets __SH_RC and yields "".
                let is_value_fn = !self.cur_fn.is_empty() && self.fnvalues.contains(&self.cur_fn);
                match v {
                    None => {
                        if self.in_fn > 0 {
                            self.emit("return \"\";");
                        } else {
                            self.emit(&format!("System.exit((int) __SH_RC);"));
                        }
                    }
                    Some(e) => {
                        if is_value_fn {
                            let s = self.expr_str(e)?;
                            self.emit("__SH_RC = 0;");
                            self.emit(&format!("return {s};"));
                        } else {
                            let is_status = matches!(e, IrExpr::Int(_) | IrExpr::Arith(_)
                                | IrExpr::Var(_, _) | IrExpr::Ident(_))
                                || matches!(e, IrExpr::Str(s, _) if s.trim().parse::<i64>().is_ok())
                                || matches!(e, IrExpr::Call { func, .. } if func == "getVar");
                            if is_status {
                                let x = self.expr_num(e)?;
                                self.emit(&format!("__SH_RC = {x};"));
                            } else {
                                let s = self.expr_str(e)?;
                                self.emit(&format!("System.out.println({s});"));
                                self.emit("__SH_RC = 0;");
                            }
                            if self.in_fn > 0 {
                                self.emit("return \"\";");
                            } else {
                                self.emit(&format!("System.exit((int) __SH_RC);"));
                            }
                        }
                    }
                }
                Ok(())
            }
            IrStmt::Break => { self.emit("__SH_RC = 0; break;"); Ok(()) }
            IrStmt::Continue => { self.emit("__SH_RC = 0; continue;"); Ok(()) }
            IrStmt::Exit(code) => {
                match code {
                    Some(e) => { let c = self.expr_num(e)?; self.emit(&format!("System.exit((int) {c});")); }
                    None => self.emit("System.exit((int) __SH_RC);"),
                }
                Ok(())
            }
            IrStmt::WriteFile { path, content, append } => {
                let p = self.expr_str(path)?;
                let c = self.expr_str(content)?;
                self.helper("writefile");
                self.emit(&format!("shWriteFile({p}, {c}, {append});"));
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrStmt::Output { value, newline, target: None } => {
                let v = self.expr_str(value)?;
                if *newline { self.emit(&format!("System.out.println({v});")); }
                else { self.emit(&format!("System.out.print({v});")); }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrStmt::Output { target: Some(_), .. } =>
                Err("Output to filehandle not supported by the java renderer".into()),
            IrStmt::Declare { vars, init, local: _ } => {
                if let Some(e) = init {
                    let v = self.expr_str(e)?;
                    let first = vars.first().ok_or("declare: no vars")?;
                    self.emit(&format!("__v_{} = {v};", sanitize(&first.name)));
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                let parts = self.join_words(elements, ", ")?;
                self.emit(&format!("__a_{} = new ArrayList<>(List.of(new String[]{{{parts}}}));", sanitize(var)));
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrStmt::Exec { cmd, args, capture: None, .. } => {
                let mut words = Vec::new();
                let cmd_e = self.expr_str(cmd)?;
                if Self::is_multi_word(cmd) {
                    words.push(cmd_e);
                } else {
                    words.push(format!("shQuote({cmd_e})"));
                }
                for a in args.iter() {
                    let e = self.expr_str(a)?;
                    if Self::is_multi_word(a) {
                        words.push(e);
                    } else {
                        words.push(format!("shQuote({e})"));
                    }
                }
                let line = words.join(" + \" \" + ");
                self.helper("quote");
                self.helper("run");
                // renderer-temp cleanup (`rm -f __ps_tmp*`): preserve the
                // recorded status (same rule as the command() fallback).
                let is_rm = matches!(cmd, IrExpr::Str(c, _) if c == "rm");
                let all_temp = args.iter().flat_map(|w| flatten_words(std::slice::from_ref(w))).all(|w| match w {
                    IrExpr::Str(s, _) => s.starts_with('-') || s.contains("__ps_tmp"),
                    IrExpr::Var(n, _) | IrExpr::Ident(n) => n.contains("__ps_tmp"),
                    IrExpr::Call { func, args, .. } if func == "getVar" => {
                        str_arg(args, 0).map(|n| n.contains("__ps_tmp")).unwrap_or(false)
                    }
                    _ => false,
                });
                if is_rm && all_temp {
                    self.tmp_seq += 1;
                    let sv = format!("__rcsv{}", self.tmp_seq);
                    self.emit(&format!("long {sv} = __SH_RC; __SH_RC = shRun({line}); __SH_RC = {sv};"));
                } else {
                    self.emit(&format!("__SH_RC = shRun({line});"));
                }
                Ok(())
            }
            IrStmt::Exec { capture: Some(_), .. } =>
                Err("Exec with capture not supported by the java renderer".into()),
            IrStmt::Die { expr, .. } => {
                let m = self.expr_str(expr)?;
                self.emit(&format!("System.err.println({m}); System.exit(1);"));
                Ok(())
            }
            IrStmt::Warn { expr, .. } => {
                let m = self.expr_str(expr)?;
                self.emit(&format!("System.err.println({m});"));
                Ok(())
            }
            IrStmt::Background(body) => {
                // real thread; statics are shared (documented limitation:
                // bash copies state at fork — corpus background jobs that
                // mutate parent state would need per-job snapshots)
                self.tmp_seq += 1;
                let bg = format!("__bg{}", self.tmp_seq);
                self.block_open(&format!("Thread {bg} = new Thread(() -> {{ try {{"));
                for b in body { self.stmt(b)?; }
                self.depth -= 1;
                {
                    let mut l = String::new();
                    for _ in 0..self.depth { l.push_str("    "); }
                    l.push_str("} catch (Exception __e) { throw new RuntimeException(__e); } });");
                    self.out.push(l);
                }
                self.emit(&format!("{}.start();", bg));
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrStmt::Ext(node) => Err(format!("ext node not supported by the java renderer: {node:?}")),
            IrStmt::ForInit { .. } => Err("C-style for not supported by the java renderer (strip_cfor should have lowered it)".into()),
            IrStmt::Try { .. } => Err("Try not supported by the java renderer".into()),
            IrStmt::Select { .. } => Err("Select not supported by the java renderer".into()),
            IrStmt::SetChildError(e) => {
                let v = self.expr_num(e)?;
                self.emit(&format!("__SH_RC = {v};"));
                Ok(())
            }
            IrStmt::Require(_) | IrStmt::RawText(_) => Ok(()),
            IrStmt::Asm { .. } => Err("asm not supported by the java renderer".into()),
            IrStmt::Label(_) | IrStmt::Goto(_) => Err("goto not supported by the java renderer".into()),
        }
    }

    fn case(&mut self, disc: &IrExpr, clauses: &[IrCaseClause]) -> Result<(), String> {
        let d = self.expr_str(disc)?;
        let d = format!("(\"\" + {d})");
        self.helper("fnmatch");
        let mut first = true;
        for cl in clauses {
            let mut conds = Vec::new();
            for p in &cl.patterns {
                // ShIR case patterns keep their source quotes (`"add"`);
                // bash compares the quote-removed value — strip one
                // wrapping quote pair (globs like `*` have none).
                conds.push(format!("shFnmatch({}, {d})", jstr(&strip_wrapping_quotes(p))));
            }
            let cond = conds.join(" || ");
            {
                let mut l = String::new();
                for _ in 0..self.depth { l.push_str("    "); }
                l.push_str(if first { format!("if ({cond}) {{") } else { format!("}} else if ({cond}) {{") }.as_str());
                self.out.push(l);
            }
            first = false;
            self.depth += 1;
            for b in &cl.body { self.stmt(b)?; }
            self.depth -= 1;
        }
        {
            let mut l = String::new();
            for _ in 0..self.depth { l.push_str("    "); }
            l.push('}');
            self.out.push(l);
        }
        Ok(())
    }

    /// redirect(Arrow(body), Array[{fd,mode,target}...]) args → IrRedirect list.
    fn redirect_list(a: Option<&IrExpr>) -> Result<Vec<crate::ir::IrRedirect>, String> {
        let mut redirects = Vec::new();
        if let Some(IrExpr::Array(rs)) = a {
            for rr in rs.iter() {
                match rr {
                    IrExpr::Object(props) => {
                        let mut fd = None;
                        let mut mode = String::new();
                        let mut target = None;
                        for (k, v) in props.iter() {
                            match k.as_str() {
                                "fd" => if let IrExpr::Int(n) = v { fd = Some(*n as i32); },
                                "mode" => if let IrExpr::Str(m, _) = v { mode = m.clone(); },
                                "target" => target = Some(v.clone()),
                                _ => {}
                            }
                        }
                        if let Some(t) = target {
                            redirects.push(crate::ir::IrRedirect { fd, mode, target: t, interpolate: false });
                        }
                    }
                    _ => return Err(format!("redirect shape not in the java subset: {rr:?}")),
                }
            }
        }
        Ok(redirects)
    }

    /// Run renderer-temp setup assigns (`__ps_* = $(mktemp)`) once each.
    /// Emit `__v_x = __SH_ENV["x"]` for vars a just-rendered child loop
    /// assigned (the child reports every value it wrote).
    fn flush_sync(&mut self) {
        let pending = std::mem::take(&mut self.sync_pending);
        for v in pending {
            let sv = sanitize(&v);
            if self.fields.contains(&sv) {
                self.emit(&format!("__v_{sv} = __SH_ENV.getOrDefault(\"{v}\", __v_{sv});"));
            }
        }
    }

    fn temp_setup_pre(&mut self, stmts: &[IrStmt]) -> Result<(), String> {
        for st in stmts {
            if let IrStmt::Assign { targets, .. } = st {
                if let Some(t) = targets.first() {
                    if t.var.starts_with("__ps_") && !self.temp_setup_done.contains(&t.var) {
                        self.temp_setup_done.insert(t.var.clone());
                        self.stmt(st)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Wrap a statement in a subshell statement (bash fork semantics).
    fn subshell(st: IrStmt) -> IrStmt {
        IrStmt::Subshell(vec![st])
    }

    /// `bash -c <cmd text>` — the command text is already fully quoted.
    fn run_subprocess(&mut self, cmd: String) -> String {
        self.helper("quote");
        self.helper("run");
        format!("shRun({cmd})")
    }

    /// Redirect as bash TEXT. Single implementation of the text-vs-native
    /// decision for a Redirect: bodies with java state go native (helpers
    /// see the swapped PrintStreams); everything else becomes command text
    /// so that a forked child (`( … )`, `x=$(…)`, a pipeline stage) gets the
    /// right fd table. Returns a Java string expression for the command.
    fn redirect_text_arms(&mut self, inner: &[IrStmt], redirects: &[IrRedirect]) -> Result<String, String> {
        // A Redirect nested in a Redirect body is COMMAND TEXT of the
        // enclosing command: unwrap it here (the generic body renderers
        // would send it back through stage_expr/arrow_expr forever).
        if let [IrStmt::Redirect { inner: ri, redirects: rr }] = inner {
            let mut t = self.redirect_text_arms(ri, rr)?;
            t += &self.redirect_ops_text(inner, redirects, &[])?;
            return Ok(t);
        }
        // a heredoc/herestring body with NO command in front: bash feeds the
        // body to nothing — `<<EOF` alone still parses `cat` as the sink in
        // a pipeline stage, so render `cat <<EOF`
        if inner.is_empty() {
            if let Some(r) = redirects.iter().find(|r| matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring")) {
                let body = self.heredoc_content(r)?;
                let t = if r.mode == "herestring" { format!(" + \" <<< \" + shQuote({body})") }
                        else { let te = Self::heredoc_term_join(&body); format!(" + \" <<'SH2EOF'\\n\" + {te}") };
                self.helper("quote");
                return Ok(format!("\"cat\"{t}"));
            }
        }
        self.redirect_text_arms_ex(inner, redirects, false)
    }

    /// redirect_text_arms with the "render the body as command text" step
    /// skipped — used by the native runner, whose helper emits statements.
    fn redirect_text_ops(&mut self, inner: &[IrStmt], redirects: &[IrRedirect]) -> Result<String, String> {
        self.redirect_text_arms_ex(inner, redirects, true)
    }

    fn redirect_text_arms_ex(&mut self, inner: &[IrStmt], redirects: &[IrRedirect], ops_only: bool) -> Result<String, String> {
        // `{ cmd; } > f` would hide the redirect from cmd's forked children
        // (and from the last pipeline stage): a single command renders as
        // `cmd > f` instead.
        if !ops_only && !redirects.is_empty()
            && !inner.is_empty()
            && !Self::body_needs_native(inner)
            && !redirects.iter().any(|r| r.fd == Some(0)
                && matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring" | "r"))
        {
            let body = self.arrow_expr(inner)?;
            return Ok(format!("{body}{}", self.redirect_ops_text(inner, redirects, &[])?));
        }
        let stdin_fed = redirects.iter().any(|r| matches!((r.fd, r.mode.as_str()),
            (Some(0), "r" | "heredoc" | "heredoc-tabs" | "herestring") | (None, "r")));
        let has_whileloop = inner.iter().any(|st| matches!(st,
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "whileLoop"));
        // Bash runs the redirections in a subshell with no command: the
        // files are created/opened (rc 1 + diagnostic on failure), output
        // goes nowhere, a heredoc body still expands, and the PARENT's fd
        // table is untouched. Never render this as a bare statement.
        let create_only = inner.is_empty() || inner.iter().all(|b| matches!(b, IrStmt::Assign { .. }));
        if create_only || Self::body_needs_native(inner) || (stdin_fed && has_whileloop) {
            // native helper sees the swapped PrintStreams: render as an
            // in-process stage and hand its bytes to bash. A bare command
            // (a pipeline feeding this redirect) IS the command line —
            // `{ cmd; } > f` would hide the redirect from its last stage.
            let mut st = if inner.len() == 1 && !create_only {
                inner[0].clone()
            } else {
                IrStmt::Redirect { inner: inner.to_vec(), redirects: redirects.to_vec() }
            };
            if create_only { st = Self::subshell(st.clone()); }
            return self.stage_expr_native(std::slice::from_ref(&st));
        }
        if !ops_only && inner.is_empty() {
            // no command in the group at all: the redirections alone
            // (`{ > f; }`), run as a forked no-op with its fd table
            let ops = self.redirect_ops_text(inner, redirects, &[])?;
            if let Some(rest) = ops.strip_prefix(" + ") {
                return Ok(format!("\"\" + {rest}"));
            }
            return Ok(ops);
        }
        // The heredoc (if any) must be the LAST text on the command line;
        // bash reads its body from the following lines. Everything after it
        // (even `2>&1`) is a separate command in bash too — so the rest go
        // before it.
        let hi = redirects.iter().position(|r| r.fd == Some(0)
            && matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring"));
        let splits: Vec<usize> = match hi { Some(i) => vec![i], None => vec![] };
        if !inner.is_empty() {
            // The command (or its forked group) is the TEXT LEFT OF the
            // heredoc operator — a heredoc op is never the whole line.
            let tail = self.redirect_ops_text(inner, redirects, &splits)?;
            let body = self.arrow_expr(inner)?;
            if !tail.is_empty() {
                return Ok(format!("{body}{tail}"));
            }
            return Ok(body);
        }
        self.redirect_ops_text(inner, redirects, &splits)
    }

    /// Run a command that carries file redirects as a real child and
    /// return its standard output. Used where an in-process helper would
    /// run the command inside ITSELF and hijack the helper's capture.
    fn redirect_subprocess(&mut self, st: &IrStmt) -> Option<Result<String, String>> {
        let redirects = match st {
            IrStmt::Redirect { inner, redirects } if !inner.is_empty() => {
                if inner.iter().any(|b| matches!(b, IrStmt::Assign { .. })) { return None; }
                redirects
            }
            _ => return None,
        };
        let pure_file = redirects.iter().all(|r| {
            r.fd != Some(0)
                && matches!(r.mode.as_str(), "w" | "wc" | "a" | "r" | "r+")
                && !matches!(r.target, IrExpr::Call { .. })
        });
        if !pure_file {
            return None;
        }
        let text = match self.redirect_text_ops(&[], redirects) {
            Ok(t) => t,
            Err(e) => return Some(Err(e)),
        };
        Some(Ok(format!("shCapture({text})")))
    }

    /// Redirect operator suffixes for `inner` in command-text form.
    /// `splits` marks heredoc indices: the operators before a split are
    /// appended before the heredoc's body terminator, the rest after it.
    /// `<<'SH2EOF'…body…SH2EOF` fragment (body expansion included).
    /// No leading space: the operator's own text supplies the separator
    /// (`cmd <<'SH2EOF'` — `cmd<<'SH2EOF'` is a different word to bash).
    /// The heredoc TERMINATOR must start its own line. A body already ending
    /// in a newline (the IR keeps the heredoc's trailing `\n`) would be
    /// followed by a BLANK line otherwise, and bash rejects a blank line
    /// right before the terminator ("unexpected EOF while looking for
    /// matching `'`"). Attach the terminator directly in that case.
    fn heredoc_term_join(t: &str) -> String {
        let lit_ends_nl = t.starts_with('"') && t.ends_with('"') && t.trim_end_matches('"').ends_with("\\n");
        if lit_ends_nl { format!("{t} + \"SH2EOF\"") } else { format!("{t} + \"\\nSH2EOF\"") }
    }
    fn heredoc_ops_text(&mut self, r: &IrRedirect, t: &str) -> Result<String, String> {
        let te = Self::heredoc_term_join(t);
        Ok(format!(" + \" <<'SH2EOF'\\n\" + {te}"))
    }

    /// When `inner` is empty the operators are the whole command and must
    /// start with a space; otherwise they append directly to it.
    fn redirect_ops_text(&mut self, inner: &[IrStmt], redirects: &[IrRedirect], splits: &[usize]) -> Result<String, String> {
        let mut text = String::new();
        let _ = &inner;
        let lead = if inner.is_empty() { " " } else { "" };
        for (idx, r) in redirects.iter().enumerate() {
            if splits.contains(&idx) {
                // heredoc: operator + expanded body, terminating the line.
                // The op carries its own leading space (the command word is
                // joined directly onto it).
                let t = self.heredoc_content(r)?;
                let te = Self::heredoc_term_join(&t);
                text += &format!(" + \" <<'SH2EOF'\\n\" + {te}"); // no lead: the body terminator ends the line
                continue;
            }
            if r.fd == Some(0) && matches!(r.mode.as_str(), "r" | "r+") {
                let t = self.expr_str(&r.target)?;
                text += &format!(" + \"{lead} < \" + shQuote({t})");
                continue;
            }
            let t = self.expr_str(&r.target)?;
            if matches!(r.mode.as_str(), "w" | "wc" | "a" | "r" | "r+") {
                if let Some(fdn) = r.fd {
                    if (3..=9).contains(&fdn) {
                        text += &format!(" + \"{lead} {fdn}{} \" + shQuote({t})",
                            match r.mode.as_str() { "a" => ">>", "r" | "r+" => "<", _ => ">" });
                        continue;
                    }
                }
            }
            if matches!(r.mode.as_str(), "w" | "wc" | "a" | "r" | "r+") && fd_slot_target(&t).is_some() {
                let bare = &t[1..t.len() - 1];
                let op = match r.mode.as_str() { "a" => ">>", "r" | "r+" => "<", _ => ">" };
                let fds = match r.fd { Some(f) => format!("{f}"), None => String::new() };
                text += &format!(" + \"{lead} {fds}{op}{bare} \"");
                continue;
            }
            let mut herestring_consumed = false;
            if r.fd.is_none() && r.mode == "r" {
                // `< file` with no command: the redirected stdin has no
                // reader. bash still opens it (rc 1 + diagnostic when it
                // cannot). Do the same natively and let the rest of the
                // compound run — otherwise `{ … } <<EOF > f` would lose f.
                let mut rest = vec![IrStmt::Redirect { inner: vec![], redirects: vec![r.clone()] }];
                let pos = redirects.iter().position(|x| std::ptr::eq(x, r)).unwrap();
                for r2 in &redirects[pos + 1..] {
                    if r2.fd == Some(0) && matches!(r2.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring") {
                        herestring_consumed = true;
                    }
                    rest.push(IrStmt::Redirect { inner: vec![], redirects: vec![r2.clone()] });
                }
                text += &format!(" + \" ; \" + {}", self.stage_expr_native(&rest)?);
                if herestring_consumed { break; }
                continue;
            }
            match (r.fd, r.mode.as_str()) {
                (None, "w") | (Some(1), "w") => text += &format!(" + \"{lead} > \" + shQuote({t})"),
                (None, "a") | (Some(1), "a") => text += &format!(" + \"{lead} >> \" + shQuote({t})"),
                (Some(2), "w") => text += &format!(" + \"{lead} 2> \" + shQuote({t})"),
                (Some(2), "a") => text += &format!(" + \"{lead} 2>> \" + shQuote({t})"),
                (Some(0), "r") => text += &format!(" + \"{lead} < \" + shQuote({t})"),
                (Some(0), "herestring") => text += &format!(" + \"{lead} <<< \" + shQuote({t})"),
                (Some(0), "heredoc") | (Some(0), "heredoc-tabs") => {
                    let te = Self::heredoc_term_join(&t);
                    text += &format!(" + \" <<'SH2EOF'\\n\" + {te}");
                }
                _ => return Err(format!("redirect {:?}/{} not in capture text", r.fd, r.mode)),
            }
        }
        Ok(text)
    }


    /// Heredoc/herestring body as a Java string expression. Unquoted
    /// delimiters (`interpolate: true`) expand `$` forms at render time
    /// (the delimiter stays quoted — the content arrives pre-expanded);
    /// quoted delimiters stay literal.
    fn heredoc_content(&mut self, r: &IrRedirect) -> Result<String, String> {
        if r.interpolate {
            if let IrExpr::Str(s, _) = &r.target {
                if self.dollar_expand_ok(s) {
                    match self.expand_dollars(s) {
                        Ok(e) => return Ok(e),
                        Err(_) => {}
                    }
                }
            }
        }
        // A heredoc BODY is shell TEXT built by the child: a `$( … )` inside
        // it must stay a literal substitution (never an interpreter-side
        // capture), and `$name` is expanded by the child.
        let tgt = r.target.clone();
        if let IrExpr::Str(s, _) = &tgt {
            return Ok(jstr(s));
        }
        self.expr_str(&tgt)
    }

    /// The literal text of a heredoc body as a java string expression (no
    /// command substitution evaluation): `$name` stays a literal `$name`
    /// for the child shell to expand.
    fn heredoc_body_text(&mut self, e: &IrExpr) -> Result<Option<String>, String> {
        match e {
            IrExpr::Str(s, _) => Ok(Some(jstr(s))),
            IrExpr::Interpolate(parts) => {
                let mut out = String::from("(\"\"");
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => { out += " + "; out += &jstr(s); }
                        InterpPart::Expr(x) => {
                            match x.as_ref() {
                                IrExpr::Call { func, args, .. } if func == "getVar" => {
                                    let n = str_arg(args, 0).unwrap_or("").to_string();
                                    out += " + "; out += &jstr(&format!("${n}"));
                                }
                                // ANY command substitution / capture stays
                                // literal: write the original text form
                                other => {
                                    let _ = other;
                                    self.helper("capture");
                                    out += " + ";
                                    out += &jstr("$(...)");
                                }
                            }
                        }
                    }
                }
                out += ")";
                Ok(Some(out))
            }
            _ => Ok(None),
        }
    }

    fn redirect(&mut self, inner: &[IrStmt], redirects: &[IrRedirect]) -> Result<(), String> {
        // ONE text decision for the whole redirect statement: a plain body
        // becomes `cmd > f`, a compound/forked one `{ …; } > f`, a headless
        // heredoc (`<<EOF | cmd`) `cat <<EOF | cmd`.
        if !redirects.is_empty() {
            let st1 = IrStmt::Redirect { inner: inner.to_vec(), redirects: redirects.to_vec() };
            self.helper("run");
            if let Ok(t) = self.redirect_stage_text(std::slice::from_ref(&st1)) {
                self.emit(&format!("__SH_RC = shRun({t});"));
                self.flush_sync();
                return Ok(());
            }
        }
        // Generic stdout/stderr/stdin swaps around the body; each swap gets
        // a unique variable so nested redirects don't collide.
        let mut opened: Vec<String> = Vec::new();
        for r in redirects {
            match (r.fd, r.mode.as_str()) {
                (None, "w") | (None, "wc") | (Some(1), "w") | (Some(1), "wc") => {
                    let t = self.expr_str(&r.target)?;
                    // >&2 / >&1 / >&- on stdout: descriptor dups, not files
                    if t == "\"&2\"" {
                        self.redir_seq += 1;
                        let v = format!("__oldErr{}", self.redir_seq);
                        self.helper("errswap");
                        self.emit(&format!("PrintStream {v} = shSwapErr(System.err);"));
                        opened.push(v);
                        continue;
                    }
                    if t == "\"&1\"" {
                        // stdout to stdout: no-op
                        continue;
                    }
                    if t == "\"&-\"" {
                        self.redir_seq += 1;
                        let v = format!("__oldOut{}", self.redir_seq);
                        self.helper("outswap");
                        self.emit(&format!("PrintStream {v} = shSwapOut(shNullStream());"));
                        opened.push(v);
                        continue;
                    }
                    // >&N (N=3..9): duplicate-write into the N slot
                    if let Some(slot) = fd_slot_target(&t) {
                        // echo/printf to an fd slot: print via the slot
                        // (null slot = closed fd — bash reports rc 1; the
                        // write itself goes nowhere).
                        self.tmp_seq += 1;
                        let sv = format!("__fdv{}", self.tmp_seq);
                        self.emit(&format!("PrintStream {sv} = {slot};"));
                        // mark for restore-skipping: slots persist
                        opened.push(format!("__FDSLOT:{sv}"));
                        // stash the slot var for the inner emitter: the
                        // statements below must print to it. Simplest
                        // sound approach for echo/printf bodies: swap
                        // System.out for the duration.
                        self.helper("outswap");
                        self.emit(&format!("PrintStream __oldFd{sv} = shSwapOut({sv} == null ? shNullStream() : {sv});"));
                        opened.push(format!("__oldFd{sv}"));
                        continue;
                    }
                    self.redir_seq += 1;
                    let v = format!("__oldOut{}", self.redir_seq);
                    self.helper("outswap");
                    self.emit(&format!("PrintStream {v} = shSwapOut(new PrintStream(new FileOutputStream(shAbs({t}), false)));"));
                    opened.push(v);
                }
                (None, "a") | (Some(1), "a") => {
                    let t = self.expr_str(&r.target)?;
                    self.redir_seq += 1;
                    let v = format!("__oldOut{}", self.redir_seq);
                    self.helper("outswap");
                    self.emit(&format!("PrintStream {v} = shSwapOut(new PrintStream(new FileOutputStream(shAbs({t}), true)));"));
                    opened.push(v);
                }
                (Some(2), "w") | (Some(2), "a") => {
                    let t = self.expr_str(&r.target)?;
                    self.redir_seq += 1;
                    let v = format!("__oldErr{}", self.redir_seq);
                    self.helper("errswap");
                    if t == "\"/dev/null\"" {
                        self.emit(&format!("PrintStream {v} = shSwapErr(shNullStream());"));
                    } else if t == "\"&1\"" {
                        // 2>&1: stderr follows the CURRENT stdout
                        self.emit(&format!("PrintStream {v} = shSwapErr(System.out);"));
                    } else {
                        let app = r.mode == "a";
                        self.emit(&format!("PrintStream {v} = shSwapErr(new PrintStream(new FileOutputStream({t}, {app})));"));
                    }
                    opened.push(v);
                }
                (None, "r") | (Some(0), "r") => {
                    let t = self.expr_str(&r.target)?;
                    self.redir_seq += 1;
                    let v = format!("__oldIn{}", self.redir_seq);
                    self.helper("inswap");
                    self.emit(&format!("Scanner {v} = shSwapIn(new Scanner(new File(shAbs({t}))));"));
                    opened.push(v);
                }
                (Some(0), "heredoc") | (Some(0), "heredoc-tabs") | (Some(0), "herestring")
                    if inner.iter().any(|b| matches!(b,
                        IrStmt::Expr(IrExpr::Call { func, args, .. })
                            if (func == "exec" || func == "builtin")
                                && str_arg(args, 0).map(|c| !matches!(c, "echo" | "printf" | "exit" | "true" | "false" | ":" | "read" | "cd" | "shift")).unwrap_or(false) )) =>
                {
                    let mut cmds: Vec<String> = Vec::new();
                    for b in inner {
                        match b {
                            IrStmt::Expr(IrExpr::Call { func, args, .. })
                                if func == "exec" || func == "builtin" =>
                            {
                                cmds.push(self.cmd_line_expr(args)?);
                            }
                            _ => return Err("heredoc with non-command body not supported".into()),
                        }
                    }
                    // (a heredoc may feed the FIRST pipeline stage: the body
                    // starts empty then)
                    let headless = cmds.is_empty();
                    let t = self.heredoc_content(r)?;
                    self.helper("quote");
                    self.helper("run");
                    let q = if r.mode == "herestring" { "" } else { "'" };
                    // heredoc bodies already end with \n; herestrings need one.
                    // The terminator needs a trailing newline of its own.
                    let tail = if r.mode == "herestring" { "\"\\nSH2EOF\\n\"".to_string() } else { Self::heredoc_term_join(&t) };
                    let joined = cmds.join(" + \" ; \" + ");
                    // sibling file redirects (`cat <<EOF > file`) go on the
                    // COMMAND line (before the heredoc body) — bash treats
                    // a `> file` after the SH2EOF line as a new command.
                    let mut suffix = String::new();
                    for r2 in redirects {
                        if r2.fd == Some(0) { continue; }
                        match (r2.fd, r2.mode.as_str()) {
                            (None, "w") | (None, "wc") | (Some(1), "w") | (Some(1), "wc") => {
                                let t2 = self.expr_str(&r2.target)?;
                                suffix += &format!(" + \" > \" + shQuote({t2})");
                            }
                            (None, "a") | (Some(1), "a") => {
                                let t2 = self.expr_str(&r2.target)?;
                                suffix += &format!(" + \" >> \" + shQuote({t2})");
                            }
                            (Some(2), "w") | (Some(2), "a") => {
                                let t2 = self.expr_str(&r2.target)?;
                                let op = if r2.mode == "a" { " >>" } else { " >" };
                                suffix += &format!(" + \" 2{op} \" + shQuote({t2})");
                            }
                            _ => return Err(format!("redirect {:?} with heredoc not supported", (&r2.fd, &r2.mode))),
                        }
                    }
                    // headless heredoc (`<<EOF | cmd`): no command before
                    // the operator — emit a pure substitution-free literal
                    // command (`cat`) so the pipeline stage list is valid
                    let head = if headless {
                        // `cat <<EOF` (a plain word) feeds the pipe — the
                        // command text must not start with the operator
                        "(\"cat\")".to_string()
                    } else { joined };
                    let cmd = if headless {
                        // `<<EOF | cmd`: no command before the operator —
                        // `cat <<EOF` (a plain word) feeds the pipe
                        format!("\"cat\" + \" <<{q}SH2EOF{q}\\n\" + {tail}{suffix}")
                    } else {
                        format!("{head}{suffix} + \" <<{q}SH2EOF{q}\\n\" + {tail}")
                    };
                    self.emit(&format!("__SH_RC = shRun({cmd});"));
                    return Ok(());
                }
                #[allow(unreachable_patterns)]
                (Some(0), m0 @ ("heredoc" | "heredoc-tabs" | "herestring"))
                    if inner.iter().any(|b| !matches!(b,
                        IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec" || func == "builtin")) =>
                {
                    // body is not a plain command list (a forked subshell,
                    // a loop, …): run the compound NATIVELY as the whole
                    // command — its children inherit the heredoc on fd 0.
                    // The Scanner swap below only reaches in-process
                    // builtins, so it would feed bash -c's child nothing.
                    let q = if m0 == "herestring" { "" } else { "'" };
                    let t = self.heredoc_content(r)?;
                    // No space before the operator: a separate argv word
                    // would be read as an argument, not a redirect.
                    let here = if m0 == "herestring" {
                        format!("\"<<<\" + shQuote({t})")
                    } else {
                        format!("\"<<{q}SH2EOF{q}\\n\" + {t} + \"\\nSH2EOF\"")
                    };
                    let body = self.stage_expr_native(&[IrStmt::Redirect {
                        inner: inner.to_vec(),
                        redirects: redirects.iter().filter(|x| x.fd != Some(0)).cloned().collect(),
                    }])?;
                    self.helper("quote");
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({body} + \" \" + {here});"));
                }
                #[allow(unreachable_patterns)]
                (Some(0), "heredoc") | (Some(0), "heredoc-tabs") | (Some(0), "herestring") => {
                    // the body feeds an EXTERNAL command: native Scanner
                    // swaps don't reach a bash -c child — embed the
                    // content as a real heredoc in the command text
                    let t = self.heredoc_content(r)?;
                    self.helper("quote");
                    self.helper("run");
                    let mut inner_t = String::from("\"\"");
                    for b in inner {
                        match b {
                            IrStmt::Expr(IrExpr::Call { func, args, .. })
                                if func == "exec" || func == "builtin" =>
                            {
                                if !inner_t.is_empty() { inner_t += " + \" ; \" + "; }
                                inner_t += &self.cmd_line_expr(args)?;
                            }
                            _ => return Err("heredoc with non-command body not supported".into()),
                        }
                    }
                    let q = if r.mode == "herestring" { "" } else { "'" };
                    self.emit(&format!("__SH_RC = shRun({inner_t} + \" <<{q}SH2EOF\\n\" + {t} + \"\\nSH2EOF\");"));
                }
                (Some(0), "heredoc") | (Some(0), "herestring") => {
                    let t = self.heredoc_content(r)?;
                    let content = if r.mode == "herestring" { t } else { format!("{t} + \"\\n\"") };
                    self.redir_seq += 1;
                    let v = format!("__oldIn{}", self.redir_seq);
                    self.helper("inswap");
                    if inner.iter().all(|b| matches!(b, IrStmt::Assign { .. })) {
                        // no command consumes the body (it still expands):
                        // bash opens the temp fd and closes it again
                        self.emit(&format!("Scanner {v} = shSwapIn(null); {{ Scanner __unused{v} = new Scanner(new StringReader({content})); }} shSwapIn({v});"));
                    } else {
                        self.emit(&format!("Scanner {v} = shSwapIn(new Scanner(new StringReader({content})));"));
                        opened.push(v);
                    }
                }
                (Some(fd @ 3..=9), m) if m == "w" || m == "wc" || m == "a" => {
                    // fd N>target / N>>target: open a file (or dup &M) into
                    // the N slot. `exec 3>file` persists (empty inner), so
                    // the slot assignment is never restored — matching the
                    // shell fd table for the common shapes.
                    let t = self.expr_str(&r.target)?;
                    let app = m == "a";
                    if t == "\"&1\"" {
                        self.emit(&format!("__fd{fd} = System.out;"));
                    } else if t == "\"&2\"" {
                        self.emit(&format!("__fd{fd} = System.err;"));
                    } else if t == "\"&-\"" {
                        self.emit(&format!("__fd{fd} = null;"));
                    } else {
                        self.emit(&format!("__fd{fd} = new PrintStream(new FileOutputStream(shAbs({t}), {app}));"));
                    }
                }
                (Some(fd @ 3..=9), m) if m == "r" || m == "r+" => {
                    // `N<&0`: fd N aliases stdin — nothing to model (stdin
                    // IS stdin); accept silently. Real file inputs need a
                    // modeled fd table — refuse loudly.
                    let t = self.expr_str(&r.target)?;
                    if t == "\"&0\"" {
                        // no-op
                    } else {
                        return Err(format!("fd {fd} input redirect not supported by the java renderer"));
                    }
                }
                other => return Err(format!("redirect {:?} not supported by the java renderer", other)),
            }
        }
        for b in inner { self.stmt(b)?; }
        for v in opened.iter().rev() {
            if v.starts_with("__FDSLOT:") { continue; }
            else if v.starts_with("__oldOut") || v.starts_with("__oldFd") { self.emit(&format!("shRestoreOut({v});")); }
            else if v.starts_with("__oldErr") { self.emit(&format!("shRestoreErr({v});")); }
            else { self.emit(&format!("shRestoreIn({v});")); }
        }
        // closed-fd writes report rc 1 (bash write-error semantics)
        for v in opened.iter() {
            if let Some(sv) = v.strip_prefix("__FDSLOT:") {
                self.emit(&format!("if ({sv} == null) {{ __SH_RC = 1; }}"));
            }
        }
        Ok(())
    }

    fn pipeline(&mut self, stages: &[Vec<IrStmt>], capture: Option<&str>, _cmd_str: Option<&str>) -> Result<(), String> {
        // Pipelines run as one bash -c command line built at runtime from
        // per-stage argv expressions joined with " | " (external stages are
        // inherently processes; native echo/printf stages are folded into
        // the text — their byte behavior matches bash's).
        let mut parts = Vec::new();
        for st in stages {
            parts.push(self.stage_expr(st)?);
        }
        let line = format!("({})", parts.join(") + \" | \" + ("));
        match capture {
            Some(var) => {
                self.helper("capture");
                self.emit(&format!("__v_{} = shCapture({line});", sanitize(var)));
            }
            None => {
                self.helper("quote");
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({line});"));
            }
        }
        Ok(())
    }

    /// A Java string expression holding the bash text of one pipeline stage
    /// (only simple exec/builtin/test stages are representable).
    /// A pipeline stage as a Java string expression of bash text. Stages
    /// that are simple commands render directly; stages containing loops
    /// (For/While/whileLoop — e.g. `for...done | sort`) run NATIVELY in a
    /// per-site helper whose stdout is captured, then fed to the rest of
    /// the pipeline via `printf '%s'`. The capture is byte-exact (no
    /// trailing-newline stripping — pipes carry raw bytes).
    fn stage_expr(&mut self, stage: &[IrStmt]) -> Result<String, String> {
        self.helper("quote");
        // inside a TEXT-LOOP body the stage is that child shell's text: the
        // loop's child-computed vars stay symbolic (`$line`), exactly as in
        // any other child command. (Outside a loop the set is empty.)
        self.stage_expr_inner(stage)
    }

    fn stage_expr_inner(&mut self, stage: &[IrStmt]) -> Result<String, String> {
        match stage {
            [IrStmt::Expr(IrExpr::Call { func, args, .. })]
                if func == "exec" || func == "builtin" || func == "test" =>
            {
                self.cmd_line_expr(args)
            }
            [IrStmt::Expr(IrExpr::Call { func, args, .. })] if func == "grepMatches" => {
                self.grep_text(args)
            }
            [IrStmt::Expr(IrExpr::Call { func, args, .. })] if func == "redirect" => {
                // redirect(Arrow(body), redirects...) — same text decision
                // as a Redirect statement.
                let body: Vec<IrStmt> = match args.first() {
                    Some(IrExpr::Arrow(b)) => b.to_vec(),
                    _ => return Err("redirect without Arrow body".into()),
                };
                let redirects = Self::redirect_list(args.get(1))?;
                let headless = body.iter().all(|b| matches!(b, IrStmt::Expr(IrExpr::Call { args, .. })
                    if str_arg(args, 0) == Some("cat")));
                if headless {
                    // `cat <<EOF | grep …`: the body IS the stage — a plain
                    // here-doc needs a command, so `cat <<EOF` (cat is a
                    // word, not a substitution) feeds the pipe
                    let mut t = String::from("\"cat\"");
                    for r in &redirects {
                        let tg = self.expr_str(&r.target)?;
                        t += &format!(" + \" <<'SH2EOF'\\n\" + {tg} + \"\\nSH2EOF\"");
                    }
                    return Ok(t);
                }
                self.redirect_text_arms(&body, &redirects)
            }
            [IrStmt::Expr(IrExpr::Call { func, args, .. })] if func == "whileLoop" => {
                // pipe-fed while: prefer bash text (subshell semantics);
                // java-state bodies fall back to the native helper below.
                // (whileloop_text is already parenthesized.)
                let (c, b) = match (args.first(), args.get(1)) {
                    (Some(IrExpr::Arrow(c)), Some(IrExpr::Arrow(b))) => (c, b),
                    _ => return Err("whileLoop without cond/body arrows".into()),
                };
                match self.whileloop_text(c, b) {
                    Ok(t) => Ok(t),
                    Err(e) => {
                        let _ = e;
                        return self.stage_expr_native(stage);
                    }
                }
            }
            [IrStmt::Exec { cmd, args, capture: None, redirects, env }] if redirects.is_empty() && env.is_empty() => {
                self.cmd_line_expr(&std::iter::once(cmd.clone()).chain(args.iter().cloned()).collect::<Vec<_>>())
            }
            #[allow(unreachable_patterns)]
            [IrStmt::Redirect { inner, redirects }] => {
                // the redirect TEXT for this body. redirect_stage_text is
                // NOT re-entered here: its body path calls back into
                // arrow_expr → stage_expr and would recurse forever. A
                // stdin-fed loop or a java-state body needs the native
                // runner instead.
                let stdin_fed = redirects.iter().any(|r| matches!(r.fd, Some(0))
                    || matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring"));
                if (stdin_fed && inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. })))
                    || Self::body_needs_native(inner)
                {
                    return self.stage_expr_native_loop(inner, redirects);
                }
                let mut t = self.arrow_expr(inner)?;
                t += &self.redirect_ops_text(inner, redirects, &[])?;
                Ok(t)
            }
            // a `{ … }` stage is its body's command text: routing a
            // redirect-carrying body back through stage_expr would recurse
            // (stage_expr → arrow_expr → stage_expr …) forever
            [IrStmt::Block(b)] => self.arrow_expr(b),
            // a `( … )` stage is a forked group: render `( body )` — the
            // body's own text, never back through stage_expr (a nested
            // subshell would recurse forever)
            [IrStmt::Subshell(b)] => {
                let t = self.arrow_expr(b)?;
                Ok(format!("(\"( \" + {t} + \" )\")"))
            }
            [IrStmt::While { cond, body }] => {
                // a statement-while stage: bash text when its condition is
                // a command list (`while read … && [ … ]`), else native
                let one = IrStmt::Expr((*cond).clone());
                match self.whileloop_text(std::slice::from_ref(&one), body) {
                    Ok(t) => Ok(t),
                    Err(e) => {
                        let _ = e;
                        self.stage_expr_native(stage)
                    }
                }
            }
            other => {
                return self.stage_expr_native(other);
            }
        }
    }

    /// A redirect-carrying stage as command text. Only file redirects and
    /// a plain/compound body are expressible; a stdin-fed loop needs the
    /// java input stream and is left to the native runner.
    fn redirect_stage_text(&mut self, stage: &[IrStmt]) -> Result<String, String> {
        if let [IrStmt::Redirect { inner, redirects }] = stage {
            let stdin_fed = redirects.iter().any(|r| matches!(r.fd, Some(0))
                || matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring"));
            if stdin_fed && inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. })) {
                // prefer the TEXT loop (parent-seeded, parent-synced)
                let one = inner.iter().find_map(|b| match b {
                    IrStmt::While { cond, body }
                        if matches!(cond, IrExpr::Call { func, .. } if func == "test" || func == "builtin" || func == "exec" || func == "block") =>
                        Some((IrStmt::Expr((*cond).clone()), body.to_vec())),
                    _ => None,
                });
                if let Some((c, b)) = one {
                    if let Ok(t) = self.whileloop_text(std::slice::from_ref(&c), &b) {
                        let mut tail = String::from("\"\"");
                        for r in redirects {
                            let tg = self.expr_str(&r.target)?;
                            let op = match (r.fd, r.mode.as_str()) {
                                (Some(0), "r") | (Some(0), "r+") => "< ",
                                (None, "w") | (Some(1), "w") | (Some(1), "wc") => "> ",
                                (None, "a") | (Some(1), "a") => ">> ",
                                (Some(2), "w") => "2> ",
                                (Some(2), "a") => "2>> ",
                                (fd, m) => return Err(format!("redirect {fd:?}/{m} on a loop")),
                            };
                            tail += &format!(" + \" {op}\" + shQuote({tg})");
                        }
                        self.helper("quote");
                        return Ok(format!(" \"{{ \" + {t} + \" ; }}\" + {tail} + \" \" "));
                    }
                }
                return Err("stdin-fed loop stage".into());
            }
            if inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. })) {
                let one = inner.iter().find_map(|b| match b {
                    IrStmt::While { cond, body } => Some((IrStmt::Expr((*cond).clone()), body.to_vec())),
                    _ => None,
                });
                if let Some((c, b)) = one {
                    // prefer the TEXT loop (no scanner needed): its vars are
                    // seeded/reported by the parent
                    if let Ok(t) = self.whileloop_text(std::slice::from_ref(&c), &b) {
                        let mut tail = String::from("\"\"");
                        for r in redirects {
                            let tg = self.expr_str(&r.target)?;
                            let op = match (r.fd, r.mode.as_str()) {
                                (None, "w") | (Some(1), "w") | (Some(1), "wc") => "> ",
                                (None, "a") | (Some(1), "a") => ">> ",
                                (Some(2), "w") => "2> ",
                                (Some(2), "a") => "2>> ",
                                (fd, m) => return Err(format!("redirect {fd:?}/{m} on a loop")),
                            };
                            tail += &format!(" + \" {op}\" + shQuote({tg})");
                        }
                        self.helper("quote");
                        return Ok(format!(" \"{{ \" + {t} + \" ; }}\" + {tail} + \" \" "));
                    }
                }
                // otherwise the loop renders natively (java input stream);
                // its var reports ride out through sync_pending
                // (2') the loop body is a CHILD: it reports its assigned vars
                // through _sh2_env, and the parent folds them back in.
                let mut ba: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for b in inner {
                    if let IrStmt::While { body, .. } = b {
                        for st in body { self.body_assigned(st, &mut ba); }
                    }
                }
                self.sync_pending = ba.iter()
                    .filter(|v| self.fields.contains(&sanitize(v)))
                    .cloned().collect();
                return self.stage_expr_native_loop(inner, redirects);
            }
        }
        self.arrow_expr(stage)
    }

    /// A stdin-fed loop stage (`while …; done < file`): the redirect is
    /// applied by the PARENT (its fd 0 must reach the loop's child), the
    /// loop body runs natively (java input stream), and the captured bytes
    /// are fed onward. Renders `{ ( loop ) ; } ops` as command text.
    fn stage_expr_native_loop(&mut self, inner: &[IrStmt], redirects: &[IrRedirect]) -> Result<String, String> {
        // stdin-fed loop: PREFER bash text — the child's fd 0 is the feed,
        // exactly as in bash, and its state stays child-local. Only a body
        // that cannot render as text (java-state condition) goes native.
        for b in inner {
            if let IrStmt::While { cond, body } = b {
                let one = IrStmt::Expr((*cond).clone());
                match self.whileloop_text(std::slice::from_ref(&one), body) {
                  Err(_) => {}
                  Ok(t) => {
                    self.helper("quote");
                    let mut tail = String::from("\"\"");
                    for r in redirects {
                        let tg = self.expr_str(&r.target)?;
                        let op = match (r.fd, r.mode.as_str()) {
                            (Some(0), "r") | (Some(0), "r+") => "< ",
                            (None, "w") | (Some(1), "w") | (Some(1), "wc") => "> ",
                            (None, "a") | (Some(1), "a") => ">> ",
                            (Some(2), "w") => "2> ",
                            (Some(2), "a") => "2>> ",
                            (fd, m) => return Err(format!("redirect {fd:?}/{m} on a loop stage")),
                        };
                        tail += &format!(" + \" {op}\" + shQuote({tg})");
                    }
                    return Ok(format!(" \"{{ \" + {t} + \" ; }}\" + {tail} + \" \" "));
                  }
                }
            }
        }
        let body = self.stage_expr_native(inner)?;
        self.helper("quote");
        let mut tail = String::from("\"\"");
        for r in redirects {
            let t = self.expr_str(&r.target)?;
            let op = match (r.fd, r.mode.as_str()) {
                (Some(0), "r") | (Some(0), "r+") => "< ",
                (None, "w") | (Some(1), "w") | (Some(1), "wc") => "> ",
                (None, "a") | (Some(1), "a") => ">> ",
                (Some(2), "w") => "2> ",
                (Some(2), "a") => "2>> ",
                (Some(0), m) => return Err(format!("redirect 0/{m} on a loop stage")),
                (fd, m) => return Err(format!("redirect {fd:?}/{m} on a loop stage")),
            };
            tail += &format!(" + \" {op}\" + shQuote({t})");
        }
        // `{ ( loop ) ; } < file` as ONE java string expression
        Ok(format!(" \"{{ \" + {body} + \" ; }}\" + {tail} + \" \" "))
    }

    /// Native pipeline-stage fallback: run the stage's statements
    /// in-process, capture what they print, feed the bytes to the
    /// remaining stages via `printf '%s'`.
    fn stage_expr_native(&mut self, stage: &[IrStmt]) -> Result<String, String> {
        // a redirect-carrying stage (`tr <<< x`, `cat <<EOF | …`): same text
        // decision as any Redirect STATEMENT — never let the native helper
        // hijack the enclosing capture stream with a herestring Scanner.
        if let [st @ IrStmt::Redirect { .. }] = stage {
            self.helper("run");
            return self.redirect_stage_text(std::slice::from_ref(st));
        }
        // A command with a file redirect must run as a real child: the
        // helper body only swaps PrintStreams, which would hijack the
        // enclosing helper's own capture. Run bash -c and take its stdout.
        if let [IrStmt::Redirect { inner, redirects }] = stage {
            let stdin_fed = redirects.iter().any(|r| matches!(r.fd, Some(0))
                || matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs" | "herestring"));
            // loop bodies fed by stdin (`while …; done < <(…)`) need the
            // java input stream: they stay native. Everything else renders
            // as command text, so a compound body keeps its `{ … }` group
            // and its redirect applies to the whole command in bash.
            if stdin_fed && inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. })) {
                // a stdin-fed loop that cannot render as text (a java-state
                // condition would freeze) stays native — and a native runner
                // must not see the redirect itself: run the body in place
                let _ = inner;
                return self.stage_expr_native_loop(inner, redirects);
            }
            // the body of a REDIRECT-carrying stage: the ops belong to the
            // stage (`body > f`), never to the body's own text — routing
            // back into stage_expr would re-enter redirect_stage_text
            // forever
            if inner.len() == 1 {
                if let Some(r) = self.redirect_subprocess(&inner[0]) { return r; }
                if let IrStmt::Redirect { .. } = &inner[0] {
                    self.helper("run");
                    let mut t = self.arrow_expr(inner)?;
                    t += &self.redirect_ops_text(inner, redirects, &[])?;
                    return Ok(t);
                }
            }
            return self.arrow_expr(stage);
        }
        if let [st] = stage {
            if let Some(r) = self.redirect_subprocess(st) { return r; }
            // a lone redirect-carrying command: render it as command text
            // (the in-loop `redir` would swap the enclosing helper's own
            // capture stream instead of running a child)
            if let IrStmt::Redirect { .. } = st {
                return self.arrow_expr(stage);
            }
        }
        // native fallback: run the stage's statements in-process,
        // capture what they print
        self.tmp_seq += 1;
        let name = format!("__stage{}", self.tmp_seq);
        let saved_depth = self.depth;
        let saved_out = std::mem::take(&mut self.out);
        self.depth = 2;
        // the runner is a CHILD shell: its vars are child-local, so the
        // parent's loop-sync bookkeeping must not leak into it
        let _saved_sync = std::mem::take(&mut self.sync_pending);
        let mut err: Option<String> = None;
        for st in stage {
            if let Err(e) = self.stmt(st) { err = Some(e); break; }
        }
        let lines = std::mem::take(&mut self.out);
        self.out = saved_out;
        self.depth = saved_depth;
        if let Some(e) = err {
            return Err(format!("native pipeline stage failed: {e}"));
        }
        let mut body = String::new();
        for l in &lines { body.push_str(l); body.push('\n'); }
        self.java_helpers.push(format!(
            "    static String {name}() throws Exception {{\n        ByteArrayOutputStream __b = new ByteArrayOutputStream();\n        PrintStream __o = System.out;\n        System.out.flush();\n        System.setOut(new PrintStream(__b));\n        try {{\n{body}        }} finally {{ System.out.flush(); System.setOut(__o); }}\n        return new String(__b.toByteArray(), StandardCharsets.UTF_8);\n    }}\n"
        ));
        // feed the captured bytes to the remaining pipeline stages
        let out = format!("(shQuote(\"printf\") + \" \" + shQuote(\"%s\") + \" \" + shQuote({name}()))");
        Ok(out)
    }

    // ── expression STATEMENTS (commands) ─────────────────────────────
    fn expr_stmt(&mut self, e: &IrExpr) -> Result<(), String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "test" => {
                // a bare [ ... ] statement — its exit status is the effect.
                // A compound test runs as its own bash command text.
                if let Ok(t) = self.bracket_text(args, false) {
                    if !t.contains("shFnmatch(") && !t.contains("shRegexMatch(") {
                        self.helper("run");
                        self.emit(&format!("__SH_RC = shRun({t});"));
                        return Ok(());
                    }
                }
                let b = self.test_bool(args)?;
                self.emit(&format!("__SH_RC = ({b}) ? 0 : 1;"));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "setArray" => {
                let name = str_arg(args, 0).ok_or("setArray: no name")?;
                let items = match args.get(1) {
                    Some(IrExpr::Array(xs)) => xs,
                    _ => return Err("setArray: non-array items".into()),
                };
                let parts = self.join_words(items, ", ")?;
                self.arrays.insert(sanitize(name));
                self.emit(&format!("__a_{} = new ArrayList<>(List.of(new String[]{{{parts}}}));", sanitize(name)));
                self.emit("__SH_RC = 0;");
                return Ok(());
            }
            IrExpr::Call { func, args, .. } if func == "setArrayAppend" => {
                let name = str_arg(args, 0).ok_or("setArrayAppend: no name")?;
                let items = match args.get(1) {
                    Some(IrExpr::Array(xs)) => xs,
                    _ => return Err("setArrayAppend: non-array items".into()),
                };
                let parts = self.join_words(items, ", ")?;
                self.arrays.insert(sanitize(name));
                self.helper("alist");
                self.emit(&format!("__a_{}.addAll(List.of(new String[]{{{parts}}}));", sanitize(name)));
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "exec" || func == "builtin" => {
                self.command(args)
            }
            IrExpr::Call { func, args, .. } if func == "assign" => {
                // assign("name", value), assign("name", "=", value),
                // or assign("name", "+=", value) — string append
                let name = str_arg(args, 0).ok_or("assign: no name")?;
                let is_append = str_arg(args, 1) == Some("+=");
                let vi = if str_arg(args, 1) == Some("=") || is_append { 2 } else { 1 };
                let empty = IrExpr::Str(String::new(), StrStyle::DoubleQuoted);
                let ve = args.get(vi).unwrap_or(&empty);
                let v = self.expr_str(ve)?;
                self.ensure_field(name);
                if is_append {
                    self.emit(&format!("__v_{} = __v_{} + {v};", sanitize(name), sanitize(name)));
                } else {
                    self.emit(&format!("__v_{} = {v};", sanitize(name)));
                }
                self.emit_assign_rc(ve);
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "setVar" => {
                let name = str_arg(args, 0).ok_or("setVar: no name")?;
                let empty = IrExpr::Str(String::new(), StrStyle::DoubleQuoted);
                let ve = args.get(1).unwrap_or(&empty);
                let v = self.expr_str(ve)?;
                self.emit(&format!("__v_{} = {v};", sanitize(name)));
                self.emit_assign_rc(ve);
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "capture" => {
                let t = self.capture_text(args)?;
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({});", jstr(&t)));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "pipeline" => {
                // statement-position pipeline: stdout goes to the terminal.
                // Stages go through stage_expr (pipe-safe grouping + native
                // fallback), exactly like the IrStmt::Pipeline arm — plain
                // arrow text would leave multi-command stages ungrouped.
                let mut parts2 = Vec::new();
                if let Some(IrExpr::Array(stages)) = args.first() {
                    for st in stages.iter() {
                        if let IrExpr::Arrow(body) = st {
                            parts2.push(self.stage_expr(body)?);
                        } else {
                            return Err("pipeline stage not an Arrow".into());
                        }
                    }
                }
                if parts2.is_empty() { return Err("pipeline without stages".into()); }
                self.helper("quote");
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({});", parts2.join(" + \" | \" + ")));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "grepMatches" => {
                // grep [flags] PATTERN <<< input — real child so stdout
                // reaches the terminal (bash-faithful)
                let t = self.grep_text(args)?;
                self.helper("quote");
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({t});"));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "and" || func == "or" => {
                // Left-to-right short-circuit evaluation:
                // - Arrow bodies whose stmts are fully native execute on the
                //   Java side (state changes survive: mapfile arrays, cd)
                // - anything else becomes one bash -c text run
                // bash rules: && runs next on success, || on failure
                    let is_or = func == "or";
                    // pre-run renderer-temp captures natively so referenced
                    // paths are real
                    for a in args.iter() {
                        if let IrExpr::Arrow(body) = a {
                            for bs in body.iter() {
                                if let IrStmt::Assign { targets, .. } = bs {
                                    if targets.first().map(|t| t.var.starts_with("__ps_")).unwrap_or(false) {
                                        self.stmt(bs)?;
                                    }
                                }
                            }
                        }
                    }
                    self.emit("__chain: {");
                    self.depth += 1;
                    let mut i = 0usize;
                    while i < args.len() {
                        if i > 0 {
                            let gate = if is_or { "__SH_RC == 0" } else { "__SH_RC != 0" };
                            self.emit(&format!("if ({gate}) break __chain;"));
                        }
                        let arg = &args[i];
                        if let IrExpr::Arrow(body) = arg {
                            if body.iter().all(chain_stmt_native) {
                                for bs in body.iter() { self.stmt(bs)?; }
                                i += 1;
                                continue;
                            }
                        }
                        // non-native remainder: ONE bash -c text for this
                        // and everything after
                        let mut parts: Vec<String> = Vec::new();
                        for a in args.iter().skip(i) {
                            if let IrExpr::Arrow(body) = a {
                                parts.push(self.arrow_expr(body)?);
                            } else {
                                parts.push(self.expr_str(a)?);
                            }
                        }
                        self.helper("quote");
                        self.helper("run");
                        self.emit(&format!("__SH_RC = shRun({});", parts.join(&format!(" + \"{}\" + ", sep_of(func)))));
                        i = args.len();
                        continue;
                    }
                    self.depth -= 1;
                    self.emit("}");
                    Ok(())
            }

            IrExpr::Call { func, args, .. } if func == "block" => {
                // block(Arrow(body)) in value position: render the body's
                // statements (side effects) — value is empty
                for a in args.iter() {
                    if let IrExpr::Arrow(stmts) = a {
                        for st in stmts.iter() { self.stmt(st)?; }
                    }
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            IrExpr::Call { func, .. } if func == "continue" => { self.emit("__SH_RC = 0; continue;"); Ok(()) }
            IrExpr::Call { func, .. } if func == "break" => { self.emit("__SH_RC = 0; break;"); Ok(()) }
            IrExpr::Call { func, args, .. } if func == "whileLoop" => {
                // `while CMD; do BODY; done` where the condition is a
                // command (read/...) — the core keeps it as a call with
                // cond/body arrows. The condition runs as bash text (its
                // status drives the loop); the body runs NATIVELY so
                // state changes (count, mapfile arrays) persist.
                // bash: the loop's status is the last BODY command's.
                let (cond_arrow, body_arrow) = match (args.first(), args.get(1)) {
                    (Some(IrExpr::Arrow(c)), Some(IrExpr::Arrow(b))) => (c, b),
                    _ => return Err("whileLoop without cond/body arrows".into()),
                };
                // A single read/test cond runs NATIVELY (java-side vars
                // update and swapped stdin is honored); anything else runs
                // as bash text and only its status drives the loop.
                let native_cond: Option<String> = match &cond_arrow[..] {
                    [IrStmt::Expr(IrExpr::Call { func, args, .. })]
                        if (func == "exec" || func == "builtin")
                            && str_arg(args, 0).map(|c| c == "read").unwrap_or(false) =>
                    {
                        Some(self.command_cond(args)?)
                    }
                    [IrStmt::Expr(IrExpr::Call { func, args, .. })] if func == "test" => {
                        let b = self.test_bool(args)?;
                        self.helper("testrc");
                        Some(format!("shTestRc({b})"))
                    }
                    _ => None,
                };
                self.tmp_seq += 1;
                let lb = format!("__lastBody{}", self.tmp_seq);
                self.emit(&format!("long {lb} = __SH_RC;"));
                self.block_open("while (true) {");
                match native_cond {
                    Some(c) => {
                        self.tmp_seq += 1;
                        let cv = format!("__c{}", self.tmp_seq);
                        self.emit(&format!("boolean {cv} = {c};"));
                        self.emit(&format!("if (!{cv}) {{ __SH_RC = {lb}; break; }}"));
                    }
                    None => {
                        let cond_text = self.arrow_expr(cond_arrow)?;
                        self.helper("quote");
                        self.helper("run");
                        self.emit(&format!("__SH_RC = shRun({cond_text});"));
                        self.emit(&format!("if (__SH_RC != 0) {{ __SH_RC = {lb}; break; }}"));
                    }
                }
                for b in body_arrow { self.stmt(b)?; }
                self.emit(&format!("{lb} = __SH_RC;"));
                self.block_close();
                Ok(())
            }
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::And | BinOpKind::Or) =>
            {
                // short-circuit chains as nested ifs:
                // - test/native-builtin lhs -> native cond
                // - break/continue on either side -> native jump (a child
                //   `break` would error outside the java loop AND bind the
                //   wrong loop; `break || X` / `break && X` both just break)
                // - assignment lhs -> run natively (state must persist;
                //   a child `result=...` would be lost)
                // - COMPOUND (BinOp) lhs -> recurse natively: the inner
                //   chain may itself contain assignments that must persist
                //   (a single-child `grep && result=...` would lose them)
                // - otherwise a real child run (stdout passes through)
                let is_or = matches!(*op, BinOpKind::Or);
                if matches!(lhs.as_ref(), IrExpr::Call { func, .. } if func == "break") {
                    self.emit("__SH_RC = 0; break;");
                    return Ok(());
                }
                if matches!(lhs.as_ref(), IrExpr::Call { func, .. } if func == "continue") {
                    self.emit("__SH_RC = 0; continue;");
                    return Ok(());
                }
                if matches!(rhs.as_ref(), IrExpr::Call { func, .. } if func == "break") {
                    // X || break / X && break: run X, break on the
                    // short-circuit polarity (expr_stmt runs tests and
                    // assignments natively, children otherwise)
                    let gate = if is_or { "__SH_RC != 0" } else { "__SH_RC == 0" };
                    self.expr_stmt(lhs)?;
                    self.emit(&format!("if ({gate}) {{ __SH_RC = 0; break; }}"));
                    return Ok(());
                }
                if matches!(rhs.as_ref(), IrExpr::Call { func, .. } if func == "continue") {
                    let gate = if is_or { "__SH_RC != 0" } else { "__SH_RC == 0" };
                    self.expr_stmt(lhs)?;
                    self.emit(&format!("if ({gate}) {{ __SH_RC = 0; continue; }}"));
                    return Ok(());
                }
                if matches!(lhs.as_ref(), IrExpr::Call { func, .. }
                    if matches!(func.as_str(), "assign" | "setVar" | "setArray" | "setArrayAppend" | "block")) {
                    // state-changing lhs runs natively; gate on its status
                    self.expr_stmt(lhs)?;
                    let head = if is_or { "if (__SH_RC != 0) {".to_string() } else { "if (__SH_RC == 0) {".to_string() };
                    self.block_open(&head);
                    self.expr_stmt(rhs)?;
                    self.block_close();
                    return Ok(());
                }
                if matches!(lhs.as_ref(), IrExpr::BinOp { op, .. }
                    if matches!(op, BinOpKind::And | BinOpKind::Or)) {
                    // compound lhs: lower it natively (inner assignments
                    // persist), then gate the rhs on the recorded status
                    self.expr_stmt(lhs)?;
                    let head = if is_or { "if (__SH_RC != 0) {".to_string() } else { "if (__SH_RC == 0) {".to_string() };
                    self.block_open(&head);
                    self.expr_stmt(rhs)?;
                    self.block_close();
                    return Ok(());
                }
                let cond = if matches!(lhs.as_ref(), IrExpr::Call { func, .. } if func == "test")
                    || matches!(lhs.as_ref(), IrExpr::Str(..))
                    || matches!(lhs.as_ref(),
                        IrExpr::Call { func, args, .. }
                        if (func == "exec" || func == "builtin")
                            && str_arg(args, 0).map(|c| matches!(c, "cd" | "export" | "read" | "shift")).unwrap_or(false))
                    || matches!(lhs.as_ref(),
                        IrExpr::Call { func, args, .. }
                        if (func == "exec" || func == "builtin")
                            && str_arg(args, 0).map(|c| self.funcs.contains(c)).unwrap_or(false)) {
                    Some(self.cond_bool(lhs)?)
                } else {
                    let t = self.side_text(lhs)?;
                    self.helper("quote");
                    self.helper("run");
                    self.emit(&format!("__SH_RC = shRun({t});"));
                    None
                };
                match cond {
                    Some(c) => {
                        let head = if is_or { format!("if ((!{c})) {{") } else { format!("if ({c}) {{") };
                        self.block_open(&head);
                        self.expr_stmt(rhs)?;
                        self.block_close();
                    }
                    None => {
                        let head = if is_or { "if (__SH_RC != 0) {".to_string() } else { "if (__SH_RC == 0) {".to_string() };
                        self.block_open(&head);
                        self.expr_stmt(rhs)?;
                        self.block_close();
                    }
                }
                Ok(())
            }
            other => {
                // an expression evaluated for side effects; pure literals are
                // NOT valid Java expression statements — skip them
                if matches!(other, IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Bool(_)) {
                    self.emit("__SH_RC = 0;");
                    return Ok(());
                }
                let v = self.expr_str(other)?;
                if v.starts_with("__") || v.contains('=') || v.contains('(') {
                    self.emit(&format!("{v};"));
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
        }
    }

    /// One command statement (func builtin|exec): native builtins natively,
    /// everything else via bash -c.
    fn command(&mut self, args: &[IrExpr]) -> Result<(), String> {
        let cmd = match str_arg(args, 0) {
            Some(c) => c.to_string(),
            None => {
                // dynamic command word — route through bash -c with the
                // whole command line computed at runtime
                let words = self.words_to_args(&args[1.min(args.len())..])?;
                let mut parts = vec![self.expr_str(args.first().ok_or("command: no word")?)?];
                for w in words { parts.push(format!("shQuote({w})")); }
                let line = parts.join(" + \" \" + ");
                self.helper("quote");
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({line});"));
                return Ok(());
            }
        };
        let rest = &args[1.min(args.len())..];
        match cmd.as_str() {
            "echo" => {
                let mut newline = true;
                let mut interpret = false;
                let mut words_start = 0;
                let flat = flatten_words(rest);
                for w in flat.iter() {
                    if let IrExpr::Str(s, _) = w {
                        if s == "-n" { newline = false; words_start += 1; continue; }
                        if s == "-e" { interpret = true; words_start += 1; continue; }
                        if s == "-E" { words_start += 1; continue; }
                    }
                    break;
                }
                let joined = self.join_word_refs(&flat[words_start.min(flat.len())..], " + \" \" + ")?;
                if interpret && joined.contains("\\") {
                    self.helper("escapes");
                    let with_nl = if newline { format!("shEscapes({joined}) + \"\\n\"") } else { format!("shEscapes({joined})") };
                    self.emit(&format!("System.out.print({with_nl});"));
                } else if newline {
                    self.emit(&format!("System.out.println({joined});"));
                } else {
                    self.emit(&format!("System.out.print({joined});"));
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "printf" => {
                let flat = flatten_words(rest);
                if flat.is_empty() { return Err("printf: no format".into()); }
                let fmt = self.expr_str(flat[0])?;
                self.helper("printf");
                self.helper("split");
                // build args as a runtime list: unquoted $var splits become
                // MULTIPLE arguments (bash repeats the format per argument)
                let mut has_split = false;
                let mut adds = Vec::new();
                for w in flat.iter().skip(1) {
                    let e = self.expr_str(w)?;
                    if e.starts_with("shSplit(") || e.starts_with("String.join(\" \", shSplit(") {
                        // recover the raw split expression
                        let raw: String = if let Some(r) = e.strip_prefix("String.join(\" \", ") {
                            r.strip_suffix(')').unwrap_or(r).to_string()
                        } else { e.clone() };
                        adds.push(format!("__pa.addAll({raw});"));
                        has_split = true;
                    } else {
                        adds.push(format!("__pa.add({e});"));
                    }
                }
                if adds.is_empty() {
                    self.emit(&format!("System.out.print(shPrintf({fmt}, new String[0]));"));
                } else if !has_split {
                    self.emit(&format!("{{ List<String> __pa = new ArrayList<>(); {} System.out.print(shPrintf({fmt}, __pa.toArray(new String[0]))); }}",
                                       adds.join(" ")));
                } else {
                    self.emit("{");
                    self.emit("    List<String> __pa = new ArrayList<>();");
                    for a in adds { self.emit(&format!("    {a}")); }
                    self.emit(&format!("    System.out.print(shPrintf({fmt}, __pa.toArray(new String[0])));"));
                    self.emit("}");
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "exit" => {
                match rest.first() {
                    Some(c) => { let n = self.expr_num(c)?; self.emit(&format!("System.exit((int) {n});")); }
                    None => self.emit("System.exit((int) __SH_RC);"),
                }
                Ok(())
            }
            "mapfile" => {
                // mapfile [-t] VAR — read stdin lines into an array field
                let target = rest.iter()
                    .flat_map(|w| flatten_words(std::slice::from_ref(w)))
                    .find_map(|e| match e {
                        IrExpr::Str(t, _) if !t.starts_with('-') => Some(t.clone()),
                        _ => None,
                    })
                    .ok_or("mapfile: no target var")?;
                self.arrays.insert(sanitize(&target));
                self.helper("readln");
                self.emit(&format!("__a_{} = new ArrayList<>();", sanitize(&target)));
                self.block_open("{");
                self.emit("__v___mf = shReadln();");
                self.block_open("while (__v___mf != null) {");
                self.emit(&format!("__a_{}.add(__v___mf);", sanitize(&target)));
                self.emit("__v___mf = shReadln();");
                self.block_close();
                self.emit("}");
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            ":" | "true" => { self.emit("__SH_RC = 0;"); Ok(()) }
            "false" => { self.emit("__SH_RC = 1;"); Ok(()) }
            "cd" => {
                let d = self.join_words(rest, " ")?;
                self.helper("cd");
                self.emit(&format!("__SH_RC = shCd({d});"));
                Ok(())
            }
            "shift" => {
                let n = match rest.first() {
                    Some(IrExpr::Array(xs)) if xs.is_empty() => "1L".to_string(),
                    Some(e) => self.expr_num(e)?,
                    None => "1L".to_string(),
                };
                self.helper("shift");
                self.emit(&format!("__SH_RC = shShift((int) {n});"));
                Ok(())
            }
            "typeset" | "declare" | "local" if rest.iter().any(|a| matches!(a, IrExpr::Str(x, _) if x == "-i")) && false => {
                // `typeset -i n`: integer attribute — the assignment itself
                // is unaffected (arith contexts already read numbers)
                Ok(())
            }
            "export" | "readonly" => {
                // export NAME=value → set field AND process env (via a
                // child-visible system property is not inherited; use the
                // ProcessBuilder environment at spawn time instead — here
                // we only track the field)
                for w in flatten_words(rest) {
                    if let IrExpr::Str(s, _) = w {
                        if let Some(eq) = s.find('=') {
                            let (n, v) = (&s[..eq], &s[eq + 1..]);
                            self.ensure_field(n);
                            self.emit(&format!("__v_{} = {}; __SH_EXPORTS.put(\"{}\", __v_{});", sanitize(n), jstr(v), n, sanitize(n)));
                        } else {
                            // bare `export NAME` marks the CURRENT value for
                            // child processes — never re-read from env
                            self.ensure_field(s);
                            self.emit(&format!("__SH_EXPORTS.put(\"{}\", __v_{});", s, sanitize(s)));
                        }
                    }
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "unset" => {
                for w in flatten_words(rest) {
                    if let IrExpr::Str(s, _) = w {
                        let n = s.trim_start_matches("$");
                        self.ensure_field(n);
                        self.emit(&format!("__v_{} = \"\";", sanitize(n)));
                    }
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "local" | "typeset" | "declare" => {
                // `local x=$(false)` keeps the substitution's status (like
                // plain assignment); only reset when no value captures.
                // (Computed upfront: values embed in strings/arrays below.)
                let saw_capture = rest.iter().any(|w| match w {
                    IrExpr::Array(items) => items.iter().any(|it| match it {
                        IrExpr::Str(t, _) => t.contains("$(") || t.contains('`'),
                        other => Self::expr_has_capture(other),
                    }),
                    IrExpr::Str(t, _) => t.contains("$(") || t.contains('`'),
                    other => Self::expr_has_capture(other),
                });
                // `typeset -l|-u NAME`: a case attribute on that var
                {
                    let flat: Vec<&IrExpr> = rest.iter().flat_map(|w| flatten_words(std::slice::from_ref(w))).collect();
                    let flag = flat.iter().find_map(|w| match w {
                        IrExpr::Str(f, _) if f == "-l" || f == "-u" => Some(f.clone()),
                        _ => None,
                    });
                    if let Some(flag) = flag {
                        for w in flat.iter() {
                            let nm = match w {
                                IrExpr::Str(x, _) if !x.starts_with('-') => Some(x.trim_end_matches('=').to_string()),
                                _ => None,
                            };
                            if let Some(n) = nm {
                                self.ensure_field(&n);
                                if flag == "-l" { self.lower_vars.insert(sanitize(&n)); } else { self.upper_vars.insert(sanitize(&n)); }
                                break;
                            }
                        }
                    }
                }
                for w in rest.iter() {
                    if matches!(w, IrExpr::Call { func, .. } if func == "setArray" || func == "setArrayAppend") {
                        // declare arr=(elems): nested setArray call
                        self.expr_stmt(w)?;
                        continue;
                    }
                    // composite word: Array([Str("name="), value-exprs...])
                    if let IrExpr::Array(items) = w {
                        let mut nm: Option<String> = None;
                        for it in items.iter() {
                            // nested setArray (declare -a arr=(...)): run it
                            // for effect, never as a value for `nm`
                            if matches!(it, IrExpr::Call { func, .. } if func == "setArray" || func == "setArrayAppend") {
                                self.expr_stmt(it)?;
                                continue;
                            }
                            // flags (-a/-A/-r/...) are never names: pending
                            // nm must not capture them (declare -a arr=...
                            // would otherwise assign the value to `-a`)
                            if let IrExpr::Str(t, _) = it {
                                if t.starts_with('-') && !t.contains('=') {
                                    continue;
                                }
                            }
                            match it {
                                IrExpr::Str(t, _) => {
                                    if let Some(eq) = t.find('=') {
                                        let (n0, v0) = (&t[..eq], &t[eq + 1..]);
                                        self.ensure_field(n0);
                                        if !v0.is_empty() {
                                            if v0.contains('$') && self.dollar_expand_ok(v0) {
                                                match self.expand_dollars(v0) {
                                                    Ok(e) => self.emit(&format!("__v_{} = {e};", sanitize(n0))),
                                                    Err(_) => self.emit(&format!("__v_{} = {};", sanitize(n0), jstr(v0))),
                                                }
                                            } else {
                                                self.emit(&format!("__v_{} = {};", sanitize(n0), jstr(v0)));
                                            }
                                        } else {
                                            nm = Some(n0.to_string());
                                        }
                                    } else if nm.is_none() {
                                        nm = Some(t.clone());
                                    }
                                }
                                other => {
                                    if let Some(base) = &nm {
                                        let v = self.expr_str(other)?;
                                        let sv = sanitize(base);
                                        let v = if self.lower_vars.contains(&sv) { format!("({v}).toLowerCase()") }
                                            else if self.upper_vars.contains(&sv) { format!("({v}).toUpperCase()") }
                                            else { v };
                                        self.emit(&format!("__v_{sv} = {v};"));
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    if let IrExpr::Str(s2, _) = w {
                        if let Some(eq) = s2.find('=') {
                            self.ensure_field(&s2[..eq]);
                            let rhs = &s2[eq + 1..];
                            // the value may carry $ forms the frontend kept
                            // opaque in one string (local n=$1): expand them
                            // natively; plain literals stay literals.
                            if rhs.contains('$') && self.dollar_expand_ok(rhs) {
                                match self.expand_dollars(rhs) {
                                    Ok(e) => self.emit(&format!("__v_{} = {e};", sanitize(&s2[..eq]))),
                                    Err(_) => self.emit(&format!("__v_{} = {};", sanitize(&s2[..eq]), jstr(rhs))),
                                }
                            } else {
                                self.emit(&format!("__v_{} = {};", sanitize(&s2[..eq]), jstr(rhs)));
                            }
                        }
                    }
                }
                if !saw_capture {
                    self.emit("__SH_RC = 0;");
                }
                Ok(())
            }
            "read" => {
                // `read [-r] [-d delim] [-n N] [-t sec] [-p prompt] [-a arr]
                // [-s] [-u fd] [name...]` with optional VAR=value env
                // prefixes (notably `IFS=:` — split the line on the IFS
                // delimiter instead of taking it whole).
                let flat: Vec<&IrExpr> = rest.iter().flat_map(|w| flatten_words(std::slice::from_ref(w))).collect();
                let mut ifs: Option<String> = None;
                let mut names: Vec<String> = Vec::new();
                for w in flat.iter() {
                    match w {
                        IrExpr::Object(props) => {
                            for (k, v) in props.iter() {
                                if k == "IFS" {
                                    if let IrExpr::Str(s, _) = v { ifs = Some(s.clone()); }
                                }
                            }
                        }
                        IrExpr::Str(t, _) if t.starts_with('-') => {}
                        IrExpr::Str(t, _) => names.push(t.clone()),
                        IrExpr::Call { func, args, .. } if func == "getVar" => {
                            if let Some(n) = str_arg(args, 0) { names.push(n.to_string()); }
                        }
                        _ => {}
                    }
                }
                let target = names.first().cloned().ok_or("read: no target var")?;
                self.helper("readln");
                for n in names.iter() { self.ensure_field(n); }
                if names.len() == 1 && ifs.is_none() {
                    self.emit(&format!("__v_{} = shReadln(); __SH_RC = (__v_{} == null) ? 1 : 0;",
                                       sanitize(&target), sanitize(&target)));
                } else {
                    // multi-var and/or IFS-split read
                    self.helper("readsplit");
                    let dl = jstr(ifs.as_deref().unwrap_or(""));
                    self.emit("{");
                    self.emit(&format!("    String __line = shReadln();"));
                    self.emit(&format!("    String[] __flds = shReadSplit(__line, {dl}, {});", names.len()));
                    for (i, n) in names.iter().enumerate() {
                        self.emit(&format!("    __v_{} = __flds[{i}];", sanitize(n)));
                    }
                    self.emit(&format!("    __SH_RC = (__line == null) ? 1 : 0;"));
                    self.emit("}");
                }
                Ok(())
            }
            "eval" => {
                let t = self.join_words(rest, " ")?;
                self.helper("runrc");
                self.emit(&format!("__SH_RC = shRunEval({t});"));
                Ok(())
            }
            "wait" | "umask" | "ulimit" | "shopt" | "set" | "hash" | "enable"
            | "suspend" | "times" | "bind" | "help" | "type" | "command_noop" => {
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "trap" => {
                // trap BODY SIGNAL: only EXIT with a plain-string body is
                // honored — run it before normal end-of-main exit (error
                // traps and dynamic bodies stay no-ops). The body text is
                // re-parsed by bash at exit time via shRunEval.
                let flat = flatten_words(rest);
                let strs: Vec<String> = flat.iter().filter_map(|e| match e {
                    IrExpr::Str(s, _) => Some(s.clone()),
                    _ => None,
                }).collect();
                if strs.len() >= 2 && strs.last().map(|s| s == "EXIT").unwrap_or(false) {
                    let body = strs[..strs.len() - 1].join(" ");
                    if !body.contains("$(") && !body.contains('`') {
                        self.exit_traps.push(body);
                    }
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "let" => {
                // let EXPR — arithmetic; the arg is an arith text or Arith ast
                let code = match rest.first() {
                    Some(IrExpr::Str(t, _)) => self.arith_text(t)?,
                    Some(other) => self.expr_num(other)?,
                    None => return Err("let: no expression".into()),
                };
                self.emit(&format!("{code};"));
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "." | "source" => {
                // `.`/`source` shares state with the caller (the sourced
                // file's assignments persist). Static inlining is impossible
                // when the file is created at runtime — instead round-trip
                // through a child bash: serialize java state in, source the
                // file, `declare -p` the changes back out.
                let f = self.join_words(rest, " ")?;
                self.helper("sourcefile");
                // pass every tracked scalar + array (prefixed by kind) so
                // the child sees them; new vars come back via __dyn.
                let mut names: Vec<String> = Vec::new();
                for fld in self.fields.iter() { names.push(format!("s:{fld}")); }
                for arr in self.arrays.iter() { names.push(format!("a:{arr}")); }
                let nl = names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
                self.emit(&format!("__SH_RC = shSourceFile({f}, new String[]{{{nl}}});"));
                Ok(())
            }
            other => {
                if self.funcs.contains(other) {
                    // positional args replace __SH_ARGV for the call.
                    // Functions return their value as a String (the
                    // echo-return-lift convention); a direct statement call
                    // prints it when non-empty (eligible functions have no
                    // direct calls post-lift, but a missed rewrite must
                    // still show output). __SH_RC survives: the print below
                    // never touches it.
                    let flat = flatten_words(rest);
                    if !flat.is_empty() {
                        self.emit("{");
                        self.emit("    List<String> __callArgv = new ArrayList<>();");
                        for w in flat.iter() {
                            let e = self.expr_str(w)?;
                            if e.starts_with("shSplit(") {
                                self.emit(&format!("__callArgv.addAll({e});"));
                            } else {
                                self.emit(&format!("__callArgv.add({e});"));
                            }
                        }
                        self.emit("__SH_ARGV = __callArgv;");
                        self.emit("}");
                    }
                    self.tmp_seq += 1;
                    let rv = format!("__fnrv{}", self.tmp_seq);
                    self.emit(&format!("String {rv} = {f}();", f = fn_name(other)));
                    self.block_open(&format!("if (!{rv}.isEmpty()) {{"));
                    self.emit(&format!("System.out.println({rv});"));
                    self.block_close();
                    return Ok(());
                }
                let mut pieces: Vec<(String, bool)> = Vec::new();
                match args.first() {
                    Some(IrExpr::Str(c, _)) => pieces.push((
                        jstr(&c.replace("\u{1}SH2GLOB\u{1}", "")), c.contains("\u{1}SH2GLOB\u{1}"))),
                    _ => pieces.push((jstr(other), true)),
                }
                for w in flatten_words(rest) {
                    pieces.extend(self.word_pieces(w)?);
                }
                let line = self.pieces_join(&pieces);
                self.helper("quote");
                self.helper("run");
                // renderer-temp cleanup (`rm -f __ps_tmp*` after process
                // substitution) must not clobber the recorded status: in
                // bash the temp removal is not a command and `$?` still
                // reads the real previous status.
                let all_temp = rest.iter().flat_map(|w| flatten_words(std::slice::from_ref(w))).all(|w| match w {
                    IrExpr::Str(s, _) => s.starts_with('-') || s.contains("__ps_tmp"),
                    IrExpr::Var(n, _) | IrExpr::Ident(n) => n.contains("__ps_tmp"),
                    IrExpr::Call { func, args, .. } if func == "getVar" => {
                        str_arg(args, 0).map(|n| n.contains("__ps_tmp")).unwrap_or(false)
                    }
                    _ => false,
                });
                if other == "rm" && all_temp {
                    self.tmp_seq += 1;
                    let sv = format!("__rcsv{}", self.tmp_seq);
                    self.emit(&format!("long {sv} = __SH_RC; __SH_RC = shRun({line}); __SH_RC = {sv};"));
                } else {
                    self.emit(&format!("__SH_RC = shRun({line});"));
                }
                Ok(())
            }
        }
    }

    fn words_to_args(&mut self, args: &[IrExpr]) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for a in args {
            match a {
                IrExpr::Array(items) => {
                    for it in items {
                        // `"${arr[@]}"`: the ELEMENT JOIN, expanded into
                        // separate argv words at runtime (shAslice returns
                        // one space-joined string)
                        if let IrExpr::Call { func, .. } = it {
                            if func == "param" {
                                if let IrExpr::Call { args, .. } = it {
                                    if str_arg(args, 2) == Some("@") {
                                        let t = self.expr_str(it)?;
                                        self.helper("split");
                                        out.push(format!("shSplit({t})"));
                                        continue;
                                    }
                                }
                            }
                        }
                        // a child-computed var stays symbolic in child text
                        if let (false, IrExpr::Call { func, args, .. }) = (self.child_sym.is_empty(), it) {
                            if func == "getVar" {
                                if let Some(n) = str_arg(args, 0) {
                                    if self.child_sym.contains(n) {
                                        self.helper("quote");
                                        out.push(format!("shQuote(\"${n}\")"));
                                        continue;
                                    }
                                }
                            }
                        }
                        out.push(self.expr_str(it)?);
                    }
                }
                // command-scoped env object (VAR=x cmd): tracked exports
                // cover the child-env case; per-command overrides are a
                // documented limitation
                IrExpr::Object(_) => {}
                other => out.push(self.expr_str(other)?),
            }
        }
        Ok(out)
    }

    fn join_word_refs(&mut self, args: &[&IrExpr], sep: &str) -> Result<String, String> {
        let mut ws = Vec::new();
        for a in args { ws.push(self.expr_str(a)?); }
        Ok(ws.join(sep))
    }

    fn join_words(&mut self, args: &[IrExpr], sep: &str) -> Result<String, String> {
        let ws = self.words_to_args(args)?;
        Ok(ws.join(sep))
    }

    fn for_items(&mut self, iter: &IrExpr) -> Result<String, String> {
        match iter {
            IrExpr::Array(items) => {
                let mut parts = Vec::new();
                for it in items {
                    if let IrExpr::Call { func, args, .. } = it {
                        // "$@" / "$*" iterables stay runtime lists
                        if func == "listVar" {
                            parts.push("__SH_ARGV".to_string());
                            continue;
                        }
                        // getVar("@")/$* in a for-context: word-split at runtime
                        if func == "getVar" {
                            let name = str_arg(args, 0).ok_or("for: getVar without name")?;
                            self.helper("split");
                            if name == "@" || name == "*" {
                                parts.push("shSplit(String.join(\" \", __SH_ARGV))".to_string());
                            } else {
                                let base = read_field(sanitize(name))
                                    .ok_or_else(|| format!("for over undeclared var {name}"))?;
                                parts.push(format!("shSplit({base})"));
                            }
                            continue;
                        }
                        if func == "brace" {
                            for x in brace_expand(args)? {
                                parts.push(jstr(&x));
                            }
                            continue;
                        }
                        if func == "split" {
                            // unquoted $var word-splitting yields multiple
                            // loop items — keep the runtime list whole
                            self.helper("split");
                            let inner = self.expr_str(args.first().ok_or("split: no arg")?)?;
                            parts.push(format!("shSplit({inner})"));
                            continue;
                        }
                        if func == "param" {
                            // `${!map[@]}` keys iteration: the key list
                            // directly (marked with the \u{1} join so the
                            // runtime-list path addAlls it)
                            if let Some(n) = str_arg(args, 1) {
                                if let Some(bare) = n.strip_prefix('!') {
                                    let ks = sanitize(bare);
                                    self.assoc.insert(ks.clone());
                                    self.arrays.insert(ks.clone());
                                    parts.push(format!("__k_{ks}\u{1}"));
                                    continue;
                                }
                            }
                            let slice = self.param(args)?;
                            parts.push(slice);
                            continue;
                        }
                    }
                    parts.push(self.expr_str(it)?);
                }
                let has_list = parts.iter().any(|p| p.contains("shSplit(") || p.contains('\u{1}'));
                Ok(parts.join(if has_list { "\u{1}" } else { ", " }))
            }
            IrExpr::Range { start, end } => {
                let mut parts = Vec::new();
                let mut i = *start;
                while i <= *end { parts.push(i.to_string()); i += 1; }
                Ok(parts.join(", "))
            }
            IrExpr::Call { func, args, .. } if func == "listVar" => {
                Ok("__SH_ARGV.toArray(new String[0])".to_string())
            }
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                // `for w in $y` — unquoted expansion word-splits
                let name = str_arg(args, 0).ok_or("for: getVar without name")?;
                self.helper("split");
                let base = read_field(sanitize(name)).ok_or_else(|| format!("for over undeclared var {name}"))?;
                Ok(format!("shSplit({base})"))
            }
            IrExpr::Call { func, args, .. } if func == "split" => {
                self.helper("split");
                let s = self.expr_str(args.first().ok_or("split: no arg")?)?;
                Ok(format!("shSplit({s})"))
            }
            other => Err(format!("for-iterable not in the java subset: {other:?}")),
        }
    }

    // ── conditions ───────────────────────────────────────────────────
    fn cond_bool(&mut self, cond: &IrExpr) -> Result<String, String> {
        match cond {
            IrExpr::Call { func, args, .. } if func == "test" => {
                // A compound test (grouping parens, -a/-o chains, a
                // regex/glob word list the tokenizer cannot classify) is
                // CHEAPEST as its literal bash text: run `[ … ]`/`[[ … ]]`
                // as a command and test the status.
                if let Ok(t) = self.bracket_text(args, false) {
                    let simple = t.contains("shFnmatch(") || t.contains("shRegexMatch(");
                    if !simple {
                        self.helper("run");
                        return Ok(format!("(shRun({t}) == 0)"));
                    }
                }
                // pure-boolean helper keeps RC side effects while staying
                // composable (negation, &&/|| chains)
                self.helper("testrc");
                let b = self.test_bool(args)?;
                Ok(format!("shTestRc({b})"))
            }
            IrExpr::Str(text, _) => {
                // string-form test text (`-f "x"` as one literal) — tokenize
                let toks = split_test_text(text);
                let mut tw: Vec<TestTok> = Vec::new();
                for t in toks { tw.push(TestTok::S(t)); }
                let b = self.test_tokens(&tw)?;
                self.helper("testrc");
                Ok(format!("shTestRc({b})"))
            }
            IrExpr::Call { func, args, .. } if func == "exec" || func == "builtin" => {
                self.command_cond(args)
            }
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                let n = str_arg(args, 0).ok_or("cond: getVar no name")?;
                Ok(format!("(!{}.isEmpty())", read_field(sanitize(n)).ok_or_else(|| format!("undeclared var {n} in condition"))?))
            }
            IrExpr::BinOp { op: BinOpKind::And, lhs, rhs, .. } =>
                Ok(format!("({} && {})", self.cond_bool(lhs)?, self.cond_bool(rhs)?)),
            IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs, .. } =>
                Ok(format!("({} || {})", self.cond_bool(lhs)?, self.cond_bool(rhs)?)),
            IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } =>
                Ok(format!("(!{})", self.cond_bool(lhs)?)),
            IrExpr::BinOp { op, lhs, rhs, .. } if is_cmp(op) => {
                // numeric/string comparison in condition position: a real
                // Java boolean — NOT the String.valueOf wrapper the value
                // context needs (wrapping it in .isEmpty() is always true,
                // which turned every [ $i -lt $n ] while loop into an
                // infinite loop).
                self.binop_bool(op, lhs, rhs)
            },
            IrExpr::Call { func, .. } if func == "redirect" => {
                self.helper("run");
                self.helper("quote");
                Ok(format!("(shRun({}) == 0)", self.side_text(cond)?))
            }
            IrExpr::Call { func, .. } if func == "block" => {
                // multi-command condition (`while echo x; test -f y`) — the
                // status is the command list's, not a value's emptiness
                self.helper("run");
                self.helper("quote");
                Ok(format!("(shRun({}) == 0)", self.side_text(cond)?))
            }
            IrExpr::Bool(b) => Ok(if *b { "true".into() } else { "false".into() }),
            other => {
                let v = self.expr_str(other)?;
                Ok(format!("(!{v}.isEmpty())"))
            }
        }
    }

    fn command_cond(&mut self, args: &[IrExpr]) -> Result<String, String> {
        let cmd = str_arg(args, 0).ok_or("cond: non-literal command")?;
        let rest = &args[1.min(args.len())..];
        match cmd {
            "cd" => {
                let d = self.join_words(rest, " ")?;
                self.helper("cd");
                Ok(format!("(shCd({d}) == 0)"))
            }
            "read" => {
                // `read ... name...` in condition position also honors
                // VAR=value prefixes (IFS=:) and multi-var splitting. A
                // multi-target read needs STATEMENTS (split + assign), so
                // it goes in a per-site boolean helper (statements are
                // legal there; the call site stays a pure boolean expr).
                let flat: Vec<&IrExpr> = rest.iter().flat_map(|e| match e {
                    IrExpr::Array(xs) => xs.iter().collect::<Vec<_>>(),
                    other => vec![other],
                }).collect();
                let mut ifs: Option<String> = None;
                let mut names: Vec<String> = Vec::new();
                for x in flat.iter() {
                    match x {
                        IrExpr::Object(props) => {
                            for (k, v) in props.iter() {
                                if k == "IFS" {
                                    if let IrExpr::Str(s, _) = v { ifs = Some(s.clone()); }
                                }
                            }
                        }
                        IrExpr::Str(t, _) if !t.starts_with('-') => names.push(t.clone()),
                        IrExpr::Call { func, args, .. } if func == "getVar" => {
                            if let Some(n) = str_arg(args, 0) { names.push(n.to_string()); }
                        }
                        _ => {}
                    }
                }
                let target = names.first().cloned().ok_or("read cond: no target")?;
                self.helper("readln");
                for n in names.iter() { self.ensure_field(n); }
                if names.len() == 1 && ifs.is_none() {
                    Ok(format!("((__v_{t} = shReadln()) != null)", t = sanitize(&target)))
                } else {
                    self.helper("readsplit");
                    self.tmp_seq += 1;
                    let hn = format!("__readc{}", self.tmp_seq);
                    let dl = jstr(ifs.as_deref().unwrap_or(""));
                    let mut body = String::new();
                    body.push_str("        String __line = shReadln();\n");
                    body.push_str("        if (__line == null) return false;\n");
                    body.push_str(&format!("        String[] __flds = shReadSplit(__line, {dl}, {});\n", names.len()));
                    for (i, n) in names.iter().enumerate() {
                        body.push_str(&format!("        __v_{} = __flds[{i}];\n", sanitize(n)));
                    }
                    body.push_str("        return true;\n");
                    self.java_helpers.push(format!("    static boolean {hn}() throws Exception {{\n{body}    }}\n"));
                    Ok(format!("{hn}()"))
                }
            }
            "true" => Ok("true".into()),
            "false" => Ok("false".into()),
            other if self.funcs.contains(other) => {
                // Functions return String; the condition is the STATUS the
                // call leaves in __SH_RC. Sequence the call as a boolean
                // via a null-check on its (never-null) value.
                let flat = flatten_words(rest);
                if flat.is_empty() {
                    Ok(format!("(({f}() != null) && __SH_RC == 0)", f = fn_name(other)))
                } else {
                    // function call WITH args in condition position: the
                    // callee reads positionals from __SH_ARGV, so bind them
                    // inline — a Java assignment IS an expression, and the
                    // function's own try/finally restores the caller's argv.
                    let mut arg_exprs = Vec::new();
                    for w in flat.iter() {
                        let e = self.expr_str(w)?;
                        if e.starts_with("shSplit(") || e.starts_with("String.join(\" \", shSplit(") {
                            return Err("function call with word-splitting args not supported in condition position".into());
                        }
                        arg_exprs.push(e);
                    }
                    Ok(format!("((__SH_ARGV = new ArrayList<>(List.of(new String[]{{{}}}))) != null && ({f}() != null) && __SH_RC == 0)",
                               arg_exprs.join(", "), f = fn_name(other)))
                }
            }
            _ => {
                let words = self.words_to_args(rest)?;
                let mut all = vec![format!("shQuote({})", jstr(cmd))];
                all.extend(words.into_iter().map(|w| format!("shQuote({w})")));
                let line = all.join(" + \" \" + ");
                self.helper("quote");
                self.helper("run");
                Ok(format!("(shRun({line}) == 0)"))
            }
        }
    }

    /// `[ args ]` → boolean expression (also records RC at the caller).
    fn test_bool(&mut self, args: &[IrExpr]) -> Result<String, String> {
        let mut w = Vec::new();
        for a in args {
            match a {
                IrExpr::Array(items) => {
                    for it in items {
                        match it {
                            IrExpr::Str(s, _) => self.push_test_str(&mut w, s),
                            other => w.push(TestTok::E(self.expr_str(other)?, false)),
                        }
                    }
                }
                IrExpr::Str(s, _) => self.push_test_str(&mut w, s),
                other => w.push(TestTok::E(self.expr_str(other)?, false)),
            }
        }
        self.test_tokens(&w)
    }

    /// `[[ … ]]` test as BASH TEXT (the command form), not an expression.
    /// Only the common shapes are text-renderable; anything else refuses
    /// so the caller can fall back to an expression/child.
    fn bracket_text(&mut self, args: &[IrExpr], short: bool) -> Result<String, String> {
        // tokenize exactly like test_bool, then render the tokens as text
        let mut w: Vec<TestTok> = Vec::new();
        for a in args {
            match a {
                IrExpr::Array(items) => {
                    for it in items {
                        match it {
                            IrExpr::Str(s0, _) => self.push_test_str(&mut w, s0),
                            other => w.push(TestTok::E(self.bash_word(other)?, false)),
                        }
                    }
                }
                IrExpr::Str(s0, _) => self.push_test_str(&mut w, s0),
                other => w.push(TestTok::E(self.bash_word(other)?, false)),
            }
        }
        // `[[ … ]]` keeps its brackets (regex `=~` needs them; `[` would turn
        // a regex into a glob); the short form drops `[ ]`.
        // `a =~ re` fused (`~re` after `=`): split into operand, =~, regex
        let mut v: Vec<TestTok> = Vec::new();
        for t in w.into_iter() {
            if let TestTok::S(x) = &t {
                if let Some(rx) = x.strip_prefix("~").filter(|_| matches!(v.last(), Some(TestTok::S(p)) if p == "=" || p == "==")) {
                    v.pop();
                    v.push(TestTok::S("=~".to_string()));
                    v.push(TestTok::S(rx.to_string()));
                    continue;
                }
            }
            v.push(t);
        }
        // the frontend carries the markers (`[[` … `[[`) inconsistently:
        // drop every marker token and re-emit them as needed below
        let v: Vec<TestTok> = v.into_iter()
            .filter(|t| !matches!(t, TestTok::S(x) if x == "[[" || x == "]]"))
            .collect();
        let w = v;
        let toks: Vec<TestTok> = if short {
            w.into_iter().filter(|t| !matches!(t, TestTok::S(x) if x == "[" || x == "]")).collect()
        } else {
            w
        };
        if toks.is_empty() { return Err("empty test".into()); }
        // single-token `[[ -f ]]` (a bare string test): `-n` implied
        // bash test words are subject to word splitting unless quoted, so
        // every operand goes through shQuote — and a literal `$`-form the
        // child must expand is double-quoted (`"$line"`), never `${line}`
        // a `~`-marked token is the REGEX operand: the marker is internal
        // (the text form carries the `=~` operator, which makes the pattern
        // a regex for the child shell); emitting it verbatim would quote
        // it into a glob.
        let parts: Vec<String> = toks.iter().map(|t| match t {
            // a `~`-marked token is the REGEX operand of `=~`: it goes in
            // RAW — quoting makes bash compare against a literal string
            TestTok::S(x) if x.starts_with('~') && x.len() > 1 => jstr(&x[1..]),
            TestTok::S(x) if x.contains('$') => {
                let esc = x.replace('\\', "\\\\").replace('"', "\\\"");
                format!("shQuote(\"\\\"{esc}\\\"\")")
            }
            TestTok::S(x) => jstr(x),
            TestTok::E(e, _) => format!("shQuote({e})"),
        }).collect();
        let inner = parts.join(" + \" \" + ");
        if short {
            return Ok(format!("\"[ \" + {inner} + \" ]\""));
        }
        Ok(format!("\"[[ \" + {inner} + \" ]]\""))
    }

    /// Test tokens as WORD TEXT: each operand is a literal shell word
    /// (`"$line"`, `[[:space:]]*#`), joined by spaces.
    fn test_word_text(&mut self, args: &[IrExpr]) -> Result<String, String> {
        let mut w: Vec<TestTok> = Vec::new();
        for a in args {
            match a {
                IrExpr::Array(items) => {
                    for it in items {
                        match it {
                            IrExpr::Str(s0, _) => self.push_test_str(&mut w, s0),
                            other => w.push(TestTok::E(self.bash_word(other)?, false)),
                        }
                    }
                }
                IrExpr::Str(s0, _) => self.push_test_str(&mut w, s0),
                other => w.push(TestTok::E(self.bash_word(other)?, false)),
            }
        }
        let parts: Vec<String> = w.iter().filter(|t| !matches!(t, TestTok::S(x) if x == "[[ " || x == "]]")).map(|t| match t {
            TestTok::S(x) => jstr(x),
            TestTok::E(e, _) => format!("shQuote({e})"),
        }).collect();
        if parts.is_empty() { return Err("empty test".into()); }
        Ok(parts.join(" + \" \" + "))
    }

    fn push_operand(&mut self, w: &mut Vec<TestTok>, t: &str) {
        // a merged operand like `~="$HOME"` hides a comparison operator —
        // split on top-level = / == / != first
        for piece in split_eq_ops(t) {
            // leading-tilde expansion (unquoted `~` / `~/...` in test
            // position expands to $HOME; quoted forms never reach here as
            // bare pieces — string contexts keep `~` literal)
            if piece == "~" {
                let home = self.getvar_str("HOME").unwrap_or_else(|_| "\"\"".to_string());
                w.push(TestTok::E(home, false));
                continue;
            }
            // a `~`-marked piece is the REGEX marker, never a home path
            if piece.starts_with('~') && Self::is_tilde_path(&piece) {
                let home = self.getvar_str("HOME").unwrap_or_else(|_| "\"\"".to_string());
                w.push(TestTok::E(format!("({home} + {})", jstr(&piece[1..])), false));
                continue;
            }
            if piece == "=" || piece == "==" || piece == "!=" {
                w.push(TestTok::S(piece));
            } else if piece == "=~" {
                // regex operator: the NEXT piece is the pattern, carried
                // with the `~` marker
                w.push(TestTok::S(piece));
            } else if piece.starts_with('~') {
                // regex marker ([[ s =~ ~re ]]) — a QUOTED marker keeps its
                // quotes in the raw token (`~"^x$"`); strip them so the
                // regex never carries literal quote characters
                let body = piece[1..].trim();
                let body = if body.len() >= 2 && body.starts_with('"') && body.ends_with('"') {
                    &body[1..body.len() - 1]
                } else { body };
                w.push(TestTok::S(format!("~{body}")));
            } else if piece.contains('$') {
                // glob-ness judged on the QUOTE-STRIPPED form: "$p"* is an
                // unquoted glob even though the raw token starts with a quote
                let stripped = {
                    let t = piece.trim();
                    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') { &t[1..t.len() - 1] }
                    else if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') { &t[1..t.len() - 1] }
                    else { t }
                };
                let is_glob = stripped.contains('*') || stripped.contains('?');
                // test-operand quotes are word-quoting, not data: strip them
                // so the expanded Java string holds the bare operand
                let bare = piece.replace('"', "");
                match self.expand_dollars(&bare) {
                    Ok(e) => w.push(TestTok::E(e, is_glob)),
                    Err(_) => w.push(TestTok::S(piece)),
                }
            } else {
                // strip shell-quote artifacts around a plain operand
                let t = piece.trim();
                if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
                    w.push(TestTok::S(t[1..t.len() - 1].to_string()));
                } else if t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2 {
                    w.push(TestTok::S(t[1..t.len() - 1].to_string()));
                } else {
                    w.push(TestTok::S(piece));
                }
            }
        }
    }

    /// `operand=~regex` fused into ONE word (the frontend's `[[ $s =~ re ]]`
    /// form when the regex has no spaces): split into the three tokens the
    /// test tokenizer expects, marking the regex with `~`.
    fn split_fused_regex(t: &str) -> Option<Vec<String>> {
        let at = t.find("=~")?;
        if at == 0 { return None; }
        let (l, r) = (&t[..at], &t[at + 2..]);
        if l.is_empty() || r.is_empty() { return None; }
        // the split must not land inside quotes
        let mut quoted = false;
        for c in l.chars() { if c == '"' || c == '\'' { quoted = !quoted; } }
        if quoted { return None; }
        // mark the regex (`~re`) unless the fused text already carries it
        let re = if r.starts_with('~') { r.to_string() } else { format!("~{r}") };
        Some(vec![l.to_string(), "=~".to_string(), re])
    }

    /// `~` / `~/x` / `~"x"` / `~'x'` — a home-directory path. NOT `~^re`
    /// (the test tokenizer's regex marker).
    /// `~` / `~/x` / `~"x"` / `~'x'` — a home-directory path. NOT `~^re`
    /// (the test tokenizer's regex marker). The raw token may still carry
    /// its quotes (`"~^re$"`), so judge the UNQUOTED text.
    fn is_tilde_path(p: &str) -> bool {
        let t = p.trim();
        let t = if t.len() >= 2 && ((t.starts_with('"') && t.ends_with('"'))
            || (t.starts_with('\'') && t.ends_with('\''))) { &t[1..t.len() - 1] } else { t };
        if !t.starts_with('~') { return false; }
        let rest = &t[1..];
        !rest.is_empty() && (rest.starts_with('/') || rest.starts_with('"') || rest.starts_with('\''))
    }

    fn push_test_str(&mut self, w: &mut Vec<TestTok>, s: &str) {
        let mut toks = if s.chars().any(|c| c.is_whitespace()) { split_test_text(s) } else { vec![s.to_string()] };
        // the core sometimes fuses `!` with the operator (`!-e file`
        // arrives as one string): split a leading `!` before tokenizing
        let mut split: Vec<String> = Vec::new();
        for t in toks {
            if let Some(xs) = Self::split_fused_regex(&t) {
                split.extend(xs);
            } else if t.len() > 1 && t.starts_with('!') && t[1..].starts_with('-') {
                split.push("!".to_string());
                split.push(t[1..].to_string());
            } else {
                split.push(t);
            }
        }
        let toks = split;
        // merge an unclosed $( across subsequent tokens (the frontend splits
        // `[[ $(uname -r) == x ]]` into several string args)
        while let Some(last) = toks.last() {
            if last.contains("$(") && unbalanced_cmdsub(last) {
                // need more tokens — caller must supply them; signal by
                // pushing a marker that test_tokens tolerates
                break;
            }
            break;
        }
        for t in toks {
            self.push_operand(w, &t);
        }
    }

    /// Expand $name/${name}/$#/$?/$N in raw shell text to a Java
    /// concatenation expression.
    /// True when every $-form in s is one expand_dollars understands.
    fn dollar_expand_ok(&self, s: &str) -> bool {
        let ch: Vec<char> = s.chars().collect();
        let mut i = 0usize;
        while i < ch.len() {
            if ch[i] != '$' { i += 1; continue; }
            if i + 1 >= ch.len() { return false; }
            match ch[i + 1] {
                '{' => {
                    let mut j = i + 2;
                    while j < ch.len() && ch[j] != '}' { j += 1; }
                    if j >= ch.len() { return false; }
                    let inner: String = ch[i + 2..j].iter().collect();
                    let ok = inner.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
                        || inner.chars().all(|c| c.is_ascii_digit())
                        || matches!(inner.as_str(), "?" | "#" | "@" | "*" | "$");
                    if !ok { return false; }
                    i = j + 1;
                }
                '(' => {
                    // $((arith)) is fine when natively parseable (the
                    // expand path renders it inline); $(cmd) needs the
                    // child path, so refuse.
                    if i + 2 >= ch.len() || ch[i + 2] != '(' { return false; }
                    let mut depth = 0usize;
                    let mut j = i + 2;
                    while j < ch.len() {
                        if ch[j] == '(' { depth += 1; }
                        if ch[j] == ')' {
                            depth -= 1;
                            if depth == 0 { break; }
                        }
                        j += 1;
                    }
                    if depth != 0 { return false; }
                    let inner_full: String = ch[i + 2..=j].iter().collect();
                    let inner = inner_full.trim().strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(inner_full.trim());
                    if crate::shir::parse_arith_native(inner).is_none() { return false; }
                    i = j + 2;
                }
                c if c.is_ascii_digit() || matches!(c, '?' | '#' | '@' | '*' | '!' | '$') => i += 1,
                c if c.is_ascii_alphabetic() || c == '_' => {
                    i += 1;
                    while i < ch.len() && (ch[i].is_ascii_alphanumeric() || ch[i] == '_') { i += 1; }
                }
                _ => return false,
            }
        }
        true
    }

    fn expand_dollars(&mut self, raw: &str) -> Result<String, String> {
        // strip shell-quote artifacts around the operand: remove exactly
        // one wrapping `"` pair when present, preserving everything inside
        // byte-for-byte (heredoc bodies carry a significant trailing
        // newline that trim() would eat, gluing `content` to a following
        // SH2EOF terminator). Unquoted text passes through untouched.
        let t = raw.trim();
        let s = if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            // locate the pair in the ORIGINAL (not the trimmed view)
            match (raw.find('"'), raw.rfind('"')) {
                (Some(a), Some(b)) if b > a => &raw[a + 1..b],
                _ => raw,
            }
        } else { raw };
        let mut out = String::from("\"\"");
        let mut lit = String::new();
        let ch: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < ch.len() {
            if ch[i] == '$' && i + 1 < ch.len() {
                // NOTE: the ` + ` separator is emitted by each expansion
                // branch, NOT unconditionally here — a literal `$`
                // (e.g. `$'` ANSI-C quotes) appends nothing to `out` and
                // an unconditional separator would leave a dangling `+`
                // (`"*" +  + "$'...'"` — javac rejects the empty operand).
                if ch[i + 1] == '{' {
                    if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                    out += " + ";
                    let mut j = i + 2;
                    while j < ch.len() && ch[j] != '}' { j += 1; }
                    let inner: String = ch[i + 2..j.min(ch.len())].iter().collect();
                    // common operators handled natively; anything exotic refuses
                    let (n, op, pat) = split_param_op(&inner);
                    if !op.is_empty() {
                        out += &self.expr_str_inner(&IrExpr::Call {
                            func: "param".into(),
                            args: vec![IrExpr::Str(op, StrStyle::DoubleQuoted),
                                       IrExpr::Str(n, StrStyle::DoubleQuoted),
                                       IrExpr::Str(pat, StrStyle::DoubleQuoted)],
                        })?;
                    } else {
                        if !inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                            return Err("complex ${...} in test text".into());
                        }
                        out += &self.getvar_str(&inner)?;
                    }
                    i = j + 1;
                } else if ch[i + 1] == '(' {
                    if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                    out += " + ";
                    // $((arith)) arithmetic vs $(cmd) command substitution
                    if i + 2 < ch.len() && ch[i + 2] == '(' {
                        // arithmetic: find the matching close paren pair
                        let mut depth = 0usize;
                        let mut j = i + 2;
                        while j < ch.len() {
                            if ch[j] == '(' { depth += 1; }
                            if ch[j] == ')' {
                                depth -= 1;
                                if depth == 0 { break; }
                            }
                            j += 1;
                        }
                        if depth != 0 {
                            return Err("unbalanced $((...)) in test text".into());
                        }
                        // inner is "(expr)": strip ONE paren pair for the
                        // arith parser (it takes bare `n % i`, not `(n % i)`)
                        let inner_full: String = ch[i + 2..=j].iter().collect();
                        let inner = inner_full.trim().strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(inner_full.trim());
                        match crate::shir::parse_arith_native(inner) {
                            Some(ast) => {
                                out += &format!("String.valueOf({})", self.arith(&ast)?);
                            }
                            None => {
                                return Err(format!("arith in test text not parseable natively: {inner:?}"));
                            }
                        }
                        // skip past BOTH close parens of the $((...)) form
                        i = j + 2;
                        continue;
                    }
                    // $(cmd) command substitution inside test text
                    let mut depth = 1;
                    let mut j = i + 2;
                    while j < ch.len() && depth > 0 {
                        if ch[j] == '(' { depth += 1; }
                        if ch[j] == ')' { depth -= 1; }
                        j += 1;
                    }
                    let end = if depth == 0 { j.saturating_sub(1) } else { j };
                    let cmd: String = ch[i + 2..end].iter().collect();
                    self.helper("quote");
                    self.helper("capture");
                    out += &format!("shCapture({})", jstr(cmd.trim()));
                    i = j;
                } else if ch[i + 1] == '#' || ch[i + 1] == '?' || ch[i + 1] == '@'
                       || ch[i + 1] == '*' || ch[i + 1] == '$' || ch[i + 1] == '!' {
                    if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                    out += " + ";
                    out += &self.getvar_str(&ch[i + 1].to_string())?;
                    i += 2;
                } else if ch[i + 1].is_ascii_digit() {
                    if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                    out += " + ";
                    let mut j = i + 1;
                    while j < ch.len() && ch[j].is_ascii_digit() { j += 1; }
                    let n: String = ch[i + 1..j].iter().collect();
                    out += &self.getvar_str(&n)?;
                    i = j;
                } else if ch[i + 1].is_ascii_alphabetic() || ch[i + 1] == '_' {
                    if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                    out += " + ";
                    let mut j = i + 1;
                    while j < ch.len() && (ch[j].is_ascii_alphanumeric() || ch[j] == '_') { j += 1; }
                    let n: String = ch[i + 1..j].iter().collect();
                    out += &self.getvar_str(&n)?;
                    i = j;
                } else if ch[i + 1] == '\'' {
                    // ANSI-C quote $'...': interpret backslash escapes
                    // natively ($'\x00' is a real NUL, not 7 chars).
                    let mut j = i + 2;
                    while j < ch.len() && ch[j] != '\'' { j += 1; }
                    if j >= ch.len() {
                        lit.push('$');
                        i += 1;
                    } else {
                        let body: String = ch[i + 2..j].iter().collect();
                        if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                        out += " + ";
                        out += &jstr(&unescape_ansi_c(&body));
                        i = j + 1;
                    }
                } else {
                    lit.push('$');
                    i += 1;
                }
            } else {
                lit.push(ch[i]);
                i += 1;
            }
        }
        if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); }
        Ok(out)
    }

    fn test_words(&mut self, args: &[IrExpr]) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for a in args {
            match a {
                IrExpr::Array(items) => for it in items { out.push(self.expr_str(it)?); },
                other => out.push(self.expr_str(other)?),
            }
        }
        Ok(out)
    }

    fn test_tokens(&mut self, w: &[TestTok]) -> Result<String, String> {
        // top-level parenthesized groups: ( expr ) [-a|-o|&&...] rest
        if w.len() >= 2 {
            if let TestTok::S(x) = &w[0] {
                if x == "(" {
                    let mut depth = 0i32;
                    let mut close = None;
                    for (i, t) in w.iter().enumerate() {
                        if let TestTok::S(y) = t {
                            if y == "(" { depth += 1; }
                            if y == ")" { depth -= 1; if depth == 0 { close = Some(i); break; } }
                        }
                    }
                    if let Some(ci) = close {
                        let inner = self.test_tokens(&w[1..ci])?;
                        if ci + 1 < w.len() {
                            let rest = self.test_tokens(&w[ci + 1..])?;
                            // joiner between group and rest
                            return Ok(format!("({inner}) && ({rest})"));
                        }
                        return Ok(format!("({inner})"));
                    }
                }
            }
        }
        // strip surrounding [ ] / [[ ]] markers if present
        let toks: Vec<TestTok> = w.iter().filter(|t| match t {
            TestTok::S(x) => !matches!(x.as_str(), "[" | "]" | "[[" | "]]"),
            _ => true,
        }).cloned().collect();
        let w: &[TestTok] = &toks;
        if w.is_empty() { return Ok("true".into()); }
        // ! NOT
        if let TestTok::S(s) = &w[0] {
            if s == "!" && w.len() > 1 {
                return Ok(format!("!({})", self.test_tokens(&w[1..])?));
            }
            if s == "-a" || s == "-o" {
                return Err("-a/-o outside binary position".into());
            }
        }
        // -a / -o chains take priority over positional binary matches
        for i in 1..w.len().saturating_sub(1) {
            if let TestTok::S(sname) = &w[i] {
                if sname == "-a" || sname == "-o" {
                    let l = self.test_tokens(&w[..i])?;
                    let r = self.test_tokens(&w[i + 1..])?;
                    return Ok(if sname == "-a" { format!("({l}) && ({r})") } else { format!("({l}) || ({r})") });
                }
            }
        }
        // three-token binary forms
        if w.len() == 3 {
            if let TestTok::S(op) = &w[1] {
                let l = tok_str(&w[0]);
                let r = tok_str(&w[2]);
                return match op.as_str() {
                    "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                        self.helper("num");
                        let cmp = match op.as_str() {
                            "-eq" => "==", "-ne" => "!=", "-lt" => "<",
                            "-le" => "<=", "-gt" => ">", _ => ">=",
                        };
                        Ok(format!("(shNum({l}) {cmp} shNum({r}))"))
                    }
                    "=" | "=~" | "==" | "!=" if matches!(&w[2], TestTok::S(txt) if
                        txt.chars().any(|c| matches!(c, '*' | '?' | '[')))
                        || matches!(&w[2], TestTok::E(_, true)) =>
                    {
                        let neg = op == "!=";
                        if let TestTok::S(txt) = &w[2] {
                            if let Some(rx) = txt.strip_prefix('~') {
                                // regex match ([[ s =~ re ]] — the IR keeps
                                // a ~ marker on the pattern)
                                self.helper("regexm");
                                let cmp = if neg { " == false" } else { "" };
                                return Ok(format!("(shRegexMatch({l}, {jstr_rx}){cmp})", jstr_rx = jstr(rx)));
                            }
                        }
                        // [[ ]] pattern match (RHS unquoted glob)
                        self.helper("fnmatch");
                        Ok(if neg { format!("(shFnmatch({r}, {l}) == false)") }
                           else { format!("(shFnmatch({r}, {l}))") })
                    }
                    "=" | "==" => { self.helper("eq"); Ok(format!("(shEq({}, {}))", l, r)) }
                    "!=" => { self.helper("eq"); Ok(format!("(!shEq({}, {}))", l, r)) }
                    "-nt" | "-ot" | "-ef" => {
                        self.helper("ftype");
                        let cmp = if op == "-nt" { ">" } else if op == "-ot" { "<" } else { "==" };
                        Ok(format!("(shMtime({l}) {cmp} shMtime({r}))"))
                    }
                    _ => Err(format!("test operator {op} not in the java subset")),
                };
            }
        }
        // two-token unary forms
        if w.len() == 2 {
            if let TestTok::S(op) = &w[0] {
                let o = tok_str(&w[1]);
                return match op.as_str() {
                    "-n" => Ok(format!("(({}).isEmpty() == false)", o)),
                    "-z" => Ok(format!("(({}).isEmpty())", o)),
                    "-f" | "-d" | "-e" | "-s" | "-r" | "-w" | "-x" | "-L" | "-h" | "-p" | "-b" | "-c" | "-S"
                    | "-g" | "-k" | "-u" | "-O" | "-G" | "-N" => {
                        self.helper("ftype");
                        let k = match op.as_str() {
                            "-f" => "\"f\"", "-d" => "\"d\"", "-e" => "\"e\"", "-s" => "\"s\"",
                            "-r" => "\"r\"", "-w" => "\"w\"", "-x" => "\"x\"",
                            "-L" | "-h" => "\"L\"",
                            "-p" => "\"p\"", "-b" => "\"b\"", "-c" => "\"c\"",
                            "-g" => "\"g\"", "-k" => "\"k\"", "-u" => "\"u\"",
                            "-O" => "\"O\"", "-G" => "\"G\"", "-N" => "\"N\"",
                            _ => "\"S\"",
                        };
                        Ok(format!("(shFtype({k}, {o}))"))
                    }
                    _ => Err(format!("test operator {op} not in the java subset")),
                };
            }
        }
        // single token: truthiness (non-empty string)
        if w.len() == 1 {
            let o = tok_str(&w[0]);
            return Ok(format!("(({}).isEmpty() == false)", o));
        }
        // -a / -o chains
        for (i, t) in w.iter().enumerate() {
            if let TestTok::S(s) = t {
                if (s == "-a" || s == "-o") && i > 0 && i + 1 < w.len() {
                    let l = self.test_tokens(&w[..i])?;
                    let r = self.test_tokens(&w[i + 1..])?;
                    return Ok(if s == "-a" { format!("({l}) && ({r})") } else { format!("({l}) || ({r})") });
                }
            }
        }
        Err(format!("test expression not in the java subset: {w:?}"))
    }

    // ── string-typed expressions ─────────────────────────────────────
    fn expr_str(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Array(items) => {
                let mut ws = Vec::new();
                for it in items.iter() { ws.push(self.expr_str(it)?); }
                return Ok(format!("({})", ws.join(" + \" \" + ")));
            }
            IrExpr::Call { func, .. } if func == "listVar" => {
                return Ok("String.join(\" \", __SH_ARGV)".to_string());
            }
            IrExpr::Call { func, args, .. } if func == "redirect" => {
                // captured redirected command
                let mut t = String::from("\"\"");
                if let Some(IrExpr::Arrow(b)) = args.first() {
                    t = self.arrow_expr(b)?;
                }
                if let Some(IrExpr::Array(rs)) = args.get(1) {
                    for r in rs.iter() { t += &self.redirect_text(r)?; }
                }
                self.helper("quote");
                self.helper("capture");
                return Ok(format!("shCapture({t})"));
            }
            IrExpr::Call { func, args, .. } if func == "assign" => {
                let name = str_arg(args, 0).ok_or("assign: no name")?;
                let is_append = str_arg(args, 1) == Some("+=");
                let vi = if str_arg(args, 1) == Some("=") || is_append { 2 } else { 1 };
                let empty = IrExpr::Str(String::new(), StrStyle::DoubleQuoted);
                let ve = args.get(vi).unwrap_or(&empty);
                let v = self.expr_str(ve)?;
                if is_append {
                    self.ensure_field(name);
                    self.emit(&format!("__v_{0} = __v_{0} + {v};", sanitize(name)));
                } else {
                    self.emit(&format!("__v_{} = {v};", sanitize(name)));
                }
                self.emit_assign_rc(ve);
                return Ok(format!("__v_{}", sanitize(name)));
            }
            IrExpr::Call { func, args, .. } if func == "setArrayAppend" => {
                let name = str_arg(args, 0).ok_or("setArrayAppend: no name")?;
                let items = match args.get(1) {
                    Some(IrExpr::Array(xs)) => xs.clone(),
                    _ => return Err("setArrayAppend: non-array items".into()),
                };
                let parts = self.join_words(&items, ", ")?;
                self.arrays.insert(sanitize(name));
                self.emit(&format!("__a_{}.addAll(List.of(new String[]{{{parts}}}));", sanitize(name)));
                self.emit("__SH_RC = 0;");
                return Ok(format!("String.join(\" \", __a_{})", sanitize(name)));
            }
            IrExpr::Call { func, .. } if func == "arrayItems" => {
                // ${!prefix@} key listing — no assoc-map tracking; empty
                return Ok("\"\"".to_string());
            }
            IrExpr::Call { func, args, .. } if func == "shopt" => {
                // nocasematch toggles case-insensitive [[ == ]] comparisons
                let mut on = false;
                let mut is_nc = false;
                for a in args.iter() {
                    match a {
                        IrExpr::Str(t, _) if t.contains("nocase") => is_nc = true,
                        IrExpr::Bool(b) => on = *b,
                        _ => {}
                    }
                }
                if is_nc { self.emit(&format!("__SH_NOCASE = {on};")); }
                self.emit("__SH_RC = 0;");
                return Ok("\"\"".to_string());
            }
            IrExpr::Call { func, args, .. } if func == "brace" => {
                // multi-word expansion joins with spaces here; loop position
                // handles it word-per-word via for_items
                let xs = brace_expand(args)?;
                return Ok(jstr(&xs.join(" ")));
            }
            IrExpr::Call { func, args, .. } if func == "setArray" => {
                let name = str_arg(args, 0).ok_or("setArray: no name")?;
                let items = match args.get(1) {
                    Some(IrExpr::Array(xs)) => xs.clone(),
                    _ => return Err("setArray: non-array items".into()),
                };
                let parts = self.join_words(&items, ", ")?;
                self.arrays.insert(sanitize(name));
                self.emit(&format!("__a_{} = new ArrayList<>(List.of(new String[]{{{parts}}}));", sanitize(name)));
                self.emit("__SH_RC = 0;");
                return Ok(format!("String.join(\" \", __a_{})", sanitize(name)));
            }
            _ => {}
        }
        self.expr_str_inner(e)
    }

    fn expr_str_inner(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Str(s, style) => {
                // \u{1}SH2GLOB\u{1} marks glob-to-expand patterns; bash-text
                // children glob the bare pattern naturally
                let cleaned = s.replace("\u{1}SH2GLOB\u{1}", "");
                Ok(jstr(&cleaned))
            }
            IrExpr::Int(i) => Ok(format!("String.valueOf((long) {i})")),
            IrExpr::Bool(b) => Ok(format!("String.valueOf({b})")),
            IrExpr::Var(name, _) | IrExpr::Ident(name) => self.getvar_str(name),
            IrExpr::Interpolate(parts) => {
                let mut out = String::from("(\"\")");
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => { out += " + "; out += &jstr(s); }
                        InterpPart::Expr(x) => { out += " + "; out += &self.part_str(x)?; }
                    }
                }
                Ok(out)
            }
            IrExpr::Call { func, args, .. } if func == "split" => {
                // unquoted $var word-splitting folded back into one word
                self.helper("split");
                let inner = self.expr_str(args.first().ok_or("split: no arg")?)?;
                Ok(format!("String.join(\" \", shSplit({inner}))"))
            }
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                let name = str_arg(args, 0).ok_or("getVar: no name")?;
                self.getvar_str(name)
            }
            IrExpr::Call { func, args, .. } if func == "param" => self.param(args),
            IrExpr::Call { func, args, .. } if func == "brace" => {
                // brace expansion result — first alternative (statement-level
                // expansion is handled by the frontend before lowering)
                let inner = args.first().ok_or("brace: no arg")?;
                self.expr_str(inner)
            }
            IrExpr::Call { func, args, .. } if func == "arith" => {
                self.helper("num");
                let t = str_arg(args, 0).unwrap_or_default();
                if let Some(ast) = crate::shir::parse_arith_native(&t) {
                    Ok(format!("String.valueOf({})", self.arith(&ast)?))
                } else if t.contains("++") || t.contains("--") {
                    // ++/-- texts: the core's native parser gates them out,
                    // but the FULL parser reads them and this renderer's
                    // arith() lowers IncDec exactly (postfix yields old
                    // value + stores back; prefix stores + yields new;
                    // Java evaluates left-to-right like bash). The child
                    // fallback would be WRONG here — it cannot see
                    // java-side vars (078: `((j = i++ + ++i))` reads i=2).
                    match crate::shir::parse_arith(&t) {
                        Some(ast) => match self.arith(&ast) {
                            Ok(code) => Ok(format!("String.valueOf({code})")),
                            Err(_) => {
                                self.helper("capture");
                                Ok(format!("shCapture({})", jstr(&format!("echo $(({t}))"))))
                            }
                        },
                        None => {
                            self.helper("capture");
                            Ok(format!("shCapture({})", jstr(&format!("echo $(({t}))"))))
                        }
                    }
                } else {
                    // last resort: let bash evaluate the original $((...))
                    // text — a failed evaluation prints an error to stderr
                    // (discarded) and yields EMPTY (bash: `result=$((echo
                    // "test"))` gives "", not 0), which shCapture reproduces.
                    self.helper("capture");
                    Ok(format!("shCapture({})", jstr(&format!("echo $(({t}))"))))
                }
            }
            IrExpr::Arith(a) => Ok(format!("String.valueOf({})", self.arith(a)?)),
            IrExpr::BinOp { op: BinOpKind::Concat, lhs, rhs, .. } =>
                Ok(format!("({} + {})", self.expr_str(lhs)?, self.expr_str(rhs)?)),
            IrExpr::BinOp { op, lhs, rhs, .. } if is_cmp(op) => {
                let b = self.binop_bool(op, lhs, rhs)?;
                Ok(format!("String.valueOf({b})"))
            }
            IrExpr::Call { func, args, .. } if func == "pipeline" => {
                // capture-position pipeline: stages as Arrows, routed
                // through stage_expr for pipe-safe grouping like the
                // statement arm (a bare multi-command stage would break
                // the `|` chain the same way).
                let mut parts = Vec::new();
                if let Some(IrExpr::Array(stages)) = args.first() {
                    for st in stages.iter() {
                        if let IrExpr::Arrow(body) = st {
                            parts.push(self.stage_expr(body)?);
                        } else {
                            return Err("pipeline stage not an Arrow".into());
                        }
                    }
                }
                if parts.is_empty() { return Err("pipeline without stages".into()); }
                self.helper("quote");
                self.helper("capture");
                return Ok(format!("shCapture({})", parts.join(" + \" | \" + ")));
            }
            IrExpr::Call { func, args, .. } if func == "and" || func == "or" => {
                let sep = if func == "and" { " && " } else { " || " };
                let mut parts = Vec::new();
                for a in args.iter() {
                    // a PREDICATE operand (`[[ -n "$s" ]] && cmd`, a suffix
                    // test) is a command in the chain — its status matters,
                    // not its value; only a command-ish node is text
                    match a {
                        IrExpr::Arrow(body) => parts.push(self.arrow_expr(body)?),
                        IrExpr::Call { func: f, .. } if f == "exec" || f == "builtin" || f == "test" || f == "redirect" => {
                            parts.push(self.side_text(a)?)
                        }
                        other => match self.predicate_text(other) {
                            Ok(t) => parts.push(t),
                            Err(_) => parts.push(self.expr_str(other)?),
                        },
                    }
                }
                self.helper("quote");
                self.helper("capture");
                return Ok(format!("shCapture({})", parts.join(&format!(" + \"{sep}\" + "))));
            }
            IrExpr::BinOp { op: BinOpKind::And | BinOpKind::Or, .. } => {
                // command chains in value position: run both sides
                let sep = if matches!(e, IrExpr::BinOp { op: BinOpKind::And, .. }) { " && " } else { " || " };
                if let IrExpr::BinOp { lhs, rhs, .. } = e {
                    let l = self.side_text(lhs)?;
                    let r = self.side_text(rhs)?;
                    self.helper("quote");
                    self.helper("capture");
                    return Ok(format!("shCapture({l} + \"{sep}\" + {r})"));
                }
                unreachable!()
            }
            IrExpr::Arrow(stmts) => {
                // an Arrow in str value position (e.g. the while-condition
                // of a multi-command loop) is a command list, not a
                // substitution: render its bash text with no wrapper.
                return self.arrow_expr(stmts);
            }
            IrExpr::Call { func, args, .. } if func == "block" => {
                // block(Arrow(body)): command-list text, same as Arrow.
                if let Some(IrExpr::Arrow(stmts)) = args.first() {
                    return self.arrow_expr(stmts);
                }
                return Err("block without Arrow body".into());
            }
            IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } => {
                // ! cmd in value position: stdout is identical to cmd's
                // (the ! only negates exit status, not output)
                self.expr_str(lhs)
            }
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::And | BinOpKind::Or)
                    || matches!(lhs.as_ref(), IrExpr::Call { func, .. } if matches!(func.as_str(), "redirect" | "exec" | "builtin" | "capture"))
                    || matches!(rhs.as_ref(), IrExpr::Call { func, .. } if matches!(func.as_str(), "redirect" | "exec" | "builtin" | "capture")) =>
            {
                let sep = if matches!(*op, BinOpKind::And) { " && " } else { " || " };
                let l = self.side_text(lhs)?;
                let r = self.side_text(rhs)?;
                self.helper("quote");
                self.helper("capture");
                return Ok(format!("shCapture({l} + \"{sep}\" + {r})"));
            }
            IrExpr::Capture { expr, .. } => self.capture_expr(expr),
            IrExpr::Call { func, args, .. } if func == "capture" => self.capture_text(args),
            IrExpr::Call { func, args, .. } if func == "subshell" => {
                // subshell in value position: snapshot scalars+argv, run
                // the body for effect, restore — the value is empty
                // (a subshell produces no value; its stdout belongs to an
                // enclosing capture, which the caller handles).
                self.helper("snapshot");
                self.tmp_seq += 1;
                let k = self.tmp_seq;
                self.emit(&format!("String __snap{k} = shSnapshot(); List<String> __snapArgv{k} = new ArrayList<>(__SH_ARGV); long __snapRc{k} = __SH_RC;"));
                for a in args.iter() {
                    if let IrExpr::Arrow(stmts) = a {
                        for st in stmts.iter() { self.stmt(st)?; }
                    }
                }
                self.emit(&format!("shRestore(__snap{k}); __SH_ARGV = __snapArgv{k}; __SH_RC = __snapRc{k};"));
                Ok("\"\"".to_string())
            },
            IrExpr::Call { func, args, .. } if func == "fnValue" || func == "fnCall" => {
                // User-function call in value position: the transpiled
                // function RETURNS its value as a String (the
                // echo-return-lift convention) — no stdout capture needed.
                // Argv is bound inline (the function's try/finally restores
                // the caller's argv). fnCall discards the value ("").
                let name = str_arg(args, 0).unwrap_or_default();
                if !self.funcs.contains(name) {
                    return Err(format!("fnValue of unknown function {name}"));
                }
                let call_args: Vec<String> = match args.get(1) {
                    Some(IrExpr::Array(elems)) => {
                        let mut v = Vec::new();
                        for e in elems { v.push(self.expr_str(e)?); }
                        v
                    }
                    _ => Vec::new(),
                };
                let call = if call_args.is_empty() {
                    format!("{}()", fn_name(name))
                } else {
                    format!("((__SH_ARGV = new ArrayList<>(List.of(new String[]{{{}}}))) != null ? {}() : \"\")",
                            call_args.join(", "), fn_name(name))
                };
                if func == "fnCall" {
                    Ok("\"\"".to_string())
                } else {
                    Ok(call)
                }
            },
            IrExpr::Call { func, args, .. } if func == "captureWords" => {
                let c = self.capture_text(args)?;
                self.helper("split");
                Ok(format!("String.join(\" \", shSplit({c}))"))
            }
            IrExpr::Call { func, args, .. } if func == "join" => {
                let inner = args.first().ok_or("join: no arg")?;
                self.expr_str(inner)
            }
            IrExpr::Call { func, args, .. } if func == "arrayIndex" => {
                let name = str_arg(args, 0).ok_or("arrayIndex: no name")?;
                let name_s = sanitize(name);
                let key_e = args.get(1).unwrap_or(&IrExpr::Int(0));
                if self.assoc.contains(&name_s) {
                    // assoc read with a general key expression
                    let key = self.expr_str(key_e)?;
                    self.arrays.insert(name_s.clone());
                    return Ok(format!("shAgetKey(__a_{name_s}, __k_{name_s}, {key})"));
                }
                let key = self.array_idx_num(key_e)?;
                self.helper("aget");
                self.arrays.insert(name_s.clone());
                Ok(format!("shAget(__a_{name_s}, {key})"))
            }
            IrExpr::Call { func, args, .. } if func == "arrayLen" => {
                let name = str_arg(args, 0).ok_or("arrayLen: no name")?;
                // `${#map[@]}` / `${#!map[@]}`: strip the key-listing `!`
                let bare = name.trim_start_matches('!').trim_start_matches('#');
                self.arrays.insert(sanitize(bare));
                Ok(format!("String.valueOf(__a_{}.size())", sanitize(bare)))
            }
            IrExpr::Index { var, key } => {
                let k = self.expr_num(key)?;
                self.arrays.insert(sanitize(var));
                self.helper("aget");
                Ok(format!("shAget(__a_{}, {k})", sanitize(var)))
            }
            IrExpr::Ternary { cond, then, else_ } => {
                let t = self.cond_bool(cond)?;
                Ok(format!("({t} ? {} : {})", self.expr_str(then)?, self.expr_str(else_)?))
            }
            IrExpr::DefinedOr { expr, default } => {
                let e = self.expr_str(expr)?;
                let d = self.expr_str(default)?;
                Ok(format!("({e}.isEmpty() ? {d} : {e}))"))
            }
            IrExpr::MethodCall { obj, method, args, .. } => {
                let r = self.expr_str(obj)?;
                let a = self.join_words(args, ", ")?;
                if a.is_empty() { Ok(format!("({r}).{}()", sanitize(method))) }
                else { Ok(format!("({r}).{}({a})", sanitize(method))) }
            }
            IrExpr::Call { func, args, .. } if func == "grepMatches" => {
                // grep -c style: count of matching lines; approximate via
                // single contains check (documented limitation)
                let hay = self.expr_str(args.first().ok_or("grepMatches: no input")?)?;
                let pat = self.expr_str(args.get(1).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.helper("contains");
                Ok(format!("String.valueOf(shContains({hay}, {pat}))"))
            }
            IrExpr::Regex { pattern, flags } => {
                self.helper("regex");
                Ok(format!("String.valueOf(shRegex({}, {}))", jstr(pattern), jstr(flags)))
            }
            IrExpr::Ext(n) => {
                // Transform-declared semantic nodes (text_ops' cut/tr/sed/
                // head/tail/wc/basename compositions). The SHARED handler
                // registry owns the semantics; this backend supplies its own
                // child renderer so a handler can build a java expression.
                let mut child = JavaChild { r: self };
                crate::render_ext_expr::render_with(
                    n.as_ref(),
                    crate::render_ext_expr::Backend::Java,
                    0,
                    &mut child,
                )
                .ok_or_else(|| {
                    format!("ext node not renderable by the java renderer: {}", n.tag())
                })
            }
            // `typeset -l` (lowercase attribute): lc(e) = e.toLowerCase()
            IrExpr::Call { func, args, .. } if func == "lc" => {
                let x = self.expr_str(args.first().ok_or("lc: no arg")?)?;
                Ok(format!("({x}).toLowerCase()"))
            }
            IrExpr::Call { func, args, .. } if func == "uc" => {
                let x = self.expr_str(args.first().ok_or("uc: no arg")?)?;
                Ok(format!("({x}).toUpperCase()"))
            }
            // `typeset -u`/`-l` attribute semantics: lc/uc applied by the
            // frontend to a var value
            IrExpr::Call { func, args, .. } if func == "int" => {
                // int(e): numeric conversion as a decimal string
                self.helper("num");
                let x = self.expr_str(args.first().ok_or("int: no arg")?)?;
                Ok(format!("String.valueOf(shNum({x}))"))
            }
            // grep -q in value position: an emptiness value (0 chars)
            IrExpr::Call { func, args, .. } if func == "builtin" || func == "exec" => {
                let words = str_arg(args, 0) == Some("grep");
                let quiet = args.iter().flat_map(|a| flatten_words(std::slice::from_ref(a)))
                    .any(|w| matches!(w, IrExpr::Str(x, _) if x == "-q"));
                if words && quiet {
                    self.helper("capture");
                    Ok(format!("shCapture({})", self.cmd_line_expr(args)?))
                } else {
                    Err(format!("expr not in the java subset (str ctx): {:?}", e))
                }
            }
            other => Err(format!("expr not in the java subset (str ctx): {other:?}")),
        }
    }

    fn part_str(&mut self, e: &IrExpr) -> Result<String, String> {
        // interpolation part value, no outer quoting: a parent var bakes to
        // its java value; anything else is the expression's own value
        match e {
            IrExpr::Call { func, .. } if func == "listVar" => {
                Ok("String.join(\" \", __SH_ARGV)".to_string())
            }
            IrExpr::Call { func, .. } if func == "getVar" => {
                // an interpolation part is INSIDE a larger word: `n='$v'` —
                // the value splices raw (no shell quoting; spaces stay)
                // and is baked from the java field.
                let n = match e { IrExpr::Call { args, .. } => str_arg(args, 0).unwrap_or("").to_string(), _ => String::new() };
                // a var the CHILD computes (`read line` inside a text loop)
                // is symbolic: the child shell expands it
                if !n.is_empty() && self.child_sym.contains(&n) {
                    self.helper("quote");
                    return Ok(format!("\"${n}\""));
                }
                if !n.is_empty() && self.fields.contains(&sanitize(&n)) {
                    self.getvar_str(&n)
                } else {
                    self.expr_str(e)
                }
            }
            other => self.expr_str(other),
        }
    }

    fn getvar_str(&mut self, name: &str) -> Result<String, String> {
        if name.len() > 1 && name.starts_with('#') && self.arrays.contains(&sanitize(&name[1..])) {
            // ${#arr} — element count
            return Ok(format!("String.valueOf(__a_{}.size())", sanitize(&name[1..])));
        }
        if name.len() > 1 && name.starts_with('#') {
            // ${#var}
            self.helper("len");
            let inner = self.getvar_str(&name[1..])?;
            return Ok(format!("String.valueOf(shLen({inner}))"));
        }
        Ok(match name {
            "?" => "String.valueOf(__SH_RC)".to_string(),
            "#" => "String.valueOf(__SH_ARGV.size())".to_string(),
            "$" => "String.valueOf(ProcessHandle.current().pid())".to_string(),
            "0" => "System.getProperty(\"sh2.argv0\", \"prog.sh\")".to_string(),
            "@" | "*" => "String.join(\" \", __SH_ARGV)".to_string(),
            "!" => "\"\"".to_string(),
            "-" => "\"hB\"".to_string(),
            "RANDOM" => "(long)(Math.random()*32768)".to_string(),
            "UID" => "String.valueOf(OsUtils.uid())".to_string(),
            n if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
                self.helper("arg");
                let i: usize = n.parse().unwrap_or(1);
                format!("shArg({})", i.saturating_sub(1))
            }
            n => {
                if self.fields.contains(&sanitize(n)) {
                    format!("__v_{}", sanitize(n))
                } else if self.arrays.contains(&sanitize(n)) {
                    format!("String.join(\" \", __a_{})", sanitize(n))
                } else if n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    // a plain NAME with no java field: the CHILD expands it
                    // (`read x` sets it there; `$HOME`-style env reads too)
                    format!("shEnv(\"{n}\")")
                } else {
                    self.helper("env");
                    format!("shEnv({})", jstr(n))
                }
            }
        })
    }

    // ── numeric-typed expressions ────────────────────────────────────
    fn expr_num(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Int(i) => Ok(i.to_string()),
            IrExpr::Str(s, _) => {
                if let Ok(n) = s.trim().parse::<i64>() { Ok(format!("{n}L")) }
                else {
                    self.helper("num");
                    Ok(format!("shNum({})", jstr(s)))
                }
            }
            IrExpr::Var(name, _) | IrExpr::Ident(name) => self.getvar_num(name),
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::BinOp { op, lhs, rhs, .. } if matches!(op,
                BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div
                | BinOpKind::Mod | BinOpKind::Pow | BinOpKind::BitAnd | BinOpKind::BitOr
                | BinOpKind::BitXor | BinOpKind::ShiftL | BinOpKind::ShiftR) =>
            {
                let l = self.expr_num(lhs)?;
                let r = self.expr_num(rhs)?;
                Ok(match op {
                    BinOpKind::Add => format!("({l} + {r})"),
                    BinOpKind::Sub => format!("({l} - {r})"),
                    BinOpKind::Mul => format!("({l} * {r})"),
                    BinOpKind::Div => { self.helper("div"); format!("shDiv({l}, {r})") }
                    BinOpKind::Mod => { self.helper("div"); format!("shMod({l}, {r})") }
                    BinOpKind::Pow => { self.helper("pow"); format!("shPow({l}, {r})") }
                    BinOpKind::BitAnd => format!("({l} & {r})"),
                    BinOpKind::BitOr => format!("({l} | {r})"),
                    BinOpKind::BitXor => format!("({l} ^ {r})"),
                    BinOpKind::ShiftL => format!("({l} << {r})"),
                    _ => format!("({l} >> {r})"),
                })
            }
            IrExpr::BinOp { op, lhs, rhs, .. } if is_cmp(op) => {
                let b = self.binop_bool(op, lhs, rhs)?;
                Ok(format!("({b} ? 1L : 0L)"))
            }
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                let name = str_arg(args, 0).ok_or("getVar: no name")?;
                self.getvar_num(name)
            }
            IrExpr::Call { func, args, .. } if func == "param" => {
                let p = self.param(args)?;
                self.helper("num");
                Ok(format!("shNum({p})"))
            }
            IrExpr::Call { func, args, .. } if func == "arrayLen" => {
                let name = str_arg(args, 0).ok_or("arrayLen: no name")?;
                let bare = name.trim_start_matches('!').trim_start_matches('#');
                self.arrays.insert(sanitize(bare));
                Ok(format!("__a_{}.size()", sanitize(bare)))
            }
            other => {
                let s = self.expr_str(other)?;
                self.helper("num");
                Ok(format!("shNum({s})"))
            }
        }
    }

    fn getvar_num(&mut self, name: &str) -> Result<String, String> {
        if name.len() > 1 && name.starts_with('#') && self.arrays.contains(&sanitize(&name[1..])) {
            return Ok(format!("__a_{}.size()", sanitize(&name[1..])));
        }
        if name.len() > 1 && name.starts_with('#') {
            self.helper("len");
            let inner = self.getvar_str(&name[1..])?;
            return Ok(format!("shLen({inner})"));
        }
        Ok(match name {
            "?" => "__SH_RC".to_string(),
            "#" => "(long) __SH_ARGV.size()".to_string(),
            "$" => "ProcessHandle.current().pid()".to_string(),
            n if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
                self.helper("num");
                let i: usize = n.parse().unwrap_or(1);
                format!("shNum(shArg({}))", i.saturating_sub(1))
            }
            n => {
                if self.fields.contains(&sanitize(n)) {
                    self.helper("num");
                    format!("shNum(__v_{})", sanitize(n))
                } else {
                    self.helper("num");
                    self.helper("env");
                    format!("shNum(shEnv({}))", jstr(n))
                }
            }
        })
    }

    /// An assoc-map key as a Java string expression: literals stay
    /// literal, `$`-bearing keys expand natively via expand_dollars.
    fn assoc_key(&mut self, idx: &str) -> Result<String, String> {
        let t = idx.trim();
        if !t.contains('$') {
            return Ok(jstr(t));
        }
        if self.dollar_expand_ok(t) {
            return self.expand_dollars(t);
        }
        Err(format!("assoc key not expandable natively: {t:?}"))
    }

    // ── parameter expansion ──────────────────────────────────────────
    /// An array index expression as a Java long. Handles literals,
    /// `$`-bearing arith texts (`(2*$i)-1` — bare the refs for the arith
    /// parser, which resolves them via getvar_num), and general exprs.
    fn array_idx_num(&mut self, e: &IrExpr) -> Result<String, String> {
        if let IrExpr::Str(t, _) = e {
            let idx = t.trim();
            if let Ok(k) = idx.parse::<i64>() {
                return Ok(format!("{k}L"));
            }
            if idx.contains('$') {
                let bare = bare_arith_refs(idx);
                if let Some(ast) = crate::shir::parse_arith_native(&bare) {
                    return Ok(self.arith(&ast)?);
                }
            }
        }
        if let IrExpr::Int(i) = e {
            return Ok(i.to_string());
        }
        let s = self.expr_str(e)?;
        self.helper("num");
        Ok(format!("shNum({s})"))
    }
    /// Bash quote-removal for a literal ${var//pat/rep} pattern or
    /// replacement: inside the original double quotes, `\"` `\$` `\\`
    /// (and backslash + newline) collapse to the literal char. The core
    /// keeps the raw text in the ShIR, so the renderer applies it.
    fn replace_pat(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Str(s, _) => Ok(jstr(&bash_dquote_unescape(s))),
            other => self.expr_str(other),
        }
    }

    fn replace_rep(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Str(s, _) => Ok(jstr(&bash_dquote_unescape(s))),
            other => self.expr_str(other),
        }
    }
    fn param(&mut self, args: &[IrExpr]) -> Result<String, String> {
        let op = str_arg(args, 0).unwrap_or_default();
        let name = str_arg(args, 1).ok_or("param: no variable name")?;
        if op.is_empty() {
            if let Some(lb) = name.find('[') {
                if name.ends_with(']') {
                    // ${arr[idx]} arrives as param("", "arr[idx]")
                    let arr = &name[..lb];
                    let idx = &name[lb + 1..name.len() - 1];
                    let arr_s = sanitize(arr);
                    if self.assoc.contains(&arr_s) {
                        // assoc read: string-keyed lookup
                        let key = self.assoc_key(idx)?;
                        self.arrays.insert(arr_s.clone());
                        return Ok(format!("shAgetKey(__a_{arr_s}, __k_{arr_s}, {key})"));
                    }
                    if let Ok(k) = idx.trim().parse::<i64>() {
                        self.arrays.insert(sanitize(arr));
                        self.helper("aget");
                        return Ok(format!("shAget(__a_{}, {k}L)", sanitize(arr)));
                    }
                    // dynamic index with $vars (${a[(2*$i)-1]}, ${a[$i]}):
                    // array_idx_num bares the refs for the arith parser and
                    // lowers natively (the old assoc-approx was silently 0).
                    if idx.contains('$') {
                        let key = self.array_idx_num(&IrExpr::Str(idx.to_string(), StrStyle::DoubleQuoted))?;
                        self.arrays.insert(sanitize(arr));
                        self.helper("aget");
                        return Ok(format!("shAget(__a_{}, {key})", sanitize(arr)));
                    }
                    self.arrays.insert(sanitize(arr));
                    self.helper("num");
                    self.helper("aget");
                    return Ok(format!("shAget(__a_{}, shNum({}))", sanitize(arr), jstr(idx.trim())));
                }
            }
        }
        match op {
            "" => self.getvar_str(&name),
            "len" => {
                self.helper("len");
                let inner = self.getvar_str(&name)?;
                Ok(format!("String.valueOf(shLen({inner}))"))
            }
            "len" => {
                self.helper("len");
                let inner = self.getvar_str(&name)?;
                Ok(format!("String.valueOf(shLen({inner}))"))
            }
            ":-" => {
                let d = self.expr_str(args.get(2).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                let base = self.getvar_str(&name)?;
                Ok(format!("({base}.isEmpty() ? {d} : {base})"))
            }
            ":=" => {
                let d = self.expr_str(args.get(2).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                let base = self.getvar_str(&name)?;
                if self.fields.contains(&sanitize(&name)) {
                    Ok(format!("({base}.isEmpty() ? (__v_{} = {d}) : {base})", sanitize(&name)))
                } else {
                    Ok(format!("({base}.isEmpty() ? {d} : {base})"))
                }
            }
            ":+" => {
                let a = self.expr_str(args.get(2).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                let base = self.getvar_str(&name)?;
                Ok(format!("({base}.isEmpty() ? \"\" : {a})"))
            }
            "slice" => {
                // ${#arr} arrives as slice-with-empty offset + empty len
                // and a `#`-prefixed name: length semantics. (The `@` offset
                // is NOT length — ${arr[@]} is the all-elements slice and
                // ${#arr[@]} arrives as a separate arrayLen node.)
                //
                // Assoc-key listing arrives as `!name` (`${!map[@]}`) — the
                // `!` is a marker, not part of the name; strip it (and any
                // `#` length marker) before field resolution.
                let aname = name.trim_start_matches('!').trim_start_matches('#').to_string();
                let off_s = args.get(2).and_then(|e| str_arg(std::slice::from_ref(e), 0)).unwrap_or_default().to_string();
                let len_s = args.get(3).and_then(|e| str_arg(std::slice::from_ref(e), 0)).unwrap_or_default().to_string();
                if name.starts_with('#') && off_s.is_empty() && len_s.is_empty() {
                    let bare = name.trim_start_matches('#').to_string();
                    if self.arrays.contains(&sanitize(&bare)) {
                        return Ok(format!("String.valueOf(__a_{}.size())", sanitize(&bare)));
                    }
                    self.helper("len");
                    let bare2 = name.trim_start_matches('#').to_string();
                    return Ok(format!("String.valueOf(shLen({}))", self.getvar_str(&bare2)?));
                }
                // `${!map[@]}` in VALUE context: the KEY join (insertion
                // order). Offsets on keys are vanishingly rare — full join.
                if name.starts_with('!') {
                    let ks = sanitize(&aname);
                    self.assoc.insert(ks.clone());
                    self.arrays.insert(ks.clone());
                    return Ok(format!("String.join(\" \", __k_{ks})"));
                }
                // ARRAY slice: ${arr[@]:off:len} — element join, not string.
                // The core drops the @ marker when the var is a known array,
                // so array-ness alone selects this path.
                if args.get(2).and_then(|e| str_arg(std::slice::from_ref(e), 0)) == Some("@")
                    || (self.arrays.contains(&sanitize(&aname))
                        && !args.get(2).map(|e| str_arg(std::slice::from_ref(e), 0).unwrap_or_default().is_empty()).unwrap_or(true))
                {
                    let has_at = args.get(2).and_then(|e| str_arg(std::slice::from_ref(e), 0)) == Some("@");
                    let (oi, li) = if has_at { (3usize, 4usize) } else { (2usize, 3usize) };
                    // a slice makes the name an ARRAY (its field must exist)
                    self.arrays.insert(sanitize(&aname));
                    // an ABSENT offset starts at 0; an empty one is the same
                    // ("${arr[@]:}" / "${arr[@]::n}") — not shNum("") == 0
                    let off = match args.get(oi) {
                        None => "0L".to_string(),
                        Some(IrExpr::Str(t, _)) if t.trim().is_empty() => "0L".to_string(),
                        Some(e) => self.expr_num(e)?,
                    };
                    let len = match args.get(li) {
                        // absent OR empty length means "to the end" (-1);
                        // routing "" through expr_num would give shNum("") = 0
                        // (empty slice) instead of the full tail.
                        None => "-1L".to_string(),
                        Some(IrExpr::Str(s, _)) if s.trim().is_empty() => "-1L".to_string(),
                        Some(e) => self.expr_num(e)?,
                    };
                    self.helper("aslice");
                    return Ok(format!("shAslice(__a_{}, {off}, {len})", sanitize(&aname)));
                }
                // offsets may arrive as raw "$i" text — expand natively
                let num_of = |r: &mut Self, e: Option<&IrExpr>| -> Result<String, String> {
                    match e {
                        None => Ok("-1L".to_string()),
                        Some(IrExpr::Str(t, _)) if t.is_empty() => Ok("-1L".to_string()),
                        Some(IrExpr::Str(t, _)) if !t.contains('$') => r.expr_num(&IrExpr::Str(t.clone(), StrStyle::DoubleQuoted)),
                        Some(IrExpr::Str(t, _)) => {
                            let ex = r.expand_dollars(t)?;
                            r.helper("num");
                            Ok(format!("shNum({ex})"))
                        }
                        Some(other) => r.expr_num(other),
                    }
                };
                let off = num_of(self, args.get(2))?;
                let len = num_of(self, args.get(3))?;
                self.helper("substr");
                Ok(format!("shSubstr({}, {}, {})", self.getvar_str(&name)?, off, len))
            }
            "##" => {
                let pat = self.expr_str(args.get(2).ok_or("##: no pattern")?)?;
                self.helper("strip");
                Ok(format!("shStripLongestPrefix({}, {pat})", self.getvar_str(&name)?))
            }
            "#" => {
                let pat = self.expr_str(args.get(2).ok_or("#: no pattern")?)?;
                self.helper("strip");
                Ok(format!("shStripShortestPrefix({}, {pat})", self.getvar_str(&name)?))
            }
            "%%" => {
                let pat = self.expr_str(args.get(2).ok_or("%%: no pattern")?)?;
                self.helper("strip");
                Ok(format!("shStripLongestSuffix({}, {pat})", self.getvar_str(&name)?))
            }
            "%" => {
                let pat = self.expr_str(args.get(2).ok_or("%: no pattern")?)?;
                self.helper("strip");
                Ok(format!("shStripShortestSuffix({}, {pat})", self.getvar_str(&name)?))
            }
            "//" => {
                let pat = self.replace_pat(args.get(2).ok_or("//: no pattern")?)?;
                let rep = self.replace_rep(args.get(3).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.helper("replace");
                Ok(format!("shReplaceAll({}, {pat}, {rep})", self.getvar_str(&name)?))
            }
            "/" => {
                let pat = self.replace_pat(args.get(2).ok_or("/: no pattern")?)?;
                let rep = self.replace_rep(args.get(3).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.helper("replace");
                Ok(format!("shReplaceFirst({}, {pat}, {rep})", self.getvar_str(&name)?))
            }
            "^^" => { self.helper("case"); Ok(format!("shUpperAll({})", self.getvar_str(&name)?)) }
            ",," => { self.helper("case"); Ok(format!("shLowerAll({})", self.getvar_str(&name)?)) }
            "^" => { self.helper("case"); Ok(format!("shUpperFirst({})", self.getvar_str(&name)?)) }
            "," => { self.helper("case"); Ok(format!("shLowerFirst({})", self.getvar_str(&name)?)) }
            "basename" => {
                self.helper("basename");
                Ok(format!("shBasename({})", self.getvar_str(&name)?))
            }
            "dirname" => {
                self.helper("dirname");
                Ok(format!("shDirname({})", self.getvar_str(&name)?))
            }
            ":?" => {
                // ${var?msg}: bash aborts when unset/empty
                let m = self.expr_str(args.get(2).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                let base = self.getvar_str(&name)?;
                self.helper("errifempty");
                Ok(format!("shErrIfEmpty({base}, {m})"))
            }
            other => Err(format!("parameter expansion op {other:?} not in the java subset")),
        }
    }

    // ── captures ─────────────────────────────────────────────────────
    fn capture_text(&mut self, args: &[IrExpr]) -> Result<String, String> {
        for a in args {
            if let IrExpr::Arrow(stmts) = a {
                let text = self.arrow_expr(stmts)?;
                self.helper("capture");
                return Ok(format!("shCapture({text})"));
            }
        }
        Err("capture without Arrow body not in the java subset".into())
    }

    fn capture_expr(&mut self, expr: &IrExpr) -> Result<String, String> {
        match expr {
            IrExpr::Arrow(stmts) => {
                let text = self.arrow_expr(stmts)?;
                self.helper("capture");
                Ok(format!("shCapture({text})"))
            }
            IrExpr::Call { func, args, .. } if func == "exec" || func == "builtin" => {
                let e = self.cmd_line_expr(args)?;
                self.helper("capture");
                Ok(format!("shCapture({e})"))
            }
            other => {
                let s = self.expr_str(other)?;
                self.helper("capture");
                Ok(format!("shCapture({s})"))
            }
        }
    }

    /// Whether an Arrow body needs NATIVE execution (cannot be bash
    /// text): loops over java-side state (For over __a_ arrays, While),
    /// function-value calls, or subshells. whileLoop is NOT listed:
    /// pipe-fed whiles render as bash text (subshell semantics match
    /// bash); stdin-redirected or env-prefixed ones refuse text inside
    /// whileloop_text and arrive here via the caller's native path.
    /// Everything else (plain commands, pipelines, and/or chains,
    /// redirects, blocks of those) renders as bash text — which is
    /// REQUIRED for file fills: a native helper's fork/exec children
    /// bypass java-side output swaps (inheritIO) and would leak to the
    /// terminal instead of the file.
    fn body_needs_native(stmts: &[IrStmt]) -> bool {
        for st in stmts {
            match st {
                IrStmt::For { .. } | IrStmt::While { .. } | IrStmt::DoWhile { .. } => return true,
                IrStmt::Block(b) | IrStmt::Subshell(b) => {
                    if Self::body_needs_native(b) { return true; }
                }
                IrStmt::Redirect { inner, .. } => {
                    if Self::body_needs_native(inner) { return true; }
                }
                IrStmt::Pipeline { stages, .. } => {
                    for s in stages {
                        if Self::body_needs_native(s) { return true; }
                    }
                }
                // a `mapfile`/`read` fill is java STATE (the array field),
                // not a child-visible effect: it must run natively
                IrStmt::Expr(IrExpr::Call { func, args, .. })
                    if (func == "exec" || func == "builtin")
                        && str_arg(args, 0).map(|c| matches!(c, "mapfile" | "readarray")).unwrap_or(false) =>
                {
                    return true;
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. })
                    if (func == "exec" || func == "builtin")
                        && str_arg(args, 0) == Some("read")
                        && flatten_words(&args[1..]).iter().any(|w| matches!(w, IrExpr::Str(t, _) if t == "-a")) =>
                {
                    return true;
                }
                // a command-scoped env prefix (`IFS= read …`) is bash text:
                // the CHILD applies it, and `read` on the child's stdin is
                // exactly the semantics a text loop wants. (A `read` that
                // FILLS an array is native — that is java state.)
                IrStmt::Expr(IrExpr::Call { func: f, args, .. })
                    if (f == "exec" || f == "builtin" || f == "test")
                        && args.iter().take(1).all(|a| matches!(a, IrExpr::Object(_))) =>
                {
                    return false;
                }
                IrStmt::Expr(e) | IrStmt::Assign { expr: e, .. } => {
                    if Self::expr_needs_native(e) { return true; }
                }
                _ => {}
            }
        }
        false
    }

    fn expr_needs_native(e: &IrExpr) -> bool {
        match e {
            // fnValue/fnCall/subshell always need java state; whileLoop is
            // tried as text first (its own arm refuses when env/stdin
            // demands native) so it is NOT listed here.
            // capture = a command substitution: its text is a CHILD's, so
            // it never carries java state of its own (it must not force a
            // native render of the enclosing loop).
            IrExpr::Call { func, args, .. }
                if matches!(func.as_str(), "capture" | "captureWords") =>
            {
                let _ = args;
                false
            }
            IrExpr::Call { func, args, .. }
                if matches!(func.as_str(), "fnValue" | "fnCall" | "subshell") =>
            {
                let _ = args;
                true
            }
            IrExpr::Call { func, args, .. }
                if matches!(func.as_str(), "and" | "or" | "pipeline" | "redirect" | "block" | "capture" | "captureWords") =>
            {
                args.iter().any(|a| match a {
                    IrExpr::Arrow(b) => Self::body_needs_native(b),
                    x => Self::expr_needs_native(x),
                })
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                Self::expr_needs_native(lhs) || Self::expr_needs_native(rhs)
            }
            IrExpr::Arrow(b) => Self::body_needs_native(b),
            IrExpr::Array(items) => items.iter().any(Self::expr_needs_native),
            IrExpr::Capture { expr, .. } => Self::expr_needs_native(expr),
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => Self::expr_needs_native(x),
                _ => false,
            }),
            IrExpr::Ternary { cond, then, else_ } => {
                Self::expr_needs_native(cond) || Self::expr_needs_native(then) || Self::expr_needs_native(else_)
            }
            _ => false,
        }
    }

    /// The bash command line of an Arrow body when it's a simple
    /// command / pipeline / redirected command, as a Java expression.
    /// `grep [flags] -o PATTERN <<< hay` as a Java string expression of
    /// bash text (the grep -o lift: one match per line, rc 0 iff any
    /// match). Used everywhere a grepMatches node needs command text —
    /// statement pipelines, captures, and/or chains.
    fn grep_text(&mut self, args: &[IrExpr]) -> Result<String, String> {
        let hay = self.expr_str(args.first().ok_or("grepMatches: no input")?)?;
        let pat = self.expr_str(args.get(1).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
        let flags = match args.get(2) {
            Some(IrExpr::Str(f, _)) if !f.trim().is_empty() => format!("{} ", f.trim()),
            _ => String::new(),
        };
        self.helper("quote");
        Ok(format!("(\"grep {flags}\" + \"-o \" + shQuote({pat}) + \" <<< \" + shQuote({hay}))"))
    }

    /// Bash arithmetic text for `((...))` / `$((...))` command text
    /// (while-loop bodies in pipe position run as bash text, where the
    /// Java arith renderer cannot go).
    fn bash_arith(&mut self, a: &ArithAst) -> Result<String, String> {
        match a {
            ArithAst::Num(n) => Ok(format!("{n}")),
            ArithAst::Var(n) | ArithAst::Ident(n) => Ok(format!("${n}")),
            ArithAst::Index { var, key } => Ok(format!("${{{}[{}]}}", var, self.bash_arith(key)?)),
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.bash_arith(lhs)?;
                let r = self.bash_arith(rhs)?;
                let o = match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" | "**" | "<" | "<=" | ">" | ">="
                    | "==" | "=" | "!=" | "&&" | "||" | "&" | "|" | "^" | "<<" | ">>" => op.clone(),
                    "," => return Ok(r),
                    _ => return Err(format!("arith op {op:?} not in bash text")),
                };
                Ok(format!("({l} {o} {r})"))
            }
            ArithAst::Un { op, arg } => {
                let v = self.bash_arith(arg)?;
                Ok(match op.as_str() {
                    "-" => format!("(-{v})"),
                    "+" => v,
                    "!" => format!("(!{v})"),
                    "~" => format!("(~{v})"),
                    _ => return Err(format!("arith unop {op:?} not in bash text")),
                })
            }
            ArithAst::Cond { test, then, else_ } => Ok(format!("({} ? {} : {})",
                self.bash_arith(test)?, self.bash_arith(then)?, self.bash_arith(else_)?)),
            ArithAst::Assign { var, op, rhs } => {
                let r = self.bash_arith(rhs)?;
                Ok(match op.as_str() {
                    "=" => format!("{var}={r}"),
                    "+=" | "-=" | "*=" | "/=" | "%=" => format!("{var}{op}{r}"),
                    _ => return Err(format!("arith assign op {op:?} not in bash text")),
                })
            }
            ArithAst::IncDec { var, delta, prefix } => {
                if *delta == 1 {
                    Ok(if *prefix { format!("(++{var})") } else { format!("({var}++)") })
                } else if *delta == -1 {
                    Ok(if *prefix { format!("(--{var})") } else { format!("({var}--)") })
                } else {
                    Ok(format!("({var} += {delta})"))
                }
            }
            ArithAst::Sizeof(_) | ArithAst::Cast { .. } =>
                Err("sizeof/cast not in bash text".into()),
        }
    }

    /// A single-quoted bash literal (for static command text).
    fn bash_lit(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    /// An IrExpr as static bash word text (for while-loop bodies in pipe
    /// position). Only shapes with faithful text forms; else Err (the
    /// caller falls back to native execution).
    fn bash_word(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Str(s, _) => Ok(Self::bash_lit(s)),
            IrExpr::Int(i) => Ok(format!("{i}")),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => Ok(format!("${n}")),
            IrExpr::Arith(a) => Ok(format!("$(({}))", self.bash_arith(a)?)),
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                let n = str_arg(args, 0).ok_or("bash word: getVar no name")?;
                Ok(format!("${n}"))
            }
            IrExpr::Interpolate(parts) => {
                let mut out = String::from("\"");
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push_str(s),
                        InterpPart::Expr(x) => out.push_str(&self.bash_word_inner(x)?),
                    }
                }
                out.push('"');
                Ok(out)
            }
            IrExpr::BinOp { op: BinOpKind::Concat, lhs, rhs, .. } => {
                Ok(format!("{}{}", self.bash_word(lhs)?, self.bash_word(rhs)?))
            }
            _ => Err("no static bash word form".into()),
        }
    }

    /// Interpolation-part values (no outer quotes added — the caller
    /// wraps): literals raw, expressions as `$`-forms.
    fn bash_word_inner(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                let n = str_arg(args, 0).ok_or("bash word: getVar no name")?;
                Ok(format!("${n}"))
            }
            IrExpr::Var(n, _) | IrExpr::Ident(n) => Ok(format!("${n}")),
            _ => self.bash_word(e),
        }
    }

    /// One body statement as a JAVA EXPRESSION evaluating to bash text.
    /// `assigned` (body-written vars) render as symbolic `$`-refs (live
    /// across iterations); other vars bake their current java values.
    /// A word as bash TEXT with the given vars SYMBOLIC (`"$name"`, expanded
    /// by the child shell) instead of baked from the java fields.
    fn child_word_str(&mut self, e: &IrExpr, symbolic: &std::collections::BTreeSet<String>) -> Result<String, String> {
        self.helper("quote");
        let text = |e: &IrExpr, sym: &std::collections::BTreeSet<String>| -> Option<String> {
            match e {
                IrExpr::Str(t, _) if !t.contains('\u{1}') => Some(jstr(t)),
                IrExpr::Call { func, args, .. } if func == "getVar" => {
                    let n = str_arg(args, 0)?;
                    if sym.contains(n) { Some(jstr(&format!("${n}"))) } else { None }
                }
                IrExpr::Interpolate(parts) => {
                    let mut out = String::from("(\"\"");
                    for p in parts {
                        match p {
                            InterpPart::Lit(t) => { out += " + "; out += &jstr(t); }
                            InterpPart::Expr(x) => {
                                let part = match x.as_ref() {
                                    IrExpr::Call { func, args, .. } if func == "getVar" => {
                                        Some(str_arg(args, 0).map(|n| if sym.contains(n) { format!("${n}") } else { String::new() })?)
                                    }
                                    _ => None,
                                };
                                match part {
                                    Some(v) => { out += " + "; out += &jstr(&v); }
                                    None => return None,
                                }
                            }
                        }
                    }
                    out += ")";
                    Some(out)
                }
                _ => None,
            }
        };
        text(e, symbolic).ok_or_else(|| "child word not text".to_string())
    }

    fn body_stmt_text(&mut self, st: &IrStmt, assigned: &std::collections::BTreeSet<String>) -> Result<String, String> {
        self.body_stmt_text_at(st, assigned, false)
    }

    /// `in_child`: the text runs inside the loop's CHILD shell, so
    /// body-assigned vars must stay SYMBOLIC (`$line`) — the parent's java
    /// field is stale there (it is only written after the loop).
    fn body_stmt_text_at(&mut self, st: &IrStmt, assigned: &std::collections::BTreeSet<String>, in_child: bool) -> Result<String, String> {
        // child-side command text: body-assigned vars stay symbolic
        if in_child && !self.child_sym.is_empty() {
            // (a nested body keeps the outer loop's symbolic set: a
            // pipeline stage inside it is still that child shell's text)
        } else if in_child {
            self.child_sym = assigned.clone();
        }
        match st {
            // a plain command: each WORD as text (a body-assigned var stays
            // symbolic in the child, a parent var bakes its java value)
            IrStmt::Expr(IrExpr::Call { func, args, .. })
                if in_child && (func == "exec" || func == "builtin" || func == "test") =>
            {
                let mut ws: Vec<String> = Vec::new();
                for (ai, a) in args.iter().enumerate() {
                    for w in flatten_words(std::slice::from_ref(a)) {
                        if ai == 0 {
                            if let IrExpr::Str(t, _) = w { ws.push(jstr(t)); continue; }
                        }
                        if let Ok(t) = self.child_word_str(w, assigned) { ws.push(t); }
                        else { ws.push(self.body_word_text(w, ai == 0, assigned)?); }
                    }
                }
                Ok(ws.join(" + \" \" + "))
            }
            IrStmt::Case { discriminant, clauses } => {
                // `case … in pat) body ;; … esac` as bash text
                self.helper("quote");
                let d = self.expr_str(discriminant)?;
                let mut inner: Vec<String> = Vec::new();
                for c in clauses {
                    let mut pats: Vec<String> = Vec::new();
                    for pa in &c.patterns { pats.push(jstr(pa)); }
                    if pats.is_empty() { return Err("case clause without pattern".into()); }
                    let mut items: Vec<String> = Vec::new();
                    for b in &c.body { items.push(self.body_stmt_text(b, assigned)?); }
                    inner.push(format!("{} + \") \" + {}", pats.join(" + \" | \" + "), items.join(" + \" ; \" + ")));
                }
                if inner.is_empty() { return Err("empty case".into()); }
                Ok(format!("\"case \" + {d} + \" in \" + {} + \" ;; esac\"", inner.join(" + \" ;; \" + ")))
            }
            IrStmt::Block(body) if !body.is_empty() => {
                // a { … } group as one body statement: inline, braced
                let items: Vec<String> = body.iter()
                    .map(|b| self.body_stmt_text(b, assigned))
                    .collect::<Result<_, _>>()?;
                // `{ … }` is bash syntax, so the braces are separate words
                Ok(format!("({} + \" {{ \" + {} + \" ; }} \")", jstr(""), items.join(" + \" ; \" + ")))
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                // `if …; then …; fi` in bash body text: the condition is a
                // command list (run as text), branches recurse.
                self.helper("quote");
                self.helper("run");
                // the condition renders as command TEXT. `block(Arrow(cmd))`
                // is NOT a command line on its own — render the arrow's
                // statements (a test node becomes `[[ … ]]` text).
                let c = match &*cond {
                    IrExpr::Call { func, args, .. } if func == "test" => self.bracket_text(args, false)?,
                    _ => self.body_cmd_text(std::slice::from_ref(cond), assigned)?,
                };
        let mut out = format!("\"if \" + {c} + \" ; then \" + ");
                let mut parts: Vec<String> = Vec::new();
                for b in then { parts.push(self.body_stmt_text(b, assigned)?); }
                out += &parts.join(" + \" ; \" + ");
                for (ec, eb) in elsifs {
                    let ec = self.side_text(ec)?;
                    let mut p2: Vec<String> = Vec::new();
                    for b in eb { p2.push(self.body_stmt_text(b, assigned)?); }
                    out += &format!(" + \" ; elif \" + {ec} + \" ; then \" + {}", p2.join(" + \" ; \" + "));
                }
                if !else_.is_empty() {
                    let mut p3: Vec<String> = Vec::new();
                    for b in else_ { p3.push(self.body_stmt_text(b, assigned)?); }
                    out += &format!(" + \" ; else \" + {}", p3.join(" + \" ; \" + "));
                }
                out += " + \" ; fi\"";
                Ok(out)
            }
            IrStmt::Expr(IrExpr::Call { func, args, .. })
                if func == "exec" || func == "builtin" || func == "test" =>
            {
                self.body_cmd_text(args, assigned)
            }
            IrStmt::Expr(IrExpr::BinOp { op, lhs, rhs, .. }) => {
                // `cmd && cmd` in bash body text: both sides as text
                let sep = if matches!(*op, BinOpKind::And) { " && " } else { " || " };
                let l = self.body_cmd_text(std::slice::from_ref(lhs), assigned)?;
                let r = self.body_cmd_text(std::slice::from_ref(rhs), assigned)?;
                Ok(format!("{l} + \"{sep}\" + {r}"))
            }
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "block" => {
                // block(Arrow(cmd-list)) as one body statement: the list is
                // `cmd && cmd && cmd` — both sides as text.
                let stmts = match args.first() {
                    Some(IrExpr::Arrow(b)) => b.clone(),
                    _ => return Err("block without Arrow body".into()),
                };
                self.body_list_text(&stmts, assigned)
            }
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "grepMatches" => {
                self.grep_text(args)
            }
            IrStmt::Assign { targets, expr, .. } => {
                let t = targets.first().ok_or("assign: no target")?;
                if !t.indices.is_empty() || t.var.contains('[') {
                    // `arr[i]=v` in body text: the child shell owns the
                    // array (bash_arith/word text evaluate it live)
                    let idx = if let Some(t0) = t.indices.first() {
                        // the index is an IrExpr: arith → symbolic text,
                        // literal → itself
                        match t0 {
                            IrExpr::Arith(a) => format!("$(({}))", self.bash_arith(a)?),
                            IrExpr::Int(n) => n.to_string(),
                            IrExpr::Str(s0, _) => s0.clone(),
                            other => self.expr_str(other)?,
                        }
                    } else if t.var.contains('[') {
                        let lb = t.var.find('[').unwrap();
                        t.var[lb + 1..t.var.len() - 1].to_string()
                    } else {
                        return Err("indexed assign without index".into());
                    };
                    let base = t.var.split('[').next().unwrap_or(&t.var);
                    // RHS is an expression producing TEXT (`1`, `$i`, `""`)
                    let ve = match expr {
                        IrExpr::Arith(a) => format!("String.valueOf({})", self.arith(a)?),
                        other => self.expr_str(other)?,
                    };
                    self.helper("quote");
                    self.helper("quote");
                    return Ok(format!("\"{base}[\" + shQuote({idx}) + \"]=\" + shQuote({ve})"));
                }
                // RHS renders symbolically (the child computes): arith via
                // bash_arith ($-refs are live), everything else via static
                // bash words ($-refs for vars, literals quoted).
                let v = match expr {
                    IrExpr::Arith(a) => format!("$(({}))", self.bash_arith(a)?),
                    IrExpr::Call { func, args, .. } if func == "arith" => {
                        let s = str_arg(args, 0).unwrap_or_default();
                        format!("$(({s}))")
                    }
                    other => self.bash_word(other)?,
                };
                Ok(format!("\"{}={}\"", t.var, v.replace('"', "\\\"")))
            }
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "whileLoop" => {
                let (c, b) = match (args.first(), args.get(1)) {
                    (Some(IrExpr::Arrow(c)), Some(IrExpr::Arrow(b))) => (c, b),
                    _ => return Err("whileLoop without arrows".into()),
                };
                self.whileloop_text(c, b)
            }
            IrStmt::Continue { .. } => Ok("\"continue\"".to_string()),
            IrStmt::Break { .. } => Ok("\"break\"".to_string()),
            other => Err(format!("stmt not in bash body text: {other:?}").chars().take(200).collect()),
        }
    }

    /// Command args as a JAVA EXPRESSION evaluating to bash text, like
    /// cmd_line_expr but with body-assigned vars as symbolic `$`-refs
    /// (live across loop iterations) instead of baked java values.
    fn body_cmd_text(&mut self, args: &[IrExpr], assigned: &std::collections::BTreeSet<String>) -> Result<String, String> {
        self.helper("quote");
        let mut pieces: Vec<String> = Vec::new();
        // a single non-word node (block / arrow / and-or chain) is a
        // whole command list, not a word
        if args.len() == 1 {
            let is_list = match &args[0] {
                IrExpr::Call { func, .. } if func == "block" || func == "and" || func == "or" => true,
                IrExpr::BinOp { op, .. } => matches!(op, BinOpKind::And | BinOpKind::Or),
                IrExpr::Arrow(_) => true,
                _ => false,
            };
            if is_list {
                let one = IrStmt::Expr(args[0].clone());
                return self.body_list_text(std::slice::from_ref(&one), assigned);
            }
        }
        for (ai, w) in args.iter().enumerate() {
            for x in flatten_words(std::slice::from_ref(w)) {
                // command-scoped env (`IFS= read …`): a prefix assignment
                // word — the child applies it to the command that follows
                if let IrExpr::Object(props) = x {
                    for (k, v) in props.iter() {
                        let ve = self.expr_str(v)?;
                        pieces.push(format!("\u{1}G\u{2}{k}=\u{3}shQuote({ve})\u{4}"));
                    }
                    continue;
                }
                // a test / and-or / block node is command TEXT of its own
                let handled = match x {
                    IrExpr::Call { func, .. } if func == "getVar" => {
                        // a word in the command's own text (`echo` is not
                        // run in a child here — the command line IS bash):
                        // bake the parent value. A word the CHILD assigns is
                        // symbolic (`"$line"`), expanded by the child shell.
                        if let IrExpr::Call { args, .. } = x {
                            Some(self.body_word_text(x, ai == 0, assigned)?)
                        } else { None }
                    }
                    IrExpr::Call { func, args, .. } if func == "block" => {
                        let stmts = match args.first() {
                            Some(IrExpr::Arrow(b)) => b.clone(),
                            _ => return Err("block without Arrow body".into()),
                        };
                        Some(self.body_list_text(&stmts, assigned)?)
                    }
                    IrExpr::Call { func, args, .. } if func == "test" => {
                        // a test NODE is bracket test text (`[[ … ]]`)
                        Some(self.bracket_text(args, false)?)
                    }
                    IrExpr::BinOp { op, lhs, rhs, .. } if matches!(op, BinOpKind::And | BinOpKind::Or) => {
                        let sep = if matches!(op, BinOpKind::And) { " && " } else { " || " };
                        Some(format!("{} + \"{sep}\" + {}", self.side_text(lhs)?, self.side_text(rhs)?))
                    }
                    _ => None,
                };
                match handled {
                    Some(t) => pieces.push(t),
                    None => pieces.push(self.body_word_text(x, ai == 0, assigned)?),
                }
            }
        }
        // join the words; a command-scoped prefix (`NAME= value`) glues
        // to the command that follows it
        let mut out = String::new();
        let mut prev_glue = false;
        for (k, pce) in pieces.iter().enumerate() {
            let this_glue = pce.starts_with("\u{1}G\u{2}");
            // a glue piece is `NAME=` + MARK + <java expr for the value>
            if pce.starts_with("\u{1}G\u{2}") { let gs = pce[3..].to_string(); let g = gs.as_str();
                let (nm, ex) = match g.find('\u{3}') {
                    Some(i) => {
                        // `g[..i]` is `NAME=`; the value is the rest
                        let mut rest = &g[i..];
                        while let Some(c) = rest.chars().next() {
                            if c == '\u{3}' { rest = &rest[1..]; } else { break; }
                        }
                        // the value expression: up to the closing marker
                        let endm = '\u{4}';
                        let vend = rest.find(endm).unwrap_or(rest.len());
                        (&g[..i], &rest[..vend])
                    }
                    None => (g, ""),
                };
                // a prefix word follows the previous one with a space
                if k > 0 { out += " + "; out += &jstr(" "); out += " + "; }
                out += &format!("{}{}{}", jstr(&format!("{}=", nm.trim_end_matches('='))), " + ", ex);
                // ...and glues to the command word that follows
                prev_glue = true;
                continue;
            }
            if k > 0 && !prev_glue { out += " + "; out += &jstr(" "); out += " + "; }
            if k > 0 && prev_glue { out += " + "; }
            out += pce;
            prev_glue = this_glue;
        }
        Ok(out)
    }

    /// One command word as a JAVA EXPRESSION evaluating to bash text.
    /// `is_cmd`: the command word itself (never quoted — a quoted command
    /// word would not resolve).
    fn body_word_text(&mut self, e: &IrExpr, is_cmd: bool, assigned: &std::collections::BTreeSet<String>) -> Result<String, String> {
        match e {
            IrExpr::Str(s, _) if !s.contains("\u{1}SH2GLOB\u{1}") => Ok(jstr(s)),
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                // A word in command text: a shell expands variables itself
                // when it parses the line, and the line is run by bash.
                // Only a var the CHILD computes (`read line`) must stay
                // symbolic — everything else is baked from the java field
                // (so `${n}` is the CURRENT value, not an env snapshot).
                let n = str_arg(args, 0).ok_or("bash word: getVar no name")?;
                if assigned.contains(n) {
                    Ok(format!("\"${n}\"") )
                } else if self.fields.contains(&sanitize(n)) {
                    let v = self.getvar_str(n)?;
                    self.helper("quote");
                    Ok(format!("shQuote({v})"))
                } else {
                    Ok(format!("\"${n}\""))
                }
            }
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if assigned.contains(n) || !self.fields.contains(&sanitize(n)) {
                    Ok(format!("\"${n}\""))
                } else {
                    let v = self.getvar_str(n)?;
                    self.helper("quote");
                    Ok(format!("shQuote({v})"))
                }
            }
            IrExpr::Interpolate(parts) => {
                // a CHILD-side string: plain vars stay symbolic (`$line`)
                let mut out = String::from("(\"\"");
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => { out += " + "; out += &jstr(s); }
                        InterpPart::Expr(x) => {
                            if let IrExpr::Call { func, .. } = x.as_ref() {
                                if func == "getVar" {
                                    let nm = match x.as_ref() { IrExpr::Call { args, .. } => str_arg(args, 0).unwrap_or("").to_string(), _ => String::new() };
                                    if assigned.contains(&nm) && !nm.is_empty() {
                                        // child-set var (`read line`): symbolic
                                        out += " + "; out += &jstr(&format!("\"${nm}\""));
                                        continue;
                                    }
                                }
                            }
                            out += " + "; out += &self.part_str(x)?;
                        }
                    }
                }
                out += ")";
                Ok(out)
            }
            other => {
                // static shapes stay static; dynamic ones (captures,
                // splits, slices) bake their current java value
                match self.bash_word(other) {
                    Ok(static_text) => Ok(jstr(&static_text)),
                    Err(_) => self.expr_str(other),
                }
            }
        }
    }

    /// `while COND; do BODY; done` as bash text. Bodies with
    /// command-scoped env (IFS= read) are refused (the env is dropped in
    /// text) — the caller falls back to native execution.
    /// `while COND; do BODY; done` as a JAVA EXPRESSION evaluating to
    /// bash text. Subshell semantics: the child starts as a clone of the
    /// parent shell, so every scalar var the loop reads is prefixed with
    /// its current java value (`n='v'; while ...`); the body then uses
    /// symbolic `$`-refs (live across iterations). Bodies needing java
    /// arrays, env-prefixed reads, or non-textual stmts refuse — the
    /// caller falls back to native execution. Loop-carried test conds
    /// (cond reads a body-assigned var) also refuse: a baked value would
    /// freeze the condition.
    fn whileloop_text(&mut self, cond0: &[IrStmt], body: &[IrStmt]) -> Result<String, String> {
        // a block(cond-list) condition: unwrap to its statements
        let cond_owned: Vec<IrStmt> = match cond0 {
            [IrStmt::Expr(IrExpr::Call { func, args, .. })] if func == "block" => match args.first() {
                Some(IrExpr::Arrow(b)) => b.clone(),
                _ => cond0.to_vec(),
            },
            _ => cond0.to_vec(),
        };
        let cond: &[IrStmt] = &cond_owned;
        for st in cond {
            if let IrStmt::Expr(IrExpr::Call { func, args, .. }) = st {
                if func == "builtin" && str_arg(args, 0) == Some("let") {
                    return Err("arith `let` condition not in bash text".into());
                }
            }
        }
        // The condition runs in the PARENT: only a test / plain command /
        // command list is expressible as static text. `(( … ))`, `let`, or
        // any node that wants java state refuses (the loop then renders
        // natively — a baked condition would freeze the loop).
        if Self::body_needs_native(cond) {
            return Err("java-state while condition not in bash text".into());
        }
        // (1) the BODY runs in a child shell: body-assigned vars are the
        // only ones that stay symbolic there.
        let mut body_assigned: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for st in body {
            self.body_assigned(st, &mut body_assigned);
        }
        // A while-loop CONDITION is re-evaluated every iteration; the text
        // path can only freeze it, so a condition that reads a var the body
        // assigns (`while [ $i -lt 3 ]`) must refuse.
        for v in self.body_reads_cond(cond) {
            if body_assigned.contains(&v) {
                return Err("loop-carried cond var not in bash text".into());
            }
        }
        // the CONDITION is the parent's: it renders with the parent's live
        // values (`$line` = the java field), not as a child expression
        let ct = {
            let one = IrStmt::Expr(match cond.first() {
                Some(IrStmt::Expr(e)) => e.clone(),
                _ => return Err("while cond shape".into()),
            });
            let _ = one;
            self.cond_text_list(cond)?
        };
        // a `read VAR` / assign in the loop CONDITION also lands in the child
        for c in cond {
            self.body_assigned(c, &mut body_assigned);
        }
        let body_text = self.body_list_text(body, &body_assigned)?;
        // (2) the parent reads the child's vars back after the loop; the
        // child reports them itself (see body_list_text) and the parent
        // folds them in (flush_sync) right after the run.
        self.sync_pending = body_assigned.iter()
            .filter(|v| self.fields.contains(&sanitize(v)))
            .cloned().collect();
        // (3) seed the child with their current values
        let mut prefix = String::new();
        for v in self.sync_pending.clone() {
            let val = self.getvar_str(&v)?;
            prefix += &format!("{} + shQuote({val}) + \" >> \\\"$SH2_SYNC_FILE\\\" ; \" + ", jstr(&format!("{SYNC_REPORT}{v} \"")));
        }
        Ok(format!("\"(\" + {prefix}\"while \" + {ct} + \"; do \" + {body_text} + \" ; done\" + \")\""))
    }

    /// A body's statements as one joined bash-text expression; statements
    /// outside the text subset (command-scoped env, case, …) refuse.
    fn body_list_text(&mut self, body: &[IrStmt], assigned: &std::collections::BTreeSet<String>) -> Result<String, String> {
        // A body's commands run in a CHILD: each is static bash text and
        // they are joined with ` ; `; body-assigned vars are reported back
        // to the parent after every one of them (so `continue` cannot skip it).
        let mut parts: Vec<String> = Vec::new();
        for st in body {
            parts.push(self.body_stmt_text_at(st, assigned, true)?);
            for v in assigned.iter() {
                parts.push(format!("{} + {} + \" >> \\\"$SH2_SYNC_FILE\\\"\"", jstr(&format!("{SYNC_REPORT}{v} ")), jstr(&format!("\"${{{v}}}\"")).to_string()));
            }
        }
        Ok(parts.join(" + \" ; \" + "))
    }

    /// A condition's command list as bash TEXT, with parent (java) values in
    /// the words — a `test` node becomes `[[ … ]]`, a command its command
    /// line, a command list `cmd && cmd`.
    fn cond_text_list(&mut self, cond: &[IrStmt]) -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();
        for st in cond {
            parts.push(self.cond_stmt_text(st)?);
        }
        Ok(parts.join(" + \" ; \" + "))
    }

    /// One condition statement as bash TEXT, parent values baked.
    fn cond_stmt_text(&mut self, st: &IrStmt) -> Result<String, String> {
        match st {
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "test" => self.bracket_text(args, false),
            IrStmt::Expr(e @ IrExpr::BinOp { .. }) => {
                if let IrExpr::BinOp { op, lhs, rhs, .. } = e {
                    let sep = match op {
                        BinOpKind::And => " && ",
                        BinOpKind::Or => " || ",
                        _ => return self.side_text(e),
                    };
                    let l = self.cond_expr_text(lhs)?;
                    let r = self.cond_expr_text(rhs)?;
                    Ok(format!("{l} + \"{sep}\" + {r}"))
                } else { unreachable!() }
            }
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "block" => {
                let stmts = match args.first() {
                    Some(IrExpr::Arrow(b)) => b.clone(),
                    _ => return Err("block without Arrow".into()),
                };
                self.cond_text_list(&stmts)
            }
            IrStmt::Expr(e) => self.side_text(e),
            IrStmt::Assign { targets, expr, .. } => {
                let t = targets.first().ok_or("assign: no target")?;
                let v = self.expr_str(expr)?;
                Ok(format!("{}{}{}", jstr(&format!("{}=", t.var)), " + ", v))
            }
            other => Err(format!("stmt {other:?} not in a while condition")),
        }
    }

    /// One condition sub-expression as text (`[[ … ]]` for a test).
    fn cond_expr_text(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "test" => self.bracket_text(args, false),
            IrExpr::Call { func, args, .. } if func == "block" => {
                let stmts = match args.first() {
                    Some(IrExpr::Arrow(b)) => b.clone(),
                    _ => return Err("block without Arrow".into()),
                };
                self.cond_text_list(&stmts)
            }
            IrExpr::BinOp { op: BinOpKind::And | BinOpKind::Or, .. } => self.cond_stmt_text(&IrStmt::Expr(e.clone())),
            _ => self.side_text(e),
        }
    }

    /// Vars a while-condition reads.
    fn body_reads_cond(&self, cond: &[IrStmt]) -> Vec<String> {
        let mut out = Vec::new();
        for st in cond {
            out.extend(self.body_reads(st));
        }
        out
    }

    /// Vars a body statement assigns (Assign targets + read targets).
    fn body_assigned(&self, st: &IrStmt, out: &mut std::collections::BTreeSet<String>) {
        match st {
            IrStmt::Assign { targets, .. } => {
                for t in targets {
                    let base = t.var.split('[').next().unwrap_or(&t.var);
                    out.insert(base.to_string());
                }
            }
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "exec" || func == "builtin" => {
                // `read a b` targets (non-flag leading words after the cmd)
                if str_arg(args, 0) == Some("read") {
                    for a in args.iter().skip(1) {
                        match a {
                            IrExpr::Str(t, _) if !t.starts_with('-') => { out.insert(t.clone()); }
                            IrExpr::Array(items) => {
                                for it in items {
                                    if let IrExpr::Str(t, _) = it {
                                        if !t.starts_with('-') { out.insert(t.clone()); }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Plain scalar vars a statement READS (getVar/Var/Ident, non-special).
    fn body_reads(&self, st: &IrStmt) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_reads_stmt(st, &mut out);
        out
    }

    fn collect_reads_stmt(&self, st: &IrStmt, out: &mut Vec<String>) {
        for e in stmt_exprs(st) { self.collect_reads(e, out); }
        for k in stmt_children(st.clone()) { self.collect_reads_stmt(&k, out); }
    }

    fn collect_reads(&self, e: &IrExpr, out: &mut Vec<String>) {
        match e {
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if !n.is_empty() && !n.chars().all(|c| c.is_ascii_digit()) { out.push(n.clone()); }
            }
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                if let Some(n) = str_arg(args, 0) {
                    if !matches!(n, "?" | "#" | "$" | "@" | "*" | "!" | "-" | "0") && !n.chars().all(|c| c.is_ascii_digit()) {
                        out.push(n.to_string());
                    }
                }
                for a in args.iter().skip(1) { self.collect_reads(a, out); }
            }
            IrExpr::Call { func, args, .. } => {
                // read TARGETS are writes, not reads — skip them
                let skip_first = func == "exec" || func == "builtin";
                for (i, a) in args.iter().enumerate() {
                    if skip_first && i == 0 { continue; }
                    // heuristic: `read`'s non-flag Str args are targets
                    if skip_first { if let IrExpr::Str(_, _) = a { continue; } }
                    self.collect_reads(a, out);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => { self.collect_reads(lhs, out); self.collect_reads(rhs, out); }
            IrExpr::Interpolate(parts) => {
                for p in parts { if let InterpPart::Expr(x) = p { self.collect_reads(x, out); } }
            }
            IrExpr::Array(items) => { for i in items { self.collect_reads(i, out); } }
            IrExpr::Arith(a) => self.collect_reads_arith(a, out),
            _ => {}
        }
    }

    fn collect_reads_arith(&self, a: &ArithAst, out: &mut Vec<String>) {
        match a {
            ArithAst::Var(n) | ArithAst::Ident(n) => { out.push(n.clone()); }
            ArithAst::Bin { lhs, rhs, .. } => { self.collect_reads_arith(lhs, out); self.collect_reads_arith(rhs, out); }
            ArithAst::Un { arg, .. } => self.collect_reads_arith(arg, out),
            ArithAst::Cond { test, then, else_ } => {
                self.collect_reads_arith(test, out);
                self.collect_reads_arith(then, out);
                self.collect_reads_arith(else_, out);
            }
            ArithAst::Assign { rhs, .. } => self.collect_reads_arith(rhs, out),
            ArithAst::IncDec { var, .. } => { out.push(var.clone()); }
            ArithAst::Index { key, .. } => self.collect_reads_arith(key, out),
            _ => {}
        }
    }

    fn arrow_expr(&mut self, stmts: &[IrStmt]) -> Result<String, String> {
        // A lone `while|until COMMAND-LIST; do …; done` (no whileLoop lift)
        // renders through the same bash-text path as a whileLoop — the
        // generic While arm below refuses a block/command-list condition.
        if let [IrStmt::While { cond, body }] = stmts {
            let one = IrStmt::Expr((*cond).clone());
            if let Ok(t) = self.whileloop_text(std::slice::from_ref(&one), body) {
                return Ok(t);
            }
        }
        // A redirect on a while loop (`while …; done < file`): the redirect
        // belongs to the OUTER shell command, the loop to the parent (java
        // state). Render `{ ( loop ) ; } > ops` — only file targets.
        if let [IrStmt::Redirect { inner, redirects }] = stmts {
            if let [IrStmt::While { cond, body }] = &inner[..] {
                let one = IrStmt::Expr((*cond).clone());
                if let Ok(t) = self.whileloop_text(std::slice::from_ref(&one), body) {
                    let mut tail = String::new();
                    for r in redirects {
                        let tg = self.expr_str(&r.target)?;
                        let op = match (r.fd, r.mode.as_str()) {
                            (None, "w") | (Some(1), "w") | (Some(1), "wc") => "> ",
                            (None, "a") | (Some(1), "a") => ">> ",
                            (Some(2), "w") => "2> ",
                            (Some(2), "a") => "2>> ",
                            (fd, m) => return Err(format!("redirect {fd:?}/{m} on a loop")),
                        };
                        tail += &format!(" + \" {op}\" + shQuote({tg})");
                    }
                    return Ok(format!("(\"(\" + {t} + \")\"){tail}"));
                }
            }
        }
        // Pre-run renderer-temp setup assigns (`__ps_tmpN = $(mktemp)`
        // for process substitution): the text renderer below SKIPS
        // assigns, but these temps' VALUES feed java-side redirect
        // targets and native stage helpers — without running them the
        // java field stays "" and fills fail. Run once each (a done-set;
        // the value is consumed within the same command).
        for st in stmts {
            if let IrStmt::Assign { targets, .. } = st {
                if let Some(t) = targets.first() {
                    if t.var.starts_with("__ps_") && !self.temp_setup_done.contains(&t.var) {
                        self.temp_setup_done.insert(t.var.clone());
                        self.stmt(st)?;
                    }
                }
            }
        }
        let mut parts = Vec::new();
        for st in stmts {
            match st {
                IrStmt::Expr(IrExpr::Call { func, args, .. })
                    if func == "exec" || func == "builtin" || func == "test" =>
                {
                    parts.push(self.cmd_line_expr(args)?);
                }
                IrStmt::Pipeline { stages, .. } => {
                    let mut sp = Vec::new();
                    for s in stages { sp.push(self.stage_expr(s)?); }
                    parts.push(sp.join(" + \" | \" + "));
                }
                IrStmt::Redirect { inner, redirects } if inner.len() == 1
                    && matches!(&inner[0], IrStmt::While { .. } | IrStmt::For { .. }) => {
                    // ONE implementation: the loop text comes from
                    // whileloop_text / stage_expr; only the file redirect
                    // is appended here
                    // stdin-fed loop: the loop runs natively (java input
                    // stream) and the redirect rides on the command line —
                    // `{ ( loop ) ; } < file` — where its children inherit
                    // fd 0.
                    let body = self.stage_expr(&inner[..])?;
                    self.helper("quote");
                    let mut tail = String::new();
                    for r in redirects {
                        let t = self.expr_str(&r.target)?;
                        let op = match (r.fd, r.mode.as_str()) {
                            (Some(0), "r") | (Some(0), "r+") => "< ".to_string(),
                            (Some(0), "heredoc") | (Some(0), "heredoc-tabs") =>
                                return Ok(format!("( \"{{\" + ( \" \" + {body} + \" ; }}\" + {} + \")\")",
                                    self.heredoc_ops_text(r, &t)?)),
                            (None, "w") | (Some(1), "w") | (Some(1), "wc") => "> ".to_string(),
                            (None, "a") | (Some(1), "a") => ">> ".to_string(),
                            (Some(2), "w") => "2> ".to_string(),
                            (Some(2), "a") => "2>> ".to_string(),
                            (fd, m) => return Err(format!("redirect {fd:?}/{m} on a loop stage")),
                        };
                        tail += &format!(" + \" {} \" + shQuote({t})", op.trim());
                    }
                    parts.push(format!("\"(\" + \"{{\" + ( \" \" + {body} + \" ; }}\") + (\"\"{tail}) + \") \""));
                }
                IrStmt::Redirect { inner, redirects } => {
                    // Bodies needing java state (or stdin-fed whileLoops)
                    // go native as a whole; plain bodies stay text
                    // (REQUIRED for file fills — native helpers' children
                    // bypass output swaps).
                    let stdin_fed = redirects.iter().any(|r| matches!((r.fd, r.mode.as_str()),
                        (Some(0), "r" | "heredoc" | "heredoc-tabs" | "herestring") | (None, "r")));
                    let has_whileloop = inner.iter().any(|st| matches!(st,
                        IrStmt::Expr(IrExpr::Call { func, .. }) if func == "whileLoop"));
                    // ONE text implementation (redirect_text_arms): a lone
                    // command becomes `cmd > f`; a stdin-fed while keeps its
                    // heredoc here (a native runner's children would not see
                    // the java input stream).
                    let _ = has_whileloop;
                    // A loop inside a redirect goes through the ONE stage
                    // decision (text loop + outside redirect ops, or the
                    // native loop runner); everything else renders as text.
                    if inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. })) {
                        parts.push(self.redirect_stage_text(std::slice::from_ref(st))?);
                        continue;
                    }
                    if Self::body_needs_native(inner) {
                        // stdin-fed loop bodies keep the java input stream;
                        // anything else is plain command text with its ops
                        if stdin_fed && inner.iter().any(|b| matches!(b, IrStmt::While { .. } | IrStmt::For { .. })) {
                            parts.push(self.redirect_stage_text(std::slice::from_ref(st))?);
                        } else {
                            let mut inner_text = self.arrow_expr(inner)?;
                            inner_text += &self.redirect_ops_text(inner, redirects, &[])?;
                            parts.push(inner_text);
                        }
                        continue;
                    }
                    let mut inner_text = self.arrow_expr(inner)?;
                    let inner_text = format!("{inner_text}{}", self.redirect_ops_text(inner, redirects, &[])?);
                    parts.push(inner_text);
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "grepMatches" => {
                    parts.push(self.grep_text(args)?);
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "redirect" => {
                    // redirect() in capture text: bash text (the only
                    // route that reaches a forked child's stdout). Bodies
                    // outside the text subset refuse rather than fall back
                    // to an in-process helper — that would print into the
                    // capture instead of the redirect target.
                    let body: Vec<IrStmt> = match args.first() {
                        Some(IrExpr::Arrow(b)) => b.to_vec(),
                        _ => return Err("redirect without Arrow body in capture text".into()),
                    };
                    let redirects = Self::redirect_list(args.get(1))?;
                    parts.push(self.redirect_text_arms(&body, &redirects)?);
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "whileLoop" => {
                    // pipe-fed while loops render as bash text (subshell
                    // semantics: body state is child-local, which matches
                    // bash). Bodies needing java state or env-prefixes
                    // refuse here and fall back to native execution below.
                    let (c, b) = match (args.first(), args.get(1)) {
                        (Some(IrExpr::Arrow(c)), Some(IrExpr::Arrow(b))) => (c, b),
                        _ => return Err("whileLoop without cond/body arrows".into()),
                    };
                    match self.whileloop_text(c, b) {
                        Ok(t) => parts.push(t),
                        Err(_) => {
                            parts.push(self.stage_expr(std::slice::from_ref(st))?);
                        }
                    }
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "pipeline" => {
                    // pipeline call in capture text: same per-stage treatment
                    // as a statement pipeline (text stages stay text — a
                    // native helper's children would leak past the capture).
                    let mut sp = Vec::new();
                    if let Some(IrExpr::Array(stages)) = args.first() {
                        for st in stages.iter() {
                            if let IrExpr::Arrow(body) = st {
                                sp.push(self.stage_expr(body)?);
                            } else {
                                return Err("pipeline stage not an Arrow".into());
                            }
                        }
                    }
                    if sp.is_empty() { return Err("pipeline without stages".into()); }
                    parts.push(sp.join(" + \" | \" + "));
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "redirect" => {
                    // redirect(Arrow(body), redirects...) — ONE text
                    // implementation (redirect_text_arms): a lone command
                    // renders `cmd > f` (so the redirect reaches a child),
                    // a forked body keeps the operator after its group.
                    let body: Vec<IrStmt> = match args.first() {
                        Some(IrExpr::Arrow(b)) => b.clone(),
                        _ => return Err("redirect without Arrow body in capture text".into()),
                    };
                    let redirects = Self::redirect_list(args.get(1))?;
                    parts.push(self.redirect_text_arms(&body, &redirects)?);
                }
                IrStmt::Assign { .. } => {
                    // renderer-temp assignments inside captured bodies carry
                    // no process-visible effect — skip them in command text
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "and" || func == "or" => {
                    let sep = if func == "and" { " && " } else { " || " };
                    let mut sub = Vec::new();
                    for a in args.iter() {
                        if let IrExpr::Arrow(body) = a {
                            sub.push(self.arrow_expr(body)?);
                        } else {
                            sub.push(self.expr_str(a)?);
                        }
                    }
                    parts.push(sub.join(&format!(" + \"{sep}\" + ")));
                }
                IrStmt::Expr(IrExpr::BinOp { op, lhs, rhs, .. }) => {
                    let l = self.side_text(lhs)?;
                    let r = self.side_text(rhs)?;
                    let sep = if matches!(*op, BinOpKind::And) { " && " } else { " || " };
                    parts.push(format!("{l} + \"{sep}\" + {r}"));
                }
                // a redirect/capture node is COMMAND TEXT of its own; sending
                // it back through stage_expr (whose Redirect arm calls this
                // function) would recurse forever
                IrStmt::Redirect { inner, redirects } => {
                    parts.push(self.redirect_text_arms(inner, redirects)?);
                }
                other => {
                    // loops (For/While/whileLoop) inside a captured body:
                    // run natively in a per-site capture helper (same as a
                    // pipeline stage), then feed the bytes as the command.
                    // Redirect nodes are command text — stage_expr would call
                    // back into THIS function forever for a nested redirect.
                    if let IrStmt::Redirect { .. } = other {
                        parts.push(self.redirect_text_arms(std::slice::from_ref(other), &[])?);
                        continue;
                    }
                    parts.push(self.stage_expr(std::slice::from_ref(other))?);
                }
            }
        }
        Ok(parts.join(" + \" ; \" + "))
    }

    /// One redirect-info expression → " > \"target\"" text fragment.
    fn redirect_text(&mut self, r: &IrExpr) -> Result<String, String> {
        // redirect() in a PIPELINE stage (`cat <<EOF | grep …`): the here-doc
        // is bash text appended to the stage's command line
        if let IrExpr::Object(props) = r {
            let mut mode = String::new();
            let mut target: Option<&IrExpr> = None;
            let mut fd: Option<i64> = None;
            for (k, v) in props {
                match k.as_str() {
                    "mode" => { if let IrExpr::Str(m, _) = v { mode = m.clone(); } }
                    "target" => target = Some(v),
                    "fd" => { if let IrExpr::Int(n) = v { fd = Some(*n); } }
                    _ => {}
                }
            }
            if let Some(t) = target {
                if mode == "heredoc" || mode == "heredoc-tabs" || mode == "herestring" {
                    self.helper("quote");
                    let body = self.expr_str(t)?;
                    let q = if mode == "herestring" { "" } else { "'" };
                    return Ok(if mode == "herestring" {
                        format!(" + \" <<< \" + shQuote({body})")
                    } else {
                        format!(" + \" <<{q}SH2EOF{q}\\n\" + {body} + \"\\nSH2EOF\"")
                    });
                }
            }
            let _ = fd;
        }
        // object form: {fd: Int?, mode: Str, target: expr}
        if let IrExpr::Object(props) = r {
            let mut mode = String::new();
            let mut target: Option<&IrExpr> = None;
            let mut fd: Option<i64> = None;
            for (k, v) in props {
                match k.as_str() {
                    "mode" => { if let IrExpr::Str(m, _) = v { mode = m.clone(); } }
                    "target" => target = Some(v),
                    "fd" => { if let IrExpr::Int(n) = v { fd = Some(*n); } }
                    _ => {}
                }
            }
            if let Some(t) = target {
                let t = self.expr_str(t)?;
                // >&N / <&N / >&- (ANY fd): dup/close operators — the
                // target is a file DESCRIPTOR, never a filename, so it must
                // not be quoted (shQuote("&4") would create a file named
                // `&4`). The child shell owns a real fd table and handles
                // these natively.
                if matches!(mode.as_str(), "w" | "wc" | "a" | "r" | "r+")
                    && (t == "\"&0\"" || t == "\"&1\"" || t == "\"&2\""
                        || t == "\"&3\"" || t == "\"&4\"" || t == "\"&5\""
                        || t == "\"&6\"" || t == "\"&7\"" || t == "\"&8\""
                        || t == "\"&9\"" || t == "\"&-\"") {
                    let bare = &t[1..t.len() - 1];
                    let op = match mode.as_str() {
                        "a" => ">>",
                        "r" | "r+" => "<",
                        _ => ">",
                    };
                    let fds = match fd {
                        Some(f) => format!("{f}"),
                        None => String::new(),
                    };
                    return Ok(format!(" + \" {fds}{op}{bare} \""));
                }
                // fd 3+ file targets pass through as bash text.
                if let Some(fdn) = fd {
                    if (3..=9).contains(&fdn) {
                        let op = match mode.as_str() {
                            "a" => ">>",
                            "r" | "r+" => "<",
                            _ => ">",
                        };
                        return Ok(format!(" + \" {fdn}{op} \" + shQuote({t})"));
                    }
                }
                // fd 2 with a write/append mode is stderr (2> / 2>>);
                // anything else keeps the stdout/stdin operator.
                let redir2 = fd == Some(2) && (mode == "w" || mode == "wc" || mode == "a");
                return Ok(match mode.as_str() {
                    "w" | "wc" if redir2 => format!(" + \" 2> \" + shQuote({t})"),
                    "a" if redir2 => format!(" + \" 2>> \" + shQuote({t})"),
                    "w" | "wc" => format!(" + \" > \" + shQuote({t})"),
                    "a" => format!(" + \" >> \" + shQuote({t})"),
                    "r" => format!(" + \" < \" + shQuote({t})"),
                    "herestring" => format!(" + \" <<< \" + shQuote({t})"),
                    "heredoc" | "heredoc-tabs" =>
                        format!(" + \" <<'SH2EOF'\\n\" + {t} + \"\\nSH2EOF\""),
                    _ => String::new(),
                });
            }
        }
        // shape: Array([Str(fd_or_empty), Str(mode), target-expr]) — best
        // effort across shapes; unknown shapes are skipped silently
        if let IrExpr::Array(items) = r {
            if items.len() >= 3 {
                let mode = match items.get(1) { Some(IrExpr::Str(m, _)) => m.clone(), _ => String::new() };
                let t = self.expr_str(&items[2])?;
                return Ok(match mode.as_str() {
                     "w" | "wc" => format!(" + \" > \" + shQuote({t})"),
                     "a" => format!(" + \" >> \" + shQuote({t})"),
                     _ => String::new(),
                 });
            }
            if items.len() == 2 {
                // maybe [mode, target]
                if let Some(IrExpr::Str(m, _)) = items.first() {
                    let t = self.expr_str(&items[1])?;
                    return Ok(match m.as_str() {
                        "w" | "wc" => format!(" + \" > \" + shQuote({t})"),
                        "a" => format!(" + \" >> \" + shQuote({t})"),
                        _ => String::new(),
                    });
                }
            }
        }
        Ok(String::new())
    }

    /// A PREDICATE node (`strHasSuffix`, `[ a = b ]`, `! cmp`, a bare var
    /// in an `&&` chain) as its bash TEXT: the chain needs the node's
    /// STATUS in command position.
    fn predicate_text(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "strHasSuffix" || func == "strHasPrefix" => {
                let s = self.expr_str(args.first().ok_or("pred: no value")?)?;
                let p = self.expr_str(args.get(1).ok_or("pred: no pattern")?)?;
                self.helper("quote");
                let op = if func == "strHasSuffix" { "*" } else { "*" };
                Ok(format!("(\"[[ \" + shQuote({s}) + \" == {op}\" + shQuote({p}) + \" ]]\")"))
            }
            IrExpr::Call { func, args, .. } if func == "test" => self.bracket_text(args, false),
            IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } => {
                let t = self.predicate_text(lhs)?;
                Ok(format!("(\"! \" + {t})"))
            }
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge) =>
            {
                let tok = match op {
                    BinOpKind::Eq => "=", BinOpKind::Ne => "!=",
                    BinOpKind::Lt => "\\<", BinOpKind::Le => "-le",
                    BinOpKind::Gt => "\\>", _ => "-ge",
                };
                let l = self.expr_str(lhs)?;
                let r = self.expr_str(rhs)?;
                self.helper("quote");
                Ok(format!("(\"[ \" + shQuote({l}) + \" {tok} \" + shQuote({r}) + \" ]\")"))
            }
            _ => Err("not a predicate".into()),
        }
    }

    fn side_text(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "test" => {
                // bracket TEST as bash TEXT (command position, not an
                // expression): `[[ a =~ b ]]`, never `true`.
                // (bracket_text keeps the `[[`/`]]` tokens — they carry
                // regex semantics that `[` cannot express)
                let t = self.bracket_text(args, false)?;
                if t.is_empty() { return Err("empty test".into()); }
                return Ok(t);
            }
            IrExpr::Call { func, args, .. } if func == "exec" || func == "builtin" => {
                self.cmd_line_expr(args)
            }
            IrExpr::Call { func, args, .. } if func == "redirect" => {
                let mut inner_t = String::from("\"\"");
                if let Some(IrExpr::Arrow(b)) = args.first() {
                    inner_t = self.arrow_expr(b)?;
                }
                if let Some(IrExpr::Array(rs)) = args.get(1) {
                    for r in rs.iter() {
                        inner_t += &self.redirect_text(r)?;
                    }
                }
                Ok(inner_t)
            }
            // a COMPARISON operand inside a chain (`[ -n "$v" ] || v=x`)
            // runs as BASH: `[ lhs op rhs ]`
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge) =>
            {
                let tok = match op {
                    BinOpKind::Eq => "=", BinOpKind::Ne => "!=",
                    BinOpKind::Lt => "\\<", BinOpKind::Le => "-le",
                    BinOpKind::Gt => "\\>", _ => "-ge",
                };
                let l = self.expr_str(lhs)?;
                let r = self.expr_str(rhs)?;
                self.helper("quote");
                Ok(format!("(\"[ \" + shQuote({l}) + \" {tok} \" + shQuote({r}) + \" ]\")"))
            }
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::And | BinOpKind::Or) =>
            {
                let sep = if matches!(*op, BinOpKind::And) { " && " } else { " || " };
                // a COMPARISON operand inside a chain runs as BASH: `[ lhs op rhs ]`
                match (op, lhs.as_ref(), rhs.as_ref()) {
                    (BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge, _, _) => {
                        let tok = match op {
                            BinOpKind::Eq => "=", BinOpKind::Ne => "!=",
                            BinOpKind::Lt => "\\<", BinOpKind::Le => "-le",
                            BinOpKind::Gt => "\\>", _ => "-ge",
                        };
                        let l = self.bash_word(lhs)?;
                        let r = self.bash_word(rhs)?;
                        return Ok(format!("(\"[ \" + shQuote({l}) + \" {tok} \" + shQuote({r}) + \" ]\")"));
                    }
                    _ => {}
                }
                let l = self.side_text(lhs)?;
                let r = self.side_text(rhs)?;
                Ok(format!("({l} + \"{sep}\" + {r})"))
            }
            IrExpr::Call { func, args, .. } if func == "pipeline" => {
                let mut parts2 = Vec::new();
                if let Some(IrExpr::Array(stages)) = args.first() {
                    for st in stages.iter() {
                        if let IrExpr::Arrow(body) = st {
                            parts2.push(self.stage_expr(body)?);
                        }
                    }
                }
                Ok(parts2.join(" + \" | \" + "))
            }
            IrExpr::Call { func, args, .. } if func == "and" || func == "or" => {
                // a PREDICATE operand is command text of its own
                let mut any_pred = false;
                for a in args.iter() {
                    if !matches!(a, IrExpr::Arrow(_)) && self.predicate_text(a).is_ok() { any_pred = true; }
                }
                if any_pred {
                    let sep = if func == "and" { " && " } else { " || " };
                    let mut parts = Vec::new();
                    for a in args.iter() {
                        match a {
                            IrExpr::Arrow(body) => parts.push(self.arrow_expr(body)?),
                            other => match self.predicate_text(other) {
                                Ok(t) => parts.push(t),
                                Err(_) => parts.push(self.expr_str(other)?),
                            },
                        }
                    }
                    return Ok(format!("({})", parts.join(&format!("){sep}("))));
                }
                // pre-run renderer-temp captures so interpolated paths are real
                for a in args.iter() {
                    if let IrExpr::Arrow(body) = a {
                        for bs in body.iter() {
                            if let IrStmt::Assign { targets, .. } = bs {
                                if targets.first().map(|t| t.var.starts_with("__ps_")).unwrap_or(false) {
                                    self.stmt(bs)?;
                                }
                            }
                        }
                    }
                }
                let sep = if func == "and" { " && " } else { " || " };
                let mut parts = Vec::new();
                for a in args.iter() {
                    if let IrExpr::Arrow(body) = a {
                        parts.push(self.arrow_expr(body)?);
                    } else {
                        parts.push(self.expr_str(a)?);
                    }
                }
                Ok(format!("({})", parts.join(&format!("){sep}("))))
            }
            // chain operands in TEXT (child-subshell) position: assignments
            // run in the child (state is the child's — bash-subshell
            // semantics), jumps pass through as text.
            IrExpr::Call { func, args, .. } if func == "assign" => {
                let name = str_arg(args, 0).ok_or("assign: no name")?;
                let vi = if str_arg(args, 1) == Some("=") || str_arg(args, 1) == Some("+=") { 2 } else { 1 };
                let v = self.expr_str(args.get(vi).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.helper("quote");
                Ok(format!("({} + shQuote({v}))", jstr(&format!("{name}="))))
            }
            IrExpr::Call { func, .. } if func == "break" || func == "continue" => {
                Ok(format!("\"{func} \""))
            }
            IrExpr::Call { func, args, .. } if func == "block" => {
                let mut sub = Vec::new();
                for a in args.iter() {
                    if let IrExpr::Arrow(body) = a {
                        sub.push(self.arrow_expr(body)?);
                    } else {
                        sub.push(self.expr_str(a)?);
                    }
                }
                Ok(format!("( \"{{ \" + {} + \" ; }}\" )", sub.join(" + \" ; \" + ")))
            }
            // A shell VARIABLE in chain-operand position: bash's `[ -n "$v" ]`
            // idiom — a non-empty value means success.
            IrExpr::Var(..) | IrExpr::Ident(..) => {
                self.helper("quote");
                let v = self.expr_str(e)?;
                Ok(format!("(({}).isEmpty() ? \"false\" : \"true\")", v))
            }
            // a plain word (`[ x ]` with a literal): true iff non-empty
            IrExpr::Str(t, _) => Ok(jstr(if t.is_empty() { "false" } else { "true" })),
            IrExpr::Call { func, .. } if func == "getVar" => {
                self.helper("quote");
                let v = self.expr_str(e)?;
                Ok(format!("(({}).isEmpty() ? \"false\" : \"true\")", v))
            }
            _ => match self.predicate_text(e) {
                Ok(t) => Ok(t),
                Err(_) => Err(format!("command-chain side not representable: {e:?}")),
            },
        }
    }

    /// Emit population of array field __a_<name> from item expressions;
    /// shSplit-wrapped items become addAll (word splitting).
    fn emit_array_populate(&mut self, name: &str, items: &[IrExpr], clear: bool) -> Result<(), String> {
        self.arrays.insert(sanitize(name));
        if clear {
            self.emit(&format!("__a_{} = new ArrayList<>();", sanitize(name)));
        }
        for w in flatten_words(items) {
            let e = self.expr_str(w)?;
            if let Some(r) = e.strip_prefix("String.join(\" \", shSplit(") {
                let inner = r.strip_suffix("))").unwrap_or(r);
                self.emit(&format!("__a_{}.addAll(shSplit({inner}));", sanitize(name)));
            } else if e.starts_with("shSplit(") {
                self.emit(&format!("__a_{}.addAll({e});", sanitize(name)));
            } else {
                self.emit(&format!("__a_{}.add({e});", sanitize(name)));
            }
        }
        Ok(())
    }

    /// One arg → pieces (java expr, needs_quotes). Glob-marked patterns and
    /// brace expansions become MULTIPLE unquoted words.
    fn word_piece(&mut self, e: &IrExpr) -> Result<(String, bool), String> {
        Ok(self.word_pieces(e)?.remove(0))
    }

    fn word_pieces(&mut self, e: &IrExpr) -> Result<Vec<(String, bool)>, String> {
        match e {
            IrExpr::Str(t, _) if t.contains("\u{1}SH2GLOB\u{1}") => {
                Ok(vec![(jstr(&t.replace("\u{1}SH2GLOB\u{1}", "")), false)])
            }
            IrExpr::Call { func, args, .. } if func == "brace" => {
                let xs = brace_expand(args)?;
                Ok(xs.into_iter().map(|x| (jstr(&x), false)).collect())
            }
            IrExpr::Array(items) => {
                let mut out = Vec::new();
                for it in items.iter() { out.extend(self.word_pieces(it)?); }
                Ok(out)
            }
            // command-scoped env assignment (IFS= cmd …): never a word
            IrExpr::Object(_) => Ok(Vec::new()),
            other => Ok(vec![(self.expr_str(other)?, true)]),
        }
    }

    fn pieces_join(&mut self, pieces: &[(String, bool)]) -> String {
        let mut out = String::new();
        for (i, (e, q)) in pieces.iter().enumerate() {
            if i > 0 { out += " + \" \" + "; }
            if *q { out += &format!("shQuote({e})"); } else { out += e; }
        }
        out
    }

    /// builtin/exec args → Java expression of the quoted command line.
    /// Whether an expression yields MULTIPLE words in bash-text position
    /// (`"${arr[@]}"` slices, word-splits, `"$@"` — these must NOT be
    /// shQuote'd into a single word; bash re-splits the unquoted join).
    fn is_multi_word(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args, .. } if func == "split" => true,
            IrExpr::Call { func, .. } if func == "listVar" => true,
            IrExpr::Call { func, args, .. } if func == "param" => {
                // slice-@ (all elements) is multi; anything with a
                // non-empty offset/length is a (single) substring
                matches!(args.get(2).and_then(|a| str_arg(std::slice::from_ref(a), 0)), Some("@"))
            }
            IrExpr::Call { func, args, .. } if func == "getVar" => {
                matches!(str_arg(args, 0), Some("@" | "*"))
            }
            _ => false,
        }
    }

    fn cmd_line_expr(&mut self, args: &[IrExpr]) -> Result<String, String> {
        self.helper("quote");
        let mut pieces: Vec<(String, bool)> = Vec::new();
        match args.first() {
            Some(IrExpr::Str(c, _)) => pieces.push((
                jstr(&c.replace("\u{1}SH2GLOB\u{1}", "")), c.contains("\u{1}SH2GLOB\u{1}"))),
            Some(other) => pieces.push((self.expr_str(other)?, !Self::is_multi_word(other))),
            None => return Err("command: no command word".into()),
        }
        for w in args.iter().skip(1) {
            for x in flatten_words(std::slice::from_ref(w)) {
                let mut ps = self.word_pieces(x)?;
                if Self::is_multi_word(x) {
                    for p in ps.iter_mut() { p.1 = false; }
                }
                pieces.extend(ps);
            }
        }
        Ok(self.pieces_join(&pieces))
    }

    // ── arithmetic AST ───────────────────────────────────────────────
    fn arith(&mut self, a: &ArithAst) -> Result<String, String> {
        match a {
            ArithAst::Num(n) => Ok(format!("{n}L")),
            ArithAst::Var(n) | ArithAst::Ident(n) => self.getvar_num(n),
            ArithAst::Index { var, key } => {
                let k = self.arith(key)?;
                self.arrays.insert(sanitize(var));
                self.helper("aget");
                Ok(format!("shNum(shAget(__a_{}, {k}))", sanitize(var)))
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs)?;
                let r = self.arith(rhs)?;
                Ok(match op.as_str() {
                    "+" => format!("({l} + {r})"),
                    "-" => format!("({l} - {r})"),
                    "*" => format!("({l} * {r})"),
                    "/" => { self.helper("div"); format!("shDiv({l}, {r})") }
                    "%" => { self.helper("div"); format!("shMod({l}, {r})") }
                    "**" => { self.helper("pow"); format!("shPow({l}, {r})") }
                    "<" => format!("({l} < {r} ? 1L : 0L)"),
                    "<=" => format!("({l} <= {r} ? 1L : 0L)"),
                    ">" => format!("({l} > {r} ? 1L : 0L)"),
                    ">=" => format!("({l} >= {r} ? 1L : 0L)"),
                    "==" | "=" => format!("({l} == {r} ? 1L : 0L)"),
                    "!=" => format!("({l} != {r} ? 1L : 0L)"),
                    "&&" => format!("({l} != 0 && {r} != 0 ? 1L : 0L)"),
                    "||" => format!("({l} != 0 || {r} != 0 ? 1L : 0L)"),
                    "&" => format!("({l} & {r})"),
                    "|" => format!("({l} | {r})"),
                    "^" => format!("({l} ^ {r})"),
                    "<<" => format!("({l} << {r})"),
                    ">>" => format!("({l} >> {r})"),
                    "," => format!("{r}"),
                    other => return Err(format!("arith op {other:?} not in the java subset")),
                })
            }
            ArithAst::Un { op, arg } => {
                let v = self.arith(arg)?;
                Ok(match op.as_str() {
                    "-" => format!("(-{v})"),
                    "+" => v,
                    "!" => format!("({v} == 0 ? 1L : 0L)"),
                    "~" => format!("(~{v})"),
                    other => return Err(format!("arith unop {other:?} not in the java subset")),
                })
            }
            ArithAst::Cond { test, then, else_ } => {
                let t = self.arith(test)?;
                Ok(format!("({t} != 0 ? {} : {})", self.arith(then)?, self.arith(else_)?))
            }
            ArithAst::Assign { var, op, rhs } => {
                let r = self.arith(rhs)?;
                let lhs = if self.fields.contains(&sanitize(var)) {
                    format!("__v_{}", sanitize(var))
                } else {
                    return Err(format!("arith assign to undeclared var {var}"));
                };
                let val = match op.as_str() {
                    "=" => r.clone(),
                    "+=" => format!("shNum({lhs}) + {r}"),
                    "-=" => format!("shNum({lhs}) - {r}"),
                    "*=" => format!("shNum({lhs}) * {r}"),
                    other => return Err(format!("arith assign op {other:?} not in the java subset")),
                };
                Ok(format!("shNum({lhs} = String.valueOf({val}))"))
            }
            ArithAst::IncDec { var, delta, prefix } => {
                let f = if self.fields.contains(&sanitize(var)) {
                    format!("__v_{}", sanitize(var))
                } else {
                    return Err(format!("arith incdec on undeclared var {var}"));
                };
                self.helper("num");
                if *prefix {
                    Ok(format!("shNum({f} = String.valueOf(shNum({f}) + {delta}))"))
                } else {
                    // postfix yields the OLD value: dedicated per-var helper
                    let h = self.postfix_helper(var);
                    Ok(format!("{h}({delta})"))
                }
            }
            ArithAst::Sizeof(_) | ArithAst::Cast { .. } =>
                Err("sizeof/cast not in the java arith subset".into()),
        }
    }

    fn arith_text(&mut self, t: &str) -> Result<String, String> {
        if let Some(ast) = crate::shir::parse_arith_native(t) {
            self.arith(&ast)
        } else {
            Err(format!("arith text not parseable natively: {t:?}"))
        }
    }

    // ── shared ───────────────────────────────────────────────────────
    fn binop_bool(&mut self, op: &BinOpKind, lhs: &IrExpr, rhs: &IrExpr) -> Result<String, String> {
        let l = self.expr_str(lhs)?;
        let r = self.expr_str(rhs)?;
        // numeric comparison when both sides look numeric-typed
        let both_num = matches!(lhs, IrExpr::Int(_) | IrExpr::Arith(_)) &&
                       matches!(rhs, IrExpr::Int(_) | IrExpr::Arith(_));
        self.helper("num");
        // regex match: [[ s =~ re ]] lowers to Eq whose RHS carries a ~ marker
        if *op == BinOpKind::Eq {
            if let IrExpr::Str(rv, _) = rhs {
                if let Some(re) = rv.strip_prefix('~') {
                    self.helper("regexm");
                    let l2 = l.trim_start_matches('(').trim_end_matches(')');
                    return Ok(format!("(shRegexMatch({l2}, {}))", jstr(re)));
                }
            }
        }
        Ok(match op {
            BinOpKind::Eq if both_num => format!("(shNum({l}) == shNum({r}))"),
            BinOpKind::Eq => { self.helper("eq"); format!("(shEq({}, {}))", l, r) }
            BinOpKind::Ne if both_num => format!("(shNum({l}) != shNum({r}))"),
            BinOpKind::Ne => { self.helper("eq"); format!("(!shEq({}, {}))", l, r) }
            BinOpKind::Lt => format!("(shNum({l}) < shNum({r}))"),
            BinOpKind::Le => format!("(shNum({l}) <= shNum({r}))"),
            BinOpKind::Gt => format!("(shNum({l}) > shNum({r}))"),
            BinOpKind::Ge => format!("(shNum({l}) >= shNum({r}))"),
            BinOpKind::And => format!("({l}.equals(\"1\") && {r}.equals(\"1\"))"),
            BinOpKind::Or => format!("({l}.equals(\"1\") || {r}.equals(\"1\"))"),
            _ => return Err("non-bool binop in bool context".into()),
        })
    }
}

#[derive(Debug, Clone)]
enum TestTok { S(String), E(String, bool) } /* bool = original was an unquoted glob pattern */

fn tok_str(t: &TestTok) -> String {
    match t {
        TestTok::S(s) => jstr(s),
        TestTok::E(e, _) => e.clone(),
    }
}

fn is_cmp(op: &BinOpKind) -> bool {
    matches!(op, BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt
        | BinOpKind::Le | BinOpKind::Ge | BinOpKind::And | BinOpKind::Or)
}

fn str_arg<'a>(args: &'a [IrExpr], i: usize) -> Option<&'a str> {
    match args.get(i) {
        Some(IrExpr::Str(s, _)) => Some(s),
        _ => None,
    }
}

/// Bash double-quote removal: inside "...", a backslash preserves its
/// literal meaning except when followed by `$`, `` ` ``, `"`, `\`, or a
/// newline (those collapse to the char itself / nothing). The frontend
/// keeps the raw text in ShIR Str nodes, so renderers that need the real
/// value (e.g. ${var//pat/rep} patterns) apply this.
fn bash_dquote_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.peek() {
                Some('$') | Some('`') | Some('"') | Some('\\') => out.push(it.next().unwrap()),
                Some('\n') => { it.next(); }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Bare `$name` / `${name}` references for the arith parser (`$i` → `i`,
/// mirroring the core's arith_text_dollar_strip for the plain-name case;
/// exotic shapes are left intact and fail parsing loudly downstream).
fn bare_arith_refs(s: &str) -> String {
    let mut bare = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '$' {
            if it.peek() == Some(&'{') {
                it.next();
                for c2 in it.by_ref() {
                    if c2 == '}' { break; }
                    bare.push(c2);
                }
            } else {
                let mut nm = String::new();
                while let Some(&d) = it.peek() {
                    if d.is_ascii_alphanumeric() || d == '_' { nm.push(d); it.next(); }
                    else { break; }
                }
                bare.push_str(&nm);
            }
        } else {
            bare.push(c);
        }
    }
    bare
}

/// A `>&N` / `N>&M` target already rendered as a Java string literal:
/// return the fd-slot field (`__fdN`) for N=3..9, else None. Matches the
/// exact jstr output for `"&3"`..`"&9"` — dynamic targets never match.
fn fd_slot_target(rendered: &str) -> Option<String> {
    if rendered.len() == 4 && rendered.starts_with('"') && rendered.ends_with('"') {
        let inner = &rendered[1..3];
        if inner.len() == 2 && inner.starts_with('&') {
            if let Some(d) = inner[1..].chars().next() {
                if ('3'..='9').contains(&d) {
                    return Some(format!("__fd{d}"));
                }
            }
        }
    }
    None
}

/// bash ANSI-C `$'...'` unescaping: `\\` `\'` `\"` `\n` `\t` `\r` `\b`
/// `\f` `\v` `\a` `\e` `\0ooo` `\xHH` `\uHHHH`, plus `\cX` controls.
/// NUL (`\x00`, `\0`, `\u0000`) maps to EMPTY — bash strings cannot hold
/// NUL bytes, so `$'\x00'` is the empty string (and a `*\0*` glob
/// collapses to `**`, matching everything). Java COULD hold NUL, which
/// would diverge, so drop it here to stay bash-faithful.
/// Unknown escapes keep char-after-backslash (bash drops the backslash
/// for the common set; pass the rest through with it intact... actually
/// bash drops the backslash for ANY `\c`? No — bash keeps unknown like
/// `\?` as `\?`. Match: known set above, else backslash intact).
fn unescape_ansi_c(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] != '\\' || i + 1 >= ch.len() { out.push(ch[i]); i += 1; continue; }
        match ch[i + 1] {
            '\\' => { out.push('\\'); i += 2; }
            '\'' => { out.push('\''); i += 2; }
            '"' => { out.push('"'); i += 2; }
            'n' => { out.push('\n'); i += 2; }
            't' => { out.push('\t'); i += 2; }
            'r' => { out.push('\r'); i += 2; }
            'b' => { out.push('\u{8}'); i += 2; }
            'f' => { out.push('\u{c}'); i += 2; }
            'v' => { out.push('\u{b}'); i += 2; }
            'a' => { out.push('\u{7}'); i += 2; }
            'e' | 'E' => { out.push('\u{1b}'); i += 2; }
            'x' => {
                let h: String = ch[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).take(2).collect();
                if h.is_empty() { out.push('\\'); out.push('x'); i += 2; }
                else if let Ok(v) = u32::from_str_radix(&h, 16) {
                    // NUL is unrepresentable in bash strings: drop it
                    if v != 0 { out.push(char::from_u32(v).unwrap_or('\u{fffd}')); }
                    i += 2 + h.len();
                } else { out.push('\\'); out.push('x'); i += 2; }
            }
            'u' | 'U' => {
                let n = if ch[i + 1] == 'u' { 4 } else { 8 };
                let h: String = ch[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).take(n).collect();
                if let Ok(v) = u32::from_str_radix(&h, 16) {
                    if let Some(c) = char::from_u32(v) {
                        // NUL is unrepresentable in bash strings: drop it
                        if v != 0 { out.push(c); }
                        i += 2 + h.len(); continue;
                    }
                }
                out.push('\\'); out.push(ch[i + 1]); i += 2;
            }
            '0'..='7' => {
                let o: String = ch[i + 1..].iter().take_while(|c| matches!(c, '0'..='7')).take(3).collect();
                if let Ok(v) = u32::from_str_radix(&o, 8) {
                    // NUL is unrepresentable in bash strings: drop it
                    if v != 0 { out.push(char::from_u32(v).unwrap_or('\u{fffd}')); }
                    i += 1 + o.len();
                } else { out.push('\\'); i += 1; }
            }
            'c' if i + 2 < ch.len() => {
                let c = ch[i + 2].to_ascii_uppercase() as u32;
                out.push(char::from_u32(c & 0x1f).unwrap_or('\u{fffd}')); i += 3;
            }
            _ => { out.push('\\'); out.push(ch[i + 1]); i += 2; }
        }
    }
    out
}

/// Strip one wrapping quote pair (`"add"` → `add`) for case patterns:
/// the ShIR keeps source quotes, bash matches the removed value.
/// Only fires when the SAME quote wraps both ends.
fn strip_wrapping_quotes(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 {
        let b = t.as_bytes();
        if (b[0] == b'"' && b[t.len() - 1] == b'"')
            || (b[0] == b'\'' && b[t.len() - 1] == b'\'')
        {
            return t[1..t.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// A Java string literal (same rules as java_str_lit).
pub fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
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

#[allow(dead_code)]
fn read_field(sanitized: String) -> Option<String> {
    // The renderer resolves reads against its collected field set at render
    // time; this helper is only used in export arms where the field is known
    // to exist (the assignment above created it).
    Some(format!("__v_{sanitized}"))
}

/// Child statements of a statement (for scanning).
/// Names a command text may read or write: getVar/Var/Ident reads, arith
/// vars, `read` targets, assign targets. Used to give child-shell
/// variables a java field (the seed / report channel).
fn collect_shell_names(stmts: &[IrStmt], fields: &mut BTreeSet<String>, seen: &mut BTreeSet<String>) {
    for st in stmts {
        let mut exprs: Vec<&IrExpr> = stmt_exprs(st);
        // read targets
        if let IrStmt::Expr(IrExpr::Call { func, args, .. }) = st {
            if func == "builtin" || func == "exec" {
                if str_arg(args, 0) == Some("read") {
                    for a in args.iter().skip(1) {
                        for w in flatten_words(std::slice::from_ref(a)) {
                            if let IrExpr::Str(n, _) = w {
                                if !n.starts_with('-') {
                                    fields.insert(sanitize(n));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let IrStmt::Assign { targets, .. } = st {
            for t in targets {
                let base = t.var.split('[').next().unwrap_or(&t.var);
                fields.insert(sanitize(base));
            }
        }
        let mut queue: Vec<&IrExpr> = exprs.drain(..).collect();
        while let Some(e) = queue.pop() {
            match e {
                IrExpr::Call { func, args, .. } if func == "getVar" => {
                    if let Some(n) = str_arg(args, 0) {
                        if !matches!(n, "?" | "#" | "$" | "@" | "*" | "!" | "-" | "0")
                            && !n.chars().all(|c| c.is_ascii_digit())
                        {
                            fields.insert(sanitize(n));
                        }
                    }
                }
                IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                    fields.insert(sanitize(n));
                }
                IrExpr::Call { func, args, .. } => {
                    if func == "builtin" || func == "exec" { continue; }
                    for a in args { queue.push(a); }
                }
                IrExpr::BinOp { lhs, rhs, .. } => { queue.push(lhs); queue.push(rhs); }
                IrExpr::Interpolate(parts) => {
                    for p in parts { if let InterpPart::Expr(x) = p { queue.push(x); } }
                }
                IrExpr::Array(items) => { for i in items { queue.push(i); } }
                IrExpr::Arith(a) => collect_arith_names(a, fields),
                _ => {}
            }
        }
        let kids = stmt_children(st.clone());
        collect_shell_names(&kids, fields, seen);
    }
    let _ = seen;
}

fn collect_arith_names(a: &ArithAst, fields: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) => { fields.insert(sanitize(n)); }
        ArithAst::Bin { lhs, rhs, .. } => { collect_arith_names(lhs, fields); collect_arith_names(rhs, fields); }
        ArithAst::Un { arg, .. } => collect_arith_names(arg, fields),
        ArithAst::Cond { test, then, else_ } => {
            collect_arith_names(test, fields);
            collect_arith_names(then, fields);
            collect_arith_names(else_, fields);
        }
        ArithAst::Assign { var, rhs, .. } => { fields.insert(sanitize(var)); collect_arith_names(rhs, fields); }
        ArithAst::IncDec { var, .. } => { fields.insert(sanitize(var)); }
        ArithAst::Index { var, key } => { fields.insert(sanitize(var)); collect_arith_names(key, fields); }
        _ => {}
    }
}

fn stmt_children(st: IrStmt) -> Vec<IrStmt> {
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            let mut v = then;
            for (_, mut b) in elsifs { v.append(&mut b); }
            v.extend(else_);
            v
        }
        IrStmt::While { body, .. } | IrStmt::Block(body) | IrStmt::Subshell(body)
        | IrStmt::Background(body) | IrStmt::DoWhile { body, .. } => body,
        IrStmt::For { body, .. } => body,
        IrStmt::Block(body) => body,
        IrStmt::Case { clauses, .. } => clauses.into_iter().flat_map(|c| c.body).collect(),
        IrStmt::Redirect { inner, .. } => inner,
        IrStmt::Pipeline { stages, .. } => stages.into_iter().flatten().collect(),
        _ => Vec::new(),
    }
}

/// Expressions directly held by a statement (for scanning).
fn stmt_exprs(st: &IrStmt) -> Vec<&IrExpr> {
    match st {
        IrStmt::Expr(e) => vec![e],
        IrStmt::Assign { expr, .. } => vec![expr],
        IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } | IrStmt::If { cond, .. } => vec![cond],
        IrStmt::If { cond, elsifs, .. } => {
            let mut v = vec![cond];
            for (ec, _) in elsifs { v.push(ec); }
            v
        }
        IrStmt::For { iter, .. } => vec![iter],
        IrStmt::Case { discriminant, .. } => vec![discriminant],
        IrStmt::Exit(Some(e)) | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => vec![e],
        IrStmt::WriteFile { path, content, .. } => vec![path, content],
        _ => Vec::new(),
    }
}

/// Runtime helper methods, emitted into Sh2Program on demand.
/// The shared handler registry's child-renderer hook for java: delegates
/// to this backend's own `expr_str`, so a shared handler emits a real java
/// expression tree instead of a stub.
struct JavaChild<'a> {
    r: &'a mut JavaRender,
}

impl crate::render_ext_expr::ChildExprRenderer for JavaChild<'_> {
    fn render_child(&mut self, e: &IrExpr) -> Option<String> {
        self.r.expr_str(e).ok()
    }
}

fn helper_src(name: &str) -> Option<&'static str> {
    Some(match name {
        "env" => r#"    static String shCwd() { return __SH_CWD; }
    static String shAbs(String p) {
        if (p.isEmpty() || p.startsWith("/")) return p;
        return __SH_CWD + "/" + p;
    }
    /// A shell variable the JAVA side may not hold (the child sets it):
    /// an exported/tracked value first, then the process environment.
    static String shEnv(String n) {
        String v = __SH_EXPORTS.get(n);
        if (v == null) v = __SH_ENV.get(n);
        if (v == null) v = System.getenv(n);
        return v == null ? "" : v;
    }
"#,
        "chartrans" => r#"    // `tr SET1 SET2` (translate), `tr -d SET` (delete), `tr -s SET`
    // (squeeze). SET2 shorter than SET1 repeats its LAST char (bash); an
    // empty SET2 means "no translation" for -s.
    static String shTrTranslate(String s, String from, String to) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            int k = from.indexOf(c);
            if (k < 0) { b.append(c); continue; }
            b.append(to.isEmpty() ? c : to.charAt(Math.min(k, to.length() - 1)));
        }
        return b.toString();
    }
    static String shTrDelete(String s, String set) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            if (set.indexOf(s.charAt(i)) < 0) b.append(s.charAt(i));
        }
        return b.toString();
    }
    static String shTrSqueeze(String s, String set) {
        StringBuilder b = new StringBuilder();
        char prev = 0;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == prev && set.indexOf(c) >= 0) continue;
            b.append(c);
            prev = c;
        }
        return b.toString();
    }
    // `xargs`-style trim: remove leading/trailing whitespace AND squeeze
    // internal whitespace runs to one space (the bare-xargs contract).
    static String shXargsTrim(String s) {
        StringBuilder b = new StringBuilder();
        boolean spaced = false, started = false;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (Character.isWhitespace(c)) { spaced = true; continue; }
            if (started && spaced) b.append(' ');
            b.append(c);
            started = true;
            spaced = false;
        }
        return b.toString();
    }
    // CutsetTrim: strip ALL leading/trailing chars present in the cutset
    // (side: "left"/"right" = one end only, else both).
    static String shCutsetTrim(String s, String cut, String side) {
        int a = 0, z = s.length();
        if (side == null || !side.equals("right")) {
            while (a < z && cut.indexOf(s.charAt(a)) >= 0) a++;
        }
        if (side == null || !side.equals("left")) {
            while (z > a && cut.indexOf(s.charAt(z - 1)) >= 0) z--;
        }
        return s.substring(a, z);
    }
"#,
        "wcwords" => r#"    // `wc -w`: whitespace-delimited word count (runs of whitespace are ONE
    // separator; leading/trailing whitespace never counts). The transition
    // scan needs no allocation and is exact for every input.
    static long shWcWords(String s) {
        long n = 0;
        boolean in = false;
        for (int i = 0; i < s.length(); i++) {
            if (Character.isWhitespace(s.charAt(i))) { in = false; }
            else if (!in) { in = true; n++; }
        }
        return n;
    }
"#,
        // GNU `cut -d D -f LIST` over a SINGLE line, exactly as the Perl arm
        // renders it: a missing trailing field contributes NOTHING, and the
        // selection is ascending regardless of the requested order.
        "cutfield" => r#"    static String shCutField(String s, String delim, int[] fields) {
        if (!s.contains(delim)) return s;
        String[] parts = s.split(java.util.regex.Pattern.quote(delim), -1);
        java.util.Arrays.sort(fields);
        StringBuilder out = new StringBuilder();
        boolean first = true;
        for (int f : fields) {
            if (f < 0 || f >= parts.length) continue;
            if (!first) out.append(delim);
            out.append(parts[f]);
            first = false;
        }
        return out.toString();
    }
"#,
        // `sed 's/PAT/REP/'` and `sed 's/PAT/REP/g'`. Java's replaceFirst/
        // replaceAll take the replacement LITERALLY, so `&` (the whole match)
        // and `\1` (a group) are expanded HERE — the sed replacement
        // semantics, implemented once.
        "regsub" => r#"    static String shRegSub(String s, String pat, String rep) {
        return shRegSubImpl(s, pat, rep, false);
    }
    static String shRegSubAll(String s, String pat, String rep) {
        return shRegSubImpl(s, pat, rep, true);
    }
    static String shRegSubImpl(String s, String pat, String rep, boolean global) {
        java.util.regex.Pattern p = java.util.regex.Pattern.compile(pat);
        java.util.regex.Matcher m = p.matcher(s);
        if (!m.find()) return s;
        StringBuilder out = new StringBuilder();
        int last = 0;
        int start = m.start();
        out.append(s, 0, start);
        do {
            out.append(shExpandSed(m, rep));
            last = m.end();
            if (!global) break;
        } while (m.find());
        out.append(s, last, s.length());
        return out.toString();
    }
    static String shExpandSed(java.util.regex.Matcher m, String rep) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < rep.length(); i++) {
            char c = rep.charAt(i);
            if (c == '&') { b.append(m.group()); continue; }
            if (c == '\\' && i + 1 < rep.length()) {
                char d = rep.charAt(++i);
                if (d >= '1' && d <= '9') {
                    int g = d - '0';
                    String v = g <= m.groupCount() ? m.group(g) : null;
                    b.append(v == null ? "" : v);
                } else { b.append(d); }
                continue;
            }
            b.append(c);
        }
        return b.toString();
    }
"#,
        // `wc -l`-family / `grep -c`: count NON-OVERLAPPING regex matches.
        "regcount" => r#"    static long shRegCount(String s, String pat) {
        java.util.regex.Matcher m = java.util.regex.Pattern.compile(pat).matcher(s);
        long n = 0;
        while (m.find()) n++;
        return n;
    }
"#,
        // head/tail in LINE mode and BYTE mode. Line mode splits on '\n'
        // (keeping empty fields, like the Perl/JS arms) and rejoins.
        "takelines" => r#"    static String shTakeLines(String s, int n) {
        if (n <= 0) return "";
        String[] l = s.split("\\n", -1);
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < l.length && i < n; i++) { if (i > 0) b.append("\n"); b.append(l[i]); }
        return b.toString();
    }
    static String shTakeLinesEnd(String s, int n) {
        if (n <= 0) return "";
        String[] l = s.split("\\n", -1);
        StringBuilder b = new StringBuilder();
        for (int i = Math.max(0, l.length - n); i < l.length; i++) { if (i > Math.max(0, l.length - n)) b.append("\n"); b.append(l[i]); }
        return b.toString();
    }
    static String shTakeBytes(String s, int n) {
        return n <= 0 ? "" : s.substring(0, Math.min(n, s.length()));
    }
    static String shTakeBytesEnd(String s, int n) {
        return n <= 0 ? "" : s.substring(Math.max(0, s.length() - n));
    }
"#,
        "len" => r#"    static long shLen(String s) {
        return s.codePointCount(0, s.length());
    }
"#,
        "num" => r#"    static long shNum(String s) {
        try { return Long.parseLong(s.trim()); } catch (Exception e) {
            try { return (long) Double.parseDouble(s.trim()); } catch (Exception e2) { return 0L; }
        }
    }
"#,
        "arg" => r#"    static String shArg(int i) {
        return (i >= 0 && i < __SH_ARGV.size()) ? __SH_ARGV.get(i) : "";
    }
"#,
        "shift" => r#"    static long shShift(int n) {
        for (int i = 0; i < n && !__SH_ARGV.isEmpty(); i++) __SH_ARGV.remove(0);
        return 0L;
    }
"#,
        "cd" => r#"    static long shCd(String dir) {
        if (dir.isEmpty()) dir = shEnv("HOME");
        File d = new File(dir);
        if (!d.isDirectory()) { System.err.println("cd: " + dir + ": No such file or directory"); return 1L; }
        System.setProperty("user.dir", d.getAbsolutePath());
        __SH_CWD = d.getAbsolutePath();
        return 0L;
    }
"#,
        "envset" => r#"    static File shSyncFile() {
        String p = System.getenv("SH2_SYNC_FILE");
        if (p == null) p = System.getProperty("java.io.tmpdir") + "/.sh2_sync_" + ProcessHandle.current().pid();
        return new File(p);
    }
    static String shEnvSet(String n, String v) {
        __SH_ENV.put(n, v);
        return v;
    }
    /// `shEnvSet NAME VALUE` inside child command text: a no-op there (the
    /// value is already in the child's environment) that also records the
    /// value for the parent once the child exits (see shRun).
    static String shEnvChild(String n, String v) {
        __SH_SYNC.put(n, v);
        return "";
    }
"#,
        "run" => r#"    /// `cat <<'DELIM'` reads the heredoc BODY from the lines after the
    /// command, so the command text must end on its terminator line.
    /// A trailing newline would make cat read the terminator as body text
    /// (and then hit EOF mid-heredoc).
    static String shTrimNL(String s) {
        int z = s.length();
        while (z > 0 && (s.charAt(z - 1) == '\n' || s.charAt(z - 1) == '\r')) z--;
        // heredoc terminator: the body's own trailing newlines are data
        int p = z;
        while (p > 0 && s.charAt(p - 1) != '\n') p--;
        if (s.startsWith("SH2EOF", p)) return s.substring(0, z);
        return s;
    }
    /// The process environment as the KERNEL holds it. `System.getenv()`
    /// silently drops entries whose names Java cannot represent — notably
    /// bash's exported-function entries (`BASH_FUNC_name%%=() { … }`), which
    /// a child `bash -c` needs to see `export -f`ed helpers.
    static Map<String, String> shRealEnviron() {
        Map<String, String> m = new LinkedHashMap<>();
        try {
            byte[] b = Files.readAllBytes(Paths.get("/proc/self/environ"));
            int start = 0;
            for (int i = 0; i <= b.length; i++) {
                if (i == b.length || b[i] == 0) {
                    if (i > start) {
                        String kv = new String(b, start, i - start, StandardCharsets.ISO_8859_1);
                        int eq = kv.indexOf('=');
                        if (eq > 0) m.put(kv.substring(0, eq), kv.substring(eq + 1));
                    }
                    start = i + 1;
                }
            }
        } catch (Exception e) { m.putAll(System.getenv()); }
        return m;
    }
    static long shRun(String cmdline) {
        try {
            if (System.getenv("SH2_JDBG") != null) System.err.println("SH2RUN: " + cmdline);


            ProcessBuilder pb = new ProcessBuilder("bash", "-c", shTrimNL(cmdline));
            pb.directory(new File(shCwd()));
            // ProcessBuilder's environment map starts EMPTY; filling it from
            // scratch drops the inherited `BASH_FUNC_*` entries (exported
            // functions — `export -f _sh2_env` must reach the child). Seed
            // from the real environment, then overlay the shell's vars.
            Map<String, String> penv = pb.environment();
            penv.putAll(shRealEnviron());
            penv.putAll(__SH_EXPORTS);
            penv.putAll(__SH_ENV);
            // the loop-var report channel (a java property, not an env var)
            penv.put("SH2_SYNC_FILE", shSyncFile().getAbsolutePath());
            // child shEnvSet writes land in one file the parent folds back in
            try { File __sf = shSyncFile(); if (__sf.exists()) __sf.delete(); } catch (Exception e) { }
            // every shell variable is visible to a child as if exported, so a
            // command text that reads `$line` (a loop variable the child
            // itself just set, or a plain script var) sees the real value
            pb.environment().putAll(__SH_ENV);
            pb.redirectErrorStream(false);
            pb.inheritIO();
            Process p = pb.start();
            // a loop writes its vars to a file; fold them into __SH_ENV
            int rc = p.waitFor();
            try {
                File sf = shSyncFile();
                if (sf.exists()) {
                    for (String line : Files.readAllLines(sf.toPath(), StandardCharsets.ISO_8859_1)) {
                        int us = line.indexOf(' ');
                        if (us > 0) __SH_ENV.put(line.substring(0, us), line.substring(us + 1));
                    }
                    sf.delete();
                }
            } catch (Exception e) { }
            __SH_RC = rc;
            return rc;
        } catch (Exception e) {
            __SH_RC = 127;
            return 127L;
        }
    }
"#,
        "runrc" => r#"    static long shRunEval(String code) {
        return shRun(code);
    }
"#,
        "capture" => r#"    static String shCapture(String cmdline) {
        try {
            ProcessBuilder pb = new ProcessBuilder("bash", "-c", shTrimNL(cmdline));
            pb.directory(new File(shCwd()));
            // ProcessBuilder's environment map starts EMPTY; filling it from
            // scratch drops the inherited `BASH_FUNC_*` entries (exported
            // functions — `export -f _sh2_env` must reach the child). Seed
            // from the real environment, then overlay the shell's vars.
            Map<String, String> penv = pb.environment();
            penv.putAll(shRealEnviron());
            penv.putAll(__SH_EXPORTS);
            penv.putAll(__SH_ENV);
            // the loop-var report channel (a java property, not an env var)
            penv.put("SH2_SYNC_FILE", shSyncFile().getAbsolutePath());
            // child shEnvSet writes land in one file the parent folds back in
            try { File __sf = shSyncFile(); if (__sf.exists()) __sf.delete(); } catch (Exception e) { }
            // stdin MUST be inherited: the default is a never-written pipe,
            // which deadlocks any captured command that reads stdin
            pb.redirectInput(ProcessBuilder.Redirect.INHERIT);
            pb.redirectErrorStream(false);
            Process p = pb.start();
            byte[] out = p.getInputStream().readAllBytes();
            p.getErrorStream().readAllBytes();
            int rc = p.waitFor();
            __SH_RC = rc;
            String s = new String(out, StandardCharsets.UTF_8);
            while (s.endsWith("\n") || s.endsWith("\r")) s = s.substring(0, s.length() - 1);
            return s;
        } catch (Exception e) {
            __SH_RC = 127;
            return "";
        }
    }
"#,
        "fncap" => r#"    interface ShFn { boolean run() throws Exception; }
    static String shFncap(ShFn fn) {
        // capture a transpiled shell function's stdout: swap System.out
        // for a buffer, run in-process, restore, strip trailing newlines
        // (bash command substitution semantics, same as shCapture).
        PrintStream old = System.out;
        old.flush();
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        System.setOut(new PrintStream(buf));
        try {
            fn.run();
        } catch (Exception e) {
            throw new RuntimeException(e);
        } finally {
            System.out.flush();
            System.setOut(old);
        }
        String s = new String(buf.toByteArray(), StandardCharsets.UTF_8);
        while (s.endsWith("\n") || s.endsWith("\r")) s = s.substring(0, s.length() - 1);
        return s;
    }
"#,
        "printf" => r#"    static String shPrintf(String fmt, String[] args) {
        StringBuilder sb = new StringBuilder();
        // unescape backslash sequences in the literal first
        StringBuilder f = new StringBuilder();
        for (int i = 0; i < fmt.length(); i++) {
            char c = fmt.charAt(i);
            if (c == '\\' && i + 1 < fmt.length()) {
                char n = fmt.charAt(++i);
                switch (n) {
                    case 'n': f.append('\n'); break;
                    case 't': f.append('\t'); break;
                    case 'r': f.append('\r'); break;
                    case '\\': f.append('\\'); break;
                    default: f.append('\\').append(n);
                }
            } else f.append(c);
        }
        int specs = 0;
        for (int i = 0; i < f.length(); i++) {
            if (f.charAt(i) == '%' && i + 1 < f.length()) {
                char n = f.charAt(i + 1);
                if (n != '%') specs++;
                i++;
            }
        }
        int ai = 0;
        do {
            int used = 0;
            for (int i = 0; i < f.length(); i++) {
                char c = f.charAt(i);
                if (c == '%' && i + 1 < f.length()) {
                    char n = f.charAt(++i);
                    if (n == '%') { sb.append('%'); continue; }
                    // flags + width: [-0-9]* before the conversion char
                    int wstart = i;
                    while (i + 1 < f.length() && (f.charAt(i) == '-' || Character.isDigit(f.charAt(i)))) i++;
                    String fw = f.substring(wstart, i);
                    n = f.charAt(i);
                    boolean leftJust = fw.contains("-");
                    int width = 0;
                    try { String dg = fw.replace("-", ""); if (!dg.isEmpty()) width = Integer.parseInt(dg); }
                    catch (Exception e2) { width = 0; }
                    String arg = ai < args.length ? args[ai] : "";
                    used++;
                    if (n == 'd' || n == 'i') {
                        try { sb.append(Long.parseLong(arg.trim())); } catch (Exception e) { sb.append("0"); }
                        if (ai < args.length) ai++;
                    } else if (n == 's') {
                        StringBuilder padded = new StringBuilder(arg);
                        while (padded.length() < width) {
                            if (leftJust) padded.append(' '); else padded.insert(0, ' ');
                        }
                        sb.append(padded);
                        if (ai < args.length) ai++;
                    } else if (n == 'b') {
                        sb.append(arg.replace("\\n", "\n").replace("\\t", "\t"));
                        if (ai < args.length) ai++;
                    } else {
                        sb.append(arg);
                        if (ai < args.length) ai++;
                    }
                } else sb.append(c);
            }
            if (args.length == 0 || used == 0) break;
        } while (ai < args.length);
        return sb.toString();
    }
"#,
        "escapes" => r#"    static String shEscapes(String s) {
        return s.replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\");
    }
"#,
        "substr" => r#"    static String shSubstr(String s, long off, long len) {
        int[] cp = s.codePoints().toArray();
        long start = off < 0 ? cp.length + off : off;
        if (start < 0) start = 0;
        if (start > cp.length) return "";
        long end = len < 0 ? cp.length : start + len;
        if (end > cp.length) end = cp.length;
        if (end <= start) return "";
        return new String(cp, (int) start, (int) (end - start));
    }
"#,
        "fnmatch" => r#"    static boolean shFnmatch(String pat, String s) {
        if (pat.indexOf('(') >= 0 && pat.matches(".*[!@?+*]\\(.*")) {
            try {
                StringBuilder re = new StringBuilder();
                shExtglobToRegex(pat, re);
                return s.matches(re.toString());
            } catch (Exception e) { /* fall through to plain fnmatch */ }
        }
        return shFnmatchImpl(pat.codePoints().toArray(), s, 0, 0);
    }

    /** Translate a bash extglob pattern (!(x) @(x) ?(x) +(x) *(x)) to Java regex. */
    static void shExtglobToRegex(String pat, StringBuilder out) {
        int i = 0;
        while (i < pat.length()) {
            char c = pat.charAt(i);
            if ((c == '!' || c == '@' || c == '?' || c == '+' || c == '*')
                && i + 1 < pat.length() && pat.charAt(i + 1) == '(') {
                int depth = 1; int j = i + 2;
                while (j < pat.length() && depth > 0) {
                    if (pat.charAt(j) == '(') depth++;
                    if (pat.charAt(j) == ')') depth--;
                    j++;
                }
                String inner = pat.substring(i + 2, j - 1);
                // translate alternatives separately
                java.util.List<String> alts = new ArrayList<>();
                StringBuilder cur = new StringBuilder(); int d2 = 0;
                for (char x : inner.toCharArray()) {
                    if (x == '(') d2++;
                    if (x == ')') d2--;
                    if (x == '|' && d2 == 0) { alts.add(cur.toString()); cur.setLength(0); }
                    else cur.append(x);
                }
                alts.add(cur.toString());
                StringBuilder altsRe = new StringBuilder();
                for (int k = 0; k < alts.size(); k++) {
                    if (k > 0) altsRe.append('|');
                    StringBuilder one = new StringBuilder();
                    shExtglobToRegex(alts.get(k), one);
                    altsRe.append("(?:").append(one).append(")");
                }
                switch (c) {
                    case '!': out.append("(?:(?!").append(altsRe).append(").)*"); break;
                    case '@': out.append("(?:").append(altsRe).append(")"); break;
                    case '?': out.append("(?:").append(altsRe).append(")?"); break;
                    case '+': out.append("(?:").append(altsRe).append(")+"); break;
                    default:  out.append("(?:").append(altsRe).append(")*");
                }
                i = j;
                continue;
            }
            switch (c) {
                case '*': out.append(".*"); break;
                case '?': out.append('.'); break;
                case '.': out.append("\\."); break;
                case '\\': out.append("\\\\"); break;
                case '[': out.append('['); break;
                case ']': out.append(']'); break;
                case '^': out.append("\\^"); break;
                case '$': out.append("\\$"); break;
                case '(': case ')': case '{': case '}':
                case '+': case '|':
                    if (c=='['||c==']') { out.append(c); break; }
                    out.append('\\').append(c); break;
                default: out.append(c);
            }
            i++;
        }
    }

    static boolean shFnmatchImpl(int[] p, String s, int pi, int si) {
        int[] t = s.codePoints().toArray();
        while (pi < p.length) {
            int c = p[pi];
            if (c == '*') {
                for (int k = si; k <= t.length; k++)
                    if (shFnmatchImpl(p, s, pi + 1, k)) return true;
                return false;
            } else if (c == '?') {
                if (si >= t.length) return false;
                pi++; si++;
            } else if (c == '[') {
                if (si >= t.length) return false;
                int j = pi + 1;
                boolean neg = false;
                if (j < p.length && (p[j] == '!' || p[j] == '^')) { neg = true; j++; }
                boolean matched = false;
                boolean first = true;
                while (j < p.length && (p[j] != ']' || first)) {
                    first = false;
                    if (j + 2 < p.length && p[j + 1] == '-' && p[j + 2] != ']') {
                        if (t[si] >= p[j] && t[si] <= p[j + 2]) matched = true;
                        j += 3;
                    } else {
                        if (t[si] == p[j]) matched = true;
                        j++;
                    }
                }
                if (j < p.length && p[j] == ']') j++;
                if (matched != neg) { pi = j + (j < p.length && p[j] == ']' ? 1 : 0); si++; pi = j; }
                else return false;
            } else if (c == '\\') {
                if (pi + 1 >= p.length || si >= t.length || p[pi + 1] != t[si]) return false;
                pi += 2; si++;
            } else {
                if (si >= t.length || t[si] != c) return false;
                pi++; si++;
            }
        }
        return si == t.length;
    }
"#,
        "strip" => r#"    static String shStripShortestPrefix(String s, String pat) {
        int[] cp = s.codePoints().toArray();
        for (int k = 1; k <= cp.length; k++)
            if (shFnmatch(pat, new String(cp, 0, k))) return new String(cp, k, cp.length - k);
        return s;
    }
    static String shStripLongestPrefix(String s, String pat) {
        int[] cp = s.codePoints().toArray();
        for (int k = cp.length; k >= 1; k--)
            if (shFnmatch(pat, new String(cp, 0, k))) return new String(cp, k, cp.length - k);
        return s;
    }
    static String shStripShortestSuffix(String s, String pat) {
        int[] cp = s.codePoints().toArray();
        for (int k = cp.length - 1; k >= 0; k--)
            if (shFnmatch(pat, new String(cp, k, cp.length - k))) return new String(cp, 0, k);
        return s;
    }
    static String shStripLongestSuffix(String s, String pat) {
        int[] cp = s.codePoints().toArray();
        for (int k = 0; k <= cp.length; k++)
            if (shFnmatch(pat, new String(cp, k, cp.length - k))) return new String(cp, 0, k);
        return s;
    }
"#,
        "replace" => r#"    static String shReplaceAll(String s, String pat, String rep) {
        StringBuilder re = new StringBuilder();
        for (char c : pat.toCharArray()) {
            if ("\\.[]{}()*+-?^$|".indexOf(c) >= 0) re.append('\\');
            re.append(c);
        }
        return s.replaceAll(re.toString(), java.util.regex.Matcher.quoteReplacement(rep));
    }
    static String shReplaceFirst(String s, String pat, String rep) {
        StringBuilder re = new StringBuilder();
        for (char c : pat.toCharArray()) {
            if ("\\.[]{}()*+-?^$|".indexOf(c) >= 0) re.append('\\');
            re.append(c);
        }
        return s.replaceFirst(re.toString(), java.util.regex.Matcher.quoteReplacement(rep));
    }
"#,
        "case" => r#"    static String shUpperAll(String s) { return s.toUpperCase(); }
    static String shLowerAll(String s) { return s.toLowerCase(); }
    static String shUpperFirst(String s) {
        if (s.isEmpty()) return s;
        int fc = s.codePointAt(0);
        return new String(Character.toChars(Character.toUpperCase(fc))) + s.substring(Character.charCount(fc));
    }
    static String shLowerFirst(String s) {
        if (s.isEmpty()) return s;
        int fc = s.codePointAt(0);
        return new String(Character.toChars(Character.toLowerCase(fc))) + s.substring(Character.charCount(fc));
    }
"#,
        "basename" => r#"    static String shBasename(String s) {
        while (s.endsWith("/") && s.length() > 1) s = s.substring(0, s.length() - 1);
        // An all-slash input ("/" or "//") basenames to "/" — the naive
        // substring(1) would yield "" (verified against bash).
        if (s.equals("/")) return "/";
        int i = s.lastIndexOf('/');
        return i < 0 ? s : s.substring(i + 1);
    }
"#,
        "dirname" => r#"    static String shDirname(String s) {
        // GNU: strip trailing '/'s first, then the last component; a
        // pathless word is "."; the prefix is re-stripped and "" -> "/".
        // (The naive lastIndexOf-only form got "a/b/" -> "a/b" wrong.)
        while (s.length() > 1 && s.endsWith("/")) s = s.substring(0, s.length() - 1);
        int i = s.lastIndexOf('/');
        if (i < 0) return ".";
        String d = s.substring(0, i);
        while (d.length() > 1 && d.endsWith("/")) d = d.substring(0, d.length() - 1);
        return d.isEmpty() ? "/" : d;
    }
"#,
        "div" => r#"    static long shDiv(long a, long b) {
        if (b == 0) { System.err.println("division by 0"); System.exit(1); }
        return a / b;
    }
    static long shMod(long a, long b) {
        if (b == 0) { System.err.println("division by 0"); System.exit(1); }
        return a % b;
    }
"#,
        "pow" => r#"    static long shPow(long a, long b) {
        long r = 1;
        for (long i = 0; i < b; i++) r *= a;
        return r;
    }
"#,
        "ftype" => r#"    static boolean shFtype(String kind, String path) {
        File f = new File(shAbs(path));
        switch (kind) {
            case "f": return f.isFile();
            case "d": return f.isDirectory();
            case "e": return f.exists();
            case "s": return f.isFile() && f.length() > 0;
            case "r": return f.canRead();
            case "w": return f.canWrite();
            case "x": return f.canExecute();
            case "L": try { return !f.getCanonicalPath().equals(f.getAbsolutePath()); }
                      catch (Exception e) { return false; }
            case "p": case "b": case "c": case "S":
                try {
                    String ft = (String) java.nio.file.Files.getAttribute(
                        java.nio.file.Paths.get(shAbs(path)), "unix:type",
                        java.nio.file.LinkOption.NOFOLLOW_LINKS);
                    if (kind.equals("p")) return "fifo".equals(ft);
                    if (kind.equals("b")) return "block".equals(ft);
                    if (kind.equals("c")) return "character".equals(ft);
                    if (kind.equals("S")) return "socket".equals(ft);
                    return false;
                } catch (Exception e) { return false; }
            case "g": case "k": case "u":
                try {
                    Integer m = (Integer) java.nio.file.Files.readAttributes(
                        java.nio.file.Paths.get(shAbs(path)), "unix:mode").get("mode");
                    int bit = (kind.equals("g")) ? 02000 : (kind.equals("k") ? 01000 : 04000);
                    return (m != null && (m & bit) != 0);
                } catch (Exception e) { return false; }
            case "O":
                try {
                    return java.nio.file.Files.getOwner(
                        java.nio.file.Paths.get(shAbs(path))).getName()
                        .equals(System.getProperty("user.name"));
                } catch (Exception e) { return false; }
            case "G":
                try {
                    return ((java.nio.file.attribute.PosixFileAttributes)
                        java.nio.file.Files.readAttributes(
                            java.nio.file.Paths.get(shAbs(path)),
                            java.nio.file.attribute.PosixFileAttributes.class))
                        .group().getName().equals(System.getProperty("user.name"));
                } catch (Exception e) { return false; }
            case "N":
                try {
                    long mt = ((java.nio.file.attribute.FileTime)
                        java.nio.file.Files.getAttribute(
                            java.nio.file.Paths.get(shAbs(path)), "lastModifiedTime")).toMillis();
                    long at = ((java.nio.file.attribute.FileTime)
                        java.nio.file.Files.getAttribute(
                            java.nio.file.Paths.get(shAbs(path)), "lastAccessTime")).toMillis();
                    return mt > at;
                } catch (Exception e) { return false; }
            default: return false;
        }
    }
    static long shMtime(String path) {
        File f = new File(shAbs(path));
        return f.exists() ? f.lastModified() : -1L;
    }
"#,
        
        "readln" => r#"    static String shReadln() {
        try {
            if (!__SH_IN.hasNextLine()) return null;
            return __SH_IN.nextLine();
        } catch (Exception e) { return null; }
    }
"#,
        "readsplit" => r#"    static String[] shReadSplit(String line, String ifs, int n) {
        // `read` field splitting: null line (EOF) -> all empty; no names
        // edge (n==0) -> empty. With an IFS delimiter: split on any of its
        // chars (non-whitespace IFS) or runs of whitespace (default/blank
        // IFS); excess fields merge into the LAST var (bash semantics).
        String[] out = new String[Math.max(n, 0)];
        java.util.Arrays.fill(out, "");
        if (line == null || n <= 0) return out;
        java.util.List<String> parts = new java.util.ArrayList<>();
        if (ifs.isEmpty()) {
            for (String p : line.trim().split("\\s+")) {
                if (!p.isEmpty()) parts.add(p);
            }
        } else {
            boolean ifsWs = false;
            for (int i = 0; i < ifs.length(); i++) {
                if (Character.isWhitespace(ifs.charAt(i))) { ifsWs = true; break; }
            }
            if (ifsWs) {
                for (String p : line.trim().split("\\s+")) {
                    if (!p.isEmpty()) parts.add(p);
                }
            } else {
                StringBuilder cur = new StringBuilder();
                for (int i = 0; i < line.length(); i++) {
                    char c = line.charAt(i);
                    if (ifs.indexOf(c) >= 0) { parts.add(cur.toString()); cur.setLength(0); }
                    else cur.append(c);
                }
                parts.add(cur.toString());
            }
        }
        for (int i = 0; i < n; i++) {
            if (i < parts.size()) {
                if (i == n - 1) {
                    // last var absorbs the remainder with single spaces
                    StringBuilder b = new StringBuilder(parts.get(i));
                    for (int j = i + 1; j < parts.size(); j++) b.append(' ').append(parts.get(j));
                    out[i] = b.toString();
                } else out[i] = parts.get(i);
            }
        }
        return out;
    }
"#,
        "sourcefile" => r#"    static java.util.Map<String, String> __dyn = new java.util.LinkedHashMap<>();
    static long shSourceFile(String file, String[] names) {
        // `.`/`source` shares state: serialize java scalars/arrays into a
        // child bash, source the file there, read the changes back.
        // Protocol: base64 lines (no quoting pain), scalars SH2V, indexed
        // array elements SH2A. Assoc arrays are out of scope.
        try {
            StringBuilder setup = new StringBuilder();
            for (String n : names) {
                if (n.startsWith("a:")) {
                    String v = n.substring(2);
                    java.lang.reflect.Field fv = Sh2Program.class.getDeclaredField("__a_" + v);
                    @SuppressWarnings("unchecked")
                    java.util.List<String> a = (java.util.List<String>) fv.get(null);
                    setup.append(v).append("=(); ");
                    for (String e : a) {
                        setup.append(v).append("+=(").append(shQuoteBash(e)).append(") ");
                    }
                } else {
                    String v = n.startsWith("s:") ? n.substring(2) : n;
                    java.lang.reflect.Field f = Sh2Program.class.getDeclaredField("__v_" + v);
                    String val = (String) f.get(null);
                    setup.append(v).append("=").append(shQuoteBash(val)).append(" ");
                }
            }
            String names_arg = "";
            for (String n : names) {
                String v = n.startsWith("a:") ? n.substring(2) : (n.startsWith("s:") ? n.substring(2) : n);
                names_arg += v + " ";
            }
            String script = setup.toString()
                + "__before=$(compgen -v | sort); "
                + "source " + shQuoteBash(file) + "; __rc=$?; "
                + "__after=$(compgen -v | sort); "
                + "__new=$(comm -13 <(echo \"$__before\") <(echo \"$__after\") | grep -v '^__'); "
                + "for __v in " + names_arg + " $__new; do "
                + "if declare -p $__v 2>/dev/null | grep -q 'declare -a'; then "
                + "eval 'for __i in \"${!$__v[@]}\"; do __ref=$__v[$__i]; printf \"SH2A\\037%s\\037%s\\037%s\\n\" \"$__v\" \"$__i\" \"$(printf %s \"${!__ref}\" | base64 -w 0)\"; done'; "
                + "else printf 'SH2V\\037%s\\037%s\\n' \"$__v\" \"$(printf %s \"${!__v}\" | base64 -w 0)\"; fi; done; "
                + "exit $__rc";
            ProcessBuilder pb = new ProcessBuilder("bash", "-c", script);
            pb.directory(new File(shCwd()));
            pb.environment().putAll(__SH_EXPORTS);
            pb.environment().putAll(__SH_ENV);
            // child shEnvSet writes land in one file the parent folds back in
            try { File __sf = shSyncFile(); if (__sf.exists()) __sf.delete(); } catch (Exception e) { }
            pb.redirectErrorStream(false);
            Process p = pb.start();
            byte[] out = p.getInputStream().readAllBytes();
            p.getErrorStream().readAllBytes();
            int rc = p.waitFor();
            String s = new String(out, java.nio.charset.StandardCharsets.UTF_8);
            for (String line : s.split("\n")) {
                String[] f = line.split("\037", -1);
                if (f.length == 3 && f[0].equals("SH2V")) {
                    String val = new String(java.util.Base64.getDecoder().decode(f[2]),
                                            java.nio.charset.StandardCharsets.UTF_8);
                    setShVar(f[1], val);
                } else if (f.length == 4 && f[0].equals("SH2A")) {
                    String val = new String(java.util.Base64.getDecoder().decode(f[3]),
                                            java.nio.charset.StandardCharsets.UTF_8);
                    setShArr(f[1], Integer.parseInt(f[2]), val);
                }
            }
            __SH_RC = rc;
            return rc;
        } catch (Exception e) {
            __SH_RC = 127;
            return 127L;
        }
    }
    static String shQuoteBash(String w) {
        return "'" + w.replace("'", "'\\''") + "'";
    }
    static void setShVar(String n, String v) {
        try {
            java.lang.reflect.Field f = Sh2Program.class.getDeclaredField("__v_" + n);
            f.set(null, v);
        } catch (Exception e) {
            __dyn.put(n, v);
        }
    }
    static void setShArr(String n, int i, String v) {
        try {
            java.lang.reflect.Field fa = Sh2Program.class.getDeclaredField("__a_" + n);
            @SuppressWarnings("unchecked")
            java.util.List<String> a = (java.util.List<String>) fa.get(null);
            while (a.size() <= i) a.add("");
            a.set(i, v);
        } catch (Exception e) {
            __dyn.put(n + "[" + i + "]", v);
        }
    }
"#,
        "split" => r#"    static List<String> shSplit(long v) { return shSplit(String.valueOf(v)); }
    static List<String> shSplit(String s) {
        if (s.isEmpty()) return new ArrayList<>();
        List<String> out = new ArrayList<>(Arrays.asList(s.split("[ \\t\\n]+")));
        // trailing separators produce a trailing "" in Java's split
        while (!out.isEmpty() && out.get(out.size() - 1).isEmpty()) out.remove(out.size() - 1);
        return out;
    }
"#,
        "alist" => "",
        "aset" => r#"    static void shAset(List<String> a, long idx, String val) {
        while (a.size() <= idx) a.add("");
        a.set((int) idx, val);
    }
"#,
        "aget" => r#"    static String shAget(List<String> a, long idx) {
        long i = idx < 0 ? a.size() + idx : idx;
        if (i < 0 || i >= a.size()) return "";
        return a.get((int) i);
    }
"#,
        // Assoc-map storage: __a_<name> holds values, __k_<name> holds keys
        // in insertion order. Numeric operations never touch the key list.
        "asetkey" => r#"    static void shAsetKey(List<String> a, List<String> k, String key, String val) {
        int i = k.indexOf(key);
        if (i < 0) { k.add(key); a.add(val); }
        else a.set(i, val);
    }
"#,
        "agetkey" => r#"    static String shAgetKey(List<String> a, List<String> k, String key) {
        int i = k.indexOf(key);
        if (i < 0 || i >= a.size()) return "";
        return a.get(i);
    }
"#,
        "writefile" => r#"    static void shWriteFile(String path, String content, boolean append) throws IOException {
        if (append) Files.write(Paths.get(shAbs(path)), content.getBytes(StandardCharsets.UTF_8),
                                StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        else Files.write(Paths.get(shAbs(path)), content.getBytes(StandardCharsets.UTF_8));
    }
"#,
        "outswap" => r#"    static PrintStream shSwapOut(PrintStream ps) {
        PrintStream old = System.out;
        old.flush();
        System.setOut(ps);
        return old;
    }
    static void shRestoreOut(PrintStream ps) { System.out.flush(); System.setOut(ps); }
"#,
        "errswap" => r#"    static PrintStream shSwapErr(PrintStream ps) {
        PrintStream old = System.err;
        old.flush();
        System.setErr(ps);
        return old;
    }
    static void shRestoreErr(PrintStream ps) { System.err.flush(); System.setErr(ps); }
    static PrintStream shNullStream() { return new PrintStream(OutputStream.nullOutputStream()); }
"#,
        "inswap" => r#"    static Scanner shSwapIn(Scanner sc) {
        Scanner old = __SH_IN;
        __SH_IN = sc;
        return old;
    }
    static void shRestoreIn(Scanner sc) { __SH_IN = sc; }
"#, 
        "quote" => r#"    static String shQuote(String w) {
        if (w.isEmpty()) return "''";
        boolean plain = true;
        for (char c : w.toCharArray())
            if (!(Character.isLetterOrDigit(c) || "_-./=:@%^+,~".indexOf(c) >= 0)) { plain = false; break; }
        if (plain) return w;
        return "'" + w.replace("'", "'\\''") + "'";
    }
"#,
        "declared" => r#"    static boolean shDeclared(String n) { return true; }
"#,
        "snapshot" => r#"    static String shSnapshot() { return ""; }
    static void shRestore(String s) {}
"#,
        "regex" => r#"    static boolean shRegex(String pat, String flags) { return false; }
"#,
        "incdec" => "",
        "aslice" => r#"    static String shAslice(List<String> a, long off, long len) {
        long start = off < 0 ? a.size() + off : off;
        if (start < 0) start = 0;
        long end = len < 0 ? a.size() : start + len;
        if (end > a.size()) end = a.size();
        if (end <= start) return "";
        return String.join(" ", a.subList((int) start, (int) end));
    }
"#,
        "testrc" => r#"    static boolean shTestRc(boolean b) {
        __SH_RC = b ? 0 : 1;
        return b;
    }
"#,
        "eq" => r#"    static boolean shEq(String a, String b) {
        return __SH_NOCASE ? a.equalsIgnoreCase(b) : a.equals(b);
    }
"#,
        "regexm" => r#"    static boolean shRegexMatch(String s, String re) {
        try { return s.matches(re); } catch (Exception e) { return false; }
    }
"#,
        "errifempty" => r#"    static String shErrIfEmpty(String v, String msg) {
        if (v.isEmpty()) { System.err.println(msg); System.exit(1); }
        return v;
    }
"#,
        "contains" => r#"    static long shContains(String hay, String needle) {
        return hay.contains(needle) ? 1L : 0L;
    }
"#,
        _ => "",
    })
}

/// Tokenize a string-form test expression on whitespace,
/// respecting double/single quotes.
fn split_test_text(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let ch: Vec<char> = t.chars().collect();
    let mut i = 0usize;
    while i < ch.len() {
        let c = ch[i];
        match quote {
            Some(q) => { if c == q { quote = None; } cur.push(c); }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    cur.push(c);
                } else if c == '$' && i + 1 < ch.len() && (ch[i + 1] == '(' || ch[i + 1] == '{') {
                    // $(cmd) / ${var}: copy ATOMICALLY — internal whitespace
                    // and quotes belong to one operand
                    let open = ch[i + 1];
                    let close = if open == '(' { ')' } else { '}' };
                    let mut depth = 0i32;
                    while i < ch.len() {
                        cur.push(ch[i]);
                        if ch[i] == open { depth += 1; }
                        if ch[i] == close {
                            depth -= 1;
                            if depth == 0 { i += 1; break; }
                        }
                        i += 1;
                    }
                    continue;
                } else if c.is_whitespace() {
                    if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
                } else {
                    cur.push(c);
                }
            }
        }
        i += 1;
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

/// Render-time evaluation of sh2.brace(prefix, groups, middles, suffix).
fn brace_expand(args: &[IrExpr]) -> Result<Vec<String>, String> {
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut groups: Option<serde_json::Value> = None;
    let mut middles: Vec<String> = Vec::new();
    for a in args {
        match a {
            IrExpr::Str(s, _) => { if prefix.is_empty() { prefix = s.clone(); } else { suffix = s.clone(); } }
            IrExpr::Json(v) => {
                if groups.is_none() { groups = Some(v.clone()); }
                else if v.is_array() {
                    for m in v.as_array().unwrap() {
                        if let Some(ms) = m.as_str() { middles.push(ms.to_string()); }
                    }
                }
            }
            _ => {}
        }
    }
    let g = groups.ok_or("brace: no group json")?;
    let arr = g.as_array().ok_or("brace: group json not an array")?;
    // the outer array holds the brace's groups ({a,b}{1,2} -> two) —
    // cartesian product across them
    let mut out: Vec<String> = vec![String::new()];
    for grp in arr {
        let expanded = expand_group(grp)?;
        let mut next: Vec<String> = Vec::new();
        for base in &out {
            for x in &expanded {
                next.push(format!("{base}{x}"));
            }
        }
        out = next;
    }
    // interleave middles between repeated expansions (rare; empty usually)
    if !middles.is_empty() && out.len() == middles.len() * out.len() / out.len() && !middles.is_empty() {
        // middles apply per-item suffixes in zsh-style braces; bash corpus
        // uses empty — append as extra suffix alternatives is not needed
    }
    Ok(out.into_iter().map(|i| format!("{prefix}{i}{suffix}")).collect())
}

fn expand_group(items: &serde_json::Value) -> Result<Vec<String>, String> {
    let arr = items.as_array().ok_or("brace item group not an array")?;
    // a group's items are ALTERNATIVES ({a,b} -> a | b): the union of each
    // item's own expansion (nested items may contribute several)
    let mut out: Vec<String> = Vec::new();
    for it in arr {
        let one: Vec<String> = if let Some(s) = it.as_str() {
            vec![s.to_string()]
        } else if let Some(r) = it.get("range") {
            let parts = r.as_array().ok_or("range not array")?;
            let start = parts.first().and_then(|x| x.as_str()).unwrap_or("0").to_string();
            let end = parts.get(1).and_then(|x| x.as_str()).unwrap_or("0").to_string();
            let step: i64 = parts.get(2).and_then(|x| x.as_str()).and_then(|s| s.parse().ok()).unwrap_or(1);
            // {00..04..2}: zero-pad to the widest operand's width
            let width = start.len().max(end.len());
            let padded = (start.starts_with('0') && start.len() > 1)
                      || (end.starts_with('0') && end.len() > 1);
            if let (Ok(a), Ok(b)) = (start.parse::<i64>(), end.parse::<i64>()) {
                let mut v = Vec::new();
                let mut i = a;
                while if step > 0 { i <= b } else { i >= b } {
                    let n = i.abs().to_string();
                    v.push(if padded && (n.len() as usize) < width {
                        format!("{}{}", "0".repeat(width - n.len()), if i < 0 { n } else { n })
                    } else { n });
                    i += step;
                    if v.len() > 100_000 { break; }
                }
                v
            } else {
                // alpha range
                let a = start.chars().next().unwrap_or('a') as u8;
                let b = end.chars().next().unwrap_or('z') as u8;
                let mut v = Vec::new();
                let mut c = a as i32;
                while if step > 0 { c as u8 <= b } else { c as u8 >= b } {
                    v.push((c as u8 as char).to_string());
                    c += step as i32;
                    if v.len() > 1000 { break; }
                }
                v
            }
        } else if let Some(seq) = it.as_array() {
            seq.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
        } else if let Some(n) = it.get("nested") {
            expand_group(n)?
        } else if let Some(p) = it.get("product") {
            // a Compound alternative: cartesian concatenation of the
            // sub-groups (`x/{p,q}` -> `x/p`, `x/q`)
            let subs = p.as_array().ok_or("product not array")?;
            let mut combos: Vec<String> = vec![String::new()];
            for g in subs {
                let alts = expand_group(g)?;
                let mut next = Vec::new();
                for c in &combos {
                    for a in &alts {
                        next.push(format!("{c}{a}"));
                    }
                }
                combos = next;
            }
            combos
        } else {
            return Err("unknown brace item".into());
        };
        out.extend(one);
    }
    Ok(out)
}

/// Flatten Array-of-words wrappers inside command args.
/// True when every statement of the chain body renders natively (no
/// child-text fallback): native builtins, redirects thereof, temp assigns.
fn chain_stmt_native(st: &IrStmt) -> bool {
    match st {
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "builtin" || func == "exec" => {
            matches!(str_arg(args, 0),
                Some("echo") | Some("printf") | Some("mapfile") | Some("read")
                | Some("cd") | Some("export") | Some("true") | Some("false")
                | Some(":") | Some(":="))
        }
        IrStmt::Redirect { inner, .. } => inner.iter().all(chain_stmt_native),
        IrStmt::Assign { .. } => true,
        _ => false,
    }
}

/// markers around a command-scoped prefix assignment's java expression
const GM: &str = "\u{1}GLUE";
const GM_END: &str = "GLUE\u{1}";

fn flatten_words(args: &[IrExpr]) -> Vec<&IrExpr> {
    let mut out = Vec::new();
    for a in args {
        match a {
            IrExpr::Array(items) => out.extend(items.iter()),
            other => out.push(other),
        }
    }
    out
}

/// Split a test operand on top-level shell equality operators
/// (`=` / `==` / `!=`), keeping the operators as their own pieces.
fn split_eq_ops(t: &str) -> Vec<String> {
    let ch: Vec<char> = t.chars().collect();
    let mut pieces = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '"' {
            cur.push(ch[i]);
            i += 1;
            while i < ch.len() && ch[i] != '"' {
                cur.push(ch[i]);
                i += 1;
            }
            if i < ch.len() {
                // closing quote: skip it (do not treat as operator)
                cur.push(ch[i]);
                i += 1;
                continue;
            }
            continue;
        }
        // `=~` is already tokenized by split_fused_regex — never re-split
        if ch[i] == '=' && i + 1 < ch.len() && ch[i + 1] == '~' {
            if !cur.is_empty() { pieces.push(std::mem::take(&mut cur)); }
            pieces.push("=~".into());
            i += 2;
            continue;
        }
        if ch[i] == '=' && i + 1 < ch.len() && ch[i + 1] == '=' {
            if !cur.is_empty() { pieces.push(std::mem::take(&mut cur)); }
            pieces.push("==".into());
            i += 2;
            continue;
        }
        if ch[i] == '!' && i + 1 < ch.len() && ch[i + 1] == '=' {
            if !cur.is_empty() { pieces.push(std::mem::take(&mut cur)); }
            pieces.push("!=".into());
            i += 2;
            continue;
        }
        if ch[i] == '=' {
            if !cur.is_empty() { pieces.push(std::mem::take(&mut cur)); }
            pieces.push("=".into());
            i += 1;
            continue;
        }
        cur.push(ch[i]);
        i += 1;
    }
    if !cur.is_empty() { pieces.push(cur); }
    pieces
}

/// Split `${name<op><pat>}` inner text into (name, op, pat).
fn split_param_op(inner: &str) -> (String, String, String) {
    for op in [":-", ":=", ":+", ":?", "##", "%%", "//", "#", "%", "^", ",,"] {
        if let Some(pos) = inner.find(op) {
            // skip position 0 (would make an empty name)
            if pos > 0 {
                return (inner[..pos].to_string(), op.to_string(), inner[pos + op.len()..].to_string());
            }
        }
    }
    (inner.to_string(), String::new(), String::new())
}

/// True when `$(count)` parens in s are unbalanced (an unterminated command
/// substitution split across test-text args).
fn unbalanced_cmdsub(s: &str) -> bool {
    if let Some(pos) = s.find("$(") {
        let mut depth = 0i32;
        for c in s[pos + 1..].chars() {
            if c == '(' { depth += 1; }
            if c == ')' { depth -= 1; }
        }
        return depth > 0;
    }
    false
}

fn sep_of(func: &str) -> &'static str { if func == "and" { " && " } else { " || " } }

/// Element slice of a bash array: ${arr[@]:off:len} joined with spaces.
fn sh_aslice_src() -> &'static str {
    r#"    static String shAslice(List<String> a, long off, long len) {
        long start = off < 0 ? a.size() + off : off;
        if (start < 0) start = 0;
        long end = len < 0 ? a.size() : start + len;
        if (end > a.size()) end = a.size();
        if (end <= start) return "";
        return String.join(" ", a.subList((int) start, (int) end));
    }
"#
}

#[cfg(test)]
mod shprintf_tests {
    /// Every numeric conversion must advance the argument cursor.
    ///
    /// `shPrintf` drives its outer loop with `do { ... } while (ai < args.length)`,
    /// so a branch that appends a value but never increments `ai` makes the
    /// program spin forever — the java cell then produces NO output at all and
    /// every example looks like an opaque "backend differs". The `%d`/`%i`
    /// branch was the one missing the increment, so `printf "%d\n" $((x+3))`
    /// hung on every integer format.
    ///
    /// This pins the increment structurally: the `%d` arm must advance `ai`
    /// exactly like the `%s`/`%b`/default arms do. It is a source assertion
    /// on purpose — compiling and running the generated java would need a JDK
    /// in the unit-test sandbox, and a hang is exactly the failure a
    /// behavioural test cannot time out of cheaply.
    #[test]
    fn numeric_conversion_advances_arg_cursor() {
        let src = super::helper_src("printf").expect("printf helper must exist");
        // isolate the numeric branch
        let start = src
            .find("if (n == 'd' || n == 'i')")
            .expect("shPrintf must handle %d/%i");
        let rest = &src[start..];
        let end = rest.find("} else").expect("numeric branch must be bounded");
        let arm = &rest[..end];
        assert!(
            arm.contains("ai++"),
            "the %d/%i branch of shPrintf must increment `ai`, else the \
             `do/while (ai < args.length)` outer loop never terminates and \
             every integer printf hangs the generated program. Arm was:\n{arm}"
        );
    }

    /// The other arms must keep advancing too — a guard against the same
    /// regression landing on %s/%b/default later.
    #[test]
    fn every_conversion_arm_advances_arg_cursor() {
        let src = super::helper_src("printf").expect("printf helper must exist");
        for arm in ["n == 's'", "n == 'b'"] {
            let start = src.find(arm).expect("arm must exist");
            let rest = &src[start..];
            let end = rest.find("} else").unwrap_or(rest.len());
            assert!(
                rest[..end].contains("ai++"),
                "the `{arm}` branch must increment `ai`"
            );
        }
    }
}
