// Standalone model of the case-lowering Tier-1 obligations
// (`sh2perl/src/transforms/case_lowering.rs`): lowering an all-static `case`
// dispatch to an if/elif chain over the shared native string-test
// primitives preserves the first-match-wins dispatch.
//
// Proves the soundness obligations:
//
//   pat_sound: each lowered per-pattern condition denotes the source
//   pattern's match (Exact via Not(Ne) = equality — the test_lowering
//   encoding; Prefix/Suffix/Substr via the shared affix primitives;
//   Glob via the backend's own glob engine, same engine both sides).
//   dispatch_sound: the lowered if/elif chain (catch-all = else,
//   post-catch-all dropped) selects the same branch as the source case
//   for every subject string.
//
// Strings are Seq<char> (char-based, like the runtime helpers and the
// test_lowering model, whose str_prefix/str_suffix/str_contains
// definitions are reused verbatim). Clause bodies are nat ids; evaluation
// returns the selected body (None = no branch taken = empty else).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - the exact per-class lowering: Exact -> Not(Ne) (denotes ==),
//     Prefix/Suffix/Substr -> strHasPrefix/strHasSuffix/contains,
//     Glob -> globMatch(value, pattern) verbatim, Any -> else-arm.
//     Multi-pattern clauses OR their conditions (split_case_alternatives).
//   - the Glob arm is target-gated (estree/rs/rust/java accept; everything
//     else refuses the whole case) — modelled as a `glob_ok` parameter.
//     The glob MATCHER is the backend's own engine on both renders, so it
//     is an `arbitrary()` oracle called identically on both sides: the
//     theorem is that the lowering preserves the oracle call, not what any
//     engine computes.
//   - quote classification (`"*"` is a literal star, partial quotes and
//     `$`/backtick/backslash refuse, char-classes/`?`/mid-star are Glob)
//     is string PARSING — the model takes classified patterns as input
//     (Refused = whole-case refusal). The nocasematch program-wide probe
//     is a `nocase` parameter (refuse = whole-program None).
//   - the leading-catch-all refusal (no condition to bind — the chain
//     would need a vacuous cond) and post-catch-all dropping (bash's
//     first-match semantics) are both modelled.
//   - the impure-subject single-binding (`Block[Declare temp, If]`): bash
//     evaluates the case word ONCE — the model evaluates the chain against
//     the same subject string on both sides (substitution of an identical
//     value, as in copy_propagation's single-store induction). The temp
//     NAME freshness probe is a guard (presence-checked by sync), not a
//     value claim. `pure_subject`'s volatile/reserved refusals are the
//     `pure` parameter.
//   - traversal (`walk` recursion into every statement container) is out
//     of scope: each `Case` site is an independent dispatch rewrite — the
//     theorem is per-site, as in every fold here.
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// Classified pattern alternatives (the output of classify_pattern +
// split_case_alternatives; Refused = dynamic/partial-quote/escape shapes
// that refuse the whole case).
pub enum Pat {
    Exact(Str),
    Prefix(Str),
    Suffix(Str),
    Substr(Str),
    Glob(Str),
    Any,
    Refused,
}

// The backend glob engine, shared by both renders: an arbitrary matcher
// called identically on both sides (the lowering preserves the call).
pub open spec fn glob_match(s: Str, p: Str) -> bool {
    arbitrary()
}

// Prefix/suffix/contains (verbatim from the test_lowering model).
pub open spec fn str_prefix(s: Str, p: Str) -> bool {
    p.len() <= s.len() && s.subrange(0, p.len() as int) == p
}

pub open spec fn str_suffix(s: Str, p: Str) -> bool {
    p.len() <= s.len() && s.subrange(s.len() as int - p.len() as int, s.len() as int) == p
}

pub open spec fn str_contains(s: Str, p: Str) -> bool {
    exists|i: int| #[trigger] s.subrange(i, i + p.len() as int) == p && 0 <= i && i + p.len() <= s.len()
}

// Source-side match: does the pattern accept the subject?
pub open spec fn matches(p: Pat, s: Str) -> bool {
    match p {
        Pat::Exact(lit) => s == lit,
        Pat::Prefix(lit) => str_prefix(s, lit),
        Pat::Suffix(lit) => str_suffix(s, lit),
        Pat::Substr(lit) => str_contains(s, lit),
        Pat::Glob(pat) => glob_match(s, pat),
        Pat::Any => true,
        Pat::Refused => false,
    }
}

// Lowered per-pattern conditions (one disjunct per accepted pattern;
// the Exact arm is the Not(Ne) encoding, which denotes equality).
pub enum Cond {
    Eq(Str),
    HasPrefix(Str),
    HasSuffix(Str),
    Contains(Str),
    GlobMatch(Str),
}

pub open spec fn ev_cond(c: Cond, s: Str) -> bool {
    match c {
        Cond::Eq(lit) => s == lit,
        Cond::HasPrefix(lit) => str_prefix(s, lit),
        Cond::HasSuffix(lit) => str_suffix(s, lit),
        Cond::Contains(lit) => str_contains(s, lit),
        Cond::GlobMatch(pat) => glob_match(s, pat),
    }
}

// Lower one pattern to its condition (None = refuses the whole case:
// Any, Refused shapes, or Glob without an accepting target).
pub open spec fn lower_pat(p: Pat, glob_ok: bool) -> Option<Cond> {
    match p {
        Pat::Exact(lit) => Some(Cond::Eq(lit)),
        Pat::Prefix(lit) => Some(Cond::HasPrefix(lit)),
        Pat::Suffix(lit) => Some(Cond::HasSuffix(lit)),
        Pat::Substr(lit) => Some(Cond::Contains(lit)),
        Pat::Glob(pat) => {
            if glob_ok {
                Some(Cond::GlobMatch(pat))
            } else {
                None
            }
        },
        Pat::Any => None,
        Pat::Refused => None,
    }
}

// THE OBLIGATION (pat): each lowered condition denotes its pattern.
proof fn pat_sound(p: Pat, s: Str, glob_ok: bool)
    requires lower_pat(p, glob_ok).is_Some(),
    ensures ev_cond(lower_pat(p, glob_ok).get_Some_0(), s) == matches(p, s),
{
    match p {
        Pat::Exact(_) => {},
        Pat::Prefix(_) => {},
        Pat::Suffix(_) => {},
        Pat::Substr(_) => {},
        Pat::Glob(_) => {},
        Pat::Any => {},
        Pat::Refused => {},
    }
}

// ---- Dispatch ----

// Witnesses for concrete-string prefix facts (verbatim pattern from the
// test_lowering model: the solver needs the subrange pinned explicitly).
proof fn prefix_witness(s: Str, p: Str)
    requires p.len() <= s.len(), s.subrange(0, p.len() as int) == p,
    ensures str_prefix(s, p),
{
}

proof fn not_prefix_witness(s: Str, p: Str, i: int)
    requires 0 <= i, i < p.len(), i < s.len(), s[i] != p[i],
    ensures !str_prefix(s, p),
{
    if str_prefix(s, p) {
        assert(s.subrange(0, p.len() as int) == p);
        assert(s.subrange(0, p.len() as int)[i] == p[i]);
        assert(s[i] == p[i]);
        assert(false);
    }
}

// A source clause: pattern alternatives + body id.
pub struct Clause {
    pub pats: Seq<Pat>,
    pub body: nat,
}

pub open spec fn clause_matches(c: Clause, s: Str) -> bool {
    exists|i: int| 0 <= i < c.pats.len() && #[trigger] matches(c.pats[i], s)
}

// Source dispatch: first matching clause wins; else the default.
pub open spec fn eval_case(clauses: Seq<Clause>, els: Option<nat>, s: Str) -> Option<nat>
    decreases clauses.len(),
{
    if clauses.len() == 0 {
        els
    } else if clause_matches(clauses[0], s) {
        Some(clauses[0].body)
    } else {
        eval_case(clauses.drop_first(), els, s)
    }
}

// A lowered chain link: disjunct conditions + body id.
pub struct Link {
    pub conds: Seq<Cond>,
    pub body: nat,
}

pub open spec fn ev_link(l: Link, s: Str) -> bool {
    exists|i: int| 0 <= i < l.conds.len() && #[trigger] ev_cond(l.conds[i], s)
}

pub open spec fn eval_chain(chain: Seq<Link>, els: Option<nat>, s: Str) -> Option<nat>
    decreases chain.len(),
{
    if chain.len() == 0 {
        els
    } else if ev_link(chain[0], s) {
        Some(chain[0].body)
    } else {
        eval_chain(chain.drop_first(), els, s)
    }
}

// Lower a pattern list to its disjunct conditions.
pub open spec fn lower_pats(pats: Seq<Pat>, glob_ok: bool) -> Option<Seq<Cond>>
    decreases pats.len(),
{
    if pats.len() == 0 {
        Some(Seq::empty())
    } else {
        match lower_pat(pats[0], glob_ok) {
            None => None,
            Some(c) => {
                match lower_pats(pats.drop_first(), glob_ok) {
                    None => None,
                    Some(t) => Some(seq![c].add(t)),
                }
            },
        }
    }
}

pub open spec fn has_any(pats: Seq<Pat>) -> bool {
    exists|i: int| 0 <= i < pats.len() && #[trigger] pats[i] == Pat::Any
}

// A catch-all clause always matches (Any accepts every subject).
proof fn any_clause_matches(c: Clause, s: Str)
    requires has_any(c.pats),
    ensures clause_matches(c, s),
{
    let i = choose|i: int| 0 <= i < c.pats.len() && c.pats[i] == Pat::Any;
    assert(matches(c.pats[i], s)) by {
        assert(c.pats[i] == Pat::Any);
    };
}

// lower_pats preserves length.
proof fn lower_pats_len(pats: Seq<Pat>, glob_ok: bool)
    requires lower_pats(pats, glob_ok).is_Some(),
    ensures lower_pats(pats, glob_ok).get_Some_0().len() == pats.len(),
    decreases pats.len(),
{
    if pats.len() == 0 {
        assert(lower_pats(pats, glob_ok).get_Some_0() == Seq::<Cond>::empty()) by {
            reveal_with_fuel(lower_pats, 2);
        };
    } else {
        assert(lower_pat(pats[0], glob_ok).is_Some()) by {
            reveal_with_fuel(lower_pats, 3);
        };
        lower_pats_len(pats.drop_first(), glob_ok);
        assert(lower_pats(pats, glob_ok).get_Some_0() == seq![
            lower_pat(pats[0], glob_ok).get_Some_0()
        ].add(lower_pats(pats.drop_first(), glob_ok).get_Some_0())) by {
            reveal_with_fuel(lower_pats, 3);
        };
    }
}

// Pointwise: each lowered disjunct denotes its source alternative.
proof fn lower_pats_pointwise(pats: Seq<Pat>, s: Str, glob_ok: bool, i: int)
    requires lower_pats(pats, glob_ok).is_Some(), 0 <= i < pats.len(),
    ensures ev_cond(lower_pats(pats, glob_ok).get_Some_0()[i], s) == matches(pats[i], s),
    decreases pats.len(),
{
    assert(lower_pat(pats[0], glob_ok).is_Some()) by {
        reveal_with_fuel(lower_pats, 3);
    };
    pat_sound(pats[0], s, glob_ok);
    if i == 0 {
        assert(lower_pats(pats, glob_ok).get_Some_0()[0] == lower_pat(pats[0], glob_ok).get_Some_0()) by {
            reveal_with_fuel(lower_pats, 3);
        };
        assert(pats[0] == pats[i]);
    } else {
        lower_pats_len(pats.drop_first(), glob_ok);
        lower_pats_pointwise(pats.drop_first(), s, glob_ok, i - 1);
        assert(lower_pats(pats, glob_ok).get_Some_0()[i] == lower_pats(
            pats.drop_first(),
            glob_ok,
        ).get_Some_0()[i - 1]) by {
            reveal_with_fuel(lower_pats, 3);
        };
        assert(pats[i] == pats.drop_first()[i - 1]) by {
            assert(0 <= i - 1 < pats.drop_first().len());
        };
    }
}

// Lowered disjuncts denote the source alternatives (as a disjunction).
proof fn lower_pats_sound(pats: Seq<Pat>, s: Str, glob_ok: bool)
    requires lower_pats(pats, glob_ok).is_Some(),
    ensures {
        let conds = lower_pats(pats, glob_ok).get_Some_0();
        (exists|i: int| 0 <= i < conds.len() && #[trigger] ev_cond(conds[i], s)) == (exists|i: int|
            0 <= i < pats.len() && #[trigger] matches(pats[i], s))
    },
{
    lower_pats_len(pats, glob_ok);
    let conds = lower_pats(pats, glob_ok).get_Some_0();
    if exists|i: int| 0 <= i < conds.len() && ev_cond(conds[i], s) {
        let i = choose|i: int| 0 <= i < conds.len() && ev_cond(conds[i], s);
        lower_pats_pointwise(pats, s, glob_ok, i);
        assert(0 <= i < pats.len());
        assert(matches(pats[i], s));
    }
    if exists|i: int| 0 <= i < pats.len() && matches(pats[i], s) {
        let i = choose|i: int| 0 <= i < pats.len() && matches(pats[i], s);
        lower_pats_pointwise(pats, s, glob_ok, i);
        assert(0 <= i < conds.len());
        assert(ev_cond(conds[i], s));
    }
}

// A lowered link denotes its source clause.
proof fn link_sound(pats: Seq<Pat>, conds: Seq<Cond>, body: nat, s: Str, glob_ok: bool)
    requires
        lower_pats(pats, glob_ok).is_Some(),
        lower_pats(pats, glob_ok).get_Some_0() == conds,
    ensures ev_link(Link { conds, body }, s) == clause_matches(Clause { pats, body }, s),
{
    lower_pats_sound(pats, s, glob_ok);
    let want_l = exists|i: int| 0 <= i < conds.len() && ev_cond(conds[i], s);
    let want_c = exists|i: int| 0 <= i < pats.len() && matches(pats[i], s);
    assert(want_l == want_c);
    assert(ev_link(Link { conds, body }, s) == want_l);
    assert(clause_matches(Clause { pats, body }, s) == want_c);
}

// Lower the clause list: Any becomes the else arm (post-catch-all
// dropped — bash first-match); leading Any, Refused patterns,
// target-rejected Globs, and empty pattern lists refuse the whole case.
// `started` tracks whether a chain link was emitted yet.
pub open spec fn lower_clauses(
    clauses: Seq<Clause>,
    glob_ok: bool,
    started: bool,
) -> Option<(Seq<Link>, Option<nat>)>
    decreases clauses.len(),
{
    if clauses.len() == 0 {
        Some((Seq::empty(), None))
    } else if has_any(clauses[0].pats) {
        if started {
            Some((Seq::empty(), Some(clauses[0].body)))
        } else {
            None
        }
    } else {
        match lower_pats(clauses[0].pats, glob_ok) {
            None => None,
            Some(conds) => {
                if conds.len() == 0 {
                    None
                } else {
                    match lower_clauses(clauses.drop_first(), glob_ok, true) {
                        None => None,
                        Some((tail, els)) => {
                            Some((seq![Link { conds, body: clauses[0].body }].add(tail), els))
                        },
                    }
                }
            },
        }
    }
}

// THE OBLIGATION (dispatch): the lowered chain selects the same branch
// as the source case for every subject. Generalised over the `started`
// flag (the tail always lowers with started=true); the top-level theorem
// is the started=false instance.
proof fn dispatch_sound_gen(clauses: Seq<Clause>, s: Str, glob_ok: bool, started: bool)
    requires lower_clauses(clauses, glob_ok, started).is_Some(),
    ensures {
        let (chain, els) = lower_clauses(clauses, glob_ok, started).get_Some_0();
        eval_chain(chain, els, s) == eval_case(clauses, els, s)
    },
    decreases clauses.len(),
{
    if clauses.len() == 0 {
        assert(lower_clauses(clauses, glob_ok, started).get_Some_0() == (Seq::<Link>::empty(), None)) by {
            reveal_with_fuel(lower_clauses, 2);
        };
        reveal_with_fuel(eval_chain, 2);
        reveal_with_fuel(eval_case, 2);
    } else if has_any(clauses[0].pats) {
        if started {
            any_clause_matches(clauses[0], s);
            assert(lower_clauses(clauses, glob_ok, started).get_Some_0() == (
                Seq::<Link>::empty(),
                Some(clauses[0].body),
            )) by {
                reveal_with_fuel(lower_clauses, 3);
            };
            reveal_with_fuel(eval_chain, 3);
            reveal_with_fuel(eval_case, 3);
        } else {
            // Leading catch-all refuses — excluded by the requires.
            assert(false) by {
                reveal_with_fuel(lower_clauses, 3);
            };
        }
    } else {
        // Ordinary head: the whole lowers iff the patterns lower and the
        // tail lowers (with started=true).
        assert(lower_pats(clauses[0].pats, glob_ok).is_Some()) by {
            reveal_with_fuel(lower_clauses, 4);
        };
        assert(lower_clauses(clauses.drop_first(), glob_ok, true).is_Some()) by {
            reveal_with_fuel(lower_clauses, 4);
        };
        lower_pats_sound(clauses[0].pats, s, glob_ok);
        dispatch_sound_gen(clauses.drop_first(), s, glob_ok, true);
        let (chain, els) = lower_clauses(clauses, glob_ok, started).get_Some_0();
        let (tchain, tels) = lower_clauses(clauses.drop_first(), glob_ok, true).get_Some_0();
        assert(chain[0] == Link {
            conds: lower_pats(clauses[0].pats, glob_ok).get_Some_0(),
            body: clauses[0].body,
        }) by {
            reveal_with_fuel(lower_clauses, 4);
        };
        assert(chain.drop_first() == tchain) by {
            reveal_with_fuel(lower_clauses, 4);
        };
        assert(els == tels) by {
            reveal_with_fuel(lower_clauses, 4);
        };
        let conds0 = lower_pats(clauses[0].pats, glob_ok).get_Some_0();
        assert(chain[0] == Link { conds: conds0, body: clauses[0].body }) by {
            reveal_with_fuel(lower_clauses, 4);
        };
        link_sound(clauses[0].pats, conds0, clauses[0].body, s, glob_ok);
        assert(chain[0].conds == conds0);
        assert(clause_matches(clauses[0], s) == ev_link(chain[0], s));
        if clause_matches(clauses[0], s) {
            reveal_with_fuel(eval_chain, 3);
            reveal_with_fuel(eval_case, 3);
        } else {
            assert(eval_case(clauses, els, s) == eval_case(clauses.drop_first(), els, s)) by {
                reveal_with_fuel(eval_case, 3);
            };
            assert(eval_chain(chain, els, s) == eval_chain(chain.drop_first(), els, s)) by {
                reveal_with_fuel(eval_chain, 3);
            };
        }
    }
}

// Top-level dispatch theorem (pipeline entry: started=false).
proof fn dispatch_sound(clauses: Seq<Clause>, s: Str, glob_ok: bool)
    requires lower_clauses(clauses, glob_ok, false).is_Some(),
    ensures {
        let (chain, els) = lower_clauses(clauses, glob_ok, false).get_Some_0();
        eval_chain(chain, els, s) == eval_case(clauses, els, s)
    },
{
    dispatch_sound_gen(clauses, s, glob_ok, false);
}

// ---- Fidelity pins (mirror the source's own tests) ----

// A pure-subject literal case lowers unbound (optimizer getVar subject).
proof fn pin_pure_subject_lowers_unbound()
    ensures match lower_case(
        seq![Clause { pats: seq![Pat::Exact(seq!['s', 't', 'a', 'r', 't'])], body: 1nat }],
        true,
        true,
        false,
    ) {
        Lowered::Chain { bound: false, .. } => true,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 4);
    reveal_with_fuel(lower_pats, 3);
}

// A reserved-name subject binds once (volatile re-reads refused).
proof fn pin_reserved_subject_binds()
    ensures match lower_case(
        seq![Clause { pats: seq![Pat::Exact(seq!['x'])], body: 1nat }],
        false,
        true,
        false,
    ) {
        Lowered::Chain { bound: true, .. } => true,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 4);
    reveal_with_fuel(lower_pats, 3);
}

// Literal + affix clauses dispatch: "a"->1, "b*"->2, else none.
proof fn pin_literal_affix_dispatch()
    ensures eval_case(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        ],
        None,
        seq!['a'],
    ) == Some(1nat) && eval_case(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        ],
        None,
        seq!['b', '9'],
    ) == Some(2nat) && eval_case(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        ],
        None,
        seq!['z'],
    ) == None,
{
    assert(
        lower_clauses(
            seq![
                Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
                Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
            ],
            true,
            false,
        ).is_Some()
    ) by {
        reveal_with_fuel(lower_clauses, 8);
        reveal_with_fuel(lower_pats, 5);
    };
    assert(seq!['b', '9'].subrange(0, 1) == seq!['b']) by {
        assert forall|i: int| 0 <= i < 1 implies seq!['b', '9'][i] == seq!['b'][i] by {
        };
    };
    prefix_witness(seq!['b', '9'], seq!['b']);
    not_prefix_witness(seq!['z'], seq!['b'], 0);
    assert(seq!['z'][0] != seq!['a'][0]);
    assert(seq!['z'] != seq!['a']);
    assert(Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat }.pats[0] == Pat::Exact(
        seq!['a'],
    ));
    assert(Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat }.pats[0] == Pat::Prefix(
        seq!['b'],
    ));
    assert(matches(Pat::Exact(seq!['a']), seq!['a']));
    assert(matches(Pat::Prefix(seq!['b']), seq!['b', '9']));
    assert(!matches(Pat::Exact(seq!['a']), seq!['b', '9']));
    assert(!matches(Pat::Exact(seq!['a']), seq!['z']));
    assert(!matches(Pat::Prefix(seq!['b']), seq!['z']));
    assert(clause_matches(
        Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
        seq!['a'],
    ));
    assert(!clause_matches(
        Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
        seq!['b', '9'],
    ));
    assert(clause_matches(
        Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        seq!['b', '9'],
    ));
    assert(!clause_matches(
        Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
        seq!['z'],
    ));
    assert(!clause_matches(
        Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        seq!['z'],
    ));
    reveal_with_fuel(eval_case, 8);
    dispatch_sound(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        ],
        seq!['a'],
        true,
    );
    dispatch_sound(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        ],
        seq!['b', '9'],
        true,
    );
    dispatch_sound(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Prefix(seq!['b'])], body: 2nat },
        ],
        seq!['z'],
        true,
    );
}

// A quoted star is a literal star, not the catch-all.
proof fn pin_quoted_star_is_literal()
    ensures matches(Pat::Exact(seq!['*']), seq!['*']) && !matches(
        Pat::Exact(seq!['*']),
        seq!['a', 'b', 'c'],
    ),
{
}

// A glob class refuses the whole case without an accepting target.
proof fn pin_glob_refuses_without_target()
    ensures lower_case(
        seq![Clause { pats: seq![Pat::Glob(seq!['[', 'a', '-', 'z', ']'])], body: 1nat }],
        true,
        false,
        false,
    ) == Lowered::Refused,
{
    reveal_with_fuel(lower_clauses, 4);
}

// With an accepting target the glob clause lowers via globMatch.
proof fn pin_glob_lowers_with_target()
    ensures match lower_case(
        seq![Clause { pats: seq![Pat::Glob(seq!['i', '*'])], body: 1nat }],
        true,
        true,
        false,
    ) {
        Lowered::Chain { bound: false, .. } => true,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 4);
    reveal_with_fuel(lower_pats, 3);
}

// Dynamic / partial-quote patterns refuse the whole case.
proof fn pin_refused_pattern_refuses()
    ensures lower_case(
        seq![Clause { pats: seq![Pat::Refused], body: 1nat }],
        true,
        true,
        false,
    ) == Lowered::Refused,
{
    reveal_with_fuel(lower_clauses, 4);
}

// An impure discriminant binds once (Block[Declare, If] shape).
proof fn pin_impure_binds_once()
    ensures match lower_case(
        seq![Clause { pats: seq![Pat::Exact(seq!['x'])], body: 1nat }],
        false,
        true,
        false,
    ) {
        Lowered::Chain { bound: true, .. } => true,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 4);
    reveal_with_fuel(lower_pats, 3);
}

// An arith subject binds in text form (arith is impure).
proof fn pin_arith_subject_binds()
    ensures match lower_case(
        seq![Clause { pats: seq![Pat::Suffix(seq!['.'])], body: 1nat }],
        false,
        true,
        false,
    ) {
        Lowered::Chain { bound: true, .. } => true,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 4);
    reveal_with_fuel(lower_pats, 3);
}

// A leading catch-all refuses (no condition to bind).
proof fn pin_leading_catchall_refuses()
    ensures lower_case(
        seq![
            Clause { pats: seq![Pat::Any], body: 1nat },
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 2nat },
        ],
        true,
        true,
        false,
    ) == Lowered::Refused,
{
    reveal_with_fuel(lower_clauses, 4);
}

// Alternations split into ORed conditions (two disjuncts, one link).
proof fn pin_alternations_or()
    ensures match lower_case(
        seq![
            Clause {
                pats: seq![Pat::Exact(seq!['s']), Pat::Exact(seq!['d'])],
                body: 1nat,
            },
        ],
        true,
        true,
        false,
    ) {
        Lowered::Chain { chain, .. } => chain.len() == 1 && chain[0].conds.len() == 2,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 5);
    reveal_with_fuel(lower_pats, 4);
}

// A nocasematch mention refuses the whole program.
proof fn pin_nocasematch_refuses()
    ensures lower_case(
        seq![Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat }],
        true,
        true,
        true,
    ) == Lowered::Refused,
{
}

// A volatile positional subject binds (same rule as reserved names).
proof fn pin_volatile_positional_binds()
    ensures match lower_case(
        seq![Clause { pats: seq![Pat::Prefix(seq!['p'])], body: 1nat }],
        false,
        true,
        false,
    ) {
        Lowered::Chain { bound: true, .. } => true,
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 4);
    reveal_with_fuel(lower_pats, 3);
}

// Post-catch-all clauses are dropped (bash first-match): the chain has
// one link and the else arm is the catch-all body (model-internal:
// no source test asserts the dropped tail shape directly).
proof fn pin_post_catchall_dropped()
    ensures match lower_case(
        seq![
            Clause { pats: seq![Pat::Exact(seq!['a'])], body: 1nat },
            Clause { pats: seq![Pat::Any], body: 2nat },
            Clause { pats: seq![Pat::Exact(seq!['b'])], body: 3nat },
        ],
        true,
        true,
        false,
    ) {
        Lowered::Chain { chain, els, .. } => chain.len() == 1 && els == Some(2nat),
        _ => false,
    },
{
    reveal_with_fuel(lower_clauses, 6);
    reveal_with_fuel(lower_pats, 3);
}

// Lowered output: refusal, or the if-chain (bound = the impure subject
// was bound once to a temp denoting the same string).
pub enum Lowered {
    Refused,
    Chain { bound: bool, chain: Seq<Link>, els: Option<nat> },
}

pub open spec fn lower_case(clauses: Seq<Clause>, pure: bool, glob_ok: bool, nocase: bool) -> Lowered {
    if nocase {
        Lowered::Refused
    } else {
        match lower_clauses(clauses, glob_ok, false) {
            None => Lowered::Refused,
            Some((chain, els)) => Lowered::Chain { bound: !pure, chain, els },
        }
    }
}

fn main() {}

}
