//! Capability: the clean scalar forms of `typeset` — `typeset
//! [-i|-r|-l|-u|-x] VAR=VAL` with a literal VAL. -i/-r assign the value
//! (the integer/readonly attributes have no perl equivalent to enforce),
//! -l/-u case-fold it, -x also exports. Array/nameref/append forms refuse.

use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Exec { call, cond: false } = ctx else {
        return None;
    };
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "typeset") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    let mut flags = String::new();
    let mut assign: Option<&IrExpr> = None;
    for w in words {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                let f: String = s[1..].chars().filter(|c| "irlux".contains(*c)).collect();
                if f.len() != s.len() - 1 {
                    return None; // unsupported flag
                }
                flags.push_str(&f);
            }
            _ => {
                if assign.is_some() {
                    return None; // only one VAR=VAL
                }
                assign = Some(w);
            }
        }
    }
    if flags.is_empty() || assign.is_none() {
        return None;
    }
    if flags.chars().any(|c| !matches!(c, 'i' | 'r' | 'l' | 'u' | 'x')) {
        return None;
    }
    let a = assign.unwrap();
    let IrExpr::Str(text, _) = a else {
        return None;
    };
    let Some((var, value)) = text.split_once('=') else {
        return None;
    };
    if var.is_empty() {
        return None;
    }
    let val = crate::ir::safe_perl_q_string(value);
    // The attribute is applied to the VALUE ONCE and the same expression
    // is stored in %ENV: bash exports the value the variable actually
    // holds, so `$ENV{x} = raw` alongside `$x = lc(raw)` would publish the
    // un-folded text.
    let store = if flags.contains('l') {
        format!("lc({val})")
    } else if flags.contains('u') {
        format!("uc({val})")
    } else {
        val
    };
    let mut stmt = format!("${var} = {store};");
    if flags.contains('x') {
        stmt.push_str(&format!(" $ENV{{{var}}} = {store};"));
    }
    stmt.push_str("\n$main_exit_code = $CHILD_ERROR = 0;");
    Some(NativeEmit::Stmt(stmt))
}
