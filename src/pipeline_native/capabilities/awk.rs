//! Capability: `awk` — the `{print $N}` field-print form the corpus uses
//! (`cat f | awk '{print $1}'`), lowered to a native per-line split.
//!
//! Only a program that is EXACTLY `{print $N}` (N ≥ 1, whitespace
//! tolerated: `{ print $3 }`, `{print$3}`) is lifted; any other program,
//! an `-v`/`-i` flag or a FILE argument refuses and keeps the shell-out
//! (REFUSE > GUESS).
//!
//! Semantics: perl's `split(' ', $l)` IS awk's default-FS splitting
//! (documented as such — leading whitespace stripped, runs collapse), and
//! a missing field prints the empty line, same as awk. The emitted loop
//! follows the sed capability's exact shape so the two compose in a chain.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "awk" {
        return None;
    }
    let field = parse_args(&p.words)?;
    let body = &p.content_perl;
    let stmt = format!(
        "my @__o; for my $__l (split(/\\n/, {body}, -1)) {{ my @__f = split(' ', $__l); push @__o, @__f >= {field} ? $__f[{field} - 1] : ''; }} print join(\"\\n\", @__o), (@__o ? \"\\n\" : \"\"); $main_exit_code = $CHILD_ERROR = 0;"
    );
    Some(NativeEmit::Stmt(stmt))
}

/// Exactly one word — the `{print $N}` program → the 1-based field.
fn parse_args(words: &[String]) -> Option<usize> {
    if words.len() != 1 {
        return None;
    }
    let prog = words[0].trim();
    let inner = prog.strip_prefix('{')?.strip_suffix('}')?;
    // Whitespace-insensitive compare: `{print $1}` / `{ print $3 }` /
    // `{print$3}` are the same program.
    let flat: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
    let n = flat.strip_prefix("print$")?.parse::<usize>().ok()?;
    if n == 0 {
        return None;
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_corpus_spellings() {
        assert_eq!(parse_args(&words(&["{print $1}"])), Some(1));
        assert_eq!(parse_args(&words(&["{ print $3 }"])), Some(3));
        assert_eq!(parse_args(&words(&["{print$3}"])), Some(3));
    }

    #[test]
    fn refuses_everything_else() {
        assert_eq!(parse_args(&words(&["{print $0}"])), None);
        assert_eq!(parse_args(&words(&["{print $1, $2}"])), None);
        assert_eq!(parse_args(&words(&["{for(i=1;i<=NF;i++) print $i}"])), None);
        assert_eq!(parse_args(&words(&["-v", "x=1", "{print $1}"])), None);
        assert_eq!(parse_args(&words(&["{print $1}", "file.txt"])), None);
        assert_eq!(parse_args(&words(&[])), None);
    }
}
