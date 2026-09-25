//! Capability: `printf|echo|seq LITERAL | head -c N` → native
//! `substr(…, 0, N)`, and `… | head -N` → native first-N-lines.

use crate::pipeline_native::{exact_num, NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "head" {
        return None;
    }
    // `head -N` — the first N lines (fewer if the input is shorter).
    // The payload has no trailing newline (the printf/echo/seq producer
    // convention), so an empty payload yields no lines, and a payload
    // with a trailing `\n` (e.g. from `echo -e "a\n"`) must not count
    // the empty field after it as a line — drop it first, like sort.
    if p.words.len() == 1 {
        let n = p.words[0].strip_prefix('-')?.parse::<i64>().ok()?;
        if n < 0 {
            return None; // `head -N` with a negative N means "all but the last N"
        }
        if n == 0 {
            // `head -0` prints nothing
            return Some(NativeEmit::Stmt(
                "$main_exit_code = $CHILD_ERROR = 0;".to_string(),
            ));
        }
        return Some(NativeEmit::Stmt(format!(
            "my @__pl = split(/\\n/, {}); pop @__pl if @__pl && $__pl[-1] eq ''; @__pl = @__pl[0..{end}] if @__pl > {n}; print(join(\"\\n\", @__pl), \"\\n\") if @__pl; $main_exit_code = $CHILD_ERROR = 0;",
            p.content_perl,
            n = n,
            end = n - 1
        )));
    }
    let n = exact_num(&p.words, "-c")?;
    Some(NativeEmit::Stmt(format!(
        "print(substr({}, 0, {n})); $main_exit_code = $CHILD_ERROR = 0;",
        p.content_perl
    )))
}
