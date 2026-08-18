//! Capability: `printf LITERAL | head -c N` → native `substr(…, 0, N)`.
use crate::pipeline_native::{exact_num, PipelineCtx};

pub(crate) fn emit(ctx: &PipelineCtx) -> Option<String> {
    if ctx.cmd != "head" {
        return None;
    }
    let n = exact_num(&ctx.words, "-c")?;
    Some(format!(
        "print(substr({}, 0, {n})); $main_exit_code = $CHILD_ERROR = 0;",
        ctx.content_perl
    ))
}
