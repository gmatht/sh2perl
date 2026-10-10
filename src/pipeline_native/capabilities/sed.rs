//! Capability: `sed` — the common `s/PAT/REPL/[gp]` substitution forms the
//! corpus uses, in pipe position (`paste … | sed 's/\\t/ /g'`) and in
//! capture position (`x=$(sed -n 's/foo/bar/p' < FILE)`).
//!
//! Only the substitution command is lifted. BRE groups/operators are
//! converted to their Perl equivalents (`\\(` → `(`, `\\)` → `)`, `\\1` →
//! `$1`); `-n`/`p` print only substituted lines. Flags/commands we do not
//! model refuse and keep the shell-out.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    match ctx {
        NativeCtx::Pipe(p) if p.cmd == "sed" => {
            let (script, quiet) = parse_args(&p.words)?;
            let (op, only) = compile(&script, quiet)?;
            let body = &p.content_perl;
            let stmt = if only {
                format!(
                    "my @__o; for my $__l (split(/\\n/, {body}, -1)) {{ push @__o, $__l if $__l =~ {op}; }} print join(\"\\n\", @__o), (@__o ? \"\\n\" : \"\"); $main_exit_code = $CHILD_ERROR = 0;"
                )
            } else {
                format!(
                    "my @__o; for my $__l (split(/\\n/, {body}, -1)) {{ $__l =~ {op}; push @__o, $__l; }} print join(\"\\n\", @__o), (@__o ? \"\\n\" : \"\"); $main_exit_code = $CHILD_ERROR = 0;"
                )
            };
            Some(NativeEmit::Stmt(stmt))
        }
        NativeCtx::Capture { call } => {
            let (words, input) = capture_input(call)?;
            let (script, quiet) = parse_args(&words)?;
            let (op, only) = compile(&script, quiet)?;
            let val = if only {
                format!(
                    "(do {{ my @__o; for my $__l (split(/\\n/, {input}, -1)) {{ push @__o, $__l if $__l =~ {op}; }} join(\"\\n\", @__o) }})"
                )
            } else {
                format!(
                    "(do {{ my @__o; for my $__l (split(/\\n/, {input}, -1)) {{ $__l =~ {op}; push @__o, $__l; }} join(\"\\n\", @__o) }})"
                )
            };
            Some(NativeEmit::Stmt(format!(
                "$__sh_cap = {val}; $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        _ => None,
    }
}

/// `[-n] SCRIPT` → (script, quiet).
fn parse_args(words: &[String]) -> Option<(String, bool)> {
    let mut quiet = false;
    let mut script: Option<String> = None;
    for w in words {
        if w == "-n" {
            quiet = true;
        } else if w.starts_with('-') && w.len() > 1 {
            return None;
        } else if script.is_none() {
            script = Some(w.clone());
        } else {
            return None;
        }
    }
    Some((script?, quiet))
}

/// `s/PAT/REPL/FLAGS` → a Perl `s{PAT}{REPL}FLAGS` operator and whether
/// only substituted lines are printed.
fn compile(script: &str, quiet: bool) -> Option<(String, bool)> {
    let rest = script.trim().strip_prefix('s')?;
    let mut it = rest.chars();
    let delim = it.next()?;
    let body: String = it.collect();
    let mut fields: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut it = body.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some(n) if n == delim => cur.push(n),
                Some(n) => {
                    cur.push('\\');
                    cur.push(n);
                }
                None => cur.push('\\'),
            }
            continue;
        }
        if c == delim {
            fields.push(std::mem::take(&mut cur));
            if fields.len() == 2 {
                let flags: String = it.collect();
                let only = quiet || flags.contains('p');
                let perl_flags: String = flags.chars().filter(|c| *c != 'p').collect();
                return Some((
                    format!("s{{{}}}{{{}}}{}", bre_pat(&fields[0]), bre_repl(&fields[1]), perl_flags),
                    only,
                ));
            }
            continue;
        }
        cur.push(c);
    }
    None
}

/// BRE pattern → Perl regex.
fn bre_pat(p: &str) -> String {
    let mut out = String::new();
    let mut it = p.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some(x) if "(){}|+?".contains(x) => out.push(x),
                Some(x) => {
                    out.push('\\');
                    out.push(x);
                }
                None => out.push('\\'),
            }
        } else if "(){}|+?".contains(c) {
            out.push('\\');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

/// BRE replacement → Perl replacement (`\1` → `$1`, `&` → `$&`).
fn bre_repl(r: &str) -> String {
    let mut out = String::new();
    let mut it = r.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some(d) if d.is_ascii_digit() => {
                    out.push('$');
                    out.push(d);
                }
                Some('&') => out.push('&'),
                Some(x) => {
                    out.push('\\');
                    out.push(x);
                }
                None => out.push('\\'),
            }
        } else if c == '&' {
            out.push_str("$&");
        } else if c == '$' || c == '@' {
            out.push('\\');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

/// Unwrap a `redirect(Arrow([exec sed]), [{fd:0,r,FILE}])` call into the
/// sed words and a Perl expression for the input text.
fn capture_input(call: &crate::ir::IrExpr) -> Option<(Vec<String>, String)> {
    let crate::ir::IrExpr::Call { func, args } = call else {
        return None;
    };
    if func != "redirect" {
        return None;
    }
    let arrow = match args.first() {
        Some(crate::ir::IrExpr::Arrow(a)) => a,
        _ => return None,
    };
    let [crate::ir::IrStmt::Expr(e)] = arrow.as_slice() else {
        return None;
    };
    let crate::ir::IrExpr::Call { func: f2, args: a2 } = e else {
        return None;
    };
    if !matches!(f2.as_str(), "exec" | "builtin") {
        return None;
    }
    let [crate::ir::IrExpr::Str(cmd, _), crate::ir::IrExpr::Array(words)] = a2.as_slice() else {
        return None;
    };
    if cmd != "sed" {
        return None;
    }
    let ws: Vec<String> = words
        .iter()
        .map(crate::ir::grep_lit_str)
        .collect::<Option<_>>()?;
    let mut input: Option<String> = None;
    for spec in args.iter().skip(1) {
        let specs: Vec<&crate::ir::IrExpr> = match spec {
            crate::ir::IrExpr::Array(el) => el.iter().collect(),
            o => vec![o],
        };
        for sp in specs {
            if let crate::ir::IrExpr::Object(fields) = sp {
                let mut fd = 1i64;
                let mut mode = String::new();
                let mut target = String::new();
                for (k, v) in fields {
                    match k.as_str() {
                        "fd" => {
                            if let crate::ir::IrExpr::Int(n) = v {
                                fd = *n;
                            }
                        }
                        "mode" => mode = crate::ir::call_arg_str(v).unwrap_or_default(),
                        "target" => target = crate::ir::call_arg_str(v).unwrap_or_default(),
                        _ => {}
                    }
                }
                if fd == 0 && mode == "r" {
                    let tq = crate::ir::safe_perl_q_string(&target);
                    input = Some(format!(
                        "(do {{ local $/; if (open(my $__f, '<', {tq})) {{ my $__c = <$__f>; close $__f; $__c }} else {{ '' }} }})"
                    ));
                } else if mode == "herestring" {
                    input = Some(crate::ir::safe_perl_q_string(&format!("{target}\n")));
                }
            }
        }
    }
    Some((ws, input?))
}
