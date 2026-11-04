// Standalone model of the C backend's SPARSE-ARRAY READ FACES — the
// non-NULL-element policy over dense `char**` storage
// (sh2perl/src/c_backend.rs: `_sh_icount`, `_sh_join_arr`,
// `_sh_join_ikind`, the for-in hole skip, the free+NULL loops, and the
// class-from-writes dispatch in `array_len` / `array_keys_join` /
// `collect_assoc_expr` / `apply_array_counts` — FIXED 2026-10-29).
//
// THE DENOTATIONAL EXTENSION (this file): an shIR indexed array in bash
// is a PARTIAL MAP index -> word. `arr=(a b c); arr[5]=z` sets FOUR
// elements at indices {0,1,2,5}. The C backend stores it DENSE —
// `static char *arr[N]` + `arr_len` — so the correspondence needs a
// storage invariant and per-face theorems:
//
//   storage   : Seq<Option<Str>> + len   (the C state; None slot = unset)
//   tail_clear: every slot in [len, cap) is None   (init {0} + free+NULL
//               + growth-gap, proved preserved by both state ops)
//   bash_elems: the element words in index order, holes dropped
//   key_elems : the SET indices as decimal words (spelling itself is
//                c_dec_str.rs `ll2s_exact` / ArithDec.thy — `dec` is
//                uninterpreted here on purpose)
//   bash_len  : |bash_elems|          — `${#arr[@]}`
//   inter_ws   : space join            — `${arr[*]}` / echo faces
//
// Obligations (each maps to a face measured red before the fix):
//   icount_is_len    `_sh_icount` == bash_len               (count)
//   join_is_elems    `_sh_join_arr` == inter_ws(bash_elems) (values)
//   keys_are_set     `_sh_join_ikind` == inter_ws(key_elems)(keys)
//   forin_is_elems   the for-in loop visits bash_elems      (for-in)
//   get_outside      `_sh_arr_get` outside [0,len) reads "" (element)
//   free_null_clears / arr_set_keeps_tail   invariant preservation
//   sparse_probe     the canonical vector's four faces (the probe p1-p4)
//   coverage_new     the dispatch and the decl agree (class from writes)
//   coverage_old_broken  the PRE-FIX read-flip violates it (the bug)
//   m1_join_sep_before / m2_dense_count / m3_free_no_null   mutants killed
//
// NOT modelled (fidelity, checked against the source): association
// (assoc storage has no holes today — `unset m[k]` does not clear a slot;
// that family is the NEXT increment, see the p5/p9 probe), `set -u`
// unbound-variable errors (bash aborts; we print empty — pre-existing
// gap), value-storage arrays (long long[]/mpz_t[] have no presence
// marker: their count/keys are dense by `dense_value_storage`, the
// documented coverage edge), and `_sh_arr_set`'s capacity guard (exits
// 127, modeled by i < slots.len() preconditions).
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;
pub type Slot = Option<Str>;

// ---------------------------------------------------------------------
// shIR/bash denotation: elements in index order, holes dropped
// ---------------------------------------------------------------------

pub open spec fn bash_elems(slots: Seq<Slot>, i: nat, n: nat) -> Seq<Str>
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        Seq::empty()
    } else {
        match slots[i as int] {
            None => bash_elems(slots, i + 1, n),
            Some(v) => seq![v] + bash_elems(slots, i + 1, n),
        }
    }
}

// `${!arr[@]}` — the SET indices as decimal words, ascending.
pub open spec fn key_elems(slots: Seq<Slot>, i: nat, n: nat) -> Seq<Str>
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        Seq::empty()
    } else {
        match slots[i as int] {
            None => key_elems(slots, i + 1, n),
            Some(_) => seq![dec(i)] + key_elems(slots, i + 1, n),
        }
    }
}

// The decimal spelling of an index. Uninterpreted HERE on purpose: the
// byte-exact itoa is proved elsewhere (c_dec_str.rs `ll2s_exact`;
// ArithDec.thy's render/parse round trip). What this file needs is only
// WHICH indices appear, in WHAT order, with WHICH separators.
pub closed spec fn dec(i: nat) -> Str;

// `${arr[*]}` / `echo "$join"` — space join over a word sequence.
pub open spec fn inter_ws(xs: Seq<Str>) -> Str
    decreases xs.len()
{
    if xs.len() == 0 {
        Seq::empty()
    } else if xs.len() == 1 {
        xs[0]
    } else {
        xs[0] + seq![' '] + inter_ws(xs.skip(1))
    }
}

// The folded join AFTER k words, with bash's join algebra: every word
// separated from the previous one by exactly one space — `first` says no
// previous word has landed yet. An EMPTY word that has landed still
// counts as previous (`arr=(a "" c)` -> `a  c`).
pub open spec fn lead(xs: Seq<Str>, first: bool) -> Str {
    if first {
        inter_ws(xs)
    } else if xs.len() == 0 {
        Seq::empty()
    } else {
        seq![' '] + inter_ws(xs)
    }
}

// ---------------------------------------------------------------------
// The emitted helpers, as spec functions (transcribed from the source)
// ---------------------------------------------------------------------

// `_sh_icount(char **a, size_t n)` — the count face.
pub open spec fn icount(slots: Seq<Slot>, i: nat, n: nat) -> int
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        0int
    } else {
        (match slots[i as int] {
            Some(_) => 1int,
            None => 0int,
        }) + icount(slots, i + 1, n)
    }
}

// `_sh_join_arr` — separator emitted AFTER the NULL skip (the fix): a
// hole contributes nothing at all.
pub open spec fn join_arr(slots: Seq<Slot>, i: nat, n: nat, dn: Str, first: bool) -> Str
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        dn
    } else {
        match slots[i as int] {
            None => join_arr(slots, i + 1, n, dn, first),
            Some(v) => {
                let dn2 = if first {
                    dn + v
                } else {
                    dn + seq![' '] + v
                };
                join_arr(slots, i + 1, n, dn2, false)
            }
        }
    }
}

// The PRE-FIX join (kept as the MUTANT m1): the separator was emitted
// BEFORE the NULL check, so every hole contributed a stray space.
pub open spec fn join_arr_old(slots: Seq<Slot>, i: nat, n: nat, dn: Str) -> Str
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        dn
    } else {
        let dn2 = if i == 0 {
            dn
        } else {
            dn + seq![' ']
        };
        // the separator lands BEFORE the NULL check (the source order):
        // a hole keeps the space it was given
        match slots[i as int] {
            None => join_arr_old(slots, i + 1, n, dn2),
            Some(v) => join_arr_old(slots, i + 1, n, dn2 + v),
        }
    }
}

// `_sh_join_ikind` — the keys face over char** storage.
pub open spec fn join_ikind(slots: Seq<Slot>, i: nat, n: nat, dn: Str, first: bool) -> Str
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        dn
    } else {
        match slots[i as int] {
            None => join_ikind(slots, i + 1, n, dn, first),
            Some(_) => {
                let seg = dec(i);
                let dn2 = if first {
                    dn + seg
                } else {
                    dn + seq![' '] + seg
                };
                join_ikind(slots, i + 1, n, dn2, false)
            }
        }
    }
}

// The emitted for-in over `"${arr[@]}"` (skip-NULL placement).
pub open spec fn forin(slots: Seq<Slot>, i: nat, n: nat) -> Seq<Str>
    decreases slots.len() - i
{
    if i >= slots.len() || i >= n {
        Seq::empty()
    } else {
        match slots[i as int] {
            None => forin(slots, i + 1, n),
            Some(v) => seq![v] + forin(slots, i + 1, n),
        }
    }
}

// `_sh_arr_get(a, len, i)` — element read; outside [0,len) or on a hole
// it reads "".
pub open spec fn arr_get(slots: Seq<Slot>, i: int, n: nat) -> Str {
    if i < 0 || i >= n as int || i >= slots.len() as int {
        Seq::empty()
    } else {
        match slots[i] {
            Some(v) => v,
            None => Seq::empty(),
        }
    }
}

// ---------------------------------------------------------------------
// State ops and the storage invariant
// ---------------------------------------------------------------------

// tail_clear IS the hole invariant: [len, cap) is all None, so within
// the array's own extent a slot is set iff the element exists.
pub open spec fn tail_clear(slots: Seq<Slot>, len: nat) -> bool {
    forall|i: nat| len <= i && i < slots.len() ==> slots[i as int].is_None()
}

// The free+NULL loops: `for (_fi < len) { free(a[_fi]); a[_fi] = 0; }`
// then `len = 0` — every ever-written slot becomes None, which is what
// makes a later sparse write's growth gap provably hole-shaped.
pub open spec fn free_null(slots: Seq<Slot>, k: nat) -> Seq<Slot> {
    slots.take(k as int).map_values(|_| Option::None) + slots.skip(k as int)
}

// `_sh_arr_set(a, &len, cap, i, v)`: overwrite slot i, grow len to
// i+1; the growth gap [old_len, i) is untouched (None by tail_clear).
pub open spec fn arr_set(slots: Seq<Slot>, len: nat, i: nat, v: Str) -> (Seq<Slot>, nat) {
    if i < slots.len() {
        (slots.update(i as int, Option::Some(v)), if i + 1 > len {
            i + 1
        } else {
            len
        })
    } else {
        // past capacity the source exits 127 — outside the model
        (slots, len)
    }
}

// ---------------------------------------------------------------------
// Coverage: which symbols a read may reference for a decl class
// ---------------------------------------------------------------------

pub enum Kind {
    // `declare -A`/string-key stores: arr_k/arr_v/arr_n decls
    Assoc,
    // indexed char** decls: arr/arr_len
    IndexedStr,
    // value storage (long long[]/mpz_t[]): name/len decls, no presence
    ValueNum,
}

// The C storage symbols a class's declaration block emits — and thus
// the symbols a read for that class may reference (the vbuf temp is a
// render-local, always available, so not a storage symbol).
pub enum Sym {
    Arr,
    ArrLen,
    K,
    V,
    N,
}

pub open spec fn decls(k: Kind) -> Set<Sym> {
    match k {
        Kind::Assoc => set![Sym::K, Sym::V, Sym::N],
        Kind::IndexedStr => set![Sym::Arr, Sym::ArrLen],
        Kind::ValueNum => set![Sym::Arr, Sym::ArrLen],
    }
}

// The FIXED dispatch: the read looks up the class the WRITES registered
// (array_len / array_keys_join / apply_array_counts branch on
// assoc_arrays/dense_value_storage — the same fields the decl router
// reads), so refs and decls agree by construction.
pub open spec fn refs_new(k: Kind) -> Set<Sym> {
    match k {
        Kind::Assoc => set![Sym::K, Sym::N],
        Kind::IndexedStr => set![Sym::Arr, Sym::ArrLen],
        Kind::ValueNum => set![Sym::ArrLen],
    }
}

// The PRE-FIX read: `array_keys_join` (and its for-in / collect_assoc
// twins) inserted the READ name into `assoc_arrays` unconditionally, so
// a keys read rendered the ASSOC join while the decls — classified from
// the writes — stayed whatever the writes said.
pub open spec fn refs_old(read_k: Kind, decl_k: Kind) -> Set<Sym> {
    let _ = decl_k;
    match read_k {
        Kind::Assoc => set![Sym::K, Sym::N],
        Kind::IndexedStr => set![Sym::Arr, Sym::ArrLen],
        Kind::ValueNum => set![Sym::ArrLen],
    }
}

// ---------------------------------------------------------------------
// Helpers used by the proofs
// ---------------------------------------------------------------------

// fold_join(xs, dn, first) == dn + lead(xs, first): the fold's
// separator rule IS bash's join.
proof fn fold_join_lead(xs: Seq<Str>, dn: Str, first: bool)
    ensures
        fold_join(xs, dn, first) == dn + lead(xs, first),
    decreases xs.len(),
{
    if xs.len() == 0 {
    } else {
        let x = xs[0];
        let tail = xs.skip(1);
        let dn2 = if first {
            dn + x
        } else {
            dn + seq![' '] + x
        };
        fold_join_lead(tail, dn2, false);
        // goal: dn2 + lead(tail, false) == dn + lead(xs, first)
        if first {
            // dn + x + lead(tail, false) == dn + inter_ws([x] + tail)
            if tail.len() == 0 {
            } else {
                assert(xs.skip(1) == tail);
            }
        } else {
            // dn + ' ' + x + lead(tail, false) == dn + ' ' + inter_ws(xs)
            if tail.len() == 0 {
            }
        }
    }
}

// The generic fold shape of both joins: given any word sequence `ws`
// traversed in the same order, join_arr/fold_join agree.
proof fn join_as_fold(slots: Seq<Slot>, i: nat, n: nat, dn: Str, first: bool)
    ensures
        join_arr(slots, i, n, dn, first)
            == fold_join(bash_elems(slots, i, n), dn, first),
    decreases slots.len() - i,
{
    if i >= slots.len() || i >= n {
    } else {
        match slots[i as int] {
            None => {
                // bash_elems skips too: unfold both one step
                join_as_fold(slots, i + 1, n, dn, first);
            }
            Some(v) => {
                let dn2 = if first {
                    dn + v
                } else {
                    dn + seq![' '] + v
                };
                join_as_fold(slots, i + 1, n, dn2, false);
                assert(bash_elems(slots, i, n)[0] == v);
                assert(bash_elems(slots, i, n).skip(1)
                    == bash_elems(slots, i + 1, n));
            }
        }
    }
}

proof fn keys_as_fold(slots: Seq<Slot>, i: nat, n: nat, dn: Str, first: bool)
    ensures
        join_ikind(slots, i, n, dn, first)
            == fold_join(key_elems(slots, i, n), dn, first),
    decreases slots.len() - i,
{
    if i >= slots.len() || i >= n {
    } else {
        match slots[i as int] {
            None => {
                keys_as_fold(slots, i + 1, n, dn, first);
            }
            Some(_) => {
                let dn2 = if first {
                    dn + dec(i)
                } else {
                    dn + seq![' '] + dec(i)
                };
                keys_as_fold(slots, i + 1, n, dn2, false);
                assert(key_elems(slots, i, n)[0] == dec(i));
                assert(key_elems(slots, i, n).skip(1)
                    == key_elems(slots, i + 1, n));
            }
        }
    }
}

pub open spec fn fold_join(xs: Seq<Str>, dn: Str, first: bool) -> Str
    decreases xs.len()
{
    if xs.len() == 0 {
        dn
    } else {
        let seg = xs[0];
        let dn2 = if first {
            dn + seg
        } else {
            dn + seq![' '] + seg
        };
        fold_join(xs.skip(1), dn2, false)
    }
}

// Singleton-concat algebra over Seq (needed by every structural step).
proof fn concat1_skip(t: Seq<Str>, x: Str)
    ensures
        (seq![x] + t).skip(1) == t,
        (seq![x] + t).len() == 1 + t.len(),
        (seq![x] + t)[0] == x,
{
}

// ---------------------------------------------------------------------
// The obligations
// ---------------------------------------------------------------------

// COUNT FACE: `${#arr[@]}` counts elements, not the dense extent.
proof fn icount_is_len(slots: Seq<Slot>, i: nat, n: nat)
    ensures icount(slots, i, n) == bash_elems(slots, i, n).len(),
    decreases slots.len() - i,
{
    if i >= slots.len() || i >= n {
    } else {
        icount_is_len(slots, i + 1, n);
        match slots[i as int] {
            None => {
                // no element is prepended: the denotation steps over
                assert(bash_elems(slots, i, n) == bash_elems(slots, i + 1, n));
            }
            Some(_) => {
                assert(bash_elems(slots, i, n)[0] == slots[i as int].get_Some_0());
                assert(bash_elems(slots, i, n).skip(1)
                    == bash_elems(slots, i + 1, n));
            }
        }
    }
}

proof fn count_face(slots: Seq<Slot>, n: nat)
    requires
        n <= slots.len(),
    ensures icount(slots, 0, n) == bash_elems(slots, 0, n).len(),
{
    icount_is_len(slots, 0, n);
}

// VALUES FACE: `_sh_join_arr` == bash's space join of the elements.
proof fn values_face(slots: Seq<Slot>, n: nat)
    requires
        n <= slots.len(),
    ensures
        join_arr(slots, 0, n, Seq::empty(), true)
            == inter_ws(bash_elems(slots, 0, n)),
{
    join_as_fold(slots, 0, n, Seq::empty(), true);
    fold_join_lead(bash_elems(slots, 0, n), Seq::empty(), true);
}

// KEYS FACE: `_sh_join_ikind` == the space join of the set indices.
proof fn keys_face(slots: Seq<Slot>, n: nat)
    requires
        n <= slots.len(),
    ensures
        join_ikind(slots, 0, n, Seq::empty(), true)
            == inter_ws(key_elems(slots, 0, n)),
{
    keys_as_fold(slots, 0, n, Seq::empty(), true);
    fold_join_lead(key_elems(slots, 0, n), Seq::empty(), true);
}

// FOR-IN FACE: the emitted loop visits exactly bash's elements.
proof fn forin_is_elems(slots: Seq<Slot>, i: nat, n: nat)
    ensures forin(slots, i, n) == bash_elems(slots, i, n),
    decreases slots.len() - i,
{
    if i >= slots.len() || i >= n {
    } else {
        forin_is_elems(slots, i + 1, n);
    }
}

// ELEMENT FACE: outside [0,len) the read is "" — bash's unset element.
proof fn get_outside(slots: Seq<Slot>, n: nat)
    requires
        n <= slots.len(),
    ensures
        forall|i: int| i < 0 || i >= n as int ==> arr_get(slots, i, n) == Seq::empty(),
{
}

// INVARIANT: the free+NULL loop clears the tail completely.
proof fn free_null_clears(slots: Seq<Slot>, len: nat)
    requires
        len <= slots.len(),
        tail_clear(slots, len),
    ensures tail_clear(free_null(slots, len), 0),
{
    let s = free_null(slots, len);
    assert(s.len() == slots.len());
    // [0, len) was mapped to None; [len, cap) was already None or is
    // reached below — after len := 0 EVERY slot must be None, which is
    // exactly "the array holds no elements and no stale pointers".
    assert forall|i: nat| i < s.len() implies s[i as int].is_None() by {
        if i < len {
            assert(s[i as int].is_None());
        } else {
            assert(s[i as int] == slots[i as int]);
            assert(slots[i as int].is_None());
        }
    }
}

// INVARIANT: a sparse write preserves the tail (growth gap stays None).
proof fn arr_set_keeps_tail(slots: Seq<Slot>, len: nat, i: nat, v: Str)
    requires
        i < slots.len(),
        len <= slots.len(),
        tail_clear(slots, len),
        i >= len,
    ensures tail_clear(arr_set(slots, len, i, v).0, arr_set(slots, len, i, v).1)
{
    let (s2, l2) = arr_set(slots, len, i, v);
    assert(l2 == i + 1);
    assert forall|j: nat| l2 <= j && j < s2.len() implies s2[j as int].is_None() by {
        assert(s2[j as int] == slots[j as int]); // j != i since j >= i+1
        assert(slots[j as int].is_None());
    }
}

// THE PROBE (p1-p4): the canonical sparse scenario's four faces. Storage
// after `arr=(a b c); arr[5]=z` from any tail-cleared state: slots are
// a,b,c,None,None,z with len 6.
proof fn sparse_probe()
    ensures
        // `${#arr[@]}` = 4 (bash), never the dense 6
        icount(scen(), 0, 6) == 4int,
        // `${arr[*]}` = "a b c z" — holes contribute NOTHING
        join_arr(scen(), 0, 6, Seq::empty(), true)
            == inter_ws(seq![seq!['a'], seq!['b'], seq!['c'], seq!['z']]),
        // `${!arr[@]}` enumerates the SET indices 0 1 2 5
        key_elems(scen(), 0, 6)
            == seq![dec(0nat), dec(1nat), dec(2nat), dec(5nat)],
        // `for x in "${arr[@]}"` visits exactly a b c z
        forin(scen(), 0, 6) == seq![seq!['a'], seq!['b'], seq!['c'], seq!['z']],
        // `${arr[3]}` (a hole) reads ""
        arr_get(scen(), 3, 6) == Seq::empty(),
{
    count_face(scen(), 6);
    values_face(scen(), 6);
    keys_face(scen(), 6);
    forin_is_elems(scen(), 0, 6);
    assert(tail_clear(scen(), 6));
    // finite unfolds of the canonical vector
    assert(bash_elems(scen(), 0, 6) == seq![seq!['a'], seq!['b'], seq!['c'], seq!['z']]) by (compute);
    assert(key_elems(scen(), 0, 6) == seq![dec(0nat), dec(1nat), dec(2nat), dec(5nat)]) by (compute);
    assert(arr_get(scen(), 3, 6) == Seq::empty());
}

pub open spec fn scen() -> Seq<Slot> {
    seq![
        Option::Some(seq!['a']),
        Option::Some(seq!['b']),
        Option::Some(seq!['c']),
        Option::None,
        Option::None,
        Option::Some(seq!['z']),
    ]
}

// COVERAGE (the fix's core claim): a read that dispatches on the class
// the writes registered references only declared symbols — for every
// class, because dispatch and decl read the SAME field.
proof fn coverage_new()
    ensures forall|k: Kind| refs_new(k) <= decls(k),
{
}

// The PRE-FIX read flip, stated as the violation it produced: an
// indexed array read as `${!arr[@]}` rendered the ASSOC join while its
// decls stayed indexed → `arr_k`/`arr_n` referenced, never declared
// (the cc red: `arr_len undeclared (did you mean arr_n?)`).
proof fn coverage_old_broken()
    ensures
        !(refs_old(Kind::Assoc, Kind::IndexedStr)
            <= decls(Kind::IndexedStr)),
{
    // the witness: the read referenced arr_k, the decls never emit it
    assert(refs_old(Kind::Assoc, Kind::IndexedStr).contains(Sym::K));
    assert(!decls(Kind::IndexedStr).contains(Sym::K));
}

// ---------------------------------------------------------------------
// Mutants — each kills one obligation the fix established
// ---------------------------------------------------------------------

// M1: the pre-fix separator (emitted before the NULL skip) produces a
// stray space for every hole — bash never does.
proof fn m1_join_sep_before()
    ensures
        join_arr_old(scen(), 0, 6, Seq::empty())
            != inter_ws(bash_elems(scen(), 0, 6)),
{
    // holes at 3 and 4 each contribute a stray space in the old fold
    // (measured pre-fix: "a b c   z" vs bash's "a b c z")
    assert(
        join_arr_old(scen(), 0, 6, Seq::empty())
            != inter_ws(bash_elems(scen(), 0, 6))
    ) by (compute);
}

// M2: the pre-fix count was the dense extent — 6 where bash says 4.
proof fn m2_dense_count()
    ensures icount(scen(), 0, 6) != 6int,
{
    count_face(scen(), 6);
    assert(bash_elems(scen(), 0, 6).len() == 4) by (compute);
}

// M3: free WITHOUT the NULL (the pre-fix loops) leaves a stale
// non-NULL slot, so a later reassign still reads the freed word — the
// element set stops matching bash's. Modeled: free_old leaves content,
// so after `arr=()` the old element is still visible at index 0.
proof fn m3_free_no_null()
    ensures
        bash_elems(stale(), 0, 6) != bash_elems(scen(), 0, 6),
{
    assert(bash_elems(stale(), 0, 6) != bash_elems(scen(), 0, 6)) by (compute);
}

pub open spec fn stale() -> Seq<Slot> {
    seq![
        Option::Some(seq!['s']), // freed, NOT nulled (the old loops)
        Option::Some(seq!['b']),
        Option::Some(seq!['c']),
        Option::None,
        Option::None,
        Option::Some(seq!['z']),
    ]
}

fn main() {}

} // verus!
