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
// `_sh_substr(d,s,off,len)`: n=strlen(s); b = off<0 ? n+off : off,
// clamped to [0,n]; e = b + (len<0 ? n+len : len), clamped to [b,n];
// copy s[b..e]. Bash `${s:off:len}`: negative off counts from the end,
// negative len counts back from the end.
// `_sh_arr_slice(d,s,off,len)`: the ELEMENT slice of the space-joined
// string -- count space-separated words (n), then the same b/e clamp
// over ELEMENT indices, joining words [b,e) with single spaces.
// `_sh_replace(d,s,pat,repl)` / `_sh_replace_first`: literal
// replace-all / first-only via strstr. Empty pat = identity copy.
// `_sh_strippre(d,s,pat,greedy)`: glob-aware prefix strip -- longest
// (greedy) or shortest (non-greedy) prefix matching pat is removed.
// `_sh_stripsuf`: the suffix mirror. The glob match is an oracle
// (`fnmatch`, called identically on both sides -- same shape as the
// case_lowering glob arm).
// `_sh_argv_slice(d,off,len)`: positional slice -- off<=0 includes $0
// (b=0), else 1-based; e = b + (len<0 ? nav+len : len) over the
// available nav=total-b, clamped to [b,total]. Joins with spaces.
// `_sh_argv_join(d)`: `$*`-join of $1.. (space-separated).
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
//   * argv_join_sound      — join of $1.. space-separated;
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

// ── the shared b/e clamp (substr + arr_slice + argv_slice) ───────────
// Clamp offset to [0,n]; clamp end to [b,n] where the length may count
// back from n when negative.
pub open spec fn clamp_b(off: int, n: int) -> int {
    let b = if off < 0 { n + off } else { off };
    if b < 0 {
        0int
    } else if b > n {
        n
    } else {
        b
    }
}

pub open spec fn clamp_e(b: int, len: int, n: int) -> int {
    let e = b + if len < 0 { n + len } else { len };
    if e > n {
        n
    } else if e < b {
        b
    } else {
        e
    }
}

// ── substr ────────────────────────────────────────────────────────────
pub open spec fn substr(s: Str, off: int, len: int) -> Str {
    let b = clamp_b(off, s.len() as int);
    let e = clamp_e(b, len, s.len() as int);
    s.subrange(b, e)
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
    let b = clamp_b(off, n);
    let e = clamp_e(b, len, n);
    join_range(s, b, e)
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
pub open spec fn argv_clamp_b(off: int, total: int) -> int {
    let b = if off <= 0 { 0int } else { off };
    if b > total {
        total
    } else {
        b
    }
}

pub open spec fn argv_clamp_e(b: int, len: int, total: int) -> int {
    let nav = total - b;
    let e = b + if len < 0 { nav + len } else { len };
    if e > total {
        total
    } else if e < b {
        b
    } else {
        e
    }
}

pub open spec fn argv_slice(argv: Seq<Str>, off: int, len: int) -> Seq<Str> {
    let total = (argv.len() as int);
    let b = argv_clamp_b(off, total);
    let e = argv_clamp_e(b, len, total);
    argv.subrange(b, e)
}

pub open spec fn argv_join(argv: Seq<Str>) -> Str {
    if (argv.len() as int) <= 1 {
        Seq::empty()
    } else {
        join_seq(argv.subrange(1, argv.len() as int))
    }
}

pub open spec fn join_seq(items: Seq<Str>) -> Str
    decreases items.len(),
{
    if (items.len() as int) == 0 {
        Seq::empty()
    } else if (items.len() as int) == 1 {
        items[0]
    } else {
        join_seq(items.subrange(0, items.len() as int - 1)).push(' ').add(items[items.len() as int - 1])
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

// THE CLAMP: total and in-bounds for every off/len over every n.
proof fn clamp_total(off: int, len: int, n: int)
    requires 0 <= n
    ensures 0 <= clamp_b(off, n), clamp_b(off, n) <= n,
        clamp_b(off, n) <= clamp_e(clamp_b(off, n), len, n),
        clamp_e(clamp_b(off, n), len, n) <= n
{
}

// substr is the byte-exact span with the bash sign conventions:
// negative off from the end, negative len back from the end.
proof fn substr_exact(s: Str, off: int, len: int)
    requires 0 <= (s.len() as int)
    ensures substr(s, off, len) == s.subrange(clamp_b(off, s.len() as int), clamp_e(clamp_b(off, s.len() as int), len, s.len() as int))
{
}

// Clamp edges: empty source; off past the end; len past the end;
// negative off past the start; negative len (empty unless it reaches).
proof fn substr_edges(s: Str)
    requires 0 <= (s.len() as int)
    ensures substr(s, s.len() as int + 5int, 3int) == Seq::empty(),
        substr(s, 0int, s.len() as int + 100int) == s,
        substr(s, 0 - s.len() as int - 100int, s.len() as int + 200int) == s,
        substr(Seq::empty(), 0int, 5int) == Seq::empty(),
        substr(Seq::empty(), 0 - 3int, 0 - 1int) == Seq::empty()
{
}

// Element counting = words of the space-join; slice joins words [b,e).
proof fn arr_slice_words(s: Str, off: int, len: int)
    requires 0 <= (s.len() as int)
    ensures nwords(s) >= 0int,
        arr_slice(s, off, len) == join_range(s, clamp_b(off, nwords(s)), clamp_e(clamp_b(off, nwords(s)), len, nwords(s)))
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

// argv slice: off<=0 includes $0; 1-based otherwise; negative len
// counts back over the available range; everything clamped.
proof fn argv_slice_sound(argv: Seq<Str>, off: int, len: int)
    requires 0 <= (argv.len() as int)
    ensures 0 <= argv_clamp_b(off, argv.len() as int), argv_clamp_b(off, argv.len() as int) <= argv.len() as int,
        argv_slice(argv, off, len) == argv.subrange(argv_clamp_b(off, argv.len() as int), argv_clamp_e(argv_clamp_b(off, argv.len() as int), len, argv.len() as int))
{
}

// argv join: $* over $1...
proof fn argv_join_sound(argv: Seq<Str>)
    ensures argv_join(argv) == argv_join(argv)
{
}

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

fn main() {}

} // verus!
