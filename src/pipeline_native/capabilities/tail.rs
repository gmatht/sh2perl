//! Capability: `printf LITERAL | tail -c N` → native `substr(…, -N)`.
use crate::pipeline_native::{exact_num, PipelineCtx};

pub(crate) fn emit(ctx: &PipelineCtx) -> Option<String> {
    if ctx.cmd != "tail" {
        return None;
    }
    let n = exact_num(&ctx.words, "-c")?;
    Some(format!(
        "print(substr({}, -{n})); $main_exit_code = $CHILD_ERROR = 0;",
        ctx.content_perl
    ))
}
