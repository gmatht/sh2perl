// Standalone model of `typeset_attrs`
// (sh2perl/src/transforms/typeset_attrs.rs).
//
// bash's `typeset -i n` / `typeset -l s` are not one-shot assignments: the
// attribute is LIVE for the rest of the scope, so every later assignment to
// that name is coerced the same way. The pass records the attributes a
// declaration pins and rewrites later single-target assignments to
// `int(arith(TEXT))` and/or `lc(EXPR)` / `uc(EXPR)`, or folds a literal
// directly.
//
// ## What is proved here (the bash-side law, not the state machine)
//
//   rewrite_matches_bash: for ANY attribute and ANY literal assignment text,
//   the value the rewritten expression produces equals the value bash stores
//   for the same attribute — including the three cases that make this a real
//   obligation rather than a restatement:
//
//     - `-i` + canonical decimal: the pass folds to `Int(n)`, and bash reads
//       the text arithmetically, so both give n (`strict_dec_is_arith`);
//     - `-i` + anything else: `int(arith(TEXT))`, and bash's integer
//       attribute coerces a non-numeric name to 0, which `arith` does too;
//     - `-il`: bash coerces to the INTEGER first and then case-folds, so the
//       fold sees the decimal rendering, not the original text — the model
//       composes the same way (`fold_sees_the_coerced_value`).
//
//   octal_fold_is_wrong: `042` is OCTAL 34 in bash arithmetic, so treating a
//   leading zero as decimal (42) is a real miscompile. The pass refuses to
//   fold it (`strict_dec` returns None); this obligation shows the refusal is
//   load-bearing.
//
//   lu_is_refused / non_declaration_records_nothing / other_names_untouched /
//   assignment_before_declaration_untouched: the state-machine guards, each
//   with a witness where the naive rule differs.
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - `arith(text)`: the model's arithmetic evaluator is total over the model's
//   text alphabet (canonical decimals, leading-zero octals, everything else →
//   0, matching "an unset name is 0"). That the BACKENDS' `arith` node
//   implements bash's arithmetic for the full language is a runtime-library
//   contract, not proved here.
// - `lower`/`upper`: ASCII case folding only, which is exactly the pass's
//   guard (non-ASCII keeps the `lc`/`uc` call). The backends' `lc`/`uc`
//   agreeing with bash per locale is the same kind of runtime contract.
// - Text is modelled as `Seq<int>` character codes; variables inside the
//   arithmetic text are not modelled (the pass hands the text to `arith`,
//   which is the runtime contract above).
// - The recursion into nested statement lists and the `interpolate` refusal
//   are pinned (an expanded RHS is refused), not re-derived per node.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 typeset_attrs.rs
//   -> verification results:: 29 verified, 0 errors
//
//   Mutants (checked, measured on copies under the same flags):
//     - `attrs_at` ignores the left-to-right order (takes the final attribute
//       set at the point of use): 28 verified, 1 error — the
//       `assignment_before_declaration_untouched` postcondition goes red.
//     - `-il` folds the WRONG WAY (`Uc` where the pass emits `Lc`): 28
//       verified, 1 error — `rewrite_matches_bash`'s IL arm.
//     - no attribute coerces to int too (`bash_assign(NoAttr) = I(arith t)`):
//       27 verified, 2 errors.
//     - accepting a leading zero in `strict_dec` (the octal hazard) is
//       rejected one stage earlier: `octal_fold_is_wrong`'s `arith("042") ==
//       34` simplifies to `42 == 34`, a `compute_only` contradiction, so there
//       is no verified/error count for it — the obligation fires at
//       simplification time.
//   Restored and re-checked at 29 verified, 0 errors.
use vstd::prelude::*;

verus! {

// One variable's live attributes (the source's `Attr`).
pub enum A {
    NoAttr,
    I,
    L,
    U,
    IL,
    IU,
}

pub enum E {
    // the assignment's literal text, verbatim
    Text(Seq<int>),
    // the folded canonical decimal
    IntLit(int),
    // `arith(TEXT)` — bash arithmetic over the text
    Arith(Seq<int>),
    // `int(EXPR)` — the integer attribute's coercion
    IntCoerce(Box<E>),
    Lc(Box<E>),
    Uc(Box<E>),
}

pub enum V {
    I(int),
    S(Seq<int>),
}

// ---- character-code helpers (ASCII: '0'=48, 'A'=65, 'a'=97) ----

pub open spec fn is_dec(c: int) -> bool {
    48 <= c && c <= 57
}

pub open spec fn is_oct(c: int) -> bool {
    48 <= c && c <= 55
}

pub open spec fn all_dec(t: Seq<int>) -> bool
    decreases t.len(),
{
    if t.len() == 0 {
        true
    } else {
        is_dec(t[0]) && all_dec(t.subrange(1, t.len() as int))
    }
}

pub open spec fn all_oct(t: Seq<int>) -> bool
    decreases t.len(),
{
    if t.len() == 0 {
        true
    } else {
        is_oct(t[0]) && all_oct(t.subrange(1, t.len() as int))
    }
}

pub open spec fn dec_acc(t: Seq<int>, i: int, acc: int) -> int
    decreases t.len() - i,
{
    if i >= t.len() {
        acc
    } else {
        dec_acc(t, i + 1, acc * 10 + (t[i] - 48))
    }
}

pub open spec fn oct_acc(t: Seq<int>, i: int, acc: int) -> int
    decreases t.len() - i,
{
    if i >= t.len() {
        acc
    } else {
        oct_acc(t, i + 1, acc * 8 + (t[i] - 48))
    }
}

pub open spec fn is_neg(t: Seq<int>) -> bool {
    t.len() > 0 && t[0] == 45
}

pub open spec fn is_pos(t: Seq<int>) -> bool {
    t.len() > 0 && t[0] == 43
}

pub open spec fn body(t: Seq<int>) -> Seq<int> {
    if is_neg(t) || is_pos(t) { t.subrange(1, t.len() as int) } else { t }
}

pub open spec fn sign_of(t: Seq<int>) -> int {
    if is_neg(t) { -1 } else { 1 }
}

// Canonical signed decimal: no leading zero (except the single digit `0`),
// no empty body. `042`, `0x10`, `1_000`, `42.0`, `hello` all refuse.
pub open spec fn strict_dec(t: Seq<int>) -> Option<int> {
    let b = body(t);
    if b.len() == 0 || !all_dec(b) {
        Option::None
    } else if b.len() > 1 && b[0] == 48 {
        Option::None
    } else {
        Option::Some(sign_of(t) * dec_acc(b, 0, 0))
    }
}

// Leading-zero octal (`042` = 34), which is how bash reads it.
pub open spec fn octal(t: Seq<int>) -> Option<int> {
    let b = body(t);
    if b.len() > 1 && b[0] == 48 && all_oct(b) {
        Option::Some(sign_of(t) * oct_acc(b, 1, 0))
    } else {
        Option::None
    }
}

// bash's arithmetic value of the text. Every other shape is 0 — bash reads an
// unset name as 0, and the pass hands whatever it cannot fold to `arith`.
pub open spec fn arith(t: Seq<int>) -> int {
    match strict_dec(t) {
        Option::Some(n) => n,
        Option::None => match octal(t) {
            Option::Some(n) => n,
            Option::None => 0,
        },
    }
}

pub open spec fn is_ascii(t: Seq<int>) -> bool
    decreases t.len(),
{
    if t.len() == 0 {
        true
    } else {
        t[0] < 128 && is_ascii(t.subrange(1, t.len() as int))
    }
}

pub open spec fn lower(t: Seq<int>) -> Seq<int>
    decreases t.len(),
{
    if t.len() == 0 {
        Seq::empty()
    } else {
        let c = t[0];
        let c2 = if 65 <= c && c <= 90 { c + 32 } else { c };
        seq![c2] + lower(t.subrange(1, t.len() as int))
    }
}

pub open spec fn upper(t: Seq<int>) -> Seq<int>
    decreases t.len(),
{
    if t.len() == 0 {
        Seq::empty()
    } else {
        let c = t[0];
        let c2 = if 97 <= c && c <= 122 { c - 32 } else { c };
        seq![c2] + upper(t.subrange(1, t.len() as int))
    }
}

// Decimal rendering of an integer (bash's `-i` store is a decimal string).
pub open spec fn dec_digits(n: int) -> Seq<int>
    decreases n,
{
    if n < 10 {
        seq![48 + n]
    } else {
        dec_digits(n / 10) + seq![48 + n % 10]
    }
}

pub open spec fn render(v: V) -> Seq<int> {
    match v {
        V::S(t) => t,
        V::I(n) => if n < 0 { seq![45] + dec_digits(-n) } else { dec_digits(n) },
    }
}

// ---- bash's assignment semantics under a live attribute ----

pub open spec fn bash_assign(a: A, t: Seq<int>) -> V {
    match a {
        A::NoAttr => V::S(t),
        A::I => V::I(arith(t)),
        A::L => V::S(lower(t)),
        A::U => V::S(upper(t)),
        // `-i` coerces to the integer FIRST; the case fold then applies to
        // the stored decimal, not to the original text.
        A::IL => V::S(lower(render(V::I(arith(t))))),
        A::IU => V::S(upper(render(V::I(arith(t))))),
    }
}

// ---- the pass's rewrite (the source's `apply_attributes`) ----

pub open spec fn rewrite_i(t: Seq<int>) -> E {
    match strict_dec(t) {
        Option::Some(n) => E::IntLit(n),
        Option::None => E::IntCoerce(Box::new(E::Arith(t))),
    }
}

pub open spec fn rewrite(a: A, t: Seq<int>) -> E {
    match a {
        A::NoAttr => E::Text(t),
        A::I => rewrite_i(t),
        A::L => if is_ascii(t) { E::Text(lower(t)) } else { E::Lc(Box::new(E::Text(t))) },
        A::U => if is_ascii(t) { E::Text(upper(t)) } else { E::Uc(Box::new(E::Text(t))) },
        A::IL => E::Lc(Box::new(rewrite_i(t))),
        A::IU => E::Uc(Box::new(rewrite_i(t))),
    }
}

// bash's integer attribute: an integer stays, a word is read arithmetically.
pub open spec fn coerce(v: V) -> V {
    match v {
        V::I(n) => V::I(n),
        V::S(t) => V::I(arith(t)),
    }
}

// The emitted expression's value.
pub open spec fn eval(e: E) -> V
    decreases e,
{
    match e {
        E::Text(t) => V::S(t),
        E::IntLit(n) => V::I(n),
        E::Arith(t) => V::I(arith(t)),
        // a non-numeric word coerces to 0, exactly as bash's integer
        // attribute does for an unset name
        E::IntCoerce(x) => coerce(eval(*x)),
        E::Lc(x) => V::S(lower(render(eval(*x)))),
        E::Uc(x) => V::S(upper(render(eval(*x)))),
    }
}

// ---- obligations ----

// The `-i` case keeps bash's value: a canonical decimal folds to itself,
// anything else keeps `int(arith(TEXT))`, and both bash and `arith` read a
// non-numeric word as 0.
proof fn i_case_preserves(t: Seq<int>)
    ensures eval(rewrite_i(t)) == bash_assign(A::I, t),
{
    match strict_dec(t) {
        Option::Some(n) => {
            assert(rewrite_i(t) == E::IntLit(n));
            assert(eval(E::IntLit(n)) == V::I(n));
            // `arith` takes the same branch, so bash stores n as well
            assert(bash_assign(A::I, t) == V::I(n));
            assert(eval(rewrite_i(t)) == bash_assign(A::I, t));
        }
        Option::None => {
            assert(rewrite_i(t) == E::IntCoerce(Box::new(E::Arith(t))));
            assert(eval(E::Arith(t)) == V::I(arith(t)));
            assert(eval(E::IntCoerce(Box::new(E::Arith(t)))) == V::I(arith(t)));
            assert(coerce(V::I(arith(t))) == V::I(arith(t)));
            assert(bash_assign(A::I, t) == V::I(arith(t)));
            assert(eval(rewrite_i(t)) == bash_assign(A::I, t));
        }
    }
}

// THE REWRITE LAW: the value the pass's expression produces is the value
// bash stores under the same attribute, for every attribute and every text.
proof fn rewrite_matches_bash(a: A, t: Seq<int>)
    ensures eval(rewrite(a, t)) == bash_assign(a, t),
{
    match a {
        A::NoAttr => {
            assert(rewrite(a, t) == E::Text(t));
            assert(eval(E::Text(t)) == V::S(t));
            assert(bash_assign(a, t) == V::S(t));
        }
        A::I => {
            assert(rewrite(a, t) == rewrite_i(t));
            i_case_preserves(t);
        }
        A::L => {
            if is_ascii(t) {
                assert(rewrite(a, t) == E::Text(lower(t)));
                assert(eval(E::Text(lower(t))) == V::S(lower(t)));
                assert(bash_assign(a, t) == V::S(lower(t)));
            } else {
                assert(rewrite(a, t) == E::Lc(Box::new(E::Text(t))));
                assert(eval(E::Text(t)) == V::S(t));
                assert(eval(E::Lc(Box::new(E::Text(t)))) == V::S(lower(t)));
                assert(bash_assign(a, t) == V::S(lower(t)));
            }
        }
        A::U => {
            if is_ascii(t) {
                assert(rewrite(a, t) == E::Text(upper(t)));
                assert(eval(E::Text(upper(t))) == V::S(upper(t)));
                assert(bash_assign(a, t) == V::S(upper(t)));
            } else {
                assert(rewrite(a, t) == E::Uc(Box::new(E::Text(t))));
                assert(eval(E::Text(t)) == V::S(t));
                assert(eval(E::Uc(Box::new(E::Text(t)))) == V::S(upper(t)));
                assert(bash_assign(a, t) == V::S(upper(t)));
            }
        }
        A::IL => {
            // eval(Lc(inner)) = lower(render(eval inner)); bash's IL is the
            // same composition over the integer value
            assert(rewrite(a, t) == E::Lc(Box::new(rewrite_i(t))));
            i_case_preserves(t);
            assert(eval(rewrite_i(t)) == V::I(arith(t)));
            assert(eval(E::Lc(Box::new(rewrite_i(t))))
                == V::S(lower(render(V::I(arith(t))))));
            assert(bash_assign(a, t) == V::S(lower(render(V::I(arith(t))))));
        }
        A::IU => {
            assert(rewrite(a, t) == E::Uc(Box::new(rewrite_i(t))));
            i_case_preserves(t);
            assert(eval(rewrite_i(t)) == V::I(arith(t)));
            assert(eval(E::Uc(Box::new(rewrite_i(t))))
                == V::S(upper(render(V::I(arith(t))))));
            assert(bash_assign(a, t) == V::S(upper(render(V::I(arith(t))))));
        }
    }
}

// The fold sees the COERCED value, not the original text: `-il` on `42` is
// the decimal "42" folded, and on `hello` the integer 0 folded to "0".
proof fn fold_sees_the_coerced_value()
    ensures ({
        let a = A::IL;
        // `42` -> int 42 -> "42"
        bash_assign(a, seq![52, 50]) == V::S(seq![52, 50])
            // `hello` -> int 0 -> "0" (bash's integer attribute)
            && bash_assign(a, seq![104, 101, 108, 108, 111]) == V::S(seq![48])
            // and the pass emits exactly that
            && eval(rewrite(a, seq![52, 50])) == V::S(seq![52, 50])
            && eval(rewrite(a, seq![104, 101, 108, 108, 111])) == V::S(seq![48])
    }),
{
    assert(bash_assign(A::IL, seq![52, 50]) == V::S(seq![52, 50])) by (compute_only);
    assert(bash_assign(A::IL, seq![104, 101, 108, 108, 111]) == V::S(seq![48]))
        by (compute_only);
    assert(eval(rewrite(A::IL, seq![52, 50])) == V::S(seq![52, 50])) by (compute_only);
    assert(eval(rewrite(A::IL, seq![104, 101, 108, 108, 111])) == V::S(seq![48]))
        by (compute_only);
}

// ---- the refusal that keeps the law true ----

// `042` is 34 in bash arithmetic. A naive decimal fold (accepting a leading
// zero) would store 42, so refusing to fold is load-bearing.
pub open spec fn naive_dec(t: Seq<int>) -> int {
    if all_dec(body(t)) { sign_of(t) * dec_acc(body(t), 0, 0) } else { 0 }
}

proof fn octal_fold_is_wrong()
    ensures ({
        let t = seq![48, 52, 50]; // "042"
        // bash reads it as octal
        arith(t) == 34
            // the shipped rule refuses to fold it...
            && strict_dec(t) is None
            // ...and a naive leading-zero fold would claim 42
            && naive_dec(t) == 42
            && arith(t) != naive_dec(t)
            // so the emitted expression is the runtime coercion
            && eval(rewrite(A::I, t)) == V::I(34)
    }),
{
    assert(arith(seq![48, 52, 50]) == 34) by (compute_only);
    assert(strict_dec(seq![48, 52, 50]) is None) by (compute_only);
    assert(naive_dec(seq![48, 52, 50]) == 42) by (compute_only);
    assert(eval(rewrite(A::I, seq![48, 52, 50])) == V::I(34)) by (compute_only);
}

// `0x10` / `1_000` / `42.0` / empty / `hello` are not canonical decimals: the
// pass keeps the runtime coercion instead of folding them.
proof fn non_decimal_texts_refuse()
    ensures strict_dec(seq![48, 120, 49, 48]) is None    // 0x10
        && strict_dec(seq![49, 95, 48, 48, 48]) is None  // 1_000
        && strict_dec(seq![52, 50, 46, 48]) is None      // 42.0
        && strict_dec(Seq::empty()) is None
        && strict_dec(seq![104, 101, 108, 108, 111]) is None, // hello
{
    assert(strict_dec(seq![48, 120, 49, 48]) is None) by (compute_only);
    assert(strict_dec(seq![49, 95, 48, 48, 48]) is None) by (compute_only);
    assert(strict_dec(seq![52, 50, 46, 48]) is None) by (compute_only);
    assert(strict_dec(Seq::empty()) is None) by (compute_only);
    assert(strict_dec(seq![104, 101, 108, 108, 111]) is None) by (compute_only);
}

// A negative canonical decimal folds with its sign.
proof fn negative_decimal_folds()
    ensures strict_dec(seq![45, 52, 50]) == Option::Some(-42)
        && eval(rewrite(A::I, seq![45, 52, 50])) == V::I(-42),
{
    assert(strict_dec(seq![45, 52, 50]) == Option::Some(-42)) by (compute_only);
    assert(eval(rewrite(A::I, seq![45, 52, 50])) == V::I(-42)) by (compute_only);
}

// ---- the state machine's guards (each with the naive rule it rules out) ----

// Without an attribute the assignment stores its text: `m` is untouched by
// `typeset -i n`.
proof fn other_names_untouched()
    ensures ({
        let t = seq![109, 43, 49]; // "m+1"
        eval(rewrite(A::NoAttr, t)) == V::S(t)
            && bash_assign(A::NoAttr, t) == V::S(t)
    }),
{
}

// `-lu` together is refused (the source will not guess which wins): the
// model has no attribute for it, so the assignment keeps its text — while
// EITHER single fold would have changed the stored value, which is why
// refusing beats picking one.
proof fn lu_is_refused_and_bash_would_fold()
    ensures ({
        let t = seq![65, 98]; // "Ab"
        // no attribute recorded by the pass => text stored
        eval(rewrite(A::NoAttr, t)) == V::S(t)
            // while either single fold would have changed it
            && bash_assign(A::L, t) == V::S(seq![97, 98])   // "ab"
            && bash_assign(A::U, t) == V::S(seq![65, 66])   // "AB"
            && bash_assign(A::L, t) != V::S(t)
            && bash_assign(A::U, t) != V::S(t)
    }),
{
    assert(bash_assign(A::L, seq![65, 98]) == V::S(seq![97, 98])) by (compute_only);
    assert(bash_assign(A::U, seq![65, 98]) == V::S(seq![65, 66])) by (compute_only);
}

// A declaration whose flags carry no int/l fold (`typeset -r`) records
// nothing (the source's `attr.is_empty()`), so the assignment is untouched.
proof fn non_attribute_declaration_records_nothing()
    ensures ({
        let t = seq![55]; // "7"
        eval(rewrite(A::NoAttr, t)) == V::S(t)
            && bash_assign(A::I, t) == V::I(7)
    }),
{
    assert(bash_assign(A::I, seq![55]) == V::I(7)) by (compute_only);
}

// An ASSIGNMENT BEFORE the declaration is untouched; the same assignment
// after it is not. The model's rewrite is per-assignment given the attribute
// at that point; `attrs_at` carries the left-to-right order.
pub enum Ev {
    Decl(nat, A),
    // a literal assignment: name, text
    Assign(nat, Seq<int>),
}

pub enum Evs {
    Nil,
    Cons(Ev, Box<Evs>),
}

pub open spec fn scan(p: Evs, st: Map<nat, A>) -> Map<nat, A>
    decreases p,
{
    match p {
        Evs::Nil => st,
        Evs::Cons(Ev::Decl(n, a), rest) => scan(*rest, st.insert(n, a)),
        Evs::Cons(Ev::Assign(_, _), rest) => scan(*rest, st),
    }
}

// The attributes in force just BEFORE event k.
pub open spec fn attrs_at(p: Evs, k: nat, st: Map<nat, A>) -> Map<nat, A>
    decreases p,
{
    match p {
        Evs::Nil => st,
        Evs::Cons(Ev::Decl(n, a), rest) => {
            if k == 0 { st } else { attrs_at(*rest, (k - 1) as nat, st.insert(n, a)) }
        }
        Evs::Cons(Ev::Assign(_, _), rest) => {
            if k == 0 { st } else { attrs_at(*rest, (k - 1) as nat, st) }
        }
    }
}

proof fn assignment_before_declaration_untouched()
    ensures ({
        let p = Evs::Cons(
            Ev::Assign(3, seq![110, 43, 49]),
            Box::new(Evs::Cons(Ev::Decl(3, A::I), Box::new(Evs::Nil))),
        );
        // at the assignment (index 0) the name has no attribute
        !attrs_at(p, 0, Map::<nat, A>::empty()).contains_key(3)
            // at the end it does
            && scan(p, Map::<nat, A>::empty()).contains_key(3)
            // and only then is the text rewritten
            && rewrite(A::NoAttr, seq![110, 43, 49]) == E::Text(seq![110, 43, 49])
            && (match rewrite(A::I, seq![110, 43, 49]) { E::IntCoerce(_) => true, _ => false })
    }),
{
    reveal_with_fuel(scan, 4);
    reveal_with_fuel(attrs_at, 4);
    assert(scan(
        Evs::Cons(
            Ev::Assign(3, seq![110, 43, 49]),
            Box::new(Evs::Cons(Ev::Decl(3, A::I), Box::new(Evs::Nil))),
        ),
        Map::<nat, A>::empty(),
    ).contains_key(3));
    assert(strict_dec(seq![110, 43, 49]) is None) by (compute_only);
}

// A declaration records EVERY name on its command line (the source's
// `names.into_iter().map(...)`), not just the first.
proof fn declaration_records_every_name()
    ensures ({
        let p = Evs::Cons(
            Ev::Decl(4, A::IL),
            Box::new(Evs::Cons(Ev::Decl(5, A::IL), Box::new(Evs::Nil))),
        );
        let st = scan(p, Map::<nat, A>::empty());
        st.contains_key(4) && st.contains_key(5) && st[4] == A::IL && st[5] == A::IL
    }),
{
    reveal_with_fuel(scan, 4);
}

// ---- pins: the source's tests ----

// integer_attribute_makes_later_assignments_arithmetic: `n+1` and `hello`
// both become int(arith(...)), and bash stores 0 for `hello`.
proof fn pin_integer_attribute_is_arithmetic()
    ensures ({
        let a = A::I;
        (match rewrite(a, seq![110, 43, 49]) {
            E::IntCoerce(_) => true,
            _ => false,
        }) && (match rewrite(a, seq![104, 101, 108, 108, 111]) {
            E::IntCoerce(_) => true,
            _ => false,
        }) && bash_assign(a, seq![104, 101, 108, 108, 111]) == V::I(0)
    }),
{
    assert(strict_dec(seq![110, 43, 49]) is None) by (compute_only);
    assert(strict_dec(seq![104, 101, 108, 108, 111]) is None) by (compute_only);
    assert(bash_assign(A::I, seq![104, 101, 108, 108, 111]) == V::I(0)) by (compute_only);
}

// other_variables_are_left_alone.
proof fn pin_other_variables_left_alone()
    ensures rewrite(A::NoAttr, seq![109, 43, 49]) == E::Text(seq![109, 43, 49]),
{
}

// a_non_attribute_typeset_records_nothing (`-r`, no int/fold).
proof fn pin_non_attribute_typeset()
    ensures eval(rewrite(A::NoAttr, seq![55])) == V::S(seq![55]),
{
}

// an_expanded_rhs_refuses_rather_than_guessing: modelled as a `Text` with
// the Expanded flag, which the pass never rewrites (it has no literal text).
pub open spec fn rewrites_expanded(a: A, expanded: bool) -> bool {
    !expanded || a == A::NoAttr
}

proof fn pin_expanded_rhs_refuses()
    ensures !rewrites_expanded(A::I, true) && rewrites_expanded(A::I, false),
{
}

// a_canonical_decimal_integer_folds_to_the_literal.
proof fn pin_canonical_decimal_folds()
    ensures rewrite(A::I, seq![45, 52, 50]) == E::IntLit(-42)
        && eval(rewrite(A::I, seq![45, 52, 50])) == bash_assign(A::I, seq![45, 52, 50]),
{
    assert(rewrite(A::I, seq![45, 52, 50]) == E::IntLit(-42)) by (compute_only);
    assert(eval(rewrite(A::I, seq![45, 52, 50])) == bash_assign(A::I, seq![45, 52, 50]))
        by (compute_only);
}

// an_octal_looking_integer_refuses_the_fold.
proof fn pin_octal_refuses()
    ensures (match rewrite(A::I, seq![48, 52, 50]) {
        E::IntCoerce(_) => true,
        _ => false,
    }),
{
    assert(strict_dec(seq![48, 52, 50]) is None) by (compute_only);
}

fn main() {}
}
