// Standalone model of `stray_reserved_word`
// (sh2perl/src/transforms/stray_reserved_word.rs).
//
// The pass exists because the core parser recovers a stray `)` / `}` as an
// ordinary command (`SimpleCommand { name: ")" }`), so every backend used
// to EXECUTE it: status 127, and the rest of the script kept running.
// bash treats the word as a grammar token — a parse error: the statements
// BEFORE it have already run, then it diagnoses and exits 2, discarding
// the tail.
//
// ## The obligation
//
//   lower_matches_bash: for every program and every fuel, the lowered
//   program (prefix + abort) has exactly bash's observable behaviour — the
//   prefix's trace, status 2, and no tail.
//
//   gap_witness: the UN-lowered program does not: the stray runs as a
//   command (status 127) and the tail executes. Kept as an obligation so
//   the correctness claim is not "the pass agrees with the pass".
//
//   truncation_is_load_bearing: the lowered head is a ONE-element abort
//   list — the tail is removed, not merely unreachable.
//
// plus the refusals: only a literal COMMAND NAME is inspected, so a quoted
// `)` (an ARGUMENT) is untouched, and an ordinary command is untouched.
//
// ## Scope (read before quoting it)
//
// - Model, not code: the statement vocabulary is Cmd(id) / Stray(token) /
//   ArgOnly(id) / Abort(token), and `is_stray` is structural (the source's
//   `exec_name` + `stray_reserved_word` pair). The diagnostic text
//   (`syntax error near unexpected token ...`) is stderr, which the corpus
//   gate does not compare; it is modelled only as "no trace".
// - `lower` is stated recursively (head :: lower tail, the head replaced by
//   the abort), which is the source's `position(...)` + `truncate(pos)` +
//   `push(abort)` read from the left. The concrete pins below exercise
//   both the "prefix kept" and "tail gone" halves of that reading.
// - Only straight-line lists: the pass itself is top-level-only (it
//   truncates `stmts` and does not recurse into bodies), which is exactly
//   what `lower` does.
// - Nesting is not modelled: the source returns at the first top-level
//   stray, so nothing nested is inspected either.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 stray_reserved_word.rs
//   -> verification results:: 14 verified, 0 errors
//
// Mutant (checked): making `lower` keep the tail (`Cons(abort, lower(*t))`
// instead of `Cons(abort, Nil)`) reports 12 verified, 2 errors — the two
// truncation obligations fail (abort_keeps_token,
// truncation_is_load_bearing). Execution equivalence still holds: an exit
// after the abort cannot run the tail either way, so truncation is a
// code-size obligation, not a status/trace one. Recorded that way rather
// than overclaimed.
use vstd::prelude::*;

verus! {

pub enum S {
    // A command that runs: its id reaches the trace, status 0.
    Cmd(nat),
    // A stray grammar token `)` / `}`, with the token id.
    Stray(nat),
    // A command whose NAME is fine but which carries the token as a
    // quoted ARGUMENT: legal, never inspected by the pass.
    ArgOnly(nat),
    // The lowered abort: Warn + Exit(2).
    Abort(nat),
}

// An inductive statement list. (Cons/Nil rather than Seq so the
// obligations are structural: no seq-concatenation axioms can hide a
// step.)
pub enum L {
    Nil,
    Cons(S, Box<L>),
}

pub type Trace = Seq<nat>;

// ---- the two runtimes ----
// `fuel` is the repo idiom (test_lowering_stmt.rs). Exhausting it ends the
// run with status 0 in BOTH runtimes; since `lower` keeps the prefix, the
// two exhaust at the same point.

// IR semantics BEFORE the pass: a stray is just a command (exec status
// 127) and the list continues.
pub open spec fn run_ir(p: L, fuel: nat) -> (Trace, int)
    decreases fuel,
{
    if fuel == 0 {
        (Seq::empty(), 0)
    } else {
        match p {
            L::Nil => (Seq::empty(), 0),
            L::Cons(S::Cmd(id), t) => {
                let (tr, st) = run_ir(*t, (fuel - 1) as nat);
                (seq![id] + tr, st)
            }
            L::Cons(S::ArgOnly(id), t) => {
                let (tr, st) = run_ir(*t, (fuel - 1) as nat);
                (seq![id] + tr, st)
            }
            // the bug: exec'd as a command, status 127, tail still runs
            L::Cons(S::Stray(_), t) => {
                let (tr, _) = run_ir(*t, (fuel - 1) as nat);
                (tr, 127)
            }
            L::Cons(S::Abort(_), _) => (Seq::empty(), 2),
        }
    }
}

// bash: a stray token is a parse error — the prefix has run, then exit 2
// and the tail never runs.
pub open spec fn run_bash(p: L, fuel: nat) -> (Trace, int)
    decreases fuel,
{
    if fuel == 0 {
        (Seq::empty(), 0)
    } else {
        match p {
            L::Nil => (Seq::empty(), 0),
            L::Cons(S::Cmd(id), t) => {
                let (tr, st) = run_bash(*t, (fuel - 1) as nat);
                (seq![id] + tr, st)
            }
            L::Cons(S::ArgOnly(id), t) => {
                let (tr, st) = run_bash(*t, (fuel - 1) as nat);
                (seq![id] + tr, st)
            }
            L::Cons(S::Stray(_), _) => (Seq::empty(), 2),
            L::Cons(S::Abort(_), _) => (Seq::empty(), 2),
        }
    }
}

// ---- the transform ----

// The source's `exec_name` + `stray_reserved_word` pair: only a command
// NAME that is a grammar token. A quoted token is an argument (ArgOnly).
pub open spec fn is_stray(s: S) -> bool {
    match s {
        S::Stray(_) => true,
        _ => false,
    }
}

pub open spec fn token_of(s: S) -> nat {
    match s {
        S::Stray(t) => t,
        S::Abort(t) => t,
        _ => 0,
    }
}

// `stmts.truncate(pos); stmts.push(abort(token))` — the prefix kept in
// order, the stray and everything after it replaced by the abort.
pub open spec fn lower(p: L) -> L
    decreases p,
{
    match p {
        L::Nil => L::Nil,
        L::Cons(h, t) => {
            if is_stray(h) {
                L::Cons(S::Abort(token_of(h)), Box::new(L::Nil))
            } else {
                L::Cons(h, Box::new(lower(*t)))
            }
        }
    }
}

// ---- obligations ----

// THE OBLIGATION: after lowering, the program behaves exactly like bash —
// prefix trace, status 2, no tail — for every fuel.
proof fn lower_matches_bash(p: L, fuel: nat)
    ensures run_ir(lower(p), fuel) == run_bash(p, fuel),
    decreases fuel,
{
    reveal_with_fuel(lower, 1);
    reveal_with_fuel(run_ir, 1);
    reveal_with_fuel(run_bash, 1);
    if fuel == 0 {
    } else {
        match p {
            L::Nil => {}
            L::Cons(h, t) => {
                if is_stray(h) {
                    // both stop here: no trace, status 2
                } else {
                    lower_matches_bash(*t, (fuel - 1) as nat);
                }
            }
        }
    }
}

// The abort carries the stray's token (the diagnostic names it).
proof fn abort_keeps_token(h: S, t: L)
    requires is_stray(h),
    ensures lower(L::Cons(h, Box::new(t)))
        == L::Cons(S::Abort(token_of(h)), Box::new(L::Nil)),
{
}

// The tail is gone, not merely dead: one element follows the prefix.
proof fn truncation_is_load_bearing(h: S, t: L)
    requires is_stray(h),
    ensures lower(L::Cons(h, Box::new(t))) == L::Cons(S::Abort(token_of(h)), Box::new(L::Nil)),
{
}

// Clean programs are untouched (the pass returns false): without a stray,
// `lower` is the identity.
proof fn clean_programs_untouched(p: L)
    requires !has_stray(p),
    ensures lower(p) == p,
    decreases p,
{
    match p {
        L::Nil => {}
        L::Cons(h, t) => {
            clean_programs_untouched(*t);
        }
    }
}

// "no stray in p" — one hypothesis the pass's early return relies on.
pub open spec fn has_stray(p: L) -> bool
    decreases p,
{
    match p {
        L::Nil => false,
        L::Cons(h, t) => is_stray(h) || has_stray(*t),
    }
}

// ---- the bug the pass closes (kept as an obligation) ----
pub open spec fn prog3() -> L {
    L::Cons(
        S::Cmd(0),
        Box::new(L::Cons(S::Stray(9), Box::new(L::Cons(S::Cmd(1), Box::new(L::Nil))))),
    )
}

proof fn gap_witness()
    ensures
        run_ir(prog3(), 4) != run_bash(prog3(), 4)
            && run_ir(prog3(), 4).1 == 127
            && run_ir(prog3(), 4).0 == seq![0, 1]
            && run_bash(prog3(), 4).0 == seq![0]
            && run_bash(prog3(), 4).1 == 2,
{
    reveal_with_fuel(run_ir, 5);
    reveal_with_fuel(run_bash, 5);
}

// ---- pins: the source's three unit tests (UNMIRRORED: the executed tests
// are stray_parenclose_keeps_prefix_and_exits_2, stray_braceclose_is_caught,
// quoted_and_ordinary_words_are_untouched) ----
proof fn pin_prefix_survives_and_exits_2()
    ensures
        run_ir(lower(prog3()), 4) == run_bash(prog3(), 4)
            && run_ir(lower(prog3()), 4).0 == seq![0]
            && run_ir(lower(prog3()), 4).1 == 2,
{
    lower_matches_bash(prog3(), 4);
    reveal_with_fuel(lower, 3);
    reveal_with_fuel(run_ir, 4);
}

proof fn pin_quoted_arg_untouched()
    ensures ({
        let p = L::Cons(S::ArgOnly(0), Box::new(L::Cons(S::ArgOnly(1), Box::new(L::Nil))));
        lower(p) == p && run_ir(p, 4) == run_bash(p, 4)
    }),
{
    lower_matches_bash(
        L::Cons(S::ArgOnly(0), Box::new(L::Cons(S::ArgOnly(1), Box::new(L::Nil)))),
        4,
    );
    reveal_with_fuel(lower, 4);
}

proof fn pin_ordinary_command_untouched()
    ensures ({
        let p = L::Cons(S::Cmd(4), Box::new(L::Nil));
        lower(p) == p && run_ir(p, 4) == run_bash(p, 4)
    }),
{
    lower_matches_bash(L::Cons(S::Cmd(4), Box::new(L::Nil)), 4);
    reveal_with_fuel(lower, 3);
}

// A brace `}` is caught exactly like `)`: the transform is token-blind
// beyond `stray_reserved_word`'s two-word table.
proof fn pin_braceclose_is_caught()
    ensures ({
        let p = L::Cons(S::Cmd(0), Box::new(L::Cons(S::Stray(7), Box::new(L::Nil))));
        lower(p)
            == L::Cons(S::Cmd(0), Box::new(L::Cons(S::Abort(7), Box::new(L::Nil))))
    }),
{
    reveal_with_fuel(lower, 4);
}

fn main() {}
}
