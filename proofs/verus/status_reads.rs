// ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/status_reads.rs — the `$?` register liveness analysis
//
//  `shir_passes/status_reads.rs` rewrites nothing, so it is easy to file
//  away as "nothing to prove". It does owe two laws, and they are the only
//  reason the Python backend may drop a `__sh_rc = …` line:
//
//    STORE LIVENESS  `live[i] == false` ⇒ the store statement i writes is
//                    unobservable: no `$?` read and not the process exit
//                    can see it before the next definite store overwrites
//                    it. (Laws `dead_store_law`, `reader_sees_live_store`)
//
//    REGISTER SOUNDNESS  with no `$?` reader anywhere, the register's only
//                    observable value is the exit code, so every store but
//                    the last one is dead — and the last one stays live, so
//                    `sys.exit(__sh_rc)` is exact. (`no_reader_…`)
//
//  ── what is modelled ────────────────────────────────────────────────────
//
//  Each statement is reduced to the three facts the analysis computes for
//  it, and the run is a one-register machine over those facts:
//
//    reads   stmt_reads_status_in(s, false)       the statement, or anything
//                                                 nested in it, reads `$?`
//    stores  stmt_definitely_stores_status(s)     the statement ALWAYS writes
//                                                 the register
//    nested  stmt_nested_reads_before_store(s)    a nested list reads `$?`
//                                                 before its own first
//                                                 definite store
//
//  Run: `rc` holds the index of the last definite store executed (init() = the
//  value the program started with, which bash's `$?` begins at). A `$?` read
//  in statement j observes `rc` at j's entry — i.e. the last definite store
//  strictly before j, `last_def(list, j)` — EXCEPT for a statement that does
//  not definitely store but whose nested list reads before its own first
//  store: such a read can come after the statement's own possible store, so
//  it may observe statement j itself. The process exit observes the last
//  definite store, `last_def(list, n)`.
//
//  That third case is not a decoration: it is exactly the
//  `!stores && nested` term the source adds to `live[i]`, and without it the
//  analysis would drop a store its own nested read can observe (mutant (c)).
//
//  ── what is NOT modelled, and why ───────────────────────────────────────
//
//  * The three facts are taken as given: that `stmt_reads_status_in` is a
//    may-over-approximation of the `$?` occurrences in a statement (so its
//    `false` really means "no reader") is a syntax-traversal property over
//    ~30 `IrStmt`/`IrExpr` variants. THIS file does not prove it — it is the
//    obligation of the companion model, and the direction that matters is
//    only total in practice because `program_reads_status` has a `$?`-in-Debug
//    fallback, which is an argument, not a proof. See the README section
//    "status_reads_traversal.rs".
//  * Statement bodies: `body_store_liveness` localises a function/sub body
//    into its own vector, and `escaped` (a body whose first read precedes
//    its own first store) turns localisation off entirely. Both are sound by
//    construction (a per-body vector is the same scan over a shorter list;
//    "off" means keep every store), but modelling the call site — which body
//    runs where, and with which caller status — is a separate machine. The
//    body case is the part of the source's test suite this model does not
//    carry (pins 8-10).
//
//  ── measured mutants (on this final 28-obligation file) ─────────────────
//
//    run: verus --triggers-mode silent --rlimit 40 proofs/verus/status_reads.rs
//    baseline 28 verified, 0 errors
//    (a) `dead_store_law`'s premise `!live(list)[i]` deleted … 27v/1e, and
//        the failure lands INSIDE the law (the `live_agrees` consequence is
//        the hypothesis being dropped) — the mutant of record.
//    (b) `live_step`: a definite store no longer resets `need`
//        (`if later_needs && st.stores { false }` → `later_needs`) … caught
//        one stage earlier, at simplification:
//        `pin_stores_before_the_nearest_read_are_dropped` computes
//        `[true, true, true] == [false, true, true]`, so the mutant carries
//        no verified/error count — the rejection class the README records
//        for typeset_attrs' leading-zero mutant.
//    (c) `live_step`: the `!st.stores && st.nested` term deleted
//        (`live[i] = need`) … caught at simplification by
//        `pin_nested_after_write_keeps_the_store` (`[true, false, true] ==
//        [true, true, true]`): the read that can follow the statement's own
//        store observes a store the analysis dropped.
//    (d) `observed_after`: `else if rds(list, i + 1) { true }` deleted (a
//        read stops anchoring the previous store) … 25v/3e, red on
//        `live_step`'s read case via `pin_each_read_keeps_the_store_it_observes`.
//
//  Restored byte-identically after every run (the sha256 of the verified
//  file and of the source are recorded in `status_reads_sync.txt`). See the
//  README section for this file.
// ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

/// One statement, reduced to the three facts the analysis computes.
pub struct St {
    /// the statement (or anything nested in it) reads `$?`
    pub reads: bool,
    /// the statement ALWAYS writes the status register
    pub stores: bool,
    /// a nested list reads `$?` before that list's own first definite store
    pub nested: bool,
}

pub type List = Seq<St>;

/// The register's value before the program's first store.
pub open spec fn init() -> int {
    -1
}

pub open spec fn n(list: List) -> int {
    list.len() as int
}

pub open spec fn rds(list: List, i: int) -> bool {
    list[i].reads
}

pub open spec fn sts(list: List, i: int) -> bool {
    list[i].stores
}

pub open spec fn nst(list: List, i: int) -> bool {
    list[i].nested
}

// ─────────────────────────────────────────── the source's backward scan ──

/// One step of `body_store_liveness`'s loop body, transcribed:
/// `need` = "would a store written at this position be observed by a later
/// read, or by the exit?"; the step returns `(live[i], need')` where `need'`
/// is what the loop carries to `i - 1`.
pub open spec fn live_step(st: St, need: bool) -> (bool, bool) {
    if st.reads {
        // A read inside i observes the PRECEDING store. i's own store is
        // observed only by later reads (a simple statement reads its
        // expression before writing the status) — unless a nested list reads
        // `$?` before its own first store, which can come after i's own
        // (possible) store.
        (need || (!st.stores && st.nested), true)
    } else {
        (
            need,
            if need && st.stores {
                false
            } else {
                need
            },
        )
    }
}

/// The loop, right to left. `scan(list, i, need)` is the live vector for
/// indices `0 ..= i`:
///     for i in (0..n).rev() {
///         let later_needs = need;
///         if reads[i] { live[i] = later_needs || (!stores && nested); need = true; }
///         else { live[i] = later_needs; if later_needs && stores { need = false; } }
///     }
pub open spec fn scan(list: List, i: int, need: bool) -> Seq<bool> decreases i + 1 {
    if i < 0 {
        Seq::empty()
    } else {
        let (li, nd) = live_step(list[i], need);
        scan(list, i - 1, nd).push(li)
    }
}

/// `body_store_liveness`'s result: `need` starts true (the list's last store
/// is observed by whatever runs next — the process exit, or a caller's `$?`).
pub open spec fn live(list: List) -> Seq<bool> {
    scan(list, n(list) - 1, true)
}

// ──────────────────────────────────────────────────── the closed form ────

/// Would a store written by statement `i` be observed before the next
/// definite store overwrites it?
///
///   observed_after(i) = the exit reads it            (nothing stores after i)
///                     | a later read sees it          (nothing stores between)
pub open spec fn observed_after(list: List, i: int) -> bool decreases n(list) - i {
    if i + 1 >= n(list) {
        // Nothing can overwrite it: the process exit (or the caller's `$?`)
        // reads the register.
        true
    } else if rds(list, i + 1) {
        // The next statement reads `$?` at its entry, before it stores.
        true
    } else if sts(list, i + 1) {
        // Overwritten before anything reads it.
        false
    } else {
        observed_after(list, i + 1)
    }
}

/// `live[i]` as the source intends it: i's own store is observed, or i is a
/// statement that may store and has a nested read that can follow that store.
pub open spec fn live_closed(list: List, k: int) -> bool {
    observed_after(list, k) || (rds(list, k) && !sts(list, k) && nst(list, k))
}

// ─────────────────────────────────────────────────── the run machine ─────

/// The value a `$?` read at statement `j` observes: the index of the last
/// definite store executed before it, or init() when the program has not
/// stored yet.
pub open spec fn last_def(list: List, j: int) -> int decreases j {
    if j <= 0 {
        init()
    } else if sts(list, j - 1) {
        j - 1
    } else {
        last_def(list, j - 1)
    }
}

/// Does the `$?` read performed by statement `j` observe statement `i`'s
/// write?
pub open spec fn sees(list: List, j: int, i: int) -> bool {
    rds(list, j) && (if j == i {
        // a statement that does not definitely store, whose nested list reads
        // `$?` before its own first store: such a read can come after the
        // statement's own (possible) store
        !sts(list, i) && nst(list, j)
    } else {
        // every other read observes the last definite store before it
        last_def(list, j) == i
    })
}

/// The process exit observes the value of the last definite store.
pub open spec fn exit_sees(list: List, i: int) -> bool {
    sts(list, i) && last_def(list, n(list)) == i
}

/// Is the value statement `i` writes observed by anything at all?
pub open spec fn observed(list: List, i: int) -> bool {
    (exists|j: int| 0 <= j < n(list) && sees(list, j, i)) || exit_sees(list, i)
}

// ───────────────────────────────────────────────────────────── lemmas ───

/// The scan computes the closed form: entering index `i` with
/// `need = observed_after(i)`, every entry `0 ..= i` is `live_closed`.
pub proof fn scan_agrees(list: List, i: int)
    requires
        -1 <= i < n(list),
    ensures
        scan(list, i, observed_after(list, i)).len() == i + 1,
        forall|k: int|
            #![trigger scan(list, i, observed_after(list, i))[k]]
            0 <= k <= i ==> scan(list, i, observed_after(list, i))[k] == live_closed(list, k),
    decreases i + 1,
{
    if i < 0 {
        assert(scan(list, -1, observed_after(list, -1)) == Seq::<bool>::empty());
    } else {
        let need = observed_after(list, i);
        let (li, nd) = live_step(list[i], need);
        scan_agrees(list, i - 1);
        // the loop carries `observed_after(i - 1)` to `i - 1`
        assert(nd == observed_after(list, i - 1));
        assert(scan(list, i, need) == scan(list, i - 1, nd).push(li));
        assert(scan(list, i - 1, nd)[i - 1 + 1 - 1] == scan(list, i - 1, nd)[i - 1]);
        assert(li == live_closed(list, i));
        assert forall|k: int|
            #![trigger scan(list, i, need)[k]]
            0 <= k <= i implies scan(list, i, need)[k] == live_closed(list, k)
        by {
            if k < i {
                assert(scan(list, i - 1, nd)[k] == live_closed(list, k));
            }
        }
    }
}

/// `live(list)[k]` is the closed form.
pub proof fn live_agrees(list: List, k: int)
    requires
        0 <= k < n(list),
    ensures
        live(list)[k] == live_closed(list, k),
{
    assert(observed_after(list, n(list) - 1));
    assert(live(list) == scan(list, n(list) - 1, observed_after(list, n(list) - 1)));
    scan_agrees(list, n(list) - 1);
}

/// The live vector is exactly as long as the list.
pub proof fn live_len(list: List)
    requires
        n(list) > 0,
    ensures
        live(list).len() == n(list),
{
    assert(observed_after(list, n(list) - 1));
    scan_agrees(list, n(list) - 1);
}

/// A read anchors the store before it: nothing stores between them, so the
/// store `i` writes is observed.
pub proof fn reaches_read(list: List, i: int, j: int)
    requires
        0 <= i < j < n(list),
        rds(list, j),
        forall|k: int| i < k < j ==> !sts(list, k),
    ensures
        observed_after(list, i),
    decreases j - i,
{
    if i + 1 >= n(list) {
    } else if rds(list, i + 1) {
    } else if sts(list, i + 1) {
        assert(!sts(list, i + 1)) by {
            if i + 1 < j {
            }
        }
    } else {
        reaches_read(list, i + 1, j);
    }
}

/// Nothing overwrites it before the end: the exit observes it.
pub proof fn reaches_exit(list: List, i: int)
    requires
        0 <= i < n(list),
        forall|k: int| i < k < n(list) ==> !sts(list, k),
    ensures
        observed_after(list, i),
    decreases n(list) - i,
{
    if i + 1 >= n(list) {
    } else if rds(list, i + 1) {
    } else if sts(list, i + 1) {
        assert(!sts(list, i + 1));
    } else {
        reaches_exit(list, i + 1);
    }
}

/// Bounds: `last_def` is init() or an index strictly before `j`.
pub proof fn last_def_bound(list: List, j: int)
    requires
        0 <= j <= n(list),
    ensures
        -1 <= last_def(list, j) < j || last_def(list, j) == init(),
        last_def(list, j) == init() || last_def(list, j) < j,
    decreases j,
{
    if j <= 0 {
    } else if sts(list, j - 1) {
    } else {
        last_def_bound(list, j - 1);
    }
}

/// The last definite store before `j` is a definite store, and nothing
/// between it and `j` stores.
pub proof fn last_def_window(list: List, j: int)
    requires
        0 <= j <= n(list),
    ensures
        last_def(list, j) == init() || (sts(list, last_def(list, j)) && forall|k: int|
            last_def(list, j) < k < j ==> !sts(list, k)),
    decreases j,
{
    if j <= 0 {
    } else if sts(list, j - 1) {
    } else {
        last_def_window(list, j - 1);
    }
}

// ───────────────────────────────────────────────────────── the laws ─────

/// STORE LIVENESS. A store the analysis marks dead cannot be observed: no
/// `$?` read sees it, and the process exit does not see it. This is the
/// licence the backend uses to drop the `__sh_rc = …` line.
pub proof fn dead_store_law(list: List, i: int)
    requires
        0 <= i < n(list),
        !live(list)[i],
    ensures
        !observed(list, i),
{
    live_agrees(list, i);
    assert(!observed_after(list, i));
    assert(!(rds(list, i) && !sts(list, i) && nst(list, i)));
    if observed(list, i) {
        if exit_sees(list, i) {
            last_def_window(list, n(list));
            reaches_exit(list, i);
        } else {
            let j = choose|j: int| 0 <= j < n(list) && sees(list, j, i);
            if j == i {
            } else {
                last_def_window(list, j);
                last_def_bound(list, j);
                assert(i < j);
                reaches_read(list, i, j);
            }
        }
    }
}

/// THE DROP IS INVISIBLE. Every `$?` read — and the process exit — observes a
/// store the analysis keeps (or init(), which the backend's initialiser
/// carries). So rendering with the dead stores removed reads the same
/// registers everywhere the source could tell them apart.
pub proof fn reader_sees_live_store(list: List, j: int)
    requires
        0 <= j <= n(list),
        j == n(list) || rds(list, j),
    ensures
        last_def(list, j) == init() || live(list)[last_def(list, j)],
{
    let k = last_def(list, j);
    if k != init() {
        last_def_window(list, j);
        last_def_bound(list, j);
        if j == n(list) {
            reaches_exit(list, k);
        } else {
            reaches_read(list, k, j);
        }
        assert(observed_after(list, k));
        live_agrees(list, k);
    }
}

/// With no `$?` reader anywhere, a definite store that some later statement
/// overwrites is not observed: nothing reads it, and the exit sees the later
/// store.
pub proof fn no_reader_not_observed(list: List, k: int)
    requires
        forall|m: int| 0 <= m < n(list) ==> !rds(list, m),
        0 <= k < n(list),
        exists|m: int| k < m < n(list) && sts(list, m),
    ensures
        !observed_after(list, k),
    decreases n(list) - k,
{
    if k + 1 >= n(list) {
        let m = choose|m: int| k < m < n(list) && sts(list, m);
        assert(false);
    } else if rds(list, k + 1) {
        assert(false);
    } else if sts(list, k + 1) {
        // overwritten immediately
    } else {
        no_reader_not_observed(list, k + 1);
    }
}

/// A definite store at `k` is no later than the last definite store before
/// `j`.
pub proof fn last_def_after_store(list: List, j: int, k: int)
    requires
        0 <= k < j <= n(list),
        sts(list, k),
    ensures
        last_def(list, j) >= k,
    decreases j - k,
{
    if j == k + 1 {
        assert(last_def(list, j) == k);
    } else if sts(list, j - 1) {
        assert(last_def(list, j) == j - 1);
    } else {
        last_def_after_store(list, j - 1, k);
    }
}

/// REGISTER SOUNDNESS, at the granularity the analysis works at: with no
/// `$?` reader anywhere, a store that is not the last one is dead (nothing
/// reads it, the next store overwrites it before the exit), and the last one
/// is live because the exit code is observed.
pub proof fn no_reader_keeps_only_the_exit_store(list: List, k: int)
    requires
        forall|m: int| 0 <= m < n(list) ==> !rds(list, m),
        0 <= k < n(list),
        sts(list, k),
        k != last_def(list, n(list)),
    ensures
        !live(list)[k],
        !observed(list, k),
{
    last_def_window(list, n(list));
    last_def_bound(list, n(list));
    last_def_after_store(list, n(list), k);
    // `last_def(n)` is a definite store AFTER k, so it overwrites k's value
    // before the exit can read it
    assert(exists|m: int| k < m < n(list) && sts(list, m)) by {
        let m = last_def(list, n(list));
        assert(0 <= m < n(list) && sts(list, m));
    }
    no_reader_not_observed(list, k);
    assert(!live_closed(list, k));
    live_agrees(list, k);
    dead_store_law(list, k);
}

/// A read with no live store before it observes the register's INITIAL
/// value — the fact behind `init_observed`: the backend must still emit the
/// initialiser for such a read.
pub proof fn first_read_observes_init(list: List, r: int)
    requires
        0 <= r < n(list),
        rds(list, r),
        forall|k: int| 0 <= k < r ==> !rds(list, k),
        forall|k: int| 0 <= k < r ==> !live(list)[k],
    ensures
        last_def(list, r) == init(),
{
    if last_def(list, r) != init() {
        last_def_window(list, r);
        last_def_bound(list, r);
        reaches_read(list, last_def(list, r), r);
        assert(observed_after(list, last_def(list, r)));
        live_agrees(list, last_def(list, r));
    }
}

// ───────────────────────────────────────────────────── the pins ─────────
//
// Statements: `store` = a statement that always writes the status and reads
// nothing (`echo hi`, `a=1`); `read` = a statement that reads `$?` in its own
// expression and always writes (`[[ -z "$?" ]]`, `x=$?`); `nested_read` = a
// statement that does not definitely store but whose nested list reads `$?`
// before its own first store (`redirect`, the re-entering arms).

pub open spec fn store() -> St {
    St { reads: false, stores: true, nested: false }
}

pub open spec fn read() -> St {
    St { reads: true, stores: true, nested: false }
}

pub open spec fn nested_read() -> St {
    St { reads: true, stores: false, nested: true }
}

/// `a=1; echo hi; read` → only the store the read observes survives.
/// Mirrors `stores_before_the_nearest_read_are_dropped`.
pub proof fn pin_stores_before_the_nearest_read_are_dropped()
    ensures
        live(seq![store(), store(), read()]) == seq![false, true, true],
{
    assert(live(seq![store(), store(), read()]) == seq![false, true, true]) by (compute);
}

/// `read; a=1; b=2` → nothing reads the first statement's store, so only the
/// last store (the exit code) survives.
/// Mirrors `stores_after_the_last_read_keep_only_the_exit_store`.
pub proof fn pin_stores_after_the_last_read_keep_only_the_exit_store()
    ensures
        live(seq![read(), store(), store()]) == seq![false, false, true],
{
    assert(live(seq![read(), store(), store()]) == seq![false, false, true]) by (compute);
}

/// `read; a=1; read` → the second read observes `a=1`'s store.
/// Mirrors `each_read_keeps_the_store_it_observes`.
pub proof fn pin_each_read_keeps_the_store_it_observes()
    ensures
        live(seq![read(), store(), read()]) == seq![false, true, true],
{
    assert(live(seq![read(), store(), read()]) == seq![false, true, true]) by (compute);
}

/// No read at all: only the exit code's store survives.
/// Mirrors `last_statement_store_is_live_without_a_reader`.
pub proof fn pin_last_statement_store_is_live_without_a_reader()
    ensures
        live(seq![store(), store()]) == seq![false, true],
{
    assert(live(seq![store(), store()]) == seq![false, true]) by (compute);
}

/// A bare `exit` observes the current status, so the store before it stays
/// live. Mirrors `bare_exit_reads_the_status`.
pub proof fn pin_bare_exit_reads_the_status()
    ensures
        live(seq![store(), read()]) == seq![true, true],
{
    assert(live(seq![store(), read()]) == seq![true, true]) by (compute);
}

/// A statement that does not definitely store, whose nested list reads `$?`
/// before its own first store: its own (possible) store is observed by that
/// read, so it is kept even though nothing after it reads.
pub proof fn pin_nested_after_write_keeps_the_store()
    ensures
        live(seq![store(), nested_read(), store()]) == seq![true, true, true],
{
    assert(live(seq![store(), nested_read(), store()]) == seq![true, true, true]) by (compute);
}

/// The dead store in `store; store` really is unobservable — the law's
/// instance on the smallest list.
/// Mirrors `last_statement_store_is_live_without_a_reader` (the dropped half).
pub proof fn pin_dead_store_is_unobservable()
    ensures
        !live(seq![store(), store()])[0],
        !observed(seq![store(), store()], 0),
        !sees(seq![store(), store()], 1, 0),
        !exit_sees(seq![store(), store()], 0),
{
    assert(!live(seq![store(), store()])[0]) by (compute);
    dead_store_law(seq![store(), store()], 0);
}

/// The kept store in `store; store` is what the exit observes, so
/// `sys.exit(__sh_rc)` is exact.
pub proof fn pin_exit_observes_the_kept_store()
    ensures
        exit_sees(seq![store(), store()], 1),
        live(seq![store(), store()])[1],
{
    assert(exit_sees(seq![store(), store()], 1)) by (compute);
    assert(live(seq![store(), store()])[1]) by (compute);
    reader_sees_live_store(seq![store(), store()], 2);
}

/// A read with nothing live before it reads the register's INITIAL value, so
/// the initialiser cannot be dropped.
pub proof fn pin_init_read_needs_the_initialiser()
    ensures
        last_def(seq![read(), store()], 0) == init(),
        !live(seq![read(), store()])[0],
{
    assert(last_def(seq![read(), store()], 0) == init()) by (compute);
    assert(!live(seq![read(), store()])[0]) by (compute);
    first_read_observes_init(seq![read(), store()], 0);
}

/// The empty list has no stores and no readers: the vector is empty (the
/// source returns `None` before scanning).
pub proof fn pin_empty_list()
    ensures
        live(Seq::<St>::empty()) == Seq::<bool>::empty(),
{
}

/// One statement that reads and stores: its store is the exit code, so it is
/// live.
pub proof fn pin_single_read_is_live()
    ensures
        live(seq![read()]) == seq![true],
{
    assert(live(seq![read()]) == seq![true]) by (compute);
}

fn main() {}

} // verus!
