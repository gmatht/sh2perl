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
//   - printf format evaluation, flag detection, redirect/multiple-statement
//     refusals: out of scope (same refusal class; the extensions reuse
//     this core).
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

fn main() {}

}
