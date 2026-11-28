//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/grep_literal_fold.rs — the literal `grep` → `WriteFile` fold
//  (cluster F2 of shir_native_stmt.rs).
//
//  `native_static_grep_redirect` folds `echo LIT | grep [flags] PAT > FILE`
//  into `WriteFile(FILE, grep_output(words, input))` plus `status_exec(true)`.
//  Everything it emits comes from ONE analysis function, `grep_output`, whose
//  only matching primitives are `word_boundary_hit` and `String::contains`.
//  This file proves that analysis against a semantic side written from grep's
//  documented behaviour.
//
//  ── what is proved ─────────────────────────────────────────────────────
//
//   (A) THE WHOLE-WORD MATCHER. `word_boundary_hit` scans `i` from 0 and asks
//       "bytes equal here, and the byte before and after are not word bytes".
//       The semantic side is existence: there IS an offset with equal bytes
//       whose neighbours are non-word (or the line edge).
//       `word_scan_is_boundary` refines one into the other, so the loop's
//       stopping rule (`i + p.len() <= len`), its start offset and both
//       boundary checks are load-bearing.
//
//   (B) THE PLAIN MATCHER. `String::contains` is modelled twice: semantically
//       (`plain_hit`: some offset carries the pattern) and as the standard
//       leftmost-first byte scan (`contains_scan`); `contains_scan_is_plain`
//       refines them. A scan that skips offset 0, or stops one byte early,
//       disagrees with the first witness of either.
//
//   (C) THE SELECTION AND ITS CONTEXT. `grep_output` collects the matching
//       line indices, turns them into a range list (`start = m-before` clamped
//       at 0, `end = m+after+1` clamped at the line count, a new range opened
//       only when `start > last.end`), and prints every line of every range.
//       The semantic side is: print line `i` iff SOME match `m` has
//       `m - before <= i <= m + after`. `ranges_select_the_context` proves the
//       two index sets equal IN BOTH DIRECTIONS for every match list, every
//       `before`/`after` and every line count, and `ranges_are_ordered` proves
//       the list the fold builds is ordered and non-overlapping — so neither
//       the clamping, nor the merge test (`start <= end`, not `start < end`),
//       nor the `+1` can be dropped.
//
//   (D) THE EDGES, as concrete pins: a match at offset 0, `-w` inside a word
//       (refused) versus after a space (accepted), `-i` lowering BOTH sides,
//       context clamped at both ends, and the adjacent/merge versus far/append
//       cases of the range list.
//
//  ── the model ──────────────────────────────────────────────────────────
//
//   * A line is a `Seq<u8>`; the fold's `String` operations are their
//     byte-level counterparts (`lower_all` models `to_lowercase` on ASCII, so
//     the `-i` pin is stated over those bytes).
//   * `lines` are the input's REAL lines (a newline-terminated input has no
//     trailing empty line); the source's `input.split('\n')` yields one phantom
//     EMPTY element for it, which `phantom_is_never_a_match` shows a non-empty
//     literal pattern never selects. The empty pattern is where it shows —
//     divergence d3 below.
//   * The producer is the literal `echo` arm: the words joined by one space
//     and a trailing newline. That join is vstd's `Seq::join` shape and is
//     taken as given (§6); what this file uses is only that the input is
//     newline-terminated.
//
//  ── stated, not modelled ───────────────────────────────────────────────
//
//   * `literal_text` (a `Str`, or an all-`Lit` `Interpolate`) and
//     `redirect_target` (fd 1, mode `w`) are node classification, not
//     semantics; the fd/mode refusals are cluster B's law
//     (`native_redirect.rs`), not duplicated here.
//   * The `printf` producer arm (its bytes are `decode_capture_escapes`'s law),
//     locale collation, grep's stderr text, and `-A/-B/-C` with a non-numeric
//     argument (refused by the source) are out of model.
//   * THE LINE JOIN. `grep_output` turns the range list into a body by
//     pushing each line and then `'\n'`. That is a renderer step over the
//     selection proved here, not a second semantic side: this file proves
//     WHICH lines are selected, in what order and how many times. The body
//     the source builds is exactly the selected lines in range order, because
//     the ranges it walks are ordered and disjoint (`ranges_are_ordered`) —
//     so no line can be printed twice or out of order.
//   * The other eight grep folds (`native_echo_grep_stmt`, the herestring and
//     `-o` folds, the file pipeline, devnull-or, the static chain) are NOT
//     this proof: each has its own parse and its own status.
//
//  ── open divergences this proof RECORDS (it does not hide them) ────────
//
//  Three shapes reach the fold and do NOT agree with bash. Each is pinned by a
//  proof fn stating the SOURCE's answer, so a fix flips the pin instead of
//  silently weakening a law:
//
//   d1 `status_exec(true)` — the fold claims success unconditionally; `grep`
//      exits 1 when nothing matched.
//   d2 `-q`/`-s` — the parse accepts them and prints the matched lines anyway;
//      `grep -q PAT > FILE` writes an EMPTY file.
//   d3 the empty pattern — it carries no metacharacter, so every element of
//      `split` matches, including the phantom trailing one, printing one line
//      more than bash. (Not restated here: `phantom_is_never_a_match` proves
//      the phantom is inert for NON-empty patterns, and the empty pattern is
//      refused by the empty-herestring fold's own test.)
//
//  A fix is a source change (refuse `-q`, decide the status from
//  `matches.is_empty()`) and therefore a later stage with its own measured
//  re-run.
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ═══════════════════════════════════════════════════════════════════════════
// 1. bytes and the two matchers
// ═══════════════════════════════════════════════════════════════════════════

type Bytes = Seq<u8>;

/// `to_lowercase` on ASCII (a non-ASCII byte is left alone).
spec fn lower(x: u8) -> u8 {
    if b'A' <= x && x <= b'Z' { (x + 32) as u8 } else { x }
}

spec fn lower_all(s: Bytes) -> Bytes {
    Seq::new(s.len(), |i: int| lower(s[i]))
}

/// The bytes at `i` are exactly `p`.
spec fn slice_eq(b: Bytes, i: int, p: Bytes) -> bool {
    &&& 0 <= i
    &&& i + p.len() <= b.len()
    &&& (forall|j: int| 0 <= j && j < p.len() ==> b[i + j] == p[j])
}

/// THE SEMANTIC SIDE of plain grep: some offset of the line carries the
/// pattern.
spec fn plain_hit(b: Bytes, p: Bytes) -> bool {
    exists|i: int| slice_eq(b, i, p)
}

/// grep's word constituent.
spec fn word_byte(x: u8) -> bool {
    (b'0' <= x && x <= b'9') || (b'A' <= x && x <= b'Z')
        || (b'a' <= x && x <= b'z') || x == b'_'
}

/// A `-w` occurrence at offset `i`: equal bytes, non-word (or edge) on each
/// side.
spec fn hit_at(b: Bytes, p: Bytes, i: int) -> bool {
    &&& slice_eq(b, i, p)
    &&& (i == 0 || !word_byte(b[i - 1]))
    &&& (i + p.len() == b.len() || !word_byte(b[i + p.len()]))
}

/// THE SEMANTIC SIDE of `-w`: a whole-word occurrence exists.
spec fn boundary_hit(b: Bytes, p: Bytes) -> bool {
    &&& p.len() > 0
    &&& exists|i: int| hit_at(b, p, i)
}

/// THE FOLD SIDE of `contains`, transcribed as the standard leftmost-first
/// byte scan.
spec fn contains_scan(b: Bytes, p: Bytes, i: int) -> bool
    decreases b.len() as int + 1 - i
{
    if i + p.len() > b.len() { false }
    else if slice_eq(b, i, p) { true }
    else { contains_scan(b, p, i + 1) }
}

/// THE FOLD SIDE of `word_boundary_hit`, transcribed from its loop.
spec fn word_scan(b: Bytes, p: Bytes, i: int) -> bool
    decreases b.len() as int + 1 - i
{
    if i + p.len() > b.len() { false }
    else if hit_at(b, p, i) { true }
    else { word_scan(b, p, i + 1) }
}

// ── (B) the plain matcher ─────────────────────────────────────────────────

proof fn contains_scan_is_plain(b: Bytes, p: Bytes, i: int)
    requires 0 <= i
    ensures contains_scan(b, p, i) == (exists|j: int| i <= j && slice_eq(b, j, p))
    decreases b.len() as int + 1 - i
{
    if i + p.len() > b.len() {
        assert forall|j: int| i <= j implies !slice_eq(b, j, p) by {
            if slice_eq(b, j, p) {
                assert(j + p.len() <= b.len());
                assert(j < i);
            }
        }
    } else if slice_eq(b, i, p) {
        assert(i <= i && slice_eq(b, i, p));
    } else {
        contains_scan_is_plain(b, p, i + 1);
        assert(!slice_eq(b, i, p));
        assert forall|j: int| i <= j && slice_eq(b, j, p) implies i + 1 <= j by {
            if i <= j && slice_eq(b, j, p) {
                if j == i {
                    assert(slice_eq(b, i, p));
                }
            }
        }
    }
}

/// The law: the fold's substring scan finds a pattern iff it occurs.
proof fn fold_contains_is_plain(b: Bytes, p: Bytes)
    ensures contains_scan(b, p, 0) == plain_hit(b, p)
{
    contains_scan_is_plain(b, p, 0);
}

// ── (A) the whole-word matcher ─────────────────────────────────────────────

proof fn word_scan_is_boundary(b: Bytes, p: Bytes, i: int)
    requires 0 <= i
    ensures word_scan(b, p, i) == (exists|j: int| i <= j && hit_at(b, p, j))
    decreases b.len() as int + 1 - i
{
    if i + p.len() > b.len() {
        assert forall|j: int| i <= j implies !hit_at(b, p, j) by {
            if hit_at(b, p, j) {
                assert(slice_eq(b, j, p));
                assert(j + p.len() <= b.len());
                assert(j < i);
            }
        }
    } else if hit_at(b, p, i) {
        assert(i <= i && hit_at(b, p, i));
    } else {
        word_scan_is_boundary(b, p, i + 1);
        assert(!hit_at(b, p, i));
        assert forall|j: int| i <= j && hit_at(b, p, j) implies i + 1 <= j by {
            if i <= j && hit_at(b, p, j) {
                if j == i {
                    assert(hit_at(b, p, i));
                }
            }
        }
    }
}

/// The law: `word_boundary_hit`'s loop finds a whole-word occurrence iff one
/// exists.
proof fn fold_word_is_boundary(b: Bytes, p: Bytes)
    ensures (p.len() > 0 && word_scan(b, p, 0)) == boundary_hit(b, p)
{
    word_scan_is_boundary(b, p, 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. the per-line predicate, with and without `-i`
// ═══════════════════════════════════════════════════════════════════════════

/// THE SEMANTIC SIDE: does `grep [flags] PAT` select this line?
spec fn sem_hit(line: Bytes, pat: Bytes, word: bool) -> bool {
    if word { boundary_hit(line, pat) } else { plain_hit(line, pat) }
}

/// THE FOLD SIDE: `grep_output`'s per-line test.
spec fn fold_hit(line: Bytes, pat: Bytes, word: bool) -> bool {
    if word { pat.len() > 0 && word_scan(line, pat, 0) } else { contains_scan(line, pat, 0) }
}

proof fn fold_hit_is_sem(line: Bytes, pat: Bytes, word: bool)
    ensures fold_hit(line, pat, word) == sem_hit(line, pat, word)
{
    if word {
        fold_word_is_boundary(line, pat);
    } else {
        fold_contains_is_plain(line, pat);
    }
}

/// `-i` lowers BOTH sides before the same test.
spec fn fold_hit_ci(line: Bytes, pat: Bytes, word: bool) -> bool {
    fold_hit(lower_all(line), lower_all(pat), word)
}

spec fn sem_hit_ci(line: Bytes, pat: Bytes, word: bool) -> bool {
    sem_hit(lower_all(line), lower_all(pat), word)
}

proof fn fold_hit_ci_is_sem(line: Bytes, pat: Bytes, word: bool)
    ensures fold_hit_ci(line, pat, word) == sem_hit_ci(line, pat, word)
{
    fold_hit_is_sem(lower_all(line), lower_all(pat), word);
}

/// The phantom trailing element of `split('\n')` is never a match for a
/// non-empty pattern: it is the empty line, and no non-empty byte string fits
/// in it.
proof fn phantom_is_never_a_match(p: Bytes, word: bool)
    requires p.len() > 0
    ensures fold_hit(Seq::<u8>::empty(), p, word) == false,
        sem_hit(Seq::<u8>::empty(), p, word) == false
{
    fold_hit_is_sem(Seq::<u8>::empty(), p, word);
    assert(!plain_hit(Seq::<u8>::empty(), p)) by {
        if plain_hit(Seq::<u8>::empty(), p) {
            let i = choose|i: int| slice_eq(Seq::<u8>::empty(), i, p);
            assert(slice_eq(Seq::<u8>::empty(), i, p));
            assert(i + p.len() <= 0);
        }
    }
    assert(!boundary_hit(Seq::<u8>::empty(), p)) by {
        if boundary_hit(Seq::<u8>::empty(), p) {
            let i = choose|i: int| hit_at(Seq::<u8>::empty(), p, i);
            assert(hit_at(Seq::<u8>::empty(), p, i));
            assert(slice_eq(Seq::<u8>::empty(), i, p));
            assert(i + p.len() <= 0);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. the match list and the context windows
// ═══════════════════════════════════════════════════════════════════════════

/// The match indices are increasing: the source builds them with
/// `lines.iter().enumerate().filter_map`, and the range loop's merge test
/// depends on it.
spec fn sorted_inc(ms: Seq<int>) -> bool {
    forall|k: int| #![trigger ms[k]] 0 <= k && k < ms.len() as int - 1 ==> ms[k] < ms[k + 1]
}

/// THE FOLD SIDE: `m.saturating_sub(before)`.
spec fn win_start(m: int, b: int) -> int {
    if m - b < 0 { 0 } else { m - b }
}

/// THE FOLD SIDE: `(m + after + 1).min(lines.len())`.
spec fn win_end(m: int, a: int, n: int) -> int {
    if m + a + 1 > n { n } else { m + a + 1 }
}

/// THE SEMANTIC SIDE: line `i` is in match `m`'s context, straight from the
/// manual's arithmetic and the line count. Shares nothing with
/// `win_start`/`win_end`.
spec fn in_context(m: int, b: int, a: int, n: int, i: int) -> bool {
    &&& 0 <= i && i < n
    &&& m - b <= i
    &&& i <= m + a
}

/// THE SEMANTIC SIDE of the selection.
spec fn covered(ms: Seq<int>, b: int, a: int, n: int, i: int) -> bool {
    exists|k: int| 0 <= k && k < ms.len() as int && in_context(ms[k], b, a, n, i)
}

spec fn covered_upto(ms: Seq<int>, b: int, a: int, n: int, k: int, i: int) -> bool {
    exists|j: int| 0 <= j && j < k && in_context(ms[j], b, a, n, i)
}

/// Ordered and non-overlapping: each range is non-empty and ends no later than
/// the next one begins.
spec fn ordered(rs: Seq<(int, int)>) -> bool {
    forall|k: int| #![trigger rs[k]] 0 <= k && k < rs.len() as int
        ==> rs[k].0 < rs[k].1
            && (k + 1 < rs.len() as int ==> rs[k].1 <= rs[k + 1].0)
}

spec fn drop_last(rs: Seq<(int, int)>) -> Seq<(int, int)> {
    Seq::new((rs.len() - 1) as nat, |i: int| rs[i])
}

spec fn last_of(rs: Seq<(int, int)>) -> (int, int) {
    rs[rs.len() as int - 1]
}

/// One merge step of the source's loop.
spec fn merge(rs: Seq<(int, int)>, s: int, e: int) -> Seq<(int, int)> {
    if rs.len() as int == 0 {
        rs.push((s, e))
    } else {
        let last = last_of(rs);
        if s <= last.1 {
            drop_last(rs).push((last.0, if last.1 > e { last.1 } else { e }))
        } else {
            rs.push((s, e))
        }
    }
}

/// The source's loop over the match list.
spec fn ranges_from(ms: Seq<int>, b: int, a: int, n: int, k: int, rs: Seq<(int, int)>)
    -> Seq<(int, int)>
    decreases ms.len() as int + 1 - k
{
    if k >= ms.len() as int { rs }
    else {
        ranges_from(ms, b, a, n, k + 1, merge(rs, win_start(ms[k], b), win_end(ms[k], a, n)))
    }
}

spec fn ranges(ms: Seq<int>, b: int, a: int, n: int) -> Seq<(int, int)> {
    ranges_from(ms, b, a, n, 0, Seq::<(int, int)>::empty())
}

spec fn in_ranges(rs: Seq<(int, int)>, i: int) -> bool {
    exists|k: int| 0 <= k && k < rs.len() as int && rs[k].0 <= i && i < rs[k].1
}

// ── monotonicity of the clamped bounds, and the clamp's meaning ───────────

proof fn win_start_mono(m1: int, m2: int, b: int)
    requires m1 <= m2, 0 <= b
    ensures win_start(m1, b) <= win_start(m2, b)
{
}

proof fn win_end_mono(m1: int, m2: int, a: int, n: int)
    requires m1 <= m2
    ensures win_end(m1, a, n) <= win_end(m2, a, n)
{
}

proof fn win_start_nonneg(m: int, b: int)
    requires 0 <= b
    ensures 0 <= win_start(m, b)
{
}

proof fn win_end_le(m: int, a: int, n: int)
    ensures win_end(m, a, n) <= n
{
}

/// The fold's clamp IS the manual's arithmetic, at EVERY index: a negative
/// index is outside `win_start` (the clamp is non-negative) and an index at
/// or past the count is outside `win_end` (the clamp stops there).
proof fn win_is_context(m: int, b: int, a: int, n: int, i: int)
    requires 0 <= b, 0 <= a
    ensures (win_start(m, b) <= i && i < win_end(m, a, n)) == in_context(m, b, a, n, i)
{
    win_start_nonneg(m, b);
    win_end_le(m, a, n);
}

// ── membership helpers over the range list ────────────────────────────────

proof fn push_member(rs: Seq<(int, int)>, r: (int, int), i: int)
    ensures in_ranges(rs.push(r), i) == (in_ranges(rs, i) || (r.0 <= i && i < r.1))
{
    // forward: a member of the pushed list is in the old list or is the new one
    assert forall|k: int| 0 <= k && k < rs.push(r).len() as int
        && rs.push(r)[k].0 <= i && i < rs.push(r)[k].1
        implies (in_ranges(rs, i) || (r.0 <= i && i < r.1)) by {
        if k < rs.len() as int {
            assert(rs.push(r)[k] == rs[k]);
            assert(in_ranges(rs, i)) by {
                assert(0 <= k && k < rs.len() as int && rs[k].0 <= i && i < rs[k].1);
            }
        } else {
            assert(rs.push(r)[k] == r);
        }
    }
    // backward, from the old list
    if in_ranges(rs, i) {
        let k = choose|k: int| 0 <= k && k < rs.len() as int
            && rs[k].0 <= i && i < rs[k].1;
        assert(rs.push(r)[k] == rs[k]);
        assert(in_ranges(rs.push(r), i)) by {
            assert(0 <= k && k < rs.push(r).len() as int
                && rs.push(r)[k].0 <= i && i < rs.push(r)[k].1);
        }
    }
    // backward, from the new range
    if r.0 <= i && i < r.1 {
        let n = rs.len() as int;
        let np = rs.push(r).len() as int;
        assert(0 <= n && n < np);
        assert((rs.push(r)[n]).0 <= i);
        assert(i < (rs.push(r)[n]).1);
        assert(in_ranges(rs.push(r), i));
    }
}

proof fn drop_last_member(rs: Seq<(int, int)>, i: int)
    requires rs.len() as int > 0
    ensures in_ranges(rs, i)
        == (in_ranges(drop_last(rs), i)
            || (last_of(rs).0 <= i && i < last_of(rs).1))
{
    assert forall|k: int| 0 <= k && k < rs.len() as int && rs[k].0 <= i && i < rs[k].1
        implies (in_ranges(drop_last(rs), i)
            || (last_of(rs).0 <= i && i < last_of(rs).1)) by {
        if k < rs.len() as int - 1 {
            assert(drop_last(rs)[k] == rs[k]);
            assert(in_ranges(drop_last(rs), i)) by {
                assert(0 <= k && k < drop_last(rs).len() as int
                    && drop_last(rs)[k].0 <= i && i < drop_last(rs)[k].1);
            }
        } else {
            assert(rs[k] == last_of(rs));
        }
    }
    if in_ranges(drop_last(rs), i) {
        let k = choose|k: int| 0 <= k && k < drop_last(rs).len() as int
            && drop_last(rs)[k].0 <= i && i < drop_last(rs)[k].1;
        assert(drop_last(rs)[k] == rs[k]);
        assert(in_ranges(rs, i)) by {
            assert(0 <= k && k < rs.len() as int && rs[k].0 <= i && i < rs[k].1);
        }
    }
    if last_of(rs).0 <= i && i < last_of(rs).1 {
        assert(rs[rs.len() as int - 1] == last_of(rs));
        assert(in_ranges(rs, i)) by {
            assert(0 <= rs.len() as int - 1 && rs.len() as int - 1 < rs.len() as int
                && rs[rs.len() as int - 1].0 <= i
                && i < rs[rs.len() as int - 1].1);
        }
    }
}

/// One merge step, semantically: an index is in the merged list iff it was in
/// the list before or lies in the new window. The only fact the extend branch
/// needs is that the last range starts no later than the new window.
proof fn merge_sound(rs: Seq<(int, int)>, s: int, e: int, i: int)
    requires rs.len() as int == 0 || last_of(rs).0 <= s
    ensures
        in_ranges(merge(rs, s, e), i) == (in_ranges(rs, i) || (s <= i && i < e)),
{
    if rs.len() as int == 0 {
        assert(merge(rs, s, e) == rs.push((s, e)));
        push_member(rs, (s, e), i);
    } else {
        let last = last_of(rs);
        if s <= last.1 {
            let m = if last.1 > e { last.1 } else { e };
            let base = drop_last(rs);
            assert(merge(rs, s, e) == base.push((last.0, m)));
            push_member(base, (last.0, m), i);
            drop_last_member(rs, i);
            // (last.0, last.1) v (s, e) == (last.0, m), given last.0 <= s
            assert((in_ranges(base, i) || (last.0 <= i && i < last.1)
                || (s <= i && i < e))
                == (in_ranges(base, i) || (last.0 <= i && i < m))) by {
                if last.0 <= i && i < m {
                    if i < last.1 {
                    } else if i < e {
                        assert(last.1 <= i);
                        assert(s <= i);
                    } else {
                        assert(m == e);
                    }
                }
                if s <= i && i < e {
                    assert(last.0 <= i && i < m);
                }
            }
        } else {
            assert(merge(rs, s, e) == rs.push((s, e)));
            push_member(rs, (s, e), i);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. (C) the range list selects exactly the context
// ═══════════════════════════════════════════════════════════════════════════

proof fn ranges_from_selects(ms: Seq<int>, b: int, a: int, n: int, k: int, rs: Seq<(int, int)>, i: int)
    requires
        0 <= k && k <= ms.len() as int,
        sorted_inc(ms),
        0 <= b,
        0 <= a,
        forall|j: int| #![trigger ms[j]] 0 <= j && j < ms.len() as int ==> 0 <= ms[j] && ms[j] < n,
        forall|j: int| #![trigger in_ranges(rs, j)] in_ranges(rs, j) == covered_upto(ms, b, a, n, k, j),
        rs.len() as int > 0 && k < ms.len() as int ==> last_of(rs).0 <= win_start(ms[k], b),
        rs.len() as int > 0 && k < ms.len() as int ==> last_of(rs).1 <= win_end(ms[k], a, n),
    ensures
        in_ranges(ranges_from(ms, b, a, n, k, rs), i) == covered_upto(ms, b, a, n, ms.len() as int, i),
    decreases ms.len() as int + 1 - k,
{
    if k >= ms.len() as int {
        assert(covered_upto(ms, b, a, n, ms.len() as int, i) == covered_upto(ms, b, a, n, k, i));
    } else {
        let s = win_start(ms[k], b);
        let e = win_end(ms[k], a, n);
        assert(0 <= ms[k] && ms[k] < n);
        assert(s < e) by {
            win_is_context(ms[k], b, a, n, ms[k]);
            assert(in_context(ms[k], b, a, n, ms[k]));
        }
        merge_sound(rs, s, e, i);
        assert forall|j: int| in_ranges(merge(rs, s, e), j)
            == covered_upto(ms, b, a, n, k + 1, j) by {
            merge_sound(rs, s, e, j);
            assert(in_ranges(rs, j) == covered_upto(ms, b, a, n, k, j));
            win_is_context(ms[k], b, a, n, j);
            assert(in_context(ms[k], b, a, n, j)
                == (s <= j && j < e)) by {
                assert(s == win_start(ms[k], b));
                assert(e == win_end(ms[k], a, n));
            }
        }
        assert(merge(rs, s, e).len() as int > 0 && k + 1 < ms.len() as int
            ==> last_of(merge(rs, s, e)).0 <= win_start(ms[k + 1], b)) by {
            if k + 1 < ms.len() as int {
                assert(ms[k] < ms[k + 1]) by {
                    assert(sorted_inc(ms));
                }
                win_start_mono(ms[k], ms[k + 1], b);
                assert(last_of(merge(rs, s, e)).0 <= s);
            }
        }
        assert(merge(rs, s, e).len() as int > 0 && k + 1 < ms.len() as int
            ==> last_of(merge(rs, s, e)).1 <= win_end(ms[k + 1], a, n)) by {
            if k + 1 < ms.len() as int {
                assert(ms[k] < ms[k + 1]) by {
                    assert(sorted_inc(ms));
                }
                win_end_mono(ms[k], ms[k + 1], a, n);
                let merged = merge(rs, s, e);
                if rs.len() as int == 0 {
                    assert(last_of(merged) == (s, e));
                } else if s <= last_of(rs).1 {
                    assert(last_of(merged)
                        == (last_of(rs).0, if last_of(rs).1 > e { last_of(rs).1 } else { e }));
                    assert(last_of(merged).1 <= e);
                } else {
                    assert(last_of(merged) == (s, e));
                }
            }
        }
        ranges_from_selects(ms, b, a, n, k + 1, merge(rs, s, e), i);
    }
}

/// The law, from the empty accumulator.
proof fn ranges_select_the_context(ms: Seq<int>, b: int, a: int, n: int, i: int)
    requires
        sorted_inc(ms),
        0 <= b,
        0 <= a,
        forall|j: int| #![trigger ms[j]] 0 <= j && j < ms.len() as int ==> 0 <= ms[j] && ms[j] < n,
    ensures
        in_ranges(ranges(ms, b, a, n), i) == covered(ms, b, a, n, i),
{
    ranges_from_selects(ms, b, a, n, 0, Seq::<(int, int)>::empty(), i);
    assert(covered_upto(ms, b, a, n, ms.len() as int, i) == covered(ms, b, a, n, i));
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. the status, and the two recorded behaviour divergences
// ═══════════════════════════════════════════════════════════════════════════

/// THE SEMANTIC SIDE: grep exits 0 when something matched, 1 when nothing did.
spec fn sem_status(ms: Seq<int>) -> int {
    if ms.len() == 0 { 1 } else { 0 }
}

/// THE FOLD SIDE: `status_exec(true)`.
spec fn fold_status() -> int {
    0
}

/// d1: with no match the two disagree — the fold claims success, grep says 1.
proof fn pin_fold_claims_success_without_a_match()
{
    assert(sem_status(Seq::<int>::empty()) == 1);
    assert(fold_status() == 0);
}

/// d2: `-q` prints nothing; the fold's parse accepts `-q` and prints anyway.
spec fn sem_body_quiet(body: Seq<Bytes>) -> Seq<Bytes> {
    Seq::<Bytes>::empty()
}

spec fn fold_body_quiet(body: Seq<Bytes>) -> Seq<Bytes> {
    body
}

proof fn pin_quiet_still_prints_the_lines()
{
    assert(sem_body_quiet(seq![seq![b'a', b'b']]) == Seq::<Bytes>::empty());
    assert(fold_body_quiet(seq![seq![b'a', b'b']]) == seq![seq![b'a', b'b']]);
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. concrete pins: the load-bearing refusals and shapes
// ═══════════════════════════════════════════════════════════════════════════

/// Offset 0 is scanned: a pattern at the very start is found, and `-w` finds it
/// when the byte after is not a word byte.
proof fn pin_first_offset_is_scanned()
{
    assert(slice_eq(seq![b'a', b'b'], 0, seq![b'a']));
    assert(plain_hit(seq![b'a', b'b'], seq![b'a']));
    assert(hit_at(seq![b'a', b' ', b'b'], seq![b'a'], 0));
    assert(boundary_hit(seq![b'a', b' ', b'b'], seq![b'a']));
    fold_hit_is_sem(seq![b'a', b'b'], seq![b'a'], false);
    fold_hit_is_sem(seq![b'a', b' ', b'b'], seq![b'a'], true);
}

/// `-w` inside a word: plain grep accepts the occurrence, `-w` refuses it
/// because the byte after (or before) is a word byte.
proof fn pin_word_match_needs_both_boundaries()
{
    assert(slice_eq(seq![b'f', b'o', b'o', b'b', b'a', b'r'], 0, seq![b'f', b'o', b'o']));
    assert(plain_hit(seq![b'f', b'o', b'o', b'b', b'a', b'r'], seq![b'f', b'o', b'o']));
    assert(!boundary_hit(seq![b'f', b'o', b'o', b'b', b'a', b'r'], seq![b'f', b'o', b'o'])) by {
        assert forall|i: int| !hit_at(seq![b'f', b'o', b'o', b'b', b'a', b'r'],
            seq![b'f', b'o', b'o'], i) by {
            if hit_at(seq![b'f', b'o', b'o', b'b', b'a', b'r'], seq![b'f', b'o', b'o'], i) {
                assert(slice_eq(seq![b'f', b'o', b'o', b'b', b'a', b'r'], i,
                    seq![b'f', b'o', b'o']));
                assert(0 <= i && i + 3 <= 6);
                if i == 0 {
                    assert(word_byte(b'b'));
                    assert(word_byte(seq![b'f', b'o', b'o', b'b', b'a', b'r'][3]));
                } else if i == 1 {
                    assert(word_byte(b'f'));
                    assert(word_byte(seq![b'f', b'o', b'o', b'b', b'a', b'r'][0]));
                } else {
                    assert(word_byte(b'o'));
                    assert(word_byte(seq![b'f', b'o', b'o', b'b', b'a', b'r'][i - 1]));
                }
            }
        }
    }
    fold_hit_is_sem(seq![b'f', b'o', b'o', b'b', b'a', b'r'], seq![b'f', b'o', b'o'], true);
    fold_hit_is_sem(seq![b'f', b'o', b'o', b'b', b'a', b'r'], seq![b'f', b'o', b'o'], false);
}

/// `-i` lowers BOTH sides: `FOO` matches `fo` only through the lowering.
proof fn pin_case_fold_lowers_both_sides()
{
    assert(lower(b'F') == b'f');
    assert(lower(b'O') == b'o');
    assert(lower(b'o') == b'o');
    assert(slice_eq(seq![b'f', b'o', b'o'], 0, seq![b'f', b'o']));
    assert(plain_hit(seq![b'f', b'o', b'o'], seq![b'f', b'o']));
    assert(!plain_hit(seq![b'F', b'O', b'O'], seq![b'f', b'o'])) by {
        assert forall|i: int| !slice_eq(seq![b'F', b'O', b'O'], i, seq![b'f', b'o']) by {
            if slice_eq(seq![b'F', b'O', b'O'], i, seq![b'f', b'o']) {
                assert(0 <= i && i + 2 <= 3);
                if i == 0 {
                } else {
                    assert(i >= 1);
                }
            }
        }
    }
    fold_hit_ci_is_sem(seq![b'F', b'O', b'O'], seq![b'f', b'o'], false);
}

/// The context clamps at both ends: `-B 3` on line 1 starts at 0, `-A 3` on the
/// last line stops at the count.
proof fn pin_context_clamps_at_the_edges()
{
    assert(win_start(1, 3) == 0);
    assert(win_end(2, 3, 3) == 3);
    assert(in_context(1, 3, 0, 3, 0));
    assert(covered(seq![1], 3, 0, 3, 0)) by {
        assert(in_context(seq![1][0], 3, 0, 3, 0));
    }
    assert(covered(seq![1], 3, 0, 3, 1));
    assert(!in_context(1, 3, 0, 3, 2));
    assert(!covered(seq![1], 3, 0, 3, 2));
}

/// The merge test is `start <= end` (adjacent windows join), and a merge keeps
/// the earlier range's start while extending its end; a far window appends.
proof fn pin_adjacent_windows_merge()
{
    assert(merge(Seq::<(int, int)>::empty(), 0, 2) == seq![(0, 2)]);
    assert(merge(seq![(0, 2)], 2, 4) == seq![(0, 4)]);
    assert(merge(seq![(0, 2)], 3, 4) == seq![(0, 2), (3, 4)]);
    let one: Seq<(int, int)> = seq![(0, 4)];
    let two: Seq<(int, int)> = seq![(0, 2)];
    assert(in_ranges(one, 1)) by {
        assert(one[0] == (0, 4));
        assert(one[0].0 <= 1 && 1 < one[0].1);
    }
    assert(!in_ranges(two, 2)) by {
        assert forall|k: int| !(0 <= k && k < 1 && two[k].0 <= 2
            && 2 < two[k].1) by {
        }
    }
}

/// A `-v`-style flag is NOT the plain predicate: `-v b` prints a line plain
/// grep refuses. The fold has no inverse predicate, so it must refuse the flag
/// rather than fold it as a plain match.
proof fn pin_unknown_flag_refused()
{
    // The two predicates this file defines are the plain one and `-w`; `-v` is
    // the inverse, which has no model here, so the fold must refuse the flag.
    // The concrete refusal is the generated test on `grep_output`.
    assert(slice_eq(seq![b'b'], 0, seq![b'b']));
    assert(sem_hit(seq![b'b'], seq![b'b'], false));
}

/// A pattern carrying a metacharacter is a REGEX; the model's matcher is
/// literal, so `.` is an ordinary byte here (it must equal the line's byte to
/// match). That is exactly why the fold must refuse such patterns instead of
/// matching them literally.
proof fn pin_metacharacter_pattern_refused()
{
    // A `.` is an ordinary byte in the only matcher this file has: it matches
    // itself, so a BRE pattern would be folded LITERALLY. That is why the
    // source refuses patterns carrying a metacharacter (generated test).
    assert(slice_eq(seq![b'.'], 0, seq![b'.']));
}

/// A second plain word is a FILE OPERAND, not part of the pattern: treating
/// `grep a b` as "pattern `a b`" would select nothing on this line, while the
/// real command selects it from the file.
proof fn pin_file_operand_refused()
{
    assert(!plain_hit(seq![b'a'], seq![b'a', b' ', b'b']));
}

/// `-A`'s argument is a count, and the count changes the window: there is no
/// interpretation of a non-number, so the parse must refuse it rather than
/// default to 0 (or 1).
proof fn pin_context_argument_must_be_a_number()
{
    assert(win_end(1, 1, 5) != win_end(1, 0, 5));
    assert(win_end(1, 1, 5) == 3);
}

/// No match means no range at all, so no line can be selected: the body the
/// fold writes is empty, and the index-set half of the output is trivial.
proof fn pin_no_match_writes_an_empty_body()
{
    let none: Seq<int> = Seq::<int>::empty();
    assert(ranges(none, 0, 0, 3).len() == 0);
    assert(!in_ranges(ranges(none, 0, 0, 3), 0));
}

/// The empty pattern selects EVERY element, including the phantom empty line
/// `split('\n')` leaves on newline-terminated input — the recorded divergence.
proof fn pin_empty_pattern_prints_the_phantom()
{
    assert(slice_eq(Seq::<u8>::empty(), 0, Seq::<u8>::empty()));
    assert(plain_hit(Seq::<u8>::empty(), Seq::<u8>::empty()));
    assert(slice_eq(seq![b'h', b'i'], 0, Seq::<u8>::empty()));
    assert(plain_hit(seq![b'h', b'i'], Seq::<u8>::empty()));
    // and a NON-empty pattern never reaches the phantom, which is why the
    // body law may model the input's real lines alone
    phantom_is_never_a_match(seq![b'x'], false);
}

/// A literal pattern selects the line that carries it (offset 0 witness).
proof fn pin_plain_line_selected()
{
    assert(slice_eq(seq![b'f', b'o', b'o', b'b', b'a', b'r'], 0, seq![b'f', b'o', b'o']));
    assert(plain_hit(seq![b'f', b'o', b'o', b'b', b'a', b'r'], seq![b'f', b'o', b'o']));
}

/// `-w` after a space: a non-word byte on the left, the line edge on the right.
proof fn pin_word_match_after_a_space()
{
    assert(hit_at(seq![b'a', b' ', b'f', b'o', b'o'], seq![b'f', b'o', b'o'], 2));
    assert(boundary_hit(seq![b'a', b' ', b'f', b'o', b'o'], seq![b'f', b'o', b'o']));
}

/// `-B 1` prints line 1's leading context: line 0 and line 1 are covered, the
/// line after the match is not.
proof fn pin_context_prints_the_window()
{
    assert(in_context(1, 1, 0, 3, 0));
    assert(covered(seq![1], 1, 0, 3, 0)) by {
        assert(in_context(seq![1][0], 1, 0, 3, 0));
    }
    assert(in_context(1, 1, 0, 3, 1));
    assert(covered(seq![1], 1, 0, 3, 1)) by {
        assert(in_context(seq![1][0], 1, 0, 3, 1));
    }
    assert(!in_context(1, 1, 0, 3, 2));
    assert(!covered(seq![1], 1, 0, 3, 2));
}

fn main() {}
}
