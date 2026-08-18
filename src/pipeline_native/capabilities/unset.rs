//! Capability: `unset VAR…` → native undef.
//!
//! Type-agnostic: bash `unset x` removes the variable whatever its kind, so
//! the emission clears all three perl sigils by symbol-table name inside a
//! `no strict 'refs'` block (undef-ing a slot that doesn't exist is a
//! harmless no-op). This makes `${x+x}`-style "is-set" reads see "unset"
//! regardless of whether the variable was scalar/array/hash.

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
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "unset") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    if words.is_empty() {
        return None;
    }
    let mut out = String::from("{ no strict 'refs';");
    for w in &words {
        let name = crate::ir::grep_lit_str(w)?;
        let n = crate::ir::safe_perl_q_string(&name);
        out.push_str(&format!(" undef ${{{n}}}; undef @{{{n}}}; undef %{{{n}}};"));
    }
    out.push_str(" }\n$main_exit_code = $CHILD_ERROR = 0;");
    Some(NativeEmit::Stmt(out))
}
