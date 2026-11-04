// Standalone model of the C backend's ASSOCIATIVE-ARRAY faces — the
// k/v/n table protocol over `char **k, char **v, size_t n`
// (sh2perl/src/c_backend.rs: `_sh_assoc_set`, `_sh_assoc_get`,
//  `_sh_assoc_unset`, the value join `_sh_join_arr(&t, id_v, id_n)`,
//  the key join `_sh_join_keys(&t, id_k, id_n)`, the `{id}_n` count arm
//  of `array_len`, and the `m[k]+=v` read-modify-write through the
//  `assign` arm — all measured red before 2026-10-30's fixes).
//
// THE DENOTATIONAL EXTENSION (this file): a bash associative array is a
// finite MAP key -> value where every key is a STRING — `declare -A m;
// m[0]=x` stores the key "0", and `m[i]=v` stores the key "i" (a bare
// name in an assoc subscript is NOT an arithmetic index). The C backend
// stores it as two parallel DENSE arrays plus a count, so the
// correspondence needs the wf invariant and per-face theorems:
//
//   storage  : (Seq<Str> keys, Seq<Str> vals), parallel and dense
//   wf       : equal length AND no duplicate keys  (init {0} + the
//              xstrdup-insert path + first-match replace preserve it)
//   the_map  : the denotation — a Map obtained by the forward scan
//   count    : keys.len()          — `${#m[@]}`   (arm `{id}_n`)
//   join_vals: space join of vals  — `${m[@]}`/`${m[*]}` (`_sh_join_arr`)
//   join_keys: space join of keys  — `${!m[@]}`    (`_sh_join_keys`)
//
// Obligations (each maps to a face measured red before the fix):
//   get_sound        `_sh_assoc_get` == map lookup, "" off the map
//   set_sound        `_sh_assoc_set` == map insert (replace or append)
//   unset_sound      `_sh_assoc_unset` == map remove, survivors in order
//   count_face       keys.len() == bash's distinct-key count
//   keys_set_face    the key SET the join enumerates == map domain
//   join_vals_sound  the emitted fold == the space join (the fix:
//                    `_sh_join_arr_nl`'s newline separator is retired)
//   rmw_face         `m[k]+=v` stores map[k] ++ v (c4/d6)
//   int_attr_face    `declare -i n; n+=3` routes to the arithmetic arm
//                    (measured: 8 under the fix, 53 without — c6)
//   veto_covers_param  the homing veto fires on a `param("slice",…,"@")`
//                    whole-array join (a6: long long[] vs char** SIGSEGV)
//   probe_unset_*    the measured b1/b2/b3 scenarios, set-level
//   m1_nl_join / m2_unset_noop / m3_int_attr_concat / m4_veto_miss
//                    mutants, each killing one obligation
//
// ORDER, THE DOCUMENTED EDGE: bash iterates its OWN hash table, so
// bash's `${m[@]}`/`${!m[@]}` spell the SAME words in an order that is
// not our insertion order. The string-level theorems therefore pin OUR
// spelling (`inter_ws` over our seq), the SET-level theorems pin the
// bash agreement (keys_set_face, count_face, get_sound), and the
// byte-level divergence is recorded as the probe's SORTONLY verdict
// (order-only, content equal) rather than hidden. Relating the two
// spellings as multisets would need a split/join round-trip lemma over
// concatenated word sequences; it is deliberately NOT claimed here.
//
// NOT modelled (fidelity, checked against the source): the capacity
// guard (`*n < cap`, else exit 127 — outside the model like
// `_sh_arr_set`'s), string VALUE ownership (the xstrdup/free pairing is
// an ownership property, not a denotational one), and the integer
// RENDERING of the int-attr arm (byte-exact itoa is c_dec_str.rs; this
// file models that arm at the INTEGER level only).
use vstd::prelude::*;

verus! {

// vstd's broadcast lemma groups are NOT in scope by default: `prelude`
// imports names but the axiom groups (seq push/update/subrange laws,
// map insert/remove domain, set len/ext) must be opted into. Without
// them the remove reasoning and the seq-length bridges below fail
// outright (measured: `m.remove(k).get(x)` and `vs.update(0,v).len()`
// are opaque).
broadcast use vstd::seq::group_seq_axioms,
    vstd::seq_lib::group_seq_lib_default,
    vstd::map::group_map_lemmas,
    vstd::set::group_set_lemmas,
    vstd::iset::group_iset_lemmas,
    vstd::set_lib::group_set_lib_default;

pub type Str = Seq<char>;

// ---------------------------------------------------------------------
// Helpers over the parallel key/value arrays
// ---------------------------------------------------------------------

// A state whose keys are pairwise distinct — the wf of every reachable
// table (the insert path only appends a key it did not find, and the
// first-match replace never adds).
pub open spec fn distinct(xs: Seq<Str>) -> bool {
    forall|i: int, j: int|
        0 <= i < xs.len() && 0 <= j < xs.len() && i != j ==> xs[i] != xs[j]
}

pub open spec fn wf(ks: Seq<Str>, vs: Seq<Str>) -> bool {
    ks.len() == vs.len() && distinct(ks)
}

// The key SET of a sequence (front-recursive, like every walker below).
pub open spec fn sset(xs: Seq<Str>) -> Set<Str>
    decreases xs.len()
{
    if xs.len() == 0 {
        set![]
    } else {
        sset(xs.skip(1)).insert(xs[0])
    }
}

// Bash's `${#m[@]}`: the number of DISTINCT keys — which for a wf table
// is exactly the length. (Defined by distinct-recursion so the count
// theorem never needs Set cardinality axioms.)
pub open spec fn distinct_len(xs: Seq<Str>) -> nat
    decreases xs.len()
{
    if xs.len() == 0 {
        0nat
    } else if sset(xs.skip(1)).contains(xs[0]) {
        distinct_len(xs.skip(1))
    } else {
        1 + distinct_len(xs.skip(1))
    }
}

// ---------------------------------------------------------------------
// The emitted helpers, transcribed as spec functions
// ---------------------------------------------------------------------

// `_sh_assoc_get(k, v, n, key)` — forward scan, first match, "" for a
// missing key (and for a NULL value slot: `v[i] ? v[i] : ""`).
pub open spec fn assoc_get(ks: Seq<Str>, vs: Seq<Str>, k: Str) -> Str
    decreases ks.len()
{
    if ks.len() == 0 || vs.len() == 0 {
        Seq::empty()
    } else if ks[0] == k {
        vs[0]
    } else {
        assoc_get(ks.skip(1), vs.skip(1), k)
    }
}

// `_sh_assoc_set(k, v, &n, cap, key, val)` — the C loop: replace AT THE
// FIRST MATCHING SLOT, else append (the `*n < cap` guard excluded here).
// Transcribed recursively so the scan order IS the spec: only the empty
// base appends, every other frame reconstructs the head.
pub open spec fn assoc_set(
    ks: Seq<Str>,
    vs: Seq<Str>,
    k: Str,
    v: Str,
) -> (Seq<Str>, Seq<Str>)
    decreases ks.len()
{
    if ks.len() == 0 || vs.len() == 0 {
        (ks.push(k), vs.push(v))
    } else if ks[0] == k {
        (ks, vs.update(0int, v))
    } else {
        let (rks, rvs) = assoc_set(ks.skip(1), vs.skip(1), k, v);
        (seq![ks[0]] + rks, seq![vs[0]] + rvs)
    }
}

// `_sh_assoc_unset(k, &v, &n, key)` (FIXED 2026-10-30) — drop the FIRST
// entry with the key, compacting: the head frame reconstructs every
// survivor, so the order of the survivors is preserved by construction
// (exactly the two entries a bash `unset` removes from its own lists).
pub open spec fn assoc_unset(
    ks: Seq<Str>,
    vs: Seq<Str>,
    k: Str,
) -> (Seq<Str>, Seq<Str>)
    decreases ks.len()
{
    if ks.len() == 0 || vs.len() == 0 {
        (ks, vs)
    } else if ks[0] == k {
        (ks.skip(1), vs.skip(1))
    } else {
        let (rks, rvs) = assoc_unset(ks.skip(1), vs.skip(1), k);
        (seq![ks[0]] + rks, seq![vs[0]] + rvs)
    }
}

// The value join the backend EMITS now: `_sh_join_arr` over `{id}_v` —
// the C loop with the separator BETWEEN present elements (an assoc
// values table has no holes, so every element presents).
pub open spec fn join_fold(xs: Seq<Str>, first: bool) -> Str
    decreases xs.len()
{
    if xs.len() == 0 {
        Seq::empty()
    } else if first {
        xs[0] + join_fold(xs.skip(1), false)
    } else {
        seq![' '] + xs[0] + join_fold(xs.skip(1), false)
    }
}

// bash's join: one space between consecutive words.
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

// The RETIRED newline join (`_sh_join_arr_nl`, d40cbd454): values one
// per line — faked argv-splitting for `printf '%s\n' "${m[@]}" | sort`
// but diverged from bash in EVERY plain string context (probe a1/a2/a4:
// `echo "[${m[@]}]"` printed one value per line). Kept as the M1
// mutant's transcription.
pub open spec fn inter_nl(xs: Seq<Str>) -> Str
    decreases xs.len()
{
    if xs.len() == 0 {
        Seq::empty()
    } else if xs.len() == 1 {
        xs[0]
    } else {
        xs[0] + seq!['\n'] + inter_nl(xs.skip(1))
    }
}

// ---------------------------------------------------------------------
// The denotation: the parallel arrays as a bash finite map
// ---------------------------------------------------------------------

pub open spec fn the_map(ks: Seq<Str>, vs: Seq<Str>) -> Map<Str, Str>
    decreases ks.len()
{
    if ks.len() == 0 || vs.len() == 0 {
        Map::empty()
    } else {
        the_map(ks.skip(1), vs.skip(1)).insert(ks[0], vs[0])
    }
}

// ---------------------------------------------------------------------
// Mechanical lemmas over front-recursive sequences
// (every op below peels the head, so these are the only shape facts
//  the proofs need — each is a pointwise extensionality argument.)
// ---------------------------------------------------------------------

// update(0) is invisible to skip(1).
proof fn skip_update0(vs: Seq<Str>, v: Str)
    requires
        // every caller sits under `wf` with a nonempty table; the
        // length law below is false at vs.len()==0 (skip(1) of the
        // empty seq is empty, but vs.len()-1 would be -1)
        vs.len() > 0,
    ensures vs.update(0int, v).skip(1) == vs.skip(1),
{
    // the length bridge needs the seq length laws spelled out (the
    // subgroup axioms fire, but the goal needs the intermediate facts)
    assert(vs.update(0int, v).len() == vs.len());
    assert(vs.skip(1).len() == vs.len() - 1);
    let lhs = vs.update(0int, v).skip(1);
    let rhs = vs.skip(1);
    assert(lhs.len() == rhs.len());
    assert forall|j: int| 0 <= j < lhs.len() implies lhs[j] == rhs[j] by {
        // both sides read vs[j + 1]; the update touched only index 0
        assert(vs.update(0int, v)[j + 1] == vs[j + 1]);
        assert(lhs[j] == vs[j + 1]);
        assert(rhs[j] == vs[j + 1]);
    }
}

// insert commutes with a DIFFERENT key.
proof fn insert_commute(m: Map<Str, Str>, a: Str, b: Str, c: Str, d: Str)
    requires a != c,
    ensures m.insert(a, b).insert(c, d) == m.insert(c, d).insert(a, b),
{
    let lhs = m.insert(a, b).insert(c, d);
    let rhs = m.insert(c, d).insert(a, b);
    assert forall|k: Str| lhs.get(k) == rhs.get(k) by {
        if k == c {
            assert(lhs.get(k) == Some(d));
            assert(rhs.get(k) == Some(d));
        } else if k == a {
            assert(lhs.get(k) == Some(b));
            // rhs: k != c, k == a -> m[a] ... but a != c already, and
            // on the right the (c,d) insert is invisible at k
            assert(rhs.get(k) == Some(b));
        } else {
            assert(lhs.get(k) == m.get(k));
            assert(rhs.get(k) == m.get(k));
        }
    }
    assert(lhs == rhs);
}

// A key inserted twice keeps only the LAST value (set_sound's ks[0]==k
// frame folds two same-key inserts).
proof fn insert_same(m: Map<Str, Str>, k: Str, v: Str, w: Str)
    ensures m.insert(k, v).insert(k, w) == m.insert(k, w),
{
    let lhs = m.insert(k, v).insert(k, w);
    let rhs = m.insert(k, w);
    assert forall|x: Str| lhs.get(x) == rhs.get(x) by {
        if x == k {
            assert(lhs.get(x) == Some(w));
            assert(rhs.get(x) == Some(w));
        } else {
            assert(lhs.get(x) == m.get(x));
            assert(rhs.get(x) == m.get(x));
        }
    }
    assert(lhs == rhs);
}

// Removing a key BEFORE inserting a different one commutes.
proof fn insert_remove_commute(
    m: Map<Str, Str>,
    k: Str,
    a: Str,
    b: Str,
)
    requires k != a,
    ensures m.insert(a, b).remove(k) == m.remove(k).insert(a, b),
{
    let lhs = m.insert(a, b).remove(k);
    let rhs = m.remove(k).insert(a, b);
    assert forall|x: Str| lhs.get(x) == rhs.get(x) by {
        if x == a {
            // insert then remove a DIFFERENT key: at x = a the
            // inserted entry survives (the old middle assertion
            // compared against m.get(a), which m never promised)
            assert(lhs.get(x) == Some(b));
            assert(rhs.get(x) == Some(b));
        } else {
            assert(lhs.get(x) == if x == k { None } else { m.get(x) });
            assert(rhs.get(x) == if x == k { None } else { m.get(x) });
        }
    }
    assert(lhs == rhs);
}

// ---------------------------------------------------------------------
// Set-level bridges — the wf/distinct side of the mutation theorems
// ---------------------------------------------------------------------

// insert order does not matter for the resulting SET (set_sound's else
// frame reconstructs head around a tail that already gained k).
proof fn set_insert_commute(s: Set<Str>, a: Str, b: Str)
    ensures s.insert(a).insert(b) == s.insert(b).insert(a),
{
    assert forall|x: Str| s.insert(a).insert(b).contains(x)
        == s.insert(b).insert(a).contains(x) by {
        if x == a || x == b {
            // both sides true by insert_same
        } else {
            // both sides reduce to s.contains(x) by insert_different
        }
    }
}

// every element of a sequence sits in its key set (the membership
// direction `sset_index` does not give).
proof fn sset_member(xs: Seq<Str>, i: int)
    requires 0 <= i < xs.len(),
    ensures sset(xs).contains(xs[i]),
    decreases xs.len(),
{
    if xs.len() == 0 {
    } else if i == 0 {
        // sset(xs) = sset(tail).insert(xs[0]); contains by insert_same
        assert(xs[0] == xs[i]);
    } else {
        sset_member(xs.skip(1), i - 1);
        if xs[i] == xs[0] {
            // insert_same on the outer insert
        } else {
            // insert_different reduces to the tail membership
        }
    }
}

// cons onto a distinct tail whose SET misses the head keeps distinctness.
proof fn distinct_cons(a: Str, xs: Seq<Str>)
    requires
        distinct(xs),
        !sset(xs).contains(a),
    ensures distinct(seq![a] + xs),
{
    let ys = seq![a] + xs;
    assert(ys.len() == xs.len() + 1);
    assert forall|i: int, j: int|
        0 <= i < ys.len() && 0 <= j < ys.len() && i != j
        implies ys[i] != ys[j] by {
        if i == 0 {
            assert(ys[0] == a);
            assert(ys[j] == xs[j - 1]);
            sset_member(xs, j - 1);
            assert(sset(xs).contains(xs[j - 1]));
            // a is missing from the set, so it differs from every member
        } else if j == 0 {
            assert(ys[j] == a);
            assert(ys[i] == xs[i - 1]);
            sset_member(xs, i - 1);
            assert(sset(xs).contains(xs[i - 1]));
        } else {
            assert(ys[i] == xs[i - 1]);
            assert(ys[j] == xs[j - 1]);
            // distinct(xs) at the shifted indices
        }
    }
}

// insert-then-remove of a key the map NEVER held is the identity —
// what makes unset's head frame sound (the tail cannot hold k under
// the wf distinctness).
proof fn insert_remove_fresh(m: Map<Str, Str>, k: Str, v: Str)
    requires !m.dom().contains(k),
    ensures m.insert(k, v).remove(k) == m,
{
    assert forall|x: Str| (m.insert(k, v).remove(k)).get(x) == m.get(x) by {
        if x == k {
            // lhs removes it -> None; m never had it -> get is None
        } else {
            // both sides reduce to m.get(x) (insert/remove at a
            // different key are invisible)
        }
    }
}

// remove-then-insert at a DIFFERENT key commutes (unset's else frame).
proof fn set_remove_insert_commute(s: Set<Str>, a: Str, b: Str)
    requires a != b,
    ensures s.remove(a).insert(b) == s.insert(b).remove(a),
{
    assert forall|x: Str| s.remove(a).insert(b).contains(x)
        == s.insert(b).remove(a).contains(x) by {
        if x == b {
            // lhs insert_same; rhs: remove(a) sees b != a, keeps it
        } else if x == a {
            // both sides drop a (remove_same / remove_different chain)
        } else {
            // both sides reduce to s.contains(x)
        }
    }
}

// ---------------------------------------------------------------------
// T1 — get: the forward scan IS the map lookup, "" off the map
// ---------------------------------------------------------------------

proof fn get_sound(ks: Seq<Str>, vs: Seq<Str>, k: Str)
    requires wf(ks, vs),
    ensures
        the_map(ks, vs).dom().contains(k) ==> the_map(ks, vs)[k]
            == assoc_get(ks, vs, k),
        !the_map(ks, vs).dom().contains(k) ==> assoc_get(ks, vs, k)
            == Seq::empty(),
    decreases ks.len(),
{
    if ks.len() == 0 || vs.len() == 0 {
        assert(!the_map(ks, vs).dom().contains(k));
        assert(assoc_get(ks, vs, k) == Seq::empty());
    } else if ks[0] == k {
        // the head insert is a hit at k
        assert(the_map(ks, vs).dom().contains(k));
        assert(the_map(ks, vs)[k] == vs[0]);
        assert(assoc_get(ks, vs, k) == vs[0]);
    } else {
        skip_update0(vs, Seq::empty()); // no-op keeps frame shape visible
        assert(the_map(ks, vs) == the_map(ks.skip(1), vs.skip(1)).insert(ks[0], vs[0]));
        assert(assoc_get(ks, vs, k) == assoc_get(ks.skip(1), vs.skip(1), k));
        // wf closed under the head peel
        assert(wf(ks.skip(1), vs.skip(1)));
        get_sound(ks.skip(1), vs.skip(1), k);
        if the_map(ks.skip(1), vs.skip(1)).dom().contains(k) {
            assert(the_map(ks, vs).dom().contains(k));
            assert(the_map(ks, vs)[k] == the_map(ks.skip(1), vs.skip(1))[k]);
        } else {
            assert(!the_map(ks, vs).dom().contains(k));
        }
    }
}

// ---------------------------------------------------------------------
// T2 — set: replace-at-first-match / append IS map insert
// ---------------------------------------------------------------------

proof fn set_sound(ks: Seq<Str>, vs: Seq<Str>, k: Str, v: Str)
    requires wf(ks, vs),
    ensures
        the_map(assoc_set(ks, vs, k, v).0, assoc_set(ks, vs, k, v).1)
            == the_map(ks, vs).insert(k, v),
        wf(assoc_set(ks, vs, k, v).0, assoc_set(ks, vs, k, v).1),
        // the KEY SET only gained k — what makes the else frame's
        // distinctness restoration provable (head absent from rks)
        sset(assoc_set(ks, vs, k, v).0) == sset(ks).insert(k),
    decreases ks.len(),
{
    if ks.len() == 0 || vs.len() == 0 {
        // base: both empty under wf, so the push makes the singleton
        assert(ks.len() == 0 && vs.len() == 0);
        let (rks, rvs) = assoc_set(ks, vs, k, v);
        assert(rks == seq![k] && rvs == seq![v]);
        // stepping stones: seq![k] is push(empty), so skip(1)/[0] land
        // exactly where the the_map/sset unfolds need them
        assert(seq![k].skip(1) == Seq::empty() && seq![v].skip(1) == Seq::empty());
        assert(seq![k][0] == k && seq![v][0] == v);
        assert(sset(Seq::empty()) == set![]);
        assert(sset(ks) == set![]);
        assert(the_map(seq![k], seq![v])
            == the_map(Seq::empty(), Seq::empty()).insert(k, v));
        assert(the_map(Seq::empty(), Seq::empty()) == Map::empty());
        assert(the_map(rks, rvs) == Map::empty().insert(k, v));
        assert(the_map(ks, vs).insert(k, v) == Map::empty().insert(k, v));
        assert(sset(seq![k]) == sset(Seq::empty()).insert(k));
    } else if ks[0] == k {
        skip_update0(vs, v);
        let (rks, rvs) = assoc_set(ks, vs, k, v);
        assert(rks == ks && rvs == vs.update(0int, v));
        assert(the_map(rks, rvs) == the_map(ks.skip(1), vs.skip(1)).insert(k, v));
        assert(the_map(ks, vs) == the_map(ks.skip(1), vs.skip(1)).insert(k, vs[0]));
        insert_same(the_map(ks.skip(1), vs.skip(1)), k, vs[0], v);
        // wf: distinct preserved (the key set is unchanged)
        assert(distinct(ks));
        assert(wf(rks, rvs));
        // the key set: k is already a member, so insert absorbs it
        sset_member(ks, 0);
        assert(sset(ks).contains(k));
        assert forall|x: Str| sset(ks).insert(k).contains(x) == sset(ks).contains(x) by {
            if x == k {
            } else {
            }
        }
        assert(sset(ks) == sset(ks).insert(k));
    } else {
        skip_update0(vs, Seq::empty());
        let (rks, rvs) = assoc_set(ks.skip(1), vs.skip(1), k, v);
        assert(wf(ks.skip(1), vs.skip(1)));
        set_sound(ks.skip(1), vs.skip(1), k, v);
        assert(the_map(rks, rvs) == the_map(ks.skip(1), vs.skip(1)).insert(k, v));
        // reconstruct: the_map(cons) = the_map(rest).insert(head) —
        // the concat unfolds need the skip/index facts spelled out
        assert((seq![ks[0]] + rks).skip(1) == rks
            && (seq![vs[0]] + rvs).skip(1) == rvs);
        assert((seq![ks[0]] + rks)[0] == ks[0]
            && (seq![vs[0]] + rvs)[0] == vs[0]);
        assert(the_map(seq![ks[0]] + rks, seq![vs[0]] + rvs)
            == the_map(rks, rvs).insert(ks[0], vs[0]));
        assert(the_map(ks, vs)
            == the_map(ks.skip(1), vs.skip(1)).insert(ks[0], vs[0]));
        insert_commute(the_map(ks.skip(1), vs.skip(1)), ks[0], vs[0], k, v);
        assert(the_map(rks, rvs).insert(ks[0], vs[0])
            == the_map(ks, vs).insert(k, v));
        // the key set: cons(head) of the IH set, in either insert order
        assert(sset(rks) == sset(ks.skip(1)).insert(k));
        assert(sset(seq![ks[0]] + rks) == sset(rks).insert(ks[0]));
        assert(sset(ks) == sset(ks.skip(1)).insert(ks[0]));
        set_insert_commute(sset(ks.skip(1)), ks[0], k);
        // wf: the head is absent from the tail's set and from rks's
        assert(ks[0] != k);
        assert(!sset(ks.skip(1)).contains(ks[0])) by {
            if sset(ks.skip(1)).contains(ks[0]) {
                tail_ne_head(ks, ks[0]);
            }
        }
        assert(!sset(rks).contains(ks[0]));
        distinct_cons(ks[0], rks);
        assert(wf(seq![ks[0]] + rks, seq![vs[0]] + rvs));
    }
}

// ---------------------------------------------------------------------
// T3 — unset: first-match drop IS map remove, survivors in order
// ---------------------------------------------------------------------

proof fn unset_sound(ks: Seq<Str>, vs: Seq<Str>, k: Str)
    requires wf(ks, vs),
    ensures
        the_map(assoc_unset(ks, vs, k).0, assoc_unset(ks, vs, k).1)
            == the_map(ks, vs).remove(k),
        wf(assoc_unset(ks, vs, k).0, assoc_unset(ks, vs, k).1),
        // survivors keep their RELATIVE order (the compaction shifts
        // left — this is the string-level content of the fix)
        assoc_unset(ks, vs, k).0.len() == ks.len()
            - (if sset(ks).contains(k) { 1int } else { 0int }),
        // the KEY SET lost exactly k — the wf/distinct bridge for the
        // else frame's head restoration
        sset(assoc_unset(ks, vs, k).0) == sset(ks).remove(k),
    decreases ks.len(),
{
    if ks.len() == 0 || vs.len() == 0 {
        let (rks, rvs) = assoc_unset(ks, vs, k);
        assert(rks == ks && rvs == vs);
        assert(the_map(rks, rvs) == the_map(ks, vs).remove(k));
        assert(wf(rks, rvs));
        assert(rks.len() == ks.len());
        assert(sset(ks) == set![]);
        assert forall|x: Str| sset(ks).remove(k).contains(x) == sset(ks).contains(x) by {
        }
    } else if ks[0] == k {
        let (rks, rvs) = assoc_unset(ks, vs, k);
        assert(rks == ks.skip(1) && rvs == vs.skip(1));
        assert(sset(ks) == sset(ks.skip(1)).insert(ks[0]));
        assert(sset(rks) == sset(ks.skip(1)));
        assert(the_map(ks, vs)
            == the_map(ks.skip(1), vs.skip(1)).insert(ks[0], vs[0]));
        // insert then remove the same key is the identity HERE — the
        // tail cannot hold k (distinct puts k only at the head), which
        // is exactly insert_remove_fresh's precondition
        assert(wf(ks.skip(1), vs.skip(1)));
        keys_set_face(ks.skip(1), vs.skip(1));
        assert(!sset(ks.skip(1)).contains(k)) by {
            if sset(ks.skip(1)).contains(k) {
                tail_ne_head(ks, k);
            }
        }
        assert(!the_map(ks.skip(1), vs.skip(1)).dom().contains(k));
        insert_remove_fresh(the_map(ks.skip(1), vs.skip(1)), k, vs[0]);
        assert(the_map(rks, rvs) == the_map(ks.skip(1), vs.skip(1)));
        assert(wf(rks, rvs));
        assert(sset(ks).contains(k));
        assert(rks.len() == ks.len() - 1);
    } else {
        let (rks, rvs) = assoc_unset(ks.skip(1), vs.skip(1), k);
        assert(wf(ks.skip(1), vs.skip(1)));
        unset_sound(ks.skip(1), vs.skip(1), k);
        assert(the_map(rks, rvs) == the_map(ks.skip(1), vs.skip(1)).remove(k));
        // reconstruct: the_map(cons) — same concat stepping stones as
        // set_sound (skip/index of a 1-element prefix)
        assert((seq![ks[0]] + rks).skip(1) == rks
            && (seq![vs[0]] + rvs).skip(1) == rvs);
        assert((seq![ks[0]] + rks)[0] == ks[0]
            && (seq![vs[0]] + rvs)[0] == vs[0]);
        assert(the_map(seq![ks[0]] + rks, seq![vs[0]] + rvs)
            == the_map(rks, rvs).insert(ks[0], vs[0]));
        assert(the_map(ks, vs)
            == the_map(ks.skip(1), vs.skip(1)).insert(ks[0], vs[0]));
        insert_remove_commute(
            the_map(ks.skip(1), vs.skip(1)),
            k,
            ks[0],
            vs[0],
        );
        assert(the_map(rks, rvs).insert(ks[0], vs[0])
            == the_map(ks, vs).remove(k));
        // the key set: cons(head) of the IH survivors, and k is gone
        assert(sset(rks) == sset(ks.skip(1)).remove(k));
        assert(sset(seq![ks[0]] + rks) == sset(rks).insert(ks[0]));
        assert(sset(ks) == sset(ks.skip(1)).insert(ks[0]));
        set_remove_insert_commute(sset(ks.skip(1)), k, ks[0]);
        // wf: the head is absent from the tail's set and from rks's
        assert(ks[0] != k);
        assert(!sset(ks.skip(1)).contains(ks[0])) by {
            if sset(ks.skip(1)).contains(ks[0]) {
                tail_ne_head(ks, ks[0]);
            }
        }
        assert(!sset(rks).contains(ks[0]));
        distinct_cons(ks[0], rks);
        assert(wf(seq![ks[0]] + rks, seq![vs[0]] + rvs));
        // the head is not k, so the membership verdict for k comes
        // from the tail alone (the old assert compared against the
        // ORIGINAL table, which still holds k when k sat deeper)
        assert(sset(ks).contains(k) == sset(ks.skip(1)).contains(k));
        assert((seq![ks[0]] + rks).len()
            == ks.len() - (if sset(ks).contains(k) { 1int } else { 0int }));
    }
}

// ---------------------------------------------------------------------
// The faces
// ---------------------------------------------------------------------

// `${#m[@]}` — the count is the DISTINCT key count; for a wf table the
// keys are already distinct, so the length IS bash's count.
proof fn count_face(ks: Seq<Str>, vs: Seq<Str>)
    requires wf(ks, vs),
    ensures ks.len() == distinct_len(ks),
    decreases ks.len(),
{
    if ks.len() == 0 {
        assert(distinct_len(ks) == 0);
    } else {
        assert(!sset(ks.skip(1)).contains(ks[0])) by {
            // distinct: the head is absent from the tail's set
            assert forall|x: Str| sset(ks.skip(1)).contains(x) implies x != ks[0] by {
                // any tail member sits at some index >= 1
                if sset(ks.skip(1)).contains(x) {
                    tail_ne_head(ks, x);
                }
            }
        }
        count_face(ks.skip(1), vs.skip(1));
        assert(ks.len() == 1 + distinct_len(ks.skip(1)));
        assert(distinct_len(ks) == 1 + distinct_len(ks.skip(1)));
    }
}

// helper: under distinct, no tail element equals the head
proof fn tail_ne_head(ks: Seq<Str>, x: Str)
    requires
        distinct(ks),
        ks.len() > 0,
        sset(ks.skip(1)).contains(x),
    ensures x != ks[0],
    decreases ks.len(),
{
    if ks[0] == x {
        // the tail set contains x == ks[0], contradicting distinctness
        sset_index(ks.skip(1), x);
    } else if ks.len() == 1 {
        // tail is empty: its set contains nothing
        assert(sset(ks.skip(1)) == set![]);
        assert(false);
    } else {
        if sset(ks.skip(1).skip(1)).contains(x) {
            tail_ne_head(ks.skip(1), x);
            assert(x != ks[1]);
            assert(ks[0] != ks[1]); // distinct
        } else {
            // x must be the (now) head of the tail, i.e. ks[1]
            assert(x == ks[1]);
            assert(ks[0] != ks[1]); // distinct
        }
    }
}

// membership implies existence at an index (used above)
proof fn sset_index(xs: Seq<Str>, x: Str)
    requires sset(xs).contains(x),
    ensures exists|i: int| 0 <= i < xs.len() && xs[i] == x,
    decreases xs.len(),
{
    if xs.len() == 0 {
        assert(sset(xs) == set![]);
        assert(false);
    } else if xs[0] == x {
        assert(exists|i: int| 0 <= i < xs.len() && xs[i] == x);
    } else {
        sset_index(xs.skip(1), x);
        let j = choose|i: int| 0 <= i < xs.skip(1).len() && xs.skip(1)[i] == x;
        assert(xs[j + 1] == x);
        assert(exists|i: int| 0 <= i < xs.len() && xs[i] == x);
    }
}

// `${!m[@]}` — the key SET the table enumerates is the map's domain.
proof fn keys_set_face(ks: Seq<Str>, vs: Seq<Str>)
    requires wf(ks, vs),
    ensures sset(ks) == the_map(ks, vs).dom(),
    decreases ks.len(),
{
    if ks.len() == 0 || vs.len() == 0 {
        assert(sset(ks) == set![]);
        assert(the_map(ks, vs).dom() == set![]);
    } else {
        assert(the_map(ks, vs)
            == the_map(ks.skip(1), vs.skip(1)).insert(ks[0], vs[0]));
        keys_set_face(ks.skip(1), vs.skip(1));
        assert(sset(ks) == sset(ks.skip(1)).insert(ks[0]));
        assert(the_map(ks, vs).dom()
            == the_map(ks.skip(1), vs.skip(1)).dom().insert(ks[0]));
    }
}

// `${m[@]}` — the EMITTED fold is bash's space join (the fix; M1 kills
// it with the retired newline separator).
proof fn join_vals_sound(xs: Seq<Str>)
    ensures join_fold(xs, true) == inter_ws(xs),
    decreases xs.len(),
{
    if xs.len() == 0 {
        assert(join_fold(xs, true) == Seq::empty());
        assert(inter_ws(xs) == Seq::empty());
    } else if xs.len() == 1 {
        assert(join_fold(xs, true) == xs[0] + join_fold(xs.skip(1), false));
        assert(join_fold(xs.skip(1), false) == Seq::empty());
        assert(join_fold(xs, true) == xs[0]);
        assert(inter_ws(xs) == xs[0]);
    } else {
        join_fold_nonempty(xs.skip(1));
        join_vals_sound(xs.skip(1));
        assert(join_fold(xs, true) == xs[0] + join_fold(xs.skip(1), false));
        assert(join_fold(xs.skip(1), false) == seq![' '] + join_fold(xs.skip(1), true));
        assert(join_fold(xs, true) == xs[0] + seq![' '] + join_fold(xs.skip(1), true));
        assert(inter_ws(xs) == xs[0] + seq![' '] + inter_ws(xs.skip(1)));
        assert(join_fold(xs, true) == inter_ws(xs));
    }
}

// a nonempty frame's false-flag fold is "space + the true-flag fold"
proof fn join_fold_nonempty(xs: Seq<Str>)
    requires xs.len() > 0,
    ensures join_fold(xs, false) == seq![' '] + join_fold(xs, true),
    decreases xs.len(),
{
    if xs.len() == 1 {
        assert(join_fold(xs, false) == seq![' '] + xs[0] + join_fold(xs.skip(1), false));
        assert(join_fold(xs.skip(1), false) == Seq::empty());
        assert(join_fold(xs, true) == xs[0] + join_fold(xs.skip(1), false));
    } else {
        join_fold_nonempty(xs.skip(1));
        assert(join_fold(xs, false) == seq![' '] + xs[0] + join_fold(xs.skip(1), false));
        assert(join_fold(xs, true) == xs[0] + join_fold(xs.skip(1), false));
        assert(join_fold(xs, false) == seq![' '] + join_fold(xs, true));
    }
}

// `m[k]+=v` — the compound read-modify-write stores map[k] ++ v (the
// assign arm concatenates `compound_read` with the RHS and the element
// store writes it back through _sh_assoc_set; c4/d6 measured).
proof fn rmw_face(ks: Seq<Str>, vs: Seq<Str>, k: Str, v: Str)
    requires
        wf(ks, vs),
        the_map(ks, vs).dom().contains(k),
    ensures
        the_map(
            assoc_set(ks, vs, k, assoc_get(ks, vs, k) + v).0,
            assoc_set(ks, vs, k, assoc_get(ks, vs, k) + v).1,
        ) == the_map(ks, vs).insert(k, the_map(ks, vs)[k] + v),
{
    get_sound(ks, vs, k);
    set_sound(ks, vs, k, assoc_get(ks, vs, k) + v);
    assert(assoc_get(ks, vs, k) == the_map(ks, vs)[k]);
}

// ---------------------------------------------------------------------
// The int-attr arm (`declare -i n; n+=3`) — INTEGER level
// (c6: bash evaluates 8; the pre-fix always-concat arm produced 53).
// Byte spelling of both numbers is c_dec_str.rs's territory.
// ---------------------------------------------------------------------

pub open spec fn digits(n: nat) -> nat
    decreases n,
{
    if n < 10 {
        1nat
    } else {
        1 + digits(n / 10)
    }
}

pub open spec fn pow10(k: nat) -> nat
    decreases k,
{
    if k == 0 {
        1nat
    } else {
        // k != 0 and k: nat  ==>  k >= 1
        10 * pow10((k - 1) as nat)
    }
}

// the numeric reading of `old ++ rhs` (string concatenation of decimal
// words) — what `atoll(concat(old, rhs))` computes for digit strings
pub open spec fn cat_num(old: nat, rhs: nat) -> nat {
    old * pow10(digits(rhs)) + rhs
}

// bash's rule vs the renderer's two arms, at the integer level
pub open spec fn bash_compound(int_attr: bool, old: nat, rhs: nat) -> nat {
    if int_attr {
        old + rhs
    } else {
        cat_num(old, rhs)
    }
}

pub open spec fn arm_post(int_attr: bool, old: nat, rhs: nat) -> nat {
    // the FIXED arm: the int-attr guard routes to the arithmetic RMW
    if int_attr {
        old + rhs
    } else {
        cat_num(old, rhs)
    }
}

// the PRE-FIX arm: no attr check — every `+=` concatenated
pub open spec fn arm_pre(int_attr: bool, old: nat, rhs: nat) -> nat {
    cat_num(old, rhs)
}

proof fn int_attr_face()
    ensures forall|int_attr: bool, old: nat, rhs: nat|
        arm_post(int_attr, old, rhs) == bash_compound(int_attr, old, rhs),
{
}

// M3: the measured c6 pair — pre-fix 53, bash 8
proof fn m3_int_attr_concat()
    ensures
        arm_post(true, 5, 3) == 8,
        arm_pre(true, 5, 3) == 53,
        arm_pre(true, 5, 3) != bash_compound(true, 5, 3),
{
    assert(pow10(digits(3)) == 10) by (compute);
    assert(cat_num(5, 3) == 53) by (compute);
}

// ---------------------------------------------------------------------
// The homing veto (a6): a whole-array join shaped as
// `param("slice", NAME, "@", LEN)` must veto native (long long[])
// homing — the pre-fix walker only knew slice/arrayItems/listVar calls,
// so `v="${arr[@]}"` homed `arr` as long long[] while `_sh_join_arr`
// took that long long* where char** was expected (SIGSEGV measured).
// ---------------------------------------------------------------------

pub enum Shape {
    // param("slice", name, "@", len)
    ParamSliceAt { name: Str },
    // param("slice", name, "0", len) — a scalar slice, no veto
    ParamSliceScalar { name: Str },
    // a plain scalar read
    ScalarRead,
}

// the FIXED walker: which names land in str_read (the veto set)
pub open spec fn marks(shape: Shape) -> Set<Str> {
    match shape {
        Shape::ParamSliceAt { name } => set![strip_bang_at(name)],
        Shape::ParamSliceScalar { name } => set![],
        Shape::ScalarRead => set![],
    }
}

// `${!m[@]}`'s `!` prefix / `[@]` suffix are stripped before the check
pub open spec fn strip_bang_at(n: Str) -> Str {
    let base = if n.len() > 0 && n[0] == '!' {
        n.skip(1)
    } else {
        n
    };
    // drop a trailing "[@]"/"[*]" (3 chars)
    if base.len() >= 3
        && base.skip(base.len() - 3) == seq!['[', '@', ']']
    {
        base.take(base.len() - 3)
    } else if base.len() >= 3
        && base.skip(base.len() - 3) == seq!['[', '*', ']']
    {
        base.take(base.len() - 3)
    } else {
        base
    }
}

// the homing decision: a name in the veto set may NOT home as native
pub open spec fn homed(name: Str, veto: Set<Str>) -> bool {
    !veto.contains(name)
}

proof fn veto_covers_param_join()
    ensures
        homed(
            seq!['a', 'r', 'r'],
            marks(Shape::ParamSliceAt { name: seq!['a', 'r', 'r'] }),
        ) == false,
        homed(
            seq!['a', 'r', 'r'],
            marks(Shape::ParamSliceScalar { name: seq!['a', 'r', 'r'] }),
        ) == true,
        homed(seq!['x'], marks(Shape::ScalarRead)) == true,
{
    let arr = seq!['a', 'r', 'r'];
    assert(strip_bang_at(arr) == arr) by (compute);
    assert(marks(Shape::ParamSliceAt { name: arr }).contains(arr));
}

// M4: the PRE-FIX walker did not know the param shape at all
proof fn m4_veto_miss()
    ensures homed(seq!['a', 'r', 'r'], pre_fix_marks()) == true,
{
}

pub open spec fn pre_fix_marks() -> Set<Str> {
    // slice/arrayItems/listVar only — `param` never inserted
    set![]
}

// ---------------------------------------------------------------------
// The measured probes (set-level: content, with the hash-order edge
// documented rather than chased)
// ---------------------------------------------------------------------

// b1: `declare -A m=([a]=1 [b]=2 [c]=3); unset 'm[b]'`
//     pre-fix: no code emitted, count stayed 3 (bash: 2)
//
// EVALUATION DISCIPLINE (measured): `by (compute)` DIVERGES on any
// expression containing a let-bound recursion result (rks/rvs) — the
// evaluator cannot head-reduce the_map/rks and spins forever — and it
// cannot reduce `.dom().len()` at all (dom is uninterp). So computes
// always take the FULL concrete call term, and the count is stated
// over `.0.len()` — the `{id}_n` arm, which is what the backend emits
// and what this model's count_face already denotates.
proof fn probe_unset_str_key()
    ensures
        // `${#m[@]}` after `unset 'm[b]'` is 2 (bash measured 2)
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0.len()
            == 2int,
        sset(
            assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0,
        ).contains(ka()),
        sset(
            assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0,
        ).contains(kc()),
        !sset(
            assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0,
        ).contains(kb()),
{
    assert(
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0
            == seq![ka(), kc()]
    ) by (compute);
    assert(
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).1
            == seq![v1(), v3()]
    ) by (compute);
    assert(
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0.len()
            == 2int
    ) by (compute);
    assert(
        sset(assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0)
            .contains(ka())
    ) by (compute);
    assert(
        sset(assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0)
            .contains(kc())
    ) by (compute);
    assert(
        sset(assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0)
            // sset is front-recursive: sset([a,c]) = empty.insert(c).insert(a)
            == set![kc(), ka()]
    ) by (compute);
    assert(ka()[0] == 'a' && kb()[0] == 'b' && kc()[0] == 'c');
    assert(kb() != kc() && kb() != ka());
    assert(
        !sset(assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0)
            .contains(kb())
    );
    let (rks, rvs) = assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb());
    assert(rks == seq![ka(), kc()]);
    assert(rvs == seq![v1(), v3()]);
}

// b2: `declare -A m=([0]=x [1]=y); unset 'm[0]'` — the subscript "0"
// is a STRING key: the entry stored under key "0" goes, key "1" stays.
// (The pre-fix arm emitted an indexed clear naming `m_len` — cc red.)
proof fn probe_unset_num_key()
    ensures
        sset(assoc_unset(
            seq![seq!['0'], seq!['1']],
            seq![seq!['x'], seq!['y']],
            seq!['0'],
        ).0) == set![seq!['1']],
        the_map(assoc_unset(
            seq![seq!['0'], seq!['1']],
            seq![seq!['x'], seq!['y']],
            seq!['0'],
        ).0, assoc_unset(
            seq![seq!['0'], seq!['1']],
            seq![seq!['x'], seq!['y']],
            seq!['0'],
        ).1)[seq!['1']] == seq!['y'],
{
    assert(
        assoc_unset(
            seq![seq!['0'], seq!['1']],
            seq![seq!['x'], seq!['y']],
            seq!['0'],
        ).0 == seq![seq!['1']]
    ) by (compute);
    assert(
        assoc_unset(
            seq![seq!['0'], seq!['1']],
            seq![seq!['x'], seq!['y']],
            seq!['0'],
        ).1 == seq![seq!['y']]
    ) by (compute);
    let (rks, rvs) = assoc_unset(
        seq![seq!['0'], seq!['1']],
        seq![seq!['x'], seq!['y']],
        seq!['0'],
    );
    assert(rks == seq![seq!['1']]);
    assert(rvs == seq![seq!['y']]);
    assert(
        the_map(
            assoc_unset(
                seq![seq!['0'], seq!['1']],
                seq![seq!['x'], seq!['y']],
                seq!['0'],
            ).0,
            assoc_unset(
                seq![seq!['0'], seq!['1']],
                seq![seq!['x'], seq!['y']],
                seq!['0'],
            ).1,
        ) == Map::empty().insert(seq!['1'], seq!['y'])
    ) by (compute);
    assert(
        sset(assoc_unset(
            seq![seq!['0'], seq!['1']],
            seq![seq!['x'], seq!['y']],
            seq!['0'],
        ).0) == set![seq!['1']]
    ) by (compute);
}

// b3: `k=b; unset "m[$k]"` — the interpolated key resolves at runtime
// to the same string the literal form uses; the denotation is the map
// remove under that string.
proof fn probe_unset_dyn_key()
    ensures
        the_map(assoc_unset(
            seq![ka(), kb()],
            seq![v1(), v2()],
            seq!['b'],
        ).0, assoc_unset(
            seq![ka(), kb()],
            seq![v1(), v2()],
            seq!['b'],
        ).1) == Map::empty().insert(ka(), v1()),
{
    assert(
        the_map(
            assoc_unset(seq![ka(), kb()], seq![v1(), v2()], seq!['b']).0,
            assoc_unset(seq![ka(), kb()], seq![v1(), v2()], seq!['b']).1,
        ) == Map::empty().insert(ka(), v1())
    ) by (compute);
}

proof fn probe_set_append_and_replace()
    ensures
        // `declare -A m=([a]=1); m+=([b]=2)` — append a NEW key; the
        // count arm is keys.len()
        assoc_set(seq![ka()], seq![v1()], kb(), v2()).0.len() == 2int,
        // `m[k]+=w` style replace keeps the key set
        sset(assoc_set(seq![ka(), kb()], seq![v1(), v2()], ka(), v3()).0)
            == sset(seq![ka(), kb()]),
        the_map(assoc_set(seq![ka(), kb()], seq![v1(), v2()], ka(), v3()).0,
                 assoc_set(seq![ka(), kb()], seq![v1(), v2()], ka(), v3()).1)
            == the_map(seq![ka(), kb()], seq![v1(), v2()]).insert(ka(), v3()),
{
    assert(assoc_set(seq![ka()], seq![v1()], kb(), v2()).0 == seq![ka(), kb()])
        by (compute);
    assert(assoc_set(seq![ka()], seq![v1()], kb(), v2()).1 == seq![v1(), v2()])
        by (compute);
    let (a1, b1s) = assoc_set(seq![ka()], seq![v1()], kb(), v2());
    assert(a1 == seq![ka(), kb()]);
    assert(b1s == seq![v1(), v2()]);
    assert(a1.len() == 2int);
    // the replace arm: same key set, last value wins (set_sound is
    // the general theorem; here the concrete shape pins it)
    assert(
        assoc_set(seq![ka(), kb()], seq![v1(), v2()], ka(), v3()).0
            == seq![ka(), kb()]
    ) by (compute);
    set_sound(seq![ka(), kb()], seq![v1(), v2()], ka(), v3());
}

// ---------------------------------------------------------------------
// Mutants — each kills one obligation the fixes established
// ---------------------------------------------------------------------

// M1: the RETIRED newline join — bash never joins with '\n' in a string
// context (measured: `echo "[${m[@]}]"` printed `1\n2\n3`).
proof fn m1_nl_join()
    ensures
        inter_nl(seq![v1(), v2(), v3()]) != inter_ws(seq![v1(), v2(), v3()]),
{
    assert(inter_ws(seq![v1(), v2(), v3()]) == seq!['1', ' ', '2', ' ', '3']) by (compute);
    assert(inter_nl(seq![v1(), v2(), v3()]) == seq!['1', '\n', '2', '\n', '3']) by (compute);
}

// M2: the PRE-FIX unset was a no-op (no arm for a non-numeric key), so
// the map kept the entry bash drops (count 3 vs bash's 2).
proof fn m2_unset_noop()
    ensures
        the_map(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()])
            != the_map(
                seq![ka(), kb(), kc()],
                seq![v1(), v2(), v3()],
            ).remove(kb()),
{
    assert(
        the_map(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()])
            == Map::empty().insert(kc(), v3()).insert(kb(), v2()).insert(ka(), v1())
    ) by (compute);
    assert(ka()[0] == 'a' && kb()[0] == 'b' && kc()[0] == 'c');
    assert(kb() != kc() && ka() != kb());
    let full = the_map(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()]);
    let less = full.remove(kb());
    // kb IS in the full table's domain (the insert chain says so)
    assert(full.dom().contains(kb()));
    // remove drops exactly that key
    assert(less.dom() == full.dom().remove(kb()));
    assert(!less.dom().contains(kb()));
    // unequal domains — the maps cannot be equal
    assert(full != less);
}

// ---- the canonical keys/values of the probe ----
pub open spec fn ka() -> Str { seq!['a'] }
pub open spec fn kb() -> Str { seq!['b'] }
pub open spec fn kc() -> Str { seq!['c'] }
pub open spec fn v1() -> Str { seq!['1'] }
pub open spec fn v2() -> Str { seq!['2'] }
pub open spec fn v3() -> Str { seq!['3'] }

// ---------------------------------------------------------------------
// The canonical scenario, all faces at once (the b/c probes' content)
// ---------------------------------------------------------------------

proof fn canonical_faces()
    ensures
        // `${#m[@]}` after unset 'm[b]' is 2 (bash measured 2)
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0.len()
            == 2,
        // the join spells the survivors in OUR order, space-separated
        // (bash spells them in hash order — the documented edge)
        join_fold(
            assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).1,
            true,
        ) == seq!['1', ' ', '3'],
        // `${m[@]}` of the untouched table is the space join
        join_fold(seq![v1(), v2(), v3()], true) == inter_ws(seq![v1(), v2(), v3()]),
{
    assert(
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).0
            == seq![ka(), kc()]
    ) by (compute);
    assert(
        assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).1
            == seq![v1(), v3()]
    ) by (compute);
    assert(
        join_fold(
            assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb()).1,
            true,
        ) == seq!['1', ' ', '3']
    ) by (compute);
    let (rks, rvs) = assoc_unset(seq![ka(), kb(), kc()], seq![v1(), v2(), v3()], kb());
    assert(rks == seq![ka(), kc()]);
    assert(rvs == seq![v1(), v3()]);
    join_vals_sound(seq![v1(), v2(), v3()]);
    assert(inter_ws(seq![v1(), v2(), v3()]) == seq!['1', ' ', '2', ' ', '3']) by (compute);
}

} // verus!

fn main() {}
