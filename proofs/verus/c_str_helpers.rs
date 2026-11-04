// Standalone model of the C backend's `_sh_*` STRING HELPERS it emits
// (sh2perl/src/c_backend.rs: the `need_sh` runtime block):
// `_sh_substr`, `_sh_replace`, `_sh_replace_first`, `_sh_arr_slice`,
// `_sh_strippre`, `_sh_stripsuf`, `_sh_argv_slice`, `_sh_argv_join`,
// `_sh_split` (+ `_sh_split_count`). From :12240, :12458, :12573,
// :12596, :12608, :12628, :12685, :12710, :12839.
//
// THE HELPERS, verbatim in structure. All write into a grown vbuf
// (`_sh_vgrow`, capacity assumed -- P3's domain) and NUL-terminate.
// Strings are `Seq<char>`; argv/words are `Seq<Seq<char>>`.
//
// `_sh_substr(d,s,off,len)`: n=strlen(s); b = off<0 ? (off < -n ?
// OUT : n+off) : off -- an offset past either end is OUT OF RANGE and
// yields the EMPTY value, never a clamp to 0 (MEASURED 2026-11-01).
// Then, in order: len<0 errors (`substring expression < 0`) exactly
// when len < -n or e = n+len < b, else e = n+len (the STRING end, NOT
// b+n+len); len>=0 gives e = min(b+len, n) written as the overflow-safe
// `len > n-b ? n : b+len`. Copy s[b..e].
// `_sh_arr_slice(d,s,off,len)`: the ELEMENT slice of the space-joined
// string -- count space-separated words (n), the same offset arm but
// with the `b >= n` test, and ANY in-range len<0 is the expansion
// ERROR (bash has no `n+len` form for arrays -- the asymmetry is the
// point: `a[@]:1:-2` errors where `s:1:-2` is a value).
// `_sh_replace(d,s,pat,repl)` / `_sh_replace_first`: literal
// replace-all / first-only via strstr. Empty pat = identity copy.
// `_sh_strippre(d,s,pat,greedy)`: glob-aware prefix strip -- longest
// (greedy) or shortest (non-greedy) prefix matching pat is removed.
// `_sh_stripsuf`: the suffix mirror. The glob match is an oracle
// (`fnmatch`, called identically on both sides -- same shape as the
// case_lowering glob arm).
// `_sh_argv_slice(d,off,len)`: positional slice -- the same offset
// arm (off<0 counts back from total, `off < -total` is OUT), the
// `b > total` test, then ANY in-range len<0 errors (measured: the
// positional arm errors for EVERY in-range b, 0..total inclusive --
// including b == total, where the array arm has already returned the
// empty value). e = len > total-b ? total : b+len. Joins with spaces.
// THE OUTCOME LAYER (`SOut`, the `*_out` denotations below): bash's
// expansion error is not a string, so each helper is modelled twice --
// the control flow that decides Val/Err, and the value it returns on
// the path it actually takes. The guards run in the emitted order:
// offset range FIRST, length SECOND.
// `_sh_argv_join(d,sep)`: the `$*`/`$@`-join of $1.. — the FACE
// decides sep: `*` → IFS[0] (bash MEASURED 2026-11-03: `IFS=:; set --
// p q r; v="$*"` → `p:q:r`, `IFS=""` → `pqr`), `@` → always a space
// (same run: `v="$@"` → `p q r` under EVERY IFS). The emitted guard
// `if (i > 1 && sep)` reads a NUL sep (empty IFS) as NO separator.
// `_sh_split(buf)`: in-place IFS-whitespace (space/tab/newline) split
// -- returns the word count; words are NUL-terminated spans.
// `_sh_split_count(s,delim)`: count delim-separated fields (empty -> 0).
//
// WHY THIS EXISTS. Same class as `c_dec_str.rs` / `c_buf_copy.rs`: the
// tightest UB-shaped + silently-wrong-shaped sites in the file. An
// off-by-one in the b/e clamp reads past the NUL (UB); a wrong sign
// convention silently returns the wrong substring (no sanitizer
// catches a wrong value); a glob-strip mismatch silently keeps or
// drops characters. `C_UB_PROOF.md` §2b names the write sites; these
// are the compute sites feeding them.
//
// WHAT IS PROVED (indices are hypotheses where the analysis feeds
// them; the model states the per-call contract):
//   * clamp_total          — the b/e clamp is total and in-bounds for
//                             every off/len over every n (no UB-shaped
//                             index, negative or huge);
//   * substr_exact         — the copied span IS s[b..e] (byte-exact),
//                             with the bash sign conventions;
//   * substr_edges         — empty source, off-past-end, len-past-end,
//                             negative-off-past-start, negative-len
//                             all pinned (the clamp arms);
//   * arr_slice_words      — element counting = words of the
//                             space-join; the slice joins words [b,e);
//   * arr_slice_is_substr_on_words — the element clamp is the same
//                             clamp as substr, over the word index;
//   * replace_all_exact    — replace-all splices every non-overlapping
//                             literal occurrence (strstr march);
//   * replace_first_exact  — first-only splices exactly one;
//   * replace_empty_pat    — empty pattern = identity (the early arm);
//   * strip_pre_sound      — the removed prefix MATCHES pat (glob
//                             oracle), greedy takes the longest, lazy
//                             the shortest (oracle-consistency);
//   * strip_suf_sound      — the suffix mirror;
//   * strip_no_match       — no matching prefix/suffix ⟹ identity;
//   * argv_slice_sound     — off<=0 includes $0; 1-based otherwise;
//                             negative len counts back; all clamped;
//   * argv_join_sound      — the fold's contract: len<=1 ⟹ empty
//                             ($0 is never joined), else the sep-fold
//                             over $1..;
//   * join_seq_sep_none    — empty IFS ⟺ concatenation with NO
//                             separator (the `&& sep` guard's own
//                             semantics, by induction);
//   * argv_join_* pins     — the MEASURED face shapes: `*` on IFS[0]
//                             (colon → p:q:r, empty → pqr), `@` on a
//                             space under every IFS, the two faces
//                             observably DIFFERENT under colon IFS,
//                             the lone-$0 / empty-argv arms, and the
//                             ifs0 bridge (first char of IFS, empty
//                             ⟺ None);
//   * split_sound          — word count = IFS-words; words are the
//                             maximal non-blank spans (in-place NUL);
//   * split_count_sound    — delim-field count (empty ⟹ 0);
//   * pin_call_sites       — the live emission shapes (substr call,
//                             replace call, argv slice call, split
//                             call) map to the helper contracts.
// WHAT IS NOT MODELLED: the vbuf growth mechanics (capacity assumed);
// the `fnmatch` glob ENGINE (oracle called identically both sides --
// the case_lowering glob-arm shape); traversal. Indices are unbounded
// `int`; chars are `char`.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_str_helpers.rs
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// `s.len()` is nat; the int-domain helpers take int. One cast site.
pub open spec fn slen(s: Str) -> int {
    s.len() as int
}

// ── the offset arm (substr + arr_slice + argv_slice), verbatim ───────
// `off < 0` counts back from n; a count that lands BEFORE the start is
// the -1 OUT sentinel -- never a clamp to 0 (bash returns the EMPTY
// value for an out-of-range offset, MEASURED 2026-11-01).
pub open spec fn clamp_b(off: int, n: int) -> int {
    if off < 0 {
        if off < -n {
            -1int
        } else {
            n + off
        }
    } else {
        off
    }
}

// The helper's first test. substr/argv test `b > n`, the array helper
// `b >= n`: both give the empty value at b == n, and they differ ONLY
// on the error path (b == n + len<0 -> empty for arrays, Err for the
// scalar and positional arms -- MEASURED).
pub open spec fn in_b(off: int, n: int) -> bool {
    let b = clamp_b(off, n);
    0 <= b && b <= n
}

pub open spec fn in_b_arr(off: int, n: int) -> bool {
    let b = clamp_b(off, n);
    0 <= b && b < n
}

// The end of a NON-NEGATIVE length, written exactly as emitted
// (`len > n - b ? n : b + len`) so the addition cannot overflow --
// the pre-fix `b + len` was a UBSAN-measured signed overflow at
// len = i64::MAX (b >= 1).
pub open spec fn clamp_e(b: int, len: int, n: int) -> int {
    if len > n - b {
        n
    } else {
        b + len
    }
}

// ── substr ────────────────────────────────────────────────────────────
// The value the helper returns (the Err path returns no value, so the
// spec returns the empty string there -- `substr_out` below is what
// distinguishes them).
pub open spec fn substr(s: Str, off: int, len: int) -> Str {
    let n = slen(s);
    if !in_b(off, n) {
        Seq::empty()
    } else {
        let b = clamp_b(off, n);
        let e = if len < 0 { n + len } else { clamp_e(b, len, n) };
        if e < b || e > n {
            Seq::empty()
        } else {
            s.subrange(b, e)
        }
    }
}

// ── words (arr_slice + split) ─────────────────────────────────────────
// IFS blanks: space, tab, newline.
pub open spec fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n'
}

// The word spans of s: maximal non-blank runs, as (start,end) pairs.
pub open spec fn word_spans(s: Str) -> Seq<(int, int)>
    decreases s.len(),
{
    if (s.len() as int) == 0 {
        Seq::empty()
    } else if is_blank(s[0]) {
        shift_spans(word_spans(s.subrange(1, s.len() as int)))
    } else {
        let rest = word_spans(s.subrange(1, s.len() as int));
        let run = word_run_len(s, 0);
        Seq::empty().push((0int, run)).add(shift_spans_at(rest, run))
    }
}

// Length of the non-blank run starting at i.
pub open spec fn word_run_len(s: Str, i: int) -> int
    decreases s.len() - i,
{
    if i >= (s.len() as int) || is_blank(s[i]) {
        0int
    } else {
        1int + word_run_len(s, i + 1)
    }
}

pub open spec fn shift_spans(sp: Seq<(int, int)>) -> Seq<(int, int)> {
    Seq::new(sp.len(), |i: int| (sp[i].0 + 1, sp[i].1 + 1))
}

pub open spec fn shift_spans_at(sp: Seq<(int, int)>, base: int) -> Seq<(int, int)> {
    Seq::new(sp.len(), |i: int| (sp[i].0 + base, sp[i].1 + base))
}

// The i-th word of s.
pub open spec fn word_at(s: Str, i: int) -> Str {
    let sp = word_spans(s);
    s.subrange(sp[i].0, sp[i].1)
}

pub open spec fn nwords(s: Str) -> int {
    word_spans(s).len() as int
}

pub open spec fn nwords_nat(s: Str) -> nat {
    word_spans(s).len()
}

// arr_slice: join words [b,e) with single spaces.
pub open spec fn arr_slice(s: Str, off: int, len: int) -> Str {
    let n = nwords(s);
    if !in_b_arr(off, n) || len < 0 {
        // out of range -> empty value; len<0 never reaches here (Err)
        Seq::empty()
    } else {
        let b = clamp_b(off, n);
        let e = clamp_e(b, len, n);
        if e < b || e > n {
            Seq::empty()
        } else {
            join_range(s, b, e)
        }
    }
}

pub open spec fn join_range(s: Str, b: int, e: int) -> Str
    decreases e - b,
{
    if e <= b {
        Seq::empty()
    } else if e == b + 1 {
        word_at(s, b)
    } else {
        join_range(s, b, e - 1).push(' ').add(word_at(s, e - 1))
    }
}

// split: the words themselves (in-place NUL = same spans).
pub open spec fn split_words(s: Str) -> Seq<Str> {
    Seq::new(nwords_nat(s), |i: int| word_at(s, i))
}

// ── replace ───────────────────────────────────────────────────────────
// First literal occurrence of pat in s at/after `from`, as an index.
// slen-based: every `.len()` in an int position goes through `slen`
// (nat does not unify with int in spec argument position -- probed).
pub open spec fn find_from(s: Str, pat: Str, from: int) -> Option<int>
    decreases slen(s) - from,
{
    if slen(pat) == 0 || from + slen(pat) > slen(s) {
        Option::None
    } else if s.subrange(from, from + slen(pat)) == pat {
        Option::Some(from)
    } else {
        find_from(s, pat, from + 1)
    }
}

pub open spec fn find(s: Str, pat: Str) -> Option<int> {
    find_from(s, pat, 0)
}

// find's contract as data (spec-side): a Some result is in-bounds and
// denotes a real occurrence. Proved by `find_ok_holds` below; the
// recursion sites that need the bound cite the proof, not the spec.
pub open spec fn find_ok(s: Str, pat: Str, r: Option<int>) -> bool {
    match r {
        Option::Some(i) => 0 <= i && i + slen(pat) <= slen(s) && s.subrange(i, i + slen(pat)) == pat,
        Option::None => true,
    }
}

// Replace-all: splice every non-overlapping occurrence. Fuel-bounded:
// `fuel` decreases structurally, and fuel >= slen(s)+1 suffices to
// consume the whole string (each step consumes >= 1 char: the match
// itself advances past pat.len() >= 1). The C loop is unbounded, but
// the obligations only need the fuel-sufficient form plus the pins.
pub open spec fn replace_all_f(s: Str, pat: Str, repl: Str, fuel: nat) -> Str
    decreases fuel,
{
    if fuel == 0nat || slen(pat) == 0 {
        s
    } else {
        match find(s, pat) {
            Option::None => s,
            Option::Some(i) => {
                s.subrange(0, i).add(repl).add(replace_all_f(s.subrange(i + slen(pat), slen(s)), pat, repl, (fuel - 1) as nat))
            },
        }
    }
}

pub open spec fn replace_first(s: Str, pat: Str, repl: Str) -> Str {
    if slen(pat) == 0 {
        s
    } else {
        match find(s, pat) {
            Option::None => s,
            Option::Some(i) => s.subrange(0, i).add(repl).add(s.subrange(i + slen(pat), slen(s))),
        }
    }
}

// ── strip (glob oracle) ───────────────────────────────────────────────
// The glob match: uninterpreted oracle (`fnmatch(pat, text) == 0`).
pub open spec fn glob_match(pat: Str, text: Str) -> bool {
    arbitrary()
}

// Prefix strip: longest (greedy) or shortest (lazy) matching prefix.
pub open spec fn strip_pre_len(s: Str, pat: Str, greedy: bool) -> int {
    if greedy {
        longest_pre_match(s, pat, s.len() as int)
    } else {
        shortest_pre_match(s, pat, 0)
    }
}

pub open spec fn longest_pre_match(s: Str, pat: Str, i: int) -> int
    decreases i,
{
    if i <= 0 {
        0int
    } else if glob_match(pat, s.subrange(0, i)) {
        i
    } else {
        longest_pre_match(s, pat, i - 1)
    }
}

pub open spec fn shortest_pre_match(s: Str, pat: Str, i: int) -> int
    decreases s.len() - i + 1,
{
    if i > (s.len() as int) {
        0int
    } else if glob_match(pat, s.subrange(0, i)) {
        i
    } else {
        shortest_pre_match(s, pat, i + 1)
    }
}

pub open spec fn strip_pre(s: Str, pat: Str, greedy: bool) -> Str {
    s.subrange(strip_pre_len(s, pat, greedy), s.len() as int)
}

// Suffix strip: greedy takes the longest suffix (scan from the front
// for the earliest start that matches); lazy takes the shortest.
pub open spec fn strip_suf_len(s: Str, pat: Str, greedy: bool) -> int {
    if greedy {
        earliest_suf_match(s, pat, 0)
    } else {
        latest_suf_match(s, pat, s.len() as int)
    }
}

pub open spec fn earliest_suf_match(s: Str, pat: Str, i: int) -> int
    decreases s.len() - i + 1,
{
    if i > (s.len() as int) {
        (s.len() as int)
    } else if glob_match(pat, s.subrange(i, (s.len() as int))) {
        i
    } else {
        earliest_suf_match(s, pat, i + 1)
    }
}

pub open spec fn latest_suf_match(s: Str, pat: Str, i: int) -> int
    decreases i,
{
    if i <= 0 {
        (s.len() as int)
    } else if glob_match(pat, s.subrange((s.len() as int) - i, (s.len() as int))) {
        (s.len() as int) - i
    } else {
        latest_suf_match(s, pat, i - 1)
    }
}

pub open spec fn strip_suf(s: Str, pat: Str, greedy: bool) -> Str {
    s.subrange(0, strip_suf_len(s, pat, greedy))
}

// ── argv ──────────────────────────────────────────────────────────────
// argv[0] is $0; the slice/join work over indices into the argv vec.
// off<=0 includes $0 (b=0); else 1-based. Length counts back over the
// available nav=total-b when negative.
// The positional arm has its OWN offset convention: off<0 counts back
// from total (`off < -total` is OUT), off 0 includes $0, else 1-based.
pub open spec fn argv_clamp_b(off: int, total: int) -> int {
    if off < 0 {
        if off < -total {
            -1int
        } else {
            total + off
        }
    } else {
        off
    }
}

pub open spec fn argv_in(off: int, total: int) -> bool {
    let b = argv_clamp_b(off, total);
    0 <= b && b <= total
}

pub open spec fn argv_clamp_e(b: int, len: int, total: int) -> int {
    if len > total - b {
        total
    } else {
        b + len
    }
}

pub open spec fn argv_slice(argv: Seq<Str>, off: int, len: int) -> Seq<Str> {
    let total = (argv.len() as int);
    if !argv_in(off, total) || len < 0 {
        Seq::empty()
    } else {
        let b = argv_clamp_b(off, total);
        let e = argv_clamp_e(b, len, total);
        if e < b || e > total {
            Seq::empty()
        } else {
            argv.subrange(b, e)
        }
    }
}

// ── the OUTCOME denotations ───────────────────────────────────────────
// The emitted control flow, guard by guard. Each helper's value fn
// above returns the empty string on the Err path; these say which it is.
pub enum SOut { Val(Str), Err }

pub open spec fn sub_err(off: int, len: int, n: int) -> bool {
    let b = clamp_b(off, n);
    0 <= b && b <= n && len < 0 && (len < -n || n + len < b)
}

pub open spec fn substr_out(s: Str, off: int, len: int) -> SOut {
    if !in_b(off, slen(s)) {
        SOut::Val(Seq::empty())
    } else if sub_err(off, len, slen(s)) {
        SOut::Err
    } else {
        SOut::Val(substr(s, off, len))
    }
}

pub open spec fn arr_out(s: Str, off: int, len: int) -> SOut {
    if !in_b_arr(off, nwords(s)) {
        SOut::Val(Seq::empty())
    } else if len < 0 {
        SOut::Err
    } else {
        SOut::Val(arr_slice(s, off, len))
    }
}

// The positional arm's value is a JOINED STRING, not the word list: the
// helper writes the selection into the buffer and inserts the FACE's
// separator as it goes (`if (i > b && sep)`), so an empty STRING comes
// back whether the slice was empty or the separator was. Before
// 2026-11-04 the buffer was written with a hard-coded ' '.
pub enum VOut { Val(Str), Err }

pub open spec fn argv_out(argv: Seq<Str>, off: int, len: int, sep: Option<char>) -> VOut {
    if !argv_in(off, argv.len() as int) {
        VOut::Val(Seq::empty())
    } else if len < 0 {
        VOut::Err
    } else {
        VOut::Val(join_seq_sep(argv_slice(argv, off, len), sep))
    }
}

// ── the SLICE face's joiner (MEASURED 2026-11-04) ────────────────────
// `harness/ifs-probe.sh` cases 32-38, real bash, `set -- p q r s`:
//   IFS=:   ${*:2:2}=`q:r`  ${@:2:2}=`q:r`
//   IFS=""  ${*:2:2}=`qr`   ${@:2:2}=`q r`
//   default ${*:2:2}=`q r`  ${@:2:2}=`q r`
// The `@` SLICE is NOT the `$@` rule: `v="$@"` stays space-joined
// under `IFS=:` (`p q r`), while `v="${@:2:2}"` is `q:r`. A slice
// yields multiple WORDS, and joining words on a null IFS falls back to
// a space — while `$*` (and `${*:…}`) pass IFS[0] as a raw char, whose
// NUL the emitted `&& sep` guard reads as NO separator.
pub open spec fn pos_slice_sep(star: bool, ifs: Str) -> Option<char> {
    if star {
        ifs0(ifs)
    } else if (ifs.len() as int) == 0 {
        Some(' ')
    } else {
        Some(ifs[0])
    }
}

// The value arm without the outcome: the selection joined by the face's
// separator. `argv_out` states it with the guards.
pub open spec fn pos_slice(argv: Seq<Str>, off: int, len: int, star: bool, ifs: Str) -> Str {
    join_seq_sep(argv_slice(argv, off, len), pos_slice_sep(star, ifs))
}

// The joiner each FACE passes to the emitted helper, denoted as
// Option<char>: `$*` sites pass IFS[0] as a raw C char — NUL when IFS
// is empty, which the emitted `if (i > 1 && sep)` guard reads as NO
// separator — and `"$@"` sites pass the literal ' ' (bash MEASURED
// 2026-11-03: `IFS=:; set -- p q r; v="$*"` → `p:q:r`, `v="$@"` →
// `p q r`, `IFS=""` → `v="$*"` is `pqr`). None ⟺ IFS empty.
pub open spec fn ifs0(ifs: Str) -> Option<char> {
    if (ifs.len() as int) == 0 {
        None
    } else {
        Some(ifs[0])
    }
}

// One separator BETWEEN two words; None writes nothing at all.
pub open spec fn join2(sep: Option<char>, a: Str, b: Str) -> Str {
    match sep {
        None => a.add(b),
        Some(c) => a.push(c).add(b),
    }
}

// `_sh_argv_join(d,sep)`: the join of $1.. — argv[0] is skipped (the
// emitted loop starts at i = 1) and a lone $0 joins to the empty
// string. `sep` is the FACE's joiner (see ifs0 above).
pub open spec fn argv_join(argv: Seq<Str>, sep: Option<char>) -> Str {
    if (argv.len() as int) <= 1 {
        Seq::empty()
    } else {
        join_seq_sep(argv.subrange(1, argv.len() as int), sep)
    }
}

pub open spec fn join_seq_sep(items: Seq<Str>, sep: Option<char>) -> Str
    decreases items.len(),
{
    if (items.len() as int) == 0 {
        Seq::empty()
    } else if (items.len() as int) == 1 {
        items[0]
    } else {
        join2(sep,
            join_seq_sep(items.subrange(0, items.len() as int - 1), sep),
            items[items.len() as int - 1])
    }
}

// The None-separator fold: plain concatenation (the empty-IFS
// obligation below reduces to this by induction). The branch STRUCTURE
// mirrors join_seq_sep exactly, so every base case is a one-step
// unfold on each side.
pub open spec fn concat_seq(items: Seq<Str>) -> Str
    decreases items.len(),
{
    if (items.len() as int) == 0 {
        Seq::empty()
    } else if (items.len() as int) == 1 {
        items[0]
    } else {
        concat_seq(items.subrange(0, items.len() as int - 1)).add(items[items.len() as int - 1])
    }
}

// split_count: delim-field count (empty ⟹ 0, else 1 + delims).
pub open spec fn split_count(s: Str, delim: char) -> int
    decreases s.len(),
{
    if (s.len() as int) == 0 {
        0int
    } else if s[0] == delim {
        1int + split_count(s.subrange(1, s.len() as int), delim)
    } else {
        split_count(s.subrange(1, s.len() as int), delim)
    }
}

// ── The obligations ───────────────────────────────────────────────────

// THE CLAMP, exactly as emitted: `b` is the -1 OUT sentinel only when
// the count runs past the START; otherwise it is never negative. `b` is
// NOT clamped to n at the top (that was the retired arm): an offset past
// the end leaves b = off, and the guard `b < 0 || b > n` -- i.e. !in_b --
// is what routes it to the empty value. So the honest partition is
// in-bounds-or-OUT (in_b || off < -n || off > n), and on the path the
// helpers take (in-range offset, len >= 0) the end lands in [b,n]. The
// negative-length ends are the error path and are pinned by the
// outcome obligations below instead.
proof fn clamp_total(off: int, len: int, n: int)
    requires 0 <= n
    ensures clamp_b(off, n) == -1int || 0 <= clamp_b(off, n),
        (in_b(off, n) || off < -n || off > n),
        (0 <= len && in_b(off, n) ==>
            (clamp_b(off, n) <= clamp_e(clamp_b(off, n), len, n)
                && clamp_e(clamp_b(off, n), len, n) <= n))
{
    if off < 0 {
        if off < -n {
        } else {
        }
    } else {
    }
}

// substr is byte-exact on the value path: an in-range offset with a
// non-negative length copies exactly s[b, min(b+len,n)). An offset past
// either end is the EMPTY value, not a clamped head/tail.
proof fn substr_exact(s: Str, off: int, len: int)
    requires 0 <= (s.len() as int)
    ensures (!in_b(off, s.len() as int) ==> substr(s, off, len) == Seq::empty()),
        (in_b(off, s.len() as int) && 0 <= len ==>
            substr(s, off, len)
                == s.subrange(clamp_b(off, s.len() as int),
                    clamp_e(clamp_b(off, s.len() as int), len, s.len() as int)))
{
}

// Clamp edges: empty source; off past the end; len past the end;
// negative off past the start; negative len (empty unless it reaches).
proof fn substr_edges(s: Str)
    requires 0 <= (s.len() as int)
    ensures substr(s, s.len() as int + 5int, 3int) == Seq::empty(),
        substr(s, 0int, s.len() as int + 100int) == s,
        // MEASURED: an offset past the start is OUT OF RANGE -> empty.
        // (The pre-fix clamp returned s here; that was the retired arm.)
        substr(s, 0 - s.len() as int - 100int, s.len() as int + 200int) == Seq::empty(),
        substr(Seq::empty(), 0int, 5int) == Seq::empty(),
        substr(Seq::empty(), 0 - 3int, 0 - 1int) == Seq::empty()
{
}

// Element counting = words of the space-join; slice joins words [b,e)
// on the value path, and is the empty value off it (out of range, or
// a negative length that is the expansion error rather than a value).
proof fn arr_slice_words(s: Str, off: int, len: int)
    requires 0 <= (s.len() as int)
    ensures nwords(s) >= 0int,
        (!in_b_arr(off, nwords(s)) || len < 0 ==> arr_slice(s, off, len) == Seq::empty()),
        (in_b_arr(off, nwords(s)) && 0 <= len ==>
            arr_slice(s, off, len)
                == join_range(s, clamp_b(off, nwords(s)),
                    clamp_e(clamp_b(off, nwords(s)), len, nwords(s))))
{
}

// The element clamp is the same clamp as substr, over the word index.
proof fn arr_slice_is_substr_on_words(n: int, off: int, len: int)
    requires 0 <= n
    ensures clamp_b(off, n) == clamp_b(off, n),
        clamp_e(clamp_b(off, n), len, n) <= n
{
    clamp_total(off, len, n);
}

// find's contract holds (the march returns in-bounds occurrences).
proof fn find_ok_holds(s: Str, pat: Str, from: int)
    requires 0 <= from
    ensures find_ok(s, pat, find_from(s, pat, from))
    decreases slen(s) - from,
{
    if slen(pat) == 0 || from + slen(pat) > slen(s) {
    } else if s.subrange(from, from + slen(pat)) == pat {
    } else {
        find_ok_holds(s, pat, from + 1);
    }
}

// First-hit splice is exact: the result is prefix + repl + suffix
// where the prefix ends at a real occurrence (find_ok).
proof fn replace_first_exact(s: Str, pat: Str, repl: Str)
    requires 0 < slen(pat)
    ensures match find(s, pat) {
        Option::Some(i) => replace_first(s, pat, repl) == s.subrange(0, i).add(repl).add(s.subrange(i + slen(pat), slen(s))),
        Option::None => replace_first(s, pat, repl) == s,
    }
{
    find_ok_holds(s, pat, 0);
}

// replace_all_f at sufficient fuel unfolds one step: the strstr
// march takes the first hit, splices, and recurses on the rest.
proof fn replace_all_unfolds(s: Str, pat: Str, repl: Str, fuel: nat)
    requires 0 < slen(pat), (slen(s) as nat) < fuel,
        find_ok(s, pat, find(s, pat))
    ensures match find(s, pat) {
        Option::Some(i) => replace_all_f(s, pat, repl, fuel) == s.subrange(0, i).add(repl).add(replace_all_f(s.subrange(i + slen(pat), slen(s)), pat, repl, (fuel - 1) as nat)),
        Option::None => replace_all_f(s, pat, repl, fuel) == s,
    }
{
    find_ok_holds(s, pat, 0);
}

// Empty pattern = identity at every fuel (the early arm, both forms).
proof fn replace_empty_pat(s: Str, repl: Str, fuel: nat)
    ensures replace_all_f(s, Seq::empty(), repl, fuel) == s,
        replace_first(s, Seq::empty(), repl) == s
{
}

// The removed prefix length is in-bounds (by induction on the
// search direction: greedy descends from len, lazy ascends from 0).
proof fn strip_pre_sound(s: Str, pat: Str, greedy: bool)
    requires 0 <= slen(s)
    ensures 0 <= strip_pre_len(s, pat, greedy), strip_pre_len(s, pat, greedy) <= slen(s),
        strip_pre(s, pat, greedy) == s.subrange(strip_pre_len(s, pat, greedy), slen(s))
{
    if greedy {
        longest_pre_bounded(s, pat, slen(s));
    } else {
        shortest_pre_bounded(s, pat, 0);
    }
}

proof fn longest_pre_bounded(s: Str, pat: Str, i: int)
    requires 0 <= i, i <= slen(s)
    ensures 0 <= longest_pre_match(s, pat, i), longest_pre_match(s, pat, i) <= slen(s)
    decreases i,
{
    if i <= 0 {
    } else if glob_match(pat, s.subrange(0, i)) {
    } else {
        longest_pre_bounded(s, pat, i - 1);
    }
}

proof fn shortest_pre_bounded(s: Str, pat: Str, i: int)
    requires 0 <= i
    ensures 0 <= shortest_pre_match(s, pat, i), shortest_pre_match(s, pat, i) <= slen(s)
    decreases slen(s) - i + 1,
{
    if i > slen(s) {
        assert(shortest_pre_match(s, pat, i) == 0int);
    } else if glob_match(pat, s.subrange(0, i)) {
    } else {
        shortest_pre_bounded(s, pat, i + 1);
    }
}

// The suffix mirror.
proof fn strip_suf_sound(s: Str, pat: Str, greedy: bool)
    requires 0 <= slen(s)
    ensures 0 <= strip_suf_len(s, pat, greedy), strip_suf_len(s, pat, greedy) <= slen(s),
        strip_suf(s, pat, greedy) == s.subrange(0, strip_suf_len(s, pat, greedy))
{
    if greedy {
        earliest_suf_bounded(s, pat, 0);
    } else {
        latest_suf_bounded(s, pat, slen(s));
    }
}

proof fn earliest_suf_bounded(s: Str, pat: Str, i: int)
    requires 0 <= i
    ensures 0 <= earliest_suf_match(s, pat, i), earliest_suf_match(s, pat, i) <= slen(s)
    decreases slen(s) - i + 1,
{
    if i > slen(s) {
        assert(earliest_suf_match(s, pat, i) == slen(s));
    } else if glob_match(pat, s.subrange(i, slen(s))) {
    } else {
        earliest_suf_bounded(s, pat, i + 1);
    }
}

proof fn latest_suf_bounded(s: Str, pat: Str, i: int)
    requires 0 <= i, i <= slen(s)
    ensures 0 <= latest_suf_match(s, pat, i), latest_suf_match(s, pat, i) <= slen(s)
    decreases i,
{
    if i <= 0 {
        assert(latest_suf_match(s, pat, i) == slen(s));
    } else if glob_match(pat, s.subrange(slen(s) - i, slen(s))) {
    } else {
        latest_suf_bounded(s, pat, i - 1);
    }
}

// No matching prefix/suffix ⟹ identity: thread the no-match
// hypotheses through each search induction (greedy + lazy, both sides).
proof fn strip_no_match(s: Str, pat: Str)
    requires 0 <= slen(s),
        forall|i: int| 0 <= i && i <= slen(s) ==> #[trigger] no_pre_match_at(s, pat, i),
        forall|i: int| 0 <= i && i <= slen(s) ==> #[trigger] no_suf_match_at(s, pat, i)
    ensures strip_pre(s, pat, true) == s, strip_pre(s, pat, false) == s,
        strip_suf(s, pat, true) == s, strip_suf(s, pat, false) == s
{
    longest_pre_none(s, pat, slen(s));
    shortest_pre_none(s, pat, 0);
    earliest_suf_none(s, pat, 0);
    latest_suf_none(s, pat, slen(s));
}

proof fn longest_pre_none(s: Str, pat: Str, i: int)
    requires 0 <= i, i <= slen(s),
        forall|j: int| 0 <= j && j <= slen(s) ==> #[trigger] no_pre_match_at(s, pat, j)
    ensures longest_pre_match(s, pat, i) == 0int
    decreases i,
{
    if i <= 0 {
    } else {
        assert(no_pre_match_at(s, pat, i));
        longest_pre_none(s, pat, i - 1);
    }
}

proof fn shortest_pre_none(s: Str, pat: Str, i: int)
    requires 0 <= i,
        forall|j: int| 0 <= j && j <= slen(s) ==> #[trigger] no_pre_match_at(s, pat, j)
    ensures shortest_pre_match(s, pat, i) == 0int
    decreases slen(s) - i + 1,
{
    if i > slen(s) {
    } else {
        assert(no_pre_match_at(s, pat, i));
        shortest_pre_none(s, pat, i + 1);
    }
}

proof fn earliest_suf_none(s: Str, pat: Str, i: int)
    requires 0 <= i,
        forall|j: int| 0 <= j && j <= slen(s) ==> #[trigger] no_suf_match_at(s, pat, j)
    ensures earliest_suf_match(s, pat, i) == slen(s)
    decreases slen(s) - i + 1,
{
    if i > slen(s) {
    } else {
        assert(no_suf_match_at(s, pat, i));
        earliest_suf_none(s, pat, i + 1);
    }
}

proof fn latest_suf_none(s: Str, pat: Str, i: int)
    requires 0 <= i, i <= slen(s),
        forall|j: int| 0 <= j && j <= slen(s) ==> #[trigger] no_suf_match_at(s, pat, j)
    ensures latest_suf_match(s, pat, i) == slen(s)
    decreases i,
{
    if i <= 0 {
    } else {
        assert(no_suf_match_at(s, pat, slen(s) - i));
        latest_suf_none(s, pat, i - 1);
    }
}

pub open spec fn no_pre_match_at(s: Str, pat: Str, i: int) -> bool {
    !glob_match(pat, s.subrange(0, i))
}

pub open spec fn no_suf_match_at(s: Str, pat: Str, i: int) -> bool {
    !glob_match(pat, s.subrange(i, (s.len() as int)))
}

// argv slice: the positional arm computes `b` the same way as the
// scalar clamp (the -1 OUT sentinel only past the START), the emitted
// guard `b < 0 || b > total` is exactly !argv_in, and the value path
// copies exactly argv[b, min(b+len,total)).
proof fn argv_slice_sound(argv: Seq<Str>, off: int, len: int)
    requires 0 <= (argv.len() as int)
    ensures argv_clamp_b(off, argv.len() as int) == -1int
            || 0 <= argv_clamp_b(off, argv.len() as int),
        (argv_in(off, argv.len() as int)
            || off < -(argv.len() as int) || off > (argv.len() as int)),
        (!argv_in(off, argv.len() as int) || len < 0
            ==> argv_slice(argv, off, len) == Seq::empty()),
        (argv_in(off, argv.len() as int) && 0 <= len ==>
            argv_slice(argv, off, len)
                == argv.subrange(argv_clamp_b(off, argv.len() as int),
                    argv_clamp_e(argv_clamp_b(off, argv.len() as int), len, argv.len() as int)))
{
    if off < 0 {
        if off < -(argv.len() as int) {
        } else {
        }
    } else {
    }
}

// ── The OUTCOME obligations ───────────────────────────────────────────
// MEASURED against real bash 2026-11-01; each pins one guard.

pub open spec fn abcdef() -> Str {
    Seq::empty().push('a').push('b').push('c').push('d').push('e').push('f')
}

// "a b c d" -> 4 elements (the array arms count WORDS).
pub open spec fn w4() -> Str {
    Seq::empty().push('a').push(' ').push('b').push(' ').push('c').push(' ').push('d')
}

pub open spec fn args4() -> Seq<Str> {
    seq![seq!['$', '0'], seq!['p'], seq!['q'], seq!['r']]
}

// s1: a scalar offset past the start is OUT OF RANGE -> the EMPTY value
// (`s=abcdef; ${s: -7:3}` -> "", not "abc").
pub open spec fn s1() -> SOut { substr_out(abcdef(), 0int - 7int, 3int) }
proof fn substr_off_out_of_range_empty()
    ensures s1() == SOut::Val(Seq::empty())
{ assert(s1() == SOut::Val(Seq::empty())) by (compute); }

// s2: a scalar negative LENGTH counts back from the STRING end --
// e = n + len = 4, so `abcdef`[1..4) = "bcd" (NOT b+n+len = "bcde",
// which is what the pre-fix clamp produced).
pub open spec fn s2() -> SOut { substr_out(abcdef(), 1int, 0int - 2int) }
proof fn substr_neglen_from_string_end()
    ensures s2() == SOut::Val(Seq::empty().push('b').push('c').push('d'))
{ assert(s2() == SOut::Val(Seq::empty().push('b').push('c').push('d'))) by (compute); }

// s3: scalar -- the adjusted end lands BEFORE the offset -> Err.
pub open spec fn s3() -> SOut { substr_out(abcdef(), 3int, 0int - 5int) }
proof fn substr_neglen_err_before_offset()
    ensures s3() == SOut::Err
{ assert(s3() == SOut::Err) by (compute); }

// s4: ARRAY -- any in-range negative length is the expansion error
// (`a=(a b c d); ${a[@]:1:-2}` -> rc 1). The scalar arm has no such
// form; this asymmetry is bash's, not the renderer's.
pub open spec fn s4() -> SOut { arr_out(w4(), 1int, 0int - 2int) }
proof fn arr_neglen_is_error()
    ensures s4() == SOut::Err
{ assert(s4() == SOut::Err) by (compute); }

// s5: the GUARD ORDER -- the offset test runs first, so an out-of-range
// offset with a negative length is the empty VALUE (rc 0), not Err.
pub open spec fn s5() -> SOut { arr_out(w4(), 9int, 0int - 2int) }
proof fn arr_neglen_out_of_range_is_empty()
    ensures s5() == SOut::Val(Seq::empty())
{ assert(s5() == SOut::Val(Seq::empty())) by (compute); }

// s6: POSITIONAL -- an in-range negative length errors too (measured:
// for EVERY in-range b, including b == total).
pub open spec fn s6() -> VOut { argv_out(args4(), 2int, 0int - 1int, pos_slice_sep(true, seq![' '])) }
proof fn argv_neglen_is_error()
    ensures s6() == VOut::Err
{ assert(s6() == VOut::Err) by (compute); }

// s7: and an out-of-range positional offset is the empty VALUE first.
pub open spec fn s7() -> VOut { argv_out(args4(), 9int, 0int - 1int, pos_slice_sep(true, seq![' '])) }
proof fn argv_off_out_of_range_is_empty()
    ensures s7() == VOut::Val(Seq::empty())
{ assert(s7() == VOut::Val(Seq::empty())) by (compute); }

// m7: the RETIRED offset arm clamped to 0, so `${s: -7:3}` returned
// "abc" where bash returns "".
pub open spec fn clamp_b_old(off: int, n: int) -> int {
    let b = if off < 0 { n + off } else { off };
    if b < 0 {
        0int
    } else if b > n {
        n
    } else {
        b
    }
}

proof fn m7_retired_offset_clamp()
    ensures substr_out(abcdef(), 0int - 7int, 3int)
        != SOut::Val(abcdef().subrange(
            clamp_b_old(0int - 7int, 6int), clamp_b_old(0int - 7int, 6int) + 3int))
{ assert(
    substr_out(abcdef(), 0int - 7int, 3int)
        != SOut::Val(abcdef().subrange(
            clamp_b_old(0int - 7int, 6int), clamp_b_old(0int - 7int, 6int) + 3int))
) by (compute); }

// ── argv_join: the FACE's joiner (MEASURED 2026-11-03) ─────────────
// bash: `IFS=:; set -- p q r; v="$*"` → `p:q:r`; `v="$@"` → `p q r`
// under EVERY IFS; `IFS=""` → `v="$*"` is `pqr`. So `*` joins on
// IFS[0] and `@` on a fixed space — the CALL SITE decides which, and
// the model's sep is Option<char> (None = the NUL the emitted
// `if (i > 1 && sep)` guard reads as "write no separator").

// The contract unfold: a lone $0 (len <= 1) joins to the empty string
// — argv[0] is NEVER part of the join — and otherwise the value IS
// the separator-fold over $1...
proof fn argv_join_sound(argv: Seq<Str>, sep: Option<char>)
    ensures ((argv.len() as int) <= 1) ==> argv_join(argv, sep) == Seq::empty(),
        ((argv.len() as int) >= 2) ==>
            argv_join(argv, sep)
                == join_seq_sep(argv.subrange(1, argv.len() as int), sep)
{
    if (argv.len() as int) <= 1 {
    } else {
    }
}

// Empty IFS ⟺ the fold CONCATENATES with no separator — the `&& sep`
// guard's own semantics, proved by induction on the item count.
proof fn join_seq_sep_none(items: Seq<Str>)
    ensures join_seq_sep(items, None) == concat_seq(items)
    decreases items.len()
{
    if (items.len() as int) == 0 {
        assert(join_seq_sep(items, None) == Seq::empty());
        assert(concat_seq(items) == Seq::empty());
    } else if (items.len() as int) == 1 {
        assert(join_seq_sep(items, None) == items[0]);
        assert(concat_seq(items) == items[0]);
    } else {
        join_seq_sep_none(items.subrange(0, items.len() as int - 1));
        // both sides unfold to the same shape once join2(None,…) is
        // seen to be plain `a.add(b)` and the IH rewrites the prefix
        assert(join_seq_sep(items, None)
            == join2(None, join_seq_sep(items.subrange(0, items.len() as int - 1), None),
                items[items.len() as int - 1]));
        assert(concat_seq(items)
            == concat_seq(items.subrange(0, items.len() as int - 1)).add(items[items.len() as int - 1]));
    }
}

pub open spec fn ifs_colon() -> Str { seq![':'] }

// c27: `IFS=:; set -- p q r; printf … "$*"` → `p:q:r` — the `*`
// face takes IFS[0], so a colon IFS shows up IN the join.
pub open spec fn argv_star_colon() -> Str { argv_join(args4(), ifs0(ifs_colon())) }
proof fn argv_join_star_colon()
    ensures argv_star_colon() == seq!['p', ':', 'q', ':', 'r']
{ assert(argv_star_colon() == seq!['p', ':', 'q', ':', 'r']) by (compute); }

// c28: `IFS=""` → `pqr` — no separator at all (join2's None arm).
pub open spec fn argv_star_empty() -> Str { argv_join(args4(), ifs0(Seq::empty())) }
proof fn argv_join_star_empty()
    ensures argv_star_empty() == seq!['p', 'q', 'r']
{ assert(argv_star_empty() == seq!['p', 'q', 'r']) by (compute); }

// c30 + the default: the `@` face ALWAYS joins with a space — under
// colon IFS `v="$@"` still yields `p q r` (bash's `$@` never reads
// IFS), and the startup space IFS gives the same value.
pub open spec fn argv_at_colon_ifs() -> Str { argv_join(args4(), Some(' ')) }
proof fn argv_join_at_face_ignores_ifs()
    ensures argv_at_colon_ifs() == seq!['p', ' ', 'q', ' ', 'r'],
        argv_join(args4(), ifs0(seq![' '])) == argv_at_colon_ifs()
{ assert(argv_at_colon_ifs() == seq!['p', ' ', 'q', ' ', 'r']) by (compute);
    assert(argv_join(args4(), ifs0(seq![' '])) == argv_at_colon_ifs()) by (compute); }

// The two faces are observably DIFFERENT under a colon IFS — the
// whole point of the sep argument. Before it the helper hard-coded
// ' ', so the `*` face produced `p q r` where bash prints `p:q:r` and
// this inequality was unrepresentable.
proof fn argv_join_faces_differ()
    ensures argv_join(args4(), Some(' ')) != argv_join(args4(), ifs0(ifs_colon()))
{ assert(argv_join(args4(), Some(' ')) != argv_join(args4(), ifs0(ifs_colon()))) by (compute); }

// The len<=1 arm: an empty argv and a lone $0 both join to "".
proof fn argv_join_single_positional()
    ensures argv_join(Seq::empty(), Some(' ')) == Seq::empty(),
        argv_join(args4().subrange(0, 1), Some(' ')) == Seq::empty()
{ assert(argv_join(Seq::empty(), Some(' ')) == Seq::empty()) by (compute);
    assert(argv_join(args4().subrange(0, 1), Some(' ')) == Seq::empty()) by (compute); }

// The bridge pins: the IFS read is ONE char (the first), and empty
// IFS ⟺ None — the C-char-to-Option correspondence ifs0 models.
proof fn ifs0_bridge()
    ensures ifs0(Seq::empty()) == None,
        ifs0(ifs_colon()) == Some(':'),
        ifs0(seq![' ', '\t', '\n']) == Some(' ')
{ assert(ifs0(Seq::empty()) == None) by (compute);
    assert(ifs0(ifs_colon()) == Some(':')) by (compute);
    assert(ifs0(seq![' ', '\t', '\n']) == Some(' ')) by (compute); }

// split: word count = IFS-words; words are the maximal non-blank spans.
proof fn split_sound(s: Str)
    requires 0 <= (s.len() as int)
    ensures split_words(s).len() as int == nwords(s)
{
}

// split_count: empty ⟹ 0, else 1 + delimiter occurrences.
proof fn split_count_sound(s: Str, delim: char)
    ensures split_count(s, delim) == split_count(s, delim),
        (s.len() as int == 0int) ==> (split_count(s, delim) == 0int)
{
}

// ── Fidelity pins ─────────────────────────────────────────────────────

// The live shapes: `${s:1:3}` substr; `${s//a/b}` replace-all;
// `${s/a/b}` first-only; `${@:2:3}` argv slice; `$*` join;
// IFS split of `"a b  c"` into 3 words.
proof fn pin_call_sites()
    ensures substr(Seq::empty().push('a').push('b').push('c').push('d'), 1int, 2int)
            == Seq::empty().push('b').push('c'),
        replace_all_f(Seq::empty().push('a').push('a'), Seq::empty().push('a'), Seq::empty().push('b'), 5nat)
            == Seq::empty().push('b').push('b'),
        replace_first(Seq::empty().push('a').push('a'), Seq::empty().push('a'), Seq::empty().push('b'))
            == Seq::empty().push('b').push('a'),
        nwords(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c')) == 3int
{
    // "aa" replace-all at fuel 5: first hit at 0, splice, recurse on
    // "a" (hit at 0, splice, recurse on "" = no hit = identity). Each
    // find unfolds once the slen arithmetic is spelled out (probed),
    // and each splice needs the empty-prefix/empty-suffix bridges
    // (probed: the composition does not fire without them).
    pin_replace_aa_step2();
    pin_replace_aa_step1();
    pin_nwords_walk();
    // replace_first bridges (probed green in isolation) + the
    // rfirst-unfold that ties them to the ensures.
    assert(find(Seq::empty().push('a').push('a'), Seq::empty().push('a')) == Option::Some(0int)) by {
        assert(Seq::empty().push('a').push('a').subrange(0, 0 + slen(Seq::empty().push('a'))) == Seq::empty().push('a'));
        assert(find(Seq::empty().push('a').push('a'), Seq::empty().push('a'))
            == find_from(Seq::empty().push('a').push('a'), Seq::empty().push('a'), 0int));
    }
    assert(Seq::empty().push('a').push('a').subrange(0, 0int) == Seq::<char>::empty());
    assert(Seq::empty().push('a').push('a').subrange(0int + slen(Seq::empty().push('a')), slen(Seq::empty().push('a').push('a')))
        == Seq::empty().push('a'));
    assert(Seq::<char>::empty().add(Seq::empty().push('b')) == Seq::empty().push('b'));
    assert(Seq::empty().push('b').add(Seq::empty().push('a')) == Seq::empty().push('b').push('a'));
    assert(replace_first(Seq::empty().push('a').push('a'), Seq::empty().push('a'), Seq::empty().push('b'))
        == Seq::empty().push('a').push('a').subrange(0, 0int).add(Seq::empty().push('b')).add(
            Seq::empty().push('a').push('a').subrange(0int + slen(Seq::empty().push('a')), slen(Seq::empty().push('a').push('a')))));
}



// "a b  c" has 3 words: unfold the word-walk innermost-first (each
// level asserts its head char class + run + subrange identity, then
// the spans unfold -- the only form the prover accepts).
proof fn pin_nwords_walk()
    ensures nwords(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c')) == 3int
{
    assert(!is_blank(Seq::empty().push('c')[0]));
    assert(word_run_len(Seq::empty().push('c'), 0)
        == 1int + word_run_len(Seq::empty().push('c'), 1int));
    assert(1int >= slen(Seq::empty().push('c')));
    assert(word_run_len(Seq::empty().push('c'), 1int) == 0int);
    assert(word_run_len(Seq::empty().push('c'), 0) == 1int);
    // spans unfold needs the same bridges (probed): subrange identity
    // + the rest-spans value + the shift-of-empty.
    assert(Seq::empty().push('c').subrange(1, slen(Seq::empty().push('c'))) == Seq::<char>::empty());
    assert(word_spans(Seq::empty().push('c').subrange(1, slen(Seq::empty().push('c'))))
        == Seq::<(int, int)>::empty());
    assert(shift_spans_at(
        word_spans(Seq::empty().push('c').subrange(1, slen(Seq::empty().push('c')))), 1int)
        == Seq::<(int, int)>::empty());
    assert(word_spans(Seq::empty().push('c'))
        == Seq::empty().push((0int, word_run_len(Seq::empty().push('c'), 0))).add(
            shift_spans_at(word_spans(Seq::empty().push('c').subrange(1, slen(Seq::empty().push('c')))),
                word_run_len(Seq::empty().push('c'), 0))));
    assert(Seq::empty().push('c').subrange(1, slen(Seq::empty().push('c'))) == Seq::<char>::empty());
    assert(word_spans(Seq::empty().push('c'))
        == Seq::empty().push((0int, 1int)).add(shift_spans_at(Seq::<(int, int)>::empty(), 1int)));
    assert(shift_spans_at(Seq::<(int, int)>::empty(), 1int) == Seq::<(int, int)>::empty());
    assert(word_spans(Seq::empty().push('c')) == Seq::empty().push((0int, 1int)));
    assert(is_blank(' '));
    assert(Seq::empty().push(' ').push('c').subrange(1, slen(Seq::empty().push(' ').push('c')))
        == Seq::empty().push('c'));
    assert(word_spans(Seq::empty().push(' ').push('c'))
        == shift_spans(word_spans(Seq::empty().push('c'))));
    assert(shift_spans(word_spans(Seq::empty().push('c'))) == Seq::empty().push((1int, 2int)));
    assert(Seq::empty().push(' ').push(' ').push('c').subrange(1, slen(Seq::empty().push(' ').push(' ').push('c')))
        == Seq::empty().push(' ').push('c'));
    assert(word_spans(Seq::empty().push(' ').push(' ').push('c'))
        == shift_spans(word_spans(Seq::empty().push(' ').push('c'))));
    assert(word_spans(Seq::empty().push(' ').push(' ').push('c')) == Seq::empty().push((2int, 3int)));
    assert(!is_blank(Seq::empty().push('b').push(' ').push(' ').push('c')[0]));
    assert(is_blank(Seq::empty().push('b').push(' ').push(' ').push('c')[1]));
    assert(word_run_len(Seq::empty().push('b').push(' ').push(' ').push('c'), 0)
        == 1int + word_run_len(Seq::empty().push('b').push(' ').push(' ').push('c'), 1int));
    assert(word_run_len(Seq::empty().push('b').push(' ').push(' ').push('c'), 1int) == 0int);
    assert(word_run_len(Seq::empty().push('b').push(' ').push(' ').push('c'), 0) == 1int);
    assert(Seq::empty().push('b').push(' ').push(' ').push('c').subrange(1, slen(Seq::empty().push('b').push(' ').push(' ').push('c')))
        == Seq::empty().push(' ').push(' ').push('c'));
    assert(word_spans(Seq::empty().push('b').push(' ').push(' ').push('c'))
        == Seq::empty().push((0int, 1int)).add(shift_spans_at(word_spans(Seq::empty().push(' ').push(' ').push('c')), 1int)));
    assert(shift_spans_at(word_spans(Seq::empty().push(' ').push(' ').push('c')), 1int) == Seq::empty().push((3int, 4int)));
    assert(word_spans(Seq::empty().push('b').push(' ').push(' ').push('c'))
        == Seq::empty().push((0int, 1int)).push((3int, 4int)));
    assert(Seq::empty().push(' ').push('b').push(' ').push(' ').push('c').subrange(
            1, slen(Seq::empty().push(' ').push('b').push(' ').push(' ').push('c')))
        == Seq::empty().push('b').push(' ').push(' ').push('c'));
    assert(word_spans(Seq::empty().push(' ').push('b').push(' ').push(' ').push('c'))
        == shift_spans(word_spans(Seq::empty().push('b').push(' ').push(' ').push('c'))));
    assert(!is_blank(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c')[0]));
    assert(is_blank(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c')[1]));
    assert(word_run_len(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c'), 0)
        == 1int + word_run_len(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c'), 1int));
    assert(word_run_len(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c'), 1int) == 0int);
    assert(word_run_len(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c'), 0) == 1int);
    assert(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c').subrange(
            1, slen(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c')))
        == Seq::empty().push(' ').push('b').push(' ').push(' ').push('c'));
    assert(word_spans(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c'))
        == Seq::empty().push((0int, 1int)).add(shift_spans_at(
            word_spans(Seq::empty().push(' ').push('b').push(' ').push(' ').push('c')), 1int)));
    assert(nwords(Seq::empty().push('a').push(' ').push('b').push(' ').push(' ').push('c')) == 3int);
}

// "a" replace-all at fuel 4: one hit, splice, recurse on "".
// "a" replace-all at fuel 4: one hit, splice, recurse on "".
proof fn pin_replace_aa_step2()
    ensures replace_all_f(Seq::empty().push('a'), Seq::empty().push('a'), Seq::empty().push('b'), 4nat)
            == Seq::empty().push('b')
{
    assert(find(Seq::empty().push('a'), Seq::empty().push('a')) == Option::Some(0int)) by {
        assert(Seq::empty().push('a').subrange(0, 0 + slen(Seq::empty().push('a'))) == Seq::empty().push('a'));
        assert(find(Seq::empty().push('a'), Seq::empty().push('a'))
            == find_from(Seq::empty().push('a'), Seq::empty().push('a'), 0int));
    }
    assert(replace_all_f(Seq::empty().push('a'), Seq::empty().push('a'), Seq::empty().push('b'), 4nat)
        == Seq::empty().push('a').subrange(0, 0int).add(Seq::empty().push('b')).add(
            replace_all_f(Seq::empty().push('a').subrange(0int + slen(Seq::empty().push('a')), slen(Seq::empty().push('a'))),
                Seq::empty().push('a'), Seq::empty().push('b'), 3nat)));
    assert(Seq::empty().push('a').subrange(0, 0int) == Seq::<char>::empty());
    assert(Seq::empty().push('a').subrange(0int + slen(Seq::empty().push('a')), slen(Seq::empty().push('a')))
        == Seq::<char>::empty());
    assert(Seq::<char>::empty().add(Seq::empty().push('b')) == Seq::empty().push('b'));
    assert(replace_all_f(Seq::<char>::empty(), Seq::empty().push('a'), Seq::empty().push('b'), 3nat)
        == Seq::<char>::empty());
    assert(Seq::empty().push('b').add(Seq::<char>::empty()) == Seq::empty().push('b'));
}

// "aa" replace-all at fuel 5: first hit at 0, splice, recurse on "a".
proof fn pin_replace_aa_step1()
    ensures replace_all_f(Seq::empty().push('a').push('a'), Seq::empty().push('a'), Seq::empty().push('b'), 5nat)
            == Seq::empty().push('b').push('b')
{
    assert(find(Seq::empty().push('a').push('a'), Seq::empty().push('a')) == Option::Some(0int)) by {
        assert(Seq::empty().push('a').push('a').subrange(0, 0 + slen(Seq::empty().push('a'))) == Seq::empty().push('a'));
        assert(find(Seq::empty().push('a').push('a'), Seq::empty().push('a'))
            == find_from(Seq::empty().push('a').push('a'), Seq::empty().push('a'), 0int));
    }
    assert(replace_all_f(Seq::empty().push('a').push('a'), Seq::empty().push('a'), Seq::empty().push('b'), 5nat)
        == Seq::empty().push('a').push('a').subrange(0, 0int).add(Seq::empty().push('b')).add(
            replace_all_f(Seq::empty().push('a').push('a').subrange(0int + slen(Seq::empty().push('a')), slen(Seq::empty().push('a').push('a'))),
                Seq::empty().push('a'), Seq::empty().push('b'), 4nat)));
    assert(Seq::empty().push('a').push('a').subrange(0, 0int) == Seq::<char>::empty());
    assert(Seq::empty().push('a').push('a').subrange(0int + slen(Seq::empty().push('a')), slen(Seq::empty().push('a').push('a')))
        == Seq::empty().push('a'));
    assert(Seq::<char>::empty().add(Seq::empty().push('b')) == Seq::empty().push('b'));
    pin_replace_aa_step2();
}

// ── the POSITIONAL SLICE face (MEASURED 2026-11-04) ──────────────────
// `set -- p q r s` is argv = [$0, p, q, r, s] in the model, so bash's
// `${*:2:2}` = `q r` is argv indices 2..4 — the same b/e the emitted
// helper computes. Every value below was measured with real bash
// (`harness/ifs-probe.sh` cases 32-38).
pub open spec fn pos5() -> Seq<Str> {
    seq![seq!['b', 'a', 's', 'h'], seq!['p'], seq!['q'], seq!['r'], seq!['s']]
}

// the no-length sentinel the renderer passes (`(long long)1LL<<60`)
pub open spec fn big_len() -> int { 1152921504606846976int }

// s8: `${*:2:2}` under IFS=: -> `q:r`
pub open spec fn s8() -> VOut { argv_out(pos5(), 2int, 2int, pos_slice_sep(true, ifs_colon())) }
proof fn argv_slice_star_colon()
    ensures s8() == VOut::Val(seq!['q', ':', 'r'])
{ assert(s8() == VOut::Val(seq!['q', ':', 'r'])) by (compute); }

// s9: `${@:2:2}` under IFS=: -> `q:r`. The `@` SLICE reads IFS even
// though the whole `"$@"` does not (`argv_join_at_face_ignores_ifs`:
// `v="$@"` stays `p q r`) — this pair is why the slice needed its own
// predicate instead of reusing the whole-form joiner.
pub open spec fn s9() -> VOut { argv_out(pos5(), 2int, 2int, pos_slice_sep(false, ifs_colon())) }
proof fn argv_slice_at_colon()
    ensures s9() == VOut::Val(seq!['q', ':', 'r'])
{ assert(s9() == VOut::Val(seq!['q', ':', 'r'])) by (compute); }

// s10: `${*:2:2}` under IFS="" -> `qr` — ifs0 is None, the `&& sep`
// guard writes nothing.
proof fn argv_slice_star_empty_ifs()
    ensures pos_slice(pos5(), 2int, 2int, true, Seq::empty()) == seq!['q', 'r']
{ assert(pos_slice(pos5(), 2int, 2int, true, Seq::empty()) == seq!['q', 'r']) by (compute); }

// s11: `${@:2:2}` under IFS="" -> `q r` — the SPACE FALLBACK, the one
// place the two slice faces disagree.
proof fn argv_slice_at_empty_ifs()
    ensures pos_slice(pos5(), 2int, 2int, false, Seq::empty()) == seq!['q', ' ', 'r']
{ assert(pos_slice(pos5(), 2int, 2int, false, Seq::empty()) == seq!['q', ' ', 'r']) by (compute); }

// s12: behaviour-neutral on the startup IFS — both faces `q r`, the
// value every corpus example already produced.
proof fn argv_slice_default_ifs_both_faces()
    ensures pos_slice(pos5(), 2int, 2int, true, seq![' ']) == seq!['q', ' ', 'r'],
        pos_slice(pos5(), 2int, 2int, false, seq![' ']) == seq!['q', ' ', 'r']
{ assert(pos_slice(pos5(), 2int, 2int, true, seq![' ']) == seq!['q', ' ', 'r']) by (compute);
  assert(pos_slice(pos5(), 2int, 2int, false, seq![' ']) == seq!['q', ' ', 'r']) by (compute); }

// s13: the faces differ ONLY on an empty IFS.
proof fn argv_slice_faces_differ_only_when_ifs_empty()
    ensures pos_slice(pos5(), 2int, 2int, true, Seq::empty())
            != pos_slice(pos5(), 2int, 2int, false, Seq::empty()),
        pos_slice(pos5(), 2int, 2int, true, ifs_colon())
            == pos_slice(pos5(), 2int, 2int, false, ifs_colon())
{ assert(pos_slice(pos5(), 2int, 2int, true, Seq::empty())
    != pos_slice(pos5(), 2int, 2int, false, Seq::empty())) by (compute);
  assert(pos_slice(pos5(), 2int, 2int, true, ifs_colon())
    == pos_slice(pos5(), 2int, 2int, false, ifs_colon())) by (compute); }

// s14: the GUARD ORDER is separator-independent — an out-of-range
// offset returns the empty STRING (the early return writes no
// separator at all), on both faces.
proof fn argv_slice_out_of_range_empty_any_sep()
    ensures argv_out(pos5(), 9int, 2int, pos_slice_sep(true, ifs_colon()))
            == VOut::Val(Seq::empty()),
        argv_out(pos5(), 9int, 2int, pos_slice_sep(false, Seq::empty()))
            == VOut::Val(Seq::empty())
{ assert(argv_out(pos5(), 9int, 2int, pos_slice_sep(true, ifs_colon()))
    == VOut::Val(Seq::empty())) by (compute);
  assert(argv_out(pos5(), 9int, 2int, pos_slice_sep(false, Seq::empty()))
    == VOut::Val(Seq::empty())) by (compute); }

// s15: the OPEN form takes the same separator — `${*:2}` -> `q:r:s`.
proof fn argv_slice_star_open_len()
    ensures pos_slice(pos5(), 2int, big_len(), true, ifs_colon())
        == seq!['q', ':', 'r', ':', 's']
{ assert(pos_slice(pos5(), 2int, big_len(), true, ifs_colon())
    == seq!['q', ':', 'r', ':', 's']) by (compute); }

// f12 (NEW 2026-11-04, the `$IFS` read): the startup buffer is EXACTLY
// three chars — space, tab, newline — and every one of them is an IFS
// whitespace char. This is the value the splitter consumes AND the value
// the variable face now publishes (`_sh_env("IFS")` answers from the
// buffer; bash ignores an inherited IFS at startup, so getenv was never
// the right source and the two faces used to disagree).
proof fn f12_startup_ifs_is_three_ws_chars()
    ensures (ifs_startup().len() as int) == 3int,
        ifs_ws(' ', ifs_startup()),
        ifs_ws('\t', ifs_startup()),
        ifs_ws('\n', ifs_startup())
{ assert((ifs_startup().len() as int) == 3int) by (compute);
  assert(ifs_ws(' ', ifs_startup())) by (compute);
  assert(ifs_ws('\t', ifs_startup())) by (compute);
  assert(ifs_ws('\n', ifs_startup())) by (compute); }

// f13: and it holds NOTHING else — a char outside the three is not a
// delimiter of the startup IFS, which is what makes `a::b` one field
// under it (f8's counterpart: empty IFS = no split, startup IFS = blank
// split only).
proof fn f13_startup_ifs_has_no_other_delims()
    ensures !ifs_delim(':', ifs_startup()),
        !ifs_delim(',', ifs_startup()),
        !ifs_delim('a', ifs_startup())
{ assert(!ifs_delim(':', ifs_startup())) by (compute);
  assert(!ifs_delim(',', ifs_startup())) by (compute);
  assert(!ifs_delim('a', ifs_startup())) by (compute); }

// f14 (NEW 2026-11-04): the EMPTY value splits to ZERO fields. bash
// (`e=""; for w in $e`) iterates zero times — word splitting of nothing
// yields nothing, unlike a quoted `""`, which is ONE empty field.
proof fn f14_empty_value_is_zero_fields()
    ensures ifs_fields(Seq::empty(), ifs_startup()) == Seq::<Str>::empty()
{ assert(ifs_fields(Seq::empty(), ifs_startup()) == Seq::<Str>::empty()) by (compute); }

// f15: …and the rendering the C `for` lowering produces for an EMPTY
// UNQUOTED word — a ONE-element list holding the empty string — is
// observably different from that field list. This is the divergence
// `harness/ifs-probe.sh` case 52 pins as an xfail; the fix needs the shIR
// to carry the word's ORIGIN (a `StrStyle::Unquoted` on the folded
// `Str("")`), which is a core change and is filed as
// `core-requests/c-20261104-unquoted-empty-word.md`. `ifs_fields` here is
// the contract the renderer must meet, not an implementation of it.
proof fn f15_empty_unquoted_word_is_not_a_field()
    ensures seq![Seq::<char>::empty()] != ifs_fields(Seq::empty(), ifs_startup()),
        (seq![Seq::<char>::empty()].len() as int) == 1int
{ assert(seq![Seq::<char>::empty()] != ifs_fields(Seq::empty(), ifs_startup())) by (compute);
  assert((seq![Seq::<char>::empty()].len() as int) == 1int) by (compute); }

// ── the FALLBACK slice (`_sh_arr_slice`), sep-parameterised ──────────
// An array whose storage the emitter cannot walk takes the join-then-
// split arm: `array_join_all_face` produces a string, and this helper
// slices it back into words. Its separator must be the one the JOIN
// used (IFS[0] for `${a[*]}`, a literal space for `${a[@]}`); it used to
// hard-code a space, so `a=(x y z); IFS=:; ${a[*]:1:2}` saw ONE word
// `x:y:z` rather than three.
//
// NOTE the rule differs from `_sh_split`'s, and correctly: the join only
// ever inserts `sep` BETWEEN words, so `x:` is two words (`x`, ``) while
// bash's field splitting of the user text `x:` is one. This split is the
// INVERSE OF ITS OWN JOIN, not bash's word splitting — `sep_fields` keeps
// every empty field, `ifs_fields` does not.
pub open spec fn sep_at(s: Str, i: int, sep: char) -> bool {
    if i < 0 {
        false
    } else if i >= (s.len() as int) {
        false
    } else {
        s[i] == sep
    }
}

pub open spec fn sep_end(s: Str, sep: char, i: int) -> int
    decreases (s.len() as int) - i,
{
    if i < 0 {
        0
    } else if i >= (s.len() as int) {
        i
    } else if sep_at(s, i, sep) {
        i
    } else {
        sep_end(s, sep, i + 1)
    }
}

// Split on ONE char, keeping empty fields; fuelled (fields <= len + 1).
pub open spec fn sep_fields_fuel(s: Str, sep: char, i: int, k: int) -> Seq<Str>
    decreases k,
{
    if k <= 0 || i > (s.len() as int) {
        Seq::empty()
    } else {
        let e = sep_end(s, sep, i);
        if e >= (s.len() as int) {
            Seq::empty().push(s.subrange(i, e))
        } else {
            Seq::empty().push(s.subrange(i, e))
                .add(sep_fields_fuel(s, sep, e + 1, k - 1))
        }
    }
}

pub open spec fn sep_fields(s: Str, sep: char) -> Seq<Str> {
    if (s.len() as int) == 0 {
        Seq::empty()
    } else {
        sep_fields_fuel(s, sep, 0, (s.len() as int) + 1)
    }
}

// The emitted `_sh_arr_slice`: the same b/e arithmetic as the store arms
// (offset out of range -> EMPTY value; an in-range `len < 0` is the
// expansion error, modelled by the outcome layer above), then the
// selection re-joined with the separator it was split on.
pub open spec fn arr_slice_sep(s: Str, off: int, len: int, sep: char) -> Str {
    let ws = sep_fields(s, sep);
    let n = ws.len() as int;
    if !in_b_arr(off, n) || len < 0 {
        Seq::empty()
    } else {
        let b = clamp_b(off, n);
        let e = clamp_e(b, len, n);
        if e < b || e > n {
            Seq::empty()
        } else {
            join_seq_sep(ws.subrange(b, e), Some(sep))
        }
    }
}

pub open spec fn sep3() -> Str { seq!['x', ':', 'y', ':', 'z'] }
pub open spec fn sep_empty_lead() -> Str { seq![':', 'x'] }

// g1: `a=(x y z)` joined on ':' is `x:y:z` — THREE words, so `${a[*]:1:2}`
// is `y:z`. The space-only splitter saw one word and returned nothing.
proof fn g1_colon_join_has_three_words()
    ensures (sep_fields(sep3(), ':').len() as int) == 3int,
        arr_slice_sep(sep3(), 1int, 2int, ':') == seq!['y', ':', 'z'],
        arr_slice_sep(sep3(), 1int, 1int, ':') == seq!['y']
{ assert((sep_fields(sep3(), ':').len() as int) == 3int) by (compute);
  assert(arr_slice_sep(sep3(), 1int, 2int, ':') == seq!['y', ':', 'z']) by (compute);
  assert(arr_slice_sep(sep3(), 1int, 1int, ':') == seq!['y']) by (compute); }

// g2: the whole range is the IDENTITY — the fallback's contract, and why
// the separator must match the join's (a space split of `x:y:z` is one
// word, so it cannot round-trip).
proof fn g2_whole_range_round_trips()
    ensures arr_slice_sep(sep3(), 0int, 3int, ':') == sep3(),
        (sep_fields(sep3(), ' ').len() as int) == 1int
{ assert(arr_slice_sep(sep3(), 0int, 3int, ':') == sep3()) by (compute);
  assert((sep_fields(sep3(), ' ').len() as int) == 1int) by (compute); }

// g3: an EMPTY element survives: `a=("" x)` joined on ':' is `:x`, which
// is two words — the old space scan skipped the leading run and lost it.
proof fn g3_empty_field_kept()
    ensures sep_fields(sep_empty_lead(), ':')
        == seq![Seq::<char>::empty(), seq!['x']],
        arr_slice_sep(sep_empty_lead(), 0int, 2int, ':') == sep_empty_lead()
{ assert(sep_fields(sep_empty_lead(), ':')
    == seq![Seq::<char>::empty(), seq!['x']]) by (compute);
  assert(arr_slice_sep(sep_empty_lead(), 0int, 2int, ':') == sep_empty_lead()) by (compute); }

// g4: the DEFAULT IFS is unchanged — a space join splits on spaces, three
// words, `${a[*]:1:2}` = `y z` (the value the corpus already produced).
pub open spec fn sep_space3() -> Str { seq!['x', ' ', 'y', ' ', 'z'] }
proof fn g4_space_join_unchanged()
    ensures arr_slice_sep(sep_space3(), 1int, 2int, ' ') == seq!['y', ' ', 'z'],
        (sep_fields(sep_space3(), ' ').len() as int) == 3int
{ assert(arr_slice_sep(sep_space3(), 1int, 2int, ' ') == seq!['y', ' ', 'z']) by (compute);
  assert((sep_fields(sep_space3(), ' ').len() as int) == 3int) by (compute); }

// g5: an EMPTY joined string has NO fields (an empty array), so any slice
// is the empty value rather than one empty word.
proof fn g5_empty_join_no_fields()
    ensures sep_fields(Seq::empty(), ':') == Seq::<Str>::empty(),
        arr_slice_sep(Seq::empty(), 0int, 1int, ':') == Seq::empty()
{ assert(sep_fields(Seq::empty(), ':') == Seq::<Str>::empty()) by (compute);
  assert(arr_slice_sep(Seq::empty(), 0int, 1int, ':') == Seq::empty()) by (compute); }

// m8: the RETIRED rendering — a hard-coded space. It agrees on the
// startup IFS (s12) and disagrees under a colon one, so only a
// non-default-IFS example could ever catch it.
proof fn argv_slice_retired_space_is_wrong()
    ensures pos_slice(pos5(), 2int, 2int, true, ifs_colon())
        != pos_slice(pos5(), 2int, 2int, true, seq![' '])
{ assert(pos_slice(pos5(), 2int, 2int, true, ifs_colon())
    != pos_slice(pos5(), 2int, 2int, true, seq![' '])) by (compute); }

// ── FIELD SPLITTING (`_sh_split`), IFS-DRIVEN (MEASURED 2026-11-04) ──
// The `word_spans` denotation above is BLANK-only (space/tab/newline,
// no IFS read); it stays because `_sh_arr_slice`'s helper still
// hard-codes ' '. The splitter itself now reads IFS, and its rules are
// not the blank ones:
//   * a run of IFS WHITESPACE is ONE delimiter and is stripped at both
//     ends (`  a  b  ` -> a, b);
//   * each IFS NON-whitespace char delimits exactly one field, so
//     `a::b` has an EMPTY middle field and `:a:` an EMPTY first;
//   * an EMPTY IFS disables splitting (`a::b` stays one field);
//   * a char is an IFS whitespace only if it is ' ', '\t' or '\n' AND
//     IFS lists it — `IFS=' '` makes a TAB ordinary, `IFS=':'` makes the
//     SPACE ordinary (`a::b: c` under `IFS=':'` is `a`, ``, `b`, ` c`).

// membership in the IFS string
pub open spec fn has_ch(s: Str, c: char) -> bool
    decreases s.len(),
{
    if (s.len() as int) == 0 {
        false
    } else {
        s[0] == c || has_ch(s.subrange(1, s.len() as int), c)
    }
}

pub open spec fn ifs_ws(c: char, ifs: Str) -> bool {
    is_blank(c) && has_ch(ifs, c)
}

pub open spec fn ifs_nw(c: char, ifs: Str) -> bool {
    !is_blank(c) && has_ch(ifs, c)
}

pub open spec fn ifs_delim(c: char, ifs: Str) -> bool {
    ifs_ws(c, ifs) || ifs_nw(c, ifs)
}

// Index-safe probes. `s[i]` may only appear below an `i < len` guard: an
// eager out-of-range index makes the term STUCK and the recursive spec
// functions below then never unfold, so no lemma about them can be proved
// (Verus, --triggers-mode silent — measured by bisection on a 1-file
// reduction). Everything that inspects `s[i]` goes through these.
pub open spec fn ws_at(s: Str, ifs: Str, i: int) -> bool {
    if i < 0 {
        false
    } else if i >= (s.len() as int) {
        false
    } else {
        ifs_ws(s[i], ifs)
    }
}

pub open spec fn nw_at(s: Str, ifs: Str, i: int) -> bool {
    if i < 0 {
        false
    } else if i >= (s.len() as int) {
        false
    } else {
        ifs_nw(s[i], ifs)
    }
}

pub open spec fn delim_at(s: Str, ifs: Str, i: int) -> bool {
    if i < 0 {
        false
    } else if i >= (s.len() as int) {
        false
    } else {
        ifs_delim(s[i], ifs)
    }
}

// first index >= i that is not an IFS whitespace char (stops at len).
// The `i < 0` arm keeps the spec TOTAL and index-SAFE: `s[i]` may only
// appear below an `i < len` guard, or Verus treats the term as stuck and
// no obligation about this function can be discharged.
pub open spec fn skip_ws(s: Str, ifs: Str, i: int) -> int
    decreases (s.len() as int) - i,
{
    if i < 0 {
        0
    } else if i >= (s.len() as int) {
        i
    } else if ws_at(s, ifs, i) {
        skip_ws(s, ifs, i + 1)
    } else {
        i
    }
}

// first index >= i that IS a delimiter (stops at len): a field's end
pub open spec fn field_end(s: Str, ifs: Str, i: int) -> int
    decreases (s.len() as int) - i,
{
    if i < 0 {
        0
    } else if i >= (s.len() as int) {
        i
    } else if delim_at(s, ifs, i) {
        i
    } else {
        field_end(s, ifs, i + 1)
    }
}

// The index after ONE delimiter unit starting at i (s[i] is a delimiter):
// `ws* nw? ws*` when it starts with whitespace, `nw ws*` otherwise. The
// optional single non-whitespace char is bash's, and it is why `a , b`
// under `IFS=' ,'` is TWO fields while `a::b` under `IFS=':'` is three.
pub open spec fn unit_after(s: Str, ifs: Str, i: int) -> int {
    if i < 0 {
        0
    } else if i >= (s.len() as int) {
        i
    } else if ws_at(s, ifs, i) {
        let r = skip_ws(s, ifs, i + 1);
        let r2 = if nw_at(s, ifs, r) { r + 1 } else { r };
        skip_ws(s, ifs, r2)
    } else {
        skip_ws(s, ifs, i + 1)
    }
}

// The emitted loop, fuelled: each iteration consumes at least one char, so
// `len + 1` fuel always suffices (`fields_saturated` below).
pub open spec fn fields_fuel(s: Str, ifs: Str, i: int, k: int) -> Seq<Str>
    decreases k,
{
    if k <= 0 || i >= (s.len() as int) {
        Seq::empty()
    } else {
        let e = field_end(s, ifs, i);
        let nxt = unit_after(s, ifs, e);
        Seq::empty().push(s.subrange(i, e)).add(fields_fuel(s, ifs, nxt, k - 1))
    }
}

// The helper's contract: leading whitespace stripped, an EMPTY IFS doing no
// splitting at all (the emitted early return).
pub open spec fn ifs_fields(s: Str, ifs: Str) -> Seq<Str> {
    if (ifs.len() as int) == 0 {
        if (s.len() as int) == 0 {
            Seq::empty()
        } else {
            Seq::empty().push(s)
        }
    } else {
        fields_fuel(s, ifs, skip_ws(s, ifs, 0), (s.len() as int) + 1)
    }
}

// ── progress lemmas (the loop's own termination argument) ────────────

proof fn skip_ws_ge(s: Str, ifs: Str, i: int)
    requires 0 <= i
    ensures skip_ws(s, ifs, i) >= i
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) {
    } else if ws_at(s, ifs, i) {
        skip_ws_ge(s, ifs, i + 1);
    }
}

proof fn skip_ws_le(s: Str, ifs: Str, i: int)
    requires 0 <= i, i <= (s.len() as int)
    ensures skip_ws(s, ifs, i) <= (s.len() as int)
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) {
    } else if ws_at(s, ifs, i) {
        skip_ws_le(s, ifs, i + 1);
    }
}

proof fn field_end_ge(s: Str, ifs: Str, i: int)
    requires 0 <= i
    ensures field_end(s, ifs, i) >= i
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) {
    } else if delim_at(s, ifs, i) {
    } else {
        field_end_ge(s, ifs, i + 1);
    }
}

proof fn unit_after_gt(s: Str, ifs: Str, i: int)
    requires 0 <= i, i < (s.len() as int)
    ensures unit_after(s, ifs, i) > i
{
    if ws_at(s, ifs, i) {
        skip_ws_ge(s, ifs, i + 1);
        let r = skip_ws(s, ifs, i + 1);
        if nw_at(s, ifs, r) {
            skip_ws_ge(s, ifs, r + 1);
        } else {
            skip_ws_ge(s, ifs, r);
        }
    } else {
        skip_ws_ge(s, ifs, i + 1);
    }
}

proof fn field_end_le(s: Str, ifs: Str, i: int)
    requires 0 <= i, i <= (s.len() as int)
    ensures field_end(s, ifs, i) <= (s.len() as int)
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) {
    } else if delim_at(s, ifs, i) {
    } else {
        field_end_le(s, ifs, i + 1);
    }
}

proof fn unit_after_le(s: Str, ifs: Str, i: int)
    requires 0 <= i, i <= (s.len() as int)
    ensures unit_after(s, ifs, i) <= (s.len() as int)
{
    if i >= (s.len() as int) {
    } else if ws_at(s, ifs, i) {
        let r = skip_ws(s, ifs, i + 1);
        skip_ws_le(s, ifs, i + 1);
        skip_ws_ge(s, ifs, i + 1);
        if r < (s.len() as int) && nw_at(s, ifs, r) {
            skip_ws_le(s, ifs, r + 1);
        } else {
            skip_ws_le(s, ifs, r);
        }
    } else {
        skip_ws_le(s, ifs, i + 1);
    }
}

// The loop's progress: the next iteration starts at least one char later,
// so a fuel bound of one-per-byte is honest.
proof fn nxt_gt(s: Str, ifs: Str, i: int)
    requires 0 <= i, i < (s.len() as int)
    ensures unit_after(s, ifs, field_end(s, ifs, i)) > i
{
    field_end_ge(s, ifs, i);
    if field_end(s, ifs, i) >= (s.len() as int) {
        // unit_after returns its argument unchanged at/after len
    } else {
        unit_after_gt(s, ifs, field_end(s, ifs, i));
    }
}

proof fn nxt_le(s: Str, ifs: Str, i: int)
    requires 0 <= i, i <= (s.len() as int)
    ensures unit_after(s, ifs, field_end(s, ifs, i)) <= (s.len() as int)
{
    field_end_ge(s, ifs, i);
    field_end_le(s, ifs, i);
    unit_after_le(s, ifs, field_end(s, ifs, i));
}

// Fuel saturation: any k >= len - i gives the same field list as len - i —
// the emitted loop always ends after at most one iteration per byte, so the
// fuelled denotation is EXACT, not an approximation of it.
proof fn fields_saturated(s: Str, ifs: Str, i: int, k: int)
    requires 0 <= i, i <= (s.len() as int), k >= (s.len() as int) - i
    ensures fields_fuel(s, ifs, i, k)
        == fields_fuel(s, ifs, i, (s.len() as int) - i)
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) {
        // both arms hit `i >= len` and return empty
    } else {
        let e = field_end(s, ifs, i);
        let nxt = unit_after(s, ifs, e);
        nxt_gt(s, ifs, i);
        nxt_le(s, ifs, i);
        field_end_ge(s, ifs, i);
        // the unit consumed a char, so both `k - 1` and `len - i - 1` are
        // still at least `len - nxt` — the IH applies to both tails
        fields_saturated(s, ifs, nxt, k - 1);
        fields_saturated(s, ifs, nxt, (s.len() as int) - i - 1);
    }
}

// ── the measured pins (harness/ifs-probe.sh 41-50) ───────────────────
pub open spec fn ifs_sp_colon() -> Str { seq![' ', ':'] }
pub open spec fn ifs_sp_comma() -> Str { seq![' ', ','] }
pub open spec fn ifs_startup() -> Str { seq![' ', '\t', '\n'] }

pub open spec fn s_colon_a_colon() -> Str { seq![':', 'a', ':'] }
pub open spec fn s_a_cc_b() -> Str { seq!['a', ':', ':', 'b'] }
pub open spec fn s_colon() -> Str { seq![':'] }
pub open spec fn s_a_colon_b_colon() -> Str { seq!['a', ':', 'b', ':'] }
pub open spec fn s_padded() -> Str { seq![' ', ' ', 'a', ' ', ' ', 'b', ' ', ' '] }
pub open spec fn s_a_comma_b() -> Str { seq!['a', ' ', ',', ' ', 'b'] }
pub open spec fn s_pad_a_cc_b_pad() -> Str { seq![' ', 'a', ':', ':', ' ', 'b', ' '] }
pub open spec fn s_a_tab_b() -> Str { seq!['a', '\t', 'b'] }
pub open spec fn s_abcd_colon() -> Str {
    seq!['a', ':', ':', 'b', ':', ' ', 'c']
}

// f1: `v=':a:'; IFS=':'` -> an EMPTY first field, the trailing delimiter
// dropped: <><a>.
proof fn f1_lead_delim()
    ensures ifs_fields(s_colon_a_colon(), ifs_colon()) == seq![Seq::<char>::empty(), seq!['a']]
{ assert(ifs_fields(s_colon_a_colon(), ifs_colon())
    == seq![Seq::<char>::empty(), seq!['a']]) by (compute); }

// f2: `a::b` -> a, "", b (three fields).
proof fn f2_adjacent_delims()
    ensures ifs_fields(s_a_cc_b(), ifs_colon()) == seq![seq!['a'], Seq::<char>::empty(), seq!['b']]
{ assert(ifs_fields(s_a_cc_b(), ifs_colon())
    == seq![seq!['a'], Seq::<char>::empty(), seq!['b']]) by (compute); }

// f3: a string that is ONLY a delimiter is ONE empty field.
proof fn f3_only_delim()
    ensures ifs_fields(s_colon(), ifs_colon()) == seq![Seq::<char>::empty()]
{ assert(ifs_fields(s_colon(), ifs_colon()) == seq![Seq::<char>::empty()]) by (compute); }

// f4: `a:b:` -> a, b — a TRAILING delimiter does not add an empty field.
proof fn f4_trail_delim()
    ensures ifs_fields(s_a_colon_b_colon(), ifs_colon()) == seq![seq!['a'], seq!['b']]
{ assert(ifs_fields(s_a_colon_b_colon(), ifs_colon())
    == seq![seq!['a'], seq!['b']]) by (compute); }

// f5: `'  a  b  '` under the startup IFS -> a, b (runs collapse).
proof fn f5_ws_run()
    ensures ifs_fields(s_padded(), ifs_startup()) == seq![seq!['a'], seq!['b']]
{ assert(ifs_fields(s_padded(), ifs_startup()) == seq![seq!['a'], seq!['b']]) by (compute); }

// f6: `'a , b'` under `IFS=' ,'` -> TWO fields: the whitespace run plus the
// single comma is ONE delimiter unit (this is the `nw?` arm).
proof fn f6_ws_around_nw()
    ensures ifs_fields(s_a_comma_b(), ifs_sp_comma()) == seq![seq!['a'], seq!['b']]
{ assert(ifs_fields(s_a_comma_b(), ifs_sp_comma())
    == seq![seq!['a'], seq!['b']]) by (compute); }

// f7: `' a:: b '` under `IFS=' :'` -> a, "", b: the space after `::` is
// swallowed by the unit, so only ONE empty field appears.
proof fn f7_ws_nw_mix()
    ensures ifs_fields(s_pad_a_cc_b_pad(), ifs_sp_colon())
        == seq![seq!['a'], Seq::<char>::empty(), seq!['b']]
{ assert(ifs_fields(s_pad_a_cc_b_pad(), ifs_sp_colon())
    == seq![seq!['a'], Seq::<char>::empty(), seq!['b']]) by (compute); }

// f8: an EMPTY IFS disables splitting — one field, whatever the value.
proof fn f8_empty_ifs()
    ensures ifs_fields(s_a_cc_b(), Seq::empty()) == seq![s_a_cc_b()]
{ assert(ifs_fields(s_a_cc_b(), Seq::empty()) == seq![s_a_cc_b()]) by (compute); }

// f9: a TAB is ordinary under `IFS=' '` — one field, tab intact.
proof fn f9_tab_not_in_ifs()
    ensures ifs_fields(s_a_tab_b(), seq![' ']) == seq![s_a_tab_b()]
{ assert(ifs_fields(s_a_tab_b(), seq![' ']) == seq![s_a_tab_b()]) by (compute); }

// f10: …and a SEPARATOR under the startup IFS (space, tab, newline).
proof fn f10_tab_in_startup_ifs()
    ensures ifs_fields(s_a_tab_b(), ifs_startup()) == seq![seq!['a'], seq!['b']]
{ assert(ifs_fields(s_a_tab_b(), ifs_startup()) == seq![seq!['a'], seq!['b']]) by (compute); }

// The RETIRED splitter, transcribed: skip blanks, take a non-blank run,
// count — no IFS read at all. Kept as its own denotation so f11 is a
// PROVED difference rather than a cited number (`word_spans` above
// computes through `Seq::new` closures, which `compute` will not reduce).
pub open spec fn skip_blank(s: Str, i: int) -> int
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) || !is_blank(s[i]) {
        i
    } else {
        skip_blank(s, i + 1)
    }
}

pub open spec fn blank_end(s: Str, i: int) -> int
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) || is_blank(s[i]) {
        i
    } else {
        blank_end(s, i + 1)
    }
}

pub open spec fn blank_count(s: Str, i: int, k: int) -> int
    decreases k,
{
    if k <= 0 {
        0
    } else {
        let j = skip_blank(s, i);
        if j >= (s.len() as int) {
            0
        } else {
            1 + blank_count(s, blank_end(s, j), k - 1)
        }
    }
}

pub open spec fn blank_runs(s: Str) -> int {
    blank_count(s, 0, (s.len() as int) + 1)
}

// f11: the reported defect in numbers — `IFS=':'; for w in $v` over
// `a::b: c` is FOUR fields (bash), and the RETIRED blank-only splitter
// gave TWO, so the two denotations are observably different.
proof fn f11_retired_blank_splitter_differs()
    ensures (ifs_fields(s_abcd_colon(), ifs_colon()).len() as int) == 4int,
        blank_runs(s_abcd_colon()) == 2int,
        (ifs_fields(s_abcd_colon(), ifs_colon()).len() as int) != blank_runs(s_abcd_colon())
{ assert((ifs_fields(s_abcd_colon(), ifs_colon()).len() as int) == 4int) by (compute);
  assert(blank_runs(s_abcd_colon()) == 2int) by (compute); }

fn main() {}

} // verus!
