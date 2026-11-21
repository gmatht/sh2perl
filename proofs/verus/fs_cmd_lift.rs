// fs_cmd_lift.rs — model of `fs-cmd-lift`
// (sh2perl/src/transforms/fs_cmd_lift.rs).
//
// The pass lifts a STATEMENT-position `exec CMD …` (`rm`, `mkdir`, `touch`)
// into the `FsCommand` node. Two different things can go wrong, and they are
// separate obligations:
//
//   1. THE ARGV MUST BE STATIC. bash resolves a glob/expansion/quote word at
//      runtime, while the IR holds it as one `Str`; a node rendered from that
//      `Str` would pass the pattern itself, not the files it names. The pass
//      refuses every word outside `plain_word`'s character set, and
//      `a_plain_word_has_no_hazard_char` proves that refusal is exactly what
//      keeps the emitted argv equal to the argv bash would pass: a word built
//      only from the plain characters contains NO character outside them, so
//      it contains no `*`, `?`, `$`, space, quote, backslash or `;`.
//
//   2. THE NODE MUST CARRY THE SAME COMMAND AND ARGV, IN THE SAME PLACES.
//      `the_walk_preserves_command_and_argv` proves the statement walk is a
//      TAG-ONLY image: the tree shape and the (cmd, argv) sequence at every
//      leaf are unchanged (a dropped/reordered/renamed word is red). The
//      complement is `every_liftable_exec_is_a_node` (no liftable exec is
//      left behind anywhere in the tree) and `only_legal_nodes_are_built`
//      (nothing that the renderer cannot spell becomes a node).
//
//   3. THE ACCEPTANCE TABLE IS THE RENDERER'S TABLE. `rm` allows f,r,R,d;
//      `mkdir` p,v; `touch` none; a `--long` form and a lone `-` are refused.
//      Each letter is a separate pin, so widening one table is red.
//
// ## The renderer contract (A1 — stated, not proved)
//
// The `FsCommand` node's Rust arm reuses the backend's `native_fs_stmt`
// renderer, which was already the route for these commands. The theorem below
// is about the LIFT (the same command and argv reach that renderer); the
// renderer's own job — reproducing bash's status aggregation, the `-f`
// missing-path rule and the diagnostics — is the node's pre-existing contract
// and is measured end to end by `harness/anchor-fs-cmd-lift.sh` (bash vs the
// generated C and Perl, byte for byte and status for status). No part of this
// model claims to prove std::fs equals rm(1).
//
// ## Scope
//
// - Words are `Seq<int>` byte strings; a `Word` is either a literal `Str` or
//   a dynamic expression (`Dyn`). The source's `IrExpr::Str`/anything-else
//   distinction is exactly this two-way split.
// - The statement tree covers the source's walker arms: Exec leaves, Seq,
//   If/elsif/else, While, Block/Subshell/Background and Function bodies
//   (all body lists are flattened into the same `Stmt` shapes here; the walk
//   treats every body identically, and the model's If has then/else).
// - `offered` (the env/manifest gate) is an input boolean to `transform`;
//   both directions are pinned.
// - Not modelled: the node's rendering (A1), the filesystem, and the
//   `args.get(1)` being absent-vs-empty distinction (both refuse: the model's
//   empty word list is the pin `an_empty_argv_is_refused`).
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 60 fs_cmd_lift.rs
//   -> verification results:: 30 verified, 0 errors
//
// Mutants (checked, measured on copies by harness/mut-fs-cmd-lift.py):
//   see the harness output; every mutant is red on a named obligation here.
use vstd::prelude::*;

verus! {

// ── bytes and the plain-word character set ───────────────────────────────

pub open spec fn dash() -> int { 45 }          // '-'
pub open spec fn star() -> int { 42 }          // '*'
pub open spec fn question() -> int { 63 }      // '?'
pub open spec fn dollar() -> int { 36 }        // '$'
pub open spec fn space() -> int { 32 }         // ' '
pub open spec fn backslash() -> int { 92 }     // '\'
pub open spec fn semi() -> int { 59 }          // ';'

pub open spec fn is_digit(c: int) -> bool { 48 <= c && c <= 57 }
pub open spec fn is_lower(c: int) -> bool { 97 <= c && c <= 122 }
pub open spec fn is_upper(c: int) -> bool { 65 <= c && c <= 90 }
pub open spec fn is_alnum(c: int) -> bool { is_digit(c) || is_lower(c) || is_upper(c) }

// `"/._-+:,@=~"` — the source's literal set, as ten disjuncts so the
// predicate is decidable by computation (no existential).
pub open spec fn is_plain_punct(c: int) -> bool {
    c == 47 || c == 46 || c == 95 || c == 45 || c == 43
        || c == 58 || c == 44 || c == 64 || c == 61 || c == 126
}

pub open spec fn is_plain_char(c: int) -> bool {
    is_alnum(c) || is_plain_punct(c)
}

pub open spec fn contains_byte(w: Seq<int>, c: int) -> bool {
    exists|i: int| 0 <= i < w.len() && w[i] == c
}

// Every byte of the word is plain (structural, so concrete words compute).
pub open spec fn all_plain(w: Seq<int>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        true
    } else {
        is_plain_char(w[0]) && all_plain(w.skip(1))
    }
}

// `plain_word`: non-empty, not the lone `-`, every character plain.
pub open spec fn plain_word(w: Seq<int>) -> bool {
    w.len() > 0 && w != seq![dash()] && all_plain(w)
}

// ── the command table and the flag table ─────────────────────────────────

pub open spec fn w_rm() -> Seq<int> { seq![114, 109] }              // "rm"
pub open spec fn w_mkdir() -> Seq<int> { seq![109, 107, 100, 105, 114] } // "mkdir"
pub open spec fn w_touch() -> Seq<int> { seq![116, 111, 117, 99, 104] }  // "touch"

pub open spec fn cmd_static(cmd: Seq<int>) -> bool {
    cmd == w_rm() || cmd == w_mkdir() || cmd == w_touch()
}

// `rm`: f, r, R, d.  `mkdir`: p, v.  `touch`: none.
pub open spec fn allowed(cmd: Seq<int>, c: int) -> bool {
    (cmd == w_rm() && (c == 102 || c == 114 || c == 82 || c == 100))
        || (cmd == w_mkdir() && (c == 112 || c == 118))
}

// The letters of a `-flag` from `i` on are all in the command's table.
pub open spec fn letters_ok(cmd: Seq<int>, w: Seq<int>, i: int) -> bool
    decreases w.len() - i,
{
    if i >= w.len() {
        true
    } else {
        allowed(cmd, w[i]) && letters_ok(cmd, w, i + 1)
    }
}

// One word's flag check, exactly the source's branches:
//   * `--` is the separator and is skipped;
//   * a lone `-` is an OPERAND (it continues here, and `plain_word` refuses
//     it because no operand of these commands may be `-`);
//   * any other `-` word must have every letter in the command's table;
//   * a `--long` form is refused.
pub open spec fn flag_ok(cmd: Seq<int>, w: Seq<int>) -> bool {
    if w == seq![dash(), dash()] {
        true
    } else if w.len() > 0 && w[0] == dash() {
        if w.len() == 1 {
            true
        } else if w[1] == dash() {
            false
        } else {
            letters_ok(cmd, w, 1)
        }
    } else {
        true
    }
}

pub enum Word {
    Lit(Seq<int>),
    Dyn,
}

pub open spec fn is_lit(w: Word) -> bool {
    match w {
        Word::Lit(_) => true,
        Word::Dyn => false,
    }
}

pub open spec fn word_text(w: Word) -> Seq<int> {
    match w {
        Word::Lit(s) => s,
        Word::Dyn => seq![],
    }
}

pub open spec fn words_plain(ws: Seq<Word>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        true
    } else {
        is_lit(ws[0]) && plain_word(word_text(ws[0])) && words_plain(ws.skip(1))
    }
}

pub open spec fn flags_ok(cmd: Seq<int>, ws: Seq<Word>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        true
    } else {
        is_lit(ws[0]) && flag_ok(cmd, word_text(ws[0])) && flags_ok(cmd, ws.skip(1))
    }
}

// ── the lift's acceptance predicate ──────────────────────────────────────

pub open spec fn liftable(cmd: Seq<int>, ws: Seq<Word>) -> bool {
    cmd_static(cmd) && ws.len() > 0 && words_plain(ws) && flags_ok(cmd, ws)
}

// ── the statement tree and the statement walk ────────────────────────────

pub enum Stmt {
    Exec(Seq<int>, Seq<Word>),
    Node(Seq<int>, Seq<Word>),
    Other(int),
    Seq(Box<Stmt>, Box<Stmt>),
    If(Box<Stmt>, Box<Stmt>, Box<Stmt>),
    While(Box<Stmt>),
    Block(Box<Stmt>),
    Fn(Box<Stmt>),
}

// The worked example: a liftable exec becomes the node carrying the SAME
// command and argv; every other shape is rebuilt with the walk applied.
pub open spec fn walk(st: Stmt) -> Stmt
    decreases st,
{
    match st {
        Stmt::Exec(cmd, ws) => {
            if liftable(cmd, ws) { Stmt::Node(cmd, ws) } else { Stmt::Exec(cmd, ws) }
        }
        Stmt::Node(cmd, ws) => Stmt::Node(cmd, ws),
        Stmt::Other(x) => Stmt::Other(x),
        Stmt::Seq(a, b) => Stmt::Seq(Box::new(walk(*a)), Box::new(walk(*b))),
        Stmt::If(c, t, e) => Stmt::If(Box::new(walk(*c)), Box::new(walk(*t)), Box::new(walk(*e))),
        Stmt::While(a) => Stmt::While(Box::new(walk(*a))),
        Stmt::Block(a) => Stmt::Block(Box::new(walk(*a))),
        Stmt::Fn(a) => Stmt::Fn(Box::new(walk(*a))),
    }
}

// The `offered` gate: no lift at all when the backend does not declare the
// node.
pub open spec fn transform(offered: bool, st: Stmt) -> Stmt
    decreases st,
{
    if offered { walk(st) } else { st }
}

// ── the two verification predicates ─────────────────────────────────────

// Every exec left in the tree is one the lift must refuse: no liftable exec
// survives anywhere (completeness of the walk).
pub open spec fn no_liftable_exec(st: Stmt) -> bool
    decreases st,
{
    match st {
        Stmt::Exec(cmd, ws) => !liftable(cmd, ws),
        Stmt::Node(_, _) => true,
        Stmt::Other(_) => true,
        Stmt::Seq(a, b) => no_liftable_exec(*a) && no_liftable_exec(*b),
        Stmt::If(c, t, e) => {
            no_liftable_exec(*c) && no_liftable_exec(*t) && no_liftable_exec(*e)
        }
        Stmt::While(a) => no_liftable_exec(*a),
        Stmt::Block(a) => no_liftable_exec(*a),
        Stmt::Fn(a) => no_liftable_exec(*a),
    }
}

// The pass starts from a tree with no node of its own kind (it is the first
// producer of `FsCommand`); a pre-existing node is outside this walk's input.
pub open spec fn no_nodes(st: Stmt) -> bool
    decreases st,
{
    match st {
        Stmt::Exec(_, _) => true,
        Stmt::Node(_, _) => false,
        Stmt::Other(_) => true,
        Stmt::Seq(a, b) => no_nodes(*a) && no_nodes(*b),
        Stmt::If(c, t, e) => no_nodes(*c) && no_nodes(*t) && no_nodes(*e),
        Stmt::While(a) => no_nodes(*a),
        Stmt::Block(a) => no_nodes(*a),
        Stmt::Fn(a) => no_nodes(*a),
    }
}

// Every node in the tree is one the acceptance predicate licenses.
pub open spec fn only_legal_nodes(st: Stmt) -> bool
    decreases st,
{
    match st {
        Stmt::Exec(_, _) => true,
        Stmt::Node(cmd, ws) => liftable(cmd, ws),
        Stmt::Other(_) => true,
        Stmt::Seq(a, b) => only_legal_nodes(*a) && only_legal_nodes(*b),
        Stmt::If(c, t, e) => {
            only_legal_nodes(*c) && only_legal_nodes(*t) && only_legal_nodes(*e)
        }
        Stmt::While(a) => only_legal_nodes(*a),
        Stmt::Block(a) => only_legal_nodes(*a),
        Stmt::Fn(a) => only_legal_nodes(*a),
    }
}

// The command/argv leaves of the tree, in order — the thing the lift must not
// touch (only the Exec/Node TAG may change).
pub open spec fn skeleton(st: Stmt) -> Seq<(Seq<int>, Seq<Word>)>
    decreases st,
{
    match st {
        Stmt::Exec(cmd, ws) => seq![(cmd, ws)],
        Stmt::Node(cmd, ws) => seq![(cmd, ws)],
        Stmt::Other(_) => seq![],
        Stmt::Seq(a, b) => skeleton(*a) + skeleton(*b),
        Stmt::If(c, t, e) => skeleton(*c) + skeleton(*t) + skeleton(*e),
        Stmt::While(a) => skeleton(*a),
        Stmt::Block(a) => skeleton(*a),
        Stmt::Fn(a) => skeleton(*a),
    }
}

// ── 1. the static-argv lemma ─────────────────────────────────────────────

// THE LEMMA: a plain word contains no character outside the plain set — so
// no glob, expansion, quote, space or separator can reach the node's argv.
proof fn all_plain_has_no_hazard_char(w: Seq<int>, c: int)
    requires all_plain(w), !is_plain_char(c),
    ensures !contains_byte(w, c),
    decreases w.len(),
{
    if w.len() > 0 {
        // the head is plain, so it is not the (non-plain) hazard byte
        assert(w[0] != c);
        all_plain_has_no_hazard_char(w.skip(1), c);
        if contains_byte(w, c) {
            let i = choose|i: int| 0 <= i < w.len() && w[i] == c;
            if i == 0 {
                assert(w[0] == c);
            } else {
                assert(0 <= i - 1 < w.skip(1).len() && w.skip(1)[i - 1] == c);
                assert(contains_byte(w.skip(1), c));
            }
        }
    }
}

proof fn a_plain_word_has_no_hazard_char(w: Seq<int>, c: int)
    requires plain_word(w), !is_plain_char(c),
    ensures !contains_byte(w, c),
{
    all_plain_has_no_hazard_char(w, c);
}

proof fn a_plain_word_is_never_the_lone_dash(w: Seq<int>)
    requires plain_word(w),
    ensures w != seq![dash()],
{
}

proof fn an_empty_word_is_not_plain(w: Seq<int>)
    requires w.len() == 0,
    ensures !plain_word(w),
{
}

// ── 2. the acceptance table ──────────────────────────────────────────────

// Any position inside an accepted flag's letter list is a modelled letter.
proof fn letters_ok_at(cmd: Seq<int>, w: Seq<int>, i: int, j: int)
    requires letters_ok(cmd, w, i), i <= j < w.len(),
    ensures allowed(cmd, w[j]),
    decreases w.len() - i,
{
    if i < w.len() {
        if i == j {
        } else {
            letters_ok_at(cmd, w, i + 1, j);
        }
    }
}

// An accepted flag word's letters are all in the command's modelled table.
proof fn an_accepted_flag_letter_is_modelled(cmd: Seq<int>, w: Seq<int>, i: int)
    requires
        flag_ok(cmd, w),
        w.len() > 1,
        w[0] == dash(),
        w[1] != dash(),
        1 <= i < w.len(),
    ensures allowed(cmd, w[i]),
{
    letters_ok_at(cmd, w, 1, i);
}

// An accepted word that starts with `-` is not a `--long` form.
proof fn a_long_flag_is_never_accepted(cmd: Seq<int>, w: Seq<int>)
    requires flag_ok(cmd, w), w.len() > 2, w[0] == dash(), w[1] == dash(),
    ensures false,
{
}

proof fn a_separator_is_not_a_flag(cmd: Seq<int>)
    ensures flag_ok(cmd, seq![dash(), dash()]),
{
}

// ── 3. the walk preserves the command and argv ───────────────────────────

proof fn skeleton_walk_leaf(cmd: Seq<int>, ws: Seq<Word>)
    ensures skeleton(walk(Stmt::Exec(cmd, ws))) == skeleton(Stmt::Exec(cmd, ws)),
{
    reveal_with_fuel(walk, 1);
    reveal_with_fuel(skeleton, 1);
    if liftable(cmd, ws) {
        assert(walk(Stmt::Exec(cmd, ws)) == Stmt::Node(cmd, ws));
    } else {
        assert(walk(Stmt::Exec(cmd, ws)) == Stmt::Exec(cmd, ws));
    }
}

// THE CARRIER LAW: the walk changes only tags. The (cmd, argv) sequence of
// the whole tree — same entries, same order — is untouched.
proof fn the_walk_preserves_command_and_argv(st: Stmt)
    ensures skeleton(walk(st)) == skeleton(st),
    decreases st,
{
    reveal_with_fuel(walk, 1);
    reveal_with_fuel(skeleton, 1);
    match st {
        Stmt::Exec(cmd, ws) => {
            skeleton_walk_leaf(cmd, ws);
        }
        Stmt::Node(_, _) => {}
        Stmt::Other(_) => {}
        Stmt::Seq(a, b) => {
            the_walk_preserves_command_and_argv(*a);
            the_walk_preserves_command_and_argv(*b);
        }
        Stmt::If(c, t, e) => {
            the_walk_preserves_command_and_argv(*c);
            the_walk_preserves_command_and_argv(*t);
            the_walk_preserves_command_and_argv(*e);
        }
        Stmt::While(a) => the_walk_preserves_command_and_argv(*a),
        Stmt::Block(a) => the_walk_preserves_command_and_argv(*a),
        Stmt::Fn(a) => the_walk_preserves_command_and_argv(*a),
    }
}

// ── 4. completeness and legality of the walk ─────────────────────────────

proof fn every_liftable_exec_is_a_node(st: Stmt)
    ensures no_liftable_exec(walk(st)),
    decreases st,
{
    reveal_with_fuel(walk, 1);
    reveal_with_fuel(no_liftable_exec, 1);
    match st {
        Stmt::Exec(cmd, ws) => {
            if liftable(cmd, ws) {
                assert(walk(st) == Stmt::Node(cmd, ws));
            }
        }
        Stmt::Node(_, _) => {}
        Stmt::Other(_) => {}
        Stmt::Seq(a, b) => {
            every_liftable_exec_is_a_node(*a);
            every_liftable_exec_is_a_node(*b);
        }
        Stmt::If(c, t, e) => {
            every_liftable_exec_is_a_node(*c);
            every_liftable_exec_is_a_node(*t);
            every_liftable_exec_is_a_node(*e);
        }
        Stmt::While(a) => every_liftable_exec_is_a_node(*a),
        Stmt::Block(a) => every_liftable_exec_is_a_node(*a),
        Stmt::Fn(a) => every_liftable_exec_is_a_node(*a),
    }
}

proof fn only_legal_nodes_are_built(st: Stmt)
    requires no_nodes(st),
    ensures only_legal_nodes(walk(st)),
    decreases st,
{
    reveal_with_fuel(walk, 1);
    reveal_with_fuel(only_legal_nodes, 1);
    reveal_with_fuel(no_nodes, 1);
    match st {
        Stmt::Exec(cmd, ws) => {
            if !liftable(cmd, ws) {
                assert(walk(st) == Stmt::Exec(cmd, ws));
            }
        }
        Stmt::Node(_, _) => {
            // `no_nodes` makes this arm unreachable
            assert(false);
        }
        Stmt::Other(_) => {}
        Stmt::Seq(a, b) => {
            assert(no_nodes(*a) && no_nodes(*b));
            only_legal_nodes_are_built(*a);
            only_legal_nodes_are_built(*b);
        }
        Stmt::If(c, t, e) => {
            assert(no_nodes(*c) && no_nodes(*t) && no_nodes(*e));
            only_legal_nodes_are_built(*c);
            only_legal_nodes_are_built(*t);
            only_legal_nodes_are_built(*e);
        }
        Stmt::While(a) => {
            assert(no_nodes(*a));
            only_legal_nodes_are_built(*a);
        }
        Stmt::Block(a) => {
            assert(no_nodes(*a));
            only_legal_nodes_are_built(*a);
        }
        Stmt::Fn(a) => {
            assert(no_nodes(*a));
            only_legal_nodes_are_built(*a);
        }
    }
}

// ── 5. pins: the source's tests, as facts about the model ────────────────

pub open spec fn lit(s: Seq<int>) -> Word { Word::Lit(s) }

pub open spec fn f() -> Seq<int> { seq![102] }
pub open spec fn a() -> Seq<int> { seq![97] }
pub open spec fn b() -> Seq<int> { seq![98] }
pub open spec fn c() -> Seq<int> { seq![99] }
pub open spec fn d() -> Seq<int> { seq![100] }
pub open spec fn dir() -> Seq<int> { seq![100, 105, 114] }
pub open spec fn empty() -> Seq<int> { seq![101, 109, 112, 116, 121] }
pub open spec fn gone() -> Seq<int> { seq![103, 111, 110, 101, 46, 116, 120, 116] }
pub open spec fn ab() -> Seq<int> { seq![97, 47, 98] }
pub open spec fn flag_f() -> Seq<int> { seq![45, 102] }
pub open spec fn flag_rf() -> Seq<int> { seq![45, 114, 102] }
pub open spec fn flag_d() -> Seq<int> { seq![45, 100] }
pub open spec fn flag_p() -> Seq<int> { seq![45, 112] }
pub open spec fn flag_pv() -> Seq<int> { seq![45, 112, 118] }
pub open spec fn flag_i() -> Seq<int> { seq![45, 105] }
pub open spec fn long_parents() -> Seq<int> { seq![45, 45, 112, 97, 114, 101, 110, 116, 115] }
pub open spec fn glob_txt() -> Seq<int> { seq![102, 105, 108, 101, 95, 42, 46, 116, 120, 116] }
pub open spec fn lone_dash() -> Seq<int> { seq![45] }
pub open spec fn separator() -> Seq<int> { seq![45, 45] }
pub open spec fn w_sleep() -> Seq<int> { seq![115, 108, 101, 101, 112] }
pub open spec fn w_mktemp() -> Seq<int> { seq![109, 107, 116, 101, 109, 112] }
pub open spec fn w_cat() -> Seq<int> { seq![99, 97, 116] }

// The static-argv guarantee, instantiated on each hazard character: an
// accepted word carries no glob, expansion, quote, space or separator.
proof fn pin_plain_words_exclude_every_hazard()
    ensures ({
        let w = seq![102, 105, 108, 101, 46, 116, 120, 116]; // "file.txt"
        plain_word(w)
            && !contains_byte(w, star())
            && !contains_byte(w, question())
            && !contains_byte(w, dollar())
            && !contains_byte(w, space())
            && !contains_byte(w, backslash())
            && !contains_byte(w, semi())
    }),
{
    assert(plain_word(seq![102, 105, 108, 101, 46, 116, 120, 116])) by (compute_only);
    a_plain_word_has_no_hazard_char(seq![102, 105, 108, 101, 46, 116, 120, 116], star());
    a_plain_word_has_no_hazard_char(seq![102, 105, 108, 101, 46, 116, 120, 116], question());
    a_plain_word_has_no_hazard_char(seq![102, 105, 108, 101, 46, 116, 120, 116], dollar());
    a_plain_word_has_no_hazard_char(seq![102, 105, 108, 101, 46, 116, 120, 116], space());
    a_plain_word_has_no_hazard_char(seq![102, 105, 108, 101, 46, 116, 120, 116], backslash());
    a_plain_word_has_no_hazard_char(seq![102, 105, 108, 101, 46, 116, 120, 116], semi());
}

// The source's accepted shapes (lifts_static_rm_mkdir_touch).
proof fn pin_the_accepted_shapes_lift()
    ensures
        liftable(w_rm(), seq![lit(flag_f()), lit(a()), lit(b())]),
        liftable(w_rm(), seq![lit(flag_rf()), lit(dir())]),
        liftable(w_rm(), seq![lit(flag_d()), lit(empty())]),
        liftable(w_rm(), seq![lit(gone())]),
        liftable(w_mkdir(), seq![lit(flag_p()), lit(ab())]),
        liftable(w_mkdir(), seq![lit(flag_pv()), lit(d())]),
        liftable(w_mkdir(), seq![lit(d())]),
        liftable(w_touch(), seq![lit(f())]),
        liftable(w_touch(), seq![lit(a()), lit(b()), lit(c())]),
{
    assert(liftable(w_rm(), seq![lit(flag_f()), lit(a()), lit(b())])) by (compute_only);
    assert(liftable(w_rm(), seq![lit(flag_rf()), lit(dir())])) by (compute_only);
    assert(liftable(w_rm(), seq![lit(flag_d()), lit(empty())])) by (compute_only);
    assert(liftable(w_rm(), seq![lit(gone())])) by (compute_only);
    assert(liftable(w_mkdir(), seq![lit(flag_p()), lit(ab())])) by (compute_only);
    assert(liftable(w_mkdir(), seq![lit(flag_pv()), lit(d())])) by (compute_only);
    assert(liftable(w_mkdir(), seq![lit(d())])) by (compute_only);
    assert(liftable(w_touch(), seq![lit(f())])) by (compute_only);
    assert(liftable(w_touch(), seq![lit(a()), lit(b()), lit(c())])) by (compute_only);
}

// And the tag really changes: one accepted shape's walk is the node.
proof fn pin_an_accepted_shape_becomes_the_node()
    ensures walk(Stmt::Exec(w_rm(), seq![lit(flag_f()), lit(a()), lit(b())]))
        == Stmt::Node(w_rm(), seq![lit(flag_f()), lit(a()), lit(b())]),
{
    assert(liftable(w_rm(), seq![lit(flag_f()), lit(a()), lit(b())])) by (compute_only);
    reveal_with_fuel(walk, 1);
}

// The source's refused shapes (refuses_dynamic_and_unmodelled_shapes).
proof fn pin_the_refused_shapes_stay_exec()
    ensures
        !liftable(w_rm(), seq![Word::Dyn]),
        !liftable(w_rm(), seq![lit(glob_txt())]),
        !liftable(w_rm(), seq![lit(flag_i()), lit(f())]),
        !liftable(w_mkdir(), seq![lit(long_parents()), lit(d())]),
        !liftable(w_rm(), seq![]),
        !liftable(w_rm(), seq![lit(lone_dash())]),
        !liftable(w_sleep(), seq![lit(seq![49])]),
        !liftable(w_mktemp(), seq![lit(flag_d())]),
        !liftable(w_mktemp(), seq![lit(f())]),
        !liftable(w_cat(), seq![lit(f())]),
{
    assert(!liftable(w_rm(), seq![Word::Dyn])) by (compute_only);
    assert(!liftable(w_rm(), seq![lit(glob_txt())])) by (compute_only);
    assert(!liftable(w_rm(), seq![lit(flag_i()), lit(f())])) by (compute_only);
    assert(!liftable(w_mkdir(), seq![lit(long_parents()), lit(d())])) by (compute_only);
    assert(!liftable(w_rm(), seq![])) by (compute_only);
    assert(!liftable(w_rm(), seq![lit(lone_dash())])) by (compute_only);
    assert(!liftable(w_sleep(), seq![lit(seq![49])])) by (compute_only);
    assert(!liftable(w_mktemp(), seq![lit(flag_d())])) by (compute_only);
    assert(!liftable(w_mktemp(), seq![lit(f())])) by (compute_only);
    assert(!liftable(w_cat(), seq![lit(f())])) by (compute_only);
}

proof fn pin_a_refusal_is_the_identity()
    ensures walk(Stmt::Exec(w_rm(), seq![lit(flag_i()), lit(f())]))
        == Stmt::Exec(w_rm(), seq![lit(flag_i()), lit(f())]),
{
    assert(!liftable(w_rm(), seq![lit(flag_i()), lit(f())])) by (compute_only);
    reveal_with_fuel(walk, 1);
}

// The `--` separator is an accepted WORD and survives in the node's argv.
proof fn pin_the_separator_survives()
    ensures
        liftable(w_rm(), seq![lit(separator()), lit(flag_f()), lit(a())]),
        skeleton(walk(Stmt::Exec(w_rm(), seq![lit(separator()), lit(flag_f()), lit(a())])))[0].1[0]
            == Word::Lit(separator()),
{
    assert(liftable(w_rm(), seq![lit(separator()), lit(flag_f()), lit(a())])) by (compute_only);
    reveal_with_fuel(walk, 1);
    reveal_with_fuel(skeleton, 1);
}

// The offered gate: an unoffered backend gets the program unchanged, even for
// a liftable shape; an offered one lifts it.
proof fn pin_the_offered_gate()
    ensures
        transform(false, Stmt::Exec(w_rm(), seq![lit(flag_f()), lit(a())]))
            == Stmt::Exec(w_rm(), seq![lit(flag_f()), lit(a())]),
        transform(true, Stmt::Exec(w_rm(), seq![lit(flag_f()), lit(a())]))
            == Stmt::Node(w_rm(), seq![lit(flag_f()), lit(a())]),
{
    assert(liftable(w_rm(), seq![lit(flag_f()), lit(a())])) by (compute_only);
    reveal_with_fuel(transform, 1);
    reveal_with_fuel(walk, 1);
}

// The walk reaches every body: a tree with a liftable exec in each position
// leaves none behind.
pub open spec fn mixed_tree() -> Stmt {
    Stmt::Seq(
        Box::new(Stmt::Exec(w_rm(), seq![lit(flag_f()), lit(a())])),
        Box::new(Stmt::If(
            Box::new(Stmt::Exec(w_mkdir(), seq![lit(flag_p()), lit(d())])),
            Box::new(Stmt::Block(Box::new(Stmt::While(Box::new(
                Stmt::Exec(w_touch(), seq![lit(f())]),
            ))))),
            Box::new(Stmt::Fn(Box::new(Stmt::Exec(w_rm(), seq![lit(gone())])))),
        )),
    )
}

proof fn pin_the_walk_reaches_every_position()
    ensures no_liftable_exec(walk(mixed_tree())), only_legal_nodes(walk(mixed_tree())),
{
    assert(no_nodes(mixed_tree())) by (compute_only);
    every_liftable_exec_is_a_node(mixed_tree());
    only_legal_nodes_are_built(mixed_tree());
}

// A second run changes nothing (the node arm is the identity).
pub open spec fn small_tree() -> Stmt {
    Stmt::Seq(
        Box::new(Stmt::Exec(w_rm(), seq![lit(flag_f()), lit(a())])),
        Box::new(Stmt::If(
            Box::new(Stmt::Other(0)),
            Box::new(Stmt::Exec(w_touch(), seq![lit(f())])),
            Box::new(Stmt::Other(1)),
        )),
    )
}

proof fn pin_idempotent()
    ensures walk(walk(small_tree())) == walk(small_tree()),
{
    assert(liftable(w_rm(), seq![lit(flag_f()), lit(a())])) by (compute_only);
    assert(liftable(w_touch(), seq![lit(f())])) by (compute_only);
    reveal_with_fuel(walk, 3);
}

fn main() {}
}
