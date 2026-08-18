//! Capability: `printf LITERAL | sort[-nrf]` → native perl sort.
//! Self-gating: `None` unless the parent's consumer is `sort`.
use crate::pipeline_native::PipelineCtx;

pub(crate) fn emit(ctx: &PipelineCtx) -> Option<String> {
    if ctx.cmd != "sort" {
        return None;
    }
    let body = sort_body(&ctx.words)?;
    Some(format!(
        "my @__pl = split(/\\n/, {}); pop @__pl if @__pl && $__pl[$#__pl] eq ''; @__pl = {}; print(join(\"\\n\", @__pl), \"\\n\"); $main_exit_code = $CHILD_ERROR = 0;",
        ctx.content_perl, body
    ))
}

fn sort_body(words: &[String]) -> Option<String> {
    let (mut num, mut rev, mut fold) = (false, false, false);
    for w in words {
        let f = w.strip_prefix('-')?;
        for c in f.chars() {
            match c {
                'n' => num = true,
                'r' => rev = true,
                'f' => fold = true,
                _ => return None,
            }
        }
    }
    let op = if num { "<=>" } else { "cmp" };
    let cmp = if fold { format!("lc($a) {op} lc($b)") } else { format!("$a {op} $b") };
    let base = format!("sort {{ {cmp} }} @__pl");
    if rev { Some(format!("reverse({base})")) } else { Some(base) }
}
