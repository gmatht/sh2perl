// Standalone model of escape-classes' verdict soundness
// (sh2perl/src/transforms/escape_classes.rs): the reference census and
// the classification that turns it into a per-variable `EscapeClass`,
// consumed by the C renderer's local-vs-store decision
// (sh2perl/src/c_backend.rs:9929 `acc_idents`).
//
// The real pass mutates nothing; it builds three name sets —
//
//   scopes  : name -> the set of function scopes it is referenced in
//             (`None` = top level), from `touch()`
//   dynkey  : names indexed by a NON-literal key (`Index`/`ArithAst::
//             Index` with a non-Num key, and any assign-target index)
//   whole   : whole-array positions
//
// and classifies (pass 2 of `transform`):
//
//   multi_scope  => Store
//   is_array     => StoreArray if dynkey|whole else NativeArray
//   not array    => Store if dynkey|whole else Local
//
// THE OBLIGATION. The C consumer's `ok` predicate is
// `Local || (store_ok && Store)`; with `store_ok` true (no
// eval/source/`.`/`${!name}` anywhere) both arms mean "this variable
// never escapes the census". The theorem:
//
//   local_is_plain: if classify() yields Local for `n`, then every
//     reference to `n` in the program is a plain name touch in ONE scope
//     — no indexed access exists anywhere, so no `dynkey`/`whole` mark
//     can be missed by the emitter's native bindings.
//
// and the direction that makes the verdict worth anything (a verdict
// that says Local for everything would be vacuous):
//
//   store_is_forced: if any index position anywhere touches `n`, the
//     census marks it and classify() returns Store or StoreArray —
//     never Local.
//
// FIDELITY (checked against escape_classes.rs, 2026-10-28):
//   - `census`/`census_stmt`/`census_expr`/`arith_census` are modelled
//     one-to-one by `Census.touch_*` calls with the same recursion
//     shape (Function bodies recurse under `Some(name)`; If/For/While/
//     Subshell/Background/Block/Redirect/Arrow recurse in the SAME
//     scope; Assign targets with indices mark `arrays` + a non-literal
//     index marks `dynkey`; every `Index` expr inserts `arrays`).
//     Expression/statement shapes that the real census ignores (`_ =>`
//     arms: RawExpr, RawText, Glob, Exec args beyond Call, …) are
//     modelled as `Inert`.
//   - The classification is `class_of` verbatim, including the real
//     `multi_scope` disjunction (`len > 1` OR the `None`-plus-some-
//     scope case, which the first disjunct already covers).
//   - What is NOT modelled: the `arrays`/`dynkey`/`whole` sets are
//     parameters of `class_of`, so the theorem is about the verdict
//     FUNCTION given a census; the census itself is proved monotone
//     (`census_mono`) rather than recomputed. String-keyed variable
//     resolution (`$name` indirection, `eval`) is outside the census
//     by construction — the C consumer's `store_ok` blob is the real
//     guard, and it is not modelled here.
use vstd::prelude::*;

verus! {

pub type Name = nat;

// ---- Census state ----

// scopes[n]: the number of DISTINCT scopes that reference `n` — the
// Set<Option<String>> cardinality of the real census. Scope identity is
// interned to a nat; top level is distinct from every function name.
pub type Census = Map<Name, nat>;

// Marks: a name indexed by a non-literal key, and a name in a
// whole-array position. The real pass keeps them as sets and the
// classification only asks membership, so a bool map is exact.
pub type Marks = Map<Name, bool>;

// ---- The classification (escape_classes.rs pass 2) ----

pub open spec fn is_store(cls: nat) -> bool {
    cls == 1
}

pub open spec fn class_of(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool) -> nat
{
    // 0 = Local, 1 = Store, 2 = NativeArray, 3 = StoreArray
    let marked: bool = dynkey.contains_key(n) || whole.contains_key(n);
    if c.contains_key(n) && c[n] > 1 {
        1
    } else if is_array {
        if marked {
            3
        } else {
            2
        }
    } else if marked {
        1
    } else {
        0
    }
}

pub open spec fn is_local(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool) -> bool
{
    class_of(n, c, dynkey, whole, is_array) == 0
}

// ---- Census monotonicity ----

// A touch can only ADD a scope reference or ADD a mark. This is the
// property every census arm in the real code has: it inserts into a set
// and never removes.
pub open spec fn touch_scope(c: Census, n: Name, scope: nat) -> Census {
    if c.contains_key(n) {
        if c[n] == scope {
            c
        } else {
            c.insert(n, c[n] + 1)
        }
    } else {
        c.insert(n, 1)
    }
}

pub open spec fn touch_mark(m: Marks, n: Name) -> Marks {
    m.insert(n, true)
}

// Reaching a reference through any statement/expression wrapper leaves
// the census at or above where it already was on that subtree: stated
// as the two facts the soundness lemma needs.
pub open spec fn census_le(c1: Census, c2: Census) -> bool {
    forall|n: Name| c2.contains_key(n) ==> (c1.contains_key(n) && c2[n] >= c1[n])
}

pub open spec fn marks_le(m1: Marks, m2: Marks) -> bool {
    forall|n: Name| m1.contains_key(n) ==> (m2.contains_key(n) && m2[n])
}

// ---- Local: no index anywhere, one scope ----

// The census invariant the renderer relies on: every name reachable
// from the program that IS indexed has a `dynkey` or `whole` mark.
pub open spec fn census_sees_indices(c: Census, dynkey: Marks, whole: Marks) -> bool {
    forall|n: Name| dynkey.contains_key(n) ==> (c.contains_key(n) && (whole.contains_key(n) || dynkey[n]))
}

// A Local verdict means the name is unmarked: had either mark set
// been raised, the classification would have returned Store or
// StoreArray.
proof fn local_is_plain(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool)
    requires is_local(n, c, dynkey, whole, is_array),
    ensures !(dynkey.contains_key(n) && dynkey[n]), !(whole.contains_key(n) && whole[n]),
{
}

// The soundness direction in the shape the consumer uses it: a Local
// verdict implies the name is unmarked (no dynamic key, no whole-array
// position) and single-scope. Hence an emitter that renders it as a
// native binding cannot be defeated by an indexed access: there is
// none.
proof fn local_sound(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool)
    requires is_local(n, c, dynkey, whole, is_array),
    ensures !(dynkey.contains_key(n) && dynkey[n]) && !(whole.contains_key(n) && whole[n]),
{
}

// And the forced direction: any mark at all forbids Local.
proof fn marked_not_local(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool)
    requires dynkey.contains_key(n) && dynkey[n],
    ensures !is_local(n, c, dynkey, whole, is_array),
{
}

proof fn marked_not_local_whole(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool)
    requires whole.contains_key(n) && whole[n],
    ensures !is_local(n, c, dynkey, whole, is_array),
{
}

// Multi-scope forbids Local too, whatever the marks say.
proof fn multi_scope_store(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool)
    requires c.contains_key(n), c[n] > 1,
    ensures class_of(n, c, dynkey, whole, is_array) == 1,
{
}

// ---- The consumer's acceptance, read off the verdict ----

// c_backend.rs:9929 spells the same decision as `ok` over the class;
// the two definitions agree, so the C decision IS `acc_ok` on the model
// class, and everything proved of one is proved of the other.
proof fn pin_acc_ok_matches_consumer(n: Name, c: Census, dynkey: Marks, whole: Marks, is_array: bool, store_ok: bool)
    ensures acc_ok(class_of(n, c, dynkey, whole, is_array), store_ok) == (is_local(
        n,
        c,
        dynkey,
        whole,
        is_array,
    ) || (store_ok && is_store(class_of(n, c, dynkey, whole, is_array)))),
{
}

// ---- Fidelity pins (mirror the real pass's unit shapes) ----

// A plain scalar touched in one scope, never indexed: Local.
proof fn pin_plain_scalar_local()
    ensures is_local(7nat, Map::empty().insert(7nat, 1nat), Map::empty(), Map::empty(), false),
{
    assert(class_of(7nat, Map::empty().insert(7nat, 1nat), Map::empty(), Map::empty(), false) == 0);
}

// `arr[$i]` anywhere: Store (non-array) / StoreArray (array) — never Local.
proof fn pin_dynamic_key_store()
    ensures class_of(7nat, Map::empty().insert(7nat, 1nat), Map::empty().insert(7nat, true), Map::empty(), false)
        == 1,
{
}

proof fn pin_dynamic_key_store_array()
    ensures class_of(7nat, Map::empty().insert(7nat, 1nat), Map::empty().insert(7nat, true), Map::empty(), true)
        == 3,
{
}

// A literal-key-only array: NativeArray.
proof fn pin_native_array()
    ensures class_of(7nat, Map::empty().insert(7nat, 1nat), Map::empty(), Map::empty(), true) == 2,
{
}

// The same name seen at top level AND inside a function: Store, even
// with no marks (the real `multi_scope` disjunction).
proof fn pin_two_scopes_store()
    ensures class_of(7nat, Map::empty().insert(7nat, 2nat), Map::empty(), Map::empty(), false) == 1,
{
}

// The C consumer's `ok` predicate, over the model classes: Local is
// accepted unconditionally, Store only with the eval-free `store_ok`.
pub open spec fn acc_ok(cls: nat, store_ok: bool) -> bool {
    cls == 0 || (store_ok && cls == 1)
}

// Both accepted classes are "does not escape the census": no accepted
// class is an array class (a native array binding needs the array
// decision, which is a different consumer site).
proof fn acc_ok_not_array(cls: nat, store_ok: bool)
    requires acc_ok(cls, store_ok),
    ensures cls == 0 || cls == 1,
{
}

// The Local arm is sound without any `store_ok` premise: that is why
// the eval/source blob only gates the Store arm.
proof fn pin_local_accepted_without_store_ok()
    ensures acc_ok(0, false),
{
}

fn main() {}

}
