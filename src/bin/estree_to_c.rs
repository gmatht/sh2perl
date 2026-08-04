//! DRAFT: ESTree JSON -> C renderer (proof of shape).
//!
//! Consumes `debashc file --estree foo.sh` output on stdin and emits C.
//! Deliberately worktree-local (branch `backend/c`): reads only the
//! cross-repo ESTree-JSON contract (PLAN.md 1.2), touches NO shared core
//! (src/shir.rs, src/ir.rs, src/estree.rs, src/parser/) and never merges
//! to main — the estree worker owns those single-owner files. The C
//! backend is a *consumer* of the JSON contract, exactly like sh2runtime.
//!
//! Draft scope (v0):
//!   - VariableDeclaration / AssignmentExpression / BinaryExpression
//!   - IfStatement, TemplateLiteral, Literal, Identifier
//!   - process.stdout.write(...) -> fputs/printf (echo shape: arr.join + "\n")
//!   - Math.trunc/floor/ceil/round/sqrt/pow, String(x), a few JS string
//!     methods (toUpperCase/toLowerCase/includes/slice) -> tiny C shims
//!   - sh2.* calls -> compile-able stubs (abort with a TODO message)
//! Everything else -> a `/* TODO(unsupported) */` marker so the draft
//! always compiles. The naive type tracking (numeric init -> long long,
//! else char*) is exactly the "C needs type inference" problem PLAN.md
//! v2 flagged; the draft surfaces it rather than solving it.
//!
//! Usage:
//!   /nvme/ai/sh2loop/sh2perl/target/debug/debashc file --estree foo.sh \
//!     | cargo run --bin estree_to_c > foo.c && gcc foo.c -lm -o foo

use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::io::{self, Read, Write};

#[derive(Default)]
struct Render {
    out: Vec<String>,
    depth: usize,
    /// Identifier name -> is numeric (long long) or string (char*)
    var_types: HashMap<String, bool>,
    /// distinct sh2.* callee names seen
    sh2_calls: BTreeSet<String>,
    /// helper shims needed
    need_upper: bool,
    need_lower: bool,
    need_includes: bool,
    need_slice: bool,
    todo: usize,
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

    /// Escape a JS string value as a C string literal.
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

    // ---------- expressions ----------

    fn expr(&mut self, n: &Value) -> String {
        match n.get("type").and_then(|t| t.as_str()) {
            Some("Literal") => {
                let v = n.get("value");
                match v {
                    Some(Value::Bool(b)) => {
                        if *b { "1".into() } else { "0".into() }
                    }
                    Some(Value::Number(num)) => num.to_string(),
                    Some(Value::String(s)) => Self::cstr(s),
                    _ => "0".into(),
                }
            }
            Some("Identifier") => n["name"].as_str().unwrap_or("").to_string(),
            Some("TemplateLiteral") => {
                self.mark_todo("TemplateLiteral standalone");
                "0".into()
            }
            Some("BinaryExpression") => {
                let op = n["operator"].as_str().unwrap_or("");
                let l = self.expr(&n["left"]);
                let r = self.expr(&n["right"]);
                if op == "**" {
                    format!("pow({l},{r})")
                } else if op == "!==" {
                    format!("({l} != {r})")
                } else if op == "===" {
                    format!("({l} == {r})")
                } else {
                    format!("({l} {op} {r})")
                }
            }
            Some("UnaryExpression") => {
                format!("({}{})", n["operator"].as_str().unwrap_or(""), self.expr(&n["argument"]))
            }
            Some("AssignmentExpression") => format!(
                "({} {} {})",
                self.expr(&n["left"]),
                n["operator"].as_str().unwrap_or("="),
                self.expr(&n["right"])
            ),
            Some("CallExpression") => self.call(n),
            Some("MemberExpression") => {
                let obj = self.expr(&n["object"]);
                let prop = n["property"]["name"].as_str().unwrap_or("");
                if obj == "process" && prop == "stdout" {
                    "stdout".into()
                } else if obj == "sh2" {
                    // sh2.* state reads (lastExit etc.) are runtime concerns
                    self.mark_todo(&format!("sh2.{prop} read"));
                    "0".into()
                } else {
                    format!("{obj}.{prop}")
                }
            }
            Some("ArrayExpression") => {
                let elems: Vec<String> = n["elements"]
                    .as_array()
                    .map(|a| a.iter().map(|e| self.expr(e)).collect())
                    .unwrap_or_default();
                format!("[{}]", elems.join(", "))
            }
            other => {
                self.mark_todo(&format!("expr {}", other.unwrap_or("?")));
                "0".into()
            }
        }
    }

    fn call(&mut self, n: &Value) -> String {
        let callee = &n["callee"];
        let args: Vec<Value> = n["arguments"].as_array().cloned().unwrap_or_default();
        if callee["type"] == "MemberExpression" {
            let obj = &callee["object"];
            let prop = callee["property"]["name"].as_str().unwrap_or("");
            // process.stdout.write(X)
            if obj["type"] == "MemberExpression" && self.expr(obj) == "stdout" && prop == "write" {
                return self.stdout_write(args.first().cloned().unwrap_or(Value::Null));
            }
            // arr.join(sep)
            if obj["type"] == "ArrayExpression" && prop == "join" {
                let parts = self.join_parts(obj);
                return self.printf_from_parts(parts);
            }
            // Math.*
            if obj["type"] == "Identifier" && obj["name"] == "Math" {
                match prop {
                    "trunc" => return format!("((long long)({}))", self.expr(&args[0])),
                    "floor" | "ceil" | "round" => {
                        return format!("((long long){}({}))", prop, self.expr(&args[0]))
                    }
                    "sqrt" | "pow" => {
                        let a: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                        return format!("{}({})", prop, a.join(", "));
                    }
                    _ => {}
                }
            }
            // String(x) wrapper
            if obj["type"] == "Identifier" && obj["name"] == "String" {
                return format!("((char*)({}))", self.expr(&args[0]));
            }
            // JS string methods -> tiny C shims
            match prop {
                "toUpperCase" => {
                    self.need_upper = true;
                    return format!("c_upper((char*)({}))", self.expr(obj));
                }
                "toLowerCase" => {
                    self.need_lower = true;
                    return format!("c_lower((char*)({}))", self.expr(obj));
                }
                "includes" => {
                    self.need_includes = true;
                    return format!("c_includes((char*)({}), (char*)({}))", self.expr(obj), self.expr(&args[0]));
                }
                "slice" => {
                    self.need_slice = true;
                    let a: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                    return format!("c_slice((char*)({}), {})", self.expr(obj), a.join(", "));
                }
                _ => {}
            }
            // sh2.* namespace
            if obj["type"] == "Identifier" && obj["name"] == "sh2" {
                let name = prop.to_string();
                self.sh2_calls.insert(name.clone());
                let a: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                return format!("sh2_{name}({})", a.join(", "));
            }
            let a: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
            return format!("{}({})", self.expr(callee), a.join(", "));
        }
        if callee["type"] == "Identifier" {
            let name = callee["name"].as_str().unwrap_or("");
            if name == "String" {
                return format!("((char*)({}))", self.expr(&args[0]));
            }
            let a: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
            return format!("{name}({})", a.join(", "));
        }
        self.mark_todo(&format!("call {}", callee["type"]));
        "0".into()
    }

    // ---------- echo-shape helpers ----------

    /// `[e1, e2, ..].join(sep)` -> printf parts (used by stdout_write).
    fn join_parts(&mut self, arr: &Value) -> Vec<Part> {
        arr["elements"]
            .as_array()
            .map(|a| {
                let mut p = Vec::new();
                for e in a {
                    p.extend(self.parts_of(e));
                }
                p
            })
            .unwrap_or_default()
    }

    /// Split an expression into printf parts.
    /// Part::Lit(text) | Part::Arg(cexpr, is_num)
    fn parts_of(&mut self, n: &Value) -> Vec<Part> {
        match n.get("type").and_then(|t| t.as_str()) {
            Some("Literal") => {
                let v = n.get("value");
                match v {
                    Some(Value::Bool(b)) => vec![Part::Arg(if *b { "1" } else { "0" }.into(), true)],
                    Some(Value::Number(num)) => vec![Part::Arg(num.to_string(), true)],
                    Some(Value::String(s)) => vec![Part::Lit(s.clone())],
                    _ => vec![Part::Arg("0".into(), false)],
                }
            }
            Some("TemplateLiteral") => {
                let mut out = Vec::new();
                let quasis = n["quasis"].as_array().cloned().unwrap_or_default();
                let exprs = n["expressions"].as_array().cloned().unwrap_or_default();
                for (i, q) in quasis.iter().enumerate() {
                    let text = q["value"]["cooked"].as_str().or_else(|| q["value"]["raw"].as_str());
                    if let Some(t) = text {
                        out.push(Part::Lit(t.to_string()));
                    }
                    if let Some(e) = exprs.get(i) {
                        out.push(Part::Arg(self.expr(e), self.is_num(e)));
                    }
                }
                out
            }
            Some("Identifier") => {
                let name = n["name"].as_str().unwrap_or("").to_string();
                let is_num = self.var_types.get(&name).copied().unwrap_or(false);
                vec![Part::Arg(name, is_num)]
            }
            Some("CallExpression") => {
                let callee = &n["callee"];
                // String(x) wrapper -> unwrap
                if callee["type"] == "Identifier"
                    && callee["name"] == "String"
                    && !n["arguments"].as_array().map(|a| a.is_empty()).unwrap_or(true)
                {
                    return self.parts_of(&n["arguments"][0]);
                }
                // [e1, e2, ..].join(sep) -> its flattened parts
                if callee["type"] == "MemberExpression"
                    && callee["object"]["type"] == "ArrayExpression"
                    && callee["property"]["name"] == "join"
                {
                    return self.join_parts(&callee["object"]);
                }
                vec![Part::Arg(self.call(n), self.is_num(n))]
            }
            Some("BinaryExpression") => vec![Part::Arg(self.expr(n), true)],
            _ => vec![Part::Arg(self.expr(n), false)],
        }
    }

    fn is_num(&mut self, n: &Value) -> bool {
        match n.get("type").and_then(|t| t.as_str()) {
            Some("Literal") => matches!(n.get("value"), Some(Value::Number(_))),
            Some("Identifier") => self
                .var_types
                .get(n["name"].as_str().unwrap_or(""))
                .copied()
                .unwrap_or(false),
            Some("BinaryExpression") | Some("UnaryExpression") => true,
            Some("CallExpression") => {
                let callee = &n["callee"];
                callee["type"] == "MemberExpression"
                    && callee["object"]["type"] == "Identifier"
                    && callee["object"]["name"] == "Math"
            }
            _ => false,
        }
    }

    fn stdout_write(&mut self, arg: Value) -> String {
        if arg.is_null() {
            return "fputs(\"\", stdout)".into();
        }
        if arg["type"] == "Literal" && arg.get("value").and_then(|v| v.as_str()).is_some() {
            return format!("fputs({}, stdout)", Self::cstr(arg["value"].as_str().unwrap()));
        }
        // The echo shape is `(arr.join(sep) + "\n")`; decompose the
        // top-level string concatenation into printf parts instead of
        // rendering it as one C expression.
        let mut parts = Vec::new();
        if arg["type"] == "BinaryExpression" && arg["operator"] == "+" {
            parts.extend(self.parts_of(&arg["left"]));
            parts.extend(self.parts_of(&arg["right"]));
        } else {
            parts = self.parts_of(&arg);
        }
        self.printf_from_parts(parts)
    }

    fn printf_from_parts(&mut self, parts: Vec<Part>) -> String {
        let mut fmt = String::new();
        let mut cargs = Vec::new();
        for p in parts {
            match p {
                Part::Lit(t) => fmt.push_str(&t),
                Part::Arg(v, is_num) => {
                    if is_num {
                        fmt.push_str("%lld");
                        cargs.push(format!("(long long)({v})"));
                    } else {
                        fmt.push_str("%s");
                        cargs.push(format!("({v})"));
                    }
                }
            }
        }
        if cargs.is_empty() {
            format!("fputs({}, stdout)", Self::cstr(&fmt))
        } else {
            format!("printf({}, {})", Self::cstr(&fmt), cargs.join(", "))
        }
    }

    // ---------- statements ----------

    fn stmt(&mut self, n: &Value) {
        match n.get("type").and_then(|t| t.as_str()) {
            Some("VariableDeclaration") => {
                let decls = n["declarations"].as_array().cloned().unwrap_or_default();
                for d in decls {
                    let name = d["id"]["name"].as_str().unwrap_or("").to_string();
                    let init = d.get("init").cloned().unwrap_or(Value::Null);
                    if init.is_null() {
                        self.emit(&format!("char* {name} = NULL;"));
                        self.var_types.insert(name, false);
                    } else if init["type"] == "Literal"
                        && init.get("value").and_then(|v| v.as_i64()).is_some()
                    {
                        self.emit(&format!("long long {name} = {};", init["value"].as_i64().unwrap()));
                        self.var_types.insert(name, true);
                    } else {
                        self.emit(&format!("char* {name} = NULL;"));
                        self.var_types.insert(name.clone(), false);
                        let init_expr = self.expr(&init);
                        self.emit(&format!("{name} = {init_expr};"));
                    }
                }
            }
            Some("ExpressionStatement") => {
                let e = &n["expression"];
                // sh2.lastExit = N / sh2.setVar(...) etc -> ignore in draft
                if e["type"] == "AssignmentExpression" && e["left"]["type"] == "MemberExpression" {
                    let obj = &e["left"]["object"];
                    if obj["type"] == "Identifier" && obj["name"] == "sh2" {
                        let prop = e["left"]["property"]["name"].as_str().unwrap_or("");
                        let rhs = self.expr(&e["right"]);
                        self.emit(&format!("/* sh2.{prop} = {rhs} */"));
                        return;
                    }
                }
                if e["type"] == "CallExpression" {
                    let c = self.call(e);
                    self.emit(&format!("{c};"));
                } else {
                    let x = self.expr(e);
                    self.emit(&format!("({x});"));
                }
            }
            Some("IfStatement") => {
                let test = self.expr(&n["test"]);
                self.emit(&format!("if ({test}) {{"));
                self.depth += 1;
                if let Some(body) = n["consequent"]["body"].as_array() {
                    for s in body {
                        self.stmt(s);
                    }
                }
                self.depth -= 1;
                let alt = n.get("alternate");
                if let Some(alt) = alt {
                    self.emit("} else {");
                    self.depth += 1;
                    if alt["type"] == "BlockStatement" {
                        if let Some(body) = alt["body"].as_array() {
                            for s in body {
                                self.stmt(s);
                            }
                        }
                    } else {
                        self.stmt(alt);
                    }
                    self.depth -= 1;
                    self.emit("}");
                } else {
                    self.emit("}");
                }
            }
            Some("BlockStatement") => {
                self.emit("{");
                self.depth += 1;
                if let Some(body) = n["body"].as_array() {
                    for s in body {
                        self.stmt(s);
                    }
                }
                self.depth -= 1;
                self.emit("}");
            }
            other => self.mark_todo(&format!("stmt {}", other.unwrap_or("?"))),
        }
    }

    // ---------- program ----------

    fn program(&mut self, prog: &Value) {
        // Render the body FIRST so the helper shim flags are known before
        // the preamble is emitted (C needs declarations before use).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        if let Some(body) = prog["body"].as_array() {
            for n in body {
                self.stmt(n);
            }
        }
        self.emit("return 0;");
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        self.emit("#include <stdio.h>");
        self.emit("#include <stdlib.h>");
        self.emit("#include <string.h>");
        self.emit("#include <ctype.h>");
        self.emit("#include <math.h>");
        self.emit("");
        if self.need_upper || self.need_lower {
            self.emit("static char* c_upper(char* s) { static char b[65536]; size_t i; for (i = 0; s[i]; i++) b[i] = (char)toupper((unsigned char)s[i]); b[i] = 0; return b; }");
            self.emit("static char* c_lower(char* s) { static char b[65536]; size_t i; for (i = 0; s[i]; i++) b[i] = (char)tolower((unsigned char)s[i]); b[i] = 0; return b; }");
        }
        if self.need_includes {
            self.emit("static int c_includes(const char* s, const char* p) { return strstr(s, p) != NULL; }");
        }
        if self.need_slice {
            self.emit("static char* c_slice(char* s, long long a, long long b) { static char buf[4096]; long long n = (long long)strlen(s); if (a < 0) a += n; if (b < 0) b += n; if (a < 0) a = 0; if (b > n) b = n; long long l = b - a; if (l < 0) l = 0; memcpy(buf, s + a, (size_t)l); buf[l] = 0; return buf; }");
        }
        self.emit("");
        self.emit("int main(void) {");
        self.out.extend(body_out.iter().cloned());
        self.emit("}");
        if !self.sh2_calls.is_empty() {
            self.emit("");
            self.emit("/* sh2.* runtime stubs — TODO: implement the namespace (PLAN.md 1.2) */");
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                self.emit(&format!("static void sh2_{name}(void) {{"));
                self.emit(&format!("  fprintf(stderr, \"TODO sh2.{name}\\n\");"));
                self.emit("  exit(2);");
                self.emit("}");
            }
        }
        if self.todo > 0 {
            self.emit(&format!("/* {} construct(s) lowered to TODO markers */", self.todo));
        }
    }
}

enum Part {
    Lit(String),
    Arg(String, bool),
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("read stdin");
    let prog: Value = serde_json::from_str(&input).expect("parse ESTree JSON");
    let mut r = Render::default();
    r.program(&prog);
    let mut stdout = io::stdout();
    for line in &r.out {
        writeln!(stdout, "{line}").expect("write stdout");
    }
}
