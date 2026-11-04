// Standalone model of the C backend's WRITE ARM — the `"=" | ":="`
// branch of the `${...}` param renderer (sh2perl/src/c_backend.rs,
// `match op.as_str()` in the expansion emitter), together with the two
// gates that make it safe: `param_write_target` (the census gate) and
// the length-census supplement that sizes the buffer the arm `strcpy`s
// into.
//
// THE ARM, verbatim in structure:
//   v = (nonempty_test(name) ? <cur> : <word>);          // the value
//   if is_num(name) || name is empty   ->  v             // NO store
//   else if buf_bound(name) = Some(b)  ->  (strcpy(buf, v), v)
//   else                               ->  (x = (test ? x : _sh_xstrdup(v)),
//                                           v)           // heap-owned
//
// WHY THIS EXISTS — four measured defects of this path, found with the
// probe battery under /tmp/wam* (three fixed in source, one pinned as
// a known gap):
//  1. GLUE (parser, FIXED): the quoted chain glued a bare `=` operator
//     into the NAME — `param("", "v1=d")` — so every backend read a
//     variable literally named `v1=d`, no write ever happened, and the
//     probe printed `unset1=[] z=[]` where bash printed
//     `unset1=[f] z=[f]`. Fix: ident-guarded split ahead of the
//     pattern/suffix arms in BOTH parse chains
//     (`parse_variable_expansion_inner`,
//     `parse_parameter_expansion_content_inner`); unit tests
//     `parser::words::bare_assign_tests`. A `/` or `-` inside the
//     default word (`${v=/usr/local}`, `${v=b-c}`) was mis-read as the
//     operator; both now split correctly (probe wam3: C == bash).
//  2. BOUNDS (census, FIXED): `analyze_string_lengths` collects
//     neither `IrStmt::Declare` inits nor any `${NAME=word}` write (an
//     expression-side effect), so `local x=1; x=` yielded
//     `max_len 0` -> `char x[1]` -> `assert(strlen("1") <= 0)` aborts
//     at STARTUP (probe wam5), and an `:=` default longer than the
//     bound `strcpy`s past the buffer (probe wam4). Fix:
//     `supplement_write_lengths` MAX-folds both store shapes into the
//     core census BEFORE any consumer runs (c_backend init, right
//     after `analyze_string_lengths`).
//  3. STALE FOLD (copy-propagation, FIXED): `count_stmt_assigns`
//     never walked an Assign's RHS, so the write inside `v=${x:=d}`
//     was invisible to def-counting, `x=""` kept count==1, and reads
//     of x after the write folded to the pre-write literal (probe
//     wam4 printed `colon:[d][]` for bash's `colon:[d][d]`). Fix:
//     Assign-RHS recursion in `count_stmt_assigns` plus a param-write
//     site in `count_expr_assigns`' Call arm (op ∈ {`=`,`:=`},
//     deliberately without an ident gate: an extra site only makes a
//     var less foldable — strictly safe).
//  4. NORMALIZATION (known gap, PINNED — not fixed): the bash parser
//     normalizes bare `=` to `:=` (shir.rs `param_ir`:
//     AssignDefault -> ":="), so the unset-only rule of `${n=w}` is
//     unrepresentable downstream: with x SET-EMPTY bash leaves it
//     empty (`bare:[][]`) while the C arm assigns (`bare:[d][d]`).
//     Corpus impact: ZERO — 0 bare-`=` shapes in 651 corpus files
//     (only `:=`, 3 occurrences). The model proves `:=` fully, proves
//     `=` agrees whenever the name is unset-or-nonempty, and proves
//     the gap's exact shape (`bare_setempty_gap`) rather than hiding
//     it.
//
// THE EXTENDED shIR DENOTATION (the "extend as convenient" half):
// a store that distinguishes UNSET from SET-EMPTY, with C cells
// carrying ownership:
//   * bash's view of a name is `Option<Str>` — None = unset,
//     Some("") = set-but-empty: exactly the distinction `=` observes
//     and `:=` collapses;
//   * a C cell is `Unset` | `Owned(bytes, id)` (a heap block the
//     scope-end cleanup will `free`) | `Static(bytes)` (a C literal or
//     fixed buffer — freeing it is the p13/p11/p14
//     `munmap_chunk(): invalid pointer` abort);
//   * storage classes: Heap (`char*`, NULL encodes unset), Buf
//     (`char x[b+1]`, present by construction — SET, so unset and
//     set-empty are indistinguishable IN the buffer), Num
//     (`long long`, always set).
//
// WHAT IS PROVED:
//   * needs_eq_test — the bridge: the arm's nonempty test is the
//     NEGATION of the colon rule's `needs` (under the `=` proviso);
//   * value_agrees — the emitted value expression equals the
//     denotation for `:=` always, and for `=` when the name is not
//     set-empty;
//   * heap_double_eval — the heap branch evaluates the test TWICE
//     (store expression, then the value re-read AFTER the store);
//     both evaluations yield the same value (the source comment's
//     "the value arm re-reads y (same bytes)");
//   * heap_sound / buf_sound / num_sound — value AND bash-view store
//     equal the denotation in all three classes (heap/buf under the
//     no-set-empty proviso for op `=`; `:=` unconditional), and a
//     heap slot never leaves the arm Static (ownership preserved);
//   * bare_setempty_gap — op `=` on a set-empty name: the arm assigns
//     where bash does not — the honest statement of known gap #4,
//     shaped exactly like probe wam4's `bare:[d][d]` vs `[][]`;
//   * bounds: `bump` (the supplement's MAX fold) with `buf_bound`'s
//     FIXED_BUF_CAP filter routes correctly — either the fixed buffer
//     fits the default word (`strcpy_fits`) or the name stays `char*`
//     (heap path); a bound is never SHRUNK by the fold;
//   * mutants: `mutant_static_store` (the pre-fix literal store —
//     xstrdup dropped — violates the heap ownership invariant) and
//     `mutant_flip_colon` (the flipped op rule contradicts the
//     denotation on set-empty);
//   * pins for every probe shape: unset-colon assign, set-empty colon,
//     set-empty bare gap, set-nonempty no-write + cell identity,
//     wam5 bounds, double-eval instances, path-word value.
//
// WHAT IS NOT MODELLED: the operand renderers that BUILD `cur` and
// `word` (oracle inputs); dynamic default words (`$(cmd)` — every
// corpus write-op word is a literal Str; dynamic words enter only via
// `bump` -> None -> unbounded -> heap path); global allocator
// freshness across calls (ownership here is "Owned, never Static",
// plus cell identity when the test holds); the numeric class's string
// rendering (num-validity — always set — is the class hypothesis).
//
// Probes (bash vs compiled C, rerun after all three fixes):
//   wam3 MATCH; wam5 MATCH; wam4 `colon:[d][d]` MATCH,
//   `bare:[d][d]` vs bash `bare:[][]` = gap #4; wam matches except
//   the same gap line. C gate 588/56/7 (= baseline), ./fail 563/563.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_write_arm.rs
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Str = Seq<char>;

// ── The extended store: C cells with ownership ────────────────────────

pub enum Cell {
    // No binding (a heap slot before first assignment; an absent name
    // reads the same way).
    Unset,
    // A heap block the runtime owns and will `free` at end of scope;
    // `id` is the allocation's provenance tag.
    Owned(Str, nat),
    // Static storage: a C string literal or a fixed buffer. Freeing
    // it is UB — the `munmap_chunk` abort class (p13/p11/p14) behind
    // the arm's `_sh_xstrdup`.
    Static(Str),
}

pub type Store = Map<Name, Cell>;

// The bash view of one name: None = unset, Some(s) = set (s may be
// empty) — exactly the distinction `${n=w}` observes and `${n:=w}`
// collapses.
pub open spec fn read_c(s: Store, n: Name) -> Option<Str> {
    if s.dom().contains(n) {
        match s[n] {
            Cell::Unset => Option::None,
            Cell::Owned(b, _) => Option::Some(b),
            Cell::Static(b) => Option::Some(b),
        }
    } else {
        Option::None
    }
}

pub open spec fn set_empty(cur: Option<Str>) -> bool {
    cur.is_Some() && cur.get_Some_0() == Seq::empty()
}

// ── The denotation of the write op ────────────────────────────────────

pub enum Op {
    Bare,  // `${n=w}`   — assign iff UNSET
    Colon, // `${n:=w}`  — assign iff UNSET or EMPTY
}

// Every corpus write-op word is a literal, so the denotation takes it
// directly; dynamic words enter only through the bounds side (None =
// unbounded = heap path).
pub open spec fn needs(op: Op, cur: Option<Str>) -> bool {
    match op {
        Op::Colon => cur.is_None() || set_empty(cur),
        Op::Bare => cur.is_None(),
    }
}

pub open spec fn denote_val(op: Op, cur: Option<Str>, w: Str) -> Str {
    if needs(op, cur) {
        w
    } else {
        match cur {
            Option::Some(v) => v,
            Option::None => w, // unreachable: needs holds when None
        }
    }
}

// The binding after the expansion: bash writes the word back iff
// `needs`; otherwise the binding is untouched. Only this name changes.
pub open spec fn denote_upd(op: Op, cur: Option<Str>, w: Str) -> Option<Str> {
    if needs(op, cur) {
        Option::Some(w)
    } else {
        cur
    }
}

// ── The C arm ─────────────────────────────────────────────────────────

pub enum Class {
    Heap, // `char*` — NULL encodes unset; scope-end cleanup frees it
    Buf,  // `char x[b+1]` — present from declaration, never freed
    Num,  // `long long` — always set (class hypothesis below)
}

// The nonempty test the arm computes: `(x && x[0])` on a heap slot,
// `(x[0])` on a buffer (present by construction — same predicate).
pub open spec fn nonempty_test(cur: Option<Str>) -> bool {
    cur.is_Some() && cur.get_Some_0() != Seq::empty()
}

// The emitted value expression `(test ? cur : word)`. The heap class
// evaluates this AFTER the store (comma operator) — see
// `heap_double_eval`.
pub open spec fn arm_value(t: bool, cur: Option<Str>, w: Str) -> Str {
    if t {
        match cur {
            Option::Some(v) => v,
            Option::None => Seq::empty(), // unreachable: t requires Some
        }
    } else {
        w
    }
}

// One evaluation of the arm at name n: the value it yields and the
// store it leaves (n updated only when a store is emitted).
// `nid` = the allocator's next provenance tag for `_sh_xstrdup`.
pub open spec fn arm_at(
    s: Store,
    class: Class,
    op: Op,
    n: Name,
    w: Str,
    nid: nat,
) -> (Str, Store) {
    let cur = read_c(s, n);
    let t = nonempty_test(cur);
    let v = arm_value(t, cur, w);
    match class {
        // is_num || name empty -> value only, NO store.
        Class::Num => (v, s),
        // (strcpy(buf, v), v): the buffer holds v afterwards (the
        // same bytes it already held when the test holds — strcpy
        // copies cur back over cur).
        Class::Buf => (v, s.insert(n, Cell::Static(v))),
        // (x = (t ? x : _sh_xstrdup(v)), value-re-read):
        Class::Heap => {
            let s2 = if t {
                s // cell unchanged: same bytes, same id
            } else {
                s.insert(n, Cell::Owned(v, nid))
            };
            let after = read_c(s2, n);
            let v2 = arm_value(nonempty_test(after), after, w);
            (v2, s2)
        }
    }
}

// ── Class validity (the hypotheses each theorem carries) ──────────────

// A heap-class slot holds Owned bytes or nothing — never Static.
pub open spec fn heap_cell_ok(c: Cell) -> bool {
    match c {
        Cell::Unset => true,
        Cell::Owned(_, _) => true,
        Cell::Static(_) => false,
    }
}

pub open spec fn heap_valid(s: Store) -> bool {
    forall|n: Name| s.dom().contains(n) ==> heap_cell_ok(s[n])
}

// A buffer's storage exists from declaration, so its bash view is
// always SET — the class hypothesis that makes `:=` exact on buffers
// (and makes unset-vs-empty unrepresentable in the buffer itself).
pub open spec fn is_static(c: Cell) -> bool {
    match c {
        Cell::Static(_) => true,
        _ => false,
    }
}

pub open spec fn buf_valid(s: Store, n: Name) -> bool {
    s.dom().contains(n) && is_static(s[n])
}

// A numeric-homed name always holds a number in C, so its bash view is
// set and nonempty: neither `=` nor `:=` assigns and the arm's
// no-store branch is correct.
pub open spec fn num_valid(cur: Option<Str>) -> bool {
    cur.is_Some() && cur.get_Some_0() != Seq::empty()
}

// ── The bridge: the arm's test is the NEGATION of `needs` ─────────────

proof fn needs_eq_test(op: Op, cur: Option<Str>)
    requires
        op == Op::Colon || !set_empty(cur),
    ensures
        needs(op, cur) == !nonempty_test(cur),
{
    match op {
        Op::Colon => {
            if cur.is_None() {
                assert(needs(op, cur) == true);
                assert(nonempty_test(cur) == false);
            } else if set_empty(cur) {
                assert(needs(op, cur) == true);
                assert(nonempty_test(cur) == false);
            } else {
                assert(needs(op, cur) == false);
                assert(nonempty_test(cur) == true);
            }
        },
        Op::Bare => {
            assert(!set_empty(cur));
            if cur.is_None() {
                assert(needs(op, cur) == true);
                assert(nonempty_test(cur) == false);
            } else {
                // Some(s), s != "" (premise excludes set_empty)
                assert(needs(op, cur) == false);
                assert(nonempty_test(cur) == true);
            }
        },
    }
}

// ── Value soundness ───────────────────────────────────────────────────

// The value half, class-agnostic: `:=` always; `=` when the name is
// not set-empty.
proof fn value_agrees(op: Op, cur: Option<Str>, w: Str)
    requires
        op == Op::Colon || !set_empty(cur),
    ensures
        arm_value(nonempty_test(cur), cur, w) == denote_val(op, cur, w),
{
    needs_eq_test(op, cur);
    let t = nonempty_test(cur);
    if t {
        assert(!needs(op, cur));
        assert(denote_val(op, cur, w) == cur.get_Some_0());
        assert(arm_value(true, cur, w) == cur.get_Some_0());
    } else {
        assert(needs(op, cur));
        assert(denote_val(op, cur, w) == w);
        assert(arm_value(false, cur, w) == w);
    }
}

// The heap branch evaluates the test twice: once before the store,
// then the value expression re-reads the POST-store bytes. Both yield
// the same value. (`t` is the test OF THIS CUR — the link the arm's
// own definition maintains; without it the statement is false, e.g.
// t=true on cur=Some("").)
proof fn heap_double_eval(t: bool, cur: Option<Str>, w: Str)
    requires
        t == nonempty_test(cur),
    ensures
        {
            let after = if t {
                cur
            } else {
                Option::Some(w)
            };
            arm_value(nonempty_test(after), after, w) == arm_value(t, cur, w)
        },
{
    if t {
        // after == cur: the same expression
        assert(nonempty_test(cur) == nonempty_test(cur));
    } else {
        // after == Some(w); the first evaluation is w.
        if w == Seq::empty() {
            assert(nonempty_test(Option::Some(w)) == false);
            assert(arm_value(false, Option::Some(w), w) == w);
        } else {
            assert(nonempty_test(Option::Some(w)) == true);
            assert(arm_value(true, Option::Some(w), w) == w);
        }
        assert(arm_value(false, cur, w) == w);
    }
}

// ── Per-class soundness (each statement packaged as a predicate) ──────

// HEAP, delivered op `op`: value and bash-view store equal the
// denotation, and the slot never leaves Static.
pub open spec fn heap_ok(s: Store, n: Name, op: Op, w: Str, nid: nat) -> bool {
    let cur = read_c(s, n);
    let (v, s2) = arm_at(s, Class::Heap, op, n, w, nid);
    v == denote_val(op, cur, w) && read_c(s2, n) == denote_upd(op, cur, w)
        && heap_valid(s2)
}

proof fn heap_sound(s: Store, n: Name, op: Op, w: Str, nid: nat)
    requires
        heap_valid(s),
        op == Op::Colon || !set_empty(read_c(s, n)),
    ensures
        heap_ok(s, n, op, w, nid),
{
    let cur = read_c(s, n);
    value_agrees(op, cur, w);
    let t = nonempty_test(cur);
    let (v, s2) = arm_at(s, Class::Heap, op, n, w, nid);
    if t {
        // the cell is untouched: s2 == s
        assert(s2 == s);
        assert(after_is(s2, n, cur));
        heap_double_eval(t, cur, w);
        assert(v == arm_value(t, cur, w));
        assert(v == denote_val(op, cur, w));
        assert(read_c(s2, n) == cur);
        needs_eq_test(op, cur);
        assert(!needs(op, cur));
        assert(denote_upd(op, cur, w) == cur);
        assert(heap_valid(s2));
    } else {
        // the store inserts Owned(v1, nid) with v1 == w (t false)
        heap_double_eval(t, cur, w);
        assert(v == arm_value(t, cur, w));
        assert(v == w);
        assert(v == denote_val(op, cur, w));
        assert(s2 == s.insert(n, Cell::Owned(w, nid)));
        assert(read_c(s2, n) == Option::Some(w));
        needs_eq_test(op, cur);
        assert(needs(op, cur));
        assert(denote_upd(op, cur, w) == Option::Some(w));
        // ownership: the inserted cell is Owned; every other cell is
        // untouched and was ok by the precondition
        assert(heap_cell_ok(Cell::Owned(w, nid)));
        assert forall|m: Name| s2.dom().contains(m) ==> heap_cell_ok(s2[m]) by {
            if m != n {
                assert(s2[m] == s[m]);
                assert(s2.dom().contains(m) == s.dom().contains(m));
            }
        }
        assert(heap_valid(s2));
    }
}

// (helper for the t-branch: after the no-store branch the view is
// unchanged — stated to keep the unfold readable)
spec fn after_is(s2: Store, n: Name, cur: Option<Str>) -> bool {
    read_c(s2, n) == cur
}

// BUF: value and bash-view store equal the denotation under the
// buffer's set-by-construction hypothesis.
pub open spec fn buf_ok(s: Store, n: Name, op: Op, w: Str, nid: nat) -> bool {
    let cur = read_c(s, n);
    let (v, s2) = arm_at(s, Class::Buf, op, n, w, nid);
    v == denote_val(op, cur, w) && read_c(s2, n) == denote_upd(op, cur, w)
}

proof fn buf_sound(s: Store, n: Name, op: Op, w: Str, nid: nat)
    requires
        buf_valid(s, n),
        op == Op::Colon || !set_empty(read_c(s, n)),
    ensures
        buf_ok(s, n, op, w, nid),
{
    let cur = read_c(s, n);
    value_agrees(op, cur, w);
    let t = nonempty_test(cur);
    let (v, s2) = arm_at(s, Class::Buf, op, n, w, nid);
    assert(s2 == s.insert(n, Cell::Static(v)));
    assert(read_c(s2, n) == Option::Some(v));
    needs_eq_test(op, cur);
    if t {
        assert(!needs(op, cur));
        assert(denote_upd(op, cur, w) == cur);
        assert(cur.is_Some());
        assert(v == cur.get_Some_0());
    } else {
        assert(needs(op, cur));
        assert(denote_upd(op, cur, w) == Option::Some(w));
        assert(v == w);
    }
}

// NUM: no store emitted; under num-validity (always set, nonempty)
// neither op assigns, so value and store both match the denotation —
// for BOTH ops, no proviso.
pub open spec fn num_ok(s: Store, n: Name, op: Op, w: Str, nid: nat) -> bool {
    let cur = read_c(s, n);
    let (v, s2) = arm_at(s, Class::Num, op, n, w, nid);
    v == denote_val(op, cur, w) && read_c(s2, n) == denote_upd(op, cur, w)
}

proof fn num_sound(s: Store, n: Name, op: Op, w: Str, nid: nat)
    requires
        num_valid(read_c(s, n)),
    ensures
        num_ok(s, n, op, w, nid),
{
    let cur = read_c(s, n);
    assert(!set_empty(cur)); // nonempty
    value_agrees(op, cur, w);
    let t = nonempty_test(cur);
    assert(t == true);
    let (v, s2) = arm_at(s, Class::Num, op, n, w, nid);
    assert(s2 == s);
    needs_eq_test(op, cur);
    assert(!needs(op, cur));
    assert(denote_upd(op, cur, w) == cur);
    assert(read_c(s2, n) == read_c(s, n));
}

// ── Known gap #4: bare `=` on a SET-EMPTY name ────────────────────────

// The arm assigns where bash does not. Class-independent (the value
// expression is shared); stated on the wam4 shape: a buffer holding "".
pub open spec fn bare_gap_holds(w: Str, nid: nat) -> bool {
    let s = Map::empty().insert(0nat, Cell::Static(Seq::empty()));
    let cur = read_c(s, 0nat);
    let (v, s2) = arm_at(s, Class::Buf, Op::Bare, 0nat, w, nid);
    set_empty(cur) && v == w && v != denote_val(Op::Bare, cur, w)
        && read_c(s2, 0nat) == Option::Some(w) && denote_upd(Op::Bare, cur, w) == cur
}

proof fn bare_setempty_gap(w: Str, nid: nat)
    requires
        w != Seq::empty(),
    ensures
        bare_gap_holds(w, nid),
{
    let s = Map::empty().insert(0nat, Cell::Static(Seq::empty()));
    assert(read_c(s, 0nat) == Option::Some(Seq::empty()));
    let (v, s2) = arm_at(s, Class::Buf, Op::Bare, 0nat, w, nid);
    assert(nonempty_test(read_c(s, 0nat)) == false);
    assert(v == w);
    assert(denote_val(Op::Bare, read_c(s, 0nat), w) == Seq::empty());
    assert(denote_upd(Op::Bare, read_c(s, 0nat), w) == read_c(s, 0nat));
    assert(read_c(s2, 0nat) == Option::Some(w));
}

// ── Bounds: the census fold routes every word to a safe path ──────────

// c_backend.rs FIXED_BUF_CAP (a spec fn: `int` is not a legal const type)
pub open spec fn cap() -> int {
    1024
}

// `supplement_write_lengths`' MAX fold (bump_len): max with an
// existing bound, insert when missing, widen to unbounded (None) when
// either side is dynamic.
pub open spec fn bump(old: Option<int>, add: Option<int>) -> Option<int> {
    match (old, add) {
        (Option::Some(a), Option::Some(b)) => Option::Some(if a >= b {
            a
        } else {
            b
        }),
        _ => Option::None,
    }
}

// `buf_bound`: Some(b) -> `char v[b+1]`, beyond CAP -> None (char*).
pub open spec fn buf_bound(lens: Option<int>) -> Option<int> {
    match lens {
        Option::Some(b) if 0 <= b && b <= cap() => Option::Some(b),
        _ => Option::None,
    }
}

// strcpy into `char x[b+1]` needs |src| + 1 bytes.
pub open spec fn strcpy_fits(b: int, src: Str) -> bool {
    0 <= b && (src.len() as int) <= b
}

// The fold never SHRINKS an existing bound: an entry the core walk
// already produced stays Some, just widened.
proof fn bump_never_shrinks(old: Option<int>, k: int)
    requires
        old.is_Some(),
    ensures
        bump(old, Some(k)).is_Some(),
{
    match old {
        Option::Some(a) => {
            assert(bump(old, Some(k)) == Option::Some(if a >= k {
                a
            } else {
                k
            }));
        },
        Option::None => {},
    }
}

// THE routing lemma: after folding the write's word length in, either
// the name stays unbounded (buf_bound None -> the `char*` heap path,
// no buffer to overflow) or the fixed buffer FITS the default word —
// including the CAP filter (beyond CAP -> None -> heap).
proof fn bound_routes(old: Option<int>, w: Str)
    ensures
        match buf_bound(bump(old, Some(w.len() as int))) {
            Option::Some(b) => strcpy_fits(b, w),
            Option::None => true,
        },
{
    match bump(old, Some(w.len() as int)) {
        Option::Some(b) => {
            // b = max(old, |w|) >= |w|
            match old {
                Option::Some(a) => {
                    if a >= (w.len() as int) {
                        assert(b == a);
                        assert(b >= (w.len() as int));
                    } else {
                        assert(b == (w.len() as int));
                    }
                },
                Option::None => {},
            }
            assert(b >= (w.len() as int));
            match buf_bound(Option::Some(b)) {
                Option::Some(b2) => {
                    assert(b2 == b);
                    assert(strcpy_fits(b2, w));
                },
                Option::None => {},
            }
        },
        Option::None => {},
    }
}

// The Declare-init half of the supplement: `local x=1` folds 1 in on
// top of the core walk's 0 (probe wam5's exact input), so the
// `assert(strlen("1") <= b)` the decl emitter writes is satisfiable
// and the buffer fits the later `:=` default.
proof fn pin_bounds_wam5()
    ensures
        buf_bound(bump(Some(0), Some(1))) == Option::Some(1),
        strcpy_fits(1, Seq::empty().push('d')),
{
    assert(bump(Some(0), Some(1)) == Option::Some(1));
    assert(buf_bound(Option::Some(1)) == Option::Some(1));
    assert(strcpy_fits(1, Seq::empty().push('d')));
}

// ── In-file mutants (the negative controls) ───────────────────────────

// MUTANT A — the pre-fix heap store: the default word stored
// DIRECTLY (xstrdup dropped), leaving a Static literal in a slot the
// scope-end cleanup will `free`. The ownership invariant fails.
pub open spec fn arm_static_store(s: Store, n: Name, w: Str) -> Store {
    s.insert(n, Cell::Static(w))
}

proof fn mutant_static_store(s: Store, n: Name, w: Str)
    requires
        heap_valid(s),
    ensures
        !heap_valid(arm_static_store(s, n, w)),
{
    assert(arm_static_store(s, n, w)[n] == Cell::Static(w));
    assert(!heap_cell_ok(Cell::Static(w)));
    assert(arm_static_store(s, n, w).dom().contains(n));
    let bad = arm_static_store(s, n, w);
    assert(!heap_cell_ok(bad[n]));
    assert(!(forall|m: Name| bad.dom().contains(m) ==> heap_cell_ok(bad[m])));
}

// MUTANT B — the flipped op rule: `:=` treated as unset-only. It
// contradicts the denotation exactly where colon and bare differ.
pub open spec fn denote_val_flip(op: Op, cur: Option<Str>, w: Str) -> Str {
    denote_val(Op::Bare, cur, w)
}

proof fn mutant_flip_colon(w: Str)
    requires
        w != Seq::empty(),
    ensures
        denote_val_flip(Op::Colon, Option::Some(Seq::empty()), w)
            != denote_val(Op::Colon, Option::Some(Seq::empty()), w),
{
    assert(denote_val(Op::Bare, Option::Some(Seq::empty()), w) == Seq::empty());
    assert(denote_val(Op::Colon, Option::Some(Seq::empty()), w) == w);
}

// ── Fidelity pins (the probe shapes, bash vs C) ───────────────────────

// Probe wam (unset name, `:=`): the arm assigns the word, the heap
// slot becomes Owned — `unset1=[f] z=[f]` matched bash after the
// parser fix.
proof fn pin_unset_colon_assign(w: Str, nid: nat)
    ensures
        {
            let s: Store = Map::empty();
            let (v, s2) = arm_at(s, Class::Heap, Op::Colon, 0nat, w, nid);
            v == w && read_c(s2, 0nat) == Option::Some(w) && heap_valid(s2)
                && s2[0nat] == Cell::Owned(w, nid)
        },
{
    let s: Store = Map::empty();
    heap_sound(s, 0nat, Op::Colon, w, nid);
    let (v, s2) = arm_at(s, Class::Heap, Op::Colon, 0nat, w, nid);
    assert(v == w);
    assert(read_c(s2, 0nat) == Option::Some(w));
    assert(s2 == s.insert(0nat, Cell::Owned(w, nid)));
    assert(s2[0nat] == Cell::Owned(w, nid));
    assert(heap_valid(s2));
}

// Probe wam4 colon: buffer set-EMPTY, `:=` — assigns, bash agrees
// (`colon:[d][d]` on both sides).
proof fn pin_setempty_colon_buf(w: Str, nid: nat)
    requires
        w != Seq::empty(),
    ensures
        {
            let s = Map::empty().insert(0nat, Cell::Static(Seq::empty()));
            let (v, s2) = arm_at(s, Class::Buf, Op::Colon, 0nat, w, nid);
            v == w && read_c(s2, 0nat) == Option::Some(w)
        },
{
    let s = Map::empty().insert(0nat, Cell::Static(Seq::empty()));
    assert(read_c(s, 0nat) == Option::Some(Seq::empty()));
    buf_sound(s, 0nat, Op::Colon, w, nid);
    let (v, s2) = arm_at(s, Class::Buf, Op::Colon, 0nat, w, nid);
    assert(v == w);
    assert(read_c(s2, 0nat) == Option::Some(w));
}

// Probe wam (set nonempty): `${x:=d}` on x="hi" — no write at all,
// value = current bytes, the cell (id included) unchanged.
proof fn pin_set_nonempty_no_write(nid: nat)
    ensures
        {
            let cur = Option::Some(Seq::empty().push('h').push('i'));
            let s = Map::empty().insert(0nat, Cell::Owned(cur.get_Some_0(), 7nat));
            let (v, s2) = arm_at(s, Class::Heap, Op::Colon, 0nat, Seq::empty().push('d'), nid);
            v == cur.get_Some_0() && s2 == s
        },
{
    let cur = Option::Some(Seq::empty().push('h').push('i'));
    let s = Map::empty().insert(0nat, Cell::Owned(cur.get_Some_0(), 7nat));
    assert(cur.get_Some_0().len() == 2);
    assert(Seq::<char>::empty().len() == 0);
    assert(cur.get_Some_0() != Seq::empty());
    assert(nonempty_test(cur) == true);
    let (v, s2) = arm_at(s, Class::Heap, Op::Colon, 0nat, Seq::empty().push('d'), nid);
    // the test holds: no store, value re-reads cur
    assert(s2 == s);
    assert(v == cur.get_Some_0());
}

// Double-eval instances: the heap branch's two test evaluations agree
// on both sides of the test.
proof fn pin_double_eval_instances()
    ensures true
{
    let a = Seq::empty().push('a');
    // the test holds (nonempty current value)
    assert(a.len() == 1);
    assert(a != Seq::empty());
    assert(nonempty_test(Option::Some(a)) == true);
    heap_double_eval(true, Option::Some(a), Seq::empty().push('b'));
    // test fails on set-empty: store then re-read both yield the word
    heap_double_eval(false, Option::Some(Seq::empty()), Seq::empty().push('b'));
    // the unset case (heap arm's assign branch)
    heap_double_eval(false, Option::None, Seq::empty().push('b'));
}

// Probe wam3 (`${q1=/usr/local}`): after the parser fix the word is
// the whole path and the colon denotation yields it verbatim.
proof fn pin_path_word_value()
    ensures
        {
            let w: Str = Seq::empty().push('/').push('u').push('s').push('r');
            let cur = Option::None;
            arm_value(nonempty_test(cur), cur, w) == denote_val(Op::Colon, cur, w)
        },
{
    let w: Str = Seq::empty().push('/').push('u').push('s').push('r');
    value_agrees(Op::Colon, Option::None, w);
}

} // verus!

fn main() {}
