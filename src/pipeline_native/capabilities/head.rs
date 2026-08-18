//! Capability: `printf LITERAL | head -c N` → native `substr(…, 0, N)`.

use crate::pipeline_native::{exact_num, NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "head" {
        return None;
    }
    let n = exact_num(&p.words, "-c")?;
    Some(NativeEmit::Stmt(format!(
        "print(substr({}, 0, {n})); $main_exit_code = $CHILD_ERROR = 0;",
        p.content_perl
    )))
}
