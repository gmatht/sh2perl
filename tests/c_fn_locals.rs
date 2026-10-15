//! Function-local vs file-scope rendering in the C backend.
//!
//! The py-sh-go frontend pins Python function params and loop counters
//! with bare `Declare(local: true)` scope annotations (the initializing
//! Assign is always emitted alongside, so Assign-keyed analyses see an
//! unchanged IR). The C backend must render those as true function
//! locals — not the shell-global default the vacuous fvar hoist would
//! otherwise produce (t86 `all_factors`: `n` and `i` were file-scope
//! `long long` globals; now they live inside the function).
//!
//! The counter-test pins the other direction: a var written in a
//! function body but READ at top level (the shell post-call `$n`
//! idiom) must STAY file-scope. Over-privatizing it (a function-local
//! shadow) would leave the top-level read stale — a miscompile.
//!
//! Vehicle: JSON IR (the exact frontend A1 shape) → ingress → render →
//! structural assertions on the generated text. No gcc needed.

fn render(json: &str) -> String {
    let prog = debashl::shir_json_in::shir_json_to_ir(json)
        .expect("test IR must ingress");
    debashl::c_backend::shir_to_c(&prog)
}

/// Byte offset of `name` as a whole identifier (not a substring of a
/// longer identifier), if present.
fn whole_ident_pos(line: &str, name: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut start = 0;
    while let Some(rel) = line[start..].find(name) {
        let i = start + rel;
        let before_ok = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        let j = i + name.len();
        let after_ok = j >= b.len() || !(b[j].is_ascii_alphanumeric() || b[j] == b'_');
        if before_ok && after_ok {
            return Some(i);
        }
        start = j;
    }
    None
}

fn is_decl_of(line: &str, name: &str) -> bool {
    // A name inside a function parameter list — `static char *f(long long n);`
    // (prototype) or its definition — is a PARAMETER, not a file-scope
    // variable. Only a name before any unmatched `(` can be a var decl.
    if let Some(pos) = whole_ident_pos(line, name) {
        let prefix = &line[..pos];
        if prefix.matches('(').count() > prefix.matches(')').count() {
            return false;
        }
    }
    // `long long n = 0;` / `char* n = NULL;` / `uint8_t n = 0;` …
    // the declared name is the first identifier after the type words.
    let mut toks = line
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '*'))
        .filter(|t| !t.is_empty());
    let mut prev_is_type = false;
    for t in toks {
        if matches!(
            t,
            "long" | "char" | "int" | "short" | "unsigned" | "signed" | "size_t"
                | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t"
                | "int8_t" | "int16_t" | "int32_t" | "int64_t"
        ) {
            prev_is_type = true;
            continue;
        }
        if prev_is_type && t == name {
            return true;
        }
        // `*` never survives the split (it is a separator); any other
        // identifier resets the type-word run.
        prev_is_type = false;
    }
    false
}

fn col0_decl(c: &str, name: &str) -> bool {
    // a file-scope declaration sits at column 0; function-body lines
    // are indented.
    c.lines()
        .any(|l| !(l.starts_with(' ') || l.starts_with('\t')) && is_decl_of(l, name))
}

fn indented_decl(c: &str, name: &str) -> bool {
    c.lines()
        .any(|l| (l.starts_with(' ') || l.starts_with('\t')) && is_decl_of(l, name))
}

/// `name` is a parameter of `fname` in a signature line (`f(long long n)`).
fn signature_param(c: &str, fname: &str, name: &str) -> bool {
    c.lines()
        .filter(|l| l.contains(&format!("{fname}(")))
        .any(|l| {
            let Some(open) = l.find('(') else { return false };
            let close = l[open..].find(')').map(|i| open + i).unwrap_or(l.len());
            l[open + 1..close]
                .split(',')
                .any(|p| p.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).any(|t| t == name))
        })
}

/// t86 shape, minimized: param `n` (Declare + `n = $1`) and loop
/// counter `i` (Declare + `i = 1`) inside a function must render as
/// function locals — no file-scope globals.
#[test]
fn param_and_loop_counter_are_function_locals() {
    let json = r#"{
        "type": "Program", "contract_version": 1,
        "imports": [], "requires": [], "subs": [],
        "var_types": [{"name": "n", "type": "Int"}, {"name": "i", "type": "Int"}],
        "stmts": [
            {"type": "Function", "name": "f", "body": [
                {"type": "Declare", "vars": [{"name": "n", "sigil": null}], "init": null, "local": true},
                {"type": "Assign", "targets": [{"var": "n", "sigil": null, "indices": []}],
                 "expr": {"type": "Arith", "ast": {"type": "Var", "name": "1"}}},
                {"type": "Declare", "vars": [{"name": "i", "sigil": null}], "init": null, "local": true},
                {"type": "Assign", "targets": [{"var": "i", "sigil": null, "indices": []}],
                 "expr": {"type": "Arith", "ast": {"type": "Num", "value": 1}}},
                {"type": "While",
                 "cond": {"type": "Arith", "ast": {"type": "Bin", "op": "<",
                    "lhs": {"type": "Var", "name": "i"}, "rhs": {"type": "Num", "value": 5}}},
                 "body": [
                    {"type": "Assign", "targets": [{"var": "i", "sigil": null, "indices": []}],
                     "expr": {"type": "Arith", "ast": {"type": "Bin", "op": "+",
                        "lhs": {"type": "Var", "name": "i"}, "rhs": {"type": "Var", "name": "n"}}}}
                 ]},
                {"type": "Expr", "expr": {"type": "Call", "func": "exec", "purity": "Emulable",
                 "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"},
                    {"type": "Array", "elements": [{"type": "Call", "func": "getVar", "purity": "Emulable",
                     "args": [{"type": "Str", "style": "DoubleQuoted", "value": "i"}]}]}]}}
            ]},
            {"type": "Expr", "expr": {"type": "Call", "func": "exec", "purity": "Emulable",
             "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"},
                {"type": "Array", "elements": [{"type": "Str", "style": "DoubleQuoted", "value": "done"}]}]}}
        ]
    }"#;
    let c = render(json);
    assert!(
        !col0_decl(&c, "n"),
        "param `n` leaked to file scope:\n{c}"
    );
    assert!(
        !col0_decl(&c, "i"),
        "loop counter `i` leaked to file scope:\n{c}"
    );
    assert!(
        // the numeric param is named directly in the signature now
        // (865f147b); an indented local (the pre-865f147b `_p1` copy)
        // is equally function-local.
        signature_param(&c, "f", "n") || indented_decl(&c, "n"),
        "param `n` is neither a signature parameter nor a function-local decl:\n{c}"
    );
    assert!(
        indented_decl(&c, "i"),
        "loop counter `i` has no function-local declaration:\n{c}"
    );
}

/// Counter-test (shell post-call idiom): `n` assigned in the function
/// body with NO Declare, then read at top level after the call, must
/// stay a file-scope global. A function-local shadow here would be a
/// miscompile (the top-level read would go stale).
#[test]
fn post_call_read_keeps_global() {
    let json = r#"{
        "type": "Program", "contract_version": 1,
        "imports": [], "requires": [], "subs": [],
        "var_types": [{"name": "n", "type": "Int"}],
        "stmts": [
            {"type": "Function", "name": "f", "body": [
                {"type": "Assign", "targets": [{"var": "n", "sigil": null, "indices": []}],
                 "expr": {"type": "Arith", "ast": {"type": "Num", "value": 12}}},
                {"type": "Expr", "expr": {"type": "Call", "func": "exec", "purity": "Emulable",
                 "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"},
                    {"type": "Array", "elements": [{"type": "Call", "func": "getVar", "purity": "Emulable",
                     "args": [{"type": "Str", "style": "DoubleQuoted", "value": "n"}]}]}]}}
            ]},
            {"type": "Expr", "expr": {"type": "Call", "func": "fnValue", "purity": "Emulable",
             "args": [{"type": "Str", "style": "DoubleQuoted", "value": "f"},
                {"type": "Array", "elements": []}]}},
            {"type": "Expr", "expr": {"type": "Call", "func": "exec", "purity": "Emulable",
             "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"},
                {"type": "Array", "elements": [{"type": "Call", "func": "getVar", "purity": "Emulable",
                 "args": [{"type": "Str", "style": "DoubleQuoted", "value": "n"}]}]}]}}
        ]
    }"#;
    let c = render(json);
    assert!(
        col0_decl(&c, "n"),
        "post-call-read `n` lost its file-scope global (over-privatized):\n{c}"
    );
}
