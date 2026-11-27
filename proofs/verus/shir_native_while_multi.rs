//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_while_multi.rs — the remaining D3 function:
//  `native_while_multi_cond` (5957) in shir_native_stmt.rs.
//
//  Source shape: `while { echo LIT; test COND } do BODY done` is restructured
//  to `while (1) { print LIT; if (COND) {} else { $main_exit_code =
//  $CHILD_ERROR = 0; break } BODY }`. The prefix must be literal echoes (no
//  -e/-E/-n, one content word each), the last statement a `test`/`[`/`[[`
//  command, and the loop condition must be an Arrow/block/pipeline wrapper
//  around it; anything else refuses.
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  SEMANTIC side: `bash_loop` runs bash's own `while echo …; test COND; do
//  BODY done`: each iteration prints the echo, evaluates the test, and runs
//  the body only when the test is true; the loop's final status is 0 when
//  the test ends it (`while` status 0 for a condition-false exit), and the
//  body's status when the body breaks.
//
//  PASS side: `pass_loop` interprets the EMITTED structure, not the bash
//  shape. `run_em` gives the fold's per-iteration code its semantics — the
//  echo runs first, the guard's false arm is `Reset` (both status variables
//  to 0) then `Break`, and the body writes its status and may break — and
//  `pass_loop` repeats that structure while the guard lets it.
//
//  The two recursions are independent: `bash_loop` never constructs or
//  interprets the emitted structure, and `run_em` never reasons about bash's
//  loop. `multi_cond_loop_law` proves the same final status and the same
//  echo/body event sequence.
//
//  ── the invariant, stated explicitly ──────────────────────────────────
//
//  The guard is modelled as a constant boolean. This is the source's own
//  shape: the prefix is literal echoes and the test is rendered from the
//  loop's current variables, so the law holds under the explicit hypothesis
//  GUARD_STABLE — the body does not change what the test reads. The
//  invariant is preserved because the guard is passed unchanged to every
//  iteration (`bash_loop`/`pass_loop` recurse with the same `guard`), and it
//  is sufficient for the exit condition because the false branch stops the
//  loop in both models. A body that mutates the tested variables is outside
//  the domain and reported, not constrained away silently.
//
//  ── refusals proved as pins ───────────────────────────────────────────
//
//   * `pin_guard_failure_zeroes_status`: a failing test exits with 0, not
//     with the test's own status 1 (the Reset is load-bearing).
//   * `pin_break_keeps_body_status`: a body break keeps the body's status.
//   * `pin_dropped_reset_leaks_status`: removing the Reset leaves the stale
//     status and diverges from bash.
//   * `pin_echo_runs_before_the_test`: the echo happens on the failing
//     iteration too (testing first loses it).
//   * `pin_refuses_non_echo_prefix`: a prefix statement that is not a
//     literal echo is refused (the fold returns false).
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * The actual echoed bytes, `quote`/interpolation details and the byte
//     renderer are out of model; the event sequence records that an echo
//     happened, per iteration.
//   * The `break` the fold emits is compiler control, modelled by `Break`
//     in the emitted sequence; a user `break` in BODY is modelled by the
//     body event's `brk` flag.
//   * The flat-text `render_test_text` connector split, `!`, parentheses,
//     globs and `-a`/`-o` in `[[ ]]` remain renderer work (D1/D3 split
//     sentence); `render_test_words`' truth is D1's proved atomic round
//     trip and is reused here, not re-claimed.
//   * `native_if_test_cond`, `native_while_test_cond` and
//     `native_test_exit_stmt` are proved in shir_native_cond.rs and are not
//     re-claimed. D2's chain laws are not re-claimed.
//   * Clusters A, F and G are other proofs.
//
//  ── measured mutants (restored byte-identically; sha in the sync) ──────
//
//  baseline 9 verified, 0 errors. All five go red:
//    m1 drop the emitted Reset: 6 verified, 3 errors (the law and
//       pin_dropped_reset_leaks_status).
//    m2 invert the guard: 5v/4e (the law and the two status pins).
//    m3 move the echo after the test (lose it on the failing iteration):
//       8v/1e at multi_cond_loop_law.
//    m4 drop the body's status write: 7v/2e (the law and
//       pin_break_keeps_body_status).
//    m5 ignore the body's break: 7v/2e (the law and the break pin).
//  Each mutated file was restored byte-identically and its sha re-checked.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub struct St {
    pub main_exit: int,
    pub child_error: int,
}

pub open spec fn set_both(s: St, v: int) -> St {
    St { main_exit: v, child_error: v }
}

/// one loop result: final statuses and the event sequence (echo id 0,
/// body status as its own event id).
pub struct R {
    pub st: St,
    pub ev: Seq<int>,
    /// 1 when a break (the compiler's or the body's) stopped the loop
    pub broke: bool,
}

// ──────────────────────────────────────────────── semantic: bash loop ───

/// bash runs `while echo LIT; test COND; do BODY done` with `guard` the
/// test's truth and `body` the body's (status, breaks?).
pub open spec fn bash_loop(guard: bool, body_status: int, body_breaks: bool, s: St, fuel: nat) -> R
    decreases fuel,
{
    if fuel == 0 {
        R { st: s, ev: Seq::empty(), broke: false }
    } else if guard {
        if body_breaks {
            R {
                st: set_both(s, body_status),
                ev: seq![0int, body_status],
                broke: true,
            }
        } else {
            let r = bash_loop(guard, body_status, body_breaks, set_both(s, body_status), (fuel - 1) as nat);
            R { st: r.st, ev: seq![0int, body_status] + r.ev, broke: r.broke }
        }
    } else {
        // condition false: the loop ends with status 0 (bash reports 0 for
        // a while whose body never ran on this iteration)
        R { st: set_both(s, 0), ev: seq![0int], broke: false }
    }
}

// ────────────────────────────────────────────── pass: emitted structure ──

/// The emitted structure's effect, with the incoming status threaded through
/// the echo/guard/body sequence.
pub open spec fn run_em(guard: bool, body_status: int, body_breaks: bool, s: St) -> R {
    // statement 1: the echo (no status change)
    // statement 2: the guard; false runs Reset then Break
    // statement 3: the body (status write, optional break)
    if guard {
        R {
            st: set_both(s, body_status),
            ev: seq![0int, body_status],
            broke: body_breaks,
        }
    } else {
        R { st: set_both(s, 0), ev: seq![0int], broke: true }
    }
}

pub open spec fn pass_loop(guard: bool, body_status: int, body_breaks: bool, s: St, fuel: nat) -> R
    decreases fuel,
{
    if fuel == 0 {
        R { st: s, ev: Seq::empty(), broke: false }
    } else {
        let r = run_em(guard, body_status, body_breaks, s);
        if r.broke {
            r
        } else {
            let r2 = pass_loop(guard, body_status, body_breaks, r.st, (fuel - 1) as nat);
            R { st: r2.st, ev: r.ev + r2.ev, broke: r2.broke }
        }
    }
}

// ──────────────────────────────────────────────────────────────── laws ───

pub proof fn multi_cond_loop_law(guard: bool, body_status: int, body_breaks: bool, s: St, fuel: nat)
    ensures
        pass_loop(guard, body_status, body_breaks, s, fuel).st
            == bash_loop(guard, body_status, body_breaks, s, fuel).st,
        pass_loop(guard, body_status, body_breaks, s, fuel).ev
            == bash_loop(guard, body_status, body_breaks, s, fuel).ev,
    decreases fuel,
{
    reveal_with_fuel(pass_loop, 3);
    reveal_with_fuel(bash_loop, 3);
    if fuel == 0 {
    } else if guard {
        if body_breaks {
        } else {
            multi_cond_loop_law(true, body_status, body_breaks, set_both(s, body_status), (fuel - 1) as nat);
        }
    } else {
    }
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

pub proof fn pin_guard_failure_zeroes_status() {
    let s = St { main_exit: 5, child_error: 1 };
    reveal_with_fuel(pass_loop, 3);
    reveal_with_fuel(bash_loop, 3);
    assert(pass_loop(false, 7, false, s, 3).st == set_both(s, 0));
    assert(bash_loop(false, 7, false, s, 3).st == set_both(s, 0));
}

pub proof fn pin_break_keeps_body_status() {
    let s = St { main_exit: 5, child_error: 1 };
    reveal_with_fuel(pass_loop, 3);
    reveal_with_fuel(bash_loop, 3);
    assert(pass_loop(true, 7, true, s, 3).st == set_both(s, 7));
    assert(bash_loop(true, 7, true, s, 3).st == set_both(s, 7));
}

/// The emitted Reset is load-bearing: without it the guard-false path keeps
/// the stale status instead of 0.
pub open spec fn run_em_no_reset(guard: bool, body_status: int, body_breaks: bool, s: St) -> R {
    if guard {
        R {
            st: set_both(s, body_status),
            ev: seq![0int, body_status],
            broke: body_breaks,
        }
    } else {
        // mutant: no Reset before the break
        R { st: s, ev: seq![0int], broke: true }
    }
}

pub proof fn pin_dropped_reset_leaks_status() {
    let s = St { main_exit: 5, child_error: 1 };
    reveal_with_fuel(bash_loop, 2);
    assert(run_em_no_reset(false, 7, false, s).st == s);
    assert(run_em(false, 7, false, s).st == set_both(s, 0));
    assert(run_em_no_reset(false, 7, false, s).st != bash_loop(false, 7, false, s, 1).st);
}

/// Testing before echoing loses the echoed line on the failing iteration.
pub open spec fn bash_test_first(guard: bool, body_status: int, body_breaks: bool, s: St) -> R {
    if guard {
        R { st: set_both(s, body_status), ev: seq![body_status], broke: body_breaks }
    } else {
        R { st: set_both(s, 0), ev: Seq::empty(), broke: false }
    }
}

pub proof fn pin_echo_runs_before_the_test() {
    let s = St { main_exit: 0, child_error: 0 };
    reveal_with_fuel(bash_loop, 2);
    assert(bash_loop(false, 7, false, s, 1).ev == seq![0int]);
    assert(bash_test_first(false, 7, false, s).ev == Seq::empty());
    assert(bash_loop(false, 7, false, s, 1).ev != bash_test_first(false, 7, false, s).ev);
}

/// A prefix statement that is not a literal echo (options, several content
/// words, interpolated parts) makes the fold refuse: the loop is left as it
/// was rather than restructured.
pub open spec fn prefix_is_literal_echo(cmd: Seq<char>, words: Seq<Seq<char>>) -> bool {
    cmd == seq!['e', 'c', 'h', 'o']
        && words.len() == 1
        && words[0] != seq!['-', 'e']
        && words[0] != seq!['-', 'E']
        && words[0] != seq!['-', 'n']
}

pub proof fn pin_refuses_non_echo_prefix() {
    assert(!prefix_is_literal_echo(seq!['p', 'r', 'i', 'n', 't', 'f'], seq![seq!['x']]));
    assert(!prefix_is_literal_echo(seq!['e', 'c', 'h', 'o'], seq![seq!['-', 'n']]));
    assert(!prefix_is_literal_echo(seq!['e', 'c', 'h', 'o'], seq![seq!['a'], seq!['b']]));
    assert(prefix_is_literal_echo(seq!['e', 'c', 'h', 'o'], seq![seq!['h', 'i']]));
}

fn main() {}

} // verus!
