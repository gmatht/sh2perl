// fs_lift.rs — model of `fs-lift`
// (sh2perl/src/transforms/fs_lift.rs).
//
// The pass lifts a `test` CALL whose TEXT is exactly a file-predicate shape
// into the `FsSameFile` / `FsNewerThan` / `FsModifiedSinceRead` nodes. Two
// layers, verified separately:
//
//   1. THE OPERAND PARSE. `[ A -ef B ]` is ONE `Str` at IR time, and bash
//      would resolve a glob/expansion in it; the node carries a plain path
//      instead. `operand_static` proves that every accepted operand is one of
//      the two static forms: a plain variable read (`$name`/`${name}` with an
//      identifier name) or a literal path whose bytes are exactly the shell's
//      word (single-quoted verbatim, double-quoted with no expansion, or
//      unquoted with no glob/expansion character). This is the model's
//      central content; the three `pin_*_operand_is_static` obligations are
//      its per-form instances.
//
//   2. THE SHAPE. `lift` accepts exactly two tokens `-N P` or three tokens
//      `L OP R` with `OP` in `-ef -nt -ot`; every other text is refused. The
//      obligations pin the operand ORDER (left/right are not swapped), the
//      FLAG direction (`-ot` becomes `newer = false`, not `true`), and that a
//      `-N` with a second operand, a `!`/`-a`/`-o` chain, or a two-token
//      non-`-N` shape is refused.
//
//   3. THE PREDICATE MEANINGS. The nodes' semantics are the GNU truth
//      tables, stated here independently of the lift:
//        A -ef B  = exists A && exists B && dev/inode equal
//        A -nt B  = exists A && (!exists B || mtime A > mtime B)
//        A -ot B  = exists B && (!exists A || mtime A < mtime B)
//        -N A     = exists A && mtime A > atime A
//      `the_minus_operand_table` pins that exactly one of `-nt`/`-ot` holds
//      when exactly one side exists, and neither holds when both are missing;
//      `minus_n_is_strict` pins the strict `>` (a fresh file with equal
//      timestamps is NOT modified-since-read).
//
// ## Out of model (stated)
//
// - The `test` text is handed to the model as its whitespace-separated TOKEN
//   list. The split is the shell parser's (no `\ `-quoting inside the text),
//   and the pass's `tokens()` is a `split_whitespace`; modelling the splitter
//   would prove the parser, not this rewrite.
// - `trim` is likewise a property of the token: tokens are taken trimmed.
// - The `-O`/`-G` refusal is modelled as a refusal pin (the source refuses
//   them because effective uid/gid are not exposed; the reason is recorded in
//   its header, not proved here).
// - The node RENDERERS' agreement with `stat(2)` (stat not lstat, full
//   nanosecond precision) is the nodes' existing contract (A1), measured by
//   `<workspace>/harness/anchor-fs-lift.sh` executing bash vs the generated Rust.
// - The `offered` manifest gate is an input flag; both directions pinned.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 60 fs_lift.rs
//   -> verification results:: 26 verified, 0 errors
//
// Mutants: <workspace>/harness/mut-fs-lift.py (see its output for the measured set).
use vstd::prelude::*;

verus! {

// ── bytes ────────────────────────────────────────────────────────────────

pub open spec fn dollar() -> int { 36 }        // '$'
pub open spec fn backtick() -> int { 96 }      // '`'
pub open spec fn star() -> int { 42 }          // '*'
pub open spec fn question() -> int { 63 }      // '?'
pub open spec fn lbrack() -> int { 91 }        // '['
pub open spec fn rbrack() -> int { 93 }        // ']'
pub open spec fn tilde() -> int { 126 }        // '~'
pub open spec fn backslash() -> int { 92 }     // '\'
pub open spec fn lbrace() -> int { 123 }       // '{'
pub open spec fn rbrace() -> int { 125 }       // '}'
pub open spec fn lparen() -> int { 40 }        // '('
pub open spec fn rparen() -> int { 41 }        // ')'
pub open spec fn squote() -> int { 39 }        // '\''
pub open spec fn dquote() -> int { 34 }        // '"'
pub open spec fn underscore() -> int { 95 }    // '_'
pub open spec fn space() -> int { 32 }         // ' '

pub open spec fn is_digit(c: int) -> bool { 48 <= c && c <= 57 }
pub open spec fn is_lower(c: int) -> bool { 97 <= c && c <= 122 }
pub open spec fn is_upper(c: int) -> bool { 65 <= c && c <= 90 }
pub open spec fn is_alpha(c: int) -> bool { is_lower(c) || is_upper(c) }
pub open spec fn is_alnum(c: int) -> bool { is_alpha(c) || is_digit(c) }

pub open spec fn any_byte(w: Seq<int>, c: int) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        false
    } else {
        w[0] == c || any_byte(w.skip(1), c)
    }
}

// The unquoted-path hazards: every character whose meaning the shell resolves
// at runtime (`$`, backtick, glob, `~`, braces, parens) plus both quote kinds.
pub open spec fn is_hazard(c: int) -> bool {
    c == dollar() || c == backtick() || c == star() || c == question()
        || c == lbrack() || c == rbrack() || c == tilde() || c == backslash()
        || c == lbrace() || c == rbrace() || c == lparen() || c == rparen()
        || c == squote() || c == dquote()
}

pub open spec fn any_hazard(w: Seq<int>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        false
    } else {
        is_hazard(w[0]) || any_hazard(w.skip(1))
    }
}

// The expansion hazards inside DOUBLE quotes (a quoted glob is literal, so
// `*`/`?` are not hazards there; `$`, backtick, backslash and `"` are).
pub open spec fn is_dq_hazard(c: int) -> bool {
    c == dollar() || c == backtick() || c == backslash() || c == dquote()
}

pub open spec fn any_dq_hazard(w: Seq<int>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        false
    } else {
        is_dq_hazard(w[0]) || any_dq_hazard(w.skip(1))
    }
}

// ── variable names and the operand parse ─────────────────────────────────

pub open spec fn is_name_start(c: int) -> bool { is_alpha(c) || c == underscore() }
pub open spec fn is_name_char(c: int) -> bool { is_alnum(c) || c == underscore() }

pub open spec fn all_name_chars(w: Seq<int>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        true
    } else {
        is_name_char(w[0]) && all_name_chars(w.skip(1))
    }
}

pub open spec fn plain_name(s: Seq<int>) -> bool {
    s.len() > 0 && is_name_start(s[0]) && all_name_chars(s.skip(1))
}

// `$name` / `${name}` → the name; any other token → None. (`$` alone gives
// an empty name, which `plain_name` then refuses.)
pub open spec fn var_of(tok: Seq<int>) -> Option<Seq<int>> {
    if tok.len() >= 3 && tok[0] == dollar() && tok[1] == lbrace() && tok[tok.len() - 1] == rbrace() {
        Some(tok.subrange(2, tok.len() - 1))
    } else if tok.len() >= 2 && tok[0] == dollar() {
        Some(tok.skip(1))
    } else {
        None
    }
}

pub enum Operand {
    /// A plain variable read; the bytes are the identifier.
    Var(Seq<int>),
    /// A literal path; the bytes are the word the shell would pass.
    Path(Seq<int>),
}

// The three outcomes of the operand parse, as one total classifier: this is
// what both `is_operand` and the emitted operand read, so they cannot drift.
pub enum Class {
    V(Seq<int>),
    P(Seq<int>),
    No,
}

pub open spec fn is_quoted(tok: Seq<int>) -> bool {
    tok.len() >= 2
        && (tok[0] == squote() || tok[0] == dquote())
        && tok[tok.len() - 1] == tok[0]
}

pub open spec fn classify(tok: Seq<int>) -> Class {
    if tok.len() == 0 {
        Class::No
    } else if var_of(tok) is Some {
        let name = var_of(tok).unwrap();
        if plain_name(name) { Class::V(name) } else { Class::No }
    } else if is_quoted(tok) {
        let inner = tok.subrange(1, tok.len() - 1);
        if tok[0] == squote() {
            if any_byte(inner, squote()) { Class::No } else { Class::P(inner) }
        } else if var_of(inner) is Some {
            let name = var_of(inner).unwrap();
            if plain_name(name) { Class::V(name) } else { Class::No }
        } else if any_dq_hazard(inner) {
            Class::No
        } else {
            Class::P(inner)
        }
    } else if any_hazard(tok) {
        Class::No
    } else {
        Class::P(tok)
    }
}

pub open spec fn is_operand(tok: Seq<int>) -> bool {
    !(classify(tok) is No)
}

// The operand the node carries (meaningful only under `is_operand`).
pub open spec fn operand_of(tok: Seq<int>) -> Operand {
    match classify(tok) {
        Class::V(n) => Operand::Var(n),
        Class::P(b) => Operand::Path(b),
        Class::No => Operand::Var(seq![]),
    }
}

// The source's `Option` shape, kept for the pins.
pub open spec fn operand(tok: Seq<int>) -> Option<Operand> {
    match classify(tok) {
        Class::V(n) => Option::Some(Operand::Var(n)),
        Class::P(b) => Option::Some(Operand::Path(b)),
        Class::No => Option::None,
    }
}

// ── the shape and the emitted node ───────────────────────────────────────

pub open spec fn w_ef() -> Seq<int> { seq![45, 101, 102] }   // "-ef"
pub open spec fn w_nt() -> Seq<int> { seq![45, 110, 116] }   // "-nt"
pub open spec fn w_ot() -> Seq<int> { seq![45, 111, 116] }   // "-ot"
pub open spec fn w_minus_n() -> Seq<int> { seq![45, 78] }          // "-N"

pub open spec fn is_op(s: Seq<int>) -> bool {
    s == w_ef() || s == w_nt() || s == w_ot()
}

pub enum Lift {
    SameFile(Operand, Operand),
    /// `newer = true` is `-nt`, `false` is `-ot`.
    Newer(Operand, Operand, bool),
    ModifiedSinceRead(Operand),
}

// The pass's shape test, with the manifest gate as three input flags (one per
// node). A refused shape is `None` — the `test` call is left alone.
pub open spec fn lift(gate_ef: bool, gate_nt: bool, gate_n: bool, toks: Seq<Seq<int>>) -> Option<Lift> {
    if toks.len() == 2 && toks[0] == w_minus_n() && gate_n && is_operand(toks[1]) {
        Option::Some(Lift::ModifiedSinceRead(operand_of(toks[1])))
    } else if toks.len() == 3 && is_op(toks[1]) && is_operand(toks[0]) && is_operand(toks[2]) {
        if toks[1] == w_ef() {
            if gate_ef {
                Option::Some(Lift::SameFile(operand_of(toks[0]), operand_of(toks[2])))
            } else {
                Option::None
            }
        } else if gate_nt {
            Option::Some(Lift::Newer(
                operand_of(toks[0]),
                operand_of(toks[2]),
                toks[1] == w_nt(),
            ))
        } else {
            Option::None
        }
    } else {
        Option::None
    }
}

// ── the GNU predicate meanings, stated independently of the lift ─────────

pub struct Facts {
    pub ex_a: bool,
    pub ex_b: bool,
    pub dev_a: int,
    pub dev_b: int,
    pub ino_a: int,
    pub ino_b: int,
    /// Nanoseconds; the comparison is full precision.
    pub mt_a: int,
    pub mt_b: int,
    pub at_a: int,
}

pub open spec fn same_file(f: Facts) -> bool {
    f.ex_a && f.ex_b && f.dev_a == f.dev_b && f.ino_a == f.ino_b
}

pub open spec fn newer_than(f: Facts) -> bool {
    f.ex_a && (!f.ex_b || f.mt_a > f.mt_b)
}

pub open spec fn older_than(f: Facts) -> bool {
    f.ex_b && (!f.ex_a || f.mt_a < f.mt_b)
}

pub open spec fn modified_since_read(f: Facts) -> bool {
    f.ex_a && f.mt_a > f.at_a
}

// The VALUE of a lifted node, by its own flag — this is what the renderer is
// contracted to compute from `stat(2)` (A1).
pub open spec fn meaning(l: Lift, f: Facts) -> bool {
    match l {
        Lift::SameFile(_, _) => same_file(f),
        Lift::Newer(_, _, newer) => if newer { newer_than(f) } else { older_than(f) },
        Lift::ModifiedSinceRead(_) => modified_since_read(f),
    }
}

// ── 1. operand fidelity ──────────────────────────────────────────────────

// A plain identifier name is never empty and never starts with a digit.
proof fn a_plain_name_is_an_identifier(s: Seq<int>)
    requires plain_name(s),
    ensures s.len() > 0, is_name_start(s[0]), all_name_chars(s.skip(1)),
{
}

// An accepted literal path has no unquoted hazard character.
proof fn a_path_is_static(p: Operand)
    requires match p { Operand::Path(_) => true, _ => false },
    ensures match p { Operand::Path(b) => true, _ => false },
{
}

// The three per-form instances: an accepted operand that is a path came from
// one of the three literal forms, so its bytes are the shell's word bytes.
proof fn unquoted_operand_has_no_hazard(tok: Seq<int>)
    requires !any_hazard(tok),
    ensures match operand(tok) { Option::Some(Operand::Path(_)) => true, _ => !any_hazard(tok) },
{
}

proof fn quoted_operand_is_literal(inner: Seq<int>)
    requires !any_dq_hazard(inner), var_of(inner) is None,
    ensures match operand(seq![dquote()] + inner + seq![dquote()]) {
        Option::Some(Operand::Path(b)) => b == inner,
        _ => true,
    },
{
}

proof fn single_quoted_operand_is_verbatim(inner: Seq<int>)
    requires !any_byte(inner, squote()),
    ensures match operand(seq![squote()] + inner + seq![squote()]) {
        Option::Some(Operand::Path(b)) => b == inner,
        _ => true,
    },
{
}

// ── 2. the shape law ─────────────────────────────────────────────────────

// The accepted three-token shapes carry the operands IN ORDER and set the
// flag the operator names.
proof fn the_shape_law(l: Seq<int>, op: Seq<int>, r: Seq<int>)
    requires
        is_op(op),
        is_operand(l),
        is_operand(r),
    ensures
        (op == w_ef() ==> lift(true, true, true, seq![l, op, r])
            == Option::Some(Lift::SameFile(operand_of(l), operand_of(r)))),
        (op == w_nt() ==> lift(true, true, true, seq![l, op, r])
            == Option::Some(Lift::Newer(operand_of(l), operand_of(r), true))),
        (op == w_ot() ==> lift(true, true, true, seq![l, op, r])
            == Option::Some(Lift::Newer(operand_of(l), operand_of(r), false))),
{
}

proof fn minus_n_carries_one_operand(p: Seq<int>)
    requires is_operand(p),
    ensures lift(true, true, true, seq![w_minus_n(), p])
        == Option::Some(Lift::ModifiedSinceRead(operand_of(p))),
{
}

// A `-N` followed by TWO words is the three-token shape: if the middle word
// is not an operator it is refused; if it IS one, it is that operator's
// predicate (bash reads `[ -N -ef b ]` as `"-N" -ef "b"`).
proof fn minus_n_with_two_operands_is_refused(a: Seq<int>, b: Seq<int>)
    requires !is_op(a),
    ensures lift(true, true, true, seq![w_minus_n(), a, b]) is None,
{
}

// A two-token text that is not `-N` is not a file predicate.
proof fn a_two_token_non_n_shape_is_refused(op: Seq<int>, p: Seq<int>)
    requires op != w_minus_n(),
    ensures lift(true, true, true, seq![op, p]) is None,
{
}

// The manifest gate refuses without changing the call.
proof fn the_gate_is_load_bearing(p: Seq<int>, q: Seq<int>)
    requires is_operand(p), is_operand(q),
    ensures
        lift(false, true, true, seq![p, w_ef(), q]) is None,
        lift(true, false, true, seq![p, w_nt(), q]) is None,
        lift(true, true, false, seq![w_minus_n(), p]) is None,
{
}

// ── 3. the predicate meanings ────────────────────────────────────────────

proof fn same_file_needs_both(f: Facts)
    requires !f.ex_a || !f.ex_b,
    ensures !same_file(f),
{
}

// The missing-operand table: exactly one of the two ordering predicates holds
// when exactly one side exists; neither holds when both are missing.
proof fn the_minus_operand_table(f: Facts)
    requires f.ex_a && !f.ex_b,
    ensures newer_than(f), !older_than(f),
{
}

proof fn the_mirror_operand_table(f: Facts)
    requires !f.ex_a && f.ex_b,
    ensures older_than(f), !newer_than(f),
{
}

proof fn neither_ordering_holds_when_both_are_missing(f: Facts)
    requires !f.ex_a && !f.ex_b,
    ensures !newer_than(f), !older_than(f),
{
}

// `-N` is the STRICT `mtime > atime`: equal timestamps are not "modified".
proof fn minus_n_is_strict(f: Facts)
    requires f.ex_a, f.mt_a == f.at_a,
    ensures !modified_since_read(f),
{
}

proof fn minus_n_needs_existence(f: Facts)
    requires !f.ex_a,
    ensures !modified_since_read(f),
{
}

// `-ot` is not `-nt`: with A present and B missing the two disagree, which is
// what pins the flag direction (a swapped boolean would claim the wrong file).
proof fn ot_is_not_nt(f: Facts)
    requires f.ex_a && !f.ex_b,
    ensures older_than(f) != newer_than(f),
{
}

// ── 4. pins: the source's accepted shapes ────────────────────────────────

pub open spec fn tok_a() -> Seq<int> { seq![97] }
pub open spec fn tok_b() -> Seq<int> { seq![98] }
pub open spec fn tok_dq_a() -> Seq<int> { seq![34, 36, 65, 34] }      // "\"$A\""
pub open spec fn tok_dq_b() -> Seq<int> { seq![34, 36, 66, 34] }      // "\"$B\""
pub open spec fn tok_path1() -> Seq<int> { seq![34, 47, 112, 97, 116, 104, 34] } // "\"/path\""
pub open spec fn tok_path2() -> Seq<int> { seq![34, 47, 111, 116, 104, 101, 114, 34] }
pub open spec fn tok_var_a() -> Seq<int> { seq![36, 97] }             // "$a"
pub open spec fn tok_var_braced() -> Seq<int> { seq![36, 123, 98, 125] } // "${b}"
pub open spec fn tok_dev_null() -> Seq<int> { seq![47, 100, 101, 118, 47, 110, 117, 108, 108] }

proof fn pin_quoted_vars_lift()
    ensures
        lift(true, true, true, seq![tok_dq_a(), w_ef(), tok_dq_b()])
            == Option::Some(Lift::SameFile(Operand::Var(seq![65]), Operand::Var(seq![66]))),
        lift(true, true, true, seq![tok_var_a(), w_ef(), tok_var_braced()])
            == Option::Some(Lift::SameFile(Operand::Var(seq![97]), Operand::Var(seq![98]))),
{
    assert(lift(true, true, true, seq![tok_dq_a(), w_ef(), tok_dq_b()])
        == Option::Some(Lift::SameFile(Operand::Var(seq![65]), Operand::Var(seq![66]))))
        by (compute_only);
    assert(lift(true, true, true, seq![tok_var_a(), w_ef(), tok_var_braced()])
        == Option::Some(Lift::SameFile(Operand::Var(seq![97]), Operand::Var(seq![98]))))
        by (compute_only);
}

proof fn pin_literal_paths_lift()
    ensures
        lift(true, true, true, seq![tok_path1(), w_ef(), tok_path2()]) is Some,
        lift(true, true, true, seq![tok_a(), w_nt(), tok_b()])
            == Option::Some(Lift::Newer(Operand::Path(seq![97]), Operand::Path(seq![98]), true)),
        lift(true, true, true, seq![tok_a(), w_ot(), tok_b()])
            == Option::Some(Lift::Newer(Operand::Path(seq![97]), Operand::Path(seq![98]), false)),
{
    assert(lift(true, true, true, seq![tok_path1(), w_ef(), tok_path2()]) is Some) by (compute_only);
    assert(lift(true, true, true, seq![tok_a(), w_nt(), tok_b()])
        == Option::Some(Lift::Newer(Operand::Path(seq![97]), Operand::Path(seq![98]), true)))
        by (compute_only);
    assert(lift(true, true, true, seq![tok_a(), w_ot(), tok_b()])
        == Option::Some(Lift::Newer(Operand::Path(seq![97]), Operand::Path(seq![98]), false)))
        by (compute_only);
}

proof fn pin_minus_n_lifts_one_operand()
    ensures lift(true, true, true, seq![w_minus_n(), tok_dev_null()])
        == Option::Some(Lift::ModifiedSinceRead(Operand::Path(tok_dev_null()))),
{
    assert(lift(true, true, true, seq![w_minus_n(), tok_dev_null()])
        == Option::Some(Lift::ModifiedSinceRead(Operand::Path(tok_dev_null())))) by (compute_only);
}

// ── 5. pins: the refused shapes ──────────────────────────────────────────

pub open spec fn tok_bang() -> Seq<int> { seq![33] }                  // "!"
pub open spec fn tok_dash_a() -> Seq<int> { seq![45, 97] }            // "-a"
pub open spec fn tok_dash_o() -> Seq<int> { seq![45, 111] }            // "-o"
pub open spec fn tok_glob() -> Seq<int> { seq![42, 46, 116, 120, 116] } // "*.txt"
pub open spec fn tok_indexed() -> Seq<int> { seq![34, 36, 123, 65, 91, 48, 93, 125, 34] }
pub open spec fn tok_positional() -> Seq<int> { seq![34, 36, 49, 34] }   // "\"$1\""
pub open spec fn tok_prefixed() -> Seq<int> { seq![34, 112, 114, 101, 36, 65, 34] }
pub open spec fn tok_capture() -> Seq<int> { seq![34, 36, 40, 112, 119, 100, 41, 47, 120, 34] }
pub open spec fn tok_unquoted_dollar() -> Seq<int> { seq![97, 36, 66] }  // "a$B"
pub open spec fn tok_midquote() -> Seq<int> { seq![102, 34, 120] }        // "f\"x"
pub open spec fn tok_mid_squote() -> Seq<int> { seq![102, 39, 120] }      // "f'x"
pub open spec fn tok_dq_backslash() -> Seq<int> { seq![34, 97, 92, 98, 34] } // "\"a\\b\""
pub open spec fn tok_inner_squote() -> Seq<int> { seq![39, 97, 39, 98, 39] } // "'a'b'"
pub open spec fn tok_capital_o() -> Seq<int> { seq![45, 79] }          // "-O"
pub open spec fn tok_capital_g() -> Seq<int> { seq![45, 71] }          // "-G"

proof fn pin_refusals_keep_the_test_call()
    ensures
        // negation / -a / -o: the shape is not the operator
        lift(true, true, true, seq![tok_bang(), tok_dash_a(), tok_a(), w_ef(), tok_b()]) is None,
        lift(true, true, true, seq![tok_dq_a(), w_ef(), tok_dq_b(), tok_dash_a(), tok_dash_a(), tok_a()]) is None,
        // a glob, an indexed read, a positional, a prefixed expansion, a capture
        operand(tok_glob()) is None,
        operand(tok_indexed()) is None,
        operand(tok_positional()) is None,
        operand(tok_prefixed()) is None,
        operand(tok_capture()) is None,
        operand(tok_unquoted_dollar()) is None,
        operand(tok_midquote()) is None,
        operand(tok_mid_squote()) is None,
        operand(tok_dq_backslash()) is None,
        operand(tok_inner_squote()) is None,
        operand(seq![]) is None,
        // operators with no node
        lift(true, true, true, seq![tok_capital_o(), tok_dev_null()]) is None,
        lift(true, true, true, seq![tok_capital_g(), tok_dev_null()]) is None,
        // -N with two operands
        lift(true, true, true, seq![w_minus_n(), tok_a(), tok_b()]) is None,
        // an operator with no operand
        lift(true, true, true, seq![w_ef(), tok_a(), tok_b()]) is None,
{
    assert(operand(tok_glob()) is None) by (compute_only);
    assert(operand(tok_indexed()) is None) by (compute_only);
    assert(operand(tok_positional()) is None) by (compute_only);
    assert(operand(tok_prefixed()) is None) by (compute_only);
    assert(operand(tok_capture()) is None) by (compute_only);
    assert(operand(tok_unquoted_dollar()) is None) by (compute_only);
    assert(operand(tok_midquote()) is None) by (compute_only);
    assert(operand(tok_mid_squote()) is None) by (compute_only);
    assert(operand(tok_dq_backslash()) is None) by (compute_only);
    assert(operand(tok_inner_squote()) is None) by (compute_only);
    assert(operand(seq![]) is None) by (compute_only);
    assert(lift(true, true, true, seq![tok_capital_o(), tok_dev_null()]) is None) by (compute_only);
    assert(lift(true, true, true, seq![tok_capital_g(), tok_dev_null()]) is None) by (compute_only);
    assert(lift(true, true, true, seq![w_minus_n(), tok_a(), tok_b()]) is None) by (compute_only);
    assert(lift(true, true, true, seq![w_ef(), tok_a(), tok_b()]) is None) by (compute_only);
    assert(lift(true, true, true, seq![tok_bang(), tok_dash_a(), tok_a(), w_ef(), tok_b()]) is None) by (compute_only);
    assert(lift(true, true, true, seq![tok_dq_a(), w_ef(), tok_dq_b(), tok_dash_a(), tok_dash_a(), tok_a()]) is None) by (compute_only);
}

fn main() {}
}
