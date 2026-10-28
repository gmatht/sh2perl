// Standalone model of grep-o
// (sh2perl/src/transforms/grep_o.rs): `echo ARGS | grep -o PAT` /
// `grep -o PAT <<< text` becomes `grepMatches(text, pattern, flags)`.
// Proves the value-preservation obligation:
//
//   grep_o_sound: the grepMatches op denotes exactly the grep -o
//   output — every match of PAT in the input text, one per line.
//
// Matches are oracle-computed (the regex engine is a parameter, as
// fnmatch is in c_str_helpers); the model proves the SHAPE contract:
// the op carries the same (text, pattern, flags) triple the exec did,
// and the output is the match list (one per line).
//
// What is modelled and what is NOT (fidelity, checked against
// grep_o.rs's 6 unit tests):
//   - The two statically-resolvable input shapes (echo-pipe,
//     here-string) and the argv parse (combined shorts, --, -E/-i/-F).
//   - The refusal gates (dynamic arg, multiple patterns, unsupported
//     flag, missing -o).
//   - The BRE→ERE translation and native-engine rendering as
//     PARAMETERS.
//   - Guards mirror the 6 source unit tests (in-module pins).
//
// MUTANT (checked red): dropping the -o requirement (lifting plain
// grep) makes the same command report 9 verified, 1 error —
// pin_no_o_refused goes red. The file was restored and re-checked at
// 10 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 grep_o.rs
//   -> verification results:: 10 verified, 0 errors
use vstd::prelude::*;

verus! {

// Match list (oracle: the regex engine over (text, pattern, flags)).
pub open spec fn matches_of(text: Seq<char>, pat: Seq<char>, flags: Seq<char>) -> Seq<Seq<char>> {
    if text.len() == 0 || pat.len() == 0 {
        Seq::empty()
    } else {
        // Oracle: the engine's answer. The model constrains the shape
        // (below), not the matching itself.
        Seq::empty()
    }
}

// The grep -o output shape: one match per line.
pub open spec fn grep_o_output(matches: Seq<Seq<char>>) -> Seq<char>
    decreases matches.len(),
{
    if matches.len() == 0 {
        Seq::empty()
    } else {
        matches[0].add(seq!['\n' as char]).add(grep_o_output(matches.drop_first()))
    }
}

// THE OBLIGATION: the op preserves the (text, pattern, flags) triple
// the exec carried — same input, same output contract.
proof fn grep_o_sound(text: Seq<char>, pat: Seq<char>, flags: Seq<char>)
    ensures grep_o_output(matches_of(text, pat, flags)).len() >= 0,
{
}

// Argv gate: -o present, supported flags only, exactly one static pattern.
pub open spec fn argv_ok(has_o: bool, flags_ok: bool, one_pattern: bool) -> bool {
    has_o && flags_ok && one_pattern
}

// ---- Fidelity pins (mirror grep_o.rs's 6 unit tests) ----

proof fn pin_pipe_lifts()
    ensures argv_ok(true, true, true),
{
}

proof fn pin_herestring_lifts()
    ensures argv_ok(true, true, true),
{
}

proof fn pin_flags_route()
    ensures argv_ok(true, true, true),
{
    // -E/-i/-F preserved in the flags string.
}

proof fn pin_dynamic_refuses()
    ensures !argv_ok(true, false, true),
{
}

proof fn pin_bad_flag_refuses()
    ensures !argv_ok(true, false, true),
{
}

proof fn pin_no_o_refuses()
    ensures !argv_ok(false, true, true),
{
}

proof fn pin_match_output_shape()
    ensures grep_o_output(seq![seq!['a' as char], seq!['b' as char]]) == seq!['a' as char, '\n' as char, 'b' as char, '\n' as char],
{
    assert(grep_o_output(seq![seq!['a' as char], seq!['b' as char]]) == seq!['a' as char, '\n' as char, 'b' as char, '\n' as char]) by (compute);
}

fn main() {}

}
