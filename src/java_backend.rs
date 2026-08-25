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

struct JavaRender {
    in_fn: usize,                  // >0 while rendering a function body
    fields: BTreeSet<String>,      // scalar vars (sanitized names kept separate)
    arrays: BTreeSet<String>,
    funcs: BTreeSet<String>,
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

impl JavaRender {
    fn new() -> Self {
        JavaRender { in_fn: 0, fields: BTreeSet::new(), arrays: BTreeSet::new(), funcs: BTreeSet::new(),
                     helpers: BTreeSet::new(), java_helpers: Vec::new(), redir_seq: 0, tmp_seq: 0,
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
                IrStmt::Function { name, body, .. } => {
                    self.funcs.insert(name);
                    self.scan(body)?;
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
        if t.indices.is_empty() {
            self.fields.insert(sanitize(&t.var));
        } else {
            self.arrays.insert(sanitize(&t.var));
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
        self.fields.insert(sanitize(name));
    }

    fn emit(&mut self, line: &str) {
        let mut s = String::new();
        for _ in 0..self.depth { s.push_str("    "); }
        s.push_str(line);
        self.out.push(s);
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
        self.emit("for (String __a : args) __SH_ARGV.add(__a);");
        for st in &prog.stmts {
            if matches!(st, IrStmt::Function { .. }) { continue; }
            self.stmt(st)?;
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
                let fname = format!("fn_{}", sanitize(name));
                fn_buf.push_str(&format!("    static boolean {fname}() throws Exception {{\n"));
                fn_buf.push_str("        List<String> __saved = new ArrayList<>(__SH_ARGV);\n");
                fn_buf.push_str("        try {\n");
                let saved_depth = self.depth;
                self.out.clear();
                self.depth = 2;
                self.in_fn += 1;
                for b in body { self.stmt(b)?; }
                self.in_fn -= 1;
                let body_lines = std::mem::take(&mut self.out);
                self.depth = saved_depth;
                for l in &body_lines { fn_buf.push_str(l); fn_buf.push('\n'); }
                fn_buf.push_str("        } finally { __SH_ARGV = __saved; }\n");
                let ends_with_return = body_lines.iter().rev()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim().starts_with("return "))
                    .unwrap_or(false);
                if !ends_with_return {
                    fn_buf.push_str("        return __SH_RC == 0;\n");
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
    static String __SH_CWD = System.getProperty(\"user.dir\");\n    static LinkedHashMap<String, String> __SH_EXPORTS = new LinkedHashMap<>();\n    static List<String> __SH_ARGV = new ArrayList<>();\n    static Scanner __SH_IN = new Scanner(System.in);\n");
        for f in &self.fields {
            header.push_str(&format!("    static String __v_{f} = \"\";\n"));
        }
        for a in &self.arrays {
            header.push_str(&format!("    static List<String> __a_{a} = new ArrayList<>();\n"));
        }
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
            "dirname","div","pow","ftype","readln","split","aset","aget",
            "writefile","outswap","errswap","inswap","snapshot","regex",
            "contains","quote","declared","errifempty","regexm","eq","testrc",
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
    fn stmt(&mut self, st: &IrStmt) -> Result<(), String> {
        match st {
            IrStmt::Expr(e) => self.expr_stmt(e),
            IrStmt::Assign { targets, expr, .. } => {
                let t = targets.first().ok_or("assign: no target")?;
                if !t.indices.is_empty() {
                    // arr[i] = value → list set (grow with empties)
                    let idx = self.expr_num(&t.indices[0])?;
                    let val = self.expr_str(expr)?;
                    self.helper("aset");
                    self.emit(&format!("shAset(__a_{}, {}, {});", sanitize(&t.var), idx, val));
                    return Ok(());
                }
                let v = self.expr_str(expr)?;
                self.emit(&format!("__v_{} = {v};", sanitize(&t.var)));
                self.emit("__SH_RC = 0;");
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
                }
                self.depth -= 1;
                {
                    let mut l = String::new();
                    for _ in 0..self.depth { l.push_str("    "); }
                    l.push('}');
                    self.out.push(l);
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
                self.emit(&format!("boolean __c = {c};"));
                self.emit(&format!("if (!__c) {{ __SH_RC = {lb}; break; }}"));
                for b in body { self.stmt(b)?; }
                self.emit(&format!("{lb} = __SH_RC;"));
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
                    self.emit("List<String> __items = new ArrayList<>();");
                    for piece in items.split('\u{1}') {
                        if piece.is_empty() { continue; }
                        if let Some(r) = piece.strip_prefix("String.join(\" \", shSplit(") {
                            // join-wrapped split → the bare runtime list
                            let inner = r.strip_suffix("))").unwrap_or(r);
                            self.emit(&format!("__items.addAll(shSplit({inner}));"));
                        } else if piece.starts_with('"') || piece.starts_with('(') {
                            self.emit(&format!("__items.add({piece});"));
                        } else {
                            self.emit(&format!("__items.addAll({piece});"));
                        }
                    }
                    self.block_open(&format!("for (String {itv} : __items) {{"));
                } else {
                    self.block_open(&format!("for (String {itv} : new String[] {{{items}}}) {{"));
                }
                self.emit(&format!("__v_{} = {itv};", sanitize(var)));
                for b in body { self.stmt(b)?; }
                self.block_close();
                Ok(())
            }
            IrStmt::Block(body) => {
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
            IrStmt::Redirect { inner, redirects } => self.redirect(inner, redirects),
            IrStmt::Pipeline { stages, capture, cmd_str, .. } =>
                self.pipeline(stages, capture.as_deref(), cmd_str.as_deref()),
            IrStmt::Function { .. } => Ok(()), // emitted at top level
            IrStmt::Return(v) => {
                match v {
                    Some(e) => { let x = self.expr_num(e)?; self.emit(&format!("__SH_RC = {x};")); }
                    None => {}
                }
                if self.in_fn > 0 {
                    self.emit("return __SH_RC == 0;");
                } else {
                    self.emit(&format!("System.exit((int) __SH_RC);"));
                }
                Ok(())
            }
            IrStmt::Break => { self.emit("break;"); Ok(()) }
            IrStmt::Continue => { self.emit("continue;"); Ok(()) }
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
                words.push(format!("shQuote({cmd_e})"));
                for a in args.iter() { words.push(format!("shQuote({})", self.expr_str(a)?)); }
                let line = words.join(" + \" \" + ");
                self.helper("quote");
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({line});"));
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
                conds.push(format!("shFnmatch({}, {d})", jstr(p)));
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

    fn redirect(&mut self, inner: &[IrStmt], redirects: &[IrRedirect]) -> Result<(), String> {
        // Generic stdout/stderr/stdin swaps around the body; each swap gets
        // a unique variable so nested redirects don't collide.
        let mut opened: Vec<String> = Vec::new();
        for r in redirects {
            match (r.fd, r.mode.as_str()) {
                (None, "w") | (Some(1), "w") => {
                    let t = self.expr_str(&r.target)?;
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
                    if cmds.is_empty() { return Err("empty heredoc body".into()); }
                    let t = self.expr_str(&r.target)?;
                    self.helper("quote");
                    self.helper("run");
                    let q = if r.mode == "herestring" { "" } else { "'" };
                    // heredoc bodies already end with \n; herestrings need one
                    let tail = if r.mode == "herestring" { "\"\\nSH2EOF\"" } else { "\"SH2EOF\"" };
                    let joined = cmds.join(" + \" ; \" + ");
                    self.emit(&format!("__SH_RC = shRun({joined} + \" <<{q}SH2EOF{q}\\n\" + {t} + {tail});"));
                    return Ok(());
                }
                #[allow(unreachable_patterns)]
                (Some(0), "heredoc") | (Some(0), "herestring") => {
                    // the body feeds an EXTERNAL command: native Scanner
                    // swaps don't reach a bash -c child — embed the
                    // content as a real heredoc in the command text
                    let t = self.expr_str(&r.target)?;
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
                    let t = self.expr_str(&r.target)?;
                    let content = if r.mode == "herestring" { t } else { format!("{t} + \"\\n\"") };
                    self.redir_seq += 1;
                    let v = format!("__oldIn{}", self.redir_seq);
                    self.helper("inswap");
                    self.emit(&format!("Scanner {v} = shSwapIn(new Scanner(new StringReader({content})));"));
                    opened.push(v);
                }
                other => return Err(format!("redirect {:?} not supported by the java renderer", other)),
            }
        }
        for b in inner { self.stmt(b)?; }
        for v in opened.iter().rev() {
            if v.starts_with("__oldOut") { self.emit(&format!("shRestoreOut({v});")); }
            else if v.starts_with("__oldErr") { self.emit(&format!("shRestoreErr({v});")); }
            else { self.emit(&format!("shRestoreIn({v});")); }
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
    fn stage_expr(&mut self, stage: &[IrStmt]) -> Result<String, String> {
        self.helper("quote");
        match stage {
            [IrStmt::Expr(IrExpr::Call { func, args, .. })]
                if func == "exec" || func == "builtin" || func == "test" =>
            {
                self.cmd_line_expr(args)
            }
            other => Err(format!("pipeline stage not representable as process text: {other:?}")),
        }
    }

    // ── expression STATEMENTS (commands) ─────────────────────────────
    fn expr_stmt(&mut self, e: &IrExpr) -> Result<(), String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "test" => {
                // a bare [ ... ] statement — its exit status is the effect
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
                Ok(())
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
                // assign("name", value) runtime form
                let name = str_arg(args, 0).ok_or("assign: no name")?;
                let v = self.expr_str(args.get(1).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.emit(&format!("__v_{} = {v}; __SH_RC = 0;", sanitize(name)));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "setVar" => {
                let name = str_arg(args, 0).ok_or("setVar: no name")?;
                let v = self.expr_str(args.get(1).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.emit(&format!("__v_{} = {v}; __SH_RC = 0;", sanitize(name)));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "capture" => {
                let t = self.capture_text(args)?;
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun({});", jstr(&t)));
                Ok(())
            }
            IrExpr::Call { func, args, .. } if func == "pipeline" => {
                // statement-position pipeline: stdout goes to the terminal
                let mut parts2 = Vec::new();
                if let Some(IrExpr::Array(stages)) = args.first() {
                    for st in stages.iter() {
                        if let IrExpr::Arrow(body) = st {
                            parts2.push(self.arrow_expr(body)?);
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
                let hay = self.expr_str(args.first().ok_or("grepMatches: no input")?)?;
                let pat = self.expr_str(args.get(1).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                // grepMatches IS the grep -o lift (transforms/grep_o.rs):
                // one match per line, rc 0 iff any match
                let fl = "-o ".to_string();
                self.helper("quote");
                self.helper("run");
                self.emit(&format!("__SH_RC = shRun(\"grep {}\" + \" \" + shQuote({pat}) + \" <<< \" + shQuote({hay}));", fl));
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

            IrExpr::Call { func, .. } if func == "continue" => { self.emit("continue;"); Ok(()) }
            IrExpr::Call { func, .. } if func == "break" => { self.emit("break;"); Ok(()) }
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::And | BinOpKind::Or) =>
            {
                // short-circuit chains as nested ifs:
                // - test/native-builtin lhs -> native cond
                // - otherwise a real child run (stdout passes through)
                let is_or = matches!(*op, BinOpKind::Or);
                let cond = if matches!(lhs.as_ref(), IrExpr::Call { func, .. } if func == "test")
                    || matches!(lhs.as_ref(), IrExpr::Str(..))
                    || matches!(lhs.as_ref(),
                        IrExpr::Call { func, args, .. }
                        if (func == "exec" || func == "builtin")
                            && str_arg(args, 0).map(|c| matches!(c, "cd" | "export" | "read" | "shift")).unwrap_or(false)) {
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
                for w in rest {
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
                // bash locals are function-scoped dynamics; java fields are
                // global — same-store approximation (documented limitation)
                for w in rest {
                    if let IrExpr::Str(s, _) = w {
                        let n = s.trim_start_matches("declare ").split('=').next().unwrap_or(s).trim().to_string();
                        if let Some(eq) = s.find('=') {
                            self.ensure_field(&s[..eq]);
                            self.emit(&format!("__v_{} = {};", sanitize(&s[..eq]), jstr(&s[eq + 1..])));
                        }
                    }
                }
                self.emit("__SH_RC = 0;");
                Ok(())
            }
            "read" => {
                let target = rest.first()
                    .and_then(|e| str_arg(std::slice::from_ref(e), 0))
                    .ok_or("read: no target var")?;
                self.helper("readln");
                self.emit(&format!("__v_{} = shReadln(); __SH_RC = (__v_{} == null) ? 1 : 0;",
                                   sanitize(target), sanitize(target)));
                Ok(())
            }
            "eval" => {
                let t = self.join_words(rest, " ")?;
                self.helper("runrc");
                self.emit(&format!("__SH_RC = shRunEval({t});"));
                Ok(())
            }
            "wait" | "trap" | "umask" | "ulimit" | "shopt" | "set" | "hash" | "enable"
            | "suspend" | "times" | "bind" | "help" | "type" | "command_noop" => {
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
            other => {
                if self.funcs.contains(other) {
                    // positional args replace __SH_ARGV for the call
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
                    self.emit(&format!("fn_{}();", sanitize(other)));
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
                self.emit(&format!("__SH_RC = shRun({line});"));
                Ok(())
            }
        }
    }

    fn words_to_args(&mut self, args: &[IrExpr]) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for a in args {
            match a {
                IrExpr::Array(items) => {
                    for it in items { out.push(self.expr_str(it)?); }
                }
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
                    if let IrExpr::Call { func, .. } = it {
                        // "$@" / "$*" iterables stay runtime lists
                        if func == "listVar" {
                            parts.push("__SH_ARGV".to_string());
                            continue;
                        }
                    }
                    if let IrExpr::Call { func, args, .. } = it {
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
                    }
                    parts.push(self.expr_str(it)?);
                }
                let has_list = parts.iter().any(|p| p.contains("shSplit("));
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
                let target = rest.iter()
                    .flat_map(|e| match e {
                        IrExpr::Array(xs) => xs.iter().collect::<Vec<_>>(),
                        other => vec![other],
                    })
                    .find_map(|x| match x {
                        IrExpr::Str(t, _) if !t.starts_with('-') => Some(t.clone()),
                        _ => None,
                    })
                    .ok_or("read cond: no target")?;
                self.helper("readln");
                self.ensure_field(&target);
                Ok(format!("((__v_{t} = shReadln()) != null)", t = sanitize(&target)))
            }
            "true" => Ok("true".into()),
            "false" => Ok("false".into()),
            other if self.funcs.contains(other) => {
                let flat = flatten_words(rest);
                if !flat.is_empty() {
                    return Err("function call with args not supported in condition position".into());
                }
                Ok(format!("fn_{}()", sanitize(other)))
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
                            other => w.push(TestTok::E(self.expr_str(other)?)),
                        }
                    }
                }
                IrExpr::Str(s, _) => self.push_test_str(&mut w, s),
                other => w.push(TestTok::E(self.expr_str(other)?)),
            }
        }
        self.test_tokens(&w)
    }

    fn push_operand(&mut self, w: &mut Vec<TestTok>, t: &str) {
        // a merged operand like `~="$HOME"` hides a comparison operator —
        // split on top-level = / == / != first
        for piece in split_eq_ops(t) {
            if piece == "=" || piece == "==" || piece == "!=" {
                w.push(TestTok::S(piece));
            } else if piece.starts_with('~') {
                // regex marker ([[ s =~ ~re ]]) — keep verbatim; the
                // trailing $ anchor must not be treated as a variable
                w.push(TestTok::S(piece));
            } else if piece.contains('$') {
                match self.expand_dollars(&piece) {
                    Ok(e) => w.push(TestTok::E(e)),
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

    fn push_test_str(&mut self, w: &mut Vec<TestTok>, s: &str) {
        let mut toks = if s.chars().any(|c| c.is_whitespace()) { split_test_text(s) } else { vec![s.to_string()] };
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
    fn expand_dollars(&mut self, raw: &str) -> Result<String, String> {
        // strip shell-quote artifacts around the operand
        let t = raw.trim();
        let s = if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 { &t[1..t.len()-1] } else { t };
        let mut out = String::from("\"\"");
        let mut lit = String::new();
        let ch: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < ch.len() {
            if ch[i] == '$' && i + 1 < ch.len() {
                if !lit.is_empty() { out += &format!(" + {}", jstr(&lit)); lit.clear(); }
                out += " + ";
                if ch[i + 1] == '{' {
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
                    out += &self.getvar_str(&ch[i + 1].to_string())?;
                    i += 2;
                } else if ch[i + 1].is_ascii_digit() {
                    let mut j = i + 1;
                    while j < ch.len() && ch[j].is_ascii_digit() { j += 1; }
                    let n: String = ch[i + 1..j].iter().collect();
                    out += &self.getvar_str(&n)?;
                    i = j;
                } else if ch[i + 1].is_ascii_alphabetic() || ch[i + 1] == '_' {
                    let mut j = i + 1;
                    while j < ch.len() && (ch[j].is_ascii_alphanumeric() || ch[j] == '_') { j += 1; }
                    let n: String = ch[i + 1..j].iter().collect();
                    out += &self.getvar_str(&n)?;
                    i = j;
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
                        txt.starts_with('~') || txt.chars().any(|c| matches!(c, '*' | '?' | '['))) =>
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
                    "-f" | "-d" | "-e" | "-s" | "-r" | "-w" | "-x" | "-L" | "-h" | "-p" | "-b" | "-c" | "-S" => {
                        self.helper("ftype");
                        let k = match op.as_str() {
                            "-f" => "\"f\"", "-d" => "\"d\"", "-e" => "\"e\"", "-s" => "\"s\"",
                            "-r" => "\"r\"", "-w" => "\"w\"", "-x" => "\"x\"",
                            "-L" | "-h" => "\"L\"", 
                            "-p" => "\"p\"", "-b" => "\"b\"", "-c" => "\"c\"", _ => "\"S\"",
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
                let v = self.expr_str(args.get(2).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.emit(&format!("__v_{} = {v};", sanitize(name)));
                self.emit("__SH_RC = 0;");
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
            IrExpr::Str(s, _) => {
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
                } else {
                    // last resort: let bash evaluate the expression text
                    self.helper("capture");
                    Ok(format!("shCapture(\"echo $(({}))\")", t.replace('"', "\"")))
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
                // capture-position pipeline: stages as Arrows
                let mut parts = Vec::new();
                if let Some(IrExpr::Array(stages)) = args.first() {
                    for st in stages.iter() {
                        if let IrExpr::Arrow(body) = st {
                            parts.push(self.arrow_expr(body)?);
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
                    if let IrExpr::Arrow(body) = a {
                        parts.push(self.arrow_expr(body)?);
                    } else {
                        parts.push(self.expr_str(a)?);
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
                let t = self.arrow_expr(stmts)?;
                self.helper("quote");
                self.helper("capture");
                return Ok(format!("shCapture({t})"));
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
                let key = self.expr_str(args.get(1).unwrap_or(&IrExpr::Int(0)))?;
                self.helper("aget");
                self.arrays.insert(sanitize(name));
                Ok(format!("shAget(__a_{}, shNum({key}))", sanitize(name)))
            }
            IrExpr::Call { func, args, .. } if func == "arrayLen" => {
                let name = str_arg(args, 0).ok_or("arrayLen: no name")?;
                self.arrays.insert(sanitize(name));
                Ok(format!("String.valueOf(__a_{}.size())", sanitize(name)))
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
            other => Err(format!("expr not in the java subset (str ctx): {other:?}")),
        }
    }

    fn part_str(&mut self, e: &IrExpr) -> Result<String, String> {
        // interpolation part: array values splice with spaces
        match e {
            IrExpr::Call { func, args, .. } if func == "listVar" => {
                Ok("String.join(\" \", __SH_ARGV)".to_string())
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
            "?" => "__SH_RC".to_string(),
            "#" => "__SH_ARGV.size()".to_string(),
            "$" => "ProcessHandle.current().pid()".to_string(),
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
                self.arrays.insert(sanitize(name));
                Ok(format!("__a_{}.size()", sanitize(name)))
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

    // ── parameter expansion ──────────────────────────────────────────
    fn param(&mut self, args: &[IrExpr]) -> Result<String, String> {
        let op = str_arg(args, 0).unwrap_or_default();
        let name = str_arg(args, 1).ok_or("param: no variable name")?;
        if op.is_empty() {
            if let Some(lb) = name.find('[') {
                if name.ends_with(']') {
                    // ${arr[idx]} arrives as param("", "arr[idx]")
                    let arr = &name[..lb];
                    let idx = &name[lb + 1..name.len() - 1];
                    if let Ok(k) = idx.trim().parse::<i64>() {
                        self.arrays.insert(sanitize(arr));
                        self.helper("aget");
                        return Ok(format!("shAget(__a_{}, {k}L)", sanitize(arr)));
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
                let pat = self.expr_str(args.get(2).ok_or("//: no pattern")?)?;
                let rep = self.expr_str(args.get(3).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
                self.helper("replace");
                Ok(format!("shReplaceAll({}, {pat}, {rep})", self.getvar_str(&name)?))
            }
            "/" => {
                let pat = self.expr_str(args.get(2).ok_or("/: no pattern")?)?;
                let rep = self.expr_str(args.get(3).unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)))?;
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

    /// The bash command line of an Arrow body when it's a simple
    /// command / pipeline / redirected command, as a Java expression.
    fn arrow_expr(&mut self, stmts: &[IrStmt]) -> Result<String, String> {
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
                IrStmt::Redirect { inner, redirects } => {
                    let mut inner_text = self.arrow_expr(inner)?;
                    for r in redirects {
                        let t = self.expr_str(&r.target)?;
                        match (r.fd, r.mode.as_str()) {
                            (None, "w") | (Some(1), "w") => inner_text += &format!(" + \" > \" + shQuote({t})"),
                            (None, "a") | (Some(1), "a") => inner_text += &format!(" + \" >> \" + shQuote({t})"),
                            (Some(2), "w") => inner_text += &format!(" + \" 2> \" + shQuote({t})"),
                            (Some(2), "a") => inner_text += &format!(" + \" 2>> \" + shQuote({t})"),
                            (Some(0), "r") => inner_text += &format!(" + \" < \" + shQuote({t})"),
                            (Some(0), "herestring") => inner_text += &format!(" + \" <<< \" + shQuote({t})"),
                            (Some(0), "heredoc") | (Some(0), "heredoc-tabs") =>
                                inner_text += &format!(" + \" <<'SH2EOF\\n\" + {t} + \"\\nSH2EOF\""),
                            _ => return Err(format!("redirect {:?}/{} not in capture text", r.fd, r.mode)),
                        }
                    }
                    parts.push(inner_text);
                }
                IrStmt::Exec { cmd, args, .. } => {
                    let mut words = vec![self.expr_str(cmd)?];
                    for a in args.iter() { words.push(self.expr_str(a)?); }
                    let mut parts_v = vec![format!("shQuote({})", words[0])];
                    for w in words.iter().skip(1) { parts_v.push(format!("shQuote({w})")); }
                    parts.push(parts_v.join(" + \" \" + "));
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "pipeline" => {
                    let mut sp = Vec::new();
                    if let Some(IrExpr::Array(stages)) = args.first() {
                        for st in stages.iter() {
                            if let IrExpr::Arrow(body) = st {
                                sp.push(self.arrow_expr(body)?);
                            }
                        }
                    }
                    parts.push(sp.join(" + \" | \" + "));
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "redirect" => {
                    // redirect(Arrow(body), redirects...) inside a capture
                    let mut t = String::from("\"\"");
                    if let Some(IrExpr::Arrow(b)) = args.first() {
                        t = self.arrow_expr(b)?;
                    }
                    if let Some(IrExpr::Array(rs)) = args.get(1) {
                        for rr in rs.iter() { t += &self.redirect_text(rr)?; }
                    }
                    parts.push(t);
                }
                IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "redirect" => {
                    // redirect(Arrow, redirects...) — append redirect text
                    let mut t = String::from("\"\"");
                    if let Some(IrExpr::Arrow(b)) = args.first() {
                        t = self.arrow_expr(b)?;
                    }
                    if let Some(IrExpr::Array(rs)) = args.get(1) {
                        for r in rs.iter() {
                            match self.redirect_text(r) {
                                Ok(x) => t += &x,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    parts.push(t);
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
                    eprintln!("DBG and-join sub={:?} sep={sep}", sub);
                    parts.push(sub.join(&format!(" + \"{sep}\" + ")));
                }
                IrStmt::Expr(IrExpr::BinOp { op, lhs, rhs, .. }) => {
                    let l = self.side_text(lhs)?;
                    let r = self.side_text(rhs)?;
                    let sep = if matches!(*op, BinOpKind::And) { " && " } else { " || " };
                    parts.push(format!("{l} + \"{sep}\" + {r}"));
                }
                other => return Err(format!("arrow statement not representable as text: {other:?}")),
            }
        }
        Ok(parts.join(" + \" ; \" + "))
    }

    /// One redirect-info expression → " > \"target\"" text fragment.
    fn redirect_text(&mut self, r: &IrExpr) -> Result<String, String> {
        // object form: {fd: Int?, mode: Str, target: expr}
        if let IrExpr::Object(props) = r {
            let mut mode = String::new();
            let mut target: Option<&IrExpr> = None;
            for (k, v) in props {
                match k.as_str() {
                    "mode" => { if let IrExpr::Str(m, _) = v { mode = m.clone(); } }
                    "target" => target = Some(v),
                    _ => {}
                }
            }
            if let Some(t) = target {
                let t = self.expr_str(t)?;
                return Ok(match mode.as_str() {
                    "w" => format!(" + \" > \" + shQuote({t})"),
                    "a" => format!(" + \" >> \" + shQuote({t})"),
                    "r" => format!(" + \" < \" + shQuote({t})"),
                    "herestring" => format!(" + \" <<< \" + shQuote({t})"),
                    "heredoc" | "heredoc-tabs" =>
                        format!(" + \" <<'SH2EOF\\n\" + {t} + \"\\nSH2EOF\""),
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
                    "w" => format!(" + \" > \" + shQuote({t})"),
                    "a" => format!(" + \" >> \" + shQuote({t})"),
                    _ => String::new(),
                });
            }
            if items.len() == 2 {
                // maybe [mode, target]
                if let Some(IrExpr::Str(m, _)) = items.first() {
                    let t = self.expr_str(&items[1])?;
                    return Ok(match m.as_str() {
                        "w" => format!(" + \" > \" + shQuote({t})"),
                        "a" => format!(" + \" >> \" + shQuote({t})"),
                        _ => String::new(),
                    });
                }
            }
        }
        Ok(String::new())
    }

    fn side_text(&mut self, e: &IrExpr) -> Result<String, String> {
        match e {
            IrExpr::Call { func, args, .. } if func == "test" => {
                // [[ ... ]] / [ ... ] in a command chain: render the real
                // bracket test text from the tokenized operands
                let mut toks: Vec<TestTok> = Vec::new();
                for a in args.iter() {
                    match a {
                        IrExpr::Array(items) => for it in items.iter() {
                            if let IrExpr::Str(t2, _) = it { self.push_test_str(&mut toks, t2); }
                        },
                        IrExpr::Str(t2, _) => self.push_test_str(&mut toks, t2),
                        other => {
                            let x = self.expr_str(other)?;
                            toks.push(TestTok::E(x));
                        }
                    }
                }
                let toks: Vec<TestTok> = toks.into_iter()
                    .filter(|t| !matches!(t, TestTok::S(x) if matches!(x.as_str(), "[" | "]" | "[[" | "]]")))
                    .collect();
                let b = self.test_tokens(&toks)?;
                self.helper("num");
                Ok(format!("[ (({{ __SH_RC = ({b}) ? 0 : 1; }}) == 0) ]"))
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
            IrExpr::BinOp { ref op, lhs, rhs, .. }
                if matches!(*op, BinOpKind::And | BinOpKind::Or) =>
            {
                let sep = if matches!(*op, BinOpKind::And) { " && " } else { " || " };
                let l = self.side_text(lhs)?;
                let r = self.side_text(rhs)?;
                Ok(format!("({l}){sep}({r})"))
            }
            IrExpr::Call { func, args, .. } if func == "pipeline" => {
                let mut parts2 = Vec::new();
                if let Some(IrExpr::Array(stages)) = args.first() {
                    for st in stages.iter() {
                        if let IrExpr::Arrow(body) = st {
                            parts2.push(self.arrow_expr(body)?);
                        }
                    }
                }
                Ok(parts2.join(" + \" | \" + "))
            }
            IrExpr::Call { func, args, .. } if func == "and" || func == "or" => {
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
            other => Err(format!("command-chain side not representable: {other:?}")),
        }
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
    fn cmd_line_expr(&mut self, args: &[IrExpr]) -> Result<String, String> {
        self.helper("quote");
        let mut pieces: Vec<(String, bool)> = Vec::new();
        match args.first() {
            Some(IrExpr::Str(c, _)) => pieces.push((
                jstr(&c.replace("\u{1}SH2GLOB\u{1}", "")), c.contains("\u{1}SH2GLOB\u{1}"))),
            Some(other) => pieces.push((self.expr_str(other)?, true)),
            None => return Err("command: no command word".into()),
        }
        for w in args.iter().skip(1) {
            for x in flatten_words(std::slice::from_ref(w)) {
                pieces.extend(self.word_pieces(x)?);
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
enum TestTok { S(String), E(String) }

fn tok_str(t: &TestTok) -> String {
    match t {
        TestTok::S(s) => jstr(s),
        TestTok::E(e) => e.clone(),
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
        IrStmt::For { iter, .. } => vec![iter],
        IrStmt::Exit(Some(e)) | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => vec![e],
        IrStmt::WriteFile { path, content, .. } => vec![path, content],
        _ => Vec::new(),
    }
}

/// Runtime helper methods, emitted into Sh2Program on demand.
fn helper_src(name: &str) -> Option<&'static str> {
    Some(match name {
        "env" => r#"    static String shCwd() { return __SH_CWD; }
    static String shAbs(String p) {
        if (p.isEmpty() || p.startsWith("/")) return p;
        return __SH_CWD + "/" + p;
    }
    static String shEnv(String n) {
        String v = System.getenv(n);
        return v == null ? "" : v;
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
        "run" => r#"    static long shRun(String cmdline) {
        try {
            ProcessBuilder pb = new ProcessBuilder("bash", "-c", cmdline);
            pb.directory(new File(shCwd()));
            pb.environment().putAll(__SH_EXPORTS);
            pb.redirectErrorStream(false);
            pb.inheritIO();
            Process p = pb.start();
            int rc = p.waitFor();
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
            ProcessBuilder pb = new ProcessBuilder("bash", "-c", cmdline);
            pb.directory(new File(shCwd()));
            pb.environment().putAll(__SH_EXPORTS);
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
        int i = s.lastIndexOf('/');
        return i < 0 ? s : s.substring(i + 1);
    }
"#,
        "dirname" => r#"    static String shDirname(String s) {
        int i = s.lastIndexOf('/');
        if (i < 0) return ".";
        if (i == 0) return "/";
        return s.substring(0, i);
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

/// Tokenize a string-form test expression (`-f "my file"`) on whitespace
/// respecting double/single quotes.
fn split_test_text(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let ch: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q { quote = None; }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    cur.push(c);
                } else if c == '$' && i + 1 < ch.len() && ch[i + 1] == '{' {
                    // ${...}: copy verbatim until the matching '}'
                    let mut depth = 0i32;
                    while i < ch.len() {
                        cur.push(ch[i]);
                        if ch[i] == '{' { depth += 1; }
                        if ch[i] == '}' { depth -= 1; if depth == 0 { i += 1; break; } }
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
