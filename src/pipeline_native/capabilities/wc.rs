//! Capability: `printf LITERAL | wc -L/-l` → native longest-line / line count.
use crate::pipeline_native::PipelineCtx;

pub(crate) fn emit(ctx: &PipelineCtx) -> Option<String> {
    if ctx.cmd != "wc" {
        return None;
    }
    if ctx.words == ["-L"] {
        Some(format!(
            "my $__m = 0; for my $__l (split(/\\n/, {})) {{ $__m = $__m < length($__l) ? length($__l) : $__m; }} print $__m, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;",
            ctx.content_perl
        ))
    } else if ctx.words == ["-l"] {
        Some(format!(
            "my @__pl = split(/\\n/, {}); print scalar(@__pl), \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;",
            ctx.content_perl
        ))
    } else {
        None
    }
}
