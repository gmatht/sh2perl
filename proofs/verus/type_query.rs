// Standalone model of type-query (sh2perl/src/transforms/type_query.rs):
// `type NAME...` is answered from the PROGRAM (`NAME is a function`) or
// from the true-builtin table (`NAME is a shell builtin`); anything else
// keeps its original lowering. Proves the Tier-1 verdict-soundness
// obligation:
//
//   rewrite_shape:        a rewrite preserves the queried names and their
//                         ORDER (the echo has one line per name, so the
//                         pipeline's `head -1` reports the FIRST name --
//                         the parse-eval-multiline why the pass exists);
//   answer_complete:      every emitted line has an answer: the rewrite
//                         requires all_classified, so no name is answered
//                         by guesswork;
//   refusal_is_identity:  when the pass declines, the call is untouched.
//                         NO PARTIAL ANSWER: a mixed known/unknown list
//                         prints nothing, because bash prints the known
//                         line AND a not-found line and a half rewrite
//                         would silently swallow the error;
//   function_precedes_builtin / external_no_claim: the precedence rules
//                         that choose the text (`type printf` with a user
//                         function `printf` is a FUNCTION; `type ls` is
//                         never "a shell builtin").
//
// What is modelled and what is NOT (checked against the source):
//   - the call shape `exec|builtin("type", [Str names...])`. The theorem
//     is stated over the call's FIELDS (name list + the two per-word
//     refusal flags the source tests): `flags` is a name starting '-'
//     (`type -t`/`-a`) and `dynamic` is a non-Str word. Structural
//     recognition, and the recursion over statements/expressions/
//     pipelines that finds the call, are out of scope -- per call site.
//   - the echo's per-line TEXT is not modelled: a line is a function of
//     its name and its classification, and the classification is what
//     `answer_complete` pins. The echo node carries the names in order:
//     one line per name is the length rule, exactly what `head -1` reads.
//   - the function-name set `fns` and the builtin table `table` are
//     PARAMETERS. The abstraction is auditable on the Rust side:
//     `builtin_table_matches_compgen` asserts the constant equals
//     `compgen -A builtin` on the bash that runs the suite (61 names — `.`,
//     `:` and `break` were missing while the table was recited rather than
//     measured), and `builtin_table_excludes_externals_and_has_no_copies`
//     keeps coreutils out (`ls`/`head` are not builtins, so `type ls` must
//     refuse) and the list duplicate-free.
//   - `shadowed` is the program defining its own `type` (the source
//     returns false for the whole statement list). Modelled per node,
//     which is the same observable: no rewrite anywhere.
//   - which names land in `fns` (collect_functions and the literal-eval
//     payload scan) is out of scope; the model quantifies over ANY set,
//     so it covers both collectors.
use vstd::prelude::*;

verus! {

pub type Name = nat;

/// What bash's `type` reports for the names this pass can decide.
pub enum Kind {
    Function,
    Builtin,
}

pub enum Expr {
    /// `exec|builtin("type", [names...])` as it arrives in the shIR.
    TypeCall { names: Seq<Name>, flags: bool, dynamic: bool },
    /// The rewrite: `builtin("echo", [lines])` -- one line per name, in
    /// the queried order (see the header on what a line's text is).
    Echo { names: Seq<Name> },
    /// Everything else (opaque to this pass).
    Other,
}

// ---- accessors (so the theorems can name the pieces of the rewrite) ----

pub open spec fn is_echo(e: Expr) -> bool {
    match e {
        Expr::Echo { .. } => true,
        _ => false,
    }
}

pub open spec fn out_names(e: Expr) -> Seq<Name> {
    match e {
        Expr::Echo { names, .. } => names,
        _ => Seq::empty(),
    }
}

/// The classification, mirroring `type_call_answer`'s test order: a
/// program function first (it SHADOWS a builtin of the same name), then
/// the true-builtin table, else nothing (an external -- PATH is runtime
/// state the pass refuses to guess at).
pub open spec fn classify(fns: Seq<Name>, table: Seq<Name>, n: Name) -> Option<Kind> {
    if fns.contains(n) {
        Some(Kind::Function)
    } else if table.contains(n) {
        Some(Kind::Builtin)
    } else {
        None
    }
}

/// Every queried name is answerable. This is the all-or-nothing rule: the
/// reply exists only when NO name needs runtime state.
pub open spec fn all_classified(fns: Seq<Name>, table: Seq<Name>, names: Seq<Name>) -> bool {
    forall|i: int| 0 <= i < names.len() ==> classify(fns, table, names[i]).is_some()
}

/// The transform, mirroring `transform` + `type_call_answer`.
pub open spec fn lower(e: Expr, fns: Seq<Name>, table: Seq<Name>, shadowed: bool) -> Expr {
    match e {
        Expr::TypeCall { names, flags, dynamic } => {
            if shadowed || flags || dynamic || names.len() == 0 || !all_classified(
                fns,
                table,
                names,
            ) {
                e
            } else {
                Expr::Echo { names: names }
            }
        }
        _ => e,
    }
}

// ---- the obligation ----

/// A rewrite preserves the queried names and their ORDER; a refusal
/// leaves the call untouched. Order + one-line-per-name is the whole
/// contract the echo consumer sees (`head -1` takes the first name).
proof fn rewrite_shape(
    names: Seq<Name>,
    flags: bool,
    dynamic: bool,
    fns: Seq<Name>,
    table: Seq<Name>,
    shadowed: bool,
)
    ensures
        is_echo(lower(Expr::TypeCall { names, flags, dynamic }, fns, table, shadowed))
            ==> out_names(lower(Expr::TypeCall { names, flags, dynamic }, fns, table, shadowed))
            == names,
        !is_echo(lower(Expr::TypeCall { names, flags, dynamic }, fns, table, shadowed)) ==> lower(
            Expr::TypeCall { names, flags, dynamic },
            fns,
            table,
            shadowed,
        ) == (Expr::TypeCall { names, flags, dynamic }),
{
    let e = Expr::TypeCall { names, flags, dynamic };
    if shadowed || flags || dynamic || names.len() == 0 || !all_classified(fns, table, names) {
        assert(lower(e, fns, table, shadowed) == e);
        assert(!is_echo(e));
    } else {
        assert(lower(e, fns, table, shadowed) == (Expr::Echo { names: names }));
        assert(out_names(lower(e, fns, table, shadowed)) == names);
    }
}

/// Every emitted line has an answer: the rewrite's premise is
/// `all_classified`, so no line is a guess.
proof fn answer_complete(
    names: Seq<Name>,
    flags: bool,
    dynamic: bool,
    fns: Seq<Name>,
    table: Seq<Name>,
    shadowed: bool,
    i: int,
)
    requires
        0 <= i < names.len(),
        is_echo(lower(Expr::TypeCall { names, flags, dynamic }, fns, table, shadowed)),
    ensures
        classify(fns, table, names[i]).is_some(),
{
    // the rewrite's guard is all_classified, which instantiates at i
    assert(all_classified(fns, table, names));
}

/// Any unclassifiable name refuses the WHOLE call (no partial answer).
proof fn unclassified_refuses(
    names: Seq<Name>,
    flags: bool,
    dynamic: bool,
    fns: Seq<Name>,
    table: Seq<Name>,
    i: int,
)
    requires
        0 <= i < names.len(),
        classify(fns, table, names[i]).is_none(),
    ensures
        lower(Expr::TypeCall { names, flags, dynamic }, fns, table, false) == (Expr::TypeCall {
            names,
            flags,
            dynamic,
        }),
{
    let e = Expr::TypeCall { names, flags, dynamic };
    // i is a counterexample to all_classified
    assert(!all_classified(fns, table, names));
    assert(lower(e, fns, table, false) == e);
}

/// The program's own `type` shadows the builtin: nothing is rewritten.
proof fn shadowed_identity(
    names: Seq<Name>,
    flags: bool,
    dynamic: bool,
    fns: Seq<Name>,
    table: Seq<Name>,
)
    ensures
        lower(Expr::TypeCall { names, flags, dynamic }, fns, table, true) == (Expr::TypeCall {
            names,
            flags,
            dynamic,
        }),
{
    rewrite_shape(names, flags, dynamic, fns, table, true);
}

/// `all_classified` for a one-name list, from that name's answer.
proof fn all_classified_one(fns: Seq<Name>, table: Seq<Name>, n: Name)
    requires
        classify(fns, table, n).is_some(),
    ensures
        all_classified(fns, table, seq![n]),
{
    let ns = seq![n];
    assert forall|i: int| 0 <= i < ns.len() implies classify(fns, table, ns[i]).is_some() by {
        assert(ns[i] == n);
    }
}

/// `all_classified` for a two-name list, from both answers.
proof fn all_classified_two(fns: Seq<Name>, table: Seq<Name>, a: Name, b: Name)
    requires
        classify(fns, table, a).is_some(),
        classify(fns, table, b).is_some(),
    ensures
        all_classified(fns, table, seq![a, b]),
{
    let ns = seq![a, b];
    assert forall|i: int| 0 <= i < ns.len() implies classify(fns, table, ns[i]).is_some() by {
        if i == 0 {
            assert(ns[i] == a);
        } else {
            assert(ns[i] == b);
        }
    }
}

// ---- pins (the source's own tests, as lemmas) ----

/// Flag forms (`type -t`, `type -a`) keep the runtime path.
proof fn pin_flag_refused(fns: Seq<Name>, table: Seq<Name>)
    ensures
        lower(
            Expr::TypeCall { names: seq![7nat], flags: true, dynamic: false },
            fns,
            table,
            false,
        ) == (Expr::TypeCall { names: seq![7nat], flags: true, dynamic: false }),
{
    rewrite_shape(seq![7nat], true, false, fns, table, false);
}

/// Dynamic words (not a literal name) keep the runtime path.
proof fn pin_dynamic_refused(fns: Seq<Name>, table: Seq<Name>)
    ensures
        lower(
            Expr::TypeCall { names: seq![7nat], flags: false, dynamic: true },
            fns,
            table,
            false,
        ) == (Expr::TypeCall { names: seq![7nat], flags: false, dynamic: true }),
{
    rewrite_shape(seq![7nat], false, true, fns, table, false);
}

/// `type` with no names is a usage error in bash; the pass must not
/// manufacture an empty echo for it.
proof fn pin_empty_refused(fns: Seq<Name>, table: Seq<Name>)
    ensures
        lower(
            Expr::TypeCall { names: Seq::<Name>::empty(), flags: false, dynamic: false },
            fns,
            table,
            false,
        ) == (Expr::TypeCall { names: Seq::<Name>::empty(), flags: false, dynamic: false }),
{
    rewrite_shape(Seq::<Name>::empty(), false, false, fns, table, false);
}

/// A function beats a builtin of the same name.
proof fn pin_function_precedes_builtin(fns: Seq<Name>, table: Seq<Name>, n: Name)
    requires
        fns.contains(n),
        table.contains(n),
    ensures
        classify(fns, table, n) == Some(Kind::Function),
{
}

/// A name that is neither a program function nor a true builtin is never
/// answered -- `type ls` must not claim "ls is a shell builtin".
proof fn pin_external_no_claim(fns: Seq<Name>, table: Seq<Name>, n: Name)
    requires
        !fns.contains(n),
        !table.contains(n),
    ensures
        classify(fns, table, n).is_None(),
{
}

/// Mixed known + unknown: the whole call survives (bash would print the
/// known line AND a not-found error; a partial rewrite loses the error).
proof fn pin_mixed_refuses(fns: Seq<Name>, table: Seq<Name>, a: Name, b: Name)
    requires
        classify(fns, table, a).is_Some(),
        classify(fns, table, b).is_None(),
    ensures
        lower(
            Expr::TypeCall { names: seq![a, b], flags: false, dynamic: false },
            fns,
            table,
            false,
        ) == (Expr::TypeCall { names: seq![a, b], flags: false, dynamic: false }),
{
    unclassified_refuses(seq![a, b], false, false, fns, table, 1);
}

/// A table entry with no program function of that name answers "a shell
/// builtin" (the `type printf` case).
proof fn pin_builtin_answers(fns: Seq<Name>, table: Seq<Name>, n: Name)
    requires
        !fns.contains(n),
        table.contains(n),
    ensures ({
        let r = lower(
            Expr::TypeCall { names: seq![n], flags: false, dynamic: false },
            fns,
            table,
            false,
        );
        is_echo(r) && out_names(r) == seq![n]
    }),
{
    assert(classify(fns, table, n) == Some(Kind::Builtin));
    all_classified_one(fns, table, n);
    rewrite_shape(seq![n], false, false, fns, table, false);
}

/// A program function answers "is a function" -- and takes precedence
/// over a builtin of the same name (the derived shadowing case).
proof fn pin_function_answers(fns: Seq<Name>, table: Seq<Name>, n: Name)
    requires
        fns.contains(n),
    ensures
        is_echo(lower(
            Expr::TypeCall { names: seq![n], flags: false, dynamic: false },
            fns,
            table,
            false,
        )),
{
    assert(classify(fns, table, n) == Some(Kind::Function));
    all_classified_one(fns, table, n);
    rewrite_shape(seq![n], false, false, fns, table, false);
}

/// Two names keep their ORDER, and both lines are answered (the `head -1`
/// property: the first line is about the first name).
proof fn pin_two_names_keep_order(fns: Seq<Name>, table: Seq<Name>, a: Name, b: Name)
    requires
        classify(fns, table, a).is_Some(),
        classify(fns, table, b).is_Some(),
    ensures ({
        let r = lower(
            Expr::TypeCall { names: seq![a, b], flags: false, dynamic: false },
            fns,
            table,
            false,
        );
        is_echo(r) && out_names(r) == seq![a, b] && classify(
            fns,
            table,
            out_names(r)[0],
        ).is_Some() && classify(fns, table, out_names(r)[1]).is_Some()
    }),
{
    all_classified_two(fns, table, a, b);
    rewrite_shape(seq![a, b], false, false, fns, table, false);
    answer_complete(seq![a, b], false, false, fns, table, false, 0);
    answer_complete(seq![a, b], false, false, fns, table, false, 1);
}

/// An external name (`type definitely_not_a_builtin`) keeps the call.
proof fn pin_external_keeps_call(fns: Seq<Name>, table: Seq<Name>, n: Name)
    requires
        classify(fns, table, n).is_none(),
    ensures
        lower(
            Expr::TypeCall { names: seq![n], flags: false, dynamic: false },
            fns,
            table,
            false,
        ) == (Expr::TypeCall { names: seq![n], flags: false, dynamic: false }),
{
    unclassified_refuses(seq![n], false, false, fns, table, 0);
}

fn main() {}

}
