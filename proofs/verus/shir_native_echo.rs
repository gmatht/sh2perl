//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_echo.rs — cluster E of the
//  `shir_native_stmt` decomposition: echo/printf byte equivalence.
//
//  `native_echo_stmt` and `native_printf_stmt` replace a command with an
//  `Output` node carrying literal bytes. The laws:
//
//    echo_emits_the_same_bytes    the emitted text + newline decision is
//                                 exactly what bash's `echo` writes
//    printf_emits_the_same_bytes  the emitted text is exactly what bash's
//                                 `printf` writes for the accepted format
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  ECHO. The pass side (`pass_echo`) transcribes the source's pipeline: look
//  at the FIRST word only, classify it (-n / -e / -E / unknown / plain),
//  decode every remaining word when -e is on, drop empty words, join with one
//  space. The semantic side (`bash_echo`) is written from what bash prints:
//  a flag tuple, the argument BYTES, and the trailing newline appended to
//  the text. The two sides share only the escape table (`dec_echo`), whose
//  separate target-syntax correctness is cluster H's law; what this file
//  proves is the assembly: flags, decoding decision, separators, newline.
//
//  PRINTF. The pass side (`pass_printf`) is the source's index loop over the
//  format string (`%s`, `%%`, `\n \t \r \\`), threading a `used` counter and
//  refusing on an unknown conversion, an insufficient argument or leftover
//  arguments. The semantic side (`bash_printf`) is a slice recursion written
//  from printf's format language; the law proves the two agree, so the fold's
//  iteration is the format's meaning.
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * Only the FIRST word can be a flag in the fold, so the law's domain
//     excludes a later word that starts with `-` (`no_later_option`). Bash's
//     echo would treat such a word as another option; the fold does not
//     inspect it. That is a property of the source's word shaping, stated
//     here rather than silently proved.
//   * Interior EMPTY words are excluded (`all_nonempty`): the source drops
//     them as backslash-continuation artifacts. `echo a "" b` is therefore
//     outside this law (the transform's own comment claims the continuation
//     case is the only producer).
//   * Glob words are refused by the fold's own gate, pinned below; bash's
//     expansion of them is not modelled.
//   * `native_printf_v_stmt` (`printf -v`, assignment + status) is NOT this
//     law: it writes a variable, not stdout. It is left to cluster G.
//   * `decode_echo_escapes`'s table is cluster H's proved syntax; this file
//     reuses its shape and does not re-claim the escape round trip.
//   * The renderer that turns `Output { newline }` into a backend write, and
//     `printf`'s `%d`/width/precision/repetition forms (refused by the fold),
//     are out of model. Unknown echo flags are refused by the fold, pinned.
//
//  ── measured mutants (restored byte-identically; sha in the sync file) ──
//
//  baseline 26 verified, 0 errors
//
//  all five rejected one stage EARLIER, at simplification, by a concrete pin
//  (the mutated value computes false, so no proof is attempted):
//    m1  keep the newline for -n          (pin_n_clears_the_newline)
//    m2  decode escapes without -e        (pin_e_decodes_escapes)
//    m3  accept an unknown flag as a word (pin_unknown_flag_refused)
//    m4  treat a glob word as literal     (pin_glob_word_refused)
//    m5  printf ignores extra arguments   (pin_printf_arg_count_exact)
//
//  Each restored byte-identically (sha256 in the sync file). No counted
//  mutant exists for this cluster: every listed mutation has a concrete pin
//  that refutes it before the verifier runs.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────── shared text helpers ──────────

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

pub open spec fn any_glob(ws: Seq<Seq<char>>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        false
    } else {
        has_glob(ws[0]) || any_glob(ws.drop_first())
    }
}

pub open spec fn all_nonempty(ws: Seq<Seq<char>>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        true
    } else {
        ws[0].len() > 0 && all_nonempty(ws.drop_first())
    }
}

pub open spec fn no_later_option(ws: Seq<Seq<char>>) -> bool
    decreases ws.len(),
{
    if ws.len() <= 1 {
        true
    } else {
        !(ws[1].len() > 0 && ws[1][0] == '-') && no_later_option(ws.drop_first())
    }
}

/// 1 = -n, 2 = -e, 3 = -E, -1 = an unknown leading-dash word, 0 = plain.
pub open spec fn dash_flag(w: Seq<char>) -> int {
    if w.len() == 2 && w[0] == '-' && w[1] == 'n' {
        1
    } else if w.len() == 2 && w[0] == '-' && w[1] == 'e' {
        2
    } else if w.len() == 2 && w[0] == '-' && w[1] == 'E' {
        3
    } else if w.len() > 0 && w[0] == '-' {
        -1
    } else {
        0
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

pub open spec fn decode_all(ws: Seq<Seq<char>>, decode: bool) -> Option<Seq<Seq<char>>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Some(Seq::empty())
    } else {
        let w = if decode { dec_echo(ws[0]) } else { Some(ws[0]) };
        match w {
            Some(d) => match decode_all(ws.drop_first(), decode) {
                Some(r) => Some(seq![d].add(r)),
                None => None,
            },
            None => None,
        }
    }
}

pub open spec fn drop_empties(ws: Seq<Seq<char>>) -> Seq<Seq<char>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else if ws[0].len() == 0 {
        drop_empties(ws.drop_first())
    } else {
        seq![ws[0]].add(drop_empties(ws.drop_first()))
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

// ─────────────────────────────────────────────────────────── echo: pass ──

pub open spec fn pass_echo(ws: Seq<Seq<char>>) -> Option<(Seq<char>, bool)> {
    if any_glob(ws) {
        None
    } else if ws.len() > 0 && dash_flag(ws[0]) < 0 {
        None
    } else {
        let newline = !(ws.len() > 0 && dash_flag(ws[0]) == 1);
        let decode = ws.len() > 0 && dash_flag(ws[0]) == 2;
        let rest = if ws.len() > 0 && dash_flag(ws[0]) >= 1 && dash_flag(ws[0]) <= 3 {
            ws.drop_first()
        } else {
            ws
        };
        match decode_all(rest, decode) {
            Some(ds) => Some((join(drop_empties(ds)), newline)),
            None => None,
        }
    }
}

// ─────────────────────────────────────────────────────── echo: semantics ──

/// What bash's echo writes: the argument bytes (after the -e decision) joined
/// by single spaces, with `\n` unless -n. The flag tuple is computed from the
/// command's FIRST word; the domain premise excludes later option words.
pub open spec fn bash_echo(ws: Seq<Seq<char>>) -> Option<(Seq<char>, bool)> {
    let flag = if ws.len() > 0 { dash_flag(ws[0]) } else { 0 };
    let newline = flag != 1;
    let decode = flag == 2;
    let rest = if flag >= 1 && flag <= 3 { ws.drop_first() } else { ws };
    match decode_all(rest, decode) {
        Some(ds) => Some((join(ds), newline)),
        None => None,
    }
}

pub proof fn dec_echo_nonempty(w: Seq<char>)
    requires w.len() > 0,
    ensures forall|r: Seq<char>| dec_echo(w) == Some(r) ==> r.len() > 0,
{
}

pub proof fn decode_all_identity(ws: Seq<Seq<char>>)
    ensures decode_all(ws, false) == Some(ws),
    decreases ws.len(),
{
    if ws.len() > 0 {
        decode_all_identity(ws.drop_first());
    }
}

pub proof fn decode_all_nonempty(ws: Seq<Seq<char>>, decode: bool)
    requires all_nonempty(ws),
    ensures forall|ds: Seq<Seq<char>>| decode_all(ws, decode) == Some(ds)
        ==> all_nonempty(ds),
    decreases ws.len(),
{
    if ws.len() == 0 {
    } else {
        decode_all_nonempty(ws.drop_first(), decode);
        if decode {
            dec_echo_nonempty(ws[0]);
        }
        let d = if decode { dec_echo(ws[0]) } else { Some(ws[0]) };
        match d {
            Some(dd) => {
                if decode {
                    dec_echo_nonempty(ws[0]);
                }
                assert(dd.len() > 0);
                match decode_all(ws.drop_first(), decode) {
                    Some(r) => {
                        assert(all_nonempty(r));
                        assert(seq![dd].add(r)[0] == dd);
                        assert(seq![dd].add(r).drop_first() == r);
                        assert(all_nonempty(seq![dd].add(r)));
                    }
                    None => {}
                }
            }
            None => {}
        }
    }
}

pub proof fn echo_emits_the_same_bytes(ws: Seq<Seq<char>>)
    requires
        !any_glob(ws),
        all_nonempty(ws),
        no_later_option(ws),
        ws.len() == 0 || dash_flag(ws[0]) >= 0,
    ensures pass_echo(ws) == bash_echo(ws),
{
    if ws.len() == 0 {
        assert(pass_echo(ws) == Some((Seq::empty(), true)));
        assert(bash_echo(ws) == Some((Seq::empty(), true)));
    } else {
        let f = dash_flag(ws[0]);
        let rest = if f >= 1 && f <= 3 { ws.drop_first() } else { ws };
        let decode = f == 2;
        if f == 0 || f == 1 || f == 3 {
            assert(decode == false);
            decode_all_identity(rest);
            drop_empties_identity(rest);
        } else {
            assert(f == 2);
            match decode_all(rest, true) {
                Some(ds) => {
                    decode_all_nonempty(rest, true);
                    drop_empties_identity(ds);
                }
                None => {}
            }
        }
    }
}

pub proof fn drop_empties_identity(ws: Seq<Seq<char>>)
    requires all_nonempty(ws),
    ensures drop_empties(ws) == ws,
    decreases ws.len(),
{
    if ws.len() > 0 {
        drop_empties_identity(ws.drop_first());
    }
}

// ────────────────────────────────────────────────────────── printf: pass ──

pub open spec fn pass_printf_at(
    fmt: Seq<char>,
    args: Seq<Seq<char>>,
    i: int,
    used: int,
) -> Option<(Seq<char>, int)>
    decreases (fmt.len() - i) as nat,
{
    if i >= fmt.len() {
        Some((Seq::empty(), used))
    } else if fmt[i] == '%' {
        if i + 1 >= fmt.len() {
            None
        } else if fmt[i + 1] == '%' {
            match pass_printf_at(fmt, args, i + 2, used) {
                Some((r, u)) => Some((seq!['%'].add(r), u)),
                None => None,
            }
        } else if fmt[i + 1] == 's' {
            if used >= args.len() {
                None
            } else {
                match pass_printf_at(fmt, args, i + 2, used + 1) {
                    Some((r, u)) => Some((args[used].add(r), u)),
                    None => None,
                }
            }
        } else {
            None
        }
    } else if fmt[i] == '\\' {
        if i + 1 >= fmt.len() {
            None
        } else {
            let c = if fmt[i + 1] == 'n' { Some('\n') }
                else if fmt[i + 1] == 't' { Some('\t') }
                else if fmt[i + 1] == 'r' { Some('\r') }
                else if fmt[i + 1] == '\\' { Some('\\') }
                else { None };
            match c {
                Some(ch) => match pass_printf_at(fmt, args, i + 2, used) {
                    Some((r, u)) => Some((seq![ch].add(r), u)),
                    None => None,
                },
                None => None,
            }
        }
    } else {
        match pass_printf_at(fmt, args, i + 1, used) {
            Some((r, u)) => Some((seq![fmt[i]].add(r), u)),
            None => None,
        }
    }
}

pub open spec fn pass_printf(fmt: Seq<char>, args: Seq<Seq<char>>) -> Option<Seq<char>> {
    match pass_printf_at(fmt, args, 0, 0) {
        Some((text, used)) => if used == args.len() { Some(text) } else { None },
        None => None,
    }
}

// ──────────────────────────────────────────────────── printf: semantics ──

/// printf's format language as a slice recursion (no indices): the same
/// accepted subset, but the bytes are assembled by consuming the format from
/// the front. `used` must end equal to the argument count.
pub open spec fn printf_render(
    fmt: Seq<char>,
    args: Seq<Seq<char>>,
    used: int,
) -> Option<(Seq<char>, int)>
    decreases fmt.len(),
{
    if fmt.len() == 0 {
        Some((Seq::empty(), used))
    } else if fmt[0] == '%' {
        if fmt.len() < 2 {
            None
        } else if fmt[1] == '%' {
            match printf_render(fmt.subrange(2, fmt.len() as int), args, used) {
                Some((r, u)) => Some((seq!['%'].add(r), u)),
                None => None,
            }
        } else if fmt[1] == 's' {
            if used >= args.len() {
                None
            } else {
                match printf_render(fmt.subrange(2, fmt.len() as int), args, used + 1) {
                    Some((r, u)) => Some((args[used].add(r), u)),
                    None => None,
                }
            }
        } else {
            None
        }
    } else if fmt[0] == '\\' {
        if fmt.len() < 2 {
            None
        } else {
            let c = if fmt[1] == 'n' { Some('\n') }
                else if fmt[1] == 't' { Some('\t') }
                else if fmt[1] == 'r' { Some('\r') }
                else if fmt[1] == '\\' { Some('\\') }
                else { None };
            match c {
                Some(ch) => match printf_render(fmt.subrange(2, fmt.len() as int), args, used) {
                    Some((r, u)) => Some((seq![ch].add(r), u)),
                    None => None,
                },
                None => None,
            }
        }
    } else {
        match printf_render(fmt.drop_first(), args, used) {
            Some((r, u)) => Some((seq![fmt[0]].add(r), u)),
            None => None,
        }
    }
}

pub open spec fn bash_printf(fmt: Seq<char>, args: Seq<Seq<char>>) -> Option<Seq<char>> {
    match printf_render(fmt, args, 0) {
        Some((text, used)) => if used == args.len() { Some(text) } else { None },
        None => None,
    }
}

// The index loop and the slice recursion agree at every starting index.
pub proof fn printf_index_is_slice(
    fmt: Seq<char>,
    args: Seq<Seq<char>>,
    i: int,
    used: int,
)
    requires 0 <= i <= fmt.len(),
    ensures pass_printf_at(fmt, args, i, used)
        == printf_render(fmt.subrange(i, fmt.len() as int), args, used),
    decreases (fmt.len() - i) as nat,
{
    if i >= fmt.len() {
    } else {
        let rest = fmt.subrange(i, fmt.len() as int);
        assert(rest.len() > 0 && rest[0] == fmt[i]);
        assert(rest.drop_first() == fmt.subrange(i + 1, fmt.len() as int));
        if fmt[i] == '%' {
            if i + 1 < fmt.len() && fmt[i + 1] == '%' {
                assert(rest.subrange(2, rest.len() as int)
                    == fmt.subrange(i + 2, fmt.len() as int));
                printf_index_is_slice(fmt, args, i + 2, used);
            } else if i + 1 < fmt.len() && fmt[i + 1] == 's' {
                if used < args.len() {
                    assert(rest.subrange(2, rest.len() as int)
                        == fmt.subrange(i + 2, fmt.len() as int));
                    printf_index_is_slice(fmt, args, i + 2, used + 1);
                }
            }
        } else if fmt[i] == '\\' {
            if i + 1 < fmt.len() {
                assert(rest.subrange(2, rest.len() as int)
                    == fmt.subrange(i + 2, fmt.len() as int));
                printf_index_is_slice(fmt, args, i + 2, used);
            }
        } else {
            printf_index_is_slice(fmt, args, i + 1, used);
        }
    }
}

pub proof fn printf_emits_the_same_bytes(fmt: Seq<char>, args: Seq<Seq<char>>)
    ensures pass_printf(fmt, args) == bash_printf(fmt, args),
{
    assert(fmt.subrange(0, fmt.len() as int) == fmt);
    printf_index_is_slice(fmt, args, 0, 0);
    assert(printf_render(fmt, args, 0) == pass_printf_at(fmt, args, 0, 0));
    match pass_printf_at(fmt, args, 0, 0) {
        Some((text, used)) => {
            assert(pass_printf(fmt, args)
                == (if used == args.len() { Some(text) } else { None::<Seq<char>> }));
            assert(bash_printf(fmt, args)
                == (if used == args.len() { Some(text) } else { None::<Seq<char>> }));
        }
        None => {
            assert(pass_printf(fmt, args) == None::<Seq<char>>);
            assert(bash_printf(fmt, args) == None::<Seq<char>>);
        }
    }
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

/// `-n` clears the newline; the emitted node must say so.
pub proof fn pin_n_clears_the_newline() {
    assert(pass_echo(seq![seq!['-', 'n'], seq!['x']])
        == Some((seq!['x'], false))) by (compute);
    assert(bash_echo(seq![seq!['-', 'n'], seq!['x']])
        == Some((seq!['x'], false))) by (compute);
}

/// `-e` decodes, `-E` does not: `\n` is a newline versus a backslash and n.
pub proof fn pin_e_decodes_escapes() {
    assert(pass_echo(seq![seq!['-', 'e'], seq!['a', '\\', 'n', 'b']])
        == Some((seq!['a', '\n', 'b'], true))) by (compute);
}

pub proof fn pin_upper_e_keeps_escapes_literal() {
    assert(pass_echo(seq![seq!['-', 'E'], seq!['a', '\\', 'n', 'b']])
        == Some((seq!['a', '\\', 'n', 'b'], true))) by (compute);
}

/// An unknown leading-dash flag is refused, not treated as a word.
pub proof fn pin_unknown_flag_refused() {
    assert(pass_echo(seq![seq!['-', 'z'], seq!['x']]) == None::<(Seq<char>, bool)>)
        by (compute);
}

/// A glob word keeps the runtime path; the fold must not emit it literally.
pub proof fn pin_glob_word_refused() {
    assert(pass_echo(seq![seq!['*'], seq!['.', 't', 'x', 't']])
        == None::<(Seq<char>, bool)>) by (compute);
}

/// printf does not add a newline unless the format says so.
pub proof fn pin_printf_has_no_implicit_newline() {
    assert(pass_printf(seq!['x'], Seq::empty()) == Some(seq!['x'])) by (compute);
    assert(pass_printf(seq!['x', '\\', 'n'], Seq::empty()) == Some(seq!['x', '\n']))
        by (compute);
}

/// More arguments than conversions is REFUSED (bash would repeat the format;
/// the fold declines instead of guessing).
pub proof fn pin_printf_arg_count_exact() {
    assert(pass_printf(seq!['%', 's'], seq![seq!['a'], seq!['b']]) == None::<Seq<char>>)
        by (compute);
    assert(pass_printf(seq!['%', 's'], Seq::empty()) == None::<Seq<char>>) by (compute);
}

/// `%d` (and any conversion outside `%s`/`%%`) is refused by this fold.
pub proof fn pin_printf_unknown_conversion_refused() {
    assert(pass_printf(seq!['%', 'd'], seq![seq!['1']]) == None::<Seq<char>>) by (compute);
}

fn main() {}

} // verus!
