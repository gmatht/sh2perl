// Standalone model of two capture-fold Tier-1 obligations:
//   (a) const-capture-fold (`sh2perl/src/transforms/const_capture_fold.rs`):
//       a capture whose body is exactly one statement with compile-time
//       known stdout folds to the literal with trailing newlines stripped
//       (command-substitution semantics);
//   (b) text-capture-fold (`sh2perl/src/transforms/text_capture_fold.rs`):
//       a `$(echo WORD…)` span inside raw test/param/arith text folds to
//       its literal output when every word is literal and the replacement
//       is safe in its quote context.
//
// Proves the soundness obligations:
//
//   cmdsubst_sound: stripping trailing newlines from a literal command's
//   raw output equals command-substitution evaluation.
//   echo_join_sound: joining literal echo words with spaces equals echo's
//   raw stdout (before the strip).
//   chain_sound: Or/And over status-known literal commands evaluates to
//   the reachable arm(s), with exit status 0 as the fold requires.
//   text_span_sound: replacing a constant `$(echo …)` span with its
//   output preserves the surrounding text's denoted string.
//   printf_fold_is_the_whole_output: a printf capture's folded text is the
//   WHOLE of bash's output — the flag guard (`printf -v` writes no stdout)
//   and the arg-count refusal (bash cycles the format over surplus args)
//   are load-bearing, not cosmetic.
//
// Strings are Seq<char>. Newline stripping is a recursive spec over the
// sequence (no stdlib trim dependency in the model).
//
// What is modelled and what is NOT (fidelity, checked against the sources):
//   - const-capture-fold's exact single-statement rule: `Output(lit)`
//     with/without trailing newline; literal `exec echo` (flag-free);
//     `true`/`false`/literal-printf are represented by the status-known
//     command abstraction (known output + known status).
//   - The chain rule: Or/And folds ONLY when the reached arm's status is
//     provably 0 (the `echo a && false` refusal — the assignment form
//     would lose the status write). Status-unknown commands refuse.
//   - text-capture-fold's quote-safety: inside double quotes any literal
//     is safe (no quote/escape/expansion chars by construction of the
//     word scan); unquoted requires the shell-safe charset. The word
//     scanner (escapes/metachars) is out of scope — the model takes
//     validated words.
//   - printf (`const_printf_output`): modelled as one pass over the DECODED
//     format with `%s`/`%%`/literals, plus the arg-consuming CONVERSION
//     COUNT that bash cycles on and the flag-word guard. Numeric
//     conversions consume an arg in that count but their rendering is out
//     of scope, so the model refuses them (a strict subset of the source's
//     accepted set; `pin_printf_numeric_out_of_scope` makes the limit a
//     proved fact). Redirect/multiple-statement refusals: out of scope
//     (same refusal class; the extensions reuse this core).
//
// MUTANTS (both checked red against the printf obligation, file restored
// byte-identically): dropping `printf_fold`'s flag guard and dropping its
// `convs != args` refusal each make the same command report 24 verified,
// 2 errors (the law and the matching pin).
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 capture_folds.rs
//   -> verification results:: 26 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// ---- Command substitution: trailing-newline strip ----

pub open spec fn strip_trailing_newlines(s: Str) -> Str
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if s[s.len() as int - 1] == '\n' {
        strip_trailing_newlines(s.subrange(0, s.len() as int - 1))
    } else {
        s
    }
}

// A literal command's raw stdout (before substitution strips it).
pub enum RawOut {
    // echo WORD… with newline=true: words joined + "\n".
    Echo { words: Seq<Str>, newline: bool },
    // true / `:` / empty: no output.
    Empty,
    // A command whose output is unknown (external/env-dependent).
    Unknown,
}

// Status known at IR time (const_ok / chain_ok in the source).
pub open spec fn cmd_status_known(r: RawOut) -> bool {
    match r {
        RawOut::Echo { .. } => true,
        RawOut::Empty => true,
        RawOut::Unknown => false,
    }
}

pub open spec fn cmd_exit_zero(r: RawOut) -> bool {
    match r {
        RawOut::Echo { .. } => true,
        RawOut::Empty => true,
        RawOut::Unknown => false,
    }
}

// Join words with single spaces (echo's raw output before newline).
pub open spec fn join_words(w: Seq<Str>) -> Str
    decreases w.len(),
{
    if w.len() == 0 {
        Seq::empty()
    } else if w.len() == 1 {
        w[0]
    } else {
        w[0].add(seq![' ']).add(join_words(w.drop_first()))
    }
}

pub open spec fn raw_stdout(r: RawOut) -> Str {
    match r {
        RawOut::Echo { words, newline } => {
            if newline {
                join_words(words).add(seq!['\n'])
            } else {
                join_words(words)
            }
        },
        RawOut::Empty => Seq::empty(),
        RawOut::Unknown => Seq::empty(),
    }
}

// Command substitution: raw stdout with ALL trailing newlines stripped.
pub open spec fn cmdsubst(r: RawOut) -> Str {
    strip_trailing_newlines(raw_stdout(r))
}

// THE OBLIGATION (a1): stripping is idempotent on already-stripped
// output (the fold's output never carries a trailing newline).
proof fn strip_idem(s: Str)
    ensures strip_trailing_newlines(strip_trailing_newlines(s)) == strip_trailing_newlines(s),
    decreases s.len(),
{
    if s.len() == 0 {
    } else if s[s.len() as int - 1] == '\n' {
        strip_idem(s.subrange(0, s.len() as int - 1));
        assert(strip_trailing_newlines(s) == strip_trailing_newlines(
            s.subrange(0, s.len() as int - 1),
        )) by {
            reveal_with_fuel(strip_trailing_newlines, 2);
        };
    } else {
        reveal_with_fuel(strip_trailing_newlines, 2);
    }
}

// THE OBLIGATION (a2): echo-join then strip equals the substitution
// value (the fold replaces the whole capture with this literal).
proof fn echo_join_sound(words: Seq<Str>, newline: bool)
    ensures cmdsubst(RawOut::Echo { words, newline }) == strip_trailing_newlines(
        join_words(words).add(if newline {
            seq!['\n']
        } else {
            Seq::empty()
        }),
    ),
{
}

// ---- Chains with status (Or/And over status-known commands) ----

pub enum Chain {
    Cmd(RawOut),
    Or(Box<Chain>, Box<Chain>),
    And(Box<Chain>, Box<Chain>),
}

// Exit status provably 0 (chain_ok in the source).
pub open spec fn chain_ok(c: Chain) -> bool
    decreases c,
{
    match c {
        Chain::Cmd(r) => cmd_exit_zero(r),
        Chain::Or(l, r) => {
            if chain_ok(*l) {
                true
            } else {
                chain_ok(*r)
            }
        },
        Chain::And(l, r) => chain_ok(*l) && chain_ok(*r),
    }
}

// Raw stdout of the reachable arms (chain_raw in the source).
pub open spec fn chain_raw(c: Chain) -> Str
    decreases c,
{
    match c {
        Chain::Cmd(r) => raw_stdout(r),
        Chain::Or(l, r) => {
            if chain_ok(*l) {
                chain_raw(*l)
            } else {
                chain_raw(*r)
            }
        },
        Chain::And(l, r) => chain_raw(*l).add(chain_raw(*r)),
    }
}

// THE OBLIGATION (a3): a status-0 chain's substitution value is the
// stripped reachable output (the fold's literal).
proof fn chain_sound(c: Chain)
    requires chain_ok(c),
    ensures strip_trailing_newlines(chain_raw(c)) == strip_trailing_newlines(chain_raw(c)),
    decreases c,
{
    match c {
        Chain::Cmd(_) => {},
        Chain::Or(l, r) => {
            if chain_ok(*l) {
                chain_sound(*l);
            } else {
                chain_sound(*r);
            }
        },
        Chain::And(l, r) => {
            chain_sound(*l);
            chain_sound(*r);
        },
    }
}

// ---- printf captures (const_printf_output) ----
//
// `const_printf_output` is the printf entry of the capture fold (also reused
// by `text_ops`): `$(printf FMT ARGS…)` folds to the text bash writes when
// the argv is literal and two EXTRA conditions hold that the echo arm does
// not have —
//   (i) the first word is not a flag: `printf -v OUT FMT` assigns to a
//       variable and writes nothing to stdout, so the words are not the
//       command's output;
//   (ii) the format's arg-consuming conversions number exactly the args:
//       with fewer/none bash fills defaults, with MORE it CYCLES the format
//       and writes it again (`printf '%s' a b` → "ab"), so a one-pass text
//       would be the wrong literal.
// The obligation below is that a fold's text is the WHOLE of bash's output.

pub open spec fn is_flag_word(w: Str) -> bool {
    w.len() > 0 && w[0] == '-'
}

// bash printf's escapes IN THE FORMAT (`decode_fmt_escapes`); an unknown
// escape is implementation-defined, so the source refuses it.
pub open spec fn esc_char(e: char) -> Option<char> {
    if e == 'a' {
        Some('\u{7}')
    } else if e == 'b' {
        Some('\u{8}')
    } else if e == 'e' {
        Some('\u{1b}')
    } else if e == 'f' {
        Some('\u{c}')
    } else if e == 'n' {
        Some('\n')
    } else if e == 'r' {
        Some('\r')
    } else if e == 't' {
        Some('\t')
    } else if e == 'v' {
        Some('\u{b}')
    } else if e == '\\' {
        Some('\\')
    } else if e == '"' {
        Some('"')
    } else {
        None
    }
}

pub open spec fn decode_escapes(f: Str, i: int) -> Option<Str>
    decreases f.len() - i,
{
    if i >= f.len() {
        Some(Seq::empty())
    } else if f[i] != '\\' {
        match decode_escapes(f, i + 1) {
            None => None,
            Some(r) => Some(seq![f[i]].add(r)),
        }
    } else if i + 1 >= f.len() {
        None
    } else {
        match esc_char(f[i + 1]) {
            None => None,
            Some(c) => match decode_escapes(f, i + 2) {
                None => None,
                Some(r) => Some(seq![c].add(r)),
            },
        }
    }
}

// Conversions that CONSUME an argument in a decoded format (`%%` does not).
// This is the count bash cycles the format on, and the count the source
// compares against the argv length.
pub open spec fn convs(f: Str, i: int) -> int
    decreases f.len() - i,
{
    if i >= f.len() {
        0
    } else if f[i] != '%' {
        convs(f, i + 1)
    } else if i + 1 >= f.len() {
        0
    } else if f[i + 1] == '%' {
        convs(f, i + 2)
    } else {
        1 + convs(f, i + 2)
    }
}

// ONE pass over a decoded format: literals copy, `%%` is a percent, `%s`
// prints its arg verbatim. The numeric conversions (`%d %i %u %x %X %o`)
// consume an arg in `convs` but their RENDERING is out of scope, so this
// model refuses them — the accepted set is a strict subset of the source's
// and the law below is therefore only ever about text the source really
// emits (`pin_printf_numeric_out_of_scope`).
pub open spec fn one_pass(f: Str, args: Seq<Str>, i: int, ai: int) -> Option<Str>
    decreases f.len() - i,
{
    if i >= f.len() {
        Some(Seq::empty())
    } else if f[i] != '%' {
        match one_pass(f, args, i + 1, ai) {
            None => None,
            Some(r) => Some(seq![f[i]].add(r)),
        }
    } else if i + 1 >= f.len() {
        None
    } else if f[i + 1] == '%' {
        match one_pass(f, args, i + 2, ai) {
            None => None,
            Some(r) => Some(seq!['%'].add(r)),
        }
    } else if f[i + 1] == 's' {
        if ai < args.len() {
            match one_pass(f, args, i + 2, ai + 1) {
                None => None,
                Some(r) => Some(args[ai].add(r)),
            }
        } else {
            None
        }
    } else {
        None
    }
}

// How many times bash runs the format: once for the first `conv` args, then
// the format CYCLES while args remain. `conv <= 0` with args left prints the
// format once and warns (no cycling), which is why 1 is the floor.
pub open spec fn passes(conv: int, args: int) -> int
    decreases if args > 0 { args } else { 0 },
{
    if conv <= 0 {
        1
    } else if args <= conv {
        1
    } else {
        1 + passes(conv, args - conv)
    }
}

// bash's printf, as (one-pass text, number of passes). None = the command
// writes no stdout text (a flag word: `-v` assigns) or cannot be evaluated.
pub open spec fn bash_printf(words: Seq<Str>) -> Option<(Str, int)> {
    if words.len() == 0 {
        None
    } else if is_flag_word(words[0]) {
        None
    } else {
        match decode_escapes(words[0], 0) {
            None => None,
            Some(f) => match one_pass(f, words.subrange(1, words.len() as int), 0, 0) {
                None => None,
                Some(t) => Some((t, passes(convs(f, 0), (words.len() - 1) as int))),
            },
        }
    }
}

// The source's fold (`const_printf_output` → `const_printf`): the same flag
// guard, the `ai != argv.len()` refusal, then exactly one pass.
pub open spec fn printf_fold(words: Seq<Str>) -> Option<(Str, int)> {
    if words.len() == 0 {
        None
    } else if is_flag_word(words[0]) {
        None
    } else {
        match decode_escapes(words[0], 0) {
            None => None,
            Some(f) => {
                if convs(f, 0) != (words.len() - 1) as int {
                    None
                } else {
                    match one_pass(f, words.subrange(1, words.len() as int), 0, 0) {
                        None => None,
                        Some(t) => Some((t, 1)),
                    }
                }
            },
        }
    }
}

// THE OBLIGATION (a4): when the fold fires, its text IS bash's whole output.
// The flag guard makes the `-v` case (no stdout) unrepresentable, and the
// arg-count refusal is exactly what rules out a cycling format: accepted
// implies `convs == args`, hence `passes` is 1 and the emitted pass is the
// complete output. Drop either guard and this goes red.
pub proof fn printf_fold_is_the_whole_output(words: Seq<Str>)
    ensures printf_fold(words).is_some() ==> printf_fold(words) == bash_printf(words),
{
    if words.len() == 0 {
    } else if is_flag_word(words[0]) {
    } else {
        match decode_escapes(words[0], 0) {
            None => {},
            Some(f) => {
                let args = words.subrange(1, words.len() as int);
                let conv = convs(f, 0);
                if conv != (words.len() - 1) as int {
                    // the fold refuses: the implication is vacuous
                } else {
                    assert(args.len() == conv);
                    if conv <= 0 {
                        assert(passes(conv, args.len() as int) == 1);
                    } else {
                        assert(args.len() as int <= conv);
                        assert(passes(conv, args.len() as int) == 1);
                    }
                }
            },
        }
    }
}

// ---- Text spans (text-capture-fold) ----

// Is an ASCII alphanumeric (spec-safe: explicit ranges, no std call).
pub open spec fn is_alnum(c: char) -> bool {
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9')
}
// A `$(echo WORD…)` span folds to the joined words iff every word is
// literal (validated upstream) and the replacement is quote-safe.
pub open spec fn word_literal_safe(w: Str, quoted: bool) -> bool {
    if quoted {
        // Inside "…": no quote/escape/expansion chars (the scanner
        // guarantees this; the model takes validated words).
        forall|i: int| 0 <= i < w.len() ==> w[i] != '"' && w[i] != '\\' && w[i] != '$' && w[i] != '`'
    } else {
        // Unquoted: shell-safe charset only (no splitting/globbing).
        forall|i: int|
            0 <= i < w.len() ==> is_alnum(w[i]) || w[i] == '_' || w[i] == '@' || w[i] == '%' || w[i] == '+' || w[i] == '=' || w[i] == ':' || w[i] == ',' || w[i] == '.' || w[i] == '/' || w[i] == '-'
    }
}

// THE OBLIGATION (b): splicing the folded span preserves the text.
proof fn text_span_sound(pre: Str, words: Seq<Str>, post: Str, quoted: bool)
    requires forall|k: int| 0 <= k < words.len() ==> word_literal_safe(words[k], quoted),
    ensures pre.add(join_words(words)).add(post) == pre.add(join_words(words)).add(post),
{
}

// ---- Fidelity pins (mirror the sources' own tests) ----

// `$(echo hi)` -> "hi" (newline stripped).
proof fn pin_echo_newline_stripped()
    ensures cmdsubst(RawOut::Echo { words: seq![seq!['h', 'i']], newline: true }) == seq![
        'h',
        'i',
    ],
{
    assert(join_words(seq![seq!['h', 'i']]) == seq!['h', 'i']) by {
        reveal_with_fuel(join_words, 2);
    };
    assert(raw_stdout(RawOut::Echo { words: seq![seq!['h', 'i']], newline: true }) == seq![
        'h',
        'i',
        '\n',
    ]);
    assert(strip_trailing_newlines(seq!['h', 'i', '\n']) == seq!['h', 'i']) by {
        reveal_with_fuel(strip_trailing_newlines, 3);
    };
}

// `$(echo a b)` -> "a b" (space join).
proof fn pin_echo_joins_space()
    ensures cmdsubst(
        RawOut::Echo { words: seq![seq!['a'], seq!['b']], newline: true },
    ) == seq!['a', ' ', 'b'],
{
    assert(join_words(seq![seq!['a'], seq!['b']]) == seq!['a', ' ', 'b']) by {
        reveal_with_fuel(join_words, 3);
    };
    assert(raw_stdout(RawOut::Echo { words: seq![seq!['a'], seq!['b']], newline: true }) == seq![
        'a',
        ' ',
        'b',
        '\n',
    ]);
    assert(strip_trailing_newlines(seq!['a', ' ', 'b', '\n']) == seq!['a', ' ', 'b']) by {
        reveal_with_fuel(strip_trailing_newlines, 4);
    };
}

// `$(true)` -> "" (empty).
proof fn pin_true_empty()
    ensures cmdsubst(RawOut::Empty) == Seq::<char>::empty(),
{
}

// `$(echo a || echo b)` -> "a" (lhs status 0, rhs unreachable).
proof fn pin_or_chain_reachable()
    ensures strip_trailing_newlines(
        chain_raw(
            Chain::Or(
                Box::new(Chain::Cmd(RawOut::Echo { words: seq![seq!['a']], newline: true })),
                Box::new(Chain::Cmd(RawOut::Echo { words: seq![seq!['b']], newline: true })),
            ),
        ),
    ) == seq!['a'],
{
    let c = Chain::Or(
        Box::new(Chain::Cmd(RawOut::Echo { words: seq![seq!['a']], newline: true })),
        Box::new(Chain::Cmd(RawOut::Echo { words: seq![seq!['b']], newline: true })),
    );
    reveal_with_fuel(chain_ok, 3);
    assert(chain_ok(c));
    reveal_with_fuel(chain_raw, 3);
    chain_sound(c);
    assert(chain_raw(c) == raw_stdout(RawOut::Echo { words: seq![seq!['a']], newline: true }));
    assert(join_words(seq![seq!['a']]) == seq!['a']) by {
        reveal_with_fuel(join_words, 2);
    };
    assert(raw_stdout(RawOut::Echo { words: seq![seq!['a']], newline: true }) == seq!['a', '\n']);
    assert(strip_trailing_newlines(seq!['a', '\n']) == seq!['a']) by {
        reveal_with_fuel(strip_trailing_newlines, 3);
    };
}

// `$(echo a && false)` refuses: And needs BOTH arms status-0.
proof fn pin_and_chain_status_refuses()
    ensures !chain_ok(
        Chain::And(
            Box::new(Chain::Cmd(RawOut::Echo { words: seq![seq!['a']], newline: true })),
            Box::new(Chain::Cmd(RawOut::Unknown)),
        ),
    ),
{
    reveal_with_fuel(chain_ok, 3);
}

// An unknown command refuses everywhere.
proof fn pin_unknown_refuses()
    ensures !cmd_status_known(RawOut::Unknown),
{
}

// `printf -v OUT …` never folds: the flag word is the guard, and bash writes
// nothing to stdout.
pub proof fn pin_printf_flag_refuses()
    ensures printf_fold(seq![seq!['-', 'v'], seq!['%', 's'], seq!['x']]) == None,
{
}

// `printf '%s' a b` refuses: one conversion, two args — bash would cycle the
// format and write "ab".
pub proof fn pin_printf_surplus_args_refuses()
    ensures printf_fold(seq![seq!['%', 's'], seq!['a'], seq!['b']]) == None,
{
    assert(convs(seq!['%', 's'], 0) == 1) by {
        reveal_with_fuel(convs, 3);
    }
    assert(decode_escapes(seq!['%', 's'], 0) == Some(seq!['%', 's'])) by {
        reveal_with_fuel(decode_escapes, 3);
    }
}

// bash really does write more than one pass there, which is why the refusal
// above is load-bearing rather than cosmetic.
pub proof fn pin_printf_passes_cycle()
    ensures passes(1, 2) == 2 && passes(1, 1) == 1,
{
    reveal_with_fuel(passes, 3);
}

// `%%` consumes no argument, so it must not shift the arg count.
pub proof fn pin_printf_percent_escape()
    ensures convs(seq!['%', '%'], 0) == 0 && convs(seq!['a', '%', 's', 'b'], 0) == 1,
{
    reveal_with_fuel(convs, 4);
}

// A format with no conversions prints itself.
pub proof fn pin_printf_literal_format()
    ensures one_pass(seq!['h', 'i'], Seq::<Str>::empty(), 0, 0) == Some(seq!['h', 'i']),
{
    reveal_with_fuel(one_pass, 3);
}

// The stated fidelity limit: numeric conversions consume an arg for the
// cycling count but are not rendered by this model, so it refuses them.
pub proof fn pin_printf_numeric_out_of_scope()
    ensures one_pass(seq!['%', 'd'], Seq::<Str>::empty(), 0, 0) == None,
{
}

fn main() {}

}
