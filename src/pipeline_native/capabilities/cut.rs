//! Capability: `cut` — the single-field pipeline form the corpus uses
//! (`… | cut -d: -f1`), lowered to a native per-line split.
//!
//! Only `-d D -f N` (one delimiter char, one 1-based field) is lifted;
//! ranges/lists (`-f1,3`), `-s`, `-o`, attached-file arguments and any
//! other flag refuse and keep the shell-out (REFUSE > GUESS).
//!
//! Semantics copied from cut(1): a line with no delimiter is output
//! UNCHANGED (no `-s`), a missing field yields an empty line. The emitted
//! loop follows the sed capability's exact shape (per-line `\n` split
//! with `-1`, join back with a trailing newline iff any line was output,
//! status write at the end), so the two compose in a chain.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "cut" {
        return None;
    }
    let (delim, field) = parse_args(&p.words)?;
    let d = crate::ir::safe_perl_q_string(&delim);
    let body = &p.content_perl;
    let stmt = format!(
        "my @__o; for my $__l (split(/\\n/, {body}, -1)) {{ if (index($__l, {d}) >= 0) {{ my @__f = split(/\\Q{delim}\\E/, $__l, -1); push @__o, @__f >= {field} ? $__f[{field} - 1] : ''; }} else {{ push @__o, $__l; }} }} print join(\"\\n\", @__o), (@__o ? \"\\n\" : \"\"); $main_exit_code = $CHILD_ERROR = 0;"
    );
    Some(NativeEmit::Stmt(stmt))
}

/// `[-d D] [-f N]` (detached or attached) → (delim, field). Anything else
/// — a list/range field spec, `-s`/`-o`, a second flag, a FILE argument —
/// refuses.
fn parse_args(words: &[String]) -> Option<(String, usize)> {
    let mut delim: Option<String> = None;
    let mut field: Option<usize> = None;
    let mut it = words.iter();
    while let Some(w) = it.next() {
        if w == "-d" {
            if delim.is_some() {
                return None;
            }
            delim = Some(it.next()?.clone());
        } else if let Some(rest) = w.strip_prefix("-d") {
            if delim.is_some() || rest.is_empty() {
                return None;
            }
            delim = Some(rest.to_string());
        } else if w == "-f" {
            if field.is_some() {
                return None;
            }
            field = Some(it.next()?.parse().ok()?);
        } else if let Some(rest) = w.strip_prefix("-f") {
            if field.is_some() || rest.is_empty() {
                return None;
            }
            field = Some(rest.parse().ok()?);
        } else {
            // -s / -o / ranges smuggled through / FILE args / unknowns
            return None;
        }
    }
    // A single-char delimiter that survives being spliced into both a
    // perl string literal and a `\Q…\E` regex; `/`, `\` and the line
    // terminators would need escaping we refuse to guess at.
    let d = delim?;
    if d.chars().count() != 1
        || d.chars().any(|c| matches!(c, '/' | '\\' | '\n' | '\r'))
    {
        return None;
    }
    let n = field?;
    if n == 0 {
        return None;
    }
    Some((d, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_corpus_form() {
        assert_eq!(
            parse_args(&words(&["-d", ":", "-f", "1"])),
            Some((":".to_string(), 1))
        );
        assert_eq!(
            parse_args(&words(&["-f1", "-d-"])),
            Some(("-".to_string(), 1))
        );
    }

    #[test]
    fn refuses_lists_ranges_and_extra_flags() {
        assert_eq!(parse_args(&words(&["-d", ":", "-f", "1,3"])), None);
        assert_eq!(parse_args(&words(&["-d", ":", "-f", "2-3"])), None);
        assert_eq!(parse_args(&words(&["-d", ":", "-f", "1", "-s"])), None);
        assert_eq!(parse_args(&words(&["-d", ":", "-f", "1", "file"])), None);
        assert_eq!(parse_args(&words(&["-f", "0", "-d", ":"])), None);
        assert_eq!(parse_args(&words(&["-d", "//", "-f", "1"])), None);
        assert_eq!(parse_args(&words(&[])), None);
    }
}
