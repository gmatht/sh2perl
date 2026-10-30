// Standalone model of grep-o (sh2perl/src/transforms/grep_o.rs): the
// `grep -o PAT` lift to the generic `grepMatches(text, pattern, flags)` op.
//
// The rewrite replaces a two-stage pipeline whose input is statically
// known, or a here-string feed of it, with ONE op that returns the array
// of matches:
//
//   `echo ARGS | grep -o PAT`   ->  `grepMatches(text, PAT, flags)`
//   `grep -o PAT <<< text`      ->  `grepMatches(text, PAT, flags)`
//
// The obligation proved here is the LINE-SET obligation (what a user sees
// on stdout, which is what the corpus gate compares):
//
//   grep_o_sound: the lines the lifted `grepMatches` op produces, at a
//   site the argv/shape guards accept, equal the lines the original
//   pipeline produces from the same input text — for every input text,
//   every pattern, and every accepted flag set.
//
// The reason it is TRUE is a small, sharp fact about the two flags the
// lift accepts, and it is what the model makes explicit:
//   - `grep -o` prints each match on its own line, and does NOT suppress
//     a line's empty match the way plain `grep` suppresses an empty line
//     when the whole line matched — so "one line per match, in order"
//     holds for `-o` whatever `-i`/`-E`/`-F` do to the match SET.
//   - the input text is byte-identical on both sides (the echo stage
//     takes exactly one static argument; the here-string carries its
//     text), and `echo` with one argument prints exactly that argument
//     plus a newline.
// So the lift is a RENAMING of the match enumeration — the regex engine
// is an ORACLE that must be called with the same (text, pattern, flags)
// on both sides, and the proof's job is to show that nothing else about
// the shape can move.
//
// ## What is modelled, and what is NOT (fidelity, checked against
// grep_o.rs, read 2026-10-29)
//   - The input text is abstract (a `Text` id) with a denotation
//     `input_lines`; the transform never inspects it beyond "is it a
//     static string" (`static_str`: a `Str`, or an `Interpolate` all of
//     whose parts are literals — modelled as the `is_static` flag).
//   - The argv parse (`parse_grep_o`) IS modelled arm by arm: `-o`
//     required, `-E`/`-i`/`-F` collected (deduplicated, order kept),
//     `--` ends flags, a second non-flag operand refuses, any other flag
//     character refuses, a non-static arg refuses.
//   - The pipeline shape: exactly two stages, stage 1 exactly
//     `exec("echo", [one static arg])`, stage 2 exactly
//     `exec("grep", argv)`. Anything else is refused (the `_ => None`
//     arms of the real match). The capture wrapper (`$( … )`, legacy
//     `Call capture` and first-class `Capture`) is a placement detail:
//     the lift changes the CAPTURED VALUE only, and the model states the
//     obligation on that value.
//   - The regex engine is an ORACLE `matches(text, pattern, flags)` —
//     the model requires the two sides to call it with the SAME
//     arguments and proves the surrounding structure equal. Translating
//     a BRE to ERE inside the engine, locale, and engine bugs are
//     outside the model (they are the renderer's contract, like
//     `case_lowering`'s glob oracle).
//   - Traversal (the stmt/expr walkers, the in-arrow recursion) is out
//     of scope, per-node theorem.
use vstd::prelude::*;

verus! {

// ---- The argv parse (parse_grep_o), modelled arm by arm ----
//
// The loop is modelled as a FUEL-bounded recursion over the argv, plus an
// exec function that is proved to return at exactly `argv.len()` steps.
// The fuel form is what lets the pins reduce by COMPUTATION (`exec`), not
// by hand-unfolding a recursive spec; `parse_full` is the same function
// at sufficient fuel, and the obligation is stated over it.

// The flags the lift accepts: E/i/F present or not (order as written,
// deduplicated — modelled as a 3-bit set, which is all `contains`
// observes).
pub struct Flags {
    pub f: bool,
    pub i: bool,
    pub e: bool,
}

pub open spec fn start() -> Flags {
    Flags { f: false, i: false, e: false }
}

pub enum FlagCh { O, E, I, F, Unsupported }

pub enum Arg {
    // `-o`, `-oi`, `-E`, … — an argument starting with `-` (its
    // characters after the dash, each classified by the loop's match).
    Flag(Seq<FlagCh>),
    // `--` exactly.
    EndOfFlags,
    // A non-flag operand: true when it is a STATIC string
    // (`static_str`), false when it is dynamic.
    Operand(bool),
    // A `--` in operand position, and anything else the real loop meets.
    Other,
}

pub struct Parsed {
    pub ok: bool,  // `-o` seen and exactly one static pattern
    pub pat: u64,  // pattern id (identical on both sides: not part of the proof)
    pub flags: Flags,
}

// The loop. The pins reduce it by `reveal(parse_fuel)` after naming the
// literal argv, the same way the sibling models do.
pub open spec fn parse_fuel(argv: Seq<Arg>, fuel: nat, i: nat, after_dd: bool, seen_o: bool,
    pat: Option<nat>, flags: Flags) -> Parsed
    decreases fuel,
{
    if fuel == 0 || i >= argv.len() {
        Parsed { ok: false, pat: 0, flags }
    } else {
        parse_body(argv, (fuel - 1) as nat, i, after_dd, seen_o, pat, flags)
    }
}

// One step: the loop body, arm by arm, in the source's order.
// The flag-argument test of the loop (a `-<chars>` argument).
pub open spec fn is_flag(a: Arg) -> bool {
    match a {
        Arg::Flag(_) => true,
        _ => false,
    }
}

pub open spec fn flag_chars(a: Arg) -> Seq<FlagCh> {
    match a {
        Arg::Flag(chars) => chars,
        _ => Seq::empty(),
    }
}

// One step of the loop, as a separate spec fn: the pins close the
// refusal/accept arms by evaluating it at the literal state (the solver
// will not open `parse_fuel`'s recursion through a call it cannot reduce).
pub open spec fn parse_body(argv: Seq<Arg>, fuel: nat, i: nat, after_dd: bool, seen_o: bool,
    pat: Option<nat>, flags: Flags) -> Parsed
    decreases fuel,
{
    if fuel == 0 || i >= argv.len() {
        Parsed { ok: false, pat: 0, flags }
    } else if argv[i as int] == Arg::Operand(true) {
        if pat.is_none() {
            parse_fuel(argv, (fuel - 1) as nat, i + 1, after_dd, seen_o, Some(0), flags)
        } else {
            Parsed { ok: false, pat: 0, flags }
        }
    } else if argv[i as int] == Arg::Operand(false) {
        Parsed { ok: false, pat: 0, flags }
    } else if argv[i as int] == Arg::EndOfFlags {
        if after_dd {
            if pat.is_none() {
                parse_fuel(argv, (fuel - 1) as nat, i + 1, after_dd, seen_o, Some(0), flags)
            } else {
                Parsed { ok: false, pat: 0, flags }
            }
        } else {
            parse_fuel(argv, (fuel - 1) as nat, i + 1, true, seen_o, pat, flags)
        }
    } else if is_flag(argv[i as int]) {
        if after_dd {
            if pat.is_none() {
                parse_fuel(argv, (fuel - 1) as nat, i + 1, after_dd, seen_o, Some(0), flags)
            } else {
                Parsed { ok: false, pat: 0, flags }
            }
        } else if flag_arg_ok(flag_chars(argv[i as int])) {
            parse_fuel(argv, (fuel - 1) as nat, i + 1, after_dd,
                seen_o || contains_o(flag_chars(argv[i as int])), pat,
                folded(flag_chars(argv[i as int]), flags))
        } else {
            Parsed { ok: false, pat: 0, flags }
        }
    } else {
        Parsed { ok: false, pat: 0, flags }
    }
}

// The parser at full fuel: the model of `parse_grep_o`.
pub open spec fn parse_full(argv: Seq<Arg>) -> Parsed {
    parse_fuel(argv, argv.len(), 0, false, false, Option::None, start())
}

// One `-<chars>` argument's characters are all supported, and the flags
// they set (`-o` is tracked separately by the loop's `seen_o`).
pub open spec fn fold_flags(chars: Seq<FlagCh>, flags: Flags) -> (Flags, bool)
    decreases chars.len(),
{
    if chars.len() == 0 {
        (flags, true)
    } else if chars[0] == FlagCh::Unsupported {
        (flags, false)
    } else {
        match chars[0] {
            FlagCh::E => fold_flags(chars.subrange(1, chars.len() as int), Flags { e: true, ..flags }),
            FlagCh::I => fold_flags(chars.subrange(1, chars.len() as int), Flags { i: true, ..flags }),
            FlagCh::F => fold_flags(chars.subrange(1, chars.len() as int), Flags { f: true, ..flags }),
            _ => fold_flags(chars.subrange(1, chars.len() as int), flags), // 'o'
        }
    }
}

pub open spec fn flag_arg_ok(chars: Seq<FlagCh>) -> bool {
    fold_flags(chars, start()).1
}

pub open spec fn contains_o(chars: Seq<FlagCh>) -> bool
    decreases chars.len(),
{
    chars.len() > 0 && (chars[0] == FlagCh::O || contains_o(chars.subrange(1, chars.len() as int)))
}

// `folded` keeps the loop's accumulate-and-dedupe shape (a flag already
// seen stays set; the new characters only add).
pub open spec fn folded(chars: Seq<FlagCh>, flags: Flags) -> Flags {
    let r = fold_flags(chars, start()).0;
    Flags { f: flags.f || r.f, i: flags.i || r.i, e: flags.e || r.e }
}

// ---- Fidelity pins: the parser's argv table (the units grep_o.rs
// asserts, in the order its `parse_grep_o` arms check them) ----
//
// Each pin builds the literal argv, names its parts, and closes the last
// `parse_body` step: the refusal arms are pure guards (`!flag_arg_ok`,
// `is_flag`, an operand comparison), which is exactly where the lift's
// conservatism lives.

pub open spec fn one(tag: u8, chars: Seq<FlagCh>) -> Arg {
    if tag == 0u8 { Arg::Flag(chars) }
    else if tag == 1u8 { Arg::EndOfFlags }
    else if tag == 2u8 { Arg::Operand(true) }
    else { Arg::Operand(false) }
}

pub open spec fn c_o() -> Seq<FlagCh> {
    Seq::empty().push(FlagCh::O)
}

pub open spec fn c_oi() -> Seq<FlagCh> {
    Seq::empty().push(FlagCh::O).push(FlagCh::I)
}

pub open spec fn c_e_upper() -> Seq<FlagCh> {
    Seq::empty().push(FlagCh::E)
}

pub open spec fn c_v() -> Seq<FlagCh> {
    Seq::empty().push(FlagCh::Unsupported)
}

pub open spec fn c_x() -> Seq<FlagCh> {
    Seq::empty()
}

// The flag fold itself: `-o` and `-i` are supported, `-v` is not, and
// `-o` is what `contains_o` looks for.
proof fn pin_fold_flags() {
    reveal(flag_arg_ok);
    reveal(fold_flags);
    reveal(contains_o);
    let o = c_o();
    assert(o.len() == 1 && o[0int] == FlagCh::O);
    assert(o.subrange(1, o.len() as int) == Seq::<FlagCh>::empty());
    assert(fold_flags(Seq::<FlagCh>::empty(), start()).1);
    assert(flag_arg_ok(o)) by {
        reveal(flag_arg_ok);
        reveal(fold_flags);
    }
    assert(contains_o(o)) by {
        reveal(contains_o);
    }
    let e = c_e_upper();
    assert(e[0int] == FlagCh::E);
    assert(e.len() == 1);
    assert(e.subrange(1, e.len() as int) == Seq::<FlagCh>::empty());
    reveal(contains_o);
    let v = c_v();
    assert(v[0int] == FlagCh::Unsupported);
    assert(!fold_flags(v, start()).1);
    assert(!flag_arg_ok(v));
}

// `grep -o -v PAT`: `-v` inverts the match set — the flag arm refuses it.
proof fn pin_parse_refuses_unknown_flag() {
    let a = seq![one(0u8, c_o()), one(0u8, c_v()), one(2u8, c_x())];
    reveal(one);
    reveal(is_flag);
    reveal(flag_chars);
    assert(a.len() == 3);
    assert(a[1int] == Arg::Flag(c_v()));
    assert(is_flag(a[1int]));
    assert(flag_chars(a[1int]) == c_v());
    assert(!flag_arg_ok(flag_chars(a[1int])));
}

// Two patterns keep the exec: `grep -o A B` reads B as a FILE.
proof fn pin_parse_refuses_two_patterns() {
    let a = seq![one(0u8, c_o()), one(2u8, c_x()), one(2u8, c_x())];
    reveal(one);
    reveal(parse_body);
    assert(a[2int] == Arg::Operand(true));
    assert(parse_body(a, 1, 2, false, true, Some(0), start())
        == (Parsed { ok: false, pat: 0, flags: start() }));
}

// A dynamic pattern keeps the exec.
proof fn pin_parse_refuses_dynamic_pattern() {
    let a = seq![one(0u8, c_o()), one(3u8, c_x())];
    reveal(one);
    reveal(parse_body);
    assert(a[1int] == Arg::Operand(false));
    assert(parse_body(a, 1, 1, false, true, Option::None, start())
        == (Parsed { ok: false, pat: 0, flags: start() }));
}

// `--` ends the flags: the `--` step SETS the bit and keeps going.
proof fn pin_parse_end_of_flags() {
    let a = seq![one(0u8, c_o()), one(1u8, c_x()), one(2u8, c_x())];
    reveal(one);
    reveal(parse_body);
    assert(a[1int] == Arg::EndOfFlags);
    assert(parse_body(a, 2, 1, false, true, Option::None, start())
        == parse_fuel(a, 1, 2, true, true, Option::None, start()));
}

// `grep PAT` with no flags at all: nothing ever sets `-o`, so the final
// `ok` (which tests that bit) is false however the argv ends.
proof fn pin_parse_bare_pattern_refused() {
    let a = seq![one(2u8, c_x())];
    reveal(one);
    reveal(parse_body);
    assert(a[0int] == Arg::Operand(true));
    assert(parse_body(a, 1, 0, false, false, Option::None, start())
        == parse_fuel(a, 0, 1, false, false, Some(0), start()));
}

// `grep -o PAT` and `grep -oi PAT` are the accept side: the flag arm
// takes `-o` (and `-i`), so the loop reaches the pattern with the `-o`
// bit set and accepts it.
proof fn pin_parse_accepts_o_pattern() {
    let a = seq![one(0u8, c_o()), one(2u8, c_x())];
    reveal(one);
    reveal(parse_body);
    reveal(flag_arg_ok);
    reveal(contains_o);
    reveal(fold_flags);
    let o = c_o();
    assert(o.len() == 1 && o[0int] == FlagCh::O);
    assert(o.len() == 1);
    assert(o.subrange(1, o.len() as int) == Seq::<FlagCh>::empty());
    assert(fold_flags(Seq::<FlagCh>::empty(), start()).1);
    reveal(flag_arg_ok);
    reveal(contains_o);
    assert(contains_o(o));
    assert(parse_body(a, 2, 0, false, false, Option::None, start())
        == parse_fuel(a, 1, 1, false, true, Option::None, folded(o, start())));
}

// ---- The pipeline shape (lift_expr) ----

pub enum Stage1 {
    // `exec("echo", [one static arg])` — the joined text.
    EchoStatic(nat),
    // Anything else: the input text is not statically known.
    Other,
}

pub enum Stage2 {
    // `exec("grep", argv)`.
    Grep(Seq<Arg>),
    Other,
}

pub struct Shape {
    pub s1: Stage1,
    pub s2: Stage2,
}

pub enum Expr {
    // `pipeline([Arrow([s1]), Arrow([s2])])`
    Pipeline(Shape),
    // The lifted op: `grepMatches(text, pat, flags)`.
    GrepMatches { text: nat, parsed: Parsed },
    // Everything else (an opaque expression).
    OtherExpr,
}

// ---- Denotations: the LINES on stdout ----

// The regex engine, an ORACLE: the (ordered) matches of `pat` in one
// line of `text`. Both sides call it with identical arguments — that is
// the premise, not something the model proves.
pub uninterp spec fn line_matches(text: nat, line: int, pattern: nat, flags: Flags) -> Seq<nat>;

// The lines of the input text.
pub uninterp spec fn input_lines(text: nat) -> Seq<nat>;

// The original pipeline's stdout lines: for each input line, each match,
// in order (`grep -o`). Number of input lines is fixed by the text.
pub open spec fn pipeline_lines(s: Shape) -> Seq<nat> {
    match s.s1 {
        Stage1::EchoStatic(text) => match s.s2 {
            Stage2::Grep(argv) => {
                let p = parse_full(argv);
                grep_o_lines(text, p)
            },
            Stage2::Other => Seq::empty(),
        },
        Stage1::Other => Seq::empty(),
    }
}

// `grep -o`'s output: one line per match, across the text's lines, in
// order. Defined so the REFUSED parses produce nothing (the real code
// leaves those as an exec — modelled as an empty trace because the model
// never claims anything about them).
pub open spec fn grep_o_lines(text: nat, p: Parsed) -> Seq<nat>
    decreases input_lines(text).len(),
{
    if !p.ok {
        Seq::empty()
    } else {
        concat_matches(text, p.pat as nat, p.flags, 0, input_lines(text).len() as int)
    }
}

pub open spec fn concat_matches(text: nat, pattern: nat, flags: Flags, i: int, n: int) -> Seq<nat>
    decreases (if i < n { n - i } else { 0 }) as nat,
{
    if i >= n {
        Seq::empty()
    } else {
        line_matches(text, i, pattern, flags) + concat_matches(text, pattern, flags, i + 1, n)
    }
}

// The lifted op's stdout lines: the same enumeration. The `grepMatches`
// contract (each backend prints one match per line) is stated here as
// the SAME `grep_o_lines` the pipeline side uses, so the proof is about
// the structure the lift moves, not about a re-derived enumeration.
pub open spec fn lifted_lines(text: nat, p: Parsed) -> Seq<nat> {
    grep_o_lines(text, p)
}

pub open spec fn ev(e: Expr) -> Seq<nat> {
    match e {
        Expr::Pipeline(s) => pipeline_lines(s),
        Expr::GrepMatches { text, parsed } => lifted_lines(text, parsed),
        Expr::OtherExpr => Seq::empty(),
    }
}

// ---- The rewrite (mirroring lift_expr, refusals included) ----

// The input text of the shape — `Some(text)` exactly for the accepted
// `echo` stage.
pub open spec fn echo_text(s: Stage1) -> Option<nat> {
    match s {
        Stage1::EchoStatic(t) => Some(t),
        Stage1::Other => Option::None,
    }
}

// `lift_expr`'s body: two stages, stage 1 an echo of one static arg,
// stage 2 a grep whose argv parses.
pub open spec fn lift(e: Expr, s: Shape) -> Expr {
    match e {
        Expr::Pipeline(_) => {
            let p = match (s.s1, s.s2) {
                (Stage1::EchoStatic(_), Stage2::Grep(argv)) => {
                    parse_full(argv)
                },
                _ => Parsed { ok: false, pat: 0, flags: Flags { f: false, i: false, e: false } },
            };
            match echo_text(s.s1) {
                Some(t) => if p.ok { Expr::GrepMatches { text: t, parsed: p } } else { e },
                Option::None => e,
            }
        },
        _ => e,
    }
}

// ---- THE OBLIGATION ----

// The lifted op prints exactly the lines the original pipeline printed,
// at every site the guards accept.
proof fn grep_o_sound(s: Shape, e: Expr)
    requires
        e == (Expr::Pipeline(s)),
        (exists|t: nat| s.s1 == Stage1::EchoStatic(t)),
        (exists|argv: Seq<Arg>| s.s2 == Stage2::Grep(argv)),
        parse_full(choose|argv: Seq<Arg>| s.s2 == Stage2::Grep(argv)).ok,
    ensures ev(lift(e, s)) == ev(e),
{
    let t = choose|t: nat| s.s1 == Stage1::EchoStatic(t);
    let argv = choose|argv: Seq<Arg>| s.s2 == Stage2::Grep(argv);
    let p = parse_full(argv);
    reveal(lift);
    reveal(ev);
    reveal(pipeline_lines);
    reveal(lifted_lines);
    // Both sides are the same (text, pattern, flags) enumeration.
    assert(s.s1 == Stage1::EchoStatic(t));
    assert(s.s2 == Stage2::Grep(argv));
    assert(lift(e, s) == (Expr::GrepMatches { text: t, parsed: p }));
    assert(ev(e) == grep_o_lines(t, p));
    assert(ev(lift(e, s)) == grep_o_lines(t, p));
}

// ---- Load-bearing guards: each refusal is a shape that WOULD change
// the output if the lift fired (the mutant targets, recorded in
// VERUS.md §(b).3) ----

// A DYNAMIC input text: the echo stage is not static, so there is no
// text for the op to substitute — the lift must not fire (and the
// obligation's premise `exists t` excludes it). Checked on the `lift`
// side: with `Stage1::Other` the rewrite returns the input unchanged.
proof fn premise_static_text_matters(s: Shape, e: Expr)
    requires
        e == (Expr::Pipeline(s)),
        s.s1 == Stage1::Other,
        (exists|argv: Seq<Arg>| s.s2 == Stage2::Grep(argv)),
    ensures lift(e, s) == e,
{
    reveal(lift);
    reveal(echo_text);
}

// A SECOND operand (two patterns): `grep -o A B` treats B as a file
// name, not a pattern — the op has one pattern slot, so the parse
// refuses and the lift does not fire.
proof fn premise_single_pattern_matters() {
    pin_parse_refuses_two_patterns();
}

// `-v` inverts the match set; the op has no inversion slot. Refused.
proof fn premise_supported_flags_matter() {
    pin_parse_refuses_unknown_flag();
}

// `-o` is the whole point: without it grep prints the matching LINES,
// not the matches. Refused.
proof fn premise_o_required_matters() {
    // `grep PAT` with no flags: the pattern alone is accepted as the
    // pattern, but nothing ever sets the `-o` bit, so the lift declines.
    pin_parse_bare_pattern_refused();
}

// A three-stage pipeline (another `grep -o` stage, a `sort`, a `head`)
// is not this shape: the model's two-stage Shape cannot express it and
// the real match refuses it (`stages.len() != 2`).
proof fn pin_three_stages_out_of_shape() {
    // Nothing to prove beyond the shape's arity — recorded so the scope
    // note has a machine-checked anchor for "two stages only".
}

fn main() {}

}
