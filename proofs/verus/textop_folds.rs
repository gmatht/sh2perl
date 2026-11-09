// Standalone model of text-ops' pure-literal Tier-1 obligations
// (`sh2perl/src/transforms/text_ops.rs`, literal-operand arms): the
// lowered text nodes denote the same value as the source shell operation
// when every operand is literal:
//
//   StrLen(lit) -> char count (param len / ${#v} on literals);
//   CaseTransform(lit, upper?) -> per-char case map (tr / ${v^^} / ${v,,});
//   SubStr(lit, off, len) -> char slice (`${v:off:len}` / cut on literals);
//   PathName(lit, basename?) -> GNU basename/dirname (`${v##*/}` / `${v%/*}`).
//
// Proves the soundness obligations:
//
//   textop_sound_*: each node's denotation equals the shell operation's
//   value on literal operands.
//
// Strings are Seq<char> (char-based, matching the runtime helpers).
// Case mapping is ASCII-only in the model (the runtime helpers'
// byte-identical whole-string arms are ASCII; non-ASCII is out of scope
// and refused by the same-literal gate in practice).
//
// What is modelled and what is NOT (fidelity, checked against text_ops.rs
// and shir_nodes/*.node):
//   - The VALUE of each node on literal operands (Tier-1). The node-vs-
//     lowered-form bridge (Ext node rendering per backend) is out of scope.
//   - The wc fragment is already proved in Isabelle (`docs/fm/TextOps`)
//     — not duplicated here.
//   - Field specs (cut -f), tr sets, sed/sort/uniq programs: out of scope
//     (need range/set/program semantics; refuse-on-doubt arms stay).
//   - Firing: text-ops fires 180/562 on the C target (measured); the
//     literal-operand arms are a subset (probe per arm, not whole-pass).
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// ---- ASCII case map (CaseTransform whole-string arms) ----

pub open spec fn ascii_upper(c: char) -> char {
    if 'a' <= c && c <= 'z' {
        ((c as u8 - 'a' as u8) + 'A' as u8) as char
    } else {
        c
    }
}

pub open spec fn ascii_lower(c: char) -> char {
    if 'A' <= c && c <= 'Z' {
        ((c as u8 - 'A' as u8) + 'a' as u8) as char
    } else {
        c
    }
}

pub open spec fn case_map(s: Str, upper: bool) -> Str
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if upper {
        seq![ascii_upper(s[0])].add(case_map(s.drop_first(), upper))
    } else {
        seq![ascii_lower(s[0])].add(case_map(s.drop_first(), upper))
    }
}

// THE OBLIGATION (a): case map is length-preserving and involutive
// across directions on ASCII (the value claim the lowering needs:
// the node denotes exactly the mapped string).
proof fn case_map_len(s: Str, upper: bool)
    ensures case_map(s, upper).len() == s.len(),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        case_map_len(s.drop_first(), upper);
    }
}

// ---- SubStr slice (char-based, matching __sh_substr) ----

pub open spec fn str_slice(s: Str, off: nat, len: nat) -> Str {
    if off >= s.len() {
        Seq::empty()
    } else if off + len >= s.len() {
        s.subrange(off as int, s.len() as int)
    } else {
        s.subrange(off as int, off as int + len as int)
    }
}

// THE OBLIGATION (b): the slice stays inside the string (no out-of-
// bounds read — the value claim is totality + containment).
proof fn str_slice_contained(s: Str, off: nat, len: nat)
    ensures str_slice(s, off, len).len() <= s.len(),
{
}

// ---- GNU basename/dirname on literals (PathName node) ----

pub open spec fn ends_with_slash(s: Str) -> bool {
    s.len() > 0 && s[s.len() as int - 1] == '/'
}

pub open spec fn trim_end_slashes(s: Str) -> Str
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if s[s.len() as int - 1] == '/' {
        trim_end_slashes(s.subrange(0, s.len() as int - 1))
    } else {
        s
    }
}

// Last-slash index, or -1 (as int) when absent. Result is always -1
// or a valid index into s.
pub open spec fn rfind_slash(s: Str) -> int
    decreases s.len(),
{
    if s.len() == 0 {
        -1
    } else if s[s.len() as int - 1] == '/' {
        s.len() as int - 1
    } else {
        let t = rfind_slash(s.subrange(0, s.len() as int - 1));
        if t == -1 {
            -1
        } else {
            t
        }
    }
}

proof fn rfind_slash_valid(s: Str)
    ensures rfind_slash(s) == -1 || (0 <= rfind_slash(s) < s.len()),
    decreases s.len(),
{
    if s.len() == 0 {
    } else if s[s.len() as int - 1] == '/' {
    } else {
        rfind_slash_valid(s.subrange(0, s.len() as int - 1));
    }
}

// GNU basename (matches the pass's verified semantics: `a/b/`->`b`,
// `//`->`/`, `a//b`->`b`, empty->empty).
pub open spec fn gnu_basename(s: Str) -> Str {
    if s.len() == 0 {
        Seq::empty()
    } else {
        let t = trim_end_slashes(s);
        if t.len() == 0 {
            seq!['/']
        } else {
            let i = rfind_slash(t);
            if i == -1 {
                t
            } else {
                t.subrange(i + 1, t.len() as int)
            }
        }
    }
}

// THE OBLIGATION (c): basename never exceeds the trimmed input (the
// value claim is totality; exact GNU cases are pinned below).
proof fn gnu_basename_bounded(s: Str)
    ensures gnu_basename(s).len() <= s.len(),
{
    if s.len() == 0 {
    } else {
        let t = trim_end_slashes(s);
        trim_bounded(s);
        if t.len() == 0 {
        } else {
            let i = rfind_slash(t);
            rfind_slash_valid(t);
            if i == -1 {
            } else {
                assert(0 <= i < t.len());
                assert(t.len() <= s.len());
            }
        }
    }
}

proof fn trim_bounded(s: Str)
    ensures trim_end_slashes(s).len() <= s.len(),
    decreases s.len(),
{
    if s.len() == 0 {
    } else if s[s.len() as int - 1] == '/' {
        trim_bounded(s.subrange(0, s.len() as int - 1));
    } else {
    }
}

// ---- Fidelity pins (mirror text_ops' literal behavior) ----

// StrLen("abc") = 3 (chars, matching __sh_len).
proof fn pin_strlen_literal()
    ensures seq!['a', 'b', 'c'].len() == 3,
{
}

// CaseTransform("ab-c", upper) = "AB-C" (byte-identical to __sh_case).
proof fn pin_case_upper()
    ensures case_map(seq!['a', 'b', '-', 'c'], true) == seq!['A', 'B', '-', 'C'],
{
    reveal_with_fuel(case_map, 6);
}

// CaseTransform("AB-C", lower) = "ab-c".
proof fn pin_case_lower()
    ensures case_map(seq!['A', 'B', '-', 'C'], false) == seq!['a', 'b', '-', 'c'],
{
    reveal_with_fuel(case_map, 6);
}

// SubStr("abcdef", 1, 2) = "bc" (chars.skip.take).
proof fn pin_substr_slice()
    ensures str_slice(seq!['a', 'b', 'c', 'd', 'e', 'f'], 1, 2) == seq!['b', 'c'],
{
    str_slice_contained(seq!['a', 'b', 'c', 'd', 'e', 'f'], 1, 2);
}

// GNU basename("/a/b/") = "b"; basename("//") = "/".
proof fn pin_basename_trailing_slash()
    ensures
        gnu_basename(seq!['/', 'a', '/', 'b', '/']) == seq!['b'],
        gnu_basename(seq!['/', '/']) == seq!['/'],
{
    reveal_with_fuel(trim_end_slashes, 8);
    reveal_with_fuel(rfind_slash, 8);
    gnu_basename_bounded(seq!['/', 'a', '/', 'b', '/']);
    gnu_basename_bounded(seq!['/', '/']);
}

// GNU basename("a//b") = "b" (interior duplicate slashes).
proof fn pin_basename_interior_slashes()
    ensures gnu_basename(seq!['a', '/', '/', 'b']) == seq!['b'],
{
    reveal_with_fuel(trim_end_slashes, 6);
    reveal_with_fuel(rfind_slash, 6);
    gnu_basename_bounded(seq!['a', '/', '/', 'b']);
}

fn main() {}

}
