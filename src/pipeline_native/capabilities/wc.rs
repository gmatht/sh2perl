//! Capability: `printf LITERAL | wc -L/-l` → native longest-line / count.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "wc" {
        return None;
    }
    if p.words == ["-L"] {
        Some(NativeEmit::Stmt(format!(
            "my $__m = 0; for my $__l (split(/\\n/, {})) {{ $__m = $__m < length($__l) ? length($__l) : $__m; }} print $__m, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;",
            p.content_perl
        )))
    } else if p.words == ["-l"] {
        Some(NativeEmit::Stmt(format!(
            "my @__pl = split(/\\n/, {}); print scalar(@__pl), \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;",
            p.content_perl
        )))
    } else {
        None
    }
}
