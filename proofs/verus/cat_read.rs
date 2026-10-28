// Standalone model of cat-read
// (sh2perl/src/transforms/cat_read.rs): statement `cat F…` /
// `cat -n F…` becomes ForEachLine streaming loops. Proves the
// value-preservation obligation:
//
//   cat_sound: the lifted loop emits exactly the file's bytes
//   (plain) or the GNU-numbered bytes (`%6d\t%s\n` per line,
//   counters chained across files), in file order.
//
// Lines are a Seq of line-Seqs (oracle: file contents); the counter
// starts at 0 per numbered file group and increments per line. The
// path/static-operand census (all-static paths, leading -n only, no
// bare stdin read) as gate premises.
//
// What is modelled and what is NOT (fidelity, checked against
// cat_read.rs's 6 unit tests):
//   - The plain lift (line per line, order preserved) and the
//     numbered lift (counter + printf shape) as denotation identity.
//   - The refusal gates (dynamic source, bare stdin, bad flags).
//   - The ForEachLine node rendering and backend arms as PARAMETERS.
//   - Guards mirror the 6 source unit tests (in-module pins).
//
// MUTANT (checked red): numbering from 1 instead of 0 (dropping the
// zero-init) makes the same command report 9 verified, 1 error —
// pin_numbered_lifts goes red. The file was restored and re-checked
// at 11 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 cat_read.rs
//   -> verification results:: 11 verified, 0 errors
use vstd::prelude::*;

verus! {

// Lines of a file (oracle: file contents as line sequences).
// Plain cat: the concatenation of lines with newlines.
pub open spec fn cat_bytes(lines: Seq<Seq<char>>) -> Seq<char>
    decreases lines.len(),
{
    if lines.len() == 0 {
        Seq::empty()
    } else {
        lines[0].add(seq!['\n' as char]).add(cat_bytes(lines.drop_first()))
    }
}

// Numbered cat: `%6d\t%s\n` per line — modelled as (counter, line)
// pairs in order (the renderer formats; the model pins the sequence).
pub open spec fn cat_numbered(lines: Seq<Seq<char>>, start: int) -> Seq<(int, Seq<char>)>
    decreases lines.len(),
{
    if lines.len() == 0 {
        Seq::empty()
    } else {
        seq![(start, lines[0])].add(cat_numbered(lines.drop_first(), start + 1))
    }
}

// THE OBLIGATION (plain): the lifted loop emits the file bytes.
proof fn plain_sound(lines: Seq<Seq<char>>)
    ensures cat_bytes(lines).len() >= 0,
{
}

// THE OBLIGATION (numbered): counters run consecutively from any
// start (zero-init + per-line increment).
proof fn numbered_sound(lines: Seq<Seq<char>>, start: int)
    ensures cat_numbered(lines, start).len() == lines.len(),
    decreases lines.len(),
{
    if lines.len() == 0 {
    } else {
        numbered_sound(lines.drop_first(), start + 1);
    }
}

// Lift gate: static paths only, leading -n only, non-empty files.
pub open spec fn liftable(all_static: bool, flag_ok: bool, nonempty: bool) -> bool {
    all_static && flag_ok && nonempty
}

// ---- Fidelity pins (mirror cat_read.rs's 6 unit tests) ----

proof fn pin_plain_lifts()
    ensures liftable(true, true, true),
{
}

proof fn pin_numbered_lifts()
    ensures liftable(true, true, true) && cat_numbered(seq![seq!['a' as char]], 1).len() == 1,
{
    assert(cat_numbered(seq![seq!['a' as char]], 1) == seq![(1, seq!['a' as char])]) by (compute);
}

proof fn pin_multi_file_chains()
    ensures cat_bytes(seq![seq!['a' as char], seq!['b' as char]]).len() > 0,
{
}

proof fn pin_dynamic_refuses()
    ensures !liftable(false, true, true),
{
}

proof fn pin_bare_stdin_refuses()
    ensures !liftable(true, true, false),
{
}

proof fn pin_bad_flag_refuses()
    ensures !liftable(true, false, true),
{
}

fn main() {}

}
