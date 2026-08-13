//! Canonicalization of simple-command arguments — the layer between the raw
//! lexer and the AST/shIR that backends consume.
//!
//! Combined short flags (`-rf`, `-la`) are normalized getopt-style into one
//! flag per argument (`-r -f`, `-l -a`) for a whitelist of flag-taking
//! commands, so the AST is canonical and no generator ever needs to know
//! combined forms — and `rm -rf x` vs `rm -r f x` stay distinguishable in the
//! AST (`-f` vs a literal `f` file argument).
//!
//! Why a whitelist? Splitting is only safe when the command uses getopt-style
//! short options. NOT splitting is always safe (the command receives the word
//! exactly as bash passes it), so commands not in the table — `echo -rf`,
//! `printf`, `cat`, ... — keep combined words untouched. Only commands whose
//! flags are known to be single-character getopt options belong here.
//!
//! Value-taking flags (e.g. `grep -A2`, `head -n5`) split into `-A` + `2`
//! (getopt semantics: the remainder of the word is the flag's value). The
//! `COMMAND_FLAG_VALUE_CHARS` table encodes which single-char flags take a
//! value per command; keeping a flag out of the value set (or the whole
//! command out of the table) is the conservative choice.
//!
//! Rules honoured:
//! - only the argument run BEFORE a `--` terminator is normalized;
//! - a lone `-` (stdin marker) is never touched;
//! - only pure literal words are split (interpolations pass through);
//! - the command name (args[0] of the command) is never touched.

use std::collections::HashMap;

use crate::ast::Word;

/// Single-char flags that take a VALUE for each whitelisted command
/// (getopt-style). A flag NOT listed here is treated as boolean.
fn command_flag_value_chars() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // boolean-only short flags
    for cmd in [
        "rm", "wc", "comm", "chmod", "chown", "cp", "mv", "ln", "rmdir", "tee", "diff",
    ] {
        m.insert(cmd, "");
    }
    m.insert("ls", "w"); // -w width
    m.insert("set", "o"); // -o option (e.g. `set -euo pipefail`)
    m.insert("uniq", "fs"); // -f skip fields, -s skip chars
    m.insert("head", "nc"); // -n lines, -c bytes
    m.insert("tail", "nc"); // -n lines, -c bytes
    m.insert("sort", "kto"); // -k key, -t separator, -o output
    m.insert("cut", "dfc"); // -d delimiter, -f fields, -c chars
    m.insert("grep", "efmABC"); // -e pattern, -f file, -m max, -A/-B/-C context
    m.insert("egrep", "efmABC");
    m.insert("fgrep", "efmABC");
    m.insert("cmp", "n"); // -n bytes
    m.insert("xargs", "n"); // -n max args
    m.insert("mkdir", "m"); // -m mode
    m.insert("touch", "dtr"); // -d/-t time, -r reference
    m
}

/// Normalize combined short flags in `args` (the arguments AFTER the command
/// name) for whitelisted commands. `name` is the command name.
pub fn normalize_combined_flags(name: &str, args: &mut Vec<Word>) {
    let value_flags = match command_flag_value_chars().get(name) {
        Some(vf) => vf.chars().collect::<Vec<char>>(),
        None => return, // not a flag-taking command: keep words as bash passes them
    };

    let mut out = Vec::with_capacity(args.len());
    let mut after_dashdash = false;
    for arg in args.iter() {
        let Word::Literal(s, _) = arg else {
            out.push(arg.clone());
            continue;
        };
        if after_dashdash {
            out.push(arg.clone());
            continue;
        }
        // Long options (`--no-run-if-empty`) and the `--` terminator are never
        // split; only `--` (exactly) switches to literal-arg mode.
        if s.starts_with("--") {
            if s == "--" {
                after_dashdash = true;
            }
            out.push(arg.clone());
            continue;
        }
        if s == "-" || !s.starts_with('-') || s.len() < 2 {
            out.push(arg.clone());
            continue;
        }
        // getopt-style split of "-xy..." → "-x" "-y" ...
        let chars: Vec<char> = s.chars().skip(1).collect();
        // `-10`, `-5` … are positional shorthands (head -10 = 10 lines), not
        // combined flags — never split a purely numeric argument.
        if !chars.is_empty() && chars.iter().all(|c| c.is_ascii_digit()) {
            out.push(arg.clone());
            continue;
        }
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if value_flags.contains(&c) {
                // value flag: "-c" plus the remainder of the word as its value
                out.push(Word::Literal(format!("-{c}"), None));
                if i + 1 < chars.len() {
                    let val: String = chars[i + 1..].iter().collect();
                    out.push(Word::Literal(val, None));
                }
                i = chars.len();
            } else {
                out.push(Word::Literal(format!("-{c}"), None));
                i += 1;
            }
        }
    }
    *args = out;
}
