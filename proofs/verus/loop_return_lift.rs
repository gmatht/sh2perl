// Standalone model of loop-return-lift
// (sh2perl/src/transforms/loop_return_lift.rs): an echo+return inside a loop
// body is rewritten from
//
//   loop { if C; then echo "$v"; return; fi }   [post]
//
// to
//
//   local flag=0; local ret
//   loop { if C; then ret="$v"; flag=1; break; fi }
//   if (( flag == 1 )); then echo "$ret"; else [post]; fi
//
// Proves the soundness obligation:
//
//   the_lift_preserves_the_observable: for EVERY loop (any iteration sequence,
//   any post-loop statements, any trailing statements) and every placement,
//   the lifted program emits exactly the reference bytes — with the verdict
//   `census` computed FROM the placement, so a refused program takes both
//   sides with the flag `false`.
//
// ## What is modelled
//
// An iteration is `Iter { hit, val }`: `hit` is the If's condition and `val`
// the echoed word's value AT THAT ITERATION. A loop is a `Seq<Iter>`; the
// reference walks it by index and prints `it[k].val` at the FIRST hit, then
// returns (leaves the function). The rewrite runs the loop to `flag`/`ret`:
// `loop_run` stops at the first hit with the value captured there, or runs to
// the end with the flag still `false`. The post-loop `if` then prints the
// capture (flag 1) or runs the moved `post` statements (flag 0).
//
// The placement is three-valued in effect, but is computed from two inputs:
//   in_function  the loop is inside a function body
//   tail         the bytes produced by whatever follows the enclosing block
//                at function level (empty exactly when the block is in tail
//                position)
//   post         the statements after the loop in its own block
//
//   census(in_function, tail) = in_function && tail.len() == 0
//
// A refused program (top level, or a non-tail function loop) is unchanged:
// the flag is false on both sides.
//
// ## The three load-bearing rules, one obligation each
//
//  L1 the_lift_preserves_the_observable — the census is sound: every accepted
//     program's lifted bytes equal the reference bytes.
//  L2 captures_the_matching_iteration — the value printed after the loop is
//     the one the original echo saw at the FIRST matching iteration.
//  L3 stops_at_the_first_match — the flag is set exactly when an iteration
//     matched, and the loop stops there (a later match never prints).
//
// ## What is NOT modelled
//
//   - The TRIGGER conditions: the then arm must be exactly
//     `[echo "$v", return]` with a pure `$v`, the else arm silent, and the
//     `$v` word quoted. Those are the recognition, not the law; the law holds
//     for every iteration/value sequence the lift is applied to.
//   - `local` scoping / variable lifetime (the capture is function-scoped and
//     read only on the found path — a renderer contract).
//   - The estree path: there is no JS engine here, so only the C renderer is
//     anchored byte-for-byte (see the transform's tests and the harness
//     anchor below).
//
// ## Defects the model forced (all repaired in the transform; tests pin them)
//
//   1. The echo value was re-evaluated AFTER the loop, so the matchers'
//      `for f in ...` printed the element after the match (`found 3` where
//      bash prints `found 2`). Repaired by capturing at the echo point.
//      Test: the_value_is_captured_at_the_matching_iteration.
//   2. The lift was applied to a loop that was not in tail position, where
//      the original `return` skipped the following statements but the new
//      `break` does not (an `echo unreachable` ran). Repaired with the
//      `tail` conjunct. Test: a_loop_that_is_not_in_tail_position_is_not_lifted.
//   3. The lift was applied at the TOP LEVEL, where `local` is invalid and
//      `return` does not end the script (the C output failed to build with
//      `redefinition of '__sh2_found'`). Repaired with the `in_function`
//      conjunct. Test: a_loop_outside_a_function_is_not_lifted.
//
// ## Mutants (checked red, each a real edit of this file)
//
//   identity / no-lift (census false)      -> a_tail_loop_in_a_function_is_flagged
//   census widening: drop `tail == 0`      -> L1 at the non-tail witness
//   census widening: drop `in_function`    -> L1 at the top-level witness
//   census widening: accept everything     -> L1 at both witnesses
//   drop the flag/break stop rule          -> L3 + L1
//   keep iterating after the hit (the
//   first capture still wins)              -> the_break_really_stops_the_loop
//   capture the next iteration's value     -> L2 + L1
//   drop the post arm                      -> L1 at the no-hit witness
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 loop_return_lift.rs
//   -> verification results:: 22 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Str = Seq<u8>;

/// One iteration, as the lifted echo+return sees it.
pub struct Iter {
    /// the If's condition at this iteration
    pub hit: bool,
    /// the echoed word's value at this iteration
    pub val: Str,
}

pub open spec fn s(b: u8) -> Str {
    seq![b]
}

pub open spec fn empty_str() -> Str {
    Seq::<u8>::empty()
}

/// `a` followed by `b`; appending nothing is the identity, definitionally.
pub open spec fn cat(a: Str, b: Str) -> Str {
    if b.len() == 0 {
        a
    } else {
        a + b
    }
}

/// The first matching iteration at or after `i` (`len` if none).
pub open spec fn first_hit(it: Seq<Iter>, i: int) -> int
    decreases it.len() - i,
{
    if i >= it.len() {
        it.len() as int
    } else if it[i].hit {
        i
    } else {
        first_hit(it, i + 1)
    }
}

/// The reference semantics, inside the loop: at the first hit print that
/// iteration's value and leave the function; otherwise the loop completes
/// and emits nothing.
pub open spec fn eager(it: Seq<Iter>, i: int) -> Str
    decreases it.len() - i,
{
    if i >= it.len() {
        empty_str()
    } else if it[i].hit {
        it[i].val
    } else {
        eager(it, i + 1)
    }
}

/// The lifted loop's exit state: the flag, the captured value, and how many
/// iterations actually ran.
pub struct Exit {
    pub flag: bool,
    pub ret: Str,
    pub ran: nat,
}

/// The rewritten loop body: on a hit capture the value, set the flag and
/// BREAK (the loop stops — later matches never run); otherwise keep going
/// with the flag/ret unchanged.
pub open spec fn loop_run(it: Seq<Iter>, i: int, flag: bool, ret: Str, ran: nat) -> Exit
    decreases it.len() - i,
{
    if i >= it.len() {
        Exit { flag: flag, ret: ret, ran: ran }
    } else if it[i].hit {
        Exit { flag: true, ret: it[i].val, ran: ran + 1 }
    } else {
        loop_run(it, i + 1, flag, ret, ran + 1)
    }
}

/// The whole program's reference bytes.
///
/// A hit inside a function returns: neither `post` nor `tail` runs. At top
/// level `return` is an error and the script CONTINUES, so `post` and `tail`
/// both run. With no hit the loop falls through to `post`, then `tail`.
pub open spec fn reference(in_function: bool, it: Seq<Iter>, post: Str, tail: Str) -> Str {
    let k = first_hit(it, 0);
    if k < it.len() {
        if in_function {
            it[k].val
        } else {
            cat(cat(it[k].val, post), tail)
        }
    } else {
        cat(post, tail)
    }
}

/// The lifted program's bytes: the loop to `flag`/`ret`, then the post-loop
/// `if` (capture on the found path, the moved `post` on the fall-through
/// path), then whatever follows the block.
pub open spec fn rewritten(it: Seq<Iter>, post: Str, tail: Str) -> Str {
    let e = loop_run(it, 0, false, empty_str(), 0);
    cat(if e.flag { e.ret } else { post }, tail)
}

/// The verdict, computed from the placement: the lift is sound only inside a
/// function with nothing following the enclosing block, because `break`
/// leaves only the loop while `return` leaves the function.
pub open spec fn census(in_function: bool, tail: Str) -> bool {
    in_function && tail.len() == 0
}

/// One equation for every input: the flag selects the strategy and is
/// computed from the input.
pub open spec fn go(in_function: bool, it: Seq<Iter>, post: Str, tail: Str, lift: bool) -> Str {
    if lift {
        rewritten(it, post, tail)
    } else {
        reference(in_function, it, post, tail)
    }
}

// ---------------------------------------------------------------------------
// Structural facts about the iteration sequence
// ---------------------------------------------------------------------------

proof fn first_hit_is_at_least(it: Seq<Iter>, i: int)
    requires
        0 <= i <= it.len(),
    ensures
        first_hit(it, i) >= i,
    decreases it.len() - i,
{
    reveal_with_fuel(first_hit, 2);
    if i >= it.len() {
        assert(first_hit(it, i) == it.len());
    } else if it[i].hit {
        assert(first_hit(it, i) == i);
    } else {
        first_hit_is_at_least(it, i + 1);
        assert(first_hit(it, i) == first_hit(it, i + 1));
    }
}

/// `first_hit` never runs past the end of the sequence.
proof fn first_hit_is_at_most(it: Seq<Iter>, i: int)
    requires
        0 <= i,
    ensures
        first_hit(it, i) <= it.len(),
    decreases it.len() - i,
{
    reveal_with_fuel(first_hit, 2);
    if i >= it.len() {
        assert(first_hit(it, i) == it.len() as int);
    } else if it[i].hit {
    } else {
        first_hit_is_at_most(it, i + 1);
        assert(first_hit(it, i) == first_hit(it, i + 1));
    }
}

/// If something matches, the run stops at the FIRST match with that
/// iteration's value in `ret` and the flag set.
proof fn loop_run_finds_the_hit(it: Seq<Iter>, i: int, flag: bool, ret: Str, ran: nat)
    requires
        0 <= i <= it.len(),
        first_hit(it, i) < it.len(),
    ensures
        loop_run(it, i, flag, ret, ran).flag,
        loop_run(it, i, flag, ret, ran).ret == it[first_hit(it, i)].val,
    decreases it.len() - i,
{
    first_hit_is_at_least(it, i);
    reveal_with_fuel(first_hit, 2);
    reveal_with_fuel(loop_run, 2);
    if i >= it.len() {
        assert(false);
    } else if it[i].hit {
        assert(first_hit(it, i) == i);
    } else {
        assert(first_hit(it, i) == first_hit(it, i + 1));
        loop_run_finds_the_hit(it, i + 1, flag, ret, ran + 1);
        assert(loop_run(it, i, flag, ret, ran) == loop_run(it, i + 1, flag, ret, ran + 1));
    }
}

/// If nothing matches, the loop runs to the end and the exit state is the
/// entry state (flag still `false`).
proof fn loop_run_exhausts_the_loop(it: Seq<Iter>, i: int, flag: bool, ret: Str, ran: nat)
    requires
        0 <= i <= it.len(),
        first_hit(it, i) == it.len(),
    ensures
        loop_run(it, i, flag, ret, ran).flag == flag,
        loop_run(it, i, flag, ret, ran).ret == ret,
    decreases it.len() - i,
{
    reveal_with_fuel(first_hit, 2);
    reveal_with_fuel(loop_run, 2);
    if i >= it.len() {
    } else {
        assert(!it[i].hit);
        assert(first_hit(it, i) == first_hit(it, i + 1));
        loop_run_exhausts_the_loop(it, i + 1, flag, ret, ran + 1);
        assert(loop_run(it, i, flag, ret, ran) == loop_run(it, i + 1, flag, ret, ran + 1));
    }
}

// ---------------------------------------------------------------------------
// The laws
// ---------------------------------------------------------------------------

/// L1: THE LAW. For every placement, iteration sequence, post-loop body and
/// trailing body, an ACCEPTED program emits exactly the reference bytes. A
/// refused program is `go(..., false)` on both sides.
proof fn the_lift_preserves_the_observable(
    in_function: bool,
    it: Seq<Iter>,
    post: Str,
    tail: Str,
)
    ensures
        go(in_function, it, post, tail, census(in_function, tail)) == go(
            in_function,
            it,
            post,
            tail,
            false,
        ),
{
    if census(in_function, tail) {
        assert(in_function);
        assert(tail.len() == 0);
        let k = first_hit(it, 0);
        if k < it.len() {
            first_hit_is_at_least(it, 0);
            loop_run_finds_the_hit(it, 0, false, empty_str(), 0);
            // rewritten: cat(it[k].val, tail), and tail is empty
            assert(rewritten(it, post, tail) == it[k].val);
            assert(reference(in_function, it, post, tail) == it[k].val);
        } else {
            first_hit_is_at_most(it, 0);
            assert(k == it.len());
            loop_run_exhausts_the_loop(it, 0, false, empty_str(), 0);
            // both sides are cat(post, tail)
            assert(rewritten(it, post, tail) == cat(post, tail));
            assert(reference(in_function, it, post, tail) == cat(post, tail));
        }
    } else {
        // refused: the flag is `false`, both sides are the reference
    }
}

/// L2: the value the lifted program prints is the one the original `echo`
/// saw at the first matching iteration — captured THERE, not re-evaluated
/// after the loop.
proof fn captures_the_matching_iteration(it: Seq<Iter>, post: Str, tail: Str)
    ensures
        first_hit(it, 0) < it.len() ==> rewritten(it, post, tail) == cat(
            it[first_hit(it, 0)].val,
            tail,
        ),
        first_hit(it, 0) == it.len() ==> rewritten(it, post, tail) == cat(post, tail),
{
    let k = first_hit(it, 0);
    if k < it.len() {
        first_hit_is_at_least(it, 0);
        loop_run_finds_the_hit(it, 0, false, empty_str(), 0);
        assert(rewritten(it, post, tail) == cat(it[k].val, tail));
    } else {
        first_hit_is_at_most(it, 0);
        assert(k == it.len());
        loop_run_exhausts_the_loop(it, 0, false, empty_str(), 0);
        assert(rewritten(it, post, tail) == cat(post, tail));
    }
}

/// L3: the flag records exactly whether an iteration matched — the stop rule
/// and the else-arm condition are the same fact.
proof fn stops_at_the_first_match(it: Seq<Iter>)
    ensures
        first_hit(it, 0) < it.len() ==> loop_run(it, 0, false, empty_str(), 0).flag,
        first_hit(it, 0) == it.len() ==> !loop_run(it, 0, false, empty_str(), 0).flag,
{
    if first_hit(it, 0) < it.len() {
        loop_run_finds_the_hit(it, 0, false, empty_str(), 0);
    } else {
        first_hit_is_at_most(it, 0);
        assert(first_hit(it, 0) == it.len());
        loop_run_exhausts_the_loop(it, 0, false, empty_str(), 0);
    }
}

// ---------------------------------------------------------------------------
// Witnesses
// ---------------------------------------------------------------------------

pub open spec fn mk(hit: bool, b: u8) -> Iter {
    Iter { hit: hit, val: s(b) }
}

/// hits at 1 and 2: the first value is 2 and the LAST is 3, so a wrong
/// iteration or a missing break is observable.
pub open spec fn w_tail() -> Seq<Iter> {
    seq![mk(false, 1), mk(true, 2), mk(true, 3)]
}

/// no hit at all: the loop falls through to `post`.
pub open spec fn w_nohit() -> Seq<Iter> {
    seq![mk(false, 4), mk(false, 5)]
}

// ---------------------------------------------------------------------------
// Completeness / non-vacuity: the verdict is a decision computed from the
// placement, not a constant.
// ---------------------------------------------------------------------------

/// A tail loop in a function IS accepted (an identity policy is red here).
proof fn a_tail_loop_in_a_function_is_flagged()
    ensures
        census(true, empty_str()),
        !census(false, empty_str()),
        !census(true, s(9)),
{
}

/// Statements following the block are not skipped by a `break`, so a
/// non-tail loop is refused.
proof fn a_loop_followed_by_statements_is_not_flagged()
    ensures
        !census(true, s(9)),
{
}

/// A top-level loop is refused (`local` is invalid there, and `return` does
/// not end the script).
proof fn a_top_level_loop_is_not_flagged()
    ensures
        !census(false, empty_str()),
{
}

/// The accepted witness really prints the matching value, and the hook only
/// doubles as a decision when both answers are reachable.
proof fn the_lift_prints_the_matching_value()
    ensures
        go(true, w_tail(), s(7), empty_str(), true) == s(2),
        go(true, w_nohit(), s(7), empty_str(), true) == s(7),
{
    reveal_with_fuel(loop_run, 8);
}

/// The post arm runs EXACTLY when the loop completes: on a hit it is skipped
/// (the original `return` skipped it), on fall-through it runs.
proof fn the_post_arm_runs_exactly_when_the_loop_completes()
    ensures
        go(true, w_tail(), s(7), empty_str(), true) == s(2),
        go(true, w_nohit(), s(7), empty_str(), true) == s(7),
        census(true, empty_str()),
{
    reveal_with_fuel(loop_run, 8);
}

/// The break really stops: on `w_tail` (hit at 1) only two iterations ran, so
/// a "keep iterating after the hit" implementation is red here.
proof fn the_break_really_stops_the_loop()
    ensures
        loop_run(w_tail(), 0, false, empty_str(), 0).ran == 2,
        loop_run(w_nohit(), 0, false, empty_str(), 0).ran == 2,
{
    reveal_with_fuel(loop_run, 8);
}

/// The value must come from the matching iteration: the loop variable after
/// the break sits on the NEXT element, so re-reading it would print 3.
proof fn the_value_after_the_match_is_a_different_iteration()
    ensures
        first_hit(w_tail(), 0) == 1,
        w_tail()[1].val == s(2),
        w_tail()[2].val == s(3),
{
    reveal_with_fuel(first_hit, 8);
}

// ---------------------------------------------------------------------------
// Pins: each census rule is LOAD-BEARING — dropping it makes the law false on
// a concrete placement, so the rule is what keeps L1 sound. Both sides are
// stated explicitly (reference vs lifted) so the difference is not hidden in
// the flag.
// ---------------------------------------------------------------------------

/// Dropping `tail.len() == 0` (accepting a non-tail function loop): the
/// reference prints 2 and returns, the lift prints 2 then the following 9.
proof fn a_non_tail_loop_would_differ()
    ensures
        go(true, w_tail(), s(7), s(9), false) == s(2),
        go(true, w_tail(), s(7), s(9), true) == s(2) + s(9),
        (s(2) + s(9)).len() != s(2).len(),
        !census(true, s(9)),
{
    reveal_with_fuel(first_hit, 8);
    reveal_with_fuel(loop_run, 8);
}

/// Dropping `in_function` (accepting a top-level loop): the failed `return`
/// makes the script continue through `post` and `tail`, while the lifted
/// `break` skips `post`.
proof fn a_top_level_loop_would_differ()
    ensures
        go(false, w_tail(), s(7), s(9), false) == s(2) + s(7) + s(9),
        go(false, w_tail(), s(7), s(9), true) == s(2) + s(9),
        (s(2) + s(7) + s(9)).len() != (s(2) + s(9)).len(),
        !census(false, s(9)),
{
    reveal_with_fuel(first_hit, 8);
    reveal_with_fuel(loop_run, 8);
}

/// Dropping the post arm loses the fall-through output.
proof fn dropping_the_post_arm_would_lose_bytes()
    ensures
        go(true, w_nohit(), s(7), empty_str(), false) == s(7),
        !census(true, s(9)),
{
    reveal_with_fuel(first_hit, 8);
}

/// L1 instantiated with no lemma in between: a widened census makes a
/// conjunct false HERE, so the failure is pinned on the law itself.
proof fn law_at_the_witnesses()
    ensures
        go(true, w_tail(), s(7), empty_str(), census(true, empty_str())) == go(
            true,
            w_tail(),
            s(7),
            empty_str(),
            false,
        ),
        go(true, w_tail(), s(7), s(9), census(true, s(9))) == go(
            true,
            w_tail(),
            s(7),
            s(9),
            false,
        ),
        go(false, w_tail(), s(7), s(9), census(false, s(9))) == go(
            false,
            w_tail(),
            s(7),
            s(9),
            false,
        ),
{
    the_lift_preserves_the_observable(true, w_tail(), s(7), empty_str());
    the_lift_preserves_the_observable(true, w_tail(), s(7), s(9));
    the_lift_preserves_the_observable(false, w_tail(), s(7), s(9));
}

fn main() {}

}
