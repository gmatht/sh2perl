// Standalone model of `split_fold`
// (sh2perl/src/shir_passes/split_fold.rs, `fold_trivial_splits`).
//
// An unquoted expansion in bash yields the value's WORDS. When the value is
// provably free of IFS whitespace the split produces exactly one field equal
// to the value — so `split[e]` is a no-op and the pass replaces it with `e`.
//
// ## The law
//
//   fields(x) is what a value position observes as a word list: a `split`
//   observes the words of its argument, every other expression observes one
//   field. `txt(x)` is the text the expression carries.
//
//   fold_preserves_fields: for EVERY expression, every environment, both the
//   word list and the carried text are unchanged by the fold. That is the
//   rewrite law; the fold may only swap a node for one that denotes the same
//   thing in every position.
//
//   The obligation that makes it true is words_nonsep: a whitespace-free
//   NON-EMPTY value splits to exactly `[v]`. Both halves of that licence are
//   load-bearing, and both are pinned by going red:
//
//   - empty_is_zero_words: `split("")` is ZERO fields while the bare word is
//     ONE empty field. The pre-fix pass folded on whitespace-freedom alone and
//     collapsed the two spellings (MEASURED 2026-11-05: `z=""; set -- $z` was
//     C 1 word / bash 0, and `set -- "$z"` lowered byte-identically, so the
//     fold destroyed the only bit that told them apart). The model exhibits
//     it, and the mutant that drops the non-emptiness half is red.
//   - whitespace_is_load_bearing: `split("a b")` is two fields, so a fold
//     that ignored the whitespace check is wrong. The mutant is red.
//
//   An identity pass satisfies nothing: `fold_leaves_nothing` (completeness)
//   and `fold_is_idempotent` are separate obligations, and the identity
//   mutant is red on the completeness obligation.
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - REACHABILITY / REGISTRATION: that the pass runs (the C path wires it at
//   `bash-o4/src/pipeline.rs`'s O3/O4 pre-render stage) and that frontends
//   emit the `split` marker at all is the pipeline's business, not the law's.
// - THE VERDICTS: the pass does not decide nospace/non-empty at the rewrite
//   site; it reads verdicts computed elsewhere and models them here as node
//   kinds. Two such facts are hypotheses of the law, named after the code
//   that must supply them:
//     * a var with the `var_nospace` bit really reads whitespace-free text —
//       `shir::analyze_var_nospace`'s per-var fixpoint (every assignment RHS
//       nospace, no runtime-store writes) plus `program`-level gating. The
//       model carries it as `nospace_env_ok(ns)`; the DOT is `NSVar`.
//     * a var with an A2 `Int`/`Int32`/`Int64`/`UInt32`/`UInt64` verdict
//       really reads canonical decimal text, which is non-empty and
//       whitespace-free. The model carries it as `int_env_ok(ie)`; the node
//       is `Int`. (Float is excluded by the source for exactly this reason:
//       its formatting is backend-defined, so there is no such fact.)
//   What the model DOES discharge is the shape reasoning the pass performs on
//   literals, interpolations and arithmetic — `verdict_sound` proves the two
//   static checks are conservative with respect to the text.
// - THE IFS GATE: `shir::program_may_custom_ifs` makes the pass return early.
//   The model keeps the default separator set as a parameter (`ifs`) and the
//   custom-IFS pin shows why: with `IFS=:` the whitespace-free value `a:b`
//   splits into two fields, so the default-IFS gate is not decoration.
// - THE ELEMENT-POSITION CARVE-OUT: an element store into an array that is
//   NOT already int-homed keeps its split. That is a CONSERVATIVE skip, not a
//   value obligation — it protects the C ingest's array homing (the
//   051_primes crash: folding flipped `char**` to `long long*` and the
//   string-only slice path died). The model carries the gate as `elem_ok`;
//   the value law holds for both settings, and the pins show the fold is
//   observable only through it.
// - CORPUS DOMAIN: whether any input program reaches the fold is a coverage
//   question; the law is universally quantified over the modelled shapes.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 split_fold.rs
//   -> verification results:: 41 verified, 0 errors
//
//   Mutants (spec `fold` edited on copies outside the tree, same flags):
//     - drop the non-emptiness half (whitespace-freedom alone — the measured
//       2026-11-05 bug): 36 verified, 5 errors.
//     - drop the whitespace half (non-emptiness alone): 38 verified, 3 errors.
//     - identity pass (the marker is never removed): 33 verified, 8 errors.
//     - fold when EITHER half holds: 34 verified, 7 errors.
use vstd::prelude::*;

verus! {

// ---------- bytes ----------

pub open spec fn mem(s: Seq<int>, c: int) -> bool
    decreases s.len(),
{
    if s.len() == 0 {
        false
    } else {
        s[0] == c || mem(s.subrange(1, s.len() as int), c)
    }
}

// The default IFS word-split set: space, tab, newline, carriage return.
pub open spec fn ws() -> Seq<int> {
    seq![32, 9, 10, 13]
}

// A custom IFS (`IFS=:`).
pub open spec fn colon() -> Seq<int> {
    seq![58]
}

pub open spec fn all_nonsep(v: Seq<int>, ifs: Seq<int>) -> bool {
    forall|i: int| 0 <= i < v.len() ==> !mem(ifs, v[i])
}

// ---------- the tokenizer (bash's word split) ----------

// Scan left to right, accumulating finished words and the current one. One
// byte at a time so termination is immediate.
pub open spec fn scan(
    v: Seq<int>,
    ifs: Seq<int>,
    acc: Seq<Seq<int>>,
    cur: Seq<int>,
) -> (Seq<Seq<int>>, Seq<int>)
    decreases v.len(),
{
    if v.len() == 0 {
        (acc, cur)
    } else if mem(ifs, v[0]) {
        if cur.len() == 0 {
            scan(v.subrange(1, v.len() as int), ifs, acc, cur)
        } else {
            scan(v.subrange(1, v.len() as int), ifs, acc + seq![cur], Seq::empty())
        }
    } else {
        scan(v.subrange(1, v.len() as int), ifs, acc, cur + seq![v[0]])
    }
}

// The words an unquoted expansion yields. An EMPTY value yields ZERO words;
// that is the fact the non-emptiness half of the licence exists for.
pub open spec fn words(v: Seq<int>, ifs: Seq<int>) -> Seq<Seq<int>> {
    let r = scan(v, ifs, Seq::empty(), Seq::empty());
    if r.1.len() == 0 {
        r.0
    } else {
        r.0 + seq![r.1]
    }
}

// ---------- lemmas about the tokenizer ----------

proof fn all_nonsep_tail(v: Seq<int>, ifs: Seq<int>)
    requires all_nonsep(v, ifs), v.len() > 0,
    ensures all_nonsep(v.subrange(1, v.len() as int), ifs),
{
    assert forall|i: int| 0 <= i < v.subrange(1, v.len() as int).len()
        implies !mem(ifs, v.subrange(1, v.len() as int)[i]) by {
        assert(v.subrange(1, v.len() as int)[i] == v[i + 1]);
    }
}

proof fn all_nonsep_append(a: Seq<int>, b: Seq<int>, ifs: Seq<int>)
    requires all_nonsep(a, ifs), all_nonsep(b, ifs),
    ensures all_nonsep(a + b, ifs),
{
    assert forall|i: int| 0 <= i < (a + b).len() implies !mem(ifs, (a + b)[i]) by {
        if i < a.len() {
            assert((a + b)[i] == a[i]);
        } else {
            assert((a + b)[i] == b[i - a.len()]);
        }
    }
}

// Literal sequences built byte by byte (the pins need these to state that a
// concrete literal has no separators).
proof fn nonsep1(a: int, ifs: Seq<int>)
    requires !mem(ifs, a),
    ensures all_nonsep(seq![a], ifs),
{
    assert forall|i: int| 0 <= i < seq![a].len() implies !mem(ifs, seq![a][i]) by {
        assert(seq![a][i] == a);
    }
}

proof fn nonsep2(a: int, b: int, ifs: Seq<int>)
    requires !mem(ifs, a), !mem(ifs, b),
    ensures all_nonsep(seq![a, b], ifs),
{
    assert forall|i: int| 0 <= i < seq![a, b].len() implies !mem(ifs, seq![a, b][i]) by {
        if i == 0 {
            assert(seq![a, b][i] == a);
        } else {
            assert(seq![a, b][i] == b);
        }
    }
}

proof fn nonsep3(a: int, b: int, c: int, ifs: Seq<int>)
    requires !mem(ifs, a), !mem(ifs, b), !mem(ifs, c),
    ensures all_nonsep(seq![a, b, c], ifs),
{
    assert forall|i: int| 0 <= i < seq![a, b, c].len() implies !mem(ifs, seq![a, b, c][i]) by {
        if i == 0 {
            assert(seq![a, b, c][i] == a);
        } else if i == 1 {
            assert(seq![a, b, c][i] == b);
        } else {
            assert(seq![a, b, c][i] == c);
        }
    }
}

// On whitespace-free input the scanner appends every byte to `cur` and never
// flushes, so it returns `acc` unchanged and the whole value as the current
// word.
proof fn scan_nonsep(v: Seq<int>, ifs: Seq<int>, acc: Seq<Seq<int>>, cur: Seq<int>)
    requires all_nonsep(v, ifs),
    ensures scan(v, ifs, acc, cur) == (acc, cur + v),
    decreases v.len(),
{
    if v.len() > 0 {
        let c = v[0];
        let rest = v.subrange(1, v.len() as int);
        assert(!mem(ifs, c));
        assert(v =~= seq![c] + rest);
        all_nonsep_tail(v, ifs);
        scan_nonsep(rest, ifs, acc, cur + seq![c]);
        assert((cur + seq![c]) + rest == cur + (seq![c] + rest));
        assert(cur + seq![c] + rest == cur + v);
    }
}

// THE tokenizer obligation the fold rests on: a whitespace-free non-empty
// value is exactly one word, itself.
proof fn words_nonsep(v: Seq<int>, ifs: Seq<int>)
    requires all_nonsep(v, ifs), v.len() > 0,
    ensures words(v, ifs) == seq![v],
{
    scan_nonsep(v, ifs, Seq::<Seq<int>>::empty(), Seq::<int>::empty());
    assert(Seq::<Seq<int>>::empty() + seq![v] == seq![v]);
}

// ---------- the IR ----------

// An expression, at the granularity the pass distinguishes.
//   S      a string literal
//   Var    a read with NO verdict (anything may come back)
//   NSVar  a read carrying the `var_nospace` verdict (no blanks — maybe empty)
//   Int    a read carrying an A2 int verdict (canonical decimal)
//   Split  the `split[e]` marker
//   Cat    any two-position container (interpolation, binop, argument list)
pub enum X {
    S(Seq<int>),
    Var(nat),
    NSVar(nat),
    Int(nat),
    Split(Box<X>),
    Cat(Box<X>, Box<X>),
}

// The text an expression carries. A `split` node carries the text of its
// argument (its word LIST is what `fields` observes).
pub open spec fn txt(
    x: X,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
) -> Seq<int>
    decreases x,
{
    match x {
        X::S(s) => s,
        X::Var(n) => us[n],
        X::NSVar(n) => ns[n],
        X::Int(n) => ie[n],
        X::Split(e) => txt(*e, us, ns, ie),
        X::Cat(a, b) => txt(*a, us, ns, ie) + txt(*b, us, ns, ie),
    }
}

// What a value position observes: a `split` observes the argument's WORDS,
// everything else observes a single field.
pub open spec fn fields(
    x: X,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
) -> Seq<Seq<int>>
    decreases x,
{
    match x {
        X::Split(e) => words(txt(*e, us, ns, ie), ws()),
        _ => seq![txt(x, us, ns, ie)],
    }
}

// ---------- the pass's two static checks ----------

// Mirror of `expr_nospace` (over this node set).
pub open spec fn snospace(x: X) -> bool
    decreases x,
{
    match x {
        X::S(s) => all_nonsep(s, ws()),
        X::Var(_) => false,
        X::NSVar(_) => true,
        X::Int(_) => true,
        // the pass never treats a nested split as nospace (`_ => false`)
        X::Split(_) => false,
        X::Cat(a, b) => snospace(*a) && snospace(*b),
    }
}

// Mirror of `expr_nonempty`. A nospace STRING var is deliberately NOT here:
// the verdict proves no blanks, never non-emptiness.
pub open spec fn snonempty(x: X) -> bool
    decreases x,
{
    match x {
        X::S(s) => s.len() > 0,
        X::Var(_) => false,
        X::NSVar(_) => false,
        X::Int(_) => true,
        X::Split(_) => false,
        X::Cat(a, b) => snonempty(*a) || snonempty(*b),
    }
}

pub open spec fn guard(x: X) -> bool {
    snospace(x) && snonempty(x)
}

// A guarded expression is never a `split` node (the checks return false for
// it), so its word list is the single field of its text.
pub open spec fn is_split(x: X) -> bool {
    match x {
        X::Split(_) => true,
        _ => false,
    }
}

proof fn guard_nonsplit(x: X)
    requires guard(x),
    ensures !is_split(x),
{
    match x {
        X::S(_) => {}
        X::Var(_) => {}
        X::NSVar(_) => {}
        X::Int(_) => {}
        X::Split(_) => {}
        X::Cat(_, _) => {}
    }
}

proof fn fields_of_nonsplit(
    x: X,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
)
    requires !is_split(x),
    ensures fields(x, us, ns, ie) == seq![txt(x, us, ns, ie)],
{
}

// ---------- the fold ----------

// `elem_ok` is the element-position carve-out: a fold is allowed unless the
// value is stored into an array that is not already int-homed.
pub open spec fn fold(x: X, elem_ok: bool) -> X
    decreases x,
{
    match x {
        X::S(_) => x,
        X::Var(_) => x,
        X::NSVar(_) => x,
        X::Int(_) => x,
        X::Cat(a, b) => X::Cat(Box::new(fold(*a, elem_ok)), Box::new(fold(*b, elem_ok))),
        X::Split(e) => {
            let inner = fold(*e, elem_ok);
            if elem_ok && guard(inner) {
                inner
            } else {
                X::Split(Box::new(inner))
            }
        }
    }
}

// Program shape: the custom-IFS gate returns immediately, bodies are walked.
pub open spec fn fold_prog(x: X, elem_ok: bool, custom_ifs: bool) -> X {
    if custom_ifs {
        x
    } else {
        fold(x, elem_ok)
    }
}

pub open spec fn fold_bodies(bs: Seq<X>, elem_ok: bool) -> Seq<X>
    decreases bs.len(),
{
    if bs.len() == 0 {
        Seq::<X>::empty()
    } else {
        seq![fold(bs[0], elem_ok)] + fold_bodies(bs.subrange(1, bs.len() as int), elem_ok)
    }
}

// ---------- the hypotheses, named ----------

// `analyze_var_nospace`'s bit is sound: a tagged var reads whitespace-free
// text. (Verdict supplied by A1 export; not re-derived here.)
pub open spec fn nospace_env_ok(ns: Map<nat, Seq<int>>) -> bool {
    forall|n: nat| all_nonsep(ns[n], ws())
}

// An A2 int verdict is sound: the var reads canonical decimal text, which is
// non-empty and whitespace-free. (Supplied by the A2 type verdict.)
pub open spec fn int_env_ok(ie: Map<nat, Seq<int>>) -> bool {
    forall|n: nat| all_nonsep(ie[n], ws()) && ie[n].len() > 0
}

// The two static checks are conservative with respect to the actual text:
// whitespace-freedom, then non-emptiness (proved separately, each by
// induction, then combined — the two facts are needed in different places).
proof fn snospace_sound(
    x: X,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
)
    requires nospace_env_ok(ns), int_env_ok(ie), snospace(x),
    ensures all_nonsep(txt(x, us, ns, ie), ws()),
    decreases x,
{
    match x {
        X::S(_) => {}
        X::Var(_) => {}
        X::NSVar(n) => {
            assert(all_nonsep(ns[n], ws()));
        }
        X::Int(n) => {
            assert(all_nonsep(ie[n], ws()));
        }
        X::Split(_) => {}
        X::Cat(a, b) => {
            assert(snospace(*a) && snospace(*b));
            snospace_sound(*a, us, ns, ie);
            snospace_sound(*b, us, ns, ie);
            all_nonsep_append(txt(*a, us, ns, ie), txt(*b, us, ns, ie), ws());
            assert(txt(x, us, ns, ie) == txt(*a, us, ns, ie) + txt(*b, us, ns, ie));
        }
    }
}

proof fn snonempty_sound(
    x: X,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
)
    requires nospace_env_ok(ns), int_env_ok(ie), snonempty(x),
    ensures txt(x, us, ns, ie).len() > 0,
    decreases x,
{
    match x {
        X::S(s) => {}
        X::Var(_) => {}
        X::NSVar(_) => {}
        X::Int(n) => {
            assert(ie[n].len() > 0);
        }
        X::Split(_) => {}
        X::Cat(a, b) => {
            assert(snonempty(*a) || snonempty(*b));
            assert(txt(x, us, ns, ie) == txt(*a, us, ns, ie) + txt(*b, us, ns, ie));
            assert(txt(*a, us, ns, ie).len() + txt(*b, us, ns, ie).len()
                == (txt(*a, us, ns, ie) + txt(*b, us, ns, ie)).len());
            if snonempty(*a) {
                snonempty_sound(*a, us, ns, ie);
            } else {
                snonempty_sound(*b, us, ns, ie);
            }
        }
    }
}

// The two static checks are conservative with respect to the actual text.
proof fn verdict_sound(
    x: X,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
)
    requires nospace_env_ok(ns), int_env_ok(ie), guard(x),
    ensures all_nonsep(txt(x, us, ns, ie), ws()) && txt(x, us, ns, ie).len() > 0,
    decreases x,
{
    snospace_sound(x, us, ns, ie);
    snonempty_sound(x, us, ns, ie);
}

// ---------- the law ----------

// THE REWRITE LAW: the fold preserves the value in every position, for every
// expression and every environment that satisfies the verdict hypotheses.
proof fn fold_preserves_fields(
    x: X,
    elem_ok: bool,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
)
    requires nospace_env_ok(ns), int_env_ok(ie),
    ensures fields(fold(x, elem_ok), us, ns, ie) == fields(x, us, ns, ie),
    decreases x,
{
    reveal_with_fuel(fold, 1);
    reveal_with_fuel(fields, 1);
    match x {
        X::S(_) => {
            assert(fields(fold(x, elem_ok), us, ns, ie) == fields(x, us, ns, ie));
        }
        X::Var(_) => {
            assert(fields(fold(x, elem_ok), us, ns, ie) == fields(x, us, ns, ie));
        }
        X::NSVar(_) => {
            assert(fields(fold(x, elem_ok), us, ns, ie) == fields(x, us, ns, ie));
        }
        X::Int(_) => {
            assert(fields(fold(x, elem_ok), us, ns, ie) == fields(x, us, ns, ie));
        }
        X::Split(e) => {
            fold_preserves_fields(*e, elem_ok, us, ns, ie);
            fold_preserves_txt(*e, elem_ok, us, ns, ie);
            let inner = fold(*e, elem_ok);
            reveal_with_fuel(fold, 3);
            reveal_with_fuel(fields, 1);
            assert(txt(inner, us, ns, ie) == txt(*e, us, ns, ie));
            if elem_ok && guard(inner) {
                assert(fold(x, elem_ok) == inner);
                verdict_sound(inner, us, ns, ie);
                words_nonsep(txt(inner, us, ns, ie), ws());
                guard_nonsplit(inner);
                fields_of_nonsplit(inner, us, ns, ie);
                assert(fields(inner, us, ns, ie) == seq![txt(inner, us, ns, ie)]);
                assert(fields(X::Split(e), us, ns, ie) == seq![txt(*e, us, ns, ie)]);
            } else {
                assert(fold(x, elem_ok) == X::Split(Box::new(inner)));
                assert(fields(fold(x, elem_ok), us, ns, ie)
                    == words(txt(inner, us, ns, ie), ws()));
            }
        }
        X::Cat(a, b) => {
            fold_preserves_fields(*a, elem_ok, us, ns, ie);
            fold_preserves_fields(*b, elem_ok, us, ns, ie);
            fold_preserves_txt(*a, elem_ok, us, ns, ie);
            fold_preserves_txt(*b, elem_ok, us, ns, ie);
            assert(fields(fold(x, elem_ok), us, ns, ie) == fields(x, us, ns, ie));
        }
    }
}

// The carried text is preserved too (a container above the fold sees it).
proof fn fold_preserves_txt(
    x: X,
    elem_ok: bool,
    us: Map<nat, Seq<int>>,
    ns: Map<nat, Seq<int>>,
    ie: Map<nat, Seq<int>>,
)
    requires nospace_env_ok(ns), int_env_ok(ie),
    ensures txt(fold(x, elem_ok), us, ns, ie) == txt(x, us, ns, ie),
    decreases x,
{
    match x {
        X::S(_) => {}
        X::Var(_) => {}
        X::NSVar(_) => {}
        X::Int(_) => {}
        X::Split(e) => {
            fold_preserves_txt(*e, elem_ok, us, ns, ie);
        }
        X::Cat(a, b) => {
            fold_preserves_txt(*a, elem_ok, us, ns, ie);
            fold_preserves_txt(*b, elem_ok, us, ns, ie);
        }
    }
}

// ---------- completeness and idempotence ----------

// No foldable split is left anywhere.
pub open spec fn settled(x: X) -> bool
    decreases x,
{
    match x {
        X::Split(e) => !guard(*e) && settled(*e),
        X::Cat(a, b) => settled(*a) && settled(*b),
        _ => true,
    }
}

proof fn fold_leaves_nothing(x: X)
    ensures settled(fold(x, true)),
    decreases x,
{
    match x {
        X::S(_) => {}
        X::Var(_) => {}
        X::NSVar(_) => {}
        X::Int(_) => {}
        X::Split(e) => {
            fold_leaves_nothing(*e);
        }
        X::Cat(a, b) => {
            fold_leaves_nothing(*a);
            fold_leaves_nothing(*b);
        }
    }
}

proof fn fold_is_idempotent(x: X, elem_ok: bool)
    ensures fold(fold(x, elem_ok), elem_ok) == fold(x, elem_ok),
    decreases x,
{
    match x {
        X::S(_) => {}
        X::Var(_) => {}
        X::NSVar(_) => {}
        X::Int(_) => {}
        X::Split(e) => {
            fold_is_idempotent(*e, elem_ok);
        }
        X::Cat(a, b) => {
            fold_is_idempotent(*a, elem_ok);
            fold_is_idempotent(*b, elem_ok);
        }
    }
}

// ---------- the naive alternatives go red ----------

// MUTANT A: the pre-fix rule — whitespace-freedom alone. Folds the split of a
// value that may be EMPTY, which bash reads as zero words.
pub open spec fn fold_no_nonempty(x: X, elem_ok: bool) -> X
    decreases x,
{
    match x {
        X::S(_) => x,
        X::Var(_) => x,
        X::NSVar(_) => x,
        X::Int(_) => x,
        X::Cat(a, b) => X::Cat(
            Box::new(fold_no_nonempty(*a, elem_ok)),
            Box::new(fold_no_nonempty(*b, elem_ok)),
        ),
        X::Split(e) => {
            let inner = fold_no_nonempty(*e, elem_ok);
            if elem_ok && snospace(inner) {
                inner
            } else {
                X::Split(Box::new(inner))
            }
        }
    }
}

// MUTANT B: the emptiness rule alone — no whitespace check.
pub open spec fn fold_no_nospace(x: X, elem_ok: bool) -> X
    decreases x,
{
    match x {
        X::S(_) => x,
        X::Var(_) => x,
        X::NSVar(_) => x,
        X::Int(_) => x,
        X::Cat(a, b) => X::Cat(
            Box::new(fold_no_nospace(*a, elem_ok)),
            Box::new(fold_no_nospace(*b, elem_ok)),
        ),
        X::Split(e) => {
            let inner = fold_no_nospace(*e, elem_ok);
            if elem_ok && snonempty(inner) {
                inner
            } else {
                X::Split(Box::new(inner))
            }
        }
    }
}

// MUTANT C: the identity pass.
pub open spec fn fold_identity(x: X) -> X {
    x
}

// The measured 2026-11-05 bug, reconstructed: a var carrying the nospace
// verdict whose value is EMPTY at runtime. `set -- $z` is zero words; the
// bare word is one empty field; mutating the guard loses that distinction.
proof fn empty_is_zero_words()
    ensures ({
        let us = Map::<nat, Seq<int>>::empty();
        let ns = Map::<nat, Seq<int>>::empty().insert(0, Seq::<int>::empty());
        let ie = Map::<nat, Seq<int>>::empty();
        // bash: the unquoted empty expansion is ZERO words
        fields(X::Split(Box::new(X::NSVar(0))), us, ns, ie) == Seq::<Seq<int>>::empty()
            // the bare word is ONE empty field — a different observation
            && fields(X::NSVar(0), us, ns, ie) == seq![Seq::<int>::empty()]
            // so the pre-fix fold (whitespace-freedom alone) is WRONG here…
            && fields(fold_no_nonempty(X::Split(Box::new(X::NSVar(0))), true), us, ns, ie)
                == seq![Seq::<int>::empty()]
            // …and the real pass keeps the marker (no non-emptiness proof)
            && fold(X::Split(Box::new(X::NSVar(0))), true)
                == X::Split(Box::new(X::NSVar(0)))
            // so it is a no-op, as the law requires
            && fields(fold(X::Split(Box::new(X::NSVar(0))), true), us, ns, ie)
                == Seq::<Seq<int>>::empty()
    }),
{
    let us = Map::<nat, Seq<int>>::empty();
    let ns = Map::<nat, Seq<int>>::empty().insert(0, Seq::<int>::empty());
    let ie = Map::<nat, Seq<int>>::empty();
    reveal_with_fuel(mem, 8);
    reveal_with_fuel(scan, 8);
    reveal_with_fuel(fold, 4);
    reveal_with_fuel(fold_no_nonempty, 4);
}

// A spaced value is TWO words, so the whitespace half is load-bearing too.
proof fn whitespace_is_load_bearing()
    ensures ({
        let us = Map::<nat, Seq<int>>::empty();
        let ns = Map::<nat, Seq<int>>::empty();
        let ie = Map::<nat, Seq<int>>::empty();
        let ab: Seq<int> = seq![97, 32, 98]; // "a b"
        // bash: two fields
        fields(X::Split(Box::new(X::S(ab))), us, ns, ie) == seq![seq![97], seq![98]]
            // the fold-anyway mutant collapses them to one
            && fields(fold_no_nospace(X::Split(Box::new(X::S(ab))), true), us, ns, ie)
                == seq![ab]
            && fields(X::Split(Box::new(X::S(ab))), us, ns, ie) != seq![ab]
            // and the real pass keeps the marker
            && fold(X::Split(Box::new(X::S(ab))), true) == X::Split(Box::new(X::S(ab)))
    }),
{
    let us = Map::<nat, Seq<int>>::empty();
    let ns = Map::<nat, Seq<int>>::empty();
    let ie = Map::<nat, Seq<int>>::empty();
    let ab: Seq<int> = seq![97, 32, 98];
    assert(ab.subrange(1, 3) == seq![32int, 98]);
    assert(ab.subrange(2, 3) == seq![98int]);
    reveal_with_fuel(mem, 8);
    reveal_with_fuel(scan, 12);
    reveal_with_fuel(fold, 4);
    reveal_with_fuel(fold_no_nospace, 4);
}

// An identity pass is not a fold: a foldable split survives it.
proof fn identity_is_not_a_fold()
    ensures !settled(fold_identity(X::Split(Box::new(X::Int(0)))))
        && settled(fold(X::Split(Box::new(X::Int(0))), true)),
{
    reveal_with_fuel(fold, 4);
}

// ---------- pins: the source's tests ----------

// folds_int_typed_var_read, folds_element_split_of_int_array.
proof fn pin_int_var_read_folds()
    ensures fold(X::Split(Box::new(X::Int(0))), true) == X::Int(0),
{
    reveal_with_fuel(fold, 4);
}

// keeps_unknown_var.
proof fn pin_unknown_var_kept()
    ensures fold(X::Split(Box::new(X::Var(0))), true) == X::Split(Box::new(X::Var(0))),
{
    reveal_with_fuel(fold, 4);
}

// keeps_whitespace_literal.
proof fn pin_whitespace_literal_kept()
    ensures fold(X::Split(Box::new(X::S(seq![97, 32, 98]))), true)
        == X::Split(Box::new(X::S(seq![97, 32, 98]))),
{
    reveal_with_fuel(fold, 4);
}

// folds_spaceless_literal.
proof fn pin_spaceless_literal_folds()
    ensures fold(X::Split(Box::new(X::S(seq![52, 50]))), true) == X::S(seq![52, 50]),
{
    reveal_with_fuel(mem, 8);
    reveal_with_fuel(fold, 4);
    reveal_with_fuel(snospace, 2);
    reveal_with_fuel(snonempty, 2);
    assert(!mem(ws(), 52) && !mem(ws(), 50));
    nonsep2(52, 50, ws());
    assert(snospace(X::S(seq![52int, 50int])));
    assert(snonempty(X::S(seq![52int, 50int])));
}

// nospace_is_not_nonempty.
proof fn pin_nospace_var_kept()
    ensures fold(X::Split(Box::new(X::NSVar(0))), true)
        == X::Split(Box::new(X::NSVar(0))),
{
    reveal_with_fuel(mem, 8);
    reveal_with_fuel(fold, 4);
}

// keeps_split_of_empty_literal.
proof fn pin_empty_literal_kept()
    ensures fold(X::Split(Box::new(X::S(Seq::<int>::empty()))), true)
        == X::Split(Box::new(X::S(Seq::<int>::empty()))),
{
    reveal_with_fuel(fold, 4);
}

// folds_split_with_a_nonempty_part: `"a$i"` is never empty.
proof fn pin_nonempty_part_folds()
    ensures fold(
        X::Split(Box::new(X::Cat(
            Box::new(X::S(seq![97])),
            Box::new(X::Int(0)),
        ))),
        true,
    ) == X::Cat(Box::new(X::S(seq![97])), Box::new(X::Int(0))),
{
    reveal_with_fuel(mem, 8);
    reveal_with_fuel(fold, 6);
    reveal_with_fuel(snospace, 2);
    reveal_with_fuel(snonempty, 2);
    assert(!mem(ws(), 97));
    nonsep1(97, ws());
    assert(snospace(X::Cat(Box::new(X::S(seq![97int])), Box::new(X::Int(0)))));
    assert(snonempty(X::Cat(Box::new(X::S(seq![97int])), Box::new(X::Int(0)))));
}

// custom_ifs_blocks_the_fold: with `IFS=:` the whitespace-free `a:b` splits
// into two fields, so the default-IFS gate is required; the pass returns
// immediately when a custom IFS is possible.
proof fn pin_custom_ifs_gate()
    ensures ({
        let e = X::Split(Box::new(X::S(seq![97, 58, 98]))); // "a:b"
        // default IFS: one word — the guard's premise holds
        words(seq![97, 58, 98], ws()) == seq![seq![97, 58, 98]]
            // custom IFS `:`: two words — the fold would be wrong
            && words(seq![97, 58, 98], colon()) == seq![seq![97], seq![98]]
            // so the pass leaves the whole program alone
            && fold_prog(e, true, true) == e
    }),
{
    let abc: Seq<int> = seq![97, 58, 98];
    assert(abc.subrange(1, 3) == seq![58int, 98]);
    assert(abc.subrange(2, 3) == seq![98int]);
    assert(seq![58int, 98].subrange(1, 2) == seq![98int]);
    assert(seq![98int].subrange(1, 1) == Seq::<int>::empty());
    assert(abc[0] == 97 && abc[1] == 58 && abc[2] == 98);
    assert(seq![58int, 98][0] == 58 && seq![58int, 98][1] == 98);
    assert(seq![98int][0] == 98);
    reveal_with_fuel(mem, 8);
    assert(!mem(ws(), 97) && !mem(ws(), 58) && !mem(ws(), 98));
    assert(!mem(colon(), 97) && !mem(colon(), 98));
    nonsep3(97, 58, 98, ws());
    assert(!mem(colon(), 97) && !mem(colon(), 98));
    assert(mem(colon(), 58));
    scan_nonsep(abc, ws(), Seq::<Seq<int>>::empty(), Seq::<int>::empty());
    reveal_with_fuel(scan, 16);
    assert(words(abc, colon()) == seq![seq![97int], seq![98int]]);
    assert(words(abc, ws()) == seq![abc]);
}

// nested_split_and_subs: nested splits fold inside-out, and function bodies
// are walked.
proof fn pin_nested_and_bodies()
    ensures fold(X::Split(Box::new(X::Split(Box::new(X::Int(0))))), true) == X::Int(0)
        && fold_bodies(seq![X::Split(Box::new(X::Int(0)))], true) == seq![X::Int(0)],
{
    reveal_with_fuel(fold, 6);
    reveal_with_fuel(fold_bodies, 4);
}

// keeps_element_split_of_non_int_array: the carve-out skips the fold.
proof fn pin_element_carveout_keeps()
    ensures fold(X::Split(Box::new(X::Int(0))), false)
        == X::Split(Box::new(X::Int(0))),
{
    reveal_with_fuel(fold, 4);
}

// ast_to_ir_folds_trivial_splits_at_the_shared_funnel: a decimal arith word
// folds at the funnel while a possibly-empty word keeps its marker (the
// funnel calls this pass; that wiring is the stated registration hypothesis).
proof fn pin_funnel_shapes()
    ensures fold(X::Split(Box::new(X::Int(7))), true) == X::Int(7)
        && fold(X::Split(Box::new(X::S(Seq::<int>::empty()))), true)
            == X::Split(Box::new(X::S(Seq::<int>::empty())))
        && fold(X::Split(Box::new(X::Var(0))), true)
            == X::Split(Box::new(X::Var(0))),
{
    reveal_with_fuel(fold, 4);
}

fn main() {}
}
