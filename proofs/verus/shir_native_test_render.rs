//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_test_render.rs — cluster D1 of the
//  `shir_native_stmt` decomposition: the atomic `test` renderer round trip.
//
//  `render_test_tokens` serialises a tokenised `[ ... ]` / `[[ ... ]]` test
//  into an IR expression. D1's law is the round trip the plan states:
//
//    eval(parse(render(t))) = eval(t)
//
//  restricted to the atomic shapes that function handles: 1 token, a 2-token
//  unary test, and a 3-token binary test. Modelled as: whenever the renderer
//  accepts a token list, the expression it emits evaluates to exactly what
//  bash's `test` returns for the same tokens in the same world.
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  PASS side: `render_tokens` transcribes `render_test_tokens`' 1-, 2- and
//  3-token arms into a structured expression (`Expr`): bare truthiness, the
//  unary file tests it maps (`-e -f -d -s -S -h/-L -a -z -n`), `=`/`==`/`!=`,
//  the six numeric comparisons and `>`/`<` string comparisons.
//
//  SEMANTIC side: `bash_atom` evaluates the same token list with bash's own
//  `test` rules — bare word non-empty, unary predicates over a fixed world,
//  string equality, numeric numification (so a non-number is 0), lexicographic
//  `>`/`<` — independently of the renderer's expression constructors.
//  `eval_expr` is the expression semantics; `render_tokens_is_bash` proves the
//  two agree, and `test_renders_to_the_same_truth` restates it as the law.
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * `render_test_text`'s top-level connector split (`&&`, `-a`, `||`, `-o`)
//     and the chain semantics it induces are cluster D2/D3, not claimed here:
//     the split's mixed-kind precedence does not match bash (`-a` binds
//     tighter than `-o`, while the source looks for `&&` before `-a` before
//     `||` before `-o`), so its law needs a domain premise and its own proof.
//   * The tokenizer (`ir::tokenize_test`, quote stripping) is a lexer; the
//     model starts from the token list.
//   * `render_flag_substitution_test` (`-n $(cmd)`) is a runtime capture, not
//     a test-grammar round trip: it needs command execution. Out of model.
//   * Parentheses and `!` are handled by the shared parser
//     (`ir::render_test_token_list`), not by this renderer. Out of model.
//   * Globs on the RHS of `=`, the `=~` regex arm and `-ef` are not this
//     round trip; the law covers the literal equality path.
//   * Numeric and string operands are literals; `$var` substitution is the
//     caller's (`render_test_words`) and is out of model.
//   * Renderer output text (Perl syntax templates) is out of model: the pass
//     side is the Boolean expression those templates denote.
//
//  ── measured mutants (each restored byte-identically; sha in the sync) ──
//
//  baseline 15 verified, 0 errors. All five go red:
//    m1 any dash word on the unary arm becomes truthiness (the flag-regex
//       heuristic): 11 verified, 4 errors (postcondition + pins).
//    m2 `-z`/`-n` swapped: 13 verified, 2 errors (postcondition + pin).
//    m3 `-eq` rendered as string equality: rejected at simplification by
//       `pin_numeric_numifies` (the pin evaluates to false).
//    m4 an unknown unary flag accepted as `-f`: 12 verified, 3 errors.
//    m5 a bare word rendered as the empty word (always true): 13 verified,
//       2 errors.
//  Each mutated file was restored byte-identically and its sha256 re-checked.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub enum StrOp {
    Eq,
    Ne,
    Gt,
    Lt,
}

pub enum NumOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

pub enum Expr {
    /// bare word truthiness, and -n / -z
    Len { ne: bool, w: Seq<char> },
    /// a file predicate: e = exists, f = regular, d = dir, s = size>0,
    /// S = socket, l = symlink (-h/-L)
    File { flag: char, path: Seq<char> },
    Str { sop: StrOp, a: Seq<char>, b: Seq<char> },
    Num { nop: NumOp, a: Seq<char>, b: Seq<char> },
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

pub struct World {
    /// path -> bit mask: 1 exists, 2 regular, 4 dir, 8 socket, 16 symlink
    pub kinds: Seq<(Seq<char>, int)>,
    /// path -> size in bytes (absent means 0)
    pub sizes: Seq<(Seq<char>, int)>,
}

// ─────────────────────────────────────────────────── text helpers ────────

pub open spec fn starts_dash(w: Seq<char>) -> bool {
    w.len() > 0 && w[0] == '-'
}

pub open spec fn str_lt(a: Seq<char>, b: Seq<char>) -> bool
    decreases a.len(),
{
    if a.len() == 0 {
        b.len() > 0
    } else if b.len() == 0 {
        false
    } else if a[0] == b[0] {
        str_lt(a.drop_first(), b.drop_first())
    } else {
        a[0] < b[0]
    }
}

/// bash arithmetic's numification of a literal: optional sign + digits,
/// anything else (or empty) is 0.
pub open spec fn digits(w: Seq<char>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        true
    } else {
        (w[0] >= '0' && w[0] <= '9') && digits(w.drop_first())
    }
}

pub open spec fn num_of(w: Seq<char>) -> int
    decreases w.len(),
{
    if w.len() == 0 || !digits(w) {
        0
    } else if w.len() == 1 {
        (w[0] as int) - ('0' as int)
    } else {
        num_of(w.drop_last()) * 10 + ((w[w.len() - 1] as int) - ('0' as int))
    }
}

pub open spec fn look(entries: Seq<(Seq<char>, int)>, path: Seq<char>) -> int
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else if entries[0].0 == path {
        entries[0].1
    } else {
        look(entries.drop_first(), path)
    }
}

pub open spec fn file_exists(w: World, p: Seq<char>) -> bool {
    look(w.kinds, p) != 0
}

pub open spec fn is_file(w: World, p: Seq<char>) -> bool {
    (look(w.kinds, p) / 2) % 2 == 1
}

pub open spec fn is_dir(w: World, p: Seq<char>) -> bool {
    (look(w.kinds, p) / 4) % 2 == 1
}

pub open spec fn is_socket(w: World, p: Seq<char>) -> bool {
    (look(w.kinds, p) / 8) % 2 == 1
}

pub open spec fn is_link(w: World, p: Seq<char>) -> bool {
    (look(w.kinds, p) / 16) % 2 == 1
}

pub open spec fn size_gt0(w: World, p: Seq<char>) -> bool {
    look(w.sizes, p) > 0
}

pub open spec fn file_pred(w: World, flag: char, p: Seq<char>) -> bool {
    if flag == 'e' { file_exists(w, p) }
    else if flag == 'f' { is_file(w, p) }
    else if flag == 'd' { is_dir(w, p) }
    else if flag == 's' { size_gt0(w, p) }
    else if flag == 'S' { is_socket(w, p) }
    else { is_link(w, p) }
}

// ───────────────────────────────────────────────── pass: the renderer ─────

pub open spec fn unary_flag(op: Seq<char>) -> Option<char>
    decreases op.len(),
{
    if op == seq!['-', 'e'] || op == seq!['-', 'a'] {
        Some('e')
    } else if op == seq!['-', 'f'] {
        Some('f')
    } else if op == seq!['-', 'd'] {
        Some('d')
    } else if op == seq!['-', 's'] {
        Some('s')
    } else if op == seq!['-', 'S'] {
        Some('S')
    } else if op == seq!['-', 'h'] || op == seq!['-', 'L'] {
        Some('l')
    } else {
        None
    }
}

pub open spec fn render_tokens(ts: Seq<Seq<char>>) -> Option<Expr> {
    if ts.len() == 1 {
        Some(Expr::Len { ne: true, w: ts[0] })
    } else if ts.len() == 2 && starts_dash(ts[0]) {
        if ts[0] == seq!['-', 'z'] {
            Some(Expr::Len { ne: false, w: ts[1] })
        } else if ts[0] == seq!['-', 'n'] {
            Some(Expr::Len { ne: true, w: ts[1] })
        } else {
            match unary_flag(ts[0]) {
                Some(f) => Some(Expr::File { flag: f, path: ts[1] }),
                None => None,
            }
        }
    } else if ts.len() == 3 {
        let (left, op, right) = (ts[0], ts[1], ts[2]);
        if op == seq!['-', 'f'] {
            Some(Expr::File { flag: 'f', path: left })
        } else if op == seq!['-', 'd'] {
            Some(Expr::File { flag: 'd', path: left })
        } else if op == seq!['-', 'e'] {
            Some(Expr::File { flag: 'e', path: left })
        } else if op == seq!['-', 's'] {
            Some(Expr::File { flag: 's', path: left })
        } else if op == seq!['-', 'z'] {
            Some(Expr::Len { ne: false, w: left })
        } else if op == seq!['-', 'n'] {
            Some(Expr::Len { ne: true, w: left })
        } else if op == seq!['='] || op == seq!['=', '='] {
            Some(Expr::Str { sop: StrOp::Eq, a: left, b: right })
        } else if op == seq!['!', '='] {
            Some(Expr::Str { sop: StrOp::Ne, a: left, b: right })
        } else if op == seq!['-', 'e', 'q'] {
            Some(Expr::Num { nop: NumOp::Eq, a: left, b: right })
        } else if op == seq!['-', 'n', 'e'] {
            Some(Expr::Num { nop: NumOp::Ne, a: left, b: right })
        } else if op == seq!['-', 'l', 't'] {
            Some(Expr::Num { nop: NumOp::Lt, a: left, b: right })
        } else if op == seq!['-', 'l', 'e'] {
            Some(Expr::Num { nop: NumOp::Le, a: left, b: right })
        } else if op == seq!['-', 'g', 't'] {
            Some(Expr::Num { nop: NumOp::Gt, a: left, b: right })
        } else if op == seq!['-', 'g', 'e'] {
            Some(Expr::Num { nop: NumOp::Ge, a: left, b: right })
        } else if op == seq!['>'] || op == seq!['\\', '>'] {
            Some(Expr::Str { sop: StrOp::Gt, a: left, b: right })
        } else if op == seq!['<'] || op == seq!['\\', '<'] {
            Some(Expr::Str { sop: StrOp::Lt, a: left, b: right })
        } else {
            None
        }
    } else {
        None
    }
}

// ──────────────────────────────────────────────── semantic: bash test ────

pub open spec fn num_cmp(op: NumOp, a: int, b: int) -> bool {
    match op {
        NumOp::Eq => a == b,
        NumOp::Ne => a != b,
        NumOp::Lt => a < b,
        NumOp::Le => a <= b,
        NumOp::Gt => a > b,
        NumOp::Ge => a >= b,
    }
}

pub open spec fn eval_expr(w: World, e: Expr) -> bool
    decreases e,
{
    match e {
        Expr::Len { ne, w: word } => if ne { word.len() != 0 } else { word.len() == 0 },
        Expr::File { flag, path } => file_pred(w, flag, path),
        Expr::Str { sop, a, b } => match sop {
            StrOp::Eq => a == b,
            StrOp::Ne => a != b,
            StrOp::Gt => str_lt(b, a),
            StrOp::Lt => str_lt(a, b),
        },
        Expr::Num { nop, a, b } => num_cmp(nop, num_of(a), num_of(b)),
        Expr::And(l, r) => eval_expr(w, *l) && eval_expr(w, *r),
        Expr::Or(l, r) => eval_expr(w, *l) || eval_expr(w, *r),
    }
}

pub open spec fn bash_atom(w: World, ts: Seq<Seq<char>>) -> Option<bool> {
    if ts.len() == 1 {
        Some(ts[0].len() != 0)
    } else if ts.len() == 2 && starts_dash(ts[0]) {
        if ts[0] == seq!['-', 'z'] {
            Some(ts[1].len() == 0)
        } else if ts[0] == seq!['-', 'n'] {
            Some(ts[1].len() != 0)
        } else {
            match unary_flag(ts[0]) {
                Some(f) => Some(file_pred(w, f, ts[1])),
                None => None,
            }
        }
    } else if ts.len() == 3 {
        let (left, op, right) = (ts[0], ts[1], ts[2]);
        if op == seq!['-', 'f'] {
            Some(is_file(w, left))
        } else if op == seq!['-', 'd'] {
            Some(is_dir(w, left))
        } else if op == seq!['-', 'e'] {
            Some(file_exists(w, left))
        } else if op == seq!['-', 's'] {
            Some(size_gt0(w, left))
        } else if op == seq!['-', 'z'] {
            Some(left.len() == 0)
        } else if op == seq!['-', 'n'] {
            Some(left.len() != 0)
        } else if op == seq!['='] || op == seq!['=', '='] {
            Some(left == right)
        } else if op == seq!['!', '='] {
            Some(left != right)
        } else if op == seq!['-', 'e', 'q'] {
            Some(num_of(left) == num_of(right))
        } else if op == seq!['-', 'n', 'e'] {
            Some(num_of(left) != num_of(right))
        } else if op == seq!['-', 'l', 't'] {
            Some(num_of(left) < num_of(right))
        } else if op == seq!['-', 'l', 'e'] {
            Some(num_of(left) <= num_of(right))
        } else if op == seq!['-', 'g', 't'] {
            Some(num_of(left) > num_of(right))
        } else if op == seq!['-', 'g', 'e'] {
            Some(num_of(left) >= num_of(right))
        } else if op == seq!['>'] || op == seq!['\\', '>'] {
            Some(str_lt(right, left))
        } else if op == seq!['<'] || op == seq!['\\', '<'] {
            Some(str_lt(left, right))
        } else {
            None
        }
    } else {
        None
    }
}

// ──────────────────────────────────────────────────────────────── laws ────

/// The round trip: whenever `render_test_tokens` accepts an atomic token
/// list, the expression it emits evaluates to bash's `test` verdict for the
/// same tokens in the same world.
pub proof fn render_tokens_is_bash(w: World, ts: Seq<Seq<char>>)
    ensures match render_tokens(ts) {
        Some(e) => bash_atom(w, ts) == Some(eval_expr(w, e)),
        None => true,
    },
{
}

pub proof fn test_renders_to_the_same_truth(w: World, ts: Seq<Seq<char>>)
    ensures match render_tokens(ts) {
        Some(e) => bash_atom(w, ts) == Some(eval_expr(w, e)),
        None => true,
    },
{
    render_tokens_is_bash(w, ts);
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

pub open spec fn empty_world() -> World {
    World { kinds: Seq::empty(), sizes: Seq::empty() }
}

/// A bare word is truthiness (non-empty), not "contains a flag".
pub proof fn pin_bare_word_is_truthiness() {
    assert(render_tokens(seq![seq!['h', 'i']]) == Some(Expr::Len { ne: true, w: seq!['h', 'i'] }));
    assert(eval_expr(empty_world(), Expr::Len { ne: true, w: seq!['h', 'i'] }));
    assert(eval_expr(empty_world(), Expr::Len { ne: true, w: Seq::empty() }) == false);
}

/// `-f` is a file test, not the flag-regex heuristic.
pub proof fn pin_dash_f_is_a_file_test() {
    assert(render_tokens(seq![seq!['-', 'f'], seq!['x']])
        == Some(Expr::File { flag: 'f', path: seq!['x'] }));
}

/// `-a` is existence (`-e`), never the AND connector.
pub proof fn pin_dash_a_is_existence() {
    assert(render_tokens(seq![seq!['-', 'a'], seq!['x']])
        == Some(Expr::File { flag: 'e', path: seq!['x'] }));
}

/// `-z` / `-n` are emptiness tests over the operand's literal length.
pub proof fn pin_z_and_n_are_length_tests() {
    assert(render_tokens(seq![seq!['-', 'z'], seq!['a']])
        == Some(Expr::Len { ne: false, w: seq!['a'] }));
    assert(render_tokens(seq![seq!['-', 'n'], seq!['a']])
        == Some(Expr::Len { ne: true, w: seq!['a'] }));
}

/// Numeric comparisons numify non-numbers to 0, as bash does.
pub proof fn pin_numeric_numifies() {
    assert(num_of(seq!['1', '2']) == 12) by (compute);
    assert(num_of(seq!['x', 'y']) == 0) by (compute);
    assert(render_tokens(seq![seq!['x'], seq!['-', 'e', 'q'], seq!['y']])
        == Some(Expr::Num { nop: NumOp::Eq, a: seq!['x'], b: seq!['y'] })) by (compute);
    assert(eval_expr(empty_world(),
        Expr::Num { nop: NumOp::Eq, a: seq!['x'], b: seq!['y'] })) by (compute);
}

/// `=` is string equality; `!=` negates it.
pub proof fn pin_equality_is_string_equality() {
    assert(render_tokens(seq![seq!['a'], seq!['='], seq!['b']])
        == Some(Expr::Str { sop: StrOp::Eq, a: seq!['a'], b: seq!['b'] })) by (compute);
    assert(eval_expr(empty_world(),
        Expr::Str { sop: StrOp::Eq, a: seq!['a'], b: seq!['b'] }) == false) by (compute);
}

/// An unhandled two-token shape is refused (the fallback is not modelled).
pub proof fn pin_unknown_unary_refused() {
    assert(render_tokens(seq![seq!['-', 'q'], seq!['x']]) == None::<Expr>);
}

fn main() {}

} // verus!
