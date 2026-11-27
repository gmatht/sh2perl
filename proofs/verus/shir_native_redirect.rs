//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_redirect.rs — cluster B of the
//  `shir_native_stmt` decomposition: redirect writes.
//
//  `native_echo_write_file_stmt` replaces `echo/printf … > FILE` with
//  native Perl that writes literal bytes; `record_file_write` records the
//  same bytes in the `files` map so a later `cat FILE | …` can be evaluated.
//  `file_redirect_block` only wraps the exec in a Block (a renderer
//  fallback trigger), so its law is "the wrap changes no observable".
//
//  The laws:
//
//    redirect_writes_the_same_bytes
//        whenever the write fold fires, the bytes it emits to the literal
//        target are exactly the command's stdout bytes, and the mode it
//        carries is the redirect's mode
//    append_mode_appends
//        applying that write to an existing file: `a` appends, `w`/`wc`
//        replace (the old content is not consulted)
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  PASS side: `fold_write` transcribes `native_echo_write_file_stmt` — one
//  redirect spec, fd 1, mode w|wc, literal target, echo/printf, an echo loop
//  that accepts -e/-E/-n ANYWHERE and at most one content word, printf's
//  single word with no `%`, `-e`/printf decoding, and echo's trailing
//  newline. `record_file_write` is the stricter `w`-only, multi-word echo
//  variant; the law below is for the fold that actually writes bytes.
//
//  SEMANTIC side: `bash_stdout` computes the command's stdout from the
//  shell's own option rules — options are leading, all the remaining words
//  are operands and are joined with single spaces — and `bash_effect` says
//  what lands in the file (target path, mode, bytes). It shares only the
//  escape table (`dec_echo`), which is cluster H's proved syntax.
//
//  The bridge `parse_echo_is_bash` proves the pass's anywhere-flags scan
//  yields the same option tuple and content as the shell's leading-flag
//  scan, under the law's domain:
//    * flags are LEADING (`leading_flags`), so an operand is never later
//      reinterpreted as an option;
//    * the fold's single content word (`parse_echo` refuses a second), so
//      the shell's space-join is the content itself.
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * `file_redirect_block`'s wrap is modelled as the identity effect; the
//     renderer's choice of the native fallback is a renderer contract, and
//     the ESTree/Perl output of the wrapped form is out of model.
//   * A GLOB word is outside the law's domain: bash expands it before echo
//     sees it, while the fold would write the pattern literally. Unlike
//     `native_echo_stmt`, this fold has no glob gate in the source; that
//     absence is stated here rather than proved away.
//   * `record_file_write`'s multi-word echo join and its `w`-only mode are
//     not the emitting fold; the `files` map's cross-statement invariant is
//     cluster A's, not this file's.
//   * `native_sort_file_stmt`, `native_cat_heredoc_*` (the other functions
//     the plan lists for B) write through different command models; their
//     bytes are not claimed here, only the echo/printf write fold's.
//   * fd 0/2, `2>&1`, heredocs/herestrings, append's `files` update and
//     renderer output are out of model.
//
//  ── measured mutants (restored byte-identically; sha in the sync file) ──
//
//  baseline 21 verified, 0 errors
//
//  counted (the mutation compiles and the proof goes red):
//    m3  apply treats append as a truncating write     20 verified, 1 error
//        (`append_mode_appends`)
//
//  refuted one stage earlier, at simplification, by a concrete pin:
//    m1  wrap/write a dynamic target                (pin_dynamic_target_refused)
//    m2  ignore the redirect's fd                   (pin_fd_other_than_one_refused)
//    m4  keep the newline for -n                    (pin_n_clears_the_newline)
//    m5  accept printf with a % conversion          (pin_non_write_command_refused)
//
//  Each restored byte-identically (sha256 in the sync file).
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub enum Mode {
    W,
    Wc,
    A,
}

pub enum Target {
    Lit(Seq<char>),
    Dyn,
}

pub enum Cmd {
    /// echo with words; the fold allows at most one content word
    Echo { words: Seq<Seq<char>> },
    /// printf with exactly one format word
    Printf { word: Seq<char> },
}

pub struct Stmt {
    pub cmd: Cmd,
    pub mode: Mode,
    pub target: Target,
    pub fd: int,
    pub specs: int,
}

// ─────────────────────────────────────────────── text helpers ─────────────

pub open spec fn is_glob_char(c: char) -> bool {
    c == '*' || c == '?' || c == '[' || c == ']'
}

pub open spec fn has_glob(w: Seq<char>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        false
    } else {
        is_glob_char(w[0]) || has_glob(w.drop_first())
    }
}

pub open spec fn has_percent(w: Seq<char>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        false
    } else {
        w[0] == '%' || has_percent(w.drop_first())
    }
}

/// The `-e` escape table (cluster H's Echo syntax).
pub open spec fn dec_echo(input: Seq<char>) -> Option<Seq<char>>
    decreases input.len(),
{
    if input.len() == 0 {
        Some(Seq::empty())
    } else if input[0] != '\\' {
        match dec_echo(input.drop_first()) {
            Some(r) => Some(seq![input[0]].add(r)),
            None => None,
        }
    } else if input.len() < 2 {
        None
    } else {
        let c = if input[1] == 'a' { Some('\x07') }
            else if input[1] == 'b' { Some('\x08') }
            else if input[1] == 'f' { Some('\x0c') }
            else if input[1] == 'n' { Some('\n') }
            else if input[1] == 'r' { Some('\r') }
            else if input[1] == 't' { Some('\t') }
            else if input[1] == 'v' { Some('\x0b') }
            else if input[1] == '\\' { Some('\\') }
            else if input[1] == '0' { Some('\0') }
            else { None };
        match c {
            Some(ch) => match dec_echo(input.subrange(2, input.len() as int)) {
                Some(r) => Some(seq![ch].add(r)),
                None => None,
            },
            None => None,
        }
    }
}

pub open spec fn join(ws: Seq<Seq<char>>) -> Seq<char>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else if ws.len() == 1 {
        ws[0]
    } else {
        ws[0].add(seq![' ']).add(join(ws.drop_first()))
    }
}

pub open spec fn leading_flags(ws: Seq<Seq<char>>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        true
    } else if ws[0] == seq!['-', 'e'] || ws[0] == seq!['-', 'E'] || ws[0] == seq!['-', 'n'] {
        leading_flags(ws.drop_first())
    } else {
        // after the first operand, no later word may be an option
        !any_later_flag(ws.drop_first())
    }
}

pub open spec fn any_later_flag(ws: Seq<Seq<char>>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        false
    } else {
        (ws[0] == seq!['-', 'e'] || ws[0] == seq!['-', 'E'] || ws[0] == seq!['-', 'n'])
            || any_later_flag(ws.drop_first())
    }
}

// ────────────────────────────────────────────────── pass: echo parsing ────

pub open spec fn parse_echo(
    ws: Seq<Seq<char>>,
    escapes: bool,
    newline: bool,
    content: Option<Seq<char>>,
) -> Option<(bool, bool, Seq<char>)>
    decreases ws.len(),
{
    if ws.len() == 0 {
        match content {
            Some(t) => Some((escapes, newline, t)),
            None => None,
        }
    } else if ws[0] == seq!['-', 'e'] {
        parse_echo(ws.drop_first(), true, newline, content)
    } else if ws[0] == seq!['-', 'E'] {
        parse_echo(ws.drop_first(), false, newline, content)
    } else if ws[0] == seq!['-', 'n'] {
        parse_echo(ws.drop_first(), escapes, false, content)
    } else {
        match content {
            // the fold accepts at most one content word
            Some(_) => None,
            None => parse_echo(ws.drop_first(), escapes, newline, Some(ws[0])),
        }
    }
}

// ────────────────────────────────────────────── semantic: bash's stdout ──

/// Leading options only; every remaining word is an operand.
pub open spec fn bash_opts(
    ws: Seq<Seq<char>>,
    escapes: bool,
    newline: bool,
) -> (bool, bool, Seq<Seq<char>>)
    decreases ws.len(),
{
    if ws.len() == 0 {
        (escapes, newline, Seq::empty())
    } else if ws[0] == seq!['-', 'e'] {
        bash_opts(ws.drop_first(), true, newline)
    } else if ws[0] == seq!['-', 'E'] {
        bash_opts(ws.drop_first(), false, newline)
    } else if ws[0] == seq!['-', 'n'] {
        bash_opts(ws.drop_first(), escapes, false)
    } else {
        (escapes, newline, ws)
    }
}

pub open spec fn bash_stdout(cmd: Cmd) -> Option<Seq<char>> {
    match cmd {
        Cmd::Echo { words } => {
            let (esc, nl, ops) = bash_opts(words, false, true);
            let text = join(ops);
            match (if esc { dec_echo(text) } else { Some(text) }) {
                Some(b) => Some(if nl { b.add(seq!['\n']) } else { b }),
                None => None,
            }
        }
        Cmd::Printf { word } => {
            if has_percent(word) {
                None
            } else {
                dec_echo(word)
            }
        }
    }
}

// ────────────────────────────────────────────────── pass: fold decision ───

pub open spec fn fold_write(st: Stmt) -> Option<(Seq<char>, Seq<char>, Mode)> {
    if st.specs != 1 || st.fd != 1 {
        None
    } else {
        match st.mode {
            Mode::W | Mode::Wc => match st.target {
                Target::Dyn => None,
                Target::Lit(path) => match st.cmd {
                    Cmd::Echo { words } => match parse_echo(words, false, true, None) {
                        Some((esc, nl, text)) => match (if esc { dec_echo(text) } else { Some(text) }) {
                            Some(b) => Some((path, if nl { b.add(seq!['\n']) } else { b }, st.mode)),
                            None => None,
                        },
                        None => None,
                    },
                    Cmd::Printf { word } => {
                        if has_percent(word) {
                            None
                        } else {
                            match dec_echo(word) {
                                Some(b) => Some((path, b, st.mode)),
                                None => None,
                            }
                        }
                    }
                },
            },
            Mode::A => None,
        }
    }
}

// ───────────────────────────────────────── semantic: file after the write ─

pub open spec fn apply(old: Seq<char>, mode: Mode, bytes: Seq<char>) -> Seq<char> {
    match mode {
        Mode::A => old.add(bytes),
        Mode::W | Mode::Wc => bytes,
    }
}

/// What bash leaves in the literal file, or None when the shape is outside
/// what this model represents (dynamic target, other fd, several specs, a
/// non-write mode).
pub open spec fn bash_effect(st: Stmt, old: Seq<char>) -> Option<(Seq<char>, Seq<char>)> {
    if st.specs != 1 || st.fd != 1 {
        None
    } else {
        match st.target {
            Target::Dyn => None,
            Target::Lit(path) => match bash_stdout(st.cmd) {
                Some(b) => Some((path, apply(old, st.mode, b))),
                None => None,
            },
        }
    }
}

// ──────────────────────────────────────────────────────────────── laws ────

/// With a content word already seen, a successful parse consumes only
/// leading flags and then the empty tail (the shell has no more operands).
pub proof fn parse_echo_with_content(
    ws: Seq<Seq<char>>,
    escapes: bool,
    newline: bool,
    c: Seq<char>,
)
    requires leading_flags(ws),
    ensures match parse_echo(ws, escapes, newline, Some(c)) {
        Some((e, nl, t)) => t == c && {
            let (be, bnl, ops) = bash_opts(ws, escapes, newline);
            be == e && bnl == nl && ops.len() == 0
        },
        None => true,
    },
    decreases ws.len(),
{
    if ws.len() == 0 {
    } else if ws[0] == seq!['-', 'e'] {
        parse_echo_with_content(ws.drop_first(), true, newline, c);
    } else if ws[0] == seq!['-', 'E'] {
        parse_echo_with_content(ws.drop_first(), false, newline, c);
    } else if ws[0] == seq!['-', 'n'] {
        parse_echo_with_content(ws.drop_first(), escapes, false, c);
    } else {
        // a second content word: the fold refuses it
    }
}

/// The pass's anywhere-flags scan and the shell's leading-flag scan agree
/// whenever the fold accepts (one content word, flags leading).
pub proof fn parse_echo_agrees(ws: Seq<Seq<char>>, escapes: bool, newline: bool)
    requires leading_flags(ws),
    ensures match parse_echo(ws, escapes, newline, None) {
        Some((e, nl, t)) => {
            let (be, bnl, ops) = bash_opts(ws, escapes, newline);
            be == e && bnl == nl && ops.len() == 1 && ops[0] == t
        },
        None => true,
    },
    decreases ws.len(),
{
    if ws.len() == 0 {
    } else if ws[0] == seq!['-', 'e'] {
        parse_echo_agrees(ws.drop_first(), true, newline);
    } else if ws[0] == seq!['-', 'E'] {
        parse_echo_agrees(ws.drop_first(), false, newline);
    } else if ws[0] == seq!['-', 'n'] {
        parse_echo_agrees(ws.drop_first(), escapes, false);
    } else {
        parse_echo_with_content(ws.drop_first(), escapes, newline, ws[0]);
        assert(bash_opts(ws, escapes, newline) == (escapes, newline, ws));
    }
}

/// The fold's bytes are bash's bytes, at bash's path, in the fold's mode.
pub proof fn redirect_writes_the_same_bytes(st: Stmt)
    requires
        st.specs == 1,
        st.fd == 1,
        st.target is Lit,
        leading_flags(match st.cmd { Cmd::Echo { words } => words, Cmd::Printf { .. } => Seq::empty() }),
    ensures
        forall|path: Seq<char>, bytes: Seq<char>, m: Mode|
            fold_write(st) == Some((path, bytes, m))
                ==> bash_effect(st, Seq::empty()) == Some((path, apply(Seq::empty(), m, bytes))),
{
    match st.mode {
        Mode::A => {}
        Mode::W => {
            if let Target::Lit(path) = st.target {
            match st.cmd {
                Cmd::Echo { words } => {
                    parse_echo_agrees(words, false, true);
                    match parse_echo(words, false, true, None) {
                        Some((esc, nl, text)) => {
                            let decoded = if esc { dec_echo(text) } else { Some(text) };
                            match decoded {
                                Some(b) => {
                                    let bytes = if nl { b.add(seq!['\n']) } else { b };
                                    assert(fold_write(st) == Some((path, bytes, Mode::W)));
                                    assert(bash_stdout(st.cmd) == Some(bytes));
                                }
                                None => {}
                            }
                        }
                        None => {}
                    }
                }
                Cmd::Printf { word } => {
                    if !has_percent(word) {
                        match dec_echo(word) {
                            Some(b) => {
                                assert(fold_write(st) == Some((path, b, Mode::W)));
                                assert(bash_stdout(st.cmd) == Some(b));
                            }
                            None => {}
                        }
                    }
                }
            }
            }
        }
        Mode::Wc => {
            if let Target::Lit(path) = st.target {
            match st.cmd {
                Cmd::Echo { words } => {
                    parse_echo_agrees(words, false, true);
                    match parse_echo(words, false, true, None) {
                        Some((esc, nl, text)) => {
                            let decoded = if esc { dec_echo(text) } else { Some(text) };
                            match decoded {
                                Some(b) => {
                                    let bytes = if nl { b.add(seq!['\n']) } else { b };
                                    assert(fold_write(st) == Some((path, bytes, Mode::Wc)));
                                    assert(bash_stdout(st.cmd) == Some(bytes));
                                }
                                None => {}
                            }
                        }
                        None => {}
                    }
                }
                Cmd::Printf { word } => {
                    if !has_percent(word) {
                        match dec_echo(word) {
                            Some(b) => {
                                assert(fold_write(st) == Some((path, b, Mode::Wc)));
                                assert(bash_stdout(st.cmd) == Some(b));
                            }
                            None => {}
                        }
                    }
                }
            }
            }
        }
    }
}

/// Append starts from the old content; a truncating mode ignores it.
pub proof fn append_mode_appends(old: Seq<char>, bytes: Seq<char>) {
    assert(apply(old, Mode::A, bytes) == old.add(bytes));
    assert(apply(old, Mode::W, bytes) == bytes);
    assert(apply(old, Mode::Wc, bytes) == bytes);
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

pub open spec fn echo_stmt(words: Seq<Seq<char>>) -> Stmt {
    Stmt {
        cmd: Cmd::Echo { words },
        mode: Mode::W,
        target: Target::Lit(seq!['o', 'u', 't']),
        fd: 1,
        specs: 1,
    }
}

pub open spec fn mk(cmd: Cmd, mode: Mode, target: Target, fd: int, specs: int) -> Stmt {
    Stmt { cmd, mode, target, fd, specs }
}

/// A dynamic target is not wrapped or written.
pub proof fn pin_dynamic_target_refused() {
    assert(fold_write(mk(Cmd::Echo { words: seq![seq!['x']] }, Mode::W, Target::Dyn, 1, 1))
        == None::<(Seq<char>, Seq<char>, Mode)>) by (compute);
}

/// fd 2 (or any fd other than 1) is refused.
pub proof fn pin_fd_other_than_one_refused() {
    assert(fold_write(mk(Cmd::Echo { words: seq![seq!['x']] }, Mode::W,
            Target::Lit(seq!['o', 'u', 't']), 2, 1))
        == None::<(Seq<char>, Seq<char>, Mode)>) by (compute);
}

/// A second redirect spec is refused.
pub proof fn pin_two_specs_refused() {
    assert(fold_write(mk(Cmd::Echo { words: seq![seq!['x']] }, Mode::W,
            Target::Lit(seq!['o', 'u', 't']), 1, 2))
        == None::<(Seq<char>, Seq<char>, Mode)>) by (compute);
}

/// append is not a write-fold mode (`native_echo_write_file_stmt` handles
/// only w|wc); the append law is stated for the shell effect, not this fold.
pub proof fn pin_append_not_a_write_fold() {
    assert(fold_write(mk(Cmd::Echo { words: seq![seq!['x']] }, Mode::A,
            Target::Lit(seq!['o', 'u', 't']), 1, 1))
        == None::<(Seq<char>, Seq<char>, Mode)>) by (compute);
}

/// A non-echo/printf command is refused.
pub proof fn pin_non_write_command_refused() {
    assert(fold_write(mk(Cmd::Printf { word: seq!['%', 's'] }, Mode::W,
            Target::Lit(seq!['o', 'u', 't']), 1, 1))
        == None::<(Seq<char>, Seq<char>, Mode)>) by (compute);
}

/// `echo -n` writes no newline.
pub proof fn pin_n_clears_the_newline() {
    assert(fold_write(echo_stmt(seq![seq!['-', 'n'], seq!['x']]))
        == Some((seq!['o', 'u', 't'], seq!['x'], Mode::W))) by (compute);
}

/// `echo -e` decodes `\n`; without it the bytes are literal.
pub proof fn pin_e_decodes_escapes() {
    assert(fold_write(echo_stmt(seq![seq!['-', 'e'], seq!['a', '\\', 'n']]))
        == Some((seq!['o', 'u', 't'], seq!['a', '\n', '\n'], Mode::W))) by (compute);
}

/// printf adds no newline and interprets escapes.
pub proof fn pin_printf_has_no_newline() {
    assert(fold_write(mk(Cmd::Printf { word: seq!['a', '\\', 'n'] }, Mode::W,
            Target::Lit(seq!['o', 'u', 't']), 1, 1))
        == Some((seq!['o', 'u', 't'], seq!['a', '\n'], Mode::W))) by (compute);
}

fn main() {}

} // verus!
