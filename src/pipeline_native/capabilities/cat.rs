//! Capability: `cat <<'EOF' … EOF` — the parent already verified the shape
//! (cat with no args, a single stdin heredoc redirect with a literal body);
//! this leaf just emits the body as a native `print`.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Heredoc(body) = ctx else {
        return None;
    };
    Some(NativeEmit::Stmt(format!(
        "print({}); $main_exit_code = $CHILD_ERROR = 0;",
        crate::ir::safe_perl_q_string(body)
    )))
}
