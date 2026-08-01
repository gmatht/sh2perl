//! bc → native `Math.*` lowering — a CLI-side ESTree post-pass (M8-adjacent).
//!
//! The pattern `x=$(echo 'expr' | bc -l)` (and `bc -l <<< 'expr'`) is a
//! floating-point computation that currently spawns echo+bc per evaluation.
//! With the `SH2_BC_NATIVE=1` env var, this pass rewrites the emitted ESTree
//! JSON: a `sh2.capture` over a two-stage `sh2.pipeline([exec echo <expr>,
//! exec bc -l])` becomes a native `String(<Math expression>)` — zero spawns,
//! zero runtime dispatch.
//!
//! Deliberately a CLI-side pass (cli/src), NOT the emitter: it must not touch
//! the estree worker's fix surface (sh2perl/src, harness/*) and must be
//! opt-in so the corpus output stays byte-identical by default (`bc` in the
//! corpus, e.g. 070_cmp_basic.sh, keeps the real spawn path unless
//! SH2_BC_NATIVE is set).
//!
//! Compiled bc subset (bc -l): literals (incl. decimals), unary +/-,
//! `+ - * / % ^`, parens, and the -l functions `s c a l e sqrt` →
//! `Math.sin/cos/atan/log/exp/sqrt`; `^` → `Math.pow`; `%` → JS `%` (bc
//! float-remainder, close enough for uniforms). Anything else (bessel `j`,
//! `scale`/`ibase` vars, assignments) fails the compile → the spawn stays.

use serde_json::{Value, json};

fn is_sh2_call(node: &Value, name: &str) -> bool {
    node.get("type").and_then(|t| t.as_str()) == Some("CallExpression")
        && node.get("callee").and_then(|c| c.get("type").and_then(|t| t.as_str()))
            == Some("MemberExpression")
        && node["callee"]["object"]["name"].as_str() == Some("sh2")
        && node["callee"]["property"]["name"].as_str() == Some(name)
}

fn ident(name: &str) -> Value {
    json!({ "type": "Identifier", "name": name })
}

fn member(obj: Value, prop: &str) -> Value {
    json!({
        "type": "MemberExpression", "object": obj, "property": ident(prop),
        "computed": false, "optional": false,
    })
}

fn call(callee: Value, args: Vec<Value>) -> Value {
    json!({ "type": "CallExpression", "callee": callee, "arguments": args, "optional": false })
}

/// Extract the literal string from an `echo` arg: a plain string Literal or a
/// single-quasi TemplateLiteral with no interpolations.
fn literal_string(node: &Value) -> Option<String> {
    match node.get("type").and_then(|t| t.as_str()) {
        Some("Literal") => node.get("value").and_then(|v| v.as_str()).map(String::from),
        Some("TemplateLiteral") => {
            let quasis = node.get("quasis")?.as_array()?;
            let exprs = node.get("expressions")?.as_array()?;
            if quasis.len() == 1 && exprs.is_empty() {
                quasis[0]["value"]["raw"].as_str().map(String::from)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Does this arrow stage reduce to a `sh2.<callee>(...)` call where the
/// callee property matches? Returns the CallExpression itself.
fn arrow_call<'a>(node: &'a Value, callee: &str) -> Option<&'a Value> {
    let arrow = node.get("body")?;
    let expr = match arrow.get("type").and_then(|t| t.as_str()) {
        Some("BlockStatement") => {
            let body = arrow.get("body")?.as_array()?;
            let stmt = body.first()?;
            let es = stmt.get("expression")?;
            match es.get("type").and_then(|t| t.as_str()) {
                Some("AwaitExpression") => es.get("argument")?,
                _ => es,
            }
        }
        Some("AwaitExpression") => arrow.get("argument")?,
        _ => arrow,
    };
    if !is_sh2_call(expr, callee) {
        return None;
    }
    Some(expr)
}

/// Does this arrow stage reduce to `exec("<cmd>", [args...])`? Returns the
/// exec call's ARGUMENTS array (index 1).
fn arrow_exec<'a>(node: &'a Value, cmd: &str) -> Option<&'a Value> {
    let call = arrow_call(node, "exec")?;
    let args = call.get("arguments")?.as_array()?;
    if literal_string(&args[0])?.as_str() != cmd {
        return None;
    }
    args.get(1)
}

/// Try to rewrite `capture` → `String(<Math expr>)`. Returns Some(new node).
fn rewrite_capture(capture: &Value) -> Option<Value> {
    // capture(async () => pipeline([stage0, stage1]))
    let arrow = capture.get("arguments")?.get(0)?;
    let pipeline_call = arrow_call(arrow, "pipeline")?;
    let stages = pipeline_call
        .get("arguments")?
        .get(0)?
        .get("elements")?
        .as_array()?;
    if stages.len() != 2 {
        return None;
    }
    // stage 0: exec("echo", [<expr-string>])
    let echo_args = arrow_exec(&stages[0], "echo")?;
    // ESTree ArrayExpression nodes are JSON OBJECTS ({"type":"ArrayExpression",
    // "elements":[...]}) — extract the real JSON array via "elements".
    let echo_args = echo_args.get("elements")?.as_array()?;
    if echo_args.len() != 1 {
        return None;
    }
    let expr_src = literal_string(&echo_args[0])?;
    // stage 1: exec("bc", ["-l"]) or exec("bc", [])
    let bc_args = arrow_exec(&stages[1], "bc")?;
    let bc_args = bc_args.get("elements")?.as_array()?;
    let bc_ok = bc_args.is_empty()
        || (bc_args.len() == 1 && literal_string(&bc_args[0]).as_deref() == Some("-l"));
    if !bc_ok {
        return None;
    }
    let math = compile_bc_expr(&expr_src)?;
    Some(call(ident("String"), vec![math]))
}

/// Recursively rewrite bc captures anywhere in the tree. Returns true if any
/// rewrite happened.
fn rewrite_node(node: &mut Value) -> bool {
    let is_capture = is_sh2_call(node, "capture");
    let is_await_capture = matches!(
        node.get("type").and_then(|t| t.as_str()),
        Some("AwaitExpression")
    ) && node.get("argument").map_or(false, |a| is_sh2_call(a, "capture"));
    let mut changed = false;
    if let Some(arr) = node.as_array_mut() {
        for el in arr.iter_mut() {
            changed |= rewrite_node(el);
        }
    } else if let Some(obj) = node.as_object_mut() {
        // The sh2.capture call itself: its args may contain nested captures
        // (rewrite them first — innermost first is fine either way).
        for (_, v) in obj.iter_mut() {
            changed |= rewrite_node(v);
        }
        if is_capture {
            if let Some(replacement) = rewrite_capture(node) {
                *node = replacement;
                changed = true;
            }
        }
    }
    if is_await_capture {
        // The inner capture was rewritten by the recursion (or not); if it no
        // longer is a capture, drop the now-spurious `await` wrapper.
        let arg = node.get("argument").expect("await argument").clone();
        if !is_sh2_call(&arg, "capture") {
            *node = arg;
            changed = true;
        }
    }
    changed
}

/// Rewrite a full ESTree Program JSON string. Returns the (possibly
/// unchanged) JSON — the caller gates this on SH2_BC_NATIVE.
pub fn lower_bc_native(json: &str) -> String {
    let mut root: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return json.to_string(),
    };
    let ch = rewrite_node(&mut root);
    if ch {
        serde_json::to_string(&root).unwrap_or_else(|_| json.to_string())
    } else {
        json.to_string()
    }
}

// ── tiny bc -l expression compiler ───────────────────────────────────

struct BcParser<'a> {
    chars: Vec<char>,
    pos: usize,
    src: &'a str,
}

impl<'a> BcParser<'a> {
    fn new(src: &'a str) -> Self {
        BcParser { chars: src.chars().collect(), pos: 0, src }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ') | Some('\t') | Some('\n')) {
            self.pos += 1;
        }
    }

    fn expr(&mut self) -> Option<Value> {
        self.skip_ws();
        let mut left = self.term()?;
        loop {
            self.skip_ws();
            let op = match self.peek() {
                Some('+') | Some('-') => self.next(),
                _ => return Some(left),
            };
            let rhs = self.term()?;
            left = json!({
                "type": "BinaryExpression",
                "operator": op.unwrap().to_string(),
                "left": left,
                "right": rhs,
            });
        }
    }

    fn term(&mut self) -> Option<Value> {
        self.skip_ws();
        let mut left = self.power()?;
        loop {
            self.skip_ws();
            let op = match self.peek() {
                Some('*') | Some('/') | Some('%') => self.next(),
                _ => return Some(left),
            };
            let rhs = self.power()?;
            left = json!({
                "type": "BinaryExpression",
                "operator": op.unwrap().to_string(),
                "left": left,
                "right": rhs,
            });
        }
    }

    fn power(&mut self) -> Option<Value> {
        self.skip_ws();
        let base = self.unary()?;
        self.skip_ws();
        if self.peek() == Some('^') {
            self.next();
            let exp = self.unary()?;
            Some(call(member(ident("Math"), "pow"), vec![base, exp]))
        } else {
            Some(base)
        }
    }

    fn unary(&mut self) -> Option<Value> {
        self.skip_ws();
        match self.peek() {
            Some('-') => {
                self.next();
                Some(json!({
                    "type": "UnaryExpression", "operator": "-",
                    "argument": self.unary()?, "prefix": true,
                }))
            }
            Some('+') => {
                self.next();
                self.unary()
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Option<Value> {
        self.skip_ws();
        match self.next()? {
            '(' => {
                let e = self.expr()?;
                self.skip_ws();
                if self.next() != Some(')') {
                    return None;
                }
                Some(e)
            }
            c if c.is_ascii_digit() || c == '.' => {
                let mut s = String::new();
                s.push(c);
                while let Some(d) = self.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        s.push(d);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                // bc allows `1.` and `.5`; JS JSON numbers want `1.0`/`0.5`.
                if s.starts_with('.') {
                    s.insert(0, '0');
                }
                if s.ends_with('.') {
                    s.push('0');
                }
                let n: f64 = s.parse().ok()?;
                // ESTree Literal node (a bare JSON number would break the printer)
                Some(json!({"type": "Literal", "value": n, "raw": null}))
            }
            c if c.is_ascii_alphabetic() => {
                let mut name = String::new();
                name.push(c);
                while let Some(d) = self.peek() {
                    if d.is_ascii_alphanumeric() || d == '_' {
                        name.push(d);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                // function call required for -l math functions
                self.skip_ws();
                if self.next() != Some('(') {
                    return None;
                }
                let arg = self.expr()?;
                self.skip_ws();
                if self.next() != Some(')') {
                    return None;
                }
                let math_name = match name.as_str() {
                    "s" => "sin",
                    "c" => "cos",
                    "a" => "atan",
                    "l" => "log",
                    "e" => "exp",
                    "sqrt" => "sqrt",
                    _ => return None,
                };
                Some(call(member(ident("Math"), math_name), vec![arg]))
            }
            _ => None,
        }
    }
}

/// Compile a bc -l expression string to a native JS Math AST (or None).
fn compile_bc_expr(src: &str) -> Option<Value> {
    let mut p = BcParser::new(src);
    let e = p.expr()?;
    p.skip_ws();
    if p.pos == p.chars.len() {
        Some(e)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_bc_math() {
        let v = compile_bc_expr("s(0.5)").unwrap();
        assert_eq!(v["callee"]["property"]["name"], "sin");
        let v = compile_bc_expr("30 * 3.14159 / 180").unwrap();
        assert_eq!(v["operator"], "/"); // left-assoc: (30*pi)/180
        assert_eq!(v["left"]["operator"], "*");
        assert!(compile_bc_expr("j(1,2)").is_none()); // bessel unsupported
        assert!(compile_bc_expr("scale=2; s(1)").is_none());
    }

    #[test]
    fn rewrites_capture_pattern() {
        // Build the exact emission shape of `x=$(echo 's(0.5)' | bc -l)`:
        // sh2.capture(async () => await sh2.pipeline([...stages...]))
        let stage = |name: &str, args: Vec<Value>| json!({
            "type": "ArrowFunctionExpression", "async": true, "expression": false,
            "body": { "type": "AwaitExpression", "argument": {
                "type": "CallExpression", "optional": false,
                "callee": { "type": "MemberExpression", "object": ident("sh2"),
                            "property": ident("exec"), "computed": false, "optional": false },
                "arguments": [ json!({"type":"Literal","value":name,"raw":null}),
                               json!({"type":"ArrayExpression","elements":args}) ],
            } },
        });
        let pipeline = json!({
            "type": "CallExpression", "optional": false,
            "callee": { "type": "MemberExpression", "object": ident("sh2"),
                        "property": ident("pipeline"), "computed": false, "optional": false },
            "arguments": [ { "type": "ArrayExpression", "elements": [
                stage("echo", vec![json!({"type":"Literal","value":"s(0.5)","raw":null})]),
                stage("bc", vec![json!({"type":"Literal","value":"-l","raw":null})]),
            ] } ],
        });
        let capture = json!({
            "type": "CallExpression", "optional": false,
            "callee": { "type": "MemberExpression", "object": ident("sh2"),
                        "property": ident("capture"), "computed": false, "optional": false },
            "arguments": [ { "type": "ArrowFunctionExpression", "async": true, "expression": true,
                             "body": { "type": "AwaitExpression", "argument": pipeline } } ],
        });
        let program = json!({ "type": "Program", "sourceType": "module", "body": [
            { "type": "ExpressionStatement", "expression": capture },
        ] });
        let out = lower_bc_native(&serde_json::to_string(&program).unwrap());
        assert!(out.contains("\"name\":\"Math\""), "output: {out}");
        assert!(out.contains("\"name\":\"sin\""));
        assert!(!out.contains("\"name\":\"bc\""));
        assert!(!out.contains("pipeline"));
    }


}
