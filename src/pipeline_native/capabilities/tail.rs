//! Capability: `printf|echo|seq LITERAL | tail -c N` → native
//! `substr(…, -N)`, and `… | tail -N` → native last-N-lines.

use crate::pipeline_native::{exact_num, NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "tail" {
        return None;
    }
    // `tail -N` — the last N lines (fewer if the input is shorter). The
    // payload's trailing empty field (from a payload that ends in `\n`)
    // is not a line; drop it first, like sort/head.
    if p.words.len() == 1 {
        let n = p.words[0].strip_prefix('-')?.parse::<i64>().ok()?;
        if n < 0 {
            return None; // `tail -N` with a negative N means "all but the first N"
        }
        if n == 0 {
            // `tail -0` prints nothing
            return Some(NativeEmit::Stmt(
                "$main_exit_code = $CHILD_ERROR = 0;".to_string(),
            ));
        }
        return Some(NativeEmit::Stmt(format!(
            "my @__pl = split(/\\n/, {}); pop @__pl if @__pl && $__pl[-1] eq ''; @__pl = @__pl[(@__pl - {n})..$#__pl] if @__pl > {n}; print(join(\"\\n\", @__pl), \"\\n\") if @__pl; $main_exit_code = $CHILD_ERROR = 0;",
            p.content_perl
        )));
    }
    let n = exact_num(&p.words, "-c")?;
    Some(NativeEmit::Stmt(format!(
        "print(substr({}, -{n})); $main_exit_code = $CHILD_ERROR = 0;",
        p.content_perl
    )))
}
