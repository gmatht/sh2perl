//! text-filter-lift — a pipeline of STATIC text filters as one node.
//!
//! `echo -e "a\nb" | uniq`, `seq 1 10 | head -3` and `echo "x:y" | cut -d: -f2`
//! are rendered today as `__sh_spawn("echo 'x' | …")`
//! — a `bash -c` for text that `std` reproduces exactly. This module lifts the
//! whole pipeline into one `TextFilter` node (`src/shir_nodes/text_filter.node`)
//! when EVERY stage is statically known: a literal producer (`echo`, `printf`,
//! `seq`, `cat PATH`) followed by the small filters whose semantics the node
//! pins (`wc`, `head`, `tail`, `uniq`, `cut -d/-f`).
//!
//! ## The sound rule
//!
//! * Every stage is exactly one command with literal argv words — no expansion,
//!   no redirect inside the pipeline, no `;`, no subshell stage.
//! * The producer is a LITERAL (`echo [-e|-n] words`, `printf` with a format
//!   that has no `%`, `seq A B`) or a single-path `cat` — the only file read
//!   whose error text this lift can spell (`cat: PATH: <strerror>`).
//! * A filter is one of the exact shapes below; every other flag/operand keeps
//!   the pipeline spawning, because a wrong fold is worse than a spawn.
//! * The pipeline's status is the LAST stage's, and every lifted shape's last
//!   stage succeeds, so the node ends at status 0. A failing `cat` READ still
//!   prints `cat: PATH: <strerror>` (as the pipe would) and leaves the text
//!   empty — exactly what the following filter sees in bash.
//!
//! ## Refused on purpose (each keeps the spawn and has a refusal witness)
//!
//! * `head`/`tail`/`cat` with MORE THAN ONE operand: the multi-file form prints
//!   `==> name <==` headers between files (`cat` concatenates silently, which is
//!   a different node).
//! * `tail -f`, `tail -N +M`, `head -c`, `head -q/-v`.
//! * `cut -c` (character positions, multibyte-aware), `cut --complement`,
//!   `cut` with a RANGE field list (`1-3`), a multi-byte or multi-character
//!   delimiter.
//! * `wc -L` (display width — locale and combining-mark dependent), `wc` with
//!   several operands (it prints a total line), `wc` with a file operand (the
//!   operand makes it print aligned columns only in the multi-file form, but a
//!   read error's text is the command's own).
//! * `uniq -c/-d/-u/-i` and `uniq` with a file operand.
//! * `tee` in EVERY form: its write-error text is the ENVIRONMENT's (measured:
//!   this box's `tee` prints `<path>: No such file or directory (os error 2)`,
//!   GNU prints `tee: <path>: …`), so no one message is byte-exact; the
//!   success path alone cannot buy the fold.
//! * `cat` with no operand or `-`.
//! * any interpreter or collating filter (`sort`, `grep`, `awk`, `perl`,
//!   `xargs`, `sed`, `tr`, `od`, `bc`) — the plan keeps those out.
//!
//! ## Where it runs
//!
//! Registered as `("text-filter-lift", transform)`: it walks the statement tree
//! (bodies AND conditions, like `fs-lift`) and answers the backend manifest, so
//! only a target that declares `TextFilter` ever receives the node.

use crate::ir::{IrExpr, IrStmt, StrStyle};

/// The node tag this lift produces (the manifest gate must carry it).
pub const TAGS: &[&str] = &["TextFilter"];

fn offered() -> bool {
    let enabled = std::env::var("SH2_TRANSFORMS").unwrap_or_default();
    if enabled.split(',').any(|s| s.trim() == "text-filter-lift") {
        return true;
    }
    // An UNSET target means NO: only the Rust manifest declares this node, and
    // a lift taken for a backend with no arm would hand it a node it cannot
    // render.
    match crate::transforms::target() {
        Some(t) => crate::transforms::planner::node_manifest(&t).contains(&TAGS[0]),
        None => false,
    }
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if !offered() {
        return false;
    }
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= walk_stmt(st);
    }
    changed
}

fn walk_stmts(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= walk_stmt(s);
    }
    c
}

fn walk_stmt(st: &mut IrStmt) -> bool {
    // The lift itself: a whole pipeline statement becomes one node expression.
    if let Some(new) = lift_stmt(st) {
        *st = new;
        return true;
    }
    match st {
        IrStmt::If { cond: _, then, elsifs, else_ } => {
            let mut c = walk_stmts(then);
            for (_, b) in elsifs.iter_mut() {
                c |= walk_stmts(b);
            }
            c |= walk_stmts(else_);
            c
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => walk_stmts(body),
        IrStmt::For { body, .. } => walk_stmts(body),
        IrStmt::ForInit { init, body, .. } => {
            let a = walk_stmts(init);
            let b = walk_stmts(body);
            a || b
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => walk_stmts(b),
        IrStmt::Redirect { inner, .. } => walk_stmts(inner),
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = walk_stmts(body);
            for (_, b) in named_blocks.iter_mut() {
                c |= walk_stmts(b);
            }
            c
        }
        IrStmt::Case { clauses, .. } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                c |= walk_stmts(&mut cl.body);
            }
            c
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut c = walk_stmts(body);
            for ex in excepts.iter_mut() {
                c |= walk_stmts(&mut ex.body);
            }
            c |= walk_stmts(else_body);
            c |= walk_stmts(finally_body);
            c
        }
        _ => false,
    }
}

fn lift_stmt(st: &IrStmt) -> Option<IrStmt> {
    let stages = match st {
        // A pipeline statement (`printf … | wc -l`) — as long as it is not a
        // `$(…)` capture, whose value convention this node does not carry.
        IrStmt::Pipeline { stages, capture: None, .. } => stages,
        // The expression form (`echo a | uniq`), one Arrow per stage.
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
            let arr = match args.first() {
                Some(IrExpr::Array(items)) => items,
                _ => return None,
            };
            let mut stages: Vec<Vec<IrStmt>> = Vec::with_capacity(arr.len());
            for a in arr {
                match a {
                    IrExpr::Arrow(body) => stages.push(body.clone()),
                    _ => return None,
                }
            }
            return finish(&stages);
        }
        // `cmd … < FILE` — the redirect IS the producer: one static path read.
        IrStmt::Redirect { inner, redirects } if redirects.len() == 1 => {
            let r = &redirects[0];
            if r.fd.unwrap_or(0) != 0 || r.mode != "r" {
                return None;
            }
            let IrExpr::Str(path, _) = &r.target else {
                return None;
            };
            if !op_path_ok(path) {
                return None;
            }
            let words = argv(inner)?;
            if words.is_empty() {
                return None;
            }
            let op = filter(&words[0], &words[1..])?;
            return Some(IrStmt::Expr(IrExpr::Ext(Box::new(
                crate::shir_nodes::TextFilter {
                    src: IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
                    ops: format!("cat:{path};{op}"),
                },
            ))));
        }
        _ => return None,
    };
    finish(stages)
}

/// Turn the stage list into the node statement, or `None` if any stage is not
/// one of the exact shapes.
fn finish(stages: &[Vec<IrStmt>]) -> Option<IrStmt> {
    if stages.len() < 2 {
        return None;
    }
    let mut ops: Vec<String> = Vec::new();
    let mut src = String::new();
    for (i, stage) in stages.iter().enumerate() {
        let words = argv(stage)?;
        if words.is_empty() {
            return None;
        }
        let cmd = words[0].as_str();
        let args = &words[1..];
        if i == 0 {
            // Producer: literal text and/or the one file read this lift spells.
            match producer(cmd, args)? {
                Prod::Text(t) => src = t,
                Prod::Read(p) => {
                    if !op_path_ok(&p) {
                        return None;
                    }
                    ops.push(format!("cat:{p}"));
                }
            }
            continue;
        }
        ops.push(filter(cmd, args)?);
    }
    if ops.is_empty() {
        return None;
    }
    Some(IrStmt::Expr(IrExpr::Ext(Box::new(
        crate::shir_nodes::TextFilter {
            src: IrExpr::Str(src, StrStyle::DoubleQuoted),
            ops: ops.join(";"),
        },
    ))))
}

enum Prod {
    Text(String),
    Read(String),
}

/// The argv of a single-command stage, or `None` when the stage is anything
/// but `builtin|exec(cmd, [static words…])` with one command in it.
fn argv(stage: &[IrStmt]) -> Option<Vec<String>> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stage else {
        return None;
    };
    if func != "builtin" && func != "exec" {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(items)] = args.as_slice() else {
        return None;
    };
    let mut words = vec![cmd.clone()];
    for it in items {
        match it {
            IrExpr::Str(w, _) => words.push(w.clone()),
            _ => return None,
        }
    }
    Some(words)
}

/// A path or delimiter that can ride the `;`/`:`-separated op list unchanged.
fn op_path_ok(s: &str) -> bool {
    !s.is_empty() && !s.contains(';')
}

/// Text escapes `echo -e` / `printf` interpret: `\n \t \r \\`. Any OTHER
/// backslash escape is refused (its byte value is a C-quoting question).
fn unescape(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            _ => return None,
        }
    }
    Some(out)
}

fn producer(cmd: &str, args: &[String]) -> Option<Prod> {
    match cmd {
        // `echo [-e] [-n] words…`
        "echo" => {
            let mut interp = false;
            let mut nl = true;
            let mut rest: &[String] = args;
            loop {
                match rest.first().map(|s| s.as_str()) {
                    Some("-e") => {
                        interp = true;
                        rest = &rest[1..];
                    }
                    Some("-n") => {
                        nl = false;
                        rest = &rest[1..];
                    }
                    Some("-E") => rest = &rest[1..],
                    _ => break,
                }
            }
            let mut text = String::new();
            for (i, w) in rest.iter().enumerate() {
                if i > 0 {
                    text.push(' ');
                }
                match (interp, unescape(w)) {
                    (true, Some(t)) => text.push_str(&t),
                    (true, None) => return None,
                    (false, _) => text.push_str(w),
                }
            }
            if nl {
                text.push('\n');
            }
            Some(Prod::Text(text))
        }
        // `printf FORMAT` — a format with no `%` and no unknown escape.
        "printf" => {
            let fmt = args.first()?;
            if fmt.contains('%') || args.len() != 1 {
                return None;
            }
            Some(Prod::Text(unescape(fmt)?))
        }
        // `seq A B` / `seq N` — integers only.
        "seq" => {
            let (a, b) = match args {
                [n] => (1i64, n.parse().ok()?),
                [a, b] => (a.parse().ok()?, b.parse().ok()?),
                _ => return None,
            };
            if b < a {
                return None;
            }
            let mut text = String::new();
            for i in a..=b {
                text.push_str(&i.to_string());
                text.push('\n');
            }
            Some(Prod::Text(text))
        }
        // `cat PATH` — the one read whose error text the node spells.
        "cat" => match args {
            [p] if op_path_ok(p) && !p.starts_with('-') => Some(Prod::Read(p.clone())),
            _ => None,
        },
        _ => None,
    }
}

fn filter(cmd: &str, args: &[String]) -> Option<String> {
    match cmd {
        // `wc -l|-w|-c` with no operand (the operand forms print a total line
        // and carry their own read-error text).
        "wc" => match args {
            [f] => match f.as_str() {
                "-l" => Some("wc:l".to_string()),
                "-w" => Some("wc:w".to_string()),
                "-c" => Some("wc:c".to_string()),
                _ => None,
            },
            _ => None,
        },
        // `head -N` / `head -n N`, N a positive integer; no `-c/-q/-v`, and a
        // second operand (the multi-file header form) is refused.
        "head" | "tail" => {
            let n: i64 = match args {
                [a] => match a.strip_prefix('-') {
                    Some(num) => num.parse().ok()?,
                    None => return None,
                },
                [flag, num] if flag == "-n" => match num.strip_prefix('+') {
                    Some(_) => return None,
                    None => num.parse().ok()?,
                },
                _ => return None,
            };
            if n < 1 {
                return None;
            }
            Some(format!("{}:{n}", if cmd == "head" { "head" } else { "tail" }))
        }
        // `uniq` with no flag and no operand.
        "uniq" => match args {
            [] => Some("uniq".to_string()),
            _ => None,
        },
        // `cut -d D -f LIST` — one delimiter CHARACTER, a comma list of
        // positive field numbers, no `-c`, no ranges, no `--complement`.
        "cut" => {
            let mut delim: Option<String> = None;
            let mut fields: Option<String> = None;
            let mut it = args.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "-d" => delim = Some(it.next()?.clone()),
                    "-f" => {
                        let f = it.next()?;
                        for part in f.split(',') {
                            let n: usize = part.parse().ok()?;
                            if n == 0 {
                                return None;
                            }
                        }
                        fields = Some(f.clone());
                    }
                    _ => return None,
                }
            }
            let d = delim?;
            if d.chars().count() != 1 || d.contains(';') {
                return None;
            }
            Some(format!("cut:{d}:{}", fields?))
        }
        // `tee` is REFUSED, and the reason is a measurement, not a taste:
        // its write-error text is the ENVIRONMENT's, not the shell's. On this
        // box `echo x | tee /nope/dir/f` prints
        //   /nope/dir/f: No such file or directory (os error 2)
        // (a Rust-backed `tee`), while GNU tee prints `tee: <path>: …`. A node
        // that hard-codes either one is silently wrong on the other, so the
        // success path alone cannot buy the fold. The pipeline keeps spawning.
        "tee" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(cmd: &str, words: &[&str]) -> Vec<IrStmt> {
        let mut arr = vec![IrExpr::Str(cmd.to_string(), StrStyle::DoubleQuoted)];
        for w in words {
            arr.push(IrExpr::Str(w.to_string(), StrStyle::DoubleQuoted));
        }
        vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str(cmd.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(arr[1..].to_vec()),
            ],
        })]
    }

    fn lift(stages: &[Vec<IrStmt>]) -> Option<String> {
        crate::transforms::set_target(Some("rs"));
        let r = finish(stages);
        crate::transforms::set_target(None);
        match r {
            Some(IrStmt::Expr(IrExpr::Ext(n))) => Some(
                n.as_any()
                    .downcast_ref::<crate::shir_nodes::TextFilter>()
                    .expect("TextFilter")
                    .ops
                    .clone(),
            ),
            _ => None,
        }
    }

    #[test]
    fn static_literal_pipelines_lift() {
        assert_eq!(
            lift(&[st("echo", &["hi"]), st("wc", &["-l"])]).as_deref(),
            Some("wc:l")
        );
        assert_eq!(
            lift(&[st("seq", &["1", "10"]), st("head", &["-3"])]).as_deref(),
            Some("head:3")
        );
        assert_eq!(
            lift(&[st("echo", &["a:b"]), st("cut", &["-d", ":", "-f", "2"])]).as_deref(),
            Some("cut:::2")
        );
        assert_eq!(
            lift(&[st("cat", &["f"]), st("uniq", &[])]).as_deref(),
            Some("cat:f;uniq")
        );
    }

    #[test]
    fn refusals_keep_the_spawn() {
        for stages in [
            // interpreters
            vec![st("echo", &["hi"]), st("awk", &["{print $1}"])],
            // multi-file head (header form)
            vec![st("seq", &["1", "3"]), st("head", &["-1", "a", "b"])],
            // tail -f / +N
            vec![st("seq", &["1", "3"]), st("tail", &["-f"])],
            vec![st("seq", &["1", "3"]), st("tail", &["-n", "+2"])],
            // cut -c, ranges, multi-byte delimiter
            vec![st("echo", &["ab"]), st("cut", &["-c", "1"])],
            vec![st("echo", &["a:b:c"]), st("cut", &["-d", ":", "-f", "1-2"])],
            // wc -L and the operand form
            vec![st("echo", &["hi"]), st("wc", &["-L"])],
            vec![st("echo", &["hi"]), st("wc", &["-l", "f"])],
            // uniq flags
            vec![st("echo", &["a"]), st("uniq", &["-c"])],
            // tee (any form): its error text is the environment's
            vec![st("echo", &["a"]), st("tee", &["-a", "f"])],
            vec![st("echo", &["a"]), st("tee", &["f"])],
            // cat with no operand
            vec![st("cat", &[]), st("wc", &["-l"])],
            // a dynamic producer
            vec![
                vec![IrStmt::Expr(IrExpr::Call {
                    func: "builtin".to_string(),
                    args: vec![
                        IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                        IrExpr::Array(vec![IrExpr::Call {
                            func: "getVar".to_string(),
                            args: vec![IrExpr::Str("x".to_string(), StrStyle::DoubleQuoted)],
                        }]),
                    ],
                })],
                st("wc", &["-l"]),
            ],
        ] {
            assert!(lift(&stages).is_none(), "lifted a shape it must refuse");
        }
    }
}
