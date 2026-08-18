//! Capability: `cmp -s [-n N] F1 F2` → native silent byte compare
//! (files-equal boolean in condition position, or a status assignment when
//! used as a statement). Bare `cmp` prints a differ message we won't
//! reproduce, so only the `-s` (silent) form lifts.

use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Exec { call, cond } = ctx else {
        return None;
    };
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "cmp") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    let b = cmp_boolean(&words)?;
    if *cond {
        Some(NativeEmit::Cond(b))
    } else {
        Some(NativeEmit::Stmt(format!(
            "$main_exit_code = $CHILD_ERROR = ({b}) ? 0 : 1;"
        )))
    }
}

/// `cmp -s [-n N] F1 F2` → the files-equal perl expression (read both
/// files; compare whole, or a `-n N` prefix). `-l/-b/-i`, non-literal
/// paths and globs refuse.
fn cmp_boolean(words: &[&IrExpr]) -> Option<String> {
    let mut quiet = false;
    let mut limit: Option<i64> = None;
    let mut files: Vec<&IrExpr> = Vec::new();
    let mut it = words.iter();
    while let Some(w) = it.next() {
        match w {
            IrExpr::Str(s, _) if s == "-s" => quiet = true,
            IrExpr::Str(s, _) if s == "-n" => {
                limit = it
                    .next()
                    .and_then(|v| crate::ir::grep_lit_str(v))
                    .and_then(|v| v.parse().ok())
                    .or(limit);
            }
            IrExpr::Str(s, _) if s.starts_with('-') => return None,
            _ => files.push(w),
        }
    }
    if !quiet || files.len() != 2 {
        return None;
    }
    let f1 = crate::ir::grep_lit_str(files[0])?;
    let f2 = crate::ir::grep_lit_str(files[1])?;
    if f1.contains('*') || f2.contains('*') {
        return None;
    }
    let p1 = crate::ir::safe_perl_q_string(&f1);
    let p2 = crate::ir::safe_perl_q_string(&f2);
    let compare = match limit {
        Some(n) => format!("substr($__c1,0,{n}) eq substr($__c2,0,{n})"),
        None => "$__c1 eq $__c2".to_string(),
    };
    Some(format!(
        "(sub {{ open(my $__f1,'<',{p1}) && open(my $__f2,'<',{p2}) or return 0; local $/; my $__c1=<$__f1>; my $__c2=<$__f2>; close $__f1; close $__f2; return ({compare}) ? 1 : 0; }}->())",
        p1 = p1,
        p2 = p2,
        compare = compare
    ))
}
