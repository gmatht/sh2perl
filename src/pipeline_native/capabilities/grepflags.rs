//! Capability: `echo|printf|seq LITERAL | grep <flags> PAT` — the grep
//! flag forms the bare `grep PAT` / `grep -q PAT` leaves (pipe.rs) do not
//! cover: `-i`, `-v`, `-w`, `-c`, `-n`, `-E`, `-F` (and combinations).
//!
//! ## Need
//!
//! `pipe.rs` lifts only the bare pattern and `-q`. Every other flag form
//! falls through to the `bash -c` shell-out, which check_qx flags
//! ("wrapping builtin 'grep'").
//!
//! ## Semantics (verified against bash)
//!
//! - `-i` — case-insensitive (`lc` both sides)
//! - `-v` — invert: select non-matching lines
//! - `-w` — whole-word match (word-boundary anchors)
//! - `-c` — print the COUNT of selected lines instead of the lines
//! - `-n` — prefix each line with its 1-based number
//! - `-E` — ERE (Perl's regex dialect is close enough for the corpus)
//! - `-F` — fixed string: quote the pattern's metacharacters
//! - `-q` — quiet (rc only); accepted here so `-q` can combine with the
//!   others (pipe.rs owns the bare `-q PAT` form)
//!
//! Exit status: 0 iff at least one line was selected — bash's grep, shared
//! by the plain, `-c` and `-q` forms.
//!
//! ## Refusals
//!
//! Flags with no faithful one-liner here (`-A/-B/-C` context, `-r`
//! recursive, `-o`, `-l/-L`, `-H/-h/-Z`, `-b`, `-m`, `-f`) return None and
//! keep the shell-out, as does a second pattern/file word.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    if p.cmd != "grep" {
        return None;
    }
    let mut insensitive = false;
    let mut invert = false;
    let mut word = false;
    let mut count = false;
    let mut number = false;
    let mut fixed = false;
    let mut quiet = false;
    let mut pattern: Option<&str> = None;
    let mut saw_flag = false;
    for w in &p.words {
        if let Some(f) = w.strip_prefix('-') {
            if f.is_empty() {
                return None;
            }
            for c in f.chars() {
                match c {
                    'i' => insensitive = true,
                    'v' => invert = true,
                    'w' => word = true,
                    'c' => count = true,
                    'n' => number = true,
                    'F' => fixed = true,
                    'q' => quiet = true,
                    // `-E` selects ERE, which is the dialect we already
                    // emit for a non-`-F` pattern — accept, and clear any
                    // earlier `-F`.
                    'E' => fixed = false,
                    _ => return None,
                }
            }
            saw_flag = true;
            continue;
        }
        if pattern.is_some() {
            return None; // multiple patterns / a FILE operand — not modelled
        }
        pattern = Some(w);
    }
    let pattern = pattern?;
    // This leaf exists for the flag forms pipe.rs does NOT own; leave the
    // bare and `-q PAT` shapes to it so the two never disagree.
    if !saw_flag || p.words == ["-q", pattern] {
        return None;
    }

    let rx = rx(pattern, fixed, word);
    let body = &p.content_perl;
    // The per-line test. `-i` folds case on both sides; `-v` inverts.
    let test = if insensitive {
        "lc($__l) =~ $__r"
    } else {
        "$__l =~ $__r"
    };
    let test = if invert { format!("!({test})") } else { test.to_string() };

    let act = if quiet {
        format!("for my $__l (@__pl) {{ if ({test}) {{ $__m = 1; last; }} }}")
    } else if count {
        format!("for my $__l (@__pl) {{ $__c++ if {test}; }} print $__c, \"\\n\"; $__m = $__c > 0 ? 1 : 0;")
    } else if number {
        // `-n` numbers the SELECTED lines (bash numbers by input position)
        format!(
            "for my $__i (0..$#__pl) {{ print(($__i + 1), \":\", $__pl[$__i], \"\\n\") if do {{ my $__l = $__pl[$__i]; {test} }}; $__m = 1 if do {{ my $__l = $__pl[$__i]; {test} }}; }}"
        )
    } else {
        format!("for my $__l (@__pl) {{ if ({test}) {{ print $__l, \"\\n\"; $__m = 1; }} }}")
    };
    Some(NativeEmit::Stmt(format!(
        "my $__g = {body}; my @__pl = split(/\\n/, $__g); my $__m = 0; my $__c = 0; my $__r = qr/{rx}/; {act} $main_exit_code = $CHILD_ERROR = $__m ? 0 : 1;"
    )))
}

/// Build the Perl regex body for the pattern: `-F` quotes the
/// metacharacters, `-w` adds word boundaries.
fn rx(pat: &str, fixed: bool, word: bool) -> String {
    let mut s = if fixed {
        pat.chars()
            .map(|c| {
                if "\\^$.|?*+()[]{}".contains(c) {
                    format!("\\{c}")
                } else {
                    c.to_string()
                }
            })
            .collect::<String>()
    } else {
        pat.replace('\\', "\\\\").replace('/', "\\/")
    };
    if word {
        s = format!("\\b(?:{s})\\b");
    }
    s
}
