//! Capability: `printf LITERAL | tail -c N` → native `substr(…, -N)`.

use crate::pipeline_native::{exact_num, NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "tail" {
        return None;
    }
    let n = exact_num(&p.words, "-c")?;
    Some(NativeEmit::Stmt(format!(
        "print(substr({}, -{n})); $main_exit_code = $CHILD_ERROR = 0;",
        p.content_perl
    )))
}
