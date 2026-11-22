// text_filter_lift.rs — model of `text-filter-lift`
// (sh2perl/src/transforms/text_filter_lift.rs).
//
// The pass turns a pipeline of STATIC text filters into one `TextFilter`
// node: a literal producer (`echo`, `printf`, `seq`, `cat PATH`) followed by
// `wc`, `head`, `tail`, `uniq` or `cut`. The model proves:
//
//   1. THE STAGE SHAPE. Every stage must be EXACTLY one `builtin`/`exec` call
//      whose argv is all literal `Str` words (`stage_ok`); the producer and
//      filter classifiers require it, and the pins cover a dynamic word, a
//      two-statement stage, a non-filter second stage and a one-stage
//      pipeline.
//
//   2. THE PRODUCER RULES. `echo`'s leading `-n`/`-e`/`-E` scan (the newline
//      flag, and — under `-e` — that every escape is one of the four the pass
//      decodes), `printf`'s single `%`-free format, `seq`'s integer bounds
//      with `b >= a`, and `cat`'s single path (non-empty, not a dash, no
//      `;`). Each is a separate acceptance predicate with a refusal pin.
//
//   3. THE OPS ARE THE STAGES, IN ORDER. `finish` walks the stage list once:
//      the first stage must be a producer, every later stage a modelled
//      filter, and the emitted op list has exactly one entry per filter, in
//      the same order (`filters_of_len`), starting at stage 1
//      (`the_second_stage_is_a_filter`). The concrete pins show the exact op
//      for each accepted shape.
//
//   4. THE `;` ENCODING IS UNAMBIGUOUS. The node carries its ops as one
//      `;`-joined string, so a field containing `;` would merge two ops. A
//      `cat` path or a `cut` delimiter containing `;` is refused (pins); a
//      `cut` field list must be positive integers separated by single commas
//      (no empty field, no range, no zero).
//
// ## Out of model (stated, and measured by the anchor)
//
// - A1: the node renderer reproduces GNU `wc`/`head`/`tail`/`uniq`/`cut` for
//   the accepted flag subset, `cat`'s read-error text, and the PRODUCER BYTES
//   (`echo`'s one-space join, its newline, its four decodes; `printf`'s
//   format; `seq`'s decimal range). `harness/anchor-text-filter-lift.sh`
//   executes bash vs the generated Rust for every accepted shape.
// - The exact `format!` spelling of the op string: the model keeps the typed
//   op list, the anchor checks the emitted text.
// - `parse::<i64>` overflow: a digit string is its mathematical value.
// - The statement walk: `finish` is modelled on the stage list the walk hands
//   it for each pipeline occurrence (the walk recursion is `fs_lift`'s).
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 60 text_filter_lift.rs
//   -> verification results:: 30 verified, 0 errors
use vstd::prelude::*;

verus! {

// ── bytes ────────────────────────────────────────────────────────────────

pub open spec fn semi() -> int { 59 }          // ';'
pub open spec fn dash() -> int { 45 }          // '-'
pub open spec fn comma() -> int { 44 }         // ','
pub open spec fn percent() -> int { 37 }       // '%'
pub open spec fn zero() -> int { 48 }          // '0'
pub open spec fn backslash() -> int { 92 }     // '\'
pub open spec fn nl() -> int { 10 }            // newline

pub open spec fn is_digit(c: int) -> bool { 48 <= c && c <= 57 }

pub open spec fn any_byte(w: Seq<int>, c: int) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        false
    } else {
        w[0] == c || any_byte(w.skip(1), c)
    }
}

pub open spec fn all_digits(w: Seq<int>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        true
    } else {
        is_digit(w[0]) && all_digits(w.skip(1))
    }
}

pub open spec fn value(w: Seq<int>, acc: int) -> int
    decreases w.len(),
{
    if w.len() == 0 {
        acc
    } else {
        value(w.skip(1), acc * 10 + (w[0] - zero()))
    }
}

pub open spec fn positive_int(w: Seq<int>) -> bool {
    w.len() > 0 && all_digits(w) && value(w, 0) > 0
}

// ── stages ───────────────────────────────────────────────────────────────

pub struct Stage {
    pub one_call: bool,
    pub all_literal: bool,
    pub cmd: Seq<int>,
    pub args: Seq<Seq<int>>,
}

pub open spec fn stage_ok(st: Stage) -> bool {
    st.one_call && st.all_literal
}

// ── producers ────────────────────────────────────────────────────────────

// `Echo(words, newline)`: the words after the flag run, and whether the
// terminator is printed. The BYTES are A1; this records the decisions.
pub enum Prod {
    Echo(Seq<Seq<int>>, bool),
    Printf(Seq<int>),
    Range(int, int),
    /// `cat PATH`: the node's `cat:PATH` op.
    Read(Seq<int>),
    No,
}

pub open spec fn s_echo() -> Seq<int> { seq![101, 99, 104, 111] }
pub open spec fn s_printf() -> Seq<int> { seq![112, 114, 105, 110, 116, 102] }
pub open spec fn s_seq() -> Seq<int> { seq![115, 101, 113] }
pub open spec fn s_cat() -> Seq<int> { seq![99, 97, 116] }

pub open spec fn is_echo_flag(w: Seq<int>) -> bool {
    w == seq![dash(), 101] || w == seq![dash(), 110] || w == seq![dash(), 69]
}

pub open spec fn echo_prefix(args: Seq<Seq<int>>) -> int
    decreases args.len(),
{
    if args.len() == 0 {
        0
    } else if is_echo_flag(args[0]) {
        1 + echo_prefix(args.skip(1))
    } else {
        0
    }
}

// Does the leading flag run contain `-<flag>`? Stops where the scan stops.
pub open spec fn prefix_has(args: Seq<Seq<int>>, flag: int) -> bool
    decreases args.len(),
{
    if args.len() == 0 {
        false
    } else if !is_echo_flag(args[0]) {
        false
    } else if args[0] == seq![dash(), flag] {
        true
    } else {
        prefix_has(args.skip(1), flag)
    }
}

// Under `-e`, every backslash must begin one of `\n \t \r \\`; any other
// escape refuses the producer (its byte value would be wrong).
pub open spec fn escapes_ok(w: Seq<int>) -> bool
    decreases w.len(),
{
    if w.len() == 0 {
        true
    } else if w[0] != backslash() {
        escapes_ok(w.skip(1))
    } else if w.len() < 2 {
        false
    } else if w[1] == 110 || w[1] == 116 || w[1] == 114 || w[1] == backslash() {
        escapes_ok(w.skip(2))
    } else {
        false
    }
}

pub open spec fn words_escapes_ok(ws: Seq<Seq<int>>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        true
    } else {
        escapes_ok(ws[0]) && words_escapes_ok(ws.skip(1))
    }
}

pub open spec fn echo_ok(args: Seq<Seq<int>>) -> bool {
    let p = echo_prefix(args);
    (!prefix_has(args, 101) || words_escapes_ok(args.skip(p)))
}

pub open spec fn producer(st: Stage) -> Prod {
    if !stage_ok(st) {
        Prod::No
    } else if st.cmd == s_echo() {
        if echo_ok(st.args) {
            Prod::Echo(st.args.skip(echo_prefix(st.args)), !prefix_has(st.args, 110))
        } else {
            Prod::No
        }
    } else if st.cmd == s_printf() {
        if st.args.len() == 1 && !any_byte(st.args[0], percent()) && escapes_ok(st.args[0]) {
            Prod::Printf(st.args[0])
        } else {
            Prod::No
        }
    } else if st.cmd == s_seq() {
        if st.args.len() == 1 && positive_int(st.args[0]) {
            Prod::Range(1, value(st.args[0], 0))
        } else if st.args.len() == 2
            && positive_int(st.args[0])
            && positive_int(st.args[1])
            && value(st.args[1], 0) >= value(st.args[0], 0)
        {
            Prod::Range(value(st.args[0], 0), value(st.args[1], 0))
        } else {
            Prod::No
        }
    } else if st.cmd == s_cat() {
        if st.args.len() == 1
            && st.args[0].len() > 0
            && !any_byte(st.args[0], semi())
            && st.args[0][0] != dash()
        {
            Prod::Read(st.args[0])
        } else {
            Prod::No
        }
    } else {
        Prod::No
    }
}

// ── filters ──────────────────────────────────────────────────────────────

pub enum Wc {
    Lines,
    Words,
    Bytes,
}

pub enum Op {
    WcOp(Wc),
    Head(int),
    Tail(int),
    Uniq,
    Cat(Seq<int>),
    Cut(Seq<int>, Seq<int>),
}

pub open spec fn s_wc() -> Seq<int> { seq![119, 99] }
pub open spec fn s_head() -> Seq<int> { seq![104, 101, 97, 100] }
pub open spec fn s_tail() -> Seq<int> { seq![116, 97, 105, 108] }
pub open spec fn s_uniq() -> Seq<int> { seq![117, 110, 105, 113] }
pub open spec fn s_cut() -> Seq<int> { seq![99, 117, 116] }

pub open spec fn one_arg(st: Stage, w: Seq<int>) -> bool {
    st.args.len() == 1 && st.args[0] == w
}

// A comma-separated field list of POSITIVE integers: no empty field
// (leading/trailing/double comma), no range, no all-zero field.
pub open spec fn fields_scan(w: Seq<int>, i: int, seen_nonzero: bool) -> bool
    decreases w.len() - i,
{
    if i >= w.len() {
        seen_nonzero
    } else if w[i] == comma() {
        seen_nonzero && i + 1 < w.len() && fields_scan(w, i + 1, false)
    } else if is_digit(w[i]) {
        fields_scan(w, i + 1, seen_nonzero || w[i] != zero())
    } else {
        false
    }
}

pub open spec fn fields_positive(w: Seq<int>) -> bool {
    fields_scan(w, 0, false)
}

// `cut -d D -f LIST` in either flag order; one delimiter character, no `;`.
pub open spec fn cut_scan(
    args: Seq<Seq<int>>,
    i: int,
    seen_d: bool,
    d: Seq<int>,
    seen_f: bool,
    f: Seq<int>,
) -> Option<Op>
    decreases args.len() - i,
{
    if i >= args.len() {
        if seen_d && seen_f && d.len() == 1 && !any_byte(d, semi()) && fields_positive(f) {
            Option::Some(Op::Cut(d, f))
        } else {
            Option::None
        }
    } else if args[i] == seq![dash(), 100] && i + 1 < args.len() {
        cut_scan(args, i + 2, true, args[i + 1], seen_f, f)
    } else if args[i] == seq![dash(), 102] && i + 1 < args.len() {
        cut_scan(args, i + 2, seen_d, d, true, args[i + 1])
    } else {
        Option::None
    }
}

pub open spec fn count_arg(args: Seq<Seq<int>>) -> Option<int> {
    if args.len() == 1 && args[0].len() > 1 && args[0][0] == dash()
        && positive_int(args[0].skip(1))
    {
        Option::Some(value(args[0].skip(1), 0))
    } else if args.len() == 2 && args[0] == seq![dash(), 110] && positive_int(args[1]) {
        Option::Some(value(args[1], 0))
    } else {
        Option::None
    }
}

pub open spec fn filter(st: Stage) -> Option<Op> {
    if !stage_ok(st) {
        Option::None
    } else if st.cmd == s_wc() {
        if one_arg(st, seq![dash(), 108]) {
            Option::Some(Op::WcOp(Wc::Lines))
        } else if one_arg(st, seq![dash(), 119]) {
            Option::Some(Op::WcOp(Wc::Words))
        } else if one_arg(st, seq![dash(), 99]) {
            Option::Some(Op::WcOp(Wc::Bytes))
        } else {
            Option::None
        }
    } else if st.cmd == s_head() {
        match count_arg(st.args) {
            Option::Some(n) => Option::Some(Op::Head(n)),
            Option::None => Option::None,
        }
    } else if st.cmd == s_tail() {
        match count_arg(st.args) {
            Option::Some(n) => Option::Some(Op::Tail(n)),
            Option::None => Option::None,
        }
    } else if st.cmd == s_uniq() {
        if st.args.len() == 0 { Option::Some(Op::Uniq) } else { Option::None }
    } else if st.cmd == s_cut() {
        cut_scan(st.args, 0, false, seq![], false, seq![])
    } else {
        // `tee` and every interpreter keep the spawn.
        Option::None
    }
}

pub open spec fn filters_of(stages: Seq<Stage>) -> Option<Seq<Op>>
    decreases stages.len(),
{
    if stages.len() == 0 {
        Option::Some(seq![])
    } else {
        match (filter(stages[0]), filters_of(stages.skip(1))) {
            (Option::Some(o), Option::Some(rest)) => Option::Some(seq![o] + rest),
            _ => Option::None,
        }
    }
}

// ── the emitted pipeline ─────────────────────────────────────────────────

pub struct Pipeline {
    pub prod: Prod,
    pub ops: Seq<Op>,
}

pub open spec fn finish(stages: Seq<Stage>) -> Option<Pipeline> {
    if stages.len() < 2 {
        Option::None
    } else {
        match producer(stages[0]) {
            Prod::No => Option::None,
            Prod::Read(p) => match filters_of(stages.skip(1)) {
                Option::Some(os) => Option::Some(Pipeline {
                    prod: Prod::Read(p),
                    ops: seq![Op::Cat(p)] + os,
                }),
                Option::None => Option::None,
            },
            p => match filters_of(stages.skip(1)) {
                Option::Some(os) => Option::Some(Pipeline { prod: p, ops: os }),
                Option::None => Option::None,
            },
        }
    }
}

// ── 1. the shape ─────────────────────────────────────────────────────────

proof fn accepted_stages_are_static(stages: Seq<Stage>)
    requires finish(stages) is Some, stages.len() >= 2
    ensures stage_ok(stages[0]), producer(stages[0]) != Prod::No
{
    assert(!(stages.len() < 2));
    if producer(stages[0]) == Prod::No {
        assert(finish(stages) is None);
    }
}

proof fn the_second_stage_is_a_filter(stages: Seq<Stage>)
    requires finish(stages) is Some, stages.len() >= 2
    ensures filter(stages[1]) is Some
{
    assert(!(stages.len() < 2));
    if filter(stages[1]) is None {
        assert(filters_of(stages.skip(1)) is None);
        assert(finish(stages) is None);
    }
}

proof fn a_one_stage_pipeline_is_refused(st: Stage)
    ensures finish(seq![st]) is None
{
    assert(seq![st].len() == 1);
}

// ── 2. the ops track the stages ──────────────────────────────────────────

proof fn filters_of_len(stages: Seq<Stage>)
    requires filters_of(stages) is Some,
    ensures filters_of(stages).unwrap().len() == stages.len(),
    decreases stages.len(),
{
    if stages.len() > 0 {
        filters_of_len(stages.skip(1));
    }
}

proof fn the_ops_track_the_stages(stages: Seq<Stage>)
    requires filters_of(stages) is Some
    ensures filters_of(stages).unwrap().len() == stages.len()
{
    filters_of_len(stages);
}

// ── 3. pins: the accepted shapes (the source's first test) ───────────────

pub open spec fn stage(cmd: Seq<int>, args: Seq<Seq<int>>) -> Stage {
    Stage { one_call: true, all_literal: true, cmd: cmd, args: args }
}

pub open spec fn w_hi() -> Seq<int> { seq![104, 105] }
pub open spec fn w_a_colon_b() -> Seq<int> { seq![97, 58, 98] }
pub open spec fn w_colon() -> Seq<int> { seq![58] }
pub open spec fn w_2() -> Seq<int> { seq![50] }
pub open spec fn w_f() -> Seq<int> { seq![102] }
pub open spec fn w_1() -> Seq<int> { seq![49] }
pub open spec fn w_10() -> Seq<int> { seq![49, 48] }
pub open spec fn flag_l() -> Seq<int> { seq![dash(), 108] }

pub open spec fn pipe_echo_wc() -> Seq<Stage> {
    seq![stage(s_echo(), seq![w_hi()]), stage(s_wc(), seq![flag_l()])]
}

pub open spec fn pipe_seq_head() -> Seq<Stage> {
    seq![stage(s_seq(), seq![w_1(), w_10()]), stage(s_head(), seq![seq![dash(), 51]])]
}

pub open spec fn pipe_echo_cut() -> Seq<Stage> {
    seq![
        stage(s_echo(), seq![w_a_colon_b()]),
        stage(s_cut(), seq![seq![dash(), 100], w_colon(), seq![dash(), 102], w_2()]),
    ]
}

pub open spec fn pipe_cat_uniq() -> Seq<Stage> {
    seq![stage(s_cat(), seq![w_f()]), stage(s_uniq(), seq![])]
}

proof fn pin_static_literal_pipelines_lift()
{
    assert(finish(pipe_echo_wc()) == Option::Some(Pipeline {
        prod: Prod::Echo(seq![w_hi()], true),
        ops: seq![Op::WcOp(Wc::Lines)],
    })) by (compute_only);
    assert(finish(pipe_seq_head()) == Option::Some(Pipeline {
        prod: Prod::Range(1, 10),
        ops: seq![Op::Head(3)],
    })) by (compute_only);
    assert(finish(pipe_echo_cut()) == Option::Some(Pipeline {
        prod: Prod::Echo(seq![w_a_colon_b()], true),
        ops: seq![Op::Cut(w_colon(), w_2())],
    })) by (compute_only);
    assert(finish(pipe_cat_uniq()) == Option::Some(Pipeline {
        prod: Prod::Read(w_f()),
        ops: seq![Op::Cat(w_f()), Op::Uniq],
    })) by (compute_only);
}

// `echo -n` keeps its words and records the suppressed newline.
proof fn pin_echo_n_flag()
{
    assert(producer(stage(s_echo(), seq![seq![dash(), 110], w_hi()]))
        == Prod::Echo(seq![w_hi()], false)) by (compute_only);
    assert(producer(stage(s_echo(), seq![seq![dash(), 101], w_hi()]))
        == Prod::Echo(seq![w_hi()], true)) by (compute_only);
    // A flag-looking word AFTER a word is a WORD: the scan has stopped.
    assert(producer(stage(s_echo(), seq![w_hi(), seq![dash(), 110]]))
        == Prod::Echo(seq![w_hi(), seq![dash(), 110]], true)) by (compute_only);
}

// ── 4. pins: the refusals (the source's second test) ─────────────────────

pub open spec fn w_awk() -> Seq<int> { seq![97, 119, 107] }
pub open spec fn w_tee() -> Seq<int> { seq![116, 101, 101] }
pub open spec fn w_plus2() -> Seq<int> { seq![43, 50] }
pub open spec fn w_1_2() -> Seq<int> { seq![49, 45, 50] }
pub open spec fn flag_f() -> Seq<int> { seq![dash(), 102] }
pub open spec fn flag_c() -> Seq<int> { seq![dash(), 99] }
pub open spec fn flag_upper_l() -> Seq<int> { seq![dash(), 76] }
pub open spec fn flag_n() -> Seq<int> { seq![dash(), 110] }
pub open spec fn flag_e() -> Seq<int> { seq![dash(), 101] }
pub open spec fn w_3() -> Seq<int> { seq![51] }

proof fn pin_refusals_keep_the_spawn()
{
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(w_awk(), seq![w_hi()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_seq(), seq![w_1(), w_3()]), stage(s_head(), seq![seq![dash(), 49], w_f(), w_f()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_seq(), seq![w_1(), w_3()]), stage(s_tail(), seq![flag_f()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_seq(), seq![w_1(), w_3()]), stage(s_tail(), seq![flag_n(), w_plus2()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_seq(), seq![w_1(), w_3()]), stage(s_head(), seq![flag_c()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(s_cut(), seq![flag_c(), w_1()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(s_cut(), seq![seq![dash(), 100], w_colon(), flag_f(), w_1_2()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(s_wc(), seq![flag_upper_l()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(s_wc(), seq![flag_l(), w_f()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(s_uniq(), seq![flag_c()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()]), stage(w_tee(), seq![w_f()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_cat(), seq![]), stage(s_wc(), seq![flag_l()])]) is None) by (compute_only);
    assert(finish(seq![stage(s_echo(), seq![w_hi()])]) is None) by (compute_only);
}

proof fn a_dynamic_word_is_refused()
{
    assert(finish(seq![
        Stage { one_call: true, all_literal: false, cmd: s_echo(), args: seq![w_hi()] },
        stage(s_wc(), seq![flag_l()]),
    ]) is None) by (compute_only);
}

proof fn a_two_statement_stage_is_refused()
{
    assert(finish(seq![
        Stage { one_call: false, all_literal: true, cmd: s_echo(), args: seq![w_hi()] },
        stage(s_wc(), seq![flag_l()]),
    ]) is None) by (compute_only);
}

// ── 5. pins: the encoding guards ─────────────────────────────────────────

proof fn a_semicolon_path_is_refused()
{
    assert(producer(stage(s_cat(), seq![seq![97, semi(), 98]])) == Prod::No) by (compute_only);
    assert(producer(stage(s_cat(), seq![seq![]])) == Prod::No) by (compute_only);
    assert(producer(stage(s_cat(), seq![flag_f()])) == Prod::No) by (compute_only);
}

proof fn a_semicolon_delimiter_is_refused()
{
    assert(filter(stage(s_cut(), seq![
        seq![dash(), 100], seq![semi()], seq![dash(), 102], w_1(),
    ])) is None) by (compute_only);
}

proof fn a_two_character_delimiter_is_refused()
{
    assert(filter(stage(s_cut(), seq![
        seq![dash(), 100], seq![58, 58], seq![dash(), 102], w_1(),
    ])) is None) by (compute_only);
}

proof fn a_trailing_comma_field_is_refused()
{
    assert(filter(stage(s_cut(), seq![
        seq![dash(), 100], w_colon(), seq![dash(), 102], seq![49, comma()],
    ])) is None) by (compute_only);
}

proof fn a_leading_or_double_comma_is_refused()
{
    assert(filter(stage(s_cut(), seq![
        seq![dash(), 100], w_colon(), seq![dash(), 102], seq![comma(), 50],
    ])) is None) by (compute_only);
    assert(filter(stage(s_cut(), seq![
        seq![dash(), 100], w_colon(), seq![dash(), 102], seq![49, comma(), comma(), 50],
    ])) is None) by (compute_only);
}

proof fn a_zero_field_is_refused()
{
    assert(filter(stage(s_cut(), seq![
        seq![dash(), 100], w_colon(), seq![dash(), 102], seq![zero()],
    ])) is None) by (compute_only);
}

// An unknown escape under `-e` refuses the producer; the four decoded ones
// are accepted. A lone trailing backslash is also refused.
proof fn an_unknown_escape_is_refused()
{
    assert(producer(stage(s_echo(), seq![flag_e(), seq![92, 113]])) == Prod::No) by (compute_only);
    assert(producer(stage(s_echo(), seq![flag_e(), seq![92]])) == Prod::No) by (compute_only);
    assert(producer(stage(s_echo(), seq![flag_e(), seq![92, 110]]))
        == Prod::Echo(seq![seq![92, 110]], true)) by (compute_only);
}

proof fn a_percent_format_is_refused()
{
    assert(producer(stage(s_printf(), seq![seq![37, 115]])) == Prod::No) by (compute_only);
    assert(producer(stage(s_printf(), seq![w_hi(), w_f()])) == Prod::No) by (compute_only);
    assert(producer(stage(s_printf(), seq![seq![92, 113]])) == Prod::No) by (compute_only);
    assert(producer(stage(s_printf(), seq![w_hi()])) == Prod::Printf(w_hi())) by (compute_only);
}

pub open spec fn w_5() -> Seq<int> { seq![53] }

proof fn a_reversed_range_is_refused()
{
    assert(producer(stage(s_seq(), seq![w_5(), w_1()])) == Prod::No) by (compute_only);
    assert(producer(stage(s_seq(), seq![seq![zero()]])) == Prod::No) by (compute_only);
    assert(filter(stage(s_head(), seq![seq![zero()]])) is None) by (compute_only);
    // `head 3` is the OPERAND form (a file named 3), not a count — and so is
    // `head 13` (bash strips no dash, the lift must not either).
    assert(filter(stage(s_head(), seq![w_3()])) is None) by (compute_only);
    assert(filter(stage(s_head(), seq![seq![49, 51]])) is None) by (compute_only);
}

fn main() {}
}
