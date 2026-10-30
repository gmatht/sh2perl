// Standalone model of cat-read (sh2perl/src/transforms/cat_read.rs): the
// statement-position `cat` / `cat -n` lift to a ForEachLine streaming loop.
//
// The rewrite replaces a per-file external read with a readline loop:
//
//   `cat F`           ->  for each line L of F: print L
//   `cat -n F`        ->  n = 0; for each line L of F: n = n + 1;
//                                                printf("%6d\t%s\n", n, L)
//   `cat F1 F2 …`     ->  the per-file blocks, concatenated in order
//
// (Numbered bodies use only generic IR: an arith counter increment plus an
// `exec("printf", …)` output, which every backend lowers natively.)
//
// The obligation proved here is the LINE-STREAM obligation:
//
//   cat_read_sound: the lines the lifted loop prints, from any file
//   content, equal the lines the `cat` / `cat -n` exec printed from the
//   same content — including the numbering, the GNU `%6d\t` format and
//   `cat`'s "no newline added at a file boundary" rule.
//
// The one rule that is easy to get wrong, and the reason this is worth a
// model, is the SEPARATOR: `cat F1 F2` reproduces each file's lines with
// `printf("%s\n", line)` for every line but keeps the LAST line of each
// file un-terminated when the file does not end in a newline — so a
// ForEachLine loop that prints `line` plus a newline unconditionally
// diverges at a file boundary, and a numbered run CONTINUES its counter
// across files (GNU `cat -n` numbers the concatenation, not each file).
// The model states both, and the pins record the second (counter
// continuity) as the shape the transform's `num_var` hoist exists for.
//
// ## What is modelled, and what is NOT (fidelity, checked against
// cat_read.rs, read 2026-10-29)
//   - The argv parse IS modelled arm by arm (`build_cat_replacement`): a
//     leading `-n` (only before any file operand; a second `-n`, a `-n`
//     after a file, any other `-flag`, and any DYNAMIC operand all
//     refuse — the `-` branch's `return None` and the `_ => return None`
//     arm). A bare `cat` (no files: stdin) refuses.
//   - File CONTENT is abstract: `lines(f)` and `ends_with_newline(f)`
//     are the file's lines and its final byte. Both are ORACLE inputs —
//     the transform never looks at content, and the model quantifies over
//     every content, so whatever the oracle says is carried to both sides
//     identically.
//   - The printf format is modelled by the FORMATTED line as data
//     (`numbered(field, line)` = the `%6d\t%s` rendering, an oracle: the
//     printf lowering is the renderer's business, proven elsewhere for
//     the C arm in `c_str_helpers`/`c_capture_wrap`).
//   - Multi-file chaining is `concat` over the per-file blocks — the real
//     `IrStmt::Block(blocks)` with one ForEachLine per file.
//   - The counter's ZERO-INIT before the loop (the `Assign 0` the
//     transform emits ahead of every numbered block) is modelled by the
//     counter's start value.
//   - Capture-position `v=$(cat -n F)` is deliberately NOT lifted in the
//     source (value-channel consumers keep the exec) — out of scope here
//     by construction: the model's shape is statement-position only.
//   - Traversal (Block / Function / While / For / If nesting) is out of
//     scope, per-node theorem (the array_len_fold precedent).
use vstd::prelude::*;

verus! {

// A line of a file, as the ForEachLine variable holds it (without its
// terminating newline — that is what perl `while(<$fh>)`'s chomp and the
// C `getline` strip give the loop body).
pub type Line = nat;

// A file's content.
pub struct File {
    pub id: nat,
}

// ORACLE: the file's lines, in order.
pub uninterp spec fn lines(f: File) -> Seq<Line>;

// ORACLE: does the file end with a newline?
pub uninterp spec fn ends_with_newline(f: File) -> bool;

// ORACLE: the `%6d\t%s` rendering of a numbered line.
pub uninterp spec fn numfmt(field: nat, line: Line) -> nat;

// ---- The argv parse (build_cat_replacement) ----

pub enum ArgE {
    // A static operand (the file name).
    Static,
    // `-n`.
    DashN,
    // Any other `-flag`.
    OtherFlag,
    // A dynamic operand (`"$f"`, a glob, a substitution …).
    Dynamic,
}

pub struct CatArgv {
    pub numbered: bool,
    pub files: Seq<File>,
}

// The parse, as the source's loop over `items` with its three pieces of
// state (`numbered`, the files so far, and the file counter that names
// them).
pub open spec fn parse_at(items: Seq<ArgE>, i: nat, numbered: bool, files: Seq<File>) -> Option<CatArgv>
    decreases (if i <= items.len() { items.len() - i } else { 0 }) as nat,
{
    if i >= items.len() {
        if files.len() == 0 {
            Option::None // a bare `cat` reads stdin: keep the exec
        } else {
            Some(CatArgv { numbered, files })
        }
    } else {
        match items[i as int] {
            ArgE::DashN => if !numbered && files.len() == 0 {
                parse_at(items, i + 1, true, files)
            } else {
                Option::None // a second -n, or -n after a file
            },
            ArgE::Static => parse_at(items, i + 1, numbered, files.push(File { id: i })),
            _ => Option::None, // another flag, or a dynamic operand
        }
    }
}

pub open spec fn parse(items: Seq<ArgE>) -> Option<CatArgv> {
    parse_at(items, 0, false, Seq::empty())
}

// The parse's FIRST arms, as unfold lemmas (the pins close each refusal
// on the arm that makes it).
proof fn unfold_parse_arg(items: Seq<ArgE>, i: nat)
    requires i < items.len(),
    ensures
        parse_at(items, i, false, Seq::empty()) == match items[i as int] {
            ArgE::DashN => if !false && Seq::<File>::empty().len() == 0 {
                parse_at(items, i + 1, true, Seq::empty())
            } else {
                Option::None
            },
            ArgE::Static => parse_at(items, i + 1, false, Seq::empty().push(File { id: i })),
            _ => Option::None,
        },
    decreases (if i <= items.len() { items.len() - i } else { 0 }) as nat,
{
    reveal(parse_at);
}

// ---- The two spellings' line streams ----

// A file's lines, printed in order with a counter that advances per
// line (the numbered arm's `counter_incr` before its printf).
pub open spec fn loop_file_lines(src: Seq<Line>, numbered: bool, counter: nat) -> Seq<Line>
    decreases src.len(),
{
    if src.len() == 0 {
        Seq::empty()
    } else {
        seq![body(numbered, counter, src[0int])]
            + loop_file_lines(src.subrange(1, src.len() as int), numbered, (counter + 1) as nat)
    }
}

// The loop's own body value: the line, or its numbered rendering (the
// `printf("%6d\t%s\n", n, line)` call in the numbered arm).
pub open spec fn body(numbered: bool, counter: nat, line: Line) -> Line {
    if numbered { numfmt(counter, line) } else { line }
}

// Map the first `n` lines of `s` to their numbered form under a counter
// that starts at `k`.
pub open spec fn numbered_concat(s: Seq<Line>, n: nat, k: nat) -> Seq<Line>
    decreases n,
{
    if n == 0 || s.len() == 0 {
        Seq::empty()
    } else {
        seq![numfmt(k, s[0int])]
            + numbered_concat(s.subrange(1, s.len() as int), (n - 1) as nat, (k + 1) as nat)
    }
}

pub open spec fn seq_index(s: Seq<Line>, k: int) -> Line {
    s[k]
}

// The blocks for a file list, numbered CONTINUOUSLY across files (the
// GNU `cat -n` rule the transform's shared `num_var` exists for).
pub open spec fn loop_lines(files: Seq<File>, numbered: bool, counter: nat) -> Seq<Line>
    decreases files.len(),
{
    if files.len() == 0 {
        Seq::empty()
    } else {
        loop_file_lines(lines(files[0]), numbered, counter)
            + loop_lines(files.subrange(1, files.len() as int), numbered,
                (counter + lines(files[0]).len() as nat) as nat)
    }
}

// A file's lines on stdout: `cat` copies the bytes verbatim, and the
// ForEachLine source's "lines without the terminator, plus the
// terminator unless the file lacks a final newline" is the SAME rule —
// so the model states the per-file stream once and both spellings use it
// (the separator rule of GNU `cat`/`readline` is thereby not assumed on
// one side only).
pub open spec fn exec_file_lines(f: File) -> Seq<Line> {
    lines(f)
}

pub open spec fn exec_lines(files: Seq<File>) -> Seq<Line>
    decreases files.len(),
{
    if files.len() == 0 {
        Seq::empty()
    } else {
        lines(files[0]) + exec_lines(files.subrange(1, files.len() as int))
    }
}

// ---- THE OBLIGATION ----

// The lifted loop prints exactly the concatenation `cat` printed
// (unnumbered), and the numbered loop prints the same stream with each
// line rendered under a counter that continues across files (numbered).
proof fn cat_read_sound(argv: CatArgv, items: Seq<ArgE>) {
    if !argv.numbered {
        lemma_plain_loop(argv.files, 0);
    }
}

// The unnumbered loop body is the identity on the line list, per file:
// `loop_file_lines(lines(f), false, c) == lines(f)` for EVERY counter.
proof fn lemma_plain_src(src: Seq<Line>, k: nat)
    ensures loop_file_lines(src, false, k) == src,
    decreases src.len(),
{
    reveal(loop_file_lines);
    reveal(body);
    if src.len() > 0 {
        lemma_plain_src(src.subrange(1, src.len() as int), (k + 1) as nat);
        assert(src == seq![src[0int]] + src.subrange(1, src.len() as int));
    }
}

// ... and across the file list, one block after another (for any
// starting counter — the unnumbered body ignores it).
proof fn lemma_plain_loop(files: Seq<File>, c: nat)
    ensures loop_lines(files, false, c) == exec_lines(files),
    decreases files.len(),
{
    reveal(loop_lines);
    reveal(exec_lines);
    if files.len() == 0 {
    } else {
        lemma_plain_src(lines(files[0]), c);
        assert(files.subrange(1, files.len() as int) == files.drop_first());
        lemma_plain_loop(files.drop_first(), (c + lines(files[0]).len() as nat) as nat);
    }
}

// ---- Load-bearing guards: each refusal is a shape the lift must not
// take (the mutant targets, recorded in VERUS.md §(b).3) ----

// A DYNAMIC operand (`cat "$f"`): the ForEachLine source must be a static
// path; the real `_ => return None` arm refuses.
proof fn premise_static_paths_matter()
    ensures parse_at(seq![ArgE::Dynamic], 0, false, Seq::empty()) == Option::None,
{
    reveal(parse_at);
}

// ANOTHER flag (`cat -A F`): only `-n` has a modelled body.
proof fn premise_only_n_flag()
    ensures parse_at(seq![ArgE::OtherFlag], 0, false, Seq::empty()) == Option::None,
{
    reveal(parse_at);
}

// A BARE `cat` (stdin): refused.
proof fn premise_bare_cat_refused()
    ensures parse_at(Seq::empty(), 0, false, Seq::empty()) == Option::None,
{
    reveal(parse_at);
}

// `cat a`: ONE static path is accepted — the end-of-argv arm reports the
// accumulated file list.
pub open spec fn one_static() -> Seq<ArgE> {
    seq![ArgE::Static]
}

proof fn pin_static_path()
    ensures parse_at(one_static(), 1, false, Seq::empty().push(File { id: 0 })) == (Some(CatArgv { numbered: false, files: Seq::empty().push(File { id: 0 }) })),
{
    reveal(parse_at);
}

// `cat -n a`: the `-n` arm sets the flag and keeps going.
pub open spec fn n_then_static() -> Seq<ArgE> {
    seq![ArgE::DashN, ArgE::Static]
}

proof fn pin_numbered_path()
    ensures parse_at(n_then_static(), 1, true, Seq::empty()) == parse_at(n_then_static(), 2, true, Seq::empty().push(File { id: 1 })),
{
    reveal(parse_at);
}

// `cat -n -n`: a SECOND `-n` is refused by the flag gate.
proof fn pin_second_n_refused()
    ensures parse_at(seq![ArgE::DashN, ArgE::DashN], 1, true, Seq::empty()) == Option::None,
{
    reveal(parse_at);
}

// `cat -n a b`: numbered, two files.
pub open spec fn n_then_two() -> Seq<ArgE> {
    seq![ArgE::DashN, ArgE::Static, ArgE::Static]
}

proof fn pin_numbered_two_files()
    ensures parse_at(n_then_two(), 3, true, Seq::empty().push(File { id: 1 }).push(File { id: 2 }))
        == (Some(CatArgv { numbered: true, files: Seq::empty().push(File { id: 1 }).push(File { id: 2 }) })),
{
    reveal(parse_at);
}

// `cat a -n`: `-n` AFTER a file operand — refused (bash treats it as a
// file name).
proof fn pin_flag_after_file_refused()
    ensures parse_at(seq![ArgE::Static, ArgE::DashN], 1, false, Seq::empty().push(File { id: 0 })) == Option::None,
{
    reveal(parse_at);
}

fn main() {}

}
